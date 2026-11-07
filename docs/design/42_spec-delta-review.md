# 42_spec-delta-review — Delta re-review of experiments spec v0.1 → v0.2
Stream: O
Researched: 2026-11-07T02:20:43Z
Author: independent fresh-context delta reviewer, reproduced verbatim by the coordinator
Ledger rows covered: O2 (delta)
Repository baseline: branch claude/experiments-spec 0877244d → 6c2dc6d9
Confidence overall: as stated per finding

---

# O2 delta re-review — experiments spec v0.1 → v0.2

- **Reviewer:** independent delta re-reviewer, fresh context (not the author or the reviser)
- **Recorded:** 2026-11-07T02:18:48Z (`date -u`)
- **Baseline:** worktree `MetaHarness-harnessharness-experiments`, branch `claude/experiments-spec` @ `6c2dc6d9` (v0.2). Diff reviewed: `0877244d..6c2dc6d9 -- docs/experiments-spec.md` (+537/−328). Findings source: `docs/design/41_spec-review.md`. Dispositions: spec Appendix E. Rulings: ledger §Q (Q-X22, Q-X47…Q-X63).
- **Mode:** delta-only (`synthesize-spec/modes/review.md`, round ≥ 2).
- **Mechanical checks run** (stdlib Python `-I`, no network; scripts in `scratchpad/rd/`):
  - `ids.py`, body vs Appendix A, every ID: 368 body rows = 368 adopted index rows; 454 index rows (338 adopted + 30 synthesis-allocated + 81 merged + 5 superseded). The per-family counts match the §0.2 table. Every hook and layer matches. Every alias has the `(target)` marker, its target's hook, and a mention in its target's *Absorbs* clause. AC hooks run in sequence within each subsection. There are no dangling `REQ-X` references. **0 errors.**
  - `q.py`, Appendix B vs ledger §Q: same ID set. Every quoted operator string and every answered Q-X47…Q-X55 cell matches the ledger byte for byte. Pending rows Q-X56…Q-X63 are mirrored; only Q-X62's text diverges (RD-6).
  - Appendix E: 62 rows, the same IDs as the review. Severities 4 / 29 / 29. 36 accept + 26 adapt; the stated counts hold.
  - `budget.py`, §10.1.5 recompute: every model row, cumulative and total reproduces. Compute ceilings: W1 5.0504, W2 2.4085, W3 2.2514, W4 7.5177, Wc 2.5656, Σ **$19.794**, about $0.21 under $20. Planning hours, the 159 prove trials, slice s = $0.848 and C·s = $3.39 reproduce. Usable root is $52.61 at A = $70 and $44.61 at A = $60. The claim that "A = $70 covers W2 + the protected pair ($50.07) and nothing more" holds at m = .20.
  - The reviewer's `calc/rb3.py`, re-run at ε = .20/.25, reproduces the §10.1.3 P(`equivalent`) table exactly (one cell: .885 against RB-3's .886). `calc/rb2.py`, re-run, reproduces the RB-9 figures (P(m ≥ 7) ≈ .197, mean non-tied 5.13, half-width .14).

## 1. Prior findings: blockers and majors

| finding | sev | status | evidence (v0.2 text) |
|---|---|---|---|
| **RA-1** | blocker | **closed** (residual: RD-1) | REQ-XC-330: "Each (harness × task image) layer and each bundle SHALL be built at most once per pass and SHALL be exported at the end of its build window … Each later window SHALL load it and verify the image ID (config digest), which is the identity the slot records; a registry RepoDigest … is never the identity. The HH runner, helper, relay and kernel binaries SHALL be built once per stage and carried the same way … A rebuild of any carried artifact is a new participant or a new runner version and needs a gate." W4 now carries every claim's arms (§5.2 "all of a claim's slots share one binary set"). |
| **RA-2** | blocker | **closed** (residual: RD-3) | REQ-XC-126: "slice SHALL be at least the arm's P90 $/trial … plus one worst-case call reservation w … w ≈ $0.10 … every arm SHALL get the same slice … s ≈ $0.85 … A slot refusal while the slot's consumed-to-date is below s − w is premature and SHALL be `inconclusive{infra:budget_reservation}`, never scored … Root-cap feasibility checks SHALL count the in-flight slices (C × s)". §3.3 table row "Premature slot budget refusal → `infrastructure_failure`". REQ-XC-003 → superseded (S-14). |
| **RB-1** | blocker | **closed** | §10.6: "Paid bodies follow REQ-XH-001 and REQ-XH-024 unchanged: every window's run rows (W1–W4 and Wc spend compute; W2 and W4 also spend model) are `Kind: skeleton` rows with no Run line until their gate passes, and a scoped `decompose-spec` insert then fills them … (Q-X58 recorded default)". §10.1.6 row for REQ-XH-001/024 says "unchanged". Q-X58 is mirrored in the ledger. (Scope wording: RD-9.) |
| **RB-2** | blocker | **closed** | REQ-XH-111: "Every main-window arm SHALL run in one window (W4) in one seeded interleaved schedule … with each (task, replicate) O/H pair launched together (… Q-X52)". FP-5: "L2 pairs launch simultaneously, so REQ-XD-111 applies unchanged." REQ-XH-113 AC: "independence rationale cites simultaneous launch in W4 and carries a catalog snapshot per round boundary, which must be equal or the cohort ends." The W5 string and the "time-separated" design are gone (only delta-paragraph and history mentions remain). |
| RA-3 | major | closed | Ledger IDs are used throughout. The only remaining Q-X47a–d / -P mentions are in the delta paragraph. Quotes are byte-identical (`q.py`). Pending items are real rows Q-X56/57. |
| RA-4 | major | closed | REQ-XH-103: the live-credential set is the Ollama key, the HF token and GCP credentials. Revoked slot tokens are kept in raw files and tombstoned in derived files. An unrevoked token counts as a hit. |
| RA-5 | major | closed | REQ-XH-107 sets the order rotate → purge → tombstone. The HUMAN row is in §10.3. Q-X59 is pending. |
| RA-6 | major | closed | REQ-XH-101 adds the store roots. REQ-XF-124 adds the roles `native_log` and `store_segment`. §4.10 states what is recomputable from what. (A new index gap is in RD-13.) |
| RA-7 | major | closed | REQ-XH-102 requires an upload per window, before the next paid gate. The DAG has ARCHIVE-UPLOAD-Wn after every window. |
| RA-8 | major | closed | REQ-XC-127 makes settlement a controller-side act, with a clock of ≥ 2 h after the last call's hour boundary. REQ-XC-121 keeps stale agreeing reads `pending`. Teardown step 9 records the clock start. |
| RA-9 | major | closed | §10.1.1 "What bounds what" gives each bound with its condition. The totals row is "Bounded by mechanism, under the stated conditions". The only "hard-bound" string left is in the delta paragraph. |
| RA-10 | major | closed | REQ-XB-128 sets the drain cut-off and incremental harvest. REQ-XB-224 pre-authorizes one restart per window, quoted in the ceiling. Q-X60 is pending. |
| RA-11 | major | closed (adapt a) | The W1 row lists builds, export, observe, census, smokes and R-replay. Wc ($2.57) holds one repair window inside $20. REQ-XB-125's repair rule is intact. |
| RA-12 | major | closed (Q-X55) | S-3: "No grant is made and no grant decision sits at PROV-W1." |
| RA-13 | major | closed (Q-X55) | The first-pass agent phase has no github.com. REQ-XD-070 adds the TB solution-repo signatures. Results name their granted hosts. (Wording issue: RD-16.) |
| RA-14 | major | closed (scope reduction) | R-live is dropped from the first pass (§5.7, REQ-XC-325, §10.1.6). Its later-pass egress posture must be stated before it runs. |
| RA-15 | major | **partially** | REQ-XC-302/214/311/324, §4.3 and §10.3 are rewritten. Still present: REQ-XD-041 "`withheld{admission:G<n>}`" (a G-number used as an admission gate) and "(window W1)" in a later-pass pilot requirement (RD-11). |
| RA-16 | major | closed | S-14…S-17 are in place. REQ-XA-010 is retargeted to REQ-XB-101 (index l.~2320). |
| RA-17 | major | closed | REQ-XC-101 now has the facade carve-out. Its AC reconciles the hosting leg and records that the leg is not in `ci.yml`. |
| RA-18 | major | **partially** | §10.1.6 maps REQ-XC-210/322/325 and REQ-XH-005/007/102. Still unmapped: REQ-XD-062 ("The pilot calibrates the in-flight limit") and REQ-XD-221 (P4 spend "at G-P4-X … outside the $60 pilot authorization"), although V5 is authorized at FP-E (RD-12). |
| RA-19 | major | closed | The baseline is `185c16c7`. REQ-XA-114 adds BL-50's note and both BL-RECONCILE records. |
| RB-3 | major | closed (Q-X53) | ε = ±.20. The N = 50 table reproduces. REQ-XH-122 gives a scoped label (but that label trips its own test: RD-5). |
| RB-4 | major | closed (Q-X54) | FP-8 makes the high line binding. The expected column uses $0.057/$0.028. The FP-S consequence is stated. |
| RB-5 | major | closed | Residual-disk (6 h), restart and egress-estimate terms are in every ceiling, and Wc is inside $20 (recomputed $19.794). The margin is now $0.21 (see RD-2, RD-13). |
| RB-6 | major | closed | REQ-XH-120 makes the pass one phase with one project and a carried root pool; an unharvested ledger blocks the next window; billing-off or meter-only is recorded at FP-S. |
| RB-7 | major | closed (residual RD-3) | The protected pair is exempt. "protect-e" is gone. There is one formula, p̂ = 1.2 × max(1.5·mean, P90). Caps are given by formula, with values at FP-E. |
| RB-8 | major | closed | REQ-XH-112: re-projection at every round boundary withholds V5 → Claude Code → Pi, never a protected arm while an unprotected one remains. |
| RB-9 | major | closed | The §10.1.3 table reproduces. REQ-XD-024 adds T = 10, H = 3/2. REQ-XH-119 adds the near-zero-power label. |
| RB-10 | major | closed (residual RD-4) | REQ-XH-121 seals outcomes. The firewall paragraph states that cost is outcome-correlated. |
| RB-11 | major | closed (Q-X52) | REQ-XH-114 uses a development-task corpus partitioned by task 3/2/2 (12/8/8). The REQ-XD-119 audit draws from W4 O recordings only. H is identified by rule before any L2 subject. |
| RB-12 | major | closed | `phase3.json`/`phase4.json` and REQ-XF-143 (F2-h) are added. "equivalent" is now `conditional`. Bare "leader" is defined. |
| RB-13 | major | closed | §10.1.6 is completed. AR-33 is added and listed in the method-admission gate. |
| RB-14 | major | closed (Q-X62 pending) | FP-S follows the E2-T8 rung record (DAG and §10.3). REQ-XH-123 adds the fallback and expiry. The GATE AR-24 amendment is in the DAG and §10.3. (Q-X62 text drift: RD-6.) |

The minors spot-checked (RA-20…37, RB-15…25) are folded as Appendix E states. Their W2 row values recompute.

## 2. New findings introduced or exposed by the revision

### RD-1 · major · REQ-XC-330 (§5.2), §10.1.4 W3 row, §10.1.5 W3 compute row, §3.3 `identity`
**Evidence.**
- REQ-XC-330 requires "in the first pass, one binary set for W1–W3 and one frozen set, fixed at G-L1, for every W4 slot".
- W1 builds "HH binaries" before the E2 chain lands. The DAG has `{E2-T10, K4} ──► GATE PROV-W3`.
- W3 runs "L1.0, L1.1 and L1.2 on the frozen H". That H needs the E2-T0…T10 kernel code, which does not exist at W1. W3 therefore cannot use the W1 set.
- The W4 set must be the bytes that G-L1 passed, yet no window builds or exports it. The W3 quote lists only "L1 run … 4.0 h", with no build or export line and a 1 GiB egress estimate.
- The carried set also leaves out the `hh-model-proxy` and egress-mediator binaries. Both mutate or decide traffic, and neither digest is in the slot `identity` ("runner, helper and relay binary digests").

**Fix.** Replace the sentence with: "In the first pass: an infrastructure set (runner, helper, relay, `hh-model-proxy`, egress mediator) is built in W1 and carried by digest through W4. The H set (kernel binary + HIR/compiled artifacts) is built once at the start of W3, exported from W3 with sha256, run by W3's L1 and frozen by digest at G-L1. W4 loads both sets, and every W4 slot records both digests." Add the proxy and mediator digests to §3.3 `identity`. Add "H-set build + export (≈ 0.5 h, UNVERIFIED)" to the W3 row of §10.1.5 and re-sum. The W3 ceiling grows by about $0.16; Wc or the egress line has to absorb it.

### RD-2 · major · §10.1.5 compute table (W4: W_p 19.6 h, D 21 h), REQ-XB-128, REQ-XH-016/125, REQ-XH-112
**Evidence.**
- W4's W_p = 250 × 1,040 s / 4 + 1.5 h = 19.56 h, against D = 21 h.
- REQ-XB-128 stops admissions at D − (largest agent + verifier deadline + harvest), which is roughly 0.5–0.7 h. The effective slack is therefore about 0.7–0.9 h, or 4 %.
- W_p excludes the phase-2/4 retries that REQ-XH-016 allows (≤ 10 % of plans) and that REQ-XH-125 confines to the same window. With 25 retries, W_p = 21.36 h > D, which breaks REQ-XH-118's own "W_p ≤ D". The 1,040 s prior is also UNVERIFIED.
- The Σ ≤ $20 margin is $0.21. FP-E can therefore raise W4's D by only about 0.65 h, and no text lets FP-E release the unused Wc reservation to W4.
- Every continuation rule projects the model root only. REQ-XH-112 lists "compute ceiling" as a cost field, but no rule uses it.
- Under interleaving, a wall-time shortfall truncates round 5 of every arm, protected pair included. That gives k < 5 for all arms on the truncated tasks and R < 5 for L2. The disposition of slots never admitted before the cut-off is undefined.

**Fix.**
1. Add the declared retry allowance to W_p: W4 W_p = (250 + ⌈0.1 × 250 non-L2 slots⌉) × t̄ / C + overhead.
2. At FP-E, recompute W4's W_p from W2-measured wall time per arm. If W_p > D_max, where D_max is the largest D that keeps Σ ≤ $20 and may include Wc's unused reservation, apply the cost-only priority order to wall time *before W4 starts*: drop V5, then Claude Code, then Pi. Record this as a pre-registered rule in REQ-XH-112 and REQ-XH-118.
3. At each W4 round boundary, also project remaining wall time against the REQ-XB-128 cut-off and withhold by the same order.
4. Slots not admitted before the cut-off are `withheld{window_deadline}`, and an O/H pair is never split.
5. Add a fixture for each rule.

### RD-3 · major · REQ-XH-117 vs REQ-XC-126
**Evidence.**
- Admission rule: "An arm's slot is admitted only when its consumed + reserved + one slot slice (REQ-XC-126) fits its cap", with cap = planned slots × p̂_h.
- The slice is uniform and sized from the most expensive arm (s ≈ $0.85), while Pi's cap at the binding prior is 50 × $0.123 = $6.15.
- With one Pi slot in flight, the 50th slot needs consumed + 2s ≤ 6.15, i.e. consumed ≤ $4.45. Any Pi mean in ($0.093, $0.123) therefore withholds Pi's last ~6–8 slots (round 5) while its true 50-trial spend still fits the cap. The same band for Claude Code is about ($0.18, $0.21).
- This is RA-2's "cheap arms are stopped early" defect at the admission layer. It is typed `withheld{budget}`, not scored. But it makes Pi's k < 5 on several tasks, so Pi's P2-FP member becomes unavailable for a sizing reason rather than a cost reason.

**Fix.** Either set cap_h = planned_h × p̂_h + C × s, or admit on expected cost (consumed + Σ in-flight p̂_h + p̂_h ≤ cap_h) and draw the slice reservation only against the root (whose check already subtracts C × s). Add an AC fixture: an arm running exactly at p̂_h/1.8 completes all 50 slots.

### RD-4 · major · REQ-XH-121 vs REQ-XB-125, REQ-XH-116, AC-3.8-1, REQ-XC-324
**Evidence.**
- REQ-XH-121 seals "First-pass outcome fields (slot verdict, reward, `ctrf` counts, outcome members of the disposition, verifier output and every derivative) … gate readouts … carry cost fields only", with the AC "an outcome field in a gate readout … fails the schema check".
- The W1 oracle re-proof slots are first-pass slots. Their verdicts must be read to admit or exclude tasks: REQ-XB-125's packet is "reviewed at the W1 gate", REQ-XH-116 drives replacements, and FP-PREREG freezes the post-replacement list.
- The $0 smoke battery is AG3/AG5/AG6 evidence, and SM-402 "slot verifies".
- Read literally, the seal makes the re-proof and admission gates unpassable.

**Fix.** Scope REQ-XH-121 to "agent-participant slots of W2 and W4". State explicitly that oracle re-proof slots, smoke-battery records and PP0 admission-gate evidence (AG3/AG5/AG6/AG8 are not task verdicts) are unsealed. Clarify §10.1.4's "mini-swe-agent's PP0 trials pass" as "pass AG3/AG5/AG6/AG8", not verdict success.

### RD-5 · minor · REQ-XH-122, REQ-XD-126, AC-10.1-13
**Evidence.** The mandated label is "first-pass L2 equivalent at a ±.20 margin (N = 50; gross-infidelity screen; **not a parity-verified baseline**)". The mutation test says "'parity-verified baseline' beside a first-pass L2 packet fails". The required label therefore fails its own check. This is the same collision class as RB-12's "leader".
**Fix.** Reword the tail to "…; not parity-verified)". Alternatively, define the check as the phrase not preceded by "not a ", and add that case to the mutation battery.

### RD-6 · minor · Appendix B Q-X62 and §10.1.4 vs ledger Q-X62
**Evidence.**
- Ledger: "hosted-only fallback fires if G-L1 has not passed 14 days after FP-S is otherwise ready".
- Spec (§10.1.4 and Appendix B): "14 days after W2's teardown, or E2-T8 landing on C-05 rung 3".
- The anchor differs, and the rung-3 trigger is absent from the ledger. A pending default must be mirrored identically, because the operator rules on the ledger row.
**Fix.** Make the coordinator re-sync the ledger row to the spec text, or the spec to the ledger. Record which is authoritative.

### RD-7 · minor · REQ-XH-005 vs §10.1.5 "Reading the model table", §10.3 FP-E content
**Evidence.**
- REQ-XH-005's later default is m = .10 "once a `settled` reconciliation from REQ-XC-127 exists". REQ-XC-127 requires W2's settlement before FP-E.
- At FP-E the default m would then be .10, giving root $63 and usable $59.6 at A = $70. Under that root, W2 + the pair + Pi ($56.22) fit at planning.
- §10.1.5's readings and §10.1.1's table assume m = .20 throughout, and FP-E's gate content omits m.
**Fix.** State whether m is fixed at .20 for the whole first pass or re-declared at FP-E. Add `m` to the FP-E content, and qualify the reading accordingly.

### RD-8 · minor · FP-12, §10.1.1 "Model bill", HUMAN credit-purchase row, REQ-XC-004
**Evidence.**
- With A = $70 authorized at FP-S and no purchase (because the FP-E quote did not need one), the meter root stays $56. With billing off, the provider stops at the $60 available credit.
- A meter under-count of more than 7 % would then end W4 calls with provider refusals mid-round. Those are receipted infra events, uneven across rounds and arms.
**Fix.** At FP-E, either purchase up to A − included-remaining, or re-declare A for W4 as the available credit (included remaining + purchased) and recompute the root. Record which one in the FP-E readout.

### RD-9 · minor · §10.1.6 REQ-XH-001/024 row vs §10.6 and Q-X58
**Evidence.** §10.1.6 says "the W2 and W4 run rows are `Kind: skeleton`". §10.6 and Q-X58 say "every window's run rows (W1–W4 and Wc …)". W1, W3 and Wc spend compute, so they are paid bodies.
**Fix.** In §10.1.6, replace "the W2 and W4 run rows" with "every window's run rows (W1–W4, Wc)".

### RD-10 · minor · FP-10 (§0.3.4), §10.3 gate table
**Evidence.**
- FP-10's chain "PROV-W1 → FP-S → W2 → G-L1 → FP-E → W4" omits PROV-W3 and W3.
- The DAG's "GATE PROV-Wc" has no §10.3 row, so REQ-XH-025 has no pre-registered content to quote.
**Fix.** Use "PROV-W1 → W1 → FP-S → W2 → PROV-W3 → W3 → G-L1 → FP-E → W4". Add a PROV-Wc row with: compute quote; which of W1′/W3′/restart-overflow; D, restart and h_r; scope of the repair; $0 model.

### RD-11 · minor · REQ-XD-041; S-17
**Evidence.**
- REQ-XD-041 still contains "`withheld{admission:G<n>}`" and labels a later-pass pilot step "(window W1)", which reuses a first-pass reserved token.
- S-17 keeps REQ-XC-201's literal "gates G1–G8" as a merged predicate. REQ-XH-003 preserves that predicate's bytes, and S-12 reserves G-numbers for HIR gaps.
**Fix.** Change REQ-XD-041 to `withheld{admission:AG<n>}` and "(its provisioning window)". Add one sentence to S-17: "the alias's literal 'G1–G8' denotes AG1–AG8 under S-12".

### RD-12 · minor · §10.1.6 adaptation table
**Evidence.**
- REQ-XD-062: the pilot calibrates the in-flight limit. The first pass fixes C = 4 and has no pilot.
- REQ-XD-221: P4 spend is authorized at G-P4-X/C, "outside the $60 pilot authorization". The first pass authorizes V5 at FP-E inside A.
- Neither requirement is mapped.
**Fix.** Add two rows: "REQ-XD-062: C = 4 declared; W2's queue-wait measurement is reported at FP-E, which may lower C" and "REQ-XD-221: first-pass V5 spend is inside A, authorized at FP-S and included at FP-E (FP-6)".

### RD-13 · minor · REQ-XC-330 vs REQ-XF-124; §10.1.5 W1 egress
**Evidence.**
- REQ-XC-330 indexes layer and bundle exports "in `hh-receipt-index/1`". REQ-XF-124's index is per slot attempt, and its roles (`raw_receipts|transcript|atif|egress_log|native_log|store_segment`) have no artifact role.
- The load path into later VMs (controller → VM over IAP) and its time are unquoted.
- About 52 layers (mini 20, Pi 16, Claude Code 16) sit against a "≤ 6 GiB" estimate. The compute margin is $0.21, and each extra GiB costs $0.12.
- Archiving the Claude Code bundle to an HF dataset, even a private one, is not checked against Q-X26's terms.
**Fix.**
- Add a pass-level `artifacts[{role: layer_export|bundle|binary_set, …}]` section to `hh-receipt-index/1`.
- State the load path and add its time to each later window's overhead.
- Make PROV-W1 refuse if the measured export size breaks Σ ≤ $20, and name what shrinks.
- Exclude proprietary bundles from upload unless Q-X26's terms allow it; keep them as a digest-only index entry.

### RD-14 · minor · §10.1.4 W2 row ("PP0 infra hold … holds that harness for repair"), Wc row, §10.1.2 development cells
**Evidence.**
- A repair is a new participant version, which needs a gate and a window under REQ-XC-330. Wc is $0 model and offers only W1′/W3′/restart. A Pi or Claude Code PP0 hold therefore has no first-pass path.
- A mini-swe-agent hold blocks the corpus. REQ-XH-123's fallback triggers only on G-L1.
- The band-A development easy cell has 1 task, so an exclusion there has no within-cell replacement.
**Fix.**
- A held unprotected arm is `withheld{admission:AG<n>}` for the pass and drops from FP-E.
- A held mini arm triggers the hosted-only fallback's analogue, or a recorded scope decision at FP-E.
- A development-cell exclusion with an empty cell is replaced from the adjacent cell, and the substitution is disclosed.

### RD-15 · minor · Appendix C
**Evidence.**
- The binding-inputs sentence still ends "…carrying Q-X51…Q-X55; the coordinator then records Q-X47)". That is a v0.1 leftover; the later rows are Q-X56…Q-X63.
- The O2 row has no sha256 prefix. `41_spec-review.md` is `cd8c6dbe39f484b2` at `e4190705` and in the working tree.
**Fix.** Correct the sentence and add the prefix.

### RD-16 · minor · REQ-XB-116, §3.5
**Evidence.** "Explicit denies cover … `github.com/harbor-framework/*`". The agent registry channel is CONNECT, which reveals the host only. The same paragraph says "a granted shared host cannot be path-filtered", so a path deny is unenforceable whenever github.com is granted (later passes).
**Fix.** Use: "host-level denies: `hub.harborframework.com`, `tbench.ai`; `github.com` is denied whenever it is not granted (always in the first pass); when granted, `github.com/harbor-framework/*` is an integrity-scan signature (REQ-XD-070), not a deny".

### RD-17 · minor · §3.3 outcome table
**Evidence.** Row 1, "Valid reward (any end cause, including timeout or budget refusal) → `scored`", overlaps the new "Premature slot budget refusal → `infrastructure_failure`" row, and no precedence is stated. REQ-XB-120 demands "no relabelling".
**Fix.** Change row 1 to "… or a non-premature budget refusal" and state that the premature row takes precedence.

## 3. Verdict

**Verdict: ready to ratify.** All four blockers (RA-1, RA-2, RB-1, RB-2) are closed in the text, and the revision introduces no new blocker. 27 of the 29 majors are closed or credibly adapted. RA-15 and RA-18 are each partially closed, with small residuals (RD-11, RD-12). The four new majors (RD-1 binary-set staging, RD-2 W4 wall-time against D and the $20 margin, RD-3 per-arm cap against the uniform slice, RD-4 seal scope against the re-proof) are text-level fixes. They should be folded or dispositioned at O3, before the hand-off digest is frozen.
