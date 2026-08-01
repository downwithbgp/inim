//! NOC incident-workbench presentation model (Session 36).
//!
//! `ObserverEpisode` groups observer-prefix streams at ONE observer
//! session (collector + peer) that share a meaningful, temporally
//! coherent signature. Effect kinds are presentation-level groupings
//! derived from EXISTING lifecycle/transition evidence — they introduce
//! no new route-transition semantics.
//!
//! This module is the single presentation model shared by the web
//! workbench, the text report, and the JSON API (Part 12). No counts are
//! recalculated in templates.
//!
//! Token discipline: this file contains no operator-specific plane
//! identity; plane labels and ASNs arrive at runtime via the reviewed
//! profile data (network-profile.json) and the run's manifest payload.

use crate::catalog::domain::{RunTransitionRecord, SemanticWaveSummary, StreamLifecycleSummary};
use crate::catalog::netprofile::{
    classify_route, CollectorLocationRegistry, ServicePlaneProfile, SessionRelationship,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
/// Presentation-level effect kind of one episode.
///
/// These are groupings of existing evidence, never new transition
/// semantics. Each variant maps to lifecycle/transition evidence:
///
/// - `TemporaryStreamAbsence`: streams withdrawn then restored
///   (withdrawal + restoration evidence, absence interval).
/// - `RouteWithdrawal`: streams withdrawn and NOT restored inside the
///   analysis window (end state unresolved).
/// - `PathReplacement`: material AS-path change without plane departure
///   (path-replacement transitions, transit retained).
/// - `NamedPlaneDeparture`: streams departed the reviewed named plane
///   (path departed the reviewed plane's ASN set).
/// - `NamedPlaneReturn`: streams returned to the reviewed named plane
///   after departing it.
/// - `PrependChange`: AS-path prepending increased or reduced with the
///   plane membership unchanged.
/// - `MixedRouteChange`: the session shows several of the above across
///   its streams (or per-stream multi-cycle evidence).
/// - `NoRouteStateChange`: qualifying baseline existed and no route-state
///   change was observed at this session.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EffectKind {
    TemporaryStreamAbsence,
    RouteWithdrawal,
    PathReplacement,
    NamedPlaneDeparture,
    NamedPlaneReturn,
    PrependChange,
    MixedRouteChange,
    NoRouteStateChange,
}

impl EffectKind {
    /// Stable machine label.
    pub fn label(&self) -> &'static str {
        match self {
            EffectKind::TemporaryStreamAbsence => "TemporaryStreamAbsence",
            EffectKind::RouteWithdrawal => "RouteWithdrawal",
            EffectKind::PathReplacement => "PathReplacement",
            EffectKind::NamedPlaneDeparture => "NamedPlaneDeparture",
            EffectKind::NamedPlaneReturn => "NamedPlaneReturn",
            EffectKind::PrependChange => "PrependChange",
            EffectKind::MixedRouteChange => "MixedRouteChange",
            EffectKind::NoRouteStateChange => "NoRouteStateChange",
        }
    }
}

/// Relationship of the observer session to the reviewed named plane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationshipKind {
    Direct,
    Indirect,
    Other,
    Ambiguous,
}

impl RelationshipKind {
    pub fn label(&self) -> &'static str {
        match self {
            RelationshipKind::Direct => "Direct",
            RelationshipKind::Indirect => "Indirect",
            RelationshipKind::Other => "Other",
            RelationshipKind::Ambiguous => "Ambiguous",
        }
    }
}

/// Coverage status of one observer session for the target.
///
/// These three states must NEVER collapse into one zero:
/// - `NoChange`: a qualifying baseline existed and no route-state change
///   occurred at this session.
/// - `NoBaselineVisibility`: the target was not visible at this session
///   (no qualifying baseline stream).
/// - `IncompleteCoverage`: the observation could not be completed
///   (run incomplete / archive coverage limitation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoverageStatus {
    NoChange,
    NoBaselineVisibility,
    IncompleteCoverage,
}

impl CoverageStatus {
    pub fn label(&self) -> &'static str {
        match self {
            CoverageStatus::NoChange => "NoChange",
            CoverageStatus::NoBaselineVisibility => "NoBaselineVisibility",
            CoverageStatus::IncompleteCoverage => "IncompleteCoverage",
        }
    }
}

/// One episode: streams at one observer session sharing a signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObserverEpisode {
    /// Analysis run id this episode is derived from.
    pub analysis_run: i64,
    /// Observer session identity: "<family>/<collector> peer <peer_ip>".
    pub observer_session: String,
    pub observer_site: String,
    pub observer_region: String,
    /// Peer ASN when reviewed evidence provides it; None → unreviewed.
    pub peer_asn: Option<u32>,
    pub peer_label: String,
    pub peer_role: String,
    pub relationship: RelationshipKind,
    /// Named path plane label (runtime profile data), or "none reviewed".
    pub named_path_plane: String,
    pub effect_kind: EffectKind,
    /// First observed route-state change in the episode (UTC).
    pub first_change: Option<String>,
    /// Peak interval of the episode's semantic evidence (UTC..UTC).
    pub peak_interval: Option<(String, String)>,
    /// Last observed route-state change in the episode (UTC).
    pub last_change: Option<String>,
    pub restoration_start: Option<String>,
    pub restoration_end: Option<String>,
    /// Streams with a qualifying baseline at this session (per run).
    pub baseline_stream_count: usize,
    /// Streams in the episode that changed.
    pub changed_stream_count: usize,
    /// Distinct prefixes across the episode's changed streams.
    pub distinct_prefix_count: usize,
    /// Route instances involved in the episode's changed streams.
    pub route_instance_count: usize,
    /// Changed streams whose restoration is unresolved.
    pub unresolved_count: usize,
    pub coverage_status: CoverageStatus,
    /// Data-supported sentence describing the episode.
    pub representative_evidence: String,
    /// Member stream rows (prefix-level evidence, for drill-down).
    pub streams: Vec<EpisodeStream>,
}

/// One member stream of an episode (prefix-level presentation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpisodeStream {
    pub prefix: String,
    pub category: String,
    pub withdrawn: bool,
    pub restored: bool,
    pub baseline_instances: i64,
    pub transition_count: i64,
    pub add_path_ambiguous: bool,
    pub evidence_refs: String,
}

/// Load all runs' streams+transitions+waves for a set of run ids.
#[derive(Debug, Default)]
pub struct RunEvidence {
    pub streams: Vec<StreamLifecycleSummary>,
    pub transitions: Vec<RunTransitionRecord>,
    pub waves: Vec<SemanticWaveSummary>,
}

impl RunEvidence {
    pub fn load(conn: &Connection, run_ids: &[i64]) -> Result<Self, String> {
        let mut out = RunEvidence::default();
        for run_id in run_ids {
            out.streams
                .extend(crate::catalog::db::list_streams(conn, *run_id, None, None)?);
            out.transitions
                .extend(crate::catalog::db::list_transitions(conn, *run_id)?);
            out.waves
                .extend(crate::catalog::db::list_waves(conn, *run_id)?);
        }
        Ok(out)
    }
}

/// Derive one stream's primary effect kind from lifecycle evidence.
///
/// Precedence: unresolved withdrawal > restored withdrawal (temporary
/// absence) > plane departure > plane return > prepend change > plain
/// path replacement > no change. Multi-cycle evidence (both absence and
/// path replacement) reads as `MixedRouteChange` only when the caller
/// merges kinds; per-stream we keep the strongest single signal and the
/// caller detects mixed sessions by comparing kinds across streams.
pub fn stream_effect_kind(stream: &StreamLifecycleSummary) -> EffectKind {
    let withdrawn_unresolved = stream.withdrawn && !stream.restored;
    let temporary_absence = stream.withdrawn && stream.restored;
    let departed = stream.transit_state == "departed" || stream.transit_state == "left";
    let returned = stream.transit_state == "returned";
    let prepend = stream.category == "PrependOnly";
    let path_changed = stream.category == "PathChangedStillViaTransit"
        || stream.category == "GenericPathChange"
        || stream.transition_count > 0;
    if withdrawn_unresolved {
        EffectKind::RouteWithdrawal
    } else if temporary_absence {
        EffectKind::TemporaryStreamAbsence
    } else if departed {
        EffectKind::NamedPlaneDeparture
    } else if returned {
        EffectKind::NamedPlaneReturn
    } else if prepend {
        EffectKind::PrependChange
    } else if path_changed {
        EffectKind::PathReplacement
    } else {
        EffectKind::NoRouteStateChange
    }
}

/// Build observer episodes for one run.
///
/// Streams are grouped by (observer session, effect kind). `peer_asn`,
/// `peer_label`, `peer_role`, and `relationship` come from the reviewed
/// session context when provided (per collector, keyed by peer IP).
/// `named_path_plane` is the run's reviewed plane label (runtime data).
/// Deterministic: episodes sorted by (first change, region, collector,
/// peer ASN); no-change episodes sort after changed episodes.
pub fn build_episodes(
    run_id: i64,
    streams: &[StreamLifecycleSummary],
    transitions: &[RunTransitionRecord],
    waves: &[SemanticWaveSummary],
    registry: &CollectorLocationRegistry,
    session_peers: &BTreeMap<(String, String), (u32, String, String, RelationshipKind)>,
    named_path_plane: &str,
) -> Vec<ObserverEpisode> {
    // Group streams by session + kind.
    let mut groups: BTreeMap<(String, String, EffectKind), Vec<&StreamLifecycleSummary>> =
        BTreeMap::new();
    for s in streams {
        let kind = stream_effect_kind(s);
        groups
            .entry((s.collector.clone(), s.peer_ip.clone(), kind))
            .or_default()
            .push(s);
    }

    let mut episodes = Vec::new();
    for ((collector, peer_ip, kind), members) in groups {
        let family = "catalog"; // family label is a display concern
        let session_key = (collector.clone(), peer_ip.clone());
        let peer_facts = session_peers.get(&session_key);
        let (peer_asn, peer_label, peer_role, relationship) = match peer_facts {
            Some((asn, label, role, rel)) => (Some(*asn), label.clone(), role.clone(), rel.clone()),
            None => (
                None,
                "unreviewed peer".to_string(),
                "unreviewed".to_string(),
                RelationshipKind::Ambiguous,
            ),
        };

        // Per-session transition timestamps for this peer.
        let mut first_change: Option<String> = None;
        let mut last_change: Option<String> = None;
        for t in transitions {
            if t.collector == collector && t.peer_ip == peer_ip {
                let ts = t.occurred_utc.clone();
                if first_change
                    .as_deref()
                    .map(|f| ts.as_str() < f)
                    .unwrap_or(true)
                {
                    first_change = Some(ts.clone());
                }
                if last_change
                    .as_deref()
                    .map(|l| ts.as_str() > l)
                    .unwrap_or(true)
                {
                    last_change = Some(ts.clone());
                }
            }
        }

        // Restoration evidence from stream lifecycle fields.
        let mut restoration_start: Option<String> = None;
        let mut restoration_end: Option<String> = None;
        let mut changed_count = 0usize;
        let mut distinct_prefixes: Vec<String> = Vec::new();
        let mut route_instances = 0usize;
        let mut unresolved = 0usize;
        for s in &members {
            if s.transition_count > 0 || s.withdrawn || s.category != "Unchanged" {
                changed_count += 1;
                if !distinct_prefixes.contains(&s.prefix) {
                    distinct_prefixes.push(s.prefix.clone());
                }
                route_instances += s.max_active_instances.max(1) as usize;
            }
            if (s.withdrawn && !s.restored) || s.add_path_ambiguous {
                unresolved += 1;
            }
            // Restoration interval: min/max of restored stream's
            // withdrawal_audit restoration times come from lifecycle
            // artifacts; here we use the stream-level restored flag and
            // the last change as the conservative bound (the sentence
            // generator only claims what the evidence supports).
            if s.restored {
                let est = last_change.clone();
                restoration_start = match (restoration_start.clone(), est.clone()) {
                    (None, e) => e,
                    (Some(cur), Some(e)) if e < cur => Some(e),
                    (cur, _) => cur,
                };
                restoration_end = match (restoration_end.clone(), est.clone()) {
                    (None, e) => e,
                    (Some(cur), Some(e)) if e > cur => Some(e),
                    (cur, _) => cur,
                };
            }
        }

        // Peak interval from overlapping semantic waves of this session.
        let mut peak_interval: Option<(String, String)> = None;
        for w in waves {
            if w.start
                < peak_interval
                    .as_ref()
                    .map(|p| p.0.clone())
                    .unwrap_or_default()
                || peak_interval.is_none()
            {
                peak_interval = Some((w.start.clone(), w.end.clone()));
            }
        }

        // Coverage status: a qualifying baseline exists at this session
        // (streams present), so the status is NoChange when no route-state
        // change was observed. NoBaselineVisibility / IncompleteCoverage
        // are derived at the workbench level from run coverage, because a
        // session without streams never reaches this per-run builder.
        let coverage = CoverageStatus::NoChange;

        let episode = ObserverEpisode {
            analysis_run: run_id,
            observer_session: format!("{family}/{collector} peer {peer_ip}"),
            observer_site: registry
                .location("", &collector)
                .map(|c| c.location.clone())
                .unwrap_or_else(|| collector.clone()),
            observer_region: registry.region("", &collector),
            peer_asn,
            peer_label,
            peer_role,
            relationship,
            named_path_plane: named_path_plane.to_string(),
            effect_kind: kind.clone(),
            first_change,
            peak_interval,
            last_change,
            restoration_start,
            restoration_end,
            baseline_stream_count: members.len(),
            changed_stream_count: changed_count,
            distinct_prefix_count: distinct_prefixes.len(),
            route_instance_count: route_instances,
            unresolved_count: unresolved,
            coverage_status: coverage,
            representative_evidence: String::new(), // filled by sentence renderer
            streams: members
                .iter()
                .map(|s| EpisodeStream {
                    prefix: s.prefix.clone(),
                    category: s.category.clone(),
                    withdrawn: s.withdrawn,
                    restored: s.restored,
                    baseline_instances: s.baseline_instances,
                    transition_count: s.transition_count,
                    add_path_ambiguous: s.add_path_ambiguous,
                    evidence_refs: s.evidence_refs.clone(),
                })
                .collect(),
        };
        episodes.push(episode);
    }

    // Deterministic ordering: changed first (by first change), then
    // no-change; within a tier by (region, collector, peer ASN, kind).
    episodes.sort_by(|a, b| {
        let a_changed = a.effect_kind != EffectKind::NoRouteStateChange;
        let b_changed = b.effect_kind != EffectKind::NoRouteStateChange;
        match (a_changed, b_changed) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                let ac = (
                    a.first_change.clone(),
                    a.observer_region.clone(),
                    a.observer_site.clone(),
                    a.peer_asn.unwrap_or(u32::MAX),
                );
                let bc = (
                    b.first_change.clone(),
                    b.observer_region.clone(),
                    b.observer_site.clone(),
                    b.peer_asn.unwrap_or(u32::MAX),
                );
                ac.cmp(&bc)
            }
        }
    });
    episodes
}

/// Map a session's historical relationship evidence to the presentation
/// RelationshipKind (Direct/Indirect/Other/Ambiguous).
pub fn relationship_kind(rels: &[SessionRelationship]) -> RelationshipKind {
    if rels.is_empty() {
        return RelationshipKind::Ambiguous;
    }
    let direct = rels
        .iter()
        .any(|r| matches!(r, SessionRelationship::DirectPeerToNamedPlane { .. }));
    let indirect = rels
        .iter()
        .any(|r| matches!(r, SessionRelationship::IndirectPathViaNamedPlane { .. }));
    match (direct, indirect) {
        (true, _) => RelationshipKind::Direct,
        (false, true) => RelationshipKind::Indirect,
        (false, false) => RelationshipKind::Other,
    }
}

/// Classify a session's relationship using route evidence.
pub fn classify_session_relationship(
    profile: &ServicePlaneProfile,
    peer_asn: u32,
    as_paths: &[Vec<u32>],
) -> RelationshipKind {
    let mut rels: Vec<SessionRelationship> = Vec::new();
    for path in as_paths {
        for rel in classify_route(profile, peer_asn, path) {
            if !rels.contains(&rel) {
                rels.push(rel);
            }
        }
    }
    relationship_kind(&rels)
}

// ── Operational sentence rendering (Part 4) ─────────────────────────

/// Render one episode as a concise, data-supported operational sentence.
///
/// Rules enforced here (and by the required tests):
/// - The collector SITE is named at the collector's reviewed location;
///   the peer's own location is never implied ("RRC06 at Otemachi,
///   Tokyo, receiving routes from peer AS…" — the peer is not said to be
///   in Tokyo unless separately reviewed).
/// - The verb is effect-specific (became absent / was withdrawn /
///   changed AS path / left the reviewed path plane / returned to
///   visibility / restored its baseline path / remained changed).
/// - Visibility restoration ("returned to visibility") is distinct from
///   baseline restoration ("restored its baseline path").
/// - When stream and prefix counts differ, both units are named
///   ("14 observer-prefix streams covering 11 prefixes").
/// - The sentence never claims traffic loss and never claims causation.
pub fn render_episode_sentence(episode: &ObserverEpisode) -> String {
    let site = &episode.observer_site;
    let collector = collector_from_session(&episode.observer_session);
    let peer = match episode.peer_asn {
        Some(asn) => format!("peer AS{asn}"),
        None => {
            // Peer identity is unreviewed: name only the peer IP.
            let ip = episode
                .observer_session
                .rsplit("peer ")
                .next()
                .unwrap_or("unreviewed peer");
            format!("peer {ip} (ASN unreviewed)")
        }
    };
    let plane = &episode.named_path_plane;
    let unit = match (episode.changed_stream_count, episode.distinct_prefix_count) {
        (s, p) if s == p && s == 1 => "1 selected prefix".to_string(),
        (s, p) if s == p => format!("{s} selected prefixes"),
        (s, p) => format!("{s} observer-prefix streams covering {p} prefixes"),
    };

    let head = format!("{collector} at {site}, receiving routes from {peer},");

    match episode.effect_kind {
        EffectKind::TemporaryStreamAbsence => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            let restored = match (&episode.restoration_start, &episode.restoration_end) {
                (Some(a), Some(b)) if a != b => format!(" between {a} and {b} UTC"),
                (Some(a), _) => format!(" at {a} UTC"),
                _ => String::new(),
            };
            format!(
                "{head} saw {unit} become absent at {t} UTC and return to visibility{restored}."
            )
        }
        EffectKind::RouteWithdrawal => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            format!(
                "{head} saw {unit} was withdrawn at {t} UTC and remained absent through the end of the analysis window."
            )
        }
        EffectKind::PathReplacement => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            let restored = match &episode.restoration_start {
                Some(r) => format!(" Their initial path class returned by {r} UTC."),
                None => String::new(),
            };
            format!("{head} saw {unit} change AS path at {t} UTC.{restored}")
        }
        EffectKind::NamedPlaneDeparture => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            format!("{head} saw {unit} leave the {plane} path at {t} UTC.")
        }
        EffectKind::NamedPlaneReturn => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            format!("{head} saw {unit} return to the {plane} path at {t} UTC.")
        }
        EffectKind::PrependChange => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            format!(
                "{head} saw {unit} change AS-path prepending at {t} UTC while retaining the {plane} path."
            )
        }
        EffectKind::MixedRouteChange => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            format!("{head} saw mixed route-state changes across {unit} beginning at {t} UTC.")
        }
        EffectKind::NoRouteStateChange => {
            format!("{head} observed no route-state change among the selected streams.")
        }
    }
}

/// Extract the collector label from an observer session string of the
/// form "<family>/<collector> peer <ip>".
fn collector_from_session(session: &str) -> &str {
    match session.split_once('/') {
        Some((_, rest)) => match rest.split_once(" peer ") {
            Some((c, _)) => c,
            None => rest,
        },
        None => session,
    }
}

#[cfg(test)]
mod sentence_tests {
    use super::*;

    fn episode(kind: EffectKind) -> ObserverEpisode {
        ObserverEpisode {
            analysis_run: 1,
            observer_session: "catalog/rrc06 peer 192.0.2.1".to_string(),
            observer_site: "Otemachi, Tokyo, Japan".to_string(),
            observer_region: "APAC".to_string(),
            peer_asn: Some(64500),
            peer_label: "AS64500".to_string(),
            peer_role: "regional-re".to_string(),
            relationship: RelationshipKind::Direct,
            named_path_plane: "Plane A".to_string(),
            effect_kind: kind,
            first_change: Some("2019-08-21T16:45:25Z".to_string()),
            peak_interval: None,
            last_change: Some("2019-08-21T17:02:19Z".to_string()),
            restoration_start: Some("2019-08-21T16:59:00Z".to_string()),
            restoration_end: Some("2019-08-21T17:02:00Z".to_string()),
            baseline_stream_count: 11,
            changed_stream_count: 11,
            distinct_prefix_count: 11,
            route_instance_count: 11,
            unresolved_count: 0,
            coverage_status: CoverageStatus::NoChange,
            representative_evidence: String::new(),
            streams: Vec::new(),
        }
    }

    #[test]
    fn sentence_distinguishes_collector_site_from_peer_location() {
        let ep = episode(EffectKind::TemporaryStreamAbsence);
        let s = render_episode_sentence(&ep);
        // The collector site is named; the peer is NOT said to be in Tokyo.
        assert!(s.contains("rrc06 at Otemachi, Tokyo, Japan"), "{s}");
        assert!(s.contains("receiving routes from peer AS64500"), "{s}");
        assert!(
            !s.contains("peer AS64500 at Otemachi") && !s.contains("peer in Tokyo"),
            "peer location must not be implied from the collector site: {s}"
        );
    }

    #[test]
    fn sentence_uses_effect_specific_verb() {
        let s_absent = render_episode_sentence(&episode(EffectKind::TemporaryStreamAbsence));
        assert!(s_absent.contains("become absent"), "{s_absent}");
        let s_withdrawn = render_episode_sentence(&episode(EffectKind::RouteWithdrawal));
        assert!(s_withdrawn.contains("was withdrawn"), "{s_withdrawn}");
        let s_path = render_episode_sentence(&episode(EffectKind::PathReplacement));
        assert!(s_path.contains("change AS path"), "{s_path}");
        let s_depart = render_episode_sentence(&episode(EffectKind::NamedPlaneDeparture));
        assert!(s_depart.contains("leave the Plane A path"), "{s_depart}");
        let s_prepend = render_episode_sentence(&episode(EffectKind::PrependChange));
        assert!(
            s_prepend.contains("change AS-path prepending"),
            "{s_prepend}"
        );
        // "rerouted" is never a universal substitute.
        assert!(!s_absent.contains("rerouted"));
        assert!(!s_withdrawn.contains("rerouted"));
        assert!(!s_path.contains("rerouted"));
    }

    #[test]
    fn sentence_distinguishes_visibility_restoration_from_baseline_restoration() {
        let s = render_episode_sentence(&episode(EffectKind::TemporaryStreamAbsence));
        assert!(
            s.contains("return to visibility"),
            "visibility restoration wording: {s}"
        );
        assert!(
            !s.contains("restored its baseline path"),
            "baseline restoration is a different claim: {s}"
        );
        let ep = episode(EffectKind::PathReplacement);
        let s2 = render_episode_sentence(&ep);
        assert!(s2.contains("initial path class returned"), "{s2}");
    }

    #[test]
    fn sentence_names_stream_and_prefix_units_when_they_differ() {
        let mut ep = episode(EffectKind::TemporaryStreamAbsence);
        ep.changed_stream_count = 14;
        ep.distinct_prefix_count = 11;
        let s = render_episode_sentence(&ep);
        assert!(
            s.contains("14 observer-prefix streams covering 11 prefixes"),
            "both units must be named when they differ: {s}"
        );
    }

    #[test]
    fn sentence_never_claims_traffic_loss() {
        for kind in [
            EffectKind::TemporaryStreamAbsence,
            EffectKind::RouteWithdrawal,
            EffectKind::PathReplacement,
            EffectKind::NamedPlaneDeparture,
            EffectKind::NamedPlaneReturn,
            EffectKind::PrependChange,
            EffectKind::MixedRouteChange,
            EffectKind::NoRouteStateChange,
        ] {
            let s = render_episode_sentence(&episode(kind));
            assert!(
                !s.to_lowercase().contains("traffic lost")
                    && !s.to_lowercase().contains("traffic loss")
                    && !s.to_lowercase().contains("downtime"),
                "traffic-loss claim: {s}"
            );
        }
    }

    #[test]
    fn sentence_never_claims_causation() {
        for kind in [
            EffectKind::TemporaryStreamAbsence,
            EffectKind::RouteWithdrawal,
            EffectKind::PathReplacement,
            EffectKind::NamedPlaneDeparture,
            EffectKind::NoRouteStateChange,
        ] {
            let s = render_episode_sentence(&episode(kind));
            assert!(
                !s.to_lowercase().contains("caused")
                    && !s.to_lowercase().contains("because")
                    && !s.to_lowercase().contains("due to"),
                "causation claim: {s}"
            );
        }
    }

    #[test]
    fn unreviewed_peer_asn_is_never_claimed() {
        let mut ep = episode(EffectKind::TemporaryStreamAbsence);
        ep.peer_asn = None;
        let s = render_episode_sentence(&ep);
        assert!(s.contains("ASN unreviewed"), "{s}");
    }
}

/// Build a session-peer context map from reviewed audit rows.
///
/// Key: (collector, peer_ip) → (peer_asn, peer_label, peer_role,
/// relationship). The relationship is derived from the audit row's own
/// path-class evidence plus the profile; direct (peer ASN ∈ plane) and
/// indirect (plane ASN in path) are separate facts.
pub fn session_peer_context(
    profile: &ServicePlaneProfile,
    audit_rows: &[crate::catalog::netprofile::SessionAuditRow],
) -> BTreeMap<(String, String), (u32, String, String, RelationshipKind)> {
    let mut out = BTreeMap::new();
    for row in audit_rows {
        // Collect the row's per-plane evidence into relationship kinds.
        let mut rels: Vec<SessionRelationship> = Vec::new();
        for plane in &profile.service_planes {
            let in_path = row
                .path_class
                .per_plane_contains
                .iter()
                .any(|(id, _)| id == &plane.id);
            if plane.asns.contains(&row.peer_asn) {
                rels.push(SessionRelationship::DirectPeerToNamedPlane {
                    plane_id: plane.id.clone(),
                });
            } else if in_path {
                rels.push(SessionRelationship::IndirectPathViaNamedPlane {
                    plane_id: plane.id.clone(),
                });
            }
        }
        let relationship = relationship_kind(&rels);
        out.insert(
            (row.collector.clone(), row.peer_ip.clone()),
            (
                row.peer_asn,
                format!("AS{}", row.peer_asn),
                profile.role_label(row.peer_asn),
                relationship,
            ),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)] // test fixture builder
    fn stream(
        collector: &str,
        peer: &str,
        prefix: &str,
        category: &str,
        withdrawn: bool,
        restored: bool,
        transit_state: &str,
        transitions: i64,
    ) -> StreamLifecycleSummary {
        StreamLifecycleSummary {
            id: 0,
            run_id: 1,
            collector: collector.to_string(),
            peer_ip: peer.to_string(),
            prefix: prefix.to_string(),
            category: category.to_string(),
            baseline_instances: 1,
            max_active_instances: 1,
            transition_count: transitions,
            withdrawn,
            restored,
            transit_state: transit_state.to_string(),
            add_path_ambiguous: false,
            evidence_refs: "[]".to_string(),
        }
    }

    fn registry() -> CollectorLocationRegistry {
        CollectorLocationRegistry {
            as_of: "2019-09-05".to_string(),
            collectors: vec![
                crate::catalog::netprofile::CollectorLocation {
                    family: "ris".to_string(),
                    collector: "rrc06".to_string(),
                    location: "Otemachi, Tokyo, Japan".to_string(),
                    facility: "DIX-IE / JPIX".to_string(),
                    note: None,
                    region: "APAC".to_string(),
                    multihop: false,
                },
                crate::catalog::netprofile::CollectorLocation {
                    family: "ris".to_string(),
                    collector: "rrc00".to_string(),
                    location: "Amsterdam, Netherlands".to_string(),
                    facility: "RIPE-NCC Multihop".to_string(),
                    note: None,
                    region: "EMEA".to_string(),
                    multihop: true,
                },
            ],
        }
    }

    fn peers() -> BTreeMap<(String, String), (u32, String, String, RelationshipKind)> {
        let mut m = BTreeMap::new();
        m.insert(
            ("rrc06".to_string(), "192.0.2.1".to_string()),
            (
                64500,
                "AS64500".to_string(),
                "regional-re".to_string(),
                RelationshipKind::Direct,
            ),
        );
        m.insert(
            ("rrc06".to_string(), "192.0.2.2".to_string()),
            (
                64599,
                "AS64599".to_string(),
                "unclassified observed ASN".to_string(),
                RelationshipKind::Indirect,
            ),
        );
        m
    }

    #[test]
    fn episode_groups_same_observer_and_signature() {
        let streams = vec![
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/25",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/26",
                "Unchanged",
                false,
                false,
                "retained",
                0,
            ),
        ];
        let eps = build_episodes(1, &streams, &[], &[], &registry(), &peers(), "plane-a");
        // Two absent streams share one signature → one episode; the
        // unchanged stream forms a separate no-change episode.
        assert_eq!(eps.len(), 2);
        let absent = eps
            .iter()
            .find(|e| e.effect_kind == EffectKind::TemporaryStreamAbsence)
            .unwrap();
        assert_eq!(absent.baseline_stream_count, 2);
        assert_eq!(absent.changed_stream_count, 2);
        assert_eq!(absent.distinct_prefix_count, 2);
        assert_eq!(absent.observer_session, "catalog/rrc06 peer 192.0.2.1");
    }

    #[test]
    fn different_peers_at_one_collector_remain_separate() {
        let streams = vec![
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc06",
                "192.0.2.2",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
        ];
        let eps = build_episodes(1, &streams, &[], &[], &registry(), &peers(), "plane-a");
        assert_eq!(eps.len(), 2, "each peer session stays separate");
        assert!(eps
            .iter()
            .all(|e| e.observer_session.contains("peer 192.0.2.")));
        assert_ne!(eps[0].observer_session, eps[1].observer_session);
    }

    #[test]
    fn direct_and_indirect_observations_remain_separate() {
        let streams = vec![
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc06",
                "192.0.2.2",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
        ];
        let eps = build_episodes(1, &streams, &[], &[], &registry(), &peers(), "plane-a");
        let direct = eps.iter().find(|e| e.peer_asn == Some(64500)).unwrap();
        let indirect = eps.iter().find(|e| e.peer_asn == Some(64599)).unwrap();
        assert_eq!(direct.relationship, RelationshipKind::Direct);
        assert_eq!(indirect.relationship, RelationshipKind::Indirect);
        assert_ne!(direct.observer_session, indirect.observer_session);
    }

    #[test]
    fn episode_counts_distinct_prefixes_and_streams_separately() {
        // 3 streams, 2 distinct prefixes, all changed.
        let streams = vec![
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "DepartedTransitPath",
                false,
                false,
                "departed",
                1,
            ),
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/25",
                "PathChangedStillViaTransit",
                false,
                false,
                "retained",
                1,
            ),
        ];
        let eps = build_episodes(1, &streams, &[], &[], &registry(), &peers(), "plane-a");
        let total_streams: usize = eps.iter().map(|e| e.baseline_stream_count).sum();
        let total_prefixes: usize = eps.iter().map(|e| e.distinct_prefix_count).sum();
        assert_eq!(total_streams, 3, "streams counted per stream");
        assert_eq!(total_prefixes, 3, "prefixes counted per episode (2+1)");
    }

    #[test]
    fn episode_restoration_uses_existing_lifecycle_evidence() {
        let streams = vec![stream(
            "rrc06",
            "192.0.2.1",
            "198.51.100.0/24",
            "Withdrawn",
            true,
            true,
            "retained",
            2,
        )];
        let transitions = vec![
            crate::catalog::domain::RunTransitionRecord {
                id: 0,
                run_id: 1,
                seq: 0,
                kind: "Withdrawal".to_string(),
                occurred_utc: "2019-08-21T16:45:25Z".to_string(),
                run_phase: "Event".to_string(),
                collector: "rrc06".to_string(),
                peer_ip: "192.0.2.1".to_string(),
                prefix: "198.51.100.0/24".to_string(),
                path_id: None,
                material_path_changed: false,
                communities_changed: false,
                announced: false,
                withdrawn: true,
                observation_id: None,
                archive_sha256: None,
            },
            crate::catalog::domain::RunTransitionRecord {
                id: 1,
                run_id: 1,
                seq: 1,
                kind: "Restoration".to_string(),
                occurred_utc: "2019-08-21T16:45:27Z".to_string(),
                run_phase: "Event".to_string(),
                collector: "rrc06".to_string(),
                peer_ip: "192.0.2.1".to_string(),
                prefix: "198.51.100.0/24".to_string(),
                path_id: None,
                material_path_changed: false,
                communities_changed: false,
                announced: true,
                withdrawn: false,
                observation_id: None,
                archive_sha256: None,
            },
        ];
        let eps = build_episodes(
            1,
            &streams,
            &transitions,
            &[],
            &registry(),
            &peers(),
            "plane-a",
        );
        let ep = &eps[0];
        assert_eq!(ep.effect_kind, EffectKind::TemporaryStreamAbsence);
        assert_eq!(ep.first_change.as_deref(), Some("2019-08-21T16:45:25Z"));
        assert_eq!(ep.last_change.as_deref(), Some("2019-08-21T16:45:27Z"));
        assert_eq!(ep.unresolved_count, 0);
    }

    #[test]
    fn episode_generation_is_deterministic() {
        let streams = vec![
            stream(
                "rrc06",
                "192.0.2.2",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc00",
                "192.0.2.9",
                "198.51.100.0/24",
                "Unchanged",
                false,
                false,
                "retained",
                0,
            ),
        ];
        let a = build_episodes(1, &streams, &[], &[], &registry(), &peers(), "plane-a");
        let b = build_episodes(1, &streams, &[], &[], &registry(), &peers(), "plane-a");
        assert_eq!(a, b);
        // No-change episodes sort after changed episodes.
        let idx_unchanged = a
            .iter()
            .position(|e| e.effect_kind == EffectKind::NoRouteStateChange)
            .unwrap();
        assert!(a[..idx_unchanged]
            .iter()
            .all(|e| e.effect_kind != EffectKind::NoRouteStateChange));
    }

    #[test]
    fn no_change_and_unresolved_withdrawal_are_distinct_kinds() {
        let streams = vec![
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                false,
                "retained",
                1,
            ),
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/25",
                "Unchanged",
                false,
                false,
                "retained",
                0,
            ),
        ];
        let eps = build_episodes(1, &streams, &[], &[], &registry(), &peers(), "plane-a");
        assert!(eps
            .iter()
            .any(|e| e.effect_kind == EffectKind::RouteWithdrawal));
        assert!(eps
            .iter()
            .any(|e| e.effect_kind == EffectKind::NoRouteStateChange));
        let w = eps
            .iter()
            .find(|e| e.effect_kind == EffectKind::RouteWithdrawal)
            .unwrap();
        assert_eq!(w.unresolved_count, 1);
    }

    #[test]
    fn session_peer_context_maps_audit_rows() {
        let profile = ServicePlaneProfile {
            service_planes: vec![crate::catalog::netprofile::NamedServicePlane {
                id: "plane-a".to_string(),
                display_label: "Plane A".to_string(),
                asns: vec![64500],
            }],
            asn_roles: vec![crate::catalog::netprofile::ReviewedAsnRole {
                asn: 64500,
                role: "regional-re".to_string(),
            }],
            updated_utc: "2026-08-02T00:00:00Z".to_string(),
            provenance: "test".to_string(),
        };
        let rows = vec![crate::catalog::netprofile::SessionAuditRow {
            source_family: "ris".to_string(),
            collector: "rrc06".to_string(),
            location: "Otemachi, Tokyo, Japan".to_string(),
            rib_timestamp_utc: "2019-08-21T00:00:00Z".to_string(),
            rib_source_sha: "sha".to_string(),
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64500,
            address_family: "ipv4".to_string(),
            origin_route_count: 12,
            distinct_prefixes: 12,
            path_class: crate::catalog::netprofile::PathClassCounts {
                per_plane_contains: vec![("plane-a".to_string(), 12)],
                neither_plane: 0,
                total: 12,
            },
        }];
        let ctx = session_peer_context(&profile, &rows);
        let (asn, _label, role, rel) = ctx
            .get(&("rrc06".to_string(), "192.0.2.1".to_string()))
            .unwrap();
        assert_eq!(*asn, 64500);
        assert_eq!(role, "regional-re");
        assert_eq!(*rel, RelationshipKind::Direct);
    }
}
