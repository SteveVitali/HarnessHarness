# Throwaway spike trees

Stage-0 spikes measure the three ecosystem criteria ADR-0050 scored from citable evidence
(C5 fan-out, C7 footprint, C12 boundary cost) and mark `unmeasured-by-spike` — the spec's
`spec/CANONICAL_SPEC.md` §10.6 acceptance check. **Rules R1–R6 (ADR-0050 / §10.6):** one spec,
fresh executors, throwaway trees, nothing merged into a subsystem, same host/inputs/wall-clock
budget, and the **measurement sheet is the only durable output** — the spike code itself is
disposable.

These trees are **outside the Cargo workspace** (`exclude = ["spikes"]`): they build against the
persistent kernel but are never a build dependency of it. Deleting `spikes/` never breaks the
kernel, CI or the drift check.

- `s2-boundary-spike/` — drives `hh-kernel serve` over stdio with the generated client and
  reports per-crossing overhead. Runner: `scripts/spike-s2-boundary.sh`. This is the S0.1
  **first measurement** of AC-R-2.11.4-9; the full comparative S1/S2 battery across the
  E5a/E5b/E5c splits (and the ADR-0050 amendment-log append) is ticket **S0.3**.
