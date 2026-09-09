# Ledger — Long-horizon orchestration & shared-memory meta-analysis

Created: 2026-09-09 · Owner: Claude (Fable 5.1) for Steve Vitali
Deliverable: `~/MetaHarness/long-horizon-memory-structures.md` (opened in Zed)
Source prompt: see §0. Status legend: `[ ]` todo · `[~]` in progress · `[x]` done · `[-]` dropped (reason)

## 0. Prompt distilled

Goal: understand the relationship between `~/agent-skills` (decompose-spec / orchestrate-build /
implement-spec + docs skills) and the two concrete ways `~/Eleutheria` and `~/Rhēma` organize shared
markdown state for very-long-horizon, ticket-chained builds; map the projects' structures onto the
skills; find gaps rigorously; contrast the two projects; extract one elegant, strict, reusable
substructure and say exactly where it gets encoded in the skills. Rhēma ≈ gold standard; Eleutheria's
repo-facing docs (auto-ADRs, build docs) must be considered for incorporation.

## 1. Task decomposition

### A. Review `~/agent-skills` (what the skills assume about memory/layout)
- [x] A1 implement-spec: run-ledger contract (path, name, sections), what it reads (AGENTS.md, agent_docs), what it writes, staging exclusions
- [x] A2 decompose-spec: build-ledger contract (CURRENT STATE keys, sections), tickets dir + manifest, `tickets_dir` semantics (gitignored requirement), contract file format, spec-amendment log
- [x] A3 orchestrate-build + drive-build.sh: what is parsed from the ledger, dispatch tiers, SETUP/CAPSTONE, where evidence goes
- [x] A4 agent-docs / refresh-repo-docs / self-review: touchpoints with docs/ and memory
- [x] A5 READMEs (design rationale) for implement/decompose/orchestrate: stated assumptions about memory location + lifecycle
- [x] A6 Enumerate every on-disk artifact the skills *name* (table: artifact → producer → consumer → location → committed?)

### B. Review `~/Eleutheria`
- [x] B1 `.agents/scratch` inventory + README: ledgers/, fixtures/, logs/, planning/, pr/, tools/ — what each is, which are reusable patterns vs throwaway
- [x] B2 `docs/` top-level: 1_deep_research_overview, 2_canonical_design_spec (size), README, risk_register, traceability
- [x] B3 `docs/adr`: README, format, how generated (`tools/gen_adrs.py`), retro-fit ADRs (060), ADR-058 (tickets committed)
- [x] B4 `docs/build`: README, BUILD_INDEX, PLANNING_LEDGER, BACKLOG(+csv/themes), LEDGER_DEFERRALS, TICKET_VS_SPEC, SPEC_RECONCILIATION_PLAN, CAPSTONE_*, COMPOSED_E2E_REPORT, SUCCESSION, live_runs/, okc/, rights/, tools/ — classify each: reusable-agent-guidance / reusable-ledger-pattern / project-specific artifact / throwaway
- [x] B5 `docs/research` + `_meta` (CONVENTIONS, SPEC_OUTLINE, OUTLINE_TRACE, GAP_ANALYSIS, LEAD_SPOTCHECKS, spec_src/ + BUILD.sh): the research→spec pipeline
- [x] B6 `docs/tickets`: 00_MANIFEST, _TEMPLATE, a representative ticket, P19.1 (build memory & hygiene), P22.x docs-refresh tickets
- [x] B7 AGENTS.md / CLAUDE.md: how agents are pointed at the memory structures
- [x] B8 `.agents/scratch/planning/sig-postbuild-build-ledger.md` vs docs/build/PLANNING_LEDGER.md: which is the "build ledger" and how they relate
- [x] B9 git history: when tickets/ADRs/build docs were committed; whether any skill drove it

### C. Review `~/Rhēma`
- [x] C1 `.agents/scratch`: rhema-build-ledger.md (structure vs decompose-spec template), per-ticket implement-spec ledgers (naming drift: two conventions)
- [x] C2 `docs/` top-level: 1_founding-prompt, 2_research-and-design-ledger, 5_round2-ledger, 6_ticket-decomposition-prompt, rhema-design-spec (size, structure, REQ IDs), phase0-exit-readout; numbering gap (3_?)
- [x] C3 `docs/design/` + `docs/research/`: numbering, briefs, reviews, how the ledger indexes them
- [x] C4 `docs/tickets/`: 00_MANIFEST, HUMAN-H* and GATE-G* rows, DEFERRALS.md, sub-ticket naming (P0.15b..i, 16_ prefix reuse), spec amendments log
- [x] C5 AGENTS.md: how it points at spec/tickets/ledger
- [x] C6 gitignore posture: `.agents/` ignored; `docs/tickets` committed (contradicts decompose-spec's "must be gitignored")

### D. Mapping + gap analysis
- [x] D1 Artifact-class matrix: for each class found in B/C → which skill (if any) produces/consumes it; location in each project; committed?
- [x] D2 Gap list (rigorous): things the projects needed that no skill provides or that the skills contradict
- [x] D3 Eleutheria vs Rhēma differences (table), with a judgment on each difference
- [x] D4 Where projects deviated from the skills' letter and whether the deviation was right

### E. Synthesis
- [x] E1 Canonical layout proposal (repo-facing `docs/` + agent-facing `.agents/`) — strict, minimal, justified
- [x] E2 Lifecycle: research → design → spec → decomposition → build → capstone → post-build (backlog/deferrals/docs refresh) — which skill owns each stage; new skill(s) needed
- [x] E3 Per-skill change list (file, section, exact change)
- [x] E4 Reusable templates to encode (ledger, manifest, ticket, ADR, deferrals, build index, research ledger)
- [x] E5 Migration/adoption notes + open questions for Steve

### F. Deliverable
- [x] F1 Write `~/MetaHarness/long-horizon-memory-structures.md`
- [x] F2 Open in Zed; final recap

## 2. Progress log (append-only)
- 2026-09-09 ledger created. Surveyed dir trees. agent-skills SKILL.md for implement/decompose/orchestrate read in full.

## 3. Findings scratch (raw notes, promoted to the report as they firm up)

### A. agent-skills findings (2026-09-09)
- A1 implement-spec: run ledger at `$SCRATCH/implement-spec_<branch-with-slashes-dashed>_<YYYYMMDD>.md`; SCRATCH = `$AGENT_SCRATCH_DIR` or `<main-worktree>/.agents/scratch` (must be gitignored). Reads AGENTS.md/CLAUDE.md hierarchy + `agent_docs/`. Sections: spec path, base commit, branch, requirements+AC checklist, test matrix (1.4), gap table (4.1), evidence log (5.4). Staging rule excludes `.agents/` unless spec includes it. No mention of: ADRs, build docs, deferrals, backlog, PR-body file, fixtures dir, tools dir, logs dir. PR body is passed inline to `gh pr create`.
- A2 decompose-spec: build ledger at `$SCRATCH/<build_name>-build-ledger.md`; tickets at `$SCRATCH/<build_name>-tickets/` (default) or `tickets_dir` (MUST be gitignored — skill adds ignore line). Ticket file `T<nn>__<slug>.md` / `P<phase>.<k>__<slug>.md`; header: sequence, phase, forks-from/PR-base, depends-on, run line. Manifest `00_MANIFEST.md`: how-to-build, chain table (with HUMAN/GATE rows), gates, REQ-ID→ticket index, spec-amendments-applied log. Ledger sections: CURRENT STATE (13 keys), PHASE PLAN, CROSS-CUTTING INVARIANTS, OUT OF SCOPE, SETUP checklist, CAPSTONE checklist, PHASE LOG. Hard-stop if spec lacks derivable design (no design authoring). No ADR, no deferrals file, no post-capstone phases.
- A3 orchestrate-build/drive-build.sh: parses only projectStatus, nextTicket, pauseRequested, blockedOn, buildWorktree via `^key:` grep. Logs to `<ledger-dir>/drive-build-logs/`. Worker prompt tells fresh session to follow orchestrate-build then implement-spec. CAPSTONE = gap analysis + closure branch + composed E2E → DONE. Nothing after DONE (no backlog/deferrals/docs-refresh/release).
- A4 agent-docs owns AGENTS.md hierarchy + `agent_docs/`; refresh-repo-docs owns README/docs/CHANGELOG/ADRs (says: ADRs mark superseded, never delete). Neither creates ADRs. Neither knows about tickets/ledgers/build docs.
- A5 READMEs: state on disk in gitignored scratch; ledger "nests" with implement-spec ledger; §7 of decompose README credits "two production builds" (Eleutheria 46-ticket, Rhēma) for per-ticket files + manifest, and rejected a `docs/tickets` default as repo-assuming. Insists tickets are "derived build scaffolding, regenerated from the spec, never committed".
- A6 Artifact table → see report §2.
- Git: last commits 2026-09-01 (tickets_dir). Note commit message rationale for rejecting docs/tickets default — the projects then did exactly that AND committed them (Eleutheria ADR-058 titled "tickets and build memory are committed and scratch is unified"). This is the central contradiction to resolve.

### B/C first-hand findings (2026-09-09, before subagent reports)
- Eleutheria post-build ledger (`.agents/scratch/planning/sig-postbuild-build-ledger.md`, 195 lines) EXTENDS the decompose-spec template with: `## OPEN FINDINGS (carry to CAPSTONE; not per-ticket blocks)`, `## GATE PROTOCOL`, `## GATE DECISIONS (append-only table: date, ticket, gate id, item, answer, consequence)`, `## RETURN PASS (tickets re-run after operator acts)`, a `returnPass:` key, and the rule "human gates are NOT blocks". Secrets never in ledger (only `provided: yes/no`). Inserted ticket P20.4 recorded as PHASE PLAN row 54.5. → Gaps G-gate, G-open-findings, G-return-pass.
- Eleutheria ADR-058 (2026-09-08): tickets "are NOT regenerable — no decompose ledger on disk"; committed docs/tickets (65 files) + docs/build memory; unified scratch layout `ledgers/ pr/ tools/ fixtures/ logs/ planning/ + README.md`; one ledger per ticket `implement-spec_<PXX.Y>.md`; orchestrator machine ledger stays gitignored. Revisit trigger: raw run ledgers/PR bodies wanted in git.
- Eleutheria lifecycle after the 46-ticket chain: P19.1 memory+hygiene → P19.2 capstone gap analysis (COVERAGE_MATRIX.csv 668 ids, verdict enum, routing enum) → P19.3 composed verification → P19.4/P19.5 closure → P20.1 ONE backlog (BACKLOG.csv/md/THEMES + check_backlog.py; merges 4 overlapping backlogs: risk-register deferred tables, ADR revisit triggers, ledger deferrals, checklist→None) + OPERATIONAL_READINESS → P20.2 spec reconciliation (TICKET_VS_SPEC in-spec/spec-implied/ticket-added; spec_src amendments via BUILD.sh; fold-back new IDs; Appendix F/G; ADR-062; check_spec_src.py) → P20.3 integration/release plan → P21.x live wiring → P22.1 refresh-repo-docs → P22.2 agent-docs. i.e. CAPSTONE was decomposed into TICKETS (P19.2–P19.5) rather than run as orchestrate-build §3 — because it was too large for one context. → Gap G-capstone-as-tickets.
- Eleutheria ticket header fields: Sequence n of N, Phase, base_branch, Depends on, Run line (with per-ticket `live_verification`), Gate status block (operator ticks; answers may come via ledger GATE DECISIONS), Goal, Load (read these — do not re-read others), In scope — deliverables (numbered), Out of scope, Acceptance criteria (each tagged deterministic/agentic), Requirement IDs to stamp, Cross-cutting invariants, Notes (ownership of enums/id spaces). Every ticket appends to docs/risk_register.md (RISK-Pnn-mm) and docs/traceability.md and often writes an ADR.
- Eleutheria DECISION_MEMO.md = the "spec" for the post-build decomposition (planning-only session output) — i.e. a second decompose-spec run took a *memo*, not the canonical spec, as input. Pattern: post-build re-planning = new mini-spec → decompose → append tickets to the same manifest (rows 47–63).
- Rhēma build ledger (104 lines) = decompose-spec template nearly verbatim; CURRENT STATE still NOT_STARTED / dispatchTarget manual, while 22 per-ticket ledgers exist → orchestrate-build was NOT used to drive; hand-chained. Manifest (401 lines) is the real driver. Spec 4071 lines (REQ-<AREA>-n IDs). Later-phase tickets are SKELETONS ("do not run implement-spec until body written after G3") → progressive elaboration pattern (gap: decompose-spec assumes all contracts authored up front).
- Rhēma sub-tickets P0.15b..i (16_ prefix reused) = run-time SPLIT/INSERT events done by hand; two per-ticket ledger naming styles (branch-derived vs ticket-id).
- Rhēma repo has no ADRs, no risk register, no traceability doc; deferrals live in docs/tickets/DEFERRALS.md (D-<ticket>-n ids seen in phase0-exit-readout).

- 2026-09-09 Eleutheria build-docs inventory agent done (B1/B3/B4/B8/B9). Research-pipeline and Rhēma agents died on network error near completion; resumed via SendMessage.

### B4/B3/B1 findings from inventory agent (2026-09-09)
- docs/build classification: reusable-ledger-pattern = README (index of build memory), BUILD_INDEX (per-ticket row: branch/PR/ledger/ADRs/live-verification status+evidence source), PLANNING_LEDGER (status glyphs ☐◐☑✗, fresh-context prompts, resume protocol table), LEDGER_DEFERRALS (id families LD-V/F/D/H/P/X + LH; append-only closure log), DECISION_MEMO (post-build mini-spec w/ GATE: lines), COVERAGE_MATRIX.csv schema, CAPSTONE_GAP_ANALYSIS, COMPOSED_E2E_REPORT (xfail-reason-starts-with-LD-id rule), CAPSTONE_CLOSURE (ACCEPTED list + operator signature), BACKLOG.csv/md/THEMES, OPERATIONAL_READINESS, TICKET_VS_SPEC, SPEC_RECONCILIATION_PLAN, INTEGRATION_PLAN; tools check_backlog.py / check_coverage_matrix.py / merge_dryrun.sh / retro_cli_matrix.sh. Project-specific: rights/, okc/, live_runs/, CI_STATUS, RELEASE_NOTES, runbooks. Reusable idioms: "every number carries the command that produced it"; "empty gated dir with README"; docs/README.md mode table (generated/frozen/append-only/historical/living).
- ADR format: `ADR-NNN-<slug>.md`; H1 `# ADR-NNN: title`; field block Status/Date/Phase/Requirement ids/Spec; sections Context/Decision/Consequences/Alternatives considered/Revisit trigger (revisit trigger machine-enforced by test; ADR set == spec Appendix F enforced by check_spec_src.py). Decisions immutable → new ADR. ADR-001..020 generated in one go by gen_adrs.py at P00.2 (content inline; no external input — "auto" only in the sense of scripted); ADR-021+ hand-written one per ticket in the ticket's PR. Drift: field names diverge in P21 era; index not numeric order; ADR-064 gap; dates bogus in some.
- scratch: README defines layout ledgers/ pr/ tools/ fixtures/ logs/ planning/; house naming `implement-spec_<PXX.Y>.md` REPLACED skill naming; post-P19.1 workers STILL drifted (5 skill-named ledgers, duplicate P20.3 ledgers, bogus dates 20260101/20260820, 4.5 MB logs at root). Per-ticket ledgers unreliable (15 stubs); PR bodies + orchestrator PHASE LOG became the real evidence record. → Lesson: naming conventions in prose don't hold; need a deterministic path rule tied to ticket id + a validator; and evidence should be promoted to a committed artifact, not left in the run ledger.
- git: ADRs committed in the same PR as the ticket from P00.2; docs/build only exists from P19.1 (2026-09-08); docs/tickets committed at P19.1.
- machine ledger CURRENT STATE extras: returnPass, mergePolicy; canonicalSpec was re-pointed to DECISION_MEMO.md; buildWorktree = main checkout (deliberate override); dispatchTarget subagent (Devin).

- 2026-09-09 First-hand reads complete for both projects (manifests, AGENTS.md, research ledgers, spec front matter, DEFERRALS, build ledgers, per-ticket ledger skeletons, git logs). Starting D/E synthesis + F1 draft while resumed agents finish.

### C findings first-hand (2026-09-09)
- Rhēma upstream pipeline: 1_founding-prompt → 2_research-and-design-ledger (v2; sections 0 theses, A ground truth, B research, C–N design streams, O spec synthesis, Q operator-decisions register, change log; status vocab open/in-progress/done/blocked-on-operator/dropped; owner vocab R/D/A/P/S; every done row carries evidence path) → docs/research/1..13 + docs/design/1..9 → spec v0.2 → reviews (design/11–13) → 4_external critique → design/14 crosswalk → 5_round2-ledger (A–F streams, Q-21+) → design/15–19 → spec v0.3 → v1.0 ratified (2026-09-01) → 6_ticket-decomposition-prompt → decompose-spec (tickets_dir=docs/tickets) → docs/tickets. Spec: REQ-<FAMILY>-n (22 families, 368 design IDs preserved + REQ-SPEC/OBJ/EVAL/POS/BETA), Appendix A requirement index, B decision register (Q-* + R-* rulings), C research index, D glossary/reserved strings, E review dispositions, F external-critique dispositions. "Five contracts adopted by reference" to design docs.
- Rhēma tickets: global sequence prefix NN_ + ticket id + slug; HUMAN-H* and GATE-G* marker docs; P1 skeletons; DEFERRALS.md hand-maintained companion (rows D-<ticket>-n; OPEN/DONE/WONTFIX/ACCEPTED-SKELETON/PARTIAL; 86 rows, 46 OPEN); AGENTS.md mandates reading DEFERRALS at start of every run and closing rows the ticket unblocks; gates must not pass with OPEN rows in phase. Manifest carries: derived-artifact banner, committed banner, how-to-build, HUMAN rows, chain table, milestone gates (thresholds verbatim), phase gates/ownership notes, cross-cutting invariants, out of scope, REQ→ticket index, spec amendments applied (2 entries), decomposition decisions (8 incl. Phase-4 adversarial review record).
- Rhēma build ledger: decompose template verbatim; PHASE PLAN/INVARIANTS/OUT-OF-SCOPE point at manifest ("verbatim list: manifest §…"); CURRENT STATE never advanced (NOT_STARTED, dispatchTarget manual) though 22 tickets ran → orchestrate-build never used; PHASE LOG has 3 seed entries only. The human runbook + git + DEFERRALS were the memory. No PHASE LOG of ticket completions exists anywhere except git and PRs (! gap: no BUILD_INDEX equivalent).
- Rhēma per-ticket ledgers: follow implement-spec sections well (Requirements, ACs, Req IDs, Invariants, "Deferrals read", Plan, Test matrix, Progress, Gap table, Evidence log) — richer than Eleutheria's; naming drift (branch-derived vs ticket-id vs no user prefix). P0.15 series: each follow-up ticket "synthesizes previous ticket's blockers into one runnable ticket" (commit msgs) → run-time re-planning by hand; Stage A (offline, PR-able) / Stage B (live, operator-gated, paid) split inside a ticket — a pattern the skills lack (live verification requiring operator authorization + spend).
- Rhēma git: docs committed 2026-09-01 (revision to Q-13 "intermediates gitignored"); DEFERRALS added same day, moved into docs/tickets; one commit per ticket for P0.01–P0.15; then many commits per sub-ticket (Stage A/B). ~87 commits.
- Eleutheria upstream: 1_deep_research_overview (outline, 2026-08-20) → docs/research/R1..R13 (CONVENTIONS: header, F<n>.<m> findings with Status VERIFIED/…, Outline delta, "Spec requirements emitted" REQ-R<n>-<mm>) → _meta/SPEC_OUTLINE (doc architecture) → _meta/OUTLINE_TRACE (480 OL-* obligations) → spec_src/*.md + BUILD.sh → 2_canonical_design_spec (9055 lines; SIG-<AREA>-nnn ids append-only, reserved ids, RFC2119; Appendix A traceability to OL-*, F ADR index, G corrections) → _meta/GAP_ANALYSIS (adversarial superset check: 395/58/27) → LEAD_SPOTCHECKS. Spec is a BUILD ARTIFACT; edits go to spec_src + ADR (SIG-ENG-003); check_spec_src.py enforces byte-identity + ADR set == Appendix F.
- Eleutheria AGENTS.md "Where things live" section: docs/tickets (committed contract record), docs/build (build memory), .agents/scratch (gitignored layout), docs/adr. Rules: append-only build memory; stacked PR; no merges.

- 2026-09-09 Report written (~8k words) at ~/MetaHarness/long-horizon-memory-structures.md; opened in Zed. Eleutheria research-pipeline agent report folded in (SIG-ENG-003/004/005/037, annotate-not-rewrite). Awaiting Rhēma agent report for final corrections.

### G. Follow-on (2026-09-09, second request)
- [x] G1 Operator amendments: commit everything under docs/build (logs excepted); capstone-as-tickets always when N>1.
- [x] G2 Research agent-skills conventions (frontmatter, hub+modes, script headers, plugin manifest, README authoring rules); Rhēma PRs/branches; Eleutheria PRs/scratch sizes.
- [x] G3 Write ~/MetaHarness/build-memory-v2-spec.md: Part I design (D1–D14, contracts BM-*), Part II tickets SK.1 / EL.1 / RH.1 with run lines; opened in Zed.
- [x] G4 Fold in the Rhēma inventory report (companions line, missing chain rows e–i, G1 marker path, DEFERRALS = 60 rows, HUMAN markers as living state).
- [x] G5 Amendment note prepended to the analysis doc.
- 2026-09-09 Spec written (~9.7k words). Next: operator review of Appendix B decisions; then `implement-spec` on SK.1 → EL.1 → RH.1.
