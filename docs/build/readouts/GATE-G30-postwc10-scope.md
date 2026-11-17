# GATE-G30 — post-Wc10 scope decision: the A3/R-8 stop rule's `blockedOn` presentation on the *repaired* machinery (both legs ran to COMPLETION — 12/14 covering `admit`; `task_lists.state: blocked` verbatim on query-optimize's `o_fail:flaky` exclusion → `blocked{un_reproofed_successor}`, pytorch-model-cli's register `machinery{h_allow}` roll-up over the recorded `allowlist_gap{download.pytorch.org}` signature → `blocked{adjacent_order_exhausted}`, and the 3 standing `blocked{no_adjacent_donor}` slots; a `machinery{…}` register row + a `verifier_unavailable:*` cell fire the rule literally; the §6 ladder, options 2–5; no Wc11 is drafted) · readout
Status: PENDING   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

The post-Wc10 stop rule — `docs/tickets/309s_R6.45o__wc10-run.md`'s declared stop, carried verbatim from
the A3/R-8 line (spec §10.5 amendment row A3, ADR-0445; operator ruling R-8): "a machinery discordance
or `verifier_unavailable:*` after the window → `blockedOn` + a scope-decision readout with options 2–5 —
**no Wc11 is drafted**." Wc10's append-only `hh-reproof-register/1` carries
`classification: machinery{h_allow}` for pytorch-model-cli and its cell reports carry
`verifier_unavailable:bootstrap:signature` on query-optimize/H-allow/r0 → the rule fires on the record's
literal terms → **no Wc11 is drafted** and this readout is the scope-decision presentation owed (insert
spec §6 — "options 2–5"). It is **records-only, $0** — a scope disposition, never a compute
authorization; any `run`-shape continuation returns through its own PROV readout under REQ-XH-004.
This file does not discharge ticket 312's FP-E criterion: the chain sits `blockedOn` upstream of FP-E;
`GATE-G30.md` stays uncreated and this readout rides the GATE-G30 slot a fourth time — the chain sits
`blockedOn` exactly as it did at the post-Wc9, post-smoke4 and post-smoke5 decisions
(`312_GATE-G30__fp-e.md` stays uncreated).

## Evidence (per item: source, `date -u` of the read, layer reached)

- The Wc10 outcome — `docs/build/reports/2026-11-17-fp-wc10-tasklists.json`
  (`hh-task-lists-outcome/1`, window `wc10`, gate GATE-G28l): **`task_lists.state: blocked`
  verbatim — 12/14 members carry a covering `admit`**; per-slot walks recorded (medium order →
  `merge-diff-arc-agi-task` un-reproofed; easy adjacent order exhausted; 3 standing
  `blocked{no_adjacent_donor}`) — read 2026-11-17 — layer: committed record.
- The two packets — `run-root/prove-held/reproof-packet.json`: 9 `admit` + query-optimize
  `exclude:runner_infeasible:oracle_fails_on_O{unclassified}`; `run-root/prove-dev/reproof-packet.json`:
  3 `admit` + pytorch-model-cli `repair:machinery{h_allow}` — minted on-VM ~02:58Z, harvested to
  `docs/build/reports/compute/harvest/fp-wc10/run-root/` — read 2026-11-17 — layer: harvested run-root
  (manifest-verified 2044/2044).
- The register rows — `prove-held/reproof-register.json` query-optimize `o_fail:flaky` (surfaces O 2/3,
  H-pub 3/3, H-allow 2/3; denied_hosts []) and `prove-dev/reproof-register.json` pytorch-model-cli
  `machinery{h_allow}` (O 2/2, H-pub 2/2, H-allow 0/2; denied_hosts []) — read 2026-11-17 — layer:
  harvested run-root.
- The cell signatures — `trials/query-optimize/H-allow/r0`:
  `verifier_unavailable:bootstrap:signature` (verify_attempt 1, reward null; r1/r2 `decided` reward
  1000000); `trials/query-optimize/O/surface-outcome.json` rewards `[true,false,true]`,
  `harbor_exception: null` (the failed repeat produced a scored artifact — oracle nondeterminism,
  NOT the systematic W1 `o_fail` machinery signature); `trials/pytorch-model-cli/H-allow/r{0,1}`:
  `decided` reward_ppm 0, `exited{0}`, `denied_hosts []` — **byte-identical in signature to Wc9's
  recorded `allowlist_gap{download.pytorch.org}` cells** (`~/hh-retained/wc9/harvest/trials/…` read
  2026-11-17) — layer: harvested run-root.
- The mechanism, now visible — `collect/agent-stderr.txt` (persisted by the exec-drain repair Wc9
  lacked): `ProxyError … Tunnel connection failed: 403 Forbidden` on
  `download.pytorch.org/whl/cpu/torch/` → `No matching distribution found for torch==2.7.0` →
  `ModuleNotFoundError: No module named 'torch'` → `Error loading weights`; same-index `numpy` and
  `deb.debian.org` fetched clean in the same transcript — the recorded **agent-phase legit exclusion**
  mechanism (X-06's narrow agent allowlist — the deny lives at the egress proxy, never traverses the
  mediator, hence `denied_hosts []` epoch-consistent); A5's `allowlist_gap`-needs-nonempty-denied_hosts
  literal is what rolled the register row to `machinery{h_allow}` — read 2026-11-17 — layer: harvested
  cell transcripts.
- The repair verification on the re-proof itself — pypi-server (the Wc9 `repair:machinery`
  discordance): 6/6 cells `decided` reward 1000000 `exited{0}` at `model_port` 49152 — the task's
  `:8080` bound cleanly, no collision; every leg streamed past the ~64 KiB pipe bound with persisted
  transcripts — read 2026-11-17 — layer: harvested run-root.
- The checkpoint — `run-root/checkpoint-latest.json`: `distinct_h_allow_members: 3`,
  `all_decided_reward1: true` → continue (fired ~00:26Z at the first-H-allow-cells arm; the
  R-8-shaped instrument exercised as declared) — read 2026-11-17 — layer: harvested run-root.
- The window record — `run-root/wc10-window-record.json` (`hh-wc-repair/1`, ticket R6.45o): 76
  cells — 75 `decided` + 1 `verifier_unavailable:bootstrap:signature`; both packets folded; freeze
  `sha256:e61a6ed9…` carried verbatim; runner digests `hh-w1 sha256:621b9444…` /
  `af3a0d86…` / `727b7b83…` **byte-identical to smoke5's verified build** at declared pinned
  commit `b3f72ab7` — read 2026-11-17 — layer: harvested run-root.
- The store-check — `run-root/wc10-store-check.json`: 38 mediated cells folded, `result: fail` with
  exactly the three recorded anomalies — the by-design agent-phase `download.pytorch.org` denies on
  pytorch-model-cli's H-allow cells (flagged verbatim, same handling as smoke2/Wc9) + the
  `verifier_unavailable:bootstrap:signature` cell; zero 5xx elsewhere — read 2026-11-17 — layer:
  harvested run-root.
- The window lifecycle — receipts `docs/build/reports/compute/2026-11-16-fp-wc10-{provision,verify,
  prepull}.json` + `2026-11-17-fp-wc10-{harvest,teardown,cost}.json`; provisioned ~23:38Z 11-16,
  held leg ~23:51Z→~02:25Z, dev leg ~02:25Z→~02:58Z (~3.1h wall vs D=8h, dead-man 07:53Z never
  approached); instance `hh-fp-wc10` torn down ~03:17Z, `compute-open` empty; the podman-socket
  anomaly recurred and got the recorded repair verbatim (socket `root:hh`, `usermod -aG hh`);
  `hh-fp-a1b2c3` is ACTIVE under `14stevevitali@gmail.com` (recorded account-discovery anomaly
  variant — the window ran under `CLOUDSDK_CORE_ACCOUNT` pinned) — read 2026-11-17 — layer: committed
  records.
- The live-evidence record — `docs/build/reports/live/2026-11-17-reproof-wc10.json`
  (`hh-live-evidence/1`, `result: pass`, cost_usd 0.0) + transcript — read 2026-11-17 — layer:
  committed records.
- The spend — `PHASE_SPEND.jsonl` `sp-69eeb3b6da203388`: Wc10 estimate **1,271,324 µ$ ≤ 3,193,860
  ceiling** (`pq-a2d99ef16c6c3814`); model spend $0 (`model_line: none`, `deliver-secret` ABSENT);
  cumulative $21.702850 — read 2026-11-17 — layer: committed records.
- The archive — `docs/build/reports/archive/2026-11-17-fp-wc10-{upload,verify}.json` +
  `hh-receipt-index-wc10.json`: 37 window records → `SteveVitali/harnessharness-archive` (private),
  **archive_verified** (REQ-XH-104 read-back sha256-clean), cites Wc9's `archive_verified` terminal
  revision `ab1e8634e0134c05ca07a7218a6b72f1f1e248ce`; Wc10's terminal dataset revision
  `6d315bfbb1200e073ae52518ef58cc0229d046f8` — read 2026-11-17 — layer: committed records.
- The settlement — `docs/build/reports/2026-11-17-fp-wc10-settlement.json` (`hh-settlement/1`):
  4 paced reads (balance-pre → usage ×2 → balance-post, ≥65 s apart), balance 59.99988 → 59.99988
  unchanged, usage buckets all-zero, `status: pending` clock-honest (the ≤24 h provider settlement
  clock has not run — same convention as Wc9) — read 2026-11-17 — layer: committed records.

<!-- agent-drafted:begin sha256=362261ff2912 -->
## Decision content for the operator

**What Wc10 answered.** The GATE-G28l question — does the verified machinery produce covering
`reproofed` verdicts at re-proof scale — is answered **yes for 12 of 14 members, with every residual
typed**. Both declared machinery repairs were verified live **on the re-proof itself**, not on a smoke:
pypi-server — the member whose `repair:machinery` discordance held the list — ran 6/6 cells decided
reward 1 at `model_port` 49152; the exec-drain repair surfaced the mechanism-bearing transcripts that
let pytorch-model-cli's deny be read at its layer. Both legs ran to COMPLETION in ~3.1 h of the 8 h
deadline — no dead-man, no incomplete leg, no early_stop; the R-8 checkpoint fired and continued as
declared.

**Why `blocked` stands.** Three disjoint declared-data causes, each recorded verbatim:

1. **query-optimize — new exclusion.** `o_fail:flaky` (O 2/3, `harbor_exception: null` — the failed
   repeat produced a scored artifact: oracle nondeterminism on a task whose oracle is not
   deterministic; categorically NOT the systematic W1 `o_fail` signature, which failed every repeat
   through an infrastructure path). Its H-allow r0 adds a single
   `verifier_unavailable:bootstrap:signature` cell (r1/r2 verified clean — a transient bootstrap
   signature-verification hiccup of the recorded Wc7-era class). The medium committed order's next
   member `merge-diff-arc-agi-task` was never re-proofed — REQ-XH-116's further-successor leg needs
   its own window → `blocked{un_reproofed_successor}`.
2. **pytorch-model-cli — the register roll-up.** `machinery{h_allow}` over a signature byte-identical
   to the recorded `allowlist_gap{download.pytorch.org}` legit exclusion — and for the first time the
   mechanism is *visible in evidence*: the persisted stderr shows the egress-proxy 403 on the torch
   wheel path while the same index served numpy. A5's literal (allowlist_gap requires non-empty
   `denied_hosts`) produced the `machinery{…}` label; whether the classifier should name this family
   `allowlist_gap` on transcript evidence is a **declared-data/classifier question**, presented here
   honestly rather than silently re-labeled — the register is append-only. The REQ-XH-127(c) walk is
   still exhausted → `blocked{adjacent_order_exhausted}`.
3. **3 standing `blocked{no_adjacent_donor}` dev slots** — unchanged, no member exists to re-proof.

**The stop rule fired on the record's literal terms** — the register carries `machinery{h_allow}`
and a cell carries `verifier_unavailable:*` → this `blockedOn` + readout; **no Wc11 is drafted**.
Honest context for the disposition: unlike Wc9's systematic discordance (6/6 decided-0, unexplained
at the time), both of Wc10's trigger rows carry *explained* mechanisms in the record — one is a
classified declared-data exclusion the contract itself names (`pytorch-model-cli` "carries its
recorded `allowlist_gap{download.pytorch.org}` agent-phase legit exclusion" — AC-5), one is a
single-cell transient of the recorded Wc7-era class. The rule's conservative literal is recorded
verbatim anyway; the disposition is the operator's.

**The §6 re-scope ladder (insert spec §6 — options 2–5; `run` of another same-shape window does not
exist — the stop rule forbids a Wc11).**

- `re-scope{harden-again}` — a bounded diagnostic could discriminate two open questions *without*
  another re-proof: (a) whether the A5 denied_hosts literal should re-classify the
  egress-proxy-deny family as `allowlist_gap` on transcript evidence (a classifier amendment —
  declared data, its own ticket/ADR path); (b) whether the single `bootstrap:signature` cell was a
  transient fetch hiccup (inspectable from the harvested verifier streams — the Wc7-era class, one
  cell, self-resolving on sibling repeats). No un-diagnosed defect class stands today: every failure
  carries a typed, mechanism-explained signature. A machinery repair has nothing standing to repair.
- `re-scope{exclude-and-replace}` — recorded as **unavailable**: pytorch-model-cli already IS the
  recorded exclusion and its walk is exhausted; query-optimize's exclusion already walked to an
  un-reproofed successor. The slots *are* the exclusions — there is no lever left in the committed
  orders under the current evidence.
- `re-scope{end-first-pass-early}` — record the honest outcome: the machinery verified live on the
  re-proof itself (12 `admit` verdicts, zero 5xx, eof-only closes, `denied_hosts []`, the checkpoint
  exercised, transcripts persisted) while the frozen list stays `blocked` on declared residuals —
  the re-proof's engineering goal met **without a fabricated `frozen`**; W4 skipped; the withheld
  first-pass claims named in the disposition record; G-L1 keeps `withheld{un_reproofed_successor:
  task_lists_state=blocked}` verbatim.
- `not_run` — `task_lists.state` stays `blocked` verbatim; every subject leg keeps its withheld
  state.

Your own ruling is also recordable verbatim — e.g., a directed successor re-proof
(`merge-diff-arc-agi-task` for the medium slot, or a re-proof of the order's remaining members)
would keep `frozen` reachable before the Q-X62 trigger, but it is a **new window**: the stop rule
forbids this readout drafting it; it would need its own insert + PROV gate if you rule it.

**Prediction-track table (A3 / R-8 — what each readout predicted vs what happened).**

| window | the readout predicted | what actually happened |
|---|---|---|
| Wc–Wc8, smoke1–5 | (carried verbatim from the post-Wc9 + post-smoke5 readouts — every prediction held on a new machinery assumption; each window verified that assumption and either named the next defect or verified the repair live) | see `GATE-G30-postwc9-scope.md` + `GATE-G30-postsmoke{4,5}-scope.md` |
| **Wc9** | first window on verified-live machinery → held members land covering verdicts | 9 `admit` + pypi-server `repair:machinery` + dev `incomplete{dead_man}` → `blocked` → the stop rule fired → repair line discharged by smoke4/smoke5 |
| **Wc10** | the discharged machinery line + band model ports → covering verdicts at scale; `frozen` iff every slot admits | **12/14 `admit`**, both legs COMPLETE ~3.1 h; pypi-server discharged live on the re-proof; residuals are all typed declared-data causes (exclusion walks + `no_adjacent_donor`) plus the roll-up/classifier question → **`blocked` verbatim → the stop rule fired on the literal record → this readout** |

**The A2 budget table** (completed windows at recorded estimates; authorized-but-unrun at ceilings;
compute ≤ $30 first-pass; model ≤ $70; $10 unauthorized reserve; total ≤ $110):

| window | counted as | µ$ | $ |
|---|---|---|---|
| W1 + W1-rerun | estimate | 2,129,191 | 2.13 |
| W2 | estimate | 201,242 | 0.20 |
| W3 | estimate | 167,296 | 0.17 |
| Wc–Wc8 (8 windows) | estimates | 12,483,928 | 12.48 |
| smoke1 + smoke2 | estimates | 630,396 | 0.63 |
| Wc9 | estimate (settlement `pending`) | 2,662,615 | 2.66 |
| smoke4 + smoke5 | estimates | 624,118 | 0.62 |
| **Wc10** | **estimate (settlement `pending`)** | **1,271,324** | **1.27** |
| (other spend rows — non-window legs in the PHASE_SPEND register) | recorded | 1,532,740 | 1.53 |
| **completed Σ (PHASE_SPEND cumulative verbatim, 78 rows)** | | **21,702,850** | **$21.70** |
| smoke3 (authorized, envelope closed — cannot spend) | ceiling | 830,480 | 0.83 |
| W4 (planned, unauthorized) | declared ceiling | 7,988,890 | 7.99 |
| **Σ worst case (completed + W4)** | | **29,691,740** | **~29.69 ≤ 30** |

Model spend $0 throughout (every window `model_line: none`; `provided.OLLAMA_API_KEY` used only for
the clock-honest settlement reads); the ≤ $70 model authorization unexercised; the $10 reserve
untouched; total ≤ $110 holds at ~29.69 + 0 + 10.

**What Wc10 proved.** The machinery is the deliverable and it is verified live *at re-proof scale*:
both legs complete well inside D=8h (the Wc9 dead-man residual did not recur); the pipe-drain and
model-port repairs held on the evidence leg itself; the checkpoint instrument fired and continued;
the packet machinery minted 12 covering `admit` verdicts — three more than any prior window — and for
the first time a discordance's *mechanism* is readable in the persisted transcripts rather than
inferred. What `frozen` requires beyond that — every slot admitting a covering-verdict member — stays
blocked on declared-data residuals (two exhausted/un-reproofed replacement walks + three standing
donor-less slots), not on any standing machinery defect. Nothing in the run fabricated `frozen`.

**Open follow-ups (honest inventory).** DF-R6.45l-1 (transitive verify-host derivation) stays OPEN —
Wc10's stores again show `denied_hosts []` on every completed cell; the only denies are the by-design
agent-phase `download.pytorch.org` rows recorded verbatim. DF-R6.23-1 (gate-citation predicate) —
Wc10's `archive_verified` index `hh-receipt-index-wc10.json` + terminal revision `6d315bfbb…` now
exist (the row's R6.45o share); the row stays OPEN for its R6.47 owner per the recorded handling.
PR #407's merge remains operator-owned. The A5 classifier roll-up question (egress-proxy-deny family
→ `allowlist_gap` on transcript evidence) is a candidate owed-work row — a declared-data/classifier
amendment, not a machinery defect.

**G-L1 consequence.** `task_lists.state: blocked` → W3's withheld L1 corpus legs keep
`withheld{un_reproofed_successor:task_lists_state=blocked}` verbatim → `full` stays unpresentable on
evidence; Q-X62's fallback trigger instant is 2026-11-24T21:42:46Z (~7.5 days out). `frozen` remains
reachable only through a successor re-proof of `merge-diff-arc-agi-task` (the medium slot's
next-order member) under a new insert + PROV — which this readout does not draft.

**Dispositions — your verbatim answer (the agent signs nothing).**
`re-scope{harden-again}` · `re-scope{exclude-and-replace}` · `re-scope{end-first-pass-early}` ·
`not_run` — per the ladder above; or your own ruling (recorded verbatim, e.g., a directed successor
re-proof insert). There is no `run` of another same-shape re-proof window: the stop rule forbids a
Wc11. Any `run`-shape continuation means a different shape — a bounded diagnostic under
`harden-again`, or a new-window insert on your ruling — each returning through its own gate and PROV
readout under REQ-XH-004.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
<!-- Appended only when the operator has answered; an agent never writes these lines on its own authority:
Operator decision (verbatim, received <date -u +%FT%TZ> via <channel>): "<exact words>"
Disposition: <not_run | re-scope{harden-again} | re-scope{exclude-and-replace} | re-scope{end-first-pass-early} | <operator's own ruling>>; basis: "<words>"
GATE DECISIONS row: <date -u +%FT%TZ> | GATE-G30 (post-Wc10 scope decision)
Operator confirmation of agent-drafted text (verbatim, <date -u +%FT%TZ>): "<words>" — covers sha256:<first 12>
Signed by: the operator. Recorded by <harness/model>, which does not sign.
-->
