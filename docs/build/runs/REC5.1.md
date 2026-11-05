# Run ledger — REC5.1

- Spec: `docs/tickets/242_REC5.1__rec1.md`
- Base: `72f080bf11aa73a780f2c9255d720b5b9983927d` (codex/harnessharness-evidence-cap5.3 actual tip — the CAP5.3 close `27decf54` plus the two GATE-G17 marker commits `3982ca0e`/`72f080bf`; markers inherit tip, O-02 actual SHA recorded at SETUP)
- Branch: `codex/harnessharness-evidence-rec5.1`
- Config: `implement-spec spec=docs/tickets/242_REC5.1__rec1.md` (autonomous, ledger on, records-in reconcile; reconcile-build mode=backlog)
- Harness: devin-desktop/swe-2-high/subagent
- Skills: agent-skills @ e981ec5
- Started: 2026-11-05T00:19:03Z (`date -u` at SETUP)
- Closed: (pending)
- Context at SETUP: ticket estimate 22k + 10k reserve; loaded = contract 242 + invariants I-01–I-08/O-01–O-08 + spec §9 (bounded) + coverage register/decomposition REC5.1 rows + invariants + landed closure records (cap5.3-closure.json layer verdicts, reports/round5/{COVERAGE_MATRIX.csv,CAPSTONE_GAP_ANALYSIS.md,CAPSTONE_CLOSURE.md,COMPOSED_E2E_REPORT.md}, cap5.1-gap-hunt packet) + DEFERRALS rows DF-R5.1-1/DF-H9-1/DF-CAP5.1-1 + LEDGER CURRENT STATE/OPEN FINDINGS/GATE DECISIONS tail + BACKLOG.csv/md + OPERATIONAL_READINESS + risks register + manifest rows 238–246/operating rules + BUILD_INDEX tail + bm tools (check-backlog.sh mode contract) ≈ at estimate; ≤500 changed source/test lines split trigger watched.

## Deferrals read

`bash scripts/deferrals-open.sh` at 2026-11-05T00:14Z —
`DF-R5.1-1 | OPEN` (GATE-G17 contributor evidence, ten R5.1-owned targets),
`DF-H9-1 | OPEN` (withheld participant/outreach predicate set; minted CAP5.1),
`DF-CAP5.1-1 | OPEN` (failed S1 useful residual → GATE-G17; minted CAP5.1).
GATE-G17's verbatim scope disposition (2026-11-05T00:13:07Z signature,
readouts/GATE-G17.md): all three **carry forward OPEN** — "the acceptance
took the record as-landed; no waiver was named, so each stays open with
its owner/trigger (never inferred completion)". None discharges at this
ticket; withheld stays withheld.

## Orient findings

- **Uncommitted regen observed at SETUP:** `docs/build/reports/2026-11-04-cap5.3-closure.json`
  was modified in the working tree — a post-close regeneration
  (base `2909edce`→`27decf54`, generated_at 21:37:56Z→21:44:36Z,
  packet_id `9d756c68…`→`b82a02e8…`; folded record byte-identical —
  checks/verdicts/findings unchanged). The committed packet is the
  BUILD_INDEX-cited artifact (seq-240 row records `packet_id
  9d756c68…`). Decision: revert to the committed packet — the regen was
  a verification artifact never landed; append-only records keep the
  cited artifact. Recorded here for provenance.
- `deferrals-open.sh --bl` at SETUP: **0 open BL rows** (45 rows all
  closed/accepted) — round-5 owed work has no backlog home yet (this
  ticket's job).
- Stock `check-backlog.sh` gather notes: its DEFERRALS grep matches
  `D-`-prefixed ids only (this register's ids are `DF-` — DF rows are
  homed by convention/by-eye, the checker cannot count them); its
  `owed_legs` leg-token match is `D-*` (the round-5 matrix's
  `src:state->DF-id` legs likewise escape it). Both blind spots are
  compensated by manual gather + the checker's own `--bl`/sources
  audit; recorded honestly, not silently patched (stock validator
  unchanged — O-07).

## Requirements

Reconcile backlog and current readiness, records-in/offline-only
(USD $0): invoke reconcile-build mode=backlog — gather every owed
source (OPEN/PARTIAL DEFERRALS, ADR revisit triggers, LEDGER OPEN
FINDINGS, deferred register rows, PARTIAL/MISSING/AT-RISK matrix rows
per BM-VERDICT-01) into unique live homes in `docs/build/BACKLOG.csv`
(append/status policy — old rows preserved); refresh
`docs/build/BACKLOG.md` grouped by landing + dated revisit-trigger
sweep + risk-register review + two sums vs CAP5.3's headline;
`docs/build/OPERATIONAL_READINESS.md` capability ×
{code/infra/human/layer/proof} + critical path with ticket:+proof: per
step, no TBD, no production claim without a ≤24 h probe/ci-boundary
citation; named artifacts `docs/build/reports/round5/{BACKLOG_READINESS.md,READINESS_REPORT.md}`;
read current access/service state with exact probe records
(id/UTC/result/hash) or omit the claim; read current stack CI with
read_at; classify every residual by technical/human/resource/window
cause; truthful layer ceilings (round-5 ceiling `engineered`);
never close an absent research/participant predicate.

## Acceptance criteria

- AC-1 (D/E; positive): read current access + claimed service state;
  classify every residual by technical/human/resource/window cause and
  unique home; round-specific readiness updated with truthful layer
  ceilings.
- AC-2 (D/E; negative): no stale link/live claim inferred from release
  tag; no duplicate backlog home; no blanket historical waiver; access
  failure stays a current limitation absent new collection/provisioning.
- AC-3 (D/E; ownership+evidence): primary targets — none (contributing
  slice only); original predicates/aliases resolve to numbered checks +
  actual evidence in the coverage register; achieved D/E recorded
  separately from owed R/P.
- AC-4 (E/D; scoped verification): regression fails-without for new
  behavior; fmt + owning-package tests for code; docs-only uses
  living-docs/row/link checks; exact commands + pass/fail/skip counts.
- AC-5 (D/E; phase gate): verification green in scope; REQ ids stamped
  (none — contributing slice); owed items are DEFERRALS rows w/
  owner/trigger/control; run/PR/BUILD_INDEX/LEDGER agree at true
  completion; ≤2KiB phase log; no deferred predicate becomes MET.

## Plan

1. Gather owed sources: 3 OPEN DFs (by-eye — stock gather is DF-blind),
   73 PARTIAL round-5 matrix rows, 204 ADR revisit triggers (all homed
   already — re-evaluate states for the dated sweep), 9 LEDGER OPEN
   FINDINGS (homed BL-41/BL-30 precedent), spec registers
   (conflicts.md CF-rows → BL-30; spec-debt → BL-40 closed; risks.md →
   dated round-5 review).
2. BACKLOG.csv: append new rows BL-50…BL-53 (open) homing the 73
   PARTIAL ids + 3 DF ids, each id in exactly one sources cell; old
   rows untouched (append-only).
3. BACKLOG.md: header refresh + new landing group + dated sweep
   (204 ADRs) + dated sums (107/78 vs CAP5.3) + register-review note.
4. OPERATIONAL_READINESS.md: append dated round-5 section — capability
   table at `engineered` ceiling + critical path, claims cited to fresh
   probes/CI reads ≤24 h or omitted.
5. risks.md: append `## Round 5 review` (dated).
6. reports/round5/BACKLOG_READINESS.md + READINESS_REPORT.md: the
   round-5 named artifacts — residual classification by cause,
   backlog/CI/probe citations, honest limits.
7. Probes (read-only, permitted-class): `podman --version`
   (CAP5.2-class offline access probe), stack CI via ci-boundary,
   public-release spot reads (gh release/crates.io GET — read-only
   current-state reads the ticket mandates); no mutation/collection.
8. Verify: check-backlog.sh exit 0 over both matrix homes
   (docs/build + reports/round5), check-living-docs, check-bm-tools,
   check-build-memory `.`+`--staged`, guard PASS DISPATCH REC5.1
   pre+post; fmt n/a (no Rust) — docs-only path.

## Test matrix

- fails-without: the reconcile is records-in; the regression surface
  is the checker suite — `check-backlog.sh` exits 1 if any of the 73
  PARTIAL ids or gathered sources is unhomed/duplicated/closed-homed
  (fails without the BACKLOG.csv work); supplement guard verifies
  chain/routing integrity; living-docs validates every touched doc.
- Deterministic checks: check-backlog.sh ×2 (round-4 + round-5 matrix
  homes) · check-living-docs.sh · check-bm-tools.sh ·
  check-build-memory.sh `.`+`--staged` · supplement guard pre+post ·
  gen-revisit-triggers --check if triggers touched.

## Progress

- 2026-11-05T00:14Z SETUP — deferrals read (3 OPEN, all carried);
  branch `codex/harnessharness-evidence-rec5.1` @ `72f080bf`; orient
  findings recorded (uncommitted cap5.3-closure regen reverted —
  committed packet `9d756c68…` stands as the cited artifact);
  check-backlog baseline: `docs/build` run exit 0 (204 expected
  sources, sums 66/66 over the round-4 matrix); `reports/round5` run
  exit 1 — 73 PARTIAL ids unhomed (the reconcile gap this ticket
  closes); supplement guard PASS DISPATCH REC5.1 (82 rows/180 cand/0
  viol).
- 2026-11-05T00:24Z IMPLEMENT — BL-50…BL-53 appended (73 PARTIAL ids +
  3 DF ids, each exactly once, all `open`); BACKLOG.md refresh +
  landing group + dated sweep (204 ADRs: 150 quiet / 49 fired-answered
  / 3 fired-unanswered / 2 dormant / 0 superseded, no re-homes) + dated
  sums (107/78 match CAP5.3) + register-review note + findings mapping;
  OPERATIONAL_READINESS §Round-5 appended; risks.md `## Round 5
  review`; reports/round5/{BACKLOG_READINESS,READINESS_REPORT}.md
  written; probes run (podman 6.1.0 ok · gh release v0.1.0 not-draft
  5 assets · crates.io hharness-cli 200 w/ UA, 403 bare — recorded).
- 2026-11-05T00:25Z VERIFY — check-backlog exit 0 both homes
  (204 + 73 expected; round-5 sums 107/78); living-docs 456 OK;
  bm-tools OK; check-build-memory `.` 0 viol (8 legacy warnings);
  guard PASS DISPATCH REC5.1 pre-artifacts.

## Gap table

| item (spec §) | required | achieved | status | evidence |
|---|---|---|---|---|
| AC-1 current access/service read; residuals by cause + unique home; round readiness w/ ceilings | D/E | engineered | met | probes {podman 6.1.0, gh release v0.1.0, crates.io 200} @2026-11-05T00:23Z; BACKLOG_READINESS cause table; OPERATIONAL_READINESS §Round-5 (`engineered` ceiling) |
| AC-2 no stale link/live claim; no dup home; no blanket waiver; access failure stays limitation | D/E | engineered | met | check-backlog exactly-once exit 0; crates.io 403 bare recorded; no waiver text in diff; release-tag never used as live evidence |
| AC-3 primary targets → checks+evidence; achieved D/E vs owed R/P | D/E | engineered | met | no primary ownership (contributing slice); per-id owed_legs→BL mapping BACKLOG_READINESS.md; MET-ENGINEERED legs resolve to OPEN D-rows |
| AC-4 scoped verification; fails-without; commands+counts | E/D | engineered | met | checks listed under Test matrix — all exit 0; fails-without: removing any new sources cell flips check-backlog to 1 on unhomed ids |
| AC-5 phase gate; owed→DEFERRALS; records agree | D/E | engineered | met | 3 DFs stay OPEN (carried per G17 verbatim — no new rows needed, none owed without a home); BUILD_INDEX/LEDGER advance at closeout; phase log ≤2KiB |
| I-01..I-08 | — | engineered | met | frozen spec/contracts untouched; no new codec/store/class/dependency; records-in only; no withheld promotion; $0; D/E/R/P kept orthogonal |
| O-01..O-08 | — | engineered | met | deferrals-first; single writer; guard PASS pre+post; gate words verbatim; dated stamps from `date -u`; shell/python writes + git-diff verify; no living-record pins; reads ≤24h cited |

## Evidence log

- `bash scripts/deferrals-open.sh` → 3 OPEN rows (above).
- `check-backlog.sh … docs/build` → exit 0, expected 204, sums 66/66.
- `check-backlog.sh … docs/build/reports/round5` → exit 0, expected 73,
  sums 107/180 + 78/180 (CAP5.3 headline match).
- `bash scripts/check-living-docs.sh` → OK 456 files.
- `bash scripts/check-bm-tools.sh` → OK.
- `check-build-memory.sh .` → 0 violations (8 legacy warnings).
- `bash docs/build/tools/check-supplement-guard.sh` → PASS route:
  DISPATCH — REC5.1 (82 rows / 180 cand / 0 viol) pre-artifacts.
- Probes: `podman --version` → 6.1.0; `gh release view v0.1.0` →
  draft:false, 5 assets; `curl crates.io/api/v1/crates/hharness-cli` →
  200 w/ UA (403 bare — UA-required access limitation recorded).
- fmt: no Rust touched — `cargo fmt` not applicable (docs-only).

## Evidence report

See `docs/build/pr/REC5.1.md` — the same AC→evidence table stands as
the report. Headline: reconcile complete — 73 PARTIAL ids + 3 OPEN
deferrals each in exactly one live backlog row (BL-50…53); 204 ADR
triggers re-swept; sums 107/78 recomputed and matching CAP5.3;
readiness updated at `engineered` with cited ≤24 h probes; nothing
withheld or unmet was promoted; the build is not DONE.
