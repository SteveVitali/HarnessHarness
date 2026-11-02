# 23_parity-method-review — A finite-manifest parity route

Stream: D — Scientific methodology\
Researched: 2026-10-29 UTC\
Author: /root/d4_methods; read-only audit by /root/d4_methods/parity_audit\
Ledger rows covered: D4\
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; approved uncommitted planning package and accepted D1/D2/E3/H1.\
Confidence overall: medium; mathematical proposal checked, integration/live feasibility unverified.

**Recommend one bounded route:** replace D2's per-task CP/Bonferroni parity construction with **Bernoulli KL concentration bounds on directional runner disagreement**, retaining the fixed manifest/equal repeats. This supplies conservative finite-sample coverage under stated independence. It is neither exact-binomial inversion nor an optimized interval. Near agreement can be measured much more economically; ordinary stochastic disagreement can still make parity unaffordable. No new fitter/statistics platform is proposed.

Keep the compaction comparison conditional. Funding remains **$0**; 60/300 subjects are not parity funding/power. The ±10-point/+5-point pair remains unratified (F6); this note changes no contract or execution authority.

## Basis

Read [brief](../brief.md), [approved Q policies/assignment](../research-ledger.md), [review](../protocol-resource-review.md), [conventions](../research/CONVENTIONS.md), [trace](../OUTLINE_TRACE.md), [D1](09_measurement-standard.md), [D2](10_compaction-protocol.md), [E3](14_integration-design.md), [H1 F7](20_build-handoff-design.md), root/docs instructions and `synthesize-spec` run mode at skills `e981ec57d487a2c41d416672483b5a4df15b1233`. Existing records were inspected, not rerun; coordinator owns state.

## Findings

### F1 — Fixture parity does not admit this study
Claim: No admissible same-participant HH/original-runner equivalence evidence was found in the inspected repository records.
Status: PARTIALLY VERIFIED\
Evidence kind: source inspection\
Evidence: [S3.12b](../../crates/hh-bench/tests/s3_12b.rs), 422–501/605–681; [parity wrapper](../../crates/hh-bench/src/parity.rs), 15–33; [recorded fixture evidence](../build/runs/S3.12b.md), 137–151.\
Retrieved: 2026-10-29T22:01:09Z; baseline above.\
Implication for the spec: Require authentic parity evidence before component headlines.\
Outline delta: CONFIRMS OL-09/15–16; EXTENDS OL-20–21.\
Limitations: Bounded parity/original-run/live-record audit, not proof that private external evidence does not exist.

The fixture repeats recorded `model_io`, with empty model snapshots and `equivalence_margin: None`; assertions check matching/metric identity, not equivalence. The wrapper checks only `ArtifactBenefit`. [ParityReport](../../crates/hh-lab/src/analysis.rs), 1294–1346, stores a comparison/provenance references. [ADR-0277 D4–D6](../adr/ADR-0277-s3.3-eval-kernel-and-bench-adapter-rulings.md) oracle transport equality is separate conformance.

Reuse requires participant/runner/task/split/model/environment/oracle pins, independent first attempts, budgets/cache, registered method/margins and equivalent verdict, with justified current-policy applicability. Empty maps comparing equal are not evidence. Otherwise retain product smoke, `parity=none`.

### F2 — Preserve the target and distinguish dimensions from caps
Claim: Fixed-task repetitions and matched resource ceilings do not imply task-population inference or equivalence of every realized resource total.
Status: VERIFIED\
Evidence kind: source inspection and inference\
Evidence: [§10.1–10.3](../../spec/sections/10-evaluation-reproducibility-plan.md); [§5h.2.1/AC-R-2.9.4-3](../../spec/sections/05h-measurement-evolution.md), 190–200/533; ADR-0142 D3/0144 D4–D7; [ADR-0213 adopted rulings](../../research/decisions/ADR-0213.md), 74–91.\
Retrieved: 2026-10-29T22:01:09Z; baseline above.\
Implication for the spec: Freeze target, equal task weights/repeats and a dimension-to-metric admission map.\
Outline delta: CONFIRMS OL-09–11/15; EXTENDS OL-21.\
Limitations: Exact pins, map and meaningful margins remain execution inputs.

For one fixed participant/policy and contamination stratum, `Δ=Σ_t(p_Ht−p_Ot)/T`; p is expected first-attempt outcome under HH/original runner. Equal R gives N=TR pairs. This is not a task-population estimand. Retries/replay/regrades add no independent n. Preserve adapter floor ≥3/task and compaction R≥5 where applicable.

Four matched caps remain enforced input/output totals, calls and wall time with same snapshots/cold start. Consumption imbalance is reported; it is not automatically equivalence failure. Statistical margins are separate: absolute pp for rates, relative for required cost/latency outcomes.

Sign a dimension→applicable metric/margin map before data: **capability** executable success/denominators; **reliability** failures/stops/pass^k (nonlinear pass^k needs separate inference); **grounding** observable detectors; **efficiency** named required cost/latency outcomes; **autonomy** approvals/interventions; **security/compliance** vetoes/integrity; **portability/evolvability** fixed-configuration limits and typed inapplicability. Inapplicable is not passed. Required unavailable dimensions block overall equivalence. Do not demand every scorecard metric, assume four caps are four parity outcomes, or delete hard outcomes later.

### F3 — Efficient standard methods still need the right design
Claim: Established approximate intervals and Bayesian credible intervals cannot be adopted as small-repeat frequentist parity by name alone.
Status: PARTIALLY VERIFIED\
Evidence kind: primary-source inspection and recorded analytic checks\
Evidence: [Tang 2021 v1 §§2/4](https://arxiv.org/pdf/2110.12636); [Tango 1998 pp.891–893](https://www.site.uottawa.ca/~nat/Courses/csi5388/Tango.paired.pdf); [statsmodels TOST assumptions](https://www.statsmodels.org/dev/generated/statsmodels.stats.proportion.tost_proportions_2indep.html); Appendix.\
Retrieved: 2026-10-29T21:52:38Z–21:57:00Z; documents read, no packages installed/executed.\
Implication for the spec: Admit a specific target/method; prohibit post-outcome method shopping.\
Outline delta: CORRECTS OL-10; EXTENDS OL-11/16/20–21.\
Limitations: Diagnostics are not exhaustive coverage validation or production tests.

D2 CP/Bonferroni is valid under binomial marginals but expensive. Task Beta(1,1) is a Bayesian finite-task candidate, not TOST or hierarchical population inference. Independent score/Newcombe/Miettinen–Nurminen assume binomial samples; pooled heterogeneous tasks do not qualify. Tang's stratified MOVER permits fixed weights/heterogeneous effects but is asymptotic. Tango score uses iid paired counts; fixed heterogeneous tasks are not automatically iid. Task bootstrap targets task-mixture variation; sign flips require sharp-null exchangeability. Select directional KL, preserving the design without a nuisance fitter.

At T30/R5, 5,000 simulated datasets/scenario: MOVER AV 90% coverage=.8112 for pH=pO=.5; false equivalence=.0900 for (.6,.5), ε=.10, nominal α=.05 (MC SE=.0040). AC gave .8538/.0458 but was unavailable for every deterministic-arm dataset. A supplementary Welch approximation gave false equivalence=.0643 at (.1,0). No approximate method is admitted from these checks.

### F4 — The chosen binary bound is explicit
Claim: Directional KL bounds provide conservative finite-sample one-sided coverage for this equal-weight target under independence across actual pairs.
Status: PARTIALLY VERIFIED\
Evidence kind: inference from primary theorem, analytic checks and design proposal\
Evidence: [Hoeffding, JASA 58 (1963), pp.13–15, Theorem 1 Eq.(2.1)](https://www.cs.rpi.edu/academics/courses/spring06/random/hoefding.pdf), visually inspected; Appendix.\
Retrieved: 2026-10-29T22:00:08Z.\
Implication for the spec: ADR-admit this procedure, pair metadata, assumptions and outward rounding.\
Outline delta: EXTENDS OL-09–11/16/20–21.\
Limitations: Independence/valid grades need evidence; conservative coverage is not optimal efficiency.

Hoeffding bounds independent [0,1] variables without identical distributions by `exp(−N kl(x,μ))`; affine transformations handle known finite ranges. **Application derived here:** preassign H/O first attempts within `(task,repeat,pair_id)`; define `W+=1{YH=1,YO=0}`, `W−=1{YH=0,YO=1}`. Then Δ equals the difference of their mean expectations. Within-pair dependence is allowed; across-pair independence is required. IDs create no correlation/CRN: independent p=.5 outcomes typically yield q+=q−=.25.

For x=c/N, `Kη(x,N)=[L,U]` solves

```
kl(x,p)=x log(x/p)+(1−x) log((1−x)/(1−p))
N kl(x,L)=N kl(x,U)=log(1/η), L≤x≤U.
L=0 at x=0; U=1 at x=1; continuous endpoint limits.
```

Each endpoint has one-sided noncoverage≤η. At TOST α=.05 use η=α/2 for each direction and `LΔ=L+−U−`, `UΔ=U+−L−`, point=(c+−c−)/N. Union bounds give each Δ tail error≤α without W+/W− independence; joint coverage≥1−2α. Equivalent requires **LΔ>−ε and UΔ<ε**. Under either null branch, passing requires rejecting that true branch: size≤α, not 2α. A separate 95% reporting interval uses α=.025/η=.0125. Disjoint=not-equivalent; overlap=inconclusive.

This is concentration, not CP/binomial inversion. Equal task weights/R identify the target; unequal designs require another admitted bound. Shared shocks, carryover/cache or repeated subjects violate independence; calendar balance alone is insufficient. Current §10 allows replicate pairing only with honoured seeds: explicitly amend it for this statistical grouping, keeping task pairing and seed status separate. This claims no CRN or fitted correlation. Unsupported dependence refuses inference.

### F5 — Missing and continuous dimensions have explicit limits
Claim: Binary parity cannot replace required continuous outcomes, excluded classes, missingness or vetoes.
Status: UNVERIFIED\
Evidence kind: design proposal and mathematical reduction\
Evidence: F2/F4; [D1 F2/F5/F8](09_measurement-standard.md); [FDA October 2022, §III.C.1 p.9](https://www.fda.gov/media/162416/download), all-endpoints conjunction principle.\
Retrieved: Repository 2026-10-29T22:01:09Z; FDA 2026-10-29T21:57:27Z.\
Implication for the spec: Freeze target, range, availability, margins and conjunction before collection.\
Outline delta: CONFIRMS OL-10–11/15; EXTENDS OL-16/21.\
Limitations: Real bounds/detector coverage are unmeasured; unavailable required dimensions cannot pass.

Preserve planned N. With u uncertain pairs, each directional interval becomes `[Kη(c/N).L,Kη((c+u)/N).U]`; a conservative completion envelope, not missing-at-random deletion. Keep canonical/raw/joint-without-observed-veto populations and class counts; unknown is neither zero nor clean. Complete fixed-denominator binary outcomes admit F4. Informative eligible-only exclusions do not acquire coverage by shrinking N; required conditional claims need their own admitted method.

For required continuous absolute contrast `D=XH−XO∈[a,b]`, apply K to `mean((D−a)/(b−a))`, then transform back. Known a=b is exact; zero sample variance is not a known zero range. For relative margin ρ target **ratio of finite-manifest means**, not mean task ratios: require lower bound for `EH−(1−ρ)EO>0` and upper bound for `EH−(1+ρ)EO<0`, using known physical ranges and K at α per direction. Do not divide by observed reference or compare milliseconds with ppm. Zero/near-zero reference needs a preregistered absolute convention or refusal; unbounded/unknown billing, missing values without completion bounds, unbounded intensities or different eligible-only populations require unavailable inference. No clipping changes the target. This relative rule needs an explicit owner amendment; task-bootstrap/tail descriptions remain separate.

For one preregistered **all-dimensions/strata-equivalent** claim, IUT needs no extra endpoint Bonferroni: at least one true null must be rejected. Admit this explicit exception; selected individual/multiple claims or any-endpoint efficacy still need family correction, including D2 Holm. Required unknown members and security/compliance vetoes cannot be removed or averaged away.

### F6 — Calibration follows the claim
Claim: Baseline adapter parity alone cannot bound bias in a two-policy treatment contrast.
Status: VERIFIED\
Evidence kind: algebraic inference and design proposal\
Evidence: D1/D2 target definitions, F2/F4 and identity below.\
Retrieved: 2026-10-29T22:02:11Z.\
Implication for the spec: Choose native/transfer scope and practical-error budget before outcomes.\
Outline delta: CORRECTS OL-09–11; EXTENDS OL-15–16.\
Limitations: No useful-effect or calibration margin is ratified by this arithmetic.

For policies A/B let `b_a=μ_H,a−μ_O,a`. Then `Δ_H−Δ_O=b_B−b_A`; bounds |b_a|≤εa permit error≤εA+εB. Baseline-only parity leaves bB unknown; bA=−.10,bB=.10 permits spurious +.20. Cancellation needs evidence.

**Prefer HH-native finite-manifest efficacy for the first wave**, after required adapter admission scoped to the actual participant/configurations. No cross-runner effect claim follows. Baseline parity cannot silently admit both policies: require their own evidence or predeclared justified reuse within identical bridge semantics.

Transferred effects are conditional: calibrate both policies under both runners/target pins or admit a runner×policy analysis. Register useful gain δ, calibration budget κ and joint error allocation; require εA+εB≤κ and jointly valid `L_H−κ>δ`, or `L_H−U_bB+L_bA>δ`. Allocate uncertainty across efficacy and both calibration claims; two 90% equivalence intervals are not a 95% joint certificate. Efficacy multiplicity is additional.

Choose δ from task value/cost and κ as tolerable error before data, then quote precision. Illustratively κ=δ/2 with equal allocation gives εA=εB=δ/4=.0125 for δ=.05; not a ratified margin. Infeasibility narrows transfer scope or leaves inconclusive results, never post-outcome margin widening. Parity does not bound shared oracle bias; upstream controls/validity/leakage remain gates.

### F7 — Improved attainability is not power
Claim: The proposed construction removes D2's task-count penalty near agreement but can remain futile under ordinary stochastic disagreement.
Status: PARTIALLY VERIFIED\
Evidence kind: recorded analytic checks and design proposal\
Evidence: Appendix Python 3.14.8 checks completed by 2026-10-29T22:01:09Z; D2 F8/E3 F6/approved Q-4.\
Retrieved: 2026-10-29T22:01:09Z.\
Implication for the spec: Price every required endpoint/policy; fund only a feasible frozen design.\
Outline delta: EXTENDS OL-09–11/20–21.\
Limitations: Hypothetical distributions, not HH pilot observations or authorized sample increases.

At α=.05, zero discordances gives `±[1−.025^(1/N)]`:

| Tasks × R | Pairs / subjects | D2 half-width | Proposed |
|---|---:|---:|---:|
| 2 × 5 | 10 / 20 | .583723 | .308497 |
| 6 × 5 | 30 / 60 | .665837 | .115703 |
| 30 × 5 | 150 / 300 | .757806 | .024293 |

At N150, equal directional counts {1,3,8,38} give half-widths {.042944,.065355,.100601,.190899}. Zero-disagreement minimum N for ε={.10,.05,.025,.0125} is {36,72,146,294} before floors/block rounding: **attainability, not power**. Both-policy calibration doubles subjects; joint confidence increases requirements.

Hypothetical iid trinomial enumeration, Δ=0, gives ε=.10 pass probabilities at total discordance q={.02,.10,.50}: N150={.998308,.195500,<10^-26}; N300={.99999999,.901768,<10^-22}. At ε=.025,q=.02 they are .048296/.038398; discreteness need not improve monotonically. D2's 300 A/B subjects are not 300 HH/original pairs; compatible original observations need separate funding/prespecified reuse.

Sequence: engineering/method admission → separately quoted feasibility → one frozen parity tranche only if all required endpoint/policy precision scenarios fit ceilings → confirmation after parity. Six dev tasks do not establish 30 held-out tasks' disagreement. No easier-task selection, pilot relabelling, fixed-N peeking/top-ups. Futility preserves the unmet comparison while independent useful-task/receipt work advances. Quote both runners/policies/strata, instrument/retry/regrade/cancellation costs, CPU/memory/storage/retention/time and prices; unset funding stays zero.

### F8 — Small owner changes still need admission and tests
Claim: Existing report plumbing cannot be assumed to execute the proposed method or relative margins correctly.
Status: VERIFIED\
Evidence kind: source inspection and design proposal\
Evidence: [compare](../../crates/hh-eval/src/compare.rs), 622–634/781–791/883–892; [stats](../../crates/hh-eval/src/stats.rs), 237–275; [benefits](../../crates/hh-eval/src/benefits.rs), 427–485; [analysis kernel](../../crates/hh-analysis/src/kernel.rs), 557–569/1583–1591; ADR-0158/0277 and E3 F6.\
Retrieved: 2026-10-29T22:01:09Z.\
Implication for the spec: Amend method/pair metadata and verify actual existing-owner routing.\
Outline delta: EXTENDS OL-10–11/16/20–24.\
Limitations: No production changes or Rust tests; numerical proposals need independent review.

Compare currently always constructs paired bootstrap with empty floors; draws are fixed at 4,000. Equivalence parses relative margins but compares raw Δ against ±margin_ppm. Empty snapshot maps compare equal. Store-backed analysis applies multiplicity but uses the same computation.

Narrow ADR: admit finite-manifest concentration, independent-run grouping without CRN, bounded relative-mean tests, completion envelopes and IUT logic. Explicit exception to small-n Bayesian interval selection; no credible-as-TOST relabelling, lower global floors or dependency/fitter. Keep R floors/classes/history. D2 efficacy methods remain distinct conditional choices.

Existing owners: eval math/selection, lab schema, analysis persistence/families, bench authentic pairs, ontology/codegen. Record α/η, target, weights/R/pair IDs, range/margin units and version. Use logarithms/bisection; round outward and compare strictly. Invalid/nonfinite counts, unequal cells, missing margins/keys and unsupported dependence/ranges refuse.

Acceptance: endpoints/monotonicity, small-N/heterogeneous tails, sign reversal, reporting/TOST levels, relative scale invariance/zero reference, missing/veto envelopes, retry/shared-shock counterexamples, empty-pin refusal and actual report dispatch. Independent review checks derivation/routing; no approval of margins/spending is implied.
## Reproducible analytic checks

All computations used OS temporary scratch, Python 3.14.8 standard library; no production imports, benchmark tasks or provider/environment execution. The core below reproduces the KL endpoints and the finite trinomial pass probabilities in F7. No Monte Carlo seed is involved in this enumeration. Outward rounding/robust production error handling remain implementation work.

```python
import math
eta = .025

def kl(x, p):
    return ((x*math.log(x/p) if x else 0) +
            ((1-x)*math.log((1-x)/(1-p)) if x < 1 else 0))

def bounds(x, n):
    cut = math.log(1/eta)/n
    def root(a, b, lower):
        for _ in range(90):
            m = (a+b)/2
            if (kl(x, m) > cut) == lower: a = m
            else: b = m
        return (a+b)/2
    return (0 if x == 0 else root(0, x, True),
            1 if x == 1 else root(x, 1, False))

def pass_probability(n, q, delta, eps):
    ps = ((q+delta)/2, (q-delta)/2, 1-q)
    bs = [bounds(k/n, n) for k in range(n+1)]
    total = 0.
    for b in range(n+1):
        for c in range(n-b+1):
            ks = (b, c, n-b-c)
            if any(p == 0 and k for p, k in zip(ps, ks)): continue
            lo, hi = bs[b][0]-bs[c][1], bs[b][1]-bs[c][0]
            if lo > -eps and hi < eps:
                lp = (math.lgamma(n+1)-sum(math.lgamma(k+1) for k in ks)
                      + sum(k*math.log(p) for p, k in zip(ps, ks) if k))
                total += math.exp(lp)
    return total
```

Executed 32 scenarios: `(N,eps)` = (150,.10),(150,.025),(300,.10),(300,.025); each tests Δ=0 with q=0,.02,.10,.50 and Δ=eps with q=eps,.20,.50,.90. Largest boundary pass probability among these was .02242293. Three finite Poisson-binomial convolutions at N20 used p vectors `.05×10+.95×10`, `[.1,.2,.3,.4,.5]×4`, and `0×10+.2×10`. Update masses by `new[k]+=old[k](1−p); new[k+1]+=old[k]p`. Sum masses where lower>mean or upper<mean. Tail errors were respectively (.0000016752,.0000016752), (.00323554,.00565784), (.00086436,0), each ≤η; mass totals within 10^-12. Three assertions passed. The theorem supplies general coverage; these examples check the analytic code only.

For F3, use `z=NormalDist().inv_cdf(.95)` and Wilson cell interval
`mid=(p+z²/(2R))/(1+z²/R)`, `h=z sqrt(p(1−p)/R+z²/(4R²))/(1+z²/R)`.
For AV, `d=mean(pH−pO)`, lower width `sqrt(Σ[(pH−LH)²+(UO−pO)²])/T`, upper width with U/L reversed. For AC, within each arm set `s_t=sqrt(p_t(1−p_t)/R)`, `z*=z sqrt(Σs_t²)/Σs_t`, calculate Wilson bounds using z*, average bounds/points, then square-and-add the two arm distances. A zero denominator is unavailable, not patched. Simulate Bernoulli runs with `random.Random(629301)`, looping R=5,10,20, each over six scenarios in this order: (.5,.5),(.6,.5),(.1,0),(1,.9),(.01,.01), then alternating task pairs (pH,pO)=(.9,.8),(.1,0). For each, T30, 5,000 datasets; draw all H task runs then all O task runs; count coverage and strict containment in ±.10. This is 90,000 datasets, 18 scenarios; all completed. The first two reproduce F3's reported values. Unavailable AC datasets are retained in the total denominator.

The supplementary Welch diagnostic used seed 629302, R=5,10, 10,000 datasets for those same scenarios omitting (1,.9). Set `v_at=p_at(1−p_at)/(R−1)/T²`, `V=Σv`, `df=V²/Σ(v²/(R−1))`; V=0 unavailable. The t quantile used the diagnostic expansion `z+(z³+z)/(4df)+(5z⁵+16z³+3z)/(96df²)+(3z⁷+19z⁵+17z³−15z)/(384df³)`; interval d±t√V. All 100,000 datasets completed; this approximation and its quantile were not validated for adoption. No favorable result from these exploratory checks is a method acceptance or a study result.

## Answer and completion assessment

D4 resolves a concrete candidate: fixed-manifest directional KL bounds for binary parity, bounded linear-contrast tests only for required continuous means, explicit pairing/method amendments and endpoint-specific feasibility. Prefer HH-native finite-manifest efficacy in this first wave; transferred effects remain conditional. Existing parity evidence is insufficient. Negative/inconclusive science is acceptable; missing admission is a blocker, not a null study.

## Open questions

- **Operator/method reviewer + E3/H2 → O1:** ratify the narrow method/pairing/IUT amendment and dimension map; review does not change the frozen contract.
- **Operator/study owner → D2/F3:** choose exact native/configuration scope, δ/harm/margins and any transfer κ/joint allocation before data; no automatic .05/.10 approval.
- **Execution owner/operator → P2/P7/P8/F1:** actual runner/task/model/oracle/backend pins, reset/independence evidence, physical bounds and complete separately funded quote. Missing inputs refuse dispatch.
- **E3/reviewer → implementation acceptance:** independently check derivation, numerics, units, missingness and actual routing. No broad statistics platform is needed.

## Spec requirements emitted

| ID | Candidate | Findings / outline | Acceptance |
|---|---|---|---|
| REQ-D-301 | Authentic original-runner admission. | F1; OL-09/15–16 | Fixture/wrapper alone rejected. |
| REQ-D-302 | Fixed target, equal weights/R, independent groups. | F2/F4; OL-09–10/21 | Manifest/count/dependence checks. |
| REQ-D-303 | Dimension map separate from caps. | F2/F5; OL-10/15/21 | Required unavailable outcome blocks. |
| REQ-D-304 | Declared KL bounds/TOST levels. | F4; OL-10–11/16 | Tail/endpoint/outward-rounding checks. |
| REQ-D-305 | Planned missingness/veto populations. | F5; OL-10–11/15 | Unknown/retry completion envelopes. |
| REQ-D-306 | Correct bounded relative-mean tests. | F5/F8; OL-10/16/20 | Scale/zero/unknown-bound refusals. |
| REQ-D-307 | All-endpoint versus selected-claim multiplicity. | F5; OL-10–11 | Frozen family/unknown-member checks. |
| REQ-D-308 | Scope calibration to actual policies/claim. | F6; OL-09–11/15 | Bias-addition/joint-error counterexamples. |
| REQ-D-309 | Feasible quotes/fixed final sample. | F7; OL-09–11/20 | Discordance scenarios; zero unset funding. |
| REQ-D-310 | Existing-owner admission/routing. | F3/F8; OL-16/20–24 | ADR, numerical and dispatch acceptance. |

## Handoff and verification

H2/O1 reconcile D2 REQ-D-107/108/109 and E3 F6 with this follow-up; preserve D2's separately conditional Bayesian efficacy analysis. H1 routes the narrow ADR. No extra research unit is needed to specify the candidate; implementation validation and execution gates remain explicit. Portfolio unchanged; unmet parity withholds component headlines.

Deferrals-first returned no rows. Planning validation: 0 violations/0 validator warnings (locale fallback emitted). Analytic checks completed: 32 finite trinomial enumerations, three heterogeneous-tail assertions, 18 MOVER scenarios/90,000 datasets and ten Welch diagnostics/100,000 datasets; unavailable/error behavior reported above. Scoped validation passed: 22 relative links, eight complete findings, ten unique candidate IDs, four closing sections and no unexpected whitespace/personal paths. No-index diff check emitted no diagnostics (exit 1 denotes the added file); added-file diff/stat reviewed. These are analytic/document checks, not product or Rust evidence. No builds, benchmark/provider/container runs, installations, commits or outreach. Only this note was written in the repository. Coordinator owns final package acceptance/state.
