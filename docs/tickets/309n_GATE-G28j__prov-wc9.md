<!--
  Round-6 contract — H-allow hardening insert (decompose-spec mode=extend,
  orchestrate-build §2.4; insert spec
  docs/build/planning/2026-11-15_hallow-hardening-insert.md sha256 ff39c66f…).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator under Q-X20).
  Insert authorized by operator ruling R-2 (interactive, 2026-11-15). GATE-G28j
  is a normal gate, presented to the operator — NOT pre-answered.
  This contract CITES the spec; it does not copy a design. Executed contracts
  are frozen: amend by appending a dated note.
-->
# GATE-G28j — PROV-Wc9: authorize the Wc9 evidence re-proof's compute (the hardened-runner re-proof under epoch-4-or-3, D sized from smoke-measured walls, with the R-8 early-inspection checkpoint)

- **Sequence:** 309n of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** Wc9 · **Alias:** GATE PROV-Wc9
- **base_branch:** current checkout
- **Depends on:** R6.45l
- **Readout:** `docs/build/readouts/GATE-G28j.md` (owed — presented to the operator; not pre-answered)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10), gate table (verbatim):

> | PROV-Wc (at most once) | first | the triggering event: a W1 machinery repair (W1′), a gate-A L1 failure (W3′) or a restart overflow | compute quote (planning, bound, ceiling with restart, h_r and egress terms) within Wc's remaining reservation (at most $1.94, less any W1 export overrun taken from it); which of W1′, W3′ or restart overflow; D, restart and h_r; the repair's scope and its new participant or runner version record (REQ-XC-330); $0 model; Σ ≤ $20 recomputed (REQ-XH-118) |

§10.1.4, Wc row (verbatim):

> | Wc | at most one contingency window: a repair re-proof (W1′), an L1 rerun on gate-B (W3′), or a harvest-restart overflow; D = 4 h gives about 1.75 h of trial time after overhead, load and the admission cut-off (≈ 42 oracle trials at C = 4), and a larger repair is not runnable in the first pass, a scope decision PROV-Wc records | $0 | its own PROV-Wc gate | if not authorized by FP-E, its reservation is released to W4 except one restart-overflow reservation (REQ-XH-126) |

Cited requirement, verbatim:

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

Wc9 is a **recorded spec deviation — a ninth repair window**: the spec's gate table
declares PROV-Wc "at most once" and the Wc row "at most one contingency window" —
discharged at R6.45; the recorded Wc2–Wc8 amendments were discharged at R6.45b–R6.45h.
Under the R-5/A2 accounting the first-pass compute ceiling is $30 (completed windows
at billed/estimate figures, authorized-but-unrun at ceilings); the readout carries
the amended budget table, never the superseded Σ-of-ceilings rule.

## What the readout must contain (pre-registered — every element present before presentation)

1. **The Wc8 outcome** — `early_stop:operator_ruling R-1`, `blocked` verbatim, the D8/D9 machinery discordance observed live (first-error-line tally cited).
2. **SMOKE-PASS evidence** — the R6.45l smoke records (`docs/build/reports/smoke/`), or the honest exhausted-envelope diagnosis if the envelope ran out.
3. **The conformance-leg run URL** — the `egress-real-clients` GitHub Actions run (red baseline + green fixed-tip) with its `hh-live-evidence/1` record.
4. **The D sizing** — `D = clamp(⌈1.5 × projected wall from the smoke-measured per-member walls × the declared trial count ÷ C⌉, 4 h, 8 h)`, with the smoke walls cited per member.
5. **The quote** — minted by the one quote path (`receipt_lib.py quoteid`), ceiling by the §3.10 formula; `model_line: none`.
6. **The A2 budget table** — first-pass compute under the amended accounting (completed windows at billed/estimate figures, authorized-but-unrun at ceilings; ≤ $30 first-pass compute; model ≤ $70; $10 reserve; total ≤ $110).
7. **The §6 re-scope ladder** — besides `run | not_run`: `re-scope{harden-again}`, `re-scope{exclude-and-replace}` (only where the A5 rule makes an exclusion legitimate — a named denied host / O-fail — replacement from the seeded order per FP-7/REQ-XH-116), `re-scope{end-first-pass-early}` (record the re-proof outcome as the first-pass result and skip W4, naming the claims withheld).
8. **The prediction-track table** (A3) — each prior window's readout-predicted outcome vs the actual outcome (Wc→Wc8 inclusive).

## The window this gate would authorize

- **Shape:** R6.45h's evidence-window shape, cloned — the evidence rules unchanged: no in-window fixes, `retry_trials: 0`, typed outcomes, never rescued.
- **Runner:** the hardened chain-tip commit carrying all R6.45j/k/l repairs, each recorded with its carrier and regression (REQ-XC-330 version record).
- **Freeze:** epoch-4 if minted under the A4/R-6 conditional authority, else epoch-3 (`sha256:787db9e1…` carried verbatim).
- **Members:** the 14 `needs: wc_reproof` members at their committed R (held 10 @ R=3 + dev 4 @ R=2), as Wc8's.
- **Early-inspection checkpoint** (operator-preauthorized by R-8 as the readout records it): once the first H-allow cells of ≥ 4 distinct members complete, harvest their cell reports; if any shows a machinery signature (`verifier_unavailable:*`, a mediator 5xx, or a tunnel `close_cause` ≠ `eof`), stop the leg, tear down, record `incomplete{early_stop:machinery_signature}`; otherwise run to completion.
- **Post-Wc9 stop rule (§6):** if Wc9 leaves any member with a `machinery{…}` discordance or a `verifier_unavailable:*` classification, **no Wc10 is drafted** — `blockedOn` + a scope-decision readout with options 2–5.

## Dispositions (pre-registered)

`run` (the evidence window as specified) | `not_run` (the task list stays `blocked`) | `re-scope{harden-again}` | `re-scope{exclude-and-replace}` | `re-scope{end-first-pass-early}` — the operator answers when presented; nothing is pre-answered.

## Out of scope

- The window's execution — R6.45m's body (fleshed after this gate).
- Re-litigating R-3's smoke envelope or R-5's accounting — they are cited, not re-decided.
