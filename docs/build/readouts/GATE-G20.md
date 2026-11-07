# GATE-G20 — Review the Round-6 plan and authorize the GUARD-X bootstrap · readout
Status: PASSED

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is approval.

## Criterion (verbatim from the spec)

> | R6 plan review + bootstrap | first | decomposition | partition, coverage register; authorizes only GUARD-X |
>
> **REQ-XH-002** The first ticket after Round 6's bootstrap gate SHALL extend `hh-supplement-guard/1` to this supplement: second frozen-source digest, REQ-X coverage register, Round-6 routing (AR-31). No product ticket dispatches before it lands.
>
> **REQ-XH-025** Every gate marker SHALL quote its thresholds verbatim from this spec before the gated work begins. A pending gate is RETURN PASS, never `blockedOn`.

## Evidence (per item: source, `date -u` of the read, layer reached)

### Item 1 — artifact identity (recomputed at presentation)

- `docs/experiments-spec.md` sha256 `4329b2740fe6be0ff80be585388bae63e1802ed34740478ba4a37d9d03e78531` — read 2026-11-07T06:10Z — **differs from the hand-off's `0bcfb3572dcc0bc9…`**: the recorded §10.6 dated amendment (decomposition sizing — 74 rows / 49 implement-spec / `tail=minimal`, operator-verbatim 2026-11-07) explains the drift; the ratified text sha256 `7373d5205f8a…` (commit `47baec98`) is the ratified basis verbatim in the status line — layer: engineered
- `docs/experiments-decomposition-prompt.md` sha256 `fd7fe273d559ec1e…` — read 2026-11-07T06:10Z
- planning files sha256 (first 16): `2026-11-07_round6-decomposition.md` `5ed657f768206f47` · `…-coverage.md` `30bbb936a1848256` · `…-coverage.json` `8fcea993231fe139` · `…-invariants.md` `b08c5a11b7b17392` · `check_round6_coverage.py` `d6fa89dcdac06e88` — read 2026-11-07T06:10Z
- `check_round6_coverage.py .` re-run at presentation: **RESULT: PASS (35/35)** — read 2026-11-07T06:10Z
- `main` `185c16c7` ancestor of chain base: **yes** (verified at presentation; `origin/main` `347ae3ae`) — read 2026-11-07T06:10Z
- Manifest Round-6 banner `tail=minimal` present (line 503); rows 252–325 present (74 files) — read 2026-11-07T06:10Z
- LEDGER: `round: 6` · `nextTicket: GATE-G20` · `chainTip: main` · `buildBranchBase: codex/harnessharness-evidence` · `pinnedBaseSha: 001490c14a…` — read 2026-11-07T06:10Z
- `bash scripts/deferrals-open.sh` output verbatim at presentation: **empty** (0 open DF rows); `--bl` → `BL-50 | open` (the round-5 scope-disposition carrier) — read 2026-11-07T06:10Z

### Item 6 — bootstrap route + the 46-warning disclosure (quoted, never re-baselined)

- `check-supplement-guard.sh` (full, stock checker): `route: HOLD — gate GATE-G20 pending — RETURN PASS`; `counts: 74 active-round rows · 366 chain rows · 180 candidates · stock_violations 0 · stock_warnings 8` — read 2026-11-07T06:10Z
- `check-supplement-guard.sh --no-checker`: same HOLD route — read 2026-11-07T06:10Z
- Stock validator `check-build-memory.sh .`: **0 violations / 8 warnings (baseline)** — read 2026-11-07T06:10Z
- **46-warning OPEN FINDING (honest observation):** the hand-off discloses 46 `unexplained-diagnostic` legacy `reqcov` warnings as an OPEN FINDING for R6.1 (AC-3). **At presentation the stock checker emits ZERO `reqcov` warnings** (0 matching lines; the 8 baseline warnings are all other classes). The count recorded here is the actually-observed `0` — the planning-time observation is quoted verbatim and stands as the finding's provenance; R6.1 owns reconciling the discrepancy. The disclosed warnings list's sha256 is therefore `n/a{empty_at_presentation}` — recorded, never re-baselined.

## Pre-registered decisions for the operator (each separate)

2. **Partition + coverage register** — accept or redirect: 74 rows seq 252–325 (49 implement-spec runs = 45 tickets + 4 tail runs; 13 gates; 7 HUMAN; 5 skeletons); register 456 candidates / 370 adopted, each exactly one primary owner.

3. **Q-X46 linearization** — `A` (phase 1 + site shell first, the plan's order) or `B′` (the checked permutation `linearization-B-prime`: R6.6, R6.20, R6.21, HUMAN-H12, GATE-G22, R6.31 moved to just after GATE-G29). No recorded default.

4. **Deviations** (each `accept` or `refuse`; a refused item returns to scoped re-plan):
   - D-1 ARCHIVE-UPLOAD-W1 runs as the first step of W2's fleshed skeleton (R6.40), after GATE-G26 and before PROV-W2 — departs REQ-XH-102's "before the next paid window's gate"
   - D-2 five window reconcile reads consolidated in HUMAN-H18 before the tail; REQ-XH-008's sequence read as ordered per-window subsequence
   - D-3 `tail = full` replaced by `tail=minimal` (banner-declared, split to CAP6.1/CAP6.2/CAP6.3 → GATE-G32 → DOC6.1)
   - D-4 RECIPE's §10.6 pre-split (a/b) merged into R6.13, B3's seam kept as in-row stage boundary
   - D-5 row/implement-spec counts below the ≈100–110/85–90 band, rows ≤95k focused + 10k reserve
   - D-6 Wc block placed between W3 and G-L1

5. **Interpretations** — confirm or redirect the plan's twelve "Spec ambiguities resolved here" + REQ-XH-002's "second Round-6 row" read as R6.2.

6. **Scoped bootstrap authorization** — authorizes **R6.1 only**; post-gate ROUTE must read `DISPATCH — R6.1`; stock validator 0 violations. Expiry: R6.1 landing; from R6.2 on `PASS` with stock checker required. **Void if:** reviewed artifact bytes change, base moves without re-review, or validator reports a violation.

7. **Not authorized:** product semantics, spend, provisioning, publication, external writes or merges.

## Disposition

**PASSED — 2026-11-07T06:10Z · interactive operator chat · verbatim: "accept all and proceed"**

- Item 1 (artifact identity): **confirmed** — recomputed digests accepted verbatim incl. the recorded §10.6 amendment sha256 `4329b2740fe6be0f…` and the 0-observed `reqcov` count.
- Item 2 (partition + register): **accepted** — 74 rows / 49 implement-spec / 456 candidates / 370 adopted.
- Item 3 (Q-X46): **A** — "accept all" accepts the plan as presented, which is linearization A (the plan's order); B′ is a re-order of the plan, not selected. Recorded interpretation, flagged for the operator to redirect if it misreads intent.
- Item 4 (D-1…D-6): **all accepted**.
- Item 5 (interpretations): **confirmed** — the twelve ambiguity resolutions + "second row" = R6.2.
- Item 6 (bootstrap): **authorized — R6.1 only**; post-gate route verified `DISPATCH — R6.1` below; expiry at R6.1 landing; void conditions per contract.
- Item 7 (non-authorizations): acknowledged — no spend/provisioning/publication/merges by this gate.
- Carried scoped items: the 46-warning OPEN FINDING (observed 0 at presentation) → R6.1 AC-3; BL-50 open carrier.
- Agent-draft block hash: presented state above confirmed by the operator's verbatim acceptance.
