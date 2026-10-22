# Maintaining HarnessHarness

The maintainer policy for the E1 reference build — the standing rules a
maintainer follows between rounds and at release. This file lands the
2026-10-20 standing-rows ruling ("Convert to maintainer policy" —
`docs/build/LEDGER.md` decision table): what were the BACKLOG rows BL-41
(write integrity) and BL-42/BL-43 (the ADR revisit-trigger catalogues) are
maintainer policy here; the three rows closed `converted-to-policy`
(ADR-0377). Companion documents: `GOVERNANCE.md` (decision authority),
`CONTRIBUTING.md` (how a change lands), `AGENTS.md` (the behavioural
contract).

## Releasing

The release act itself is an operator act (the HUMAN-H7 row); everything up
to it is scripted and dry-runnable.

1. **Version.** `Cargo.toml` `[workspace.package].version` is the single
   version source — every member carries `version.workspace = true`. Bump
   it once per release; the launch release is `0.1.0`.
2. **CHANGELOG.md.** Keep a Changelog format: work accumulates under
   `## [Unreleased]`; at release, rename the section `## [x.y.z] —
   <YYYY-MM-DD>` (UTC date) and open a fresh `## [Unreleased]` above it.
3. **Tag.** `v<x.y.z>` on the merged `main` commit, pushed by the operator.
4. **Release workflow.** The L4.5 release workflow builds the declared
   target set (macOS arm64/x86_64, linux x86_64/aarch64; Windows excluded)
   plus checksums and drafts the GitHub Release.
5. **Publish order.** `cargo publish` runs crate-by-crate in dependency
   order over the publish set (fixtures, plugins and test-support crates
   are `publish = false`). `scripts/publish-order.sh` (lands at L4.5)
   prints that order and drives the `--dry-run` / `cargo package` sweep.
   crates.io ownership of the `hharness` + `hharness-<x>` names is checked
   at publish time — a name collision is a stop-and-ask, never a rename or
   a quiet drop from the set.

## Evidence classes

The claims ladder is `engineered` → `fixture-verified` → `staging-verified`
→ `live-executed` → `public`, plus `human-completed` for legs only a person
did and `foreign-run-verified` (a foreign toolchain's recorded replay of
the committed `hh-xcheck` bundle — a distinct class below the in-tree
ceiling, never conflated with `fixture-verified` or `live-executed`).

- The default verified ceiling is `fixture-verified` — exercised
  end-to-end against fixtures, never a live service.
  `staging-verified`/`public` stay forbidden until the L4.6/R4.18 legs earn
  them; `docs/build/OPERATIONAL_READINESS.md` is the capability account.
- `live-executed` is claimable only with a cited `hh-live-evidence/1`
  record under `docs/build/reports/live/` produced by
  `scripts/live-verify.sh` (ADR-0363). A leg that did not execute records
  `skipped(<reason>)`/`environment-pending` — never a pass.

**When a live leg must be re-run.** A live evidence record attests to its
recorded pins — the code commit, the upstream corpus/toolchain revision,
the provider surface, and dated declared pricing (ADR-0367). Re-run the leg
and record a fresh `hh-live-evidence/1` when:

- the code path it evidences changed;
- a recorded pin moved — an upstream corpus/toolchain rev, a provider API
  surface, or the declared pricing;
- the register's `evidence_max_age` horizon passed (one model generation —
  the RK-06 rule below);
- an RK-06 revalidation sweep or an ADR revisit trigger names the leg.

A lapsed leg is never quietly dropped: it is re-run, or its claim is
downgraded with a dated note.

## Write integrity

Two standing rules — formerly BL-41 / LEDGER-OF-1…6 (the findings behind
them are all discharged; the rules remain):

1. **Write via shell/python and verify with `git diff`.** The agent `edit`
   tool has a confirmed stale-overlay anomaly — it can report a write that
   never landed. Write through the shell or a script and diff-verify before
   committing; `git show --stat` must list every file the commit claims.
   (Confirmed on the build-memory files; the rule covers every file.)
2. **The lead-token / machine-readable re-sweep at round closure**
   (LEDGER-OF-6). `DEFERRALS.md` status cells are token-led and
   marker-governed: a dated `**DONE**` marker can leave a stale leading
   `OPEN`/`PARTIAL` (the G4-4 class — swept at R3.1 and R3.10), and
   machine-readable pending sets (`hh-xcheck-bundle` `pending_cells[]` /
   `arms[].status`) understate partial foreign discharge. Owed state is
   always read from the dated markers (`scripts/deferrals-open.sh`), never
   from lead tokens alone.

The resolved findings behind the rules, kept for provenance: the
concurrent-writer and foreign-writer claims were runner re-read artifacts
resolved at the same sweep; the G1 operator deviations were discharged at
GATE-ACCEPT; the CC4 report-prose nit was adjudicated out-of-scope at
CAP.3. The full mapping lives at `docs/build/BACKLOG.md` §"Findings
mapping".

## Round-closure sweeps

Before a round's closure gate reads counts:

- Re-sweep `DEFERRALS.md` lead tokens and the machine-readable pending sets
  against the dated markers (the rule above) — flip stale leads with the
  dated evidence preserved.
- Re-run `check-backlog` — every owed id homed exactly once on a
  non-closed row.
- Regenerate `docs/adr/REVISIT_TRIGGERS.md` and triage the delta (below).
- The `docs/build/COVERAGE_MATRIX.csv` verdict flips are the closure's own
  accounting act, applied there.

## ADR revisit triggers

Every `docs/adr/ADR-*.md` carries a mandatory `## Revisit trigger`. The
generated catalogue is `docs/adr/REVISIT_TRIGGERS.md` — regenerate with
`scripts/gen-revisit-triggers.sh`; CI drift-checks it (`--check`), so the
catalogue is produced, never hand-copied. The `BACKLOG.csv` `sources` cells
remain the classification authority; this file is the readable projection.

Classes and their triage:

- **quiet-conditional** (index home BL-47) — the condition is unmet; the
  decision stands and re-fires on its own text. When the condition lands,
  record a dated re-check at `docs/build/reports/<date>-<adr>-trigger.md`
  (ADR bodies are frozen — the re-check never appends to the ADR), re-home
  the id (BL-47 → BL-48 when answered, or onto the row that owns the
  residual), and regenerate the catalogue.
- **fired-answered** (index home BL-48) — the condition landed during the
  build and was discharged with dated evidence; the trigger re-arms per its
  own text.
- **fired-unanswered / row-bound** — the condition met and a residual is
  owed; the named backlog row owns it. Triage is reading that row.
- A new ADR is catalogued by placing its id in exactly one `sources` cell
  (its trigger's verdict decides which) and regenerating; an unplaced ADR
  fails the generator and `check-backlog` alike.

Never answer a quiet trigger by editing the catalogue — the answer is a
dated record plus the re-home.

## Spec-debt revalidation (RK-06)

The `spec-debt.md` register's post-Phase-5 revalidation rides this policy
(OQ-445's adopted disposition — GATE-G7 "Adopt all", 2026-10-22; ADR-0210
amendment log; spec §11.6):

- **Owner: the maintainer** — the revalidation review is a maintainer act
  on the RK-06 cadence.
- **`evidence_max_age` = one model generation** — the register-header
  MUST-data (distinct from `DebtPolicy.evidence_max_age` = 180 d,
  OQ-441/R4.15b). When the model generation a record's evidence assumes
  rolls, the record is due for re-grading at the next sweep.
- **A refuted `provisional` source flags `expiring`, never auto-reopens**
  the ADR — re-opening stays a decision act (a new ADR or a recorded
  re-check), never a silent state change.

A revalidation sweep re-grades each `AssumptionDebtRecord`'s evidence
against its recorded generation, flips `expiring` where a cited provisional
source is refuted, and appends a dated sweep summary plus a Log row to the
register.
