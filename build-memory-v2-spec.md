# Build Memory v2 — design and implementation specification

**Document:** `~/MetaHarness/build-memory-v2-spec.md` · **Version:** 1.0.0 · **Status:** Ratified 2026-09-09
**Date:** 2026-09-09 · **Author:** Claude (Fable 5.1) for Steve Vitali
**Derived from:** `~/MetaHarness/long-horizon-memory-structures.md` (the analysis; §4 gaps G1–G18, §6 proposal) with two operator amendments: (a) build memory is committed under `docs/build/`, the gitignored `.agents/scratch/` is retired; (b) capstone-as-tickets always, whenever a build has more than one ticket.
**Requirement IDs:** `BM-<AREA>-<nn>`, append-only. Areas: `ROOT` (memory root + modes), `LAYOUT`, `LEDGER`, `TICKET`, `MANIFEST`, `GATE`, `DEFER`, `ADR`, `INDEX`, `TAIL`, `SYNTH`, `RECON`, `VALID`, `COMPAT`, `DOCS`, `MIGE` (Eleutheria), `MIGR` (Rhēma).
**Normative language:** MUST / SHOULD / MAY per RFC 2119. Prose without a keyword is rationale.

---

# Part 0 — How to use this document

This spec produces **three pull requests in three repositories**, each an `implement-spec` run against one of the ticket contracts in Part II:

| # | Ticket | Repo / worktree | Run line |
|---|---|---|---|
| 1 | `SK.1` — Build memory v2 in `agent-skills` | `~/agent-skills` | `implement-spec spec=~/MetaHarness/build-memory-v2-spec.md#SK.1 worktree=~/agent-skills base_branch=main branch_name=svitali/build-memory-v2` |
| 2 | `EL.1` — Migrate Eleutheria | `~/Eleutheria` | `implement-spec spec=~/MetaHarness/build-memory-v2-spec.md#EL.1 worktree=~/Eleutheria live_verification=false` (base = current chain tip; see EL.1 header) |
| 3 | `RH.1` — Migrate Rhēma | `~/Rhēma` | `implement-spec spec=~/MetaHarness/build-memory-v2-spec.md#RH.1 worktree=~/Rhēma live_verification=false` (base = current chain tip; see RH.1 header) |

`#SK.1` etc. name the Part II section the worker implements; the worker reads Part 0, Part I in full (the contracts), and its own ticket. Tickets 2 and 3 depend on ticket 1 being merged (or checked out) because they invoke the new `build-memory` skill. Ticket 1 is sized for one headless `implement-spec` run with compaction; if the worker reports overflow, split it at the seam named in SK.1 §"Split seam" and land two stacked PRs.

### Operator runbook (the exact sessions)

0. **Ratify.** Settle Appendix B, then change the Status line at the top of this file to `Ratified <date>`. Workers refuse to implement a draft.
1. **Session 1 — `cd ~/agent-skills` on a clean `main`.** Prompt:
   > Invoke the implement-spec skill and follow it completely. spec = `~/MetaHarness/build-memory-v2-spec.md`. Implement ticket **SK.1** (Part II) only; read Part 0 and Part I in full first — they are the contract. base_branch=main, branch_name=svitali/build-memory-v2, autonomous=true. If you overflow, stop at the split seam named in SK.1 and report; do not skip deliverables.
   Review (`review-pr` or by hand), merge to `main`. Then install the three new skills for every later session:
   `ln -s ~/agent-skills/skills/build-memory ~/agent-skills/skills/synthesize-spec ~/agent-skills/skills/reconcile-build ~/.claude/skills/`
   (the existing symlinks already cover the revised skills; they resolve into the working tree, so whatever branch is checked out in `~/agent-skills` is what later sessions run).
2. **Session 2 — `cd ~/Eleutheria` on the chain tip** (`git checkout devin/p22-2-agent-docs-refresh`, PR #67, or whatever the manifest's last row left checked out). Prompt:
   > Invoke the implement-spec skill and follow it completely. spec = `~/MetaHarness/build-memory-v2-spec.md`. Implement ticket **EL.1** (Part II) only; read Part 0 and Part I first. base_branch = current checkout (stacked PR chain), live_verification=false, autonomous=true. First action: append this ticket to `docs/tickets/00_MANIFEST.md` as row 66 and write `docs/tickets/P22.3__build-memory-v2-migration.md` from the EL.1 section, then implement it.
3. **Session 3 — `cd ~/Rhēma` on the chain tip.** Land or park P0.15i first (it has no PR yet); then check out the tip (`stevevitali/p0-15i-…` if it opened a PR, else `stevevitali/p0-15h-per-phase-review-green-exit`, PR #24). Prompt:
   > Invoke the implement-spec skill and follow it completely. spec = `~/MetaHarness/build-memory-v2-spec.md`. Implement ticket **RH.1** (Part II) only; read Part 0 and Part I first. base_branch = current checkout (stacked PR chain), live_verification=false, autonomous=true. First action: write `docs/tickets/16z_MAINT.1__build-memory-v2-migration.md` from the RH.1 section and add its chain-table row, then implement it.

Sessions 2 and 3 are independent of each other and both depend on session 1. Each needs `gh` authenticated for the repo and write permissions for the agent (bypass or accept-edits). No `orchestrate-build` is needed: these are single tickets on the manual floor.

The worker for SK.1 must treat `~/agent-skills` as a public, in-use open-source repo: existing inputs keep working, existing ledgers keep parsing, and repos that never opted into build memory see no behaviour change (Part I §9).

---

# Part I — Design

## 1. Problem, goals, non-goals

**Problem.** The multi-session build skills (`decompose-spec`, `orchestrate-build`, `implement-spec`) keep all durable state in a gitignored scratch dir, assume a finished spec arrives from nowhere, treat tickets as regenerable scaffolding, stall on human gates, judge the capstone in one context, and end at `DONE`. Two production builds (Eleutheria: 65 tickets; Rhēma: 22 ticket runs) each hand-rolled the same missing structures and both reversed the gitignore rule. The analysis document records the evidence.

**Goals.**
1. One committed, validated layout for build memory that every skill reads and writes the same way (`docs/build/`, `docs/tickets/`, `docs/adr/`).
2. The lifecycle covered end to end: brief → research → spec → decomposition → build → capstone → reconciliation → docs → next round.
3. Human gates pause the chain without blocking it; cross-ticket obligations have a committed home that every run reads first.
4. Deterministic paths and validators before prose conventions.
5. Zero disruption for current users: every existing input, ledger and layout keeps working; new behaviour is opt-in by the presence of `docs/build/README.md`.

**Non-goals.** No change to `self-review`, `review-pr`, `address-pr-comments` beyond references. No CI polling, merging, or notifications (existing deliberate omissions stand). No harness-specific directives in any SKILL.md.

## 2. Design decisions (first principles)

Each decision states the alternative considered and why it lost.

**D1 — Build memory is committed; only regenerable bulk is not.** Alternative: keep scratch gitignored and "promote" selected artifacts (the analysis document's original proposal). Rejected because promotion is a second, error-prone step (Eleutheria's post-hoc reconstruction cost a full ticket and lost fidelity), because a sibling build worktree cannot see the main worktree's scratch (the current `git-common-dir` trick makes ledger and branch disagree), and because git already is the artifact store the harness literature recommends. Carve-outs: `docs/build/logs/` is gitignored (regenerable, large: Eleutheria accumulated 4.5 MB); fixtures are size-capped by the validator; secrets never enter any file (validator greps token patterns); personal data is scanned before commit (Eleutheria P19.1 rule).

**D2 — The memory root is `docs/build/`, opted in by a marker.** Alternative: keep `.agents/scratch/` as a committed subdirectory of `docs/build/` (the operator's suggestion, literally). Adopted in substance, not in name: a committed directory of run ledgers is not "scratch", and the name would invite the same "delete freely" behaviour that lost Eleutheria's logs. The subdirectories keep their roles (`runs/`, `pr/`, `tools/`, `fixtures/`, `logs/`, `planning/`). Opt-in is the presence of `docs/build/README.md` containing the line `<!-- build-memory: v2 -->`; a repo that already has an unrelated `docs/build/` is not captured by accident. Repos without the marker run in **legacy scratch mode**, byte-for-byte the current behaviour.

**D3 — One machine-state file, holding only state.** Alternative: keep the seeded ledger's PHASE PLAN / invariants / out-of-scope / SETUP / CAPSTONE sections. Rejected: both projects duplicated these in the manifest and they drifted (Eleutheria's PHASE PLAN listed P22.x twice; Rhēma's ledger cites the manifest by section). The ledger keeps `CURRENT STATE`, `OPEN FINDINGS`, `GATE DECISIONS`, `RETURN PASS`, `PHASE LOG`; the manifest is the plan.

**D4 — The worker closes its own ticket in the ledger.** Alternative: the orchestrator advances the ledger (current design). Rejected because on the manual floor there is no orchestrator, which is exactly how Rhēma ended with a ledger that never moved. `implement-spec` detects a chain ticket (a `Sequence:` header plus `docs/build/LEDGER.md`) and, as its final step, appends the PHASE LOG entry, advances `CURRENT STATE`, writes its `BUILD_INDEX.md` row, and commits. `orchestrate-build` verifies the advance and owns only gate decisions, pauses, inserts/splits and open findings.

**D5 — Tickets are the committed contract record and carry a global sequence.** Filenames `NN[a-z]?_<ID>__<slug>.md`; lexical order is chain order; HUMAN and GATE rows are marker files in the same sequence; skeleton tickets are permitted after a gate. Legacy names (`P00.1__slug.md`) stay valid when the manifest chain table lists them: historical contracts are never renamed. Rhēma's history shows why the chain table must be authoritative and validated: five of its eight hand-inserted P0.15 sub-tickets never reached the table, and its gate marker still names the first two as the closers.

**D6 — The tail of every chain is standard tickets.** When a build has more than one ticket, `decompose-spec` appends the tail rows `CAP.1` gap analysis, `CAP.2` composed verification, `CAP.3` closure, `GATE-ACCEPT` (operator signs the accepted-deviations list), `REC.1` backlog + readiness, `REC.2` spec reconciliation, `REC.3` integration plan, `DOC.1` repo docs refresh, `DOC.2` agent docs refresh. Each is an ordinary `implement-spec` contract instantiated from a template; `orchestrate-build` has no special CAPSTONE unit any more. Alternative: keep a single-context CAPSTONE for small builds. Rejected per the operator: the context-size argument that justifies tickets applies to the capstone, and a two-ticket build's tail rows are short.

**D7 — Deferrals live beside the contracts.** `docs/tickets/DEFERRALS.md` (Rhēma's location and rules), not `docs/build/`: a deferral is owed contract work and is read with the contract; keeping the path also avoids rewriting Rhēma's history. Every `implement-spec` run reads it first and closes what it unblocks.

**D8 — Decisions are ADRs, written by the ticket that decides.** `docs/adr/ADR-NNN-<slug>.md` with Eleutheria's header and sections; `docs/adr/README.md` is generated from the files, never hand-edited (Eleutheria's hand-kept index fell out of order). A spec that carries an ADR appendix must equal the file set; the validator checks it.

**D9 — Everything derived is regenerated and checked, never maintained.** ADR index, BUILD_INDEX completeness, REQ→ticket coverage, ticket sequence, DEFERRALS ids, layout — all checked by `check-build-memory.sh`, exit-code gated, runnable in CI.

**D10 — The upstream half is a ledger-driven skill of the same shape as the build.** `synthesize-spec` writes `docs/research-ledger.md` (rows, owners, status, evidence, operator-decisions register) and executes rows in fresh contexts, like `orchestrate-build` executes tickets; `drive-build.sh` can drive it with `--skill synthesize-spec`. Alternative: a single-context "write me a spec" skill. Rejected: both projects' upstream halves were multi-day, multi-context efforts with operator decision rounds.

**D11 — Closeout procedures are a skill invoked by tail tickets.** `reconcile-build` holds the procedures and templates for backlog, ticket-vs-spec, spec reconciliation, integration plan and readiness; the `REC.*` tickets say "invoke `reconcile-build mode=…`", the way P22.x said "invoke `refresh-repo-docs`". Alternative: fold the procedures into `orchestrate-build`. Rejected: it would push that SKILL.md past the 500-line ceiling and mix sequencing with content.

**D12 — A `build-memory` skill owns the contract, the templates, the validator and migration.** Precedent: `agent-docs` owns the shared Doc Authoring Guidelines that other skills cite. Modes: `init`, `check`, `migrate`, `adr-index`. All other skills cite `skills/build-memory/layout.md` rather than restating it.

**D13 — Naming.** New skills follow the repo's verb-object kebab pattern: `synthesize-spec` pairs with `decompose-spec` and `implement-spec`; `reconcile-build` pairs with `orchestrate-build`; `build-memory` follows `agent-docs` (a noun-named owner of a shared layer). Fixed-name memory files are upper-case (`LEDGER.md`, `BUILD_INDEX.md`, `BACKLOG.csv`, `00_MANIFEST.md`), matching the existing manifest and Eleutheria; subdirectories are lower-case.

**D14 — Version and compatibility.** Plugin version `0.1.0` → `0.2.0`; a new `CHANGELOG.md`; a README "Upgrading" section. Legacy ledgers (with PHASE PLAN and `nextTicket: CAPSTONE`) remain driveable: a legacy `CAPSTONE` unit instructs the fresh context to run `decompose-spec mode=extend` to append the tail rows and continue.

## 3. The layout contract (`skills/build-memory/layout.md`)

```
<repo>/
├── AGENTS.md                         # + "Build memory" section (BM-DOCS-01)
├── docs/
│   ├── README.md                     # docs map with a mode column (BM-DOCS-02)
│   ├── brief.md                      # S0 (optional; frozen)
│   ├── research-ledger.md            # S1–S3 work list + Q register (living until ratified, then frozen)
│   ├── research/  design/            # numbered notes; research/CONVENTIONS.md
│   ├── <spec>.md  [spec_src/ + BUILD.sh]   # canonical spec (generated if spec_src exists)
│   ├── decomposition-prompt.md       # the committed hand-off (frozen)
│   ├── adr/                          # README.md (generated) · _TEMPLATE.md · ADR-NNN-<slug>.md
│   ├── tickets/                      # contracts: the record of what was owed
│   │   ├── 00_MANIFEST.md · _TEMPLATE.md · DEFERRALS.md
│   │   ├── NN[a-z]?_<ID>__<slug>.md              # implement-spec contracts
│   │   └── NN[a-z]_HUMAN-H<k>__<slug>.md · NN[a-z]_GATE-G<k>__<slug>.md   # marker docs
│   └── build/                        # the memory root: the record of what happened
│       ├── README.md                 # contains `<!-- build-memory: v2 -->`
│       ├── LEDGER.md                 # the machine-state file (§4)
│       ├── BUILD_INDEX.md            # one row per landed ticket (§6.3)
│       ├── runs/<ID>.md              # implement-spec run ledgers
│       ├── pr/<ID>.md                # PR bodies as submitted
│       ├── readouts/GATE-G<k>.md     # gate readouts (append-only)
│       ├── planning/                 # re-planning rounds: <date>_planning-ledger.md, <date>_decision-memo.md
│       ├── reports/                  # project-specific reports a ticket produces
│       ├── tools/                    # validators + one-off generators (committed)
│       ├── fixtures/                 # small scratch fixtures (size-capped)
│       ├── logs/                     # gitignored: `*` + `!.gitignore`; drive-build/ under it
│       ├── COVERAGE_MATRIX.csv · CAPSTONE_GAP_ANALYSIS.md · COMPOSED_E2E_REPORT.md · CAPSTONE_CLOSURE.md
│       └── BACKLOG.csv · BACKLOG.md · TICKET_VS_SPEC.md · SPEC_RECONCILIATION_PLAN.md · INTEGRATION_PLAN.md · OPERATIONAL_READINESS.md
```

- **BM-LAYOUT-01 (MUST).** `layout.md` states the tree above, the role of each entry, its mode (generated / frozen / append-only / historical / living), and who writes it (§8). Every other skill cites this file and does not restate the tree.
- **BM-LAYOUT-02 (MUST).** Ticket ids match `^(HUMAN-H|GATE-G)?[A-Z]+[0-9]*(\.[0-9]+[a-z]?)?$` (examples: `T1`, `T12a`, `P0.15b`, `PRB.04`, `CAP.1`, `REC.2`, `DOC.1`, `MAINT.1`, `HUMAN-H0`, `GATE-G1`). Filenames match `^[0-9]{2,3}[a-z]?_<ID>__[a-z0-9-]+\.md$`. A legacy filename is accepted when the manifest chain table lists it.
- **BM-LAYOUT-03 (MUST).** `docs/build/logs/` is gitignored by a checked-in `docs/build/logs/.gitignore` (`*` and `!.gitignore`). Nothing else under `docs/build/` is ignored.
- **BM-LAYOUT-04 (SHOULD).** Fixtures over 1 MB and any single file over 5 MB under `docs/build/` are flagged by the validator; logs are never committed.
- **BM-LAYOUT-05 (MUST).** No file under `docs/build/` or `docs/tickets/` contains a secret; the validator greps for common token shapes (`AKIA[0-9A-Z]{16}`, `sk-[A-Za-z0-9]{20,}`, `ghp_[A-Za-z0-9]{36}`, `-----BEGIN [A-Z ]*PRIVATE KEY-----`, `xox[baprs]-`) and fails on a hit. Ledgers record `provided: yes/no` for credentials, never values.
- **BM-ROOT-01 (MUST).** `skills/build-memory/scripts/memory-root.sh` prints two lines, `mode=<committed|scratch>` and `root=<absolute path>`, resolved as: (1) `$BUILD_MEMORY_ROOT` if set; (2) `<worktree toplevel>/docs/build` if its `README.md` contains `<!-- build-memory: v2 -->`; (3) otherwise `mode=scratch` with `root=${AGENT_SCRATCH_DIR:-<git-common-dir parent>/.agents/scratch}` (today's rule, unchanged). Every build skill obtains the root from this script.
- **BM-ROOT-02 (MUST).** In `committed` mode the root is resolved from the *current worktree*, so a sibling build worktree reads the ledger on its own chain tip, never the main worktree's copy.

## 4. `LEDGER.md` — the machine-state contract

- **BM-LEDGER-01 (MUST).** Sections, in order: an `OPERATING MODE` blockquote (the verbatim resume prompt for a fresh session), `## CURRENT STATE`, `## OPEN FINDINGS`, `## GATE DECISIONS`, `## RETURN PASS`, `## PHASE LOG`. No plan sections: the manifest is the plan (`manifest:` key).
- **BM-LEDGER-02 (MUST).** `CURRENT STATE` is a fenced `key: value` block with exactly these keys, in this order: `projectStatus` (NOT_STARTED | IN_PROGRESS | BLOCKED | PAUSED | DONE), `nextTicket`, `lastCompleted`, `blockedOn`, `pauseRequested`, `returnPass`, `manifest`, `canonicalSpec`, `memoryRoot`, `dispatchTarget`, `buildWorktree`, `buildBranchBase`, `pinnedBaseSha`, `chainTip`, `benchmarkSet`, `autonomy`, `mergePolicy` (NONE | OPERATOR | AUTO-BOTTOM-UP), `round` (integer, starts 1), `updatedAt`. Trailing `# comments` are allowed (the reader strips them).
- **BM-LEDGER-03 (MUST).** `blockedOn` is reserved for real blocks (red verification, missing dependency, missing infrastructure the operator refused). A pending human gate is never a block; it is a `RETURN PASS` row.
- **BM-LEDGER-04 (MUST).** `GATE DECISIONS` is an append-only table `| date | ticket | gate | item | answer | consequence |`. Secrets never; `provided: yes/no` only.
- **BM-LEDGER-05 (MUST).** `RETURN PASS` is a table `| ticket | gates | what the operator must do | re-run line |`; `returnPass:` lists the same ticket ids.
- **BM-LEDGER-06 (MUST).** `PHASE LOG` entries are append-only, newest last, one per event, of the fixed shape `- <date> — <TICKET> <done|blocked|inserted|split|gate|pause|round> — branch · PR · base · one-line summary · **Verify:** … · **Deferrals:** opened/closed ids · **Deviations:** … · chainTip → … · next → …`.
- **BM-LEDGER-07 (MUST).** `drive-build.sh` parses `projectStatus`, `nextTicket`, `pauseRequested`, `blockedOn`, `buildWorktree`, `returnPass`, `manifest` with the existing never-fail `status_val` reader; unknown keys are ignored; a legacy ledger without the new keys still drives.

## 5. Manifest, ticket, markers, skeletons, tail

**Manifest (`docs/tickets/00_MANIFEST.md`)** — **BM-MANIFEST-01 (MUST)** sections in order: title + three banners (committed contract record; cite-don't-copy with the spec amendment protocol; deferrals companion); `## How to build` (the stacked-chain recipe, the four rules: table order, stay on the previous branch, stop at GATE rows, never let a code ticket block on a HUMAN row; the run-line pattern; how `orchestrate-build` drives it); `## Human prerequisites` (H-rows); `## The chain` (table `| # | file | phase | kind | scope | gate |`, `kind` ∈ ticket | human | gate | skeleton | capstone | reconcile | docs, marker rows interleaved); `## Milestone gates` (thresholds quoted verbatim); `## Phase gates & ownership notes`; `## Cross-cutting invariants`; `## Out of scope`; `## Requirement-ID → ticket index`; `## Spec amendments applied` (append-only: date, section, before/after or pointer, approver, ADR); `## Decomposition decisions` (incl. the Phase-4 adversarial review record); `## Plan extensions` (append-only: inserts, splits, rounds).
**BM-MANIFEST-02 (MUST).** The manifest is complete on its own: a human with a terminal can drive the chain from it without the ledger.
**BM-MANIFEST-03 (MUST).** Inserting a ticket at run time uses filename suffix letters (`16a_…`, `16b_…`) and a `## Plan extensions` line; splitting a ticket produces `<ID>a`, `<ID>b` files and marks the original `superseded-by-split` in the chain table (the original file is kept). Every inserted file is a chain-table row; a file in `docs/tickets/` that is neither a chain row nor a listed companion is a validator error.
**BM-MANIFEST-04 (MUST).** A `companions:` line under the banners lists non-chain files kept in `docs/tickets/` (default `DEFERRALS.md`, `_TEMPLATE.md`; a project may add audit or readout companions such as Rhēma's `16_P0.15g__preflight-audit.md`). The validator accepts exactly those.

**Ticket (`docs/tickets/_TEMPLATE.md`)** — **BM-TICKET-01 (MUST)** header bullets: `Sequence: <n> of <N>` (an inserted ticket uses its filename prefix, e.g. `16e of 50`) · `Phase` · `Kind` · `Tag` (optional, e.g. beta / post-beta) · `base_branch: current checkout` · `Depends on` · `Run:` (the exact `implement-spec` line incl. per-ticket flags) · `Gate status:` (block of operator ticks, or `none`) · `Live stage:` (none | offline-only | operator-gated: <budget>). Body sections: `## Goal` · `## Load (read these — do not re-read others)` · `## In scope — deliverables` (numbered; each names the requirement ids it satisfies) · `## Out of scope` (names the owning ticket) · `## Acceptance criteria` (each tagged *(deterministic)* or *(agentic)*; the last is the universal phase-gate AC) · `## Requirement IDs to satisfy and stamp in the PR` · `## Cross-cutting invariants` (cited from the manifest) · `## Notes` (decisions this ticket owns; anchors to re-confirm).
**BM-TICKET-02 (MUST).** The universal phase-gate AC reads: "verification green; every new behaviour has a test that fails if it is removed; requirement ids stamped in the PR; anything not automatically verifiable is a `DEFERRALS.md` row with its compensating control; ADRs written for every deviation and owned decision; `BUILD_INDEX.md` row and `LEDGER.md` advanced."
**BM-TICKET-03 (MUST).** A skeleton ticket carries `Kind: skeleton`, a `> Skeleton only` banner naming the gate after which the body is written, and only the header, `## Scope (one line)`, `## Spec §§` and `## REQ coverage`. The validator refuses a run line for a skeleton (`implement-spec` MUST stop if asked to run one).
**BM-TICKET-04 (MUST).** Executed contracts are never rewritten; a later amendment appends a dated note (`> Amended <date>: …`) that points at the manifest amendment line.

**Markers** — **BM-TICKET-05 (MUST).** `HUMAN-H<k>` marker: the operator work as a checklist, exit criterion, which tickets it blocks, and the DEFERRALS rule for tickets that run before it completes; its checkboxes are living state, ticked (with a date) by the operator or by `orchestrate-build` when the ledger records the item done — a marker whose items are demonstrably done but unticked is a validator warning. `GATE-G<k>` marker: the criterion verbatim from the spec, the readout path `docs/build/readouts/GATE-G<k>.md`, the rule "never guessed past", the pre-registered thresholds, and the DEFERRALS rule (no OPEN row scoped to the phase).

**Tail rows** — **BM-TAIL-01 (MUST).** When the chain has more than one implement-spec ticket, `decompose-spec` appends, from `skills/build-memory/templates/tail/`: `CAP.1__capstone-gap-analysis` (independent fresh context; verdicts MET / MET-DIFFERENTLY / PARTIAL / MISSING / AT-RISK-INTEGRATION before reading any run ledger; `COVERAGE_MATRIX.csv` with columns `id, level, spec_section, class, verdict, evidence, owning_tickets, tests, adrs, routing, note`; seam hunt; `CAPSTONE_GAP_ANALYSIS.md`), `CAP.2__capstone-composed-verification` (the whole build as one unit; unwired seams are `xfail`/skips whose reason starts with a DEFERRALS id; `COMPOSED_E2E_REPORT.md`), `CAP.3__capstone-closure` (close routed gaps on `<user>/<build>-capstone`; `CAPSTONE_CLOSURE.md` with the ACCEPTED-deviations list proposed for signature), `GATE-ACCEPT` marker (operator signs), `REC.1__backlog-and-readiness`, `REC.2__spec-reconciliation`, `REC.3__integration-plan`, `DOC.1__repo-docs-refresh` (invokes `refresh-repo-docs`), `DOC.2__agent-docs-refresh` (invokes `agent-docs`). `tail=full` (default) | `minimal` (`CAP.1`, `CAP.3`, `DOC.1`, `DOC.2`); `tail=none` is refused when N > 1.
**BM-TAIL-02 (MUST).** Tail templates carry placeholders (`{{build_name}}`, `{{spec_path}}`, `{{req_id_pattern}}`, `{{ticket_count}}`, `{{last_ticket}}`) that `decompose-spec` fills; the instantiated files are ordinary tickets in the chain table with `kind` capstone | reconcile | docs.
**BM-TAIL-03 (MUST).** `projectStatus: DONE` requires: every chain row landed or consciously skipped (recorded), `BUILD_INDEX.md` complete, no `OPEN` deferral without a `landing`, and the `GATE-ACCEPT` readout signed.

## 6. Deferrals, ADRs, build index, readouts

**BM-DEFER-01 (MUST).** `docs/tickets/DEFERRALS.md` header states the four rules verbatim: read first every run; never delete a row; a deferral not in the file did not happen; gates refuse to pass with an `OPEN` row scoped to the phase. Row schema: `| id | item | why deferred | unblocked by | how to verify | proxy now | status |`; `id` = `D-<TICKET>-<n>`; `status` ∈ OPEN | PARTIAL | DONE | WONTFIX | ACCEPTED-SKELETON; an optional `kind` column ∈ V (verification) | F (functionality) | D (deviation) | H (handoff seam) | P (human prerequisite) | X (other).
**BM-DEFER-02 (MUST).** `implement-spec` Phase 0.3 reads the file; closing any row this ticket or its landed prerequisites unblock is in scope; Phase 5.3 live verification that cannot run because a `Live stage` is operator-gated or infrastructure is absent records an OPEN row (proxy, unblocked-by, how-to-verify) and reports "gate pending", never a failure and never a fabricated pass.

**BM-ADR-01 (MUST).** `docs/adr/ADR-NNN-<slug>.md`: H1 `# ADR-NNN: <title>`; header bullets `Status` (Proposed | Accepted | Superseded by ADR-MMM), `Date`, `Ticket`, `Requirement ids`, `Spec` (sections); sections `## Context`, `## Decision`, `## Consequences`, `## Alternatives considered`, `## Revisit trigger`. Decisions are immutable; a change is a new ADR; a retro-fitted record says so in its title.
**BM-ADR-02 (MUST).** `docs/adr/README.md` is generated by `scripts/adr-index.sh` from the files (numeric order; columns ADR · title · ticket · status) and carries `<!-- generated by build-memory adr-index; do not edit -->`. The validator regenerates to a temp file and diffs.
**BM-ADR-03 (MUST).** `implement-spec` writes an ADR for: every MET-DIFFERENTLY verdict in its gap table, every SHOULD-level deviation, and every decision its ticket's `## Notes` says it owns; the ADR lands in the same PR.
**BM-ADR-04 (SHOULD).** When the spec carries an ADR appendix or index, the validator checks the two sets are equal (Eleutheria SIG-ENG-039).

**BM-INDEX-01 (MUST).** `docs/build/BUILD_INDEX.md` has one row per landed chain row: `| seq | ticket | kind | branch | PR | base | landed | ADRs | deferrals opened → closed | live verification (run / fixture-only / n-a / gate-pending) | evidence |`, where `evidence` points at `runs/<ID>.md#evidence` or `pr/<ID>.md`. Written by the worker at close (D4), never reconstructed later.
**BM-INDEX-02 (MUST).** `docs/build/runs/<ID>.md` is the implement-spec run ledger: sections `Spec / Base / Branch / Config`, `## Deferrals read`, `## Requirements`, `## Acceptance criteria`, `## Plan`, `## Test matrix`, `## Progress`, `## Gap table`, `## Evidence log`, `## Evidence report` (the text submitted as the PR body). `docs/build/pr/<ID>.md` is the PR body as submitted (`gh pr create --body-file`).
**BM-INDEX-03 (MUST).** Gate readouts `docs/build/readouts/GATE-G<k>.md` are append-only: criterion, per-item evidence, verdict (PASSED | NOT PASSABLE | SKIPPED-BY-OPERATOR), date, and the operator's disposition line.

## 7. Gate protocol

- **BM-GATE-01 (MUST).** Before dispatching a ticket whose `Gate status` block has unticked items, `orchestrate-build` pauses (in `checkpoint` and `manual` autonomy; in `auto` it treats all items as "skip"), presents the block, records each answer in `GATE DECISIONS`, commits `LEDGER.md` on the chain tip, and dispatches with the prompt line "Gate answers are in `docs/build/LEDGER.md` GATE DECISIONS — copy them into the ticket's Gate status block in your first commit and act on them."
- **BM-GATE-02 (MUST).** An answer of "skip" runs the ticket ungated: it does everything up to the gate, records the gated remainder as `DEFERRALS.md` rows, opens its PR, and is listed in `RETURN PASS` with its re-run line. Re-running the same ticket file after ticking is idempotent.
- **BM-GATE-03 (MUST).** A `GATE-G<k>` marker row is executed by `orchestrate-build` as: read the readout (or produce it from the named evidence sources when the marker says the orchestrator authors it), present it, record the operator's disposition (PASSED / SKIPPED-BY-OPERATOR / NOT PASSABLE + what would pass it), commit, continue or stop. Never guessed past.
- **BM-GATE-04 (MUST).** `drive-build.sh` exits 0 with "gate pending — answer in LEDGER.md GATE DECISIONS and re-run" when the only obstacle is a gate under `checkpoint`/`manual`; exit 2 remains for real blocks.

## 8. Who writes what

| Artifact | Writer | When |
|---|---|---|
| `docs/build/README.md`, `LEDGER.md` (seed), `BUILD_INDEX.md` (header), `logs/.gitignore`, `docs/README.md` rows, `docs/adr/{README,_TEMPLATE}.md`, `docs/tickets/{00_MANIFEST,_TEMPLATE,DEFERRALS}.md`, tickets, markers, tail rows, `docs/decomposition-prompt.md` | `decompose-spec` via `build-memory init` | seed / extend |
| `runs/<ID>.md`, `pr/<ID>.md`, ADRs, `DEFERRALS.md` rows, `BUILD_INDEX.md` row, `LEDGER.md` close (CURRENT STATE + PHASE LOG), `reports/*`, project registers the spec mandates | `implement-spec` | per ticket, two commits: code+docs, then `docs(build): close <ID>` |
| `LEDGER.md` GATE DECISIONS / RETURN PASS / OPEN FINDINGS / pause / insert / split; `readouts/GATE-G<k>.md` | `orchestrate-build` | at boundaries, committed on the chain tip |
| `COVERAGE_MATRIX.csv`, `CAPSTONE_*.md`, `COMPOSED_E2E_REPORT.md` | `CAP.*` tickets (workers) | tail |
| `BACKLOG.*`, `TICKET_VS_SPEC.md`, `SPEC_RECONCILIATION_PLAN.md`, `INTEGRATION_PLAN.md`, `OPERATIONAL_READINESS.md` | `REC.*` tickets via `reconcile-build` | tail |
| `docs/research-ledger.md`, `research/*`, `design/*`, the spec, `decomposition-prompt.md` | `synthesize-spec` | upstream |
| `planning/<date>_*.md` | `reconcile-build` (seed) and `decompose-spec mode=extend` | next round |

## 9. Compatibility and migration policy

- **BM-COMPAT-01 (MUST).** A repo without the `docs/build/README.md` marker behaves exactly as with 0.1.0: scratch mode, gitignored ledgers, `tickets_dir` default in scratch, `.agents/` excluded from staging.
- **BM-COMPAT-02 (MUST).** All existing inputs (`spec`, `build_name`, `dispatch_target`, `granularity`, `ledger`, `tickets_dir`, `build_worktree`, `pinned_base_sha`, `dispatch`, `agent_cmd`, `autonomy`, `parallel`, and every `implement-spec` toggle) keep their names and defaults, except that `tickets_dir` defaults to `docs/tickets` **in committed mode only**.
- **BM-COMPAT-03 (MUST).** A legacy ledger (PHASE PLAN present, `manifest:` absent) is driven from its own PHASE PLAN; on `nextTicket: CAPSTONE` the fresh context runs `decompose-spec mode=extend tail=full` (which converts the ledger to v2, writes the manifest if missing, appends tail rows) and continues. The one-context capstone procedure is retained verbatim in `skills/orchestrate-build/modes/legacy-capstone.md` for operators who set `legacy_capstone=true`.
- **BM-COMPAT-04 (MUST).** `build-memory migrate` moves a legacy `.agents/scratch/` (or a custom `$AGENT_SCRATCH_DIR`) into `docs/build/`: ledgers → `runs/<ID>.md` (ticket id from the file's own header or the manifest; unresolvable names keep their basename), PR drafts → `pr/`, tools → `tools/`, fixtures → `fixtures/`, logs dropped, the machine ledger → `LEDGER.md` (legacy sections retained, `manifest:`/`memoryRoot:`/`round:` added), and writes a rename mapping into `docs/build/README.md`. It prints a dry-run plan by default and mutates only with `apply=true`; move/rename only, contents never edited.
- **BM-COMPAT-05 (SHOULD).** Historical ticket filenames are not renamed; the manifest chain table carries the sequence.

## 10. Validator (`skills/build-memory/scripts/check-build-memory.sh`)

- **BM-VALID-01 (MUST).** Bash 3.2, read-only, exit 0 clean / 1 violations / 2 not a build-memory repo; human summary to stdout; JSON to `/tmp/build-memory-check.json` (same shape convention as the two freshness detectors). Checks: layout (only the named entries at the root of `docs/build/`; `logs/.gitignore` present); ticket filename grammar and unique, monotone sequence (letters allowed); every manifest chain row has a file and every ticket file has a chain row; `Depends on` only points backward; skeletons have no run line; every marker referenced; `DEFERRALS.md` ids unique with valid statuses; no OPEN row scoped to a gate whose readout says PASSED; ADR files ↔ generated index; every ADR has `## Revisit trigger`; spec ADR appendix equals the file set when present; `LEDGER.md` keys present and in order, `nextTicket` names a chain row or DONE; every `PHASE LOG` "done" ticket has a `BUILD_INDEX.md` row and a `runs/<ID>.md`; REQ→ticket index: every id cited by a ticket exists in the spec (pattern from the manifest's `req_id_pattern:` line) and every in-scope id has exactly one owner; size and secret checks (§3).
- **BM-VALID-02 (MUST).** `decompose-spec` runs it after seeding, `implement-spec` runs it before the close commit, `orchestrate-build` runs it at every boundary; a failure is a real block.

## 11. The new skills

### 11.1 `build-memory`
Inputs: `mode` (init | check | migrate | adr-index; default check), `memory_root` (default resolved), `from` (legacy scratch path for migrate), `apply` (migrate: default false = dry-run). Files: `SKILL.md` (hub, ≤200 lines), `layout.md` (the contract, §3–§8 of this spec in reference form), `README.md` (rationale: the two builds, ADR-058, the sibling-worktree defect, why committed), `templates/` (`docs-README.md`, `build-README.md`, `LEDGER.md`, `BUILD_INDEX.md`, `DEFERRALS.md`, `adr-TEMPLATE.md`, `ticket.md`, `ticket-skeleton.md`, `HUMAN.md`, `GATE.md`, `MANIFEST.md`, `AGENTS-build-memory.md`, `research-ledger.md`, `research-CONVENTIONS.md`, `spec-front-matter.md`, `COVERAGE_MATRIX.csv`, `BACKLOG.csv`, `tail/CAP.1…DOC.2.md`), `scripts/` (`memory-root.sh`, `check-build-memory.sh`, `adr-index.sh`, `migrate-legacy-scratch.sh` — the last mutating, stated in its header).

### 11.2 `synthesize-spec`
Inputs: `brief` (file, required), `spec_out` (default `docs/<build_name>-spec.md`), `build_name`, `rounds` (default 1), `adversarial_review` (default true), `outline_trace` (default true when a brief exists), `operator_questions` (default true), `mode` (plan | run | synthesize | review | ratify; default auto from the ledger). Hub `SKILL.md` + `modes/plan.md`, `modes/run.md`, `modes/synthesize.md`, `modes/review.md`, `modes/ratify.md`, `README.md`.
- **BM-SYNTH-01 (MUST).** `plan` writes `docs/research-ledger.md` from the template: header (status vocabulary `open → in-progress → done | blocked-on-operator | dropped(reason)`; owner vocabulary R research / D design / A architecture / P product / S synthesis; ⚑ marks operator-fixed constraints; "nothing cited that was not read"), `## 0. Working theses`, lettered streams with `| id | item | owner | status | evidence |` tables, `## O. Spec synthesis & review process`, `## Q. Operator decisions register`, `## Change log`, plus a `CURRENT STATE` block (`nextUnit`, `projectStatus`, …) so `drive-build.sh --skill synthesize-spec` can drive it.
- **BM-SYNTH-02 (MUST).** `run` executes one open row per fresh context (read-only research/design fan-out is allowed inside a row); each note is written under `docs/research/` or `docs/design/` as `NN_<slug>.md` per `research/CONVENTIONS.md` (finding format: claim, status VERIFIED | PARTIALLY VERIFIED | UNVERIFIED | CONTRADICTED | INACCESSIBLE, evidence URL fetched, retrieved date, implication, outline delta; `## Open questions`; `## Spec requirements emitted` with `REQ-<STREAM>-<n>` ids).
- **BM-SYNTH-03 (MUST).** `synthesize` writes the spec with the front-matter block (Status/version, delta paragraph, synthesized-from, supersedes), `§0 How to read` (contracts adopted by reference), `§0.2 Requirement-ID convention` (`REQ-<FAMILY>-<n>`, append-only, reserved ids, `req_id_pattern`), a decision register, and appendices A (requirement index with AC hooks), B (decision register: Q-* and rulings), C (research index), D (glossary + reserved strings), E (review dispositions). With a brief: an `OUTLINE_TRACE.md` of `OL-*` obligations and an Appendix proving superset coverage.
- **BM-SYNTH-04 (MUST).** `review` runs a fresh-context adversarial pass (consistency, coverage of every ledger row, MVP realism, contradictions of the spec's own claims) and, when the spec has a prior version, a delta-only review; findings are dispositioned in Appendix E (accept / adapt / rebut / defer), the same vocabulary as `respond-to-review`.
- **BM-SYNTH-05 (MUST).** `ratify` presents the open Q rows, records answers in §Q, flips the spec Status to "canonical", freezes the ledger (change-log line), and writes `docs/decomposition-prompt.md` with the exact `decompose-spec` invocation and project binding constraints; it then hands off with the run line.

### 11.3 `reconcile-build`
Inputs: `mode` (backlog | spec | integration; required), `ledger`, `manifest`, `apply_amendments` (spec mode; default false = proposals only). Hub `SKILL.md` + `modes/backlog.md`, `modes/spec.md`, `modes/integration.md`, `README.md`.
- **BM-RECON-01 (MUST).** `backlog`: one `BACKLOG.csv` (`bl_id, title, type, sources, req_ids, package, blocks, landing, gate, size, status`) where every OPEN/PARTIAL deferral, every ADR revisit trigger, every OPEN FINDINGS entry, every spec-mandated register's deferred row, and every non-MET matrix row appears in exactly one `sources` cell; `BACKLOG.md` grouped by landing; `OPERATIONAL_READINESS.md` (capability × {code, infra, human gate, owner}; critical path with `ticket:` and `proof:` on every step; no TBD); `docs/build/tools/check_backlog.py`-equivalent as a bash script in the skill (`check-backlog.sh`).
- **BM-RECON-02 (MUST).** `spec`: `TICKET_VS_SPEC.md` (each landed ticket's deliverables tagged in-spec / spec-implied / ticket-added with dispositions), `SPEC_RECONCILIATION_PLAN.md` (amendments with target file, anchor, before/after, tick state; fold-back ids appended to their families; ADR set vs spec appendix), applied only where ticked, always through `spec_src` when it exists, with a manifest amendment line and an ADR.
- **BM-RECON-03 (MUST).** `integration`: `INTEGRATION_PLAN.md` (PR graph, read-only merge dry-run via `merge-dryrun.sh` — never merges — strategies, copy-pasteable operator procedure, rollback, post-merge verification), release-notes draft from `BUILD_INDEX.md`, and a `planning/<date>_decision-memo.md` skeleton seeding the next `decompose-spec mode=extend`.

## 12. Repo conventions the SK.1 worker must respect

Frontmatter with `name`, `license: MIT`, `description` (what + when), `inputs` (name/required/description); harness-neutral prose; concrete-enough-to-verify instructions; SKILL.md ≤ ~500 lines, hub + `modes/` beyond that (precedent `agent-docs`); `README.md` per skill = design rationale with a "Honest limitations" section and references; scripts in `scripts/`, bash 3.2, header comment with Usage / Checks / Output / Exit codes / "Compatible with bash 3.2+", read-only unless the header says otherwise; `checklists/` and templates as progressive-disclosure material referenced from the hub; `// turbo` before read-only bash blocks is optional; no `.claude/` or harness-specific directives in skill files; commit messages: imperative summary ≤ 72 chars, body explains why; `.claude-plugin/plugin.json` and `marketplace.json` descriptions list the skills; README "Repo layout" and "The skills around it" tables enumerate every skill.

---

# Part II — Tickets

## SK.1 — Build memory v2 in `~/agent-skills`

- **Sequence:** 1 of 3 · **Phase:** skills · **Kind:** ticket
- **base_branch:** `main` · **branch_name:** `svitali/build-memory-v2` · **worktree:** `~/agent-skills`
- **Depends on:** nothing
- **Run:** `implement-spec spec=~/MetaHarness/build-memory-v2-spec.md#SK.1 worktree=~/agent-skills base_branch=main branch_name=svitali/build-memory-v2 live_verification=false`
- **Gate status:** none · **Live stage:** offline-only

### Goal
Encode the Part I contracts in `~/agent-skills`: a new `build-memory` skill that owns the layout, templates, validator and migration; new `synthesize-spec` and `reconcile-build` skills; the three build skills and the two docs skills revised to read and write the committed memory root; version 0.2.0 with full backward compatibility for repos that have not opted in.

### Load (read these — do not re-read others)
- This document: Part 0, Part I in full.
- `~/MetaHarness/long-horizon-memory-structures.md` §4 (gaps), §6.4 (templates) — for the rationale text of READMEs only.
- `~/agent-skills/README.md` (Authoring a new skill; Repo layout; Design principles), `.claude-plugin/*.json`, `skills/agent-docs/SKILL.md` + `modes/*.md` + `guidelines.md` (the hub/modes precedent), `skills/agent-docs/scripts/check-agent-docs-freshness.sh` (script header and JSON-report conventions), `skills/self-review/scripts/verify.sh` (exit-code conventions), `skills/decompose-spec/SKILL.md`, `skills/orchestrate-build/SKILL.md` + `scripts/drive-build.sh`, `skills/implement-spec/SKILL.md`, `skills/refresh-repo-docs/SKILL.md`, the three build-skill READMEs.
- Reference instances (read, never copy verbatim): `~/Rhēma/docs/tickets/00_MANIFEST.md`, `~/Rhēma/docs/tickets/DEFERRALS.md` (header rules), `~/Rhēma/docs/2_research-and-design-ledger.md` (§0, §O, §Q), `~/Rhēma/docs/rhema-design-spec.md` lines 1–80 (front matter), `~/Eleutheria/docs/tickets/_TEMPLATE.md`, `~/Eleutheria/docs/adr/ADR-058-*.md`, `~/Eleutheria/.agents/scratch/planning/sig-postbuild-build-ledger.md` (GATE PROTOCOL / GATE DECISIONS / RETURN PASS), `~/Eleutheria/docs/tickets/P19.2__capstone-gap-analysis.md`, `P20.1__backlog-and-operational-readiness.md`, `P20.2__spec-reconciliation.md`, `~/Eleutheria/docs/research/_meta/CONVENTIONS.md`, `~/Eleutheria/docs/README.md`.

### In scope — deliverables
1. **`skills/build-memory/`** (BM-LAYOUT-01..05, BM-ROOT-01..02, BM-LEDGER-01..07, BM-TICKET-01..05, BM-MANIFEST-01..04, BM-TAIL-01..03, BM-DEFER-01, BM-ADR-01..02, BM-INDEX-01..03, BM-VALID-01, BM-COMPAT-04): `SKILL.md` (modes init / check / migrate / adr-index), `layout.md`, `README.md`, `templates/*` as listed in §11.1 (every template self-describing with a header comment naming who fills it and when), `scripts/memory-root.sh`, `scripts/check-build-memory.sh`, `scripts/adr-index.sh`, `scripts/migrate-legacy-scratch.sh`. `init` writes the layout into a repo (idempotent; never overwrites an existing file; adds `docs/README.md` rows if the file exists, else creates it from the template). Tail templates `tail/CAP.1__capstone-gap-analysis.md`, `CAP.2__capstone-composed-verification.md`, `CAP.3__capstone-closure.md`, `GATE-ACCEPT.md`, `REC.1__backlog-and-readiness.md`, `REC.2__spec-reconciliation.md`, `REC.3__integration-plan.md`, `DOC.1__repo-docs-refresh.md`, `DOC.2__agent-docs-refresh.md`, each a complete ticket per BM-TICKET-01 with placeholders per BM-TAIL-02 and procedures derived from `orchestrate-build` §3 (today) and the Eleutheria P19.2/P19.3/P19.5/P20.x contracts.
2. **`skills/decompose-spec/`** revised (BM-COMPAT-02, BM-TAIL-01, BM-MANIFEST-01, BM-TICKET-03/05): new inputs `mode` (seed | extend), `tail` (full | minimal), `sequence_prefix` (default true); Phase 0 also reads `docs/research-ledger.md` §Q, `DEFERRALS.md`, `BACKLOG.csv` in extend mode; Phase 2 emits HUMAN/GATE marker files and skeletons; Phase 3 uses the ticket template; Phase 5 calls `build-memory init`, writes the manifest v2 and the state-only ledger, appends tail rows, persists `docs/decomposition-prompt.md` when invoked from one, runs the validator, prints the manual floor; the "must be gitignored" clauses removed; the spec amendment protocol subsection added ("amend the source; bump version/delta; ids append-only; manifest amendment line with before/after and approver; ADR when a design decision changes; then update affected tickets' Load/AC lines"); `README.md` §7 rewritten to the committed rationale (ADR-058's argument; the sibling-worktree defect) and §8 limitations updated.
3. **`skills/orchestrate-build/`** revised (BM-LEDGER-07, BM-GATE-01..04, BM-COMPAT-03, BM-VALID-02): §0 reads `manifest:`/`memoryRoot:`/`returnPass:` and routes legacy ledgers; §1 SETUP resolves the root via `memory-root.sh` and commits the seeded state; §2.1 gate protocol; §2.3 becomes "confirm the worker closed the ticket (ledger advanced, index row, run ledger present, validator green); reconcile if not"; §2.4 insert/split per BM-MANIFEST-03; §3 replaced by "the standard tail" (tail rows are ordinary units; `DONE` per BM-TAIL-03) with the old text moved verbatim to `modes/legacy-capstone.md`; guardrails add "a gate is a pause, not a block" and "secrets never enter any ledger"; `scripts/drive-build.sh` gains `--skill <name>` (default orchestrate-build; the prompt names that skill), parses `returnPass`/`manifest`, exits 0 on gate-pending, defaults `--log-dir` to `<memoryRoot>/logs/drive-build/` in committed mode, keeps the lock and no-progress guard; `README.md` §2 and §6 updated (the ledger is committed and travels with the chain tip; the capstone is tickets).
4. **`skills/implement-spec/`** revised (BM-DEFER-02, BM-ADR-03, BM-INDEX-01..02, BM-TICKET-02/03/04, BM-COMPAT-01): 0.1 resolves the root via `memory-root.sh`; 0.3 reads the AGENTS.md build-memory section, `DEFERRALS.md`, the ticket header (`Gate status`, `Live stage`; refuses skeletons); 0.4 ledger path `runs/<ID>.md` in committed mode (branch/date name in scratch mode, unchanged); 4.2/6.1a decision record (ADRs); 5.3 gate-pending rule; 6.1 staging includes `docs/build/**`, `docs/adr/**`, `docs/tickets/DEFERRALS.md` this run wrote and still excludes legacy `.agents/`; 6.3 writes `pr/<ID>.md` and uses `--body-file`; 6.4 evidence report is also the last section of the run ledger; new 6.5 "Close the ticket" (BUILD_INDEX row, LEDGER CURRENT STATE + PHASE LOG, validator, commit `docs(build): close <ID>`, push) when the spec is a chain ticket; `README.md` §6 portability bullet updated.
5. **`skills/synthesize-spec/`** new (BM-SYNTH-01..05): hub + five modes + README (rationale: both projects' upstream halves; the research ledger shape; the review-round structure; self-preference bias references already in the repo).
6. **`skills/reconcile-build/`** new (BM-RECON-01..03): hub + three modes + README + `scripts/check-backlog.sh` + `scripts/merge-dryrun.sh` (read-only; derived from Eleutheria's `docs/build/tools/merge_dryrun.sh` behaviour, rewritten in the repo's script style).
7. **`skills/agent-docs/`** (BM-DOCS-01): `guidelines.md` gains an optional "Build memory" section (where tickets/manifest/deferrals/build index/ADRs live; "read `DEFERRALS.md` first every run" as a Critical Gotcha; append-only and stacked-PR rules), generated by bootstrap/refresh when `docs/build/README.md` carries the marker; the freshness detector reports its absence as a coverage gap in such repos.
8. **`skills/refresh-repo-docs/`** (BM-DOCS-02): Phase 0 reads a `docs/README.md` mode table when present; `generated` → fix source/regenerate; `frozen` / `historical` / `append-only` → report-only; `docs/tickets/` and `docs/build/` default to historical.
9. **Repo docs and manifest**: `README.md` (lifecycle table S0–S9 with owning skills; layout diagram; "Upgrading from 0.1" section stating the opt-in rule and the migrate mode; every table lists the three new skills; the "Repo layout" block gains `templates/` and `tests/` entries; design principle "commit by audit value, ignore only regenerable bulk"); `CHANGELOG.md` (new; 0.2.0 entry); `.claude-plugin/plugin.json` and `marketplace.json` version `0.2.0` and descriptions listing all eleven skills.
10. **Self-test fixtures**: `skills/build-memory/tests/` with three tiny fixture repos (`v2-clean`, `v2-violations`, `legacy-scratch`) and `run-tests.sh` that inits git in a temp copy, runs `memory-root.sh`, `check-build-memory.sh`, `adr-index.sh`, `migrate-legacy-scratch.sh --apply` on `legacy-scratch`, and asserts exit codes and expected files; documented in `build-memory/README.md` and runnable by `self-review`'s verify step.

### Out of scope
- Any change to `self-review`, `review-pr`, `address-pr-comments` beyond link text. Any harness-specific config. Migrating Eleutheria or Rhēma (EL.1, RH.1). Implementing CI polling, merging, notifications.

### Split seam (only if the worker reports overflow)
SK.1a = deliverables 1–4, 9, 10 (the memory contract and the three build skills); SK.1b = deliverables 5–8 (the two new lifecycle skills and the docs-skill edits), stacked on SK.1a.

### Acceptance criteria
- [ ] `bash skills/build-memory/tests/run-tests.sh` exits 0 and prints one PASS line per fixture; `check-build-memory.sh` exits 2 on `legacy-scratch`, 1 on `v2-violations` (naming every seeded violation: bad filename, duplicate sequence, forward dependency, skeleton with run line, orphan deferral status, ADR without revisit trigger, secret pattern), 0 on `v2-clean`. *(deterministic)*
- [ ] `memory-root.sh` prints `mode=scratch` in a temp repo with no marker and `mode=committed root=<path>/docs/build` after `build-memory init`; with `BUILD_MEMORY_ROOT` set it prints that path. *(deterministic)*
- [ ] `migrate-legacy-scratch.sh` on the `legacy-scratch` fixture (which contains ledgers in the three Eleutheria naming styles, a `pr/` dir, a `tools/` dir, `logs/*.log`, and a legacy build ledger) produces `runs/<ID>.md` for each resolvable ledger, keeps unresolvable basenames, drops logs, writes `LEDGER.md` with the added keys, writes the mapping into `docs/build/README.md`, and leaves file contents byte-identical (`cmp` per moved file). *(deterministic)*
- [ ] `adr-index.sh` regenerates the fixture's `docs/adr/README.md` byte-identically; editing an ADR title changes exactly that row. *(deterministic)*
- [ ] `drive-build.sh --help` documents `--skill`; `bash -n` passes on every script; every script header states usage, checks, exit codes, and bash 3.2 compatibility; scripts run under `bash --posix`-free bash 3.2 syntax (no associative arrays, no `mapfile`). *(deterministic)*
- [ ] Every SKILL.md has the frontmatter fields the README prescribes; `wc -l` ≤ 500 for every SKILL.md; every mode/template file is referenced from its hub; no file contains a harness-specific directive (`grep -rniE 'claude code|cursor|windsurf' skills/*/SKILL.md` matches only the existing dispatch table in orchestrate-build and the CLAUDE.md bridge in agent-docs). *(deterministic)*
- [ ] `README.md` tables enumerate exactly the eleven skills present under `skills/`; `plugin.json` and `marketplace.json` say `0.2.0`; `CHANGELOG.md` exists. *(deterministic)*
- [ ] A dry run of `decompose-spec` on a small sample spec placed in a temp repo (the worker writes one in the fixture dir: three requirements, two tickets) yields `docs/tickets/{00_MANIFEST,_TEMPLATE,DEFERRALS}.md`, two ticket files with `NN_` prefixes, nine tail rows in the chain table, a state-only `docs/build/LEDGER.md`, and a green validator. *(agentic — the worker performs the run and records the tree listing in the evidence report)*
- [ ] Legacy behaviour preserved: in a temp repo without the marker, `implement-spec` Phase 0 text and `decompose-spec` Phase 5 text resolve to the gitignored scratch dir with today's file names (evidence: the worker quotes the resolution steps and runs `memory-root.sh`). *(deterministic)*
- [ ] Fresh-context review (implement-spec Phase 3 Pass 2) confirms each Part I MUST is implemented or explicitly deferred in the evidence report; the gap table maps every BM-* id except `BM-MIGE-*` and `BM-MIGR-*`. *(agentic)*
- [ ] **Phase gate:** `self-review` green; `bash skills/self-review/scripts/verify.sh` exits 2 (nothing inferable) or 0; no red. *(deterministic)*

### Requirement IDs to satisfy and stamp in the PR
All `BM-ROOT`, `BM-LAYOUT`, `BM-LEDGER`, `BM-TICKET`, `BM-MANIFEST`, `BM-GATE`, `BM-DEFER`, `BM-ADR`, `BM-INDEX`, `BM-TAIL`, `BM-SYNTH`, `BM-RECON`, `BM-VALID`, `BM-COMPAT-01..05`, `BM-DOCS-01..02`.

### Cross-cutting invariants
- **Zero disruption:** no existing input renamed; no default changed outside committed mode; legacy ledgers drive.
- **Harness-neutral prose; deterministic before LLM; progressive disclosure; proportional rigor** (README design principles).
- **One source of truth:** the layout is stated once in `layout.md`; skills cite it.

### Notes
- This ticket owns: the `BM-*` vocabulary in skill text; the ticket-id grammar; the `LEDGER.md` key set; the tail row ids `CAP.*`, `REC.*`, `DOC.*`, `GATE-ACCEPT`.
- Keep `decompose-spec`'s existing Phase 4 adversarial review untouched; the manifest's "Decomposition decisions" section records it.
- Re-confirm at build time: line numbers in `orchestrate-build/SKILL.md` §2.3/§3 and `implement-spec/SKILL.md` 0.4/6.1/6.3 (anchors drift).

---

## EL.1 — Migrate Eleutheria to build memory v2

- **Sequence:** 2 of 3 · **Phase:** migration · **Kind:** ticket
- **base_branch:** current checkout — the Eleutheria chain tip at run time (`devin/p22-2-agent-docs-refresh`, PR #67, as of 2026-09-09; if P22.2 has landed a later tip, use that) · **worktree:** `~/Eleutheria`
- **Depends on:** SK.1 merged into `~/agent-skills` `main` (or checked out) and installed (`~/.claude/skills/build-memory` symlink present); P22.2 (AGENTS.md refresh) landed so this ticket edits the refreshed docs.
- **Run:** `implement-spec spec=~/MetaHarness/build-memory-v2-spec.md#EL.1 worktree=~/Eleutheria live_verification=false`
- **Gate status:** none · **Live stage:** offline-only
- **Chain bookkeeping:** this ticket is appended to `docs/tickets/00_MANIFEST.md` as row 66, file `docs/tickets/P22.3__build-memory-v2-migration.md` (legacy naming kept for this repo), phase 22; the ticket file is a copy of this section with the Load list resolved to repo paths.

### Goal
Retire `.agents/scratch/`, commit the build's memory under `docs/build/` in the v2 layout, convert the machine ledger, complete `BUILD_INDEX.md` for rows 47–65, seed `docs/tickets/DEFERRALS.md` with the currently owed gate-pending work, record the decision as ADR-073, and leave the repo green under `check-build-memory.sh` — without renaming any historical ticket or rewriting any historical content.

### Load (read these — do not re-read others; SIG-ENG-001)
- This document Part I §3–§10; `~/agent-skills/skills/build-memory/layout.md` and `SKILL.md` (migrate mode).
- `docs/adr/ADR-058-*.md` (the decision being superseded in part; its revisit trigger), `docs/adr/README.md`, `docs/research/_meta/spec_src/99a_appF_adr.md`, `docs/build/tools/check_spec_src.py` (Appendix F ↔ ADR set), `AGENTS.md` (post-P22.2), `docs/README.md` (mode table), `docs/build/README.md`, `.agents/scratch/README.md` (the 44-row rename mapping — carry it forward), `.agents/scratch/planning/sig-postbuild-build-ledger.md`, `docs/tickets/00_MANIFEST.md` (banners, rows 47–65, rules 1–4), `docs/build/BUILD_INDEX.md` (rows 1–46; §A.4 column notes), `docs/build/BACKLOG.csv` (`landing`, `gate` columns), `.gitignore`.
- Ground truth for the index rows 47–65: the ledger's PHASE LOG entries and `gh pr view 47..67 --json body,baseRefName,headRefName,state`.

### In scope — deliverables
1. **Migrate scratch** (BM-MIGE-01): run `build-memory migrate from=.agents/scratch apply=true`. Expected mapping: `ledgers/implement-spec_<PXX.Y>.md` → `docs/build/runs/<PXX.Y>.md` (52 files); the five skill-named ledgers → their ticket ids (`P19.2`, `P21.2`, `P21.3`, `P21.5`, `P21.9`); the hybrids `implement-spec_P21.4_20260909.md` → `P21.4.md`, `implement-spec_p20-3_20260909.md` → `P20.3.md` and the pre-existing `implement-spec_P20.3.md` → `P20.3-dup1.md` (older by mtime keeps the plain name; both kept), `implement-spec_p22-1-repo-docs-refresh.md` → `P22.1.md`; the root ledger `implement-spec_devin-p19-3-…_20260115.md` → `runs/P19.3.md`; `pr/*` → `docs/build/pr/` normalized to `<ID>.md` where the id is derivable from the filename (`pr_N.md` captures keep their names under `pr/captures/`); root PR bodies (`p19-3-pr-body.md`, `p216_pr_body.md`, `pr_p22_2_body.md`, `ledgers/p20-3_pr_body.md`) → `pr/<ID>.md`; `tools/*` → `docs/build/tools/generators/` (the validators already under `docs/build/tools/` stay); `fixtures/*` → `docs/build/fixtures/`; every `*.log` (root and `logs/`) deleted; `planning/sig-postbuild-build-ledger.md` → `docs/build/LEDGER.md` with `manifest: docs/tickets/00_MANIFEST.md`, `memoryRoot: docs/build`, `round: 2` added to CURRENT STATE and the legacy PHASE PLAN / invariants / SETUP / CAPSTONE sections retained under a `## LEGACY PLAN (superseded by the manifest; kept for provenance)` heading. The rename mapping (44 historical rows carried from the scratch README + the new rows) lands in `docs/build/README.md`. `.agents/` removed; `.gitignore` gains `docs/build/logs/` (the `scratch/` line stays, harmless); `.devinignore` (untracked) left alone. Contents byte-identical (`cmp`); nothing edited.
2. **Ledger state** (BM-MIGE-02): `LEDGER.md` CURRENT STATE reflects reality at run time (P22.1 and P22.2 done → PHASE LOG entries for both from their PR bodies; `nextTicket` = this ticket then `CAPSTONE`-legacy → replace with the tail rows: see 5); `returnPass` unchanged; `updatedAt` bumped.
3. **`BUILD_INDEX.md` rows 47–66** (BM-MIGE-03): appended in the existing column set plus `deferrals` and `live verification` columns per BM-INDEX-01, one row per PHASE LOG "done" entry, evidence pointing at `runs/<ID>.md` or `pr/<ID>.md`; the inserted P20.4 and the P22.0 plan-extension commit recorded as rows.
4. **`docs/tickets/DEFERRALS.md`** (BM-MIGE-04): created from the template with the four rules; seeded with one OPEN row per RETURN PASS item (P21.1 HG-03/04; P21.3 HG-03/09; P21.4 HG-01/11/Go-public; P21.5 HG-07; P21.7 HG-08/10; P21.8 and P21.9 HG-03/04 per source) using the ledger's RETURN PASS text for "what unblocks it" and "how to verify", ids `D-P21.n-1…`, a `kind` column, and a header line pointing at `docs/build/BACKLOG.csv` for pre-existing normalized debt (no duplication: each DEFERRALS row cites its `BL-` id where one exists).
5. **Manifest and tail** (BM-MIGE-05): banners updated (the ledger is committed at `docs/build/LEDGER.md`; run ledgers at `docs/build/runs/`; DEFERRALS companion); rule 4 text updated (GATE DECISIONS now committed); row 66 = this ticket; a `## Plan extensions` section; the legacy CAPSTONE checklist replaced by tail rows `CAP.1`, `CAP.2`, `CAP.3`, `GATE-ACCEPT`, `REC.1`, `REC.2`, `REC.3` instantiated from the v2 templates as `docs/tickets/P23.1__capstone-gap-analysis.md` … `P23.7__integration-plan.md` (legacy naming; `DOC.*` omitted because P22.x already ran) with Load lines pointing at the existing P19.x/P20.x artifacts so the second capstone is a *delta* over the first (the templates' placeholders filled; a `> Round 2` banner explains the relationship to P19–P20). `CURRENT STATE.nextTicket` → `P23.1` after this ticket.
6. **ADR-073** (BM-MIGE-06): "Build memory is committed under docs/build; agent scratch is retired" — Status Accepted, Ticket P22.3, Requirement ids SIG-ENG-001/003/031, supersedes ADR-058 §3 (ADR-058 marked `Superseded in part by ADR-073` in its Status line is *not* done — ADRs are immutable; instead ADR-073 states what it supersedes and the generated index shows both); Appendix F row added in `spec_src/99a_appF_adr.md`, `BUILD.sh` run, `check_spec_src.py` green (ADR set == Appendix F; id count unchanged); `docs/adr/README.md` regenerated by `build-memory adr-index` (numeric order; the trailing prose paragraph preserved below the table under `## Notes`).
7. **Docs** (BM-MIGE-07): `AGENTS.md` "Where things live" rewritten to the v2 paths plus a "Deferred obligations — read first" gotcha (mirroring Rhēma's), within the 250-line budget; `docs/README.md` rows for `build/` (historical, now including run ledgers), `tickets/DEFERRALS.md` (append-only), and `adr/README.md` (generated); `docs/build/README.md` rewritten from the template with the layout, the rename mapping, and the list of legacy artifacts.
8. **Registers** (project rule): `docs/risk_register.md` section `## Phase 22 — Build memory v2 (P22.3)` with `RISK-P22-05` (committed run ledgers may carry personal data → scan performed, none found / redacted list) and `RISK-P22-06` (generated ADR index drifts from files → validator in `make docs-check`); `docs/traceability.md` section for the stamped ids; `make docs-check` gains `check-build-memory.sh .` (vendored to `scripts/docs/` like the two detectors, MIT header).

### Out of scope
- Renaming any `docs/tickets/P*.md`. Editing any historical ledger, PR body, ADR body, or register row. Running the tail tickets. Merging anything (operator). Changing code.

### Acceptance criteria
- [ ] `bash scripts/docs/check-build-memory.sh .` exits 0; `test ! -d .agents`; `git ls-files docs/build/runs | wc -l` ≥ 62; `find docs/build -name '*.log' | wc -l` = 0; `docs/build/logs/.gitignore` exists. *(deterministic)*
- [ ] For every moved file, `cmp` against its pre-move content (the worker records the pre-move `sha256sum` list in `docs/build/README.md` and verifies it) — 0 differences. *(deterministic)*
- [ ] `grep -c '^| ' docs/build/BUILD_INDEX.md` grew by exactly the number of PHASE LOG "done" entries for rows 47–66; every new row's evidence path exists. *(deterministic)*
- [ ] `docs/tickets/DEFERRALS.md` has one OPEN row per RETURN PASS ticket/gate pair (13 rows: P21.1×2, P21.3×2, P21.4×3, P21.5×1, P21.7×2, P21.8×1, P21.9×1 — recount at run time) and each cites a `BL-` id where one exists. *(deterministic)*
- [ ] `python docs/build/tools/check_spec_src.py` exits 0 with 71 ADRs; `docs/adr/README.md` carries the generated marker and lists ADR-001…073 (minus 064) in numeric order. *(deterministic)*
- [ ] `docs/build/LEDGER.md` CURRENT STATE has the BM-LEDGER-02 key set in order; `nextTicket: P23.1`; `manifest:` and `memoryRoot:` set; PHASE LOG has entries for P22.1, P22.2, P22.3. *(deterministic)*
- [ ] `make check` result identical to P22.2's (no code change); `make docs-check` green incl. the new validator. *(deterministic)*
- [ ] A reader can find any RETURN PASS item's owed work in one hop from `DEFERRALS.md` (reviewer check on 3 rows). *(agentic)*
- [ ] **Phase gate (§51.3):** CI green; ADR-073 indexed; risk register + traceability appended; `BUILD_INDEX.md` row for P22.3 and `LEDGER.md` closed by the worker. *(deterministic)*

### Requirement IDs to satisfy and stamp in the PR
`BM-MIGE-01..07`, `BM-COMPAT-04..05`; project ids SIG-ENG-001, SIG-ENG-003, SIG-ENG-031, SIG-ENG-039.

### Cross-cutting invariants
- **Append-only & provenance (P1–P3):** move/rename only; mapping recorded; nothing historical edited.
- **Part VIII is binding:** scan every newly committed file for private individuals' personal data before staging; record the scan in the PR body.
- **No code change; `make check` identical.**

### Notes
- This ticket owns the `docs/build/` v2 layout in this repo and the `D-P21.n-*` deferral ids. P23.x consume `DEFERRALS.md` and `LEDGER.md`.
- Re-confirm at build time: the chain tip, PR numbers 66–67, whether P22.2 changed `AGENTS.md` line budgets.

---

## RH.1 — Migrate Rhēma to build memory v2

- **Sequence:** 3 of 3 · **Phase:** migration · **Kind:** ticket
- **base_branch:** current checkout — the Rhēma chain tip at run time (`stevevitali/p0-15i-agent-web-research-sandbox` if P0.15i has a PR, else `stevevitali/p0-15h-per-phase-review-green-exit`, PR #24, as of 2026-09-09) · **worktree:** `~/Rhēma`
- **Depends on:** SK.1 merged and installed.
- **Run:** `implement-spec spec=~/MetaHarness/build-memory-v2-spec.md#RH.1 worktree=~/Rhēma live_verification=false`
- **Gate status:** none · **Live stage:** offline-only
- **Chain bookkeeping:** appended to `docs/tickets/00_MANIFEST.md` as an inserted row after the last P0.15 row, file `docs/tickets/16z_MAINT.1__build-memory-v2-migration.md` (suffix insert convention), kind ticket, before `16a_GATE-G1`.

### Goal
Bring Rhēma onto the v2 layout: create the committed memory root, move the 22 run ledgers, reconstruct the machine ledger and `BUILD_INDEX.md` from git and the 24 PRs so the build finally has a completion record, seed `docs/adr/` with retro-fitted records of the three build-time decisions that today live only in manifest paragraphs, add the docs map and the AGENTS.md build-memory section, and leave the validator green — without renaming the numbered docs the frozen spec cites.

### Load (read these — do not re-read others)
- This document Part I §3–§10; `~/agent-skills/skills/build-memory/layout.md`, `SKILL.md` (migrate, init, adr-index).
- `docs/tickets/00_MANIFEST.md` (banners, chain table, spec amendments applied — the two paragraphs that become ADRs), `docs/tickets/DEFERRALS.md` (header, status vocabulary in use incl. `PARTIAL`), `AGENTS.md`, `.agents/scratch/rhema-build-ledger.md`, the 22 `.agents/scratch/implement-spec_*.md` headers (ticket id is in each H1), `docs/2_research-and-design-ledger.md` § Change log (last two entries), `docs/phase0-exit-readout.md` (head), `.gitignore`, `git log --format='%h %ad %s' --date=short` and `gh pr list --state all --json number,headRefName,baseRefName,state,title`.

### In scope — deliverables
1. **Init + migrate** (BM-MIGR-01): `build-memory init` (creates `docs/build/README.md` with the marker, `logs/.gitignore`, `docs/adr/{README,_TEMPLATE}.md`, `docs/README.md`); `build-memory migrate from=.agents/scratch apply=true`: `implement-spec_<…>.md` → `docs/build/runs/<ID>.md` where `<ID>` is parsed from each file's H1 (`P0.01` … `P0.15g`; both naming styles resolve); `rhema-build-ledger.md` → `docs/build/LEDGER.md` (see 2). `.agents/` removed; `.gitignore`: drop `.agents/` and `agent-scratch/`? — **no**: keep both lines (harmless, and the research ledger's Q-13 history references them), add `docs/build/logs/`.
2. **Ledger reconstruction** (BM-MIGR-02): `LEDGER.md` rewritten to the v2 state-only shape: `projectStatus: IN_PROGRESS`, `lastCompleted` = the last landed P0.15 sub-ticket (P0.15h per PR #24; P0.15i if landed), `nextTicket` = the next chain row after this ticket (`16a_GATE-G1` or `16_P0.15i`), `chainTip` = the current branch, `pinnedBaseSha` = the merge-base of `stevevitali/p0-01-monorepo-and-infra` with `main` at P0.01's fork (`git merge-base`), `buildWorktree: ~/Rhēma`, `dispatchTarget: manual`, `autonomy: checkpoint`, `mergePolicy: OPERATOR`, `round: 1`, `manifest`, `memoryRoot`, `returnPass: (none)`; `GATE DECISIONS` empty; `OPEN FINDINGS` seeded with the G1 blocker from `phase0-exit-readout.md` (cited-not-fetched); `PHASE LOG` reconstructed: the three seed entries retained, then one "done" entry per PR #1–#24 in chain order (date = PR merge/creation date, branch, PR, base, one-line title, `Deferrals:` = the `D-<ID>-*` ids that PR's DEFERRALS section opened), plus an "inserted" entry for PR #7 (cloud switch) and one per P0.15b–h "synthesized from previous blockers" insert. Legacy sections dropped (they cite the manifest by section already).
3. **`BUILD_INDEX.md`** (BM-MIGR-03): rows for PR #1–#24 (+ #7 as an insert row) with `live verification` = `gate-pending` for Stage-B-deferred items, `run` for P0.15c/e/g/h Stage B, `n-a` for pure-library tickets; evidence → `runs/<ID>.md` where a ledger exists (22) else the PR body (the worker saves `gh pr view N --json body` to `docs/build/pr/<ID>.md` for all 24 so every row has a committed evidence file).
4. **ADRs** (BM-MIGR-04): retro-fitted records (title suffix "— retro-fitted record"), Status Accepted, from the manifest's "Spec amendments applied" and the research ledger's change log: `ADR-001` cloud switch AWS → GCP and inference via the direct Anthropic API (2026-09-02; PR #7; spec §12–13 amendment); `ADR-002` `@rhema/protocols` as a committed workspace member with export-time stub swap (2026-09-01; REQ-ARCH-37); `ADR-003` SST → Pulumi IaC rewrite (2026-09-03; D-P0.01-7; PR #18); `ADR-004` `docs/` committed and build memory versioned in git (2026-09-01 revision to Q-13, extended by this ticket to `docs/build/`). Each with Context / Decision / Consequences / Alternatives / Revisit trigger, `Ticket:` naming the originating PR, and `docs/adr/README.md` generated.
5. **Manifest and markers** (BM-MIGR-05): the "machine ledger stays gitignored" banner sentence replaced with the committed path; a sentence on `docs/build/runs/`; a `companions:` line (`DEFERRALS.md`, `16_P0.15g__preflight-audit.md`); chain-table rows added for the hand-inserted `16_P0.15e`, `16_P0.15f`, `16_P0.15g`, `16_P0.15h`, `16_P0.15i` (today only b/c/d are listed) and for this ticket; a `## Plan extensions` section listing every P0.15b–i insert with its date and the commit that added it, plus this ticket; the `Spec amendments applied` entries gain `(ADR-001)` / `(ADR-002)` / `(ADR-003)` pointers (append-only: parenthetical at line end); `16a_GATE-G1__phase0-exit.md` gains an appended `> Updated <date>` block pointing its disposition line at `docs/build/LEDGER.md` GATE DECISIONS and `docs/build/readouts/GATE-G1.md`, and naming P0.15d–i as the closers (append, never rewrite the original text); `00a_HUMAN-H0` items that the run ledgers prove done (GCP project `rhema-507501`, the Anthropic key) are ticked with a date and the evidence pointer.
6. **Deferrals** (BM-MIGR-06): header status vocabulary extended to include `PARTIAL` (already in use — rows are not rewritten); the preamble's "live AWS deploy" corrected to GCP in the living header only; a `kind` column is **not** back-filled (append-only); a one-line pointer to `docs/build/LEDGER.md` added under the rules; the P0.15g/P0.15h rows that were physically inserted inside the P0.15f table are left where they are (the validator keys on ids, not sections).
7. **Docs** (BM-MIGR-07): `docs/README.md` mode map covering `1_founding-prompt.md` (frozen), `2_research-and-design-ledger.md` (frozen), `4_…critique.md` (frozen), `5_round2-ledger.md` (frozen), `6_ticket-decomposition-prompt.md` (frozen), `rhema-design-spec.md` (frozen-with-amendments: amendment protocol = manifest line + ADR), `phase0-exit-readout.md` (append-only), `research/`, `design/` (frozen), `adr/` (append-only; README generated), `tickets/` (historical; DEFERRALS append-only), `build/` (historical record); numbered filenames **not** renamed (the spec cites them). `AGENTS.md` gains the "Build memory" section from the template (kept ≤ 250 lines; the existing "Deferred obligations — READ FIRST" section stays first). `docs/2_research-and-design-ledger.md` § Change log gains one dated line: "2026-09-xx — build memory committed under `docs/build/` (ADR-004); `.agents/` retired."
8. **Validation**: `check-build-memory.sh .` green; the `req_id_pattern: REQ-[A-Z]+-[0-9]+` line added to the manifest so the REQ coverage check runs against `docs/rhema-design-spec.md`; any coverage finding recorded as an OPEN FINDINGS entry, not fixed here.

### Out of scope
- Renaming numbered docs; editing DEFERRALS rows, tickets, research or design notes; running G1; any code change; merging.

### Acceptance criteria
- [ ] `bash ~/agent-skills/skills/build-memory/scripts/check-build-memory.sh .` exits 0; `test ! -d .agents`; `ls docs/build/runs | wc -l` = 22 and every name matches `<ID>.md` with an id present in the manifest chain table. *(deterministic)*
- [ ] `docs/build/LEDGER.md`: key set per BM-LEDGER-02; `lastCompleted` and `chainTip` equal `gh pr list` / `git branch --show-current` reality; PHASE LOG has ≥ 24 "done" entries + the seeds; `grep -c 'D-P0' docs/build/LEDGER.md` > 0. *(deterministic)*
- [ ] `docs/build/BUILD_INDEX.md` has 25 rows (24 PRs + insert #7) and every evidence path exists; `ls docs/build/pr | wc -l` = 24. *(deterministic)*
- [ ] `docs/adr/` has ADR-001…004 with `## Revisit trigger`; `README.md` generated marker present; `adr-index.sh` regenerates byte-identically. *(deterministic)*
- [ ] `docs/README.md` has one row per top-level `docs/` entry with a mode; `AGENTS.md` ≤ 250 lines and contains `docs/build/LEDGER.md`, `docs/build/runs/`, `DEFERRALS.md`. *(deterministic)*
- [ ] The manifest chain table has a row for every `NN*_` file in `docs/tickets/` except the listed companions (validator check), including `16_P0.15e` … `16_P0.15i` and `16z_MAINT.1`; `16a_GATE-G1` no longer points at `.agents/scratch`. *(deterministic)*
- [ ] Moved files byte-identical (`cmp` on all 22 + the ledger's retained sections). *(deterministic)*
- [ ] `pnpm turbo build lint typecheck` and `pnpm test` unchanged from the base (no code touched). *(deterministic)*
- [ ] A fresh session given only `AGENTS.md` + `docs/tickets/00_MANIFEST.md` can state the next chain row, where the ledger is, and what is owed before G1 (reviewer check). *(agentic)*
- [ ] **Phase gate:** CI green; `BUILD_INDEX.md` row for MAINT.1; `LEDGER.md` closed by the worker with `nextTicket` pointing at the next chain row. *(deterministic)*

### Requirement IDs to satisfy and stamp in the PR
`BM-MIGR-01..07`, `BM-COMPAT-04..05`; project: REQ-ARCH-37 (record), REQ-BETA-22 (gate readouts on file — the readouts dir is created).

### Cross-cutting invariants
- **Append-only:** research ledger and DEFERRALS gain lines, never lose them; historical tickets untouched.
- **Nothing cited that was not read:** every reconstructed PHASE LOG entry cites its PR number.
- **No secrets:** validator's secret grep green (the run ledgers contain GCP project ids and run ids, which are fine; tokens are not).

### Notes
- This ticket owns `docs/build/` in this repo and the ADR numbering start (ADR-001…004 are records, not new decisions).
- Re-confirm at build time: whether P0.15i has a PR; the exact `pinnedBaseSha`; the DEFERRALS row count (60 `D-*` rows as of 2026-09-09); which `00a` items are provably done.

---

# Appendix A — Requirement index

| Id | Where specified | Ticket |
|---|---|---|
| BM-ROOT-01..02 | I §3 | SK.1 |
| BM-LAYOUT-01..05 | I §3 | SK.1 |
| BM-LEDGER-01..07 | I §4 | SK.1 |
| BM-MANIFEST-01..04 | I §5 | SK.1 |
| BM-TICKET-01..05 | I §5 | SK.1 |
| BM-TAIL-01..03 | I §5 | SK.1 |
| BM-DEFER-01..02 | I §6 | SK.1 |
| BM-ADR-01..04 | I §6 | SK.1 |
| BM-INDEX-01..03 | I §6 | SK.1 |
| BM-GATE-01..04 | I §7 | SK.1 |
| BM-COMPAT-01..05 | I §9 | SK.1 (04–05 also EL.1, RH.1) |
| BM-VALID-01..02 | I §10 | SK.1 |
| BM-SYNTH-01..05 | I §11.2 | SK.1 |
| BM-RECON-01..03 | I §11.3 | SK.1 |
| BM-DOCS-01..02 | I §11 / SK.1 items 7–8 | SK.1 |
| BM-MIGE-01..07 | II EL.1 | EL.1 |
| BM-MIGR-01..07 | II RH.1 | RH.1 |

# Appendix B — Decisions taken on the operator's behalf (veto here)

1. `docs/tickets/DEFERRALS.md` stays beside the tickets rather than moving under `docs/build/` (D7).
2. Committed run ledgers are named `runs/<ID>.md` with the date inside the file; scratch mode keeps the branch/date name.
3. The worker, not the orchestrator, advances the ledger (D4).
4. `GATE-ACCEPT` is a marker row after `CAP.3`, so the accepted-deviations signature is a first-class gate.
5. `synthesize-spec` is one skill with five modes rather than two skills; `drive-build.sh --skill` drives its ledger.
6. Eleutheria's second capstone (P23.x) is a delta over P19–P20, instantiated from the tail templates; DOC rows are omitted there because P22.x already ran.
7. Rhēma's retro-fitted ADRs start the numbering at ADR-001; the migration ticket is inserted as `16z_MAINT.1` before G1.
8. Historical ticket filenames are never renamed in either repo.
