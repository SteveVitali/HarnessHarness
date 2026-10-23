# AGENTS.md — crates/ (the workspace)

## Purpose

The 54-member Cargo workspace that *is* the system: the E1 kernel, its Stage-1
service slices, the Lab (experiment/evaluation plane), the C3–C5 services, and
the surfaces. Members are declared — with a one-line provenance comment each —
in the root `Cargo.toml`. Dependency edges point downward through the layers
below; the root AGENTS.md has the layering narrative.

## Crate map

| Crate | Purpose |
|---|---|
| `hharness-wire` | canonical JSON, JSON-RPC 2.0 framing, SHA-2 — transport primitives, no verbs |
| `hharness-tls` | the workspace's ONE TLS home (ADR-0365): HTTPS client / TLS-server / mTLS over the admitted rustls/`ring` graph — the only member that may name it |
| `hharness-ontology` | the formal model: seven-plane home-plane rule (CC10 anchors here), κ, config ids, compliance schema |
| `hharness-identity` | idp/1 identity, `ContentAddress`, `VersionedRef`, the one canonicalizer (CC1 anchor), sameness ladder |
| `hharness-provenance` | the one `ProvenanceRecord` + `Label` lattice, seven-class `AuthorityClass` order, seal/endorse basis |
| `hharness-hir` | the HIR/1 IR: 13 entity kinds, validate/canonicalize/identity/seal/diff/apply/invert/classify |
| `hharness-ledger` | the event store / run ledger — append-only hash-chained log, fenced writer, WAL, blob pool (+ per-event AEAD `BlobCipher`/`HHBE`, ADR-0361), projections |
| `hharness-budget` | resource economics: `ResourceQuantity`, `BudgetTree`, spend provenance, `MatchSpec` budgets |
| `hharness-registry` | the component-variation registry store (`hh/` + `local/` namespaces, closed `registry/1` kinds) |
| `hharness-secrets` | the credential broker — `SecretRef`/`SecretChannel`, mint/mediate/revoke, deny-by-default projection; DPoP sender-constraint verify + AEAD over the ADR-0359 `ring` pin |
| `hharness-telemetry` | measurement points M1–M19, span/cost/metric views as pure folds, `TokenVector` |
| `hharness-monitor` | the Π reference monitor (TCB): `AuthorityHandle` table, `authorize`, delegation attenuation |
| `hharness-containment` | the containment floor — `ContainmentPolicy/1`, layered meet, fail-closed attach |
| `hharness-env` | execution environments: `EnvHandle` lifecycle, sandboxed tool exec contract, effect capture |
| `hharness-gateway` | the model boundary: dialect-parameterised wire, normalized `ModelEvent`, static routing, cache semantics |
| `hharness-context` | the context builder + memory slices (K6 semantic cache included), assembly inputs |
| `hharness-control` | the control plane: `control_strategy` contract, the sealed envelope, guard points, stop protocol |
| `hharness-verification` | the verification substrate: claims, reconciliation classes, validator/critic declarations |
| `hharness-assembly` | the configuration & composition model: assembly grammar, `resolve`, `validate_assembly` |
| `hharness-compiler` | the compilation pipeline: link → lower → seal to `RuntimePlan/1`; `hh-compile` binary |
| `hharness-embed-schema` | the single schema source for `hh-embed/1` (CC7) + the JSON-Schema exporter |
| `hharness-embed` | the `hh-embed/1` boundary service — binding (a) in-process + (b) stdio dispatch through one `handle` |
| `hharness-embed-client-generated` | the generated drift-checked client — emitted by `hh-codegen`, never hand-edited |
| `hharness-codegen` | the schema-export → generated-client pipeline; `hh-codegen` binary |
| `hharness-kernel` | the kernel binary: `serve` (b), `serve --http` (c), `export-schema`, `hello`, `doctor` |
| `hharness-helper` | the out-of-process sandbox helper binary + `hh-helper/1` codec (seatbelt/podman backends) |
| `hharness-cli` | the `hh` operator CLI — a generated client + renderer over the boundary (K-2: no kernel semantics) |
| `hharness-lab` | the Lab schema plane: `hh-experiment/1` dialect, analysis records, the canonical exemplars |
| `hharness-experiment` | the single-worker experiment engine — LabDocs store + the register…close lifecycle |
| `hharness-results` | the results store: `ResultsRow/1`, watermark reads, `verify_row`, leaderboard projection |
| `hharness-analysis` | the estimator kernel: `summarize`/`compare`/`equivalence`/`multiplicity` over results rows |
| `hharness-eval` | the evaluation-first slice: metric catalogue, deterministic oracles, pass^k, typed `n/a` lattice |
| `hharness-bench` | the benchmark-adapter contract + the hermetic `fixtures/benchset/stage3_v1` corpus |
| `hharness-bundle` | the reproducible run bundle: `hh-bundle/1` manifest, staged validation, repro levels |
| `hharness-hosting` | the `hh-hosting/1` ABI: hosted-event envelope, `proj`/lift, participant/adapter records, adapter zero |
| `hharness-mcp` | the Stage-3 MCP protocol edge + `hh-mcp-serve` binary (fixture surface) |
| `hharness-mcp-lab` | the C1 `hh-lab/1` MCP surface — Lab ops as tools, surface-session ledger, caller bindings |
| `hharness-acp` | the ACP session boundary + the C2 A2A edge (AgentCard/TaskState records) |
| `hharness-web` | the C1 read-only web instrument over binding (c); `hh-web` binary |
| `hharness-plugin` | the extension contract surface: `ContractRef`, tier/`depends_on`, spec-DAG check; `hh-plugin-check` |
| `hharness-plugin-fixture` | the shared plugin-side `plugin_abi/1` runtime + conformance fixture modes; binary |
| `hharness-varhost` | the kernel-side out-of-process variant host over the live `hh-helper` boundary |
| `hharness-compact-evict-oldest` | packaged `compaction_strategy` variant `evict_oldest` (plugin under `plugins/`) |
| `hharness-memory-store` | packaged `memory_store` variant — `MemoryStorePort` served out of process (plugin) |
| `hharness-subagent` | the C1 subagent kernel slice: `spawn`, `merge`, ownership grants (TCB, never model-facing) |
| `hharness-orchestrator` | the C3 orchestrator class: topology presets, wait policies, delegation runtime |
| `hharness-fleet` | the C4 fleet reconciler — human-agent org layer over durable `FleetSpec`/`WorkItem` rows |
| `hharness-fleet-adapter` | the `work_source_adapter` plugin-class contract (the tracker boundary codec) |
| `hharness-tracker-fixture` | the packaged reference tracker variant — the honest fixture proxy for HUMAN-H2 |
| `hharness-github-issues` | the packaged GitHub Issues tracker variant (R4.7; DF-S5.6-1) — `GithubTracker` out of process |
| `hharness-evolution` | the C5 evolution pipeline: S0–S10 candidate state machine over durable rows |
| `hharness-debt` | the C4 assumption-debt manager service (sweep/settle/retire, `tier-c4` on `hharness-embed`) |
| `hharness-xcheck` | the cross-implementation replay/checker packaging (spec §10.7, R2.21, ADR-0353): `hh-xcheck-bundle/1` export + E1 self-check + foreign-answers `verify`; `hh-xcheck` binary |
| `hharness` | the umbrella facade crate (L4.4): re-exports the publishable public-API crates under short module names (`hharness::ledger`, `hharness::hir`, …) for library consumers |

## Build & Test

```sh
cargo test -p hharness-<crate>                 # one crate
cargo test -p hharness-<crate> --test <name>   # one acceptance battery (tests/<ticket>.rs)
cargo build -p hharness-helper --no-default-features   # tier-off build (removability leg)
cargo build -p hharness-embed --no-default-features    # tier-c4 off
```

## Code conventions

- The workspace is dependency-free by contract **modulo ADR-0359 +
  ADR-0365**: only `hharness-secrets` may name the admitted `ring =0.17.14`
  pin, and only inside the ADR's closed API scope (DPoP signature
  verification + test-time fixture signing + `ring::aead`/`ring::rand`,
  the blob-pool AEAD R3.8 consumed under ADR-0361's `BlobCipher` seam);
  and only `hharness-tls` may name the admitted rustls graph
  (`rustls =0.23.46` on the `ring` backend + `rustls-webpki =0.103.16`
  + `webpki-roots =1.0.10` — the closed HTTPS-client / TLS-server / mTLS
  scope). Every
  other crate and every other admitted surface stays hermetic.
- A crate's public contract is its `src/lib.rs` doc header — it names the spec
  section, the `R-2.*` slices, the ticket and the owning ADRs. Read it before
  editing the crate.
- Kernel-owned event classes are registered in `hharness-ledger`'s class table with
  declared durability/audit grade — a new `lifecycle.*`/`control.*`/etc. row is
  a schema change, not an implementation detail.
- Lower tiers never depend upward; plugin/adapter halves communicate through
  `plugin_abi/1` records — records-in/records-out, no back-channel authority.
- Binaries are thin: every binary is a `main.rs` over library code — `hh`,
  `hh-kernel`, `hh-helper`, `hh-web`, `hh-mcp-serve`, `hh-codegen`,
  `hh-compile`, `hh-authorize`, `hh-bench-adapter`, `hh-eval-oracle`,
  `hh-gateway-stub`, `hh-plugin-check`, `hh-plugin-fixture`,
  `hh-compact-evict-oldest`, `hh-memory-store`, `hh-tracker-fixture`,
  `hh-xcheck` — plus `hh-ir-op` (the §3.1.7 ops driven out-of-process over
  canonical bytes).

## Critical gotchas

1. `hh-embed-client-generated/src/generated.rs` and `schema/*.json` are
   generated — `bash scripts/check-drift.sh` regenerates and fails on drift.
2. `hharness-embed`'s default features include `tier-c4` — check feature flags
   before assuming a type is always compiled.
3. `spikes/` trees are outside the workspace — changes there never affect
   `cargo build --workspace`.
4. The `hh-*-fixture`/`hh-plugin-*` crates are the *plugin side* of
   `plugin_abi/1`; `hharness-varhost` is the host side. A variant bug can live on
   either side of the process boundary — identify which before editing.
