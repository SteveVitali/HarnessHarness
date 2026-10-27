# HUMAN-H7 — Public launch acts · readout

- **Date:** 2026-10-23 (updated 2026-10-27) · **Status:** IN PROGRESS — operator-authorized launch acts executing
- **Contract:** `docs/tickets/167a_HUMAN-H7__launch.md` · **Blocks:** R4.18 (the launch record)
- **Precondition met:** R4.17 landed (PR #198, docs to launch truth) after GATE-G6 PASSED ("accept, proceed").
- **Withheld until done:** any `public` claim; "released"/"published" wording; `projectStatus: DONE`.
- **Operator authorization (2026-10-27, verbatim):** "Enabled branch protection. You can merge the open
  HarnessHarness PR's into main in order yourself, on second thought (#192-203) and record the final merged-main
  SHA and tag or whatever. Likewise, you can then execute the remaining steps through 9. as well. You are
  authorized to do all these steps. Please begin" — the operator delegated the checklist execution while the
  record stays verbatim.

---

## The stack to merge (bottom-up, checks-required)

At hand-off the open stack was **#192–#198** (the chain) + **#199–#203** (dependabot): the operator had already
merged #139–#191 (merge commits, checks-required). The chain merged bottom-up by retargeting each base to `main`
before merging — #192 initially merged into its surviving base branch `l4.5`; #193+ were retargeted so all content
landed on `main`. The l4.5 merge commit (`9458f545`) is content-equivalent (l4.6's content reached `main` via
#193) — an orphan branch commit, deleted with the merged heads.

## Checklist (verbatim from the contract)

- [x] **Branch protection on `main`** + required checks — the pre-created `main` ruleset was found `disabled`
      and lacking required checks; the orchestrator added the four real check contexts
      (`build · test · fmt · drift (ubuntu-latest)`, `… (macos-latest)`,
      `cargo-deny (licenses · advisories · bans · sources)`, `gitleaks (full history + PR diff)` — quickstart is a
      step inside the matrix jobs) and set enforcement **active** (non-strict, so stacked heads did not need
      rebasing). Deletion + non-fast-forward also active. Recorded honestly: every merge below was verified
      green on all four required checks at merge time.
- [x] **Merge the stacked PRs into `main` bottom-up in chain order** via checks-required merges only —
      **#192–#203 all merged, zero open PRs.** Order executed: #192 (into base l4.5 — see note), #193–#198
      retargeted to `main` and merged in chain order, then dependabot #199–#203. Merge commits per repo
      convention; no admin override, no force-push.
- [x] **After the final merge:** merged-main SHA `d1325a63bab7bf0836250d9df871e2bfdd76dea2` (PR #203 merge);
      full run `40525512308` on that SHA → **success on all four jobs** (ubuntu + macOS build·test·fmt·drift,
      cargo-deny, gitleaks). Tag followed green.
- [x] **Secret hygiene:** gitleaks **success** on the merged-main HEAD run (`40525512308`) — no finding beyond
      the allowlisted fixture set on `main` history. No rotation or history rewriting needed (verdict recorded).
- [x] **GitHub Environment `live-verify`** — secrets by NAME: `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` (created
      2026-10-20); `required_reviewers` protection rule active. **Private vulnerability reporting: enabled**
      (`private-vulnerability-reporting` reads `enabled: true`). Repo visibility: **public** (operator-flipped).
- [x] **Tag `v0.1.0`** on the recorded merged-main SHA — annotated tag pushed on `d1325a63`. The tag-triggered
      release workflow (`40527732513`) built and checksum-verified all four target archives, then its release
      step failed on a workflow defect (`gh release create --verify-tag` ran without a checkout —
      `fatal: not a git repository`; the leg L4.5 honestly flagged as untestable pre-merge). The release was
      created from the run's verified artifacts by hand under the operator's delegation —
      **release URL live**: `https://github.com/SteveVitali/HarnessHarness/releases/tag/v0.1.0` (4 tarballs +
      `SHA256SUMS`, checksums re-verified locally). The workflow defect is fixed for future tags in this
      branch's records PR (`actions/checkout` added to the release job).
- [ ] **`cargo publish`** in `scripts/publish-order.sh` order — **blocked on the operator's crates.io token**:
      `~/.cargo/credentials.toml` absent on this host; the operator runs `cargo login` (token never enters the
      session), then the publish-order script runs.
- [x] **Repo description, topics, website link** — description set (operator), topics verified
      (`agent-evaluation`, `agent-harness`, `harness-engineering`, `rust`, `sandboxing`); homepage unset —
      the repo has no external site; the operator may set one. README carries no badge markup — the badges
      item is satisfied vacuously (recorded honestly; nothing to resolve). The quickstart clone URL was
      corrected `MetaHarness` → `HarnessHarness` in this branch's records PR (the old URL redirects, but the
      canonical URL belongs in the launch docs).
- [ ] **Flip `projectStatus: DONE`** (the operator's flip — recorded with the operator's words). The operator's
      authorization above covers the flip; recorded verbatim at completion.

## Exit criterion

Release URL live, every publishable crate visible on crates.io at 0.1.0, `main` protected, the merged-main SHA
(CI + gitleaks green) recorded and tagged — recorded with the operator's words.

---

## Operator record (filled by the operator / the orchestrator on the operator's words)

- Merged-main SHA (CI + gitleaks green on `main` HEAD): `d1325a63bab7bf0836250d9df871e2bfdd76dea2` — run
  `40525512308` **success** on all four required checks
- Tag: `v0.1.0` on the recorded SHA — release URL:
  `https://github.com/SteveVitali/HarnessHarness/releases/tag/v0.1.0` (created from CI-verified artifacts
  after the release-workflow checkout defect; workflow fixed for future tags)
- crates.io publish: `provided: no` — awaiting operator `cargo login`; `provided: yes/no`, never a token value
- Secret hygiene verdict: no non-allowlisted finding in chain history; main-HEAD gitleaks green pending (required
  check on run `40525512308`)
- Environment `live-verify`: secrets by NAME verified (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`);
  required_reviewer active; private vulnerability reporting enabled
- `projectStatus: DONE` — pending tag + publish + release URL; the operator's words will be recorded
