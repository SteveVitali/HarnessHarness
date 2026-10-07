# AGENTS.md — HarnessHarness

## Purpose

The E1 reference build of the MetaHarness program: a kernel + Laboratory for
*harness engineering* — durable, observable, model-conditioned runtimes for
goal-directed agents, with provenance, budgets and governance carried as
load-bearing contracts rather than conventions. The system is a 50-crate
pure-std Rust workspace plus committed contract, spec and build records.
Human-facing overview: `README.md`. The behavioural contract:
`spec/CANONICAL_SPEC.md` (frozen — amend via an ADR + the manifest protocol).

## Architecture

- **One schema source.** `crates/hh-embed-schema` defines the `hh-embed/1`
  boundary; `hh-codegen` emits the checked-in artifacts
  (`schema/hh-embed-1.schema.json`, `schema/plugin-abi-1.schema.json`,
  `schema/compat-1.matrix.json`,
  `crates/hh-embed-client-generated/src/generated.rs`). Drift is a build
  failure (CC7).
- **One boundary, three bindings.** Every `hh-embed/1` op dispatches through
  one `handle` path: (a) the in-process `EmbedService`, (b) stdio JSON-RPC 2.0
  (`hh-kernel serve`), (c) loopback HTTP (`hh-kernel serve --http`). The `hh`
  CLI (`hh-cli`) is a generated client over the boundary — no kernel semantics
  (K-2).
- **Kernel spine (deps point downward only).** `hh-wire` (canonical JSON /
  JSON-RPC framing / SHA-2) → `hh-ontology` + `hh-identity` + `hh-provenance`
  (the formal model, idp/1 identity, the label lattice) → `hh-hir` (the HIR/1
  IR: validate/canonicalize/seal/diff) → `hh-ledger` (the append-only
  hash-chained event store — the sole authoritative record of a run). The
  Stage-1 slices sit beside it: `hh-budget`, `hh-registry`, `hh-secrets`,
  `hh-telemetry`, `hh-monitor` (the Π reference monitor), `hh-containment`,
  `hh-env` (execution environments + sandboxed exec), `hh-gateway` (model
  boundary), `hh-context`, `hh-control`, `hh-verification`; then
  `hh-assembly`/`hh-compiler` (assembly grammar → `RuntimePlan/1`),
  `hh-embed` (the boundary service) and the binaries `hh-kernel`, `hh-helper`.
- **Lab tier (records-in/records-out).** `hh-lab` (schemas + exemplars) →
  `hh-experiment`/`hh-results`/`hh-analysis`/`hh-eval`/`hh-bench` →
  `hh-hosting` (the hosted-participant ABI) → the C3–C5 services
  `hh-subagent`, `hh-orchestrator`, `hh-fleet` (+ `hh-fleet-adapter`),
  `hh-evolution`, `hh-debt`.
- **Surfaces & plugins.** `hh-cli` (`hh`), `hh-web`, `hh-mcp`/`hh-mcp-lab`,
  `hh-acp`; packaged variants under `plugins/` run out of process through
  `hh-varhost` + `hh-helper` over `plugin_abi/1`.

Full per-crate map and workspace rules: `crates/AGENTS.md`.

## Key files

| Path | What |
|---|---|
| `Cargo.toml` | workspace `members` — annotated one-line-per-crate; the hermetic-deps note |
| `rust-toolchain.toml` | pins Rust 1.94 (+ `rustfmt`); CI honors it |
| `crates/` | the 50-member workspace — see `crates/AGENTS.md` |
| `plugins/` | packaged first-party variants: `hh-compact-evict-oldest`, `hh-memory-store`, `hh-tracker-fixture` |
| `schema/` | generated contract artifacts — regenerate via `scripts/check-drift.sh`, never hand-edit |
| `scripts/` | `check-drift.sh` (CC7), `check-removability.sh` (CC6), spike runners |
| `spec/` | `CANONICAL_SPEC.md` + `sections/` sources + `READINESS_REPORT.md` |
| `docs/` | the docs corpus — `docs/adr/` (build ADRs), `docs/tickets/` (chain + `DEFERRALS.md`), `docs/build/` (build memory); mode table in `docs/README.md` |
| `research/` | the program ledger + research ADRs — frozen at the readiness gate |
| `spikes/` | committed-but-throwaway measurement trees; excluded from the workspace (`exclude = ["spikes"]`) |
| `.github/workflows/ci.yml` | fmt + build + test + drift |

## Build & Test

```sh
cargo build --workspace               # all crates + binaries
cargo build --workspace --all-targets # what CI builds
cargo test --workspace                # the full suite (~3k tests)
cargo test -p hh-<crate>              # one crate's tests
cargo fmt --all --check               # CI gate
cargo clippy -p hh-<crate> -- -D warnings   # convention: clean on touched crates
bash scripts/check-drift.sh           # regenerate schema artifacts; fail on drift
bash scripts/check-removability.sh    # CC6 tier-removability gate
bash <skills>/build-memory/scripts/check-build-memory.sh .   # build-memory hygiene
```

Smoke: `cargo build -p hh-cli -p hh-kernel` then
`HH_KERNEL_CMD=./target/debug/hh-kernel ./target/debug/hh doctor` →
`connectivity: ok`. Env vars: `HH_KERNEL_CMD` (kernel child command),
`HH_STORE_ROOT` (default `<cwd>/.hh/store` — gitignored),
`HH_WORKSPACE_ROOT` (default cwd).

## Code conventions

- **Pure std, hermetic.** No external runtime crates; the single exception is
  the `tempfile` dev-dep of `hh-varhost` tests. Do not add dependencies.
- **CC1 — one canonical implementation.** Reuse the existing codec, store,
  canonicalizer or class table; a second spelling of a contract is a defect.
- **CC7 — the schema source owns both directions.** Change `hh-embed-schema`
  and regenerate; never hand-edit generated artifacts.
- **CC10 — home-plane ownership.** A record/event kind lives in exactly one
  plane's home table (`hh-ontology` owns the rule).
- **Honest refusal, never silent degrade.** A missing capability answers a
  typed refusal (`Unsupported`, `Refused`, `n/a{reason}`) — never a
  fabricated result or a silent zero.
- **Durable-before-visible.** Kernel state changes append through the run's
  single fenced writer; projections are pure folds over the durable prefix.
- **Feature tiers = spec maturity.** C1+ halves sit behind `tier-c*` features
  (`hh-helper`/`hh-ledger`/`hh-env`: `tier-c1`; `hh-embed`: `tier-c4` →
  `hh-fleet`/`hh-evolution`/`hh-debt`); an absent tier answers `unsupported`.
- **Tests** are acceptance batteries named per ticket:
  `crates/hh-*/tests/<ticket>.rs` (e.g. `crates/hh-embed/tests/cap_2_composed.rs`).
  Every new behaviour gets a test that fails if removed; `ignore`d tests cite
  a `DF-*` deferral row.

## Critical gotchas

1. **`docs/tickets/DEFERRALS.md` — read it first, every run.** Owed work lives
   there; if your change unblocks an `OPEN` row, closing it is part of the
   run. A deferral not in the file did not happen. (See Build memory below.)
2. **Generated files are never hand-edited:** `schema/*.json`,
   `crates/hh-embed-client-generated/src/generated.rs`, `docs/adr/README.md`
   (regenerate: `build-memory adr-index docs/adr`).
3. **`hh-kernel doctor` exits 1 (`identity mismatch`) on a clean build** —
   known defect DF-DOC.1-1: the binary self-check compares the semver tail it
   stores as `kernel.version` against the full `hh-kernel/<ver>` id. The
   working self-check is `hh doctor` over the boundary. Do not "fix" docs to
   claim the binary doctor passes.
4. **The `edit` tool drops writes on build-memory files** (overlay anomaly):
   write `LEDGER.md`/`DEFERRALS.md`/`BUILD_INDEX.md` via shell/python and
   verify with `git diff` before committing.
5. **Offline/hermetic by design.** The verified ceiling is fixture-verified —
   never write `staging-verified`, `live-executed` or `public` claims
   (`docs/build/OPERATIONAL_READINESS.md` is the capability account).
6. **The manifest chain is a stack, not a queue:** work forks from the
   previous ticket's branch; append-only history; a gate is a pause, not a
   block; secrets never enter a ledger (`provided: yes/no` only).

## Terminology

| Term | Meaning |
|---|---|
| kernel / E1 | the reference runtime — winner of the Stage-0 ecosystem decision (ADR-0050) |
| Lab | the experiment/evaluation plane (spec §6) |
| `hh-embed/1` | the one typed kernel boundary every surface speaks |
| binding (a)/(b)/(c) | in-process / stdio JSON-RPC / loopback HTTP |
| Π | the reference monitor's authorization interpreter (`hh-monitor`) |
| run ledger | the append-only hash-chained event store (`hh-ledger`) |
| HIR/1 | the Harness IR dialect (`hh-hir`) |
| DF-* | a `docs/tickets/DEFERRALS.md` row — owed work with a compensating control |
| BL-* | a `docs/build/BACKLOG.csv` normalized-debt row |
| gate | a manifest STOP row — a pause for operator readout, not a block |
| C0…C5, `tier-c*` | spec maturity tiers; mapped to cargo features |
| `n/a{reason}` | the typed not-applicable lattice — claims carry their reason |
| fixture-verified | the build's honesty ceiling: exercised end-to-end against fixtures, never a live service |

## Build memory

This repo uses committed build memory (agent-skills `build-memory` v2). If you are running a
build ticket, read these first:

- **`docs/tickets/DEFERRALS.md` — read it first, every run.** It is the append-only ledger of
  owed work. If your ticket (or a landed prerequisite) unblocks an `OPEN` row, closing it is
  part of your run. Never delete a row; a deferral not in the file did not happen. *(Critical
  Gotcha: skipping this is how obligations silently vanish between sessions.)*
- **`docs/tickets/00_MANIFEST.md`** — the ticket chain: the authoritative order, the four
  build rules, the requirement-ID index. A human can drive the chain from it alone.
- **`docs/build/LEDGER.md`** — the machine state (`nextTicket`, `chainTip`, gate decisions).
  `orchestrate-build` parses it; the chain is driven by `implement-spec` per ticket (or by
  hand from the manifest table).
- **`docs/build/`** — the record of what happened: `runs/<ID>.md` (per-ticket run ledgers),
  `pr/<ID>.md` (PR bodies), `BUILD_INDEX.md` (one row per landed ticket), `readouts/` (gates).
- **`docs/adr/`** — decision records; `README.md` is generated (`build-memory adr-index`) —
  never hand-edit it.

Rules that hold across the build: contracts and history are **append-only** (amend by adding a
dated note or a new ADR, never by rewriting a landed file); PRs **stack** (each ticket forks
from the previous ticket's branch); a **gate is a pause, not a block**; **secrets never** enter
any ledger (record `provided: yes/no`). Validate with
`bash <skills>/build-memory/scripts/check-build-memory.sh .`.

## Do

- Run `cargo fmt --all --check` and the scoped `cargo test -p hh-<crate>` (or
  `--workspace`) before claiming verification — cite the numbers.
- Cite requirement ids (`R-2.*`), `DF-*`/`BL-*` rows and ADRs when discussing
  scope or owed work.
- Amend history by appending dated notes, never by rewriting landed files.

## Don't

- Don't add runtime dependencies — the workspace is hermetic by contract.
- Don't hand-edit generated artifacts, `docs/adr/README.md`, or anything under
  `docs/build/` history regions.
- Don't merge PRs — `mergePolicy: OPERATOR`; the operator merges.
- Don't put secrets, credentials, or live-service claims in any ledger.
