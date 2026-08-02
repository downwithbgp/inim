#!/usr/bin/env bash
# Session 37 — validated workbench screenshot harness.
#
# Captures the redesigned NOC workbench at explicit viewports into
# tmp/ui-review/session-37/. For each capture this harness:
#   1. sets the requested viewport explicitly (playwright context)
#   2. verifies the document content marker (fail when absent)
#   3. performs the required navigation (server-rendered query state)
#   4. waits for the target state marker (verified after page load)
#   5. captures a full-page PNG
#   6. asserts the PNG width equals the requested viewport width
#   7. records the SHA-256
#   8. asserts distinct states have distinct hashes
#
# The harness FAILS (non-zero exit) when: a marker is absent, two
# supposedly distinct states have identical hashes, the PNG width does
# not match the viewport width, the page returns an error, or an
# expansion did not occur.
#
# Requirements: the playwright node module (resolved from the repo
# root) and a built release binary at target/release/inim.
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DB="${UI_REVIEW_DB:-data/inim.sqlite}"
OUT=tmp/ui-review/session-38
PORT="${UI_REVIEW_PORT:-8189}"
BIND="127.0.0.1:$PORT"
mkdir -p "$OUT"

if [ ! -f "$DB" ]; then
  echo "catalog database not found at $DB" >&2
  exit 3
fi
if [ ! -x "$ROOT/target/release/inim" ]; then
  echo "release binary not found at target/release/inim (run: cargo build --release)" >&2
  exit 3
fi
if ! node -e "require.resolve('playwright')" >/dev/null 2>&1; then
  echo "playwright module not resolvable from $ROOT" >&2
  exit 3
fi

"$ROOT/target/release/inim" serve --db "$DB" --root "$ROOT" --bind "$BIND" \
  >"$OUT/serve.log" 2>&1 &
SERVER_PID=$!
cleanup() {
  kill "$SERVER_PID" 2>/dev/null || true
  wait "$SERVER_PID" 2>/dev/null || true
}
trap cleanup EXIT

for _ in $(seq 1 30); do
  if curl -sS -o /dev/null "http://$BIND/"; then break; fi
  sleep 0.3
done

# name|url|width|height|marker|fullpage
# Session 38 captures: corrected units, timeline context strip, I2PX
# assessment, mobile first view; true viewport-only (non-full-page)
# first-screen captures at all three viewports.
cat >"$OUT/captures.txt" <<EOF
manlan-first|http://$BIND/case-studies/manlan-2019/workbench|1440|900|Route-state changes appeared at|full
manlan-first-viewport|http://$BIND/case-studies/manlan-2019/workbench|1440|900|Route-state changes appeared at|viewport
manlan-timeline-context|http://$BIND/case-studies/manlan-2019/workbench?view=timeline|1440|900|tl-context|full
manlan-cooldown|http://$BIND/case-studies/manlan-2019/workbench|1280|800|Still changing at 17:52:16 UTC|full
manlan-changed-table|http://$BIND/case-studies/manlan-2019/workbench?changed=1|1440|900|wb-episode-row wb-changed|full
manlan-expanded-absence|http://$BIND/case-studies/manlan-2019/workbench?episode=3|1440|900|<details class="wb-episode-details" open|full
manlan-prefix-drilldown|http://$BIND/case-studies/manlan-2019/workbench?prefixes=3|1440|900|<details class="wb-prefix-drilldown" open|full
ripe-i2px-assessment|http://$BIND/events/INC0302574/workbench|1280|800|Insufficient public-collector visibility for the named I2PX relationship|full
ripe-viewport|http://$BIND/events/INC0302574/workbench|1280|800|Insufficient public-collector visibility for the named I2PX relationship|viewport
uva-corrected-breadth|http://$BIND/events/INC0299001/workbench|1280|800|4 of 4 eligible observer sessions|full
uva-viewport|http://$BIND/events/INC0299001/workbench|1280|800|Route-state changes at 4 of 4 eligible observer sessions|viewport
manlan-mobile-first|http://$BIND/case-studies/manlan-2019/workbench|390|844|Route-state changes appeared at|full
manlan-mobile-viewport|http://$BIND/case-studies/manlan-2019/workbench|390|844|Route-state changes appeared at|viewport
EOF

node "$ROOT/scripts/screenshot-session37-capture.js" "$OUT" <"$OUT/captures.txt" \
  >"$OUT/capture-ok.txt" 2>"$OUT/capture-err.txt"
DRIVER_RC=$?
cat "$OUT/capture-err.txt" >&2
if [ "$DRIVER_RC" -ne 0 ]; then
  echo "screenshot driver finished with failures" >&2
  exit 1
fi

FAILED=0
declare -A HASHES

png_width() {
  python3 - "$1" <<'EOF'
import struct, sys
with open(sys.argv[1], 'rb') as f:
    head = f.read(24)
    if head[:8] != b'\x89PNG\r\n\x1a\n':
        sys.exit(1)
    w, h = struct.unpack('>II', head[16:24])
    print(w, h)
EOF
}

while IFS='|' read -r NAME OUT_FILE REQ_W REQ_H; do
  [ -z "$NAME" ] && continue
  DIMS=$(png_width "$OUT_FILE") || {
    echo "FAILED: $NAME — not a valid PNG" >&2
    FAILED=1
    continue
  }
  WIDTH=${DIMS%% *}
  if [ "$WIDTH" != "$REQ_W" ]; then
    echo "FAILED: $NAME — PNG width $WIDTH != viewport width $REQ_W" >&2
    FAILED=1
    continue
  fi
  HASH=$(sha256sum "$OUT_FILE" | cut -d' ' -f1)
  HASHES["$NAME"]="$HASH"
  echo "captured: $OUT_FILE ($DIMS) sha256=$HASH"
done <"$OUT/capture-ok.txt"

# Distinct states must have distinct hashes.
for a in "${!HASHES[@]}"; do
  for b in "${!HASHES[@]}"; do
    if [ "$a" != "$b" ] && [ "${HASHES[$a]}" = "${HASHES[$b]}" ]; then
      echo "FAILED: states '$a' and '$b' have identical SHA-256 (${HASHES[$a]})" >&2
      FAILED=1
    fi
  done
done

if [ "$FAILED" -ne 0 ]; then
  echo "session-38 screenshot harness FAILED (see $OUT/)" >&2
  exit 1
fi
echo "session-38 screenshot set written to $OUT/ (${#HASHES[@]} captures, all distinct)"
