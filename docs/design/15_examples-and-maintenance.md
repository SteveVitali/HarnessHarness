# 15_examples-and-maintenance — Executable lessons and honest freshness

Stream: F — Receipts and maintenance\
Researched: 2026-10-29 UTC\
Author: /root/f2_examples\
Ledger rows covered: F2\
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; approved uncommitted planning and accepted dependencies.\
Confidence overall: medium; suite/budgets unexecuted.

**Recommendation:** four families, semantic/negative checks, separate code acceptance/freshness/study maintenance. Offline first; other modes follow owning fixes. No new executable commands or execution authorization.

## Basis and scope

Read root/docs instructions, brief/ledger/conventions/outline, `synthesize-spec` run mode (`e981ec57d487a2c41d416672483b5a4df15b1233`) and [A3](../research/03_newcomer-journey.md), [C2](08_feasibility-and-scope.md), [E2](12_feasibility-findings.md), [F1](13_receipt-contract.md), [G1](17_reader-journey.md). **2026-10-29T20:44:01Z** is inspection time, not execution; code sources use baseline. Q-3–Q-6 retain program decisions, not reader approval steps. Reuse G1's guides/three proposed reader pages; no platform/viewer/schedule.

## Material findings and design

### F1 — Existing checks support a suite but not every reader promise
Claim: Current quickstart/Lab paths expose reusable seams, while selected CLI/R0 defects remain.
Status: VERIFIED\
Evidence kind: source inspection; recorded execution inspected through A3\
Evidence: [quickstart checker](../../scripts/check-quickstart.sh), extraction/assertion sections; [composed battery](../../crates/hh-embed/tests/cap_2_composed.rs), `exp_drain` 1192–1255 and analysis 1836–1980; [conformance](../../crates/hh-embed/tests/conformance.rs), analysis round-trip/refusals 4566–4777. A3 F3/F6 records CLI request failure/R0 drift.\
Retrieved: 2026-10-29T20:44:01Z.\
Implication for the spec: Regress repaired paths before publishing commands.\
Outline delta: CONFIRMS OL-08, OL-17; CORRECTS OL-03's documentation-only shortcut.\
Limitations: Source is not execution; fixtures prove no task/study efficacy.

[Exemplars](../../crates/hh-lab/src/exemplars.rs), 1–71, are parameterized shapes. [Analysis](../../crates/hh-embed/src/analysis_ops.rs), 165–195, needs `spec`/task context; [CLI comparison](../../crates/hh-cli/src/lab.rs), 915–940, sends baseline/candidate only. [Experiment](../../crates/hh-embed/src/experiment_ops.rs), 715–800, needs returned `run_plan_id`/launch `lease_id`. Fix wrappers or supply the checked restricted runner; no guessed operands or invented `hh run replay`. Repair R0 `finished`/`succeeded` comparison; until regressions pass, cards remain unavailable with reason.

### F2 — Four families cover the first-wave examples
Claim: Four example families can share one recipe/check structure.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: C2 F2/F3 bounds the connected portfolio/restricted runner; E2 F1–F7 identifies owning fixes; G1 F1–F4 specifies branches/gates.\
Retrieved: 2026-10-29T20:44:01Z.\
Implication for the spec: Reuse this minimal inventory.\
Outline delta: CONFIRMS OL-05–OL-08, OL-12, OL-17; EXTENDS OL-20.\
Limitations: IDs/sizes/paths proposed; task/model/runner unresolved.

| Proposed family | Minimum case / admission |
|---|---|
| EX-01 setup/inspect/replay/receipt | Scripted quickstart, attach by run ID, six actual-event excerpts, replay and clean-recipient export/verify/R0. Fixture only; full card needs R0/F1 closure fixes; separate procedure verdicts. |
| EX-02 offline Lab | Two synthetic arms × two synthetic tasks × five repetitions = 20 subjects. Actual register/expand/open/next/claim/launch/settle/close, projection/persisted analysis; edit one fixture observation. Checked runner first; no efficacy claim. |
| EX-03 useful task/adapt/live | One Q-3 task, real submission/unchanged executable oracle; validate/compile consumed cap/permission edit and new attempt; EX-01 receipt. E3 subject/effect/grade/cap fixes/F1 closure first. Prerecorded inspection is not task execution. Live adds provider/resources/ADR-0363. |
| EX-04 operations | Same confined sink/effect for control/Kp4, deny, permission pause/respond/resume, exhaustion. D3/E3 dispatcher/Account, declared backend/features, independent sink. Scratch is not sandboxing; no general containment/exactly-once claim. |

EX-02 proposes task 1 successes A=1/5, B=4/5; task 2 A=3/5, B=2/5, with pinned binary sequences. Require an admitted informative comparison of these unequal, nondegenerate contrasts, without forcing significance. E3 verifies the exact report path before publication; method limits stay visible. One-task/constant/missing-row variants are separate refusal/undefined controls. Fixture-only teaching is not study evidence. D2 remains separate; optional G2 component/F3 consumer: one each under C2.

Proposed, unimplemented `scripts/examples/`: recipes/tiny fixtures/metadata (case/version/mode/owner, prerequisites/features/platform, pins, checker/negative case, caps, receipts/reader locations). Owning crates keep codecs/checks; scripts compose the restricted runner. Cards show prerequisites/network/cost, exact commands/results, failure exit/next choice. Publish only after recorded execution.

### F3 — Semantic outcomes and negative controls prevent false passes
Claim: Each example binds semantic/negative checks, exact pins, resources and receipts, separately from process exit.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in source inspection\
Evidence: A3 F5/F6 exposes replay markers/exit-zero R0 drift; F1 F3/F5/F7 separates invocation/verdict/closure; conformance 4720–4777/composed battery 1933–1980 assert typed refusals; [toolchain](../../rust-toolchain.toml) pins `1.94`; F1 F2/F4/F6 and E2 F4 define identity/access/accounting constraints.\
Retrieved: 2026-10-29T20:44:01Z.\
Implication for the spec: Parse outputs through existing codecs and retain native verdicts.\
Outline delta: EXTENDS OL-06–OL-08, OL-15–OL-17, OL-20–OL-21.\
Limitations: Battery unexecuted; native types authoritative.

| Check | Positive / negative assertions |
|---|---|
| Setup/trace | Connectivity `ok`, returned run joins `succeeded/completed`; event ID/seq/payload/receipt resolve. Binary mismatch, wrong schema or unresolved annotation fails. |
| Replay/R0/closure | Named verdict, unchanged source prefix, allowed replay markers, no unexpected provider/effect work; lifecycle/recipient bytes agree. Tamper/missing member/mismatch fails; access loss makes procedure unavailable. Exit-zero `not_reproduced` is failure. |
| Lab | Twenty slots join launches/settlements/rows; report/body/record agree; same inputs retain report identity, fixture edit changes results. Missing spec/row/bad lease/incompatible arm refuses; duplicate settlement adds no n. |
| Task/edit | Subject→effect→submission→oracle→grade→settlement joins; edited field consumed/enforced. Wrong output scores failure; oracle crash/timeout is failure/unavailable, not zero; unsupported edit refuses. |
| Operations | Control sink once; Kp4 commit/unchanged sink/unknown-probe recovery; deny before prepare; valid permission required; Account blocks new dispatch. Invalid answer/tier-off refuse; unknown in-flight outcomes remain. Protocol input/resume is not approval. |

Record F1 case/mode/revision, native attempt refs, commands/inputs, prerequisites/invocation, exit/signal/timeout, verdict/reason/checker/resources/evidence. **Assertion pass differs from operation pass:** expected refusals remain refused. Required positive skips block readiness; optional skips award no evidence class. Missing fields fail.

Compare exact IDs/hashes/deterministic fields and F3's per-field numerical tolerances. Never normalize grades/enforcement/losses. Omit only declared time/temp-path variation from excerpts, retaining raw evidence. No whole augmented-ledger equality. Screenshots derive from tested receipts with equivalent text.

Pin commit/dirty-patch or dirty unknown; `rustc -Vv`, Cargo.lock, OS/features/binary/schema; fixture/definition/registry/variant/checker/AnalysisSpec/seeds. Task: suite/task/instruction/submission/oracle/runner commits/image digests. Live: requested/served fingerprints, roles/routing/fallback/cache/seeds, pricing snapshot/source/date. No mutable-tag/alias exact pins. Native canonicalization; document SHA-256 labelled separately.

Separate setup/subject/verification/retention/rerun costs. Offline is no service calls after provisioning; setup may need network. $0 provider is not zero local/Codex cost. Live: native caps/pinned-rate estimates, actual/failed/unknown usage; no inherited R4 allowance/retry/fallback. CI timeout is not native cap enforcement; E3/D2 owns dimensions/timing.

Link F1 immutable attempts/heads/grades/analysis/closure and legacy live-record bytes/revision; no shadow store/home fallback. Redaction/expiry discloses losses; restricted closure is not public closure. Link F3 canonical procedures/tolerances.

### F4 — CI and refresh need different gates
Claim: Separate static checks, offline execution, live freshness and study revision while preserving current CI classification.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in source inspection\
Evidence: [workflow](../../.github/workflows/ci.yml), path/quickstart steps; [fast-path](../../scripts/ci/fast-path.sh), 1–49; [gating lint](../../scripts/ci/check-ci-gating.sh), step/readers rules; [ADR-0362](../adr/ADR-0362-r4.2a-ci-docs-only-fast-path-artifact-anchor-and-full-run-cadence.md), D-B/D-D. [Living-docs guard](../../scripts/check-living-docs.sh) checks tracked paths, not execution; [documentation modes](../README.md#how-documentation-changes) preserves history; [ADR-0363](../adr/ADR-0363-r4.5a-live-executed-evidence-class-spend-ledger-and-live-verify-gating.md), D1–D5, and [live validator](../../scripts/check-live-evidence.sh) distinguish evidence/spend invariants from freshness. F1 F6/F7 requires losses/supersession.\
Retrieved: 2026-10-29T20:44:01Z.\
Implication for the spec: Preserve classification/dispatch cadence; route freshness/study changes separately with dated supersession.\
Outline delta: EXTENDS OL-11, OL-15–OL-17, OL-20–OL-21, OL-24.\
Limitations: Ceilings/cadence are proposals, not measurements, current settings, scheduled jobs or spend approval.

| Lane | Proposed ceiling / checks |
|---|---|
| Static, both modes | ≤60 s/OS; ≤20 MiB output; no builds/network. Relative links/anchors, command-block equality, checker/pins/annotations/claim/procedure links. Missing/stale fails. |
| Full offline | One Linux leg: ≤5 min after existing build, ≤1 GiB scratch, ≤50 MiB diagnostics. EX-01/02 plus admitted deterministic EX-03/04 controls; no credentials/services. |
| Platform acceptance | Existing Linux/macOS owner batteries/tier checks; added cases ≤2 min/leg after build. Assert supported/refused behavior. |
| Clean quickstart | Existing Linux full-path `bash scripts/check-quickstart.sh`; build cost separate. **Committed HEAD**, never working edits. |
| Live | No routine PR calls; authorized local/protected leg/caps; unset allowance=zero. ADR-0363 subleg records; skips never pass. |

Timeouts retain partial results; diagnostic reruns retain failures. Required unavailable cases block cards. Only `docs/`/`research/` are docs paths; README/scripts/fixtures/workflow are non-docs. Canonical inputs/assertions stay outside them. Both-mode checks compare displayed/canonical blocks and output/check mappings; full execution reads canonical inputs **without committed docs/research reads**. This transitive check needs no Rust on fast mode.

After `id: path`, any direct/indirect committed docs/research reader needs `# ci-mode: both — <reason>`; others use `if: steps.path.outputs.mode != 'fast'`. Never allowlist real reads or gate expensive docs execution silently. Uncovered recipes require bounded both-mode execution or explicit CI redesign. Run `bash scripts/ci/check-ci-gating.sh .github/workflows/ci.yml`; keep anchor pair last and maintainer/gate dispatch cadence. Uncommitted README receives static review, then committed exact-head execution; old HEAD cannot verify edits.

| Trigger | Owner/action and claim effect |
|---|---|
| Commands/schema/runtime/fixtures/checker/toolchain/features | Implementation/examples owner: scoped regressions, affected offline cases/equivalence, ceiling measurement. New runnable revision only after pass. |
| Artifact/access/expiry/redaction | F1/F3: identity/recipient closure, new derivative/loss/supersession; dependent procedures become restricted/unavailable. |
| Provider/model/price/environment drift/failure report | Maintainer: metadata/access review, stale fresh-run claim, capped authorized diagnostic proposal. Connectivity cannot renew useful-task/study claims. |
| Oracle/analysis/grade/cohort error | D2/F1/F3: affected observations, versioned recompute/regrade/correction; retain old outputs/attempts, no extra n. |
| New task/model/treatment/method/consequential drift | D2/operator: new protocol/pilot/admission/resources; freeze before confirmation. Not docs refresh; preserve old population/conclusion/null results. |

Propose release and 30-day launch metadata/link/failure reviews. Live-dependent “current” cards older than 30 days become **refresh due**, not false or permission to spend. Historical findings do not expire. Cadence ends after launch unless adopted; Q-5/Q-6 fix retention/publication. Existing evidence index holds owner/revision/time/receipt/trigger; no automatic collection.

## Answer and completion assessment

F2 settles examples/checks/pins/CI/refresh. Owning integration gaps block implementation, not design. Teaching/task/study remain distinct; no runtime readiness, measured CI performance or publication claimed.

## Open questions

- E3 → F2/G2/H1: invocation, admitted synthetic analysis and consumed edit field; provisional until owner design, broader API/backend returns to Q-2.
- D2/E3/operator → F2/F3: exact task/oracle/image/model pins/live caps, Q-3/Q-4.
- F1/F3/G3/operator: rights/retention/expiry/public freshness, Q-5/Q-6. No new operator question.
- Implementation/CI maintainer → H1: measure proposed ceilings; revise scope/budget on overrun, never silently skip.

## Spec requirements emitted

Candidates only; H1/O1 reconcile A3/E2/F1/G1 overlap.

| ID | Requirement / rationale | Finding / outline | Acceptance |
|---|---|---|---|
| REQ-F-101 | Four families; offline/live/readiness separation. | F1/F2; OL-05–OL-08, OL-12, OL-17 | Owner/recipe/unavailable mapping; no fixture substitution. |
| REQ-F-102 | Semantic/negative checks; invocation distinct. | F3; OL-07–OL-08, OL-15–OL-17 | CLI/R0/tamper fail; expected refusal keeps refused outcome. |
| REQ-F-103 | Synthetic Lab lifecycle/projection/persisted analysis. | F1–F3; OL-08, OL-17 | Joins, repeat identity, changed-fixture response, unavailable controls. |
| REQ-F-104 | Exact pins and separate costs. | F3; OL-16–OL-17, OL-20 | Unresolved caps block live; unknown stays unknown; offline no provider. |
| REQ-F-105 | F1 native receipts/recipient closure/event links. | F3; OL-06–OL-07, OL-16 | No home fallback; missing/corrupt/restricted inputs tested. |
| REQ-F-106 | Both-mode equivalence/full execution under ADR-0362. | F4; OL-17, OL-20–OL-21 | Lint; docs drift fails fast path; recipe change selects full. |
| REQ-F-107 | Separate working-tree/HEAD/live checks. | F1/F4; OL-15, OL-17, OL-21 | Old HEAD cannot verify edits; skips never become live passes. |
| REQ-F-108 | Code/freshness/study maintenance with supersession. | F4; OL-11, OL-16–OL-17, OL-24 | Lost inputs disable procedure; drift never silently recollects. |

## Handoff

Design dependencies resolved. E3 owns fixes; D3 full operational acceptance; F3 procedures; G2 contributions; G3 publication; H1 build/CI sizing. Examples do not reduce D3 acceptance. No new row/portfolio change; coordinator owns acceptance/state.

Verification: deferrals exit 0/empty; planning validator zero violations/warnings (locale fallback). Python write, scoped links/fields/IDs/closing sections/diff checked; counts/hash to coordinator. **0 builds/tests/runtime/task/oracle/provider executions, installs, commits or paid calls.** Only this note written.
