//! Route types — prefixes, AS paths, route state, and transitions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A BGP prefix (e.g. "192.0.2.0/24").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Prefix(pub String);

impl From<&str> for Prefix {
    fn from(s: &str) -> Self {
        Prefix(s.to_string())
    }
}

impl std::fmt::Display for Prefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An AS path (sequence of ASNs).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AsPath(pub Vec<u32>);

impl std::fmt::Display for AsPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path: Vec<String> = self.0.iter().map(|asn| asn.to_string()).collect();
        write!(f, "{}", path.join(" "))
    }
}

/// Route attributes observed at a point in time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteAttributes {
    pub as_path: AsPath,
    /// The origin AS (rightmost AS in the path).
    pub origin_as: u32,
    /// Multi-exit discriminator.
    pub med: Option<u32>,
    /// Local preference.
    pub local_pref: Option<u32>,
    /// Communities as string representations (e.g. "11537:1000").
    pub communities: Vec<String>,
}

impl RouteAttributes {
    /// Create bare route attributes from an AS path.
    pub fn from_as_path(as_path: Vec<u32>) -> Self {
        let origin_as = *as_path.last().unwrap_or(&0);
        RouteAttributes {
            as_path: AsPath(as_path),
            origin_as,
            med: None,
            local_pref: None,
            communities: vec![],
        }
    }
}

/// The state of a route as observed by a specific collector/peer at a specific time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteState {
    pub prefix: Prefix,
    pub attributes: RouteAttributes,
    pub timestamp: DateTime<Utc>,
    /// The observer that reported this state (collector:peer, e.g. "route-views2:AS6447").
    pub observer: String,
}

/// The kind of route transition between two states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionKind {
    /// A previously absent route is announced.
    Announcement,
    /// A route is withdrawn.
    Withdrawal,
    /// An exact duplicate of the previous state (no change).
    ExactDuplicate,
    /// The AS path changed (e.g. failover to alternate).
    PathChange {
        old: AsPath,
        new: AsPath,
    },
    /// Non-path attributes changed (no path difference).
    AttributeChange,
    /// Observer session discontinuity — not a real route change.
    SessionReset,
    /// A previously withdrawn route is restored with its original path.
    Restoration,
}

/// A transition from one route state to another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteTransition {
    /// The previous state, if any (None = no prior state known).
    pub from: Option<RouteState>,
    /// The new state.
    pub to: RouteState,
    /// The kind of transition that occurred.
    pub kind: TransitionKind,
}

impl RouteTransition {
    pub fn new(from: Option<RouteState>, to: RouteState, kind: TransitionKind) -> Self {
        RouteTransition { from, to, kind }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0).unwrap()
    }

    fn sample_state(prefix: &str, path: Vec<u32>, observer: &str) -> RouteState {
        RouteState {
            prefix: Prefix::from(prefix),
            attributes: RouteAttributes::from_as_path(path),
            timestamp: sample_time(),
            observer: observer.to_string(),
        }
    }

    #[test]
    fn prefix_from_str() {
        let p = Prefix::from("192.0.2.0/24");
        assert_eq!(p.0, "192.0.2.0/24");
    }

    #[test]
    fn as_path_display() {
        let path = AsPath(vec![11537, 237, 1101]);
        assert_eq!(format!("{path}"), "11537 237 1101");
    }

    #[test]
    fn route_attributes_from_as_path() {
        let attrs = RouteAttributes::from_as_path(vec![11537, 237, 1101]);
        assert_eq!(attrs.origin_as, 1101);
        assert_eq!(attrs.as_path.0.len(), 3);
    }

    #[test]
    fn route_attributes_empty_path() {
        let attrs = RouteAttributes::from_as_path(vec![]);
        assert_eq!(attrs.origin_as, 0);
    }

    #[test]
    fn transition_announcement() {
        let state = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let t = RouteTransition::new(None, state, TransitionKind::Announcement);
        assert_eq!(t.kind, TransitionKind::Announcement);
        assert!(t.from.is_none());
    }

    #[test]
    fn transition_withdrawal() {
        let from = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let to = sample_state("192.0.2.0/24", vec![], "rv2:AS6447");
        let t = RouteTransition::new(Some(from), to, TransitionKind::Withdrawal);
        assert_eq!(t.kind, TransitionKind::Withdrawal);
    }

    #[test]
    fn transition_path_change() {
        let from = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let to = sample_state("192.0.2.0/24", vec![11537, 237, 1101], "rv2:AS6447");
        let kind = TransitionKind::PathChange {
            old: from.attributes.as_path.clone(),
            new: to.attributes.as_path.clone(),
        };
        let t = RouteTransition::new(Some(from), to, kind);
        assert!(matches!(t.kind, TransitionKind::PathChange { .. }));
    }

    #[test]
    fn transition_exact_duplicate() {
        let from = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let to = from.clone();
        let t = RouteTransition::new(Some(from), to, TransitionKind::ExactDuplicate);
        assert_eq!(t.kind, TransitionKind::ExactDuplicate);
    }

    #[test]
    fn transition_restoration() {
        let from = sample_state("192.0.2.0/24", vec![11537, 237, 1101], "rv2:AS6447");
        let original = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let t = RouteTransition::new(Some(from), original, TransitionKind::Restoration);
        assert_eq!(t.kind, TransitionKind::Restoration);
    }

    #[test]
    fn route_state_serialization_roundtrip() {
        let state = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let json = serde_json::to_string(&state).unwrap();
        let parsed: RouteState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, parsed);
    }

    #[test]
    fn transition_serialization_roundtrip() {
        let from = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let to = sample_state("192.0.2.0/24", vec![11537, 237, 1101], "rv2:AS6447");
        let kind = TransitionKind::PathChange {
            old: from.attributes.as_path.clone(),
            new: to.attributes.as_path.clone(),
        };
        let t = RouteTransition::new(Some(from), to, kind);
        let json = serde_json::to_string(&t).unwrap();
        let parsed: RouteTransition = serde_json::from_str(&json).unwrap();
        assert_eq!(t, parsed);
    }
}
