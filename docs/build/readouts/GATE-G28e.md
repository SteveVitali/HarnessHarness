# GATE-G28e — PROV-Wc5: authorize the fifth repair window's compute (re-proof under the widened allowlist) · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. Wc5 is a
scope-amendment window — a FIFTH repair window — authorized by the operator's scope-decision. This readout
carries its compute authorization for REQ-XH-004.

## Evidence (per item: source, `date -u` of the read, layer reached)

- Wc5 declaration — `scripts/gce-backend/declarations/fp-wc5.json` (`gate PROV-Wc5`, quote
  `pq-4183fe9dc342aa10`, `model_line: none`, ceiling 2,565,580 µ$, `verbatim_sha256` unstamped) — read
  2026-11-13.
- Wc5 quote — `docs/build/reports/2026-11-13-fp-wc5-quote.json` (planning 732,994 µ$, bound 2,096,465 µ$,
  ceiling 2,565,580 µ$) — read 2026-11-13.
- The machinery — all five O-surface defects repaired + verified live; runner `b1ba7ffdc…` (egress
  `f9b6ad05…` + pkill `c8184fed…` + verifier `fe87c637`) — layer: committed code + repair records.
- The allowlist amendment — epoch-2 successor `fp-w1-q-x55` (`sha256:86def07e…`) covering
  `archive.ubuntu.com`/`security.ubuntu.com`/`ports.ubuntu.com`; epoch-1 untouched per REQ-XB-116 — layer:
  committed records (`28617cb0`).
- The trigger — `task_lists.state: blocked` (Wc4's re-proof ran to COMPLETION — machinery proven — but
  0/14 covering `reproofed`: every H-allow cell `allowlist_gap` under the Debian-family epoch-1 freeze,
  task images Ubuntu noble) — layer: committed records.

<!-- agent-drafted:begin sha256=be7aa37f4ac8a6d8 -->
## Decision content for the operator

**The scope.** Wc5 is the **fifth** repair window — the re-proof on the thrice-repaired O-surface machinery
(`b1ba7ffdc…`) under the **widened allowlist** (epoch-2 `fp-w1-q-x55` covering Ubuntu noble's apt hosts —
the declared-data amendment `28617cb0`). Wc4 proved the machinery works — the re-proof ran to completion and
produced real typed verdicts — but every H-allow cell got `exclude:runner_infeasible:allowlist_gap` because
the epoch-1 allowlist is Debian-family while the task env images are Ubuntu noble (the verifier's apt
bootstrap legitimately denied). The widened allowlist should let the apt install → real `reward.txt` scores
→ `reproofed` verdicts → `task_lists.state: frozen` → every withheld subject leg unblocks.

**Authorization (REQ-XH-004).** Wc5 is **$0 model** (`model_line: none`; `deliver-secret` ABSENT). **Compute**
verbatim and separate: quote **`pq-4183fe9dc342aa10`**, ceiling **2,565,580 µ$ (~$2.57)**; §3.10 hard-cap
`r·(D + 1h restart) + S·r_disk·h_r + 0.12·E_gib` = `314,140·(6+1) + 300·137·6 + 0.12·1` = `2,198,980 +
246,600 + 120,000` = **2,565,580 µ$**.

**Shape.** D = **6 h** dead-man (Wc4's re-proof completed in ~4.6h — the pace is proven; 6h covers held+dev
comfortably); one ≤1h harvest-only restart; h_r 6h; C = 4; ~38 trials (14 `needs: wc_reproof` × committed
R); egress 1 GiB; e2-standard-8, debian-12, 300 GiB. The widened freeze lands verbatim as the new-epoch
record (`hh-w1 prove` reads `freeze_file`).

**The Σ-exceed (recorded verbatim).** The declared Σ of window ceilings was 19,793,630 µ$ ≤ $20. Wc2/Wc3/Wc4
pushed it to 26,233,810 µ$; Wc5 pushes it to **28,799,390 µ$ ≈ $28.80** — over the declared bound. The scope
amendments are authorized (the repair windows + the allowlist amendment). The windows' **actual** spend is
~$8.2M of declared ~$28.8M.

**Amendment scope (REQ-XB-116).** The declared-data amendment — epoch-2 successor `fp-w1-q-x55`
(`sha256:86def07e…`) adding exactly `archive.ubuntu.com`/`security.ubuntu.com`/`ports.ubuntu.com` to the
Debian-family set (the task env images' apt bootstrap); epoch-1 untouched. This is a declared-data
amendment, not machinery — recorded as a scope-decision.

**Dispositions.** `run` — the compute authorized verbatim; R6.45e dispatches for the Wc5 body (PROV → LOAD
→ the thrice-repaired runner + widened freeze → re-proof → frozen-list outcome → teardown → upload →
settle). `not_run` — the task list stays `blocked`, all subject legs withhold. Your verbatim answer
dispositions it.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decision (verbatim, received 2026-11-13T06:00:17Z, channel: interactive): "authorized, continue"
Disposition: PASSED — run; basis: "the compute authorized verbatim; R6.45e dispatches for the Wc5 body"
Recorded verbatim: Wc5 $0 model (model_line: none, deliver-secret ABSENT); compute pq-4183fe9dc342aa10,
ceiling 2,565,580 µ$ (~$2.57), formula 314,140x7 + 300x137x6 + 0.12x1; D=6h, restart<=1h, h_r 6h, C=4,
~38 trials; the re-proof on the thrice-repaired runner b1ba7ffdc… under the widened epoch-2 allowlist
(REQ-XB-116); Σ-exceed recorded (28,799,390 µ$ ≈ $28.80 — scope amendments authorized).
verbatim_sha256 = sha256_text("authorized, continue") = a8428147fe1c7c1577dfe062327cf41a19491a49daf68c9432b9f276f8569996 — stamped fp-wc5.json.
GATE DECISIONS row: 2026-11-13T06:00:17Z | GATE-G28e
Operator confirmation of agent-drafted text (verbatim, 2026-11-13T06:00:17Z): "authorized, continue"
— covers sha256:be7aa37f4ac8a6d8. Signed by: the operator. Recorded by Devin (SWE-2 High, coordinator),
which does not sign.
