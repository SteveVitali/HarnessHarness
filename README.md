# HarnessHarness

HarnessHarness is a Rust runtime for building and studying **AI agent harnesses**:
the code around a model that manages context, tools, execution, and limits.
It combines a **kernel** that executes agent runs with a **Laboratory** for
comparing harness configurations. Budgets, permissions, provenance, and a
durable event history are part of the runtime's contracts.

Use it to:

- **Inspect agent execution.** Follow model calls, tool effects, approval
  decisions, and budget consumption through a run's event ledger.
- **Compare harness designs.** Define experiments over components such as
  context policies and control strategies, then inspect results and evaluations.
- **Build integrations.** Use the `hh` CLI, Rust APIs, or the shared
  `hh-embed/1` interface over stdio or loopback HTTP. MCP and ACP surfaces are
  also included.

HarnessHarness is the reference implementation of the MetaHarness research
program. Start with the [architecture](docs/architecture.md) for the design,
and [status and limitations](docs/status.md) for what has been tested and the
current boundaries of support.

## Try an offline run

You need Git, a shell, a platform C compiler and linker, and [Rust with rustup](https://www.rust-lang.org/tools/install).
The repository pins Rust 1.94. The first build may download the toolchain and
dependencies; the example itself needs no model server or API key.

This builds the CLI and kernel, checks their connection, and completes a
scripted run using a fixture included in the repository:

<!-- quickstart:begin -->
```sh
# Build the CLI and kernel
git clone https://github.com/SteveVitali/HarnessHarness.git
cd HarnessHarness
cargo build -p hharness-cli -p hharness-kernel

# Check their connection
HH_KERNEL_CMD=./target/debug/hh-kernel ./target/debug/hh doctor
# → {"kind":"result","payload":{"connectivity":"ok", …}}

# Complete a scripted run and read its events
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

Look for `"connectivity":"ok"`, then a `lifecycle.run.finished` event with
`"status":"succeeded"` and `"stop_reason":{"kind":"completed"}`. The fixture
uses a deterministic model implementation so that you can try the runtime
before configuring a provider. `sess-1` is the first session in this fresh
kernel process; integrations should use the session ID returned by
`open_session`.

The run is stored in `.hh/store/`. The [getting started guide](docs/getting-started.md)
explains the requests, how to inspect a run, and installation alternatives.
The offline block above is exercised by the
[clean-clone quickstart check](scripts/check-quickstart.sh).

Prebuilt binaries are available from [GitHub Releases](https://github.com/SteveVitali/HarnessHarness/releases)
as archives containing `hh`, `hh-kernel`, and `hh-helper` for macOS and
Linux. See the [platform and installation guide](docs/getting-started.md#install-from-a-release).

## Connect a local model

After the source build, you can use a local server that implements the
OpenAI-compatible Chat Completions API. Export `HH_LOCAL_BASE_URL` as its API
base URL, including a prefix such as `/v1` if required, and `HH_LOCAL_MODEL`
as a model ID that server serves. The [setup guide](docs/getting-started.md#connect-a-local-model)
explains these settings.

<!-- quickstart-live:begin -->
```sh
: "${HH_LOCAL_BASE_URL:?set to your OpenAI-compatible base URL, e.g. http://127.0.0.1:11434/v1}"
: "${HH_LOCAL_MODEL:?set to a model id the endpoint serves}"
HH_KERNEL_CMD=./target/debug/hh-kernel \
  ./target/debug/hh run start clients/ts/fixture-definition.json \
    'You have exactly one tool available: hh_submit, which takes no arguments. Call hh_submit now with empty arguments {}. If you cannot call tools, reply with the single word done.' \
    --override '/assembly/slots/model_port={"one":{"enabled":true,"variant":{"class_id":"model_port","variant_id":"hh/gateway_http","version_selector":"latest"},"params":{"dialect":"openai-compatible@1","endpoint":"env:HH_LOCAL_BASE_URL","provider_model_id":"env:HH_LOCAL_MODEL"}}}'
# → …"class":"lifecycle.run.finished"… — inspect status and stop_reason
```
<!-- quickstart-live:end -->

This asks the model to call the completion tool. A terminal event means the
run ended; check its `status` and `stop_reason` to learn whether it succeeded.
A model that does not produce the required tool-call shape can finish with
`format_failure`. The [recorded local-model example](docs/build/reports/live/2026-10-23-quickstart-live.json)
exercised a real provider call and ended with that outcome.

## Documentation

| I want to… | Start here |
|---|---|
| Build, run, and inspect my first example | [Getting started](docs/getting-started.md) |
| Understand the kernel, Lab, and run lifecycle | [Architecture](docs/architecture.md) |
| Find commands, configuration, and integration entry points | [Reference](docs/reference.md) |
| Check maturity, supported surfaces, and known limitations | [Status](docs/status.md) |
| Report a bug or contribute a change | [Contributing](CONTRIBUTING.md) |
| Work on the repository with a coding agent | [Agent instructions](AGENTS.md) |
| Explore the specification and design history | [Documentation map](docs/README.md) |

## Contributing

Bug reports, reproducible examples, and documentation improvements are useful
ways to start. Read [CONTRIBUTING.md](CONTRIBUTING.md) for development setup,
verification, and the change process. Decisions and releases follow
[GOVERNANCE.md](GOVERNANCE.md) and [MAINTAINING.md](MAINTAINING.md).

Report vulnerabilities through the private route in [SECURITY.md](SECURITY.md).
Community participation follows the [Code of Conduct](CODE_OF_CONDUCT.md).

The repository also preserves its research and construction history under
[`research/`](research/), [`docs/adr/`](docs/adr/),
[`docs/tickets/`](docs/tickets/), and [`docs/build/`](docs/build/).
These records explain how the system was built; the guides above provide the
current orientation. The [documentation map](docs/README.md) explains how each
part of the corpus is maintained.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. Contributions use the same terms. See
[third-party notices](THIRD_PARTY_NOTICES.md) for dependency licenses.
