//! Catalog synchronization — populates and updates the catalog without
//! ever starting planning or analysis.

use rusqlite::Connection;

use super::domain::*;
use super::grnoc::EventCatalogSource;
use super::store;

/// Result of one sync run.
pub struct SyncSummary {
    pub events_examined: i64,
    pub new_events: i64,
    pub changed_events: i64,
    pub unchanged_events: i64,
    pub failures: i64,
}

/// Run a catalog sync inside a transaction.
///
/// Snapshots are immutable: a changed ticket creates a NEW snapshot; the
/// latest event view is derived from the latest snapshot.
pub fn sync_catalog(
    conn: &Connection,
    source: &dyn EventCatalogSource,
    started_at: &str,
) -> Result<SyncSummary, String> {
    let mut summary = SyncSummary {
        events_examined: 0,
        new_events: 0,
        changed_events: 0,
        unchanged_events: 0,
        failures: 0,
    };

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot start sync transaction: {e}"))?;

    let items = match source.list_items() {
        Ok(items) => items,
        Err(e) => {
            return Err(format!("sync source failed: {e}"));
        }
    };

    for item in &items {
        summary.events_examined += 1;
        let external_id = super::grnoc::event_external_id(item);
        let event_id = match store::upsert_event(conn, &item.source, &external_id, started_at) {
            Ok(id) => id,
            Err(e) => {
                summary.failures += 1;
                eprintln!("  sync: failed for {external_id}: {e}");
                continue;
            }
        };

        let sha = hex_sha256(&item.raw_payload);
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM event_snapshots WHERE event_id = ?1 AND content_sha256 = ?2",
                rusqlite::params![event_id, sha],
                |r| r.get(0),
            )
            .ok();
        match existing {
            Some(_) => {
                // Same payload: no new snapshot; last_seen was already
                // refreshed by upsert_event.
                summary.unchanged_events += 1;
            }
            None => {
                let snapshot = EventSnapshot {
                    id: 0,
                    event_id,
                    fetched_at: started_at.to_string(),
                    source_url: item.source_url.clone(),
                    content_sha256: sha,
                    raw_payload: item.raw_payload.clone(),
                    normalized_json: item.normalized_json.clone(),
                    parser_version: super::grnoc::GRNOC_PARSER_VERSION.to_string(),
                };
                match store::insert_snapshot(&tx, event_id, &snapshot) {
                    Ok(_) => {
                        // A new snapshot on first sight is "new"; on later
                        // syncs it is a changed ticket.
                        let first: bool = tx
                            .query_row(
                                "SELECT COUNT(*) FROM event_snapshots WHERE event_id = ?1",
                                rusqlite::params![event_id],
                                |r| r.get::<_, i64>(0),
                            )
                            .map(|c| c <= 1)
                            .unwrap_or(true);
                        if first {
                            summary.new_events += 1;
                        } else {
                            summary.changed_events += 1;
                        }
                    }
                    Err(e) => {
                        summary.failures += 1;
                        eprintln!("  sync: snapshot failed for {external_id}: {e}");
                    }
                }
            }
        }
    }

    let sync_run = CatalogSyncRun {
        id: 0,
        source: source.source().to_string(),
        started_at: started_at.to_string(),
        completed_at: Some(started_at.to_string()),
        status: "Complete".to_string(),
        events_examined: summary.events_examined,
        new_events: summary.new_events,
        changed_events: summary.changed_events,
        unchanged_events: summary.unchanged_events,
        failures: summary.failures,
    };
    store::insert_sync_run(&tx, &sync_run)?;

    tx.commit()
        .map_err(|e| format!("cannot commit sync: {e}"))?;
    Ok(summary)
}

/// Hex SHA-256 of a string payload.
pub fn hex_sha256(payload: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(payload.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;
    use crate::catalog::grnoc::GrnocCatalogSource;
    use std::path::Path;

    fn open_temp_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        (dir, conn)
    }

    #[test]
    fn grnoc_sync_creates_catalog_items() {
        let (_dir, conn) = open_temp_db();
        let source = GrnocCatalogSource::new(
            Path::new("tests/fixtures/grnoc").to_path_buf(),
            "2026-07-31T00:00:00Z".into(),
        );
        let summary = sync_catalog(&conn, &source, "2026-07-31T00:00:00Z").unwrap();
        assert_eq!(summary.new_events, 1);
        assert_eq!(summary.failures, 0);
        let events = db::list_events(&conn).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].external_id, "INC0301970");
        let snapshots = db::list_snapshots(&conn, events[0].id).unwrap();
        assert_eq!(snapshots.len(), 1);
        // Sync does NOT create manifests, plans, or runs.
        assert!(db::list_manifest_revisions(&conn, events[0].id)
            .unwrap()
            .is_empty());
        assert!(db::list_runs_for_event(&conn, events[0].id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn grnoc_sync_is_incremental() {
        let (_dir, conn) = open_temp_db();
        let source = GrnocCatalogSource::new(
            Path::new("tests/fixtures/grnoc").to_path_buf(),
            "2026-07-31T00:00:00Z".into(),
        );
        sync_catalog(&conn, &source, "2026-07-31T00:00:00Z").unwrap();
        let s2 = sync_catalog(&conn, &source, "2026-07-31T01:00:00Z").unwrap();
        assert_eq!(s2.new_events, 0);
        assert_eq!(s2.changed_events, 0);
        assert_eq!(s2.unchanged_events, 1);
        let events = db::list_events(&conn).unwrap();
        // last_seen refreshed; still one snapshot.
        assert_eq!(events[0].last_seen, "2026-07-31T01:00:00Z");
        assert_eq!(db::list_snapshots(&conn, events[0].id).unwrap().len(), 1);
    }

    #[test]
    fn ticket_edit_creates_new_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let p = src.join("INC0301970.json");
        std::fs::write(
            &p,
            r#"{"number":"INC0301970","short_description":"Outage - X","start":"2026-07-28T04:35:00Z"}"#,
        )
        .unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        let source = GrnocCatalogSource::new(src.clone(), "2026-07-31T00:00:00Z".into());
        sync_catalog(&conn, &source, "2026-07-31T00:00:00Z").unwrap();
        // Edit the ticket (title changes).
        std::fs::write(
            &p,
            r#"{"number":"INC0301970","short_description":"Outage - X (revised)","start":"2026-07-28T04:35:00Z"}"#,
        )
        .unwrap();
        let s2 = sync_catalog(&conn, &source, "2026-07-31T02:00:00Z").unwrap();
        assert_eq!(s2.changed_events, 1);
        let events = db::list_events(&conn).unwrap();
        let snapshots = db::list_snapshots(&conn, events[0].id).unwrap();
        assert_eq!(snapshots.len(), 2, "old snapshot must not be overwritten");
        // Latest snapshot reflects the edit; old remains readable.
        assert!(snapshots[0].raw_payload.contains("revised"));
        assert!(!snapshots[1].raw_payload.contains("revised"));
    }

    #[test]
    fn open_to_closed_change_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let p = src.join("T.json");
        std::fs::write(
            &p,
            r#"{"number":"T","short_description":"x","start":"2026-07-01T00:00:00Z"}"#,
        )
        .unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        let source = GrnocCatalogSource::new(src.clone(), "2026-07-31T00:00:00Z".into());
        sync_catalog(&conn, &source, "2026-07-31T00:00:00Z").unwrap();
        std::fs::write(
            &p,
            r#"{"number":"T","short_description":"x","start":"2026-07-01T00:00:00Z","end":"2026-07-01T01:00:00Z"}"#,
        )
        .unwrap();
        sync_catalog(&conn, &source, "2026-07-31T03:00:00Z").unwrap();
        let events = db::list_events(&conn).unwrap();
        let snapshots = db::list_snapshots(&conn, events[0].id).unwrap();
        let latest: serde_json::Value =
            serde_json::from_str(&snapshots[0].normalized_json).unwrap();
        assert_eq!(latest["end"], "2026-07-01T01:00:00Z");
        let old: serde_json::Value = serde_json::from_str(&snapshots[1].normalized_json).unwrap();
        assert_eq!(old["end"], serde_json::Value::Null);
    }

    #[test]
    fn sync_does_not_create_reviewed_manifest() {
        let (_dir, conn) = open_temp_db();
        let source = GrnocCatalogSource::new(
            Path::new("tests/fixtures/grnoc").to_path_buf(),
            "2026-07-31T00:00:00Z".into(),
        );
        sync_catalog(&conn, &source, "2026-07-31T00:00:00Z").unwrap();
        let events = db::list_events(&conn).unwrap();
        assert!(db::list_manifest_revisions(&conn, events[0].id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn sync_does_not_start_analysis() {
        let (_dir, conn) = open_temp_db();
        let source = GrnocCatalogSource::new(
            Path::new("tests/fixtures/grnoc").to_path_buf(),
            "2026-07-31T00:00:00Z".into(),
        );
        sync_catalog(&conn, &source, "2026-07-31T00:00:00Z").unwrap();
        assert_eq!(db::list_runs_for_event(&conn, 1).unwrap().len(), 0);
    }

    #[test]
    fn one_bad_ticket_does_not_discard_good_tickets() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("bad.json"), "not json").unwrap();
        std::fs::write(
            src.join("good.json"),
            r#"{"number":"GOOD","short_description":"ok","start":"2026-07-01T00:00:00Z"}"#,
        )
        .unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        let source = GrnocCatalogSource::new(src, "2026-07-31T00:00:00Z".into());
        let summary = sync_catalog(&conn, &source, "2026-07-31T00:00:00Z").unwrap();
        assert_eq!(summary.new_events, 1);
        let events = db::list_events(&conn).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].external_id, "GOOD");
    }
}
