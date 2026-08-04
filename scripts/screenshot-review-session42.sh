#!/usr/bin/env bash
# Compact-findings visual-review harness (2026-08).
#
# Usage: scripts/screenshot-review-session42.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
DB="${UI_REVIEW_DB:-data/inim.sqlite}"
OUT=tmp/ui-review/session42
PORT="${UI_REVIEW_PORT:-8216}"
BIND="127.0.0.1:$PORT"
mkdir -p "$OUT"
if [ ! -d "$HOME/.cache/ms-playwright" ]; then echo "browser unavailable" >&2; exit 3; fi
"$ROOT/target/release/inim" serve --db "$DB" --root "$ROOT" --bind "$BIND" >"$OUT/serve.log" 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true' EXIT
for _ in $(seq 1 30); do curl -sS -o /dev/null "http://$BIND/" && break; sleep 0.3; done
declare -a PAGES=(
  "manlan-compact-1440x900:/case-studies/manlan-2019/workbench:1440,900"
  "manlan-route-sequence-expanded-1440x900:/case-studies/manlan-2019/workbench?expand=1:1440,900"
  "manlan-mobile-390x844:/case-studies/manlan-2019/workbench:390,844"
  "uva-compact-1280x800:/events/INC0299001/workbench:1280,800"
  "inc0302574-concise-1280x800:/events/INC0302574/workbench:1280,800"
)
FAILED=0
for entry in "${PAGES[@]}"; do
  NAME="${entry%%:*}"; REST="${entry#*:}"; PATH_="${REST%%:*}"; VP="${REST##*:}"
  if ! npx --no-install playwright screenshot --viewport-size="$VP" "http://$BIND${PATH_}" "$OUT/${NAME}.png" >"$OUT/playwright.log" 2>&1; then
    echo "FAILED: $NAME @ $VP" >&2; FAILED=1
  else
    echo "captured: $OUT/${NAME}.png"
  fi
done
[ "$FAILED" -ne 0 ] && { echo "harness failures (see $OUT/playwright.log)" >&2; exit 1; }
echo "session42 screenshots written to $OUT/"
