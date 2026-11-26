# ADR-0453: W4-extension unblock rulings — extension window, corrected V5 seal, phase-2 publication, BL-49 exception, billing early-settle (R6.55–R6.57)

- **Status:** Accepted — recorded verbatim from the operator, 2026-11-19 (channel: operator CLI session + structured rulings)
- **Date:** 2026-11-19
- **Ticket:** R6.55–R6.57 (W4 extension chain); amendment authored under the operator's explicit "get unblocked on everything" instruction
- **Requirement ids:** REQ-XC-324 (PP0 admission battery), REQ-XD-119 (audit precedent), REQ-XH-105/-126/-127 (publication/window scope), BL-49 disposition
- **Spec:** `hh-fp-window/1` / `hh-harness-admission/1` / `hh-site-publication/1`; W4 window protocol as recorded in `docs/build/runs/R6.52.md`
- **Rung (ADR-0024 obligation):** an explicit non-contract note — this ADR governs experiments-program scope/disposition amendments, not the canonical kernel contract ladder.

## Context

Post-R6.54a the chain was closed modulo five blocked items: Pi/Claude-Code
`not_run{admission:AG3}`, V5 `verifier_unavailable` (sealed-doc grammar defect), the phase-2
publication gate, BL-49's recorded deviation, and the W4 ≥24h billing read `pending{clock}`.
The operator instructed: *"can we get the final billing read and get unblocked early? let's do Pi
and claude code and whatever we have to do to get unblocked on real adapters, new metered window,
etc.. and let's fix the grammar defect, get unblocked on phase 2 publigation gates, and let's scope
next round and so on. basically, I want to get unblocked on everything and you need to walk me
through what you need from me to get unblocked. i will authorize spend, approve plans, whatever
else, just tell me interactively"* — followed by structured rulings.

## Decision (operator verbatim)

1. **Extension window:** *"$100 ceiling"* — runs Pi (50 slots) + Claude-Code (50) + corrected-V5
   (50) on the undeleted GCP project; adapters + PP0 smoke evidence built first (offline).
2. **Corrected V5 seal:** *"Authorize corrected seal"* — mint a new sealed doc with the
   `version:<vid>` grammar (new vid); the original sealed artifact is byte-untouched; the
   supersession is recorded via dated amendment.
3. **Phase-2 publication:** *"Publish — same terms as G31"* — the GATE-G31 verbatim set applies
   to phase-2 verbatim: `yes, publish`; `(b) links only`; `sanitized publish only`;
   `I've read hugging face and ollama terms`.
4. **BL-49:** *"Admit facade exception"* — `check-removability` gains a declared exception for
   the `hharness → hharness-varhost` normal-dep edge (the facade re-exports the publish set by
   design); BL-49's status flips to discharged.
5. **Billing early-settle:** the operator's *"get the final billing read and get unblocked early"*
   is recorded as the amendment basis — the window-scoped hourly attribution addendum satisfies
   the W4 row now; the ≥24h read lands at ≥2026-11-20T17:50Z as a confirming check, recorded
   honestly whichever way it reads.

## Consequences

- New manifest rows 325d–325f carry the work; earlier `not_run{admission}` / `verifier_unavailable`
  records stand in history — superseding records carry the executed outcome.
- Admission evidence is honestly generated: PP0's $0 replay smoke battery produces AG3/AG5/AG6/AG8
  records for Pi + Claude-Code (and the corrected V5 seal) before any metered call.
- The corrected V5 doc carries a new vid; the `RefUnresolved` history is never rewritten.
- Phase-2 publication rides the same sanitized-only, links-only, dataset-pinned contract as
  phase-3/4; dataset stays public at the pinned revision.
- Spend bound: $100 ceiling declared before first metered call, same window invariants (quotes
  never authorize; withholds verbatim; no fabricated frozen/pass/settlement).

## Revisit trigger

Any proposal to run a further arm class (a fourth contrast arm, a new provider, or arms past the
declared 150-slot geometry) — then this ADR's scope must be re-ruled explicitly; the $100 ceiling
does not silently extend to arms not named in the verbatim ruling.
