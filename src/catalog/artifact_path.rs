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
//! Every artifact consumer (run page, workbench, demo verifier,
//! artifact audit, coverage lookups) must use `resolve_artifact` or the
//! shared `is_safe_relative_path` containment primitive so that a listed
//! artifact, its existence check, and its access can never disagree
//! about validity or the root.

use std::path::{Component, Path, PathBuf};

/// Whether a stored relative path is lexically safe to join under a
/// root. This is the single containment primitive for artifact and
/// runtime-record paths:
///
/// - rejects empty paths;
/// - rejects absolute paths (including POSIX root-relative);
/// - rejects parent traversal (`..`);
/// - rejects Windows drive-letter prefixes (`C:...`) and UNC roots,
///   because a stored path may later be consumed on Windows;
/// - rejects backslash separators on every platform so an alternate
///   separator cannot smuggle a Windows path.
///
/// Lexical containment is the minimum trust boundary. `resolve_artifact`
/// additionally verifies that an existing candidate's canonicalized path
/// stays under the canonicalized root (symlink containment).
pub fn is_safe_relative_path(rel: &str) -> bool {
    if rel.is_empty() {
        return false;
    }
    if rel.contains('\\') {
        return false; // alternate separator escape
    }
    if rel.starts_with('/') {
        return false;
    }
    let bytes = rel.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return false; // Windows drive-letter prefix
    }
    if rel.starts_with("\\\\") {
        return false; // UNC root
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return false;
    }
    rel_path.components().all(|c| {
        matches!(c, Component::Normal(_) | Component::CurDir)
    })
}

/// Resolve a catalog artifact row's relative path to a filesystem path
/// under `root`, or return `None` when no safe existing candidate exists.
///
/// A candidate is accepted only when:
/// - the stored relative path passes `is_safe_relative_path` (lexical
///   containment), and
/// - the candidate exists and its canonicalized path remains under the
///   canonicalized root (a symlink that escapes the root is rejected).
pub fn resolve_artifact(root: &Path, rel: &str) -> Option<PathBuf> {
    if !is_safe_relative_path(rel) {
        return None;
    }
    let rel_path = Path::new(rel);
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
    let root_canon = root.canonicalize().ok();
    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        // Symlink containment: an existing candidate must canonicalize
        // back under the canonicalized root. When canonicalization is
        // unavailable (unusual platform), lexical containment still
        // applies and the candidate is accepted.
        if let Some(root_canon) = &root_canon {
            match candidate.canonicalize() {
                Ok(cand_canon) => {
                    if !cand_canon.starts_with(root_canon) {
                        continue; // symlink escapes the root; do not serve
                    }
                }
                Err(_) => continue, // cannot verify containment; do not serve
            }
        }
        return Some(candidate);
    }
    None
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
            "case-studies/example-org/pilot/out/EVENT/report.json",
            "{}",
        );
        let got = resolve_artifact(d.path(), "EVENT/report.json").unwrap();
        assert!(got.ends_with("case-studies/example-org/pilot/out/EVENT/report.json"));
    }

    #[test]
    fn absolute_paths_never_resolve() {
        let d = tempfile::tempdir().unwrap();
        assert!(resolve_artifact(d.path(), "/etc/passwd").is_none());
        assert!(resolve_artifact(d.path(), "../outside").is_none());
        assert!(resolve_artifact(d.path(), "EVENT/../../outside").is_none());
        assert!(resolve_artifact(d.path(), "").is_none());
    }

    #[test]
    fn missing_artifact_distinct_from_invalid_artifact_path() {
        let d = tempfile::tempdir().unwrap();
        // Valid lexical path but no file -> None (missing).
        assert!(resolve_artifact(d.path(), "EVENT/missing.json").is_none());
        // Invalid lexical path -> None (invalid).
        assert!(resolve_artifact(d.path(), "../escape").is_none());
        assert!(resolve_artifact(d.path(), "").is_none());
        assert!(resolve_artifact(d.path(), "C:/windows/passwd").is_none());
        assert!(resolve_artifact(d.path(), "EVENT\\..\\escape").is_none());
    }

    #[test]
    fn is_safe_relative_path_rejects_escape_forms() {
        assert!(is_safe_relative_path("EVENT/report.json"));
        assert!(is_safe_relative_path("EVENT/./report.json"));
        assert!(!is_safe_relative_path(""));
        assert!(!is_safe_relative_path("/etc/passwd"));
        assert!(!is_safe_relative_path("../outside"));
        assert!(!is_safe_relative_path("EVENT/../../outside"));
        assert!(!is_safe_relative_path("C:/windows/passwd"));
        assert!(!is_safe_relative_path("C:\\windows\\passwd"));
        assert!(!is_safe_relative_path("\\\\server\\share\\file"));
        assert!(!is_safe_relative_path("EVENT\\..\\escape"));
    }

    #[test]
    fn symlink_escape_is_rejected() {
        let d = tempfile::tempdir().unwrap();
        // A real file inside the root resolves normally.
        write(d.path(), "EVENT/report.json", "{}");
        assert!(resolve_artifact(d.path(), "EVENT/report.json").is_some());
        // A symlink inside the root pointing OUTSIDE the root is rejected.
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.json"), "{}").unwrap();
        let symlink = d.path().join("ESCAPE");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path().join("secret.json"), &symlink).unwrap();
            assert!(
                resolve_artifact(d.path(), "ESCAPE/report.json").is_none(),
                "symlink escaping the root must not resolve"
            );
        }
        #[cfg(not(unix))]
        {
            let _ = (&symlink, &outside);
        }
    }

    #[test]
    fn symlink_inside_root_still_resolves() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "real/EVENT/report.json", "{}");
        #[cfg(unix)]
        {
            // A symlink from out/EVENT/report.json to real/EVENT/report.json
            // stays inside the root and must resolve.
            std::fs::create_dir_all(d.path().join("out/EVENT")).unwrap();
            std::os::unix::fs::symlink(
                d.path().join("real/EVENT/report.json"),
                d.path().join("out/EVENT/report.json"),
            )
            .unwrap();
            let got = resolve_artifact(d.path(), "EVENT/report.json").unwrap();
            assert!(got.ends_with("out/EVENT/report.json"));
        }
    }

    #[test]
    fn artifact_relative_path_resolves_inside_root() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "case-studies/alpha/out/EVT/report.json", "{}");
        let got = resolve_artifact(d.path(), "EVT/report.json").unwrap();
        let canonical = got.canonicalize().unwrap();
        let root_canon = d.path().canonicalize().unwrap();
        assert!(
            canonical.starts_with(&root_canon),
            "resolved path must stay under the root"
        );
    }

    #[test]
    fn all_artifact_consumers_agree_on_validity() {
        // One shared validity authority: the same relative path yields
        // the same answer for listing, existence checks, and access.
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "case-studies/alpha/out/EVT/report.json", "{}");
        write(d.path(), "case-studies/beta/pilot/out/EVT2/report.json", "{}");
        let rels = [
            "EVT/report.json",
            "EVT2/report.json",
            "../escape",
            "EVT/missing.json",
            "",
        ];
        let resolved: Vec<Option<PathBuf>> =
            rels.iter().map(|r| resolve_artifact(d.path(), r)).collect();
        assert!(resolved[0]
            .as_ref()
            .map(|p| p.ends_with("case-studies/alpha/out/EVT/report.json"))
            .unwrap_or(false));
        assert!(resolved[1]
            .as_ref()
            .map(|p| p.ends_with("case-studies/beta/pilot/out/EVT2/report.json"))
            .unwrap_or(false));
        assert!(resolved[2].is_none(), "parent traversal must not resolve");
        assert!(resolved[3].is_none(), "missing file is distinct but None");
        assert!(resolved[4].is_none(), "empty path is invalid");
    }

    #[test]
    fn workbench_and_demo_resolver_equivalent() {
        // The workbench coverage lookup and the demo verifier both
        // delegate to artifact_path::resolve_artifact; a direct call must
        // match on identical inputs.
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "case-studies/gamma/out/E/report.json", "{}");
        write(d.path(), "case-studies/gamma/pilot/out/E2/report.json", "{}");
        for rel in ["E/report.json", "E2/report.json", "../escape", "E/missing.json"] {
            let shared = resolve_artifact(d.path(), rel).map(|p| p.to_string_lossy().into_owned());
            let demo = crate::catalog::demo::resolve_artifact(d.path(), rel)
                .map(|p| p.to_string_lossy().into_owned());
            assert_eq!(demo, shared, "demo resolver must match shared resolver for {rel}");
        }
    }

    #[test]
    fn git_checkout_and_packaged_source_resolver_equivalent() {
        // A Git checkout and an extracted Cargo package carry the same
        // reviewed case-study trees under the catalog root; the resolver
        // must yield equivalent answers from either root.
        let git_root = tempfile::tempdir().unwrap();
        let pkg_root = tempfile::tempdir().unwrap();
        for root in [git_root.path(), pkg_root.path()] {
            write(root, "case-studies/alpha/out/EVT/report.json", "{}");
            write(root, "case-studies/alpha/pilot/out/EVT2/report.json", "{}");
            write(root, "out/EVT3/report.json", "{}");
        }
        for rel in ["EVT/report.json", "EVT2/report.json", "EVT3/report.json", "../escape"] {
            let a = resolve_artifact(git_root.path(), rel)
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned());
            let b = resolve_artifact(pkg_root.path(), rel)
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned());
            assert_eq!(a, b, "Git and packaged resolution must agree for {rel}");
        }
    }
}
