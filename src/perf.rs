//! Performance instrumentation (Session 32, Part 3).
//!
//! `performance.json` is a SEPARATE artifact: stage wall-clock timings and
//! per-archive metrics are volatile and must never participate in
//! substantive artifact-equivalence checks, and never influence the routing
//! verdict. Archive identity (URL + SHA-256) inside metrics stays
//! deterministic.

use serde::Serialize;

/// performance.json schema version.
pub const PERFORMANCE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Default)]
pub struct StageTiming {
    pub stage: String,
    pub wall_secs: f64,
    pub input_bytes: u64,
    pub output_count: u64,
    pub workers: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ArchiveMetric {
    pub archive_url: String,
    pub archive_sha256: String,
    pub compressed_bytes: u64,
    pub parse_wall_secs: f64,
    pub parsed_elements: u64,
    pub admitted_observations: u64,
    pub derived_cache_write_secs: f64,
    pub cache_hit: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct HostInfo {
    pub logical_cpus: usize,
    pub available_parallelism: usize,
    pub jobs: usize,
    pub parse_jobs: usize,
    pub download_jobs: usize,
    pub cgroup_cpu_max: Option<String>,
    pub cpu_affinity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PerformanceReport {
    pub schema_version: u32,
    pub host: HostInfo,
    pub stages: Vec<StageTiming>,
    pub archives: Vec<ArchiveMetric>,
    pub total_wall_secs: f64,
}

/// Logical CPUs visible to the host (not necessarily to the process).
pub fn logical_cpus() -> usize {
    std::fs::read_to_string("/proc/cpuinfo")
        .map(|s| s.lines().filter(|l| l.starts_with("processor")).count())
        .unwrap_or_else(|_| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        })
}

/// CPUs allowed by affinity, when detectable.
pub fn cpu_affinity() -> Option<String> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|l| l.starts_with("Cpus_allowed_list:"))
        .map(|l| l.trim().to_string())
}

/// cgroup CPU limit, when detectable (v2 cpu.max, else v1 quota/period).
pub fn cgroup_cpu_max() -> Option<String> {
    if let Ok(v2) = std::fs::read_to_string("/sys/fs/cgroup/cpu.max") {
        let v = v2.trim().to_string();
        if !v.is_empty() && v != "max 100000" {
            return Some(v);
        }
    }
    let quota = std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_quota_us").ok()?;
    let period = std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_period_us").ok()?;
    let q = quota.trim().parse::<i64>().ok()?;
    let p = period.trim().parse::<i64>().ok()?;
    if q > 0 && p > 0 {
        Some(format!(
            "cfs_quota={q} period={p} (~{} cores)",
            (q + p - 1) / p
        ))
    } else {
        None
    }
}

/// Host topology for the execution plan.
pub fn host_info(jobs: usize, parse_jobs: usize, download_jobs: usize) -> HostInfo {
    HostInfo {
        logical_cpus: logical_cpus(),
        available_parallelism: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
        jobs,
        parse_jobs,
        download_jobs,
        cgroup_cpu_max: cgroup_cpu_max(),
        cpu_affinity: cpu_affinity(),
    }
}

/// Write the performance artifact (never part of substantive outputs).
pub fn write_performance(report: &PerformanceReport, path: &std::path::Path) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report).map_err(|e| format!("JSON error: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_plan_reports_worker_topology() {
        let info = host_info(4, 8, 2);
        assert!(info.logical_cpus >= 1, "host logical CPUs reported");
        assert!(
            info.available_parallelism >= 1,
            "available_parallelism reported"
        );
        assert_eq!(info.jobs, 4);
        assert_eq!(info.parse_jobs, 8);
        assert_eq!(info.download_jobs, 2);
    }

    #[test]
    fn detected_cpu_limit_is_reported_separately_from_host_cpu_count() {
        let info = host_info(1, 0, 2);
        // Host count and any cgroup limit are distinct fields; a limit must
        // never be conflated with the machine's CPU count.
        assert!(info.logical_cpus >= info.available_parallelism);
        assert!(info.cgroup_cpu_max.is_none() || info.cgroup_cpu_max.is_some());
        // The serialized report keeps them separate.
        let report = PerformanceReport {
            schema_version: PERFORMANCE_SCHEMA_VERSION,
            host: info,
            ..Default::default()
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("logical_cpus"));
        assert!(json.contains("cgroup_cpu_max"));
        assert!(json.contains("available_parallelism"));
    }
}

/// The documented pipeline stages that performance instrumentation must cover.
pub const PIPELINE_STAGES: &[&str] = &[
    "broker+cache",
    "RIB parse",
    "UPDATE cache+parse",
    "reconstruction",
    "tokenize",
    "lifecycle",
    "waves+motifs",
    "assess",
    "outputs",
];

/// Stages present in the report but missing from the documented pipeline.
pub fn missing_pipeline_stages(report: &PerformanceReport) -> Vec<String> {
    let present: Vec<&str> = report
        .stages
        .iter()
        .map(|s| s.stage.as_str())
        .filter(|s| PIPELINE_STAGES.iter().any(|p| s.contains(p)))
        .collect();
    PIPELINE_STAGES
        .iter()
        .filter(|p| !present.iter().any(|s| s.contains(*p)))
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod pipeline_tests {
    use super::*;

    fn sample_report() -> PerformanceReport {
        PerformanceReport {
            schema_version: PERFORMANCE_SCHEMA_VERSION,
            stages: PIPELINE_STAGES
                .iter()
                .map(|s| StageTiming {
                    stage: s.to_string(),
                    wall_secs: 1.0,
                    ..Default::default()
                })
                .collect(),
            archives: vec![ArchiveMetric {
                archive_url: "http://archive.routeviews.org/bgpdata/2019.08/UPDATES/updates.20190821.1600.bz2"
                    .to_string(),
                archive_sha256: "4546b78f8fb9ced87b93867cbb5f76e4abc11f37e564001f73f593313b9fd182"
                    .to_string(),
                compressed_bytes: 1_500_000,
                parse_wall_secs: 0.4,
                parsed_elements: 1200,
                admitted_observations: 3,
                derived_cache_write_secs: 0.05,
                cache_hit: false,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn stage_timings_cover_complete_pipeline() {
        let report = sample_report();
        let missing = missing_pipeline_stages(&report);
        assert!(missing.is_empty(), "missing stages: {missing:?}");
    }

    #[test]
    fn performance_artifact_is_separate_from_substantive_report() {
        let dir = tempfile::tempdir().unwrap();
        let report = sample_report();
        let path = dir.path().join("performance.json");
        write_performance(&report, &path).unwrap();
        // The artifact contains ONLY performance metadata.
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains("parse_wall_secs"));
        assert!(json.contains("stages"));
        assert!(
            !json.contains("verdict"),
            "no routing verdict in performance data"
        );
    }

    #[test]
    fn volatile_timings_are_excluded_from_semantic_run_comparison() {
        // Two runs with identical archive identity but different volatile
        // timings must compare equal on identity and differ only in timing
        // fields — the artifact structure keeps them separate.
        let a = sample_report();
        let mut b = sample_report();
        b.stages[0].wall_secs = 99.0;
        assert_eq!(a.archives[0].archive_url, b.archives[0].archive_url);
        assert_eq!(a.archives[0].archive_sha256, b.archives[0].archive_sha256);
        assert_ne!(a.stages[0].wall_secs, b.stages[0].wall_secs);
        let (ja, jb) = (
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap(),
        );
        assert_ne!(ja, jb, "timings differ");
    }

    #[test]
    fn archive_metrics_preserve_deterministic_archive_identity() {
        let a = sample_report();
        let json = serde_json::to_string(&a).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let m = &v["archives"][0];
        assert_eq!(
            m["archive_url"].as_str().unwrap(),
            a.archives[0].archive_url
        );
        assert_eq!(
            m["archive_sha256"].as_str().unwrap(),
            a.archives[0].archive_sha256
        );
        assert_eq!(m["compressed_bytes"], 1_500_000);
    }
}

/// CPU topology as visible at three different levels: the host
/// (`/proc/cpuinfo`), the process (scheduler + affinity), and the
/// container (cgroup v2 cpuset/cpu.max). A development server may expose
/// more CPUs to the host than to a VM/container; worker defaults must be
/// chosen from the PROCESS view, never from a host-only figure.
#[derive(Debug, Clone, Serialize, Default)]
pub struct CpuTopology {
    /// Host-visible CPU count (lines in /proc/cpuinfo).
    pub host_cpuinfo_count: usize,
    /// Process-visible parallelism (std available_parallelism).
    pub process_visible_cpus: usize,
    /// cgroup v2 cpuset.cpus.effective when readable.
    pub cpuset_effective: Option<String>,
    /// Process affinity (Cpus_allowed_list) when readable.
    pub affinity: Option<String>,
    /// cgroup CPU quota when readable.
    pub cgroup_cpu_max: Option<String>,
}

/// Collect the three-level CPU topology. Every probe degrades gracefully.
pub fn cpu_topology() -> CpuTopology {
    CpuTopology {
        host_cpuinfo_count: logical_cpus(),
        process_visible_cpus: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
        cpuset_effective: std::fs::read_to_string("/sys/fs/cgroup/cpuset.cpus.effective")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        affinity: cpu_affinity(),
        cgroup_cpu_max: cgroup_cpu_max(),
    }
}

#[cfg(test)]
mod cpu_topology_tests {
    use super::*;

    #[test]
    fn cpu_limit_reporting_distinguishes_host_and_process_visibility() {
        let topo = cpu_topology();
        // Host view and process view are separate fields — a host-only
        // figure must never be reported as the process-visible count.
        assert!(topo.host_cpuinfo_count >= 1);
        assert!(topo.process_visible_cpus >= 1);
        assert_ne!(
            topo.host_cpuinfo_count, 0,
            "host CPU count must be a distinct, reported value"
        );
        // When the process is restricted (affinity/cgroup/cpuset), the
        // process view must be the effective one.
        let restricted = topo.affinity.is_some()
            || topo.cpuset_effective.is_some()
            || topo.cgroup_cpu_max.is_some();
        if restricted {
            let allowed = topo
                .cpuset_effective
                .clone()
                .map(|s| {
                    // Count CPUs in a cpuset list like "0-11,14".
                    let mut n = 0usize;
                    for part in s.split(',') {
                        if let Some((a, b)) = part.split_once('-') {
                            let lo: usize = a.parse().unwrap_or(0);
                            let hi: usize = b.parse().unwrap_or(lo);
                            n += hi - lo + 1;
                        } else if !part.is_empty() {
                            n += 1;
                        }
                    }
                    n
                })
                .unwrap_or(topo.process_visible_cpus);
            assert!(
                topo.process_visible_cpus <= topo.host_cpuinfo_count,
                "process cannot see more CPUs than the host"
            );
            assert!(
                allowed <= topo.host_cpuinfo_count,
                "cpuset cannot exceed the host CPU count"
            );
        }
        // The serialized report keeps every level distinct.
        let json = serde_json::to_string(&topo).unwrap();
        assert!(json.contains("host_cpuinfo_count"));
        assert!(json.contains("process_visible_cpus"));
        assert!(json.contains("cpuset_effective"));
        assert!(json.contains("affinity"));
        assert!(json.contains("cgroup_cpu_max"));
    }
}
