# Run ledger — REC5.2

- Spec: `docs/tickets/243_REC5.2__rec2.md`
- Base: `169e3c2538ef0f19d994410b0f29fae5420943c7` (codex/harnessharness-evidence-rec5.1 actual tip — REC5.1 change `4853e792` + close `169e3c25`; O-02 actual SHA recorded at SETUP)
- Branch: `codex/harnessharness-evidence-rec5.2`
- Config: `implement-spec spec=docs/tickets/243_REC5.2__rec2.md` (autonomous, ledger on, records-in reconcile; reconcile-build mode=spec, proposals-only `apply_amendments=false`)
- Harness: devin-desktop/swe-2-high/subagent
- Skills: agent-skills @ e981ec5
- Started: 2026-11-05T00:32:49Z (`date -u` at SETUP)
- Closed: 2026-11-05T00:5xZ (`date -u` at §6.5 close)
- Context at SETUP: ticket estimate 24k + 10k reserve; loaded = contract 243 + invariants I-01–I-08/O-01–O-08 + supplement §9 (bounded) + coverage register REC5.2/REQ-E-210/REQ-H-004 rows + OL-14/OL-20 mappings + manifest rows 168–246 + operating/notes rows + 3 OPEN DEFERRALS rows + LEDGER CURRENT STATE + BUILD_INDEX round-5 rows + per-ticket pr/<id>.md requirement-ids + landed file inventory (git diff base→HEAD) + cap5 packets/verdicts + reconcile-build mode=spec + REC.2 precedent artifacts ≈ at estimate; ≤550 changed source/test lines split trigger watched (docs-only scope).

## Deferrals read

`bash scripts/deferrals-open.sh` at 2026-11-05T00:32Z —
`DF-R5.1-1 | OPEN` (GATE-G17 contributor evidence, ten R5.1-owned targets),
`DF-H9-1 | OPEN` (withheld participant/outreach predicate set; minted CAP5.1),
`DF-CAP5.1-1 | OPEN` (failed S1 useful residual; minted CAP5.1).
GATE-G17's verbatim scope disposition (2026-11-05T00:13:07Z signature,
readouts/GATE-G17.md): all three **carry forward OPEN** — never inferred
completion. None discharges at this ticket; withheld stays withheld.

## Orient findings

- Guard pre-dispatch: `hh-supplement-guard/1` v1.0.0 PASS — route
  DISPATCH REC5.2 (82 active-round rows / 180 candidates / 0 viol /
  8 baseline-class warnings), input digests recorded.
- Baselines: check-build-memory `.` 0 viol / 8 scoped warnings (the
  §9.5 baseline set, incl. BM-ADR-04 appendix mismatch — dispositioned
  legacy, carried as plan row, not rewritten); check-bm-tools OK;
  check-living-docs 456 OK; gen-revisit-triggers --check in sync;
  check-drift regenerates byte-identical.
- Landed-scope inventory (git diff `001490c1` r4-boundary → HEAD):
  63 landed BUILD_INDEX rows seq 169–242; ~234 crate files
  (+84,581/−390); **zero** changes under `schema/`, `clients/`,
  `crates/hh-codegen`, `hh-embed-schema`, `hh-embed-client-generated`,
  `THIRD_PARTY_NOTICES.md`; only manifest change = `hh-env`
  `[dev-dependencies]` ×7 internal path crates + Cargo.lock +7
  (test-only, reverse-edge-free — no dep in that set lists hh-env);
  39 new ADRs (ADR-0382–0420); 20 committed evidence packets.
- Coverage anchors: the round-5 COVERAGE_MATRIX.csv names
  `docs/build/pr/REC5.2.md` + `docs/build/runs/REC5.2.md` as
  REQ-E-210's evidence — the only two missing pointers at SETUP;
  this ticket lands both (anchor closes at PR time, recorded honestly).
- REC.2 precedent: `docs/build/TICKET_VS_SPEC.md` +
  `SPEC_RECONCILIATION_PLAN.md` remain the round-1–4 record
  (append-only, untouched — new round artifacts live under
  `docs/build/reports/round5/` per manifest note).

## Requirements

Reconcile landed code, ADRs and supplemental coverage, records-in/
offline-only (USD $0): invoke reconcile-build mode=spec — tag every
landed round-5 deliverable in-spec/spec-implied/ticket-added with a
disposition in `docs/build/reports/round5/TICKET_VS_SPEC.md`; write
`docs/build/reports/round5/SPEC_RECONCILIATION_PLAN.md` (proposed
amendments w/ target file, anchor, before/after, tick state — all `[ ]`);
verify every changed contract/generator/tier/dependency admission and
supplemental layer link against committed records; REQ-E-210's exact
predicate resolved to numbered checks + actual evidence; achieved D/E
recorded separately from owed R/P or actual-execution E; never close
an absent research/participant predicate.

## Acceptance criteria

- AC-1 (D/E; positive): read actual landed code/ADRs; verify every
  changed contract/generator/tier/dependency admission + supplemental
  layer link; append authorized reconciliation without rewriting
  original contracts; generated index/triggers updated through tools.
- AC-2 (D/E; negative): missing amendment, reverse edge, hand-edited
  generated artifact, unadmitted dependency or silent deviation is
  owed work; documentation cannot retroactively authorize semantics.
- AC-3 (D/E; ownership+evidence): REQ-E-210's exact original
  predicate/aliases resolved to numbered checks + actual evidence in
  the coverage register; achieved D/E separate from owed R/P /
  actual-execution E; contributor evidence R5.3/R5.4/R5.5 recorded;
  full-target predicate not claimed by this close.
- AC-4 (E/D; scoped verification): fails-without for new behavior;
  docs-only = living-docs/row/link checks; exact commands + counts.
- AC-5 (D/E; phase gate): green in scope; REQ-E-210 stamped
  (contributing REQ-H-004); owed items are DEFERRALS rows w/
  owner/trigger/control; records agree at true completion; ≤2KiB
  phase log; no deferred predicate becomes MET by this close.

## Plan

1. Reconcile-verify (AC-1/AC-2 checks): dependency-admission audit
   (Cargo.toml/lock diff), reverse-edge audit, schema/generator/
   generated audit + check-drift, tier/feature audit, amendment-admission
   map (§9.3 7 rows → PADR → GATE-G9 → landing ADRs), ADR index/trigger
   regen checks, class-table one-home check (CC10), packet inventory,
   coverage-anchor resolution, deviation↔manifest-note consistency.
2. `reports/round5/TICKET_VS_SPEC.md`: 63 landed rows tagged
   in-spec/spec-implied/ticket-added + dispositions + verification
   table + REQ-E-210 resolution (AC-3).
3. `reports/round5/SPEC_RECONCILIATION_PLAN.md`: proposed amendments —
   all `[ ]`; ADR-set vs appendix delta dispositioned (frozen mismatch
   preserved as legacy, never rewritten).
4. `pr/REC5.2.md` + run-ledger gap table/evidence; verify; closeout.

## Test matrix

- fails-without: the reconcile is records-in; the regression surface
  is the checker suite — check-build-memory fails if protected regions
  mutate wrong or generated indexes drift; supplement guard fails if
  coverage/routing breaks; living-docs fails on bad links in the two
  new artifacts. REQ-E-210's numbered checks are the audit cells —
  each fails-without if its predicate broke (e.g., a new external dep
  would fail V-1; a hand-edited generated file fails check-drift).
- Deterministic checks: check-living-docs.sh · check-bm-tools.sh ·
  check-build-memory `.`+`--staged` · supplement guard pre+post ·
  check-drift · gen-revisit-triggers --check · adr-index --check diff ·
  ci-boundary at close.

## Progress

- 2026-11-05T00:32Z SETUP — deferrals read (3 OPEN, all carried);
  branch `codex/harnessharness-evidence-rec5.2` @ `169e3c25`; orient
  findings + baselines recorded; guard PASS DISPATCH REC5.2.
- 2026-11-05T00:3xZ RECONCILE-VERIFY — audits run: dependency diff =
  hh-env dev-deps ×7 only (test-only, acyclic — no crate in the set
  lists hh-env); zero schema/codegen/generated diff; check-drift
  byte-identical; no feature/tier change; 39 ADRs all cite admitted
  packets or direct sections; classes.rs single home (+6 classes);
  20 packets parse with honest dispositions; 47 manifest note rows
  cover every recorded deviation; DF-R5.17-1 DONE via owning
  admission (ADR-0418 D1); adr-index --check == committed index.

## Gap table

| item (spec §) | required | achieved | status | evidence |
|---|---|---|---|---|
| AC-1 read landed code/ADRs; verify every contract/generator/tier/dependency admission + supplemental layer link; append authorized reconciliation | D/E | engineered | met | TICKET_VS_SPEC.md V-1…V-15 — diff-audited at `169e3c25`; artifacts appended (round-1–4 records untouched); adr-index --check identical; gen-revisit-triggers in sync |
| AC-2 missing amendment / reverse edge / hand-edited generated / unadmitted dep / silent deviation = owed work | D/E | engineered | met | none found: V-1 (deps clean), V-2 (no reverse edge), V-3/V-4 (generated zero-diff + drift byte-identical), V-5 (tiers clean), V-6 (amendment map complete), V-12 (47 manifest notes cover all deviations); R5A-1 owed spec text is a proposed unticked row — nothing retro-authorized |
| AC-3 REQ-E-210 predicate → numbered checks + evidence; achieved D/E vs owed R/P / actual-execution E; contributors R5.3/R5.4/R5.5 | D/E | engineered | met | TICKET_VS_SPEC.md REQ-E-210 table E210-1…7 (all met at engineered); required domain D/E only — no R/P leg exists; no actual-execution E claimed; contributor evidence = landed dossier + checker (runs/R5.3–5.md, ADR-0384); full-target NOT claimed by this close |
| AC-4 fails-without + commands/counts | E/D | engineered | met | docs-only battery (below) — all exit 0; fails-without = audit cells themselves fail if the predicate breaks (e.g., a new ext dep flips V-1; a hand-edit flips check-drift); no Rust changed — cargo suites n/a |
| AC-5 phase gate; REQ stamps; owed→DEFERRALS; records agree; ≤2KiB log | D/E | engineered | met | REQ-E-210 + REQ-H-004 stamped in PR; 3 OPEN DFs carried (none owed anew — every found item already homed); BUILD_INDEX/LEDGER advance at closeout; phase log fixed-shape |
| Deliverable: `reports/round5/TICKET_VS_SPEC.md` — every landed row tagged + dispositioned | D/E | engineered | met | 63 rows tagged; counts + REQ-E-210 resolution + invariant/operating dispositions |
| Deliverable: `reports/round5/SPEC_RECONCILIATION_PLAN.md` — proposed amendments w/ target/anchor/before-after/tick | D/E | engineered | met | 1 proposed (R5A-1, `[ ]`), 0 applied; 6 dispositioned rows; ADR-set-vs-appendix delta dispositioned |
| I-01…I-08 | — | engineered | met | per-invariant disposition table in TICKET_VS_SPEC.md — all applicable rows held/verified |
| O-01…O-08 | — | engineered | met | per-clause disposition table; guard PASS pre+post; `date -u` stamps; shell/python writes + git-diff verify; reads are records-in |

## Evidence log

- `bash scripts/deferrals-open.sh` → 3 OPEN (DF-R5.1-1, DF-H9-1, DF-CAP5.1-1 — carried per GATE-G17 verbatim).
- `bash docs/build/tools/check-supplement-guard.sh` → hh-supplement-guard/1 v1.0.0 PASS, route DISPATCH — REC5.2 (82 rows / 180 cand / 0 viol / 8 warnings) @ pre-artifacts.
- `git diff 001490c1..HEAD -- '*Cargo.toml' Cargo.lock` → hh-env `[dev-dependencies]` ×7 internal only; Cargo.lock +7.
- reverse-edge grep → 0 hits for `hh-env|hharness-env` in {gateway,eval,bench,lab,plugin,bundle,analysis}/Cargo.toml.
- `git diff --name-only 001490c1..HEAD -- schema clients crates/hh-codegen crates/hh-embed-schema crates/hh-embed-client-generated THIRD_PARTY_NOTICES.md` → empty.
- `bash scripts/check-drift.sh` → ok — regenerated outputs byte-identical.
- feature diff → none (only dev-dep section added round-5-wide).
- PADR→ADR cite map → 39 ADRs; each landing ADR cites its admitted packet (PADR-R5-1…7 / Packet A–G / S.1–S.6) or a direct section; ADR-0385–0393↔PADR-R5-1, 0387/0391/0392/0396–0398↔PADR-R5-2, 0399–0405↔PADR-R5-3/Packet C, 0395/0418↔PADR-R5-4/D, 0417↔PADR-R5-5/E, 0416↔PADR-R5-6/F, 0414/0415↔PADR-R5-7.
- classes one-home → 6 new classes all in `crates/hh-ledger/src/classes.rs` only.
- packet inventory → 20 `docs/build/reports/2026-11-*.json` parse; dispositions executed×4/failed×2/inconclusive×2/withheld×6/unselected×2/reported×2/schema-verdict×2.
- `grep -cE '^\| [0-9]+[ab]? note'` manifest → 47 round-5 note rows covering every recorded deviation/SPLIT/AMEND.
- `adr-index.sh --check docs/adr` → generated section identical to committed README (204 ADR files ADR-0217–0420).
- `bash scripts/gen-revisit-triggers.sh --check` → in sync.
- matrix pointer resolution → 107 MET rows; only missing evidence = `pr/REC5.2.md` + `runs/REC5.2.md` (this ticket's artifacts).
- anchors → `req-e-210`/`req-h-004`/`ac-9-1`/`ac-9-2`/`s9` once each in supplement; `ac-rec5-2-3` in contract.
- `bash scripts/check-living-docs.sh` → OK 456 files (baseline, pre-artifacts).
- `bash scripts/check-bm-tools.sh` → OK.
- `check-build-memory.sh .` → 0 violations / 8 scoped warnings (baseline, unchanged).
- fmt: no Rust touched — `cargo fmt` not applicable (docs-only).
- 2026-11-05T00:42Z CLOSE — correction per BM-HIST-01: header `Closed:`
  committed at `6d43cf1f` (line 10) carried the `00:5xZ` placeholder;
  the actual close stamp is 2026-11-05T00:42:24Z (`date -u` at §6.5).
- ci-boundary/1: `ci: pass #275@6d43cf1 (cargo-deny; gitleaks; build·test·fmt·drift ubuntu; build·test·fmt·drift macos — run 41942931551)` → `logs/ci-rec5.2.json` (force-added; read_at per record).

## Evidence report

See `docs/build/pr/REC5.2.md` — the same AC→evidence table stands as the report.
Headline: round-5 reconcile complete and clean — 63 landed rows tagged
(in-spec covers the stamped REQ set; spec-implied the admission machinery and
honest-disposition records; ticket-added the build tooling) with zero contradicting
deliverables; every dependency/generator/tier/amendment admission verified against
committed records (V-1…V-15); 1 proposed unticked amendment (R5A-1 appendix range)
+ 6 dispositioned rows; REQ-E-210 resolved E210-1…7 at D/E engineered; the 3 OPEN
deferrals carry forward and no withheld predicate was promoted.
