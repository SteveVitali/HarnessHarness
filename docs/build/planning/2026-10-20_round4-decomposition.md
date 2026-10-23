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

**2026-10-20 (R4.1) — G5-1 root cause, dated record.** The GATE-G5 flag: the operator's pre-check
reported "no violations (51 warnings)"; every subsequent invocation reports 0 violations / 7 warnings.
Reproduced under R4.1 on this tree (`bash ~/.claude/skills/build-memory/scripts/check-build-memory.sh .`,
exit 0): **0 violations, 7 warnings** — the same seven aggregated classes the gate named (manifest banner
debt ×108, ADR appendix drift, `nextTicket`-vs-lowest-unlanded ordering, PHASE-LOG >2 KiB ×109, BUILD_INDEX
column-count ×2, pre-guard readouts ×2, pre-Harness-header run ledgers ×82). The gate's own sweep already
covered the candidate invocations — the build worktree (7), the main checkout @ `85a3960` (2), two sibling
repos (21, 34), the older vendored copy at `~/Eleutheria` (4 violations/6 warnings, a stricter vintage),
history mode `--range` (emits violations, not warnings) — and none yields 51. The remaining explanations,
unchanged in order of plausibility: the pre-check ran against a transiently different tree (the crashed
session's partial artifacts), a different working directory, or a miscount/alternate invocation. **No
defect exists in this repository** — the ledger text, the seven warning classes and the mechanical
predicate (0 violations, exit 0) are all correct as committed; the validator script lives outside the repo
(`~/agent-skills`, per ticket out-of-scope — not edited) and its counting over this tree is mechanically
right. If the operator's 51 ever names its invocation, the repro above is the reconcile path; absent that,
the flag is explained, not hidden.

## Amendment 2026-10-20 — launch re-charter

**Mode:** `decompose-spec mode=extend` amendment of round 4 on `svitali/harnessharness-r3.11` (head `2f6f8a5`), records only,
same-round precedent `75d61ab` (unexecuted bodies rewritten in place with dated `> Amended` banners; no ticket file renamed —
BM-COMPAT-05; new rows are suffix-letter inserts). Executor: SWE-2, 256k context, dispatch=subagent (no compaction).

### Inputs (re-read 2026-10-20)
- LEDGER GATE DECISIONS `### Round 4` — the charter + every ruling row (read in full); CURRENT STATE; the round-4 PHASE LOG.
- The 19 owed DEFERRALS rows (status cells + dated markers read per row); BACKLOG rows BL-01…BL-46 (open + accepted set);
  COVERAGE_MATRIX non-MET rows; GATE-ACCEPT §3 (the ten deviations); the seed contracts 150–167; ADR-0346/0253/0266/0284
  revisit triggers; `crates/hh-embed/src/runtime.rs` (`EmbedModel` — the only bound `ModelPort`), `hh-gateway` `Transport`/
  `CredentialPort`, `hh-secrets` `LaunchEnvResolver`/`KERNEL_NONINHERITABLE`; the validator's id grammar.
- **Facts verified:** repo `SteveVitali/HarnessHarness` is PUBLIC, `licenseInfo: null`; sandbox `SteveVitali/hh-live-sandbox`
  PRIVATE; `git merge-tree --write-tree origin/main HEAD` → exactly one conflict (`crates/hh-cli/tests/in_process.rs`);
  main-only commits `eac030a`/`d9074a4` (ac5), `958f675`/`cd7657c` (deflake), `5668d54` (ci), `0b56aa7`/`eb9a88d` (e2 +
  CHECKER fix). **Correction to the brief:** main's ac5 rule is the derivation-aware one (admits `view_hash`/`plan_id` only
  under upstream measured drift); the chain's is the per-member list → R4.0 resolves to main's (as `6392736` did).
- **Provisioning observation (names only, 2026-10-20T15:55Z):** `live.env` present (0600) with `HH_LOCAL_BASE_URL`,
  `HH_LOCAL_MODEL`, `HH_GITHUB_WEBHOOK_SECRET`, `HH_GITHUB_SANDBOX_REPO`; absent at that time: `ANTHROPIC_API_KEY`,
  `OPENAI_API_KEY`, `HH_GITHUB_SANDBOX_TOKEN`, `HH_GITHUB_OAUTH_CLIENT_ID` (the operator is provisioning concurrently).
  `qwen2.5:7b-instruct` pulled; cloudflared, gitleaks, node v25, python 3.14 present. Recorded on HUMAN-H6.

### Round objective (stated as GATE-G6's criterion)
At GATE-G6: zero OPEN DEFERRALS rows (DONE with dated evidence, or closed waived/descoped by an ADR carrying the operator's
recorded ruling); zero open BL rows; every COVERAGE_MATRIX row MET, or MET-DIFFERENTLY only as a permanent design decision
with its ADR; `live-executed` evidence on every live leg (spend ≤ USD 100); OSS launch hardening green; the ten GATE-ACCEPT
deviations discharged by a named row or confirmed permanent. Then HUMAN-H7 and the launch record (R4.18).

### Final row list (49 rows; budgets = changed non-mechanical lines)
| # | id | kind | scope | budget | live / USD |
|---|---|---|---|---|---|
| 149a | R4.0 | ticket | main→chain merge; ac5 conflict → main's rule; regenerate; suite green | 300 | — |
| 149b | HUMAN-H6 | human | live provisioning by NAME | — | operator |
| 150 | R4.1 | ticket | context-economy tools + read rule; ac5 post-merge; helper-spawn; G5-1 | 800 | — |
| 151 | R4.2 | ticket | E3=TS bind + generated TS client + node smoke vs `serve --http` | 900 | — |
| 152 | R4.3 | ticket | xcheck HIR/ledger extension arms | 900 | — |
| 153 | R4.4 | ticket | e3-ts part 1 (core + 4 arms) | 900 | — |
| 153a | R4.4a | ticket | e3-ts part 2 (plugin + extension arms, full verify) | 900 | — |
| 154 | R4.5 | ticket | two-toolchain CI + C12 + trigger-5 | 700 | — |
| 154a | R4.5a | ticket | live-evidence ADR, records, spend ledger, `live-verify.sh` | 700 | live 0 |
| 154b | R4.5b | ticket | gateway `ModelPort` + HTTP codec home; local Ollama run | 900 | live 0 |
| 154c | R4.5c | ticket | rustls ADR + HTTPS/mTLS + `tls.terminate` | 900 | live 0 |
| 154d | R4.5d | ticket | Anthropic + OpenAI transports + broker credentials | 900 | live 5 |
| 154e | R4.5e | ticket | probe_profile + live-signal arms (closes DF-S1.18-1) | 800 | live 5 |
| 155 | R4.6 | ticket | LT-03 live corpus + live foreign-manifest import | 800 | live 15 |
| 156 | R4.7 | ticket | GitHub Issues adapter + signed webhook via tunnel (sandbox) | 900 | live 0 |
| 156a | R4.7a | ticket | OAuth device flow + mTLS mediator (operator present) | 800 | live 0 |
| 156b | R4.7b | ticket | Embedder/Summarizer/ProviderCompaction adapters | 800 | live 5 |
| 156c | R4.7c | ticket | live observability sink (local collector) | 600 | live 0 |
| 156d | R4.7d | ticket | verification reference-corpus AC halves (hermetic) | 800 | — |
| 156e | R4.7e | ticket | live judge binding + calibration + independence | 900 | live 15 |
| 156f | R4.7f | ticket | audit `subagent_anchor` + `producer_resolution` | 600 | — |
| 157 | R4.8 | ticket | BL-33 brief + ruled-posture records + naming market check + trust fields | 700 | — |
| 158 | R4.9 | ticket | DF-S4.2-2 four foreign vocabularies (hermetic) | 900 | — |
| 159 | R4.10 | ticket | BL-34 brief | 600 | — |
| 160 | R4.11 | ticket | BL-35+BL-39 brief | 800 | — |
| 161 | R4.12 | ticket | BL-36 brief part 1 (12 OQs) | 600 | — |
| 161a | R4.12a | ticket | BL-36 brief part 2 (11 OQs) | 600 | — |
| 162 | R4.13 | ticket | BL-37 brief part 1 (17 OQs) | 700 | — |
| 162a | R4.13a | ticket | BL-37 brief part 2 (17 OQs) | 700 | — |
| 163 | R4.14 | ticket | BL-38 brief + 24 §10 minors | 800 | — |
| 164 | R4.15 | ticket | BL-30 brief + OQ-170/OQ-388 proposals + H4 waiver + CF-490 | 700 | — |
| 164a | GATE-G7 | gate | rulings gate ("GATE-R4R") | — | operator |
| 164b | R4.15a | skeleton | records conform A (0210/0211/0212/0213/0216) | ≤900 at flesh | — |
| 164e | R4.15b | skeleton | records conform B (0214/0215, BL-30 CFs, OQ-388) | ≤900 | — |
| 164h | R4.15c | skeleton | code conform (CF-492, CF-493, adopted code legs) | ≤900 | — |
| 164k | R4.15d | skeleton | remote fetch + trust fields (DF-S4.2-1) | ≤900 | live 0 |
| 164m | R4.15e | skeleton | signer-key custody (DF-S2.5-1, DF-S1.15-1) | ≤900 | — |
| 164p | L4.1 | ticket | OSS files + dual license + notices + scrub | 700 (+mechanical) | — |
| 164q | L4.2 | ticket | MAINTAINING/GOVERNANCE; BL-41/42/43 closed | 600 | — |
| 164r | L4.3 | ticket | CI matrix, cargo-deny, gitleaks, dependabot, live-verify workflow | 600 | dispatch-only |
| 164s | L4.4 | ticket | scripted rename `hharness-*` + ADR | 400 (+mechanical) | — |
| 164t | L4.5 | ticket | publish readiness + release workflow + v0.1.0 | 800 (+mechanical) | — |
| 164u | L4.6 | ticket | live E2E capstone | 800 | live 30 |
| 164v | L4.7 | ticket | clean-clone quickstart test | 400 | live 0 |
| 165 | R4.16 | reconcile | zero-deferral closure accounting | 700 | — |
| 166 | GATE-G6 | gate | discharge + launch readiness | — | operator |
| 167 | R4.17 | docs | public docs final | 700 | — |
| 167a | HUMAN-H7 | human | operator launch acts | — | operator |
| 167b | R4.18 | reconcile | launch record | 300 | — |

**Spend allocation (USD 100):** R4.5d 5 · R4.5e 5 · R4.6 15 · R4.7b 5 · R4.7e 15 · L4.6 30 · reserve 25 (reserve use needs
the operator's words); every other live leg USD 0.

### Zero-deferral coverage matrix (mandatory — no "carry")

**(A) Owed DEFERRALS rows (19) → residual members → exactly one closing row**
| DF row | residual member(s) (from dated markers) | owning row(s) — closer in bold |
|---|---|---|
| DF-S0.3-2 | two-toolchain CI, C12 timing, far-side parse timing, trigger-5 row | **R4.5** (R4.4/R4.4a supply E3 evidence) |
| DF-S0.3-3 | R2 human signature | **R4.15** — closed waived-by-ADR on the operator's H4 decline |
| DF-S1.2-2 | HIR/ledger corpus extension + foreign re-run | R4.3 (arms) → **R4.4a** |
| DF-S1.5-3 | E3 ledger-transcript arm (E2 via R4.0) | **R4.4** |
| DF-S1.8-1 | E3 registry arm | **R4.4** |
| DF-S1.15-1 | (a) signer custody (OQ-170) · (b) `subagent_anchor` · (c) `producer_resolution` | (b)(c) R4.7f · (a) **R4.15e** (post GATE-G7) |
| DF-S1.18-1 | (a) live Transport/CredentialPort · (b) AC-R-2.3.1-11 live OOP · (c) real-provider `probe_profile` · (d) live-signal arms | (a) R4.5b local + R4.5d hosted · (b) R4.5d · (c)(d) **R4.5e** |
| DF-S1.21-2 | (a) reference-corpus AC fixture halves · (b) RedundantJudge/IndependenceVector on real bindings | (a) R4.7d · (b) **R4.7e** |
| DF-S1.21-3 | live `CriticDeclaration{kind=judge}` at `link` | **R4.7e** |
| DF-S1.24-2 | live foreign-manifest import | **R4.6** |
| DF-S1.27-1 | E3 plugin-manifest arm | **R4.4a** |
| DF-S2.4-2 | TLS-terminating transport | **R4.5c** |
| DF-S2.5-1 | signer-custody ruling + implementation | R4.15 (proposal) → GATE-G7 → **R4.15e** |
| DF-S4.2-1 | remote fetch with trust fields | R4.8 (trust-field proposal) → GATE-G7 → **R4.15d** |
| DF-S4.2-2 | four foreign vocabularies | **R4.9** |
| DF-S4.10-1 | E3 bind + in-ecosystem client | **R4.2** |
| DF-S4.11-2 | OAuth exchange + mTLS peer→subject | **R4.7a** |
| DF-S4.13-1 | OQ-388 ratification act | R4.15 (proposal) → GATE-G7 → **R4.15b** |
| DF-S5.6-1 | real tracker + signed webhook live | **R4.7** |

**(B) Open / accepted BL rows → members → row** (BL flips land at R4.16 except where noted)
| BL | residual members | row(s) |
|---|---|---|
| BL-01 | DF-S0.3-2, DF-S1.2-2, DF-S1.5-3, DF-S1.8-1, DF-S1.27-1 | R4.3, R4.4, R4.4a, R4.5 |
| BL-02 | DF-S0.3-3 | R4.15 (waived-by-ADR) |
| BL-03 | DF-S4.10-1, ADR-0319 trigger | R4.2 |
| BL-04 | DF-S5.6-1 | R4.7 |
| BL-05 | LT-03 live corpus · DF-S1.21-2 · DF-S1.15-1 (DF-S1.13-3/-17-1/-22-1/-24-1/-14-4/S5.4-1 verified DONE) | R4.6 · R4.7d+R4.7e · R4.7f+R4.15e |
| BL-17 | live judge (ADR-0347/0348/0300/0311) | R4.7e |
| BL-26 | live foreign-manifest import (resolver leg DONE R2.17) | R4.6 |
| BL-27 | oauth/mtls mediator (Ref + SSE DONE R2.19) | R4.7a |
| BL-30 | CF-476/487/488/491 (records) · CF-492/493 (code) · CF-490 fix · OQ-170 · OQ-388 · debt.hypothesis + parked-detach (verify answered) | R4.15 → GATE-G7 → R4.15b · R4.15c · R4.15 · R4.15e · R4.15b · R4.15 |
| BL-31 | DF-S1.18-1 · DF-S2.4-2/R-2.8.4/ADR-0266/0340/0341 · adapters (ADR-0253) · ADR-0284 arms · ADR-0346 live sink | R4.5b/d/e · R4.5c · R4.7b · R4.5e · R4.7c |
| BL-32 | DF-S4.2-2 · DF-S4.2-1 | R4.9 · R4.15d |
| BL-33 | R-2.12.4 + 13 OQs | R4.8 → GATE-G7 → R4.15a; implementation L4.1/L4.2/L4.3/L4.5 |
| BL-34 | D-1..D-6 + OQ-461/462/463 | R4.10 → R4.15a |
| BL-35 / BL-39 | 12 + 3 OQs | R4.11 → R4.15a |
| BL-36 | 23 OQs | R4.12 + R4.12a → R4.15a |
| BL-37 | 34 OQs | R4.13 + R4.13a → R4.15b |
| BL-38 | 15 OQs (+ the 24 READINESS_REPORT §10 minors) | R4.14 → R4.15b |
| BL-41 / BL-42 / BL-43 | standing rules + trigger catalogues | L4.2 (closed converted-to-policy) |

**(C) Non-MET coverage rows → owner or permanent ADR**
| requirement | verdict now | owner (closing evidence) |
|---|---|---|
| R-2.2.1 | PARTIAL | R4.15b (OQ-388 ratified) |
| R-2.7.1 | PARTIAL | R4.7e (R4.7d prerequisite) |
| R-2.7.2 | MET-DIFFERENTLY | **permanent** — ADR-0146 split (design decision; children carry verdicts) |
| R-2.7.2b | PARTIAL | R4.7e |
| R-2.8.3 / R-2.8.4 | MET-ENGINEERED(DF-S2.4-2) | R4.5c |
| R-2.8.6 | PARTIAL | R4.15e (R4.7f prerequisite) |
| R-2.9.3 | PARTIAL | R4.15d |
| R-2.9.4 / R-2.9.6 | PARTIAL | R4.6 |
| R-2.10.3 | PARTIAL | R4.15d (R4.9 prerequisite) |
| R-2.10.5 | PARTIAL | R4.9 |
| R-2.11.2 | MET-DIFFERENTLY (environment gap) | R4.2 → MET |
| R-2.11.3 | PARTIAL | R4.7a |
| R-2.12.1 | PARTIAL | R4.5 (R4.4a evidence + R4.15 waiver of the human cell) |
| R-2.12.3 | MET-DIFFERENTLY | R4.5 (measured C12 + trigger 5) — R4.16 re-verdicts; if it stays MET-DIFFERENTLY it is the permanent ADR-0050 ecosystem decision |
| R-2.12.4 | MET-DIFFERENTLY (environment gap — window now open) | L4.1 (+ R4.8 records, L4.2/L4.3/L4.5) → MET |
| R-2.12.5 | MET-DIFFERENTLY | L4.4 (ruled scheme applied; R4.8 records the market check + the `harnessharness` collision) — program-level thesis stays ADR-0003/0006 if R4.16 keeps MET-DIFFERENTLY |
| R-2.12.6 | MET-DIFFERENTLY (environment gap) | R4.7 → MET |

**(D) GATE-ACCEPT deviations (10)**
| # | deviation | disposition |
|---|---|---|
| 1 | R-2.7.2 split (ADR-0146) | permanent (design) |
| 2 | R-2.12.3 foreign legs not executed | discharged by R4.4/R4.4a/R4.5 (+R4.0 e2) |
| 3 | R-2.12.4 spec-level deferral | discharged by L4.1 (+R4.8, L4.2, L4.3, L4.5) |
| 4 | R-2.12.5 naming thesis | discharged by R4.8 (market check) + L4.4 (scheme applied) |
| 5 | R-2.11.2 E3 binding | discharged by R4.2 |
| 6 | R-2.12.6 real tracker | discharged by R4.7 |
| 7 | R2 human signature | discharged by the operator's H4 decline, recorded at R4.15 (waived-by-ADR) |
| 8 | trigger 5 satisfied-by-analysis | discharged by R4.5's measurement |
| 9 | foreign-toolchain cells | discharged by R4.4/R4.4a/R4.5 |
| 10 | GATE-G2 recorded set | discharged by R4.6 (LT-03), R4.7d/R4.7e (DF-S1.21-2), R4.7f/R4.15e (DF-S1.15-1); the other members are DONE |

**Counts:** 19/19 DF rows placed (28 residual members); 21 BL ids placed (BL-01/02/03/04/05/17/26/27/30/31/32/33/34/35/36/37/
38/39/41/42/43); 18 non-MET requirement rows placed (1 permanent); 10/10
deviations placed (1 permanent); 103 package OQs + D-1..D-6 + 7 CFs + 24 §10 minors routed to a brief → GATE-G7 → conform.
**Members I could not place:** none. Conditional: if the operator declines a brief proposal at GATE-G7 without a replacement,
that id has no landing — GATE-G7's text requires a replacement disposition at the same gate.

### Deviations from the orchestrator's proposed restructure (each with reasoning)
1. **R4.0 resolution reversed** — main's ac5 rule is the derivation-aware one; resolve to main's (the brief assumed the chain's).
2. **"154d gateway hermetic remainder" dropped** — DF-S1.18-1's hermetic members already landed (S2.10/S3.7/R2.7); the
   live track is R4.5a framework → R4.5b model port + local → R4.5c TLS → R4.5d providers → R4.5e probes/live arms.
3. **Proof on Ollama split** — R4.5a proves the framework on a zero-spend reachability leg; the real `ModelPort`/`Transport`
   proof is R4.5b (no gateway-backed `ModelPort` exists today — `EmbedModel` is scripted; that work is a ticket of its own).
4. **Evidence path** `docs/build/reports/live/…` instead of `docs/build/evidence/live/…` — `evidence/` is not an allowed
   build-memory root entry (BM-LAYOUT-01).
5. **GATE-R4R → `GATE-G7`** marker id (validator grammar admits only `GATE-G<n>`); alias recorded; readout `GATE-G7.md`.
6. **C2 detectors / Stage-3 batteries / OOP validator binding not scheduled as new engineering** — landed at S4.16c/S3.10/R2.15;
   the remaining DF-S1.21-2 work is R4.7d (AC fixture halves) + R4.7e (judge consumption).
7. **Audit Stage-2 halves = one hermetic ticket (R4.7f) + one post-ruling skeleton (R4.15e)** — the residual is 3 members, not
   the full Stage-2 list.
8. **R4.4 pre-split** (R4.4/R4.4a) — not in the brief; the foreign implementation exceeds the ≲900 budget in one ticket.
9. **HUMAN-H6 inserted** (provisioning marker) with a round-4 override: an unprovisioned NAME stops-and-asks, never opens a D-row.
10. **24 READINESS_REPORT §10 minors added to R4.14** — inherited spec-level deferrals the charter's "no remaining deferred
    work" covers; not in the brief.
11. **Two records-conform skeletons (R4.15a/b) + one code-conform (R4.15c)** instead of one records + one code — the
    adopted set may span ~130 ids; split by ADR package pre-registered.
12. **Launch ids `L4.1`–`L4.7`** for the launch track (suffix rows after 164) instead of `R4.15f…` — readability; grammar-valid.
13. **No row superseded** — every seed row keeps a live scope (R4.9 narrows to the hermetic half; its remote half → R4.15d).
14. **BL-26/BL-27 "remaining members"** reduce to R4.6/R4.7a (verified DONE elsewhere) — no extra rows.
15. **Release workflow merged into L4.5** (needs final names, after the rename) rather than L4.3's CI ticket.

### Phase 4 — fresh-context adversarial review (amendment)
**Reviewer honesty:** the `subagent_explore` tool named in the brief is not available in this session's tool set, so the
review used the skill's fallback discipline ("re-read the spec and plan from disk and argue each judgment from what is
there") plus scripted mechanical checks over every round-4 contract (forward `Depends on`, Load-line whole-file reads,
secret-shaped strings, skeleton run lines / fleshing notes, pre-answered gates, keep-open proposals). The orchestrator may
re-run a true fresh-context critic over this commit; findings would land as a dated addendum here.

| # | finding | resolution |
|---|---|---|
| P4-1 | **Overflow — large source files.** `hh-control/src/driver.rs` is 412 KB (~100k tokens), `router.rs` 127 KB, `runtime.rs` 107 KB, `broker.rs` 106 KB, `audit.rs` 87 KB, `egress.rs` 60 KB — several live contracts named them as plain Load entries. | Every one of the 40 executable contracts now carries a "large source files are never read whole — `grep -n` + `sed -n` ranges" bullet naming them. |
| P4-2 | **No room for gate-time splits.** With R4.15a–e at 164b–164f and the launch track at 164g–164m, a split of any conform skeleton at GATE-G7 had no free filename letter (one letter per prefix). | Re-lettered (new, uncommitted files only): skeletons at 164b/164e/164h/164k/164m, launch track 164p–164v; free letters c,d,f,g,i,j,l,n,o,w–z. |
| P4-3 | **R4.0 runs before the row tools exist** while AGENTS.md gotcha 1 and the LEDGER OPERATING MODE still say "read DEFERRALS first" (~78k tokens). | R4.0/R4.1 carry the literal-grep CTX block that explicitly forbids whole reads; R4.1 rewrites the rule (AGENTS.md + archived OPERATING MODE). Residual risk accepted for one ticket; this amendment does not edit AGENTS.md (planning-only scope). |
| P4-4 | R4.16's Load used `awk -F,` over a CSV with quoted commas (fragile, may dump the file). | Replaced by an explicit `scripts/bm-row.sh` id list. |
| P4-5 | R4.5e could self-mint a requirement verdict via a "price discovery" deviation ADR. | Notes added: the ADR is an engineering deviation only; any verdict change surfaces at GATE-G6 via R4.16 — never self-ruled. |
| P4-6 | R4.6 would commit upstream corpus bytes without a license check. | Added: record upstream license; commit bytes only where redistribution is permitted (+ L4.1 notices), else commit fetch script + sha256 manifest. |
| P4-7 | GATE-G7 decline-without-replacement would leave an id unplaced (violating zero-deferral). | GATE-G7 text requires a replacement disposition at the same gate; recorded as the matrix's only conditional. |
| P4-8 | Live legs vs prerequisites. | Checked: no live leg precedes R4.5a; no HTTPS leg precedes R4.5c; hosted-provider legs follow R4.5d; R4.15d follows R4.5c; L4.6 follows every live row. |
| P4-9 | Forward deps / skeleton run lines / pre-answered gates / secrets / keep-open proposals. | Scripted check: none found. |
| P4-10 | Ticket that both engineers and rules. | None: R4.15 records only operator-issued rulings (H4 decline, CF-490) and proposes the rest; R4.8's closures cite GATE DECISIONS rows; L4.4's lib-name/directory choice is an engineering ADR the brief asked the ticket to decide. |
| P4-11 | Provisioning gap: four NAMES absent at planning time (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `HH_GITHUB_SANDBOX_TOKEN`, `HH_GITHUB_OAUTH_CLIENT_ID`). | HUMAN-H6 records the observation (names only); live tickets stop-and-ask on `provided: no` — surfaced to the orchestrator for the operator. |
| P4-12 | Double-mapping risk: DF-S1.18-1 spans R4.5b/d/e; R-2.12.1's owner R4.5 precedes the R4.15 waiver. | Members are disjoint and the closer is named per row; R4.16 re-verdicts R-2.12.1 after R4.15 — recorded in the matrix. |

**Plan revisability:** the plan is a starting point — `orchestrate-build` may split an overflowing ticket (free letters are
reserved) or merge trivial ones at run time via a scoped `mode=extend`, writing the change back to the manifest.

### Amendment 2026-10-20 (b) — independent critic fixes

**Mode:** same-round `decompose-spec mode=extend` amendment on `svitali/harnessharness-r3.11` (head `02e37fe`), records only;
precedents `75d61ab` and `02e37fe` (no round-4 row has executed — unexecuted contracts edited in place with dated
`> Amended 2026-10-20 (b)` banners; no ticket file renamed; no GATE DECISIONS row added). Input: an independent
fresh-context critic's review of `02e37fe` (4 MAJOR + 8 MINOR findings — the true fresh-context pass P4's honesty note
anticipated). Dates from `date -u` (2026-10-20T16:29Z–16:41Z).

| # | finding | change (file — one line) |
|---|---|---|
| MAJOR-1(a) | crates.io names: the rename could collide after the fact | `164s_L4.4` — `rename-crates.sh` FIRST checks crates.io over the publish set derived from `cargo metadata --no-deps` (+ umbrella `hharness`), STOP-and-ask on any taken/inconclusive name before renaming anything; dated per-name output in the run ledger; new AC + Notes baseline |
| MAJOR-1(b) | R4.8's market check named only "hharness/hharness-*" | `157_R4.8` — the full to-be-published set (umbrella + one `hharness-<x>` per `cargo metadata` package) enumerated in the brief with dated per-name HTTP results; new AC |
| MAJOR-1(c) | L4.5 re-check outcome unspecified | `164t_L4.5` — run-time re-check kept; a collision is a STOP-and-ask, never a silent rename or quiet drop |
| MAJOR-2 | repo already public; first evidence commits had no secret-scan gate | `154a_R4.5a` — deliverable 8: gitleaks over the staged diff before any evidence commit (refuse + stop-and-ask), one-off `gitleaks detect --log-opts=--all` baseline with the 7 fake fixture hits (`crates/hh-secrets/tests/{acceptance,sv}.rs`, `crates/hh-telemetry/tests/acceptance.rs`) allowlisted in a committed `.gitleaks.toml`; `164r_L4.3` — CI reuses that config (CC1); `167a_HUMAN-H7` — rotate-first on any finding, history-rewrite decision recorded verbatim before tagging |
| MAJOR-3 | capstone missed `hh-acp`, `hh-mcp-lab`, SDK/embed, packaged plugins | **split:** new `164w_L4.6a` (row 45 of 50) carries the four leg families; `164u_L4.6` points to it; `165_R4.16` deliverable 4 + AC and `166_GATE-G6` threshold: EVERY public surface/binding/plugin `live-executed` with a cited record or an operator-recorded `n/a{reason}` |
| MAJOR-4 | mid-drive main drift + post-merge main never verified | `167a_HUMAN-H7` — protect first, checks-required bottom-up merges only, wait for CI matrix + gitleaks green on `main` HEAD, record the merged-main SHA, tag that SHA; `167b_R4.18` records it and proves tag = SHA; `165_R4.16` deliverable 7 + `166_GATE-G6` threshold: `git merge-base --is-ancestor origin/main HEAD`, else merge (as R4.0 did) + re-verify, drift recorded |
| MINOR-5 | DF-S1.15-1 "three members" assumed, not enumerated | `156f_R4.7f` — Goal qualified; its DF note enumerates the FULL scope-column membership against dated markers (land an unmarked hermetic member or STOP; "redaction endorsement rules" has no explicit marker at planning time); `164m_R4.15e` fleshing note — the same re-enumeration before writing DONE (last closer); `165_R4.16` deliverable 8 + `166` threshold |
| MINOR-6 | no rustdoc gate / docs.rs readiness | `164r_L4.3` — `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` in CI; `164t_L4.5` — `package.metadata.docs.rs` where needed + a docs.rs-settings doc check |
| MINOR-7 | `rust-version` undeclared policy | `164t_L4.5` — declared deliberately (pinned 1.94 unless an MSRV check proves lower); CI builds at exactly that version |
| MINOR-8 | R4.17 docs edits could break the quickstart | `167_R4.17` — AC: L4.7's `check-quickstart.sh` re-runs green after the edits |
| MINOR-9 | evidence not operator-auditable; spend authority unstated | `154a_R4.5a` — record carries provider request ids + allowlisted non-secret response headers; `SPEND.jsonl` is an estimate, console billing is the authority; `166_GATE-G6` — readout spot-audit line + console reconciliation |
| MINOR-10 | no single pre-flight before the first live row | `149b_HUMAN-H6` — "Pre-flight before R4.5a" section with a literal names-only shell check (tested against a fake env file: empty/commented/quoted-empty → `provided: no`); `154a_R4.5a` adds `live-verify.sh --check-names`; manifest HUMAN-H6 paragraph carries the orchestrator note |
| MINOR-11 | release target set unrecorded | `164t_L4.5` — release ADR records targets: macOS arm64 + x86_64 (x86_64 cross-built via `rustup target add` if no Intel runner), linux x86_64 + **aarch64 included** (GitHub-hosted `ubuntu-24.04-arm`, public repo, no new deps; unavailable → stop-and-ask), **Windows excluded** (helper backends are seatbelt/podman only — no containment floor on Windows; revisit trigger) |
| MINOR-12 | R4.15d skeleton silent on live clauses | `164k_R4.15d` fleshing note — `live_verification=true`, `Live stage: operator-gated: USD 0`, the standard live clauses (stop-and-ask on missing NAME, evidence record, leak scan) |
| secrets location | — | every contract naming `live.env` (14 live tickets' operating clause, the new 164w, `154a` Goal, `149b`, manifest H6 paragraph) now says it is a symlink to a gitignored `.env` in the operator's main checkout (names only; no value anywhere) |

**crates.io baseline (planning-time, names only):** `GET https://crates.io/api/v1/crates/<name>` for `hharness` + the 51
`hharness-<x>` names derived from `cargo metadata --no-deps` (every `hh-<x>` package) → **52/52 HTTP 404** between
2026-10-20T16:31:51Z and 16:33:02Z (≥ 1 s spacing); the orchestrator's same-day check agrees. Binding checks remain L4.4's
(pre-rename) and L4.5's (pre-publish); R4.8 records the dated R-2.12.5 evidence.

**Split decision (MAJOR-3):** L4.6 already carried ~10 legs (3 providers × 3 bindings, sandboxed exec, judge, verify/replay,
`hh-web`, `hh-mcp`, fleet webhook, bundle reproduce) at an 800-line budget. ACP + MCP-lab + SDK consumer + three
out-of-process plugins need four more crates' surfaces in the working set (`hh-acp`, `hh-mcp-lab`, `hh-embed`/facade,
`hh-varhost` + plugin manifests) — beyond one ≲100k-token subagent working set. No letter slot exists between `164u` and
`164v`, so the insert is **`164w` = L4.6a** after L4.7 and before `165` (Depends-on backward: L4.7 chain position, L4.6).
It runs entirely on the local model (USD 0) — the hosted-provider matrix stays L4.6's — so the R4.5a allocation table
(R4.5d 5 · R4.5e 5 · R4.6 15 · R4.7b 5 · R4.7e 15 · L4.6 30 · reserve 25) is unchanged. ACP client: the planning
expectation is the official ACP SDK (S0.3b precedent: python `agent-client-protocol` / node
`@zed-industries/agent-client-protocol`, temp dir outside the repo); hh-acp's in-tree client over the real transport only
as a stated fallback. MCP-lab via the official MCP SDK client likewise. `165_R4.16` now depends on `L4.6a`.

**Row count:** round 4 is **50 rows** (149a–167b + 164w). Every round-4 contract's `Sequence` line now reads "of 50"; rows
165/166/167/167a/167b renumber to 46/47/48/49/50. The `### Final row list (49 rows…)` table above is the amendment-(a)
record and is not rewritten; add `| 164w | L4.6a | ticket | live capstone part 2 — surface & plugin sweep | 700 | live 0 |`
after `164v` when reading it. R4.5a's size budget 700 → 800 (gitleaks gate + `--check-names`).

**Qualification of deviation 7 (amendment (a)):** "the residual is 3 members, not the full Stage-2 list" stands only as
far as the dated markers prove it (S2.5 checkpoints/proofs/anchors/auditor, S4.9 `fleet_anchor` + escalation emitters,
S6.1a `evolution_link`, R2.13 gc/redact emitters). R4.7f, R4.15e and R4.16 now verify every scope member against its
marker; an unmarked member (e.g. redaction endorsement rules) is landed or surfaced — never assumed.

**Not changed:** no GATE DECISIONS row; LEDGER CURRENT STATE `updatedAt` only (the `nextTicket` comment's "49 rows" is
left as written — the PHASE LOG entry records 50); DEFERRALS untouched (the duties live in the contracts); no ticket file
renamed or deleted.


## Accounting — R4.16 closure (2026-10-23)

*Appended by R4.16 (`svitali/harnessharness-r4.16`), the round-4 closure ticket. This is the
landed-vs-carried record GATE-G6's readout reads.*

### Deferral register (`docs/tickets/DEFERRALS.md`, 101 rows)

- **99 / 101 discharged** — every row carries a dated `DONE`/`WONTFIX`/`CLOSED` marker or an
  ADR-cited waiver. `scripts/deferrals-open.sh` prints exactly the two rows below; the
  lead-token sweep (BL-41 / LEDGER-OF-6, MAINTAINING.md §Round-closure sweeps) normalized
  six stale leads this closure (DF-S2.4-2, DF-S4.2-1, DF-S4.2-2, DF-S4.10-1, DF-S4.13-1,
  DF-S5.6-1 — all carried dated DONE markers; leads normalized with dated notes, evidence
  unchanged). The same sweep normalized register leads `CF-492`, `CF-493`, `OQ-430`
  (`open`→`resolved` on their dated R4.15c markers) and `OQ-170` (`open`→`resolved` — its
  `needs-code-leg(R4.15e)` pointer discharged: custody landed R4.15e, ADR-0376).
- **2 / 101 OPEN — gate-blocking findings, surfaced, not carried:**
  - **DF-S1.2-2** — the e2-tree `hir-documents` extension cell: `verified_by[]` empty;
    `foreign/e2-python` does not implement the HIR/ledger-corpus extension arms (the E3
    legs replay them 5/5 — `foreign-run-verified`; the E2 arm is unimplemented, not
    failed). Owed under BL-01 (`accepted`).
  - **DF-S1.5-3** — the e2-tree `ledger-scenarios` extension cell, same state (E3 replays
    the three scenario cases byte-for-byte; the E2 arm unimplemented). Owed under BL-01.
  - `bundle.json` `pending_cells[]`/`arms[].status` machine-readable sweep (the BL-41
    machine half): verified — every cell carrying `verified_by[]` entries is discharged
    with dated markers; exactly the two cells above (plus `polyglot-ci.two-toolchain`,
    whose DONE rests on the R4.5 measurement per its recorded spelling) carry empty
    `verified_by[]`, matching the OPEN rows one-for-one.
- **Closed at R4.16:** DF-S1.8-1 (registry-snapshot replay — E2 1/1 + E3 1/1 both
  `foreign-run-verified`); DF-R4.2a-1 (the Actions-API measurement below).

### Backlog (`docs/build/BACKLOG.csv`, 44 rows)

- **41 closed** — including the 13 flipped exactly once at this closure: BL-02, BL-03,
  BL-05 (accepted→closed) and BL-30…BL-39 (open→closed); BL-04/17/26/27 had already
  closed in-round; BL-41/42/43 converted-to-policy at L4.2.
- **3 accepted (standing, by design):** BL-01 (the two e2 extension cells above),
  BL-47 + BL-48 (the standing ADR revisit-trigger index homes — `accepted` is their
  terminal state per MAINTAINING.md §"ADR revisit triggers").
- **Re-home:** 29 row-bound ADR ids moved off the closing rows' `sources` cells to BL-47
  (every one retains at least one standing quiet-conditional clause in its own
  `## Revisit trigger` text; none is wholly consumed). `REVISIT_TRIGGERS.md` regenerated
  (`scripts/gen-revisit-triggers.sh`); the catalogue now reads 165 ADRs: 113
  quiet-conditional / 49 fired-and-answered / 3 fired-unanswered row-bound
  (ADR-0223, ADR-0239, ADR-0353 — homed on BL-01 with its two open cells) /
  0 superseded.
- `check-backlog.sh` exit **0**: complete + non-duplicating + verdict-consistent over
  166 expected sources; `--bl` sweep empty.

### Coverage (`docs/build/COVERAGE_MATRIX.csv`, 66 requirement rows)

- **engineering closed 65 / 66 · requirement satisfied 65 / 66** — 60 MET, 5
  MET-DIFFERENTLY (each a permanent-design ADR: R-2.7.2→ADR-0146, R-2.11.2→ADR-0050,
  R-2.12.3→ADR-0050, R-2.12.4→ADR-0210, R-2.12.5→ADR-0003), 1 PARTIAL.
- **The one PARTIAL is R-2.12.1** — cross-implementation replay: every leg discharged
  except the DF-S1.2-2/DF-S1.5-3 e2 extension cells above. It is the honest shadow of the
  two OPEN deferral rows.
- **Flipped to MET at this closure** (the recorded "verdict flip is R4.16 accounting"
  executes): R-2.7.1, R-2.7.2b (R4.7e judge legs), R-2.9.3, R-2.10.3 (R4.15d fetch),
  R-2.9.4, R-2.9.6 (R4.6 import + ADR-0331), R-2.10.5 (R4.9 vocabularies), R-2.11.3
  (R4.7d oauth-device); R-2.12.6 MET-DIFFERENTLY→MET (R4.7 real adapter superseded the
  fixture proxy). Every other named row carries a dated re-check note.

### The ten GATE-ACCEPT deviations — discharge table

| # | Deviation | Disposition at R4.16 | Discharging rows | Permanent ADR |
|---|---|---|---|---|
| 1 | R-2.7.2 split | **permanent (design)** — confirmed | — | ADR-0146 |
| 2 | R-2.12.3 foreign legs not executed | discharged | R4.4/R4.4a/R4.5 (+R4.0 e2); DF-S0.3-2 DONE (measured), DF-S0.3-3 waived | ADR-0050 (bind), ADR-0371 (waiver) |
| 3 | R-2.12.4 spec-level deferral | discharged | R4.8 rulings; L4.1 (OSS files/license), L4.2 (policy), L4.4 (rename), L4.5 (publish metadata/release workflow) | ADR-0210 (the spec-level deferral is the disposition) |
| 4 | R-2.12.5 naming thesis | discharged | R4.8 market check (54/54 publish-set names free) + L4.4 scheme applied | ADR-0003/0006 (thesis stands program-level) |
| 5 | R-2.11.2 E3 binding | discharged | R4.2 — operator's H1 "Confirm TypeScript" verbatim on ADR-0050's log; generated `clients/ts` executed (e3-smoke 17/17) | ADR-0050 |
| 6 | R-2.12.6 real tracker | discharged | R4.7 — GitHub Issues adapter + signed webhook live (`github-live`, `capstone-fleet-webhook`); DF-S5.6-1 DONE | ADR-0369 |
| 7 | R2 human signature | discharged — **waived, not signed** | R4.15 — operator's verbatim "Record a decline instead" on the H4 row | ADR-0371 (the decline is recorded; the signature is never fabricated) |
| 8 | trigger-5 satisfied-by-analysis | discharged | R4.5 measurement — C12(E5a)=5 vs assumed 2; decision HOLDS, recorded on ADR-0050's amendment log | ADR-0050 |
| 9 | foreign-toolchain cells | discharged — **with the two e2 extension cells surfaced** | R4.0 (e2 candidate), R4.3 (bundle extension arms), R4.4/4a/4b (e3 toolchain: 57/57 all eight arms), R4.5 (xcheck-foreign + polyglot measurement); residual: DF-S1.2-2/-S1.5-3 OPEN | ADR-0353 (packaging) |
| 10 | GATE-G2 recorded set | discharged | R4.6 (LT-03 live corpus), R4.7d/R4.7e (DF-S1.21-2), R4.7f/R4.15e (DF-S1.15-1 full-membership) — the earlier members were already DONE | — |

### DF-R4.2a-1 — hosted-CI measurement (Actions API, 2026-10-23)

**Window:** `pull_request` runs on chain branches `svitali/harnessharness-r4.3` →
`-l4.6a`, PRs #152–#195 (44 tickets). **Push runs on chain branches: 0.**
Method: `gh api repos/…/actions/runs` per branch + per-run `jobs` (final attempt);
classification `fast` = the docs-only path (`ci-mode: fast` — path-filtered jobs only),
`full` = the complete job set. Raw collection retained at the closure record.

| Class | Runs | Job-minutes | Avg/run |
|---|---|---|---|
| fast-path heads | 106 | 190 | 1.8 |
| full-path heads | 40 | 486 | 12.2 |
| **actual total** | **146** | **676** | — |
| counterfactual (every head full at its era's job set) | 146 | ~1882 | — |
| **reduction** | | | **64.1%** |

Split: R4.3→R4.15e window — 107 runs, 66.1% reduction, fast avg 1.1 min, full avg 8.7.
L4.1→L4.6a window — 39 runs, 61.9% reduction, fast avg 3.9, full avg 20.2 (the workflow
grew a two-OS build matrix + always-on `cargo-deny`/`gitleaks` at L4.x; both floors rose).
Docs-only heads: 27 runs, avg **0.96 min**, all ≤ 2 min. Spot-check: five fast-path heads
on code PRs (ff1f0fc3, c351f356, aeed5a32, a561f04c, 648f3e58) — every touched file under
`docs/`/`research/` only; no fast path ever ran on a non-docs head (fail-closed per
ADR-0362). First cross-PR fast path: run 39550855673 on PR #152 (2026-10-21T05:40Z).
GATE-G7 full-run evidence: `workflow_dispatch` run **39765822939** on the R4.15 head
`9a6590a` — 12 job-min, success (ADR-0362 D4 per-gate full read; the only other dispatch
in the window is R4.2a's own baseline, 39522657481). Red full runs: 8 across the window
(2 fast-path fails: #158 1 min, #188 1 min — both re-ran green); each is recorded
per-ticket in its run ledger — none is a fast-path defect, each re-ran or was retried to
green on the same head.

**Verdict:** **missed by ~13–16 points against the projected 77–80%** — measured
64.1%. Cause: (a) the L4-era job set roughly doubled (two-OS matrix + supply-chain jobs),
raising the fast floor to ~4–5 min and full runs to ~20–24 min; (b) 23 superseded-head
cancels consumed minutes the projection never modelled; (c) stacked-PR retries produced
more full anchors per ticket than the single-anchor projection assumed. The
docs-only-head sub-projection ("around one minute") is **met** (0.96 min avg). The
fail-closed classifier is sound — no fast run carried non-docs changes; the miss is a
workload-shape miss, not a safety miss. DF-R4.2a-1 **DONE 2026-10-23** — projection
partially missed, recorded, not hidden (the row's `how to verify` names the verdict
form "met or missed by <x> (cause)" — this is the "missed" arm, honestly recorded).

### Spend and pre-authorizations

- **Live spend:** USD **25.259768 / 100 cap** — 90 `SPEND.jsonl` rows; R4.16 itself spent
  USD 0 (offline ticket; `live_verification=false`). The R4.5a allocation table held.
- **HUMAN-H3 (foreign-toolchain material):** consumed exactly as pre-authorized — the E2
  candidate environment (PR #136, 49/49 base arms) and the E3 toolchain for
  R4.4/4a/4b + R4.4b's extension-arm runs; the e2 extension arms stayed unimplemented —
  surfaced, not provisioned around.
- **HUMAN-H1/H2/H4:** consumed — "Confirm TypeScript" (ADR-0050 log, R4.2), H2's decline
  on the tracker signature (superseded by R4.7's real adapter, ADR-0369), H4's recorded
  decline → ADR-0371 waiver. **HUMAN-H5:** `ring`/rustls admissions consumed under
  ADR-0359/0361/0365's closed scopes only.
- **ADR-0367 (declared pricing limitation):** spend figures are ledger-side estimates;
  console billing is the authority — spot-reconciled at L4.6's capstone closeout, no
  discrepancy surfaced; the $100 cap was never approached (25.3% consumed).

### Mid-drive main drift (containment record)

`origin/main` moved 34 commits mid-drive (PR merges #138–#170 of earlier chain tickets
r3.3–r4.8). R4.16 merged it at `6a6475f` (2026-10-23): the merge content was already
ancestor-contained — **0 conflicts**; re-verification (suite/drift/quickstart) recorded in
`docs/build/runs/R4.16.md`. Post-merge `git merge-base --is-ancestor origin/main HEAD`
exits 0.

### Remaining debt statement (for GATE-G6)

Exactly two OPEN deferral rows (the e2 extension cells, BL-01 accepted) + three standing
accepted rows (BL-01, BL-47, BL-48). Everything else: closed. The two OPEN rows are
surfaced as gate-blocking findings with full provenance — they require the
`foreign/e2-python` extension arms to be implemented (a foreign-toolchain leg, not E1
engineering debt) or an operator descope ruling at the gate.
