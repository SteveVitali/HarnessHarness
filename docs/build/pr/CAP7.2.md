# PR — CAP7.2: Round-7 closure — O-3 scope-out record, BL-50 dispositions, register reconciliation

Manifest 336 · capstone · records block · offline-only · **USD $0.00** · branch
`codex/harnessharness-r6.53` · depends on CAP7.1 (`29a8fac6`).

## What

Lands Round-7's records closure under GATE-G34/ADR-0454's conditional authority
(operator verbatim "O1 approve, O-2 i'm not sure, O-3 approved, yes fold in BL-50.
go" — cited, not re-signed):

1. **The O-3 scope-out record mints** — `docs/build/reports/round7/O3-scope-out.md`
   (+`.sha256`). CAP7.1's terminal state re-verified: record
   `764ec84f…` byte-identical (sidecar `shasum -c` OK), `determination: complete`
   (16 satisfied + 2 typed_terminal, 0 unsatisfied/pending/absent over the 18
   routed §8.x ids) → the conditional is satisfied. The record carries the routed
   -set remainder verbatim — **16 non-§8.x ids** (§7.8 REQ-XE-002, XE-010…XE-013,
   XE-128…XE-132; §5.5 REQ-XC-314, XC-316; §6.2/§6.7 REQ-XD-013, XD-052; §2.5
   REQ-XA-126; §3.8 REQ-XH-033) — each `permanently later/not-v1`; **0 §8.x
   additions**. The `not_effective{…}` branch does not exist — exactly one branch,
   matching the verified determination.
2. **BL-50's 40 carried rows dispositioned in the same record** — 23 closed on
   cited evidence (the 2026-11-06 BL-RECONCILE discharges verbatim); 17 rows'
   remaining legs take the O-3 disposition verbatim (`R5.47:withheld` ×12;
   `R5.54`/`P-layer` ×4; `R5.55`/`P-layer` ×1); **zero silently dropped; 0
   satisfied by the §8.x pass's own evidence** (honest attribution). BL-50's
   landing cell carries the dated note; `status` open→closed per ADR-0454. No
   Round-5 record rewritten — matrix verdicts stay PARTIAL verbatim.
3. **Register/lead-token reconciliation** — 11 stale `OPEN` lead tokens
   normalized per their dated DONE/discharged dispositions with the `(was: …)`
   convention: DF-CAP6.1-1…5, DF-R6.45l-1 (dated DONE via R6.51/PR #413 — rides
   `main`, not this chain — integration fact), DF-R6.50-1, DF-R6.53-1, DF-R7.1-1,
   DF-G35-1, DF-R7.3-3. `deferrals-open.sh` re-run: **19 → 8** — the true owed
   set (DF-R7.1-2, DF-R7.3-1/-2/-4, DF-R7.5-1/-2, DF-CAP7.1-1/-2). Revisit
   triggers `--check` in sync.
4. **Closure bookkeeping** — phase cumulative $68.828421 (PHASE_SPEND + HUMAN-H21's
   signed entries; p4c ≥24 h confirming read `pending{clock}` verbatim due
   ≥2026-11-27 ~07:26Z); BUILD_INDEX row 336; LEDGER `lastCompleted: CAP7.2` /
   `nextTicket: GATE-G37`; **`docs/build/readouts/GATE-G37.md` presented PENDING**
   with all six pre-registered elements (agent-drafted block sha256 `8a62849e…`;
   an agent signs nothing).

## Discrepancies rowed (never silently repaired)

- BL-49's malformed row (unquoted comma → 12 fields, `status "s"`) — normalized by
  quoting the landing cell verbatim; check-backlog now reads `accepted`.
- check-backlog round-5 matrix: 73 `home-closed` post-closure — expected O-3
  aftermath (checker reads raw PARTIAL verdicts; invariant amendment is
  guard-owner scope — surfaced at GATE-G37).
- `compute-open.sh` `p4 | p4c` — operator-side teardown verified zero billable
  resources; no `hh-compute-receipt/1` minted; recorded, never fabricated.
- `pending{clock}` + the site-publication pin — verbatim carries to GATE-G37.

## Verification

- `check-supplement-guard.sh --no-checker` — PASS `DISPATCH — CAP7.2` pre-write →
  PASS `HOLD — gate GATE-G37 pending — RETURN PASS` post-write (the expected
  gate-presentation posture).
- `deferrals-open.sh` 8 rows (true owed set); `--bl` empty; `bm-row.sh` per row.
- `gen-revisit-triggers.sh --check` in sync; `check-bm-tools.sh` OK; sidecars
  re-verified; `git diff` reviewed per BL-41 before commit.
- Not run: cargo suites/check-fp-* (records-only — no code changed); external
  check-build-memory.sh where available.
