#!/usr/bin/env python3
"""Session 35, Part 6 — cross-observer matrix builder.

Reads the session audit (MRT peer facts), the four R&E-plane runs, and the
four I2PX preflights; writes cross-observer-matrix.{json,md}. The matrix
keeps every observer's evidence separate — nothing is merged into one
verdict, and direct (peer ASN equals plane ASN) vs indirect (path contains
plane ASN) relationships are never conflated.
"""
import json, os, sys
from collections import OrderedDict

ROOT = "case-studies/manlan-2019/pilot"
OUT_DIR = os.path.join(ROOT, "out")
PLANES = ["internet2-re", "internet2-i2px"]

def load(path):
    with open(path) as f:
        return json.load(f)

def main():
    audit = load(os.path.join(ROOT, "session-audit-2019.json"))
    # Session facts per (family, collector, peer_ip)
    sessions = {}
    for r in audit:
        sessions[(r["source_family"], r["collector"], r["peer_ip"])] = r

    # Run records: per (collector, plane) the run summary.
    runs = {
        "route-views2": ("RE-RV2", "RouteViews"),
        "rrc00": ("RE-RRC00", "RipeRis"),
        "rrc06": ("RE-RRC06", "RipeRis"),
        "rrc15": ("RE-RRC15", "RipeRis"),
    }
    collectors = ["route-views2", "rrc00", "rrc06", "rrc15"]
    locations = {
        ("RouteViews", "route-views2"): "Eugene, Oregon, US",
        ("RipeRis", "rrc00"): "Amsterdam, Netherlands",
        ("RipeRis", "rrc06"): "Otemachi, Tokyo, Japan",
        ("RipeRis", "rrc15"): "Sao Paulo, Brazil",
    }

    matrix_rows = []
    for coll in collectors:
        slug, family = runs[coll]
        run_dir = os.path.join(OUT_DIR, f"MANLAN-2019-NORDUNET-PILOT-{slug}")
        report = load(os.path.join(run_dir, "report.json"))
        transitions = load(os.path.join(run_dir, "transitions.json"))["transitions"]
        lifecycle = load(os.path.join(run_dir, "lifecycle.json"))

        # Cohort streams: distinct (peer_ip, prefix) with baseline. The
        # baseline counts come from the run's lifecycle evidence (streams
        # with zero transitions never appear in transitions.json).
        lifecycle_evidence = report["result"]["finding"]
        import re
        m_total = re.search(r"Across (\d+) selected observer-prefix streams", lifecycle_evidence)
        if not m_total:
            m_total = re.search(r"(\d+) of (\d+) selected observer-prefix streams", lifecycle_evidence)
            n_streams = int(m_total.group(2)) if m_total else 0
        else:
            n_streams = int(m_total.group(1))
        if not m_total:
            m_total = re.search(r"Among the remaining (\d+) streams", lifecycle_evidence)
            if m_total:
                n_streams = int(m_total.group(1))
        streams = set()
        for t in transitions:
            streams.add((t["collector"], t["peer_ip"], t["prefix"]))
        prefixes = len({p for _, _, p in streams}) if streams else n_streams

        # Session roles from the audit: direct/indirect per plane.
        session_roles = {}
        for (fam, col, peer) in sessions:
            if col != coll:
                continue
            s = sessions[(fam, col, peer)]
            re_direct = s["peer_asn"] in (11537,)
            re_indirect = any(pid == "internet2-re" and n > 0 for pid, n in s["path_class"]["per_plane_contains"])
            i2px_direct = s["peer_asn"] in (11164,)
            i2px_indirect = any(pid == "internet2-i2px" and n > 0 for pid, n in s["path_class"]["per_plane_contains"])
            session_roles[peer] = {
                "peer_asn": s["peer_asn"],
                "direct_re": re_direct,
                "indirect_re": re_indirect and not re_direct,
                "direct_i2px": i2px_direct,
                "indirect_i2px": i2px_indirect and not i2px_direct,
                "origin_routes": s["origin_route_count"],
            }

        # Transition classification per the R&E cohort.
        withdrawals = [t for t in transitions if t["withdrawn"]]
        replacements = [t for t in transitions if t["kind"] == "PathReplacement"]
        returns = [t for t in transitions if t["kind"] == "ReturnToBaseline"]
        # Stream-level (peer, prefix) sets: two peers announcing the same
        # prefix are two streams.
        absent_streams = {(t["peer_ip"], t["prefix"]) for t in withdrawals}
        restored = {(t["peer_ip"], t["prefix"]) for t in returns if (t["peer_ip"], t["prefix"]) in absent_streams}
        departed_streams = {(t["peer_ip"], t["prefix"]) for t in replacements}
        returned_streams = {(t["peer_ip"], t["prefix"]) for t in returns}

        times = sorted({t["occurred_utc"] for t in transitions})
        evidence_interval = (times[0], times[-1]) if times else (None, None)

        # Peer composition of the cohort: transition peers plus the
        # audit's R&E-plane sessions (unchanged cohorts have no
        # transitions and their peers come from the historical audit).
        cohort_peers = sorted({p for _, p, _ in streams})
        for (fam, col, peer) in sessions:
            if col == coll and peer not in cohort_peers:
                s = sessions[(fam, col, peer)]
                re_related = s["peer_asn"] in (11537,) or any(
                    pid == "internet2-re" and n > 0
                    for pid, n in s["path_class"]["per_plane_contains"]
                )
                if re_related:
                    cohort_peers.append(peer)
        cohort_peers.sort()
        peer_lines = []
        for p in cohort_peers:
            role = session_roles.get(p, {})
            rel = []
            if role.get("direct_re"):
                rel.append("direct R&E (peer ASN 11537)")
            if role.get("indirect_re"):
                rel.append("indirect R&E (path contains AS11537)")
            if role.get("direct_i2px"):
                rel.append("direct I2PX (peer ASN 11164)")
            if role.get("indirect_i2px"):
                rel.append("indirect I2PX (path contains AS11164)")
            if not rel:
                rel.append("other observed path")
            peer_lines.append({"peer_ip": p, "peer_asn": role.get("peer_asn"), "relationship": rel})

        matrix_rows.append(OrderedDict([
            ("collector", coll),
            ("collector_location", locations[(family, coll)]),
            ("source_family", family),
            ("cohort_predicate", "ContainsAny[11537] (R&E plane)"),
            ("baseline_streams", n_streams),
            ("baseline_prefixes", prefixes),
            ("peer_sessions", peer_lines),
            ("temporary_stream_absences", len(absent_streams)),
            ("path_replacements", len(replacements)),
            ("re_plane_departures", len(departed_streams)),
            ("re_plane_returns", len(returned_streams)),
            ("pex_plane_departures", 0),
            ("pex_plane_returns", 0),
            ("other_path_transitions", len([t for t in transitions if t["kind"] not in ("PathReplacement", "ReturnToBaseline", "Withdraw")])),
            ("restoration", f"{len(restored)} of {len(absent_streams)} absent streams restored"),
            ("evidence_interval_utc", f"{evidence_interval[0]} .. {evidence_interval[1]}"),
            ("verdict", report["result"]["verdict"]),
            ("finding", report["result"]["finding"]),
        ]))

    # I2PX plane: zero baseline at every observer (preflight records).
    pex_rows = []
    for coll in collectors:
        slug = f"I2PX-{'RV2' if coll == 'route-views2' else coll.upper()}"
        pre = load(os.path.join(OUT_DIR, f"MANLAN-2019-NORDUNET-PILOT-{slug}", "preflight.json"))
        pex_rows.append(OrderedDict([
            ("collector", coll),
            ("cohort_predicate", "ContainsAny[11164] (I2PX plane)"),
            ("qualifying_frozen_streams", pre["qualifying_frozen_streams"]),
            ("outcome", "no I2PX-plane baseline at this observer (no AS11164 session or AS11164-in-path route in the 2019-08-21 baseline); absence of a baseline is NOT evidence of no I2PX-plane event change"),
        ]))

    profile = load(os.path.join(ROOT, "network-profile.json"))
    re_label = next(p["display_label"] for p in profile["service_planes"] if 11537 in p["asns"])
    pex_label = next(p["display_label"] for p in profile["service_planes"] if 11164 in p["asns"])
    matrix = OrderedDict([
        ("schema_version", 1),
        ("generated_utc", "2026-08-02T00:00:00Z"),
        ("reviewed_target", "NORDUnet (AS2603)"),
        ("re_plane_label", re_label),
        ("pex_plane_label", pex_label),
        ("window_utc", "2019-08-21 16:00:00Z .. 17:30:00Z"),
        ("note", "Each row is an independent AnalysisRun; observations are never merged into one verdict. Direct (peer ASN equals the plane ASN) and indirect (path contains the plane ASN) are distinct evidence classes. Collector location describes where the collector is hosted, not the path taken by observed routes."),
        ("re_plane_runs", matrix_rows),
        ("pex_plane_preflights", pex_rows),
    ])
    with open(os.path.join(ROOT, "cross-observer-matrix.json"), "w") as f:
        json.dump(matrix, f, indent=1)

    # Markdown rendering
    L = []
    L.append("# Cross-observer matrix — NORDUnet pilot (Session 35, Part 6)")
    L.append("")
    L.append(f"**Reviewed target:** NORDUnet (AS2603) · **window:** 2019-08-21 16:00:00Z – 17:30:00Z")
    L.append("")
    L.append("Each row is an **independent AnalysisRun**; observations are never merged into one")
    L.append("verdict. **Direct** (peer ASN equals the plane ASN) and **indirect** (path contains")
    L.append("the plane ASN) are distinct evidence classes. Collector location describes where the")
    L.append("collector is hosted, not the path taken by observed routes.")
    L.append("")
    L.append("## R&E-plane runs (cohort selector `ContainsAny[11537]`)")
    L.append("")
    L.append("| collector | location | peer sessions | streams | prefixes | absences | replacements | R&E departures | R&E returns | restoration | verdict |")
    L.append("|---|---|---|---:|---:|---:|---:|---:|---:|---|---|")
    for r in matrix_rows:
        peers = "; ".join(f"{p['peer_ip']} (AS{p['peer_asn']}, {', '.join(p['relationship'])})" for p in r["peer_sessions"])
        L.append(f"| {r['collector']} | {r['collector_location']} | {peers} | {r['baseline_streams']} | {r['baseline_prefixes']} | {r['temporary_stream_absences']} | {r['path_replacements']} | {r['re_plane_departures']} | {r['re_plane_returns']} | {r['restoration']} | {r['verdict']} |")
    L.append("")
    L.append("### Evidence intervals")
    L.append("")
    for r in matrix_rows:
        L.append(f"- **{r['collector']}**: {r['evidence_interval_utc']}")
    L.append("")
    L.append("## I2PX-plane preflights (cohort selector `ContainsAny[11164]`)")
    L.append("")
    L.append("| collector | qualifying frozen streams | outcome |")
    L.append("|---|---:|---|")
    for r in pex_rows:
        L.append(f"| {r['collector']} | {r['qualifying_frozen_streams']} | {r['outcome']} |")
    L.append("")
    with open(os.path.join(ROOT, "cross-observer-matrix.md"), "w") as f:
        f.write("\n".join(L) + "\n")
    print(f"matrix: {len(matrix_rows)} R&E rows + {len(pex_rows)} peer-exchange preflights")

if __name__ == "__main__":
    main()
