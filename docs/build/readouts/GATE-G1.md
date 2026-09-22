# GATE-G1 readout — Stage-0 acceptance & ecosystem-decision revalidation

*Append-only. Each operator reading is a new dated block; never edit an earlier one.*

## Criterion (verbatim from the spec / gate marker)
S1/S2 gates pass and the revalidation rule is evaluated: any C5/C7/C12 score outside the assumed ±1 band, or a
gate failure on the winning candidate, re-runs ADR-0009 steps 5–7 (§10.6; OQ-131).

---

## 2026-09-15 — Reading 1: offline half (S0.3) → PENDING; operator authorized an online spike

**Evidence sources:** `docs/build/reports/S0.3-measurement-sheet.md`, ADR-0223 (spike method + deviations),
ADR-0224 (revalidation verdict), the ADR-0050 amendment log (Stage-0 spike block), `runs/S0.3.md`, `DEFERRALS.md`.

### Pre-registered thresholds — MET on the offline (winning-candidate) half
- S1 gates **G1–G4 pass**; **M-S2-6 pass**; **M-S2-7 = 100%** (hash-equality). (N=5 repeat-scored.)
- Derived **C5 = 4, C7 = 4/5, C12** (transport) all within the assumed **±1** band vs ADR-0050 §3.
- **Ecosystem decision (ADR-0050) HOLDS** — no revalidation trigger fires on the winning candidate; ADR-0009
  steps 5–7 NOT re-run. C5/C7 `u`-marks cleared for the E1 kernel; C12 `u`-mark retained (re-scored at Stage 3).

### Honesty flags surfaced (not fabricated green)
1. **C5 basis.** C5 = 4 is scored from CPU-normalized fan-out (1.58× core-norm) + cancel + tail. The raw durable
   fan-out (~20×) / throughput are disclosed as **fsync-serialization / 12-core host artifacts**, not the
   ecosystem mechanism; a strict durable reading would mechanically arm the trigger and is documented and rejected
   as a host artifact (ADR-0223/0224). The operator accepts this interpretation as part of the disposition below.
2. **Cross-candidate cells were offline-unreachable** and remain OPEN, scoped to this phase (DEFERRALS rule 4 →
   the gate does not auto-pass on the offline half alone):
   - **DF-S0.3-1** — MCP/ACP official-SDK round-trip (M-S1-7) + cross-candidate G1 byte-identity (E2/E3).
   - **DF-S0.3-3** — E2/E3 cross-candidate scoring + E5b/E5c splits + the R2 cross-camp reviewer signature.
   Compensating control on the offline half: the winning candidate (E1/E5a) is fully measured; ADR-0050 §4
   adversarial + kernel-family sweep survives 0.979 of corners.

### Operator disposition — **PENDING (not yet PASSED)**
The operator ruled: **authorize an online spike budget first** to close the machine-achievable cells of
DF-S0.3-1 / DF-S0.3-3 before Stage 1. Recorded in `LEDGER.md` GATE DECISIONS (2026-09-15).
- Inserted ticket **S0.3b** (`003a_S0.3b__cross-candidate-online-spike.md`) runs the online cross-candidate spike.
- Machine limit acknowledged: the **R2 human cross-camp reviewer signature** cannot be produced autonomously; it
  stays an OPEN residual of DF-S0.3-3 for the operator to arrange.
- **GATE-G1 remains STOPPED**; a second reading is written here after S0.3b lands and the operator re-reads the
  extended sheet + verdict, at which point the gate is dispositioned PASSED | SKIPPED-BY-OPERATOR | NOT PASSABLE.
