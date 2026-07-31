//! Network entity types — participants, peers, exchanges, router sites.

use serde::{Deserialize, Serialize};

/// The type of network entity referenced in an event.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    /// An Internet2 participant (university, research institution).
    Participant,
    /// A BGP peer.
    Peer,
    /// An Internet exchange point.
    Exchange,
    /// A specific router or site.
    RouterSite,
    /// Unknown or unclassified.
    Unknown,
}

/// A network entity referenced in an operational event.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkEntity {
    pub name: String,
    pub entity_type: EntityType,
    /// Optional site code extracted from parenthesized convention.
    pub site_code: Option<String>,
}

impl NetworkEntity {
    pub fn participant(name: &str) -> Self {
        NetworkEntity {
            name: name.to_string(),
            entity_type: EntityType::Participant,
            site_code: None,
        }
    }

    pub fn exchange(name: &str) -> Self {
        NetworkEntity {
            name: name.to_string(),
            entity_type: EntityType::Exchange,
            site_code: None,
        }
    }

    pub fn peer(name: &str) -> Self {
        NetworkEntity {
            name: name.to_string(),
            entity_type: EntityType::Peer,
            site_code: None,
        }
    }

    pub fn router_site(name: &str, site_code: &str) -> Self {
        NetworkEntity {
            name: name.to_string(),
            entity_type: EntityType::RouterSite,
            site_code: Some(site_code.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn participant_entity() {
        let e = NetworkEntity::participant("University of Michigan");
        assert_eq!(e.entity_type, EntityType::Participant);
        assert_eq!(e.name, "University of Michigan");
    }

    #[test]
    fn exchange_entity() {
        let e = NetworkEntity::exchange("DE-CIX");
        assert_eq!(e.entity_type, EntityType::Exchange);
    }

    #[test]
    fn peer_entity() {
        let e = NetworkEntity::peer("RIPE");
        assert_eq!(e.entity_type, EntityType::Peer);
    }

    #[test]
    fn router_site_entity() {
        let e = NetworkEntity::router_site("New York 32AOA", "NEWY32AOA");
        assert_eq!(e.entity_type, EntityType::RouterSite);
        assert_eq!(e.site_code, Some("NEWY32AOA".into()));
    }

    #[test]
    fn entity_serialization_roundtrip() {
        let e = NetworkEntity::router_site("New York 32AOA", "NEWY32AOA");
        let json = serde_json::to_string(&e).unwrap();
        let parsed: NetworkEntity = serde_json::from_str(&json).unwrap();
        assert_eq!(e, parsed);
    }
}
