# CAP6.3 run ledger — spec reconciliation + integration plan (manifest 323)

Harness: devin-cli/swe-2-high/subagent · Branch `codex/harnessharness-cap6.3` ·
base `codex/harnessharness-cap6.2` @ `ada3f40f` · offline, USD $0.00, no merges,
no secrets, no production mutations (read-only `gh`/git reads only). Ticket:
`docs/tickets/323_CAP6.3__reconcile.md`. Owns no REQ-X ids; contributing
REQ-XH-029 (stack-CI read, CAP6.2-owned) + REQ-XH-015 (history citation,
CAP6.2-owned — cited, not re-checked as owner). "Stage 1 and stage 2 are
ordered stages with a committed checkpoint between them" — honored:

## Stages (contract order — each committed before the next)

1. `0c16ac9e` — stage 1: `TICKET_VS_SPEC.md` + `SPEC_RECONCILIATION_PLAN.md`
   (+ `.sha256` sidecars).
2. `27050a28` — stage 2: `INTEGRATION_PLAN.md` + `RELEASE_NOTES.md` +
   `planning/2026-11-19_round7-decision-memo.md` + committed read evidence
   (`merge-dryrun-2026-11-19.json`, `ci-stack-2026-11-19.json`,
   `ci-410-…json`, `ci-413-…json`) (+ sidecars).
3. (this commit) — stage 3: `runs/CAP6.3.md` + `pr/CAP6.3.md` + sidecars.

## Reconciliation counts (reconcile-build mode=spec)

- **Landed rows tagged:** 85 BUILD_INDEX rows (seq 253–322 incl. inserts) —
  the whole round-6 chain. Every row tagged `in-spec` (stamped primary REQ-X
  ids) / `spec-implied` (record/gate machinery the supplement's §3.9–§10.3
  honesty rules imply) / `ticket-added` (emitters, checkers, batteries —
  none earns a requirement id).
- **Dispositions:** `ID` throughout; two `FB→plan` amendments proposed, both
  `[ ]` unticked (`apply_amendments=false`):
  - **R6A-1** — canonical Appendix-B range `ADR-0217–0330` → `–0451`
    (carries forward unticked R5A-1; the BM-ADR-04 family).
  - **R6A-2** — mint the two owed §10.5 ADRs **AR-08** (proxy) + **AR-10**
    (`GenParamPolicy`) as dated landing records — R6.10 contract item 12 was
    never discharged; alternatively G32 waives them as an accepted deviation.
- **ADR parity:** 19/21 needed-in-round-6 ADRs minted; +7 intervention ADRs
  (A1–A7 → ADR-0443…0449, manifest lines 919–925); +2 unplanned record ADRs
  (ADR-0450/0451, placed BL-47, triggers in sync). Register non-ADR kinds
  honored: AR-02/03 (REC→R6.3), AR-16 (SUPP→R6.14 mechanism), AR-24
  (AMEND-C→applied verbatim at R6.38), AR-25 (SCHEMA→regenerated, drift
  check ok), AR-31 (decision→R6.1), AR-32 (SUPP→R6.3). Later-pass rows
  AR-19/26/27/29/30 absent by design.
- **Spec identity:** `docs/experiments-spec.md` sha256 `1879824c5fe5…` ==
  the re-pinned digest (A1–A7 append); body frozen otherwise (X-01).
- **REQ-X coverage (cited from CAP6.1×CAP6.2, not re-computed):**
  410/456 engineering-closed + satisfied (MET 409 + MET-DIFFERENTLY(ADR-0439)
  1; 0 MISSING). 46 PARTIAL dispositioned: 34 routed · 5 CAP6.2-discharged ·
  4 scope-ended verbatim (XD-110/XD-119/XH-111/XH-115, GATE-G30) · 1
  unexecuted (XC-325, waiver rides G32) · 2 aliases inheriting at targets.
- **§0.3 history (REQ-XH-015 cited):** CAP6.2's stage-3 check stands —
  15 restated decision rows read by `load_decisions`; guard PASS.

## Seam verdicts (recorded, never silently passed)

| seam | verdict | home |
|---|---|---|
| S-1 AR-08/AR-10 no ADR | real gap → R6A-2 proposed `[ ]`; mint-or-waive is a G32 item | SPEC_RECONCILIATION_PLAN |
| S-2 Appendix-B range stale | R5A-1 still unticked → R6A-1 proposed `[ ]` | same |
| S-3 DF-R6.45l-1 DONE text on unmerged #413 | row emits OPEN at tip; lands with #413 or a coordinator dated row (proposed below) | INTEGRATION_PLAN I-2 |
| S-4 DF-CAP6.1-1…5 no dated status rows | dispositions live in CAPSTONE_CLOSURE; proposed status-update rows below (DF-R6.50-1 pattern) | this file |
| S-5 §10.5 "21 ADRs" vs landed | 19 minted + 2 owed + 7 intervention + 2 record = 29 round-6 ADRs | dispositioned |
| S-6 red stack #414–#418 | check-ci-gating readers scan — 57 hits + 3 stale; diagnosed, never greened | INTEGRATION_PLAN §1c |

## Stack CI + merge dry-run (reconcile-build mode=integration)

- **Stack CI read** (`ci-boundary.sh --pr 418 --stack --no-wait`, evidence
  `ci-stack-2026-11-19.json`, read 2026-11-19T06:33:05Z): #407–#412 pass
  (4/4 each); **#414–#418 FAIL** — one leg only:
  `build · test · fmt · drift` ubuntu+macos (runs 44449044249 /
  44454413682 / 44458421386 / 44461964712 / 44470135241); cargo-deny +
  gitleaks pass on every PR. Out-of-band reads: #410 pass @`11a6524`,
  #413 pass @`ae72242`.
- **Red diagnosis (named, not assumed green):** `check-ci-gating`
  readers scan — 57 un-allowlisted `docs/build/**` literal reads
  (fp_results.rs bin+lib, r6_48/49/46 tests, fp_e_emit.rs) + 3 stale
  allowlist entries; first red at #414's head (31 hits, run
  44449044249, sole failing step `test-check-ci-gating`). A
  maintainer/operator decision — the checker's hint reserves
  allowlisting real reads. INTEGRATION_PLAN item I-3.
- **Merge dry-run** (`merge-dryrun.sh --ci`, read-only `git merge-tree`,
  2026-11-19T06:36:27Z): **13/13 branches clean onto main — 0
  conflicts**; 7 pass / 5 FAIL / 1 none (cap6.3, no PR yet).
- **Topology finding (I-1):** forked ancestry below `r6.48` — branches
  forked from in-flight parent tips (merge-bases recorded); true
  ancestry r6.48→cap6.3. All tips contain `main` `3c254707`.
- **#413 out-of-band:** base `gate-g30`, leaf — merges strictly after
  #411 (retarget to main) or is rebased.

## GATE-G32 signature list — staged verbatim

1. **Accepted deviations** (each with ADR/control): W1 upload as W2's
   first step + the reconcile placement (§10.5 A-rows/ADR-0444 lineage);
   prereg `admission_sha256` drift incl. `fp-seal-open` (CAP6.2's dated
   amendment); skeleton→ticket fleshing kind normalization; the
   exec-drain/model-port machinery line (PRs #404/#407); **+ CAP6.3:
   the two owed §10.5 ADRs AR-08/AR-10 (mint-or-waive).**
2. **Scoped-out owed legs with owner/trigger:** the 34 routed ids —
   owner = named later pass, trigger per row; **REQ-XC-325
   `unexecuted{no_executor}`** (waiver request
   `docs/build/reports/2026-11-19-req-xc-325-unexecuted.md`, never
   satisfied); **operator-disk retention — `unrecorded`, operator
   decision required, not inferred**; **stack-CI red at base —
   diagnosed §1c, repair decision owed**.
3. **Waivers requested:** REQ-XC-325 unexecuted; **Ws1 compute
   estimate-over-ceiling — `5,250,954 µ$` vs declared `4,136,280 µ$`
   (≈$5.25 vs ≈$4.14, ≈127%; crash/recovery + ~24 h wall time, recorded
   verbatim at R6.45q)**; **the two-fold compute-estimate variance —
   interim fold `30.024544` vs authoritative 21-record fold
   `27.367214` (membership scoping: smoke/ws1r rows); `both folds sit
   ≤ $30` is not claimed.**
4. **PR merge ordering:** Phase A green bottom-up
   `407→408→409→410→411→413→412`; red span `414–418`(+CAP6.3) holds
   pending the I-3 repair decision; #413 strictly after its `gate-g30`
   base lands or is rebased.

## Proposed coordinator-owned text (not edited — coordinator applies)

### LEDGER.md — CURRENT STATE block changes

```
nextTicket:      GATE-G32
lastCompleted:   CAP6.3
chainTip:        codex/harnessharness-cap6.3
updatedAt:       2026-11-19T06:55Z
```

(`blockedOn:` stays `(none)` — the red span is a merge-order item for
the operator, not a dispatch blocker; GATE-G32 is RETURN PASS per the
gate rule.)

### LEDGER.md — PHASE LOG entry (proposed)

```
- 2026-11-19T06:55Z · **CAP6.3 done — spec reconciliation + integration plan
  (manifest 323; offline, $0)** — branch `codex/harnessharness-cap6.3` (PR
  #TBD) · base `codex/harnessharness-cap6.2`. Stage 1: TICKET_VS_SPEC (85
  landed rows tagged in-spec/spec-implied/ticket-added) +
  SPEC_RECONCILIATION_PLAN — R6A-1 (Appendix-B range →ADR-0451) + R6A-2
  (owed ADRs AR-08/AR-10) proposed, all `[ ]` unticked; ADR parity 19/21
  +7 intervention +2 record; spec sha256 re-verified (frozen); seams S-1…S-6
  dispositioned. Stage 2: INTEGRATION_PLAN — 12-PR stack graph, merge
  dry-run 13/13 clean 0 conflicts, dated CI reads — **#414–#418 red on
  check-ci-gating readers scan (57 hits + 3 stale; diagnosed, never
  greened)**; #413 out-of-band ordered after #411; G32 signature list
  staged verbatim; RELEASE_NOTES draft + round-7 decision memo seeded (no
  manifest row). chainTip → codex/harnessharness-cap6.3 · nextTicket →
  GATE-G32 · engineered (offline, $0.00) · devin-cli/swe-2-high/subagent
```

### BUILD_INDEX.md — row 323 (proposed)

```
| 323 | CAP6.3 | ticket | codex/harnessharness-cap6.3 | [#TBD](https://github.com/SteveVitali/HarnessHarness/pull/TBD) | codex/harnessharness-cap6.2 | 2026-11-19 (PR open) | (none) | (none opened; DF-R6.45l-1 + DF-CAP6.1-1…5 status rows proposed) | engineered — spec reconciliation + integration plan (offline, USD $0.00): 85 landed rows tagged; ADR parity 19/21, AR-08/AR-10 owed (R6A-2 `[ ]`); R6A-1 `[ ]`; 13/13 merge-dryruns clean; #414–#418 red readers-scan diagnosed not greened; G32 staged verbatim | runs/CAP6.3.md · pr/CAP6.3.md · reports/round6/TICKET_VS_SPEC.md · SPEC_RECONCILIATION_PLAN.md · INTEGRATION_PLAN.md · RELEASE_NOTES.md |
```

(`#TBD` → the PR number once `gh pr create` returns; the coordinator's
row lands after the PR exists.)

### DEFERRALS.md — dated status-update rows (proposed, append-only)

Following the DF-R6.50-1 status-update precedent (dated row, lead token
preserved — row readers then emit the latest disposition):

```
- 2026-11-19 status-update (CAP6.3 proposal; coordinator applies):
  DF-CAP6.1-1 → DONE at CAP6.2 (15 decision rows restated verbatim in
  GATE DECISIONS; guard PASS DISPATCH) — lead OPEN cell is history.
  DF-CAP6.1-2 → discharged as accepted deviation via dated amendment
  `2026-11-19-fp-prereg-hash-drift.md`. DF-CAP6.1-3 → deferred to GATE-G32
  waiver list (XC-325 `unexecuted{no_executor}`) — rides the signature
  list. DF-CAP6.1-4 → DONE at CAP6.2 (normalization record + 12
  BUILD_INDEX backfills). DF-CAP6.1-5 → DONE at CAP6.2 (checker repairs).
  DF-R6.45l-1 → DONE text landed on `codex/harnessharness-r6.51` (PR #413)
  — row stays OPEN in the reader until #413 merges or this dated row
  lands; also normalizes its malformed `DONE  **OPEN**` lead.
```

## Verification battery (executed, this worktree, offline $0)

- `check-supplement-guard.sh` → **PASS** — `hh-supplement-guard/1 v1.1.0`,
  route `DISPATCH — CAP6.3` (post-dispatch re-read).
- `check-build-memory.sh .` → **0 violations, 64 warnings** — all
  pre-existing families (manifest banner, BM-ADR-04 appendix range —
  R6A-1's subject, ledger nextTicket-vs-lowest-row, oversized PHASE LOG,
  BUILD_INDEX column-count, legacy live-verification vocab, pre-guard
  readouts, missing Harness headers, legacy reqcov, harvest sizes).
  My files added none.
- `check-backlog.sh` (reconcile-build) → **PASS** — 66/66 both sums,
  complete + non-duplicating + verdict-consistent (235 expected sources).
- `gen-revisit-triggers.sh --check` → **in sync**.
- `check-bm-tools.sh` → **OK** (spec-section, unknown-id, --bl legs).
- `scripts/check-drift.sh` → **ok** — generated artifacts in sync.
- `git diff --check` (whole CAP6.3 delta) → clean.
- `.sha256` sidecars — 11 new files written in `shasum -a 256` format;
  all 20 round-6 `.sha256` sidecars recompute-match their payloads.
- `check-ci-gating.sh --readers` (diagnosis leg) → 57 hits + 3 stale —
  the inherited red; **not a CAP6.3 failure** — recorded as such.
- `deferrals-open.sh` → 7 OPEN rows emitted (DF-R6.45l-1, DF-R6.50-1,
  DF-CAP6.1-1…5) — the row-reader seams S-3/S-4, dispositioned with
  proposed coordinator text above.
- Not run: `cargo test`/`fmt`/workspace builds (docs-only delta — no
  code touched; inherited red recorded separately); `check-site.sh`
  (no site change); `check-living-docs.sh` leg — no living-doc edit;
  live reads beyond the dated CI/PR/Pages probes.
- One environment caveat: a `timeout`-prefixed probe earlier failed
  (`timeout: command not found`, exit 0) — **never counted** as a
  check; all results above are direct executions.

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
| docs/build/runs/CAP6.3.md | this file |
| docs/build/pr/CAP6.3.md | sibling record |

*Nothing above asserts a merged PR, a discharged operator act, a satisfied
routed id, or green CI on the red span.*
