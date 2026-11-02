# 02_evidence-audit — What the existing receipts establish

Stream: A — Capability and evidence audit  
Researched: 2026-10-29 UTC  
Author: /root/a2_evidence  
Ledger rows covered: A2  
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; uncommitted approved planning/research documents, including accepted A1, were present. No implementation changes.  
Confidence overall: high for receipt inventory, transcript integrity and inspected assertions; medium for completeness of historical attempt recovery and native artifact availability.

Existing receipts support bounded live transport, runtime, operational and extension claims. They do not establish a controlled real-task efficacy result. Every current live transcript is available and hash-matches, but the current filenames retain only the latest receipt for each date/leg. Task outcomes, replay scope and client compatibility require qualifications that a top-level pass count conceals.

## Scope and method

Inspection occurred **2026-10-29T18:18:22Z–18:21:19Z**. `Retrieved` below names this inspection interval, not a historical execution time. All historical results were inspected only. Started with `bash scripts/deferrals-open.sh` (exit 0, empty); read root/docs instructions, approved brief/addendum, A2 row, conventions, outline trace and [accepted A1](01_capability-inventory.md).

Parsed every current `hh-live-evidence/1` JSON and all spend rows; checked transcript existence, Git tracking and SHA-256; scanned every transcript for outcome/failure/skip markers and read the material exceptions and their source assertions. Inspected bounded source, historical run notes, foreign answer files and launch addenda. No provider calls, live reruns, builds, Rust tests, foreign execution, uploads, source changes, commits or central-state edits occurred. This is not a reconstruction of every historical retry or an independent verification of provider billing.

## Receipt availability and claim matrix

| Evidence set | Availability / observed result | Claim supported; limit |
|---|---|---|
| Current [live directory](../build/reports/live/) | 34 JSON records; 34/34 tracked transcripts, 34/34 matching hashes; 142,610 transcript bytes total | Integrity of the **redacted transcript bytes**; not original raw responses or native event stores. |
| Current record statuses | 32 `pass`; one `fail:live-tests rc=2`; one `fail:live-tests rc=101`; zero top-level skipped records | Latest retained date/leg statuses only; neither attempt success rate nor task success rate. |
| [Spend ledger](../build/reports/live/SPEND.jsonl) | 90 entries, 34 unique receipt paths; 22 repeated paths; current receipts match the last timestamp for all 34 paths | Retry accounting exists; 56 earlier entries lack a distinct current-path receipt. Their outcomes cannot be inferred from the final record. |
| Ticket run-note references | 34 references to 16 distinct Markdown notes, all present; 0/34 current hashes match historical snapshots; 1/34 exists and matches at the receipt's `git_head` | Snapshot identity at emission, not an agent WAL head. Later append-only notes and code/evidence commit separation explain why current hashes need not match. Exact older snapshots were not exhaustively recovered. |
| Referenced log locations | Of 18 explicit full `docs/build/logs/` paths extracted from those 16 notes, 18 present, 16 tracked; two private OAuth files inspected for existence only | Bounded local availability inventory. Private local files are not distributed evidence; CI metadata is not a replay store. |
| Task/binding receipts | Local/OpenAI capstone, quickstart, SDK and client checks exist; six passing transcripts explicitly contain `format_failure` | Provider calls, lifecycle/transport, verification and selected replay mechanisms; no universal successful-task claim. |
| Judge receipts | Three current records: calibration, judge-run and capstone-judge; live model usage and verdict markers | Small labelled-set calibration and live judged-run plumbing; no benchmark efficacy estimate. |
| Bundle/replay receipts | Clean-store bundle receipt explicitly achieves **R0**; later effect-dispatch replay repair uses a scripted lane | Recorded scope-specific reproduction/replay; no fresh task rerun or R1–R3 award from this bundle. |
| Extension receipts | Three packaged variants have invocation markers: memory 10, compaction 3, tracker fixture 5 | Actual subprocess contract exercise, rather than merely a passing test that might early-return; no measured task benefit. |
| Foreign corpus | Committed E2/E3 answer files each cover eight arms; historical comparator reports 57/0 each | `foreign-run-verified` at corpus scope; no new replay was executed in A2. |

The live records span **2026-10-21T15:56:14Z–2026-10-23T08:31:17Z**, across **16 code revisions**; all referenced commits exist locally. The inventory is exhaustive for current JSONs in this directory, not for every run ever attempted. Receipt groups overlap in the matrix.

## Material findings

### F1 — Transcript integrity is strong within the recorded artifact boundary

Claim: All 34 current live records have committed, matching redacted transcripts, and the repository validator accepts their schema/accounting invariants.

Status: VERIFIED  
Evidence kind: source inspection and local read-only validation  
Evidence: [check-live-evidence.sh](../../scripts/check-live-evidence.sh), evidence-record and spend-correspondence loops; [ADR-0363 D2–D4](../adr/ADR-0363-r4.5a-live-executed-evidence-class-spend-ledger-and-live-verify-gating.md). Executed `bash scripts/check-live-evidence.sh` at the baseline: exit 0, `OK — 34 record(s), cumulative spend $25.2598, cap enforced`. Independent Python `hashlib.sha256` plus `git ls-files` checks: 34 present/tracked/matching, zero mismatches. The writer hashes `transcript.red` after canonical redaction, [live-verify.sh](../../scripts/live-verify.sh) `cmd_run`, lines 1574–1582.

Retrieved: 2026-10-29T18:18:22Z–18:21:19Z.  
Implication for the spec: Reuse these integrity/evidence conventions, while naming the artifact whose digest is verified.  
Outline delta: CONFIRMS OL-15, OL-16.  
Limitations: The validator checks shape, header allowlists, transcript hashes and spend constraints; it does not prove task correctness, raw-response identity, native run reproducibility or a scientifically valid analysis.

### F2 — Current receipt counts hide retry history

Claim: Date-plus-leg filenames preserve the last same-day receipt while spend rows preserve multiple attempts, so 32/34 cannot be reported as an experimental or operational success rate.

Status: VERIFIED  
Evidence kind: source inspection  
Evidence: [SPEND.jsonl](../build/reports/live/SPEND.jsonl) contains 90 entries and 34 distinct `record` values; 22 values recur. All 34 JSON `recorded_at` values equal their final spend-row timestamps. [live-verify.sh](../../scripts/live-verify.sh) lines 1622–1626 constructs `<date>-<leg>.json`, copies over that path/transcript, and appends the spend row. [check-live-evidence.sh](../../scripts/check-live-evidence.sh) checks correspondence with sets, not one-to-one attempt identities. Example: judge-run has eight spend entries on 2026-10-22 ending 06:11:40Z; only the last has its own current-path receipt. The [capstone report, Spend](../build/reports/2026-10-23-live-e2e-capstone.md) explicitly records repeated legs.

Retrieved: 2026-10-29T18:18:22Z–18:21:19Z.  
Implication for the spec: Assign immutable attempt IDs and retain status, failure, exclusion and supersession for every collection attempt.  
Outline delta: EXTENDS OL-10, OL-15, OL-16.  
Limitations: The 56 earlier entries may have additional historical artifacts elsewhere; A2 did not recover all Git versions/local logs. They are not 56 inferred failures. The $25.259768 cumulative ledger is an estimate, not a bill: 14 current records carry non-null usage; the failed Anthropic capstone records a $2 declared bound without usage. ADR-0363 makes provider billing authoritative; no console reconciliation was performed here.

### F3 — Receipt integrity does not imply availability of native replay artifacts

Claim: The receipts identify transcript bytes and historical ticket-note snapshots, but do not themselves package the native run stores needed for independent replay.

Status: PARTIALLY VERIFIED  
Evidence kind: source inspection  
Evidence: [live-verify.sh](../../scripts/live-verify.sh) `emit_record`, lines 1513–1539, hashes the working-tree `docs/build/runs/<ticket>.md` and records `git rev-parse HEAD`. Thus `run_ledger_head` is a ticket Markdown snapshot, as ADR-0363 D2 specifies, not the agent hash-chain head. All 16 notes exist now; none of the 34 references matches their current appended bytes. `git show <git_head>:<note>` finds a matching snapshot only for [oauth-device](../build/reports/live/2026-10-22-oauth-device.json); the other 33 note paths do not yet exist at their declared code heads. All 34 code commits resolve. None of these JSONs provides a native-store/bundle manifest locator field. [l4_6_live.rs](../../crates/hh-embed/tests/l4_6_live.rs) `l46_live_bundle_repro` writes its bundle and clean store under a temporary directory, then removes that directory.

Retrieved: 2026-10-29T18:18:22Z–18:21:19Z.  
Implication for the spec: Distribute selected native event prefixes, referenced blobs, definitions, manifests and analysis inputs under a content-addressed artifact index; identify a retrievable evidence snapshot separately from the code revision.  
Outline delta: EXTENDS OL-07, OL-16, OL-18.  
Limitations: Append-only edits and evidence committed after its code baseline are expected; these observations do **not** establish corruption or invalid evidence. Exact old note snapshots and native stores might survive elsewhere; A2 establishes the limited retrieval supplied by these records, not global nonexistence. Private OAuth artifacts were never opened.

### F4 — Infrastructure passes include explicitly unsuccessful task formats

Claim: The live capstone and quickstart passes prove their asserted infrastructure behavior, while several actual task terminals report `format_failure`.

Status: VERIFIED  
Evidence kind: recorded execution (inspected only) and source inspection  
Evidence: Six passing transcripts contain `format_failure`: [capstone-run-local](../build/reports/live/2026-10-23-capstone-run-local.transcript.txt), [capstone-run-openai](../build/reports/live/2026-10-23-capstone-run-openai.transcript.txt), [capstone-ledger-replay](../build/reports/live/2026-10-23-capstone-ledger-replay.transcript.txt), [capstone-web](../build/reports/live/2026-10-23-capstone-web.transcript.txt), [capstone-bundle-repro](../build/reports/live/2026-10-23-capstone-bundle-repro.transcript.txt) and [quickstart-live](../build/reports/live/2026-10-23-quickstart-live.transcript.txt). The first five were recorded 2026-10-23 03:09:50Z–03:22:13Z at `9412b2ba82e70eec4ca8484656e3860aff83ae1a`; quickstart at 05:18:40Z, `648f3e58600d4e117d1074bad0463b06782897e7`. [l4_6_live.rs](../../crates/hh-embed/tests/l4_6_live.rs) `REFERENCE_TASK` asks for `hh_submit` text “done”; `assert_live_run` checks model request/completion, usage, verification, replay verdict and unchanged model-row count, not an independent successful-task grade.

Retrieved: 2026-10-29T18:18:22Z–18:21:19Z.  
Implication for the spec: Show task terminal, submission correctness, infrastructure assertions and grader outcome as separate fields. Require independent correctness evidence for the useful-task example.  
Outline delta: CONFIRMS OL-05, OL-15; EXTENDS OL-06.  
Limitations: Six transcripts are not six independent task samples: each can contain multiple arms and some paths reuse the same reference-task shape. A1 correctly identified this risk; transcript inspection extends it to the OpenAI capstone. No model capability ranking follows.

### F5 — Live judged runs exist, but are not a benchmark comparison

Claim: The recorded judges were exercised against real providers and a real beneficiary run, but their calibrated toy submissions and replay assertions do not establish live benchmark efficacy.

Status: VERIFIED  
Evidence kind: recorded execution (inspected only) and source inspection  
Evidence: [judge-calibrate](../build/reports/live/2026-10-22-judge-calibrate.json), recorded 05:57:20Z at `2e187e578ab4527284d5aa5501be8fd14cb866d3`, reports 6/6 decided and agreement/consistency 1,000,000 ppm for both judges. [labelled_cases.json](../../crates/hh-embed/tests/fixtures/r4_7e/labelled_cases.json) has three recorded submission examples and three distractors, not an independent task campaign. [judge-run transcript](../build/reports/live/2026-10-22-judge-run.transcript.txt), recorded 06:11:40Z at that revision, shows Completed, five judged verdicts, two reconciliations and `replay_ppm=0`; [capstone-judge transcript](../build/reports/live/2026-10-23-capstone-judge.transcript.txt), recorded 03:23:02Z at `9412b2ba82e70eec4ca8484656e3860aff83ae1a`, shows five verdicts, one reconciliation and the same zero replay value. [r4_7e_live.rs](../../crates/hh-embed/tests/r4_7e_live.rs) lines 632–649 directs the beneficiary to submit `{"answer":"hello"}`; lines 718–750 require a recorded panel verdict and equality of replayed confidence, not a favorable score. The local judge shares the beneficiary family; the hosted judge supplies the different-family leg (lines 523–556).

Retrieved: 2026-10-29T18:18:22Z–18:21:19Z.  
Implication for the spec: Preserve this positive evidence of actual judge wiring; separately validate the selected executor, task artifacts, independent oracle, result production and controlled analysis.  
Outline delta: CONFIRMS OL-05, OL-09, OL-10, OL-20.  
Limitations: `replay_ppm=0` is a recorded confidence value, not zero replay error and not an independently derived benchmark failure rate. Six calibration cases do not establish general grader accuracy. Across this receipt inventory no controlled compaction efficacy comparison was identified; this is a scoped absence, not proof that none exists anywhere.

### F6 — Failures, skips and repairs must remain distinct observations

Claim: The retained failures and partial sublegs are meaningful evidence, and later repairs do not retroactively change their outcomes.

Status: VERIFIED  
Evidence kind: recorded execution (inspected only)  
Evidence: [GitHub failure transcript](../build/reports/live/2026-10-21-github-live.transcript.txt), recorded 2026-10-21T23:59:38Z at `248e54f345c0887a5abe4e189993f6f29d707eff`, shows `DuplicateDecision` and a plugin test returning “issues.json absent … skipped” while libtest counts it passed. [Next GitHub record](../build/reports/live/2026-10-22-github-live.json), 00:01:47Z at the same code head, records the live sandbox path, plugin invocation and cleanup passing. [Context adapters](../build/reports/live/2026-10-22-context-adapters.transcript.txt), 03:46:59Z at `fa169a5c748cf791f3b6b8d48e5509d30fcdbe4a`, counts four passing tests but explicitly skips local embeddings because that endpoint does not support embeddings; hosted embeddings, local summarization and Anthropic compaction have their own markers. [Anthropic capstone failure](../build/reports/live/2026-10-23-capstone-run-anthropic.transcript.txt), 03:21:40Z at `9412b2ba82e70eec4ca8484656e3860aff83ae1a`, reaches all three bindings then fails attached CLI-run replay with `replay_diverged`. [Later repair receipt](../build/reports/live/2026-10-23-l4-6b-replay.json), 04:28:56Z at `6e7cd8a09d35ed2c0adf482f2b1ac6e79a03b7ef`, explicitly uses a scripted lane and proves the repaired effect-dispatch shape with no new model/effect rows. [Status qualification](../status.md) and [ADR-0381](../adr/ADR-0381-l4.6b-deterministic-replay-re-mints-the-recorded-dispatch-ceremony.md) preserve that scope.

Retrieved: 2026-10-29T18:18:22Z–18:21:19Z.  
Implication for the spec: Record subleg outcomes and typed reasons; link a repair to the failure and state exactly what was rerun.  
Outline delta: CONFIRMS OL-12, OL-15, OL-21.  
Limitations: Neither a same-head later pass nor a scripted repair proves current production behavior. Probe `SKIPPED` capability verdicts also remain in the routing transcript; zero top-level skipped records does not mean zero skipped checks.

### F7 — Bundle reproduction is R0, and replay is not a fresh execution

Claim: The committed bundle transcript proves its declared clean-store R0 procedure, while deterministic replay receipts describe recorded-decision reuse rather than fresh task performance.

Status: VERIFIED  
Evidence kind: recorded execution (inspected only) and source inspection  
Evidence: [Bundle receipt](../build/reports/live/2026-10-23-capstone-bundle-repro.json), 2026-10-23T03:13:09Z at `9412b2ba82e70eec4ca8484656e3860aff83ae1a`, and its [transcript](../build/reports/live/2026-10-23-capstone-bundle-repro.transcript.txt) show `max_supported_level: R0`, achieved R0 and `verdict: reproduced`, despite the source task's format failure. [l4_6_live.rs](../../crates/hh-embed/tests/l4_6_live.rs) `l46_live_bundle_repro` chooses the reported maximum, passes a clean store, and accepts the reproduced verdict. Its `assert_live_run` and [L4.6b receipt](../build/reports/live/2026-10-23-l4-6b-replay.json) check replay mode and absence of newly minted model/effect rows. The stored receipt/transcript are not the exported bundle itself.

Retrieved: 2026-10-29T18:18:22Z–18:21:19Z.  
Implication for the spec: Define inspection, hash verification, analysis recomputation, deterministic replay and fresh execution as separately testable procedures, with artifact closure, environment and comparison tolerances.  
Outline delta: CONFIRMS OL-07, OL-16; EXTENDS OL-18.  
Limitations: A2 did not rerun any procedure. R0 is not proof of R1–R3, correctness, fresh model equivalence or zero overall resource cost. Higher-level supported mechanisms may exist; this particular receipt does not establish them.

### F8 — Surface and extension evidence has exact protocol and invocation boundaries

Claim: The receipts establish specific client and plugin paths, including compatibility failures that remain visible inside passing legs.

Status: VERIFIED  
Evidence kind: recorded execution (inspected only)  
Evidence: [Artifact MCP transcript](../build/reports/live/2026-10-23-capstone-mcp.transcript.txt), recorded 03:11:30Z at `9412b2ba82e70eec4ca8484656e3860aff83ae1a`, shows the official SDK refusing protocol `2026-07-28`; the in-repo client passes by observing `stage_pending` typed refusal for `l46.echo`. [Lab MCP transcript](../build/reports/live/2026-10-23-surface-mcp-lab.transcript.txt), recorded 08:24:02Z at `388704d81e40016f18eb53be3d72c525621c16aa`, also falls back from that SDK refusal but actually launches/submits a local run and replays it. [ACP transcript](../build/reports/live/2026-10-23-surface-acp.transcript.txt), 08:31:17Z at the same revision, uses the official SDK after numeric version 1 refusal and `v1-compat` negotiation; permission/replay assertions pass, while the SDK logs an invalid `session/update` notification and `updates=0`. At that revision, [memory](../build/reports/live/2026-10-23-plugin-memory-store.transcript.txt), [compaction](../build/reports/live/2026-10-23-plugin-compact-evict-oldest.transcript.txt) and [tracker fixture](../build/reports/live/2026-10-23-plugin-tracker-fixture.transcript.txt) records at 08:24:37Z–08:24:40Z show `subprocess_confined`, one bound row each and respectively 10/3/5 invocation rows.

Retrieved: 2026-10-29T18:18:22Z–18:21:19Z.  
Implication for the spec: Pin client versions/protocols and demonstrate required notifications as well as requests; use plugin invocation markers and named contracts instead of only aggregate tests.  
Outline delta: EXTENDS OL-13, OL-15, OL-17.  
Limitations: These are historical compatibility observations, not claims about current external SDKs. The tracker is a fixture adapter despite real subprocess execution. A1's ordinary-test early-return concern is valid, but these three explicit invocation receipts establish that their specific sublegs ran.

### F9 — Security, import and foreign conformance claims remain bounded

Claim: The operational and foreign receipts support their explicit checks without establishing broad security, task efficacy or alternate-runtime completeness.

Status: VERIFIED  
Evidence kind: recorded execution (inspected only) and source inspection  
Evidence: [LT-03](../build/reports/live/2026-10-21-lt03-live.json), 2026-10-21T23:15:10Z at `0c39584f65c56dc6b85cd100d17553f6432436eb`, records 16 `carried` rows across eight attack-shaped prompts and two provider families, plus sentinel/canary/sink checks. [r4_6_live.rs](../../crates/hh-embed/tests/r4_6_live.rs) lines 1–32 and 620–649 classify request-view placeholder carriage/leak detection, not full AgentDojo utility or attack success. [Manifest import](../build/reports/live/2026-10-21-bench-manifest-live.json), 23:12:19Z at the same revision, records 64 imported tasks, validation and stable IDs, not 64 solved tasks. [E2](../../foreign/e2-python/answers.json) and [E3](../../foreign/e3-typescript/answers.json) each contain eight arms and 66 answer entries; the comparator's **57 checks** are a distinct count. [R4.4b evidence log](../build/runs/R4.4b.md) records E3 57/0 with Node v25.2.1 during 2026-10-21T14:33Z–14:49:27Z, implementation `6919481`; [R4.16a evidence log](../build/runs/R4.16a.md) records the later E2 extension and E1 97/0, E2/E3 57/0 on 2026-10-23 (base `4c2fafc`, closed 14:22Z). The [current corpus manifest](../../crates/hh-xcheck/fixtures/cross-impl/bundle.json) retains 11 `environment-pending` cells with `needs: HUMAN-H3`, alongside `verified_by` evidence. Later Python extension notes supersede earlier six-arm absences.

Retrieved: 2026-10-29T18:18:22Z–18:21:19Z.  
Implication for the spec: State the checked boundary, denominator, corpus version and remaining prerequisites for every operational/conformance claim.  
Outline delta: CONFIRMS OL-12, OL-13, OL-15.  
Limitations: No foreign comparator or security trial was executed in A2. HIR cases reaching unported machinery remain outside the foreign implementations' coverage, as the [Python](../../foreign/e2-python/README.md) and [TypeScript](../../foreign/e3-typescript/README.md) guides explain. Manifest pending-cell vocabulary does not reopen discharged historical deferrals.

### F10 — Later launch evidence changes availability, not efficacy

Claim: The 2026-10-28 launch addendum supersedes pre-launch availability language without converting earlier receipts into broader task or production-readiness evidence.

Status: VERIFIED  
Evidence kind: recorded execution (inspected only)  
Evidence: [Launch record](../build/reports/2026-10-28-launch-record.md) reports 49/49 publishable crates verified at 0.1.0 on 2026-10-28, tag `v0.1.0` at `d1325a63bab7bf0836250d9df871e2bfdd76dea2`, release assets and launch controls. [Status, 2026-10-28 update](../status.md) awards `public` to the named published/release/control surfaces and expressly withholds a production-readiness rating. These later records must accompany earlier live reports and historical “public unearned” statements.

Retrieved: 2026-10-29T18:18:22Z–18:21:19Z.  
Implication for the spec: Date claims and separate distribution/launch status from experimental and operational evidence.  
Outline delta: CONFIRMS OL-02, OL-15, OL-21.  
Limitations: This is inspection of the launch record, not a new remote release/API check or a new launch decision.

## Missing evidence for the first-wave portfolio

| Candidate | What the existing evidence can supply | Additional evidence needed before the promised claim |
|---|---|---|
| Useful end-to-end task | Live calls, submission/judge wiring, terminal and trace mechanisms | A useful selected task, preserved output, independently executed grader and failure cases; terminal success distinct from API success. |
| Inspect/replay/share | Hash-matched transcripts; selected replay and clean-store R0 records | Redistributable native bundle closure; annotated real task trace; exact replay/recompute commands independently run on the distributed artifact; explicit nondeterminism. |
| Offline Lab tutorial | Existing fixture capability from A1; this audit does not add a live comparison | Executed, checked tutorial commands and result/analysis artifacts clearly labelled as teaching fixtures. |
| Live compaction comparison | Real compaction adapter call and real judges, separately evidenced | E1 executor-to-grader integration; task/control/treatment pins; matched accounting; immutable attempts; pilot/confirmation separation; repetitions, uncertainty and executable analysis. |
| Recovery/authorization trials | Bounded helper, leak/veto and webhook receipts | Explicit fault/attack matrix, recovery/task outcomes, denied/unsupported cases and observable durable invariants; no extrapolation from 16 carriage checks. |
| Extension/replication contribution | Actual three-variant subprocess invocations; eight-arm foreign corpus | Reproducible package prerequisites, external contribution task, exact compatible client/ABI versions, clean environment reproduction and tamper/refusal cases. |

## Answer and completion assessment

A2 is complete at authorized receipt-audit scope. Transcript integrity and current record inventory are settled; historical retry recovery, native-store availability elsewhere and scientific integration remain bounded uncertainties rather than reasons to invent an efficacy conclusion. Existing records make a credible evidence tour possible **if each claim remains scoped**. They do not yet supply the selected useful-task/controlled-study package the first wave promises. No deferral or launch state was changed.

## Open questions

| Question | Owner / affected units | Operator decision now? |
|---|---|---|
| Which chosen artifacts can be distributed with native run/blob closure, and what privacy/licensing exclusions remain? | F1/F3; C2, G1 | No for design; publication/resource approval later. |
| Which executor and independent grader generate genuine study rows? | E1/E2; C2, D1/D2 | No; portfolio and later resource review own selection. |
| Which old attempts merit targeted recovery rather than a newly designed pilot? | F1 and coordinator; D1, F3 | No exhaustive recovery required by A2. |
| Which MCP/ACP client behavior is mandatory for the selected tutorial? | E1/F2; C1, G1 | Portfolio choice at checkpoint; no current SDK recheck authorized here. |

## Spec requirements emitted

Candidate supplemental requirements only; no frozen-contract amendment is made.

| ID | Candidate requirement / rationale | Findings / outline | Proposed acceptance |
|---|---|---|---|
| REQ-A-101 | Each collection attempt has an immutable ID, timestamp, code/config revision, outcome, exclusion and supersession; retries never replace earlier observations. Preserve existing evidence and use the ADR/schema workflow for any receipt-contract change. | F2, F6; OL-10, OL-15, OL-16 | Two same-day retries produce distinct retained receipts; totals reconcile; all failures/skips remain queryable. |
| REQ-A-102 | A claim index separates infrastructure assertion, task terminal, independent grade, analysis conclusion, evidence class and checked denominator. | F1, F4–F6, F9; OL-05, OL-12, OL-15 | Format-failure/pass and early-return-skip fixtures render honestly; published claims resolve to exact observations. |
| REQ-A-103 | Distributed receipts enumerate artifact roles and hashes, redaction/access treatment, native event/blob closure and retrievable evidence snapshot independently of code HEAD. | F1, F3, F7; OL-07, OL-16, OL-18 | A clean recipient verifies selected artifacts and completes every advertised procedure; missing/private artifacts produce explicit limits. |
| REQ-A-104 | Useful-task and study evidence preserves actual submission, independently executed verifier result and task-level accounting; judge calibration discloses cases and limitations. | F4, F5, F9; OL-05, OL-09, OL-10, OL-20 | One pilot traverses executor→submission→grader→result→analysis with inspected artifacts and deliberate incorrect-output cases. |
| REQ-A-105 | Each replay/recompute/rerun procedure declares its level, inputs, comparisons, provider-call behavior and resource assumptions; repaired scope is linked without relabelling old failures. | F6, F7; OL-07, OL-15, OL-16 | Independently exercise each promised procedure, including effect dispatch and negative/tampered cases. |
| REQ-A-106 | Client and extension receipts record actual versions, negotiation, invocation/subleg outcomes and callback/notification failures; foreign claims declare corpus and missing coverage. | F8, F9; OL-13, OL-15, OL-17 | Chosen client completes required requests and updates; plugin markers prove invocation; comparator reports omitted cases explicitly. |

## Handoff and verification

A1's F8/F9/F12 leads are resolved at deeper receipt scope; A2 confirms them with the qualifications above. New material risks are same-day attempt overwrites, missing native-artifact closure, additional format failures and passing client legs containing compatibility errors. C1/C2 should use the portfolio gap table; E1 should preserve the real judge/driver evidence while identifying the missing complete study bridge; F1/F3 own attempt identity and artifact packaging. No portfolio decision is made here; no conflict with accepted A1 is identified.

Executed checks: zero-open-deferrals query passed; `bash scripts/check-live-evidence.sh` passed (34 records); Python inventory/hash/tracking checks passed (34/34 matching transcripts), with snapshot/retry exceptions reported rather than treated as failures of that validator. **0 Rust tests, 0 new live legs, 0 foreign reruns.** Read-only locator searches included corrected nonexistent-path lookups; those were not checks or evidence passes. The note was written with Python and its added-file diff inspected (`git diff --no-index`, expected exit 1). All 54 relative links resolve; all 10 findings carry the eight required fields. `bash scripts/check-living-docs.sh` passed: 387 living files scanned, no personal paths. Coordinator owns acceptance and central-state updates.
