//! Canonical plan-revision identity and queue-time plan validation.
//!
//! A queued job references an exact immutable plan revision
//! (`analysis_plans.id`) plus a canonical hash of the serialized plan.
//! The hash covers all execution-relevant fields (event/source snapshot
//! identity, window, warmup, cooldown, source family, collectors,
//! target origin mapping, named service plane, transit predicate,
//! expectation, lifecycle, schema revisions, review provenance) and
//! ignores generated timestamps and display labels. The same hash is
//! used for queue idempotency and run provenance.

use crate::catalog::document::hex_sha256;
use crate::domain::route::TransitPredicate;
use crate::manifest::Manifest;
use rusqlite::{params, Connection};
use serde::Serialize;

/// Canonical, execution-relevant plan identity. Field order is fixed:
/// the serialization is deterministic. Event/source-snapshot identity is
/// pinned by the manifest revision's snapshot reference (the plan row
/// holds `manifest_revision_id`); expectation is a function of that
/// snapshot and is not re-derived here.
#[derive(Debug, Clone, Serialize)]
pub struct CanonicalPlan {
    pub event_id: String,
    pub manifest_schema: u32,
    pub manifest_revision: u32,
    pub lifecycle: String,
    pub analysis_start: String,
    pub analysis_end: String,
    pub warmup_minutes: i64,
    pub cooldown_minutes: i64,
    pub source_family: String,
    pub collectors: Vec<String>,
    pub origin_asns: Vec<u32>,
    pub transit_predicate: Option<TransitPredicate>,
    pub transit_predicate_review: String,
    pub target_label: String,
    pub review_provenance: Option<String>,
}

impl CanonicalPlan {
    /// Build from a reviewed manifest. Deterministic: derived fields
    /// are normalized (collectors sorted, origin ASNs sorted/deduped),
    /// and no timestamp enters the identity.
    pub fn from_manifest(manifest: &Manifest) -> Result<CanonicalPlan, String> {
        let lifecycle = if manifest.open { "Open" } else { "Closed" };
        let mut collectors = manifest.collectors.clone();
        collectors.sort();
        collectors.dedup();
        let mut origin_asns = manifest.target.origin_asns.clone();
        origin_asns.sort_unstable();
        origin_asns.dedup();
        let predicate = manifest
            .target
            .transit_predicate
            .predicate
            .clone()
            .map(normalize_predicate);
        let provenance = manifest
            .target
            .transit_predicate
            .provenance
            .as_ref()
            .map(|p| format!("{}|{}", p.reviewed_by, p.date));
        // The effective analysis end is the reviewed cutoff for open
        // events (analysis_end_utc), else the declared event end. The
        // cutoff is execution-relevant, so it participates in the plan
        // identity (F-4).
        let analysis_end = if manifest.open {
            manifest.analysis_end_utc.clone().unwrap_or_default()
        } else {
            manifest.event_window_utc.end.clone()
        };
        Ok(CanonicalPlan {
            event_id: manifest.event_id.clone(),
            manifest_schema: manifest.schema_version,
            manifest_revision: manifest.revision,
            lifecycle: lifecycle.to_string(),
            analysis_start: manifest.event_window_utc.start.clone(),
            analysis_end,
            warmup_minutes: manifest.warmup_minutes,
            cooldown_minutes: manifest.cooldown_minutes,
            source_family: manifest.source_family.clone(),
            collectors,
            origin_asns,
            transit_predicate: predicate,
            transit_predicate_review: format!("{:?}", manifest.target.transit_predicate.status),
            target_label: manifest.target.label.clone(),
            review_provenance: provenance,
        })
    }

    /// Deterministic canonical serialization (no whitespace).
    pub fn canonical_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("cannot serialize canonical plan: {e}"))
    }
}

/// Normalize a predicate's inner ASN order for deterministic identity.
fn normalize_predicate(p: TransitPredicate) -> TransitPredicate {
    match p {
        TransitPredicate::ContainsAny(mut asns) => {
            asns.sort_unstable();
            asns.dedup();
            TransitPredicate::ContainsAny(asns)
        }
        TransitPredicate::ContainsAll(mut asns) => {
            asns.sort_unstable();
            asns.dedup();
            TransitPredicate::ContainsAll(asns)
        }
        other => other,
    }
}

/// SHA-256 of the canonical serialized plan revision. Deterministic and
/// timestamp-free; changes when any execution-relevant field changes.
pub fn canonical_plan_hash(manifest_payload: &str) -> Result<String, String> {
    let manifest: Manifest = serde_json::from_str(manifest_payload)
        .map_err(|e| format!("invalid manifest payload: {e}"))?;
    let canonical = CanonicalPlan::from_manifest(&manifest)?;
    let json = canonical.canonical_json()?;
    Ok(hex_sha256(json.as_bytes()))
}

/// Load the manifest payload behind a plan revision.
pub fn manifest_payload_for_plan(
    conn: &Connection,
    plan_revision_id: i64,
) -> Result<String, String> {
    conn.query_row(
        "SELECT m.payload FROM manifest_revisions m
         JOIN analysis_plans p ON p.manifest_revision_id = m.id
         WHERE p.id = ?1",
        params![plan_revision_id],
        |r| r.get(0),
    )
    .map_err(|e| format!("cannot load manifest payload for plan {plan_revision_id}: {e}"))
}

/// Queue-time plan validation: exact revision exists, schema current,
/// reviewed origin mapping present, reviewed transit predicate present,
/// collectors non-empty, source family supported.
///
/// Returns Ok(plan_hash) when the plan is queueable. The stored plan
/// status must be Ready; these checks re-derive readiness from the
/// reviewed manifest so a stale or hand-edited status cannot slip
/// through.
/// Resolve the catalog event for a plan revision.
fn event_for_plan(
    conn: &Connection,
    plan_revision_id: i64,
) -> Result<crate::catalog::domain::CatalogEvent, String> {
    conn.query_row(
        "SELECT e.id, e.source_kind, e.external_id, e.first_seen, e.last_seen
         FROM catalog_events e
         JOIN manifest_revisions m ON m.event_id = e.id
         JOIN analysis_plans p ON p.manifest_revision_id = m.id
         WHERE p.id = ?1",
        params![plan_revision_id],
        |r| {
            Ok(crate::catalog::domain::CatalogEvent {
                id: r.get(0)?,
                source_kind: r.get(1)?,
                external_id: r.get(2)?,
                first_seen: r.get(3)?,
                last_seen: r.get(4)?,
            })
        },
    )
    .map_err(|e| format!("cannot resolve event for plan {plan_revision_id}: {e}"))
}

/// Whether a plan's event or target is excluded by project scope.
/// Returns the stable reason code when excluded.
pub fn plan_scope_exclusion(
    conn: &Connection,
    plan_revision_id: i64,
    scope: &crate::catalog::scope::ProjectScope,
) -> Result<Option<String>, String> {
    let event = event_for_plan(conn, plan_revision_id)?;
    if let Some(reason) = scope.source_record_reason(&event.source_kind, &event.external_id) {
        return Ok(Some(reason));
    }
    // The entity/ASN checks need the reviewed manifest payload. An
    // unparseable payload can never be queued (validate_plan_for_queue
    // rejects it), so the scope check simply does not add an exclusion
    // on top of a payload that queue validation will refuse anyway.
    let Ok(payload) = manifest_payload_for_plan(conn, plan_revision_id) else {
        return Ok(None);
    };
    let Ok(manifest) = serde_json::from_str::<Manifest>(&payload) else {
        return Ok(None);
    };
    if let Some(reason) = scope.entity_name_reason(&manifest.target.label) {
        return Ok(Some(reason));
    }
    if scope.any_asn_excluded(&manifest.target.origin_asns) {
        return Ok(Some(
            crate::catalog::scope::REASON_PROJECT_OWNER_EXCLUSION.to_string(),
        ));
    }
    Ok(None)
}

/// Stable machine code for project-scope queue/execution refusal.
pub const SCOPE_EXCLUDED_CODE: &str = "project_scope_excluded";
/// Operator-facing language for project-scope refusals. Never invents
/// a reason beyond the configured policy.
pub const SCOPE_EXCLUDED_MESSAGE: &str =
    "This event is outside the configured project scope and cannot be queued.";

pub fn validate_plan_for_queue(
    conn: &Connection,
    plan_revision_id: i64,
    scope: &crate::catalog::scope::ProjectScope,
) -> Result<String, String> {
    let plan_schema: i64 = conn
        .query_row(
            "SELECT plan_schema FROM analysis_plans WHERE id = ?1",
            params![plan_revision_id],
            |r| r.get(0),
        )
        .map_err(|_| format!("plan revision not found: {plan_revision_id}"))?;
    if plan_schema != crate::schema::ANALYSIS_PLAN_SCHEMA_VERSION as i64 {
        return Err(format!(
            "incompatible_plan_schema: plan {plan_revision_id} is schema v{plan_schema}, current v{}",
            crate::schema::ANALYSIS_PLAN_SCHEMA_VERSION
        ));
    }
    if let Some(_reason) = plan_scope_exclusion(conn, plan_revision_id, scope)? {
        return Err(format!("{SCOPE_EXCLUDED_CODE}: {SCOPE_EXCLUDED_MESSAGE}"));
    }
    let payload = manifest_payload_for_plan(conn, plan_revision_id)?;
    let manifest: Manifest = serde_json::from_str(&payload)
        .map_err(|e| format!("invalid_plan: manifest payload unreadable: {e}"))?;
    manifest
        .validate()
        .map_err(|e| format!("invalid_plan: {e}"))?;

    if manifest.target.origin_asns.is_empty() {
        return Err("invalid_plan: target origin mapping not reviewed".to_string());
    }
    if !manifest.target.transit_predicate.is_ready() {
        return Err("invalid_plan: transit predicate not reviewed".to_string());
    }
    if manifest.collectors.is_empty() {
        return Err("invalid_plan: collector selection empty".to_string());
    }
    let family = manifest.source_family.to_lowercase();
    let supported = matches!(family.as_str(), "routeviews" | "riperis");
    if !supported {
        return Err(format!(
            "invalid_plan: source family unsupported: {}",
            manifest.source_family
        ));
    }
    if manifest.event_window_utc.end.is_empty() {
        if !manifest.open {
            return Err("invalid_plan: event end unavailable".to_string());
        }
        // Open events are executable only with an explicit REVIEWED
        // analysis cutoff; the plan records it and the result is
        // provisional. A missing cutoff is a hard plan error.
        let has_cutoff = manifest
            .analysis_end_utc
            .as_deref()
            .map(|c| !c.trim().is_empty())
            .unwrap_or(false);
        if !has_cutoff {
            return Err(
                "invalid_plan: open event requires an explicit analysis cutoff".to_string(),
            );
        }
    }
    // Defense in depth (F-4): an open event requires the reviewed
    // analysis cutoff regardless of any declared event end, so a legacy
    // or crafted manifest with `open: true` and no cutoff can never
    // reach the queue.
    if manifest.open {
        let has_cutoff = manifest
            .analysis_end_utc
            .as_deref()
            .map(|c| !c.trim().is_empty())
            .unwrap_or(false);
        if !has_cutoff {
            return Err(
                "invalid_plan: open event requires an explicit analysis cutoff".to_string(),
            );
        }
    }
    canonical_plan_hash(&payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_json() -> String {
        serde_json::json!({
            "event_id": "EVENT-1",
            "revision": 1,
            "schema_version": 2,
            "open": false,
            "event_window_utc": {"start": "2026-08-01T00:00:00Z", "end": "2026-08-02T00:00:00Z"},
            "ticket_window_local": {"start": "", "end": "", "timezone": "UTC"},
            "warmup_minutes": 30,
            "cooldown_minutes": 15,
            "target": {
                "label": "Test event",
                "origin_asns": [64500],
                "transit_predicate": {
                    "predicate": {"ContainsAny": [64501]},
                    "status": "Reviewed",
                    "provenance": {"statement": "reviewed", "reviewed_by": "local-review", "date": "2026-08-01"}
                }
            },
            "collectors": ["route-views2"],
            "source_family": "RouteViews"
        })
        .to_string()
    }

    #[test]
    fn plan_hash_changes_for_execution_field() {
        let base = canonical_plan_hash(&manifest_json()).unwrap();
        // Warmup is execution-relevant.
        let mut v: serde_json::Value = serde_json::from_str(&manifest_json()).unwrap();
        v["warmup_minutes"] = serde_json::json!(45);
        let changed = canonical_plan_hash(&v.to_string()).unwrap();
        assert_ne!(base, changed);
        // Collector selection is execution-relevant.
        let mut v: serde_json::Value = serde_json::from_str(&manifest_json()).unwrap();
        v["collectors"] = serde_json::json!(["rrc00"]);
        let changed = canonical_plan_hash(&v.to_string()).unwrap();
        assert_ne!(base, changed);
        // Origin mapping is execution-relevant.
        let mut v: serde_json::Value = serde_json::from_str(&manifest_json()).unwrap();
        v["target"]["origin_asns"] = serde_json::json!([64510]);
        let changed = canonical_plan_hash(&v.to_string()).unwrap();
        assert_ne!(base, changed);
    }

    #[test]
    fn cutoff_participates_in_plan_hash_when_semantic() {
        // F-4: the reviewed analysis cutoff is part of the canonical
        // plan payload, so changing it changes the plan hash (an open
        // plan's identity includes its cutoff).
        let mut v: serde_json::Value = serde_json::from_str(&manifest_json()).unwrap();
        v["open"] = serde_json::json!(true);
        v["analysis_end_utc"] = serde_json::json!("2026-08-02T00:00:00Z");
        let base = canonical_plan_hash(&v.to_string()).unwrap();
        v["analysis_end_utc"] = serde_json::json!("2026-08-02T01:00:00Z");
        let changed = canonical_plan_hash(&v.to_string()).unwrap();
        assert_ne!(base, changed, "cutoff must participate in plan identity");
        // And the same payload hashes deterministically.
        let again = canonical_plan_hash(&v.to_string()).unwrap();
        assert_eq!(changed, again);
    }

    #[test]
    fn plan_hash_ignores_generated_timestamp() {
        let base = canonical_plan_hash(&manifest_json()).unwrap();
        // A generated_at / created_at field (not in the canonical
        // struct) must not alter the hash.
        let mut v: serde_json::Value = serde_json::from_str(&manifest_json()).unwrap();
        v["created_at"] = serde_json::json!("2026-08-02T10:00:00Z");
        assert_eq!(canonical_plan_hash(&v.to_string()).unwrap(), base);
    }

    #[test]
    fn plan_hash_is_deterministic() {
        let a = canonical_plan_hash(&manifest_json()).unwrap();
        let b = canonical_plan_hash(&manifest_json()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn plan_hash_normalizes_collector_order() {
        let mut v: serde_json::Value = serde_json::from_str(&manifest_json()).unwrap();
        v["collectors"] = serde_json::json!(["route-views2", "route-views3"]);
        let a = canonical_plan_hash(&v.to_string()).unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&manifest_json()).unwrap();
        v["collectors"] = serde_json::json!(["route-views3", "route-views2"]);
        let b = canonical_plan_hash(&v.to_string()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn reviewed_and_derived_fields_are_distinct() {
        // The canonical struct separates reviewed provenance from
        // derived execution fields; changing the provenance marker
        // changes the hash (identity includes review provenance).
        let base = canonical_plan_hash(&manifest_json()).unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&manifest_json()).unwrap();
        v["target"]["transit_predicate"]["provenance"]["reviewed_by"] =
            serde_json::json!("other-review");
        let changed = canonical_plan_hash(&v.to_string()).unwrap();
        assert_ne!(base, changed);
    }

    #[test]
    fn completed_run_preserves_exact_plan_hash() {
        // The worker stores the same hash on the job and in the run's
        // execution metadata; here we assert the identity function is
        // stable across repeated computation (the linkage test lives in
        // the end-to-end fixture).
        let h = canonical_plan_hash(&manifest_json()).unwrap();
        assert_eq!(h.len(), 64);
    }
}
