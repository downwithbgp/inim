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



def check_job_workflow_docs() -> list[str]:
    """Stable job-workflow documentation properties (ADR-004)."""
    problems = []
    readme = (ROOT / "README.md").read_text(errors="replace")
    if "analysis-job" not in readme:
        problems.append("README.md: analysis-job commands not documented")
    if "worker" not in readme:
        problems.append("README.md: `inim worker` not documented")
    if "--enable-writes" not in readme:
        problems.append("README.md: --enable-writes must be described (disabled by default)")
    glossary = (ROOT / "docs/GLOSSARY.md").read_text(errors="replace")
    for term in ["Analysis job", "Worker lease", "Plan revision", "Staging artifact"]:
        if term not in glossary:
            problems.append(f"docs/GLOSSARY.md: missing glossary term {term!r}")
    for name in NORMATIVE_DOCS:
        f = ROOT / name
        if not f.exists():
            continue
        for i, line in enumerate(f.read_text(errors="replace").splitlines(), 1):
            low = line.lower()
            if ("web" in low or "http" in low or "request" in low) and (
                "executes analysis" in low or "runs analysis" in low
            ):
                if "never" in low or "not" in low or "doesn't" in low:
                    continue
                problems.append(f"{name}:{i}: implies the web request executes analysis: {line.strip()}")
            if "job" in low and "outcome" in low and "verdict" in low:
                if "is not" in low or "never" in low or "distinct" in low:
                    continue
                problems.append(f"{name}:{i}: calls a job outcome a verdict: {line.strip()}")
    return problems


def check_action_pins() -> list[str]:
    """GitHub Action pins must be full SHAs with a trailing reviewed
    release comment (policy in .github/workflows/ci.yml)."""
    problems = []
    for wf in sorted((ROOT / ".github/workflows").glob("*.yml")):
        for i, line in enumerate(wf.read_text(errors="replace").splitlines(), 1):
            m = re.search(r"uses:\s*([^\s]+)", line)
            if not m:
                continue
            spec = m.group(1)
            if spec.startswith("dtolnay/rust-toolchain"):
                continue  # documented exception: the selector IS the interface
            if "@" not in spec:
                problems.append(f"{wf.relative_to(ROOT)}:{i}: action without version pin: {spec}")
                continue
            ref = spec.split("@", 1)[1]
            if not re.fullmatch(r"[0-9a-f]{40}", ref):
                problems.append(
                    f"{wf.relative_to(ROOT)}:{i}: action pin {spec} is not a full 40-char SHA"
                )
                continue
            comment = line.split("#", 1)[-1].strip() if "#" in line else ""
            if not re.fullmatch(r"v?[0-9][0-9a-z.\-]*", comment):
                problems.append(
                    f"{wf.relative_to(ROOT)}:{i}: pinned action {spec} lacks a reviewed release comment"
                )
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


def check_second_network_docs() -> list[str]:
    """Second-network relationship-scope drift guards (Session 50)."""
    problems = []
    docs = ["docs/DESIGN.md", "docs/DOMAIN.md", "docs/UX.md", "docs/OBSERVABILITY.md",
            "docs/DATA_PROVENANCE.md", "README.md"]
    for name in docs:
        f = ROOT / name
        if not f.exists():
            continue
        text = f.read_text(errors="replace").lower()
        for forbidden in ["globally single-homed", "single homed", "total internet outage"]:
            if forbidden in text:
                problems.append(f"{name}: claims {forbidden!r} without independent evidence")
    glossary = (ROOT / "docs/GLOSSARY.md").read_text(errors="replace")
    for term in ["Managed network", "Named managed relationship", "Attachment qualifier",
                 "Direct relationship observation", "Indirect relationship observation",
                 "Provisional analysis", "Snapshot cutoff"]:
        if term.lower() not in glossary.lower():
            problems.append(f"docs/GLOSSARY.md: missing glossary term {term!r}")
    # Open-event documentation must carry an explicit cutoff.
    for name in ["docs/UX.md", "docs/DESIGN.md"]:
        f = ROOT / name
        if not f.exists():
            continue
        text = f.read_text(errors="replace")
        for line in text.splitlines():
            low = line.lower()
            if "open event" in low and "cutoff" not in low and "provisional" not in low:
                problems.append(f"{name}: open-event text must reference cutoff/provisional: {line.strip()}")
    return problems


def check_project_scope_docs() -> list[str]:
    """Project-scope documentation drift guards (Session 49)."""
    problems = []
    config = ROOT / "config/project-scope.toml"
    if not config.exists():
        problems.append("config/project-scope.toml missing (project-scope policy required)")
        return problems
    text = config.read_text(errors="replace")
    if "schema_version = 1" not in text:
        problems.append("config/project-scope.toml must declare schema_version = 1")
    glossary = (ROOT / "docs/GLOSSARY.md").read_text(errors="replace")
    for term in ["Project scope", "Project-scope exclusion"]:
        if term.lower() not in glossary.lower():
            problems.append(f"docs/GLOSSARY.md: missing glossary term {term!r}")
    if "analytical applicability" not in glossary.lower():
        problems.append("docs/GLOSSARY.md: must define analytical applicability")
    design = (ROOT / "docs/DESIGN.md").read_text(errors="replace")
    if "scope" not in design.lower() or "applicability" not in design.lower():
        problems.append("docs/DESIGN.md: must distinguish project scope from analytical applicability")
    # The excluded name may appear ONLY in the allowlisted reference
    # points: the policy config, the dated removal audit, and the
    # current-policy integration test. Audit-file cross-references by
    # filename are allowed (they link to the dated audit).
    allowlisted = [
        "config/project-scope.toml",
        "docs/audits/2026-08-project-scope-noaa-removal.md",
        "tests/project_scope_policy_test.rs",
        # The enforcement suite asserts the ABSENCE of excluded material
        # (negative assertions), which requires naming it.
        "tests/project_scope_enforcement_test.rs",
        # Same: the second-network semantics suite asserts the NOAA
        # exclusions remain in force.
        "tests/second_network_semantics_test.rs",
        # The candidates audit must name the excluded record to record
        # why it is absent from the shortlist.
        "docs/audits/2026-08-non-noaa-ip-event-candidates.md",
        # CI asserts the packaged absence of excluded material.
        ".github/workflows/ci.yml",
        "scripts/audit_docs.py",  # this guard itself names the tokens
    ]
    # Dated-audit cross-reference filenames are links, not entity
    # mentions; a line containing only such a link is allowed.
    audit_links = [
        "2026-08-project-scope-noaa-removal.md",
        "2026-08-non-noaa-ip-event-candidates.md",
    ]
    import subprocess
    hits = subprocess.run(
        ["git", "grep", "-inE", "NOAA|INC0303298"],
        capture_output=True, text=True,
    ).stdout.splitlines()
    for hit in hits:
        hit = hit.strip()
        if not hit:
            continue
        path = hit.split(":", 1)[0]
        if path in allowlisted:
            continue
        line = hit.split(":", 2)[2] if hit.count(":") >= 2 else ""
        if any(link in line for link in audit_links) and "NOAA" not in line.replace(
            *("'", " "), 1
        ).split("noaa", 1)[0].upper().split(" ")[0]:
            # The line mentions the audit FILENAME only (no standalone
            # entity mention).
            continue
        problems.append(f"excluded name appears outside the allowlist: {hit}")
    return problems


def check_evaluation_docs() -> list[str]:
    """NOC alpha evaluation drift guards (Session 51)."""
    problems = []
    freeze = ROOT / "docs/evaluation/ALPHA-FREEZE.md"
    if not freeze.exists():
        problems.append("docs/evaluation/ALPHA-FREEZE.md missing (alpha freeze policy required)")
        return problems
    freeze_text = freeze.read_text(errors="replace")
    # Freeze policy content anchors (Part 1).
    if "exit condition" not in freeze_text.lower():
        problems.append("ALPHA-FREEZE.md: exit conditions must be defined")
    for allowed in ["security", "correctness"]:
        if allowed not in freeze_text.lower():
            problems.append(f"ALPHA-FREEZE.md: must allow {allowed} fixes during the freeze")
    if "feature expansion" not in freeze_text.lower():
        problems.append("ALPHA-FREEZE.md: must prohibit feature expansion without evidence")
    for doc in ["docs/README.md", "CONTRIBUTING.md"]:
        f = ROOT / doc
        if f.exists() and "ALPHA-FREEZE" not in f.read_text(errors="replace"):
            problems.append(f"{doc}: must link the alpha freeze policy")
    # Task booklet exists and contains no answer material (Part 7/10).
    # Task text may name reviewed ASNs when the task itself must ask
    # about them (Smithville E4 names the reviewed pair); it must never
    # contain timestamps, paths, or verdicts.
    tasks = ROOT / "docs/evaluation/evaluator/NOC-ALPHA-TASKS.md"
    if not tasks.exists():
        problems.append("docs/evaluation/evaluator/NOC-ALPHA-TASKS.md missing")
    else:
        tasks_text = tasks.read_text(errors="replace")
        for leak in ["answer key", "AS225", "11164", "16:45:25", "07:33:59",
                     "11537 40220", "expected evaluator understanding"]:
            if leak.lower() in tasks_text.lower():
                problems.append(f"NOC-ALPHA-TASKS.md: evaluator-visible answer leakage ({leak!r})")
    # Generated answer key exists with a generation header (Part 9/52).
    ak = ROOT / "evaluation/generated/answer-key.json"
    if not ak.exists():
        problems.append("evaluation/generated/answer-key.json missing (run scripts/build-evaluation-answer-key.py)")
    else:
        try:
            doc = json.loads(ak.read_text(errors="replace"))
            if doc.get("schema_version") != 1:
                problems.append("answer-key.json: schema_version must be 1")
            if not doc.get("generator"):
                problems.append("answer-key.json: generation header missing generator")
            if not doc.get("source_demo_manifest_sha256"):
                problems.append("answer-key.json: generation header missing demo-manifest SHA")
            for s in doc.get("scenarios", []):
                if not s.get("reviewed_relationship", {}).get("reference"):
                    problems.append(f"answer-key.json: scenario {s.get('source_event', {}).get('id')} missing artifact reference")
        except ValueError:
            problems.append("answer-key.json: invalid JSON")
    # Scenario manifest (Part 6): versioned, unique ids, relative paths.
    manifest = ROOT / "evaluation/scenarios.toml"
    if not manifest.exists():
        problems.append("evaluation/scenarios.toml missing")
    else:
        try:
            import tomllib
            m = tomllib.loads(manifest.read_text(errors="replace"))
            if m.get("schema_version") != 1:
                problems.append("evaluation/scenarios.toml: schema_version must be 1")
            ids = [s.get("id") for s in m.get("scenarios", [])]
            if len(ids) != len(set(ids)):
                problems.append("evaluation/scenarios.toml: scenario ids must be unique")
            task_ids = [t for s in m.get("scenarios", []) for t in s.get("task_ids", [])]
            # Task IDs must be unique WITHIN each scenario; tasks may be
            # shared across scenarios (Section A orientation tasks apply
            # to every scenario).
            for s in m.get("scenarios", []):
                ids = s.get("task_ids", [])
                if len(ids) != len(set(ids)):
                    problems.append(f"evaluation/scenarios.toml: scenario {s.get('id')} has duplicate task ids")
            for s in m.get("scenarios", []):
                p = s.get("path", "")
                if p.startswith("http") or ":" in p.split("/")[0]:
                    problems.append(f"evaluation/scenarios.toml: scenario path must be relative ({p})")
                if s.get("status") not in ("evaluator", "optional", "facilitator-only"):
                    problems.append(f"evaluation/scenarios.toml: scenario {s.get('id')} has invalid status")
        except Exception as exc:  # noqa: BLE001 - drift guard reports any parse failure
            problems.append(f"evaluation/scenarios.toml: unparseable ({exc})")
    # Pilot registry: no fabricated sessions (Part 42/48).
    registry = ROOT / "docs/evaluation/PILOT-REGISTRY.md"
    if registry.exists():
        reg = registry.read_text(errors="replace")
        if "No external evaluations completed yet" not in reg and "external sessions recorded" not in reg.lower():
            problems.append("PILOT-REGISTRY.md: must truthfully state the external-session count")
    # No external-evaluation claims (Part 48).
    for name in ["README.md", "docs/README.md"]:
        f = ROOT / name
        if not f.exists():
            continue
        text = f.read_text(errors="replace").lower()
        for claim in ["evaluated by network engineers", "externally validated", "noc validated",
                      "user tested", "proven useful", "production ready", "validated by network"]:
            if claim in text:
                problems.append(f"{name}: unsupported external-validation claim ({claim!r})")
    readme = ROOT / "README.md"
    if readme.exists() and "alpha" not in readme.read_text(errors="replace").lower():
        problems.append("README.md: must call the project a public alpha")
    # Response sheet must not require personal identity (Part 12).
    sheet = ROOT / "docs/evaluation/evaluator/NOC-ALPHA-RESPONSE-SHEET.md"
    if sheet.exists():
        s = re.sub(r"\s+", " ", sheet.read_text(errors="replace"))
        if "no real name and no employer are required" not in s.lower():
            problems.append("NOC-ALPHA-RESPONSE-SHEET.md: must state identity is not required")
    # Issue form warns against confidential data (Part 15).
    form = ROOT / ".github/ISSUE_TEMPLATE/noc-alpha-feedback.yml"
    if form.exists():
        ftext = form.read_text(errors="replace")
        if "confidential" not in ftext.lower():
            problems.append("noc-alpha-feedback.yml: must warn against confidential data")
        for required in ["organization", "real name", "email"]:
            if f"required: true" in ftext and required in ftext.lower():
                pass  # required flags exist only for the privacy confirmation
    # Bootstrap: read-only, loopback, no worker (Part 5). The guard
    # matches command positions, not prose (the script documents that
    # write mode and the worker are never used).
    boot = ROOT / "scripts/evaluator-bootstrap.sh"
    if boot.exists():
        b = boot.read_text(errors="replace")
        if re.search(r"serve[^\n]*--enable-writes", b):
            problems.append("evaluator-bootstrap.sh: must never enable write mode")
        if re.search(r'(^|[\s"\'])(inim|"\$BIN"|\$BIN)[\s"\']+worker\b', b):
            problems.append("evaluator-bootstrap.sh: must not start the worker")
        if "127.0.0.1" not in b:
            problems.append("evaluator-bootstrap.sh: must default to loopback bind")
    # Evaluator material contains no excluded project-scope material (Part 36).
    import subprocess as _sp
    excluded = ["NOAA", "INC0303298"]
    eval_paths = ["evaluation/", "docs/evaluation/"]
    hits = _sp.run(
        ["git", "grep", "-inE", "NOAA|INC0303298", "--", *eval_paths],
        capture_output=True, text=True,
    ).stdout.splitlines()
    for hit in hits:
        hit = hit.strip()
        if not hit:
            continue
        problems.append(f"excluded material in evaluation files: {hit}")
    return problems


def main() -> int:
    binary = str(ROOT / "target/debug/inim")
    if not Path(binary).exists():
        print("building debug binary for CLI checks...", file=sys.stderr)
        subprocess.check_call(["cargo", "build", "-q"], cwd=ROOT)
    problems: list[str] = []
    problems += check_links()
    problems += check_action_pins()
    problems += check_absolute_paths()
    problems += check_terminology()
    problems += check_cli_examples(binary)
    problems += check_adr_index()
    problems += check_case_study_index()
    problems += check_session_narrative()
    problems += check_inventory_and_render()
    problems += check_job_workflow_docs()
    problems += check_project_scope_docs()
    problems += check_second_network_docs()
    problems += check_evaluation_docs()
    if problems:
        print("documentation drift audit FAILED:", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1
    print("documentation drift audit: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
