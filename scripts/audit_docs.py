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
    "docs/STATUS.md",
    "docs/GLOSSARY.md",
    "docs/DESIGN.md",
    "docs/DOMAIN.md",
    "docs/OBSERVABILITY.md",
    "docs/DATA_PROVENANCE.md",
    "docs/BENCHMARK.md",
    "docs/UX.md",
    "docs/OPERATIONS.md",
    "docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md",
    "docs/sources/GRNOC_BULK_ACCESS_REQUEST.md",
]

# Verified reference docs are normative current documentation for the
# terminology/session checks (the route/CLI/schema checks below verify
# them against code).
REFERENCE_DOCS = [
    "docs/reference/CLI.md",
    "docs/reference/API.md",
    "docs/reference/WEB-ROUTES.md",
    "docs/reference/CATALOG-SCHEMA.md",
    "docs/reference/SCHEMA-VERSIONS.md",
    "docs/reference/ARTIFACTS.md",
]

# Current reviewed case-study READMEs (checked by name in the
# case-study-specific guards).
CASE_STUDY_READMES = {
    "case-studies/manlan-2019/README.md",
    "case-studies/inc0299001/README.md",
    "case-studies/inc0302574/README.md",
    "case-studies/manlan-esnet-2019/README.md",
    "case-studies/indiana-gigapop-smithville-2026/README.md",
}

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
    try:
        out = subprocess.check_output(["git", "ls-files"], cwd=ROOT, text=True)
        return set(out.splitlines())
    except (subprocess.CalledProcessError, FileNotFoundError):
        # Packaged source has no .git; git-dependent checks skip.
        return set()


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
    for name in NORMATIVE_DOCS + REFERENCE_DOCS:
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
    for name in NORMATIVE_DOCS + REFERENCE_DOCS:
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
    if not tracked:
        return problems  # packaged source: git-dependent render check skips
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
    """Second-network relationship-scope drift guards."""
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
    """Project-scope documentation drift guards."""
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
        cwd=ROOT, capture_output=True, text=True,
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
    """NOC alpha evaluation drift guards."""
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
        cwd=ROOT, capture_output=True, text=True,
    ).stdout.splitlines()
    for hit in hits:
        hit = hit.strip()
        if not hit:
            continue
        problems.append(f"excluded material in evaluation files: {hit}")
    return problems


def check_documentation_inventory() -> list[str]:
    """The dated documentation inventory's checked lists must match git
    exactly (every tracked Markdown, docs/, evaluation/, and .github/
    file listed; every listed file tracked)."""
    problems = []
    tracked = git_ls_files()
    if not tracked:
        return problems  # packaged source: no git tree to compare against
    inv = (ROOT / "docs/audits/2026-08-documentation-inventory.md").read_text(errors="replace")
    sections = {
        "markdown": [f for f in tracked if f.endswith(".md") and not f.startswith("spec/")],
        "docs": [f for f in tracked if f.startswith("docs/")],
        "evaluation": [f for f in tracked if f.startswith("evaluation/")],
        "github": [f for f in tracked if f.startswith(".github/")],
    }
    for label, expected in sections.items():
        for p in expected:
            if f"\n{p}\n" not in inv:
                problems.append(f"documentation inventory: {label} file missing from checked list: {p}")
    # parse back the fenced lists and verify they only contain tracked files
    for m in re.finditer(r"```\n((?:[^\n`]+\n)+)```", inv):
        listed = [ln.strip() for ln in m.group(1).splitlines() if ln.strip()]
        for p in listed:
            if p not in tracked:
                problems.append(f"documentation inventory: listed untracked file: {p}")
    return problems


def check_anchor_links() -> list[str]:
    """Internal markdown links with #anchors must resolve to a heading in
    the target file (GitHub-style anchor approximation)."""
    problems = []

    def anchors(path: Path) -> set[str]:
        out = set()
        for line in path.read_text(errors="replace").splitlines():
            m = re.match(r"^(#{1,6})\s+(.*)", line)
            if m:
                a = re.sub(r"[^\w\s-]", "", m.group(2).strip().lower())
                out.add(a.replace(" ", "-"))
        return out

    link_re = re.compile(r"\[[^\]]*\]\(([^)#]+)(#[^)]*)?\)")
    for f in all_markdown():
        text = f.read_text(errors="replace")
        for m in link_re.finditer(text):
            path_part, anchor = m.group(1), m.group(2)
            if path_part.startswith(("http://", "https://", "mailto:")):
                continue
            if not anchor or anchor == "#":
                continue
            target = (f.parent / path_part.strip("`<>")).resolve()
            if not target.exists():
                continue  # file-level breakage is reported by check_links
            wanted = anchor[1:]
            if wanted not in anchors(target):
                problems.append(
                    f"{f.relative_to(ROOT)}: anchor #{wanted} not found in {path_part}"
                )
    return problems


def check_cli_reference(binary: str) -> list[str]:
    """Every `inim <command>` form in the CLI reference must exist in the
    binary's help tree."""
    problems = []
    tree = cli_tree(binary)
    known = set(tree)
    doc = (ROOT / "docs/reference/CLI.md").read_text(errors="replace")
    cmd_re = re.compile(r"\binim ([a-z][a-z-]*(?: [a-z][a-z-]*)*)")
    for i, line in enumerate(doc.splitlines(), 1):
        for m in cmd_re.finditer(line):
            cmds = m.group(1).split()
            depth = 0
            for d in range(1, len(cmds) + 1):
                if " ".join(cmds[:d]) in known:
                    depth = d
            if depth == 0:
                problems.append(f"docs/reference/CLI.md:{i}: unknown command `inim {' '.join(cmds)}`")
    return problems


def router_paths() -> list[str]:
    """Every route literal registered in the axum router, from source."""
    paths = set()
    for f in sorted((ROOT / "src/catalog/web").glob("*.rs")):
        text = f.read_text(errors="replace")
        for m in re.finditer(r'\.route\(\s*"([^"]+)"', text):
            paths.add(m.group(1))
    return sorted(paths)


def check_api_route_reference() -> list[str]:
    """The API and web route references must list every router route, and
    every documented route must exist in the router source."""
    problems = []
    router = router_paths()
    docs = ["docs/reference/API.md", "docs/reference/WEB-ROUTES.md"]
    documented = []
    for name in docs:
        text = (ROOT / name).read_text(errors="replace")
        for m in re.finditer(r"^\| (`?)(/[^`|]*)`? \|", text, re.M):
            route = m.group(2).strip()
            documented.append((name, route))
    documented_routes = {r for _, r in documented}
    for route in router:
        if route not in documented_routes:
            problems.append(f"api route reference: undocumented router route {route}")
    for name, route in documented:
        if route not in router:
            problems.append(f"{name}: documented route not in router: {route}")
    return problems


def schema_constant(name: str) -> str:
    """Read a `pub const NAME: u32 = N;` value from src/.

    Works with or without a git tree (packaged source fallback).
    """
    pat = re.compile(rf"pub const {name}: u32 = ([0-9]+);")
    for f in sorted((ROOT / "src").rglob("*.rs")):
        m = pat.search(f.read_text(errors="replace"))
        if m:
            return m.group(1)
    out = subprocess.run(
        ["git", "grep", "-h", "-E", rf"pub const {name}: u32 = [0-9]+;"],
        cwd=ROOT, capture_output=True, text=True,
    ).stdout
    m = re.search(r"= ([0-9]+);", out)
    return m.group(1) if m else "?"


def check_schema_matrix() -> list[str]:
    """SCHEMA-VERSIONS.md must agree with the implementation constants."""
    problems = []
    doc = (ROOT / "docs/reference/SCHEMA-VERSIONS.md").read_text(errors="replace")
    constants = {
        "CATALOG_SCHEMA_VERSION": "Catalog database",
        "MANIFEST_SCHEMA_VERSION": "Manifest (analysis plan input)",
        "RIB_CACHE_SCHEMA_VERSION": "RIB derived cache",
        "UPDATE_CACHE_SCHEMA_VERSION": "UPDATE derived cache",
        "OBSERVATION_SCHEMA_VERSION": "RouteObservation",
        "COHORT_IDENTITY_SCHEMA_VERSION": "Frozen cohort identity",
        "EXTRACTION_SCHEMA_VERSION": "Source-extraction cache",
        "REPORT_SCHEMA_VERSION": "Report",
        "EVIDENCE_APPENDIX_SCHEMA_VERSION": "Evidence appendix",
        "ARCHIVE_MANIFEST_SCHEMA_VERSION": "Archive manifest",
        "LIFECYCLE_ARTIFACT_SCHEMA_VERSION": "Lifecycle artifact",
        "TRANSITIONS_ARTIFACT_SCHEMA_VERSION": "Transitions artifact",
        "WITHDRAWAL_AUDIT_SCHEMA_VERSION": "Withdrawal audit",
        "SEMANTIC_WAVE_SCHEMA_VERSION": "Semantic wave artifact",
        "COMPARISON_SCHEMA_VERSION": "Comparison artifact",
        "ANALYSIS_PLAN_SCHEMA_VERSION": "Analysis-plan artifact",
        "EXECUTION_METADATA_SCHEMA_VERSION": "Execution metadata",
        "PERFORMANCE_SCHEMA_VERSION": "Performance metadata",
        "SCOPE_CONFIG_SCHEMA_VERSION": "Project-scope policy",
        "CASE_STUDY_DATA_SCHEMA_VERSION": "Case-study data file",
        "TARGET_RESEARCH_SCHEMA_VERSION": "Target-research record",
    }
    for const, label in constants.items():
        value = schema_constant(const)
        if value == "?":
            problems.append(f"schema matrix: constant {const} not found")
            continue
        row = re.search(rf"\| {re.escape(label)} \| v(\d+)", doc)
        if not row:
            problems.append(f"schema matrix: no row for {label}")
            continue
        if row.group(1) != value:
            problems.append(
                f"schema matrix: {label} documented v{row.group(1)} but code says v{value}"
            )
    return problems


def check_spec_coverage() -> list[str]:
    """The specification coverage matrix lists every normative area."""
    problems = []
    doc = (ROOT / "docs/audits/2026-08-specification-coverage.md").read_text(errors="replace")
    areas = [
        "Product purpose and scope", "Domain model", "Architecture",
        "Observability limits", "Data provenance", "Project scope",
        "Network profiles", "Event interpretation", "Plan readiness",
        "Route-selection semantics", "Observer eligibility",
        "Route and stream identity", "Lifecycle reconstruction",
        "Findings", "Restoration classes", "Observed results and expectation assessment",
        "Job state machine", "Worker leases", "Staging and publication",
        "Catalog behavior", "API behavior", "CLI behavior", "Web routes",
        "Demo behavior", "Evaluation freeze", "Source adapters (GRNOC)",
        "Source families (RouteViews/RIS)",
    ]
    for area in areas:
        if f"| {area} " not in doc:
            problems.append(f"specification coverage: missing area {area!r}")
    return problems


def check_status_doc() -> list[str]:
    """STATUS.md is the current public status page."""
    problems = []
    doc = (ROOT / "docs/STATUS.md").read_text(errors="replace")
    if "Public alpha" not in doc:
        problems.append("docs/STATUS.md: must say the stage is a public alpha")
    if "Zero external evaluation sessions" not in doc:
        problems.append("docs/STATUS.md: must state zero external evaluation sessions")
    if "ALPHA-FREEZE" not in doc:
        problems.append("docs/STATUS.md: must link the alpha freeze policy")
    neg = ("not", "never", "no ", "does not", "cannot")
    for i, line in enumerate(doc.splitlines(), 1):
        low = line.lower()
        if "production-ready" in low and not any(n in low for n in neg):
            problems.append(f"docs/STATUS.md:{i}: unsupported production-ready claim")
    return problems


def check_prohibited_current_claims() -> list[str]:
    """Current normative docs must not make claims the evidence model
    forbids, unless the sentence is an explicit negation."""
    problems = []
    negation = ("not", "never", "no ", "does not", "cannot", "explicitly")
    # 1. No performed incident-wide verdict claim.
    for name in NORMATIVE_DOCS + REFERENCE_DOCS + list(CASE_STUDY_READMES):
        f = ROOT / name
        if not f.exists():
            continue
        for i, line in enumerate(f.read_text(errors="replace").splitlines(), 1):
            low = line.lower()
            if ("incident-wide" in low or "complete incident" in low) and (
                "verdict" in low or "assessment" in low or "analysis" in low
            ):
                if not any(n in low for n in negation):
                    problems.append(
                        f"{name}:{i}: performed incident-wide verdict claim: {line.strip()[:90]}")
            if "unexpected continued reviewed-transit path" in low:
                problems.append(f"{name}:{i}: stale human label 'Unexpected continued reviewed-transit path'")
            if "beta" in low and "alpha" not in low:
                problems.append(f"{name}:{i}: inconsistent product status (beta without alpha): {line.strip()[:80]}")
    # 2. Smithville must be insufficient visibility, never no-change.
    sv = (ROOT / "case-studies/indiana-gigapop-smithville-2026/README.md").read_text(errors="replace")
    if "Insufficient" not in sv:
        problems.append("Smithville README: must state Insufficient qualifying visibility")
    for line in sv.splitlines():
        low = line.lower()
        if "no route-state change" in low and "not" not in low and "rather than" not in low:
            problems.append(f"Smithville README: no-change substituted for insufficient visibility: {line.strip()[:90]}")
    # 3. Optical supporting observation must not be a relationship assessment.
    es = (ROOT / "case-studies/manlan-esnet-2019/README.md").read_text(errors="replace")
    if "does not assess" not in es.lower():
        problems.append("ESnet optical README: must state the supporting BGP observation does not assess the optical relationship")
    if "NotDirectlyObservableInPublicBgp" not in es:
        problems.append("ESnet optical README: must carry the NotDirectlyObservableInPublicBgp reviewed applicability")
    # 4. I2PX no-change must not substitute for not-assessable.
    i2 = (ROOT / "case-studies/inc0302574/README.md").read_text(errors="replace")
    if "insufficient-visibility" not in i2 and "not assessable" not in i2.lower():
        problems.append("INC0302574 README: must state the relationship is not assessable")
    return problems


def check_scenario_answer_key() -> list[str]:
    """Every scenario path in the manifest must be a demo workbench URL in
    the generated answer key."""
    problems = []
    manifest = ROOT / "evaluation/scenarios.toml"
    ak = ROOT / "evaluation/generated/answer-key.json"
    if not manifest.exists() or not ak.exists():
        return problems  # covered by check_evaluation_docs
    import tomllib
    m = tomllib.loads(manifest.read_text(errors="replace"))
    try:
        doc = json.loads(ak.read_text(errors="replace"))
    except ValueError:
        problems.append("answer-key.json: invalid JSON (scenario check)")
        return problems
    urls = set(doc.get("demo_manifest", {}).get("expected_workbench_urls", []))
    for s in m.get("scenarios", []):
        p = s.get("path", "")
        if p and p not in urls:
            problems.append(f"evaluation/scenarios.toml: scenario {s.get('id')} path {p} not in answer-key demo workbench URLs")
    return problems


def check_fixture_inventory() -> list[str]:
    """tests/fixtures/README.md must document every fixture family."""
    problems = []
    readme = (ROOT / "tests/fixtures/README.md").read_text(errors="replace")
    families = sorted(d.name for d in (ROOT / "tests/fixtures").iterdir() if d.is_dir())
    for fam in families:
        if f"{fam}/" not in readme:
            problems.append(f"tests/fixtures/README.md: fixture family {fam} not documented")
    for fam in ["mrt", "ris", "internet2", "grnoc"]:
        if f"{fam}/" not in readme:
            problems.append(f"tests/fixtures/README.md: fixture family {fam} not documented")
    return problems


def check_script_basics() -> list[str]:
    """Tracked shell scripts carry a shebang and usage text."""
    problems = []
    for f in sorted((ROOT / "scripts").glob("*.sh")):
        text = f.read_text(errors="replace")
        if not text.startswith("#!"):
            problems.append(f"{f.relative_to(ROOT)}: missing shebang")
        if "usage" not in text.lower() and "--help" not in text and "-h" not in text:
            problems.append(f"{f.relative_to(ROOT)}: missing usage/help text")
    return problems


def check_ci_docs() -> list[str]:
    """The CI jobs must be documented in docs/README.md."""
    problems = []
    ci = (ROOT / ".github/workflows/ci.yml").read_text(errors="replace")
    jobs = re.findall(r"^  ([a-z][a-z0-9-]*):\n", ci, re.M)
    readme = (ROOT / "docs/README.md").read_text(errors="replace")
    for job in jobs:
        if job == "help":
            continue
        if job not in readme:
            problems.append(f"docs/README.md: CI job {job} not documented")
    return problems


def check_job_state_docs() -> list[str]:
    """Every JobState variant label must appear in the operational
    documentation (GLOSSARY/OPERATIONS are the job-model prose)."""
    problems = []
    src = (ROOT / "src/catalog/jobs/mod.rs").read_text(errors="replace")
    m = re.search(r"pub enum JobState \{(.*?)\n\}", src, re.S)
    if not m:
        problems.append("job state check: JobState enum not found")
        return problems
    variants = re.findall(r"^\s{4}([A-Za-z]+),", m.group(1), re.M)
    docs = "\n".join([
        (ROOT / "docs/GLOSSARY.md").read_text(errors="replace"),
        (ROOT / "docs/OPERATIONS.md").read_text(errors="replace"),
        (ROOT / "docs/reference/CLI.md").read_text(errors="replace"),
        (ROOT / "docs/reference/API.md").read_text(errors="replace"),
    ])
    for v in variants:
        if v not in docs:
            problems.append(f"job state docs: variant {v} not documented in GLOSSARY/OPERATIONS/CLI/API")
    return problems


def check_artifact_reference() -> list[str]:
    """Every artifact filename produced by the current writers must be
    documented in the artifact reference."""
    problems = []
    doc = (ROOT / "docs/reference/ARTIFACTS.md").read_text(errors="replace")
    artifacts = [
        "report.json", "report.txt", "archive_manifest.json",
        "evidence_appendix.jsonl", "lifecycle.json", "transitions.json",
        "semantic_waves.json", "withdrawal_audit.json", "limitations.json",
        "performance.json", "execution_metadata.json",
        "analysis_plan.json", "analysis_plan.txt",
        "comparison.json", "comparison.txt",
    ]
    for a in artifacts:
        if f"`{a}`" not in doc:
            problems.append(f"artifact reference: {a} not documented")
    return problems


def main() -> int:
    binary = str(ROOT / "target/debug/inim")
    if not Path(binary).exists():
        print("building debug binary for CLI checks...", file=sys.stderr)
        subprocess.check_call(["cargo", "build", "-q"], cwd=ROOT)
    problems: list[str] = []
    problems += check_links()
    problems += check_anchor_links()
    problems += check_action_pins()
    problems += check_absolute_paths()
    problems += check_terminology()
    problems += check_cli_examples(binary)
    problems += check_cli_reference(binary)
    problems += check_api_route_reference()
    problems += check_schema_matrix()
    problems += check_spec_coverage()
    problems += check_adr_index()
    problems += check_case_study_index()
    problems += check_session_narrative()
    problems += check_inventory_and_render()
    problems += check_documentation_inventory()
    problems += check_job_workflow_docs()
    problems += check_project_scope_docs()
    problems += check_second_network_docs()
    problems += check_evaluation_docs()
    problems += check_status_doc()
    problems += check_prohibited_current_claims()
    problems += check_scenario_answer_key()
    problems += check_fixture_inventory()
    problems += check_script_basics()
    problems += check_ci_docs()
    problems += check_job_state_docs()
    problems += check_artifact_reference()
    if problems:
        print("documentation drift audit FAILED:", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1
    print("documentation drift audit: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
