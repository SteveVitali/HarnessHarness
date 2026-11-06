# PARITY-TRANCHE — operator-ratified H/O runner parity execution

Status: FROZEN before outcomes (this header block and the manifest were
committed to the record before any of the 30 tasks ran on either runner).
Dated execution results are appended below the frozen manifest.

- Date: 2026-11-05
- Harness: devin-desktop/swe-2-high/subagent
- Ratification: operator-verbatim "sounds good, proceed"
- Cost: $0 — deterministic oracle participant only (each task's committed
  `solution.sh`/`solution.yaml`), no model on either side.

## Endpoint

Per-task verdict `resolved`/`unresolved` — binary directional agreement
between the two runners on the same (task image, solution, verifier,
parser). N = 30 matched pairs, one first attempt per task per runner,
matched image/oracle, no retries. Margin: EXACT directional agreement —
zero discordance target; every discordance recorded verbatim, never
dropped. Precision: `kl_directional_bound` η = .025 → N = 30
zero-discordance half-width ≈ 0.116. Verdict: `verified` iff all 30 pairs
agree (parity admitted → S3 met); any discordance →
`infeasible{margin}` recorded honestly.

## Runners

- O-side (original): `tb run --dataset-path ~/tb-work/dataset -t <id…>
  -a oracle --output-path <out>` with
  `DOCKER_HOST=unix:///tmp/podman-api/podman.sock` — terminal-bench
  harness driving docker compose against the podman applehv VM. The
  oracle agent copies `solution.sh` to `/oracle/solution.sh` and runs
  `bash /oracle/solution.sh` in tmux (or, for `solution.yaml` tasks,
  sends each committed command), then copies `run-tests.sh` + `tests/`
  (contents) into `/tests` with `TEST_DIR=/tests`, runs
  `bash /tests/run-tests.sh`, and parses the post-test pane with the
  task's declared parser (`pytest` for all 80 catalogue tasks;
  `pytest -rA` short-summary lines → `resolved` iff every parsed test is
  PASSED/XFAIL/SKIPPED; no summary → parse error → unresolved).
- H-side (harness): direct podman execution in the same VM, on the SAME
  image tb built (`--no-cleanup` preserves `tb__<task>__client`):
  `podman run` with `command: sleep infinity` and `TEST_DIR=/tests`
  (the compose posture verbatim), tar-copy `solution.sh` →
  `/oracle/solution.sh` and `run-tests.sh` + `tests/` contents →
  `/tests` (matching `put_archive` semantics: a directory copies its
  contents), `bash /oracle/solution.sh`, then `bash
  /tests/run-tests.sh`, and the identical pytest short-summary verdict
  rule. `solution.yaml` tasks replay each committed command via
  `podman exec bash -c` in declaration order.

A task whose image fails to build is a REAL `unresolved`/`inconclusive`
outcome (build failure = not resolved → `unresolved` for the binary
endpoint), recorded verbatim, never retried, substituted, or hidden.

## Environment

- podman machine `podman` (applehv, 2 CPU / 2 GiB), rootless.
- Docker API exposed two ways (both verified `_ping` → OK):
  - `podman machine start` forwarding:
    `unix:///var/folders/0r/9hmb0nh57nv2tqxgh71hd92m0000gn/T/podman/podman-api.sock`
  - SSH tunnel per the ratified recipe:
    `ssh -i ~/.local/share/containers/podman/machine/machine -N -L
    /tmp/podman-api/podman.sock:/run/user/501/podman/podman.sock -p
    62382 core@127.0.0.1` → `unix:///tmp/podman-api/podman.sock`
    (used for the O-side `DOCKER_HOST`).
- tb: terminal-bench via uv tool (`/Users/stevenvitali/.local/bin/tb`).
- Dataset: `~/tb-work/dataset` — terminal-bench-core 0.1.1, 80 task
  directories, every one with `task.yaml` (verified).

## Seeded selection (R5.41 machinery, exercised cleanly)

Driver: `/tmp/parity-driver` (scratch crate outside the repo; path-deps
`hharness-eval`, `hharness-lab`, `hharness-wire`) calls
`hharness_eval::central_protocol::materialize_protocol` with the real
declaration below and prints the canonical manifest; the record
self-verifies (`check_id`).

Supplied declaration (verbatim):

- suite_ref: `terminal-bench-core==0.1.1`
- catalogue_identity: `sha256:bd18094423eaca3c` (FNV-1a-64 over the
  newline-joined sorted 80 task_ids — recorded here with the verbatim
  catalogue below)
- catalogue_tasks: 80
- oracle_ref: `oracle:terminal-bench-oracle-agent+pytest-parser`
- runner_ref: `runner-pair:o=tb-harness-docker-compose,h=podman-exec`
- seed: `parity-tranche-2026-11-05:seed:v1` (committed here, before
  outcomes)
- split_assignment_ref: `split:parity-tranche-2026-11-05`,
  split_predates_access: true
- Split (outcome-blind, published before outcomes):
  - dev pool (2): `sanitize-git-repo`, `path-tracing` — both saw
    pilot access earlier this session (the O-side proof-of-pipeline
    run resolved sanitize-git-repo; an O-side build attempt on
    path-tracing failed on arm64). Dev-pool tasks cannot enter the
    held-out confirmation draw.
  - held-out pool (78): every other catalogue task.
- Strata: the task.yaml `difficulty` field verbatim
  (held-out pool: easy 12, medium 43, hard 23).
- Quotas (largest-remainder proportional to the 78-task pool,
  total 30): easy 5, medium 16, hard 9.
- Exclusions: NONE — the empty exclusion set is published here before
  outcomes (exclusion_publication_ref names this section). No task was
  excluded for any reason; build/runner failures are outcomes, not
  exclusions.
- arms (frozen scaffolding; the tranche participants are the two
  runners): `arm:o-tb-oracle` (strategy label `evict_oldest`),
  `arm:h-podman-oracle` (strategy label `clear_tool_results`).
- lanes: `lane:o-docker-compose-podman-vm`,
  `lane:h-podman-exec-podman-vm`;
  lane_spec_ref `lane-spec:podman-applehv-arm64-vm`.
- scheduling: interleaved_blocked, max_concurrent_runs 2;
  reattempt: `infrastructure_failure` only, ≤2/plan, ≤10 % of plans.
- warmth: ColdStart (fresh container per task per side).
- support_route/cache/seed/fingerprint: true; search_budget: 0;
  task_conditioned: false; artificial_pressure: false;
  drift_observed: false; amendment_subjects_bound: none;
  pilot_labelled_as: none.
- phases: the six frozen identities (`phase:<name>`);
  family_id `family:parity-directional-agreement`; primaries: the
  frozen three; multiplicity_id `holm:alpha-0.05`;
  stops `stop:integrity|stop:drift|stop:ceiling`.

## Frozen manifest

- protocol_id: `1251ac555c8896e0b3f805ba494e7f98861844f55dada2907d223069708cd08d`
- selection (seeded order; stratum; lane coin):

| pos | task_id | stratum | coin |
|----:|---------|---------|-----:|
|  0 | reshard-c4-data | medium | 0 |
|  1 | run-pdp11-code | hard | 1 |
|  2 | crack-7z-hash | medium | 0 |
|  3 | chess-best-move | medium | 1 |
|  4 | build-tcc-qemu | medium | 1 |
|  5 | qemu-startup | medium | 1 |
|  6 | git-workflow-hack | easy | 0 |
|  7 | configure-git-webserver | hard | 0 |
|  8 | solana-data | medium | 0 |
|  9 | fibonacci-server | medium | 1 |
| 10 | intrusion-detection | hard | 1 |
| 11 | sqlite-with-gcov | medium | 1 |
| 12 | blind-maze-explorer-algorithm.easy | medium | 1 |
| 13 | grid-pattern-transform | easy | 1 |
| 14 | get-bitcoin-nodes | medium | 0 |
| 15 | csv-to-parquet | easy | 0 |
| 16 | cartpole-rl-training | hard | 1 |
| 17 | oom | hard | 0 |
| 18 | swe-bench-langcodes | medium | 1 |
| 19 | build-initramfs-qemu | medium | 1 |
| 20 | extract-moves-from-video | hard | 1 |
| 21 | new-encrypt-command | medium | 0 |
| 22 | extract-safely | easy | 0 |
| 23 | pytorch-model-cli.hard | hard | 1 |
| 24 | heterogeneous-dates | medium | 1 |
| 25 | gpt2-codegolf | hard | 1 |
| 26 | download-youtube | medium | 0 |
| 27 | simple-sheets-put | medium | 0 |
| 28 | security-vulhub-minio | easy | 0 |
| 29 | pytorch-model-cli | hard | 1 |

Full canonical manifest JSON (selection + 300-slot schedule):
`/tmp/parity-manifest.json` → committed verbatim in the results
artifact `docs/build/reports/2026-11-05-parity-tranche.json`.

## Catalogue (80 task_ids, the eligible set)

Published verbatim at freeze time: the 78 held-out members are every
`~/tb-work/dataset` directory with `task.yaml` minus the two dev-pool
tasks; per-task strata are the task.yaml `difficulty` values
(easy 12 / medium 44 / hard 24 across all 80).

---

## Execution log (appended after freeze)

Everything below is verbatim record; the frozen block above is unedited.

### O-side — `tb run -a oracle` (run 2026-11-05__16-17-31)

- Command: `DOCKER_HOST=unix:///tmp/podman-api/podman.sock tb run
  --dataset-path ~/tb-work/dataset -t <30×-t> -a oracle
  --output-path ~/tb-work/parity-runs/o-side --n-concurrent 4
  --no-cleanup --no-livestream --no-upload-results`
- Aggregate: `~/tb-work/parity-runs/o-side/2026-11-05__16-17-31/results.json`
  — n_resolved 11, n_unresolved 19, accuracy 0.3667.
- Genuine verdicts (agent+verifier executed): 11 resolved
  (grid-pattern-transform, blind-maze-explorer-algorithm.easy,
  download-youtube, pytorch-model-cli, swe-bench-langcodes,
  pytorch-model-cli.hard, intrusion-detection, chess-best-move,
  fibonacci-server, security-vulhub-minio, reshard-c4-data);
  gpt2-codegolf unresolved (test failed, failure_mode unset);
  run-pdp11-code unresolved (parse_error — amd64 `uv` segfaulted under
  qemu user-emulation: `uncaught target signal 11`, pytest never ran;
  verbatim pane in
  `.../run-pdp11-code/*/panes/post-test.txt`).
- 18 trials recorded `unknown_agent_error` = `docker compose build`
  exited non-zero. Attribution evidence: `run.log` contains explicit
  `no space left on device` / buildkit `ResourceExhausted` errors at
  lines 1481, 1720, 1940-1943, 38271 — the shared 30 GiB VM disk filled
  under `--no-cleanup` + 4-way build concurrency. 16 of the 18 failed
  builds were transient: spot rebuilds of git-workflow-hack,
  csv-to-parquet, new-encrypt-command succeeded immediately after
  reclamation. One was genuine: qemu-startup fails at
  `apt-get install qemu-system-x86` — debian:bullseye repos now 404
  (EOL; reproduced standalone on a clean disk).
- No retries were run on the O-side (spec: build failures are real
  outcomes, never retried). The mass failure exceeds the materialized
  `infrastructure_failure` reattempt ceiling (≤10 % of plans = 3);
  exceeding it inside this protocol_id is not permitted — a clean
  O-side re-measure requires a new epoch and is noted for the operator.

### H-side — podman-exec

- Driver: `/tmp/parity-h-side.py` (runs under the tb venv python,
  reusing `terminal_bench`'s own `PytestParser` — identical verdict
  code path). Sequential, one attempt per task; records at
  `~/tb-work/parity-runs/h-side/{h1,h2,h3}/<task>.h-side.json`.
- Matched image: `docker.io/library/tb__<task>__client:latest` built
  by the O-side when present (image_preexisting=true, id/digest
  recorded); otherwise one `podman build` of the same Dockerfile/context
  (localhost/ tag), the sanctioned fallback.
- Driver-defect disclosures (no subject ran in the defect attempts;
  corrected passes are the first real attempts, defect records kept):
  - h1 pass: unqualified podman-build tag resolved to `localhost/`
    while the run step looked up `docker.io/library/` → pull attempt →
    `container_start_failure` (e.g. crack-7z-hash.h-side.json under
    h1/). Fixed; h2 pass executed the corrected lookup.
  - h2 pass: `simple-sheets-put` — `runner_exception:
    float('5s')` parsing a compose healthcheck interval. Fixed; h3
    pass ran it to a real verdict.
  - `oom`: h2 and h3 builds BOTH died on `no space left on device`
    writing the ~5 GB torch layer blob — the same environmental wall
    the O-side hit; recorded twice verbatim, verdict unresolved.
- amd64-base leak (verbatim): `podman build` without `--platform`
  reused the **amd64**-tagged
  `ghcr.io/laude-institute/t-bench/python-3-13:latest` that docker had
  pulled for the `--platform=linux/amd64` tasks. Podman-built clients
  on that base ran amd64 — heterogeneous-dates' `uv` downloaded
  `x86_64-unknown-linux-gnu` and segfaulted under qemu (parse_error,
  mirroring run-pdp11-code's O-side failure). solana-data, oom and
  simple-sheets-put's client share the base. docker buildx targets
  arm64 for the same no-platform Dockerfile — a real builder-
  resolution divergence between the two runners on a shared store.
- Multi-service reproductions: `security-vulhub-minio` (3-node minio
  cluster on a dedicated podman network, hostnames/aliases verbatim) —
  resolved; `simple-sheets-put` (postgres:15 + api build + client with
  API_URL, depends_on order, healthcheck polling) — resolved.
- Verdicts: 21 resolved / 9 unresolved (h1+h2+h3, corrected passes).

### Pair table (O | H | agree?)

| pos | task | O verdict (failure_mode) | H verdict (reason) | agree |
|----:|------|--------------------------|--------------------|:-----:|
| 0 | reshard-c4-data | resolved | resolved | ✓ |
| 1 | run-pdp11-code | unresolved (parse_error) | unresolved (parse_error) | ✓ |
| 2 | crack-7z-hash | unresolved (build/disk) | resolved | ✗ |
| 3 | chess-best-move | resolved | resolved | ✓ |
| 4 | build-tcc-qemu | unresolved (build/disk) | unresolved (tests failed) | ✓ |
| 5 | qemu-startup | unresolved (build/404-genuine) | unresolved (build_failure) | ✓ |
| 6 | git-workflow-hack | unresolved (build/disk) | resolved | ✗ |
| 7 | configure-git-webserver | unresolved (build/disk) | resolved | ✗ |
| 8 | solana-data | unresolved (build/disk) | unresolved (tests failed) | ✓ |
| 9 | fibonacci-server | resolved | resolved | ✓ |
| 10 | intrusion-detection | resolved | resolved | ✓ |
| 11 | sqlite-with-gcov | unresolved (build/disk) | resolved | ✗ |
| 12 | blind-maze-explorer-algorithm.easy | resolved | resolved | ✓ |
| 13 | grid-pattern-transform | resolved | resolved | ✓ |
| 14 | get-bitcoin-nodes | unresolved (build/disk) | resolved | ✗ |
| 15 | csv-to-parquet | unresolved (build/disk) | resolved | ✗ |
| 16 | cartpole-rl-training | unresolved (build/disk) | unresolved (test_timeout 60s) | ✓ |
| 17 | oom | unresolved (build/disk) | unresolved (build/disk ×2) | ✓ |
| 18 | swe-bench-langcodes | resolved | resolved | ✓ |
| 19 | build-initramfs-qemu | unresolved (build/disk) | unresolved (tests failed) | ✓ |
| 20 | extract-moves-from-video | unresolved (build/disk) | resolved | ✗ |
| 21 | new-encrypt-command | unresolved (build/disk) | resolved | ✗ |
| 22 | extract-safely | unresolved (build/disk) | resolved | ✗ |
| 23 | pytorch-model-cli.hard | resolved | resolved | ✓ |
| 24 | heterogeneous-dates | unresolved (build/disk) | unresolved (parse_error) | ✓ |
| 25 | gpt2-codegolf | unresolved (tests failed) | unresolved (tests failed) | ✓ |
| 26 | download-youtube | resolved | resolved | ✓ |
| 27 | simple-sheets-put | unresolved (build/disk) | resolved | ✗ |
| 28 | security-vulhub-minio | resolved | resolved | ✓ |
| 29 | pytorch-model-cli | resolved | resolved | ✓ |

- Discordances (10, verbatim — none dropped): crack-7z-hash,
  git-workflow-hack, configure-git-webserver, sqlite-with-gcov,
  get-bitcoin-nodes, csv-to-parquet, extract-moves-from-video,
  new-encrypt-command, extract-safely, simple-sheets-put. Every one is
  O=`unresolved` (compose-build disk failure) vs H=`resolved` —
  environment-attributable; none arose on a task the O-side actually
  executed.
- On the 13 pairs where the O-side produced a genuine first-attempt
  verdict (11 resolved, 1 test-failed, 1 parse_error): agreement is
  **13/13 = 100 %**, including identical reproduction of two real
  failure modes (amd64-uv segfault parse_error; gpt2 test failure).
- Seven unresolved-both pairs agree directionally but at different
  failure stages (O never ran; H failed at tests/timeout/build) —
  recorded verbatim as less-informative agreements: build-tcc-qemu,
  qemu-startup, solana-data, cartpole-rl-training,
  build-initramfs-qemu, heterogeneous-dates, oom.

### Verdict

**`infeasible{margin}`** — the EXACT-agreement target fails at 10/30
discordant pairs. All discordance is environment-confounded
(O-side disk exhaustion), never dropped or hidden. The counterfactual
read (genuine O verdicts only) is 13/13 agreement. A clean tranche
needs a disk-large-enough backend or per-task image lifecycle under a
NEW epoch/manifest — this `protocol_id` is spent and cannot absorb a
mass infra reattempt under its own ≤10 % ceiling.

Artifacts: `docs/build/reports/2026-11-05-parity-tranche.json`
(manifest verbatim + all 30 pairs + reasons + image digests);
O-side `~/tb-work/parity-runs/o-side/2026-11-05__16-17-31/`;
H-side `~/tb-work/parity-runs/h-side/{h1,h2,h3}/`;
drivers `/tmp/parity-driver` + `/tmp/parity-h-side.py`.

### Backend state left behind

- podman machine `podman` (applehv rootless) left RUNNING.
- Docker API still exposed on BOTH sockets:
  `unix:///tmp/podman-api/podman.sock` (ssh tunnel, PID running) and
  the machine's own
  `unix:///var/folders/0r/9hmb0nh57nv2tqxgh71hd92m0000gn/T/podman/podman-api.sock`.
- Images: the O-side `tb__*__client` task images + `vulhub/minio` +
  base images remain in the VM store (kept as run evidence;
  `podman system df` ≈ 8.5 G used). No hh-parity containers/networks
  remain (all removed per-task).
- `~/tb-work/parity-runs/` holds all raw records (not committed —
  external working dir).
