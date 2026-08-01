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
