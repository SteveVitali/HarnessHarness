# GATE-G3 readout — Stage-6 governed-evolution preconditions (all mandatory)

*Append-only. Each operator reading is a new dated block; never edit an earlier one.*

## Criterion (verbatim — the gate ticket quoting the spec's §9.7 gate)

All mandatory preconditions hold before the governed evolution pipeline runs (§9.7):

> a. Stage 3 complete — every engine op, MatchSpec, RemovalTest, held-out machinery; the
>    ready-to-evolve exemplar. [§5h.4, R-2.10.4, R-2.10.5]
> b. AT-H1-05 passing — the evolution service's delegate-class confinement demonstrated against
>    the D-5 handle set (fs_read, memory_write@run, model_call, exec sandboxed; no
>    permission_request, no delegable). [§05h, ADR-0053 D-5]
> c. ADR-0135's counterfactual hook and factual arm: paired baseline + counterfactual branches off
>    one fork point, `arm_role ∈ {factual, counterfactual}`, instrument-charged, `MatchSpec` on
>    every arm, the factual arm re-executes with its dispersion recorded as the report's noise
>    floor.
> d. `exp/` namespaces — the evolution service publishes only under `exp/<id>`; the `exp/<id>`
>    owner acts only through the service's delegate seat; deployment obeys §5h.8 + DF-S1.2-2's
>    governance checklist; extension lane revocable whole.
> e. the `retirement` kind + the §10.4 retirement diff + the ADR-0126 removal test — retire a
>    conditioned rule, swap a routing/budget primitive, retire a codemode surface, retire a
>    workload slice each via one removal experiment + one report.
> f. `plugin_abi/1` `subprocess_confined` — never in-process for evolution-service plugins
>    (§5.1h.3 hosting, ADR-0182 D8).
> g. the assumption-debt manager + retirement machinery — debt schemas, `RemovabilityIndex`, the
>    staged `removal_test` experiments (R-2.9.6⁰ᵃ/⁰ᵇ), and the Stage-3 retirement fixtures
>    (ADR-0209 D2).

**Blocks:** S6.1 through S6.4. No evolution claim runs until all of the above hold; every
Stage-6 claim is a `ComparisonReport{benefit_kind: artifact_benefit, label: confirmatory}` at
`matched_total` with a complete `SearchBudgetRecord`, `transfer` with sign and interval, no
veto regression, and retention within margin under §10.2's gates 1–6.

**Pre-registered thresholds.** *"Every precondition above is demonstrably met (each cites its
landed ticket/AC)."*

**DEFERRALS rule applied (docs/tickets/DEFERRALS.md rule 4):** *"Gates refuse to pass while any
`OPEN` row scoped to that phase remains. Treat an open row as gate-blocking."*

---

## 2026-10-02 — Reading 1 (orchestrator-authored evidence audit) → PENDING operator disposition

**Evidence sources:** the gate ticket `docs/tickets/091_GATE-G3__stage6-preconditions.md`;
`spec/CANONICAL_SPEC.md` §9.7/§10.2/§10.4; `docs/tickets/DEFERRALS.md` (97 rows — 33 DONE,
64 not-fully-discharged, all swept below); `docs/build/LEDGER.md` (CURRENT STATE +
GATE DECISIONS: GATE-G1 PASSED 2026-09-17, GATE-G2 PASSED 2026-09-25);
`docs/build/readouts/GATE-G1.md`/`GATE-G2.md` (disposition precedent);
`docs/build/BUILD_INDEX.md` rows 44–89 (Stage-3 through S5.8 landings);
`docs/build/runs/{S2.2,S3.4a–c,S3.6,S3.11a,S3.12,S3.12b,S4.1,S4.8,S5.4,S5.8}.md`;
the crate sources and test batteries cited per item (file:line);
the live suite evidence recorded below (258 blocks / 2813 passed / 0 failed).

### §9.7 preconditions (a)–(g) — verdict table

| # | Precondition | Verdict | Landed ticket / AC + executable evidence |
|---|---|---|---|
| a | Stage 3 complete (engine ops, MatchSpec, RemovalTest, held-out machinery, ready-to-evolve exemplar) | **MET** | GATE-G2 dispositioned PASSED 2026-09-25 (`LEDGER.md` GATE DECISIONS; `readouts/GATE-G2.md` Reading-2 operator block). S3.4a engine (AC-R-2.10.3-{1–8}, hh-experiment 29), S3.4b results store (AC-R-2.10.5, hh-results 14), S3.4c estimators (AC-R-2.10.4, hh-analysis 20), S3.3 bench/adapters + held-out (AC-R-2.9.4), S3.12b `benchset.stage3.v1` + exemplar→`ComparisonReport` legs. R-2.9.6⁰ᵇ staged `removal_test` machinery at item (e)/(g). |
| b | AT-H1-05 passing — delegate-class confinement vs the D-5 posture | **MET** | `crates/hh-monitor/tests/s3_11a.rs:665 at_h1_05_evolution_is_delegate_class_everywhere` (S3.11a, AC-R-2.8.1-5): (i) evolution/model-origin flow-policy widening refused `PolicyEditError::EvolutionOrigin`, model-delegate widening `WideningRequiresHuman`; (ii) `permission_request` from a Delegate-labelled proposer → deny unattended / ask attended; (iii) delegate-class security-label endorsement → `EndorsementError::IllegitimateEndorsement`; (iv) model/evolution-authored `Permission` → `MintError::IllegitimateIssuer` (`hh-monitor/src/mint.rs:113`) — the evolution service can never mint its own deploy authority. See caveat G3-1. |
| c | ADR-0135 counterfactual hook + factual arm | **MET** | `crates/hh-embed/src/replay_ops.rs:204 counterfactual` (S3.6, ADR-0283): paired factual+counterfactual arms off one `fork_point`, `branch_kind = counterfactual`, `arm_role`, `charged_to = instrument`, `MatchSpec` mandatory on every arm (`UnbudgetedArm`), `factual_arm` refusal enforced, factual dispersion → `comparison_ref` noise-floor record; `hh-ledger::replay::{ReplayDriverMode,ValidityMode,SourceRecord,ReplayValidityReport}` + `hh_control::replay::deterministic_replay`. Tests: `hh-embed/tests/conformance.rs::s3_6_counterfactual_*`, `hh-ledger/tests/replay.rs` (8). |
| d | `exp/` namespaces | **MET** | `crates/hh-identity/src/names.rs:27 Namespace::Experiment(String)` → rendered `exp/<id>` (:39); registry grammar + authority gate `crates/hh-registry/src/store.rs::register_namespace` — `exp/` admits ≥principal registrars, `hh`/`local` kernel-only; `crates/hh-registry/tests/s4_1.rs:1015–1084` exercises `exp/e1` register/publish/yank/status end-to-end (S4.1, ADR-0292). See caveat G3-2. |
| e | `retirement` kind + §10.4 retirement diff + ADR-0126 removal test | **MET** | `hh_lab::experiment::ExperimentKind::Retirement` + `ExperimentRefusal::NotARetirementDiff` (`crates/hh-lab/src/experiment.rs:969,1218`); removal test = paired matched-budget non-inferiority `ComparisonReport` + per-task regressed share → `hh_eval::compare::RemovalVerdict` + `hh_lab::debt::{settle_removal_test, retire, RemovalTestState}`; fixtures `crates/hh-lab/tests/retirement.rs` (6 tests — `every_minimal_profile_rule_registers_a_retirement_experiment`, `settle_and_retire_chain_over_the_minimal_profiles`, `retire_refuses_unevidenced_and_non_human`, `retirement_diff_classification_refusals`, …) + the engine-driven leg `crates/hh-embed/tests/s3_12b.rs:880 retirement_chain_is_engine_driven_end_to_end` (S3.12 R-2.9.6⁰ᵇ; S3.12b; ADR-0126). |
| f | `plugin_abi/1` `subprocess_confined` for evolution-service plugins (ADR-0182 D8) | **MET** | `crates/hh-varhost` is the `subprocess_confined` kernel side (`src/lib.rs:4`, `src/session.rs:46 Placement::SubprocessConfined`; S2.2, ADR-0264); registry admission refuses `in_process` for any non-`hh/` namespace — `crates/hh-registry/tests/plugin.rs:244 third_party_in_process_is_refused`, `s4_1.rs:272 ac9_third_party_out_of_process_resolves_in_process_refused`. `exp/<id>` namespaces are non-`hh/` by construction, so an evolution-service plugin can only ever place `subprocess_confined` (or a C1 stronger lane). Kit/isolation/protocol batteries: `hh-varhost/tests/{kit,isolation,protocol,package_variant,tracker_plugin}.rs`. |
| g | Debt schemas + staged `removal_test` experiments (R-2.9.6⁰ᵃ/⁰ᵇ) + Stage-3 retirement fixtures (ADR-0209 D2) | **MET** | C0/S1 slice (S1.24): `hh-ontology::debt` (`DebtHome`/`debt_home`/`DebtPolicy`/`ExpiryKind`/`RemovalTestKind`/`DebtStatus`/`RemovalVerdict`/`Verdict`), `hh-hir::records::AssumptionDebtRecord`, `hh-hir::debt::{RetirementRecord, validate_removal_test, validate_for_home, DebtError}` (AC-R-2.9.6-{1,2,8,9}). C0/S3 slice (S3.12): `hh-lab::debt` — `evaluate_debt`, `debt_index`, `route_notices`, `DebtIndexRow`/`DebtReport`/`DebtNotice`/`AssumptionDebtHealth` + the retirement fixtures of (e). C1 halves (S5.4): `DebtObservables` live triggers, `probation_overrun` fold, `guard_at_bind`, Group L `lab.debt.*` ops (`hh-embed/src/debt_ops.rs`; `hh-lab/tests/s5_4.rs` 19, `hh-embed/tests/s5_4.rs` 7; ADR-0315). See caveat G3-3 for the residual manager service. |

### Evidence detail — what is and is not being claimed

- **(a) Stage 3 complete.** GATE-G2's operator block (PASSED 2026-09-25, `readouts/GATE-G2.md`
  lines 135–142) is the standing disposition; nothing since has reopened it. The
  "ready-to-evolve exemplar" leg: `hh-lab::exemplars` drives both exemplars
  engine→`project_row`→`eval_run`→`analyze`→`hh_eval::compare` producing real
  `ComparisonReport{budget_match.status=matched}` rows (`hh-embed/tests/s3_12b.rs`).
  The `benchmarkSet` precondition is discharged (`benchset.stage3.v1` in the ledger).
- **(b) AT-H1-05.** The kernel-side battery is green and re-runs every suite. What is
  *not* claimed: the §05h-side named refusals `SealRefused{not_human}` /
  `SelfModificationRefused` — those symbols are the pipeline's own refusal spellings and
  land with the §05h machinery at 6a (AC-R-2.9.5-1); their substance (the evolution service
  cannot mint deploy authority, cannot endorse its own candidate, cannot widen its own
  policy) is what the S3.11a battery proves today. The D-5 root handle set itself is
  Stage-6 machinery — there is no evolution `AgentProcess` to seal yet; the precondition
  as written is "AT-H1-05 passing", and it is.
- **(c) ADR-0135.** The op exists at the embed boundary with the full refusal set; the
  replay determinism contract it stands on is `hh-ledger::replay` + `hh-control`'s driver
  (declared-`model_io` requirement, divergence → invalid, gap → degraded).
- **(d) `exp/` namespaces.** The namespace arm, its codec, and the registry admission rule
  are landed and tested. The criterion's second and third clauses ("the `exp/<id>` owner
  acts only through the service's delegate seat"; "deployment obeys §5h.8 + DF-S1.2-2's
  governance checklist") are Stage-6 obligations — the seat is minted when the pipeline
  exists; DF-S1.2-2 itself is a foreign-toolchain carry-forward (see sweep). Nothing in
  the landed layer contradicts them; the readout does not pretend they are exercised yet.
- **(e)/(g) retirement + debt.** The full chain — experiment kind, diff classification
  refusals, settle → removal-test → `RemovalVerdict` → human-sealed `retire`, debt report
  bucketing — runs end-to-end over the minimal profiles, engine-driven (no fed-in
  verdicts since S3.12b). What remains OPEN by design: the standing debt-manager
  *service* (sweep cadence, the probation ledger→`evaluate_debt` seam) — ADR-0209 D2's
  own staging, owned by S6.1b (DF-S5.4-1, DF-S1.24-1 residual cell).

### DEFERRALS sweep — 97 rows audited; 33 DONE; 64 not-fully-discharged, every one classified

No row was closed, amended or hidden by this gate. Under rule 4 the question is which OPEN
rows are *scoped to this gate's phase*. Two readings exist and the distinction is load-bearing
here — it is the operator's call (question 1 below):

- **Reading A (literal):** the gate's phase column reads "Stage 6" → any row whose owner sits in
  S6.x is "scoped to that phase" → the gate cannot pass while they remain OPEN.
- **Reading B (operative, the G1/G2 precedent):** gates disposition rows scoped to work that has
  *run*; a row whose only owner is a ticket this gate itself releases cannot produce evidence at
  gate time — G1 already carried an in-phase row (the R2 signature, "expected closure: this
  gate") as an accepted residual, and G2 carried Stage-4+ rows forward the same way.

**Class 1 — Stage-6-scoped (owner inside the phase this gate releases):**

| Row | Residual cell | Owner |
|---|---|---|
| DF-S5.4-1 (F) | The ledgered probation trigger surface — manager-side `probation_overrun` fold is landed; the ledger-append→consume seam is not | **S6.1b** (the debt-manager service ticket — itself blocked by this gate) |
| DF-S1.24-1 (F) | The residual cell of the measurement/Lab slice: the assumption-debt manager *service* (scheduling, sweep cadence, standing monitor) | **S6.1b** — every other named owner (S3.3, S3.4a–c, S5.4) has landed its half |
| DF-S1.15-1 (F) | Residual cells include "the §05f evolution tickets" (evolution-link/producer-resolution legs) | §05f = Stage 6 |
| DF-S1.14-4 (H) | The live exporter seam — `delivered/activated/followed` §05c/**§05f** emitter classes | §05f emitters = Stage 6 (part) |
| DF-S1.22-1 (F) | Judge independence/calibration runtime checks; `declare_*` op semantics where schema-only; the imported-experiment path (OQ-377) | S3.4a–c landed their halves; the residual rides Stage-6 judge integrity |

**Class 2 — CAP-scoped (capstone audit/chore tickets):**

| Row | Residual | Owner |
|---|---|---|
| DF-S1.9-4 (V) | Audit ran at S3.12b (29/74 ADRs carry the rung statement); row stays open until the finding lands | CAP.1 / next gate readout |
| DF-S3.12b-2 (F) | The rung-statement uniformity retcon (43 ADRs) | CAP.1 or a dedicated chore |
| DF-S3.12b-1 (F) | `measurement.experiment.closed` audit-cap overrun at exemplar scale (asserted via `assert_close_audit_cap`, never worked around) | "the first gate needing `closed` at exemplar scale" — deferred |

**Class 3 — human/operator prerequisites and governance rulings:**

| Row | Residual | Owner |
|---|---|---|
| DF-S4.10-1 (P) | HUMAN-H1 — the E3 ecosystem binding sign-off ("gate pending", never a failure) | operator |
| DF-S5.6-1 (P) | HUMAN-H2 — real tracker + signed webhook credentials; the `memory` fixture is the declared proxy (`provided: no`) | operator |
| DF-S0.3-3 (V) | R2 cross-camp human reviewer signature — carried forward since GATE-G1 | operator budget |
| DF-S4.13-1 (F) | OQ-388's ratification half | WS-B1/K2 (spec-governance act) |
| DF-S2.5-1 (F) | The custody/emission ruling | ADR-0213 resolution |
| DF-S1.24-3 (H) | `ModelProfile/2` dialect bump, or an ADR ruling the string form canonical | profile-compiler owner / ADR |

**Class 4 — foreign-toolchain / external corpus (cannot close hermetically; the G1/G2 carry set):**

DF-S0.3-2 (cross-impl polyglot halves), DF-S1.2-2 (cross-impl identity corpus — also item (d)'s
governance checklist), DF-S1.5-3 (ledger corpus), DF-S1.8-1 (registry corpus), DF-S1.27-1
(manifest corpus), DF-S1.13-3 (LT-03 live AgentDojo corpus arm — offline authorization covers
local only), DF-S1.24-2 (live foreign-manifest import residual).

**Class 5 — out-of-build-scope (deferred-ADR workstreams):**

DF-S4.2-1 (remote fetch transports — WS-L6; the manifest marks R-2.12.4 deferred per ADR-0210),
DF-S4.2-2 (remaining foreign vocabularies — per-system owners).

**Class 6 — residual cells of tickets that already landed (Stage ≤5); carried forward under the
G2 precedent:**

DF-S1.5-1 (retention/compression — rides DF-S2.9-1's bucket; retention ticket unscheduled),
DF-S1.9-2 (profile_binding constraints grammar, organisation layers C2, AC-CC-11 corpus),
DF-S1.11-2 (check-8 `check_write` substance landed at S4.8 — `hh-subagent/tests/s4_8.rs` Ok/
NotOwner/Fenced + deny audit; the depth-3 chain cell remains — S5.5 landed depth-2 T3 recursion),
DF-S1.12-1, DF-S1.12-2 (S2.4 mediator decision-order + amend halves — substantially landed,
never appended), DF-S1.12-4 (per-backend `lowering_loss` rows + four metrics emission — S3.11b ran),
DF-S1.12-6 (`tls.terminate`/`inspect_hooks` C2 halves — R-2.8.4² is an unstaged C2 slice per
§4.4's verification note), DF-S1.13-1 (mediated-delivery landed; credential_supply → DF-S1.13-4,
`net_egress` call site → DF-S2.4-1), DF-S1.13-4 (`wrapped_long_lived` delivery half — S3.11b/
S4.14b ran), DF-S1.14-1 (convention sink fixtures, hosted `n/a` sweep, per-configuration
catalogue), DF-S1.14-2 (post-S4.15 residual cells), DF-S1.14-3 (residual tail), DF-S1.15-3
(witness cosignatures, receiver receipts, per-event encryption — S4.14b landed 4/7 cells),
DF-S1.17-1 (regex-vs-NL form arm — needs a non-`native_fc` family), DF-S1.17-2 (C1 halves —
S5.2 ran), DF-S1.17-3 (C2 code-mode — unscheduled), DF-S1.18-1 (turn-loop/C1 residual cells),
DF-S1.19-1 (producer call sites), DF-S1.19-2 (labelled-corpus/`resume_set`/environment_epoch/
judged-conflict fixtures), DF-S1.20-1 (steerable `decide` interpreter, OOP conformance suite,
`workflow`/`program`, env-owned kill half), DF-S1.21-1 (postconditions/`diff_sanity` residual),
DF-S1.21-2 (OOP validator battery + computed-metric emission on live runs), DF-S1.21-3
(`CriticDeclaration{kind = judge}` binding on live judge calls — judge stays outside the kernel
under `offline-only`), DF-S1.22-2 (combined failure fixture, `search_budget = unknown`,
`LeakedSplit`, HAL `FaultType` levels), DF-S1.23-1 (`ApprovalOption.modify` C2 + `ApproverGrant`
records C1), DF-S1.23-2 (`DeclaredSource` scanners, `LegCrossing`/L3 runtime checks,
`TextHygieneReport` consumers — S4.14a landed the emitters), DF-S1.25-1
(`DefinitionInput::Ref` resolution — the publish-surface owner's discharge status is
bookkeeping-stale), DF-S1.26-2 (parked-detach `ExitClass` surface — S2.12 retained the interim
member), DF-S2.3-1 (`effect_terminal` producer + non-fleet `timer`/`retry_due` call sites,
`suspend`/`compensate`/`heal` entries), DF-S2.4-1 (member (a) + member (c) rebind — needs the
broker custody seam), DF-S2.4-2 (TLS production transport — unscheduled), DF-S2.4-3 (leak-scan
surfaces), DF-S2.7-1 (`labels_leaves` per-leaf walk, remedy-ingress producer, produce-time
caller), DF-S2.8-1 (producer call sites), DF-S2.9-1 (⁰ᵇ replay landed; retention/compression
machinery unscheduled), DF-S2.9-3 (snapshot cadence), DF-S2.10-1 (unbacked env verbs —
deepened at S5.8, still `stage_pending`), DF-S2.11-1 (`queue_next_turn` steer + OQ-316's
admission arm), DF-S4.11-1 (server-initiated MCP traffic arm — S5.7 landed the subscription
half), DF-S4.11-2 (H3 mediator `CallerAuth` resolution), DF-S4.11-3 (surface-permission reply
path — C2/C3 surface family).

### Workspace evidence recorded at this gate

| Check | Command | Result |
|---|---|---|
| Full workspace suite | `cargo test --workspace -j2` → `target/gate_g3_ws.log` | **258 test binaries / 2813 passed / 0 failed / 1 ignored; exit 0** — matches the standing baseline exactly |
| Removability | `bash scripts/check-removability.sh` (serial, post-suite) | exit 0 — all tier boundaries verified (S2.1/S2.2/S4.6/S4.9/S4.11/S4.12 legs; tier-0 refusals; hosting edges `[]`) |
| Build memory | `bash <skills>/build-memory/scripts/check-build-memory.sh .` | **0 violations**, 7 pre-existing warnings (manifest banner, ADR appendix drift, `nextTicket` ordering note, ledger-entry sizes, index column counts, pre-guard readouts, pre-Harness-header run ledgers) — none introduced by this gate |

### GAPS and honest caveats

**G3-1 · AT-H1-05's named-refusal spellings are Stage-6 machinery.** The battery proves the
substance (no evolution-minted deploy authority, no self-endorsement, no self-widening) but the
literal `SealRefused{not_human}`/`SelfModificationRefused` types do not exist in the tree —
they are the §05h pipeline's own refusals (AC-R-2.9.5-1), landing at 6a. S3.11a's ledger
records AT-H1-05 discharged; the readout repeats that claim only as substance.

**G3-2 · `exp/` governance halves are staged downstream.** The namespace arm + admission rule
are landed; the "owner acts only through the delegate seat" and §5h.8 deployment-checklist
clauses bind the Stage-6 pipeline, and DF-S1.2-2's cross-impl checklist leg is a carried
foreign-toolchain row. Not a gap in the precondition itself; recorded so no one reads (d) as
claiming them.

**G3-3 · The debt-manager *service* is intentionally absent.** ADR-0209 D2 stages the standing
service at S6.1b; the gate precondition is the schemas + staged experiments + fixtures, all
landed. But it makes DF-S5.4-1 and DF-S1.24-1's residual cell *Stage-6-scoped* — see the
readings under the sweep and operator question 1.

**G3-4 · `DF-S3.12b-1` names this gate's class of evidence.** The row's owner cell says "the
first gate needing `closed` at exemplar scale". GATE-G3 does not need an exemplar `closed` row
(the precondition is machinery, not a claim), so this gate does not unblock it — but the next
exemplar-scale close attempt will hit it. Operator may wish to assign the owner now.

### Pre-registered threshold check

*"Every precondition above is demonstrably met (each cites its landed ticket/AC)"* — all seven
preconditions are MET on landed, executable, test-pinned evidence (table above). The forward
claim-shape rule (`ComparisonReport{benefit_kind: artifact_benefit, label: confirmatory}` at
`matched_total` with complete `SearchBudgetRecord`, transfer with sign+interval, no veto
regression, retention within margin per §10.2 gates 1–6) is machinery-ready:
`hh-budget::MatchMode::MatchedTotal`, `hh-ontology::eval::SearchBudgetRecord`,
`hh-eval::compare::{RemovalVerdict, ExploratoryRun refusal, LeakedSplit, veto rows}`,
`hh-analysis` `confirmatory` label gates are all in-tree and exercised. Its binding target is
Stage-6 output, which does not exist yet — machinery presence is what this gate can and does
verify.

### Operator questions for disposition

1. **DF-S5.4-1 / DF-S1.24-1 residual — in-phase blocking or carried?** Both are owned by
   S6.1b, a ticket this gate releases. Reading A (literal rule 4) makes the gate
   NOT PASSABLE while they are OPEN — circular, since only S6.1b can close them. Reading B
   (the G1 in-phase carry-forward precedent) treats them as the released phase's own
   obligations, to be verified at GATE-ACCEPT. Recommended: rule them Stage-6-scoped
   carry-forward, with S6.1b's Definition-of-Done absorbing both.
2. **The carried-forward mass.** Classes 2–6 above (59 rows) are foreign-toolchain, human,
   governance, or residual cells of landed tickets — the same shape G2 accepted as carried
   deviations. Does the operator carry them again to GATE-ACCEPT, or insert a verification
   pass (the S3.12b pattern) for the machine-achievable bookkeeping cells before release?
3. **HUMAN-H1 / HUMAN-H2 remain unprovisioned** (`provided: no` recorded; no secrets in any
   ledger). They gate nothing in this criterion but are standing obligations for the
   operator.
4. **DF-S3.12b-1 owner** — assign the E-4 close-row shape owner, or accept deferral to the
   first gate/ticket that needs exemplar `closed` at scale.

### Disposition — PENDING

*An operator or authorized human record supplies the decision; an agent must not sign or
assume silence is approval.*

- [ ] PASSED
- [ ] SKIPPED-BY-OPERATOR
- [ ] NOT PASSABLE — what would pass it: ____________________

## 2026-10-02 · Operator disposition — **PASSED**

Operator ruled: **passed, proceed** — accept carried-forward residuals (G1/G2 precedent).

All seven §9.7 preconditions verified MET on landed, test-pinned evidence. The five
Stage-6-scoped OPEN deferral rows (DF-S5.4-1, DF-S1.24-1 residual, DF-S1.15-1,
DF-S1.14-4, DF-S1.22-1) carry forward as residuals — their owning ticket (S6.1b)
sits behind this gate, so a literal rule-4 reading would be circular; the operative
reading releases them. Caveats G3-1 (substance-only SealRefused spellings), G3-2
(`exp/` owner-seat clauses bind Stage 6, unexercised), G3-3 (debt-manager service is
S6.1b-staged) recorded for the accepting operator. Suite baseline: 258 blocks /
2813 tests / 0 failures. Recorded in LEDGER GATE DECISIONS. Stage 6 released.
