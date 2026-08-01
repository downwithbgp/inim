#!/usr/bin/env bash
# Pilot rerun equivalence (Session 32, Part 10): jobs=1 / default / 24,
# local raw caches, rebuilt UPDATE derived caches. Substantive artifacts
# must be byte-identical across job counts; only performance.json differs.
set -eu
EVENT=case-studies/manlan-2019/pilot/pilot-event.json
MANIFEST=case-studies/manlan-2019/pilot/manifests/MANLAN-2019-NORDUNET-PILOT.json
DEFAULT_JOBS="${PILOT_DEFAULT_JOBS:-8}"
SUBSTANTIVE="report.json report.txt transitions.json lifecycle.json semantic_waves.json withdrawal_audit.json evidence_appendix.jsonl archive_manifest.json"
mkdir -p tmp/rerun
for N in 1 "$DEFAULT_JOBS" 24; do
  D=tmp/rerun/jobs$N
  mkdir -p "$D"
  ./target/release/inim analyze \
    --event "$EVENT" --manifest "$MANIFEST" --cache cache --out "$D" \
    --jobs 1 --parse-jobs "$N" --download-jobs 1 --rebuild-update-caches \
    > "$D/stdout.json" 2> "$D/time.log"
done
HASHES=""
for N in 1 "$DEFAULT_JOBS" 24; do
  D=tmp/rerun/jobs$N
  H=$(cd "$D" && cat $SUBSTANTIVE | sha256sum | cut -d' ' -f1)
  echo "jobs=$N substantive_hash=$H"
  HASHES="$HASHES $H"
done
echo "hashes:$HASHES"
