# 22_independent-spec-review — O2 review of supplemental draft v0.1

**Recorded:** 2026-10-29T22:58:21Z.  
**Reviewer:** `/root/o2_review`, fresh independent context.  
**Review baseline:** `001490c14a6568972d16a663bb04c07a4255d30d` plus the identified uncommitted planning inputs.  
**Verdict:** **Not ready for Q-7 in this exact snapshot:** resolve or explicitly rebut the two P2 findings below and perform a targeted rereview. Neither finding requires a new broad research program or execution of the deferred studies. No P0/P1 defect was established in this bounded review. This is a review recommendation, not a coordinator disposition, method admission, operator ratification or implementation authorization.

The specification is substantially decomposable as a conditional program. It preserves the central real comparison, its unmet state when admission fails, the native ownership boundaries, and separate engineering/research/participation acceptance. The remaining exact task pins, meaningful margins, quotes, rights, backend evidence and actual participant consent are appropriately stated as later binding gates. They are not automatically evidence of a missing design. The two findings concern contradictory acceptance language and incomplete acceptance routing already present in the draft.

## Independence, scope and input identities

The initial substantive inputs were only the draft, research ledger and founding brief. I read the root/docs instructions, ran deferrals-first, and applied `synthesize-spec` review mode at workflow revision `e981ec57d487a2c41d416672483b5a4df15b1233`. I did not read the authoring conversation, contact the author, or use another agent. The ledger's current state and work/decision/dependency tables were read; authoring-session history was not used to infer intent. This note is the sole repository write by this reviewer. Original findings remain below for any later dated rereview.

| Input | SHA-256 at review |
|---|---|
| [Supplemental draft](../evidence-and-adoption-spec.md) | `df924757b5dd3853c398e3149e2fda630113bc185cab5d4b5ab8a64c96812ed9` |
| [Research ledger](../research-ledger.md) | `5c85ac835ac326b8b8e51ed04c550054bcf4bb3af1b473947180984bff878427` |
| [Founding brief](../brief.md) | `fbc2daa2ce3078f57d36b772dfc4293d81408b93794201a0a819fd69766d04e8` |
| [Outline trace](../OUTLINE_TRACE.md) | `9bd6b234e9fc2dd3046d2525eb7a2d4adf11c19c47c7ad62abb39cb5466817c9` |
| [Root instructions](../../AGENTS.md) | `6597bb89dde4a4270d79007568f02170ae753af2aa00ed4189f89d88e7f7b60a` |
| [Documentation instructions](../AGENTS.md) | `a32f4d1dec294b664a215ef16536d68d07f1ca9734b90051091e37bfcef0f008` |

All **22 accepted-note hashes** were independently recomputed against Appendix C and matched. That immutable draft digest therefore identifies the complete checked source-hash manifest. There were no missing/changed source identities or unexpected candidate counts. Source bytes were read for these checks, but hashing a note does not mean every citation within that note was independently audited.

## Findings requiring coordinator disposition

### O2-01 — P2 / medium: reconcile one-direction interchange with retained round-trip acceptance

**Location:** Draft §0.1 line 17; §1.1 S6 line 74; §1.4 line 125; Appendix A.B2 `REQ-B-102` lines 770 and 780. Related OL-04/OL-16; AC-1-1/AC-6-2.

**Problem:** The selected scope permits “at most one external consumer/direction” and asks S6 to exercise that direction. The adopted source requirement's acceptance still requires “Round-trip one real artifact through a pinned external reader.” Section 0.1 expressly preserves source acceptance predicates unless an explicit ruling replaces them. The decision register does not explicitly reconcile this particular predicate. One-way export with a successful reader check can satisfy §1.4 while failing the ordinary two-way meaning of the retained round-trip requirement. The registry still says simply `adopted`.

**Evidence:** [B2's source requirement](../research/05_ecosystem-and-benchmarks.md#spec-requirements-emitted) contains the same round-trip predicate. [C2 F2](08_feasibility-and-scope.md#f2--stage-readiness-without-replacing-the-task-or-study-objective) and [H2 F2](21_synthesis-readiness.md#f2--the-selected-scope-is-one-shared-evidence-chain) select one consumer/direction; the draft carries that selected scope. This review does not recommend expanding the approved optional exercise into two directions. The problem is the surviving, differently testable acceptance obligation.

**Proposed repair:** Preserve the literal original row and ID. Add an explicit ruling that adapts `REQ-B-102`'s round-trip acceptance to the approved one-direction consumer validation, naming the required real artifact, pinned reader, field/loss checks and unsupported direction/version controls. Link that ruling from its registry row and make the governing acceptance hook explicit. Alternatively, define precisely what “round-trip” means here if no reverse conversion is intended, or explicitly restore and authorize a two-direction scope. Do not silently claim one-way validation proves round-trip interoperability.

**Targeted recheck:** A reviewer should be able to give one unambiguous pass/fail answer for a native→foreign export accepted by the external reader with no return import. Confirm the optional scope and its withheld interoperability claims remain intact. Recommended disposition: adapt the acceptance mapping; the coordinator decides.

### O2-02 — P2 / medium: route retained reader-observation predicates to participant acceptance

**Location:** Appendix A.B2 `REQ-B-107` source/registry lines 775/785; Appendix A.G1 `REQ-G-001`, `REQ-G-003`, `REQ-G-005` source lines 1109/1111/1113 and registry lines 1121/1123/1125. Governing §0.2 line 29, §8.4 and AC-8-2; OL-01/OL-04/OL-06/OL-19.

**Problem:** The source acceptance for `REQ-B-107` requires an editorial/source audit **plus a newcomer or researcher walkthrough**, yet its only registry hook is AC-1-1, a D-layer catalogue review. Its layer column says D/P, but there is no P hook. `REQ-G-001` explicitly retains G3 observations, `REQ-G-003` retains reader explanation, and `REQ-G-005` retains the reader's procedure choice. Their registry rows point to AC-7-1, AC-7-2 and AC-6-2 with D/E layers. Those checks do not require the G3 navigation/comprehension/procedure-choice observations. In particular, AC-6-2's outside verify/recompute pass is not the formative interpretation task.

The body already supplies suitable participant work in §8.4 and AC-8-2. Thus this is an acceptance-routing defect, not a request to invent another study. A decomposer consuming the advertised per-ID governing hook/layer can close these requirements after technical checks while the retained source predicates remain unmet. Section 0.2 explicitly says an E pass is not a P pass.

**Evidence:** The candidate rows are preserved verbatim and the corresponding original G1/B2 predicates agree. The gap is visible without disputing their content: compare those rows with [AC-1-1](../evidence-and-adoption-spec.md#ac-1-1), [AC-6-2](../evidence-and-adoption-spec.md#ac-6-2), [AC-7-1](../evidence-and-adoption-spec.md#ac-7-1) and [AC-8-2](../evidence-and-adoption-spec.md#ac-8-2). The broad OL mappings and separately adopted `REQ-G-009` do not state that its participant evidence is also required to close these individual IDs.

**Proposed repair:** Add the explicit §8.4/AC-8-2 P-layer dependency to these registry rows, or supply an equally explicit cross-requirement acceptance dependency on the owning G3 observation requirement. Preserve one primary owner per adopted requirement while naming the consumer evidence dependency. Clarify that a technical/document slice can complete without marking the retained participant predicate satisfied. Review analogous reader predicates when making this small mapping correction; do not require new participants beyond the already selected cohort.

**Targeted recheck:** With all guide/receipt/CI tests passing but no formative observations, the coverage map must still show these participant predicates as unmet. With the required consented observations and branch/interpretation records, it must identify the exact shared evidence and permit closure without duplicate work. Recommended disposition: accept the routing correction; the coordinator decides.

## Coverage and consistency assessment

I read the **entire 1,327-line / 222,517-byte draft**, including §0–§9 and Appendices A–F, in bounded ranges. Every candidate and registry row was inspected, not only the heading index. I compared all 24 outline-obligation texts and the six scenarios, 22 capability families, nine later groups and eight packages against the brief/trace and accepted scope. Later-family dispositions are planning coverage, not feature execution claims. OL-13's optional package boundary is visible, and neither extension success nor scientific replication is falsely claimed as completed.

A read-only Python check independently parsed all emitting source-note tables and the draft:

| Check | Result |
|---|---|
| Accepted source hashes/counts | 22/22 matched |
| Literal source candidate rows copied into Appendix A | 180/180 byte-identical line strings |
| Source ID set = Appendix A source ID set = registry ID set | 180 IDs in each; equal |
| Dispositions | 167 adopted, 12 merged, 1 superseded; no unknown form |
| Alias/supersession targets | All target existing adopted IDs; no dangling target |
| Registry AC references | Every row has at least one resolvable AC hook; semantic exceptions are O2-01/02 |
| Explicit anchors | 218, unique; no unresolved internal anchor references |
| Local file links | 55 occurrences / 34 unique paths; 0 missing files |
| OL identity/text equality against OUTLINE_TRACE | 24/24 matched |

All accepted notes' candidate tables and implication statements were checked for coverage. I additionally read bounded substantive portions of C1/C2, D2/D4, E1/E2/E3, F3 and H1/H2 when tracing the body, with the deepest source review on the D2/D4 statistical design, E3 lifecycle/accounting design, F3 recipient procedures and H2 reconciliation. For the other notes, the coverage review relied on their candidate tables and implication statements, with selected claims/limits where inspected. I did **not** reread every line of all 22 notes, nor re-audit each note's underlying external or repository citation. The draft's incorporation of binding lower-layer contracts matters: existing opacity/compliance semantics, for example, need not be reimplemented from a short table cell. I inspected the relevant canonical evaluation/benchmark/measurement passages and ADR-0077 metric definitions rather than treating that shortening alone as a missing design.

The twelve merged aliases preserve their substantive predicates through their target sections and linked checks. The single D-108→D-304 supersession is explicit, with D-309/310 as dependencies rather than additional canonical targets. I found no duplicate requirement owner allocation, silent new ID, hidden claim that the old CP engine remains mandatory, or evidence that drafting has changed the frozen behavioral contract. The source-index and literal-row checks establish identity preservation; they are not a universal semantic proof, as O2-01/02 illustrate.

S10 correctly removes an otherwise artificial dependency: the informative offline lesson requires an admitted exact analysis path, but that early analysis slice need not wait for live parity or collection. The package graph now expresses that distinction. The post-terminal grade design also names pending gates across consumers, immutable corrections and crash tests; the physical-accounting rule explicitly prevents logical reversal from replenishing spent capacity. I found no additional contradiction in those selected semantics. Actual implementations must still demonstrate the specified cases.

## Statistical and methodological review

**Directional KL and S11:** I found the written algebra coherent under the stated assumptions. For independent bounded observations, the elementary exponential-moment argument uses convexity to bound each moment by its Bernoulli endpoint mixture and concavity of the log to bound the product by the mixture at the average expectation; optimizing the exponential bound yields the Bernoulli KL tail. Applying the one-sided endpoints separately to W+ and W− and union-bounding the two components of each contrast tail gives ≤2η=α per tail. It does not require independence between W+ and W− within a pair. The fixed equal-repeat design identifies the stated finite-manifest contrast; dependence across pairs, changed weights or unknown ranges are outside that argument.

Strict interval containment with α=.05 is consequently the stated two-one-sided-test equivalence rule, with at least 90% joint interval coverage. S11 correctly separates its negative inference: the 95% reporting interval outside the band controls the two-sided negative decision under the stated selected-claim correction. The two relative-contrast negative branches likewise use .025 each. The single all-endpoint conjunction IUT is distinct from selected endpoint or efficacy-family claims. I found no justification for combining these bounds with a Bayesian efficacy endpoint into a frequentist joint transfer certificate; the draft expressly prohibits that shortcut.

**Finite-task efficacy:** The conjugate posterior calculation is correct for the declared independent binomial likelihood and independent Beta priors. The [Agresti/Hitchcock survey, §2.1](https://people.stat.sc.edu/Hitchcock/bayesfinal.pdf) was accessed and its Beta conjugacy/mean discussion read. The draft properly distinguishes this conditional finite-task credible interval from task-mixture bootstrap sensitivity and sharp-null randomization testing. This does not establish that actual future runs meet those assumptions or that R=5 is informative. The mandatory simulation/precision/refusal review is therefore substantive future acceptance, not paperwork.

**Independent numeric checks executed:** A Python standard-library diagnostic used natural-log KL and 80-step bisection with conservative bracket endpoints. It independently reproduced all seven rounded widths in §4.5:

| Case | Independently computed width |
|---|---:|
| Zero discordance, N=10 | 0.30849710781876094 |
| Zero discordance, N=30 | 0.11570330822202773 |
| Zero discordance, N=150 | 0.0242925971661086 |
| N=150, c+=c−=1 | 0.04294385641379865 |
| N=150, c+=c−=3 | 0.06535456054109016 |
| N=150, c+=c−=8 | 0.10060077699968914 |
| N=150, c+=c−=38 | 0.19089945371025174 |

Three exact finite Poisson-binomial convolutions at N=20 reproduced D4's one-sided endpoint error examples: approximately `(0.00000167523, 0.00000167523)`, `(0.00323553887, 0.00565784427)`, and `(0.0008643584, 0)`, all ≤η=.025; probability masses normalized within 1e−12. One check enumerated every feasible directional-count completion of three uncertain pairs and verified that the proposed envelope contained each completed interval. One check verified positive-unit scale invariance of the normalized relative-contrast construction at scale factors 1, 1000 and .001. **12 diagnostic assertion groups passed; 0 failed.** These are bounded checks, not production implementation tests or exhaustive coverage validation.

**Still required:** Expert review of the actual registered endpoint/population/range/applicability map, independence and original-runner/policy applicability; a numerical reviewer for floating-point/outward-rounding behavior, endpoint extremes and posterior inversion/RNG/draw-error rules; actual existing-owner routing tests; the specified simulation battery with Monte Carlo uncertainty; and empirical feasibility/precision/resource admission before collection. I did not run D4's full trinomial/MOVER/Welch experiments or the proposed ≥2,000-dataset-per-scenario efficacy battery. No actual method implementation, original-runner parity, provider bounds, rights or user evidence was validated. I do not claim universal mathematical proof or engineering feasibility from the diagnostics.

## Checks executed and review limits

- `bash scripts/deferrals-open.sh`: passed, exit 0, empty output (zero open rows); no deferral closed.
- `git rev-parse HEAD` and worktree status: baseline confirmed; pre-existing uncommitted planning work preserved.
- External `check-build-memory.sh --planning docs/research-ledger.md`: passed, 0 violations / 0 warnings. Locale fallback warnings were environmental output, not validator findings. The validator's temporary report was outside the repository.
- Read-only Python identity/coverage/link checks: results above; 0 structural failures.
- Read-only Python numerical diagnostics: 12 passed / 0 failed groups, as scoped above.
- `bash scripts/check-living-docs.sh`: passed, 387 living files scanned, no personal paths. Review-note relative links/anchors were checked; one draft link was corrected before delivery.
- No Cargo build, test, fmt, drift/removability battery or executable example was run: this review changes only prose and claims no code verification. No benchmark task, provider, container, install, recruitment, hosting, publication, spend, commit or further agent was used.

The external Hoeffding PDF URL cited by D4 was opened and screenshot retrieval attempted, but its tool response exposed no readable theorem text/pixels in this review. I do not count that as independent visual source verification; the mathematical discussion above is the reviewer's bounded derivation. Other source statements are evaluated against the accepted records unless a direct source inspection is expressly named.

The check for local file existence did not audit every external link or every implicit heading anchor. The review did not re-run historical live evidence, independently verify every capability family or pre-existing build warning, certify consent/rights/security, measure implementation effort, or test the future handoff guard. Those limits do not excuse a false present claim; the draft generally labels these matters as unexecuted, scoped, or gated.

## Coordinator handoff

Record an explicit accept/adapt/rebut/defer decision for **O2-01** and **O2-02** in Appendix E. Preserve these original findings. Apply the chosen bounded corrections, then request a delta rereview of the actual changed draft/hash and affected mappings. If rebutting, identify the exact governing rule that yields unambiguous acceptance for the counterexample in each finding; favorable intended meaning is insufficient.

Once these findings are resolved and targeted rereview confirms that result, I see no other established obstacle in this review to presenting the conditional specification for Q-7. That future human decision remains separate from ADR admission, build readiness, funding, collection, recipient/publication readiness and scientific conclusions.


<a id="delta-rereview"></a>
## Dated delta rereview — v0.2

**Recorded:** 2026-10-29T23:11:32Z.  
**Reviewer:** `/root/o2_review`, continuing the independent review context.  
**Current verdict:** **Ready to present for Q-7 operator ratification.** O2-01 and O2-02 are closed against the exact v0.2 digest below. No new blocking finding was established in the bounded delta review. This supersedes the earlier readiness recommendation for v0.1; every original finding and paragraph above remains unchanged. This is not human ratification, formal method/ADR admission, implementation/build readiness, execution funding or a claim that any future acceptance test has run.

### Reviewed delta and identities

I compared the entire v0.1→v0.2 textual diff, read the affected surrounding sections and acceptance/registry/OL hooks, and checked the relevant original source predicates. The earlier draft copy supplied for comparison exactly matched the originally reviewed digest. No authoring conversation was used. The only repository mutation in this rereview is this appended section.

| Input | SHA-256 |
|---|---|
| Earlier draft v0.1 | `df924757b5dd3853c398e3149e2fda630113bc185cab5d4b5ab8a64c96812ed9` |
| [Reviewed draft v0.2](../evidence-and-adoption-spec.md) | `914ea2fe1b9dbbd5b37ab3a6a35987c72f40f9f159ee52fd0520b982c09f648c` |
| Review note before this append | `12cdc620fd7c637ec872d8ba0b0d572cb22a5ebfc4949c8e7eed70c2a3625860` |
| [Research ledger](../research-ledger.md) at rereview | `92e51c87c8199f2988d25b63a7f6d0f7670564f2a91888cb7f17f3f738264a82` |
| [Outline trace](../OUTLINE_TRACE.md) at rereview | `5f2b0378cb8a0cc7e91fe37e099e0913ad7a6beb688898f21e0812ebe0e05977` |

All 22 accepted-note hashes still match the unchanged Appendix C source manifest. The review does not authorize changing those source notes to make the reconciliation appear retrospective.

### Finding closure

**O2-01 — closed; coordinator's adapt disposition implemented.** S12 in §0.3 and §1.4 explicitly replaces only the retained `REQ-B-102` round-trip predicate, while preserving the original source row. [AC-1-2](../evidence-and-adoption-spec.md#ac-1-2) now gives the requested counterexample an unambiguous result: a real native artifact exported to the selected foreign format and accepted by the pinned reader can pass the bounded exercise without a return import. Preserved-field/loss inspection and unsupported direction/version controls remain required. The rule expressly withholds reverse/round-trip/general interoperability claims, and an unselected S6 earns no compatibility claim. The registry links the adapted acceptance, C-007 consumes it, and affected OL hooks agree with the trace. The one-way scope is consistent with the approved C2/H2 optional exercise; no second conversion direction or new product surface has been smuggled into acceptance.

**O2-02 — closed; coordinator's accept disposition implemented.** `REQ-B-107`, `REQ-G-001`, `REQ-G-003` and `REQ-G-005` now each link §8 participant evidence/AC-8-2 and have a P-bearing layer. The explicit shared dependency in §8.4 requires pre-session per-ID/branch observation coverage and identifies shared evidence, assistance, outcomes and unmet predicates. It expressly prevents a passing technical slice from closing the participant predicate. Therefore, the original counterexample—passing guide/receipt/CI checks with no formative observations—leaves these predicates unmet. The same recorded observations can later serve the named consumers without duplicate cohorts or extra primary owners.

The analogous A-201/C-007 routing is consistent with their retained first-use/report predicates and existing E/P or D/E/P labels. Preparing a precise sanitized example report makes C-007's existing reader-report predicate observable; actual submission remains optional, so it grants no outreach authority. It remains within the same consented cohort/session ceiling, with unfinished branches visible. B-001's change to D-only correctly matches its original portfolio-choice/document acceptance; the draft still labels demand as a hypothesis and retains empirical claims under participant evidence. No additional requirement identity, study cohort or automatic evidence upgrade results.

### Additional coordinator clarifications challenged

| Item | Rereview assessment and limits |
|---|---|
| C-01: method-specific constant data | Coherent correction. EX-02's generic constant-data refusal language previously conflicted with valid selected binary methods. For sufficient, otherwise admissible cells, all-success/all-failure does not make the Beta posterior or KL interval undefined. The new wording preserves positive uncertainty and still refuses genuinely undefined estimators. Section 3's “degenerate estimator” condition can now be read consistently as method-specific; constant observations are not automatically a degenerate estimator. One-task, missing-key, sample-floor, dependence and missing-row requirements remain in force. No new method, inference claim or favorable-result criterion was added. |
| C-02: numerical tranche versus total implementation budget | Coherent scoped clarification, not an additional budget. The unchanged 8-hour / 32-core-hour / 2-GiB amounts now explicitly govern D2's numerical/simulation tranche. The D2 source is a methods/protocol note; the separate example/operations/recipient/package envelopes already exist. Other build/provisioning/synthetic-workflow activity must obtain its own scoped authorization and prerequisites, with §7.3 limits where applicable. The added text does not authorize unquoted work or move numerical work outside its cap merely by naming it a package test. It creates no implementation-effort estimate, provider/task/container permission or automatic resource increase. |
| C-03: O8 detector acceptance | Coherent repair of a literal contradiction. The original D3 text required every invariant correct while deliberately seeding an invariant violation in O8. Section 5.2 now requires O1–O7's stated invariants and all three O8 duplicate detections, with no invariant-pass label for O8. The 8×3 design, reached/observable denominators, separate detected/3 count, failed-negative-control block and confinement/resource stops remain. A clean sink with no seeded duplicate cannot satisfy O8 merely because its invariant held. |

These clarifications are explicitly dispositioned in Appendix E. They preserve the source rows as history and clarify governing acceptance without modifying the frozen canonical contract. I found no additional design work needed before the Q-7 presentation from these deltas. The ordinary later ADR/method/resource gates continue to apply.

### Delta checks and remaining limits

- Deferrals-first rerun: exit 0, empty output; no deferral changed.
- Full textual diff reviewed against the verified v0.1 copy; v0.2 hash confirmed before appending.
- All 180 literal source rows are unchanged from v0.1 and match their accepted notes. Source/registry sets remain equal; dispositions remain 167 adopted / 12 merged / 1 superseded.
- All 22 accepted-source hashes/counts match; 219 explicit draft anchors are unique and internal references resolve. Draft local file-link check: 56 occurrences, 0 missing paths.
- All six named participant consumers carry AC-8-2 and a P-bearing layer; B-001 is D-only. All 24 OL texts and their AC-hook sets agree between the draft and OUTLINE_TRACE.
- Four bounded constant-data analytic assertion cases passed, 0 failed: Beta(1,6) and Beta(6,1) each have posterior variance `0.015306122448979591`; N=5 KL at η=.025 yields `[0, 0.5218237501049815]` for all failures and `[0.4781762498950185, 1]` for all successes. These illustrate C-01's retained uncertainty and are not an admitted report-path test.
- External planning validator rerun: 0 violations / 0 warnings; environmental locale fallback output persisted. Documentation checks and append-prefix verification are recorded in the delivery check below.

This was a delta rereview, not a repeated whole-spec audit. The original review's mathematical, source-inspection, engineering and empirical limits remain. The statistical formulas did not change, so I did not repeat the full earlier numerical diagnostics or run a simulation campaign. No Rust/runtime/example/backend/provider/benchmark execution, install, outreach, spend, commit or new agent occurred. Future numerical implementation, independence/applicability, exact endpoints/pins, original-runner feasibility, physical bounds, rights, quotes and participant results remain unvalidated and gated. **Q-7 may now be presented; only the operator can ratify the reviewed specification.**

**Delivery check — 2026-10-29T23:11:51Z:** `check-living-docs.sh` passed (387 living files, no personal paths); all 17 local links in this review note resolved, with 0 path/anchor failures; the `delta-rereview` anchor appears exactly once. The original 19,098-byte note prefix retains SHA-256 `12cdc620fd7c637ec872d8ba0b0d572cb22a5ebfc4949c8e7eed70c2a3625860`, confirming that every original paragraph is preserved. The final note digest is supplied in the review handoff.
