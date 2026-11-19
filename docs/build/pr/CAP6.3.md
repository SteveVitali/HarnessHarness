# CAP6.3 — spec reconciliation + integration plan (manifest 323)

Requirements: none owned (contributing REQ-XH-029 stack-CI read + REQ-XH-015
history citation — CAP6.2-owned). Base: `codex/harnessharness-cap6.2`
(PR #418 stack). Offline, USD $0.00 — no merges, no secrets, no production
mutations, no spec edits (`apply_amendments=false`; spec frozen, X-01).

## What this lands

**Stage 1 — spec reconciliation** (`reconcile-build mode=spec`, proposals-only):

- `docs/build/reports/round6/TICKET_VS_SPEC.md` — all **85** landed round-6
  BUILD_INDEX rows (seq 253–322) tagged `in-spec` / `spec-implied` /
  `ticket-added` with dispositions; 15 landed-record verification checks
  (V-1…V-15): zero new dependencies, generated artifacts in sync, every
  admitted amendment carries its manifest line + ADR, spec sha256 re-verified
  against the re-pin.
- `docs/build/reports/round6/SPEC_RECONCILIATION_PLAN.md` — two proposed
  amendments, both `[ ]` **unticked**: **R6A-1** canonical Appendix-B range
  → ADR-0451; **R6A-2** the two owed §10.5 ADRs **AR-08/AR-10** (R6.10
  contract item 12 never discharged — mint-or-waive is a G32 item).
  Explicit dispositions for every register difference: non-ADR kinds
  (REC/SUPP/SCHEMA/decision) landed; later-pass ARs absent by design;
  **no new requirement ids proposed**.
- ADR parity: 19/21 needed-in-round-6 minted + 7 intervention (A1–A7) +
  2 record ADRs (0450/0451, BL-47, triggers in sync) = 29 round-6 ADRs.
- REQ-X coverage cited: **410/456 closed** (46 PARTIAL all dispositioned —
  34 routed · 5 discharged · 4 scope-ended · 1 unexecuted XC-325 · 2 aliases).
- §0.3 history: CAP6.2's REQ-XH-015 result cited; guard PASS `DISPATCH — CAP6.3`.

**Stage 2 — integration plan** (`reconcile-build mode=integration`):

- `docs/build/reports/round6/INTEGRATION_PLAN.md` — the 12-PR stack graph
  (bases/heads/commits/dated CI), external state (main `3c254707`, 15 PRs
  merged since chain began, no non-chain opens, Pages deploys pending),
  read-only merge dry-run **13/13 clean, 0 conflicts**, the **red-state
  diagnosis** (see below), the bottom-up merge procedure with #413's
  out-of-band ordering, rollback + post-merge verification, and the
  verbatim GATE-G32 staging list.
- `docs/build/reports/round6/RELEASE_NOTES.md` — first-pass-honest draft.
- `docs/build/planning/2026-11-19_round7-decision-memo.md` — round-7 seed
  (34 routed ids, clustered; no manifest row created).
- Committed read evidence: `merge-dryrun-2026-11-19.json`,
  `ci-stack-2026-11-19.json`, `ci-410/413-2026-11-19.json`.

## The red span — named, never silently greened

`check-ci-gating` readers scan: **57 un-allowlisted `docs/build/**`
literal reads + 3 stale allowlist entries** — introduced by the R6.48/49
results/freeze emitters + tests (records-in design reading committed
build records — the same class already allowlisted 25× for fp_select.rs).
First red: **#414** (run 44449044249, sole failing leg `test-check-ci-gating`;
cargo-deny + gitleaks green on all five PRs). The repair decision is the
maintainer's (allowlist extension vs ADR-0362-family amendment) —
INTEGRATION_PLAN item I-3; this ticket does not ship it.

## GATE-G32 staging (verbatim in runs/CAP6.3.md §"signature list")

REQ-XC-325 `unexecuted{no_executor}` waiver · Ws1 estimate-over-ceiling
`5,250,954 µ$` vs `4,136,280 µ$` (~127%) · two-fold compute-estimate
variance (`30.024544` interim vs `27.367214` authoritative folds) ·
operator-disk retention `unrecorded` · PR merge ordering (green span
bottom-up, #413 after #411, red span held pending repair decision) ·
AR-08/AR-10 mint-or-waive.

## Verification

Guard PASS (`DISPATCH — CAP6.3`) · build-memory 0 violations / 64 warnings
(all pre-existing families) · check-backlog PASS (66/66) · triggers in sync ·
check-drift ok · bm-tools OK · `git diff --check` clean · 20/20 round-6
sidecars recompute-match. Not run: cargo suite (docs-only delta; the
inherited CI red is recorded separately, never counted as CAP6.3's).

## Artifacts + SHA-256

| file | sha256 |
|---|---|
| docs/build/reports/round6/TICKET_VS_SPEC.md | `18e1dbb8ac4a83f4…` |
| docs/build/reports/round6/SPEC_RECONCILIATION_PLAN.md | `b341f5986383fea3…` |
| docs/build/reports/round6/INTEGRATION_PLAN.md | `05d96286159192ed…` |
| docs/build/reports/round6/RELEASE_NOTES.md | `c6fdcb389e53c4b7…` |
| docs/build/planning/2026-11-19_round7-decision-memo.md | `c2699eab210757ba…` |
| docs/build/reports/round6/merge-dryrun-2026-11-19.json | `a05af5e537a240ae…` |
| docs/build/reports/round6/ci-stack-2026-11-19.json | `3af6347437e2bfe4…` |
| docs/build/reports/round6/ci-413-2026-11-19.json | `caa364cef4ca3ae3…` |
| docs/build/reports/round6/ci-410-2026-11-19.json | `ba974544abaaabbb…` |

*This PR is a reconciliation/integration record: it asserts no merged PR,
no discharged operator act, no satisfied routed id, and no green on the
red span. `mergePolicy: OPERATOR` — only the operator merges (XO-04).*
