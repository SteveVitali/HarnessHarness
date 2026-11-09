# Evidence and readiness

What you can rely on today, per claim surface: the dated evidence behind
each statement, its limits, and the route to check it yourself. This page
keeps **useful-task**, **teaching**, **receipt/replay**, and **study**
readiness separate — a pass in one never upgrades another.

First value means a newly produced, independently correct artifact plus a
correct interpretation of its outcome — not connectivity, a terminal
event, or a fixture completion.

## Gate states

Every card below carries exactly one public state. These states sit on
top of the [evidence classes](status.md#how-to-read-the-evidence)
(`fixture-verified`, `live-executed`, `foreign-run-verified`,
`skipped(...)`, `environment-pending`); they do not replace them.

| State | Meaning |
|---|---|
| `Available` | Runnable/inspectable now, with the cited scoped evidence |
| `Not yet available` | The surface does not exist or its evidence has not landed; the reason is named |
| `Restricted` | Reachable only under stated access/authorization conditions |
| `Unsupported` | Refused or absent by design, with a typed reason |

<!-- spine:begin -->
## Choose your entry

Readiness is listed first so you can pick a route that matches what you
have and what each route can actually show.

- **Run a useful task** — `Not yet available`. No verified useful-task
  recipe exists yet: the bounded round-5 execution was run and
  recorded `failed`, and no independently checked task receipt has
  landed (see the [useful-task card](#card--run-a-useful-task-useful-task)
  below). The scripted offline run exercises the runtime with a fixture
  model; it cannot stand in for a useful artifact or a study result.
- **Inspect a recorded result** — `Available`. The offline quickstart
  produces a durable, inspectable run you can read, export, and replay
  ([fixture-verified](status.md#how-to-read-the-evidence); see the
  [receipt card](#card--inspect-a-recorded-result-replay-and-r0-receipt)).
- **Learn offline** — `Available`. Four versioned example cards run
  without a provider, credentials, or spend: setup→inspect→replay→verify
  (EX-01), a two-task two-arm Lab lesson (EX-02), a useful-task drive
  with sealed submission, oracle and the bounded cap-field edit (EX-03),
  and the confined-effect matrix with exhaustion/refusal controls
  (EX-04). All are labelled fixtures; the executed packets they pin
  retain their own evidence class.

Experienced integrators may enter the [reference](reference.md) directly.
No model, environment, or budget means a labelled receipt/fixture path —
the useful-task milestone stays unmet, it is not relabeled.

Every route ends in a real exit: a failure, refusal, or wrong output
leads to [troubleshooting](getting-started.md#troubleshooting), the
recorded trace, or a [sanitized report](../CONTRIBUTING.md) — none
requires a successful setup.
<!-- spine:end -->

## Evidence cards

Each card names its state, the dated scope it covers, the evidence
behind it, its limits, and the access or exit route.

### Card — Setup and scripted quickstart [setup]

- **State:** Available — scoped evidence
- **Scope:** connectivity, session/run lifecycle, and durable event
  output on the scripted lane, exercised by the committed-HEAD
  quickstart check (`scripts/check-quickstart.sh`, run in CI on the
  ubuntu leg) as of 2026-11-03.
- **Evidence:** [README offline quickstart](../README.md#try-an-offline-run);
  the recorded live variant
  [docs/build/reports/live/2026-10-23-quickstart-live.json](build/reports/live/2026-10-23-quickstart-live.json)
  shows a real provider call ending `format_failure` — terminal success
  is not task success.
- **Limits:** fixture model, no useful-artifact claim, no provider or
  environment coverage; exit zero alone is never the verdict.
- **Access:** the README block or
  [getting started](getting-started.md); failures route to
  [troubleshooting](getting-started.md#troubleshooting).

### Card — Learn offline: setup, inspect, replay, verify [ex-01]

- **State:** Available — scoped evidence
- **Scope:** the versioned `example-card/1` record
  [`scripts/examples/ex-01.json`](../scripts/examples/ex-01.json)
  (card version 2026-11-03.r5.29): build → attach by run id → six
  annotated excerpt checks → native export → clean-recipient R0 →
  replay admission, with tamper and stale-pin negative controls.
- **Evidence:** `cargo test -p hharness --test r5_29 ex01` (12/12 on the
  recorded landing); static check `scripts/check-examples.sh` /
  `--head`; full-offline lane `scripts/run-examples.sh`.
- **Limits:** the scripted `EmbedModel` lane is deterministic — the
  excerpts prove trace resolution, not model behavior; fixture
  teaching, never task/study/live evidence.
- **Access:** [the examples index](../scripts/examples/README.md) and
  [getting started](getting-started.md).

### Card — Learn offline: the two-arm Lab lesson [ex-02]

- **State:** Available — scoped evidence
- **Scope:** [`scripts/examples/ex-02.json`](../scripts/examples/ex-02.json)
  with pinned fixture data `ex-02-fixture.json`: two tasks × two arms ×
  five repetitions = 20 subjects through the real
  register/expand/open/next/claim/launch/grade/settle/close chain,
  projected rows, a persisted `finite_manifest_contrast` report and
  clean-recipient recomputation.
- **Evidence:** `cargo test -p hharness --test r5_29 ex02`; same static
  and offline lanes as EX-01.
- **Limits:** synthetic inputs — the contrast is a plumbing lesson, not
  an efficacy or study result; a one-task variant correctly reports
  `n/a{estimator_undefined}`.
- **Access:** [the examples index](../scripts/examples/README.md).

### Card — Learn offline: useful task, oracle and the bounded edit [ex-03]

- **State:** Available — scoped evidence
- **Scope:** [`scripts/examples/ex-03.json`](../scripts/examples/ex-03.json):
  a scripted-lane useful-task drive with a sealed submission, the
  unchanged deterministic oracle (wrong output → task fail; unavailable
  oracle → typed failure), the bounded `model_calls` cap-field edit
  through validate/compile/rerun, and the EX-01 receipt join — plus the
  pinned executed
  [hh-s1-useful/1 packet](build/reports/2026-11-04-r5.43-s1-useful.json),
  verified byte-for-byte.
- **Evidence:** `cargo test -p hharness --test r5_45 ex03`; static and
  offline lanes as EX-01.
- **Limits:** fixture/scratch posture — the cap edit proves the field
  is consumed and enforced (adaptation), never a causal-efficacy or
  live claim; the packet's honest dispositions are retained verbatim.
- **Access:** [the examples index](../scripts/examples/README.md) and
  [inspect a run](inspect-a-run.md).

### Card — Learn offline: the confined effect matrix [ex-04]

- **State:** Available — scoped evidence
- **Scope:** [`scripts/examples/ex-04.json`](../scripts/examples/ex-04.json):
  the §5 confined sink/effect matrix verified on the pinned executed
  [hh-ops-live/1 packet](build/reports/2026-11-04-r5.44-ops-live-second.json)
  (allow/Kp4, deny, pause/respond/resume, exhaustion; honest
  `undeterminable` probes; `discharged_predicates`), plus deterministic
  exhaustion and typed refusal legs on the scripted lane.
- **Evidence:** `cargo test -p hharness --test r5_45 ex04`; static and
  offline lanes as EX-01.
- **Limits:** input-required is not approval; scratch is not sandbox —
  the real podman cells ran under R5.44's authorized scope and the card
  does not re-run them.
- **Access:** [the examples index](../scripts/examples/README.md).

### Card — Inspect a recorded result: replay and R0 [receipt]

- **State:** Available — scoped evidence
- **Scope:** bundle export → clean-recipient `check_completeness` +
  `reproduce` (R0 = lifecycle recompute over the exported record), and
  supported recorded-input replay admission with prefix/marker rules.
- **Evidence:** the L4.6b boundary regression
  [`crates/hh-embed/tests/l4_6b.rs`](../crates/hh-embed/tests/l4_6b.rs)
  and the recorded replay
  [docs/build/reports/live/2026-10-23-l4-6b-replay.json](build/reports/live/2026-10-23-l4-6b-replay.json);
  EX-01's `export_verify_r0` and `replay_admission` legs.
- **Limits:** replay is not a fresh model drive; R0 is not independent
  scientific replication; the original Anthropic replay failure record
  stays a failure record — the repair is not a fresh end-to-end pass.
- **Access:** the receipt leg of EX-01 or `hh run export` /
  `hh bundle validate` on your own run (see
  [reference](reference.md)).

### Card — Run a useful task [useful-task]

- **State:** Not yet available — the bounded execution ran and failed;
  no verified useful-task recipe exists
- **Scope:** the S1 scenario — one unchanged upstream task, two fixed
  fresh subjects, an independently checked submission, and an actual
  annotated native receipt.
- **Evidence:** the executed
  [hh-s1-useful/1 packet](build/reports/2026-11-04-r5.43-s1-useful.json)
  records `disposition: failed` — the bounded drive ran under the
  round-5 admission and produced no independently correct artifact
  (`acceptance withheld{no_independent_correct_artifact}`). The
  operator-authorized 2026-11-05 re-execution on the repaired machinery
  recorded `disposition: executed` with `useful_acceptance: marked`
  ([packet](build/reports/2026-11-05-df-cap5.1-s1-useful.json)) —
  a bounded slice, not the full objective. S1 remains unmet in the
  latest fold; the re-execution awaits re-folding into this card's
  state — a promotion is its own gated ticket (the
  [phase-1 record](results/phase1-execution-surface-replay.md) carries
  the account). The scripted-lane fixture and the recorded live
  `format_failure` run are not useful-task evidence.
- **Limits:** a recorded attempt is not a passing receipt; this unmet
  objective is retained, not relabeled; a fixture or setup success
  never substitutes for it
  (see [the partial-release note](#dispositions-and-the-partial-release)).
- **Access:** watch [status](status.md) and this page; the report route
  below does not require a successful setup.

### Card — The real compaction study [study]

- **State:** Not yet available — the admitted study reported
  inconclusive; the efficacy objective stays unmet
- **Scope:** the S3 scenario — the central conditional HH-native
  two-arm finite-manifest study with an oracle, policy-specific parity
  admission, four matched caps, detectors, and a frozen analysis.
- **Evidence:** the
  [hh-study-report/1 record](build/reports/2026-11-07-r6.3-study-report.json)
  (`disposition: reported`) carries the honest verdict verbatim:
  `unmet{s1_objective}` with the fold's `s1_unmet` flag — the
  useful-task objective — `true`, `s3_unmet` (the parity predicate)
  `false`, and `s4_unmet` (the confirmation-cohort predicate)
  `false`. Its bounded legs really executed — activation and the
  pilot ran (10/10 pilot first-subjects, $0 recorded exposure) — the
  tranche's own `verified` verdict stands at
  [30/30](build/reports/2026-11-05-parity-tranche-3.json)
  (`tb` ↔ a reference runner, R=1) and the confirmation cohort is
  `executed{300_300}` as a four-epoch composite outside the frozen
  reattempt bound ([deviation record](build/reports/2026-11-07-s4-protocol-deviations.md)).
  The efficacy leg's `computed{strata:easy,hard,medium}` is 0 by
  construction — no efficacy result exists. The earlier withheld and
  failed records
  ([`hh-parity-n0/1`](build/reports/2026-11-04-r5.47-parity-n0.json),
  [`hh-confirm-withheld/1`](build/reports/2026-11-04-r5.48-confirm-withheld.json),
  [R5.59 fold](build/reports/2026-11-06-r5.59-study-report.json))
  stand as committed history; the
  [phase-1 record](results/phase1-execution-surface-replay.md) carries
  the account.
- **Limits:** an admitted inconclusive report is not a positive
  research claim — no efficacy, parity, sharp-null or adoption result
  exists; fixtures cannot fulfill this objective, and a sound
  null/negative admitted result would still be valid when collected.
- **Access:** status updates land on this page and
  [status.md](status.md) when gated evidence exists.

### Card — Bounded operations [operations]

- **State:** Not yet available — the bounded matrix executed; the
  S4 predicate stays unmet
- **Scope:** the S4 scenario — eight cells × three trials over the
  shared task/effect apparatus with an independent sink, stage
  denominators, and negative controls.
- **Evidence:** the second
  [hh-ops-live/1 packet](build/reports/2026-11-04-r5.44-ops-live-second.json)
  records `disposition: executed` under the bounded podman admission —
  all 24 planned attempts invoked and boundary-observed, 0 violations,
  honest `undeterminable` probes retained. The confirmation cohort
  executed as a four-epoch composite outside the frozen reattempt
  bound — the fold's `s4_unmet` flag (the confirmation-cohort
  predicate) is `false`, while the operations predicate stays unmet;
  the dated [deviation record](build/reports/2026-11-07-s4-protocol-deviations.md)
  is committed and the card's promotion stays its own gated ticket.
- **Limits:** a bounded execution is real evidence of the apparatus,
  not a `live-executed` record and not the S4 conclusion; named
  host/fault/threat assumptions only — no reliability or security
  rating.
- **Access:** [the operational test batteries](../crates/hh-env/tests/)
  for the fixture layer.

### Card — Execution-surface replay record [replay-record]

- **State:** Available — internal record; not outside-verified
- **Scope:** the phase-1 methodology record — what executed (parity
  tranches, the four-epoch S4 cohort, the study folds), the honest
  bounds, the dated corrections, the deviation record and the edition
  ADR.
- **Evidence:** the
  [phase-1 record](results/phase1-execution-surface-replay.md)
  (recorded execution — internal record) over the committed packets,
  the [records-in verifier](build/tools/p1_records.py) (RC-1…RC-6) and
  the emitted [cross-check](build/reports/2026-11-07-p1-surface-xcheck.json)
  and [estimator](build/reports/2026-11-07-p1-estimators.json) packets.
- **Limits:** internal record only — not outside-verified, not
  `live-executed`, not an admission or efficacy result; the tranche's
  own `verified` verdict is exact agreement over `tb` ↔ a reference
  runner, and the efficacy contrast is 0 by construction.
- **Access:** read the record; corrections land as dated addenda.

### Card — The results site [results-site]

- **State:** Available — internal record; not outside-verified
- **Scope:** the public results site at the recorded project URL —
  the pinned phase-1 pages (`/phase-1/`, `/phase-1/tasks/`) and the
  always-present pages (home, method, spend, limits, changelog,
  data, about). Phases 2–4 render their designed absent states:
  "not yet published — publication gate pending", no numbers.
- **Evidence:** the committed
  [`hh-site-publication/1` record](results/publication/phase-1-2026-11-09.json)
  pins the phase-1 input set (`inputs_digest`
  `sha256:15f819883d372ebe…`) and references the
  [GATE-G22 readout](build/readouts/GATE-G22.md); `check-site.sh`
  (every profile and `--head`) verifies the rendered claims and
  provenance.
- **Limits:** internal record; not outside-verified — public
  visibility upgrades nothing (no capability or evidence label is
  earned by the deploy). Deployment follows only the operator's
  merge to `main` through `pages.yml`; the served site is read
  back, never assumed.
- **Access:** <https://stevevitali.github.io/HarnessHarness/> —
  a phase without a publication record stays publication-gate
  pending; the dataset stays private.

### Card — Optional packaged component [component]

- **State:** Restricted — a conditional exercise, recorded unselected
- **Scope:** at most one `hharness-compact-evict-oldest` package
  exercise: build → seal → load → bind → invoke → verify eviction and
  provenance, with tamper and missing-helper negatives.
- **Evidence:** the package and host exist
  ([plugins/](../plugins/), `hharness-varhost`); the round-5 exercise
  was not selected — the
  [hh-plugin-unselected/1 record](build/reports/2026-11-04-r5.50-plugin-unselected.json)
  carries `unselected{stage_voided}` after the stage was voided for
  this round.
- **Limits:** no invocation has been observed; ABI conformance would
  support no provider-treatment or efficacy claim; an unselected
  exercise awards no claim and does not silently disappear.
- **Access:** unselected — see the
  [contribution routes](../CONTRIBUTING.md) for the gated lane.

### Card — External interchange [interchange]

- **State:** Restricted — optional and recorded unselected
- **Scope:** at most one external consumer/direction (the inspected
  candidate is the Inspect JSON direction), exercised with one real
  artifact, a preserved-field/loss map, and unsupported
  direction/version controls.
- **Evidence:** none executed — the
  [hh-interchange-unselected/1 record](build/reports/2026-11-04-r5.51-interchange-unselected.json)
  carries `unselected{stage_voided}`; existing conversion fixtures are
  not a real external-reader result.
- **Limits:** no return import is promised or claimed; an unselected
  direction awards no compatibility claim; imported foreign data does
  not gain native authority.
- **Access:** unselected — pending native readiness and a pinned
  consumer.

### Card — Downloadable evidence and outside reports [evidence-access]

- **State:** Restricted — access and rights conditions apply; the
  outside stage was withheld
- **Scope:** redistributable receipt bundles, sanitized report
  excerpts, and outside verification procedures.
- **Evidence:** the recipient-closure machinery (R5.27 handoff,
  profile/completeness checks) is engineered; the outside
  clean-recipient verify/recompute stayed withheld — the
  [hh-outside-withheld/1 record](build/reports/2026-11-04-r5.52-outside-withheld.json)
  carries `withheld{stage_voided}` and the participant/outreach
  predicate set stays open under [DF-H9-1](tickets/DEFERRALS.md); no
  report has been published.
- **Limits:** secrets/signed URLs/private paths/model I/O/solutions/
  oracles are inspected before anything is attached; nothing here is a
  public evidence badge.
- **Access:** the receipt and attach-evidence lanes in
  [CONTRIBUTING](../CONTRIBUTING.md).

### Card — Report a problem or contribute [contribute]

- **State:** Available — documented lanes with a static check
- **Scope:** the ordinary contribution routes — a no-run failure
  report, a wrong-run/receipt report with cleared evidence, a docs/PR
  lane, and the verifyfixed disposition — plus the explicitly gated
  component and replication routes.
- **Evidence:** [CONTRIBUTING.md](../CONTRIBUTING.md) lanes landed
  2026-11-03 (R5.31) with `scripts/check-contributing.sh` validating
  every lane, field, and privacy rule.
- **Limits:** routes exist; outside reports and replications are the
  evidence they would produce, which has not yet arrived.
- **Access:** [CONTRIBUTING](../CONTRIBUTING.md); private reports use
  [SECURITY.md](../SECURITY.md).

## The selected job

<!-- job:begin -->
This portfolio serves one selected primary job: a **technically
prepared harness evaluator** — an engineer or researcher comparing
useful task outcomes after changing one harness policy under fixed
resources, who needs to inspect *why* outcomes differ.

- **Prerequisites:** comfort with a source build and the pinned
  toolchain; for provider routes, a model endpoint and credentials of
  your own; for the Lab lesson, none.
- **Secondary roles:** integrators, replicators, and contributors share
  the same evidence — they enter at reference, receipt, or contribution
  lanes respectively.
- **Deliberate first-wave exclusions:** production/security
  certification, turnkey fleet or no-code service, universal
  runtime/client compatibility, an authored benchmark suite, a new
  leaderboard or viewer, and uniqueness claims. Broader fault/security
  campaigns, cross-framework compaction, and multi-model factorials
  need their own protocols.
- **Demand label:** the audience need is a *hypothesis* chosen from
  operator intent and source analysis — no outside demand or usability
  result has been established. That claim class maps to
  [research/04_audiences-and-jobs.md](research/04_audiences-and-jobs.md)
  and [§1 of the supplement](evidence-and-adoption-spec.md#s1).
<!-- job:end -->

## The capability portfolio

One primary job (above); every capability family keeps an explicit
disposition — first-wave slice, later group, or exclusion — with its
revisit condition. Nothing is silently deleted; a changed scope is
recorded, not dropped.

<!-- portfolio:families -->
### Families — 22

| Family | Entry point | Contract | Evidence / limit | Disposition |
|---|---|---|---|---|
| Run lifecycle and durable history | `hh run start/status/events/inspect/resume/cancel`; `EmbedService` | R-2.2.1; R-2.11.4 | Scripted boundary lifecycle; selected historical live bindings. Read success ≠ task outcome | First-wave — foundation for every observation |
| Definition assembly, identity and registry | `hh definition plan/apply/diff/explain/validate/identity/compile`; `hh registry` | R-2.1.2–4; R-2.10.1–2 | Assembly source ≠ bare HIR; typed diagnostics and fixture conformance | First-wave — pinned edit/validate/compile loop |
| Providers, model profiles, routing and caches | `model_port`; `GatewayModel`; `hh profile` | R-2.3.1–4 | Local-compatible, OpenAI, Anthropic lanes; selected real-call receipts | First-wave — one lane/model level and declared cache policy |
| Context building and compaction | `context_policy`/compaction bindings; `KernelAssembler` | R-2.4.1–2 | Real builder wiring with fixtures; a policy's existence is not a quality result | First-wave — the S3 canonical two-arm contrast |
| Retrieval, memory lifecycle and consolidation | `MemoryStore`, indexed retrieval, consolidation rules | R-2.4.3–4 | Boundary tests over durable procedure/memory rows; fake/recorded embeddings | Later — L1 |
| Skills and Procedure IR | procedure supplies; `hh/workflow`, `hh/program` | R-2.4.5 | Procedure selection/activation fixtures; not arbitrary imported-skill success | Later — L2 |
| Control and planning | `control_strategy` variants; durable steering | R-2.6.1–2 | Scripted driver/steering tests; unknown variants refuse | First-wave — fixed baseline control; comparative study later L3 |
| Delegation, topology and coordination | `hharness_subagent::spawn`; `Orchestrator` | R-2.6.3; R-2.6.5 | T0/T1/T2/T3/T6 admitted; T4/T5/T7 absent | Later — L4 |
| Value of computation / budgeted choice | `compute_policy`; `policy_for_configured` | R-2.6.4; R-2.1.6 | Policies selected in code; no measured efficiency result | Later — L5; fixed compute in S3 |
| Resource budgets and telemetry | budget definitions; `account`; cost/trace views | R-2.1.6; R-2.9.1 | Reserve-before-spend, attribution, matched budgets | First-wave — accounting in S1/S3, bounded limit in S4 |
| Authorization, approvals, containment and secrets | `hh approval`; `respond_permission`; env/helper; credential broker | R-2.8.1–7; R-2.5.5 | Typed permission/environment paths; focused attack/secret batteries; no general security certification | First-wave — S4 named deny/pause cases |
| Verification and independent critics | completion gate; validators; lane-backed judge adapters | R-2.7.1–3 | Scripted completion-gate and fake-provider judge tests; model claims are not authoritative grades | First-wave — S1/S3 executable oracle |
| Recovery and environment lifecycle | `suspend`, `compensate`, `heal`, `continue_goal`; `env.*` | R-2.2.3–5 | Fixture lifecycle/restart cases; snapshot/hibernate stays explicit unsupported | First-wave — S4 one fault window |
| Replay, branching and causal attribution | `fork`, `replay`, `counterfactual`; `lab.attribution.*` | R-2.2.4; R-2.9.7 | Reconstruct vs deterministic replay; fresh causal evidence is separate | First-wave — S1 selected replay; causal intervention later L6 |
| Benchmark integration and grading | `BenchmarkAdapter`; `grade` | R-2.9.4 | Fixture adapters; grade consumes supplied verifier output | First-wave — S1/S3 selected external task; highest execution uncertainty |
| Experiment scheduling and matching | `lab.experiment.*`; CLI `experiment` | R-2.10.3 | Durable launch/settle infrastructure; some CLI operands cannot reach required params | First-wave — S2/S3 one supported runner |
| Evaluation, results, comparison and analysis | `lab.eval.*`, `lab.results.*`, `lab.analysis.*`; Rust analysis kernel | R-2.9.2; R-2.10.4–5 | Class-aware results and estimator plumbing; typed n/a/refusals | First-wave — S2/S3 kernel and reports; new public ranking surface excluded |
| Portable bundles and import/export | `hh bundle create/validate/reproduce/show`; `hh run export/import` | R-2.9.3 | One inspected clean-store receipt is R0; no fresh-execution inference | First-wave — S1/S5 native closure and corrected R0 |
| Plugins and external harnesses | `plugin_abi/1`; `VariantHost`; `lab.extension.*` | R-2.12.2; R-2.8.5; R-2.10.6 | Packaged binary flow and hosted fixture composition | First-wave contingent — S5 one package; external participants later L7 |
| Fleet and external work sources | `fleet.*`; CLI view/items/reconcile/observe | R-2.12.6 | C4 boundary, fixture source, signed-webhook lane selection | Later — L8 |
| Evolution, debt retirement, model drift and co-evolution | `lab.evolution.*`; debt/model/coevolution families | R-2.9.5–8 | Record-driven governed services, some C4-gated; no weight training | Later — L9 |
| Web, MCP, ACP, SDK and foreign conformance | loopback HTTP; generated clients; `hh-web`, `hh lab serve`, `hh acp`, `hh-xcheck` | R-2.5.4; R-2.11.2–4 | Artifact MCP catalog vs Lab MCP execution are separate; bounded foreign corpus | First-wave — one supported SDK/RPC/CLI route; S6 optional exchange |
<!-- portfolio:families:end -->

<!-- portfolio:later -->
### Later groups — nine

| Group | Question | Revisit driver |
|---|---|---|
| L1 — memory/retrieval correctness before efficacy | Retrieve the right durable item without stale/invalid/leaked memory? | deterministic lifecycle/retrieval assertions, then matched task comparison; pinned corpus, leakage detectors |
| L2 — one skill/procedure before a strategy survey | Execute and adapt one admitted procedure with observable control? | real workflow/program ports and a versioned procedure |
| L3 — control/planning/context builders | Does a richer strategy improve this task family under the match mode? | canonical control design first; isolated factors and repeats |
| L4 — delegation and topology | Does an admitted team beat a matched solo configuration, and where does coordination fail? | bound delegate ports, supported topology, total team accounting |
| L5 — value of computation/constrained optimization | When is extra computation worth its total cost? | matched caps/frontiers, split discipline, full selection accounting |
| L6 — branching/causal attribution | Did one recorded decision cause a different downstream result? | executed counterfactual arms under a declared intervention |
| L7 — external harnesses/benchmark participation | Can an outside runner consume tasks or receipts with known losses and parity? | pinned runner/adapter conformance and parity first; Harness-Bench manifest discrepancy unresolved |
| L8 — fleet/work-source operations | Can an operator reconcile one external work source accountably? | confined source/webhook trials, opt-in C4, credentials, human gates |
| L9 — retirement, drift, evolution/co-evolution | Does a rule still earn its cost after a model change? | old/new snapshots, full search spend, applicable C4 gates; no weight-training claim |
<!-- portfolio:later:end -->

<!-- portfolio:scenarios -->
### Scenarios — six

| Scenario | Job | Current state |
|---|---|---|
| S1 — useful task, inspect, replay, share | One unchanged upstream task, two fixed subjects, independently checked submission, annotated native receipt | **Not yet available** — bounded execution `failed`, machinery repaired; the 2026-11-05 re-execution executed with `useful_acceptance: marked` (bounded slice; DF-CAP5.1-1 done); S1 remains unmet in the latest fold — promotion is a gated ticket |
| S2 — offline Lab | Two synthetic tasks × two arms × five repetitions through the real chain | Available — fixture-verified (EX-02 card) |
| S3 — compaction | The central conditional two-arm finite-manifest study | **Not yet available** — the tranche's own `verified` verdict stands at 30/30 (`tb` ↔ reference runner, R=1 — `parity_admission` refuses the design); the efficacy contrast computed `computed{strata:easy,hard,medium}` but is 0 by construction; the S3 study objective stays unmet |
| S4 — operations | Eight cells × three over the shared apparatus with an independent sink | **Not yet available** — bounded matrix executed (24/24, 0 violations); the confirmation cohort executed 300/300 as a four-epoch composite outside the frozen bound (dated deviation record); unmet retained |
| S5 — participation | Fixed no-run bug, verifyfixed, verify/recompute paths | Restricted — lanes documented (contribute card); stage voided for the round; component exercise recorded unselected |
| S6 — interchange | At most one external consumer/direction | Restricted — recorded `unselected{stage_voided}`; optional after native readiness |
<!-- portfolio:scenarios:end -->

### Dispositions and the partial release

<!-- unmet:begin -->
This page describes a **partial release**. The S1 useful-task and S3
compaction-study objectives are **unmet**: recorded, not inferred —
the bounded S1 execution ran and failed (the machinery was repaired,
and the 2026-11-05 re-execution executed with `useful_acceptance:
marked` — a bounded slice; S1 remains unmet in the latest fold and the
re-execution awaits re-folding into the card state), and the S3 study
objective (a finite-manifest efficacy result) stays unmet — the
tranche's own `verified` verdict stands at 30/30 (`tb` ↔ a reference
runner) and the computed contrast is 0 by construction, not an
efficacy result. No
checked-task minimum has
been retained by substitution — the fixture cards are labelled teaching
and explicitly cannot satisfy either objective. Publishing this reader
surface while those objectives remain open is a scope disposition
recorded by the operator through the round-5 admissions (the GATE-G8
bootstrap scope, the GATE-G9 seven-row admission, the GATE-G10
narrow offline-evidence admission, and the GATE-G13–G16 withheld/voided
stage dispositions — all in the
[GATE DECISIONS](build/LEDGER.md) of the build ledger) and the
[round-5 partition](build/planning/2026-10-30_evidence-decomposition.md).
The fresh round-5 acceptance,
[GATE-G17](build/readouts/GATE-G17.md) PASSED 2026-11-05T00:13:07Z,
took the record as-landed: engineering `executed`, research
`inconclusive`, portability/public/adoption `withheld`, build
`not_done` — with DF-R5.1-1, DF-H9-1 and DF-CAP5.1-1 carried open and
no merge or publication permission granted. A fixture or setup success
is never first useful value, study proof, or outside comprehension;
absent provider and wire capabilities stay explicit. No readiness
claim on this page precedes gated evidence.
<!-- unmet:end -->

Every disposition change is recorded: this table and the
[frozen supplement §1](evidence-and-adoption-spec.md#s1) agree, and a
divergence is a defect to correct, not a quiet edit.

## Where each claim class comes from

<!-- claims:begin -->
| Claim class | What it covers here | Source of record |
|---|---|---|
| Benefit | Why each first-wave surface serves the selected job | [research/04_audiences-and-jobs.md](research/04_audiences-and-jobs.md) (B1 findings); [design/06_demo-and-study-portfolio.md](design/06_demo-and-study-portfolio.md) (C1 F1–F7); [supplement §1](evidence-and-adoption-spec.md#s1) |
| Overlap | Where benchmark/inspection tooling overlaps the job (TB2, Harbor boundaries, Inspect evaluation/log tooling, Harness-Bench, TraceTrial, GEPA-like search) | [research/05_ecosystem-and-benchmarks.md](research/05_ecosystem-and-benchmarks.md) (B2); [supplement §1.4](evidence-and-adoption-spec.md#s1) |
| Exclusion | The deliberate first-wave exclusions and scoped-out campaigns | [supplement §1.3](evidence-and-adoption-spec.md#s1); [design/06](design/06_demo-and-study-portfolio.md) F7 |
| Revisit | The L1–L9 cumulative prerequisites and revisit conditions | [supplement §1.3](evidence-and-adoption-spec.md#s1); [design/06](design/06_demo-and-study-portfolio.md) F7 |
| Capability matrix | The 22-family inventory, entries, contracts, and limits | [research/01_capability-inventory.md](research/01_capability-inventory.md) (A1 F1–F12); [supplement §1.2](evidence-and-adoption-spec.md#s1) |
| Evidence classes | What each verification label means | [status.md](status.md#how-to-read-the-evidence); [MAINTAINING.md](../MAINTAINING.md) |
<!-- claims:end -->

## How claims graduate

Claims on this page move in the gated order: internal recipe/negative/
closure readiness → one outside clean verify/recompute → formative
review → an operator capability-publication decision → a separately
gated scientific report → manual day-30/day-90 review. Capability
sections mark scientific fields inapplicable rather than filling them
with fixture results; no fixture, hash, tag, or distribution creates a
scientific or public evidence badge. Historical negative results stay
visible — see [status.md](status.md) and the
[build records](build/README.md) for the dated account.
