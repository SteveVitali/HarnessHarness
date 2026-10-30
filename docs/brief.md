# HarnessHarness evidence and adoption — founding brief

**Recorded:** 2026-10-29T18:01:07Z  
**Status:** Charter for an upstream research/design program; planning package awaiting operator review.  
**Inputs:** The operator's requests and approved meta-plan in the planning conversation; repository baseline `001490c14a6568972d16a663bb04c07a4255d30d`.  
**Workflow reference:** `SteveVitali/agent-skills` at `e981ec57d487a2c41d416672483b5a4df15b1233`, especially `synthesize-spec`, `decompose-spec`, `build-memory`, and `orchestrate-build`.

This brief preserves the founding intent. Record later changes as dated addenda;
the [research ledger](research-ledger.md) carries evolving work and decisions.
Start here for review, then read the ledger's work units and decision register.

## 1. Purpose and success

Make HarnessHarness understandable and useful to someone encountering it on
GitHub for the first time. Establish what it can do through inspectable
examples and reproducible evidence, and give interested users a practical way
to adopt it and contribute.

The program must answer three different questions:

1. **Can I use it?** A newcomer can perform a useful task, inspect what happened,
   and understand how to adapt the example.
2. **What does the evidence establish?** A reader can distinguish a working
   capability, an experimental result, and an unsupported hypothesis, then
   inspect or reproduce the relevant receipt.
3. **Can I extend it?** A contributor can add a bounded component, reproduce a
   reported failure, or independently replicate a study with clear acceptance
   criteria.

The immediate deliverable is a research/design plan. The downstream deliverable
is a reviewed, operator-ratified supplemental specification and a compatible
handoff for a new implementation round. No favorable experimental result is
assumed. A well-conducted null, negative, or inconclusive study can satisfy the
research deliverable; changing a product default requires separate evidence.

## 2. Authorization and present boundary

The operator approved the direction and requested: “Okay, sounds good. Let's
proceed, and when you're done the next phase, open in zed for me to review before
proceeding”. This package implements the charter/ledger phase of that direction.
The ledger is paused for that review before any research unit starts.

⚑ Preserve the goal of covering as much of the project as is useful, while
keeping the first wave achievable. Every major capability family must receive
an explicit first-wave, later, or excluded disposition with a reason.

⚑ Produce concrete, reproducible receipts and communicate their actual scope.
Do not substitute attractive examples or passing infrastructure checks for a
scientific efficacy claim.

⚑ Critically assess the alternative model's suggestions. Retain useful ideas;
do not accept uniqueness claims, effort estimates, or technical guarantees
without evidence.

⚑ Use the repository's specification, ADR, ticket, and build-memory system for
the eventual implementation, with the external agent skills as the workflow.
This upstream package does not modify the completed build's machine state,
existing gates, historical records, or the frozen canonical specification.

⚑ Stop at the review checkpoints below. The present authorization does not
approve paid experiments, public posting, a specific experimental budget, a
specification amendment, or an implementation run.

## 3. Three connected tracks

| Track | Reader outcome | Proposed proof |
|---|---|---|
| Learn and inspect | Understand a useful run and the runtime/Lab distinction | A runnable task; annotated trace; replay; bundle inspection; offline experiment tutorial |
| Measure and verify | Assess a bounded scientific or operational claim | Real task execution and independent grading; controlled comparisons; recovery/security trials; reproducible analysis |
| Extend and contribute | Make a useful first contribution | Component walkthrough; reproducible bug report; independent replication guide; small contribution contracts |

Primary audiences are hypotheses to validate: harness researchers, developers
building agents, and maintainers/contributors. The first wave should choose a
primary audience and concrete job rather than promise equal depth for everyone.

## 4. Critical synthesis of the alternative proposal

| Suggestion | Disposition and reason |
|---|---|
| Annotated anatomy of a run and a short first encounter | Retain. Connect trace events to a useful task and explain the decisions a reader can investigate. |
| Executable examples and checks that detect documentation drift | Retain. Design a small offline path and separately gated live checks; determine the maintenance cost. |
| Replay as “the same run again for free” | Reframe. Define what is replayed, what is compared, and which provider calls are avoided. Do not imply a fresh execution, identical newly written bytes, or zero total cost. |
| Portable bundles as reproducible bug reports | Retain with distinct verification, analysis recomputation, replay, and fresh rerun instructions; establish what is actually included and distributable. |
| Offline experiment before the live study | Retain as teaching and plumbing validation. A fixture cannot establish real-task efficacy. |
| Budget/permission demonstrations and approval flows | Retain as bounded operational claims with explicit attack/fault assumptions and measurable outcomes. |
| Plugin extension “without recompiling anything” | Investigate the actual extension boundary, packaging, and build steps; make no blanket promise. |
| Web screenshots and foreign-client demonstrations | Supporting material. Attach exact runnable behavior and conformance scope; do not imply complete alternate runtimes or universal interoperability. |
| An evidence tour built around live-record counts and spend | Use an evidence index, including failures and skipped sublegs. Counts, spending, and a top-level pass are not evidence of task success or utility. |
| “Nobody else ships this” and a quick implementation estimate | Reject as unestablished. Research adjacent systems and integration opportunities; estimate after checking engineering seams. |
| Live compaction comparison as a stretch goal | Keep a real controlled comparison central to the proposed first wave. Feasibility may change its size or sequencing, not silently replace it with fixtures. |
| Fleet demonstration later | Provisional deferral, with a recorded coverage disposition and conditions for reconsideration. |

These are planning judgments, not completed research findings. Units A–C will
recheck their technical premises and may revise the portfolio.

## 5. Candidate portfolio

The first-wave hypothesis is a connected journey, not six unrelated showcases.
The portfolio checkpoint selects its final scope after the audit and feasibility
work. Demonstrations and studies must carry different claims and acceptance
criteria.

| Candidate | Intended value | Evidence to design |
|---|---|---|
| Useful end-to-end task | Show what HarnessHarness does for a real user | Observable output plus an independent correctness check; identify fixture and live versions |
| Inspect, explain, replay, and share that run | Make decisions and failures understandable | Annotated events, explicit replay comparison, portable receipt, clean-store verification, limits |
| First offline Lab experiment | Teach experiment setup, comparison, and analysis | Small deterministic exercise; checked commands and outputs; explicit non-efficacy label |
| Live compaction comparison | Test whether a harness choice changes useful outcomes under resource limits | Real tasks, graders, baseline/control arms, matched accounting, repeats, uncertainty, raw observations and analysis |
| Bounded recovery and authorization trials | Test reliability and policy enforcement under specified conditions | Fault/attack matrix, expected decisions, durable state invariants, recovery outcomes, counterexamples |
| Component or replication contribution | Show that someone else can participate | Versioned interface, actual build/package steps, conformance checks, contribution and replication instructions |

The full catalogue must also consider control/planning strategies, context
builders, memory correctness and retrieval, procedure execution, rule retirement
after model changes, delegation and solo/team comparisons, topology, approvals,
extension trust, value of computation and constrained optimization, fleet and
evolution, web inspection, foreign-client conformance, and ecosystem adapters.
The capability inventory can add missing families. Each catalogue entry needs
a user question, scoped claim, evidence type, prerequisites, likely engineering,
cost drivers, and an explicit disposition. Covering a family does not require
funding a separate study for it.

## 6. Research and design workstreams

| Stream | Question | Required output |
|---|---|---|
| A — Capability and evidence audit | What is implemented, reachable, and actually evidenced? | Capability/claim/evidence/limitation matrix; newcomer workflow audit; missing links |
| B — Audiences and ecosystem | Who benefits, and how does this fit adjacent work? | Concrete user jobs; primary-source comparison; integration and collaboration opportunities |
| C — Portfolio and feasibility | Which demonstrations/studies offer the best coverage for the effort? | Prioritized catalogue; first-wave proposal; dependencies, cost ranges, risks and fallback scope |
| D — Scientific methodology | What would make each conclusion defensible? | Shared measurement rules; pilot and confirmatory protocols; operational test claims and validity limits |
| E — Execution and integration | What engineering is needed to run the protocols honestly? | Task-to-grader execution design; justified spikes; interface/contract decisions and proposed ADR scope |
| F — Receipts and maintenance | Can others inspect, recompute, replay, and rerun the evidence? | Receipt/export contract; storage and redistribution policy; example/CI design; replication procedure |
| G — Documentation and participation | How does a reader become a user or contributor? | Reader journey; example/contribution design; publication sequence and adoption measures |
| H — Synthesis readiness | Can this become implementable work without losing the intent? | Traceability, unresolved-decision closure, compatible build-artifact design and handoff |

A and B can proceed independently. C uses their findings and an initial execution
seam assessment. D–G then develop the selected portfolio together. H reconciles
those outputs before specification synthesis and independent review.

## 7. Research quality and implementation constraints

Use the [note conventions](research/CONVENTIONS.md). A claim must cite material
actually read, identify the source version and inspection time, and separate
source evidence, execution evidence, inference, and proposal. A source path is
not proof that a command was executed.

Separate infrastructure success, task success, performance improvement, and
usability/adoption. Define the task-level unit of analysis, controls, resource
accounting, failure/timeout handling, exclusions, repetitions, uncertainty and
minimum practically useful effects before confirmatory evaluation. Use pilots
for feasibility and variance estimates; preserve pilot outcomes and disclose
any protocol changes. Hash-chained provenance supports an audit trail, not the
truth of a grade or scientific conclusion.

Treat benchmark selection, independent grading, adapter parity, model/version
pinning, leakage, provider drift, task licensing and redistribution as design
questions. The existing benchmark and architecture contracts remain binding;
propose an ADR when a chosen design needs an amendment. Current ecosystem
versions do not silently replace the versions in the frozen contract.

Record failed runs, unsupported behavior, inaccessible sources, missing
prerequisites and skipped legs honestly. Design exact reproducibility only
where the mechanism supports it, and statistical replication where fresh model
or environment behavior is variable. Reuse the repository's evidence classes
and live-evidence workflow; identify any additional study metadata needed.

## 8. Checkpoints and handoff

| Checkpoint | Review package | Decision unlocked |
|---|---|---|
| Charter/ledger review — current stop | This brief, work units, dependencies, outline trace and open questions | Start the research phase |
| Portfolio/feasibility review | A–B findings, C proposal and E1 seam assessment; cost drivers and proposed spikes | Select the first wave and research/spike envelope |
| Protocol/resource review | D protocols, E feasibility, F receipt design; concrete model/task/cost and publication constraints | Accept the execution design; authorize any separately proposed pilot or paid run explicitly |
| Specification ratification | Supplemental spec, traceability, independent review dispositions and closed operator decisions | Freeze the upstream package and hand off to decomposition |
| Build readiness — later phase | New ticket/manifest/build artifacts and validation results | Start an `orchestrate-build` run |

The planned skill sequence is `synthesize-spec` plan → run → synthesize → review
→ ratify, followed by `decompose-spec` in extension mode and a separate build
readiness review. The future specification target is
`docs/evidence-and-adoption-spec.md`; it does not exist yet. Its relationship to
the frozen behavioral contract must be explicit, with amendments handled by
ADRs. Research requirements must map to the supplemental spec and then to
implementation tickets and acceptance evidence.

This package is ready for review when every founding obligation maps to a work
unit, every unit has a bounded output and dependency, operator decisions are
visible, and no research result or downstream approval has been invented.

## Addendum — charter approval and research authorization

**Recorded:** 2026-10-29T18:08:07Z. The operator approved this brief, the working ledger and
coverage trace in the planning conversation:

> Okay, I approved the charter, working ledger, and coverage trace from above, and I approve them. I agree with your effort recommendations. So let's follow your best critical judgement, and I'll leave the current main session on Extra High to be safe. Please proceed. Again open intermediate results in zed for me to review when it's time

Q-1 is resolved. Proceed through the portfolio/feasibility checkpoint and open
the intermediate review package in Zed at that point. The agreed effort policy
uses Extra High for main-session synthesis and High for bounded research
workers, with downstream effort chosen for ambiguity and the cost of errors.
This addendum preserves the original seed status above as history.

## Addendum — portfolio approval (2026-10-29T19:37:48Z)

The operator reviewed the portfolio/feasibility package in Zed and said: “looks good to me. proceed”. The [ledger](research-ledger.md) records the approved first-wave scope and bounded offline tranche. Continue dependency-ready design to the protocol/resource checkpoint. Paid execution, later task/container diagnostics, publication and implementation remain subject to the recorded later decisions.

## Addendum — protocol/resource planning approval (2026-10-29T21:53:12Z)

The operator reviewed the protocol/resource package and said: “Looks good, proceed”. The ledger records Q-3–Q-6 planning policies and the bounded independent methods follow-up. Continue through that review, synthesis reconciliation, supplemental-spec drafting and fresh independent review. Return for Q-7 ratification in Zed. Execution funding remains zero; this approval does not start implementation, task/provider runs, hosting, recruitment or publication.
