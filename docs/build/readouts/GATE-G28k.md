# GATE-G28k — PROV-SMOKE post-repair diagnostic envelope: the non-evidence diagnostic smoke envelope's compute authorization (up to 2 smoke windows on the diagnostic recipe — pypi-server + make-doom-for-mips, H-allow r=1 + H-pub r=1 — $0 model, results never re-proof evidence) · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket
authorizes spend. This envelope executes GATE-G30's recorded `re-scope{harden-again}` disposition — "a
targeted pass on pypi-server's reward-0: a bounded smoke/diagnostic under a new envelope (the R-3 smoke
class is the existing instrument)" — under the A1 `smoke` window class: declared compute under an envelope,
$0 model, non-evidence — never a declared spec window and never silently treated as one. It is the
diagnosis record's prescribed verification, verbatim: "a post-repair smoke leg on pypi-server +
make-doom-for-mips verifies against the recorded signature."

## Evidence (per item: source, `date -u` of the read, layer reached)

- The scope decision — `docs/build/readouts/GATE-G30-postwc9-scope.md` Status **PASSED**, signature
  Reading 1 (operator verbatim "I authorized, proceed", 2026-11-16) → Disposition
  `re-scope{harden-again}`; LEDGER `## GATE DECISIONS` row — read 2026-11-16 — layer: committed records.
- The diagnosis — `docs/build/reports/2026-11-16-pypi-server-discordance-diagnosis.json`
  (`hh-discordance-diagnosis/1`): `residual_machinery_reparable` — `exec_guarded`'s undrained exec pipes
  produce the `agent_timeout` signature (~64 KiB bound); h_allow egress fully exonerated (identical
  signature on zero-mediated-tunnel H-pub); task provable (oracle ~121 s vs 900 s deadline) — read
  2026-11-16 — layer: committed records.
- The repair — `codex/harnessharness-exec-drain` tip `6801febffdfba7487598a505f076955ac568f04e` (PR #404):
  `0c4e0547` drain threads in `exec_guarded` + `4679635d` agent-stream persistence
  (`collect/agent-{stdout,stderr}.txt`); the XO-05 `exec_drain` battery fails 3 legs pre-fix / passes 4+1
  post-fix; `4679635d`'s XO-05e leg fails pre-fix — read 2026-11-16 — layer: committed code + test battery.
- Smoke declarations — `scripts/gce-backend/declarations/fp-smoke{4,5}.json` (gate `PROV-SMOKE-DIAG`,
  `model_line: none`, `verbatim_sha256` all-zeros pending the gate stamp; `validate_decl.py` valid ×2;
  `runner.commit` declared `6801febffdfba7487598a505f076955ac568f04e` — the repair tip) — read
  2026-11-16 — layer: committed records.
- Smoke quotes — `docs/build/reports/2026-11-16-fp-smoke{4,5}-quote.json` (`hh-compute-quote/1` via the one
  quote path `receipt_lib.py`; planning 269,125 µ$, bound 511,454 µ$, ceiling 830,480 µ$ each) — read
  2026-11-16 — layer: committed records.
- The amendments — `docs/experiments-spec.md` §10.5 rows A1 (ADR-0443, the `smoke` window class) and A3
  (ADR-0445, gate content + prediction track + post-Wc9 stop) — read 2026-11-16 — layer: committed records.

<!-- agent-drafted:begin sha256=e211ba90dfaa -->

**The authorization sought (under A1/R-3, bounded for the diagnostic).** GATE-G30's recorded
`re-scope{harden-again}` is the basis; this envelope is the compute authorization for the bounded
post-repair diagnostic:

- **$0 model** — `model_line: none`, no `deliver-secret`, no model key in-window, no metered calls
  (REQ-XB-219's rule applies verbatim).
- **Up to 2 smoke windows** on the exact diagnostic recipe (`hh-compute-decl/1` `mode: full`), each a
  distinct `window: smoke4|smoke5` — the verification smoke plus one contingency window on the identical
  recipe (retry on infra noise or a bounded follow-up if smoke4 is inconclusive; a newly named defect
  class is a §6-shaped readout, never a silently re-scoped third window).
- **Each ceiling ≤ 1 250 000 µ$** by the §3.10 formula; **Σ ceilings ≤ 2 500 000 µ$**. Minted:
  `pq-9b4d23564e37c0b1` (smoke4) and `pq-684bcc89e7f7701d` (smoke5), each **830,480 µ$** — Σ 1,660,960 µ$,
  inside the envelope.
- **Non-evidence (A1 verbatim).** Every artifact is labelled "non-evidence smoke (internal; never
  re-proof evidence)", never folded into `hh-reproof-packet/1` or `hh-reproof-register/1`, never cited as
  re-proof or parity evidence; `task_lists.state` stays `blocked` verbatim through this envelope — the
  smoke verifies machinery, it does not re-grade the member.

**Identical shape per smoke** (the diagnostic recipe): `mode: full`, e2-standard-8, debian-12, the
declared podman/crun/netavark pins, IAP tcp:22 ingress only, `service_account: none`, boot disk 150 GiB,
D = 1.5 h dead-man (in-guest 105 min), ≤ 0.5 h harvest-only restart, h_r 6 h, C = 4, egress 1 GiB
(estimated), trials 4 — **H-allow r = 1 on `pypi-server` and `make-doom-for-mips`; H-pub r = 1 on both** —
the two members carrying the recorded `agent_timeout` signature, on both planes it appeared on. Declared
starting commit `6801febffdfba7487598a505f076955ac568f04e` — the exec-drain tip carrying `0c4e0547` (drain
threads) + `4679635d` (stream persistence). `verbatim_sha256` all-zeros — stamped at signing.

**Smoke-runner rule (R-10, verbatim).** "In a smoke window only, the runner may be rebuilt in-window from
commits on the smoke branch. The PROV runner-commit match applies at provisioning to the declared starting
commit. Every in-window rebuild records its commit and the sha256 of each rebuilt binary in the smoke
record. The carried-binary sha256 pin is waived for smoke windows only, and never for evidence windows.
Every in-window change must land with its regression before Wc9, and Wc9 runs on one declared, pinned
commit." — for this diagnostic, "Wc9" reads as the next evidence window; the in-window-iteration arm is
not expected to be exercised (the repair is already landed and regressed — the smoke verifies it).

**Pass clauses (the recorded signature).** The smoke verifies the repaired capture path live against the
diagnosis's recorded signature:

1. every cell's agent exec completes — `exited`, never lapsed mid-stream (the pipe-bound freeze /
   return-hang signature absent);
2. the captured agent transcript exceeds the recorded ~64 KiB pipe bound end-to-end (pypi-server's oracle
   trajectory produces ~78 KB; make-doom-for-mips ~68 KB);
3. `collect/agent-{stdout,stderr}.txt` land on every cell — the `4679635d` observability leg, the
   byte-pinned transcript Wc9's harvest lacked;
4. mediator stores: zero 5xx, eof-only closes, `denied_hosts []` on the H-allow legs.

Reward values are reported, never required — the smoke verifies machinery, not task outcome: a cell whose
agent now completes but earns reward 0 is a `task_provability` datapoint for the next gate, never a
machinery failure of this repair.

**The A1 label (verbatim).** Smoke results are labelled "non-evidence smoke (internal; never re-proof
evidence)": never folded into `hh-reproof-packet/1` or `hh-reproof-register/1`, never cited as re-proof or
parity evidence. Evidence windows keep every existing rule unchanged.

**Re-scope ladder (A3 / R-8 — the §6 options).** `envelope_authorized` — the named smokes may PROV under
this envelope; R6.45n dispatches smoke4 first. `not_run` — the envelope is not exercised; the task list
stays `blocked` verbatim and the frozen-list continuation does not proceed. A minted quote outside the
envelope stops for a separate operator answer — never silently re-scoped. The post-diagnostic paths stay
as GATE-G30's ladder recorded them: verified machinery → the frozen-list continuation returns through its
own gate; a newly named defect class → a hardening insert plus a new envelope; `end-first-pass-early`
remains available at any later scope decision.

**Prediction-track table (A3 / R-8 — what each readout predicted vs what happened).**

| window | the readout predicted | what actually happened |
|---|---|---|
| Wc | the W1′ repair + re-proof produces the frozen list | re-proof executed but 0/14 covering — surfaced the third defect (the mediated-egress leg never mediated; empty `denied_hosts`) → `blocked` |
| Wc2 | the egress repair → `reproofed` → frozen | repair verified live (requested/decided rows land); 3 h dead-man at 28/30 + the `pkill` cell-terminate defect → `blocked` |
| Wc3 | `pkill` fix + D=5h → frozen | dead-man again at 28/30; every H-allow cell `verifier_unavailable:reward_missing` (the verifier sink defect) → `blocked` |
| Wc4 | verifier fix + D=6h → frozen | COMPLETED (~4.6 h), all repairs verified live; every H-allow cell `exclude:runner_infeasible:allowlist_gap` (epoch-1 Debian vs Ubuntu noble) → `blocked` |
| Wc5 | the epoch-2 widened allowlist → frozen | allowlist verified live (real `reward.txt`, zero denied hosts) but 29/30 at the 6 h STOP (`query-optimize` in flight); dev never started → `blocked` |
| Wc6 | D=8h covers the proven pace → frozen | machinery completes; 6th defect — duplicate `Host:` → origin `400` → bootstrap fail → `reward.txt = 0` → `blocked` |
| Wc7 | six-defect runner + epoch-3 → frozen | duplicate-`Host` verified live; 7th defect — the 8 MiB forward body cap `502`s the noble `Packages` index → `reward.txt = 0` → `blocked` |
| Wc8 | seven-defect runner + epoch-3 at D=8h → frozen | `early_stop:operator_ruling R-1` ~23:20Z — first H-allow cells all `verifier_unavailable:bootstrap:signature` (the D8/D9 allowed-CONNECT starve) → `blocked` |
| smoke1/2 (non-evidence) | hardened egress verified or the next defect named | machinery verified live both windows (eof-only closes, zero 5xx, real bytes, `task_toml` deadlines, kill-on-lapse); residual named each time — declared-data (smoke1) then the agent-phase legit exclusion (smoke2) |
| smoke3 (non-evidence) | (envelope's third smoke — unexercised) | never dispatched — smoke2's residual was a declared-policy boundary, not a machinery signature a further smoke could diagnose; its ceiling stays authorized-unrun |
| Wc9 | the first window on verified-live machinery → held members land covering verdicts | held leg COMPLETE — 9 `admit` + pypi-server `repair:machinery` (the `agent_timeout` ×6 signature; diagnosed as the undrained-pipe capture defect and repaired); dev leg `incomplete{dead_man}` at 74/76 → `blocked` |
| GATE-G30 | — | `re-scope{harden-again}` (operator verbatim "I authorized, proceed") → this envelope |

**The A2 budget table** (R-5 amended — completed windows at billed figures where present else recorded
estimates, never ceilings; authorized-but-unrun at ceilings; compute ≤ $30 first-pass; model ≤ $70;
$10 unauthorized reserve; total ≤ $110):

| window | counted as | µ$ | $ |
|---|---|---|---|
| completed Σ (W1, W1-rerun, W2, W3, Wc–Wc8, smoke1, smoke2, Wc9 — Wc9 settlement `pending`, billed read is the operator's) | estimates | 18,274,668 | 18.27 |
| smoke3 (authorized, unrun) | ceiling | 830,480 | 0.83 |
| smoke4 + smoke5 (this envelope, authorized-unrun at ceilings if PASSED) | ceilings | 1,660,960 | 1.66 |
| W4 (planned, unauthorized) | declared ceiling | 7,988,890 | 7.99 |
| **Σ first-pass compute (worst case, this readout)** | | **28,755,000** | **28.76 ≤ 30** |

The honest spend register: `docs/build/reports/live/PHASE_SPEND.jsonl` (every row `basis: estimate`,
authority `gcp_billing`, billed reads pending per HUMAN-H18). Model spend $0 throughout — every window
`model_line: none`; the ≤ $70 model authorization is unexercised; the $10 reserve untouched; total ≤ $110
holds at $28.76 + $0 + $10.

**Dispositions — your verbatim answer (the agent signs nothing).**
`envelope_authorized` — the named smokes may PROV under this envelope; R6.45n dispatches smoke4 first,
evaluating to the recorded verification or a contingency window on the identical recipe; envelope
exhausted or a newly named defect class → `blockedOn` + a §6-shaped readout. `not_run` — the envelope is
not exercised; the task list stays `blocked` verbatim.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
<!-- Appended only when the operator has answered; an agent never writes these lines on its own authority:
Operator decision (verbatim, received <date -u +%FT%TZ> via <channel>): "<exact words>"
Disposition: <envelope_authorized | not_run>; basis: "<words>"
verbatim_sha256 = sha256_text("<exact words>") — stamped fp-smoke{4,5}.json per answer.
GATE DECISIONS row: <date -u +%FT%TZ> | GATE-G28k
Operator confirmation of agent-drafted text (verbatim, <date -u +%FT%TZ>): "<words>" — covers sha256:<first 12>
Signed by: the operator. Recorded by <harness/model>, which does not sign.
-->

> **Signature (Reading 1, 2026-11-16)** — verbatim "I authorize, please continue" (operator, interactive) in response to the GATE-G28k diagnostic-envelope presentation (agent-drafted block sha256 `e211ba90dfaa`; quotes `pq-9b4d23564e37c0b1`/`pq-684bcc89e7f7701d`, 830,480 µ$ each — Σ 1,660,960 ≤ 2,500,000). Disposition: **`envelope_authorized`** — R6.45n dispatches smoke4 first on the diagnostic recipe (pypi-server + make-doom-for-mips, H-allow + H-pub r=1) on the pinned repair tip `6801febf`; smoke5 is the same-recipe contingency only. The envelope's `verbatim_sha256` stamps onto `fp-smoke{4,5}.json`; each smoke's PROV checks authorization `match` before any create under REQ-XH-004.
