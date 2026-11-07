# S4 protocol-deviation record — 2026-11-07 (AR-02 · REQ-XA-115)

Evidence class: recorded execution — internal record; not outside-verified.

This is a dated **protocol-deviation record**, never a protocol
amendment: a protocol cannot be pre-registered after its outcomes
(ruling S-16; Q-X25 recorded default). It enumerates each departure of
the executed S4 confirmation cohort from the frozen protocol with
counts recomputed records-in (equal to `p1_records.py` RC-4 over the
committed raw set), the verbatim per-epoch operator authorizations, and
the analysis consequence.

Frozen protocol: `central-protocol/1`,
`protocol_id 9edc5327f89e2d66f8003bedc2530bc3e7d37326aad82cdfcab70465eda60a5f`
(seed `parity-tranche-2026-11-05:seed:v3`, never re-picked) — 30 tasks
× 2 arms × 5 rounds = 300 slots. Frozen reattempt bound (declared at
`PARITY-TRANCHE-3.md`): `infrastructure_failure` only, ≤ 2 per plan and
≤ 10 % of plans.

## Per-epoch authorizations (verbatim)

| Epoch | Packet | Verbatim operator authorization |
|---|---|---|
| confirmation (frozen run) | `2026-11-05-s4-confirmation.json` | GATE-G14 PASSED quote-only, verbatim "confirmed, please continue fully autonomously" (interactive, 2026-11-04T08:04:17Z); the frozen run executed under that continuation after parity verified (GATE-G14 dated note 2026-11-06) |
| egress | `2026-11-05-s4-egress-cohort.json` | "let's just fix that and run the full experiment with egress profiles" (operator chat, recorded verbatim in `runs/S4-EGRESS.md`) |
| s4-completion-gce | `2026-11-06-s4-completion.json` | "insert these into the build plan as new ticket(s), then continue driving orchestrate-build over them to the end" + "you can provision everything. just make sure we don't forget to deprovision at the end. proceed" (operator chat, received 2026-11-06; `runs/S4-COMPLETION.md`) |
| s4-30slot-repairs-gce | `2026-11-06-s4-30slot-repairs.json` | "let's do these" (operator chat, received 2026-11-06; `runs/S4-30SLOT-REPAIRS.md`) |

## Departures (recomputed counts)

1. **Re-attempts beyond the frozen bound.** Slot-record multiplicity
   across the four epochs: {1: 160, 2: 90, 3: 20, 4: 30} — 140 slots
   received more than one attempt (90 twice, 20 three times, 30 four
   times), where the frozen bound is ≤ 2 per plan and ≤ 10 % of plans.
2. **Round-0 receipts are not all first attempts.** 28 of the 60
   round-0 (`first`) slots' final receipts are later-epoch attempts.
3. **Two backends and architectures.** The final fold is 250
   arm64-applehv + 50 amd64-GCE slots.
4. **Two network modes.** The final fold is 160 `--network none` +
   140 bridge slots.
5. **Grading outside the HH machinery.** Verdicts were parsed by tb's
   `PytestParser` in scratch Python; 100 of 300 final receipts used the
   `vendored_offline` channel, bypassing run-tests.sh's bootstrap.
6. **Intended-vs-served policy divergence.** 100 served deviations
   across all epoch records — B-arm slots recorded
   `served_variant: hh/evict-oldest@1` no-op labels (60 in the final
   fold); the analysis basis is assignment (ITT), never served.
7. **One `protocol_id` across re-attempt epochs.** Tranche-1's record
   states a mass re-measure inside one `protocol_id` is not permitted;
   the S4 epochs kept `9edc5327` under the verbatim authorizations
   above.

## Analysis consequence

The frozen first-attempt subset is the **confirmation epoch's attempt-1
records** — never the 300/300 composite. The 300/300 figure is
presented as an **epoch-stratified supplement**: 160 + 90 + 20 + 30
executed slots across four separately authorized epochs, with the
residue chain 140 → 50 → 30 → 0 and first subjects 32 + 18 + 4 + 6 =
60/60. With the oracle participant the A/B compaction contrast is 0 by
construction and is not efficacy evidence.

## Repair register (`hh-protocol-deviations/1` `repairs[]`)

Structured per REQ-XA-008 (class ∈ {machinery-committed,
machinery-scratch, environment, admission}; code location with commit;
affected slots) — the same register carried by the JSON twin
`2026-11-07-s4-protocol-deviations.json`:

| Class | Repair | Code location (commit) | Affected slots |
|---|---|---|---|
| machinery-scratch | H-driver `localhost/` tag lookup (t1); healthcheck `float('5s')` (t1); amd64-base platform pin (t2); ANSI strip (t3) | `docs/build/reports/provenance/2026-11-06-scratch-drivers/files/parity-h-side.py` (archive commit `39fb3f23`) | tranche O↔H pairs (30) |
| machinery-scratch | `task_workdir` — the s4-cell `cd /app` default vs declared `WORKDIR` | `…/files/s4-cell/src/main.rs` (archive commit `39fb3f23`) | 10 repairs-epoch slots (`swe-bench-astropy-2`) |
| machinery-committed | `exec_backend_hello_clobber` + `probe_verb_enoent` machinery repairs (R5.43/R5.44; instrument-fold bind-id repair DF-CAP5.1-1) | `crates/hh-env/tests/r5_43.rs`, `crates/hh-env/src/useful.rs` (`ef35f501`) | the two S1 subjects |
| machinery-committed | `bridge` network cell variant + `declared_deps` admission (R5.56, ADR-0421) | `crates/hh-helper/src/` podman backend (`90fde635`) | 140 bridge slots |
| machinery-committed | `verifier_bound` admission 360 → 900 s (R5.57, ADR-0422) | `crates/hh-env/src/verifier_bound.rs` (`814b3d50`) | 10 astropy slots (completion + repairs) |
| environment | O-side `--n-concurrent 1`; prune + buildx-prune + diskwatch; larger GCE backend (OOM/ENOSPC lifted) | scratch drivers + operator record `provenance/…/extra/gce-backend.md` (`39fb3f23`) | tranche-3 pairs; 50 GCE slots |
| admission | `declared_deps` egress bridge (scratch variant in the egress epoch, committed from completion); `archive_mirror` (rewritten `FROM`); `vendored_mirror` (`COPY` + `url.insteadOf`) | `…/files/s4-egress-orch.py`, `…/files/s4-repairs-orch.py` (`39fb3f23`) | 140 egress + 20 repairs-epoch slots |

Companion JSON: `2026-11-07-s4-protocol-deviations.json`
(`hh-protocol-deviations/1`). The record cites the correction notes of
the same date (AR-03) and the edition ADR (AR-01, ADR-0423).
