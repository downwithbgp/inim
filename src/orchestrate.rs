//! Analysis orchestration — the real-analysis pipeline.
//!
//! Wires ticket → manifest → broker discovery → cache → ingest →
//! reconstruct → tokenize → waves → assess → outcomes.
//! Live broker execution is deferred; this skeleton provides the
//! structural framework.

use std::path::Path;

use crate::outcome::AnalysisOutcome;

/// Run the real analysis pipeline for a given event and manifest.
///
/// The pipeline order:
/// 1. Parse the ticket fixture (expectation, window)
/// 2. Load the reviewed manifest (UTC window, target, collectors)
/// 3. Per collector: query broker → select RIB + updates → cache
/// 4. RIB preflight: scan for relevant observer-route streams
/// 5. If no relevant streams: stop with InsufficientVisibility
/// 6. Cache and ingest UPDATE files for relevant collectors
/// 7. Reconstruct, tokenize, waves, motifs, assess
/// 8. Produce outputs (reports, manifest, evidence, limitations)
///
/// Live broker execution is deferred; currently returns
/// `Incomplete` with a precise explanation.
pub fn run_real_analysis(
    _event_path: &Path,
    _manifest_path: &Path,
    _cache_dir: &Path,
    _out_dir: &Path,
) -> Result<AnalysisOutcome, String> {
    Err(
        "Real analysis pipeline not yet wired. Broker discovery and \
         live ingest are deferred. Use synthetic demo (omit --manifest)."
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_analysis_returns_incomplete_for_now() {
        let result = run_real_analysis(
            Path::new("event.json"),
            Path::new("manifest.json"),
            Path::new("cache"),
            Path::new("out"),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not yet wired"));
    }
}
