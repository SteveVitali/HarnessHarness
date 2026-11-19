# DOC6.1 run ledger — Round-6 docs refresh (manifest 325)

Harness: devin-cli/swe-2-high/subagent · Branch `codex/harnessharness-doc6.1` ·
base `codex/harnessharness-cap6.3` @ `65d5b479` (signed GATE-G32 tip) ·
isolated worktree `/tmp/hh-doc61-wt` · offline, USD $0.00, no merges, no
secrets, no production mutations (read-only `gh` reads only). Ticket:
`docs/tickets/325_DOC6.1__docs.md`. Owns no REQ-X ids; wording cites
REQ-XF-136, REQ-XH-019, REQ-XH-119, REQ-XD-092 without re-owning them.
Ordered stages honoured — stage-1 commit `451cc07e` landed before stage 2,
stage-2 commit `361f46a6` before this record.

## Deferrals-first (gotcha 1)

`bash scripts/deferrals-open.sh` → six lead-OPEN rows; each read via
`scripts/bm-row.sh`. Latest **dated dispositions** govern (gotcha 8):

| Row | Latest dated disposition |
|---|---|
| DF-R6.45l-1 | DONE 2026-11-19 — derivation machinery landed (R6.51, PR #413) |
| DF-R6.50-1 | DONE 2026-11-19 — HUMAN-H17 readout + superseding flip records carry `visibility: public` |
| DF-CAP6.1-1…5 | DONE 2026-11-19 — CAP6.2 (guard restatements, deviation record, waiver routing, normalization, checker repairs) |

**Zero open DF rows by dated marker.** Lead-token vs machine-status
reconciliation is coordinator-owned at round closure (MAINTAINING.md) —
proposed text below; this run touches no coordinator-owned file.

## Stage 1 — `refresh-repo-docs` (committed `451cc07e`)

Detector 1202 docs → 1 flagged "broken ref" (verified false positive —
`old[k](1−p)` math in **frozen** `docs/design/23_parity-method-review.md`)
+ 38 stale suspects (24 report-only by mode; rest audited).

- **Stale fixed (4):** `docs/results/README.md` (flip records + public
  dataset), `docs/evidence.md` (results-site/hir-parity/config-sweep cite
  superseding `-flip` records; dataset public `a11591e9…`), `docs/status.md`
  predecessor lines kept verbatim with the new dated update, `README.md`
  spine site wording.
- **Gaps filled (2):** `docs/status.md` Round-6 section (GATE-G32 sums,
  withhelds verbatim, waiver-not-execution, routed scope stays owed);
  `docs/reference.md` round-6 binaries + `docs/architecture.md` one-line
  apparatus note.
- **Verified-no-change (6):** getting-started, CONTRIBUTING, GOVERNANCE,
  CHANGELOG, both foreign READMEs.
- **Report-only (24):** frozen `docs/design|research/*`, historical
  `docs/tickets/*` — surfaced in the findings ledger, never rewritten.

Every changed claim re-verified: `[[bin]]` names in each `Cargo.toml`;
HUMAN-H17/GATE-G31 readouts; `-flip` record members; CAP6.2's folded sums.
Withheld outcomes carried verbatim; nothing promoted.

## Stage 2 — `agent-docs` refresh (committed `361f46a6`)

Detector: 5 docs, **0 issues** before and after. Surgical additions only —
root architecture rows (experiment apparatus + results site), Build memory
Round-7 entry points (CAP6.2 `LATER_PASS_ROUTING.md`, CAP6.3
`planning/2026-11-19_round7-decision-memo.md`), `crates/` map binary notes,
`docs/AGENTS.md` `check-site.sh`/`check-publication.sh` surface. The
"Build memory" section is present (AC-2). CLAUDE.md bridge intact
(`@AGENTS.md`).

## Verification — counts (full detail in `reports/round6/READER_DOCS_CHECK.md`)

- check-living-docs: **OK** — 554 files.
- check-position: **OK 14/14** working tree **and** `--head` @`451cc07e`.
- check-examples: **OK** (4 cards, 6 pins, 29 ids) both modes.
- check-trace: **OK all legs** both modes.
- check-contributing: **OK 12 ids** both modes.
- check-site `--profile internal` **and** `public`: **OK 15 legs** each.
- run-examples: **EX-01–04 green**; live `skipped(no routine PR calls — ADR-0363)`.
- check-quickstart: **green** on committed HEAD `451cc07e` (18 verbatim
  lines → `connectivity:ok` → `succeeded/completed`).
- `python3 scripts/site/build.py --check`: drift clean.
- agent-docs detector re-run: **0 issues**.
- Claims lexicon: green via check-position on both passes (no banned phrase).

## Stack CI read (BM-CI-01 / AC-4)

`ci-boundary.sh --stack --no-wait`, `read_at` 2026-11-19T07:15Z, committed
records `reports/round6/ci-{stack,410,413,420}-doc6.1-2026-11-19.json`:
#407–#412 pass; #414–#419 FAIL on the diagnosed `check-ci-gating`
docs/build-literal leg (cargo-deny+gitleaks green on all); out-of-band #410
pass @`11a6524`, #413 pass @`ae72242`, **#420 (I-3 repair) FAIL** on the
same leg family — the repair is in-flight, red recorded honestly.

**Own-PR boundary read:** PR #421 created (head `d04877e5`);
`ci-boundary.sh --pr 421 --stack --no-wait` at `read_at`
2026-11-19T07:18:47Z (record `reports/round6/ci-pr421-doc6.1-2026-11-19.json`):
**FAIL on `build · test · fmt · drift` (ubuntu-latest, run 44478574009;
macos pending at read)** — failed step `CI scripts self-test`
(`test-check-ci-gating`), the identical inherited leg: un-allowlisted
`docs/build/**` literal reads in `crates/hh-runner/src/bin/fp_e_emit.rs`
and `fp_results.rs` only. This docs-only delta adds no source reads;
the red is the I-3 span, not DOC6.1's. cargo-deny + gitleaks were
pending at read time on #421; they pass on every stacked PR.

## Dispatch guard (BM-DOCS-01)

`AGENT_SKILLS_ROOT=/Users/stevenvitali/agent-skills bash docs/build/tools/check-supplement-guard.sh`
→ **FAIL / route `HOLD — gate GATE-G32 pending — RETURN PASS`**, exactly
two ✗ items, both coordinator-owned bookkeeping:

- `[last-completed] lastCompleted GATE-G32 has no landed/decision evidence`
- `[nextticket] nextTicket 'DOC6.1' is not the active round's first
  unlanded row (GATE-G32)`

The signed GATE-G32 readout, the `## GATE DECISIONS` row and the
change-log entry all exist (LEDGER lines 20/48/1362); the missing piece
is the **GATE-G32 BUILD_INDEX landing row** — the index stops at 323
(CAP6.3). This surfaced when the gate was signed after CAP6.3's own
`guard PASS (DISPATCH — CAP6.3)`. Same owner class as the lead-token
reconciliation: proposed text below, never written by this worker.
stock_violations 0 / stock_warnings 64 (all dated-baseline-explained).

## Proposed coordinator text (worker owns none of these files)

- **BUILD_INDEX gate row (proposed — discharges both guard ✗ items):**
  `| 324 | GATE-G32 | gate | codex/harnessharness-cap6.3 | [#419](https://github.com/SteveVitali/HarnessHarness/pull/419) | codex/harnessharness-cap6.2 | 2026-11-19 | PASSED — accepted as recorded | — | landed — Round-6 engineering closure 410/456 accepted verbatim (deviations+waivers accepted: XC-325 waived-not-executed, Ws1 over-ceiling + two-fold variance, AR-08/10 waived; I-3 "Fix with allowlist"; disk retention RETAIN) | readouts/GATE-G32.md |`

- **LEDGER CURRENT STATE:** `lastCompleted: GATE-G32` → `DOC6.1`;
  `nextTicket: DOC6.1` → round-closure disposition per BM-TAIL-03 (the
  operator's — never inferred here); `chainTip` → this PR's branch tip;
  `updatedAt` → closeout UTC.
- **BUILD_INDEX row (proposed):**
  `| 325 | DOC6.1 | ticket | codex/harnessharness-doc6.1 | [#<PR>](https://github.com/SteveVitali/HarnessHarness/pull/<PR>) | codex/harnessharness-cap6.3 | 2026-11-19 (PR open) | (none) | lead-token reconciliation DF-R6.45l-1/R6.50-1/CAP6.1-1…5 (all dated-DONE) | engineered — reader docs + agent docs converged on GATE-G32-accepted evidence (stale 4 fixed, gaps 2 filled, 1 detector false-positive recorded; all doc checkers green both modes; stack CI read cited) | runs/DOC6.1.md · pr/DOC6.1.md · reports/round6/READER_DOCS_CHECK.md · reports/round6/AGENT_DOCS_CHECK.md |`
- **DEFERRALS lead-token normalization (proposed dated note, append-only):**
  "2026-11-19 · DOC6.1 — the six rows DF-R6.45l-1, DF-R6.50-1,
  DF-CAP6.1-1…5 carry dated DONE dispositions; lead tokens reconcile to
  their dated state at round closure (MAINTAINING.md). No row content
  changes."
- **projectStatus:** stays as GATE-G32's decision recorded — never a
  blanket DONE while BL-50 and routed later-pass scope stand (ticket
  consistency note, item 22). Left to the operator.

## Deviations / blockers

- Detector's residual "1 broken ref" is a **recorded false positive** in a
  frozen doc — AC-1's substance holds; the counter is cited verbatim, not
  gamed.
- No cargo suite run (docs-only delta, per scoped-check rule); CI's own
  jobs run on the PR.
- `#420`'s red is the I-3 repair's own leg — recorded, not re-diagnosed.
