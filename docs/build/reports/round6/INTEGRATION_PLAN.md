# INTEGRATION_PLAN — landing the round-6 stacked chain (CAP6.3 / REC.3)

> **Read-only.** This plan *reports* mergeability and CI state; it merges
> nothing and releases nothing — `mergePolicy: OPERATOR` (XO-04). Every
> mutating step is the operator's, from §6. Dry-run evidence:
> `docs/build/reports/round6/merge-dryrun-2026-11-19.json` (read-only
> `git merge-tree` + one `ci-boundary.sh --no-wait` read per PR, run
> 2026-11-19T06:36:27Z); the stack CI read:
> `docs/build/reports/round6/ci-stack-2026-11-19.json` (read
> 2026-11-19T06:33:05Z); out-of-band reads `ci-410-2026-11-19.json` /
> `ci-413-2026-11-19.json` (2026-11-19T06:34–06:35Z). Mergeable state
> (`gh pr list`) read at **2026-11-19T06:36Z**.

## 0. State correction vs the ticket brief

The ticket describes "the stacked PR chain `#407 → #418`" plus the
out-of-band `#413`. **Confirmed against the live repo, with two
corrections recorded:** (a) `#410` (`df-r6.23-1-smokes`) is a *sibling*
leaf off `human-h15`, not on the main spine — the spine runs
#407→#408→#409→**#411**→#412→#414→…→#418; (b) below `r6.48` the chain
is **forked ancestry, not linear ancestry** — each branch forked from
its parent's *in-flight working tip* and the parent then gained more
commits (§4). Every open branch does contain current `main` head
`3c254707` as an ancestor. 12 open chain PRs + this PR.

## 1. The PR graph

### 1a. Landed portion (for orientation)

| PRs | tickets | merged into | window |
|---|---|---|---|
| #2–#391 | rounds 1–4 + round-6 chain through R6.45h (seq 253–309h) | `main` | 2026-09-22 → 2026-11-16 |
| #392–#406 | R6.45i…R6.45n + GATE-G28i/j/k/l + H-allow insert + #406 test-race fix (15 PRs) | `main` | 2026-11-16 → 2026-11-17 |

All merges are operator merge commits. `main`'s head is
`3c254707c3857fcc75591f8f78ef5f3aa9c1261f` (merge of #406,
2026-11-17T00:29:54Z).

### 1b. Open stack — 12 PRs (+ this PR), bottom to top

Declared bases still point at parent *branches* (the landed convention:
retarget each to `main` right before merging it — §6). Mergeability =
`git merge-tree` vs `origin/main` (dry-run JSON above); CI = per-head
ci-boundary reads at 06:33–06:36Z. `gh` reports every open PR
`MERGEABLE` — mergeability and CI success are distinct axes; §1c
carries the red diagnosis.

| order | PR | ticket | head @ tip | declared base | commits vs `origin/main` | merge | CI @ 06:33–06:36Z |
|---|---|---|---|---|---|---|---|
| 1 | [#407](https://github.com/SteveVitali/HarnessHarness/pull/407) | model-port-guard (exec-drain/model-port machinery line) | `0d4e85b6` | `main` | +89 | clean | **pass** (4/4) |
| 2 | [#408](https://github.com/SteveVitali/HarnessHarness/pull/408) | R6.46 — FP-E quote + K7 L2 wiring | `662dab5e` | model-port-guard | +94 | clean | **pass** |
| 3 | [#409](https://github.com/SteveVitali/HarnessHarness/pull/409) | HUMAN-H15 — credit NOT_NEEDED | `9ece22d4` | r6.46 | +95 | clean | **pass** |
| 4a | [#410](https://github.com/SteveVitali/HarnessHarness/pull/410) | DF-R6.23-1 smokes (sibling leaf) | `11a65242` | human-h15 | +97 | clean | **pass** |
| 4 | [#411](https://github.com/SteveVitali/HarnessHarness/pull/411) | GATE-G30 — `re-scope{end-first-pass-early}` | `9f27c49b` | human-h15 | +94 | clean | **pass** |
| 5a | [#413](https://github.com/SteveVitali/HarnessHarness/pull/413) | R6.51 — verify-host derivation (out-of-band leaf) | `ae722425` | **gate-g30** | +94 | clean | **pass** |
| 5 | [#412](https://github.com/SteveVitali/HarnessHarness/pull/412) | R6.47 — W4 `unused{scope_end}` close | `7dab4e25` | gate-g30 | +101 | clean | **pass** |
| 6 | [#414](https://github.com/SteveVitali/HarnessHarness/pull/414) | R6.48 — FP-RESULTS seal-open + packets | `7363013a` | r6.47 | +96 | clean | **FAIL** — run 44449044249 |
| 7 | [#415](https://github.com/SteveVitali/HarnessHarness/pull/415) | R6.49 — flagged-review freeze + phase-3/4 | `be6315b2` | r6.48 | +103 | clean | **FAIL** — run 44454413682 |
| 8 | [#416](https://github.com/SteveVitali/HarnessHarness/pull/416) | R6.50 — first-pass publication records (+H16/H17 bases) | `31fc815b` | r6.49 | +110 | clean | **FAIL** — run 44458421386 |
| 9 | [#417](https://github.com/SteveVitali/HarnessHarness/pull/417) | CAP6.1 — gap analysis + coverage matrix | `50e93509` | r6.50 | +115 | clean | **FAIL** — run 44461964712 |
| 10 | [#418](https://github.com/SteveVitali/HarnessHarness/pull/418) | CAP6.2 — round closure | `ada3f40f` | cap6.1 | +120 | clean | **FAIL** — run 44470135241 |
| 11 | #— (this PR) | CAP6.3 | `codex/harnessharness-cap6.3` | cap6.2 | +~124 | — | read after push |

### 1c. Red-state diagnosis — `check-ci-gating` readers scan (not mergeable ambiguity)

The five FAIL PRs share **one** failing leg: `build · test · fmt ·
drift` on ubuntu + macos; cargo-deny and gitleaks pass everywhere.
First failing step in run 44449044249 (#414's head): the
`test-check-ci-gating` leg → `check-ci-gating` **readers scan**
(ADR-0362 fast-path guard — `git grep` for `docs/`·`research/` literals
under `crates/`) rejects the tree:

- **57 un-allowlisted hits at the cap6.2 tip** (verified locally at
  `ada3f40f`, `check-ci-gating.sh --readers`):
  `src/bin/fp_results.rs` 21 · `tests/r6_48.rs` 13 · `tests/r6_49.rs`
  10 · `tests/r6_46.rs` 6 · `src/fp_results.rs` 4 ·
  `src/bin/fp_e_emit.rs` 3 — all literal `docs/build/**` record paths.
- **+3 stale allowlist entries** (`scripts/spike-s2-boundary.sh` ×2,
  `scripts/check-docsrs.sh` — their literal lines drifted).
- First PR red: **#414** — R6.48 introduced `fp_results` reading
  committed `docs/build/**` records (readouts/run/report JSONs) as
  freeze-packet inputs; 31 hits at its head. #415 added `r6_49.rs`
  hits; CAP6.2's `e4bf06f2` added the `fp_e_emit.rs`/`r6_46.rs` hits →
  57 at #417/#418's heads.
- Root cause is a **design/policy question, not a test flake**: the
  round-6 records-in design has emitters reading committed
  build-memory records — the same class already allowlisted 25× for
  `fp_select.rs`, `r6_17b.rs`, `r6_19.rs`, `r6_27.rs`, etc. The
  checker's own hint: *"never allowlist a real read … or STOP and
  ask"* → this is a maintainer/operator call (see I-3, §4).

## 2. External state

- **`main` head:** `3c254707c3857fcc75591f8f78ef5f3aa9c1261f` (#406
  merge, 2026-11-17T00:29:54Z). `pages.yml` last succeeded
  2026-11-17T00:29:57Z on main (run 43988096065).
- **Chain descends from the pinned base:** `pinnedBaseSha` `001490c1`
  is an ancestor of `main`; every open branch's merge-base with `main`
  is `3c254707` — the whole stack sits on *current* main (no
  main-merge commits in the branches).
- **PRs merged since the chain began (#392, 2026-11-16):** 15 — all
  round-6 chain rows (#392–#406: R6.45i…R6.45n, GATE-G28i…G28l,
  HUMAN-allow insert, exec-drain + test-race fixes). No non-chain PRs.
- **Open PRs that are not chain rows:** none (`gh pr list --state
  open`, 2026-11-19T06:36Z — 12 rows, all listed in §1b).
- **Remote naming:** canonical repo is
  `github.com/SteveVitali/HarnessHarness` (MetaHarness redirects).
- **Release/close conditions:** `mergePolicy: OPERATOR`;
  `projectStatus: IN_PROGRESS` — GATE-G32 acceptance and DOC6.1 remain;
  `benchmarkSet` unset; GATE-G31 PASSED with the three-part signature
  list carried (§7); dataset flipped public at `a11591e9` (HUMAN-H17,
  recorded); GCP compute billing `unavailable{no_billing_export}`;
  operator-disk retention **unrecorded** — carried, not inferred.
- **Pages deploys pending (R6.50):** GH Pages serves `main` via
  workflow (`html_url https://stevevitali.github.io/HarnessHarness/`,
  public, read 2026-11-19T06:40Z). The phase-3/4 + flip
  `hh-site-publication/1` records are committed on the stack; the
  deploy of that content fires only on merge to main → post-merge
  verification reads `gh api repos/…/pages/builds/latest` + the site
  read-back legs (the DF-R6.31-1 pattern). R6.31's phase-1 read-back
  is already discharged.

## 3. Merge dry-run (read-only)

Command (reproduces `reports/round6/merge-dryrun-2026-11-19.json`):

```bash
bash "$AGENT_SKILLS_ROOT/skills/reconcile-build/scripts/merge-dryrun.sh" --ci \
  --json docs/build/reports/round6/merge-dryrun-2026-11-19.json \
  main \
  codex/harnessharness-{model-port-guard,r6.46,human-h15,gate-g30,r6.51,r6.47,r6.48,r6.49,r6.50,cap6.1,cap6.2,cap6.3,df-r6.23-1-smokes}
```

**Result: 13/13 branches merge clean onto `main` — 0 conflicts, 0
missing refs.** CI attached: 7 pass, **5 FAIL (#414–#418, §1c)**, 1
none (cap6.3 — no PR yet). Conflicts none; the red rows' merges are
topologically clean — the blocker is the CI/policy item, not the
merge.

## 4. Anomalies

- **I-1 — forked ancestry below `r6.48`.** Only the top span
  (r6.48→r6.49→r6.50→cap6.1→cap6.2→cap6.3) is true linear ancestry.
  Below it, each branch forked from its parent's in-flight tip:
  merge-bases `model-port-guard↔r6.46` = `9977e1c5`,
  `r6.46↔human-h15` = `4efb8c58`, `human-h15↔{df-smokes,gate-g30}` =
  `56479176`, `gate-g30↔{r6.51,r6.47}` = `38693dc6`,
  `r6.47↔r6.48` = `e68ddaee`. Consequence: each child is missing some
  of its parent's *latest* commits — harmless to the bottom-up merge
  plan (those commits enter via the parent's own merge), but it means
  a child's declared-base diff can contain commits that also exist at
  the parent's tip; GitHub's three-way diff already hides them. No
  action required; recorded so a reviewer doesn't misread the PR
  diffs.
- **I-2 — out-of-band #413.** `r6.51` is declared on `gate-g30` and is
  a leaf. It must merge **after #411** (retarget to `main` once
  gate-g30 lands) or be rebased; merging it into `gate-g30` first
  would inject R6.51's delta into #411's diff. Order shown in §6.
- **I-3 — the red span #414–#418 (the only blocking item).** §1c. The
  repair is a maintainer decision: (a) extend
  `scripts/ci/docs-readers.allow` with the 57 literal lines + refresh
  the 3 stale entries — the established mechanism, matching the 25
  existing per-line entries incl. the same emitter class; the checker's
  hint reserves this for a maintainer call, or (b) an ADR-0362-family
  amendment formalizing build-memory record readers, or (c) move the
  inputs — rejected, the records-in design puts them in `docs/build/`
  by contract. CAP6.3 deliberately does not ship the repair: it is a
  product-surface/policy change outside this ticket's scope.
- **I-4 — #410 sibling ordering.** `df-r6.23-1-smokes` is a leaf off
  human-h15 with no descendants; it can merge any time after #409.
  Keeping it on the spine order (after #409, before #411) preserves
  one-merge-per-PR bookkeeping.

## 5. Merge strategy

**Continue the landed convention: bottom-up merge commits into `main`,
retargeting each PR's base to `main` immediately before its merge** —
with the red span held until its repair decision lands (§6 phasing).
Merge commits preserve the per-ticket closeout commits the ledgers and
`runs/*.md` cite; squash/rebase would invalidate cited SHAs (the
round-5 convention). Retarget never needs a rebase: after each parent
lands, `merge-base(main, child)` moves to the parent's tip and the
child's PR diff is still exactly its ticket's delta (I-1's forked
ancestry does not change this — the child's *own* commits are what
retargeting exposes, and §3 shows every merge is clean).
`mergePolicy: OPERATOR` — every step below is manual.

## 6. Operator procedure (copy-pasteable)

Pre-flight once:

```bash
cd <worktree>
git fetch origin
gh repo set-default SteveVitali/HarnessHarness
```

### Phase A — the green span (merges #407–#413 in this exact order)

```bash
# ── per PR, in this order: 407 → 408 → 409 → 410 → 411 → 413 → 412 ──
# (#410 and #413 are leaf siblings: 410 after its base 409;
#  413 strictly AFTER 411 — retarget to main first, §4 I-2)
PR=407

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
```

Then **#413**: `gh pr edit 413 --base main` → checks → merge.
Then **#412** the same way. Phase-A order:
`407, 408, 409, 410, 411, 413, 412`.

### Phase B — the red span (#414–#418): hold, decide, repair, then merge

**Do not merge #414–#418 as-is.** Their heads are red on the
readers-scan leg (§1c); merging them puts the red on `main`. Sequence:

1. **Decide the repair (I-3):** allowlist entries (per-line, with
   `# reason`) + stale-entry refresh — or the ADR-0362-family
   amendment. This decision is the maintainer's.
2. **Apply the repair to each red head.** Because the scan runs at
   each PR's head, the fix must be present on each of
   `r6.48, r6.49, r6.50, cap6.1, cap6.2, cap6.3`:

   ```bash
   # one repair commit, then propagate down the stack:
   git checkout codex/harnessharness-r6.48 && <apply allowlist fix> && git commit -m "…"
   git checkout codex/harnessharness-r6.49  && git cherry-pick <fix-sha> && git push
   # … repeat r6.50, cap6.1, cap6.2, cap6.3 …
   ```

   (or rebase each upper branch onto the fixed parent — operator's
   call; cherry-pick preserves cited SHAs except the new fix commit.)
3. **Merge the repaired span in order:** `414, 415, 416, 417, 418,
   <CAP6.3 PR>` — same per-PR loop as Phase A.

### Alternative (recorded, not recommended)

- **Merge-red:** the operator may merge #414–#418 while red — recorded
  here as a red merge, never green; `main`'s CI then fails the same
  leg until the repair lands. Not recommended.
- **Top-only merge:** merging only #418 lands the whole stack but
  leaves #407–#417 dangling and breaks one-merge-per-ticket records.

## 7. GATE-G32 signature list — staged verbatim (the decision packet)

Staged for the operator's signature at GATE-G32, verbatim from
CAP6.2's closure (`CAPSTONE_CLOSURE.md`, three-part list) with the
CAP6.3 reads attached:

1. **Accepted deviations** (each with ADR/control): W1 upload as W2's
   first step + the reconcile placement (§10.5 A-rows/ADR-0444
   lineage); prereg `admission_sha256` drift incl. `fp-seal-open`
   (CAP6.2's dated amendment); skeleton→ticket fleshing kind
   normalization; the exec-drain/model-port machinery line
   (PRs #404/#407); **+CAP6.3:** the two owed §10.5 ADRs AR-08/AR-10
   (R6A-2 — mint-or-waive is a G32 item).
2. **Scoped-out owed legs with owner/trigger:** the 34 routed ids
   (`LATER_PASS_ROUTING.md`) — owner = the named later pass, trigger
   per row; **REQ-XC-325 R-replay executor — `unexecuted{no_executor}`
   — waiver request** (`docs/build/reports/2026-11-19-req-xc-325-unexecuted.md`,
   never satisfied); **operator-disk retention decision — `unrecorded`,
   the operator's call**; **stack-CI red legs at base — inherited,
   diagnosed in §1c, repair decision owed**.
3. **Waivers requested** (operator-quoting records/ADRs):
   **REQ-XC-325 unexecuted waiver request**; **the Ws1 compute
   estimate-over-ceiling — estimate `5,250,954 µ$` vs declared ceiling
   `4,136,280 µ$` (≈$5.25 vs ≈$4.14, ≈127%), recorded verbatim at
   R6.45q closeout — driven by crash/recovery + ~24 h wall time**;
   **the two-fold compute-estimate variance** — the interim fold
   `30.024544` vs the authoritative 21-record fold `27.367214`
   (membership scoping: smoke/ws1r rows), both retained verbatim,
   `both folds sit ≤ $30` is **not** claimed.
4. **PR merge ordering (CAP6.3's contribution to the packet):**
   bottom-up `407→408→409→410→411→413→412`, hold the red span
   `414–418`(+CAP6.3) pending the I-3 repair decision; #413 strictly
   after its `gate-g30` base lands (or rebased) — out-of-band, never
   silently re-ordered.

## 8. Rollback

If a post-merge check fails after PR `#k` lands:

- **Back out a contiguous tail** — revert merges `#k … #last` in
  reverse order, each:

  ```bash
  git fetch origin && git checkout main && git pull
  MERGE_SHA=$(gh pr view $PR --json mergeCommit --jq .mergeCommit.oid)
  git revert -m 1 --no-edit $MERGE_SHA
  git push origin main    # or a revert PR if main is protected
  ```

- Reverting a middle PR alone is unsupported (descendants were
  authored on its content); revert the tail.
- If Phase A merged and the red-span repair is later ruled
  differently, no rollback is needed: the green span stands; the
  unmerged red span keeps its branches.
- Pausing loses nothing: unmerged PRs keep their branches; resume §6.

## 9. Post-merge verification (whole-build check)

After the last chain PR lands on `main`:

```bash
git fetch origin
for b in model-port-guard r6.46 human-h15 df-r6.23-1-smokes gate-g30 \
         r6.51 r6.47 r6.48 r6.49 r6.50 cap6.1 cap6.2 cap6.3; do
  git merge-base --is-ancestor origin/codex/harnessharness-$b origin/main \
    && echo "$b in main" || echo "$b MISSING"
done
gh pr list --state open          # expect: no round-6 chain rows

git checkout main && git pull
cargo test --workspace                          # expect: 0 failures
cargo fmt --all --check
bash scripts/check-drift.sh
bash scripts/ci/check-ci-gating.sh --readers    # expect: clean (the I-3 repair)
bash "$AGENT_SKILLS_ROOT/skills/build-memory/scripts/check-build-memory.sh" .
bash docs/build/tools/check-supplement-guard.sh # guard PASS
bash scripts/check-living-docs.sh
bash scripts/check-backlog.sh
bash scripts/gen-revisit-triggers.sh --check

# Pages deploy reads (R6.50 pending leg):
gh api repos/SteveVitali/HarnessHarness/pages/builds/latest --jq '{status,commit}'
# then the site read-back legs for /phase-3/ + /phase-4/ + the flip record set
```

`LEDGER.projectStatus` flips only after GATE-G32 signs and DOC6.1
lands and the whole chain is on `main` — never before.

## 10. Next round

- Release-notes draft: `docs/build/reports/round6/RELEASE_NOTES.md`
  (operator edits; not a published note).
- Next-round seed: `docs/build/planning/2026-11-19_round7-decision-memo.md`
  (seeded from CAP6.2's routing register — no manifest row created).
- Carry-forward: the 34 routed ids (owner = later pass, trigger per
  row) + REQ-XC-325 waiver + retention decision + the §0.3-inherited
  OPEN `DF-*` lead-token seams + BL rows — none discharged by this plan.
