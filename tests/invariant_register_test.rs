//! Invariant-register consistency test (Session 57, Part 13).
//!
//! The maintained invariant register (`docs/design/invariants.md`) is
//! the normative record of invariants and their enforcement status. It
//! declares totals in its "Invariant counts" section. This test parses
//! the register's table rows and verifies the declared arithmetic, so a
//! status edit without a count update fails loudly.

use std::collections::BTreeMap;

fn manifest_dir() -> &'static std::path::Path {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn invariant_register_declared_counts_match_rows() {
    let path = manifest_dir().join("docs/design/invariants.md");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("cannot read {path:?}: {e}");
    });

    // Row pattern: | ID-1 | statement | status | enforcement | falsification |
    let row_re = regex::Regex::new(r"^\| ([A-Z]{2}-[0-9]+) \| .*? \| (.+?) \| .*? \| .*? \|$").unwrap();

    // Declared totals: "- Enforced: N" etc.
    let count_re = regex::Regex::new(r"^- (Enforced|Partially enforced|Assumed|Claimed|Unknown|Total table rows): (\d+)$").unwrap();

    let mut rows = 0usize;
    let mut by_status: BTreeMap<String, usize> = BTreeMap::new();
    let mut declared: BTreeMap<String, usize> = BTreeMap::new();

    for line in text.lines() {
        if let Some(cap) = row_re.captures(line) {
            rows += 1;
            let status = cap[2].trim().to_string();
            // Compound statuses (e.g. "enforced (rows); partially
            // (event row)") count under the leading category.
            let category = if status.starts_with("partially enforced") {
                "partially enforced"
            } else if status.starts_with("enforced") {
                "enforced"
            } else if status.starts_with("assumed") {
                "assumed"
            } else if status.starts_with("claimed") {
                "claimed"
            } else {
                "unknown"
            };
            *by_status.entry(category.to_string()).or_insert(0) += 1;
        }
        if let Some(cap) = count_re.captures(line) {
            let key = cap[1].to_lowercase();
            let n: usize = cap[2].parse().unwrap();
            declared.insert(key, n);
        }
    }

    assert!(rows > 0, "invariant register table must not be empty");
    assert_eq!(
        rows,
        *declared.get("total table rows").unwrap_or(&0),
        "declared total rows must equal table rows"
    );
    for (category, actual) in &by_status {
        let declared_n = declared.get(category.as_str()).copied().unwrap_or(0);
        assert_eq!(
            actual, &declared_n,
            "declared {category} count does not match register rows"
        );
    }
    // Every declared category must be present and the categories with no
    // rows (claimed/unknown) must be declared as zero or absent.
    let sum: usize = by_status.values().sum();
    assert_eq!(sum, rows, "status counts must sum to the row total");
    let declared_sum: usize = declared
        .iter()
        .filter(|(k, _)| *k != "total table rows")
        .map(|(_, v)| v)
        .sum();
    assert_eq!(
        declared_sum, rows,
        "declared status counts must sum to the row total"
    );
}
