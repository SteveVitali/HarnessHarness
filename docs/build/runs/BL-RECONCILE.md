# Run ledger — BL-RECONCILE (reconcile BL-50…BL-53 against post-GATE-G17 evidence)

- **Harness:** devin-desktop/swe-2-high/subagent
- **Observed runtime identity:** Devin Desktop subagent context (parent: devin-desktop coordinator); declared tier swe-2-high; exact model id is not exposed to this session — no independent self-report available, recorded honestly. Nesting depth 1: no subagent spawning this run.
- **Skills:** agent-skills @ e981ec5 — build-memory + reconcile-build (check-backlog.sh)
- **Started:** 2026-11-06T22:55Z
- **Closed:** 2026-11-06T23:06Z
- **Scope:** `docs/build/BACKLOG.csv` rows BL-50/51/52/53 — honest backlog maintenance, same class as `reconcile-build mode=backlog`. Reconcile the four stale backlog rows against the evidence that landed after they were opened at REC5.1 (2026-11-05).
- **Authorization:** operator task dispatch (non-manifest records-in work; no code touched, USD $0.00)
- **Base:** `origin/main` `@09fe8523` (merged R5.59 tip — the 251-row chain complete)
- **Branch:** `codex/harnessharness-bl-reconcile`
- **ci:** pending at close

## Gate status

No gate. Non-manifest records-in work: `bash docs/build/tools/check-supplement-guard.sh`
(AGENT_SKILLS_ROOT=$HOME/agent-skills) returns **PASS, route `HOLD — active round
fully landed`** (counts: 180 candidates / 87 active rows / 0 violations / 8
baseline warnings) — the manifest chain is complete (`nextTicket: DONE`), so no
`DISPATCH — <id>` route exists for this work. Same posture the DF-CAP5.1-1 and
DF-R5.1-1 deferral runs recorded.

## Deferrals read (at SETUP, before edits)

`bash scripts/deferrals-open.sh` — **empty** (zero open DF rows; DF-R5.1-1,
DF-H9-1 and DF-CAP5.1-1 all carry dated DONE dispositions).

`bash scripts/deferrals-open.sh --bl` — four open rows:
`BL-50` (40-id GATE-G17-carried scope-disposition carrier),
`BL-51` (DF-H9-1 withheld participant/outreach predicate set, 23 ids),
`BL-52` (DF-CAP5.1-1 failed S1 useful residual, 9 ids),
`BL-53` (DF-R5.1-1 guard-boundary contributor evidence, 1 id).

## Evidence verified (parsed, never taken on faith)

- `reports/2026-11-05-df-cap5.1-s1-useful.json` — `hh-s1-useful/1`
  `disposition: executed`, all seven legs `verified`, `useful_acceptance:
  marked`, `packet_id 7bbd15a2…c13a82`; `oracle_chain` facts
  `subject_a: decided:1000000`, `subject_b: decided:1000000`.
- `reports/2026-11-05-df-cap5.1-s1-decided-artifacts.json` —
  `hh-s1-decided-artifacts/1`: both artifacts `outcome: decided`,
  `reward_ppm: 1000000`, verbatim `payload_hex`/`attempt_ref`/`verdict_ref`.
- `reports/2026-11-05-df-r5.1-evidence.json` — `hh-guard-boundary-evidence/1`
  v1.0.0: EV-1…EV-6 `verified`; EV-1 records all **67** work-row dispatch
  boundaries carrying guard pre+post `PASS DISPATCH — <row>` (R5.2 stub-leg
  transcripts VALID); EV-2 counts 180/82/0 viol/8 warn.
- `reports/2026-11-05-parity-tranche-3.json` — `hh-parity-tranche-report/1`
  `verdict: verified` — EXACT directional agreement **30/30** pairs, zero
  discordance, `protocol_id 9edc5327…`; S3 met.
- `reports/2026-11-06-s4-30slot-repairs.json` — `hh-s4-30slot-repairs/1`:
  counts executed **30/30**, inconclusive 0 — completing the four-epoch chain
  to 300/300 (160 + 90 + 20 + 30; first subjects 60/60).
- `reports/2026-11-06-r5.59-efficacy.json` — `hh-s4-efficacy/1`:
  `verdict: computed{strata:easy,hard,medium}` — observed 0 / posterior 0
  ppm, credible95 intervals symmetric about zero (honest null-equivalent).
- `reports/2026-11-06-r5.59-study-report.json` — `hh-study-report/1`
  `disposition: reported`: parity `verified{30_30}`, confirmation
  `executed{300_300}`, efficacy leg computed, `s4_unmet:false`,
  `study: unmet{s1_failed}` carried verbatim.

## Per-row disposition applied

- **BL-52 → closed, satisfied.** Its named exit was the narrow deferral
  condition — "an executed `hh-s1-useful/1` packet with an independent Decided
  artifact" — and it is met verbatim by the packets above (operator-authorized
  re-execution "approved, proceed", PR #280; DF-CAP5.1-1 DONE 2026-11-05).
- **BL-53 → closed, satisfied.** The boundary-evidence record its exit named
  is committed (`…-df-r5.1-evidence.json`, all six legs verified over the 67
  boundaries); DF-R5.1-1 DONE 2026-11-05.
- **BL-51 → closed, scope-dispositioned — never satisfied.** Its exit named
  "a fresh scope disposition"; DF-H9-1 carries the operator's verbatim
  2026-11-05T16:04:15Z disposition **permanently unmet-by-decision** (no
  access to outside humans). The participant/outside-reviewer predicates are
  honestly unmet — dispositioned, never satisfied and never waived.
- **BL-50 → stays open, dated note.** Its R-layer legs discharged by real
  evidence (parity 30/30; S4 300/300; efficacy computed null-equivalent;
  GATE-G19 PASSED; study `reported`). Its P-layer legs (publication
  REQ-G-201/202/207/209, value-observation REQ-G-206) stay voided/carried —
  no publication or adoption evidence exists and none was authorized.

## Re-home mechanics (the dedup rule kept green)

`check-backlog.sh` gathers every round-5 PARTIAL matrix id and requires it on
exactly one *open* row. The three closing rows' 33 still-PARTIAL ids therefore
re-home to BL-50's `sources` cell — the BL-12/BL-21 convention (sources cell
empties on the donor row; `req_ids` keeps the row's tracked requirement set;
landing records the re-home). BL-50 now homes all 73 round-5 PARTIAL ids.
The CAP5.1-era matrix verdicts are **not** re-verdicted by this records-only
reconcile — re-verdicting requirement rows is capstone-class independent
verdict work; the still-carried legs (P-layer, participant-layer) and the
evidence-landed legs are recorded on BL-50's dated note until that pass.

## Writes (all shell/python per the write-integrity rule; `git diff` verified)

- `docs/build/BACKLOG.csv` — four rows: BL-50 sources/req_ids += 33 re-homed
  ids (73 total, sorted) + dated landing/gate notes, status `open`;
  BL-51/52/53 sources emptied, `status: closed`, dated landing/gate notes
  (CRLF endings on the round-5 rows preserved; LF rows byte-identical).
- `docs/build/BACKLOG.md` — header reconcile marker, Mode line, dated
  machine-form note, `*reconcile read (2026-11-06):*` paragraph appended
  after the REC5.1 read (historical text untouched).
- `docs/build/LEDGER.md` — CURRENT STATE `chainTip`/`updatedAt`; PHASE LOG —
  Round 5 entry appended (1,878 B < 2 KiB).
- `docs/build/BUILD_INDEX.md` — `—`-seq reconcile row appended.
- `docs/build/runs/BL-RECONCILE.md`, `docs/build/pr/BL-RECONCILE.md` — this
  ledger + PR body.

## Verification

- `bash scripts/deferrals-open.sh` → empty (0 open DF rows — before and after).
- `bash scripts/deferrals-open.sh --bl` → **BL-50 only** (BL-51/52/53 dropped).
- `bash scripts/check-bm-tools.sh` → OK.
- `check-backlog.sh` (agent-skills, default `docs/build/` matrix) → exit 0,
  206 expected sources, complete + non-duplicating + verdict-consistent.
- `check-backlog.sh` (round-5 matrix `docs/build/reports/round5`) → exit 0,
  73 expected sources — every round-5 PARTIAL id on the open BL-50 exactly
  once; no `home-closed`.
- `check-build-memory.sh` (external bm validator) → recorded at close.
- `check-supplement-guard.sh` → PASS, route HOLD (round landed).
- `git diff` reviewed — write-integrity rule satisfied.

## Deferrals

None opened, none closed by this run (all DF rows already carry dated
dispositions). BL state transitions: BL-51/52/53 closed; BL-50 stays open.

## Deviations

None.

## Next

Unchanged — `nextTicket: DONE` (the 251-row manifest chain is complete).
chainTip → `codex/harnessharness-bl-reconcile`.
