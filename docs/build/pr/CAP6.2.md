# CAP6.2 — Round-6 closure: billed-figure sums, live reads and the later-pass routing register

Requirements: REQ-XH-110 · REQ-XH-021 · REQ-XH-029 · REQ-XH-106 · REQ-XH-015 (closure);
routed per O4/REQ-XH-001. Base: `codex/harnessharness-cap6.1` (PR #417 stack).

## Closure headline

Engineering closed **410/456** · requirement satisfied **410/456** (46 PARTIAL, 0 MISSING;
MET-ENGINEERED never counted as MET). Spend: model **$4.52 billed** (HUMAN-H18) + compute
**$27.367214 estimated** (21 cost_lib records, flagged — never billed) + credit $0 =
**$31.887214 ≤ $110**; authorized ≤ $100 holds (W4 `unused{scope_end}`). GCP compute
billing `unavailable{no_billing_export}` — honest token. `compute-open.sh` empty;
`hh-fp-a1b2c3` `DELETE_REQUESTED` 2026-11-19T02:01:58Z (read, not re-executed).

## Artifacts + SHA-256

| file | sha256 |
|---|---|
| docs/build/reports/round6/CAPSTONE_CLOSURE.md | 3861bffd0895d4af… |
| docs/build/reports/round6/BACKLOG.csv | e0e90c80bed48bdc… |
| docs/build/reports/round6/BACKLOG.md | a56b76673805d001… |
| docs/build/reports/round6/OPERATIONAL_READINESS.md | 885297aae17537f9… |
| docs/build/reports/round6/LATER_PASS_ROUTING.md | 7d26ef7558a796e9… |
| docs/build/reports/round6/2026-11-19-fp-prereg-hash-drift.md | 178bbbee9cf5e0ba… |
| docs/build/reports/round6/2026-11-19-req-xc-325-unexecuted.md | e84a139987b2ffa3… |
| docs/build/reports/round6/2026-11-19-register-normalization.md | 5e1ca5a3661a5e0d… |

(.sha256 sidecars committed alongside each.)

## DF dispositions

DF-CAP6.1-1 closed (15 decision rows restated verbatim; guard PASS); DF-CAP6.1-2
discharged (dated prereg-drift amendment; frozen records untouched); DF-CAP6.1-3 deferred
(honest unexecuted + waiver request); DF-CAP6.1-4 closed (dated normalizations + 12
BUILD_INDEX landings); DF-CAP6.1-5 closed (checker repairs — compute-receipts scoping,
publication `-flip` records, fp_e_emit clippy, ADR index/triggers, stock false positive).

## CI / verification

Guard PASS `DISPATCH — CAP6.2` · check-backlog PASS · check-site 15/15 (internal+public)
· compute-receipts PASS · round6-coverage 35/35 · clippy/fmt clean · compute-open empty.
Inherited base-stack CI red on #414–#416 recorded unchanged — coordinator diagnosis owed.

## Operator blockers / GATE-G32 carries

1. Stack CI diagnosis (PRs #414–#416 red at base — inherited, not CAP6.2-caused).
2. Operator-disk retention decision (unrecorded — GATE-G32 signature list).
3. REQ-XC-325 unexecuted waiver request + Ws1 estimate-over-ceiling (item 3 of the
   three-part signature list).
4. Proposed coordinator text: LEDGER nextTicket CAP6.3 / lastCompleted CAP6.2 /
   chainTip this branch tip; BUILD_INDEX row `322 | CAP6.2 | ticket |
   codex/harnessharness-cap6.2 | <this PR> | codex/harnessharness-cap6.1 | 2026-11-19 |
   GATE-G32 pending | engineered — closure stages committed; runs/CAP6.2.md ·
   pr/CAP6.2.md · reports/round6/`.
