//! Impact wave types — temporally coherent groups of route transitions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::route::Prefix;

/// A SEQUITUR-derived motif describing the structural shape of a wave.
///
/// Motif identity is a deterministic FNV-1a 64-bit hash of the fully
/// expanded terminal sequence — portable across runs, unlike rule numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveMotif {
    /// Deterministic identity: FNV-1a 64-bit hex hash of the fully
    /// expanded terminal sequence.
    pub id: String,
    /// The fully expanded terminal sequence (space-separated symbols).
    pub expanded: String,
    /// Hierarchical rule representation: lines like "R2 → PATH_CHANGE WITHDRAWAL"
    /// plus the root line. Rule numbers are for readability only; do not
    /// rely on them across runs.
    pub structure: Vec<String>,
    /// How many times this motif occurred across grouped sequences.
    pub occurrences: usize,
    /// How many terminal symbols this motif covers across all occurrences.
    pub covered_terminals: usize,
    /// Total terminal symbols in the wave.
    pub total_terminals: usize,
    /// Sequence scopes in which the motif appeared (e.g. "rv2:185.1.8.65 192.0.2.0/24").
    pub scopes: Vec<String>,
    /// Evidence ranges for representative occurrences.
    pub evidence_ranges: Vec<MotifEvidenceRange>,
}

/// Evidence range for one occurrence of a motif.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotifEvidenceRange {
    /// Observer identifier (e.g. "route-views2:185.1.8.65").
    pub observer: String,
    /// BGP prefix.
    pub prefix: String,
    /// Start timestamp of the covered transitions.
    pub time_start: DateTime<Utc>,
    /// End timestamp of the covered transitions.
    pub time_end: DateTime<Utc>,
    /// Start transition index within the wave.
    pub transition_start: usize,
    /// End transition index within the wave.
    pub transition_end: usize,
}

/// An observed routing-impact wave: a temporally concentrated set of
/// related route transitions across multiple observations.
///
/// This describes the structural shape of an event, not literal
/// propagation delay through the Internet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactWave {
    /// Sequential wave identifier, assigned by the detector.
    pub id: usize,
    /// Human-readable label (e.g. "Primary-path detachment").
    pub label: String,
    /// Start of the wave.
    pub start: DateTime<Utc>,
    /// Peak activity timestamp.
    pub peak: DateTime<Utc>,
    /// End of the wave.
    pub end: DateTime<Utc>,
    /// Prefixes affected during this wave.
    pub affected_prefixes: Vec<Prefix>,
    /// Observer peers that saw transitions in this wave.
    pub affected_peers: Vec<String>,
    /// Optional SEQUITUR-derived motif describing the structure.
    pub motif: Option<WaveMotif>,
}

impl ImpactWave {
    pub fn new(
        label: &str,
        start: DateTime<Utc>,
        peak: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Self {
        ImpactWave {
            id: 0,
            label: label.to_string(),
            start,
            peak,
            end,
            affected_prefixes: vec![],
            affected_peers: vec![],
            motif: None,
        }
    }
}

// ── FNV-1a 64-bit hash ────────────────────────────────────────────

/// Compute the FNV-1a 64-bit hash of a byte sequence.
/// Returns a 16-character lowercase hex string.
pub fn fnv1a_64(data: &[u8]) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(s: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0).unwrap() + chrono::Duration::seconds(s)
    }

    #[test]
    fn impact_wave_construction() {
        let wave = ImpactWave::new("Detachment", t(0), t(10), t(30));
        assert_eq!(wave.label, "Detachment");
        assert!(wave.affected_prefixes.is_empty());
        assert!(wave.affected_peers.is_empty());
        assert!(wave.motif.is_none());
    }

    #[test]
    fn impact_wave_with_motif() {
        let mut wave = ImpactWave::new("Detachment", t(0), t(10), t(30));
        wave.affected_prefixes.push(Prefix::from("192.0.2.0/24"));
        wave.affected_peers.push("rv2:AS6447".into());
        wave.motif = Some(WaveMotif {
            id: "abc123".into(),
            expanded: "PATH_CHANGE RETURN_TO_BASELINE".into(),
            structure: vec!["R0 → PATH_CHANGE RETURN_TO_BASELINE".into()],
            occurrences: 2,
            covered_terminals: 4,
            total_terminals: 4,
            scopes: vec!["rv2:185.1.8.65 192.0.2.0/24".into()],
            evidence_ranges: vec![MotifEvidenceRange {
                observer: "rv2:185.1.8.65".into(),
                prefix: "192.0.2.0/24".into(),
                time_start: t(0),
                time_end: t(10),
                transition_start: 0,
                transition_end: 2,
            }],
        });
        assert!(wave.motif.is_some());
    }

    #[test]
    fn impact_wave_serialization_roundtrip() {
        let mut wave = ImpactWave::new("Detachment", t(0), t(10), t(30));
        wave.affected_prefixes.push(Prefix::from("192.0.2.0/24"));
        wave.motif = Some(WaveMotif {
            id: "deadbeef".into(),
            expanded: "PATH_CHANGE".into(),
            structure: vec!["ROOT → PATH_CHANGE".into()],
            occurrences: 1,
            covered_terminals: 1,
            total_terminals: 1,
            scopes: vec![],
            evidence_ranges: vec![],
        });
        let json = serde_json::to_string(&wave).unwrap();
        let parsed: ImpactWave = serde_json::from_str(&json).unwrap();
        assert_eq!(wave, parsed);
    }

    #[test]
    fn fnv1a_empty() {
        assert_eq!(fnv1a_64(b""), "cbf29ce484222325");
    }

    #[test]
    fn fnv1a_hello() {
        // Known FNV-1a test vector
        assert_eq!(fnv1a_64(b"hello"), "a430d84680aabd0b");
    }
}
