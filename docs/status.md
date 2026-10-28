# Status and limitations

HarnessHarness is an early Rust agent runtime and harness experimentation
system. Start with the [offline example](../README.md#try-an-offline-run) to evaluate
its workflow, then use the [architecture guide](architecture.md) to find the
parts relevant to your application.

## Availability

As checked on **2026-10-27**, the GitHub repository is public and the
[v0.1.0 release](https://github.com/SteveVitali/HarnessHarness/releases/tag/v0.1.0)
provides archives for macOS (Apple silicon and Intel) and Linux (x86_64 and
AArch64). Each archive contains `hh`, `hh-kernel`, and `hh-helper`; the release
also provides `SHA256SUMS`.

Crates.io publication is incomplete at that check: `hharness-cli` and
`hharness-helper` have version `0.1.0`, while `hharness-kernel` and the
`hharness` facade are unavailable. Use a source build or a release archive
for the CLI/kernel pair. See [installation](getting-started.md) for the
supported routes. The workspace package version alone does not establish
registry availability.

Repository visibility and downloadable binaries are separate from the
project's formal launch acceptance. The recorded
[launch checklist](build/readouts/HUMAN-H7.md) still has pending work, and the
build ledger has not recorded launch closeout. This page does not award the
project's `public` operational evidence class or a production-readiness rating.

*Update 2026-10-28 (R4.18):* crates.io publication is complete — all 49
publishable crates are at `0.1.0` (each verified via the crates.io API),
the launch checklist is COMPLETE, and the build ledger records launch
closeout (`projectStatus: DONE`). The `public` evidence class is now
earned for the published crate set, the tagged release, protected
`main`, private vulnerability reporting, and the `live-verify`
environment — per the
[launch record](build/reports/2026-10-28-launch-record.md). A
production-readiness rating is still not awarded; the qualifications
below stand.

## What you can evaluate

| Area | Implemented surface | Evidence and scope |
|---|---|---|
| Durable agent runs | Definitions, budgets, permissions, events, inspection, resume and replay | [Boundary acceptance tests](../crates/hh-embed/tests/conformance.rs); [recorded ledger replay](build/reports/live/2026-10-23-capstone-ledger-replay.json) |
| Model connections | Local OpenAI-compatible APIs, OpenAI, and Anthropic transports | [Local run](build/reports/live/2026-10-23-capstone-run-local.json), [OpenAI run](build/reports/live/2026-10-23-capstone-run-openai.json), [Anthropic call](build/reports/live/2026-10-21-anthropic-call.json); see replay qualification below |
| Contained tools | Separate helper process and mediated tool effects | [Sandboxed execution record](build/reports/live/2026-10-23-capstone-sandboxed-exec.json), on the recorded macOS backend |
| Client interfaces | CLI, Rust embedding, TypeScript HTTP client, web and ACP | [SDK record](build/reports/live/2026-10-23-embed-sdk-live.json), [web record](build/reports/live/2026-10-23-capstone-web.json), [ACP record](build/reports/live/2026-10-23-surface-acp.json) |
| MCP | Compiled artifact server and a separate Lab MCP surface | [Artifact-server record](build/reports/live/2026-10-23-capstone-mcp.json), [Lab MCP record](build/reports/live/2026-10-23-surface-mcp-lab.json); execution scope below |
| Lab | Experiment plans, evaluation, derived results, comparison and bundles | [Experiment exemplars](../crates/hh-lab/src/exemplars.rs), [bundle reproduction record](build/reports/live/2026-10-23-capstone-bundle-repro.json) |
| Plugins and work sources | Packaged component variants and a GitHub Issues adapter | [Plugin manifests](../plugins/), [sandbox-repository webhook record](build/reports/live/2026-10-23-capstone-fleet-webhook.json) |
| Independent replay implementations | Python and TypeScript implementations of the committed replay corpus | [Python guide](../foreign/e2-python/README.md), [TypeScript guide](../foreign/e3-typescript/README.md); these are conformance implementations, not complete alternate kernels |

Each live record describes a particular environment, revision, and set of
assertions. It is evidence for that scope, not a guarantee for every provider,
model, operating system, or deployment. The project has not earned
`staging-verified`; the documented live checks use local, sandbox, or
credential-controlled environments.

## Replay and client compatibility

An early capstone recorded an Anthropic-run replay failure caused by event-ID
allocation during tool-effect dispatch. **The shared replay defect was repaired
in L4.6b**: the [boundary regression test](../crates/hh-embed/tests/l4_6b.rs) and
[later recorded replay](build/reports/live/2026-10-23-l4-6b-replay.json) cover an
effect-dispatching run with a scripted model. That later check made no provider
calls. The [original Anthropic failure record](build/reports/live/2026-10-23-capstone-run-anthropic.json)
remains a failure record; the repair is not a fresh end-to-end Anthropic pass.
Real replay divergences still return `invalid{replay_diverged}`.

`hh-mcp-serve` serves a compiled tool catalog and has no kernel connection.
Ordinary catalog tool calls return the typed refusal `stage_pending`; its
recorded capstone pass verifies negotiation, tool listing, and that refusal.
It does not demonstrate executing arbitrary tools through this binary.
`hh lab serve` is a separate MCP surface for Lab operations, with its own
[execution record](build/reports/live/2026-10-23-surface-mcp-lab.json).

The MCP server's protocol-era pin is `2026-07-28`. The recorded MCP and Lab MCP
checks used the in-repository client because the official SDK used in that
verification refused this pin. Check your client's supported protocol version
before relying on that integration. The ACP evidence used the official SDK
identified in its record.

A model call can complete while the task fails. In particular, the
[local-model quickstart record](build/reports/live/2026-10-23-quickstart-live.json)
passed its transport/lifecycle assertions but the run stopped with
`format_failure`. Inspect the run's terminal outcome and stop reason, rather
than treating receipt of a response or a successful inspection command as task
success.

For reduced Rust builds, also read the
[feature-tier qualification](reference.md#rust-libraries-and-feature-tiers):
disabling facade defaults alone does not remove all transitive tiers.

## How to read the evidence

| Label | Meaning in this repository |
|---|---|
| `fixture-verified` | Exercised against controlled fixtures |
| `live-executed` | A real-environment leg with a cited `hh-live-evidence/1` record |
| `foreign-run-verified` | An independent implementation replayed the committed corpus; a separate kind of conformance evidence |
| `skipped(...)` / `environment-pending` | The leg did not execute or its environment was unavailable |

The [capability account](build/OPERATIONAL_READINESS.md) and
[build index](build/BUILD_INDEX.md) preserve the detailed construction history.
They contain earlier conclusions alongside later amendments. Read the dated
updates and subsequent ticket evidence, including the
[L4.6b replay repair](build/runs/L4.6b.md), when interpreting an old finding.
The [documentation map](README.md) distinguishes current guides from protected
historical records.

For bugs and contributions, see [Contributing](../CONTRIBUTING.md).
For private vulnerability reports, use the [security policy](../SECURITY.md).
