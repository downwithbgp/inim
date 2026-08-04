# Second-network alpha handoff — 2026-08-04

A compact handoff for another network engineer evaluating inim against a
second managed network (Indiana GigaPOP) without session history. The
offline demo carries the reviewed Internet2 cases; the Smithville
peer-event review concluded the named relationship is not assessable
through the selected public collectors, so no second-network demo case
exists yet — the second-network questions below are the deferred
evaluation tasks.

## Prerequisites

- Rust toolchain (stable), git, ~10 minutes build time.
- No network access required after cloning (the demo is offline).

## Steps

```sh
git clone https://github.com/downwithbgp/inim.git
cd inim
cargo build --release --locked
./target/release/inim demo init --db inim-demo.sqlite --root . --force
./target/release/inim demo verify --db inim-demo.sqlite --root .
./target/release/inim serve --db inim-demo.sqlite --root . --bind 127.0.0.1:8080
```

Read-only server (no writes; writes are disabled by default). URLs:

- `/` — dashboard (no severity score)
- `/events` — event list
- `/events/INC0302574/workbench` — a no-change supporting-plane workbench
- `/events/INC0299001/workbench` — a partial-impact workbench
- `/case-studies/manlan-2019/workbench` — the NORDUnet multi-observer pilot
- `/events/INC0040293` — an optical participant event (not directly
  observable in public BGP; supporting observation only)
- `/analysis-queue` — candidate readiness (excluded records omitted)
- `/case-studies` — case-study index

## Scope-policy checks

```sh
./target/release/inim project-scope show --root .
./target/release/inim project-scope audit --db inim-demo.sqlite --root .
```

The demo contains no excluded record; `demo verify` fails if one is
present.

## Evaluation tasks (approximately 20–30 minutes)

General:

1. Identify the named managed-network relationship for each case study.
2. Identify whether the evidence is direct or indirect (reviewed
   AS-path adjacency) for each workbench.
3. Identify the target prefixes and the first route-state change.
4. Identify the final state and the restoration class.
5. Identify what inim cannot conclude about a target's total
   connectivity (no global single-homing claim).

Second-network (deferred until a tracked case exists):

- Identify the named Indiana GigaPOP–Smithville peer relationship and
  why the selected public collectors cannot observe it (no direct
  AS19782 session; no AS11550 path traverses AS19782; IPv6 absent).
- Ask: what would you check next on internal network equipment to
  observe the relationship?

For every task record: what did you conclude, which page supported it,
what was unclear, which statement seemed stronger than the evidence,
what would you check next. No telemetry, no analytics.
