# 18_contribution-path — Small contributions with checkable outcomes

Stream: G — Documentation and participation\
Researched: 2026-10-29 UTC\
Author: /root/g2_contributions\
Ledger rows covered: G2\
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; approved planning and accepted E3/F2/F3/G1 notes.\
Confidence overall: medium; source-backed boundaries, unexecuted design.

**Recommendation:** start with a setup/semantic report, focused correction or no-provider receipt review. Offer one optional packaged compaction contribution. Fresh scientific replication has its own protocol/resources. Ordinary contributors need neither this program's Q approvals nor its build ledger.

## Basis

Read [brief](../brief.md), [ledger/Q](../research-ledger.md), [conventions](../research/CONVENTIONS.md), [outline](../OUTLINE_TRACE.md), root/docs/crates instructions and `synthesize-spec` run mode at skills `e981ec57d487a2c41d416672483b5a4df15b1233`. Accepted inputs: [E3](14_integration-design.md), [F2](15_examples-and-maintenance.md), [F3](16_replication-and-artifacts.md), [G1](17_reader-journey.md). Source inspection checkpoint **2026-10-29T21:07:11Z**; code locators use baseline. No runtime, task/provider, issue, PR or outreach execution.

## Material findings and proposed paths

### F1 — Small public changes already have a lighter workflow
Claim: Current contribution policy distinguishes focused changes from assigned manifest work.
Status: VERIFIED\
Evidence kind: source inspection\
Evidence: [CONTRIBUTING](../../CONTRIBUTING.md), report/check/PR/manifest sections; [PR template](../../.github/pull_request_template.md), optional ticket notes; [GOVERNANCE](../../GOVERNANCE.md), authority/spec; [CODEOWNERS](../../CODEOWNERS), default owner.\
Retrieved: 2026-10-29T21:07:11Z.\
Implication for the spec: Extend existing entry points and preserve contract review.\
Outline delta: CONFIRMS OL-18; EXTENDS OL-19–OL-21.\
Limitations: Policy inspection proves no uptake, turnaround or merged contribution.

Propose three short CONTRIBUTING subsections: **Report or reproduce**, **Check a receipt**, **Try the packaged component**. Reuse issue/PR templates, adding optional receipt/procedure and allowed-data prompts to the bug form. Link G1's three proposed pages (`docs/inspect-a-run.md`, `docs/experiments.md`, `docs/evidence.md`) and existing guides when ready. No new platform or template per procedure.

Public path: select/search problem → fork/branch from designated base → reproduce/explain → focused change → applicable checks → PR with evidence → review. Small PRs require no prior issue or successful setup. Existing MIT OR Apache-2.0, no DCO/CLA, conduct/security policies apply. The maintainer reviews all paths and handles program integration/traceability. CODEOWNERS' broad “everything lands through the manifest ticket chain” comment should later be reconciled with CONTRIBUTING's explicit optional ticket workflow; outsiders should not operate internal ledgers. Consequential contract/dependency decisions still need ADRs; frozen-spec amendments use the manifest protocol. Preserve `mergePolicy: OPERATOR`/human merges. Q-3–Q-6 govern program resources/publication, not independent local contributors.

### F2 — Reports can precede successful startup or export
Claim: Reports need a precise failure and permitted evidence, not necessarily a complete bundle.
Status: UNVERIFIED\
Evidence kind: design proposal; source inspection\
Evidence: [bug form](../../.github/ISSUE_TEMPLATE/bug_report.yml), reproduction/version/environment; [lab.rs](../../crates/hh-cli/src/lab.rs), `cmd_run_export` 1047–1076/`cmd_bundle_validate` 989–1000; [security policy](../../SECURITY.md), reporting; E3 F5/F3 F2/F6 retain closure/access limits.\
Retrieved: 2026-10-29T21:07:11Z.\
Implication for the spec: Support no-run and existing-run branches; preserve native identity/losses.\
Outline delta: CONFIRMS OL-18; EXTENDS OL-07, OL-15–OL-17, OL-20.\
Limitations: Current export does not implement full study closure or guarantee reproducibility.

| Entry | Minimum useful report | Maintainer action |
|---|---|---|
| Build/install/startup fails; no run | Command, expected/actual output, exit/signal, revision/dirty state, OS/architecture/toolchain, relevant features/backend/prerequisites, sanitized stderr; doctor only if available. | Reproduce that stage; distinguish unavailable prerequisite from defect. No bundle, run ID or paid account required. |
| Wrong run/receipt result | Above plus run/attempt/configuration/input pins, relevant event/grade/procedure, expected/observed semantic fields; permitted native receipt or access/loss reason. | Verify identity/closure in scratch; route CLI/runtime/analysis/artifact defect. Terminal success and exit zero do not establish correctness. |

Source-inspected syntax, **unexecuted here**: `hh run export <run-id> --out <dir>` creates a native export; `hh bundle validate <path>` returns staged completeness. Use the actual returned bundle locator, matching CLI/kernel and scratch destination. F2 must test the printed setup/output/negative cases before a runnable label. No `hh bug-bundle`, `hh recompute` or `hh replicate` interface is claimed. R0 lifecycle drift remains until E3's regression passes.

The report is a cover note; native export remains the receipt. Inspect members/logs for credentials, signed URLs, private paths/content, model I/O, task solutions/oracles and redistribution rights. Attach only cleared data, withholding sensitive identifiers/hashes too where necessary. A minimal synthetic reproducer is welcome with its limits. Missing export, withheld inputs or failed validation remain reportable. Use the private security route for vulnerabilities, without real secrets.

F1/F3 own derivative identities, permitted mappings, transform/loss and procedure unavailability. Never redact bytes while keeping the original identity. Restricted payloads/access tokens stay outside public issues; use only an approved access route or report unavailable. Reporting does not oblige access, publication or a paid rerun.

### F3 — Verifyfixed preserves the original failure
Claim: A fix report needs comparable before/after evidence and a defect-detecting regression.
Status: UNVERIFIED\
Evidence kind: design proposal; source inspection\
Evidence: CONTRIBUTING regression/check rules; [evidence policy](../../MAINTAINING.md#evidence-classes); E3 F5 preserves old receipts/R0 controls; F2 F3/F4 separates semantics/freshness/revision.\
Retrieved: 2026-10-29T21:07:11Z.\
Implication for the spec: Treat verifyfixed as a report disposition, not a CLI or replacement receipt.\
Outline delta: EXTENDS OL-07, OL-15–OL-18, OL-21.\
Limitations: Different-input success cannot prove a fix; fixture regression cannot refresh a live claim.

Pin reported revision/input/recipe/rights; preserve original failure and copy allowed evidence to scratch. Missing prerequisites gives **unable to reproduce**, not fixed. Exercise the smallest relevant path; record semantic and infrastructure results. Add an owner regression that fails without the fix, then compare the same case on the candidate revision. Disclose changed inputs/backend and narrow the conclusion.

Append candidate/check counts, old/new identities and corrected predicate to issue/PR. Reporter verification, maintainer reproduction and outside verification are distinct; unavailable reporters cannot be assumed to agree. For R0 retain old `not_reproduced` and compare `open|finished`; cover successful/failed/open plus wrong lifecycle, tampered and missing members. A new checker report can reference unchanged old input; re-export creates a new receipt. Grade/analysis corrections follow E3/F1 supersession, never overwrite failures or add subject repetitions.

Retain native verdict/reason separately from reproduced/verifyfixed/unresolved. Expected refusal can pass a test while the operation remains refused. `live-executed` requires the ADR-0363 record produced by `scripts/live-verify.sh`; source inspection/package tests cannot earn it.

### F4 — Receipt review is the smallest outside evidence contribution
Claim: One bounded verify/recompute attempt contributes without funding fresh scientific replication.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: F3 F1–F7 owns recipient procedures/tolerances/access/independence/costs; G1 F4/F5 supplies receipt-first/failure branches; E3 F5/F6 retains closure/method/parity gates.\
Retrieved: 2026-10-29T21:07:11Z.\
Implication for the spec: Offer small review first; quote fresh replication separately.\
Outline delta: EXTENDS OL-07, OL-10–OL-11, OL-15–OL-20, OL-24.\
Limitations: No recipe/package/outsider certified; arithmetic agreement proves neither collection truth nor efficacy.

Route: select accessible card/procedure → inspect rights/pins/size → acquire tools/package → follow F3 clean-recipient recipe with verification/recomputation network disabled → retain mismatch/verdict/unavailable → submit permitted report. Acquisition never executes bundled code. Missing inputs or unimplemented recipe means unavailable, not a provider rerun prompt. Until E3/F2 supply checked recipes, advertise **not yet available**; reporting that gap is useful.

Reuse F3's proposed bounds: prebuilt-tool procedure ≤2 wall-hours, 2 cores/4 GiB RAM, 12 GiB scratch including ≤2 GiB download/8 GiB expansion; source setup separately quoted. These are unmeasured ceilings, not guarantees. $0 provider fees still incur local setup/time/storage/network costs; stop/report/requote on breach. Reuse native report and F3 fields: procedure/revision/receipt, expected/observed fields, native verdict/reason, mismatches/losses and costs/assistance. No new receipt schema.

Recommend one consenting outsider for launch under G3/Q-6 recruitment authority. Negative/blocked attempts count as participation but leave portability unmet until repair/recheck. Distinct installation/signer, relationship and assistance matter; another author-owned directory is insufficient. Unsolicited contributors may independently use published materials without Q approval.

Fresh replication is optional and separately scoped/funded: frozen question/tasks/models/oracle/environment, methods/margins/parity, caps, new attempts and comparison under F3/D2/D3. Quote setup, all provider roles/retries/regrades, instruments, storage/egress before program dispatch; no inherited pilot allowance or automatic rerun. Preserve null/negative/inconclusive results. Recompute/replay/single fresh demo/conformance do not award R2/R3 or improvement; E3's R=5 parity/method gate remains.

### F5 — The optional component needs build, seal, invoke and negative checks
Claim: Existing evict_oldest supports a bounded extension exercise with real packaging work.
Status: PARTIALLY VERIFIED\
Evidence kind: source inspection; design proposal\
Evidence: [variant manifest](../../crates/hh-compact-evict-oldest/Cargo.toml), `publish=false`; [ADR-0380](../adr/ADR-0380-l4.5-publish-set-rust-version-v0.1.0-targets-and-docsrs-readiness.md) §a/c; [package.rs](../../crates/hh-varhost/src/package.rs), `seal/load/materialize`; [package battery](../../crates/hh-varhost/tests/package_variant.rs), materialize/end-to-end/tamper/pin; [differential battery](../../crates/hh-compact-evict-oldest/tests/r2_5_differential.rs), corpus/subdomain.\
Retrieved: 2026-10-29T21:07:11Z.\
Implication for the spec: Offer one optional contribution; require observed phases beyond exit zero.\
Outline delta: CONFIRMS OL-13; CORRECTS OL-03's no-compilation promise; EXTENDS OL-17–OL-18, OL-20.\
Limitations: Unrun tests; differential empty/omission-placeholder cases are `n/a`, not scientific parity/provider-treatment proof.

Choose a conformance/diagnostic improvement to **existing evict_oldest**, not another study arm. Prerequisites: Rust 1.94/rustfmt, Cargo.lock, Git/C toolchain, compatible Unix helper/backend; provisioning may require network. No Windows promise. Variant/shared fixture runtime are not crates.io packages; use source checkout, not `cargo install`. Compile/storage/helper costs remain with no provider.

Prospective commands, names checked against source; **unexecuted here**, default debug layout:

```sh
cargo build -p hharness-helper -p hharness-compact-evict-oldest
cargo test -p hharness-compact-evict-oldest -- --nocapture
HH_HELPER_BIN="$PWD/target/debug/hh-helper" cargo test -p hharness-varhost --test package_variant -- --nocapture
cargo fmt --all --check
```

The package test copies binary/template into scratch, calls `seal_package`, writes canonical manifest, then `load_package` recomputes pins. Spawn through helper → bind `compaction_strategy` 1.0 → assess/propose/execute four 100-token items at cap 200 → check required/evict/remaining ≤200 and host-stamped external provenance → close. Binary/pin tampering must yield `PinMismatch`, never implicit reseal. Changed binaries require deliberately newly sealed packages. No standalone `hh plugin seal/load/invoke` is asserted; future recipe composes existing Rust APIs.

**Trap:** current end-to-end test returns early for missing binary/helper; tamper tests return early without binary. Require availability, actual phase evidence and not-run counts. Future required-positive checks must fail on missing prerequisites, alongside a missing-helper negative case. Invalid `HH_HELPER_BIN` alone is insufficient: [helper resolution](../../crates/hh-env/src/helper.rs), `helper_binary` 43–68, falls back to adjacent binaries/PATH. Isolate all paths or inject the existing seam. Restore default helper after tier-off checks; preserve timeouts/reruns.

Optional maintainer evidence route: `bash scripts/live-verify.sh plugin-compact-evict-oldest` ([script](../../scripts/live-verify.sh), 1464–1489) builds prerequisites/records helper-session lifecycle. It is not required for every small PR. Fixture/ABI success proves no compaction efficacy or provider-wire treatment; E3 P6/D2 own that. Defer optional work if it competes with core task/receipt readiness.

## Starter ownership and acceptance

Maintainer reviews all paths via CODEOWNERS. Prospective checks, not results; examples do not assert defects remain open.

| Starter | Files/technical owner | Review/checks |
|---|---|---|
| Fix setup/failure guidance | `docs/getting-started.md`, `docs/reference.md`, CONTRIBUTING; docs/examples | Source/links/semantics, `bash scripts/check-living-docs.sh`, changed offline recipe; quickstart checks committed candidate HEAD. |
| Reproduce CLI/R0 mismatch | `crates/hh-cli/src/lab.rs` / `crates/hh-embed/src/bundle_ops.rs`; CLI/embed | Failing case, `cargo test -p hharness-cli` or `cargo test -p hharness-embed`, format, negative controls; kernel semantics stay in owner. |
| Add corrupt/missing member regression | `crates/hh-bundle/tests/`; bundle | `cargo test -p hharness-bundle`, format; no fallback. New closure contract goes to E3/F1 ADR work. |
| Compaction boundary/prerequisite check | `crates/hh-compact-evict-oldest/src/compact.rs`, differential test, varhost package test | Above commands plus affected varhost suite; fail without fix, retain `n/a`, phases/tamper/no-helper; reuse ABI/dependencies. |
| Report receipt/recompute discrepancy | Existing issue, permitted native report/evidence card; bundle/analysis | F3 pins/tolerances/missingness/losses and outside relationship; new estimator needs method/ADR review. |

Cross-boundary/schema/tier changes add applicable workspace/drift/removability checks. Full CI dispatch, live services and external build-memory/internal ledgers are not universal public PR prerequisites. Assigned manifest work follows its contract; maintainer owns program integration.

## Answer and completion assessment

G2 settles public routes, report/verifyfixed criteria, bounded outside review, optional component and starter ownership. Missing recipes/closure/prerequisite instrumentation block corresponding runnable claims, not design completion. No efficacy, merge, release or outreach authority inferred.

## Open questions

- **E3/F2 → H1:** implement recipes and explicit package phase/prerequisite outcomes; routine engineering, no new operator question.
- **Maintainer → H1:** reconcile CODEOWNERS comment with contribution guidance, preserving internal integration/human merges.
- **Q-5/Q-6, operator → F3/G3/H2:** rights/access/outside review/publication before invitations/hosting; no extra approval for ordinary contributors.
- **Q-3/Q-4, operator → D2/E3/H2:** pins/method/parity and separate fresh-replication funding; no calls authorized here.

## Spec requirements emitted

Candidates; H1 reconciles, F1/F3 retain receipt/procedure authority.

| ID | Candidate / rationale | Finding / outline | Proposed acceptance |
|---|---|---|---|
| REQ-G-101 | Reuse public guides/forms; separate ordinary PR/internal duties. | F1; OL-18–21 | Small PR path needs no Q/ledger; human merge retained. |
| REQ-G-102 | Accept no-run reports and native evidence/access losses. | F2; OL-15–18/20 | No-binary/export-failure/restricted-input report routes work. |
| REQ-G-103 | Preserve original failure; compare candidate for verifyfixed. | F3; OL-07/15–18/21 | Regression fails without fix; old evidence/changed inputs retained. |
| REQ-G-104 | Offer bounded no-provider outside verify/recompute. | F4; OL-07/16/18–20 | Clean recipient reports phases/costs/verdict/relationship; failures retained. |
| REQ-G-105 | Separate fresh replication admission/funding. | F4; OL-10–11/15–16/20/24 | Missing parity/pins/resources prevents program dispatch; no fixture upgrade. |
| REQ-G-106 | At most one optional built/sealed/invoked component. | F5; OL-03/13/17–18/20 | Phase/tamper/no-helper evidence; early return cannot pass required case. |
| REQ-G-107 | Map starter scope to owner/regression/relevant checks. | F1–F5; OL-17–18/21 | Review without new platform; contracts/schema/dependencies routed correctly. |

## Handoff

E3/F2 own recipes/gates; F3 recipient/access semantics; G3 authorized participation/distribution; H1 requirements/guidance; H2 Q-3–Q-6. No new row/portfolio change or accepted-note conflict. CODEOWNERS wording is an existing guidance discrepancy. Only this note written; coordinator owns review/central state. Scoped checks/hash reported to coordinator; no builds/tests/runtime executions or commits.

Validation: deferrals-first empty; read-only planning validator passed, zero warnings. Scoped check passed: 23 relative links/1 anchor, five findings × eight fields, seven unique requirements, four closing sections. Added-file diff reviewed; whitespace check clean (no-index exit 1 denotes new content). Runtime/build/test commands remain unexecuted; coordinator owns final documentation checks.
