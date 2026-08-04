#!/bin/bash
# RIPE RIS collector RIB preflight for the NORDUnet pilot (2026-08).
#
# Usage: scripts/ris_collector_preflight.sh
# For each historically available candidate collector: download the
# 2019-08-21 00:00 bview (8-hour grid, pre-warmup baseline) and run the
# reviewed RIB preflight (origin AS2603 + ContainsAny[11537]).
# Runs up to 4 collectors in parallel (parsing is single-threaded).
# Output: tmp/ris-preflight/preflight-<rrc>.json per collector.
set -euo pipefail
cd "$(dirname "$0")/.."

EVENT=case-studies/manlan-2019/pilot/pilot-event.json
BASE=case-studies/manlan-2019/pilot/manifests/MANLAN-2019-NORDUNET-PILOT.json
PROBE_DIR=tmp/ris-preflight
CACHE=cache/ris-preflight
mkdir -p "$PROBE_DIR"

# Priority order: NORDUnet-relevant first (Nordic/EU peer diversity),
# archive giants later. rrc22 is EXCLUDED: its 2019-08 bview is a 3.9KB
# stub with no usable baseline RIB (recorded as rejected on metadata in
# the selection report).
COLLECTORS=(rrc07 rrc01 rrc03 rrc04 rrc05 rrc06 rrc11 rrc12 rrc13 rrc14 rrc16 rrc20 rrc21 rrc23 rrc24 rrc00 rrc10 rrc15)

run_one() {
  local c="$1"
  local MAN="$PROBE_DIR/probe-$c.json"
  python3 - "$BASE" "$MAN" "$c" <<'PYEOF'
import json, sys
base, out, collector = sys.argv[1], sys.argv[2], sys.argv[3]
m = json.load(open(base))
# Keep the reviewed pilot event/window/target; swap only the collector
# and family. Preflight runs are probes, not AnalysisRuns.
m["collectors"] = [collector]
m["source_family"] = "RipeRis"
m["analyst_notes"] = m.get("analyst_notes", []) + ["RIS collector selection preflight (2026-08); not a pilot run."]
json.dump(m, open(out, "w"), indent=2)
PYEOF
  echo "== $c preflight =="
  if ./target/debug/inim analyze --event "$EVENT" --manifest "$MAN" \
      --cache "$CACHE" --out "$PROBE_DIR/out-$c" --preflight-only \
      --download-jobs 2 --parse-jobs 2 > "$PROBE_DIR/preflight-$c.json" 2> "$PROBE_DIR/preflight-$c.log"; then
    echo "== $c OK =="
  else
    echo "== $c FAILED =="
    tail -3 "$PROBE_DIR/preflight-$c.log" || true
  fi
}
export -f run_one
export EVENT BASE PROBE_DIR CACHE

printf '%s\n' "${COLLECTORS[@]}" | xargs -P 4 -I{} bash -c 'run_one "$@"' _ {}

echo "preflight runs complete"
