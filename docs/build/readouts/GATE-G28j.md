# GATE-G28j — PROV-Wc9: authorize the Wc9 evidence re-proof's compute (the hardened-runner re-proof under epoch-5 at one declared pinned commit, D=8h, with the R-8 early-inspection checkpoint) · readout
Status: PENDING   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket
authorizes spend. Wc9 is a **recorded spec deviation — a ninth repair window**: the spec's gate table
declares PROV-Wc "at most once" and the Wc row "at most one contingency window" (discharged at R6.45;
Wc2–Wc8 discharged at R6.45b–R6.45h — recorded amendments, never declared windows). Under the R-5/A2
accounting the first-pass compute ceiling is $30 — completed windows count at billed figures where present
else recorded estimates, authorized-but-unrun at ceilings; this readout carries the amended budget table,
never the superseded Σ-of-ceilings rule (the ≈ $38.38 Σ-exceed history is superseded, never erased).

## Evidence (per item: source, `date -u` of the read, layer reached)

- The Wc8 outcome — `docs/build/reports/2026-11-15-fp-wc8-tasklists.json` (`hh-task-lists-outcome/1`,
  committed via R6.45h `b37f2619`, merged main PR #391): `early_stop:operator_ruling R-1` (~20:54Z
  held-leg launch → ~23:20Z kill, inside D=8h) — every completed H-allow cell
  `verifier_unavailable:bootstrap:signature`, `denied_hosts []`; first-error tally **4/4 identical**
  `curl: (28) Proxy CONNECT aborted due to timeout`
  (`harvest/fp-wc8/run-root/verifier-first-errors.txt`, R-11) — the allowed-CONNECT starve, the D8/D9
  machinery discordance observed live; `task_lists.state: blocked` verbatim — read 2026-11-16 — layer:
  committed records.
- The smoke records — `docs/build/reports/smoke/2026-11-16-smoke{1,2}.json` (`hh-smoke/1`, A1-labelled
  non-evidence): smoke1 — prove-plus-comm / torch-tensor-parallelism / query-optimize H-allow **decided
  reward 1** + query-optimize H-pub decided 1, real CONNECT bytes, eof-only closes, zero 5xx,
  `task_toml` deadlines honored (D15/D17 live); residual a verify-phase declared-data gap
  (`download-r2.pytorch.org`) → `not_passed{declared_data_residual}`. smoke2 — **all 5 cells decided**
  under epoch-5 (`sha256:e61a6ed9…`): the whole pytorch-model-cli verify bootstrap mediated (417→417
  decided, 67 eof tunnels, `denied_hosts []`, zero 5xx); pytorch-model-cli **decided reward 0** — the
  oracle agent's `pip install torch --index download.pytorch.org` denied by the AGENT layer by design
  (X-06) → `not_passed{task_reward_zero}`; residual is the A5 legit-exclusion shape. The envelope's
  third smoke (`pq-b2ed8578ab9ede1b`) is unexercised — smoke2's residual is a declared-policy boundary
  (agent-phase by design), not a machinery signature a further smoke could diagnose — read 2026-11-16 —
  layer: committed records.
- The conformance leg — `docs/build/reports/live/2026-11-16-egress-real-clients-green.json`
  (`hh-live-evidence/1`): `egress-real-clients` GREEN on fixed tip `54ae9c9e`, GitHub Actions x86_64 run
  43791844390 (`https://github.com/SteveVitali/HarnessHarness/actions/runs/43791844390`) — 996
  requested→decided, 118 tunnels eof, expected denies only; red baseline `6ea180f3` recorded pre-fix —
  read 2026-11-16 — layer: committed live evidence.
- The machinery — declared pinned commit `27ac8db985207e1d8ad112ae0e9c55d7d5bff52b` (R-10: one declared
  pinned commit) carrying every repair: R6.45j (D15 `task.toml` deadlines, D17 lapsed-exec own-tree
  kill + confirmed death, D18 declared-resource audit, D19 the A5 `machinery{h_allow}` classifier) +
  R6.45k (D8–D14 mediated-egress hardening). The smoke loop built the runner in-window at this stamped
  commit — hh-w1 `sha256:d91c2c38…`, mediator `sha256:8f830a7f…`, helper `sha256:727b7b83…`, digests
  byte-identical across smoke1 and smoke2; **zero in-window rebuilds — R-10 unexercised** — layer:
  committed code + recorded digests.
- The freeze — epoch-5 `docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-5.json`
  `sha256:e61a6ed9771d506e0606dd7a953ebe49a56dc96d8427d9eb410ae492e7f3b328` (verify +=
  `download-r2.pytorch.org` / `ossci-datasets.s3.amazonaws.com` / `yann.lecun.com` — runtime-observed at
  smoke1, honestly recorded as exceeding R-6's scan-only authority under the operator-delegated
  declared-data scope call; agent/permanent_denies/withheld byte-identical to epoch-4; checker 12/12
  legs OK; DF-R6.45l-1 OPEN for the runtime-derivation arm) + amendment record
  `docs/build/reports/2026-11-16-fp-w1-q-x55-amendment-epoch5.md` — read 2026-11-16 — layer: committed
  records.
- The rulings — LEDGER `## GATE DECISIONS` `intervention-2026-11-15` rows R-1 (exercised), R-5-amended
  (A2), R-7 (A5), R-8 (A3), R-10 (one declared pinned commit) — read 2026-11-16 — layer: committed
  records.
- The amendments — `docs/experiments-spec.md` §10.5 rows A2 (ADR-0444, the accounting), A3 (ADR-0445,
  gate content + prediction track + early-inspection + post-Wc9 stop), A5 (ADR-0447, the `allowlist_gap`
  classifier) — read 2026-11-16 — layer: committed records.

<!-- agent-drafted:begin sha256=6e59ff12d4222adf -->
## Decision content for the operator

**The scope.** Wc9 is an **evidence re-proof window** — the ninth repair window — on the hardened runner
at the declared pinned commit `27ac8db985207e1d8ad112ae0e9c55d7d5bff52b` under **epoch-5**
(`sha256:e61a6ed9…` carried verbatim), at **D = 8 h** dead-man (in-guest 495 min — the Wc6/Wc7 bound),
C = 4, h_r 6 h, ≤ 1 h harvest-only restart, 1 GiB egress, ~38 trials (the 14 `needs: wc_reproof`
members × committed R — held 10 @ R=3 + dev 4 @ R=2). The evidence rules are unchanged from Wc8: no
in-window fixes, `retry_trials: 0`, typed outcomes, never rescued. $0 model (`model_line: none`,
`deliver-secret` ABSENT — a model call refuses, REQ-XB-219); compute is authorized separately.

**Why the evidence basis differs from every prior window.** Each prior re-proof ran on a *new machinery
assumption* — this one runs on machinery the smoke loop **verified live**: real CONNECT bytes both ways,
every tunnel `close_cause: eof`, zero mediator 5xx, `deadline_source:task_toml` honored and the
lapsed-exec kill working, the full verify bootstrap end-to-end on three members plus the
pytorch-model-cli bootstrap mediated whole. The D8/D9 CONNECT-hang class is closed (red `6ea180f3` →
green `54ae9c9e` on the registered `egress-real-clients` leg, and live in both smokes). The one
remaining residual is a **legit exclusion**, not machinery (below).

**The Wc9 declaration + quote** (`hh-compute-decl/1` `scripts/gce-backend/declarations/fp-wc9.json` —
`mode: full`, e2-standard-8, debian-12, the declared podman/crun/netavark pins, 300 GiB pd-balanced,
IAP tcp:22 ingress only, `service_account: none`; `runner.commit` the declared pinned commit;
`model_line: none`, `deliver-secret` ABSENT; `verbatim_sha256` all-zeros pending the gate stamp;
`validate_decl.py` valid). The quote is minted by the one quote path (`receipt_lib.py
quote`/`quoteid`), `hh-compute-quote/1` `docs/build/reports/2026-11-16-fp-wc9-quote.json`:

| | µ$ |
|---|---|
| planning | 732,994 |
| bound | 2,096,465 |
| **ceiling** | **3,193,860** |

Quote id **`pq-dcfd02eec84112b3`**; §3.10 hard-cap `r·(D + H_rs) + S·r_disk·h_r + 0.12·E_gib` =
`314,140·(8+1) + 300·137·6 + 120,000·1` = `2,827,260 + 246,600 + 120,000` = **3,193,860 µ$ (~$3.19)**
(r = 268,040 + 300·137 + 5,000 = 314,140 µ$/h, re-minted from the pinned live rate table `pricing.json`
`sha256:045e0b86…`).

**The D sizing.** `D = clamp(⌈1.5 × projected wall⌉, 4 h, 8 h)`: the smoke-measured walls
(prove-plus-comm 28.7 s; torch-tensor-parallelism ~4.6 min incl. a ~3.0 GB wheel; query-optimize
~14.4 min at its task_toml 1,800 s verify; all 5 smoke cells landed inside ~28 min at C=4) verify the
declared 2,100 s per-trial bound holds with headroom. The bound-priced projection — 38 trials ×
2,100 s ÷ C=4 ≈ 5.54 h — ×1.5 ≈ 8.3 h clamps at the 8 h maximum; the Wc6/Wc7 completed legs landed
~4.8 h / ~5.4 h inside it. **D = 8 h.**

**The early-inspection checkpoint** (R-8, pre-authorized as this readout records it — the R-1 shape):
at the first completed H-allow cells of **≥ 3 distinct members OR 2.5 h into the prove leg, whichever
comes first**, harvest those cells' `cell-report.json` / `collect/` / verifier streams / mediator
stores. If **every completed H-allow cell is `decided` reward 1** → continue to completion; else stop →
the `early_stop` typed outcome (`incomplete{early_stop:machinery_signature}` or the observed class) →
teardown → harvest → upload → settle, inside h_r.

**The pytorch-model-cli exclusion (recorded verbatim).** `allowlist_gap{download.pytorch.org}` —
**agent-phase**: the oracle's own `pip install torch --index download.pytorch.org` is denied by the
AGENT layer by design (X-06's narrow agent set — apt/PyPI/npm/crates only in the first pass, Q-X55;
`download.pytorch.org` is not PyPI's index host). No `weights.json` → the verifier's honest grade 0 →
the member's `exclude:runner_infeasible:allowlist_gap{download.pytorch.org}`. A named denied host makes
this a **legit exclusion under A5/R-7** — a handled declared-data outcome, never `machinery{h_allow}`
and never a machinery discordance. No verify-phase epoch can fix it — the deny is at the agent layer.
**Expected frozen-list coverage:** 13 members target covering `reproofed` verdicts;
`pytorch-model-cli` carries the legit exclusion — the member is a dev cell occupied via REQ-XH-127(c)'s
adjacent-donor disclosure. The governing predicate (`freeze_post_replacement`, REQ-XH-116/127(c)):
`task_lists.state` is `frozen` **only when every slot admits** a member carrying a covering verdict —
an exclusion walks the slot's committed within-cell order (then the adjacent-donor rule) and a
successor needs its own covering verdict; otherwise the state stays `blocked` verbatim with the full
walk recorded — the honest state, never a silently substituted list. The exclusion itself is the
handled declared-data outcome — not a machinery block and not a post-Wc9 stop-rule trigger.

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
| smoke1/2 (non-evidence) | hardened egress verified or the next defect named | machinery verified live both windows (eof-only closes, zero 5xx, real bytes, `task_toml` deadlines, kill-on-lapse); residual named each time — declared-data (smoke1) then the agent-phase legit exclusion (smoke2) |

Every prior prediction held on a **new machinery assumption**; each window verified that assumption and
found the next defect — until Wc8, whose assumption failed early enough that R-1's early-stop paid for
itself in ~20 minutes. Wc9 is the first window whose machinery basis is **verified live** (the smoke
loop + the registered conformance leg), not assumed.

**Re-scope ladder (A3 / R-8 — the §6 options besides `run`/`not_run`).**
`re-scope{harden-again}` — another R6.45k-style insert plus a new smoke envelope, for a newly named
defect class · `re-scope{exclude-and-replace}` — only where the A5 rule makes an exclusion legitimate
(a named denied host / O-fail), replacement from the seeded order per FP-7/REQ-XH-116 ·
`re-scope{end-first-pass-early}` — record the re-proof outcome as the first-pass result and skip W4,
naming the claims withheld.

**The post-Wc9 stop rule (A3 / R-8, verbatim).** "if Wc9 leaves any member with a `machinery{…}`
discordance or a `verifier_unavailable:*` classification → `blockedOn` + a scope-decision readout with
options 2–5 — **no Wc10 is drafted**." The `pytorch-model-cli` legit exclusion does not trigger it (a
handled declared-data outcome, not a machinery discordance, not a `verifier_unavailable:*`
classification).

**The A2 budget table** (R-5 amended — completed windows at billed figures where present else recorded
estimates, never ceilings; authorized-but-unrun at ceilings; compute ≤ $30 first-pass; model ≤ $70;
$10 unauthorized reserve; total ≤ $110):

| window | counted as | µ$ | $ |
|---|---|---|---|
| W1 | estimate | 784,750 | 0.78 |
| W1-rerun | estimate | 1,344,441 | 1.34 |
| W2 | estimate | 201,242 | 0.20 |
| W3 | estimate | 167,296 | 0.17 |
| Wc | estimate | 1,090,084 | 1.09 |
| Wc2 | estimate | 1,080,485 | 1.08 |
| Wc3 | estimate | 1,801,523 | 1.80 |
| Wc4 | estimate | 1,738,259 | 1.74 |
| Wc5 | estimate | 2,005,926 | 2.01 |
| Wc6 | estimate | 1,757,980 | 1.76 |
| Wc7 | estimate | 1,879,796 | 1.88 |
| Wc8 | estimate | 1,129,875 | 1.13 |
| smoke1 | estimate | 357,320 | 0.36 |
| smoke2 | estimate | 273,076 | 0.27 |
| **completed Σ** | | **15,612,053** | **15.61** |
| smoke3 (authorized, unrun) | ceiling | 830,480 | 0.83 |
| **Wc9 (this gate)** | ceiling | 3,193,860 | 3.19 |
| W4 (planned, unauthorized) | declared ceiling | 7,988,890 | 7.99 |
| **Σ first-pass compute (worst case at G28j)** | | **27,625,283** | **27.63 ≤ 30** |

The honest spend register: `docs/build/reports/live/PHASE_SPEND.jsonl` (every row `basis: estimate`,
authority `gcp_billing`, billed reads pending per HUMAN-H18). For history, the superseded Σ of declared
window ceilings would stand at 38,380,970 + 3,193,860 = **41,574,830 µ$ ≈ $41.57** — recorded,
superseded by A2, never erased; the amended accounting governs. Model spend $0 throughout — every
window `model_line: none`; the ≤ $70 model authorization is unexercised; the $10 reserve untouched;
total ≤ $110 holds at $27.63 + $0 + $10.

**Dispositions — your verbatim answer (the agent signs nothing).**
`run` — the evidence window as specified; R6.45m (309o) dispatches for the Wc9 body (PROV → LOAD → the
pinned hardened runner + epoch-5 freeze → re-proof at D=8h under the early-inspection checkpoint →
frozen-list outcome → teardown → harvest → upload → settle). `not_run` — the task list stays `blocked`,
all subject legs withhold. `re-scope{harden-again}` / `re-scope{exclude-and-replace}` /
`re-scope{end-first-pass-early}` — per the ladder above.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
<!-- Appended only when the operator has answered; an agent never writes these lines on its own authority:
Operator decision (verbatim, received <date -u +%FT%TZ> via <channel>): "<exact words>"
Disposition: <run | not_run | re-scope{harden-again} | re-scope{exclude-and-replace} | re-scope{end-first-pass-early}>; basis: "<words>"
Recorded verbatim: Wc9 $0 model (model_line: none, deliver-secret ABSENT); compute pq-dcfd02eec84112b3,
ceiling 3,193,860 µ$; verbatim_sha256 = sha256_text("<exact words>") — stamped fp-wc9.json.
GATE DECISIONS row: <date -u +%FT%TZ> | GATE-G28j
Operator confirmation of agent-drafted text (verbatim, <date -u +%FT%TZ>): "<words>" — covers sha256:6e59ff12d4222adf
Signed by: the operator. Recorded by <harness/model>, which does not sign.
-->
