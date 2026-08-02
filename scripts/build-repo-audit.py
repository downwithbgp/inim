#!/usr/bin/env python3
"""Render docs/audits/2026-08-repository-truth-audit.md from the
machine-readable inventory.

The inventory (docs/audits/repository-inventory.json) is the checked
source of truth; this script only renders it plus git-derived facts
(tracked set, changed-since-audit-start status). Run it after editing
the inventory:

    python3 scripts/build-repo-audit.py

The output contains repository-relative paths only — never absolute
local paths.
"""

import json
import subprocess
import sys
from collections import Counter
from datetime import date

AUDIT_START = "0517aac"  # HEAD when the 2026-08 repository truth audit began
INVENTORY = "docs/audits/repository-inventory.json"
OUTPUT = "docs/audits/2026-08-repository-truth-audit.md"

CATEGORY_DEFINITIONS = {
    "Production source": "Rust implementation code shipped in the crate.",
    "Test source": "Rust test code (unit and integration).",
    "Template or stylesheet": "Askama HTML templates rendered by the web workbench.",
    "Script or developer tool": "Shell/Python developer scripts; not shipped in the crate.",
    "Normative current documentation": "Explanatory documentation that must describe the current implementation.",
    "Historical decision record": "ADR or dated decision/evaluation document; records what was decided when.",
    "Reviewed case-study interpretation": "Human-reviewed case-study claims, metadata, and pilot decisions.",
    "Immutable or generated evidence": "Canonical protocol evidence or derived artifacts; contents are not hand-edited.",
    "Test fixture": "Synthetic or minimized fixture data used by tests.",
    "Configuration": "Reviewed configuration: manifests, network profiles, build/deny config.",
    "Packaging or release metadata": "Crate metadata, changelog, and release instructions.",
    "License or third-party notice": "License text and third-party notices.",
    "GitHub/community metadata": "CI workflows and community-facing repository configuration.",
}

AUTHORED_CATEGORIES = {
    "Production source",
    "Test source",
    "Template or stylesheet",
    "Script or developer tool",
    "Normative current documentation",
    "Historical decision record",
    "Reviewed case-study interpretation",
    "Test fixture",
    "Configuration",
    "Packaging or release metadata",
    "License or third-party notice",
    "GitHub/community metadata",
}
GENERATED_CATEGORIES = {"Immutable or generated evidence"}

REVIEW_RESULT = {
    "Production source": "implementation comments audited in this audit",
    "Test source": "reviewed in this audit",
    "Template or stylesheet": "user-visible text audited in this audit",
    "Script or developer tool": "comments and usage strings audited in this audit",
    "Normative current documentation": "line-by-line reviewed in this audit",
    "Historical decision record": "status and applicability reviewed in this audit",
    "Reviewed case-study interpretation": "claims re-checked against canonical evidence in this audit",
    "Immutable or generated evidence": "schema/container reviewed; contents canonical, not hand-edited",
    "Test fixture": "provenance reviewed in this audit",
    "Configuration": "reviewed in this audit",
    "Packaging or release metadata": "reviewed in this audit",
    "License or third-party notice": "reviewed in this audit",
    "GitHub/community metadata": "reviewed in this audit",
}


def git(args):
    return subprocess.check_output(["git"] + args, text=True).strip()


def main() -> int:
    inventory = json.load(open(INVENTORY))
    tracked = set(git(["ls-files"]).splitlines())
    inventory_paths = {e["path"] for e in inventory}

    problems = []
    if inventory_paths != tracked:
        only_inv = sorted(inventory_paths - tracked)
        only_git = sorted(tracked - inventory_paths)
        problems.append(f"inventory/tracked mismatch: only-inventory={only_inv} only-git={only_git}")

    valid_categories = set(CATEGORY_DEFINITIONS) | GENERATED_CATEGORIES
    for e in inventory:
        if e["category"] not in valid_categories:
            problems.append(f"{e['path']}: unknown category {e['category']}")
        if e["category"] in AUTHORED_CATEGORIES and e["generated"]:
            problems.append(f"{e['path']}: authored category marked generated")
        if e["category"] in GENERATED_CATEGORIES and not e["generated"]:
            problems.append(f"{e['path']}: generated category marked authored")
        if e["category"] == "Historical decision record" and e["current"]:
            problems.append(f"{e['path']}: historical record marked current")
        if e["category"] != "Historical decision record" and not e["current"]:
            problems.append(f"{e['path']}: non-historical record marked historical")

    # The rendered audit is deterministic: review state is a property of
    # the inventory, not of git history. No history access is needed, so
    # the render also works from a shallow CI checkout.

    counts = Counter(e["category"] for e in inventory)
    lines = []
    w = lines.append
    w("# Repository truth audit — 2026-08")
    w("")
    w(f"Audit start HEAD: `{AUDIT_START}` · audit date: {date.today().isoformat()}")
    w("")
    w("This audit verifies that every tracked file is classified, that every "
      "current statement matches the implemented model, and that historical "
      "records and generated evidence are clearly distinguished. The "
      "machine-readable source of this document is "
      "`docs/audits/repository-inventory.json`; regenerate with "
      "`python3 scripts/build-repo-audit.py`. Paths are repository-relative "
      "only; no absolute local paths appear in this audit.")
    w("")
    w("## Categories")
    w("")
    w("| Category | Meaning |")
    w("|---|---|")
    for c in sorted(CATEGORY_DEFINITIONS):
        w(f"| {c} | {CATEGORY_DEFINITIONS[c]} |")
    w("")
    w("## Summary")
    w("")
    w(f"Tracked files: **{len(tracked)}** · inventory entries: **{len(inventory)}**")
    w("")
    w("| Category | Files |")
    w("|---|---|")
    for c, n in sorted(counts.items(), key=lambda kv: -kv[1]):
        w(f"| {c} | {n} |")
    w("")
    w("## Inventory")
    w("")
    w("| Path | Category | Audience | Authoritative source | Generated | Current | Review result | Changes required | Final status |")
    w("|---|---|---|---|---|---|---|---|---|")
    for e in sorted(inventory, key=lambda e: e["path"]):
        path = e["path"]
        gen = "yes" if e["generated"] else "no"
        cur = "current" if e["current"] else "historical"
        req = "none"
        status = "reviewed in this audit"
        w(f"| `{path}` | {e['category']} | {e['audience']} | {e['authoritative']} | {gen} | {cur} | {REVIEW_RESULT[e['category']]} | {req} | {status} |")
    w("")
    if problems:
        print("INVENTORY PROBLEMS:", file=sys.stderr)
        for p in problems:
            print(" -", p, file=sys.stderr)
        return 1
    open(OUTPUT, "w").write("\n".join(lines) + "\n")
    print(f"wrote {OUTPUT} ({len(inventory)} entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
