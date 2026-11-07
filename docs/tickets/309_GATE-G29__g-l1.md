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
# GATE-G29 — G-L1: record the L1 decision as `full` or `fallback`

- **Sequence:** 309 of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** W3 · **Alias:** GATE G-L1
- **base_branch:** current checkout
- **Depends on:** R6.44, R6.45
- **Readout:** `docs/build/readouts/GATE-G29.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** A decision gate: the chain STOPS here until
> the operator dispositions the readout. Pending operator review is RETURN PASS; a failed technical
> prerequisite is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10), gate table — **this gate carries the G-L1 part** (`hh-l1-conformance/1`: first divergence fails, BM1–BM19 coverage complete, census complete, rule table frozen; pass or fail only); the PROV-W3 part was GATE-G27's:

> | PROV-W3 + **G-L1** | first | E2-T10, K4, partition commit | compute quote, listing the H-set build and export and the load; frozen H record named (the H-set digest frozen at G-L1, REQ-XC-330); `hh-l1-conformance/1`: first divergence fails, BM1–BM19 coverage complete, census complete, rule table frozen; pass or fail only |

§10.1.4 (verbatim):

> **Fallback (Q-X52; REQ-XH-123).** If G-L1 fails on both gate partitions, or has not passed by the trigger recorded at FP-S, FP-E is held in its hosted-only form. W4 then runs original mini-swe-agent, Pi and Claude Code alone (phase 2 only). P3-L2 and P4-FP are `withheld{l1_not_passed}` and move to a later pass, which the committed W2 corpus serves. The trigger's recorded default is the ledger's Q-X62 row, quoted verbatim: "First-pass spend gate (FP-S) waits for the E2-T8 rung record; hosted-only fallback fires if G-L1 has not passed 14 days after FP-S is otherwise ready (14 is a synthesis placeholder)". The ledger row governs (O2 delta RD-6). Three specifics follow from it and are not ledger text:

§7.4 (verbatim):

> **Failure handling.** A fail is resolved by fixing H, or by a version-bumped allowlist rerun on a fresh partition; it is never relabelled.

Cited requirements and decision, verbatim:

> | **REQ-XH-123** | The first pass SHALL carry Q-X52's pre-declared fallback: if G-L1 fails on both gate partitions, or has not passed by the trigger recorded at FP-S (Q-X62), W4 SHALL run the three hosted arms alone (phase 2 only), and P3-L2 and P4-FP SHALL be `withheld{l1_not_passed}` and moved to a later pass. The fallback is decided at FP-E from the trigger record, never from an outcome field. [Synthesis-allocated, O2 RB-14; Q-X52.] | Fallback fixture: a trigger record yields a hosted-only W4 schedule and both typed withholdings; the FP-E readout cites the trigger. · `AC-10.1-14` · E |

> | **REQ-XD-102** | No paid L2 pair SHALL run before G-L1 records a pass for the exact H version used. Any H change after G-L1 requires a full L1 rerun, and a new cohort if a tranche is in progress. | The runner refuses to schedule L2 without a matching `hh-l1-conformance/1` pass id. · `AC-7.4-2` · E |

> | **REQ-XD-106** | Allowlist changes after a gate run SHALL bump the version and rerun on an unused gate partition. Partitions SHALL be by task, and gate partitions SHALL be committed by digest before H development. | The partition-digest commit precedes the H freeze commit; version-bump test. · `AC-7.4-6` · D |

> | **REQ-XH-114** | The first-pass L1 natural corpus SHALL be RECORD captures of the original mini-swe-agent on the 7 development tasks of §10.1.2 in W2 (28 trajectories), partitioned by task under a committed seed into dev, gate-A and gate-B (3 / 2 / 2 tasks; 12 / 8 / 8 trajectories) before H development uses the dev partition. H development SHALL see only the dev partition, with verdict, reward and verifier fields removed (REQ-XH-121). H's identity is fixed by rule in the pre-registration ("the version that passes G-L1") and frozen by digest at G-L1, before any L2 subject. The HH-native arm SHALL run only after G-L1 passes for the exact H version. [Synthesis-allocated from Q-X47; revised O2 per Q-X52, RB-11.] | Partition-digest commit precedes the H freeze; no development task is in the held-out manifest; the dev projection has no verdict member (schema test); the runner refuses the HH-native arm without the matching `hh-l1-conformance/1` pass id. · `AC-10.1-5` · E |

> | Q-X62 | FP-S sequencing against the E2 chain, and the G-L1 fallback trigger (RB-14; Q-X46, Q-X52) | pending — ledger default (verbatim): "First-pass spend gate (FP-S) waits for the E2-T8 rung record; hosted-only fallback fires if G-L1 has not passed 14 days after FP-S is otherwise ready (14 is a synthesis placeholder)". Spec specifics (v0.3 re-synced to the ledger, RD-6; the v0.2 anchor "W2's teardown" and its rung-3 trigger are withdrawn): "otherwise ready" means every FP-S precondition except the E2-T8 rung record is met; a G-L1 failure on both gate partitions triggers Q-X52's fallback at once; an E2-T8 rung-3 record is not a trigger (REQ-XE-120's ADR route); once the trigger fires, FP-S no longer waits for the rung record (§10.1.4; REQ-XH-123) | ratification (the operator fixes the number); recorded at FP-S |

## Pre-registered thresholds

The L1 result itself is binary and deterministic (one failure is a fail; §7.4). The readout presents:

1. **`hh-l1-conformance/1`** for the exact H (by digest) from W3 (gate-A) and, if run, Wc's W3′ (gate-B, bumped `hh-l1-canon` version): legs L1.0/L1.1/L1.2, first-divergence result, the coverage table (BM1–BM19 complete), census complete, the frozen rule table, the stable-prefix histogram, and an `hh-l1-diff/1` record for every divergent trajectory (R6.28 schemas).
2. **H freeze:** the H-set digest built in W3 (REQ-XC-330) and the partition-digest commit preceding it (REQ-XD-106, REQ-XH-114).
3. **Trigger state:** R6.33's FP-S readiness record re-run and cited — the readiness instant, the trigger instant (with the number of days GATE-G26 fixed) and whether it has passed with no L1 pass recorded.
4. **Masking:** no first-pass outcome field; the L1 result is machinery evidence (REQ-XH-121).

**Pre-registered dispositions** — either is recorded `PASSED — disposition <token>; basis: <verbatim>` (GATE-G15 precedent); the basis never contains the tokens `FAILED` or `NOT PASSABLE` (write "did not pass"):

- `full` — L1 passed for the exact H, frozen by digest here; the HH-native arms may run in W4 (REQ-XD-102); R6.46 quotes five arms.
- `fallback` — the Q-X62 trigger fired, or L1 did not pass on either gate partition, or L1 did not pass on gate-A and no gate-B rerun is runnable in the first pass because GATE-G28 recorded Wc unused or Wc served another trigger (§10.1.4 Wc row: at most one contingency window; §7.4: a fail is never relabelled — the basis cites GATE-G28's readout and the Q-X62 trigger instant, which then necessarily finds L1 unpassed); FP-E is held in its hosted-only form; P3-L2 and P4-FP are `withheld{l1_not_passed}` (REQ-XH-123); R6.46 quotes the three hosted arms.

Only an operator refusal stops the chain. Never pre-answered: which disposition applies is read from
the presented records at presentation.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to L1, W3 or Wc remains
(DEFERRALS rule 4); never silently passed.

## Disposition

Record the operator's exact words, UTC and channel, the disposition token and basis verbatim, the
frozen H digest (under `full`) or the trigger / partition record (under `fallback`), and confirmation of
any agent-drafted block's sha256; append the readout Signature and the matching LEDGER GATE DECISIONS row.

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) XO-01…XO-06 and the
manifest operating rules; X-03 (decision gate, never pre-answered), X-07, X-08 (L1 is a precondition
gate, not parity evidence). No code branch is created by a gate.

## Notes

- Owns no REQ id. Consumes `hh-l1-conformance/1` / `hh-l1-diff/1` / the L1 pass id (R6.28), the readiness record (R6.33) and the H set (R6.23 kit, built in R6.44).

> Consistency fix 2026-11-07: the state "gate-A did not pass, no gate-B rerun" is pre-registered: a gate-A failure is a W3′ trigger presented at GATE-G28; if Wc runs W3′, this gate reads gate-B; if Wc is unused or served another trigger, this gate records `fallback` with that basis. Never `full`, never a relabel. (item 14)

