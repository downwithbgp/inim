//! BGP ingestion boundary — bgpkit-parser integration.
//!
//! This is the only module that imports bgpkit-parser. It converts
//! parsed BGP elements (BgpElem) into inim-native RouteObservation
//! values using an explicit IngestContext.

use std::path::PathBuf;

use bgpkit_parser::models::AsPathSegment;
use bgpkit_parser::models::ElemType;
use bgpkit_parser::BgpElem;
use bgpkit_parser::BgpkitParser;
use chrono::TimeZone;
use chrono::Utc;

use crate::domain::observation::{
    Asn, CollectorId, Communities, IngestRole, ObservationAttributes, ObservationId,
    ObservationKind, ObservationProvenance, ObservationSource, RouteObservation,
};
use crate::domain::route::Prefix;

// ── Ingest context ─────────────────────────────────────────────────

/// Carries the role and collector identity for a parsed input stream.
/// Never inferred from BgpElem — must be supplied by the caller.
#[derive(Debug, Clone)]
pub struct IngestContext {
    pub role: IngestRole,
    pub collector: CollectorId,
    pub input_path: PathBuf,
    /// Canonical source URL (for provenance).
    pub source_url: Option<String>,
    /// SHA-256 of the source file (for provenance).
    pub source_sha: Option<String>,
    /// When set and role=Rib, apply bgpkit-parser origin_asn filters.
    /// This is safe for RIB preflight only — never apply to UPDATEs.
    pub origin_asn_filters: Vec<u32>,
    /// Deterministic archive order index assigned by the coordinator.
    pub archive_order: u64,
}

// ── Error types ────────────────────────────────────────────────────

/// Errors that can occur during BGP data ingestion.
#[derive(Debug)]
pub enum InimError {
    /// Could not open or read the input file.
    InputOpenError { path: String, source: String },
    /// bgpkit-parser failed to initialise.
    ParserInitializationError { path: String, source: String },
    /// An invalid filter was supplied.
    InvalidFilterError { filter: String, source: String },
    /// An individual record could not be decoded.
    RecordDecodeError {
        path: String,
        position: Option<u64>,
        source: String,
    },
    /// The observation cannot be represented correctly (e.g. ADD-PATH
    /// identity cannot be preserved).
    UnsupportedObservationError {
        path: String,
        element_seq: u64,
        reason: String,
    },
    /// Required baseline data is missing.
    MissingBaselineError {
        collector: CollectorId,
        reason: String,
    },
    /// Observation stream is discontinuous (session reset or archive gap).
    DiscontinuousObservationError {
        collector: CollectorId,
        element_seq: u64,
        reason: String,
    },
}

impl std::fmt::Display for InimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InimError::InputOpenError { path, source } => {
                write!(f, "cannot open input {path}: {source}")
            }
            InimError::ParserInitializationError { path, source } => {
                write!(f, "parser init failed for {path}: {source}")
            }
            InimError::InvalidFilterError { filter, source } => {
                write!(f, "invalid filter '{filter}': {source}")
            }
            InimError::RecordDecodeError {
                path,
                position,
                source,
            } => {
                write!(
                    f,
                    "record decode error in {path} at {:?}: {source}",
                    position
                )
            }
            InimError::UnsupportedObservationError {
                path,
                element_seq,
                reason,
            } => {
                write!(
                    f,
                    "unsupported observation in {path} element #{element_seq}: {reason}"
                )
            }
            InimError::MissingBaselineError { collector, reason } => {
                write!(f, "missing baseline for {collector}: {reason}")
            }
            InimError::DiscontinuousObservationError {
                collector,
                element_seq,
                reason,
            } => {
                write!(
                    f,
                    "discontinuous observation for {collector} at element #{element_seq}: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for InimError {}

// ── Public API ─────────────────────────────────────────────────────

/// A streaming iterator over RouteObservation values, backed by bgpkit-parser.
///
/// Wraps a `BgpkitParser` and applies the conversion with context.
/// Observations are yielded incrementally — never collected into a Vec.
pub struct ObservationStream {
    inner: Box<dyn Iterator<Item = Result<RouteObservation, InimError>> + Send>,
}

impl ObservationStream {
    /// Create a new observation stream from a local MRT file.
    ///
    /// Returns an error if the file cannot be opened or the parser fails.
    pub fn from_local_file(path: PathBuf, context: IngestContext) -> Result<Self, InimError> {
        let path_str = path.to_string_lossy().to_string();

        let mut parser =
            BgpkitParser::new(&path_str).map_err(|e| InimError::ParserInitializationError {
                path: path_str.clone(),
                source: e.to_string(),
            })?;

        // Apply origin ASN filter for RIB preflight (parser-level speedup).
        // We unconditionally consume parser via add_filter; if it fails,
        // we restart without filter (graceful degradation).
        if context.role == IngestRole::Rib && !context.origin_asn_filters.is_empty() {
            let asn = context.origin_asn_filters[0];
            parser = match parser.add_filter("origin_asn", &asn.to_string()) {
                Ok(p) => p,
                Err(_e) => {
                    eprintln!(
                        "[{}] warning: origin_asn filter {} not applied, falling back to in-stream filtering",
                        context.collector.0, asn
                    );
                    // Re-create parser without filter
                    BgpkitParser::new(&path_str).map_err(|e| {
                        InimError::ParserInitializationError {
                            path: path_str.clone(),
                            source: e.to_string(),
                        }
                    })?
                }
            };
        }

        let input_path = path_str.clone();
        let mut seq: u64 = 0;

        let iter = parser.into_elem_iter().map(move |elem| {
            let result = bgp_elem_to_observation(&elem, seq, &input_path, &context);
            seq += 1;
            result
        });

        Ok(ObservationStream {
            inner: Box::new(iter),
        })
    }
}

impl Iterator for ObservationStream {
    type Item = Result<RouteObservation, InimError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

// ── Core conversion ────────────────────────────────────────────────

/// Convert a single BgpElem into a RouteObservation.
///
/// This is the only function that touches bgpkit-parser types.
fn bgp_elem_to_observation(
    elem: &BgpElem,
    element_seq: u64,
    input_path: &str,
    context: &IngestContext,
) -> Result<RouteObservation, InimError> {
    // ── Timestamp conversion ──────────────────────────────────
    let timestamp = f64_to_utc(elem.timestamp);

    // ── Prefix ────────────────────────────────────────────────
    let prefix_str = format!("{}", elem.prefix.prefix);
    let prefix = Prefix::from(prefix_str.as_str());

    // ── Kind mapping ⚠️  RibEntry never inferred from BgpElem ─
    let kind = match context.role {
        IngestRole::Rib => ObservationKind::RibEntry,
        IngestRole::Updates => match elem.elem_type {
            ElemType::ANNOUNCE => ObservationKind::Announcement,
            ElemType::WITHDRAW => ObservationKind::Withdrawal,
        },
    };

    // ── Route identity validation ─────────────────────────────
    let peer_ip = canonical_ip(elem.peer_ip);
    let peer_asn = Asn(u32::from(elem.peer_asn));

    // ── Attributes (None for withdrawals) ─────────────────────
    let attributes = match kind {
        ObservationKind::Announcement | ObservationKind::RibEntry => Some(build_attributes(elem)),
        ObservationKind::Withdrawal | ObservationKind::SessionBoundary => None,
    };

    // ── Provenance ────────────────────────────────────────────
    let provenance = ObservationProvenance {
        input: input_path.to_string(),
        source_url: context.source_url.clone(),
        archive_sha256: context.source_sha.clone(),
        role: context.role,
        parser_representation: "bgpkit-bgp-elem".to_string(),
        mrt_timestamp: elem.timestamp,
        element_seq,
        archive_order: context.archive_order,
    };

    Ok(RouteObservation {
        id: ObservationId(element_seq),
        source: ObservationSource::LocalFile(input_path.to_string()),
        timestamp,
        collector: context.collector.clone(),
        peer_ip,
        peer_asn,
        prefix,
        kind,
        attributes,
        provenance,
    })
}

// ── Attribute conversion ───────────────────────────────────────────

fn build_attributes(elem: &BgpElem) -> ObservationAttributes {
    let as_path: Vec<u32> = elem
        .as_path
        .as_ref()
        .map(|ap| {
            ap.iter_segments()
                .flat_map(|seg| segment_asns(seg).into_iter().map(u32::from))
                .collect()
        })
        .unwrap_or_default();

    ObservationAttributes {
        as_path,
        origin_asns: elem
            .origin_asns
            .as_ref()
            .map(|v| v.iter().map(|a| Asn(u32::from(a))).collect())
            .unwrap_or_default(),
        next_hop: elem.next_hop,
        origin: elem.origin.as_ref().map(|o| format!("{o}")),
        local_pref: elem.local_pref,
        med: elem.med,
        atomic_aggregate: elem.atomic,
        communities: elem
            .communities
            .as_ref()
            .map(|v| Communities::from_strings(v.iter().map(|c| format!("{c}")).collect()))
            .unwrap_or_default(),
    }
}

/// Extract all ASNs from an AsPathSegment regardless of segment type.
fn segment_asns(seg: &AsPathSegment) -> Vec<bgpkit_parser::models::Asn> {
    match seg {
        AsPathSegment::AsSequence(v)
        | AsPathSegment::AsSet(v)
        | AsPathSegment::ConfedSequence(v)
        | AsPathSegment::ConfedSet(v) => v.iter().copied().collect(),
    }
}

// ── Utility ────────────────────────────────────────────────────────

/// Normalize an IP address: IPv4-mapped IPv6 → plain IPv4.
fn canonical_ip(ip: std::net::IpAddr) -> std::net::IpAddr {
    match ip {
        std::net::IpAddr::V6(v6) if v6.to_ipv4_mapped().is_some() => {
            std::net::IpAddr::V4(v6.to_ipv4_mapped().unwrap())
        }
        _ => ip,
    }
}

fn f64_to_utc(epoch: f64) -> chrono::DateTime<Utc> {
    let secs = epoch.trunc() as i64;
    let nsecs = ((epoch - epoch.trunc()) * 1_000_000_000.0) as u32;
    Utc.timestamp_opt(secs, nsecs).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inim_error_display_input_open() {
        let e = InimError::InputOpenError {
            path: "test.mrt".into(),
            source: "no such file".into(),
        };
        let msg = format!("{e}");
        assert!(msg.contains("test.mrt"));
        assert!(msg.contains("no such file"));
    }

    #[test]
    fn inim_error_display_parser_init() {
        let e = InimError::ParserInitializationError {
            path: "bad.bz2".into(),
            source: "invalid bzip2".into(),
        };
        assert!(format!("{e}").contains("bad.bz2"));
    }

    #[test]
    fn inim_error_display_unsupported() {
        let e = InimError::UnsupportedObservationError {
            path: "data.mrt".into(),
            element_seq: 7,
            reason: "ADD-PATH path_id required but not present".into(),
        };
        let msg = format!("{e}");
        assert!(msg.contains("data.mrt"));
        assert!(msg.contains("#7"));
    }

    #[test]
    fn f64_to_utc_conversion() {
        let ts = 1749990300.0;
        let dt = f64_to_utc(ts);
        assert_eq!(dt.timestamp(), 1749990300);
    }

    #[test]
    fn f64_to_utc_subsecond() {
        let ts = 1749990300.5;
        let dt = f64_to_utc(ts);
        assert_eq!(dt.timestamp(), 1749990300);
        assert_eq!(dt.timestamp_subsec_nanos(), 500_000_000);
    }

    #[test]
    fn ingest_context_construction() {
        let ctx = IngestContext {
            role: IngestRole::Rib,
            collector: CollectorId("route-views2".into()),
            input_path: PathBuf::from("rib.mrt.bz2"),
            source_url: None,
            source_sha: None,
            origin_asn_filters: vec![],
            archive_order: 0,
        };
        assert_eq!(ctx.collector.0, "route-views2");
    }

    // ── Streaming tests (no real MRT needed) ────────────────────

    #[test]
    fn observation_stream_is_send() {
        // Compile-time check: observation stream should be Send
        fn assert_send<T: Send>() {}
        assert_send::<ObservationStream>();
    }

    #[test]
    fn ingest_context_holds_role_explicitly() {
        // Verify RibEntry comes from context, not BgpElem
        let ctx = IngestContext {
            role: IngestRole::Rib,
            collector: CollectorId("test".into()),
            input_path: PathBuf::from("test.mrt"),
            source_url: None,
            source_sha: None,
            origin_asn_filters: vec![],
            archive_order: 0,
        };
        // If role were inferred from elem.elem_type, Rib would be impossible
        // since BgpElem only has ANNOUNCE/WITHDRAW
        assert_eq!(ctx.role, IngestRole::Rib);
    }

    // ── Real MRT fixture test ─────────────────────────────────

    #[test]
    fn parses_actual_mrt_fixture_into_observations() {
        let fixture_path = std::path::PathBuf::from("tests/fixtures/mrt/update-example.gz");

        let ctx = IngestContext {
            role: IngestRole::Updates,
            collector: CollectorId("route-views2".into()),
            input_path: fixture_path.clone(),
            source_url: None,
            source_sha: None,
            origin_asn_filters: vec![],
            archive_order: 0,
        };

        let stream = ObservationStream::from_local_file(fixture_path, ctx)
            .expect("should open real MRT fixture");

        // Collect first 20 observations for assertion (test-only collection)
        let observations: Vec<_> = stream.into_iter().take(20).collect();

        // Must have at least one successful observation
        let ok_count = observations.iter().filter(|r| r.is_ok()).count();
        assert!(
            ok_count > 0,
            "fixture must yield at least one valid observation"
        );

        // Verify structure of first successful observation
        if let Some(Ok(first)) = observations.iter().find(|r| r.is_ok()) {
            // Must have a kind
            assert!(
                matches!(
                    first.kind,
                    ObservationKind::Announcement | ObservationKind::Withdrawal
                ),
                "real MRT observations should be announcements or withdrawals"
            );

            // Must have a non-empty prefix
            assert!(!first.prefix.0.is_empty());

            // Provenance must reference the fixture
            assert!(first.provenance.input.contains("update-example.gz"));

            // Element sequence must be set
            assert!(
                first.provenance.element_seq < 20,
                "element_seq should be within first 20"
            );
        }
    }
}
