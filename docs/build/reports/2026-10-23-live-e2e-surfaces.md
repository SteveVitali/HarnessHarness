# L4.6a — Live E2E surface & plugin sweep report (2026-10-23)

Ticket `164w` (`docs/tickets/164w_L4.6a__live-e2e-surface-plugin-sweep.md`).
Round 4's deep end-to-end live verification, part 2: every public
HarnessHarness surface and packaged plugin not covered by L4.6 — `hh acp`,
`hh-mcp-lab`, the public SDK/embed library API, and the three packaged
variants (`hh-memory-store`, `hh-compact-evict-oldest`,
`hh-tracker-fixture`) — each driven live through
`scripts/live-verify.sh` under `scripts/live-capstone.sh --sweep l4.6a`
(each leg independently re-runnable; `--list` prints the order). Secrets
were referenced by NAME only; every record carries `provided: yes/no`
and no credential value. Companion to
`2026-10-23-live-e2e-capstone.md` (L4.6).

**Sweep outcome: 6/6 pass.** Live model: local Ollama
`qwen2.5:7b-instruct` via `HH_LOCAL_BASE_URL`/`HH_LOCAL_MODEL`. **L4.6a
debits total USD 0** — the local lane; cumulative live spend at report
time USD 25.259768 (global cap USD 100, unchanged by this sweep).

## Leg ledger

| Leg | Surface | Client | Record (`docs/build/reports/live/`) | Result | Cost (USD) |
|---|---|---|---|---|---|
| surface-acp | shipped `hh acp` verb over a real pty (BSD `script`, `stty -echo`) | **official-sdk** (`@zed-industries/agent-client-protocol`, temp-dir install) | `2026-10-23-surface-acp.json` + transcript | **pass** — initialize → session/new → session/prompt → `session/request_permission` round-trip → `security.permission.decided{allow}` → `state.turn.completed`; `_hh/account` + `_hh/ledger/read` exercised; fresh `hh-kernel serve` attach: `verify` ok + `replay{deterministic}` byte-identical (model.* rows unchanged) | 0 |
| surface-mcp-lab | `hh-mcp-lab` LabServer over loopback HTTP | **in-repo** `McpClient` — see client note | `2026-10-23-surface-mcp-lab.json` + transcript | **pass** — tools/list carries the lab verbs; `launch_run` opens a real child run on the local lane; `submit_input` lowers content→input, turn completes; resources/list+read serve the surface views; second service `verify` ok + `replay{deterministic}` offline | 0 |
| embed-sdk-live | public SDK/embed API (`hharness::embed_client::Client` + `NetClient` + out-of-workspace consumer crate) | generated client + facade | `2026-10-23-embed-sdk-live.json` + transcript | **pass** — hello → open_session → submit → `model.call.*{usage}` on stdio `serve`, loopback `serve --http`, and a temp-dir consumer crate over `path` deps; `verify` ok + `replay{deterministic}` on each arm | 0 |
| plugin-memory-store | packaged `memory_store` variant out of process | varhost + `hh-helper` (`plugin_abi/1`) | `2026-10-23-plugin-memory-store.json` + transcript | **pass** — pack→seal→load (real code_pointer pins), `subprocess_confined` spawn, `memory_store 1.0` bind, memory_abi conformance battery + typed `UnknownName` refusal, `lifecycle.component.bound/invoked` rows durable (bound=1, invoked=10) | 0 |
| plugin-compact-evict-oldest | packaged `compaction_strategy` variant out of process | varhost + `hh-helper` | `2026-10-23-plugin-compact-evict-oldest.json` + transcript | **pass** — pack→seal→load, `subprocess_confined` spawn, `compaction_strategy 1.0` bind, assess→propose→execute over the live session (V1 host-stamped provenance; `remaining_tokens ≤ cap`), component rows durable (bound=1, invoked=3) | 0 |
| plugin-tracker-fixture | packaged `work_source_adapter` variant out of process | varhost + `hh-helper` | `2026-10-23-plugin-tracker-fixture.json` + transcript | **pass** — pack→seal→load, `subprocess_confined` spawn, `work_source_adapter 1.0` bind, capabilities→occurrences→activate_run→list/get over the live session, component rows durable (bound=1, invoked=5) | 0 |

## ACP client note (the ticket's client clause)

The official `@zed-industries/agent-client-protocol` installed
runtime-only into a temp dir (never a repo dep) and **ran the full
exchange** — the record carries `client=official-sdk`. Two transport +
dialect findings the leg surfaced and fixed:

- `hh acp` needs a TTY stdin, and BSD `script` cannot `tcgetattr` the
  socketpair node hands spawned children (`Operation not supported on
  socket` → dead pty → EPIPE). `live-capstone-acp.mjs` interposes `cat`
  so `script`'s stdin is a real pipe.
- The wire's `session/request_permission` emitted the Stage-1 interim
  shape — `toolCall` without `toolCallId`, `options: []` — which the
  official zod schema rejects (`-32602`, recorded honestly as
  `deny{cancelled}`). The durable `security.permission.pending` now
  records the declared offer under `request.options` (the R-2.11
  spelling, ADR-0343 D4) and the wire renders spec-shaped
  `{optionId,name,kind}` options + `toolCallId`.
- The SDK's `extMethod(name, params)` prepends `_` itself — calling it
  with `"_hh/account"` sent `__hh/account` → `-32601`. The driver now
  passes the bare member name; `_hh/account` and `_hh/ledger/read` are
  exercised verbatim on the official arm.
- The pty had an echo race: `stty -echo` runs inside `script`'s child,
  so a frame landing before it applies echoes back to the SDK's duplex,
  which reads its own `initialize` as an incoming request — the SDK's
  `methodNotFound` reply (id-correlated) then rejected the pending call
  for every offered version. A bounded 500 ms settle window in
  `live-capstone-acp.mjs` closes the race deterministically.

Recorded pin negotiation, verbatim: the spec offer `protocolVersion: 1`
refused (the dialect's N2 pin set is the string labels `0.1.0` /
`v1-compat` — never coerced), the driver's dialect-label probe
`"v1-compat"` **negotiated** (`ACP_INIT: offered="v1-compat"
negotiated="v1-compat"`), then session/new → prompt → permission
round-trip → `_hh/account` all on the official connection.
Carried dialect gap (non-blocking): the dialect's
`session/update{sessionUpdate: state_update}` notification fails the
SDK's closed update union — the peer logs `-32602` and drops it; the
round-trip and durable rows are unaffected (updates are a projection,
not the record).

## MCP-lab client note (the ticket's "otherwise" clause)

The official `@modelcontextprotocol/sdk` installed bounded (95
packages) and refused the server's protocol-era pin —
`Server's protocol version is not supported: 2026-07-28`. The in-repo
`McpClient<HttpTransport>` ran the same transport shape; the record
carries `client=in-repo` — the same pin-divergence outcome L4.6's
`capstone-mcp` leg recorded.

## Findings fixed in-ticket (small, test-backed)

- `extract_recorded` synthesized a `HumanInput::Approval` cue for any
  human `security.permission.decided` — including boundary-level
  capability grants decided out of loop through `permission_decision`
  (no `effect_id`, never an inbox cue). The spurious cue minted an
  extra `propose` and skewed the shared `next_id` allocator
  (`expected mc-21, got mc-13` → `replay_diverged`). The cue is now
  emitted only when the decided row names an `effect_id` — the same
  condition the boundary submits under (`hh-embed/src/work.rs`).
- `live-capstone.sh` ran only the first leg of a sweep — the leg
  runners' stdin (SDK `npm install` probes, pty cells) ate the leg
  list the driver loop was reading; the inner `live-verify.sh` call now
  rides `</dev/null`.
- `hh acp`'s `--capability` flag is now parsed (`hh-cli/src/lab.rs`) —
  the open-time approval-gated capability the permission round-trip
  needs was previously undeliverable from the verb.
- `replay{driver_mode}` answers a `diverged{at_seq,expected,got}`
  member when the deterministic driver diverges — the honest detail
  that diagnosed the cue bug.

## AC layers

- `live-executed`: the six legs above (each cites its
  `hh-live-evidence/1` record); the ACP leg names `official-sdk`, the
  MCP-lab leg `in-repo` — both verbatim.
- `deterministic`: replay verdicts are the system's own typed verdicts
  (`mode: deterministic`, byte-identical, model.* counts unchanged).
- `agentic`: this report, the readiness flips, and the closeout
  records.

## Spend

`SPEND.jsonl` L4.6a debits: every leg USD 0 (one debit per execution;
re-runs append honestly). Cumulative USD 25.259768 of 100.
`check-live-evidence.sh`: OK — 34 records.
