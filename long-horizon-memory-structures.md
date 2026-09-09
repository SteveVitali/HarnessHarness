# Long-horizon build memory: `~/agent-skills` vs. Eleutheria vs. Rhēma

**Date:** 2026-09-09 · **Author:** Claude (Fable 5.1) for Steve Vitali · **Ledger:** `~/MetaHarness/.agents/scratch/lh-memory-meta-analysis-ledger.md`
**Inputs read:** every SKILL.md/README/script in `~/agent-skills`; Eleutheria `docs/` (spec Part 0, research `_meta`, all 34 `docs/build` files, ADR set, tickets P19–P22, manifest, template, AGENTS.md, scratch README, the post-build machine ledger); Rhēma `docs/` (founding prompt, both research/design ledgers, spec front matter + Appendix map, decomposition prompt, manifest, DEFERRALS, AGENTS.md, build ledger, per-ticket ledgers), plus both git histories.

---

> **Amendments (2026-09-09, after operator review).** Two conclusions below were superseded when this analysis was turned into the implementation spec `~/MetaHarness/build-memory-v2-spec.md`: (1) build memory is **committed** under `docs/build/` and the gitignored `.agents/scratch/` is retired (only `docs/build/logs/` stays ignored), which replaces §6.1 principle 1 and the `.agents/scratch` half of §6.2; (2) the capstone is decomposed into tail tickets **whenever a build has more than one ticket**, replacing the threshold discussed in §4 G7 and §8 question 3. The spec is authoritative where the two differ.

## 0. The answer in one page

**What the skills assume.** `decompose-spec` → `orchestrate-build` → `implement-spec` model the build as: *a finished spec arrives; it is cut into gitignored, regenerable ticket files; one gitignored machine ledger is the only cross-session memory; the build ends at CAPSTONE → DONE.* Every durable artifact the skills name lives under `.agents/scratch/`. Nothing is committed. Nothing exists before the spec or after DONE.

**What both projects actually needed.** Both projects independently grew the same five things the skills do not provide, and both reversed the skills' one explicit storage rule:

1. **An upstream pipeline** (founding prompt → research ledger → research/design notes → versioned, ratified spec → decomposition hand-off prompt), run with hand-built ledgers that have the *same* shape in both repos (status vocabulary, owner vocabulary, evidence column, operator-decisions register).
2. **Committed tickets.** Both committed `docs/tickets/` (Rhēma from day one of the build; Eleutheria at P19.1 with ADR-058 stating tickets "are NOT regenerable — no decompose ledger or inputs on disk"). The skill still says the tickets dir "must be gitignored".
3. **A cross-ticket obligations ledger.** Rhēma: `docs/tickets/DEFERRALS.md`, read at the start of every run, gate-blocking. Eleutheria: reconstructed after the fact as `LEDGER_DEFERRALS.md` (90 rows mined from 44 run ledgers) and then normalized into `BACKLOG.csv` — expensive archaeology that a committed companion file would have avoided.
4. **Human gates that pause without blocking.** Eleutheria added `GATE PROTOCOL`, an append-only `GATE DECISIONS` table, a `RETURN PASS` table and a `returnPass:` key to the machine ledger; Rhēma slotted `HUMAN-H*` and `GATE-G*` marker docs into the chain. The skill treats an unmet gate as `blockedOn`, which stalls the chain.
5. **A post-capstone lifecycle.** Eleutheria ran five more phases after the 46-ticket chain: capstone decomposed into four tickets (because one fresh context could not judge a 668-requirement spec), backlog unification, spec reconciliation with fold-back requirement IDs, integration/release plan, live operationalization, and a docs-refresh pair. The skills end at `projectStatus: DONE`.

**Where they differ, and which to keep.** Rhēma is the cleaner *forward* pipeline: sequence-prefixed ticket filenames, marker docs for human/gate rows, skeleton tickets for post-gate phases, a self-contained spec with a requirement index and decision register, `DEFERRALS.md` plus the AGENTS.md read-first rule, and a committed decomposition prompt. Eleutheria is the stronger *record*: one ADR per ticket with a mechanically enforced revisit trigger and an index that must equal the spec's ADR appendix, a per-ticket risk register and traceability append, the spec as a build artifact (`spec_src/` + `BUILD.sh` + byte-identity check), a committed `docs/build/` memory with CSV ledgers and validators, and a docs map that classifies every doc by mutability. Rhēma's weakness is that its machine ledger was never advanced (CURRENT STATE still `NOT_STARTED` after 22 ticket runs) so there is no completion log outside git; Eleutheria's weakness is that its memory was created post hoc, so it carries three ledger-naming styles, four overlapping backlogs that had to be merged, and a flat 34-file `docs/build/`.

**The proposal (§6–§7).** Encode one layout and one lifecycle in the skills:

- `docs/` holds everything with audit value, committed: brief, research ledger + notes, design notes, the canonical spec (optionally assembled from `spec_src/`), `adr/`, `tickets/` (manifest, template, sequenced contracts, HUMAN/GATE markers, `DEFERRALS.md`), and `build/` (`BUILD_INDEX.md`, capstone reports, backlog, reconciliation, readiness, validators).
- `.agents/scratch/` holds only per-run machine state, gitignored, with a fixed six-directory layout and deterministic ticket-id-based filenames.
- One machine file (`<build>-build-ledger.md`) carries only *state*: CURRENT STATE, GATE DECISIONS, RETURN PASS, OPEN FINDINGS, PHASE LOG. Every *plan* section it currently duplicates lives in the committed manifest and is cited, not copied.
- Two new skills (`research-to-spec` upstream; `reconcile-build` for the closeout), and concrete edits to the five existing ones (§7), plus one deterministic validator script that checks the layout before any LLM work.

---

## 1. What `~/agent-skills` assumes today

### 1.1 The artifacts the skills name

| Artifact | Producer | Consumer | Location (as written) | Committed? | Machine-parsed? |
|---|---|---|---|---|---|
| Canonical spec | *(given)* | all three | anywhere (`spec` input) | n/a | no |
| Build ledger `<build>-build-ledger.md` | decompose-spec §5 | orchestrate-build, `drive-build.sh` | `$SCRATCH` | **never** | yes: `projectStatus`, `nextTicket`, `pauseRequested`, `blockedOn`, `buildWorktree` |
| Ticket contracts `T<nn>__slug.md` / `P<p>.<k>__slug.md` | decompose-spec §3 | implement-spec (`spec=` input) | `$SCRATCH/<build>-tickets/` or `tickets_dir` | **"must be gitignored"** | no |
| `00_MANIFEST.md` | decompose-spec §5 | human (manual floor) | beside tickets | never | no |
| Run ledger `implement-spec_<branch>_<date>.md` | implement-spec 0.4 | the same run (resume) | `$SCRATCH` | never | no |
| PR body + evidence report | implement-spec 6.3/6.4 | GitHub, operator | inline to `gh` / chat | PR only | no |
| `drive-build-logs/iter-NNN-<unit>.log` | drive-build.sh | human | `<ledger dir>/drive-build-logs/` | never | no |
| AGENTS.md hierarchy, `agent_docs/` | agent-docs | implement-spec 0.3, decompose-spec §0 | repo tree | yes | detector only |
| README/docs/ADRs | refresh-repo-docs | humans | repo tree | yes | detector only |

`$SCRATCH = ${AGENT_SCRATCH_DIR:-<main-worktree>/.agents/scratch}`. The skills say nothing about subdirectories inside it.

### 1.2 The lifecycle the skills cover

```
[spec exists] → decompose-spec → SETUP → T1 … Tn (implement-spec each) → CAPSTONE → DONE
```

Stated boundaries: decompose-spec "does not invent a missing design" (hard stop); orchestrate-build does "no merge-main, no CI polling, no notifications"; implement-spec does "no merge-main, no CI polling". Nothing produces ADRs, a deferrals ledger, a backlog, a build index, a release plan, or a research artifact. `refresh-repo-docs` knows ADRs exist (it marks them superseded, never deletes) but nothing writes them.

### 1.3 Explicit storage rules in the skills

- Run ledger and build ledger: "must be gitignored; never commit."
- Tickets: "derived build scaffolding, regenerated from the spec, never committed — add the ignore line if missing." The 2026-09-01 commit message rejects a `docs/tickets` default because it "would assume a docs/ convention and make a planning skill mutate the working tree by default."
- implement-spec 6.1 excludes `.agents/` from staging unless the spec includes it.

These three rules are the ones both projects overrode. §4 G1 treats this as the central contradiction.

---

## 2. What the two projects built

### 2.1 The observed lifecycle (union of both)

| Stage | Eleutheria | Rhēma | Skill today |
|---|---|---|---|
| S0 Brief | `docs/1_deep_research_overview.md` (landscape + brief for a design agent) | `docs/1_founding-prompt.md` | none |
| S1 Research plan | implicit in `docs/research/_meta/SPEC_OUTLINE.md` + `OUTLINE_TRACE.md` (480 `OL-*` obligations) | `docs/2_research-and-design-ledger.md` (rows A–O, Q; owner vocab R/D/A/P/S) | none |
| S2 Research/design notes | `docs/research/R1..R13` under `_meta/CONVENTIONS.md` (finding format with verification status + emitted `REQ-R<n>-<m>`) | `docs/research/1..13`, `docs/design/1..19` | none |
| S3 Spec synthesis | `spec_src/*.md` + `BUILD.sh` → `2_canonical_design_spec.md`; adversarial `GAP_ANALYSIS.md` vs the outline; `LEAD_SPOTCHECKS.md` | spec v0.2 → reviews (design/11–13) → external critique + crosswalk (design/14) → round-2 ledger → v0.3 → delta review (design/19) → **v1.0 ratified** | none (decompose-spec hard-stops if the design is missing) |
| S4 Hand-off | ticket template hand-written; tickets hand-authored | `docs/6_ticket-decomposition-prompt.md` (committed) → `decompose-spec tickets_dir=docs/tickets` | decompose-spec |
| S5 Build | 46 tickets hand-chained (Devin), run ledgers in scratch | 22 ticket runs hand-chained (Claude Code), run ledgers in scratch | implement-spec (+ orchestrate-build, unused by both for the main chain) |
| S6 Capstone | decomposed into P19.2–P19.5 with `COVERAGE_MATRIX.csv`, seam hunt, composed E2E, closure + operator-signed ACCEPTED list | not yet reached (Phase 0 exit gate G1 pending) | orchestrate-build §3, single fresh context |
| S7 Closeout | P20.1 backlog + readiness; P20.2 spec reconciliation (`TICKET_VS_SPEC.md`, fold-back IDs, Appendix F/G, ADR-062); P20.3 integration/release plan | `phase0-exit-readout.md` per gate | none |
| S8 Operationalize | P21.1–P21.9 behind `HG-*` gates | P0.15b–i "Stage B" live sessions | none |
| S9 Docs | P22.1 `refresh-repo-docs`, P22.2 `agent-docs` as the last two tickets | AGENTS.md updated inside tickets | the two docs skills, not sequenced |
| S10 Re-plan | `PLANNING_LEDGER.md` → `DECISION_MEMO.md` (a mini-spec) → decompose again, rows appended to the same manifest | P0.15c/d/e/f/g/h each "synthesize the previous ticket's blockers into one runnable ticket" | orchestrate-build SPLIT/MERGE events only |

Both projects ran the same shape; only the file names differ. The upstream half (S0–S3) is roughly as large as the build in both repos (Eleutheria research cache: 26,862 lines; Rhēma design docs: ~90k words).

### 2.2 Eleutheria layout, classified

| Path | What | Class |
|---|---|---|
| `docs/README.md` | map of the docs tree; a **mode table** (generated / frozen / append-only / historical record / living) assigning a change-mode to every entry | reusable pattern |
| `docs/1_deep_research_overview.md` | the brief | project artifact (role reusable) |
| `docs/2_canonical_design_spec.md` | 9,055-line build artifact; `SIG-<AREA>-<nnn>` IDs, append-only, reserved IDs, RFC 2119, Part 0 "how to use" (§0.4 names the three skills as the execution model; SIG-ENG-003 = the amendment MUST: stop, write an ADR, propose the amendment, proceed under it; SIG-ENG-004/005 = Definition of Done incl. "requirement id appears in the PR" and "unverifiable → risk register with compensating control"; SIG-ENG-037 = traceability regenerated and CI-checked at every phase gate, never hand-maintained), Appendix A traceability to `OL-*`, F ADR index, G corrections | reusable conventions |
| `docs/research/R*.md`, `_meta/{CONVENTIONS,SPEC_OUTLINE,OUTLINE_TRACE,GAP_ANALYSIS,LEAD_SPOTCHECKS}.md`, `_meta/spec_src/ + BUILD.sh` | the research→spec pipeline | reusable pattern (conventions, outline trace, adversarial gap analysis, spec-as-build-artifact) |
| `docs/adr/README.md` + `ADR-NNN-slug.md` ×70 | Context / Decision / Consequences / Alternatives / **Revisit trigger** (test-enforced); Status/Date/Phase/Requirement ids/Spec header; immutable, new ADR supersedes; index ↔ spec Appendix F enforced by `check_spec_src.py`; ADR-001..020 scripted at P00.2, one per ticket after | reusable pattern |
| `docs/tickets/00_MANIFEST.md`, `_TEMPLATE.md`, `PXX.Y__slug.md` ×62 | committed contract record (ADR-058); template fields: Sequence, Phase, base_branch, Depends on, Run line (per-ticket `live_verification`), **Gate status block**, Goal, **Load** ("read these — do not re-read others"), In scope (numbered), Out of scope, ACs tagged *deterministic/agentic* + a universal phase-gate AC, Requirement IDs to stamp, Cross-cutting invariants, Notes (ownership) | reusable template |
| `docs/risk_register.md`, `docs/traceability.md` | per-ticket append (`RISK-P<phase>-<nn>`; requirement → where → test) | reusable "universal phase gate" pattern; content project-specific |
| `docs/build/README.md` | index of build memory + who appends what | reusable |
| `docs/build/BUILD_INDEX.md` | one row per ticket: branch, PR, ledger, ADRs, live-verification status + evidence source | reusable (should be written live, not reconstructed) |
| `docs/build/PLANNING_LEDGER.md`, `DECISION_MEMO.md` | plan-for-the-plan (status glyphs, fresh-context prompts, resume protocol) and the post-build mini-spec with `GATE:` lines | reusable (re-planning round) |
| `docs/build/LEDGER_DEFERRALS.md` | 90 rows in families LD-V/F/D/H/P/X + LH, append-only closure log | reusable id families; the *reconstruction* is the anti-pattern |
| `docs/build/COVERAGE_MATRIX.csv` + `CAPSTONE_GAP_ANALYSIS.md` + `COMPOSED_E2E_REPORT.md` + `CAPSTONE_CLOSURE.md` | the decomposed capstone: verdict enum, routing enum, seam hunt, xfail-tagged-with-LD-id rule, ACCEPTED list with operator signature | reusable schemas |
| `docs/build/BACKLOG.{csv,md}`, `BACKLOG_THEMES.md`, `tools/check_backlog.py` | one backlog; every deferred risk row / ADR revisit trigger / LD row appears in exactly one `sources` cell | reusable schema + validator |
| `docs/build/TICKET_VS_SPEC.md`, `SPEC_RECONCILIATION_PLAN.md`, `tools/check_spec_src.py` | in-spec / spec-implied / ticket-added tagging; amendment table with before/after + tick state; fold-back IDs | reusable pattern |
| `docs/build/OPERATIONAL_READINESS.md`, `INTEGRATION_PLAN.md`, `tools/merge_dryrun.sh` | capability × {code, rights, infra, human} + critical path with `ticket:`/`proof:`; PR-stack merge dry run | reusable pattern |
| `docs/build/{rights/,okc/,live_runs/,CI_STATUS,RELEASE_NOTES,*_REPORT,*_RUNBOOK}` | project artifacts | project-specific (the "empty gated dir with a README" idiom is reusable) |
| `.agents/scratch/README.md` + `ledgers/ pr/ tools/ fixtures/ logs/ planning/` | fixed layout, naming rule `implement-spec_<PXX.Y>.md`, retention, rename mapping | reusable layout |
| `.agents/scratch/planning/sig-postbuild-build-ledger.md` | machine ledger + `OPEN FINDINGS`, `GATE PROTOCOL`, `GATE DECISIONS`, `RETURN PASS`, keys `returnPass:` `mergePolicy:`, an `OPERATING MODE` resume prompt | reusable extensions |
| `AGENTS.md` "Where things live" + nested `web/ db/ connectors/` | tells agents where tickets, build memory, scratch, ADRs live; append-only rule; stacked-PR rule | reusable section |

Drift observed even after unification: 5 skill-named ledgers, duplicate P20.3 ledgers, dates `20260101`/`20260820` in filenames, ~4.5 MB of logs at the scratch root, ADR index out of numeric order, ADR-064 gap, P21-era ADR headers using different field names. Prose conventions did not hold without a validator.

### 2.3 Rhēma layout, classified

| Path | What | Class |
|---|---|---|
| `docs/1_founding-prompt.md` | the brief | role reusable |
| `docs/2_research-and-design-ledger.md` (345 lines) | §0 theses, A ground truth, B research, C–N design streams, O spec synthesis process, **Q operator-decisions register**, change log; status `open→in-progress→done \| blocked-on-operator \| dropped`; owner R/D/A/P/S; every `done` carries an evidence path | reusable ledger |
| `docs/4_…critique.md`, `docs/design/14_…crosswalk.md`, `docs/5_round2-ledger.md` | external review → 81-row crosswalk → round-2 streams A–F, Q-21+ | reusable review round |
| `docs/research/1..13`, `docs/design/1..19` | numbered notes and reviews; ledger rows point at them | reusable structure (would sit under `docs/research/`, `docs/design/`) |
| `docs/rhema-design-spec.md` (4,071 lines) | front matter: Status/version/ratification, delta paragraph, synthesized-from, supersedes; §0.1 "five contracts adopted by reference"; §0.2 `REQ-<FAMILY>-n` (22 families, 368 design IDs preserved); Appendix A requirement index (with AC hooks), B decision register (Q-* + R-* rulings), C research index, D glossary + reserved strings, E review dispositions, F critique dispositions | reusable spec skeleton |
| `docs/6_ticket-decomposition-prompt.md` | committed hand-off: exact skill inputs + project binding constraints + what to report | reusable |
| `docs/tickets/00_MANIFEST.md` (401 lines) | derived-artifact banner, committed banner, deferrals banner, how-to-build, HUMAN rows H0–H5, chain table with `NN_` sequence prefix and HUMAN/GATE marker docs, **milestone gates with thresholds quoted verbatim**, phase gates + ownership notes, 11 invariants, out-of-scope, REQ→ticket index, spec amendments applied, decomposition decisions incl. the Phase-4 review record | the gold-standard manifest |
| `docs/tickets/NN_<ID>__slug.md` ×52 incl. `HUMAN-H*`, `GATE-G*`, `[SKELETON]` P1.x | sequence-ordered; skeletons say "do not run implement-spec until the body is written after G3" | reusable |
| `docs/tickets/DEFERRALS.md` (385 lines, 86 rows) | `D-<ticket>-<n>` rows: item, why deferred, unblocked by, how to verify, proxy now, status `OPEN/DONE/WONTFIX/ACCEPTED-SKELETON/PARTIAL`; never delete; gate-blocking; "a deferral not in this file did not happen" | reusable companion |
| `docs/phase0-exit-readout.md` | gate input document, updated by successive tickets | reusable (per-gate readout) |
| `AGENTS.md` | "Deferred obligations — READ FIRST, EVERY RUN" section; stack, commands, invariants, stages, CI, secrets | reusable section |
| `.agents/scratch/rhema-build-ledger.md` | decompose-spec template verbatim; PLAN/INVARIANTS/OUT-OF-SCOPE cite the manifest; CURRENT STATE never advanced; PHASE LOG has only the 3 seed entries | evidence that the machine ledger is dead weight on the manual path |
| `.agents/scratch/implement-spec_*.md` ×22 | good implement-spec structure (Requirements, ACs, Req IDs, Invariants, **Deferrals read**, Plan, Test matrix, Progress, Gap table, Evidence log); two naming styles | run ledgers |

Rhēma has no ADRs, no risk register, no traceability doc, no build index, and no completion log: after 22 runs the only record of *what landed* is git + PR bodies + DEFERRALS rows + the exit readout. The P0.15 series (b…i, seven sub-tickets over six days, dozens of commits) shows run-time re-planning done entirely by hand, with each new ticket "synthesizing the previous ticket's blockers".

### 2.4 Differences that matter

| Dimension | Eleutheria | Rhēma | Judgment |
|---|---|---|---|
| Ticket filenames | `PXX.Y__slug.md` (phase order, no global sequence) | `NN_<ID>__slug.md` (lexicographic = chain order) + marker docs | **Rhēma.** Global prefix removes the "which is next" question and gives HUMAN/GATE rows a slot. Inserts use `16_P0.15b`; Eleutheria used row `54.5` in the ledger only. |
| Human work in the chain | manifest prose + `Gate status` block in tickets + ledger GATE PROTOCOL/DECISIONS/RETURN PASS | HUMAN/GATE marker docs + verbatim thresholds in the manifest | **Both.** Rhēma's markers for *plan*, Eleutheria's protocol for *run-time state*. |
| Cross-ticket obligations | mined post hoc into `LEDGER_DEFERRALS.md`, then `BACKLOG.csv` | `DEFERRALS.md` maintained per run, read-first rule | **Rhēma**, with Eleutheria's id families (V/F/D/H/P/X) and the backlog normalization as the closeout step. |
| Decision records | ADR per ticket, revisit trigger enforced, index ↔ spec appendix | none (rulings live in spec Appendix B and the ledger's Q register) | **Eleutheria** for build-time decisions; Rhēma's Appendix B for design-time rulings. Both belong. |
| Spec maintenance | `spec_src/` + `BUILD.sh` + byte-identity check + ADR + Appendix F/G | single file, version/status front matter, amendments logged in the manifest | Either is fine; the invariants to keep are: versioned status line, amendment log with before/after, ID append-only, checker for ID grammar. `spec_src/` is worth it above ~5k lines. |
| Completion record | orchestrator PHASE LOG (rich, gitignored) + `BUILD_INDEX.md` (reconstructed) | none | **Neither is right.** Write `BUILD_INDEX.md` at ticket close, committed. |
| Per-ticket run ledgers | 15 of 44 are stubs; PR bodies became the evidence | complete and well-structured | Rhēma's ledgers show the implement-spec structure works when followed; the durable evidence still needs promotion into a committed row. |
| Capstone | decomposed into tickets with CSV matrix | not reached | **Eleutheria** — encode "capstone as tickets" for large specs. |
| Post-build phases | backlog, reconciliation, release plan, readiness, docs refresh | exit readouts per gate | **Eleutheria**, generalized as a closeout skill. |
| Docs map | `docs/README.md` mode table | none | **Eleutheria.** |
| Scratch layout | six dirs + README | flat | **Eleutheria's layout**, with a validator. |
| Research→spec | outline trace + adversarial gap analysis + spot-checks; spec is a build artifact | ledger with theses/owners/Q register; external-critique round; ratification | **Merge.** Rhēma's ledger + decisions register + review rounds; Eleutheria's research conventions, outline trace and superset proof. |

---

## 3. Mapping the artifact classes onto the skills

| Class | Eleutheria | Rhēma | Skill owner today | Verdict |
|---|---|---|---|---|
| Brief / founding prompt | `docs/1_…overview.md` | `docs/1_founding-prompt.md` | — | **gap** (S0) |
| Research plan ledger + decisions register | `_meta/SPEC_OUTLINE`, `OUTLINE_TRACE` | `2_research-and-design-ledger.md` §Q | — | **gap** (S1) |
| Research/design notes + conventions | `docs/research/R*`, `_meta/CONVENTIONS` | `docs/research/*`, `docs/design/*` | — | **gap** (S2) |
| Spec synthesis + adversarial review + ratification | `spec_src`, `GAP_ANALYSIS`, `LEAD_SPOTCHECKS` | v0.2→v0.3→v1.0, design/11–13/19 | — (decompose-spec hard-stops) | **gap** (S3) |
| Spec amendment protocol | `spec_src`+`BUILD.sh`+ADR+App. F/G+checker | front-matter status + manifest log | manifest "spec amendments applied" only | **partial** |
| Decomposition hand-off prompt | — | `6_ticket-decomposition-prompt.md` | decompose-spec inputs (not persisted) | **partial** |
| Ticket contracts | committed | committed | gitignored by rule | **contradiction** |
| Manifest | committed, ~170 lines | committed, ~400 lines | gitignored by rule | **contradiction** |
| Ticket template | `_TEMPLATE.md` (13 fields) | in-manifest conventions | 5-field header in §3 | **partial** |
| HUMAN / GATE rows | manifest prose + gate blocks | marker docs `00a_`, `16a_`… | "marked non-code rows" in the table | **partial** (no marker files, no protocol) |
| Skeleton tickets after a gate | — | `[SKELETON]` P1.01–P1.22 | — | **gap** |
| Deferrals companion | post hoc | `DEFERRALS.md` | — | **gap** |
| Human-gate state (decisions, return pass) | ledger sections | — | `blockedOn` | **gap** |
| Open findings carried to capstone | ledger section | — | — | **gap** |
| Build machine ledger | gitignored, extended | gitignored, unused | decompose-spec §5 | covered, needs slimming + extensions |
| Run ledger | `ledgers/implement-spec_<ID>.md` | branch-named | implement-spec 0.4 | covered, naming rule wrong in practice |
| PR body drafts | `scratch/pr/` (6 naming styles) | inline | inline | **gap** (no deterministic path) |
| Build index / completion log | reconstructed | none | PHASE LOG (gitignored) | **gap** |
| ADRs | 70, enforced | none | refresh-repo-docs marks superseded | **gap** |
| Risk register / traceability per ticket | yes (spec-mandated) | no | — | project-specific, but the *universal phase-gate AC* is reusable |
| Capstone artifacts | 4 files + CSV | — | prose in §3 | **partial** |
| Backlog | `BACKLOG.csv/md` + checker | DEFERRALS doubles as backlog | — | **gap** |
| Spec reconciliation (ticket-added scope, fold-backs) | `TICKET_VS_SPEC`, plan, ADR-062 | — | — | **gap** |
| Release / integration plan | `INTEGRATION_PLAN`, `merge_dryrun.sh` | — | "no merge-main" | **gap** (deliberate, but the *plan* is not a merge) |
| Readiness map | `OPERATIONAL_READINESS` | exit readouts | — | **gap** |
| Docs map with modes | `docs/README.md` | — | refresh-repo-docs (no modes) | **partial** |
| Scratch layout + retention | README + 6 dirs | flat | `$SCRATCH` only | **gap** |
| AGENTS.md "where memory lives / read first" | yes | yes | agent-docs template lacks it | **gap** |
| Re-planning round (memo → tickets appended) | `PLANNING_LEDGER`, `DECISION_MEMO` | P0.15 chain by hand | SPLIT/MERGE only | **gap** |
| Stage A (offline) / Stage B (live, paid, operator-gated) inside a ticket | HG-gated tickets | P0.15b–h | implement-spec 5.3 assumes it can run | **gap** |

---

## 4. Gaps and contradictions, ranked

Severity: **S1** = the chain breaks or memory is lost without it; **S2** = expensive rework observed in a project; **S3** = drift/hygiene.

**G1 · S1 · Committed vs. gitignored tickets (contradiction).** Both projects committed `docs/tickets/` and the manifest. The stated reason (ADR-058) is decisive: the tickets are not regenerable, because they encode decomposition-time decisions (ownership notes, shared-decision assignment, AC subsetting, gate placement, amendments applied) that the spec does not contain, and they are the only durable record of each PR's contract. The skill's "derived scaffolding" premise is false in practice. The skill's other reason, "a planning skill must not mutate the tree by default", is a real concern but is answered by making the write explicit and committing it as a docs-only PR. *Fix:* `tickets_dir` defaults to `docs/tickets`, committed; the scratch default is retained only for `ledger=false` dry runs.

**G2 · S1 · No upstream stage.** Both projects spent roughly half their effort before decompose-spec could run, with hand-built ledgers of identical shape. decompose-spec's hard stop ("needs a design pass first") points at a skill that does not exist. *Fix:* a `research-to-spec` skill (§6.3).

**G3 · S1 · Human gates stall the chain.** The skill maps an unmet gate to `blockedOn`; `drive-build.sh` exits 2. Eleutheria's operator wanted "skip for now, run ungated, re-run later" for 7 of 9 gated tickets. *Fix:* GATE PROTOCOL + GATE DECISIONS + RETURN PASS + `returnPass:` in orchestrate-build and the ledger; a `Gate status` block in the ticket template; `drive-build.sh` treats a gate as a pause, not a block.

**G4 · S1 · No committed cross-ticket obligations ledger.** implement-spec records deferrals in the run ledger and the PR; nothing makes the *next* ticket read them. Eleutheria paid for this with a 44-ledger archaeology (`LEDGER_DEFERRALS.md`, then `BACKLOG.csv`). Rhēma's `DEFERRALS.md` + AGENTS.md read-first rule is the fix, already proven over 86 rows. *Fix:* seed `docs/tickets/DEFERRALS.md` in decompose-spec; implement-spec 0.3 reads it and 6.4 appends; gates refuse to pass with OPEN rows in scope.

**G5 · S2 · No completion record outside a gitignored file.** Rhēma has none; Eleutheria reconstructed `BUILD_INDEX.md` from PR bodies and 44 ledgers of varying quality. *Fix:* orchestrate-build 2.3 (and the manual floor) appends one row to a committed `docs/build/BUILD_INDEX.md` at ticket close: ticket, branch, PR, base, ADRs, deferrals opened/closed, live-verification status, evidence pointer.

**G6 · S2 · No ADR step.** Eleutheria's spec mandated ADRs (SIG-ENG-003) and the build produced 70, one per ticket in the ticket's PR, with a test-enforced revisit trigger and an index kept equal to the spec's appendix. Rhēma has zero, so its build-time decisions (AWS→GCP, protocols committed, SST→Pulumi) live only in manifest amendment paragraphs. implement-spec already computes the input (Phase 4.2 "sound deviation", 6.4 "deviations/deferrals"); it just never writes the file. *Fix:* implement-spec 6.1a "decision record": any SHOULD-deviation, any new shared decision named in the ticket's Notes, and any design choice the gap analysis calls MET-DIFFERENTLY → `docs/adr/ADR-NNN-slug.md` from a fixed template + index row, in the same PR. decompose-spec seeds `docs/adr/README.md` + `_TEMPLATE.md`.

**G7 · S2 · Capstone assumes one fresh context.** A 668-requirement whole-spec gap analysis does not fit one context with rigor headroom (the same constraint decompose-spec applies to tickets). Eleutheria decomposed it into gap analysis → composed verification → spine wiring → closure, with a CSV matrix as the shared state and an independence rule (verdicts before reading ledger self-assessments). *Fix:* orchestrate-build §3 gains "capstone as tickets" with named artifacts and schemas (`COVERAGE_MATRIX.csv`, `CAPSTONE_GAP_ANALYSIS.md`, `COMPOSED_E2E_REPORT.md`, `CAPSTONE_CLOSURE.md` + ACCEPTED list signature), selected when the spec's requirement count or ticket count exceeds a threshold.

**G8 · S2 · Nothing after DONE.** Backlog unification, spec reconciliation (ticket-added scope → fold-back IDs; amendments with before/after and tick state), integration plan (a dry-run script, not a merge), readiness map, docs refresh. Every one of these was needed once the chain ended. *Fix:* a `reconcile-build` skill invoked by orchestrate-build after CAPSTONE (§6.3), and the two docs skills scheduled as the last two tickets by decompose-spec.

**G9 · S2 · Run-ledger and PR-body paths are not deterministic.** Three naming styles in Eleutheria (even after a written rule), two in Rhēma, bogus dates, duplicate ledgers, PR bodies in six styles. Anything a fresh context must *find* needs a computable path. *Fix:* `.agents/scratch/ledgers/implement-spec_<TICKET-ID>.md` (date inside the file), `.agents/scratch/pr/<TICKET-ID>_pr_body.md`; ticket id comes from the ticket file header, branch derives from it; a validator checks the layout.

**G10 · S2 · Manifest and machine ledger duplicate the plan.** Rhēma's ledger cites the manifest ("verbatim list: manifest §…"); Eleutheria's copies invariants from the template. Two projections of one plan drift (Eleutheria's PHASE PLAN lists P22.x twice). *Fix:* the ledger holds state only (CURRENT STATE, GATE DECISIONS, RETURN PASS, OPEN FINDINGS, PHASE LOG); PHASE PLAN, invariants, out-of-scope, SETUP and CAPSTONE checklists live in the committed manifest and the ledger points at them.

**G11 · S2 · Re-planning rounds have no procedure.** Eleutheria: planning ledger → decision memo (a mini-spec) → decompose again → rows 47–65 appended to the same manifest, `canonicalSpec` re-pointed at the memo. Rhēma: seven hand-written P0.15 sub-tickets. *Fix:* decompose-spec `mode=extend` (append rows to an existing manifest/ledger from any authoritative document, preserve companions), plus the manifest insert convention (`16_P0.15b`, `54.5`).

**G12 · S2 · Offline vs. live/paid stages.** Rhēma's "Stage A (offline, PR-able) / Stage B (live, operator-authorized, budgeted)" split and Eleutheria's `provided: yes/no` credential gates are the same need: implement-spec 5.3 live verification that needs infrastructure, secrets or spend the worker cannot obtain. *Fix:* the ticket template carries a `Live stage` field (none / offline-only / operator-gated with budget); implement-spec 5.3 records "not run: gate pending" as a DEFERRALS row rather than a failure; secrets are never written to any ledger.

**G13 · S3 · Skeleton tickets.** decompose-spec authors every contract up front; Rhēma correctly refused to write 22 full-IDE bodies before the beta gate reads. *Fix:* a `[SKELETON]` ticket form (title, dependencies, REQ coverage, "do not run") and a manifest rule for when bodies get written.

**G14 · S3 · Scratch layout undefined.** *Fix:* the six-directory layout with a README, plus `scripts/check-build-memory.sh`.

**G15 · S3 · AGENTS.md does not know about build memory.** Both projects added it by hand. *Fix:* agent-docs guidelines gain a "Build memory" section (where tickets/deferrals/build index/scratch live; read DEFERRALS first; append-only rules) generated whenever `docs/tickets/` exists.

**G16 · S3 · Docs modes.** refresh-repo-docs should not "fix" a historical record or a generated spec. *Fix:* honour a `docs/README.md` mode table (generated / frozen / append-only / historical / living); report-only on non-living docs.

**G17 · S3 · Spec amendment protocol is thin.** The manifest log exists, but nothing says *how* an amendment is made (source file vs. built file, ID append-only, before/after text, who approves, ADR). Eleutheria's spec carries the protocol as a MUST (SIG-ENG-003: "stop, record the finding as an ADR, propose the amendment, and proceed under the amendment; never silently implement something different"), which is exactly the text the skill should carry. *Fix:* a short protocol in decompose-spec §5 and implement-spec 0.3/4.2 ("spec drift found → ADR + amendment proposal in DEFERRALS or the manifest, never a silent ticket-level fork").

**G18 · S3 · Ticket template is thinner than either project's.** Missing from the skill: Load list with the "do not re-read others" rule, Gate status, Notes/ownership, deterministic/agentic tags, universal phase-gate AC, Requirement IDs to stamp. *Fix:* adopt Eleutheria's template with Rhēma's header.

**Observed but not a gap:** neither project used `drive-build.sh` for the main chain (Rhēma: `dispatch_target=manual`; Eleutheria: Devin subagents with a foreground-only constraint). The manual floor is the primary path in practice, so the manifest must be complete on its own. That is consistent with the skills' design; it just raises the bar on the manifest.

---

## 5. Where the projects deviated from the skills and whether they were right

| Deviation | Right? | Why |
|---|---|---|
| Commit tickets + manifest | **Yes** | not regenerable; audit trail; cited by path from later tickets |
| Keep machine ledger gitignored | **Yes** | per-run state, changes every iteration, would pollute history |
| Rename run ledgers to ticket id | **Yes** | the branch-and-date name is not computable by the next context |
| Re-point `canonicalSpec` at a decision memo | **Yes, as an interim** | the memo was the authoritative document for that round; the skill should support "extend from any authoritative doc" |
| `buildWorktree` = main checkout, no base switch | **Yes, for a hand-chained stacked build** | the sibling-worktree default exists for headless parallel safety; single-driver builds don't need it |
| Gates skipped → run ungated → return pass | **Yes** | the alternative is a stalled chain waiting on outreach/legal/credentials |
| Capstone decomposed into tickets | **Yes** | context-size constraint applies to the capstone too |
| Rhēma never advancing the machine ledger | **No** | cost them the completion log; symptom of G10 (the ledger had nothing they needed once the manifest existed) |
| Eleutheria's post-hoc memory reconstruction | **Necessary, but the need was avoidable** | G4/G5/G9 |
| Docs skills run as the last two tickets | **Yes** | docs converge on the finished code; this should be the default tail of every chain |

---

## 6. The reusable substructure

### 6.1 Principles (derived from what held and what drifted)

1. **Commit by audit value and regenerability, not by "derived".** If a fresh context or a human will cite it by path, or if it cannot be regenerated byte-for-byte from something committed, it is committed. Tickets, manifest, deferrals, ADRs, build index, capstone reports: committed. Run ledgers, PR drafts, logs, the machine ledger: scratch.
2. **One machine-state file, and it holds only state.** Everything that is a *plan* is in the committed manifest; the ledger cites it.
3. **Every cross-ticket obligation has a committed home, an id, and a validator.** Deferrals (`D-<ticket>-<n>`), decisions (`ADR-NNN`), backlog (`BL-nnn`), gates (`G<n>`/`HG-nn`), findings carried to capstone. "A deferral not in the file did not happen."
4. **Deterministic before LLM; regenerate, don't hand-maintain.** Paths are computed from the ticket id; layouts are checked by a script; derived tables (coverage, traceability, ADR index, REQ→ticket index) are regenerated and checked, not edited. Conventions written only in prose drift within days (Eleutheria) or never get followed (Rhēma's ledger); Eleutheria's own gap analysis found a correction table that had not been propagated 97 lines away from the section designed to record it, and made regeneration a MUST (SIG-ENG-037).
5. **Append-only history, promotion at fixed moments.** Scratch → committed happens at ticket close (BUILD_INDEX row, DEFERRALS rows, ADR), at gate reads (readout), and at closeout (reports). Nothing is rewritten; supersession is a new row or a new ADR.
6. **The manifest is the manual floor and must be complete alone.** In both projects it was the actual driver.
7. **Memory types map to homes.** Using the taxonomy from the MetaHarness harness-engineering survey (§5.1): *procedural* → skills, AGENTS.md, conventions; *durable session state* → machine ledger, run ledgers; *artifact memory* → spec, tickets, ADRs, reports; *episodic* → PHASE LOG, DEFERRALS history, GATE DECISIONS, risk register. Each artifact below is one of these and lives where that type lives.

### 6.2 Canonical layout

```
<repo>/
├── AGENTS.md                      # + "Build memory" section (where things live; read DEFERRALS first; append-only rules)
├── CLAUDE.md                      # @AGENTS.md
├── docs/
│   ├── README.md                  # map of docs/ with a mode column: generated | frozen | append-only | historical | living
│   ├── brief.md                   # S0: founding prompt / landscape brief (frozen)
│   ├── research-ledger.md         # S1: research + design work list, theses, operator-decisions register Q-*, change log (append-only)
│   ├── research/                  # S2: numbered notes NN_slug.md under research/CONVENTIONS.md (frozen once cited)
│   ├── design/                    # S2: numbered design notes and review rounds (frozen once cited)
│   ├── <build>-spec.md            # S3: the canonical spec (generated if spec_src/ exists, else frozen-with-amendments)
│   ├── spec_src/ + BUILD.sh       # optional, recommended above ~5k lines
│   ├── decomposition-prompt.md    # S4: the committed hand-off (frozen)
│   ├── adr/                       # README.md index + _TEMPLATE.md + ADR-NNN-slug.md (append-only, immutable)
│   ├── tickets/                   # committed contract record (historical once run)
│   │   ├── 00_MANIFEST.md
│   │   ├── _TEMPLATE.md
│   │   ├── DEFERRALS.md           # hand-maintained companion, append-only
│   │   ├── NN_<ID>__slug.md       # contracts; NN = global sequence; inserts NNa_/NNb_
│   │   ├── NNa_HUMAN-H<k>__slug.md, NNb_GATE-G<k>__slug.md   # marker docs, never implement-spec inputs
│   │   └── readouts/GATE-G<k>.md  # gate readouts (append-only)
│   └── build/                     # durable build memory (append-only / historical)
│       ├── README.md
│       ├── BUILD_INDEX.md         # one row per ticket, written at ticket close
│       ├── COVERAGE_MATRIX.csv, CAPSTONE_GAP_ANALYSIS.md, COMPOSED_E2E_REPORT.md, CAPSTONE_CLOSURE.md
│       ├── BACKLOG.csv, BACKLOG.md, TICKET_VS_SPEC.md, SPEC_RECONCILIATION_PLAN.md
│       ├── INTEGRATION_PLAN.md, OPERATIONAL_READINESS.md
│       ├── planning/              # re-planning rounds: <date>_planning-ledger.md, <date>_decision-memo.md
│       └── tools/                 # validators (check_backlog, check_coverage_matrix, check_spec_ids, merge_dryrun)
└── .agents/scratch/               # gitignored; layout checked by scripts/check-build-memory.sh
    ├── README.md
    ├── <build>-build-ledger.md    # the one machine-state file
    ├── ledgers/implement-spec_<ID>.md
    ├── pr/<ID>_pr_body.md, <ID>_commit_msg.txt
    ├── tools/  fixtures/  logs/   # logs deletable; tools/fixtures kept as provenance
    └── drive-build-logs/
```

Project-specific registers (Eleutheria's risk register, traceability, rights packets) sit beside these under `docs/` and are cited from the ticket template's universal phase-gate AC when the spec mandates them.

### 6.3 Lifecycle with owners

| Stage | Owner | Produces | Consumes |
|---|---|---|---|
| S0 brief | human | `docs/brief.md` | — |
| S1–S3 research → design → spec | **new `research-to-spec`** | `research-ledger.md` (theses, streams, owners, evidence, Q-register), `research/*`, `design/*`, spec vN with front matter (status, version, delta, synthesized-from, supersedes), Appendices A–F pattern, adversarial superset review (Eleutheria `OUTLINE_TRACE` + `GAP_ANALYSIS` when a brief exists), review rounds, **ratification** as a status flip | brief, operator answers |
| S4 decompose | `decompose-spec` (revised) | manifest, template, DEFERRALS seed, sequenced contracts, marker docs, skeletons, ADR dir seed, BUILD_INDEX seed, docs/README map, scratch README, machine ledger (state only), `decomposition-prompt.md` persisted | ratified spec |
| S5 build | `orchestrate-build` + `implement-spec` (revised) | per ticket: PR, ADR(s), DEFERRALS rows, BUILD_INDEX row, run ledger, PR draft; per gate: readout + GATE DECISIONS | manifest, DEFERRALS, AGENTS.md |
| S6 capstone | `orchestrate-build` §3, decomposable into capstone tickets | coverage matrix, gap analysis, composed E2E, closure + ACCEPTED signature | whole spec, BUILD_INDEX, DEFERRALS |
| S7 closeout | **new `reconcile-build`** | BACKLOG (one source per item), TICKET_VS_SPEC, spec reconciliation + fold-back IDs, INTEGRATION_PLAN (dry-run, no merge), OPERATIONAL_READINESS, release notes draft | capstone artifacts, DEFERRALS, ADR revisit triggers, risk register if present |
| S8 docs | `refresh-repo-docs`, `agent-docs` | as the last two chain tickets | finished code |
| S9 re-plan | `decompose-spec mode=extend` | planning ledger + decision memo under `docs/build/planning/`, rows appended to the manifest | backlog, readiness |

### 6.4 Templates to encode (the minimum set)

- **Machine ledger v2** — `CURRENT STATE` (existing 13 keys + `returnPass:` + `mergePolicy:` + `manifest:`), `OPEN FINDINGS`, `GATE DECISIONS` (date · ticket · gate · item · answer · consequence; secrets never, only `provided: yes/no`), `RETURN PASS` (ticket · gates · what the operator must do · re-run line), `PHASE LOG`. A fixed `OPERATING MODE` header with the verbatim resume prompt.
- **Manifest v2** — banners (committed contract record; cite-don't-copy; deferrals companion), how-to-build, HUMAN rows, chain table (`NN_` prefixed, marker rows interleaved, Gate column), milestone gates with thresholds verbatim, phase gates + ownership notes, invariants, out-of-scope, REQ→ticket index, spec amendments applied, decomposition decisions (incl. the Phase-4 review record), plan extensions log.
- **Universal Definition of Done** (from Eleutheria SIG-ENG-004/005, generalized): behaviour on the branch; an automated test that fails if it is removed; the requirement id stamped in the PR; anything not automatically verifiable recorded as a deferral with its compensating control, never as "done". Placed in the manifest and referenced by every ticket's phase-gate AC.
- **Executed contracts are annotated, never rewritten.** When a later reconciliation changes a requirement, the affected ticket file gets an appended note pointing at the amendment (as P20.2 did to P02.1), so the historical contract stays legible.
- **Ticket v2** — header: Sequence n of N · Phase · Tag (beta/post-beta etc.) · base_branch · Depends on · Run line (with per-ticket flags) · Gate status block · Live stage. Body: Goal · Load (with "do not re-read others") · In scope (numbered, each with REQ ids) · Out of scope (naming the owning ticket) · ACs tagged deterministic/agentic + the universal phase-gate AC · Requirement IDs to stamp · Cross-cutting invariants · Notes (decisions this ticket owns, anchors to re-confirm). Skeleton variant.
- **DEFERRALS.md** — Rhēma's rules verbatim; row schema (id, item, why, unblocked by, how to verify, proxy now, status); statuses `OPEN/DONE/WONTFIX/ACCEPTED-SKELETON/PARTIAL`; Eleutheria's family letter as an optional `kind` column (V/F/D/H/P/X).
- **ADR** — Eleutheria's header (Status/Date/Ticket/Requirement ids/Spec) + sections Context/Decision/Consequences/Alternatives/Revisit trigger; README index in numeric order with title and ticket; rule "decisions are immutable; a change is a new ADR".
- **BUILD_INDEX.md row** — ticket · branch · PR · base · commits · ADRs · deferrals opened/closed · live-verification status (run / fixture-only / n/a / gate-pending) · evidence pointer.
- **COVERAGE_MATRIX.csv** — `id, level, spec_section, class, verdict, evidence, owning_tickets, tests, adrs, routing, note` with the verdict enum from orchestrate-build §3 and Eleutheria's class enum.
- **BACKLOG.csv** — `bl_id, title, type, sources, req_ids, package, blocks, landing, gate, size, status`; invariant: every source id appears in exactly one row.
- **docs/README.md** — entries with mode column.
- **Scratch README** — layout, naming, retention.
- **AGENTS.md "Build memory" section** — five bullets: where tickets/manifest are; DEFERRALS read-first rule; where build memory is; scratch is gitignored; append-only + stacked-PR + no-merge rules.
- **research-ledger.md** — Rhēma's header (status vocab, owner vocab, ground truth read, ⚑ fixed constraints), §0 theses, lettered streams, §O synthesis process, §Q operator-decisions register, change log; `research/CONVENTIONS.md` from Eleutheria (finding format with verification status, outline delta, emitted requirements).

### 6.5 Validators (deterministic before LLM)

`scripts/check-build-memory.sh` (bash 3.2, read-only, exit-code gated), run by decompose-spec at seed time, orchestrate-build at each boundary, and optionally in CI:

- `docs/tickets/`: every `NN_` file has the header fields; sequence numbers unique and monotone (letters allowed); every `Depends on` points backward; every marker doc referenced in the manifest chain table; every manifest row has a file.
- `DEFERRALS.md`: ids unique and match `D-<ticket>-<n>`; statuses in the enum; no OPEN row scoped to a gate whose readout says PASSED.
- `docs/adr/`: files ↔ README index rows; every ADR has a `Revisit trigger` section; if the spec has an ADR appendix, sets are equal.
- Spec IDs: grammar from a front-matter line (e.g. `id-pattern: REQ-[A-Z]+-\d+`); every ID cited by a ticket exists; every in-scope ID has exactly one owner in the REQ→ticket index.
- Scratch: only the six dirs + README + ledger at the root; ledgers named by ticket id; no duplicate ledger per ticket; logs older than N days flagged.
- `BUILD_INDEX.md`: one row per completed ticket in the ledger's PHASE LOG.

---

## 7. Per-skill change list

### 7.1 `decompose-spec`

- **Inputs.** `tickets_dir` default → `docs/tickets` (committed); remove the "must be gitignored / add the ignore line" clause; add `mode: seed | extend` (extend appends rows to an existing manifest and ledger from any authoritative document, preserving `DEFERRALS.md`, readouts, and existing tickets); add `sequence_prefix: true` default.
- **Phase 0.** Also read `docs/research-ledger.md` §Q and the spec's decision register if present; read `DEFERRALS.md` and `BACKLOG.csv` in extend mode.
- **Phase 2.** HUMAN and GATE rows become marker files (`NNa_HUMAN-H<k>__slug.md`, `NNb_GATE-G<k>__slug.md`) with thresholds quoted verbatim; post-gate tickets may be skeletons.
- **Phase 3.** Adopt the ticket v2 template (Eleutheria's fields, Rhēma's header); write `_TEMPLATE.md`; every ticket names its `Live stage`.
- **Phase 5.** Machine ledger slimmed to state-only sections (+ GATE DECISIONS, RETURN PASS, OPEN FINDINGS, OPERATING MODE header); PHASE PLAN/invariants/out-of-scope/SETUP/CAPSTONE live in the manifest v2 and the ledger cites them; seed `DEFERRALS.md`, `docs/adr/README.md` + template, `docs/build/README.md` + empty `BUILD_INDEX.md`, `docs/README.md` mode map, `.agents/scratch/README.md`; persist the invocation as `docs/decomposition-prompt.md`; schedule `refresh-repo-docs` and `agent-docs` as the last two rows; when requirements > ~300 or tickets > ~30, emit capstone tickets (gap analysis / composed verification / closure) instead of a single CAPSTONE unit.
- **Spec amendment protocol** (new subsection): amend the source (spec_src or the spec file), bump the front-matter version/delta, keep IDs append-only, log before/after + date + approver in the manifest, record an ADR if the amendment changes a design decision, then update affected tickets' Load/AC lines.
- **Hand off.** Run `check-build-memory.sh`; print the manual floor exactly as Rhēma's manifest does.
- **README §7.** Replace "gitignored beside the canonical spec" with the committed rationale (ADR-058 argument) and drop the "regenerated from the spec" claim.

### 7.2 `orchestrate-build` + `drive-build.sh`

- **§0.** Read `manifest:` from the ledger; treat `returnPass` and gate pauses as distinct from `blockedOn`.
- **§2.1 gate protocol.** Before a ticket with a gate: pause (in `checkpoint`/`manual`), present the block, record answers verbatim in GATE DECISIONS (never secrets), pass "gate answers are in the ledger" to the worker; "skip" → run ungated → add to RETURN PASS.
- **§2.3.** Append the BUILD_INDEX row (committed, docs-only commit on the ticket branch or a trailing docs commit); copy new DEFERRALS ids into the PHASE LOG entry; carry non-blocking findings to OPEN FINDINGS.
- **§2.4.** Insert/split convention: `NNa_` filename, manifest row `NN.5`, `INSERT`/`SPLIT` log line; `mode=extend` for larger re-plans.
- **§3.** Capstone-as-tickets branch with the four named artifacts, the independence rule (verdicts before reading self-assessments), the "xfail reason starts with the deferral id" rule, and the ACCEPTED-list operator signature; then **invoke `reconcile-build`** before `DONE`; `DONE` additionally requires BUILD_INDEX complete and no OPEN deferral without a landing.
- **`drive-build.sh`.** Parse `returnPass`; exit 0 with a "gate pending" message instead of 2 when the only obstacle is a human gate in `checkpoint` mode; add `--lock-timeout`; log dir under `.agents/scratch/drive-build-logs/`.
- **Guardrails.** Add: "a gate is a pause, not a block"; "secrets never enter any ledger".

### 7.3 `implement-spec`

- **0.3.** Read `AGENTS.md` build-memory section, `docs/tickets/DEFERRALS.md` (close rows this ticket unblocks; that closure is in scope), and the ticket's Gate status / Live stage.
- **0.4.** Ledger path `.agents/scratch/ledgers/implement-spec_<TICKET-ID>.md` when the spec is a ticket file (fallback to the branch/date name otherwise); date recorded inside.
- **4.2 / 6.1a decision record.** MET-DIFFERENTLY verdicts, SHOULD-deviations, and decisions the ticket's Notes say it owns → ADR file + index row in the same PR.
- **5.3.** If the live stage is operator-gated and not authorized: record a DEFERRALS row (proxy, unblocked-by, how to verify), report "gate pending", never fail or fabricate.
- **6.1.** Stage `docs/adr/`, `docs/tickets/DEFERRALS.md`, `docs/build/BUILD_INDEX.md` changes when present (they are part of the change); keep `.agents/` excluded.
- **6.3.** Write the PR body to `.agents/scratch/pr/<TICKET-ID>_pr_body.md` first, then pass it to `gh`; stamp requirement IDs and deferral IDs.
- **6.4.** Evidence report also appends the BUILD_INDEX row when no orchestrator will.

### 7.4 `agent-docs`

- `guidelines.md`: add an optional **Build memory** section (generated when `docs/tickets/` exists) with the five bullets from §6.4; treat "read DEFERRALS.md first" as a Critical Gotcha.
- `bootstrap.md`/`refresh.md`: the detector flags a missing Build memory section as a coverage gap.

### 7.5 `refresh-repo-docs`

- Phase 0: read `docs/README.md` mode table if present; `generated` docs → fix the generator/source; `frozen`/`historical`/`append-only` docs → report-only (ADRs already handled this way).
- Phase 1: `docs/tickets/`, `docs/build/` are historical by default.

### 7.6 New skill: `research-to-spec`

Inputs: `brief` (file), `spec_out`, `rounds` (default 1), `operator_questions` (default true), `adversarial_review` (default true), `outline_trace` (default true when a brief exists).
Phases: (0) read the brief, extract fixed constraints (⚑) and theses; (1) write `research-ledger.md` with lettered streams, owners, status vocabulary and the Q register; (2) fan out read-only research/design subagents writing numbered notes under `research/CONVENTIONS.md` (verification status per finding, outline delta, emitted requirement IDs); (3) synthesize the spec with front matter, ID grammar, decision register, requirement index with AC hooks, research index, glossary/reserved strings; (4) fresh-context adversarial review (consistency, coverage, MVP realism) + outline superset trace where a brief exists; dispositions appendix; (5) operator questions → answers → ratification flips Status to canonical; (6) write `decomposition-prompt.md` and hand off to decompose-spec.
Rationale README: cite the two ledgers as the hand-rolled instances, and the Rhēma review-round structure (critique → crosswalk → round-2 ledger → delta review).

### 7.7 New skill: `reconcile-build`

Inputs: `ledger`, `manifest`, `apply_amendments` (default false: proposals only). Phases: (1) BUILD_INDEX completeness check; (2) one backlog: every deferred item, ADR revisit trigger, open finding, risk-register deferred row → exactly one `BL-` row with a landing (validator); (3) TICKET_VS_SPEC tagging (in-spec / spec-implied / ticket-added) with dispositions; (4) spec reconciliation plan (amendments with before/after and tick state; fold-back IDs; ADR index ↔ spec appendix) — applied only if ticked; (5) integration plan with a read-only merge dry-run (never merges); (6) readiness map (capability × {code, infra, human gate}, critical path with `ticket:`/`proof:`, no TBD); (7) release-notes draft from BUILD_INDEX; (8) a re-plan seed: `docs/build/planning/<date>_decision-memo.md` skeleton for the next `decompose-spec mode=extend`.

### 7.8 `~/agent-skills/README.md`

Replace the "multi-session build" section with the S0–S9 lifecycle table and the layout diagram; state the commit rule (§6.1 principle 1) as a design principle; list the validator.

---

## 8. Migration notes and open questions

**Retrofitting Eleutheria** (low cost): rename `docs/build/planning`-class files into `docs/build/planning/`; move the three skill-named ledgers and the root logs per the scratch layout; regenerate the ADR index in numeric order; add `DEFERRALS.md` as the forward-looking companion (BACKLOG stays the normalized post-build view); add the Build-memory AGENTS.md section as a bulleted "read first" (it is mostly there). Its `docs/README.md` mode table and validators port directly into the skills.

**Retrofitting Rhēma** (medium cost): write `docs/build/BUILD_INDEX.md` from PRs #1–#24 (one afternoon, since PR bodies are complete); either advance the machine ledger to reality or delete it until orchestrate-build is used; create `docs/adr/` and back-fill the three known decisions (GCP switch, protocols committed, SST→Pulumi) as ADRs from the manifest amendment paragraphs; move `docs/research/` and `docs/design/` unchanged (they already match the layout); rename `docs/2_research-and-design-ledger.md` → `docs/research-ledger.md` and `docs/6_ticket-decomposition-prompt.md` → `docs/decomposition-prompt.md` when convenient; prune `DEFERRALS.md` PARTIAL rows into DONE + new OPEN rows so the status enum stays clean.

**Open questions for you**

1. Default `tickets_dir=docs/tickets` and committed: agreed? (This reverses the 2026-09-01 decision; the counter-argument in that commit was repo-neutrality, which a docs-only commit on a build branch preserves.)
2. Should `research-to-spec` be one skill or two (`plan-research` producing the ledger and fanning out; `synthesize-spec` producing and reviewing the spec)? One skill keeps the ledger contract in one place; two match how Rhēma actually paused between them.
3. Threshold for capstone-as-tickets: requirement count, ticket count, or always? I'd default to "always when the chain has more than ~20 tickets".
4. Should BUILD_INDEX rows be committed on the ticket's own branch (docs-only commit after the code commit) or on a trailing `build-memory` branch? On-branch is simpler and is what Eleutheria's post-build tickets did.
5. Risk register and traceability appends were spec-mandated in Eleutheria. Make them optional template fields (on when the spec names them) rather than universal?
6. Naming: `docs/<build>-spec.md` unnumbered (Rhēma's final choice) vs. Eleutheria's numbered `1_… 2_…` pipeline docs. I recommend role names + the README map; numbering broke in both repos (Rhēma has a `3_` gap; Eleutheria's numbering stops at 2).

---

## Appendix — evidence pointers

- Skill rules quoted: `~/agent-skills/skills/decompose-spec/SKILL.md` (inputs `tickets_dir`, Phase 3, Phase 5); `orchestrate-build/SKILL.md` §0, §2.3, §3; `implement-spec/SKILL.md` 0.4, 6.1; commit `ba3f634` message.
- Eleutheria: `docs/adr/ADR-058-…md`; `.agents/scratch/planning/sig-postbuild-build-ledger.md` (GATE PROTOCOL, GATE DECISIONS, RETURN PASS, OPEN FINDINGS, `returnPass:`); `.agents/scratch/README.md`; `docs/tickets/00_MANIFEST.md` rows 47–65 and rules 1–4; `docs/tickets/_TEMPLATE.md`; `docs/tickets/P19.2`, `P20.1`, `P20.2`, `P22.2`; `docs/build/README.md`, `BUILD_INDEX.md`, `LEDGER_DEFERRALS.md`, `BACKLOG.csv`, `COVERAGE_MATRIX.csv`, `tools/*.py`; `docs/research/README.md`, `_meta/CONVENTIONS.md`, `_meta/spec_src/BUILD.sh`; `docs/2_canonical_design_spec.md` Part 0; `AGENTS.md`.
- Rhēma: `docs/tickets/00_MANIFEST.md`; `docs/tickets/DEFERRALS.md` (rules 1–4, status enum, 86 rows / 46 OPEN); `AGENTS.md` "Deferred obligations — READ FIRST"; `docs/2_research-and-design-ledger.md` (§0, §O, §Q, change log incl. the 2026-09-01 revision to Q-13); `docs/5_round2-ledger.md`; `docs/6_ticket-decomposition-prompt.md`; `docs/rhema-design-spec.md` front matter + §0.1–0.3; `.agents/scratch/rhema-build-ledger.md` (CURRENT STATE `NOT_STARTED`); per-ticket ledgers `implement-spec_stevevitali-p0-06…`, `implement-spec_p0-15g_20260908.md`; git log (P0.15b–h commit series).
