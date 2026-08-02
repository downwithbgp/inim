#!/usr/bin/env bash
# Documentation drift audit — reproducible repository-local checks.
#
# Verifies internal Markdown links, absolute-path hygiene, obsolete
# terminology in current docs, CLI examples against the built binary,
# the ADR index, the case-study index, session-narrative hygiene, and
# the audit inventory/render. External URLs are not checked (they are
# transient; see docs/audits/external-links-2026-08.md).
#
# Usage:
#   scripts/audit-docs.sh
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/audit_docs.py
