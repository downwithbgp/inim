//! Integration test: redundant maintenance vertical slice.
//!
//! Tests the full pipeline: ticket → synthetic observations → reconstruction
//! → tokenization → waves → assessment → verdict.
//!
//! No MRT files or network access required.

use chrono::{TimeZone, Utc};

use inim::assess;
use inim::domain::event::EventId;
use inim::domain::expectation::ImpactExpectation;
use inim::domain::assessment::Verdict;
use inim::fixtures;
use inim::routes;
use inim::tokenize;
use inim::waves;

#[test]
fn redundant_maintenance_vertical_slice() {
    let collector = "route-views2";
    let event_start = Utc.with_ymd_and_hms(2025, 6, 15, 1, 0, 0).unwrap();
    let event_end = Utc.with_ymd_and_hms(2025, 6, 15, 6, 0, 0).unwrap();

    // ── Build synthetic observations ───────────────────────────
    let obs = vec![
        // RIB: baseline state for two peers
        fixtures::make_synthetic_rib(
            "192.0.2.0/24", collector, "185.1.8.65", 6447,
            vec![6447, 11537, 1101],
            Utc.with_ymd_and_hms(2025, 6, 15, 0, 50, 0).unwrap(), 0,
        ),
        fixtures::make_synthetic_rib(
            "192.0.2.0/24", collector, "2001:7f8:4::1", 6447,
            vec![6447, 11537, 1101],
            Utc.with_ymd_and_hms(2025, 6, 15, 0, 50, 0).unwrap(), 1,
        ),
        // Event: failover to alternate path
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24", collector, "185.1.8.65", 6447,
            vec![6447, 237, 1101],
            Utc.with_ymd_and_hms(2025, 6, 15, 1, 2, 14).unwrap(), 2,
        ),
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24", collector, "2001:7f8:4::1", 6447,
            vec![6447, 237, 1101],
            Utc.with_ymd_and_hms(2025, 6, 15, 1, 2, 18).unwrap(), 3,
        ),
        // Restoration to baseline
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24", collector, "185.1.8.65", 6447,
            vec![6447, 11537, 1101],
            Utc.with_ymd_and_hms(2025, 6, 15, 5, 51, 44).unwrap(), 4,
        ),
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24", collector, "2001:7f8:4::1", 6447,
            vec![6447, 11537, 1101],
            Utc.with_ymd_and_hms(2025, 6, 15, 5, 53, 11).unwrap(), 5,
        ),
    ];

    // ── Reconstruct ────────────────────────────────────────────
    let (store, changes) = routes::reconstruct_routes(obs, event_start, event_end);

    // ── Tokenize ───────────────────────────────────────────────
    let baseline_map: std::collections::HashMap<_, _> = store
        .all_states()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let transitions = tokenize::tokenize(changes, &baseline_map);

    // ── Waves ──────────────────────────────────────────────────
    let mut detected_waves = waves::detect_waves(&transitions, chrono::Duration::seconds(30));
    waves::summarize_waves(&mut detected_waves);

    // ── Assess ─────────────────────────────────────────────────
    let expectation = ImpactExpectation::redundant(
        Some("NEWY32AOA"),
        "Internet2 title convention",
    );
    let assessment = assess::assess(
        EventId::from("CHG0107955"),
        expectation,
        &transitions,
        detected_waves,
        false,
    );

    // ── Assertions ─────────────────────────────────────────────
    // Verdict: redundant maintenance should produce ExpectedRedundantImpact
    assert_eq!(
        assessment.verdict,
        Verdict::ExpectedRedundantImpact,
        "Expected EXPECTED_REDUNDANT_IMPACT for redundant failover scenario"
    );

    // No withdrawals should be detected
    let has_withdrawals = transitions.iter().any(|t| {
        matches!(t.kind, inim::domain::route::TransitionKind::Withdrawal)
    });
    assert!(
        !has_withdrawals,
        "Redundant scenario must have no withdrawals"
    );

    // At least one wave detected
    assert!(
        !assessment.waves.is_empty(),
        "Should detect at least one impact wave"
    );

    // Evidence must be present
    assert!(
        !assessment.evidence.is_empty(),
        "Assessment must include evidence"
    );

    // Evidence should reference the path changes
    let has_path_change_evidence = assessment.evidence.iter().any(|e| {
        e.description.contains("Path changes")
    });
    assert!(
        has_path_change_evidence,
        "Evidence must mention path changes"
    );
}
