//! Artifact inventory tests (repository truth audit, Part 8).
//!
//! Every tracked generated artifact under `case-studies/` must:
//! * carry the current schema version (never an archived old-schema
//!   format presented as current);
//! * be part of a complete run artifact set;
//! * contain no absolute local paths;
//! * contain no obsolete terminology in current outputs;
//! * be the only artifact claiming its identity within its run.

use std::collections::BTreeMap;
use std::path::Path;

use inim::schema::{
    EVIDENCE_APPENDIX_SCHEMA_VERSION, LIFECYCLE_ARTIFACT_SCHEMA_VERSION, REPORT_SCHEMA_VERSION,
    SEMANTIC_WAVE_SCHEMA_VERSION, TRANSITIONS_ARTIFACT_SCHEMA_VERSION,
    WITHDRAWAL_AUDIT_SCHEMA_VERSION,
};

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn tracked_files() -> Vec<String> {
    let out = std::process::Command::new("git")
        .arg("ls-files")
        .current_dir(manifest_dir())
        .output()
        .expect("git ls-files must run");
    assert!(out.status.success(), "git ls-files failed");
    String::from_utf8(out.stdout)
        .expect("git ls-files output is UTF-8")
        .lines()
        .map(str::to_string)
        .collect()
}

/// All tracked files under case-studies/ (every case-study tree).
fn case_study_files() -> Vec<String> {
    tracked_files()
        .into_iter()
        .filter(|p| p.starts_with("case-studies/"))
        .collect()
}

/// Run directories: `out/<RUN_ID>/` under a case study.
fn run_dirs() -> BTreeMap<String, Vec<String>> {
    let mut dirs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in case_study_files() {
        if let Some(pos) = p.find("/out/") {
            let rest = &p[pos + 5..];
            let run = rest.split('/').next().unwrap_or("");
            if !run.is_empty() {
                dirs.entry(run.to_string()).or_default().push(p);
            }
        }
    }
    dirs
}

/// Files whose schema_version field must match a current constant.
const SCHEMA_FILES: &[(&str, u32)] = &[
    ("report.json", REPORT_SCHEMA_VERSION),
    ("evidence_appendix.jsonl", EVIDENCE_APPENDIX_SCHEMA_VERSION),
    ("lifecycle.json", LIFECYCLE_ARTIFACT_SCHEMA_VERSION),
    ("semantic_waves.json", SEMANTIC_WAVE_SCHEMA_VERSION),
    ("withdrawal_audit.json", WITHDRAWAL_AUDIT_SCHEMA_VERSION),
    ("transitions.json", TRANSITIONS_ARTIFACT_SCHEMA_VERSION),
];

/// Stale terminology that must never appear in current generated outputs.
const OBSOLETE_OUTPUT_TERMS: &[&str] = &[
    "departed-I2",
    "departed I2",
    "OpenEvent",
    "Internet impact",
    "global impact",
    "outage severity",
    "affected Internet",
    "failover confirmed",
    "traffic restored",
];

fn json_value(path: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(manifest_dir().join(path))
        .unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("cannot parse {path}: {e}"))
}

#[test]
fn generated_artifacts_use_current_schema_versions() {
    for (name, version) in SCHEMA_FILES {
        for p in case_study_files() {
            if !p.ends_with(name) || !p.contains("/out/") {
                continue;
            }
            if name.ends_with(".jsonl") {
                // JSONL artifacts (evidence appendix) carry the schema as
                // a format property, not a per-line field: every line
                // must parse and carry the core evidence fields.
                for (i, line) in std::fs::read_to_string(manifest_dir().join(&p))
                    .unwrap_or_else(|e| panic!("cannot read {p}: {e}"))
                    .lines()
                    .enumerate()
                {
                    if line.trim().is_empty() {
                        continue; // a no-change run may have an empty appendix
                    }
                    let v: serde_json::Value = serde_json::from_str(line)
                        .unwrap_or_else(|e| panic!("{p}:{i}: unparseable JSONL line: {e}"));
                    for field in ["route_key", "phase", "timestamp", "archive_url"] {
                        assert!(
                            v.get(field).is_some(),
                            "{p}:{i}: JSONL line missing {field}"
                        );
                    }
                }
                continue;
            }
            let v = json_value(&p)["schema_version"]
                .as_u64()
                .unwrap_or_else(|| {
                    panic!("{p}: missing schema_version");
                });
            assert_eq!(
                v as u32, *version,
                "{p}: {name} must be schema v{version} (current), not an archived schema"
            );
        }
    }
}

#[test]
fn run_directories_have_complete_artifact_sets() {
    for (run, files) in run_dirs() {
        // Blocked/preflight-only runs legitimately carry only
        // preflight.json + stderr/stdout logs.
        if files.iter().any(|f| f.ends_with("/preflight.json")) {
            assert!(
                files.iter().all(|f| {
                    let name = f.rsplit('/').next().unwrap_or("");
                    name == "preflight.json"
                        || name == "stderr.log"
                        || name == "stdout.json"
                        || name == "archive_manifest.json"
                }),
                "{run}: preflight-only run has unexpected files: {files:?}"
            );
            continue;
        }
        let names: Vec<&str> = files
            .iter()
            .map(|f| f.rsplit('/').next().unwrap_or(""))
            .collect();
        for required in [
            "archive_manifest.json",
            "report.json",
            "report.txt",
            "lifecycle.json",
            "semantic_waves.json",
            "withdrawal_audit.json",
            "limitations.json",
            "evidence_appendix.jsonl",
        ] {
            assert!(
                names.contains(&required),
                "{run}: missing required artifact {required}; files: {names:?}"
            );
        }
        // No stray files: every artifact is part of the known set.
        for n in names {
            assert!(
                [
                    "archive_manifest.json",
                    "report.json",
                    "report.txt",
                    "lifecycle.json",
                    "semantic_waves.json",
                    "withdrawal_audit.json",
                    "limitations.json",
                    "evidence_appendix.jsonl",
                    "transitions.json",
                    "performance.json",
                    "execution_metadata.json",
                    "relationship-audit.json",
                    "stderr.log",
                    "stdout.json",
                ]
                .contains(&n),
                "{run}: unexpected artifact {n}"
            );
        }
    }
}

#[test]
fn artifact_identity_matches_run_directory() {
    for (run, files) in run_dirs() {
        for p in files {
            if p.ends_with("report.json") || p.ends_with("archive_manifest.json") {
                let event_id = json_value(&p)["event_id"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{p}: missing event_id"))
                    .to_string();
                assert_eq!(
                    event_id.as_str(),
                    run,
                    "{p}: artifact event_id {event_id} does not match run directory {run}"
                );
            }
        }
    }
}

#[test]
fn generated_artifacts_contain_no_absolute_local_paths() {
    for p in case_study_files() {
        if !p.contains("/out/") {
            continue;
        }
        if p.ends_with(".json")
            || p.ends_with(".jsonl")
            || p.ends_with(".txt")
            || p.ends_with(".log")
        {
            let text = std::fs::read_to_string(manifest_dir().join(&p))
                .unwrap_or_else(|e| panic!("cannot read {p}: {e}"));
            for marker in ["/home/", "/Users/", "C:\\Users", "/tmp/"] {
                assert!(
                    !text.contains(marker),
                    "{p}: absolute local path marker {marker:?} present"
                );
            }
        }
    }
}

#[test]
fn current_outputs_contain_no_obsolete_terminology() {
    for p in case_study_files() {
        // Reports, appendices, and generated markdown summaries are
        // current outputs; reviewed source documents are not scanned.
        if !p.contains("/out/") {
            continue;
        }
        let text = std::fs::read_to_string(manifest_dir().join(&p))
            .unwrap_or_else(|e| panic!("cannot read {p}: {e}"));
        for term in OBSOLETE_OUTPUT_TERMS {
            assert!(
                !text.contains(term),
                "{p}: obsolete output term {term:?} present"
            );
        }
    }
}

#[test]
fn archived_artifacts_are_not_mistaken_for_current() {
    // If an old-schema artifact set is ever archived under
    // out/<run>/archive/ or out/archive/, the current run directory must
    // not contain a second copy of the same artifact name — archived
    // artifacts are located and labeled as historical, never imported as
    // the current result.
    for (run, files) in run_dirs() {
        let mut names: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for p in &files {
            let name = p.rsplit('/').next().unwrap_or("");
            names.entry(name).or_default().push(p);
        }
        for (name, paths) in &names {
            assert_eq!(
                paths.len(),
                1,
                "{run}: duplicate artifact identity {name}: {paths:?}"
            );
        }
    }
}
