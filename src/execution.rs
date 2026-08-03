//! Shared analysis execution service.
//!
//! One execution path is used by the direct CLI (`inim analyze`), the
//! job worker, and tests. Inputs: immutable plan inputs (event + manifest
//! materialized to files), execution configuration, a progress sink, a
//! cancellation flag, and an artifact staging root. Outputs: a validated
//! staged analysis result. The progress sink and cancellation flag never
//! change deterministic results when not cancelled.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::discover::ArchiveDiscovery;
use crate::orchestrate::{CacheControl, PipelineError};
use crate::outcome::AnalysisOutcome;

/// One stage-boundary progress event. `stage` uses the job-state stage
/// vocabulary (`DiscoveringArchives`, `AcquiringArchives`,
/// `ParsingBaseline`, `FreezingCohort`, `ParsingUpdates`,
/// `ReconstructingRoutes`, `DerivingEvidence`, `RenderingArtifacts`).
/// `current`/`total` are factual counts; when the total is unknown both
/// are None and no percentage may be derived.
#[derive(Debug, Clone)]
pub struct ProgressEvent {
    pub stage: &'static str,
    pub message: String,
    pub current: Option<u64>,
    pub total: Option<u64>,
    pub unit: Option<&'static str>,
}

/// Progress sink abstraction. Implementations: `NoopSink` (direct CLI),
/// `TermSink` (direct CLI), and the worker's database sink.
pub trait ProgressSink: Send + Sync {
    fn emit(&self, ev: &ProgressEvent);
}

/// Discards all events.
pub struct NoopSink;

impl ProgressSink for NoopSink {
    fn emit(&self, _ev: &ProgressEvent) {}
}

/// Prints concise stage lines to stderr (direct CLI behavior).
pub struct TermSink;

impl ProgressSink for TermSink {
    fn emit(&self, ev: &ProgressEvent) {
        let progress = match (ev.current, ev.total) {
            (Some(c), Some(t)) => format!(" {c}/{t} {}", ev.unit.unwrap_or("")),
            (Some(c), None) => format!(" {c} {}", ev.unit.unwrap_or("")),
            _ => String::new(),
        };
        eprintln!("→ {}: {}{}", ev.stage, ev.message, progress);
    }
}

/// Execution configuration shared by the direct CLI and the worker.
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    pub cache_dir: PathBuf,
    pub jobs: usize,
    pub parse_jobs: usize,
    pub download_jobs: usize,
    pub no_derived_cache: bool,
    pub rebuild_derived_cache: bool,
    pub rebuild_update_caches: bool,
    /// Offline: any cache miss is a hard error (no network acquisition).
    pub offline: bool,
}

impl ExecutionConfig {
    pub fn to_cache_control(&self) -> CacheControl {
        CacheControl {
            no_derived_cache: self.no_derived_cache,
            rebuild_derived_cache: self.rebuild_derived_cache,
            rebuild_update_caches: self.rebuild_update_caches,
            jobs: self.jobs,
            parse_jobs: self.parse_jobs,
            download_jobs: self.download_jobs,
            offline: self.offline,
        }
    }
}

/// A staged analysis result: the outcome plus the directory containing
/// the rendered artifacts (validated by the caller before publication).
#[derive(Debug, Clone)]
pub struct StagedAnalysis {
    pub outcome: AnalysisOutcome,
    pub artifact_root: PathBuf,
}

/// Execution failure: cooperative cancellation or a real failure with a
/// stable machine error code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionError {
    Cancelled,
    Failed { code: &'static str, summary: String },
}

impl ExecutionError {
    pub fn code(&self) -> &'static str {
        match self {
            ExecutionError::Cancelled => crate::catalog::jobs::error_code::CANCELLED,
            ExecutionError::Failed { code, .. } => code,
        }
    }
}

/// Execute one analysis into `out_dir` (a job-specific staging root for
/// the worker; the direct CLI's out directory otherwise).
///
/// Cancellation is cooperative: the pipeline checks at stage and archive
/// boundaries. A single archive parse may finish before cancellation
/// takes effect. Never called per BGP element.
#[allow(clippy::too_many_arguments)]
pub fn execute_analysis(
    event_path: &Path,
    manifest_path: &Path,
    config: &ExecutionConfig,
    discovery: &dyn ArchiveDiscovery,
    out_dir: &Path,
    cancel: &AtomicBool,
    progress: &dyn ProgressSink,
) -> Result<StagedAnalysis, ExecutionError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(ExecutionError::Cancelled);
    }
    let cache_control = config.to_cache_control();
    let outcome = crate::orchestrate::run_real_analysis(
        event_path,
        manifest_path,
        &config.cache_dir,
        out_dir,
        discovery,
        cache_control,
        false,
        cancel,
        progress,
    );
    match outcome {
        AnalysisOutcome::Incomplete { failure } if failure == "cancelled" => {
            Err(ExecutionError::Cancelled)
        }
        AnalysisOutcome::Incomplete { failure } => Err(ExecutionError::Failed {
            code: classify_failure(&failure),
            summary: failure,
        }),
        completed => Ok(StagedAnalysis {
            outcome: completed,
            artifact_root: out_dir.to_path_buf(),
        }),
    }
}

/// Map a pipeline failure string to a stable machine error code.
pub fn classify_failure(failure: &str) -> &'static str {
    use crate::catalog::jobs::error_code;
    let lower = failure.to_lowercase();
    if lower.contains("broker discovery failed") || lower.contains("broker query failed") {
        error_code::SOURCE_DISCOVERY_FAILED
    } else if lower.contains("not cached") || lower.contains("offline") {
        error_code::ARCHIVE_NOT_CACHED
    } else if lower.contains("no rib found") || lower.contains("archive not found") {
        error_code::ARCHIVE_NOT_FOUND
    } else if lower.contains("checksum")
        || lower.contains("sha256")
        || lower.contains("hash mismatch")
    {
        error_code::ARCHIVE_CHECKSUM_MISMATCH
    } else if lower.contains("rate limit") || lower.contains("429") || lower.contains("retry-after")
    {
        error_code::ARCHIVE_RATE_LIMITED
    } else if lower.contains("forbidden") || lower.contains("403") {
        error_code::ARCHIVE_FORBIDDEN
    } else if lower.contains("failed to cache rib") || lower.contains("failed to cache update") {
        error_code::ARCHIVE_NOT_FOUND
    } else if lower.contains("rib parse") || lower.contains("baseline") {
        error_code::BASELINE_PARSE_FAILED
    } else if lower.contains("update") && lower.contains("parse") {
        error_code::UPDATE_PARSE_FAILED
    } else if lower.contains("output") || lower.contains("write") {
        error_code::ARTIFACT_PUBLICATION_FAILED
    } else {
        error_code::INTERNAL
    }
}

/// Compatibility shim so `PipelineError` display matches the direct CLI.
pub fn pipeline_error_message(e: &PipelineError) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_is_stable() {
        assert_eq!(
            classify_failure("broker discovery failed for RIBs: x"),
            "source_discovery_failed"
        );
        assert_eq!(
            classify_failure("archive not cached and offline mode is set: x"),
            "archive_not_cached"
        );
        assert_eq!(
            classify_failure("no RIB found for collector rrc00 at/before warmup"),
            "archive_not_found"
        );
        assert_eq!(
            classify_failure("checksum mismatch for x"),
            "archive_checksum_mismatch"
        );
        assert_eq!(
            classify_failure("update parse failed for x"),
            "update_parse_failed"
        );
        assert_eq!(classify_failure("something unknown"), "internal_error");
    }

    #[test]
    fn cancel_before_start_returns_cancelled() {
        let cancel = AtomicBool::new(true);
        let dir = tempfile::tempdir().unwrap();
        let cfg = ExecutionConfig {
            cache_dir: dir.path().join("cache"),
            jobs: 1,
            parse_jobs: 0,
            download_jobs: 2,
            no_derived_cache: false,
            rebuild_derived_cache: false,
            rebuild_update_caches: false,
            offline: true,
        };
        let err = execute_analysis(
            Path::new("nonexistent.json"),
            Path::new("nonexistent.json"),
            &cfg,
            &crate::discover::LiveArchiveDiscovery,
            &dir.path().join("out"),
            &cancel,
            &NoopSink,
        )
        .unwrap_err();
        assert_eq!(err, ExecutionError::Cancelled);
    }

    #[test]
    fn progress_sink_does_not_change_output() {
        // A sink that panics on emit must not affect the direct path:
        // NoopSink and TermSink are interchangeable by construction.
        let ev = ProgressEvent {
            stage: "ParsingUpdates",
            message: "x".into(),
            current: Some(1),
            total: Some(2),
            unit: Some("archives"),
        };
        NoopSink.emit(&ev);
        TermSink.emit(&ev);
    }

    #[test]
    fn unknown_total_has_no_percentage() {
        // The sink renders counts only when both sides are known; the
        // event itself carries no percentage anywhere.
        let ev = ProgressEvent {
            stage: "AcquiringArchives",
            message: "downloading".into(),
            current: Some(7),
            total: None,
            unit: None,
        };
        assert!(ev.total.is_none());
        let rendered = format!("{ev:?}");
        assert!(!rendered.contains('%'));
    }
}
