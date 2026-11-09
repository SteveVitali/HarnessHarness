# GATE-G25 — PROV-W1: authorize the W1 compute window ($0 model) · readout
Status: PASSED

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion (verbatim from the spec)

Spec §10.3, gate table (presented after "SELECT commit + P2-infra + HUMAN billing alert"):

> | PROV-W1 | first | SELECT commit + P2-infra + HUMAN billing alert | compute quote (planning, bound, ceiling with restart, h_r and egress terms); dead-man action (Q-X38) and the pre-authorized restart (Q-X60); network posture (Q-X55); re-proof scope (REQ-XH-116); layer-export size estimate against W1's egress line (a quote above it refuses, REQ-XH-118) and the controller's free disk; Q-X39b |

Cited requirements (verbatim in the ticket): REQ-XH-004 (model + compute authorizations verbatim and separate, quote id, hard-cap formula — no ticket authorizes spend); REQ-XH-116 (re-proof scope + SELECT-before-PROV ordering); REQ-XH-118 (C ≥ 4, W_p ≤ D − cut-off, W1 itemization, Σ ceilings ≤ $20, export ≤ egress line); REQ-XB-224 (one ≤ 1 h harvest-and-teardown-only restart quoted inside the ceiling; h_r ≤ 6 h as teardown deadline); REQ-XB-125 (re-proof decides every task on a typed outcome-blind basis; discordance register append-only).

Cited operator decisions: Q-X38 (compute safeguards — ratification at PROV-W1 amounts); Q-X60 (pre-authorized harvest-and-teardown-only restart — window-gate amount); Q-X55 (first-pass agent/grading network — answered; presented verbatim below); Q-X39b (re-proof evidence label — pending, decided here).

## Evidence (per item: source, `date -u` of the read, layer reached)
- SELECT commit (the draw, split, successors, replacement orders, development list) — `docs/build/reports/2026-11-08-fp-select-commit.json` sha256:b8b2615315dfe9e3 — read 2026-11-09T20:11Z — landed (R6.30, merged)
- W1 `hh-compute-quote/1` (planning / bound / ceiling in integer µ$) — `docs/build/reports/2026-11-08-fp-w1-quote.json` sha256:78372fb57e189664 — read 2026-11-09T20:11Z — landed (R6.14 machinery, R6.30 commit)
- `comparative-quote/1` (all five window ceilings + Σ) — `docs/build/reports/2026-11-08-fp-w1-comparative-quote.json` sha256:98398b1bdc38f6ba — read 2026-11-09T20:11Z — landed (R6.30, G23-admitted machinery)
- W1 `hh-compute-decl/1` (dead-man, restart, h_r, network, work items, no `deliver-secret`) — `scripts/gce-backend/declarations/fp-w1.json` sha256:f0b8e6cd5f5fd0f1 — read 2026-11-09T20:11Z — landed (R6.30)
- Declaration battery — `bash scripts/check-gce-backend.sh` → 110 passed, 0 failed — run 2026-11-09T20:1xZ (fresh at presentation; covers W_p ≤ D − cut-off, the export-overrun refusal, the restart declaration, teardown terms)
- HUMAN billing alert — `docs/build/readouts/HUMAN-H13.md` sha256:72fb4a9e6c742c44 — read 2026-11-09T20:11Z — COMPLETE ($20 @ 50/90/100 %, recipients/billing/gcloud `provided: yes`; merged 2dc50d0)
- P2-infra methods admitted — `docs/build/readouts/GATE-G23.md` — PASSED, merged a494f38
- Controller free disk — `df -g .` → 77 GiB free — read 2026-11-09T20:11Z — controller-local
- Re-proof scope (tasks, per-surface R, successors) — the `reproof_scope` block of the select-commit (sha256 above)

<!-- agent-drafted:begin sha256=e67778fa2c628126b7b7769fa182a46ec42077bfa38fb82bb7566f59112021e5 -->
## Decision content for the operator (pre-registered thresholds, never moved to fit a quote)

**1. Authorizations (REQ-XH-004).**
- Model authorization: **none — W1 is a $0-model window** (§10.1.4 W1 (a)). No model key is delivered; the declaration carries `model_line: "none"` and no `deliver-secret` field exists anywhere in `fp-w1.json` (REQ-XB-219).
- Compute authorization (separate): the W1 ceiling **$5.05 (5,050,420 µ$)** inside the first-pass $20 billing authorization configured at HUMAN-H13.
- Quote id: `pq-3e4ad6c78a564b77` (`comparative-quote/1` row `w1`, priced by R6.14 `hh-compute-quote/1`).
- Hard-cap formula (§3.10), integer µ$, every term shown: `ceiling = r·(D + restart) + S·r_disk·h_r + 0.12·E_gib` = `314,140 µ$/h · 13 h` (machine e2-standard-8 268,040 + ephemeral IPv4 5,000 + boot disk 300 GiB · 137 = 41,100, sum 314,140/h; D = 12 h + 1 h restart) `+ 300 GiB · 137 · 6 h` (residual disk 1,800 GiB·h = 246,600) `+ 0.12 $/GiB · 6 GiB` (720,000) = **4,083,820 + 246,600 + 720,000 = 5,050,420 µ$**.

**2. Quote content (REQ-XH-118).** planning 3,822,553 µ$ · bound 9,799,955 µ$ · ceiling 5,050,420 µ$, restart + h_r + egress terms all present. The W1 quote lists every required work item: `observe_pass, rust_build, admission_build, layer_export, census, smoke_battery`. C = 4 (model_in_flight_limit 4; 26 GiB / 6 GiB per trial; 8 vCPU). W_p ≤ D − cut-off (admission cut-off 0.65 h; load 1.3 h + overhead 1.5 h + per-trial bound 2,100 s · 179 trials / C, retry allowance 0 declared) — verified green by `check-gce-backend.sh` at presentation. Σ of every window ceiling incl. Wc = **19,793,630 µ$ ≤ 20,000,000** (w1 5,050,420 · w2 2,408,510 · w3 2,408,510 · w4 7,988,890 · wc 1,937,300). §10.1.5's W1 row (D 12 h, ceiling $5.05, egress ≤ 6 GiB) was a planning input — the presented quote governs and matches.

**3. Export line.** Layer-export estimate **5 GiB ≤ W1's declared egress line 6 GiB** — passes; a quote above it would refuse. Controller free disk 77 GiB vs the 5 GiB estimate.

**4. Dead-man and restart (Q-X38, Q-X60, REQ-XB-224).** GCE `max_run_duration` 12 h with action **STOP** (plus in-guest 735 min timer); D = 12 h from the authorized quote; h_r declared **6 h** = the teardown deadline after stop or window end; exactly **one pre-authorized harvest-and-teardown-only restart ≤ 1 h** — no slot admission, no model key delivery — quoted inside the ceiling (the 13th instance-hour above); teardown across every operator-gate wait.

**5. Network posture (Q-X55, verbatim record).** Agent phase: **model proxy + apt/PyPI/npm/crates only — no github.com**; grading phase may reach the hosts its verifier needs; every egress attempt logged per slot; a task needing another agent-phase host is excluded `runner_infeasible:allowlist_gap{host}` and replaced from the committed within-cell order. VM egress is NAT-free with an ephemeral IPv4 (`proxy_on_vm: true`).

**6. Re-proof scope (REQ-XH-116, REQ-XB-125).** The 10 drawn held-out tasks + one successor per difficulty cell (easy `fix-git`, medium `tune-mjcf`, hard `password-recovery`) at **R = 3**, and the 7 development tasks (including smoke) at **R = 2** — on surfaces O, H-pub, H-allow, admit rule R/R per surface; 20 tasks total in `reproof_scope.tasks`. The SELECT commit (R6.30) predates this gate. Discordances go to the append-only register; an exclusion is typed `runner_infeasible:oracle_fails_on_O{harbor_cause}` and replaced from the committed order; dev verdicts enter no claim.

**7. Q-X39b — the re-proof's evidence label (decided here).**
- **(a) recorded default:** register the `reproof` `live-verify.sh` leg (R6.17) so the re-proof can earn **`live-executed`**; or
- **(b)** label it **"recorded execution"**.

**8. Preconditions.** HUMAN-H13 COMPLETE ($20 @ 50/90/100 %, `provided: yes` fields); R6.30's SELECT commit + W1 declaration committed before this record; P2-infra landed (GATE-G23 PASSED); window pattern GATE-PROV → PROV → runs → TEARDOWN → HUMAN-reconcile applies (reconcile consolidated in HUMAN-H18).

**DEFERRALS rule.** DF-R6.28-1 (the real-O replay leg) is scoped to W1's own execution body — its unblock trigger is the W1 window itself (owner R6.32), so it is work W1 performs, not an open prerequisite; DF-R6.23-1 is scoped to the W2+ window gates. No OPEN row is a W1-gate prerequisite.

**Dispositions.** `PASSED` records verbatim the authorization, the quote id and the Q-X answers — it authorizes PROV-W1 provisioning of exactly this declaration; or refusal (the chain stops). A quote failing items 2 or 3 is not passable until re-quoted.
<!-- agent-drafted:end -->

## Signature
Operator decision (verbatim, received 2026-11-09T20:16Z via interactive chat): "authorized, continue"
Dispositions recorded from the verbatim: (a) PROV-W1 PASSED — provisioning of the presented declaration authorized; (b) Q-X38/Q-X60 amounts ratified as presented ($5.05 ceiling, D = 12 h, h_r = 6 h, one <= 1 h harvest-and-teardown-only restart); (c) Q-X39b — presented recorded default (i) stands, the verbatim authorized the gate as presented without changing it: the `reproof` `live-verify.sh` leg (R6.17) is registered so the re-proof can earn `live-executed`.
verbatim_sha256 = sha256_text("authorized, continue") = a8428147fe1c7c1577dfe062327cf41a19491a49daf68c9432b9f276f8569996 — stamped into fp-w1.json `authorization.verbatim_sha256` (declaration sha256 after stamp: 2502b777d2e118f3).
GATE DECISIONS row: 2026-11-09T20:16Z | GATE-G25
Operator confirmation of agent-drafted text (verbatim, 2026-11-09T20:16Z): "authorized, continue" — covers sha256:e67778fa2c62
Signed by: the operator. Recorded by Devin (SWE-2 High, coordinator), which does not sign.

