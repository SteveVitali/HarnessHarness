# DOC.2 run — agent docs refresh (the closing act)

- **Harness:** devin-cli subagent (Cognition Devin; deterministic/CPU
  only — a docs ticket: no runtime code changed, no credentials, no
  network legs; `git`/shell writes only)
- **Ticket:** `docs/tickets/106_DOC.2__agent-docs-refresh.md`
  (manifest row 106 — docs 2 of 2, the last row of the chain)
- **Branch:** `svitali/harnessharness-doc.2` (stacked on the chain tip
  `svitali/harnessharness-doc.1` @ `13ea223`)
- **PR:** base `svitali/harnessharness-doc.1` — do not merge,
  OPERATOR policy
- **Skill invoked:** `agent-docs` refresh mode (SKILL.md followed end to
  end: Phase 0 scope+detect → refresh-mode R.1 triage → R.2 critical fix
  → R.4 coverage → R.5 severe-drift regenerate → Phase 2 shared tail:
  validate → consistency sweep → CLAUDE.md bridge → checklist → commit)

## What the detector said (Phase 0)

`agent-docs/scripts/check-agent-docs-freshness.sh .` → **1 doc scanned,
1 issue**: `AGENTS.md:18` referenced `drive-build.sh`, which does not
exist (classified `[authoring error]` — never existed at the doc's last
edit either; it is template boilerplate naming an external harness
driver, not a repo file). Zero moderate/minor issues — the coverage-gap
check finds every `crates/*/src` covered by the root doc's hierarchy.

The structural report understated the real drift: the root `AGENTS.md`
was the **28-line Build-memory template block only** — no Purpose, no
crate map, no build/test commands, no conventions. A fresh agent reading
it cold could not orient, navigate or contribute. Per Regeneration Rule
2 (>40% of the doc's required surface absent → regenerate with
bootstrap's machinery), the root doc was rewritten from verified source:
workspace membership and dep layering read off `Cargo.toml` members +
per-crate `Cargo.toml` descriptions + `src/lib.rs` headers; commands off
`.github/workflows/ci.yml`; env vars + the honesty ceiling off the
landed `README.md`/`OPERATIONAL_READINESS.md`; `tier-c*` features off
the feature stanzas (`hh-helper`/`hh-ledger`/`hh-env` c1, `hh-embed`
c4).

## What landed

- **`AGENTS.md` regenerated** (28 → 187 lines): Purpose, Architecture
  (schema source → boundary bindings → kernel spine → Lab tier →
  surfaces/plugins), Key files, Build & Test (exact commands incl.
  `check-drift.sh`/`check-removability.sh`/`check-build-memory.sh`),
  Code conventions (hermetic std-only, CC1/CC7/CC10, typed refusals,
  durable-before-visible, `tier-c*` features, per-ticket test
  batteries), Critical Gotchas (DEFERRALS-first, generated files,
  **the DF-DOC.1-1 `hh-kernel doctor` defect called out so no agent
  "fixes" docs to claim it passes**, the `edit`-tool overlay anomaly on
  build-memory files, the fixture-verified ceiling, stacked-PR rules),
  Terminology, Do/Don't — and the **Build memory section preserved
  verbatim** except the one stale line (`drive-build.sh` → the real
  consumers: `orchestrate-build` parses the ledger, `implement-spec`
  drives tickets) and the dropped template-provenance comment.
- **`crates/AGENTS.md` created** (101 lines): the full 50-crate map
  (one line each, distilled from the verified Cargo.toml descriptions),
  scoped test commands, the lib.rs-header contract convention, the
  binaries list, and plugin-side vs host-side boundary gotchas.
  Per-crate leaf docs were deliberately *not* generated: 50 near-empty
  minimal files would add maintenance surface without orientation value
  — the map covers every crate at the `crates/` level (nearest-file-wins
  gives every `crates/hh-*/src` this doc).
- **`CLAUDE.md` created** — the Phase-2.3 bridge: `@AGENTS.md`.
- **`docs/tickets/DEFERRALS.md`** — DF-DOC.1-1 progress note appended
  (append-only): stays **OPEN** as backlog; no `BL-` id exists because
  `BACKLOG.csv` was minted at REC.1, before the row was opened — the
  next `reconcile-build mode=backlog` run assigns it. Compensating
  control unchanged (`hh doctor` boundary test; the defect fails
  loudly).

## Verification

- Detector re-run (Phase 2.1): **0 issues / 2 docs scanned** — the AC
  holds (`0 critical` is the ticket's deterministic AC).
- `check-build-memory.sh .` → **0 violations** (7 pre-existing
  warnings, unchanged).
- `grep -n '^||' docs/tickets/DEFERRALS.md` → empty.
- Crate-map completeness verified by diff: all 50 `crates/hh-*`
  directories appear in `crates/AGENTS.md`; every backticked path in
  both docs was `ls`-verified.
- `cargo test --workspace` **not run** — docs ticket, no runtime code
  changed.
- Write-integrity: all writes via python heredocs; `git diff` /
  `git show --stat` verified per commit.

## DONE preconditions (verified before `projectStatus: DONE`)

- Manifest chain: 108 rows = 106 numbered + inserted 3a + 59a. Kind
  split: 102 tickets/capstone/reconcile/docs rows, 4 gates
  (`004_GATE-G1`, `060_GATE-G2`, `091_GATE-G3`, `101_GATE-ACCEPT`), 2
  human rows (`061_HUMAN-H1`, `082_HUMAN-H2`).
- BUILD_INDEX after this row: 102 rows — every non-gate/non-human
  manifest row has a landed index row (seq 1–103 with gate/human seqs
  absent by design; `3a`/`59a` present).
- Gates: all four dispositioned **PASSED** in `LEDGER.md` GATE
  DECISIONS (G1 reading 2 post-S0.3b; G2 reading 2 post-S3.12b; G3;
  GATE-ACCEPT reading 1 — which recorded "`projectStatus: DONE`
  unblocked pending REC.1–3 + DOC.1–2").
- Human rows: carried as signed accepted rows (BL-03/BL-04,
  "GATE-ACCEPT signed"), per the manifest's HUMAN-row rule — not
  blockers.
- No undispatched `nextTicket`: the manifest ends at row 106 = this
  ticket.

## Evidence report

Deterministic ACs: freshness detector 0 critical (post-run re-run: 0
issues total); "Build memory" section present in a marked repo (the
`docs/build/README.md` marker is present and the section is carried).
Universal phase-gate AC: docs-only — no new behaviour, so nothing
needing a runtime test; the one unverifiable-in-build item (the
`hh-kernel doctor` defect) is already a DEFERRALS row (DF-DOC.1-1) and
is now also a documented Critical Gotcha. Requirement ids: the spec's
docs-scope item is **R-2.12.4** — `deferred(ADR-0210)` to WS-L6, out of
build scope; named for the record in the PR, not discharged. No ADR
owed — no decision owned (hierarchy design follows the skill's rules).
BUILD_INDEX row 103 + LEDGER (`projectStatus: DONE`,
`nextTicket: DONE`, `lastCompleted: DOC.2`,
`chainTip: svitali/harnessharness-doc.2`) advanced in the closeout
commit.
