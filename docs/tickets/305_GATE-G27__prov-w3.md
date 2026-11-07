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
# GATE-G27 — PROV-W3: authorize the W3 L1 window or record why it does not run

- **Sequence:** 305 of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** W3 · **Alias:** GATE PROV-W3
- **base_branch:** current checkout
- **Depends on:** R6.43, R6.28, R6.42, R6.41, R6.23
- **Readout:** `docs/build/readouts/GATE-G27.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10), gate table — one row that the plan splits into two gates because the decisions fall on either side of the W3 run (plan §Gate and HUMAN numbering). **This gate carries the PROV-W3 part** (the compute quote, the H-set build and export, the load, the frozen H record named); the `hh-l1-conformance/1` pass-or-fail part is GATE-G29's:

> | PROV-W3 + **G-L1** | first | E2-T10, K4, partition commit | compute quote, listing the H-set build and export and the load; frozen H record named (the H-set digest frozen at G-L1, REQ-XC-330); `hh-l1-conformance/1`: first divergence fails, BM1–BM19 coverage complete, census complete, rule table frozen; pass or fail only |

Cited requirements, verbatim:

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

> | **REQ-XH-118** | First-pass compute SHALL use the default shape or a smaller declared shape that admits C ≥ 4 trials in flight under REQ-XB-214 and gives W_p ≤ D − the REQ-XB-128 admission cut-off for every window, with W_p including the declared retry allowance (REQ-XH-016) and the load of carried artifacts (REQ-XC-330); SHALL tear each VM down across every operator-gate wait not bounded inside its D; and SHALL keep Σ of the windows' ceilings — r·(D + restart) + residual disk at the declared h_r + egress estimate, including one contingency window — ≤ $20 (Q-X54). The W1 quote SHALL list the observe pass, the Rust and admission builds, the layer export, the census and the smoke battery explicitly, and the W3 quote the H-set build and export. PROV-W1 SHALL refuse a W1 quote whose layer-export size estimate exceeds W1's declared egress line. If W1's measured export exceeds that line, the overrun SHALL be taken from Wc's D at the next compute gate; if Wc cannot absorb it, that gate refuses until a recorded scope decision names what shrinks (Wc first, then W4's arms in REQ-XH-126's order). [Synthesis-allocated from Q-X47; revised O2 per Q-X54, RA-11, RA-25, RB-5; O3 per RD-1, RD-2, RD-13.] | Declarations and quotes: Σ ceiling_micros ≤ 20 000 000 with every term present; C ≥ 4 and W_p ≤ D − cut-off in each declaration, with the retry allowance and load terms present; an export-size estimate above W1's egress line refuses at PROV-W1; receipts show teardown before each gate wait. · `AC-10.1-9` · E |

> | **REQ-XC-330** | Each (harness × task image) layer and each bundle SHALL be built at most once per pass and SHALL be exported at the end of its build window (`podman save`, or its harness-specific blobs plus the image config with base blobs re-pulled by digest) into the harvested run root with sha256, indexed in the pass-level `artifacts` section of `hh-receipt-index/1` (REQ-XF-124). Each later window SHALL load it by the §5.2 load path (controller → VM over IAP, `podman load`, base blobs re-pulled by digest), timed and quoted in that window's overhead, and SHALL verify the image ID (config digest), which is the identity the slot records; a registry RepoDigest, which a load drops, is never the identity. HH binaries SHALL be built once per stage in two carried sets, carried the same way. In the first pass: an infrastructure set (runner, helper, relay, `hh-model-proxy`, egress mediator) is built in W1 and carried by digest through W4; an H set (the `hh-kernel` participant build plus the HIR and compiled artifacts, the phase-4 level's included) is built once at the start of W3, exported from W3 with sha256, run by W3's L1 and frozen by digest at G-L1. W4 loads both sets; every W4 slot records the infrastructure-set digest and, for HH-native and V5 slots, the H-set digest. A rebuild of any carried artifact is a new participant or a new runner version and needs a gate. The transfer is an egress line in the exporting window's quote. A bundle or layer containing a proprietary harness (first pass: Claude Code) SHALL NOT be uploaded to the archive unless the operator records that its licence permits redistribution (Q-X26's admission terms do not say so); it is a digest-only index entry, and its bytes stay on the operator controller. [Synthesis-allocated, O2 RA-1; revised O3 per RD-1, RD-13.] | Load-and-verify fixture: a mismatched image ID refuses the slot as `withheld{admission:harness_layer_mismatch}`; a rebuilt-layer fixture refuses without a gate record; W4 slot records carry one infrastructure-set digest and, on HH-native and V5 slots, the H-set digest frozen at G-L1; an archive fixture holding a Claude Code bundle yields a `digest_only` index entry and no uploaded bytes. · `AC-5.2-5` · E |

> | **REQ-XB-224** | Each window's gate SHALL pre-authorize one harvest-and-teardown-only restart of a dead-man-stopped VM (≤ 1 h, no slot admission, no model key delivery), quoted inside the window's ceiling; any other restart needs a new authorization (REQ-XB-209). The residual-disk hours h_r of each window SHALL be declared with a maximum (first pass: 6 h), which is also the teardown deadline after a stop or the window's end. [Synthesis-allocated, O2 RA-9, RA-10, RB-5; Q-X60 pending.] | Shim test: a restart-only declaration refuses slot admission and secret delivery; the quote carries the restart and h_r terms; a teardown receipt later than h_r opens a dated `DF-*` row. · `AC-3.9-18` · E |

> | **REQ-XH-127** | A first-pass PP0 hold SHALL have a declared disposition. A hold is ≥ 1 of 3 PP0 trials infrastructure-failed, or a failed AG3/AG5/AG6/AG8 check. The disposition must be declared because a repair is a new participant version (REQ-XC-330) and no first-pass window offers a paid re-smoke (Wc is $0 model). (a) A held Pi or Claude Code arm is `withheld{admission:AG<n>}` for the pass, naming the failed gate, or `withheld{admission:pp0_infra_hold}` for an infrastructure hold with no failed gate. It drops from FP-E and stays in P2-FP's Holm family as an unavailable member. (b) A held original mini-swe-agent withholds the corpus, so G-L1 cannot pass. FP-E SHALL then present a recorded scope decision whose default ends the pass after W2, with every first-pass claim withheld by the hold's typed reason; a repaired arm is a later pass. (c) A development-task exclusion that empties its difficulty cell SHALL be replaced from the adjacent cell's committed order (easy or hard → medium), re-proofed under REQ-XH-116, and disclosed in the pre-registration. [Synthesis-allocated, O3 RD-14.] | Fixtures: a held Pi arm is absent from the FP-E schedule while its family member is retained as unavailable; a held mini-swe-agent yields the FP-E scope-decision record with the typed withholdings; an empty-cell exclusion draws from the adjacent cell, and the disclosure is present. · `AC-10.1-18` · E |

> | **REQ-XH-123** | The first pass SHALL carry Q-X52's pre-declared fallback: if G-L1 fails on both gate partitions, or has not passed by the trigger recorded at FP-S (Q-X62), W4 SHALL run the three hosted arms alone (phase 2 only), and P3-L2 and P4-FP SHALL be `withheld{l1_not_passed}` and moved to a later pass. The fallback is decided at FP-E from the trigger record, never from an outcome field. [Synthesis-allocated, O2 RB-14; Q-X52.] | Fallback fixture: a trigger record yields a hosted-only W4 schedule and both typed withholdings; the FP-E readout cites the trigger. · `AC-10.1-14` · E |

## Pre-registered thresholds

1. **Authorizations (REQ-XH-004).** W3 is $0 model (§10.1.4 W3 row); compute authorization verbatim and separate; quote id; the §3.10 ceiling formula in µ$.
2. **Quote content (REQ-XH-118).** Planning, bound and ceiling with restart, h_r and egress terms; the W3 quote lists the H-set build and export and the load explicitly; C ≥ 4; W_p ≤ D − cut-off; Σ of every window's ceiling ≤ $20, completed windows at billed figures where a reconcile read exists (HUMAN-H18 entries) and at ceilings otherwise. §10.1.5's W3 row (D = 5.5 h, ceiling $2.41) is a planning input, UNVERIFIED.
3. **Dead-man and restart** (REQ-XB-224): one harvest-and-teardown-only restart ≤ 1 h inside the ceiling; h_r ≤ 6 h; no `deliver-secret` ($0 window).
4. **Frozen H record named** (REQ-XC-330): the H set to be built once at the start of W3 — the `hh-kernel` participant build plus the HIR and compiled artifacts, the phase-4 level's included (R6.43) — and that its digest is frozen at G-L1.
5. **Inputs landed and cited:** the partition commit with digests preceding H development (R6.42; REQ-XD-106, REQ-XH-114); E2-T10 + D3-FP (R6.43); K2–K4 and `hh-l1-conformance/1` (R6.28); E2-T7 + E2-T8 (b) (R6.41); the carried-set kit (R6.23).
6. **Previous window closed:** W2's harvest verified, its index `archive_verified` (REQ-XH-102), its settlement status (REQ-XC-127) and its proxy ledger harvested and verified (REQ-XH-120) — each cited.

**Pre-registered dispositions** (each recorded `PASSED — disposition <token>; basis: <verbatim>`, GATE-G15 precedent; basis text never contains the tokens `FAILED` or `NOT PASSABLE`):

- `run` — items 1–6 met and authorized; R6.44 is fleshed for the W3 body.
- `not_run{fallback}` — a Q-X62 trigger record exists before PROV-W3 is presented (REQ-XH-123); R6.44 is fleshed in place to `unused{…}` with `n/a{w3_not_run}`.
- `not_run{pp0_hold}` — a held original mini-swe-agent withheld the corpus (REQ-XH-127(b)), so G-L1 cannot pass; same in-place fleshing.

Under either `not_run`, HUMAN-H18's W3 entry is `N/A`. Only an operator refusal stops the chain.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to W2's closure, the partition or
the L1 machinery remains (DEFERRALS rule 4); never silently passed.

## Disposition

Record the operator's exact words, UTC and channel, the disposition token and its basis verbatim, the
authorization (for `run`), and confirmation of any agent-drafted block's sha256; append the readout
Signature and the matching LEDGER GATE DECISIONS row. A scoped `decompose-spec` insert then fleshes
R6.44 per the disposition (X-03, Q-X58).

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) XO-01…XO-06 and the
manifest operating rules; X-02, X-03, X-07 (the readout carries cost fields only; no corpus verdict).
No code branch is created by a gate.

## Notes

- Owns no REQ id; contributes the readout leg of REQ-XH-004 (owner R6.1).
- A gate-A-only L1 failure is not decided here: it arises after W3 and routes through GATE-G28 (W3′ on gate-B, with REQ-XD-106's allowlist version bump as a scoped-insert code row before PROV-Wc).
