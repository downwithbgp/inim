# NOC alpha evaluation — invitation draft

Draft text for the project owner to send manually to a prospective
evaluator. Do not send it automatically; do not insert a specific
evaluator or employer.

---

Subject: inim public alpha — 20–30 minute routing-tool evaluation

Hello,

I am preparing an early public alpha evaluation of **inim**, a local
event-conditioned BGP analysis workbench. inim takes operator-declared
network events and shows what selected RouteViews and RIPE RIS observer
sessions saw for the affected prefixes — nothing more.

I would like ~20–30 minutes of your time to try it and tell me where it
misleads you.

What the session involves:

- You inspect preloaded public-BGP incident examples in a local,
  read-only demo — no private network access is required.
- You answer a short task sheet (identifying routes, timestamps,
  prefixes, and what the evidence can and cannot show).
- No telemetry is collected; nothing leaves your machine unless you
  choose to file a public GitHub issue afterwards.
- The goal is to find incorrect or misleading interpretation — not to
  compliment the design. Please be direct.

Expected technical background: familiarity with prefixes, AS numbers,
AS paths, BGP peers, and route withdrawals. No knowledge of inim, MRT
archives, or the repository history is needed.

A note on scope: the tool observes **public BGP only**, from specific
observer sessions. It does not measure traffic, and it cannot see
inside your network. Part of the evaluation is testing whether that
limitation is clear.

If you are interested, I will send setup instructions (a single
bootstrap command) and a link to the task sheet.

Repository: <repository link placeholder>
Scheduling: <scheduling placeholder>

Thank you for considering it.

---

Avoid in any version of this text: revolutionary, comprehensive,
AI-powered, production-ready, industry-leading, complete incident
analysis.
