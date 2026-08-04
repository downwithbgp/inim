#!/bin/sh
# build-evaluation-pack.sh — build a reproducible evaluation pack
# directory under an explicit output path.
#
# POSIX shell. The pack contains the evaluator/facilitator documents,
# the generated answer key, the scenario manifest, the demo manifest,
# repository commit metadata, and a SHA-256 integrity manifest. It
# never contains: the runtime SQLite database, raw MRT, derived cache,
# private logs, screenshots, CSRF tokens, write-enabled server
# instructions, or excluded project-scope material.
#
# Usage:
#   scripts/build-evaluation-pack.sh --output PATH [--db DEMO.sqlite]
#                                    [--root DIR] [--force]
#
# The output directory must be outside Git by default. Refuses to
# overwrite an existing output directory unless --force.
#
# Byte determinism: the pack contains no volatile timestamps; the
# repository commit is recorded from git when available (packaged
# source runs record 'unavailable').

set -u

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
# Absolute-ize the repository root so the output-path guards compare
# absolute paths even when --root is passed relative.
case "$ROOT" in
    /*) ;;
    *) ROOT=$(CDPATH= cd -- "$ROOT" && pwd) ;;
esac

OUT=""
DB=""
FORCE=0

usage() {
    printf '%s\n' \
        "usage: $0 --output PATH [--db DEMO.sqlite] [--root DIR] [--force]" \
        "" \
        "  --output PATH  pack output directory (must be outside Git)" \
        "  --db PATH      demo database (locates demo-manifest.json; optional)" \
        "  --root DIR     repository root (default: auto-detected)" \
        "  --force        replace an existing output directory" \
        ""
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --output) OUT=$2; shift 2 ;;
        --db) DB=$2; shift 2 ;;
        --root) ROOT=$2; shift 2 ;;
        --force) FORCE=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *)
            printf 'error: unknown argument: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [ -z "$OUT" ]; then
    printf 'error: --output PATH is required\n' >&2
    usage >&2
    exit 2
fi

case "$OUT" in
    /*) OUT_ABS=$OUT ;;
    *)
        OUT_DIR=$(CDPATH= cd -- "$(dirname -- "$OUT")" 2>/dev/null && pwd) || {
            printf 'error: cannot resolve output directory: %s\n' "$(dirname -- "$OUT")" >&2
            exit 2
        }
        OUT_ABS=$OUT_DIR/$(basename -- "$OUT")
        ;;
esac
# Normalize a trailing "/." so guards compare the real directory.
OUT_ABS=$(printf '%s' "$OUT_ABS" | sed 's:/\.$::')
[ -n "$OUT_ABS" ] || OUT_ABS=/

# Safety guards: never allow the output to be a filesystem root, the
# repository root, or anything inside the repository (the pack is a
# generated distribution artifact that must stay outside Git). A ".."
# component is rejected outright — a simple output path never needs it
# and string-normalizing it is not worth the risk.
case "$OUT_ABS" in
    *"/../"*|*"/..")
        printf 'error: --output must not contain ".." components: %s\n' "$OUT" >&2
        exit 2
        ;;
esac
if [ "$OUT_ABS" = "/" ] || [ "$OUT_ABS" = "$ROOT" ]; then
    printf 'error: refusing to use %s as the evaluation pack output\n' "$OUT_ABS" >&2
    exit 2
fi
case "$OUT_ABS" in
    "$ROOT"/*)
        printf 'error: evaluation pack output must be outside the repository (%s)\n' "$ROOT" >&2
        exit 2
        ;;
esac

if [ -e "$OUT_ABS" ]; then
    if [ "$FORCE" -eq 0 ]; then
        printf 'error: output directory already exists: %s\n' "$OUT_ABS" >&2
        printf '       Re-run with --force to replace it, or choose a new --output path.\n' >&2
        exit 1
    fi
    rm -rf "$OUT_ABS"
fi
mkdir -p "$OUT_ABS/documents"

copy_doc() {
    # $1 = repo-relative source, $2 = pack-relative destination
    if [ -f "$ROOT/$1" ]; then
        mkdir -p "$(dirname -- "$OUT_ABS/$2")"
        cp "$ROOT/$1" "$OUT_ABS/$2"
    else
        printf 'warning: missing document %s (skipped)\n' "$1" >&2
    fi
}

# ── Documents ───────────────────────────────────────────────────────
copy_doc docs/evaluation/evaluator/NOC-ALPHA-TASKS.md documents/NOC-ALPHA-TASKS.md
copy_doc docs/evaluation/evaluator/NOC-ALPHA-RESPONSE-SHEET.md documents/NOC-ALPHA-RESPONSE-SHEET.md
copy_doc docs/evaluation/evaluator/TERMS.md documents/TERMS.md
copy_doc docs/evaluation/facilitator/NOC-ALPHA-FACILITATOR-GUIDE.md documents/NOC-ALPHA-FACILITATOR-GUIDE.md
copy_doc docs/evaluation/facilitator/SESSION-NOTES-TEMPLATE.md documents/SESSION-NOTES-TEMPLATE.md
copy_doc docs/evaluation/facilitator/POST-SESSION-DECISION.md documents/POST-SESSION-DECISION.md
copy_doc docs/evaluation/ALPHA-FREEZE.md documents/ALPHA-FREEZE.md
copy_doc docs/evaluation/FEEDBACK-TRIAGE.md documents/FEEDBACK-TRIAGE.md
copy_doc docs/evaluation/EVALUATION-DATA-HANDLING.md documents/EVALUATION-DATA-HANDLING.md
copy_doc docs/evaluation/EXTERNAL-PILOT-CHECKLIST.md documents/EXTERNAL-PILOT-CHECKLIST.md
copy_doc docs/evaluation/POST-PILOT-DECISION-GATE.md documents/POST-PILOT-DECISION-GATE.md
copy_doc docs/evaluation/PILOT-REGISTRY.md documents/PILOT-REGISTRY.md
copy_doc docs/evaluation/NOC-ALPHA-INVITATION.md documents/NOC-ALPHA-INVITATION.md

# ── Manifest and generated artifacts ────────────────────────────────
if [ -f "$ROOT/evaluation/scenarios.toml" ]; then
    cp "$ROOT/evaluation/scenarios.toml" "$OUT_ABS/scenarios.toml"
fi

# Answer key (generated, current artifact).
if [ -f "$ROOT/evaluation/generated/answer-key.json" ]; then
    cp "$ROOT/evaluation/generated/answer-key.json" "$OUT_ABS/answer-key.json"
    cp "$ROOT/evaluation/generated/answer-key.md" "$OUT_ABS/answer-key.md"
else
    printf 'warning: answer-key.json not generated yet (run scripts/build-evaluation-answer-key.py first)\n' >&2
fi

# Demo manifest (next to the demo database; may be absent for packaged runs).
DEMO_MANIFEST=""
if [ -n "$DB" ]; then
    case "$DB" in
        /*) DEMO_MANIFEST=$(dirname -- "$DB")/demo-manifest.json ;;
        *) DEMO_MANIFEST=$(CDPATH= cd -- "$(dirname -- "$DB")" && pwd)/demo-manifest.json ;;
    esac
    if [ -f "$DEMO_MANIFEST" ]; then
        cp "$DEMO_MANIFEST" "$OUT_ABS/demo-manifest.json"
    else
        printf 'warning: demo-manifest.json not found next to %s\n' "$DB" >&2
        DEMO_MANIFEST=""
    fi
fi

# Bootstrap script for rebuilding the demo from the repository.
if [ -f "$ROOT/scripts/evaluator-bootstrap.sh" ]; then
    cp "$ROOT/scripts/evaluator-bootstrap.sh" "$OUT_ABS/evaluator-bootstrap.sh"
    chmod +x "$OUT_ABS/evaluator-bootstrap.sh"
fi

# ── Repository commit metadata ──────────────────────────────────────
COMMIT_SHA="unavailable (packaged source)"
COMMIT_SUBJECT=""
if [ -d "$ROOT/.git" ] && command -v git >/dev/null 2>&1; then
    GIT_SHA=$(cd "$ROOT" && git rev-parse HEAD 2>/dev/null || true)
    if [ -n "$GIT_SHA" ]; then
        COMMIT_SHA=$GIT_SHA
        COMMIT_SUBJECT=$(cd "$ROOT" && git log -1 --format=%s 2>/dev/null || true)
    fi
fi

# ── Integrity manifest (SHA-256 only; not cryptographic signing) ────
{
    printf '# evaluation pack manifest\n'
    printf 'repository_commit: %s\n' "$COMMIT_SHA"
    printf 'repository_commit_subject: %s\n' "$COMMIT_SUBJECT"
    if [ -n "$DEMO_MANIFEST" ]; then
        printf 'demo_manifest_sha256: '
        sha256sum_demo=$( (command -v sha256sum >/dev/null 2>&1 && sha256sum "$DEMO_MANIFEST") || shasum -a 256 "$DEMO_MANIFEST" 2>/dev/null || true)
        printf '%s\n' "$sha256sum_demo" | awk '{print $1}'
    else
        printf 'demo_manifest_sha256: unavailable\n'
    fi
    printf '\nfiles:\n'
    # Deterministic file list: sorted relative paths + SHA-256. The
    # manifest itself is excluded (its own hash would cover partial
    # content while the block is still writing).
    (cd "$OUT_ABS" && find . -type f ! -name SHA256SUMS | sort | while read -r f; do
        f=${f#./}
        if command -v sha256sum >/dev/null 2>&1; then
            h=$(sha256sum "$f" | awk '{print $1}')
        else
            h=$(shasum -a 256 "$f" 2>/dev/null | awk '{print $1}')
        fi
        printf '%s  %s\n' "$h" "$f"
    done)
} > "$OUT_ABS/SHA256SUMS"

# ── Summary ─────────────────────────────────────────────────────────
COUNT=$(find "$OUT_ABS" -type f | wc -l | tr -d ' ')
SIZE=$(du -sk "$OUT_ABS" 2>/dev/null | awk '{print $1}')
printf 'evaluation pack written to %s\n' "$OUT_ABS"
printf 'files: %s, size: %s KiB\n' "$COUNT" "$SIZE"
printf 'repository commit: %s\n' "$COMMIT_SHA"
printf 'integrity manifest: SHA256SUMS (SHA-256 only; not a cryptographic signature)\n'
exit 0
