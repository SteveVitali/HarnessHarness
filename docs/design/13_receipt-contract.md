# 13_receipt-contract — Native evidence packages and bounded claims

Stream: F — Receipts and maintenance\
Researched: 2026-10-29 UTC\
Author: /root/f1_receipts\
Ledger rows covered: F1\
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; approved uncommitted planning and accepted dependencies.\
Confidence overall: medium; ownership is source-backed, proposed profile/procedures are unimplemented.

**Recommendation:** a native bundle profile with immutable attempts, all-planned outcomes and tested recipient closure. Preserve existing authorities/evidence classes/R0–R3. Q-5/Q-6 remain decisions, not publication authorization.

## Basis and scope

Read brief/ledger/conventions/trace, root/docs instructions, [A2](../research/02_evidence-audit.md), [A3](../research/03_newcomer-journey.md), [C1](06_demo-and-study-portfolio.md), [C2](08_feasibility-and-scope.md), [D1](09_measurement-standard.md), [E2](12_feasibility-findings.md), and `synthesize-spec` run mode (skills `e981ec57d487a2c41d416672483b5a4df15b1233`). Retrieval checkpoint **2026-10-29T20:33:48Z** is inspection, not execution.

## Material findings and proposed contract

### F1 — Native owners supply the index without another authoritative store
Claim: Package native durable facts and derived views through `hh-bundle`, retaining their existing owners.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in source inspection\
Evidence: [manifest](../../crates/hh-bundle/src/manifest.rs), `MemberRef`/`LedgerExport`/`SubjectSection`; [identity](../../crates/hh-identity/src/lib.rs), CC1; [Results projection](../../crates/hh-results/src/projection.rs), 562–598, pins sources.\
Retrieved: 2026-10-29T20:33:48Z.\
Implication for the spec: E3/H1 admit profile/owner changes through ADR; no shadow store.\
Outline delta: EXTENDS OL-16, OL-18, OL-20–OL-21.\
Limitations: Current results can be empty; proposed metadata is not implemented.

Proposed **`evidence-package/1`** is one typed member/role profile inside `hh-bundle/1`. `hh-bundle` owns codec/validator; identity/wire retain `idp/1`/canonicalization. Bundle `version_id` remains identity, archives transport. New kinds/domains/events need registry admission/ADR. Never store authoritative decisions only in this index/manifest extras.

| Fact | Authority / exporter input |
|---|---|
| Plans, launches/attempt bindings, settlement/closure | `hh-experiment` durable events; derive population index. |
| Prefixes, causes, payloads, blobs | `hh-ledger`; export pinned prefixes and required bytes. |
| Task/submission/oracle/grade attempt | `hh-bench` and existing instrument/measurement owners; E3 admits missing finalization/grade-attempt records. |
| Costs, rows, analysis | `hh-budget` Account, `hh-results`, `hh-analysis`/`hh-eval`; package raw inputs and exact derived versions. |
| Identity, provenance, access, publication | Existing identity/provenance/security/bundle owners; annotations never raise authority. |

Use owner-supplied typed closure inputs; no reversed dependencies, duplicate folds or new dependencies. Public boundary changes use `hh-embed-schema`→`hh-codegen`; profile changes need no standalone schema platform.

### F2 — Slots, subjects, grade attempts and exports need distinct identities
Claim: Derive immutable execution/grade coordinates from native bindings, and identify later exports as new snapshots.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: [experiment engine](../../crates/hh-experiment/src/engine.rs), 20–43; [Event/Head](../../crates/hh-ledger/src/event.rs), 166–205/307–317; A2 F2–F3 (overwrites/build-note hashes), E2 F2/F6 (grade/closure).\
Retrieved: 2026-10-29T20:33:48Z.\
Implication for the spec: Native coordinates, never date/leg filenames or provider IDs, are joins.\
Outline delta: EXTENDS OL-10, OL-15–OL-16, OL-18.\
Limitations: Missing historical coordinates cannot be reconstructed by assertion.

**Proposed fields:** `R` required even when partial; `C` required for dependent claim/procedure; `O` optional. Unknown carries reason, never zero. Refs retain native type/domain.

| Field/member | Required contract |
|---|---|
| `profile`, `source_versions`, `claims[]` | R: version; code commit/dirty-patch member or dirty unknown, runtime/toolchain/features; claim ID/text/scope/class/population and native/report refs. |
| `protocol_ref`, `phase`, `plan_ref` | R study: immutable design/preregistration/task-manifest; diagnostic/pilot/parity/confirmatory phase; task/split/stratum/arm/replicate/full-factor slot. Demo uses bounded case manifest, not invented preregistration. |
| `attempts[]` | R: plan ID, native attempt/launch binding, subject run if launched, ordinal, predecessor/reason, settlement. Binding is identity; ordinal orders. Prelaunch refusal has experiment event, no fake subject. |
| `grade_attempts[]` | C task claim: instrument run/event ID, subject/submission digest/oracle version, start/end/result refs, ordinal, supersession/selection rule. Grades/retries never create replicates. |
| `native_heads`, `watermarks` | R: `{run_id,seq,event_id,hash}` per included subject/experiment/instrument/overlay/lineage prefix and projection watermark. Last exported event agrees; later appends do not change snapshot. |
| `build_note_snapshot`, `legacy_receipts[]` | O: retrievable bytes/path/revision and digest algorithm; explicit build-note kind, never accepted as native head. |
| `supersedes`, `losses`, `access_policy` | R, empty lists allowed: reason/affected members/claims; rights, recipient class, retention. Regrade/correction/redaction produces a new bundle, preserving old history. |

Declarations precede dispatch durably; verify causality/order and available trusted clock/checkpoints. Hashes/self-reported time prove no authenticity. Identity paths are create-if-absent; conflicting bytes refuse. Latest pointers are derived; export retries add no subjects. Later grades use overlays, never reopen finished WALs.

### F3 — Closure covers every planned slot and every observed outcome
Claim: Completeness is relative to a frozen population/watermark, retaining failures, unlaunched slots and unknowns.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: D1 F1–F2/F5/F7 (denominators/cost); E2 F2/F4 (settlement/timing); A2 F4–F6 (task failures/skips inside infrastructure passes).\
Retrieved: 2026-10-29T20:33:48Z.\
Implication for the spec: Export/process success cannot substitute for collection completion or task success.\
Outline delta: EXTENDS OL-10–OL-11, OL-15–OL-16, OL-20.\
Limitations: No validator discovers executions absent from every supplied authority or proves oracle truth.

Required `collection`: frozen slot set/hash/count, close reason/watermark, planned/launched/first-subject/all-attempt/gradeable/selected-grade dispositions and exclusions/policy/evidence. Reconcile slots→plans→launches→settlement→rows→analysis by **set equality**; retain orphans/deviations and partial `not_run{reason}` slots. No retrospective shrink, best retry or regrade-inflated n.

Per slot/attempt/subleg: prerequisite `available|unavailable|unknown`; invocation `not_run|started|finished|unknown`; process exit/signal/timeout; semantic `pass|fail|refused|unsupported|skipped|unknown`, with reason/detector/time/evidence. Keep native lifecycle/terminal/stop/outcome, raw reward, veto/availability and grade revision separate. A passing process cannot erase skips/format failure; missing oracle is not task zero.

Per metric retain membership/reasons, eligible counts, all-planned `s,f,u,M`, weights/bounds/censoring. Zero opportunities may be undefined; absent detector=`no_detector`. Regrades preserve failures/settlement. E3 resolves folds/timing; retain created/quiescent/verification/finished boundaries without subtracting verification from frozen wall time.

### F4 — Pins, raw observations and analysis inputs precede conclusions
Claim: Claim references must resolve to exact factors, raw evidence and versioned analysis inputs.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: [§10.1–10.4](../../spec/sections/10-evaluation-reproducibility-plan.md); [GradeResult](../../crates/hh-bench/src/grade.rs), 110–145; D1 F3–F8/E2 F3–F5 (fidelity/enforcement/method gates).\
Retrieved: 2026-10-29T20:33:48Z.\
Implication for the spec: Capture native evidence; do not reconstruct plausible missing observations.\
Outline delta: EXTENDS OL-05, OL-09–OL-10, OL-15–OL-16, OL-20.\
Limitations: Pins guarantee neither availability, grading validity nor universal physical-cost truth.

| Members | Required content / condition |
|---|---|
| Task/environment | R: suite/task/version/source, split/stratum/validity/contamination, three task-surface identities/submission kind; image digests, runner/platform/dependencies, fault/network/isolation/cache policies. Restricted bodies explicit; tags/foreign digest claims unpinned. |
| Models/roles/treatment | R: requested/served snapshots/fingerprints, provider/role/seed support/material, routing/fallback/cache, sealed config/registry/variants/parameters. C treatment claim: trigger/opportunity→invocation→retained/omitted IDs→actual dispatched payload/response and call joins. |
| Oracle/observations | C task claim: pinned oracle/validator/calibration, invocation/executable/environment, submission/application evidence, raw stdout/stderr/reward/errors/exit/timeouts, mapping version, isolation/access/egress/veto evidence. Retain empty/nonfinite output as errors; judged/human values separate. |
| Accounting | R: eval/search/instrument budgets, MatchSpec, per-dimension enforcement evidence, MeteringFormula/price snapshot/confidence; role/call/transport-attempt, reservations, actual primary usage/failed usage/unknown exposure. Account owns deduplication; token totals derived. Retry/regrade/preparation/environment/storage outlays measured or marked unmeasured; logical reverted-effect totals distinct from incurred/billed use. |
| Rows/analysis | C quantitative claim: ResultsRow versions/overlay watermarks, AnalysisSpec/record/report ID, executable/code/dependency lock, registry/oracle/prices, selected/excluded slots/runs/grades, raw values/numerators/denominators/reducers/task weights. Estimand/units/direction, estimator/version/assumptions/prior, seed/draws/floors/refusals, interval type/level, bounds/tails/margins and entire frozen multiplicity family including unavailable members. |
| Operational proof | C: planned/reached fault/attack opportunities, actual injection/deny/permission/exhaustion and independent sink observations; expected/observed/duplicate/missing/unknown effects, fault/threat assumptions. |
| Presentation | O: transcripts/screenshots/annotations/foreign export, exact event links and export losses; cannot replace required evidence. |

Claims bind admission/parity/budget/validity prerequisites and actual verdicts. Hashes do not prove truth; model aliases do not pin weights. Unsupported methods/unobserved costs remain unavailable.

### F5 — Recipient closure requires typed traversal and verified bytes
Claim: Static declared completeness and recipient-resolvable procedure closure are distinct checks.
Status: VERIFIED\
Evidence kind: source inspection\
Evidence: [export](../../crates/hh-bundle/src/export.rs), 95–151, indexes blobs without adding bytes; [levels](../../crates/hh-bundle/src/levels.rs), 1–16/53–65, accepts present/fetch declarations; [validate](../../crates/hh-bundle/src/validate.rs), 1–39, distinguishes unmaterialized; [fetch](../../crates/hh-bundle/src/fetch.rs), 1–27/76–104, verifies arrival/trust; E2 F6 identifies typed-payload gaps.\
Retrieved: 2026-10-29T20:33:48Z.\
Implication for the spec: Add procedure-scoped closure without redefining existing completeness/level semantics.\
Outline delta: EXTENDS OL-07, OL-16, OL-18, OL-20.\
Limitations: No export/fetch executed; this does not establish every bundle's contents.

Traverse claim/procedure roots→native heads/manifests/pages/causes/lineage/`refs[]`/IR refs/**typed payload refs**→task/submission/grade/rows/analysis. Owning codecs enumerate fields, including response bytes; no hash regex/second event table. Deduplicate with visited sets; retain parent/role/field/media/size. Unknown relevant schema or traversal-limit exhaustion fails closure.

Every required edge resolves to member or explicit `fetch|redacted|gc`; missing/unknown fails, never fabricates an address. `present` requires verified recipient bytes. Fetch requires identity, admitted locator/trust/expiry/rights and fresh-pool arrival under existing durable rules. No producer-path fallback/credential URLs; expired/unsupported fetch refuses. Fetch never executes.

Report `{bundle_id,profile_version,procedure,recipient_class,checked_at,required,resolved,withheld,unresolved,verdict,failures}`; sidecars cannot raise levels. Test after producer cleanup without home-store fallback; verify head/page/cause/watermark consistency. Metadata may verify while dependent procedures remain unavailable.

### F6 — Access and sanitization must identify exact claim losses
Claim: Default to public permitted metadata with separately governed payload access and per-procedure loss reports.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: Frozen §10.4 prohibits secret-bearing bundles, restricts contamination-sensitive material and requires loss/supersession; A2 F1/F3 limits transcript hashes to redacted bytes; C2/Q-5 preserves rights/resource decisions.\
Retrieved: 2026-10-29T20:33:48Z.\
Implication for the spec: Q-5 chooses concrete rights/hosting/retention before dependent commitments.\
Outline delta: EXTENDS OL-07, OL-15–OL-16, OL-18, OL-20–OL-21.\
Limitations: No license/privacy/provider-terms clearance or publication occurred.

Recommend permitted metadata/code/analysis in Git, large immutable blobs in a selected object/release store, restricted originals in controlled encrypted storage. **No secret-bearing bundles.** Held-out solutions/model I/O default restricted. Per item record source/version, license/consent evidence, recipients/uses/owner/decision; unknown rights=withheld. Q-5 chooses host/quota/retention dates/contact.

| Tier/change | Procedure scope/loss |
|---|---|
| Public metadata | Permitted pins/counts/rows/analysis/loss inventory; restricted bodies may prevent independent grade/fidelity inspection. |
| Controlled evidence | Authorized recipients receive permitted raw hashes/bytes with audited access/expiry. Closure belongs to recipient class/time, not the public. Recommend retaining through the agreed review/replication window; duration unresolved. |
| Sanitized derivative | New bundle/member IDs, transform version/mapping/loss. Sanitized transcript digest proves only those bytes. Altered event bytes cannot retain original chain/hash or native R1 claim; inaccessible original is a commitment, not verified history. |
| Expired/deleted/withheld | Permitted tombstone/reason/time/policy/affected claims in superseding status. Required-input loss makes dependent procedures unavailable; arithmetic may survive if all row inputs remain. |

Missing wire loses exact fidelity inspection; submission/oracle loses grade checking; grades/selectors/weights loses recomputation; model/image access loses fresh execution. Re-evaluate every basis/procedure; restricted completeness never transfers publicly. Minimize personal paths/IDs/sensitive low-entropy hashes; disclose unverifiable linkage. Provenance never requires retaining forbidden bytes.

### F7 — Preserve legacy evidence and separately test stronger claims
Claim: Keep historical receipts readable and unchanged while requiring new-profile validation for new closure/attempt claims.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: [live emitter](../../scripts/live-verify.sh), 1513–1539/1622–1626, hashes a build note and overwrites date/leg paths; frozen §10.4 fixes R0–R3/independence/supersession; A3 F5–F6/E2 F6 identifies replay markers and R0 lifecycle mismatch.\
Retrieved: 2026-10-29T20:33:48Z.\
Implication for the spec: Version claims and test semantic verdicts, not merely process exit.\
Outline delta: EXTENDS OL-07, OL-15–OL-18, OL-21.\
Limitations: No procedure executed here; self-consistency is not independent authenticity.

`hh-live-evidence/1` retains ADR-0363 validation/`live-executed` scope. Reference exact bytes/revision; label `run_ledger_head` a legacy build-note snapshot in new views. Never rewrite old receipts/spend, infer native heads/missing outcomes or upgrade evidence. Fixture/foreign/live, R level, closure and scientific admission remain separate. Future collector/native-link changes need ADR.

| Procedure | Required demonstration / limit |
|---|---|
| Inspect/verify/R0 | Named member access and identity/chain/schema/closure results. Native R0 full validation compares `open\|finished`, separately from task terminal/success. |
| Analysis recomputation | Supplied rows/selectors/weights/registry/prices/code reproduce named fields under declared exact/numerical tolerances. Does not validate collection/oracle or automatically award R1. |
| Replay/R1 | Native delete/rederive and declared decision comparisons; separate source prefix from new replay markers; observe no unintended model/effect work. Replay alone does not satisfy all R1 basis. |
| Fresh execution/R2/R3 | Fresh run alone awards no level. R2 needs pinned config/seeds/validators/margins/repetitions; R3 registered distributions/seeds/fingerprints/MatchSpec. Costs, nondeterminism and availability explicit. |
| Independence/replication | Distinct installation/signer evidence for native independence; F3 defines procedure. Transfer/replicability is distinct from R3/status. One author's clean directory is not outside participation. |

Procedure record: prerequisites/command/version/inputs/comparisons/tolerances, ceilings/actual or unknown cost, exit **and semantic verdict**, independence/time/report. Proposed/supported=`not_run`. Claimed/static maximum/requested/achieved level/verdict differ; R3 is not cumulative R2. Unavailable services refuse.

Optional minor additions cannot strengthen old claims; changed requirements/semantics need new major/ADR. Unknown major/mandatory feature refuses new claims, preserving legacy inspection. Ignoring a member cannot award its claims. Keep frozen amendment/generation rules.

## Proposed validator and failure battery

Extend staged native validation; report all applicable checks, distinguishing invalid, partial and unavailable. **Future tests, unexecuted:**

| Case | Required result |
|---|---|
| Same-day retries, multiple grades, export retry/collision | All attempts retained, one first-subject slot denominator; export does not add n; unequal bytes at same ID refuse. |
| Deleted slot/orphan/duplicate grade; format failure/skipped oracle | Reconciliation/selection fails or explicit partial/deviation; separate verdicts; no favorable shrink or fabricated zero. |
| Build-note substituted for native head; stale watermark | Typed coordinate/head/projection mismatch. |
| Missing `refs[]` blob or payload-only response | Closure fails after producer cleanup. |
| Expired/unauthorized fetch or wrong bytes | Typed fetch/hash failure; recipient-specific loss. |
| Changed weights/family/seed/prices/draws | New analysis/bundle identity; old report unchanged. |
| Hierarchical charges/failed calls/reverted effects/unknown bill | One owning total; physical outlay separate; unknown not zero/exact. |
| Redaction/GC/secret-bearing export | New losses; forbidden export invalid; dependent procedures unavailable. |
| R0 success/failure/open/tampered state | Native lifecycle semantics; wrong state/hash fails despite exit 0. |
| Legacy/unknown profile; negative report/missing model | Legacy readable, no new award; explicit refusal/unavailability and negative evidence retained. |

## Answer and completion assessment

F1 settles membership/identity/closure/compatibility design. Grade/accounting/R0/closure implementation remains gated; task/model/method/hosting choices stay explicit dependencies. No capability upgrade is claimed.

## Open questions

| Recommendation / question | Owner / consumers | Boundary |
|---|---|---|
| Approve public metadata + controlled permitted payloads; select host/quotas/retention/rights review. | Q-5; F3/G2/G3 | Before collection/distribution commitments; no publication authorization. |
| Start with one independent clean-environment package review, then separately budget fresh replication if useful; retain negatives/losses. | Q-6; F3/G2/G3 | Recommendation, not outreach/release/spend permission. |
| Admit profile/grade records/typed traversal and resolve frozen timing branch. | E3/H1; D2/D3/F2/F3 | Technical/ADR gate before implementation. |
| Select pins, estimators and resource ceilings. | Q-3/Q-4; D2/E3/F3 | Freeze before dependent collection. |

## Spec requirements emitted

Candidates only; H1/O1 deduplicate with A2/D1/E2 without changing existing IDs.

| ID | Requirement / rationale | Findings / outline | Acceptance |
|---|---|---|---|
| REQ-F-001 | Minimal native profile; derived index, existing authorities. | F1; OL-16, OL-20–OL-21 | Regenerate from sources; unknown profile refusal. |
| REQ-F-002 | Immutable slot/attempt/subject/grade/export identities and native heads. | F2; OL-10, OL-15–OL-16 | Collision/retry/regrade/head-substitution cases. |
| REQ-F-003 | All-planned reconciliation; separate prerequisite/process/task/veto verdicts. | F3; OL-10–OL-11, OL-15 | Partial/orphan/format-failure/skip cases. |
| REQ-F-004 | Pins/raw grades/errors/usage/treatment and versioned analysis. | F4; OL-05, OL-09–OL-10, OL-16 | Recompute weights/selectors/families; reject altered inputs. |
| REQ-F-005 | Native/typed reference traversal and recipient procedure closure. | F5; OL-07, OL-16, OL-18 | Missing blob/payload/fetch/tamper/cleanup cases. |
| REQ-F-006 | Rights/access/retention and per-claim loss; sanitization supersedes. | F6; OL-15–OL-16, OL-20–OL-21 | Public/restricted/GC/redaction losses. |
| REQ-F-007 | Legacy compatibility; levels distinct from actual procedure verdicts. | F7; OL-07, OL-15–OL-17, OL-21 | Legacy/R0/unknown-profile regression cases. |
| REQ-F-008 | Claims bind tested prerequisites/population/evidence/limits. | F3–F7; OL-10–OL-11, OL-15–OL-16 | Unrun/unavailable/negative cannot become verified. |

## Handoff and verification

A2/D1/E2 reconciled. E3 owns interfaces/ADRs/timing; F2 examples; F3 procedures/cost; G2/G3 participation. D2/D3 retain method/opportunity choices. No portfolio/new-row change; Q-5/Q-6 unapproved.

Checks: deferrals-first passed (empty); planning validator passed, zero violations/warnings (locale fallback). Scoped validation passed: 18 relative links, 7 × 8 finding fields, 8 unique requirement IDs, 8 tables and four closing sections; no personal paths/whitespace diagnostics. Added-file diff reviewed; no-index exit 1 denotes new content. Only this note written. **0 builds, Rust tests, runtime/provider/task/oracle executions, uploads, installs or commits.** Central state remains coordinator-owned.
