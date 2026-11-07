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
# GATE-G24 — Approve or refuse the AR-24 canonical amendment (`stop_rule.submit`)

- **Sequence:** 296 of 325 · **Phase:** Round 6 · **Kind:** gate
- **base_branch:** current checkout
- **Depends on:** R6.37
- **Readout:** `docs/build/readouts/GATE-G24.md` (create from `docs/build/readouts/_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite is `blockedOn`. Agents never sign, tick or infer approval, and silence is not approval.

## Criterion (verbatim from the spec)

Spec §10.3 gate table row:

> | AR-24 canonical amendment approval | first | E2-T4 (with E2-T0's docs bundle) | the §5e.1 `stop_rule.submit` amendment text through the canonical amendment protocol; approve or refuse (a refusal stops E2-T5 and is raised to the operator) |

Spec §10.5 register row:

> | AR-24 | §5e.1 `stop_rule.submit{marker, line, require_returncode}` | AMEND-C | E2-T0 → T5 | first |

Spec §7.3 (the gap and the requirement the amendment serves):

> - **G10.** The submit marker is documented but unimplemented, and its literal has no declared home. That needs a canonical-spec amendment (AR-24).

> **Stage-one spec changes:** exactly one canonical amendment (AR-24), three ADRs (AR-21 rendering, AR-22 exec and participant topology, AR-23 templates and renderers) and one schema change (AR-25).

> | **REQ-XE-116** | `stop_rule = submit{marker, line, require_returncode}` SHALL be detected by the dispatch gate on the settled raw output, under the canonical-spec amendment of §5e.1 (AR-24, G10). | BM12/BM13 fixtures. · `AC-7.3-13` · E |

Source: [§10.3](../experiments-spec.md#s10), [§10.5](../experiments-spec.md#s10), [§7.3](../experiments-spec.md#s7).

## Presented content (assembled by orchestrate-build, inside a labelled agent-drafted block)

- The AR-24 amendment text exactly as R6.34 committed it (E2-T0 docs bundle), with its sha256 as recorded in R6.34's run ledger, and the before/after of canonical §5e.1 `stop_rule` (`spec/sections/05e-control-orchestration.md` and its inline copy in `spec/CANONICAL_SPEC.md`).
- The three companion ADRs (AR-21, AR-22, AR-23) by number, for context only — they are not decided here.
- Landed state of the E2 rows ahead of the gate (R6.34, R6.35, R6.36, R6.37: BUILD_INDEX rows and PR links), per spec §10.3 "Presented after E2-T4".
- The amendment-protocol route the approval enables: application in R6.38 (§10.5 "E2-T0 → T5") as a dated `## Spec amendments applied` entry in `docs/tickets/00_MANIFEST.md` naming this gate as approver, plus the canonical front-matter status-line note (precedent: the R4.15a/b/c entries approved at GATE-G7).

## Pre-registered thresholds

The decision is binary and is not a numeric threshold. Exactly one disposition is recorded, verbatim from the operator, using R6.1's gate-disposition template:

- **`approve`** — the presented amendment text, at the presented sha256, is approved for application through the canonical amendment protocol; R6.38 applies it verbatim and implements REQ-XE-116 under it. Any change to the text after presentation voids the approval and needs a new presentation.
- **`refuse`** — the amendment is not approved. Per §10.3, "a refusal stops E2-T5 and is raised to the operator": the chain stops before R6.38. Only the operator's recorded decision then authorizes the pre-registered contingency in the plan ([Disposition gates and fallback routing](../build/planning/2026-11-07_round6-decomposition.md#disposition-gates-and-fallback-routing-sr-1)): a scoped insert that permutes {HUMAN-H14, GATE-G26, R6.40} ahead of the first unlanded E2 row; the HH-native arms then follow G-L1's disposition.

No disposition is pre-answered here. An approval of the companion ADRs, of AR-25, or of any spend is not part of this gate.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `docs/tickets/DEFERRALS.md` scoped to the E2 block (R6.34…R6.37) remains, unless an explicit protocol-compliant disposition with owner/trigger and withheld claims is recorded (DEFERRALS rule 4).

## Disposition

Recorded only from the operator's own words: UTC, channel, the disposition (`approve` | `refuse`) verbatim, the approved text's sha256, and any expiry/void conditions. The readout appends a Signature block and flips its `Status:` line (PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE + what would pass it); the same words go verbatim into `docs/build/LEDGER.md` GATE DECISIONS (`kind: decision`). Disposition basis text never contains the tokens `FAILED` or `NOT PASSABLE` unless that is the decision. A pending or refused gate stops dependent dispatch.

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) X-01, X-03 and XO-01…XO-06, and the [manifest operating rules](00_MANIFEST.md#operating-rules). No code branch is created by a gate. Nothing here authorizes spend (X-02) or edits `spec/` (the application is R6.38's).
