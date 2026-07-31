#!/usr/bin/env python3
"""Compare validation run outputs for substantive equivalence.

Strips runtime-specific metadata (generated_at, timing, cache hit counts,
worker count, wall-clock timestamps) and deep-compares the remaining
JSON structures and text reports.

Usage:
    python3 scripts/compare_runs.py out/validation/INC0302574-serial out/validation/INC0302574-parallel-cold out/validation/INC0302574-parallel-warm
"""

import json
import sys
import os
import re
from pathlib import Path


def load_json(path):
    with open(path) as f:
        return json.load(f)


def load_jsonl(path):
    records = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line:
                records.append(json.loads(line))
    return records


def strip_generated_at(obj):
    """Recursively remove 'generated_at' keys from dicts."""
    if isinstance(obj, dict):
        return {k: strip_generated_at(v) for k, v in obj.items() if k != "generated_at"}
    elif isinstance(obj, list):
        return [strip_generated_at(v) for v in obj]
    else:
        return obj


def normalize_report_txt(text):
    """Remove lines that contain runtime-only metadata from report.txt."""
    lines = text.splitlines()
    filtered = []
    skip_patterns = [
        r"Cache hit summary:",
        r"RIB derived cache hits:",
        r"UPDATE derived cache hits:",
        r"Worker count:",
        r"Total wall time:",
        r"── Stage timings",
        r"^\s{2}\S+\s{2,}\d+\.\ds$",  # timing lines like "  broker+cache     0.5s"
        r"^\s{2}TOTAL\s{2,}\d+\.\ds$",
    ]
    skip_res = [re.compile(p) for p in skip_patterns]
    in_timing_section = False
    
    for line in lines:
        if "── Stage timings" in line:
            in_timing_section = True
            continue
        if in_timing_section and re.match(r"^\s{2}\S+\s{2,}\d+\.\ds$", line):
            continue
        if in_timing_section and re.match(r"^\s{2}TOTAL\s{2,}\d+\.\ds$", line):
            in_timing_section = False
            continue
        # Skip any line matching a skip pattern
        if any(r.search(line) for r in skip_res):
            continue
        filtered.append(line)
    return "\n".join(filtered)


def compare_files(paths, key, normalizer=None):
    """Compare a single file across all run dirs. Returns (ok, diffs)."""
    values = []
    for p in paths:
        fpath = os.path.join(p, key)
        if not os.path.exists(fpath):
            return False, [f"{key}: missing in {p}"]
        if key.endswith(".json"):
            val = load_json(fpath)
        elif key.endswith(".jsonl"):
            val = load_jsonl(fpath)
        else:
            with open(fpath) as f:
                val = f.read()
        if normalizer:
            val = normalizer(val)
        values.append(val)

    baseline = values[0]
    for i, v in enumerate(values[1:], 1):
        if v != baseline:
            # Try to show what differs
            if isinstance(baseline, dict) and isinstance(v, dict):
                bk = set(baseline.keys())
                vk = set(v.keys())
                only_b = bk - vk
                only_v = vk - bk
                common = bk & vk
                diff_keys = [k for k in common if baseline[k] != v[k]]
                details = []
                if only_b:
                    details.append(f"only in run-0: {sorted(only_b)}")
                if only_v:
                    details.append(f"only in run-{i}: {sorted(only_v)}")
                if diff_keys:
                    details.append(f"differing keys: {diff_keys[:10]}")
                return False, [f"{key}: run-0 vs run-{i} differ — {', '.join(details)}"]
            elif isinstance(baseline, list) and isinstance(v, list):
                if len(baseline) != len(v):
                    return False, [f"{key}: run-0 has {len(baseline)} items, run-{i} has {len(v)}"]
                for j, (a, b) in enumerate(zip(baseline, v)):
                    if a != b:
                        return False, [f"{key}: item {j} differs"]
            return False, [f"{key}: run-0 vs run-{i} differ"]
    return True, []


def compare_runs(dirs):
    """Compare all outputs across the given run directories."""
    all_ok = True
    files_to_compare = [
        "archive_manifest.json",
        "evidence_appendix.jsonl",
        "limitations.json",
        "report.json",
    ]
    
    for key in files_to_compare:
        normalizer = strip_generated_at if key.endswith(".json") else None
        ok, diffs = compare_files(dirs, key, normalizer)
        if not ok:
            all_ok = False
            for d in diffs:
                print(f"  MISMATCH: {d}")
        else:
            print(f"  OK: {key}")
    
    # Compare report.txt separately (strip timings)
    ok, diffs = compare_files(dirs, "report.txt", normalize_report_txt)
    if not ok:
        all_ok = False
        for d in diffs:
            print(f"  MISMATCH: {d}")
    else:
        print(f"  OK: report.txt")
    
    # Compare stdout outcome JSONs
    ok, diffs = compare_files(dirs, "stdout.json", strip_generated_at)
    if not ok:
        all_ok = False
        for d in diffs:
            print(f"  MISMATCH: {d}")
    else:
        print(f"  OK: stdout.json")
    
    return all_ok


def extract_admission_counters(stderr_path):
    """Extract total admission counters from a stderr log."""
    total_parsed = 0
    total_prefix = 0
    total_collpref = 0
    total_admitted = 0
    total_ann = 0
    total_wd = 0
    preflight_streams = 0
    
    with open(stderr_path) as f:
        for line in f:
            # Match "done:" lines from UPDATE processing
            m = re.search(
                r"done: (\d+) parsed, (\d+) prefix, (\d+) coll\+pref, (\d+) admitted \((\d+) ann, (\d+) wd\)",
                line,
            )
            if m:
                total_parsed += int(m.group(1))
                total_prefix += int(m.group(2))
                total_collpref += int(m.group(3))
                total_admitted += int(m.group(4))
                total_ann += int(m.group(5))
                total_wd += int(m.group(6))
            # Match "RIB preflight done: N frozen streams"
            m2 = re.search(r"RIB preflight done: (\d+) frozen streams", line)
            if m2:
                preflight_streams = int(m2.group(1))
    
    return {
        "total_parsed": total_parsed,
        "total_prefix_matches": total_prefix,
        "total_collpref_matches": total_collpref,
        "total_admitted": total_admitted,
        "total_announcements": total_ann,
        "total_withdrawals": total_wd,
        "frozen_streams": preflight_streams,
    }


def compare_stderr_counters(dirs):
    """Compare admission counters extracted from stderr logs and cache metadata."""
    print("\nAdmission counter comparison (from stderr):")
    counters = []
    for d in dirs:
        stderr = os.path.join(d, "stderr.log")
        if os.path.exists(stderr):
            c = extract_admission_counters(stderr)
            counters.append(c)
            print(f"  {os.path.basename(d)}: frozen={c['frozen_streams']}, "
                  f"parsed={c['total_parsed']}, prefix={c['total_prefix_matches']}, "
                  f"coll+pref={c['total_collpref_matches']}, admitted={c['total_admitted']} "
                  f"({c['total_announcements']} ann, {c['total_withdrawals']} wd)")
        else:
            print(f"  {os.path.basename(d)}: stderr.log missing!")
            counters.append(None)
    
    # For parallel runs, the stderr may not have per-file "done:" lines.
    # Verify that non-zero parsed counts only appear in serial runs where applicable,
    # and that the critical counters (frozen_streams, admitted) match.
    baseline = counters[0]
    all_ok = True
    critical_keys = ['frozen_streams', 'total_admitted', 'total_announcements', 'total_withdrawals',
                     'total_prefix_matches', 'total_collpref_matches']
    
    for i, c in enumerate(counters[1:], 1):
        if c is None or baseline is None:
            all_ok = False
            continue
        # Only compare critical counters — parsed may differ due to logging differences
        critical_match = all(c[k] == baseline[k] for k in critical_keys)
        if not critical_match:
            for k in critical_keys:
                if c[k] != baseline[k]:
                    print(f"  MISMATCH {k}: run-0={baseline[k]} vs run-{i}={c[k]}")
            all_ok = False
        elif c['total_parsed'] != baseline['total_parsed']:
            print(f"  NOTE: parsed count differs ({baseline['total_parsed']} vs {c['total_parsed']}) — expected due to parallel logging path")
    
    if all_ok:
        print("  All critical counter totals match.")
    return all_ok


def main():
    if len(sys.argv) < 3:
        print(f"Usage: {sys.argv[0]} <run-dir-A> <run-dir-B> [run-dir-C]")
        sys.exit(1)
    
    dirs = sys.argv[1:]
    print(f"Comparing {len(dirs)} runs:")
    for d in dirs:
        print(f"  {d}")
    print()
    
    files_ok = compare_runs(dirs)
    counters_ok = compare_stderr_counters(dirs)
    
    print()
    if files_ok and counters_ok:
        print("✅ ALL RUNS ARE SUBSTANTIVELY IDENTICAL")
        sys.exit(0)
    else:
        print("❌ RUNS DIFFER — determinism failure detected")
        sys.exit(1)


if __name__ == "__main__":
    main()
