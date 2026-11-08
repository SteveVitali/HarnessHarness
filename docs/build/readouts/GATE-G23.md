# GATE-G23 — Method admission: protocol, quote, estimators, schedule randomization and AR-33's departures · readout
Status: PASSED

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is approval.

## Criterion (verbatim from the spec)

Spec §10.3 gate table, row "Method admission":

> | Method admission | first | D1-b | independent numerical and routing review record; known-answer tests; battery records (stopped scenarios never counted), including the first-pass T = 10 grid; admit or refuse `comparative_protocol/1`, `comparative-quote/1`, the descriptive estimators, `slot_schedule_randomization`, and AR-33's first-pass departures (shared baseline, family of one, T = 10 / H = 3 below the §4.5 precision target) |

Spec §6.4:

> **REQ-XD-023** | New method pieces SHALL be implemented only after an independent numerical and routing review and an operator method-admission gate, following §4.3's route (AR-17). | Review record and gate entry precede first use. · `AC-6.4-2` · D

> **REQ-XD-024** | A comparative extension of `sim_battery` (H ∈ {2,4,8}, T ∈ {20,37,48}, k = 5, mixtures `board`/`easyband`/`hard`, σ ∈ {0, .75}, interaction, informative infra missingness, flagged-pass vetoes, shared-shock rounds; ≥ 2,000 datasets per scenario; frozen seed rule) SHALL run on the exact pipeline before the first confirmatory subject. For the first pass the grid SHALL add T = 10 with H = 3 and H = 2 (one arm dropped), and the pre-registration SHALL quote from it P(m ≥ 7 non-tied tasks), the power table and the median half-width (§10.1.3). | Battery records retained; stopped scenarios never count as admission. · `AC-6.4-3` · E

Spec §6.5:

> One Holm family per tranche is an explicit departure from §4.3's cross-strata family. It needs a method-admission decision (AR-17; Q-X40, recorded default: accept).

Spec §10.5 register rows:

> | AR-17 | Method admission of the new estimators; one Holm family per tranche | ADR via §4.3 route + gate | D1-b | first |
> | AR-33 | First-pass design and departures (Q-X47…Q-X55): one interleaved window with coupled O/H pairs; the shared phase-4 baseline and its concurrency with L2 (REQ-XD-215, REQ-XD-220); the family-of-one P4 contrast; a T = 10, H = 3 family below the §4.5 precision target; ε = ±.20 with the scoped label; the cost-only checkpoint and arm priority | ADR via the §4.3 route; listed at the method-admission gate | FP-PREREG | first |

Evidence spec §4.3, AC-4-2 final sentence:

> This accepts an apparatus, never a favorable treatment effect.

## Evidence (per item: source, `date -u` of the read, layer reached)

### Item 1 — the independent numerical and routing review record (REQ-XD-023)

- `docs/build/reports/2026-11-08-r6.27-method-review.md` sha256 `9d8d24550ad1acd1c12862f112521deec32ab88d56be1f5ee7ee50de3ddc26b9` — read 2026-11-08T14:06Z — covers `comparative_protocol/1` + `comparative-quote/1` (R6.18), the new estimators (R6.19) and the battery's routing; the independence basis is recorded inside it (fresh-context reviewer) for the operator to judge — layer: engineered
- `docs/build/reports/2026-11-08-r6.27-method-dossier.md` sha256 `76886ef2778fba9c24d4b6e2e1d54a95d6f7b1f6c8e93a0f5d2b7c8a1e4f6390` — read 2026-11-08T14:06Z — the method dossier binding every record by path + sha256 — layer: engineered

### Item 2 — known-answer tests (AC-6.4-1), green, with run references

- `cargo test -p hharness-eval --test r6_19` → **9 passed, 0 failed** — run 2026-11-08T14:06Z; the four known-answer legs land inside `rate_marginal_equality_and_known_answer` (constant p gives f = 2p(1−p); the single-arm interval equals the contrast machinery's per-arm marginal) and `flips_and_solve_known_answers` (the swap−rerun identity; $/solve at 0 solves refuses) — layer: engineered
- `task_mixture_slot_schedule_randomization` (same run) — the seeded schedule-randomization leg green — layer: engineered

### Item 3 — the battery records (REQ-XD-024)

- `docs/build/reports/2026-11-08-r6.27-comparative-battery.json` sha256 `ae3adccc14209274601952c67c5ffbdf8acbb2c0be0ac7575a25af14e1436bc2` — read 2026-11-08T14:06Z — `rollup=complete`, **77/77 cells × 2,000 datasets = 154,000**, 10,183 s wall; **0 stopped scenarios** (none counted toward admission); frozen-seed byte-identity tested (`ac1_seed_rule_byte_identical`); stopped-scenario refusal tested (`ac1_stopped_never_admits`) — layer: engineered
- Quoted first-pass quantities the pre-registration uses: fp null cell `P(m ≥ 7 non-tied tasks) = .208` (spec ≈ .20); median bootstrap half-width ≈ .14 (spec ≈ .14); T=10 power near-zero as pre-stated; informative-infra produces universal `UnresolvedCell` refusals; flagged-pass vetoes score as failures in the joint population; FM interval coverage ~98–99% at null, Holm type-I ≤ ~3% — layer: engineered (simulation outputs labelled UNVERIFIED for real tasks; no outcome data exists at this gate)

### Item 4 — AR-17 and AR-33 as proposed ADRs

- `docs/adr/ADR-0438-r6.27-ar17-method-admission.md` sha256 `5360b2b6bcee234cb85278ac2014eb5f9f1946b342c726c7c0f6ee0262a04345` — read 2026-11-08T14:06Z — **proposed**, never adopted — layer: engineered
- `docs/adr/ADR-0439-r6.27-ar33-first-pass-departures.md` sha256 `3f76ea5e64d96c721c7707f836daddeaa4643f86207fad0c08184bdb77830e29` — read 2026-11-08T14:06Z — **proposed**, never adopted — layer: engineered

### Item 5 — chain state at presentation

- `check-supplement-guard.sh`: `route: HOLD — gate GATE-G23 pending — RETURN PASS`; counts `74 active-round rows · stock_violations 0 · stock_warnings 8` — read 2026-11-08T14:06Z — layer: engineered
- `bash scripts/deferrals-open.sh` verbatim: `DF-R6.23-1 | OPEN` — scoped to the paid-window gate readouts (owners R6.40/R6.44/R6.45/R6.47, HOSTING block), not METHODS — read 2026-11-08T14:06Z; `--bl` → `BL-50 | open` (the Round-5 scope-disposition carrier, standing) — layer: engineered
- No OPEN METHODS-scoped deferral rows, including R6.27 review findings — the DEFERRALS rule is satisfied — layer: engineered
- `cargo test -p hharness-eval --test r6_27` → 5/5; `cargo test -p hharness-runner --test r6_27` → 3/3 (answer-scan, request-fingerprint, review-record schema) — run 2026-11-08T14:06Z — layer: engineered

## Pre-registered decisions for the operator (each separate, one verbatim disposition per item: `admit` | `refuse`)

- **(a) `comparative_protocol/1`** — §10.3 row; REQ-XD-020, REQ-XD-021.
- **(b) `comparative-quote/1`** — §10.3 row; REQ-XD-050, REQ-XH-030.
- **(c) the descriptive estimators** (rerun flip, harness-swap flip, $/solve, tied-with-leader) and the other new estimator pieces AR-17 names (the single-arm finite-manifest rate) — §10.3 row; §6.4; AR-17.
- **(d) `slot_schedule_randomization`** as the sharp-null exchangeability argument — §10.3 row; §6.4 table.
- **(e) one Holm family per tranche** (Q-X40 recorded default: accept — confirmed only at this gate) — §6.5; AR-17.
- **(f) AR-33's first-pass departures**: shared baseline, family of one, T = 10 / H = 3 below the §4.5 precision target, and the other departures AR-33's §10.5 row lists (one interleaved window with coupled O/H pairs; ε = ±.20 with the scoped label; the cost-only checkpoint and arm priority) — §10.3 row; §10.5 AR-33.

**Not decided at this gate:** any spend (none is authorized); ε (fixed at ±.20 by Q-X53); the V5 level (Q-X56, at FP-S); arm priority (Q-X61, at FP-S); the fallback trigger (Q-X62, at FP-S); Q-X57's single-contrast departure is cited from the ledger, not answered here.

<!-- agent-drafted:begin sha256=c3c524fd9eb36907710cf8970da7f4e6f07ac83efe2ddc66c5927bfe4ad2d74a -->
Draft summary for the operator (agent-written — judge the record set above, not this summary):

- **What is admitted if all of (a)–(f) pass:** the method machinery only — the records, estimators, schedule-randomization argument and departures become *usable for the recorded purposes*; no confirmatory analysis, first-pass outcome, or spend is authorized by this gate. The R6.19 estimators' typed refusal on first-pass use lifts exactly per the admitted items' recorded scope.
- **The battery's honest reading:** the T=10/H=3 first-pass family is under-powered by construction (power near zero in the fp cells — pre-stated, not discovered). Admission does not claim detection ability it lacks; it admits that the machinery reports honestly within its recorded bounds, incl. `UnresolvedCell` refusals and never-ranking the pilot.
- **AR-33's departures are first-pass scoped.** Admission carries each departure only inside the first pass; a later pass re-decides. Q-X57 stays pending — its citation is honest, not an admission.
- **If any item is refused:** it is not admitted for any consumer — R6.30, R6.33 and every quote/analysis row refuse typed for it; the chain stops here; repair is a scoped insert authorized by recorded operator words, never inferred.
<!-- agent-drafted:end -->

## Disposition

**PASSED — 2026-11-08T20:42Z · interactive operator chat · verbatim: "approve all and continue"**

- (a) `comparative_protocol/1`: **admitted**
- (b) `comparative-quote/1`: **admitted**
- (c) the descriptive estimators + the single-arm finite-manifest rate: **admitted**
- (d) `slot_schedule_randomization` as the sharp-null exchangeability argument: **admitted**
- (e) one Holm family per tranche (Q-X40 recorded default accept — confirmed): **admitted**
- (f) AR-33's first-pass departures (interleaved window + coupled O/H pairs, shared baseline, family of one, T=10/H=3 below §4.5, ε=±.20 scoped label, cost-only checkpoint + arm priority): **admitted**
- Scope: method machinery only — no confirmatory analysis, first-pass outcome, or spend authorized; each AR-33 departure is first-pass scoped and re-decided at a later pass; Q-X57 stays pending.
- Not decided here (unchanged): spend, ε (fixed ±.20 by Q-X53), V5 level (Q-X56 at FP-S), arm priority (Q-X61 at FP-S), fallback trigger (Q-X62 at FP-S), Q-X57.

## Signature
Operator decision (verbatim, received 2026-11-08T20:42Z via interactive chat): "approve all and continue"
GATE DECISIONS row: 2026-11-08T20:42Z | GATE-G23
Operator confirmation of agent-drafted text (verbatim, 2026-11-08T20:42Z): "approve all and continue" — covers sha256:c3c524fd9eb3
Signed by: the operator. Recorded by Devin (SWE-2 High), which does not sign.
