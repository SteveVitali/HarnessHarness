# READER_DOCS_CHECK — DOC6.1 stage 1 (DOC.1)

Record class: ticket artifact · authored 2026-11-19 (UTC) · branch
`codex/harnessharness-doc6.1` · stage commit `451cc07e` (reader docs) ·
offline, USD $0.00. Stage 1 invoked `refresh-repo-docs`
(`~/.claude/skills/refresh-repo-docs`, audit=incremental, scope `.`,
apply=true) and followed it completely under `docs/README.md`'s mode
table and `docs/AGENTS.md`'s check scoping.

## Detector (phase 0 → phase 3)

`bash ~/.claude/skills/refresh-repo-docs/scripts/check-repo-docs-freshness.sh .`

- **Before:** docsScanned 1202 · broken refs **1** · stale suspects 38
  (`/tmp/repo-docs-freshness.json`, worklist ledger preserved at
  `/tmp/repo-docs-findings.md`).
- **After:** docsScanned 1202 · broken refs **1** · stale suspects 40.
- **The one "broken ref" is a verified false positive, not a broken
  reference.** `docs/design/23_parity-method-review.md` → `1−p`: the
  detector's `](target)` extraction reads the math/code expression
  `old[k](1−p)` (line 202) as a markdown link. The doc is **frozen**
  (accepted design note; mode table) and may not be edited to satisfy a
  parser — the finding is recorded here, verbatim, and the reference
  itself does not exist to break. AC-1's substance — no *real* broken
  references — holds; the deterministic counter is recorded honestly
  rather than gamed.
- Remaining stale suspects are report-only by mode: `docs/design/*` and
  `docs/research/*` (frozen accepted notes — drift of design-time prose
  is expected and preserved), `docs/tickets/*` (historical contracts),
  plus audited-and-current living docs whose heuristic flag survives
  only because the flag compares git dates (CONTRIBUTING, GOVERNANCE,
  getting-started, foreign READMEs — each claim verified current; see
  below).

## Findings → dispositions (every changed claim verified)

| Doc | Finding | Disposition |
|---|---|---|
| `docs/results/README.md` | publication table omitted the superseding HUMAN-H17 `-flip` records; footer said "the dataset stays private (HUMAN-H17 pending)" | **fixed** — two flip rows added citing `phase-{3,4}-2026-11-19-flip.json` (`dataset.visibility: public`, `dataset_flip: done{human_h17:2026-11-19T04:23Z}`); footer now: dataset `SteveVitali/harnessharness-archive` **public** at `a11591e931442908c34a5bc7a65e8e664f4752d7`, sanitized derivatives only, transcripts "(b) links only"; phase 2 stays publication-gate pending |
| `docs/evidence.md` | `[results-site]` card: "the dataset stays private (…HUMAN-H17 pending)"; `[hir-parity]`/`[config-sweep]` cards cite superseded base records | **fixed** — all three cards cite the superseding `-flip` records; Access now records the public pinned revision, sanitized-only scope and links-only transcripts; card states unchanged (`Available — internal record; not outside-verified`), no outcome promoted |
| `docs/status.md` | gap — no Round-6 account (last update 2026-11-07 R6.3) | **filled** — dated Round-6 update: GATE-G32 PASSED accepted-as-recorded (410/456 closed+satisfied; 46 PARTIAL verbatim); spend $31.887214 ≤ $100 ($4.52 billed + $27,367,214 µ$ estimated); every phase-3/4 outcome `withheld{…}` verbatim; dataset public; REQ-XC-325 waived-not-executed; routed scope stays owed; #414–#418+ red span and I-3 carried |
| `README.md` | spine: site "renders the same pinned record" (phase-1 only) | **fixed** — "renders that pinned record and the first-pass phase-3/4 pages"; spine pin structure preserved (one SITE_URL, labels intact) |
| `docs/reference.md` | binaries table omitted the landed round-6 binaries | **filled** — `hh-runner` (hharness-runner), `hh-model-proxy` (hharness-hosting), `hh-cell-relay` (hharness-helper) — each `[[bin]]` name verified in its Cargo.toml |
| `docs/architecture.md` | Lab section silent on the committed experiment apparatus (R6.8–R6.16) | **filled** — one proportionate sentence: `hh-runner` slot workers, `hh-model-proxy`, `hh-cell-relay` allowlist-mediated egress |
| `docs/getting-started.md` | flagged vs `rust-toolchain.toml` | **verified, no change** — "Rust 1.94 and `rustfmt`" still exact (the added musl target is not contradicted) |
| `CONTRIBUTING.md` | flagged vs `rust-toolchain.toml` | **verified, no change** — same toolchain claim; lanes unchanged |
| `GOVERNANCE.md` | flagged vs `CODEOWNERS` | **verified, no change** — CODEOWNERS exists; claim accurate |
| `CHANGELOG.md` | not flagged | **verified, no change** — per-release file; v0.1.0 is the only release; round-6 `RELEASE_NOTES.md` is a committed *draft*, not a changelog source — no fabricated entry |
| `foreign/e2-python/README.md`, `foreign/e3-typescript/README.md` | flagged vs `answers.json` | **verified, no change** — "eight arms and 57 checked answers" matches the comparator's `verify: 57 passed` contract (66 answer members incl. informational pins) |
| `docs/design/*`, `docs/research/*`, `docs/tickets/*` | stale-suspect flags | **report-only** — frozen/historical modes; surfaced, never rewritten |

Counts: stale **4** fixed · cruft **0** removed · gaps **2** filled ·
verified-no-change **6** docs · report-only **24** docs · remaining **1**
detector false positive (above).

## Doc checkers (working tree, then `--head` on `451cc07e`)

| Command | Result |
|---|---|
| `bash scripts/check-living-docs.sh` | OK — 554 living files scanned, no personal paths |
| `bash scripts/check-position.sh` | OK — 14 leg(s) green [working tree]; live: `skipped(no routine PR calls — ADR-0363 …)` |
| `bash scripts/check-position.sh --head` | OK — 14 leg(s) green [committed HEAD `451cc07e`, clean clone] |
| `bash scripts/check-examples.sh` | OK — 4 cards, 6 pins recomputed, 29 runner check ids, displayed block equivalent [working tree] |
| `bash scripts/check-examples.sh --head` | OK — same, committed HEAD `451cc07e` |
| `bash scripts/check-trace.sh` | OK — all legs green [working tree] |
| `bash scripts/check-trace.sh --head` | OK — all legs green, committed HEAD `451cc07e` |
| `bash scripts/check-contributing.sh` | OK — 12 check ids green [working tree] |
| `bash scripts/check-contributing.sh --head` | OK — 12 check ids green, committed HEAD `451cc07e` |
| `bash scripts/check-site.sh --profile internal` | OK — 15 leg(s) green [working tree] |
| `bash scripts/check-site.sh --profile public` | OK — 15 leg(s) green [working tree] |
| `bash scripts/run-examples.sh` | offline cards EX-01–04 green (diagnostics 5,673 B ≤ 50 MiB; scratch ≤ 1 GiB); live: `skipped(no routine PR calls — ADR-0363)` |
| `bash scripts/check-quickstart.sh` | extracted 18 verbatim lines; clone `451cc07e…` → cargo build → doctor `connectivity:ok` → run finished `succeeded/completed` |
| `python3 scripts/site/build.py --check` | site-data drift check clean |

Every changed claim was verified against current code or a committed
record (GATE-G31/GATE-G32/HUMAN-H17 readouts, the `-flip` publication
records, per-crate `[[bin]]` entries, `COVERAGE_MATRIX.csv` sums as
folded by CAP6.2). First-pass results appear only with their
REQ-XH-119 labels; no card exceeds "Available — internal record; not
outside-verified"; the claims lexicon leg of `check-position.sh`
(banned-phrase scan over the record, `evidence.md`, and the spine) is
green on both passes.

## Stack CI read (BM-CI-01, AC-4)

`ci-boundary.sh --stack --no-wait` reads committed under
`docs/build/reports/round6/` (`ci-stack-doc6.1-2026-11-19.json`,
`ci-410-doc6.1-2026-11-19.json`, `ci-413-doc6.1-2026-11-19.json`,
`ci-420-doc6.1-2026-11-19.json`, all `read_at` 2026-11-19T07:15Z):

- Stack walk from #419: **#407–#412 pass**; **#414–#419 FAIL** on
  `build · test · fmt · drift` (ubuntu+macos) — the recorded
  check-ci-gating `docs/build/**` reader-literal leg; cargo-deny +
  gitleaks pass on every PR. Consistent with GATE-G32's diagnosis and
  the I-3 ruling; #419 (CAP6.3) is red on the same leg.
- Out-of-band: **#410 pass** @`11a6524`; **#413 pass** @`ae72242`;
  **#420 (I-3 allowlist repair) FAIL** on the same leg family —
  the repair is in-flight, its red is recorded, not re-diagnosed here.
- DOC6.1's own PR has no CI read at authoring time (created after this
  record); it is read at the PR boundary below.
