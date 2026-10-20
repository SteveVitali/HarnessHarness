<!-- Round-4 launch re-charter insert (decompose-spec mode=extend amendment, 2026-10-20). A HUMAN marker
     (BM-TICKET-05): operator work, NOT an implement-spec input; no run line. -->
# HUMAN-H7 — Public launch acts (merge, protect, tag, release, publish)

- **Sequence:** 167a (round-4 row 49 of 50) · **Kind:** human · **Phase:** Round 4
- **Blocks:** R4.18 (the launch record)
- **Owner:** the operator   **Scheduled:** trigger = R4.17 landed after GATE-G6 PASSED
- **Withheld until done:** any `public` claim; "released"/"published" wording; `projectStatus: DONE`
- **Deferrals so far:** 0
- **Readout:** `docs/build/readouts/HUMAN-H7.md`

> **Operator work — NOT an `implement-spec` input.** `mergePolicy: OPERATOR` — only the operator merges, tags, publishes.
> The crates.io token is the operator's and never enters any ledger (`provided: yes/no`).

> Amended 2026-10-20 (b) (independent critic fixes, before execution): MAJOR-4 — checks-required bottom-up merges, then
> the full CI matrix + gitleaks green on `main` HEAD and the merged-main SHA recorded before tagging; MAJOR-2 — rotate-first
> on any gitleaks finding, history-rewrite decision recorded verbatim. See
> `docs/build/planning/2026-10-20_round4-decomposition.md` §Amendment 2026-10-20 (b).

## Checklist
- [ ] Branch protection on `main` + required checks (the CI matrix, cargo-deny, gitleaks, quickstart) — set first, so
      every merge below is checks-gated
- [ ] Merge the stacked PRs into `main` bottom-up in chain order (#139–#147, then the round-4 PRs) via checks-required
      merges only (each PR merges with its required checks green; no admin override, no force-push)
- [ ] After the final merge: wait for the full CI matrix + gitleaks on `main` HEAD to finish **green**; record the
      merged-main SHA verbatim in the readout — only then tag
- [ ] Secret hygiene: if any gitleaks finding exists on `main` or chain history (beyond the allowlisted fixture set in
      `.gitleaks.toml`): rotate the credential first; then decide — recorded verbatim — on history rewriting before tagging
- [ ] GitHub Environment `live-verify` — secrets by NAME + required reviewer (if not done at HUMAN-H6); private vulnerability reporting enabled
- [ ] Tag `v0.1.0` on the recorded merged-main SHA → the release workflow publishes binaries + `SHA256SUMS`
- [ ] `cargo publish` in `scripts/publish-order.sh` order with the operator's crates.io token
- [ ] Repo description, topics, website link; README badges resolve
- [ ] Flip `projectStatus: DONE` (the operator's flip — recorded with the operator's words)

## Exit criterion
Release URL live, every publishable crate visible on crates.io at 0.1.0, `main` protected, the merged-main SHA (CI +
gitleaks green) recorded and tagged — recorded with the operator's words.

## DEFERRALS rule for tickets that run before this completes
R4.18 runs only after this marker's items are recorded; nothing withheld above may be claimed earlier.
