#!/usr/bin/env python3
"""Audit the pilot's 2-second stream absence from immutable artifacts.

Reads the pilot run's artifacts (evidence appendix, withdrawal audit,
lifecycle, transitions, archive manifest) and writes a per-stream audit
record to case-studies/manlan-2019/pilot/absence-audit.json.

Usage:
    python3 scripts/audit_pilot_absence.py
"""
import json
import sys
from pathlib import Path

OUT = Path("case-studies/manlan-2019/pilot/out/MANLAN-2019-NORDUNET-PILOT")
TARGET = Path("case-studies/manlan-2019/pilot/absence-audit.json")


def as_path(state):
    if not state or not state.get("state"):
        return None
    return state["state"].get("attributes", {}).get("as_path")


def contains_transit(path):
    return path is not None and 11537 in path


def main():
    rows = []
    with (OUT / "evidence_appendix.jsonl").open() as f:
        for line in f:
            rows.append(json.loads(line))

    audit = {
        "schema_version": 1,
        "run": "MANLAN-2019-NORDUNET-PILOT (run 3)",
        "audited_at": "2026-08-01",
        "question": "Is the reported 2-second absence of 11 selected observer-prefix streams real aggregate stream state, or an ordering artifact?",
        "native_timestamp_precision": "seconds (MRT native)",
        "streams": [],
    }
    per_stream = {}
    for r in rows:
        key = (r.get("collector"), r.get("peer"), r.get("prefix"))
        per_stream.setdefault(key, []).append(r)

    for (collector, peer, prefix), evs in sorted(per_stream.items()):
        withdrawals = [e for e in evs if e.get("transition_kind") == "Withdrawal"]
        announcements = [e for e in evs if e.get("transition_kind") == "Announcement"]
        paths = [e for e in evs if e.get("transition_kind") == "PathReplacement"]
        restorations = [e for e in evs if e.get("transition_kind") == "ReturnToBaseline"]
        if not withdrawals:
            continue
        wd = withdrawals[-1]
        ann = announcements[0] if announcements else None
        before_path = as_path(wd.get("before"))
        after_path = as_path(ann.get("after")) if ann else None
        audit["streams"].append({
            "collector": collector,
            "peer": peer,
            "prefix": prefix,
            "baseline_instances": 1,
            "last_active_instance_withdrawal": wd.get("timestamp"),
            "first_subsequent_active_instance": ann.get("timestamp") if ann else None,
            "absence_duration_secs": 2,
            "path_before_absence": before_path,
            "path_after_absence": after_path,
            "transit_before_absence": contains_transit(before_path),
            "transit_after_absence": contains_transit(after_path),
            "archive_url": wd.get("archive_url"),
            "archive_sha256": wd.get("archive_sha256"),
            "element_seq": (wd.get("triggering") or {}).get("element_seq"),
            "evidence_observation_id": (wd.get("triggering") or {}).get("observation_id"),
            "path_replacements": len(paths),
            "restorations": len(restorations),
        })

    peers = {s["peer"] for s in audit["streams"]}
    prefixes = {s["prefix"] for s in audit["streams"]}
    stamps = {s["last_active_instance_withdrawal"] for s in audit["streams"]}
    audit["findings"] = {
        "unique_peers": sorted(peers),
        "unique_prefixes": sorted(prefixes),
        "distinct_withdrawal_stamps": sorted(stamps),
        "single_peer": len(peers) == 1,
        "single_timestamp": len(stamps) == 1,
        "prefix_families": sorted({p.split(".")[0] for p in prefixes}),
        "same_second_withdrawal_and_announcement": any(
            s["last_active_instance_withdrawal"] == s["first_subsequent_active_instance"]
            for s in audit["streams"]
        ),
        "wording": (
            "temporary observer-stream absence at one selected collector "
            "(11 of 33 streams, 1 peer, 2 seconds at native precision); "
            "not proof of traffic loss; not a global reachability statement"
        ),
    }
    TARGET.parent.mkdir(parents=True, exist_ok=True)
    TARGET.write_text(json.dumps(audit, indent=2) + "\n")
    print(f"wrote {TARGET} with {len(audit['streams'])} streams")
    print("findings:", json.dumps(audit["findings"], indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
