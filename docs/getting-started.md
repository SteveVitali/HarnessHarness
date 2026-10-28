# Getting started

This guide takes you from a source checkout to a completed offline run, then
to a local model. Start with the offline example to check the runtime and
learn how to inspect its output.

## What you need

- **Git and a shell** for the commands in the [README quickstart](../README.md#try-an-offline-run).
- **A platform C compiler and linker** for native dependencies and executable linking.
- **Rust installed through rustup.** The repository's
  [toolchain file](../rust-toolchain.toml) selects Rust 1.94 and `rustfmt`.
- **Network access for the first build** if the toolchain or pinned Cargo
  dependencies are not cached. The offline run itself makes no model API calls.

The release platforms are macOS and Linux on ARM64 and x86-64. Windows is
outside the v0.1.0 release target set. See [release installation](#install-from-a-release)
if you prefer prebuilt binaries.

## 1. Build and complete the offline example

Run the complete [offline quickstart block](../README.md#try-an-offline-run)
from a directory where you want to clone the repository. It builds two
executables:

| Executable | Role |
|---|---|
| `target/debug/hh` | The command-line client |
| `target/debug/hh-kernel` | The service that owns the run and its durable state |

`hh doctor` starts the kernel through `HH_KERNEL_CMD` and checks the
connection. A healthy result includes `"connectivity":"ok"`.

The rest of the example sends four JSON-RPC requests to `hh-kernel serve`:

1. **`hello`** negotiates the `hh-embed/1` interface.
2. **`open_session`** creates a run from the included
   [fixture definition](../clients/ts/fixture-definition.json).
3. **`submit`** supplies an explicit invocation of the `hh.submit` completion
   capability to the scripted model.
4. **`read`** retrieves the run's events.

The fixture is a small harness definition, including its agent, permissions,
control strategy, context policy, and a budget. It uses the runtime's
deterministic model implementation, so the example needs no model weights or
provider credentials. Its purpose is to exercise the run lifecycle.

A successful result includes a `lifecycle.run.finished` event whose payload
contains:

```json
{"status":"succeeded","stop_reason":{"kind":"completed"}}
```

This is an excerpt from the event payload, not the entire response.
The preceding `hello` and `open_session` responses contain connection and
session metadata. In particular, keep the **`run_id`** returned by
`open_session` if you want to inspect this run again. The example's
`sess-1` is a session handle within the fresh kernel process; it is different
from the durable run ID.

## 2. Inspect the recorded run

Keep the same repository working directory. The kernel stores the run in
`.hh/store/`, which is ignored by Git. Restarting the CLI can read that store.

Set the kernel command for subsequent CLI invocations:

```sh
export HH_KERNEL_CMD="$PWD/target/debug/hh-kernel"
```

Use the commands below, replacing `<run_id>` with the ID from the
`open_session` response:

| Command | What you get |
|---|---|
| `./target/debug/hh run status <run_id>` | A summary of the run |
| `./target/debug/hh run events <run_id>` | Its recorded event stream |
| `./target/debug/hh run inspect <run_id> context_view` | Its context projection |

Look for the terminal event and its `stop_reason` when diagnosing an outcome.
The CLI command's success in reading a run is separate from that run's own
success or failure.

Three environment variables control where the CLI and kernel operate:

| Variable | Default | Use |
|---|---|---|
| `HH_KERNEL_CMD` | `hh-kernel` on `PATH` | Choose the kernel executable the CLI starts |
| `HH_STORE_ROOT` | `.hh/store` under the kernel's working directory | Choose the durable store |
| `HH_WORKSPACE_ROOT` | The kernel's working directory | Choose the workspace for `local_host` sessions |

To keep the same store across working directories, export `HH_STORE_ROOT`
as an absolute path before starting either executable, and reuse that path
when inspecting a run. The [reference](reference.md)
covers additional commands and configuration.

## Connect a local model

The [README's local-model block](../README.md#connect-a-local-model) uses the
same fixture definition with a `model_port` override. That override selects
`hh/gateway_http` and the `openai-compatible@1` dialect, which sends a Chat
Completions request to your server.

Have the server running and a model available before proceeding. Export these
variables in the shell where you will run the command:

| Variable | Value |
|---|---|
| `HH_LOCAL_BASE_URL` | The server's **API base URL**, including any required path prefix |
| `HH_LOCAL_MODEL` | A model ID served by that endpoint |

For example, if the server accepts requests at
`http://127.0.0.1:11434/v1/chat/completions`, the base URL is
`http://127.0.0.1:11434/v1`. The transport appends `/chat/completions`; it does
not add `/v1` for you. Use your server's actual port and API path.

The model should support tool calls. The example asks it to call `hh_submit`
with empty arguments. The names `env:HH_LOCAL_BASE_URL` and
`env:HH_LOCAL_MODEL` in the override tell the kernel to resolve those settings
from its environment.

Run the [local-model block](../README.md#connect-a-local-model) from the
repository root. Inspect both the model-call events and the terminal event:

- `model.call.completed` records a completed provider call.
- `lifecycle.run.finished` records the run's terminal state.
- `status` and `stop_reason` tell you whether the task completed successfully
  or stopped for another reason, such as `format_failure`.

The [2026-10-23 recorded run](build/reports/live/2026-10-23-quickstart-live.json)
made a real provider call and finished with `format_failure`. This example
therefore demonstrates connecting a model and recording its outcome; a
successful completion still depends on the model producing the required
response. See [status and limitations](status.md) for the evidence behind
other provider and integration claims.

## Install from a release

The [v0.1.0 release](https://github.com/SteveVitali/HarnessHarness/releases/tag/v0.1.0)
contains these archives, each with `hh`, `hh-kernel`, and `hh-helper`:

| System | Archive |
|---|---|
| macOS, Apple silicon | `hharness-0.1.0-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `hharness-0.1.0-x86_64-apple-darwin.tar.gz` |
| Linux, x86-64 | `hharness-0.1.0-x86_64-unknown-linux-gnu.tar.gz` |
| Linux, ARM64 | `hharness-0.1.0-aarch64-unknown-linux-gnu.tar.gz` |

Download the archive for your system and its release's `SHA256SUMS` file.
Compare the archive's SHA-256 digest with the matching line in that file,
then extract it. Put the three executables in a directory on your `PATH` and
run:

```sh
hh doctor
```

The CLI starts `hh-kernel` by name unless `HH_KERNEL_CMD` overrides it. Keep
the binaries from the same release together.

The tutorial's fixture is in the source repository, so clone the repository
if you want to run those examples with released binaries. Replace
`./target/debug/hh` and `./target/debug/hh-kernel` with your installed
executable paths.

For now, use a release archive or a source build for the complete CLI/kernel
pair. The presence of individual crates on crates.io does not guarantee that
the full package set has been published. The [status page](status.md)
tracks distribution availability.

## Troubleshooting

| Symptom | What to check |
|---|---|
| `hh` cannot start the kernel | Build both packages from the README command; set `HH_KERNEL_CMD` to the resulting `hh-kernel`, or put the released binaries on `PATH`. |
| The fixture file cannot be read | Run the example from the repository root, where `clients/ts/fixture-definition.json` exists. |
| A saved run cannot be found | Use its `run_id`, not its `session_id`, and the same `HH_STORE_ROOT` or working directory as the original run. |
| The local-model command stops before sending a request | Ensure both `HH_LOCAL_BASE_URL` and `HH_LOCAL_MODEL` are exported in the current shell. |
| The server returns a missing-route error | Check the API base path; the kernel appends `/chat/completions` to the base you supplied. |
| The run finishes with `format_failure` | Inspect the terminal reason and model-call events. The model's answer did not satisfy the required response shape. |

If the problem persists, include the command, operating system, version or
commit, and observed output in a [bug report](../CONTRIBUTING.md).
Use the private route in [SECURITY.md](../SECURITY.md) for vulnerabilities.

## Next steps

Read the [architecture](architecture.md) to understand definitions, runs,
permissions, and the Lab. Use the [reference](reference.md) to find CLI,
Rust, TypeScript, MCP, and ACP entry points. For source changes, follow
[CONTRIBUTING.md](../CONTRIBUTING.md).
