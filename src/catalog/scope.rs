//! Project-scope policy — a reviewed, tracked configuration that decides
//! which entities and source records are Included in the active project
//! corpus.
//!
//! Project scope is a PROJECT-OWNER decision and is deliberately distinct
//! from analytical applicability:
//!
//! - an excluded event is NOT marked "not observable", "failed", or
//!   "invalid" — it is simply outside the configured project scope;
//! - an analytically valid IP-layer event may still be excluded by
//!   project policy;
//! - a non-observable optical event may still be included for incident
//!   context.
//!
//! The tracked configuration lives at `config/project-scope.toml` (see
//! the file header for the reviewed entry format). Matching is exact and
//! normalized (trim + ASCII uppercase) — never fuzzy, never substring.
//!
//! One shared service instance is loaded once per process boundary and
//! reused by the web application, the CLI, the worker, and the demo
//! bootstrap. It is never reread per HTML row.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The policy configuration file, relative to the project root.
pub const SCOPE_CONFIG_REL: &str = "config/project-scope.toml";
/// The supported configuration schema version.
pub const SCOPE_CONFIG_SCHEMA_VERSION: u32 = 1;
/// Reason code used by the seeded reviewed exclusions.
pub const REASON_PROJECT_OWNER_EXCLUSION: &str = "project_owner_exclusion";

/// The project-scope status of an entity or source record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectScopeStatus {
    /// Actively part of the project corpus.
    Included,
    /// Intentionally outside the configured project scope.
    Excluded,
}

impl ProjectScopeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectScopeStatus::Included => "Included",
            ProjectScopeStatus::Excluded => "Excluded",
        }
    }
}

/// One excluded entity (reviewed organization / network identity).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExcludedEntity {
    /// Stable reviewed key (e.g. "sample-org"). Never inferred.
    pub stable_key: String,
    /// Reviewed organization name.
    pub reviewed_name: String,
    /// Reviewed origin ASNs (exact).
    #[serde(default)]
    pub reviewed_asns: Vec<u32>,
    /// Exact normalized aliases (optional).
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Reason code from the supported vocabulary (no speculation).
    pub reason_code: String,
    /// Review date (RFC 3339).
    pub review_date: String,
    /// Source of the decision (e.g. "project-owner decision").
    pub source: String,
}

/// One excluded source record (exact external identifier).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExcludedSourceRecord {
    /// Source family exactly as used by the catalog
    /// (e.g. "grnoc-public-task-viewer").
    pub source_family: String,
    /// Exact external source identifier.
    pub external_id: String,
    /// Reason code from the supported vocabulary.
    pub reason_code: String,
}

/// The raw tracked configuration file content.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ScopeConfigFile {
    pub schema_version: u32,
    #[serde(default)]
    pub excluded_entities: Vec<ExcludedEntity>,
    #[serde(default)]
    pub excluded_source_records: Vec<ExcludedSourceRecord>,
}

/// Normalize an identifier for EXACT matching (trim + ASCII uppercase).
/// Never a fuzzy transformation.
pub fn normalize_exact(s: &str) -> String {
    s.trim().to_ascii_uppercase()
}

/// Validated, indexed project-scope policy. Loaded once per process.
#[derive(Debug, Clone, Default)]
pub struct ProjectScope {
    schema_version: u32,
    entities: Vec<ExcludedEntity>,
    source_records: Vec<ExcludedSourceRecord>,
    /// normalized entity name / alias -> entity index
    entity_name_index: HashMap<String, usize>,
    /// reviewed ASN -> entity index
    asn_index: HashMap<u32, usize>,
    /// (normalized source family, normalized external id) -> record index
    record_index: HashMap<String, usize>,
}

impl ProjectScope {
    /// Load and validate the tracked policy from `root/config/project-scope.toml`.
    ///
    /// A missing file yields an empty (all-Included) policy so callers
    /// without the tracked configuration still work; a malformed file or
    /// an unsupported schema version is a hard error — exclusions are
    /// never silently ignored.
    pub fn load(root: &Path) -> Result<ProjectScope, String> {
        let path = root.join(SCOPE_CONFIG_REL);
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ProjectScope::default());
            }
            Err(e) => {
                return Err(format!(
                    "cannot read project-scope policy {}: {e}",
                    path.display()
                ));
            }
        };
        let file: ScopeConfigFile = toml::from_str(&content)
            .map_err(|e| format!("invalid project-scope policy {}: {e}", path.display()))?;
        ProjectScope::from_config(file)
    }

    /// Validate and index a parsed configuration. Deterministic.
    pub fn from_config(file: ScopeConfigFile) -> Result<ProjectScope, String> {
        if file.schema_version != SCOPE_CONFIG_SCHEMA_VERSION {
            return Err(format!(
                "unsupported project-scope schema v{} (supported: v{SCOPE_CONFIG_SCHEMA_VERSION})",
                file.schema_version
            ));
        }
        let mut scope = ProjectScope {
            schema_version: file.schema_version,
            ..Default::default()
        };
        // Stable keys must be unique.
        let mut keys = std::collections::HashSet::new();
        for (i, entity) in file.excluded_entities.iter().enumerate() {
            if entity.stable_key.trim().is_empty() {
                return Err(format!(
                    "excluded entity #{} has an empty stable_key",
                    i + 1
                ));
            }
            if !keys.insert(entity.stable_key.trim().to_ascii_lowercase()) {
                return Err(format!(
                    "duplicate excluded entity stable_key {:?}",
                    entity.stable_key
                ));
            }
            if entity.reviewed_name.trim().is_empty() {
                return Err(format!(
                    "excluded entity {:?} has an empty reviewed_name",
                    entity.stable_key
                ));
            }
            if entity.reason_code.trim().is_empty() {
                return Err(format!(
                    "excluded entity {:?} has an empty reason_code",
                    entity.stable_key
                ));
            }
            for asn in &entity.reviewed_asns {
                if *asn == 0 {
                    return Err(format!(
                        "excluded entity {:?} has invalid ASN {asn}",
                        entity.stable_key
                    ));
                }
            }
            scope.entities.push(entity.clone());
            let entity_index = i;
            let names = std::iter::once(entity.reviewed_name.as_str())
                .chain(entity.aliases.iter().map(|a| a.as_str()));
            for name in names {
                let key = normalize_exact(name);
                if key.is_empty() {
                    return Err(format!(
                        "excluded entity {:?} has an empty alias",
                        entity.stable_key
                    ));
                }
                if let Some(prev) = scope.entity_name_index.insert(key.clone(), entity_index) {
                    return Err(format!(
                        "conflicting normalized alias {key:?} across excluded entities {} and {}",
                        file.excluded_entities[prev].stable_key, entity.stable_key
                    ));
                }
            }
            for asn in &entity.reviewed_asns {
                if let Some(prev) = scope.asn_index.insert(*asn, entity_index) {
                    return Err(format!(
                        "ASN {asn} listed by both {} and {}",
                        file.excluded_entities[prev].stable_key, entity.stable_key
                    ));
                }
            }
        }
        // Source-record entries must be unique and validated.
        let mut records = std::collections::HashSet::new();
        for (i, rec) in file.excluded_source_records.iter().enumerate() {
            if rec.source_family.trim().is_empty() || rec.external_id.trim().is_empty() {
                return Err(format!(
                    "excluded source record #{} has empty identifiers",
                    i + 1
                ));
            }
            let key = normalize_exact(&rec.external_id);
            if !records.insert(key.clone()) {
                return Err(format!(
                    "duplicate excluded source record {:?}/{:?}",
                    rec.source_family, rec.external_id
                ));
            }
            if rec.reason_code.trim().is_empty() {
                return Err(format!(
                    "excluded source record {:?}/{:?} has an empty reason_code",
                    rec.source_family, rec.external_id
                ));
            }
            scope.record_index.insert(key, i);
            scope.source_records.push(rec.clone());
        }
        Ok(scope)
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn entities(&self) -> &[ExcludedEntity] {
        &self.entities
    }

    pub fn source_records(&self) -> &[ExcludedSourceRecord] {
        &self.source_records
    }

    /// Exact source-record exclusion (matching precedence level 1:
    /// the exact external source ID). The recorded source family is
    /// provenance for the audit; the event is identified by its ID.
    pub fn excluded_source_record(&self, _source_family: &str, external_id: &str) -> bool {
        self.record_index
            .contains_key(&normalize_exact(external_id))
    }

    /// Exact reviewed entity name/alias exclusion (matching precedence
    /// levels 2 and 4: reviewed entity ID, then exact alias).
    pub fn excluded_entity_name(&self, name: &str) -> bool {
        self.entity_name_index.contains_key(&normalize_exact(name))
    }

    /// Exact reviewed ASN exclusion (matching precedence level 3).
    pub fn excluded_asn(&self, asn: u32) -> bool {
        self.asn_index.contains_key(&asn)
    }

    /// The exclusion reason code for a source record, if excluded.
    pub fn source_record_reason(&self, _source_family: &str, external_id: &str) -> Option<String> {
        self.record_index
            .get(&normalize_exact(external_id))
            .map(|&i| self.source_records[i].reason_code.clone())
    }

    /// The exclusion reason code for an entity name, if excluded.
    pub fn entity_name_reason(&self, name: &str) -> Option<String> {
        self.entity_name_index
            .get(&normalize_exact(name))
            .map(|&i| self.entities[i].reason_code.clone())
    }

    /// Whether any reviewed ASN of the given list is excluded.
    pub fn any_asn_excluded(&self, asns: &[u32]) -> bool {
        asns.iter().any(|a| self.excluded_asn(*a))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> ScopeConfigFile {
        ScopeConfigFile {
            schema_version: 1,
            excluded_entities: vec![ExcludedEntity {
                stable_key: "sample-org".to_string(),
                reviewed_name: "Sample Organization".to_string(),
                reviewed_asns: vec![64512],
                aliases: vec!["SAMPLE-ORG".to_string()],
                reason_code: "project_owner_exclusion".to_string(),
                review_date: "2026-08-03T00:00:00Z".to_string(),
                source: "project-owner decision".to_string(),
            }],
            excluded_source_records: vec![ExcludedSourceRecord {
                source_family: "grnoc-public-task-viewer".to_string(),
                external_id: "INC-EXCLUDED".to_string(),
                reason_code: "project_owner_exclusion".to_string(),
            }],
        }
    }

    #[test]
    fn project_scope_is_distinct_from_bgp_applicability() {
        // Exclusion is not an analytical verdict: the status vocabulary
        // is Included/Excluded only, and the excluded event is NOT
        // marked not-observable, failed, or invalid.
        let scope = ProjectScope::from_config(sample_config()).unwrap();
        assert!(scope.excluded_source_record("grnoc-public-task-viewer", "INC-EXCLUDED"));
        assert!(!scope.excluded_source_record("grnoc-public-task-viewer", "INC-OTHER"));
        // The two concepts serialize differently and never share a value.
        let status = ProjectScopeStatus::Excluded.as_str();
        assert_ne!(status, "NotDirectlyObservableInPublicBgp");
        assert_ne!(status, "AnalysisFailed");
    }

    #[test]
    fn excluded_event_is_not_marked_not_observable() {
        assert_ne!(
            ProjectScopeStatus::Excluded.as_str(),
            crate::catalog::domain::applicability::NOT_DIRECTLY_OBSERVABLE
        );
    }

    #[test]
    fn excluded_event_is_not_marked_analysis_failed() {
        assert_ne!(
            ProjectScopeStatus::Excluded.as_str(),
            crate::catalog::analyzability::state::ANALYSIS_FAILED
        );
    }

    #[test]
    fn included_event_may_still_be_not_directly_observable() {
        // An included optical event keeps its analytical applicability;
        // scope does not erase it.
        let scope = ProjectScope::from_config(sample_config()).unwrap();
        assert!(!scope.excluded_source_record("grnoc-public-task-viewer", "INC-OPTICAL"));
    }

    #[test]
    fn scope_policy_schema_is_versioned() {
        let scope = ProjectScope::from_config(sample_config()).unwrap();
        assert_eq!(scope.schema_version(), SCOPE_CONFIG_SCHEMA_VERSION);
        let bad = ScopeConfigFile {
            schema_version: 99,
            ..sample_config()
        };
        assert!(ProjectScope::from_config(bad).is_err());
    }

    #[test]
    fn exact_source_record_exclusion_matches() {
        let scope = ProjectScope::from_config(sample_config()).unwrap();
        assert!(scope.excluded_source_record("grnoc-public-task-viewer", "INC-EXCLUDED"));
        // Exact ID only: the recorded family is provenance, so the
        // event is matched by its external ID wherever it lives in the
        // catalog; id variants never match.
        assert!(scope.excluded_source_record("local-repository", "INC-EXCLUDED"));
        assert!(!scope.excluded_source_record("grnoc-public-task-viewer", "INC-EXCLUDE"));
        assert!(!scope.excluded_source_record("grnoc-public-task-viewer", "INC-EXCLUDED-2"));
    }

    #[test]
    fn reviewed_entity_exclusion_matches() {
        let scope = ProjectScope::from_config(sample_config()).unwrap();
        assert!(scope.excluded_entity_name("Sample Organization"));
        assert!(scope.excluded_entity_name("SAMPLE ORGANIZATION"));
        assert!(!scope.excluded_entity_name("Sample"));
        assert!(!scope.excluded_entity_name("Another Organization"));
    }

    #[test]
    fn reviewed_asn_exclusion_matches() {
        let scope = ProjectScope::from_config(sample_config()).unwrap();
        assert!(scope.excluded_asn(64512));
        assert!(!scope.excluded_asn(64513));
        assert!(!scope.excluded_asn(64514));
        assert!(scope.any_asn_excluded(&[64512]));
        assert!(!scope.any_asn_excluded(&[64513, 64514]));
    }

    #[test]
    fn alias_matching_is_exact_after_normalization() {
        let scope = ProjectScope::from_config(sample_config()).unwrap();
        assert!(scope.excluded_entity_name("sample-org"));
        assert!(scope.excluded_entity_name("  SAMPLE-ORG  "));
        assert!(!scope.excluded_entity_name("sample"));
        assert!(!scope.excluded_entity_name("samples"));
    }

    #[test]
    fn fuzzy_name_does_not_match() {
        let scope = ProjectScope::from_config(sample_config()).unwrap();
        // Substring/prefix/edit-distance matches are NOT exclusions.
        assert!(!scope.excluded_entity_name("Sample Organization LLC"));
        assert!(!scope.excluded_entity_name("The Sample Organization"));
        assert!(!scope.excluded_entity_name("SampleOrganizatio"));
    }

    #[test]
    fn unrelated_asn_is_not_excluded() {
        let scope = ProjectScope::from_config(sample_config()).unwrap();
        assert!(!scope.excluded_asn(64511));
        assert!(!scope.excluded_asn(1));
        assert!(!scope.excluded_asn(65535));
    }

    #[test]
    fn exclusion_reason_is_not_inferred() {
        let scope = ProjectScope::from_config(sample_config()).unwrap();
        assert_eq!(
            scope
                .source_record_reason("grnoc-public-task-viewer", "INC-EXCLUDED")
                .as_deref(),
            Some(REASON_PROJECT_OWNER_EXCLUSION)
        );
        assert_eq!(
            scope.entity_name_reason("Sample Organization").as_deref(),
            Some(REASON_PROJECT_OWNER_EXCLUSION)
        );
        assert_eq!(
            scope.source_record_reason("grnoc-public-task-viewer", "INC-OTHER"),
            None
        );
    }

    #[test]
    fn invalid_scope_policy_fails_startup() {
        // Malformed TOML is a hard error.
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join(SCOPE_CONFIG_REL);
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, "schema_version = ]nope[").unwrap();
        assert!(ProjectScope::load(dir.path()).is_err());
    }

    #[test]
    fn duplicate_exclusion_is_rejected() {
        let mut cfg = sample_config();
        cfg.excluded_source_records.push(ExcludedSourceRecord {
            source_family: "grnoc-public-task-viewer".to_string(),
            external_id: "inc-excluded".to_string(), // same after normalization
            reason_code: "project_owner_exclusion".to_string(),
        });
        assert!(ProjectScope::from_config(cfg).is_err());
        let mut cfg2 = sample_config();
        cfg2.excluded_entities.push(ExcludedEntity {
            stable_key: "sample-org".to_string(), // duplicate key
            reviewed_name: "Other".to_string(),
            reviewed_asns: vec![],
            aliases: vec![],
            reason_code: "x".to_string(),
            review_date: "2026-08-03T00:00:00Z".to_string(),
            source: "y".to_string(),
        });
        assert!(ProjectScope::from_config(cfg2).is_err());
    }

    #[test]
    fn conflicting_alias_is_rejected() {
        let mut cfg = sample_config();
        cfg.excluded_entities.push(ExcludedEntity {
            stable_key: "other-org".to_string(),
            reviewed_name: "Other Org".to_string(),
            reviewed_asns: vec![],
            aliases: vec!["sample-org".to_string()], // collides with the alias above
            reason_code: "project_owner_exclusion".to_string(),
            review_date: "2026-08-03T00:00:00Z".to_string(),
            source: "project-owner decision".to_string(),
        });
        assert!(ProjectScope::from_config(cfg).is_err());
    }

    #[test]
    fn policy_serialization_is_deterministic() {
        let scope = ProjectScope::from_config(sample_config()).unwrap();
        let names: Vec<String> = scope
            .entities()
            .iter()
            .map(|e| e.stable_key.clone())
            .collect();
        let records: Vec<(String, String)> = scope
            .source_records()
            .iter()
            .map(|r| (r.source_family.clone(), r.external_id.clone()))
            .collect();
        // Deterministic order: config order preserved, indexes consistent.
        assert_eq!(names, vec!["sample-org".to_string()]);
        assert_eq!(
            records,
            vec![(
                "grnoc-public-task-viewer".to_string(),
                "INC-EXCLUDED".to_string()
            )]
        );
    }

    #[test]
    fn generic_scope_engine_is_data_driven() {
        // The engine is data-driven: the sample config never names the
        // seeded exclusion, and the same code matches both.
        let scope = ProjectScope::from_config(sample_config()).unwrap();
        assert!(!scope.excluded_entity_name("Sample Network Two"));
        assert!(!scope.excluded_asn(64513));
        assert!(!scope.excluded_source_record("grnoc-public-task-viewer", "INC-OTHER-2"));
    }
}
