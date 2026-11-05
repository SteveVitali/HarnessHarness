# REC5.2 — Reconcile landed code, ADRs and supplemental coverage

## Summary

Round-5 spec reconcile (`reconcile-build mode=spec`, proposals-only, records-in, offline —
USD $0, no task/provider/container/participant execution). Reads the actual landed record at
`169e3c25` and reconciles it against `docs/evidence-and-adoption-spec.md`: every one of the 63
landed round-5 BUILD_INDEX rows (seq 169–242) is tagged in-spec / spec-implied / ticket-added
with a disposition, and every changed contract/generator/tier/dependency admission plus every
supplemental layer link is verified against what is committed. Delivers the contract's two
named artifacts under `docs/build/reports/round5/` — `TICKET_VS_SPEC.md` and
`SPEC_RECONCILIATION_PLAN.md` — never overwriting the round-1–4 same-named records.

## What changed

- `docs/build/reports/round5/TICKET_VS_SPEC.md` — the round-5 tag table (63 rows) + a
  15-check landed-record verification table (V-1…V-15) + the REQ-E-210 resolution
  (E210-1…E210-7) + per-invariant and per-operating-clause dispositions.
- `docs/build/reports/round5/SPEC_RECONCILIATION_PLAN.md` — **1 proposed amendment, 0
  applied** (R5A-1: canonical Appendix-B by-reference range ADR-0330→0420, `[ ]` unticked —
  the frozen BM-ADR-04 mismatch preserved as dispositioned legacy, never rewritten) plus six
  dispositioned rows (R5A-2…R5A-7: register state, existing-contract fixes, DF-R5.17-1, gate
  decisions, withheld-record vocabulary, the owed predicate set).
- `docs/build/runs/REC5.2.md` — run ledger (deferrals read, orient findings, gap table,
  evidence log).
- `docs/build/pr/REC5.2.md` — this record.

## Reconcile findings (the real record)

- **Dependency admission: clean.** The whole round-5 diff adds zero external dependencies;
  the only manifest change is `hh-env` `[dev-dependencies]` ×7 internal path crates (test-only,
  annotated no-cycle; none of the 7 lists `hh-env` back). `Cargo.lock` +7 lines.
- **Generator/schema/generated: clean.** Zero files changed under `schema/`, `clients/`,
  `hh-codegen`, `hh-embed-schema`, `hh-embed-client-generated`, `THIRD_PARTY_NOTICES.md`;
  `check-drift` regenerates byte-identical (CC7). No feature/tier diff at all.
- **Amendment routing: complete.** All seven §9.3 register rows → PADR-R5-1…7 → GATE-G9
  verbatim admission → 39 landing ADRs (ADR-0382–0420, every landing cites its packet);
  §9.3's existing-contract fixes (M1 origin, R0 mapping) landed as repairs, not amendments.
- **Coverage anchors: resolve.** Register hooks (`req-e-210`, `ac-9-1/2`, `s9`,
  `ac-rec5-2-3`) exist; the guard PASSes 180 candidates / 0 violations; the COVERAGE_MATRIX's
  only two missing MET-row evidence pointers were this ticket's own `pr/` + `runs/` artifacts —
  now landed.
- **Deviations: none silent.** 47 manifest note rows cover every recorded size/SPLIT/AMEND
  deviation; DF-R5.17-1 (a real landed defect found by R5.17's independent battery) is DONE
  via the owning admission ADR-0418 D1.
- **Honesty: held.** 20 packets carry executed/failed/inconclusive/withheld/unselected/
  reported dispositions verbatim; the 3 OPEN deferrals (DF-R5.1-1, DF-H9-1, DF-CAP5.1-1)
  carry forward — none closed, none inferred, withheld stays withheld.

## Requirement ids

Primary ownership: **REQ-E-210**. Contributing only (never duplicate ownership): **REQ-H-004**.
REQ-E-210's full-target predicate is recorded achieved at D/E engineered through the numbered
checks E210-1…E210-7 (target file §9.1–9.4); R/P layers do not exist on this target and no
actual-execution E is claimed. The contributor cells (R5.3/R5.4/R5.5) cite landed evidence.

## Acceptance criteria → evidence

| AC | verdict | evidence |
|---|---|---|
| AC-1 (positive) | **met (D/E engineered)** | V-1…V-15 in `reports/round5/TICKET_VS_SPEC.md`: actual landed code/ADRs read via `git diff 001490c1…169e3c25`; every contract/generator/tier/dependency admission verified; reconciliation appended (new artifacts, originals untouched); index/triggers verified through tools (adr-index --check == committed; gen-revisit-triggers in sync) |
| AC-2 (negative) | **met (D/E engineered)** | No missing amendment (V-6 map complete), no reverse edge (V-2), no hand-edited generated artifact (V-3/V-4), no unadmitted dependency (V-1), no silent deviation (V-12); the one owed spec-text candidate (R5A-1) is a proposed, unticked row — documentation authorizes nothing retroactively |
| AC-3 (ownership/evidence) | **met (D/E engineered)** | REQ-E-210 resolved to E210-1…7 with committed evidence; achieved D/E recorded separately — required domain D/E only, no R/P owed, no actual-execution E claimed; contributors R5.3/R5.4/R5.5 evidence cited; full-target predicate NOT claimed satisfied by this close |
| AC-4 (scoped verification) | **met (E/D engineered)** | docs-only run: check-living-docs, check-bm-tools, check-build-memory `.`/`--staged`, check-drift, gen-revisit-triggers --check, adr-index --check diff, supplement guard pre+post; fails-without = the checkers themselves (each audit cell fails if its predicate broke); commands + counts in `runs/REC5.2.md` |
| AC-5 (phase gate) | **met (D/E engineered)** | REQ-E-210 + REQ-H-004 stamped; 3 deferrals remain OPEN with owner/trigger/control (none owed by this ticket — every found item already has a record home); run/PR/BUILD_INDEX/LEDGER advance at closeout; ≤2KiB phase log; no deferred predicate becomes MET by this close |

## Verification

- `bash scripts/deferrals-open.sh` → 3 OPEN rows (carried, per GATE-G17 verbatim).
- `bash docs/build/tools/check-supplement-guard.sh` → `hh-supplement-guard/1` v1.0.0 PASS —
  route DISPATCH REC5.2 (82 active-round rows / 180 candidates / 0 viol / 8 baseline warnings),
  pre-artifacts.
- `bash scripts/check-drift.sh` → ok, generated byte-identical.
- `bash $AGENT_SKILLS_ROOT/skills/build-memory/scripts/adr-index.sh --check docs/adr` →
  generated section identical to committed `docs/adr/README.md`.
- `bash scripts/gen-revisit-triggers.sh --check` → in sync.
- `bash scripts/check-living-docs.sh` → OK 456 files.
- `bash scripts/check-bm-tools.sh` → OK.
- `bash $AGENT_SKILLS_ROOT/skills/build-memory/scripts/check-build-memory.sh .` → 0 violations,
  8 scoped warnings (unchanged baseline).
- Audits (recorded in `runs/REC5.2.md` evidence log): Cargo.toml/lock diff; reverse-edge grep;
  generated-surface diff; classes.rs one-home; packet inventory ×20 parse+disposition;
  manifest-note↔deviation count (47); matrix evidence-pointer resolution (2 missing = this
  ticket's own, landed).
- `cargo` suites: **not applicable** — zero Rust source/test lines changed (docs-only
  reconcile); `cargo fmt` n/a.
- ci-boundary/1 at pushed head → `docs/build/logs/ci-rec5.2.json` (force-added).

## Out of scope honored

No new provider/backend/dependency, no schema hand edit, no old evidence rewrite, no implicit
outreach/spend/publication, no merge; sibling/later owners untouched; the 3 OPEN deferrals and
every withheld predicate carry forward exactly.
