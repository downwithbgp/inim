## Change category

- [ ] Correctness / semantic fix
- [ ] Evidence or provenance fix
- [ ] Security fix
- [ ] Accessibility fix
- [ ] Documentation
- [ ] Evaluation kit
- [ ] Other (describe)

## Alpha-freeze check

Feature expansion is paused during the external alpha evaluation
(`docs/evaluation/ALPHA-FREEZE.md`). If this change adds a product
capability rather than fixing a defect:

- [ ] This is a correctness/security/accessibility/evaluator-blocking
      fix permitted by the freeze
- [ ] This is a freeze exception requested by the project owner

## Evidence

- What evidence or evaluator feedback supports this change (task ID,
  evidence reference)? Screenshots alone are not semantic proof.

## Impact

- Semantic impact (does evidence interpretation change?):
- Canonical-artifact impact (does any immutable artifact change?):
- Project-scope impact (excluded material affected?):
- Network-access impact (does any code path contact a live source?):
- CI: no live-network tests are added (CI is fully offline):

## Tests

- [ ] Quality gates pass (`cargo test`, `cargo clippy -- -D warnings`,
      `scripts/audit-docs.sh`)
