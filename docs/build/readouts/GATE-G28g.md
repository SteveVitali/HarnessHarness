# GATE-G28g — PROV-Wc7: authorize the seventh repair window's compute (D=8h re-proof on the 4-times-repaired runner under epoch-3) · readout
Status: PENDING   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. Wc7 is a
scope-amendment window — a SEVENTH repair window — authorized by the operator's scope-decision. This readout
carries its compute authorization for REQ-XH-004.

## Evidence (per item: source, `date -u` of the read, layer reached)

- Wc7 declaration — `scripts/gce-backend/declarations/fp-wc7.json` (`gate PROV-Wc7`, quote
  `pq-5bea73a4613ebdad`, `model_line: none`, ceiling 3,193,860 µ$, `verbatim_sha256` unstamped) — read
  2026-11-14.
- Wc7 quote — `docs/build/reports/2026-11-14-fp-wc7-quote.json` (planning 732,994 µ$, bound 2,096,465 µ$,
  ceiling 3,193,860 µ$) — read 2026-11-14.
- The machinery — the six-defect-repaired runner `b8310b68` (egress + pkill + verifier + duplicate-Host);
  epoch-3 widened allowlist `sha256:787db9e1…` — layer: committed code + records.
- The trigger — `task_lists.state: blocked` (Wc6's re-proof completed but every H-allow cell produced `0`
  because the duplicate-Host proxy bug 400'd the verifier's apt/curl/uv bootstrap) — layer: committed
  records.

<!-- agent-drafted:begin sha256=7948c52349a3ef3d -->
## Decision content for the operator

**The scope.** Wc7 is the **seventh** repair window — the re-proof on the six-defect-repaired runner
(`b8310b68`) under the **epoch-3** widened allowlist (`sha256:787db9e1…`) at **D=8h**. Wc6 proved the
machinery completes and the declared data is right — but surfaced a REAL 6th machinery defect: the
mediated-egress transports injected `Host:` then re-appended the client's own `Host:` → duplicate `Host:`
→ origin `400` → the verifier's `apt`/`curl`/`uvx` bootstrap failed → pytest never ran → `reward.txt = 0`
(denied_hosts `[]` — the 400 was the origin's, not a deny). That defect is repaired (`b8310b68`, with
verifier-stream persistence → `bootstrap:*` classification). Epoch-3 adds `release-assets.githubusercontent.com`
to `verify` (the uv-install 302 target). With the proxy fixed + the mirror host covered, the verifier's
bootstrap installs → pytest runs → real `reproofed` verdicts → `task_lists.state: frozen`.

**Authorization (REQ-XH-004).** Wc7 is **$0 model** (`model_line: none`; `deliver-secret` ABSENT). **Compute**
verbatim and separate: quote **`pq-5bea73a4613ebdad`**, ceiling **3,193,860 µ$ (~$3.19)**; §3.10 hard-cap
`r·(D + 1h restart) + S·r_disk·h_r + 0.12·E_gib` = `314,140·(8+1) + 300·137·6 + 0.12·1` = `2,827,260 +
246,600 + 120,000` = **3,193,860 µ$**.

**Shape.** D = 8 h dead-man (the proven real-work pace); one ≤1h harvest-only restart; h_r 6h; C = 4;
~38 trials (14 `needs: wc_reproof` × committed R); egress 1 GiB; e2-standard-8, debian-12, 300 GiB. The
epoch-3 freeze lands verbatim (`hh-w1 prove` reads `freeze_file`).

**The Σ-exceed (recorded verbatim).** The declared Σ of window ceilings was 19,793,630 µ$ ≤ $20. The repair
windows pushed it to 31,993,250 µ$; Wc7 pushes it to **35,187,110 µ$ ≈ $35.19** — over the declared bound.
The scope amendments are authorized (the repair windows + the allowlist amendments). The windows' **actual**
spend is ~$12M of declared ~$35.2M.

**Dispositions.** `run` — the compute authorized verbatim; R6.45g dispatches for the Wc7 body (PROV → LOAD
→ the six-defect-repaired runner + epoch-3 freeze → re-proof at D=8h → frozen-list outcome → teardown →
upload → settle). `not_run` — the task list stays `blocked`, all subject legs withhold. Your verbatim answer
dispositions it.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
