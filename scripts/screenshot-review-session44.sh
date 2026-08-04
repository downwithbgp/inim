#!/usr/bin/env bash
# Visual-review harness: three genuinely distinct UVA states
# with required-marker and hash-distinct validation.
#
# Usage: scripts/screenshot-review-session44.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
DB="${UI_REVIEW_DB:-data/inim.sqlite}"
OUT=tmp/ui-review/session44
PORT="${UI_REVIEW_PORT:-8233}"
BIND="127.0.0.1:$PORT"
mkdir -p "$OUT"
if [ ! -d "$HOME/.cache/ms-playwright" ]; then echo "browser unavailable" >&2; exit 3; fi
"$ROOT/target/release/inim" serve --db "$DB" --root "$ROOT" --bind "$BIND" >"$OUT/serve.log" 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true' EXIT
for _ in $(seq 1 30); do curl -sS -o /dev/null "http://$BIND/" && break; sleep 0.3; done

PREPEND_ID="route-views2-163-253-3-14-prepending-changed-2026-07-14T07:24:47.679275035Z-11prefixes-1"
BASE="http://$BIND/events/INC0299001/workbench"
declare -a STATES=(
  "uva-compact-1280x800:$BASE:1280,800"
  "uva-prepend-expanded-1280x800:$BASE#finding-$PREPEND_ID:1280,800"
  "uva-withdrawal-route-sequence-1280x800:$BASE?expand=1:1280,800"
  "manlan-compact-1440x900:http://$BIND/case-studies/manlan-2019/workbench:1440,900"
  "inc0302574-concise-1280x800:http://$BIND/events/INC0302574/workbench:1280,800"
)
FAILED=0
declare -A HASHES
for entry in "${STATES[@]}"; do
  NAME="${entry%%:*}"; REST="${entry#*:}"; URL="${REST%:*}"; VP="${REST##*:}"
  if ! npx --no-install playwright screenshot --viewport-size="$VP" "$URL" "$OUT/${NAME}.png" >"$OUT/playwright.log" 2>&1; then
    echo "FAILED: capture $NAME @ $VP" >&2; FAILED=1; continue
  fi
  HASHES[$NAME]=$(sha256sum "$OUT/${NAME}.png" | cut -d' ' -f1)
  echo "captured: $OUT/${NAME}.png"
done

# ── Required-marker validation (HTML, not pixels) ──────────────────
check_marker() { # name url marker...
  local name="$1"; shift
  local url="$1"; shift
  local html
  html=$(curl -sS "$url")
  for marker in "$@"; do
    if ! printf '%s' "$html" | grep -qF -- "$marker"; then
      echo "FAILED: $name missing marker: $marker" >&2; FAILED=1
    fi
  done
}
check_marker "prepend"        "$BASE#finding-$PREPEND_ID" "Prepending changed" "AS225×7 → AS225×1"
check_marker "withdrawal-seq" "$BASE?expand=1"            "Pre-withdrawal route" "Absent" "First route after return"
check_marker "compact"        "$BASE"                     "withdrawn from this observer for 54 ms" "Temporarily absent"

# ── Wrong-details-element validation ────────────────────────────────
# The withdrawal state must have an OPEN route-sequence details; the
# compact and prepend states must not.
SEQ_OPEN_W=$(curl -sS "$BASE?expand=1" | grep -c '<details class="wb-route-sequence" open')
SEQ_OPEN_P=$(curl -sS "$BASE#finding-$PREPEND_ID" | grep -c '<details class="wb-route-sequence" open')
SEQ_OPEN_C=$(curl -sS "$BASE" | grep -c '<details class="wb-route-sequence" open')
[ "$SEQ_OPEN_W" -ge 1 ] || { echo "FAILED: withdrawal state has no open route sequence" >&2; FAILED=1; }
[ "$SEQ_OPEN_P" -eq 0 ] || { echo "FAILED: prepend state wrongly opens a route sequence" >&2; FAILED=1; }
[ "$SEQ_OPEN_C" -eq 0 ] || { echo "FAILED: compact state wrongly opens a route sequence" >&2; FAILED=1; }

# ── Hash-distinct validation (full SHA-256) ─────────────────────────
if [ "${HASHES[uva-compact-1280x800]:-}" = "${HASHES[uva-prepend-expanded-1280x800]:-}" ] \
   || [ "${HASHES[uva-compact-1280x800]:-}" = "${HASHES[uva-withdrawal-route-sequence-1280x800]:-}" ] \
   || [ "${HASHES[uva-prepend-expanded-1280x800]:-}" = "${HASHES[uva-withdrawal-route-sequence-1280x800]:-}" ]; then
  echo "FAILED: UVA screenshot states are not distinct" >&2; FAILED=1
fi

echo "── SHA-256 ──"
for name in uva-compact-1280x800 uva-prepend-expanded-1280x800 uva-withdrawal-route-sequence-1280x800 manlan-compact-1440x900 inc0302574-concise-1280x800; do
  echo "${HASHES[$name]:-MISSING}  $name.png"
done
[ "$FAILED" -ne 0 ] && { echo "harness validation FAILED" >&2; exit 1; }
echo "session44 screenshots + validation passed"
