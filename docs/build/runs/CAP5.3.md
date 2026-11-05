# Run ledger — CAP5.3

- Spec: `docs/tickets/240_CAP5.3__cap3.md`
- Base: `2909edce90f7991f0d1eb140e8c597a0d07ca38f` (codex/harnessharness-evidence-cap5.2 tip; PR #272 green at code head `f06fe1b`)
- Branch: `codex/harnessharness-evidence-cap5.3`
- Config: `implement-spec spec=docs/tickets/240_CAP5.3__cap3.md` (autonomous, ledger on, records-in closure)
- Harness: devin-desktop/swe-2-high/subagent
- Skills: agent-skills @ e981ec5
- Started: 2026-11-04T21:32:04Z (`date -u` at SETUP)
- Closed: 2026-11-04T21:44Z (`date -u` at §6.5 close)
- Context at SETUP: ticket estimate 24k + 10k reserve; loaded = contract 240 + invariants I-01–I-08/O-01–O-08 + coverage register rows (REQ-C-005/H-009 + bounded heads) + decomposition rows 238–246 + CAP5.1/CAP5.2 run ledgers + CAPSTONE_GAP_ANALYSIS/COVERAGE_MATRIX/COMPOSED_E2E_REPORT + both capstone packets + GATE DECISIONS + readouts G15/G16 + LEDGER OPEN FINDINGS + DEFERRALS rows DF-R5.1-1/DF-H9-1/DF-CAP5.1-1 + BUILD_INDEX tail + historical CAPSTONE_CLOSURE/GATE-ACCEPT precedent ≈ at estimate; ≤600 changed source/test lines split trigger watched.

## Deferrals read

`bash scripts/deferrals-open.sh` at 2026-11-04T21:32Z —
`DF-R5.1-1 | OPEN` (GATE-G17 contributor evidence, ten R5.1-owned targets),
`DF-H9-1 | OPEN` (withheld participant/outreach predicate set; minted CAP5.1),
`DF-CAP5.1-1 | OPEN` (failed S1 useful residual → GATE-G17; minted CAP5.1).
All three are carried OPEN; none discharges here.

## Requirements

Close layers and prepare fresh acceptance: round-specific closure report
with engineering/research/portability/public/adoption verdicts, per-ID
evidence and a unique owed-work owner/trigger per residual; reconcile
old/new findings; recompute engineering-closed vs requirement-satisfied
totals and highest evidence/domain ceiling from the matrix; three-part
signature list for GATE-G17 (accepted deviations w/ ADR-control, scoped
owed legs w/ owner/trigger, waiver requests w/ operator-quoting ADR);
never mark the whole build DONE; never promote a withheld
participation/publication predicate. Offline-only; records-in; USD 0.

## Acceptance criteria

- AC-1 (D/E; positive): closure report with the five layer verdicts,
  per-ID evidence, unique owed-work owner/trigger; old/new findings
  reconciled; fresh acceptance readout prepared; admitted
  null/negative/inconclusive research may complete honestly.
- AC-2 (D/E; negative): missing/unadmitted study or participant work
  never marked complete/waived by engineering evidence; explicit human
  scope disposition required; no reused old GATE-ACCEPT signatures, no
  overwritten old reports, no favorable-effect completion criterion.
- AC-3 (D/E; ownership+evidence): primary targets REQ-C-005, REQ-H-009 —
  predicates resolve to numbered checks + actual evidence in the
  coverage register; achieved D/E recorded separately from owed R/P;
  contributor evidence R5.53/R5.52/R5.54/R5.49/R5.55 is an evidence
  dependency, not a dispatch dependency — full-target predicates stay
  owed to their later owner.
- AC-4 (E/D; scoped verification): regression fails without new
  behavior; fmt + owning-package tests for code; docs checks for
  docs-only work; record commands + counts; scratch stores.
- AC-5 (D/E; phase gate): verification green; REQ ids stamped; owed
  items are DEFERRALS rows; run/PR/BUILD_INDEX/LEDGER agree; no deferred
  full-target predicate becomes MET by this close.

## Concrete checks (AC-1/AC-2 binding — recorded at SETUP)

- CL-1 matrix recompute: 180 rows; verdict histogram; engineering-closed
  (MET+MET-DIFFERENTLY+MET-ENGINEERED) vs requirement-satisfied
  (MET+MET-DIFFERENTLY); highest achieved_domain; BM-VERDICT-01 only.
- CL-2 five-layer fold: engineering/research/portability/public/
  adoption verdicts folded from committed packet dispositions — never
  hand-asserted.
- CL-3 per-ID evidence: every verdict + owed leg cites a committed
  record path that exists and parses; REQ-C-005/REQ-H-009 register rows
  resolve to CAP5.3 AC-3 + contributors R5.49/52/53/54/55.
- CL-4 owed-work uniqueness: every owed leg routes to exactly one
  resolvable home (DF row w/ non-empty unblocked-by/how-to-verify, the
  committed `241_GATE-G17__accept.md`, or a `landed:` ticket id).
- CL-5 reconcile old/new: CAP5.1 F-1…F-4 repairs still present; LEDGER
  OPEN FINDINGS capstone items adjudicated (CC4 hygiene nit; GATE-G1
  carried items surfaced to the signature list).
- CL-6 withheld never promoted: recursive scan — no
  `claim_awarded:true`; withheld/failed/unselected/inconclusive
  dispositions never read met; composed residual legs pending/recorded,
  never closed; `live_authorization: pending{not_claimed}`; study
  `unmet{…}` verbatim.
- CL-7 no old signature: no committed `readouts/GATE-G17.md`; no record
  cites GATE-ACCEPT/GATE-G1 as round-5 satisfaction; unmet study
  predicates (s1/s3/s4) never fold to met.
- CL-8 three-part signature list: accepted deviations cite an existing
  ADR/split-review record; scoped owed legs carry owner+trigger; waiver
  requests name the operator-quoting requirement.
- CL-9 build never marked DONE: CURRENT STATE projectStatus ≠ DONE;
  manifest seq-241 row is GATE-G17.
- CL-10 GATE-G17 prep: gate ticket + readout template exist;
  acceptance_prep emits read items, sha256 digests of the capstone
  artifacts, expiry/void fields, the verbatim criterion.

## Plan

Phase 0 setup + deferrals ✓ → Job 1 fleshing (contract dated note,
manifest 240 note, this ledger, PHASE LOG entry; guard DISPATCH CAP5.3
already PASS) → Job 2: `docs/build/tools/cap5_closure.py` records-in
checker (CL-1…CL-10) → emitted packet
`docs/build/reports/2026-11-04-cap5.3-closure.json` (`hh-cap5-closure/1`)
→ `docs/build/reports/round5/CAPSTONE_CLOSURE.md` → self-review + gap
table → verify (living-docs/bm-tools/check-build-memory `./--staged`,
guard, fmt, ≤600 source/test lines) → commit → PR → ci-boundary → §6.5
close (BUILD_INDEX seq-240, lastCompleted CAP5.3 / nextTicket GATE-G17)
→ report.

## Test matrix

Docs/analysis ticket — no Rust surface. Regression = the checker's own
deterministic checks over committed records (a mutated packet, missing
row, promoted withheld claim or absent owed-work home flips a check to
FAIL — fails-without by construction of the check predicates). Docs
checks: check-living-docs / check-bm-tools / check-build-memory (`.`
+ `--staged`) / supplement guard.

## Progress

- 2026-11-04T21:32Z — SETUP: branch forked at `2909edce`; deferrals read
  (3 OPEN — all carried); contract/invariants/coverage/decomposition/
  CAP5.1+CAP5.2 records/GATE DECISIONS/readouts/BUILD_INDEX loaded
  (bounded). Guard `PASS route: DISPATCH — CAP5.3` (82 active rows /
  180 candidates / 0 violations / 8 scoped warnings).
- 2026-11-04T21:34Z — Job 1 fleshing: contract 240 dated note (CL-1…
  CL-10 bound) + manifest row-240 note + PHASE LOG fleshing entry.
- 2026-11-04T21:5xZ — Job 2: `cap5_closure.py` records-in checker
  written (CL-1…CL-10); first pass found 3 authoring defects in the
  checker itself (VERDICTS set missing MET-ENGINEERED — 29 false
  findings; engineering layer routed through the r5.* glob; over-strict
  disposition scan on the dispositions-absent sim-battery/1 record) —
  all repaired; manifest first-.md token fix (DEFERRALS.md precedes
  CAPSTONE_CLOSURE.md — stock checker extracts first `.md` per row).
  Re-run clean: **10/10 checks pass, 0 findings**; packet emitted
  `reports/2026-11-04-cap5.3-closure.json` id `9d756c68…`;
  `CAPSTONE_CLOSURE.md` written from the packet.
- 2026-11-04T22:0xZ — verify: fmt clean; living-docs 456 OK; bm-tools
  OK; ci-gating OK (14/14 allowlisted); bm `.` 0 violations (8 legacy
  warnings); guard PASS DISPATCH CAP5.3 post-artifacts; changed
  source/test lines = 572 (`cap5_closure.py` only — all other changes
  are docs/build records + contract/manifest/ledger notes) ≤600, no
  trigger.


## Gap table

| item | required | achieved | status | evidence |
|---|---|---|---|---|
| AC-1 closure report, five layers, per-ID evidence, unique owed homes, reconcile, acceptance prep | D/E | engineered | met | `reports/round5/CAPSTONE_CLOSURE.md`; packet `2026-11-04-cap5.3-closure.json` — layer verdicts {engineering:executed, research:inconclusive, portability/public/adoption:withheld}; totals eng_closed 107 / req_satisfied 78 / ceiling engineered; 49 rulings + 3 carried deviations; 7 owed legs; 3 waiver requests |
| AC-2 missing/unadmitted work never complete; no old signatures; no favorable-effect criterion | D/E | engineered | met | CL-6 withheld-never-promoted scan (17 packets + residuals — zero claim_awarded / promoted); CL-7 no GATE-G17 readout + unmet{} verbatim; fold has no effect-driven term |
| AC-3 REQ-C-005/REQ-H-009 predicates→checks+evidence; achieved vs owed | D/E | engineered | met | CL-3 register rows resolve CAP5.3 AC-3 + contributors R5.49/52/53/54/55; closure report primary-target table records achieved D/E vs owed R→G17/P→DF-H9-1+G17 |
| AC-4 scoped verification; fails-without | E/D | engineered | met | checker 10/10 (fails-without by construction — mutated packet/row/home flips a check; demonstrated: 3 checker-defect classes caught in-run); fmt clean; living-docs/bm-tools/bm `.`/ci-gating/guard green |
| AC-5 phase gate; no predicate promoted | D/E | engineered | met-at-close | 3 DF rows carried OPEN; CL-9 projectStatus stays IN_PROGRESS; run/PR/BUILD_INDEX/LEDGER agree at close |
| I-01 preserve frozen spec/authority/aliases | — | engineered | met | records-in only; no semantic/wire/dependency change |
| I-02 owners/durability | — | engineered | met | one writer; shell/python writes + git-diff verified |
| I-03 subject/grades | — | engineered | met | no lifecycle writes; composed packet evidence cited verbatim |
| I-04 treatment/caps | — | engineered | met | no dispatch; $0 spend |
| I-05 methods/missingness | — | engineered | met | study unmet{} verbatim retained; inconclusive fold honest |
| I-06 receipts/readers | — | engineered | met | recipient verdicts cite composed packet; no recipient claim |
| I-07 required domains orthogonal | — | engineered | met | five-layer fold separate; no deferred predicate became MET |
| I-08 scope/gates | — | engineered | met | withheld stages stay withheld; no outreach/spend/publication |
| O-01 orientation/clock | — | engineered | met | deferrals first; bounded reads; `date -u` stamps |
| O-02 state writer/stacking | — | engineered | met | one writer; branch forks cap5.2 tip @2909edce |
| O-03 explicit gates | — | engineered | met | GATE-G17 left RETURN PASS pending; nothing inferred |
| O-04 routing guard | — | engineered | met | guard PASS DISPATCH CAP5.3 pre+post |
| O-05 verification | — | engineered | met | fails-without by construction; commands+counts in evidence log |
| O-06 close evidence | — | engineered | met-at-close | ci-boundary read at close; ≤2KiB phase log |
| O-07 validation/history | — | engineered | met | bm `.`/`--staged` + repo row tools + guard, separately; no history rewrite (append-only note/PHASE LOG) |
| O-08 production/sensitive | — | engineered | met | no production mutation; no credentials recorded |


## Evidence log

- `python3 docs/build/tools/cap5_closure.py` → **10/10 pass,
  findings=0**; packet `docs/build/reports/2026-11-04-cap5.3-closure.json`
  `hh-cap5-closure/1` disposition `reported`, `packet_id 9d756c68…`,
  `build_status: not_done`, `live_authorization: pending{not_claimed}`,
  `spend_usd: 0`.
- `bash scripts/deferrals-open.sh` → DF-R5.1-1 / DF-H9-1 / DF-CAP5.1-1
  OPEN (all carried, none discharged here).
- `AGENT_SKILLS_ROOT=$HOME/agent-skills bash docs/build/tools/
  check-supplement-guard.sh` → **PASS route: DISPATCH — CAP5.3**
  pre+post-artifacts (82 rows / 180 candidates / 0 violations / 8 scoped
  warnings; manifest first-.md fix verified — row extracts DEFERRALS.md).
- `cargo fmt --all --check` → clean (no Rust touched — run anyway).
- `bash scripts/check-living-docs.sh` → OK, 456 files.
- `bash scripts/check-bm-tools.sh` → OK.
- `bash scripts/ci/check-ci-gating.sh .github/workflows/ci.yml` → OK
  (readers scan clean, 14 hits / 14 allowlisted — checker lives under
  docs/build/tools/, a docs path).
- `bash $AGENT_SKILLS_ROOT/skills/build-memory/scripts/
  check-build-memory.sh .` → **0 violations** (8 legacy warnings only).
- Changed source/test lines: **572** (`cap5_closure.py` only; the
  reports/packet/run ledger/contract note/manifest note/ledger entries
  are docs/build records, not source/test) — ≤600 trigger not crossed.


## Evidence report

Records-in closure over the landed round-5 record: **10/10 checks
pass, 0 findings.** Five layer verdicts folded from committed packet
dispositions — engineering `executed` (hh-cap5-composed/1 8/8 stages,
activation+ops-live executed, 0 MISSING/AT-RISK), research
`inconclusive` (study report `reported` carrying
`unmet{efficacy_unavailable;parity_withheld}` + s1/s3/s4 unmet
verbatim; S1 useful `failed`, parity/confirm `withheld`), portability
`withheld` (outside reviewer never supplied), public `withheld` (stage
voided at G16), adoption `withheld` (nothing published to observe).
Totals recomputed: engineering-closed **107** / requirement-satisfied
**78** / highest domain `engineered` (180 rows: MET 78 /
MET-ENGINEERED 29 / PARTIAL 73). Every owed leg routes to exactly one
resolvable home (3 OPEN DF rows + GATE-G17); 52 accepted-deviation
entries each carry a control record; 3 waiver requests each name the
operator-quoting requirement. CAP5.1 findings F-1…F-4 verified repaired;
CC4 nit adjudicated no-action-accepted; GATE-G1 carried items
re-surface for fresh signature. The build is not DONE — next GATE-G17.
ci: pass #273@20bdd7d — `ci-boundary/1` at 2026-11-04: all 4 checks pass (run 41918289769); stack #272…#260 pass; `docs/build/logs/ci-cap5.3.json`.

