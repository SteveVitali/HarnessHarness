# HUMAN-H18 readout — round-6 billing reconcile
Status: PASSED   <!-- the ONLY line that may change; set at signature -->

> One dated entry per window, ≥ 24 h after TEARDOWN: `within` / `outside` / `unavailable` / `N/A`
> with cited unused records. Billed figures in the phase spend ledger's unit; source named
> generically; billing-account id never enters (REQ-XB-219); credentials `provided: yes/no` only.

## Criterion (verbatim)

Spec §10.3 HUMAN row "Reconcile window Wn": billing read ≥ 24 h after each TEARDOWN for every
window (W1–W4 and Wc) → `within` / `outside` / `unavailable`; round closure totals. Contributes to
REQ-XH-110 (owner CAP6.2 — the closure recompute uses the billed figure wherever an entry exists).

## Per-window entries (read UTC 2026-11-19T04:2x–04:4xZ; sources named generically)

| window | read | figure | basis |
|---|---|---|---|
| **W1** | `within` | model line: Ollama provider API + operator console agree — month-to-date **USD 4.52** (trailing 7d/30d identical read; tracked Ws1 ~3.51 + smokes ≈ consistent) | provider usage API read 04:23Z (`2026-11-19-ws1-settle-confirm-read.json`, trailing-24h $0.34 tail) + operator console verbatim "$4.52 of Ollama billing so far this month" |
| **W2** | model: `within` (same $4.52 month total covers W2's Ollama model line) · compute: `unavailable` | GCP billed compute: no API-read exists (deleted project `hh-fp-a1b2c3`; no billing export wired) — tracked cost_lib estimate ~$29.7 recorded as estimate, not billed | provider usage API (model); compute read attempted → unavailable |
| **W3** | compute: `unavailable` · model: `N/A` | W3 was compute-authorized, `model_line: none` (R6.44: "$0 model window") — no model figure exists to reconcile; GCP billed compute `unavailable` as above | R6.44 run record + fp-w3 declaration |
| **Wc** | model: `within` ($4.52 total covers Wc windows) · compute: `unavailable` | same basis as W2 | provider usage API; GCP read unavailable |
| **W4** | `N/A` | `unused{scope_end}` — W4 never provisioned; project deletion receipt `docs/build/reports/compute/2026-11-19-fp-w4-teardown.json` UTC `2026-11-19T02:01:58Z` cited | R6.47 teardown + `n/a{scope_end}` records |

`unavailable` reads are recorded as such — not failures; CAP6.2 folds them (REQ-XH-110 recompute
uses billed figures where they exist, estimates flagged otherwise). `OLLAMA_API_KEY`: provided yes,
read at use, never printed/stored. `HF_WRITE_TOKEN`: provided yes (H17, separate).

## Signature   <!-- appended at signing; nothing above is edited or deleted -->

- **Operator:** Steve Vitali · **UTC:** 2026-11-19T04:45Z · **Channel:** operator CLI session.
- **Verbatim:** "I'm only seeing $4.52 of Ollama billing so far this month. but you can confirm for
  yourself what was billed however you like. regardless, I authorize you to continue" — operator
  console figure recorded ($4.52, confirmed identical via provider usage API); agent execution of
  the remaining provider reads authorized verbatim.
- Layer: `signed` — five entries complete; no fabricated figures; `unavailable` is a landed token.
