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
  reports the C12 boundary measurements (M-S2-1…7). Started as the S0.1 **first measurement** of
  AC-R-2.11.4-9; ticket **S0.3** extended it to the full S2 set (per-run split-vs-native overhead,
  serialize+validate, version-mismatch refusal + resume, hash-equality).
- `s1-kernel-spike/` — ticket **S0.3** throwaway S1 kernel slice: an append-only ledger (ADR-0027
  id model, durable-before-visible, hash chain, blob offload), one `project()` view with
  rebuild-equality, one sandboxed tool call through the `s1-helper` **binary** boundary, and the
  N=1/N=50 fan-out workload. Reports the C5/C7 measurements (M-S1-1…9) and the gates G1–G4
  (deterministic unit tests). Links only pure-std `hh-wire`.

Runner: `scripts/spike-s0.3-measurement-sheet.sh` runs both spikes **N=5** and prints the
aggregated distribution; the durable output is `docs/build/reports/S0.3-measurement-sheet.md` and
the append to the ADR-0050 amendment log (§10.6 OQ-131). The spike code is **committed-but-throwaway**
(ADR-0223/ADR-0220): outside the Cargo workspace, not a kernel dependency, deletable at the Stage-0
boundary.
