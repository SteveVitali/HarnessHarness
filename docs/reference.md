# Interfaces and configuration

This page covers the entry points used after the
[first run](getting-started.md). Run shell examples from the repository root
with the CLI and kernel built. The [architecture guide](architecture.md)
explains definitions, runs, and sessions.

## Binaries

| Binary | Cargo package | Purpose |
|---|---|---|
| `hh` | `hharness-cli` | Run, inspect, and manage work through the kernel boundary |
| `hh-kernel` | `hharness-kernel` | Serve the kernel protocol, report identity, and export its schema |
| `hh-helper` | `hharness-helper` | Execute sandboxed work in a separate process |
| `hh-web` | `hharness-web` | Inspect runs through the read-only web surface |
| `hh-mcp-serve` | `hharness-mcp` | Serve compiled MCP tool catalogs; ordinary tool calls return `stage_pending` |
| `hh lab serve` | `hharness-cli` with `hharness-mcp-lab` | Expose Lab operations through MCP |
| `hh-compile` | `hharness-compiler` | Compile definitions through the process boundary |
| `hh-codegen` | `hharness-codegen` | Generate schemas and clients |
| `hh-xcheck` | `hharness-xcheck` | Export and verify the cross-implementation replay corpus |

The [workspace manifest](../Cargo.toml) and [crate map](../crates/AGENTS.md)
cover the additional development, evaluation, and plugin binaries. The release
archives contain `hh`, `hh-kernel`, and `hh-helper`.

## CLI entry points

For a source checkout, set the kernel path once in your shell:

```sh
export HH_KERNEL_CMD="$PWD/target/debug/hh-kernel"
./target/debug/hh doctor
./target/debug/hh version
```

`hh` normally launches `hh-kernel serve` as a child process. `--kernel-cmd`
overrides `HH_KERNEL_CMD`; otherwise the executable is found as `hh-kernel` on
`PATH`.

| Task | Command shape |
|---|---|
| Start a run | `hh run start <definition-file-or-ref> [prompt-or-stdin]` |
| Read the durable event stream | `hh run events <run_id>` |
| Read the current summary | `hh run status <run_id>` |
| Inspect a projection | `hh run inspect <run_id> [run_summary\|context_view\|checkpoint\|compact]` |
| Resume or cancel | `hh run resume <run_id>`, `hh run cancel <run_id>` |
| Handle approvals | `hh approval list`, `hh approval show`, `hh approval respond` command families |
| Work with definitions | `hh definition plan`, `apply`, `diff`, `explain`, `validate`, `identity`, `compile` |
| Work with registered components | `hh registry` command family |
| Run experiments and examine results | `hh experiment`, `hh results`, `hh eval`, `hh compare`, `hh analysis` command families |
| Package runs | `hh bundle`, `hh run export`, `hh run import` command families |
| Connect other surfaces | `hh serve`, `hh acp`, `hh lab serve` |

Angle-bracket operands above describe required values. Obtain `run_id` from
the `open_session` response or the CLI's output; it is different from
`session_id`. A definition reference uses the `ref:` prefix. For `run start`,
a prompt of `-` reads standard input. A declared budget is required, either
in the definition or through the command's budget input.

The definition authoring commands `plan`, `apply`, `explain`, and `validate`
read an [AssemblySource](../crates/hh-lab/src/assembly/source.rs) document, which
wraps a document scaffold and composition layers. This input differs from the
bare HIR document accepted by `run start` and `definition compile`; passing the
quickstart fixture directly to `definition validate` is a schema error.

The CLI's current `--help` response is a short usage error, not an exhaustive
command reference. The [core dispatch](../crates/hh-cli/src/cli.rs) and
[Lab command mappings](../crates/hh-cli/src/lab.rs) define the exact operands
and supported flags. Unknown commands and flags return typed invocation
errors.

For automation, use the structured `result.exit_class` string. The process
exit code is zero for `ok` and uses a defined nonzero band for failures; the
[class table](../crates/hh-cli/src/exit_class.rs) is authoritative. A successful
`run status` or `run events` command means the read succeeded. Check the run's
own outcome and `stop_reason` to determine whether the task succeeded.

## Process configuration

| Setting | Default or meaning |
|---|---|
| `HH_KERNEL_CMD` | `hh-kernel`; CLI child command |
| `HH_STORE_ROOT` | `<current-directory>/.hh/store`; durable kernel store |
| `HH_WORKSPACE_ROOT` | Current directory; workspace bound to `local_host` sessions |
| `HH_PRINCIPAL` | CLI principal name; falls back to `USER`, then an unknown principal |

The kernel reads store and workspace settings when it starts. Use the same
store root when inspecting a previously created run. The source definitions
are in [CLI startup](../crates/hh-cli/src/main.rs) and
[kernel configuration](../crates/hh-kernel/src/serve.rs).

Model endpoint and model selection belong to the definition's `model_port`
configuration. The [local-model example](../README.md#connect-a-local-model) uses
`env:HH_LOCAL_BASE_URL` and `env:HH_LOCAL_MODEL` as references resolved by the
kernel. These names are coordinates chosen by that example, rather than
mandatory global configuration. The HTTP transport appends its operation path
to the supplied API base, so include the provider's required prefix, such as
`/v1`.

## Kernel bindings

| Binding | Wire behavior |
|---|---|
| In-process Rust | `hharness_embed::EmbedService::open` creates the service; `handle` accepts a parsed JSON-RPC request |
| `hh-kernel serve` | UTF-8 JSON-RPC 2.0, one message per line on stdin/stdout; diagnostics on stderr |
| `hh-kernel serve --http 127.0.0.1:7777` | Loopback HTTP; `POST /hh-embed/1` for RPC and `GET /hh-embed/1/events` for NDJSON notifications |

The HTTP binding requires a bearer token, an exact `Host` matching the bound
authority, and requests without browser `Origin` or `Sec-Fetch-*` headers.
Tokens in query strings are refused. It is intended for local clients; the
server refuses non-loopback listeners. Use the web surface for browser access.

The kernel generates a token when none is supplied. `--token-file <path>`
writes it to a file with owner-only permissions on Unix. Without that option,
the launch diagnostic prints the token on stderr. For implementation details,
see [HTTP transport](../crates/hh-embed/src/net.rs) and
[kernel token delivery](../crates/hh-kernel/src/serve.rs).

## Client and schema sources

| Integration | Authoritative starting point |
|---|---|
| Rust generated client | [Client crate](../crates/hh-embed-client-generated/src/lib.rs) |
| TypeScript HTTP client | [Generated module](../clients/ts/hh-embed.ts), [worked smoke example](../clients/ts/smoke.ts) |
| Protocol operation and type definitions | [Schema source](../crates/hh-embed-schema/src/lib.rs) |
| Exported boundary schema | [hh-embed/1 schema](../schema/hh-embed-1.schema.json) |
| Extension protocol | [Plugin ABI schema](../schema/plugin-abi-1.schema.json), [packaged manifests](../plugins/) |
| Compatibility data | [Compatibility matrix](../schema/compat-1.matrix.json) |

Clients negotiate identity with `hello` before other operations. Keep generated
clients and the kernel at compatible schema identities; an identity mismatch
is reported explicitly.

To exercise the TypeScript client against a temporary local kernel and store,
use `bash scripts/e3-smoke.sh` after building `hharness-kernel`. It requires a
Node version that executes TypeScript directly; the
[TypeScript replay guide](../foreign/e3-typescript/README.md) lists supported
version prerequisites. This smoke test uses the shipped fixture without a
model-provider account.

## Rust libraries and feature tiers

The local `hharness` facade re-exports public library crates under names such
as `hharness::ledger`, `hharness::hir`, and `hharness::embed`. Individual crates
can also be used directly. See [availability](status.md#availability) before
choosing a registry dependency.

The owning crates expose `tier-c1` (helper, ledger, environment) and `tier-c4`
(embed) features. Their disabled operations return typed unsupported results
when compiled without the relevant tier. The facade enables both features by
default, but **turning off the facade's default features does not currently
turn off all dependency tiers**: dependency defaults and Cargo feature
unification can enable them. Check the resolved graph when constructing a
reduced build. [The removability checker](../scripts/check-removability.sh)
exercises the scoped tier configurations used by this repository.
