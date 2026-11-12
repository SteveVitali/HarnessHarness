# GATE-G28d — PROV-Wc4: authorize the fourth repair window's compute (re-proof on the thrice-repaired runner) · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. Wc4 is a
scope-amendment window — a FOURTH repair window (Wc, Wc2, Wc3 discharged) — authorized by the operator's
scope-decision. This readout carries its compute authorization for REQ-XH-004.

## Evidence (per item: source, `date -u` of the read, layer reached)

- Wc4 declaration — `scripts/gce-backend/declarations/fp-wc4.json` (`gate PROV-Wc4`, quote
  `pq-78f1e79eeb33f575`, `model_line: none`, ceiling 2,565,580 µ$, `verbatim_sha256` unstamped) — read
  2026-11-12.
- Wc4 quote — `docs/build/reports/2026-11-12-fp-wc4-quote.json` (planning 732,994 µ$, bound 2,096,465 µ$,
  ceiling 2,565,580 µ$) — read 2026-11-12.
- The repairs — all five O-surface defects fixed: compose-frontend + cp_in (`ad2321c8`), mediated-egress
  (`f9b6ad05…`, `5b8e8f28`, verified live), cell-terminate pkill (`c8184fed…`, `bb60a46f`), verifier
  `reward_missing` (`b1ba7ffdc…`, `fe87c637`) — layer: committed code + repair records.
- The trigger — `task_lists.state: blocked` (Wc3's re-proof ran real trials to the 5h dead-man at 28/30
  surfaces; all H-allow cells `verifier_unavailable:reward_missing` — the verifier's reward path, now
  repaired) — layer: committed records.
- Previous window — Wc3 `archive_verified` (rev `c34bb22f…`), both its repairs verified live — layer:
  committed records.

<!-- agent-drafted:begin sha256=a687d50492056d8f -->
## Decision content for the operator

**The scope.** Wc4 is the **fourth** repair window — the re-proof on the thrice-repaired O-surface machinery
(egress `f9b6ad05…` + pkill `c8184fed…` + verifier `b1ba7ffdc…`, all five defects now repaired). Wc3 proved
real trials execute (mediated-egress + cell-terminate verified live) but (a) hit the 5h dead-man at 28/30
surfaces — slow members need more headroom — and (b) all H-allow cells hit `verifier_unavailable:reward_missing`
— the verifier's `/logs/verifier` sink was never created so `reward.txt` couldn't land. Both are now fixed
(the verifier repair `fe87c637`, D=6h). Wc4 re-proofs the 14 `needs: wc_reproof` members → `reproofed`
verdicts → `task_lists.state: frozen` → every withheld subject leg unblocks.

**Authorization (REQ-XH-004).** Wc4 is **$0 model** (`model_line: none`; `deliver-secret` ABSENT). **Compute**
verbatim and separate: quote **`pq-78f1e79eeb33f575`**, ceiling **2,565,580 µ$ (~$2.57)**; §3.10 hard-cap
`r·(D + 1h restart) + S·r_disk·h_r + 0.12·E_gib` = `314,140·(6+1) + 300·137·6 + 0.12·1` = `2,198,980 +
246,600 + 120,000` = **2,565,580 µ$**.

**Shape.** D = **6 h** dead-man (Wc3's 5h bound hit at 28/30 — the re-proof's real-work pace has ~2 slow ~1h
members; 6h gives headroom; `W_p ≈ 2.333 ≤ D − cut-off 5.35` holds); one ≤1h harvest-only restart; h_r 6h;
C = 4; ~38 trials (14 `needs: wc_reproof` × committed R); egress 1 GiB; e2-standard-8, debian-12, 300 GiB.
The held/dev split-leg option is recorded as the later-gate fallback if even D=6 is tight.

**The Σ-exceed (recorded verbatim).** The declared Σ of window ceilings was 19,793,630 µ$ ≤ $20. Wc2/Wc3
pushed it to 23,668,230 µ$; Wc4 pushes it to **26,233,810 µ$ ≈ $26.23** — over the declared bound. The scope
amendments are authorized (Wc2 "that's okay if it exceeded by a little"; subsequent windows' scope-decisions).
The windows' **actual** spend is ~$6.48M of declared ~$26.2M.

**Repair scope + version (REQ-XC-330).** Scope: the verifier `reward_missing` defect — `/logs/verifier` was
never created in the cell (the H-allow cells' apt bootstrap is legitimately denied under the frozen
allowlist → pytest never runs → `reward.txt` write fails → `reward_missing`); plus the deny-fold read `host`
vs `host_norm`. The thrice-repaired runner `hh-w1` `b1ba7ffdc…` (carrying all three fixes) is the new
participant/runner version (`carried_rebuild_guard` satisfied — this gate authorizes it).

**Dispositions.** `run` — the compute authorized verbatim; R6.45d dispatches for the Wc4 body (PROV → LOAD
→ re-proof on `b1ba7ffdc…` at D=6h → frozen-list outcome → teardown → upload → settle). `not_run` — the task
list stays `blocked`, all subject legs withhold. Your verbatim answer dispositions it.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decision (verbatim, received 2026-11-12T17:09:25Z, channel: interactive): "authorized, continue"
Disposition: PASSED — run; basis: "the compute authorized verbatim; R6.45d dispatches for the Wc4 body"
Recorded verbatim: Wc4 $0 model (model_line: none, deliver-secret ABSENT); compute pq-78f1e79eeb33f575,
ceiling 2,565,580 µ$ (~$2.57), formula 314,140x7 + 300x137x6 + 0.12x1; D=6h, restart<=1h, h_r 6h, C=4,
~38 trials; repair scope = verifier reward_missing fix (runner b1ba7ffdc… carrying egress+pkill, REQ-XC-330);
Σ-exceed recorded (26,233,810 µ$ ≈ $26.23 — scope amendments authorized).
verbatim_sha256 = sha256_text("authorized, continue") = a8428147fe1c7c1577dfe062327cf41a19491a49daf68c9432b9f276f8569996 — stamped fp-wc4.json.
GATE DECISIONS row: 2026-11-12T17:09:25Z | GATE-G28d
Operator confirmation of agent-drafted text (verbatim, 2026-11-12T17:09:25Z): "authorized, continue"
— covers sha256:a687d50492056d8f. Signed by: the operator. Recorded by Devin (SWE-2 High, coordinator),
which does not sign.
