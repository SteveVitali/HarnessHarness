# DOC6.1 — Round-6 docs refresh: reader docs, then agent docs (manifest 325)

Requirements: none owned; contributing wording cites REQ-XF-136,
REQ-XH-019, REQ-XH-119, REQ-XD-092 (never re-owned). Base:
`codex/harnessharness-cap6.3` (the signed GATE-G32 tip; PR #419 stack).
Offline, USD $0.00 — no merges, no secrets, no production mutations.
Final Round-6 row; GATE-G32 PASSED and signed upstream.

## What this lands

**Stage 1 — `refresh-repo-docs`** (commit `451cc07e`; full record
`reports/round6/READER_DOCS_CHECK.md`):

- `docs/evidence.md`, `docs/results/README.md` — cite the superseding
  `phase-{3,4}-2026-11-19-flip.json` publication records; the dataset is
  **public** at `a11591e9…` (HUMAN-H17, 2026-11-19T04:23Z), sanitized
  derivatives only, transcripts "(b) links only". Card states unchanged:
  "internal record; not outside-verified".
- `docs/status.md` — dated Round-6 update: GATE-G32 accepted-as-recorded
  (410/456 closed+satisfied; 46 PARTIAL verbatim), spend $31.887214 ≤
  $100 ($4.52 billed + $27,367,214 µ$ *estimated*), every phase-3/4
  outcome `withheld{…}` verbatim, REQ-XC-325 waived-not-executed, routed
  scope stays owed.
- `README.md` spine — site renders phase-1 **and** first-pass phase-3/4.
- `docs/reference.md` — `hh-runner` / `hh-model-proxy` / `hh-cell-relay`
  binaries (each `[[bin]]` verified).
- `docs/architecture.md` — one-line experiment-apparatus note.
- Detector: 1 residual "broken ref" = **verified false positive**
  (`old[k](1−p)` math in frozen `docs/design/23…`); 24 report-only docs
  (frozen/historical) surfaced, never rewritten. Claims verified per
  ledger: stale 4 · gaps 2 · verified-no-change 6.

**Stage 2 — `agent-docs` refresh** (commit `361f46a6`; record
`reports/round6/AGENT_DOCS_CHECK.md`):

- Detector **0 issues** before and after (5 docs).
- Root `AGENTS.md`: experiment-apparatus + results-site architecture
  rows; Build memory Round-7 entry points (`LATER_PASS_ROUTING.md`,
  `2026-11-19_round7-decision-memo.md`).
- `crates/AGENTS.md`: `hh-cell-relay`/`hh-model-proxy` binary notes;
  `docs/AGENTS.md`: `check-site.sh`/`check-publication.sh` surface.

## Verification

check-living-docs OK (554) · check-position **14/14 both modes** ·
check-examples OK both · check-trace OK both · check-contributing **12/12
both** · check-site internal+public **15/15 each** · run-examples
EX-01–04 green · check-quickstart green on `451cc07e` · site-data drift
clean · agent-docs 0 issues · claims lexicon green. Live legs:
`skipped(no routine PR calls — ADR-0363)` verbatim.

## Stack CI (BM-CI-01, `read_at` 2026-11-19T07:15Z)

`ci-boundary.sh --stack --no-wait` (records committed under
`reports/round6/`): #407–#412 pass; #414–#419 FAIL on the diagnosed
check-ci-gating literal leg (the I-3 repair, #420, rides its own PR and
is itself still red on that leg — recorded, not greened). #410, #413
pass out-of-band. This PR's own CI read lands at its boundary.

*Docs-only delta. Nothing here promotes a withheld outcome, merges a PR,
or discharges an operator act. `mergePolicy: OPERATOR` (XO-04).*
