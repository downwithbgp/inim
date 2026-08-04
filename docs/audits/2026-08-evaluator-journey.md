# Evaluator journey audit — 2026-08-04

Dated execution audit of the clean evaluator path: public repository
clone to first evaluation task. Measured in a fresh temporary clone
(`git clone` of the public repository at commit `041fdf8`,
session-51 branch; the post-merge `main` state). No existing
`target/`, demo database, runtime catalog, cache, `out/`, `data/`, or
private environment variables were reused. No absolute temporary paths
appear in this audit.

The journey succeeded, but it is not claimed to be easy merely because
it succeeded; ambiguities are recorded below.

## Steps and measurements

| Step | Command | Result | Duration |
|---|---|---|---|
| 1. Clone | `git clone <public HTTPS URL>` | success | ~1 s (local mirror) |
| 2. Inspect prerequisites | `cargo --version`, `rustc --version` | present (1.97.1) | — |
| 3. Build | `cargo build --release --locked` | success | 3 m 53 s (cold) |
| 4. Initialize demo | `scripts/evaluator-bootstrap.sh --db ./inim-demo.sqlite` | success | 3.2 s (demo init) |
| 5. Verify demo | `inim demo verify` (inside bootstrap) | ok | 0.012 s |
| 6. Start server | `inim serve --db ... --root . --bind 127.0.0.1:8080` | listening | < 1 s |
| 7. Open starting page | `http://127.0.0.1:8080/` | HTTP 200 | 12 ms |
| 8. First workbench | `/case-studies/manlan-2019/workbench` | HTTP 200 | 29 ms |

Commands required: **4** (clone, bootstrap, server, open browser).
Manual decisions required: **1** (accept the default database path and
port). No network access occurred after dependency/build resolution;
`cargo build` may need network on a fresh machine.

## Ambiguities recorded during the journey

1. **How to stop the server.** The server runs in the foreground and
   stops with Ctrl-C; the bootstrap output does not say so. Low impact
   for a 20–30 minute session, but a facilitator may be asked.
2. **Whether the demo needs the top-level `inim-demo.sqlite` path.**
   The bootstrap defaults the database to the repository root; the
   legacy `docs/evaluation/NOC-ALPHA-EVALUATION.md` setup block uses
   `./inim-demo.sqlite` and does not mention the bootstrap script. The
   legacy protocol document is now superseded by the evaluation kit
   (task booklet + bootstrap); its setup block was already accurate
   and remains consistent.
3. **Port conflicts.** If 8080 is occupied the server fails with
   `Address already in use`; the bootstrap accepts `--port` but the
   error message itself does not suggest the flag. The bootstrap usage
   text covers it; a facilitator should pre-check the port.
4. **Read-only confirmation.** The serve banner prints
   `(read-only, no analysis on request path)`; this is the only
   visible confirmation that write mode is off. Adequate, but worth
   verifying once per session.
5. **What cases exist.** The bootstrap prints the scenario URLs, so
   the evaluator does not need to search the event list before the
   first task. Confirmed sufficient for the first task.

## Corrections made as a result of this audit

- The bootstrap now prints the expected scenario URLs from the
  reviewed manifest (previously the evaluator had to find them).
- The provisional snapshot cutoff is rendered on open-event
  workbenches (INC0301970) so the Smithville page does not read as
  final.
- Insufficient visibility with zero eligible sessions no longer
  renders as "no route-state change at 0 of 0".

## Verdict

The journey from clone to first task is **four commands, one manual
decision, no network access after build**, with the first task URL
printed by the bootstrap. The remaining ambiguities (server stop,
port conflict) are facilitator-level and are recorded in the
facilitator guide and the external-pilot checklist.
