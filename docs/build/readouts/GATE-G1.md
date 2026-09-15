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

---

## 2026-09-15 — Reading 2: online cross-candidate spike (S0.3b) → **PASSED**

**Evidence sources:** `docs/build/reports/S0.3-measurement-sheet.md` (S0.3b §1–§5), ADR-0225 (method + candidate→toolchain bindings + single-executor R1 deviation), ADR-0226 (revalidation disposition), `runs/S0.3b.md`, PR #5.

### Cross-candidate acceptance — MET on the substantive checks
- **Gate G1 byte-identity holds across E1/E2/E3** — E2 and E3, each with their own canonical serializer, independently reproduce the E1 reference head `f0b9620d0f061b777e341125780efbba6e117e7997a0b69e35679390afe3ab20` for the shared corpus `209d34b9…`; a live negative test proves the gate fires on a mutated corpus. Cross-ecosystem hash-equality M-S2-7 = 100% (10k events/split).
- **M-S1-7** MCP/ACP round-trips via official SDKs against local reference peers, sub-ms local — within the fast-boundary band.
- **Winning decision E5a (E1 kernel) re-confirmed** — its scores within ±1; gates pass.

### Revalidation trigger — ARMED, and consciously dispositioned
- Read literally, **trigger 5 arms**: the non-winning candidate **E2** scores outside the assumed ±1 band — C5 4→1 (kernel concurrency fan-out 12.2× core-norm), C7 2→5. E3 is within band.
- ADR-0226 analysis (not fabricated to "within band"): E2's out-of-band score is a **kernel** fan-out score, but **E2 is never the proposed kernel** — E5a assigns E1 the kernel and E2 the per-run/IO-bound lab role (where E2's fast start is favorable). The ADR-0009 steps-5–7 re-run therefore **re-confirms E5a rather than moving it**; ADR-0050 §4 kernel-family robustness = 0.979.

### Operator disposition — **PASSED** (2026-09-15)
The operator ruled **PASS**: accept ADR-0226 — the armed trigger sits on a non-kernel candidate and does not move the winning ecosystem decision; ADR-0009 steps 5–7 are treated as **satisfied-by-analysis** and recorded as an **accepted deviation**. Stage 1 (S1.*) is released.

**Carried-forward residual (accepted, to be re-surfaced at GATE-ACCEPT):** the **R2 cross-camp reviewer human signature** on the cross-candidate sheet remains OPEN in DF-S0.3-3 (a human deliverable the operator will arrange). Per DEFERRALS rule 4 this is a Stage-0-scoped OPEN row; the operator consciously accepted it as a tracked carry-forward rather than a blocker. No other Stage-0-scoped deferral remains open (DF-S0.3-2 is Stage-3-scoped; DF-S0.1-2 is S1.2-scoped).
