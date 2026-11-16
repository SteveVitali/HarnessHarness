# 01 — Diagnosis: why the H-allow re-proof has not converged (Wc → Wc7), and what is still broken

Author: an independent Claude Code session (Opus), read-only audit of `~/MetaHarness-harnessharness`
at `origin/main` `9a415e08` (2026-11-15 ~20:30Z). Line anchors are as of that commit. **Re-confirm every
anchor at execution (XO-01)**; reproduce every PREDICTED item before fixing it.

Confidence labels:
- **CONFIRMED**: code read and recorded evidence agree.
- **CODE-CONFIRMED / PREDICTED**: the mechanism is in the code; the failure has not yet been observed in a window
  (in most cases because an earlier defect masked it).
- **LOW**: plausible, cheap to fix, unproven.

---

## 1. Why the loop doesn't converge

1. **Only HH's own machinery fails.** In every window since Wc4, all 14 members pass **O** (Harbor) and **H-pub**
   (HH cells, `declared_deps`) except `query-optimize` (see D15/D17). Only **H-allow** (`allowlist_mediated`) fails, with
   `denied_hosts: []`. The failing component is therefore HH's egress path (cell → `hh-cell-relay` →
   `hh-egress-mediator` → upstream). It is not the tasks and not the allowlist data (one exception, D16).
2. **The verifier bootstraps are serial and shared.** Every member's `tests/test.sh` runs the same chain: `apt-get update` →
   `apt-get install -y curl` → `curl -LsSf https://astral.sh/uv/0.9.5/install.sh | sh` (which follows a GitHub
   release redirect) → `uvx -p 3.13 -w pytest… pytest …` (uv downloads a managed CPython plus wheels, concurrently).
   One defect in the egress path fails all 14 members the same way, and it **hides the next defect in the
   chain**. Each repair moved the failure one step further along: proxy scope → mediated egress → kill → verifier sink →
   allowlist hosts → duplicate `Host` → 8 MiB body cap. As of `abc8d850`, apt should get through. The **next** steps
   (curl's cross-host redirect, uv's concurrent HTTPS) exercise code paths that have **never yet run under the mediator in
   a window**. §2 shows they are broken too.
3. **The debugging instrument is the 5–8 h, ~38-trial evidence window, with in-window fixes forbidden.** Every
   defect therefore costs a full window plus a new gate (about a day of wall time). The defects found so far are ordinary
   HTTP-proxy bugs that a real `apt`/`curl`/`uv` client run through the mediator would expose in minutes. The
   hermetic batteries use synthetic fixtures (e.g. a 16 MiB fixture body), not real clients.
4. **The gates allow no other choice.** Each PROV readout offers only `run | not_run`, and each predicts success
   ("Wc7 proved the machinery completes … the `max_body` cap was the last confirmed blocker"). That prediction has
   been wrong seven times.

---

## 2. Defect register (continuing the session's D1–D7 numbering)

D1–D7 are the session's own: the scope-mint, mediated-egress, `pkill`, verifier sink, apt hosts, duplicate `Host`,
and 8 MiB body cap defects. All are repaired.

### D8 — The mediator serves one connection at a time · CODE-CONFIRMED / PREDICTED high
`crates/hh-env/src/bin/hh_egress_mediator.rs:278`: `for conn in listener.incoming() { … serve_conn(stream, …) }` is
synchronous. A CONNECT tunnel (`forward_tunnel`) blocks the mediator for the tunnel's **whole lifetime**. The in-cell
relay (`crates/hh-helper/src/bin/hh_cell_relay.rs`) *is* concurrent (one thread per connection), so later connections
queue in the mediator's accept backlog.

How it fails: curl `-L` keeps its first connection (astral.sh) pooled while it opens a second CONNECT to github.com
/ release-assets. uv keeps idle pooled connections (≈90 s) to pypi.org while it opens files.pythonhosted.org and github.com. The
second CONNECT waits until the client closes the first. curl waits up to its default 300 s connect timeout, longer than
much of the verifier budget. uv times out (`UV_HTTP_TIMEOUT` 30 s) and retries. Exact client timing is a hypothesis;
the conformance harness measures it.

Signature to expect: H-allow verifier walls inflated by ~30–600 s; `verifier_unavailable:verifier_timeout`, or uv
`Failed to download` → `reward 0`.

Corroboration: Wc2, Wc3 and Wc5 hit their 3 h, 5 h and 6 h dead-man; the "~1 h+ slow-member stretches"
(pypi-server H-allow, query-optimize) are recorded in `docs/build/runs/R6.45d.md`.

### D9 — The tunnel pump aborts on 5 s of upstream silence, doesn't propagate half-close, and swallows errors · CODE-CONFIRMED
- `crates/hh-env/src/egress.rs:251` gives `LocalHttpTransport` a 5 s read/write timeout, and `dial` (`:256` ff.) applies
  it to the **tunnel** upstream socket.
- In `forward_tunnel` (`:1664` ff.), `std::io::copy(&mut up_reader, client).unwrap_or(0)` (`:1712`) returns on the first
  5 s idle read (`WouldBlock`/`TimedOut`). The error is swallowed and `bytes_down` is recorded as 0.
- The mediator then blocks on `up.join()` until the **client** closes its side. The client is never shut down, so it
  never sees EOF; under D8 the whole mediator is stuck.
- `close_cause` is hard-coded `"eof"`.

How it fails: any server think-time or idle pooled connection over 5 s kills downstream delivery while leaving the
client hanging. It also falsifies the `security.egress.tunnel.closed` row (bytes 0, cause eof).

### D10 — Response headers forwarded verbatim; framing duplicated; no `Connection: close` downstream · CODE-CONFIRMED
`serve_conn` (`hh_egress_mediator.rs:322` ff.) relays `response.headers` unfiltered into `hharness_wire::http::write_response`
(`crates/hh-wire/src/http.rs:482`). That function **appends** `Content-Length` after whatever headers it was given.

Consequences:
- Upstream `Content-Length` is emitted twice.
- If upstream answered `Transfer-Encoding: chunked`, the client gets `TE: chunked` plus `Content-Length` plus an
  already-de-chunked body. TE wins, so the client mis-parses.
- Upstream `Connection`/`Keep-Alive`/`Trailer` pass through.

The request side was fixed at D6 (`push_end_to_end_headers`); the **response side was never filtered**. The mediator
serves exactly one request per client connection but never says `Connection: close`. A client that reuses a persistent
connection (apt and curl both try to) finds it closed under it: it either retries, at a cost in wall time, or fails,
depending on the client and timing.

### D11 — The codec reads a body for bodiless responses · CODE-CONFIRMED / PREDICTED medium
`body_mode` (`http.rs:213`) considers only `Transfer-Encoding`/`Content-Length`, never the request method or the
status. A `HEAD` reply carrying `Content-Length: N`, or a 204/304, makes `read_response_full` wait for N bytes. It then
times out (5 s) → `EgressError::Transport` → **502**. RFC 9112 §6.3: responses to HEAD, and 1xx/204/304, have no body.
The codec is shared with the model proxy, so the fix must keep the proxy's batteries green.

### D12 — Forward-path upstream timeouts are 5 s · CODE-CONFIRMED / PREDICTED low–medium
The same 5 s timeout bounds the forward path's first byte and each read. A slow mirror or a large index generating
for over 5 s gives a 502. A first-byte timeout ≥ 60 s with a typed **504** (not 502) is the norm for a forward proxy.

### D13 — Buffered forward bodies (`FORWARD_MAX_BODY` = 128 MiB, `egress.rs:237`) · CODE-CONFIRMED (limit), LOW for the verify phase
D7's fix raised the cap but kept full-body buffering (resident memory equals the body size). Plain-HTTP `.deb` payloads over
128 MiB (agent phase in W4: CUDA/firmware/LLVM-sized packages) would get a typed refusal. Streaming the body
through (re-framed with chunked encoding or the upstream length) removes this class. `write_response_head_chunked`
already exists in `hh-wire`.

### D14 — `Host` always carries the port (`egress.rs:284`, `:352`: `format!("{host}:{port}")`) · LOW
Legal, but non-canonical for default ports (RFC 9110 §7.2). Signed-URL origins that sign the host can reject it.
Omit `:80`/`:443`.

### D15 — Agent and verifier deadlines are global defaults, not the task's declared timeouts · CONFIRMED
`crates/hh-runner/src/bin/hh_w1.rs:147–148` sets `agent_deadline_ms` 900 000 and `verify_deadline_ms` **600 000**
(10 min) for every cell. Harbor (the O surface) uses each task's `task.toml`. At the pinned TB 2.1 checkout
`harbor-framework/terminal-bench-2-1@12b75656`:
- `query-optimize`: `[verifier] timeout_sec = 1800`
- `make-doom-for-mips`, `pypi-server`, `torch-tensor-parallelism`: 900 each

So HH grades with a stricter deadline than O, which violates §3.8's "same verification semantics as Harbor". With H-allow
bootstraps (apt + uv + a CPython download) now on the critical path, 600 s will also start timing out members whose
verifier fits in 900 s.

### D16 — `download.pytorch.org` is missing from the verify allowlist · CONFIRMED (data), cause unknown
`pytorch-model-cli`'s `tests/test.sh` runs `uvx --index https://download.pytorch.org/whl/cpu … -w torch==2.7.1 …`.
The epoch-3 freeze (`docs/build/reports/2026-11-14-fp-egress-allowlist-freeze-epoch-3.json`) lacks that host, even though
the verify set is defined as agent base ∪ `tests/*` scan (`scan_hosts_in_text`, `crates/hh-env/src/egress_allowlist.rs`
~L241) ∪ oracle-observed. Either the scan missed it (a derivation defect) or the freeze was derived from a member set
that excluded this dev member. **Determine which before acting** (operator ruling R-6).

### D17 — A verifier that times out is not killed, so verify attempts overlap · CONFIRMED
The mechanism is in `crates/hh-runner/src/cell.rs`, `exec` (~L138–180). On deadline it runs `child.kill()`, which kills
only the **host-side `podman exec` client**, then `self.terminate_group("agent")`. The pattern is **hard-coded `"agent"`
for every phase**. `terminate_group` (`crates/hh-helper/src/podman.rs`) kills in-cell processes whose `/proc/*/cmdline`
matches that ERE. The verifier's tree (`bash /tests/test.sh` → `uvx` → `pytest …`) never matches, so it **keeps running
inside the cell**. The timeout path also returns empty `stdout`/`stderr`, so the verifier streams (bootstrap evidence) are
discarded exactly when they matter.

The ST5 loop (~L340–412) then starts attempt 2 (`max_grade_attempts = 2`) while attempt 1 is still running.

Evidence from Wc7 `query-optimize` H-pub r0–r2:
- `ctrf.json` shows 6/6 passed, pytest durations 821 / 805 / 812 s (> 600 s).
- `collect/reward.txt` = `1` (sha256 `4355a46b…` = sha256("1\n")).
- `cell-report.json` = `verifier_unavailable:verifier_timeout`, `verify_attempts: 2`.

That is: the leaked attempt-1 pytest finished after its deadline and its outputs were collected under a timed-out verdict.
Two overlapping verifiers also share the cell's CPU, which inflates walls. **D15 + D17 together fully explain
`query-optimize`'s "standing machinery discordance".**

### D18 — Per-slot memory below a task's declared memory · LOW (audit)
`fp-wc8.json` declares `memory_per_trial_gib: 6`; `torch-tensor-parallelism` declares `memory_mb = 8192`. It passes
H-pub, so the cap may not bind, but record HH cell limits against `task.toml` `cpus`/`memory_mb` for fidelity.

### D19 — Classification masks machinery failures as data exclusions · CONFIRMED (rule)
Member roll-ups label H-allow < R/R with `denied_hosts: []` as `exclude:runner_infeasible:allowlist_gap{unknown}`. The
spec's intent (§3.5 l.546: "excluded as `runner_infeasible:allowlist_gap{host}`… Denies are host-level") is that an
allowlist gap names the denied host. With no denied host it is a machinery discordance: "repaired, never excluded". The
orchestrator has correctly refused to treat these as exclusions, but the committed classifier would let a later window
exclude-and-replace members on a machinery failure (operator ruling R-7).

---

## 3. Budget wall (independent of the proxy)

FP-9 / REQ-XH-110 cap first-pass compute at **$20 as the Σ of window ceilings**. The Σ of declared ceilings is
**$38.38** (recorded each time as an "exceed"). Recorded compute estimates (`docs/build/reports/live/PHASE_SPEND.jsonl`)
total **$13.85**:

| Window | Estimate |
|---|---|
| W1 | $0.79 |
| W1 rerun | $1.34 |
| W2 | $0.20 |
| W3 | $0.17 |
| Wc | $1.09 |
| Wc2 | $1.08 |
| Wc3 | $1.80 |
| Wc4 | $1.74 |
| Wc5 | $2.01 |
| Wc6 | $1.76 |
| Wc7 | $1.88 |

W4 is planned at a **$7.99** ceiling (spec §10.1.5 table). R6.46 computes D_max "with completed windows at HUMAN-H18
billed figures where present, ceilings otherwise". Under that rule GATE-G30 cannot fit W4 even after the re-proof clears.
**Operator ruling R-5** resolves this.

---

## 4. Evidence still needed (cheapest source first)

1. The Wc8 H-allow cells now running (VM `hh-fp-wc8`, created 2026-11-15T20:40Z). This is the first window in which apt should
   get through, so it should show D8/D9 (or refute them) in the first completed H-allow cells. Harvest those cells'
   `cell-report.json`, `collect/*`, verifier streams and mediator stores.
2. The real-client conformance harness (insert row R6.45k). It reproduces D8–D12 locally or in CI in minutes, against real mirrors.
3. Smoke windows (insert row R6.45l). They run the exact window recipe, H-allow r=1, on 3–4 members.
