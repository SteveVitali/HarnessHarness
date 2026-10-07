# INTEGRATION_PLAN — landing the stacked chain (REC.3)

> **Read-only.** This plan *reports* mergeability and CI state; it merges
> nothing. Every mutating step below is the operator's, from the
> copy-pasteable procedure in §6. Merge dry-run evidence:
> `docs/build/reports/merge-dryrun-2026-10-06.json` (written by
> `reconcile-build/scripts/merge-dryrun.sh --ci`, `git merge-tree` only —
> no branch mutated). CI/mergeability read at **2026-10-06T14:04Z**
> (`date -u`; one read per PR via `ci-boundary.sh --no-wait`).

## 0. State correction vs the ticket brief

The ticket described "49 open stacked PRs (#53–#101)". **Stale.** The
operator has been landing the chain continuously: **86 PRs are already
merged into `main`** (PRs #2–#86 plus trunk #48, all by the operator,
2026-09-22 → 2026-10-06), and `gh pr list --state open` returns exactly
**15 open PRs — #87 through #101** — which are the rows this plan lands.
Everything below reflects the live repo, verified 2026-10-06.

## 1. The PR graph

### 1a. Landed portion (merged, for orientation)

| PRs | tickets | merged into | window |
|---|---|---|---|
| #2–#47 | S0.1 … S3.3 (rows 1–46) | `main` | 2026-09-22 → 2026-09-30 |
| #48 | pre-chain trunk PR (`svitali/harnessharness`, the `buildBranchBase`) | `main` | 2026-09-22 |
| #49–#86 | S3.4a … S5.4 (rows 47–85) | `main` | 2026-09-30 → 2026-10-06 |

All merged PRs landed on `main` with **merge commits** (`Merge pull
request #N from SteveVitali/svitali/harnessharness-*`). `main`'s head is
`9e6d95e` (merge of PR #86 / S5.4). One pre-chain PR (#1, 2026-09-10)
predates the build.

### 1b. Open stack — 15 PRs, bottom to top

Current bases still point at parent *branches* (the landed convention:
retarget each to `main` right before merging it — §6). CI state from the
dry-run read; mergeability from `gh` (MERGEABLE/CLEAN unless noted).

| order | PR | ticket | head branch @ tip | declared base | commits vs `origin/main` | merge | CI @ 14:04Z |
|---|---|---|---|---|---|---|---|
| 1 | [#87](https://github.com/SteveVitali/HarnessHarness/pull/87) | S5.5 | `svitali/harnessharness-s5.5` @ `8b9df23` | s5.4 (merged) | +10 | clean | **pass** |
| 2 | [#88](https://github.com/SteveVitali/HarnessHarness/pull/88) | S5.6 | `svitali/harnessharness-s5.6` @ `98e32fc` | s5.5 | +12 | clean | **pass** |
| 3 | [#89](https://github.com/SteveVitali/HarnessHarness/pull/89) | S5.7 | `svitali/harnessharness-s5.7` @ `3419809` | s5.6 | +14 | clean | **pass** |
| 4 | [#90](https://github.com/SteveVitali/HarnessHarness/pull/90) | S5.8 | `svitali/harnessharness-s5.8` @ `61f5ca2` | s5.7 | +18 | clean | **pass** |
| 5 | [#91](https://github.com/SteveVitali/HarnessHarness/pull/91) | S6.1a | `svitali/harnessharness-s6.1a` @ `716c6d4` | s5.8 | +20 | clean | **pass** |
| 6 | [#92](https://github.com/SteveVitali/HarnessHarness/pull/92) | S6.1b | `svitali/harnessharness-s6.1b` @ `3fc6495` | s6.1a | +22 | clean | **pass** |
| 7 | [#93](https://github.com/SteveVitali/HarnessHarness/pull/93) | S6.2 | `svitali/harnessharness-s6.2` @ `4642c9e` | s6.1b | +24 | clean | **pass** |
| 8 | [#94](https://github.com/SteveVitali/HarnessHarness/pull/94) | S6.3a | `svitali/harnessharness-s6.3a` @ `f0fd7a5` | s6.2 | +26 | clean | **pass** |
| 9 | [#95](https://github.com/SteveVitali/HarnessHarness/pull/95) | S6.3b | `svitali/harnessharness-s6.3b` @ `96eb0ec` | s6.3a | +29 | clean | **pass** |
| 10 | [#96](https://github.com/SteveVitali/HarnessHarness/pull/96) | S6.4 | `svitali/harnessharness-s6.4` @ `f29f3b5` | s6.3b | +32 | clean | **pass** |
| 11 | [#97](https://github.com/SteveVitali/HarnessHarness/pull/97) | CAP.1 | `svitali/harnessharness-cap.1` @ `63f3f95` | s6.4 | +36 | clean | **pass** |
| 12 | [#98](https://github.com/SteveVitali/HarnessHarness/pull/98) | CAP.2 | `svitali/harnessharness-cap.2` @ `5668d54` | cap.1 | +38 | clean | **pass** |
| 13 | [#99](https://github.com/SteveVitali/HarnessHarness/pull/99) | CAP.3 | `svitali/harnessharness-capstone` @ `82ef6b0` | cap.2 | +45 | clean | **pass** |
| 14 | [#100](https://github.com/SteveVitali/HarnessHarness/pull/100) | REC.1 | `svitali/harnessharness-rec.1` @ `bb8b482` | capstone | +47 | clean | **UNSTABLE — one cancelled run** (not a failure; §4) |
| 15 | [#101](https://github.com/SteveVitali/HarnessHarness/pull/101) | REC.2 | `svitali/harnessharness-rec.2` @ `2593908` | rec.1 | +49 | clean | **pending** (checks in flight) |
| — | #102 (this PR) | REC.3 | `svitali/harnessharness-rec.3` | rec.2 | — | — | — |

REC.3's own PR (#102, base `svitali/harnessharness-rec.2`) appends at
the tail the moment it exists — same recipe: after #101 lands, retarget
to `main`, merge. The docs tail (DOC.1 → DOC.2, manifest rows 105–106)
stacks on `svitali/harnessharness-rec.3` and follows the same rule when
their PRs appear.

**Topology note.** The open stack is *not* naive ancestry: every open
branch shares merge-base `2c3d477` with `main` and carries the full
cumulative ticket history plus **its own copy of one CI-fix commit** at
the tip (§4). Each PR's diff vs `main` after retargeting = exactly that
ticket's delta + its fix-copy — verified clean by the dry-run.

## 2. External state

- **`main` head:** `9e6d95e` (PR #86 / S5.4 merge, 2026-10-06T12:24Z).
- **Chain still descends from the pinned base:** `pinnedBaseSha`
  `85a3960` is an ancestor of `main` (verified via
  `git merge-base --is-ancestor`). The `buildBranchBase`
  `svitali/harnessharness` landed as PR #48 (2026-09-22); its branch tip
  `4e82e27` is inside `main`.
- **PRs merged since the chain began:** 86 — all chain rows (#2–#86) +
  the trunk (#48), all merged by the operator into `main`; no non-chain
  PR has landed inside the chain's window.
- **Open PRs that are not chain rows:** none (`gh pr list --state open`
  = exactly #87–#101).
- **Remote naming:** canonical repo is
  `github.com/SteveVitali/HarnessHarness`; the configured remote URL
  `SteveVitali/MetaHarness` redirects there. Use the canonical URL in
  every command below.

## 3. Merge dry-run (read-only)

Command (reproduces `docs/build/reports/merge-dryrun-2026-10-06.json`):

```bash
bash ~/.claude/skills/reconcile-build/scripts/merge-dryrun.sh --ci \
  --json target/rec3/merge-dryrun.json \
  origin/main \
  svitali/harnessharness-s5.5 svitali/harnessharness-s5.6 \
  svitali/harnessharness-s5.7 svitali/harnessharness-s5.8 \
  svitali/harnessharness-s6.1a svitali/harnessharness-s6.1b \
  svitali/harnessharness-s6.2 svitali/harnessharness-s6.3a \
  svitali/harnessharness-s6.3b svitali/harnessharness-s6.4 \
  svitali/harnessharness-cap.1 svitali/harnessharness-cap.2 \
  svitali/harnessharness-capstone \
  svitali/harnessharness-rec.1 svitali/harnessharness-rec.2
```

**Result: 15/15 branches merge clean onto `origin/main` — 0 conflicts,
0 missing refs.** CI at read: 13 pass, #100 red (a *cancelled* workflow
run — `37473774516` "REC.1 closeout" cancelled when a newer push
superseded it — not a code failure), #101 pending (checks running at
read time).

## 4. Known anomalies the operator will see

- **The foreign CI-fix commit, ×N.** Commit `e04933d`
  (`fix(ci): seatbelt skip-guards, token-echo re-drain, and port the S2
  boundary spike`) sits inside PR #99's head on
  `svitali/harnessharness-capstone`, and **every open branch tip carries
  its own copy** of the same patch — `git patch-id` proves it:
  `51c7eea` (on `main`, via merged s5.4), `8b9df23` (s5.5), `98e32fc`
  (s5.6), `3419809` … `f29f3b5` (s6.4 tips), `63f3f95`, `5668d54`,
  `e04933d` (capstone) all share patch-id `2625dd23…`. It is the *same*
  fix propagated per branch so each PR's CI could go green
  independently. Harmless: `git merge-tree` resolves each copy against
  the already-landed one cleanly (dry-run §3 confirms). Do not "fix" the
  duplication — it self-resolves merge-by-merge.
- **#100's red check is a cancelled run, not a failure.** The pass run
  (`37473500044`, `success`) and the cancelled run (`37473774516`)
  share head `bb8b482`; GitHub surfaces cancelled as failing. Re-run or
  dismiss the cancelled check, or confirm the required-check set only
  needs the passing run.
- **#101 was pending at read time** — expected: its checks were still
  running. Re-read before merging (`gh pr checks 101`).
- **Branch tips moved after local checkout.** Local copies of the open
  branches were stale; the plan uses `origin/` tips everywhere. The
  remote is authoritative.

## 5. Merge strategy

**Continue the landed convention: bottom-up merge commits into `main`,
retargeting each PR's base to `main` immediately before its merge.**

- *Why bottom-up:* each PR's diff is exactly one ticket's delta; CI runs
  per merge; a failure isolates to one row.
- *Why merge commits:* matches all 86 landed merges (`Merge pull request
  #N`); preserves the per-ticket closeout commits the ledgers cite.
  Squash/rebase would discard the one-commit-per-phase-log-entry shape
  the build memory references and would invalidate the commit SHAs cited
  throughout `runs/*.md` / `BUILD_INDEX.md`. Rebase-merge additionally
  cannot work — the open branches deliberately do not contain their
  parent's foreign fix-commit, so a rebase chain would need surgery.
- *Why retarget first (not delete-and-autoretarget):* deleting the
  parent branch *does* auto-retarget the child on GitHub, but explicit
  `gh pr edit --base main` is deterministic, race-free, and keeps the
  merged branches on the remote for the record. Retarget never needs a
  rebase: after the parent merges, `merge-base(main, child)` equals the
  parent's pre-fix tip, so the child diff is unchanged.
- *mergePolicy:* `OPERATOR` (LEDGER) — every step below is manual.

## 6. Operator procedure (copy-pasteable)

Pre-flight once:

```bash
cd <worktree>
git fetch origin
gh repo set-default SteveVitali/HarnessHarness   # canonical name; MetaHarness redirects
```

Then, **in order** — one iteration per PR (shown for #87; repeat with
each row of §1b's `order` column):

```bash
# ── per PR ──────────────────────────────────────────────────────────
PR=87                       # then 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102(REC.3)

# 1. retarget the base to main
gh pr edit $PR --base main

# 2. sanity: the diff must show only this ticket's delta
gh pr diff $PR --name-only | head -20   # eyeball: one ticket's files

# 3. let the retargeted head's checks settle
gh pr checks $PR --watch

# 4. merge (merge commit — the landed convention)
gh pr merge $PR --merge

# 5. verify the merge landed before continuing
gh pr view $PR --json state --jq .state   # expect MERGED
# ────────────────────────────────────────────────────────────────────
```

Per-step notes:

- **#100 (step 3):** the red check is the *cancelled* run, not a
  failure — either re-run it (`gh run rerun 37473774516`) or merge with
  the passing run green. A step that merges a red PR says so: this is
  the only one, and it is red-by-cancellation only.
- **#101 (step 3):** was `pending` at read time; wait for green.
- **#102 / DOC.1 / DOC.2 PRs:** same recipe whenever they exist — each
  opens stacked on its predecessor's branch, retargets to `main`, merges.
- **Do not merge out of order.** If a merge unexpectedly fails or goes
  red, stop: the next PRs still stack correctly on their (unmerged)
  parent branches, so pausing is safe.
- **Faster-path alternative (not recommended):** merging only the tip
  PR brings the whole stack in one merge — but leaves #87–#100 dangling
  open and breaks the one-merge-per-ticket record. Use the loop.

## 7. Rollback

If a post-merge check fails after PR `#k` lands:

- **Back out a contiguous tail.** The chain is cumulative — revert the
  merges of `#k … #last` **in reverse order** (last merged first), each:

  ```bash
  git fetch origin && git checkout main && git pull
  MERGE_SHA=$(gh pr view $PR --json mergeCommit --jq .mergeCommit.oid)
  git revert -m 1 --no-edit $MERGE_SHA    # repeat per PR, newest first
  git push origin main                    # or open a revert PR if main is protected
  ```

- **Reverting a middle PR alone is not supported** — descendants were
  authored on its content; revert the tail instead.
- **Re-merge caveat:** after a merge is reverted, re-landing that PR
  needs a revert-of-the-revert (or a rebase) — standard Git behaviour.
- **Check-state rollback:** no data is lost by pausing — unmerged open
  PRs keep their branches; a halted chain resumes by continuing §6.

## 8. Post-merge verification (whole-build check)

After the last chain PR lands on `main`:

```bash
git fetch origin
# every chain branch tip is now inside main
for b in s5.5 s5.6 s5.7 s5.8 s6.1a s6.1b s6.2 s6.3a s6.3b s6.4 \
         cap.1 cap.2 capstone rec.1 rec.2 rec.3; do
  git merge-base --is-ancestor origin/svitali/harnessharness-$b origin/main \
    && echo "$b in main" || echo "$b MISSING"
done
gh pr list --state open                          # expect: no chain rows

git checkout main && git pull
cargo test --workspace                           # expect: ≥280 result blocks / ≥3028 tests / 0 failures
bash ~/.claude/skills/build-memory/scripts/check-build-memory.sh .   # expect: 0 violations
```

The suite floor is CAP.3's landed evidence: **280 result blocks / 3028
tests / 0 failures / 2 ignored** at `82ef6b0` (the REC tickets add no
code). `LEDGER.projectStatus` flips to `DONE` only after DOC.1–DOC.2
land and the chain is fully on `main`, per the manifest.

## 9. Next round

- Release-notes draft: `docs/build/reports/RELEASE_NOTES.md` (derived from
  `BUILD_INDEX.md`; a draft the operator edits, not a published note).
- Next-round seed: `docs/build/planning/2026-10-06_decision-memo.md` —
  the skeleton the next `decompose-spec mode=extend` starts from.
- Carry-forward inventory: `docs/build/BACKLOG.csv` (43 rows: 32 open,
  7 accepted, standing/register/resume packages) — the REC.3-scoped rows
  are BL-31/BL-32 (requirement family `R-2.3.1 · R-2.8.3 · R-2.8.4 ·
  R-2.9.3 · R-2.10.3 · R-2.10.5`).
