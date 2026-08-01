//! Reference-document import (Session 30, Part 3).
//!
//! Immutable external supporting material (e.g. an after-action report) is
//! stored as local catalog data under `<root>/data/documents/<sha12>/` with a
//! catalog-relative path in the database. Identical content deduplicates by
//! SHA-256; changed content creates a new document revision. The import never
//! requires OCR: page count and PDF metadata are extracted best-effort from
//! the raw bytes ("where safely available").

use rusqlite::Connection;

use super::domain::{DocumentRevision, ReferenceDocument};
use super::store;

/// Extension → media type allowlist for importable documents.
pub const MEDIA_TYPE_ALLOWLIST: &[(&str, &str)] = &[
    ("pdf", "application/pdf"),
    ("txt", "text/plain"),
    ("json", "application/json"),
    ("md", "text/markdown"),
    ("csv", "text/csv"),
];

/// Outcome of a document import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentImportOutcome {
    pub document_id: i64,
    pub revision_id: i64,
    pub revision: i64,
    pub sha256: String,
    pub relative_path: String,
    pub media_type: String,
    pub page_count: Option<i64>,
    pub metadata_json: Option<String>,
    /// False when the exact content was already present.
    pub created: bool,
}

/// Hex SHA-256 of bytes.
pub fn hex_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// Media type for a file extension, or None when unsupported.
pub fn media_type_for(path: &std::path::Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    MEDIA_TYPE_ALLOWLIST
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, mt)| *mt)
}

/// Best-effort PDF page count from raw bytes.
///
/// First counts `/Type /Page` occurrences not followed by `s` (i.e.
/// excluding `/Type /Pages`). When the raw-byte scan cannot safely count
/// (e.g. compressed object streams), falls back to `pdfinfo` resolved via
/// PATH; absent or failing `pdfinfo` yields None.
pub fn pdf_page_count(bytes: &[u8]) -> Option<i64> {
    let hay = String::from_utf8_lossy(bytes);
    let mut count = 0i64;
    let mut pos = 0usize;
    while let Some(rel) = hay[pos..].find("/Type /Page") {
        let abs = pos + rel;
        let after = hay[abs + "/Type /Page".len()..].chars().next();
        if after != Some('s') {
            count += 1;
        }
        pos = abs + "/Type /Page".len();
    }
    if count > 0 {
        return Some(count);
    }
    pdfinfo_page_count(bytes)
}

/// Page count via the `pdfinfo` external tool (best-effort, PATH lookup).
fn pdfinfo_page_count(bytes: &[u8]) -> Option<i64> {
    let tmp = std::env::temp_dir().join(format!("inim-pdf-{}.pdf", hex_sha256(bytes).get(..12)?));
    if !tmp.is_file() {
        std::fs::write(&tmp, bytes).ok()?;
    }
    let out = std::process::Command::new("pdfinfo")
        .arg(&tmp)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find(|l| l.starts_with("Pages:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|n| n.parse::<i64>().ok())
        .filter(|n| *n > 0)
}

/// Best-effort PDF Info-dict metadata from raw bytes.
///
/// Scans for `/Title (...)`, `/Author (...)`, `/Creator (...)` pairs as
/// stored in the trailer Info dictionary. None when not safely available.
pub fn pdf_metadata(bytes: &[u8]) -> Option<String> {
    let hay = String::from_utf8_lossy(bytes);
    let mut meta: Vec<(String, String)> = Vec::new();
    for (name, key) in [
        ("Title", "title"),
        ("Author", "author"),
        ("Creator", "creator"),
    ] {
        let needle = format!("/{name} ");
        if let Some(rel) = hay.find(&needle) {
            let rest = &hay[rel + needle.len()..];
            let Some(open) = rest.find('(') else { continue };
            let Some(close) = rest[open + 1..].find(')') else {
                continue;
            };
            let value = rest[open + 1..open + 1 + close].to_string();
            if !value.is_empty() {
                meta.push((key.to_string(), value));
            }
        }
    }
    if meta.is_empty() {
        None
    } else {
        serde_json::to_string(
            &meta
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>(),
        )
        .ok()
    }
}

/// Import a document file into the catalog.
///
/// `root` is the catalog root; the file is stored at
/// `<root>/data/documents/<sha12>/<basename>` and the database records the
/// catalog-relative path. `--file` is copied by basename only, so the stored
/// path can never escape the documents directory.
pub fn import_document(
    conn: &Connection,
    root: &std::path::Path,
    file: &std::path::Path,
    source_url: &str,
    title: Option<&str>,
    doc_type: Option<&str>,
    provenance: Option<&str>,
) -> Result<DocumentImportOutcome, String> {
    let media_type = media_type_for(file)
        .ok_or_else(|| format!("unsupported document type '{}'", file.display()))?;
    let bytes = std::fs::read(file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let sha = hex_sha256(&bytes);
    let base = file
        .file_name()
        .ok_or_else(|| "document file must have a file name".to_string())?
        .to_string_lossy()
        .to_string();
    if base.contains('/') || base.contains('\\') || base.contains("..") {
        return Err("document file name must be a plain basename".to_string());
    }
    let rel = format!("data/documents/{}/{base}", &sha[..12]);
    let dest = root.join(&rel);
    if dest.exists() {
        let existing =
            std::fs::read(&dest).map_err(|e| format!("cannot read existing document: {e}"))?;
        if hex_sha256(&existing) != sha {
            return Err("refusing to overwrite an existing immutable document file".to_string());
        }
    } else {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::copy(file, &dest).map_err(|e| format!("cannot copy document: {e}"))?;
    }

    let title = title.unwrap_or(&base).to_string();
    let doc_type = doc_type.unwrap_or("Reference").to_string();
    let provenance = provenance
        .unwrap_or("imported by administrator via `inim catalog document import`")
        .to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let page_count = (media_type == "application/pdf")
        .then(|| pdf_page_count(&bytes))
        .flatten();
    let metadata_json = (media_type == "application/pdf")
        .then(|| pdf_metadata(&bytes))
        .flatten();

    // Idempotent short-circuit: the exact content is already cataloged.
    let existing: Option<(i64, i64, i64, Option<String>)> = conn
        .query_row(
            "SELECT r.id, r.revision, r.document_id, r.local_path
             FROM document_revisions r WHERE r.sha256 = ?1",
            [&sha],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok();
    if let Some((rev_id, revision, document_id, local_path)) = existing {
        // The content is already cataloged. If the file was only referenced
        // by metadata (local_path NULL), attach it now: this is an
        // availability update, never a content overwrite.
        if local_path.is_none() {
            conn.execute(
                "UPDATE document_revisions SET local_path = ?1 WHERE id = ?2",
                rusqlite::params![rel, rev_id],
            )
            .map_err(|e| format!("catalog write failed: {e}"))?;
        }
        return Ok(DocumentImportOutcome {
            document_id,
            revision_id: rev_id,
            revision,
            sha256: sha,
            relative_path: local_path.unwrap_or(rel),
            media_type: media_type.to_string(),
            page_count,
            metadata_json,
            created: false,
        });
    }

    let doc = ReferenceDocument {
        id: 0,
        title: title.clone(),
        source_url: Some(source_url.to_string()),
        doc_type,
        redistribution_status: "Unknown".to_string(),
        publication_date: None,
        provenance,
        imported_utc: now.clone(),
    };
    let document_id = store::insert_reference_document(conn, &doc)?;
    let next_revision: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM document_revisions WHERE document_id = ?1",
            [document_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;

    let rev = DocumentRevision {
        id: 0,
        document_id,
        revision: next_revision,
        sha256: sha.clone(),
        media_type: media_type.to_string(),
        page_count,
        local_path: Some(rel.clone()),
        metadata_json: metadata_json.clone(),
        imported_utc: now,
    };
    let revision_id = store::insert_document_revision(conn, &rev)?;
    Ok(DocumentImportOutcome {
        document_id,
        revision_id,
        revision: next_revision,
        sha256: sha,
        relative_path: rel,
        media_type: media_type.to_string(),
        page_count,
        metadata_json,
        created: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;

    fn open_temp_db() -> (tempfile::TempDir, rusqlite::Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        (dir, conn)
    }

    /// Minimal synthetic PDF with an uncompressed page object and Info dict.
    fn synthetic_pdf(title: &str) -> Vec<u8> {
        format!(
            "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
             2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
             3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n\
             trailer\n<< /Size 4 /Root 1 0 R /Info 4 0 R >>\nstartxref\n999\n%%EOF\n\
             4 0 obj\n<< /Title ({title}) /Author (Test) >>\nendobj\n"
        )
        .into_bytes()
    }

    fn write_pdf(dir: &std::path::Path, name: &str, title: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, synthetic_pdf(title)).unwrap();
        p
    }

    #[test]
    fn document_import_calculates_sha256() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let file = write_pdf(tmp.path(), "aar.pdf", "AAR");
        let outcome = import_document(
            &conn,
            &root,
            &file,
            "https://example.invalid/aar.pdf",
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(outcome.sha256, hex_sha256(&synthetic_pdf("AAR")));
        assert_eq!(outcome.page_count, Some(1));
        assert!(outcome.created);
        let meta: serde_json::Value =
            serde_json::from_str(outcome.metadata_json.as_deref().unwrap()).unwrap();
        assert_eq!(meta["title"], "AAR");
    }

    #[test]
    fn identical_document_import_is_idempotent() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let file = write_pdf(tmp.path(), "aar.pdf", "AAR");
        let a = import_document(
            &conn,
            &root,
            &file,
            "https://example.invalid/aar.pdf",
            None,
            None,
            None,
        )
        .unwrap();
        let b = import_document(
            &conn,
            &root,
            &file,
            "https://example.invalid/aar.pdf",
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(a.document_id, b.document_id);
        assert_eq!(a.revision_id, b.revision_id);
        assert!(!b.created);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM document_revisions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn changed_document_creates_distinct_record() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let file_a = write_pdf(tmp.path(), "aar.pdf", "AAR v1");
        let a = import_document(
            &conn,
            &root,
            &file_a,
            "https://example.invalid/aar.pdf",
            Some("AAR"),
            None,
            None,
        )
        .unwrap();
        // Different content under the same reviewed title (new revision).
        let file_b = write_pdf(tmp.path(), "aar2.pdf", "AAR v2");
        let b = import_document(
            &conn,
            &root,
            &file_b,
            "https://example.invalid/aar.pdf",
            Some("AAR"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(a.document_id, b.document_id);
        assert_ne!(a.revision_id, b.revision_id);
        assert_eq!(b.revision, 2);
        assert_ne!(b.sha256, a.sha256);
    }

    #[test]
    fn document_path_is_catalog_relative() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let file = write_pdf(tmp.path(), "aar.pdf", "AAR");
        let outcome = import_document(
            &conn,
            &root,
            &file,
            "https://example.invalid/aar.pdf",
            None,
            None,
            None,
        )
        .unwrap();
        let rel = outcome.relative_path;
        assert!(rel.starts_with("data/documents/"), "{rel}");
        assert!(!rel.starts_with('/'), "{rel}");
        assert!(!rel.contains(".."), "{rel}");
        assert!(root.join(&rel).is_file());
    }

    #[test]
    fn unsupported_document_type_fails_cleanly() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let exe = tmp.path().join("tool.exe");
        std::fs::write(&exe, b"MZ...").unwrap();
        let err = import_document(
            &conn,
            &root,
            &exe,
            "https://example.invalid/tool",
            None,
            None,
            None,
        )
        .unwrap_err();
        assert!(err.contains("unsupported document type"), "{err}");
    }

    #[test]
    fn pdf_page_count_excludes_pages_object() {
        let bytes = synthetic_pdf("AAR");
        assert_eq!(pdf_page_count(&bytes), Some(1));
    }

    #[test]
    fn pdf_page_count_falls_back_to_pdfinfo_when_available() {
        if std::process::Command::new("pdfinfo")
            .arg("-v")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            // The real-world AAR uses compressed object streams; the
            // raw-byte scan cannot count them, so the pdfinfo fallback must.
            // Located via its catalog-relative storage path (sha prefix).
            let dir = std::path::Path::new("data/documents/d29df26a2699");
            let real = std::fs::read_dir(dir).ok().and_then(|mut it| {
                it.next()
                    .and_then(|e| e.ok())
                    .and_then(|e| std::fs::read(e.path()).ok())
            });
            if let Some(bytes) = real {
                assert_eq!(pdfinfo_page_count(&bytes), Some(15));
            }
        }
    }

    #[test]
    fn pdf_metadata_is_best_effort() {
        assert_eq!(pdf_metadata(b"%PDF-1.4 no info"), None);
        assert!(pdf_metadata(&synthetic_pdf("AAR")).is_some());
    }
}
