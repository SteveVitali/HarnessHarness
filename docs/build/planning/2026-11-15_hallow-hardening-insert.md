# 02 — Insert spec: H-allow hardening, smoke loop and the Wc9 re-proof

**What this is.** The scoped input for `decompose-spec mode=extend` (orchestrate-build §2.4 "insert"). It defines
the rows to insert, their contracts' substance, the acceptance criteria and the stop rules. The executing session authors the
actual contract files in the repo's own format (Phase 3), runs the Phase-4 fresh-context review, and adds the chain rows,
`## Plan extensions` line and `inserted` PHASE LOG entry. Ticket IDs and sequence letters below are **proposals**; use the
manifest's next free suffixes and conventions.

**Provenance.** Copy this file verbatim to `docs/build/planning/2026-11-15_hallow-hardening-insert.md` and cite its
sha256 in the PHASE LOG entry. Read `01_DIAGNOSIS.md` (copy it to `…_hallow-diagnosis.md`) for evidence and line anchors.
The operator's binding words are the rulings R-1…R-9 in the session message, not this file.

**Precedence.** The ratified specs, the round-6 invariants X-01…X-08 / XO-01…XO-06, the build-memory contract and
operator rulings R-1…R-9 govern. If anything here conflicts with them, or with code reality as found at execution,
**stop and surface it**. Do not silently deviate in either direction. Each defect claim is a hypothesis to reproduce first;
a refuted claim is recorded as refuted and its fix is dropped.

---

## 0. Placement and order

Insert after R6.45h (309h, Wc8) closes and **before GATE-G29** (309). Proposed rows, in chain order:

| Proposed # | ID | Kind | One-line scope | Paid? |
|---|---|---|---|---|
| 309i | R6.45i | ticket (records) | Operator rulings → GATE DECISIONS; spec §10.5 amendment rows A1–A7 + ADRs; dated notes on affected unexecuted contracts | $0 |
| 309j | R6.45j | ticket | Cell fidelity: per-task deadlines from `task.toml` (D15); kill the verifier on deadline, no overlapping attempts (D17); resource audit (D18); classifier rule (D19) | $0 |
| 309k | R6.45k | ticket | Egress hardening (D8–D14) + real-client conformance leg + allowlist scan check / conditional epoch-4 (D16) | $0 |
| 309l | GATE-G28i | gate | PROV-SMOKE envelope readout: records R-3's envelope; each smoke's quote appended as minted | — |
| 309m | R6.45l | ticket | H-allow smoke loop: ≤ 3 non-evidence windows on the exact recipe; in-window fixes allowed; exit on SMOKE-PASS | ≤ $3.50 Σ ceilings |
| 309n | GATE-G28j | gate | PROV-Wc9: normal gate, presented to the operator; readout carries smoke results, the prediction-track table and re-scope options | — |
| 309o | R6.45m | skeleton → ticket | Wc9 evidence re-proof on the hardened runner (R6.45h's shape plus the early-inspection checkpoint); fleshed after G28j | per G28j |

R6.45j and R6.45k are independent. Run R6.45j first because it is small and clears `query-optimize`. Both must land
(PRs open, CI green) before smoke 1. Smoke windows build the runner from the chain tip that includes both.

**Documented split seam for R6.45k** (pre-declared so a split is legal under the size rule): **k(a)** = conformance
harness + red baseline + D8 + D9; **k(b)** = D10–D14 + D16 scan check / epoch-4. Split only if the measured working set
exceeds the budget (XO-03).

---

## 1. R6.45i — Intervention records and spec amendments ($0, records-only)

**Deliverables**
1. LEDGER `GATE DECISIONS` rows for R-1…R-9: verbatim, `date -u` of receipt, channel `interactive`.
2. Experiments-spec §10.5 amendment-register rows plus ADRs. The spec body stays frozen (X-01); amendments are rows.
   - **A1 Non-evidence smoke windows (R-3).** A new window class `smoke`:
     - $0 model;
     - compute under a gate envelope;
     - in-window code changes permitted;
     - results labelled "non-evidence smoke (internal; never re-proof evidence)", never folded into
       `hh-reproof-packet/1` or `hh-reproof-register/1`, never cited as re-proof or parity evidence.

     Evidence windows keep every existing rule.
   - **A2 Compute accounting (R-5).** FP-9 / REQ-XH-110 / REQ-XH-126 / D_max:
     - first-pass compute ≤ **$30**;
     - completed windows count at HUMAN-H18 billed figures where present, else at their recorded `cost_lib`
       estimate (PHASE_SPEND), **never their ceilings**;
     - authorized-but-unrun windows count at their ceilings;
     - model ≤ $70 unchanged; $10 unauthorized reserve unchanged;
     - first-pass total ceiling **$110**.

     Record the Σ-exceed history as superseded by this accounting, not erased.
   - **A3 Gate content and stop rule (R-8).** Every PROV readout for a repair or re-proof window lists re-scope
     alternatives (§6 ladder) besides `run | not_run`, and includes a **prediction-track table** (each prior window's
     readout-predicted outcome vs the actual outcome). Post-Wc9 stop rule: §6.
   - **A4 Conditional epoch-4 (R-6).** Authority to mint an epoch-4 freeze if, and only if, R6.45k's scan check confirms
     a derivation gap.
   - **A5 Classification (R-7).** `allowlist_gap` requires a non-empty `denied_hosts`. H-allow < R/R with an empty
     `denied_hosts` is `machinery{h_allow}`: repaired, never excluded.
   - **A6 Fidelity correction (D15/D17).** H-surface deadlines derive from the task's `task.toml` exactly as the pinned
     Harbor version does. A lapsed verifier is terminated before any further attempt or copy-out. Record this as an ADR;
     it implements §3.8's existing "same verification semantics", so it is not a method change.
   - **A7 Harvest footprint (R-9).** From Wc9 on, commit harvest summaries, digests and the small evidence excerpts
     a verdict cites. Raw run trees stay on the controller and in the private archive upload. If REQ-XB-220 or any
     committed requirement mandates in-repo raw trees, keep current behavior and surface the conflict instead.
3. Dated amendment notes, appended and never rewritten, on the unexecuted contracts affected:
   - R6.46: D_max accounting per A2.
   - GATE-G30's readout template.
   - HUMAN-H18: add the Wc8, smoke1–3 and Wc9 windows to the reconcile list.
   - Any Round-6 row that cites FP-9's old Σ-of-ceilings rule.
4. R6.45h's manifest note: Wc8 executed under R-1's early-inspection rule (whatever it recorded). No further window
   of the Wc8 shape will be proposed.

**Acceptance**
- The stock validator reports 0 violations and `check-supplement-guard.sh` gives `PASS`.
- Every R-row appears verbatim exactly once in GATE DECISIONS.
- Amendment rows A1–A7 resolve from the manifest's `## Spec amendments applied`.
- The ADR index is regenerated.

**Size:** small (records).

---

## 2. R6.45j — Cell fidelity ($0, code)

**Scope**
1. **D15 deadlines.**
   - Agent and verifier deadlines come from each task's `task.toml` (`[agent].timeout_sec`,
     `[verifier].timeout_sec`) in the pinned checkout, using the **same semantics as the pinned Harbor version**. Read
     Harbor's code at the pin: timeout multipliers, the default when the value is absent, whether the agent deadline
     covers setup. Record the exact source lines in the ADR.
   - `cfg.{agent,verify}_deadline_ms` become fallbacks only.
   - `cell-report.json` records `deadline_ms` and `deadline_source: task_toml|cfg_default` per phase.
2. **D17 kill on lapse.**
   - `exec`'s timeout path currently kills the host-side `podman exec` client and calls `terminate_group("agent")` for
     **every** phase. Replace that with termination of the **lapsed exec's own in-cell process tree**, whatever the
     phase. Do not depend on a cmdline pattern that the phase's processes may not carry: run each phase exec as a
     process-group/session leader in-cell and kill the group, or tag it with a unique per-exec marker and walk for that.
   - **Confirm death** by a `/proc` walk before `rm_glob`, the next attempt or copy-out.
   - Keep the stdout/stderr captured up to the lapse, rather than returning empty vectors, so a timed-out verifier
     still lands its streams as evidence.
   - Attempts never overlap. Collected files belong to exactly one attempt; record the attempt number with the files.
   - Re-check the agent phase under the same change: its `"agent"` pattern must still terminate the oracle/agent tree.
   - Confirm `max_grade_attempts` against the governing REQ and Harbor's behavior at the pin. Change it only if the spec
     says so; otherwise record the observation.
3. **D18 audit.** Record HH cell cpu/memory limits against each re-proof member's `task.toml` `cpus`/`memory_mb`. If
   HH grants less than a task declares, raise the cell limit to at least the declared value (Harbor semantics) and re-check
   the packing/C rule. Otherwise record parity.
4. **D19 / A5 classifier.**
   - Member roll-up and `reproof` classification: `allowlist_gap{host}` only with a named denied host.
   - H-allow < R/R with `denied_hosts: []` becomes `machinery{h_allow}`.
   - Re-fold the Wc4–Wc8 records' member verdicts under the amended rule **as an appended, dated re-classification
     record**. Never rewrite the register (it is append-only).

**Acceptance (each regression must fail on pre-fix code — XO-05)**
- **AC-1:** a fixture task with `[verifier].timeout_sec = 1800` gets an 1 800 000 ms verify deadline with source
  `task_toml`; an absent value gets the cfg fallback with source `cfg_default`.
- **AC-2:** a fake cell whose verifier tree (a child process with no "agent" in its cmdline) outlives its deadline is
  killed, tree included. Attempt 2 starts only after confirmed death. The collected `reward.txt` is attempt 2's (or
  none), never attempt 1's, and the lapsed attempt's partial stdout/stderr are recorded.
- **AC-3:** a regression built from Wc7 `query-optimize` H-pub's recorded bytes (ctrf 6/6, `reward.txt` = 1, 812 s):
  under 1800 s it classifies decided, reward 1.
- **AC-4:** the A5 classifier: an empty `denied_hosts` with H-allow < R/R gives `machinery{h_allow}`; a named host
  gives `allowlist_gap{host}`.
- **AC-5:** `cargo fmt --all --check`, scoped `cargo test -p hharness-runner` (plus any touched crate), and
  `scripts/check-drift.sh` if a schema changed.

**Size:** small–medium.

---

## 3. R6.45k — Egress hardening and the real-client conformance leg ($0, code + a $0 live leg)

### 3.1 First, a red baseline
Build the **real-client conformance harness** (3.2) and run it against the **pre-fix** chain tip. Record which
bootstrap shapes fail and how; this is the red baseline. Fold in Wc8's harvested H-allow cell evidence. Confirm or refute
D8–D12 from that evidence **before** writing fixes.

### 3.2 The conformance harness (`scripts/egress-conformance/`, live leg `egress-real-clients`)
- **Topology:** one Linux container from `ubuntu:noble` (the task images' family). It contains:
  - the chain-tip `hh-egress-mediator` and `hh-cell-relay` built for that arch;
  - the mediator started exactly as `hh-w1 h-run` starts it (same argv shape; same policy **bytes** rendered from the
    current freeze by the same code path; expose a small `hh-w1 render-policy` subcommand if needed rather than
    re-deriving it);
  - the relay on `127.0.0.1:3128`;
  - `http_proxy`/`https_proxy`/`HTTP(S)_PROXY`/`NO_PROXY` exactly as `crates/hh-runner/src/render.rs` sets them.

  No podman-in-podman is needed.
- **Shapes:** derived **mechanically** from the committed `tests/test.sh` of the 14 re-proof members and their
  pre-re-proofed successors. A script extracts the unique bootstrap lines so the list cannot drift. Shapes run
  verbatim, with the pytest leg replaced by `pytest --version` or an import check:
  - **S1** apt update → `apt-get install -y curl` → the astral uv installer → `uvx -p 3.13 -w pytest==8.4.1 -w pytest-json-ctrf==0.3.5 pytest --version`.
  - **S2** the pytorch-index shape (`--index https://download.pytorch.org/whl/cpu`, torch 2.7.1 / torchvision 0.22.1 / numpy / opencv) → `python -c "import torch"`. Expected: a typed 403 deny naming `download.pytorch.org` under epoch-3, success under epoch-4.
  - **S3** the pip shapes (`pip install … --break-system-packages`).
  - **S4** the heavier apt sets seen (`ffmpeg libsm6 libxext6`, `gcc`, `python3-pillow`, `vim`).
- **Assertions per shape:**
  - exit 0, except the expected S2 deny under epoch-3;
  - every `security.egress.requested` row has a `decided` row;
  - zero mediator 5xx replies;
  - every tunnel's `close_cause` ∈ {`eof`}, with true byte counts;
  - per-step wall times and the peak number of concurrent mediator connections recorded (the D8 proof).
- **Where it runs:**
  - **(a)** GitHub Actions `workflow_dispatch` on `ubuntu-24.04` x86_64. This is preferred: it matches the GCE
    arch and costs $0 for a public repo. Trigger with `gh workflow run`; read the result with `gh run view`.
  - **(b)** The operator's Mac via Docker Desktop, arm64-native, as secondary evidence only. Mind the host: about
    32 GiB free (93% used). Keep the footprint under 5 GiB and clean up afterwards.
- **Registration:** a `scripts/live-verify.sh` leg (ADR-0363 pattern) named `egress-real-clients`, budget $0, no
  secrets, evidence record per the leg convention, authorized by R-4. It is **not** part of per-PR CI; real-network calls
  stay out of the default pipeline.

### 3.3 Fixes (each with a hermetic loopback regression that fails pre-fix)
- **D8 concurrency.**
  - Serve each accepted connection on its own thread, bounded (e.g. 64 in flight; excess waits in backlog).
  - The ledger `Store` and lease are shared behind a `Mutex`. Hold it only for `pre_wire`
    (requested → decide → decided, durable before the wire) and for the close/charge phase. **Never hold it during
    byte pumping or the upstream exchange.**
  - Preserve the module's ordering contract (SV-5, §5g.4 §6):
    - requested durable before decision;
    - decided durable before the wire;
    - `tunnel.closed` after the pump.
  - Load the policy once per connection, as now, or once with a reload check. Never per request inside the lock.
- **D9 tunnel pump.**
  - No idle read timeout on established tunnel legs, or a declared idle timeout ≥ 300 s, recorded.
  - When one direction reaches EOF, `shutdown(Write)` the peer (the relay's pattern). Join both directions.
  - Record real byte counts and a true `close_cause` (`eof|idle_timeout|error:<kind>`). Never `unwrap_or(0)`.
- **D10 response hygiene.**
  - Strip hop-by-hop **response** headers: `Connection`, `Keep-Alive`, `Transfer-Encoding`, `Content-Length` (re-framed),
    `Proxy-*`, `Trailer`, `Upgrade`, plus any header named in `Connection:`.
  - Emit exactly one framing header and `Connection: close` (the mediator serves one request per connection).
- **D11 bodiless responses.**
  - The codec decides body presence from the request method and the status: HEAD, 1xx, 204 and 304 carry no body.
  - Thread the method through without breaking the model proxy's callers. Run the `hharness-wire` and model-proxy
    batteries.
- **D12 timeouts.** Declared constants:
  - connect ≤ 10 s;
  - first byte / read ≥ 60 s;
  - a lapse answers a typed **504** (not 502), recorded on the decided/closed row.
- **D13 streaming (preferred).** In the mediator binary path, stream the upstream body to the client as it arrives
  (re-framed chunked, or passthrough length after D10 hygiene) with bounded resident memory. Keep the in-process
  buffered `forward` API for existing callers. If streaming does not fit the size budget, keep the 128 MiB bound, record
  a DEFERRALS row and say so in the PR. Never silently skip it.
- **D14 Host.** Omit the default port from the `Host` header.
- **D16 scan check.**
  - Run `scan_hosts_in_text` over every re-proof member's and successor's `tests/*` bytes and diff the result against
    the epoch-3 verify set.
  - **If** a scanned host is missing (expected: `download.pytorch.org`), find why (derivation input set / scan bug),
    fix it with a regression, and mint **epoch-4** under `protocol_id fp-w1-q-x55`. Epoch-4 is a new epoch
    (REQ-XB-116) with verify-phase additions only, never agent-phase (X-06). Record it with the amendment document
    pattern used for epoch-2/3.
  - **Else** record the result. The member's `allowlist_gap{download.pytorch.org}` exclusion then stands (S-3).

### 3.4 Acceptance
- **AC-1:** hermetic battery, all failing pre-fix:
  - two concurrent CONNECTs, the first held idle, the second completes in < 2 s (D8);
  - 32 concurrent forwards keep per-request ledger order with no lost rows (D8);
  - upstream silent 10 s then sends, the client receives it (D9);
  - upstream EOF reaches the client promptly; client half-close reaches upstream; the closed row carries true counts and cause (D9);
  - a chunked upstream through the forward path parses correctly at the client, with exactly one framing header and
    `Connection: close` (D10);
  - no duplicate `Content-Length` (D10);
  - HEAD with `Content-Length: N`, 204 and 304 return promptly with no body (D11);
  - 10 s first-byte succeeds; a lapse beyond the declared bound gives a typed 504 (D12);
  - a 250 MiB body streams with bounded RSS growth, or the deferral is recorded (D13);
  - `Host` omits `:80`/`:443` (D14).
- **AC-2:** the conformance leg is red on the pre-fix tip (recorded) and green on the fixed tip on GitHub Actions
  x86_64 for S1, S3 and S4. S2 is green under epoch-4, or a recorded typed deny under epoch-3.
- **AC-3:** ordering-contract regressions still pass under concurrency.
- **AC-4:** `cargo fmt --all --check`, scoped tests for `hharness-env`, `hharness-wire`, `hh-helper` and the model
  proxy crate, `check-drift.sh` if schemas changed, and `scripts/check-live-evidence.sh` for the new leg.

**Size:** medium–large. Split at the §0 seam if measured over budget.

---

## 4. GATE-G28i — PROV-SMOKE envelope (readout)

The readout records R-3 verbatim as the compute authorization: $0 model (`model_line: none`, no `deliver-secret`);
up to 3 smoke windows; each ceiling ≤ 1 250 000 µ$ by the §3.10 formula; Σ ≤ 3 500 000 µ$.

Each smoke's declaration and quote (minted by the one quote path, `receipt_lib.py quoteid`) is appended to the readout
as an agent-drafted row with its hash, under the envelope, before that window's PROV. A quote outside the envelope
stops for a separate operator answer.

The readout states the A1 label rule and that smoke results are never re-proof evidence. Its status is set from the
operator's R-3 words; the agent signs nothing.

---

## 5. R6.45l — The H-allow smoke loop (≤ 3 windows, ≤ $3.50 Σ ceilings)

**Shape per smoke window**
- Same recipe as the Wc windows: `hh-compute-decl/1` `mode: full`, e2-standard-8, debian-12, the declared
  podman/crun/netavark pins.
- `window: smoke1|smoke2|smoke3`; boot disk 150 GiB unless the image set needs more.
- **D = 1.5 h** dead-man; ≤ 0.5 h harvest-only restart; h_r 6 h.
- Ceiling example: 314 140·(1.5 + 0.5) + 150·137·6 + 0.12·1 GiB ≈ 871 580 µ$. Re-mint from the live rate table; never hand-compute the declared value.
- Runner built from the chain tip that includes R6.45j and R6.45k.

**Members**
- **H-allow, r = 1:** `prove-plus-comm` (standard bootstrap), `torch-tensor-parallelism` (heavy, 8 GiB),
  `query-optimize` (long verifier), and `pytorch-model-cli` only if epoch-4 was minted.
- **H-pub, r = 1:** `query-optimize` only (checks the D15/D17 fix on the non-mediated surface).

**In-window iteration (A1)**
- On a failure, diagnose from the cell report, verifier streams and mediator store.
- Patch on the smoke branch, rebuild only the affected binary, re-run only the affected cell, inside D.
- Every in-window patch must, before Wc9:
  - land as a commit with a hermetic regression that fails without it, in this row's PR or a follow-up insert;
  - be added to the conformance leg where real-client-shaped.

**SMOKE-PASS (all of)**
- Every smoke member's H-allow cell is classified *decided* (no `verifier_unavailable:*`), reward 1.
- The mediator store shows zero 5xx replies and no tunnel with `close_cause` other than `eof`.
- `denied_hosts` is empty, or exactly the hosts epoch policy denies by design.
- `query-optimize` H-pub is decided, reward 1.
- Per-member H-allow verify walls are recorded. They feed Wc9's D sizing (§7).

**Exit**
- SMOKE-PASS → tear down → settle → records → the next row (GATE-G28j).
- Not passed and smokes remain → next smoke. Each smoke starts with what the previous one learned, written down first.
- Not passed after smoke 3 → **`blockedOn: H-allow smoke envelope exhausted`**, with a diagnosis readout using the §6
  options. No further spend.

**Records**
- `docs/build/reports/smoke/<date>-smokeN.json` (summary + cited excerpts + digests) and a run ledger.
- PHASE_SPEND compute-estimate rows `window: smokeN`; settlement per the window pattern.
- Raw trees stay in `~/hh-retained/smokeN` and are not committed (A7).
- Every artifact is labelled per A1.

---

## 6. Stop rule and re-scope ladder (A3)

The GATE-G28j readout, and any later PROV readout for a repair or re-proof window, lists:
1. `run`: the evidence window as specified.
2. `not_run`: the task list stays `blocked`.
3. `re-scope{harden-again}`: another R6.45k-style insert plus a new smoke envelope, for a newly named defect class.
4. `re-scope{exclude-and-replace}`: only where the A5 rule makes an exclusion legitimate (named denied host / O-fail),
   replacement from the seeded order per FP-7/REQ-XH-116.
5. `re-scope{end-first-pass-early}`: record the re-proof outcome as the first-pass result and skip W4. Name the
   claims withheld.

Every readout also carries the **prediction-track table** (the window, what its readout predicted, what happened).

**Post-Wc9 stop rule:** if Wc9 leaves any member with a `machinery{…}` discordance or a `verifier_unavailable:*`
classification, do **not** draft a Wc10. Set `blockedOn` and present a scope-decision readout with options 2–5.

---

## 7. GATE-G28j and R6.45m — the Wc9 evidence re-proof

- **Shape:** R6.45h's contract, cloned. The evidence rules are unchanged: no in-window fixes, `retry_trials: 0`,
  typed outcomes, never rescued.
- **Inputs:** runner = the hardened chain-tip commit (all R6.45j/k/l repairs, recorded with each repair's carrier and
  regression); freeze = epoch-4 if minted, else epoch-3.
- **D:** clamp(⌈1.5 × projected wall from the smoke-measured per-member walls × the declared trial count ÷ C⌉, 4 h, 8 h).
- **Early-inspection checkpoint** (operator-preauthorized by R-8 as the readout records it):
  - once the first H-allow cells of ≥ 4 distinct members complete, harvest their cell reports;
  - if any shows a machinery signature (`verifier_unavailable:*`, a mediator 5xx, or a tunnel `close_cause` ≠ `eof`),
    stop the leg, tear down, and record `incomplete{early_stop:machinery_signature}`;
  - otherwise run to completion.
- **The G28j readout must contain:**
  - the Wc8 outcome;
  - SMOKE-PASS evidence;
  - the conformance-leg run URL;
  - the D sizing;
  - the quote;
  - the A2 budget table (below);
  - the §6 ladder;
  - the prediction-track table.

  The operator answers G28j when presented. **It is not pre-answered.**

---

## 8. Budget under A2 (planning, UNVERIFIED until billed)

| Item | Counted as | Amount |
|---|---|---|
| Completed W1…Wc7 | recorded estimates | $13.85 |
| Wc8 (early-stopped or complete) | estimate | ~$0.5–2.0 |
| Smoke ×≤3 | ceilings until run, then estimates | ≤ $3.50 → expected ~$1.5 |
| Wc9 | ceiling until run | ~$2.5–3.2 |
| W4 | planned ceiling | $7.99 |
| **Σ (worst case at G28j time)** | | **≈ $29.5 ≤ $30** |

At the FP-E quote (R6.46), once smoke and Wc9 have run and count as estimates, W4 headroom is ≈ $30 − ~$18.5 ≈ $11.5,
which is at least $7.99. Model stays ≤ $70 (W4). HUMAN-H18 billed reads should replace estimates as soon as each window is
≥ 24 h past teardown.
