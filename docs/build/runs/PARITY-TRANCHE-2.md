# PARITY-TRANCHE-2 — fresh-epoch H/O runner parity execution

Status: FROZEN before outcomes (this header block and the manifest were
committed to the record before any of the 30 tasks ran on either runner).
Dated execution results are appended below the frozen manifest.

- Date: 2026-11-05
- Harness: devin-desktop/swe-2-high/subagent
- Ratification: operator-verbatim "sounds good, proceed" (design carried
  unchanged from the spent tranche)
- Cost: $0 — deterministic oracle participant only (each task's committed
  `solution.sh`/`solution.yaml`), no model on either side.
- Provenance: this is a NEW epoch. The prior tranche
  (`docs/build/runs/PARITY-TRANCHE.md`, protocol_id
  `1251ac555c8896e0…`, seed `parity-tranche-2026-11-05:seed:v1`) is
  spent: 18/30 O-side first attempts were confounded by
  `no space left on device` in the 30 GiB VM (10/30 discordances, all
  environment-attributable). The operator expanded available disk
  (~44 GiB free on the host that backs the VM's sparse disk; 27 GiB
  free inside the VM after `podman system prune -af --volumes`). This
  tranche uses a NEW seed and NEW protocol_id — never a retry of the
  spent manifest. The reattempt ceiling inside the old protocol_id
  could not absorb a mass infra re-measure, which is why a new epoch
  was required rather than an in-protocol retry.

## Endpoint

Per-task verdict `resolved`/`unresolved` — binary directional agreement
between the two runners on the same (task image, solution, verifier,
parser). N = 30 matched pairs, one first attempt per task per runner,
matched image/oracle, no retries. Margin: EXACT directional agreement —
zero discordance target; every discordance recorded verbatim, never
dropped. Precision: `kl_directional_bound` η = .025 → N = 30
zero-discordance half-width ≈ 0.116. Verdict: `verified` iff all 30
pairs agree (parity admitted → S3 met); any discordance →
`infeasible{margin}` recorded honestly.

## Runners

Identical to the ratified tranche-1 spec, plus two environment fixes
recorded verbatim:

- O-side (original): `tb run --dataset-path ~/tb-work/dataset -t <id…>
  -a oracle --output-path <out> --n-concurrent 4 --no-cleanup
  --no-livestream --no-upload-results` with
  `DOCKER_HOST=unix:///tmp/podman-api/podman.sock`. Executed in five
  batches of six (manifest order), with `podman system prune -af
  --volumes` BETWEEN batches (after the matching H-side batch) so the
  retained-image set never exceeds ~6 task images + shared bases —
  the disk-hygiene fix for the tranche-1 confound.
- H-side (harness): `/tmp/parity-h-side.py` — direct podman execution
  reproducing tb's oracle verdict pipeline verbatim (same
  PytestParser code path). Tranche-2 changes vs tranche-1 driver:
  `podman build` now pins `--platform` (compose-declared platform if
  any, else `linux/arm64`) — the fix for the amd64-base tag leak the
  prior run found (a stale amd64-tagged base image was reused by
  podman's no-platform build resolution). No selected task declares a
  `platform:` in docker-compose this epoch, so all builds pin arm64.

A task whose image fails to build is a REAL `unresolved` outcome
(build failure = not resolved), recorded verbatim, never retried,
substituted, or hidden — same rule as tranche 1. Driver-defect
attempts (no subject ran) are disclosed, not counted.

## Environment

- podman machine `podman` (applehv, 2 CPU / 2 GiB / 30 GiB sparse),
  rootless; host backing store ≈44 GiB free so the sparse disk can
  grow to its 30 GiB ceiling.
- Docker API: `unix:///tmp/podman-api/podman.sock` (SSH tunnel per the
  ratified recipe; `_ping` → OK verified 2026-11-05).
- tb: terminal-bench via uv tool (`/Users/stevenvitali/.local/bin/tb`).
- Dataset: `~/tb-work/dataset` — terminal-bench-core 0.1.1, 80 task
  directories, every one with `task.yaml` (verified).

## Seeded selection (R5.41 machinery)

Driver: `/tmp/parity-driver` (scratch crate outside the repo; path-deps
`hharness-eval`, `hharness-lab`, `hharness-wire`) calls
`hharness_eval::central_protocol::materialize_protocol` with the
declaration below and prints the canonical manifest; the record
self-verifies (`check_id`). Same declaration as tranche 1 except the
seed.

Supplied declaration (verbatim, deltas vs tranche 1 marked ‹NEW›):

- suite_ref: `terminal-bench-core==0.1.1`
- catalogue_identity: `sha256:bd18094423eaca3c` (FNV-1a-64 over the
  newline-joined sorted 80 task_ids — same catalogue, re-verified:
  all 80 task.yaml `difficulty` values match the recorded strata)
- catalogue_tasks: 80
- oracle_ref: `oracle:terminal-bench-oracle-agent+pytest-parser`
- runner_ref: `runner-pair:o=tb-harness-docker-compose,h=podman-exec`
- seed: `parity-tranche-2026-11-05:seed:v2` ‹NEW› — committed here,
  before outcomes
- split_assignment_ref: `split:parity-tranche-2026-11-05`,
  split_predates_access: true
- Split (outcome-blind, published before outcomes):
  - dev pool (2): `sanitize-git-repo`, `path-tracing` — both saw
    pilot access earlier in the session history (O-side pipeline
    probe resolved sanitize-git-repo; an O-side build attempt on
    path-tracing failed on arm64). Dev-pool tasks cannot enter the
    held-out confirmation draw. The 30 tranche-1 draws are NOT moved
    to dev: tranche 1 is a spent epoch, not pilot access — the
    operator-ratified design holds the dev pool at these two.
  - held-out pool (78): every other catalogue task.
- Strata: task.yaml `difficulty` verbatim (held-out pool: easy 12,
  medium 43, hard 23).
- Quotas (largest-remainder proportional to 78, total 30): easy 5,
  medium 16, hard 9.
- Exclusions: NONE — the empty exclusion set is published before
  outcomes (exclusion_publication_ref names this section). Build or
  runner failures are outcomes, not exclusions.
- arms (frozen scaffolding): `arm:o-tb-oracle` (`evict_oldest`),
  `arm:h-podman-oracle` (`clear_tool_results`).
- lanes: `lane:o-docker-compose-podman-vm`,
  `lane:h-podman-exec-podman-vm`; lane_spec_ref
  `lane-spec:podman-applehv-arm64-vm`.
- scheduling: interleaved_blocked, max_concurrent_runs 2;
  reattempt: `infrastructure_failure` only, ≤2/plan, ≤10 % of plans.
- warmth: ColdStart; support_route/cache/seed/fingerprint: true;
  search_budget: 0; task_conditioned: false; artificial_pressure:
  false; drift_observed: false; amendment_subjects_bound: none;
  pilot_labelled_as: none.
- phases: the six frozen identities; family_id
  `family:parity-directional-agreement`; primaries: the frozen three;
  multiplicity_id `holm:alpha-0.05`; stops
  `stop:integrity|stop:drift|stop:ceiling`.

## Frozen manifest

- protocol_id: `995d3694e720787112507667528e6dcb12d2ae9cec8df9568056eaca3db75752`
- selection (seeded order; stratum; lane coin):

| pos | task_id | stratum | coin |
|----:|---------|---------|-----:|
|  0 | count-dataset-tokens | easy | 0 |
|  1 | write-compressor | hard | 1 |
|  2 | build-initramfs-qemu | medium | 1 |
|  3 | incompatible-python-fasttext.base_with_hint | medium | 0 |
|  4 | raman-fitting | medium | 0 |
|  5 | openssl-selfsigned-cert | medium | 1 |
|  6 | extract-moves-from-video | hard | 1 |
|  7 | git-multibranch | hard | 1 |
|  8 | password-recovery | hard | 0 |
|  9 | blind-maze-explorer-algorithm.hard | medium | 0 |
| 10 | eval-mteb.hard | medium | 1 |
| 11 | security-vulhub-minio | easy | 1 |
| 12 | fix-permissions | easy | 0 |
| 13 | get-bitcoin-nodes | medium | 0 |
| 14 | processing-pipeline | easy | 1 |
| 15 | build-linux-kernel-qemu | medium | 0 |
| 16 | qemu-startup | medium | 0 |
| 17 | hf-model-inference | medium | 0 |
| 18 | tmux-advanced-workflow | medium | 0 |
| 19 | play-zork | hard | 0 |
| 20 | oom | hard | 1 |
| 21 | qemu-alpine-ssh | medium | 1 |
| 22 | train-fasttext | hard | 0 |
| 23 | sqlite-db-truncate | medium | 1 |
| 24 | pytorch-model-cli.easy | hard | 1 |
| 25 | solana-data | medium | 1 |
| 26 | reshard-c4-data | medium | 0 |
| 27 | fibonacci-server | medium | 0 |
| 28 | grid-pattern-transform | easy | 1 |
| 29 | swe-bench-astropy-2 | hard | 1 |

Full canonical manifest JSON (selection + 300-slot schedule):
`/tmp/parity-manifest-v2.json` → committed verbatim in the results
artifact `docs/build/reports/2026-11-05-parity-tranche-2.json`.

Overlap with the spent tranche-1 draw: 10 tasks (expected under
re-sampling ≈ 30·30/78 ≈ 11.5) — build-initramfs-qemu,
extract-moves-from-video, fibonacci-server, get-bitcoin-nodes,
grid-pattern-transform, oom, qemu-startup, reshard-c4-data,
security-vulhub-minio, solana-data.

## Selection properties (recorded at freeze)

- Multi-service compose: `security-vulhub-minio` only (client + 3
  minio nodes on a dedicated podman network — same reproduction as
  tranche 1).
- `solution.yaml` tasks (keystroke-stream replay):
  incompatible-python-fasttext.base_with_hint, fix-permissions, oom,
  train-fasttext, reshard-c4-data. `hf-model-inference` ships both
  solution.sh and solution.yaml; both runners use .sh (driver
  precedence recorded; same as tranche 1).
- No selected task declares `platform:` in docker-compose — all
  images build/run linux/arm64 natively.
- Declared `max_test_timeout_sec` range 60–600 s
  (build-linux-kernel-qemu 600, swe-bench-astropy-2 360, two at 240,
  three at 120/90).

---

## Execution log (appended after freeze)

Everything below is verbatim record; the frozen block above is unedited.

### Disk hygiene protocol actually executed

Five `tb run` invocations, one per 6-task batch in manifest order
(`--n-concurrent 4 --no-cleanup --no-livestream --no-upload-results`,
`DOCKER_HOST=unix:///tmp/podman-api/podman.sock`). After each batch the
matching H-side driver ran (`/tmp/parity-h-side.py t2 <batch-file>`,
reusing `tb__<task>__client` images where the O-side build had produced
them — `image_preexisting` recorded per task), then
`podman system prune -af --volumes` AND
`docker buildx prune -af` (the buildx cache volume — 9.8–18.4 GB per
cycle — is what `podman system prune` misses and what killed
tranche 1). VM disk returned to 26–27 GiB free before every batch.

### O-side — `tb run -a oracle` (five runs)

| batch | run dir | verdicts |
|-------|---------|----------|
| 1 | `o-side-2/2026-11-05__20-06-24` | raman-fitting R, openssl-selfsigned-cert R, write-compressor R, count-dataset-tokens U(tests), build-initramfs-qemu U(tests), incompatible-python-fasttext.base_with_hint U(build OOM) |
| 2 | `o-side-2/2026-11-05__20-48-18` | extract-moves-from-video R, git-multibranch R, password-recovery R, blind-maze-explorer-algorithm.hard R, security-vulhub-minio R, eval-mteb.hard U(build OOM/137) |
| 3 | `o-side-2/2026-11-05__20-53-13` | fix-permissions R, get-bitcoin-nodes R, processing-pipeline R, build-linux-kernel-qemu U(tests), hf-model-inference U(tests), qemu-startup U(build 404) |
| 4 | `o-side-2/2026-11-05__22-14-03` | tmux-advanced-workflow R, play-zork R, train-fasttext R, sqlite-db-truncate R, qemu-alpine-ssh U(build 404), oom U(build) |
| 5 | `o-side-2/2026-11-05__22-23-36` | pytorch-model-cli.easy R, reshard-c4-data R, fibonacci-server R, grid-pattern-transform R, swe-bench-astropy-2 R, solana-data U(tests) |

O-side totals: 20 resolved / 10 unresolved. Of the 10 unresolved: 5 are
genuine agent+verifier failures (tests failed / parse), 5 are
`unknown_agent_error` compose-build failures — attributed per task:
`eval-mteb.hard` pip install OOM-killed (exit 137, reproduced by the
H-side's own sequential podman build → genuine resource-bound build);
`qemu-startup` and `qemu-alpine-ssh` debian:bullseye 404 (EOL repos,
identical under podman → genuine); `oom` mixed (apt step hit bullseye
404s AND buildx logged `failed to load image: no space left on device`
inside the batch window — partially environment-flavored, recorded
verbatim); `incompatible-python-fasttext.base_with_hint` cc1plus
OOM-killed under 4-way buildx concurrency (transient — the sequential
podman build succeeded → builder divergence).

### H-side — `podman exec` driver

All 30 records at `~/tb-work/parity-runs/h-side/t2/*.h-side.json`,
single pass, first attempt each, no retries. 25 tasks reused the
O-side-built `tb__*__client` image verbatim (`image_preexisting=true`,
id/digest recorded); 5 fell back to one `podman build
--platform=linux/arm64` of the same Dockerfile/context where the O-side
build had failed (fasttext.bwh — succeeded; eval-mteb.hard, qemu-startup,
qemu-alpine-ssh, oom — failed with the same root cause as O-side).

Verdicts: 20 resolved / 10 unresolved.

### Pair table (O | H | agree?)

| pos | task | O verdict (failure_mode) | H verdict (reason) | agree |
|----:|------|------------------------|--------------------|:-----:|
| 0 | count-dataset-tokens | unresolved (tests failed) | unresolved (some_tests_failed) | ✓ |
| 1 | write-compressor | resolved | resolved | ✓ |
| 2 | build-initramfs-qemu | unresolved (tests failed) | unresolved (some_tests_failed) | ✓ |
| 3 | incompatible-python-fasttext.base_with_hint | unresolved (build OOM) | resolved | ✗ |
| 4 | raman-fitting | resolved | resolved | ✓ |
| 5 | openssl-selfsigned-cert | resolved | resolved | ✓ |
| 6 | extract-moves-from-video | resolved | resolved | ✓ |
| 7 | git-multibranch | resolved | resolved | ✓ |
| 8 | password-recovery | resolved | resolved | ✓ |
| 9 | blind-maze-explorer-algorithm.hard | resolved | resolved | ✓ |
| 10 | eval-mteb.hard | unresolved (build OOM/137) | unresolved (build 137) | ✓ |
| 11 | security-vulhub-minio | resolved | resolved | ✓ |
| 12 | fix-permissions | resolved | resolved | ✓ |
| 13 | get-bitcoin-nodes | resolved | resolved | ✓ |
| 14 | processing-pipeline | resolved | resolved | ✓ |
| 15 | build-linux-kernel-qemu | unresolved (tests failed) | unresolved (some_tests_failed) | ✓ |
| 16 | qemu-startup | unresolved (build 404) | unresolved (build 404) | ✓ |
| 17 | hf-model-inference | unresolved (tests failed) | unresolved (some_tests_failed) | ✓ |
| 18 | tmux-advanced-workflow | resolved | resolved | ✓ |
| 19 | play-zork | resolved | resolved | ✓ |
| 20 | oom | unresolved (build 404+ENOSPC) | unresolved (build ENOSPC) | ✓ |
| 21 | qemu-alpine-ssh | unresolved (build 404) | unresolved (build 404) | ✓ |
| 22 | train-fasttext | resolved | resolved | ✓ |
| 23 | sqlite-db-truncate | resolved | resolved | ✓ |
| 24 | pytorch-model-cli.easy | resolved | resolved | ✓ |
| 25 | solana-data | unresolved (tests failed) | unresolved (some_tests_failed) | ✓ |
| 26 | reshard-c4-data | resolved | resolved | ✓ |
| 27 | fibonacci-server | resolved | resolved | ✓ |
| 28 | grid-pattern-transform | resolved | resolved | ✓ |
| 29 | swe-bench-astropy-2 | resolved | unresolved (parsed={} ANSI) | ✗ |

### Discordances (2, verbatim — none dropped)

1. **incompatible-python-fasttext.base_with_hint** — O=`unresolved`
   (`unknown_agent_error`: `docker compose build` OOM-killed
   `cc1plus` compiling the fasttext wheel under 4-way buildx
   concurrency in the 2 GiB VM) vs H=`resolved` (sequential
   `podman build --platform=linux/arm64` of the identical
   Dockerfile/context succeeded first attempt; solution.yaml replay +
   pytest → `all_tests_passed`). Builder/environment-attributable —
   the O-side never reached agent+verifier on a build that is not
   inherently too large (unlike eval-mteb.hard, which OOM'd on both
   builders including the sequential one).
2. **swe-bench-astropy-2** — O=`resolved` (9 test_qdp tests all
   `passed` in `parser_results`) vs H=`unresolved` on the IDENTICAL
   image. Both sides genuinely ran agent + verifier. Mechanism,
   verified verbatim: astropy's pytest config emits ANSI color
   unconditionally; the O-side verdict input is a **tmux
   `capture-pane` render** (ANSI consumed by the terminal emulator →
   clean `PASSED` tokens → 9 parsed), while the H-side feeds the
   **raw `podman exec` byte stream** (`\x1b[32mPASSED\x1b[0m …`)
   into the same `PytestParser`, whose status regex maps
   ANSI-wrapped tokens to UNKNOWN → `parse()` returns `{}` →
   unresolved (`"parsed": {}` recorded verbatim). A genuine-execution
   structural divergence at the capture channel — the substantive
   parity break this tranche exists to detect; NOT environment noise.

On the 25 pairs where the O-side produced a genuine agent+verifier
verdict (20 resolved, 5 unresolved), agreement is 24/25 — the single
miss being the ANSI capture-channel divergence above. Four further
pairs agree at the build-failure stage (both sides unresolved, same
root cause): eval-mteb.hard, qemu-startup, qemu-alpine-ssh, oom. The
fasttext pair is the opposite direction — O-side build failure vs
H-side full resolve — and is discordant.

### Verdict

**`infeasible{margin}`** — the EXACT directional-agreement target fails
at 2/30 discordant pairs, recorded verbatim and never dropped. Unlike
tranche 1 (10/30 discordant, all environment-confounded), this tranche
isolates ONE genuine structural divergence (ANSI byte-level capture
channel) plus one builder-concurrency divergence; on tasks both
runners actually executed, agreement is 24/25. The capture-channel
finding gives parity work a concrete fix target rather than an
environment caveat.

Artifacts: `docs/build/reports/2026-11-05-parity-tranche-2.json`
(manifest verbatim + all 30 pairs + reasons + image digests +
discordance mechanism evidence); O-side
`~/tb-work/parity-runs/o-side-2/2026-11-05__{20-06-24,20-48-18,20-53-13,22-14-03,22-23-36}/`;
H-side `~/tb-work/parity-runs/h-side/t2/`; drivers
`/tmp/parity-driver` + `/tmp/parity-h-side.py` (platform-pinned).

### Backend state left behind

- podman machine `podman` (applehv rootless) left RUNNING; SSH-tunnel
  Docker API socket `unix:///tmp/podman-api/podman.sock` still up
  (`_ping` OK).
- Post-run cleanup executed: `podman system prune -af --volumes` +
  `docker buildx prune -af` → VM disk at ~27 GiB free (3.1 GiB used)
  before shutdown of the run; the buildx builder container
  (`buildx_buildkit_default`, ~240 MB image) remains — tb recreates/
  reuses it on next `docker compose build`.
- No hh-parity or tb task containers/networks remain (all removed
  per-task and by the final prune).
- `~/tb-work/parity-runs/` holds all raw records (external working
  dir, not committed).
