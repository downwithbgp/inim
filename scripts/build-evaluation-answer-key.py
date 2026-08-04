#!/usr/bin/env python3
"""Generate the evidence-derived facilitator answer key for the NOC alpha
evaluation (offline, deterministic).

Reads only reviewed tracked artifacts and the deterministic demo
manifest; never reads runtime databases, caches, or live sources.
Writes:

    evaluation/generated/answer-key.json   (authoritative, machine-readable)
    evaluation/generated/answer-key.md     (facilitator-readable)

Both outputs carry a generation header: generator command, schema
version, and the source demo-manifest SHA-256. No volatile timestamps
are embedded; regeneration is byte-deterministic for the same tracked
inputs.

Usage:
    python3 scripts/build-evaluation-answer-key.py \
        --db /path/to/demo.sqlite --root . --out evaluation/generated

The --db argument locates the demo-manifest.json written next to the
demo database by `inim demo init` (its SHA-256 is recorded in the
header). --db is optional when the manifest is not needed (CI offline
checks may omit it).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tomllib
from pathlib import Path

SCHEMA_VERSION = 1
GENERATOR = "scripts/build-evaluation-answer-key.py"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def load_json(path: Path) -> dict:
    return json.loads(path.read_text())


def fail(msg: str) -> None:
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(1)


# ─────────────────────────────────────────────────────────────────────
# Artifact helpers (repository-relative paths only; never absolute)
# ─────────────────────────────────────────────────────────────────────

def artifact(root: Path, *parts: str) -> Path:
    return root.joinpath(*parts)


def checked_json(root: Path, *parts: str) -> dict:
    p = artifact(root, *parts)
    if not p.is_file():
        fail(f"missing reviewed artifact: {'/'.join(parts)}")
    return load_json(p)


def path_as_ref(*parts: str) -> str:
    """Repository-relative artifact reference for the answer key."""
    return "/".join(parts)


def asn_path(path: list) -> str:
    return " ".join(str(a) for a in path) if path else "—"


# ─────────────────────────────────────────────────────────────────────
# Scenario sections
# ─────────────────────────────────────────────────────────────────────

def nordunet_section(root: Path) -> dict:
    pilot = checked_json(root, "case-studies/manlan-2019/pilot", "pilot-result.json")
    matrix = checked_json(root, "case-studies/manlan-2019/pilot", "cross-observer-matrix.json")
    rrc15 = checked_json(
        root, "case-studies/manlan-2019/pilot/out",
        "MANLAN-2019-NORDUNET-PILOT-RIS-RRC15", "report.json",
    )
    rv2_lifecycle = checked_json(
        root, "case-studies/manlan-2019/pilot/out",
        "MANLAN-2019-NORDUNET-PILOT-RE-RV2", "lifecycle.json",
    )
    # Direct route-views2 run facts from the reviewed matrix. Missing
    # collectors are a reviewed-artifact shape change: fail loudly.
    def matrix_run(collector: str) -> dict:
        for r in matrix["re_plane_runs"]:
            if r["collector"] == collector:
                return r
        fail(f"cross-observer-matrix.json missing collector {collector}")

    direct = matrix_run("route-views2")
    rrc00 = matrix_run("rrc00")
    rrc06 = matrix_run("rrc06")
    rrc15m = matrix_run("rrc15")
    # Exact baseline-restoration range for the 11-prefix group: the
    # ReturnToBaseline transition timestamps of the temporarily absent
    # streams only (matches the workbench's "11-prefix group" claim;
    # the pilot result reports all 33 streams restored by 17:02:19Z).
    restorations = []
    absences = []  # (withdrawal_ts, return_ts) for the absent streams
    for lc in rv2_lifecycle["lifecycles"]:
        if not lc.get("was_withdrawn", False):
            continue
        first_return = None
        for t in lc.get("transitions", []):
            if t.get("kind") == "ReturnToBaseline" and t.get("timestamp"):
                restorations.append(t["timestamp"])
            if t.get("kind") == "Announcement" and t.get("timestamp") and first_return is None:
                first_return = t["timestamp"]
        withdrawal = lc.get("stream_withdrawal_time")
        if withdrawal and first_return:
            absences.append((withdrawal, first_return))
    restoration_range = (
        (min(restorations), max(restorations)) if restorations else (None, None)
    )
    # Absence duration: earliest withdrawal to earliest return across
    # the absent group (2 s in the reviewed pilot result).
    absence_seconds = None
    if absences:
        def iso_ts(s: str) -> float:
            return float(s.replace("Z", "")) if "T" in s else float(s)
        w = min(a[0] for a in absences)
        r = min(a[1] for a in absences)
        try:
            from datetime import datetime, timezone
            fmt = "%Y-%m-%dT%H:%M:%S.%fZ" if "." in w else "%Y-%m-%dT%H:%M:%SZ"
            wdt = datetime.strptime(w, fmt).replace(tzinfo=timezone.utc)
            fmt2 = "%Y-%m-%dT%H:%M:%S.%fZ" if "." in r else "%Y-%m-%dT%H:%M:%SZ"
            rdt = datetime.strptime(r, fmt2).replace(tzinfo=timezone.utc)
            absence_seconds = round((rdt - wdt).total_seconds())
        except ValueError:
            absence_seconds = None
    # RRC15 cooldown transitions (11 path replacements after 17:30).
    transitions = rrc15.get("transitions", {})
    cooldown_count = transitions.get("cooldown", 0)
    rrc15_transitions = checked_json(
        root, "case-studies/manlan-2019/pilot/out",
        "MANLAN-2019-NORDUNET-PILOT-RIS-RRC15", "transitions.json",
    )
    cooldown_first = None
    for t in rrc15_transitions.get("transitions", []):
        if t.get("phase") == "Cooldown" and t.get("occurred_utc"):
            ts = t["occurred_utc"]
            if cooldown_first is None or ts < cooldown_first:
                cooldown_first = ts
    if cooldown_first is None:
        fail("RRC15 transitions: no Cooldown-phase transition found")
    return {
        "source_event": {
            "id": "MAN LAN 2019-08-21 (multi-ticket operator incident)",
            "case_study_slug": pilot["case_study_slug"],
            "reference": path_as_ref("case-studies/manlan-2019/pilot/pilot-result.json"),
        },
        "target": {
            "name": pilot["target"],
            "reference": path_as_ref("case-studies/manlan-2019/pilot/pilot-result.json"),
        },
        "reviewed_relationship": {
            "text": "NORDUnet (AS2603) routes via the Internet2 R&E plane (AS11537)",
            "predicate": "origin AS2603 AND baseline AS path contains AS11537",
            "reference": path_as_ref("case-studies/manlan-2019/pilot/cross-observer-matrix.json"),
        },
        "analysis_window_utc": pilot["window_start_utc"] + " .. " + pilot["window_end_utc"],
        "observers": [
            {
                "collector": "route-views2",
                "collector_site": "Eugene, Oregon, US",
                "source_family": "RouteViews",
                "relationship": "direct R&E (peer ASN 11537)",
                "peer_ip": "64.57.28.241",
                "peer_asn": 11537,
                "baseline_streams": direct["baseline_streams"],
                "baseline_prefixes": direct["baseline_prefixes"],
                "temporary_stream_absences": direct["temporary_stream_absences"],
                "evidence_interval_utc": direct["evidence_interval_utc"],
                "verdict": direct["verdict"],
                "reference": path_as_ref("case-studies/manlan-2019/pilot/cross-observer-matrix.json"),
            },
            {
                "collector": "rrc00",
                "collector_site": "Amsterdam, Netherlands",
                "source_family": "RipeRis",
                "relationship": "indirect R&E (path contains AS11537)",
                "peer_ip": "203.119.104.1",
                "peer_asn": 4608,
                "baseline_streams": rrc00["baseline_streams"],
                "temporary_stream_absences": rrc00["temporary_stream_absences"],
                "verdict": rrc00["verdict"],
                "reference": path_as_ref("case-studies/manlan-2019/pilot/cross-observer-matrix.json"),
            },
            {
                "collector": "rrc06",
                "collector_site": "Otemachi, Tokyo, Japan",
                "source_family": "RipeRis",
                "relationship": "indirect R&E (path contains AS11537)",
                "peer_ip": "202.249.2.20",
                "peer_asn": 4777,
                "baseline_streams": rrc06["baseline_streams"],
                "temporary_stream_absences": rrc06["temporary_stream_absences"],
                "evidence_interval_utc": rrc06["evidence_interval_utc"],
                "verdict": rrc06["verdict"],
                "finding": rrc06["finding"],
                "reference": path_as_ref("case-studies/manlan-2019/pilot/cross-observer-matrix.json"),
            },
            {
                "collector": "rrc15",
                "collector_site": "Sao Paulo, Brazil",
                "source_family": "RipeRis",
                "relationship": "indirect R&E (path contains AS11537)",
                "peer_ip": "187.16.216.4",
                "peer_asn": 1916,
                "baseline_streams": rrc15m["baseline_streams"],
                "temporary_stream_absences": rrc15m["temporary_stream_absences"],
                "evidence_interval_utc": rrc15m["evidence_interval_utc"],
                "verdict": rrc15m["verdict"],
                "cooldown_transitions": cooldown_count,
                "cooldown_reference": path_as_ref(
                    "case-studies/manlan-2019/pilot/out",
                    "MANLAN-2019-NORDUNET-PILOT-RIS-RRC15", "report.json",
                ),
                "reference": path_as_ref("case-studies/manlan-2019/pilot/cross-observer-matrix.json"),
            },
        ],
        "route_changes": {
            "first_direct_absence_utc": direct["evidence_interval_utc"].split("..")[0].strip(),
            "affected_prefix_count": direct["temporary_stream_absences"],
            "absence_duration_seconds": absence_seconds if absence_seconds is not None else "not derived",
            "returned_path": "11537 22388 24489 24489 24489 24489 24490 20965 2603 (still traverses AS11537)",
            "exact_baseline_restoration_range_utc": (
                restoration_range[0] + " .. " + restoration_range[1]
                if restoration_range[0]
                else "not observed"
            ),
            "analysis_final_state": "exact event-baseline path present at analysis end (18:30:00 UTC)",
            "rrc15_cooldown": {
                "count": cooldown_count,
                "first_change_utc": cooldown_first,
                "note": "path replacements in the cooldown window; no restoration observed before analysis end",
                "reference": path_as_ref(
                    "case-studies/manlan-2019/pilot/out",
                    "MANLAN-2019-NORDUNET-PILOT-RIS-RRC15", "report.json",
                ),
            },
            "reference": path_as_ref("case-studies/manlan-2019/pilot/pilot-result.json"),
        },
        "observed_result": {
            "direct_observer": pilot["bgp_observation"],
            "finding": pilot["finding"],
            "reference": path_as_ref("case-studies/manlan-2019/pilot/pilot-result.json"),
        },
        "non_conclusions": [
            "collector site (Eugene, Oregon, US) does not establish peer location or target location",
            "observer-route absence does not prove traffic loss",
            "a 2-second absence at one observer may reflect session behavior rather than the participant's own action",
            "this is a single-target, single-collector-direct pilot — not a complete MAN LAN incident verdict",
            "temporal association with the reported instability interval is not attribution to a specific interface action",
        ],
        "likely_confusions": [
            "exact baseline returned (17:02:03Z) versus final route state",
            "one observer's result (route-views2) versus all observers",
            "collector site versus peer location",
        ],
        "evidence_needed": [
            "the direct observer session (route-views2 peer 64.57.28.241) for the absence/return pair",
            "the exact-baseline restoration timestamps per prefix (lifecycle.json)",
            "the RRC15 cooldown transitions (report.json transitions.cooldown = 11)",
        ],
        "unsupported_stronger_conclusion": [
            "attributing the absence to the reported 16:50 interface-disable action",
            "claiming traffic interruption from the 2-second BGP absence",
            "an incident-wide MAN LAN assessment from this single-target pilot",
        ],
    }


def uva_section(root: Path) -> dict:
    audit = checked_json(root, "case-studies/inc0299001", "finding-chronology-audit.json")
    report = checked_json(root, "case-studies/inc0299001/out/INC0299001", "report.json")
    manifest = checked_json(root, "manifests", "INC0299001.json")
    prefixes = audit["prefixes"]
    # Principal 11-prefix group: 54 ms absences; the 12th prefix differs.
    principal = [p for p in prefixes if p.get("absence_duration_secs", 0) < 1.0]
    outlier = [p for p in prefixes if p.get("absence_duration_secs", 0) >= 1.0]
    if len(principal) != 11 or len(outlier) != 1:
        fail(
            f"UVA chronology audit changed shape: expected 11 principal + 1 outlier, "
            f"got {len(principal)} + {len(outlier)}"
        )
    p0 = principal[0]
    out = outlier[0]
    withdrawal = p0["withdrawal_timestamp"]
    # Pre-withdrawal route: the path immediately before the Withdrawal
    # transition; first-return timestamp: the Announcement after it.
    pre_withdrawal = None
    return_ts = None
    for t in p0["transitions"]:
        if t["kind"] == "Withdrawal":
            pre_withdrawal = t.get("before_path")
        if t["kind"] == "Announcement":
            return_ts = t["timestamp"]
    if pre_withdrawal is None or return_ts is None:
        fail("UVA chronology audit: withdrawal/announcement transitions missing")
    absence_secs = round(p0["absence_duration_secs"], 3)
    # Prepend-count change: count the target-ASN repetitions in the
    # event-baseline path vs the pre-withdrawal path.
    target_asn = manifest["target"]["origin_asns"][0]
    baseline_count = sum(1 for a in p0["baseline_route"] if a == target_asn)
    pre_count = sum(1 for a in pre_withdrawal if a == target_asn)
    prepend_change = (
        f"AS{target_asn} prepend reduced from {baseline_count} to {pre_count} "
        f"while routes remained visible"
    )
    return {
        "source_event": {
            "id": "INC0299001",
            "reference": path_as_ref("case-studies/inc0299001/finding-chronology-audit.json"),
        },
        "target": {
            "name": manifest["target"]["label"],
            "origin_asns": manifest["target"]["origin_asns"],
            "reference": path_as_ref("manifests/INC0299001.json"),
        },
        "reviewed_relationship": {
            "text": "UVA (AS225) via Internet2 (AS11537)",
            "predicate": "origin AS225 AND baseline AS path contains AS11537",
            "reference": path_as_ref("manifests/INC0299001.json"),
        },
        "analysis_window_utc": report["observed_event_signature"]["analysis_window_utc"],
        "observers": [
            {
                "collector": "route-views2",
                "collector_site": "Eugene, Oregon, US",
                "source_family": "RouteViews",
                "relationship": "direct Internet2 session (peer ASN 11537)",
                "peer_ip": "163.253.3.14",
                "peer_asn": 11537,
                "note": "the chronology audit session for the 11-prefix group",
                "reference": path_as_ref("case-studies/inc0299001/finding-chronology-audit.json"),
            }
        ],
        "route_changes": {
            "event_baseline_path": asn_path(p0["baseline_route"]),
            "pre_withdrawal_path": asn_path(pre_withdrawal),
            "prepend_count_change": prepend_change,
            "withdrawal_timestamp": withdrawal,
            "return_timestamp": return_ts,
            "absence_duration_secs": absence_secs,
            "first_returned_path": asn_path(p0["first_returned_path"]),
            "final_path": asn_path(p0["analysis_final_path"]),
            "final_matches": "pre-withdrawal route (AS225×1), not the event baseline (AS225×7)",
            "principal_prefix_count": len(principal),
            "example_prefixes": [p["prefix"] for p in principal[:3]],
            "outlier_prefix": {
                "prefix": out["prefix"],
                "absence_duration_secs": round(out["absence_duration_secs"], 3),
                "baseline_path": asn_path(out["baseline_route"]),
                "final_path": asn_path(out["analysis_final_path"]),
                "note": "baseline already the reduced path; much longer absence than the 11-prefix group",
            },
            "reference": path_as_ref("case-studies/inc0299001/finding-chronology-audit.json"),
        },
        "observed_result": {
            "verdict": report["result"]["verdict_label"],
            "finding": report["result"]["finding"],
            "reference": path_as_ref("case-studies/inc0299001/out/INC0299001/report.json"),
        },
        "expectation": "ParticipantRelationshipUnavailable (no parenthesized site code in the title)",
        "non_conclusions": [
            "observer-route absence does not prove traffic loss",
            "the 54 ms absence at one observer session is not a measured outage duration",
            "exact baseline restoration at return does not mean the final state matched the baseline",
        ],
        "likely_confusions": [
            "event baseline (AS225×7) versus pre-withdrawal route (AS225×1)",
            "prepend change (07:24:47Z) versus withdrawal (07:33:59Z)",
            "11-prefix group versus the 12th prefix (137.54.122.0/23)",
        ],
        "evidence_needed": [
            "the finding-chronology audit for prefix-level baseline/withdrawal/return/final paths",
            "the report.json finding for the 13-of-48 stream signature",
        ],
        "unsupported_stronger_conclusion": [
            "claiming the final state restored the event baseline",
            "claiming traffic impact from BGP absence",
        ],
    }


def i2px_section(root: Path) -> dict:
    audit = checked_json(root, "case-studies/inc0302574/out/INC0302574", "relationship-audit.json")
    report = checked_json(root, "case-studies/inc0302574/out/INC0302574", "report.json")
    scope = report["observed_event_signature"]["observer_scope"]
    return {
        "source_event": {
            "id": audit["event_id"],
            "title": audit["relationship"],
            "reference": path_as_ref("case-studies/inc0302574/out/INC0302574/relationship-audit.json"),
        },
        "target": {
            "name": "RIPE (AS3333)",
            "reference": path_as_ref("case-studies/inc0302574/out/INC0302574/relationship-audit.json"),
        },
        "reviewed_relationship": {
            "text": audit["relationship"],
            "reference": path_as_ref("case-studies/inc0302574/out/INC0302574/relationship-audit.json"),
        },
        "direct_sessions_reviewed": [
            {
                "collector": s["collector"],
                "peer_ip": s["peer_ip"],
                "address_family": s["address_family"],
                "peer_asn": s["peer_asn"],
                "as3333_origin_route_count": s["as3333_origin_route_count"],
            }
            for s in audit["direct_i2px_sessions"]
        ],
        "non_qualification_reason": (
            "all four direct AS11164 sessions existed on 2026-07-30 but carried zero "
            "AS3333-origin routes; no AS3333-origin path contained AS11164; no qualifying "
            "I2PX baseline exists"
        ),
        "supporting_observation": {
            "text": report["result"]["finding"],
            "verdict": report["result"]["verdict_label"],
            "baseline_observer_prefix_streams": scope["baseline_observer_prefix_streams"],
            "collectors": scope["collectors"],
            "note": "supporting AS11537 observation only; does not assess the named I2PX relationship",
            "reference": path_as_ref("case-studies/inc0302574/out/INC0302574/report.json"),
        },
        "decision": audit["decision"],
        "assessment": audit["assessment"],
        "strongest_conclusion": (
            "the named I2PX relationship cannot be assessed from public-collector evidence "
            "at the event date; the direct sessions existed but carried no target-origin "
            "baseline, so no route-state claim about the relationship is supported"
        ),
        "non_conclusions": [
            "the supporting no-change observation (route-views2/route-views6, AS11537) does not assess the named I2PX relationship",
            "a direct AS11164 session existing does not make the relationship observable without a target-origin baseline",
            "no route-state change on a supporting plane is not 'the I2PX relationship was stable'",
        ],
        "likely_confusions": [
            "direct AS11164 session existed (true) versus a qualifying baseline existed (false)",
            "AS3333-origin routes visible via other peers versus visible through the I2PX sessions",
            "supporting AS11537 evidence treated as named-relationship evidence",
        ],
        "evidence_needed": [
            "the relationship-audit direct-session rows with zero AS3333-origin counts",
            "the supporting report.json 19-stream no-change observation",
        ],
        "unsupported_stronger_conclusion": [
            "claiming the I2PX relationship was stable",
            "claiming the event had no routing impact from a supporting plane",
        ],
    }


def smithville_section(root: Path) -> dict:
    manifest = checked_json(root, "manifests", "INC0301970.json")
    report = checked_json(root, "case-studies/indiana-gigapop-smithville-2026/out/INC0301970", "report.json")
    cutoff = manifest.get("analysis_end_utc") or ""
    return {
        "source_event": {
            "id": manifest["event_id"],
            "open": manifest.get("open", False),
            "reference": path_as_ref("manifests/INC0301970.json"),
        },
        "target": {
            "name": manifest["target"]["label"],
            "origin_asns": manifest["target"]["origin_asns"],
            "reference": path_as_ref("manifests/INC0301970.json"),
        },
        "reviewed_relationship": {
            "text": "Indiana GigaPOP (AS19782) peer Smithville (AS11550)",
            "predicate": "Adjacent(19782, 11550)",
            "provenance": manifest["target"]["transit_predicate"]["provenance"]["statement"],
            "reference": path_as_ref("manifests/INC0301970.json"),
        },
        "collectors_selected": manifest["collectors"],
        "provisional_cutoff_utc": cutoff,
        "provisional_language": (
            "source event remains open; result is provisional through the reviewed "
            "snapshot cutoff; a later source refresh creates a new snapshot and run"
        ),
        "visibility_facts": {
            "as11550_routes_visible": True,
            "as11550_prefix_count": 13,
            "as11550_transit": "Cogent/Telia/BroadbandONE transit only (no AS19782)",
            "routes_traversing_as19782": 0,
            "direct_as19782_sessions": 0,
            "as11550_at_route_views6": 0,
            "note": "facts from the reviewed manifest analyst notes (event-date baseline preflight)",
            "reference": path_as_ref("manifests/INC0301970.json"),
        },
        "result": {
            "verdict": report["result"]["verdict"],
            "verdict_label": report["result"]["verdict_label"],
            "assessment_statement": report["assessment"]["statement"],
            "reference": path_as_ref("case-studies/indiana-gigapop-smithville-2026/out/INC0301970/report.json"),
        },
        "why_insufficient_visibility": (
            "no selected RouteViews observer had a pre-event route matching the reviewed "
            "path predicate, so no qualifying baseline exists; the run records "
            "InsufficientVisibility with no UPDATE acquisition. This is distinct from "
            "observing no route-state change: there was no qualifying observation at all."
        ),
        "non_conclusions": [
            "no qualifying relationship evidence was observed through the reviewed cutoff",
            "not claimed: no relationship existed",
            "not claimed: no routing change occurred",
            "not claimed: Smithville was unaffected",
            "the named peer relationship is not all Smithville connectivity",
            "the result is provisional; the source event remained open at the cutoff",
        ],
        "likely_confusions": [
            "target AS11550 routes were visible (true) versus the reviewed AS19782–AS11550 relationship was visible (false)",
            "insufficient visibility versus no change",
            "named peer relationship versus all Smithville connectivity",
        ],
        "evidence_needed": [
            "the manifest analyst notes (event-date baseline preflight counts)",
            "the report.json InsufficientVisibility assessment",
        ],
        "unsupported_stronger_conclusion": [
            "claiming Smithville had no routing change",
            "claiming the peer relationship was stable",
            "claiming Smithville was unaffected by the outage",
        ],
    }


def esnet_section(root: Path) -> dict:
    report = checked_json(root, "case-studies/manlan-esnet-2019/out/INC0040293", "report.json")
    manifest = checked_json(root, "manifests", "INC0040293.json")
    scope = report["observed_event_signature"]["observer_scope"]
    return {
        "source_event": {
            "id": "INC0040293",
            "reference": path_as_ref("case-studies/manlan-esnet-2019/out/INC0040293/report.json"),
        },
        "target": {
            "name": manifest["target"]["label"],
            "origin_asns": manifest["target"]["origin_asns"],
            "reference": path_as_ref("manifests/INC0040293.json"),
        },
        "reviewed_relationship": {
            "text": "I2 Optical Participant ESnet (optical participant relationship)",
            "reference": path_as_ref("case-studies/manlan-esnet-2019/out/INC0040293/report.json"),
        },
        "scope_statement": "public BGP does not directly observe the named optical participant interface",
        "supporting_observation": {
            "text": report["result"]["finding"],
            "verdict": report["result"]["verdict_label"],
            "baseline_observer_prefix_streams": scope["baseline_observer_prefix_streams"],
            "collectors": scope["collectors"],
            "note": "contemporaneous supporting observation with scope mismatch; retained separately",
            "reference": path_as_ref("case-studies/manlan-esnet-2019/out/INC0040293/report.json"),
        },
        "non_conclusions": [
            "public BGP cannot assess the optical interface state",
            "stable contemporaneous BGP routes do not assess an optical interface",
            "not claimed: less impact than expected",
            "not claimed: no optical impact",
            "not claimed: optical service stayed available",
        ],
        "likely_confusions": [
            "contemporaneous stable BGP routes treated as an optical-interface assessment",
        ],
        "evidence_needed": [
            "the report.json scope-mismatch supporting observation",
            "the event detail page's 'not directly assessable with public BGP' statement",
        ],
        "unsupported_stronger_conclusion": [
            "claiming the optical service stayed available",
            "comparing the supporting observation as an IP-participant result",
        ],
    }


# ─────────────────────────────────────────────────────────────────────
# Assembly
# ─────────────────────────────────────────────────────────────────────

def build(root: Path, demo_manifest_sha: str, demo_manifest: dict) -> dict:
    scenarios = [
        nordunet_section(root),
        uva_section(root),
        i2px_section(root),
        smithville_section(root),
        esnet_section(root),
    ]
    return {
        "schema_version": SCHEMA_VERSION,
        "generator": GENERATOR,
        "source_demo_manifest_sha256": demo_manifest_sha,
        "demo_manifest": demo_manifest,
        "scenarios": scenarios,
        "summary_table": summary_table(scenarios),
    }


def summary_table(scenarios: list) -> list:
    """Part 25 facilitator summary. No severity, no success/failure,
    no incident verdict, no ranking."""
    rows = []
    for s in scenarios:
        rows.append({
            "scenario": s_id_of(s),
            "named_relationship": s["reviewed_relationship"]["text"],
            "target": s["target"]["name"],
            "source_lifecycle": s.get("source_event", {}).get("open", "Closed"),
            "observer_eligibility": observer_eligibility(s),
            "observed_result": observed_result(s),
            "expectation_assessment": expectation_assessment(s),
            "final_state": final_state(s),
            "primary_limitation": primary_limitation(s),
        })
    return rows


def s_id_of(s: dict) -> str:
    mapping = {
        "MAN LAN": "nordunet-route-changes",
        "INC0299001": "uva-prepend-withdrawal",
        "INC0302574": "i2px-not-assessable",
        "INC0301970": "smithville-insufficient-visibility",
        "INC0040293": "esnet-optical-scope",
    }
    for k, v in mapping.items():
        if k in s["source_event"]["id"]:
            return v
    return s["source_event"]["id"]


def observer_eligibility(s: dict) -> str:
    if s["source_event"]["id"] == "INC0302574":
        return "4 direct AS11164 sessions existed; 0 qualifying baselines"
    if s["source_event"]["id"] == "INC0301970":
        return "target visible; reviewed relationship absent; 0 direct sessions"
    if "MAN LAN" in s["source_event"]["id"]:
        return "1 direct (route-views2) + 3 indirect RIS observers"
    if s["source_event"]["id"] == "INC0299001":
        return "direct AS11537 session at route-views2; 48 baseline streams"
    return "1 supporting observer (scope mismatch)"


def observed_result(s: dict) -> str:
    if s["source_event"]["id"] == "INC0302574":
        return "named relationship not assessable (insufficient visibility)"
    if s["source_event"]["id"] == "INC0301970":
        return s["result"]["verdict_label"]
    if "MAN LAN" in s["source_event"]["id"]:
        return "direct observer: 11 streams absent 2 s, returned, baseline restored"
    if s["source_event"]["id"] == "INC0299001":
        return s["observed_result"]["verdict"]
    return "supporting observation only"


def expectation_assessment(s: dict) -> str:
    if "MAN LAN" in s["source_event"]["id"]:
        return "not incident-wide; pilot only"
    if s["source_event"]["id"] == "INC0301970":
        return "insufficient visibility; no expectation assessment possible"
    if s["source_event"]["id"] == "INC0302574":
        return "no expectation assessment for the named relationship"
    return s.get("expectation", "not stated")


def final_state(s: dict) -> str:
    if "MAN LAN" in s["source_event"]["id"]:
        return s["route_changes"]["analysis_final_state"]
    if s["source_event"]["id"] == "INC0299001":
        return s["route_changes"]["final_path"] + " (" + s["route_changes"]["final_matches"] + ")"
    if s["source_event"]["id"] == "INC0301970":
        return "no qualifying observation through cutoff (provisional)"
    if s["source_event"]["id"] == "INC0302574":
        return "not assessable"
    return "not assessable (optical scope)"


def primary_limitation(s: dict) -> str:
    if "MAN LAN" in s["source_event"]["id"]:
        return "single direct observer; 2 s absence may be session behavior"
    if s["source_event"]["id"] == "INC0299001":
        return "observer-scoped BGP only; no traffic measurement"
    if s["source_event"]["id"] == "INC0302574":
        return "no target-origin baseline through direct sessions"
    if s["source_event"]["id"] == "INC0301970":
        return "open event; provisional cutoff; relationship not visible"
    return "optical interface not observable in public BGP"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", help="demo SQLite path (locates demo-manifest.json)", default=None)
    parser.add_argument("--root", default=".", help="repository root")
    parser.add_argument("--out", default="evaluation/generated", help="output directory")
    args = parser.parse_args()

    root = Path(args.root)
    demo_manifest_path = None
    if args.db:
        candidate = Path(args.db).with_name("demo-manifest.json")
        if candidate.is_file():
            demo_manifest_path = candidate
    demo_manifest = {}
    demo_manifest_sha = ""
    if demo_manifest_path:
        demo_manifest = json.loads(demo_manifest_path.read_text())
        demo_manifest_sha = sha256_file(demo_manifest_path)

    doc = build(root, demo_manifest_sha, demo_manifest)

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    json_path = out_dir / "answer-key.json"
    md_path = out_dir / "answer-key.md"
    json_path.write_text(json.dumps(doc, indent=1, sort_keys=True) + "\n")
    md_path.write_text(render_markdown(doc))
    print(f"wrote {json_path}")
    print(f"wrote {md_path}")
    return 0


def render_markdown(doc: dict) -> str:
    lines = [
        "# NOC alpha evaluation — facilitator answer key (generated)",
        "",
        "Generated from reviewed tracked artifacts. This document is for the",
        "facilitator only; it must not be distributed to evaluators.",
        "",
        f"- Generator: `{doc['generator']}`",
        f"- Schema version: {doc['schema_version']}",
        f"- Source demo-manifest SHA-256: `{doc['source_demo_manifest_sha256'] or 'not recorded'}`",
        "",
        "Every factual answer below carries a repository-relative artifact",
        "reference. If a value contradicts the referenced artifact, the artifact",
        "is authoritative and the contradiction is a P0 defect.",
        "",
    ]
    for s in doc["scenarios"]:
        lines.append(f"## Scenario: {s['source_event']['id']}")
        lines.append("")
        lines.append(f"- **Source event**: {s['source_event']['id']}")
        lines.append(f"- **Target**: {s['target']['name']}")
        lines.append(f"- **Reviewed relationship**: {s['reviewed_relationship']['text']}")
        lines.append(f"- **Artifact**: `{s['reviewed_relationship'].get('reference', '')}`")
        lines.append("")
        lines.append("### Route state answers")
        lines.append("")
        if "route_changes" in s:
            rc = s["route_changes"]
            for k, v in rc.items():
                if k == "reference":
                    continue
                if isinstance(v, dict):
                    lines.append(f"- **{k}**:")
                    for kk, vv in v.items():
                        lines.append(f"  - {kk}: {vv}")
                else:
                    lines.append(f"- **{k}**: {v}")
            lines.append(f"- **reference**: `{rc.get('reference', '')}`")
        if "direct_sessions_reviewed" in s:
            lines.append("")
            lines.append("### Direct sessions reviewed")
            lines.append("")
            lines.append("| Collector | Peer IP | Family | Peer ASN | AS3333-origin routes |")
            lines.append("|---|---|---|---|---|")
            for d in s["direct_sessions_reviewed"]:
                lines.append(
                    f"| {d['collector']} | {d['peer_ip']} | {d['address_family']} "
                    f"| {d['peer_asn']} | {d['as3333_origin_route_count']} |"
                )
            lines.append("")
            lines.append(f"- **Non-qualification reason**: {s['non_qualification_reason']}")
        if "visibility_facts" in s:
            lines.append("")
            lines.append("### Visibility facts")
            lines.append("")
            for k, v in s["visibility_facts"].items():
                if k != "reference":
                    lines.append(f"- **{k}**: {v}")
            lines.append(f"- **reference**: `{s['visibility_facts'].get('reference', '')}`")
            lines.append("")
            lines.append(f"- **Provisional cutoff**: {s['provisional_cutoff_utc']}")
            lines.append(f"- **Provisional language**: {s['provisional_language']}")
            lines.append(f"- **Why insufficient visibility**: {s['why_insufficient_visibility']}")
        if "observed_result" in s:
            lines.append("")
            lines.append("### Observed result")
            lines.append("")
            if isinstance(s["observed_result"], dict):
                for k, v in s["observed_result"].items():
                    if k != "reference":
                        lines.append(f"- **{k}**: {v}")
                lines.append(f"- **reference**: `{s['observed_result'].get('reference', '')}`")
            else:
                lines.append(f"- {s['observed_result']}")
        if "assessment" in s:
            lines.append("")
            lines.append(f"- **Assessment**: {s['assessment']}")
        if "strongest_conclusion" in s:
            lines.append("")
            lines.append(f"- **Strongest supported conclusion**: {s['strongest_conclusion']}")
        if "scope_statement" in s:
            lines.append("")
            lines.append(f"- **Scope statement**: {s['scope_statement']}")
        if "supporting_observation" in s:
            lines.append("")
            lines.append("### Supporting observation")
            lines.append("")
            so = s["supporting_observation"]
            for k, v in so.items():
                if k != "reference":
                    lines.append(f"- **{k}**: {v}")
            lines.append(f"- **reference**: `{so.get('reference', '')}`")
        lines.append("")
        lines.append("### Non-conclusions")
        lines.append("")
        for nc in s["non_conclusions"]:
            lines.append(f"- {nc}")
        lines.append("")
        lines.append("### Likely confusion (facilitator markers)")
        lines.append("")
        for c in s["likely_confusions"]:
            lines.append(f"- {c}")
        lines.append("")
        lines.append("### Evidence needed")
        lines.append("")
        for e in s["evidence_needed"]:
            lines.append(f"- {e}")
        lines.append("")
        lines.append("### Unsupported stronger conclusion")
        lines.append("")
        for u in s["unsupported_stronger_conclusion"]:
            lines.append(f"- {u}")
        lines.append("")
    lines.append("## Scenario summary table")
    lines.append("")
    lines.append("| Scenario | Named relationship | Target | Source lifecycle | Observer eligibility | Observed result | Expectation assessment | Final state | Primary limitation |")
    lines.append("|---|---|---|---|---|---|---|---|---|")
    for r in doc["summary_table"]:
        lines.append(
            f"| {r['scenario']} | {r['named_relationship']} | {r['target']} "
            f"| {r['source_lifecycle']} | {r['observer_eligibility']} | {r['observed_result']} "
            f"| {r['expectation_assessment']} | {r['final_state']} | {r['primary_limitation']} |"
        )
    lines.append("")
    lines.append("No severity, success/failure, incident verdict, or ranking is derived.")
    lines.append("")
    return "\n".join(lines)


if __name__ == "__main__":
    sys.exit(main())
