# S4-CONFIRMATION — frozen held-out confirmation execution (protocol_id 9edc5327)

Status: FROZEN before outcomes (this header block, the declared inputs,
the channel vocabulary and the stop conditions were committed to the
record before any of the 300 slots ran). Dated execution results are
appended below the frozen block.

- Date: 2026-11-05
- Harness: devin-desktop/swe-2-high/subagent
- Cost: $0 — no model calls. The subjects are the frozen native-arm
  policies (A=`evict_oldest`, B=`clear_tool_results` per §4.4); the
  deterministic participant is each task's committed oracle solution
  (`solution.sh`/`solution.yaml` — the same participant the parity
  tranche's `tb run -a oracle` / H-side driver executed).
- Unblocking fact: parity tranche 3 verified 30/30 directional
  agreement (`docs/build/reports/2026-11-05-parity-tranche-3.json`,
  verdict `verified`, S3 met) — the contract dependency that withheld
  the tranche at N=0 (`hh-confirm-withheld/1`,
  `blocked{prerequisite:parity}`) is now satisfied.
- Frozen protocol: `central-protocol/1`,
  `protocol_id = 9edc5327f89e2d66f8003bedc2530bc3e7d37326aad82cdfcab70465eda60a5f`
  — the R5.41 manifest re-materialized from `/tmp/parity-manifest-v3.json`
  (seed `parity-tranche-2026-11-05:seed:v3`, the same committed epoch).
  Never re-picked: the manifest is consumed verbatim.

## What the frozen protocol demands (declared)

- 30 held-out tasks (the frozen selection) × 2 arms × 5 interleaved
  rounds = **300 subject slots** (`counts.subjects=300`,
  `counts.first_slots=60` — the round-0 slots are the first subjects,
  the primary denominators). §9.2 worksheet `N_subject=2TR≥10T`:
  2·30·5 = 300 — exactly the ≤300 confirmation ceiling.
- Per-slot arm binding (verbatim from the frozen manifest):
  `arm:o-tb-oracle` → strategy `evict_oldest` (arm A);
  `arm:h-podman-oracle` → strategy `clear_tool_results` (arm B).
  Slot `lane_id`s are recorded verbatim; both lanes are the identical
  podman-applehv-arm64 VM (`lane_spec_ref` frozen) — the confirmation
  executes both arms on the same HH cell machinery.
- Each slot: a fresh cold-start podman cell **through the HH subject
  machinery** (`EnvDriver::provision`/`attach` → `hh-helper --backend
  podman` → `podman create --network none … <task-image> sleep
  infinity`; mediated dispatches via `Dispatcher`/`HelperExecutor`;
  the arm's real `compact()` under an eligible pressure opportunity;
  `seal_workspace`; artifact/meters collection; teardown).
- The subject's work: the task's committed oracle solution replayed
  inside the task's own environment image — `bash /oracle/solution.sh`
  for `solution.sh` tasks; the committed command stream replayed
  through one persistent interactive shell (`bash -i < /work/stream.sh`)
  for `solution.yaml` tasks (tmux keystroke semantics, the same
  reproduction the parity H-side used).
- The native receipt: the task's own verifier — `run-tests.sh`
  verbatim inside the cell (`TEST_DIR=/tests`, output captured
  verbatim to the workspace), parsed by terminal-bench's own
  `PytestParser` (resolved iff every parsed test passes).

## Declared honest bounds (recorded, never simulated)

Two machinery facts bound this tranche — both recorded verbatim per
slot, never a fabricated result, never a dropped cell:

1. **HH cells are `--network none`** (`hh-helper`'s `PodmanBackend`
   hardcodes `network: false`; `podman network connect` refuses
   none-mode cells — probed 2026-11-05). The task images/solutions/
   verifiers were authored for a networked compose environment. Where
   a solution or verifier cannot genuinely run isolated, the slot's
   disposition is `inconclusive{stage:reason}` verbatim.
2. **Derived base images.** Because cells cannot fetch at runtime,
   the orchestrator builds a per-task derived image
   (`s4-<task>:latest` = `FROM <client-image>` + the packages the
   task's own scripts install — the `pip install`/`apt-get install`
   argument sets extracted verbatim from `solution.*` and
   `run-tests.sh`, plus the verifier toolchain `pytest requests
   psutil`). Each derivation's package set is recorded verbatim in the
   slot records (`base_image_derivation`). Runtime downloads/clones/
   remote RPCs are NOT pre-staged — where a solution needs them, the
   slot is `inconclusive{work:…}`.

Verifier channels, in order, recorded per slot
(`receipt.channel` verbatim):

- `verbatim` — `bash /tests/run-tests.sh` inside the cell (succeeds
  only where the bootstrap's packages already satisfy offline).
- `vendored_offline` — the derived image's `python3 -m pytest
  <pytest_target> -rA` inside the live cell — the same test file the
  task's `run-uv-pytest.sh`/pip tail invokes, the same parser; the
  uv/pip bootstrap leg is recorded bypassed with the verbatim reason.
  swe-bench tasks: patch applied + `PYTHONPATH=<repo>` instead of
  `pip install -e .` — recorded.
- No channel available → `receipt.disposition = inconclusive{…}` —
  never a simulated verdict.

Slot dispositions (closed): `executed` (receipt produced;
`receipt.verdict` ∈ `resolved|unresolved` with the verbatim reason —
unresolved is a genuine participant outcome), `inconclusive{…}` (the
held-out task could not genuinely run under the machinery — image
unavailable / egress-bound work / verifier unavailable / multi-service
environment), `failed` (a machinery refusal/integrity event — sentinel
violation, substitute backend, seal failure).

## Stop conditions (frozen — §9.2 stop clause)

Integrity/treatment/cap/isolation/leakage/drift/ceiling failure stops
the tranche and retains partial results/costs verbatim — no
retrospective smaller manifest. Concretely: a sentinel violation, a
served-policy relabel, a slot executed outside the frozen schedule,
VM disk exhaustion that cannot be recovered by the declared pruning
order, or a per-subject envelope breach (the §9.2 pilot individual
caps: ≤100 calls, ≤1M input/≤100k output tokens, ≤30 min admitted
wall, verifier ≤15 min, ≤2 concurrent). Build/runner failures are
outcomes or `inconclusive{}` records — never exclusions, never
retried beyond the frozen `infrastructure_failure`-only bound
(≤2/plan, ≤10 % of plans).

Per-subject setup retry bound: ≤1 additional attempt on a cell
setup/provision panic only (inside the frozen ≤2/plan
infrastructure bound); a failed first attempt is retained in
`attempts[]`, never silently dropped.

## Environment (at freeze)

- podman machine `podman` (applehv, 2 CPU / 2 GiB / 30 GiB raw disk),
  rootless, RUNNING. Docker API `unix:///tmp/podman-api/podman.sock`
  via the recorded SSH-forward recipe (`_ping` OK 2026-11-05).
- Dataset `~/tb-work/dataset` — terminal-bench-core 0.1.1, 80 task
  dirs (the frozen catalogue identity's corpus).
- Drivers: `/tmp/s4-cell` (Rust slot driver — path-deps on the
  workspace's env machinery crates, the r5_46 scaffold shape) +
  `/tmp/s4-orch.py` (manifest/order/build/parse/assemble). Both are
  scratch machinery outside the repo, same class as `/tmp/parity-*`.
- Work root `~/tb-work/s4-confirm/` — stores, workspaces, per-slot
  records (external working dir, not committed).
- VM disk hygiene: check free before each task image build;
  `podman system prune -af --volumes` + `docker buildx prune -af`
  when free < 8 GiB; client+derived images pruned after the task's
  slots finish (recorded per task).

## Emit targets

- `docs/build/reports/2026-11-05-s4-confirmation.json` — the
  confirmation packet (declared inputs, freeze, per-slot records,
  denominators, costs, dispositions).
- This ledger — per-phase results appended below.

---

## Execution log (appended after freeze)

Everything below is verbatim record; the frozen block above is
unedited.

### Execution results (2026-11-05→06, verbatim)

300/300 slots executed or honestly bound — zero dropped, zero retried
beyond the frozen bound, zero fabricated. Packet:
`docs/build/reports/2026-11-05-s4-confirmation.json` (`hh-s4-confirmation/1`).

- **Dispositions:** `executed` **160/300** (130 `resolved` / 30
  `unresolved` — both are genuine participant outcomes through the
  task's own verifier); `inconclusive{…}` **140/300** (verbatim reasons
  per slot — image unavailable ×40: eval-mteb.hard OOM, fix-git
  deleted upstream repo, oom ENOSPC, qemu-alpine-ssh deb 404;
  environment/egress-bound work; verifier unavailable;
  `hf-model-inference` inconclusive{environment:cell_network_isolated}
  — the model-download solution cannot run under `--network none`);
  `failed` 0; `unattempted` 0.
- **Per-task:** 16 tasks × 10 slots executed — fully `resolved`×13
  (super-benchmark-upet, raman-fitting, hello-world,
  incompatible-python-fasttext.base_with_hint, get-bitcoin-nodes,
  blind-maze-explorer-algorithm.hard, new-encrypt-command,
  git-multibranch, path-tracing-reverse, organization-json-generator,
  jupyter-notebook-server, blind-maze-explorer-algorithm.easy,
  decommissioning-service-with-sensitive-data); `unresolved`×3
  (swe-bench-langcodes, grid-pattern-transform, solana-data — the
  verifier genuinely decided fail). 14 tasks ×10 inconclusive.
- **Denominators retained:** planned slots 300/300 recorded; first
  subjects 32/60 executed (26 resolved); every cell attempt identity
  retained (`attempts[]`, 260 cell attempts total, incl. setup retries
  inside the ≤1 bound); 970 mediated dispatches; no slot dropped.
- **§4.2 fidelity denominators (verbatim):** pressure opportunities
  120; compactions started 140; `applied` statuses 260 (no-op applies
  on below-cap transcripts recorded as applied — completions can
  exceed starts); changed_requests 140; triggered first subjects 24;
  **served_deviations 60** — on the no-pressure B-arm slots the
  compaction record's `served_variant` reported `hh/evict-oldest@1`
  rather than the clear-tool-results variant — recorded verbatim,
  never relabelled (the intended-vs-served fact is the record).
- **Costs (all-first/all-attempt, verbatim):** total slot wall
  4,969,261 ms (~83 min across 300 sequential cells; sequential ≤2
  concurrent respected); output bytes 5,452,647; tokens_freed
  698,235; USD $0.00; provider exposure 0; no model calls.
- **Stops:** none fired — no sentinel violation, no served-policy
  relabel beyond the recorded deviations, no schedule deviation, no
  unrecoverable disk exhaustion (the declared prune order held),
  individual caps held per slot.
- **Machinery state left behind:** podman VM running; images pruned
  per task; hh-env cell containers removed per slot; work root
  `~/tb-work/s4-confirm/` (stores/ws/slot records — external, not
  committed); drivers `/tmp/s4-cell` (Rust slot driver) +
  `/tmp/s4-orch.py` + `/tmp/s4-assemble.py`.
- **Verdict: `partial`** — the frozen manifest executed at 160/300
  slots with real receipts; 140 slots are honest bounds recorded
  verbatim (the HH cell's `--network none` isolation cannot genuinely
  run egress-bound solutions/verifier bootstraps, and 4 images cannot
  build). No retrospective smaller manifest; S4 recorded with all
  denominators — the executed cohort's receipts and the inconclusive
  bounds are both the record.

## Correction — 2026-11-07 (R6.3 · REQ-XA-114 / AR-03)

"the verifier genuinely decided fail" (for `swe-bench-langcodes`,
`grid-pattern-transform`, `solana-data`) overstates the evidence: every
verdict on this page was parsed by tb's `PytestParser` in scratch Python
outside the HH machinery. The records-in cross-check
(`docs/build/reports/2026-11-07-p1-surface-xcheck.json`, basis A)
agrees with tranche-3's `tb` verdicts on 14 of 16 tasks carrying a
first-attempt HH verdict and disagrees on 2 — `swe-bench-langcodes`
(HH unresolved here; the probable cause is the declared-workdir defect,
repaired only in the repairs epoch — research/24 F2) and
`grid-pattern-transform` (cause unestablished). `solana-data`'s
unresolved verdict agrees with the O side. The two discordant tasks
are therefore not "genuinely decided" — they are scratch-parser
verdicts over a defect-era driver, recorded verbatim as such.
