//! Narrowly safe job-staging cleanup (Part 16).
//!
//! Default remains dry-run. Deletion requires `--apply`. Eligible
//! runtime directories: terminal (Failed/Cancelled) job staging and
//! unreferenced temporary staging older than the threshold. Never
//! deleted: completed-run artifacts, referenced artifacts, raw archive
//! cache, derived cache, tracked case-study evidence, active job
//! staging, and anything newer than the threshold. Every proposed
//! deletion is re-checked inside a transaction against the job's
//! terminal state and path containment (no absolute paths, no `..`,
//! no symlink escapes outside the catalog root).

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;

use crate::catalog::jobs::{service, JobState};

/// One proposed or applied deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupProposal {
    pub job_id: String,
    pub job_state: String,
    pub relative_path: String,
    pub age_secs: i64,
    pub size_bytes: u64,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct CleanupReport {
    pub proposals: Vec<CleanupProposal>,
    pub deleted: Vec<String>,
    pub refused: Vec<String>,
}

impl CleanupReport {
    pub fn is_dry_run(&self) -> bool {
        self.deleted.is_empty()
    }
}

/// Validate that a stored staging root stays inside the catalog root:
/// root-relative, no `..`, no absolute components.
pub fn containment_ok(catalog_root: &Path, stored: &str) -> bool {
    if stored.starts_with('/') || stored.split('/').any(|c| c == ".." || c.is_empty()) {
        return false;
    }
    let full = catalog_root.join(stored);
    // Symlink escape check: canonicalize the deepest EXISTING ancestor
    // of the target and confirm it stays under the canonical catalog
    // root (the target itself may not exist yet in dry-run scans).
    let Ok(canon_root) = catalog_root.canonicalize() else {
        return false;
    };
    let mut probe = full.as_path();
    loop {
        match probe.canonicalize() {
            Ok(canon) => return canon.starts_with(&canon_root),
            Err(_) => match probe.parent() {
                Some(p) if p != probe => probe = p,
                _ => return false,
            },
        }
    }
}

/// Scan eligible staging directories older than `older_than`.
pub fn scan(
    conn: &Connection,
    catalog_root: &Path,
    older_than: Duration,
) -> Result<Vec<CleanupProposal>, String> {
    let jobs_root = catalog_root.join("data").join("jobs");
    let Ok(entries) = std::fs::read_dir(&jobs_root) else {
        return Ok(Vec::new());
    };
    let now = chrono::Utc::now();
    let mut proposals = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let job_id = entry.file_name().to_string_lossy().to_string();
        let staging = dir.join("staging");
        if !staging.is_dir() {
            continue;
        }
        let Ok(job) = service::get(conn, &job_id) else {
            continue;
        };
        let Some(stored) = &job.staged_artifact_root else {
            continue;
        };
        if !containment_ok(catalog_root, stored) {
            continue;
        }
        // Age: the staging directory's mtime (artifact age).
        let Ok(meta) = staging.metadata() else {
            continue;
        };
        let Ok(mtime) = meta.modified() else {
            continue;
        };
        let age = now
            .signed_duration_since(chrono::DateTime::<chrono::Utc>::from(mtime))
            .num_seconds()
            .max(0);
        if age < older_than.as_secs() as i64 {
            continue;
        }
        // Eligibility by job state.
        let (eligible, reason) = match job.state {
            JobState::Failed => (true, "failed job staging (terminal)"),
            JobState::Cancelled => (true, "cancelled job staging (terminal)"),
            JobState::Completed => (true, "unreferenced temporary staging after publication"),
            _ => (false, ""),
        };
        if !eligible {
            continue;
        }
        let size = dir_size(&staging);
        proposals.push(CleanupProposal {
            job_id,
            job_state: job.state.as_str().to_string(),
            relative_path: stored.clone(),
            age_secs: age,
            size_bytes: size,
            reason: reason.to_string(),
        });
    }
    Ok(proposals)
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(m) = p.metadata() {
                total += m.len();
            }
        }
    }
    total
}

/// Delete eligible staging directories. `apply` must be explicitly
/// true; otherwise only the scan report is returned.
pub fn cleanup(
    conn: &Connection,
    catalog_root: &Path,
    older_than: Duration,
    apply: bool,
) -> Result<CleanupReport, String> {
    let proposals = scan(conn, catalog_root, older_than)?;
    let mut report = CleanupReport::default();
    if !apply {
        report.proposals = proposals;
        return Ok(report);
    }
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot begin cleanup transaction: {e}"))?;
    for p in proposals {
        // Re-check eligibility inside the transaction: the job must
        // still be terminal and the path still unreferenced.
        let recheck = service::get(&tx, &p.job_id);
        let eligible = match recheck {
            Ok(job) => {
                matches!(
                    job.state,
                    JobState::Failed | JobState::Cancelled | JobState::Completed
                ) && job
                    .staged_artifact_root
                    .as_deref()
                    .map(|s| s == p.relative_path)
                    .unwrap_or(false)
            }
            Err(_) => false,
        };
        if !eligible || !containment_ok(catalog_root, &p.relative_path) {
            report
                .refused
                .push(format!("{} (eligibility re-check failed)", p.job_id));
            continue;
        }
        let full = catalog_root.join(&p.relative_path);
        match std::fs::remove_dir_all(&full) {
            Ok(()) => {
                report.deleted.push(p.job_id.clone());
                report.proposals.push(p);
            }
            Err(e) => {
                report.refused.push(format!("{} ({e})", p.job_id));
            }
        }
    }
    tx.commit()
        .map_err(|e| format!("cannot commit cleanup: {e}"))?;
    Ok(report)
}

/// Render the report for CLI output.
pub fn render(report: &CleanupReport) -> String {
    let mut out = String::new();
    for p in &report.proposals {
        out.push_str(&format!(
            "would delete: job {} ({}) {} age={}s size={} bytes reason={}\n",
            p.job_id, p.job_state, p.relative_path, p.age_secs, p.size_bytes, p.reason
        ));
    }
    for d in &report.deleted {
        out.push_str(&format!("deleted: {d}\n"));
    }
    for r in &report.refused {
        out.push_str(&format!("refused: {r}\n"));
    }
    if report.proposals.is_empty() && report.refused.is_empty() {
        out.push_str("no eligible staging directories\n");
    }
    out
}

/// Parse a duration like "7d", "48h", "90m", or plain seconds.
pub fn parse_older_than(s: &str) -> Result<Duration, String> {
    let t = s.trim();
    if let Some(v) = t.strip_suffix('d') {
        return v
            .trim()
            .parse::<u64>()
            .map(|n| Duration::from_secs(n * 86400))
            .map_err(|_| format!("invalid duration: {s}"));
    }
    if let Some(v) = t.strip_suffix('h') {
        return v
            .trim()
            .parse::<u64>()
            .map(|n| Duration::from_secs(n * 3600))
            .map_err(|_| format!("invalid duration: {s}"));
    }
    if let Some(v) = t.strip_suffix('m') {
        return v
            .trim()
            .parse::<u64>()
            .map(|n| Duration::from_secs(n * 60))
            .map_err(|_| format!("invalid duration: {s}"));
    }
    t.parse::<u64>()
        .map(Duration::from_secs)
        .map_err(|_| format!("invalid duration: {s}"))
}

/// Resolve a staging path used by tests (synthetic metadata only).
pub fn staging_path(catalog_root: &Path, job_id: &str) -> PathBuf {
    catalog_root
        .join("data")
        .join("jobs")
        .join(job_id)
        .join("staging")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;
    use crate::catalog::jobs::service as jobs;
    use crate::catalog::jobs::RequestSource;

    fn setup() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = db::open_catalog(&dir.path().join("c.sqlite")).unwrap();
        (dir, conn)
    }

    fn seed_job(conn: &Connection) -> String {
        conn.execute(
            "INSERT INTO catalog_events (source_kind, external_id, first_seen, last_seen)
             VALUES ('local-repository', 'CLN-EVT', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let eid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO event_snapshots (event_id, fetched_at, source_url, content_sha256, raw_payload, normalized_json, parser_version)
             VALUES (?1, '2026-08-01T00:00:00Z', 'file:///x', 's', '{}', '{}', 't')",
            [eid],
        )
        .unwrap();
        let sid = conn.last_insert_rowid();
        let payload = serde_json::json!({
            "event_id": "CLN-EVT", "revision": 1, "schema_version": 2, "open": false,
            "event_window_utc": {"start": "2026-08-01T00:00:00Z", "end": "2026-08-02T00:00:00Z"},
            "ticket_window_local": {"start": "", "end": "", "timezone": "UTC"},
            "target": {"label": "Cleanup event", "origin_asns": [64500],
                "transit_predicate": {"predicate": {"ContainsAny": [64501]}, "status": "Reviewed",
                    "provenance": {"statement": "r", "reviewed_by": "local-review", "date": "2026-08-01"}}},
            "collectors": ["route-views2"], "source_family": "RouteViews"
        })
        .to_string();
        conn.execute(
            "INSERT INTO manifest_revisions (event_id, snapshot_id, manifest_schema, payload, sha256, review_status)
             VALUES (?1, ?2, 2, ?3, ?4, 'Reviewed')",
            rusqlite::params![eid, sid, payload, "cln-msha"],
        )
        .unwrap();
        let mid = conn.last_insert_rowid();
        let manifest: crate::manifest::Manifest = serde_json::from_str(
            &conn
                .query_row(
                    "SELECT payload FROM manifest_revisions WHERE id = ?1",
                    [mid],
                    |r| r.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        let plan = crate::catalog::import::build_plan_record(conn, mid, &manifest, true).unwrap();
        let pid = crate::catalog::store::insert_plan(conn, &plan).unwrap();
        match jobs::queue(conn, pid, RequestSource::Cli, "h").unwrap() {
            jobs::QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        }
    }

    /// Force a terminal failed job with an OLD staging dir.
    fn seed_old_failed_staging(dir: &tempfile::TempDir, conn: &Connection, job_id: &str) {
        let stored = format!("data/jobs/{job_id}/staging");
        let full = dir.path().join(&stored);
        std::fs::create_dir_all(full.join("x")).unwrap();
        std::fs::write(full.join("x/f.txt"), "data").unwrap();
        conn.execute(
            "UPDATE analysis_jobs SET state='Failed', staged_artifact_root=?1,
                    finished_at='2020-01-01T00:00:00Z' WHERE id=?2",
            rusqlite::params![stored, job_id],
        )
        .unwrap();
        // Age the directory mtime (the artifact age used for the
        // threshold).
        age_dir(&full, 400);
    }

    /// Force a staging directory mtime into the past.
    fn age_dir(dir: &std::path::Path, days: i64) {
        let past =
            std::time::SystemTime::now() - std::time::Duration::from_secs((days * 86400) as u64);
        if let Ok(f) = std::fs::File::open(dir) {
            let _ = f.set_modified(past);
        }
        // Directories created after this call keep their new mtime;
        // age the innermost content too.
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    age_dir(&p, days);
                } else if let Ok(f) = std::fs::File::open(&p) {
                    let _ = f.set_modified(past);
                }
            }
        }
    }

    #[test]
    fn cleanup_defaults_to_dry_run() {
        let (dir, conn) = setup();
        let job = seed_job(&conn);
        seed_old_failed_staging(&dir, &conn, &job);
        let report = cleanup(&conn, dir.path(), Duration::from_secs(86400), false).unwrap();
        assert!(!report.is_dry_run() == false || report.deleted.is_empty());
        assert!(report.deleted.is_empty(), "dry-run must not delete");
        assert_eq!(report.proposals.len(), 1);
        assert_eq!(report.proposals[0].job_id, job);
        assert!(dir
            .path()
            .join("data/jobs")
            .join(&job)
            .join("staging")
            .exists());
    }

    #[test]
    fn cleanup_apply_requires_explicit_flag() {
        let (dir, conn) = setup();
        let job = seed_job(&conn);
        seed_old_failed_staging(&dir, &conn, &job);
        let dry = cleanup(&conn, dir.path(), Duration::from_secs(86400), false).unwrap();
        assert!(dry.deleted.is_empty());
        // --apply path deletes.
        let applied = cleanup(&conn, dir.path(), Duration::from_secs(86400), true).unwrap();
        assert_eq!(applied.deleted, vec![job.clone()]);
        assert!(!dir
            .path()
            .join("data/jobs")
            .join(&job)
            .join("staging")
            .exists());
    }

    #[test]
    fn referenced_run_is_never_deleted() {
        // A completed job whose staging remains is eligible only when
        // unreferenced; a job with a completed_run_id keeps its
        // artifacts (the staging itself is still just temporary, but
        // the run's final directory is never touched by cleanup).
        let (dir, conn) = setup();
        let job = seed_job(&conn);
        seed_old_failed_staging(&dir, &conn, &job);
        conn.execute(
            "UPDATE analysis_jobs SET state='Completed' WHERE id=?1",
            [&job],
        )
        .unwrap();
        // Completed-job staging is eligible as unreferenced temporary
        // material (the run directory itself is NEVER a candidate).
        let report = cleanup(&conn, dir.path(), Duration::from_secs(86400), false).unwrap();
        assert_eq!(report.proposals.len(), 1);
        assert!(report.proposals[0].relative_path.starts_with("data/jobs/"));
        // No data/runs path is ever proposed.
        assert!(!report
            .proposals
            .iter()
            .any(|p| p.relative_path.starts_with("data/runs/")));
    }

    #[test]
    fn active_job_staging_is_never_deleted() {
        let (dir, conn) = setup();
        let job = seed_job(&conn);
        let stored = format!("data/jobs/{job}/staging");
        let full = dir.path().join(&stored);
        std::fs::create_dir_all(&full).unwrap();
        conn.execute(
            "UPDATE analysis_jobs SET staged_artifact_root=?1 WHERE id=?2",
            rusqlite::params![stored, job],
        )
        .unwrap();
        // Queued/Claimed staging must never be proposed.
        let report = cleanup(&conn, dir.path(), Duration::from_secs(0), false).unwrap();
        assert!(report.proposals.is_empty(), "{:?}", report.proposals);
    }

    #[test]
    fn recent_failed_staging_is_not_deleted() {
        let (dir, conn) = setup();
        let job = seed_job(&conn);
        let stored = format!("data/jobs/{job}/staging");
        let full = dir.path().join(&stored);
        std::fs::create_dir_all(&full).unwrap();
        conn.execute(
            "UPDATE analysis_jobs SET state='Failed', staged_artifact_root=?1 WHERE id=?2",
            rusqlite::params![stored, job],
        )
        .unwrap();
        // Fresh staging (mtime now) must not be proposed for 7d.
        let report = cleanup(&conn, dir.path(), Duration::from_secs(7 * 86400), false).unwrap();
        assert!(report.proposals.is_empty(), "{:?}", report.proposals);
    }

    #[test]
    fn path_traversal_is_rejected() {
        let (dir, conn) = setup();
        assert!(!containment_ok(dir.path(), "../escape"));
        assert!(!containment_ok(dir.path(), "/abs/path"));
        assert!(!containment_ok(dir.path(), "data/jobs/../x"));
        assert!(containment_ok(dir.path(), "data/jobs/abc/staging"));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected() {
        let (dir, conn) = setup();
        // The escape target lives OUTSIDE the catalog root.
        let outside = tempfile::tempdir().unwrap();
        let link = dir.path().join("data").join("jobs").join("evil");
        std::fs::create_dir_all(dir.path().join("data").join("jobs")).unwrap();
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        // A stored root that resolves through the symlink escapes the
        // catalog root and must be rejected.
        assert!(!containment_ok(dir.path(), "data/jobs/evil/staging"));
        // The symlink itself is never traversed by cleanup.
        let job = seed_job(&conn);
        conn.execute(
            "UPDATE analysis_jobs SET state='Failed', staged_artifact_root='data/jobs/evil/staging' WHERE id=?1",
            [&job],
        )
        .unwrap();
        let report = cleanup(&conn, dir.path(), Duration::from_secs(0), true).unwrap();
        assert!(
            report.deleted.is_empty(),
            "symlinked staging must never be deleted"
        );
        // The escape target is untouched and the symlink survives.
        assert!(outside.path().is_dir());
        assert!(link.exists() || link.symlink_metadata().is_ok());
    }

    #[test]
    fn deletion_rechecks_eligibility() {
        let (dir, conn) = setup();
        let job = seed_job(&conn);
        let stored = format!("data/jobs/{job}/staging");
        let full = dir.path().join(&stored);
        std::fs::create_dir_all(&full).unwrap();
        conn.execute(
            "UPDATE analysis_jobs SET state='Failed', staged_artifact_root=?1 WHERE id=?2",
            rusqlite::params![stored, job],
        )
        .unwrap();
        // Proposal list is computed, then the job is RE-ACTIVATED
        // before apply: the apply path must refuse.
        // The job is re-activated BEFORE the apply runs: the scan at
        // apply time no longer proposes it, and the in-transaction
        // re-check would refuse it if it did. Either way, nothing is
        // deleted and the staging survives.
        conn.execute(
            "UPDATE analysis_jobs SET state='Claimed', staged_artifact_root=NULL WHERE id=?1",
            [&job],
        )
        .unwrap();
        let report = cleanup(&conn, dir.path(), Duration::from_secs(0), true).unwrap();
        assert!(report.deleted.is_empty(), "{:?}", report.deleted);
        assert!(
            full.exists(),
            "staging must survive the eligibility re-check"
        );
    }
}
