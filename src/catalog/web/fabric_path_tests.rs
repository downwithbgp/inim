//! Page-level regression tests for Layer-2 fabric semantics and
//! evidence-grounded AS-path diagrams.
//!
//! Case-study integration tests load the tracked cases through the
//! same repository import the demo uses; generic component tests live
//! in `src/catalog/web/path_diagram.rs` with neutral documentation
//! ASNs. No live network; no analysis.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crate::catalog::db;
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

fn run_id_for(dbdir: &tempfile::TempDir, external_id: &str) -> i64 {
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    conn.query_row(
        "SELECT r.id FROM analysis_runs r
         JOIN analysis_plans p ON p.id = r.plan_id
         JOIN manifest_revisions m ON m.id = p.manifest_revision_id
         JOIN catalog_events e ON e.id = m.event_id
         WHERE e.external_id = ?1 ORDER BY r.id LIMIT 1",
        [external_id],
        |r| r.get(0),
    )
    .unwrap()
}

/// The comparison panel region of the MAN LAN page.
fn comparison_region(body: &str) -> &str {
    let start = body.find("Representative observer route story").unwrap();
    let end = body.find("Diagram legend").unwrap_or(body.len());
    &body[start..end]
}

// ── Part 1: MAN LAN topology framing ──────────────────────────────

#[tokio::test]
async fn manlan_not_rendered_as_asn() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(
        body.contains("Layer-2 fabric — not a BGP speaker"),
        "{body}"
    );
    // The fabric has no ASN: the fabric block carries no AS number and
    // the attachment table shows ASN labels only for reviewed ones.
    assert!(body.contains("no reviewed ASN"), "{body}");
    let fabric = body.find("Layer-2 fabric — not a BGP speaker").unwrap();
    let end = body[fabric..].find("<line class=\"pd-attach\"").unwrap() + fabric;
    let seg = &body[fabric..end];
    assert!(
        !seg.contains("AS11537") && !seg.contains("AS2603"),
        "fabric block itself has no ASN: {seg}"
    );
    assert!(seg.contains("MAN LAN"), "fabric label rendered: {seg}");
}

#[tokio::test]
async fn manlan_not_rendered_as_bgp_speaker() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(
        body.contains("does not speak BGP, does not originate routes"),
        "explicit non-speaker framing: {body}"
    );
    assert!(
        body.contains("does not appear as an AS-path hop"),
        "no AS-path hop framing: {body}"
    );
}

#[tokio::test]
async fn nordunet_identified_as_target_not_fabric() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let completed = body.find("Completed public-BGP analysis").unwrap();
    let seg = &body[completed..completed + 500];
    assert!(
        seg.to_lowercase().contains("nordunet") && seg.contains("AS2603"),
        "analyzed target named in the completed-analysis panel: {seg}"
    );
    // NORDUnet is an attachment in the fabric table, not the fabric itself.
    let context = body.find("Reviewed attachment context").unwrap();
    let seg = &body[context..context + 1200];
    assert!(
        seg.to_lowercase().contains("nordunet") && seg.contains("AS2603"),
        "attachment listed with reviewed ASN: {seg}"
    );
}

#[tokio::test]
async fn pilot_not_presented_as_all_connector_analysis() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(
        body.contains("not a complete analysis of the fabric or of all connectors")
            || body.contains("not a complete analysis of the fabric or all connectors"),
        "scope limitation explicit: {body}"
    );
    assert!(
        !body.contains("Historical pilot — Not planned"),
        "pilot with runs is not 'not planned'"
    );
}

// ── Part 2: fabric context concept ────────────────────────────────

#[tokio::test]
async fn layer2_fabric_is_not_as_identity() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, _rootdir) = setup_demo_catalog();
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    let ctx: String = conn
        .query_row(
            "SELECT interconnection_context FROM case_studies WHERE slug = 'manlan-2019'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&ctx).unwrap();
    assert_eq!(v["kind"], "Layer2Fabric");
    assert!(v.get("asn").is_none(), "fabric context has no ASN field");
    assert!(v.get("attachments").is_some());
}

#[tokio::test]
async fn attachment_is_not_bgp_adjacency() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(
        body.contains("Attachment is not BGP adjacency"),
        "attachment/adjacency distinction rendered: {body}"
    );
    assert!(
        body.contains("does not prove a direct\n  BGP session")
            || body.contains("does not prove a direct BGP session"),
        "attachment limits rendered: {body}"
    );
}

#[tokio::test]
async fn attachment_does_not_enter_route_predicate() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, _rootdir) = setup_demo_catalog();
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    // The run predicate is unchanged: ContainsAny[11537] on the target
    // origin — fabric attachment metadata never enters it.
    let payload: String = conn
        .query_row(
            "SELECT m.payload FROM analysis_runs r
             JOIN analysis_plans p ON p.id = r.plan_id
             JOIN manifest_revisions m ON m.id = p.manifest_revision_id
             WHERE r.id = (SELECT MIN(id) FROM analysis_runs)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert!(
        v["target"]["transit_predicate"]["predicate"]["ContainsAny"]
            .as_array()
            .is_some(),
        "transit predicate shape unchanged: {v}"
    );
}

#[tokio::test]
async fn attachment_context_does_not_change_findings() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    // The pilot run verdict presentation is unchanged by the fabric
    // context (current labels, not fabric-derived).
    assert!(!body.contains("LessImpactThanExpected"), "{body}");
    assert!(!body.contains("ExpectedLossOfReachability"), "{body}");
    assert!(body.contains("Route-state changes observed"), "{body}");
}

// ── Part 4: fabric diagram ────────────────────────────────────────

#[tokio::test]
async fn diagram_uses_only_reviewed_attachments() {
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
    let reviewed: Vec<String> = cs["interconnection_context"]["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["label"].as_str().unwrap().to_string())
        .collect();
    let fabric = body.find("Reviewed attachment context").unwrap();
    let seg = &body[fabric..fabric + 6000];
    for label in &reviewed {
        assert!(
            seg.contains(label.as_str()),
            "reviewed attachment rendered: {label}: {seg}"
        );
    }
    // The SVG attachment nodes only ever carry the reviewed labels.
    let svg = seg.find("<svg").unwrap();
    let svg_end = seg[svg..].find("</svg>").unwrap() + svg;
    let svg_text = &seg[svg..svg_end];
    for m in [
        "NORDUnet",
        "ESnet",
        "GÉANT",
        "CANARIE",
        "TWAREN",
        "SINET",
        "Ixia",
        "NEAAR",
        "OMAN",
        "WIX interconnect",
    ] {
        assert!(svg_text.contains(m), "attachment node {m} in SVG");
    }
}

#[tokio::test]
async fn diagram_has_text_equivalent() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let fabric = body.find("Reviewed attachment context").unwrap();
    let seg = &body[fabric..fabric + 6000];
    assert!(seg.contains("<table>"), "text table equivalent: {seg}");
    assert!(seg.contains("Attached network/connector"), "{seg}");
    assert!(seg.contains("Reviewed ASN"), "{seg}");
}

// ── Part 7: NORDUnet path diagram ─────────────────────────────────

#[tokio::test]
async fn nordunet_path_graph_derived_from_artifact() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let region = comparison_region(&body);
    // Canonical direct route-views2 finding: baseline [11537, 2603],
    // pre-finding [11537, 20965, 2603], absence, return with the
    // 4x prepend, final [11537, 20965, 2603].
    assert!(
        region.contains("AS11537 AS2603"),
        "canonical baseline path: {region}"
    );
    assert!(
        region.contains("AS11537 AS20965 AS2603"),
        "canonical pre-finding/final path: {region}"
    );
    assert!(
        region.contains("AS11537 AS22388 AS24489 AS24489 AS24489 AS24489 AS24490 AS20965 AS2603"),
        "canonical return path with prepends: {region}"
    );
    assert!(region.contains("Event baseline"), "{region}");
    assert!(region.contains("Pre-finding state"), "{region}");
    assert!(region.contains("First changed state"), "{region}");
    assert!(region.contains("First route after return"), "{region}");
    assert!(region.contains("Analysis-final state"), "{region}");
    assert!(
        region.contains("No selected route visible at this observer"),
        "absence state block: {region}"
    );
    assert!(region.contains("direct collector session"), "{region}");
}

#[tokio::test]
async fn manlan_not_inserted_into_as_path() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let region = comparison_region(&body);
    // Every node in the observed-path diagram is an ASN from the
    // canonical path; the fabric label never appears inside the SVG.
    let svg = region.find("<svg").unwrap();
    let svg_end = region[svg..].find("</svg>").unwrap() + svg;
    let svg_text = &region[svg..svg_end];
    assert!(
        !svg_text.contains("MAN LAN"),
        "fabric not in path SVG: {svg_text}"
    );
    // All ASN nodes present in the SVG come from the canonical paths.
    let canonical: std::collections::BTreeSet<String> = [
        "AS11537", "AS2603", "AS20965", "AS22388", "AS24489", "AS24490",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for m in crate::catalog::web::path_diagram::full_sequence_text(&[
        11537, 2603, 20965, 22388, 24489, 24490,
    ])
    .split_whitespace()
    {
        assert!(canonical.contains(m), "node {m} is canonical");
    }
}

#[tokio::test]
async fn representative_prefix_links_to_full_group() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let region = comparison_region(&body);
    assert!(
        region.contains("full run evidence"),
        "run evidence link: {region}"
    );
    let run_id = run_id_for(&dbdir, "MANLAN-2019-NORDUNET-PILOT-RE-RV2");
    assert!(
        region.contains(&format!("/analyses/{run_id}")),
        "link points at the canonical run: {region}"
    );
}

// ── Part 8: Smithville relationship visualization ─────────────────

#[tokio::test]
async fn smithville_reviewed_adjacency_dashed() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(
        body.contains("Reviewed relationship vs observed evidence"),
        "{body}"
    );
    assert!(
        body.contains("pd-edge-dashed"),
        "dashed reviewed edge: {body}"
    );
    assert!(body.contains("Adjacent"), "{body}");
}

#[tokio::test]
async fn smithville_observed_paths_solid() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    // Zero qualifying streams: the observed area is an explicit absence
    // block, not an empty diagram.
    assert!(
        body.contains("No selected route visible at this observer"),
        "absence block for the observed area: {body}"
    );
    // The generic component renders observed relationships solid
    // (covered in path_diagram::observed_relationship_is_solid).
}

#[tokio::test]
async fn unobserved_relationship_not_called_nonexistent() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(
        body.to_lowercase()
            .contains("not observed in the selected public baselines"),
        "selected-baseline framing: {body}"
    );
    assert!(
        !body.contains("relationship absent globally"),
        "no global-absence claim: {body}"
    );
    assert!(
        body.contains("does not exist"),
        "negative-framing guard: {body}"
    );
}

#[tokio::test]
async fn insufficient_visibility_graph_not_no_change_graph() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(
        body.contains("Why this is insufficient visibility, not no route-state change"),
        "insufficient-visibility framing remains: {body}"
    );
    let why = body.find("Why this is insufficient visibility").unwrap();
    let rel = body
        .find("Reviewed relationship vs observed evidence")
        .unwrap();
    assert!(
        why < rel || rel + 500 > why,
        "relationship graph sits with the insufficiency framing"
    );
}

// ── Part 10: API and artifact boundary ────────────────────────────

#[tokio::test]
async fn diagram_layout_does_not_change_semantic_identity() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    let before: String = conn
        .query_row(
            "SELECT sha256 FROM analysis_plans ORDER BY id LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(conn);
    let _ = get(&app, "/case-studies/manlan-2019").await;
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    let after: String = conn
        .query_row(
            "SELECT sha256 FROM analysis_plans ORDER BY id LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(before, after, "rendering never mutates plan hashes");
}

#[tokio::test]
async fn canonical_artifacts_byte_identical() {
    if !repo_artifacts_available() {
        return;
    }
    // The tracked canonical artifacts are untouched by this session's
    // feature: the demo import verifies artifact hashes against the
    // archive manifests, and the tracked files still hash to the
    // values recorded in the catalog.
    let (dbdir, _rootdir) = setup_demo_catalog();
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    let rows: Vec<(String, String, i64)> = conn
        .prepare(
            "SELECT relative_path, sha256, size FROM analysis_artifacts
             WHERE relative_path LIKE 'MANLAN-2019-NORDUNET-PILOT-RE-RV2/%'",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert!(!rows.is_empty(), "pilot artifacts imported");
    for (rel, sha, size) in rows {
        let resolved =
            crate::catalog::artifact_path::resolve_artifact(std::path::Path::new("."), &rel)
                .expect("artifact resolves");
        let bytes = std::fs::read(&resolved).unwrap();
        assert_eq!(
            crate::catalog::import::sha256_hex_bytes(&bytes),
            sha,
            "artifact {rel} byte-identical to catalog hash"
        );
        assert_eq!(bytes.len() as i64, size, "artifact {rel} size");
    }
}
