#!/usr/bin/env python3
"""Repository documentation drift audit.

Checks stable, repository-local properties that must hold after any
documentation change. External links are NOT checked here (they are
transient; see docs/audits/external-links-2026-08.md for the dated
snapshot). Run via scripts/audit-docs.sh; the same checks run in CI.

Checks:
  1. every internal Markdown link resolves (file exists; anchor names
     are not validated);
  2. no absolute developer paths in docs;
  3. no obsolete terminology in current normative docs (quoted
     historical material is exempt);
  4. CLI examples in docs reference existing subcommands and options
     (against the built binary);
  5. the ADR index lists every ADR in docs/ADRs/;
  6. every tracked case study has a README and is listed in the root
     README;
  7. no unreviewed session-number narrative in current normative docs;
  8. the audit inventory covers every tracked file and the rendered
     audit is up to date.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Current normative docs (line-by-line audited). Historical records
# (ADRs, DECISIONS, MONOCLE_EVALUATION, REQUIREMENTS, TASKS,
# session-10-baseline, spec/) and dated case-study interpretation
# records are exempt from the session-number and terminology checks.
NORMATIVE_DOCS = [
    "README.md",
    "CONTRIBUTING.md",
    "CHANGELOG.md",
    "RELEASING.md",
    "docs/README.md",
    "docs/GLOSSARY.md",
    "docs/DESIGN.md",
    "docs/DOMAIN.md",
    "docs/OBSERVABILITY.md",
    "docs/DATA_PROVENANCE.md",
    "docs/BENCHMARK.md",
    "docs/UX.md",
    "docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md",
    "docs/sources/GRNOC_BULK_ACCESS_REQUEST.md",
]

OBSOLETE_TERMS = [
    "departed-I2",
    "OpenEvent",
    "affected Internet percentage",
    "failover confirmed",
    "traffic restored",
    "outage severity",
    "global impact",
    "Internet impact",
]

ABSOLUTE_PATH_MARKERS = ["/home/", "/Users/", "C:\\Users", "\\\\wsl", "/tmp/"]

SESSION_RE = re.compile(r"Session \d+")


def git_ls_files() -> set[str]:
    out = subprocess.check_output(["git", "ls-files"], cwd=ROOT, text=True)
    return set(out.splitlines())


def all_markdown() -> list[Path]:
    # spec/ is historical planning material, not current documentation.
    return [
        ROOT / p
        for p in git_ls_files()
        if p.endswith(".md") and not p.startswith("spec/")
    ]


def check_links() -> list[str]:
    problems = []
    link_re = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
    for f in all_markdown():
        text = f.read_text(errors="replace")
        for m in link_re.finditer(text):
            target = m.group(1).strip()
            if target.startswith(("http://", "https://", "mailto:", "#")):
                continue
            # strip anchor and surrounding backticks/angle brackets
            path_part = target.split("#")[0].strip("`<>")
            if not path_part:
                continue
            resolved = (f.parent / path_part).resolve()
            if not resolved.exists():
                problems.append(f"{f.relative_to(ROOT)}: broken link {target}")
    return problems


def check_absolute_paths() -> list[str]:
    problems = []
    for f in all_markdown():
        rel = f.relative_to(ROOT).as_posix()
        if rel == "docs/audits/external-links-2026-08.md":
            continue  # a URL status record: URLs contain arbitrary paths
        text = f.read_text(errors="replace")
        for marker in ABSOLUTE_PATH_MARKERS:
            if marker in text:
                problems.append(f"{rel}: absolute path marker {marker}")
    return problems


def check_terminology() -> list[str]:
    problems = []
    for name in NORMATIVE_DOCS:
        if name == "docs/GLOSSARY.md":
            continue  # the glossary is the term authority and names them
        f = ROOT / name
        if not f.exists():
            continue
        for i, line in enumerate(f.read_text(errors="replace").splitlines(), 1):
            for term in OBSOLETE_TERMS:
                if term in line:
                    # exempt quoted prohibitions and source quotes
                    if f'"{term}"' in line or f"`{term}`" in line:
                        continue
                    problems.append(f"{name}:{i}: obsolete term {term!r}")
    return problems


def _options(help_text: str) -> set[str]:
    """All --options on indented help lines (handles `-e, --event` rows)."""
    opts: set[str] = set()
    for line in help_text.splitlines():
        if re.match(r"^[ \t]+", line) and "--" in line:
            opts |= set(re.findall(r"--[a-z-]+", line))
    return opts

def cli_tree(binary: str) -> dict:
    """Map of subcommand -> set of options, from --help output."""
    tree: dict[str, set[str]] = {}
    help_out = subprocess.check_output([binary, "--help"], text=True, stderr=subprocess.STDOUT)
    top = [m for m in re.findall(r"^\s{2}([a-z][a-z-]*)\s", help_out, re.M)]
    tree[""] = _options(help_out)
    for sub in top:
        try:
            out = subprocess.check_output(
                [binary, sub, "--help"], text=True, stderr=subprocess.STDOUT
            )
        except subprocess.CalledProcessError:
            continue
        tree[sub] = _options(out)
        # nested catalog subcommands (two levels: catalog sync grnoc,
        # catalog relationships audit, catalog case-study import, ...)
        nested = [m for m in re.findall(r"^\s{2}([a-z][a-z-]*)\s", out, re.M)]
        for nsub in nested:
            try:
                nout = subprocess.check_output(
                    [binary, sub, nsub, "--help"], text=True, stderr=subprocess.STDOUT
                )
            except subprocess.CalledProcessError:
                continue
            tree[f"{sub} {nsub}"] = _options(nout)
            sub2 = [m for m in re.findall(r"^\s{2}([a-z][a-z-]*)\s", nout, re.M)]
            for nsub2 in sub2:
                try:
                    nout2 = subprocess.check_output(
                        [binary, sub, nsub, nsub2, "--help"], text=True,
                        stderr=subprocess.STDOUT,
                    )
                except subprocess.CalledProcessError:
                    continue
                tree[f"{sub} {nsub} {nsub2}"] = _options(nout2)
    return tree


def check_cli_examples(binary: str) -> list[str]:
    problems = []
    tree = cli_tree(binary)
    known = set(tree)
    cmd_re = re.compile(r"\binim ([a-z][a-z-]*(?: [a-z][a-z-]*)*)")
    top_level = {"plan", "analyze", "compare", "migrate-manifest", "catalog", "serve"}
    for name in NORMATIVE_DOCS + ["case-studies/manlan-2019/README.md",
                                  "case-studies/inc0299001/README.md",
                                  "case-studies/inc0302574/README.md"]:
        f = ROOT / name
        if not f.exists():
            continue
        for i, line in enumerate(f.read_text(errors="replace").splitlines(), 1):
            for m in cmd_re.finditer(line):
                cmds = m.group(1).split()
                # prose ("inim is a ...", "inim would ...") is not a command
                if not cmds or cmds[0] not in top_level:
                    continue
                # deepest known command prefix
                depth = 0
                for d in range(1, len(cmds) + 1):
                    if " ".join(cmds[:d]) in known:
                        depth = d
                if depth == 0:
                    problems.append(
                        f"{name}:{i}: unknown command `inim {' '.join(cmds)}`")
                    continue
                opts = re.findall(r"(--[a-z-]+)", line)
                if opts:
                    opts_known = set()
                    for d in range(1, depth + 1):
                        opts_known |= tree.get(" ".join(cmds[:d]), set())
                    for o in opts:
                        if o not in opts_known:
                            problems.append(
                                f"{name}:{i}: unknown option {o} for "
                                f"`inim {' '.join(cmds[:depth])}`")
    return problems


def check_adr_index() -> list[str]:
    problems = []
    adr_files = sorted((ROOT / "docs/ADRs").glob("*.md"))
    index = (ROOT / "docs/ADRs/README.md").read_text(errors="replace")
    for f in adr_files:
        if f.name == "README.md":
            continue
        if f.name not in index:
            problems.append(f"docs/ADRs/README.md: ADR {f.name} not listed")
    return problems


def check_case_study_index() -> list[str]:
    problems = []
    root_readme = (ROOT / "README.md").read_text(errors="replace")
    for d in sorted((ROOT / "case-studies").iterdir()):
        if not d.is_dir():
            continue
        rel = f"case-studies/{d.name}"
        if not (d / "README.md").exists():
            problems.append(f"{rel}: missing README.md")
        if rel not in root_readme:
            problems.append(f"README.md: case study {rel} not listed")
    return problems


def check_session_narrative() -> list[str]:
    problems = []
    for name in NORMATIVE_DOCS:
        f = ROOT / name
        if not f.exists():
            continue
        for i, line in enumerate(f.read_text(errors="replace").splitlines(), 1):
            if SESSION_RE.search(line):
                problems.append(f"{name}:{i}: session-number narrative: {line.strip()[:80]}")
    return problems


def check_inventory_and_render() -> list[str]:
    problems = []
    tracked = git_ls_files()
    inv_path = ROOT / "docs/audits/repository-inventory.json"
    inv = json.loads(inv_path.read_text())
    inv_paths = {e["path"] for e in inv}
    for p in sorted(tracked - inv_paths):
        problems.append(f"inventory missing tracked file: {p}")
    for p in sorted(inv_paths - tracked):
        problems.append(f"inventory names untracked file: {p}")
    # rendered audit up to date?
    render = subprocess.run(
        [sys.executable, "scripts/build-repo-audit.py"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if render.returncode != 0:
        problems.append("build-repo-audit.py failed: " + render.stderr.strip())
    else:
        # re-render must be a no-op against the committed doc
        diff = subprocess.run(
            ["git", "diff", "--quiet", "--", "docs/audits/2026-08-repository-truth-audit.md"],
            cwd=ROOT, capture_output=True,
        )
        if diff.returncode != 0:
            problems.append("rendered audit doc is out of date (run scripts/audit-docs.sh or build-repo-audit.py)")
    return problems


def main() -> int:
    binary = str(ROOT / "target/debug/inim")
    if not Path(binary).exists():
        print("building debug binary for CLI checks...", file=sys.stderr)
        subprocess.check_call(["cargo", "build", "-q"], cwd=ROOT)
    problems: list[str] = []
    problems += check_links()
    problems += check_absolute_paths()
    problems += check_terminology()
    problems += check_cli_examples(binary)
    problems += check_adr_index()
    problems += check_case_study_index()
    problems += check_session_narrative()
    problems += check_inventory_and_render()
    if problems:
        print("documentation drift audit FAILED:", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1
    print("documentation drift audit: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
