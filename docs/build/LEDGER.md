<!-- docs/build/LEDGER.md — machine-state file (BM-LEDGER-01..07). Seeded by decompose-spec.
     STATE ONLY — the manifest is the plan. CURRENT STATE keys in EXACTLY this order. -->
# Build ledger — the machine-state file

> **OPERATING MODE.** Fresh session: read this ledger, then `docs/tickets/00_MANIFEST.md`,
> then `docs/tickets/DEFERRALS.md`. Run the row named by `nextTicket` via `implement-spec`;
> a gate is a pause, not a block. Resume line:
> `implement-spec spec=docs/tickets/<nextTicket file> worktree=<buildWorktree> base_branch=<chainTip>`

## CURRENT STATE

```
projectStatus:   NOT_STARTED        # NOT_STARTED | IN_PROGRESS | BLOCKED | PAUSED | DONE
nextTicket:      SETUP
lastCompleted:   (none)
blockedOn:       (nothing)          # REAL blocks only; a pending gate is a RETURN PASS row
pauseRequested:  false
returnPass:      (none)
manifest:        docs/tickets/00_MANIFEST.md
canonicalSpec:   spec/CANONICAL_SPEC.md
memoryRoot:      docs/build
dispatchTarget:  subagent
buildWorktree:   (set at SETUP)
buildBranchBase: (set at SETUP)
pinnedBaseSha:   (set at SETUP)
chainTip:        (set at SETUP; advances per completed chained ticket)
benchmarkSet:    PENDING_CREATE     # the Stage-3 reference suite + exemplars; defined at SETUP
autonomy:        (set by orchestrate-build)
mergePolicy:     OPERATOR           # NONE | OPERATOR | AUTO-BOTTOM-UP
round:           1
updatedAt:       2026-09-11
```

## OPEN FINDINGS

(none — carry cross-ticket findings here; not per-ticket blocks)

## GATE DECISIONS

| date | ticket | gate | item | answer | consequence |
|---|---|---|---|---|---|

## RETURN PASS

| ticket | gates | what the operator must do | re-run line |
|---|---|---|---|

## PHASE LOG

- 2026-09-11 · Ledger created by decompose-spec from `spec/CANONICAL_SPEC.md` (v1.0-rc2, PASS 7/7); 100 implement-spec tickets + 3 milestone gates + 2 human prerequisites + GATE-ACCEPT (106 chain rows total); dispatch=subagent; plan is revisable at run time.
