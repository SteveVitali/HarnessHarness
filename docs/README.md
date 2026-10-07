<!--
  Template: docs/README.md — the docs map with a mode column (BM-DOCS-02). Written by
  build-memory init if absent; init ADDS the build-memory rows (build/, tickets/, adr/) when
  a docs/README.md already exists. refresh-repo-docs reads the mode column: generated rows ->
  fix source/regenerate; frozen/historical/append-only -> report-only.
-->
# `docs/` — map of the documentation tree

Each entry has a **mode** that fixes how it may change. Editing a generated or historical file
by hand falsifies the record; only *living* docs are edited in place.

| Mode | Meaning | How it changes |
|---|---|---|
| generated | derived by a script | regenerate; never hand-edit |
| frozen | written once | immutable |
| append-only | rows/entries added | never removed or rewritten |
| historical | a record of what happened | corrected by a new entry, not an edit |
| living | edited in place until frozen | direct edits |

## Top-level entries

| Entry | Mode | What it is / how to change it |
|---|---|---|
| `1_harness_engineering_frontier_report.md` | frozen | "doc 1" — the frontier survey |
| `2_Harness_Engineering_Genealogy_Anatomy_2026_Frontier_v2.md` | frozen | "doc 2" — the genealogy/anatomy (the spec's seven-plane source) |
| `3_MetaHarness_Meta_Plan_and_Research_Ledger.md` | frozen | "doc 3" — the meta-plan that produced the spec |
| `adr/` | append-only | build decision records; `adr/README.md` is **generated** (`build-memory adr-index`) |
| `tickets/` | historical | the contracts (what was owed); `DEFERRALS.md` is append-only |
| `build/` | historical | the record of what happened (ledger, run ledgers, PR bodies, index, readouts) |
| `README.md` | living | this map — edit in place when the tree changes |

## Adjacent doc surfaces (repo root, not under `docs/`)

| Entry | Mode | What it is |
|---|---|---|
| `../README.md` | living | the repo's front door |
| `../AGENTS.md` | living | agent-facing docs — refreshed under `agent-docs` (DOC.2), not `refresh-repo-docs` |
| `../spec/` | living → frozen | `CANONICAL_SPEC.md` + `sections/` + `READINESS_REPORT.md`; amend via the manifest protocol + an ADR |
| `../research/` | frozen | the program ledger (`LEDGER.md`), research ADRs (`decisions/ADR-0001…`), registers, dossiers, briefs — frozen at the readiness gate |
| `../spikes/README.md` | living | the throwaway-spike index — kept in sync with the committed trees |

## Where to start
- Building a ticket? `build/LEDGER.md` → `tickets/00_MANIFEST.md` → `tickets/DEFERRALS.md`.
- Reviewing decisions? `adr/README.md`.
