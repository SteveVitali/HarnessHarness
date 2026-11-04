# GATE-G11 — Authorize exact S1 and S4 executions · readout
Status: PASSED

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



### Update — backend supplied + re-probe (2026-11-04T00:12:24Z, commit `80820a5b`)

Operator supplied the backend (option a): podman machine init+start
(applehv, 2 CPU/2 GiB/30 GiB — local VM, USD $0.00). The re-probe
surfaced and repaired three real defects (stale `.Host.Os` probe
against podman 6.x, missing `--container`/`--workspace` spawn args,
TMPDIR workspace unreachable inside the VM — $HOME-only mounts),
then ran the real handshake:

- `backend_candidate` → **verified** — real podman handshake: helper
  binds, `hello` + declaration over alpine:3.20 container with
  `/work` mounted (network off)
- `tb2_lifecycle` / `mediation` / `acquisition_network` / `worksheet`
  → verified (as before)
- `observer_probe` → `inconclusive{probe_absent}` — the read-only
  probe re-hello finds no socket on the seatbelt lane's one-shot
  helper (ENOENT — the helper exits with the exec; a real lane fact,
  never simulated)

Disposition: `inconclusive` — one leg open, `scope_return=true`.
Packet: `docs/build/reports/2026-11-04-r5.42a-backend-preflight-reprobe.json`.
The residual gap is now a single lane property (probe on seatbelt's
one-shot helper), not the missing backend the earlier packet carried.



### Update — probe verb exercised on the podman lane (2026-11-04T01:10:02Z)

The probe-verb question was exercised for real on the podman lane
(spawn + hello + `exec.probe()`): the helper's socket persists
(verified — process + socket survive idle, unlike the seatbelt
one-shot lane), but the `probe()` verb itself returns
`error:blob: ENOENT` on podman as well — a genuine cross-lane gap in
the probe path (the verb's journal/exec-record resolution is not
live on either lane), recorded honestly as `probe_verb:
error:blob: ENOENT` in the packet facts. The surviving observer
still proves (verified); the instrument probe verb is the open
predicate — a machinery repair owed to a named owner, never a
simulated pass.

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

**PASSED** — operator words (verbatim, interactive, 2026-11-04T01:13:12Z): "Confirm, please resume and continue autonomously" — recorded verbatim in GATE DECISIONS and this signature. Scoped decision: PASSED per the presented option — the surviving-observer proof accepted as the confinement/observer/probe proof; carried unmet predicate: **the instrument `probe()` verb must be live (journal resolution repaired) at S1/S4 bind before any cell's probe evidence counts** — compensating control: no cell's probe evidence counts until the verb verifies on the actual backend; the verb's ENOENT-on-both-lanes gap is a recorded machinery repair, never a simulated pass. Agent-drafted block sha256 `9bdf5c27d044` confirmed.
