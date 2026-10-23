# GATE-G6 — Round-4 discharge + launch-readiness review · readout

- **Date:** 2026-10-23 · **Reading:** 1 (agent-drafted; the operator dispositions)
- **Criterion (the operator's charter, verbatim):** "so that we can be certain that by the end of this next
  orchestrate-build drive we have a fully ready public open-source GitHub repo launch, with no remaining deferred work
  and deep end to end live verification and so on" (Devin CLI chat, 2026-10-20).
- **Contract:** `docs/tickets/166_GATE-G6__round4-discharge.md` · **Blocks:** R4.17, HUMAN-H7, R4.18.
- **Chain tip at this read:** `svitali/harnessharness-r4.16` @ `c5bda1c` (R4.16 closeout head; stacked PR #196 open —
  mergePolicy OPERATOR; the operator merges the whole stack).

---

## Verdict — NOT PASSABLE on the zero-deferral threshold (one finding)

Ten of the eleven pre-registered thresholds are met on evidence. The **zero OPEN DEFERRALS** threshold is
**not met**: two rows remain OPEN and R4.16 surfaced them honestly rather than carrying or falsely closing them —
**`DF-S1.2-2`** and **`DF-S1.5-3`**, the `e2-python` foreign toolchain's `hir-documents.e2-replay` /
`ledger-scenarios.e2-replay` extension cells (`verified_by[]` empty — `foreign/e2-python` does not implement those
arms). They are the same residual that gates `R-2.12.1` at PARTIAL (its only non-discharged member). Per the charter's
DEFERRALS rule — *"This gate does not pass while any `OPEN` row in `docs/tickets/DEFERRALS.md` remains"* — the gate
cannot pass until the operator dispositions them.

**The two dispositions the gate offers:**
1. **An insert** — implement the two e2 extension arms in `foreign/e2-python` (the toolchain was provided under
   HUMAN-H3; the base six arms already replay 49/49 `foreign-run-verified`). This is real foreign-implementation
   work — the arms must parse/replay the committed `hh-xcheck-bundle/1` `hir-documents` and `ledger-scenarios`
   cases byte-for-byte. Round 4 → 56 rows.
2. **A descope/waiver ADR** — the operator records that the two e2 extension arms stay `environment-pending` /
   descoped as a permanent decision (the E3 arms replay the same corpus 57/57; the e2 base arms replay 49/49 —
   the extension cells are an *additional* foreign surface, not a gap in the in-tree ceiling). Carrying an ADR
   with the operator's recorded ruling satisfies the "closed waived/descoped by an ADR" arm of the threshold.

Neither is decided here — the operator's recorded words choose.

---

## Pre-registered thresholds — verified (fixed 2026-10-20, before the gated work)

| # | Threshold | Verdict | Evidence |
|---|---|---|---|
| 1 | **Zero OPEN DEFERRALS rows** | **✗ FAIL — finding** | `scripts/deferrals-open.sh` prints `DF-S1.2-2` + `DF-S1.5-3` OPEN (the e2 extension cells; the other 99 of 101 rows discharged with dated markers or waived/descoped by ADR). |
| 2 | Zero open BL rows | **PASS** | `scripts/deferrals-open.sh --bl` prints nothing. R4.16 flipped BL-02/03/05 + BL-30…BL-39 exactly once; end state 41 closed, 3 `accepted` (BL-01 the standing e2 home + BL-47/48 the trigger index homes — `accepted` index rows, not open debt); BL-41/42/43 closed `converted-to-policy` at L4.2. |
| 3 | Coverage MET or MET-DIFFERENTLY-permanent | **PASS with the one shadow** | Verdict column: **60 MET · 5 MET-DIFFERENTLY** (each cites a permanent-design ADR: ADR-0146, ADR-0050×2, ADR-0210, ADR-0003) **· 1 PARTIAL** — `R-2.12.1`, whose only non-discharged member is the same e2 residual (its other legs: DF-S0.3-2 R4.5, DF-S0.3-3 waived ADR-0371, DF-S1.8-1 R4.16, DF-S1.27-1 R4.4b). The PARTIAL clears when the finding is dispositioned. |
| 4 | Live legs evidenced; spend ≤ USD 100; no secret value | **PASS** | 34 `hh-live-evidence/1` records committed under `docs/build/reports/live/`; `SPEND.jsonl` cumulative **$25.259768 / $100**; gitleaks staged + full-history clean (only the 7 allowlisted fake-test-secret hits). |
| 5 | Launch hardening green | **PASS** | CI matrix ubuntu+macOS green on the chain tip incl. cargo-deny, gitleaks, drift, foreign legs, quickstart; publish dry-run green over the 49-crate publish set (`publish-order.sh`, 49/49 names free); release-workflow dry-mode artifacts produced (aarch64-apple-darwin tarball + sha256 locally — dry-mode dispatch is default-branch-only, recorded); license/notices/community files present (9 living OSS files at repo root). |
| 6 | Surface coverage — every public surface/binding/plugin live-executed or operator n/a | **PASS** | `docs/build/OPERATIONAL_READINESS.md` R4.16 sweep: `hh` CLI, bindings (a)/(b)/(c) incl. the SDK/embed library API via the `hharness` facade, `hh-web`, `hh-mcp`/`hh-mcp-serve`, `hh-mcp-lab`, `hh-acp` (official ACP SDK client), the TS client (E3), the three packaged plugins OOP, `hh-github-issues` adapter, every provider lane — all `live-executed` with cited records; E2/E3 foreign = `foreign-run-verified` (honest distinct class). No silent gaps. |
| 7 | Main containment | **PASS** | `git merge-base --is-ancestor origin/main HEAD` exit 0 on `c5bda1c`; the 34-commit main drift merged at `6a6475f` (R4.16, 0 conflicts) and re-verified. |
| 8 | DF-S1.15-1 full-membership enumeration | **PASS** | R4.16's dated member-by-member enumeration shows every scope member with a dated marker (custody landed R4.15e). |
| 9 | GATE-ACCEPT — ten deviations discharged or permanent | **PASS with the residual surfaced** | Discharge table (planning §651): 1 permanent (R-2.7.2 split, ADR-0146); 9 discharged by named round-4 rows. Deviation 9 (foreign-toolchain cells) carries the same two e2 extension cells as its honest residual — the gate's one finding. |
| 10 | check-build-memory 0 violations; check-backlog exit 0; pre-authorizations dispositioned | **PASS** | `check-build-memory.sh` 0 violations (8 pre-existing warnings); `check-backlog.sh` exit 0 (166 expected sources); the two round-4 pre-authorizations — the $100 live-spend cap ($25.26 consumed) and the H3 corpus/toolchain authorization (consumed by the e3/e2 legs + R4.6/R4.9) — dispositioned, expiring at this gate's disposition. |
| 11 | DF-R4.2a-1 CI after-state measurement | **PASS — honest MISS recorded** | R4.16 measured the D4 window (PRs #152–#195, 146 runs): actual 676 job-min vs ~1882 counterfactual → **64.1 % reduction**, honestly *missing* the 77–80 % projection by ~13–16 points (causes: the L4-era job set doubled, 23 superseded-head cancels, more full anchors/ticket — workload-shape, not classifier safety; 5 fast heads spot-verified carried no non-docs changes). Docs-only heads met the ~1-min projection (0.96 avg). Recorded, not hidden. |

---

## Readout duties (amend (b))

### Spot-audit — evidence records to check against the provider consoles

The committed `hh-live-evidence/1` records carry `request_ids: []` and `response_headers: {}` — the allowlist scrub
(R4.5a, ADR-0363 D2) permits empty, and the values were never captured into the committed record (the credential
broker redacts them). The operator reconciles **by provider + date + cost** against the consoles — the request ids
are not committed anywhere by design. ≥3 records, ≥1 per hosted provider:

| record (under `docs/build/reports/live/`) | provider | result | cost_usd |
|---|---|---|---|
| `2026-10-21-anthropic-call.json` | anthropic | pass | 0.000575 |
| `2026-10-23-capstone-run-anthropic.json` | anthropic | fail:live-tests (the surfaced replay divergence — fixed at L4.6b) | 2.0 (declared bound) |
| `2026-10-21-openai-call.json` | openai | pass | 0.00002 |
| `2026-10-23-capstone-run-openai.json` | openai | pass | 0.000158 |
| `2026-10-23-capstone-judge.json` | multi-provider | pass (R4.7e calibrated judge pair) | 0.001905 |

### Spend reconciliation

`SPEND.jsonl` is an estimate; the provider consoles' billing is the authority. The operator reconciles the ledger
total **$25.259768** against the consoles here and the readout records the reconciled figures. (No credential
values anywhere — `provided: yes/no` only.) *To be filled by the operator's reconciliation.*

---

## D4 full-run CI evidence (the gate's CI line)

`workflow_dispatch` run **39939140505** on chain tip `svitali/harnessharness-r4.16` @ `c5bda1c`
— **in_progress at read** (cargo-deny + gitleaks legs already green; the two build·test·fmt·drift legs in flight — the verdict is decided by the deferrals finding, not CI; the line is updated when the run lands).

## Carries (surfaced, accounted — none blocking except the finding above)

- **23 consecutive size deviations** recorded-and-ruled across the live/launch track — the under-priced live budgets
  are an honest pattern carried to this gate; the operator's merge gate decides each.
- **`accepted_signers`** (R4.15d, ADR-0375) — carried but unverified; the adopted text permits the attestation field
  to ride `attestations.jsonl` without a verify path; the ADR names the revisit trigger.
- **`ipfs:` admit-then-refuse** (R4.15d) — the `content_addressed` class admits then `transport_unsupported`-refuses,
  ledgered — the adopted ruling's own "typed Unsupported" arm.
- **`session/update{state_update}`** (L4.6a) — the dialect's state_update fails the official ACP SDK's closed update
  union → `-32602` logged + dropped (projection only); surfaced for an ADR/insert, not silently shipped.
- **DF-R4.2a-1's MISS** (64.1 % vs 77–80 %) — honestly recorded; the mechanism is sound, the projection was
  workload-shape-wrong.

## Operator actions at this gate

1. **Disposition the finding** — insert (implement the e2 extension arms) vs descope/waiver ADR for DF-S1.2-2 +
   DF-S1.5-3. The gate does not pass while they stay OPEN.
2. **Reconcile spend** — record the console-reconciled figure in §Spend reconciliation.
3. **Sign the disposition** — PASSED | SKIPPED-BY-OPERATOR | NOT PASSABLE (+ what would pass it), recorded in
   `docs/build/LEDGER.md` GATE DECISIONS and this readout.
