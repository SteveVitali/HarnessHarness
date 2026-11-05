# Run ledger — DOC5.1

- Spec: `docs/tickets/245_DOC5.1__doc1.md` (seq 245 of 246 · Phase Round 5 · Kind docs · Role DOC.1)
- Base: `42e169d181275d2cd9b43bfedd63fe4cd9575c16` (codex/harnessharness-evidence-rec5.3 actual tip — REC5.3 change `d76ca20` + close `42e169d1`; O-02 actual SHA at SETUP)
- Branch: `codex/harnessharness-evidence-doc5.1`
- Config: `implement-spec spec=docs/tickets/245_DOC5.1__doc1.md` (autonomous, ledger on; docs-only — offline, USD $0, no production mutations)
- Harness: devin-desktop/swe-2-high/subagent
- Skills: agent-skills @ e981ec5
- Started: 2026-11-05T01:10:42Z (`date -u` at SETUP)
- Closed: (pending)
- Context at SETUP: ticket estimate 20k + 10k reserve; loaded = contract 245 + invariants I-01–I-08/O-01–O-08 + supplement s1/s7/s8 + coverage register rows for the doc-surface ids (no primary targets — contributing slice) + manifest row 245/246 + 3 OPEN DEFERRALS rows + LEDGER CURRENT STATE + BUILD_INDEX tail + GATE-G17 readout + CAPSTONE_CLOSURE fold + the nine disposition packets + the ten living reader surfaces + check-position/examples/trace/contributing/quickstart scripts ≈ at estimate; ≤550 changed source/test lines + 24k/10k split trigger watched (docs-only scope).

## Deferrals read

`bash scripts/deferrals-open.sh` at 2026-11-05T01:09Z —
`DF-R5.1-1 | OPEN` (GATE-G17 contributor evidence, ten R5.1-owned targets),
`DF-H9-1 | OPEN` (withheld participant/outreach predicate set; minted CAP5.1),
`DF-CAP5.1-1 | OPEN` (failed S1 useful residual; minted CAP5.1).
GATE-G17's verbatim scope disposition (2026-11-05T00:13:07Z signature):
all three **carry forward OPEN** — no waiver named. This docs refresh
records them as open; it closes none and invents none.

## Orient findings

- Guard pre-dispatch: `bash docs/build/tools/check-supplement-guard.sh`
  → `hh-supplement-guard/1` v1.0.0 **PASS — route DISPATCH — DOC5.1**
  (82 active-round rows / 180 candidates / 0 violations / 8 baseline
  warnings); input digests recorded in the run.
- Accepted record (GATE-G17 PASSED 2026-11-05T00:13:07Z): engineering
  `executed` · research `inconclusive` · portability/public/adoption
  `withheld` · `build_status: not_done` (eng closed 107 / req satisfied
  78; MET 78 / MET-ENGINEERED 29 / PARTIAL 73 / MISSING 0).
- Landed packets verified byte-wise by reading: `hh-s1-useful/1`
  disposition `failed` (carried probe_verb_enoent +
  exec_backend_hello_clobber); `hh-ops-live/1` (second) `executed`,
  8 cells × 3 = 24 attempts, 0 violations, discharged both predicates;
  `hh-activation/1` + `hh-pilot/1` `executed`; `hh-parity-n0/1`
  `withheld` (N=0, s3_unmet); `hh-confirm-withheld/1` `withheld`
  (tranche absent, s4_unmet); `hh-study-report/1` `reported` —
  `unmet{efficacy_unavailable;parity_withheld}`, s1/s3/s4 unmet
  verbatim; `hh-plugin-unselected/1` + `hh-interchange-unselected/1`
  `unselected`; `hh-outside-withheld/1` + `hh-formative-withheld/1` +
  `hh-publication-withheld/1` + `hh-value-withheld/1` `withheld`.
- Stale reader surfaces found (claim-level audit): README spine says
  "two versioned example cards" (four exist); getting-started says
  "Two committed example-card/1 records" (four exist); evidence.md
  useful-task card says S1 never executed and names the R5.43 skeleton
  (it ran and failed); study card says "planned; collection not ready"
  (the study report landed `reported` with withheld legs); operations
  card says the 24-trial execution is owed to R5.44 (it executed);
  component/interchange/evidence-access cards predate the recorded
  unselected/withheld dispositions; the scenarios table and the
  partial-release note predate GATE-G17.
- `check-position.sh` pins the closed card states — useful-task /
  study / operations stay `Not yet available` (S1/S3/S4 remain unmet
  at GATE-G17); component / interchange / evidence-access stay
  `Restricted`. The refresh updates Scope/Evidence/Limits/Access text,
  never promotes a withheld state.

## Requirements

Refresh reader-facing docs (README, docs/, guides) to match the
accepted round-5 evidence: executed things documented as executed,
withheld/unmet things as withheld/unmet, no claims beyond the records;
preserve failure/negative/unmet findings and the ordinary contributor
path; offline-only. New artifact `docs/build/reports/round5/
READER_DOCS_CHECK.md`; regression/evidence + per-target mapping under
this run ledger and its PR record.

## Acceptance criteria

- AC-1 (D/E; positive): refresh reader guides/navigation with actual
  supported commands and ready evidence/limits; execute relevant
  offline semantic checks, validate links and source/code freshness;
  preserve failure/negative/unmet findings + ordinary contributor path.
- AC-2 (D/E; negative): no unexecuted quickstart claimed verified, no
  unknown access presented public, no implementation detail imposed;
  committed-HEAD check explicitly scoped.
- AC-3 (D/E; ownership+evidence): primary targets — none (contributing
  slice). Record achieved D/E separately from owed R/P; no full-target
  predicate claimed satisfied.
- AC-4 (E/D; scoped verification): fails-without for new behavior;
  docs-only uses living-docs/row/link checks; exact commands + counts;
  missing prerequisite is not a pass; scratch stores.
- AC-5 (D/E; phase gate): green in scope; requirement IDs stamped
  (none); owed items stay DEFERRALS rows; run/PR/BUILD_INDEX/LEDGER
  agree; ≤2KiB phase log; no deferred predicate becomes MET by close.

## Plan

1. README spine: four example cards (EX-01–EX-04, all labelled
   fixtures); useful-task bullet names the recorded failed bounded
   attempt.
2. getting-started: two → four committed example cards.
3. evidence.md: rewrite the six stale cards' evidence text to the
   landed packets; scenarios table to the accepted dispositions; unmet
   clause + new dated round-5 acceptance record (GATE-G17 + fold).
4. status.md: dated round-5 acceptance note under evidence reading.
5. experiments.md EX-03: pin the *failed* packet verbatim; 
   CONTRIBUTING.md: component route names the recorded unselected
   disposition.
6. `reports/round5/READER_DOCS_CHECK.md` — the contract artifact:
   claim-by-claim audit table, executed checks, committed-HEAD scope.
7. Verify: living-docs, check-position, check-examples (+`--head` post-
   commit), check-trace (+`--head`), check-contributing (+`--head`),
   check-quickstart (committed HEAD), bm-tools, guard post-artifacts,
   fmt; documented example commands `cargo test -p hharness --test
   r5_29 ex01/ex02` and `--test r5_45 ex03/ex04` on scratch stores.
8. Commit, PR (base rec5.3), ci-boundary → `logs/ci-doc5.1.json`,
   closeout (BUILD_INDEX 245 / LEDGER DOC5.1→DOC5.2 / phase log).

## Test matrix

- fails-without: the checkers themselves — check-position fails if a
  card state leaves the closed vocabulary or the unmet clause drops
  S1/S3; check-examples/check-trace fail on card/link drift;
  check-living-docs fails on personal paths; check-contributing fails
  on lane drift. A deleted mark block or broken link flips its check.
- Documented-command freshness: `cargo test -p hharness --test r5_29
  ex01` / `ex02` and `--test r5_45 ex03` / `ex04` executed as the docs
  instruct; `check-quickstart.sh` exercises the verbatim README block
  at committed HEAD (scope recorded).
- No test pins living-record values (counts/dates/statuses) —
  BM-TEST-01.

## Progress

- 2026-11-05T01:10Z SETUP — deferrals read (3 OPEN, carried verbatim);
  branch `codex/harnessharness-evidence-doc5.1` @ `42e169d1`; guard
  PASS DISPATCH DOC5.1 (82/180/0 viol/8 warn); claim-level audit done
  (staleness list above); every planned claim verified against the
  committed packets before writing.

## Gap table

| item (spec §) | required | achieved | status | evidence |
|---|---|---|---|---|
| AC-1 refresh guides/navigation w/ actual commands + ready evidence/limits; offline semantic checks; links + source/code freshness; preserve failure/negative/unmet + contributor path | D/E | — | pending | (recorded at close) |
| AC-2 no unexecuted quickstart/public-access/impl-detail claim; committed-HEAD check scoped | D/E | — | pending | |
| AC-3 primary targets → checks/evidence; achieved D/E vs owed R/P | D/E | engineered | met | none — contributing slice; the slice's own D/E recorded here; no full-target predicate claimed |
| AC-4 fails-without; docs-only living-docs/row/link checks; commands + counts | E/D | — | pending | |
| AC-5 universal phase gate | D/E | — | pending | |

## Invariant / operating-clause gap rows

- **I-01** held — frozen sources untouched (spec, research, packets,
  readouts); living surfaces only; no schema/generated edits.
- **I-02** held — docs-only; no kernel semantics touched.
- **I-03..I-05** n-a — no subject/grade/treatment/method surface.
- **I-06** held — packet dispositions quoted verbatim (`failed`,
  `executed`, `withheld`, `unselected`, `reported`); replay/R0 limits
  retained.
- **I-07** held — D/E recorded separately from owed R/P; withheld
  stages stay withheld; no target claimed satisfied.
- **I-08** held — no optional exercise awards a claim; $0; no
  inherited permissions; merge authority stays OPERATOR.
- **O-01** held — deferrals first; bounded reads; `date -u` stamps.
- **O-02** held — single writer; forked actual rec5.3 tip `42e169d1`;
  shell/python writes + git-diff verification.
- **O-03** held — no gate inferred; GATE-G17's verbatim dispositions
  carried.
- **O-04** held — guard PASS pre-dispatch and re-run post-artifacts.
- **O-05** held — docs-only battery + documented-command execution;
  counts recorded.
- **O-06** held — current-head + ancestor CI read at close; pending/
  unknown never called green.
- **O-07** held — stock validator + row tools + retained guard run
  separately; historical records append-only.
- **O-08** held — no production mutation; credentials never recorded.

## Evidence log

- `bash scripts/deferrals-open.sh` → 3 OPEN (DF-R5.1-1, DF-H9-1, DF-CAP5.1-1).
- `bash docs/build/tools/check-supplement-guard.sh` → PASS, route DISPATCH — DOC5.1 (82 rows / 180 cand / 0 viol / 8 warn).
- Packet reads (python json.load): the nine r5.4x/r5.5x packets above, dispositions verbatim; pilot `declared_first:10` (2 arms × 5), `exposure_micros:0`; ops-live second 24 attempts, 0 violations.
- `bash scripts/check-living-docs.sh` → OK — 456 living files, no personal paths.
- `bash scripts/check-position.sh` → OK — 13 legs green (13 cards; closed states held: useful-task/study/operations `Not yet available`, component/interchange/evidence-access `Restricted`; S1/S3/S4 unmet preserved; honesty clean; links resolve).
- `bash scripts/check-examples.sh` → OK — 4 cards, 6 pins recomputed, 29 runner check ids, displayed block equivalent.
- `bash scripts/check-trace.sh` → OK — excerpts/packets/families/honesty/links/nav green.
- `bash scripts/check-contributing.sh` → OK — 12 check ids green (lanes, gated routes, no_ritual, links).
- `bash scripts/check-bm-tools.sh` → OK.
- `cargo fmt --all --check` → clean (no Rust touched — docs-only; run per AC-4 anyway).
- Documented commands on scratch stores (`HH_STORE_ROOT`/`HH_WORKSPACE_ROOT` under `mktemp -d`): `cargo test -p hharness --test r5_29 ex01` 1/1 · `ex02` 5/5 · `--test r5_45 ex03` 3/3 · `ex04` 2/2 — every displayed example command executes as documented.
- `git diff` self-review: six living files; all claims trace to committed packets/readout; no frozen/historical file touched; no banned phrases (check-position honesty leg).

## Evidence report

(pending — written at Phase 6)
