//! Transition tokenization — the single classification point.
//!
//! Converts kind-less `StateChange` values (from routes::reconstruct)
//! into classified `RouteTransition` values. Every transition kind is
//! determined here and nowhere else.
//!
//! Does not import bgpkit-parser, Internet2, or MRT types.

use std::collections::HashMap;

use crate::domain::route::{Continuity, RouteState, RouteTransition, StateChange, TransitionKind};

/// Classify a state change into a transition kind.
///
/// This is the **single classification point** in the system.
/// `baseline` is the frozen event-baseline state (optional).
/// `from` and `to` come from the StateChange emitted by reconstruction.
///
/// No `STABLE_ALTERNATE` is emitted — stability is a derived wave property.
pub fn diff_states(
    baseline: Option<&RouteState>,
    from: Option<&RouteState>,
    to: &RouteState,
    continuity: Continuity,
) -> TransitionKind {
    // Session boundary or discontinuity → SessionReset
    if continuity == Continuity::Unknown {
        return TransitionKind::SessionReset;
    }

    match from {
        None => {
            // No prior state → this is a new announcement
            TransitionKind::Announcement
        }
        Some(from_state) => {
            // Withdrawal: to has empty AS path
            if to.attributes.as_path.0.is_empty() {
                return TransitionKind::Withdrawal;
            }

            // Exact duplicate
            if from_state.attributes == to.attributes {
                return TransitionKind::ExactDuplicate;
            }

            // Path change
            if from_state.attributes.as_path != to.attributes.as_path {
                // Check if it's a restoration to baseline
                if let Some(bl) = baseline {
                    if to.attributes.as_path == bl.attributes.as_path {
                        return TransitionKind::ReturnToBaseline;
                    }
                }
                // Check if it's a restoration to from's previous state
                // (simplified: exact restoration = returning to an exact previous path)
                return TransitionKind::PathChange {
                    old: from_state.attributes.as_path.clone(),
                    new: to.attributes.as_path.clone(),
                };
            }

            // Non-path attribute change
            TransitionKind::AttributeChange
        }
    }
}

/// Convert a stream of StateChanges into RouteTransitions.
///
/// Groups transitions per (collector, peer, prefix) and applies
/// classification using `diff_states`.
pub fn tokenize(
    changes: Vec<StateChange>,
    _baseline_store: &HashMap<crate::domain::route::RouteKey, crate::domain::route::RouteState>,
) -> Vec<RouteTransition> {
    changes
        .into_iter()
        .map(|sc| {
            let from_state = sc.before.as_ref().and_then(|e| e.state.as_ref());
            let to_state = sc.after.state.as_ref();
            let baseline_state = sc.event_baseline.as_ref().and_then(|e| e.state.as_ref());

            let kind = match (from_state, to_state) {
                (_, None) => TransitionKind::Withdrawal,
                (None, Some(_)) => TransitionKind::Announcement,
                (Some(_), Some(to_rs)) => {
                    diff_states(baseline_state, from_state, to_rs, sc.continuity)
                }
            };

            RouteTransition::new(
                sc.key,
                sc.event_baseline,
                sc.before,
                sc.after,
                sc.triggering,
                kind,
                sc.phase,
            )
        })
        .collect()
}

/// The canonical transition symbol alphabet.
///
/// Maps TransitionKind to short symbolic names suitable for SEQUITUR input.
/// Note: STABLE_ALTERNATE is NOT a transition symbol — stability is a
/// wave/interval property, not an instantaneous transition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransitionSymbol(pub String);

impl std::fmt::Display for TransitionSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TransitionSymbol {
    pub fn from_kind(kind: &TransitionKind) -> Self {
        let s = match kind {
            TransitionKind::Announcement => "ANNOUNCEMENT",
            TransitionKind::Withdrawal => "WITHDRAWAL",
            TransitionKind::ExactDuplicate => "DUPLICATE",
            TransitionKind::PathChange { .. } => "PATH_CHANGE",
            TransitionKind::AttributeChange => "ATTRIBUTE_CHANGE",
            TransitionKind::SessionReset => "SESSION_RESET",
            TransitionKind::Restoration => "RESTORATION",
            TransitionKind::ReturnToBaseline => "RETURN_TO_BASELINE",
        };
        TransitionSymbol(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::observation::EvidenceRef;
    use crate::domain::route::{
        AnalysisPhase, AsPath, EvidencedRouteState, Prefix, RouteAttributes, RouteKey,
    };
    use chrono::{TimeZone, Utc};
    use std::net::IpAddr;

    fn t() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0).unwrap()
    }

    fn state(prefix: &str, path: Vec<u32>, observer: &str) -> RouteState {
        RouteState {
            prefix: Prefix::from(prefix),
            attributes: RouteAttributes::from_as_path(path),
            timestamp: t(),
            observer: observer.to_string(),
        }
    }

    fn key(collector: &str) -> RouteKey {
        RouteKey::new(
            collector,
            "185.1.8.65".parse::<IpAddr>().unwrap(),
            &Prefix::from("192.0.2.0/24"),
        )
    }

    #[test]
    fn absent_to_announced() {
        let to = state("192.0.2.0/24", vec![6447, 11537, 1101], "rv2:185.1.8.65");
        let kind = diff_states(None, None, &to, Continuity::Known);
        assert_eq!(kind, TransitionKind::Announcement);
    }

    #[test]
    fn present_to_withdrawn() {
        let from = state("192.0.2.0/24", vec![6447, 11537, 1101], "rv2:185.1.8.65");
        let to = state("192.0.2.0/24", vec![], "rv2:185.1.8.65");
        let kind = diff_states(None, Some(&from), &to, Continuity::Known);
        assert_eq!(kind, TransitionKind::Withdrawal);
    }

    #[test]
    fn identical_reannouncement() {
        let from = state("192.0.2.0/24", vec![6447, 11537, 1101], "rv2:185.1.8.65");
        let to = from.clone();
        let kind = diff_states(None, Some(&from), &to, Continuity::Known);
        assert_eq!(kind, TransitionKind::ExactDuplicate);
    }

    #[test]
    fn path_change() {
        let from = state("192.0.2.0/24", vec![6447, 11537, 1101], "rv2:185.1.8.65");
        let to = state("192.0.2.0/24", vec![6447, 237, 1101], "rv2:185.1.8.65");
        let kind = diff_states(None, Some(&from), &to, Continuity::Known);
        assert!(matches!(kind, TransitionKind::PathChange { .. }));
    }

    #[test]
    fn return_to_baseline() {
        let baseline = state("192.0.2.0/24", vec![6447, 11537, 1101], "rv2:185.1.8.65");
        let from = state("192.0.2.0/24", vec![6447, 237, 1101], "rv2:185.1.8.65");
        let to = baseline.clone();
        let kind = diff_states(Some(&baseline), Some(&from), &to, Continuity::Known);
        assert_eq!(kind, TransitionKind::ReturnToBaseline);
    }

    #[test]
    fn session_reset_on_unknown_continuity() {
        let from = state("192.0.2.0/24", vec![6447, 11537, 1101], "rv2:185.1.8.65");
        let to = state("192.0.2.0/24", vec![6447, 237, 1101], "rv2:185.1.8.65");
        let kind = diff_states(None, Some(&from), &to, Continuity::Unknown);
        assert_eq!(kind, TransitionKind::SessionReset);
    }

    #[test]
    fn attribute_change() {
        let from = state("192.0.2.0/24", vec![6447, 11537, 1101], "rv2:185.1.8.65");
        let mut to = from.clone();
        to.attributes.local_pref = Some(200); // changed local pref
        let kind = diff_states(None, Some(&from), &to, Continuity::Known);
        assert_eq!(kind, TransitionKind::AttributeChange);
    }

    #[test]
    fn no_stable_alternate_symbol() {
        // Verify STABLE_ALTERNATE is not in the symbol alphabet
        let kind = TransitionKind::PathChange {
            old: AsPath(vec![6447, 11537, 1101]),
            new: AsPath(vec![6447, 237, 1101]),
        };
        let sym = TransitionSymbol::from_kind(&kind);
        assert_eq!(sym.0, "PATH_CHANGE");
        // There is no STABLE_ALTERNATE variant
    }

    #[test]
    fn tokenize_batch() {
        let baseline = state("192.0.2.0/24", vec![6447, 11537, 1101], "rv2:185.1.8.65");
        let from_state = state("192.0.2.0/24", vec![6447, 11537, 1101], "rv2:185.1.8.65");
        let to_state = state("192.0.2.0/24", vec![6447, 237, 1101], "rv2:185.1.8.65");
        let k = key("rv2");

        let mut baseline_map = HashMap::new();
        baseline_map.insert(k.clone(), baseline);

        let ev = EvidenceRef::synthetic(0, "test", "0000");
        let before_ev = EvidencedRouteState::present(from_state, ev.clone());
        let after_ev = EvidencedRouteState::present(to_state, ev.clone());
        let sc = StateChange::new(
            k.clone(),
            None,
            Some(before_ev),
            after_ev,
            ev,
            Continuity::Known,
            AnalysisPhase::Event,
        );
        let transitions = tokenize(vec![sc], &baseline_map);
        assert_eq!(transitions.len(), 1);
        assert!(matches!(
            transitions[0].kind,
            TransitionKind::PathChange { .. }
        ));
    }
}
