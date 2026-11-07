# PARITY-TRANCHE-3 — post-repair fresh-epoch H/O runner parity execution

Status: FROZEN before outcomes (this header block and the manifest were
committed to the record before any of the 30 tasks ran on either runner).
Dated execution results are appended below the frozen manifest.

- Date: 2026-11-05
- Harness: devin-desktop/swe-2-high/subagent
- Ratification: operator-verbatim "sounds good; do the repairs until
  final parity is achieved."
- Cost: $0 — deterministic oracle participant only (each task's
  committed `solution.sh`/`solution.yaml`), no model on either side.
- Provenance: this is a NEW epoch. Tranche 2
  (`docs/build/runs/PARITY-TRANCHE-2.md`, protocol_id
  `995d3694e7207871…`, seed `parity-tranche-2026-11-05:seed:v2`) is
  spent at 28/30 with verdict `infeasible{margin}`: two discordances,
  both subsequently diagnosed as repairable defects —
  (a) `swe-bench-astropy-2`: H-side machinery defect — the O-side
  verdict input is a tmux `capture-pane` render (ANSI consumed by the
  terminal emulator → clean `PASSED` tokens → 9 parsed), while the
  H-side fed the raw `podman exec` byte stream
  (`\x1b[32mPASSED\x1b[0m …`) into the same `PytestParser`, whose
  first-token status regex maps ANSI-wrapped tokens to UNKNOWN →
  `parsed={}` → unresolved;
  (b) `incompatible-python-fasttext.base_with_hint`: O-side environment
  confound — `cc1plus` OOM-killed under `--n-concurrent 4` buildx
  concurrency in the 2 GiB VM (the identical sequential podman build
  succeeded). Both repairs below are in the same class as the earlier
  in-run machinery repairs (R5.44 precedent); the new epoch justifies
  re-measurement.

## Repairs executed before this epoch (verbatim)

1. **ANSI capture-channel repair (H-side machinery).**
   `/tmp/parity-h-side.py` now strips ANSI escape sequences
   UNCONDITIONALLY from the verifier byte stream before handing it to
   `PytestParser` — reproducing tmux `capture-pane` render semantics
   (the O-side channel): CSI sequences including SGR `\x1b[<params>m`,
   OSC sequences (`\x1b]…BEL`/`\x1b]…\x1b\\`), remaining ESC sequences,
   residual lone ESC, and carriage-return overwrites collapsed to the
   final rendered segment per line. Raw and stripped lengths plus an
   `ansi_stripped` flag are recorded per task. Verified pre-run:
   `\x1b[32mPASSED\x1b[0m …` input reproduces the exact tranche-2
   defect (`parse → {}`); after stripping, the same bytes parse to the
   correct per-test statuses; plain (non-ANSI) output is unchanged.
2. **O-side build-OOM confound repair.** O-side runs now use
   `tb run --n-concurrent 1` (fully sequential compose builds — no
   parallel buildx workers competing for the 2 GiB VM) plus
   `podman system prune -af --volumes` AND `docker buildx prune -af`
   between batches (the buildx cache volume is what `podman system
   prune` misses).

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

Identical to the ratified tranche-2 spec, plus the two repairs above:

- O-side (original): `tb run --dataset-path ~/tb-work/dataset -t <id…>
  -a oracle --output-path <out> --n-concurrent 1 --no-cleanup
  --no-livestream --no-upload-results` with
  `DOCKER_HOST=unix:///tmp/podman-api/podman.sock`. Executed in five
  batches of six (manifest order), with `podman system prune -af
  --volumes` + `docker buildx prune -af` BETWEEN batches (after the
  matching H-side batch) so the retained-image set never exceeds ~6
  task images + shared bases.
- H-side (harness): `/tmp/parity-h-side.py` — direct podman execution
  reproducing tb's oracle verdict pipeline verbatim (same
  PytestParser code path), now with the ANSI-stripped verdict channel
  (repair 1) and the tranche-2 `--platform` build pin.

A task whose image fails to build is a REAL `unresolved` outcome
(build failure = not resolved), recorded verbatim, never retried,
substituted, or hidden — same rule as tranches 1–2. Driver-defect
attempts (no subject ran) are disclosed, not counted.

## Environment

- podman machine `podman` (applehv, 2 CPU / 2 GiB / 30 GiB raw disk —
  fully allocated host-side, so VM writes cannot grow host footprint),
  rootless; VM fs 27 GiB free / 3.1 GiB used verified before freeze.
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
self-verifies (`check_id`). Same declaration as tranche 2 except the
seed.

Supplied declaration (verbatim, deltas vs tranche 2 marked ‹NEW›):

- suite_ref: `terminal-bench-core==0.1.1`
- catalogue_identity: `sha256:bd18094423eaca3c` (FNV-1a-64 over the
  newline-joined sorted 80 task_ids — same catalogue)
- catalogue_tasks: 80
- oracle_ref: `oracle:terminal-bench-oracle-agent+pytest-parser`
- runner_ref: `runner-pair:o=tb-harness-docker-compose,h=podman-exec`
- seed: `parity-tranche-2026-11-05:seed:v3` ‹NEW› — committed here,
  before outcomes
- split_assignment_ref: `split:parity-tranche-2026-11-05`,
  split_predates_access: true
- Split (outcome-blind, published before outcomes):
  - dev pool (2): `sanitize-git-repo`, `path-tracing` — both saw
    pilot access earlier in the session history. Dev-pool tasks
    cannot enter the held-out confirmation draw. Prior-epoch draws
    are NOT moved to dev: those epochs are spent, not pilot access.
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

- protocol_id:
  `9edc5327f89e2d66f8003bedc2530bc3e7d37326aad82cdfcab70465eda60a5f`
- selection (seeded order; stratum; lane coin):

| pos | task_id | stratum | coin |
|----:|---------|---------|-----:|
|  0 | super-benchmark-upet | hard | 1 |
|  1 | raman-fitting | medium | 1 |
|  2 | hello-world | easy | 1 |
|  3 | git-workflow-hack | easy | 1 |
|  4 | build-linux-kernel-qemu | medium | 1 |
|  5 | incompatible-python-fasttext.base_with_hint | medium | 0 |
|  6 | polyglot-c-py | medium | 1 |
|  7 | get-bitcoin-nodes | medium | 1 |
|  8 | configure-git-webserver | hard | 1 |
|  9 | blind-maze-explorer-algorithm.hard | medium | 0 |
| 10 | new-encrypt-command | medium | 0 |
| 11 | eval-mteb.hard | medium | 1 |
| 12 | swe-bench-langcodes | medium | 1 |
| 13 | fix-git | medium | 0 |
| 14 | grid-pattern-transform | easy | 1 |
| 15 | security-vulhub-minio | easy | 0 |
| 16 | oom | hard | 1 |
| 17 | git-multibranch | hard | 0 |
| 18 | password-recovery | hard | 1 |
| 19 | solana-data | medium | 1 |
| 20 | qemu-alpine-ssh | medium | 1 |
| 21 | polyglot-rust-c | hard | 1 |
| 22 | path-tracing-reverse | hard | 0 |
| 23 | organization-json-generator | hard | 0 |
| 24 | jupyter-notebook-server | medium | 0 |
| 25 | swe-bench-astropy-2 | hard | 0 |
| 26 | modernize-fortran-build | easy | 0 |
| 27 | blind-maze-explorer-algorithm.easy | medium | 1 |
| 28 | decommissioning-service-with-sensitive-data | medium | 0 |
| 29 | hf-model-inference | medium | 0 |

Full canonical manifest JSON (selection + 300-slot schedule):
`/tmp/parity-manifest-v3.json` → committed verbatim in the results
artifact `docs/build/reports/2026-11-05-parity-tranche-3.json`.

Overlap with the spent tranche-2 draw: 15 tasks (expected under
re-sampling ≈ 30·30/78 ≈ 11.5) — blind-maze-explorer-algorithm.hard,
build-linux-kernel-qemu, eval-mteb.hard, get-bitcoin-nodes,
git-multibranch, grid-pattern-transform, hf-model-inference,
incompatible-python-fasttext.base_with_hint, oom, password-recovery,
qemu-alpine-ssh, raman-fitting, security-vulhub-minio, solana-data,
swe-bench-astropy-2 — including BOTH tranche-2 discordance tasks
(the ANSI case `swe-bench-astropy-2` and the OOM case
`incompatible-python-fasttext.base_with_hint`), so both repairs are
directly re-exercised on their original subjects.

## Selection properties (recorded at freeze)

- Multi-service compose: `security-vulhub-minio` only (client + 3
  minio nodes on a dedicated podman network — same reproduction as
  tranches 1–2).
- `solution.yaml` tasks (keystroke-stream replay):
  super-benchmark-upet, incompatible-python-fasttext.base_with_hint,
  new-encrypt-command, fix-git, oom. `hf-model-inference` ships both
  solution.sh and solution.yaml; both runners use .sh (driver
  precedence recorded; same as tranches 1–2).
- No selected task declares `platform:` in docker-compose — all
  images build/run linux/arm64 natively.
- Declared `max_test_timeout_sec` range 60–600 s
  (build-linux-kernel-qemu 600, swe-bench-astropy-2 360, three at
  240, four at 120/90).

---

## Execution log (appended after freeze)

Everything below is verbatim record; the frozen block above is unedited.

### Disk hygiene protocol actually executed

Five `tb run` invocations, one per 6-task batch in manifest order
(`--n-concurrent 1 --no-cleanup --no-livestream --no-upload-results`,
`DOCKER_HOST=unix:///tmp/podman-api/podman.sock`). After each batch the
matching H-side driver ran
(`~/.local/share/uv/tools/terminal-bench/bin/python
/tmp/parity-h-side.py t3 <batch-file>`, reusing `tb__<task>__client`
images where the O-side build had produced them —
`image_preexisting` recorded per task), then `podman system prune -af
--volumes` AND `docker buildx prune -af`. VM disk returned to 27 GiB
free before every batch.

**In-run machinery note (recorded verbatim, no task retried):** during
batch 3 the within-batch buildx-cache accumulation (~18 GB) produced
`no space left on device` on the `fix-git` and `oom` image loads — the
same environment-defect class as the tranche-2 OOM confound, now at
image-load rather than compile. From batch 4 onward a watchdog
(`/tmp/parity-t3-diskwatch.sh`) pruned ONLY the buildx cache when VM
`/var` free dropped below 8 GiB (tagged images preserved so H-side
image reuse is unaffected); it never fired in batches 4–5. No task was
re-executed on either side.

### O-side — `tb run -a oracle` (five runs, n-concurrent 1)

| batch | run dir | verdicts |
|-------|---------|----------|
| 1 | `o-side-3/2026-11-05__23-16-40` | super-benchmark-upet R, raman-fitting R, hello-world R, git-workflow-hack R, incompatible-python-fasttext.base_with_hint R, build-linux-kernel-qemu U(tests) |
| 2 | `o-side-3/2026-11-06__00-26-04` | polyglot-c-py R, get-bitcoin-nodes R, configure-git-webserver R, blind-maze-explorer-algorithm.hard R, new-encrypt-command R, eval-mteb.hard U(build OOM/137) |
| 3 | `o-side-3/2026-11-06__00-38-15` | swe-bench-langcodes R, grid-pattern-transform R, security-vulhub-minio R, git-multibranch R, fix-git U(build: image-load ENOSPC), oom U(build: image-load ENOSPC) |
| 4 | `o-side-3/2026-11-06__00-49-37` | password-recovery R, path-tracing-reverse R, organization-json-generator R, solana-data U(tests), polyglot-rust-c U(tests), qemu-alpine-ssh U(build deb 404) |
| 5 | `o-side-3/2026-11-06__00-55-35` | jupyter-notebook-server R, swe-bench-astropy-2 R, modernize-fortran-build R, blind-maze-explorer-algorithm.easy R, decommissioning-service-with-sensitive-data R, hf-model-inference U(tests) |

O-side totals: 22 resolved / 8 unresolved. Of the 8 unresolved: 4 are
genuine agent+verifier failures (tests failed), 4 are
`unknown_agent_error` compose-build failures — attributed per task:
`eval-mteb.hard` pip install OOM-killed (exit 137, reproduced by the
H-side's own sequential podman build → genuine resource-bound);
`qemu-alpine-ssh` debian:bullseye 404 (EOL repos, identical under
podman → genuine); `fix-git` image-load ENOSPC after a buildx
cache-hit on the `setup.sh` clone step (env — and the upstream repo
`github.com/TheMikeMerrill/personal-site` is now deleted: `Repository
not found` verified from host AND inside the VM 2026-11-06, so a
fresh build cannot succeed on either runner); `oom` image-load ENOSPC
(within-batch buildx accumulation, env).

### H-side — `podman exec` driver (ANSI-repaired parser)

All 30 records at `~/tb-work/parity-runs/h-side/t3/*.h-side.json`,
single pass, first attempt each, no retries. 26 tasks reused the
O-side-built `tb__*__client` image verbatim (`image_preexisting=true`,
id/digest recorded); 4 fell back to one `podman build
--platform=linux/arm64` of the same Dockerfile/context where the
O-side build had failed (eval-mteb.hard, qemu-alpine-ssh — same root
cause; fix-git — genuine clone-404 on deleted upstream repo; oom —
ENOSPC before the post-batch prune).

The ANSI repair was load-bearing across the tranche: 17 of 30 verifier
streams contained ANSI escapes (`ansi_stripped=true` recorded per
task). On `swe-bench-astropy-2` — the tranche-2 discordance subject —
the identical O-side-built image produced raw 205,678 B → stripped
189,403 B → 9/9 tests parsed `passed` → resolved (tranche 2 produced
`parsed={}` → unresolved on the same image). On
`incompatible-python-fasttext.base_with_hint` — the tranche-2 OOM
subject — the sequential O-side build succeeded and both runners
reached `resolved`.

Verdicts: 22 resolved / 8 unresolved — the identical set.

### Pair table (O | H | agree?)

| pos | task | O verdict (failure_mode) | H verdict (reason) | agree |
|----:|------|------------------------|--------------------|:-----:|
| 0 | super-benchmark-upet | resolved | resolved (all_tests_passed) | ✓ |
| 1 | raman-fitting | resolved | resolved (all_tests_passed) | ✓ |
| 2 | hello-world | resolved | resolved (all_tests_passed) | ✓ |
| 3 | git-workflow-hack | resolved | resolved (all_tests_passed) | ✓ |
| 4 | build-linux-kernel-qemu | unresolved (tests failed) | unresolved (some_tests_failed) | ✓ |
| 5 | incompatible-python-fasttext.base_with_hint | resolved | resolved (all_tests_passed) | ✓ |
| 6 | polyglot-c-py | resolved | resolved (all_tests_passed) | ✓ |
| 7 | get-bitcoin-nodes | resolved | resolved (all_tests_passed) | ✓ |
| 8 | configure-git-webserver | resolved | resolved (all_tests_passed) | ✓ |
| 9 | blind-maze-explorer-algorithm.hard | resolved | resolved (all_tests_passed) | ✓ |
| 10 | new-encrypt-command | resolved | resolved (all_tests_passed) | ✓ |
| 11 | eval-mteb.hard | unresolved (build OOM/137) | unresolved (build 137) | ✓ |
| 12 | swe-bench-langcodes | resolved | resolved (all_tests_passed) | ✓ |
| 13 | fix-git | unresolved (build: load ENOSPC) | unresolved (build: clone 404, repo deleted) | ✓ |
| 14 | grid-pattern-transform | resolved | resolved (all_tests_passed) | ✓ |
| 15 | security-vulhub-minio | resolved | resolved (all_tests_passed) | ✓ |
| 16 | oom | unresolved (build: load ENOSPC) | unresolved (build: pip ENOSPC) | ✓ |
| 17 | git-multibranch | resolved | resolved (all_tests_passed) | ✓ |
| 18 | password-recovery | resolved | resolved (all_tests_passed) | ✓ |
| 19 | solana-data | unresolved (tests failed) | unresolved (some_tests_failed) | ✓ |
| 20 | qemu-alpine-ssh | unresolved (build deb 404) | unresolved (build deb 404) | ✓ |
| 21 | polyglot-rust-c | unresolved (tests failed) | unresolved (some_tests_failed) | ✓ |
| 22 | path-tracing-reverse | resolved | resolved (all_tests_passed) | ✓ |
| 23 | organization-json-generator | resolved | resolved (all_tests_passed) | ✓ |
| 24 | jupyter-notebook-server | resolved | resolved (all_tests_passed) | ✓ |
| 25 | swe-bench-astropy-2 | resolved | resolved (all_tests_passed, ansi_stripped) | ✓ |
| 26 | modernize-fortran-build | resolved | resolved (all_tests_passed) | ✓ |
| 27 | blind-maze-explorer-algorithm.easy | resolved | resolved (all_tests_passed) | ✓ |
| 28 | decommissioning-service-with-sensitive-data | resolved | resolved (all_tests_passed) | ✓ |
| 29 | hf-model-inference | unresolved (tests failed) | unresolved (some_tests_failed) | ✓ |

### Discordances (0 — none dropped)

None. 30/30 pairs agree directionally; the resolved set is identical
on both runners (22 tasks) as is the unresolved set (8 tasks).

### Verdict

**`verified`** — EXACT directional agreement at 30/30 pairs. Both
tranche-2 discordance subjects were re-drawn and now agree on their
original mechanisms: `swe-bench-astropy-2` resolves on both runners
(ANSI strip reproduces the tmux capture-pane channel) and
`incompatible-python-fasttext.base_with_hint` resolves on both
runners (sequential O-side build — no concurrency OOM). Zero
discordance at N=30 → parity admitted at half-width ≈ 0.116
(`kl_directional_bound` η = .025). **S3 met.**

Residual caveats recorded honestly: three of the eight agreed
unresolved verdicts carry environment flavor (`fix-git`, `oom`
within-batch ENOSPC on one or both sides; both also independently
build-bound — fix-git's upstream repo is deleted, oom's image is
heavy), and `eval-mteb.hard` is resource-bound on BOTH builders.
Directional agreement holds regardless; no pair's agreement depends
on retry, substitution, or hidden failure.

Artifacts: `docs/build/reports/2026-11-05-parity-tranche-3.json`
(manifest verbatim + all 30 pairs + reasons + image digests +
repair evidence); O-side
`~/tb-work/parity-runs/o-side-3/2026-11-05__23-16-40`,
`2026-11-06__{00-26-04,00-38-15,00-49-37,00-55-35}/`; H-side
`~/tb-work/parity-runs/h-side/t3/`; drivers `/tmp/parity-driver` +
`/tmp/parity-h-side.py` (ANSI-stripped, platform-pinned);
pipeline/diskwatch logs `/tmp/parity-t3-*.log`,
`/tmp/parity-{o3,h3}-b*.log`.

### Backend state left behind

- podman machine `podman` (applehv rootless) left RUNNING; SSH-tunnel
  Docker API socket `unix:///tmp/podman-api/podman.sock` still up
  (`_ping` OK).
- Post-run cleanup executed: `podman system prune -af --volumes` +
  `docker buildx prune -af` → VM disk at ~27 GiB free (3.1 GiB used);
  `podman system df`: 1 image (buildx builder ~240 MB), 1 container,
  1 volume (~39 MB).
- No hh-parity or tb task containers/networks remain.
- `~/tb-work/parity-runs/` holds all raw records (external working
  dir, not committed).

## Correction — 2026-11-07 (R6.3 · REQ-XA-114 / AR-03)

The "parity admitted → S3 met" and "**S3 met.**" phrasings above are the
tranche's own exact-agreement endpoint, not an admission outcome under
the landed machinery. `parity_admission` refuses this design verbatim —
strict records-only rung `MissingKey{pins.rights_ref}`, labelled
design-probe rung `AdapterFloor{repeats:1}` (recomputed records-in,
`docs/build/reports/2026-11-07-p1-estimators.json`). The comparison was
`tb run -a oracle` against the scratch reference runner
`parity-h-side.py` — not HarnessHarness cells — at R=1, on one arm64
applehv VM; 26/30 H runs reused the O-built image. "≈ 0.116" is the
closed-form zero-discordance endpoint 1 − .025^(1/30) = 0.11570, not an
estimator output and not a registered margin. The fold's `s3_unmet:
false` flag is the parity predicate — not the supplement's S3
(compaction study) scenario, which stays unmet.
