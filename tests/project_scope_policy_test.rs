//! Current project-scope policy integration tests.
//!
//! These tests load the TRACKED policy (config/project-scope.toml) and
//! verify the seeded exclusions. The excluded entity may be named here
//! (this is one of the allowlisted reference points for the current
//! exclusion). Generic unit tests use neutral fixtures.

use std::path::Path;

use inim::catalog::scope::ProjectScope;

fn repo_root() -> &'static Path {
    Path::new(".")
}

#[test]
fn current_project_policy_excludes_inc0303298() {
    let scope = ProjectScope::load(repo_root()).unwrap();
    assert!(
        scope.excluded_source_record("grnoc-public-task-viewer", "INC0303298"),
        "tracked policy must exclude the source event by exact source record"
    );
}

#[test]
fn current_project_policy_excludes_as270() {
    let scope = ProjectScope::load(repo_root()).unwrap();
    assert!(
        scope.excluded_asn(270),
        "tracked policy must exclude the reviewed ASN"
    );
    assert!(
        scope.excluded_entity_name("NOAA"),
        "tracked policy must exclude the reviewed entity name"
    );
}

#[test]
fn current_project_policy_does_not_exclude_unrelated_government_asn() {
    let scope = ProjectScope::load(repo_root()).unwrap();
    // The exclusion is specific: adjacent/federal ASNs are NOT excluded.
    for asn in [291, 400, 174, 15169, 3356, 64512] {
        assert!(!scope.excluded_asn(asn), "ASN {asn} must not be excluded");
    }
    for name in ["National Weather Service", "Department of Commerce", "NWS"] {
        assert!(
            !scope.excluded_entity_name(name),
            "{name:?} must not be excluded"
        );
    }
}

#[test]
fn current_policy_schema_is_supported() {
    let scope = ProjectScope::load(repo_root()).unwrap();
    assert_eq!(scope.schema_version(), 1);
}

#[test]
fn current_policy_has_exact_reason_codes() {
    let scope = ProjectScope::load(repo_root()).unwrap();
    assert_eq!(
        scope
            .source_record_reason("grnoc-public-task-viewer", "INC0303298")
            .as_deref(),
        Some("project_owner_exclusion")
    );
    assert_eq!(
        scope.entity_name_reason("NOAA").as_deref(),
        Some("project_owner_exclusion")
    );
}
