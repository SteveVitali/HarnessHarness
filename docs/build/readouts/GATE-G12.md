# GATE-G12 — Authorize activation then fixed pilot · readout
Status: READING-1 PASSED · READING-2 PENDING

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is approval.

## Criterion (verbatim)

> Unset funds=$0; finite exposure missing→zero dispatch; no automatic top-up/cloud fallback.

## Pre-registered thresholds (verbatim)

Record separate one-task activation quote and, only after its result, explicit fixed pilot authorization ≤60 first subjects under §9.2 per-subject and tranche ceilings. Freeze tasks/splits/policies/caps/oracle/detectors/analysis/stops before data; no implied permission for next stage. One gate can have separate numbered readings, neither pre-signed.

## Scoped request (agent-drafted)

Two separate numbered readings, neither pre-signed:

### Reading 1 — one-task activation quote
Record a separate §5 activation quote for a single task under the
declared §9.2 worksheet (minimum admission: USD $0.00 free lines,
declared provider_unknown_exposure=0, retry/regrade ceilings at
minimum honest values). Tasks/splits/policies/caps/oracle/detectors/
analysis/stops frozen before any data — no implied permission for
the pilot.

### Reading 2 — explicit fixed pilot authorization
ONLY after the activation result is reviewed: authorize a fixed
pilot of the declared first-subject count under §9.2 per-subject and
tranche ceilings. Your declared worksheet binds the floor — the
minimum-admission pilot is N=10 (the 2·T·R≥10T floor at T=1, R=5),
well inside the ≤60 ceiling. The freeze (tasks/splits/policies/caps/
oracle/detectors/analysis/stops) is the same frozen set declared at
HUMAN-H8 and materialized by R5.41's protocol.

### Material facts verified
- R5.42 quote engine landed — refuses any stage with unbound
  pins/rights/exposure; unset funds → $0 + funding blocker; no
  automatic top-up/cloud/guessed-margin/favorable-shopping.
- Backend verified on real podman handshake (GATE-G18 + re-probe).
- Carried predicates DISCHARGED at R5.44 — the probe verb now answers
  (`undeterminable` honest) and execs run inside real podman cells
  (`observed:applied`).
- Numerical battery executed at R5.40 (84,000 datasets, complete).
- Executed packets exist: R5.44 `hh-ops-live/1` (all 11 legs
  verified) + R5.43 `hh-s1-useful/1` (honest inconclusive at its
  time — pre-repair machinery).

### Explicitly NOT admitted
- Confirmation stage (separate gate G13), parity tranche (G14),
  public/participant/publication claims, spend beyond the declared
  worksheet, substitute backend, policy search.

### Your decision (two readings)
Reading 1: approve the one-task activation quote — PASSED / NOT-PASSABLE.
Reading 2: after the activation result, authorize the fixed pilot
(N=10 declared floor / up to ≤60 ceiling) — PASSED / SKIPPED /
NOT-PASSABLE + carried predicates/compensating controls.
Confirm the agent-drafted block by its sha256 prefix.

## Signature

**Reading 1 — PASSED** — operator words (verbatim, interactive, 2026-11-04T05:30:08Z): "approve. proceed" — recorded verbatim in GATE DECISIONS and this signature. Scoped decision: the separate §5 one-task activation quote is authorized (declared §9.2 minimum-admission worksheet; frozen tasks/splits/policies/caps/oracle/detectors/analysis/stops before data; no implied permission for the pilot). Agent-drafted block sha256 `4d59d34fdc1f` confirmed.

**Reading 2 — PENDING** — the fixed pilot authorization is not answerable until the activation produces a result for review. It will be recorded verbatim separately when supplied.
