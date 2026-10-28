# AGENTS.md — HarnessHarness

## Purpose

- HarnessHarness is a Rust runtime and experiment lab for building and evaluating agent harnesses. This file governs repository work; [README.md](README.md) is the user introduction.
- The behavioral contract is [spec/CANONICAL_SPEC.md](spec/CANONICAL_SPEC.md). It is frozen: amend through an ADR and the manifest protocol, not an in-place rewrite.
- Read the applicable nested `AGENTS.md` before editing a subtree. Human contribution and maintainer policies live in [CONTRIBUTING.md](CONTRIBUTING.md), [GOVERNANCE.md](GOVERNANCE.md), and [MAINTAINING.md](MAINTAINING.md).

## Architecture

| Boundary or responsibility | Source of truth |
|---|---|
| `hh-embed/1` schema and generated clients | `crates/hh-embed-schema/` → `crates/hh-codegen/` |
| In-process, stdio JSON-RPC, and loopback HTTP bindings | `EmbedService::handle` in `crates/hh-embed/src/service.rs`; transport adapters in the same crate |
| Authoritative run history | `crates/hh-ledger/`: append-only hash-chained events, fenced writers, durable-prefix projections |
| Identity, provenance, and home-plane classification | `crates/hh-identity/`, `crates/hh-provenance/`, `crates/hh-ontology/` |
| Harness documents and compilation | `crates/hh-hir/`, `crates/hh-assembly/`, `crates/hh-compiler/` |
| Model calls and execution environments | `crates/hh-gateway/`, `crates/hh-env/`, `crates/hh-helper/` |
| Experiment, evaluation, and orchestration services | See the full package map in [crates/AGENTS.md](crates/AGENTS.md) |
| Packaged variants | Manifests in `plugins/`; host in `crates/hh-varhost/`; shared plugin runtime in `crates/hh-plugin-fixture/` |

- `hh` is a client and renderer over `hh-embed/1`; keep kernel semantics out of the CLI (K-2).
- Dependency direction and ownership matter: reuse the existing lower-layer contract; do not add a reverse dependency or a second implementation of a codec, store, or class table (CC1).
- For explanations rather than editing instructions, use [architecture](docs/architecture.md) and [reference](docs/reference.md). Current capability and release evidence belongs in [status](docs/status.md).

## Key files

| File | Lines (approx) | Purpose |
|---|---|---|
| `Cargo.toml` | ~550 | Workspace members, package metadata, dependency admissions |
| `rust-toolchain.toml` | 6 | Rust 1.94 and `rustfmt` pin |
| `.github/workflows/ci.yml` | ~300 | Linux/macOS build, test, provenance, and supply-chain checks |
| `scripts/check-drift.sh` | ~25 | Regenerate schema/client artifacts and compare the listed outputs |
| `crates/hh-codegen/src/main.rs` | ~150 | Authoritative list of schema/client outputs |
| `docs/README.md` | ~100 | Documentation navigation and edit modes |

## Build & Test

- Run from the repository root. Use the toolchain in `rust-toolchain.toml`; setup and platform prerequisites are in [CONTRIBUTING.md](CONTRIBUTING.md).

```sh
cargo build --workspace --all-targets
cargo test --workspace
cargo fmt --all --check
bash scripts/check-drift.sh
bash scripts/check-removability.sh
bash scripts/check-bm-tools.sh
```

- For a scoped change, select the package and acceptance battery you actually touched. These are concrete examples:

```sh
cargo test -p hharness-embed
cargo test -p hharness-embed --test cap_2_composed
cargo clippy -p hharness-embed -- -D warnings
```

- Run `cargo fmt --all --check` and the scoped package tests (or the workspace suite) before claiming code verification. Report executed commands and passed/failed/skipped counts; an unavailable prerequisite is not a pass.
- New behavior needs a regression test that fails without it. Acceptance batteries live under each crate's `tests/`, commonly named for their ticket. Ignored tests cite a `DF-*` row or the live-evidence ADR, for example `#[ignore = "live — ADR-0363: scripts/live-verify.sh <leg>"]`.
- Documentation checks: `bash scripts/check-living-docs.sh`. The clean-clone quickstart check, `bash scripts/check-quickstart.sh`, reads **committed HEAD**, not uncommitted README edits.
- A full CI dispatch is a maintainer/gate action under ADR-0362, not a per-ticket requirement. Follow [MAINTAINING.md](MAINTAINING.md) for CI evidence and release operations.

- Local connectivity smoke:

```sh
cargo build -p hharness-cli -p hharness-kernel
HH_KERNEL_CMD=./target/debug/hh-kernel ./target/debug/hh doctor
```

- Expected doctor field: `"connectivity":"ok"`.
- `HH_KERNEL_CMD` selects the CLI's kernel child; `HH_STORE_ROOT` defaults to the current directory's `.hh/store`; `HH_WORKSPACE_ROOT` defaults to the current directory. Use scratch paths when a check creates runs.

## Code conventions

- **Dependency admissions:** no new dependency or expanded dependency scope without an ADR. The admitted external packages are `ring =0.17.14` directly in `hharness-secrets` (DPoP verification and the ADR-0361 blob cipher, ADR-0359), the pinned rustls graph directly in `hharness-tls` (ADR-0365), and test-only `tempfile` in `hharness-varhost`. Exact pins live in the owning manifests. TLS's `ring` backend does not authorize other crates to depend directly on `ring`.
- **CC7:** edit schema definitions or generator code, then regenerate. Never patch a generated client or exported schema to make a test pass.
- **CC10:** every registered kind has one home. Reuse `crates/hh-ontology/src/planes.rs`; kernel event-class metadata lives in `crates/hh-ledger/src/classes.rs`.
- **Durable before visible:** state changes append through the run's fenced writer. Projections fold durable records; they do not become another authoritative store.
- **Honest refusal:** absent capabilities return typed `Unsupported`, `Refused`, or `n/a{reason}` outcomes. Do not silently downgrade or fabricate a result.
- **Feature tiers:** inspect the owning package's features. `hharness-helper`, `hharness-ledger`, and `hharness-env` have `tier-c1`; `hharness-embed` has `tier-c4` for fleet/evolution/debt. Preserve the removability checks and typed refusals.

| Correct ✓ | Wrong ✗ |
|---|---|
| Change `hh-embed-schema`, regenerate, review every changed output | Hand-edit `generated.rs` or `clients/ts/hh-embed.ts` |
| Reuse the existing canonicalizer and event-class table | Introduce a second JSON/identity scheme or unregistered event class |
| Record a missing live prerequisite as skipped/pending | Count a fixture or skipped leg as a live pass |

## Critical gotchas

1. **Read deferrals first, every run:** `bash scripts/deferrals-open.sh` indexes owed rows in `docs/tickets/DEFERRALS.md`. Empty output is a valid zero-open-rows result. Read relevant rows with `bash scripts/bm-row.sh DF-S0.3-3` or `bash scripts/bm-row.sh --status DF-S0.3-3`; options precede IDs. Closing a row unblocked by your change is part of the work. Never delete rows or invent an unrecorded deferral.
2. **Generated files are not editing targets:** `schema/*.json`, `crates/hh-embed-client-generated/src/generated.rs`, `clients/ts/hh-embed.ts`, `THIRD_PARTY_NOTICES.md`, `docs/adr/README.md`, and `docs/adr/REVISIT_TRIGGERS.md`. Use their generators; see [docs/AGENTS.md](docs/AGENTS.md) and [clients/AGENTS.md](clients/AGENTS.md).
3. **Doctor checks negotiated identity:** `KernelDescriptor.version` is the semantic version label returned by `kernel_version_label`, not the full `hh-kernel/<version>` ID (ADR-0332). Do not suppress a real mismatch to make the smoke pass.
4. **Verify writes:** the recorded editor overlay anomaly requires shell/Python writes and `git diff` verification before committing. After a commit, `git show --stat` must include every file claimed as changed (BL-41 standing policy).
5. **Evidence labels require evidence:** `live-executed` needs a cited `hh-live-evidence/1` record produced by `scripts/live-verify.sh` under `docs/build/reports/live/` (ADR-0363). Preserve `skipped(<reason>)` and `environment-pending` outcomes. `foreign-run-verified` is a distinct class, not an in-tree or live result. Repository visibility and a release tag do not, by themselves, earn `staging-verified` or `public` capability claims; follow the recorded R4.18 acceptance requirements.
6. **Scope the manifest workflow:** build tickets fork from the preceding ticket's branch; a gate is an operator review pause, not a missing implementation. For work outside a manifest ticket, follow the task's authorized base and scope. Do not advance `nextTicket`, change gate decisions, or close launch work as a side effect of a documentation change.
7. **Build-memory logs are deliberately ignored:** within `docs/build/`, `logs/` is the gitignored subtree. Intentionally committed CI-boundary records there require `git add -f`; do not force-add unrelated logs.
8. **Status is dated-marker governed:** deferral cells have a leading status token, but the latest dated disposition determines owed work. Reconcile lead tokens and machine-readable status at round closure under [MAINTAINING.md](MAINTAINING.md). An old OPEN mention inside a closed row is history, not new debt.
9. **Tier-off checks can leave a stale helper:** an interrupted `scripts/check-removability.sh` can leave `target/debug/hh-helper` without `tier-c1`. Restore it with `cargo build -p hharness-helper` before interpreting later `helper_live` failures.
10. **Loaded macOS hosts can stall helper spawns:** the default spawn/connect bounds are 10 s / 5 s. `HH_HELPER_SPAWN_DEADLINE_MS` and `HH_HELPER_CONNECT_DEADLINE_MS` are diagnostic overrides; the removability script prints a rerun recipe. Record the timeout and rerun outcome; do not reclassify a failure as a pass.
11. **CI path modes are exact:** only `docs/` and `research/` qualify as docs paths in ADR-0362's fast-path classifier. A step after `id: path` that reads committed content there must carry `# ci-mode: both — <reason>`; other steps use `if: steps.path.outputs.mode != 'fast'`. A job's own output under `docs/` is not a committed-content read. Validate with `bash scripts/ci/check-ci-gating.sh .github/workflows/ci.yml`.

## Terminology

| Term | Meaning |
|---|---|
| E1 / kernel | The Rust reference runtime |
| Lab | Experiment, evaluation, and analysis services |
| `hh-embed/1` | Shared typed boundary; bindings (a), (b), (c) are in-process, stdio, loopback HTTP |
| HIR/1 | Harness intermediate representation |
| Π / reference monitor | Authorization interpreter in `hharness-monitor` |
| `DF-*` / `BL-*` | Deferral row / normalized backlog row |
| ADR / `R-2.*` / CC | Architecture decision record / requirement ID / cross-cutting constraint |
| C0…C5 / `tier-c*` | Contract maturity tiers / Cargo feature gates |
| `fixture-verified` | Exercised against fixtures; does not attest to a live service |
| `foreign-run-verified` | Recorded foreign-toolchain replay of the cross-implementation corpus |

## Build memory

- The repository uses committed build memory v2. Its layout is indexed by [docs/build/README.md](docs/build/README.md); documentation edit modes are in [docs/README.md](docs/README.md).
- After the deferrals-first check, a **manifest build ticket** reads its row in `docs/tickets/00_MANIFEST.md`, machine state in `docs/build/LEDGER.md`, and its ticket contract. Runs, PR bodies, `BUILD_INDEX.md`, and gate readouts record what happened. ADRs live in `docs/adr/`; their README is generated.
- Read a row or a section, never a whole large register. Use `scripts/bm-row.sh`, `scripts/deferrals-open.sh --bl`, and `scripts/spec-section.sh`. For source files larger than about 40 KB, locate symbols with `rg -n` and read bounded `sed -n` ranges.
- Contracts and historical records are append-only: correct them with a dated note or new ADR. Preserve stacked ticket history and operator gate decisions. Secrets never enter a ledger; record only `provided: yes/no`.
- Build-ticket runs also require the external build-memory validator from [SteveVitali/agent-skills](https://github.com/SteveVitali/agent-skills). Follow [MAINTAINING.md](MAINTAINING.md#build-memory-tools) to set `AGENT_SKILLS_ROOT` to that repository checkout, then run:

```sh
bash "$AGENT_SKILLS_ROOT/skills/build-memory/scripts/check-build-memory.sh" .
```

- `scripts/check-bm-tools.sh` checks the in-repo row readers; it does not replace the external build-memory validator.

## Do

- Read the relevant source, contract, and acceptance tests before editing; cite requirement/ADR/DF/BL IDs when they determine scope.
- Preserve unrelated work, review the final diff, and report verification with its actual limits.

## Don't

- Do not hand-edit generated artifacts, rewrite historical records, or change the frozen spec without its amendment protocol.
- Do not add dependencies, duplicate authoritative state, or hide unsupported behavior.
- Do not merge PRs: `mergePolicy: OPERATOR`. Do not put credentials or unsupported service claims in any record.
