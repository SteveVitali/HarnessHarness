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
# GATE-G22 — Phase-1 publication: pin the phase-1 input set, labels and the `[results-site]` transition

- **Sequence:** 286 of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** PUB · **Alias:** GATE pub-P1
- **base_branch:** current checkout
- **Depends on:** HUMAN-H12
- **Readout:** `docs/build/readouts/GATE-G22.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite is blockedOn. Agents never sign or infer approval. **Nothing is public before this gate passes**: until then no `hh-site-publication/1` record exists for any phase and `pages.yml` deploys nothing.

## Criterion (verbatim from the spec)

§10.3, "Gates and HUMAN rows" table:

> | Phase-1 publication | first | F2-g + HUMAN Pages | `hh-site-publication/1` pinning the phase-1 input set; labels; `[results-site]` transition |

§9.6: "The publication gate governs the site as a claims surface. … CI always builds and checks an internal profile; deployment uses the public profile, gated by publication records." REQ-XF-129: "A phase's pages SHALL appear in the public profile only when a committed, append-only `hh-site-publication/1` record pins that phase's exact input set; the record references the operator's verbatim decision in a gate readout." REQ-XH-018: "Each phase that publishes SHALL have a publication GATE, an `hh-site-publication/1` record and, where transcripts exist, a separate HUMAN dataset-visibility flip after the secret and contamination scans." §9.4: "Word-bounded matching cannot catch paraphrase, so a human publication review remains necessary." Card register (§9.6): `[results-site]` — "Not yet available — publication gate pending" until the first publication record, then "Available — internal record; not outside-verified" (Q-X43 recorded default); cap "same".

## Pre-registered thresholds

**Technical prerequisites (blockedOn if unmet, not a disposition):** HUMAN-H12's readout records the Pages source as GitHub Actions (a `DEFERRED` H12 does not satisfy this gate); R6.6 landed; at the presented commit `check-site.sh --profile public` and `check-site.sh --head` are green, site-data has no drift, and the publication-gate checklist (R6.6) is complete.

**What is presented — the readout records each item with its source:**

1. The draft `hh-site-publication/1` content for phase 1 (design/39 F10 fields): `phase`, `pages[]`, every `inputs[{path, sha256}]` of the phase-1 input set as `inputs.json` names it at the presented commit, and `inputs_digest`; `labels{evidence_class, outside_verified: false}`; the gate row and readout it will reference. R6.31 commits exactly this content after the gate; a different input set is a new presentation.
2. The public-profile build at that commit (tree sha256), for review in both themes at 1440×900, 1280×800, 390×844 and 320×640 (design/39 F13); phases 2–4 render absent with no numbers.
3. The labels: every page header's REQ-XD-092 statements, card state and evidence class, and "internal record; not outside-verified" (REQ-XF-116).
4. The `[results-site]` transition text above, and that no card exceeds its cap (REQ-XH-019).
5. The publication-gate checklist: REQ-XC-014 (HF figures labelled a different serving setup, never a replication target or HH result), REQ-XC-120 (`catalog_declared` only, with its stated limits), REQ-XD-003 (no extrapolation beyond executed bands), REQ-XD-092 labels, REQ-XF-142 public-profile refusals — plus the operator's own reading for paraphrase (§9.4).
6. Transcripts: phase 1 has no model transcripts (design/39 F11), so no dataset flip is part of this phase; REQ-XH-105's transcript-option and terms-read records belong to the first-pass publication gate (GATE-G31).

**Decision content.** Publish phase 1 with exactly the presented input set and labels, or not.

**Pre-registered dispositions (none pre-answered):**
- **approve** → `Status: PASSED` with the operator's words verbatim, naming the approved `inputs_digest`. This authorizes only R6.31: committing that record, the README site link and the `[results-site]` transition, and the deploy that follows the operator's merge to `main`.
- **`PASSED — publication withheld`** → `Status: PASSED` with the operator's words verbatim (R6.1's PASSED-with-disposition template; GATE-G16 precedent): no public act — no `hh-site-publication/1` record, no README site link, no `[results-site]` transition, no deploy; the site stays unpublished (internal profile only). R6.31 then lands the no-publication state. Declining to publish never stops the engineering chain.
- **refuse** → `Status: NOT-PASSABLE` with what would pass it (a technical or content defect the operator names); a refusal stops the chain. Declining to publish is the `publication withheld` disposition above, not a refusal.

This gate authorizes no spend, no dataset visibility change, no transcript publication and no other phase's pages.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to the site or phase-1 publication (R6.4–R6.6, R6.20, R6.21, HUMAN-H12) remains (DEFERRALS rule 4). No OPEN scoped technical deferral is silently passed.

## Disposition

Recorded only from the operator's own words, never ticked or signed by an agent: the readout appends a Signature block with the decision verbatim, the UTC time, the channel, the commit, tree sha256 and `inputs_digest` reviewed, and explicit confirmation of the agent-draft hash; its `Status:` line flips to PASSED or NOT-PASSABLE (+ what would pass it). The same words go in `docs/build/LEDGER.md` GATE DECISIONS (`kind: decision`). A PASSED basis never contains the tokens `FAILED` or `NOT PASSABLE` (R6.1's templates).

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) XO-01…XO-06 (X-03, X-08) and the [manifest operating rules](00_MANIFEST.md#operating-rules). No code branch is created by a gate; no gate cell writes the publication record (R6.31 does).

## Notes

> Consistency fix 2026-11-07: added the pre-registered `PASSED — publication withheld` disposition so that declining phase-1 publication is a non-stopping outcome; R6.31 records it as the Appendix D gate outcome `declined` (no `withheld{…}` member is minted). The plan's SR-1 table, which lists no alternative for this gate, needs the matching plan-sync note. (item 12)
