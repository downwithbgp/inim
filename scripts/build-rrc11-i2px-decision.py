#!/usr/bin/env python3
"""Direct I2PX pilot decision builder.

Reads the RRC11 historical audit (rrc11-audit-2019.json) and records the
reviewed decision for the direct I2PX pilot: run the reviewed NORDUnet
pilot window through the direct AS11164/I2PX session ONLY if the 2019
baseline contains a direct AS11164 session AND qualifying AS2603-origin
routes via it. Otherwise the exact blocking reason is recorded; the
target is never broadened and the run is never merged with the R&E-plane
runs.

Usage: python3 scripts/build-rrc11-i2px-decision.py
"""

import json
import sys

OUT_DIR = "case-studies/manlan-2019/pilot"
AUDIT = f"{OUT_DIR}/rrc11-audit-2019.json"


def main() -> int:
    with open(AUDIT) as f:
        audit = json.load(f)

    summary = audit["summary"]
    direct_session_present = summary["direct_pex_session_present"]
    # Qualifying baseline: AS2603-origin routes whose path contains the
    # I2PX plane ASN (directly observed via the I2PX plane), OR received
    # from the direct session itself.
    qualifying = [
        r
        for r in audit["sessions"]
        if r["origin_route_count"] > 0
        and any(
            plane == "internet2-i2px" and count > 0
            for plane, count in r["origin_path_class"].get("per_plane_contains", [])
        )
    ]
    qualifying_via_direct = [
        r
        for r in qualifying
        if r["peer_asn"] == 11164
    ]

    if direct_session_present and qualifying:
        decision = "run"
        blocking_reason = None
    elif direct_session_present:
        decision = "blocked-no-qualifying-baseline"
        blocking_reason = (
            "Direct I2PX session present in the historical RRC11 baseline, but "
            "no qualifying NORDUnet baseline visibility: no AS2603-origin route "
            "was received via the direct session or with the I2PX plane ASN in "
            "its path. The direct I2PX pilot was not executed; absence of a "
            "baseline is NOT evidence of no I2PX-plane event change."
        )
    else:
        decision = "blocked-no-direct-session"
        blocking_reason = (
            "No direct AS11164/I2PX session exists in the historical RRC11 "
            "baseline (bview.20190821.0000.gz, 2019-08-21T00:00:00Z): zero of "
            f"{summary['session_count']} peer rows carry peer ASN 11164. The "
            "current peer list (RRC11/NYIIX direct peer AS11164) is supporting "
            "context only and does not establish a 2019 session. The direct "
            "I2PX pilot was not executed."
        )

    artifact = {
        "schema_version": 1,
        "scope": "selected-observer direct I2PX pilot decision (RRC11)",
        "reviewed_target": "NORDUnet (AS2603)",
        "observer_relationship": "direct AS11164/I2PX session at RRC11 (NYIIX, New York)",
        "pilot_window_utc": "2019-08-21T16:00:00Z .. 2019-08-21T17:30:00Z",
        "baseline_bview": {
            "collector": "rrc11",
            "filename": "bview.20190821.0000.gz",
            "timestamp_utc": audit["evidence_source"]["rib_timestamp_utc"],
            "source_sha256": audit["evidence_source"]["rib_source_sha256"],
        },
        "direct_session_present": direct_session_present,
        "qualifying_origin_routes": len(qualifying),
        "qualifying_via_direct_session": len(qualifying_via_direct),
        "decision": decision,
        "blocking_reason": blocking_reason,
        "note": (
            "The direct I2PX pilot is a separate run from the R&E-plane runs; "
            "it is never merged with them. If a qualifying baseline had existed, "
            "the reviewed NORDUnet pilot window would have been run through the "
            "direct session without requiring AS11164 to appear again inside the "
            "exported AS path."
        ),
    }

    with open(f"{OUT_DIR}/rrc11-pex-pilot-decision.json", "w") as f:
        json.dump(artifact, f, indent=1)
        f.write("\n")

    lines = [
        "# RRC11 direct I2PX pilot decision (2019-08-21)",
        "",
        f"- Reviewed target: {artifact['reviewed_target']}",
        f"- Observer relationship: {artifact['observer_relationship']}",
        f"- Pilot window: {artifact['pilot_window_utc']}",
        f"- Baseline bview: {artifact['baseline_bview']['filename']} "
        f"({artifact['baseline_bview']['timestamp_utc']})",
        "",
        f"## Decision: **{decision}**",
        "",
    ]
    if blocking_reason:
        lines.append(f"**Blocking reason:** {blocking_reason}")
    else:
        lines.append(
            "A qualifying baseline exists; the pilot window would be run through "
            "the direct session (not executed by this report builder)."
        )
    lines += [
        "",
        "The direct I2PX pilot is not merged with the R&E-plane runs. "
        "The target is never broadened to create a result.",
        "",
    ]
    with open(f"{OUT_DIR}/rrc11-i2px-pilot-decision.md", "w") as f:
        f.write("\n".join(lines))

    print(f"decision: {decision}")
    print(f"blocking_reason: {blocking_reason}")
    print(f"wrote {OUT_DIR}/rrc11-pex-pilot-decision.json")
    print(f"wrote {OUT_DIR}/rrc11-i2px-pilot-decision.md")
    return 0


if __name__ == "__main__":
    sys.exit(main())
