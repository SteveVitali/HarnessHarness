# HarnessHarness — round-6 release notes (DRAFT)

> **Draft for the operator to edit — not a published note.** Derived
> from `docs/build/BUILD_INDEX.md` round-6 rows (one row = one line; PR
> links are canonical `SteveVitali/HarnessHarness` URLs — the
> MetaHarness remote name redirects). Generated 2026-11-19 by CAP6.3.
> Companion: `reports/round6/INTEGRATION_PLAN.md` (the merge order these
> PRs land in). The build is **not DONE** — GATE-G32 and DOC6.1 remain,
> and the open tail of this chain is red on one CI leg; this draft
> describes the round-6 increment as recorded, withheld predicates
> included. **GATE-G30 ended the first pass early** (`re-scope{end-
> first-pass-early}`): no comparative claims are published; the
> results-site first-pass pages carry explicit first-pass labels and
> `outside_verified: false`.

## Headline

Round 6 — the frozen experiments supplement (`docs/experiments-spec.md`,
canonical v0.3): a five-layer experiment platform — records-in
provenance, the Instrument results site, integer-µ$ pricing and the
mediating model proxy, the cell/window experiment apparatus on GCE with
mediated egress, harness admission + three hosted harnesses, comparative
and withheld-measurement machinery, first-pass protocol apparatus (prereg
freeze, outcome seal, cost-only continuation, flagged review, results
freeze), and the round's publication records — delivered as a ~74-row
manifest with 12 window executions, 30+ gate/human pauses, and 29 ADRs.
Requirement state at closure: **410/456 engineering closed + satisfied
(46 PARTIAL, 0 MISSING)**; spend **$31.887214 recorded ≤ $110**
($4.52 model billed + $27.37 compute *estimated*, GCP billing
`unavailable{no_billing_export}`). CAP6.2's closure record:
`reports/round6/CAPSTONE_CLOSURE.md`.

### Platform: guard, provenance, site, pricing (seq 253–260)

- **R6.1** — supplemental routing guard (`hh-supplement-guard/1` v1.1.0) + REQ-X register
- **R6.2** — records-in provenance: 71 scratch drivers verbatim as unmaintained provenance
- **R6.3** — records-in corrections + phase-1 record (AR-01/ADR-0423; AR-02/03 record kinds)
- **R6.4–R6.6** — the Instrument site: tokens/themes, `hh-site-data/1` generator,
  claims lexicon + `check-site.sh` + `hh-site-publication/1` + Pages workflow (ADR-0424)
- **R6.7** — integer-µ$ `RateUnit` + `NoPrice{unrepresentable_rate}` (AR-09/ADR-0425);
  Ollama lane + `forward_raw` (AR-11/ADR-0426)

### Execution apparatus: runner, cells, proxy, recipes, egress (seq 261–269)

- **R6.8–R6.9** — `hharness-runner`/`hh-runner` (AR-04/ADR-0427); cell WORKDIR/cpu/mem
  spec (AR-06/ADR-0428) + task-declared deadlines (AR-07/ADR-0429) + verdict channels
- **R6.10–R6.12** — `hh-model-proxy`: token gate, §4.2 route/field policy,
  `GenParamPolicy` pins (AR-10 — ADR owed, CAP6.3 R6A-2), release modes + BUDGET,
  `hh.model_proxy.recording/1` strict/lenient replay + `hh-receipt-index/1`
  (AR-20/ADR-0430; AR-08 ADR owed, R6A-2)
- **R6.13–R6.14** — `gce-backend.sh` recipe + `hh-compute-decl/1`/`-receipt/1` +
  nine-step teardown (AR-15/ADR-0431); `hh-live-pricing/2` + phase ledger +
  `evaluate_admitted_costs` + `hh-slot-costs/1` (AR-14/ADR-0432; AR-16 mechanism)
- **R6.15–R6.16** — `allowlist_mediated` egress profile + static-musl `hh-cell-relay`
  mediator, decide-before-byte (AR-05/ADR-0433); `hh-runner-window/1` scheduled drive

### Subjects, selection, methods, harnesses (seq 270–284)

- **R6.17** — Harbor O-surface at the pinned tag + re-proof machinery (live-read)
- **R6.18–R6.19** — `task-selection/1` + protocol/quote/support gate (AR-12/ADR-0434,
  AR-13/ADR-0435); `finite_manifest_rate` + `hh-comparative-results/1`
- **R6.20–R6.21** — site phase-2 pages (leaderboard, frontier, noise, pilot, quote,
  grids, trials, dormant viewer)
- **R6.22–R6.25** — `hh-harness-admission/1` AG1–AG9 + ST2 verify-only (AR-18/ADR-0436);
  carried binary sets + HF upload kit (ADR-0437); `hh-harness-render/1` for
  mini-swe-agent/Harbor/HH-native; trajectory copy-out + admission smoke battery
- **R6.26–R6.27** — PS0 planner + runner-check surface (its executor is the XC-325
  unexecuted leg — waiver rides G32); comparative battery + `answer-scan/1` +
  AR-17/AR-33 dossier (ADR-0438/0439)
- **R6.28–R6.30** — L1 apparatus (`l1_obs`/`ObsReplay`/`l1_drive`); cost-only
  continuation rules + holds; `hh-fp-sealed-view/1` + `hh-fp-dev-projection/1`
  (X-07 outcome masking)

### Publication + windows (seq 287–319)

- **R6.31** — phase-1 `hh-site-publication/1` under GATE-G22 + Pages read-back
  (DF-R6.31-1 discharged)
- **R6.32–R6.45o** — the window chain: W1 (opened, repaired, re-executed; 159-trial
  re-proof `live-executed`), W2 paid (UPLOAD-W1 `archive_verified` 704/704, withheld
  legs verbatim), W3 $0-model, Wc + six repair windows (machinery repairs:
  compose-frontend, `cp_in`, task.toml deadlines D15, egress D8–D14 + epoch scans),
  smoke1/2/4/5 non-evidence diagnostics, Wc9/Wc10/Wcs1 re-proofs —
  **Ws1 withheld-subject legs: PP0 + L1 corpus + gate-A `result: fail` verbatim;
  compute estimate-over-ceiling 5,250,954 µ$ vs 4,136,280 declared (≈127%),
  waiver rides G32**
- **R6.45i** — §10.5 intervention amendments A1–A7 (operator rulings R-1…R-11,
  ADR-0443…0449, manifest lines) — spec body stayed frozen
- **R6.34–R6.39** — model-port surface: ADR-0440/0441/0442 (AR-21/22/23), `hh-tmpl/1`,
  tools surface, observation plumbing, AR-24 applied verbatim (`stop_rule.submit`)
  + `open_session.exec` (AR-25), exec-defaults + topology rung record
- **R6.41–R6.43** — compaction slot + HH-native participant; `hh-fp-partition/1`;
  `sweep-registry/1` + D3-FP wiring
- **R6.46–R6.50 + gates/humans** — FP-E quote admits zero executable work →
  GATE-G30 `re-scope{end-first-pass-early}`; credit NOT_NEEDED (H15); W4
  `unused{scope_end}` (R6.47); seal-open + answer-scan + first-pass packets
  (R6.48); flagged-review 35/35 cleared/0 confirmed (H16) + results freeze
  (R6.49); publication presentation (G31) + dataset flip public @`a11591e9`
  (H17) + first-pass publication records (R6.50); billing reconcile $4.52
  billed / compute `unavailable` (H18)
- **R6.51** — runtime-evidence verify-host derivation, exactly the three
  smoke1-observed hosts (DF-R6.45l-1; out-of-band PR #413)

### Capstones

- **CAP6.1** — `hh-cap6-gap-hunt/1` + 456-row COVERAGE_MATRIX (verdicts before
  run-ledger reads)
- **CAP6.2** — closure: 410/456, two sums recomputed, A2-amended accounting,
  live reads, 34-id later-pass routing register, §0.3 history, G32 signature list
- **CAP6.3** — this reconciliation + integration plan

## Known-limitations register (first-pass labels, verbatim)

- **No comparative claims.** GATE-G30 ended the first pass early — the
  FP-E quote admitted zero executable work; W4 never ran; all outcome
  publication carries first-pass labels + `outside_verified: false`.
- **REQ-XC-325** R-replay executor: `unexecuted{no_executor}` — waiver
  request pending GATE-G32.
- **Ws1 estimate-over-ceiling** `5,250,954 µ$` vs `4,136,280 µ$` declared
  (≈127%) — waiver request pending GATE-G32.
- **Compute spend is estimated** (cost_lib, 21 records) — GCP billing
  `unavailable{no_billing_export}`; the two-fold variance (interim
  `30.024544` vs authoritative `27.367214`) is recorded, not collapsed.
- **34 routed ids** remain owed to later passes (owner+trigger per row —
  `LATER_PASS_ROUTING.md`); **4 scope-ended** under GATE-G30 verbatim;
  operator-disk retention **unrecorded**.
- **Stack CI:** PRs #414–#418 red on the check-ci-gating readers-scan
  leg at read time (INTEGRATION_PLAN §1c) — a maintainer decision item,
  not silently green.
- **Two §10.5 ADRs owed** (AR-08, AR-10) — mint-or-waive rides G32
  (R6A-2, proposed unticked).
