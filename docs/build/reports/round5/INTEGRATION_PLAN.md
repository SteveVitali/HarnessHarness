# INTEGRATION_PLAN — landing the round-5 stacked chain (REC5.3)

> **Read-only.** This plan *reports* mergeability and CI state; it merges
> nothing and releases nothing — `mergePolicy: OPERATOR`. Every mutating
> step is the operator's, from §6. Dry-run evidence:
> `docs/build/reports/round5/merge-dryrun-2026-11-05.json` (read-only
> `git merge-tree` + one `ci-boundary.sh --no-wait` read per PR). CI and
> mergeability read at **2026-11-05T00:50:56Z** (`date -u`); mergeable
> state at **2026-11-05T00:51Z** (`gh pr list`).

## 0. State correction vs the ticket brief

The brief described "PR chain #256–#275 all green". **Partly stale:**
the operator kept landing — **#256–#259 are MERGED** (R5.42, R5.42a,
R5.43, R5.44; 2026-11-04T05:38Z–08:13Z), so `gh pr list --state open`
returns exactly **16 open PRs — #260 through #275** — the rows this
plan lands, plus **#276 (this PR)**, which appends at the tail the
moment it exists. Everything below reflects the live repo, verified
2026-11-05.

## 1. The PR graph

### 1a. Landed portion (for orientation)

| PRs | tickets | merged into | window |
|---|---|---|---|
| #2–#208 | rounds 1–4 chain (S0.1 … R4.18, gates, capstones) | `main` | 2026-09-22 → 2026-11-02 |
| #209–#259 | 51 PRs in range — 48 round-5 chain rows R5.1 … R5.44 + non-chain #216/#217 (dependabot) + #226 (ci allowlists) | `main` | 2026-11-02 → 2026-11-04 |

All merges are operator merge commits (`Merge pull request #N`).
`main`'s head is `1cacf699` (merge of #259 / R5.44); its own main CI
run `41797856940` is **success**.

### 1b. Open stack — 16 PRs (+ this PR), bottom to top

Declared bases still point at parent *branches* (the landed
convention: retarget each to `main` right before merging it — §6).
Mergeability = `git merge-tree` vs `origin/main`; CI = per-head
ci-boundary read at 00:50:56Z; `gh` reports every PR `MERGEABLE`/`CLEAN`.

| order | PR | ticket | head @ tip | declared base | commits vs `origin/main` | merge | CI @ 00:50:56Z |
|---|---|---|---|---|---|---|---|
| 1 | [#260](https://github.com/SteveVitali/HarnessHarness/pull/260) | R5.45 | `…-r5.45` @ `0d4325d` | r5.44 (merged) | +7 | clean | **pass** |
| 2 | [#261](https://github.com/SteveVitali/HarnessHarness/pull/261) | R5.46 | `…-r5.46` @ `ac942db` | r5.45 | +17 | clean | **pass** |
| 3 | [#262](https://github.com/SteveVitali/HarnessHarness/pull/262) | R5.47 | `…-r5.47` @ `5e6a6c9` | r5.46 | +22 | clean | **pass** |
| 4 | [#263](https://github.com/SteveVitali/HarnessHarness/pull/263) | R5.48 | `…-r5.48` @ `b1a8644` | r5.47 | +27 | clean | **pass** |
| 5 | [#264](https://github.com/SteveVitali/HarnessHarness/pull/264) | R5.49 | `…-r5.49` @ `bc18d37` | r5.48 | +36 | clean | **pass** |
| 6 | [#265](https://github.com/SteveVitali/HarnessHarness/pull/265) | R5.50 | `…-r5.50` @ `0b61d2b` | r5.49 | +40 | clean | **pass** |
| 7 | [#266](https://github.com/SteveVitali/HarnessHarness/pull/266) | R5.51 | `…-r5.51` @ `0fb2469` | r5.50 | +44 | clean | **pass** |
| 8 | [#267](https://github.com/SteveVitali/HarnessHarness/pull/267) | R5.52 | `…-r5.52` @ `972de4c` | r5.51 | +48 | clean | **pass** |
| 9 | [#268](https://github.com/SteveVitali/HarnessHarness/pull/268) | R5.53 | `…-r5.53` @ `676595f` | r5.52 | +54 | clean | **pass** |
| 10 | [#269](https://github.com/SteveVitali/HarnessHarness/pull/269) | R5.54 | `…-r5.54` @ `b73aab1` | r5.53 | +60 | clean | **pass** |
| 11 | [#270](https://github.com/SteveVitali/HarnessHarness/pull/270) | R5.55 | `…-r5.55` @ `27699a4` | r5.54 | +64 | clean | **pass** |
| 12 | [#271](https://github.com/SteveVitali/HarnessHarness/pull/271) | CAP5.1 | `…-cap5.1` @ `a5bf317` | r5.55 | +66 | clean | **pass** |
| 13 | [#272](https://github.com/SteveVitali/HarnessHarness/pull/272) | CAP5.2 | `…-cap5.2` @ `2909edc` | cap5.1 | +72 | clean | **pass** |
| 14 | [#273](https://github.com/SteveVitali/HarnessHarness/pull/273) | CAP5.3 | `…-cap5.3` @ `72f080b` | cap5.2 | +76 | clean | **pass** |
| 15 | [#274](https://github.com/SteveVitali/HarnessHarness/pull/274) | REC5.1 | `…-rec5.1` @ `169e3c2` | cap5.3 | +78 | clean | **pass** |
| 16 | [#275](https://github.com/SteveVitali/HarnessHarness/pull/275) | REC5.2 | `…-rec5.2` @ `a9a9db0` | rec5.1 | +80 | clean | **pass** |
| 17 | #276 (this PR) | REC5.3 | `…-rec5.3` | rec5.2 | +82 | — | read after push |

(branch prefix: `codex/harnessharness-evidence`.) The docs tail
(DOC5.1 → DOC5.2, manifest rows 245–246) stacks on `…-rec5.3` and
follows the same rule when their PRs appear.

**Topology note.** The stack is true linear ancestry: every open branch
descends from its parent; `merge-base(origin/main, tip)` = `a00acee3`
(the r5.44 tip — inside `main` as #259's second parent). No foreign
fix-commits this round (unlike round 4): each retargeted diff is
exactly its ticket's delta.

## 2. External state

- **`main` head:** `1cacf699` (#259 / R5.44 merge, 2026-11-04T08:13:30Z;
  main CI `41797856940` success). Earlier same-day main runs show
  `cancelled` — superseded-by-next-merge, not failures.
- **Chain still descends from the pinned base:** `pinnedBaseSha`
  `001490c1` is an ancestor of `main`; merge-base `a00acee3` is inside
  `main`. The chain carries no main merge commits (it descends from the
  *branch* tip, not the merge commit — normal stacked-PR shape).
- **PRs merged since the chain began (#209, 2026-11-02):** 51 — 48
  chain rows + 3 non-chain (#216/#217 dependabot action bumps, #226
  gitleaks allowlists; all 2026-11-02, operator/dependabot). The three
  non-chain commits are NOT inside the stack's merge-base — retargeted
  diffs stay per-ticket; dry-run §3 confirms clean.
- **Open PRs that are not chain rows:** none.
- **Remote naming:** canonical repo is
  `github.com/SteveVitali/HarnessHarness` (MetaHarness redirects).
- **Release/close conditions (the recommendation's constraints):**
  `mergePolicy: OPERATOR`; `projectStatus: IN_PROGRESS` — not DONE
  (DOC5.1/DOC5.2 and this landing remain); `benchmarkSet` unset
  (HUMAN-H8); GATE-G17 PASSED 2026-11-05T00:13:07Z with DF-R5.1-1 /
  DF-H9-1 / DF-CAP5.1-1 **carried OPEN** — landing code does not
  discharge them; BL-50…BL-53 open. Probes ≤24 h at write
  (2026-11-05T00:52:31Z): podman 6.1.0; release `v0.1.0` `draft:false`
  (5 assets, created 2026-10-27); crates.io `hharness-cli` GET 200 —
  the R4.18 public surface stands; no new release act is recommended
  here.

## 3. Merge dry-run (read-only)

Command (reproduces `reports/round5/merge-dryrun-2026-11-05.json`):

```bash
bash "$AGENT_SKILLS_ROOT/skills/reconcile-build/scripts/merge-dryrun.sh" --ci \
  --json docs/build/reports/round5/merge-dryrun-2026-11-05.json \
  origin/main \
  codex/harnessharness-evidence-r5.{45..55} \
  codex/harnessharness-evidence-cap5.{1..3} \
  codex/harnessharness-evidence-rec5.{1..2}
```

**Result: 16/16 branches merge clean onto `origin/main` — 0 conflicts,
0 missing refs; CI 16/16 pass at current heads** (all four required
checks per PR: cargo-deny, gitleaks, build·test·fmt·drift ubuntu +
macos). No pending, unknown or wrong-head read in the set.

## 4. Anomalies — none blocking

- No foreign/duplicated commits on the open tips (round 4's fix-commit
  anomaly does not recur); local tips == remote heads.
- No red/cancelled check on any open head at read time.
- #276's CI is unreadable until this PR exists — recorded pending at
  closeout via ci-boundary, never assumed.

## 5. Merge strategy

**Continue the landed convention: bottom-up merge commits into `main`,
retargeting each PR's base to `main` immediately before its merge** —
and retarget #260 first (its declared base `r5.44` is already merged).
Merge commits preserve the per-ticket closeout commits the ledgers and
`runs/*.md` cite; squash/rebase would invalidate cited SHAs. Retarget
never needs a rebase: after each parent lands, `merge-base(main,
child)` moves to the parent's tip and the child's PR diff is still
exactly its ticket's delta.
`mergePolicy: OPERATOR` — every step below is manual.

## 6. Operator procedure (copy-pasteable)

Pre-flight once:

```bash
cd <worktree>
git fetch origin
gh repo set-default SteveVitali/HarnessHarness
```

Then, **in order** — one iteration per PR (#260 … #276):

```bash
# ── per PR ──────────────────────────────────────────────────────────
PR=260                      # then 261 … 275, 276 (REC5.3)

# 1. retarget the base to main
gh pr edit $PR --base main

# 2. sanity: the diff must show only this ticket's delta
gh pr diff $PR --name-only | head -20

# 3. wait for the retargeted head's checks
gh pr checks $PR --watch    # merge only when green

# 4. merge (merge commit — the landed convention)
gh pr merge $PR --merge

# 5. verify the merge landed before continuing
gh pr view $PR --json state --jq .state   # expect MERGED
# ────────────────────────────────────────────────────────────────────
```

Per-step notes:

- **All 16 open PRs were green at their heads at read time** — no red
  step exists in this recommendation. Re-check each PR at merge time;
  a PR that went red or pending since the read waits (step 3), it is
  not merged.
- **Do not merge out of order.** If a merge fails or goes red, stop:
  the remaining PRs still stack correctly on unmerged parents.
- **Withheld, always:** the merges land code and records only. They do
  not discharge DF-R5.1-1 / DF-H9-1 / DF-CAP5.1-1 or BL-50…BL-53, do
  not release, and do not flip `projectStatus` — DOC5.1/DOC5.2 remain.
- **Faster-path alternative (not recommended):** merging only #276
  lands the whole stack in one merge but leaves #260–#275 dangling and
  breaks the one-merge-per-ticket record.

## 7. Rollback

If a post-merge check fails after PR `#k` lands:

- **Back out a contiguous tail** — revert merges `#k … #last` in
  reverse order, each:

  ```bash
  git fetch origin && git checkout main && git pull
  MERGE_SHA=$(gh pr view $PR --json mergeCommit --jq .mergeCommit.oid)
  git revert -m 1 --no-edit $MERGE_SHA
  git push origin main    # or a revert PR if main is protected
  ```

- Reverting a middle PR alone is unsupported (descendants were authored
  on its content); revert the tail.
- After a revert, re-landing that PR needs revert-of-the-revert or a
  rebase — standard Git behaviour.
- Pausing loses nothing: unmerged PRs keep their branches; resume §6.

## 8. Post-merge verification (whole-build check)

After the last chain PR lands on `main`:

```bash
git fetch origin
for b in r5.{45..55} cap5.{1..3} rec5.{1..3}; do
  git merge-base --is-ancestor origin/codex/harnessharness-evidence-$b origin/main \
    && echo "$b in main" || echo "$b MISSING"
done
gh pr list --state open          # expect: no round-5 chain rows

git checkout main && git pull
cargo test --workspace                          # expect: 0 failures
cargo fmt --all --check
bash scripts/check-drift.sh
bash "$AGENT_SKILLS_ROOT/skills/build-memory/scripts/check-build-memory.sh" .   # 0 violations
bash docs/build/tools/check-supplement-guard.sh # guard PASS
bash scripts/check-living-docs.sh
```

The per-head CI runs in §1b are the current test evidence (the four
required checks ran the suite on each head); the merged main re-runs
the same suite on the composed tree. `LEDGER.projectStatus` flips to
DONE only after DOC5.1/DOC5.2 land and the whole chain is on `main`.

## 9. Next round

- Release-notes draft: `docs/build/reports/round5/RELEASE_NOTES.md`
  (operator edits; not a published note).
- Next-round seed: `docs/build/planning/2026-11-05_decision-memo.md`.
- Carry-forward: `docs/build/BACKLOG.csv` round-5 rows **BL-50…BL-53**
  (open) + the three OPEN `DF-*` rows — none discharged by this plan;
  full-target R/P predicates remain owed to their named owners.
