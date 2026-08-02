//! Repository truth audit tests — keep the 2026-08 audit inventory in
//! agreement with the tracked repository.
//!
//! The inventory (docs/audits/repository-inventory.json) classifies every
//! tracked file. These tests enforce the audit contract:
//!
//! * every tracked file has exactly one audit category;
//! * the inventory contains no untracked runtime files (cache, out, data,
//!   tmp material that is not tracked);
//! * generated and authored files are distinct categories;
//! * historical records are never labeled current normative documentation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> String {
    std::fs::read_to_string(manifest_dir().join(path)).unwrap_or_else(|e| {
        panic!("cannot read {path}: {e}");
    })
}

fn tracked_files() -> BTreeSet<String> {
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

#[derive(Clone)]
struct AuditEntry {
    path: String,
    category: String,
    audience: String,
    authoritative: String,
    generated: bool,
    current: bool,
}

fn inventory() -> BTreeMap<String, AuditEntry> {
    let raw: serde_json::Value =
        serde_json::from_str(&read("docs/audits/repository-inventory.json"))
            .expect("inventory JSON parses");
    let arr = raw.as_array().expect("inventory is an array");
    let mut map = BTreeMap::new();
    for v in arr {
        let entry = AuditEntry {
            path: v["path"].as_str().expect("path").to_string(),
            category: v["category"].as_str().expect("category").to_string(),
            audience: v["audience"].as_str().expect("audience").to_string(),
            authoritative: v["authoritative"]
                .as_str()
                .expect("authoritative")
                .to_string(),
            generated: v["generated"].as_bool().expect("generated"),
            current: v["current"].as_bool().expect("current"),
        };
        assert!(
            map.insert(entry.path.clone(), entry).is_none(),
            "duplicate inventory path"
        );
    }
    map
}

const CATEGORIES: &[&str] = &[
    "Production source",
    "Test source",
    "Template or stylesheet",
    "Script or developer tool",
    "Normative current documentation",
    "Historical decision record",
    "Reviewed case-study interpretation",
    "Immutable or generated evidence",
    "Test fixture",
    "Configuration",
    "Packaging or release metadata",
    "License or third-party notice",
    "GitHub/community metadata",
];

/// Categories whose contents are authored (reviewed) rather than generated
/// by a pipeline.
const AUTHORED_CATEGORIES: &[&str] = &[
    "Production source",
    "Test source",
    "Template or stylesheet",
    "Script or developer tool",
    "Normative current documentation",
    "Historical decision record",
    "Reviewed case-study interpretation",
    "Test fixture",
    "Configuration",
    "Packaging or release metadata",
    "License or third-party notice",
    "GitHub/community metadata",
];

const GENERATED_CATEGORIES: &[&str] = &["Immutable or generated evidence"];

#[test]
fn every_tracked_file_has_audit_category() {
    let tracked = tracked_files();
    let inv = inventory();
    for path in &tracked {
        let entry = inv.get(path).unwrap_or_else(|| {
            panic!("tracked file missing from audit inventory: {path}");
        });
        assert!(
            CATEGORIES.contains(&entry.category.as_str()),
            "{}: unknown category {}",
            path,
            entry.category
        );
        assert!(!entry.audience.is_empty(), "{path}: empty audience");
        assert!(
            !entry.authoritative.is_empty(),
            "{path}: empty authoritative source"
        );
    }
}

#[test]
fn audit_contains_no_untracked_runtime_files() {
    // The inventory may not name files git does not track: cache/, out/,
    // data/, tmp/ runtime material must never be listed as audited.
    let tracked = tracked_files();
    let inv = inventory();
    for path in inv.keys() {
        assert!(
            tracked.contains(path),
            "inventory names a file git does not track: {path}"
        );
    }
}

#[test]
fn generated_and_authored_files_are_distinct() {
    let inv = inventory();
    for (path, entry) in &inv {
        if AUTHORED_CATEGORIES.contains(&entry.category.as_str()) {
            assert!(
                !entry.generated,
                "{path}: authored category {category} marked generated",
                category = entry.category
            );
        }
        if GENERATED_CATEGORIES.contains(&entry.category.as_str()) {
            assert!(
                entry.generated,
                "{path}: generated category {category} marked authored",
                category = entry.category
            );
        }
    }
}

#[test]
fn historical_records_are_not_labeled_current_normative_docs() {
    let inv = inventory();
    for (path, entry) in &inv {
        match entry.category.as_str() {
            "Historical decision record" => {
                assert!(
                    !entry.current,
                    "{path}: historical decision record marked current"
                );
            }
            "Normative current documentation" => {
                assert!(
                    entry.current,
                    "{path}: normative current documentation marked historical"
                );
            }
            _ => {}
        }
    }
}
