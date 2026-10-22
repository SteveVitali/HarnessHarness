# GATE-G7 readout — Round-4 rulings gate ("GATE-R4R")

Status: PENDING   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

*Append-only. Each operator reading is a new dated block; never edit an earlier one.*

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is approval.

## Criterion (verbatim — the gate ticket `docs/tickets/164a_GATE-G7__round4-rulings.md`)

> "Mid-round rulings gate" (Devin CLI structured question, 2026-10-20) — recorded consequence:
> window-package + BL-30 briefs run early; this gate stops the chain for the operator's adopt /
> amend / decline rows; conform tickets (skeletons until then) land the adopted set before closure;
> GATE-G6 no longer carries ruling conforms.

**Blocks:** the R4.15a–R4.15e conform skeletons (rows 164b–164m) — they flesh from the adopted set
via a scoped `decompose-spec mode=extend` after the operator's dispositions land, then the launch
rows L4.1–L4.7 (164p–164w) and the closure tail.

**Pre-registered thresholds** *(verified 2026-10-22T10:49Z — all met)*:

- Brief files exist for every package — **7/7**, each opening with the batch-adoptable table +
  `Operator act: "adopt all except: <ids>"`:
  `round4-bl33-ws-l6` (305 ln), `round4-bl34-adr-0211` (190), `round4-bl35-bl39` (266),
  `round4-bl36-adr-0213` (396), `round4-bl37-adr-0214` (641), `round4-bl38-adr-0215` (357),
  `round4-bl30-adjudication-briefs.md` (190).
- Every in-scope OQ / CF / D-n / §10 minor appears exactly once with a FINAL proposed disposition —
  **146 proposals across the 7 briefs** (counts below); zero `keep-open` / `keep-interim` / `defer`
  proposals (the token set is closed and grep-verified by the workers).
- The OQ-170 custody proposal is **concrete** (kernel-held Ed25519 via `ring::signature` — a named
  ADR-0359 D2 scope extension — file-backed per installation at `0600`, provisioned via
  `CredentialBroker::kernel_use(KernelPurpose::LedgerSigning)`, `signer_key_ids` populated, co-signed
  `rotation` checkpoint — bl30 brief §OQ-170; 3 alternatives with trade-offs included).
- The OQ-388 ratification text is **drafted** (`adopt-interim-as-final`: the ~15 ms filtered read of
  100-of-10⁵ — `ac_r_2_2_1_16_latency_1e5_fixture`, ADR-0305 D1 + ADR-0331 D4 — bl30 brief §OQ-388).
- The R4.8 trust-field section **exists** (bl33 brief §trust-fields: `FetchTrustPolicy` MUST-data;
  admission classes `first_party`/`signed_url`/`content_addressed`, cleartext refused; `expires`
  enforced; `bundle.member.fetched` ledgered arrival; S3 re-verify) — R4.15d's conform input.

**DEFERRALS rule applied:** the gate does not pass while any brief-scoped id lacks an operator
disposition — the operator's act below supplies them all.

## Evidence (per item: source, `date -u` of the read, layer reached)

- The gate ticket `docs/tickets/164a_GATE-G7__round4-rulings.md` — read 2026-10-22T10:49Z — layer: gate marker
- The 7 brief files — read 2026-10-22T10:49Z — layer: records (disposition counts derived from the
  batch tables below, not pinned)
- `docs/build/LEDGER.md` GATE DECISIONS `### Round 4` — read 2026-10-22T10:49Z — layer: machine state
  (the rulings the recording acts cite: license `MIT OR Apache-2.0`; crates.io umbrella
  `hharness`/`hharness-<component>`; project name stays HarnessHarness; provenance kept with
  living-doc scrub; `Record a decline instead` — H4; `Admit rustls (ring backend)`; CI-insert D5)
- `docs/tickets/DEFERRALS.md` via `deferrals-open.sh` — read 2026-10-22T10:49Z — layer: records
  (8 open rows — see owed set below)
- `docs/build/BACKLOG.csv` via `deferrals-open.sh --bl` — read 2026-10-22T10:49Z — layer: records
  (11 open rows — BL-30…BL-39 incl. BL-31 flip-pass candidate, BL-42)
- `docs/build/reports/live/SPEND.jsonl` — read 2026-10-22T10:49Z — layer: live-executed
  (cumulative **$21.257705** of the USD 100 cap; R4.7f onward adds none — the conform/launch rows
  are offline except R4.15d's fetch leg and L4.6/L4.6a's live capstone)
- `docs/build/reports/S0.3-measurement-sheet.md` + `docs/adr/ADR-0371*` — read 2026-10-22T10:49Z —
  layer: records (the H4 decline recorded verbatim; DF-S0.3-3 → WONTFIX waived-by-ADR; BL-02's flip
  stays R4.16's closure accounting)
- ci-boundary stack read — `ci: pass #179@dbde462 (build · test · fmt · drift 39765309483) · stack:
  #178–#154 pass` — 26 stacked PRs open, all green
- `workflow_dispatch` full run on the chain tip `svitali/harnessharness-r4.15` — run `39765822939` —
  **`build · test · fmt · drift`: success** (read 2026-10-22T11:20Z via `gh run view` — the D4
  full-run evidence; L4.3's `scripts/ci/full-run.sh` reader lands later)

## The proposals by package (derived from the batch tables — read 2026-10-22T10:49Z)

| package | rows | resolve-with-evidence | adopt-interim-as-final | descope-by-ADR | needs-code-leg | pointer-only |
|---|---|---|---|---|---|---|
| **BL-33 / WS-L6** (R4.8) | 14 | 0* | 0 | 0 | 11 × R4.15a, 1 × R4.15d | 2 |
| **BL-34 / ADR-0211** (R4.10) | 9 | 1 | 1 | 7 | 0 | 0 |
| **BL-35+39 / ADR-0212+0216** (R4.11) | 15 | 2 | 12 | 0 | 1 × R4.15c | 0 |
| **BL-36 / ADR-0213** (R4.12+12a) | 23 | 0 | 22 | 0 | 0 | 1 (OQ-170 → R4.15e) |
| **BL-37 / ADR-0214** (R4.13+13a) | 34 | 7 | 25 | 0 | 1 × R4.15c | 1 (OQ-388 → bl30) |
| **BL-38 / ADR-0215 + §10 minors** (R4.14) | 39 | 9 | 17 | 5 | 8 × R4.15b, 1 × R4.15c | 0 |
| **BL-30 adjudication** (R4.15) | 12 | 2 | 5 | 0 | 1 × R4.15b, 2 × R4.15c, 1 × R4.15e | 1 (CF-490 — ruled, recorded) |
| **Totals** | **146** | **21** | **82** | **12** | **27** | **4** |

*\*BL-33's two recording acts (R-2.12.4 postures, OQ-035 discharge) are `resolve-with-evidence` under
the GATE DECISIONS citations — recorded already, not re-proposed.*

**Recording acts already landed** (determined by landed rulings — no operator act owed):
R-2.12.4 postures (R4.8) · OQ-035 (R4.8) · OQ-131's human residual via the H4 decline (R4.11 brief;
the waiver recorded R4.15) · OQ-177 via ADR-0306 (R4.13) · DF-S0.3-3 → WONTFIX + ADR-0371 (R4.15) ·
CF-490 header fix (R4.15).

**Named residuals for the operator's eye** (not gate-blocking — the conform path owns them):
- **15 consecutive size-budget deviations** (R4.4b…R4.9) — accepted-as-recorded under the
  stop-and-surface rule; the live-track under-pricing pattern is the standing GATE-G7 carry. If any
  is objectionable, the PR is the merge-gate point.
- **ADR-0367 `DeclaredPricing` limitation** — hosted-provider price discovery has no provider API;
  declared pricing + staleness bound stands as the disposition (R4.5e, recorded for GATE-G6).
- **8 open DF rows:** DF-S1.2-2/DF-S1.5-3/DF-S1.8-1 (E3/e2 residual legs — environment-pending
  cells), DF-S1.15-1 (signer custody → OQ-170 → R4.15e), DF-S2.5-1 (same), DF-S4.2-1 (remote fetch →
  R4.15d), DF-S4.13-1 (OQ-388 → R4.15b), DF-R4.2a-1 (CI after-state → R4.16; needs the full window).
- **11 open BL rows** — flip accounting is R4.16's per the closure rule.

## What the gate produces

The operator's GATE DECISIONS rows (`kind: decision`, words verbatim — adopt / amend / decline per
id or the batch form `"adopt all except …"` per package). A **declined** proposal needs a
replacement disposition at the same gate — the round's charter admits no carried open item.

Then: the orchestrator runs scoped `decompose-spec mode=extend` fleshing skeletons R4.15a–R4.15e
from the adopted set, each with Phase-4 fresh-context review.

## Operator act requested

Adopt / amend / decline per package (one sentence each is enough — the tables are batch-adoptable):

1. **BL-33** — adopt the 12 code-leg proposals (11 × R4.15a + 1 × R4.15d)?
2. **BL-34** — adopt 1 resolve + 1 interim + 7 descope?
3. **BL-35+39** — adopt 2 resolve + 12 interim + 1 code-leg?
4. **BL-36** — adopt 22 interim + the OQ-170 custody proposal (as the R4.15e input)?
5. **BL-37** — adopt 7 resolve + 25 interim + 1 code-leg?
6. **BL-38** — adopt 9 resolve + 17 interim + 5 descope + 9 code-legs (incl. the 24 §10 minors)?
7. **BL-30** — adopt 2 resolve + 5 interim + 4 code-legs (incl. OQ-388 ratification text + OQ-170
   custody → R4.15e)?

Plus the standing deviation carries above (silence on them = the recorded state stands; only an
objection needs words).

## Disposition

- [ ] Operator disposition recorded in `docs/build/LEDGER.md` GATE DECISIONS and this file as
      PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE (+ what would pass it).
