# Round-7 gap analysis — pass terminal-state record (CAP7.1, manifest 335)

2026-11-26 · branch `codex/harnessharness-r6.53` · offline, USD $0.00 · canonical
record `2026-11-26-round7-gap-analysis.json` (+ `.sha256`); this file is the
readable companion carrying the same verdicts and sweep rows.

Routing basis: `docs/build/reports/round6/LATER_PASS_ROUTING.md` — 34 routed ids;
the 18 §8.x subset is this pass's. A routed id is **routed, never satisfied** in
Round 6 — its obligation transfers here. Ruling: GATE-G34 PASSED / ADR-0454
(operator verbatim "O1 approve, O-2 i'm not sure, O-3 approved, yes fold in
BL-50. go"). ADR-0454's "15" is the §8.3–§8.7 subset; the routed §8.x set is 18
(count reconciliation verbatim in
`docs/build/planning/2026-11-20_round7-decomposition.md`).

## Per-id verdict table (18 routed §8.x ids)

| id | § | verdict | cited evidence / verbatim typed outcome |
|---|---|---|---|
| REQ-XD-225 | §8.1 | satisfied | `hh-p4-admissibility/1` gen-3 `1ebab179…` (gen-1 `f999de0b…`) — tier-2 verdicts `admits{later_identity}` / `refused{dependency_not_landed|forecast_below_floor|no_forecast}`; `this_identity: never admitted` |
| REQ-XD-204 | §8.3 | satisfied | `hh-p4-x-exploration/1` `dca30f7c…` fidelity census — served `glm-5.3-flash` ×10,602 == 10,602 attributed receipts, declared-vs-served effort 0 mismatches; `hh-p4-admissibility/1` `1ebab179…` (`ineligible{mis_pinned}` machinery) |
| REQ-XD-205 | §8.3 | **typed_terminal** | `withheld{inert_at_admissible_window}` — `hh-p4-w-rule/1` gen-3 `98a2785a…`: F(32768)=0.22 < 0.25; V8/V9 declared, never sealed, never scheduled; W never fabricated |
| REQ-XD-206 | §8.4 | satisfied | `hh-p4-x-prereg/1` gen-3 `5db50be8…` — X0 24 + X1 440 OFAT over frozen registry `0816291e…`; executed `hh-fp-window/1` X0 24/24 `55af8a75…` + X1 slot matrix 440/440 `7ac1a07e…` |
| REQ-XD-207 | §8.4 | satisfied | `hh-p4-x-exploration/1` `dca30f7c…` exploratory-labelled throughout; `refused{confirmatory_label_on_exploration}` / `refused{missing_label}` machinery in prereg `5db50be8…` |
| REQ-XD-208 | §8.2 | satisfied (typed residual) | `hh-p4-x-exploration/1` `dca30f7c…` — measured V1/V2 1.0, V6 1.0; `unmeasured{evidence_absent}` V5 + conditional V3/V4/V7 (gate (c) fails verbatim in `a1df9b74…`); residual evidence debt rowed OPEN **DF-R7.1-2** |
| REQ-XD-209 | §8.2 | satisfied | `hh-p4-admissibility/1` `1ebab179…` + prereg `5db50be8…` — `withheld{inert_predicted}` retained with forecast; `unmeasured{evidence_absent}` honest, never a floor verdict |
| REQ-XD-210 | §8.4 | satisfied | stop set verbatim in prereg `5db50be8…` (budget/infrastructure/integrity/catalog_identity; `refused{stop_class|outcome_stop|level_drop}`); exercised live — `hh-fp-window/1` `e2f1950e…` `withheld{stop:budget}` ×222 |
| REQ-XD-211 | §8.5 | satisfied | `p4-selection-rule/1` `67009098…` byte-pinned member decoded + executed; drift → `refused{frozen_rule_mismatch}`; output `hh-p4-selection/1` `a1df9b74…` |
| REQ-XD-212 | §8.5 | satisfied | `hh-p4-selection/1` `a1df9b74…` (content `9c3c314c…`) — X1-only 9-input manifest, held-out/phase-2-3 excluded; hash predates c-prereg `1d55d48a…` that cites it |
| REQ-XD-213 | §8.5 | satisfied | `hh-p4-c-confirmation/1` `ebaf0b17…` — `selection_labelled_estimates` + `statement_exploratory` carry "selected from the same data" + "expected optimism ≈ +.03 to +.10"; costs descriptive-only |
| REQ-XD-214 | §8.5 | **typed_terminal** | `not_admitted{m_eq_1}` — C₁₂ pre-declared in the frozen rule; `selected{m=1}` mints no combination member; `check_combination` typed refusals committed (`a1df9b74…` + c-prereg `1d55d48a…` members baseline+V1 only) |
| REQ-XD-215 | §8.6 | satisfied | fresh in-window baseline arm — `hh-fp-window/1` r4 `5d1fc5c0…` baseline 185 slots; `ebaf0b17…` `baseline_label` verbatim "never L2/phase-2 reuse"; GATE-G36 no-baseline-reuse attestation |
| REQ-XD-216 | §8.6 | satisfied | `hh-p4-c-confirmation/1` `ebaf0b17…` — Holm family P4-C1 α=.05: `adjusted_p_ppm=1000000`, `rejected_at_alpha=false`, unavailable member retained at worst case |
| REQ-XD-217 | §8.6 | satisfied | `hh-p4-c-prereg/1` `1d55d48a…` committed 2026-11-24 before first confirmation subject (~20:41Z); GATE-G36 verified hash-before-subjects |
| REQ-XD-218 | §8.6 | satisfied | `comparative-quote/1` `eaaeda9f…` (`6315e250…`) X1-measured lines Σ 46,353,600 µ$ + compute `pq-3a4187759b2d8411` + amendment `pq-4e650fe71f863d22`; executed 35.9% model / 31.6%+40.4% compute |
| REQ-XD-219 | §8.6 | satisfied | `hh-p4-c-confirmation/1` `ebaf0b17…` — `inconclusive{unresolved_cell}` minted; equality/equivalence wording refused; `withheld{…}`/`not_executed{…}` typed; `earlier_attempts` carried verbatim |
| REQ-XD-221 | §8.7 | satisfied | gate order held — GATE-G35 PASSED `run` (5d67d360) + X1 amendment c98cfcdc (`pq-4263f129db914f88`) + GATE-G36 PASSED `run` (8793885e, `acba2551…`) + resume amendment 565f1324 (`pq-4e650fe71f863d22`); every spend leg post-gate |

**Counts:** 16 satisfied · 2 typed_terminal · 0 unsatisfied · 0 pending · 0
silently absent.

## Terminal-state determination (pre-registered form)

> **`complete`**

Every one of the 18 routed §8.x ids carries either a satisfied verdict on
committed evidence (16) or a spec-sanctioned typed terminal disposition (2:
REQ-XD-205 `withheld{inert_at_admissible_window}`; REQ-XD-214
`not_admitted{m_eq_1}`). Zero ids are unsatisfied, pending, or silently absent.
No routed id's verdict depends on a billed figure — the P4-C ≥24 h provider
confirming read stays `pending{clock}` verbatim as a settlement hygiene item
(HUMAN-H21), not a routed-id dependency.

**O-3 implication:** no §8.x routed member is left unsatisfied — CAP7.2's
conditional O-3 remainder set carries **no §8.x additions** on this record's
evidence.

**Carried debt (verbatim — typed, rowed, never silently absent):**

- **DF-R7.1-2 OPEN** — activation-fold evidence for conditional knobs V3/V4/V5
  `unmeasured{evidence_absent}`; unblocker verbatim: "a later pass whose retained
  run-root carries per-call `tokens.input.uncached` for every arm plus a
  metric-bearing activation record (or an operator-directed probe window under
  its own budget)".
- **DF-R7.3-1 OPEN** — refusal-bearing H drives cannot be gated by
  `compare_drives` (REQ-XD-104 fold semantics; an ADR is owed; the X1 46-slot
  `detail_lost` disposition and the confirmation fold's unresolved handling are
  material).
- **DF-R7.3-2 OPEN** — attempt-3 settlement residuals (h_r 20.97 h; ≲36,000 µ$
  harvest-boot interval; orphaned-provision lifecycle); HUMAN-H21 landed
  2026-11-26 with compute `unavailable` — residuals stand as recorded estimates
  (dated noting appended this pass).
- **DF-R7.3-4 OPEN** — harvest `rroot` derivation + empty-manifest verify gap;
  leg (b) fails-closed repair landed at R7.5 (`harvest_manifest_nonempty`); leg
  (a) outstanding (dated noting appended this pass).
- **DF-R7.5-1 OPEN** — driver/session artifacts not resume-safe (stale
  `proxy-session.json` → `withheld{admission:native_lane_hold}` ×222; hardcoded
  artifact names; writer-lease WouldBlock).
- **DF-R7.5-2 OPEN** — no per-container memory bound → host-global OOM →
  ens4/journald collateral (P4-C r3 17:39Z).
- **DF-CAP7.1-1 OPEN (minted this pass)** — `slot_result.cost_micros` snapshots
  undercount late-landing `measurement.cost.attributed` events; WAL-authoritative
  recompute is the honest figure and requires the retained store, which is not
  always in the harvest path.
- **DF-CAP7.1-2 OPEN (minted this pass)** — verify-cell podman container-name
  collision across resumes (`p4c-V1-model-extraction-relu-logits-r4` rc 125 →
  `recorded{reward_absent}` unresolved member).
- **`pending{clock}`** — P4-C ≥24 h post-teardown provider confirming read due
  ≥2026-11-27 ~07:26Z (HUMAN-H21 readout verbatim; demoted to a confirming
  check — teardown complete, no post-teardown usage possible).
- **site publication pin** — the §8.8 claims packet is not in
  `docs/results/site-data` (a `hh-site-publication/1` mint is a publication act
  outside this chain's authorization — R7.6 verbatim).

## DF-R6.45l-1 discharge verification

**Verified — the dated DONE stands.** Integration fact recorded; never owed
work.

- **Dated marker:** `docs/tickets/DEFERRALS.md` status-update row (2026-11-19):
  "DONE | derivation machinery landed + smoke1 retroactive acceptance exact
  (R6.51, PR #413 — pending merge in Phase A)".
- **Index row:** `docs/build/BUILD_INDEX.md` row 321a — "landed on its own
  branch — transitive verify-host derivation from captured runtime evidence
  emits exactly the three smoke1-observed hosts unaided; DF-R6.45l-1 DONE".
- **Landed artifacts verified** on PR #413's head (`6b14480b`):
  `crates/hh-runner/src/verify_hosts.rs` (`hh-verify-host-derivation/1` fold) +
  `bin/fp_verify_hosts.rs` + `tests/r6_51.rs` + checkers; the committed record
  `docs/build/reports/2026-11-19-fp-verify-host-derivation-smoke1.json` emits
  `hosts: [download-r2.pytorch.org, ossci-datasets.s3.amazonaws.com,
  yann.lecun.com]` — exactly the three smoke1-observed hosts, unaided with
  provenance; `download.pytorch.org` recorded under `non_verify_denies`
  ("excluded — decided under a non-verify policy layer"), never derived.
- **Branch containment (verbatim):** `git merge-base` against
  `refs/pull/413/head` (`6b14480b`) — IS an ancestor of `origin/main` (merge
  commit `3a03b4d2`), NOT an ancestor of HEAD
  (`codex/harnessharness-r6.53 @ 6e7e166b`). **PR #413 rides `main`, NOT an
  ancestor of this chain** — an integration fact, not a gap; no derivation
  ticket minted.
- **Stale lead:** the row's lead cell reads OPEN while the dated disposition is
  DONE — lead-token normalization is CAP7.2's named reconciliation scope
  (recorded, not acted on here).

## Seam/decision sweep (shared decisions over the round's machinery + packets)

| seam | decision recorded | disposition | home |
|---|---|---|---|
| gen-1→gen-3 repair loop | sealed baseline docs superseded, never rewritten: gen-1 `88042f…` failed G-L1 on two doc-authoring gaps (`transcript_render` reduced set; `control_strategy` submit-marker dormancy); gen-3 `4142fb10…` passed l1_1+l1_2 8/8 (`l1-pass:sha256:5b807c53…`) | recorded; superseding record chains per generation, all recompute byte-identical | runs/R7.3.md + `compute/2026-11-21-fp-p4x-l1-gen3-conformance.json` |
| withheld levels | V8/V9 declared with declaration/forecast/spellings, never sealed, never in any window plan | typed terminal verbatim | w-rule/registry/prereg gen-3 records |
| selection gates | `selected{m=1}` V1 (+45,455 ppm, gates a–f); V2/V5/V6 fail (d) unresolved>10%; V3/V4/V7 fail (c) `unmeasured` + (d); no C₁₂ | typed outcomes; gate (c) failures are DF-R7.1-2's live evidence | `a1df9b74…` |
| X1 46 `detail_lost` slots | `recorded{early_exit:zero_model_calls, detail_lost}` — WAL-proven zero calls; unresolved for every purpose, never imputed; counted in gate (d) + quote cost census at 0 µ$ | typed; the causing harvest loss is DF-R7.3-4 OPEN | `ee764ce1…` slots-lost + packets + DF-R7.3-1 noting |
| deadline fence | repair `06b5847c` `fence_overdue` fired 16× live across P4-C r3/r4; zero post-repair coordinator SIGTERMs | **DF-R7.3-3 live-discharged 2026-11-26** | DF-R7.3-3 dated disposition + runs/R7.5.md |
| P4-C w1 stop | checkpoint `batch_fits:false` → `withheld{stop:budget}` ×222 — driver-internal 40Mµ bound below authorized 46,353,600 | honest machinery stop; resume authorized by GATE-G36 amendment | `e2f1950e…` + readout amendment |
| resume defects r2a/r2b | stale `proxy-session.json` → `native_lane_hold` ×222; writer-lease WouldBlock (expiry takeover only); hardcoded artifact names | DF-R7.5-1 OPEN — operational remediation on r3/r4; driver unpatched | `0960a946…` + DF-R7.5-1 |
| guest outages | 14:01Z ens4 DHCP loss; 17:39Z subject ~31 GB anon-RSS → global OOM → journald SIGABRT + ens4 death | first recorded verbatim; second is DF-R7.5-2 OPEN | runs/R7.5.md + DF-R7.5-2 |
| confirmation estimand | 367/370 resolved; Δ̂ `refused{UnresolvedCell}` (2 verifier_timeout + 1 podman rc-125); bound [-10810,5406] + labelled conditional secondary [-76470,58824]/34 tasks; Holm P4-C1 not-rejected | honest refusal — never pairs absence; secondary never promoted | `ebaf0b17…` |
| cost attribution fidelity | `slot_result.cost_micros` undercounts late-landing `cost.attributed` (X1 6,099,597 vs 6,202,514; P4-C 16,638,034 vs WAL 16,896,165) | **NEW — rowed DF-CAP7.1-1**; H21 closed the P4-C residual by WAL recompute on retained disk | DEFERRALS.md DF-CAP7.1-1 |
| verify-cell collision | `p4c-V1-model-extraction-relu-logits-r4` verifier rc 125 (podman name collision) → `recorded{reward_absent}` unresolved | **NEW — rowed DF-CAP7.1-2** | DEFERRALS.md DF-CAP7.1-2 |
| settlement | model `within` both windows ($21.08 vs $24.21 boundary-mixed; $16.90 WAL vs ≈$16–18); compute `unavailable`; p4c confirming read `pending{clock}` | typed pending verbatim; CAP7.2 cites or records | readouts/HUMAN-H21.md + `hh-fp-settlement/1` ×2 |
| readout bookkeeping | G36 Status line lagged PASSED (repaired `e784f583`); PASSED decision row fused into PHASE LOG invisible to `load_decisions` (restated `dba6b08d`) | recorded + repaired append-only | commits + LEDGER dated findings |
| gate deferral | DF-G35-1 minted on operator deferral, withdrawn same-day — DONE by ruling | dated DONE; stale lead → CAP7.2 | DEFERRALS.md DF-G35-1 |
| stale lead tokens | dated DONE/discharged dispositions under OPEN lead cells: DF-CAP6.1-1…5, DF-R6.45l-1, DF-R6.50-1, DF-R6.53-1, DF-R7.1-1, DF-G35-1, DF-R7.3-3 | CAP7.2's named reconciliation scope — recorded, never flipped | DEFERRALS.md dated rows |
| results-site pin | adding the claims packet to site-data trips the publication pin — not landed | recorded honestly; claims leg satisfied packet-side | runs/R7.6.md |
| secret scrub | GH013 flagged fixture `hf_` token in window-r4 `stdout_tail`; committed copy scrubbed, pre-scrub sha `a13d6098…` binds the retained mirror | recorded verbatim | runs/R7.5.md scrub addendum |
| run-ledger duplicate | runs/R7.5.md carries the 2026-11-25T01:00Z intervention addendum twice verbatim (~lines 136–174); the events occurred once | **dated correction note appended** (append-only) | runs/R7.5.md Correction 2026-11-26 |

## Verification (executed this pass)

- `bash docs/build/tools/check-supplement-guard.sh --no-checker` → **PASS**,
  route `DISPATCH — CAP7.1` (pre-write; post-write route recorded in the run
  ledger).
- `bash scripts/check-bm-tools.sh` → **OK**.
- `bash scripts/deferrals-open.sh` + `bm-row.sh` reads for every DF row cited
  (DF-R6.45l-1, DF-R7.1-1/-2, DF-R7.3-1/-2/-3/-4, DF-R7.5-1/-2, DF-G35-1).
- PR #413 containment: `git fetch pull/413/head` → `6b14480b`; `merge-base
  --is-ancestor` vs `origin/main` → true, vs HEAD → false.
- Not run: cargo suites/check-fp-* emitters (records-only ticket — no code or
  emitted artifact changed; the fold/emit checkers remain the owners' batteries,
  last recorded green at each ticket's landing).
