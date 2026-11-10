# GATE-G33 — PROV-W1 re-presented: re-authorize the W1 window on the corrected declaration ($0 model) · readout
Status: PASSED

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion (verbatim from the spec)

Spec §10.3's PROV-W1 row applies again, verbatim, on the corrected declaration — REQ-XH-004 requires a gate row before every paid execution ticket; the first attempt's verbatim authorized a declaration whose `packages` cannot be provisioned, so a materially corrected declaration is re-presented rather than silently substituted (no silent provider/backend substitution):

> | PROV-W1 | first | SELECT commit + P2-infra + HUMAN billing alert | compute quote (planning, bound, ceiling with restart, h_r and egress terms); dead-man action (Q-X38) and the pre-authorized restart (Q-X60); network posture (Q-X55); re-proof scope (REQ-XH-116); layer-export size estimate against W1's egress line (a quote above it refuses, REQ-XH-118) and the controller's free disk; Q-X39b |

## Evidence (per item: source, `date -u` of the read, layer reached)
- Corrected declaration — `scripts/gce-backend/declarations/fp-w1.json` sha256:dd021b1fc8033289 — read 2026-11-09T23:45Z — landed (R6.32a, merged 1692de8)
- Prior admitted declaration — GATE-G25's stamp sha256:2502b777…, recorded in readouts/GATE-G25.md
- Declaration repair record — `docs/build/runs/R6.32a.md` (apt-cache verification transcript on `debian:bookworm` amd64; startup-script diagnosis: duplicate `--metadata-from-file` occurrences, the second replaced the first — fixed to one comma-separated occurrence) — read 2026-11-09T23:45Z — landed
- Declaration + battery — `validate_decl` valid; `check-gce-backend.sh` → 113/113 — run at presentation
- First-attempt spend — `docs/build/reports/compute/2026-11-09-fp-w1-cost.json` regenerated `hh-compute-cost/1`: 784,750 µ$ (~$0.78), true VM lifecycle 742 s — read 2026-11-09T23:45Z — committed
- Quote — `comparative-quote/1` row w1, quote id `pq-3e4ad6c78a564b77`, ceiling 5,050,420 µ$ — re-verified byte-identical by `check-fp-select.sh` on the corrected declaration (packages is not a quote input)
- Controller free disk — `df -g .` → 48 GiB — read 2026-11-09T23:45Z — controller-local
- Standing terms — GATE-G25 readout PASSED 2026-11-09T20:16Z; HUMAN-H13 COMPLETE ($20 @ 50/90/100 %)

<!-- agent-drafted:begin sha256=959314c5d9c468d92eee645a3b29d29b2b8db76a18d107f8081726227a2c5d09 -->
## Decision content for the operator

**1. The delta, honestly.** `fp-w1.json` member-by-member vs the declaration GATE-G25 admitted:
- `packages` — 4 pins re-cut to versions that resolve on `debian-12-bookworm-v20261014` (verified by `apt-cache policy`/`apt-get -s install` on `debian:bookworm` amd64, the declared arch): `podman` `…+deb12u1` → `…+deb12u1+b3` · `conmon` `…+deb12u1` → `2.1.6+ds1-1` · `netavark` `1.4.0-4` → `1.4.0-3` · `podman-compose`/`compose_frontend` `1.0.6-2` → `1.0.3-3`; `crun` unchanged.
- `runner.commit`, `authorization.verbatim_sha256` — re-zeroed (re-stamped at the next PROV / this gate).
- `gce-backend.sh` — one `--metadata-from-file` with comma-separated keys (the duplicate-occurrence bug; `startup-script` lands for real).
- **Unchanged:** `quote_id`, `ceiling_micros`, `dead_man`, `h_r_hours`, `network`, `machine_type`, `image`, `work_items`, labels, `quote_inputs`, `rate_table` — the W1 quote recomputes byte-identically.

**2. Spend accounting.** First attempt consumed **784,750 µ$ (~$0.78)** of the $5.05 ceiling (742 s VM lifecycle — PROV + TEARDOWN receipts + regenerated cost record committed; `billed.status: pending`). A re-run carries the same ceiling class (~5,050,420 µ$ worst case) → worst-case W1 total ≈ **5,835,170 µ$ (~$5.84)** — inside the $20 compute authorization (~$19.22 headroom). The Σ of the five declared window ceilings is unchanged (19,793,630 µ$ ≤ 20,000,000): the re-run is the same W1 window re-attempted, not a sixth declared window.

**3. Authorizations restated.** Model authorization: **none — $0 model** (no `deliver-secret`, no key delivery). Compute authorization: the W1 ceiling **$5.05 (5,050,420 µ$)** inside the $20 billing authorization. Quote id `pq-3e4ad6c78a564b77`. Hard-cap formula `r·(D + 1 h restart) + S·r_disk·h_r + 0.12·E_gib` = `314,140 · 13 + 300·137·6 + 0.12·6` = 4,083,820 + 246,600 + 720,000 = 5,050,420 µ$.

**4. Standing terms reconfirmed.** Dead-man STOP 12 h; h_r = 6 h teardown deadline; one ≤ 1 h harvest-and-teardown-only restart inside the ceiling; teardown across every operator-gate wait; network Q-X55 verbatim (agent phase: model proxy + apt/PyPI/npm/crates only, no github.com; grading reaches verifier hosts; per-slot egress log); re-proof scope REQ-XH-116 (the SELECT commit still predates); Q-X39b's recorded answer stands — the `reproof` `live-verify.sh` leg is registered so the re-proof can earn `live-executed`.

**5. Lessons carried.** `receipt_lib` normalizes RFC3339 offsets → `Z` at emission; `check-compute-receipts.sh` routes `hh-compute-cost/1` to `costvalidate`; the committed `hh_w1` driver + `run_leg_reproof` wiring carry over unchanged — the re-run executes the landed driver, not a rewrite.

**Dispositions.** `PASSED` re-authorizes W1 on the corrected declaration and stamps its `verbatim_sha256`; or refusal (the chain stops). The re-run's worst-case spend and the $20 accounting are as stated above.
<!-- agent-drafted:end -->

## Signature
Operator decision (verbatim, received 2026-11-09T23:55Z via interactive chat): "authorized, continue"
Dispositions recorded from the verbatim: (a) PROV-W1 re-presented PASSED — provisioning of the corrected declaration authorized (quote pq-3e4ad6c78a564b77, ceiling 5,050,420 µ$); the standing terms (dead-man STOP 12 h, h_r 6 h, one <= 1 h harvest-and-teardown-only restart, teardown across gate waits, Q-X55 posture, re-proof scope, Q-X39b answer) reconfirmed as presented.
verbatim_sha256 = sha256_text("authorized, continue") = a8428147fe1c7c1577dfe062327cf41a19491a49daf68c9432b9f276f8569996 — stamped into fp-w1.json `authorization.verbatim_sha256` (declaration sha256 after stamp: recorded below at commit; the identical words yield the same hash as GATE-G25's stamp — the readout_path + gate differentiate the authorization).
GATE DECISIONS row: 2026-11-09T23:55Z | GATE-G33
Operator confirmation of agent-drafted text (verbatim, 2026-11-09T23:55Z): "authorized, continue" — covers sha256:959314c5d9c4
Signed by: the operator. Recorded by Devin (SWE-2 High, coordinator), which does not sign.

