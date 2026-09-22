# WS-J3 — register additions sidecar (temporary ids; synthesis renumbers from S-499 / OQ-346 / CF-322)

## Sources

| temp-id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-504 | Li, Jamieson, DeSalvo, Rostamizadeh & Talwalkar (JMLR 18, 2018), *Hyperband: A Novel Bandit-Based Approach to Hyperparameter Optimization* — successive halving with brackets; adaptive resource allocation + early stopping; non-stochastic infinite-armed bandit | paper | A | 0 | P (abstract) | no | https://arxiv.org/abs/1603.06560 | J3, I5, F4 |
| S-505 | Weights & Biases docs — *Sweep configuration keys* (`method ∈ {grid, random, bayes}`; parameter `values/value/distribution/min/max/mu/sigma/q/probabilities`; `early_terminate{type: hyperband, min_iter, max_iter, s, eta (default 3), strict}`; `run_cap`) | doc | B | 2 | P | no | https://docs.wandb.ai/guides/sweeps/sweep-config-keys/ | J3, K1, K2 |
| S-506 | Fractional factorial design (reference page; Montgomery, *Design and Analysis of Experiments* 8th ed. 2013; Box, Hunter & Hunter, *Statistics for Experimenters* 2nd ed. 2005) — `l^(k−p)` notation, generators, defining relation, aliasing, resolution III/IV/V, sparsity of effects | paper (reference page) | A | 0 | P | no | https://en.wikipedia.org/wiki/Fractional_factorial_design | J3, J4, I2 |
| S-507 | Hydra docs — *Multi-run* (`--multirun`/`-m`; comma lists → cartesian product; sweeper vs launcher plugin split; serial default; `hydra.mode=MULTIRUN`) | doc | B | 0 | P | no | https://hydra.cc/docs/tutorials/basic/running_your_app/multi-run/ | J3, A5, K1 |

Existing S-ids opened directly in this workstream (promote S→P where still `S`; add `J3` to `feeds` where absent): **S-062** (abstract re-opened), **S-082** (abstract + HTML re-opened), **S-108** (`SPEC.md` §4.1.5–4.1.8, §8.1–8.6, §14.3), **S-114** (`sweagent/run/run_batch.py`, `batch_instances.py`, `_progress.py`, `remove_unfinished.py` @ 3ea751c), **S-115** (`src/minisweagent/run/benchmarks/swebench.py` @ 04d809c), **S-132** (`src/harbor/job.py`, `job_plan.py`, `trial/queue.py`, `models/job/config.py` @ 7d5285b), **S-134** (`hal/agent_runner.py` @ 16bb03e), **S-136** (`src/inspect_ai/_eval/evalset.py`, `eval_set_manifest.py`, `eval_set_pruning.py`, `run.py` @ 75f4891), **S-185** (via ADR-0025). Cited without re-opening: S-083, S-093, S-135, S-167, S-234, S-441, S-488.

## Open questions

| temp-id | question | resolver | due | blocking? |
|---|---|---|---|---|
| OQ-359 | Experiment-ledger growth: chain length at which an experiment run continues by `continued_from` (ADR-0131), and whether the `kind: experiment` run is the same ruling as B3's inbox run (OQ-314). | WS-B1 with WS-B3, WS-J3 | Phase 3 synthesis | yes (ADR-0155 item 1) |
| OQ-360 | Re-attempt defaults: `max_per_plan = 2`, `max_fraction_of_plans = 0.10`, backoff constants; measured at Stage 3 against the observed infrastructure-failure rate per environment class; interaction with OQ-121/OQ-339 replicate defaults. | WS-J3 with WS-I2, WS-B5 | Stage 3 measurement | no |
| OQ-361 | Fractional designs over categorical multi-level factors: which orthogonal-array families (beyond two-level `l^(k−p)` generators) are admitted, how aliasing is reported for them, and whether WS-J4's estimators accept them. | WS-J4 with WS-J3 | Phase 3 | no (C1) |
| OQ-362 | Drift-bracket cadence for multi-day experiments (open/close only vs periodic re-probe), its instrument-budget cap, and whether a mid-experiment fingerprint change pauses scheduling or only annotates. | WS-C1 with WS-J3, WS-I2 (extends OQ-286/OQ-300) | Phase 3 | no |
| OQ-363 | May an `iso_cost` arm share subject runs with a `matched_cap` arm when their hard limits coincide (one run, two reporting views), or must the engine duplicate runs to keep arms disjoint? Affects the control-strategy exemplar's Stage-3 cost. | WS-L2 with WS-J3, WS-I2 | before Stage 3 execution of `lab/control-strategy-family-v1` | yes (AC-J3-9) |
| OQ-364 | Experiment-level cost ceilings per kind (`comparative`, `equivalence`, `retirement`, `reproduction`) as packaged defaults; who may raise them; relation to the HAL cost precedent. | WS-J3 with WS-L2, WS-L6 | Phase 3 | no |

Answered for neighbours: **OQ-076** — yes, always an `experiment` layer; ad-hoc overrides are implicit `exploratory` experiments (ADR-0154 item 4). **OQ-078** (J3 half) — pinned `profile_binding` ⇒ `non_portable = true`; excluded from cross-profile factors; `ProfilePinnedAcrossProfiles` refusal (ADR-0154 item 5). **OQ-124** — engine default 10 % per dimension as packaged data. **OQ-097** — consumed as resolved (ADR-0128). **OQ-121/OQ-339** — exemplar Stage-3 sizes n = 5 recorded as inputs.

## Conflicts

| temp-id | parties | contradiction | proposed resolution |
|---|---|---|---|
| CF-331 | ADR-0026 event table (`measurement.experiment.bound{experiment_id, arm_id, configuration_id}` only) vs ADR-0155 | The engine needs `cell_id, replicate_index, attempt_no, budget_id` on `bound` and eleven new `measurement.experiment.*` classes (`registered, run_planned, run_claimed, claim_expired, run_launched, run_settled, run_replanned, paused, resumed, drift_bracket, closed, bundled`) | Payload extension + class additions in the ADR-0026 amendment log; envelope unchanged; four classes audit-grade (WS-H6) |
| CF-332 | ADR-0026 "one ledger per run" as read by WS-B3 (OQ-314 for inbox runs) vs ADR-0155 (experiment as a strategy-less run) | Whether a run without a control strategy or effects is a legitimate run kind | One ruling for inbox and experiment runs: admit `kind ∈ {agent, inbox, experiment}` on the run manifest with `continued_from` chains for length; no second store |
| CF-333 | Harbor `job.py` L268–270 / `trial/queue.py` L221, SWE-agent `run_batch.py` L389–406, mini `swebench.py` L131–133 (delete partial state on resume/retry) vs ADR-0026/0037 immutability and ADR-0155 item 5 | Precedent runners delete; the instrument never deletes | Recorded as rejected precedent; superseded runs stay with `superseded_by`; consumption reported as instrument waste |
| CF-334 | ADR-0105 decision 1 ("`MatchSpec{… matched_cap}` plus one `iso_cost` arm") vs ADR-0041 refusal of mixed modes in one comparison | Two `MatchSpec` modes in one design | Read as two arms with distinct `MatchSpec`s that are never compared directly (`IncommensurableMatch`); shared-run question is OQ-363; WS-F1/L2 to confirm |
| CF-335 | ADR-0130 closed lease-scope sum `{writer, effect, resource, environment, wakeup}` vs the engine's need for a run-plan claim | Growing a closed sum vs reusing `resource(key)` | Reuse `resource(run_plan_id)`; no dialect bump; WS-B3 to confirm the key convention |
| CF-336 | HAL `agent_runner.py` L196–200 (resume treats `"ERROR…"` strings as not completed) vs ADR-0045 outcome classes | "unmeasured ⇒ failed" as a resume hazard | Recorded as rejected precedent; settlement is by outcome class only |

## Ontology terms

| term | definition | notes |
|---|---|---|
| experiment spec | `ExperimentSpec{dialect hh-experiment/1, kind, design, pre_registration, factors, arms, suite, replicates_per_cell, seed_policy, validation_strategy, scheduling, reattempt, budgets, bundle_policy, ext}`; content-addressed (`experiment_id`); immutable after `register`; amended by supersession. | ADR-0154; MUST-data |
| experiment kind | `comparative | exploratory | equivalence | retirement | reproduction` — constraint tables over one engine; `exploratory` rows are `comparable = false`. | ADR-0156 |
| cell plan / run plan | `expand(spec)` output: cells `(arm, configuration_id, configuration_version_id, task, split_label)` and run plans `(cell, replicate_index, seed_material, environment_derivation, cache_scope_salt)`; content-addressed `plan_id`. | ADR-0154 |
| run_plan_id | `H(experiment_id ∥ arm_id ∥ configuration_version_id ∥ task_id ∥ replicate_index)`; the dedup key for exactly-once settlement; excludes scheduling/transport fields. | ADR-0154/2; Inspect `task_identifier` precedent |
| attempt number (`attempt_no`) | Ordinal of the runs opened for one run plan; orthogonal to `replicate_index`; only `infrastructure_failure` (and declared operator/hosting cancels) increment it. | ADR-0155 |
| accepted run / superseded run | The one run per plan whose outcome class is final vs a run re-planned under the re-attempt policy; superseded runs are never deleted and their consumption is instrument waste. | ADR-0155 |
| re-attempt policy | `ReattemptPolicy{max_per_plan, max_fraction_of_plans, backoff, error_classes_included?, on_cancel}`; MUST-data. | ADR-0155 |
| experiment run | A run of `kind: experiment` with no strategy or effects whose ledger carries `measurement.experiment.*`; every scheduler view is a projection of it. | ADR-0155; CF-332 |
| scheduling policy / pool / order plan / permutation seed | `SchedulingPolicy{max_concurrent_runs, pools[{key, limit}], order, permutation_seed, start_stagger_ms, deadline?, priority?}`; pools are over resource classes (model snapshot, environment class, instrument, participant); `OrderPlan` is reproducible from the seed. | ADR-0155 |
| interleaved-blocked order | Default order: the task is the block; arms visit each block in a seeded random permutation. | ADR-0155; classical randomisation within blocks |
| drift bracket | Fingerprint probes (ADR-0120) at `open_experiment` and `close`; a change annotates rows `provider_drift = observed`. | ADR-0155 |
| experiment budget node / instrument budget node | Two ADR-0040 nodes with `scope: Experiment`: the subject `pool` bounding Σ `eval_budget`s and the instrument node for graders, probes, detectors and re-attempt waste. | ADR-0155 |
| limits_enforced | `full | partial` on an arm: `partial` when a hosted level's token/spend limits are reported rather than enforced (wall-clock/`env.*` enforced at the boundary). | ADR-0155; ADR-0046 (d) |
| validation strategy | `full_set | disagreement_weighted{lambda, min_inclusion_fraction, estimator} | successive_halving{eta, min_budget, brackets}`; the latter two search-only, split-scoped, `estimated`-labelled. | ADR-0156 |
| comparable (flag) | Row/arm flag; `false` for `exploratory` experiments; `compare` rejects `false`. | ADR-0154 |
| fractional design generators / resolution / aliasing table | `generators[]`, `resolution ∈ {III, IV, V}`, and the derived aliasing table stored in the cell plan; refusal when a pre-registered effect is confounded at the declared resolution. | ADR-0154; S-506 |
| reference-only arm | An arm reported with `budget_match.status` but excluded from the primary contrast (e.g. `window_ceiling` in the compaction family). | ADR-0156; ADR-0077 |
| experiment kill points (KP-E1…E5) | Injected engine death between `run_planned`/`claim`, `claim`/`open_run`, `open_run`/`bound`, subject `finished`/`run_settled`, and during `close`; oracle = exactly one accepted run per plan. | ADR-0155; extends ADR-0132 |
