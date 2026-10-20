<!-- Round-4 launch re-charter insert (decompose-spec mode=extend amendment, 2026-10-20). A HUMAN marker
     (BM-TICKET-05): operator work, NOT an implement-spec input; no run line. -->
# HUMAN-H7 — Public launch acts (merge, protect, tag, release, publish)

- **Sequence:** 167a (round-4 row 48 of 49) · **Kind:** human · **Phase:** Round 4
- **Blocks:** R4.18 (the launch record)
- **Owner:** the operator   **Scheduled:** trigger = R4.17 landed after GATE-G6 PASSED
- **Withheld until done:** any `public` claim; "released"/"published" wording; `projectStatus: DONE`
- **Deferrals so far:** 0
- **Readout:** `docs/build/readouts/HUMAN-H7.md`

> **Operator work — NOT an `implement-spec` input.** `mergePolicy: OPERATOR` — only the operator merges, tags, publishes.
> The crates.io token is the operator's and never enters any ledger (`provided: yes/no`).

## Checklist
- [ ] Merge the stacked PRs into `main` in chain order (#139–#147, then the round-4 PRs bottom-up)
- [ ] Branch protection on `main` + required checks (the CI matrix, cargo-deny, gitleaks, quickstart)
- [ ] GitHub Environment `live-verify` — secrets by NAME + required reviewer (if not done at HUMAN-H6); private vulnerability reporting enabled
- [ ] Tag `v0.1.0` on `main` → the release workflow publishes binaries + `SHA256SUMS`
- [ ] `cargo publish` in `scripts/publish-order.sh` order with the operator's crates.io token
- [ ] Repo description, topics, website link; README badges resolve
- [ ] Flip `projectStatus: DONE` (the operator's flip — recorded with the operator's words)

## Exit criterion
Release URL live, every publishable crate visible on crates.io at 0.1.0, `main` protected — recorded with the operator's words.

## DEFERRALS rule for tickets that run before this completes
R4.18 runs only after this marker's items are recorded; nothing withheld above may be claimed earlier.
