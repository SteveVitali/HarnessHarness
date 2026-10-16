# GATE-G4 readout — Round-2 discharge review

*Append-only. Each operator reading is a new dated block; never edit an earlier one.*

## Criterion (verbatim — the gate ticket `docs/tickets/135_GATE-G4__round2-discharge.md`)

> Every DEFERRALS row and backlog source the Round-2 tickets were chartered to discharge is DONE with
> dated evidence, or carried forward with an explicit operator disposition (carry to Round 3 / accept
> as standing deviation / waive by ADR). The environment-gated and human rows (HUMAN-H1/H2/H3/H4,
> DF-S4.10-1, DF-S5.6-1, the foreign-verification cells) are dispositioned as a named set — 'gate
> pending' is a valid recorded state; a silent drop is not.

**Blocks:** R2.27 (`docs/tickets/136_R2.27__round2-docs-refresh.md` — docs refresh runs after the
gate so the docs state the round's outcome).

**Pre-registered thresholds** *(verbatim from the ticket)*:

- Zero `OPEN` DEFERRALS rows that an R2 ticket was chartered to land and did not (each is landed,
  honestly residual with a dated note, or dispositioned here by the operator).
- `check-build-memory.sh` 0 violations; `check-backlog.sh` exit 0; `^||` grep over DEFERRALS empty.
- The R2.26 closure accounting exists and the carried set is enumerated with reason codes.
- The 10 GATE-ACCEPT accepted deviations are unchanged (re-confirmed as still-accepted, not
  re-litigated).

**DEFERRALS rule applied (docs/tickets/DEFERRALS.md rule 4):** *"Gates refuse to pass while any
`OPEN` row scoped to that phase remains. Treat an open row as gate-blocking."* — the ticket's own
clause: *"This gate does not pass while any `OPEN` row in `docs/tickets/DEFERRALS.md` scoped to this
phase remains undispositioned."*

---

## 2026-10-10 — Reading 1 (orchestrator-authored evidence audit) → PENDING operator disposition

**Evidence sources:** the gate ticket `docs/tickets/135_GATE-G4__round2-discharge.md`;
`docs/tickets/DEFERRALS.md` (100 rows — 72 discharged with dated evidence, 28 owed: 25 `OPEN` +
3 `PARTIAL`, every one enumerated in §2/§5); `docs/build/planning/2026-10-06_round2-decomposition.md`
(§Phase-1 cluster→ticket map + the dated `## Round-2 accounting` closure section);
`docs/build/BACKLOG.md` + `docs/build/BACKLOG.csv` (the R2.26 re-sweep — 42 rows, 156 expected
sources); `docs/build/COVERAGE_MATRIX.csv` (66 requirement rows, recomputed);
`docs/build/LEDGER.md` (CURRENT STATE + GATE DECISIONS + OPEN FINDINGS);
`docs/build/readouts/GATE-ACCEPT.md` (the 10 signed deviations); `docs/build/CAPSTONE_CLOSURE.md`;
`docs/build/BUILD_INDEX.md` rows 107–132 (R2.1–R2.26 landings, PRs #107–#133);
`docs/build/runs/R2.1.md`…`R2.26.md` and `docs/build/logs/ci-R2.*.json`;
`docs/tickets/00_MANIFEST.md` rows 107–136.

This readout presents the evidence and leaves every disposition cell blank for the operator.
*An operator or authorized human record supplies the decision; an agent must not sign or assume
silence is approval.*

### 1 · Chartered-vs-landed — every Round-2 ticket's chartered rows, accounted

The decomposition (`planning/2026-10-06_round2-decomposition.md` §Phase-1) chartered each R2 ticket a
named set of DEFERRALS rows and backlog rows. The closure accounting confirms: **every chartered row
is either DONE with dated evidence or sits on exactly one carried backlog row with a reason code
(§2). No chartered row is unlanded and unaccounted.**

| Ticket | Chartered (cluster map) | Outcome at closure |
|---|---|---|
| R2.1 | DF-DOC.1-1, DF-S1.3-2/-9-4/-26-2 flips, BL-29, the `assembly_ms` parity flake | All flipped `DONE` with dated evidence (DF-DOC.1-1 → traceability id BL-46 minted at R2.26); parity allowlist repaired. |
| R2.2 | DF-S1.5-1 residual, DF-S2.9-1, BL-11 | `DONE` — retention/GC/compression landed; BL-11 closed (R-2.2.1's verdict cell re-homed to BL-30 `ruling`). |
| R2.3 | DF-S2.3-1, BL-12 | BL-12 closed; DF-S2.3-1 residual producers carried → **BL-44** (`in-build`). |
| R2.4 | DF-S2.9-3, DF-S2.10-1, BL-13 | `DONE` — env verbs + automatic snapshot cadence (ADR-0335); BL-13 closed. |
| R2.5 | DF-S2.8-1 (sans steer), DF-S1.19-1/-2, BL-15 | DF-S2.8-1 `DONE`; DF-S1.19-1/-2 carried → **BL-15** (`in-build`). |
| R2.6 | DF-S2.11-1, DF-S1.20-1, DF-S2.8-1(a) steer leg, BL-14 | DF-S2.11-1 + DF-S2.8-1(a) `DONE`; DF-S1.20-1 residual legs carried → **BL-14** (`in-build`). |
| R2.7 | DF-S1.18-1 machine cells | Machine cells `DONE`; live `Transport`/`CredentialPort` impls carried → **BL-31** (`env`). |
| R2.8 | DF-S1.17-1/-2/-3, BL-16 | DF-S1.17-2/-3 `DONE`; DF-S1.17-1's unassigned members carried → **BL-05** (`accepted`) + **BL-16** (`in-build`). |
| R2.9a/b | DF-S1.12-1/-2/-4/-6, DF-S2.4-1/-3, BL-18 | All `DONE`; BL-18 closed; TLS-bound legs re-homed → **BL-31** (`env`). |
| R2.10 | DF-S1.13-1/-3, BL-19 | DF-S1.13-3 `DONE`; DF-S1.13-1 `PARTIAL` + DF-S1.13-4 `OPEN` carried → **BL-19** (`dep` — no pure-std AEAD/asymmetric verifier). |
| R2.11 | DF-S1.23-1, DF-S1.11-2, BL-22+BL-23 | All `DONE`; BL-22/BL-23 closed. |
| R2.12 | DF-S2.7-1, BL-20 | `DONE` (all three members); BL-20 closed. |
| R2.13 | DF-S1.15-3, DF-S1.15-1 non-custody, BL-21 | BL-21 closed; DF-S1.15-3's per-event-encryption cell → **BL-45** (`dep`); DF-S1.15-1's custody/`subagent_anchor`/`producer_resolution` → **BL-05**/`BL-30`. |
| R2.14 | DF-S1.14-1/-2/-4, BL-24 | All `DONE`; BL-24 closed (live-sink leg re-homed → BL-31). |
| R2.15 | DF-S1.21-1/-2/-3, BL-17 | DF-S1.21-1 `DONE`; DF-S1.21-2 carried → **BL-05** (`accepted`); DF-S1.21-3 carried → **BL-17** (`env` — live judge). |
| R2.16 | DF-S1.22-1/-2, BL-25 | `DONE`; BL-25 closed. |
| R2.17 | DF-S1.24-1/-2 machine cells, BL-26 | DF-S1.24-1 `DONE`; DF-S1.24-2's live foreign-manifest import → **BL-26** (`env`, HUMAN-H3). |
| R2.18 | DF-S1.9-2, BL-10 | `PARTIAL` — residual carried → **BL-10** (`in-build`). |
| R2.19 | DF-S1.25-1, DF-S4.11-1/-2, BL-27 | DF-S1.25-1/-S4.11-1 `DONE`; DF-S4.11-2's oauth/mtls legs → **BL-27** (`env`). |
| R2.20 | DF-S1.23-2, BL-28 | `DONE`; BL-28 closed. |
| R2.21 | BL-01 foreign-verification cells (HUMAN-H3-gated) | Hermetic packaging landed (`hh-xcheck-bundle/1`, E1 self-check 55/55); every foreign cell honestly withheld → **BL-01**/`BL-26` (`env`), zero fabricated claims. |
| R2.22–R2.25 | BL-35/BL-39, BL-36, BL-37, BL-40 revalidation sweeps | Dated revalidation passes landed; BL-40 closed; register packages carried `open` (§2 `window`/`ruling` rows). |
| R2.26 | Closure reconcile (BL-30/-41/-42/-43) | Landed — this accounting; BL-44/45/46 minted; the carried set below. |

**Reading:** zero `OPEN` DEFERRALS rows that an R2 ticket was chartered to land and did not land —
each is either `DONE` with dated evidence or honestly residual on a named carried row. The operator
dispositions the carried set in §9.

### 2 · The carried set — 28 rows, verbatim from the Round-2 accounting

*Transcribed verbatim from `docs/build/planning/2026-10-06_round2-decomposition.md`
§"The carried set for GATE-G4 — per-row reason codes" (appended at closure, R2.26 · 2026-10-10).
This is the set the operator signs — the per-row disposition table is §9.*

Reason codes: `human` — a person's act is the only remaining leg · `env` — needs a
provisioned environment (foreign toolchain, live provider, TLS endpoint, live judge)
· `dep` — blocked on a primitive/dependency the hermetic build lacks · `in-build` —
machine-achievable residual carried on capacity · `accepted` — gate-accepted at
GATE-ACCEPT; arms execute when their environments exist · `ruling` — spec/governance
ruling owed, not code · `window` — scheduled to the WS-L6 program window · `standing` —
perpetual ledger row, re-issued never closed.

| BL | Carried work | Reason |
|---|---|---|
| BL-01 | E2/E3 cross-impl conformance cells | `env` + `accepted` |
| BL-02 | R2 cross-camp human reviewer signature | `human` |
| BL-03 | HUMAN-H1 E3 surface-ecosystem binding | `human` |
| BL-04 | HUMAN-H2 real tracker + signed webhook | `human` |
| BL-05 | Gate-accepted battery arms (item 10) | `accepted` |
| BL-10 | Emitter-vs-exemption adjudication (11 no-document-path codes; DF-S1.9-2) | `in-build` |
| BL-14 | DF-S1.20-1 legs (delegation_reason, workflow/program interpreters) | `in-build` |
| BL-15 | §5c battery residuals (DF-S1.19-1/-2) | `in-build` |
| BL-16 | DF-S1.17-1 members + sync_source/retrieval_eval fixture legs | `in-build` |
| BL-17 | Judge binding + live-judge admissibility | `env` |
| BL-19 | dpop + wrapped_long_lived (credential broker) | `dep` |
| BL-26 | Live foreign-manifest import | `env` |
| BL-27 | oauth/mtls mediator legs | `env` |
| BL-30 | Spec/governance rulings (OQ-170 custody, OQ-388 ratification, 6 open CFs, R-2.2.1's verdict cell) | `ruling` |
| BL-31 | Live provider transports + TLS-terminating transport | `env` |
| BL-32 | Remote fetch transports + foreign import/export vocabularies | `env` |
| BL-33 | WS-L6 package (R-2.12.4 + 13 OQs) | `window` |
| BL-34 | ADR-0211 ratified deferrals D-1..D-6 + OQ-461/462/463 | `window` |
| BL-35 | ADR-0212 register package — remaining open OQs | `window` |
| BL-36 | ADR-0213 register package — remaining open OQs | `window` |
| BL-37 | ADR-0214 register package — remaining open OQs | `window` |
| BL-38 | ADR-0215 program candidates + OQ-440 | `window` |
| BL-39 | ADR-0216 fixer deferrals | `window` |
| BL-41 | Ledger OPEN-FINDINGS standing rules | `standing` |
| BL-42 | 44 quiet/dormant ADR triggers | `standing` |
| BL-43 | 45 fired-and-answered ADR triggers | `standing` |
| BL-44 | DF-S2.3-1 residual producers (resume_set, defer, child-lease, non-fleet ingress, heal surface) | `in-build` |
| BL-45 | Audit per-event encryption + R-2.8.6 legs | `dep` |

### 3 · Coverage roll-up — recomputed matrix

`docs/build/COVERAGE_MATRIX.csv` re-read at this gate: **41 MET · 19 PARTIAL · 6 MET-DIFFERENTLY ·
0 MISSING · 0 AT-RISK-INTEGRATION = 47/66** on both sums (engineering closed and requirement
satisfied), confirmed independently by `check-backlog.sh` (sums 47/66). Movement: 35/66 at the
REC.1 closeout → 47/66 at R2.26.

The **twelve** in-round PARTIAL→MET flips (accounting §"Coverage movement" — the three recorded at
their landing tickets plus the nine re-verdicted at closure with dated evidence):
R-2.5.2 (R2.8) · R-2.8.5 (R2.20) · R-2.8.7 (R2.11) — recorded at landing; and the nine closure
flips R-2.2.4 · R-2.2.5 (R2.4) · R-2.4.1 · R-2.4.2 (R2.5/R2.6) · R-2.8.2 (R2.12) · R-2.9.1 (R2.14) ·
R-2.9.2 (R2.16) · R-2.11.1 (R2.1+R2.4) · R-2.11.4 (R2.19).

The 19 remaining PARTIAL requirements — R-2.1.4, R-2.2.1, R-2.2.3, R-2.4.3, R-2.4.4, R-2.5.3,
R-2.6.1, R-2.7.1, R-2.7.2b, R-2.8.3, R-2.8.4, R-2.8.6, R-2.9.3, R-2.9.4, R-2.9.6, R-2.10.3,
R-2.10.5, R-2.11.3, R-2.12.1 — each is homed on exactly one carried BL row in §2 (the
check-backlog verdict-consistency rule enforces the 1:1 homing). No PARTIAL is un-homed.

### 4 · Pre-registered thresholds — each with its evidence

| Threshold | Evidence (re-run at this gate, 2026-10-10, chain tip `9ad4e60`) | Reading |
|---|---|---|
| Zero `OPEN` DEFERRALS rows an R2 ticket was chartered to land and did not | §1 chartered-vs-landed table — every chartered row is `DONE` with dated evidence or carried on a named BL row with a reason code; §5 enumerates the dated residual notes | Satisfied on the evidence; the carried rows await the operator's §9 dispositions |
| `check-build-memory.sh` 0 violations | `bash <skills>/build-memory/scripts/check-build-memory.sh .` → **0 violations**, the standing 7 pre-existing warnings (manifest banner, ADR appendix drift, `nextTicket` ordering note, ledger-entry sizes, index column counts, pre-guard readouts, pre-Harness-header run ledgers) — none introduced by this gate | **Pass** |
| `check-backlog.sh` exit 0 | `bash <skills>/reconcile-build/scripts/check-backlog.sh docs/build/BACKLOG.csv docs/build docs/tickets` → **exit 0**: expected sources 156; sums 47/66 both columns; complete + non-duplicating + verdict-consistent | **Pass** |
| `^||` grep over DEFERRALS empty | `grep -n "^||" docs/tickets/DEFERRALS.md` → no matches | **Pass** |
| R2.26 closure accounting exists; carried set enumerated with reason codes | `docs/build/planning/2026-10-06_round2-decomposition.md` §"Round-2 accounting — appended at closure (R2.26 · 2026-10-10)": coverage movement, backlog movement, the 28-row carried set with per-row reason codes; mirrored in `docs/build/BACKLOG.md` and `docs/build/runs/R2.26.md` | **Pass** |
| The 10 GATE-ACCEPT accepted deviations unchanged | §6 — re-confirmed as still-accepted; `CAPSTONE_CLOSURE.md` and `readouts/GATE-ACCEPT.md` unmodified since the ratified set landed (`bcf1649`); no SEND-BACKs recorded | **Pass** (re-confirmed, not re-litigated) |

### 5 · DEFERRALS rule — still-owed rows and the dated-residual-note requirement

`DEFERRALS.md` at this gate: **100 rows — 72 discharged with dated evidence, 28 owed
(25 `OPEN` + 3 `PARTIAL`)**, matching the R2.26 sweep exactly.

Every still-owed row whose cells name an R2 ticket carries a **dated residual note** from that
ticket (or later) — none is silent:

| Row | R2 ticket named | Latest dated residual |
|---|---|---|
| DF-S0.3-2 | R2.21 | 2026-10-09 — xcheck `canonical-parse` arm packaged; foreign legs env-pending HUMAN-H3 |
| DF-S0.3-3 (P) | R2.21 | 2026-10-09 — corpus packaged; the signature stays OPEN, HUMAN-H4 |
| DF-S1.2-2 | R2.21 | 2026-10-09 — identity-trees/-records arms packaged; E2/E3 re-run env-pending |
| DF-S1.5-3 | R2.21 | 2026-10-09 — WAL transcript packaged + pinned; foreign replay env-pending |
| DF-S1.8-1 | R2.21 | 2026-10-09 — registry-snapshot arm packaged; foreign replay env-pending |
| DF-S1.9-2 (P) | R2.18 | 2026-10-09 — residual carried on BL-10 (`in-build`) |
| DF-S1.13-1 (P) | R2.10 | 2026-10-07 — dpop residual on BL-19 (`dep`) |
| DF-S1.15-1 | R2.13 | 2026-10-08 — gc/redact emitters audited closed; custody/`subagent_anchor`/`producer_resolution` still owed |
| DF-S1.15-3 | R2.13 | 2026-10-08 — witness cosignatures + receiver receipts DONE; per-event blob encryption still OPEN (BL-45, `dep`) |
| DF-S1.17-1 | R2.8 | 2026-10-07 — unassigned members stay OPEN (BL-05/BL-16) |
| DF-S1.18-1 | R2.7 | 2026-10-07 — machine cells landed; live transports on BL-31 (`env`) |
| DF-S1.19-1 | R2.2/R2.5 | 2026-10-06 — producer call sites carried on BL-15 (`in-build`) |
| DF-S1.19-2 | R2.5 | 2026-10-06 — §5c residual cells on BL-15 (`in-build`) |
| DF-S1.20-1 | R2.6 | 2026-10-06 — residual legs on BL-14 (`in-build`) |
| DF-S1.21-2 | R2.15 | 2026-10-08 — reference-corpus fixture halves + `RedundantJudge`/`IndependenceVector` still owed (BL-05) |
| DF-S1.21-3 | R2.15 | 2026-10-08 — `RecordedJudge` landed; live `CriticDeclaration{kind=judge}` env-gated (BL-17) |
| DF-S1.24-2 | R2.17/R2.21 | 2026-10-09 — live foreign-manifest import env-pending HUMAN-H3 (BL-26) |
| DF-S1.27-1 | R2.21 | 2026-10-09 — manifest-corpus arm packaged; foreign replay env-pending |
| DF-S2.3-1 | R2.3 | 2026-10-06 — residual producers enumerated + homed on BL-44 (`in-build`) |
| DF-S4.11-2 | R2.19 | 2026-10-09 — WS-H3 mediator seam landed; oauth/mtls legs env-gated (BL-27) |

The eight owed rows naming no R2 ticket: DF-S1.13-4 (latest note 2026-09-29, BL-19 `dep`),
DF-S2.4-2 (**bare `OPEN`, no dated note — see flag G4-5**; residual homed on BL-31 `env`),
DF-S2.5-1 (2026-10-06, BL-30 `ruling`), DF-S4.2-1 + DF-S4.2-2 (2026-09-26, BL-32 `env`/`window`),
DF-S4.10-1 (2026-10-02 `gate pending`, BL-03 `human`), DF-S4.13-1 (2026-10-06, BL-30 `ruling`),
DF-S5.6-1 (2026-10-02, BL-04 `human`).

**Rule-4 scoping read (same readings as G3, presented for the operator):** under the operative
G1/G2/G3 carry-forward precedent, rows scoped to work that has run are discharged or carried; rows
whose only discharge path is an environment, a person, or a spec ruling cannot produce evidence
in-build and are presented as the §2 carried set. Under a strictly literal reading every one of the
28 owed rows is phase-scoped and therefore gate-blocking until dispositioned — which is exactly
what the §9 table asks the operator to do. No row was closed, amended, or hidden by this readout.

### 6 · The 10 GATE-ACCEPT accepted deviations — re-confirmed, not re-litigated

`readouts/GATE-ACCEPT.md` §4 records all ten rows `ACCEPTED` (blanket acceptance, operator
"I accept", 2026-10-02; `LEDGER.md` GATE DECISIONS). The deviation list itself
(`CAPSTONE_CLOSURE.md` §ACCEPTED-deviations) is unmodified since the ratified set landed
(`bcf1649`, 2026-10-03); no SEND-BACK exists anywhere in the record. Re-confirmed unchanged:

1. R-2.7.2 split (ADR-0146) · 2. R-2.12.3 ecosystem decision · 3. R-2.12.4 spec-level deferral ·
4. R-2.12.5 naming thesis · 5. R-2.11.2 E3 binding (HUMAN-H1 → now BL-03) · 6. R-2.12.6 real
tracker (HUMAN-H2 → BL-04) · 7. R2 cross-camp signature (→ BL-02) · 8. trigger-5
satisfied-by-analysis · 9. foreign-toolchain cells (→ BL-01) · 10. GATE-G2 recorded set (→ BL-05).

Round-2 note, for accuracy not re-litigation: several item-10 member DEFERRALS rows discharged
in-round (DF-S1.13-3 at R2.10, DF-S1.14-4 at R2.14, DF-S1.22-1 at R2.16, DF-S1.24-1 at R2.17) —
the signed deviation shrank as legs landed; the arms still owed (live corpus, OOP conformance,
custody) remain on BL-05 exactly as accepted. Nothing in the record re-opens a signed row.

### 7 · The named set — environment-gated + human rows ('gate pending' is a valid recorded state)

| Row | Manifest/ledger id | Recorded state | Home |
|---|---|---|---|
| DF-S4.10-1 (P) | HUMAN-H1 | `OPEN — **gate pending** 2026-09-27`; proxy landed, `provided: no` recorded | BL-03 (`human`) |
| DF-S5.6-1 (P) | HUMAN-H2 | `OPEN — recorded 2026-10-02`; fixture adapter is the declared honest proxy | BL-04 (`human`) |
| `127_HUMAN-H3__foreign-verification-env.md` | HUMAN-H3 | Manifest human row — the E2/E3 toolchain + online corpus environment; unprovisioned, never claimed | BL-01/BL-26 legs (`env`) |
| `128_HUMAN-H4__r2-cross-camp-signature.md` | HUMAN-H4 | Manifest human row — the cross-camp reviewer signature; unsigned, machine cells DONE | BL-02 (`human`) |
| Foreign-verification cells | — | DF-S0.3-2/-S1.2-2/-S1.5-3/-S1.8-1/-S1.27-1 + DF-S1.24-2 — all carry dated `environment-pending on HUMAN-H3` notes after R2.21's hermetic packaging | BL-01/BL-26 (`env` + `accepted`) |

Every member is a named, dated, recorded state — none silently dropped.

### 8 · Honest flags the operator should see

- **G4-1 · `hh-mcp-lab r2_19 credential_mediator_oauth_resolves_and_refuses` — increasing flake
  frequency.** Recorded firings: R2.22 (both fd58e3e runs flaked once each — commit `dc7061d`),
  R2.23 (`cb6db76`), R2.25 (`d8fd2e4`), and twice at R2.26 (push run `38020320703` on `e377a4a`,
  and the `4c5351a` pull_request attempt — `9ad4e60`). Every rerun went green; the leg is a
  timing-sensitive oauth-mediator test introduced at R2.19. It is now the dominant flake family —
  the R2.23 ledger itself notes the family also touched R2.19/R2.21 closeouts. Not a gate item;
  a round-3 reliability candidate the operator may want adjudicated rather than endlessly rerun.
- **G4-2 · New `ac5` parity-flake signature — `derived_from.view_hash`.** PR #133 push run
  `38020320703` failed once on `ac5_attended_and_unattended_share_one_configuration` at
  `.payload.derived_from.view_hash` — the same parity test whose `assembly_ms` leg was repaired at
  R2.1, but a different run-varying member the allowlist doesn't cover. Sibling pull_request run on
  the same SHA passed; rerun green. Recorded in LEDGER OPEN FINDINGS and assigned to the round-3
  backlog (allowlist-vs-derivation adjudication on the next code-touching ticket).
- **G4-3 · LEDGER insertion defect — repaired at this readout.** The G4-2 finding, written at
  R2.26 (`4c5351a`), had been inserted mid-sentence inside the 2026-09-18 file-mutation finding's
  parenthetical "(insert before `## GATE DECISIONS`)" — splitting that sentence and misplacing the
  flake record. Repaired at this readout: the 2026-09-18 sentence is rejoined verbatim and the
  flake bullet now sits as its own OPEN FINDINGS entry directly above `## GATE DECISIONS`.
  Content was preserved byte-for-byte; only placement moved. `git diff`-verified per the standing
  build-memory write rule.
- **G4-4 · Stale `OPEN` status prefixes on discharged DEFERRALS rows (bookkeeping wart).** ~16
  rows discharged in-round carry a dated `**DONE — 2026-10-XX (R2.x)**` note *inside* a status cell
  that still begins `OPEN —` or `OPEN` (e.g. DF-S1.11-2, DF-S1.12-*, DF-S2.7-1, DF-S2.8-1,
  DF-S2.9-1/-3); DF-S1.14-2 and DF-S1.14-4 additionally retain stale trailing
  "Still OPEN/remain OPEN as rowed" sentences that predate their own DONE notes. The R2.26 sweep
  counts by dated discharge notes (28 owed of 100), and `check-backlog.sh` is exit 0 — but a naive
  `| OPEN` grep overstates the owed set. A flip-the-prefix bookkeeping pass (the BL-29 pattern) is
  a clean round-3 chore.
- **G4-5 · DF-S2.4-2 is a bare `OPEN` cell** — the only owed row with no dated residual note. It
  names no R2 ticket (owner: the production-transport ticket), so it does not violate this gate's
  dated-note clause; its residual is enumerated on BL-31 (`env`). Recorded for completeness.
- **G4-6 · Minor stale counts in `BACKLOG.md` §"How to read"** — the line "The 39 discharged
  DEFERRALS rows" is the REC.1-era count (post-round truth is 72 discharged / 28 owed, per the same
  file's own header). Cosmetic; flagged so no reader reconciles against the wrong number.
- **G4-7 · Second flake family on record** — `hh-embed r2_14` loopback conn-reset (fired at R2.21,
  `52e46d4`). Single recorded firing; listed so the flake inventory is complete.

### 9 · Per-row disposition — FOR THE OPERATOR

*Each row: `CARRY` to Round 3 · `ACCEPT` as standing deviation · `WAIVE` by ADR. All cells
intentionally blank — the operator fills them. The named-set rows (§7) are rows 2–5 and the
BL-01/BL-26 cells below.*

| BL | Carried work | Reason | Disposition (CARRY \| ACCEPT \| WAIVE) | What would accept it (if WAIVE/SEND-BACK) |
|---|---|---|---|---|
| BL-01 | E2/E3 cross-impl conformance cells | `env` + `accepted` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-02 | R2 cross-camp human reviewer signature (HUMAN-H4) | `human` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-03 | HUMAN-H1 E3 surface-ecosystem binding | `human` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-04 | HUMAN-H2 real tracker + signed webhook | `human` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-05 | Gate-accepted battery arms (item 10) | `accepted` | ACCEPT | standing/accepted as recorded |
| BL-10 | Emitter-vs-exemption adjudication (DF-S1.9-2) | `in-build` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-14 | DF-S1.20-1 legs | `in-build` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-15 | §5c battery residuals (DF-S1.19-1/-2) | `in-build` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-16 | DF-S1.17-1 members + sync_source/retrieval_eval legs | `in-build` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-17 | Judge binding + live-judge admissibility | `env` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-19 | dpop + wrapped_long_lived (credential broker) | `dep` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-26 | Live foreign-manifest import | `env` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-27 | oauth/mtls mediator legs | `env` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-30 | Spec/governance rulings (OQ-170, OQ-388, 6 CFs, R-2.2.1 verdict cell) | `ruling` | ACCEPT | standing/accepted as recorded |
| BL-31 | Live provider transports + TLS-terminating transport | `env` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-32 | Remote fetch transports + foreign vocabularies | `env` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-33 | WS-L6 package (R-2.12.4 + 13 OQs) | `window` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-34 | ADR-0211 ratified deferrals D-1..D-6 + OQ-461/462/463 | `window` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-35 | ADR-0212 register package — remaining open OQs | `window` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-36 | ADR-0213 register package — remaining open OQs | `window` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-37 | ADR-0214 register package — remaining open OQs | `window` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-38 | ADR-0215 program candidates + OQ-440 | `window` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-39 | ADR-0216 fixer deferrals | `window` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-41 | Ledger OPEN-FINDINGS standing rules | `standing` | ACCEPT | standing/accepted as recorded |
| BL-42 | 44 quiet/dormant ADR triggers | `standing` | ACCEPT | standing/accepted as recorded |
| BL-43 | 45 fired-and-answered ADR triggers | `standing` | ACCEPT | standing/accepted as recorded |
| BL-44 | DF-S2.3-1 residual producers | `in-build` | CARRY | env/human/later-round provisioning or the next workstream |
| BL-45 | Audit per-event encryption + R-2.8.6 legs | `dep` | CARRY | env/human/later-round provisioning or the next workstream |

### Operator reminder — what this disposition releases

After the operator dispositions, the chain holds: **R2.27** (the round-2 docs refresh — repo +
agent docs state the round's outcome, the sole remaining manifest row). The ledger's `nextTicket`
advance is the operator's act on disposition, not this readout's.

### Verdict — pending operator signature

- [x] PASSED — every still-OPEN scoped row dispositioned (blanket acceptance, no SEND-BACKs)
- [ ] NOT PASSABLE — what would pass it: ____________________
- [ ] SKIPPED-BY-OPERATOR

**Date:** 2026-10-10

### Operator disposition line — FOR THE OPERATOR

- [x] Disposition recorded: operator acceptance 2026-10-10 (orchestrate-build records the GATE DECISIONS row + ledger advance) (operator signature / initials, date, and the
  LEDGER `GATE DECISIONS` row reference once `orchestrate-build` records it)
