<!--
  Readout for docs/tickets/331_GATE-G36__p4-c.md (manifest 331, Round 7, gate G-P4-C).
  APPEND-ONLY: the `Status:` line is the only line that ever changes; signing APPENDS the
  Signature block and never edits or deletes anything above it.
-->
# GATE-G36 readout — G-P4-C: authorize the held-out confirmation window (baseline re-run + selected members)
Status: PENDING   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion (verbatim)

REQ-XD-221 (verbatim from the spec):

> | **REQ-XD-221** | Phase-4 spend SHALL be authorized at G-P4-X (X0 + X1) and G-P4-C (confirmation), recorded verbatim, outside the $60 pilot authorization. | Gate entries precede subjects. · `AC-8.7-2` · D |

REQ-XD-217 (verbatim from the spec):

> | **REQ-XD-217** | The confirmation pre-registration SHALL list the §8.6 members and SHALL be frozen and gated (G-P4-C) before any confirmation subject. | Hash precedes subjects. · `AC-8.6-3` · D |

REQ-XH-004 (verbatim from the spec):

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

This authorization is **outside the $60 pilot authorization** and outside the first-pass compute
accounting — a separately recorded model + compute authorization.

## Evidence (per item: source, `date -u` of the read, layer reached)

- Frozen confirmation pre-registration — `hh-p4-c-prereg/1` `docs/build/reports/2026-11-24-fp-p4c-prereg.json`
  sha256 `1d55d48a803c00848c9306c8d2bec4552f98031e52ec80eafa3d8c47ae7e722d` — read 2026-11-24 — `engineered`
  (committed record; minted before any confirmation subject — its `predates` member says so verbatim).
- Selection packet — `hh-p4-selection/1` `docs/build/reports/2026-11-24-fp-p4x-selection.json` sha256
  `a1df9b74b3165ad827b308d7b8814e53e6a6c3a68c43947ec1954de9584afc7e` (content hash
  `sha256:9c3c314c2e1db2872dae24c350f93a508c7e08de90c99658957d541ea3ccfe23`, disposition
  `selected{m=1}`) — same read — `engineered`. Predating proof: the c-prereg carries
  `selection_packet_sha256` `sha256:9c3c314c…` plus `hash_order` *"the selection packet's hash
  predates this record (REQ-XD-212; §8.7's order)"* — both committed in this diff, before any
  confirmation slot exists.
- Exploration packet — `hh-p4-x-exploration/1` `docs/build/reports/2026-11-24-fp-p4x-exploration.json`
  sha256 `dca30f7c4aa812436a5b98877c3e117e94f56bee984b1f024af1544ee08facfc` — same read — `engineered`
  (the offline, $0 fold of the committed X0/X1 records — the exploration estimates stay labelled).
- Comparative quote — `comparative-quote/1` `docs/build/reports/2026-11-24-fp-p4c-quote.json` sha256
  `6315e250810593c61bafafb090ca65e7762d6a5e1c0c334efa4db393ad1b196c`, quote id
  `eaaeda9f114936df83916f0c93e18138f020d6836bc2d4b0c043fb59751405ed` — same read — `engineered`
  (the one quote path: `fp_p4c_quote` → `hharness_eval::comparative_quote`; **X1-measured** per-level
  $/trial, `pre-quote` envelope — the first measured model figures, never the §10.4 declared priors).
- Quote inputs record — `hh-p4-c-quote-inputs/1` `docs/build/reports/2026-11-24-fp-p4c-quote-inputs.json`
  sha256 `f82877f5446cbf352e4708061b68eb53ce560a85eb0a6c9e129b2b5fccf6dd5a` — same read — `engineered`.
- Compute quote — `hh-compute-quote/1` `docs/build/reports/2026-11-24-fp-p4c-compute-quote.json` sha256
  `39706cce93c97752c2e0d6d75993797ad02088fce1645fec4a3a69bee42bec33`, `pq-3a4187759b2d8411`, ceiling
  12,229,780 µ$ — same read — `engineered` (receipt_lib `quote_doc`, the one §3.10 path; declaration
  `scripts/gce-backend/declarations/fp-p4c.json` validates; `verbatim_sha256` stays zeroed until signature).
- X1 measured cost surface — `docs/build/reports/2026-11-24-fp-p4x1-wal-slot-costs.json` (the sealed
  session WAL's authoritative slot-cost attribution) and `2026-11-24-fp-p4x1-slot-matrix.jsonl`
  (measured per-slot wall) + `2026-11-24-fp-p4x1-slots-lost.json` (46 `detail_lost` ids — each ran,
  attributed zero calls, carried 0 µ$) — same read — `engineered`/`recorded` (committed X1 evidence).
- Guard — `HOLD — gate GATE-G36 pending` (the expected state at presentation) — `engineered`.

<!-- agent-drafted:begin sha256=5b36f762d5980b974fbd29ac881c2525915a5faecdf67fd3944c4d8fc90677f8 -->
## Decision content for the operator

**The scope.** GATE-G36 is the G-P4-C spend gate for R7.5's held-out confirmation window — the only
place phase-4 confirmation spend can be authorized (REQ-XD-221; outside the $60 pilot authorization).
The confirmation body stays a skeleton until this gate records PASSED. Nothing in this readout
authorizes spend; a `run` disposition authorizes the declared envelope verbatim only.

**1 · The selection outcome (what §8.5's frozen rule produced).** The deterministic packet
`hh-p4-selection/1` — content hash `sha256:9c3c314c2e1db2872dae24c350f93a508c7e08de90c99658957d541ea3ccfe23`
— reads **`selected{m=1}`**: **V1 only** (the `low` effort lane). V1's exploratory estimate is
Δ̂ +45,455 ppm and every gate (a)–(f) passed: declared-delta + liveness; intended = served on 100 %
of receipts; gate (c) n/a for an always-active knob (measured 1.0 anyway); 5/55 unresolved within the
10 % ceiling; no integrity veto; mean $/trial within 2× baseline. No other level qualified —
V2/V5/V6 fail gate (d) unresolved (6–7/55 > 10 %), V3/V4/V7 fail (c) `unmeasured{evidence_absent}`
plus (d); no second factor qualified so **no C₁₂ combination member fires**.

**2 · The frozen confirmation pre-registration (hash + member list verbatim — REQ-XD-217).**
`hh-p4-c-prereg/1` sha256 `1d55d48a803c00848c9306c8d2bec4552f98031e52ec80eafa3d8c47ae7e722d`,
`selection_packet_sha256` `sha256:9c3c314c…` (the packet's hash predates this record — §8.7's order;
this record's hash in turn is committed before GATE-G36 and before any confirmation subject).
Members verbatim:

- `baseline` — rank 0, *"control re-run — the confirmatory pairing arm"* (REQ-XD-215: **no
  L2 or phase-2 baseline data is reused**; the baseline re-runs inside this window).
- `V1` — rank 1, *"selected member"* — factor *"reasoning effort · declared lane parameter,
  proxy pin"*; `delta_ppm` 45,455; `exploration_label` *"selected from the same data; estimate
  is exploratory"* (the expected-optimism label ≈ +.03 to +.10 rides the record).

No C₁₂ member — single-factor selection.

**3 · The declared envelope (verbatim from the frozen record).**
- **Cohort** — `c` — the band-A held-out members of the frozen task-selection/1 record, **T = 37**,
  never run in X0/X1 (the A ∪ B census-of-54 fallback did not trigger).
- **k = 5**, **2 members × 37 tasks × 5 = 370 slots** (under the ~555-slot m = 2 ceiling §8.6
  plans for).
- **Schedule** — seeded `slot_schedule_randomization`, seed `p4-c-schedule-r7.4-2026-11-24` — the
  same shuffle rule the X1 envelope declared; seeded before any subject slot.
- **Contrast** — each member vs the baseline re-run, per-task paired deltas over the joint T = 37
  population; **multiplicity Holm over the member-vs-baseline family (α = .05)**; the claim cites
  the confirmatory estimate only — exploration estimates stay labelled.

**4 · The quote** — minted by the one quote path (`fp_p4c_quote` → `hharness_eval::comparative_quote`;
C-11), `comparative-quote/1` id `eaaeda9f114936df83916f0c93e18138f020d6836bc2d4b0c043fb59751405ed`,
record sha256 `6315e250810593c61bafafb090ca65e7762d6a5e1c0c334efa4db393ad1b196c`. Slot-level rows
at t = k = 1 (the R6.32/W2 precedent): one row per member, 185 priced slots each, **every model
figure X1-MEASURED from the session WAL's authoritative attribution over the full executed census
(captured ∪ detail_lost — a lost slot carried 0 µ$)** — the first measured per-level $/trial, never
the §10.4 declared planning priors:

- `p4c-baseline` — mean 46,165 µ$/slot, p90 163,525 µ$/slot; expected **8,540,525 µ$**, machinery
  ceiling **36,302,550 µ$**.
- `p4c-V1` — mean 17,934 µ$/slot, p90 45,275 µ$/slot; expected **3,317,790 µ$**, machinery
  ceiling **10,051,050 µ$**.
- Σ expected **11,858,315 µ$ ≈ $11.86**; Σ machinery ceiling **46,353,600 µ$ ≈ $46.35** —
  far under the §10.4 planning ceiling ≈ $340 + compute share (the measured rates land well
  below the declared priors; the priors stand as planning bounds, the quote prices measurements).
- Measured wall (C = 4): baseline ≈ 17.6 h, V1 ≈ 5.6 h — ≈ 23.2 h total inside the declared
  D = 36 h dead-man bound.
- The compute share is cited verbatim from `hh-compute-quote/1` `pq-3a4187759b2d8411` — ceiling
  **12,229,780 µ$ ≈ $12.23** (planning 10,091,780; bound 34,343,404).
- Model hard-cap formula (verbatim): `cap_h = n_trials_h × max(mean, p90)_h × 1.2` — baseline
  `185 × 163,525 × 1.2` = **36,302,550 µ$**; V1 `185 × 45,275 × 1.2` = **10,051,050 µ$**;
  Σ = **46,353,600 µ$ ≈ $46.35**.
- Compute hard-cap (verbatim, §3.10): `r·(D + 1h restart) + S·r_disk·h_r + 0.12·E_gib` =
  `314,140·(36+1) + 300·137·6 + 0.12·3` = `11,623,180 + 246,600 + 360,000` = **12,229,780 µ$**.

**5 · Authorizations (REQ-XH-004 — recorded verbatim and separately; the agent signs nothing):**
- **Model authorization (verbatim):** ____________________________________
  (outside the $60 pilot authorization; measured expected ≈ $11.86, machinery ceiling
  $46.35 — **not** covered by the standing ≤$50/day sign-off; a separate verbatim is required.)
- **Compute authorization (verbatim):** ____________________________________
  (12,229,780 µ$ ≈ $12.23 — under the standing ≤$50/day sign-off where it applies.)
- **Exclusivity acknowledgement (REQ-XH-013, verbatim):** ____________________
  ("No two paid collections SHALL run concurrently on the model account; the operator's own use
  of the account counts as overlap.")
- The declaration's `authorization.verbatim_sha256` stays zeroed at presentation — it is stamped
  at PROV-time authcheck from this readout's signed block (the R6.32a/W2 precedent).

**6 · Stop rules** — the §8.4 declared set verbatim: *"budget, infrastructure, integrity and
catalog identity only — no outcome-based stopping or level dropping (REQ-XD-210)"*.
**Exclusivity + kill discipline** per the established window posture: REQ-XH-013 exclusivity is
acknowledged above; the window carries a dead-man `STOP` at D = 36 h plus the in-guest shutdown
timer at D + 15 min (REQ-XB-209); slot admission stops at D − the declared cut-off ≈ 0.65 h
(REQ-XB-128); a dead-man stop types in-flight slots `inconclusive{backend_terminated}` and leaves
no settled slot unharvested; one pre-authorized ≤ 1 h harvest-and-teardown-only restart sits
inside the ceiling (REQ-XB-224); residual disk h_r ≤ 6 h is the teardown deadline; in-cell exec
kill-on-lapse per the task's declared deadline (process-group kill, `agent_timeout`/
`verifier_unavailable:*` typed, never a scored refusal); teardown is the nine-step leg with
`compute-open` verified empty before UPLOAD/SETTLE.

**7 · The `no_candidate` path** — not in play: the frozen rule selected one member (m = 1). Had it
selected none, the readout would present that state and authorize no subject envelope (the
pre-registered fallback is recorded for completeness: under `withheld{no_candidate}` the c-prereg's
member list is `[baseline]` alone and the gate presents the baseline-only envelope).

**8 · The disposition ladder (pre-registered — your verbatim answer):**
- `run` — `PASSED — run`: you authorize the declared envelope verbatim (baseline re-run + V1,
  370 slots, model + compute lines above).
- `not_run{…}` — `NOT PASSABLE` / `FAILED`: recorded with basis; the skeleton stays a skeleton.
- `re-scope{…}` — `PASSED — re-scope{…}`: a narrowed envelope recorded verbatim. The §6
  re-scope forms are carried for completeness: `re-scope{harden-again}` (a newly named defect
  class — none is in play), `re-scope{exclude-and-replace}` (only where A5 makes an exclusion
  legitimate — none declared), `re-scope{end-first-pass-early}` (a first-pass form — this is a
  later pass; not applicable).

**The window a `run` would authorize.** R7.5's held-out confirmation under the declared envelope:
e2-standard-8 / debian-12 / 300 GiB pd-balanced / ephemeral IPv4 / NAT off, D = 36 h, C = 4,
measured wall ≈ 23.2 h; trials 370 + retry 40 declared; work items `c_confirmation`, `harvest`,
`receipt_archive`, `teardown`; receipts under `docs/build/reports/compute/`; the fleshed body
carries verbatim slots, arms, seeds, ceilings, receipts paths, teardown + settle + archive legs.
Paid bodies stay skeletons until PASSED.

**DEFERRALS rule (named, not decided):** `deferrals-open.sh` carries DF-R7.1-2 OPEN — V3/V4/V5's
activation evidence stayed `unmeasured{evidence_absent}` in the X1 corpus (the honest reason they
failed gate (c) — never a fabricated pass); its named unblocker is a later pass or an
operator-directed probe window. DF-R7.3-1 (the fold-semantics ruling owed to a later ADR — the
`detail_lost` disposition taken in R7.4's fold) is recorded for the ruling, not a gate blocker.
Whether DF-R7.1-2 blocks this gate is the operator's call.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
<!-- Appended only when the operator has answered; an agent never writes the signature lines
on its own authority. The appended lines' shape is the _TEMPLATE.md set (decision verbatim,
GATE DECISIONS row, agent-draft confirmation covering the block sha256, signed-by) — the
literal placeholder lines stay out of this file so the supplement guard's signature-reuse
leg never reads the template as a verbatim. -->
