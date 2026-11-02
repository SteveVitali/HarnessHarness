# 11_operational-proof-protocols — Bounded evidence for S1, S4 and S5

Stream: D — Scientific methodology\
Researched: 2026-10-29 UTC\
Author: /root/d3_operations\
Ledger rows covered: D3\
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d` plus approved planning/accepted inputs.\
Confidence overall: medium (source high; execution unproved).

**Decision:** one useful-task chain, one confined sink, eight operational cells. **D3 executed zero trials, builds, provider calls or demonstrations.** Repetitions specify acceptance coverage, not efficacy n or reliability. Execution/publication remain gated.

## Basis

Read [brief](../brief.md), [assignment/Q](../research-ledger.md), [conventions](../research/CONVENTIONS.md), [trace](../OUTLINE_TRACE.md) and `synthesize-spec` run mode (skills `e981ec57d487a2c41d416672483b5a4df15b1233`). Inputs: [A1](../research/01_capability-inventory.md) F5/F8/F9, [A2](../research/02_evidence-audit.md) F3–F9, [A3](../research/03_newcomer-journey.md) F4–F7, [C1](06_demo-and-study-portfolio.md) F2/F5/F6, [D1](09_measurement-standard.md) F1–F5/F8 and [E2](12_feasibility-findings.md) F1/F4/F6/F7. Retrieval stamps are inspection times, not historical executions. No web research.

## Material findings and protocols

### F1 — The composed demonstrations remain unproved
Claim: Existing evidence leaves the composed task/effect/receipt demonstrations unproved.
Status: PARTIALLY VERIFIED\
Evidence kind: source inspection; accepted execution inspected only\
Evidence: A3 F5/F6: fixture replay and R0 drift (`finished/succeeded`); A2 F7: different historical R0 success. [R0](../../crates/hh-embed/src/bundle_ops.rs), 1566–1590, compares different vocabularies. [Dispatcher](../../crates/hh-env/src/dispatch.rs), 202–224/1261–1313/1750–1788, distinguishes approval/elicitation/Kp4. [Battery](../../crates/hh-env/tests/kill_point_battery.rs), 1–28/880–910, asserts unknown/probe with an AtomicU64 sink.\
Retrieved: 2026-10-29T20:34:03Z.\
Implication for the spec: Implement E2's owner seams and retain both historical failures and narrower successes.\
Outline delta: EXTENDS OL-06–OL-07, OL-12, OL-15–OL-18; CORRECTS OL-03's effortless replay premise.\
Limitations: Assertions were read, not rerun; no real-task or general recovery result follows.

Reuse E2's native subject, single Dispatcher lifecycle, Account and closure. No fabricated pending/charge/success rows or duplicate commits. Freeze revision/features and task/monitor/executor/Account/host/observer pins. Keep S4 outside D2's fault-free cohort.

### F2 — S1 needs independent grading and separate receipt verdicts
Claim: S1 requires independently graded output and procedure-specific controls.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: C1 F2/D1 F3/E2 F1/F6 specify task/oracle/closure prerequisites. [§10.4](../../spec/sections/10-evaluation-reproducibility-plan.md), 127–146, separates R0–R3/independence; [replay](../../crates/hh-embed/src/replay_ops.rs), 140–172, appends markers/reports divergence.\
Retrieved: 2026-10-29T20:34:03Z.\
Implication for the spec: Link claims to procedures; exit/lifecycle success is not correctness.\
Outline delta: EXTENDS OL-05–OL-07, OL-15–OL-18.\
Limitations: Task/rights/model/caps need Q-3/Q-4; no efficacy or usability inference.

**Question:** can a reader produce and check one repair? Before outcomes select one eligible demo task/unchanged upstream verifier. Plan **two fresh subjects**, fixed configuration/snapshot/order. Retain both; no success-seeking replacements. Useful-output acceptance needs an independently correct artifact; negative diagnostics remain valid. Disclose any showcase selection.

Retain task/submission, native head/watermark, model/tool I/O, oracle invocation/output, grade, stop, veto coverage and costs. Annotate actual causal IDs/order and externally observed mutations.

Oracle controls: sanctioned correct output→pass; plausible wrong contents despite existing file→fail; broken oracle→oracle failure, not task zero. Diagnostic derivatives never replace benchmark tasks. Missing correct control holds acceptance. Mutated submissions require new digest/grade.

| Procedure | Required observation and negative control | Boundary |
|---|---|---|
| Native chain | Check prefix/causes/watermark/blob closure; changed event bytes fail. | Consistency, not external truth. |
| Replay | Equal declared decision/assembly sequence, unchanged prefix, no new model/effect rows/sink change; altered required input/definition refuses/diverges. Markers may append. | Recorded behavior, not fresh solving/full-stream equality. |
| R0 | Six clean-store cases: successful/failed/open→honest `finished/finished/open`; altered lifecycle/bytes/missing member→drift/invalid/incomplete. Re-seal diagnostic lifecycle negative to reach semantic comparison. | Separate level/verdict/completeness/independence; task failure can validate. |
| R1 | Contingent delete/rederive/hash/replay checks; changed derivation fails. | Replay alone is insufficient. |
| Recomputation | Pinned table/watermark; changed input changes result or fails identity. | Arithmetic, not oracle validity. |
| Fresh rerun | Second subject retains new calls/cost/output/grade. | No R2 without pins/seeds/margins/minimum n; no R3 from two. |

Clean stores on one installation imply repeatability only; independence needs distinct installation/signer evidence. F1/F3 own access/commands. No silent producer-cache borrowing; sanitization changes identity and declares losses.

### F3 — S4 requires real sink observations and reachable fault points
Claim: Operational controls must prove execution, prevention and detector sensitivity.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in source inspection\
Evidence: [KillPoint](../../crates/hh-ledger/src/fault.rs), 28–41, puts Kp4 before executor entry; [recovery](../../crates/hh-ledger/src/recovery.rs), 1–25, maps committed/no-terminal to unknown/probe. [Executor](../../crates/hh-env/src/executor.rs), 1–8/53–65/586–603, declares isolation/dedup/probe outcomes. [LocalExecutor](../../crates/hh-env/src/local.rs), 1–65/421–426, declares isolation `none` and returns an undeterminable probe. E2 F7 identifies the common seam.\
Retrieved: 2026-10-29T20:34:03Z.\
Implication for the spec: Admit confinement/observer; retain unresolved unknown.\
Outline delta: EXTENDS OL-12, OL-15–OL-16, OL-20.\
Limitations: One platform/class/fault cannot prove exactly-once effects, universal containment or prompt-injection resistance.

**Apparatus:** S1 backend/Dispatcher, fixed non-idempotent append of unique trial/effect marker, flush/digest. Freeze risk/execution requirements; no invented dedup/arbitrary commands/network. Independent supervisor survives subject death, records entry/bytes/counts/termination before launch, at boundary and after quiescence. Denied target/read-only sentinel detects stray writes; existence never passes.

Record actual isolation/mounts/network-off/observer access. Scratch paths and LocalExecutor alone are insufficient. Unsupported shared backend→unavailable/scope return to E3/operator, not an in-memory substitute. Fresh store/workspace/marker/sink/approval state per repetition.

**Minimum: eight cells × three repetitions = 24 trials.** Each block uses a frozen seeded permutation/balanced controls; inputs/backend match except declared conditions. Restart/response phases are not replicates. Acceptance requires all boundaries reached/assessable, every invariant correct and all O8 defects detected. Partial coverage stays partial.

| Cell | Trigger and required observations |
|---|---|
| O1 allowed control | Allow; one marker/matching digest/observed effect. Same-path positive reservation/call and confined gate-off comparator for O3. |
| O2 Kp4 | Commit→injected termination before executor entry, delta 0. Restart/new fence/preserved prefix→unknown/worker_lost+probe; no blind redispatch/duplicate obligations on repeated restore. |
| O3 deny | Monitor reached; deny/refused, no prepare/commit/entry/mutation. O1 proves reachability without the gate. |
| O4 permission allow | Pending→suspend→host answer→wakeup/resume, same effect/one mutation; F4 negative/restart controls. |
| O5 permission deny | Pending→offered deny→refusal; no prepare/commit/entry/mutation; conflicting answer cannot authorize. |
| O6 reservation refusal | Real Account: zero remaining model_calls; attempted reserve refuses, no transport entry/dependent mutation. Not terminal exhaustion. |
| O7 exhaustion/drain | Open committed effect behind bounded barrier; real time.wall_ms reaches Account bound. Block new dispatch, drain to observed/unknown, then exhaustion/stop. No later marker; committed work was not prevented. |
| O8 detector negative | Confined diagnostic executor appends twice despite success report; observer must flag duplicate. Seeded detector check, not invariant pass. |

O2 proves classification, not completion. Read-only instrument probe records applied/not_applied/undeterminable+evidence. External zero cannot confer native settlement; no admissible probe→unknown/no blind retry. Excludes Kp5/Kp6, power/filesystem faults, concurrency/hostile observers. O8 prevents application idempotency hiding duplicates.

### F4 — Approval and exhaustion need complete native sequences
Claim: Approval and Account proofs require native intermediate states.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in source inspection\
Evidence: [responses](../../crates/hh-embed/src/work.rs), 498–586/660–740, gate durable offers/duplicates; [r2_11](../../crates/hh-embed/tests/r2_11.rs), 1–12, fabricates test pending rows. [Account](../../crates/hh-budget/src/account.rs), 294–330/581–630/980–1018, owns reserve/drain/totals; [tree](../../crates/hh-budget/src/tree.rs), 559–593, exhausts at level ≥ hard. E2 F4: binding gaps.\
Retrieved: 2026-10-29T20:34:03Z.\
Implication for the spec: Connect owners; check intermediate states.\
Outline delta: EXTENDS OL-12, OL-15–OL-16, OL-20–OL-21.\
Limitations: Unexecuted composition; this slice does not certify all four comparison caps.

**O4:** observe actual pending/suspend; two observer intervals with no prepare/commit/mutation; unoffered answer refuses; offered more_info stays pending (optional if absent); restart preserves pending; host allow-once wakes/resumes original effect to one mutation. Same answer/key returns idempotently; conflicting new key refuses AlreadyDecided, no second grant/mutation. O5 tests deny. Retain offers/authority/causal IDs. Protocol input_required/resume_paused is no substitute: its effect may already be committed.

**O6:** deterministic diagnostic model transport, independently counted transport entry, real Account reserve; O1 actually exercises the identical positive call path. This is fixture/local provider-path evidence, not live usage/cancellation. **O7:** real root/separate instrument budget, elapsed deadline retained across restart/approval wait. Supervisor barrier releases without another effect: proposed 5 s deadline, 1 s observations, 10 s fail-safe. Expiry before commit→unreachable; timeout/unknown stays explicit.

Rebuild Account: compare root/child holds, sources, deduplicated charges and stop; repeated reconciliation cannot double-charge. Preserve incurred/billed use where reversion differs. Grace=0. E3 resolves terminal/grader timing; D2/E3 separately test four caps/derived token ceilings.

### F5 — Denominators and resource stops keep small trials honest
Claim: Stage counts prevent unreachable or unobserved attempts becoming evidence of safety.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: D1 F2/F8: missingness/opportunities; C1 F5: controls; E2 F7: tier-off/unknown; F3/F4: boundaries.\
Retrieved: 2026-10-29T20:34:03Z.\
Implication for the spec: Publish per-case coverage/violations and fixed stops.\
Outline delta: EXTENDS OL-10–OL-12, OL-15–OL-16, OL-20.\
Limitations: Three repetitions are not a representative fault distribution or basis for IID reliability intervals.

Per cell: planned P→invoked A→boundary observed R→fully observable T; violations V/expected verdicts. Publish A/P, R/A, T/R, V/T (zero denominator=undefined). No pooling faults/phases/retries/events. U=P−T gives possible planned-slot violation fraction [V/P,(V+U)/P], a missingness bound, not confidence. O8 separately: detected/3 seeded duplicates.

Keep refusals/tier-off/unreachable/missing detector/timeouts/oracle failure/unknown/cancellation; no post-outcome exclusions. At most one setup diagnostic rerun per cell within cap, new identity/no replacement. Failed negative control blocks acceptance. Integrity/confinement/unexpected destination/resource breach stops immediately; otherwise fixed completion/cap, never until green. Retain partial costs/results. Zero means 0/T named cases/host/assumptions, not exactly-once or a population rate.

**Proposed Q-4 envelope:** one subject, ≤2 cores/4 GiB RAM, 2 wall-hours/4 core-hours/3 GiB new disk, $0 provider/cloud. S4 ≤90 s/trial; remaining time for receipt/oracle/bug checks. Assumes pinned prebuilt binaries, admitted backend, small licensed inputs; no builds/downloads/installs. Missing prerequisites→unavailable. Optional prebuilt plugin: +30 min/1 core-hour/1 GiB; aggregate 2.5 h/5 core-hours/4 GiB disk. Enforce clocks/disk/child cleanup.

Two S1 task/model runs are **outside** this envelope. Q-3/Q-4 pins task/runner/model/numerical subject+instrument caps; prefer D2 configuration. Charge both; no confirmation reuse. Historical spend supplies no estimate. Limits are unapproved proposals.

### F6 — S5 connects preserved failure to verifyfixed and optional conformance
Claim: S5 can verify a mismatch repair and separately test one package.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in source inspection\
Evidence: A3 F6/F7 supplies R0 failure; C1 F6 makes packaging contingent. [Tests](../../crates/hh-varhost/tests/package_variant.rs), 61–80/132–210/246–283, seal/invoke/reject tampering but may return early without binaries/helper; A1 F9/A2 F8 distinguish skips/invocations.\
Retrieved: 2026-10-29T20:34:03Z.\
Implication for the spec: Require semantic verifyfixed and actual package invocation.\
Outline delta: EXTENDS OL-13, OL-16–OL-18; CONFIRMS OL-11, OL-21.\
Limitations: No issue/PR/package run performed; G2/F3 own instructions, Q-5/Q-6 distribution.

Initial bug: A3 expects finished, derives succeeded, exits 0 with drift. Preserve receipt/revision. Report versions/commands, accessible native/bundle/input identities, expected/actual fields and raw output; no secrets/private paths. Reader locates mismatch independently.

**Verifyfixed:** baseline and repaired-revision reproductions using equivalent fresh exports, plus six F2 R0 controls. Preserve outcome; altered lifecycle still fails. Unavailable historical bytes→labeled diagnostic equivalent with differences, no historical reproduction claim. Setup failure is reportable, not verified-fixed. Preserve A3; screenshots/exit codes are insufficient.

**Optional package:** pinned hh-compact-evict-oldest; four checks: build/seal/load/bind/assess/propose+correct output, altered binary refusal, wrong pin refusal, absent helper unavailable/refused. Require invocation/provenance markers; missing prerequisites=skips. Kernel hash before/after tests no-kernel-rebuild; plugin build still required/outside prebuilt envelope. Same backend/grants/fixtures, no third study arm. Defer behind S1; foreign conformance stays corpus-scoped.

## Answer and completion assessment

D3 answers its design question: S1 useful output/receipt controls, S4 eight cells/24 trials, S5 verifyfixed/optional packaging. Execution waits for E2 seams, backend/probe admission and resources. No safety, efficacy, usability or replication result.

## Open questions

| Question | Owner / affected units | Operator decision |
|---|---|---|
| Shared backend, effect risk class, observer/probe? | E3; D3/F1/F2 | Routine within scope; new backend/install needs expanded authorization. |
| Native host response/resume, Account/deadline and grading-time integration? | E3; D1/D2/F1 | Owner design; contract change needs ADR/ratification. |
| Demo task/correct oracle control/model and run caps? | Operator, D2/E3/F2 | Q-3/Q-4; envelope above is not approval. |
| Recipient access/negative reports and optional package? | Operator, F1/F3/G2 | Q-5/Q-6; no publication/outreach authorization. |

## Spec requirements emitted

Candidates only; canonical/build-ticket IDs remain unchanged.

| ID | Requirement | Source; outline | Proposed acceptance |
|---|---|---|---|
| REQ-D-201 | Useful output requires oracle and complete attempt history. | F1/F2; OL-05–OL-06, OL-15 | Two fixed subjects; correct/wrong/broken-oracle controls. |
| REQ-D-202 | Separate chain/replay/R0/R1/recompute/fresh claims. | F2; OL-07, OL-16 | Per-procedure controls/verdicts, no replay dispatch, actual independence. |
| REQ-D-203 | Bind operational claims to confined real sink/observer. | F3; OL-12, OL-15 | Entry/bytes/counts/sentinel and O8 duplicate detection. |
| REQ-D-204 | Preserve Kp4 unknown/probe without optimistic retry. | F1/F3; OL-12, OL-16 | O1/O2 ×3; reached fault, prefix/fence/repeated restore and probe limits. |
| REQ-D-205 | Exercise actual permission sequence and negative transitions. | F3/F4; OL-12, OL-15 | O3–O5 ×3; pending restart, denied/invalid/duplicate responses. |
| REQ-D-206 | Use real Account refusal/exhaustion with committed-effect accounting. | F4; OL-12, OL-20–OL-21 | O6/O7 ×3; elapsed deadline, drain/unknown, no later dispatch, charge reconstruction. |
| REQ-D-207 | Preserve opportunity counts/failures with fixed stops. | F5; OL-10–OL-12, OL-15 | Reconciled stage counts, unknowns, seeded detector denominator, partial cohort. |
| REQ-D-208 | Link mismatch to scoped verifyfixed. | F2/F6; OL-16–OL-18 | Baseline/repair identities and six R0 controls; preserve A3. |
| REQ-D-209 | Gate extension claims on invocation/refusal controls. | F6; OL-13, OL-18 | Four package checks, explicit skips, kernel hash/build-cost disclosure. |

## Handoff

E3 owns integration; F1/F3 closure/procedures/denominators; F2/G2 trace/verifyfixed/package. No new row/portfolio change. E2 remains unimplemented; A3 failure/A2 positives coexist. Coordinator accepts/updates central state. Stop after this note.
