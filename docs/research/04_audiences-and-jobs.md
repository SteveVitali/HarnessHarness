# 04_audiences-and-jobs — Audiences, first-use jobs and adoption evidence

Stream: B — Audiences and ecosystem

Researched: 2026-10-29 UTC

Author: /root/b1_audiences

Ledger rows covered: B1

Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; uncommitted founding brief, research ledger, outline trace, note conventions and documentation-map addition inspected as planning inputs

Confidence overall: medium — high for the inspected documentation/source facts; low for external demand and usability predictions

## Scope and evidence boundary

B1 has no prerequisite row. This note recommends an audience and defines jobs,
barriers and measures for C1/C2 and G1–G3. It does not select the final portfolio,
prove implementation completeness or perform B2's ecosystem comparison.

The first command, `bash scripts/deferrals-open.sh`, exited 0 with empty output
on 2026-10-29; that is a zero-open-row result, not a new deferral closure.
`git rev-parse HEAD` confirmed the baseline above. Read-only inspection occurred
between clock observations `2026-10-29T18:08:45Z` and
`2026-10-29T18:09:37Z`; finding timestamps below identify inspection checkpoints,
not execution times of the underlying software. Only this note is authored by
this worker. Repository sources below refer to that baseline unless explicitly
identified as uncommitted planning inputs.

No interviews, user studies, external usage analytics, issue-population audit,
provider calls or fresh runtime experiments were performed. No external sources
were fetched: the bounded question here is product/audience fit to the existing
project and founding intent, while B2 owns adjacent-system research. An inspected
guide is evidence of what the project documents; it is not a fresh runtime test.
The one actual stakeholder signal used is the operator's recorded founding
intent and approval, not a representative sample of users.

## Findings

### F1 — The observed need is the operator's evidence-and-adoption objective

**Claim:** The founding brief requests a newcomer path from a useful task to inspectable evidence and a bounded contribution, while explicitly treating the audience choice as a hypothesis.

**Status:** VERIFIED

**Evidence kind:** source inspection

**Evidence:** Uncommitted [brief §1, §3 and approval addendum](../brief.md) asks “Can I use it?”, “What does the evidence establish?” and “Can I extend it?”, names researchers, agent developers and contributors as hypotheses, and records Q-1 approval; [ledger §0 T1/T4/T6](../research-ledger.md) labels usefulness, diagnosis and participation benefits as theses to test.

**Retrieved:** 2026-10-29T18:09:37Z inspection checkpoint.

**Implication for the spec:** Separate stakeholder goals, source-supported capability fit and external-user demand; do not convert the approved research direction into proof of adoption.

**Outline delta:** CONFIRMS OL-01 and OL-19 — audience and communication work must retain this distinction.

**Limitations:** This supports the operator's intent only. It establishes neither market size nor outsider preferences, willingness to migrate, or willingness to contribute.

### F2 — Current entry points favor technically prepared evaluators

**Claim:** The documented first encounter requires command-line setup and teaches a scripted run's lifecycle before asking the reader to configure a real model.

**Status:** VERIFIED

**Evidence kind:** source inspection

**Evidence:** [README, “Try an offline run” and “Connect a local model”](../../README.md) contains a source build and four JSON-RPC requests; [getting started §1–2 and local-model section](../getting-started.md) explains `hello`, `open_session`, `submit`, `read`, durable `run_id`, process-local `session_id`, and store location. [Fixture definition, `assembly.slots` and `nodes`](../../clients/ts/fixture-definition.json) contains context/control variants, agent, budget, rule and permission entities. [Getting started, release installation](../getting-started.md#install-from-a-release) documents a binary route but still requires the repository fixture for these examples.

**Retrieved:** 2026-10-29T18:09:37Z inspection checkpoint.

**Implication for the spec:** Specify a technical prerequisite profile and a concrete useful-task outcome beyond successful lifecycle setup; measure setup separately from task use.

**Outline delta:** EXTENDS OL-01 and OL-05 — readiness and value are different milestones.

**Limitations:** No newcomer attempted the commands in this row. Setup burden and time-to-value are predictions; release availability was not checked against a remote registry or release service.

### F3 — The project exposes research vocabulary without proving an efficacy result

**Claim:** The inspected Lab exemplar expresses a controlled compaction comparison as a parameterized experiment document, rather than supplying evidence that one policy improves real tasks.

**Status:** VERIFIED

**Evidence kind:** source inspection

**Evidence:** [Architecture, “The Lab”](../architecture.md#the-lab) documents experiments, evaluation and results. [Exemplars module, lines 1–78 and `compaction_family_v1`, lines 230–316](../../crates/hh-lab/src/exemplars.rs) explicitly describes a document shape with environment-dependent pins and constructs paired policy levels, fixed model/environment factors, task blocking, a hypothesis and primary metrics. The constructor's existence and fields were inspected; no experiment was executed. [Brief §4–5](../brief.md) requires a real controlled comparison and preserves negative or inconclusive outcomes.

**Retrieved:** 2026-10-29T18:09:37Z inspection checkpoint.

**Implication for the spec:** Anchor the research audience in one bounded policy decision, with task/grader execution feasibility owned by E1 and protocol/efficacy owned by D1–D2; do not inherit exemplar replicate counts or margins as a justified new study design.

**Outline delta:** CONFIRMS OL-08, OL-09 and OL-11 — a teaching example and a scientific result serve distinct jobs.

**Limitations:** This is not an audit of every Lab operation, a real-task integration proof, or a finding that researchers want this product. A1/A2/E1 must establish those technical seams.

### F4 — The documented adoption friction is concrete, but its user impact is unmeasured

**Claim:** The inspected guidance and CLI source identify specific prerequisites and semantic distinctions that a first-use design must explain or remove from the selected path.

**Status:** PARTIALLY VERIFIED

**Evidence kind:** source inspection

**Evidence:** [Reference, “CLI entry points”](../reference.md#cli-entry-points) distinguishes bare HIR from AssemblySource and command success from run outcome; [CLI `parse_args`, lines 350–362](../../crates/hh-cli/src/cli.rs) returns a short usage error for help; [getting started, “Troubleshooting”](../getting-started.md#troubleshooting) lists kernel path, fixture path, store/session ID and model endpoint problems. [Status, “Replay and client compatibility”](../status.md#replay-and-client-compatibility) documents `stage_pending` for ordinary artifact-server tool calls, protocol-version qualifications and the distinction between provider completion and task completion.

**Retrieved:** 2026-10-29T18:09:37Z inspection checkpoint.

**Implication for the spec:** A3/G1 should test these concrete failure points and attach recovery instructions to the chosen route; avoid a generic promise that every advertised interface supports the same job.

**Outline delta:** EXTENDS OL-01, OL-02 and OL-06 — teach the distinctions needed to make sense of an outcome.

**Limitations:** Their existence is documented or source-inspected; their frequency, severity and causal effect on abandonment are unknown. This note did not reproduce any failure or independently re-audit status-page live receipts.

| Potential barrier | What was observed | What still needs outside validation |
|---|---|---|
| Setup effort | Source route needs Rust/C tooling; binary route and fixture checkout are separately documented | Whether release installation or source setup is preferred, and which prerequisite causes abandonment |
| Finding the right operation | Short help; separate input shapes for authoring and execution | Whether readers discover the reference and can adapt one example without source inspection |
| Interpreting success | Transport, inspection, terminal run outcome and task correctness are distinct | Whether a reader can explain an unsuccessful run without mistaking it for a successful task |
| Connecting a model | Endpoint prefix, model availability and tool-call shape matter | Which providers/models the selected cohort actually has and the cost they accept |
| Trusting a comparison | Parameterized exemplar and evidence-class rules require context | Whether the receipt answers the reader's decision, and which missing observation blocks confidence |
| Integrating or extending | Multiple interfaces, negotiation and compatibility qualifications | Whether the integration benefit justifies changing an existing harness or toolchain |

The rightmost column is hypothesis work, not a finding that users have
experienced these barriers. A3 owns detailed journey reproduction; this table
is a test agenda rather than a request to redesign the entire product.

### F5 — Small contributions already have a route separate from build-ticket duties

**Claim:** The human contribution workflow admits bug reports, documentation and focused fixes without requiring every newcomer to operate the manifest build machinery.

**Status:** VERIFIED

**Evidence kind:** source inspection

**Evidence:** [Contributing, introduction, “Report a problem”, “Make and check a change” and manifest-ticket subsection](../../CONTRIBUTING.md) welcomes bounded work, specifies commands/environment/version and regression tests, and reserves extra records for assigned manifest tickets. [Bug-report form](../../.github/ISSUE_TEMPLATE/bug_report.yml) asks for reproduction, version, area and environment; [PR template](../../.github/pull_request_template.md) asks for summary and relevant verification. [Governance, “Authority” and “Decisions are ADRs”](../../GOVERNANCE.md) keeps merges and consequential decisions with the maintainer.

**Retrieved:** 2026-10-29T18:09:07Z inspection checkpoint for CONTRIBUTING/GOVERNANCE; 2026-10-29T18:09:37Z for forms.

**Implication for the spec:** Preserve a small first-contribution lane; use the full ADR/build workflow when the change actually needs it. Treat replication reports and well-specified failures as useful outcomes even when they do not produce a code PR.

**Outline delta:** CONFIRMS OL-18 and OL-21 — easier entry must retain authority and evidence rules.

**Limitations:** Published policy is not evidence of turnaround time, reviewer capacity, successful outsider contributions or a verified plugin tutorial. G2 must establish a concrete component path.

### F6 — A harness evaluator is the strongest provisional primary audience

**Claim:** The first wave should primarily serve a technical harness engineer or researcher deciding between two harness configurations on a bounded task set, with agent integrators as a secondary audience.

**Status:** UNVERIFIED

**Evidence kind:** design proposal

**Evidence:** F1's stated objective, F2's prerequisite profile and F3's run-plus-Lab affordances make this a coherent fit; [README “Use it to”](../../README.md) names inspection, comparison and integration. [Brief §3–5](../brief.md) already makes the live compaction comparison central. No user-demand source was collected.

**Retrieved:** 2026-10-29T18:09:37Z inspection checkpoint for the source basis.

**Implication for the spec:** Q-2 should choose one primary job and use secondary audiences to test transfer and contribution, rather than promise equal depth for every reader. Validate the recommendation through separately authorized outside observation.

**Outline delta:** EXTENDS OL-01, OL-05, OL-13 and OL-19 — prioritize by decision and prerequisite fit.

**Limitations:** This is a recommendation, not the operator's selection or demonstrated product-market fit. It may change if the usable task-to-grader path is too costly or outside participants identify a different urgent job.

| Audience hypothesis | Situation and job | First-use completion evidence | Main adoption risk | Proposed priority |
|---|---|---|---|---|
| Harness evaluator: engineer or researcher who can run shell tools and wants to test a context/control choice | “When changing one policy, help me compare useful task outcomes under a stated resource constraint and inspect why they differ.” | One checked task output; reader identifies run outcome and supporting events; follows a small comparison from inputs to observations and states its limits | Lab scaffolding may be mistaken for a ready scientific pipeline; meaningful tasks/graders may require additional integration | Primary: best alignment with runtime + Lab and the approved comparison objective |
| Agent integration developer with an existing app or workflow | “When deciding whether to embed this runtime, help me drive one run, read its durable outcome and understand the contract/cost of one integration.” | A minimal client on one supported binding obtains the returned IDs, reads the outcome and handles a documented refusal/error | Schema/setup work and migration cost may outweigh the benefit; MCP/catalog capability can be misunderstood | Secondary: reuse the same run and receipt through one supported interface; broad migration guides later |
| Independent replicator or skeptical technical reader | “When assessing a published claim, help me inspect the receipt and repeat the procedure appropriate to that claim.” | Distinguishes verification, recomputation, replay and fresh execution; records reproduction or a precise mismatch with version/environment | Missing or nonredistributable tasks/artifacts, provider drift and unaffordable reruns | Supporting first-wave role: essential to credible evidence; final replication scope belongs to F3/Q-6 |
| Component contributor or bug reporter | “When I find a problem or have a policy idea, help me submit the smallest reproducible improvement that can be reviewed.” | Reproducible issue/documentation fix, or one component change with the actual build/package steps and relevant acceptance checks | Too much historical/contract context before the first useful contribution; review capacity | Supporting first-wave role: start with diagnosis/docs, then one bounded extension after G2 validates it |
| Production fleet operator, general end-user or primarily visual/no-code user | Wants deployment assurances, a managed service or a turnkey agent for everyday work | Would require audience-specific product and operational acceptance outside this note | [Status](../status.md) explicitly does not award production readiness; inspected entry path assumes technical setup | Later or excluded from first-wave promises, pending C1 coverage disposition; not a claim that these people cannot use the software |

Roles can overlap; do not count the same person as several independent
adopters. “Researcher” denotes a job, not an academic title. A practitioner
making one measured policy decision is in the primary audience. Maintaining
the repository is a governance responsibility, not a separate demand segment.

A proposed first-use sequence is: complete and explain the offline lifecycle
check; run one useful task with an independently checkable artifact; inspect its
outcome and one consequential decision; change one supported parameter; then
follow the controlled-comparison example and its receipt. C1/E1 must choose and
verify the useful task. A terminal `succeeded` event alone is insufficient for
that milestone. If a live task is unavailable, label the fixture exercise as
teaching and leave the live milestone unmet; do not silently redefine value.

### F7 — The adoption funnel should measure useful decisions and participation

**Claim:** A staged funnel with observable task milestones is a better first measurement design than treating traffic, stars, installations or provider-call counts as adoption.

**Status:** UNVERIFIED

**Evidence kind:** design proposal

**Evidence:** F1 identifies useful work, interpretation and contribution as outcomes; F2/F4 expose distinct setup and comprehension stages; [brief §7](../brief.md) requires infrastructure, task, scientific and adoption claims to remain separate. [Maintenance evidence policy](../../MAINTAINING.md#evidence-classes), inspected lines 51–82, requires scoped receipts and honest skip labels. The metrics below are proposed, not instrumented or measured.

**Retrieved:** 2026-10-29T18:09:37Z inspection checkpoint for the source basis.

**Implication for the spec:** G3 should define a cohort, stage events, denominators and observation windows before claiming a conversion or improvement; collect the minimum authorized information needed to answer the selected question.

**Outline delta:** EXTENDS OL-18 and OL-19; CONFIRMS OL-15 — adoption cannot be inferred from operational evidence.

**Limitations:** No baseline, target conversion, time target, cohort size, analytics installation or recruitment is authorized or established here.

| Stage | Proposed observable success | Proposed measure and denominator | Interpretation limit |
|---|---|---|---|
| Discover and self-select | Reader can say what the project does, its relevant job and a material limitation | Correct explanation among sampled readers shown the entry page, with technical background recorded | Visitors/downloads alone do not demonstrate understanding; sampled readers are not all visitors |
| Become ready | Selected install route and doctor complete, or failure is diagnosable | Ready / all observed setup attempts; setup elapsed time and active effort separately | Report unavailable prerequisites and withdrawals; do not omit them from the initial attempt count |
| Perform a useful task | Independently checked task output and correct account of outcome | Task completion / all observed task attempts; grade and attempted cohort recorded | Separate fixture/live, command exit, run outcome and task grade; a self-report alone is weaker evidence |
| Inspect and adapt | Reader locates relevant evidence, changes one supported choice and explains what can be inferred | Successful inspection/adaptation / those attempting that stage; assistance and error categories | Re-running an unchanged example is not adaptation; a change in one run is not a causal efficacy result |
| Assess a comparison | Reader traces an observation to its receipt and correctly states one conclusion/limit | Correct interpretation / observed comparison-reading attempts | Scientific performance and reader comprehension remain separate measures |
| Reuse | Person applies the path to a second self-chosen task or returns for the same job | Voluntarily reported reuse / participants eligible for a defined follow-up window | State missing follow-up explicitly; a public artifact can corroborate but is not required proof of private use |
| Participate | A report is reproducible, a replication is documented, or a focused contribution meets review criteria | Qualified submissions / submissions; also submitters / eligible observed users; time to first maintainer response separately | Maintainer acceptance and review latency depend on capacity, not only contributor usability; a negative replication can qualify |

Use a branched funnel: a reader can become a useful bug reporter directly after
setup fails, and a replicator can enter through a receipt without adopting the
runtime. Report stage counts and paths instead of forcing everyone through one
linear conversion percentage. For each measured attempt retain the version,
route, OS/toolchain, prior familiarity, prerequisite availability, assistance,
outcome and failure reason where consent permits. Distinguish first attempts
from retries and automated checks from people. Report missing observations,
small-sample uncertainty and selection bias; do not extrapolate a recruited
technical cohort to all developers.

Initially, manual opt-in observation and a small voluntary follow-up form are
sufficient design candidates. G3 must determine the recording, consent,
retention and denominator policy before implementation; this proposal does not
authorize telemetry, outreach or the collection of private run contents. Time
and conversion targets should follow a baseline and Q-6 decisions, not invented
industry benchmarks. A usability improvement claim needs a comparison design;
a before/after count alone is confounded by cohort and version changes.

### F8 — Outside validation should test falsifiable jobs before expanding scope

**Claim:** A bounded outside-validation plan should test whether the proposed primary audience can complete and value the selected job, including reasons to reject adoption.

**Status:** UNVERIFIED

**Evidence kind:** design proposal

**Evidence:** [Ledger T1/T4/T6 and Q-2/Q-6](../research-ledger.md) preserve these questions; F1–F7 identify source-backed affordances and unmeasured user effects. No participants have been contacted and no responses are claimed.

**Retrieved:** 2026-10-29T18:09:37Z inspection checkpoint for the source basis.

**Implication for the spec:** G3 should propose an authorized evaluation with tasks and observation criteria before broad distribution, and carry disconfirming findings back into the portfolio.

**Outline delta:** EXTENDS OL-19 and OL-24 — outside validation is proposed work with an explicit authorization boundary.

**Limitations:** Participant availability and representativeness remain unknown; this note specifies questions, not completed research or a recruitment commitment.

Candidate observations for G3 are whether a technically prepared outsider can
choose the right start page, obtain useful output, diagnose an intentional or
naturally occurring failure, distinguish an observed result from a hypothesis,
and submit a minimal report. Ask what workflow they currently use and what
would make this path worth returning to. A self-reported preference does not
substitute for observed completion, and observed completion does not prove
continued demand. Include readers with an existing harness, not only people
already enthusiastic about this repository. Record “my current tool is enough”,
“setup exceeds the benefit”, and “the evidence cannot answer my decision” as
informative outcomes. Sample size and stopping rules belong to the approved
G3 evaluation design; do not claim saturation from an arbitrary small count.

## Answer and completion assessment

B1's question is answered at research/design scope. The concrete recommendation
is a primary harness evaluator with one bounded configuration decision,
secondary integration users, and supporting replication/contribution roles.
The note defines first-use jobs, source-grounded barriers, a branched adoption
and contribution funnel, and measures with explicit denominators and limits.
The operator's intent is observed; outside demand and usability are unverified.

No technical blocker prevents accepting this scoped note. Actual useful-task
feasibility awaits A1/A2/E1; outsider preference awaits separately authorized
validation. Those uncertainties prevent adoption or efficacy claims, not B1
research completion. The coordinator, rather than this worker, accepts the row.
No runtime/build tests were run or claimed; this is source-based research.

## Open questions

| Question | Owner | Affected units | Operator decision needed? |
|---|---|---|---|
| Select the harness evaluator as primary, and which single task/decision should define first value? | C1/C2 coordinator and operator | C1–C2, D2, G1 | Yes, existing Q-2 at the portfolio checkpoint; recommendation only here |
| Which documented frictions actually stop a newcomer, and what can the current composed execution path deliver? | A3 and E1 | C2, F2, G1 | No new decision for source/read-only audits; bounded spikes use the existing approval boundary |
| What cohort, recruitment channel, sample/stopping rule, assistance policy and resource envelope should validate this choice? | G3 and operator | G3, H2 | Yes, existing Q-6; outreach remains unauthorized |
| What usable baseline and observation window justify time/conversion targets, and how will nonresponse be reported? | G3 | G3, H2 | Resolve in Q-6 package; no fabricated numeric target in B1 |
| Which one extension and which replication procedure can outsiders complete with practical prerequisites? | G2 and F3 | C2, F3, G2 | Q-2/Q-6 select scope after technical feasibility; do not promise every component route |

## Spec requirements emitted

These are candidate obligations for later synthesis, not changes to the frozen
canonical specification, existing IDs or implementation tickets.

| ID | Candidate requirement and rationale | Findings / outline | Proposed acceptance method |
|---|---|---|---|
| REQ-B-001 | State one selected primary audience, concrete decision/job, prerequisites, secondary roles and deliberate first-wave exclusions; label inferred demand and the evidence used to choose it. | F1–F3, F6; OL-01, OL-19 | Portfolio review records Q-2 choice and maps each selected demonstration to a job; no external-user claim without a source |
| REQ-B-002 | Define first value using an independently checked useful task output plus correct interpretation of its outcome; separate setup/lifecycle success and fixture teaching from that milestone. | F2–F4, F6; OL-05, OL-08, OL-15 | G1/F2 task rubric includes expected output/check, run outcome, evidence interpretation and explicit fixture/live labels; later executable acceptance belongs to the selected task |
| REQ-B-003 | Give the chosen first-use route exact prerequisites and recovery guidance for its relevant command, definition, identifier, store and provider failure cases. | F2, F4; OL-01, OL-06, OL-17 | A3/G1 trace each selected-route step and failure to the explanation or a scoped missing-behavior obligation; no requirement to repair every interface |
| REQ-B-004 | Provide a small contribution lane with reproducible evidence, applicable checks, privacy guidance and review expectations, separate from assigned manifest-ticket responsibilities. | F5–F6; OL-18, OL-21 | G2 walks one bug/documentation submission and one selected extension through the actual policy and packaging steps; maintainer authority and ADR rules remain explicit |
| REQ-B-005 | Define adoption stages with task-based success events, denominators, observation windows, retry/assistance handling, missing observations and branching participation paths. | F7; OL-15, OL-19 | G3 measurement dictionary can classify example success, failure, skipped prerequisite, duplicate/retry and missing-follow-up records without silently dropping them |
| REQ-B-006 | Keep adoption, understanding, scientific efficacy and operational evidence claims separate; publish the scope and uncertainty of each measured result. | F1, F3, F7; OL-10, OL-11, OL-15, OL-19 | Review rejects conversion/utility claims inferred solely from traffic, stars, provider completion, fixture passes or release availability; any causal adoption claim has a stated comparison design |
| REQ-B-007 | Propose outside validation around the chosen job and disconfirming outcomes; require an explicit scope/collection decision before recruitment or telemetry and retain unmet-user-need findings. | F6–F8; OL-19, OL-20, OL-24 | Q-6/G3 package names cohort, tasks, observations, costs, consent/privacy and stopping rule; records no outreach or collection as completed without evidence |

## Handoff

B1 had no accepted prerequisites to discharge. C1 can use F6's audience/job
matrix to score candidates; C2 carries the audience decision to Q-2. A3 tests
F4's friction agenda, E1 checks the useful-task seam, G1 designs the chosen
journey, G2 preserves small contribution paths, and G3 finalizes validation and
measurement. B2 independently owns external comparisons and integration
opportunities; B1 does not discharge OL-04's external-research portion.

No new ledger row is required now: the existing A3/E1/G1–G3 rows own the open
work. No conflict with another accepted note was assessed because no other
accepted B1 dependency exists. The provisional later/excluded audience choices
above are inputs to C1, not final capability-family dispositions or changes to
the charter. No central state, outline, source, build record or gate was edited.
