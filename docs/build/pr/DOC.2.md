# DOC.2 — agent docs refresh

**Stacked on:** `svitali/harnessharness-doc.1` (PR #103) · **Do not merge — operator-controlled**
**This is the last row of the 106-row chain** — its closeout sets `projectStatus: DONE`.

## Summary

`agent-docs` (refresh mode) run against the composed build. The Phase-0
detector found one critical drift item (`AGENTS.md` referenced a
nonexistent `drive-build.sh` — template boilerplate) and the structural
review found the bigger one: **the root AGENTS.md was the 28-line
Build-memory template block only** — a fresh agent could not orient,
navigate or contribute from it. The doc was regenerated from verified
source per the skill's severe-drift rule, and the hierarchy was
completed.

- **`AGENTS.md`** — rewritten: Purpose, Architecture (the one schema
  source → the three `hh-embed/1` bindings → kernel spine → Lab tier →
  surfaces/plugins), Key files, Build & Test (exact commands), Code
  conventions (hermetic std-only, CC1/CC7/CC10, typed refusals,
  durable-before-visible, `tier-c*` features), Critical Gotchas (incl.
  the DF-DOC.1-1 `hh-kernel doctor` defect and the `edit`-tool anomaly
  on build-memory files), Terminology, Do/Don't. **Build memory
  section preserved verbatim** except the `drive-build.sh` fix.
- **`crates/AGENTS.md`** — new: the full 50-crate map + workspace rules.
  Per-crate leaf docs deliberately not generated (the map covers every
  crate; 50 minimal stubs would be filler).
- **`CLAUDE.md`** — new: the `@AGENTS.md` bridge (Phase 2.3).
- **`DEFERRALS.md`** — DF-DOC.1-1 progress note: stays OPEN as backlog;
  no `BL-` id yet (the BACKLOG catalogue predates the row — next
  `reconcile-build` assigns it).

## Requirement ids

The spec's docs-scope item is **R-2.12.4** (packaging/licensing/OSS
governance/docs/community) — `deferred(ADR-0210)` to WS-L6, **out of
build scope**; named for the record, not discharged.

## Verification

- `check-agent-docs-freshness.sh .` → **0 issues / 2 docs** post-run
  (the ticket's deterministic AC; was 1 critical pre-run).
- "Build memory" section present in a marked repo (deterministic AC).
- `check-build-memory.sh` → 0 violations (7 pre-existing warnings).
- All paths in the new docs `ls`-verified; crate map diffed against
  `ls crates/` (50/50).
- No runtime code changed — `cargo test` not run.

## DONE preconditions checked (closing act)

102 BUILD_INDEX rows cover every non-gate/non-human manifest row after
this one; all four gates (G1/G2/G3/GATE-ACCEPT) dispositioned PASSED;
HUMAN rows carried as signed acceptances; no `nextTicket` remains (`DONE`).
LEDGER goes `projectStatus: DONE` in the closeout commit.

## For the operator

Docs-only PR; safe to land in stack order after #103. After it lands
the build ledger reads `projectStatus: DONE` — the manifest chain is
fully landed.
