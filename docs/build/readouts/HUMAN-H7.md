# HUMAN-H7 — Public launch acts · readout

- **Date:** 2026-10-23 · **Status:** PENDING the operator
- **Contract:** `docs/tickets/167a_HUMAN-H7__launch.md` · **Blocks:** R4.18 (the launch record)
- **Precondition met:** R4.17 landed (PR #198, docs to launch truth) after GATE-G6 PASSED ("accept, proceed").
- **Withheld until done:** any `public` claim; "released"/"published" wording; `projectStatus: DONE`.
- **Chain tip at this read:** `svitali/harnessharness-r4.17` @ `f061d72` · the stacked PR chain #139–#198 open —
  mergePolicy OPERATOR; only the operator merges.

---

## The stack to merge (bottom-up, checks-required)

The chain is ~64 stacked PRs. The merge order is the chain order — each PR's base is the PR below it, so GitHub
re-bases each onto the newly-merged tip as the stack collapses bottom-up. **Each PR merges only with its required
checks green — no admin override, no force-push.** (Round-3 tail + Round 4; the oldest is the earliest open PR.)

Recent tip PRs (newest last): …#191 L4.5 · #192 L4.6 · #193 L4.6b · #194 L4.7 · #195 L4.6a · #196 R4.16 ·
#197 R4.16a · #198 R4.17.

## Checklist (verbatim from the contract)

- [ ] **Branch protection on `main`** + required checks (the CI matrix, cargo-deny, gitleaks, quickstart) — set first,
      so every merge below is checks-gated.
- [ ] **Merge the stacked PRs into `main` bottom-up in chain order** via checks-required merges only.
- [ ] **After the final merge:** wait for the full CI matrix + gitleaks on `main` HEAD to finish **green**; record the
      merged-main SHA verbatim below — only then tag.
- [ ] **Secret hygiene:** if any gitleaks finding exists on `main` or chain history beyond the allowlisted fixture
      set (`.gitleaks.toml`): rotate the credential first; then decide — recorded verbatim — on history rewriting
      before tagging.
- [ ] **GitHub Environment `live-verify`** — secrets by NAME + required reviewer (if not done at HUMAN-H6); private
      vulnerability reporting enabled.
- [ ] **Tag `v0.1.0`** on the recorded merged-main SHA → the release workflow publishes binaries + `SHA256SUMS`.
- [ ] **`cargo publish`** in `scripts/publish-order.sh` order with the operator's crates.io token.
- [ ] **Repo description, topics, website link**; README badges resolve.
- [ ] **Flip `projectStatus: DONE`** (the operator's flip — recorded with the operator's words).

## Exit criterion

Release URL live, every publishable crate visible on crates.io at 0.1.0, `main` protected, the merged-main SHA
(CI + gitleaks green) recorded and tagged — recorded with the operator's words.

---

## Operator record (filled by the operator / the orchestrator on the operator's words)

- Merged-main SHA (CI + gitleaks green on `main` HEAD): `<to record>`
- Tag: `v0.1.0` on the recorded SHA — release URL: `<to record>`
- crates.io publish: `<to record>` — `provided: yes/no`, never a token value
- Secret hygiene verdict: `<to record>`
- Environment `live-verify`: `<to record>`
- `projectStatus: DONE` — `<operator's words>`
