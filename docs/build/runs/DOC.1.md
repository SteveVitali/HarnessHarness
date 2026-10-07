# DOC.1 run — repo docs refresh

- **Harness:** devin-cli subagent (Cognition Devin; deterministic/CPU
  only — a docs ticket: no runtime code changed, no credentials, no
  network legs; `git`/shell writes only, build+run of `hh`/`hh-kernel`
  for claim verification)
- **Ticket:** `docs/tickets/105_DOC.1__repo-docs-refresh.md`
  (manifest row 105 — docs 1 of 2, sequence 105 of 106)
- **Branch:** `svitali/harnessharness-doc.1` (stacked on the chain tip
  `svitali/harnessharness-rec.3` @ `1bedc05`)
- **PR:** base `svitali/harnessharness-rec.3` — do not merge,
  OPERATOR policy
- **Skill invoked:** `refresh-repo-docs` (SKILL.md followed end to end:
  Phase 0 detect → Phase 1 audit → Phase 2 fix → Phase 3 re-detect +
  consistency sweep → Phase 4 report/commit)

## State correction vs the ticket brief

The brief asserted the README/examples "were written at decomposition
time against the planned build" and went stale over 99 tickets. The live
repo tells a different story: **no root `README.md` ever existed**
(`git log --all -- README.md` is empty), **no `examples/`, `CHANGELOG`
or `CONTRIBUTING` ever existed**, and no per-crate READMEs exist. The
doc corpus is thin by design — the build's records live under
`docs/build/` (historical) and `docs/tickets/` (historical), the program
corpus (`docs/1_*`, `docs/2_*`, `docs/3_*`, `spec/`, `research/`) is
frozen. The refresh therefore centered on the genuinely living surface.

## What the detector said

`refresh-repo-docs/scripts/check-repo-docs-freshness.sh .` → **706 docs
scanned, 0 broken references, 0 stale suspects**; re-run after the fixes:
707 scanned, **0 broken refs** (the AC holds). Worklist from the
deterministic signals was empty because docs and code have committed
together all along; the ticket's deliverable required a claim-level
audit of the living surface regardless. Findings ledger (working file):
`target/doc1/repo-docs-findings.md` (per the standing rule, logs under
`target/` not `/tmp`; the detector's own JSON went to the script's
`/tmp` default, mirrored at `target/doc1/repo-docs-freshness.json`).

## Mode table honored

`docs/README.md` applied: `docs/adr/README.md` is generated (untouched —
no ADR changed, so `adr-index.sh` not re-run); `spec/`, `research/`,
`docs/1–3`, `docs/tickets/`, `docs/build/` are frozen/historical/
append-only → **report-only**, none edited; `AGENTS.md` is agent-facing →
DOC.2's scope, noted not touched.

## What landed (fixes)

- **`README.md` (new — gap, critical).** The repo's front door did not
  exist. Written claim-by-claim verified: the 50-crate workspace
  (`Cargo.toml` `members`, `exclude=["spikes"]`, std-only runtime with
  the single `tempfile` dev-dep of `hh-varhost`); Rust 1.94
  (`rust-toolchain.toml`); the CI battery (`.github/workflows/ci.yml`:
  fmt/build/test/drift); all 16 binaries from `[["bin"]]` stanzas; the
  `hh` noun surface read off `crates/hh-cli/src/cli.rs` dispatch; the
  three bindings (in-process (a) / stdio `hh-kernel serve` (b) /
  `serve --http` loopback (c)); `HH_KERNEL_CMD`/`HH_STORE_ROOT`/
  `HH_WORKSPACE_ROOT` defaults from `hh-kernel/src/serve.rs`; exemplars
  = `hh-lab::exemplars` (`lab/compaction-family-v1`,
  `lab/control-strategy-family-v1`); fixture corpus
  `hh-bench/fixtures/benchset/stage3_v1`; the **fixture-verified**
  honesty ceiling per `OPERATIONAL_READINESS.md` (no staging/live
  claims; tracker = out-of-process `hh-tracker-fixture` under
  `hh-varhost`, HUMAN-H2 owed; MCP SSE push leg owed per DF-S4.11-1).
- **`docs/README.md` (stale, moderate).** The mode-map's top-level
  table named template-seeded entries that never existed under `docs/`
  (`brief.md`, `research-ledger.md`, `research/`, `design/`,
  `decomposition-prompt.md`). Rewritten to the real corpus (docs 1–3
  frozen, adr/ append-only + generated index, tickets/ historical,
  build/ historical, this map living) plus an "Adjacent doc surfaces"
  table for the root-level surfaces (`README.md`, `AGENTS.md`, `spec/`
  living→frozen, `research/` frozen, `spikes/README.md` living).
- **`spikes/README.md` (gap+stale, minor).** The committed
  `s0.3b-online-spike/` tree had no index bullet; "runs both spikes" was
  stale (three trees). Added the verified bullet (G1 byte-identity,
  M-S1-7 SDK round-trips, E5b/E5c splits, DF-S0.3-1/-3 machine cells)
  and the runner correction (s0.3b has `run-sheet.py`/`test-gates.sh`).
- **`Cargo.toml` header comment (stale, minor).** "pure standard
  library, no external crates" → now names the single `tempfile`
  dev-dependency (Cargo.lock: 13 non-`hh-*` packages, all transitive of
  it). Comment-only; no manifest semantics touched.
- **`.gitignore` += `.hh/`** — the `HH_STORE_ROOT` default
  (`<cwd>/.hh/store`) created by running `hh doctor`; local runtime
  state should never be committed.
- **`docs/tickets/DEFERRALS.md` += DF-DOC.1-1 (OPEN, F)** — code
  suspect found by claim verification, deferred because DOC.1 is
  docs-only.

## Code suspect (reported, not fixed — docs ticket)

`hh-kernel doctor` exits 1 with `identity mismatch` on a clean build.
`crates/hh-kernel/src/main.rs::doctor` compares the hello result's
`kernel.version` to the full `kernel_version_id` (`hh-kernel/0.0.1`),
but `EmbedService` stores only the `rsplit('/')` semver tail
(`"0.0.1"`) as `kernel_version` (needed so `negotiate`'s `kernel_floor`
semver parse works), so `KernelDescriptor.version` reports `"0.0.1"`
and the check can never pass. No test exercises the binary doctor path
(the tested doctor is the CLI's over the boundary —
`version_and_doctor_answer_hello_facts`). Filed as DF-DOC.1-1 with the
reproduction + the `hh doctor` proxy evidence.

## Verification

- Detector re-run: **0 broken refs** / 707 docs (deterministic AC).
- `cargo build -p hh-cli -p hh-kernel` → green;
  `HH_KERNEL_CMD=./target/debug/hh-kernel ./target/debug/hh doctor` →
  `connectivity: ok` over the spawned binding-(b) child (the README's
  quickstart claim verified by running it).
- `check-build-memory.sh .` → **0 violations** (7 pre-existing
  warnings, unchanged from REC.3).
- `grep -n '^||' docs/tickets/DEFERRALS.md` → empty (one well-formed
  row appended).
- `cargo test --workspace` **not run** — docs ticket, no runtime code
  changed (the only non-doc file touched is a `Cargo.toml` *comment*).
- Write-integrity: all writes via shell/python heredocs (the `edit`
  tool is unreliable on this host); `git status`/`git show --stat`
  verified after every commit.

## Not done (status discipline)

- No DEFERRALS row closed — none were docs-scoped (the OPEN rows are
  foreign-toolchain/SSE/hosted-surface items; one new row opened).
- No ADR written — no decision was owned (the doctor finding is a
  defect report, not a ruling; the spec stays untouched — REC.2's
  amendments remain proposed, unapplied).
- `docs/adr/README.md` not regenerated — the ADR set did not change.
- `projectStatus` stays `IN_PROGRESS` — DONE comes after DOC.2 lands.

## Evidence report

Deterministic AC: `check-repo-docs-freshness.sh` reports **0 broken
references** after the run (707 docs scanned; the new README's cited
paths all resolve). Agentic ACs: every claim written was verified
against code — workspace membership + `exclude` (Cargo.toml), the
`hh`/`hh-kernel`/`hh-web` surfaces (cli.rs/main.rs dispatch tables),
env-var defaults (hh-kernel/src/serve.rs), the binary list ([[bin]]
stanzas), the honesty ceiling (OPERATIONAL_READINESS.md) — and the
quickstart was executed (`hh doctor` → connectivity ok). Requirement
ids: the spec's docs-scope item is R-2.12.4 — `deferred(ADR-0210)` to
WS-L6, out of build scope; named in the PR for the record, nothing
discharged. The one thing not automatically verifiable found during
the run — the `hh-kernel doctor` self-check defect — is filed as
DEFERRALS row DF-DOC.1-1 (OPEN) with the `hh doctor` boundary test as
its compensating control. No ADR owed (no decision owned; the defect
report is not a ruling). BUILD_INDEX row 102 + LEDGER advanced in the
closeout commit.
