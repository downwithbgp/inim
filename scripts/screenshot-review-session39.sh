#!/usr/bin/env bash
# Session 39 visual-review harness: operator-first workbench viewports.
#
# Captures true-viewport screenshots of the reworked workbench pages:
#   MAN LAN 1440x900, UVA 1280x800, INC0302574 1280x800,
#   MAN LAN finding expanded 1440x900, MAN LAN prefix/path drill-down
#   1440x900, MAN LAN mobile 390x844.
# Images are written to tmp/ui-review/ (gitignored) for EXTERNAL review —
# the implementation agent does not self-certify visual quality.
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DB="${UI_REVIEW_DB:-data/inim.sqlite}"
OUT=tmp/ui-review/session39
PORT="${UI_REVIEW_PORT:-8191}"
BIND="127.0.0.1:$PORT"
mkdir -p "$OUT"

if [ ! -d "$HOME/.cache/ms-playwright" ]; then
  echo "browser unavailable: no Playwright chromium build in ~/.cache/ms-playwright" >&2
  exit 3
fi
if [ ! -f "$DB" ]; then
  echo "demo catalog not found at $DB" >&2
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

declare -a PAGES=(
  "manlan-workbench-1440x900:/case-studies/manlan-2019/workbench:1440,900"
  "uva-workbench-1280x800:/events/INC0299001/workbench:1280,800"
  "inc0302574-workbench-1280x800:/events/INC0302574/workbench:1280,800"
  "manlan-finding-expanded-1440x900:/case-studies/manlan-2019/workbench?expand=1:1440,900"
  "manlan-prefix-drilldown-1440x900:/case-studies/manlan-2019/workbench?prefixes=0:1440,900"
  "manlan-mobile-390x844:/case-studies/manlan-2019/workbench:390,844"
)

FAILED=0
for entry in "${PAGES[@]}"; do
  NAME="${entry%%:*}"
  REST="${entry#*:}"
  PATH_="${REST%%:*}"
  VP="${REST##*:}"
  OUT_FILE="$OUT/${NAME}.png"
  if ! npx --no-install playwright screenshot \
    --viewport-size="$VP" \
    "http://$BIND${PATH_}" "$OUT_FILE" >"$OUT/playwright.log" 2>&1; then
    echo "FAILED: $NAME @ $VP" >&2
    FAILED=1
  else
    echo "captured: $OUT_FILE"
  fi
done

if [ "$FAILED" -ne 0 ]; then
  echo "session39 screenshot harness finished with failures (see $OUT/playwright.log)" >&2
  exit 1
fi
echo "session39 screenshots written to $OUT/"
