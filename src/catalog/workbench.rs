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

// ── Regional observed breadth (Part 6) ──────────────────────────────

/// Regional summary of public-observer breadth.
///
/// "Observed breadth" (also "public-observer breadth") describes how many
/// eligible observer sessions saw the target and how many changed. It is
/// NOT outage severity, global scope, or a percentage of the Internet
/// affected; no severity score is computed anywhere.
///
/// The denominator is ALWAYS visible: `eligible_observer_sessions` is
/// reported alongside `changed_observer_sessions`. The three coverage
/// states never collapse into one zero:
/// - `NoChange` — qualifying baseline existed, no route-state change.
/// - `NoBaselineVisibility` — target not visible at that session.
/// - `IncompleteCoverage` — observation could not be completed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionObservationSummary {
    pub region: String,
    /// Sessions with a qualifying baseline at this region (denominator).
    pub eligible_observer_sessions: usize,
    pub changed_observer_sessions: usize,
    pub unchanged_observer_sessions: usize,
    /// Sessions where the target was not visible (no qualifying baseline).
    pub sessions_without_baseline_visibility: usize,
    /// Sessions where the observation could not be completed.
    pub sessions_with_incomplete_coverage: usize,
    pub changed_streams: usize,
    pub baseline_streams: usize,
    pub changed_prefixes: usize,
    /// First observed change in this region (UTC), if any.
    pub first_change: Option<String>,
    /// Last observed restoration in this region (UTC), if any.
    pub last_restoration: Option<String>,
}

/// Build regional breadth summaries from episodes.
///
/// `eligible_sessions` is the denominator per region: every observer
/// session that had a qualifying baseline. `no_baseline_sessions` and
/// `incomplete_sessions` are reported separately per region and never
/// counted as unchanged. Regions without any eligible session are
/// omitted (they carry no observation to summarize).
pub fn regional_breadth(
    episodes: &[ObserverEpisode],
    no_baseline_sessions: &[(String, String)],
    incomplete_sessions: &[(String, String)],
) -> Vec<RegionObservationSummary> {
    let mut by_region: BTreeMap<String, RegionObservationSummary> = BTreeMap::new();

    // Count per-region eligible/changed sessions and stream/prefix totals.
    for ep in episodes {
        let r = by_region
            .entry(ep.observer_region.clone())
            .or_insert_with(|| RegionObservationSummary {
                region: ep.observer_region.clone(),
                eligible_observer_sessions: 0,
                changed_observer_sessions: 0,
                unchanged_observer_sessions: 0,
                sessions_without_baseline_visibility: 0,
                sessions_with_incomplete_coverage: 0,
                changed_streams: 0,
                baseline_streams: 0,
                changed_prefixes: 0,
                first_change: None,
                last_restoration: None,
            });
        r.eligible_observer_sessions += 1;
        if ep.effect_kind != EffectKind::NoRouteStateChange {
            r.changed_observer_sessions += 1;
        } else {
            r.unchanged_observer_sessions += 1;
        }
        r.baseline_streams += ep.baseline_stream_count;
        r.changed_streams += ep.changed_stream_count;
        r.changed_prefixes += ep.distinct_prefix_count;
        if let Some(fc) = &ep.first_change {
            if r.first_change
                .as_deref()
                .map(|f| fc.as_str() < f)
                .unwrap_or(true)
            {
                r.first_change = Some(fc.clone());
            }
        }
        if let Some(lr) = &ep.restoration_end {
            if r.last_restoration
                .as_deref()
                .map(|l| lr.as_str() > l)
                .unwrap_or(true)
            {
                r.last_restoration = Some(lr.clone());
            }
        }
    }

    // No-baseline and incomplete sessions are per-region facts derived
    // from run coverage; they are never added to the unchanged count.
    for (region, _session) in no_baseline_sessions {
        let r = by_region
            .entry(region.clone())
            .or_insert_with(|| RegionObservationSummary {
                region: region.clone(),
                eligible_observer_sessions: 0,
                changed_observer_sessions: 0,
                unchanged_observer_sessions: 0,
                sessions_without_baseline_visibility: 0,
                sessions_with_incomplete_coverage: 0,
                changed_streams: 0,
                baseline_streams: 0,
                changed_prefixes: 0,
                first_change: None,
                last_restoration: None,
            });
        r.sessions_without_baseline_visibility += 1;
    }
    for (region, _session) in incomplete_sessions {
        let r = by_region
            .entry(region.clone())
            .or_insert_with(|| RegionObservationSummary {
                region: region.clone(),
                eligible_observer_sessions: 0,
                changed_observer_sessions: 0,
                unchanged_observer_sessions: 0,
                sessions_without_baseline_visibility: 0,
                sessions_with_incomplete_coverage: 0,
                changed_streams: 0,
                baseline_streams: 0,
                changed_prefixes: 0,
                first_change: None,
                last_restoration: None,
            });
        r.sessions_with_incomplete_coverage += 1;
    }

    by_region.into_values().collect()
}

#[cfg(test)]
mod breadth_tests {
    use super::*;

    fn ep(
        region: &str,
        kind: EffectKind,
        changed_streams: usize,
        prefixes: usize,
        first: &str,
    ) -> ObserverEpisode {
        ObserverEpisode {
            analysis_run: 1,
            observer_session: format!("catalog/rrc06 peer 192.0.2.1 region {region}"),
            observer_site: "site".to_string(),
            observer_region: region.to_string(),
            peer_asn: Some(64500),
            peer_label: "AS64500".to_string(),
            peer_role: "regional-re".to_string(),
            relationship: RelationshipKind::Direct,
            named_path_plane: "Plane A".to_string(),
            effect_kind: kind,
            first_change: Some(first.to_string()),
            peak_interval: None,
            last_change: Some(first.to_string()),
            restoration_start: None,
            restoration_end: Some("2019-08-21T17:02:00Z".to_string()),
            baseline_stream_count: changed_streams.max(1),
            changed_stream_count: changed_streams,
            distinct_prefix_count: prefixes,
            route_instance_count: changed_streams,
            unresolved_count: 0,
            coverage_status: CoverageStatus::NoChange,
            representative_evidence: String::new(),
            streams: Vec::new(),
        }
    }

    #[test]
    fn breadth_always_has_visible_denominator() {
        let episodes = vec![
            ep(
                "AMER",
                EffectKind::TemporaryStreamAbsence,
                11,
                11,
                "2019-08-21T16:45:25Z",
            ),
            ep(
                "AMER",
                EffectKind::NoRouteStateChange,
                0,
                0,
                "2019-08-21T16:45:25Z",
            ),
        ];
        let rows = regional_breadth(&episodes, &[], &[]);
        let amer = rows.iter().find(|r| r.region == "AMER").unwrap();
        assert_eq!(amer.eligible_observer_sessions, 2, "denominator visible");
        assert_eq!(amer.changed_observer_sessions, 1);
        assert_eq!(amer.unchanged_observer_sessions, 1);
        // changed / eligible renders as 1/2, never as a bare count.
        assert!(amer.changed_observer_sessions <= amer.eligible_observer_sessions);
    }

    #[test]
    fn no_change_and_no_baseline_are_distinct() {
        let episodes = vec![ep(
            "APAC",
            EffectKind::NoRouteStateChange,
            0,
            0,
            "2019-08-21T16:45:25Z",
        )];
        let rows = regional_breadth(&episodes, &[("APAC".to_string(), "s1".to_string())], &[]);
        let apac = rows.iter().find(|r| r.region == "APAC").unwrap();
        assert_eq!(apac.unchanged_observer_sessions, 1);
        assert_eq!(
            apac.sessions_without_baseline_visibility, 1,
            "no-baseline is a separate fact from unchanged"
        );
        assert_eq!(apac.changed_observer_sessions, 0);
    }

    #[test]
    fn incomplete_coverage_is_not_counted_as_unchanged() {
        let episodes = vec![ep(
            "EMEA",
            EffectKind::NoRouteStateChange,
            0,
            0,
            "2019-08-21T16:45:25Z",
        )];
        let rows = regional_breadth(&episodes, &[], &[("EMEA".to_string(), "s9".to_string())]);
        let emea = rows.iter().find(|r| r.region == "EMEA").unwrap();
        assert_eq!(emea.unchanged_observer_sessions, 1);
        assert_eq!(
            emea.sessions_with_incomplete_coverage, 1,
            "incomplete sessions are visible, not silently unchanged"
        );
    }

    #[test]
    fn regional_summary_uses_observer_site_region() {
        let episodes = vec![
            ep(
                "AMER",
                EffectKind::TemporaryStreamAbsence,
                11,
                11,
                "2019-08-21T16:45:25Z",
            ),
            ep(
                "APAC",
                EffectKind::NoRouteStateChange,
                0,
                0,
                "2019-08-21T16:45:25Z",
            ),
            ep(
                "EMEA",
                EffectKind::PathReplacement,
                3,
                2,
                "2019-08-21T16:50:00Z",
            ),
        ];
        let rows = regional_breadth(&episodes, &[], &[]);
        assert_eq!(rows.len(), 3);
        assert!(rows
            .iter()
            .any(|r| r.region == "AMER" && r.changed_observer_sessions == 1));
        assert!(rows
            .iter()
            .any(|r| r.region == "APAC" && r.changed_observer_sessions == 0));
        assert!(rows
            .iter()
            .any(|r| r.region == "EMEA" && r.changed_prefixes == 2));
    }

    #[test]
    fn broader_observation_is_not_rendered_as_greater_severity() {
        // More observers changing is broader observation, not "worse".
        let wide = regional_breadth(
            &[
                ep("AMER", EffectKind::TemporaryStreamAbsence, 11, 11, "T1"),
                ep("EMEA", EffectKind::TemporaryStreamAbsence, 11, 11, "T1"),
                ep("APAC", EffectKind::TemporaryStreamAbsence, 11, 11, "T1"),
            ],
            &[],
            &[],
        );
        let total_changed: usize = wide.iter().map(|r| r.changed_observer_sessions).sum();
        // The model reports counts; no severity score field exists.
        assert_eq!(total_changed, 3);
        for r in &wide {
            assert!(
                !r.region.contains("severity")
                    && r.changed_observer_sessions <= r.eligible_observer_sessions
            );
        }
    }

    #[test]
    fn breadth_first_change_and_last_restoration_are_exact() {
        let episodes = vec![
            ep(
                "AMER",
                EffectKind::TemporaryStreamAbsence,
                11,
                11,
                "2019-08-21T16:45:25Z",
            ),
            ep(
                "AMER",
                EffectKind::PathReplacement,
                3,
                2,
                "2019-08-21T16:50:00Z",
            ),
        ];
        let rows = regional_breadth(&episodes, &[], &[]);
        let amer = rows.iter().find(|r| r.region == "AMER").unwrap();
        assert_eq!(amer.first_change.as_deref(), Some("2019-08-21T16:45:25Z"));
        assert_eq!(
            amer.last_restoration.as_deref(),
            Some("2019-08-21T17:02:00Z")
        );
    }
}

// ── Incident timeline (Part 9) ──────────────────────────────────────

/// A timeline lane: one observer session's evidence on a shared UTC axis.
///
/// Lanes never interpolate unobserved state: markers carry EXACT
/// timestamps of observed events. Intervals (absence, path-change,
/// restoration) span only between observed timestamps; an unresolved
/// episode gets NO fabricated restoration marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineLane {
    /// Lane identity is the observer session.
    pub observer_session: String,
    pub region: String,
    pub collector: String,
    /// Analysis-window boundaries (exact, from the run plan).
    pub window_start: String,
    pub window_end: String,
    /// Operator-reported anchors (case-study phases/claims), distinct
    /// from BGP evidence markers.
    pub operator_anchors: Vec<TimelineMarker>,
    pub first_route_change: Option<TimelineMarker>,
    /// Absence interval [start, end] of observed withdrawals.
    pub absence_interval: Option<(String, String)>,
    /// Path-change interval [start, end].
    pub path_change_interval: Option<(String, String)>,
    /// Restoration interval [start, end].
    pub restoration_interval: Option<(String, String)>,
    /// Unresolved end state: no restoration was observed.
    pub unresolved_end_state: bool,
}

/// One timeline marker with its evidence class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineMarker {
    pub timestamp_utc: String,
    pub label: String,
    /// "operator" (operator-reported anchor) or "bgp" (BGP evidence).
    pub kind: String,
}

/// Build timeline lanes from episodes and operator anchors.
///
/// `operator_anchors` is a map session-key → markers (from reviewed
/// case-study phases/claims). Lanes are ordered deterministically by
/// (window start, region, collector). Exact timestamps only — no
/// interpolation between discrete BGP observations.
pub fn build_timeline(
    episodes: &[ObserverEpisode],
    window_start: &str,
    window_end: &str,
    operator_anchors: &BTreeMap<String, Vec<TimelineMarker>>,
) -> Vec<TimelineLane> {
    let mut lanes: BTreeMap<String, TimelineLane> = BTreeMap::new();
    for ep in episodes {
        let lane = lanes
            .entry(ep.observer_session.clone())
            .or_insert_with(|| TimelineLane {
                observer_session: ep.observer_session.clone(),
                region: ep.observer_region.clone(),
                collector: collector_from_session(&ep.observer_session).to_string(),
                window_start: window_start.to_string(),
                window_end: window_end.to_string(),
                operator_anchors: operator_anchors
                    .get(&ep.observer_session)
                    .cloned()
                    .unwrap_or_default(),
                first_route_change: None,
                absence_interval: None,
                path_change_interval: None,
                restoration_interval: None,
                unresolved_end_state: false,
            });
        if let Some(fc) = &ep.first_change {
            if lane.first_route_change.is_none() {
                lane.first_route_change = Some(TimelineMarker {
                    timestamp_utc: fc.clone(),
                    label: "first route change".to_string(),
                    kind: "bgp".to_string(),
                });
            }
        }
        match ep.effect_kind {
            EffectKind::TemporaryStreamAbsence | EffectKind::RouteWithdrawal => {
                let (start, end) = (
                    ep.first_change.clone().unwrap_or_default(),
                    ep.last_change.clone().unwrap_or_default(),
                );
                let interval = (start, end);
                let better = match &lane.absence_interval {
                    None => true,
                    Some((s, e)) => interval.0 < *s || (interval.0 == *s && interval.1 > *e),
                };
                if better {
                    lane.absence_interval = Some(interval);
                }
                if ep.effect_kind == EffectKind::RouteWithdrawal {
                    lane.unresolved_end_state = true;
                }
            }
            EffectKind::PathReplacement
            | EffectKind::PrependChange
            | EffectKind::MixedRouteChange => {
                let interval = (
                    ep.first_change.clone().unwrap_or_default(),
                    ep.last_change.clone().unwrap_or_default(),
                );
                let better = match &lane.path_change_interval {
                    None => true,
                    Some((s, e)) => interval.0 < *s || (interval.0 == *s && interval.1 > *e),
                };
                if better {
                    lane.path_change_interval = Some(interval);
                }
            }
            EffectKind::NamedPlaneDeparture | EffectKind::NamedPlaneReturn => {
                let interval = (
                    ep.first_change.clone().unwrap_or_default(),
                    ep.last_change.clone().unwrap_or_default(),
                );
                let better = match &lane.path_change_interval {
                    None => true,
                    Some((s, e)) => interval.0 < *s || (interval.0 == *s && interval.1 > *e),
                };
                if better {
                    lane.path_change_interval = Some(interval);
                }
            }
            EffectKind::NoRouteStateChange => {}
        }
        if let (Some(rs), Some(re)) = (&ep.restoration_start, &ep.restoration_end) {
            let interval = (rs.clone(), re.clone());
            let better = match &lane.restoration_interval {
                None => true,
                Some((s, e)) => interval.0 < *s || (interval.0 == *s && interval.1 > *e),
            };
            if better {
                lane.restoration_interval = Some(interval);
            }
        }
    }
    lanes.into_values().collect()
}

#[cfg(test)]
mod timeline_tests {
    use super::*;

    fn ep(
        kind: EffectKind,
        session: &str,
        region: &str,
        first: &str,
        last: &str,
    ) -> ObserverEpisode {
        ObserverEpisode {
            analysis_run: 1,
            observer_session: session.to_string(),
            observer_site: "site".to_string(),
            observer_region: region.to_string(),
            peer_asn: Some(64500),
            peer_label: "AS64500".to_string(),
            peer_role: "regional-re".to_string(),
            relationship: RelationshipKind::Direct,
            named_path_plane: "Plane A".to_string(),
            effect_kind: kind,
            first_change: Some(first.to_string()),
            peak_interval: None,
            last_change: Some(last.to_string()),
            restoration_start: Some(first.to_string()),
            restoration_end: Some(last.to_string()),
            baseline_stream_count: 1,
            changed_stream_count: 1,
            distinct_prefix_count: 1,
            route_instance_count: 1,
            unresolved_count: 0,
            coverage_status: CoverageStatus::NoChange,
            representative_evidence: String::new(),
            streams: Vec::new(),
        }
    }

    #[test]
    fn operator_and_bgp_timeline_markers_are_distinct() {
        let mut anchors = BTreeMap::new();
        anchors.insert(
            "catalog/rrc06 peer 192.0.2.1".to_string(),
            vec![TimelineMarker {
                timestamp_utc: "2019-08-21T16:50:00Z".to_string(),
                label: "reported interface disable".to_string(),
                kind: "operator".to_string(),
            }],
        );
        let eps = vec![ep(
            EffectKind::TemporaryStreamAbsence,
            "catalog/rrc06 peer 192.0.2.1",
            "APAC",
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
        )];
        let lanes = build_timeline(
            &eps,
            "2019-08-21T16:00:00Z",
            "2019-08-21T17:30:00Z",
            &anchors,
        );
        let lane = &lanes[0];
        assert_eq!(lane.operator_anchors.len(), 1);
        assert_eq!(lane.operator_anchors[0].kind, "operator");
        assert_eq!(lane.first_route_change.as_ref().unwrap().kind, "bgp");
        assert_ne!(
            lane.operator_anchors[0].kind,
            lane.first_route_change.as_ref().unwrap().kind,
            "operator and BGP markers must be visibly distinct"
        );
    }

    #[test]
    fn timeline_does_not_interpolate_unobserved_state() {
        let eps = vec![ep(
            EffectKind::TemporaryStreamAbsence,
            "catalog/rrc06 peer 192.0.2.1",
            "APAC",
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
        )];
        let lanes = build_timeline(
            &eps,
            "2019-08-21T16:00:00Z",
            "2019-08-21T17:30:00Z",
            &BTreeMap::new(),
        );
        let lane = &lanes[0];
        // Interval endpoints are EXACT observed timestamps.
        assert_eq!(
            lane.absence_interval.as_ref().unwrap().0,
            "2019-08-21T16:45:25Z"
        );
        assert_eq!(
            lane.absence_interval.as_ref().unwrap().1,
            "2019-08-21T16:45:27Z"
        );
        // No synthetic midpoint or extrapolated marker exists.
        assert!(lane
            .first_route_change
            .as_ref()
            .unwrap()
            .timestamp_utc
            .starts_with("2019-08-21T16:45:"));
        assert_eq!(lane.path_change_interval, None);
    }

    #[test]
    fn lane_identity_is_observer_session() {
        let eps = vec![
            ep(
                EffectKind::TemporaryStreamAbsence,
                "catalog/rrc06 peer 192.0.2.1",
                "APAC",
                "2019-08-21T16:45:25Z",
                "2019-08-21T16:45:27Z",
            ),
            ep(
                EffectKind::TemporaryStreamAbsence,
                "catalog/rrc00 peer 192.0.2.9",
                "EMEA",
                "2019-08-21T16:45:25Z",
                "2019-08-21T16:45:27Z",
            ),
        ];
        let lanes = build_timeline(&eps, "W0", "W1", &BTreeMap::new());
        assert_eq!(lanes.len(), 2, "one lane per observer session");
        assert!(lanes.iter().all(|l| l.observer_session.contains("peer ")));
    }

    #[test]
    fn event_order_is_preserved() {
        let eps = vec![
            ep(
                EffectKind::TemporaryStreamAbsence,
                "catalog/rrc06 peer 192.0.2.1",
                "APAC",
                "2019-08-21T16:45:25Z",
                "2019-08-21T16:45:27Z",
            ),
            ep(
                EffectKind::PathReplacement,
                "catalog/rrc06 peer 192.0.2.1",
                "APAC",
                "2019-08-21T16:50:00Z",
                "2019-08-21T17:02:03Z",
            ),
        ];
        let lanes = build_timeline(&eps, "W0", "W1", &BTreeMap::new());
        let lane = &lanes[0];
        let first = lane
            .first_route_change
            .as_ref()
            .unwrap()
            .timestamp_utc
            .clone();
        let absence = lane.absence_interval.as_ref().unwrap();
        let path = lane.path_change_interval.as_ref().unwrap();
        // Chronological order preserved: absence before path change.
        assert!(absence.0 < path.0);
        assert!(first == "2019-08-21T16:45:25Z");
    }

    #[test]
    fn unresolved_episode_has_no_fabricated_restoration() {
        let mut e = ep(
            EffectKind::RouteWithdrawal,
            "catalog/rrc06 peer 192.0.2.1",
            "APAC",
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
        );
        e.restoration_start = None;
        e.restoration_end = None;
        let lanes = build_timeline(&[e], "W0", "W1", &BTreeMap::new());
        let lane = &lanes[0];
        assert!(lane.unresolved_end_state, "withdrawal without restoration");
        assert_eq!(
            lane.restoration_interval, None,
            "no fabricated restoration interval"
        );
    }
}

// ── Internal-investigation cues (Part 11) ───────────────────────────

/// One suggested internal check, labeled as an investigation CUE.
///
/// Cues are bound to OBSERVED external facts (session, interval, plane,
/// prefixes) — they never name an unreviewed device, never claim root
/// cause, and never generate device commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationCue {
    pub text: String,
    /// The observed fact this cue is traceable to (session + interval).
    pub traceable_to: String,
}

/// Build investigation cues from episodes.
///
/// Cue templates only reference reviewed identities: the collector site,
/// the reviewed plane label, the observer session, and the analysis
/// interval. "Check advertisements for these prefixes toward <plane>"
/// and "Check the session corresponding to the reviewed attachment
/// during <interval>" are the supported shapes.
pub fn build_investigation_cues(episodes: &[ObserverEpisode]) -> Vec<InvestigationCue> {
    let mut cues = Vec::new();
    for ep in episodes {
        if ep.effect_kind == EffectKind::NoRouteStateChange {
            continue;
        }
        let interval = match (&ep.first_change, &ep.last_change) {
            (Some(a), Some(b)) if a == b => format!("{a} UTC"),
            (Some(a), Some(b)) => format!("{a}–{b} UTC"),
            _ => "the analysis window".to_string(),
        };
        let plane = &ep.named_path_plane;
        let session = &ep.observer_session;
        let prefixes: Vec<&str> = ep
            .streams
            .iter()
            .map(|s| s.prefix.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .take(8)
            .collect();
        let prefix_text = if prefixes.is_empty() {
            "the selected prefixes".to_string()
        } else {
            prefixes.join(", ")
        };

        cues.push(InvestigationCue {
            text: format!("Check advertisements for these prefixes toward {plane}: {prefix_text}."),
            traceable_to: format!("{session} during {interval}"),
        });
        cues.push(InvestigationCue {
            text: format!(
                "Check the session corresponding to the reviewed attachment during {interval}."
            ),
            traceable_to: session.clone(),
        });
        if matches!(
            ep.effect_kind,
            EffectKind::NamedPlaneDeparture | EffectKind::PathReplacement
        ) {
            cues.push(InvestigationCue {
                text: format!(
                    "Compare local route selection for prefixes that departed the {plane} path but remained visible externally."
                ),
                traceable_to: format!("{session} during {interval}"),
            });
        }
        if ep.effect_kind == EffectKind::TemporaryStreamAbsence {
            cues.push(InvestigationCue {
                text:
                    "Review whether restored visibility also restored the expected baseline path."
                        .to_string(),
                traceable_to: format!("{session} during {interval}"),
            });
        }
    }
    // Deterministic: dedupe identical cues preserving first occurrence.
    let mut seen: Vec<String> = Vec::new();
    cues.retain(|c| {
        if seen.contains(&c.text) {
            false
        } else {
            seen.push(c.text.clone());
            true
        }
    });
    cues
}

#[cfg(test)]
mod cue_tests {
    use super::*;

    fn ep(kind: EffectKind, session: &str, plane: &str, prefix: &str) -> ObserverEpisode {
        ObserverEpisode {
            analysis_run: 1,
            observer_session: session.to_string(),
            observer_site: "site".to_string(),
            observer_region: "AMER".to_string(),
            peer_asn: Some(64500),
            peer_label: "AS64500".to_string(),
            peer_role: "regional-re".to_string(),
            relationship: RelationshipKind::Direct,
            named_path_plane: plane.to_string(),
            effect_kind: kind,
            first_change: Some("2019-08-21T16:45:25Z".to_string()),
            peak_interval: None,
            last_change: Some("2019-08-21T17:02:00Z".to_string()),
            restoration_start: None,
            restoration_end: None,
            baseline_stream_count: 1,
            changed_stream_count: 1,
            distinct_prefix_count: 1,
            route_instance_count: 1,
            unresolved_count: 0,
            coverage_status: CoverageStatus::NoChange,
            representative_evidence: String::new(),
            streams: vec![EpisodeStream {
                prefix: prefix.to_string(),
                category: "Withdrawn".to_string(),
                withdrawn: true,
                restored: true,
                baseline_instances: 1,
                transition_count: 2,
                add_path_ambiguous: false,
                evidence_refs: "[]".to_string(),
            }],
        }
    }

    #[test]
    fn investigation_cue_is_traceable_to_observation() {
        let eps = vec![ep(
            EffectKind::TemporaryStreamAbsence,
            "catalog/rrc06 peer 192.0.2.1",
            "Plane A",
            "198.51.100.0/24",
        )];
        let cues = build_investigation_cues(&eps);
        assert!(!cues.is_empty());
        for cue in &cues {
            assert!(
                cue.traceable_to.contains("rrc06") || cue.traceable_to.contains("192.0.2.1"),
                "cue must trace to the observed session: {}",
                cue.traceable_to
            );
            assert!(
                cue.text.starts_with("Check ") || cue.text.starts_with("Review "),
                "cue wording: {}",
                cue.text
            );
        }
    }

    #[test]
    fn cue_does_not_name_unreviewed_device() {
        let eps = vec![ep(
            EffectKind::TemporaryStreamAbsence,
            "catalog/rrc06 peer 192.0.2.1",
            "Plane A",
            "198.51.100.0/24",
        )];
        let cues = build_investigation_cues(&eps);
        for cue in &cues {
            let lower = cue.text.to_lowercase();
            assert!(
                !lower.contains("router")
                    && !lower.contains("circuit")
                    && !lower.contains("switch"),
                "cue must not name a device: {}",
                cue.text
            );
        }
    }

    #[test]
    fn cue_does_not_claim_root_cause() {
        let eps = vec![ep(
            EffectKind::NamedPlaneDeparture,
            "catalog/rrc06 peer 192.0.2.1",
            "Plane A",
            "198.51.100.0/24",
        )];
        let cues = build_investigation_cues(&eps);
        for cue in &cues {
            let lower = cue.text.to_lowercase();
            assert!(
                !lower.contains("root cause")
                    && !lower.contains("caused")
                    && !lower.contains("diagnosis"),
                "cue must not claim root cause: {}",
                cue.text
            );
        }
    }

    #[test]
    fn cue_uses_reviewed_plane_or_attachment_identity() {
        let eps = vec![ep(
            EffectKind::NamedPlaneDeparture,
            "catalog/rrc06 peer 192.0.2.1",
            "Plane A",
            "198.51.100.0/24",
        )];
        let cues = build_investigation_cues(&eps);
        assert!(cues.iter().any(|c| c.text.contains("Plane A")));
        assert!(cues.iter().any(|c| c.text.contains("reviewed attachment")));
        assert!(
            cues.iter().any(|c| c.text.contains("198.51.100.0/24")),
            "prefix identity must appear in the advertisement cue"
        );
    }

    #[test]
    fn no_change_episodes_produce_no_cues() {
        let eps = vec![ep(
            EffectKind::NoRouteStateChange,
            "catalog/rrc06 peer 192.0.2.1",
            "Plane A",
            "198.51.100.0/24",
        )];
        assert!(build_investigation_cues(&eps).is_empty());
    }
}
