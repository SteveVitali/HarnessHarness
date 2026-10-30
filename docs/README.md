<!--
  Documentation map with edit modes (BM-DOCS-02). refresh-repo-docs reads
  the mode column: generated entries require their generator;
  frozen/historical/append-only entries are report-only during a refresh.
-->
# Documentation

Start with the guides below for the current project. The specification,
decisions, research, and build records provide the detailed contracts and
history behind it.

## Find your starting point

| Task | Document |
|---|---|
| Try the runtime | [Getting started](getting-started.md) — build, run an offline example, inspect the result, connect a local model |
| Understand the design | [Architecture](architecture.md) — kernel, Laboratory, components, and contracts |
| Find a command or integration surface | [Reference](reference.md) — CLI, configuration, APIs, and source entry points |
| Assess readiness | [Status and limitations](status.md) — available distribution, evidence, and known limits |
| Contribute | [Contributing](../CONTRIBUTING.md) — development workflow and verification |
| Work with a coding agent | [Agent instructions](../AGENTS.md) — repository rules and scoped guidance |
| Maintain or release the project | [Maintaining](../MAINTAINING.md) and [governance](../GOVERNANCE.md) |
| Read the behavioral contract | [Canonical specification](../spec/CANONICAL_SPEC.md) and its [source sections](../spec/sections/) |
| Understand a design decision | [ADR index](adr/README.md) |
| Review the evidence and adoption program | [Round-5 partition](build/planning/2026-10-30_evidence-decomposition.md), [ownership trace](build/planning/2026-10-30_evidence-coverage.md), [Ratification guide](spec-ratification-review.md), [ratified supplement](evidence-and-adoption-spec.md), [decomposition handoff](decomposition-prompt.md), [protocol/resource review](protocol-resource-review.md); [founding brief](brief.md), [research/design ledger](research-ledger.md), and [outline trace](OUTLINE_TRACE.md) |
| Trace the build | [Build records](build/README.md), [ticket manifest](tickets/00_MANIFEST.md), and [build index](build/BUILD_INDEX.md) |

Start each work session with `bash scripts/deferrals-open.sh` from the repository
root to list outstanding work. For a build ticket, then read the relevant rows,
the current state at the head of [the build ledger](build/LEDGER.md), and the
ticket contract. Follow the row-wise navigation rules in
[AGENTS.md](../AGENTS.md); the historical registers are large.

## How documentation changes

Each entry has a **mode**. Living guides can be edited for accuracy and
clarity. Generated files must be regenerated from their source. Historical
records retain the facts and language of their date; corrections are new
entries under the applicable record protocol.

| Mode | Meaning | How it changes |
|---|---|---|
| generated | Derived by a script | Regenerate; never hand-edit |
| frozen | A preserved document | Immutable; any permitted amendment follows its named protocol |
| append-only | Entries accumulate | Add entries; never remove or rewrite landed ones |
| historical | A record of what happened | Correct with a new dated entry, not an in-place edit |
| living | Current guidance | Edit in place |

## This directory

| Entry | Mode | Purpose / how to change it |
|---|---|---|
| [getting-started.md](getting-started.md) | living | The first-run tutorial and installation guide |
| [architecture.md](architecture.md) | living | An explanation of the system and its main contracts |
| [reference.md](reference.md) | living | Commands, configuration, and integration entry points |
| [status.md](status.md) | living | A current account of availability, evidence, and limitations |
| [brief.md](brief.md) | frozen | Evidence and adoption founding intent; later changes are dated addenda. Charter approved; see its dated addendum. |
| [protocol-resource-review.md](protocol-resource-review.md) | living checkpoint summary | Accepted research/design recommendations and Q-3–Q-6 review; decisions are recorded in the research ledger |
| [spec-ratification-review.md](spec-ratification-review.md) | living checkpoint summary | Concise Q-7 scope, independent review outcome and recorded ratification; exact decisions remain in the frozen research ledger |
| [evidence-and-adoption-spec.md](evidence-and-adoption-spec.md) | frozen | Q-7-ratified supplemental requirements, proposed amendments and acceptance/coverage indexes; does not replace the original canonical contract |
| [research-ledger.md](research-ledger.md) | frozen | Completed upstream work units, exact operator decisions and append-only history; frozen at Q-7 ratification |
| [OUTLINE_TRACE.md](OUTLINE_TRACE.md) | frozen | All 24 founding obligations mapped to ratified supplemental-spec requirements; downstream build records add ticket/evidence links |
| [decomposition-prompt.md](decomposition-prompt.md) | frozen | Exact extend-mode invocation, binding constraints and validation/dispatch prerequisites for the later ticket/build-artifact phase |
| [research/](research/) | living conventions + frozen accepted notes | New evidence/adoption research; [note conventions](research/CONVENTIONS.md). Distinct from the frozen root `research/` tree. Design notes use the directory below under the same conventions. |
| [README.md](README.md) | living | This navigation and edit-mode map |
| [AGENTS.md](AGENTS.md) | living | Agent instructions for editing guides and preserving historical records |
| [1_harness_engineering_frontier_report.md](1_harness_engineering_frontier_report.md) | frozen | The founding frontier survey, called “doc 1” in historical records |
| [2_Harness_Engineering_Genealogy_Anatomy_2026_Frontier_v2.md](2_Harness_Engineering_Genealogy_Anatomy_2026_Frontier_v2.md) | frozen | The genealogy and anatomy study, “doc 2”; source of the seven-plane model |
| [3_MetaHarness_Meta_Plan_and_Research_Ledger.md](3_MetaHarness_Meta_Plan_and_Research_Ledger.md) | frozen | The meta-plan that produced the specification, “doc 3” |
| [design/](design/) | frozen accepted notes | Evidence/adoption portfolio, execution and protocol design; draft notes are reviewed before acceptance under the [note conventions](research/CONVENTIONS.md). |
| [adr/](adr/) | append-only | Build decision records. `adr/README.md` is **generated** with `build-memory adr-index`; `adr/REVISIT_TRIGGERS.md` is **generated** with `scripts/gen-revisit-triggers.sh`. |
| [tickets/](tickets/) | historical | Ticket contracts and the manifest chain. `DEFERRALS.md` is **append-only**. |
| [build/](build/) | historical | Build ledger and index, per-ticket runs and PR records, gate readouts, planning, reports, backlog and coverage registers, and the dated capability account. `logs/` is gitignored; CI-boundary records are added explicitly with `git add -f`. |

The historical build tree includes `LEDGER.md`, `BUILD_INDEX.md`, `runs/`,
`pr/`, `readouts/`, `planning/`, `reports/`, `BACKLOG.md` / `BACKLOG.csv`,
`COVERAGE_MATRIX.csv`, `OPERATIONAL_READINESS.md`, `INTEGRATION_PLAN.md`, and
the `CAPSTONE_*.md` reports. Read their dates and later amendments when
interpreting a claim. [Status](status.md) connects relevant evidence to the
current user-facing account.

## Elsewhere in the repository

| Entry | Mode | Purpose / how to change it |
|---|---|---|
| [../README.md](../README.md) | living | Project introduction and executable quickstarts |
| [../CHANGELOG.md](../CHANGELOG.md) | living | Human-facing release summaries |
| [../CONTRIBUTING.md](../CONTRIBUTING.md) | living | Contribution policy and development workflow |
| [../SECURITY.md](../SECURITY.md) | living | Vulnerability-reporting policy |
| [../CODE_OF_CONDUCT.md](../CODE_OF_CONDUCT.md) | living | Community standards and reporting route |
| [../MAINTAINING.md](../MAINTAINING.md) | living | Release process, evidence classes, write integrity, and review cadence |
| [../GOVERNANCE.md](../GOVERNANCE.md) | living | Decision authority and amendment process |
| [../LICENSE-MIT](../LICENSE-MIT) · [../LICENSE-APACHE](../LICENSE-APACHE) | frozen | Dual license texts |
| [../THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) | generated | Dependency notices; regenerate with `scripts/gen-third-party-notices.sh` |
| [../AGENTS.md](../AGENTS.md) · [../crates/AGENTS.md](../crates/AGENTS.md) · [../clients/AGENTS.md](../clients/AGENTS.md) · [../foreign/AGENTS.md](../foreign/AGENTS.md) · [../CLAUDE.md](../CLAUDE.md) | living | Root and scoped agent instructions; `CLAUDE.md` is the `@AGENTS.md` bridge. Refresh with `agent-docs`, separately from human guides. |
| [../foreign/](../foreign/) | living + append-only evidence | Python and TypeScript replay candidates, their usage guides, and committed answer corpora |
| [../clients/](../clients/) | generated + living | TypeScript `hh-embed/1` client and fixtures; regenerate the generated client through the schema pipeline |
| [../spec/](../spec/) | frozen | `CANONICAL_SPEC.md`, source sections, and the readiness report; amend the contract through the manifest protocol and an ADR |
| [../research/](../research/) | frozen | The MetaHarness program ledger, research ADRs, registers, dossiers, and briefs |
| [../spikes/README.md](../spikes/README.md) | living | Index of the experimental measurement trees, which are outside the workspace |

Preserving the build record is deliberate: it allows readers to audit how
the system evolved. Update the living explanation when it becomes stale;
record historical corrections through the rules in
[GOVERNANCE.md](../GOVERNANCE.md).
