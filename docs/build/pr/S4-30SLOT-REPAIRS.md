# S4-30SLOT-REPAIRS — three verbatim-recorded repairs close the last 30 inconclusive slots (300/300)

Operator-authorized supplement work (verbatim "let's do these",
2026-11-06) between GATE-G19 and R5.59 — not a manifest ticket;
`nextTicket`/gate state untouched. The `s4-completion-gce` epoch left
30 frozen-cohort slots honestly inconclusive. This epoch re-executed
all 30 on the same provisioned GCE backend under the same frozen
protocol (`protocol_id 9edc5327…`, seed v3, no re-pick) — named epoch
`s4-30slot-repairs-gce`, `prior_disposition` verbatim per row — under
three verbatim-recorded repair classes. USD **$0.00** model calls;
deterministic oracle participants only; backend spend ≈ **$1.41**
billed instance-hours cumulative inside the ~$20 budget.

## Result (verbatim)

- **30/30 executed** — 20 `resolved`, 10 `unresolved`; 0 inconclusive,
  0 failed, 0 unattempted. Every row carries `repair_class` + the class
  member verbatim; sentinels/observer/seals intact on all 30 cells;
  sequential cells; 120 remote ws syncs.
- **swe-bench-astropy-2 ×10 — `machinery_repair: declared_workdir` →
  resolved ×10.** OUR driver's defect, verbatim: the cell exec'd
  oracle + verifier at `cd /app` while the task image declares
  `WORKDIR /app/astropy` — every relative path in `run-tests.sh` /
  `solution.sh` resolves only at the repo root (the parity H-side run
  resolves this task). `s4-cell` spec gains `task_workdir` read
  verbatim from `podman image inspect` `.Config.WorkingDir` of the
  built cell image; the R5.57 admitted bound retained
  (`verifier_bound_admitted: 900` per row); the verbatim channel now
  produces the full pytest receipt — same repair class as the ANSI
  fix, task files byte-untouched.
- **qemu-alpine-ssh ×10 — `env_repair: archive_mirror` → unresolved
  ×10.** Upstream drift, verbatim: EOL bullseye deb404. RECORDED
  admission: mirror-patched base
  `localhost/s4-mirror/debian-bullseye-slim` (bullseye-slim + sed →
  archive.debian.org; all default suites verified) + the client built
  from the task Dockerfile with ONLY the `FROM` line rewritten —
  verbatim rewritten Dockerfile in the build record, no task file
  changes. Client build rc 0 in 145.89 s; cells ran the oracle +
  verbatim verifier; the task's own PytestParser decided
  `test_sshpass` failed on every slot — a genuine participant outcome,
  never relabelled.
- **fix-git ×10 — `env_repair: vendored_mirror` → resolved ×10.**
  Deleted upstream (rc 128) searched honestly: no repo/fork under
  TheMikeMerrill's 11 public repos; pinned `d7d3e4b` absent from
  mldangelo/personal-site + all TheMikeMerrill repos; terminal-bench
  `main` still clones the dead URL — no task-version admission exists.
  Mirror found: Software Heritage full snapshot
  `swh:1:snp:b2cbd5476b5a00d133f224e990498de4e06bad01` (visit 8,
  2026-04-04, status full) → vault git-bare vendored verbatim at
  `~/tb-work/vendored/fix-git/personal-site.git` (all branches;
  `d7d3e4ba93…` verified in master history). Client Dockerfile
  rewritten ONLY by added `COPY vendor/` + `git config --system
  url.insteadOf` lines for the verbatim `setup.sh` clone leg (config
  unset right after) — every task file byte-identical; the same
  upstream objects materialize.

## Combined cohort (verbatim)

**300/300** slots carry real receipts — **220 resolved / 80
unresolved**; 0 inconclusive; 0 failed; first subjects **60/60**
executed (44 resolved). Denominators recomputed over all four epochs
(confirmation + egress + completion + repairs); the three prior
packets stand byte-untouched.

## Evidence

- Packet: `docs/build/reports/2026-11-06-s4-30slot-repairs.json`
  (`hh-s4-30slot-repairs/1`)
- Run ledger: `docs/build/runs/S4-30SLOT-REPAIRS.md`
- Build records verbatim (rewritten Dockerfiles, mirror provenance):
  `~/tb-work/s4-repairs/logs/*-build.json` (external, not committed)
- Drivers: `/tmp/s4-cell` (patched — `task_workdir`),
  `/tmp/s4-repairs-orch.py`, `/tmp/s4-repairs-assemble.py`; vendored
  mirror `~/tb-work/vendored/fix-git/personal-site.git`

## Verify

- `docs/build/tools/check-supplement-guard.sh` — **PASS**, route
  `HOLD — gate GATE-G19 pending — RETURN PASS` (pending gate, not a
  missing implementation)
- `scripts/deferrals-open.sh` — empty (zero open rows)
- fmt/cargo — n/a (records only; no source changes)
- VM stays RUNNING + provisioned — deprovision is R5.59's mandatory
  tracked AC, not this work's

mergePolicy: OPERATOR — not merged by the agent.
