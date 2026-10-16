# GATE-G5 readout — Round-3 discharge review

Status: PENDING   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

*Append-only. Each operator reading is a new dated block; never edit an earlier one.*

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is approval.

## Criterion (verbatim — the gate ticket `docs/tickets/148_GATE-G5__round3-discharge.md`, incl. its 2026-10-14 amendment)

> Every DEFERRALS row and backlog source the Round-3 tickets were chartered to discharge is DONE with
> dated evidence, or carried forward with an explicit operator disposition (carry to Round 4 / accept
> as standing deviation / waive by ADR). The named set — HUMAN-H5's record (ruled 2026-10-14,
> disposition (b): external crypto crate admitted; dep-choice ADR inside R3.7), whatever residual the
> un-gated R3.7/R3.8/R3.9 tickets report, and the still-open human and environment rows
> (HUMAN-H1/H2/H3/H4 — provisionable per the operator's 2026-10-14 statement, the foreign-verification
> cells) — is dispositioned as a named set: 'gate pending' is a valid recorded state; a silent drop is
> not.

**Blocks:** R3.11 (`docs/tickets/149_R3.11__round3-docs-refresh.md` — the round-3 docs refresh runs
after the gate so the docs state the round's outcome; **the last manifest row, 149 of 149**).

**Pre-registered thresholds** *(verbatim from the ticket)*:

- Zero `OPEN` DEFERRALS rows that an R3 ticket was chartered to land and did not (each is landed,
  honestly residual with a dated note, or dispositioned here by the operator).
- The three flake families are repaired or carry recorded adjudications; the G4-4/G4-5/G4-6
  bookkeeping items are swept.
- `check-build-memory.sh` 0 violations; `check-backlog.sh` exit 0; `^||` grep over DEFERRALS empty.
- The R3.10 closure accounting exists and the carried set is enumerated with reason codes.
- The 10 GATE-ACCEPT accepted deviations are unchanged (re-confirmed as still-accepted, not
  re-litigated); the GATE-G4 §9 dispositions are not re-litigated.

**DEFERRALS rule applied (docs/tickets/DEFERRALS.md rule 4):** *"Gates refuse to pass while any
`OPEN` row scoped to that phase remains. Treat an open row as gate-blocking."* — the ticket's own
clause: *"This gate does not pass while any `OPEN` row in `docs/tickets/DEFERRALS.md` scoped to this
phase remains undispositioned."*

**Amendment record (2026-10-14):** the seed text named HUMAN-H5 + R3.7/R3.8/R3.9 as the *gated* set;
the operator's same-day rulings un-gated all four (H5 answered (b); R3.7/8/9 became executable
tickets). The criterion above reads current truth.

## Evidence (per item: source, `date -u` of the read, layer reached)

- The gate ticket `docs/tickets/148_GATE-G5__round3-discharge.md` + its 2026-10-14 amendment — read 2026-10-16T18:31Z — layer: gate marker
- `docs/tickets/DEFERRALS.md` — read 2026-10-16T18:33Z — layer: records (100 rows — 81 discharged with dated evidence, 19 owed: 18 `OPEN` + 1 `PARTIAL`, every owed row dated)
- `docs/build/planning/2026-10-14_round3-decomposition.md` — read 2026-10-16T18:33Z — layer: records (`## Round-3 accounting — appended at closure (R3.10)` + §"The carried set for GATE-G5 — per-row reason codes", 21 rows)
- `docs/build/BACKLOG.md` + `docs/build/BACKLOG.csv` — read 2026-10-16T18:36Z — layer: records (156 expected sources; the seven round-3 discharges closed; the 21 carried rows open/accepted)
- `docs/build/COVERAGE_MATRIX.csv` — read 2026-10-16T18:37Z — layer: records (66 rows: 47 MET + 6 MET-DIFFERENTLY + 2 MET-ENGINEERED + 11 PARTIAL)
- `docs/build/LEDGER.md` — read 2026-10-16T18:29Z — layer: machine state (CURRENT STATE; GATE DECISIONS incl. the 2026-10-14 HUMAN-H5 ruling (b), the R3.9 CF-489 direction ruling, and the H1/H2/H3-provisionable statement; OPEN FINDINGS incl. LEDGER-OF-6)
- `docs/build/readouts/GATE-ACCEPT.md` — read 2026-10-16T18:38Z — layer: records (the 10 signed deviations, `Status: SIGNED`)
- `docs/build/readouts/GATE-G4.md` — read 2026-10-16T18:32Z — layer: records (the structural template; §9 dispositions — re-confirmed, not re-litigated)
- `docs/build/CAPSTONE_CLOSURE.md` — read 2026-10-16T18:41Z — layer: records (the ACCEPTED-deviations list; unmodified since the ratified set landed `bcf1649`)
- `docs/build/OPERATIONAL_READINESS.md` — read 2026-10-16T18:38Z — layer: records (the post-R3 capability account; ceiling `fixture-verified`; the `foreign-run-verified` class stated)
- `docs/build/BUILD_INDEX.md` rows 137–146 — read 2026-10-16T18:38Z — layer: records (R3.1–R3.10; PRs #137–#146, all open, all ci-boundary reads green)
- `docs/build/runs/R3.1.md`…`R3.10.md` + `docs/build/logs/ci-R3.*.json` — read 2026-10-16T18:38–18:42Z — layer: run ledgers (flake repairs, the `syspolicyd`/helper-spawn host notes, the size flag)
- `docs/tickets/00_MANIFEST.md` rows 137–149 + `## Decomposition decisions → Round 3` — read 2026-10-16T18:39Z — layer: the plan (incl. the not-scheduled recorded set)
- `research/registers/conflicts.md` — read 2026-10-16T18:39Z — layer: registers (the open CF set)
- `ci-boundary/1` re-read at this gate — `docs/build/logs/ci-R3.10.json` @ `ca04c04` read 2026-10-16T18:29Z — **pass #146** (build · test · fmt · drift 38940780100 / 38940698193); stack #137–#146 + #135 all OPEN, all `pass`
- `check-build-memory.sh .` re-run at this gate — 2026-10-16T18:29Z — **0 violations** (warning count: flag G5-1)
- `check-backlog.sh docs/build/BACKLOG.csv docs/build docs/tickets` re-run at this gate — 2026-10-16T18:35Z — **exit 0** (156 expected sources; 55/66 engineering-closed, 53/66 requirement-satisfied; complete + non-duplicating + verdict-consistent)
- `grep -n "^||" docs/tickets/DEFERRALS.md` re-run at this gate — 2026-10-16T18:30Z — **0 matches**

This readout presents the evidence and leaves every disposition cell blank for the operator.

## 2026-10-16 — Reading 1 (orchestrator-authored evidence audit) → PENDING operator disposition

<!-- agent-drafted:begin sha256=85a016814722cb95d80de9f7062104608b9c55b7c49298e5b5267bcbcf2332aa -->

### 1 · Chartered-vs-landed — every Round-3 row's chartered work, accounted

The decomposition (`planning/2026-10-14_round3-decomposition.md` §Phase-1 cluster→ticket map, as
amended by the three operator rulings the same day) chartered each Round-3 row a named scope. The
closure accounting confirms: **every chartered row is either DONE with dated evidence or sits on
exactly one carried backlog row with a reason code (§2). No chartered row is unlanded and
unaccounted.**

| Row | Chartered (cluster map + contract) | Outcome at closure |
|---|---|---|
| R3.1 (PR #137) | The three flake families (G4-1 oauth timing, G4-2 ac5 `derived_from.view_hash`, G4-7 `r2_14` loopback) + the G4-4/G4-5/G4-6 bookkeeping items | **All three root-caused and repaired deterministically — no retries:** `verify_minted` `>`→`>=` (the expiry instant is dead; boundary regression test; 25/25); `derived_from.view_hash` joined the `ends_with` derived-wall allowlist with the derivation input named (the `assembly_ms.value` wall inside the `context_view` fold prefix; 25/25); both loopback fixtures drain the declared `Content-Length` before answering (25/25). 16 stale status leads swept (G4-4); DF-S2.4-2 gained a dated residual note (G4-5); `BACKLOG.md` counts corrected (G4-6). LEDGER ac5 finding repaired-closed. |
| R3.2 (PR #138) | BL-44 — DF-S2.3-1's five residual producer legs | **DONE** — all five landed or verified-landed (`resume_set`, `defer`, child-lease, non-fleet ingress, `heal(env_handle)`); DF-S2.3-1 DONE 2026-10-14; BL-44 closed; R-2.2.3's first flip leg. |
| R3.3 (PR #139) | BL-14 — DF-S1.20-1's interpreter/delegation legs | **DONE** — DF-S1.20-1 DONE 2026-10-14; BL-14 closed; R-2.6.1 + R-2.6.3 flipped MET (ADR-0355). |
| R3.4 (PR #140) | BL-15 — DF-S1.19-1/-2 §5c residuals | **DONE** — both rows DONE 2026-10-15; BL-15 closed; R-2.4.1/R-2.4.2/R-2.4.3/R-2.4.4 flipped MET (ADR-0356); the pre-registered R3.4a/R3.4b split seam was never invoked. |
| R3.5 (PR #141) | BL-16 — DF-S1.17-1 members + ADR-0272 `sync_source`/`retrieval_eval` legs | **DONE** — DF-S1.17-1 DONE 2026-10-15; BL-16 closed; R-2.5.3 flipped MET; ADR-0272 fired-answered → BL-43 (ADR-0357). |
| R3.6 (PR #142) | BL-10 — DF-S1.9-2 emitter-vs-exemption adjudication | **DONE** — all 11 no-producer codes adjudicated (ADR-0358: C-LINK-8/-9 real emitters, C-REF-2/-3 re-classified, seven U rulings with named owners); DF-S1.9-2 DONE 2026-10-15; BL-10 closed; R-2.1.4 flipped MET; CF-492 registered → BL-30. |
| R3.7 (PR #143) | BL-19 — dep-choice ADR + `dpop` verify + `wrapped_long_lived` (DF-S1.13-1/-4), un-gated by HUMAN-H5 | **DONE** — ADR-0359 admits `ring = 0.17.14` to `hh-secrets` under a closed scope (H5 disposition (b)); ADR-0360 governs the broker surface; DF-S1.13-1/-4 DONE 2026-10-15; BL-19 closed; R-2.8.3 → `MET-ENGINEERED(DF-S2.4-2)` (the preserved typed refusal closes the dpop leg). Residual: the TLS-terminating transport leg stays env-gated → BL-31. |
| R3.8 (PR #144) | BL-45 — per-event blob encryption + R-2.8.6 legs (DF-S1.15-3) | **DONE** — ADR-0361 (`HHBE` frame, `BlobCipher`/`RingBlobCipher` over `ring::aead` within ADR-0359's admitted scope); DF-S1.15-3 DONE 2026-10-16; BL-45 closed; R-2.8.6 stays PARTIAL honestly — custody legs re-homed to BL-30 (ADR-0345 trigger (b)). |
| R3.9 (PR #145) | CF-489 — `DebtPolicy::default()` conform (operator ruling: code conforms to spec) | **DONE** — §5h.6's 60 d `min_runway` / 30 d `warn_within` landed; CF-489 resolved 2026-10-16; the OQ-441/WS-I6 Stage-5 re-pin marker and BL-30/BL-37 stay open on their other named-workstream rulings. |
| R3.10 (PR #146) | Closure reconcile — backlog refresh, coverage recompute, OPERATIONAL_READINESS, the carried-set accounting | **DONE** — the Round-3 accounting + the 21-row carried set with per-row reason codes (§2); coverage 55/66 · 53/66; CF-493 + LEDGER-OF-6 registered; BL-01's E2 legs recorded `foreign-run-verified`; BL-42/BL-43 recounts 56/49. |
| HUMAN-H5 (row 143) | Crypto-dependency posture ruling | **RULED 2026-10-14** — disposition (b): a vetted external crypto crate admitted; the dep-choice ADR lives inside R3.7 (ADR-0359 names the crate, the ≥7-day-old pin, and the admitted scope). Record discharged — see §7. |
| R3.11 (row 149) | Round-3 docs refresh | **BLOCKED by this gate** — not yet run; the docs refresh runs after the gate so the docs state the round's outcome. The last manifest row. |

**Reading:** zero `OPEN` DEFERRALS rows that an R3 ticket was chartered to land and did not — each
chartered row is `DONE` with dated evidence, and the one chartered-adjacent residual (DF-S2.4-2 —
chartered only a dated note, not a landing, per the decomposition's decision 6) carries two dated
notes and a named home. The operator dispositions the carried set in §9.

### 2 · The carried set — 21 rows, verbatim from the Round-3 accounting

*Transcribed verbatim from `docs/build/planning/2026-10-14_round3-decomposition.md` §"The carried set
for GATE-G5 — per-row reason codes" (appended at closure, R3.10 · 2026-10-16). This is the set the
operator signs — the per-row disposition table is §9.*

Reason codes: `human` — a person's act is the only remaining leg · `env` — needs a provisioned
environment (foreign toolchain, live provider, TLS endpoint, live judge) · `dep` — blocked on a
primitive/dependency the hermetic build lacks (or on a dependency-side ruling such as WS-L6 trust
fields) · `in-build` — machine-achievable residual carried on capacity · `accepted` — gate-accepted at
GATE-ACCEPT; arms execute when their environments exist · `ruling` — spec/governance ruling owed, not
code · `window` — scheduled to the WS-L6 program window · `standing` — perpetual ledger row, re-issued
never closed.

The GATE-G4 28-row set minus the seven round-3 discharges (BL-10/14/15/16/19/44/45) = **21 rows
carried to GATE-G5**:

| BL | Carried work | Reason |
|---|---|---|
| BL-01 | E3 replay, the HIR/ledger-corpus extension, the two-toolchain C12 cells (E2 legs foreign-run-verified per PR #136 — a partial discharge the bundle cannot yet spell, CF-493) | `env` + `accepted` |
| BL-02 | R2 cross-camp human reviewer signature (DF-S0.3-3's scoped residual) | `human` |
| BL-03 | HUMAN-H1 — E3 surface-ecosystem binding (DF-S4.10-1; provisionable per the operator's 2026-10-14 statement, unprovisioned) | `human` |
| BL-04 | HUMAN-H2 — real tracker + signed webhook (DF-S5.6-1; unprovisioned) | `human` |
| BL-05 | Gate-accepted battery arms (item 10 — incl. DF-S1.15-1's custody-riding halves and DF-S1.21-2's arms) | `accepted` |
| BL-17 | Judge binding + live-judge admissibility (DF-S1.21-3; R-2.7.1/R-2.7.2b's residual) | `env` |
| BL-26 | Live foreign-manifest import (DF-S1.24-2; R-2.9.4/R-2.9.6's residual; HUMAN-H3) | `env` |
| BL-27 | oauth/mtls CallerCredential mediator legs (DF-S4.11-2; R-2.11.3's residual; WS-H3) | `env` |
| BL-30 | Spec/governance rulings — signer custody (OQ-170/DF-S2.5-1), OQ-388 ratification (DF-S4.13-1), CF-476/487/488/490/491 + CF-492/CF-493, R-2.2.1/R-2.8.6 verdict residuals, ADR-0345 trigger (b), the OQ-441/WS-I6 re-pin marker | `ruling` |
| BL-31 | Live provider transports + the TLS-terminating transport (DF-S2.4-2 — the residual R-2.8.3/R-2.8.4 both carry as MET-ENGINEERED legs; DF-S1.18-1 live variants; TLS/live-sink-bound triggers ADR-0266/0340/0341/0346) | `env` |
| BL-32 | Remote fetch transports + foreign import/export vocabularies (DF-S4.2-1/-2 — dep-gated on WS-L6 trust fields; R-2.9.3/R-2.10.3/R-2.10.5 residuals) | `dep` + `env` |
| BL-33 | WS-L6 package (R-2.12.4 + 13 OQs) | `window` |
| BL-34 | ADR-0211 ratified deferrals D-1..D-6 + OQ-461/462/463 | `window` |
| BL-35 | ADR-0212 register package — remaining open OQs | `window` |
| BL-36 | ADR-0213 register package — remaining open OQs | `window` |
| BL-37 | ADR-0214 register package — remaining open OQs (incl. the OQ-441 Stage-5 re-pin) | `window` |
| BL-38 | ADR-0215 program candidates + OQ-440 | `window` |
| BL-39 | ADR-0216 fixer deferrals | `window` |
| BL-41 | Ledger OPEN-FINDINGS standing rules (incl. LEDGER-OF-6's status-marker sweep) | `standing` |
| BL-42 | 56 quiet/dormant ADR triggers | `standing` |
| BL-43 | 49 fired-and-answered ADR triggers | `standing` |

And the three pending cells the bundle still lists but no foreign run has discharged —
`polyglot-ci.two-toolchain` (DF-S0.3-2 → BL-01), `bench-manifest.live-import` (DF-S1.24-2 → BL-26),
`lt-03.live-corpus-arm` (DF-S1.13-3 → BL-05) — plus the format-gap decision (CF-493 → BL-30) the
partially-discharged record now needs. HUMAN-H5's record is discharged (ruled 2026-10-14, disposition
(b); ADR-0359 owns the admitted scope) and HUMAN-H1/H2/H3/H4 stay `gate pending` rows on their BL
homes — provisionable per the operator's 2026-10-14 statement, none provisioned in-build.

### 3 · Coverage roll-up — recomputed matrix

`docs/build/COVERAGE_MATRIX.csv` re-read at this gate: **47 MET · 11 PARTIAL · 6 MET-DIFFERENTLY ·
2 MET-ENGINEERED · 0 MISSING · 0 AT-RISK-INTEGRATION** — **55/66 engineering closed** (MET +
MET-DIFFERENTLY + MET-ENGINEERED) and **53/66 requirement satisfied** (MET + MET-DIFFERENTLY),
confirmed independently by `check-backlog.sh` (sums 55/66, 53/66). Movement: 47/66 on both sums at
the R2.26 closure → 55/66 · 53/66 at R3.10.

The round-3 PARTIAL→MET flips, stated exactly: **R-2.2.3** (R3.2+R3.3), **R-2.4.1/R-2.4.2/R-2.4.3/
R-2.4.4** (R3.4), **R-2.5.3** (R3.5), **R-2.6.1/R-2.6.3** (R3.3), **R-2.1.4** (R3.6) — plus one
closure re-verdict: **R-2.8.4 → `MET-ENGINEERED(DF-S2.4-2)`** — every §5g.4 engineering leg landed
(CAP.3 mediated dispatch; R2.9a ask-endorsement ingress + fork-rebind; the dpop residual closed by
R3.7's preserved typed refusal); the sole owed leg is the env-gated TLS transport on BL-31 — the same
residual its sibling R-2.8.3 already carries.

The 11 remaining PARTIAL requirements — R-2.2.1, R-2.7.1, R-2.7.2b, R-2.8.6, R-2.9.3, R-2.9.4,
R-2.9.6, R-2.10.3, R-2.10.5, R-2.11.3, R-2.12.1 — each re-checked 2026-10-16 with a dated note, and
each is homed on exactly one carried BL row in §2 (the check-backlog verdict-consistency rule
enforces the 1:1 homing). No PARTIAL is un-homed; none owes an in-build leg.

### 4 · Pre-registered thresholds — each with its evidence

| Threshold | Evidence (re-run/re-read at this gate, 2026-10-16, chain tip `ca04c04`) | Reading |
|---|---|---|
| Zero `OPEN` DEFERRALS rows an R3 ticket was chartered to land and did not | §1 chartered-vs-landed — every chartered row is `DONE` with dated evidence or carried on a named BL row with a reason code; §5 enumerates the 19 owed rows and their dated residual notes | Satisfied on the evidence; the carried rows await the operator's §9 dispositions |
| The three flake families repaired or adjudicated; G4-4/G4-5/G4-6 swept | R3.1 — all three root-caused and repaired deterministically, no retries (§1; `runs/R3.1.md`; LEDGER OPEN FINDINGS entries 2026-10-14). G4-4 swept: 16 stale leads at R3.1 + 5 at R3.10 (incl. the inline-`|` miss). G4-5 swept: DF-S2.4-2 carries dated notes 2026-10-14 + 2026-10-16. G4-6 swept: `BACKLOG.md` reads post-sweep truth (81 discharged / 19 owed) | **Pass** (a fourth same-family member fired at R3.3 and was repaired in-ticket — flag G5-7) |
| `check-build-memory.sh` 0 violations | `bash <skills>/build-memory/scripts/check-build-memory.sh .` re-run 2026-10-16T18:29Z → **0 violations, 7 warnings** — all aggregated pre-existing classes (manifest banner debt ×108 rows, spec-ADR appendix drift, `nextTicket`-vs-lowest-unlanded ordering, PHASE-LOG entries > 2 KiB ×108, BUILD_INDEX column-count ×2, pre-guard-sentence readouts ×2, pre-Harness-header run ledgers ×82); none introduced by this gate. Warning-count delta vs the operator's pre-check → flag G5-1 | **Pass** |
| `check-backlog.sh` exit 0 | `bash <skills>/reconcile-build/scripts/check-backlog.sh docs/build/BACKLOG.csv docs/build docs/tickets` re-run 2026-10-16T18:35Z → **exit 0**: 156 expected sources (145 ADR + 11 owed matrix); 55/66 engineering-closed, 53/66 requirement-satisfied; complete + non-duplicating + verdict-consistent | **Pass** |
| `^||` grep over DEFERRALS empty | `grep -n "^||" docs/tickets/DEFERRALS.md` re-run 2026-10-16T18:30Z → **0 matches** | **Pass** |
| R3.10 closure accounting exists; carried set enumerated with reason codes | `planning/2026-10-14_round3-decomposition.md` §"Round-3 accounting — appended at closure (R3.10)": landed-vs-carried per source row, coverage movement, backlog movement, new findings, the 21-row carried set + the three undischarged pending cells + the CF-493 format gap, all with per-row reason codes; mirrored in `BACKLOG.md`, `COVERAGE_MATRIX.csv`, `OPERATIONAL_READINESS.md`, `runs/R3.10.md` | **Pass** |
| The 10 GATE-ACCEPT accepted deviations unchanged; GATE-G4 §9 not re-litigated | §6 — re-confirmed as still-accepted: `CAPSTONE_CLOSURE.md` unmodified since `bcf1649` (2026-10-03), `readouts/GATE-ACCEPT.md` unchanged since `82ef6b0` (2026-10-06); `readouts/GATE-G4.md` §9 stands since `6e71d01` (2026-10-10); no SEND-BACK recorded anywhere | **Pass** (re-confirmed, not re-litigated) |

### 5 · DEFERRALS rule — the owed set and the dated-residual-note requirement

`DEFERRALS.md` at this gate: **100 rows — 81 discharged with dated evidence, 19 owed (18 `OPEN` +
1 `PARTIAL`)**, matching the R3.10 sweep exactly (28 owed at GATE-G4 minus the nine rows the round-3
discharges closed: DF-S1.9-2, DF-S1.13-1, DF-S1.13-4, DF-S1.15-3, DF-S1.17-1, DF-S1.19-1,
DF-S1.19-2, DF-S1.20-1, DF-S2.3-1).

Every still-owed row carries a dated residual note — **none is silent** (the G4-5 bare-`OPEN` flag is
closed: DF-S2.4-2 gained dated notes at R3.1 and R3.10):

| Row | Latest dated residual | BL home |
|---|---|---|
| DF-S0.3-2 (V) | 2026-10-14 — E2 legs foreign-run-verified (PR #136); E3/C12 cells env-pending HUMAN-H3 | BL-01 |
| DF-S0.3-3 (V, PARTIAL) | 2026-10-09 — machine cells DONE; the R2 cross-camp signature stays owed (HUMAN-H4) | BL-02 |
| DF-S1.2-2 (V) | 2026-10-14 — E2 verified; E3 re-run env-pending | BL-01 |
| DF-S1.5-3 (V) | 2026-10-14 — E2 verified; foreign replay env-pending | BL-01 |
| DF-S1.8-1 (V) | 2026-10-14 — E2 verified; foreign replay env-pending | BL-01 |
| DF-S1.15-1 (F) | 2026-10-08 — custody/`subagent_anchor`/`producer_resolution` legs still owed | BL-05/BL-30 |
| DF-S1.18-1 (F) | 2026-10-07 — machine cells landed; live transports env-gated | BL-31 |
| DF-S1.21-2 (V) | 2026-10-08 — reference-corpus arms + `RedundantJudge`/`IndependenceVector` owed | BL-05 |
| DF-S1.21-3 (F) | 2026-10-08 — `RecordedJudge` landed; live `CriticDeclaration{kind=judge}` env-gated | BL-17 |
| DF-S1.24-2 (V) | 2026-10-09 — live foreign-manifest import env-pending HUMAN-H3 | BL-26 |
| DF-S1.27-1 (V) | 2026-10-14 — E2 verified; E3 env-pending | BL-01 |
| DF-S2.4-2 (F) | 2026-10-16 (R3.10) — re-confirmed still owed, env-gated on the TLS-terminating transport | BL-31 |
| DF-S2.5-1 (F) | 2026-10-06 — custody ruling confirmed external (OQ-170) | BL-30 |
| DF-S4.2-1 (F) | 2026-09-26 — remote fetch transports dep-gated on WS-L6 trust fields | BL-32 |
| DF-S4.2-2 (F) | 2026-09-26 — foreign vocabularies dep-gated | BL-32 |
| DF-S4.10-1 (P) | 2026-10-02 — `gate pending`; HUMAN-H1 unprovisioned (provisionable per the operator's 2026-10-14 statement) | BL-03 |
| DF-S4.11-2 (F) | 2026-10-09 — WS-H3 mediator seam landed; oauth/mtls legs env-gated | BL-27 |
| DF-S4.13-1 (F) | 2026-10-06 — OQ-388 ratification owed | BL-30 |
| DF-S5.6-1 (P) | 2026-10-02 — HUMAN-H2 unprovisioned; fixture adapter is the honest proxy | BL-04 |

**Rule-4 scoping read (same readings as G3/G4, presented for the operator):** under the operative
G1/G2/G3/G4 carry-forward precedent, rows scoped to work that has run are discharged or carried; rows
whose only discharge path is an environment, a person, or a spec ruling cannot produce evidence
in-build and are presented as the §2 carried set. Under a strictly literal reading every one of the
19 owed rows is phase-scoped and therefore gate-blocking until dispositioned — which is exactly what
the §9 tables ask the operator to do. No row was closed, amended, or hidden by this readout.

### 6 · The 10 GATE-ACCEPT accepted deviations — re-confirmed, not re-litigated

`readouts/GATE-ACCEPT.md` §4 records all ten rows `ACCEPTED` (blanket acceptance, operator
"I accept", 2026-10-02; `LEDGER.md` GATE DECISIONS; `Status: SIGNED`). The deviation list itself
(`CAPSTONE_CLOSURE.md` §ACCEPTED-deviations) is unmodified since the ratified set landed (`bcf1649`,
2026-10-03); the readout is unmodified since `82ef6b0` (2026-10-06 — the `Status:` convention fix);
no SEND-BACK exists anywhere in the record. Re-confirmed unchanged:

1. R-2.7.2 split (ADR-0146) · 2. R-2.12.3 ecosystem decision · 3. R-2.12.4 spec-level deferral ·
4. R-2.12.5 naming thesis · 5. R-2.11.2 E3 binding (HUMAN-H1 → BL-03) · 6. R-2.12.6 real tracker
(HUMAN-H2 → BL-04) · 7. R2 cross-camp signature (→ BL-02) · 8. trigger-5 satisfied-by-analysis ·
9. foreign-toolchain cells (→ BL-01) · 10. GATE-G2 recorded set (→ BL-05).

Round-3 notes, for accuracy not re-litigation: **item 9's member rows gained evidence class** — the
five foreign-verification DEFERRALS rows (DF-S0.3-2, DF-S1.2-2, DF-S1.5-3, DF-S1.8-1, DF-S1.27-1)
now carry dated `foreign-run-verified` E2 evidence (PR #136, 2026-10-14) — a partial discharge the
bundle vocabulary cannot yet spell (CF-493); the rows stay OPEN on their E3/C12 legs exactly as
accepted. **Item 10's member set shrank further** — DF-S1.17-1 discharged at R3.5 (as DF-S1.13-3 did
at R2.10 and DF-S1.14-4/DF-S1.22-1/DF-S1.24-1 at R2.14/R2.16/R2.17); the arms still owed (live
corpus, OOP conformance, custody-riding halves) remain on BL-05 exactly as accepted. Nothing in the
record re-opens a signed row. The GATE-G4 §9 dispositions (28 rows: CARRY per reason code /
ACCEPT for accepted·ruling·standing) stand as recorded at `6e71d01` — the seven rows the round
discharged left the set; the rest carry their G4 dispositions into §9 unchanged.

### 7 · The named set — HUMAN-H5's record, the R3.7/R3.8/R3.9 residuals, the human/env rows

| Member | Recorded state | Home |
|---|---|---|
| HUMAN-H5's record (manifest row 143) | **Discharged** — RULED 2026-10-14, disposition (b): a vetted external crypto crate admitted (`ring = 0.17.14`; ≥7-day-old pin; closed scope); the dep-choice ADR landed inside R3.7 (ADR-0359, consumed by R3.8 under ADR-0361). LEDGER GATE DECISIONS holds the verbatim ruling | — (record closed; nothing carried) |
| R3.7's residual | The TLS-terminating transport leg — the sole owed leg under R-2.8.3/R-2.8.4's `MET-ENGINEERED(DF-S2.4-2)` verdicts; env-gated; also the loaded-host helper-deadline env-override seam (`HH_HELPER_{SPAWN,CONNECT}_DEADLINE_MS`) — a recorded run-ledger/PHASE-LOG repair, defaults unchanged | BL-31 (`env`); host note → flag G5-9 |
| R3.8's residual | R-2.8.6 stays PARTIAL honestly — signer-custody legs re-homed to the ruling row (ADR-0345 trigger (b) stands; trigger (a) fired-answered → BL-43) | BL-30 (`ruling`) + BL-05 (`accepted`) |
| R3.9's residual | The OQ-441/WS-I6 Stage-5 `DebtIndex` re-pin marker and BL-30/BL-37's other named-workstream rulings — CF-489's conform closed the recorded divergence, it did not answer the question | BL-30 (`ruling`) + BL-37 (`window`) |
| HUMAN-H1 (manifest row 61) | `OPEN — gate pending` (DF-S4.10-1); provisionable per the operator's 2026-10-14 statement, unprovisioned; proxy landed, `provided: no` recorded | BL-03 (`human`) |
| HUMAN-H2 (manifest row 82) | `OPEN — recorded` (DF-S5.6-1); unprovisioned; the fixture adapter is the declared honest proxy | BL-04 (`human`) |
| HUMAN-H3 (manifest row 127) | The foreign-verification environment — E3 replay, HIR/ledger-corpus extension, two-toolchain C12 cells, live manifest import; provisionable per the operator's 2026-10-14 statement; H3 material was provided for the E2 legs only (the PR #136 run record — `provided: yes`, no credential in any ledger) | BL-01/BL-26 (`env`) |
| HUMAN-H4 (manifest row 128) | The R2 cross-camp reviewer signature (DF-S0.3-3, `PARTIAL`); unsigned; machine cells DONE | BL-02 (`human`) |
| The foreign-verification cells | DF-S0.3-2/-S1.2-2/-S1.5-3/-S1.8-1/-S1.27-1 + DF-S1.24-2 — all carry dated notes; the five item-9 rows gained `foreign-run-verified` E2 evidence (PR #136: `hh-xcheck verify` 49/49 over six arms — identity-trees 22/22, identity-records 4/4, ledger-transcript 3/3, registry-snapshot 1/1, plugin-manifests 14/14, canonical-parse 6/6); `environment-pending` on the E3/corpus/C12 legs | BL-01/BL-26 (`env` + `accepted`) |

Every member is a named, dated, recorded state — none silently dropped. `gate pending` is a recorded
state, never a fabricated pass.

### 8 · Honest flags the operator should see

- **G5-1 · The validator warning-count delta — the operator's pre-check reported 51; this readout's
  own re-runs reproduce 7.** R3.10's PHASE LOG records "7 pre-existing warnings" (consistent with
  every round-3 run ledger); the operator's pre-check at ~18:2xZ reported "no violations (51
  warnings)"; this orchestrator's two independent runs on the chain tip `ca04c04` (18:29Z and again
  post-evidence-read) both report **0 violations, 7 warnings** — the same seven aggregated classes
  R3.10 named (banner debt ×108, spec-ADR appendix drift, `nextTicket` ordering, PHASE-LOG >2 KiB
  ×108, index column-count ×2, pre-guard readouts ×2, pre-Harness-header run ledgers ×82). **Not
  reproduced under any tested invocation:** the same script (only one copy exists —
  `~/.claude/skills` symlinks `~/agent-skills`; `check-build-memory.sh` unchanged since 2026-10-14,
  `e981ec5`) on the build worktree (7), on the main checkout @ `85a3960` (2), and on two sibling
  repos (21, 34); the older vendored copy at `~/Eleutheria/scripts/docs/` (4 violations/6 warnings —
  a stricter vintage); history mode `--range` (emits *violations*, not warnings — see G5-5).
  Remaining explanations, in order of plausibility: the pre-check ran against a transiently
  different tree (the crashed session's partial artifacts — verified absent at hand-off), a
  different directory, or a miscount. **The mechanical predicate — 0 violations, exit 0 — holds on
  the verified tree either way; the count discrepancy is flagged, not hidden.** If the operator's 51
  came from a specific invocation, naming it would let the next reconcile reproduce and reconcile it.
- **G5-2 · CF-493 (new, → BL-30).** The `hh-xcheck-bundle/1` status vocabulary cannot express a
  partially-foreign-discharged cell — `arms[].status` spells only `e1-self-checked`,
  `pending_cells[].status` only `environment-pending`, so the landed E2 evidence (PR #136) has no
  honest in-bundle spelling. Interim rule recorded: the dated DEFERRALS notes + the register carry
  the evidence; a per-candidate discharge member is owed before the next bundle mint.
- **G5-3 · CF-492 (→ BL-30).** Spec §3.3.8's closed diagnostic `stage` set has no `seal` spelling
  for `C-SEAL-1`'s producer; interim rule recorded at R3.6, owner ruling owed.
- **G5-4 · The seven open conflicts.** CF-476 (§10.2 gate-7 scope → WS-I5/WS-J4), CF-487
  (`refused_by_kernel` interim → OQ-470), CF-488 (fixer-op group placement → OQ-468), CF-490
  (register header misidentification — cosmetic), CF-491 ((d)-clause staged assumptions →
  BL-35/BL-37 ratifications), CF-492, CF-493 — all homed on BL-30 (`ruling`), every one a named
  workstream's call, none this build's to close. CF-489 resolved at R3.9 (code conformed to spec).
- **G5-5 · LEDGER-OF-6 → BL-41 — and the append-only/history-mode consequence.** Status-marker
  drift recurred: 16 stale leads swept at R3.1, five at R3.10 (incl. the inline-`|` miss on
  DF-S1.17-1), plus the machine-readable `pending_cells[]` side (CF-493). The dated-marker-governed
  sweep is the sanctioned repair — but `check-build-memory.sh . --range
  svitali/harnessharness-r3.9..HEAD` (history mode, run at this gate) flags **6 append-only hits** on DEFERRALS lines: the five
  deliberate lead-token rewrites plus DF-S2.4-2's in-line dated residual note (a markdown table row
  is one line, so an in-cell append reads as a line rewrite to the by-line check). This is the known
  LEDGER-OF-6 class in its second costume — the repair and the by-line append-only check are in
  tension; flagged so the operator sees it rather than discovering it at the next history audit.
  (Tooling note: `check-history.sh` emitted `grep: out of memory` twice over the
  2-commit `r3.9..HEAD` range and five times over the ~40-commit round range — cosmetic,
  the judgements still printed.)
- **G5-6 · `foreign-run-verified` is a distinct evidence class — the readiness ceiling stays
  `fixture-verified`.** PR #136's e2-python replay (49/49 over six arms, 2026-10-14, HUMAN-H3
  material provided as a run record only) is a foreign toolchain's recorded run — **not** an in-tree
  verification, not a live-service claim, and it does not lift the ceiling. `OPERATIONAL_READINESS.md`
  §"What is not claimed": no `staging-verified`, `live-executed`, or `public` row exists; no live
  provider call, TLS termination, judge, tracker, OAuth/mTLS exchange, E3 binding, or manifest import
  is claimed; no credential entered a ledger (`provided:` cells stay yes/no).
- **G5-7 · The ac5 parity allowlist gained a third member in-round.** Chain: `assembly_ms` (#104 →
  repaired R2.1) → `derived_from.view_hash` (#133 → repaired R3.1) → `plan_id` (R3.3, run
  `38703040017` on `dd4c92d` — repaired same-ticket under the sibling `ends_with` convention,
  25/25). Each is a downstream `idp/1` stamp of the measured-wall prefix — the family is a class,
  not three accidents: a fourth derived member would fire identically. The adjudications named the
  derivation input each time; flagged as a recurring-class candidate alongside LEDGER-OF-6.
- **G5-8 · R3.3 landed ~1,410 changed lines vs its 1,200 budget.** Recorded in ADR-0355 rather than
  split post-hoc (a split would orphan the acceptance batteries); flagged in `runs/R3.3.md` for
  operator adjudication — surfaced here so the gate sees it.
- **G5-9 · Host `syspolicyd` first-exec stall + the helper-deadline overrides.** The loaded host
  wedged `syspolicyd` (~186 CPU-hours; each freshly-linked test binary's first `exec` blocked ~4 min
  at 0 CPU; one hh-embed doc-test binary failed to *launch* at R3.7 — rerun clean, not a code
  regression). `HH_HELPER_SPAWN_DEADLINE_MS`/`HH_HELPER_CONNECT_DEADLINE_MS` env overrides cover
  `check-removability.sh` on loaded hosts; contract defaults unchanged. Recorded in `runs/R3.7.md`/
  `R3.8.md`/`R3.9.md`/`R3.10.md` + the R3.7 PHASE LOG entry — an environment condition, not a build
  defect; CI runs (GitHub-hosted) are unaffected and green.

### 9 · Per-row disposition — FOR THE OPERATOR

*Each row: `CARRY` to Round 4 · `ACCEPT` as standing deviation · `WAIVE` by ADR. All cells
intentionally blank — the operator fills them. The named-set rows (§7) are BL-02/BL-03/BL-04 and the
BL-01/BL-26 cells below; HUMAN-H5's record is discharged and needs no disposition.*

**A · The 21 carried backlog rows** *(verbatim from §2)*:

| BL | Carried work | Reason | Disposition (CARRY \| ACCEPT \| WAIVE) | What would accept it (if WAIVE/SEND-BACK) |
|---|---|---|---|---|
| BL-01 | E3 replay, HIR/ledger-corpus extension, two-toolchain C12 cells (E2 legs foreign-run-verified) | `env` + `accepted` | | env/human/later-round provisioning or the next workstream |
| BL-02 | R2 cross-camp human reviewer signature (DF-S0.3-3 residual; HUMAN-H4) | `human` | | env/human/later-round provisioning or the next workstream |
| BL-03 | HUMAN-H1 — E3 surface-ecosystem binding (DF-S4.10-1) | `human` | | env/human/later-round provisioning or the next workstream |
| BL-04 | HUMAN-H2 — real tracker + signed webhook (DF-S5.6-1) | `human` | | env/human/later-round provisioning or the next workstream |
| BL-05 | Gate-accepted battery arms (item 10) | `accepted` | | standing/accepted as recorded |
| BL-17 | Judge binding + live-judge admissibility (DF-S1.21-3) | `env` | | env/human/later-round provisioning or the next workstream |
| BL-26 | Live foreign-manifest import (DF-S1.24-2; HUMAN-H3) | `env` | | env/human/later-round provisioning or the next workstream |
| BL-27 | oauth/mtls CallerCredential mediator legs (DF-S4.11-2; WS-H3) | `env` | | env/human/later-round provisioning or the next workstream |
| BL-30 | Spec/governance rulings (OQ-170, OQ-388, seven open CFs, verdict residuals, OQ-441 marker) | `ruling` | | standing/accepted as recorded |
| BL-31 | Live provider transports + TLS-terminating transport (DF-S2.4-2, DF-S1.18-1) | `env` | | env/human/later-round provisioning or the next workstream |
| BL-32 | Remote fetch transports + foreign vocabularies (DF-S4.2-1/-2; WS-L6-gated) | `dep` + `env` | | env/human/later-round provisioning or the next workstream |
| BL-33 | WS-L6 package (R-2.12.4 + 13 OQs) | `window` | | env/human/later-round provisioning or the next workstream |
| BL-34 | ADR-0211 ratified deferrals D-1..D-6 + OQ-461/462/463 | `window` | | env/human/later-round provisioning or the next workstream |
| BL-35 | ADR-0212 register package — remaining open OQs | `window` | | env/human/later-round provisioning or the next workstream |
| BL-36 | ADR-0213 register package — remaining open OQs | `window` | | env/human/later-round provisioning or the next workstream |
| BL-37 | ADR-0214 register package — remaining open OQs (incl. OQ-441 Stage-5 re-pin) | `window` | | env/human/later-round provisioning or the next workstream |
| BL-38 | ADR-0215 program candidates + OQ-440 | `window` | | env/human/later-round provisioning or the next workstream |
| BL-39 | ADR-0216 fixer deferrals | `window` | | env/human/later-round provisioning or the next workstream |
| BL-41 | Ledger OPEN-FINDINGS standing rules (incl. LEDGER-OF-6) | `standing` | | standing/accepted as recorded |
| BL-42 | 56 quiet/dormant ADR triggers | `standing` | | standing/accepted as recorded |
| BL-43 | 49 fired-and-answered ADR triggers | `standing` | | standing/accepted as recorded |

**B · The 19 still-owed DEFERRALS rows** *(each homed on a carried BL row — dispositioned explicitly
here so no scoped `OPEN` row is undispositioned, per DEFERRALS rule 4)*:

| DF row | BL home | Disposition (CARRY \| ACCEPT \| WAIVE) | What would accept it (if WAIVE/SEND-BACK) |
|---|---|---|---|
| DF-S0.3-2 | BL-01 | | env/human/later-round provisioning or the next workstream |
| DF-S0.3-3 (P) | BL-02 | | env/human/later-round provisioning or the next workstream |
| DF-S1.2-2 | BL-01 | | env/human/later-round provisioning or the next workstream |
| DF-S1.5-3 | BL-01 | | env/human/later-round provisioning or the next workstream |
| DF-S1.8-1 | BL-01 | | env/human/later-round provisioning or the next workstream |
| DF-S1.15-1 | BL-05/BL-30 | | env/human/later-round provisioning or the next workstream |
| DF-S1.18-1 | BL-31 | | env/human/later-round provisioning or the next workstream |
| DF-S1.21-2 | BL-05 | | env/human/later-round provisioning or the next workstream |
| DF-S1.21-3 | BL-17 | | env/human/later-round provisioning or the next workstream |
| DF-S1.24-2 | BL-26 | | env/human/later-round provisioning or the next workstream |
| DF-S1.27-1 | BL-01 | | env/human/later-round provisioning or the next workstream |
| DF-S2.4-2 | BL-31 | | env/human/later-round provisioning or the next workstream |
| DF-S2.5-1 | BL-30 | | standing/accepted as recorded |
| DF-S4.2-1 | BL-32 | | env/human/later-round provisioning or the next workstream |
| DF-S4.2-2 | BL-32 | | env/human/later-round provisioning or the next workstream |
| DF-S4.10-1 (P) | BL-03 | | env/human/later-round provisioning or the next workstream |
| DF-S4.11-2 | BL-27 | | env/human/later-round provisioning or the next workstream |
| DF-S4.13-1 | BL-30 | | standing/accepted as recorded |
| DF-S5.6-1 (P) | BL-04 | | env/human/later-round provisioning or the next workstream |

### Operator reminder — what this disposition releases

After the operator dispositions, the chain holds exactly one row: **R3.11** (`docs/tickets/
149_R3.11__round3-docs-refresh.md` — the round-3 docs refresh; repo + agent docs state the round's
outcome; the last manifest row, 149 of 149). The ledger's `nextTicket` advance is the operator's act
on disposition, not this readout's. `mergePolicy: OPERATOR` — the 10 stacked round-3 PRs
(#137–#146) plus the earlier stack remain the operator's to merge.

### Verdict — pending operator signature

- [ ] PASSED — every still-OPEN scoped row dispositioned
- [ ] SKIPPED-BY-OPERATOR
- [ ] NOT PASSABLE — what would pass it: ____________________

**Date:** ____________  *(the operator's reading date — an agent does not fill this)*

<!-- agent-drafted:end -->

## Signature

<!-- Appended only when the operator has answered; an agent never writes these lines on its own
     authority: the verbatim decision + receipt `date -u` + channel, the GATE DECISIONS row
     reference, the operator's confirmation of the agent-drafted text quoting sha256:<first 12>,
     and "Signed by: the operator. Recorded by <harness/model>, which does not sign." -->
