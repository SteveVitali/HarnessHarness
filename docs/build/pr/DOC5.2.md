# DOC5.2 — Refresh agent ownership and verification guidance

Spec: `docs/tickets/246_DOC5.2__doc2.md` (seq 246 of 246 — the final
manifest row · Round 5 · docs · role DOC.2). Base:
`codex/harnessharness-evidence-doc5.1`. Docs-only; offline; USD $0;
no production mutations.

## Summary

Refreshes the agent-facing docs (the `AGENTS.md` hierarchy plus the
ownership/verification guidance it points at) to match the accepted
round-5 record — the GATE-G17 accepted state and the landed machinery
it folded. The structural freshness detector reported 0 issues, so
this run performed the semantic claim-by-claim audit the ticket names
and made surgical fixes only.

## What changed

- `AGENTS.md` — Key-files count corrected (`ci.yml` ~300→~350); the
  docs-check line now names the round-5 doc-surface checkers
  (`check-position`/`check-examples`/`check-trace`/`check-contributing`
  + `--head` modes) and `run-examples.sh`; the build-memory section
  records the retained routing guard: `check-supplement-guard.sh` must
  return `PASS` + `DISPATCH — <id>` before/after dispatch boundaries,
  `HOLD` = RETURN PASS.
- `docs/AGENTS.md` — Build & Test names the four doc-surface checkers
  with their pinned surfaces; new gotcha: the checkers' self-tests
  (`scripts/ci/test-check-*.sh`) mutate exact pinned phrases —
  preserve mutation points (unmet clauses, marked blocks, gated lanes,
  closed card states).
- `crates/AGENTS.md` — `hh-embed/src/lib.rs` count 71→72.
- `MAINTAINING.md` — build-memory tools section names
  `docs/build/tools/` (guard + dispatch driver) and
  `merge-dryrun.sh`; Releasing names the ADR-0380 publish-readiness
  checks (`check-publish-metadata.sh`, `check-docsrs.sh`).
- `docs/build/reports/round5/AGENT_DOCS_CHECK.md` — the contract
  artifact: method, round-5 machinery vs prior guidance, corrected
  claims, verified-unchanged audit record.
- `docs/build/runs/DOC5.2.md` — run ledger.

Audited with zero drift: `clients/AGENTS.md`, `foreign/AGENTS.md`
(every key-file count, exit-code and path claim verified against
scripts/manifests).

## Verification

- `check-agent-docs-freshness.sh .` → 5 docs, 0 issues (pre + post).
- `check-living-docs.sh` OK (456 files) · `check-position.sh` 13 legs
  · `check-examples.sh` (4 cards, 6 pins, 29 ids) · `check-trace.sh`
  · `check-contributing.sh` 12 ids (no_ritual holds) ·
  `check-bm-tools.sh` OK · `check-ci-gating.sh` OK ·
  `cargo fmt --all --check` clean (no Rust touched).
- `check-build-memory.sh .` → 0 violations (8 baseline warnings, the
  guard's recorded baseline set).
- Supplemental guard `check-supplement-guard.sh` → PASS, route
  DISPATCH — DOC5.2, pre + post artifacts (identical digests).
- Every command added to the docs was verified to exist and its
  contract read from the script's own header/body; every kept claim
  verified against source/manifests/CI (audit table in
  `AGENT_DOCS_CHECK.md`).

## Requirement IDs

None — contributing slice only; no primary owner.

## AC → evidence

| AC | verdict | evidence |
|---|---|---|
| AC-1 refresh agent instructions, narrow verification + gate/guard rules | met | edits above + `AGENT_DOCS_CHECK.md`; pointers not copies; truthfulness = verbatim dispositions, no promoted states |
| AC-2 no frozen edits / invented paths / stale authority / unguarded instructions; uncompleted work visible | met | living files only; all paths verified; guard documented as precondition; DF-R5.1-1/DF-H9-1/DF-CAP5.1-1 carried OPEN |
| AC-3 ownership/evidence mapping | met | no primary targets — contributing slice; achieved D/E only |
| AC-4 scoped verification | met | docs battery + freshness detector + fmt; counts in run ledger |
| AC-5 universal phase gate | met at close | run/PR/BUILD_INDEX/LEDGER agree; ≤2KiB phase log; ci recorded |

## Deferrals

Opened: none. Closed: none. Carried OPEN per GATE-G17:
DF-R5.1-1, DF-H9-1, DF-CAP5.1-1.
