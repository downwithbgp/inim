# NOC alpha evaluation — protocol

This protocol supports a 20–30 minute evaluation of the inim alpha
workbench by a network engineer. The evaluator does not need session
history, the repository changelog, or the implementation. The goal is
to learn what a working NOC engineer can and cannot determine from the
workbench, and what wording misleads.

There is no telemetry, no analytics, and nothing leaves the machine.

## Setup (prepared for the evaluator)

```sh
git clone https://github.com/downwithbgp/inim.git
cd inim
cargo build --release
./target/release/inim demo init --db ./inim-demo.sqlite --root .
./target/release/inim serve --db ./inim-demo.sqlite --root .
```

Open:

- Dashboard: http://127.0.0.1:8080/
- Workbench (UVA event): http://127.0.0.1:8080/events/INC0299001/workbench
- Workbench (I2PX event): http://127.0.0.1:8080/events/INC0302574/workbench
- Case study (MAN LAN): http://127.0.0.1:8080/case-studies/manlan-2019/workbench

The demo catalog is built entirely from reviewed tracked material; no
network access occurs.

## Tasks

Answer each task from the workbench pages above. Record the page you
used and the time it took. Do not search the repository source.

1. **Identify the first externally observed route change** for the UVA
   event — the exact first meaningful time, the observer, and the
   collector site.
2. **Identify the affected prefixes** — which prefixes changed, and how
   many.
3. **Compare the before and after AS paths** for one affected prefix —
   what exactly changed in the path.
4. **State whether visibility returned** for that prefix, and when.
5. **State whether the event-baseline route returned** (exact baseline)
   — and if not, what the final state is.
6. **Identify one observer that saw a different signature** than the
   others, and describe the difference.
7. **Identify one visibility limitation** — something the evidence
   could not show (absent session, incomplete archive, no qualifying
   baseline, single observer).
8. **Find the evidence reference for one route transition** — the
   archive and identity behind one transition record.

## What to record for each answer

- your answer
- the page used
- time to answer
- your uncertainty (high/medium/low)

## Questions after the tasks

- What could you not determine from the workbench?
- Which term or label was unclear?
- Which result did you distrust, and why?
- What evidence would you need next to confirm or refute one finding?
- What would you check next in your own network?
- What information was missing?
- What information was unnecessary?

Do not answer "do you like the design" questions; the point is task
completion, not preference.
