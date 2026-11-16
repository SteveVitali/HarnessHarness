# GATE-G30 — post-Wc9 scope decision: the A3/R-8 stop rule's `blockedOn` presentation (pypi-server `repair:machinery` + dev leg `incomplete{dead_man}` + the exhausted REQ-XH-127(c) walk → `task_lists.state: blocked` verbatim; the §6 ladder, options 2–5; no Wc10 is drafted) · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

The post-Wc9 stop rule (`docs/experiments-spec.md` §10.5 amendment row A3, ADR-0445; operator ruling R-8,
LEDGER `## GATE DECISIONS` `intervention-2026-11-15`): "if Wc9 leaves any member with a `machinery{…}`
discordance or a `verifier_unavailable:*` classification, no Wc10 is drafted — `blockedOn` plus a
scope-decision readout carrying the ladder options." Wc9 left pypi-server carrying `repair:machinery`
(the append-only discordance register's `classification: machinery`, R-7) → the rule fired: **no Wc10 is
drafted** and this readout is the scope-decision presentation it owes (insert spec
`docs/build/planning/2026-11-15_hallow-hardening-insert.md` §6 — "options 2–5"). It is **records-only,
$0** — a scope disposition, never a compute authorization; any `run`-shape continuation returns through
its own PROV readout under REQ-XH-004. This file does not discharge ticket 312's FP-E criterion: the
chain sits `blockedOn` upstream of FP-E (whose presentation requires G-L1 or the fallback trigger + W2
settlement); `GATE-G30.md` stays uncreated and this readout records the post-Wc9 scope decision under
the GATE-G30 slot.

## Evidence (per item: source, `date -u` of the read, layer reached)

- The Wc9 outcome — `docs/build/reports/2026-11-16-fp-wc9-tasklists.json` (`hh-task-lists-outcome/1`,
  landed R6.45m `a6821534`, PR #401): **`task_lists.state: blocked` verbatim** — 9/14 members carry a
  covering `admit`; pypi-server `repair:machinery` (all six H cells decided reward 0, `denied_hosts []`);
  the dev leg `incomplete{dead_man}` at 74/76 cells (pytorch-model-recovery's H-allow pair in-flight at
  the in-guest STOP ~13:37Z; the dev packet never minted); pytorch-model-cli
  `exclude:runner_infeasible:allowlist_gap{download.pytorch.org}` — the recorded agent-phase legit
  exclusion (A5/R-7) whose REQ-XH-127(c) walk is recorded **exhausted** (the easy dev cell's committed
  order consumed, the shared medium donor pool fully consumed — no covering successor exists); 3
  standing `blocked{no_adjacent_donor}` — read 2026-11-16 — layer: committed records.
- The held packet + register — `run-root/prove-held/reproof-packet.json` (`hh-reproof-packet/1`) +
  `run-root/prove-held/reproof-register.json` (`hh-reproof-register/1`, committed under
  `docs/build/reports/compute/harvest/fp-wc9/`): **9 `admit`** (prove-plus-comm, make-doom-for-mips,
  torch-tensor-parallelism, vulnerable-secret, git-leak-recovery, polyglot-c-py, db-wal-recovery,
  sqlite-db-truncate, **query-optimize** — its standing Wc4 `repair:machinery` discordance did NOT
  re-occur; discharged) **+ pypi-server `repair:machinery`** — register row: `classification:
  machinery`, `image_digest_equal: true`, `denied_hosts: []`, surfaces **O 3/3 passed** (wall 178,289
  ms) vs **H-pub 0/3** (wall 2,817,717 ms) and **H-allow 0/3** (wall 3,102,843 ms); epoch-5 digest
  `sha256:e61a6ed9…` carried verbatim — read 2026-11-16 — layer: committed records.
- The per-cell machinery evidence — `run-root/wc9-window-record.json` + `run-root/wc9-store-check.json`:
  every pypi-server cell `agent_end_cause: agent_timeout` at the `task_toml` 900,000 ms agent deadline
  (`deadline_source: task_toml`, D15 honored), `verify_attempts: 1`, `reward_ppm 0`; each H-allow cell's
  mediator store decided **177–179/177–179** requests with 24–26 `eof` tunnels, zero 5xx, zero undecided,
  `deny_hosts: []`, `illegal_denies: []`; H-pub cells carry **empty close_causes — zero mediated
  tunnels** (the public surface never traverses the egress mediator); the store-checker's only flags
  anywhere are pytorch-model-cli's two by-design agent-phase `download.pytorch.org` denies (the recorded
  legit exclusion, same handling as smoke2) — read 2026-11-16 — layer: committed records.
- The verifier streams (R-11 first-error excerpts) — `run-root/verifier-first-errors.txt`: every
  pypi-server cell's excerpt is a **clean bootstrap** (apt configured, uv 0.9.5 fetched, pip + pygments
  downloaded through the tunnels, "Installed 7 packages in ~33–47 ms") followed by the honest grade 0 —
  **no crash line, no deny, no CONNECT/5xx signature**; pytorch-model-cli's H-allow excerpts show the
  `download.pytorch.org` agent-phase deny sequence → `weights.json` absent → `Error loading weights` —
  read 2026-11-16 — layer: committed records.
- The R-8 early-inspection checkpoint — `run-root/checkpoint-latest.json`: fired **08:12:31Z** at ≥3
  distinct members' H-allow cells — `all_decided_reward1: true` (16 cells across prove-plus-comm /
  make-doom-for-mips / torch-tensor-parallelism, uniformly decided reward 1, eof-only, zero 5xx,
  `denied_hosts []`) → continue; the machinery-signature stop class was never observed — read
  2026-11-16 — layer: committed records.
- The run record + live-evidence label — `docs/build/runs/R6.45m.md` + `docs/build/reports/live/
  2026-11-16-reproof-wc9.{json,transcript.txt}`: the `reproof` leg closed `fail:prove-driver-rc` (the
  driver-minted UTC poll deadline ~13:25Z expired inside the dead-man before the dev packet minted —
  honest, never rescued); the held packet + register stand as the leg's landed machinery evidence; the
  runner ran byte-identical smoke-loop digests at stamped `27ac8db9…` (hh-w1 `sha256:d91c2c38…`,
  mediator `sha256:8f830a7f…`, helper `sha256:727b7b83…`) — **zero in-window rebuilds, R-10
  unexercised** — read 2026-11-16 — layer: committed records.
- The archive + settlement + cost — `docs/build/reports/archive/2026-11-16-fp-wc9-{upload,verify}.json`
  **`archive_verified`** (terminal dataset revision `ab1e8634e0134c05ca07a7218a6b72f1f1e248ce`, citing
  smoke2 `3bd73242…`; the raw run tree rides as prescanned `wc9/run-root-tree.json`, the byte-exact tar
  `digest_only`) + `docs/build/reports/2026-11-16-fp-wc9-settlement.json` **`pending`** clock-honest
  (4 paced reads, balance `59.99988` unchanged, usage buckets zero) + `docs/build/reports/compute/
  2026-11-16-fp-wc9-cost.json` estimate **2,662,615 µ$ ≤ ceiling 3,193,860** — read 2026-11-16 — layer:
  committed records.
- The rulings + amendments — LEDGER `## GATE DECISIONS`: `intervention-2026-11-15` rows **R-7**
  (verbatim: "an `allowlist_gap` requires a named denied host — H-allow below R/R with empty
  `denied_hosts` is a machinery discordance: repaired, never excluded" → A5) and **R-8** (A3 — gate
  content + prediction track + early-inspection + the post-Wc9 stop rule); the **GATE-G28j** row PASSED —
  `run` (operator verbatim "ok, continue", `verbatim_sha256 1211265b…` stamped `fp-wc9.json`);
  `docs/experiments-spec.md` §10.5 rows A2 (ADR-0444 accounting), A3 (ADR-0445), A5 (ADR-0447
  classifier) + insert spec §6 (the re-scope ladder; "options 2–5") — read 2026-11-16 — layer:
  committed records.

<!-- agent-drafted:begin sha256=b86dd62d86622512 -->
## Decision content for the operator

**The blockedOn state.** `task_lists.state` is **`blocked` verbatim** (`hh-task-lists-outcome/1`,
`2026-11-16-fp-wc9-tasklists.json`). Three declared residual classes hold it, plus three standing
blocked slots:

1. **pypi-server `repair:machinery`** — the append-only discordance register's `classification:
   machinery` stands verbatim (R-7: an H surface below R/R with empty `denied_hosts` is a machinery
   discordance — repaired, never excluded). This is the member the stop rule fired on.
2. **The dev leg `incomplete{dead_man}`** — the in-guest dead-man STOPPED the instance at 74/76 cells
   (pytorch-model-recovery's H-allow pair in-flight); the dev packet never minted, so
   build-pmars / cancel-async-tasks / pytorch-model-recovery hold per-cell evidence (all landed cells
   decided reward 1) but **no covering verdict**.
3. **pytorch-model-cli `allowlist_gap{download.pytorch.org}`** — the recorded agent-phase legit
   exclusion (A5/R-7; X-06/Q-X55's narrow agent set — apt/PyPI/npm/crates only; the oracle's own
   `pip install torch --index download.pytorch.org` is denied by the AGENT layer by design). The
   REQ-XH-127(c) replacement walk is recorded **exhausted**: the easy dev cell's committed order is
   consumed and the shared medium donor pool is fully consumed (build-pmars +
   pytorch-model-recovery serve their own slots; the rest excluded) — **no covering successor exists**,
   so `frozen` is unreachable even best-case.
4. Three standing `blocked{no_adjacent_donor}` slots — `no_member_to_reproof`.

And **the machinery itself is verified live in a real evidence window**: the held leg ran to COMPLETION
inside D=8h (~06:09Z→~12:10Z); the R-8 checkpoint fired 08:12:31Z all-decided-reward-1 → continue;
**9 covering `admit` verdicts landed — the first covering verdicts in the Wc line** — including
query-optimize's discharged Wc4 discordance; every completed cell's mediator store carries zero 5xx,
eof-only closes with true byte counts, zero undecided, `denied_hosts []`; `task_toml` deadlines and the
kill-on-lapse guard held throughout. The engineering goal the Wc line was re-proving — that the
hardened runner runs the re-proof machinery correctly — is met. The frozen-list predicate's `frozen`
(every slot admits a covering-verdict member) is a separate, stricter coverage goal the declared
residuals block.

**The honest pypi-server diagnosis (machinery-vs-task-provability).** The evidence narrows the
discordance further than the register's `machinery` class alone records:

- **O 3/3 passed** — the register's surface walls are 178,289 ms for O vs 2,817,717 / 3,102,843 ms for
  H-pub / H-allow: the oracle's three repeats finished in a fraction of one 900 s agent deadline while
  every H cell burned its whole deadline — the task **is provable end-to-end**: the image is identical
  across surfaces (`image_digest_equal: true`), the verifier runs, and it grades 1 when the work lands
  (it graded 1 on the other nine members' H cells).
- **H-pub 0/3 with zero mediated tunnels** (`close_causes: {}` — the public surface never traverses
  the egress mediator or the allowlist). The mediated-egress machinery the Wc line hardened **cannot
  be implicated** in these failures — it is not on the H-pub path at all, and H-pub fails identically
  to H-allow.
- **H-allow 0/3 with clean stores** — 177–179 requests decided per cell, 24–26 eof tunnels, zero denies,
  zero 5xx, zero undecided; the verifier streams show the full bootstrap succeeding through the tunnels
  (uv + pip + pygments installed) then an honest grade 0.
- **All six H cells ended `agent_timeout`** at the `task_toml` 900 s agent deadline — the constrained
  harness agent burned its entire deadline and produced an artifact the verifier graded 0. Compare
  build-pmars / cancel-async-tasks under the identical deadline: `exited{0}` → reward 1; the oracle's
  surface completed its repeats in ~3 min of wall.

So the discordance is **isolated to the agent→verifier surface, not the egress leg**. Two honest
hypotheses remain open at this readout — the register class (`machinery`, R-7) stands verbatim and is
never re-scored or excluded pending the discrimination:

- **(a) a residual machinery issue in the shared agent/verify path** — e.g., the harness agent's work
  not landing where the verifier inspects (the same cell-fidelity class D15–D19 hardened), or an
  agent-deadline/agent-environment bound the O surface doesn't share — note the O surface ran ~3 min of
  work while the H agent timed out at 15 min, so a too-tight deadline alone is not obviously the cause.
- **(b) a task-provability finding** — pypi-server's task genuinely beyond the no-model constrained
  agent inside the `task_toml` budget: `agent_timeout` (not `exited{0}`) means the agent never produced
  a finished artifact; reward 0 is then the honest grade of incomplete work, and the defect class is
  member-selection/agent-capability, not machinery.

A bounded diagnostic (the `re-scope{harden-again}` option below) is the instrument that discriminates
(a) from (b) — e.g., replaying the oracle's recorded artifact through the H-allow cell's verify path,
or inspecting the timed-out agent's produced tree against the verifier's checks — **before** any
further window is contemplated.

**The §6 re-scope ladder (insert spec §6 — options 2–5; `run` of another same-shape window does not
exist — the stop rule forbids a Wc10).**

- `re-scope{harden-again}` — a targeted pass on pypi-server's reward-0: a bounded smoke/diagnostic under
  a new envelope (the R-3 smoke class is the existing instrument) to pin machinery-vs-task-provability
  before any window — replay the O artifact through the H-allow verify path, diff the agent's produced
  tree, probe the deadline bound. If a newly named defect class falls out, a hardening insert plus a
  new smoke envelope follows the recorded pattern.
- `re-scope{exclude-and-replace}` — recorded as **likely unavailable**: A5/R-7 makes an exclusion
  legitimate only on a named denied host or an O-fail; pypi-server's discordance is `machinery` —
  repaired, never excluded — and the REQ-XH-127(c) walk for the *legitimately* excluded member
  (pytorch-model-cli) is already recorded exhausted: no covering successor exists.
- `re-scope{end-first-pass-early}` — record the honest outcome: the machinery verified live in a real
  evidence window (9 `admit`, zero 5xx, eof-only, `denied_hosts []`, `task_toml` deadlines,
  kill-on-lapse) while the frozen list stays `blocked` on declared residuals — the re-proof's
  engineering goal met **without a fabricated `frozen`**; W4 skipped; the withheld first-pass claims
  named in the disposition record.
- `not_run` — the task list stays `blocked` verbatim; every subject leg keeps
  `withheld{un_reproofed_successor}` / `withheld{no_adjacent_donor}`.

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
| smoke3 (non-evidence) | (envelope's third smoke — unexercised) | never dispatched — smoke2's residual was a declared-policy boundary, not a machinery signature a further smoke could diagnose; its ceiling stays authorized-unrun |
| **Wc9** | the first window on **verified-live** machinery → held members land covering verdicts; pytorch-model-cli carries the legit exclusion; `frozen` only if every slot admits a covering member | held leg COMPLETE — **9 `admit`** (query-optimize discharged) + **pypi-server `repair:machinery`** (all-6 decided reward 0, clean stores, `agent_timeout` ×6); dev leg `incomplete{dead_man}` at 74/76; the REQ-XH-127(c) walk exhausted → **`blocked` verbatim → the stop rule fired → this readout** |

Every prior prediction held on a **new machinery assumption**; each window verified that assumption and
found the next defect. Wc9 was the first window whose machinery basis was verified live — and it held:
the residual is **not** a machinery-signature in the egress leg but an isolated agent→verifier
discordance plus window-bound and policy-boundary residuals the ladder exists to disposition.

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
| Wc9 | estimate (settlement `pending`; billed read is the operator's) | 2,662,615 | 2.66 |
| **completed Σ** | | **18,274,668** | **18.27** |
| smoke3 (authorized, unrun) | ceiling | 830,480 | 0.83 |
| W4 (planned, unauthorized) | declared ceiling | 7,988,890 | 7.99 |
| **Σ first-pass compute (worst case, this readout)** | | **27,094,038** | **27.09 ≤ 30** |

The honest spend register: `docs/build/reports/live/PHASE_SPEND.jsonl` (every row `basis: estimate`,
authority `gcp_billing`, billed reads pending per HUMAN-H18 — Wc9's settlement is `pending`
clock-honest). For history, the superseded Σ of declared window ceilings stands at **41,574,830 µ$ ≈
$41.57** — recorded, superseded by A2, never erased. Model spend $0 throughout — every window
`model_line: none`; the ≤ $70 model authorization is unexercised; the $10 reserve untouched; total ≤
$110 holds at $27.09 + $0 + $10.

**What Wc9 proved.** The machinery is the deliverable and it is verified live: the D8/D9
allowed-CONNECT starve that early-stopped Wc8 is gone (16 checkpoint cells + every completed cell
eof-only, zero 5xx, real bytes); the D15–D19 cell-fidelity repairs hold (`task_toml` deadlines on every
cell, the lapsed-exec kill, the declared-resource audit, the A5 classifier landing `machinery` vs
`allowlist_gap` correctly — including the by-design `download.pytorch.org` denies flagged verbatim and
handled as the legit exclusion); epoch-5 carried the whole pytorch-model-cli verify bootstrap (69 eof
tunnels per H-allow cell); and the packet machinery minted the first covering `admit` verdicts of the
re-proof program — 9 of them, including the member whose prior discordance discharged. What `frozen`
requires beyond that — every slot admitting a covering-verdict member — is blocked by the declared
residuals above; nothing in the run fabricated it.

**Dispositions — your verbatim answer (the agent signs nothing).**
`re-scope{harden-again}` · `re-scope{exclude-and-replace}` · `re-scope{end-first-pass-early}` ·
`not_run` — per the ladder above. There is no `run` of another same-shape re-proof window: the stop
rule forbids a Wc10. A `run` in this state means a **different shape**, never another same-shape
window — a diagnostic smoke under `harden-again`, or the pass continuing toward W4 only if your
disposition lifts the `blockedOn` — and any such continuation returns through its own gate and PROV
readout under REQ-XH-004.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
<!-- Appended only when the operator has answered; an agent never writes these lines on its own authority:
Operator decision (verbatim, received <date -u +%FT%TZ> via <channel>): "<exact words>"
Disposition: <not_run | re-scope{harden-again} | re-scope{exclude-and-replace} | re-scope{end-first-pass-early}>; basis: "<words>"
GATE DECISIONS row: <date -u +%FT%TZ> | GATE-G30 (post-Wc9 scope decision)
Operator confirmation of agent-drafted text (verbatim, <date -u +%FT%TZ>): "<words>" — covers sha256:<first 12>
Signed by: the operator. Recorded by <harness/model>, which does not sign.
-->

> **Signature (Reading 1, 2026-11-16)** — verbatim "I authorized, proceed" (operator, interactive) in response to the GATE-G30 post-Wc9 scope-decision presentation (agent-drafted block sha256 `b86dd62d86622512`). Disposition: **`re-scope{harden-again}`** — the bounded post-repair diagnostic line. The records-only diagnosis already discriminated machinery-vs-task-provability: the `agent_timeout` signature is the shared `exec_guarded` capture path's undrained pipes, repaired at `0c4e0547` + `4679635d` on `codex/harnessharness-exec-drain` (PR #404; the XO-05 `exec_drain` battery passes 4/4 post-fix and the agent's captured streams now persist as `collect/agent-{stdout,stderr}.txt`). The instrument that remains is the diagnosis's prescribed post-repair verification — a bounded non-evidence smoke leg on pypi-server + make-doom-for-mips under a NEW smoke envelope (the R-3 smoke class, A1 label). A run-time gate insert (`GATE-G28k` envelope + `R6.45n` smoke body) presents that envelope; any smoke returns through its own PROV under REQ-XH-004; **no Wc10 is drafted**; the task list stays `blocked` verbatim pending it.
