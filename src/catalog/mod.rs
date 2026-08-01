//! Local event catalog — source-neutral identities, immutable revisions,
//! status, and a read-only localhost web interface.
//!
//! See `docs/ADRs/LOCAL-CATALOG-AND-WEB.md` for the architecture decision.

pub mod access;
pub mod analyzability;
pub mod archive_plan;
pub mod batch;
pub mod case_study_compare;
pub mod case_study_import;
pub mod db;
pub mod discovery;
pub mod document;
pub mod domain;
pub mod grnoc;
pub mod grnoc_viewer;
pub mod grouping;
pub mod import;
pub mod migrations;
#[cfg(test)]
pub mod mock_server;
pub mod netprofile;
pub mod observer_compare;
pub mod origin_inventory;
pub mod phase_summary;
pub mod relationships;
pub mod review;
pub mod session_audit;
pub mod source_extract;
pub mod status;
pub mod store;
pub mod sync;
pub mod target_research;
#[cfg(test)]
pub mod tests;
pub mod web;

pub use domain::*;
pub use status::CatalogStatus;
