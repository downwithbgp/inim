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
# Canonical hash: report.json is hashed with the volatile generated_at
# timestamp blanked; performance.json is excluded entirely.
HASHES=""
for N in 1 "$DEFAULT_JOBS" 24; do
  D=tmp/rerun/jobs$N
  H=$(python3 - "$D" <<'PYEOF2'
import json, re, sys, hashlib, pathlib
d = sys.argv[1]
h = hashlib.sha256()
for f in ["report.json","report.txt","transitions.json","lifecycle.json",
          "semantic_waves.json","withdrawal_audit.json",
          "evidence_appendix.jsonl","archive_manifest.json"]:
    p = pathlib.Path(d) / f
    if not p.exists(): continue
    b = p.read_bytes()
    if f == "report.json":
        b = re.sub(r'"generated_at": "[^"]+"', '"generated_at": ""', b.decode()).encode()
    h.update(b)
print(h.hexdigest())
PYEOF2
)
  echo "jobs=$N substantive_hash=$H"
  HASHES="$HASHES $H"
done
echo "hashes:$HASHES"
