# GATE-G11 — Authorize exact S1 and S4 executions · readout
Status: PENDING

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is approval.

## Criterion (verbatim)

> Retain both; no success-seeking replacements.

## Pre-registered thresholds (verbatim)

Require exact unchanged task/oracle and supported HelperExecutor/Podman boundary, confinement/observer/probe proof, all native cap/wire/grade/receipt preconditions and separate §5 quotes. Bind two S1 subjects and 8×3 S4 cases, ≤1 diagnostic setup retry per cell and stops. Missing backend requires explicit scope decision, never substitute backend.

## Scoped request (agent-drafted)

Authorize exact S1 and S4 executions — two S1 subjects and 8×3 S4
cases on unchanged task/oracle, supported HelperExecutor/Podman
boundary, confinement/observer/probe proof, all native
cap/wire/grade/receipt preconditions, separate §5 quotes, ≤1
diagnostic setup retry per cell, and stops.

### Material precondition finding (honest — shapes your decision)

The R5.42a backend preflight returned **inconclusive** with
`scope_return=true`, `admission=not_s1_s4_acceptance`
(`docs/build/reports/2026-11-03-r5.42a-backend-preflight.json`):
- `backend_candidate` = `inconclusive{backend_unavailable:podman}`
  — podman is absent on this host (`podman info` exits 125, no
  machine/connection)
- `observer_probe` = `inconclusive{probe_absent}` — the
  `exec.probe` read-only probe was ENOENT on the seatbelt lane
- tb2_lifecycle / mediation / acquisition_network / worksheet = all
  `verified` (real mediation on the seatbelt lane, real `build_quote`
  worksheet, 9/9 in-tree pins hash-verified)

The pre-registered threshold is explicit: **"Missing backend requires
explicit scope decision, never substitute backend."** The gate cannot
honestly pass while `backend_candidate` is inconclusive — no
substitute backend is admissible, and the seatbelt lane is the
fallback surface, not the Podman candidate.

### Scoped items the executions would touch (on approval)
- Unchanged TB2 task/oracle — the pinned suite.tb2 tasks declared at
  HUMAN-H8 (a-count/a-echo/a-hello/a-sum/a-upper); no substitutions.
- Two S1 subjects + 8×3 S4 cases — the exact binds; unchanged
  oracle; no favorable replacement.
- HelperExecutor/Podman boundary — the supported candidate; requires
  the backend to actually exist.
- Confinement/observer/probe — real proof from the landed
  R5.32/33/34 machinery.
- Native cap/wire/grade/receipt preconditions — the landed spine.
- §5 separate quotes — the declared §9.2 minimum-admission worksheet.

### Explicitly NOT admitted (until you decide)
- Any S1/S4 execution, study subject, policy search, substitute
  backend, or new dependency.
- Spend beyond the declared worksheet floor.
- Public/participant/publication claims.

### Your decision
PASSED only authorizes the exact scoped executions above — which the
inconclusive backend currently cannot satisfy. Your real choices:
(a) supply the backend (install podman / point at a supported
environment), then re-run the preflight leg and the gate can pass on
a `verified` backend; (b) record an explicit scope decision that the
backend is genuinely unavailable now and disposition what (if any)
bounded slice can proceed — with carried unmet predicates named and
compensating controls; (c) NOT-PASSABLE + what would pass.

Confirm the agent-drafted block by its sha256 prefix.

## Signature

(pending — operator words recorded verbatim here and in GATE DECISIONS when supplied)
