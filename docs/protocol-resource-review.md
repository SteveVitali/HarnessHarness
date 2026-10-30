# HarnessHarness evidence and adoption — protocol/resource review

**Ready for the operator checkpoint.** Twenty of the original 24 research/design units are accepted. Q-3–Q-6 remain unanswered. Review the recommendations below before H2 reconciliation and supplemental-spec synthesis; the proposed D4 follow-up has not been assigned.

## Recommended direction

Build one useful-task evidence path, then use it for explanation, controlled experiments, operational demonstrations and contributions. This requires runtime integration alongside examples and documentation. The source audit found gaps between individually implemented mechanisms and the composed workflows needed for credible public claims.

Keep the approved six-scenario portfolio: a useful task with trace/receipt, an offline Lab lesson, the real compaction comparison, bounded operational proofs, a bug→verifyfixed contribution, and at most one optional consumer. The plugin and consumer slices remain contingent. All 22 capability-family dispositions and nine later investigation groups remain in the [approved portfolio](design/06_demo-and-study-portfolio.md); this first wave does not claim to prove every capability.

The implementation shape remains the eight packages approved for further design. The [integration design](design/14_integration-design.md) now identifies their owners, interfaces, ordering and acceptance:

- **P1/P2/P7:** adopt the actual launched subject, execute effects once through the existing dispatcher, seal its submission, grade independently, and enforce real resource accounts/deadlines.
- **P3/P4/P5:** preserve immutable receipts, repair R0, provide checked recipes and the offline Lab lesson, then exercise the common operational apparatus.
- **P6/P8:** prove that native compaction changes the dispatched model request; implement the admitted detector/analysis methods and study prerequisites.

These are work packages, not final tickets or an effort estimate. The earlier 8–16-slice illustration remains nonbinding. No new backend, broad public API, statistics platform or documentation platform is included by implication.

## Decisions reached by the design work

**Grade after subject completion.** The subject drains effects and seals its submission before its actual terminal event. The independent oracle then runs under an instrument budget. An explicit pending state blocks scored settlement and analysis until a valid grade/disposition exists. Regrades preserve history and do not create new subjects or recharge the original execution. This resolves the earlier provisional ordering while preserving the frozen created→finished wall-time definition. The driver also needs a correction from turn-start to run-created timing. See [E3](design/14_integration-design.md).

**Check the common backend early.** The existing HelperExecutor/local-container/Podman path is a candidate. Its suitability for unchanged Terminal-Bench tasks and the operational sink is not proved. P2 begins with that admission check; a new backend would be a material scope decision.

**Use semantic checks and independently observed effects.** The [operational design](design/11_operational-proof-protocols.md) specifies eight cases with three repetitions each: allow, Kp4 crash/recovery, deny, permission allow/deny, reservation refusal, exhaustion/drain, and a seeded duplicate that tests the detector. These 24 trials are engineering coverage, not a reliability estimate. A separate task demonstration retains two attempts and correct/wrong/broken-oracle controls. Negative and unavailable outcomes stay visible.

**Make the offline lesson informative.** The [example plan](design/15_examples-and-maintenance.md) uses two synthetic tasks × two arms × five repetitions, plus refusal controls. It teaches actual Lab orchestration and persisted analysis. Its synthetic results are never evidence of live task efficacy. Runnable docs will be linked to canonical executable recipes and semantic drift checks.

**Promise only the recipient procedure the artifacts support.** The [receipt contract](design/13_receipt-contract.md) extends the native bundle, preserving all planned slots, immutable attempts/grades, real native heads, raw observations, analysis inputs and access losses. [Recipient procedures](design/16_replication-and-artifacts.md) distinguish inspection, verification, recomputation, replay and fresh execution. A public analysis view can permit arithmetic recomputation while still withholding the inputs needed to check oracle truth.

## The statistical decision that needs care

The [compaction protocol](design/10_compaction-protocol.md) proposes one fixed provider/model-role setup, two native policies, six development tasks × two arms × five repetitions for a pilot, and—only after admission—30 held-out tasks × two arms × five repetitions for confirmation. The 60/300-subject counts are planning options, not power results, selected tasks or approved spending. The target is the selected fixed task manifest; it does not support a general benchmark-population claim.

The existing contract requires parity against the original runner before component-level headlines. D2's explicit conservative equivalence construction cannot establish the proposed ±10 percentage-point margin at five repetitions, even in its perfect-agreement examples. This exposes a limitation of that construction and sample design; it does **not** establish that efficient parity is impossible. Oracle agreement alone cannot substitute for runner parity.

**Recommendation: retain the central study and add a short independent methods review before locking its statistical tickets.** Proposed D4 would compare established alternatives, estimands, assumptions, attainable precision and resource needs, inspect any usable existing parity evidence, and recommend an admitted route or a deliberate limit on claims. It must also justify parity tolerance relative to the claimed effect: D2’s provisional ±10-point parity margin is larger than its proposed 5-point useful gain, so those values should not be ratified without a measurement-error argument. Its proposed output is `design/23_parity-method-review.md`; it has not been assigned or executed. Do not build an elaborate estimator or fund a predictably futile parity tranche merely because D2 supplied an explicit algorithm.

The useful-task, receipt and operational work can advance through their own gates while the study remains conditional. An inconclusive or negative study is publishable evidence. Missing parity or a broken measurement path is an admission failure and must be reported as such.

## Q-3–Q-6: recommended policies for review

| Decision | Recommendation | What remains a later binding gate |
|---|---|---|
| **Q-3: task/model/method** | Retain Terminal-Bench 2.0, unchanged upstream tasks/oracles, one environment family and one supported fixed provider/role set. Accept post-terminal grading and the limited finite-task target as design recommendations. Authorize proposed D4 before H2/statistical ticket lock. | Exact eligible manifest/splits, runner/image/oracle/model pins, common-backend suitability, cold-start and four-cap support, and reviewed method/parity admission. No silent suite/provider substitution. |
| **Q-4: resources and useful effects** | Keep execution authorization at **$0 / no new task-provider or cloud-runtime runs** during this planning phase. Treat 60/300 subjects and D2's per-run/aggregate caps as conditional design ceilings. Leave D2’s proposed task-gain/harm and parity values provisional pending D4; do not approve the 5-point/10-point pair by default. Authorize one fresh-context D4 note using source review and bounded analytic checks; no benchmark tasks, provider calls, installations or production implementation. | Separate activation, pilot, parity, confirmation and replication quotes with numeric currency ceilings, instrument/retry costs, CPU/storage/time limits and enforceable exposure. Register meaningful rate margins and precision requirements before collection. No unused prior allowance rolls forward. |
| **Q-5: artifacts and retention** | Public rights-cleared index/code/analysis packages; controlled originals in encrypted local staging. Optional GitHub Release assets and private S3 only after hosting approval. F3 proposes public reports for five years, assets two years, permitted controlled review originals 12 months; shorter rights obligations take precedence with loss records. | Item-level rights/contamination review, named owner/contact, region/host/price and quotas. First-review package caps do not cover the complete study archive; quote pilot/confirmation originals, backups and version overhead separately. |
| **Q-6: outside validation and distribution** | Start with one consenting outside clean-install verify/recompute report, then 4–6 formative evaluators with 60-minute sessions. Proposed total maintainer ceiling: 24 hours, including manual 30/90-day reviews. Use GitHub first; retain negative results and access limits. Fresh paid replication and wider promotion remain contingent. | Final participation/distribution plan, consent, actual recipient success, correction handling and explicit authorization before outreach or publication. Ordinary community users do not need our internal Q approvals to use the project. |

Approval at this checkpoint would select the planning policies and follow-up scope. It would not ratify the supplemental spec, start a build, authorize provider studies, create hosting accounts or send public communications. Exact execution choices can be required inputs to later admission rather than guessed now.

## Participation and build handoff

The [distribution plan](design/19_distribution-and-adoption.md) separates technical reproduction, newcomer comprehension, useful execution, contribution and later repeat use. It uses branch-specific counts and records setup, assistance, failures and missing follow-up. Stars and downloads remain visibility signals. No telemetry system or scheduled automation is proposed for creation now. Broader channels follow their current rules and an actual tryable artifact.

The [contribution design](design/18_contribution-path.md) offers startup reports without a run ID, semantic bug reports with permitted evidence, before/after verifyfixed reports, and bounded receipt review. Ordinary small PRs do not require contributors to operate this program’s internal Q register or build ledger. The optional existing compaction package exercise includes build/seal/load/invoke and refusal evidence; tests that return early for missing prerequisites cannot satisfy it.

The [handoff design](design/20_build-handoff-design.md) preserves the canonical spec pointer and historical requirement checks. A retained executable supplemental coverage/routing guard must check the new requirements, owners, acceptance criteria, first new ticket/base and later completion at the relevant boundaries. Native skill coordination invokes it directly; a one-iteration wrapper is only a conditional headless-driver integration. The existing full validator reports zero violations and eight historical warnings; their exact dispositions and strict checks on new records are part of readiness. New-round gate/readout identities must be distinct, so an old acceptance signature cannot close this work.

## Next handoff

After the checkpoint, incorporate exact operator rulings and any approved methods follow-up, then complete H2 reconciliation. Synthesize the supplemental specification, obtain a fresh-context adversarial review, and return for ratification. Only then translate it into implementation tickets and compatible build artifacts for a later `orchestrate-build` run. The frozen canonical spec and completed original build remain preserved; contract changes use explicit ADRs.

The [working ledger](research-ledger.md) records accepted notes, candidate requirements, dependencies and decisions. The [outline trace](OUTLINE_TRACE.md) keeps all founding obligations open until the supplemental specification covers them or the operator explicitly disposes of them.

## Verification and limits

This phase produced source-backed designs, not product capability receipts. E2 completed within its approved source-only option: zero runtime probes/builds or task/provider executions. No implementation, supplemental spec, ticket, publication or original-build state change was made. Normal Codex planning usage is separate from task-provider/cloud-runtime spending.

Planning validation and all four documentation/evidence checks passed. Direct checks covered 26 files, 913 relative links, 26 anchors, 147 complete findings, 170 unique candidate requirements and all 20 accepted-note hashes. The historical build validator reports zero violations and eight legacy warnings. These checks validate records, not runtime capability or study results. The review and supporting documents were opened in Zed; all files remain uncommitted.

## Approval record

2026-10-29T21:51:23Z — The operator confirmed “Looks good, proceed” in this conversation. This approves the policies and bounded methods follow-up above, under their stated limits. The research ledger records the exact scope and subsequent work; this reviewed text is preserved.
