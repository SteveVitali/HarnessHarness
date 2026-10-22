<!-- Round-4 launch re-charter insert (decompose-spec mode=extend amendment, 2026-10-20). A GATE marker
     (BM-TICKET-05, BM-GATE-03): no run line; executed by orchestrate-build; never guessed past; never pre-answered. -->
# GATE-G7 — Round-4 rulings gate ("GATE-R4R")

- **Kind:** gate · **Phase:** Round 4 · **Sequence:** 164a (round-4 row 32 of 50)
- **Readout:** `docs/build/readouts/GATE-G7.md` (from `templates/READOUT.md`)
- **Alias:** the operator's 2026-10-20 "Mid-round rulings gate" — recorded as GATE-R4R in GATE DECISIONS; the marker id is
  `GATE-G7` because the build-memory id grammar admits only `GATE-G<n>` gate markers.

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator dispositions the readout.
> Never guessed past. An operator record supplies every decision; an agent must not sign or assume silence is approval.
> Amended 2026-10-20 (R4.2a, ADR-0362): the gate's CI evidence is the D4 `full-run:`
> line in its PHASE LOG entry (the `workflow_dispatch` full run on the chain
> tip, read by `gh run view <id> --json jobs,conclusion` until L4.3 lands
> `scripts/ci/full-run.sh`).

## Criterion (the operator's ruling, verbatim)

"Mid-round rulings gate" (Devin CLI structured question, 2026-10-20) — recorded consequence: window-package + BL-30 briefs
run early; this gate stops the chain for the operator's adopt / amend / decline rows; conform tickets (skeletons until
then) land the adopted set before closure; GATE-G6 no longer carries ruling conforms.

## Pre-registered thresholds (fixed before the briefs run)
- Brief files exist for every package: `docs/build/planning/round4-{bl33-ws-l6,bl34-adr-0211,bl35-bl39,bl36-adr-0213,bl37-adr-0214,bl38-adr-0215}-briefs.md`
  and `round4-bl30-adjudication-briefs.md`; each opens with the batch-adoptable table and the "adopt all except" line.
- Every in-scope OQ / CF / D-n / §10 minor appears exactly once with a FINAL proposed disposition (resolve-with-evidence ·
  adopt-interim-as-final · descope-by-ADR · needs-code-leg(<row>)) — zero keep-open / keep-interim / defer proposals.
- The OQ-170 custody proposal is concrete; the OQ-388 ratification text is drafted; the R4.8 trust-field section exists.
- The readout lists the proposals by package with counts per disposition token (derived from the files, not pinned here).

## What the gate produces
The operator's GATE DECISIONS rows (`kind: decision`, words verbatim — adopt / amend / decline per id or "adopt all
except …"). Then the orchestrator runs a **scoped `decompose-spec mode=extend`** that fleshes skeletons R4.15a–R4.15e from
the adopted set (splitting further by the fleshing notes if any exceeds the ceiling), each with its Phase-4 fresh-context
review. A **declined** proposal needs a replacement disposition from the operator at the same gate — the round's charter
admits no carried open item.

## DEFERRALS rule
This gate does not pass while any brief-scoped id lacks an operator disposition (DEFERRALS rule 4 analogue).

## Disposition
- [ ] Operator disposition recorded in `docs/build/LEDGER.md` GATE DECISIONS and `docs/build/readouts/GATE-G7.md` as
      PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE (+ what would pass it).
