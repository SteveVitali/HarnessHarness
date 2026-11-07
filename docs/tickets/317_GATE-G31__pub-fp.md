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
# GATE-G31 — First-pass publication decision

- **Sequence:** 317 of 325 · **Phase:** Round 6 · **Kind:** gate
- **base_branch:** current checkout
- **Depends on:** R6.49
- **Readout:** `docs/build/readouts/GATE-G31.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** Pending operator review is RETURN PASS; a failed technical prerequisite is blockedOn. Agents never sign or infer approval. A decision here authorizes publication acts only; it never authorizes spend, merges or the dataset flip itself (HUMAN-H17 is a separate act).

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10) gate table, row "First-pass publication":

> | First-pass publication | first | FP-RESULTS + flagged review | `hh-site-publication/1`; dataset visibility flip as a separate HUMAN act after both scans; terms read (Q-H1-6) |

REQ-XH-105 ([§4.10](../experiments-spec.md#s4)), the gate sentence:

> The publication gate SHALL record verbatim the transcript option (Q-X43: (a) declared in-site fetch, (b) links only), the policy for files with task-derived content, and that the operator has read the Hugging Face and Ollama terms (Q-H1-6).

REQ-XH-018 ([§9.6](../experiments-spec.md#s9)):

> Each phase that publishes SHALL have a publication GATE, an `hh-site-publication/1` record and, where transcripts exist, a separate HUMAN dataset-visibility flip after the secret and contamination scans.

REQ-XF-129 ([§9.6](../experiments-spec.md#s9)):

> A phase's pages SHALL appear in the public profile only when a committed, append-only `hh-site-publication/1` record pins that phase's exact input set; the record references the operator's verbatim decision in a gate readout.

§10.3, after the HUMAN table:

> No note studied Ollama's or Hugging Face's terms for automated benchmarking or for publishing TB-derived transcripts. The first publication gate that includes transcripts records that the operator read them (Q-H1-6).

## Pre-registered thresholds

The readout presents, and the operator decides, exactly this content (never pre-answered):

1. **The input set** — R6.49's agent-drafted `hh-site-publication/1` draft (sha256 stated) pinning the frozen first-pass packets: P2-FP, P3-L1, P3-L2 with audit and L3, P4-FP, or their typed `withheld{…}` forms as R6.48 emitted them; the HUMAN-H16 readout status (a `DEFERRED` review withholds the freeze claim and is presented as such).
2. **The transcript option (Q-X43)** — recorded verbatim: (a) declared in-site fetch, or (b) links only.
3. **The policy for files with task-derived content** — recorded verbatim.
4. **Terms read (Q-H1-6)** — the operator's statement that they have read the Hugging Face and Ollama terms, recorded verbatim.
5. **The dataset flip** — stated as a separate HUMAN act (HUMAN-H17) after both the secret scan and the contamination scan, with the dataset card prerequisites of REQ-XH-105; the readout states which scan references and which dataset-card revision exist at presentation.
6. **Labels and prominence** — every page and packet carries its phase labels, both first-pass labels (REQ-XH-119), the REQ-XH-122 / REQ-XD-126 baseline wording, and withheld/negative/inconclusive outcomes with the same prominence (REQ-XH-020).

No outcome is re-presented more favorably than its packet; no threshold or label changes after data.

## Disposition

Record exact operator words, UTC, channel, the decided scope (which pages, which input set), the three REQ-XH-105 decisions verbatim, expiry/void conditions and explicit confirmation of the agent-draft hash. Append the readout signature and the matching LEDGER GATE DECISIONS row; old acceptance (GATE-G22) never substitutes. A decision not to publish, or to publish a narrower scope, is recorded verbatim using R6.1's gate-disposition templates (Round-5 GATE-G16 "PASSED — out-of-scope disposition" precedent); its basis never contains the tokens `FAILED` or `NOT PASSABLE`. A refusal stops the chain.

## DEFERRALS rule

No OPEN scoped technical deferral is silently passed. Resolve it or obtain an explicit protocol-compliant disposition with owner/trigger and withheld claims.

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) (X-03, X-05, X-08; XO-06) and the manifest operating rules. No code branch is created by a gate.
