# S4-COMPLETION — GCE-backend supplement over the 50-slot inconclusive residue (protocol_id 9edc5327)

Status: the backend, admission, mechanism, and honesty bounds in this
header block were declared before the supplement slots ran. Dated
execution results are appended below. (The ticket contract names this
ledger `docs/build/runs/R5.58.md`; the operator's dispatch names it
`S4-COMPLETION.md` — the S4-* supplement ledger series — and that name
is the one used.)

- Date: 2026-11-06
- Harness: devin-desktop/swe-2-high/subagent
- Cost: $0 — no model calls. Same deterministic participants (each
  task's committed oracle `solution.sh`/`solution.yaml`).
- Operator authorization (verbatim): implement manifest ticket
  `docs/tickets/249_R5.58__s4-completion.md` under the recorded
  extension + provisioning authorizations ("insert these into the
  build plan as new ticket(s), then continue driving
  orchestrate-build over them to the end" + "you can provision
  everything. just make sure we don't forget to deprovision at the
  end. proceed" — operator chat, received 2026-11-06).
- Prior records (untouched): `docs/build/runs/S4-CONFIRMATION.md` +
  `docs/build/reports/2026-11-05-s4-confirmation.json`
  (`hh-s4-confirmation/1`); `docs/build/runs/S4-EGRESS.md` +
  `docs/build/reports/2026-11-05-s4-egress-cohort.json`
  (`hh-s4-egress-supplement/1`, `supplement_epoch:
  egress-profile-rerun` — the 50-slot residue with verbatim reasons);
  `docs/build/reports/2026-11-06-r5.57-verifier-bound-admission.json`
  (`hh-verifier-bound-admission/1` — the admitted astropy bound).
- Ticket contract: `docs/tickets/249_R5.58__s4-completion.md` (seq
  249; AC-1..AC-5). Pre-dispatch guard: `check-supplement-guard.sh`
  PASS, route `DISPATCH — R5.58`.

## The backend (declared)

GCE `podman-s4-backend` — project `harnessharness-evidence-8d7a99`,
`e2-standard-8` (8 vCPU / 32 GiB) + 200 GB `pd-balanced`,
`us-central1-a`, debian-12, podman 4.3.1 **rootful** via
`podman-remote.service` (record: `~/tb-work/gce-backend.md`). The
local podman CLI reaches it through the recorded SSH forward:

- `CONTAINER_HOST=unix:///tmp/podman-gce/podman.sock` (unix-socket
  forward → remote `/tmp/podman.sock`), verified by `podman info`
  reporting `host: arch amd64, debian`, and a real
  `podman run --rm hello-world` executed on the remote;
- `DOCKER_HOST=tcp://localhost:23750` — same socket on TCP, verified
  by `docker info` reporting server `4.3.1 debian amd64
  podman-s4-backend`.

Machine facts (verified live, verbatim): 8 vCPU; `free -g` total 31
GiB; `df -BG /` 197 G size / 187 G free at bring-up;
`systemctl is-active podman-remote.service` → `active`. Cost ≈
$0.30/hr inside the ~$20 recorded budget; actual billed time recorded
verbatim at close.

**Workspace parity:** the cell driver's `/work` bind mount names a
host path — on a remote podman that path lives on the VM. The VM
carries the byte-identical workspace path (`/Users/stevenvitali` is
the ssh user's own mirror root); each slot's staged workspace is
pushed before provision and pulled after each dispatch via a tar
stream over `gcloud compute ssh`. The slot record carries
`remote_ws_host` + `remote_ws_syncs` verbatim. The store, ledger,
observer and sentinel stay local — only the container-visible `/work`
tree is mirrored.

## The admission (declared — first-class machinery)

`egress_profile: declared_deps` recorded verbatim on every slot row.

- **Mechanism:** the R5.56 first-class variant
  (`crates/hh-env/src/egress_profile.rs`, ADR-0421): the attached
  `ContainmentPolicy` is mutated by `admit_declared_deps` —
  `net.mode = public` with the declared
  `residual_channels[host_bridge]` (kind/statement/owner/closer) —
  and `podman_helper_args` appends `--network bridge` to the stock
  `hh-helper` spawn argv (`--network` flag, R5.56). The scratch
  `/tmp/s4-helper-egress` variant is superseded and never used; the
  helper binary is the repo's own `target/debug/hh-helper`. The cell
  spec (`EnvironmentRecord.ext`) carries the supplement's field
  spellings verbatim — `egress_profile`, `cell_network: bridge`,
  `policy_net_mode: public`, `declared_deps` — stamped by the same
  admission call.
- **Verifer bound (astropy only):** the R5.57 first-class admission
  (`crates/hh-env/src/verifier_bound.rs`, ADR-0422): the ten
  `swe-bench-astropy-2` specs carry `task_id` + the actually enforced
  decl `max_test_timeout_sec: 360` and `admit_verifier_bound` stamps
  the admitted `900` + the verbatim `verifier_bound_admission`
  record; `verifier_deadline_ms` is the sanctioned derivation on the
  `tc-verdict` + `tc-vendored` dispatches; `slot_echo` members ride
  the slot rows verbatim. The recorded proximate cause of the prior
  failure was the bootstrap cwd/`pytest` bake, NOT the bound — a
  still-failing bootstrap records `inconclusive{reason}` verbatim,
  never a forced pass.
- **Same frozen protocol:** the manifest is consumed verbatim
  (`protocol_id 9edc5327f89e2d66f8003bedc2530bc3e7d37326aad82cdfcab70465eda60a5f`,
  seed `parity-tranche-2026-11-05:seed:v3`, no re-pick) — identical
  slot_refs, arm bindings, lanes, rounds. These are named supplement
  attempts: `supplement_epoch: s4-completion-gce` and
  `prior_disposition` (the egress-run's verbatim disposition) are
  recorded on every row; the original inconclusive records stand.
- **Scope:** exactly the 50 prior-inconclusive slots (5 tasks × 10):
  `eval-mteb.hard` (OOM rc 137 on `pip install mteb==1.36.8`), `oom`
  (ENOSPC rc 125 writing the torch blob), `qemu-alpine-ssh` (`apt-get`
  rc 100, deb404), `fix-git` (deleted upstream, rc 128 —
  permanently unbuildable) — all
  `inconclusive{image_unavailable:client_build_failed}` — plus
  `swe-bench-astropy-2` ×10
  `inconclusive{verifier_unavailable:no_parseable_verdict}`.
- **Platform:** the backend is `amd64` — task images build
  `linux/amd64` natively (the prior local backend built `linux/arm64`
  under applehv; a task's own `--platform` pin is honored where the
  Dockerfile declares one). The platform is a backend fact recorded
  verbatim per build record, never a re-pick.

## Honest bounds (unchanged from the frozen run and prior supplement)

- Real execution only; per-slot verbatim records; never a fabricated
  verdict. A task that still can't genuinely run records
  `inconclusive{reason}`/`unresolved` verbatim.
- `fix-git`'s ten slots record `inconclusive{upstream_deleted}`
  verbatim — a deleted upstream is not a resource bound; the client
  build is attempted once on the remote to confirm the upstream
  stays deleted (the verbatim rc 128 recorded); never executed, never
  fabricated.
- Same verifier channels: `verbatim` (run-tests.sh — now genuinely
  runnable under bridge egress) preferred, then `vendored_offline`;
  the task's own PytestParser decides `resolved|unresolved`.
- Same per-subject envelopes: ≤30 min admitted wall, verifier per
  R5.57's admitted bound (≤15 min envelope), sequential cells (the
  driver's container sweep is indiscriminate — 1 at a time).
- Same stop conditions; `podman system prune` on the remote when VM
  free < 8 GiB (`df -BG /` via `gcloud compute ssh`).
- `failed`/`inconclusive{cell_setup_failed}` are machinery outcomes,
  never exclusions; setup retries stay inside the frozen ≤1 bound.

## Emit target

- `docs/build/reports/2026-11-06-s4-completion.json`
  (`hh-s4-completion/1`) — per-slot verdicts, `egress_profile`/helper
  variant verbatim, denominators, costs incl. billed instance-hours,
  `protocol_id` echo; the three prior packets stay byte-untouched.
- Work root `~/tb-work/s4-completion/` (external, not committed);
  drivers `/tmp/s4-cell` (patched — first-class variant + remote ws
  sync), `/tmp/s4-completion-orch.py`,
  `/tmp/s4-completion-assemble.py` — scratch run machinery, same class
  as `/tmp/s4-egress-*`; the R5.56/R5.57 in-repo machinery carries the
  variant and the bound.

---

## Execution log (appended)

Everything below is verbatim record.

### Completion supplement results (2026-11-06 13:34→13:54 EST, verbatim)

50/50 supplement slots attempted on the GCE backend — zero dropped,
zero retried beyond the frozen bound, zero fabricated. Packet:
`docs/build/reports/2026-11-06-s4-completion.json`
(`hh-s4-completion/1`, `supplement_epoch: s4-completion-gce`).

- **Dispositions:** `executed` **20/50** (10 `resolved` — `oom` ×10;
  10 `unresolved` — `eval-mteb.hard` ×10, the task's own verifier
  decided `test_mteb_performance` failed on every slot — a genuine
  participant outcome); `inconclusive{…}` **30/50** verbatim:
  `inconclusive{upstream_deleted}` ×10 — `fix-git` (the once-attempted
  remote client build failed rc 128 in 14.77 s on `RUN bash
  /app/setup.sh` — `git clone` of the deleted
  `TheMikeMerrill/personal-site` repo; permanently unbuildable, never
  executed); `inconclusive{image_unavailable:client_build_failed}`
  ×10 — `qemu-alpine-ssh` (rc 100 in 15.49 s — the verbatim deb404:
  `Failed to fetch …/qemu-system-gui_5.2+dfsg-11+deb11u5_amd64.deb
  404 Not Found`, an upstream pool removal no resource bound lifts);
  `inconclusive{verifier_unavailable:no_parseable_verdict}` ×10 —
  `swe-bench-astropy-2` under the R5.57 admitted bound (the cells ran
  all four dispatches `observed:applied` with the stamped
  `verifier_bound_admitted: 900`; the verbatim channel still exited
  rc 127 inside the bootstrap and the vendored channel still found no
  pytest module — the recorded proximate cause is the bootstrap
  cwd/`pytest` bake, NOT the bound; recorded verbatim, never a forced
  pass). `failed` 0; `unattempted` 0.
- **Builds (verbatim, linux/amd64 on the remote):** `eval-mteb.hard`
  client rc **0** in 577.51 s — the rc-137 OOM lifted on 32 GiB; `oom`
  client rc **0** in 481.66 s — the ENOSPC lifted on 187 GiB free;
  `swe-bench-astropy-2` client rc 0 in 43.99 s (derived bake still
  carries no pytest — probe `ModuleNotFoundError`, the recorded bake
  gap verbatim); `qemu-alpine-ssh` rc 100; `fix-git` rc 128. Per-task
  images `podman rmi -f`'d after each task's slots; remote ends with 0
  containers / 0 images.
- **Machinery (verbatim):** the R5.56 first-class variant carried the
  cells — `admit_declared_deps` stamped every admitted record
  (`egress_profile: declared_deps` / `cell_network: bridge` /
  `policy_net_mode: public` / `declared_deps` read back from the
  record, never asserted ahead of the admission) and
  `podman_helper_args` appended `--network bridge` to the stock
  `target/debug/hh-helper` argv (`helper_variant: hh-helper --network
  (R5.56 first-class; stock binary)` per row — the scratch
  `/tmp/s4-helper-egress` was never used). The R5.57 admission stamped
  the ten astropy specs 360→900 (`admit_verifier_bound`;
  `verifier_deadline_ms` derived the dispatch deadlines;
  `verifier_bound_admitted` echoed per row). `sentinel_intact` /
  `observer_survived` true on every cell; seals `sealed`; cells ran
  sequentially.
- **Backend sync (verbatim):** `remote_host: podman-s4-backend` per
  row; `remote_ws_syncs` totals **130** across the 30 executed cells
  (1 push + 3–4 pulls per cell over `gcloud compute ssh` tar streams —
  the byte-identical `/Users/stevenvitali/…` path on both sides).
- **Denominators (recomputed over the supplement, verbatim):**
  planned 50/50 recorded; cell attempts 30; mediated dispatches 100;
  first subjects 4/10 supplement-first executed (2 resolved); every
  attempt identity retained (`attempts[]`).
- **§4.2 fidelity denominators (verbatim):** pressure opportunities
  10; compactions started 20; `applied` statuses 30; changed_requests
  20; triggered first subjects 2; served_deviations 5 (same
  served-variant class as the prior epochs — recorded, never
  relabelled).
- **Costs (verbatim):** total slot wall 1,124,199 ms (~19 min across
  the 30 sequential cells); output bytes 218,700; tokens_freed 23,920;
  USD **$0.00**; provider exposure 0; no model calls. Backend billed
  time: instance created 2026-11-06T16:32Z (recorded in
  `~/tb-work/gce-backend.md`); ~2.4 h billed at ≈$0.30/hr through the
  run's end ≈ **$0.72** cumulative — inside the ~$20 budget.
- **Stops:** none fired — no integrity/treatment/cap/isolation/
  leakage/drift/ceiling stop triggered; no wall_cap_breach flags; no
  schedule deviation.
- **Combined cohort (confirmation + egress + completion, verbatim):**
  **270/300** slots carry real receipts (**200 resolved / 70
  unresolved**); **30** remain inconclusive with verbatim reasons
  (`upstream_deleted` ×10, `image_unavailable:client_build_failed`
  ×10 — qemu-alpine-ssh deb404, `verifier_unavailable:
  no_parseable_verdict` ×10 — astropy bootstrap); 0 failed; first
  subjects **54/60** executed (40 resolved). The original 300 records
  and both prior packets stand byte-untouched; this packet is the
  named completion epoch.
- **Machinery state left behind:** the VM stays RUNNING and
  provisioned (the deprovision is R5.59's mandatory tracked AC — not
  this row's); remote podman clean (0 containers, 0 images, 186 GiB
  free); SSH forward still up (`/tmp/podman-gce/podman.sock` +
  localhost:23750); work root `~/tb-work/s4-completion/` (stores/ws/
  slot records/specs — external, not committed); drivers `/tmp/s4-cell`
  (patched — first-class admissions + remote ws sync),
  `/tmp/s4-completion-orch.py`, `/tmp/s4-completion-assemble.py`.
- **Verdict: `partial`** — the supplement executed 20/50
  prior-inconclusive slots with real receipts; the remaining 30 are
  honest bounds the bigger backend cannot lift (deleted/moved
  upstreams ×20, a task-side verifier bootstrap gap ×10). No
  retrospective smaller manifest; denominators retained verbatim.

