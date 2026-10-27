# L4.6 — Live E2E capstone report (2026-10-23)

Ticket `164u` (`docs/tickets/164u_L4.6__live-e2e-capstone.md`). The
round-4 charter's "deep end to end live verification": the reference
task end-to-end through the shipped `hh` surface over all three
`hh-embed/1` bindings, every provider, plus the supporting live legs —
all executed through `scripts/live-verify.sh` under
`scripts/live-capstone.sh` (each leg independently re-runnable;
`scripts/live-capstone.sh --list` prints the leg order + declared
budgets). Secrets were referenced by NAME only; every record carries
`provided: yes/no` and no credential value.

**Cumulative live spend at report time: USD 25.259768** (global cap
USD 100). L4.6 debits total **USD 4.002063** against the USD 30
allocation — USD 4.000000 of that is the *declared-bound* debit on the
two failed `capstone-run-anthropic` attempts (the runs panicked before
the aggregate `LIVE_USAGE` marker printed, so the legs debited their
declared USD 2 bound rather than the ~USD 0.0002 measured scale; the
debit is a conservative upper bound, never an under-report).

## Leg ledger

| Leg | Record (`docs/build/reports/live/`) | Result | Cost (USD) |
|---|---|---|---|
| capstone-run-local | `2026-10-23-capstone-run-local.json` + transcript | **pass** — reference task over binding (a) in-process, (b) real `hh` CLI, (c) `serve --http` + NetClient; `verify` ok; `replay{deterministic}` byte-identical, zero provider re-calls on every arm | 0 |
| capstone-run-anthropic | `2026-10-23-capstone-run-anthropic.json` + transcript | **fail:live-tests rc=101** — all three bindings executed the reference task live (4 `model.call.*` rows, `usage{available}`); `hh-cli-attach` arm's replay returned `invalid{replay_diverged}` — see the finding below | 2.000000 (declared bound) |
| capstone-run-openai | `2026-10-23-capstone-run-openai.json` + transcript | **pass** — gpt-4.1-mini-2025-04-14 over all three bindings + cross-binding attach-verify; `verify` ok; `replay{deterministic}` byte-identical per arm | 0.000158 (measured) |
| capstone-sandboxed-exec | `2026-10-23-capstone-sandboxed-exec.json` + transcript | **pass** — real `hh-helper` seatbelt backend: `local_sandboxed` attach spawns the helper, exec dispatch streams journaled frames and settles observed, deadline helper-enforced | 0 |
| capstone-judge | `2026-10-23-capstone-judge.json` + transcript | **pass** — judge scoring with the calibrated R4.7e pair (different-family judges over real providers); `RecordedJudge::from_prefix` replays the verdict byte-identically | 0.001905 (measured) |
| capstone-ledger-replay | `2026-10-23-capstone-ledger-replay.json` + transcript | **pass** — `hh` run over binding (b), attach over (c): `verify` verdict ok + `replay{deterministic}` byte-identical offline, model.* row count unchanged | 0 |
| capstone-web | `2026-10-23-capstone-web.json` + transcript | **pass** — `hh-web` serving the live run over binding (c): `GET /` shell 200; `POST /api {op:run_index}` 200 carrying the live run id — the HTTP read asserted | 0 |
| capstone-mcp | `2026-10-23-capstone-mcp.json` + transcript | **pass** — `hh-mcp-serve` driven by a real MCP client; `client=in-repo` — see the client note below | 0 |
| capstone-fleet-webhook | `2026-10-23-capstone-fleet-webhook.json` + transcript | **pass** — R4.7's full sandbox loop re-executed under L4.6: cloudflared quick tunnel → signed `X-Hub-Signature-256` ingress → `fleet.reconcile` → WorkItem; trap-guarded rollback (hook deleted, test issue closed) on `SteveVitali/hh-live-sandbox` | 0 |
| capstone-bundle-repro | `2026-10-23-capstone-bundle-repro.json` + transcript | **pass** — `hh bundle create --sink` → `validate` (`max_supported_level: R0`) → `reproduce` on a clean store at R0: `outcome: pass`, `verdict: reproduced` | 0 |

## MCP client note (ticket's "otherwise" clause)

The leg first installed the official `@modelcontextprotocol/sdk` into a
temp dir (runtime-only, never a repo dep — 95 packages, ~2 s) and drove
`hh-mcp-serve` with `scripts/live-capstone-mcp.mjs`. The SDK client
connected and then refused: `Server's protocol version is not
supported: 2026-07-28` — `hh-mcp-serve` negotiates the workspace's
protocol-era pin, which the released SDK's supported-version list does
not contain. Per the ticket ("official SDK client if installable
without new repo deps — else the in-repo client, stated"), the leg fell
back to the in-repository `hharness_mcp::client::McpClient`, which ran
the full negotiate → `tools/list` (`l46.echo` carried) → `tools/call`
exchange and received the honest typed refusal `stage_pending`
("execution is not a Stage-3 verb"). The record's assertions name
`client=in-repo`. The SDK refusal is also honest evidence about
protocol-era interop — recorded, not hidden.

## Replay byte-identity verdicts

| Run | verify | replay |
|---|---|---|
| local ×3 bindings + attach | ok | `deterministic` — byte-identical, no provider re-call |
| openai ×3 bindings + attach | ok | `deterministic` — byte-identical |
| anthropic in-process | ok | `deterministic` — byte-identical |
| anthropic hh-cli (attach arm) | ok | **`invalid{replay_diverged}`** |
| anthropic http | ok | (not reached — the leg halted at the hh-cli-attach arm) |
| ledger-replay leg run | ok | `deterministic` — byte-identical |
| judge (RecordedJudge fold) | — | byte-identical |
| bundle reproduce, clean store | — | `reproduced` at R0 |

## SURFACED BLOCKER / FINDING — deterministic replay of an effect-dispatching live run

Reproduced 3/3 on the Anthropic lane: when the live run's model
proposed a tool call — the record carries `action.tool.proposed` +
`action.effect.{intended,decided,authorized,prepared,committed,observed}`
+ `security.permission.decided` + the `verification.claim/completion`
chain (13 dispatch-chain rows) — `replay{driver_mode: deterministic}`
answers `invalid` with `reasons: ["replay_diverged"]`. Runs without a
dispatch replay byte-identically on every lane (local ×3, openai ×3,
anthropic in-process on the third attempt, the ledger-replay leg).

Mechanism (diagnosed from the recorded run's `events.wal`): the model
port serves recorded calls by `model_call_id`, and `mc-N`/`tc-N`/`ef-N`/
`d-N`/`e-N` share one monotonie allocator (`hh-control` Driver
`next_id`). In the window between the recorded `mc-8` and `mc-27` the
live drive minted the live-path ceremony events — `security.permission.decided`,
`action.effect.authorized`, `action.effect.prepared` — that the
ReplayGate (which serves recorded terminals, executor-call count 0 by
construction) does not re-mint; the re-drive's next call therefore
allocates `mc-23` where the record holds `mc-27` (~4-id skew) and the
port reports the first divergence honestly. Whether the re-drive *must*
re-emit the ceremony or the port *should* match calls positionally is a
kernel-contract question — beyond a small test-backed fix, so per the
ticket's out-of-scope rule it is surfaced here for an insert, not fixed
in this ticket.

Evidence: `2026-10-23-capstone-run-anthropic.transcript.txt` (the
verdict payload at the panic) plus the two earlier divergent attempts
recorded in `SPEND.jsonl`. The AC "replay of each live run is
byte-identical offline" is therefore **not met on evidence** — this is
the ticket's surfaced blocker for the operator's ruling.

> **2026-10-23 — DISCHARGED by L4.6b (ADR-0381, leg (a)).** The operator
> ruling inserted L4.6b and the fix landed on
> `svitali/harnessharness-l4.6b`: `extract_recorded` reclassifies the
> recorded gate-`emitted` ceremony rows into the replay lattice and
> `ReplayGate` re-serves them verbatim via `GateOutcome.emitted`, so the
> driver re-appends them through `append_producer` and the shared
> `next_id` allocator mints the same `e-N`s — every later `mc-N`/`d-N`
> matches the record. The rejected alternative — positional/by-content
> matching of recorded terminals (leg (b)) — is named in ADR-0381: it
> would conceal the allocator-contract divergence the report exists to
> surface. Verified by committed tests that fail if removed
> (`ceremony_dispatch_run_replays_byte_identical` + two loud-divergence
> mutations in `hh-control`; `l46b_effect_dispatch_replay_byte_identical`
> in `hh-embed`, writer + read-only attach arms, WAL-mutation arm) and
> by the USD-0 live leg `l4-6b-replay`
> (`docs/build/reports/live/2026-10-23-l4-6b-replay.json`) — the diverged
> arm's shape re-run over binding (c): `deterministic`, byte-identical,
> no `replay_diverged` reason.

## AC layers

- `live-executed`: the nine passing legs above (each cites its
  `hh-live-evidence/1` record); the anthropic leg executed live but its
  run's replay diverged — recorded, surfaced, never claimed.
- `deterministic`: the replay verdicts are the system's own typed
  verdicts, reported verbatim.
- `agentic`: this report, the readiness flips, and the closeout records.

## Spend

`SPEND.jsonl` L4.6 debits: `capstone-run-local` ×3 0,
`capstone-sandboxed-exec` ×2 0, `capstone-ledger-replay` 0,
`capstone-web` ×2 0, `capstone-bundle-repro` ×2 0, `capstone-mcp` 0,
`capstone-run-anthropic` ×2 2.000000 (declared bound — measured usage
unavailable on the panicked runs), `capstone-run-openai` 0.000158,
`capstone-judge` 0.001905, `capstone-fleet-webhook` 0. Cumulative
USD 25.259768 of 100. `check-live-evidence.sh`: OK — 26 records.

## Follow-on (appended 2026-10-23)

L4.6a's surface & plugin sweep — `hh acp`, `hh-mcp-lab`, the SDK/embed
library API, and the three packaged variants OOP — is recorded in
`2026-10-23-live-e2e-surfaces.md` (6/6 pass, USD 0).
