//! Polite live adapter for the GRNOC Public Task Viewer (Session 33,
//! Part 5).
//!
//! The viewer exposes undocumented POST JSON endpoints (see
//! `docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md`):
//!
//! - `POST /api/get_incidents` — `INC...` records
//! - `POST /api/get_change_requests` — `CHG...` records
//!
//! Exact ticket-number lookups use `{"number": "..."}`. `TASK...`
//! records are NOT served by either endpoint (verified in the audit) and
//! are marked Unresolved without wasting a request. Search is never
//! issued by this adapter without an explicit reviewed domain + query
//! (the unscoped incident search returns 403).

use crate::catalog::access::{AccessPolicy, ClientError, FetchOutcome, PoliteClient, StopReason};
use crate::catalog::domain::*;
use crate::catalog::store;
use crate::catalog::sync::FetchMetadata;

/// Default viewer origin.
pub const GRNOC_VIEWER_BASE: &str = "https://ticket-viewer.grnoc.iu.edu";
/// Acquisition method recorded on fetch rows produced by this adapter.
pub const ACQUISITION_VIEWER_API: &str = "grnoc-viewer-api";

/// How one ticket lookup ended.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // Found carries the full source item
pub enum ViewerTicketFetch {
    /// The viewer returned the ticket; `item` is the normalized item.
    Found {
        item: CatalogSourceItem,
        status: i64,
        content_type: Option<String>,
        etag: Option<String>,
        last_modified: Option<String>,
        retries: i64,
        conditional_requested: bool,
    },
    /// The viewer answered but has no record for this number.
    NotFound {
        status: i64,
        retries: i64,
        conditional_requested: bool,
    },
    /// TASK-prefixed numbers are not served by the viewer (audited);
    /// no request is made.
    Unsupported,
}

/// Adapter error: terminal stop or transport failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrnocViewerError {
    Stop(StopReason),
    Transport(String),
    UnexpectedStatus(u16),
    Parse(String),
}

impl std::fmt::Display for GrnocViewerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrnocViewerError::Stop(s) => write!(f, "sync stopped: {s:?}"),
            GrnocViewerError::Transport(d) => write!(f, "transport error: {d}"),
            GrnocViewerError::UnexpectedStatus(s) => write!(f, "unexpected HTTP status {s}"),
            GrnocViewerError::Parse(d) => write!(f, "response parse failure: {d}"),
        }
    }
}

/// Polite GRNOC Public Task Viewer client.
pub struct GrnocViewerClient {
    client: PoliteClient,
    base_url: String,
}

impl GrnocViewerClient {
    /// A client against the production viewer with the given policy.
    pub fn new(policy: AccessPolicy) -> Result<Self, String> {
        GrnocViewerClient::new_with_base(policy, GRNOC_VIEWER_BASE.to_string())
    }

    /// A client against an explicit base URL (tests use the mock server).
    pub fn new_with_base(policy: AccessPolicy, base_url: String) -> Result<Self, String> {
        let client = PoliteClient::new(policy)?;
        Ok(GrnocViewerClient { client, base_url })
    }

    pub fn budget_remaining(&self) -> usize {
        self.client.budget_remaining()
    }

    pub fn requests_made(&self) -> u64 {
        self.client.requests_made()
    }

    /// Response bytes transferred so far (pilot accounting only).
    pub fn bytes_transferred(&self) -> u64 {
        self.client.bytes_transferred()
    }

    /// Endpoint for a ticket number; `None` for unsupported families.
    fn endpoint_for(number: &str) -> Option<&'static str> {
        let lower = number.to_ascii_lowercase();
        if lower.starts_with("inc") {
            Some("/api/get_incidents")
        } else if lower.starts_with("chg") {
            Some("/api/get_change_requests")
        } else {
            None
        }
    }

    /// Look up one ticket number. Exact lookup only — never a broad
    /// search, never an enumeration.
    pub fn fetch_ticket(&mut self, number: &str) -> Result<ViewerTicketFetch, GrnocViewerError> {
        let Some(endpoint) = Self::endpoint_for(number) else {
            return Ok(ViewerTicketFetch::Unsupported);
        };
        let url = format!("{}{}", self.base_url, endpoint);
        let body = serde_json::json!({ "number": number }).to_string();
        let outcome = self
            .client
            .fetch_post(&url, &body, None, None)
            .map_err(|e| match e {
                ClientError::Stop(s) => GrnocViewerError::Stop(s),
                ClientError::TooManyRetries(d) => GrnocViewerError::Transport(d),
                ClientError::UnexpectedStatus(s) => GrnocViewerError::UnexpectedStatus(s),
                ClientError::Transport(d) => GrnocViewerError::Transport(d),
            })?;
        let retries = self.client.last_retries() as i64;
        match outcome {
            FetchOutcome::Ok(body) => {
                let response = crate::sources::grnoc::ViewerResponse::parse_json(&body.body)
                    .map_err(GrnocViewerError::Parse)?;
                let Some(record) = response.result.into_iter().next() else {
                    return Ok(ViewerTicketFetch::NotFound {
                        status: body.status as i64,
                        retries,
                        conditional_requested: false,
                    });
                };
                let grnoc = record.to_grnoc_record();
                let item = CatalogSourceItem {
                    source: "grnoc-public-task-viewer".to_string(),
                    external_id: grnoc.number.clone(),
                    fetched_at: chrono::Utc::now().to_rfc3339(),
                    source_url: format!("{}/tickets/{}/", self.base_url, grnoc.number),
                    raw_payload: serde_json::to_string(&record).unwrap_or_default(),
                    normalized_json: serde_json::json!({
                        "id": grnoc.number,
                        "title": grnoc.short_description,
                        "task_type": grnoc.task_type,
                        "category": grnoc.category,
                        "start": grnoc.start,
                        "end": grnoc.end,
                        "opened": grnoc.opened,
                        "state": grnoc.state,
                        "state_code": grnoc.state_code,
                        "priority": grnoc.priority,
                        "priority_code": grnoc.priority_code,
                        "planned_start": grnoc.planned_start,
                        "planned_end": grnoc.planned_end,
                        "maintenance_type": grnoc.maintenance_type,
                        "description": grnoc.description,
                        "notification_text": grnoc.notification_text,
                        "source_url": grnoc.source_url,
                    })
                    .to_string(),
                };
                Ok(ViewerTicketFetch::Found {
                    item,
                    status: body.status as i64,
                    content_type: body.content_type,
                    etag: body.etag,
                    last_modified: body.last_modified,
                    retries,
                    conditional_requested: false,
                })
            }
            FetchOutcome::NotModified => Ok(ViewerTicketFetch::NotFound {
                status: 304,
                retries,
                conditional_requested: true,
            }),
            FetchOutcome::NotFound => Ok(ViewerTicketFetch::NotFound {
                status: 404,
                retries,
                conditional_requested: false,
            }),
            FetchOutcome::Forbidden => Err(GrnocViewerError::Stop(StopReason::Forbidden)),
            FetchOutcome::Unauthorized => {
                Err(GrnocViewerError::Stop(StopReason::AuthenticationRequired))
            }
        }
    }

    /// Convert a ticket fetch into sync-layer fetch metadata.
    fn metadata_for(
        fetch: &ViewerTicketFetch,
        source_url: &str,
        conditional: bool,
    ) -> Option<FetchMetadata> {
        match fetch {
            ViewerTicketFetch::Found {
                status,
                content_type,
                etag,
                last_modified,
                retries,
                ..
            } => Some(FetchMetadata {
                source_url: source_url.to_string(),
                http_status: *status,
                content_type: content_type.clone(),
                etag: etag.clone(),
                last_modified: last_modified.clone(),
                acquisition_method: ACQUISITION_VIEWER_API.to_string(),
                retry_count: *retries,
                conditional_requested: conditional,
            }),
            ViewerTicketFetch::NotFound {
                status,
                retries,
                conditional_requested,
            } => Some(FetchMetadata {
                source_url: source_url.to_string(),
                http_status: *status,
                content_type: None,
                etag: None,
                last_modified: None,
                acquisition_method: ACQUISITION_VIEWER_API.to_string(),
                retry_count: *retries,
                conditional_requested: *conditional_requested,
            }),
            ViewerTicketFetch::Unsupported => None,
        }
    }
}

/// Counts from one corpus sync run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CorpusSyncSummary {
    pub examined: usize,
    pub new_snapshots: usize,
    pub unchanged: usize,
    pub not_modified: usize,
    pub not_found: usize,
    pub unsupported: usize,
    pub failures: usize,
    pub stopped: Option<StopReason>,
    pub requests_made: u64,
}

/// Drive one polite sync over a discovery frontier.
///
/// Each ID is fetched at most once; TASK numbers are marked Unresolved
/// without a request; the per-sync budget and stop conditions are
/// enforced by the polite client. Discovery rows are advanced to
/// Fetched (found) or Unresolved (not found / unsupported). A stop
/// reason terminates the run cleanly; remaining IDs stay Pending and are
/// resumed by a later run.
pub fn sync_frontier(
    conn: &rusqlite::Connection,
    client: &mut GrnocViewerClient,
    source_kind: &str,
    ids: &[String],
    started_at: &str,
) -> Result<CorpusSyncSummary, String> {
    let mut summary = CorpusSyncSummary::default();
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot start corpus sync transaction: {e}"))?;

    let sync_run = CatalogSyncRun {
        id: 0,
        source: source_kind.to_string(),
        started_at: started_at.to_string(),
        completed_at: None,
        status: "Running".to_string(),
        events_examined: 0,
        new_events: 0,
        changed_events: 0,
        unchanged_events: 0,
        failures: 0,
    };
    let sync_run_id = store::insert_sync_run(&tx, &sync_run)?;

    for external_id in ids {
        if client.budget_remaining() == 0 {
            summary.stopped = Some(StopReason::BudgetExhausted);
            break;
        }
        summary.examined += 1;
        match client.fetch_ticket(external_id) {
            Ok(fetch) => {
                let conditional = matches!(fetch, ViewerTicketFetch::Found { ref etag, ref last_modified, .. } if etag.is_some() || last_modified.is_some());
                match &fetch {
                    ViewerTicketFetch::Found { item, .. } => {
                        let meta = GrnocViewerClient::metadata_for(
                            &fetch,
                            &ticket_url(external_id),
                            conditional,
                        )
                        .expect("found fetch always has metadata");
                        let result = super::sync::record_fetch(
                            &tx,
                            sync_run_id,
                            None,
                            Some(item),
                            &meta,
                            started_at,
                        )?;
                        if result.created_snapshot {
                            summary.new_snapshots += 1;
                        } else if result.unchanged {
                            summary.unchanged += 1;
                        } else if result.not_modified {
                            summary.not_modified += 1;
                        }
                        store::mark_frontier_fetched(&tx, source_kind, external_id)?;
                    }
                    ViewerTicketFetch::NotFound { .. } => {
                        // No catalog event exists for a ticket the source
                        // does not serve; the discovery status records the
                        // outcome (no fetch row to attach).
                        summary.not_found += 1;
                        store::update_discovery_status_rows(
                            &tx,
                            source_kind,
                            external_id,
                            DISCOVERY_STATUS_UNRESOLVED,
                        )?;
                    }
                    ViewerTicketFetch::Unsupported => {
                        summary.unsupported += 1;
                        store::update_discovery_status_rows(
                            &tx,
                            source_kind,
                            external_id,
                            DISCOVERY_STATUS_UNRESOLVED,
                        )?;
                    }
                }
            }
            Err(GrnocViewerError::Stop(reason)) => {
                summary.stopped = Some(reason);
                break;
            }
            Err(e) => {
                summary.failures += 1;
                eprintln!("  corpus sync: {external_id}: {e}");
            }
        }
    }

    summary.requests_made = client.requests_made();
    let status = match summary.stopped {
        Some(_) => "Stopped",
        None => "Complete",
    };
    tx.execute(
        "UPDATE catalog_sync_runs SET completed_at = ?1, status = ?2, events_examined = ?3,
                new_events = ?4, changed_events = 0, unchanged_events = ?5, failures = ?6
         WHERE id = ?7",
        rusqlite::params![
            started_at,
            status,
            summary.examined as i64,
            summary.new_snapshots as i64,
            summary.unchanged as i64,
            summary.failures as i64,
            sync_run_id
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;

    tx.commit()
        .map_err(|e| format!("cannot commit corpus sync: {e}"))?;
    Ok(summary)
}

/// Canonical ticket URL for provenance records.
fn ticket_url(number: &str) -> String {
    format!("{GRNOC_VIEWER_BASE}/tickets/{number}/")
}

/// Link every case-study event link whose external identifier now exists
/// in the catalog to its catalog event. Linkage is by identifier only —
/// titles are never matched. Returns the number of links resolved.
pub fn link_case_study_tickets(
    conn: &rusqlite::Connection,
    case_study_id: i64,
    source_kind: &str,
) -> Result<usize, String> {
    let mut stmt = conn
        .prepare(
            "SELECT l.id, l.external_identifier FROM case_study_event_links l
             WHERE l.case_study_id = ?1 AND l.catalog_event_id IS NULL",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([case_study_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut resolved = 0usize;
    for row in rows {
        let (link_id, external_id) = row.map_err(|e| format!("catalog read failed: {e}"))?;
        if let Some(event) = super::db::get_event_by_external(conn, source_kind, &external_id)? {
            conn.execute(
                "UPDATE case_study_event_links SET catalog_event_id = ?1 WHERE id = ?2",
                rusqlite::params![event.id, link_id],
            )
            .map_err(|e| format!("catalog write failed: {e}"))?;
            resolved += 1;
        }
    }
    Ok(resolved)
}

/// The verified source-vs-AAR timing comparison for a case study: for
/// each linked ticket, the ticket's source start/end and the case
/// study's AAR start/end. Both values are preserved; nothing is
/// reconciled.
pub fn source_vs_aar_timing(
    conn: &rusqlite::Connection,
    case_study_id: i64,
) -> Result<Vec<TimingComparison>, String> {
    let (cs_start, cs_end): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT start_utc, end_utc FROM case_studies WHERE id = ?1",
            [case_study_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let (cs_start, cs_end) = (cs_start.unwrap_or_default(), cs_end.unwrap_or_default());
    let mut stmt = conn
        .prepare(
            "SELECT l.external_identifier, l.catalog_event_id
             FROM case_study_event_links l WHERE l.case_study_id = ?1
             ORDER BY l.sort_order, l.id",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([case_study_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?))
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        let (external_id, event_id) = row.map_err(|e| format!("catalog read failed: {e}"))?;
        let (source_start, source_end) = match event_id {
            Some(eid) => {
                let snapshots = super::db::list_snapshots(conn, eid)?;
                match snapshots.first() {
                    Some(s) => {
                        let v: serde_json::Value =
                            serde_json::from_str(&s.normalized_json).unwrap_or_default();
                        (
                            v.get("start")
                                .and_then(|x| x.as_str())
                                .unwrap_or_default()
                                .to_string(),
                            v.get("end")
                                .and_then(|x| x.as_str())
                                .unwrap_or_default()
                                .to_string(),
                        )
                    }
                    None => (String::new(), String::new()),
                }
            }
            None => (String::new(), String::new()),
        };
        out.push(TimingComparison {
            external_id,
            source_start,
            source_end,
            case_study_start: cs_start.clone(),
            case_study_end: cs_end.clone(),
        });
    }
    Ok(out)
}

/// One row of the source-vs-AAR timing comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimingComparison {
    pub external_id: String,
    pub source_start: String,
    pub source_end: String,
    pub case_study_start: String,
    pub case_study_end: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;
    use crate::catalog::mock_server::{MockResponse, MockServer};

    fn fast_policy() -> AccessPolicy {
        AccessPolicy {
            requests_per_second: 1000.0,
            backoff_base_ms: 1,
            backoff_max_ms: 5,
            max_retries: 1,
            ..AccessPolicy::default()
        }
    }

    fn open_temp_db() -> (tempfile::TempDir, rusqlite::Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        (dir, conn)
    }

    fn incident_response(
        number: &str,
        title: &str,
        work_start: &str,
        work_end: &str,
    ) -> MockResponse {
        MockResponse::new(
            200,
            &serde_json::json!({
                "total": 1,
                "result": [{
                    "number": number,
                    "short_description": title,
                    "description": "",
                    "u_outgoing_notification_text": "",
                    "state": "7",
                    "category": "Circuit",
                    "work_start": work_start,
                    "work_end": work_end,
                    "opened_at": "1565802831",
                    "priority": "3"
                }]
            })
            .to_string(),
        )
    }

    fn change_response(
        number: &str,
        title: &str,
        work_start: &str,
        work_end: &str,
    ) -> MockResponse {
        MockResponse::new(
            200,
            &serde_json::json!({
                "total": 1,
                "result": [{
                    "number": number,
                    "short_description": title,
                    "description": "",
                    "u_outgoing_notification_text": "",
                    "start_date": "1566360000",
                    "end_date": "1566392400",
                    "state": "3",
                    "u_maintenance_type": "Hardware",
                    "priority": "3",
                    "work_start": work_start,
                    "work_end": work_end,
                    "opened_at": "1565802831"
                }]
            })
            .to_string(),
        )
    }

    fn empty_response() -> MockResponse {
        MockResponse::new(200, r#"{"total":0,"result":[]}"#)
    }

    // The 12 AAR-listed MAN LAN ticket ids.
    fn manlan_seed_ids() -> Vec<String> {
        [
            "CHG0038258",
            "INC0040257",
            "CHG0038386",
            "INC0040258",
            "INC0040272",
            "TASK0038206",
            "INC0040289",
            "INC0040290",
            "INC0040291",
            "INC0040293",
            "TASK0038211",
            "INC0040318",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn manlan_case_study(conn: &rusqlite::Connection, ids: &[String]) -> i64 {
        // Minimal case study + links mirroring the AAR (import would do
        // this; here we insert directly for test isolation).
        let cs = CaseStudy {
            id: 0,
            slug: "manlan-test".to_string(),
            title: "MAN LAN".to_string(),
            summary: "s".to_string(),
            start_utc: Some("2019-08-21T04:00:00Z".to_string()),
            end_utc: Some("2019-08-21T22:38:00Z".to_string()),
            status: "Active".to_string(),
            content_sha256: "abc".to_string(),
            created_utc: "2026-08-01T00:00:00Z".to_string(),
            updated_utc: "2026-08-01T00:00:00Z".to_string(),
        };
        let cs_id = store::insert_case_study(conn, &cs).unwrap();
        for (i, id) in ids.iter().enumerate() {
            store::insert_case_study_event_link(
                conn,
                &CaseStudyEventLink {
                    id: 0,
                    case_study_id: cs_id,
                    catalog_event_id: None,
                    external_identifier: id.clone(),
                    relationship: "Related".to_string(),
                    reviewed_note: Some("AAR-listed".to_string()),
                    sort_order: i as i64,
                    source_document_id: None,
                },
            )
            .unwrap();
        }
        cs_id
    }

    #[test]
    fn all_manlan_seed_ids_are_requested_once() {
        let ids = manlan_seed_ids();
        // Scripted responses: INC/CHG lookups return the ticket; TASK
        // numbers are never requested by the adapter.
        let mut responses = Vec::new();
        for id in &ids {
            if id.starts_with("TASK") {
                continue;
            }
            if id.starts_with("CHG") {
                responses.push(change_response(
                    id,
                    "Maintenance - X",
                    "1566362318",
                    "1566392400",
                ));
            } else {
                responses.push(incident_response(
                    id,
                    "Outage - X",
                    "1566362318",
                    "1566392400",
                ));
            }
        }
        let server = MockServer::start(responses);
        let (_dir, conn) = open_temp_db();
        // Record seeds so the frontier matches the AAR list.
        for id in &ids {
            crate::catalog::discovery::record_analyst_seed(
                &conn,
                "grnoc-public-task-viewer",
                id,
                "2026-08-01T00:00:00Z",
            )
            .unwrap();
        }
        let frontier = store::pending_frontier(&conn, "grnoc-public-task-viewer").unwrap();
        let mut client = GrnocViewerClient::new_with_base(fast_policy(), server.url("")).unwrap();
        let summary = sync_frontier(
            &conn,
            &mut client,
            "grnoc-public-task-viewer",
            &frontier,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        assert_eq!(summary.stopped, None);
        assert_eq!(
            summary.unsupported, 2,
            "TASK numbers are marked unsupported"
        );
        assert_eq!(summary.examined, 12);
        assert_eq!(summary.new_snapshots, 10);
        // Every INC/CHG seed was requested exactly once; nothing else.
        let requests = server.requests();
        assert_eq!(requests.len(), 10);
        for id in &ids {
            if id.starts_with("TASK") {
                continue;
            }
            let hits = requests
                .iter()
                .filter(|r| {
                    let body = r.json();
                    body.get("number").and_then(|v| v.as_str()) == Some(id.as_str())
                })
                .count();
            assert_eq!(hits, 1, "{id} requested once");
            let endpoint = if id.starts_with("CHG") {
                "/api/get_change_requests"
            } else {
                "/api/get_incidents"
            };
            assert!(
                requests.iter().any(|r| r.path == endpoint),
                "{endpoint} used"
            );
        }
        // Discovery rows advanced correctly.
        let fetched = store::list_discoveries(
            &conn,
            "grnoc-public-task-viewer",
            Some(DISCOVERY_STATUS_FETCHED),
        )
        .unwrap();
        assert_eq!(fetched.len(), 10);
        let unresolved = store::list_discoveries(
            &conn,
            "grnoc-public-task-viewer",
            Some(DISCOVERY_STATUS_UNRESOLVED),
        )
        .unwrap();
        assert_eq!(unresolved.len(), 2);
        assert!(unresolved.iter().all(|d| d.external_id.starts_with("TASK")));
    }

    #[test]
    fn retrieved_ticket_links_existing_document_reference() {
        let (_dir, conn) = open_temp_db();
        let ids = manlan_seed_ids();
        let cs_id = manlan_case_study(&conn, &ids);
        // Record the case-study references as the discovery provenance.
        crate::catalog::discovery::record_case_study_references(
            &conn,
            "grnoc-public-task-viewer",
            cs_id,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        let server = MockServer::start(vec![
            change_response(
                "CHG0038258",
                "Maintenance 1 of 2 Completed - MAN LAN Core Node (sw.net.manlan)",
                "1566362318",
                "1566392400",
            ),
            empty_response(),
            empty_response(),
            empty_response(),
            empty_response(),
            empty_response(),
            empty_response(),
            empty_response(),
            empty_response(),
            empty_response(),
        ]);
        let frontier = store::pending_frontier(&conn, "grnoc-public-task-viewer").unwrap();
        assert_eq!(frontier.len(), 12);
        let mut client = GrnocViewerClient::new_with_base(fast_policy(), server.url("")).unwrap();
        sync_frontier(
            &conn,
            &mut client,
            "grnoc-public-task-viewer",
            &frontier,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        // The retrieved ticket links to its existing document reference
        // (the AAR) — by identifier, not title.
        let resolved = link_case_study_tickets(&conn, cs_id, "grnoc-public-task-viewer").unwrap();
        assert_eq!(resolved, 1, "only CHG0038258 was retrieved");
        let stmt = "SELECT external_identifier, catalog_event_id FROM case_study_event_links WHERE case_study_id = ?1 ORDER BY sort_order";
        let mut s = conn.prepare(stmt).unwrap();
        let rows: Vec<(String, Option<i64>)> = s
            .query_map([cs_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let chg = rows.iter().find(|(id, _)| id == "CHG0038258").unwrap();
        assert!(
            chg.1.is_some(),
            "retrieved ticket linked to its case-study reference"
        );
        for (id, ev) in &rows {
            if id != "CHG0038258" {
                assert!(ev.is_none(), "{id} stays unresolved (not fetched)");
            }
        }
    }

    #[test]
    fn source_timing_and_aar_timing_remain_distinct() {
        let (_dir, conn) = open_temp_db();
        let ids = manlan_seed_ids();
        let cs_id = manlan_case_study(&conn, &ids);
        crate::catalog::discovery::record_case_study_references(
            &conn,
            "grnoc-public-task-viewer",
            cs_id,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        // AAR planned window: 04:00–13:00 (planned start_date/end_date).
        // Source actual: work_start 04:38:38, work_end 13:00:00.
        let server = MockServer::start(vec![change_response(
            "CHG0038258",
            "Maintenance 1 of 2 Completed - MAN LAN Core Node (sw.net.manlan)",
            "1566362318",
            "1566392400",
        )]);
        let frontier = store::pending_frontier(&conn, "grnoc-public-task-viewer").unwrap();
        let frontier = crate::catalog::discovery::budgeted_frontier(&frontier, 1);
        let mut client = GrnocViewerClient::new_with_base(fast_policy(), server.url("")).unwrap();
        sync_frontier(
            &conn,
            &mut client,
            "grnoc-public-task-viewer",
            frontier,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        link_case_study_tickets(&conn, cs_id, "grnoc-public-task-viewer").unwrap();
        // The case-study (AAR) timing and the ticket source timing are
        // both preserved and distinct — never silently reconciled.
        let comparisons = source_vs_aar_timing(&conn, cs_id).unwrap();
        let chg = comparisons
            .iter()
            .find(|c| c.external_id == "CHG0038258")
            .unwrap();
        assert_eq!(chg.case_study_start, "2019-08-21T04:00:00Z");
        assert_eq!(chg.source_start, "2019-08-21T04:38:38Z");
        assert_ne!(chg.source_start, chg.case_study_start);
        // The planned window is preserved separately on the snapshot.
        let events = db::list_events(&conn).unwrap();
        let snapshots = db::list_snapshots(&conn, events[0].id).unwrap();
        let v: serde_json::Value = serde_json::from_str(&snapshots[0].normalized_json).unwrap();
        assert_eq!(v["planned_start"], "2019-08-21T04:00:00Z");
        assert_eq!(v["start"], "2019-08-21T04:38:38Z");
    }

    #[test]
    fn missing_public_ticket_remains_unresolved() {
        let (_dir, conn) = open_temp_db();
        let ids = vec!["TASK0038206".to_string(), "INC0099999".to_string()];
        let cs_id = manlan_case_study(&conn, &ids);
        crate::catalog::discovery::record_case_study_references(
            &conn,
            "grnoc-public-task-viewer",
            cs_id,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        // The viewer has no record for INC0099999 (empty result).
        let server = MockServer::start(vec![empty_response()]);
        let frontier = store::pending_frontier(&conn, "grnoc-public-task-viewer").unwrap();
        let mut client = GrnocViewerClient::new_with_base(fast_policy(), server.url("")).unwrap();
        let summary = sync_frontier(
            &conn,
            &mut client,
            "grnoc-public-task-viewer",
            &frontier,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        assert_eq!(summary.not_found, 1);
        assert_eq!(summary.unsupported, 1);
        assert_eq!(summary.new_snapshots, 0);
        let unresolved = store::list_discoveries(
            &conn,
            "grnoc-public-task-viewer",
            Some(DISCOVERY_STATUS_UNRESOLVED),
        )
        .unwrap();
        assert_eq!(unresolved.len(), 2);
        // The case-study link stays unresolved (no fabricated event).
        assert!(db::list_events(&conn).unwrap().is_empty());
        let resolved = link_case_study_tickets(&conn, cs_id, "grnoc-public-task-viewer").unwrap();
        assert_eq!(resolved, 0);
    }

    #[test]
    fn case_study_link_does_not_depend_on_title_matching() {
        let (_dir, conn) = open_temp_db();
        // The case-study link names CHG0038258 with an AAR-derived title;
        // the viewer's title differs. Linkage is by identifier only.
        let cs_id = manlan_case_study(&conn, &["CHG0038258".to_string()]);
        crate::catalog::discovery::record_case_study_references(
            &conn,
            "grnoc-public-task-viewer",
            cs_id,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        let server = MockServer::start(vec![change_response(
            "CHG0038258",
            "A completely different viewer title",
            "1566362318",
            "1566392400",
        )]);
        let frontier = store::pending_frontier(&conn, "grnoc-public-task-viewer").unwrap();
        let mut client = GrnocViewerClient::new_with_base(fast_policy(), server.url("")).unwrap();
        sync_frontier(
            &conn,
            &mut client,
            "grnoc-public-task-viewer",
            &frontier,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        let resolved = link_case_study_tickets(&conn, cs_id, "grnoc-public-task-viewer").unwrap();
        assert_eq!(resolved, 1);
    }
}
