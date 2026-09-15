<!--
  Template: docs/tickets/DEFERRALS.md (BM-DEFER-01). Seeded by build-memory init; rows
  appended by implement-spec (and closed by later runs). Append-only companion — NOT a
  chain ticket, never overwritten or deleted. The four rules below are stated verbatim.
-->
# Deferred obligations ledger

> **Maintained companion — NOT a generated ticket.** Committed and hand-maintained; preserved
> across any ticket regeneration; never overwritten or deleted. It has no `NN_` sequence prefix
> so it is not mistaken for a chain ticket.

The rules:
1. **Every run, first:** read this file. If the ticket you are about to implement — or a
   prerequisite it depends on — unblocks any `OPEN` row, **closing that row is part of your
   run**: verify it for real, then flip it to `DONE` with the date and evidence.
2. **Never delete a row.** Flip `OPEN` → `DONE` (verified) or `WONTFIX` (with a reason).
   History stays.
3. **When you defer something new,** append a row here in the same run that defers it. A
   deferral that is not in this file did not happen.
4. **Gates refuse to pass** while any `OPEN` row scoped to that phase remains. Treat an open
   row as gate-blocking.

Status values: `OPEN` (owed) · `PARTIAL` · `DONE` (verified — add date + evidence) ·
`WONTFIX` (add reason) · `ACCEPTED-SKELETON` (intentionally minimal for now; revisit at the
named ticket). Optional `kind`: V (verification) | F (functionality) | D (deviation) |
H (handoff seam) | P (human prerequisite) | X (other).

Pre-existing normalized debt is tracked in `docs/build/BACKLOG.csv`; a row here cites its
`BL-` id where one exists (no duplication).

| id | item | why deferred | unblocked by | how to verify | proxy now | status |
|---|---|---|---|---|---|---|
| DF-S0.1-1 (V) | AC-R-2.11.4-9 durable-frame delivery latency (p50/p95) and ephemeral-drop rate columns; append the comparative sheet to the ADR-0050 amendment log; set connection bounds (OQ-402) as MUST-data | The Group R event stream does not exist at Stage 0 (lands Stage 1), so there are no durable frames to time; the full comparative E5a/E5b/E5c spike is a throwaway ticket (S0.3, R1–R6). ADR-0220. | S0.3 (comparative spike) / Stage-1 Group R streaming | Re-run the S2 boundary spike with the Group R stream present; the sheet reports durable-frame p50/p95 and ephemeral-drop at fan-out; append to the ADR-0050 amendment log | Per-crossing overhead + codegen round-trip + hash-equality measured now: `docs/build/reports/S0.1-boundary-spike.md`; the deferred cells are typed `n/a{stage_0_no_stream}` | OPEN |
| DF-S0.1-2 (V) | Migrate `ContractIdentity.schema_hash` to a `ContentAddress` over the `idp/1` canonical schema export (§7.4; ADR-0036) | `idp/1` canonical form and `ContentAddress` land at S1.2; not available at Stage 0. Interim recorded in ADR-0219; inherited under ADR-0212 (Stage 0–1). | S1.2 (identity/versioning) | `schema_hash` derives from the `idp/1` canonical form; the pinned `schema.json`/`EXPECTED_SCHEMA_HASH` are regenerated; confirm no second canonicalization scheme remains (CC1) | Stage-0 content address `sha256:<hex>` over sorted-key compact JSON, NIST known-answer tested (`hh-wire::sha256`) | OPEN |
| DF-S0.2-1 (V) | AC-R-2.5.5-14 **helper-binary boundary measurement**: the S1 fan-out/footprint spike (C5/C7) — one sandboxed tool call *through a helper-binary boundary* — reported as numbers on the measurement sheet (M-S1-1…9). S0.2 owns and verifies the *in-process executor* boundary half; the helper-*binary* boundary + its measurement is the throwaway S0.3 spike (operator-gated). | The baseline is deliberately in-process (§9.1: "one in-process `tool_executor`"); the helper boundary and its fan-out/footprint numbers are the S0.3 measurement spike, whose `Live stage` is operator-gated (spike budget) and offline at S0.2. | S0.3 (S1/S2 measurement spikes) | Run the S0.3 S1 spike: one sandboxed tool call through a fresh throwaway helper binary; M-S1-1…9 within the pre-registered bands; append to the ADR-0050 amendment log; feeds GATE-G1 | The in-process `tool_executor` boundary is implemented and tested now: one sandboxed shell/fs call through the `local_host` `EnvHandle` (`isolation_class = none`), sandbox-escape denied, timeout-with-kill, output masked — `crates/hh-baseline/tests/end_to_end.rs::one_headless_coding_task_runs_end_to_end` and `crates/hh-baseline/src/tools.rs` unit tests | OPEN |
