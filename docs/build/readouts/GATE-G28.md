# GATE-G28 — PROV-Wc: authorize the one contingency window, or record it unused · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion (verbatim from the spec)

Spec §10.3 gate table: "**PROV-Wc (at most once)** | first | the triggering event: a W1 machinery repair
(W1′), a gate-A L1 failure (W3′) or a restart overflow | compute quote (planning, bound, ceiling with
restart, h_r and egress terms) within Wc's remaining reservation (at most $1.94, less any W1 export
overrun); which of W1′, W3′ or restart overflow; D, restart and h_r; the repair's scope and its new
participant or runner version record (REQ-XC-330); $0 model; Σ ≤ $20 recomputed (REQ-XH-118)".

§10.1.4 Wc row: "at most one contingency window: a repair re-proof (W1′), an L1 rerun on gate-B (W3′), or a
harvest-restart overflow; D = 4 h gives about 1.75 h of trial time after overhead, load and the admission
cut-off (≈ 42 oracle trials at C = 4), and a larger repair is not runnable in the first pass, a scope
decision PROV-Wc records | $0 | its own PROV-Wc gate | if not authorized by FP-E, its reservation is
released to W4 except one restart-overflow reservation (REQ-XH-126)".

## Evidence (per item: source, `date -u` of the read, layer reached)

- Wc quote — `docs/build/reports/2026-11-08-fp-wc-quote.json` — read 2026-11-11 — layer: committed record.
- Wc declaration — `scripts/gce-backend/declarations/fp-wc.json` (`gate PROV-Wc`, quote `pq-556b0a0f5d853edb`,
  `model_line: none`, `verbatim_sha256` unstamped) — read 2026-11-11.
- The trigger — `docs/build/reports/2026-11-10-fp-prereg.json` `task_lists.state: blocked` + W1's re-proof
  register: all 20 rows `o_fail` (held 13 + dev 7) — the O-surface (Harbor oracle through local podman)
  machinery defect — layer: committed records.
- W1 export overrun — none recorded (W1's export fit its egress line — `w1` receipts) → Wc keeps its full
  $1.94 reservation.
- Previous window closed — W3's `archive_verified` index `d7af5bad…` (the R6.44 merge `5d8766f2`), proxy
  ledger harvested — layer: committed records.
- Open deferrals — `bash scripts/deferrals-open.sh`: DF-R6.23-1 OPEN (gate-citation; W3's index cited above
  — row stays OPEN for R6.45/W4).

<!-- agent-drafted:begin sha256=cad0ffd70323f4bc -->
## Decision content for the operator

**The honest situation — the O-surface defect IS the W1′ trigger.** `task_lists.state: blocked` is
unchanged — all 20 W1 re-proofs `o_fail` on the O-surface's Harbor-oracle-through-local-podman machinery
(machinery failure, not agreement data). W2's PP0 and W3's L1 corpus both withheld `un_reproofed_successor`
on this predicate — every subject leg stays withheld until the list freezes. **Wc is the contingency
window for exactly this**: the W1′ repair + its re-proof to produce the frozen task list the whole pass's
subject legs depend on. If Wc runs, it executes the O-surface machinery repair (the new participant/runner
version record, REQ-XC-330) + re-proofs the un-reproofed successors — producing the frozen list that
unblocks PP0, the L1 corpus, and every downstream experiment slot. **If Wc is not triggered, the task list
stays blocked permanently** — the pass produced W2/W3's infrastructure evidence (H-set, carried sets,
PS0/PP0 legs) but zero subject data; every experiment claim (P2-FP, P3-L2, P4-FP, the parity tranche)
withholds on `l1_not_passed`/equivalent, and W4's arms never admit.

**1. Trigger named.** W1′ — a W1 machinery repair: the O-surface (Harbor oracle through local podman)
re-proof machinery `o_fail`ed on every drawn task. This is the declared W1′ trigger, not a W3′ (the L1
corpus never ran — `un_reproofed_successor`, not an L1 failure) and not a restart overflow.

**2. Quote within reservation.** `pq-556b0a0f5d853edb`, ceiling **1,937,300 µ$ (~$1.94)** — Wc's remaining
reservation (no W1 export overrun taken). Planning 1,413,630 µ$; bound 2,436,783 µ$; D = 4 h (~1.75 h
trial time after overhead + load + cut-off ≈ 42 trials at C = 4); h_r 6 h; egress 1 GiB; restart ≤ 1 h.

**3. Shape.** D = 4 h dead-man; one harvest-only restart ≤ 1 h inside the ceiling; h_r = 6 h; C = 4;
**$0 model — `deliver-secret` ABSENT** (a call refuses).

**4. Repair scope + version record (REQ-XC-330).** Scope: repair the O-surface's Harbor-oracle machinery
path that produced the all-`o_fail` register (the machinery, not the tasks — the drawn tasks are sound);
the rebuilt carried artifact is a new participant/runner version — recorded by the repair's version entry
(a rebuilt carried artifact = a new version needing this gate, per REQ-XC-330). The repair's diagnosis +
implementation is R6.45's Wc-body work.

**5. Σ ≤ $20 recomputed.** The five windows' ceilings = **19,793,630 µ$ ≤ 20,000,000** ($20); Wc's
1,937,300 µ$ fits inside it (W1 ~2,124,750 + W2 ~201,242 + W3 ~167,296 actuals).

**6. Previous window closed.** W3's harvest verified; index `archive_verified` (rev `d7af5bad…`); this gate
cites W3's index — DF-R6.23-1's G28 share; the row stays OPEN for R6.45/W4.

**Dispositions.** `triggered{W1′}` — items 1–6 met; Wc's compute authorized; R6.45 fleshes for the Wc body
(the O-surface repair + re-proof to produce the frozen list). `PASSED — not triggered; Wc unused` — you
record verbatim that Wc is not authorized (R6.45 fleshes to `unused{not_triggered}`; the task list stays
`blocked` → every subject leg withholds → the pass's experiment claims withhold; Wc's reservation releases
to W4 at FP-E except restart-overflow). Your verbatim answer dispositions it.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decision (verbatim, received 2026-11-11T13:09:23Z, channel: interactive): "triggered{W1′}"
Disposition: PASSED — triggered{W1′}; basis: "items 1–6 met and authorized verbatim; R6.45 is fleshed for
the Wc body"
Recorded verbatim: Wc bash model (model_line: none, deliver-secret ABSENT); compute pq-556b0a0f5d853edb,
ceiling 1,937,300 µ$ (~$1.94) — within Wc's remaining reservation (no W1 export overrun); D=4h, restart
<=1h, h_r 6h, C=4; trigger W1′ (the O-surface machinery repair); repair scope + new participant/runner
version record (REQ-XC-330); Σ ceilings 19,793,630 µ$ <= $20; W3 closed (index archive_verified
d7af5bad…).
verbatim_sha256 = sha256_text("triggered{W1′}") = a2b84b81a9cc83859f000e2dfd5ae23f0fbc5e37a5f837bc042b39ae6f8596bd — stamped fp-wc.json.
GATE DECISIONS row: 2026-11-11T13:09:23Z | GATE-G28
Operator confirmation of agent-drafted text (verbatim, 2026-11-11T13:09:23Z): "triggered{W1′}" — covers
sha256:cad0ffd70323f4bc. Signed by: the operator. Recorded by Devin (SWE-2 High, coordinator), which
does not sign.
