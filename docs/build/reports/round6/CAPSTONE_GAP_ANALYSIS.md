# CAP6.1 — Round-6 Capstone Gap Analysis

Ticket: `321_CAP6.1__cap1.md` (manifest row 321) · Base: `codex/harnessharness-r6.50` @ `31fc815b` · Analysis date: 2026-11-19 · Analysis records: `docs/build/reports/round6/COVERAGE_MATRIX.csv` (456 rows), `docs/build/reports/2026-11-19-cap6.1-gap-hunt.json` (machine packet), checker `docs/build/tools/cap6_gap_hunt.py` (records-in, read-only).

## 0. Method and ordering declaration

**Verdicts were recorded before reading any `docs/build/runs/*` run ledger.** Stage 1 inputs were only: `docs/experiments-spec.md` (frozen canonical v0.3, sha256 per the register's `supplement_sha256` pin), `docs/build/planning/2026-11-07_round6-coverage.{json,md}`, `2026-11-07_round6-decomposition.md`, `2026-11-07_round6-invariants.md`, `docs/tickets/00_MANIFEST.md` rows 252–321, `docs/tickets/DEFERRALS.md`, `docs/build/LEDGER.md`, `docs/build/BUILD_INDEX.md`, all `docs/build/readouts/*`, all committed records under `docs/build/reports/` and `docs/results/`, and `git` history. Run and PR ledgers are cited in the matrix only as landed-evidence *anchors* (existence-checked, prose unread at verdict time). The mechanical fold lives in `cap6_gap_hunt.py`; every number below reproduces from it.

Verdict vocabulary is BM-VERDICT-01. Acceptance domains are BM-STATUS-01 rungs (`engineered` < `fixture-verified` < `recorded-execution` < `live-executed`(ADR-0363) < `public`; `human-completed` orthogonal). `required_domain` maps the register layer D→engineered, E→engineered, R→recorded-execution, P→public.

## 1. Verdict roll-up

| verdict | rows | share |
|---|---|---|
| MET | 409 | 89.7% |
| MET-DIFFERENTLY(ADR-0439) | 1 | 0.2% |
| PARTIAL | 46 | 10.1% |
| MET-ENGINEERED / AT-RISK / WAIVED / MISSING / N/A | 0 | 0% |

Engineering-closed (MET + MET-DIFFERENTLY): **410/456 = 89.9%**. Satisfied = 410. No register id is unrowed.

PARTIAL decomposes (never counted satisfied):

| reason | ids | home |
|---|---|---|
| later-pass routing (decomposition) | 41 ids owned by `CAP6.2` | CAP6.2 routing register (its own deliverable) |
| scope-ended execution, GATE-G30 `re-scope{end-first-pass-early}` | REQ-XH-111, REQ-XH-115, REQ-XD-110, REQ-XD-119 | CAP6.2 later-pass execution |
| committed executor never built nor run | REQ-XC-325 (R-replay + SM-* runner check) | CAP6.2 + proposed DF row |

Register integrity (NC-11): 456 candidates = 370 adopted + 81 merged-with + 5 superseded-by; layers E=330, D=92, R=33, P=1 — matches spec §0.2/Appendix A. All 86 aliases inherit their targets' verdicts; the literal predicate is retained at the target, so an alias MET is an alias of a satisfied predicate, not a second claim.

## 2. Findings register

Genuine gaps/seams are typed and routed; verified-clean items cite evidence in §3–§4.

| id | sev | finding | home |
|---|---|---|---|
| GAP-01 | high | **Ledger GATE DECISIONS region.** Eleven round-6 decision rows (GATE-G28i, G28j, G28k, G28l, G28m, G28n, G29, G30 ×3 readings, G31, HUMAN-H15) were appended at the tail of `## OPEN FINDINGS` (LEDGER.md lines 135–147), **outside** the `## GATE DECISIONS` region (line 151). `supplement_guard.load_decisions` parses only that region, so at base tip the guard emits `HOLD — gate GATE-G28i pending — RETURN PASS`, `nextTicket 'CAP6.1' is not the active round's first unlanded row (GATE-G28i)`, and `paid-body R6.47 … GATE-G30 is not recorded PASSED`. The decisions exist and are PASSED; the parser cannot see them. GATE-G28i's row also lost its date cell in commit `0bffdb3c` — an in-place rewrite of a committed row (append-only violation). | CAP6.2 — append-only repair: dated corrective decision rows inside the region; never move or rewrite history |
| GAP-02 | high | **Frozen-prereg fixture drift.** `hh-fp-prereg/1` (2026-11-10) pins arm `admission_sha256` `7407ce0e…`/`e0bd56c0…`/`658cc589…`; current `crates/hh-runner/tests/fixtures/r6_22/*.json` hash `3526f2b5…`/`67c150d6…`/`ff68ecf6…` after one-line `config.template_hash` amendments at `b07adfdd` (11-16, R6.45n model-port repair), `3d7fd37b`/`cdd2d169` (11-17, render/pi fixes). `check-fp-prereg.sh` emits `emit-identical` FAIL on the packet and its sidecar; the emitter itself is faithful to current bytes. Arm-bearing execution (W2 PS0/PP0) predates the amendments, so the inference chain for executed arms is intact; the drift is real record-state mismatch needing a dated amendment record — the frozen packet must NOT be rewritten. | CAP6.2 reconciliation + proposed DF row |
| GAP-03 | medium | **REQ-XC-325 R-replay/SM-* never executed** — "no committed executor — honestly unexecuted" (BUILD_INDEX R6.32b row). Not operator-accepted: PARTIAL, routed CAP6.2, owed leg `runner-check execution`. | CAP6.2 + proposed DF row |
| GAP-04 | medium | **Register↔plan `primary_kind` drift.** `check_round6_coverage.py` FAIL 34/35: 10 ids' JSON `primary_kind: ticket` vs decomposition row-table `skeleton` — REQ-XB-005/XB-125 (R6.32), REQ-XD-041/XH-010 (R6.40), REQ-XD-107/XE-006/XE-007/XE-008/XE-009 (R6.44), REQ-XH-116 (R6.32). The window rows were fleshed skeleton→ticket; the plan table's kind column was never updated. | CAP6.2 — dated note on the plan table (append-only) |
| GAP-05 | medium | **Manifest/table ordering + annotated dependency cells.** Insert rows 309a–309w physically precede row `309_GATE-G29`, so `R6.45o` (309s) `Depends on: GATE-G29` resolves to a later chain row (guard forward-dependency violation). Annotated Depends cells `GATE-G29 (recorded ruling)` (309r) and `GATE-G30 (disposition recorded 2026-11-19); …` (R6.51) parse as non-row ids. `309p_GATE-G28k` carries `## Criterion (the scope-decision this gate serves)` vs the expected `## Criterion (verbatim …)`; `313_R6.47` retains `Kind: skeleton` after in-place fleshing. | CAP6.2 — dated notes + cell normalization |
| GAP-06 | medium | **DF-R6.50-1 lead-token vs dated disposition.** The row leads OPEN but a dated DONE discharge exists (HUMAN-H17 + phase-3/4 flip records, `dataset.visibility: public`, all embedded pins verified — NC-4b). `deferrals-open.sh` still emits it: the scanner reads the lead token, not the dated marker. Genuinely discharged content-wise; the emitted-OPEN is a row-reader seam owed at round closure (AGENTS rule 8). DF-R6.45l-1 is genuinely OPEN (transitive verify-host derivation). | CAP6.2 — lead-token reconciliation |
| GAP-07 | low | **Phantom deferral ids.** BUILD_INDEX names `DF-R6.45b-1`/`DF-R6.45b-2` as opened/discharged; DEFERRALS.md has no such rows — a deferral not in the file did not happen (window-internal machinery rows tracked only in run ledgers). | CAP6.2 — mint real rows or annotate the index |
| GAP-08 | low | **`gen-revisit-triggers --check` drift:** ADR-0450 and ADR-0451 appear in no BACKLOG.csv sources cell — 2 unplaced live ADRs. | CAP6.2 |
| GAP-09 | low | **`check-compute-receipts.sh` world mismatch:** crashes (`'list' object has no attribute 'get'`) on `docs/build/reports/compute/2026-11-18-fp-ws1r-refold-diffs.json`, a top-level `[]` — the checker assumes every compute/*.json is an object with `.schema`. | CAP6.2 (checker repair is out of CAP6.1 scope) |
| GAP-10 | low | **`fp_e_emit.rs:189` `panic!("spend {w}: cost_usd {other:?} unreadable")`** — base clippy finding, stands at base. | CAP6.2 |
| GAP-11 | low | **`live-executed` vocabulary on the HUMAN-H17 flip readout** — describes an operator HTTP PATCH + read-back GET; honest act, but the committed evidence is the signed readout + flip records, not an `hh-live-evidence/1` (ADR-0363). Descriptive use, not a capability claim. | CAP6.2 — vocabulary note |
| GAP-12 | low | **Stock-checker false positive:** "manifest chain row names GATE-G30.md but no such file exists" — `312_GATE-G30__fp-e.md` exists; the checker resolves a bare-id filename. | CAP6.2 |

## 3. Verified-clean (no finding)

| surface | evidence |
|---|---|
| `.sha256` sidecars (35 under reports/ + publication/) | NC-4: every `shasum`-format sidecar recomputes over its payload |
| Publication embedded pins | NC-4b: every `scan_refs`/`inputs`/`dataset_card_ref` sha256 in `docs/results/publication/*flip*.json` recomputes over committed bytes |
| W4 `n/a{scope_end}` honesty | NC-6: upload + settlement records carry `status: n/a{scope_end}`, `metered_model_calls: 0`, `HF_WRITE_TOKEN: no`, basis citing GATE-G30 verbatim; teardown record shows project deletion (`post_state` empty) |
| Gate/human readouts | NC-5: every dispatched round-6 gate/human row has a committed non-PENDING readout (GATE-G32 is a future row 324 — excluded) |
| Shared-decision table | NC-8: all 38 rows — owner precedes every consumer in chain order |
| `withheld{}`/`n/a{}`/`unused{}` tokens | NC-10: 23 distinct typed tokens across records; sampled uses are typed refusals with basis, never favorable claims (P2-FP `withheld{admission:AG3}` cites a declared gate) |
| `live-executed` label | NC-7: every machine-field claim maps to a committed `docs/build/reports/live/*` record; publication `outside_verified: false` everywhere; prose uses audited (descriptive/negated/conditional/cited — see GAP-11 for the one vocabulary note) |
| Register↔spec counts | NC-11: 456 = 370+81+5; layers match §0.2 |
| Owner landed-ness | NC-2/2b: every adopted first-pass id's owner has a BUILD_INDEX row + run/pr anchors |
| Acceptance invariants | AC-1: one row per id, MET cites evidence, achieved ≥ required; AC-2: no accepted_scope raises a verdict past MET-ENGINEERED |
| Archive chain | upload+verify records carry `verification: archive_verified` over a private dataset; read-back commands exit 0 |

## 4. Seam hunt detail (required axes)

- **Verbatim withhold tokens vs claims:** all sampled `withheld{…}`/`n/a{…}`/`unused{…}` tokens are typed refusals with a cited basis (gate verbatim, scope record); none appears as a favorable result. The W4 pair `n/a{scope_end}` is backed by the GATE-G30 scope decision and zero metered calls.
- **SHA-256 pins vs actual files:** sidecars 35/35 and publication embedded pins verify. Exception → GAP-02 (frozen prereg arm-admission pins vs current fixture bytes — a frozen-record-vs-drifted-input mismatch, not a corrupted record).
- **Gate dispositions vs what executed:** disposition labels match window outcome records (e.g., GATE-G29 `full` ⇄ committed strict-replay fold; GATE-G30 `re-scope{end-first-pass-early}` ⇄ W4 n/a records). The parser-visible subset agrees; the region misplacement (GAP-01) is a layout defect, not a disposition defect — no gate was inferred passed from a later record.
- **Deferral discharge text vs verify clauses:** DF-R6.50-1's verify clause is met verbatim (HUMAN-H17, dataset revision `a11591e9…`, public flip records, pins verified) — content-discharged; scanner seam → GAP-06. DF-R6.45l-1 remains genuinely open.
- **`n/a{scope_end}` honesty:** NC-6 — used only where work genuinely never ran; W4 has no harvest and no fabricated upload.
- **Fabricated labels:** none found. `frozen`, `pass`, `archive_verified`, settlement and publication labels each trace to committed records; `live-executed` machine claims map to committed live evidence.
- **Known checker drift:** GAP-08/09/10 plus the stock-checker GAP-12 false positive; each recorded, none silently repaired (outside CAP6.1 scope).
- **Shared-decision rows:** all 38 table rows checked for owner-precedence (NC-8) and cross-ticket evidence presence; seams concentrated in the ledger/manifest surface (GAP-01/04/05), not in the shared-decision data itself.

## 5. Accepted deviations — represented as deviations

GATE-G20 (`docs/build/readouts/GATE-G20.md`, Status: PASSED, 2026-11-07T06:10Z, interactive operator chat) recorded verbatim **"accept all and proceed"**. Under it, the accepted items were: artifact identity; partition + register (74 rows / 49 implement-spec / 456 candidates / 370 adopted); **Q-X46 → linearization A**; **D-1…D-6 all accepted**; the twelve interpretations; R6.1 bootstrap authorization; non-authorizations acknowledged. The represented deviations in this analysis:

| deviation | where represented |
|---|---|
| SR-18 — ARCHIVE-UPLOAD-W1 runs as W2's first step under the FP-S upload authorization (departs REQ-XH-102's "before the next paid window's gate") | REQ-XH-102 matrix row `note` cites the deviation + GATE-G20 verbatim; verdict MET (the requirement's *intent* — verified upload chain before dependent window — is met and the deviation is named, not hidden) |
| Re-partition departures from §10.6 (3-run→4-run minimal tail; RECIPE a/b merged with the in-row stage boundary; row count below first-cut band) | tail rows 321–325 in the manifest; CAP6.1/6.2/6.3 + GATE-G32 + DOC6.1 |
| Q-X46 linearization A (plan order) over B′ | the chain as landed; R6.51/CAP6.1 reads it as the plan |
| REQ-XD-215 vs REQ-XD-220 (SR-25 shared-baseline reading) | R6.46 row scope; P4 family-of-one prereg label |
| REQ-XD-115 margin exception ε=±.20 at FP-S prereg (ADR-0439 departures admitted GATE-G23) | REQ-XD-115 verdict `MET-DIFFERENTLY(ADR-0439)` |

No accepted deviation is represented as a plain pass.

## 6. Routing and proposed deferral text

**Routed to CAP6.2** (all PARTIAL ids + every finding above). Proposed new D-rows for the coordinator's DEFERRALS.md (worker may not edit it):

```
| DF-CAP6.1-1 | OPEN | ledger GATE DECISIONS region: 11 round-6 decision rows (GATE-G28i…G28n, G29, G30×3, G31, H15) sit inside OPEN FINDINGS, invisible to supplement_guard.load_decisions — guard computes HOLD 'gate GATE-G28i pending' + nextTicket/paid-body violations; GATE-G28i's row lost its date cell in 0bffdb3c | append-only corrective decision rows inside ## GATE DECISIONS; verify guard route no longer HOLDs on G28i | CAP6.1 | CAP6.2 |
| DF-CAP6.1-2 | OPEN | check-fp-prereg emit-identical FAIL: frozen packet arm admission_sha256 pins (7407ce0e…/e0bd56c0…/658cc589…) vs current fixture bytes (template_hash amendments 11-16/17 at b07adfdd/3d7fd37b/cdd2d169) — frozen record vs drifted inputs | a dated amendment/deviation record binding the fixture drift (never rewrite the frozen packet); emit-identical leg amended or annotated | CAP6.1 | CAP6.2 |
| DF-CAP6.1-3 | OPEN | REQ-XC-325 R-replay + SM-* runner check: no committed executor — honestly unexecuted | committed executor + executed record, or an operator-accepted waiver | CAP6.1 | CAP6.2 |
| DF-CAP6.1-4 | OPEN | manifest/plan/register surface drift: register primary_kind vs plan skeleton (10 ids); inserts 309a–309w precede row 309 (forward-dep); annotated Depends cells (309r, R6.51); 313 Kind: skeleton stale; GATE-G28k criterion heading variant; DF-R6.50-1 lead token vs dated DONE; phantom DF-R6.45b-1/-2 | dated notes + cell normalization at round closure | CAP6.1 | CAP6.2 |
| DF-CAP6.1-5 | OPEN | checker drift: check-compute-receipts crash on top-level-array record; gen-revisit-triggers ADR-0450/0451 unplaced; fp_e_emit.rs:189 panic clippy; stock-checker GATE-G30.md filename false positive | per-checker repair tickets or baselined diagnostics | CAP6.1 | CAP6.2 |
```

**Environment blocks routed to the operator:** none new — GCP compute unavailability is already honestly recorded in HUMAN-H18; all CAP6.1 checks ran offline at $0.

## 7. Honest bottom line

Round 6's first pass **engineering-closed 410/456 register ids** with zero MISSING and zero fabricated labels. The execution tail honestly records its boundaries: W4 `n/a{scope_end}` under GATE-G30, GCP compute unavailable, $4.52 metered. The five PARTIAL-with-home items (41 routed + 4 scope-ended + 1 unexecuted) are typed, never satisfied. The genuine gaps are **record-keeping seams, not capability overclaims**: eleven gate decisions invisible to the guard parser, a frozen-prereg input drift that fails emit-identical, plan/register surface drift, a row-reader/dated-disposition mismatch, phantom DF ids, and five checker-drift items — all routed to CAP6.2 with proposed DF text. Nothing in this analysis was inferred from run-ledger prose; run ledgers are cited as landed anchors only.
