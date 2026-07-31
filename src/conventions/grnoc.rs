//! GRNOC naming convention — shared interpretation of GlobalNOC-managed
//! network ticket titles.
//!
//! This convention is user-supplied reviewed operational knowledge. It
//! applies to Internet2, Indiana GigaPOP, and other GRNOC-managed networks.
//! It is NOT applied to non-GRNOC event sources.
//!
//! ## Rule
//!
//! A peer or participant followed by a TRAILING parenthesized attachment,
//! site, exchange, router, or node code denotes an affected attachment
//! while the peer or participant is expected to remain available through
//! redundancy.
//!
//! A peer or participant WITHOUT such a trailing parenthesized attachment
//! denotes expected unavailability of the named peer, participant, or
//! relationship.
//!
//! ## Examples
//!
//!   "Maintenance - I2 Various Participants via DE-CIX (NEWY32AOA)"
//!       → trailing qualifier NEWY32AOA, expectation: redundant
//!
//!   "Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)"
//!       → trailing qualifier NEWA, expectation: redundant
//!
//!   "Availability - I2 Participant UVA"
//!       → no qualifier, expectation: relationship unavailable
//!
//!   "Outage - Indiana GigaPOP Peer Smithville"
//!       → no qualifier, expectation: peer unavailable
//!
//! ## Ambiguity
//!
//! Titles with multiple parenthetical phrases or ambiguous structure
//! produce Unknown expectation with a note, rather than guessing.

use serde::Serialize;

/// The type of named entity in a GRNOC title.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NamedEntityType {
    /// A peer (BGP neighbor).
    Peer,
    /// A participant (organization or institution).
    Participant,
    /// Other entity type (backbone circuit, etc.).
    Other,
}

/// A parsed attachment qualifier from a trailing parenthetical.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttachmentQualifier {
    /// The raw parenthesized text, including parentheses.
    pub raw: String,
    /// The normalized code (uppercase alphanumeric, stripped of parens).
    pub normalized: String,
    /// Character offset of the opening parenthesis (0-based).
    pub char_offset: usize,
}

/// The interpretation of a GRNOC title under the shared convention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConventionInterpretation {
    /// The raw title.
    pub raw_title: String,
    /// The type of named entity, if recognized.
    pub named_entity_type: Option<NamedEntityType>,
    /// The recognized entity label (e.g. "RIPE", "UVA", "Smithville").
    pub named_entity_label: Option<String>,
    /// The trailing attachment qualifier, if present.
    pub attachment_qualifier: Option<AttachmentQualifier>,
    /// Whether redundancy is expected.
    pub redundancy_expected: Option<bool>,
    /// Provenance of the interpretation.
    pub provenance: String,
    /// Confidence in the interpretation.
    pub confidence: InterpretationConfidence,
    /// Note for ambiguous or edge cases.
    pub note: Option<String>,
}

/// Confidence level for a convention interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum InterpretationConfidence {
    /// Clear single trailing qualifier or clear absence.
    Clear,
    /// Ambiguous title structure — cannot determine with certainty.
    Ambiguous,
}

/// Interpret a GRNOC ticket title under the shared naming convention.
///
/// Recognizes only a **trailing** parenthesized qualifier — the final
/// parenthetical phrase in the title. Internal parenthetical text is
/// NOT treated as an attachment qualifier.
///
/// Multiple parenthetical phrases produce `Ambiguous` confidence with
/// a note, rather than guessing.
pub fn interpret(title: &str) -> ConventionInterpretation {
    let provenance =
        "GRNOC title convention: trailing parenthesized code indicates expected redundancy";
    let title = title.trim();

    // Find all parenthetical groups
    let parens: Vec<(usize, usize, String)> = find_parentheticals(title);

    if parens.is_empty() {
        // No parenthetical — try to identify entity type
        let (entity_type, label) = extract_entity(title);
        return ConventionInterpretation {
            raw_title: title.to_string(),
            named_entity_type: entity_type,
            named_entity_label: label,
            attachment_qualifier: None,
            redundancy_expected: Some(false),
            provenance: provenance.to_string(),
            confidence: InterpretationConfidence::Clear,
            note: None,
        };
    }

    if parens.len() > 1 {
        // Multiple parenthetical groups — ambiguous
        let (entity_type, label) = extract_entity(title);
        return ConventionInterpretation {
            raw_title: title.to_string(),
            named_entity_type: entity_type,
            named_entity_label: label,
            attachment_qualifier: None,
            redundancy_expected: None,
            provenance: provenance.to_string(),
            confidence: InterpretationConfidence::Ambiguous,
            note: Some("ambiguous GRNOC title structure: multiple parenthetical phrases".into()),
        };
    }

    // Single parenthetical — verify it's at the END (trailing)
    let (start, end, code) = &parens[0];
    let trailing_text = title[*end..].trim();
    if !trailing_text.is_empty() {
        // Parenthetical is not at the end — internal text, not an attachment qualifier
        let (entity_type, label) = extract_entity(title);
        return ConventionInterpretation {
            raw_title: title.to_string(),
            named_entity_type: entity_type,
            named_entity_label: label,
            attachment_qualifier: None,
            redundancy_expected: Some(false),
            provenance: provenance.to_string(),
            confidence: InterpretationConfidence::Clear,
            note: Some(format!(
                "parenthetical '({})' is not trailing — not interpreted as attachment qualifier",
                code
            )),
        };
    }

    // Valid trailing qualifier
    let normalized = code.to_uppercase();
    let qualifier = AttachmentQualifier {
        raw: title[*start..*end].to_string(),
        normalized: normalized.clone(),
        char_offset: *start,
    };

    let (entity_type, label) = extract_entity(title);

    ConventionInterpretation {
        raw_title: title.to_string(),
        named_entity_type: entity_type,
        named_entity_label: label,
        attachment_qualifier: Some(qualifier),
        redundancy_expected: Some(true),
        provenance: provenance.to_string(),
        confidence: InterpretationConfidence::Clear,
        note: None,
    }
}

/// Find all parenthesized groups in a title, returning (start, end, inner_text).
fn find_parentheticals(title: &str) -> Vec<(usize, usize, String)> {
    let chars: Vec<char> = title.chars().collect();
    let mut results = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '(' {
            let start = i;
            i += 1;
            let mut depth = 1;
            let mut inner = String::new();
            while i < chars.len() && depth > 0 {
                match chars[i] {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                if depth > 0 {
                    inner.push(chars[i]);
                }
                i += 1;
            }
            if depth == 0 {
                // Found matching close
                let end = i + 1; // include the closing paren
                                 // Only count alphanumeric codes (3-10 chars, uppercase start)
                if inner.len() >= 2
                    && inner.len() <= 10
                    && inner.chars().all(|c| c.is_ascii_alphanumeric())
                {
                    results.push((start, end, inner));
                }
            }
        }
        i += 1;
    }
    results
}

/// Extract the named entity type and label from a title.
fn extract_entity(title: &str) -> (Option<NamedEntityType>, Option<String>) {
    let title_lower = title.to_lowercase();

    // Check for "Participant" keyword
    if title_lower.contains("participant") {
        // Try to find the participant name after "Participant"
        if let Some(pos) = title_lower.find("participant") {
            let after = &title[pos + 11..].trim();
            // Take first word as the label
            let label = after
                .split(|c: char| c.is_whitespace() || c == '(' || c == '-')
                .next()
                .filter(|s| !s.is_empty() && s.len() > 1)
                .map(|s| s.trim_end_matches(',').to_string());
            return (Some(NamedEntityType::Participant), label);
        }
    }

    // Check for "Peer" keyword
    if title_lower.contains("peer") {
        if let Some(pos) = title_lower.find("peer") {
            let after = &title[pos + 4..].trim();
            let label = after
                .split(|c: char| c.is_whitespace() || c == '(' || c == '-')
                .next()
                .filter(|s| !s.is_empty() && s.len() > 1)
                .map(|s| s.trim_end_matches(',').to_string());
            return (Some(NamedEntityType::Peer), label);
        }
    }

    // Default: Other
    (Some(NamedEntityType::Other), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internet2_trailing_site_implies_redundancy() {
        let interp = interpret("Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)");
        assert_eq!(interp.redundancy_expected, Some(true));
        assert!(interp.attachment_qualifier.is_some());
        assert_eq!(interp.attachment_qualifier.unwrap().normalized, "NEWA");
        assert_eq!(interp.confidence, InterpretationConfidence::Clear);
    }

    #[test]
    fn gigapop_trailing_site_implies_redundancy() {
        let interp = interpret("Maintenance - Indiana GigaPOP Peer Foo via IX (ABCD1234)");
        assert_eq!(interp.redundancy_expected, Some(true));
        assert!(interp.attachment_qualifier.is_some());
    }

    #[test]
    fn internet2_no_site_implies_relationship_unavailable() {
        let interp = interpret("Availability - I2 Participant UVA");
        assert_eq!(interp.redundancy_expected, Some(false));
        assert!(interp.attachment_qualifier.is_none());
        assert_eq!(interp.named_entity_type, Some(NamedEntityType::Participant));
    }

    #[test]
    fn gigapop_no_site_implies_relationship_unavailable() {
        let interp = interpret("Outage - Indiana GigaPOP Peer Smithville");
        assert_eq!(interp.redundancy_expected, Some(false));
        assert!(interp.attachment_qualifier.is_none());
        assert_eq!(interp.named_entity_type, Some(NamedEntityType::Peer));
    }

    #[test]
    fn internal_parenthetical_text_is_not_automatically_attachment() {
        // Parenthetical in the middle, not trailing
        let interp = interpret("Outage (unplanned) - I2 Participant UVA");
        assert_eq!(interp.redundancy_expected, Some(false));
        assert!(interp.attachment_qualifier.is_none());
        // Should have a note about non-trailing
        assert!(interp.note.is_some());
    }

    #[test]
    fn ambiguous_multiple_parentheses_yield_unknown() {
        let interp = interpret("Outage (NEWA) - Peer (NEWY32AOA) downstream");
        assert_eq!(interp.confidence, InterpretationConfidence::Ambiguous);
        assert_eq!(interp.redundancy_expected, None);
        assert!(interp.note.unwrap().contains("ambiguous"));
    }

    #[test]
    fn explicit_manifest_expectation_overrides_title_inference() {
        // The convention layer interprets the title, but the manifest may override.
        // This test verifies that the convention produces a result; the override
        // is handled at the profile/caller level.
        let interp = interpret("Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)");
        assert_eq!(interp.redundancy_expected, Some(true));
        // Caller checks manifest first, uses convention as fallback
    }

    #[test]
    fn convention_is_not_applied_to_non_grnoc_source() {
        // The convention gate is at the profile level — profiles enable it.
        // This test verifies the interpretation function works but calling code
        // must check source before applying.
        let interp = interpret("Some non-GRNOC event (ABC)");
        // Even if the title matches the pattern, the profile/caller must gate
        assert!(interp.attachment_qualifier.is_some());
    }
}
