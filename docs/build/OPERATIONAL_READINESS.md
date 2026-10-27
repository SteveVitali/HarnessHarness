# OPERATIONAL READINESS — capability picture at end of Round 3 (R3.10, 2026-10-16)

> R3.10's honest account of how far each capability climbed by the close of
> Round 3 (R3.1–R3.10 landed on the stacked chain; **GATE-G5 dispositioned
> PASSED 2026-10-16** — operator verbatim "passed, proceed"; the dated
> Signature block of `docs/build/readouts/GATE-G5.md` records 16 carried
> rows CARRY to Round 4, 5 ACCEPT, 0 WAIVE over the 21-row set; the 19 owed
> DEFERRALS rows inherit their BL home's disposition). Status vocabulary:
> `engineered` → `fixture-verified` → `staging-verified` → `live-executed` →
> `public`; `human-completed` marks rows whose only remaining step is a
> person. **One evidence class exists between nothing and the in-tree runs:
> `foreign-run-verified`** — produced by a foreign toolchain's recorded run
> over the committed replay bundle (the PR #136 E2 legs), distinct from and
> never conflated with `fixture-verified`. **(2026-10-21, R4.5a /
> ADR-0363): `live-executed` is now claimable — only with a cited
> `hh-live-evidence/1` record under `docs/build/reports/live/` written by
> `scripts/live-verify.sh`; the first is `2026-10-21-local-reachability.json`
> (local Ollama, USD 0). `staging-verified`/`public` remain unearned until
> L4.6/R4.18.** No production claim is made
> without a citation; where nothing above `fixture-verified` can be cited,
> the row says so. This document supersedes the R2.26 (2026-10-10) account —
> the delta is stated row-by-row below and summarized in the Round-3
> accounting appended to
> `planning/2026-10-14_round3-decomposition.md`.

## Live read of record

- **ci-boundary/1** · `docs/build/logs/ci-R3.10.json` · read_at
  `2026-10-16T20:09:09Z` · result **pass**: PR #146 @ `bdc0a9b`
  (build · test · fmt · drift; the stack read at the GATE-G5
  disposition head — #137–#146 + #135 all OPEN, all `pass`). This row
  names the latest landed-chain read of the code chain.
- Round-3 head of chain: branch `svitali/harnessharness-r3.10` stacked on
  `svitali/harnessharness-r3.9` (R3.10 is docs/records-only); the verified
  ceiling across the stack stays `fixture-verified` — no live read above it
  exists. The workspace's dependency posture changed in-round: `ring =
  0.17.14` is admitted to `hh-secrets` only, under ADR-0359's closed scope
  (DPoP signature verification + test-time fixture signing + the
  `aead`/`rand` surface R3.8 consumed for blob-pool AEAD under ADR-0361) —
  the single named, pinned exception to the hermetic-deps contract, ruled
  by the operator (HUMAN-H5 disposition (b), 2026-10-14). Every other crate
  and every other `ring` surface stays hermetic.

## Capability table

| Capability (R-2.x family) | Code state at end of R3 | Infrastructure needed | Human owner + date | Highest layer | Proof |
|---|---|---|---|---|---|
| Harness assembly (R-2.1.x) | R-2.1.4's emitter-vs-exemption adjudication landed at R3.6 (ADR-0358 — DF-S1.9-2 DONE; C-LINK-8/-9 gained emitters; nine owner rulings; the §3.3.8 `seal`-stage taxonomy gap registered CF-492 → BL-30); R-2.1.4 flipped MET | none beyond toolchain | — | fixture-verified | `hh-assembly` suites; R-2.1.x verdicts in COVERAGE_MATRIX.csv |
| Ledger & recovery (R-2.2.x) | Round-3 delta: DF-S2.3-1's five residual producers landed at R3.2 (BL-44 closed); DF-S1.20-1's control legs at R3.3; R-2.2.3 flipped MET; per-event blob encryption landed at R3.8 (ADR-0361 `HHBE`/`BlobCipher`/`RingBlobCipher`; DF-S1.15-3 DONE; BL-45 closed). Carried: R-2.2.1 stays PARTIAL — OQ-388 ratification is a ruling leg (BL-30), the cross-impl cells ride BL-01/BL-02. **L4.6 (2026-10-23): `hh` ledger `verify` ok + `replay{deterministic}` byte-identical offline on live runs (local ×3 bindings, OpenAI ×3, the `capstone-ledger-replay` leg) — with one surfaced limit: a run whose model dispatched an effect answers `invalid{replay_diverged}` (id-allocator skew vs the live-path ceremony; surfaced for an insert)** | none beyond toolchain | — | live-executed (verify+replay; effect-dispatch replay limit surfaced) + fixture-verified | `hh-ledger`/`hh-env` suites `r3_2.rs`; R-2.2.x verdicts; `hh-live-evidence/1` `2026-10-23-capstone-ledger-replay.json` |
| Model plane (R-2.3.x) | Unchanged in-round; live provider transports owed → BL-31. **L4.6 (2026-10-23): the reference task ran live on all three providers through all three bindings — local Ollama (`capstone-run-local` pass) and OpenAI gpt-4.1-mini (`capstone-run-openai` pass, deterministic replay byte-identical per arm); Anthropic claude-haiku-4-5 executed over all three bindings (calls landed, `usage{available}`) but the hh-cli run's replay diverged — recorded `fail`, surfaced, never claimed (see the L4.6 capstone report's blocker)** | provider endpoints + credentials | provisioner needed before first live call | live-executed (local + OpenAI; Anthropic calls executed, run replay diverged — surfaced) + fixture-verified | `hh-gateway` fake-transport suites; R-2.3.x verdicts; `hh-live-evidence/1` `2026-10-23-capstone-run-{local,openai,anthropic}.json` |
| Context & memory (R-2.4.x) | Round-3 delta: the §5c residual members landed at R3.4 — DF-S1.19-1/-2 DONE, R-2.4.1/2.4.2/2.4.3/2.4.4 flipped MET (BL-15 closed; ADR-0356) | none beyond toolchain | — | fixture-verified | `hh-embed/tests/r3_4.rs`; `hh-context` suites |
| Tools & exposure (R-2.5.x) | Round-3 delta: DF-S1.17-1's residual members landed at R3.5 — `deferred_tools`/`tool_search` flags on the minimal profiles, `lower_provider` deferral block, `surface_rejection_rate` telemetry fold (ADR-0357; BL-16 closed); R-2.5.3 flipped MET. **L4.6 (2026-10-23): sandboxed tool exec on the real `hh-helper` seatbelt backend — `local_sandboxed` attach spawns the helper, exec dispatch streams journaled frames and settles observed, deadline helper-enforced (`capstone-sandboxed-exec` pass)** | sandbox runtime for hosted exec | — | live-executed (local helper) + fixture-verified | `hh-compiler`/`hh-telemetry`/`hh-bench` r3_5 batteries; `hh-live-evidence/1` `2026-10-23-capstone-sandboxed-exec.json` |
| Control plane (R-2.6.x) | Round-3 delta: DF-S1.20-1's interpreter legs landed at R3.3 — delegation_reason on real runs, workflow/program interpreters (BL-14 closed); R-2.6.1/R-2.6.3 flipped MET | none beyond toolchain | — | fixture-verified | `hh-control`/`hh-orchestrator` r3_3 batteries |
| Verification (R-2.7.x) | Unchanged in-round. Carried: judge-binding leg DF-S1.21-3 → BL-17 (env — no live judge admissible); DF-S1.21-2's residual arms are gate-accepted → BL-05. **L4.6 (2026-10-23): judge scoring with the calibrated R4.7e judges executed live — the different-family pair over real providers, `RecordedJudge::from_prefix` replayed the verdict byte-identically (`capstone-judge` pass)** | none beyond toolchain | — | live-executed (judge pair) + fixture-verified | `hh-verification`/`hh-eval` suites; `hh-live-evidence/1` `2026-10-23-capstone-judge.json` |
| Security & governance (R-2.8.x) | Round-3 delta: the credential-broker residual landed at R3.7 — `dpop` verifies over `ring = 0.17.14` (Ed25519 + ES256 fixture-signed proofs on bind/mediate/mint/verify_minted/deliver_wrapped, fail-closed `SenderConstraintUnmet` kept) and `wrapped_long_lived` delivers with durable-before-visible revocation (ADR-0359/0360; DF-S1.13-1/-4 DONE; BL-19 closed); R-2.8.3 + R-2.8.4 read `MET-ENGINEERED(DF-S2.4-2)` — every engineering leg landed, the TLS-terminating transport leg stays env-gated → BL-31. Carried: signer custody ruling (DF-S2.5-1) → BL-30; battery halves → BL-05 | TLS-terminating transport → BL-31 | — | fixture-verified | `hh-secrets/tests/r3_7.rs`; `cap2_dispatch_net_egress_mediated` |
| Telemetry & audit (R-2.9.1, R-2.8.6) | Round-3 delta: per-event blob encryption landed at R3.8 — `ring::aead` AES-256-GCM, per-blob derived keys, fresh nonces; `key_unavailable`/`BlobCorrupt`/`CipherFault` typing, plaintext-address preserved, `audit_view` encryption member (ADR-0361; DF-S1.15-3 DONE; BL-45 closed); R-2.8.6 stays PARTIAL on the custody legs (BL-30/BL-05). Carried: live observability-sink trigger ADR-0346 → BL-31 | exporter endpoint when seam lands | — | fixture-verified | `hh-ledger`/`hh-secrets` r3_8 batteries |
| Lab: bench/eval/results (R-2.9.2–9.8, R-2.10.x) | Unchanged in-round. Carried: live foreign-manifest import → BL-26 (+ ADR-0349); remote fetch + foreign vocabularies → BL-32; `DebtPolicy::default()` conformed to spec at R3.9 (CF-489 resolved — 60 d/30 d). **L4.6 (2026-10-23): `hh bundle create --sink` → `validate` → `reproduce` on a clean store of a live run — verdict `reproduced` at the bundle's `max_supported_level: R0` (`capstone-bundle-repro` pass)** **L4.6a (2026-10-23): the three packaged variants executed live out of process through `hh-varhost` + `hh-helper` over `plugin_abi/1` — `hh-memory-store` (memory_abi conformance battery + typed `UnknownName` refusal), `hh-compact-evict-oldest` (assess→propose→execute, host-stamped provenance, `remaining_tokens ≤ cap`), `hh-tracker-fixture` (capabilities→occurrences→activate_run→list/get); each leg cites durable `lifecycle.component.*` rows — `plugin-memory-store`/`plugin-compact-evict-oldest`/`plugin-tracker-fixture` pass** | remote endpoints + foreign vocab corpora | — | live-executed (bundle reproduce; packaged variants OOP) + fixture-verified | `hh-bench`/`hh-eval`/`hh-results`/`hh-embed`/`hh-varhost` suites; `hh-live-evidence/1` `2026-10-23-capstone-bundle-repro.json`, `2026-10-23-plugin-{memory-store,compact-evict-oldest,tracker-fixture}.json` |
| Surfaces (R-2.11.x) | Unchanged in-round. Carried: oauth/mtls mediator legs → BL-27 (+ ADR-0303/0351); R-2.11.3 stays PARTIAL on DF-S4.11-2. **L4.6 (2026-10-23): `hh` ran the reference task live over binding (b) stdio and binding (c) loopback HTTP on local + OpenAI (+ Anthropic calls; the generated client + renderer, no kernel semantics); `hh-web` served the live run — `GET /` 200 + `POST /api {run_index}` 200 carrying the run (`capstone-web` pass); `hh-mcp-serve` driven by a real MCP client — the official SDK refused the server's protocol-era pin `2026-07-28`, so the in-repo `McpClient` ran negotiate/`tools/list`/`tools/call` to a typed `stage_pending` refusal (`capstone-mcp` pass, `client=in-repo`)** **L4.6a (2026-10-23): `hh acp` served a live run over real ACP stdio on a pty — the official `@zed-industries/agent-client-protocol` client ran initialize → session/new → session/prompt → `session/request_permission` → `decided{allow}` → `state.turn.completed`, `client=official-sdk`; `hh-mcp-lab` drove Lab ops live over loopback HTTP — official SDK refused the `2026-07-28` pin, in-repo `McpClient` ran (`client=in-repo`); the public SDK/embed API ran hello → open_session → submit → `model.call.*` over stdio `serve`, `serve --http`, and an out-of-workspace consumer crate (`embed-sdk-live`); every leg `verify` ok + `replay{deterministic}` offline, USD 0 local lane** | none beyond toolchain | — | live-executed (hh/hh-web/hh-mcp/hh-acp/hh-mcp-lab/sdk-embed) + fixture-verified | `hh-cli`, `hh-embed`, `hh-mcp-lab`, `hh-acp` suites; `hh-live-evidence/1` `2026-10-23-capstone-{web,mcp,run-local,run-openai}.json`, `2026-10-23-{surface-acp,surface-mcp-lab,embed-sdk-live}.json` |
| Conformance (R-2.12.1–12.3) | Round-3 delta: the E2 candidate (`foreign/e2-python`) replayed all six `hh-xcheck-bundle/1` arms byte-for-byte via `hh-xcheck verify` — 49/49 (PR #136, 2026-10-14; HUMAN-H3 material provided): identity-trees 22/22, identity-records 4/4, ledger-transcript 3/3, registry-snapshot 1/1, plugin-manifests 14/14, canonical-parse 6/6 incl. timing members — **`foreign-run-verified` evidence on the five covered DEFERRALS rows, never a fixture/native claim**. Still env-pending: E3 replay, the HIR/ledger-corpus extension, the two-toolchain C12 cells → BL-01 (gate-accepted item 9); R-2.12.1 stays PARTIAL. The bundle's status vocabulary cannot yet express a partially-foreign-discharged cell — CF-493 registered → BL-30 | foreign toolchains (E3; E2's remaining legs) | human reviewer for R2 signature → BL-02 | fixture-verified (in-tree) + foreign-run-verified (E2 legs) | `hh-xcheck` bundle + dated DEFERRALS notes (PR #136); `hh-embed/tests/conformance.rs` |
| Packaging/licensing (R-2.12.4) | Spec-deferred package (ADR-0210) → BL-33; unchanged in-round | legal counsel | counsel at WS-L6 | engineered | ADR-0210 + scope row |
| Naming thesis (R-2.12.5) | Program-level thesis landed; market check deferred — unchanged; CF-489's conformance divergence **resolved at R3.9** (code conformed to spec) | naming/legal search at WS-L6 | operator at WS-L6 | engineered | ADR-0003/0006 |
| Fleet/organizational (R-2.12.6) | Adapter + fixture legs landed; **R4.7 (2026-10-22): the real GitHub Issues adapter + signed-webhook legs executed live on the private sandbox repo** — `github-live` pass (`hh-live-evidence/1` 2026-10-22-github-live.json: webhook create/delete verified, signed `X-Hub-Signature-256` delivery → `fleet.webhook_ingress` → `fleet.reconcile` → WorkItem `dispatching`, dedupe + stale no-op, OOP `subprocess_confined` probe/list/get; ADR-0369; DF-S5.6-1 DONE; BL-04 closed; GATE-ACCEPT deviation 6 discharged). **L4.6 (2026-10-23): the same loop re-executed as the capstone's fleet leg through the cloudflared quick tunnel — `capstone-fleet-webhook` pass, trap-guarded rollback** | none for the R4.7 scope | — | live-executed (sandbox) + fixture-verified | S5.5/S5.6 signed-run records + the `github-live` + `2026-10-23-capstone-fleet-webhook.json` evidence records |
| Registers & spec-debt (ADR-0212–0216) | Unchanged in-round — all stay open at the named homes BL-35/36/37/39 (interim rules in force); OQ-441's Stage-5 `DebtIndex` re-pin stays with WS-I6 | none beyond toolchain | — | fixture-verified | `registers/open-questions.md`, `spec-debt.md` dated sweep sections |

## Ordered critical path to a production claim

Every step names its owning item (`ticket:`) and the evidence that would
close it (`proof:`). Nothing below asserts a claim that lacks a citation.
The ordering is the dependency chain, not a schedule.

1. `ticket:` BL-02 — R2 cross-camp human reviewer signature. `proof:` signed
   reviewer record appended to the S0.3 measurement sheet.
2. `ticket:` BL-01 + BL-26 — E3 byte-equality cells + the HIR/ledger-corpus
   extension + the C12 two-toolchain cells, and the live foreign-manifest
   import (HUMAN-H3 environment; the E2 legs already carry
   foreign-run-verified evidence per PR #136). `proof:` cross-ecosystem
   conformance run over the R2.21 bundle for E3 with `EXIT=0`;
   foreign-manifest ingest run record.
3. `ticket:` BL-31 + BL-27 — live provider transports, the TLS-terminating
   transport (DF-S2.4-2 — the residual R-2.8.3/R-2.8.4 both carry),
   oauth/mtls mediator legs. `proof:` live adapter impls + a probe-run
   record {id, `date -u`, result, sha256} against a real endpoint.
4. `ticket:` BL-03 — E3 surface-ecosystem binding (HUMAN-H1). `proof:` real
   E3 client bound to the generated surface + drift-check run record.
5. `ticket:` BL-04 — real tracker + signed webhook (HUMAN-H2). `proof:` AC-9
   replayed against the live source with a signed-event record.
6. `ticket:` BL-17 — live-judge admissibility + judge-binding legs.
   `proof:` a live-judge run record admitted under ADR-0347/0348's
   conditions.
7. `ticket:` BL-05 — gate-accepted executable-battery arms (live corpus/OOP
   arms signed at GATE-ACCEPT item 10). `proof:` the arms executed against
   live fixtures when the environments in steps 1–6 exist.
8. `ticket:` BL-30 — spec/governance rulings (REC.2). `proof:` the named
   spec-ruling cells written into the registers with ADR cross-refs
   (OQ-170 custody, OQ-388 ratification, CF-476/487/488/490/491,
   CF-492 `seal`-stage taxonomy, CF-493 `pending_cells`/`arms[].status`
   vocabulary).
9. `ticket:` BL-32 — remote fetch + foreign vocabularies (REC.3). `proof:`
   foreign-manifest ingest run + fetch-transport probe record.
10. `ticket:` BL-33/34/38 + BL-35/36/37/39 — WS-L6 window + register
    packages' remaining open questions. `proof:` program-resume decision
    record + per-OQ answered rows.
11. `ticket:` BL-41/42/43 — standing ledgers (re-issued, never closed).
    `proof:` the standing rules honoured — no proof owed.

## What is *not* claimed

- No `staging-verified`, `live-executed`, or `public` row exists: the build
  ran offline/hermetic by design and the only live read of record is the
  CI-boundary record above, which attests to the stacked PRs' checks — not
  to production execution. Round 3 landed no claim above `fixture-verified`.
- The `foreign-run-verified` E2 evidence (PR #136) is a foreign toolchain's
  recorded replay of the committed bundle — it is not an in-tree
  verification, not a live-service claim, and does not lift the
  `fixture-verified` ceiling; the E3/C12/corpus-extension legs remain
  `environment-pending`.
- No live provider call, live TLS termination, live judge, real tracker or
  signed webhook, real OAuth/mTLS exchange, E3 binding, or live
  foreign-manifest import is claimed — each names its provisioning
  precondition on BL-01…BL-04, BL-17, BL-26, BL-27, BL-31.
- No secret material is recorded; every `provided:` cell stays `yes/no`
  (HUMAN-H3 material was *provided* as the PR #136 run record; no
  credential entered a ledger).
- The environment-bound rows (BL-01, BL-02, BL-03, BL-04, BL-26, BL-27,
  BL-31) are honest carried debt, not defects: each names its provisioning
  precondition — they read *excluded pending HUMAN-H1/H2/H3 provisioning*
  (the paths are live, nothing is provisioned yet), never a fabricated
  pass. The round-3 round-closure set — the discharged rows
  BL-10/14/15/16/19/44/45, the rulings on BL-30, the env/human/accepted
  residuals, and the standing rows BL-41/42/43 — was enumerated with
  reason codes in the round-3 accounting
  (`planning/2026-10-14_round3-decomposition.md`) and dispositioned at
  GATE-G5 (2026-10-16: 16 CARRY / 5 ACCEPT / 0 WAIVE).

*Amended in place 2026-10-16 (R3.11):* the GATE-G5 outcome and the
latest ci-boundary read are now recorded above; every capability row
otherwise stands as written at R3.10.

*Amended in place 2026-10-23 (L4.6 — the live E2E capstone):* the
reference task executed live through the shipped `hh` surface over all
three `hh-embed/1` bindings on local Ollama and OpenAI (Anthropic calls
landed over all three bindings; the leg's run replay diverged —
recorded and surfaced, never claimed). Sandboxed tool exec ran on the
real `hh-helper` seatbelt backend; judge scoring ran with the
calibrated R4.7e judges; `hh` `verify` + `replay{deterministic}`
replayed live runs byte-identically offline wherever the run carried no
effect dispatch — **surfaced finding**: a tool-dispatching live run
answers `invalid{replay_diverged}` (3/3 reproductions; id-allocator
skew against the live-path permission/authorize ceremony — surfaced
for an insert, per the ticket's out-of-scope rule). `hh-web` served the
live run with the HTTP read asserted; `hh-mcp-serve` was driven by a
real MCP client (in-repo client — the official SDK refuses the server's
`2026-07-28` protocol-era pin); the fleet webhook→reconciler loop
re-executed on `SteveVitali/hh-live-sandbox` through a cloudflared
tunnel with trap-guarded rollback; `hh bundle` export reproduced the
live run on a clean store at R0. Nine committed `hh-live-evidence/1`
records back these rows (`docs/build/reports/live/2026-10-23-capstone-*.json`)
plus one honest `fail` record for the anthropic replay divergence;
`check-live-evidence.sh` green; cumulative live spend USD 25.259768 of
100. `staging-verified` and `public` remain unearned; the production
surface claims stay exactly as scoped above — every executed leg is a
loopback/sandbox/private-credential surface.

*Amended in place 2026-10-23 (R4.16 — round-4 closure sweep):* this
amendment reconciles the R3-era prose above against the R4/L4 evidence
that landed after it. Two statements in the header and in "What is *not*
claimed" are stale and are corrected here (append-only — the originals
stand for the record): (a) "the verified ceiling … stays
`fixture-verified`" — superseded: `live-executed` was earned at R4.5b
onward under ADR-0363 (`hh-live-evidence/1` records only), and the L4.6 /
L4.6a capstone + surface sweep carried most public surfaces to
`live-executed`; (b) "No live provider call, live TLS termination, live
judge, real tracker or signed webhook, real OAuth/mTLS exchange, E3
binding, or live foreign-manifest import is claimed" — every item on
that list has since executed live with a cited record (provider calls:
`2026-10-21-{local,openai,anthropic}-call` + the capstone runs; TLS:
`2026-10-21-https-reachability`; judge: `2026-10-22-judge-{calibrate,run}`
+ `capstone-judge`; tracker + signed webhook: `2026-10-2{1,2}-github-live`
+ `capstone-fleet-webhook`; OAuth device flow: `2026-10-22-oauth-device`;
E3 binding: the generated `clients/ts` client bound at R4.2 and the
surface legs execute over it; foreign-manifest import:
`2026-10-21-bench-manifest-live`). What remains true: no
`staging-verified` or `public` claim exists anywhere; every live leg is a
loopback/sandbox/private-credential surface.

## Public operational surface — the R4.16 sweep (2026-10-23)

The full inventory (`cargo metadata --no-deps` + `ls plugins/`), each row
at its honestly-claimed highest layer. `live-executed` rows cite the
`hh-live-evidence/1` record under `docs/build/reports/live/` produced by
`scripts/live-verify.sh` (ADR-0363). Internal tooling binaries
(`hh-codegen`, `hh-compile`, `hh-authorize`, `hh-bench-adapter`,
`hh-eval-oracle`, `hh-gateway-stub`, `hh-plugin-check`,
`hh-plugin-fixture`, `hh-secret-scan`, `hh-ir-op`, `hh-xcheck`) are
dev/CI instruments, not public surfaces — `fixture-verified` under the
workspace suite, no live claim owed.

| Public surface | Form | Highest layer | Evidence |
|---|---|---|---|
| `hh` CLI (generated boundary client; `serve`, `acp`, `lab serve`, `bundle`, `verify`, `replay`, `ledger` verbs) | bin `hharness-cli` | live-executed | `2026-10-23-capstone-run-{local,openai}.json`, `2026-10-23-embed-sdk-live.json`, `2026-10-23-quickstart-live.json` |
| `hh-kernel serve` — binding (b) stdio JSON-RPC | bin `hharness-kernel` | live-executed | capstone-run legs over binding (b); `embed-sdk-live` |
| `hh-kernel serve --http` — binding (c) loopback HTTP | bin `hharness-kernel` | live-executed | capstone-run legs over binding (c); `surface-mcp-lab` |
| binding (a) in-process `EmbedService` | lib `hharness-embed` | live-executed | `2026-10-23-embed-sdk-live.json` (out-of-workspace consumer crate) |
| `hh-web` read-only web instrument | bin `hharness-web` | live-executed | `2026-10-23-capstone-web.json` |
| `hh-mcp-serve` MCP protocol edge | bin `hharness-mcp` | live-executed | `2026-10-23-capstone-mcp.json` (`client=in-repo` — official SDK refuses the `2026-07-28` protocol-era pin; surfaced) |
| `hh lab serve` (`hh-lab/1` MCP surface) | lib `hharness-mcp-lab` via `hh` | live-executed | `2026-10-23-surface-mcp-lab.json` (`client=in-repo`, same pin limit) |
| `hh acp` ACP session boundary | lib `hharness-acp` via `hh` | live-executed | `2026-10-23-surface-acp.json` (`client=official-sdk` — `@zed-industries/agent-client-protocol`, full exchange incl. `session/request_permission`) |
| Public SDK / embed API | libs `hharness-embed`, `hharness-embed-client-generated` | live-executed | `2026-10-23-embed-sdk-live.json` (all three bindings + foreign consumer crate) |
| Generated TypeScript client `clients/ts` | generated artifact (HUMAN-H1 "Confirm TypeScript"; ADR-0050 log) | foreign-run-verified | e3-smoke 17/17 over spawned `serve --http`; `hh-codegen/tests/r4_2.rs`; drift gate CC7 |
| `hh-memory-store` packaged variant | plugin (`plugins/hh-memory-store`) OOP via `hh-varhost`+`hh-helper`/`plugin_abi/1` | live-executed | `2026-10-23-plugin-memory-store.json` |
| `hh-compact-evict-oldest` packaged variant | plugin OOP | live-executed | `2026-10-23-plugin-compact-evict-oldest.json` |
| `hh-tracker-fixture` packaged variant | plugin OOP | live-executed | `2026-10-23-plugin-tracker-fixture.json` |
| `hh-github-issues` packaged variant (real adapter + signed webhook) | plugin OOP `subprocess_confined` | live-executed | `2026-10-2{1,2}-github-live.json`, `2026-10-23-capstone-fleet-webhook.json` (ADR-0369) |
| Provider lane — local Ollama | `hharness-gateway` transport | live-executed | `2026-10-21-local-{reachability,run}.json`, `capstone-run-local` |
| Provider lane — OpenAI | `hharness-gateway` transport | live-executed | `2026-10-21-openai-call.json`, `capstone-run-openai` (replay byte-identical) |
| Provider lane — Anthropic | `hharness-gateway` transport | live-executed — calls landed, `usage{available}`; run replay diverged (recorded `fail`, surfaced, never claimed deterministic) | `2026-10-21-anthropic-call.json`, `capstone-run-anthropic` |
| TLS-terminating transport | `hharness-tls`/`hh-env` `LocalTlsTransport` (ADR-0365 rustls graph) | live-executed | `2026-10-21-https-reachability.json` |
| Sandboxed tool exec (helper, seatbelt backend) | `hharness-env`/`hh-helper` | live-executed | `2026-10-23-capstone-sandboxed-exec.json` |
| Judge pipeline (calibrated LiveJudge/PanelJudge pair) | `hharness-embed::judge_adapters` | live-executed | `2026-10-22-judge-{calibrate,run}.json`, `capstone-judge` |
| Ledger verify + deterministic replay | `hh` `verify`/`replay` | live-executed | `capstone-ledger-replay`; **limit surfaced**: tool-dispatching runs answer `invalid{replay_diverged}` (id-allocator skew, 3/3 reproductions) |
| Bundle export → reproduce | `hharness-bundle`, `hh bundle` | live-executed (R0) | `2026-10-23-capstone-bundle-repro.json` |
| OAuth device flow / mediator legs | `hharness-secrets`/`hh-monitor` | live-executed | `2026-10-22-oauth-device.json` |
| Observability sink (otel) | `hharness-telemetry` exporter | live-executed | `2026-10-22-otel-live.json` |
| Remote fetch transports | `hharness-bundle` `FetchTrustPolicy` | live-executed | `2026-10-22-r4-15d-fetch.json` |
| Foreign-manifest import | `hharness-bench` | live-executed | `2026-10-21-bench-manifest-live.json` |
| `foreign/e2-python` (E2 candidate) | foreign toolchain replay of `hh-xcheck-bundle/1` | foreign-run-verified | PR #136, 49/49 on the six base arms; **extension arms unimplemented → environment-pending** (DF-S1.2-2/DF-S1.5-3 stay OPEN — GATE-G6 findings) |
| `foreign/e3-typescript` (E3 candidate) | foreign toolchain replay | foreign-run-verified | R4.4b: 57/57 across all eight arms incl. HIR documents + ledger scenarios |
| Quickstart (public on-ramp) | `scripts/` + `hh doctor` path | live-executed | `2026-10-23-quickstart-live.json` |

**Carried (honest) residuals after R4.16:** `staging-verified` and
`public` unearned (no staging environment is provisioned; R4.18 owns the
publish decision). The e2-tree extension cells `hir-documents.e2-replay`
and `ledger-scenarios.e2-replay` (`bundle.json` `verified_by[]` empty —
`foreign/e2-python` does not implement those arms) remain
`environment-pending` on BL-01 (`accepted`) as DF-S1.2-2/DF-S1.5-3 —
surfaced as GATE-G6-blocking findings, not silently closed. The
`replay_diverged` limits (Anthropic run; tool-dispatch effect paths) are
surfaced findings per the capstone report. Live spend: USD **25.259768**
of the USD 100 cap (90 `SPEND.jsonl` rows; R4.16 itself spent USD 0 —
offline ticket). `check-live-evidence.sh` green over 34 records.
