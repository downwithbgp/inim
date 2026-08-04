#!/usr/bin/env bash
# Local raw-cache parse scaling benchmark.
# All archives are already in the local raw cache; --rebuild-derived-cache
# forces re-parse. No network acquisition.
#
# Usage: scripts/bench_parse_scaling.sh
set -u
EVENT=case-studies/manlan-2019/pilot/pilot-event.json
MANIFEST=case-studies/manlan-2019/pilot/manifests/MANLAN-2019-NORDUNET-PILOT.json
RESULT=tmp/bench/results.csv
echo "jobs,wall_s,user_s,sys_s,cpu_pct,max_rss_kb,archives_per_s,mib_per_s,elems_per_s,obs_per_s,artifact_sha" > "$RESULT"
FIRST=1
for N in 1 2 4 8 12 16 24; do
  D=tmp/bench/jobs$N
  mkdir -p "$D"
  REBUILD_FLAG="--rebuild-derived-cache"
  if [ "$FIRST" -eq 0 ]; then REBUILD_FLAG="--rebuild-update-caches"; fi
  FIRST=0
  /usr/bin/time -v ./target/release/inim analyze \
    --event "$EVENT" --manifest "$MANIFEST" --cache cache --out "$D" \
    --jobs 1 --parse-jobs "$N" --download-jobs 1 $REBUILD_FLAG \
    > "$D/stdout.json" 2> "$D/time.log"
  WALL=$(grep "Elapsed (wall clock)" "$D/time.log" | grep -oE "[0-9.]+" | head -1 || true)
  USER=$(grep "User time" "$D/time.log" | grep -oE "[0-9.]+" | head -1 || true)
  SYS=$(grep "System time" "$D/time.log" | grep -oE "[0-9.]+" | head -1 || true)
  CPU=$(grep "Percent of CPU" "$D/time.log" | grep -oE "[0-9]+" | head -1 || true)
  RSS=$(grep "Maximum resident set size" "$D/time.log" | grep -oE "[0-9]+" | head -1 || true)
  ARCH=$(python3 -c "import json;d=json.load(open('$D/performance.json'));print(len(d['archives']))")
  PARSED=$(python3 -c "import json;d=json.load(open('$D/performance.json'));print(sum(a['parsed_elements'] for a in d['archives']))")
  ADM=$(python3 -c "import json;d=json.load(open('$D/performance.json'));print(sum(a['admitted_observations'] for a in d['archives']))")
  MIB=$(python3 -c "import json;d=json.load(open('$D/performance.json'));print(sum(a['compressed_bytes'] for a in d['archives'])/1048576)")
  SHA=$(cat "$D/report.json" "$D/transitions.json" "$D/lifecycle.json" "$D/semantic_waves.json" "$D/evidence_appendix.jsonl" | sha256sum | cut -d' ' -f1)
  python3 -c "
import sys
wall=float('$WALL' or 0);arch=int('$ARCH' or 0);parsed=int('$PARSED' or 0);adm=int('$ADM' or 0);mib=float('$MIB' or 0)
print(f'$N,{wall},{USER or 0},{SYS or 0},{CPU or 0},{RSS or 0},{arch/wall if wall else 0:.2f},{mib/wall if wall else 0:.2f},{parsed/wall if wall else 0:.0f},{adm/wall if wall else 0:.0f},$SHA')
" >> "$RESULT"
  echo "jobs=$N wall=${WALL}s cpu=${CPU}% rss=${RSS}kb sha=$SHA"
done
echo "BENCH_DONE"
