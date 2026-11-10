# GATE-G26 — FP-S: authorize first-pass model spend and the W2 window · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion (verbatim from the spec)

Spec §10.3 gate table: "**FP-S** first-pass spend | first | FP-PREREG + Ollama/HF HUMAN rows + the E2-T8
rung record (Q-X62) | Q-X21 (automatic usage billing off, or meter-only recorded; exclusivity; post-reset
allowance); verbatim model authorization A ≤ $70 for the pass; m (= .20 for the whole pass, REQ-XH-005);
A_eff at FP-S (REQ-XH-120); the slice rule (REQ-XC-126); the per-arm cap formula with expected-cost
admission (REQ-XH-117) and arm priority (Q-X61); the wall-time rule (REQ-XH-126); the fallback trigger
(Q-X62, quoted from the ledger row); W2 quote, D and restart; pre-registration hash (tasks, arms, effort
`high`, ε = ±.20, the V5 level, claims and their expected precision); credit period (REQ-XH-007); the
per-window archive-upload authorization (Q-X35); Q-X47 and Q-X54 cited".

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the
model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No
ticket authorizes spend.

## Evidence (per item: source, `date -u` of the read, layer reached)

- FP-PREREG packet — `docs/build/reports/2026-11-10-fp-prereg.json` sha256 `2e36bf81d3af55491af00608c63bedf3ed035e0193cc0ce4cd0f3b609e450201` — read 2026-11-10 — layer: committed record.
- FP-S readiness record — `docs/build/reports/2026-11-10-fp-fps-readiness.json` (committed `pending{HUMAN-H14 not run}`); re-derived below with H14 COMPLETE — layer: committed + re-derived.
- HUMAN-H14 — `docs/build/readouts/HUMAN-H14.md` Status COMPLETE — read 2026-11-10 — layer: signed readout.
- E2-T8 rung record — `docs/build/reports/r6-39-c05-rung.md` — **rung 1** recorded honestly — read 2026-11-10.
- W2 comparative quote — `docs/build/reports/2026-11-10-fp-w2-comparative-quote.json`; W2 declaration `scripts/gce-backend/declarations/fp-w2.json` — read 2026-11-10.
- W1 billed figure — R6.32/W1 receipts: first attempt 784,750 µ$ + re-run ~1,340,000 µ$ ≈ 2,124,750 µ$ (~$2.13 of $20 compute) — layer: committed receipts.
- Task-lists state — the prereg packet's walk: **`blocked`** (readiness `task_lists_state`), all 20 W1 re-proof tasks `o_fail` — `docs/build/reports/compute/harvest/fp-w1/run-root/prove-{held,dev}/reproof-register.json`, `harbor_exception` digests recorded — layer: live-executed machinery-failure evidence.
- Open deferrals — `bash scripts/deferrals-open.sh`: DF-R6.23-1 OPEN (owned by R6.40's W2 readout — the paid-window archive citation, not FP-S-scoped; not a gate blocker).

<!-- agent-drafted:begin sha256=37b320bf9a24bdcf -->
## Decision content for the operator

**The state this gate authorizes under — the honest finding first.** W1's re-proof register is `o_fail`
across every drawn task (held 13 + dev 7 = 20) — the O surface (Harbor oracle through local podman)
raised machinery exceptions on every trial (`harbor_exception` digests recorded; wall ~7 s early fails).
That is `live-executed` machinery-failure evidence, **not agreement data** — and under the prereg's typed
rules the successor task list is `blocked` (`blocked{un_reproofed_successor}` / `blocked{no_adjacent_donor}`;
zero slots `frozen`). W2's subject slots cannot admit until a frozen task list exists: the O-surface
defect needs repair, and un-reproofed successors then re-proof at **Wc** (the contingency window
`pq-556b0a0f5d853edb`, 1,937,300 µ$). The schedule, census and arms are recorded; the freeze is honest.
Your disposition decides whether to authorize spend now (W2 proceeds only as the task list unblocks —
W1's archive upload is its first step and needs no list) or to hold the spend until repair + re-proof.

**1. Q-X21 confirmations.** Automatic usage billing: **not off** — verbatim "auto-reload of credits is on
and billing is set up" (H14 item 1a) → the model line is recorded **meter-enforced only** (REQ-XH-120).
Exclusivity: acknowledged for every paid window (H14 item 1b; REQ-XH-013 — no other collection and no
operator use of the model account during a paid window). **Your ruling needed:** whether the post-reset
allowance or purchased credit may fund A (Q-X54; auto-reload is on, so metered draw is live — state how
funding may be drawn).

**2. Model authorization** — **A ≤ $70** for the whole pass, verbatim (Q-X54: "Model ≤$70, compute ≤$20,
$10 reserve (Recommended)"). W2 **compute** authorization separately (REQ-XH-004): quote
`pq-89f25ab8f891d511`, ceiling **2,408,510 µ$ (~$2.41)**; hard-cap `r·(D + 1 h restart) + S·r_disk·h_r +
0.12·E_gib` = `314,140·6.5 + 300·137·6 + 0.12·1` = 2,041,910 + 246,600 + 120,000 = **2,408,510 µ$**.

**3. m = .20** for the whole pass (REQ-XH-005; Q-X36 recorded default).

**4. A_eff and root.** A_eff = min(A, included credit as Q-X21 makes it available + purchased). With
meter-enforced billing and no purchase declared, A_eff per your item-1 ruling — at A_eff = $70 the root
is **$56**; at A_eff = $60 (the $60 included-credit fixture) the root is **$48** (REQ-XH-120). Each
window's proxy pool = root − Σ prior windows' consumed + orphans at bounded.hi (harvested proxy ledgers;
unharvested blocks the next window).

**5. Slice rule** (REQ-XC-126): s ≥ arm P90 $/trial + w ≈ $0.10 (worst-case call reservation) — equal
across the comparison's arms; first pass s ≈ **$0.85** (2 × largest binding prior + w); feasibility
subtracts C × s (C = 4); premature refusals type `inconclusive{infra:budget_reservation}`, never scored.

**6. Per-arm caps + priority** (REQ-XH-117): cap_h = planned slots_h × p̂_h in integer µ$ (tabulated in
the prereg, values filled at FP-E); expected-cost admission; unprotected arms (Pi, Claude Code, V5) capped,
protected pair (original + HH-native mini-swe-agent) root-bounded only. **Q-X61 ruling needed** — recorded
default: protected pair protected from cost-dropping; truncation order **V5 → Claude Code → Pi**.

**7. Wall-time rule** (REQ-XH-126) as pre-registered: W4's W_p with retry allowance + load; D_max keeps
Σ compute ceilings ≤ $20; drop order V5 → Claude Code → Pi at FP-E if W_p > D_max − cutoff; O/H pairs
admitted or withheld together, never split.

**8. Fallback trigger** (Q-X62, ledger verbatim): "First-pass spend gate (FP-S) waits for the E2-T8 rung
record; hosted-only fallback fires if G-L1 has not passed 14 days after FP-S is otherwise ready (14 is a
synthesis placeholder)" — **you fix the number of days.** Readiness re-derived per `compute_readiness`:
inputs present — packet (2026-11-10T00:00:00Z), H14 readout (2026-11-10T21:42:46Z), W1 verified manifest
(R6.32b) → **readiness instant 2026-11-10T21:42:46Z**; trigger instant = that + your N days.

**9. W2 quote.** Quote `pq-89f25ab8f891d511`; D = 5.5 h dead-man (in-guest 345 min), one ≤ 1 h
harvest-and-teardown-only restart (REQ-XB-224), h_r = 6 h teardown deadline, egress 1 GiB, retry allowance
4, load 0.5 h, C = 4, 38 trials; teardown across every operator-gate wait. Σ five windows' ceilings =
**19,793,630 µ$ ≤ 20,000,000** ($20 compute, Q-X54); W1 counts at billed ~2,124,750 µ$ where the reconcile
read exists (REQ-XH-118), Wc's D released only while unauthorized.

**10. Pre-registration** (`hh-fp-prereg/1`, sha256 `2e36bf81d3af55491af00608c63bedf3ed035e0193cc0ce4cd0f3b609e450201`):
the 30-task held-out parity tranche + dev list (schedule k = 5 rounds, O/H pairs packed together,
`slot_schedule_randomization` admitted GATE-G23); five arms — original mini-swe-agent 2.4.6 (protected),
HH-native mini-swe-agent (protected, G-L1-gated), Pi, Claude Code, V5; effort `high` (Q-X22); ε = ±.20
(Q-X53 "±.20, expect inconclusive"); **Q-X56 V5 level ruling needed** — recorded default: V5 `rsn-drop`
(always active after turn 1; the alternative V4 `obs-2.5k`); claims P2-FP/P3-L2/P4-FP with their estimators
and expected precision (near-zero power stated honestly; `task_lists_state: blocked` as above).

**11. Credit period** (REQ-XH-007): 2026-10-22T16:30:34Z → 2026-11-22T16:30:34Z; this authorization covers
one period; a provider reset is never new authorization.

**12. Upload authorization** (Q-X35, REQ-XH-102): one authorization covering every window's
ARCHIVE-UPLOAD — W1's included as W2's first step (SR-18 deviation as GATE-G20 accepted); uploads run
from the operator's controller after each window's harvest manifest verifies, via the admitted tool
(`scripts/hf-upload.sh`); the HF write token is fine-grained, scoped to the one private dataset repo,
read per invocation from `HH_OPERATOR_ENV_FILE`, never `login`-persisted, recorded only as
`provided: yes` (H14 item 2 verified). This readout cites W1's **verified harvest manifest**
(`MANIFEST.sha256`, 32,636 members byte-exact) in place of an `archive_verified` index.

**13. Rung-record precondition.** R6.39's E2-T8 record cited: **rung 1** reached (detach/reattach passes;
rung 2 unverified, rung 3 unreached) — recorded honestly, satisfying Q-X62's wait. No trigger waiver
applies; the HH-native arms still need G-L1 before W4.

**14. Masking** (REQ-XH-121): this readout carries cost fields and recorded machinery outcomes only;
no first-pass outcome field is exposed (W1 re-proof verdicts excepted per X-07). Q-X47 ("≤$100 max first
pass") and Q-X54 ("Model ≤$70, compute ≤$20, $10 reserve") cited.

**Dispositions.** `PASSED` records items 1–14 verbatim plus your rulings on the three open questions —
(a) Q-X21 funding path (post-reset allowance and/or purchased credit may fund A; metered draw state),
(b) Q-X56 V5 level, (c) Q-X61 arm priority, (d) Q-X62 fallback days — and your ruling on the blocked
freeze: authorize now (W2 runs W1-upload first, subject slots admit only as the task list unblocks via
O-surface repair + Wc re-proof) or hold the spend until repair + re-proof produce a frozen list. Refusal
stops the chain. A `PASSED` stamps `fp-w2.json`'s `verbatim_sha256` and releases R6.40's fleshing.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decision (verbatim, received 2026-11-10T22:17:29Z via interactive Q&A): "PASSED — authorize now"
Dispositions recorded verbatim (each item's operator answer):
- Q-X21 funding: "Allowance + metered draw" — post-reset allowance AND purchased credit via auto-reload both permitted to fund A → A_eff = 0, root 6.
- Model authorization: A <= 0 verbatim (Q-X54); W2 compute separate: pq-89f25ab8f891d511, ceiling 2,408,510 usd-micro, hard-cap 314,140x6.5 + 300x137x6 + 0.12x1 verified.
- m = .20 whole pass; slice s ~= bash.85; per-arm caps per REQ-XH-117 (values at FP-E).
- Q-X56: "V5 rsn-drop" — the phase-4 variant level.
- Q-X61: "Default order" — protected pair (original + HH-native mini-swe-agent); truncation V5 -> Claude Code -> Pi.
- Q-X62: "14 days" — trigger instant = readiness instant 2026-11-10T21:42:46Z + 14 days = 2026-11-24T21:42:46Z.
- Blocked-freeze ruling: "PASSED — authorize now" — subject slots admit only as O-surface repair + Wc re-proof produce a frozen list; W2 runs W1-upload first; the all-o_fail register is machinery-failure evidence, not agreement data.
- Upload authorization: every window's ARCHIVE-UPLOAD covered (W1's as W2's first step, SR-18); credit period 2026-10-22T16:30:34Z -> 2026-11-22T16:30:34Z, one period.
verbatim_sha256 = sha256_text("PASSED — authorize now") = e92a8f33561c88a5c0fd4dc38962ff98e079349ea605b107ece26c9ede440ef1 — stamped into fp-w2.json authorization.verbatim_sha256.
GATE DECISIONS row: 2026-11-10T22:17:29Z | GATE-G26
Operator confirmation of agent-drafted text (verbatim, 2026-11-10T22:17:29Z): "PASSED — authorize now" — covers sha256:37b320bf9a24
Signed by: the operator. Recorded by Devin (SWE-2 High, coordinator), which does not sign.
