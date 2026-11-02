# 20_build-handoff-design — Extend the build without reopening history

Stream: H — Synthesis readiness\
Researched: 2026-10-29 UTC\
Author: /root/h1_handoff\
Ledger rows covered: H1\
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; approved uncommitted planning and accepted dependencies.\
Confidence overall: high for workflow limits; medium for proposed handoff.

**Recommendation:** ratify a separate supplemental specification, then extend the numbered build chain with explicit supplemental coverage and round-specific acceptance. Preserve the frozen contract, candidate identities and operator history. This note approves no architecture, decomposition, execution or publication.

## Basis

Read [brief](../brief.md), [ledger/Q](../research-ledger.md), [conventions](../research/CONVENTIONS.md), [trace](../OUTLINE_TRACE.md), root/docs instructions, [C2](08_feasibility-and-scope.md), [D2](10_compaction-protocol.md), [F1](13_receipt-contract.md), accepted [E3](14_integration-design.md) and [F2](15_examples-and-maintenance.md). Inspected local `agent-skills` revision `e981ec57d487a2c41d416672483b5a4df15b1233`: synthesize-spec SKILL/run/synthesize/review/ratify; decompose-spec SKILL; build-memory SKILL/layout/ticket/tail templates/checker; orchestrate-build SKILL/driver. Below, skill paths are relative to that checkout; [maintainer setup](../../MAINTAINING.md#build-memory-tools) resolves it. UTC inspection checkpoints: **2026-10-29T21:07:11Z**, **21:08:27Z**, **21:09:00Z**, **21:13:19Z**, **21:16:26Z**. Remote state was not queried.

## Findings

### F1 — The completed build needs a deliberate extension boundary
Claim: Current state is completed Round 4; its next-ticket pointer cannot start the new program.
Status: VERIFIED\
Evidence kind: source inspection\
Evidence: [ledger](../build/LEDGER.md), CURRENT STATE/final R4.18 entries; [manifest](../tickets/00_MANIFEST.md), rows 165–167b; orchestrate-build §0 stops on DONE.\
Retrieved: 2026-10-29T21:13:19Z.\
Implication for the spec: Re-charter later without erasing DONE or old gates.\
Outline delta: CONFIRMS OL-21–OL-24.\
Limitations: Recorded launch/CI facts were inspected, not re-verified.

Head: `projectStatus: DONE`, `nextTicket: DONE`, `lastCompleted: R4.18`, `round: 4`, `chainTip: main` after stack collapse, `mergePolicy: OPERATOR`. Sequence ends at `167b_R4.18__launch-record.md`. Old dispatch/base/benchmark settings are historical, not study defaults. Deferrals-first emitted no owed rows.

After O3 and separate build-artifact authorization, re-read chain/default-branch SHA, open-stack CI, backlog, digest and decisions. If unchanged, extend **Round 5, sequence 168 onward**; neither is allocated now. Preserve old rows/files/signatures. Append the extension explanation; update CURRENT STATE only at the authorized transition, retaining its keyset. Replaced non-state head text is archived byte-for-byte under the living-archived rule. At extension use parser-valid current values, including `blockedOn: (none)` when no real block exists; the old explanatory value is not accepted by the pinned driver. Preserve its explanation in the dated transition record. Pin actual base/ancestry; new branches default to `codex/`. Chain PRs fork from and target the preceding ticket branch; only the operator merges.

### F2 — Supplemental requirements need a scoped coverage guard
Claim: The pinned checker reads one canonical spec and one pattern, so its existing coverage pass cannot certify supplemental requirements.
Status: VERIFIED\
Evidence kind: source inspection supporting design proposal\
Evidence: Manifest pattern `R-2\.[0-9]+\.[0-9]+[a-z]?`; ledger `canonicalSpec`; build-memory checker 648–651/1094–1230; synthesize mode requires verbatim candidate IDs.\
Retrieved: 2026-10-29T21:13:19Z.\
Implication for the spec: Retain old coverage and add explicit supplemental coverage/routing checks.\
Outline delta: EXTENDS OL-21/23; CONFIRMS OL-22.\
Limitations: No multi-spec capability or proposed guard is implemented.

Target `docs/evidence-and-adoption-spec.md`: §0 says **supplement**, never implicit replacement of `spec/CANONICAL_SPEC.md`. O3's canonical status means ratified within supplemental scope. Adopt behavioral sections by reference and enumerate amendment conflicts.

Keep `REQ-<STREAM>-<n>` candidates verbatim, including allocated hundreds; proposed supplemental ERE `REQ-[A-H]-[0-9]+`. No conversion into `R-2.*`. Each candidate gets adopted, merged-with(target), superseded-by(target), or deferred(reason/owner/trigger/withheld claim); preserve original note/alias. Trace **OL → note/finding/candidate → supplemental section/ID → legacy requirement/ADR relation → ticket or deferral → AC/layer → run/PR/native evidence**. Imported invariants are references, not duplicate work ownership. Every adopted ID has one primary ticket; consumers cite it. All 24 OL obligations and capability dispositions receive coverage or explicit operator rescope.

Keep current `canonicalSpec`, legacy pattern/index and external validator unchanged. Append a **Supplemental requirement coverage** section to new-round planning/manifest records, naming the supplemental source/pattern and literal owner table. Do not invent a machine-ledger key, assume Markdown links expand the spec, or enable `req_index_literal_from` on this mixed namespace.

Propose a small repo tool under `docs/build/tools/`, tentatively `evidence_handoff.py`, reading those existing records. It must check candidate/alias closure, exact in-scope coverage, one owner, resolvable AC/layer/evidence hooks, unique chain IDs/files, backward dependencies, skeleton refusal, full-tail role mapping and new-round acceptance. It checks that CURRENT STATE `nextTicket` is exactly the active round’s first unlanded row and that its forks-from/base agrees with the verified chain tip, using evidenced historical completion; it never owns or rewrites state. Include rejection fixtures for missing/duplicate IDs, wrong owner, old-gate routing, pending gate/skeleton and an old signature falsely satisfying new acceptance. Record versions/input digests/counts/diagnostics. Retain this executable and its fixtures as versioned build artifacts, required at decomposition validation and every relevant ticket/boundary. This is a separately authorized **tooling prerequisite**, reviewed and tested before product dispatch; decomposition itself remains document authoring. No external skills expansion or runtime dependency is needed.

The pinned headless driver reads `nextTicket` directly and has no supplemental hook. Therefore future integration must actually run this guard before/after every dispatch: a bounded repo wrapper may invoke the external driver with `--max-iters 1`, preserve its CI checks and interpret iteration-cap exit separately from success. A wrapper is conditional on choosing headless dispatch, not a replacement orchestrator; native subagent/manual coordination invokes the retained guard directly at boundaries. Test CI failure, cap/no-progress exits and resume without dropping existing gates. Verify the launch path with a stub worker: an incorrect old GATE-G1 pointer dispatches zero work; a correct first-new row dispatches one; pending/failed state stops. No unguarded accumulating loop is ready. Source references and operating clauses alone are insufficient.

### F3 — Design acceptance does not admit new runtime semantics
Claim: E3's packages are bounded inputs for ADR decisions and decomposition, not approved implementations.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in accepted notes/governance\
Evidence: E3 F1–F7; C2 F3–F5; [governance](../../GOVERNANCE.md#the-spec-and-the-frozen-corpus); manifest amendment protocol; F1/F2.\
Retrieved: 2026-10-29T21:07:11Z.\
Implication for the spec: Assign changed contracts an owner and admission gate.\
Outline delta: EXTENDS OL-05/09–12/15–17/20–24.\
Limitations: No ADR numbers, approved decisions or measured effort asserted.

| Proposed route | Packages / semantics | Acceptance obligation |
|---|---|---|
| Grade/launch/settlement ADR | P1/P2 adoption, seal, post-terminal grade, pending admission, selection/correction | Legacy default preserved; crash/duplicate/regrade tests; every consumer respects pending. |
| Effect/prepared-call/admission ADR | P2/P6/P7 Dispatcher, retained view→wire, vector reservation/derived caps | One authority/lifecycle/accounting; four dimensions, real usage and restart/unknown handling. |
| Closure/profile ADR | P3 native immutable attempt/grade/export identities, typed recipient closure | All-planned reconciliation, losses/access and clean-recipient negative controls. |
| Methods/detectors ADR | P8 finite-task inference, parity, rates/missingness/multiplicity | Independent method/Q-3/Q-4 disposition before implementation; validation ≠ approval. |
| Existing-contract fixes | M1 created→finished origin; R0 open/finished mapping; needed CLI requests | Contract regression; a discovered semantic change uses ADR instead. |
| Conditional boundary expansion | Public schema, backend, dependency or feature reach | Explicit scope/ADR; owning schema→codegen/all outputs; tier-off refusals/removability. |

O1's amendment register names exact sections, before/after semantics, IDs, compatibility, alternatives, approver and landing owner. O3 ratifies reviewed scope; actual frozen-text amendments land through ADR/manifest protocol. New event/kind/domain has one codec/home/class-table authority. ADRs need revisit triggers and generated indexes. Post-ratification changes append version/delta/provenance; executed contracts receive dated amendment notes. Never conform frozen prose merely to implementation.

Preserve E3's graph: decisions→P1 interfaces→{P2,P7}; those→{P3,P6}; P3→P4; {P2,P3,P7}→P5; {P3,P6,P7,method/parity}→P8. P7 supports tasks/operations too. Split by coupling, owners, regression scope and fresh-context review headroom. P1–P8 are packages, not tickets; C2's 8–16 slices are illustrative, with no ticket cap or invented days. Backend admission may precede implementation. New scope returns to the operator; routine splitting preserves scope.

### F4 — The new chain and tail need unique identities and signatures
Claim: An append-only extension can retain full closeout rigor without reusing old ticket/gate identities.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in workflow inspection\
Evidence: decompose-spec Phases 2–5; build-memory layout §§5–7/history; ticket/GATE-ACCEPT templates; orchestrate-build §§2–3; [CONTRIBUTING](../../CONTRIBUTING.md#open-a-pull-request), manifest-ticket subsection, [CODEOWNERS](../../CODEOWNERS) comment and [PR template](../../.github/pull_request_template.md).\
Retrieved: 2026-10-29T21:16:26Z.\
Implication for the spec: Require new contracts and round-specific acceptance.\
Outline delta: EXTENDS OL-21–OL-24.\
Limitations: Final IDs, sizes and dispatch target await decomposition.

Later proposal: `decompose-spec spec=docs/evidence-and-adoption-spec.md build_name=evidence-and-adoption mode=extend tickets_dir=docs/tickets tail=full dispatch_target=<confirmed target>`. O3 writes the concrete invocation and binding constraints. Outputs: dependency/coupling map, sizing and fresh-context partition review/dispositions under `docs/build/planning/`; numbered chain banner; Plan extensions/amendment entries; full contracts/markers.

Contracts cite §§/IDs/current anchors, invariants/ownership, scope/exclusions, symbols/wire compatibility, numbered ACs/layers, negative tests, resource/stops, size/dispatch budget, Live stage, Production mutations, operating clauses and Run line. Skeletons have no Run line. HUMAN rows identify owner/trigger/verification/withheld claims. Ordinary external docs/test contributors need contribution instructions and maintainer review, not an internal ledger/Q ritual; maintainers map accepted program work into its chain. Reconcile conflicting living guidance in a later owned docs contract, without altering governance now.

Full-tail roles: CAP.1 independent gap hunt; CAP.2 composed verification; CAP.3 closure; operator acceptance; REC.1 backlog/readiness; REC.2 spec reconciliation; REC.3 integration plan; DOC.1 reader docs; DOC.2 agent docs. Map them to unused grammar-valid IDs (e.g. `CAP5.1`, `REC5.1`, `DOC5.1`) and fresh `GATE-G<next>`, recording GATE-ACCEPT role equivalence. Do not rebind existing files. New readout/signature must satisfy the scoped guard; finding the old signed `GATE-ACCEPT.md` is insufficient. Append new-round acceptance references to historical readouts without changing old signatures.

Gates cover architecture/method admission, task/environment/provider/resources, frozen protocol/quote before confirmation, rights/access before distribution and final acceptance. Pending decision→RETURN PASS; red verification/missing technical prerequisite→`blockedOn`. Record operator words, actual UTC/channel, exact items/expiry/void conditions and required agent-draft hash confirmation. Protocol/budget approval grants only what its words authorize; agents never sign.

Full tail is justified by composed execution/public receipts. REC.1 reads current artifact access/claimed service state; REC.2 landed code/ADRs/coverage; REC.3 actual stack CI/branches/release conditions. These are read obligations, not new collection or production authority. Remove unsupported current claims. Preserve historical reports; append or create dated round reports rather than overwrite past coverage/readiness snapshots.

### F5 — Eight old warnings require scoped compatibility
Claim: Fresh validation has zero violations and the same eight warning diagnostics; this does not permit new malformed records.
Status: VERIFIED\
Evidence kind: recorded execution; source inspection\
Evidence: `bash <AGENT_SKILLS_ROOT>/skills/build-memory/scripts/check-build-memory.sh .`, macOS root, baseline/tool revisions above, **2026-10-29T21:07:53Z**, exit 0; JSON `build-memory-check/2`, digest `025a0c830cb543cbb107313487760180fbc2a9bbd6d47053cf644ec20f30247a`, temporary basename `build-memory-check.8l3UEz6a`; BM-COMPAT-06/history rules.\
Retrieved: 2026-10-29T21:08:27Z.\
Implication for the spec: Rebaseline later; require clean new rows and disposition every diagnostic change.\
Outline delta: CONFIRMS OL-21/22; EXTENDS OL-23/24.\
Limitations: Build-memory evidence only; report dirty flag does not cover every untracked planning file.

| Warning | Proposed treatment |
|---|---|
| 108 unbannered chain rows | Preserve old rows; new numbered banner mandatory. |
| Spec ADR appendix/file-set mismatch | Preserve frozen appendix; new ADR/index/trigger checks mandatory; correction needs admitted amendment. |
| DONE versus lowest unlanded GATE-G1 | Preserve history; verify recorded PASSED disposition and completed-chain evidence before active-round routing. |
| 142 oversized PHASE LOG entries | Preserve old region; append new round region with ≤2 KiB fixed-shape entries linking evidence. |
| Two BUILD_INDEX column mismatches | Preserve; new round header/rows correct and escaped. |
| Two old verification vocabulary values | Preserve; new index template enum plus truthful native evidence class. |
| Four old readouts without guard | Preserve; all new readouts include exact guard/operator provenance. |
| 82 run ledgers without Harness | Preserve; all new headers include Harness/model/tier, Skills, Started/Closed/CI. |

For G1, [ledger](../build/LEDGER.md) GATE DECISIONS records the second 2026-09-15 disposition **PASSED — accept ADR-0226**; R4.18 records chain completion. The generic lowest-unlanded parser uses PHASE LOG/index tokens, not that decision table. Its warning must not select work. The proposed guard pins the historical boundary SHA and requires explicit evidence pointers for completed legacy gates plus the active-round first-unfinished calculation. Reject a pointer to an old gate, unexplained old residual, missing evidence or a pending new gate treated as complete. Do not invent closure, add an `unused` token to G1 or fabricate a landing. Recheck exact source records when implementing this compatibility rule.

Pin diagnostics by check/path/historical rows, never “allow eight warnings.” Keep new violations/unknowns fatal; use history and supplemental guard checks. Do not enable global guards solely to obtain a strict label while old incompatible regions remain. An actual routing/checker incompatibility needs a reviewed narrow remedy, not relaxed checks or rewritten history. These dispositions are proposals, not waivers.

### F6 — Readiness separates implementation, research and publication
Claim: Build readiness needs a ratified contract and executable validation, while research completion need not show a favorable effect.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in workflow/accepted notes\
Evidence: Brief §§7–8; F1 claims/procedures; F2 F3–F4; E3 F6–F7; decompose Phase 4; orchestrate close/CI/dispatch; templates.\
Retrieved: 2026-10-29T21:09:00Z.\
Implication for the spec: Carry explicit layers and unearned claims through closure.\
Outline delta: EXTENDS OL-11/15–17/20–24.\
Limitations: Future checks below are not executed product verification.

Packet: ratified spec/version/digest; frozen note index/trace/Q decisions; O2 and partition-review dispositions; ADR ownership; supplemental coverage/routing report and tested guard; manifest/contracts/tail map; warning baseline; resource/access/retention decisions; actual dispatch/skills/toolchain/base settings. Preserve OPERATOR merges, single-threaded writes, fresh context per ticket. No old benchmark validity, provider allowance, model/harness switch or unattended permission is inherited. `orchestrate-build` consumes the later seeded ledger; this note is not SETUP authority.

| Stage | Required evidence |
|---|---|
| Upstream | Planning checks, links/IDs/OL coverage, fresh O2, operator ratification. |
| Decomposition/readiness | Full build-memory/backlog checks; supplemental guard/fixtures and actual dispatch-path checks; unique files/backward chain; preserved history; fresh partition critique. |
| Product acceptance | `cargo fmt --all --check`, scoped package tests/counts, failing-without-change regressions; relevant drift/removability/CI-gating. Restore helper after tier-off checks. |
| Examples/receipts | F2 semantic/negative controls; working-tree versus committed-HEAD quickstart; exact pins/costs; native recipient closure. |
| Study/operations | Admitted wire treatment/caps/unknown use, oracle/backend/methods/detectors/parity; frozen protocol; failed/unexecuted slots retained. No favorable-effect AC. |
| Ticket close | Tree + staged/range history + supplemental guard; current-head/ancestor CI; external branch state; run/PR/index/PHASE LOG/state agreement. |
| Round tail | Independent gap hunt, composed verification, two verdict sums, unique owed-work homes, new operator acceptance, reconciliation/docs. |

Runs retain commands/results, revision/environment, harness/skills, layer/limitations; PR body, BUILD_INDEX and bounded PHASE LOG link them. Native receipts support auditing the recorded observations; hashes alone prove neither grading truth nor efficacy, and Markdown hashes do not substitute. ADR-0363 governs `live-executed`; fixture/foreign/live/engineered/public remain distinct. Full CI dispatch stays a maintainer/gate action under ADR-0362. Unknown/skipped prerequisites remain pending with owner/trigger/withheld claim. Null/negative/inconclusive scientific results can meet protocol acceptance; defaults/adoption/publication require separate evidence and decisions.

### F7 — Independent parity review should precede reconciliation
Claim: D2's conservative parity construction needs an independent feasibility decision before mandatory statistical implementation.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in accepted findings\
Evidence: D2 F6–F8: R=5 perfect-agreement half-widths .584/.666/.758 at T2/T6/T30 against proposed ±.10; E3 F6 recommends efficient established-method review; ledger Q-3/Q-4/H2.\
Retrieved: 2026-10-29T21:08:27Z.\
Implication for the spec: Admit feasible methods/resources or deliberately narrow claims before P8.\
Outline delta: EXTENDS OL-09–11/20–24.\
Limitations: This construction's conservatism does not prove parity impossible; no new review/collection ran.

Recommend **D4 — independent parity-method feasibility review**, proposed outside the current 24-unit ledger. If approved at the imminent Q-3–Q-6 checkpoint, coordinator adds a stream-D row dependent on D1/D2/E3 and exact Q-3/Q-4 scope; output `docs/design/23_parity-method-review.md` (22 reserved O2), candidate allocation REQ-D-301–399; H2 gains D4 prerequisite. No row/assignment is added now.

A fresh independent context compares established methods/estimands, paired/clustered repetition assumptions, binary/continuous dimensions, attainable precision, numerical validation and full resource needs; inspect admissible existing parity first. Justify calibration tolerance relative to the useful effect: provisional ±10pp parity and +5pp useful gain cannot be ratified by default without identification/measurement-error analysis. Return recommendation, counterexamples, ADR scope and bounded resource options. No new statistical platform, paid calls, task/provider collection or automatic sample increase. Method review, O2 spec review and decomposition critique are separate duties.

Operator may approve D4, justify another route, or explicitly limit claims to smoke/mechanisms while recording the central comparison unmet/deferred. H2 reconciles that ruling. Sequence: **Q-3–Q-6 checkpoint → H2 → O1 synthesis → O2 independent review → O3 ratification → later decomposition/readiness → orchestrate-build**. Follow-up gaps get scoped rows, never implicit approval or hidden implementation design.

## Answer and completion assessment

H1 supplies compatible identities, amendments, chain/tail, warning treatment, artifacts and validation. Fresh warning baseline is established. Method choice, ratification, tooling prerequisite and actual branch/CI readiness remain later. Next user checkpoint stays **before H2**; design completion asserts no runtime/scientific success.

## Open questions

- **Operator/Q-3/Q-4 → D4/H2/O1:** bounded independent methods follow-up or justified alternate/limited claim; pins/resources and execution permissions remain distinct.
- **Operator/Q-5/Q-6 → F/G/H2:** rights/access/retention, replication/contribution/distribution; no outreach inferred.
- **Operator/Q-7 → O3:** reviewed supplement, amendments and deferrals; this note accepts no ADR.
- **Decomposition/tooling reviewer → readiness:** actual dispatch/base and scoped guard compatibility; review before product dispatch. Routine ticket boundaries need no new scope ruling.

## Spec requirements emitted

Candidates only; O1 preserves identities and reconciles overlap.

| ID | Candidate / rationale | Findings / outline | Acceptance |
|---|---|---|---|
| REQ-H-001 | Preserve completed build/frozen contract/OPERATOR merges. | F1/F3; OL-21/24 | History diff, explicit extension/base. |
| REQ-H-002 | Separate supplement authority; retain candidate aliases. | F2; OL-21/23 | Candidate/source set equality. |
| REQ-H-003 | Trace OL/candidate/AC to one owner/disposition. | F2; OL-14/23 | Guard rejects gaps/duplicate owners. |
| REQ-H-004 | Route changed semantics through owned ADR/amendments. | F3; OL-20/21 | Section/decision/approval audit. |
| REQ-H-005 | Size packages by actual coupling/context. | F3/F4; OL-20/22 | Fresh partition review; no fixed count/days. |
| REQ-H-006 | Append unique numbered contracts/full-tail roles. | F4; OL-21/24 | Grammar/ordering and new signature. |
| REQ-H-007 | Disposition old warnings; enforce new integrity/routing. | F2/F5; OL-21/22 | Baseline/history/guard; old-gate negative test. |
| REQ-H-008 | Require readiness packet and checked dispatch path. | F2/F6; OL-20–24 | Guard/fixture/preflight evidence before dispatch. |
| REQ-H-009 | Separate implementation/research/adoption acceptance. | F6; OL-11/15–17 | Null/negative valid; unsupported claims withheld. |
| REQ-H-010 | Independently disposition parity/method feasibility. | F7; OL-09–11/24 | Reviewed methods/resources or limited scope. |
| REQ-H-011 | Preserve checkpoint order/state-writer/fresh-context duties. | F1/F7; OL-22–24 | Timestamped phase-entry audit. |

## Handoff

E3/F2 dependencies resolved. E3 post-terminal grading resolves E2's timing preference without rewriting it. H2 reconciles F3/G2/G3 and any approved D4. No family disposition or fixed ticket count changes. Coordinator owns acceptance/state/checkpoint. Only this note written; no spec/ADR/ticket/manifest/build-state/production change.

Verification: deferrals-first exit 0/empty; planning validator before/after exit 0, zero violations/warnings (locale fallback). Full external validator exit 0, **zero violations/eight legacy warnings**, with report identity in F5. Scoped review checks 7 findings ×8 fields, 11 unique allocated IDs, 17 relative links/3 anchors, four closing sections and four consistent tables; personal-path and whitespace checks clean. Full added-file diff reviewed; protected source/spec/build-history diff empty at inspection. No Rust builds/tests, runtime/task/oracle/provider executions, installations, commits or external mutations.
