# GATE-G10 readout — accept the narrow offline analysis evidence

Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

*Append-only. Each operator reading is a new dated block; never edit an earlier one.*

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is approval.

## Criterion (verbatim — `docs/tickets/187_GATE-G10__offline-gate.md`, source: supplement §7)

> "fixture success never satisfies S1/S3."

**Pre-registered thresholds (verbatim):** Accept only the admitted, tested EX-02 method/metadata/round-trip path and its fixture label. Require exact selected numerical protocol, independent routed evidence and the normal example resource envelope. No live task, parity, confirmation or public scientific claim is authorized.

## Evidence (per item: source, `date -u` of the read, layer reached)

- `crates/hh-eval/src/finite_manifest.rs` + `tests/r5_16.rs` — landed R5.16 (PR #229) — read 2026-11-02 — layer: engineered
- `crates/hh-analysis`/`hh-embed` routing + batteries — landed R5.16b (PR #230) — read 2026-11-02 — layer: engineered
- `crates/hh-analysis/tests/r5_17.rs` independent battery (SplitMix64 + integer-Beta order-statistic path, distinct from the estimator) — landed R5.17 (PR #231) — read 2026-11-02 — layer: engineered
- DEFERRALS.md — DF-R5.17-1 OPEN (owed to this gate); DF-R5.1-1 OPEN (intentional GATE-G17 row) — read 2026-11-02 — layer: records
- Stack CI `ci: pass #231@2aebc5b` (14-PR ancestor stack green) — read 2026-11-02 — layer: records
- `check-build-memory.sh` exit 0 — 0 viol / 8 scoped baseline warnings — run 2026-11-02 — layer: engineered

<!-- agent-drafted:begin sha256=6d8d0446e7a0aa03063aa4cd4854fa971f738ae2d5febcd791e744db50a4db79 -->
GATE-G10 orientation verified at presentation (reads this coordinator session):

- **Evidence presented for acceptance:** the admitted, tested EX-02 path only — `finite_manifest` estimator (R5.16, PR #229: per-cell Beta posteriors, seeded ppm draws 8k→64k doubling record, analytic mean, observed contrast, labelled 95% credible, Beta{2,2} exploratory, closed refusal set; EX-02 observed 200_000 ppm / posterior mean 142_857 ppm) + routed persisted-report path (R5.16b, PR #230: kind → analyze_and_record → analysis_report_body/1, typed boundary refusals) + independent verification battery (R5.17, PR #231: separate SplitMix64 + integer-Beta order-statistic sampler, interval reproduction ≤20_000 ppm @60k draws, 4 cannot-pass proofs, scenario harness that cannot count unrun/discrepancy/missing as pass).
- **Fixture label:** this is ENGINEERED/fixture-layer evidence on the EX-02 contract path — it does NOT satisfy S1/S3, does not touch live tasks, parity, confirmation, or any public scientific claim. Numerical validation itself remains owed to R5.40/future gates per your GATE-G9 disposition.
- **Stack:** PRs #214–#231 closed green; chainTip codex/harnessharness-evidence-r5.17; ledger lastCompleted R5.17; guard PASS; validator 0 viol / 8 scoped warnings; spend $0.
- **Open scoped technical deferrals requiring your disposition (DEFERRALS rule):**
  - **DF-R5.17-1 (OPEN, found by R5.17's independent review):** `beta{2,2}` exploratory runs persist `predeclared_exploratory`/`prior.beta.{2,2}` labels over Beta(1,1) math — the declared prior never enters the estimator's posterior mean/draws. Owed to R5.40/GATE-G10 with a proposed ~6-line seam. Needs your ruling: defer-to-R5.40 as recorded (recommended — defect is recorded, label is honest, scope is exploratory) or resolve now.
  - **DF-R5.1-1 (OPEN, intentional):** GATE-G17 contributor-evidence row, recorded at R5.2 by design — informational only.
- **Your disposition should name:** acceptance (or not) of the EX-02 offline path as engineered evidence, the DF-R5.17-1 disposition, carried unmet predicates, expiry/void conditions, and confirmation of the agent-drafted block by its hash prefix.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->

Operator decision (verbatim, received 2026-11-02T23:43:27Z via Devin Desktop chat): "1. passed. 2 defer. 3. can confirm. Please use your best judgement and continue" — PASSED the admitted, tested EX-02 offline path as engineered/fixture evidence only; DF-R5.17-1 disposition = defer to R5.40 as recorded (honest exploratory label carried, defect recorded); agent-drafted block sha256:6d8d0446e7a0aa03063aa4cd4854fa971f738ae2d5febcd791e744db50a4db79 confirmed ("can confirm" to its presented prefix). Scoped items: EX-02 estimator + routed persisted-report path + independent battery only; S1/S3, live tasks, parity, confirmation, and any public scientific claim remain unauthorized; numerical/study validation stays owed to GATE-G10's successors (R5.40) per the GATE-G9 disposition. Expiry/void: not separately stated — carried per contract posture (materially changed admitted semantics or operator cancellation). Carried unmet predicates: R/P + actual-execution E legs remain owed; DF-R5.1-1 stays OPEN (intentional GATE-G17 row).
GATE DECISIONS row: 2026-11-02 | GATE-G10
Signed by: the operator. Recorded by devin-desktop/swe-2-high, which does not sign.

