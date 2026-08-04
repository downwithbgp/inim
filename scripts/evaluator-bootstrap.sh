#!/bin/sh
# evaluator-bootstrap.sh — one supported bootstrap path for the NOC
# alpha evaluation demo (read-only, loopback, offline after build).
#
# POSIX shell (works with macOS system shell and Linux /bin/sh). No
# bashisms, no arrays, no GNU-only flags. Requires: git (for cloning,
# not used here), Rust/Cargo (cargo build needs network on a fresh
# machine to fetch dependencies), and standard POSIX tools (grep, sed,
# printf, command).
#
# Usage:
#   scripts/evaluator-bootstrap.sh [--db PATH] [--port N] [--bind HOST]
#                                  [--root DIR] [--force] [--start]
#
# Defaults:
#   --db    ./inim-demo.sqlite
#   --port  8080
#   --bind  127.0.0.1        (loopback only; never binds non-loopback
#                             without explicit --bind)
#   --root  repository root (auto-detected from the script location)
#
# Behavior:
#   1. prerequisite check (cargo, rustc)
#   2. release build (cargo build --release --locked)
#   3. deterministic demo initialization (refuses to overwrite an
#      existing database unless --force)
#   4. demo verification + project-scope audit
#   5. prints the exact read-only server command and expected URLs
#   6. with --start: execs the read-only server (never backgrounds it)
#
# The server is ALWAYS read-only (inim serve defaults to read-only;
# no --enable-writes is ever passed). No worker is started. No live
# BGP source is contacted by demo init/verify/serve.

set -u

# Resolve the repository root from the script location.
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

DB="$ROOT/inim-demo.sqlite"
PORT=8080
BIND=127.0.0.1
FORCE=0
START=0

usage() {
    printf '%s\n' \
        "usage: $0 [--db PATH] [--port N] [--bind HOST] [--root DIR] [--force] [--start]" \
        "" \
        "  --db PATH   demo database path (default: ./inim-demo.sqlite)" \
        "  --port N    read-only server port (default: 8080)" \
        "  --bind HOST loopback bind host (default: 127.0.0.1)" \
        "  --root DIR  repository root (default: auto-detected)" \
        "  --force     replace an existing demo database" \
        "  --start     start the read-only server after verification" \
        ""
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --db) DB=${2:-}; shift 2 ;;
        --port) PORT=${2:-}; shift 2 ;;
        --bind) BIND=${2:-}; shift 2 ;;
        --root) ROOT=${2:-}; shift 2 ;;
        --force) FORCE=1; shift ;;
        --start) START=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *)
            printf 'error: unknown argument: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [ -z "$DB" ] || [ -z "$PORT" ] || [ -z "$BIND" ]; then
    printf 'error: --db, --port, and --bind require values\n' >&2
    usage >&2
    exit 2
fi

# Absolute-ize the database path so messages are unambiguous.
case "$DB" in
    /*) DB_ABS=$DB ;;
    *) DB_ABS=$(CDPATH= cd -- "$(dirname -- "$DB")" && pwd)/$(basename -- "$DB") ;;
esac
if [ -d "$DB_ABS" ]; then
    printf 'error: --db path is a directory, not a database file: %s\n' "$DB_ABS" >&2
    exit 2
fi

echo "== inim NOC alpha evaluation bootstrap =="
echo "project root: $ROOT"
echo "demo database: $DB_ABS"

# ── 1. Prerequisites ────────────────────────────────────────────────
if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found. Install the Rust toolchain (https://rustup.rs),"
    echo "then run this script again." >&2
    exit 1
fi
if ! command -v rustc >/dev/null 2>&1; then
    echo "error: rustc not found. Install the Rust toolchain (https://rustup.rs),"
    echo "then run this script again." >&2
    exit 1
fi
RUSTC_VERSION=$(rustc --version 2>/dev/null || echo "unknown")
CARGO_VERSION=$(cargo --version 2>/dev/null || echo "unknown")
echo "rustc: $RUSTC_VERSION"
echo "cargo: $CARGO_VERSION"
echo "note: cargo may need network access on a fresh machine to fetch Rust"
echo "      dependencies. Demo initialization and serving never contact live"
echo "      BGP sources (RouteViews, RIPE RIS, GRNOC, PeeringDB, RIR)."

# ── 2. Release build ────────────────────────────────────────────────
echo "== building release binary (cargo build --release --locked) =="
( cd "$ROOT" && cargo build --release --locked ) || {
    echo "error: release build failed. Fix the build error above and re-run." >&2
    exit 1
}
BIN="$ROOT/target/release/inim"
if [ ! -x "$BIN" ]; then
    echo "error: built binary not found at $BIN" >&2
    exit 1
fi

# ── 3. Demo initialization (refuses overwrite without --force) ──────
echo "== deterministic demo initialization =="
if [ -e "$DB_ABS" ]; then
    if [ "$FORCE" -eq 0 ]; then
        echo "error: database already exists at $DB_ABS." >&2
        echo "       Re-run with --force to replace it, or choose a new --db path." >&2
        exit 1
    fi
    FORCE_ARG="--force"
else
    FORCE_ARG=""
fi
# shellcheck disable=SC2086
"$BIN" demo init --db "$DB_ABS" --root "$ROOT" $FORCE_ARG || {
    echo "error: demo init failed (see output above)." >&2
    exit 1
}

# ── 4. Demo verification + project scope ────────────────────────────
echo "== demo verification =="
"$BIN" demo verify --db "$DB_ABS" --root "$ROOT" || {
    echo "error: demo verify failed (see output above)." >&2
    exit 1
}
echo "== project-scope audit =="
"$BIN" project-scope audit --db "$DB_ABS" --root "$ROOT" || {
    echo "error: project-scope audit failed (see output above)." >&2
    exit 1
}

# ── 5. Expected URLs from the reviewed scenario manifest ────────────
SERVER_CMD="\"$BIN\" serve --db \"$DB_ABS\" --root \"$ROOT\" --bind $BIND:$PORT"
echo ""
echo "== read-only demo ready =="
echo "start the read-only server (loopback only) with:"
echo "    $SERVER_CMD"
echo ""
echo "expected URLs (scenario manifest evaluation/scenarios.toml):"
if [ -f "$ROOT/evaluation/scenarios.toml" ]; then
    PATHS=$(grep '^path = ' "$ROOT/evaluation/scenarios.toml" | sed 's/^path = "\(.*\)"/\1/')
    for p in $PATHS; do
        echo "    http://$BIND:$PORT$p"
    done
else
    echo "    (evaluation/scenarios.toml not found; serving the catalog at /)"
fi
echo ""
echo "no worker is started; no write mode is enabled; no live BGP source"
echo "is contacted by the demo or the server."

# ── 6. Optional explicit server start (exec; never backgrounded) ────
if [ "$START" -eq 1 ]; then
    echo "== starting read-only server =="
    exec "$BIN" serve --db "$DB_ABS" --root "$ROOT" --bind "$BIND:$PORT"
fi
exit 0
