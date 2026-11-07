# docs/build/reports/compute/ — `hh-compute-receipt/1` home

Committed receipts from `scripts/gce-backend.sh` (manifest 266/R6.13;
spec §3.9). One file per operation, named
`<YYYY-MM-DD>-<phase>-<window>-<kind>.json` where `kind ∈
provision | verify | prepull | harvest | teardown`.

- **Validation:** `bash scripts/check-compute-receipts.sh` (runs in CI on
  a `ci-mode: both` step — a docs-only head that drifts a receipt must
  not take the fast path green). The closed schema lives in
  `scripts/gce-backend/validate_receipt.py`.
- **Open windows:** `bash scripts/compute-open.sh` lists provision
  receipts with no matching teardown; empty output is required for
  phase/round closure (REQ-XB-211).
- **Gate spans:** `bash scripts/compute-gate-span.sh` (REQ-XH-009).
- **Secrets:** receipts record `provided: yes/no` only — never a value,
  credential, key, billing id, or token. Every receipt is leak-scanned
  before it is written; a scan hit refuses the write.
- **Cost:** teardown step 9 writes one `hh-compute-cost/1` per window
  (R6.14; `billed: pending`, intervals from lifecycle timestamps) and
  appends the compute estimate row to the phase spend ledger
  (`docs/build/reports/live/PHASE_SPEND.jsonl`, ADR-0432).
- Harvest trees land under `harvest/` (run-root tarball + manifest +
  serial + usage + journal) and are themselves leak-scanned.
