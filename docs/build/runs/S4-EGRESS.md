# S4-EGRESS — bounded egress supplement over the frozen confirmation cohort (protocol_id 9edc5327)

Status: the bounded admission, scope rule, mechanism, and honesty
bounds in this header block were declared before the supplement slots
ran. Dated execution results are appended below.

- Date: 2026-11-06
- Harness: devin-desktop/swe-2-high/subagent
- Cost: $0 — no model calls. Same deterministic participants (each
  task's committed oracle `solution.sh`/`solution.yaml`).
- Operator authorization (verbatim): "let's just fix that and run the
  full experiment with egress profiles" — scoped to the S4
  confirmation cohort under a bounded, honestly recorded egress
  profile.
- Prior record (untouched): `docs/build/runs/S4-CONFIRMATION.md` +
  `docs/build/reports/2026-11-05-s4-confirmation.json`
  (`hh-s4-confirmation/1`): 300/300 slots bound — 160 executed
  (130 resolved / 30 unresolved), 140 `inconclusive{…}`, 0 failed.

## The bounded admission (declared)

`egress_profile: declared_deps` — recorded verbatim on every slot row.

- **What it is:** a scoped environment admission. Slots whose tasks
  declare runtime egress deps (mechanical scan of the task's own
  `solution.*` + `run-tests.sh`: `apt`/`pip`/`uv` bootstrap,
  `git clone`, `curl`/`wget`, HF download, remote RPC) run their cell
  on `podman --network bridge` — outbound NAT through the podman VM's
  slirp4netns. All other slots keep `--network none`.
- **Mechanism:** a scoped helper variant,
  `/tmp/s4-helper-egress` — `crates/hh-helper/src/bin/hh_helper.rs`
  verbatim plus one knob (`--network <none|bridge>` /
  `S4_HELPER_NETWORK` env; default `none`). The `hharness-helper`
  server, protocol, and `PodmanBackend` are unchanged; the stock
  `hh-helper` binary and crate source are untouched. This is a named
  supplement variant — never a silent confinement bypass.
- **Policy honesty:** admitted cells declare `net.mode = public` in
  the containment policy — the N4-honest form (kernel authority +
  `residual_channels[host_bridge]` declaring the bridge, its owner,
  and its closer). fs/proc boundaries are unchanged and still probed
  (pids limit, no-new-privileges, /work mount).
- **Same frozen protocol:** the manifest is consumed verbatim
  (`protocol_id 9edc5327`, seed v3, no re-pick) — identical
  slot_refs, arm bindings, lanes, rounds. These are named supplement
  attempts: `supplement_epoch: egress-profile-rerun` and
  `prior_disposition` are recorded on every row; the original
  inconclusive records stand.
- **Scope:** exactly the 140 prior-inconclusive slots (14 tasks ×
  10). The 160 already-executed slots are NOT re-run — their first
  attempts are the record.
- **Every one of the 14 rerun tasks declared egress deps** (the
  uv/pytest verifier bootstrap at minimum; see `declared_deps` in the
  per-task build records) — so all 140 supplement slots are admitted
  under `declared_deps`. `none` remains the posture for any slot
  whose task doesn't declare deps (none in this supplement).

## Honest bounds (unchanged from the frozen run)

- Real execution only; per-slot verbatim records; never a fabricated
  verdict. A task that still can't genuinely run (deleted upstream,
  missing dep a bounded profile can't supply, multi-service
  topology) records `inconclusive{reason}`/`unresolved` verbatim.
- Same verifier channels: `verbatim` (run-tests.sh — now genuinely
  runnable: its uv/apt/pip bootstrap has egress) preferred, then
  `vendored_offline`; the task's own PytestParser decides
  `resolved|unresolved`.
- Same per-subject envelopes: ≤30 min admitted wall, verifier
  ≤15 min, sequential cells (≤2 concurrent respected — the driver's
  container sweep is indiscriminate, so the supplement runs 1 at a
  time).
- Same stop conditions; buildx/system prune when VM free < 8 GiB.
- `failed`/`inconclusive{cell_setup_failed}` are machinery outcomes,
  never exclusions; setup retries stay inside the frozen ≤1 bound.

## Machinery notes (environment deltas, recorded)

- `vm_free_gib` in the frozen run's orchestrator silently returned
  `None` (`podman machine ssh` targets `podman-machine-default`; the
  machine is named `podman`). The supplement orchestrator uses the
  recorded SSH recipe (`ssh -i …/machine -p 62382 core@127.0.0.1
  df -BG /var`); pruning still fires < 8 GiB.
- Drivers: `/tmp/s4-cell` (patched: spec fields
  `egress_profile`/`helper_bin`/`policy_net_mode` → records them
  verbatim; `net.mode`/residual declaration per above) +
  `/tmp/s4-helper-egress` (the variant above) +
  `/tmp/s4-egress-orch.py` + `/tmp/s4-egress-assemble.py`. All scratch
  machinery outside the repo, same class as `/tmp/parity-*` /
  `/tmp/s4-*`. Work root `~/tb-work/s4-egress/` (external).
- Early bring-up: the first smoke drive panicked
  `PolicyInvalid(PublicWithoutResidual)` (N4 needs a declared
  residual) — fixed by the `host_bridge` residual declaration above
  before the cohort ran. Those 10 setup-panic smoke records were
  superseded by the cohort records below.

## Emit target

- `docs/build/reports/2026-11-05-s4-egress-cohort.json`
  (`hh-s4-egress-supplement/1`) — per-slot verdicts, `egress_profile`
  verbatim, denominators, costs. The prior confirmation packet is not
  modified.

---

## Execution log (appended)

Everything below is verbatim record.

### Egress supplement results (2026-11-06 03:48→08:19 EST, verbatim)

140/140 supplement slots attempted — zero dropped, zero retried
beyond the frozen bound, zero fabricated. Packet:
`docs/build/reports/2026-11-05-s4-egress-cohort.json`
(`hh-s4-egress-supplement/1`, `supplement_epoch:
egress-profile-rerun`).

- **Dispositions:** `executed` **90/140** (60 `resolved` / 30
  `unresolved` — both genuine participant outcomes through the task's
  own verifier); `inconclusive{…}` **50/140** (verbatim reasons per
  slot: `image_unavailable:client_build_failed` ×40 — eval-mteb.hard
  OOM rc 137 on `pip install mteb==1.36.8`, fix-git deleted upstream
  repo rc 128, oom ENOSPC rc 125 writing the torch blob,
  qemu-alpine-ssh `apt-get install` rc 100 upstream;
  `verifier_unavailable:no_parseable_verdict` ×10 — swe-bench-astropy-2:
  the task's declared `max_test_timeout_sec: 60` cannot host its own
  run-tests.sh's apt+venv+`pip install -e .` bootstrap and the derived
  image's pip bake carries no pytest — an honest bound, not a
  machinery failure); `failed` 0; `unattempted` 0.
- **Per-task executed:** `resolved` ×6 — git-workflow-hack,
  polyglot-c-py, configure-git-webserver, security-vulhub-minio,
  password-recovery, modernize-fortran-build; `unresolved` ×3 —
  build-linux-kernel-qemu (kernel compile capped at the 1800s work
  bound; verifier decided fail), polyglot-rust-c (the committed oracle
  is a `# UNKNOWN` placeholder — verifier decided fail),
  hf-model-inference (HF download/flask race inside the 360s cap —
  verifier decided fail). Still `inconclusive` ×5 — eval-mteb.hard,
  fix-git, oom, qemu-alpine-ssh, swe-bench-astropy-2.
- **Receipt channels:** `verbatim` — run-tests.sh's uv/apt/pip
  bootstrap genuinely ran inside the bridge cells (recorded per slot;
  e.g. `+ pytest==9.2.0` installed by `uv add pytest` then
  `test_outputs.py` collected).
- **Denominators (recomputed over the supplement, verbatim):**
  planned 140/140 recorded; cell attempts 100; mediated dispatches
  310; first subjects 18/28 executed (12 resolved); every attempt
  identity retained (`attempts[]`).
- **§4.2 fidelity denominators (verbatim):** pressure opportunities
  30; compactions started 40; `applied` statuses 100;
  changed_requests 40; triggered first subjects 6;
  **served_deviations 30** — on no-pressure B-arm slots the
  compaction record's `served_variant` reported `hh/evict-oldest@1`
  rather than the clear-tool-results variant — same verbatim class as
  the frozen run's 60, recorded never relabelled.
- **Costs (verbatim):** total slot wall 15,562,121 ms (~259 min
  across 140 sequential cells — dominated by the kernel task's 10 ×
  ~20–30 min capped work dispatches); output bytes 8,729,540;
  tokens_freed 2,034,895; USD $0.00; provider exposure 0; no model
  calls.
- **Stops:** none fired — sentinel intact and observer survived on
  every cell (`sentinel_intact`/`observer_survived` true per row);
  seals `sealed`; no served-policy relabel beyond the recorded
  deviations; no schedule deviation; individual caps held (two
  `wall_cap_breach` flags where kernel slots' walls exceeded the
  30-min envelope — recorded verbatim; the envelope's deadline ladder
  still bound each dispatch).
- **Corrections recorded:** hf-model-inference r0/r1-o-tb-oracle —
  the egress signature scan (written for `--network none` cells)
  labelled one bridge-cell slot
  `inconclusive{environment:cell_network_isolated:Connection
  refused}`; the refusal was to `localhost:5000` — the oracle's own
  flask service racing inside the work cap, a workload outcome. The
  record carries `disposition_recomputed` verbatim → `executed`
  (receipt retained); the orch rule now guards the signature on
  `cell_network == "none"`.
- **Combined cohort (frozen run + this supplement, verbatim):**
  250/300 slots now carry real receipts (190 resolved / 60
  unresolved); 50 still inconclusive with verbatim reasons; first
  subjects 50/60 executed (38 resolved). The original 300 records
  stand; this packet is the named supplement.
- **Machinery state left behind:** podman VM running (25 GiB free);
  per-task client+derived images pruned; zero `hh-env-*` containers
  leaked; work root `~/tb-work/s4-egress/` (stores/ws/slot records —
  external, not committed); drivers `/tmp/s4-cell` (patched),
  `/tmp/s4-helper-egress` (the scoped variant), `/tmp/s4-egress-orch.py`,
  `/tmp/s4-egress-assemble.py`.
- **Verdict: `partial`** — the supplement executed 90/140
  prior-inconclusive slots with real receipts; the remaining 50 are
  honest bounds the bounded profile cannot lift (build-time OOM/ENOSPC
  and deleted upstreams are not cell-egress questions; astropy's
  verifier bootstrap exceeds the task's own 60 s declared bound). No
  retrospective smaller manifest; denominators retained verbatim.
