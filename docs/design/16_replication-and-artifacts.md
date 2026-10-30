# 16_replication-and-artifacts — Recipient procedures and bounded access

Stream: F — Receipts and maintenance\
Researched: 2026-10-29 UTC\
Author: /root/f3_replication\
Ledger rows covered: F3\
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; approved planning and accepted dependencies.\
Confidence overall: medium; source-backed boundaries, unexecuted design.

**Recommendation:** one outside reviewer verifies and recomputes a rights-cleared package from a clean installation. Paid fresh replication is separately quoted and contingent. An author's clean directory does not satisfy outside participation. No recipient procedure, download, task, provider, container or publication was executed here.

## Basis

Read [brief](../brief.md), [ledger/Q](../research-ledger.md), [conventions](../research/CONVENTIONS.md), [trace](../OUTLINE_TRACE.md), root/docs instructions and `synthesize-spec` run mode at skills revision `e981ec57d487a2c41d416672483b5a4df15b1233`. Accepted inputs: [D2](10_compaction-protocol.md), [D3](11_operational-proof-protocols.md), [E2](12_feasibility-findings.md), [F1](13_receipt-contract.md), [G1](17_reader-journey.md), bounded [A2 F7](../research/02_evidence-audit.md#f7--bundle-reproduction-is-r0-and-replay-is-not-a-fresh-execution) and [A3 F5–F7](../research/03_newcomer-journey.md#f5--replay-is-useful-but-whole-stream-byte-equality-is-the-wrong-tutorial-assertion). Source checkpoint **2026-10-29T20:52:33Z**; hosting pages fetched **2026-10-29T20:51:46Z–20:51:53Z**, undated current documentation. Inspection is not execution evidence.

## Material findings

### F1 — Each procedure has its own promise
Claim: Recipients need separate procedural and scientific verdicts.
Status: PARTIALLY VERIFIED\
Evidence kind: source inspection; design proposal\
Evidence: [Frozen §10.4](../../spec/sections/10-evaluation-reproducibility-plan.md#104-the-reproducible-harness-bundle-and-reproducibility-levels) defines R0–R3/independence; [levels.rs](../../crates/hh-bundle/src/levels.rs), 1–16, makes levels noncumulative; F1 F5–F7/A3 F5–F6 retain closure/lifecycle gaps.\
Retrieved: 2026-10-29T20:52:33Z.\
Implication for the spec: Publish separate procedure cards/reports.\
Outline delta: CONFIRMS OL-07; EXTENDS OL-15–OL-18, OL-21.\
Limitations: No current package is certified; baseline R0 drift is unrepaired.

| Job | Successful result establishes | Does not establish |
|---|---|---|
| Inspect | Named accessible claims/events/limits can be located. | Integrity or truth. |
| Verify; optionally R0 | Specified integrity/schema/chain/population/closure checks; R0 additionally full native validation. | Grade truth, task success or R1. |
| Recompute | Calculations on supplied observations agree. | Authentic collection/oracle or efficacy admission. |
| Replay; optionally R1 | Recorded decision/assembly agreement; R1 also delete/rederive equal hashes. | Fresh solving or arbitrary deterministic behavior. |
| Fresh rerun; optionally R2/R3 | New observations and registered comparisons. | Independence from an empty store or ungated component benefit. |
| Outside replication/transfer | An outside party's separately declared comparison. | An automatic R3 badge, native “replicated” status or generalization. |

Keep evidence class, claimed/static maximum/requested/achieved R level, completeness, recipient closure, native verdict and scientific admission separate. R3 is not cumulative R2. A2's historical R0 transcript and A3's later drift coexist; neither supplies missing native bytes. Exit zero may only mean a report was delivered.

### F2 — Handoff must survive producer cleanup
Claim: Verification needs a closed package and an installation unable to borrow producer state.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: F1 F2–F5 owns membership/typed closure; E2 F6 identifies blob/R0 repairs; G1 F1/F4 requires direct evidence entry.\
Retrieved: 2026-10-29T20:52:33Z.\
Implication for the spec: Test the artifact actually distributed.\
Outline delta: EXTENDS OL-07, OL-16–OL-18, OL-20.\
Limitations: Installation, rights and proposed profile support remain gates.

**Minimal handoff:** claim/scope and procedures; native/member/transport identities; code revision/dirty patch; toolchain/features/locks/platform; executable recipe/expected semantic fields; required bytes/access/expiry/losses; tolerances/resources; negative controls; failure-report route; owner/retention dates. Reuse F1's profile. Archive checksums never replace native `idp/1` identities.

Proposed sequence, to become tested E3/F2 commands:

1. Select package, recipient class and procedure; inspect losses/version/resources before fetching. Acquisition/inspection never executes bundled code.
2. Acquire a pinned release or separately build from source/locks. Record binary/source/toolchain/OS/architecture and signer. Separate setup downloads/time/disk from procedure cost. Missing supported recipe gives `unavailable{installation}`.
3. Start empty recipient store/workspace, with no producer home/cache/mount/credential/path fallback. Materialize only authorized inventoried members; verify bytes. Disable network for verify/recompute/replay. Extraction rejects escaping paths/links and oversized expansion.
4. Run native staged validation plus F1 typed procedure closure. Retain every stage, unknown schema and missing member. Reconcile planned slots→attempts→selected grades→rows→analysis, watermarks and causes without shrinking denominators.
5. Run the selected procedure; retain raw report/output, exit/signal and semantic result. Identity mismatch stops; missing rights/inputs yields unavailable/refused. G2 supplies the optional permitted-report route.

These are **unimplemented recipe requirements, not new `hh` commands**. E3/F2 must pin supported CLI/embed/runner entry points and argument files and exercise the exact printed recipe. Until then invocation is `not_run{recipe_unimplemented}`; no current `hh replicate`/`hh recompute` interface is claimed.

### F3 — Public recomputation validates arithmetic
Claim: Complete analysis inputs permit recomputation without proving hidden raw history or oracle truth.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: F1 F3/F4/F6 supplies selectors/values/losses; D2 F3/F5–F8 owns finite-task methods, missingness and numerical checks.\
Retrieved: 2026-10-29T20:52:33Z.\
Implication for the spec: Package a rights-cleared analysis view with explicit linkage limits.\
Outline delta: EXTENDS OL-10–OL-11, OL-15–OL-16, OL-20.\
Limitations: Arithmetic agreement does not validate collection/grades or award R1.

Verify inputs → reconstruct frozen selection/grade revisions → reconcile all planned slots → run pinned analysis → compare named outputs. Retain per-task/arm observations, availability/numerators/denominators, weights, methods/prior/seeds/draws, interval types, full multiplicity family and prices for currency claims. Recompute counts, unknown bounds, raw/joint veto endpoints, estimates/intervals and refusals; include failures, exclusions and unrun slots.

Public opaque IDs require leakage/rights review. A withheld native-to-public mapping prevents public verification of that join. Do not expose hidden prompts, solutions, oracle bodies, model I/O or guessable sensitive hashes to improve closure. If row-level inputs cannot be cleared, that study's public recomputation is unavailable; a separate synthetic lesson does not fulfill its real-data promise.

Identities, counts, membership/order, missingness, categorical verdicts and native derived hashes require **exact agreement**. Same pinned numerical implementation/seed is the reference. Proposed portable scalar allowance: `abs(new−ref) ≤ 1e−10 × max(1,abs(ref))`, frozen/tested before recipient execution. D2 numerical-error records remain required. This is not a scientific equivalence margin or permission to change draws/seeds. Nonfinite values or changed threshold/verdict fail even within tolerance; compare full precision before formatting. If an admitted method cannot meet this allowance, review a justified pre-execution revision; never widen it after a mismatch. Missing inputs/unsupported estimator refuse rather than silently change methods.

### F4 — Replay and R1 require controlled derivation
Claim: Replay compares recorded decisions while independently detecting accidental live work.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: A3 F5 records replay markers/no new model/effect rows; frozen §10.4 requires R1 delete/rederive; D3 F2 supplies controls; F1 F5 requires closure.\
Retrieved: 2026-10-29T20:52:33Z.\
Implication for the spec: Test replay and R1 separately.\
Outline delta: EXTENDS OL-07, OL-12, OL-15–OL-18.\
Limitations: Fixture replay does not prove arbitrary variants or external-effect recovery.

After verification, copy source evidence into disposable scratch; inventory its prefix/head and derivations. Require supported declaration, recorded responses, definitions and compiled inputs. Disable provider transport/effect execution and independently count attempted dispatches/observe the sink. Compare declared decision/assembly sequence and original prefix exactly; permitted replay markers may append, so whole augmented-ledger equality is wrong.

R1 additionally deletes required derived objects **only in that disposable copy**, rederives from the authoritative prefix, compares every required hash and replays where declared. Preserve original export. Missing response, changed derivation, unsupported variant or unexpected live attempt fails/refuses the claim; blocked live attempts still fail acceptance.

R0 separately needs D3's successful/failed/open cases and tampered bytes, missing member and re-sealed incorrect-lifecycle controls. Expected state is `open|finished`, independent of task reward/stop/veto. Require repaired semantic comparison, not exit zero; retain A3 drift. A partial public package may verify accessible members while native R0/R1 remains unavailable.

### F5 — Fresh execution preserves admission and independence
Claim: Fresh replication cannot repair absent scientific admission by terminology.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: Frozen §10.4 defines R2/R3/transfer; D2 F4/F6–F8 retains parity/finite-task limits; D3 F3–F5 scopes trials; E2 F1–F5 identifies missing bindings.\
Retrieved: 2026-10-29T20:52:33Z.\
Implication for the spec: Admit each route before dispatch; preserve negative/inconclusive comparisons.\
Outline delta: EXTENDS OL-09–OL-12, OL-15–OL-16, OL-18, OL-20–OL-21.\
Limitations: No selected task/model or execution permission; R=5 repetitions do not establish power/equivalence.

Preflight task/oracle/image/runner rights/pins, model/seed availability, isolation, grading controls, treatment-to-wire, Account caps/clock, detectors, parity and implemented analysis. Missing snapshot/service means unavailable with partial costs retained. Changed model/task/oracle/environment needs a distinct drift/transfer design, never an unannounced replacement. A single fresh demo creates a new subject/grade but supplies no R2/R3 by itself.

| Route | Required procedure / comparison |
|---|---|
| Native R2 | Pinned configuration/model/environment, recorded seeds, named validators, preregistered margins/minimum n and all native basis; paired reruns. Reproduction reruns are instrument-charged under §10.4, retaining subject usage attribution. |
| Native R3 | Registered seed distribution/design, n≥k, MatchSpec, admitted equivalence/margins; observed before/after fingerprints where model unpinned. Preserve native verdict, interval target/type and refusals. |
| Outside D2 | New phase/protocol/attempts; same finite frozen-task target, assignment, caps, first-slot selection and full analysis. Compare original/new observed and inferred contrasts separately; no favorable seeds or cohort pooling. New-task transfer asks a different question. |
| Outside D3 | Rebuild confined apparatus/observer, positive and seeded-defect controls; eight cells ×3 with reached/observable/violation counts. Twenty-four cases do not prove population reliability, exactly-once or general containment. |

**D2's conservative parity at R=5 repetitions cannot meet ±10 percentage points even with perfect agreement.** Fund adequate parity or admit a reviewed alternative before component confirmation; otherwise preserve product smoke/mechanism scope. Outside arithmetic/reruns do not upgrade it. Preserve finite-task binary inference versus task-mixture cost/rate uncertainty. Matching signs/overlapping intervals prove no general benefit; a scientific replication claim needs its own frozen practical criterion, admitted uncertainty and multiplicity. Null/negative/inconclusive remains valid research.

Native independence requires distinct installation and signer evidence. Outside participation additionally records relationship to original authors/implementation/analysis, assistance, own execution authority and attestation, with minimal personal data. Another author-controlled key/directory is insufficient. Minimum: one consenting outsider completes verification/recomputation, including a negative report if encountered. Failure completes a review attempt but leaves portability unmet until repair/recheck. G3 owns recruitment/consent; this note authorizes no outreach.

### F6 — Q-5 has a bounded storage/access default
Claim: Public metadata and controlled permitted payloads can support a small review package with explicit losses.
Status: UNVERIFIED\
Evidence kind: design proposal; primary documentation inspection\
Evidence: F1 F6 owns rights/sanitization; [GitHub releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases) requires files under 2 GiB; [S3 versioning](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Versioning.html) retains charged full versions; [SSE-S3](https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingServerSideEncryption.html) encrypts data at rest, not metadata; [presigned access](https://docs.aws.amazon.com/AmazonS3/latest/userguide/using-presigned-url.html) uses reusable bearer URLs with expiry/credential limits.\
Retrieved: 2026-10-29T20:51:46Z–20:51:53Z; undated current pages.\
Implication for the spec: Accept/revise this default before hosting/publication commitments.\
Outline delta: EXTENDS OL-15–OL-16, OL-18, OL-20–OL-21.\
Limitations: No rights clearance, account, price quote or service guarantee exists.

| Layer | Proposed first-review default |
|---|---|
| Public | Existing Git for permitted index/code/recipes; GitHub Release assets ≤512 MiB each, package ≤2 GiB compressed/8 GiB expanded, first-wave aggregate ≤10 GiB. Program caps, not platform entitlements. Retain permitted metadata/reports five years and assets two years from publication; review 90 days before expiry. |
| Controlled | Encrypted local staging remains available. After operator hosting approval, private versioned S3 with public access blocked, SSE-S3/TLS, opaque metadata and recorded region. Selected review payload ≤20 GiB; S3 ≤32 GiB including versions/logs/derivatives. No mandatory account or runtime dependency. |
| Retention | Permitted controlled review originals: 12 months from collection closure, actual expiry stated before invitations. Retain local encrypted original and separate encrypted backup within the approved storage quote; no deletion of required evidence during a promised review window. |
| Access | Designated maintainer/private contact; target response five business days, no service promise. Record procedure/member request, eligibility/rights decision/uses. Grant ≤30 days, renewable; read-only object/version links expire after one hour. No recipient write/delete/list privileges. |

These quotas cover **selected publication/review packages, not the full study archive**. Before collection quote storage/backup/version overhead for D2's 50 GiB pilot plus 200 GiB confirmation and D3 artifacts; preserve complete originals separately. A smaller review package cannot promise full raw closure. Reserve capacity or revise scope before dispatch; never discard failed attempts to fit. Quote region-specific storage/requests/egress/logs/retention before hosting activation. Quotas are not cloud billing hard stops; no spend is authorized.

If native fetch cannot authenticate this store, use authorized manual materialization followed by native identity verification; do not invent transparent fetch support. Bearer URLs stay outside bundles/reports/logs and are delivered privately; record permitted grant IDs only. Links are reusable, may expire early and cannot recall downloaded copies. Rights/terms must cover local retention/onward sharing; denied/expired access is an explicit unavailable result.

Unknown rights means withheld. Held-out solutions/oracle material/model I/O remain controlled unless contamination policy permits release. Secrets invalidate export. Sanitization creates new member/bundle IDs, transform/version, permitted linkage and per-procedure loss; changed bytes cannot keep old native hashes. Review public numerical rows too. Loss makes dependent procedures unavailable even after a prior pass: no universal complete-bundle promise. Record superseding status/tombstones; shorter rights obligations override retention with an honest loss record. At expiry review renew/delete, including noncurrent versions/backups, rather than assuming a current delete marker removed bytes. Preserve only permitted loss metadata.

### F7 — Costs and adverse cases make portability assessable
Claim: A no-provider review can be bounded without funding a fresh study.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: G1 F4/F5 separates receipt participation from task completion; D2 F8/D3 F5 require costs/stops; F1's battery preserves inaccessible inputs/negative results.\
Retrieved: 2026-10-29T20:52:33Z.\
Implication for the spec: Advertise procedures only after recipient tests.\
Outline delta: EXTENDS OL-07, OL-15–OL-18, OL-20.\
Limitations: Envelopes are proposed, not measured performance/prices.

Minimum public verify/recompute: pinned prebuilt tools, ≤2 cores/4 GiB RAM, ≤2 wall-hours/4 core-hours, ≤12 GiB scratch including ≤2 GiB download/8 GiB expansion. **$0 provider fees**; local time/power/network/storage still cost resources. Source setup separately proposes ≤4 cores/8 GiB RAM, ≤2 hours/8 core-hours/20 GiB disk and needs a tested recipe. Stop/requote on breach; no automatic installs/container pulls. Larger controlled reviews need their own envelope. Record setup/procedure costs separately, actual or unknown.

Fresh execution requires a quote for planned subjects, all roles/calls/tokens, retries/regrades, environment/image acquisition, instrument time, storage/egress/cancellation exposure, currency ceiling and local limits. D2/D3 ceilings require their approvals; unused pilot allowance does not fund replication. Absent funding/finite enforceable exposure means zero dispatch. Public readers use their own resources under documented prerequisites; internal Q decisions govern program spending/publication.

Minimal proposed result fields, reusing native reports rather than another authority:

```text
bundle_id, profile_version, procedure, recipient_class, checked_at
installation_ref, signer_ref, outside_relationship, assistance
input_refs, required/resolved/withheld/unresolved, closure_report_ref
recipe_revision, invocation_status, exit/signal, native_report_ref
expected/observed_fields, tolerances, mismatches, semantic_verdict, reason
claimed/static_max/requested/achieved_level, independent, scientific_admission
resource_caps, actual_or_unknown_costs, output_refs, losses, followup
```

Invocation starts `not_run`; wrapper semantic verdict is `pass|fail|refused|unsupported|skipped|unknown` with reason, retaining native verdict separately. Correct refusal does not make a requested procedure available.

**Future tests, unexecuted:** run after producer store/caches/credentials are inaccessible.

| Case | Required result |
|---|---|
| Rights-cleared package/clean installation | Promised members verify and calculations agree; hidden truth checks stay unavailable. |
| Missing payload-only response, grade selector, weight/price | Affected closure/recompute/replay fails/refuses; no fallback or invented zero. |
| Wrong bytes/mandatory profile, unsafe archive | Reject without execution; preserve diagnostics. |
| Wrong lifecycle/failed/open subject/replay markers | D3 R0 controls; preserve prefix while allowing declared markers. |
| Changed derivation/input or attempted live dispatch | R1/replay mismatch/refusal; independent observer detects attempts. |
| Withheld/expired/deleted/sanitized member | Rights enforced; new losses/identity; dependent procedures unavailable. |
| Omitted failure, selected retry, changed numerical verdict | Reconciliation/comparison fails; all planned denominators retained. |
| Retired model/changed oracle/R=5 repetitions parity/missing detector | No silent substitute/component claim; refuse or separate approved transfer. |
| Same-author directory versus outside installation/signer | Repeatability/outside report distinct; participation cannot be self-awarded. |

## Answer and completion assessment

F3 supplies recipient steps, prerequisites/refusals, tolerances, costs and a bounded Q-5 recommendation. First-wave outside verification/recomputation and contingent fresh replication remain separate. This does not replace the central real comparison or parity gate. Implementation/rights/funding block execution claims, not completion of this design.

## Open questions

| Decision | Owner / affected units | Operator boundary |
|---|---|---|
| Accept GitHub/optional S3 quotas, retention/access owner, region and rights-cleared package; quote full study retention separately. | Q-5; F1/G2/G3/H2 | Before hosting/distribution commitments. |
| One outside verify/recompute review first; fresh replication separately quoted. | Q-6; G2/G3/H2 | Recruitment/publication/funding; no outreach now. |
| Admit recipes, numeric allowance, closure/R0/independence checks. | E3/F2/H1 | Engineering/ADR before capability promises. |
| Select pins/margins/methods/parity and new-run ceilings. | Q-3/Q-4; D2/E3/H2 | Before execution; R=5 repetitions blocker retained. |

## Spec requirements emitted

Candidates only; F1 owns the profile and D2 statistical mathematics.

| ID | Candidate | Source / outline | Acceptance |
|---|---|---|---|
| REQ-F-201 | Separate procedure/level/verdict/claim. | F1; OL-07/15–16 | R0 failure/replay/fresh counterexamples. |
| REQ-F-202 | Pinned handoff and closure without producer fallback. | F2; OL-16–18/20 | Clean installation after cleanup. |
| REQ-F-203 | Recompute frozen fields with exact/numeric rules. | F3; OL-10–11/15–16 | Missing selectors/weights/threshold cases. |
| REQ-F-204 | Isolate replay/R1 and detect live attempts. | F4; OL-07/12/16 | Hash/prefix/tamper/dispatch controls. |
| REQ-F-205 | Gate fresh R2/R3/transfer and scientific scope. | F5; OL-09–12/16/21 | Missing pins/parity/methods refuse. |
| REQ-F-206 | Require outside execution/report for participation. | F5; OL-16/18 | Outside review; same-author negative. |
| REQ-F-207 | Bound rights/storage/access/retention and losses. | F6; OL-15–16/20–21 | Expiry/redaction/version-deletion cases. |
| REQ-F-208 | Report cost/stops/adverse results; no automatic spend. | F7; OL-16–18/20 | Caps/unknown costs/zero-budget refusal. |

## Handoff

E3/F2 implement owner-based recipes; G2 report route; G3 authorized outside review/publication; H2 retains Q-3–Q-6 and parity/rights limits. No new portfolio/implementation row. F1 membership, D2 methods, D3 controls and G1 branches preserved. No central/build-state/spec edits, commits, installs, downloads or task/provider execution. Coordinator owns acceptance/state.

Validation: deferrals-first empty; planning validator passed with zero violations/warnings (locale fallback). Scoped check: 13 relative links/3 anchors, seven findings × eight fields, eight unique requirement IDs and four closing sections; zero errors. Added-file diff reviewed; whitespace check clean (no-index diff exit 1 means new content). Nine proposed recipient case groups remain unexecuted; zero Rust/runtime tests or builds. Only this note written.
