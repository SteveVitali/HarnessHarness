# 37_sweep-method — One-factor-at-a-time sweep and held-out confirmation method
Stream: D
Researched: 2026-11-06T23:59Z – 2026-11-07T00:17Z (inspection, local simulation and writing)
Author: design worker for ledger row D3 (Claude Code subagent, claude-opus-5-5), dispatched by the experiments-program coordinator
Ledger rows covered: D3
Repository baseline: main 09fe8523. Read in worktree `claude/experiments-spec` at HEAD 49d6e69a. `git diff --stat 09fe8523 HEAD -- crates scripts spec` is empty, so every code locator below holds at 09fe8523.
Confidence overall: high for what the landed Lab, assembly and compaction code admit (source inspection). Medium for the factor registry, the exercise analysis and the selection rule: these are design proposals resting on accepted notes. Medium-low for the precision and cost tables: they come from local simulation under stated assumptions, with cost priors from C1/C3, not measurements. Low for real configuration effect sizes, which nobody has measured on this participant and surface.

**Answer in brief.**

- **What is swept.** Phase 4 sweeps the HH-native mini-swe-agent baseline. That baseline is the frozen H record that passed G-L1. Its label depends on the L2 outcome: "parity-verified" if L2 returned `equivalent`, otherwise "L1-conformant baseline, verdict parity inconclusive" (D2 F16).
- **Tier-1 registry.** Nine levels across seven factors (Part 1):
  - reasoning effort: `low` and `max`, around the phase-2 pin;
  - exec command timeout 120 s;
  - observation truncation threshold 2,500 characters;
  - dropping re-sent reasoning text;
  - a prompt ablation that keeps the submit instruction;
  - a bash + `edit_file` tool surface (conditional on an extra capability ticket);
  - two compaction levels. Each compaction level is a composite: strategy S **under a declared context-window cap W**.
- **Why compaction needs a declared cap.** At the 1,048,576-token catalog window, compaction cannot fire on Terminal-Bench 2.1, so it is not a factor there. The cap is a declared harness variable, chosen by a pre-registered rule from non-outcome token records and applied identically to every task. It is never padding.
- **Reserved.** Seven candidate levels are registered but not run, each with a stated reason: step limit, format-error cap, observation truncation off, context policy, text-action interaction mode, temperature, and cost limit.
- **Exploration.** First a 30-trial feasibility smoke (X0). Then one pre-registered OFAT batch (X1): base plus nine single-level perturbations on the 11 band-A development tasks at k = 5, which is 550 trials and about $28–$187 of model spend at the $0.05–$0.34 per-trial prior. Exploration is descriptive only.
- **Selection.** A mechanical rule picks at most two configurations, one per factor. Each must have a positive exploration estimate, pass the activation, fidelity, completion, integrity and cost gates, and be selected by a rule frozen before any exploration outcome exists. A combination of the two can be added only if pre-declared.
- **Confirmation.** It re-runs the baseline beside the selected configurations on the **37-task band-A held-out census** at k = 5: 555 trials for two configurations, about $28–$189. It uses D1's finite-manifest estimand and Holm α = .05 over the selected members.
- **Simulated properties.** Directional false confirmation stays at or below 2.6% in every scenario. The exploration estimate of a selected configuration is optimistic by about 3–10 points (winner's curse). The confirmation estimate is unbiased.
- **Likely outcome.** Power for a true 5-point gain is only 0.13–0.26 with at most two members. For realistic knob effects the most likely confirmatory result is therefore "not confirmed". That result is publishable, and it is never worded as "no difference".

## Scope, sources and method

D3 designs phase 4's method. It does not design the runner (B2), the proxy (C2), the HIR closures (E2), parity (D2), compute (B3) or the site (F2), but states what it needs from each.

Nothing was executed except local, no-network Python simulations in the session scratchpad. There was no spend, no provisioning, no container, no build, no repository script and no git branch movement.

`bash scripts/deferrals-open.sh` was **not** run, because this row's assignment limits execution to local Python simulation. I rely on E2's record that it printed no `DF-*` row at baseline and that BL-50…53 touch none of these seams (`docs/design/38_hir-native-gaps.md`, "Repository instructions"). That is inherited evidence, not my own check.

| ID | Source (locator at baseline 09fe8523 unless noted) | Read (UTC) |
|---|---|---|
| R1 | `AGENTS.md`, `crates/AGENTS.md` (system context); `docs/experiments-brief.md` §1–§5 incl. A-8; `docs/experiments-research-ledger.md` (row D3, §Q Q-X1…Q-X42, Execution contract, standing limits); `docs/research/CONVENTIONS.md`; `docs/experiments-OUTLINE_TRACE.md` rows OL-07 (l.16) and OL-15 (l.24) | 2026-11-06T23:59Z–2026-11-07T00:00Z |
| R2 | `docs/design/35_comparative-method.md` (D1), l.1–800: answer, simulation record, F1–F17, closing sections, Appendix B code through `calib` | 00:00Z–00:01Z |
| R3 | `docs/design/36_hir-parity-method.md` (D2): header and inputs (l.1–40), Part 2 §2.1–2.5 (l.233–330), Part 5, Part 6, Part 7 (l.474–620), F4–F5 (l.653–672), F16, closing sections (l.773–880) | 00:01Z–00:02Z |
| R4 | `docs/design/38_hir-native-gaps.md` (E2): header and inputs (l.1–30), Part 2 gap table G1–G23, Part 6 §6.1–6.4, Part 7 ticket table, F11–F13, requirement rows REQ-XE-122…131 and handoff lines | 00:02Z–00:03Z |
| R5 | `docs/research/26_ollama-glm.md` (C1): finding headings; F4, F5, F9, F11, F12 bodies; endpoint table rows; REQ-XC-002, -009, -014, -015 (grep-located, read in context) | 00:03Z |
| R6 | `docs/research/28_reference-harness-anatomy.md` (E1): finding headings; limits table (l.41–105); the template excerpt (l.110–131); the observation template and exec timeout (l.132–211); Terminus summarization (l.368–410); F10 (l.603–612) | 00:03Z–00:12Z |
| R7 | `docs/research/27_harness-survey.md` (C3): TB 2.1 table (l.63–78), planning table (l.410–422) | 00:04Z |
| R8 | `docs/evidence-and-adoption-spec.md` §2.3–§4.3 (l.174–282), especially l.234 (no artificial pressure/padding), l.238 (pilot limits; "Tuning requires a separate exploratory design"), l.252 (activation/fidelity), l.275 (sign flips, Holm), l.277 (simulation battery), l.227 (AC-3-2) | 00:04Z |
| R9 | `crates/hh-lab/src/experiment.rs` l.100–205 (`LevelSpec`, `FactorSpec`), l.1580–1680 (design-kind validation incl. `OneFactorAtATime` at l.1650–1672), l.2000–2020 (per-arm rebinds) | 00:04Z |
| R10 | `crates/hh-ontology/src/eval.rs` l.1321–1346 (`FactorKind`), l.1731–1751 (`DesignKind`) | 00:05Z |
| R11 | `crates/hh-context/src/compact_family.rs` l.290–320 (C1 variant tags); `crates/hh-context/src/compact.rs` `rg` hits at l.5, 33–36, 927, 935, 1083 (kernel `evict_oldest`, `clear_tool_results`) | 00:05Z–00:06Z |
| R12 | `crates/hh-env/src/activation.rs` l.1–60 (the `hh-activation/1` packet header, frozen leg vocabulary, `FreezeFacts`) | 00:06Z |
| R13 | `scripts/check-position.sh` l.316–328 (`BANNED`) | 00:12Z |
| R14 | `docs/design/33_model-mediation.md` (C2): `rg` hits for temperature/sampling/effort, read in context (l.75–80, 268–294, 416, 482, 511, 568) | 00:04Z |

`docs/research/25_tb21-harbor.md` (B1) was not read directly. Its census enters only through D1 F4's table (bands, timeouts, dev/held-out allocation). `docs/design/31_experiment-runner.md` (B2) was not read. Runner vocabulary (`ArmSpec`, `next/claim/launch/settle`) is cited only through E2 §6.2.

### Simulation record

These are design aids, not the admitted §4.3 battery. They ran on Python 3.15.2, standard library only, from `…/scratchpad/D3/`, between 2026-11-07T00:06:15Z and 00:06:43Z. Both scripts are reproduced verbatim in Appendix A.

| file | SHA-256 |
|---|---|
| `sweep_sim.py` | `fb3e175d7a113f6f9fcd170731ad2f416a547a39de1bb3fb69ca0bea4a64800a` |
| `conf_sim.py` | `7f4df6882512acb5ebc1ffa3584170649dddfd40a41f8c813e0682fa6b7b32cc` |
| `sweep_results.json` | `553f0e665c8a092c769ab39bfacdc18e17893776eaba0595b9358c57e7fa1435` |
| `conf_results.json` | `70b153ec84f9c933c1ebdd31debee47682131863b094d2f3b643bed7e7c1eb7b` |

**Data-generating process.** It follows D1's construction (D1 Simulation record), adapted to configuration variants of one harness.
- Base task solve probability `p_t ~ Beta(a,b)`, using D1's `board` (mean .60, flip .235) and `easyband` (mean .70) mixtures. The baseline arm has `p_Bt = p_t`.
- Level ℓ has `p_ℓt = expit(logit p_t + β_ℓ + γ_ℓt)`, with level-by-task interaction `γ_ℓt ~ N(0, σ²)` and `σ ∈ {0.25, 0.75}`. Configuration variants of one harness are assumed to interact with tasks less than different harnesses do, so σ = .25 is added beside D1's .75.
- `β_ℓ` is set by bisection so that the marginal effect equals the target Δ_ℓ.

**Two-stage simulation (`sweep_sim.py`, seed 20261109, 1,000 datasets per cell).**
- There are nine levels. True-effect scenarios:
  - S0, all 0;
  - S1, one level at +.10 and the rest 0;
  - S2 "mixed small", {+.05, +.03, 0, 0, 0, −.03, −.05, −.10, −.15};
  - S3 "two good", {+.10, +.05, 0, 0, 0, 0, −.05, −.10, −.15};
  - S4, one level at +.15 with the rest null or harmful.
- Exploration designs nest within each dataset, giving common random numbers. The levels use k replicates and the baseline k_b, on T_dev development tasks.
- The exploration estimate is the observed equal-task contrast.
- Selection takes the top m (m ∈ {1,2,3}) levels with an estimate above 0. The simulation does not apply the activation, cost and one-per-factor gates.
- Confirmation uses fresh tasks, T = 37, k = 5 for both arms, and fresh interaction draws.
- The test is the sharp-null sign flip in its normal approximation, `2(1−Φ(|Σd|/√Σd²))`, with Holm α = .05 over the selected members. A confirmed gain is a rejection with Σd > 0.

**Confirmation precision (`conf_sim.py`, seed 20261110, 2,000 datasets per cell).** One configuration against the baseline, for T ∈ {20, 37, 54}, k ∈ {3, 5} and Δ ∈ {0, .05, .10, .15}.
- Bootstrap half-width is `1.96·sd(d_t)/√T`.
- Finite-manifest credible half-width is `1.96·√(Σ Var Beta(c+1, k−c+1))/T`.
- Power at threshold α/m for m = 1…4, where m = 4 is three configurations plus a combination. This is the Bonferroni lower bound for one non-null member, counting positive-direction rejections only.

**Known approximation.** D1 measured the normal-approximation sign flip as anti-conservative relative to the exact Monte Carlo flip, by up to about 5 points of power (D1 Simulation record, "Checks"). The power and false-confirmation rates tabled here are therefore somewhat optimistic for the real routing. The half-widths are unaffected.

**Cross-check against D1.** At T = 37, k = 5, `board`, σ = .75, my bootstrap half-width is ±.078 and power at Δ = .10, m = 1 is 0.68. D1 reports ±.082 and 0.65. The small difference is expected, because D1 puts interaction on both arms and this DGP puts it on the variant only.

---

## Part 1 — Factor and level registry

**What a level is.** Each level is a sealed HIR document. It differs from the frozen baseline document in the declared typed paths of exactly one factor and is materialized by `hh-assembly::enumerate` (E2 §6.2, REQ-XE-124). "B" marks the baseline value. A level's identity is its sealed `version_id`, and every slot record names it (E2 §6.3).

**"Exercised on TB 2.1 with glm-5.3-flash".** This column asks a specific question: on this workload, will the knob change what the model receives, and how often? For conditional knobs the answer is an empirical fraction. The pre-exploration activation forecast (Part 2.4) estimates it from non-outcome records, and exploration measures it.

### 1.1 Tier 1 — the exploration registry (design proposal)

| ID | Factor · typed path (E2 §6.1) | B → level | Why it plausibly matters | Exercised? (activation metric) | Cost impact | Dependency |
|---|---|---|---|---|---|---|
| L1 `eff-low` | reasoning effort · declared lane parameter, applied by the proxy pin (C2 REQ-XC-107) | `high`* → `low` | Less reasoning per call. HF's mini-swe-agent row on GLM-5.3-Flash moved 66.3 → 64.0 from high to low, on another serving setup (C3 F2; C1 F16) | **Always.** Thinking cannot be disabled (C1 F5). Metric: fidelity, i.e. applied = declared on 100% of calls | ↓. Median reasoning tokens on a short prompt: low 107 vs high 182 (C1) | C2 PROXY-CORE pin; E2-T8 participant record; the F2 effort-home decision |
| L2 `eff-max` | same | `high`* → `max` | The model default and vendor recommendation; about 1.9× high's reasoning tokens (C1 F5) | **Always** (fidelity as L1) | ↑. About ×1.3 per trial (C1 cost table). Raises stream-idle and output-cap risk (C1 F11; C2 nonstream deadline) | as L1 |
| L3 `cmd-120` | exec timeout · `ToolCapability.exec_defaults.timeout_ms` | **30 s** → 120 s | mini kills a command at 30 s with rc −1 (E1, l.195). Long builds and test runs must otherwise be backgrounded or split | **Conditional.** Activation = share of first subjects with ≥ 1 command running > 30 s | wall ↑. Agent-timeout risk in band A (≤ 900 s) | E2-T6 (G17 exec semantics, timeout → rc −1). The baseline needs it anyway |
| L4 `obs-2.5k` | observation renderer · `result_renderer.{threshold, head, tail}` (`json_head_tail@1`, chars) | **10 000 / 5 000 / 5 000** → 2 500 / 1 250 / 1 250 | A smaller context is cheaper but drops the middle of outputs (E1 l.132–146, 200) | **Conditional.** Activation = share of first subjects with ≥ 1 observation whose L4 rendering differs from the baseline rendering of the same raw output. This is an exact counterfactual, because the renderer is deterministic | ↓ input tokens | E2-T4 (G8, G18). On the baseline's path |
| L5 `rsn-drop` | transcript render · `assistant_replay.reasoning` | **keep (as O)** → drop | mini re-sends the full assistant message, including `reasoning_content` (D2 R3; E1 F8). Dropping it shrinks the context and changes conditioning | **Always** after turn 1, since reasoning is always present (C1 F5). Activation = share of requests with a dropped non-empty reasoning member | ↓ input tokens | E2-T1 (G6). On the baseline's path |
| L6 `prm-wf` | instance template · `Text:instance_template` alternative leaf (`ablate`, E2 §6.1) | **verbatim** → recommended-workflow steps removed, **submit instruction and command rules kept** (F7) | Prompt guidance may steer exploration order. The direction is unknown | **Always** (wire-visible). Behavioural exercise is not separately measurable | ≈ 0 | E2-T3 (`hh-tmpl/1`). On the baseline's path |
| L7 `tool-edit` | tool surface · added capability + `description_template` | **bash only** → bash + `edit_file` | Structured edits instead of sed or heredoc. Brief §2 names "tool surface" | Wire-visible always. Behavioural activation = share of first subjects with ≥ 1 `edit_file` call | small ↑ (schema tokens) | E2-T2 (`tools[]` from the compiled surface) **plus a new exec-backed `edit_file` capability for podman cells**. The only `edit_file` in tree is a compiler test fixture (E2 l.57; `crates/hh-compiler/tests/s3_2.rs:28–60`). **Conditional Tier 1** |
| L8 `cmp-ctr@W` | `compaction_strategy` slot + declared `window_cap` | **none @ catalog window** → `clear_tool_results` @ W | Tool results dominate terminal-agent contexts. Clearing old ones bounds context and cost | **Conditional, and only under W** (F3). §4.2 activation: starts / eligible opportunities; triggered first subjects / all; changed actual requests / completions with next dispatch | ↓ input tokens on long trajectories | E2-T7 (slot wiring + `window_cap`). The variant exists (`compact.rs:1083`) |
| L9 `cmp-sum@W` | same | **none** → `hh/summarize-rolling@1` @ W | Summaries keep facts that eviction drops. It is the Terminus-2 style of compaction (E1 F6) | as L8; also overhead per §4.2 | **±**: compaction-role model calls (subject-charged, §2.4) offset by a smaller context | E2-T7 + a summarizer profile on the proxy lane. **Conditional Tier 1** |

\* The baseline effort is whatever Q-X22 pins for phases 2–3. D1 and D2 recommend `high`.

Seven factors give nine perturbation levels, or ten arms with the base. Composite levels L8 and L9 are two levels of one factor, `context_management`, whose baseline level is "none @ catalog window". That keeps the Lab design a valid OFAT design (Part 2.1).

### 1.2 Tier 2 — registered, not run in the first exploration (design proposal)

| Candidate | Reason it is reserved | Admission route |
|---|---|---|
| Step limit (`Budget.dimensions.model_calls.hard`) ∞ → 50 | HF's mean is 31.8 calls per trial (C3), so a cap of 50 binds only the tail. It is mostly a cost lever, and its success effect is plausibly ≤ 0 | A cost-oriented later exploration, once an admitted cost method exists (Open questions) |
| Format-error cap 3 → 1 / 5 | Exercised only by streaks. With native tool calls these are probably rare (inference) | Admit if the phase-2 pilot's mini receipts show a streak rate ≥ the activation floor |
| Observation truncation off / 20 000 | Exercised only by outputs > 10 000 characters, which are rarer than outputs > 2 500 | As above, using the observation-length census |
| Context policy (`recency_plus_pins`, `priority_eviction`) | **Probably inert on the wire today.** The transcript reads the compaction rows, not the assembled plan (E2 F13) | After G21 lands; the liveness check (REQ-XE-125) must pass |
| Interaction mode `shim` (text actions) | Needs G14, which is part of Terminus-2 stage two (E2-T12; Q-X42) | After the stage-two block |
| Temperature (explicit value) | The baseline sends none. The proxy forwards harness-set sampling (C2 l.290). The provider default and whether sampling is honoured under always-on thinking are both unknown (C1 F12 analogue). An explicit 1.0 may equal the default and be distributionally inert while still wire-visible | After the S0 smoke (Q-X23) records acceptance and a stated rationale for a non-default value |
| Cost limit (`Budget.spend.hard`) | Duplicates the proxy's per-trial ceiling | Not proposed |

**Registry rule.** Adding or changing a level after the exploration freeze creates a new exploration identity, X2. X2 data never merge into X1 estimates, and its selection is a separate packet. This is not `adaptive_search`, which is admissible only inside a `search_budget` (`eval.rs` l.1740 doc comment). No level is generated from outcomes.

---

## Part 2 — Exploration stage

### 2.1 Design

- **Lab design.** `DesignKind::OneFactorAtATime`. The varied factors are `effort`, `cmd_timeout`, `obs_render`, `reasoning_replay`, `instance_prompt`, `tool_surface` and `context_management`. Each declares the baseline as its first level, so the validator's base point is the baseline. The arms are exactly the base plus the nine single-level perturbations (`experiment.rs` l.1650–1672). Each `LevelSpec.ref_` is a sealed `version_id`, and `overrides` carries override spellings for attribution only (E2 §6.2, REQ-XE-124).
- **Tasks.** The **11 band-A development tasks** (D1 F4: E/M/H = 1/7/3).
  - Ten of them are the phase-2 pilot set P, which is already exploratory (D1 F4 item 5).
  - Band A matches the confirmation stratum and Q-X31's "start out with the lowest time / cheapest ones".
  - The 9 band B/C development tasks stay unused, reserved for a possible later exploration ahead of a band-B confirmation.
  - Alternative: all 20 dev tasks at k = 3 for the levels and 6 for the baseline (Part 2.5 table).
- **Replicates.** k = 5 for every arm. This meets the R ≥ 5 cell floor, so the admitted finite-manifest machinery can produce descriptive intervals. The confirmatory pipeline is also exercised end to end before any confirmatory subject.
- **Schedule.** Seeded randomized interleaving over (task, arm) in 5 rounds on identical lanes. This is D1's `slot_schedule_randomization` (D1 F8, REQ-XD-021), recorded before execution.
- **Feasibility smoke X0 before X1.** Ten arms × 3 dev tasks (one per difficulty) × 1 replicate = 30 trials. It checks that every level runs end to end with complete receipts and nonzero activation where expected, and it measures per-level $/trial multipliers (`eff-max`, `cmp-sum@W`). X0 has its own identity and is never pooled into X1 (§4.1: separate diagnostic, activation and pilot identities, l.238).
- **Stop rules.** Budget, infrastructure, integrity and catalog only, mirroring D1 F9 and C1 REQ-XC-009. There is no outcome-based stopping and no mid-run dropping of a "losing" level. AC-3-2 forbids unregistered futility tests (l.227). A registered futility rule was considered and rejected: it saves little at this size and makes the descriptive estimates harder to read.

### 2.2 Estimands (exploratory only)

For level ℓ on the executed dev manifest D_A:

```text
x_ℓt  = (1/k) Σ_r y_ℓtr        (joint_task_pass_without_observed_veto; first subject or first admissible retry; D1 F3)
Δ̂_ℓ  = (1/|D_A|) Σ_t (x_ℓt − x_Bt)
```

Each Δ̂_ℓ is reported with:
- a descriptive whole-task bootstrap 95% interval (`stats::bootstrap`, labelled descriptive);
- the finite-manifest credible interval, labelled "descriptive use of the admitted estimator; exploration identity";
- BH q-values over the nine contrasts (the admitted `benjamini_hochberg`, labelled exploratory);
- activation, fidelity, completion envelope, $/trial (mean, P50, P90), $/solve, tokens, calls, wall time, infra and timeout rates, and scan flags.

**No exploration figure is a claim.** The packet's label is "exploratory, 11 development tasks, selection input; not confirmatory".

### 2.3 Liveness before spend (extends D2 REQ-XD-125 and E2 REQ-XE-125)

Every level must pass the declared-delta and liveness check at $0 before X0:

- **Declared delta.** The L1.1 canonical request diff against the baseline is confined to the factor's declared paths.
- **Liveness.** The diff is non-empty on the factor's exercising branch.

**Executor-level knobs need a different leg (F6).** L1.1 replays recorded observations into H, so the executor is a test double and L3 `cmd-120` produces no diff. L3 needs an **executed** liveness leg instead: a scripted model, at $0 model spend, in a real cell runs a 45 s command. The baseline must return rc −1 with the timeout text, and the level must return rc 0 with full output.

The composite levels L8 and L9 use a synthetic trajectory whose assembled view crosses W.

### 2.4 Pre-exploration activation forecast and the window-cap rule

**Inputs.** Records produced by the baseline configuration, read for non-outcome fields only:
- the HH-native baseline's X0 slots;
- the phase-2 pilot's original mini-swe-agent receipts. The original has the same 30 s timeout and the same 10,000-character renderer (E1);
- D2's O-only pilot recordings (K6).

The fields used are per-call upstream input tokens, per-command durations, raw observation lengths and reasoning presence. No resolve outcome is read.

**Forecast and floor.** For each conditional level the forecast is the predicted share of first subjects that the knob would touch. The floor is **0.25** (design proposal). A level forecast below it is `withheld{inert_predicted}` before X1 spend and stays in the registry with its forecast.

**Window cap W (design proposal).**
- Let F(w) be the estimated share of baseline trials whose peak upstream input tokens exceed w.
- W is the largest value in {32 768, 49 152, 65 536, 98 304, 131 072} with F(W) ≥ 0.5, so that about half the trials compact.
- W is never below 32 768. That floor leaves room for the system and instance prompts, a full 10,000-character observation and a response.
- If F(32 768) < 0.25, both composite levels are `withheld{inert_at_admissible_window}`.
- W is frozen in the exploration pre-registration. The same W is used by both composite levels and by any later confirmation of either.

**Why this is not outcome selection.** W is chosen from the token-count distribution, which §4.1 lets a pilot estimate ("activation", l.238). It is applied uniformly to every task and arm in the family, never conditioned on a task. Peak tokens correlate with trajectory length, and therefore loosely with failure. The rule picks a dose from a nuisance distribution, not a favourable subset, and the report says so.

**Prior expectation (inference, UNVERIFIED).** HF's mini-swe-agent averaged 1.031 Mtok over 31.8 calls per trial (C3 F2), about 32 k input tokens per call. If the context grows roughly linearly, peak contexts are around twice that. W = 32 k–64 k would then make about half the trials compact. The 1 M catalog window would make none compact.

### 2.5 Exploration precision, selection quality and cost

The table below uses nine levels plus the baseline; σ = .75, `board` mixture. The half-width column (descriptive bootstrap, mean over levels), P(best selected) and P(confirm gain), the probability that the truly better level is confirmed, are for scenario S1 with m = 2 and confirmation at T = 37, k = 5. "Optimism" is the mean exploration estimate minus the true effect for selected levels under S0. Spend is model spend only. The planning ceiling is trials × $0.34 × 1.2 (infra) × 1.5 (tail), as in D1. Wall-days assume 4 trials in flight at a 700 s mean, as in D1. Per-level cost multipliers (`eff-max` ×1.3) add about 3% and are inside the ceiling.

| design | trials | $ @0.05 | $ @0.15 | $ @0.34 | planning ceiling | wall-days | mean ±hw per contrast | P(best selected) | P(confirm gain) | optimism under S0 |
|---|---|---|---|---|---|---|---|---|---|---|
| T11, k=3 | 330 | 17 | 50 | 112 | 202 | 0.7 | .169 | 0.72 | 0.41 | +.10 |
| **T11, k=5 (recommended)** | **550** | **28** | **83** | **187** | **337** | **1.1** | **.138** | **0.82** | **0.46** | **+.08** |
| T20, k=2, k_b=4 | 440 | 22 | 66 | 150 | 269 | 0.9 | .134 | 0.82 | 0.47 | +.08 |
| T20, k=3 | 600 | 30 | 90 | 204 | 367 | 1.2 | .128 | 0.87 | 0.49 | +.08 |
| T20, k=3, k_b=6 | 660 | 33 | 99 | 224 | 404 | 1.3 | .113 | 0.87 | 0.49 | +.07 |
| T20, k=5 | 1,000 | 50 | 150 | 340 | 612 | 2.0 | .104 | 0.93 | 0.53 | +.06 |
| X0 smoke (3 × 1 × 10) | 30 | 1.5 | 4.5 | 10 | 18 | 0.1 | — | — | — | — |

**Reading the table.**
- Going from 11 to 20 tasks raises P(best selected) from 0.82 to about 0.87 and P(confirm gain) from 0.46 to about 0.49. It costs 50–450 more trials, nine of the extra tasks are in longer bands, and it crosses strata.
- The recommendation is band-A k = 5, which follows Q-X31. The 20-task k = 3, k_b = 6 design is the alternative if the operator prefers a wider screen.
- mini-swe-agent's own priors are $0.155 per trial with no caching and $0.043 with caching (C3 planning table). The 550-trial X1 then costs about $85 or about $24 respectively.

---

## Part 3 — Selection rule (frozen before any X1 outcome)

The rule is part of the exploration pre-registration (Part 5.3). It is applied mechanically to the committed X1 records.

1. **Eligibility gates.** A level is eligible only if every gate holds:
   - (a) declared-delta and liveness passed (Part 2.3);
   - (b) fidelity: intended = served on 100% of receipts, meaning the applied effort, renderer version, `window_cap` and variant identity all match the declaration;
   - (c) measured X1 activation ≥ 0.25 for conditional knobs;
   - (d) completion: unresolved slots ≤ 10% of the level's planned slots;
   - (e) integrity: no confirmed answer-key or tamper veto in the level (D1 F13);
   - (f) cost feasibility: the level's mean $/trial ≤ 2.0 × the baseline's mean in X1.
2. **Score.** Δ̂_ℓ, the observed equal-task contrast in the joint population. Only Δ̂_ℓ > 0 qualifies.
3. **Ranking.** Sort by Δ̂_ℓ in descending order and keep at most **one level per factor**, the higher one. This matters for `effort` and `context_management`, which each have two levels.
4. **Count.** Select the top **m_max = 2**.
5. **Ties.** Equal Δ̂ (within 1/(|D_A|·k)) is broken by lower mean $/trial, then by registry order.
6. **Combination (optional, pre-declared `combine: true|false`).** If two levels from different factors are selected and `combine: true`, add a third member, C₁₂, which applies both levels together. C₁₂ has no exploration estimate. Its inclusion rests on an additivity assumption that OFAT cannot test (F13).
7. **No candidate.** If no level is eligible with Δ̂ > 0, confirmation is `withheld{no_candidate}`. The phase-4 result is then the exploration packet plus the statement "no explored level had a positive exploration estimate under the frozen rule". That result is publishable.
8. **Determinism.** The selection packet is a pure function of:
   - the committed X1 records, listed in an input manifest of hashes;
   - the frozen rule.

   It recomputes byte-identically. Its input manifest must contain no held-out record and no phase-2 or phase-3 outcome record (F10).

**Winner's-curse controls.**
- Confirmation uses tasks, slots and a time window that the selection never saw.
- Exploration and confirmation are never pooled.
- Every display of a selected configuration's exploration estimate carries "selected from the same data; expected optimism ≈ +.03 to +.10 (design simulation)".
- The claim is the confirmation estimate. In simulation, the confirmation estimate's mean error was |bias| ≤ .003 in every cell.

---

## Part 4 — Confirmation stage

### 4.1 Design and estimand

- **Tasks.** The 37-task **band-A held-out census**, the same manifest as phase-2 tranche 1 (D1 F4).
  - Re-using held-out tasks across phases is sound. Each phase makes its own claim under its own pre-registration, and phase 4's selection never reads their outcomes (D1 F4: "must not use phase-2 per-task outcomes to choose configurations").
  - An optional wider scope is the band-A ∪ band-B held-out census (T = 54), declared as one analysis stratum. It needs its own exploration-to-confirmation transfer caveat, because exploration was band A only.
- **Arms.** The baseline, **re-run in the same window**, plus the selected configurations (and C₁₂ if declared).
  - Do not reuse the L2 H-arm or the phase-2 original-harness slots. They ran in another window and outside this schedule's exchangeability argument (D2 Part 6: "L2 data are not reused").
  - The Lab design is a single `configuration` factor (kind `harness`) whose first level is the baseline, which is a valid OFAT design with one varied factor.
- **Replicates.** k = 5, with a seeded schedule in 5 rounds under `slot_schedule_randomization`.
- **Estimand.** D1 F3 applied per configuration j: `Δ_j = (1/37) Σ_t (x_jt − x_Bt)` on the executed manifest, joint population, all k planned replicate slots. "These 37 tasks", never TB 2.1.
- **Family.** `{Δ_j : j selected}`, with Holm α = .05 over the sharp-null sign-flip p-values: 8,000 Monte Carlo draws at T > 20 (l.275). The test is two-sided, and the claim's direction is the sign of the observed contrast.
  - Unavailable members stay in the family (D1 REQ-XD-030 pattern).
  - The phase-4 family is separate from every phase-2 tranche family and from phase 3's claim, because each has a separate pre-registration (consistent with D1 Q-D1-3).
- **Intervals and secondary outcomes.**
  - Finite-manifest 95% credible interval (8,000 draws plus the doubling record) and task-bootstrap sensitivity (BCa at T ≥ 30).
  - Secondary descriptive measures: $/trial, $/solve, tokens, calls, activation and flips.
- **Validity of selecting from exploration.** Selection uses data independent of confirmation, because the tasks, slots and window all differ. Conditional on the selected set, the confirmation family is therefore a fixed, pre-registered family, and Holm controls its familywise error at .05.
  - The simulated directional false-confirmation rate stayed at or below 0.026 in every scenario, including the global null.
  - What selection noise costs is power. It does not cost validity.

### 4.2 Precision, power and cost

The table uses the `board` mixture, σ = .75 (σ = .25 in brackets for the m = 2 power column). The "power m" columns give the probability of confirming a configuration with true gain Δ when the family has m members (Bonferroni lower bound). Spend columns are as in Part 2.5.

| T | k | m | trials | $ @0.05 | $ @0.15 | $ @0.34 | ceiling | wall-days | ±boot | ±FM | power Δ=.05 | power Δ=.10 | power Δ=.15 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 37 | 5 | 1 | 370 | 19 | 56 | 126 | 226 | 0.8 | .077 | .066 | 0.23 | 0.68 | 0.97 |
| **37** | **5** | **2** | **555** | **28** | **83** | **189** | **340** | **1.1** | **.077** | **.066** | **0.13 [0.17]** | **0.55 [0.64]** | **0.93 [0.96]** |
| 37 | 5 | 3 (2 + C₁₂) | 740 | 37 | 111 | 252 | 453 | 1.5 | .077 | .066 | 0.10 | 0.46 | 0.90 |
| 37 | 3 | 2 | 333 | 17 | 50 | 113 | 204 | 0.7 | .095 | .080 | 0.10 | 0.37 | 0.75 |
| 54 | 5 | 2 | 810 | 41 | 122 | 275 | 496 | 1.6 | .064 | .055 | 0.23 | 0.75 | 0.99 |
| 54 | 5 | 3 | 1,080 | 54 | 162 | 367 | 661 | 2.2 | .064 | .055 | 0.17 | 0.68 | 0.98 |

The `easyband` mixture gives nearly identical figures (T = 37, k = 5: ±.073 / ±.064, power 0.63 at Δ = .10, m = 2). The rejection rate at Δ = 0 was ≤ 0.031 for m = 1 and ≤ 0.018 for m = 2, counting the positive direction only.

**Precision target (design proposal, mirroring D1 F5).** Every confirmatory contrast's bootstrap half-width ≤ .10 and FM half-width ≤ .08. T = 37 with k = 5 meets it for any m.

**Envelope.** The recommended phase-4 total is X0 + X1 (T11, k = 5) + confirmation (m = 2) = 1,135 trials: about $57 / $170 / $386 at $0.05 / $0.15 / $0.34, with a planning ceiling of about $695. Each stage is separately gated (Part 5.2). None of it falls inside the $60 pilot authorization (Q-X11).

### 4.3 What a null means and how it is reported

- **"Not confirmed"** means the member's Holm-adjusted p ≥ .05. It is reported with:
  - the confirmation Δ̂_j and both intervals;
  - the exploration Δ̂_j with its selection label;
  - "on these 37 held-out tasks; the interval includes effects as large as ±b".

  It is never "no difference", "equivalent" or "the knob does not matter". No equivalence method is admitted for efficacy (D1 F6; R8 l.275: credible intervals are not TOST).
- **"Confirmed higher"** or **"confirmed lower"** (two-sided) carries the family id, the Holm-adjusted p, the FM interval, and the scope labels (Part 5.4).
- **`withheld{no_candidate}`** and **`withheld{budget|infra}`** are typed dispositions. An unexecuted confirmation is unmet, never null (brief §4).
- **Expectation, stated in advance.** For config-knob effects of about 5 points, P(confirm) is 0.13–0.26. For S2-like mixtures, P(any confirmed gain) was 0.10–0.15. The likely honest headline is therefore exploratory rankings plus "no selected configuration was confirmed different from the baseline at Holm α = .05 on 37 tasks". The pre-registration says this before data.

---

## Part 5 — Interaction with phases 2–3, budget gates and site claims

### 5.1 Order

**Hard prerequisites.**
- G-L1 records a pass for the exact H version that is to be swept (D2 REQ-XD-102). Sweeping an unconformed H would make the "baseline" undefined.
- E2-T7, E2-T9 and E2-T10 have landed (compaction slot, the mini document, sweep glue), and so has D2 K4 (the L1 runner, for declared-delta checks).
- D1's n-arm comparative protocol and the `slot_schedule_randomization` admission (D1-a, D1-b) have landed and passed their numerical-review gate.
- Any H change after G-L1 means an L1 rerun and a new phase-4 cohort.

**Recommended order:** G-L1 → L2 tranche → L2 verdict recorded → exploration (X0, X1) → selection → confirmation.

- **Why exploration does not run concurrently with L2.**
  - D2's participant record requires the Ollama account to be exclusive during the L2 tranche (D2 §2.1, "Account").
  - The three-request concurrency budget (C1 F11) is shared.
  - An L2 finding that forces an H repair would orphan exploration data collected on the old H.
- **Exploration before L2 (permitted, not recommended).** The operator may let exploration start after G-L1 and before L2 if L2 funding is deferred. Its records would then carry "L1-conformant baseline" and keep that label.
- **Confirmation waits for the L2 verdict.** Confirmation should not be pre-registered until the L2 verdict is recorded, so that the baseline label in the pre-registration is final: "parity-verified on ⟨catalog digest, window⟩" or "L1-conformant baseline, verdict parity inconclusive" (D2 REQ-XD-126).
- **Catalog changes.** A catalog-identity change between exploration and confirmation does not invalidate the confirmation, because its contrasts are within-window. It does weaken the transfer from selection, and it is recorded.

**Reuse.** Phase 4 reuses phase 2's runner, proxy, allowlisted egress, `answer-scan/1` and its HUMAN flagged-pass review, the reattempt and cause mapping (D1 F12), the dev/held-out split, the band-A census manifest, and the GCE-per-phase recipe with teardown (B3). It adds no second estimator, protocol, split or scheduler (CC1).

### 5.2 Gates

| gate | when | records verbatim |
|---|---|---|
| **G-P4-X** (exploration spend) | after the registry, the declared-delta and liveness reports, the activation forecast with W, and the exploration pre-registration (incl. selection rule) are committed | spend authorization for X0 + X1; registry hash; W; pre-registration hash, which must predate every X0 subject |
| **G-P4-C** (confirmation spend + freeze) | after the selection packet and the confirmation quote | spend authorization; selected members (incl. C₁₂ or not); confirmation pre-registration hash, which must predate every confirmation subject |
| HUMAN flagged-pass review | before each results freeze | per D1 F13 / REQ-XD-071 |
| Publication gate | after the results packet | per A-9 / F2 / H1 |

### 5.3 Pre-registration contents

**Exploration pre-registration:**
- the baseline H record (D2 REQ-XD-126 fields) and its label;
- the registry, with each level's sealed `version_id`, declared paths and liveness report id;
- the forecast inputs manifest, the forecasts, the floor, W and withheld levels;
- the X1 manifest (the 11 band-A dev task ids, from the split record);
- k, the schedule seed and rounds;
- the effort pin of the baseline;
- the population and estimators (descriptive);
- the stop rules;
- the selection rule (Part 3) with m_max, `combine` and the gates' thresholds;
- the integrity scan version.

**Confirmation pre-registration:**
- the selection packet id;
- the members, and the family (Holm, α, test, draws, seeds);
- the manifest (37 held-out ids, or 54);
- k and the schedule seed;
- the estimand, population, priors, draws and doubling;
- the precision target and the quote id;
- the stop rules, the reattempt bound and the unresolved-cell policy (D1 Q-D1-4);
- the claim templates (Part 4.3), fixed before data;
- the W carried from exploration;
- the baseline label.

### 5.4 Claims language for "which configurations perform best"

Brief goal 4's "report which configurations perform best" is rendered as two separately labelled statements:

1. **Exploratory ranking.** "In exploration on 11 development tasks, the levels with the highest estimated contrast vs the baseline were X (+a) and Y (+b). These estimates are descriptive, selected from the same data, and expected to be optimistic."
2. **Confirmatory status.** "Confirmed on 37 held-out tasks: X − baseline = +c, 95% credible [l, u], Holm-adjusted p = q (family P4-C1)". Or: "Not confirmed: the 95% interval [l, u] includes 0."

Proposed additions to F2's site check, beyond `check-position.sh`'s `BANNED` list (l.316–322) and D1 F16's additions ("best harness", "winner", "beats", "no difference", "equivalent", "same performance"):
- **Banned:** "best configuration", "optimal configuration", "optimal setting", "winning configuration", "tuned to".
- **Conditional:** "improves" or "improvement" only in a sentence that carries "Holm-adjusted p < .05 (family P4-…)".

**Mandatory labels:**
- the baseline label (D2 REQ-XD-126);
- "finite-manifest: these 37 TB 2.1 held-out tasks";
- "one-factor-at-a-time: effects are local to the baseline configuration; interactions not estimated";
- for L8 and L9, "under a declared W-token context window (a harness setting, not the model's 1M limit)";
- D1 F16's per-page labels.

---

## Part 6 — Ticket-shaping hints

Each ticket is sized for one fresh-context `implement-spec` run. Acceptance is deterministic unless marked live.

| # | ticket | depends on | deterministic acceptance | gate after |
|---|---|---|---|---|
| D3-a | **Factor registry + level documents.** `sweep-registry/1` record and validator. Sealed level documents via `enumerate` for L1–L9. Declared-delta and liveness runs (L1.1 replay plus the executed scripted-model leg for L3). `inert` refusals | E2-T9, E2-T10, D2 K4; E2-T6 (L3), E2-T7 (L8, L9), `edit_file` capability (L7) | validator rejects missing members; every level's diff is confined and non-empty; an inert fixture is refused; the L3 executed leg distinguishes rc −1 from rc 0 | — |
| D3-b | **Activation fold + forecast + W rule.** A generalized activation fold implementing §4.2 (l.252) and the counterfactual metrics. It is not `hh-activation/1`, whose leg vocabulary is frozen to round-5 scope (`activation.rs` l.22–35). Plus the forecast and W computation from non-outcome records | D3-a; phase-2 pilot records; D2 K6 records | known-answer fixtures per metric; W rule on synthetic token distributions (incl. `inert_at_admissible_window`); recompute is byte-identical; the input manifest refuses outcome fields | — |
| D3-c | **Sweep execution glue.** Lab OFAT design → `hh-runner` arms, one per sealed level. Per-arm effort pin through the proxy. Seeded schedule. Slot `participant.hir.level_assignment` | E2-T8, E2-T10, B2 RUNNER-CORE, C2 PROXY-CORE, D1-a | a fixture OFAT plan against a replay proxy and stub cells: arms = base + perturbations; schedule reproducible from seed; every slot names its document; per-arm applied effort recorded | — |
| D3-d | **Exploration pre-registration + X0 smoke** (live) | D3-a…c, G-L1 (and the L2 verdict, per 5.1) | packet hash predates every subject; every X0 slot dispositioned | **G-P4-X** before X0 |
| D3-e | **Exploration X1** (live) + exploration packet | D3-d | every slot dispositioned; the packet recomputes from records; labels present | — |
| D3-f | **Selection packet** (offline) | D3-e | byte-identical recompute; the input manifest holds only X1 records; refusal fixture when a held-out record is supplied; `no_candidate` fixture | — |
| D3-g | **Confirmation quote + pre-registration** (offline). Precision via D1's comparative battery extended with a selection-conditioned scenario (REQ-XD-024); cost from the X0/X1 measured per-level $/trial | D3-f, D1-b | quote recomputes; battery records retained | **G-P4-C** |
| D3-h | **Confirmation run** (live) | D3-g | every slot dispositioned; completion envelope | HUMAN flagged-pass review |
| D3-i | **Results packet + claims check** (offline) | D3-h | every figure recomputes; the mutation battery over the banned and conditional phrases passes | Publication gate |

D3-a may split into a "documents + validator" ticket and a "liveness runs" ticket if the size trigger fires. D3-e and D3-h are execution tickets. Their code should already exist from D3-c.

---

## Findings

### F1 — The Lab already validates OFAT designs and the assembly already materializes sealed levels; phase 4 needs execution glue and a generalized activation fold, not a new design kind
Claim: `DesignKind::OneFactorAtATime` is admitted. Its validator requires the arms to equal the base point (each varied factor's first level) plus single-level perturbations. `LevelSpec` carries a pinned `ref_` and attribution-only `overrides`. The round-5 activation packet's leg vocabulary is frozen to that study.
Status: VERIFIED (validator, types); PARTIALLY VERIFIED (activation packet scope, read from its header and types only)
Evidence kind: source inspection
Evidence:
- `crates/hh-lab/src/experiment.rs` l.113–131 (`LevelSpec`), l.177–193 (`FactorSpec`), l.1650–1672 (OFAT base and perturbations; refusal "arms are not the base point plus its single-level perturbations"), l.2000–2020 (per-arm rebinds read from `overrides`);
- `crates/hh-ontology/src/eval.rs` l.1731–1742 (`DesignKind`);
- `crates/hh-env/src/activation.rs` l.1–35 ("frozen — GATE-G12 Reading 1's scope"; `LEGS` = 7 fixed legs; `FreezeFacts.arms` must equal the frozen `ARM_STRATEGIES`);
- E2 F12 (`enumerate` has no production consumer).
Retrieved: 2026-11-07T00:04Z–00:06Z
Implication for the spec: The exploration and confirmation designs are both expressed as OFAT Lab documents over sealed levels (Part 2.1, 4.1). D3-b builds a §4.2-generic activation fold beside `hh-activation/1` and leaves the latter unchanged.
Outline delta: EXTENDS OL-07 — the OFAT design kind is reused unchanged.
Limitations: I did not read `varied_factors`, `arms_cover` or the Lab's handling of two arms with equal `ref_`. D3-c's fixture must exercise them.

### F2 — The Lab factor vocabulary has no member for a model-call parameter, so effort needs a declared home before it can be a factor
Claim: `FactorKind` is {`model_snapshot`, `harness`, `environment`, `task`, `budget`, `replicate`}. Effort is not a snapshot, and E2 calls it a participant property pinned by the proxy, not a harness slot.

The proposed home is a declared lane parameter in the sealed level document (`model_port` slot parameter `effort`). The runner reads it to set the proxy pin, and the slot participant record carries both the requested and the applied value. Each effort level is then a distinct sealed document, factor kind `harness`, and identity stays "every result names its exact document". The alternative is an ADR adding a factor kind, which touches the CC10 class home.
Status: VERIFIED (enum); UNVERIFIED (the home is a design proposal)
Evidence kind: source inspection + design proposal
Evidence: `eval.rs` l.1321–1346; E2 §6.1 closing note ("Model effort … is a participant property pinned by the proxy, not a harness slot. D3 may cross it"); C2 l.293 ("The effort level is a slot-declared parameter") and REQ-XC-107; E2 G5/G12 (the `model_port` slot params are already read, `gateway_model.rs:483–500`, as cited by E2).
Retrieved: 2026-11-07T00:02Z–00:05Z
Implication for the spec: E2/H1 must choose the effort home before D3-a. Until then, L1 and L2 are blocked as `withheld{dependency}`.
Outline delta: EXTENDS OL-07 — effort (C1 F5's "natural one-variable knob") becomes admissible only with a declared home.
Limitations: I did not check whether a `model_port` slot parameter reaches C2's proxy pin without a schema change.

### F3 — At the 1,048,576-token catalog window compaction cannot be exercised on TB 2.1; the only honest compaction factor is "strategy S under a declared window cap W"
Claim: The baseline mini-swe-agent has no compaction (E1). The participant's catalog window is 1,048,576 tokens (C1 F4). TB 2.1 trajectories for this harness average about 32 k input tokens per call, which suggests peaks of tens of thousands of tokens (inference). A compaction strategy bound at the catalog window therefore never fires, and the liveness check refuses it as `inert`.

A composite level that declares a cap W and a strategy S is exercised on roughly the share F(W) of trials. Its contrast against the uncapped baseline measures the joint effect of "run under W with S". The L8 − L9 contrast at the same W isolates the strategy, and it is exploratory.
Status: VERIFIED (no compaction in mini; the catalog window; the unwired slot); UNVERIFIED (peak-token inference; the composite-level design)
Evidence kind: source inspection + inference + design proposal
Evidence:
- E1 l.211 ("No summarization, truncation or windowing of the history exists. The context grows until the provider refuses with `ContextWindowExceededError`");
- C1 F4 (`glm5_next.context_length: 1048576`);
- C3 l.67 (mini-swe-agent 1.031 Mtok/trial, 31.8 calls/trial);
- E2 F11 and §6.1 ("**only** under a declared window well below 1 M tokens … At the catalog window it never fires");
- E1 F10 (Terminus-2's proactive trigger is effectively unreachable at a ~1 M limit);
- E2 REQ-XE-127.
Retrieved: 2026-11-07T00:02Z–00:12Z
Implication for the spec: L8 and L9 are composite levels of one `context_management` factor, with W chosen by the Part 2.4 rule. Every compaction result carries the W label. If F(32 768) < 0.25, compaction leaves phase 4 with a recorded reason, rather than running a knob that cannot act.
Outline delta: CORRECTS OL-07's implicit reading that "compaction policy" is a free knob on this workload; EXTENDS it with the composite-level rule.
Limitations: Peak tokens are unmeasured on Ollama. Re-sent reasoning (L5) inflates context, so the X0/K6 records decide. The behaviour at W when compaction cannot reduce enough (fallback, `CompactionImpossible`) must be classified (§4.2 "Ineffective/fallback") and is reported per level.

### F4 — A declared window cap is a harness variable, not the artificial pressure the round-5 supplement banned
Claim: The supplement's ban on "artificial pressure/padding" belongs to the frozen round-5 compaction contrast. The same section requires that "Tuning requires a separate exploratory design". Phase 4 is that separate design.

A cap W is admissible when it meets four conditions:
- it is declared in the level document;
- it is applied identically to every task and replicate of the level;
- it never alters task inputs;
- it is reported as a harness setting.

It is the same kind of choice as running under a smaller-context deployment. Synthetic padding of prompts or task-conditioned caps remain prohibited.
Status: VERIFIED (supplement text); UNVERIFIED (the admissibility reading is a design proposal for ratification and O2 review)
Evidence kind: source inspection + design proposal
Evidence: `docs/evidence-and-adoption-spec.md` l.234 ("No task-conditioned policy, artificial pressure/padding, adaptive search or third arm"), l.238 ("Pilot may estimate feasibility, activation … it cannot tune policies under zero search … Tuning requires a separate exploratory design").
Retrieved: 2026-11-07T00:04Z
Implication for the spec: REQ-XD-205 makes W a registry member with a validator rule. A W that varies by task, or any padding mechanism, refuses.
Outline delta: EXTENDS OL-07, OL-15.
Limitations: A reviewer may read the l.234 ban as program-wide. Ratification must state the reading explicitly.

### F5 — Registry: nine Tier-1 levels across seven factors, and seven reserved candidates, each with a stated exercise condition
Claim: The Part 1 tables give the registry. Five of the Tier-1 factors depend only on gap closures that the baseline itself needs (G6, G8/G18, G17, `hh-tmpl/1`, and the proxy pin for effort). Two levels are conditional on extra work: L7 needs an exec-backed `edit_file` capability that does not exist outside a compiler fixture, and L9 needs a summarizer profile routed through the proxy. Context policy is reserved because it is probably inert on the wire. Temperature is reserved because its honouring and default are unknown.
Status: UNVERIFIED (design proposal resting on VERIFIED E2/E1/C1 facts)
Evidence kind: design proposal + source inspection
Evidence: E2 §6.1 table and G6/G8/G11/G16/G17/G18 rows; E2 l.57 and `s3_2.rs:28–60` (as cited by E2: `edit_file` is a test-fixture capability); E2 F13; C2 l.290 (sampling forwarded, no defaults injected); C1 F12; E1 l.92–105, l.195.
Retrieved: 2026-11-07T00:02Z–00:12Z
Implication for the spec: REQ-XD-201…203. If the `edit_file` capability ticket is not funded, L7 is `withheld{dependency}` and the registry has eight levels. That changes trials by −55 (T11, k = 5).
Outline delta: EXTENDS OL-07 — the brief's "compaction policy, context strategy, tool surface" each receive an explicit disposition: composite level, reserved, conditional.
Limitations: Effect plausibility is argued, not measured. L6's behavioural exercise is unmeasurable beyond wire visibility.

### F6 — Liveness for executor-level knobs cannot use observation replay
Claim: D2's L1.1 leg replays recorded observations into H, so H's executor is a test double. A knob that acts in the executor, such as the command timeout, produces no request diff under replay, and REQ-XE-125 would wrongly refuse it as `inert`.

Executor knobs therefore need an executed liveness leg: a scripted model, $0 model spend, in a real cell, running a command that crosses the baseline timeout. Renderer knobs (L4, L5, L6), proxy knobs (L1, L2), tool-surface knobs (L7) and composite compaction levels (L8, L9 on a synthetic long trajectory) can use L1.1.
Status: UNVERIFIED (inference from D2's leg design and E2's G17 closure)
Evidence kind: inference
Evidence: D2 Part 1 summary in its answer ("L1.1 is exact construction conformance, with observation replay"; K2 "an HH exec-seam test double that serves recorded raw observations"); E2 G17 (timeout → rc −1 via the result renderer); E2-T6's live leg, marked `#[ignore = "live — ADR-0363 …"]`.
Retrieved: 2026-11-07T00:01Z–00:03Z
Implication for the spec: REQ-XD-203 adds the executed leg. It runs on the phase VM before G-P4-X, at $0 model spend.
Outline delta: EXTENDS OL-07; refines REQ-XE-125.
Limitations: I did not read K2's design detail. A replay executor that also replays durations would still not reproduce kill semantics.

### F7 — The prompt ablation must keep the submit instruction
Claim: mini's instance template carries the submit protocol inside its numbered workflow (step 6: `echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT`). An ablation of the "recommended workflow block" that also removed step 6 would test whether the model can submit at all, not whether workflow guidance matters. L6 therefore removes the guidance steps and keeps step 6 and the command-environment rules. Its declared-delta check asserts that the submit marker string is still present in the rendered request.
Status: VERIFIED (template excerpt); UNVERIFIED (the level definition)
Evidence kind: source inspection + design proposal
Evidence: E1 l.114–131 (template excerpt: "6. Submit your changes and finish your work by issuing the following command: `echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT`"); E2 G10 (submit marker detection).
Retrieved: 2026-11-07T00:12Z
Implication for the spec: D3-a authors L6's leaf from the pinned `mini.yaml` and lists the removed lines in the registry.
Outline delta: EXTENDS OL-07.
Limitations: I read only E1's excerpt of the 97-line template, not the full file.

### F8 — Activation is measured per level, counterfactually where the knob is deterministic, and forecast before spend
Claim: The §4.2 activation definitions extend to every conditional level.
- Compaction: the four §4.2 ratios.
- L4: the share of first subjects with ≥ 1 observation whose L4 rendering differs from the baseline rendering of the same raw output. This is exact, because both renderers are deterministic functions of the recorded raw output.
- L3: the share with ≥ 1 command running > 30 s.
- L5: the share of requests with a dropped reasoning member.
- L7: `edit_file` use.
- Always-on knobs (L1, L2, L6): fidelity. Wire activation is 1 by construction.

A forecast from non-outcome records withholds levels below a 0.25 floor before spend. Activated-only contrasts remain exploratory, post-treatment subsets (§4.2).
Status: UNVERIFIED (design proposal over VERIFIED definitions)
Evidence kind: design proposal + source inspection
Evidence: R8 l.252 (activation/fidelity row) and §4.2 closing text ("Untriggered subjects remain in task success; activated-only analysis is exploratory post-treatment selection"); E1 l.195, l.200 (timeout, truncation semantics); E2 G18 (raw output stays a durable blob behind `raw_output_ref`).
Retrieved: 2026-11-07T00:04Z–00:12Z
Implication for the spec: REQ-XD-208, REQ-XD-209; ticket D3-b.
Outline delta: EXTENDS OL-07 (thesis T6 becomes measurable per level).
Limitations: The 0.25 floor is a placeholder. A low-activation knob with a large per-activation effect would be withheld; the forecast is reported so that this is visible.

### F9 — Exploration: X0 smoke, then one OFAT batch on the 11 band-A development tasks at k = 5 (550 trials, about $28–$187)
Claim: The Part 2.5 table shows the trade-off. Going from 11 to 20 tasks improves P(best selected) by about 5 points and P(confirm gain) by about 3 points, for 50–450 more trials, with nine longer-band tasks and a stratum crossing. k = 5 keeps the admitted estimators' floors satisfied for descriptive use. Mean exploration half-widths are about ±.14, so exploration can screen only large effects. That is its role.
Status: VERIFIED as simulation output under the stated DGP; UNVERIFIED for the real workload
Evidence kind: recorded execution (local simulation) + design proposal
Evidence: `sweep_results.json` (`board`, σ = .75, rows for designs T11k3…T20k5, `mmax` = 2); D1 F4 (dev pool 11/5/4, P ⊂ band-A dev); Q-X31 verbatim; C3 planning table (mini $0.155 no-cache, $0.043 cached).
Retrieved: 2026-11-07T00:06:34Z (simulation finished)
Implication for the spec: REQ-XD-206, REQ-XD-207, REQ-XD-210. Q-X40's dev-pool size matters directly. A smaller dev pool shrinks band-A dev below 11, and the T11 rows degrade further.
Outline delta: EXTENDS OL-07 and OL-03 (phase-4 budget envelope).
Limitations: Several band-A dev tasks may be saturated, so their zero contrast lowers the effective T. Per-level cost multipliers are priors until X0. Wall-days ignore queueing.

### F10 — Selection: at most two configurations, one per factor, positive exploration estimate, gated; the packet is deterministic and blind to held-out records
Claim: Under the frozen rule (Part 3, T11 k = 5, m = 2), the simulation showed the following.
- When one level truly gains 10 points, it is selected 0.82–0.84 of the time and confirmed 0.46–0.56 of the time.
- When one gains 15 points, it is selected 0.97–0.98 of the time and confirmed 0.89–0.94 of the time.
- The directional false-confirmation probability was ≤ 0.026 in every scenario.
- The selected exploration estimate's optimism was +.03 to +.10.
- m = 1 gives similar or slightly higher power for "at least one confirmed" (S1: 0.53 vs 0.49 at T20k3b6, σ = .75) at a third less confirmation cost. m = 2 adds a second confirmable configuration and makes the combination possible.
Status: VERIFIED as simulation output; UNVERIFIED (rule)
Evidence kind: recorded execution + design proposal
Evidence: `sweep_results.json` (S1/S3/S4 rows, `p_best_selected`, `p_true_gain_confirmed`, `p_false_gain_confirmed`, `bias_explore`, `bias_confirm`). The simulation omits gates (a)–(f) and the one-per-factor rule.
Retrieved: 2026-11-07T00:06:34Z
Implication for the spec: REQ-XD-211…213. m_max is an operator choice (Open questions). The recommendation is 2.
Outline delta: EXTENDS OL-07 (multiplicity and winner's-curse controls), OL-15.
Limitations: The normal-approximation sign flip is anti-conservative (D1), so real power is lower. Real effects may be heterogeneous between dev and held-out tasks beyond the simulated σ.

### F11 — Two-stage validity holds, but power for realistic small effects is low; "not confirmed" is the probable and publishable outcome
Claim: Confirmation runs on tasks, slots and a window independent of selection. The Holm family over the selected members is therefore pre-registered and valid conditional on selection. The selection procedure affects power, not error control.

Expected configuration effects are a few points, and P(confirm) at Δ = .05 is 0.13–0.26 (T = 37, k = 5, m ≤ 2). The probable confirmatory outcome is therefore "not confirmed", and the pre-registration must state its reporting template in advance.
Status: VERIFIED as simulation output; UNVERIFIED (the expectation about effect sizes is inference)
Evidence kind: recorded execution + inference
Evidence: `conf_results.json` (power_m1/m2 at δ = .05 across mixtures and σ); `sweep_results.json` S2 rows (`p_any_confirmed_gain` 0.10–0.15); C3 F2 (between-harness gaps on GLM mostly ≤ 4 points; within-harness knob effects are plausibly smaller, by inference); D1 F5/F6.
Retrieved: 2026-11-07T00:06:43Z
Implication for the spec: REQ-XD-219 (null templates). An operator wanting real power at 5 points would need about 4–5× the tasks, which exceeds the 67-task held-out pool. That limit should be stated rather than hidden.
Outline delta: CONFIRMS thesis T4's analogue for knobs; EXTENDS OL-07.
Limitations: Effect sizes may be larger for harmful levels. Confirmation tests two-sided, but a level is confirmed only if it is selected as positive.

### F12 — Confirmation: the baseline re-run plus the selected configurations on the 37-task band-A held-out census, k = 5, one Holm family (555 trials at m = 2, about $28–$189)
Claim: See the Part 4.2 table. T = 37, k = 5 meets the precision target (±.077 bootstrap, ±.066 FM) for any m. T = 54 (A ∪ B) buys power (0.75 at Δ = .10, m = 2) for 255 more trials and an exploration-to-confirmation stratum mismatch.
Status: VERIFIED as simulation output; UNVERIFIED (design)
Evidence kind: recorded execution + design proposal
Evidence: `conf_results.json`; D1 F3–F5, F8 (estimand, census, schedule exchangeability, many-vs-one Holm); D2 Part 6 (re-run baseline; L2 data not reused); R8 l.275 (sign flips, Holm, unavailable members).
Retrieved: 2026-11-07T00:06:43Z
Implication for the spec: REQ-XD-215…218.
Outline delta: EXTENDS OL-07, OL-03.
Limitations: This depends on D1's `slot_schedule_randomization` admission and n-arm protocol. Without them the sign flip refuses (only `lane_randomization` is admitted today, per D1 F1/F8).

### F13 — OFAT cannot estimate interactions; a combination is admissible only as a pre-declared, confirmation-only member
Claim: OFAT estimates each level's effect at the baseline point only. It says nothing about effects at other points, and it cannot detect, for example, that compaction matters more at `max` effort.

The combination C₁₂ of the two selected levels can be confirmed against the baseline. It tests that specific combination and does not separate main effects from interaction. Its document differs from the baseline in two factors, so D2's "exactly one registered slot" rule (REQ-XD-125) needs a confirmation-only extension: a union-path declared-delta check, labelled "combination; interaction not estimated".

A fractional or full factorial over two selected levels would estimate the interaction, but it needs 4 arms and was not costed here.
Status: VERIFIED (validator: fractional designs need named-interaction resolution, `experiment.rs` l.1595–1623); UNVERIFIED (proposal)
Evidence kind: source inspection + design proposal
Evidence: `experiment.rs` l.1580–1640 (fractional/factorial handling); D2 REQ-XD-125.
Retrieved: 2026-11-07T00:04Z
Implication for the spec: REQ-XD-214. `combine` defaults to `false`, because adding the member drops m = 2 power at Δ = .10 from 0.55 to 0.46.
Outline delta: EXTENDS OL-07.
Limitations: The additivity assumption behind choosing C₁₂ is untestable from OFAT data.

### F14 — Order: after G-L1 for the exact H; recommended after the L2 verdict; never concurrent with L2
Claim: The sweep's baseline is defined only once G-L1 has passed. L2's account-exclusivity and shared concurrency make simultaneous runs incompatible. An L2-forced H repair would orphan exploration data. Exploration before L2 is technically valid but carries the weaker baseline label permanently. Confirmation should be pre-registered only after the L2 verdict is recorded.
Status: UNVERIFIED (design proposal grounded on D2 constraints)
Evidence kind: design proposal
Evidence: D2 §2.1 ("Account | the Ollama account is exclusive during the tranche"), REQ-XD-102 (H changes force an L1 rerun and a new cohort), Part 6 (baseline labels); C1 F11 (3 concurrent requests).
Retrieved: 2026-11-07T00:01Z–00:03Z
Implication for the spec: REQ-XD-220, REQ-XD-221. H1 places D3-d after the L2 results packet.
Outline delta: EXTENDS OL-07, OL-14 (decomposition order).
Limitations: If L2 is never funded, phase 4 runs on the L1-conformant label. That is valid but weaker.

### F15 — "Which configurations perform best" is published as an exploratory ranking plus confirmatory status, under extended claims checks
Claim: The brief's phrase cannot be rendered as a superlative claim. `check-position.sh` bans "outperform", "superior", "best-in-class", "significant improvement" and "statistically significant", on `docs/evidence.md` and the README spine only. Phase 4 needs program additions on the site and in the packets (Part 5.4), and the two-statement form.
Status: VERIFIED (banned list); UNVERIFIED (additions)
Evidence kind: source inspection + design proposal
Evidence: `scripts/check-position.sh` l.316–328; D1 F16 (program additions; the check applies to the site via F2).
Retrieved: 2026-11-07T00:12Z
Implication for the spec: REQ-XD-219, REQ-XD-222; F2 owns the check's mechanics.
Outline delta: EXTENDS OL-15; feeds OL-18 via F2.
Limitations: Phrase lists cannot catch every paraphrase. The publication gate's human read remains.

### F16 — Phase 4 decomposes into nine tickets, two spend gates, a HUMAN review and a publication gate
Claim: See Part 6. Six of the nine tickets have fully offline deterministic acceptance. D3-a's executed liveness leg runs at $0 model spend in a real cell. D3-d, D3-e and D3-h are live and gated.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: E2 Part 7 (T6–T10 seams); D1 F17; D2 Part 7.
Retrieved: 2026-11-07T00:02Z
Implication for the spec: H1 sequences D3-a…i after E2-T10, D2 K4/K5 (G-L1), D1-a/b, and the L2 results.
Outline delta: EXTENDS OL-14, OL-15.
Limitations: No ticket was sized against implement-spec budgets. H1 sizes them.

---

## Answer and completion assessment

**Answer.** Phase 4 is an OFAT exploration around the frozen HH-native mini-swe-agent baseline, followed by a pre-registered held-out confirmation. The parts are:

- the Tier-1 registry of nine levels across seven factors, plus seven reserved candidates (Part 1);
- the compaction factor, admitted only as composite "S under declared W" levels, with W chosen by rule from non-outcome token records (F3, F4);
- liveness checks before spend, including an executed leg for executor knobs (F6), and per-level activation forecasting and measurement (F8);
- X0 smoke, then X1 on the 11 band-A dev tasks at k = 5 (F9);
- a frozen, deterministic selection rule with m_max = 2 and an optional pre-declared combination (F10, F13);
- confirmation on the 37 band-A held-out tasks at k = 5 with Holm over the selected members (F11, F12);
- ordering after G-L1, recommended after the L2 verdict (F14);
- two-statement claims language (F15);
- nine tickets (F16).

**Settled by source inspection:**
- the Lab OFAT validator and level types exist and suit both stages (F1);
- the factor vocabulary lacks a model-parameter kind (F2);
- compaction is unwired, and inert at the catalog window (F3);
- the supplement's padding ban and its requirement of a separate tuning design (F4);
- the banned-phrase list (F15).

**Designed, with simulation support:** exploration and confirmation sizing, selection behaviour and winner's-curse magnitude (F9–F12).

**Technical blockers (owned by other rows), before any phase-4 spend:**
- G-L1 for H (D2);
- E2-T6, T7, T9 and T10;
- D2 K4;
- D1's comparative protocol and `slot_schedule_randomization` admission;
- the effort home (F2);
- for L7, an exec-backed `edit_file` capability;
- for L9, a summarizer profile on the proxy lane.

**Acceptable uncertainty for this row:**
- real effect sizes, peak-token distributions, activation rates and per-level costs. X0 and the forecast measure these by design;
- whether L2 ends `equivalent`, which only changes the label.

## Open questions

Each question lists its owner, the affected rows and whether an operator decision is needed.

1. **Q-D3-1: compaction under a declared cap.** Owner: operator, at ratification. Affects: E2, F2, H1. Operator decision: **yes**. Accept "strategy under a declared W-token window" as the phase-4 compaction factor, with W set by the Part 2.4 rule and the l.234 ban read as round-5-scoped (F3, F4)? The alternative is to drop compaction from phase 4 as inert on this workload.
2. **Q-D3-2: effort as a factor.** Owner: operator, with Q-X22. Affects: C2, D1, E2. Operator decision: **yes**. Recommendation: pin phases 2–3 at `high` (D1 F15) and sweep `low` and `max` in phase 4 (L1, L2). The effort home is a separate E2/H1 question (Q-D3-7).
3. **Q-D3-3: exploration scope.** Owner: operator, at G-P4-X. Affects: H1, and Q-X40. Operator decision: **yes**. Choose between:
   - 11 band-A dev tasks at k = 5: 550 trials, about $28–$187, ceiling about $337. Recommended, per Q-X31;
   - all 20 dev tasks at k = 3 / k_b = 6: 660 trials, about $33–$224, ceiling about $404.
4. **Q-D3-4: confirmation breadth.** Owner: operator, at ratification or G-P4-C. Affects: H1, F2. Operator decision: **yes**. Choose m_max (2 recommended; 1 is cheaper and about as powerful for "at least one"), whether to add the combination (`combine`; default false), and the scope: T = 37 band A (recommended) or T = 54 A ∪ B.
5. **Q-D3-5: `edit_file` capability.** Owner: operator via H1. Affects: E2, H1. Operator decision: **yes**. Fund an exec-backed `edit_file` capability ticket so that L7 ("tool surface", named in brief §2) runs, or withhold L7?
6. **Q-D3-6: phase-4 order.** Owner: operator via H1. Affects: D2, H1. Operator decision: **yes**. Exploration after the L2 verdict (recommended), or permitted right after G-L1 if L2 funding is deferred (F14)?
7. **Q-D3-7: effort home.** Owner: E2/H1. Affects: C2, E2. Operator decision: no, unless an ontology ADR is contested. Effort becomes a declared lane parameter in the sealed level document, or a new factor kind is added (F2).
8. **Q-D3-8: cost-oriented track.** Owner: operator, later. Affects: D1, H1. Operator decision: **yes**, if wanted. A confirmatory cost-reduction claim (for step limit, observation truncation or compaction) would need an admitted continuous-outcome primary and, for "no worse", a non-inferiority method. Neither exists. Recommendation: cost stays descriptive in this round.

Pending §Q rows that bear on this design: Q-X22 (effort), Q-X40 (dev-pool size; D3 needs the 11 band-A dev tasks), Q-X42 (Terminus-2 staging; Terminus sweeps are out of this round's scope), Q-X23 (S0 smoke; temperature reservation).

## Spec requirements emitted

| id | requirement (candidate) | rationale / findings | OL | acceptance |
|---|---|---|---|---|
| REQ-XD-201 | Phase-4 levels SHALL be declared in a frozen `sweep-registry/1` record. Each level declares its factor, typed paths, baseline and level values, tier, exercising condition, activation metric, forecast source, cost prior and dependency tickets. The registry hash is in the exploration pre-registration. | F5 | OL-07, OL-15 | Validator rejects a missing member; the hash predates X0 subjects |
| REQ-XD-202 | The Tier-1 registry SHALL be L1–L9 of Part 1.1, and Tier 2 SHALL be recorded with its reasons. A level added or changed after the freeze creates a new exploration identity whose data never merge with X1. | F5 | OL-07 | Merge-refusal fixture across exploration identities |
| REQ-XD-203 | Each level SHALL be a sealed document from `enumerate` (REQ-XE-124) and SHALL pass declared-delta and liveness (REQ-XD-125, REQ-XE-125) before X0. Executor-level knobs SHALL pass an executed scripted-model liveness leg in a real cell at $0 model spend. | F1, F6 | OL-07 | `cmd-120` executed-leg fixture (rc −1 vs 0); inert fixture refused |
| REQ-XD-204 | Reasoning effort, when swept, SHALL be carried by a declared lane parameter of the level document (or an ADR-admitted factor kind), enforced by the proxy pin, with requested and applied values per call. A level with any call whose applied value differs from the declaration is ineligible for selection. | F2 | OL-07 | Fixture: one mis-pinned call ⇒ level ineligible |
| REQ-XD-205 | A compaction level SHALL be a composite of strategy S and a declared `window_cap` W, applied identically to every task and replicate. W SHALL be chosen by the pre-registered rule from non-outcome token records (≥ 32 768; target F(W) ≥ 0.5; withhold if F(32 768) < 0.25). Task-conditioned caps and input padding SHALL refuse. | F3, F4 | OL-07, OL-15 | W-rule fixtures; task-varying W refused |
| REQ-XD-206 | Exploration SHALL be a Lab `one_factor_at_a_time` design (base plus single perturbations) on the 11 band-A development tasks at k = 5 per arm, seeded `slot_schedule_randomization` in 5 rounds, preceded by a separately identified X0 smoke (3 dev tasks × 1 × all arms). X0 and X1 SHALL never be pooled with each other or with confirmation. | F9 | OL-07 | Design validates; pooling refusal fixture |
| REQ-XD-207 | Exploration SHALL report, per level, Δ̂ (equal-task, joint population) with descriptive bootstrap and FM intervals, BH q, activation, fidelity, completion envelope and costs, all labelled exploratory. No exploration figure SHALL carry a confirmatory label. | F9 | OL-07, OL-15 | Label check over the exploration packet |
| REQ-XD-208 | A §4.2-generic activation fold SHALL compute per-level activation, including exact counterfactual activation for deterministic renderer and executor knobs. `hh-activation/1` SHALL remain unchanged. | F1, F8 | OL-07 | Known-answer fixtures per metric |
| REQ-XD-209 | Before X1 spend, a forecast from non-outcome baseline records SHALL withhold levels below the activation floor (proposed 0.25) as `withheld{inert_predicted}`, retained in the registry with the forecast. | F8 | OL-07 | Input manifest refuses outcome fields; withheld fixture |
| REQ-XD-210 | Exploration stop rules SHALL be budget, infrastructure, integrity and catalog-identity only. No outcome-based stopping or level dropping. | F9; R8 l.227 | OL-07, OL-15 | Stop-rule fixtures |
| REQ-XD-211 | The selection rule SHALL be frozen in the exploration pre-registration: gates (a)–(f), score Δ̂ > 0, one level per factor, m_max (proposed 2), tie-breaks, `combine` flag and `no_candidate` disposition (Part 3). | F10 | OL-07 | Rule fixtures incl. `no_candidate` |
| REQ-XD-212 | The selection packet SHALL be a deterministic function of committed X1 records and the frozen rule. Its input manifest SHALL exclude held-out and phase-2/3 outcome records, and its hash SHALL predate the confirmation pre-registration. | F10 | OL-07, OL-15 | Byte-identical recompute; held-out-input refusal |
| REQ-XD-213 | A selected configuration's exploration estimate SHALL be displayed only with its selection label and the simulated expected optimism. The confirmatory estimate SHALL be the claim. | F10 | OL-07, OL-15 | Claims-check fixture |
| REQ-XD-214 | A combined configuration SHALL be admissible only when pre-declared, only in confirmation, with a union-path declared-delta check, labelled "combination; interaction not estimated". | F13 | OL-07 | Union-path fixture; undeclared combination refused |
| REQ-XD-215 | Confirmation SHALL run the baseline re-run plus the selected members in one window on the band-A held-out census (T = 37, or a pre-declared A ∪ B census), at k = 5, with a seeded schedule. Baseline data from L2 or phase 2 SHALL NOT be reused. | F12, F14 | OL-07 | Manifest equals the census; reuse refusal |
| REQ-XD-216 | Confirmation SHALL estimate each member's finite-manifest equal-task Δ vs the baseline (joint population), with FM credible and task-bootstrap intervals, and test with Holm α = .05 over all selected members (unavailable members retained), two-sided sharp-null sign flips under `slot_schedule_randomization`. | F11, F12 | OL-07 | Fixture recomputation; Holm with an unavailable member |
| REQ-XD-217 | The confirmation pre-registration SHALL list the Part 5.3 members and SHALL be frozen and gated (G-P4-C) before any confirmation subject. | F12 | OL-07, OL-15 | Hash precedes subjects |
| REQ-XD-218 | The confirmation quote SHALL compute precision with D1's comparative battery (REQ-XD-024), extended with selection-conditioned scenarios, and cost from X0/X1-measured per-level $/trial. | F12 | OL-07, OL-03 | Quote recompute; battery records retained |
| REQ-XD-219 | Null and inconclusive confirmations SHALL use pre-registered templates ("not confirmed; interval includes 0"), never equality or equivalence wording. `withheld{no_candidate\|budget\|infra}` are typed dispositions. | F11, F15 | OL-07, OL-15 | Mutation battery |
| REQ-XD-220 | Phase-4 exploration SHALL require a G-L1 pass for the exact H swept. It SHOULD follow the recorded L2 verdict and SHALL NOT run concurrently with L2. Any H change SHALL start a new phase-4 cohort. | F14 | OL-07, OL-14 | Runner refuses without a matching L1 pass id |
| REQ-XD-221 | Phase-4 spend SHALL be authorized at G-P4-X (X0 + X1) and G-P4-C (confirmation), recorded verbatim, outside the $60 pilot authorization. | F9, F12 | OL-15 | Gate entries precede subjects |
| REQ-XD-222 | Phase-4 results SHALL carry the baseline label (REQ-XD-126), finite-manifest scope, the OFAT local-effect caveat, the W label for compaction levels, and the catalog window. "Which configurations perform best" SHALL be rendered as the two-statement form. The site check SHALL add the F15 banned and conditional phrases. | F15 | OL-07, OL-15 | Claims check over the packet and site |
| REQ-XD-223 | Every phase-4 scored trial SHALL pass `answer-scan/1` under the allowlisted network, and flagged passes SHALL be HUMAN-reviewed before each results freeze (REQ-XD-070/071). | D1 F13 reuse | OL-15 | Review records complete |
| REQ-XD-224 | Per-level cost, $/solve, tokens and calls SHALL be reported descriptively. No confirmatory cost or no-worse claim SHALL be made without an admitted method. | F11; Q-D3-8 | OL-07, OL-15 | Label check |
| REQ-XD-225 | A Tier-2 candidate SHALL enter only a later exploration identity, after its dependency lands and its forecast activation meets the floor. | F5 | OL-07 | Admission fixture |

## Handoff

**Dependencies resolved.**
- From D1: estimand, k, census, schedule exchangeability, Holm, pilot firewall, cost priors, ticket pattern.
- From D2: baseline definition and labels, declared-delta check, no reuse of L2 data, account exclusivity.
- From E2: knob homes, the `enumerate` → `LevelSpec.ref_` glue, liveness gate, compaction wiring, window-condition rule.
- Skimmed: C1 (window, effort, sampling), E1 (no compaction in mini, the Terminus trigger, templates and limits), C3 (priors).

**Interfaces asserted on other rows:**
- **E2:** T7 must provide a `window_cap` parameter. T10's liveness must admit an executed leg for executor knobs (F6). Choose the effort home (Q-D3-7). An `edit_file` exec capability is needed if L7 is funded.
- **D2:** K4 must expose the per-level declared-delta check over the synthetic corpus. Recordings from K6 are read only for non-outcome fields.
- **D1:** `slot_schedule_randomization` and the n-arm protocol must cover OFAT arms. The comparative battery needs a selection-conditioned scenario.
- **C2:** per-arm effort pin; the summarizer lane for L9; subject-charged compaction calls.
- **B2:** arms per sealed level.
- **F2:** the two-statement rendering and phrase additions.
- **H1:** the D3-a…i order, the gates and the envelope (about 1,135 trials; planning ceiling about $695).

**Proposed follow-up rows.** None as research rows. H1 should record Q-D3-1…8. The l.234 reading (F4) should go to O2's adversarial review.

**Conflicts with other notes.**
- E2 §6.1 lists compaction candidates `evict_oldest` and `fresh-window-with-notes` among others. This note chooses `clear_tool_results` (model-free) and `summarize-rolling` (model-based) as the two composite levels. `evict_oldest` stays the kernel fallback inside both (`compact.rs` l.33–36). This is a refinement, not a contradiction.
- E2 lists effort outside the harness-knob registry. This note admits it as a factor only through a declared home (F2), which is consistent with E2's "D3 may cross it".
- D1 F4 says phase 4 explores on D "including P". This note uses only band-A D for X1 and leaves the bands B/C dev tasks unused, which is consistent with D1.

**Capability and portfolio dispositions.** None changed. Nothing here is fixture-, live- or foreign-run-verified. The simulation outputs are local arithmetic under stated assumptions.

---

## Appendix A — simulation code (verbatim, as run)

`sweep_sim.py` (run as `python3 -I sweep_sim.py 1000`; SHA-256 `fb3e175d…4800a`):

```python
"""D3 sweep-method design aid: OFAT exploration on a dev pool -> mechanical
selection -> held-out confirmation with Holm.  Standard library only.
Not the admitted section-4.3 battery; a planning simulation."""
import random, math, json, sys

SEED = 20261109
MIX = {'board': (0.575, 0.384), 'easyband': (0.636, 0.273)}
CELLS = [('board', 0.25), ('board', 0.75), ('easyband', 0.75)]
TARGETS = [-0.15, -0.10, -0.05, -0.03, 0.0, 0.03, 0.05, 0.10, 0.15]
# true marginal effects of the 9 registry levels, per scenario
SCEN = {
    'S0_null':      [0, 0, 0, 0, 0, 0, 0, 0, 0],
    'S1_one10':     [0.10, 0, 0, 0, 0, 0, 0, 0, 0],
    'S2_mixed':     [0.05, 0.03, 0, 0, 0, -0.03, -0.05, -0.10, -0.15],
    'S3_two_good':  [0.10, 0.05, 0, 0, 0, 0, -0.05, -0.10, -0.15],
    'S4_one15':     [0.15, 0, 0, 0, -0.03, -0.05, -0.05, -0.10, -0.15],
}
# exploration designs: (label, T_dev, k_level, k_base)
DESIGNS = [('T11k3', 11, 3, 3), ('T11k5', 11, 5, 5), ('T20k2b4', 20, 2, 4),
           ('T20k3', 20, 3, 3), ('T20k3b6', 20, 3, 6), ('T20k5', 20, 5, 5)]
T_CONF, K_CONF, ALPHA = 37, 5, 0.05
MMAX = [1, 2, 3]
NDATA = int(sys.argv[1]) if len(sys.argv) > 1 else 1000


def clip(p): return min(max(p, 0.005), 0.995)
def logit(p): return math.log(p / (1 - p))
def expit(x): return 1 / (1 + math.exp(-x))
def Phi(x): return 0.5 * (1 + math.erf(x / math.sqrt(2)))


def calib(rng, mix, sigma, target, n=40000):
    a, b = MIX[mix]
    base = [clip(rng.betavariate(a, b)) for _ in range(n)]
    gs = [rng.gauss(0, sigma) for _ in range(n)]
    def marg(beta):
        return sum(expit(logit(p) + beta + g) - p for p, g in zip(base, gs)) / n
    lo, hi = -6.0, 6.0
    for _ in range(50):
        mid = (lo + hi) / 2
        if marg(mid) < target: lo = mid
        else: hi = mid
    return (lo + hi) / 2


def binom(rng, k, p):
    return sum(1 for _ in range(k) if rng.random() < p)


def pval(d):
    s = sum(d); ss = sum(x * x for x in d)
    if ss == 0: return 1.0, s
    z = abs(s) / math.sqrt(ss)
    return 2 * (1 - Phi(z)), s


def holm(ps):
    order = sorted(range(len(ps)), key=lambda i: ps[i])
    rej = [False] * len(ps); m = len(ps)
    for r, i in enumerate(order):
        if ps[i] <= ALPHA / (m - r): rej[i] = True
        else: break
    return rej


def main():
    rng = random.Random(SEED)
    beta = {}
    for mix, sig in CELLS:
        for t in TARGETS:
            beta[(mix, sig, t)] = calib(rng, mix, sig, t)
    out = {'seed': SEED, 'ndata': NDATA, 'beta': {f'{m}|{s}|{t}': v for (m, s, t), v in beta.items()}, 'cells': []}
    for mix, sig in CELLS:
        a, b = MIX[mix]
        for sname, eff in SCEN.items():
            L = len(eff)
            bl = [beta[(mix, sig, e)] for e in eff]
            best = max(range(L), key=lambda i: eff[i]) if max(eff) > 0 else None
            acc = {d[0]: {m: dict(nsel=0, nonesel=0, anyconf=0, falseconf=0, trueconf=0,
                                  bestsel=0, bias_e=0.0, bias_c=0.0, nbias=0) for m in MMAX}
                   for d in DESIGNS}
            hw = {d[0]: 0.0 for d in DESIGNS}
            for _ in range(NDATA):
                # exploration tasks (max T over designs; designs nest on first T tasks / first k)
                Tmax = 20; kmax = 6
                pt = [clip(rng.betavariate(a, b)) for _ in range(Tmax)]
                gam = [[rng.gauss(0, sig) for _ in range(Tmax)] for _ in range(L)]
                ub = [[rng.random() for _ in range(kmax)] for _ in range(Tmax)]
                ul = [[[rng.random() for _ in range(5)] for _ in range(Tmax)] for _ in range(L)]
                plt = [[expit(logit(pt[t]) + bl[l] + gam[l][t]) for t in range(Tmax)] for l in range(L)]
                # confirmation tasks and draws (shared across designs: CRN)
                pc = [clip(rng.betavariate(a, b)) for _ in range(T_CONF)]
                gc = [[rng.gauss(0, sig) for _ in range(T_CONF)] for _ in range(L)]
                cb = [binom(rng, K_CONF, pc[t]) for t in range(T_CONF)]
                cl = [[binom(rng, K_CONF, expit(logit(pc[t]) + bl[l] + gc[l][t])) for t in range(T_CONF)] for l in range(L)]
                for (dl, T, k, kb) in DESIGNS:
                    cbase = [sum(1 for r in range(kb) if ub[t][r] < pt[t]) / kb for t in range(T)]
                    est = []
                    for l in range(L):
                        d = [sum(1 for r in range(k) if ul[l][t][r] < plt[l][t]) / k - cbase[t] for t in range(T)]
                        m_ = sum(d) / T
                        sd = math.sqrt(sum((x - m_) ** 2 for x in d) / (T - 1))
                        hw[dl] += 1.96 * sd / math.sqrt(T) / L
                        est.append(m_)
                    ranked = sorted([l for l in range(L) if est[l] > 0], key=lambda l: (-est[l], l))
                    for m in MMAX:
                        A = acc[dl][m]
                        sel = ranked[:m]
                        if not sel:
                            A['nonesel'] += 1; continue
                        A['nsel'] += len(sel)
                        if best is not None and best in sel: A['bestsel'] += 1
                        ps, ss, ests_c = [], [], []
                        for l in sel:
                            d = [(cl[l][t] - cb[t]) / K_CONF for t in range(T_CONF)]
                            p, s = pval(d); ps.append(p); ss.append(s); ests_c.append(s / T_CONF)
                        rej = holm(ps)
                        anyc = fc = tc = False
                        for i, l in enumerate(sel):
                            A['bias_e'] += est[l] - eff[l]; A['bias_c'] += ests_c[i] - eff[l]; A['nbias'] += 1
                            if rej[i] and ss[i] > 0:
                                anyc = True
                                if eff[l] <= 0: fc = True
                                else: tc = True
                        A['anyconf'] += anyc; A['falseconf'] += fc; A['trueconf'] += tc
            for (dl, T, k, kb) in DESIGNS:
                for m in MMAX:
                    A = acc[dl][m]; nb = max(A['nbias'], 1)
                    out['cells'].append(dict(mix=mix, sigma=sig, scen=sname, design=dl, T=T, k=k, kb=kb, mmax=m,
                        explore_trials=(L * k + kb) * T,
                        explore_hw=round(hw[dl] / NDATA, 4),
                        p_none_selected=round(A['nonesel'] / NDATA, 3),
                        mean_selected=round(A['nsel'] / NDATA, 3),
                        p_best_selected=round(A['bestsel'] / NDATA, 3) if best is not None else None,
                        p_any_confirmed_gain=round(A['anyconf'] / NDATA, 3),
                        p_false_gain_confirmed=round(A['falseconf'] / NDATA, 3),
                        p_true_gain_confirmed=round(A['trueconf'] / NDATA, 3),
                        bias_explore=round(A['bias_e'] / nb, 4),
                        bias_confirm=round(A['bias_c'] / nb, 4)))
            print(mix, sig, sname, 'done', file=sys.stderr, flush=True)
    json.dump(out, open('sweep_results.json', 'w'), indent=1)


if __name__ == '__main__':
    main()
```

`conf_sim.py` (run as `python3 -I conf_sim.py 2000`; SHA-256 `7f4df688…2cc`):

```python
"""D3 confirmation precision/power table: one config vs re-run baseline on T
held-out tasks, k replicates, Holm over m members (Bonferroni alpha/m lower
bound for one non-null member).  Standard library only."""
import random, math, json, sys

SEED = 20261110
MIX = {'board': (0.575, 0.384), 'easyband': (0.636, 0.273)}
NDATA = int(sys.argv[1]) if len(sys.argv) > 1 else 2000


def clip(p): return min(max(p, 0.005), 0.995)
def logit(p): return math.log(p / (1 - p))
def expit(x): return 1 / (1 + math.exp(-x))
def Phi(x): return 0.5 * (1 + math.erf(x / math.sqrt(2)))


def calib(rng, mix, sigma, target, n=40000):
    a, b = MIX[mix]
    base = [clip(rng.betavariate(a, b)) for _ in range(n)]
    gs = [rng.gauss(0, sigma) for _ in range(n)]
    def marg(beta):
        return sum(expit(logit(p) + beta + g) - p for p, g in zip(base, gs)) / n
    lo, hi = -6.0, 6.0
    for _ in range(50):
        mid = (lo + hi) / 2
        if marg(mid) < target: lo = mid
        else: hi = mid
    return (lo + hi) / 2


def main():
    rng = random.Random(SEED)
    rows = []
    for mix, sig in [('board', 0.25), ('board', 0.75), ('easyband', 0.75)]:
        a, b = MIX[mix]
        for delta in [0.0, 0.05, 0.10, 0.15]:
            be = calib(rng, mix, sig, delta)
            for T in [20, 37, 54]:
                for k in [3, 5]:
                    rej = {1: 0, 2: 0, 3: 0, 4: 0}; hwb = 0.0; hwf = 0.0
                    for _ in range(NDATA):
                        d = []; vf = 0.0
                        for t in range(T):
                            p = clip(rng.betavariate(a, b))
                            q = expit(logit(p) + be + rng.gauss(0, sig))
                            cb = sum(1 for _ in range(k) if rng.random() < p)
                            cl = sum(1 for _ in range(k) if rng.random() < q)
                            d.append((cl - cb) / k)
                            for c in (cb, cl):
                                aa, bb = c + 1, k - c + 1
                                vf += aa * bb / ((aa + bb) ** 2 * (aa + bb + 1))
                        m_ = sum(d) / T
                        sd = math.sqrt(sum((x - m_) ** 2 for x in d) / (T - 1))
                        hwb += 1.96 * sd / math.sqrt(T)
                        hwf += 1.96 * math.sqrt(vf) / T
                        ss = sum(x * x for x in d)
                        p = 1.0 if ss == 0 else 2 * (1 - Phi(abs(sum(d)) / math.sqrt(ss)))
                        for m in rej:
                            if p <= 0.05 / m and sum(d) > 0: rej[m] += 1
                    rows.append(dict(mix=mix, sigma=sig, delta=delta, T=T, k=k,
                                     hw_boot=round(hwb / NDATA, 4), hw_fm=round(hwf / NDATA, 4),
                                     **{f'power_m{m}': round(v / NDATA, 3) for m, v in rej.items()}))
            print(mix, sig, delta, file=sys.stderr, flush=True)
    json.dump({'seed': SEED, 'ndata': NDATA, 'rows': rows}, open('conf_results.json', 'w'), indent=1)


if __name__ == '__main__':
    main()
```

The outputs are the two JSON files hashed in the Simulation record. All numbers quoted in Parts 2.5 and 4.2 and in F9–F12 are read from them: the m = 2 rows of `sweep_results.json` for the `board`/σ = .75 cell unless stated otherwise, and `conf_results.json` for the confirmation table.
