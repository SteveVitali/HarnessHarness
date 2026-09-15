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
| DF-S0.1-1 (V) | AC-R-2.11.4-9 durable-frame delivery latency (p50/p95) and ephemeral-drop rate columns; append the comparative sheet to the ADR-0050 amendment log; set connection bounds (OQ-402) as MUST-data | The Group R event stream does not exist at Stage 0 (lands Stage 1), so there are no durable frames to time; the full comparative E5a/E5b/E5c spike is a throwaway ticket (S0.3, R1–R6). ADR-0220. | S0.3 (comparative spike) / Stage-1 Group R streaming | Re-run the S2 boundary spike with the Group R stream present; the sheet reports durable-frame p50/p95 and ephemeral-drop at fan-out; append to the ADR-0050 amendment log | Per-crossing overhead + codegen round-trip + hash-equality measured now: `docs/build/reports/S0.1-boundary-spike.md`; the deferred cells are typed `n/a{stage_0_no_stream}` | **DONE** — 2026-09-15 (S0.3): durable-frame p50 = 5 µs / p95 = 23 µs (M-S1-9 subscribe tail at N=50), ephemeral-drop = 0; per-crossing/per-run connection bounds (M-S2-1/-2) set as OQ-402 MUST-data; comparative sheet appended to the ADR-0050 amendment log. Evidence: `docs/build/reports/S0.3-measurement-sheet.md`, ADR-0050 amendment log (Stage-0 spike), `spikes/s1-kernel-spike`. |
| DF-S0.1-2 (V) | Migrate `ContractIdentity.schema_hash` to a `ContentAddress` over the `idp/1` canonical schema export (§7.4; ADR-0036) | `idp/1` canonical form and `ContentAddress` land at S1.2; not available at Stage 0. Interim recorded in ADR-0219; inherited under ADR-0212 (Stage 0–1). | S1.2 (identity/versioning) | `schema_hash` derives from the `idp/1` canonical form; the pinned `schema.json`/`EXPECTED_SCHEMA_HASH` are regenerated; confirm no second canonicalization scheme remains (CC1) | Stage-0 content address `sha256:<hex>` over sorted-key compact JSON, NIST known-answer tested (`hh-wire::sha256`) | OPEN |
| DF-S0.2-1 (V) | AC-R-2.5.5-14 **helper-binary boundary measurement**: the S1 fan-out/footprint spike (C5/C7) — one sandboxed tool call *through a helper-binary boundary* — reported as numbers on the measurement sheet (M-S1-1…9). S0.2 owns and verifies the *in-process executor* boundary half; the helper-*binary* boundary + its measurement is the throwaway S0.3 spike (operator-gated). | The baseline is deliberately in-process (§9.1: "one in-process `tool_executor`"); the helper boundary and its fan-out/footprint numbers are the S0.3 measurement spike, whose `Live stage` is operator-gated (spike budget) and offline at S0.2. | S0.3 (S1/S2 measurement spikes) | Run the S0.3 S1 spike: one sandboxed tool call through a fresh throwaway helper binary; M-S1-1…9 within the pre-registered bands; append to the ADR-0050 amendment log; feeds GATE-G1 | The in-process `tool_executor` boundary is implemented and tested now: one sandboxed shell/fs call through the `local_host` `EnvHandle` (`isolation_class = none`), sandbox-escape denied, timeout-with-kill, output masked — `crates/hh-baseline/tests/end_to_end.rs::one_headless_coding_task_runs_end_to_end` and `crates/hh-baseline/src/tools.rs` unit tests | **DONE** — 2026-09-15 (S0.3): one sandboxed tool call through the throwaway `s1-helper` **binary** boundary measured — M-S1-6 helper round-trip median 1564 µs, boundary overhead median 0.08 ms; M-S1-1…9 reported with C5/C7 derived scores 4 (within the assumed ±1 band). Evidence: `docs/build/reports/S0.3-measurement-sheet.md` (M-S1-6), `spikes/s1-kernel-spike/src/bin/s1-helper.rs`. |
| DF-S0.3-1 (V) | M-S1-7 MCP call round-trip + ACP session setup (the candidate's official MCP/ACP SDKs); the **cross-candidate** half of gate G1 (E2/E3 produce a byte-identical hash chain for the shared corpus) | The official MCP/ACP SDKs (S-155/S-157) must be fetched over the network and the reference peers stood up; E2/E3 kernels need foreign toolchains — none runnable under the Stage-0 offline/hermetic budget. | S0.3 online spike budget (operator) / Stage-1 protocol edges (S1.25) | Re-run the S1 spike with the official SDKs + local reference `echo`/ACP peers; M-S1-7 within band; run the S1 slice in E2/E3 and confirm the hash chain is byte-identical across candidates | The stdio JSON-RPC 2.0 boundary (M-S2-2 per-sample overhead) as the protocol round-trip proxy; within-candidate G1 byte-determinism + pinned known-answer (`g1_hash_chain_is_deterministic`) | OPEN |
| DF-S0.3-2 (V) | C12 **polyglot** components: M-S2-5 two-toolchain clean-build+test CI, and the cross-ecosystem serialization multiplier, for the E5a/E5b/E5c splits — the components the assumed C12 = 2 penalty is *about* | Only the E1 toolchain is present offline; the E2 lab side (and E3 surface for E5c) cannot be built hermetically, so the polyglot CI + cross-ecosystem parse cost cannot be timed. | Stage 3 (kernel↔lab boundary first exercised with the E2 lab; §9.4) | Build both sides' clean CI and time it (≤1.5× single-toolchain → score 5); measure far-side canonical-parse+hash-verify in the E2 lab at 16/64 KiB; re-derive C12 and re-confirm trigger 5 in the ADR-0050 amendment log | Matched E1↔E1 transport measurement (per-run overhead ≈ 0.056 %, favorable); ADR-0050 §4 kernel-family robustness (0.979 of corners) | OPEN |
| DF-S0.3-3 (V) | S1 cross-candidate **scoring** (E2/E3 kernels), the S2 **E5b/E5c** splits, and the R2 **cross-camp review** signature on the sheet | E2/E3 kernel slices need foreign toolchains; the R2 reviewer briefed on a different candidate is a second-ecosystem human — none runnable under the Stage-0 offline/hermetic budget. | S0.3 online spike budget (operator) | Run each candidate's S1 slice + the E5b/E5c splits from the same spec, fresh executor per candidate; a cross-camp reviewer signs the sheet; compare each C5/C7/C12 to ADR-0050 §3 within ±1 | Winning candidate (E1/E5a) measured now; ADR-0050 §4 adversarial + kernel-family sweep already shows the kernel/lab outcome survives the worst case | OPEN |
