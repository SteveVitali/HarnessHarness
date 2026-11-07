# Execution-surface replay record — phase 1 (TB-core 0.1.1, $0 oracle)

Evidence class: recorded execution — internal record; not outside-verified; not `live-executed` (no `hh-live-evidence/1` record).

- Record date: 2026-11-07 · Baseline commit: `39fb3f23` (R6.2) · Status: internal; a publication gate governs any outside use.
- Question actually tested: execution-surface replay with a gold-script participant. It does not test agent quality.

## What ran

| Record | What ran | Basis |
|---|---|---|
| Parity tranches t1–t3 | `tb run -a oracle` vs a scratch reference podman runner (`parity-h-side.py`), not HH cells; R = 1 | [t3 packet](../build/reports/2026-11-05-parity-tranche-3.json), [t3 run](../build/runs/PARITY-TRANCHE-3.md) |
| S4 confirmation cohort | HH cells over four epochs, 300 slots, 520 records | [packets](../build/reports/2026-11-05-s4-confirmation.json) · [deviation record](../build/reports/2026-11-07-s4-protocol-deviations.md) |
| R5.59 efficacy + study fold | the committed `finite_manifest_contrast` over byte-copy fixtures | [efficacy](../build/reports/2026-11-06-r5.59-efficacy.json) · [study](../build/reports/2026-11-06-r5.59-study-report.json) · [recompute](../build/reports/2026-11-07-r6.3-study-report.json) |
| Records-in hardening | cross-check + estimator ladders over committed records | [xcheck](../build/reports/2026-11-07-p1-surface-xcheck.json) · [estimators](../build/reports/2026-11-07-p1-estimators.json) |

## Methods

Seeded manifest (quotas 5/16/9, `protocol_id 9edc5327…`); the tranche
endpoint (exact directional agreement, N = 30); the S4 schedule and its
four separately authorized epochs; the two verdict channels
(`verbatim`, `vendored_offline`); the structured repair register
(carried by the [deviation packet](../build/reports/2026-11-07-s4-protocol-deviations.json),
class ∈ {machinery-committed, machinery-scratch, environment,
admission}, code location with commit, affected slots); the records-in
cross-check and estimator procedure ([p1_records.py](../build/tools/p1_records.py),
RC-1…RC-6); the 2026-11-07 study recompute folding the executed
DF-CAP5.1-1 packet (`7bbd15a2…`).

## Results

- Tranche counts: 30/30 directional agreement (22 both-resolved, 4
  both-failed tests, 4 both-builds-failed; 26/30 H runs reused the
  O-built image). Earlier tranches `infeasible{margin}` stand verbatim.
- Cross-check: basis A 14/16 agree, 2 discordant
  (`grid-pattern-transform`, `swe-bench-langcodes`); basis B 26/30;
  basis C descriptive 260/300 records.
- Estimators, verbatim: `MissingKey{pins.rights_ref}` (strict
  records-only), `AdapterFloor{repeats:1}` (design probe),
  `UnequalDesign{pairs 16 ≠ T·R 30}` (cross-check), `MissingMargin`
  (kl_equivalence, no registered margin). Descriptive bounds only.
- Study folds: R5.59 `reported`, `unmet{s1_failed}`; the 2026-11-07
  recompute `reported`, `unmet{s1_objective}`, `s1_unmet: true` — the
  bounded slice discharged, the objective open.
- Costs: inference $0.00 structured in every packet; compute a
  rate-card estimate, never a billing figure.

<!-- bounds:begin -->
## Honest bounds

1. The parity tranches compared `tb run -a oracle` with a reference podman runner that is not HarnessHarness; HH cells were not a parity surface.
2. Each tranche ran one first attempt per task per runner (R=1).
3. `verified` is the tranche's own exact-agreement rule; the landed `parity_admission` refuses this design: on the strict records-only rung with `MissingKey{pins.rights_ref}`, and on the labelled design-probe rung with `AdapterFloor{repeats:1}`.
4. Of the 30 agreements, 22 are both-resolved, 4 both-failed tests and 4 are pairs in which both builds failed; 26 of 30 H runs reused the O-built image.
5. 0.115703 is a descriptive concentration endpoint at N=30 with zero discordance, not a registered margin or an equivalence verdict; no margin was registered.
6. The confirmation cohort ran through HH cells and has no original-runner comparison of its own; the records-in cross-check against tranche 3's `tb` verdicts agrees on 14 of 16 tasks with a first-attempt HH verdict and disagrees on 2; on the final fold it agrees on 26 of 30.
7. With the oracle participant the compaction arms cannot change an outcome; the efficacy contrast is 0 by construction and is not efficacy evidence.
8. 300/300 is a four-epoch composite: 140 slots were re-attempted beyond the frozen reattempt bound, across two backends (arm64 applehv, amd64 GCE) and two network modes.
9. Grading ran tb's `PytestParser` in scratch Python outside HH; 100 of 300 final receipts used the `vendored_offline` channel.
10. Inference cost was $0.00 (structured in every packet); compute was a rate-card estimate of ≈$1.72 for the VM's life (16:32Z–22:15:40Z), originally recorded as ≈$1.41, recorded in prose only, billing not read; local VM time is uncosted.
11. Benchmark edition: `terminal-bench-core==0.1.1` (80 tasks), substituted for the TB 2.0 the supplement names ([ADR-0423](../adr/ADR-0423-r6.3-tb-edition-substitution.md)).
12. The ANSI-strip and declared-workdir repairs were made in scratch drivers; not yet committed to the runner (the committed-runner ticket is later in the chain — this line is updated by dated addendum when it lands).
13. The drivers that produced these packets are preserved verbatim as unmaintained provenance; the packets are recomputable records-in from the committed raw set, not re-executable from the repository.
<!-- bounds:end -->

## Deviations and corrections

The dated [S4 protocol-deviation record](../build/reports/2026-11-07-s4-protocol-deviations.md)
enumerates every departure with recomputed counts and verbatim
authorizations — never an amendment. [ADR-0423](../adr/ADR-0423-r6.3-tb-edition-substitution.md)
records the edition substitution retroactively. The dated correction
notes of AR-03 (R6.3) sit on the affected run/readout/backlog records.
S1 stays unmet in the latest fold pending the gated promotion.

## Reproducibility

The provenance tree
([archive](../build/reports/provenance/2026-11-06-scratch-drivers/))
preserves the 71 drivers verbatim as unmaintained provenance; the raw
set (`raw/SHA256SUMS`, 587 files) makes every packet recomputable
records-in via `python3 -I docs/build/tools/p1_records.py --check`. A
re-run is a fresh execution, never replay — the recipe names every
hard-coded input. Not recomputable records-in: the dataset itself
(content-addressed only, canary), the vendored mirror, and any live
service.

## Glossary

- **Parity tranche (t1–t3):** one O run per task of `tb run -a oracle`, paired with one H run of the scratch reference podman runner (`parity-h-side.py`, not HH).
- **Confirmation cohort:** the 300-slot S4 schedule (30 tasks × 2 arms × 5 rounds) through HH cells.
- **Epoch:** one execution campaign. The four are confirmation, egress, completion and repairs.
- **Slot:** a planned (task, arm, round) cell.
- **Attempt index:** the position of a record among one slot's records (1 = first).
- **First subject:** round-0 attempt 1.
- **Arm:** `arm:o-tb-oracle` (A, `evict_oldest`) and `arm:h-podman-oracle` (B, `clear_tool_results`). Both are HH-cell arms; the `o-`/`h-` prefixes are inherited names and do **not** denote `tb` or the reference runner.
- **Served variant:** the compaction label actually recorded.
- **Verdict channel:** `verbatim` or `vendored_offline`.

## Links

[evidence.md](../evidence.md) · [status.md](../status.md) ·
[executed-record audit](../research/24_executed-record-audit.md) ·
[methodology record design](../design/30_methodology-record.md) ·
[experiments supplement](../experiments-spec.md)
