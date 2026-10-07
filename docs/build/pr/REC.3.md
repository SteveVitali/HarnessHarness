# REC.3 — integration plan

**Stacked on:** `svitali/harnessharness-rec.2` (PR #101) · **Do not merge — operator-controlled**

## Summary

`reconcile-build mode=integration`: the copy-pasteable plan to land the
stacked chain (`docs/build/INTEGRATION_PLAN.md`). **State correction vs
the ticket brief:** the operator has already merged 86 chain PRs into
`main` — the open stack is **15 PRs (#87–#101)**, not 49. The read-only
merge dry-run (`merge-dryrun.sh --ci`, `git merge-tree`, JSON evidence
in `docs/build/reports/`) reports **15/15 clean onto `origin/main`, 0
conflicts**; CI: 13 pass, #100 red-by-*cancellation* only (a superseded
run, not a code failure), #101 pending at read. The foreign CI-fix
commit (`e04933d` and its per-branch copies) all share one
`git patch-id` — the same patch already on `main`, self-resolving per
merge. The plan gives the operator: the PR graph with check states,
external state (main head `9e6d95e`, pinned base `85a3960` ⊂ main, no
non-chain open PRs), merge strategy (bottom-up merge commits; retarget
each base to `main` immediately before its merge — `gh pr edit --base
main`), the exact per-PR command loop, tail-revert rollback, and the
whole-build post-merge verification.

Requirement ids stamped: `R-2.3.1` · `R-2.8.3` · `R-2.8.4` · `R-2.9.3` ·
`R-2.10.3` · `R-2.10.5` (the REC.3-scoped backlog family — BL-31/BL-32,
DF-S1.18-1, DF-S2.4-2, DF-S4.2-1, DF-S4.2-2).

## Deliverables

- `docs/build/INTEGRATION_PLAN.md` — PR graph + check states, external
  state, read-only dry-run, strategy, operator procedure, rollback,
  post-merge verification.
- `docs/build/reports/merge-dryrun-2026-10-06.json` — dry-run evidence.
- `docs/build/reports/RELEASE_NOTES.md` — draft from `BUILD_INDEX.md` (the
  operator edits; not a published note).
- `docs/build/planning/2026-10-06_decision-memo.md` — next-round seed
  for `decompose-spec mode=extend`.
- `docs/build/runs/REC.3.md`, this file, BUILD_INDEX row 101, LEDGER.

## Verification

- `merge-dryrun.sh --ci` → exit 0: **15 clean / 0 conflicts** onto
  `origin/main`; 1 red PR (#100, cancelled run only).
- `check-build-memory.sh` → 0 violations / 7 pre-existing warnings.
- `cargo test --workspace` not run — reconciliation ticket, no code.
- All writes via shell/python; `git status`/`git show --stat` verified.

## For the operator

Land the tail: `gh pr edit <PR> --base main` → `gh pr checks --watch` →
`gh pr merge --merge`, bottom-up #87 → #101 → this PR → the DOC.1/DOC.2
PRs when they exist. Rollback = tail reverts (`git revert -m 1`,
newest first). Full commands + checks: `INTEGRATION_PLAN.md` §6–§8.
