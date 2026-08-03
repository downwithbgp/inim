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

// ── Per-fetch provenance ──────────────────────
//
// One `snapshot_fetches` row is recorded per HTTP fetch attempt. A
// conditional 304 or an unchanged payload creates NO new snapshot (the
// fetch row's `snapshot_id` points at the existing snapshot, or is NULL
// for 304). A changed payload creates a new immutable snapshot. Fetch
// metadata is whitelisted — cookies, authorization, and other sensitive
// headers are never stored.

/// Outcome of one polite fetch-and-sync cycle for a single ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OneFetchResult {
    pub http_status: i64,
    /// A new immutable snapshot was created (content changed).
    pub created_snapshot: bool,
    /// The payload matched an existing snapshot (no new snapshot).
    pub unchanged: bool,
    /// The server answered 304 Not Modified.
    pub not_modified: bool,
    /// The ticket does not exist at the source (404).
    pub not_found: bool,
    pub snapshot_id: Option<i64>,
    pub fetch_record_id: i64,
}

/// Whitelisted fetch metadata recorded per attempt. Deliberately NOT
/// the full header set: sensitive headers are never stored.
#[derive(Debug, Clone)]
pub struct FetchMetadata {
    pub source_url: String,
    pub http_status: i64,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub acquisition_method: String,
    pub retry_count: i64,
    pub conditional_requested: bool,
}

/// Record one fetch attempt and, when the payload is new, one immutable
/// snapshot. `item` is None for 304/404/unchanged outcomes (or when the
/// caller decided not to snapshot, e.g. schema failure); `event_id` is
/// then required so the fetch row still links to the ticket.
///
/// Caller must hold a transaction when several of these run in one sync.
pub fn record_fetch(
    conn: &Connection,
    sync_run_id: i64,
    event_id: Option<i64>,
    item: Option<&CatalogSourceItem>,
    meta: &FetchMetadata,
    started_at: &str,
) -> Result<OneFetchResult, String> {
    let (mut event_id, mut snapshot_id, mut created, mut unchanged, mut not_found) =
        (event_id, None, false, false, false);
    if meta.http_status == 404 {
        not_found = true;
    } else if let Some(item) = item {
        let external_id = super::grnoc::event_external_id(item);
        event_id = Some(store::upsert_event(
            conn,
            &item.source,
            &external_id,
            started_at,
        )?);
        let sha = hex_sha256(&item.raw_payload);
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM event_snapshots WHERE event_id = ?1 AND content_sha256 = ?2",
                rusqlite::params![event_id.unwrap(), sha],
                |r| r.get(0),
            )
            .ok();
        match existing {
            Some(id) => {
                snapshot_id = Some(id);
                unchanged = true;
            }
            None => {
                let snapshot = EventSnapshot {
                    id: 0,
                    event_id: event_id.unwrap(),
                    fetched_at: started_at.to_string(),
                    source_url: item.source_url.clone(),
                    content_sha256: sha,
                    raw_payload: item.raw_payload.clone(),
                    normalized_json: item.normalized_json.clone(),
                    parser_version: super::grnoc::GRNOC_PARSER_VERSION.to_string(),
                };
                let id = store::insert_snapshot(conn, event_id.unwrap(), &snapshot)?;
                snapshot_id = Some(id);
                created = true;
            }
        }
    }
    let fetch_event_id = event_id.ok_or_else(|| {
        "record_fetch requires an event id when no source item is provided".to_string()
    })?;
    let fetch = SnapshotFetch {
        id: 0,
        event_id: fetch_event_id,
        sync_run_id,
        fetched_at: started_at.to_string(),
        source_url: meta.source_url.clone(),
        http_status: meta.http_status,
        content_type: meta.content_type.clone(),
        etag: meta.etag.clone(),
        last_modified: meta.last_modified.clone(),
        acquisition_method: meta.acquisition_method.clone(),
        retry_count: meta.retry_count,
        snapshot_id,
        conditional_requested: meta.conditional_requested,
    };
    let fetch_record_id = store::insert_snapshot_fetch(conn, &fetch)?;
    Ok(OneFetchResult {
        http_status: meta.http_status,
        created_snapshot: created,
        unchanged,
        not_modified: meta.http_status == 304,
        not_found,
        snapshot_id,
        fetch_record_id,
    })
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
        assert_eq!(summary.new_events, 2); // INC0301970 + INC0303298
        assert_eq!(summary.failures, 0);
        let events = db::list_events(&conn).unwrap();
        assert_eq!(events.len(), 2);
        assert!(events.iter().any(|e| e.external_id == "INC0301970"));
        let event = events
            .iter()
            .find(|e| e.external_id == "INC0301970")
            .cloned()
            .unwrap();
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
        assert_eq!(s2.unchanged_events, 2); // INC0301970 + INC0303298
        let events = db::list_events(&conn).unwrap();
        assert_eq!(events.len(), 2);
        let event = events
            .iter()
            .find(|e| e.external_id == "INC0301970")
            .cloned()
            .unwrap();
        // last_seen refreshed; still one snapshot.
        assert_eq!(event.last_seen, "2026-07-31T01:00:00Z");
        assert_eq!(db::list_snapshots(&conn, event.id).unwrap().len(), 1);
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

#[cfg(test)]
mod session33_fetch_provenance_tests {
    use super::*;
    use crate::catalog::db;

    fn open_temp_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        (dir, conn)
    }

    fn make_sync_run(conn: &Connection, started_at: &str) -> i64 {
        let sync_run = CatalogSyncRun {
            id: 0,
            source: "grnoc-public-task-viewer".to_string(),
            started_at: started_at.to_string(),
            completed_at: None,
            status: "Running".to_string(),
            events_examined: 0,
            new_events: 0,
            changed_events: 0,
            unchanged_events: 0,
            failures: 0,
        };
        store::insert_sync_run(conn, &sync_run).unwrap()
    }

    fn fixture_item(number: &str, title: &str) -> CatalogSourceItem {
        // Build a viewer-shaped item programmatically.
        let record = crate::sources::grnoc::ViewerRecord {
            number: number.to_string(),
            short_description: title.to_string(),
            description: "d".to_string(),
            u_outgoing_notification_text: String::new(),
            state: "7".to_string(),
            category: "Circuit".to_string(),
            work_start: "1566362318".to_string(),
            work_end: "1566392400".to_string(),
            opened_at: "1565802831".to_string(),
            priority: "3".to_string(),
            start_date: String::new(),
            end_date: String::new(),
            u_maintenance_type: String::new(),
        };
        let rec = record.to_grnoc_record();
        CatalogSourceItem {
            source: "grnoc-public-task-viewer".to_string(),
            external_id: rec.number.clone(),
            fetched_at: "2026-08-01T00:00:00Z".to_string(),
            source_url: format!(
                "https://ticket-viewer.grnoc.iu.edu/api/get_incidents?number={}",
                rec.number
            ),
            raw_payload: serde_json::to_string(&record).unwrap(),
            normalized_json: serde_json::json!({
                "id": rec.number,
                "title": rec.short_description,
                "task_type": rec.task_type,
                "category": rec.category,
                "start": rec.start,
                "end": rec.end,
                "state": rec.state,
                "priority": rec.priority,
                "description": rec.description,
                "source_url": rec.source_url,
            })
            .to_string(),
        }
    }

    fn meta_for(
        item: Option<&CatalogSourceItem>,
        status: i64,
        etag: Option<&str>,
    ) -> FetchMetadata {
        FetchMetadata {
            source_url: item.map(|i| i.source_url.clone()).unwrap_or_else(|| {
                "https://ticket-viewer.grnoc.iu.edu/api/get_incidents?number=X".to_string()
            }),
            http_status: status,
            content_type: Some("application/json".to_string()),
            etag: etag.map(|s| s.to_string()),
            last_modified: None,
            acquisition_method: "grnoc-viewer-api".to_string(),
            retry_count: 0,
            conditional_requested: etag.is_some(),
        }
    }

    #[test]
    fn conditional_not_modified_preserves_existing_snapshot() {
        let (_dir, conn) = open_temp_db();
        let tx = conn.unchecked_transaction().unwrap();
        let sync_run = make_sync_run(&tx, "2026-08-01T00:00:00Z");
        let item = fixture_item("INC0040257", "Outage - X");
        let first = record_fetch(
            &tx,
            sync_run,
            None,
            Some(&item),
            &meta_for(Some(&item), 200, Some("\"v1\"")),
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        assert!(first.created_snapshot);
        let snap_before = db::list_snapshots(&tx, first.snapshot_id.unwrap())
            .unwrap()
            .len();

        // 304 with the same validator: no new snapshot, fetch row only.
        let events = db::list_events(&tx).unwrap();
        let event_id = events[0].id;
        let second = record_fetch(
            &tx,
            sync_run,
            Some(event_id),
            None,
            &meta_for(None, 304, Some("\"v1\"")),
            "2026-08-01T01:00:00Z",
        )
        .unwrap();
        assert!(second.not_modified);
        assert!(!second.created_snapshot);
        assert!(second.snapshot_id.is_none());
        let snapshots = db::list_snapshots(&tx, event_id).unwrap();
        assert_eq!(snapshots.len(), 1, "304 must not create a snapshot");
        let fetches = store::list_snapshot_fetches(&tx, event_id).unwrap();
        assert_eq!(fetches.len(), 2);
        assert_eq!(fetches[0].http_status, 304);
        assert_eq!(fetches[0].snapshot_id, None);
        assert_eq!(fetches[1].http_status, 200);
        assert_eq!(fetches[1].snapshot_id, Some(snapshots[0].id));
        let _ = snap_before;
        tx.commit().unwrap();
    }

    #[test]
    fn changed_etag_and_payload_create_new_snapshot() {
        let (_dir, conn) = open_temp_db();
        let tx = conn.unchecked_transaction().unwrap();
        let sync_run = make_sync_run(&tx, "2026-08-01T00:00:00Z");
        let v1 = fixture_item("INC0040257", "Outage - X");
        let first = record_fetch(
            &tx,
            sync_run,
            None,
            Some(&v1),
            &meta_for(Some(&v1), 200, Some("\"v1\"")),
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        assert!(first.created_snapshot);
        // The source publishes a changed payload with a new ETag.
        let v2 = fixture_item("INC0040257", "Outage - X (updated)");
        let second = record_fetch(
            &tx,
            sync_run,
            None,
            Some(&v2),
            &meta_for(Some(&v2), 200, Some("\"v2\"")),
            "2026-08-01T02:00:00Z",
        )
        .unwrap();
        assert!(second.created_snapshot);
        assert_ne!(second.snapshot_id, first.snapshot_id);
        let events = db::list_events(&tx).unwrap();
        let snapshots = db::list_snapshots(&tx, events[0].id).unwrap();
        assert_eq!(
            snapshots.len(),
            2,
            "changed content creates a new immutable snapshot"
        );
        // The newest snapshot carries the changed title; the old remains.
        let latest: serde_json::Value =
            serde_json::from_str(&snapshots[0].normalized_json).unwrap();
        assert_eq!(latest["title"], "Outage - X (updated)");
        tx.commit().unwrap();
    }

    #[test]
    fn fetch_metadata_does_not_include_sensitive_headers() {
        let (_dir, conn) = open_temp_db();
        let tx = conn.unchecked_transaction().unwrap();
        let sync_run = make_sync_run(&tx, "2026-08-01T00:00:00Z");
        let item = fixture_item("INC0040257", "Outage - X");
        let meta = FetchMetadata {
            source_url: item.source_url.clone(),
            http_status: 200,
            // Even if the server sent cookies, the whitelist never stores
            // them — the struct has no field for them.
            content_type: Some("application/json; charset=utf-8".to_string()),
            etag: Some("\"v1\"".to_string()),
            last_modified: None,
            acquisition_method: "grnoc-viewer-api".to_string(),
            retry_count: 2,
            conditional_requested: true,
        };
        record_fetch(
            &tx,
            sync_run,
            None,
            Some(&item),
            &meta,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        let events = db::list_events(&tx).unwrap();
        let fetches = store::list_snapshot_fetches(&tx, events[0].id).unwrap();
        assert_eq!(fetches.len(), 1);
        let f = &fetches[0];
        assert_eq!(
            f.content_type.as_deref(),
            Some("application/json; charset=utf-8")
        );
        assert_eq!(f.etag.as_deref(), Some("\"v1\""));
        assert_eq!(f.retry_count, 2);
        assert!(f.conditional_requested);
        // No sensitive header keys exist in the record at all.
        let json = serde_json::to_value(f).unwrap();
        let obj = json.as_object().unwrap();
        assert!(!obj.keys().any(|k| {
            let lk = k.to_ascii_lowercase();
            lk.contains("cookie") || lk.contains("authorization") || lk.contains("set-cookie")
        }));
        tx.commit().unwrap();
    }

    #[test]
    fn source_payload_hash_is_reproducible() {
        let (_dir, conn) = open_temp_db();
        let tx = conn.unchecked_transaction().unwrap();
        let sync_run = make_sync_run(&tx, "2026-08-01T00:00:00Z");
        let item = fixture_item("INC0040257", "Outage - X");
        record_fetch(
            &tx,
            sync_run,
            None,
            Some(&item),
            &meta_for(Some(&item), 200, None),
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        let events = db::list_events(&tx).unwrap();
        let snapshots = db::list_snapshots(&tx, events[0].id).unwrap();
        assert_eq!(snapshots.len(), 1);
        // Recomputing SHA-256 over the stored raw payload reproduces the
        // recorded content hash exactly.
        let recomputed = hex_sha256(&snapshots[0].raw_payload);
        assert_eq!(recomputed, snapshots[0].content_sha256);
        assert_eq!(recomputed.len(), 64);
        tx.commit().unwrap();
    }

    #[test]
    fn old_snapshot_remains_linked_to_historical_run() {
        let (_dir, conn) = open_temp_db();
        let tx = conn.unchecked_transaction().unwrap();
        let sync_run = make_sync_run(&tx, "2026-08-01T00:00:00Z");
        let v1 = fixture_item("INC0040257", "Outage - X");
        let first = record_fetch(
            &tx,
            sync_run,
            None,
            Some(&v1),
            &meta_for(Some(&v1), 200, Some("\"v1\"")),
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        // A reviewed manifest is created against snapshot v1 (this is the
        // input to a historical analysis run).
        let rev = ManifestRevision {
            id: 0,
            event_id: 0, // replaced below
            snapshot_id: first.snapshot_id.unwrap(),
            manifest_schema: 2,
            payload: "{}".to_string(),
            sha256: "manifest-sha-1".to_string(),
            review_status: "Reviewed".to_string(),
            reviewed_at: Some("2026-08-01T00:30:00Z".to_string()),
            reviewer: Some("analyst".to_string()),
        };
        let events = db::list_events(&tx).unwrap();
        let event_id = events[0].id;
        let rev = ManifestRevision { event_id, ..rev };
        store::insert_manifest_revision(&tx, &rev).unwrap();

        // The source changes; a second snapshot appears.
        let v2 = fixture_item("INC0040257", "Outage - X (updated)");
        record_fetch(
            &tx,
            sync_run,
            None,
            Some(&v2),
            &meta_for(Some(&v2), 200, Some("\"v2\"")),
            "2026-08-01T02:00:00Z",
        )
        .unwrap();

        // The historical manifest (and therefore any run derived from it)
        // still references snapshot v1 — never the newer snapshot.
        let manifests = db::list_manifest_revisions(&tx, event_id).unwrap();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].snapshot_id, first.snapshot_id.unwrap());
        let snapshots = db::list_snapshots(&tx, event_id).unwrap();
        assert_eq!(snapshots.len(), 2);
        assert!(snapshots.iter().any(|s| s.id == first.snapshot_id.unwrap()));
        tx.commit().unwrap();
    }
}
