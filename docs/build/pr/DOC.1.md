# DOC.1 — repo docs refresh

**Stacked on:** `svitali/harnessharness-rec.3` (PR #102) · **Do not merge — operator-controlled**

## Summary

`refresh-repo-docs` run against the composed build (sequence 105/106).
**State correction vs the ticket brief:** no root `README.md`,
`examples/`, `CHANGELOG` or `CONTRIBUTING` ever existed — the doc corpus
is thin by design, not stale-by-accident. The detector reports **0
broken refs / 0 stale suspects** over 707 docs; the real work was a
claim-level audit of the living surface and the one critical gap: the
repo's front door.

- **`README.md` created** — the verified map of the landed system:
  50-crate pure-std workspace (one `tempfile` dev-dep), Rust 1.94, the
  16 binaries, the `hh` noun surface (read off `cli.rs` dispatch), the
  three `hh-embed/1` bindings, env-var defaults, exemplars
  (`hh-lab::exemplars`), the benchset fixture, and the honest
  **fixture-verified** ceiling (no staging/live claims — cites
  `OPERATIONAL_READINESS.md`, `DEFERRALS.md`, `BACKLOG.csv`).
- **`docs/README.md` fixed** — the mode map named five entries that
  never existed under `docs/`; rewritten to the real corpus plus an
  adjacent-surfaces table for the root-level docs.
- **`spikes/README.md` fixed** — the committed `s0.3b-online-spike/`
  tree indexed; the stale "both spikes" runner line corrected.
- **`Cargo.toml` comment fixed** — "no external crates" → names the one
  `tempfile` dev-dep (comment only, no semantics).
- **`.gitignore` += `.hh/`** — the `HH_STORE_ROOT` default must not be
  committed.
- **`DEFERRALS.md` += DF-DOC.1-1 (OPEN)** — code suspect: `hh-kernel
  doctor` exits 1 (`identity mismatch`) on a clean build — the
  self-check compares `kernel.version` (`"0.0.1"`, the `rsplit('/')`
  semver tail `EmbedService` stores for `kernel_floor` parsing) against
  the full `hh-kernel/0.0.1` id; the binary path is untested. Reported,
  not fixed — docs ticket.

Requirement ids: the spec's docs-scope item is **R-2.12.4**
(packaging/licensing/OSS governance/docs/community) — it is
`deferred(ADR-0210)` to WS-L6 and **out of build scope** (manifest
cross-cutting notes), so no `R-2.*` id is discharged by this ticket;
R-2.12.4 is named for the record, not stamped as satisfied.

## Modes honored

Historical/frozen/append-only surfaces were **report-only**:
`docs/build/`, `docs/tickets/` (except the one appended DEFERRALS row),
`spec/`, `research/`, docs 1–3. `docs/adr/README.md` is generated —
untouched (no ADR changed). `AGENTS.md` is DOC.2's scope.

## Verification

- `check-repo-docs-freshness.sh .` → 0 broken refs / 707 docs (re-run
  post-fix; the ticket's deterministic AC).
- `cargo build -p hh-cli -p hh-kernel` green; the README quickstart
  (`hh doctor` over the spawned `hh-kernel serve` child) executed →
  `connectivity: ok`.
- `check-build-memory.sh` → 0 violations (7 pre-existing warnings).
- `cargo test --workspace` not run — no runtime code changed.
- All writes via shell/python; `git show --stat` verified per commit.

## Remaining gaps (reported, not fabricated)

- No `CHANGELOG`: `docs/build/BUILD_INDEX.md` is this build's change
  record (one row per landed ticket + PR).
- No `examples/`: the canonical exemplar documents are
  `crates/hh-lab/src/exemplars.rs`; runnable fixtures live in
  `crates/hh-bench/fixtures/benchset/stage3_v1` and the per-crate
  `tests/` trees.
- DF-DOC.1-1 (`hh-kernel doctor` self-check) needs a code-touching
  ticket.

## For the operator

Docs-only PR; safe to land anywhere in the stack order after its base.
DOC.2 (agent-docs refresh) follows on `svitali/harnessharness-doc.1`.
