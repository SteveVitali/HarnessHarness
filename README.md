# HarnessHarness

The E1 reference build of the MetaHarness program: a kernel + Laboratory
for *harness engineering* — durable, observable, model-conditioned
runtimes for goal-directed agents, with provenance, budgets and
governance carried as load-bearing contracts rather than conventions.

Every behavior is specified in `spec/CANONICAL_SPEC.md` (living → frozen;
amend via the manifest protocol + an ADR) and landed ticket-by-ticket — the
authoritative record of what shipped is `docs/build/`, one row per landed
ticket in `docs/build/BUILD_INDEX.md`.

## Quickstart

<!-- quickstart:begin -->
```sh
# fresh clone → build the CLI pair
git clone https://github.com/SteveVitali/HarnessHarness.git
cd HarnessHarness
cargo build -p hharness-cli -p hharness-kernel

# connectivity self-check — `hh` over the spawned kernel (binding (b))
HH_KERNEL_CMD=./target/debug/hh-kernel ./target/debug/hh doctor
# → {"kind":"result","payload":{"connectivity":"ok", …}}

# a first hermetic run — the offline scripted kernel model over the
# `hh-embed/1` stdio binding (the conformance fixture definition ships in
# the repo); the `{kind:"invoke"}` block issues the `hh.submit`
# completion call, so the run finishes `succeeded`/`completed`
DOC=$(tr -d '\n' < clients/ts/fixture-definition.json)
{
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"hello","params":{"contract_major":1,"client":{"name":"quickstart","version":"1","kind":"cli"},"capabilities":{"experimental":true}}}'
  printf '{"jsonrpc":"2.0","id":2,"method":"open_session","params":{"spec":{"kind":"new","definition":{"kind":"document","document":%s},"environment":{"kind":"connection_info","connection_info":{"class":"local_host"}},"attendance":{"value":"async","source":"declared"}},"idempotency_key":"qs-open"}}\n' "$DOC"
  printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"submit","params":{"session_id":"sess-1","input":[{"kind":"invoke","capability":"hh.submit","args":{}}],"idempotency_key":"qs-submit"}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":4,"method":"read","params":{"session_id":"sess-1","cursor":{"kind":"seq","seq":1},"limit":300}}'
} | ./target/debug/hh-kernel serve
# → …"class":"lifecycle.run.finished"…"status":"succeeded"…
```
<!-- quickstart:end -->

`sess-1` is the first session a fresh `serve` process allocates — the
pipe works because every line above is independent of the responses.
The run's durable ledger lands under `.hh/store` (gitignored); read it
back any time with `hh run events <run_id>`.

Then, if you have a local OpenAI-compatible model endpoint (Ollama or a
`llama.cpp` server), the same definition drives one real-model run — the
endpoint and model ride `env:` coordinates the kernel resolves (names
only, never values in the ledger):

<!-- quickstart-live:begin -->
```sh
: "${HH_LOCAL_BASE_URL:?set to your OpenAI-compatible base URL, e.g. http://127.0.0.1:11434}"
: "${HH_LOCAL_MODEL:?set to a model id the endpoint serves}"
HH_KERNEL_CMD=./target/debug/hh-kernel \
  ./target/debug/hh run start clients/ts/fixture-definition.json \
    'You have exactly one tool available: hh_submit, which takes no arguments. Call hh_submit now with empty arguments {}. If you cannot call tools, reply with the single word done.' \
    --override '/assembly/slots/model_port={"one":{"enabled":true,"variant":{"class_id":"model_port","variant_id":"hh/gateway_http","version_selector":"latest"},"params":{"dialect":"openai-compatible@1","endpoint":"env:HH_LOCAL_BASE_URL","provider_model_id":"env:HH_LOCAL_MODEL"}}}'
# → …"class":"lifecycle.run.finished"… — the run reaches a recorded
#   terminal state; the outcome depends on the model's tool-call fidelity
#   (a schema-violating answer finishes `format_failure` — honest, not hidden)
```
<!-- quickstart-live:end -->

`scripts/check-quickstart.sh` executes the hermetic block verbatim from a
clean clone — it runs in CI, so the recipe above is what a newcomer
actually gets.

## Live providers — credentials by name only

Everything above works fully offline. Driving a real provider is opt-in
and goes through `scripts/live-verify.sh <leg>` (ADR-0363): it sources a
`live.env` file — default `~/.config/harnessharness/live.env` (a symlink
to a gitignored `.env`; `HH_LIVE_ENV` overrides, tests only) — under
`set +x`, never echoes it, and never blanket-exports it. The names it
understands (`--check-names [--all]` reports each `provided: yes/no` —
the only thing any transcript, ledger or committed record may say about
them):

| Name | What it coordinates |
|---|---|
| `HH_LOCAL_BASE_URL` · `HH_LOCAL_MODEL` | a local OpenAI-compatible endpoint (Ollama, llama.cpp) and the model id it serves |
| `OPENAI_API_KEY` · `ANTHROPIC_API_KEY` | the hosted provider credentials — the kernel resolves `env:` `SecretRef` coordinates through the credential broker; values never enter the run ledger |
| `HH_GITHUB_OAUTH_CLIENT_ID` | the OAuth device-flow client registration |
| `HH_GITHUB_SANDBOX_TOKEN` · `HH_GITHUB_SANDBOX_REPO` · `HH_GITHUB_WEBHOOK_SECRET` | the private-sandbox tracker + signed-webhook leg |

Every live leg writes a `hh-live-evidence/1` record under
`docs/build/reports/live/` with its real cost in `SPEND.jsonl`; a leg
that did not execute records `skipped(<reason>)`, never a pass.

## Repository layout

| Path | What it is |
|---|---|
| `crates/` | the Cargo workspace — 54 member crates (`Cargo.toml` `members`), standard library modulo the ADR-admitted deps (`ring =0.17.14` in `hharness-secrets` — ADR-0359; the `rustls =0.23.46`/`ring`-backend graph in `hharness-tls` — ADR-0365) plus a single dev-dependency (`tempfile`, used by `hharness-varhost` tests) |
| `plugins/` | committed plugin manifests (`hh-compact-evict-oldest`, `hh-memory-store`, `hh-tracker-fixture`) |
| `schema/` | generated contract artifacts (`hh-embed-1.schema.json`, `plugin-abi-1.schema.json`, `compat-1.matrix.json`) — regenerated by `scripts/check-drift.sh`, never hand-edited (CC7: schema drift is a build failure) |
| `spikes/` | committed-but-throwaway Stage-0 measurement trees — outside the workspace (`exclude = ["spikes"]`), deletable without breaking the build |
| `spec/` | the canonical spec + `spec/sections/` sources + `spec/READINESS_REPORT.md` |
| `research/` | the program ledger (`LEDGER.md`), research ADRs (`decisions/ADR-0001…`), registers, dossiers, briefs — frozen at the readiness gate |
| `docs/` | docs 1–3 (the founding corpus), `docs/adr/` build ADRs (append-only), `docs/tickets/` the ticket chain + `DEFERRALS.md`, `docs/build/` build memory — see `docs/README.md` for the mode table |
| `foreign/` | the foreign-implementation replay candidates `e2-python` + `e3-typescript` — the `foreign-run-verified` evidence producers over the committed `hh-xcheck` bundle |
| `clients/` | `clients/ts` — the generated TypeScript `hh-embed/1` client (drift-checked; smoke-tested over `serve --http` by e3-smoke) |

## Toolchain

`rust-toolchain.toml` pins Rust **1.94** (`rustfmt` component). The
workspace is hermetic modulo the admitted deps — `hharness-secrets`
pins `ring =0.17.14` for the DPoP verifier plus the `aead`/`rand`
surface R3.8's blob-pool AEAD consumes under ADR-0361, and `hharness-tls`
pins the `rustls =0.23.46`/`ring`-backend graph under ADR-0365's closed
HTTPS/mTLS scope (CI fetches both
via `cargo fetch --locked`; nothing else imports them); CI
(`.github/workflows/ci.yml`) runs `cargo fmt --all --check`,
`cargo build --workspace --all-targets`, `cargo test --workspace`, and
`bash scripts/check-drift.sh`.

```sh
cargo build --workspace          # build every crate + binary
cargo test --workspace           # the full suite (3,800 tests at the R4.16 close — the last full-suite run)
bash scripts/check-drift.sh      # regenerate schema artifacts; fail on drift
bash scripts/check-removability.sh   # the CC6 tier-removability gate
```

## Binaries

| Binary | Crate | What it is |
|---|---|---|
| `hh` | `hharness-cli` | the operator CLI — every command is a typed `hh-embed/1` operation sequence (K-2: no kernel semantics in the CLI) |
| `hh-kernel` | `hharness-kernel` | the kernel binary: `serve` (stdio JSON-RPC 2.0, binding (b)) · `serve --http` (loopback, binding (c)) · `export-schema` · `hello` · `version` · `doctor` |
| `hh-helper` | `hharness-helper` | the out-of-process sandbox helper (seatbelt/podman backends; helper-enforced deadlines) |
| `hh-web` | `hharness-web` | the read-only web instrument over binding (c) |
| `hh-mcp-serve` | `hharness-mcp` | the MCP server (stdio + Streamable HTTP — POST request/response plus the `GET /mcp` server-sent-event stream landed at R2.19) |
| `hh-codegen` | `hharness-codegen` | the schema-export → generated-client pipeline (the drift check's generator) |
| `hh-compile` | `hharness-compiler` | the out-of-process profile compiler |
| `hh-authorize` | `hharness-monitor` | the out-of-process `authorize` over `hh-monitor/authorize-input/1` |
| `hh-bench-adapter` · `hh-eval-oracle` · `hh-gateway-stub` · `hh-plugin-check` · `hh-plugin-fixture` · `hh-compact-evict-oldest` · `hh-memory-store` · `hh-tracker-fixture` · `hh-xcheck` | respective crates | lab instruments, OOP oracle/stub binaries, plugin binaries, and the cross-implementation replay checker (`hh-xcheck-bundle/1` export + E1 self-check + foreign-answers `verify`, landed at R2.21) |

## Install

```sh
cargo install hharness-cli     # the `hh` operator CLI
cargo install hharness-kernel  # the `hh-kernel` service binary
cargo install hharness-helper  # the `hh-helper` sandbox helper
```

Prebuilt archives of those three binaries for the ADR-0380 target set
(macOS arm64 + x86_64, Linux x86_64 + aarch64) attach to each GitHub
Release with a `SHA256SUMS` manifest.

For library use, the umbrella crate re-exports every publishable
public-API crate under short module names:

```sh
cargo add hharness   # hharness::ledger, hharness::hir, hharness::embed, …
```

The `tier-c1`/`tier-c4` features are on by default;
`default-features = false` removes those tiers honestly (gated ops
answer typed `unsupported` refusals).

## `hh` — quick orientation

`hh` spawns `hh-kernel serve` over stdio (binding (b)); `HH_KERNEL_CMD`
overrides the child command. The kernel binds `HH_STORE_ROOT` (default
`<cwd>/.hh/store`) and `HH_WORKSPACE_ROOT` (default the current
directory).

```sh
cargo build -p hharness-cli -p hharness-kernel
HH_KERNEL_CMD=./target/debug/hh-kernel ./target/debug/hh doctor
# → {"kind":"result","payload":{"connectivity":"ok", …}}
```

Command nouns (see `crates/hh-cli/src/cli.rs` for the exact dispatch):
`version` · `doctor` · `run` {start, events, status, inspect, resume,
fork, cancel, amend, submit, export, import} · `env` {status, meters,
snapshot, derive, set-phase, list-detached, open, attach, close,
diff, restore, upload, download} · `approval` {list, show,
respond} · `compact` · `serve` · `acp` · `leaderboard` · `profile` ·
`config explain` · `definition` {plan, apply, diff, explain, validate,
identity, compile} · `registry` {catalog, query, resolve, register,
publish, snapshot, conformance, deprecate, yank, revoke, lineage,
sameness, verify} · `experiment` {register, expand, open, next, claim,
launch, settle, pause, resume, close} · `results` {query, row, history,
cells, distribution, catalogue, verify-row, export} · `eval` {catalogue,
compare, scorecard, equivalence, loss-report} · `compare` {report,
scorecard} · `analysis` {targets, design} · `model` {snapshot-claim,
regression} · `debt` {evaluate, index, report} · `bundle` {create,
validate, reproduce, show} · `participant` {describe, probe, register,
conformance} · `fleet` {view, items, item, reconcile, observe,
capabilities, records, webhook} · `lab serve`. Unknown commands refuse
`unknown_command` — no catch-all, no abbreviation matching.

`hh run start <definition|ref:…> [prompt|-]` opens a run; a declared
budget is required (the kernel re-checks — the CLI gate is UX, never
authority). The canonical exemplar `ExperimentSpec` documents
(`lab/compaction-family-v1`, `lab/control-strategy-family-v1`) live in
`crates/hh-lab/src/exemplars.rs`; the hermetic benchmark corpus is
`crates/hh-bench/fixtures/benchset/stage3_v1`.

## Status — honesty ceiling

The build ran **offline/hermetic by design** — `fixture-verified` is the
floor every surface stands on, and **`live-executed` is earned exactly
where a cited `hh-live-evidence/1` record exists**: 34 committed records
under `docs/build/reports/live/` produced by `scripts/live-verify.sh`
(ADR-0363) at a cumulative live spend of USD 25.26 of the round-4 USD 100
cap. The live-executed surface includes all three provider lanes (local
Ollama, OpenAI, Anthropic), the ADR-0365 TLS-terminating transport,
sandboxed tool exec on the real `hh-helper` backend, the calibrated
judge pair, the GitHub Issues work-source adapter + signed webhook on a
private sandbox repo, OAuth device flow, and every public surface —
`hh` over all three bindings, `hh-web`, `hh-mcp-serve`, `hh-mcp-lab`,
`hh acp` (the official ACP SDK client), the SDK/embed API, and the
packaged plugins out of process. **`staging-verified` and `public`
stay unearned**: no staging environment is provisioned; R4.18 owns the
publish decision. The E2/E3 foreign replays of the committed
`hh-xcheck` bundle are a distinct `foreign-run-verified` evidence class,
below the in-tree ceiling and never conflated — 57/57 on each toolchain
over the eight arms. Honestly surfaced limits (a replay-divergence
class on effect-dispatching runs, the official MCP SDK's protocol-era
pin) are recorded in the capability account, not hidden: the
authoritative surface-by-surface account is
`docs/build/OPERATIONAL_READINESS.md`; the remaining registers are
`docs/build/BACKLOG.csv` and the human steps (`HUMAN-H*`) in
`docs/tickets/00_MANIFEST.md`.

**Build state.** Round 1 closed at GATE-ACCEPT (2026-10-02, all ten
accepted deviations signed). Round 2 — the carried-forward backlog
discharge — landed R2.1–R2.26 and **GATE-G4 PASSED** (2026-10-10,
operator blanket acceptance): the 28-row carried set is dispositioned
(env/human rows stay gate-pending on HUMAN-H1–H4), coverage recomputed
to 41 MET / 19 PARTIAL / 6 MET-DIFFERENTLY = 47/66 on both sums, and
the verified ceiling stays fixture-verified — the round landed no claim
above it. Round 3 — the in-build residual discharge — landed
R3.1–R3.10 (the stacked PR chain #137–#146, open; the operator merges)
and **GATE-G5 PASSED** (2026-10-16, operator verbatim "passed,
proceed", blanket acceptance): the 21-row carried set is dispositioned
(16 CARRY to Round 4, 5 ACCEPT, 0 WAIVE; the 19 owed DEFERRALS rows
inherit their home's disposition), coverage recomputed to 47 MET /
11 PARTIAL / 6 MET-DIFFERENTLY / 2 MET-ENGINEERED = 55/66
engineering-closed and 53/66 requirement-satisfied, and the
fixture-verified ceiling still stands. Round 4 — the launch round
— landed R4.0–R4.16a plus the L4.x launch track (the stacked PR chain
#148–#197, open; the operator merges): the `hh-live-evidence/1`
live-verification framework and its 34 records, the rustls admission
(ADR-0365), the real GitHub Issues adapter + signed webhook, the
GATE-G7-adopted rulings set (146 dispositions) and the R4.15 conform
legs, the publish/release machinery (the `hharness` facade, crates.io
metadata, `release.yml`), the L4.6/L4.6a live E2E capstone, and the
living-docs/public-onramp polish. **GATE-G6 PASSED** (2026-10-23,
operator signed "accept, proceed"): zero OPEN DEFERRALS rows, zero open
backlog debt, coverage 61 MET / 5 MET-DIFFERENTLY / 0 PARTIAL = 66/66,
every public surface `live-executed` or honestly accounted. The
per-ticket record is `docs/build/BUILD_INDEX.md`; the rounds' full
accountings are the gate readouts (`docs/build/readouts/GATE-G4.md`,
`docs/build/readouts/GATE-G5.md`, `docs/build/readouts/GATE-G6.md`).

## Where to start

- The contract: `spec/CANONICAL_SPEC.md` (+ `spec/sections/`).
- The docs map and edit modes: `docs/README.md`.
- What shipped, per ticket: `docs/build/BUILD_INDEX.md`.
- The build chain a human can drive: `docs/tickets/00_MANIFEST.md`.
- Working on the repo as an agent: `AGENTS.md`.
- The maintainer + decision policy: `MAINTAINING.md`, `GOVERNANCE.md`.
- Vulnerability reports + contributing: `SECURITY.md`, `CONTRIBUTING.md`.

## About the build record

This repository commits its own construction history — it is part of the
artifact, not scaffolding to hide:

- `docs/build/` — the build memory: `LEDGER.md` machine state,
  `BUILD_INDEX.md` (one row per landed ticket), `runs/<ticket>.md`
  per-ticket run ledgers, `pr/`, `readouts/` gate readouts, `reports/`
  evidence.
- `docs/tickets/` — the manifest chain (`00_MANIFEST.md`) and
  `DEFERRALS.md`, the append-only ledger of owed work.
- `docs/adr/` — the build's decision records (append-only).
- `research/` — the MetaHarness program ledger, registers and research
  ADRs that produced this build.

These trees are **append-only provenance**: entries are amended by
appending dated notes, never rewritten, and they keep the absolute
paths, branch names and environment details recorded at the time —
that fidelity is what makes the record auditable. By contrast the
*living* human-facing files (this README, `CONTRIBUTING.md`, the
docs and guides outside those trees) carry no personal paths;
`scripts/check-living-docs.sh` enforces the split and CI runs it on
every head. The posture was ruled 2026-10-20 ("keep provenance, fix
living docs") and landed at L4.1.

## License

Dual-licensed under **MIT OR Apache-2.0** — see `LICENSE-MIT` and
`LICENSE-APACHE`; you may use either. Contributions are licensed under
the same terms (`CONTRIBUTING.md`). Third-party components — the named,
pinned admission set (`ring`, the rustls graph, `webpki-roots`, the
`tempfile` dev-dep) — are inventoried in `THIRD_PARTY_NOTICES.md`.
