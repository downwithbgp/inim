//! Local event catalog — source-neutral identities, immutable revisions,
//! status, and a read-only localhost web interface.
//!
//! See `docs/ADRs/LOCAL-CATALOG-AND-WEB.md` for the architecture decision.

pub mod case_study_import;
pub mod db;
pub mod document;
pub mod domain;
pub mod grnoc;
pub mod import;
pub mod migrations;
pub mod status;
pub mod store;
pub mod sync;
#[cfg(test)]
pub mod tests;
pub mod web;

pub use domain::*;
pub use status::CatalogStatus;
