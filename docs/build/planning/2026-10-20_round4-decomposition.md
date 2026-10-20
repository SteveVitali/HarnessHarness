# Round 4 decomposition — 2026-10-20 · `decompose-spec mode=extend`

**Mode:** `decompose-spec mode=extend` over the R3.11 chain tip
(`svitali/harnessharness-r3.11`, head `1b66584`, clean, pushed).
**Seed set:** rows 150–167 (`R4.1`…`R4.17`, `GATE-G6`) appended to
`docs/tickets/00_MANIFEST.md` under `### Round 4`. Build `harnessharness`,
dispatch `subagent`, granularity `balanced`, committed ledger mode.
**Round-4 input set:** the GATE-G5 dispositions + the 2026-10-20 operator
rulings (LEDGER GATE DECISIONS: H1 "Confirm TypeScript", H2 "GitHub Issues +
tunnel", H3 "Toolchains + online corpus", H4 "I'll name a reviewer") + the
operator's decomposition charter reopening the `window` set.

Records-only pass: no ticket implementation, no branches or worktrees, no code
changes — only the committed planning artifacts (this record, the manifest
extension, the ledger seed, the ticket contracts, the dated marker notes).

## Inputs read

- `docs/build/LEDGER.md` — IN_PROGRESS, `lastCompleted: R3.11`, the four
  2026-10-20 provisioning rulings (verbatim words recorded).
- `docs/build/readouts/GATE-G5.md` + `GATE-ACCEPT.md` — the 19-row owed set
  (18 OPEN + 1 PARTIAL), the named-set rule, the ten accepted deviations
  (re-confirmed, never re-litigated).
- `docs/tickets/DEFERRALS.md` — every dated marker on all 19 owed rows.
- `docs/build/BACKLOG.csv`/`BACKLOG.md` — BL-01…BL-46 source rows; the exact
  OQ lists per package.
- `docs/build/COVERAGE_MATRIX.csv`, `OPERATIONAL_READINESS.md`, `BUILD_INDEX.md`,
  `CAPSTONE_CLOSURE.md`, `research/LEDGER.md` §Q, `spec/CANONICAL_SPEC.md`.
- Human tickets `061` (H1), `082` (H2), `127` (H3), `128` (H4) — the marker +
  checklist shapes.
- `docs/build/planning/2026-10-14_round3-decomposition.md` + commits `89584c8`
  (round-3 seed convention) and `75d61ab` (round-3 same-day amend convention).
- `crates/hh-xcheck/` (CHECKER.md, bundle, self_check/verify), `foreign/e2-python`
  on main @ `ccd5fcd` (the E2 evidence: 49/49 over six arms; PR #136).
- `docs/build/reports/digests/` (operator digest generated 2026-10-20T14:25Z) +
  `ci-boundary.sh --stack --no-wait` read (exit 4 — see Phase 0).
- Skills: `decompose-spec/SKILL.md` (mode=extend), `build-memory/SKILL.md` +
  `layout.md` + templates + `scripts/check-build-memory.sh`.

## Phase 0 — state read

Chain tip `1b66584` on `svitali/harnessharness-r3.11`. The 149-row chain is
complete; the ledger parks the carried set for this decomposition
(`nextTicket: DONE`, `round: 3` → now 4). The four rulings of 2026-10-20 are
recorded in GATE DECISIONS with verbatim answers.

**CI boundary (observed 2026-10-20 ~14:24Z, `--no-wait`):** exit 4 — PR #147
(R3.11 docs refresh) pending (`build · test · fmt · drift` pending after 0s);
PR #137 pending; PRs #138–#146 pass. Recorded as observed pending, no wait.

**Chain↔main divergence (planning-relevant):** PR #136 merged to main carrying
`foreign/e2-python` + the corrected `CHECKER.md` + a regenerated `bundle.json`
(arm bytes identical; only the CHECKER.md file-hash changed). The chain tip
predates it — the chain stack merges over it. Recorded as an operator merge
note: `bundle.json` is generated (resolves by regeneration); R4.5 imports the
e2 tree verbatim so the chain's copy is byte-identical (no textual conflict).

## Authorized scope (the charter's own words)

Buildable now:

- **BL-33–BL-39 `window` packages** — the operator's charter reopens the
  WS-L6/spec-register window: BL-33 (ADR-0210: R-2.12.4 + 13 OQs), BL-34
  (ADR-0211: D-1..D-6 + OQ-461/462/463), BL-35 (ADR-0212: 12 OQs), BL-36
  (ADR-0213: 23 OQs), BL-37 (ADR-0214: 34 OQs + OQ-441 re-pin), BL-38
  (ADR-0215: 15 OQs incl. OQ-440 + HIR/2 backlog), BL-39 (ADR-0216:
  OQ-465/467/468 + CF-488). Spec/OQ closure engineering — the round's core.
- **H1 discharge** — TypeScript E3 surface binding + generated client +
  drift coverage (BL-03/DF-S4.10-1).
- **H2 legs** — GitHub Issues adapter + signed-webhook/tunnel ingress +
  capability-probe legs (BL-04/DF-S5.6-1); operator steps stay a checklist on
  row 082.
- **H3 toolchain legs** — the E3 foreign implementation over
  `hh-xcheck-bundle/1`, the two-toolchain C12 measurement + clean-CI leg,
  far-side canonical-parse timing, the HIR/ledger-corpus extension arms
  (BL-01 legs: DF-S0.3-2, DF-S1.2-2, DF-S1.5-3, DF-S1.8-1, DF-S1.27-1).
- **BL-30 adjudication prep** — per-CF evidence/briefs (CF-476/487/488/490/
  491/492/493) + the OQ-170/OQ-388 cells + LEDGER-OF-6 (BL-41 discussion);
  rulings themselves are operator GATE DECISIONS.
- **Hygiene** — the ac5 derived-wall recurring class (G5-7), the `syspolicyd`
  helper-spawn deadline work (gotcha 10), the G5-1 warning-count discrepancy.

Still gated — skeleton/marker treatment, never fabricated:

- **H3 online legs** (LT-03 live-corpus arm, DF-S1.24-2 live foreign-manifest
  import): corpus access + verification budget authorized *in principle*;
  accounts/keys + actual spend remain operator provisioning → **skeleton R4.6**.
- **BL-32** (remote fetch transports + foreign vocabularies): dep-gated on the
  BL-33 trust-field rulings + env-gated on remote fetch → **skeleton R4.9**,
  ordered behind the BL-33 package (the dependency analysis says "behind" —
  the trust fields are R4.8's outputs; the vocab-lift half may flesh early if
  the rulings stay pending — recorded in the skeleton's fleshing note).
- **BL-02/DF-S0.3-3** — the carried human marker (H4, reviewer to be named) —
  marker carried forward; gates no code.
- **BL-17, BL-31, BL-27** — live provider/model spend, TLS-terminating
  transport, OAuth/mTLS mediator: **not** covered by the corpus authorization;
  environment-pending with dated notes, flagged for a future operator ruling
  (see Decomposition decisions).
- **BL-05/BL-41/BL-42/BL-43** — standing accepted; no tickets.

Non-goals (conscious exclusions): no ticket implementation; no re-litigating
the ten GATE-ACCEPT deviations or the GATE-G4/G5 dispositions; no live-service,
live-provider or online-corpus claims; no secrets in any ledger; no merging
(`mergePolicy: OPERATOR`); the operator, not the planner, owns the final
`projectStatus: DONE` flip.

## Phase 1 — dependency / coupling map

```
R4.1 hygiene ──► R4.2 E3=TS bind (independent engineering)
R4.3 bundle-ext ──► R4.4 e3-ts impl (needs extended arms) ──► R4.5 C12 (needs e3 + e2 import)
R4.6 skeleton — adjacent to the toolchain cluster; gates on H3 provisioning
R4.7 H2 adapter (independent of the xcheck cluster)
R4.8 BL-33 (WS-L6 trust fields) ──► R4.9 skeleton (BL-32 gated on its rulings)
R4.10 BL-34 ─► R4.11 BL-35+39 ─► R4.12 BL-36 ─► R4.13 BL-37 ─► R4.14 BL-38
   (register packages: no true deps between them; chain order = review order)
R4.15 BL-30 briefs — after the packages so its briefs cite package outcomes
   (CF-488 ↔ R4.11; CF-493 ↔ R4.3; OQ-170 ↔ R4.12; OQ-388 ↔ R4.13)
R4.16 closure ──► GATE-G6 ──► R4.17 docs refresh + adopted-ruling conform
```

Shared-decision ownership (one owner each, earliest ticket):

- OQ-388's ratification leg → R4.15 (DF-S4.13-1 → BL-30); R4.13 records the
  package-side cell + pointer.
- OQ-170's custody ruling → R4.15 (DF-S2.5-1 → BL-30); R4.12 records the cell
  + pointer. Externally reserved per ADR-0331 D3 — brief, never self-ruled.
- CF-488's brief → R4.15; R4.11 records the BL-39-side register note.
- CF-493's vocabulary ruling → R4.15; R4.3/R4.5 use the current vocabulary and
  record friction, never invent members.
- Trust-field rulings → R4.8's brief (the R4.9 skeleton reads them).
- The `foreign/e2-python` verbatim import → R4.5 (R4.4 explicitly does not
  import it).

## Phase 2 — partition (18 rows)

| # | id | kind | scope (one line) | DF/BL coverage |
|---|---|---|---|---|
| 150 | R4.1 | ticket | round-4 hygiene: ac5 derived-wall class, helper-spawn deadline machinery, G5-1 warning count | G5-7, G5-1, gotcha-10; BL-41 class |
| 151 | R4.2 | ticket | E3=TypeScript bind: ADR-0050 amendment row + codegen TS surface client + check-drift coverage | DF-S4.10-1; BL-03; H1 |
| 152 | R4.3 | ticket | `hh-xcheck-bundle/1` extension arms (HIR/ledger corpus) + checker + regenerated bundle | DF-S1.2-2 leg; BL-01 |
| 153 | R4.4 | ticket | `foreign/e3-typescript` impl over all arms + answers.json + verify | DF-S1.2-2/-S1.5-3/-S1.8-1/-S1.27-1 E3 legs; BL-01 |
| 154 | R4.5 | ticket | C12 measurement + two-toolchain CI: verbatim e2 import, timing, ADR-0050 trigger-5 row | DF-S0.3-2; BL-01 |
| 155 | R4.6 | skeleton | H3 online live legs (LT-03 arm + live foreign-manifest import) | DF-S1.24-2; BL-26 + BL-05 cell |
| 156 | R4.7 | ticket | GitHub Issues adapter + signed-webhook/tunnel ingress + probes | DF-S5.6-1; BL-04; H2 |
| 157 | R4.8 | ticket | BL-33 package — ADR-0210 (R-2.12.4 + 13 OQs) | BL-33 |
| 158 | R4.9 | skeleton | BL-32 remote fetch + foreign vocabularies | DF-S4.2-1/-2; BL-32 |
| 159 | R4.10 | ticket | BL-34 package — ADR-0211 (D-1..D-6 + OQ-461/462/463) | BL-34 |
| 160 | R4.11 | ticket | BL-35+BL-39 packages — ADR-0212 + ADR-0216 | BL-35, BL-39; CF-488 note |
| 161 | R4.12 | ticket | BL-36 package — ADR-0213 (23 OQs) | BL-36 |
| 162 | R4.13 | ticket | BL-37 package — ADR-0214 (34 OQs + OQ-441 re-pin) | BL-37 |
| 163 | R4.14 | ticket | BL-38 package — ADR-0215 (15 OQs + OQ-440) | BL-38 |
| 164 | R4.15 | ticket | BL-30 adjudication prep — 7 CF briefs + OQ-170/OQ-388 + LEDGER-OF-6 + CF-490 fix | DF-S2.5-1, DF-S4.13-1; BL-30, BL-41 |
| 165 | R4.16 | reconcile | round-4 closure — backlog, coverage, readiness, carried-set accounting | all landed/carried |
| 166 | GATE-G6 | gate | round-4 discharge review + proposed-rulings dispositions | STOP |
| 167 | R4.17 | docs | round-4 docs refresh + adopted-ruling conform | post-gate records |

## Phase 3 — rules applied

- **Skeletons carry no run line** (BM-TICKET-03); gates are RETURN-PASS-class
  pauses, never `blockedOn`.
- **Per-OQ engineering rule** (the packages' shared contract): every OQ gets a
  dated engineering cell; a cell is *evidence-complete* (landed — recording
  acts only) or *briefed* (proposed disposition for the operator's GATE
  DECISIONS). Nothing is ratified without a ruling; nothing is silently open.
- **Evidence classes honest:** foreign replay records `foreign-run-verified`;
  nothing lifts the in-tree fixture-verified ceiling; live legs record
  `gate pending`/environment-pending.
- **The e2 import is verbatim** (byte-identical, merge-clean); `bundle.json`
  regenerates.
- **Ruling venue:** proposed dispositions assemble in the round's brief files;
  the operator dispositions via GATE DECISIONS (at GATE-G6 or whenever they
  arrive); R4.17 conforms what was adopted. Un-dispositioned proposals carry
  forward honestly.

## Owed-DEFERRALS coverage — all 19 rows

| DF row | home | round-4 coverage (single owner named first) |
|---|---|---|
| DF-S0.3-2 | BL-01 | **R4.5** (C12 + two-toolchain CI — the row's owner); R4.4's parse-timing leg is a supporting leg only |
| DF-S0.3-3 | BL-02 | carried HUMAN marker (row 128, H4 pending reviewer) + dated residual note |
| DF-S1.2-2 | BL-01 | **R4.4** (the E3 + extended-corpus replay — the row's owner); R4.3's extension arms are the prerequisite leg |
| DF-S1.5-3 | BL-01 | **R4.4** (E3 ledger-transcript arm) |
| DF-S1.8-1 | BL-01 | **R4.4** (E3 registry arm) |
| DF-S1.15-1 | BL-05/BL-30 | standing accepted carry (custody legs gate-accepted) + dated residual note |
| DF-S1.18-1 | BL-31 | environment-pending (live provider transports) + dated residual note |
| DF-S1.21-2 | BL-05 | standing accepted carry + dated residual note |
| DF-S1.21-3 | BL-17 | environment-pending (live judge = provider/model spend; future ruling flagged) + dated residual note |
| DF-S1.24-2 | BL-26 | **skeleton R4.6** (live foreign-manifest import) |
| DF-S1.27-1 | BL-01 | **R4.4** (E3 plugin-manifest arm) |
| DF-S2.4-2 | BL-31 | environment-pending (TLS-terminating transport; future ruling flagged) + dated residual note |
| DF-S2.5-1 | BL-30 | **R4.15** (custody-ruling cell — external; brief only) |
| DF-S4.2-1 | BL-32 | **skeleton R4.9** |
| DF-S4.2-2 | BL-32 | **skeleton R4.9** |
| DF-S4.10-1 | BL-03 | **R4.2** (H1 discharge) |
| DF-S4.11-2 | BL-27 | environment-pending (OAuth/mTLS mediator — live provider/TLS; future ruling flagged) + dated residual note |
| DF-S4.13-1 | BL-30 | **R4.15** (OQ-388 ratification brief) |
| DF-S5.6-1 | BL-04 | **R4.7** (H2 legs; live leg env-pending) |

Every row lands on exactly one ticket/marker or carries a dated residual note —
none silently dropped.

## Phase 4 — adversarial review (fresh-context pass)

Reviewed the partition adversarially for fragmented decisions, context-ceiling
overflow, orphan seams, coverage holes, ordering cycles, and over-factoring.
Findings and resolutions:

1. **E2 tree absent from the chain** — an e3-only toolchain cluster would have
   silently dropped DF-S0.3-2's *E5b* (E1+E2) half. **Resolution:** R4.5
   deliverable 1 imports `foreign/e2-python` verbatim from main @ ccd5fcd
   (byte-identical → the chain merges clean; `bundle.json` is generated and
   regenerates). The C12 measurement covers both pairs.
2. **CHECKER.md divergence hazard** — extending the chain's older CHECKER.md
   risks colliding with main's corrected prose when the chain merges.
   **Resolution:** R4.3's contract edits are append-only arm sections; the
   corrected spellings are *not* re-patched on the chain; `bundle.json` is the
   generated surface and resolves by regeneration. Recorded in R4.3's Notes.
3. **Single-owner seam OQ-388/OQ-170/CF-488** — these sit in both a register
   package and BL-30. **Resolution:** the ruling legs are R4.15's (BL-30's row
   per the GATE-G5 mapping); the packages record cells + pointers. No
   double-accounting.
4. **Overflow risk — R4.13 (34 OQs)** — the largest row. **Resolution:** split
   seam pre-registered in the ticket's Notes (`<ID>a` narrowed-set /
   `<ID>b` open-set via Plan extensions); same escape recorded for R4.12.
5. **Over-factoring check** — seven BL-33–39 rows would each carry thin
   standalone context. **Resolution:** BL-35+BL-39 merged into R4.11 (the
   R2.22 pairing — same Stage-0–1/fix register context); the other packages
   stay per-ADR (each an independently reviewable register domain — the R2.22–
   R2.24 grain). Skeletons: exactly two (both truly gated) — not one per env
   row (BL-17/BL-27/BL-31 get dated residual notes instead of placeholder
   rows; scheduling them would pretend a ruling exists).
6. **No forward `Depends on`** — every row depends only on an earlier row;
   the skeletons' gates name external events (H3 provisioning, BL-33 rulings),
   not later rows.
7. **Environment honesty check** — the H3 ruling authorizes corpus access +
   verification budget *in principle*; the online legs could have been
   over-eagerly scheduled. **Resolution:** R4.6 stays a skeleton gated on the
   provisioning steps; the LEDGER carries a scoped `pre-authorization` row
   (expires: GATE-G6); BL-17/BL-27/BL-31 are explicitly *not* scheduled —
   live provider/model spend and TLS/OAuth providers need a future ruling
   (flagged for the operator).
8. **Coverage audit** — the 19-row table above; every owed row mapped
   exactly once. The standing `accepted` rows (BL-05/BL-41/BL-42/BL-43) are
   named-not-scheduled per their gate dispositions; BL-02's human marker
   carries forward.
9. **Single-owner sharpening (this pass)** — DF-S1.2-2 spans two tickets
   (extension arms + E3 replay): ownership is assigned to **R4.4** with R4.3
   recorded as the prerequisite leg, so the row's discharge accounting has
   exactly one home; the same rule names **R4.5** the DF-S0.3-2 owner with
   R4.4's parse-timing leg supporting. The subagent exploration pass found no
   unmapped/double-mapped row, no forward Depends-on, no missing gate names on
   skeletons, and no self-minted rulings; the mechanical checks found and fixed
   two record defects (the "14 implementable" count → 13; the skeleton gate
   cells conformed to the round-3 `—` convention).

## Decomposition decisions

- **Tail choice:** the round has >1 implementation ticket → the minimal
  extended-shape tail mirrors round 3 exactly: closure reconcile (R4.16) →
  discharge gate (GATE-G6) → docs refresh (R4.17), with R4.17 additionally
  carrying the adopted-ruling conform leg (round-4-specific: the packages'
  proposed rulings need a landing point after the gate dispositions them).
- **Window opened:** the operator's charter declares BL-33–39 buildable;
  recorded here + in the manifest banner (the `window` disposition's
  "operator reopens it" clause is exercised by this decomposition).
- **Packages produce briefs, not rulings:** package tickets land dated
  engineering cells + evidence-complete *recording* closures only; every
  genuine ruling is a proposed disposition for the operator (GATE DECISIONS /
  GATE-G6), conformed by R4.17. This is the conservative read of "closure
  engineering" for a one-operator program: the engineering that makes each
  ruling a one-line decision is landed; the decision is never minted by a
  ticket.
- **Environment-pending flags for a future ruling:** BL-17 (live judge
  binding + admissibility), BL-31 (live provider transports + TLS-terminating
  transport), BL-27 (OAuth/mTLS mediator legs) — real provider/model spend and
  provider/TLS endpoints are *not* covered by the H3 corpus authorization.
  Dated residual notes appended; flagged here for the operator's next ruling.
- **H4 carried:** DF-S0.3-3 stays OPEN/PARTIAL — the operator said "I'll name
  a reviewer"; row 128's marker carries forward (dated note appended).
- **Requirement-index additions:** `R-2.12.4` → R4.8 (the package now has an
  owner), `R-2.12.6` += R4.7, `R-2.11.4` += R4.2, `R-2.12.1` += R4.3/R4.4/R4.5.
- **Ledger state:** `round: 4`, `nextTicket: R4.1`, `lastCompleted: R3.11`,
  `chainTip` stays `svitali/harnessharness-r3.11` (round-4 rows fork from the
  r3.11 tip), `updatedAt` from `date -u`; `### Round 4` opened in GATE
  DECISIONS (7-column form per layout.md — the scoped `pre-authorization` row
  for the corpus/budget authorization) and in PHASE LOG (the seed entry).

## Accounting (to be filled by R4.16)

R4.16 appends the dated landed-vs-carried accounting here; GATE-G6's readout
reads it.
