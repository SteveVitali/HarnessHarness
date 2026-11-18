# GATE-G28o — PROV-Ws1r: authorize the L1.1/L1.2 re-proof window's compute (the gate-A L1 legs re-run under the repaired machinery at `a70efad7` — the mswea profile-layer pack + control-driver ordering repair — folding the durable L1.0 stores verbatim; replay-mode model, zero metered spend) · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket
authorizes spend. Ws1r is a **re-proof window** — the Ws1 L1.1/L1.2 legs re-executed under the machinery
that repairs the recorded turn-1 divergence (`hh-l1-conformance/1` `result: fail`, `parity: none`, 2026-11-18
— authored HIR lacked O's mswea profile layer: 1,944 B `hh_submit` prompt vs O's 6,152 B `bash` prompt).
The offline drive at the repair tip traversed the corpus 94/94 turns byte-identical; this window is the
declared-environment proof of the same legs.

## Evidence (per item: source, `date -u` of the read, layer reached)

- The recorded failure — `l1-out/fold/hh-l1-conformance.json` (harvest `run-root-ws1`): L1.1 strict
  4 divergences, L1.2 lenient 1 gated + 7 reported, `s: 1` uniform, `result: fail` — the structural
  HIR gap, not a task-provability verdict — read 2026-11-19 — layer: retained harvest.
- The repair — commit `551c5d76` (309w Reading-2 L1 repair set) + clippy-hygiene `a70efad7`:
  mswea pack (`hh.ws1.mswea-mini@1` profile + template/bash nodes + `model_surface:"compiled"`),
  owner-partitioned pending intents, stale-completion handling, first-submit `FollowUp`
  suppression, GLM transcript render (`index` + `provider_specific_fields` + dual-placement
  reasoning), `KernelSink` writer-lease renewal, `SealedDefinition` idempotent re-registration —
  read 2026-11-19 — layer: committed code.
- The offline re-proof — run `run-1795006969837-000048-1934b708`: 94/94 turns byte-identical to
  O's recorded upstream requests, 96/96 effects, 3 `lifecycle.lease.renewed`, honest
  `infrastructure_failure` at corpus exhaustion — `docs/build/runs/R6.45q.md` addendum — read
  2026-11-19 — layer: committed records.
- The frozen inputs — `h-set.tar` `hir` member sha256 `98934508b393c6e9d4a2175714f09842abfdbed74e078ba52152c5d8ff3cdb63`
  byte-identical to `hir-bound.json.bound.json`'s `source_sha256`; 28 `hh.model_proxy.recording/1`
  corpus recordings + 8 extracted raws + 16 durable `l10-*` leg stores retained in
  `hh-retained/ws1/harvest/` — read 2026-11-19 — layer: retained artifacts.
- The declared data — `hh-l1-canon` C01–C07; gate-A partition `4fe2dd2d…` (sqlite-with-gcov +
  cobol-modernization, 8 trajectories); mswea pack dir `crates/hh-compiler/tests/fixtures/r6_35/` —
  read 2026-11-19 — layer: committed records.
- The machinery — pinned `a70efad70d00e4729c98b45c2b17fdcf528fe019` (the repair tip — the runner
  MUST be built in-window at this commit; the historical in-window runner `ddbfe9ef…` predates
  the pack) — read 2026-11-19 — layer: committed code.
- The declaration + quote — `scripts/gce-backend/declarations/fp-ws1r.json` minted (window
  `ws1r`, ticket `R6.45q`, gate `PROV-Ws1r`, `runner.commit a70efad7…`); quote →
  **`pq-83d07b6cdfc6a9bb`, bound 1,169,365 µ$ (~$1.17), ceiling 2,212,120 µ$ (~$2.21)** — hard-cap
  `r·(D + 1 h restart) + S·r_disk·h_r + 0.12·E_gib` = `314,140·7 + 120·137·4 + 120,000·1` =
  2,199,020 − no residual + = **2,212,120 µ$** — 8 gate-A trajectories × legs `l1_1,l1_2` only
  (`--legs` narrows; l10 legs fold `reused{durable}` from the harvested stores), D 6 h /
  in-guest 375 min, h_r 4 h, C = 1, egress 1 GiB — read 2026-11-19 — layer: committed records.

## Decision content for the operator

**What this gate authorizes.** One L1 re-proof window, Ws1r — provision an `e2-standard-8`
Debian-12 VM in `hh-fp-a1b2c3` (IAP-only SSH, dead-man STOP at 6 h), build `hh-w1` +
`hh-model-proxy` + `hh_l1_drive` in-window at `a70efad7`, stage the retained corpus + durable
L1.0 stores + frozen H-set, run `ws1-l1-driver.py --legs l1_1,l1_2` over the 8 gate-A
trajectories with `--mswea-pack` injected into every `hh_l1_drive` invocation, fold
`hh-l1-conformance/1` + `hh-l1-diff/1`, harvest, tear down. `model_line: none` — both legs are
replay-mode at the model surface (`l1_2` is live **exec**, not live provider); no
`deliver-secret` required for the legs themselves. Ceiling **2,212,120 µ$ ~$2.21** compute-only.

**Honest shape notes.** Offline parity is machinery-verification evidence, not live-window
evidence — the `frozen`/`full` outcome is still earned only by this window's conformance
record. If L1 diverges, the records emit `fail` verbatim — nothing is rescued. If a machinery
defect surfaces mid-window, the run stops and records the typed row.

**Dispositions — your verbatim answer (the agent signs nothing).**
`run` — provision + execute Ws1r as specified under this readout's stamp onto `fp-ws1r.json`.
`not_run` — the re-proof stays unexecuted; `parity` remains `none` and `full` unreachable.
`re-scope{…}` — name the residual to repair or the scope to amend first.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decision (verbatim, received 2026-11-19 via interactive session): "ok, proceed as you
suggest" — approving the scoped recommendation that included this window ("recreating hh-fp-ws1
and running the battery … $5–20 GCE compute … Release?") and the earlier interactive decision
"authorize a live re-proof window".
Disposition: run; basis: GATE-G29 Reading 2's operator disposition "my disposition is to
diagnose and fix any failures and to iterate until success" — the repairs landed and the
declared-environment re-proof is the prescribed verification; compute-only, inside the
released $5–20 envelope (actual ceiling ~$2.21).
GATE DECISIONS row: 2026-11-19 | GATE-G28o (PROV-Ws1r)
Operator confirmation of agent-drafted text (verbatim, 2026-11-19): "ok, proceed as you suggest" — covers sha256:46ef84ec6d5641
verbatim_sha256 stamped onto fp-ws1r.json covers the directive sentence ("ok, proceed as you suggest") — sha256:46ef84ec6d56…
Signed by: the operator. Recorded by devin-cli/swe-2-high, which does not sign.
