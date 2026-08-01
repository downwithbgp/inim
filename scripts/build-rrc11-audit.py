#!/usr/bin/env python3
"""Session 36, Part 1/2 — RRC11 historical audit report builder.

Consumes the full peer inventory of the RRC11 2019-08-21 baseline bview
(`inim catalog session-audit --full-inventory` output) and writes the
reviewed audit artifacts:

  case-studies/manlan-2019/pilot/rrc11-audit-2019.json
  case-studies/manlan-2019/pilot/rrc11-audit-2019.md

The report is strictly evidence-scoped: it states what the 2019 bview
peer table contained (peer IP, peer ASN, address family, route counts,
path classes). The current peer list is supporting context only and never
overrides the historical evidence.

Usage: python3 scripts/build-rrc11-audit.py INVENTORY.json
"""

import json
import sys
from collections import Counter


def plane_path_count(path_class: dict) -> dict:
    """Map plane id -> count from a path_class.per_plane_contains list."""
    return {plane_id: count for plane_id, count in path_class.get("per_plane_contains", [])}


def build(inventory_path: str) -> dict:
    with open(inventory_path) as f:
        inventory = json.load(f)

    collectors = sorted({r["collector"] for r in inventory})
    report = {
        "schema_version": 1,
        "scope": "selected-observer audit (RRC11 baseline bview, 2019-08-21T00:00:00Z)",
        "scope_note": (
            "This is a SELECTED observer audit of one baseline RIB. It is not an "
            "all-RIS audit and must not be rendered as one."
        ),
        "evidence_source": {
            "family": "RipeRis",
            "collector": "rrc11",
            "rib_filename": "bview.20190821.0000.gz",
            "rib_timestamp_utc": "2019-08-21T00:00:00Z",
            "rib_source_sha256": sorted({r["rib_source_sha"] for r in inventory})[0]
            if inventory
            else None,
        },
        "current_peer_list": {
            "status": "supporting context only",
            "note": (
                "The current RIPE RIS peer list identifies RRC11 (NYIIX, New York) "
                "with a direct peer AS11164 (Internet2 I2PX). That list is CONTEXT, "
                "not historical evidence; the 2019 bview peer table above is the "
                "source of truth for the 2019-08-21 session set."
            ),
        },
        "selected_observers": collectors,
        "sessions": [
            {
                "peer_ip": r["peer_ip"],
                "peer_asn": r["peer_asn"],
                "address_family": r["address_family"],
                "total_route_count": r["total_route_count"],
                "origin_route_count": r["origin_route_count"],
                "distinct_origin_prefixes": r["distinct_origin_prefixes"],
                "path_class": r["path_class"],
                "origin_path_class": r["origin_path_class"],
            }
            for r in inventory
        ],
    }

    sessions = report["sessions"]
    report["summary"] = {
        "session_count": len(sessions),
        "ipv4_session_count": sum(1 for s in sessions if s["address_family"] == "ipv4"),
        "ipv6_session_count": sum(1 for s in sessions if s["address_family"] == "ipv6"),
        "total_route_count": sum(s["total_route_count"] for s in sessions),
        "as2603_origin_route_count": sum(s["origin_route_count"] for s in sessions),
        "as2603_distinct_prefix_count": sum(
            s["distinct_origin_prefixes"] for s in sessions
        ),
        "sessions_with_as2603_routes": sum(
            1 for s in sessions if s["origin_route_count"] > 0
        ),
        "direct_pex_session_present": any(
            s["peer_asn"] == 11164 for s in sessions
        ),
        "direct_pex_peer_rows": [
            {"peer_ip": s["peer_ip"], "address_family": s["address_family"]}
            for s in sessions
            if s["peer_asn"] == 11164
        ],
        "pex_asn_in_any_path": any(
            plane_path_count(s["path_class"]).get("internet2-i2px", 0) > 0
            for s in sessions
        ),
        "pex_asn_in_origin_path": any(
            plane_path_count(s["origin_path_class"]).get("internet2-i2px", 0) > 0
            for s in sessions
        ),
        "as2603_origin_path_distribution": {
            "per_plane": sorted(
                {
                    plane_id: sum(
                        plane_path_count(s["origin_path_class"]).get(plane_id, 0)
                        for s in sessions
                    )
                    for plane_id in ("internet2-re", "internet2-i2px")
                }.items()
            ),
            "neither_plane": sum(
                s["origin_path_class"].get("neither_plane", 0) for s in sessions
            ),
            "total": sum(s["origin_route_count"] for s in sessions),
        },
    }
    return report


def render_markdown(report: dict) -> str:
    s = report["summary"]
    src = report["evidence_source"]
    lines = [
        "# RRC11 historical baseline audit (2019-08-21)",
        "",
        "**Scope:** selected-observer audit of one baseline RIB "
        "(`rrc11/bview.20190821.0000.gz`). This is NOT an all-RIS audit.",
        "",
        f"- Baseline bview timestamp: `{src['rib_timestamp_utc']}`",
        f"- RIB source SHA-256: `{src['rib_source_sha256']}`",
        f"- Session count (all peers): {s['session_count']} "
        f"({s['ipv4_session_count']} IPv4, {s['ipv6_session_count']} IPv6)",
        f"- Total routes in baseline: {s['total_route_count']:,}",
        "",
        "## Direct AS11164 / I2PX session (historical evidence)",
        "",
        f"- Direct session with peer ASN 11164 present in the 2019 bview: "
        f"**{'YES' if s['direct_pex_session_present'] else 'NO'}**",
    ]
    if s["direct_pex_peer_rows"]:
        for row in s["direct_pex_peer_rows"]:
            lines.append(f"  - peer {row['peer_ip']} ({row['address_family']})")
    else:
        lines.append("  - (no peer row with peer ASN 11164 in the baseline peer table)")
    lines += [
        f"- Routes received from AS11164: "
        f"{sum(r['total_route_count'] for r in report['sessions'] if r['peer_asn'] == 11164)}",
        f"- AS11164 appears inside some other session's AS path: "
        f"{'YES' if s['pex_asn_in_any_path'] else 'NO'} (indirect observation, "
        f"distinct from a direct session)",
        "",
        "**The current peer list (RRC11/NYIIX direct peer AS11164) is supporting "
        "context only.** It does not establish a 2019 session; the bview peer "
        "table above is the evidence.",
        "",
        "## AS2603-origin visibility at RRC11",
        "",
        f"- AS2603-origin route count: {s['as2603_origin_route_count']}",
        f"- Distinct AS2603 prefixes: {s['as2603_distinct_prefix_count']}",
        f"- Sessions carrying AS2603-origin routes: {s['sessions_with_as2603_routes']}",
        f"- AS2603-origin path distribution: {s['as2603_origin_path_distribution']}",
        "",
        "## Qualifying observer-prefix streams (direct I2PX pilot)",
        "",
    ]
    qualifying = [
        r
        for r in report["sessions"]
        if plane_path_count(r["origin_path_class"]).get("internet2-i2px", 0) > 0
    ]
    if qualifying:
        lines.append(
            f"- {sum(r['origin_route_count'] for r in qualifying)} AS2603-origin "
            "route(s) received with the I2PX plane ASN in path across "
            f"{len(qualifying)} session(s)."
        )
    else:
        lines.append(
            "- **No qualifying AS2603-origin baseline via the I2PX plane**: no "
            "AS2603-origin route at RRC11 contains the I2PX plane ASN in its path, "
            "and no direct AS11164 session exists in the baseline. The direct "
            "I2PX pilot has **no qualifying baseline** at RRC11; absence of a "
            "baseline is NOT evidence of no I2PX-plane event change."
        )
    lines += [
        "",
        "## Session table (all peers in the baseline)",
        "",
        "| peer IP | peer ASN | af | total routes | AS2603 routes | AS2603 prefixes |",
        "|---|---|---:|---:|---:|---:|",
    ]
    for r in sorted(
        report["sessions"], key=lambda x: (x["peer_asn"], x["peer_ip"], x["address_family"])
    ):
        lines.append(
            f"| {r['peer_ip']} | {r['peer_asn']} | {r['address_family']} "
            f"| {r['total_route_count']} | {r['origin_route_count']} "
            f"| {r['distinct_origin_prefixes']} |"
        )
    return "\n".join(lines) + "\n"


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    inventory_path = sys.argv[1]
    report = build(inventory_path)
    out_dir = "case-studies/manlan-2019/pilot"
    with open(f"{out_dir}/rrc11-audit-2019.json", "w") as f:
        json.dump(report, f, indent=1)
        f.write("\n")
    with open(f"{out_dir}/rrc11-audit-2019.md", "w") as f:
        f.write(render_markdown(report))
    print(f"wrote {out_dir}/rrc11-audit-2019.json")
    print(f"wrote {out_dir}/rrc11-audit-2019.md")
    print(f"summary: {json.dumps(report['summary'])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
