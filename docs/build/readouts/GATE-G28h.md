# GATE-G28h — PROV-Wc8: authorize the eighth repair window's compute (D=8h re-proof on the 7-defect runner under epoch-3) · readout
Status: PENDING   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. Wc8 is a
scope-amendment window — an EIGHTH repair window — authorized by the operator's scope-decision. This readout
carries its compute authorization for REQ-XH-004.

## Evidence (per item: source, `date -u` of the read, layer reached)

- Wc8 declaration — `scripts/gce-backend/declarations/fp-wc8.json` (`gate PROV-Wc8`, quote
  `pq-b14f50e9fd44e669`, `model_line: none`, ceiling 3,193,860 µ$, `verbatim_sha256` unstamped) — read
  2026-11-15.
- Wc8 quote — `docs/build/reports/2026-11-15-fp-wc8-quote.json` (planning 732,994 µ$, bound 2,096,465 µ$,
  ceiling 3,193,860 µ$) — read 2026-11-15.
- The machinery — the seven-defect-repaired runner `abc8d850` (egress + pkill + verifier + duplicate-Host +
  max_body `FORWARD_MAX_BODY = 128 MiB`); epoch-3 widened allowlist `sha256:787db9e1…` — layer: committed
  code + records.
- The trigger — `task_lists.state: blocked` (Wc7's re-proof completed but every H-allow cell produced `0`
  because the 8 MiB forward cap rejected Ubuntu's `noble/universe` `Packages` → `502` → bootstrap failed)
  — layer: committed records.

<!-- agent-drafted:begin sha256=27bdda6e3b96ced8 -->
## Decision content for the operator

**The scope.** Wc8 is the **eighth** repair window — the re-proof on the seven-defect-repaired runner
(`abc8d850`) under the **epoch-3** widened allowlist (`sha256:787db9e1…`) at **D=8h**. Wc7 verified the
duplicate-Host repair live — but surfaced a REAL 7th defect: the mediated forward's `read_response_full`
8 MiB `max_body` cap rejected Ubuntu's `noble/universe` `Packages` index (~15 MB) → `502` → the verifier's
`apt`/`curl`/`uvx` bootstrap failed → `reward.txt = 0` (denied_hosts `[]` — the 502 was the forward's body
cap, not a deny). That defect is repaired (`abc8d850` — `FORWARD_MAX_BODY = 128 MiB`, ~4-8× headroom over
the real apt indexes, regression-tested). With the proxy headers AND the body cap fixed, the verifier's
bootstrap installs → pytest runs → real `reproofed` verdicts → `task_lists.state: frozen`.

**Authorization (REQ-XH-004).** Wc8 is **$0 model** (`model_line: none`; `deliver-secret` ABSENT). **Compute**
verbatim and separate: quote **`pq-b14f50e9fd44e669`**, ceiling **3,193,860 µ$ (~$3.19)**; §3.10 hard-cap
`r·(D + 1h restart) + S·r_disk·h_r + 0.12·E_gib` = `314,140·(8+1) + 300·137·6 + 0.12·1` = `2,827,260 +
246,600 + 120,000` = **3,193,860 µ$**.

**Shape.** D = 8 h dead-man (the proven real-work pace); one ≤1h harvest-only restart; h_r 6h; C = 4;
~38 trials (14 `needs: wc_reproof` × committed R); egress 1 GiB; e2-standard-8, debian-12, 300 GiB. The
epoch-3 freeze lands verbatim (`hh-w1 prove` reads `freeze_file`).

**The Σ-exceed (recorded verbatim).** The declared Σ of window ceilings was 19,793,630 µ$ ≤ $20. The repair
windows pushed it to 35,187,110 µ$; Wc8 pushes it to **38,380,970 µ$ ≈ $38.38** — over the declared bound.
The scope amendments are authorized (the repair windows + the allowlist amendments). The windows' **actual**
spend is ~$13.9M of declared ~$38.4M.

**Dispositions.** `run` — the compute authorized verbatim; R6.45h dispatches for the Wc8 body (PROV → LOAD
→ the seven-defect-repaired runner + epoch-3 freeze → re-proof at D=8h → frozen-list outcome → teardown →
upload → settle). `not_run` — the task list stays `blocked`, all subject legs withhold. Your verbatim answer
dispositions it.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
