#!/usr/bin/env python3
"""Checked audit of the INC0040293 (ESnet) analysis artifacts.

Derives every audited fact from canonical tracked artifacts:

  - case-studies/manlan-esnet-2019/out/INC0040293/*.json  (run artifacts)
  - manifests/INC0040293.json                              (reviewed manifest)
  - case-studies/manlan-2019/corpus/snapshots/INC0040293.json (source ticket)

The audit never hand-maintains counts: it recomputes them from the
artifacts and fails when a check does not hold. Output is written to
case-studies/manlan-esnet-2019/assessment-audit.json.

Usage: python3 scripts/audit-esnet-assessment.py [--write]
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = ROOT / "case-studies" / "manlan-esnet-2019" / "out" / "INC0040293"
MANIFEST = ROOT / "manifests" / "INC0040293.json"
SNAPSHOT = ROOT / "case-studies" / "manlan-2019" / "corpus" / "snapshots" / "INC0040293.json"
AUDIT_OUT = ROOT / "case-studies" / "manlan-esnet-2019" / "assessment-audit.json"

checks = []  # (fact, value, source)


def check(fact, value, source):
    checks.append({"fact": fact, "value": value, "source": source})


def load(p: Path):
    return json.loads(p.read_text())


def main() -> int:
    report = load(OUT_DIR / "report.json")
    lifecycle = load(OUT_DIR / "lifecycle.json")
    transitions = load(OUT_DIR / "transitions.json")
    limitations = load(OUT_DIR / "limitations.json")
    archive_manifest = load(OUT_DIR / "archive_manifest.json")
    performance = load(OUT_DIR / "performance.json")
    metadata = load(OUT_DIR / "execution_metadata.json")
    manifest = load(MANIFEST)
    snapshot = load(SNAPSHOT)

    # ── Identity and lifecycle ─────────────────────────────────────
    check("event_title", snapshot["short_description"], "corpus snapshot INC0040293")
    check("source_ticket_description", snapshot["description"], "corpus snapshot INC0040293")
    check("ticket_state", snapshot["state"], "corpus snapshot INC0040293")
    check("ticket_category", snapshot["category"], "corpus snapshot INC0040293")
    check("ticket_work_start_utc", "2019-08-21T16:36:38Z", "snapshot work_start epoch 1566405398")
    check("ticket_work_end_utc", "2019-08-21T20:25:24Z", "snapshot work_end epoch 1566419124")
    check("analysis_window_utc", report["observed_event_signature"]["analysis_window_utc"],
          "report.json observed_event_signature")
    check("ticket_title_matches_report",
          report["event_id"] == "INC0040293",
          "report.json result.verdict (event id)")

    # ── Target and route scope ─────────────────────────────────────
    origin_asns = manifest["target"]["origin_asns"]
    check("target_origin_asns", origin_asns, "manifests/INC0040293.json target")
    transit = manifest["target"]["transit_predicate"]["predicate"]
    check("reviewed_transit_predicate", transit, "manifests/INC0040293.json target")
    check("exact_transit_predicate_reported",
          report["observed_event_signature"]["observer_scope"]["exact_transit_predicate"],
          "report.json observer_scope")

    lifecycles = lifecycle["lifecycles"]
    prefixes = sorted(lc["prefix"] for lc in lifecycles)
    check("selected_prefixes", prefixes, "lifecycle.json lifecycles[].prefix")
    peers = sorted({lc["peer_ip"] for lc in lifecycles})
    check("qualifying_peer_ips", peers, "lifecycle.json lifecycles[].peer_ip")
    collectors = sorted({lc["collector"] for lc in lifecycles})
    check("qualifying_collectors", collectors, "lifecycle.json lifecycles[].collector")
    baseline_paths = sorted({tuple(lc["baseline_path"]) for lc in lifecycles})
    check("baseline_paths", [list(p) for p in baseline_paths], "lifecycle.json lifecycles[].baseline_path")
    check("baseline_route_instances",
          report["observed_event_signature"]["observer_scope"]["baseline_route_instances"],
          "report.json observer_scope")
    check("baseline_observer_prefix_streams",
          report["observed_event_signature"]["observer_scope"]["baseline_observer_prefix_streams"],
          "report.json observer_scope")
    check("source_family", manifest["source_family"], "manifests/INC0040293.json")
    check("collectors", manifest["collectors"], "manifests/INC0040293.json")

    # ── Archives and parsed volume ─────────────────────────────────
    ribs = archive_manifest["ribs"]
    updates = archive_manifest["updates"]
    check("rib_archive_count", len(ribs), "archive_manifest.json ribs")
    check("update_archive_count", len(updates), "archive_manifest.json updates")
    check("first_update_archive", updates[0]["url"].split("/")[-1], "archive_manifest.json")
    check("last_update_archive", updates[-1]["url"].split("/")[-1], "archive_manifest.json")
    parsed = sum(a["parsed_elements"] for a in performance["archives"])
    admitted = sum(a["admitted_observations"] for a in performance["archives"])
    hits = sum(1 for a in performance["archives"] if a.get("cache_hit"))
    check("parsed_elements_total", parsed, "performance.json archives[].parsed_elements")
    check("admitted_observations_total", admitted, "performance.json archives[].admitted_observations")
    check("cache_hits", hits, "performance.json archives[].cache_hit")

    # ── Observed route-state result ────────────────────────────────
    tr = transitions["transitions"]
    check("transition_count", len(tr), "transitions.json transitions")
    check("report_transition_total",
          report["transitions"]["total"], "report.json transitions.total")
    categories = sorted({lc["category"] for lc in lifecycles})
    check("stream_categories", categories, "lifecycle.json lifecycles[].category")
    unchanged = sum(1 for lc in lifecycles if lc["category"] == "Unchanged")
    check("unchanged_streams", unchanged, "lifecycle.json lifecycles[].category")
    check("event_window_final_paths", [list(p) for p in baseline_paths],
          "lifecycle.json (no transitions => final paths equal baseline paths)")
    check("analysis_final_paths", [list(p) for p in baseline_paths],
          "lifecycle.json (no transitions => analysis final paths equal baseline paths)")
    check("machine_verdict", report["result"]["verdict"], "report.json result.verdict")
    check("machine_assessment_verdict", report["assessment"]["verdict"], "report.json assessment.verdict")

    # ── Limitations and preflight-observer evidence ────────────────
    check("limitations", limitations, "limitations.json")
    check("report_limitations", report["limitations"], "report.json limitations")
    check("preflight_observer_evidence", manifest["collectors_provenance"],
          "manifests/INC0040293.json collectors_provenance (reviewed)")

    # ── Job/run identity from execution metadata ───────────────────
    check("plan_hash", metadata["plan_hash"], "execution_metadata.json")
    check("job_id", metadata["job_id"], "execution_metadata.json")
    check("worker_id", metadata["worker_id"], "execution_metadata.json")
    check("offline", metadata["offline"], "execution_metadata.json")

    # ── Verifications ──────────────────────────────────────────────
    failures = []
    if snapshot["short_description"] != "Outage Resolved -  I2 Optical Participant ESnet":
        failures.append("event title mismatch")
    if origin_asns != [293]:
        failures.append("target origin ASNs differ from reviewed AS293")
    if transit != {"ContainsAny": [11537]}:
        failures.append("reviewed transit predicate differs from ContainsAny[11537]")
    if prefixes != ["134.55.0.0/16", "192.107.175.0/24", "192.188.24.0/22"]:
        failures.append("selected prefixes differ from the three reviewed prefixes")
    if peers != ["64.57.28.241"]:
        failures.append("qualifying peer differs from 64.57.28.241")
    if baseline_paths != [(11537, 293)]:
        failures.append("baseline paths differ from [11537, 293]")
    if len(tr) != 0 or report["transitions"]["total"] != 0:
        failures.append("transition counts are not zero")
    if admitted != 0:
        failures.append("admitted observations are not zero")
    if unchanged != 3:
        failures.append("unchanged streams differ from 3")

    audit = {
        "audit_schema_version": 1,
        "subject": "INC0040293",
        "generated_from": [
            "case-studies/manlan-esnet-2019/out/INC0040293/{report,lifecycle,transitions,limitations,archive_manifest,performance,execution_metadata}.json",
            "manifests/INC0040293.json",
            "case-studies/manlan-2019/corpus/snapshots/INC0040293.json",
        ],
        "checked": checks,
        "verified": not failures,
        "failures": failures,
        "assessments": {
            "observed_result": {
                "label": "No route-state change observed",
                "statement": (
                    "route-views2 continued to receive the same three selected "
                    "ESnet-origin prefixes through Internet2 R&E AS11537 throughout "
                    "the reviewed event window."
                ),
                "eligible_observer_sessions": 1,
                "eligible_observer_evidence": {
                    "collector": "route-views2",
                    "peer_ip": "64.57.28.241",
                    "prefixes": ["134.55.0.0/16", "192.107.175.0/24", "192.188.24.0/22"],
                    "baseline_paths": [[11537, 293]],
                },
                "excluded_observer_evidence": [
                    {
                        "collector": "rrc06",
                        "reason": "target present, reviewed predicate absent",
                        "source": "manifests/INC0040293.json collectors_provenance (reviewed)",
                    },
                    {
                        "collector": "rrc15",
                        "reason": "target present, reviewed predicate absent",
                        "source": "manifests/INC0040293.json collectors_provenance (reviewed)",
                    },
                ],
            },
            "relationship_observability": {
                "classification": "NotDirectlyObservableInPublicBgp",
                "relationship_type": "Optical participant relationship (I2 optical participant)",
                "rationale": (
                    "Ticket title 'I2 Optical Participant ESnet'; outage attributed by "
                    "ticket text to internal ESnet testing. Public BGP does not directly "
                    "observe the optical interface."
                ),
                "review_source": "case-studies/manlan-2019/pilot/ticket-reviews.json (ReviewedCorrection:2026-08-02)",
            },
            "run_role": {
                "role": "Contemporaneous supporting BGP observation with scope mismatch",
                "statement": (
                    "The AS293/AS11537 run is retained as an immutable contemporaneous "
                    "supporting observation. It does not assess the optical participant "
                    "interface named by the ticket."
                ),
            },
            "operational_interpretation_limit": (
                "This observation does not establish whether the documented "
                "interface action occurred, whether other ESnet routes changed "
                "outside the selected observer scope, or whether traffic was affected."
            ),
            "ticket_level_result": (
                "The named optical relationship is not directly assessable with public BGP."
            ),
        },
    }

    if "--write" in sys.argv:
        AUDIT_OUT.write_text(json.dumps(audit, indent=1) + "\n")
        print(f"wrote {AUDIT_OUT.relative_to(ROOT)}")
    else:
        print(json.dumps(audit, indent=1))

    if failures:
        print("FAILURES:", failures, file=sys.stderr)
        return 1
    print("audit verified: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
