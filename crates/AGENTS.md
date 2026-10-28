# AGENTS.md — crates/ (the workspace)

## Purpose

- The 54 workspace members implement the runtime, Lab, clients, and packaged variants. `Cargo.toml` at the repository root is the authoritative member list.
- Read the root `AGENTS.md` first. These instructions add Rust workspace guidance; use the [README documentation links](../README.md#documentation) for the architecture explanation.
- Package names use `hharness-*`, directories generally use `crates/hh-*`, and binaries have their own names. For example: package `hharness-kernel`, directory `crates/hh-kernel/`, binary `hh-kernel`.

## Architecture — crate map

| Crate | Purpose |
|---|---|
| `hharness-wire` | Canonical JSON, JSON-RPC framing, HTTP primitives, SHA-2 |
| `hharness-tls` | The admitted TLS client/server and mTLS implementation (ADR-0365) |
| `hharness-ontology` | Formal vocabulary, home-plane classification, configuration and compliance schemas |
| `hharness-identity` | Identity and version records, content addresses, canonicalization |
| `hharness-provenance` | Provenance records, authority classes, labels, sealing and endorsement |
| `hharness-hir` | HIR/1 documents: validation, identity, sealing, diff and patch operations |
| `hharness-ledger` | Fenced run ledger, WAL, blob pool, projections, and the BlobCipher seam |
| `hharness-budget` | Resource quantities, accounting, budget trees, and matched-budget records |
| `hharness-registry` | Typed component/variant registry, namespaces, snapshots and imports |
| `hharness-secrets` | Credential broker, DPoP verification, and blob AEAD producer |
| `hharness-telemetry` | Measurements, cost/token records, and ledger-derived trace/metric views |
| `hharness-monitor` | Reference monitor: authorization, handle tables, delegation and revocation |
| `hharness-containment` | Containment policy, policy meet, attachment checks and backend requirements |
| `hharness-env` | Environment lifecycle, tool dispatch, effect capture and helper client |
| `hharness-gateway` | Provider/model boundary, dialect codecs, routing and cache semantics |
| `hharness-context` | Context assembly, compaction, memory interfaces and semantic-cache policy |
| `hharness-control` | Control strategies, guarded execution envelope, loops and stopping |
| `hharness-verification` | Claims, evidence reconciliation, validators and critics |
| `hharness-assembly` | Assembly grammar, configuration resolution, composition and validation |
| `hharness-compiler` | Link, lower and seal harness documents into runtime and surface artifacts |
| `hharness-embed-schema` | The single hh-embed/1 and plugin_abi/1 schema source |
| `hharness-embed` | Boundary service: in-process, stdio and loopback HTTP share one RPC handler |
| `hharness-embed-client-generated` | Generated Rust boundary client; never edit its generated.rs by hand |
| `hharness-codegen` | Schema/client generation; hh-codegen executable |
| `hharness-kernel` | hh-kernel command dispatch, serving and identity diagnostics |
| `hharness-helper` | Out-of-process sandbox helper, protocol and seatbelt/Podman backends |
| `hharness-cli` | hh terminal client and renderer over the kernel boundary |
| `hharness-lab` | Experiment, benchmark and analysis records plus canonical exemplars |
| `hharness-experiment` | Experiment engine and durable experiment-document lifecycle |
| `hharness-results` | Results projections, watermark reads, exports and leaderboard views |
| `hharness-analysis` | Estimators and statistical analysis over results rows |
| `hharness-eval` | Metric/oracle catalogue, evaluation, comparison and typed n/a outcomes |
| `hharness-bench` | Benchmark-adapter contracts and the committed benchmark fixture corpus |
| `hharness-bundle` | Reproducible bundle manifests, validation, import and export |
| `hharness-hosting` | Hosted-participant ABI, adapters and event projection/lifting |
| `hharness-mcp` | MCP protocol edge, artifacts, client/executor and fixture server |
| `hharness-mcp-lab` | Lab MCP instrument, caller binding, launches and surface-session records |
| `hharness-acp` | ACP session/client boundary and A2A task/agent-card records |
| `hharness-web` | Read-only web instrument over the loopback HTTP binding |
| `hharness-plugin` | Plugin contracts, dependency-DAG checks and source/dependency linting |
| `hharness-plugin-fixture` | Shared plugin-side ABI runtime and conformance fixture modes |
| `hharness-varhost` | Kernel-side out-of-process variant host over hh-helper |
| `hharness-compact-evict-oldest` | Packaged evict_oldest compaction variant |
| `hharness-memory-store` | Packaged memory-store variant using the shared memory ABI |
| `hharness-subagent` | Subagent spawn, merge, messaging and ownership grants |
| `hharness-orchestrator` | Topology execution, delegation, coordination and wait policies |
| `hharness-fleet` | Durable work-item reconciliation and organizational policy |
| `hharness-fleet-adapter` | Work-source adapter ABI and tracker logic |
| `hharness-tracker-fixture` | Packaged deterministic tracker variant for conformance tests |
| `hharness-github-issues` | Packaged GitHub Issues tracker over host-supplied source documents |
| `hharness-evolution` | Candidate/campaign evolution state machine and durable views |
| `hharness-debt` | Assumption-debt sweep, settlement, retirement and proposal services |
| `hharness-xcheck` | Cross-implementation corpus export, self-check and foreign-answer comparator |
| `hharness` | Facade re-exporting public API crates under short module names |

## Key files

| File | Lines (approx) | Purpose |
|---|---|---|
| `crates/hh-embed/src/lib.rs` | 71 | Boundary bindings and module entry points |
| `crates/hh-embed-schema/src/lib.rs` | ~1.7k | Contract vocabulary, schema export, and identity helpers |
| `crates/hh-codegen/src/main.rs` | ~150 | Generator outputs and orchestration |
| `crates/hh-kernel/src/main.rs` | ~250 | Kernel command dispatch and identity diagnostics |

## Build & Test

- Commands run at the repository root. Select the package and acceptance battery for the change; these examples exercise the boundary:

```sh
cargo test -p hharness-embed
cargo test -p hharness-embed --test cap_2_composed
cargo clippy -p hharness-embed -- -D warnings
cargo fmt --all --check
```

- For schema/client changes, run `bash scripts/check-drift.sh` and review all regenerated outputs.
- For feature/dependency changes, run `bash scripts/check-removability.sh`; it rebuilds tier combinations and checks dependency edges.
- Direct-package tier-off examples are `cargo build -p hharness-helper --no-default-features` and `cargo build -p hharness-embed --no-default-features`. Restore the default helper afterward: `cargo build -p hharness-helper`.
- New behavior needs a regression test in the relevant crate. Prefer a test of an observable outcome, including refusal cases, over one that only repeats implementation details.

## Code conventions

- Read the package's `Cargo.toml`, `src/lib.rs` contract header where present, the relevant implementation, and its tests. Binary-only packages use `src/main.rs`; not every member has a library.
- Keep the root dependency admissions exact: `ring` is a direct dependency only of `hharness-secrets`; the admitted rustls graph belongs to `hharness-tls`; `tempfile` is a dev-dependency of `hharness-varhost`. ADR-0359/0361/0365 bound their permitted use.
- Keep the ledger's `BlobCipher` seam independent of its cryptographic producer in `hharness-secrets`. The TLS `ring` backend is not permission to add direct crypto dependencies elsewhere.
- Event classes are registered in `crates/hh-ledger/src/classes.rs` with durability/audit metadata. Adding a class changes a contract; coordinate its ontology home, schema, producer, projection, and acceptance tests.
- Lower layers do not depend on higher services. Plugin halves exchange `plugin_abi/1` records through the host boundary; do not add back-channel authority.
- Reuse canonical JSON, identity, and class-table implementations. Maintain typed refusals when a capability or tier is absent.

| Correct ✓ | Wrong ✗ |
|---|---|
| Add a boundary field to the schema source and regenerate both clients | Patch only the Rust client or handwritten TypeScript types |
| Use an existing workspace crate as a dev-dependency for acceptance fixtures where appropriate | Add a production dependency solely to make a test reach another layer |
| Check the host and plugin sides of an ABI failure | Bypass the helper boundary to make a packaged variant work |

## Critical gotchas

1. `crates/hh-embed-client-generated/src/generated.rs`, `clients/ts/hh-embed.ts`, and `schema/*.json` are generated. Change their source, not the output.
2. Cargo features are additive across dependencies. A package-level `--no-default-features` does not disable defaults requested by another dependency; inspect the resolved graph before claiming a tier is absent.
3. `spikes/` is excluded from the workspace. A workspace build does not validate a spike.
4. `crates/hh-plugin-fixture/` and the packaged variant crates implement the plugin side; `crates/hh-varhost/` implements the host side. Manifests are under `plugins/`.
5. An interrupted tier-off check can leave a stale `hh-helper` binary. See root gotchas 9–10 for restoration and host-timeout diagnosis.
6. Some source files exceed 40 KB. Locate symbols with `rg -n`, then read bounded ranges; do not dump a whole driver or service file into context.

## Terminology

| Term | Meaning |
|---|---|
| Package / directory / binary | Cargo selector / filesystem path / executable; their names need not match |
| ABI host / variant | Kernel-side process manager / plugin-side implementation |
| TCB | Trusted computing base; authorization and containment code whose invariants must hold |
| Class table | Canonical metadata for registered event classes, including durability and audit rules |

## Do

- Trace a change through its schema, owner, caller, and acceptance test; follow the root verification and evidence rules.

## Don't

- Do not change a contract in only one binding, add upward production dependencies, or treat a fixture result as live-service evidence.
