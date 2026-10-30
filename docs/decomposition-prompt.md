# Evidence and adoption — decomposition handoff

**Status:** Frozen upstream handoff, Q-7 ratified by the operator at 2026-10-30T02:05:32Z.  
**Operator answer:** “Looks good, proceed” — user message in the current Codex chat.  
**Scope:** Prepare the next ticket/build-artifact phase from the ratified supplemental specification. This record does not launch decomposition, implement tooling or product code, allocate execution funding, authorize outreach/publication, or merge a change. Subsequent changes to this handoff require a dated addendum; preserve this original record.

## Run line and sizing

Invoke the external `decompose-spec` skill with these exact inputs from the repository root:

```text
decompose-spec spec=docs/evidence-and-adoption-spec.md build_name=evidence-and-adoption mode=extend tickets_dir=docs/tickets tail=full dispatch_target=subagent granularity=balanced sequence_prefix=true ledger=true
```

This is a skill invocation, not a shell executable. Resolve the external checkout through [MAINTAINING](../MAINTAINING.md#build-memory-tools); the inspected skill revision is `e981ec57d487a2c41d416672483b5a4df15b1233`. Reconfirm the revision and applicable instructions before starting. Do not change external skills as a convenience.

The coordinator selects `subagent`, the skill's conservative default, for ticket sizing: each unit must fit well under half a fresh worker context with verification headroom. This resolves the prior handoff's dispatch-size placeholder without selecting a headless driver or a new product execution environment. Native skill coordination is the planned route; a later change to headless dispatch requires the conditional tested integration described below. Keep planning, architecture/method decisions and adversarial partition review at the already agreed Extra High effort; use the agreed task-dependent effort policy for later implementation. Size by actual coupling and working set, not by the number of source requirements or packages.

## Ratified inputs and identity

| Input | Binding identity / use |
|---|---|
| [Supplemental specification](evidence-and-adoption-spec.md) | canonical within supplemental scope; frozen SHA-256 `8f540171dd38711ad563459d1721277b84100b66a207379668f0efe6db8b208f` |
| Reviewed v0.2 before ratification metadata | SHA-256 `914ea2fe1b9dbbd5b37ab3a6a35987c72f40f9f159ee52fd0520b982c09f648c`; approved content, not the current file digest |
| [Independent review and closure](design/22_independent-spec-review.md#delta-rereview) | SHA-256 `f68a284d4bc6216cb1de243d59dd9e8c553f3da69dfebf7459b46a0e49efc5b0`; two findings closed, all limitations retained |
| Approved five-decision guide before its status update | SHA-256 `060966bd9d0590e3e089e00adea6c6f503677d58c447e6119659754d07ede982`; current [guide](spec-ratification-review.md) records ratification |
| [Founding brief](brief.md), [frozen research ledger/Q](research-ledger.md), [frozen outline trace](OUTLINE_TRACE.md) | Intent, exact operator decisions and all 24 covered planning obligations |
| [Source manifest](evidence-and-adoption-spec.md#appendix-c) | All 22 accepted notes and their hashes; source requirements remain literal in Appendix A |
| [Handoff design](design/20_build-handoff-design.md), [reconciliation](design/21_synthesis-readiness.md) | Workflow compatibility and reconciled scope; governing supplement resolves conflicts |
| Repository planning baseline | `001490c14a6568972d16a663bb04c07a4255d30d`; re-read actual current/default-branch state before extension |

Ratification changed status/decision/coverage metadata only. The reviewed §§1–9 plus Appendix A are byte-identical to the approved draft (SHA-256 of the contiguous text from `<a id="s1"></a>` up to, excluding, `<a id="appendix-b"></a>`: `ebb5ee730d928899f867f335e2c924711e4f7b3424f56355e6eb4abc924a034d`). Appendices C–E are also byte-identical. This proves preservation of the reviewed design, not product execution or scientific validity. Source notes and the independent review remain immutable.

Read the supplement in full, its governing contracts, the source notes relevant to each partition, and the applicable root/nested AGENTS instructions. Reconfirm code anchors from disk. Treat preliminary proposals in source notes through the ratified rulings R1–R9/S10–S12 and explicit Appendix E dispositions. Do not silently revive superseded alternatives.

## Binding scope and traceability

- Preserve the first-wave six-scenario scope, all 22 capability-family dispositions and nine later-study groups. The useful-task path, receipt, offline lesson, real controlled compaction study, bounded operations and contribution path remain connected. Optional one plugin and one native→foreign-reader exercise remain optional and bounded.
- Keep the real controlled study central. A sound null, negative or inconclusive result can satisfy admitted research acceptance. Missing admission or unexecuted collection remains unmet; fixtures cannot discharge it. A product-default change is a separate decision.
- Preserve all 180 literal candidate IDs and their predicates/dispositions: 167 adopted, 12 merged, one superseded. `REQ-D-108` supersedes only to `REQ-D-304`, with `REQ-D-309/310` dependencies. No renumbering, deleted aliases, inferred favorable-result predicate or unnoticed new requirement.
- The supplemental ERE is `REQ-[A-H]-[0-9]+`. Keep `canonicalSpec=spec/CANONICAL_SPEC.md` and the existing legacy `R-2\.[0-9]+\.[0-9]+[a-z]?` matcher/index intact. Supplemental canonical status does not replace behavioral authority. Do not invent machine-state keys or enable mixed-namespace `req_index_literal_from`.
- Trace each OL through source/finding/candidate, supplemental section/ID and legacy/ADR relation, to exactly one primary ticket per adopted target or an explicit owned deferral, numbered acceptance/layer, and eventual run/PR/native evidence. Merged aliases retain predicates; consumers reference the single owner. Do not treat 180 source IDs as a ticket-count target.
- Retain D/E/R/P acceptance distinctions and all 23 acceptance hooks. Engineering completion does not close research, participant or publication predicates. Shared observations can serve the named participant consumers without a second cohort; missing participant evidence remains unmet.
- Carry the founding fixed constraints: useful breadth with reasoned limits; reproducible receipts with scoped claims; critical use of the alternative proposal; repository ADR/spec/build workflows; operator checkpoints. Ordinary outside contributors follow normal contribution instructions, without an internal Q/ledger ritual.

## Invariants every affected contract must carry

1. **One owner and durable authority.** Reuse `hh-embed/1`, HIR/1, `idp/1`, canonicalizers, registered planes/classes and lower-layer services. CLI remains a client/renderer. Durable fenced events precede projections. Preserve typed refusal, tier removability, generated-source workflow and dependency admissions.
2. **Actual task and grade.** Adopt the real subject; drain/revoke/seal, persist finish, settle execution cost once, then instrument grading. Required-grade pending blocks all scored consumers. Preserve first-valid grade selection, first-subject denominators, historical watermarks, explicit authorized corrections and no recharge/new n. Material protocol/oracle change requires a new confirmatory cohort.
3. **Real treatment and resource limits.** Retained view→bounded serialization→whole-vector Account reservation→sealed request→dispatch→actual usage. Preserve derived input/output totals, model calls, actual created-to-finished wall time, ancestor/outstanding exposure and physical incurred usage after logical reversal. Unknown enforceable bounds refuse matched-cap execution.
4. **Distinct statistical claims.** Native finite-manifest efficacy, task-mixture sensitivity, sharp-null testing and directional KL parity have separate targets/assumptions/uncertainty. Carry D4 method/applicability amendments, S11 confidence-label rules, missingness and multiplicity. No automatic original-runner effect transfer, arbitrary endpoint mapping or post-outcome method fallback.
5. **Native recipient closure.** Receipt profile stays within native bundles, with all planned slots and typed payload closure. Preserve R0 `open`/`finished`, separate verify/recompute/replay/fresh execution/independence claims, numeric tolerance versus exact identity/verdict rules, rights/loss/retention and recipient-after-producer-cleanup evidence. A build-note hash is not a native WAL receipt.
6. **Observable bounded demonstrations.** Preserve the 8×3 operational design, independent observation and genuine mediation. O8 must detect its seeded duplicate; it never earns an invariant-held label. Preserve four example families and method-specific controls: constant binary data retains Beta/KL uncertainty when otherwise admissible.
7. **No evidence inflation.** Keep task outcomes distinct from infrastructure success and evidence class. `live-executed` requires the actual ADR-0363 record, `foreign-run-verified` its recorded corpus replay, and `fixture-verified` remains fixture evidence. Missing/failed/skipped outcomes and denominators remain visible.
8. **Compatibility and reserved strings.** Carry [Appendix D](evidence-and-adoption-spec.md#appendix-d), including all six outcome classes, four cap strings, typed refusal vocabulary and frozen study factors verbatim. Proposed interface names/profile are admission candidates, not already implemented public APIs. Preserve unused IDs and do not infer new event classes.
9. **Scientific and public gates.** Carry the amendment register and exact execution gates, budgets/quotes, privacy/redistribution and consent prerequisites. Q-7 ratifies design scope, not ADR admission or task/cloud spending. No previous build's provider/model/harness/autonomy or spend permissions carry into this round.

## Decomposition outputs and order

Write the dependency/coupling map, working-set estimates and partition rationale under `docs/build/planning/`; author the complete dependency-ordered contracts and human/gate markers under `docs/tickets/`; append a numbered round and Plan extensions to the existing manifest. The eight P packages are decomposable slices, not eight mandatory tickets. In particular, the informative offline lesson needs the early P8 admitted-analysis slice or a verified exact-path equivalent; it does not wait for paid live collection.

Before choosing IDs or changing build state, re-read the actual default-branch SHA/ancestry, `chainTip`, completion evidence, owed deferrals/backlog, operator digest, Q decisions and open-stack CI using the required skills. The observed boundary was DONE Round 4, last R4.18, chainTip main, sequence 167b. Only if still true may the extension start Round 5/sequence 168 onward. These numbers are not allocated by this handoff. Do not reset/reinitialize the existing build memory or overwrite history.

Preserve old rows/files/signatures. Archive any replaced non-state build-ledger head text byte-for-byte, append the transition and retain the approved machine-state keyset. New contract branches use `codex/` and fork from/target the preceding ticket's branch. Keep one central state writer. Use parser-valid `blockedOn: (none)` when clear; a pending operator gate uses RETURN PASS, while a failed technical prerequisite uses blockedOn. Never fabricate an old gate's closure to satisfy a parser.

Each contract must include current anchors with reconfirmation, ownership/invariants, exact scope/exclusions, symbols and persisted/wire compatibility, numbered positive/negative acceptance checks with layers and evidence, caps/stops, context/dispatch estimate, Live stage, Production mutations, operating clauses and Run line. Skeletons after unresolved gates have no Run line and cannot dispatch. HUMAN rows name owner, trigger/date, verification and withheld claims. Record upstream planning approval as its scoped provenance, never as a pre-signed future execution gate.

Get a fresh-context independent partition review before accepting the contracts. Review coverage/alias predicates, shared decisions, backward dependencies, first-new-row routing, context ceiling, actual code seams and acceptance feasibility. Disposition every finding; newly discovered substantive design must receive an explicit amendment or scoped return for design, not a silent rewrite of the frozen sources.

## Required tooling and dispatch integration

A separate, explicitly authorized tooling prerequisite must implement and retain the supplemental coverage/routing guard and rejection fixtures described in §9.4/AC-9-2. Decomposition authors its scope; this handoff does not implement it. Preserve the stock external validator and legacy coverage. Add explicit supplemental source/pattern/literal owner tables to the new-round records. The guard reads records and never becomes a second state owner.

Before any product dispatch, prove candidate/alias closure, exact in-scope coverage and one owner; resolving AC/layer/evidence hooks; unique chain IDs/files, backward dependencies and full-tail roles; refusal of skeletons/pending or failed gates; active-round first-unlanded `nextTicket` and verified chain-tip/base; fresh acceptance signature and scoped legacy compatibility. Include negative fixtures for missing/duplicate IDs, wrong owners, old gate routing, unexplained historical residuals and old-signature reuse. Record versions, input digests, counts and diagnostics.

Native coordination invokes the retained guard before and after relevant dispatch/boundaries. Prove the actual chosen launch path with a stub worker: old GATE-G1 pointer dispatches zero work, correct first-new row dispatches one, pending/failed prerequisites stop. Test CI failure and resume. If headless dispatch is later selected, add a reviewed bounded wrapper around `--max-iters 1`, distinguish iteration-cap/no-progress from success, and test its actual integration. Documentation clauses, temporary ratification checkers or a passing stock validator cannot substitute for this retained executable prerequisite. Do not launch an unguarded loop.

Rebaseline the eight historical warning classes in §9.5 against their exact paths/rows and completion evidence; do not allow any arbitrary set of eight warnings. Preserve frozen history, including the recorded second 2026-09-15 PASSED—accept ADR-0226 gate disposition. New/unknown violations remain fatal; a real incompatibility needs a narrow reviewed remedy. New phase logs remain ≤2 KiB with evidence links, new indexes use correct headers/enums, and new run records carry complete Harness/Skills/timing/CI headers.

## Full tail and live-state read decisions

Use unused grammar-valid identifiers and fresh acceptance provenance; examples in H1 are not allocated IDs. Record an explicit role-equivalence map for all nine closeout roles, with a fresh GATE-G identifier serving the GATE-ACCEPT role:

| Role | Required work / live-state reading |
|---|---|
| CAP.1 | Independent gap hunt against the full ratified scope and evidence |
| CAP.2 | Composed verification across execution, analysis, receipts and actual recipient procedures |
| CAP.3 | Closure, unique owed-work homes and accurate completion layers |
| GATE-ACCEPT | New operator readout/signature for this round's exact scope; old acceptance never substitutes |
| REC.1 | Backlog/readiness; read current artifact access and any claimed service state, because accessibility can expire |
| REC.2 | Spec reconciliation; read landed code, ADRs and coverage so the recorded contract reflects actual changes |
| REC.3 | Integration plan; read actual stack CI, branches and release conditions before recommending integration |
| DOC.1 | Reader documentation, examples and evidence links with actual claim limits |
| DOC.2 | Agent instructions and ownership/verification guidance aligned to the landed changes |

The later plan must reproduce the REC read decisions under its `## Decomposition decisions` heading, as `tail=full` requires. These read obligations grant no collection, production or publication authority. Record new gate words/UTC/channel/items/expiry/void conditions and required agent-draft confirmation; agents never sign. `mergePolicy: OPERATOR` remains binding.

## Gates, delivery and stop

Architecture/method admission precedes affected semantics/inference; exact task/model/oracle/environment/backend/resources precede execution; protocol freeze and complete quote precede confirmation; rights/access/consent precede distribution or participants. The 60/300 subject ceilings are conditional, not power/funding commitments. The methods numerical envelope is not a blanket build budget. Keep the outside reviewer and 4–6 formative sessions within existing ceilings; actual recruitment remains separately authorized.

Deliver the partition/contracts/manifest/coverage/ADR ownership and human/gate plan, the independent review/dispositions, the exact baseline/tool settings and a readiness packet listing every unmet prerequisite. Run the external build-memory/backlog and repository documentation/row validators appropriate to those changes. Readiness also requires the separately built/tested retained guard and actual dispatch integration; authoring contracts alone cannot earn it. Research, portability, publication and adoption remain separately evidenced outcomes.

Open the intermediate decomposition results in Zed for the operator before implementation orchestration, as requested. Do not infer permission to start `orchestrate-build` from Q-7 or from a completed decomposition. The current synthesize-spec phase ends with this frozen handoff.
