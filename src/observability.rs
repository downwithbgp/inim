//! Observability model — classify which maintenance signals public BGP
//! collectors can observe and what conclusions are permitted.
//!
//! inim never suggests that RFC 9003 text or RFC 8327 intent should appear
//! at a remote public collector. Community absence is never evidence of
//! mechanism non-use.

use serde::Serialize;

/// The protocol visibility scope of a maintenance signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SignalVisibility {
    /// Visible only on the BGP session carrying it (e.g. RFC 9003
    /// ADMINISTRATIVE_SHUTDOWN communication).
    DirectSessionSignal,
    /// May propagate to remote collectors as a route attribute
    /// (e.g. RFC 8326 GRACEFUL_SHUTDOWN community 65535:0).
    TransitiveRouteAttribute,
    /// The mechanism itself is not visible, but its consequences
    /// (withdrawal, announcement, path or attribute change) are.
    ExportedRouteConsequence,
    /// Neither the mechanism nor its direct consequences are visible
    /// in public BGP archives.
    InternalOrLowerLayerAction,
}

/// The strength of evidence for a particular mechanism.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum MechanismEvidence {
    /// The mechanism itself was observed (e.g. RFC 8326 community seen).
    ObservedDirectly,
    /// The observed behavior is consistent with the mechanism, but the
    /// mechanism itself was not directly visible.
    ConsistentWith,
    /// The mechanism is not observable from this dataset.
    NotObservable,
}

/// A documented mapping from a maintenance signal to its observability.
#[derive(Debug, Clone, Serialize)]
pub struct ObservabilityEntry {
    pub signal: &'static str,
    pub protocol_scope: &'static str,
    pub represented_in_data: &'static str,
    pub potentially_visible_remotely: bool,
    pub reliability: &'static str,
    pub permitted_conclusion: &'static str,
}

/// Return the observability matrix — the project's core contract about
/// what can and cannot be observed from public BGP archives.
pub fn observability_matrix() -> Vec<ObservabilityEntry> {
    vec![
        ObservabilityEntry {
            signal: "RFC 9003 Administrative Shutdown Communication",
            protocol_scope: "BGP NOTIFICATION with Administrative Shutdown subcode",
            represented_in_data: "No — NOTIFICATION messages are not stored in MRT TABLE_DUMP_V2 RIB or UPDATE archives",
            potentially_visible_remotely: false,
            reliability: "N/A — invisible to remote collectors",
            permitted_conclusion: "inim cannot assert that RFC 9003 shutdown was or was not used based on public RouteViews/RIS data",
        },
        ObservabilityEntry {
            signal: "RFC 8326 GRACEFUL_SHUTDOWN community 65535:0",
            protocol_scope: "BGP path attribute — well-known discretionary COMMUNITY",
            represented_in_data: "Yes — stored in MRT RIB entries and UPDATE messages when present; preserved through inim ingestion pipeline",
            potentially_visible_remotely: true,
            reliability: "Moderate — may be stripped by intermediate ASNs or omitted by the observing peer. Absence does not indicate non-use.",
            permitted_conclusion: "When present, inim may report: 'GRACEFUL_SHUTDOWN community observed on N streams.' When absent, inim must state: 'No GRACEFUL_SHUTDOWN community reached the selected observers. This does not establish that the mechanism was not used.'",
        },
        ObservabilityEntry {
            signal: "RFC 8327 Session Culling / BGP Cease",
            protocol_scope: "Local router action — session termination",
            represented_in_data: "Only its exported route consequences (withdrawals) are visible",
            potentially_visible_remotely: false,
            reliability: "N/A — mechanism invisible, only consequences observable",
            permitted_conclusion: "inim may report withdrawals as 'consistent with session culling' but must not assert that culling occurred",
        },
        ObservabilityEntry {
            signal: "BGP Graceful Restart",
            protocol_scope: "BGP capability negotiation (OPEN message) + session state",
            represented_in_data: "GR capability not stored in MRT TABLE_DUMP_V2; resulting route behavior (preserved forwarding during restart) may be visible as stable routes during a session flap",
            potentially_visible_remotely: false,
            reliability: "N/A — capability exchange invisible, only resulting route stability patterns observable",
            permitted_conclusion: "inim may note stable forwarding patterns but must not claim Graceful Restart was or was not negotiated",
        },
    ]
}

/// Classification labels used in user-facing reports.
pub const OBSERVED_MECHANISM_LABEL: &str = "Observed mechanism hints";
pub const OBSERVED_IMPACT_LABEL: &str = "Observed routing impact";
