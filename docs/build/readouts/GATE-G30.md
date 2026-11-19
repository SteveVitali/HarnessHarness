# GATE-G30 readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion (verbatim)

Spec §10.3 gate table — **FP-E** main window (presented after "G-L1 (or the fallback trigger) + W2 settlement"):

> | **FP-E** main window | first | G-L1 (or the fallback trigger) + W2 settlement | W2-measured $/trial and wall time per arm (cost fields only); the cost projection and arm inclusion by the priority rule, V5 only if room remains; the wall-time projection, D_max and any wall-time drop (REQ-XH-126), stating whether Wc's reservation is released; W2's queue-wait measurement and C (REQ-XD-062); per-arm cap values (µ$) under expected-cost admission (REQ-XH-117); m (= .20, restated); purchased-credit need and amount (Q-X54, Q-X63), A_eff and the recomputed root (REQ-XH-120); the carried root pool (REQ-XH-120); W4 quote, D and restart; exclusivity acknowledgement; any PP0 hold's disposition (REQ-XH-127); the hosted-only form if the fallback triggered |

Amendment note (§10.5 A2 + A3 / ADR-0444, ADR-0445; operator rulings R-5 + R-8, verbatim in the LEDGER
`intervention-2026-11-15` rows): the budget table follows A2 — compute ≤ $30, completed windows at
billed/cost_lib never ceilings, authorized-but-unrun at ceilings, first-pass total ≤ $110; the readout
carries the re-scope ladder and the prediction track.

## Evidence (per item: source, `date -u` of the read, layer reached)

- FP-E quote — `hh-fp-e/1` `docs/build/reports/2026-11-19-fp-e.json` sha256 `d6fa1dab…` — read 2026-11-19T00:40Z — layer: `engineered` (committed record, cost fields only).
- FP-E declaration — `hh-fp-e-decl/1` `2026-11-19-fp-e-decl.json` sha256 `6f2720be…` — same read — `engineered`.
- Measurement packet — `hh-fp-measurement/1` `2026-11-19-fp-ws1-measurement.json` sha256 `7bce8722…` + `hh-fp-trials/1` `2026-11-19-fp-ws1-trials.json` sha256 `6132f939…` — same read — `engineered` (retained WAL fold; no outcome fields).
- Comparative quote — `comparative-quote/1` `2026-11-19-fp-e-comparative-quote.json` sha256 `cf59cbec…` — `pre-quote` envelope — `engineered`.
- W4 compute quote — `hh-compute-quote/1` `2026-11-19-fp-w4-quote.json` `pq-4f15fc7015dc1db1` ceiling 7,988,890 µ$ — `engineered` (receipt_lib quote_doc re-issue).
- G-L1 disposition — GATE-G29 Reading 3 `full` (operator verbatim 2026-11-18T22:57Z; pass id `l1-pass:sha256:80bfafee…`, H `sha256:c451ed97…`) — `decision`.
- HUMAN-H15 — `docs/build/readouts/HUMAN-H15.md` Status `NOT_NEEDED` citing the quote — read 2026-11-19T00:34Z — `engineered`.
- W2 settlement — `pending` (clock-honest; confirming provider reads due ~05:00Z Nov 19) — `pending`.
- Prior archives — `archive_verified` W1, W2, W3, Wc, Wc2…Wc10, Wcs1, Ws1, smoke1, smoke2 (verify receipts `docs/build/reports/archive/`); smoke4/smoke5 backfill in flight under DF-R6.23-1 — `engineered`.

<!-- agent-drafted:begin sha256=38bb8dcba2e5715263c44c55bcce01ca4c43d91e33f0088d145cd2096aec1d42 -->
## Decision content for the operator

**The FP-E quote admits no executable work.** R6.46's committed `hh-fp-e/1`
(`docs/build/reports/2026-11-19-fp-e.json`, sha256 `d6fa1dab0538bf759581b9524f5184a6f74a402d73128eac82b904e6482b60da`)
is complete and honest — and every arm it quotes is withheld on cost fields:

| arm | disposition | reason |
|---|---|---|
| original mini-swe-agent | included, then withheld | protected; wall_fit fails — `withheld{window_deadline}` (protected_pair_truncated{k<5,R<5}) |
| HH-native mini-swe-agent | included, then withheld | protected; same wall_fit truncation |
| V5 | `withheld{budget}` | lowest priority; dropped first under REQ-XH-126/117 |
| Claude Code | `withheld{admission:AG3}` | PP0 union-of-failures (AG3+AG6) — retained_unavailable, repair = new participant version, later pass |
| Pi | `withheld{admission:AG3}` | same PP0 evidence |

`continuing: {}`; P2-FP / P3-L2 / P4-FP primary inference refuse
`withheld{window_deadline}:protected_pair_truncated{k<5,R<5}`.

**Why (cost fields only, REQ-XH-112 firewall):** the A2 first-pass compute pool (≤ $30) is spent
to ~330,000 µ$ headroom — completed windows at billed/cost_lib 28,192,409 µ$ + Wc restart-overflow
reservation 560,740 + W4 fixed 920,740 → **D_max 3,737,185 ms (~1.04 h)**. W4's own declared
overhead+load alone is 6,300,000 ms (1.75 h) > room 1,397,185 ms (D_max − admission cutoff). Even
one protected pair cannot be admitted before the cut-off → total truncation. This is a budget
exhaustion fact, not a machinery or task failure.

**Items 1–11 (all cost fields):**
1. Ws1-measured per-arm p̂ / t̂: mini 386,163 µ$ / 477,792 ms · HH-native 374,000 / 477,792 ·
   V5 374,000 / 477,792 · claude-code 210,000 / 551,157 · pi 123,000 / 383,870. Queue wait 0 ms
   all slots; C = 4; `wait_plus_latency_p99` 270,359 ms < admitted_client_timeout 300,000 (REQ-XD-062).
2. Inclusion by Q-X61 priority; V5 admitted only if room — none.
3. D_max 3,737,185 ms; wall drops: V5 then the protected pair; Wc ceiling released, restart-overflow
   reservation 560,740 kept (REQ-XH-126).
4. Per-arm caps (planned_slots × p̂): all 0 — no slots planned (caps.hh-arm table verbatim in the quote).
5. Purchase need **0** — HUMAN-H15 recorded `NOT_NEEDED` (needed root 7,472,434 ≤ root 48,000,000);
   Q-X63 default stands vacuously; A $70 / A_eff $60 / m .20 / root $48; carried pool consumed 4,072,434 µ$.
6. W4 `hh-compute-quote/1` re-issued `pq-4f15fc7015dc1db1` ceiling 7,988,890 µ$ (D 22.5 h, t_b 3,738 s
   = ⌈d_max/1000⌉, one harvest-only restart, project deletion at close) — the priced menu only; it
   authorizes nothing.
7. Model line: meter-enforced only (HUMAN-H14 verbatim; inside FP-S's A — no new authorization
   needed; nothing new is requested). Exclusivity for W4 is yours to acknowledge only if a run
   disposition is ruled (REQ-XH-013).
8. PP0 holds: pi + claude-code `withheld{admission:AG3}` — typed admission holds, stay in P2-FP's
   family unavailable; repair = `new_participant_version_later_pass` (REQ-XH-127(a)).
9. Authorizations on offer: model line within A ($0 new); compute W4 `pq-4f15fc7015dc1db1` ceiling
   7,988,890 µ$ — both under your standing ≤$50/day sign-off; hard-cap formula `cap_h = slots_h × p̂_h`.
10. Prior windows: W2 settlement `pending` (clock-honest — confirming reads due ~05:00Z Nov 19);
    archives `archive_verified` W1→Ws1 incl. every Wc/Wcs1 — **smoke4/smoke5 now backfilled
    verbatim** (`2026-11-19-fp-smoke{4,5}-{upload,verify}.json` + indexes; smoke4 cites wc9
    `ab1e8634…`, smoke5 cites smoke4 `b5f3c87e…`; PR #410, pending operator merge).
11. Every inclusion/drop labelled "cost-triggered; cost correlates with failure length".

**Prediction track (A3):**
| window | readout predicted | actual |
|---|---|---|
| Wc9 | verified machinery → covering | `machinery{h_allow}` pipe-drain defect → blockedOn |
| smoke4 | repair verified or new defect named | exec-drain VERIFIED + `machinery{model-port-collision}` named → blockedOn |
| smoke5 | port repair verified or envelope exhausts | VERIFIED 4/4 + envelope closed |
| Wc10 | covering verdicts → frozen | 12/14 covering + 2 typed exclusions → frozen |
| Wcs1 | bounded successor covers remainder | all slots covered |
| Ws1 | withheld legs execute → conformance | 37+4 trials executed; fold → replay re-fold `pass` (R3) |

**Pre-registered dispositions (your verbatim answer — the agent signs nothing):**
- `run{full}` — GATE-G29 recorded `full`; W4 runs the arms the quote includes. On this quote that is
  **zero executable slots** — the window would provision, withhold every slot at admission
  (cost_fit fits; wall_fit does not), burn ~920,740 µ$ fixed compute for no trials, and still run
  teardown + phase-close deletion. Legal but buys nothing.
- `run{hosted-only}` — precondition absent (G29 ruled `full`, not `fallback`).
- `end_after_w2` — its named trigger (held original mini-swe-agent, REQ-XH-127(b)) is absent —
  mini-swe-agent passed PP0. Not literally available.
- `re-scope{end-first-pass-early}` (A3 ladder) — record the outcome as the first-pass result, skip
  W4, claims withheld by typed reasons (the P2/P3/P4 `protected_pair_truncated` refusals + the
  `admission:AG3`/`budget` arm withholds). The honest fit for "the pool cannot fund any W4 arm":
  R6.47 then fleshes as the project-deletion/close act; downstream analysis folds over executed
  evidence only.
- `re-scope{harden-again}` / `re-scope{exclude-and-replace}` — no machinery defect or legit
  exclusion is in play.
- **Your own ruling** — e.g., an A2-style intervention amendment raising the compute pool so W4
  arms fit (a scope/budget amendment, your words verbatim), or any other disposition.

**DEFERRALS rule 4 (named, not decided):** `deferrals-open.sh` returns DF-R6.23-1 (the
gate-citation predicate — its smoke4/5 residual legs are now discharged: every prior window's
archive is `archive_verified`; the row stays OPEN verbatim for R6.47's own W4 leg, arguably
scoped to "the prior windows' archives" for this gate) and DF-R6.45l-1 (transitive verify-host
derivation — Wc-adjacent). Whether either blocks this gate is the operator's call.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decision (verbatim, received 2026-11-19T01:54Z via interactive chat): "ok sounds good to me. let's do that and then drive all the remaining work. i'd like ideally to authorize everything up front right now so that you won't need to keep pausing for input/authorization from me as we work through what remains"

Disposition: **PASSED — `re-scope{end-first-pass-early}`** — basis (A3 ladder arm, verbatim
precondition): the FP-E quote admits no executable work — the A2 pool is spent to ~330,000 µ$
headroom, D_max 3,737,185 ms < W_p 6,300,000; every arm withheld on cost fields (`budget` /
`window_deadline` / `admission:AG3`), `continuing: {}`, and P2-FP / P3-L2 / P4-FP carry
`protected_pair_truncated{k<5,R<5}` refusals. The verbatim is terse continuation language mapped to
the readout's own stated recommendation — "the honest fit for 'the pool cannot fund any W4 arm'" —
the only reading in which "sounds good" accepts a presented option; W4 is skipped, the outcome
stands as the first-pass result, claims stay withheld by typed reasons, and R6.47 fleshes as the
phase-close act (project deletion §3.9 step 7, `n/a` upload + settlement) instead of a metered
window. `run{full}`'s literal reading (~$0.92 M fixed for zero admitted slots) buys nothing and is
not chosen.

**DEFERRALS rule 4 disposition:** neither OPEN row blocks this gate. **DF-R6.45l-1 is
dispositioned here** — "let's do that" maps to the row's own named unblocker: the derivation
follow-up ticket (a closure-phase insert folding verify-phase-attributed `security.egress.decided`
deny rows into a candidate transitive verify set + a freeze-checker leg beside `scan-verify.sh`;
acceptance per the row — retroactive application to the smoke1 record emits exactly
{`download-r2.pytorch.org`, `ossci-datasets.s3.amazonaws.com`, `yann.lecun.com`} unaided). The row
stays OPEN verbatim pending that ticket's run. **DF-R6.23-1** stays OPEN verbatim for R6.47's W4
leg (its smoke4/5 residual is discharged — every prior window `archive_verified`).

**Standing directive recorded verbatim:** "drive all the remaining work" + "authorize everything up
front" — directed completion through the remaining chain (R6.47→DOC6.1) plus the authorized
follow-up insert; the milestone gates that remain (G31, G32) still present at their turns for the
operator's verbatim — advance authorization cannot sign evidence that does not yet exist — and
HUMAN-H16/H17/H18 stay operator acts (flagged review, the HF dataset-visibility flip, billing
reconcile figures). No spend beyond the ≤$50/day standing sign-off is inferred.

Operator confirmation of agent-drafted text (verbatim, 2026-11-19T01:54Z): "ok sounds good to me.
let's do that" — covers sha256:38bb8dcba2e5
GATE DECISIONS row: 2026-11-19T01:54Z | GATE-G30
Signed by: the operator. Recorded by Devin (SWE-2 High, coordinator), which does not sign.
