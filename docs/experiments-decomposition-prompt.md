# Experiments program — decomposition hand-off (frozen)

- **Written:** 2026-11-07T03:24:26Z by the coordinator at ratification (synthesize-spec `ratify`, O3).
- **Spec:** [`docs/experiments-spec.md`](experiments-spec.md) — canonical v0.3, ratified by the operator under Q-X20. sha256 at hand-off (with the canonical status line): `0bcfb3572dcc0bc921014f265ee34876f2f74bad7b6cd685abe214ebcf78362e`; pre-flip v0.3 draft sha256 `7373d5205f8a602de74f4cb285802f79bd284316fc5bd6526f0a84927386dda9` (commit `47baec98`).
- **Upstream record:** [research ledger](experiments-research-ledger.md) (frozen; §Q register Q-X1…Q-X63), [founding brief](experiments-brief.md), [outline trace](experiments-OUTLINE_TRACE.md) (18/18 OL obligations mapped), research index = spec Appendix C (notes `research/24…29`, `design/30…42`).
- **Mode:** frozen. Do not edit; a changed invocation is a new dated hand-off file.

## 1. Invocation

```
decompose-spec spec=docs/experiments-spec.md build_name=experiments mode=extend \
  tickets_dir=docs/tickets dispatch_target=subagent granularity=balanced tail=full
```

`mode=extend` appends a new numbered round banner `### Round 6 — <date -u +%F> · experiments first pass (decompose-spec mode=extend)` to `docs/tickets/00_MANIFEST.md` after row 251 (the manifest tail at `main` `185c16c7`; re-check at run time — `main` moves). Round 6 is the **first pass** of spec §10.1 and nothing else; later passes (Round 7+: tranches, full L2, explore → confirm, remaining harnesses, Terminus-2) are decomposed only after their gates (REQ-XH-001, REQ-XH-024; Q-X45 recorded default).

## 2. Binding constraints for every Round-6 row

- ⚑ **Spend ceiling.** First pass total ≤ $100 across model + compute + other (Q-X47, Q-X50); split model ≤ $70 / compute ≤ $20 / $10 unauthorized reserve (Q-X54); purchased Ollama credit counts in full (Q-X63). No ticket authorizes spend; spend is authorized only at the recorded gates of spec §10.3 (FP-S, FP-E, PROV-W*), verbatim, per REQ-XH-004.
- ⚑ **Skeleton rule.** Every paid or provisioning execution body (the W1–W4 and Wc window run rows) is a `Kind: skeleton` row with no Run line until its gate passes; a scoped `decompose-spec` insert then fleshes it (Q-X58, REQ-XH-001, REQ-XH-024). Never pre-answer a gate; a pending gate is RETURN PASS.
- ⚑ **Guard first.** The first row is the Round-6 bootstrap GATE (operator go-ahead); the first implement-spec row after it extends `hh-supplement-guard/1` to this supplement (REQ-XH-002, AR-31). Until that extension lands, no product ticket dispatches.
- ⚑ **Operator decisions** are those of the ledger §Q verbatim (Q-X1…Q-X63); ratified defaults are confirmed only at the gate each names.
- **Requirement IDs:** `req_id_pattern: REQ-X[A-H]-[0-9]+`, append-only, verbatim. Every adopted ID (370) maps to exactly one primary owner row; merged/superseded aliases keep their literal predicates at the target (REQ-XH-003). The round-5 `REQ-[A-H]-[0-9]+` register is untouched.
- **Repository invariants** (root `AGENTS.md`): K-2, CC1, CC7 (schema → regenerate), CC10, durable-before-visible, honest refusal (typed `Unsupported`/`Refused`/`n/a`), no new dependency without an ADR, secrets never recorded (`provided: yes/no`), append-only history, typed dispositions never relabelled, `mergePolicy: OPERATOR`, build-memory writes via shell/python + `git diff` verification (gotcha 4).
- **ADRs/amendments:** the deduplicated register of spec §10.5 (AR-01…) is owned row by row; an ADR row precedes the first code row that needs it.

## 3. Sizing (Q-X51)

The implementation worker is Cognition SWE-2 at High effort with a 256k-token context (LEDGER `harness: devin-desktop/swe-2-high/subagent`). `dispatch_target=subagent` is a hard ceiling: context estimate + 10k verification reserve ≤ 40 % of the worker context — a per-ticket working set of about 100–120k tokens including implement-spec's self-review, gap-analysis and verification passes. Use the Round-5 contract header `Size budget: <n>k estimated focused context + 10k verification reserve; ≤<lines> changed lines (split trigger, not target)`. Pre-split: PROXY-CORE (a/b), RECIPE (a/b), F2-c, D1-b. Watch: ADMIT-CORE, RUNNER-CORE, E2-T2/T4 (spec §10.6). A measured overflow stops at the next boundary for a scoped re-plan (OL-16).

## 4. Mechanics the retained guard enforces (read from `docs/build/tools/supplement_guard.py` at `185c16c7`)

- Chain IDs match `^(GATE|HUMAN)-[A-Z0-9]+$|^[A-Z]+\d*(\.\d+[a-z0-9]*)?$` (e.g. `R6.1`, `R6.12a`, `CAP6.1`, `REC6.1`, `DOC6.1`, `GATE-G20`, `HUMAN-H12`); filenames open with their sequence; kinds ∈ {ticket, gate, human, skeleton, capstone, reconcile, docs}; `Depends on:` points backward only.
- `tail=full` is mandatory: the active round must contain the contiguous kinds capstone ×3, gate, reconcile ×3, docs ×2, and that gate's filename contains `accept`.
- Routing: `nextTicket` must be the active round's first unlanded row; landed = BUILD_INDEX id/seq (tickets) or a PASSED GATE DECISIONS row (gates) or a non-PENDING readout (HUMAN).
- LEDGER at the extension: `round: 6`, `projectStatus: IN_PROGRESS`, `nextTicket:` the bootstrap gate, `chainTip: main` (the round-5 tips `codex/harnessharness-r5.59` / `codex/harnessharness-bl-reconcile` no longer resolve locally), `buildBranchBase` and `pinnedBaseSha` unchanged (both are ancestors of `main`).
- Pre-existing (not caused by this program): with the stock checker the guard reports 46 `unexplained-diagnostic` legacy `reqcov` warnings absent from its reviewed baseline; record as an OPEN FINDING for the guard-extension row, never silently re-baselined.

## 5. Coverage register

A literal owner table plus a machine-readable companion (the round-5 precedent `docs/build/planning/2026-10-30_evidence-coverage.{md,json}`), sourced from spec Appendix A, written with the decomposition under `docs/build/planning/`.
