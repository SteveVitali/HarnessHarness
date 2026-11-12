# GATE-G28c — PROV-Wc3: authorize the third repair window's compute (longer-D re-proof) · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. Wc3 is a
scope-amendment window — a THIRD repair window (Wc discharged at R6.45, Wc2 at R6.45b) — authorized by the
operator's scope-decision (LEDGER GATE-G28c row). This readout carries its compute authorization for
REQ-XH-004.

## Evidence (per item: source, `date -u` of the read, layer reached)

- Wc3 declaration — `scripts/gce-backend/declarations/fp-wc3.json` (`gate PROV-Wc3`, quote
  `pq-55417e3bc7a1f673`, `model_line: none`, ceiling 2,251,440 µ$, `verbatim_sha256` unstamped) — read
  2026-11-12.
- Wc3 quote — `docs/build/reports/2026-11-12-fp-wc3-quote.json` (planning 732,994 µ$, bound 2,096,465 µ$,
  ceiling 2,251,440 µ$) — read 2026-11-12.
- The repairs — all four O-surface defects fixed: compose-frontend + cp_in (`ad2321c8`), mediated-egress
  (`f9b6ad05…`, `5b8e8f28`, verified live in Wc2), cell-terminate pkill (`c8184fed…`, `bb60a46f`) — layer:
  committed code + repair records.
- The trigger — `task_lists.state: blocked` (Wc2's re-proof produced real trials but hit the 3h dead-man at
  28/30 surfaces — the machinery works, the window was too short; DF-R6.45b-1) — layer: committed records.
- Previous window — Wc2 `archive_verified` (rev `1d4311d1…`), the egress repair verified live — layer:
  committed records.

<!-- agent-drafted:begin sha256=6da9f848d09ac539 -->
## Decision content for the operator

**The scope.** Wc3 is the **third** repair window — a longer-D re-proof of the twice-repaired O-surface
machinery on the 14 `needs: wc_reproof` members. Wc2 verified the mediated-egress fix LIVE (the mediator's
`requested`/`decided` rows now land — real per-host allow/deny) but the re-proof hit the 3h dead-man at
28/30 surfaces — real trials now execute but are slower than the window allowed. The remaining blockers are
fixed: the `pkill` cell-terminate defect (repaired, `bb60a46f`) and the window length (D=5h). Wc3 re-proofs
on the runner carrying BOTH the egress fix (`f9b6ad05…`) and the pkill fix (`c8184fed…`) → `reproofed`
verdicts → `task_lists.state: frozen` → every withheld subject leg unblocks.

**Authorization (REQ-XH-004).** Wc3 is **$0 model** (`model_line: none` — the podman oracle, not a metered
model; `deliver-secret` ABSENT). **Compute** verbatim and separate: quote **`pq-55417e3bc7a1f673`**, ceiling
**2,251,440 µ$ (~$2.25)**; §3.10 hard-cap `r·(D + 1h restart) + S·r_disk·h_r + 0.12·E_gib` =
`314,140·(5+1) + 300·137·6 + 0.12·1` = `1,884,840 + 246,600 + 120,000` = **2,251,440 µ$**.

**Shape.** D = **5 h** dead-man (Wc2's 3h bound hit at 28/30 surfaces — the real-work pace at R=3 needs
~3.3h+ for held-30 plus dev-4's ~8; 5h gives headroom for both legs in one window; `W_p ≈ 2.333 ≤ D −
cut-off 4.35` holds comfortably — the Wc2 thin margin clears); one ≤1h harvest-only restart; h_r 6h; C = 4;
~38 trials (14 `needs: wc_reproof` × committed R); egress 1 GiB; e2-standard-8, debian-12, 300 GiB.

**The Σ-exceed (recorded verbatim).** The declared Σ of window ceilings was 19,793,630 µ$ ≤ $20. Wc2's
ceiling pushed it to 21,416,790 µ$; Wc3's pushes it to **23,668,230 µ$ ≈ $23.67** — over the declared bound.
The scope amendments are authorized (Wc2 "that's okay if it exceeded by a little"; this window "Longer-D
window + pkill fix"). The windows' **actual** spend is ~$4.68M of declared ~$23.7M.

**Repair scope + version (REQ-XC-330).** Scope: the `cell-terminate` `pkill`-absent defect (the cell's
teardown invoked `pkill`, absent on the podman guest's minimal image → `/proc` cmdline walk, `bb60a46f`).
The twice-repaired runner (`hh-w1` `c8184fed…` carrying the egress `f9b6ad05…` fix) is the new
participant/runner version (`carried_rebuild_guard` satisfied — this gate authorizes it).

**Dispositions.** `run` — the compute authorized verbatim; R6.45c dispatches for the Wc3 body (PROV → LOAD
→ re-proof on the twice-repaired runner at D=5h → frozen-list outcome → teardown → upload → settle).
`not_run` — the task list stays `blocked`, all subject legs withhold. Your verbatim answer dispositions it.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decision (verbatim, received 2026-11-12T04:49:05Z, channel: interactive): "i authorize to proceed"
Disposition: PASSED — run; basis: "the compute authorized verbatim; R6.45c dispatches for the Wc3 body"
Recorded verbatim: Wc3 $0 model (model_line: none, deliver-secret ABSENT); compute pq-55417e3bc7a1f673,
ceiling 2,251,440 µ$ (~$2.25), formula 314,140x6 + 300x137x6 + 0.12x1; D=5h, restart<=1h, h_r 6h, C=4,
~38 trials; repair scope = cell-terminate pkill fix (runner c8184fed… carrying egress f9b6ad05…, REQ-XC-330);
Σ-exceed recorded (23,668,230 µ$ ≈ $23.67 — scope amendments authorized).
verbatim_sha256 = sha256_text("i authorize to proceed") = c32505f3b30171694daadca187adbbb953107f447d6709be42292dc7824e0f3d — stamped fp-wc3.json.
GATE DECISIONS row: 2026-11-12T04:49:05Z | GATE-G28c
Operator confirmation of agent-drafted text (verbatim, 2026-11-12T04:49:05Z): "i authorize to proceed"
— covers sha256:6da9f848d09ac539. Signed by: the operator. Recorded by Devin (SWE-2 High, coordinator),
which does not sign.
