//! Regression tests for the MAN LAN entity taxonomy correction and the
//! Smithville event-page coverage/provenance repair.
//!
//! Case-study integration tests load the tracked cases through the same
//! repository import the demo uses; no live network, no analysis.
//! Generic component tests live in `src/catalog/web/path_diagram.rs`.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crate::catalog::web::server::{build_app, build_state};
use crate::catalog::web::AppState;

fn repo_artifacts_available() -> bool {
    std::path::Path::new("case-studies/manlan-2019/pilot/out").is_dir()
        && std::path::Path::new(
            "case-studies/indiana-gigapop-smithville-2026/out/INC0301970/report.json",
        )
        .is_file()
}

fn setup_demo_catalog() -> (tempfile::TempDir, std::path::PathBuf) {
    let dbdir = tempfile::tempdir().unwrap();
    let path = dbdir.path().join("catalog.sqlite");
    crate::catalog::demo::demo_init(&path, std::path::Path::new("."), false)
        .expect("demo import succeeds");
    (dbdir, std::path::PathBuf::from("."))
}

fn state_from(dbdir: &tempfile::TempDir, rootdir: &std::path::Path) -> Arc<AppState> {
    build_state(
        &dbdir.path().join("catalog.sqlite"),
        rootdir,
        "0.1.0",
        false,
    )
    .unwrap()
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, String) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

fn db_conn(dbdir: &tempfile::TempDir) -> rusqlite::Connection {
    crate::catalog::db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap()
}

/// The first SVG element (the fabric diagram on the case-study page).
fn fabric_svg(body: &str) -> &str {
    let start = body.find("<svg").unwrap_or(0);
    let end = body[start..]
        .find("</svg>")
        .map(|i| start + i)
        .unwrap_or(body.len());
    &body[start..end]
}

/// The reviewed attached-networks section, up to the other-context
/// section (so contextual entities never leak into the assertion).
fn attached_section(text: &str) -> &str {
    let start = text
        .find("Reviewed attached networks (Layer-2 fabric participants)")
        .unwrap_or(0);
    let end = text[start..]
        .find("Other incident context")
        .map(|i| start + i)
        .unwrap_or(text.len());
    &text[start..end]
}

fn strip_html(body: &str) -> String {
    let re = regex::Regex::new(r"<[^>]+>").unwrap();
    let text = re.replace_all(body, " ");
    let text = text.replace("&nbsp;", " ");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ── Ixia classification (Part 2) ──────────────────────────────────

#[tokio::test]
async fn ixia_is_not_layer2_fabric_attachment() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let text = strip_html(&body);
    // Ixia appears only inside the "Test equipment" contextual table,
    // never in the reviewed attached-networks table.
    let attached_region = attached_section(&text);
    let other_region = text
        .find("Other incident context")
        .map(|i| &text[i..])
        .unwrap_or("");
    assert!(
        !attached_region.contains("Ixia"),
        "Ixia must not appear in the reviewed attached-networks section"
    );
    assert!(
        other_region.contains("Ixia test equipment"),
        "Ixia renders as test equipment context"
    );
}

#[tokio::test]
async fn ixia_is_not_rendered_as_network_node() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    // The fabric SVG (the network diagram) contains no Ixia node.
    let svg = fabric_svg(&body);
    assert!(
        !svg.contains("Ixia"),
        "Ixia must not be a node in the fabric diagram"
    );
}

#[tokio::test]
async fn ixia_is_not_counted_as_attached_network() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let text = strip_html(&body);
    // The count line states the reviewed attached-network count; Ixia
    // is not among the counted labels (verified by the table test).
    assert!(
        text.contains("5 reviewed attached networks; 5 other source-mentioned entities"),
        "attachment count is 5, other-mentioned count is 5"
    );
}

#[tokio::test]
async fn ixia_has_no_asn() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let text = strip_html(&body);
    let ixia_region = text
        .find("Ixia test equipment")
        .map(|i| &text[i..i + 120])
        .unwrap_or("");
    assert!(
        ixia_region.contains("no reviewed ASN"),
        "Ixia carries no ASN: {ixia_region}"
    );
}

#[tokio::test]
async fn ixia_has_no_bgp_relationship() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let text = strip_html(&body);
    // No edge (solid or dashed) references Ixia, and no "peer" wording
    // is attached to the Ixia row.
    let ixia_region = text
        .find("Ixia test equipment")
        .map(|i| &text[i..i + 200])
        .unwrap_or("");
    assert!(
        ixia_region.contains("not a BGP peer"),
        "Ixia explicitly not a BGP peer: {ixia_region}"
    );
    assert!(
        !ixia_region.contains("Adjacent") && !ixia_region.contains("ContainsAny"),
        "Ixia has no adjacency/predicate wording: {ixia_region}"
    );
    // No node markup (svg text node or edge) for Ixia: the fabric SVG
    // has no Ixia, and no solid/dashed edge references it.
    assert!(
        !fabric_svg(&body).contains("Ixia"),
        "no Ixia node markup in any diagram"
    );
}

#[tokio::test]
async fn ixia_may_render_as_test_equipment_context() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(
        body.contains("Test equipment") && body.contains("Ixia test equipment"),
        "Ixia renders in the test-equipment contextual section"
    );
    assert!(
        body.contains("network test/measurement hardware"),
        "Ixia described as measurement hardware"
    );
}

// ── Taxonomy rules (Part 3) ───────────────────────────────────────

#[tokio::test]
async fn aar_mention_does_not_imply_attachment() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let text = strip_html(&body);
    let attached_region = attached_section(&text);
    let other_region = text
        .find("Other incident context")
        .map(|i| &text[i..])
        .unwrap_or("");
    for aar_mentioned in ["WIX interconnect", "NEAAR", "OMAN"] {
        assert!(
            !attached_region.contains(aar_mentioned),
            "{aar_mentioned} must not be an attached network (AAR mention only)"
        );
        assert!(
            other_region.contains(aar_mentioned),
            "{aar_mentioned} must appear in other incident context"
        );
    }
}

#[tokio::test]
async fn reviewed_asn_does_not_imply_attachment() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let text = strip_html(&body);
    let attached_region = attached_section(&text);
    assert!(
        !attached_region.contains("TWAREN"),
        "TWAREN has a reviewed ASN but not established MAN LAN attachment"
    );
    // The reviewed ASN is retained as identity detail in the
    // unresolved list.
    let unresolved_region = text
        .find("Unresolved source mentions")
        .map(|i| &text[i..])
        .unwrap_or("");
    assert!(
        unresolved_region.contains("TWAREN") && unresolved_region.contains("AS7539"),
        "TWAREN AS7539 retained as identity detail: {unresolved_region}"
    );
}

#[tokio::test]
async fn equipment_cannot_enter_as_path() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let text = strip_html(&body);
    let story_region = text
        .find("Representative observer route story")
        .map(|i| &text[i..])
        .unwrap_or("");
    assert!(
        !story_region.contains("Ixia"),
        "test equipment never enters an observed AS path"
    );
}

#[tokio::test]
async fn unresolved_entity_not_rendered_as_network() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let svg = fabric_svg(&body);
    for unresolved in ["OMAN", "NEAAR", "TWAREN"] {
        assert!(
            !svg.contains(unresolved),
            "{unresolved} must not be a fabric network node"
        );
    }
}

// ── Fabric diagram entity audit (Part 4) ──────────────────────────

#[tokio::test]
async fn diagram_entities_equal_reviewed_attached_networks() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    // Reviewed attachment labels from the tracked case-study data.
    let cs: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("case-studies/manlan-2019/case-study.json").unwrap(),
    )
    .unwrap();
    let mut expected: Vec<String> = cs["interconnection_context"]["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["label"].as_str().unwrap().to_string())
        .collect();
    expected.sort();
    // Node labels present in the fabric SVG.
    let svg = fabric_svg(&body);
    let mut found: Vec<String> = expected
        .iter()
        .filter(|label| svg.contains(label.as_str()))
        .cloned()
        .collect();
    found.sort();
    assert_eq!(
        found, expected,
        "fabric diagram entities must equal the reviewed attached networks"
    );
    // No reviewed attachment is missing from the diagram.
    assert_eq!(found.len(), expected.len());
}

#[tokio::test]
async fn contextual_entities_not_given_attachment_edges() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let svg = fabric_svg(&body);
    for contextual in ["Ixia", "NEAAR", "OMAN", "WIX", "TWAREN"] {
        assert!(
            !svg.contains(contextual),
            "{contextual} must not have a diagram attachment edge"
        );
    }
}

#[tokio::test]
async fn unresolved_entities_preserve_uncertainty() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let text = strip_html(&body);
    let unresolved_region = text
        .find("Unresolved source mentions")
        .map(|i| &text[i..])
        .unwrap_or("");
    assert!(
        unresolved_region.contains("less certain") || unresolved_region.contains("unresolved"),
        "uncertainty is preserved for unresolved entities: {unresolved_region}"
    );
    assert!(
        unresolved_region.contains("OMAN"),
        "OMAN listed as unresolved source mention"
    );
}

// ── Smithville observation coverage (Parts 7-8) ───────────────────

#[tokio::test]
async fn smithville_event_page_has_observation_coverage() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    let text = strip_html(&body);
    assert!(
        text.contains("Observation coverage"),
        "event page has an observation-coverage section"
    );
    assert!(
        text.contains("Smithville-origin routes were visible at selected public collectors"),
        "operator-readable coverage summary present: {text}"
    );
    assert!(
        text.contains("UPDATE archives were not acquired"),
        "UPDATE acquisition outcome explained"
    );
    assert!(
        text.contains("No qualifying baseline cohort was formed"),
        "why no UPDATE archives were acquired"
    );
}

#[tokio::test]
async fn target_visibility_and_relationship_visibility_distinct() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    let text = strip_html(&body);
    assert!(
        text.contains("Target visible?") && text.contains("Reviewed relationship visible?"),
        "target visibility and relationship visibility are separate columns"
    );
    assert!(
        text.contains("Yes — 13 prefixes"),
        "target prefixes visible at IPv4 collectors"
    );
    assert!(
        text.contains("Target routes visible; reviewed relationship not visible"),
        "primary human label for present-target absent-relationship collectors"
    );
    assert!(
        text.contains("Target origin not visible"),
        "primary human label for the IPv6-only collector"
    );
}

#[tokio::test]
async fn collector_site_not_peer_location() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    assert!(
        body.contains("Collector site"),
        "collector-site column present"
    );
    assert!(
        body.contains("not the observer peer's location"),
        "collector site is not labeled as peer location"
    );
}

#[tokio::test]
async fn event_summary_links_to_full_evidence() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let conn = db_conn(&dbdir);
    let run_id: i64 = conn
        .query_row(
            "SELECT r.id FROM analysis_runs r
             JOIN analysis_plans p ON p.id = r.plan_id
             JOIN manifest_revisions m ON m.id = p.manifest_revision_id
             JOIN catalog_events e ON e.id = m.event_id
             WHERE e.external_id = 'INC0301970' AND r.status = 'Complete'
             ORDER BY r.id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    assert!(
        body.contains("Full observation coverage and evidence"),
        "link to full evidence present"
    );
    assert!(
        body.contains(&format!("/analyses/{run_id}")),
        "full-evidence link targets the run workbench (run {run_id})"
    );
}

#[tokio::test]
async fn insufficient_visibility_not_no_change() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    let text = strip_html(&body);
    assert!(
        text.contains("Insufficient qualifying visibility"),
        "observed result label present"
    );
    assert!(
        !text.contains("routing was stable") && !text.contains("no route-state change"),
        "insufficient visibility is not presented as no-change"
    );
}

// ── Snapshot / cutoff provenance (Part 9) ─────────────────────────

#[tokio::test]
async fn source_snapshot_time_and_analysis_cutoff_are_distinct() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    let text = strip_html(&body);
    assert!(
        text.contains("Source snapshot fetched at: 2026-08-04T00:01:37Z"),
        "source snapshot fetch time shown: {text}"
    );
    assert!(
        text.contains("Analysis cutoff: 2026-08-04T00:01:37Z"),
        "analysis cutoff shown"
    );
    let snapshot_pos = text.find("Source snapshot fetched at");
    let cutoff_pos = text.find("Analysis cutoff");
    assert!(
        snapshot_pos.is_some() && cutoff_pos.is_some() && snapshot_pos < cutoff_pos,
        "snapshot time and cutoff are distinct labeled fields"
    );
}

#[tokio::test]
async fn open_lifecycle_claim_has_snapshot_provenance() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    let text = strip_html(&body);
    assert!(
        text.contains("Source lifecycle at snapshot: Open"),
        "lifecycle anchored to the snapshot"
    );
    assert!(
        text.contains("state In Progress"),
        "lifecycle evidence shown"
    );
}

#[tokio::test]
async fn cutoff_provenance_visible() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    let text = strip_html(&body);
    assert!(
        text.contains("Cutoff provenance"),
        "cutoff provenance field present"
    );
    assert!(
        text.contains("reviewed snapshot cutoff"),
        "cutoff identified as the reviewed snapshot cutoff"
    );
}

#[tokio::test]
async fn demo_imports_latest_reviewed_snapshot_when_present() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, _rootdir) = setup_demo_catalog();
    let conn = db_conn(&dbdir);
    let snapshot: Option<(String, String)> = conn
        .query_row(
            "SELECT s.fetched_at, s.content_sha256
             FROM event_snapshots s
             JOIN catalog_events e ON e.id = s.event_id
             WHERE e.external_id = 'INC0301970'
             ORDER BY s.id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let (fetched_at, sha) = snapshot.expect("INC0301970 snapshot imported");
    assert_eq!(fetched_at, "2026-08-04T00:01:37Z", "reviewed fetch time");
    // The imported snapshot is the tracked immutable source snapshot.
    let tracked =
        std::fs::read("case-studies/indiana-gigapop-smithville-2026/INC0301970.source.json")
            .unwrap();
    let tracked_sha = { crate::catalog::document::hex_sha256(&tracked) };
    assert_eq!(
        sha, tracked_sha,
        "imported snapshot is the tracked source.json"
    );
}

#[tokio::test]
async fn no_immutable_snapshot_mutation() {
    if !repo_artifacts_available() {
        return;
    }
    // The tracked immutable snapshot still hashes to the SHA-256
    // recorded in the case-study README.
    let tracked =
        std::fs::read("case-studies/indiana-gigapop-smithville-2026/INC0301970.source.json")
            .unwrap();
    let sha = crate::catalog::document::hex_sha256(&tracked);
    assert_eq!(
        sha, "d911687c634a5efa7eafbea5816c4aa376f61c7c8fc14bd3611042873696de77",
        "immutable snapshot content must be unchanged"
    );
}

// ── Status labels (Part 10) ───────────────────────────────────────

#[tokio::test]
async fn analysis_status_and_source_lifecycle_distinct() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    let text = strip_html(&body);
    assert!(
        text.contains("Analysis status") && text.contains("Source lifecycle"),
        "analysis status and source lifecycle are distinct labeled fields"
    );
    let status_pos = text.find("Analysis status");
    let lifecycle_pos = text.find("Source lifecycle");
    assert!(
        status_pos.is_some() && lifecycle_pos.is_some() && status_pos < lifecycle_pos,
        "analysis status precedes source lifecycle"
    );
}

#[tokio::test]
async fn complete_does_not_imply_event_closed() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    let text = strip_html(&body);
    assert!(
        text.contains("Analysis status") && text.contains("Complete"),
        "analysis status Complete shown"
    );
    assert!(
        text.contains("Source lifecycle") && text.contains("Open"),
        "source lifecycle Open shown alongside"
    );
    assert!(text.contains("Analysis cutoff"), "cutoff retained");
    assert!(
        !text.contains("event is closed") && !text.contains("Lifecycle Closed"),
        "Complete must not be presented as event closure"
    );
}

// ── Assessment wording (Part 11) ──────────────────────────────────

#[tokio::test]
async fn smithville_human_explanation_mentions_target_visibility() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    let text = strip_html(&body);
    assert!(
        text.contains("Target routes were visible at selected public collectors"),
        "operator-readable explanation mentions target visibility"
    );
}

#[tokio::test]
async fn smithville_human_explanation_mentions_relationship_visibility() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    let text = strip_html(&body);
    assert!(
        text.contains("reviewed relationship was not exposed by the selected baselines"),
        "operator-readable explanation mentions relationship visibility"
    );
}

#[tokio::test]
async fn technical_blockers_preserved() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    assert!(
        body.contains("TargetPresentRelationshipAbsent"),
        "exact machine classification preserved"
    );
    assert!(
        body.contains("RequiredSessionAbsent"),
        "required-session blocker preserved"
    );
}

// ── Neutrality / evidence safety (Part 14) ────────────────────────

#[tokio::test]
async fn equipment_never_rendered_as_as_node() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let svg = fabric_svg(&body);
    assert!(
        !svg.contains("Ixia"),
        "equipment is never an AS node in the fabric diagram"
    );
}

#[tokio::test]
async fn equipment_never_rendered_as_peer() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let text = strip_html(&body);
    let ixia_region = text
        .find("Ixia test equipment")
        .map(|i| &text[i..i + 300])
        .unwrap_or("");
    assert!(
        ixia_region.contains("not a BGP peer"),
        "equipment explicitly not a peer: {ixia_region}"
    );
}

#[tokio::test]
async fn equipment_never_enters_attachment_count() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let text = strip_html(&body);
    assert!(
        text.contains("5 reviewed attached networks"),
        "attachment count excludes equipment"
    );
}

#[tokio::test]
async fn only_reviewed_attached_networks_enter_fabric_diagram() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let cs: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("case-studies/manlan-2019/case-study.json").unwrap(),
    )
    .unwrap();
    let expected: Vec<String> = cs["interconnection_context"]["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["label"].as_str().unwrap().to_string())
        .collect();
    let svg = fabric_svg(&body);
    for label in &expected {
        assert!(
            svg.contains(label.as_str()),
            "attached network {label} drawn"
        );
    }
    // The diagram draws no entity outside the reviewed set.
    for token in ["Ixia", "NEAAR", "OMAN", "TWAREN", "WIX"] {
        if !expected.iter().any(|l| l.contains(token)) {
            assert!(!svg.contains(token), "{token} not drawn");
        }
    }
}

#[tokio::test]
async fn smithville_summary_derived_from_artifacts() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    // The rendered summary is the reviewed file's summary verbatim.
    let coverage: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            "case-studies/indiana-gigapop-smithville-2026/observation-coverage.json",
        )
        .unwrap(),
    )
    .unwrap();
    let summary = coverage["summary"].as_str().unwrap();
    let text = strip_html(&body);
    assert!(
        text.contains(summary),
        "coverage summary derived from the reviewed file"
    );
}

#[tokio::test]
async fn smithville_cutoff_provenance_derived_from_reviewed_data() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    let meta: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            "case-studies/indiana-gigapop-smithville-2026/INC0301970.source.json.meta.json",
        )
        .unwrap(),
    )
    .unwrap();
    let provenance = meta["cutoff_provenance"].as_str().unwrap();
    let text = strip_html(&body);
    assert!(
        text.contains(provenance),
        "cutoff provenance rendered from the reviewed meta file"
    );
}

#[tokio::test]
async fn canonical_bgp_artifact_hashes_unchanged() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, _rootdir) = setup_demo_catalog();
    let conn = db_conn(&dbdir);
    // For every artifact row of the complete Smithville run, the file
    // on disk hashes to the recorded catalog SHA-256.
    let rows: Vec<(i64, String, String)> = conn
        .prepare(
            "SELECT a.run_id, a.relative_path, a.sha256
             FROM analysis_artifacts a
             JOIN analysis_runs r ON r.id = a.run_id
             WHERE r.status = 'Complete'",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(|x| x.unwrap())
        .collect();
    assert!(!rows.is_empty(), "complete run has artifact rows");
    for (_run, rel, recorded) in rows {
        let Ok(bytes) = std::fs::read(&rel) else {
            continue; // runtime-only artifacts may not be tracked
        };
        let sha = crate::catalog::document::hex_sha256(&bytes);
        assert_eq!(
            sha, recorded,
            "canonical artifact {rel} hash must match the catalog"
        );
    }
}
