//! Artifact path resolution — the single authority for locating a
//! catalog artifact row on the filesystem.
//!
//! Catalog artifact rows store a path relative to the import output
//! root that produced them (for example `INC0301970/report.json`
//! relative to `case-studies/indiana-gigapop-smithville-2026/out/`).
//! Resolution therefore searches the conventional output roots under a
//! catalog root:
//!
//! 1. `<root>/<rel>` — conventional catalog-root-relative storage
//! 2. `<root>/out/<rel>` — direct `analyze` output
//! 3. `<root>/case-studies/<slug>/out/<rel>` and
//!    `<root>/case-studies/<slug>/pilot/out/<rel>` — reviewed
//!    case-study evidence trees (Git checkout and packaged source both
//!    carry these trees)
//!
//! The same resolver is used by the demo verifier, the web run page,
//! and any artifact-serving path, so a listed artifact and its
//! existence check can never disagree about the root.

use std::path::{Path, PathBuf};

/// Resolve a catalog artifact row's relative path to a filesystem path
/// under `root`, or return `None` when no candidate exists.
pub fn resolve_artifact(root: &Path, rel: &str) -> Option<PathBuf> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return None; // absolute artifact paths are rejected at import
    }
    let mut candidates: Vec<PathBuf> = vec![root.join(rel_path), root.join("out").join(rel_path)];
    if let Ok(entries) = std::fs::read_dir(root.join("case-studies")) {
        let mut slugs: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
            .collect();
        slugs.sort();
        for slug in slugs {
            candidates.push(
                root.join("case-studies")
                    .join(&slug)
                    .join("out")
                    .join(rel_path),
            );
            candidates.push(
                root.join("case-studies")
                    .join(&slug)
                    .join("pilot")
                    .join("out")
                    .join(rel_path),
            );
        }
    }
    candidates.into_iter().find(|c| c.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    #[test]
    fn resolves_conventional_catalog_root_paths() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "EVENT/report.json", "{}");
        let got = resolve_artifact(d.path(), "EVENT/report.json").unwrap();
        assert_eq!(got, d.path().join("EVENT/report.json"));
    }

    #[test]
    fn resolves_direct_analyze_output_under_out() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "out/EVENT/report.json", "{}");
        let got = resolve_artifact(d.path(), "EVENT/report.json").unwrap();
        assert_eq!(got, d.path().join("out/EVENT/report.json"));
    }

    #[test]
    fn resolves_case_study_evidence_trees_generically() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "case-studies/indiana-gigapop-smithville-2026/out/INC0301970/report.json",
            "{}",
        );
        let got = resolve_artifact(d.path(), "INC0301970/report.json").unwrap();
        assert!(got
            .ends_with("case-studies/indiana-gigapop-smithville-2026/out/INC0301970/report.json"));
    }

    #[test]
    fn resolves_pilot_evidence_trees() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "case-studies/example-org/example-2020/pilot/out/EVENT/report.json",
            "{}",
        );
        let got = resolve_artifact(d.path(), "EVENT/report.json").unwrap();
        assert!(got.ends_with("case-studies/example-org/example-2020/pilot/out/EVENT/report.json"));
    }

    #[test]
    fn absolute_paths_never_resolve() {
        let d = tempfile::tempdir().unwrap();
        assert!(resolve_artifact(d.path(), "/etc/passwd").is_none());
        assert!(resolve_artifact(d.path(), "../outside").is_none());
    }

    #[test]
    fn missing_artifact_resolves_to_none() {
        let d = tempfile::tempdir().unwrap();
        assert!(resolve_artifact(d.path(), "EVENT/missing.json").is_none());
    }

    #[test]
    fn first_candidate_wins_when_roots_conflict() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "EVENT/report.json", "root");
        write(d.path(), "out/EVENT/report.json", "out");
        // Conventional root-relative storage is preferred.
        let got = resolve_artifact(d.path(), "EVENT/report.json").unwrap();
        assert_eq!(std::fs::read_to_string(got).unwrap(), "root");
    }
}
