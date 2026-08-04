# Evaluation data handling

How evaluator data is handled during the NOC alpha evaluation. Scope is
limited to the evaluation process; this is not a product privacy policy
and claims no legal compliance certifications.

## Statements

- **No telemetry.** The evaluation workflow collects no telemetry.
- **No analytics.** No analytics service is used.
- **No automatic uploads.** Nothing is uploaded automatically; the
  evaluation is fully local.
- **Manual recording.** Evaluator responses are recorded manually on the
  response sheet or session-notes template.
- **No confidential details.** Evaluators are asked not to include
  confidential network information, credentials, private incident
  details, or customer data.
- **Optional issue submission.** Opening a GitHub issue is optional.
- **Public by default.** GitHub issues are public; anything posted there
  is public.
- **Private notes stay private.** Private session notes should remain
  outside the repository unless sanitized and deliberately committed.
- **Optional identity.** Evaluator identity is optional. The response
  sheet asks for an anonymous evaluator code, role category, BGP
  familiarity, and RouteViews/RIS familiarity — nothing personal.
- **Owner controls retention.** The project owner controls note
  retention and deletion.

## What is never collected

- employer secrets
- customer names
- private incident details
- personal address
- unnecessary demographic data
- political affiliation
- health information

## What is stored, and where

| Item | Where | Default |
|---|---|---|
| Evaluator responses | `docs/evaluation/evaluator/NOC-ALPHA-RESPONSE-SHEET.md` (copied per session) | untracked until sanitized |
| Session notes | `docs/evaluation/facilitator/SESSION-NOTES-TEMPLATE.md` (filled per session) | untracked until sanitized |
| Pilot registry row | `docs/evaluation/PILOT-REGISTRY.md` | tracked, no real names |
| Public feedback issues | GitHub issues | public, sanitized |

The application itself never stores evaluator information.
