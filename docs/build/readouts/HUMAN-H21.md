# HUMAN-H21 readout — round-7 billing reconcile
Status: PASSED   <!-- the ONLY line that may change; set at signature -->

> One dated entry per window, ≥ 24 h after TEARDOWN: `within` / `outside` / `unavailable` / `N/A`
> with cited unused records. Billed figures in the phase spend ledger's unit; source named
> generically; billing-account id never enters (REQ-XB-219); credentials `provided: yes/no` only.

## Criterion (verbatim)

One dated billing/settlement entry per executed Round-7 window (X0+X1 exploration; held-out
confirmation), each a billing read ≥ 24 h after that window's teardown, appended as soon as
it is due — the H18 consolidated pattern. Entries record billed figures verbatim (model and
compute separately), reconcile against the gate's recorded quote/hard cap, and confirm
teardown. Billed-figure claims stay `pending{clock}` until their entry lands.

## Per-window entries (reads UTC 2026-11-26T07:1x–07:1xZ; sources named generically)

| window | read | figure | basis |
|---|---|---|---|
| **X0+X1** (`p4x0`/`p4x1`, R7.3) | model: `within` · compute: `unavailable` | model recorded **USD 21.08** (X0 1,834,017 + X1 19,250,389 µ$ WAL-authoritative) vs provider billed window-span days **USD 24.21** (Nov-21 $1.85, Nov-22 $0.00, Nov-23 $6.37, Nov-24 $15.99 — Nov-24's bucket is boundary-mixed with P4-C w1's first ~3.4 h launched 20:40Z, plus shared-account non-window usage Nov-19/20 $18.80 excluded verbatim) — provider-side exceeds recorded consistent with the concurrent-usage/boundary class; recorded figure stands. Compute: no billed API read exists — recorded estimates stand (X1 $9.00 in-envelope + dead-man idle; quotes `pq-4263f129db914f88`/`pq-59a49747bfa5bdf0`) | provider usage API 7d read `2026-11-26-fp-p4x-settlement.json`; ≥24 h post-teardown clock elapsed (X1 teardown Nov-24) |
| **P4-C** (`p4c`, R7.5) | model: `within` · compute: `unavailable` · **≥24 h confirming read: `pending{clock}`** | model **WAL-authoritative USD 16.90** (16,896,165 µ$ — recomputed pre-teardown from the retained disk, sum of `measurement.cost.attributed` over runs r1/r2/r3/r4; per-slot sum $16.64 stands with the known undercount class) vs provider billed window-span ≈ **USD 16–18** (Nov-24 boundary share + Nov-25 $11.77 + Nov-26 $2.46; 24 h trailing read $10.80082 covers the r2–r4 legs) — consistent within at daily granularity. Compute: `unavailable` — recorded estimates $3.86 (`p4c` quote, 31.6 %) + $6.47 (`p4c-r2` amendment, 40.4 %) stand | provider usage API 24 h + 7d reads `2026-11-26-fp-p4c-settlement.json`; **teardown confirmed** — `gcloud compute instances delete hh-p4-p4c` ~07:26Z Nov-26 removed instance + 300 GB boot disk (autoDelete), zero billable resources (instances/disks/addresses all empty); ≥24 h confirming read due ≥ 2026-11-27 ~07:26Z — demoted to a confirming check (no post-teardown usage possible); amends, never rewrites, if it reads differently |

`unavailable` reads are recorded as such — not failures; CAP7.2 folds them (the closure
recompute uses billed figures where they exist, estimates flagged otherwise).
`OLLAMA_API_KEY`: provided yes, read at use, never printed/stored.

## Signature   <!-- appended at signing; nothing above is edited or deleted -->

- **Operator:** Steve Vitali · **UTC:** 2026-11-26T07:3xZ · **Channel:** operator CLI session.
- **Verbatim:** "proceed with B" — verbatim_sha256 `a6a80923a2c355bcb7c35472fd327fe23b844db25ece1c09d7cb6232a77da6e7`;
  elected the recompute-then-delete teardown leg and agent execution of the provider reads
  (the "unblock you and for you to proceed" posture in the same message) — the parked-disk
  residual is closed by the WAL-authoritative recompute above and the delete is the
  operator's teardown ruling, recorded as executed.
- Layer: `signed` — two entries complete; no fabricated figures; `unavailable`/`pending{clock}`
  are landed tokens.

## Amendment 2026-11-28 — P4-C ≥ 24 h confirming read landed (`pending{clock}` resolved)

The clock-gated confirming read executed 2026-11-27T07:30–07:31Z (≥ 24 h after the
~07:26Z Nov-26 teardown, at the recorded mark). It reads **clean**:

- Provider usage trailing-24 h: **20 µ$ = $0.00002** total — one 2-request bucket
  2026-11-26T11:00–12:00Z (glm-5.3-flash, 38 in / 19 out tokens — ping-sized,
  consistent with the R7.9 uncached-token probe's 2 live calls that hour;
  no window spend). Read `docs/build/reports/2026-11-27-fp-p4c-settle-confirm-read.json`.
- Purchased balance: **USD 3.42963** — down exactly $0.00002 from the settle record's
  observed 3.42965; the delta reconciles the 20 µ$ to the micro.
- GCP compute: project `hh-fp-a1b2c3` now returns **not found** — the project itself
  is deleted; strictly stronger than the earlier zero-billables read. No billed
  compute possible.

The P4-C entry's `pending{clock}` is hereby **resolved** — the settlement stands as
recorded: model `within` (WAL-authoritative USD 16.90), compute `unavailable`
(recorded estimates stand), teardown confirmed. Wrapped record:
`docs/build/reports/2026-11-27-fp-p4c-settlement-confirm.json` (+ `.sha256` sidecar).
Per the criterion the original entry is not rewritten — this amendment governs.
