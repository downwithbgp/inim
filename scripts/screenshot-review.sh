#!/usr/bin/env bash
# Deterministic visual-review harness (Session 32, Part 12).
#
# Uses the deterministic demo catalog (data/inim.sqlite), starts inim on
# loopback, captures fixed-viewport screenshots with an already-installed
# headless Chromium (via Playwright), shuts the server down, and writes the
# images to tmp/ui-review/ (gitignored, excluded from the crate package).
#
# The implementation agent must not self-certify visual quality: the images
# are produced here for EXTERNAL computer-vision review.
#
# Requirements: a Playwright chromium build in ~/.cache/ms-playwright.
# Fails with a clear "browser unavailable" message otherwise.
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DB="${UI_REVIEW_DB:-data/inim.sqlite}"
OUT=tmp/ui-review
PORT="${UI_REVIEW_PORT:-8187}"
BIND="127.0.0.1:$PORT"
mkdir -p "$OUT"

# ── Browser availability ─────────────────────────────────────────────
if [ ! -d "$HOME/.cache/ms-playwright" ]; then
  echo "browser unavailable: no Playwright chromium build in ~/.cache/ms-playwright" >&2
  echo "install with: npx playwright install chromium" >&2
  exit 3
fi

# ── Deterministic demo catalog ───────────────────────────────────────
if [ ! -f "$DB" ]; then
  echo "deterministic demo catalog not found at $DB" >&2
  echo "build it with: inim catalog init --db $DB && inim catalog import --db $DB --root ." >&2
  exit 3
fi

# ── Server lifecycle (loopback only, cleanup on failure) ─────────────
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

# Resolve run ids from the catalog (deterministic, never hard-coded).
RUN_UVA=$(sqlite3 "$DB" "SELECT r.id FROM analysis_runs r JOIN analysis_plans p ON p.id=r.plan_id JOIN manifest_revisions m ON m.id=p.manifest_revision_id JOIN catalog_events e ON e.id=m.event_id WHERE e.external_id='INC0299001' LIMIT 1" 2>/dev/null || python3 -c "
import sqlite3
c = sqlite3.connect('$DB')
row = c.execute(\"SELECT r.id FROM analysis_runs r JOIN analysis_plans p ON p.id=r.plan_id JOIN manifest_revisions m ON m.id=p.manifest_revision_id JOIN catalog_events e ON e.id=m.event_id WHERE e.external_id='INC0299001' LIMIT 1\").fetchone()
print(row[0] if row else 1)")
RUN_RIPE=$(python3 -c "
import sqlite3
c = sqlite3.connect('$DB')
row = c.execute(\"SELECT r.id FROM analysis_runs r JOIN analysis_plans p ON p.id=r.plan_id JOIN manifest_revisions m ON m.id=p.manifest_revision_id JOIN catalog_events e ON e.id=m.event_id WHERE e.external_id='INC0302574' LIMIT 1\").fetchone()
print(row[0] if row else 2)")

# ── Screenshots ──────────────────────────────────────────────────────
declare -a PAGES=(
  "dashboard:/"
  "events:/events"
  "ripe:/events/INC0302574"
  "uva-analysis:/analyses/$RUN_UVA"
  "blocked:/events/INC0301970"
  "manlan:/case-studies/manlan-2019"
  "streams:/analyses/$RUN_UVA/streams"
  "corpus:/corpus"
  "corpus-sync-runs:/corpus/sync-runs"
  "relationships:/events/CHG0038258/relationships"
  "analysis-queue:/analysis-queue"
  "incident-candidates:/incident-candidates"
  "archive-batches:/archive-batches"
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

# ── Explicit NORDUnet pilot section capture (full page includes it) ──
if ! npx --no-install playwright screenshot \
  --viewport-size="1440,900" --full-page \
  "http://$BIND/case-studies/manlan-2019" "$OUT/manlan-pilot-1440x900.png" >"$OUT/playwright.log" 2>&1; then
  echo "FAILED: manlan-pilot @ 1440x900" >&2
  FAILED=1
fi

if [ "$FAILED" -ne 0 ]; then
  echo "screenshot harness finished with failures (see $OUT/playwright.log)" >&2
  exit 1
fi
echo "screenshot review set written to $OUT/"
ls "$OUT"/*.png | wc -l
