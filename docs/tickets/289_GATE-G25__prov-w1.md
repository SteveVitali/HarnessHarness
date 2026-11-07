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
# GATE-G25 — PROV-W1: authorize the W1 compute window ($0 model)

- **Sequence:** 289 of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** W1 · **Alias:** GATE PROV-W1
- **base_branch:** current checkout
- **Depends on:** HUMAN-H13
- **Readout:** `docs/build/readouts/GATE-G25.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10), gate table (presented after "SELECT commit + P2-infra + HUMAN billing alert"):

> | PROV-W1 | first | SELECT commit + P2-infra + HUMAN billing alert | compute quote (planning, bound, ceiling with restart, h_r and egress terms); dead-man action (Q-X38) and the pre-authorized restart (Q-X60); network posture (Q-X55); re-proof scope (REQ-XH-116); layer-export size estimate against W1's egress line (a quote above it refuses, REQ-XH-118) and the controller's free disk; Q-X39b |

Cited requirements, verbatim:

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

> | **REQ-XH-116** | The first-pass oracle re-proof SHALL cover the 10 drawn tasks, one pre-re-proofed successor per difficulty cell in the replacement order, and the 7 development tasks (which include the smoke tasks), on surfaces O, H-pub and H-allow: R = 3 for held-out tasks and R = 2, the declared minimum, for development tasks, whose verdicts enter no claim. A task excluded by the re-proof or by Q-X55 is replaced by the next task in its difficulty cell's committed order; a successor beyond those re-proofed in W1 needs the contingency window. The split, the draw, the replacement order and the development task list SHALL be committed (SELECT) before PROV-W1, and FP-PREREG freezes the post-replacement list. The re-proof of the remaining eligible tasks SHALL precede any later-pass draw. [Synthesis-allocated from Q-X47; revised O2 per RA-28, RB-17.] | Re-proof packet covers every first-pass and development task; the SELECT commit precedes the PROV-W1 gate record; the replacement log matches the committed within-cell order; later-pass split records cite a covering re-proof. · `AC-10.1-7` · R |

> | **REQ-XH-118** | First-pass compute SHALL use the default shape or a smaller declared shape that admits C ≥ 4 trials in flight under REQ-XB-214 and gives W_p ≤ D − the REQ-XB-128 admission cut-off for every window, with W_p including the declared retry allowance (REQ-XH-016) and the load of carried artifacts (REQ-XC-330); SHALL tear each VM down across every operator-gate wait not bounded inside its D; and SHALL keep Σ of the windows' ceilings — r·(D + restart) + residual disk at the declared h_r + egress estimate, including one contingency window — ≤ $20 (Q-X54). The W1 quote SHALL list the observe pass, the Rust and admission builds, the layer export, the census and the smoke battery explicitly, and the W3 quote the H-set build and export. PROV-W1 SHALL refuse a W1 quote whose layer-export size estimate exceeds W1's declared egress line. If W1's measured export exceeds that line, the overrun SHALL be taken from Wc's D at the next compute gate; if Wc cannot absorb it, that gate refuses until a recorded scope decision names what shrinks (Wc first, then W4's arms in REQ-XH-126's order). [Synthesis-allocated from Q-X47; revised O2 per Q-X54, RA-11, RA-25, RB-5; O3 per RD-1, RD-2, RD-13.] | Declarations and quotes: Σ ceiling_micros ≤ 20 000 000 with every term present; C ≥ 4 and W_p ≤ D − cut-off in each declaration, with the retry allowance and load terms present; an export-size estimate above W1's egress line refuses at PROV-W1; receipts show teardown before each gate wait. · `AC-10.1-9` · E |

> | **REQ-XB-224** | Each window's gate SHALL pre-authorize one harvest-and-teardown-only restart of a dead-man-stopped VM (≤ 1 h, no slot admission, no model key delivery), quoted inside the window's ceiling; any other restart needs a new authorization (REQ-XB-209). The residual-disk hours h_r of each window SHALL be declared with a maximum (first pass: 6 h), which is also the teardown deadline after a stop or the window's end. [Synthesis-allocated, O2 RA-9, RA-10, RB-5; Q-X60 pending.] | Shim test: a restart-only declaration refuses slot admission and secret delivery; the quote carries the restart and h_r terms; a teardown receipt later than h_r opens a dated `DF-*` row. · `AC-3.9-18` · E |

> | **REQ-XB-125** | Before any paid slot, the §3.8 $0 oracle re-proof (surfaces O, H-pub, H-allow; R = 3 per surface per task, minimum 2, with the admit rule R/R on every surface) SHALL decide every task: admit it, or exclude it on a typed outcome-blind basis. Discordances go in an append-only register. A machinery discordance is repaired, never excluded. An O-surface oracle failure excludes the task on the outcome-blind basis `runner_infeasible:oracle_fails_on_O{harbor_cause}`. *Absorbs:* REQ-XB-005 (merged). | Re-proof packet and discordance register reviewed at the W1 gate. · `AC-3.8-1` · R |

Cited operator decisions, verbatim from Appendix B (recorded defaults are never operator answers; each is confirmed or changed here, in the operator's words):

> | Q-X38 | Compute safeguards | pending — default: STOP dead-man; D from the authorized quote; teardown across gate waits; residual-disk h_r ≤ 6 h; the pre-authorized restart of Q-X60; billing alert at the compute authorization (first pass: $20, at 50/90/100 %); console-read reconciliation | ratification (policy); PROV-W1 (amounts) |

> | Q-X60 | Pre-authorized harvest-and-teardown-only restart after a dead-man stop (RA-10; extends Q-X38) | pending — ledger default (verbatim): "Each window gate pre-authorizes one harvest-only VM restart (≤ 1 h) after a dead-man stop". Spec specifics: no slot admission and no key delivery during the restart; it is quoted inside the window's ceiling (REQ-XB-224) | ratification (class); each window gate (amount) |

> | Q-X55 | First-pass agent/grading network (RA-12/RA-13; clarifies Q-X39a) | "No GitHub; grading hosts ok (Recommended)" — agent phase: model proxy + apt/PyPI/npm/crates only, tasks needing github.com excluded and replaced from the seeded order; grading phase may reach the hosts its verifier needs; integrity scan still runs | answered; §3.5, S-3, FP-14 |

> | Q-X39 | Allowlist provenance; re-proof evidence label | Q-X39a: "Grant registry hosts only (Recommended)" — standard registries (apt, PyPI, npm, crates, GitHub releases) plus hosts the instruction/task files name; other oracle-observed hosts withheld and logged. Q-X39b (live-executed label) still pending (received 2026-11-06, planning conversation dialog) | Q-X39a answered (narrowed for the first pass by Q-X55); Q-X39b pending — default: register a `live-verify.sh` leg so the re-proof can earn `live-executed`; otherwise "recorded execution"; decided at PROV-W1 |

## Pre-registered thresholds

Decision content the readout must carry before the operator answers (fixed now; never moved to fit a quote):

1. **Authorizations (REQ-XH-004).** Model authorization verbatim — W1 is $0 model (§10.1.4 W1 (a)); compute authorization stated separately and verbatim; the quote id (`comparative-quote/1`, R6.30's W1 quote priced by R6.14); the hard-cap formula — §3.10's window ceiling `r·(D + 1 h restart) + S·r_disk·h_r + 0.12·E_gib` in integer µ$ with every term shown.
2. **Quote content (REQ-XH-118).** Planning, bound and ceiling with the restart, h_r and egress terms; the W1 quote lists the observe pass, the Rust and admission builds, the layer export, the census and the smoke battery explicitly; C ≥ 4; W_p ≤ D − the REQ-XB-128 admission cut-off with the retry allowance and load terms present; Σ of every window's ceiling including Wc ≤ $20 (Σ ceiling_micros ≤ 20 000 000). §10.1.5's W1 row (D = 12 h, ceiling $5.05, egress ≤ 6 GiB) is a planning input, UNVERIFIED; the presented quote governs.
3. **Export line.** The layer-export size estimate is ≤ W1's declared egress line, or this gate refuses (REQ-XH-118); the readout states the controller's free disk against the export estimate.
4. **Dead-man and restart (Q-X38, Q-X60, REQ-XB-224).** GCE max-run-duration with action STOP; D from the authorized quote; h_r declared ≤ 6 h, which is also the teardown deadline; exactly one pre-authorized harvest-and-teardown-only restart (≤ 1 h, no slot admission, no key delivery) quoted inside W1's ceiling; teardown across every operator-gate wait (REQ-XH-118). W1 declares no `deliver-secret` (REQ-XB-219: a `deliver-secret` call in a $0-window declaration refuses).
5. **Network posture (Q-X55).** Agent phase: model proxy + apt/PyPI/npm/crates only, no `github.com`; grading may reach its verifier's hosts; every egress attempt logged per slot (X-06); a task needing another agent-phase host is excluded `runner_infeasible:allowlist_gap{host}` and replaced from R6.30's committed order.
6. **Re-proof scope (REQ-XH-116, REQ-XB-125).** The 10 drawn held-out tasks plus one successor per difficulty cell at R = 3, and the 7 development tasks at R = 2, on surfaces O, H-pub and H-allow, admit rule R/R; the SELECT commit (R6.30) predates this gate's record.
7. **Q-X39b.** The operator decides the re-proof's evidence label: register the `reproof` `live-verify.sh` leg (R6.17) so it can earn `live-executed` (recorded default), or label it "recorded execution".
8. **Preconditions present and cited:** HUMAN-H13's readout (alert amount, thresholds, `provided: yes` fields); R6.30's selection commit and W1 declaration; P2-infra landed (GATE-G23 PASSED); the window pattern GATE-PROV → PROV → runs → TEARDOWN → HUMAN-reconcile (REQ-XH-008; reconcile consolidated in HUMAN-H18).

**Dispositions.** `PASSED` with the authorization, quote id and Q-X decisions recorded verbatim; or the operator's refusal (the chain stops). A quote failing item 2 or 3 is presented as not passable until re-quoted, never passed with a waiver an agent drafted. Gate basis text never contains the tokens `FAILED` or `NOT PASSABLE` when the decision is a pass (the guard reads them as non-PASSED).

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to W1 or its prerequisites
remains (DEFERRALS rule 4). Resolve it or obtain an explicit protocol-compliant disposition with
owner, trigger and withheld claims; this decomposition is not that disposition.

## Disposition

Record the operator's exact words, UTC and channel, the authorizations verbatim, the Q-X38/Q-X60/
Q-X55/Q-X39b answers, expiry or void conditions, and explicit confirmation of any agent-drafted
block's sha256. Append the readout Signature and the matching LEDGER GATE DECISIONS row
(`kind: decision`). After `PASSED`, a scoped `decompose-spec` insert fleshes R6.32 in place
(X-03, Q-X58); nothing runs from this marker.

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) XO-01…XO-06 and the
manifest operating rules; X-02 (spend only as quoted here, enforced mechanically), X-03, X-05, X-06.
No code branch is created by a gate.

## Notes

- Owns no REQ id; contributes the readout leg of REQ-XH-004 (owner R6.1).
- The carried-artifact transfer (infrastructure set, bundles, layers) is an egress line in W1's quote (REQ-XC-330: "The transfer is an egress line in the exporting window's quote"); a measured overrun is taken from Wc's D at the next compute gate (REQ-XH-118), which GATE-G26 records.
