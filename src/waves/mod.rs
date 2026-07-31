//! Waves module — grouping related transitions into temporally coherent
//! event waves.
//!
//! TODO: Implement impact-wave detection: temporally concentrated groups
//! of similar route transitions across RouteViews observations.

use crate::domain::wave::ImpactWave;

/// Detect impact waves from a set of route transitions.
pub fn detect_waves() -> Vec<ImpactWave> {
    // TODO: implement
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_detect_waves_returns_empty() {
        let waves = detect_waves();
        assert!(waves.is_empty());
    }
}
