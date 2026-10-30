# GATE-G9 readout — admit exact architecture and method contracts

Status: PENDING   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

*Append-only. Each operator reading is a new dated block; never edit an earlier one.*

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is approval.

## Criterion (verbatim — `docs/tickets/174_GATE-G9__admit.md`, source: supplement §9)

> "No new semantics or inference admitted by drafting."

**Pre-registered thresholds (verbatim):** Operator/authorized maintainer dispositions all seven §9.3 rows and exact proposed ADRs, including encoding, correction authority, compatibility and numerical protocol. Guard and native launch proofs must pass. Record allowed implementation scope, separate test resources and expiry/void conditions. This admits design and implementation only; numerical validation remains evidence to be earned and study funding stays $0.

## Evidence (per item: source, `date -u` of the read, layer reached)

- `docs/build/planning/2026-10-30_amendment-dossier.md` — read 2026-10-30 — layer: engineered (all seven §9.3 rows `prepared`: PADR-R5-1*/2*/3*/4*/5*/6*/7*)
- `docs/build/tools/check-supplement-guard.sh` PASS at every boundary (route DISPATCH per row); R5.1 fixtures 13/13 — layer: fixture-verified
- `docs/build/tools/dispatch_driver.py` + actual transcripts — read 2026-10-30 — layer: engineered (real `run_subagent` stub spawn + real resume `ledger_reread`; 22/22 refusal fixtures incl. ci-fail/pending/missing, old-gate-g1, skeleton, stale signature)
- Stack CI: `gh pr checks` on #209–#213 — pass at heads (#211@b07d0c5, #212@8eeb374, #213@cbaf825) — read 2026-10-30 — layer: records
- `check-build-memory.sh` exit 0, 0 viol / 8 scoped baseline warnings; `check-amendment-dossier.sh` OK + fails-without proofs — run 2026-10-30 — layer: engineered
- `deferrals-open.sh` — DF-R5.1-1 only (intentional GATE-G17 contributor row) — read 2026-10-30 — layer: records
- Worker identity: `subagent_general` = parent model per harness docs; coordinator window 262k observed; worker model id not self-exposed (recorded honestly) — layer: records

<!-- agent-drafted:begin sha256=3d9a2b5af03a9a5f49aaba19d8308a3460ca66117f29d789a399bebf43b92fc0 -->
GATE-G9 orientation verified at presentation (reads this coordinator session):

- **Dossier state:** all seven §9.3 amendment rows `prepared` in `docs/build/planning/2026-10-30_amendment-dossier.md` — rows 1–2 PADR-R5-1*/R5-2* (R5.3, PR #211), row 3 PADR-R5-3* (R5.4, PR #212), rows 4–7 PADR-R5-4*/5*/6*/7* (R5.5, PR #213). `check-amendment-dossier.sh` OK with fails-without proofs at each owner.
- **Stack:** codex/harnessharness-evidence → r5.1 → r5.2 → r5.3 → r5.4 → r5.5; PRs #209–#213 open, all CI pass at recorded heads (#211@b07d0c5, #212@8eeb374, #213@cbaf825); chainTip codex/harnessharness-evidence-r5.5 descends from origin/main 001490c1.
- **Prerequisites per the gate text:** retained guard v1.0.0 PASS at every boundary (route DISPATCH per row); native launch path proven — real stub spawn + real resume (ledger_reread first) + 22/22 refusal fixtures incl. ci-fail/pending/missing, old-gate-g1, skeleton, stale signature (hh-dispatch-driver/1, PR #210).
- **State:** deferrals-open shows only DF-R5.1-1 (intentional GATE-G17 contributor row). Validator exit 0 — 0 viol / 8 scoped baseline warnings. Harness devin-desktop/swe-2-high/subagent; workers run subagent_general (parent-model inheritance); window 262k coordinator-observed; worker model id not self-exposed — recorded honestly.
- **Scope:** admission of the seven proposed amendment rows admits DESIGN + implementation only. Numerical validation remains evidence to be earned (R5.16/17, GATE-G10; R5.40). Study/task/provider spend stays $0. Skeletons stay undispatchable. Nothing herein discharges D/E/R/P full-target predicates.
- **Your disposition should name:** the seven rows (adopt/amend/decline per row), allowed implementation scope, separate test-resource bounds, expiry/void conditions, and carried unmet predicates with compensating controls.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
