# 35_comparative-method — Comparative harness evaluation method
Stream: D
Researched: 2026-11-06
Author: design worker for ledger row D1 (Claude Code subagent, claude-opus-5-5), dispatched by the experiments-program coordinator
Ledger rows covered: D1
Repository baseline: main 09fe8523. Read in worktree `claude/experiments-spec`; HEAD was c12b96b1 at dispatch and 16241b3b at writing. `git diff --stat 09fe8523 HEAD -- crates scripts` is empty, so those commits add only program documents.
Confidence overall: high for what the landed machinery admits and refuses (source inspection at the baseline). Medium for the precision and cost tables: they come from local simulation under stated assumptions, and the cost inputs are priors from C1 and C3, not measurements. Low for the real effect sizes, which nobody has measured on this participant and surface.

**Answer in brief.** Phase 2 can reuse the admitted §4.3 estimators unchanged for its primary claims, with one design choice and three small method admissions.

- **Design choice: k = 5 replicates per (task, harness).** This meets the R ≥ 5 cell floor of `finite_manifest_contrast` and `task_mixture`, and it matches the TB 2.1 leaderboard's ≥ 5 trials rule.
- **Estimand.** The finite-manifest, equal-task mean resolve rate on the executed held-out manifest, under the `joint_task_pass_without_observed_veto` population. A flagged pass is a failure unless review clears it.
- **Primary family.** The H−1 contrasts of each admitted harness against a pre-registered reference (mini-swe-agent). Holm α = .05 over the sharp-null sign-flip p-values, one family per tranche.
- **Three method admissions:**
  1. an n-arm comparative protocol and quote, because `central_protocol` and `exposure_quote` are hard-wired to the two-arm compaction study and refuse an Ollama participant;
  2. a registered schedule-randomization exchangeability argument for the sign flip;
  3. descriptive estimators for flip rates, $/solve, single-arm rate intervals and "tied with leader".

The population is all 89 TB 2.1 tasks. Two QEMU tasks are pre-registered exclusions. The remaining 87 are split by a seeded, stratified draw into a 20-task development pool (reserved for phase 4; the pilot uses 10 of its cheapest-band tasks) and a 67-task held-out pool. Tranche 1 is a **census of the 37 held-out tasks in the cheapest timeout band**, so its finite-manifest claim needs no extrapolation within the band.

In simulation, tranche 1 at k = 5 gives a pairwise 95% task-bootstrap half-width of about ±0.08 (finite-manifest credible interval about ±0.07), at 740 trials for 4 harnesses or 1,480 for 8. At the $0.05–$0.34 per-trial prior that is about $37–$252 or $74–$503 of model spend, beyond the $60 pilot. With HF-board-like harness gaps (mostly ≤ 4 points), most contrasts will be honestly reported as inconclusive. With 8 harnesses, the observed leader is the true best only about 63% of the time.

The ≤ $60 pilot (recommended: 8 harnesses × 10 tasks × 2 attempts plus a 24-trial smoke, about $44 at planning values) measures cost, infra, timeouts, egress and variance only. Its outcomes never select harnesses and never become confirmatory.

## Scope, sources and method

This row designs the phase-2 comparative method. It does not design the runner (B2), the proxy (C2), hosting (C4), compute (B3) or the site (F2). Nothing was executed except local, no-network Python simulations and census arithmetic in the session scratchpad. There was no spend, provisioning, container, build or git branch movement.

`bash scripts/deferrals-open.sh` was **not** run for this row. I relied on C3's record that it printed nothing at 2026-11-06T23:01:16Z. That is inherited evidence, not my own check.

| ID | Source (locator, at baseline 09fe8523 unless noted) | Read / retrieved (UTC) |
|---|---|---|
| R1 | `docs/experiments-brief.md` §1–§5 (A-3…A-10); `docs/experiments-research-ledger.md` (row D1, §Q Q-X1…Q-X38 as present at dispatch, Execution contract); `docs/experiments-OUTLINE_TRACE.md`; `docs/research/CONVENTIONS.md` | 2026-11-06T23:20:36Z–23:21Z |
| R2 | `docs/research/25_tb21-harbor.md` (B1), entire note incl. Appendix A | 23:21Z |
| R3 | `docs/research/26_ollama-glm.md` (C1), entire note | 23:21Z |
| R4 | `docs/research/27_harness-survey.md` (C3), entire note | 23:22Z |
| R5 | `docs/research/24_executed-record-audit.md` (A1), F1–F15 headings, F6–F11, F13–F15 bodies, answer section | 23:22Z |
| R6 | `docs/evidence-and-adoption-spec.md` §3.1, §3.2, §4.1, §4.2, §4.3, §4.5 (lines 195–282, 331–343) | 23:22Z |
| R7 | `crates/hh-eval/src/finite_manifest.rs` lines 1–260, 460–760 (constants, refusal set, `finite_manifest_contrast`, `resolve_core` arm/stratum filtering) | 23:23Z |
| R8 | `crates/hh-eval/src/population.rs` lines 1–80, 136–170, 345–530 (`contrib`, slot resolution, first-admissible retry) | 23:30Z |
| R9 | `crates/hh-eval/src/task_mixture.rs` lines 1–90 (constants, family declaration); `crates/hh-eval/src/stats.rs` symbol list plus lines 555–700 (`pass_k`, `pass_at_k`, `permutation_signflip_p`, `holm`, `sign_profile`) | 23:23Z |
| R10 | `crates/hh-eval/src/sim_battery.rs` lines 1–200 (frozen scenario table, mechanisms, seed rule) | 23:23Z |
| R11 | `crates/hh-eval/src/central_protocol.rs` lines 1–240, 600–700, 860–900 | 23:24Z |
| R12 | `crates/hh-eval/src/exposure_quote.rs` lines 1–210 (stages, components, ceilings) | 23:24Z |
| R13 | `crates/hh-analysis/src/multiplicity.rs` lines 1–60; `rg` locators in `crates/hh-analysis/src/kernel.rs` (routes at 1632–1745, 1789) | 23:24Z |
| R14 | `scripts/check-position.sh` lines 1–40, 300–340 (`BANNED` list) | 23:23Z |
| W1 | `https://huggingface.co/spaces/joelniklaus/harness-leaderboard/raw/main/README.md`, through a summarizing fetcher (near-verbatim, not byte-verbatim) | between 23:20:36Z and 23:23:16Z |
| W2 | `https://huggingface.co/spaces/joelniklaus/harness-leaderboard/raw/main/docs/decisions.md`, same fetcher | between 23:20:36Z and 23:23:16Z |

The notes for C2, D2, B3 and A2 were accepted while this row ran. They were **not read** and nothing here relies on them.

### Simulation record

These are design aids, not the admitted §4.3 battery. They ran on Python 3.15.2 with the standard library only, from `…/scratchpad/D1/`, between 2026-11-06T23:25Z and 23:28:03Z. The scripts are reproduced verbatim in Appendix B. SHA-256 hashes: `sim.py` dadb3512…11c6a, `sim2.py` 96631f11…96b, `sim3.py` 9b3f1d0a…c928, outputs `pair_results.json` 16a5679…179a, `rank_results.json` a7bcb95…f584, `mc_check.json` 9264c32…96b.

**Data-generating process.**
- Base task solve probability: `p_t ~ Beta(a,b)`, clipped to [0.005, 0.995].
- Harness model: `logit p_ht = logit p_t + β_h + γ_ht`, with interaction `γ_ht ~ N(0, σ²)`.
- Outcomes: `c_ht ~ Binomial(k, p_ht)`.
- Mixtures, each calibrated from a mean rate and a mean rerun flip `2E[p(1−p)]`:
  - `board`: Beta(0.575, 0.384); mean .60, flip .235. Taken from C3 F2's GLM-5.3-Flash TB 2.1 rows (flips 16.9–28.4%).
  - `easyband`: Beta(0.636, 0.273); mean .70, flip .20. Stands in for the cheap band, where 13 of 48 tasks are HF-saturated.
  - `hard`: Beta(0.384, 0.575); mean .40.
- `σ ∈ {0, 0.75}`.
- `β_h` is solved by bisection so that the marginal Δ hits the target.

**Estimators.**
- Finite-manifest credible half-width: `1.96·sqrt(Σ_t Var[Beta(c+1,k−c+1)] summed over both arms / T²)`.
- Task-bootstrap half-width: `1.96·sd(d_t)/sqrt(T)`.
- Sharp-null sign-flip p: the normal approximation `2(1−Φ(|Σd|/sqrt(Σd²)))`.
- Power under Holm with m = H−1 members, approximated by the Bonferroni threshold `.05/m`. This is a lower bound for one non-null member.
- Designs are nested (the first T of 87 tasks, the first k of 5 attempts), giving common random numbers across cells.
- 1,000 datasets per cell, so Monte Carlo SE ≤ 1.6 points.
- Seeds: 20261106 (`sim.py`), 20261107 (`sim2.py`), 20261108 (`sim3.py`).

**Checks.**

| check | design | result |
|---|---|---|
| Normal-approx finite-manifest half-width vs 4,000 exact Beta draws | 40 datasets each at T=10/k=5, T=37/k=5, T=37/k=2 | mean difference ≤ 0.0003; max \|diff\| 0.0055 at T=10 |
| Normal-approx sign-flip vs Monte Carlo sign-flip (2,000 draws), Δ ≈ .038 | 300 datasets | decision agreement 97%. Rejection rates: T=10 0.057 vs 0.027; T=20 0.097 vs 0.063; T=37 0.14 vs 0.107. **The approximation is anti-conservative** |
| Power with Monte Carlo sign-flip (999 draws) vs normal approx | 400 datasets, headline cells | T=37, k=5, Δ=.10: m=1 0.552 vs 0.608, m=3 0.365 vs 0.395, m=7 0.237 vs 0.225. T=37, k=5, Δ=.15: 0.89/0.77/0.675 vs 0.912/0.782/0.68 |

The tabled power values therefore overstate exact-routing power by up to about 5 points. Half-widths are unaffected.

## Findings

### F1 — The admitted finite-manifest and task-mixture estimators already compute pairwise harness contrasts over a multi-arm population, but only per pair, per single stratum, with ≥5 replicates and no unresolved member per cell
Claim: `finite_manifest_contrast` filters a planned population to two named arms. That means an H-harness population needs no new estimator for its pairwise contrasts. It refuses the contrast in four cases:
- any `(task, arm)` cell has fewer than 5 replicates (`ShortCell`);
- any cell has an unresolved member (`UnresolvedCell`);
- the candidate slots span more than one stratum without a declared stratum (`StrataSelectionRequired`);
- T < 2.

`task_mixture` carries the same R ≥ 5 floor.
Status: VERIFIED
Evidence kind: source inspection
Evidence:
- R7, `finite_manifest.rs`: `CELL_REPLICATE_FLOOR: u64 = 5`, `BASE_DRAWS = 8_000`, `CAP_DRAWS = 64_000`, `LEVEL_PPM = 950_000`.
- R7, `resolve_core`: it keeps only slots whose `arm_id` equals `test_arm` or `control_arm`. It refuses an undeclared multi-stratum population. Its assumptions record reads "equal within-stratum task weights w_t=1/T; first-slot coverage" and "finite-manifest target only — no population, original-runner or catalogue extrapolation".
- R9, `task_mixture.rs`: `CELL_REPLICATE_FLOOR: u64 = 5`, `BCA_TASK_FLOOR = 30`, `CONFIRMATORY_DRAWS_FLOOR = 2_000`, `REGISTERED_EXCHANGEABILITY = "lane_randomization"`, `ALPHA_PPM = 50_000`.
- R8, `population.rs` `contrib`: in the joint population, a vetoed pass counts as failure `(0,1,0)`; `missing_required_checks` and `success: None` count as unresolved.
Retrieved: 2026-11-06T23:23Z–23:30Z
Implication for the spec:
- Phase-2 confirmatory designs use **k = 5 replicates per (task, harness)**. k = 3, as on the HF board, would need a floor amendment, which I do not recommend.
- Each tranche's executed tasks carry **one analysis-stratum label**, so equal task weights apply over the tranche (F4).
- A cell left unresolved after the frozen reattempt bound blocks that pair's primary inference. The quote must price this risk (F12).
Outline delta: EXTENDS OL-03 — the tranche design's k is set by an admitted floor, not chosen freely; CONFIRMS OL-15 (refusal scaffolding exists).
Limitations: I read the estimator and population code, not their test batteries. The kernel routing (R13) was located by `rg`, not read in full.

### F2 — `central_protocol` and `exposure_quote` are hard-wired to the two-arm compaction study and would refuse phase 2; their selection primitive and quote vocabulary are reusable, their constants are not
Claim: Phase 2 needs a sibling comparative protocol and quote that reuse their primitives, and an ADR-recorded envelope. Flipping the support booleans or editing the constants is not an option. The specific blockers:
- `materialize_protocol` requires exactly the two arm strategies `evict_oldest`/`clear_tool_results` and exactly 30 selected tasks. It also refuses unless `support_route/cache/seed/fingerprint` are all true, and C1 F12 shows the honest Ollama declarations make three of them false.
- `exposure_quote` computes `N = 2TR ≥ 10T` and caps pilot at 60 and confirmation at 300 subjects.
- Its per-subject ceilings (30 wall-minutes, 1M input tokens, 15 verifier minutes) are below TB 2.1's declared timeouts and below the measured mini-swe-agent token use (1.031 Mtok/trial, C3 F2).
Status: VERIFIED
Evidence kind: source inspection
Evidence:
- R11: `pub const TASKS: u32 = 30`, `ROUNDS = 5`, `MAX_CONCURRENT_SUBJECTS = 2`, `ARM_STRATEGIES = ["evict_oldest","clear_tool_results"]`.
- R11: the support loop `("route"…"fingerprint")` returns `SupportMismatch`; `PRIMARIES` are the three compaction primaries. `OUTCOME_BLIND_EXCLUSION_BASES` has six members.
- R11: the selection ranks candidates per quota stratum by `idp_digest("central_protocol.select", seed|stratum|task_id)` and takes the first `required`.
- R12: `PILOT_MAX_SUBJECTS = 60`, `CONFIRM_MAX_SUBJECTS = 300`, `PER_SUBJECT_CEILINGS` (`wall_minutes_per_subject` 30, `input_tokens_per_subject` 1_000_000, `verifier_minutes` 15, `concurrent_subjects` 2); "subjects=2TR>=10T".
- R2 census: agent timeouts up to 12000 s; 39 verifier timeouts above 900 s.
Retrieved: 2026-11-06T23:24Z
Implication for the spec:
- **Comparative protocol** (`comparative_protocol/1`, design proposal). It reuses `TaskSplit`, `Exclusion`, the outcome-blind basis vocabulary, the digest-ranked within-stratum selection, `ReattemptPolicy` (≤2/plan, ≤10% of plans), and the phase identities and pilot-relabel refusal. It generalizes to:
  - H ≥ 2 declared harness arms, each with an admission-record ref (F14);
  - a declared T per tranche;
  - k = R ≥ 5;
  - a schedule-randomization record (F8);
  - **declared-unsupported** route/cache/seed/fingerprint members, with the weaker claim class "stochastic hosted participant; verdict-level comparison only". This is the ADR candidate C1 open question 6 already names.
- **Comparative quote** (`comparative-quote/1`). It reuses the `COMPONENTS`, `PINS` and readiness vocabulary with `N = H·T·k`. Per-subject envelopes come from task-declared timeouts (Q-X32) and pilot-measured tokens and calls. A phase-2 envelope ADR records them.
- Both artifacts need the §4.3 admission route (independent numerical/routing review, then a gate).
Outline delta: EXTENDS OL-03 and OL-08 — new protocol/quote artifacts are owed; CONTRADICTS any reading of OL-03 that the round-5 §9.2 ceilings bound phase 2.
Limitations: I did not read `materialize_protocol`'s scheduling validation (lines 700–860) or `build_quote`'s body. The generalization's code size is not estimated here.

### F3 — Estimand: the finite-manifest equal-task resolve rate on the executed held-out manifest, from all k replicate slots, first subject or first admissible retry, joint population
Claim: For harness h, task t and replicate slots r = 1…k, the per-task rate is `x_ht = (1/k) Σ_r y_htr`. Here `y_htr` is the joint-population outcome of the slot's first subject, or of its first admissible completed retry when the first subject is an infrastructure or oracle failure (R8).

The harness rate is `θ_h = Σ_t w_t x_ht`, with `w_t = 1/T` over the tranche's executed tasks. The pairwise contrast is `Δ_gh = θ_g − θ_h`.

The target is the expected rate on **these** tasks. It is not TB 2.1 as a whole, the stratum's unexecuted tasks, or another participant. "First attempt only" is replaced by "all k pre-planned replicate slots". Harbor's `-k` attempts are HH replicates, not HH reattempts.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: R6 §3.1 (coordinates, `w_t=1/T`, first-subject primary, no best-of, retries are not repetitions); R6 §4.1 ("Target expected first-attempt outcomes on the frozen finite manifest, in each declared stratum; no population … claim"); R8 slot resolution; R2 F13 (Harbor `-k`, `-r 0`); W2 (HF: per-task mean over scored attempts, then mean over tasks).
Retrieved: 2026-11-06T23:22Z–23:30Z
Implication for the spec:
- **Primary population: `joint_task_pass_without_observed_veto`.** A confirmed or unreviewed integrity flag is an observed veto, so it scores as a failure, which matches "flagged = fail" (F13).
- **Secondary populations:** `raw`, and the leaderboard-convention all-planned lower bound `s/M` (errors = 0, B1 F13).
- **Staged estimand (Q-X31).** The population of interest is all 89 tasks. Each tranche's claims are restricted to its executed analysis stratum. A cross-tranche estimand is the equal-task mean over the union of executed held-out tasks. Because every tranche is a census of its band's held-out pool, that mean equals band weights proportional to held-out size. It needs no new estimator, provided it is pre-registered at the tranche-1 freeze and the participants and protocol are unchanged (F11).
- No extrapolation to unexecuted bands, ever.
Outline delta: EXTENDS OL-03 — "endpoints" are fixed as one primary metric with declared populations.
Limitations: Equal task weights give hard and easy tasks equal say. Saturated tasks (13 of 48 in band A per HF) contribute zero contrast and dilute Δ. That is a property of the estimand, reported through a with/without-saturated sensitivity, never used for exclusion (B1 F19).

### F4 — Strata, pools and tranche order: four timeout bands × difficulty; a 20-task development pool reserved for phase 4; tranche 1 is the 37-task held-out census of the cheapest band
Claim: Banding the 87 eligible tasks by declared agent timeout gives four bands. Proportional largest-remainder allocation of a 20-task development pool over band × difficulty cells leaves held-out pools of 37 / 17 / 11 / 2.

| band | A ≤900 s | B 1200–1800 s | C 2400–3600 s | D ≥7200 s |
|---|---|---|---|---|
| eligible tasks | 48 | 22 | 15 | 2 |
| difficulty E/M/H | 4/32/12 | 0/12/10 | 0/8/7 | 0/1/1 |
| HF saturated or ≥95% | 13 | 4 | 3 | 0 |
| verifier timeout > 900 s | 1 | 21 | 15 | 2 |
| Σ agent timeout | 11.9 h | 10.2 h | 14.3 h | 5.3 h |
| dev (E/M/H) | 11 (1/7/3) | 5 (0/3/2) | 4 (0/2/2) | 0 |
| held-out | 37 | 17 | 11 | 2 |

Status: VERIFIED (census arithmetic over B1 Appendix A); UNVERIFIED (allocation is a design proposal)
Evidence kind: source inspection + design proposal
Evidence: R2 Appendix A, parsed by `census.py` (Appendix B; output at 23:25:35Z). The 2 QEMU exclusions (B1 F11) both sit in band A medium. `alloc.py` gives the largest-remainder allocation. R6 §4.1: "Assign development and held-out pools before pilot access". R11 `TaskSplit::{Dev, HeldOut}`.
Retrieved: 2026-11-06T23:25:35Z
Implication for the spec (selection procedure, design proposal):
1. **$0 oracle pre-proof of all 87 eligible tasks before the split** (B1 REQ-XB-005). An oracle failure on the O surface becomes an outcome-blind `runner_infeasible` exclusion before any pool exists, so exclusions never perforate a drawn manifest.
2. **Pre-registered exclusions:** `qemu-startup` and `qemu-alpine-ssh` (basis `runner_infeasible`: no KVM and an EOL base, B1 F11), plus any oracle failures.
3. **Q-X30 allowlist feasibility.**
   - Five instructions require external downloads (build-cython-ext, count-dataset-tokens and hf-model-inference in band A; build-pov-ray and sam-cell-seg in band D; B1 F8).
   - Recommendation: keep them, with a **per-task declared host extension** recorded before selection. Tag them as an `allowlist_extended` covariate. Their extended hosts (GitHub, Hugging Face) widen answer-key reach, so the scan (F13) remains the backstop.
   - Excluding them under `task_surface_protection` is the alternative, and it is an operator choice.
4. **Seeded split.** Stratify by band × difficulty with a committed seed. Use the same `idp_digest` ranking as `central_protocol` under a new domain string (`comparative_protocol.split`). Allocate D = 20 by largest remainder (table). Record the split as a `SplitAssignmentRecord` before any pilot access.
5. **Pilot set** P = 10 of the 11 band-A dev tasks (E1/M7/H2, seeded). **Smoke set** = 3 of P (one per difficulty).
6. **Tranche order:** T1 = band-A held-out census (37); T2 = band B (17); T3 = bands C+D (13). Each tranche is separately quoted and gated, and claims stay within the executed bands.

The dev pool D is never touched by phase-2 confirmatory work. Phase 4 explores on D, including P, which is already exploratory. Phase-3 L2 and phase-4 confirmation draw only on held-out tasks under their own pre-registration. They must not use phase-2 per-task outcomes to choose configurations (D2/D3 own this).
Outline delta: EXTENDS OL-09 — the admitted task set is a pinned, seeded, pre-registered partition; EXTENDS OL-03.
Limitations: The band is a proxy for cost (declared timeout, not measured cost). Reserving 20 dev tasks caps T1 at 37 instead of 48 (cost and precision in F5). Difficulty labels are the dataset's own.

### F5 — Precision and cost: at k = 5 the paired contrast half-width is ≈ ±0.08 on the 37-task census and ±0.054 on 87 tasks, but power after Holm is thin for gaps below 10 points
Claim: Pairing on tasks cancels task difficulty. Contrast half-widths are much narrower than single-harness CIs (HF's ±10 points per harness at 87 × 3).

At the T1 design (T = 37, k = 5), under the `board` mixture with interaction σ = .75:
- task-mixture bootstrap half-width ±0.082;
- finite-manifest credible half-width ±0.066 (smaller, because it excludes task-sampling variance);
- sharp-null power for a true 10-point gap: 0.65 (one comparison), 0.42 (Holm over 3), 0.26 (over 7);
- for a 15-point gap: 0.93 / 0.84 / 0.71.

The cost scales as H·T·k.
Status: VERIFIED as simulation output under the stated DGP; UNVERIFIED for the real tranche
Evidence kind: recorded execution (local simulation) + design proposal
Evidence: `pair_results.json` (Simulation record; Appendix B). Rejection at Δ = 0 (m = 1) ranged 0.015–0.071 across cells, lower at T = 10 (e.g. 0.045 at T=37, k=5). The interaction model is conservative for the weak null (Limitations). Rerun flip in the board cells was 0.228–0.236.

Condensed table: board mixture, σ = .75. Spend uses the C3/C1 $/trial range. "Planning ceiling" = trials × $0.34 × 1.2 infra × 1.5 tail reserve. Wall-days assume 4 trials in flight (3 concurrent requests, C1 F11; C3 F14) at a 700 s mean. Power uses the normal approximation (≈ up to 5 points optimistic) at Bonferroni `.05/(H−1)`.

| H | T | k | trials | $ @0.05 | $ @0.15 | $ @0.34 | planning ceiling | wall-days | ±bootstrap | ±FM credible | power Δ=.10 | power Δ=.15 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 2 | 37 | 5 | 370 | 18 | 56 | 126 | 226 | 0.7 | .082 | .066 | 0.65 | 0.93 |
| 2 | 87 | 5 | 870 | 44 | 130 | 296 | 532 | 1.8 | .054 | .043 | 0.95 | 1.00 |
| 4 | 20 | 5 | 400 | 20 | 60 | 136 | 245 | 0.8 | .109 | .090 | 0.15 | 0.40 |
| 4 | 37 | 3 | 444 | 22 | 67 | 151 | 272 | 0.9 | .099 | .081 | 0.30 | 0.66 |
| **4** | **37** | **5** | **740** | **37** | **111** | **252** | **453** | **1.5** | **.082** | **.066** | **0.42** | **0.84** |
| 4 | 48 | 5 | 960 | 48 | 144 | 326 | 588 | 1.9 | .073 | .058 | 0.57 | 0.93 |
| 4 | 87 | 5 | 1740 | 87 | 261 | 592 | 1065 | 3.5 | .054 | .043 | 0.88 | 1.00 |
| 6 | 37 | 5 | 1110 | 56 | 166 | 377 | 679 | 2.2 | .082 | .066 | 0.33 | 0.77 |
| 6 | 87 | 5 | 2610 | 130 | 392 | 887 | 1597 | 5.3 | .054 | .043 | 0.83 | 1.00 |
| 8 | 20 | 5 | 800 | 40 | 120 | 272 | 490 | 1.6 | .109 | .090 | 0.06 | 0.20 |
| 8 | 37 | 3 | 888 | 44 | 133 | 302 | 543 | 1.8 | .099 | .081 | 0.17 | 0.50 |
| **8** | **37** | **5** | **1480** | **74** | **222** | **503** | **906** | **3.0** | **.082** | **.066** | **0.26** | **0.71** |
| 8 | 48 | 5 | 1920 | 96 | 288 | 653 | 1175 | 3.9 | .073 | .058 | 0.40 | 0.88 |
| 8 | 87 | 3 | 2088 | 104 | 313 | 710 | 1278 | 4.2 | .065 | .052 | 0.60 | 0.96 |
| 8 | 87 | 5 | 3480 | 174 | 522 | 1183 | 2130 | 7.0 | .054 | .043 | 0.79 | 0.99 |
| 8 | 37 | 1 | 296 | 15 | 44 | 101 | 181 | 0.6 | .157 | .107 | 0.04 | 0.14 |

Other mixtures at T = 37, k = 5: `easyband` ±.075 / ±.065, power 0.69 / 0.47 / 0.30 at Δ = .10; `hard` ±.082 / ±.066, 0.60 / 0.37 / 0.25. Without interaction (σ = 0), ±.068 / ±.067, power 0.78 / 0.58 / 0.40.

Retrieved: 2026-11-06T23:25Z–23:28:03Z (simulation runs)
Implication for the spec:
- **Declared precision target** (design proposal, not a ratified margin; it mirrors §4.3's provisional .10): every primary contrast's 95% task-bootstrap half-width ≤ 0.10 and finite-manifest credible half-width ≤ 0.08, under the pilot-informed worst-case scenario.
- T1 at k = 5 meets this for any H, because pairwise precision does not depend on H. H trades only against cost and multiplicity.
- Power is **reported, not required** (§4.5: fixed ceilings are not power). A tranche whose contrasts all straddle 0 is a valid, publishable inconclusive result.
- Raising k beyond 5 inside band A buys little: going from k=3 to k=5 moves ±.099 to ±.082. The next precision lever is more tasks, which means more bands, priced at the next gate.
- The full 60-row grid (H ∈ {2,4,6,8} × T ∈ {10,20,37,48,87} × k ∈ {1,3,5}) is regenerated by `table.py` from `pair_results.json`. The quote ticket recomputes it with exact routing.
Outline delta: EXTENDS OL-03 — "declared tranche design (N, endpoints, margins)" is now N = H·37·5 for T1 with a declared precision target; CONFIRMS thesis T4.
Limitations:
- The DGP is a logit-shift model with Gaussian interaction calibrated to HF's GLM rows on a different serving setup (C1 F16).
- Real effects, heterogeneity and the cheap band's $/trial are unmeasured.
- Normal approximations stand in for the BCa/percentile bootstrap and the exact sign flip. The exact flip is less powerful (Simulation record).
- Wall-days ignore queueing and timeouts.
- The interaction model makes the sharp-null test somewhat conservative for the weak (mean) null, which is the intended reading.

### F6 — Ranking statements: with HF-like gaps, "tied with the leader" will cover most harnesses, and the observed leader is the true best only about 60–80% of the time
Claim: I simulated the 8 harnesses with HF low-effort TB 2.1 rates (Hermes 67.8 … OpenCode 50.6; C3 F2), σ = .75. At T = 37, k = 5:
- the observed leader was the true best in 63% of datasets;
- the HF-style tied-with-leader set averaged 4.7 of 8 harnesses;
- the truly best harness fell in the tied set 99% of the time;
- the Holm many-to-one family against mini-swe-agent rejected only OpenCode (−13.4 points) with any regularity (52%);
- at 87 tasks the leader rate rose to 0.79 and the tied set shrank to 3.75;
- with 4 harnesses at T = 37, the leader rate was 0.77.
Status: VERIFIED as simulation output; UNVERIFIED for the real tranche
Evidence kind: recorded execution (local simulation)
Evidence: `rank_results.json`. Mini vs Claude Code (0.8-point gap) "tied" ≈95% at every T. W2: HF rule "a CI covering 0 marks the harness as tied", 2,000-draw task-resampling percentile bootstrap, no multiplicity correction (none mentioned).
Retrieved: 2026-11-06T23:27:40Z (simulation); W2 between 23:20:36Z and 23:23:16Z
Implication for the spec:
- "Tied with leader" is published as a **descriptive** statement labelled "leader selected from the same data; no multiplicity adjustment; covering 0 is not evidence of equality".
- It is never a confirmatory claim and never an equivalence claim (no TOST or equivalence method is admitted; R7 refuses credible-as-TOST).
- An optional pre-registered **contender set** (harnesses not Holm-rejected as worse than any other, over all H(H−1)/2 pairs) is a valid familywise statement. It can be offered at the tranche gate (Open questions).
Outline delta: EXTENDS OL-03, OL-15.
Limitations: The HF tie rule as read is a digest (W2). The simulated tie set uses a normal-approximation interval.

### F7 — Metric admission map: most HF-style metrics reuse admitted machinery; five need a small, numerically reviewed admission
Claim: The metrics divide by route as follows.

| metric | estimator | interval | route |
|---|---|---|---|
| Per-harness resolve rate θ_h | equal-task mean of per-task replicate means (F3) | (a) all-planned bounds `[s/M,(s+u)/M]` per arm (`contrast_bounds`); (b) **single-arm finite-manifest credible** `Σ_t w_t p_ht^(b)` with Beta(1,1) cells; (c) descriptive task bootstrap | (a) admitted; (b) **new**, reusing the draw, quantile and doubling code; (c) admitted `stats::bootstrap`, labelled descriptive |
| Pairwise Δ vs reference (primary) | `finite_manifest_contrast` | 95% credible, 8,000 draws plus doubling record | admitted unchanged |
| Pairwise Δ task-mixture sensitivity and sharp-null p | `task_mixture` | BCa at T ≥ 30, percentile below, ≥ 2,000 draws; sign flip exact at T ≤ 20, else 8,000 MC draws | admitted; **new exchangeability spelling** `slot_schedule_randomization` (F8) |
| Holm over the primary family; BH over exploratory | `stats::holm`, `benjamini_hochberg` | — | admitted |
| Tied with leader | leader = argmax θ̂; tied iff task-bootstrap 95% interval of Δ(L−h) covers 0 | percentile/BCa as above | **new descriptive** label/route over admitted bootstrap |
| pass^k / pass@k ("all-k / any-k") | `stats::pass_k`, `pass_at_k` per task, mean over tasks; k=5, plus the unbiased k=3 values for HF context | descriptive task bootstrap | admitted (§3.2: k ≤ n, first subjects) |
| Rerun flip f_h | mean_t 2c(k−c)/(k(k−1)) (unbiased; W2 formula) | descriptive task bootstrap | **new** |
| Harness-swap flip f_gh | mean_t [p̂_g(1−p̂_h)+p̂_h(1−p̂_g)], p̂ = c/k | descriptive task bootstrap | **new** |
| $/trial (mean, P50/P90/P95/max) | per-trial settled or labelled cost; means over scored trials; totals include infra trials (W2) | clustered task bootstrap for the mean; tails per §3.2 (max is a sample fact) | admitted bootstrap; cost labels from C2/C1 REQ-XC-007 |
| $/solve | Σ scored-trial cost / Σ solved scored trials | task-resampled ratio, percentile; `estimator_undefined` at 0 solves | **new** (ratio) |
| Infra rate, timeout rate, flag counts | counts per harness and cause | Wilson, labelled "ignores task clustering" | admitted `stats::wilson` |

Status: VERIFIED (which code exists: R7, R9, R13); UNVERIFIED (the new estimators are proposals)
Evidence kind: source inspection + design proposal
Evidence: R9 `stats.rs` symbols (`bootstrap`, `bootstrap_paired_with`, `wilson`, `pass_k`, `pass_at_k`, `permutation_signflip_p`, `holm`, `benjamini_hochberg`); R6 §3.2 (pass^k/pass@k, tails, Wilson limits); W2 (flip formulas; $/solve = scored-trial cost / solved trials; pass_all3/pass_any3 drop tasks with <3 scored attempts).
Retrieved: 2026-11-06T23:23Z
Implication for the spec: The five new pieces go through one admission ticket (REQ-XD-020…):
1. single-arm finite-manifest rate;
2. the schedule exchangeability spelling;
3. flip rates;
4. $/solve;
5. the tied-with-leader route.

That ticket carries known-answer tests:
- constant p gives f = 2p(1−p);
- the identity `f_swap,gh − ½(f_rerun,g + f_rerun,h)` equals mean_t (p_g − p_h)² in expectation;
- the ratio at 0 solves refuses;
- the single-arm credible interval equals the contrast machinery's per-arm marginal.

It also carries a comparative extension of `sim_battery` (F11). It then passes the independent numerical review gate, following the §4.3 precedent. Descriptive metrics never carry a confirmatory label.
Outline delta: EXTENDS OL-15 — method admissions are part of the phase-2 scaffolding.
Limitations: Bias of the $/solve ratio at small T is not quantified here. The admission battery must measure it.

### F8 — The primary family is many-to-one against a pre-registered reference; the sign flip needs a registered schedule-randomization argument because there is no lane coin
Claim: Phase 2 has no randomized treatment assignment, since every harness runs every task. The exchangeability that licenses a sharp-null sign flip must instead come from **randomizing the execution schedule**. Each tranche's H·T·k slots are interleaved in k rounds (round r holds replicate r). Within a round, the order over (task, harness) is a seeded rank. The scheduler runs identical, memory-packed lanes (B1 REQ-XB-010). Time-varying shocks (provider load, queueing, catalog drift, host contention) are then not aligned with harness labels.

Under the sharp null "the harness label changes no slot's outcome", the 2k slots of a task pair are exchangeable, and the sign of d_t is symmetric.

The confirmatory family is `{Δ(h − ref) : h ≠ ref}` with ref = **mini-swe-agent**. Mini-swe-agent was named by the operator, is the minimal bash baseline, is the phase-3 HIR target, and its version is unchanged since the HF pin (C3 F1). Holm α = .05 runs over these H−1 members, per tranche. The remaining pairs form an exploratory BH family.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: R9 (`REGISTERED_EXCHANGEABILITY = "lane_randomization"` is the only admitted spelling; `SHARP_NULL = "sharp_no_effect"`); R6 §4.1 (randomized order, interleaving, balance, "resets/balance alone do not prove" independence) and §4.3 (sign flips require "the registered lane randomization/exchangeability argument"); R13 `multiplicity.rs` (Holm over pre-registered primaries, BH over the rest, nothing suppressed).
Retrieved: 2026-11-06T23:23Z–23:24Z
Implication for the spec:
- Admit `slot_schedule_randomization` as a second exchangeability spelling, with its argument recorded in the comparative protocol and a test that a non-randomized schedule refuses.
- A **per-tranche** family departs from §4.3's "Holm across … confirmatory strata", which assumed one frozen study. Here later tranches may never be funded, so a program-wide family would hold unavailable members forever. Ratification must accept this explicitly.
- If a cross-tranche pooled estimand is pre-registered (F3), its contrasts form their own family.
- Choosing the reference after the pilot is forbidden. The reference is named in the pre-registration packet before any paid outcome exists.
Outline delta: EXTENDS OL-03 ("margins" → declared family, α and reference).
Limitations: Schedule randomization does not remove a harness-specific systematic effect of the environment, such as queueing that penalizes call-heavy harnesses. That is part of the harness-in-this-setup effect, and it is reported (F12).

### F9 — Pilot (≤ $60 model spend): 8 harnesses × 10 tasks × 2 attempts plus a 24-trial smoke; it measures, never ranks
Claim: C3's priors (no-cache planning values summing to $1.093 per task-attempt across the eight shortlisted harnesses, × 1.5 tail reserve) give these budgets.

| stage | design | trials | planning cost | purpose |
|---|---|---|---|---|
| S0 dialect smoke (Q-X23) | one call per dialect plus seed/`tool_choice`/`cache_control` probes (C1 open question 5) | — | ≤ $0.05 | proxy facts |
| P0 harness smoke | 3 tasks (E/M/H from P) × 1 × 8 | 24 | $4.9 | gates G3, G5, G6, G8, egress probe |
| P1 pilot | 10 tasks (P) × 2 × 8 | 160 | $32.8 | per-harness $/trial, infra, timeouts, variance |
| infra re-runs | ≤ 20% of P1 | ≤ 32 | $6.6 | |
| **total** | | **184–216** | **≈ $44.3** | under the $48 net of C1's 20% reserve |

The alternative of 6 harnesses × 15 tasks × 2 (dropping OpenHands and OpenCode) costs about $41. It adds 10 trials per harness of precision on $/trial, which the quote barely needs. But it removes two harnesses from any later tranche, because no harness enters a tranche without pilot measurements.
Status: UNVERIFIED (design proposal resting on C3 planning values)
Evidence kind: design proposal
Evidence: R4 "Ranked pilot shortlist" and "What $60 covers"; R3 F8–F11 (credit period to 2026-11-22T16:30:34Z, 3 concurrent requests, no provider cap); W2 (HF reserve = max(mean, P90) over ≥ 9 metered trials; nested smoke ⊂ pilot ⊂ full).
Retrieved: 2026-11-06T23:21Z–23:23Z
Implication for the spec: **Recommend 8 × 10 × 2 (Q-X28)**, run in this order:
1. the $0 oracle pre-proof and image pinning;
2. S0;
3. P0;
4. P1 for only those harnesses whose P0 gates pass.

A P0 failure is a recorded repair, which creates a new participant version and a P0 re-run, or a typed `withheld{admission:G<n>}`.

**Stop rules** are budget, infra and integrity rules only; no outcome-based stopping:
- **Budget.**
  - The HH proxy enforces a hard total cap equal to the authorized amount × 0.8, by pre-call reservation (C1 REQ-XC-003).
  - Per-harness caps are 41.4 × that harness's planning $/trial (≈ 1.5 tail × 23 trials × 1.2 infra): mini $6.4, Terminus-2 $7.5, Claude Code $3.6, Pi $2.1, Hermes $4.2, OpenHands $10.4, Kimi $5.3, OpenCode $5.8. They sum to about $45.3.
  - A harness at its cap stops. Its unrun slots become `withheld{budget}`, and its cost estimate is reported as a censored lower bound.
- **Infra.**
  - P0: ≥ 1 of 3 trials infra-failed holds that harness for repair.
  - P1: an infra rate > 20% after ≥ 10 trials stops that harness.
  - ≥ 3 harnesses stopped for infra stops the pilot (systemic).
- **Integrity.** Any of the following stops everything at once, keeping partial rows and costs:
  - a model call without a proxy receipt (C3 G6);
  - egress outside the declared allowlist;
  - credential exposure;
  - an O/H image-digest mismatch;
  - an Ollama catalog-identity change (C1 REQ-XC-009);
  - a confirmed answer-key receipt in more than one trial.
- **Calendar.** Finish before 2026-11-22T16:30:34Z unless the operator rules otherwise (Q-X21).

**Pilot outputs to the quote**, per harness at the pinned effort:
- n;
- $/trial mean, P50, P90, max;
- cached share;
- tokens and calls per trial;
- agent wall time and model-occupied time;
- **queue wait** per trial;
- infra rate (Wilson) by cause;
- agent-timeout rate;
- egress denials;
- proxy rewrites applied;
- receipt completeness;
- scan flags.

And globally:
- attained concurrency;
- wall hours;
- instance hours;
- per-task pass counts. These are used **only** as nuisance inputs: mean rate, rerun flip, heterogeneity.
Outline delta: EXTENDS OL-03 and OL-08 (pilot budget envelope, per-harness ceilings, spend telemetry); CONFIRMS A-3/Q-X11 ("$60 is a pilot only").
Limitations: The planning values for Terminus-2 and OpenHands are C3 inferences. Cheap-band $/trial is unmeasured and probably lower than HF's all-task mean.

### F10 — Pilot data are exploratory: they may inform nuisance parameters and costs, never harness selection, the reference or the effect sizes the quote assumes
Claim: Admitting harnesses to a tranche by their pilot resolve rate would condition the confirmatory family on noisy outcomes. With 20 trials per harness and pairwise half-widths of ±0.2 or more (F5, T = 10, k = 2), that is pure winner's-curse selection. Admission to a tranche rests only on gates G1–G9 (F14), measured cost and infra.

The quote's precision simulation may use the pilot's mean rate, rerun flip and task heterogeneity. It must widen them to the worst of {pilot estimate ± 2 SE, the `board`/`easyband`/`hard` priors}. Effect-size scenarios stay the fixed grid {0, ±.05, ±.10, ±.15}, never a pilot-observed gap.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: R6 §4.1 ("Pilot may estimate feasibility, activation, failure, variance, tails and cost; it cannot … select favorable tasks or become confirmation") and §4.3 ("Propagate pilot uncertainty"); R11 `PilotRelabel` refusal; F5 pilot-cell widths.
Retrieved: 2026-11-06T23:22Z
Implication for the spec:
- The pilot packet carries per-harness resolve counts under an `exploratory/pilot` label. The quote cannot read them except through the declared nuisance fields.
- The results site may show pilot rows only on a clearly separated "pilot (exploratory)" panel, if at all (F2 decides).
- The comparative protocol refuses a pilot identity reused as confirmation.
Outline delta: EXTENDS OL-15.
Limitations: Operator judgement at the gate can still be influenced by seeing pilot rates. Recording that the harness set was chosen from a menu built only from gate and cost data mitigates this but does not prevent it.

### F11 — Quote → gate → tranche: a deterministic menu from pilot records, a declared selection rule, a pre-registration freeze, and a comparative simulation battery before the first confirmatory subject
Claim: The tranche N follows mechanically from the pilot packet and the declared precision target:
- T = the band's held-out census;
- k = 5;
- H = the operator's choice from a menu of harness subsets that pass gates, each priced.

**Per menu row, the quote packet carries:**
- trials H·T·k;
- expected model spend Σ_h (pilot mean $/trial_h) · T·k · (1 + measured infra allowance);
- planning ceiling Σ_h max(mean, P90)_h · T·k · 1.2;
- wall hours from measured model-occupied time and queue wait at the attained concurrency;
- instance hours (B3 owns the rate);
- simulated half-widths and power (exact routing, the comparative battery);
- P(≥ 1 unresolved cell) from the measured residual infra rate (F12);
- the default precision verdict against the target (F5).

**Then the operator gate.** It records verbatim:
- the spend authorization, which a provider reset does not extend (C1 REQ-XC-004);
- the harness set;
- the effort pin;
- the version pins;
- the tranche's protocol hash, which must predate every dependent subject (R6 AC-3-2).
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: R12 (quote readiness vocabulary: `ready_for_gate`/`blocked`/`infeasible`, "Dispatch is never authorized: the best outcome is `gate_pending`"); R10 (frozen battery: ≥ 2,000 datasets per scenario, seed rule `H(canonical(scenario ∥ battery_version ∥ replicate))`, retained failures); R6 §4.3 ("simulate the exact pipeline … before dependent confirmation"); R3 F9 and REQ-XC-015 (pilot measurements are the only cost inputs).
Retrieved: 2026-11-06T23:23Z–23:24Z
Implication for the spec:

**Pre-registration packet** (frozen, hashed, committed before any tranche subject):
- the TB 2.1 three-part pin (B1 REQ-XB-001);
- the split record and seed;
- the tranche manifest;
- exclusions with bases;
- the allowlist and per-task extensions;
- harness admission-record refs (F14);
- the reference harness;
- the effort pin;
- k = 5;
- the schedule seed and rounds;
- the population (`joint`) and secondary populations;
- the estimator kinds and versions, priors, draws and seeds;
- the family (members, α = .05, Holm) and the exploratory BH set;
- the precision target;
- the stop rules;
- the infra and reattempt policy;
- the scan detector version and review rule;
- the declared timeouts and multipliers (Q-X32);
- the reporting labels (F16);
- any pooled cross-tranche estimand.

**Comparative battery** (`sim-battery` extension, frozen before execution):
- H ∈ {2, 4, 8} arms;
- T ∈ {20, 37, 48};
- k = 5;
- mixtures `board`/`easyband`/`hard`;
- σ ∈ {0, .75};
- many-to-one Holm;
- informative infra missingness;
- flagged-pass vetoes;
- shared-shock rounds.

**Tranches 2 and 3** each repeat quote → gate → freeze → execute → results, with their own family. A pooled estimand across tranches is admissible only if declared at the T1 freeze and if versions, effort, proxy rewrite version and the catalog identity are unchanged. Otherwise tranches are separate cohorts and are never relabelled.
Outline delta: EXTENDS OL-03, OL-08, OL-15 — the funding model is a recorded per-tranche authorization; the tranche design is a frozen packet.
Limitations: The precise packet schema belongs to the implementation ticket. Monte Carlo cost of the battery has not been estimated; the pure-Python grid here ran in under 40 s total.

### F12 — Missingness: infra failures re-run within the frozen bound; residual unresolved cells refuse the pair's inference; timeouts are task failures; queue wait is recorded because it can masquerade as a harness effect
Claim: Harbor and proxy failure causes map onto HH classes as follows (B1 F13; C1 F11; R6 §3.1).

| cause | HH class | handling |
|---|---|---|
| sandbox / OOM, `NonZeroAgentExitCode`, provider 429/5xx/queue-full/broken stream, missing proxy receipt | `infrastructure_failure` | re-run as a new attempt on the same replicate slot; ≤ 2 reattempts per plan, ≤ 10% of plans; first admissible completed retry counts |
| agent timeout, context exhaustion | `budget_exhausted` | verifier still runs; its grade counts (normally 0); in the denominator |
| verifier timeout, missing/empty reward, verify-time bootstrap failure | `oracle_failure{verifier_unavailable[:bootstrap]}` | regrade per §2.2 when the sealed post-agent state is retained; otherwise unresolved |

**Consequences.**
- A slot still unresolved after the bound keeps `[s/M, (s+u)/M]` bounds. The primary contrast for every pair containing that task refuses (`UnresolvedCell`), and the refusal is the reported result.
- An eligible-only conditional analysis, dropping the task from both arms, may be reported as a labelled secondary. It is never the headline.
- **Queue wait counts against the agent timeout.** With 3 concurrent requests, call-heavy harnesses (31.8 calls/trial for mini-swe-agent, 47.8 for OpenClaw; C3 F2) wait longer and time out more. Per-trial queue wait is recorded. If the pilot's median queue wait exceeds 10% of mean agent time, the in-flight trial count is lowered and requoted (design proposal). Timeout rates are reported with queue wait beside them.
Status: VERIFIED (code semantics, R8/R11); UNVERIFIED (handling rules are design proposals)
Evidence kind: source inspection + design proposal
Evidence: R8 slot resolution (infra/oracle first subject reads first admissible retry; unresolved otherwise); R7 `UnresolvedCell`; R11 `MAX_REATTEMPTS_PER_PLAN = 2`, `MAX_REATTEMPT_FRACTION_PPM = 100_000`, `RETRYABLE_CLASSES = ["infrastructure_failure"]`; R6 §3.1–3.2; W2 ("an agent timeout never counts as this", infra re-run "up to 2 more rounds", excluded and shown as error rate); A1 F8 (the S4 fold exceeded the frozen bound, the lesson this design must not repeat).
Retrieved: 2026-11-06T23:23Z–23:30Z
Implication for the spec:
- Infra failures are never silently dropped. Each one keeps its row, cause, cost and attempt number.
- The completion envelope (planned, first, recovered, unresolved slots per harness and task) is published with every estimate.
- Harbor runs with `-r 0` (B1 REQ-XB-011).
- The reattempt bound does not change for phase 2. A separately funded allowance, which §3.1 contemplates, needs an operator decision at the tranche gate (Open questions).
Outline delta: EXTENDS OL-03 (missingness rule), OL-15.
Limitations: The verifier-bootstrap regrade depends on B2's cell-retention design (not read). The 10% queue threshold is a placeholder for the pilot to calibrate.

### F13 — Integrity: allowlisted egress plus a mandatory scan of what the model received; every flagged pass is human-reviewed; flagged = fail unless cleared
Claim: Under Q-X30 the agent phase reaches only the HH model proxy and declared package registries, plus per-task extensions (F4). Even so, a scan stays mandatory, because:
- registries and extended hosts can carry answers;
- harness tools can tamper with the environment;
- HF found answer-key retrieval in ≥ 134 trials despite their scan being harness-independent.

**Design proposal.**
- **Detector** `answer-scan/1`, run over the HH proxy's verbatim receipts (model-received content and outputs, C3 F10) plus the per-slot egress log. Signals:
  1. normalized-line and shingle fingerprints of each task's `solution/*`, `tests/*` and task `README.md`;
  2. the canary GUID `26b5c67b-…` (B1 F4);
  3. requested or denied hosts matching TB/Harbor/HF-Space repositories;
  4. tamper heuristics: writes to `/logs/verifier`, `/tests`, reward files, pytest/conftest/PATH shims, verifier binaries (Harbor reward-hacking classes, B1 F16).
- **Scoring.**
  - A *received* match or a tamper hit on a passing trial is an observed veto. The trial scores 0 in the primary `joint` population unless a recorded review clears it with a reason.
  - A *requested-but-denied* fetch is recorded and reported, not scored.
  - Unreviewed flags at freeze stay vetoes. That is conservative, and it avoids an `unknown` that would refuse the cell.
- **Review.** A HUMAN row reads every flagged pass. Harness-identifying strings are masked where feasible. The reviewer records `{confirmed, cleared}`, a reason and the excerpt hash.
- **Reporting** per harness: flagged, confirmed and cleared counts, detector version, coverage, and "the confirmed count is a minimum".
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: R2 F4, F16, F18 and REQ-XB-016; R4 F9–F10; W1 ("Cheating counts as a fail … every flagged pass is read before it counts"; "the confirmed count is a minimum"); W2 (received matches fail; requested is "for reading at closeout, not scored"; a reader confirms or clears); R8 (`missing_required_checks` → unresolved; vetoed pass → joint failure).
Retrieved: 2026-11-06T23:21Z–23:30Z
Implication for the spec:
- The scan is a **required check** on every scored trial, so a missing scan is unresolved, never clean.
- Mediation completeness (C3 G6) guarantees the scan's input exists. A trial without receipts is infra, never scored.
- The deterministic acceptance is a seeded-leak fixture battery: a planted solution excerpt, the canary, a denied repo fetch, and a reward-file write. Each must be detected, and a clean control must not be flagged.
- Results are labelled "allowlisted network posture; not leaderboard-comparable" (B1 REQ-XB-015).
Outline delta: EXTENDS OL-15 (integrity scaffolding), OL-09.
Limitations: Fingerprint detection misses paraphrased or re-derived answers. Blinding the reviewer to harness identity may be impossible when trajectories reveal the harness. The detector's false-positive rate is unknown until the pilot.

### F14 — Harness admission record and version-pin policy: C3's nine gates plus seven attributes, with three D1 additions; latest stable at the pre-registration freeze, exact lock, mini-swe-agent fixed at 2.4.6
Claim: Each harness's record carries C3's gates G1–G9 with evidence and attributes A1–A7, plus three D1 fields:
1. outcome-blindness: admission is decided before any paid outcome and never from pilot rates (F10);
2. effort-pin mechanism and enforcement evidence (F15);
3. its role (reference or comparator) and its configuration-limit set (Harbor defaults or declared limits, pending Q-X29).

Claude Code's record also carries the Q-X26 terms verbatim:
- unmodified pinned binary;
- no Claude account;
- black-box arm only;
- name without logo;
- disclosure that Anthropic does not support non-Claude models.

**Version policy recommendation (Q-X27).** Pin each harness to its latest stable release (no alpha, nightly or `@latest`) as of the pre-registration freeze. Use an exact lock or resolution cutoff at the freeze time (npm lock or tarball hash; uv `--exclude-newer`), with the image recorded by digest. Exceptions:
- mini-swe-agent stays at 2.4.6 (E1 and the HF pin; latest stable as of C3);
- Terminus-2 follows the pinned stable Harbor tag;
- LiteLLM is pinned explicitly (HF's 1.103.5 lesson).

A version change after the freeze is a new participant and therefore a new cohort.
Status: UNVERIFIED (design proposal resting on VERIFIED C3/B1 facts)
Evidence kind: design proposal
Evidence: R4 admission-criteria table (G1–G9, A1–A7), F1 (HF pins superseded within days), F6 (Claude Code terms), F8 (unpinned install templates); R2 F15 and REQ-XB-014; ledger Q-X12 (no external replication anchor), Q-X26 answer.
Retrieved: 2026-11-06T23:21Z–23:22Z
Implication for the spec: Q-X12 removes any inferential reason to match HF's older pins, which serve only as weak priors. Current stable versions avoid the "stale harness" objection, and one freeze-time pin across all tranches keeps the cohort identity stable. The record validator refuses a missing gate, an unpinned version or a missing lock.
Outline delta: EXTENDS OL-09 (admission criteria made a record), OL-15.
Limitations: "Latest stable" must be determined from each project's release channel at freeze time; C3 found Codex's newest tag is an alpha. Newer versions have no cost prior, so the pilot is their only cost evidence.

### F15 — Reasoning effort: pin `high` for every harness in phase 2; effort as a factor belongs to phase 4
Claim: Effort must be one fixed value, applied by the proxy to every upstream call, or the comparison measures effort rather than harness (C1 F5; W2). Of `low`, `high` and `max`, **`high`** is recommended (Q-X22):
- **Cost.** `max`, the model default, uses about 1.9× the reasoning tokens of `high`. Precision is the binding constraint (F5), and a fixed budget at `max` buys roughly a quarter fewer trials (C1's cost table allows ×1.3 per trial for `max` over `high`; inference).
- **Failure risk.** Long reasoning raises stream-idle-timeout and output-cap failures, which would show up as infra or agent timeouts (C1 F11d; C3 F13).
- **Context.** HF ran `low`/`high`, so `high` keeps HF figures usable as labelled context.
- **Capability.** `low` is cheaper but under-states some harnesses (HF Pi 62.5 at low vs 69.4 at high).

Effort sweeps are the phase-4 OFAT knob (C1 F5 confirms OL-07). The decision must precede the pilot, because pilot costs do not transfer across effort levels.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal + source inspection
Evidence: R3 F5, F9, F11 and REQ-XC-002; R4 F2 (high-effort rates), F13; W2 ("Reasoning effort is a run parameter and a leaderboard dimension"; the meter sets effort on every call and logs the requested value).
Retrieved: 2026-11-06T23:21Z–23:23Z
Implication for the spec: The pre-registration declares `effort = high`, and every receipt records the requested and applied effort. The site states "effort pinned to high for all harnesses; harness defaults would differ".
Outline delta: EXTENDS OL-03 (effort is a declared tranche parameter).
Limitations: The 1.9× ratio comes from the zai-org route on a short prompt. Ollama's behaviour is unmeasured, and the pilot measures it only at `high`.

### F16 — Reporting contract for the results site: exact derived quantities, intervals and mandatory labels; banned and conditional claim language
Claim: The site (F2) renders only quantities recomputed from committed result packets, each with the interval and label in the table below. Labels are mandatory. Language is checked against `check-position.sh`'s `BANNED` list plus program additions.

| quantity | interval / companion | label |
|---|---|---|
| θ_h (joint) | FM credible 95%, all-planned bounds, completion envelope | "finite-manifest: these N TB 2.1 held-out tasks (band …), not the 89-task benchmark" |
| Δ(h−ref) | FM credible 95%; task bootstrap 95% (sensitivity); sign-flip raw p and Holm-adjusted p with family id | "confirmatory family T<n>; α=.05 Holm" |
| other pairs; tied-with-leader | task bootstrap 95%; BH q | "exploratory; leader selected from the same data" |
| raw rate; errors=0 lower bound | — | "secondary population" |
| pass^5/pass@5 (pass^3/@3 for context) | task bootstrap | descriptive |
| rerun and swap flips | task bootstrap | descriptive |
| $/trial (mean, P50/P90/max), $/solve, tokens, calls, time | bootstrap or sample facts; cost provenance label (settled / estimated_from_pricing / estimate-upper-bound) | descriptive; "model spend only" vs "with compute" |
| infra, timeout and flag counts | Wilson (clustering caveat) | integrity panel |
| per-task grid | c/k per (task, harness); receipt and trajectory refs | — |

Every page carries:
- "internal record, not outside-verified";
- "stochastic hosted participant glm-5.3-flash via Ollama; catalog identity reported, not fingerprinted" (C1 F4);
- "allowlisted agent network; fewer than 89 tasks; not leaderboard-comparable";
- "HF board figures are context from a different serving setup" (C1 REQ-XC-014).

Status: VERIFIED (the banned list); UNVERIFIED (the contract is a design proposal)
Evidence kind: source inspection + design proposal
Evidence: R14 `BANNED = ["proven", "guarantee", "guaranteed", "production-ready", "production ready", "state-of-the-art", "outperform", "significant improvement", "statistically significant", "users demand", "users love", "widely adopted", "market-leading", "best-in-class", "superior"]`. It is applied to `docs/evidence.md` and the README spine only (lines 322–328). A1 F12 and F14 (label hygiene; no `live-executed` without `hh-live-evidence/1`).
Retrieved: 2026-11-06T23:23Z
Implication for the spec: F2's site claims check reuses the `BANNED` list and adds program phrases:
- banned: "best harness", "winner", "beats", "no difference", "equivalent", "same performance";
- conditional: "significant" only in the exact form "Holm-adjusted p < .05 (family T<n>)"; "tied" only with "95% interval covers 0".

The check runs over the site sources and the results packets' prose. Evidence labels follow ADR-0363: a tranche is `live-executed` only with cited `hh-live-evidence/1` records.
Outline delta: EXTENDS OL-15 (publication gate inputs); feeds OL-18 via F2.
Limitations: F2 owns the site check's mechanics. The extra phrase list is a proposal for F2/H1.

### F17 — Ticket shaping: seven tickets and four operator gates per program, plus one quote-gate-execute-results cycle per further tranche
Claim: The method decomposes into fresh-context `implement-spec` units. Most have deterministic acceptance. Live work sits behind explicit gates.

| # | ticket (kind) | deterministic acceptance | gate after |
|---|---|---|---|
| D1-a | **Analysis extensions I**: `comparative_protocol/1` (n-arm, split, schedule randomization, declared-unsupported support members), `comparative-quote/1` (N = H·T·k, phase-2 envelope) | refusal matrix (third-arm-style misuse, overlap, late split, pilot relabel, unrandomized schedule, unpinned harness); reproducible split and schedule from seed | — |
| D1-b | **Analysis extensions II**: single-arm FM rate, flips, $/solve, tied-with-leader route, `slot_schedule_randomization` spelling; comparative sim battery | known-answer tests (F7); battery executed with retained per-scenario records; recomputation equality | **GATE: independent numerical review + method admission** (operator) |
| D1-c | **Integrity scanner** `answer-scan/1` and the review-record schema | seeded-leak fixture battery; clean controls unflagged | — |
| D1-d | **Selection and pre-registration packet I** (after the $0 oracle pre-proof): exclusions, split, pilot/smoke sets, admission records, effort/version pins | packet hash stable; split reproducible; validator rejects missing gates | **GATE: pilot spend** (Q-X21; verbatim authorization) |
| D1-e | **Pilot execution** (live): S0 → P0 → P1 with stop rules | pilot packet schema-valid; every slot dispositioned; reconciliation row present | — |
| D1-f | **Quote packet** (offline from pilot records) | menu recomputes byte-identically; precision cells from the admitted battery | **GATE: tranche-1 authorization**, harness set, pre-registration freeze (protocol hash) |
| D1-g | **Tranche execution** (live) then **results packet** (offline recompute, labels, claims check) | every estimate recomputes from records; completion envelope; claims check passes | **HUMAN: flagged-pass review**; **GATE: publication** (F2/H1) |

Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: brief §4 and A-9/A-10; R6 §4.3 ("implemented only after independent numerical/routing review and formal admission"); AGENTS.md (GATE semantics, `mergePolicy: OPERATOR`, evidence labels).
Retrieved: 2026-11-06T23:20:36Z–23:24Z
Implication for the spec: H1 orders these after B2 (runner), C2 (proxy), C4 (hosting) and B3 (compute). D1-g may need splitting into execution and results for size. Tranches 2–3 repeat D1-f and D1-g.
Outline delta: EXTENDS OL-14 (decomposition input), OL-15 (gates).
Limitations: Ticket sizes are not estimated against implement-spec budgets. H1 sizes them.

## Answer and completion assessment

**Settled** by source inspection:
- The admitted estimators cover phase 2's primary pairwise contrasts as they stand, given k ≥ 5, one analysis stratum per tranche and no unresolved cells (F1).
- `central_protocol` and `exposure_quote` cannot be reused unchanged (F2).

**Designed and recommended**, as proposals with proposed validation:
- the finite-manifest, joint-population estimand, staged by timeout band (F3);
- the census, the dev/held-out partition and the tranche order (F4);
- k = 5, the precision target, and the honest power picture (F5, F6);
- the metric admission map (F7);
- the many-to-one family with a schedule-randomization argument (F8);
- the pilot of 8 × 10 × 2 plus smoke, with its stop rules (F9);
- pilot-data firewalls (F10);
- the quote → gate → freeze sequence (F11);
- missingness and timeout handling (F12);
- the integrity scan (F13);
- the admission record and version policy (F14);
- effort pinned at `high` (F15);
- the reporting contract (F16);
- ticket shaping (F17).

**Technical blockers** (not uncertainty) before any confirmatory tranche:
1. The comparative protocol and quote must be implemented and admitted, including the ADR for declared-unsupported support members and the phase-2 envelope.
2. The new descriptive estimators and the exchangeability spelling must pass numerical review.
3. Phase 2 depends on the runner, proxy and hosting rows.

**Acceptable uncertainty for this row:**
- real effect sizes and heterogeneity;
- cheap-band $/trial;
- infra and queueing rates;
- scan false-positive rates.

The pilot and the quote's battery resolve these by design, and an inconclusive tranche remains a valid result.

## Open questions

- **Q-X22 (effort; operator, at ratification).** Recommendation: pin `high` for all phase-2 harnesses (F15). Affects C2, D1, D3.
- **Q-X27 (version pins; operator, at ratification).** Recommendation: latest stable at the pre-registration freeze, exact lock, with mini-swe-agent fixed at 2.4.6 and LiteLLM pinned (F14). Affects C4, D2.
- **Q-X28 (pilot breadth; operator, at the pilot gate).** Recommendation: 8 × 10 × 2 plus a 24-trial smoke, about $44 at planning values (F9).
- **Q-D1-1 (reference harness; operator, at ratification).** Should mini-swe-agent be the pre-registered reference of the many-to-one family (F8)? The alternative is an all-pairs family with lower power. Affects H1, F2.
- **Q-D1-2 (dev-pool reservation; operator, at ratification).** Reserve 20 dev tasks now (T1 = 37), or a smaller pool (T1 up to 48, F5)? Affects D3 and D2.
- **Q-D1-3 (per-tranche Holm family; ratification).** Accept one family per tranche as an explicit departure from §4.3's cross-strata family (F8)? Affects H1 and O2.
- **Q-D1-4 (residual unresolved cells; operator, at the tranche gate).** Keep the frozen reattempt bound, so residual unresolved cells make the affected pairs refuse, or fund a separately recorded extra allowance (F12)? Affects B2 and H1.
- **Q-D1-5 (allowlist-extended tasks; operator, before selection).** Keep the five instruction-required-download tasks with per-task host extensions plus scan, or exclude them under `task_surface_protection` (F4)? Affects B2, C2, C4.
- **Q-D1-6 (flagged-pass reviewer; operator).** Who performs the HUMAN review, and is harness masking required (F13)? Affects F2 and H1.
- **Q-D1-7 (contender set; operator, at the tranche gate, optional).** Pre-register an all-pairs contender set as a confirmatory ranking statement in addition to the descriptive tie (F6)?

Pending from earlier rows and affecting this design:
- Q-X21 (credit mechanics): gates the pilot.
- Q-X23 (cents-scale smoke): S0 in F9.
- Q-X29 (configuration limits / LiteLLM pin): F14.

## Spec requirements emitted

| id | requirement (candidate) | rationale / findings | OL | acceptance |
|---|---|---|---|---|
| REQ-XD-001 | Phase-2 primary estimand: finite-manifest equal-task mean of per-task replicate means on a tranche's executed held-out manifest, in the `joint_task_pass_without_observed_veto` population, using each replicate slot's first subject or first admissible completed retry. | F3 | OL-02, OL-03 | Analysis test over a fixture population reproduces θ_h and Δ by hand |
| REQ-XD-002 | Confirmatory designs use k = 5 replicate slots per (task, harness). The R ≥ 5 cell floor is not amended for phase 2. | F1, F5 | OL-03 | Protocol validator refuses k < 5 for a confirmatory identity |
| REQ-XD-003 | Each tranche's executed tasks carry one analysis-stratum label. Claims are restricted to the executed bands and never extrapolated to unexecuted tasks or bands. | F3, F4 | OL-03, OL-15 | Report labels; claims check rejects catalogue-wide wording |
| REQ-XD-004 | A cross-tranche pooled estimand is admissible only if declared at the T1 freeze and only if harness versions, effort, proxy rewrite version and Ollama catalog identity are unchanged. Otherwise tranches are separate cohorts. | F3, F11 | OL-03 | Fixture with a changed version refuses pooling |
| REQ-XD-005 | Secondary populations `raw` and the all-planned lower bound (errors = 0) are reported beside the primary. | F3 | OL-03 | Golden results packet |
| REQ-XD-010 | Population = 89 TB 2.1 tasks. Pre-registered outcome-blind exclusions: qemu-startup and qemu-alpine-ssh (`runner_infeasible`) and any task failing the $0 O-surface oracle pre-proof, which runs on all eligible tasks before the split. | F4 | OL-09 | Exclusion record precedes the split record (timestamps, hashes) |
| REQ-XD-011 | A seeded split stratified by agent-timeout band × difficulty reserves a development pool (proposed 20, largest-remainder) before any pilot access. Phase-2 confirmatory tranches draw only on the held-out pool. | F4 | OL-09 | Split reproducible from seed; overlap refusal test |
| REQ-XD-012 | Pilot set = 10 band-A dev tasks; smoke set = 3 of them (one per difficulty), seeded. | F4, F9 | OL-03 | Selection reproducible from seed |
| REQ-XD-013 | Tranche order is band A, B, then C+D. Each tranche is a census of its band's held-out pool and is separately quoted and gated. | F4, F11 | OL-03 | Manifest equals the held-out band set |
| REQ-XD-014 | Tasks needing instruction-required downloads under the Q-X30 allowlist carry declared per-task host extensions (or a recorded exclusion) before selection, and an `allowlist_extended` covariate. | F4, F13 | OL-09, OL-15 | Pre-registration lists extensions; egress logs match |
| REQ-XD-020 | A comparative protocol `comparative_protocol/1` generalizes `central_protocol` to H ≥ 2 admitted harness arms, a declared T and k ≥ 5. It reuses split, exclusion, digest-ranked selection, reattempt and phase-identity machinery, and admits declared-unsupported route/cache/seed/fingerprint members only under the weaker claim class (ADR). | F2 | OL-03, OL-15 | Refusal matrix; byte-stable materialization; ADR cited |
| REQ-XD-021 | Execution order is a seeded randomized interleaving of (task, harness) slots in k rounds over identical lanes, recorded before execution. `slot_schedule_randomization` is admitted as the exchangeability argument for sharp-null sign flips. Any other order refuses that argument. | F8 | OL-03 | Test: unrandomized schedule → typed refusal |
| REQ-XD-022 | A single-arm finite-manifest rate estimator (Beta(1,1) cells, 8,000 draws, doubling record) reports θ_h's credible interval. Rerun flip, harness-swap flip, $/solve and tied-with-leader are admitted as descriptive estimators. | F7 | OL-15 | Known-answer tests (F7) |
| REQ-XD-023 | New method pieces are implemented only after an independent numerical/routing review and an operator method-admission gate, following §4.3. | F7, F17 | OL-15 | Review record and gate entry precede first use |
| REQ-XD-024 | A comparative extension of `sim_battery` (H ∈ {2,4,8}, T ∈ {20,37,48}, k = 5, three mixtures, interaction, infra missingness, vetoes, shared shocks; ≥ 2,000 datasets per scenario; frozen seed rule) runs on the exact pipeline before the first confirmatory subject. | F5, F11 | OL-03 | Battery records retained; stopped scenarios never count as admission |
| REQ-XD-030 | The primary family per tranche is Δ(h − ref) for every admitted h ≠ ref, with a pre-registered reference (proposed mini-swe-agent), metric `capability.task_success`, joint population, sharp-null sign-flip p and Holm α = .05. Other pairs form an exploratory BH family. Unavailable members stay in the family. | F8 | OL-03 | Multiplicity test with an unavailable member |
| REQ-XD-031 | "Tied with leader" is published only as a descriptive, exploratory statement with its selection caveat. Neither credible nor bootstrap intervals are presented as equivalence evidence. | F6, F16 | OL-15 | Claims check fixture |
| REQ-XD-040 | The pilot spends ≤ the operator-authorized amount × 0.8 through HH proxy reservation. Per-harness caps are declared in advance. A capped harness's unrun slots are `withheld{budget}`, and its cost is reported as a censored lower bound. | F9 | OL-03, OL-08 | Simulated over-cap run stops without overshoot |
| REQ-XD-041 | Pilot stages run in order: $0 oracle pre-proof, S0 dialect smoke, P0 (3 × 1 × H), then P1 (10 × 2 × H) only for harnesses passing P0 gates. A P0 failure is a recorded repair (new participant version) or `withheld{admission}`. | F9 | OL-03, OL-09 | Stage records with dispositions |
| REQ-XD-042 | Pilot stop rules are budget, infra and integrity only (F9 thresholds). No outcome-based stopping. Partial rows and costs are retained. | F9 | OL-03, OL-15 | Stop-rule fixtures per trigger |
| REQ-XD-043 | Pilot outputs per harness include $/trial mean/P50/P90/max, cached share, tokens, calls, agent wall time, model-occupied time, queue wait, infra rate by cause, timeout rate, egress denials, proxy rewrites, receipt completeness and scan flags. | F9, F12 | OL-08 | Pilot packet schema test |
| REQ-XD-044 | Pilot outcomes are exploratory. They may feed only declared nuisance fields of the quote (widened to the worst of pilot ± 2 SE and the prior mixtures). They never select harnesses, the reference, the tasks or the assumed effect sizes, and never become confirmatory. | F10 | OL-15 | Quote reads no pilot outcome field outside the nuisance set; relabel refusal |
| REQ-XD-050 | The comparative quote prices a menu of gate-passing harness subsets at N = H·T·k with expected spend, planning ceiling, wall/instance hours, simulated half-widths and power, P(≥ 1 unresolved cell), and a verdict against the declared precision target (bootstrap ≤ .10, FM ≤ .08). | F5, F11 | OL-03, OL-08 | Deterministic recomputation from pilot records |
| REQ-XD-051 | Phase-2 per-subject and tranche envelopes are derived from task-declared timeouts and pilot measurements and recorded by ADR. The round-5 §9.2 ceilings are not silently widened. | F2 | OL-08, OL-16 | ADR present; quote cites it |
| REQ-XD-052 | Each tranche starts only after a gate recording verbatim spend authorization, harness set, effort and version pins, and the pre-registration packet hash. The packet lists every F11 member. | F11 | OL-03, OL-08, OL-15 | Packet hash predates every subject's record |
| REQ-XD-060 | The failure-cause mapping is as in F12. Infra re-runs are new attempts within ≤ 2 per plan and ≤ 10% of plans. Agent timeouts are `budget_exhausted` and keep the verifier's grade. Verifier failures are `oracle_failure` (regrade only from retained sealed state). | F12 | OL-03, OL-15 | Mapping table test over every cause |
| REQ-XD-061 | A residual unresolved cell makes the primary contrasts that involve its task refuse. All-planned bounds and the completion envelope are published. An eligible-only analysis appears only as a labelled conditional secondary. | F12 | OL-15 | Fixture with one unresolved slot → refusal plus bounds |
| REQ-XD-062 | Per-trial proxy queue wait is recorded and reported beside timeout rates. The pilot calibrates an in-flight limit (provisional: median queue wait ≤ 10% of mean agent time). | F12 | OL-03 | Receipt field; quote uses it |
| REQ-XD-070 | Every scored trial passes the required `answer-scan/1` check over proxy receipts and egress logs (fingerprints, canary, repository hosts, tamper heuristics). A received match or tamper hit on a pass is an observed veto unless cleared by a recorded human review. Requested-only fetches are reported, not scored. | F13 | OL-15 | Seeded-leak fixture battery; clean controls |
| REQ-XD-071 | A HUMAN row reviews every flagged pass before results freeze, recording disposition, reason and excerpt hash. Reports give flagged/confirmed/cleared counts and "the confirmed count is a minimum". | F13 | OL-15 | Review records complete for all flags |
| REQ-XD-080 | Each harness admission record carries C3 G1–G9 with evidence, attributes A1–A7, outcome-blind admission timing, the effort-pin mechanism, the reference/comparator role and the configuration-limit set. Claude Code's record carries the Q-X26 terms verbatim. | F14 | OL-09, OL-15 | Validator rejects a missing gate or term |
| REQ-XD-081 | Harness versions are the latest stable release at the pre-registration freeze, with an exact lock or resolution cutoff and image digest. mini-swe-agent stays at 2.4.6, Terminus-2 follows the pinned stable Harbor tag, and LiteLLM is pinned. A post-freeze change is a new participant. | F14 | OL-09 | Validator rejects `@latest`/prerelease/unlocked |
| REQ-XD-082 | Reasoning effort is pinned to `high` for every phase-2 harness and enforced at the proxy, with requested and applied values in every receipt. | F15 | OL-03 | Receipt fixture with a harness sending `medium` |
| REQ-XD-090 | Results packets carry every F16 quantity with its interval and label, recomputable from committed records. The site renders only these. | F16 | OL-15 | Recompute-equality test |
| REQ-XD-091 | Claims language on results surfaces reuses `check-position.sh`'s `BANNED` list plus the F16 program additions and conditional forms. | F16 | OL-15 | Mutation battery over the site and packets |
| REQ-XD-092 | Every results surface states: internal record, not outside-verified; finite-manifest scope; stochastic hosted participant with reported catalog identity; allowlisted network; not leaderboard-comparable; HF figures are context only. | F16 | OL-15 | Claims check requires the labels |

## Handoff

**Dependencies resolved.** B1 (census, Harbor semantics, integrity facts), C1 (cost prior, concurrency, effort, support declarations) and C3 (shortlist, gates, pilot priors) were consumed. A1's lessons are applied:
- the reattempt bound (F12);
- estimators actually run, and the degenerate Beta(1,1) widths over deterministic outcomes are avoided by a stochastic participant and k = 5;
- intended vs served identity (F14);
- content-digest pins (F4).

**Supplies:**
- B2: schedule randomization, cause mapping, queue-wait field, `-r 0`;
- C2: effort pin, per-harness caps, queue wait, rewrite version in participant identity;
- C4: admission record fields, version policy;
- D2: dev/held-out partition, which L2 parity must respect;
- D3: the reserved 20-task dev pool, and effort as a phase-4 factor;
- F2: the F16 reporting contract and claims additions;
- H1: tickets and gates (F17), Q-D1-1…7.

**Proposed follow-up rows.** None as research rows. The two ADR candidates (declared-unsupported protocol support members; the phase-2 envelope) go to H1's amendment register.

**Conflicts with other notes:**
- B1 F19 proposed a ~30-task proportional draw (2/18/10). This note replaces it with a band census (T1 = 37), following Q-X31. B1's difficulty stratification survives inside the split.
- B1 F18 recommended public egress plus scan. The operator chose allowlist plus scan (Q-X30), and this note follows the operator.
- C3's $6 flat per-harness pilot cap is replaced by caps proportional to each harness's planning value (F9).
- C2, D2, B3 and A2 were accepted after dispatch and were not read. The coordinator should check:
  - C2's receipt fields against REQ-XD-043, 062, 082;
  - B3's rate card against the wall and instance-hour columns;
  - D2's use of held-out tasks against F4.

**Capability/portfolio dispositions.** None changed. Nothing here is live or fixture evidence of a harness result.

## Appendix A — full precision/cost grid

`table.py` regenerates the 60-row grid from `pair_results.json`. Its columns match the F5 table, with power at Bonferroni `.05/(H−1)` using `pow_m1/m3/m5/m7` for H = 2/4/6/8. The F5 table is a verbatim subset. The quote ticket recomputes the grid under exact routing (REQ-XD-024).

## Appendix B — simulation and census code

`census.py` is verbatim except that its final debug `print` is omitted. `alloc.py` is verbatim through the allocation; its reporting lines (per-cell and per-band prints and tallies) are omitted. `sim.py` is verbatim except for its module docstring, an unused `binom` helper, an unused `res={}` assignment and a progress `print`. `sim2.py` and `sim3.py` are summarized step by step. The SHA-256 values in the Simulation record identify the files exactly as run; `table.py` is SHA-256 36cd4e7b…82bd.

`census.py` (run as `python3 -I census.py docs/research/25_tb21-harbor.md`):

```python
import sys, re, collections
rows=[]
for line in open(sys.argv[1]):
    m=re.match(r'^\| ([a-z0-9.\-]+) \| ([EMH]) \| ([a-z\-]+) \| (\d) \| (\d) \| (\d+) \| (\d+) \|(.*)\|$', line)
    if m: rows.append(dict(task=m[1],diff=m[2],cat=m[3],cpu=int(m[4]),mem=int(m[5]),agent=int(m[6]),ver=int(m[7]),flags=m[8].strip()))
print("tasks",len(rows))
def band(a):
    return 'A(<=900)' if a<=900 else 'B(1200-1800)' if a<=1800 else 'C(2400-3600)' if a<=3600 else 'D(>=7200)'
excl={'qemu-startup','qemu-alpine-ssh'}
ct=collections.Counter(); ce=collections.Counter()
for r in rows:
    ct[(band(r['agent']),r['diff'])]+=1
    if r['task'] not in excl: ce[(band(r['agent']),r['diff'])]+=1
for b in sorted({band(r['agent']) for r in rows}):
    print(b, {d:ct[(b,d)] for d in 'EMH'}, 'eligible', {d:ce[(b,d)] for d in 'EMH'}, 'sum_elig', sum(ce[(b,d)] for d in 'EMH'))
for b in sorted({band(r['agent']) for r in rows}):
    s=sum(r['agent'] for r in rows if band(r['agent'])==b and r['task'] not in excl)
    v=sum(1 for r in rows if band(r['agent'])==b and r['task'] not in excl and r['ver']>900)
    sat=sum(1 for r in rows if band(r['agent'])==b and r['task'] not in excl and ('Sat' in r['flags'] or 'sat95' in r['flags']))
    hi=sum(1 for r in rows if band(r['agent'])==b and r['task'] not in excl and r['mem']>=8)
    print(b,'agent_s_sum',s,'h',round(s/3600,2),'ver>900',v,'sat/sat95',sat,'8GiB',hi)
```

`alloc.py` (output in F4):

```python
cells={('A','E'):4,('A','M'):32,('A','H'):12,('B','M'):12,('B','H'):10,('C','M'):8,('C','H'):7,('D','M'):1,('D','H'):1}
N=sum(cells.values()); D=20
q={c:n*D/N for c,n in cells.items()}
fl={c:int(v) for c,v in q.items()}
rem=D-sum(fl.values())
order=sorted(cells,key=lambda c:(-(q[c]-fl[c]),c))
for c in order[:rem]: fl[c]+=1
```

`sim.py` (seed 20261106; 1,000 datasets per cell; outputs `pair_results.json`):

```python
import random, math, json, sys, statistics as st
SEED=20261106
MIX={'board':(0.575,0.384),'easyband':(0.636,0.273),'hard':(0.384,0.575)}
TS=[10,20,30,37,48,60,87]; KS=[1,2,3,5]; DELTAS=[0.0,0.05,0.10,0.15]
NDATA=1000
def clip(p): return min(max(p,0.005),0.995)
def logit(p): return math.log(p/(1-p))
def expit(x): return 1/(1+math.exp(-x))
def Phi(x): return 0.5*(1+math.erf(x/math.sqrt(2)))
def calib(rng,mix,sigma,target_delta,n=40000):
    a,b=MIX[mix]
    base=[clip(rng.betavariate(a,b)) for _ in range(n)]
    gs=[(rng.gauss(0,sigma),rng.gauss(0,sigma)) for _ in range(n)]
    def marg(beta):
        return sum(expit(logit(p)+beta+g1)-expit(logit(p)+g0) for p,(g0,g1) in zip(base,gs))/n
    if target_delta==0: return 0.0, marg(0.0)
    lo,hi=0.0,4.0
    for _ in range(40):
        mid=(lo+hi)/2
        if marg(mid)<target_delta: lo=mid
        else: hi=mid
    return (lo+hi)/2, marg((lo+hi)/2)
def run_pair(mix,sigma,delta,rng):
    beta,real=calib(rng,mix,sigma,delta)
    a,b=MIX[mix]
    acc={(T,k):{'fm':[], 'bs':[], 'p':[], 'flip':[], 'est':[]} for T in TS for k in KS}
    for d in range(NDATA):
        tasks=[]
        for t in range(87):
            p=clip(rng.betavariate(a,b)); l=logit(p)
            p0=expit(l+rng.gauss(0,sigma)) if sigma>0 else p
            p1=expit(l+beta+(rng.gauss(0,sigma) if sigma>0 else 0))
            tasks.append(([rng.random()<p0 for _ in range(5)],[rng.random()<p1 for _ in range(5)]))
        for k in KS:
            c=[(sum(x[:k]),sum(y[:k])) for x,y in tasks]
            for T in TS:
                cc=c[:T]
                ds=[(c1-c0)/k for c0,c1 in cc]
                m=sum(ds)/T
                v=sum(((c0+1)*(k-c0+1))/((k+2)**2*(k+3))+((c1+1)*(k-c1+1))/((k+2)**2*(k+3)) for c0,c1 in cc)/T**2
                fm=1.96*math.sqrt(v)
                sd2=sum((x-m)**2 for x in ds)/T
                bs=1.96*math.sqrt(sd2/T)
                s2=sum(x*x for x in ds)
                p=1.0 if s2==0 else 2*(1-Phi(abs(sum(ds))/math.sqrt(s2)))
                fl=[2*c0*(k-c0)/(k*(k-1)) for c0,_ in cc] if k>=2 else None
                A=acc[(T,k)]; A['fm'].append(fm); A['bs'].append(bs); A['p'].append(p); A['est'].append(m)
                if fl: A['flip'].append(sum(fl)/T)
    out=[]
    for (T,k),A in acc.items():
        row=dict(mix=mix,sigma=sigma,delta=delta,real_delta=round(real,4),T=T,k=k,
          fm_hw=round(st.mean(A['fm']),4), bs_hw=round(st.mean(A['bs']),4),
          rmse=round(math.sqrt(st.mean([(e-real)**2 for e in A['est']])),4),
          flip=round(st.mean(A['flip']),3) if A['flip'] else None)
        for mfam in (1,3,5,7):
            row[f'pow_m{mfam}']=round(sum(1 for p in A['p'] if p<0.05/mfam)/NDATA,3)
        out.append(row)
    return out
if __name__=='__main__':
    rng=random.Random(SEED)
    allrows=[]
    for mix,sigma in [('board',0.75),('easyband',0.75),('board',0.0),('hard',0.75)]:
        for delta in DELTAS:
            allrows+=run_pair(mix,sigma,delta,rng)
    json.dump(allrows,open('pair_results.json','w'),indent=0)
```

`sim2.py` (seed 20261107; ranking plus approximation checks; outputs `rank_results.json`). These are its essential steps.

**Calibration.** For each of the 8 HF target rates `[.678,.651,.640,.632,.625,.602,.571,.506]`, it bisects a per-harness logit shift under `board`, σ = .75, over 30,000 tasks.

**Per dataset** (1,000 per design), for each task and harness it draws `c ~ Bin(k, expit(logit p_t + β_h + N(0,.75²)))/k`, then computes:
- the observed leader (argmax mean);
- the tie set, where harness h is tied iff `mean(d) − 1.96·sd(d)/sqrt(T) ≤ 0` for `d = x_leader − x_h`;
- Holm over the H−1 normal-approx sign-flip p-values against mini-swe-agent, using the step-down `p_(r) ≤ .05/(m−r)`.

**Designs:** harness sets {all 8}, {hermes, mini, claude, opencode}, {mini, claude}; T ∈ {20, 37, 48, 87}; k ∈ {3, 5}.

**Check 1.** 300 datasets per T ∈ {10, 20, 37}, k = 5, Δ = hermes − mini. Compares the normal-approx decision with a Monte Carlo sign flip at 2,000 draws, `(ext+1)/(draws+1)`.

**Check 2.** 40 datasets per (T, k) ∈ {(10,5), (37,5), (37,2)}. Compares the normal-approx finite-manifest half-width with the 4,000-draw Beta equal-tail half-width (indices 99 and 3899).

`sim3.py` (seed 20261108; outputs `mc_check.json`). For Δ ∈ {.10, .15} under `board`, σ = .75, and (T, k) ∈ {(37,5), (48,5), (87,3)}, it runs 400 datasets. Each computes the 999-draw Monte Carlo sign-flip p `(ext+1)/(draws+1)` and the normal approximation, and tabulates rejection at `.05/m` for m ∈ {1, 3, 7}. Results are in the Simulation record.
