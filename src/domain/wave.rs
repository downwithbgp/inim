//! Impact wave types — temporally coherent groups of route transitions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::route::Prefix;

/// An observed routing-impact wave: a temporally concentrated set of
/// related route transitions across multiple observations.
///
/// This describes the structural shape of an event, not literal
/// propagation delay through the Internet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactWave {
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
    pub motif: Option<String>,
}

impl ImpactWave {
    pub fn new(
        label: &str,
        start: DateTime<Utc>,
        peak: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Self {
        ImpactWave {
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
    fn impact_wave_with_data() {
        let mut wave = ImpactWave::new("Detachment", t(0), t(10), t(30));
        wave.affected_prefixes.push(Prefix::from("192.0.2.0/24"));
        wave.affected_peers.push("rv2:AS6447".into());
        wave.motif = Some("PRIMARY_TO_ALTERNATE".into());
        assert_eq!(wave.affected_prefixes.len(), 1);
        assert_eq!(wave.affected_peers.len(), 1);
    }

    #[test]
    fn impact_wave_serialization_roundtrip() {
        let mut wave = ImpactWave::new("Detachment", t(0), t(10), t(30));
        wave.affected_prefixes.push(Prefix::from("192.0.2.0/24"));
        wave.motif = Some("PRIMARY_TO_ALTERNATE".into());
        let json = serde_json::to_string(&wave).unwrap();
        let parsed: ImpactWave = serde_json::from_str(&json).unwrap();
        assert_eq!(wave, parsed);
    }
}
