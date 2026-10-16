# AGENTS.md — crates/ (the workspace)

## Purpose

The 50-member Cargo workspace that *is* the system: the E1 kernel, its Stage-1
service slices, the Lab (experiment/evaluation plane), the C3–C5 services, and
the surfaces. Members are declared — with a one-line provenance comment each —
in the root `Cargo.toml`. Dependency edges point downward through the layers
below; the root AGENTS.md has the layering narrative.

## Crate map

| Crate | Purpose |
|---|---|
| `hh-wire` | canonical JSON, JSON-RPC 2.0 framing, SHA-2 — transport primitives, no verbs |
| `hh-ontology` | the formal model: seven-plane home-plane rule (CC10 anchors here), κ, config ids, compliance schema |
| `hh-identity` | idp/1 identity, `ContentAddress`, `VersionedRef`, the one canonicalizer (CC1 anchor), sameness ladder |
| `hh-provenance` | the one `ProvenanceRecord` + `Label` lattice, seven-class `AuthorityClass` order, seal/endorse basis |
| `hh-hir` | the HIR/1 IR: 13 entity kinds, validate/canonicalize/identity/seal/diff/apply/invert/classify |
| `hh-ledger` | the event store / run ledger — append-only hash-chained log, fenced writer, WAL, blob pool, projections |
| `hh-budget` | resource economics: `ResourceQuantity`, `BudgetTree`, spend provenance, `MatchSpec` budgets |
| `hh-registry` | the component-variation registry store (`hh/` + `local/` namespaces, closed `registry/1` kinds) |
| `hh-secrets` | the credential broker — `SecretRef`/`SecretChannel`, mint/mediate/revoke, deny-by-default projection |
| `hh-telemetry` | measurement points M1–M19, span/cost/metric views as pure folds, `TokenVector` |
| `hh-monitor` | the Π reference monitor (TCB): `AuthorityHandle` table, `authorize`, delegation attenuation |
| `hh-containment` | the containment floor — `ContainmentPolicy/1`, layered meet, fail-closed attach |
| `hh-env` | execution environments: `EnvHandle` lifecycle, sandboxed tool exec contract, effect capture |
| `hh-gateway` | the model boundary: dialect-parameterised wire, normalized `ModelEvent`, static routing, cache semantics |
| `hh-context` | the context builder + memory slices (K6 semantic cache included), assembly inputs |
| `hh-control` | the control plane: `control_strategy` contract, the sealed envelope, guard points, stop protocol |
| `hh-verification` | the verification substrate: claims, reconciliation classes, validator/critic declarations |
| `hh-assembly` | the configuration & composition model: assembly grammar, `resolve`, `validate_assembly` |
| `hh-compiler` | the compilation pipeline: link → lower → seal to `RuntimePlan/1`; `hh-compile` binary |
| `hh-embed-schema` | the single schema source for `hh-embed/1` (CC7) + the JSON-Schema exporter |
| `hh-embed` | the `hh-embed/1` boundary service — binding (a) in-process + (b) stdio dispatch through one `handle` |
| `hh-embed-client-generated` | the generated drift-checked client — emitted by `hh-codegen`, never hand-edited |
| `hh-codegen` | the schema-export → generated-client pipeline; `hh-codegen` binary |
| `hh-kernel` | the kernel binary: `serve` (b), `serve --http` (c), `export-schema`, `hello`, `doctor` |
| `hh-helper` | the out-of-process sandbox helper binary + `hh-helper/1` codec (seatbelt/podman backends) |
| `hh-cli` | the `hh` operator CLI — a generated client + renderer over the boundary (K-2: no kernel semantics) |
| `hh-lab` | the Lab schema plane: `hh-experiment/1` dialect, analysis records, the canonical exemplars |
| `hh-experiment` | the single-worker experiment engine — LabDocs store + the register…close lifecycle |
| `hh-results` | the results store: `ResultsRow/1`, watermark reads, `verify_row`, leaderboard projection |
| `hh-analysis` | the estimator kernel: `summarize`/`compare`/`equivalence`/`multiplicity` over results rows |
| `hh-eval` | the evaluation-first slice: metric catalogue, deterministic oracles, pass^k, typed `n/a` lattice |
| `hh-bench` | the benchmark-adapter contract + the hermetic `fixtures/benchset/stage3_v1` corpus |
| `hh-bundle` | the reproducible run bundle: `hh-bundle/1` manifest, staged validation, repro levels |
| `hh-hosting` | the `hh-hosting/1` ABI: hosted-event envelope, `proj`/lift, participant/adapter records, adapter zero |
| `hh-mcp` | the Stage-3 MCP protocol edge + `hh-mcp-serve` binary (fixture surface) |
| `hh-mcp-lab` | the C1 `hh-lab/1` MCP surface — Lab ops as tools, surface-session ledger, caller bindings |
| `hh-acp` | the ACP session boundary + the C2 A2A edge (AgentCard/TaskState records) |
| `hh-web` | the C1 read-only web instrument over binding (c); `hh-web` binary |
| `hh-plugin` | the extension contract surface: `ContractRef`, tier/`depends_on`, spec-DAG check; `hh-plugin-check` |
| `hh-plugin-fixture` | the shared plugin-side `plugin_abi/1` runtime + conformance fixture modes; binary |
| `hh-varhost` | the kernel-side out-of-process variant host over the live `hh-helper` boundary |
| `hh-compact-evict-oldest` | packaged `compaction_strategy` variant `evict_oldest` (plugin under `plugins/`) |
| `hh-memory-store` | packaged `memory_store` variant — `MemoryStorePort` served out of process (plugin) |
| `hh-subagent` | the C1 subagent kernel slice: `spawn`, `merge`, ownership grants (TCB, never model-facing) |
| `hh-orchestrator` | the C3 orchestrator class: topology presets, wait policies, delegation runtime |
| `hh-fleet` | the C4 fleet reconciler — human-agent org layer over durable `FleetSpec`/`WorkItem` rows |
| `hh-fleet-adapter` | the `work_source_adapter` plugin-class contract (the tracker boundary codec) |
| `hh-tracker-fixture` | the packaged reference tracker variant — the honest fixture proxy for HUMAN-H2 |
| `hh-evolution` | the C5 evolution pipeline: S0–S10 candidate state machine over durable rows |
| `hh-debt` | the C4 assumption-debt manager service (sweep/settle/retire, `tier-c4` on `hh-embed`) |
| `hh-xcheck` | the cross-implementation replay/checker packaging (spec §10.7, R2.21, ADR-0353): `hh-xcheck-bundle/1` export + E1 self-check + foreign-answers `verify`; `hh-xcheck` binary |

## Build & Test

```sh
cargo test -p hh-<crate>                 # one crate
cargo test -p hh-<crate> --test <name>   # one acceptance battery (tests/<ticket>.rs)
cargo build -p hh-helper --no-default-features   # tier-off build (removability leg)
cargo build -p hh-embed --no-default-features    # tier-c4 off
```

## Code conventions

- A crate's public contract is its `src/lib.rs` doc header — it names the spec
  section, the `R-2.*` slices, the ticket and the owning ADRs. Read it before
  editing the crate.
- Kernel-owned event classes are registered in `hh-ledger`'s class table with
  declared durability/audit grade — a new `lifecycle.*`/`control.*`/etc. row is
  a schema change, not an implementation detail.
- Lower tiers never depend upward; plugin/adapter halves communicate through
  `plugin_abi/1` records — records-in/records-out, no back-channel authority.
- Binaries are thin: every binary is a `main.rs` over library code — `hh`,
  `hh-kernel`, `hh-helper`, `hh-web`, `hh-mcp-serve`, `hh-codegen`,
  `hh-compile`, `hh-authorize`, `hh-bench-adapter`, `hh-eval-oracle`,
  `hh-gateway-stub`, `hh-plugin-check`, `hh-plugin-fixture`,
  `hh-compact-evict-oldest`, `hh-memory-store`, `hh-tracker-fixture`.

## Critical gotchas

1. `hh-embed-client-generated/src/generated.rs` and `schema/*.json` are
   generated — `bash scripts/check-drift.sh` regenerates and fails on drift.
2. `hh-embed`'s default features include `tier-c4` — check feature flags
   before assuming a type is always compiled.
3. `spikes/` trees are outside the workspace — changes there never affect
   `cargo build --workspace`.
4. The `hh-*-fixture`/`hh-plugin-*` crates are the *plugin side* of
   `plugin_abi/1`; `hh-varhost` is the host side. A variant bug can live on
   either side of the process boundary — identify which before editing.
