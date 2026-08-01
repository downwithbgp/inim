//! Release-readiness tests — validate licensing, packaging policy, and
//! manifest-as-data invariants. These test release behavior, not domain
//! logic (see module-level tests for the domain).

use std::path::Path;

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> String {
    std::fs::read_to_string(manifest_dir().join(path)).unwrap_or_else(|e| {
        panic!("cannot read {path}: {e}");
    })
}

#[test]
fn cargo_manifest_declares_mit() {
    let toml = read("Cargo.toml");
    // The [package] section declares the SPDX license expression.
    assert!(
        toml.contains("license = \"MIT\""),
        "Cargo.toml must declare license = \"MIT\""
    );
    // Not both license and license-file: SPDX expression + root LICENSE text.
    assert!(
        !toml.contains("license-file"),
        "Cargo.toml must not use license-file for the standard MIT license"
    );
    assert!(toml.contains("readme = \"README.md\""));
}

#[test]
fn root_license_matches_standard_mit_template() {
    let license = read("LICENSE");
    // Canonical MIT template markers (whitespace-insensitive).
    assert!(license.contains("MIT License"));
    assert!(
        license.contains("Copyright (c) 2026 Vadim Petrov"),
        "copyright line must name the holder and year"
    );
    assert!(license.contains("Permission is hereby granted, free of charge"));
    assert!(license
        .contains("to use, copy, modify, merge, publish, distribute, sublicense, and/or sell"));
    assert!(license.contains("THE SOFTWARE IS PROVIDED \"AS IS\""));
    assert!(license.contains("NONINFRINGEMENT"));
    // No custom restrictions: standard MIT imposes none of these.
    for restriction in [
        "non-commercial",
        "noncommercial",
        "may not be used to",
        "no redistribution",
        "without prior written consent",
    ] {
        assert!(
            !license.to_lowercase().contains(restriction),
            "LICENSE must not contain the custom restriction {restriction:?}"
        );
    }
}

#[test]
fn package_does_not_include_cache_directory() {
    let toml = read("Cargo.toml");
    let section = package_section(&toml);
    assert!(
        section.contains("\"cache/\"") || section.contains("\"cache\""),
        "package exclude rules must exclude cache/"
    );
}

#[test]
fn package_does_not_include_out_directory() {
    let toml = read("Cargo.toml");
    let section = package_section(&toml);
    assert!(
        section.contains("\"out/\"") || section.contains("\"out\""),
        "package exclude rules must exclude out/"
    );
}

#[test]
fn package_contains_license_and_readme() {
    // LICENSE and README.md exist at the repository root, so Cargo
    // includes them in the package (readme is declared explicitly).
    assert!(manifest_dir().join("LICENSE").is_file());
    assert!(manifest_dir().join("README.md").is_file());
    let toml = read("Cargo.toml");
    assert!(toml.contains("readme = \"README.md\""));
}

#[test]
fn canonical_manifests_remain_data_not_code() {
    // Event subjects and ASN mappings are data: canonical manifests load
    // through the strict loader, carry TransitPredicateMapping, and never
    // reintroduce legacy single-ASN shortcut fields.
    for entry in std::fs::read_dir(manifest_dir().join("manifests")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let m = inim::manifest::Manifest::load(&path)
            .unwrap_or_else(|e| panic!("manifest {} must load canonically: {e}", path.display()));
        // Data only: no legacy shortcut fields survive (structure-level
        // check — analyst notes may legitimately reference the migration).
        assert!(m.target.managed_network_asn.is_none());
        assert!(m.target.internet2_asn.is_none());
        let parsed: serde_json::Value = serde_json::to_value(&m).unwrap();
        let target = &parsed["target"];
        assert!(
            target.get("managed_network_asn").is_none(),
            "serialized target must not contain managed_network_asn"
        );
        assert!(
            target.get("internet2_asn").is_none(),
            "serialized target must not contain internet2_asn"
        );
        // The mapping is either Reviewed-with-predicate or Unresolved.
        m.target
            .transit_predicate
            .validate()
            .unwrap_or_else(|e| panic!("manifest {} predicate invalid: {e}", path.display()));
        assert_eq!(m.schema_version, inim::manifest::MANIFEST_SCHEMA_VERSION);
    }
}

fn package_section(toml: &str) -> String {
    // Return the [package] section up to the first other section header.
    let start = toml.find("[package]").expect("[package] section");
    let rest = &toml[start..];
    let end = rest.find("\n[").unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn readme_case_study_counts_match_current_artifacts() {
    // Skip when the analysis outputs are absent (e.g. packaged-crate
    // verification, which excludes out/).
    let ripe = manifest_dir().join("out/INC0302574/report.json");
    let uva = manifest_dir().join("out/INC0299001/report.json");
    if !ripe.is_file() || !uva.is_file() {
        return;
    }
    let readme = read("README.md");
    let ripe_report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&ripe).unwrap()).unwrap();
    let uva_report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&uva).unwrap()).unwrap();
    let ripe_streams = ripe_report["observed_event_signature"]["observer_scope"]
        ["baseline_observer_prefix_streams"]
        .as_u64()
        .unwrap();
    let uva_streams = uva_report["observed_event_signature"]["observer_scope"]
        ["baseline_observer_prefix_streams"]
        .as_u64()
        .unwrap();
    let uva_withdrawn = uva_report["observed_event_signature"]["stream_lifecycle"]
        ["withdrawn_streams"]
        .as_u64()
        .unwrap();
    let uva_transitions = uva_report["transitions"]["total"].as_u64().unwrap();
    // README case-study counts must match the current artifacts.
    assert!(readme.contains(&format!("{ripe_streams} selected observer-prefix streams")));
    assert!(readme.contains(&format!("{uva_streams} selected observer-prefix streams")));
    assert!(readme.contains(&format!("{uva_withdrawn} temporarily absent")));
    assert!(readme.contains(&format!("{uva_transitions} route-instance transitions")));
}

// ── Case-study packaging + neutrality (Session 30, Part 19) ─────────

/// The crate package must contain the reviewed case-study metadata and no
/// PDF or local document storage.
#[test]
fn case_study_metadata_is_in_package_and_pdf_is_not() {
    if !manifest_dir().join(".git").is_dir() {
        // `cargo package --list` needs VCS metadata; inside an unpacked
        // .crate there is none, so this check is skipped there.
        return;
    }
    let list = package_file_list();
    assert!(
        list.iter()
            .any(|p| p == "case-studies/manlan-2019/case-study.json"),
        "reviewed case-study metadata must be packaged"
    );
    assert!(
        list.iter()
            .any(|p| p == "case-studies/manlan-2019/README.md"),
        "case-study README must be packaged"
    );
    assert!(
        list.iter().all(|p| !p.ends_with(".pdf")),
        "no PDF may enter the crate package (redistribution rights unknown)"
    );
    assert!(
        list.iter().all(|p| !p.starts_with("data/")),
        "local document storage (data/) must be excluded from the package"
    );
    assert!(
        list.iter()
            .all(|p| !p.contains("documents/") || p.starts_with("case-studies/")),
        "reference-document files must not be packaged"
    );
}

/// Production source code must be incident-neutral: MAN LAN-specific names
/// are case-study data and must not appear in `src/`.
#[test]
fn production_source_is_incident_neutral() {
    let forbidden = [
        "MANLAN",
        "CANARIE",
        "NORDUnet",
        "ESnet",
        "EVPN loop",
        "CHG0038258",
    ];
    let mut found: Vec<(String, String)> = Vec::new();
    for entry in walk_rs_files() {
        let content = std::fs::read_to_string(&entry).unwrap_or_default();
        for token in forbidden {
            if content.contains(token) {
                found.push((entry.display().to_string(), token.to_string()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "incident-specific tokens in production source: {found:?}"
    );
}

fn walk_rs_files() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let src = manifest_dir().join("src");
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().map(|x| x == "rs").unwrap_or(false) {
                    out.push(p);
                }
            }
        }
    }
    out
}

fn package_file_list() -> Vec<String> {
    let output = std::process::Command::new("cargo")
        .arg("package")
        .arg("--list")
        .arg("--allow-dirty")
        .output()
        .expect("cargo package --list must run");
    assert!(output.status.success(), "cargo package --list failed");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect()
}

// ── Screenshot harness checks (Session 32, Part 12) ─────────────────

/// The harness must be loopback-only, use the deterministic demo catalog,
/// write gitignored output, and shut the server down on failure.
#[test]
fn screenshot_harness_uses_loopback_and_cleanup() {
    let script = read("scripts/screenshot-review.sh");
    // Loopback binding only.
    assert!(script.contains("127.0.0.1"), "harness must bind loopback");
    assert!(!script.contains("0.0.0.0"), "no wildcard binds");
    // Deterministic demo catalog.
    assert!(
        script.contains("data/inim.sqlite"),
        "harness must use the deterministic demo catalog"
    );
    // Server shutdown on failure (trap + kill).
    assert!(
        script.contains("trap cleanup EXIT"),
        "cleanup trap required"
    );
    assert!(
        script.contains("kill \"$SERVER_PID\""),
        "server kill required"
    );
    // Browser-unavailable message.
    assert!(
        script.contains("browser unavailable"),
        "clear failure message required"
    );
}

/// Screenshot output must be gitignored and excluded from the package.
#[test]
fn screenshot_output_is_gitignored_and_not_packaged() {
    let gitignore = read(".gitignore");
    assert!(
        gitignore.lines().any(|l| l.trim() == "tmp/"),
        "tmp/ must be gitignored"
    );
    let toml = read("Cargo.toml");
    assert!(
        toml.contains("\"tmp/\""),
        "tmp/ must be excluded from the crate package"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Session 35: reviewed service-plane model.
//
// The Internet2 R&E (AS11537) and I2PX (AS11164) identities are REVIEWED
// PROFILE DATA (case-studies/*/pilot/network-profile.json), never control
// flow. The gate below has two parts:
//   1. The I2PX plane identity (`11164`/`i2px`) must not appear ANYWHERE
//      in src/ — production code or tests — because it did not exist before
//      this session and no code path may name it.
//   2. `11537`/`internet2` may appear only in the files that already
//      contained them at Session 35 start (pre-existing doc comments naming
//      the operator, the GRNOC ticket-title source under src/sources and
//      src/profiles, legacy single-plane verdicts in src/assess.rs, legacy
//      manifest migration fixtures, and older test fixtures). The live hit
//      set must EQUAL the frozen set, so any new file referencing the
//      plane identities fails the gate.
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn production_source_contains_no_internet2_specific_plane_branch() {
    let frozen_11537: &[&str] = &[
        "src/assess.rs",
        "src/catalog/batch.rs",
        "src/catalog/target_research.rs",
        "src/cohort.rs",
        "src/compare.rs",
        "src/derived_cache.rs",
        "src/discover.rs",
        "src/domain/observation.rs",
        "src/domain/route.rs",
        "src/fixtures.rs",
        "src/lifecycle.rs",
        "src/main.rs",
        "src/manifest.rs",
        "src/output.rs",
        "src/profiles/internet2.rs",
        "src/profiles/mod.rs",
        "src/routes.rs",
        "src/sources/internet2/ticket.rs",
        "src/target.rs",
        "src/tokenize.rs",
        "src/waves.rs",
    ];
    let frozen_internet2: &[&str] = &[
        "src/assess.rs",
        "src/catalog/discovery.rs",
        "src/catalog/import.rs",
        "src/catalog/relationships.rs",
        "src/catalog/web/tests.rs",
        "src/compare.rs",
        "src/conventions/grnoc.rs",
        "src/domain/assessment.rs",
        "src/domain/entity.rs",
        "src/domain/event.rs",
        "src/domain/expectation.rs",
        "src/main.rs",
        "src/manifest.rs",
        "src/orchestrate.rs",
        "src/profiles/internet2.rs",
        "src/profiles/mod.rs",
        "src/report.rs",
        "src/sequitur/grammar.rs",
        "src/sequitur/mod.rs",
        "src/sources/grnoc.rs",
        "src/sources/internet2/mod.rs",
        "src/sources/internet2/ticket.rs",
        "src/sources/mod.rs",
        "src/tokenize.rs",
    ];

    // 1. The I2PX plane identity is data-only: zero occurrences in src/.
    for token in ["11164", "i2px"] {
        let hits = rs_files_containing(token);
        assert!(
            hits.is_empty(),
            "plane identity {token:?} must not appear in src/ (data-only): {hits:?}"
        );
    }

    // 2. Pre-existing tokens: live hit set must equal the frozen set.
    for (token, frozen) in [("11537", frozen_11537), ("internet2", frozen_internet2)] {
        let mut live: Vec<String> = rs_files_containing(token);
        live.sort();
        let mut expected: Vec<String> = frozen.iter().map(|p| p.to_string()).collect();
        expected.sort();
        assert_eq!(
            live, expected,
            "src/ files referencing {token:?} changed since Session 35 start; \
             new plane references must live in profile data files, not source"
        );
    }
}

/// The reviewed service-plane profile data (not source) declares the two
/// planes with their reviewed ASNs; display labels are presentation data.
#[test]
fn reviewed_service_plane_profile_declares_two_planes() {
    let text = read("case-studies/manlan-2019/pilot/network-profile.json");
    let profile: serde_json::Value =
        serde_json::from_str(&text).expect("network-profile.json must be valid JSON");
    let planes = profile["service_planes"]
        .as_array()
        .expect("service_planes");
    assert_eq!(planes.len(), 2, "exactly two reviewed service planes");
    let re = planes
        .iter()
        .find(|p| p["asns"][0] == 11537)
        .expect("R&E plane with ASN 11537");
    let i2px = planes
        .iter()
        .find(|p| p["asns"][0] == 11164)
        .expect("I2PX plane with ASN 11164");
    assert_eq!(re["id"], "internet2-re");
    assert_eq!(i2px["id"], "internet2-i2px");
    assert_ne!(re["display_label"], i2px["display_label"]);
    // ASN-role entries are data too.
    assert!(profile["asn_roles"]
        .as_array()
        .map(|a| a.len() >= 3)
        .unwrap_or(false));
}

fn rs_files_containing(token: &str) -> Vec<String> {
    let mut out = Vec::new();
    for entry in walk_rs_files() {
        let content = std::fs::read_to_string(&entry).unwrap_or_default();
        if content.to_lowercase().contains(&token.to_lowercase()) {
            let rel = entry
                .strip_prefix(manifest_dir())
                .unwrap_or(&entry)
                .display()
                .to_string();
            out.push(rel);
        }
    }
    out
}

/// The collector-selection report's rejected-reason wording names the
/// exact reviewed predicate (Part 4): rejection states the predicate
/// that produced zero matches, never a blanket visibility claim.
#[test]
fn rejected_collector_reason_names_the_exact_predicate() {
    let text = read("case-studies/manlan-2019/pilot/ris-collector-selection.md");
    let rejected_section = text
        .split("## Rejected collectors and reasons")
        .nth(1)
        .expect("rejected section");
    assert!(
        rejected_section.contains("AS11537-in-path"),
        "rejected reasons must name the exact predicate"
    );
    assert!(
        rejected_section.contains("AS11537"),
        "rejected reasons must name the exact predicate ASN"
    );
    // The selection doc must not claim other collectors lacked visibility
    // beyond the reviewed predicate.
    assert!(
        !rejected_section.contains("had no visibility"),
        "blanket visibility claims are forbidden"
    );
    assert!(
        !rejected_section.contains("no AS2603 visibility"),
        "blanket origin-visibility claims are forbidden"
    );
}
