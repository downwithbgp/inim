#!/usr/bin/env bash
# Local-cache stage-metrics benchmark — stage metrics, jobs sweep, and
# repeated two-plane reuse measurement. Network acquisition excluded
# (all archives cached). Writes machine-readable results to tmp/bench/.
#
# Usage: scripts/session35-benchmark.sh
set -uo pipefail
cd "$(dirname "$0")/.."

EVENT=case-studies/manlan-2019/pilot/pilot-event.json
MAN_RE=case-studies/manlan-2019/pilot/manifests/MANLAN-2019-NORDUNET-PILOT-RE.json
MAN_I2PX=case-studies/manlan-2019/pilot/manifests/MANLAN-2019-NORDUNET-PILOT-I2PX.json
CACHE=cache/ris-preflight
BENCH=tmp/bench
mkdir -p "$BENCH"
echo "bench start: $(date -u +%H:%M:%S)" | tee "$BENCH/run.log"

run_timed() {
  local label="$1"; shift
  /usr/bin/time -v ./target/release/inim "$@" > "$BENCH/$label.stdout" 2> "$BENCH/$label.time"
  local wall user sys maxrss
  wall=$(grep -oE "Elapsed \(wall clock\) time.*" "$BENCH/$label.time" | sed 's/.*: //')
  user=$(grep -oE "User time.*" "$BENCH/$label.time" | sed 's/.*: //')
  sys=$(grep -oE "System time.*" "$BENCH/$label.time" | sed 's/.*: //')
  maxrss=$(grep -oE "Maximum resident set size.*" "$BENCH/$label.time" | sed 's/.*: //')
  echo "$label|wall=$wall|user=$user|sys=$sys|maxrss_kb=$maxrss" | tee -a "$BENCH/run.log"
}

# 10.3a — one large RIS bview preflight (rrc00, fresh parse). RIB parsing
# is a single stream: jobs do not parallelize one bview, so two
# representative job counts are measured (1 and 12) and the flatness is
# documented in BENCHMARK.md.
run_timed bview-rrc00-j1 analyze --event "$EVENT" --manifest "$MAN_RE" \
  --cache "$CACHE" --out "$BENCH/out-bview-j1" --preflight-only \
  --no-derived-cache --jobs 1
run_timed bview-rrc00-j12 analyze --event "$EVENT" --manifest "$MAN_RE" \
  --cache "$CACHE" --out "$BENCH/out-bview-j12" --preflight-only \
  --no-derived-cache --jobs 12

# 10.3b — rrc00 UPDATE pilot at jobs 1/4/8/12/16/24 (fresh update caches).
for j in 1 4 8 12 16 24; do
  run_timed upd-rrc00-j$j analyze --event "$EVENT" --manifest "$MAN_RE" \
    --cache "$CACHE" --out "$BENCH/out-upd-j$j" \
    --rebuild-update-caches --jobs "$j" --parse-jobs "$j"
done

# 10.3c/10.5 — repeated two-plane RIB preflight on rrc00:
#   pair A: two fresh parses (no-derived-cache both planes)
#   pair B: first plane fresh, second plane via source extraction reuse
#   pair C: repeat of pair B (both planes extraction hits)
run_timed twoplane-A1 analyze --event "$EVENT" --manifest "$MAN_RE" \
  --cache "$CACHE" --out "$BENCH/out-2p-a1" --preflight-only --no-derived-cache --jobs 4
run_timed twoplane-A2 analyze --event "$EVENT" --manifest "$MAN_I2PX" \
  --cache "$CACHE" --out "$BENCH/out-2p-a2" --preflight-only --no-derived-cache --jobs 4
run_timed twoplane-B1 analyze --event "$EVENT" --manifest "$MAN_RE" \
  --cache "$CACHE" --out "$BENCH/out-2p-b1" --preflight-only --jobs 4
run_timed twoplane-B2 analyze --event "$EVENT" --manifest "$MAN_I2PX" \
  --cache "$CACHE" --out "$BENCH/out-2p-b2" --preflight-only --jobs 4
run_timed twoplane-C1 analyze --event "$EVENT" --manifest "$MAN_RE" \
  --cache "$CACHE" --out "$BENCH/out-2p-c1" --preflight-only --jobs 4
run_timed twoplane-C2 analyze --event "$EVENT" --manifest "$MAN_I2PX" \
  --cache "$CACHE" --out "$BENCH/out-2p-c2" --preflight-only --jobs 4

echo "bench done: $(date -u +%H:%M:%S)" | tee -a "$BENCH/run.log"
