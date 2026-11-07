# Round 6 experiments first pass — contract invariants

The common Load/operating-clause surface for every Round-6 contract. It cites the
[ratified experiments supplement](../../experiments-spec.md) (canonical v0.3, Q-X20) and its
[frozen decomposition hand-off](../../experiments-decomposition-prompt.md); it does not replace
them. Requirement semantics live in the spec; operator decisions live verbatim in the frozen
[research ledger](../../experiments-research-ledger.md) §Q. The round-5
[contract invariants](2026-10-30_evidence-invariants.md) I-01…I-08 / O-01…O-08 continue to apply
unchanged wherever a Round-6 row touches round-5 machinery; the clauses below add what this round
needs. A Round-6 contract cites this file by clause ID instead of restating it.

## Authority and acceptance

1. **X-01 Source and identity.** The behavioral contract (`spec/CANONICAL_SPEC.md`), the ratified
   evidence-and-adoption supplement and the experiments supplement are all frozen. A changed behavior
   or method goes through its ADR/amendment row of the experiments spec §10.5 register before the
   code that depends on it; until admitted, the affected behavior refuses typed. Requirement IDs
   `REQ-X[A-H]-[0-9]+` are stamped verbatim; aliases keep their literal predicates at the target.
2. **X-02 Spend ceiling.** The first pass never exceeds $100 in total (model at token rates +
   compute + other; Q-X47, Q-X50), split model ≤ $70 / compute ≤ $20 / $10 unauthorized reserve
   (Q-X54); purchased Ollama credit counts in full (Q-X63). No contract authorizes spend; spend is
   authorized only by a recorded gate readout quoting the amount verbatim (REQ-XH-004). Enforcement is
   mechanical (proxy root cap, per-slot slices, GCE dead-man ceilings), never advisory.
3. **X-03 Skeletons and gates.** Every window run body (W1–W4, Wc) is a skeleton until its gate
   passes; a scoped `decompose-spec` insert fleshes it (Q-X58). No gate is pre-answered; a pending
   gate is RETURN PASS; a failed technical prerequisite is `blockedOn`. HUMAN rows name owner,
   trigger, exit and withheld claims.
4. **X-04 Durable before visible.** Every model call is receipted durably by the proxy before any
   response byte reaches a cell; slot records, spend ledgers and cost records fold durable records
   and never become a second authoritative store (CC1). New event classes are registered once in
   `crates/hh-ledger/src/classes.rs` (CC10).
5. **X-05 Secrets.** The Ollama key lives only in the proxy process on the phase VM (Q-X37); cells
   receive per-slot proxy tokens; the Hugging Face token never reaches a VM; no secret enters any
   record, receipt, index or log (`provided: yes/no` only); the archive scan and the remediation path
   of spec §4.10 apply.
6. **X-06 Network.** Agent phase: model proxy + apt/PyPI/npm/crates only — no `github.com`; grading
   may reach its verifier's hosts; every egress attempt is logged per slot (Q-X30, Q-X39a, Q-X55).
7. **X-07 Methods and outcome masking.** Pre-registration is hashed before the first paid subject
   (REQ-XH-111). Continuation rules read cost fields only; first-pass verdict/reward fields are
   sealed from gate readouts and summaries until the results row (REQ-XH-121), except the W1 re-proof
   verdicts the W1 gate needs. Admitted estimators only; an unavailable or refused inference is a
   result, never a failure to hide.
8. **X-08 Honest labels and claims.** Results carry their phase labels plus the first-pass label
   (REQ-XH-119); `parity: none` where the spec says so; no favorable-effect acceptance criterion; no
   banned claims phrasing (`scripts/claims-lexicon.json` once landed; until then the
   `scripts/check-position.sh` list).

## Operating clauses

- **XO-01 Orientation.** Deferrals first (`bash scripts/deferrals-open.sh`), then this row's
  contract, its cited spec §§ (bounded reads; the spec is ~55k words — use `rg -n` + `sed -n`) and
  its dependency rows' run records. Reconfirm code anchors at execution.
- **XO-02 Dispatch guard.** Product dispatch requires `bash docs/build/tools/check-supplement-guard.sh`
  → `PASS` with `DISPATCH — <id>` before and after the dispatch boundary. Until the guard-extension
  row (REQ-XH-002) lands, the retained round-5 guard routes Round 6 by chain order only; the
  extension adds the REQ-X coverage register and Round-6 routing.
  Exception: R6.1 dispatches under GATE-G20's scoped bootstrap authorization (`--no-checker`
  ROUTE plus the stock validator at 0 violations, GATE-G8 precedent); from R6.2 on, `PASS` with
  the stock checker is required.
- **XO-03 Worker sizing.** The implementation worker is Cognition SWE-2 (High, 256k context; Q-X51).
  A contract's `Size budget` is its estimate; a measured working set above ~100–120k tokens
  (including implement-spec's review/gap/verify passes) stops at the next boundary for a scoped
  re-plan (OL-16) — never silently continued.
- **XO-04 Stacking.** One ticket per fresh context; `base_branch: current checkout`; the chain is
  stacked from `main` (`chainTip: main` at the start of Round 6); only the operator merges.
- **XO-05 Verification.** Every new behavior has a regression that fails without it; `cargo fmt
  --all --check` and scoped package tests for code; `bash scripts/check-drift.sh` after any
  schema/codegen change (CC7); live legs only through `scripts/live-verify.sh` (ADR-0363) and only
  under a passed gate.
- **XO-06 Records.** Run ledger `docs/build/runs/<ID>.md`, PR body `docs/build/pr/<ID>.md`,
  `BUILD_INDEX.md` row and PHASE LOG entry agree at true completion; build-memory files are written
  with shell/python and verified with `git diff` (root AGENTS gotcha 4).
