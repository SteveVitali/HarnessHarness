<!--
  Round-6 contract — experiments first pass (decompose-spec mode=extend, 2026-11-07).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator under Q-X20).
  Hand-off: docs/experiments-decomposition-prompt.md. Partition + coverage:
  docs/build/planning/2026-11-07_round6-{decomposition,coverage}.md.
  Insertion authorized by the operator (planning conversation, 2026-11-07): "I want you to
  continue on until you're totally done, the new tickets are inserted, there's a new PR with
  the changes". This contract CITES the spec; it does not copy a design. Executed contracts
  are frozen: amend by appending a dated note.
-->
# GATE-G21 — Site identity mock review: approve "Instrument" or redirect

- **Sequence:** 257 of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** SITE · **Alias:** site identity mock review (GATE mock)
- **base_branch:** current checkout
- **Depends on:** R6.4
- **Readout:** `docs/build/readouts/GATE-G21.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite is blockedOn. Agents never sign or infer approval; whether the site is beautiful is the operator's judgement alone (Q-X43, Q-X9).

## Criterion (verbatim from the spec)

§10.3, "Gates and HUMAN rows" table:

> | Site identity mock review | first | F2-a | the rendered mock in both themes at the four viewports; `test_tokens.py` record; approve "Instrument" or redirect (Q-X43) |

§9.1, "Mock-review gate":

> Whether the result is beautiful is unestablished until the operator views a rendered mock at the mock-review gate after F2-a, in both themes at F2 F13's viewports (Q-X43 identity). Redirection toward F1's "Ledger" or "Title block" directions remains open there.

REQ-XF-101 (last sentence): "Approval is reserved to the operator at the mock-review gate (Q-X43)." REQ-XF-107's acceptance names "mock-gate review"; REQ-XF-137's names "HUMAN viewport review at the mock gate". Q-X43 (Appendix B): "pending — default: identity at the mock gate; …".

## Pre-registered thresholds

**Technical prerequisites (blockedOn, not a disposition, if unmet):** R6.4 landed; its `test_tokens.py` passes (every text/status token ≥ 4.5:1 and every meaningful non-text token ≥ 3:1, panel and paper, both themes — REQ-XF-102); two mock builds give the same tree sha256.

**What is shown — exactly this, and the readout records each item with its source:**

1. The mock tree built by R6.4's mock command at the R6.4 merge-candidate commit, identified by that commit and the tree sha256 the run record states. The operator views the same bytes; a rebuilt tree with a different digest is a new presentation.
2. Pages: the home page on phase-1 fixtures; the fixture specimen page (every REQ-XF-101 element: needle readouts, noise-floor band, disposition stamps — all eight glyphs —, receipt rail, calibration block, graph-paper ground; type scale, space scale, card, chip and table specs; the series palette in its adjacent-pair form); and the all-absent site (every §9.2 path, including `/method/`, in its designed absent state).
3. Both themes, light and dark, plus the toggle and the no-JS fallback.
4. The four viewports of design/39 F13: 1440×900, 1280×800, 390×844, 320×640 (reflow at 320 CSS px per REQ-XF-137).
5. The glyph: `favicon.svg`, `favicon.png` and the 1200×630 OG image; the wordmark; the harness monograms from `harnesses.json`.
6. The `test_tokens.py` record (the ratio table for both themes) and the palette validator record.
7. The originality check of REQ-XF-107: the tokens, typefaces and motifs set beside HF house style and Artificial Analysis assets (`docs/research/29_reference-sites.md` F4, F7) — for the operator's judgement.

The operator may view in Chrome or any browser; screenshots are never committed as site assets (design/39 F13). The agent drafts the presentation text only inside a labelled agent-drafted block with its sha256.

**Decision content.** Approve "Instrument — the calibrated readout" as the site identity, or redirect: toward F1's I-A "Ledger" or I-C "Title block" directions (`docs/research/29_reference-sites.md` F12), or toward a named revision of "Instrument".

**Pre-registered dispositions (none pre-answered):**
- **approve** → `Status: PASSED`, the operator's words verbatim (including any notes). R6.5 may then dispatch.
- **redirect / revise / reject** → `Status: NOT-PASSABLE` with the operator's words as "what would pass it". **This returns R6.4 for revision:** the coordinator records the direction and adds a scoped `decompose-spec mode=extend` insert (a suffix-letter revision row after R6.4, carrying the direction verbatim; R6.4's executed contract is amended only by a dated note). The revised mock is then re-presented here as a new presentation (new commit and tree sha256). R6.5 and every later site row stay undispatched until this gate is PASSED.

No identity outcome is assumed by any later contract; the site rows cite whichever identity the readout approves.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to the site identity (R6.4) remains (DEFERRALS rule 4). No OPEN scoped technical deferral is silently passed.

## Disposition

Recorded only from the operator's own words, never ticked or signed by an agent: the readout appends a Signature block with the decision verbatim, the UTC time, the channel, the commit and tree sha256 viewed, the viewports and themes viewed, and explicit confirmation of the agent-draft hash; its `Status:` line flips to PASSED or NOT-PASSABLE (+ what would pass it). The same words go in `docs/build/LEDGER.md` GATE DECISIONS (`kind: decision`). Disposition basis text never contains the tokens `FAILED` or `NOT PASSABLE` when the outcome is PASSED (the guard reads them as non-PASSED; R6.1's templates).

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) XO-01…XO-06 (X-03: pending is RETURN PASS) and the [manifest operating rules](00_MANIFEST.md#operating-rules). No code branch is created by a gate. Nothing is published by this gate.
