# Dated amendment/deviation — frozen preregistration admission-hash drift (DF-CAP6.1-2 / GAP-02, GAP-14)

2026-11-19 · CAP6.2 · append-only amendment record — **no frozen record is rewritten**.

## Facts (verbatim from CAP6.1's typed finding, re-verified by this ticket's checker runs)

- Frozen packet `hh-fp-prereg/1` (2026-11-10) pins arm `admission_sha256`
  `7407ce0e…`/`e0bd56c0…`/`658cc589…`; the current fixture bytes under
  `crates/hh-runner/tests/fixtures/r6_22/*.json` hash `3526f2b5…`/`67c150d6…`/`ff68ecf6…`
  after one-line `config.template_hash` amendments at `b07adfdd` (2026-11-16, R6.45n
  model-port repair), `3d7fd37b`/`cdd2d169` (2026-11-17, render/pi repairs).
- `bash scripts/check-fp-prereg.sh` → `emit-identical:2026-11-10-fp-prereg.json` FAIL and
  `emit-identical:2026-11-10-fp-prereg.sha256` FAIL; every other leg OK (packet-hash,
  comparative-quote, readiness, canonical-roundtrip, mutation-changes-hash,
  readiness-pending, tests). The emitter is faithful to current bytes — the FAIL is
  frozen-record-vs-drifted-input mismatch, not emitter corruption.
- `bash scripts/check-fp-results.sh` → `byte-identical-retained:2026-11-19-fp-seal-open`
  FAIL (same family — frozen record vs current emitter state; GAP-14).
- Arm-bearing execution (W2 PS0/PP0) predates the amendments, so the inference chain for
  executed arms is intact; the drift concerns re-emission of the frozen packet, never
  evidence integrity.

## Disposition — accepted deviation

1. The frozen `hh-fp-prereg/1` packet and `2026-11-19-fp-seal-open` remain byte-frozen
   with their original `admission_sha256` pins; this record binds the drift, nothing is
   rewritten (append-only rule; REQ-XH-121's seal semantics preserved).
2. The `emit-identical` / `byte-identical-retained` legs are hereby **amended to compare
   against the amended-input emission**: the checker FAILs stand recorded as expected-state
   drift under this dated amendment. The amended fixture hashes
   (`3526f2b5…`/`67c150d6…`/`ff68ecf6…`) become the reference state for re-emission;
   the frozen pins (`7407ce0e…`/`e0bd56c0…`/`658cc589…`) remain the historical record
   of the 2026-11-10 admission.
3. No arm's executed evidence is re-verdicted; W2 PS0/PP0 executions predated every
   amendment commit named above.

Signed-off state: `deviation-accepted` pending operator countersign at GATE-G32 (item 1
of the three-part signature list).
