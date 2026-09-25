# GATE-G2 readout — Stage-3 evaluation-first acceptance (the first claim about a harness may exist)

*Append-only. Each operator reading is a new dated block; never edit an earlier one.*

## Criterion (verbatim from the spec / gate marker)
The Stage-3 suite is green: the automated gates and executable tests of §10.7 pass; `removability(0…3)` holds and
the hosting-absent build passes the native suite (AC-R-2.10.6-5; AC-R-2.11.4-12; AC-R-2.12.2-12). After this stage
a claim about a harness may exist (§10.0).

**Pre-registered thresholds:** §10.7 automated gates + executable tests (T-LCD-03/-04/-11/-13 executable;
T-05/-08/-09/-14/-15 complete); `tier_violations=[]`, `cycles=[]`, `hosting_edges=[]`; both exemplars produce
`ComparisonReport{budget_match.status}`; `removability(0…3)` unchanged; the hosting-absent build passes the
native suite. OQ-338/OQ-342 block headline eligibility, not Stage-3 acceptance (stratum A carries it).

**Blocks:** all Stage-4+ tickets. **DEFERRALS rule:** no pass while any OPEN row scoped to this phase remains.

---

## 2026-09-24/25 — Reading 1 (orchestrator-authored evidence audit) → PENDING operator disposition

**Evidence sources:** per-ticket run ledgers `docs/build/runs/S3.1.md`–`S3.12.md`, `docs/build/BUILD_INDEX.md`
rows 44–59, `docs/tickets/DEFERRALS.md` (85 rows audited), `scripts/check-removability.sh`,
`crates/hh-registry/tests/plugin.rs::spec_dag_reports_clean`, the test suites named per item below,
`docs/build/LEDGER.md` (`benchmarkSet: PENDING_CREATE`).

### §10.7 automated gates — MET (hermetic)

| Item | Implementation + executable evidence | Result |
|---|---|---|
| AC-I2-1/-2/-3/-4/-5/-8/-9/-11/-12 | `hh-eval` compare/benefits/scorecard/outcome; `tests/acceptance.rs` (15) — compare refusals + budget_match, benefit gates incl. LeakedSplit, equivalence_run, scorecard strata, vetoed-success, judged-never-headline, OOP-oracle parity (S3.3) | MET |
| AC-J3-1/-2/-4/-5/-6/-7 incl. exactly-once across KP-E1…E5 | `hh-experiment::engine`; `tests/engine.rs` (29) — lifecycle, refusal corpus, `settle_is_exactly_once`, KP-E1…E5 recovery (S3.4a; AC-R-2.10.3-{1–8}) | MET |
| KA-1/-2/-5/-8/-9/-12/-13 | `hh-analysis` kernel/summarize/multiplicity/project/engine (20 tests) + `lab.analysis.analyze` conformance (S3.4c; AC-R-2.10.4-{1,2,5,6,8,9,12,13}) | MET |
| AC-J5-1/-2/-3/-5/-6/-7/-9/-12 | `hh-results` 14/14 over real Store+LabDocs+Engine rig — project_row, verify_row tamper, WatermarkAhead, rebuild byte-reproduction, L5 comparability (S3.4b) | MET |
| AC-I3-1/-2/-3/-4/-6 | `hh-bundle` assemble/codec/levels/runrefs/repro/validate; codec_roundtrip + s3_12 suites; conformance reproduce cells (S3.1, S3.12) | MET |
| AC-I4-1/-2/-4/-5/-6/-7/-8 | `hh-bench` 13 — typed reward, infra classification, separate/shared verifier, held-out isolation, FamilyUnsupported, unpinned→level caps, unenforceable-network refusal, in-process + OOP lifecycles (S3.3; AC-R-2.9.4-{4,5,6,7,8,11,12}) | MET (hermetic) |
| kill-point battery | KP-1…9/13/15 ×4 classes (`hh-env/tests/kill_point_battery.rs`); KP-E1…E5 (hh-experiment); kill@KP-8 byte-identical resume (s3_10) | MET |

### Executable tests with review items — MIXED

| Item | Status |
|---|---|
| AC-I2-6 + AC-I4-11 (T-LCD-03 round-trip on stratum C, n ≥ 5) | **OPEN — real-suite leg never run.** S3.3 explicitly deferred the reference-harness round-trip on the real Stage-3 suite + real-runner parity legs to GATE-G2 `benchmarkSet`. Fixture/engine halves landed (`hh-compiler` T-LCD-03 opacity; `hh-control` s3_10 paired replay). `benchmarkSet` is still `PENDING_CREATE` in the ledger. |
| AC-I2-7 (T-LCD-13) | MET as a battery (activation/following rates S3.3, `followed_rate` S3.7, deterministic `followed` S3.10, compliance folds S3.8). Caveat: the per-surface `delivered{artefact_id}` + per-call `activated` arm (AC-R-2.5.3-7) was **deferred** at S3.9 → DF-S1.17-3. |
| AC-I2-10 (portability minimum) | MET — portability gate + `non_portable` arms in hh-eval acceptance (S3.3). |
| AC-I4-3 (parity for A/C/D/E) | MET hermetically (in-process + OOP lifecycle parity legs). Real-runner parity legs ride `benchmarkSet`; OQ-342 margin unset — spec §10.9: blocks adapter headline eligibility, not Stage-3 acceptance. |
| AC-J3-8/-9 (both exemplars) | Executed to `CloseStatus::Completed` with E-4 `budget_match` records (`hh-experiment` engine tests — 30/50 plans). **AC-J3-9 ran under ADR-0213's interim OQ-363 rule** — final ruling unresolved (spec §10.9: "OQ-363 blocks AC-J3-9"). **Neither exemplar produces a `ComparisonReport`** — see gap G2-2. |

### "Also live" — MET

- Suites A/C/D/E + graders + `separate` materialisation + infra detector + stratified reporting (hh-bench, S3.3).
- Lab-internal `leaderboard()` L1–L5 (`hh-results`; L5 comparability test, S3.4b).
- `bundle(kind = run)`, `validate_bundle` S1–S7, `reproduce` R0/R1/R3 (S3.1; R3 seeds/distributions + R2 validator-set + bit-exact view re-derivation at S3.12).
- Retirement fixtures on both minimal profiles (`hh-lab/tests/retirement.rs` 6/6; `removal_verdict` consumes a real CompareOutcome). Caveat: the verdict chain is driven by fed-in verdicts — the runs→compare end-to-end leg is not engine-driven.
- C0 vetoes over the ledger: attribution_completeness (S3.9), evidence_tampered/verification_skipped/metadata_shortcut (S3.10), secret_leak + audit_completeness (S3.11b), vetoed-success (S3.3).

### Removability + structural thresholds — MET

- `tier_violations=[]`, `cycles=[]` — S1.27 recorded nodes=164/edges=741 clean; `spec_dag_reports_clean` re-asserts every run; negative test proves a hh-hosting dependency produces a violation.
- `hosting_edges=[]` — `check-removability.sh` rejects any normal crate dep into `hh-hosting` via `cargo metadata`; green in every S3.x ledger.
- `removability(0…3)` — first executable S2.1/S2.2; re-verified every S3 run (tier-0 refusals, tier-1 green, extension+hosting edge-free). Sustained.

### GAPS — the gate cannot auto-pass on the recorded evidence

**G2-1 · `benchmarkSet` legs never run.** T-LCD-03 real-suite round-trip (AC-I2-6), stratum-C + real-runner
parity (AC-I4-11, the AC-I4-3 legs), and DF-S1.24-2's foreign-manifest import are all deferred to this gate's own
`benchmarkSet` — which is still `PENDING_CREATE`. These are *this gate's* pre-registered evidence, not
later-phase work. Decision needed: are they runnable hermetically (recorded `model_io` + local fixtures — the
established offline pattern) via an inserted ticket, or does the operator authorize an external budget, or accept
their absence as a carried-forward deviation?

**G2-2 · Exemplar → `ComparisonReport` threshold met only in weaker form.** Both exemplars produce E-4
`budget_match` records (spec-shaped `{arms, status, tolerance_ppm, detail}`) but neither emits a
`ComparisonReport` — `hh-lab` has no `hh-eval` dependency and nothing runs the exemplars' settled rows through
`hh_eval::compare`. Machine-achievable: an exemplar→compare wiring test would close it.

**G2-3 · No recorded hosting-absent native-suite run.** Edge-freedom is proven structurally (script-enforced,
negative-tested) but no `cargo test` of the native suite with the hosting tier physically excluded is recorded
for AC-R-2.10.6-5 / AC-R-2.11.4-12 / AC-R-2.12.2-12. Machine-achievable: a removability build variant.

**G2-4 · Stage-3-scoped OPEN DEFERRALS (rule 4).** ~20 rows are Stage-3-scoped by their own `unblocked by`
cells. Two classes: (a) **foreign-toolchain / external** — DF-S0.3-2, DF-S1.2-2, DF-S1.5-3, DF-S1.8-1,
DF-S1.27-1 (cross-implementation agreement needing E2/E3 toolchains; cannot close hermetically);
(b) **machine-achievable or bookkeeping** — DF-S1.11-2, DF-S1.12-4, DF-S1.13-3, DF-S1.14-2, DF-S1.22-2
(substantially landed, never appended — bare OPEN); DF-S1.13-4, DF-S1.17-1, DF-S1.17-3, DF-S1.19-2, DF-S1.21-2,
DF-S1.22-1, DF-S1.24-1, DF-S1.24-2, DF-S2.4-3, DF-S3.5-1, DF-S3.9-1 (residual arms, some hermetically closable).
Also DF-S1.9-4 (AC-CC-09 rung audit — unblocked-by "the next gate readout": this one).

**G2-5 · Spec-authority discrepancy.** §9.4's Stage-3 gating list is broader than §10.7's row (includes
AC-R-2.10.4-{3,6,7,11} and AC-R-2.5.3-7 — the latter explicitly deferred at S3.9). §10.7 (the marker's cited
authority) stages KA-3/-6/-7/-11 at "3–4". The operator should rule which list governs Stage-3 acceptance.

### Earlier-phase residuals carried unflipped (not Stage-3-scoped; listed for completeness)
DF-S1.5-1, DF-S1.12-1, DF-S1.12-2, DF-S1.14-1, DF-S1.15-1, DF-S1.21-1, DF-S2.9-1 — their unblocking tickets
already ran; residuals are later-phase or housekeeping.

### Operator questions for disposition
1. **§9.4 vs §10.7 scope** — which acceptance list governs the gate (in particular AC-R-2.5.3-7's deferred arm)?
2. **benchmarkSet legs (G2-1)** — insert a gap-closure ticket (S3.12b, GATE-G1's S0.3b precedent) to build the
   hermetic reference suite + run the deferred legs; authorize an external budget; or accept as carried-forward?
3. **Stage-3-scoped OPEN rows (G2-4)** — which does the operator rule (a) verify-and-close via the gap ticket,
   (b) later-phase (re-scope), or (c) accepted residual carried to GATE-ACCEPT (the GATE-G1 precedent)?
4. **Exemplar threshold (G2-2)** — do the E-4 `budget_match` records satisfy "produces
   `ComparisonReport{budget_match.status}`", or is the exemplar→compare wiring test required?
5. **Hosting-absent run (G2-3)** — required before disposition, or is structural edge-freedom + the recorded
   `n/a{class}` machinery sufficient?

### Disposition
- [ ] PENDING — awaiting operator ruling on the five questions above (and any gap-closure ticket insertion)

---

## 2026-09-25 — Reading 2: post-S3.12b gap closure → awaiting operator disposition

**Evidence sources:** `docs/build/runs/S3.12b.md`, PR #62 (`svitali/harnessharness-s3.12b`, head `95a9dea`),
`docs/tickets/DEFERRALS.md` (post-sweep), `docs/build/LEDGER.md` (`benchmarkSet: benchset.stage3.v1`), ADR-0291.

### Reading-1 gaps — disposition after the gap ticket

| Gap | Status after S3.12b |
|---|---|
| **G2-1 benchmarkSet legs** | **CLOSED.** `benchset.stage3.v1` committed (`crates/hh-bench/fixtures/benchset/stage3_v1` — 4 strata A/C/D/E, recorded `model_io`, deterministic graders, strict loader refusals); ledger `benchmarkSet` flipped. T-LCD-03 stratum-C round-trip n=5 through the canonical `hh-control` Driver + replay; A/C/D/E adapter-parity legs through the corpus's own adapters; unbudgeted-arm compare refusal — `hh-bench/tests/s3_12b.rs` 3/3 + `benchset.rs` suite. |
| **G2-2 exemplar→ComparisonReport** | **CLOSED.** Both exemplars (compaction 30 plans; control-strategy 50 incl. `iso_cost`) driven engine→`project_row`→`eval_run`→`analyze`→`hh_eval::compare`, asserting `ComparisonReport{budget_match.status=matched}`; cross-mode compare refuses `IncommensurableMatch` — ADR-0213's OQ-363 interim rule exercised and recorded **standing** for this reading. `hh-embed/tests/s3_12b.rs` 3/3. Retirement caveat closed: engine-driven runs→compare→`removal_verdict`→`settle_removal_test`→human-sealed `retire`, no fed-in verdict. |
| **G2-3 hosting-absent run** | **CLOSED.** `cargo test --workspace --exclude hh-hosting`: exit 0 — 176 binaries / 2027 tests / 0 failed; `hh-hosting` never a target. Recorded in `runs/S3.12b.md`. AC-R-2.10.6-5 / AC-R-2.11.4-12 / AC-R-2.12.2-12 absent⇒unchanged halves now evidenced. |
| **G2-4 Stage-3-scoped OPEN DEFERRALS** | **SWEPT — no bare-OPEN rows remain.** Five verify-candidates retain genuinely unmet cells (Stage-4 check-8, metric emission, per-image canary, sink fixtures, combined fault fixture) → stay OPEN with residuals named; eleven residual rows annotated; five foreign-toolchain rows carried forward; DF-S1.9-4 audit ran (29/74 ADRs carry the rung statement → new row DF-S3.12b-2). Two new discovered rows opened honestly: **DF-S3.12b-1** (`measurement.experiment.closed` audit-cap overrun at exemplar scale — asserted via `assert_close_audit_cap`, not worked around) and **DF-S3.12b-2** (rung-statement uniformity). |
| **G2-5 §9.4 vs §10.7 authority** | Documented in the run ledger; **awaits the operator's ruling** (which list governs; esp. AC-R-2.5.3-7's deferred arm — a C0/S3 AC in §9.4's list, deferred to DF-S1.17-3). |

### Residuals carried forward for this disposition (GATE-G1 precedent: accepted, tracked to GATE-ACCEPT)
- **Foreign-toolchain rows** (cannot close hermetically): DF-S0.3-2, DF-S1.2-2, DF-S1.5-3, DF-S1.8-1, DF-S1.27-1 — cross-implementation/cross-ecosystem agreement cells needing E2/E3 toolchains.
- **Human deliverable:** DF-S0.3-3's R2 cross-camp reviewer signature (carried forward from GATE-G1).
- **Externally-bounded arms:** LT-03 live AgentDojo corpus arm (gate-deferred at S3.11b under the offline authorization); DF-S1.17-1's regex-vs-NL form arm (needs a non-`native_fc` family); DF-S1.21-2's OOP validator battery.
- **Discovered defects:** DF-S3.12b-1 (close-event audit cap at exemplar scale), DF-S3.12b-2 (29/74 rung statements).
- **Later-phase residuals** annotated in the sweep (S4/S6-scoped cells of Stage-3 rows): DF-S1.13-4, DF-S1.19-2, DF-S1.22-1, DF-S1.24-1, DF-S2.4-3, DF-S3.5-1, DF-S3.9-1, plus the earlier-phase carry set (DF-S1.5-1, DF-S1.12-1/-2, DF-S1.14-1, DF-S1.15-1, DF-S1.21-1, DF-S2.9-1).
- **Spec-level items for the operator:** §9.4-vs-§10.7 scope ruling; OQ-363 interim rule standing (AC-J3-9); OQ-338/OQ-342 headline-eligibility blocks (spec: stratum A carries Stage-3 acceptance regardless).

### Disposition
- [ ] PENDING — awaiting operator ruling: PASSED | SKIPPED-BY-OPERATOR | NOT PASSABLE
