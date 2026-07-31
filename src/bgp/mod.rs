//! BGP module — MRT ingestion, RIB seeding, UPDATE application,
//! route-state reconstruction.
//!
//! TODO: Implement MRT archive parsing, RIB seeding, UPDATE processing,
//! and route-state reconstruction with correct state-machine semantics.

/// Placeholder for an MRT archive.
#[derive(Debug, Clone)]
pub struct MrtArchive;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_mrt_archive_exists() {
        let _archive = MrtArchive;
    }
}
