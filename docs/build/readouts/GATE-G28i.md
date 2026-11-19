# GATE-G28i — PROV-SMOKE: the non-evidence smoke envelope's compute authorization (R-3 — up to 3 smoke windows on the exact window recipe, $0 model, results never re-proof evidence) · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket
authorizes spend. The smoke window class is **new** — it exists under amendment row A1 (R-3, ADR-0443), not
the spec's declared §10.3 window table: a smoke is declared compute under an envelope, $0 model,
non-evidence — never a declared spec window and never silently treated as one.

## Evidence (per item: source, `date -u` of the read, layer reached)

- Smoke declarations — `scripts/gce-backend/declarations/fp-smoke{1,2,3}.json` (gate `PROV-SMOKE`,
  `model_line: none`, `deliver-secret` ABSENT, `verbatim_sha256` all-zeros pending the gate stamp;
  `validate_decl.py` valid ×3) — read 2026-11-16 — layer: committed records.
- Smoke quotes — `docs/build/reports/2026-11-16-fp-smoke{1,2,3}-quote.json` (`hh-compute-quote/1` via the
  one quote path `receipt_lib.py`; planning 281,358 µ$, bound 554,269 µ$, ceiling 830,480 µ$ each) — read
  2026-11-16 — layer: committed records.
- The rulings — LEDGER `## GATE DECISIONS` `intervention-2026-11-15` rows R-3 (amended), R-8, R-10 — read
  2026-11-16 — layer: committed records.
- The amendments — `docs/experiments-spec.md` §10.5 rows A1 (ADR-0443, the `smoke` window class) and A3
  (ADR-0445, gate content + prediction track + post-Wc9 stop) — read 2026-11-16 — layer: committed records.
- The machinery — chain tip `27ac8db9…` carrying R6.45j (D15/D17/D18/D19) + R6.45k (D8–D14, the
  `egress-real-clients` leg GREEN on fixed tip `54ae9c9e` under `hh-live-evidence/1`, GHA x86_64 run
  43791844390); epoch-4 minted `sha256:785d7092…` — layer: committed code + live evidence.

<!-- agent-drafted:begin sha256=af298c3b8b78d77c -->
## Decision content for the operator

**Compute authorization — amended R-3 (verbatim from the GATE DECISIONS row, `intervention-2026-11-15`).**
"up to 3 non-evidence smoke windows on the exact recipe — $0 model, compute under a gate envelope,
in-window code changes permitted, results labelled non-evidence smoke, never re-proof evidence" — amended
terms: each ceiling ≤ 1 250 000 µ$ by the §3.10 formula, Σ ceilings ≤ 3 500 000 µ$. And the envelope rule
as relayed in the same session: "This is an envelope, not a pre-answer: present each smoke's declaration,
quote id and cap formula in GATE-G28i, and I will answer each one verbatim."

**$0 model.** Every smoke is `model_line: none` — `deliver-secret` ABSENT; a model call refuses
(REQ-XB-219's rule applies verbatim).

**The envelope — each smoke's declaration, quote id and §3.10 cap formula**
(`ceiling = r·(D + H_rs) + S·r_disk·h_r + 0.12·E_gib`, re-minted from the pinned live rate table
`pricing.json` `sha256:045e0b86…`; r = 268,040 + 150·137 + 5,000 = 293,590 µ$/h):

| window | declaration | quote id | cap formula (µ$) | ceiling |
|---|---|---|---|---|
| smoke1 | `fp-smoke1.json` | `pq-950d9be85a360626` | 293,590·(1.5+0.5) + 150·137·6 + 120,000·1 = 587,180 + 123,300 + 120,000 | 830,480 |
| smoke2 | `fp-smoke2.json` | `pq-30c1a03d1dce50ea` | 293,590·(1.5+0.5) + 150·137·6 + 120,000·1 = 587,180 + 123,300 + 120,000 | 830,480 |
| smoke3 | `fp-smoke3.json` | `pq-b2ed8578ab9ede1b` | 293,590·(1.5+0.5) + 150·137·6 + 120,000·1 = 587,180 + 123,300 + 120,000 | 830,480 |
| **Σ ceilings** | | | | **2,491,440** |

**Envelope check: each 830,480 ≤ 1,250,000 µ$; Σ 2,491,440 ≤ 3,500,000 µ$ — the envelope holds.** (The
insert-spec example's ≈871,580 carried the Wc-recipe 300 GiB hourly; at the declared 150 GiB boot disk the
live-table re-mint is 830,480 — the re-mint governs.) A quote outside the envelope stops for a separate
operator answer.

**Identical shape per smoke** (the exact window recipe): `mode: full`, e2-standard-8, debian-12, the
declared podman/crun/netavark pins, IAP tcp:22 ingress only, `service_account: none`, boot disk 150 GiB,
D = 1.5 h dead-man (in-guest 105 min), ≤ 0.5 h harvest-only restart, h_r 6 h, C = 4, egress 1 GiB
(estimated), trials 5 — H-allow r = 1 on `prove-plus-comm`, `torch-tensor-parallelism`, `query-optimize`
and `pytorch-model-cli` (epoch-4 minted `sha256:785d7092…`), H-pub r = 1 on `query-optimize` (checks the
D15/D17 fix on the non-mediated surface). Declared starting commit `27ac8db9…` — the chain tip carrying
R6.45j + R6.45k. `verbatim_sha256` all-zeros — stamped at signing.

**Smoke-runner rule (R-10, verbatim).** "In a smoke window only, the runner may be rebuilt in-window from
commits on the smoke branch. The PROV runner-commit match applies at provisioning to the declared starting
commit. Every in-window rebuild records its commit and the sha256 of each rebuilt binary in the smoke
record. The carried-binary sha256 pin is waived for smoke windows only, and never for evidence windows.
Every in-window change must land with its regression before Wc9, and Wc9 runs on one declared, pinned
commit."

**The A1 label (verbatim).** Smoke results are labelled "non-evidence smoke (internal; never re-proof
evidence)": never folded into `hh-reproof-packet/1` or `hh-reproof-register/1`, never cited as re-proof or
parity evidence. Evidence windows keep every existing rule unchanged.

**Re-scope ladder (A3 / R-8 — the §6 options).** `run` — the evidence window as specified · `not_run` — the
task list stays `blocked` · `re-scope{harden-again}` — another R6.45k-style insert plus a new smoke
envelope, for a newly named defect class · `re-scope{exclude-and-replace}` — only where the A5 rule makes an
exclusion legitimate (named denied host / O-fail), replacement from the seeded order per FP-7/REQ-XH-116 ·
`re-scope{end-first-pass-early}` — record the re-proof outcome as the first-pass result and skip W4, naming
the claims withheld.

**Prediction-track table (A3 / R-8 — what each readout predicted vs what happened).**

| window | the readout predicted | what actually happened |
|---|---|---|
| Wc | the W1′ repair + re-proof produces the frozen list | re-proof executed but 0/14 covering — surfaced the third defect (the mediated-egress leg never mediated; empty `denied_hosts`) → `blocked` |
| Wc2 | the egress repair → `reproofed` → frozen | repair verified live (requested/decided rows land); 3 h dead-man at 28/30 + the `pkill` cell-terminate defect → `blocked` |
| Wc3 | `pkill` fix + D=5h → frozen | dead-man again at 28/30; every H-allow cell `verifier_unavailable:reward_missing` (the verifier sink defect) → `blocked` |
| Wc4 | verifier fix + D=6h → frozen | COMPLETED (~4.6 h), all repairs verified live; every H-allow cell `exclude:runner_infeasible:allowlist_gap` (epoch-1 Debian vs Ubuntu noble) → `blocked` |
| Wc5 | the epoch-2 widened allowlist → frozen | allowlist verified live (real `reward.txt`, zero denied hosts) but 29/30 at the 6 h STOP (`query-optimize` in flight); dev never started → `blocked` |
| Wc6 | D=8h covers the proven pace → frozen | machinery completes; 6th defect — duplicate `Host:` → origin `400` → bootstrap fail → `reward.txt = 0`; epoch-3 host gap identified → `blocked` |
| Wc7 | six-defect runner + epoch-3 → frozen | duplicate-`Host` verified live; 7th defect — the 8 MiB forward body cap `502`s the noble `Packages` index → `reward.txt = 0` → `blocked` |
| Wc8 | seven-defect runner + epoch-3 at D=8h → frozen | `early_stop:operator_ruling R-1` ~23:20Z — first H-allow cells all `verifier_unavailable:bootstrap:signature` (the D8/D9 allowed-CONNECT starve) → `blocked` |

Each prediction held on a new machinery assumption; each window verified that assumption and found the
next defect. The smoke loop exists because Wc8's assumption failed early enough that R-1's early-stop paid
for itself in ~20 minutes — the smokes buy the same information cheaply before Wc9's evidence spend.

**Dispositions — your verbatim answer per smoke (the agent signs nothing).**
`envelope_authorized` — the named smoke may PROV under this envelope; R6.45l dispatches the smoke loop,
each smoke starting with what the previous one learned, written down first. `not_run` — the envelope is not
exercised; GATE-G28j then decides Wc9 on the Wc8 outcome and the conformance leg alone. A minted quote
outside the envelope stops for a separate operator answer — never silently re-scoped.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
<!-- Appended only when the operator has answered; an agent never writes these lines on its own authority:
Operator decision (verbatim, received <date -u +%FT%TZ> via <channel>): "<exact words>" — one answer per smoke
Disposition: <envelope_authorized | not_run> per smoke; basis: "<words>"
Recorded verbatim: smokeN $0 model (model_line: none, deliver-secret ABSENT); compute pq-…, ceiling 830,480 µ$;
verbatim_sha256 = sha256_text("<exact words>") — stamped fp-smokeN.json per answer.
GATE DECISIONS row: <date -u +%FT%TZ> | GATE-G28i
Operator confirmation of agent-drafted text (verbatim, <date -u +%FT%TZ>): "<words>" — covers sha256:af298c3b8b78d77c
Signed by: the operator. Recorded by <harness/model>, which does not sign.
-->

> **Signature (Reading 1, 2026-11-16)** — verbatim "continue" (operator, interactive) in response to the GATE-G28i presentation of smoke1/smoke2/smoke3 (quotes `pq-950d9be85a360626`/`pq-30c1a03d1dce50ea`/`pq-b2ed8578ab9ede1b`, each 830,480 µ$ ≤ 1,250,000; Σ 2,491,440 ≤ 3,500,000 µ$). Disposition: **`envelope_authorized` — run the smoke loop** (R6.45l dispatches smoke1 first, evaluating to SMOKE-PASS or envelope exhaustion → `blockedOn`). The envelope's `verbatim_sha256` stamps onto `fp-smoke{1,2,3}.json`; each smoke's quote stays under the envelope.
