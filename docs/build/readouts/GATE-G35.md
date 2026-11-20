<!--
  Readout for docs/tickets/328_GATE-G35__p4-x.md (manifest 328, Round 7, gate G-P4-X).
  APPEND-ONLY: the `Status:` line is the only line that ever changes; signing APPENDS the
  Signature block and never edits or deletes anything above it.
-->
# GATE-G35 readout — G-P4-X: authorize the phase-4 exploration window (X0 smoke + X1 OFAT sweep)
Status: PENDING   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion (verbatim)

REQ-XD-221 (verbatim from the spec):

> | **REQ-XD-221** | Phase-4 spend SHALL be authorized at G-P4-X (X0 + X1) and G-P4-C (confirmation), recorded verbatim, outside the $60 pilot authorization. | Gate entries precede subjects. · `AC-8.7-2` · D |

REQ-XH-004 (verbatim from the spec):

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

This authorization is **outside the $60 pilot authorization** and outside the first-pass compute
accounting — a separately recorded model + compute authorization.

## Evidence (per item: source, `date -u` of the read, layer reached)

- Frozen exploration pre-registration — `hh-p4-x-prereg/1` `docs/build/reports/2026-11-20-fp-p4x-prereg.json`
  sha256 `ec27a2789f4e1db9a58f76cc923992b76dda995934d8cc777a838fd9e071d6ba` — read 2026-11-20T21:30Z — `engineered` (committed record; minted before any subject).
- Frozen §8.5 selection rule — `p4-selection-rule/1` `docs/build/reports/2026-11-20-fp-p4x-selection-rule.json`
  sha256 `67009098eb187a2a99c6c83c9646c2ba9b20d58962a3f5eda9573ab1238d3b35` (member hash `ca70eaacf…`) — same read — `engineered`.
- Sweep registry — `sweep-registry/1` `docs/build/reports/2026-11-20-fp-p4-sweep-registry.json` frozen_sha256
  `aeb366b46536a66db7cca2180466b8166688704833e5647d37e17759571cdc92`, exploration identity
  `x2:sha256:aeb366b46536a66db7cca2180466b8166688704833e5647d37e17759571cdc92` — same read — `engineered`.
- Comparative quote — `comparative-quote/1` `docs/build/reports/2026-11-20-fp-p4x-quote.json` sha256
  `3749a447cb8bc075bdb2b987139f66b6b945f3bbf5c3d24ba3431f64f2a76b70`, quote id
  `e4b3ed49d4a5a247b4a5d9cf6f80db60bac9261e3ed8b371342bdd21acf264ac` — same read — `engineered` (the one quote path: `fp_p4x_quote` → `hharness_eval::comparative_quote`; declared planning priors, `pre-quote` envelope).
- Quote inputs record — `hh-p4-x-quote-inputs/1` `docs/build/reports/2026-11-20-fp-p4x-quote-inputs.json` sha256
  `d9b0f2fe2ff83f711c74beda79b7b75d513923ac4e41f7f563e74bb1bc1ef76b` — same read — `engineered`.
- Compute quote — `hh-compute-quote/1` `docs/build/reports/2026-11-20-fp-p4x-compute-quote.json` sha256
  `72b32ceb14422d1c018b9dcdfdac8efae7b6e2af24be841894924aa08c6b3734`, `pq-59a49747bfa5bdf0`, ceiling
  13,486,340 µ$ — same read — `engineered` (receipt_lib `quote_doc`, the one §3.10 path; declaration `scripts/gce-backend/declarations/fp-p4x.json` validates; `verbatim_sha256` stays zeroed until signature).
- G-L1 pass record — `hh-l1-conformance/1` `docs/build/reports/compute/2026-11-18-fp-ws1-l1-conformance.json`,
  pass id `l1-pass:sha256:80bfafee35ac9dd669d80f786ee54deca9948049bcd8559dd7a6dee8d06dd8ef`, result `pass`
  (recorded 2026-11-18T22:37:16Z), `h_identity.digest` `sha256:c451ed97d0206821bedd6e2255e5f6bba4736646d7170601b4085492e711a8b9`,
  corpus `sha256:072de1f36f3ddc6bd8387bfde992554e7c892b05f2e5d9590c30e221f028cb0d` — read 2026-11-20T21:30Z — `recorded` (live first-pass evidence).
- Recorded L2 verdict — the first-pass `hh-fp-l2/1` carries `withheld{window_deadline}:protected_pair_truncated{k<5,R<5}` (R6.48) — `engineered` (typed withhold, never an outcome).
- Guard — `HOLD — gate GATE-G35 pending` (the expected state) — `engineered`.

<!-- agent-drafted:begin sha256=d1008caf231618e353d5c17d755ef4073d06078a71105a693ebaece82dfe5b6c -->
## Decision content for the operator

**The scope.** GATE-G35 is the G-P4-X spend gate for R7.3's X0+X1 exploration window — the only
place phase-4 spend can be authorized (REQ-XD-221; outside the $60 pilot authorization). The
window bodies stay skeletons until this gate records PASSED. Nothing in this readout authorizes
spend; a `run` disposition authorizes the declared envelope verbatim only.

**1 · The frozen exploration pre-registration.** `hh-p4-x-prereg/1` sha256
`ec27a2789f4e1db9a58f76cc923992b76dda995934d8cc777a838fd9e071d6ba` over the committed x2 registry —
exploration identity `x2:sha256:aeb366b46536a66db7cca2180466b8166688704833e5647d37e17759571cdc92`.
The W outcome verbatim: *"F(32768) < 0.25 on the committed corpus — both composite levels
withheld{inert_at_admissible_window}; no admissible W is fabricated"*. The withheld-level set:
**V8, V9** — `withheld{inert_at_admissible_window}` ("declared, never sealed — the level stays in
the registry with its forecast"). The frozen §8.5 selection rule sha256
`67009098eb187a2a99c6c83c9646c2ba9b20d58962a3f5eda9573ab1238d3b35` (member hash
`ca70eaacf4842a76a625c16ffb4063505ca89830048f7825e8b5ff9cccfcbfe7`): gates (a)–(f), Δ̂ > 0,
one level per factor, m_max = 2, exact 1/(|D_A|·k) tie band, `withheld{no_candidate}` publishable.

**2 · The declared envelope (verbatim from the frozen record).**
- **X0** — 3 dev tasks × 1 replicate × all arms = **24 slots** (`cobol-modernization`,
  `torch-pipeline-parallelism`, `extract-elf` — the first declared dev task of each band-A
  difficulty cell).
- **X1** — one-factor-at-a-time (base point plus single-level perturbations) on the 11 band-A
  development tasks at **k = 5**, seeded `slot_schedule_randomization` in **5 rounds**:
  `cobol-modernization`, `torch-pipeline-parallelism`, `polyglot-rust-c`, `cancel-async-tasks`,
  `extract-elf`, `modernize-scientific-stack`, `build-pmars`, `pytorch-model-cli`,
  `pytorch-model-recovery`, `openssl-selfsigned-cert`, `sqlite-with-gcov`.
- **550 nominal X1 slots; 440 executable** — the W-rule withhold removes V8/V9 from the
  executable set (the withheld levels never run).
- **The arm list** is the HH-native baseline plus every non-withheld registry level —
  8 arms: `baseline`, `V1`, `V2`, `V3`, `V4`, `V5`, `V6`, `V7` (baseline effort pin `high`).

**3 · The quote** — minted by the one quote path (`fp_p4x_quote` → `hharness_eval::comparative_quote`;
C-11 — no second quote routine exists), `comparative-quote/1` id
`e4b3ed49d4a5a247b4a5d9cf6f80db60bac9261e3ed8b371342bdd21acf264ac`, record sha256
`3749a447cb8bc075bdb2b987139f66b6b945f3bbf5c3d24ba3431f64f2a76b70`. The §10.4 phase-4 planning
bounds recorded verbatim: X0+X1 model ≈ **$28–$187** (the §8.4 nominal 550-slot X1 batch at
$0.05–$0.34 per trial), planning ceiling ≈ **$337** (nominal); the P4 §10.4 row carries compute
≈ $17–26 across X0+X1+confirm. On the **executable** envelope (464 slots: V8/V9 withheld): expected
model spend ≈ $23.20–$157.76 across the same three points; the packet prices the declared top
point $0.34/slot — X0 expected 8,160,000 µ$, X1 expected 149,600,000 µ$, machinery ceiling
14,688,000 + 269,280,000 = 283,968,000 µ$ (≈ $283.97). Every model figure is a **declared planning
prior, never a measurement** — the measure_refs and `hh-p4-x-quote-inputs/1` say so verbatim. The
compute share is cited verbatim from `hh-compute-quote/1` `pq-59a49747bfa5bdf0` — ceiling
**13,486,340 µ$ ≈ $13.49** (planning 12,360,569; bound 42,497,955).
- Model hard-cap formula (verbatim): `cap_h = slots_h × p̂_h` — per level `58 × 340,000` =
  **19,720,000 µ$**; Σ over the 8 levels = **157,760,000 µ$ ≈ $157.76**.
- Compute hard-cap (verbatim, §3.10): `r·(D + 1h restart) + S·r_disk·h_r + 0.12·E_gib` =
  `314,140·(40+1) + 300·137·6 + 0.12·3` = `12,879,740 + 246,600 + 360,000` = **13,486,340 µ$**.

**4 · Authorizations (REQ-XH-004 — recorded verbatim and separately; the agent signs nothing):**
- **Model authorization (verbatim):** ____________________________________
  (outside the $60 pilot authorization; the declared top-point expected ≈ $157.76 is **not**
  covered by the standing ≤$50/day sign-off — a separate verbatim is required.)
- **Compute authorization (verbatim):** ____________________________________
  (13,486,340 µ$ ≈ $13.49 — under the standing ≤$50/day sign-off where it applies.)
- **Exclusivity acknowledgement (REQ-XH-013, verbatim):** ____________________
  ("No two paid collections SHALL run concurrently on the model account; the operator's own use
  of the account counts as overlap.")
- The declaration's `authorization.verbatim_sha256` stays zeroed at presentation — it is stamped
  at PROV-time authcheck from this readout's signed block (the R6.32a/W2 precedent).

**5 · G-L1 evidence — an honest open finding, recorded verbatim:**
- The recorded G-L1 pass: `l1-pass:sha256:80bfafee35ac9dd669d80f786ee54deca9948049bcd8559dd7a6dee8d06dd8ef`,
  result `pass` (2026-11-18T22:37:16Z), bound to `h_identity.digest`
  `sha256:c451ed97d0206821bedd6e2255e5f6bba4736646d7170601b4085492e711a8b9` — the first-pass H set
  sealed at W3 (hh_kernel `acc91fd0…`, mini HIR `98934508…`, compiled_artifacts `44e7a309…`).
- The frozen pre-registration declares `baseline_ref` `sha256:88042f890bdedb91bcf865d34630499b87f17b05d53d3640a3deaa863a8a0844`
  — a declared pass-2 **fixture** baseline ("the production mini HIR document is E2-T9's unlanded
  Stage 2"); its own label promises *"the exact G-L1 pass id is cited at GATE-G35"*.
- **No committed `l1-pass` record binds `sha256:88042f…`.** The only other conformance record
  (`l1-pass:sha256:c039f01a…`, the Ws1r refold) carries an empty `h_identity.digest`. REQ-XD-220's
  precondition — "a G-L1 pass for the exact H swept" — is **not evidenced by committed records**
  as they stand. This is recorded as a gate-blocking finding for your disposition, never
  reconciled silently: if the declared baseline identity is intended to stand in for the frozen
  first-pass H, that binding needs its own evidence or ruling; an L1 conformance leg over the
  declared phase-4 H (or a cohort-corrected baseline binding) precedes a `run`.
- REQ-XD-220's other legs hold verbatim: exploration **follows the recorded L2 verdict** (the
  first-pass L2 verdict is the typed withhold `protected_pair_truncated{k<5,R<5}` — recorded,
  never an outcome) and **never runs concurrently with L2**; **any H change starts a new
  phase-4 cohort**.

**6 · Stop rules** — the §8.4 declared set verbatim: *"budget, infrastructure, integrity and
catalog identity only — no outcome-based stopping or level dropping (REQ-XD-210)"*, details as
frozen: budget = the proxy root cap / the declared window ceiling; infrastructure = repeated
infrastructure failures at the declared bounds, reattempts within §6.8's bounds; integrity = a
confirmed answer-key or tamper veto stops everything at once; catalog identity = a change
between exploration and confirmation is recorded, a mid-batch change stops the batch.
**Exclusivity + kill discipline** per the established window posture: REQ-XH-013 exclusivity is
acknowledged above; the window carries a dead-man `STOP` at D = 40 h plus the in-guest shutdown
timer at D + 15 min (REQ-XB-209); slot admission stops at D − the declared cut-off ≈ 0.65 h
(REQ-XB-128); a dead-man stop types in-flight slots `inconclusive{backend_terminated}` and leaves
no settled slot unharvested (incremental harvest to a verified manifest); one pre-authorized
≤ 1 h harvest-and-teardown-only restart sits inside the ceiling (REQ-XB-224); residual disk
h_r ≤ 6 h is the teardown deadline; in-cell exec kill-on-lapse per the task's declared deadline
(process-group kill, `agent_timeout`/`verifier_unavailable:*` typed, never a scored refusal);
teardown is the nine-step leg with `compute-open` verified empty before UPLOAD/SETTLE.

**7 · The disposition ladder (pre-registered — your verbatim answer):**
- `run` — `PASSED — run`: you authorize the declared envelope verbatim (model + compute lines
  above; the G-L1 finding at (5) resolved by your ruling's stated basis).
- `not_run{…}` — `NOT PASSABLE` / `FAILED`: recorded with basis; the skeleton stays a skeleton.
  The honest named basis as committed evidence stands: `not_run{gl1_h_identity_unmatched}` —
  the recorded G-L1 pass does not bind the declared baseline identity `sha256:88042f…`.
- `re-scope{…}` — `PASSED — re-scope{…}`: a narrowed envelope recorded verbatim (for example
  `re-scope{x0-only}` — X0 smoke authorized, X1 deferred to its own gate presentation). The §6
  re-scope forms are carried for completeness: `re-scope{harden-again}` (a newly named defect
  class — none is in play), `re-scope{exclude-and-replace}` (only where A5 makes an exclusion
  legitimate — none declared), `re-scope{end-first-pass-early}` (a first-pass form — this is a
  later pass; not applicable).

**The window a `run` would authorize.** R7.3's X0+X1 execution under the declared envelope:
e2-standard-8 / debian-12 / 300 GiB pd-balanced / ephemeral IPv4 / NAT off, D = 40 h, C = 4,
declared planning wall ≈ 33.5 h at 1,040 s/slot; trials 464 + retry 46 declared; work items
`h_level_layers_build`, `x0_smoke`, `x1_ofat`, `harvest`, `receipt_archive`, `teardown`; receipts
under `docs/build/reports/compute/`; the fleshed body carries verbatim slots, arms, seeds,
ceilings, receipts paths, teardown + settle + archive legs (the R6.45x skeleton→flesh pattern).
Paid bodies stay skeletons until PASSED.

**DEFERRALS rule (named, not decided):** `deferrals-open.sh` carries DF-R7.1-2 OPEN — the
V3/V4/V5 activation-fold evidence needs a metric-bearing corpus (the committed prereg honestly
carries `unmeasured{evidence_absent}` forecasts); its named unblocker is a later pass or an
operator-directed probe window. Older OPEN rows (DF-R6.45l-1, DF-R6.50-1, DF-CAP6.1-1…5,
DF-R6.53-1) are outside this gate's scope. Whether DF-R7.1-2 blocks this gate is the operator's
call — the forecasts are honestly typed, never fabricated.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
<!-- Appended only when the operator has answered; an agent never writes these lines on its own authority:
Operator decision (verbatim, received <date -u +%FT%TZ> via <channel>): "<exact words>"
GATE DECISIONS row: <date -u +%FT%TZ> | GATE-G35
Operator confirmation of agent-drafted text (verbatim, <date -u +%FT%TZ>): "<words>" — covers sha256:<first 12>
Signed by: the operator. Recorded by <harness/model>, which does not sign.
-->
