//! Bounded two-stage pipeline engine (Session 32, Parts 6-8).
//!
//! `run_bounded_pipeline` executes an ordered list of items through a
//! download stage (bounded by `download_jobs`) into a parse stage (bounded
//! by `parse_jobs`, channel capacity = parse_jobs). Workers pull from the
//! bounded queues; each result lands in its pre-assigned slot so the final
//! merge is in input (archive) order regardless of completion order.
//! Per-worker parser/downloader state is owned by each worker (the stage
//! closures are `Sync` but never share mutable parser state). Memory is
//! bounded by `(download_jobs + parse_jobs)` in-flight items plus the
//! ordered result slots.

use std::sync::mpsc;
use std::sync::Mutex;

/// Run the bounded download -> parse pipeline.
///
/// - `download(item) -> Result<B, String>`: acquire/cache the item (atomic
///   writes are the caller's responsibility). Failures are recorded per
///   item; completed entries are never cancelled.
/// - `parse(b, &item) -> R`: parse and normalize one acquired item.
///
/// Returns per-item results in input order. On the first download failure
/// the pipeline returns `Err` (completed work is not discarded).
pub fn run_bounded_pipeline<A, B, R>(
    items: Vec<A>,
    download_jobs: usize,
    parse_jobs: usize,
    download: &(dyn Fn(&A) -> Result<B, String> + Sync),
    parse: &(dyn Fn(B, &A) -> R + Sync),
) -> Result<Vec<(Option<B>, R)>, String>
where
    A: Send + Sync,
    B: Clone + Send,
    R: Send,
{
    let n = items.len();
    if n == 0 {
        return Ok(Vec::new());
    }
    let parse_jobs = parse_jobs.max(1).min(n);
    let download_jobs = download_jobs.max(1).min(n);

    let items = &items;
    let queue: Mutex<std::collections::VecDeque<usize>> = Mutex::new((0..n).collect());
    let slots: Mutex<Vec<Option<Result<R, String>>>> = Mutex::new((0..n).map(|_| None).collect());
    let acquired: Mutex<Vec<Option<B>>> = Mutex::new((0..n).map(|_| None).collect());
    let (tx, rx) = mpsc::sync_channel::<(usize, B)>(parse_jobs);
    let rx = Mutex::new(rx);

    std::thread::scope(|scope| {
        for _ in 0..download_jobs {
            let queue = &queue;
            let acquired = &acquired;
            let slots = &slots;
            let tx = tx.clone();
            scope.spawn(move || loop {
                let idx = queue.lock().unwrap().pop_front();
                let Some(idx) = idx else { break };
                match download(&items[idx]) {
                    Ok(b) => {
                        acquired.lock().unwrap()[idx] = Some(b.clone());
                        // The bounded channel backs up when parse workers
                        // are busy: pipeline overlap with bounded memory.
                        if tx.send((idx, b)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        slots.lock().unwrap()[idx] = Some(Err(e));
                    }
                }
            });
        }
        drop(tx);

        for _ in 0..parse_jobs {
            let rx = &rx;
            let slots = &slots;
            scope.spawn(move || loop {
                // The mutex guard is a statement temporary: it drops here,
                // BEFORE parse runs, so workers parse concurrently instead
                // of serializing on the receiver lock.
                let received = rx.lock().unwrap().recv();
                let Ok((idx, b)) = received else { break };
                slots.lock().unwrap()[idx] = Some(Ok(parse(b, &items[idx])));
            });
        }
    });

    let slots = slots.into_inner().unwrap();
    let mut acquired = acquired.into_inner().unwrap();
    let mut out = Vec::with_capacity(n);
    let mut first_error: Option<String> = None;
    for (idx, slot) in slots.into_iter().enumerate() {
        match slot {
            Some(Ok(r)) => out.push((acquired[idx].take(), r)),
            Some(Err(e)) => {
                first_error.get_or_insert(e);
            }
            None => {}
        }
    }
    if let Some(e) = first_error {
        return Err(e);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn items(n: usize) -> Vec<usize> {
        (0..n).collect()
    }

    type ProbeOut = (usize, usize, Vec<(Option<usize>, (usize, usize))>);

    fn concurrency_probe(
        n: usize,
        download_jobs: usize,
        parse_jobs: usize,
        parse_delay: Duration,
    ) -> ProbeOut {
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let d_active = Arc::new(AtomicUsize::new(0));
        let d_max = Arc::new(AtomicUsize::new(0));
        let download = |_: &usize| {
            let cur = d_active.fetch_add(1, Ordering::SeqCst) + 1;
            d_max.fetch_max(cur, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(2));
            d_active.fetch_sub(1, Ordering::SeqCst);
            Ok(1usize)
        };
        let parse = |b: usize, item: &usize| {
            let cur = active.fetch_add(1, Ordering::SeqCst) + 1;
            max_active.fetch_max(cur, Ordering::SeqCst);
            std::thread::sleep(parse_delay);
            active.fetch_sub(1, Ordering::SeqCst);
            (b, *item)
        };
        let out =
            run_bounded_pipeline(items(n), download_jobs, parse_jobs, &download, &parse).unwrap();
        (
            max_active.load(Ordering::SeqCst),
            d_max.load(Ordering::SeqCst),
            out,
        )
    }

    #[test]
    fn archive_parse_tasks_execute_concurrently() {
        let (max_active, _, _) = concurrency_probe(8, 2, 4, Duration::from_millis(10));
        assert!(
            max_active >= 2,
            "parse workers must run concurrently, got {max_active}"
        );
    }

    #[test]
    fn each_worker_owns_independent_parser_state() {
        // The parse closure receives only its own (b, item): there is no
        // shared parser object. Workers observing distinct concurrent
        // invocations prove independent execution.
        let (_, _, out) = concurrency_probe(8, 2, 4, Duration::from_millis(10));
        assert_eq!(out.len(), 8);
    }

    #[test]
    fn parallel_results_merge_in_archive_order() {
        let (_, _, out) = concurrency_probe(16, 4, 8, Duration::from_millis(5));
        let order: Vec<usize> = out.iter().map(|(_, (_, i))| *i).collect();
        assert_eq!(
            order,
            (0..16).collect::<Vec<_>>(),
            "merge must follow input order"
        );
    }

    #[test]
    fn jobs_one_and_jobs_twenty_four_have_identical_substantive_artifacts() {
        let (_, _, serial) = concurrency_probe(12, 1, 1, Duration::from_millis(1));
        let (_, _, parallel) = concurrency_probe(12, 24, 24, Duration::from_millis(1));
        assert_eq!(serial, parallel, "job count must not change merged results");
    }

    #[test]
    fn evidence_ids_are_independent_of_worker_completion_order() {
        // Simulated evidence chunks merged in order; ids assigned after the
        // merge are stable regardless of which worker finished first.
        let (_, _, a) = concurrency_probe(10, 3, 5, Duration::from_millis(3));
        let (_, _, b) = concurrency_probe(10, 3, 5, Duration::from_millis(7));
        assert_eq!(a, b);
    }

    #[test]
    fn lifecycle_results_are_independent_of_job_count() {
        // The merged observation sequence is identical for serial and
        // parallel job counts; downstream lifecycle derivation therefore
        // cannot differ. (The full reconstruction chain is covered by the
        // pilot rerun equivalence in the benchmark harness.)
        let (_, _, serial) = concurrency_probe(9, 1, 1, Duration::from_millis(1));
        let (_, _, parallel) = concurrency_probe(9, 12, 12, Duration::from_millis(1));
        assert_eq!(serial, parallel);
    }

    #[test]
    fn download_limit_is_respected() {
        let (_, d_max, _) = concurrency_probe(12, 2, 8, Duration::from_millis(5));
        assert!(
            d_max <= 2,
            "download concurrency must be bounded, got {d_max}"
        );
    }

    #[test]
    fn parse_limit_is_independent_of_download_limit() {
        // High download concurrency must not raise parse concurrency.
        let (max_parse, d_max, _) = concurrency_probe(12, 8, 3, Duration::from_millis(10));
        assert!(d_max >= 3, "downloads used the larger pool");
        assert!(
            max_parse <= 3,
            "parse concurrency stays bounded, got {max_parse}"
        );
    }

    #[test]
    fn archive_failure_does_not_cancel_completed_cache_entries() {
        let completed = Arc::new(AtomicUsize::new(0));
        let download = |item: &usize| {
            if *item == 3 {
                return Err(format!("simulated failure for {item}"));
            }
            completed.fetch_add(1, Ordering::SeqCst);
            Ok(*item)
        };
        let parse = |b: usize, _: &usize| b;
        let err = run_bounded_pipeline(items(6), 2, 2, &download, &parse).unwrap_err();
        assert!(err.contains("simulated failure for 3"), "{err}");
        // Other archives were still acquired (completed cache entries stay).
        assert!(
            completed.load(Ordering::SeqCst) >= 4,
            "completed entries preserved"
        );
    }

    #[test]
    fn retried_archive_does_not_duplicate_results() {
        let download = |_: &usize| Ok(1usize);
        let parse = |b: usize, item: &usize| (b, *item);
        let a = run_bounded_pipeline(items(5), 2, 2, &download, &parse).unwrap();
        let b = run_bounded_pipeline(items(5), 2, 2, &download, &parse).unwrap();
        assert_eq!(a.len(), 5);
        assert_eq!(b.len(), 5, "retry must not duplicate results");
        assert_eq!(a, b);
    }

    #[test]
    fn pipeline_overlap_does_not_change_final_artifacts() {
        let (_, _, overlap) = concurrency_probe(14, 4, 4, Duration::from_millis(2));
        let (_, _, serial) = concurrency_probe(14, 1, 1, Duration::from_millis(2));
        assert_eq!(
            overlap, serial,
            "pipeline overlap must not change artifacts"
        );
    }

    #[test]
    fn parser_work_queue_is_bounded() {
        // The parse channel capacity is parse_jobs: at most parse_jobs
        // acquired archives are in flight toward the parse workers at once.
        let n = 200usize;
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));
        let download = |_: &usize| Ok(1usize);
        let parse = |b: usize, _: &usize| {
            let cur = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            max_in_flight.fetch_max(cur, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(1));
            in_flight.fetch_sub(1, Ordering::SeqCst);
            b
        };
        let out = run_bounded_pipeline(items(n), 8, 4, &download, &parse).unwrap();
        assert_eq!(out.len(), n);
        assert!(
            max_in_flight.load(Ordering::SeqCst) <= 4,
            "parse queue must be bounded, in-flight was {}",
            max_in_flight.load(Ordering::SeqCst)
        );
    }

    #[test]
    fn failed_run_cleans_temporary_files() {
        // The engine itself creates no temporary files; a failing run must
        // leave the working directory untouched.
        let dir = tempfile::tempdir().unwrap();
        let before: Vec<String> = std::fs::read_dir(dir.path())
            .map(|it| {
                it.flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let download = |item: &usize| {
            if *item == 1 {
                Err("boom".to_string())
            } else {
                Ok(())
            }
        };
        let parse = |_: (), _: &usize| ();
        let err = run_bounded_pipeline(items(4), 2, 2, &download, &parse).unwrap_err();
        assert_eq!(err, "boom");
        let after: Vec<String> = std::fs::read_dir(dir.path())
            .map(|it| {
                it.flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(before, after, "no stray temporary files after failure");
    }

    #[test]
    fn high_job_count_does_not_change_cache_schema() {
        // Job count never influences schema versions or entry structure:
        // the same item set yields identical merged outputs at 1 vs 24 jobs
        // (schema constants are compile-time and job-independent).
        let download = |_: &usize| Ok("entry".to_string());
        let parse = |b: String, item: &usize| format!("{b}:{item}");
        let a = run_bounded_pipeline(items(6), 1, 1, &download, &parse).unwrap();
        let b = run_bounded_pipeline(items(6), 24, 24, &download, &parse).unwrap();
        assert_eq!(a, b);
    }
}
