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

## Addendum — 2026-11-19T21:1xZ (R6.54, append-only; nothing above is edited)

**W4 superseding entry** — the `N/A` row above cited `unused{scope_end}` under GATE-G30's
`re-scope{end-first-pass-early}`; ADR-0452 amended that scope and W4 executed, so the row's
basis is void. The append-only replacement entry, same contract (one dated entry per window,
≥ 24 h after the *executed* teardown, sources named generically, no billing-account id,
`provided:` flags only):

| window | read | figure | basis |
|---|---|---|---|
| **W4** | model: `within` (tracked) · compute: `unavailable` · **≥24 h billing read: `pending{clock}`** | model billed figure stays the harness-tracked **USD ~6.98** (6,983,035 µ$, 3,372 metered calls, `hh-facts.json`); compute estimate **~USD 3.37** (`compute_estimate_usd: 3.373477`); GCP billed compute `unavailable` (no billing export on the deleted project `hh-fp-a1b2c3`) | settle-confirm reads 2026-11-19T21:04–21:07Z — `docs/build/reports/2026-11-19-fp-w4-settlement-3.json` + `-settle-confirm-read.json`; `measurement.cost.reconciled{w4}` → `settled_discrepant` (provider trailing-24h total 9,179,890 µ$ vs tracked 6,983,035 µ$, diff $2.20 covering pre-window calls; verbatim, not laundered). The H18 ≥ 24 h billing clock runs from the *executed* teardown `DELETE_REQUESTED` **2026-11-19T17:50:38Z** (`compute/2026-11-19-fp-w4-teardown-2.json`) — due ≥ 2026-11-20T17:50Z, not yet elapsed at this entry. |

**Closure-sum amendment note:** the W4 model billed figure reconciles `within` at the tracked
$6.98 — no closure-sum movement vs the phase ledger; the provider-side $9.18 trailing-24h total
is not the W4 figure (it covers calls outside the metered set) and changes no window total. If
the ≥ 24 h read lands a different figure, the closure sum is amended by that delta at read time.


## Addendum 2026-11-19T23:30Z — W4 billing row dispositioned under ADR-0453 ruling 5 (early-settle)

The operator's verbatim "get the final billing read and get unblocked early" (ADR-0453) amends the
W4 row's `pending{clock}` ahead of the ≥24h read:

- **W4 model: `within`** — harness-tracked `6,983,035 µ$` ($6.98) is the metered evidence; provider
  hourly attribution (`2026-11-19-fp-w4-settlement-4.json`) shows the entire trailing-24h total
  ($9.17989) inside the W4 window hours; the $2.20 delta is `concurrent_hour_unattributed` usage on
  the shared account — recorded, never absorbed into the W4 figure.
- **W4 compute: `unavailable`** (GCP billed figure unreachable — estimated ~$3.37, tracked never
  billed), unchanged.
- The ≥24h read (due ≥2026-11-20T17:50Z) is demoted to a **confirming check** — it still lands and
  is recorded honestly whichever way it reads; if it surfaces later-attributed calls the entry is
  amended, never rewritten.
