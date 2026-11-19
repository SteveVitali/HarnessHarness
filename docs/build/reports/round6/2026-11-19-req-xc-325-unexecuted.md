# Honest unexecuted record — REQ-XC-325 R-replay / SM-* executor (DF-CAP6.1-3 / GAP-03)

2026-11-19 · CAP6.2 · unexecuted work record + waiver request.

## Facts

- REQ-XC-325 requires an R-replay / SM-* executor leg; CAP6.1 verified **no executor exists
  in this repository** (typed finding GAP-03) — the work is honestly `unexecuted{no_executor}`.
- CAP6.2's scope is offline closure machinery ($0, no new runtime implementation); building
  a new R-replay executor is out of scope and cannot be fabricated as done.

## Disposition — deferred with waiver request

- Obligation stands **unmet, never waived by this record**: routed to the later pass in
  `LATER_PASS_ROUTING.md` with owner = later-pass executor build-out, trigger = next pass's
  conformance tooling scope.
- **Waiver request recorded for GATE-G32** (three-part signature list, item 3): the operator
  may accept `unexecuted{out_of_scope_for_round6}` at signature; absent acceptance the leg
  carries verbatim to the later pass. This record neither infers nor asserts the waiver.
