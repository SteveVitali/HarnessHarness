# GATE-G28f — PROV-Wc6: authorize the sixth repair window's compute (D=8h re-proof on proven machinery + widened allowlist) · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. Wc6 is a
scope-amendment window — a SIXTH repair window — authorized by the operator's scope-decision. This readout
carries its compute authorization for REQ-XH-004.

## Evidence (per item: source, `date -u` of the read, layer reached)

- Wc6 declaration — `scripts/gce-backend/declarations/fp-wc6.json` (`gate PROV-Wc6`, quote
  `pq-2866041059397e91`, `model_line: none`, ceiling 3,193,860 µ$, `verbatim_sha256` unstamped) — read
  2026-11-13.
- Wc6 quote — `docs/build/reports/2026-11-13-fp-wc6-quote.json` (planning 732,994 µ$, bound 2,096,465 µ$,
  ceiling 3,193,860 µ$) — read 2026-11-13.
- The machinery — all five O-surface defects repaired + verified live; runner `b1ba7ffdc…`; epoch-2
  widened allowlist `sha256:86def07e…` (verified live in Wc5 — zero denied hosts) — layer: committed code +
  records.
- The trigger — `task_lists.state: blocked` (Wc5's re-proof ran real trials to 29/30 surfaces, zero denied
  hosts, but hit the 6h dead-man — the real-work pace ~12min/surface needs more than 6h) — layer: committed
  records.

<!-- agent-drafted:begin sha256=66c10e78556c2409 -->
## Decision content for the operator

**The scope.** Wc6 is the **sixth** repair window — the re-proof on the thrice-repaired O-surface machinery
(`b1ba7ffdc…`) under the widened epoch-2 allowlist (`sha256:86def07e…`) at **D=8h**. The machinery AND the
declared data are both **proven working** — Wc5's re-proof ran real trials to 29/30 surfaces, every H-allow
cell produced a real `reward.txt` digest with **zero denied hosts**, no `allowlist_gap`. The only remaining
blocker is wall-clock: the real bootstrap executing makes the pace ~12 min/surface, so the held leg hit the
6h dead-man at 29/30. Wc6's D=8h covers the proven pace → `reproofed` verdicts → `task_lists.state: frozen`
→ every withheld subject leg unblocks.

**Authorization (REQ-XH-004).** Wc6 is **$0 model** (`model_line: none`; `deliver-secret` ABSENT). **Compute**
verbatim and separate: quote **`pq-2866041059397e91`**, ceiling **3,193,860 µ$ (~$3.19)**; §3.10 hard-cap
`r·(D + 1h restart) + S·r_disk·h_r + 0.12·E_gib` = `314,140·(8+1) + 300·137·6 + 0.12·1` = `2,827,260 +
246,600 + 120,000` = **3,193,860 µ$**.

**Shape.** D = **8 h** dead-man (the proven ~12 min/surface pace × 30 held + dev ≈ 6.5–7h; 8h gives margin);
one ≤1h harvest-only restart; h_r 6h; C = 4; ~38 trials (14 `needs: wc_reproof` × committed R); egress 1 GiB;
e2-standard-8, debian-12, 300 GiB. The epoch-2 freeze lands verbatim (`hh-w1 prove` reads `freeze_file`).

**The Σ-exceed (recorded verbatim).** The declared Σ of window ceilings was 19,793,630 µ$ ≤ $20. The repair
windows pushed it to 28,799,390 µ$; Wc6 pushes it to **31,993,250 µ$ ≈ $32.00** — over the declared bound.
The scope amendments are authorized (the repair windows + the allowlist amendment). The windows' **actual**
spend is ~$10.2M of declared ~$32.0M.

**Dispositions.** `run` — the compute authorized verbatim; R6.45f dispatches for the Wc6 body (PROV → LOAD
→ the thrice-repaired runner + epoch-2 freeze → re-proof at D=8h → frozen-list outcome → teardown → upload
→ settle). `not_run` — the task list stays `blocked`, all subject legs withhold. Your verbatim answer
dispositions it.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decision (verbatim, received 2026-11-13T21:42:46Z, channel: interactive): "authorized, continue"
Disposition: PASSED — run; basis: "the compute authorized verbatim; R6.45f dispatches for the Wc6 body"
Recorded verbatim: Wc6 $0 model (model_line: none, deliver-secret ABSENT); compute pq-2866041059397e91,
ceiling 3,193,860 µ$ (~$3.19), formula 314,140x9 + 300x137x6 + 0.12x1; D=8h, restart<=1h, h_r 6h, C=4,
~38 trials; the re-proof on the thrice-repaired runner b1ba7ffdc… under the epoch-2 widened allowlist at
D=8h (the proven real-work pace); Σ-exceed recorded (31,993,250 µ$ ≈ $32.00 — scope amendments authorized).
verbatim_sha256 = sha256_text("authorized, continue") = a8428147fe1c7c1577dfe062327cf41a19491a49daf68c9432b9f276f8569996 — stamped fp-wc6.json.
GATE DECISIONS row: 2026-11-13T21:42:46Z | GATE-G28f
Operator confirmation of agent-drafted text (verbatim, 2026-11-13T21:42:46Z): "authorized, continue"
— covers sha256:66c10e78556c2409. Signed by: the operator. Recorded by Devin (SWE-2 High, coordinator),
which does not sign.
