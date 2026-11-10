# R6.39 — C-05 rung record (E2-T8 (a); REQ-XE-120; Q-X62's awaited record)

- **Date:** 2026-11-10 · **Ticket:** R6.39 (manifest 298) · **Branch:** `codex/harnessharness-r6.39`
- **Rung reached:** **1** — the detach/reattach spike passes.
- **What it ran on:** offline fixture host (macOS arm64; `local_sandboxed`
  seatbelt cell over a built `hh-helper`; no podman/live leg run).
  USD $0.00.

## The ladder (§0.3.2 C-05, verbatim)

> REQ-XB-103 stands. Fallback ladder for HH-native slots: (1) the
> detach/reattach spike passes; (2) the kernel child keeps cell custody
> through seal and verify over binding (b), with no new crate edge
> (UNVERIFIED); (3) only then an in-process binding (a) exception by an
> ADR amending AR-04, raised to the operator through a scoped
> `decompose-spec` insert. Never relaxed silently.

## Rung evidence

**Rung 1 — detach/reattach spike:** `crates/hh-runner/tests/r6_39.rs`
(`ac3_two_process_ownership_detach_reattach` — offline, deterministic):

1. The slot worker spawns `hh-kernel serve` as a child process over
   binding (b) — newline-delimited JSON-RPC on the child's stdio —
   with `HH_STORE_ROOT` = the slot store root and `HH_HELPER_BIN`
   pointed at the built helper. No crate edge: the runner reaches the
   kernel only as a spawned process (REQ-XB-103;
   `scripts/check-runner-edges.sh` has no `hh-embed`/`-schema`/`cli`/
   `hosting`/`kernel` edge in `cargo tree -p hharness-runner`).
2. `hello{contract_major:1, client{kind:lab, capabilities{experimental,
   serves_*}}}` → `open_session{spec:{kind:new, definition{document},
   environment{connection_info{local_sandboxed}}}}` — the child creates
   and owns the slot run (sole writer during the agent phase). A
   second `acquire_writer` on the same run from the worker process is
   refused `WouldBlock` at the store's own fence — detected, never
   bypassed.
3. `env.open{connection_info{class:local_sandboxed, workspace_roots}}`
   provisions the cell through the one `environment_spec` path (R6.39
   admits the contained local classes — no second provisioning path,
   CC1). The seatbelt helper spawns the cell.
4. `env.close{mode:"detach"}` → `action.environment.detached`;
   `env.attach` → `{attached:true, state:"ready"}` — the same cell
   re-attaches under the child's custody. **Detach/reattach passes.**
5. Custody return: `close{done}` seals the run (`lifecycle.run.finished`,
   seq 49 in fixture) and releases the writer; the worker's
   `acquire_writer` takeover then succeeds at the fence. A fresh
   `EnvDriver` re-attaching the durable `Detached` handle post-seal
   refuses typed (no live handle table / sealed run) — recorded, never
   papered over.

## Rungs not reached — honest detail

- **Rung 2 — not reached.** The child does not keep cell custody
  *through* seal and verify: the fixture's custody return goes through
  `close` (run seals → worker takes the writer back), so the kernel
  child was not holding custody when a seal/verify leg would land.
  Reaching rung 2 needs the child to stay writer through the seal pair
  — left UNVERIFIED, per the ladder's own tag.
- **Rung 3 — not reached** ⇒ no AR-04 amendment drafted, no insert
  request raised; HH-native slot launch stays refused typed until one
  is admitted (X-01).
- Fixture limitations recorded: the spike exercises the scripted/open
  session path — the HH-native model lane through R6.12's replay proxy
  is R6.41's scope; `env.attach` re-attach was exercised in-session
  (the worker-side post-seal re-attach refused typed, as recorded);
  `local_container`/podman legs are stub/live-ignored, not run.
