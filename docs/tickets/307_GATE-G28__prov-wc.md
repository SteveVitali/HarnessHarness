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
# GATE-G28 — PROV-Wc: authorize the one contingency window, or record it unused

- **Sequence:** 307 of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** Wc · **Alias:** GATE PROV-Wc
- **base_branch:** current checkout
- **Depends on:** R6.44
- **Readout:** `docs/build/readouts/GATE-G28.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10), gate table:

> | PROV-Wc (at most once) | first | the triggering event: a W1 machinery repair (W1′), a gate-A L1 failure (W3′) or a restart overflow | compute quote (planning, bound, ceiling with restart, h_r and egress terms) within Wc's remaining reservation (at most $1.94, less any W1 export overrun taken from it); which of W1′, W3′ or restart overflow; D, restart and h_r; the repair's scope and its new participant or runner version record (REQ-XC-330); $0 model; Σ ≤ $20 recomputed (REQ-XH-118) |

§10.1.4, Wc row (verbatim):

> | Wc | at most one contingency window: a repair re-proof (W1′), an L1 rerun on gate-B (W3′), or a harvest-restart overflow; D = 4 h gives about 1.75 h of trial time after overhead, load and the admission cut-off (≈ 42 oracle trials at C = 4), and a larger repair is not runnable in the first pass, a scope decision PROV-Wc records | $0 | its own PROV-Wc gate | if not authorized by FP-E, its reservation is released to W4 except one restart-overflow reservation (REQ-XH-126) |

Cited requirements, verbatim:

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

> | **REQ-XH-118** | First-pass compute SHALL use the default shape or a smaller declared shape that admits C ≥ 4 trials in flight under REQ-XB-214 and gives W_p ≤ D − the REQ-XB-128 admission cut-off for every window, with W_p including the declared retry allowance (REQ-XH-016) and the load of carried artifacts (REQ-XC-330); SHALL tear each VM down across every operator-gate wait not bounded inside its D; and SHALL keep Σ of the windows' ceilings — r·(D + restart) + residual disk at the declared h_r + egress estimate, including one contingency window — ≤ $20 (Q-X54). The W1 quote SHALL list the observe pass, the Rust and admission builds, the layer export, the census and the smoke battery explicitly, and the W3 quote the H-set build and export. PROV-W1 SHALL refuse a W1 quote whose layer-export size estimate exceeds W1's declared egress line. If W1's measured export exceeds that line, the overrun SHALL be taken from Wc's D at the next compute gate; if Wc cannot absorb it, that gate refuses until a recorded scope decision names what shrinks (Wc first, then W4's arms in REQ-XH-126's order). [Synthesis-allocated from Q-X47; revised O2 per Q-X54, RA-11, RA-25, RB-5; O3 per RD-1, RD-2, RD-13.] | Declarations and quotes: Σ ceiling_micros ≤ 20 000 000 with every term present; C ≥ 4 and W_p ≤ D − cut-off in each declaration, with the retry allowance and load terms present; an export-size estimate above W1's egress line refuses at PROV-W1; receipts show teardown before each gate wait. · `AC-10.1-9` · E |

> | **REQ-XB-224** | Each window's gate SHALL pre-authorize one harvest-and-teardown-only restart of a dead-man-stopped VM (≤ 1 h, no slot admission, no model key delivery), quoted inside the window's ceiling; any other restart needs a new authorization (REQ-XB-209). The residual-disk hours h_r of each window SHALL be declared with a maximum (first pass: 6 h), which is also the teardown deadline after a stop or the window's end. [Synthesis-allocated, O2 RA-9, RA-10, RB-5; Q-X60 pending.] | Shim test: a restart-only declaration refuses slot admission and secret delivery; the quote carries the restart and h_r terms; a teardown receipt later than h_r opens a dated `DF-*` row. · `AC-3.9-18` · E |

## Pre-registered thresholds

1. **At most once** in the pass. The trigger is one of: a W1 machinery repair (W1′), a gate-A L1 failure (W3′, an L1 rerun on gate-B), or a harvest-restart overflow — named in the readout with its triggering record.
2. **Quote** (planning, bound, ceiling with restart, h_r and egress terms) **within Wc's remaining reservation**: at most $1.94, less any W1 export overrun already taken from it (REQ-XH-118); D, restart and h_r stated; $0 model (no `deliver-secret`).
3. **Σ ≤ $20 recomputed** over every window's ceiling, completed windows at billed figures where a reconcile read exists and at ceilings otherwise (REQ-XH-118).
4. **Repair scope and version record:** the repair's scope and its new participant or runner version record (REQ-XC-330 — a rebuilt carried artifact is a new version and needs this gate). A W3′ rerun cites REQ-XD-106's allowlist version bump (a scoped-insert code row before this gate) and the unused gate-B partition.
5. **Runnable scope:** a repair larger than Wc's ≈ 1.75 h of trial time is not runnable in the first pass; that scope decision is recorded here, never silently trimmed.
6. **Previous window closed:** W3's harvest verified and its index `archive_verified` (or `n/a{w3_not_run}`) cited (REQ-XH-102).

**Pre-registered dispositions** (each recorded `PASSED — …; basis: <verbatim>`, GATE-G15 precedent; the basis never contains the tokens `FAILED` or `NOT PASSABLE`):

- `triggered{W1′|W3′|restart}` — items 1–6 met and authorized; R6.45 is fleshed for the Wc body.
- `PASSED — not triggered; Wc unused` — no triggering record exists, or a triggering record exists and the operator records verbatim that Wc is not authorized for it (the basis carries which; SR-1 fold precedent); R6.45 is fleshed in place to `unused{not_triggered}` with `n/a{wc_unused}`; HUMAN-H18's Wc entry is `N/A`; at FP-E Wc's reservation is released except one restart-overflow reservation (REQ-XH-126(b)).

Only an operator refusal stops the chain.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to the triggering repair or W3's
closure remains (DEFERRALS rule 4); never silently passed.

## Disposition

Record the operator's exact words, UTC and channel, the disposition and basis verbatim, the
authorization (when triggered) and confirmation of any agent-drafted block's sha256; append the
readout Signature and the matching LEDGER GATE DECISIONS row; a scoped `decompose-spec` insert then
fleshes R6.45 per the disposition (X-03).

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) XO-01…XO-06 and the
manifest operating rules; X-02, X-03, X-07. No code branch is created by a gate.

## Notes

- Owns no REQ id; contributes the readout leg of REQ-XH-004 (owner R6.1).
- Placement (plan tradeoff 5): the block sits between W3 and G-L1 for its likeliest trigger (W3′). A W1′ repair (before W2) or a post-W4 restart overflow moves the block by a scoped insert, never by re-presenting this gate in place.
- REQ-XH-118 also allows a W1 export overrun Wc cannot absorb to make "that gate refuse until a recorded scope decision names what shrinks" — if that case arises it is presented here as a recorded scope decision, not decided in advance.

> Consistency fix 2026-11-07: a gate-A L1 failure record is a W3′ trigger and is always presented here as `triggered{W3′}` for authorization; if Wc is not authorized (or already served another trigger), GATE-G29 records `fallback` with that basis (its pre-registered dispositions). (item 14)
