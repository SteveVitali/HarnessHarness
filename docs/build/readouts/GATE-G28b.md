# GATE-G28b — PROV-Wc2: authorize the second repair window's compute · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. Wc2 is a
scope-amendment window — a second repair window beyond the spec's declared Wc (at-most-once, discharged at
R6.45) — authorized by the operator's scope-decision (LEDGER GATE-G28b row). This readout carries its
compute authorization for REQ-XH-004.

## Evidence (per item: source, `date -u` of the read, layer reached)

- Wc2 declaration — `scripts/gce-backend/declarations/fp-wc2.json` (`gate PROV-Wc2`, quote `pq-95bf3c1ff646a476`,
  `model_line: none`, ceiling 1,623,160 µ$, `verbatim_sha256` unstamped) — read 2026-11-11.
- Wc2 quote — `docs/build/reports/2026-11-11-fp-wc2-quote.json` (planning 732,994 µ$, bound 2,096,465 µ$,
  ceiling 1,623,160 µ$) — read 2026-11-11.
- The repair — mediated-egress leg fixed (`hh_egress_mediator.rs` scope-mint + resolve-pins + port, merge
  `5b8e8f28`); new runner `sha256:f9b6ad05…` — layer: committed code + repair record `wc2-repair-record.json`.
- The trigger — `task_lists.state: blocked` (all 20 W1 re-proofs `o_fail`; the mediated-egress third defect
  un-repaired → now repaired) — layer: committed records.
- Previous window — Wc `archive_verified` (rev `309d7029…`), its re-proof ran real trials but 0/14 covering
  `reproofed` — layer: committed records.

<!-- agent-drafted:begin sha256=38b8eee326c2374a -->
## Decision content for the operator

**The scope.** Wc2 is the **second** repair window — beyond the spec's declared Wc (at-most-once, which
discharged at R6.45 producing 0/14 `reproofed` on a diagnosed-but-unrepaired third defect). The mediated-
egress defect is now repaired (`f9b6ad05…`); Wc2 re-proofs the 14 `needs: wc_reproof` members on the
repaired runner → `reproofed` verdicts → `task_lists.state: frozen` → every withheld subject leg (PP0, the
L1 corpus, the experiment arms) unblocks. This is the critical path — if the repair holds, the pass finally
produces experiment data.

**Authorization (REQ-XH-004).** Wc2 is **$0 model** (`model_line: none` — the re-proof is the podman oracle,
not a metered model; `deliver-secret` ABSENT). **Compute** verbatim and separate: quote
**`pq-95bf3c1ff646a476`**, ceiling **1,623,160 µ$ (~$1.62)**; §3.10 hard-cap `r·(D + 1h restart) +
S·r_disk·h_r + 0.12·E_gib` = `314,140·(3+1) + 300·137·6 + 0.12·1` = `1,256,560 + 246,600 + 120,000` =
**1,623,160 µ$**.

**Shape.** D = **3 h** dead-man (the repair is done — only the re-proof remains; Wc's 4h covered
repair+re-proof); one ≤1h harvest-only restart; h_r 6h; C = 4; ~38 trials (the 14 `needs: wc_reproof`
members × committed R); W_p ≈ 2.333h ≤ D − cut-off 2.35h (thin margin, ~1 min — flagged honestly); egress
1 GiB; e2-standard-8, debian-12, 300 GiB.

**The Σ-exceed (recorded verbatim).** The declared Σ of window ceilings was 19,793,630 µ$ ≤ $20. Wc2's
ceiling pushes it to **21,416,790 µ$ ≈ $21.42** — over the declared bound. Your verbatim: *"that's okay if
it exceeded by a little"* — the scope amendment. The windows' **actual** spend is ~$3.60M of declared
~$19.8M; Wc2's actuals expected well under its ceiling (Wc spent ~44%).

**Repair scope + version (REQ-XC-330).** Scope: repair the mediated egress leg's DNS/mediation path (the
mediator's `serve_conn` scope-mint violation — SchemaViolation + ScopeNotOpen → zero decided rows — plus
dropped resolve-pins and the proxy-port mismatch). The repaired runner `sha256:f9b6ad05…` is the new
participant/runner version, recorded under this gate's authorization (`carried_rebuild_guard` satisfied).

**Dispositions.** `run` — the compute authorized verbatim; R6.45b dispatches for the Wc2 body (PROV → LOAD
→ re-proof on `f9b6ad05…` → frozen-list outcome → teardown → upload → settle). `not_run` — the re-proof
doesn't run; the task list stays `blocked` → every subject leg withholds. Your verbatim answer
dispositions it.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decision (verbatim, received 2026-11-11T21:00:36Z, channel: interactive): "okay sounds good. i authorize all the above."
Disposition: PASSED — run; basis: "the compute authorized verbatim; R6.45b dispatches for the Wc2 body"
Recorded verbatim: Wc2 bash model (model_line: none, deliver-secret ABSENT); compute pq-95bf3c1ff646a476,
ceiling 1,623,160 µ$ (~$1.62), formula 314,140x4 + 300x137x6 + 0.12x1; D=3h, restart<=1h, h_r 6h, C=4,
~38 trials; repair scope = mediated-egress DNS/mediation path fix (runner f9b6ad05…, REQ-XC-330);
Σ-exceed recorded (21,416,790 µ$ ≈ $21.42 — "that's okay if it exceeded by a little").
verbatim_sha256 = sha256_text("okay sounds good. i authorize all the above.") = e8dd362b268c20f61d2abe1593a52a5c36633a4fbef69ac022d023ed30a3e910 — stamped fp-wc2.json.
GATE DECISIONS row: 2026-11-11T21:00:36Z | GATE-G28b
Operator confirmation of agent-drafted text (verbatim, 2026-11-11T21:00:36Z): "okay sounds good. i authorize all the above."
— covers sha256:38b8eee326c2374a. Signed by: the operator. Recorded by Devin (SWE-2 High, coordinator),
which does not sign.
