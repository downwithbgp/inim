#!/usr/bin/env bash
# Session 36 — deterministic workbench screenshot harness.
#
# Captures the NOC incident workbench pages at three viewports into
# tmp/ui-review/session-36/. Uses the same Playwright chromium as
# scripts/screenshot-review.sh. The implementation agent must NOT
# self-certify visual quality: the images are produced here for EXTERNAL
# review. Report only: screenshots generated, viewport, route, file path.
#
# Requirements: a Playwright chromium build in ~/.cache/ms-playwright,
# and a built release binary at target/release/inim.
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DB="${UI_REVIEW_DB:-data/inim.sqlite}"
OUT=tmp/ui-review/session-36
PORT="${UI_REVIEW_PORT:-8188}"
BIND="127.0.0.1:$PORT"
mkdir -p "$OUT"

if [ ! -d "$HOME/.cache/ms-playwright" ]; then
  echo "browser unavailable: no Playwright chromium build in ~/.cache/ms-playwright" >&2
  echo "install with: npx playwright install chromium" >&2
  exit 3
fi

if [ ! -f "$DB" ]; then
  echo "deterministic demo catalog not found at $DB" >&2
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

# Pages per the brief (Part 15). Expanded rows and drill-down are
# server-rendered via ?expand=1 (deterministic; no JS interaction).
declare -a PAGES=(
  "ripe-no-change-workbench:/events/INC0302574/workbench"
  "uva-partial-impact-workbench:/events/INC0299001/workbench"
  "manlan-multi-observer-workbench:/case-studies/manlan-2019/workbench"
  "observer-episode-expanded:/case-studies/manlan-2019/workbench?expand=1"
  "prefix-drilldown:/case-studies/manlan-2019/workbench?expand=1"
  "timeline:/case-studies/manlan-2019/workbench"
)
declare -a VIEWPORTS=("1440,900" "1280,800" "390,844")

FAILED=0
for entry in "${PAGES[@]}"; do
  NAME="${entry%%:*}"
  PATH_="${entry#*:}"
  for VP in "${VIEWPORTS[@]}"; do
    OUT_FILE="$OUT/${NAME}-${VP}.png"
    if ! npx --no-install playwright screenshot \
      --viewport-size="$VP" --full-page \
      "http://$BIND${PATH_}" "$OUT_FILE" >"$OUT/playwright.log" 2>&1; then
      echo "FAILED: $NAME @ $VP" >&2
      FAILED=1
    else
      echo "captured: $OUT_FILE"
    fi
  done
done

if [ "$FAILED" -ne 0 ]; then
  echo "screenshot harness finished with failures (see $OUT/playwright.log)" >&2
  exit 1
fi
echo "session-36 screenshot review set written to $OUT/"
ls "$OUT"/*.png | wc -l
