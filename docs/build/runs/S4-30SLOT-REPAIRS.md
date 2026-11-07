# S4-30SLOT-REPAIRS — three verbatim-recorded repairs over the 30-slot inconclusive residue (protocol_id 9edc5327)

Status: the backend, the three repair classes, the admission mechanism,
and the honesty bounds in this header block were declared before the
repair slots ran. Dated execution results are appended below. (Not a
manifest ticket — operator-authorized supplement work between GATE-G19
and R5.59; the manifest/nextTicket state is untouched.)

- Date: 2026-11-06
- Harness: devin-desktop/swe-2-high/subagent
- Cost: $0 — no model calls. Same deterministic participants (each
  task's committed oracle `solution.sh`/`solution.yaml`).
- Operator authorization (verbatim): "let's do these" — operator chat,
  received 2026-11-06, authorizing all three named repairs to the
  30-slot inconclusive residue of `s4-completion-gce`.
- Prior records (untouched): `docs/build/runs/S4-CONFIRMATION.md` +
  `docs/build/reports/2026-11-05-s4-confirmation.json`
  (`hh-s4-confirmation/1`); `docs/build/runs/S4-EGRESS.md` +
  `docs/build/reports/2026-11-05-s4-egress-cohort.json`
  (`hh-s4-egress-supplement/1`, `supplement_epoch:
  egress-profile-rerun`); `docs/build/runs/S4-COMPLETION.md` +
  `docs/build/reports/2026-11-06-s4-completion.json`
  (`hh-s4-completion/1`, `supplement_epoch: s4-completion-gce` — the
  50-slot epoch that left this 30-slot residue);
  `docs/build/reports/2026-11-06-r5.57-verifier-bound-admission.json`
  (`hh-verifier-bound-admission/1`).
- Pre-dispatch guard: `check-supplement-guard.sh` PASS, route
  `HOLD — gate GATE-G19 pending — RETURN PASS` (a pending gate, not a
  missing implementation; GATE-G19/R5.59 unchanged — this epoch only
  shrinks the honest cohort residue they rule over).

## The backend (unchanged)

GCE `podman-s4-backend` — project `harnessharness-evidence-8d7a99`,
`e2-standard-8` + 200 GB `pd-balanced`, `us-central1-a`, debian-12,
podman 4.3.1 rootful via `podman-remote.service`, reached through the
same recorded SSH forward (`CONTAINER_HOST=unix:///tmp/podman-gce/
podman.sock`; `DOCKER_HOST=tcp://localhost:23750`), per
`docs/build/runs/S4-COMPLETION.md`'s declared block and
`~/tb-work/gce-backend.md`. The staged `/work` tree is mirrored to the
byte-identical path on the VM via tar-over-`gcloud compute ssh`
(`remote_ws_syncs`/`remote_ws_push` per row). linux/amd64 native
builds, recorded verbatim.

## The three repairs (declared — verbatim per slot)

Each slot row carries `repair_class` + the class member verbatim; the
verbatim rewritten/derived build inputs live in the per-task build
records (`~/tb-work/s4-repairs/logs/*-build.json`), never the dataset.

- **swe-bench-astropy-2 ×10 — `machinery_repair: declared_workdir`.**
  OUR driver's defect, verbatim: the cell exec'd the oracle solution +
  verifier at `cd /app` while the task image declares
  `WORKDIR /app/astropy` — every relative path in `run-tests.sh`
  (`patch -p1 -i /app/test_patch.diff` → `astropy/io/…`, `sed
  pyproject.toml`, `pip install -e .[test]`, `pytest
  astropy/io/ascii/tests/test_qdp.py`) and in `solution.sh` resolves
  only at the repo root. The parity H-side run resolves this task —
  the task itself works. Repair: `/tmp/s4-cell`'s spec gains
  `task_workdir` read verbatim from `podman image inspect`
  `.Config.WorkingDir` of the built cell image; all four dispatches
  exec `cd <task_workdir>` (empty spec = the prior `/app` default,
  echoed verbatim in the slot record). Same repair class as the ANSI
  fix — a driver defect repaired, task files byte-untouched.
- **qemu-alpine-ssh ×10 — `env_repair: archive_mirror`.** Upstream
  drift, verbatim: EOL bullseye dropped off deb.debian.org — the
  client build's `apt-get` hit the recorded rc-100 deb404. The
  RECORDED environment admission builds a mirror-patched base
  `localhost/s4-mirror/debian-bullseye-slim` (= `debian:bullseye-slim`
  + `sed 's|deb.debian.org/debian|archive.debian.org/debian|g;
  s|security.debian.org/debian-security|archive.debian.org/debian-security|g'`
  — all default suites verified resolving on archive.debian.org) and
  builds the client from the task Dockerfile with ONLY the `FROM`
  line rewritten to the patched base; the verbatim rewritten
  Dockerfile is in the build record; no task file changes.
- **fix-git ×10 — `env_repair: vendored_mirror`.** Deleted upstream,
  verbatim: `setup.sh` clones `github.com/TheMikeMerrill/personal-site`
  (rc 128). Honest search, verbatim: `gh api` 404 on the repo; no
  `personal-site` repo/fork under TheMikeMerrill's 11 public repos;
  pinned commit `d7d3e4b` absent from `mldangelo/personal-site` and
  every TheMikeMerrill repo; terminal-bench `main` STILL clones the
  deleted URL — no task-version admission exists. Mirror found: the
  Software Heritage archive holds a full snapshot of the deleted
  origin — `swh:1:snp:b2cbd5476b5a00d133f224e990498de4e06bad01`
  (origin visit 8, 2026-04-04T11:00:17Z, status full); the vault
  git-bare cook is vendored verbatim at
  `~/tb-work/vendored/fix-git/personal-site.git` (~31 MiB, all
  branches; pinned commit
  `d7d3e4ba9350f634d92d39ace2b471433ee57d50` "off the job market woo"
  verified in `refs/heads/master` history via `git cat-file -t` =
  `commit`; master tip `19b5d101…`). The client build runs the task
  Dockerfile with ONLY added `COPY vendor/personal-site.git
  /app/vendor/` + `git config --system url.insteadOf` lines — the
  verbatim `setup.sh` clone is redirected to the archived mirror for
  that leg only (config unset right after); every task file
  byte-identical; the same upstream objects materialize.

## Honest bounds (unchanged)

- Real execution only; per-slot verbatim records; never a fabricated
  verdict. A task that still can't genuinely run records
  `inconclusive{reason}`/`unresolved` verbatim.
- Same frozen protocol: `protocol_id
  9edc5327f89e2d66f8003bedc2530bc3e7d37326aad82cdfcab70465eda60a5f`,
  seed `parity-tranche-2026-11-05:seed:v3`, no re-pick — identical
  slot_refs, arm bindings, lanes, rounds. Named supplement attempts:
  `supplement_epoch: s4-30slot-repairs-gce` and `prior_disposition`
  (the completion run's verbatim disposition) on every row.
- Same admissions retained verbatim: R5.56 first-class
  `egress_profile: declared_deps` → stock `hh-helper --network
  bridge` (`podman_helper_args`; declared `host_bridge` residual);
  the R5.57 verifier-bound admission on the ten astropy specs
  (`verifier_bound_admitted: 900` echoed per row).
- Same verifier channels: `verbatim` (run-tests.sh) preferred, then
  `vendored_offline`; the task's own PytestParser decides
  `resolved|unresolved`.
- Same per-subject envelopes (≤30 min admitted wall; verifier ≤ the
  admitted bound; sequential cells; ≤1 setup retry); remote
  `podman system prune` when VM free < 8 GiB.
- `failed`/`inconclusive{cell_setup_failed}` are machinery outcomes,
  never exclusions.

## Emit target

- `docs/build/reports/2026-11-06-s4-30slot-repairs.json`
  (`hh-s4-30slot-repairs/1`) — per-slot verdicts, repair-class members
  verbatim, denominators recomputed over all epochs, costs incl.
  billed instance-hours, `protocol_id` echo; the three prior packets
  stay byte-untouched.
- Work root `~/tb-work/s4-repairs/` (external, not committed); drivers
  `/tmp/s4-cell` (patched — `task_workdir` machinery repair),
  `/tmp/s4-repairs-orch.py`, `/tmp/s4-repairs-assemble.py`; vendored
  mirror `~/tb-work/vendored/fix-git/personal-site.git` (SWH git-bare).

---

## Execution log (appended)

Everything below is verbatim record.

### Repairs supplement results (2026-11-06 15:13→16:12 EST, verbatim)

30/30 supplement slots attempted on the GCE backend — zero dropped,
zero retried beyond the frozen bound, zero fabricated. Packet:
`docs/build/reports/2026-11-06-s4-30slot-repairs.json`
(`hh-s4-30slot-repairs/1`, `supplement_epoch: s4-30slot-repairs-gce`).

- **Dispositions:** `executed` **30/30** — `resolved` **20**
  (`fix-git` ×10 under `env_repair: vendored_mirror`; `swe-bench-
  astropy-2` ×10 under `machinery_repair: declared_workdir`);
  `unresolved` **10** (`qemu-alpine-ssh` under `env_repair:
  archive_mirror` — the task's own verifier decided `test_sshpass`
  failed on every slot; a genuine participant outcome, never
  relabelled). `inconclusive{…}` **0**; `failed` 0; `unattempted` 0.
- **Repairs (verbatim):** machinery `declared_workdir` ×10 —
  `task_workdir: /app/astropy` inspected from the built cell image
  verbatim and echoed per row; env `archive_mirror` ×10 —
  `localhost/s4-mirror/debian-bullseye-slim` built rc 0 (apt-get
  update clean on archive.debian.org; verbatim rewritten Dockerfile
  in the build record), client build rc **0** in 145.89 s (the prior
  rc-100 deb404 lifted); env `vendored_mirror` ×10 — SWH git-bare
  snapshot `swh:1:snp:b2cbd547…` vendored, `git url.insteadOf`
  redirected the verbatim setup.sh clone for the setup leg only,
  client build rc 0 (the prior rc-128 lifted).
- **Builds (verbatim, linux/amd64 on the remote):** `fix-git` client
  rc 0 (mirror leg; `RUN bash /app/setup.sh` cloned the archived
  objects and reset `--hard d7d3e4b`); `qemu-alpine-ssh` client rc 0
  in 145.89 s + derived rc 0; `swe-bench-astropy-2` client/derived rc
  0. Per-task images `podman rmi -f`'d after each task's slots; remote
  ends with 0 containers; `localhost/s4-mirror/debian-bullseye-slim`
  retained as the recorded admission artifact; 186 GiB free.
- **Machinery (verbatim):** the R5.56 first-class variant carried the
  cells (`egress_profile: declared_deps` / `cell_network: bridge` /
  `policy_net_mode: public` / `declared_deps` stamped by
  `admit_declared_deps`; stock `hh-helper --network bridge`). The
  R5.57 admission stamped the ten astropy specs (`verifier_bound_
  admitted: 900` echoed per row). `sentinel_intact` /
  `observer_survived` true on every cell; seals `sealed`; cells ran
  sequentially; `remote_ws_syncs` 120 (1 push + 3 pulls per cell).
- **Denominators (recomputed over the supplement, verbatim):**
  planned 30/30 recorded; cell attempts 30; mediated dispatches 90;
  first subjects 6/6 supplement-first executed (4 resolved); every
  attempt identity retained (`attempts[]`).
- **§4.2 fidelity denominators (verbatim):** pressure opportunities
  10; compactions started 20; completions (`applied`) 30;
  changed_requests 20; triggered first subjects 2; served_deviations
  5 (same served-variant class as the prior epochs — recorded, never
  relabelled).
- **Costs (verbatim):** total slot wall 2,668,051 ms (~44 min across
  the 30 sequential cells); output bytes 1,997,975; tokens_freed
  18,200; USD **$0.00**; provider exposure 0; no model calls.
  Backend billed time: instance created 2026-11-06T16:32Z (recorded
  in `~/tb-work/gce-backend.md`); ~4.7 h billed at ≈$0.30/hr through
  the run's end ≈ **$1.41** cumulative — inside the ~$20 budget.
- **Stops:** none fired — no integrity/treatment/cap/isolation/
  leakage/drift/ceiling stop triggered; no wall_cap_breach flags; no
  schedule deviation.
- **Combined cohort (confirmation + egress + completion + repairs,
  verbatim):** **300/300** slots carry real receipts (**220 resolved
  / 80 unresolved**); **0** inconclusive; 0 failed; 0 unattempted;
  first subjects **60/60** executed (44 resolved). The three prior
  packets stand byte-untouched; this packet is the named repairs
  epoch.
- **Machinery state left behind:** the VM stays RUNNING and
  provisioned (the deprovision is R5.59's mandatory tracked AC — not
  this work's); remote podman clean (0 containers; mirror base +
  pulled bases only); SSH forward still up; work root
  `~/tb-work/s4-repairs/`; vendored mirror
  `~/tb-work/vendored/fix-git/`; drivers `/tmp/s4-cell` (patched —
  `task_workdir`), `/tmp/s4-repairs-orch.py`,
  `/tmp/s4-repairs-assemble.py`.
- **Verdict: `executed`** — all 30 prior-inconclusive slots executed
  with real receipts under the three verbatim-recorded repairs; the
  full 300-slot frozen cohort now holds real evidence end to end.

## Correction — 2026-11-07 (R6.3 · REQ-XB-222 / AR-03)

The "≈ **$1.41** cumulative" figure above is a rate-card estimate
(≈4.7 h × ≈$0.30/h), not a billing read; over the VM's full life
2026-11-06T16:32Z–22:15:40Z (≈ 5.73 h) the rate-card estimate is ≈ $1.72
(design/32 B3 F3/F5). Billing was never read; the "$1.41" text is
retained verbatim per REQ-XB-222.
