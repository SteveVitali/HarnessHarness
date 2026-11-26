# PR — DOC7.1: Round-7 docs refresh — accepted-evidence surfaces + the publication pin as typed absence

Manifest 338 (final row) · docs block · offline-only · **USD $0.00** · branch
`codex/harnessharness-r6.53` · depends on GATE-G37 (PASSED `accept`,
verbatim_sha256 `ea9ac3e0…`).

## What

Refreshes the reader/agent documentation surfaces from Round-7's **accepted**
evidence — the §8.x phase-4 second pass — and lands the tail bookkeeping. No
new evidence, claims, or publication acts (none authorized by registration).

1. **Phase-4 reader surfaces** — `docs/status.md` gains the dated Round-7
   section (GATE-G37 `accept`; terminal `complete` = 16 satisfied + 2
   typed_terminal over the 18 routed §8.x ids; P4-C
   `inconclusive{unresolved_cell}` verbatim — 367/370 resolved, V1 Δ̂
   `refused{UnresolvedCell}`, Holm α = .05 not rejected; HUMAN-H21
   `within`/`unavailable`/`pending{clock}` cited; the 8-row open DF set).
   `docs/results/README.md` gains the phase-4 second-pass record row citing
   `hh-p4-c-confirmation/1` `2026-11-26-fp-p4c-confirmation.json`.
2. **The publication pin recorded verbatim as a typed absence** —
   `docs/results/README.md` carries "not yet published — publication gate
   pending" for the phase-4 second pass: adding the packet to
   `docs/results/site-data` trips the publication pin, and minting a
   `hh-site-publication/1` record is a publication act **outside this chain's
   authorization** (R7.6 verbatim). `site-data/` + `publication/` byte-untouched
   — no input added, no record minted, no generated file patched. The packet's
   §8.8 two-statement claims are quoted packet-side verbatim (confirmatory
   "Not confirmed on 37 held-out tasks…" + selection-labelled exploratory
   form with expected-optimism label).
3. **Navigation** — `docs/README.md` gains the Round-7 record-set row
   (decision memo, decomposition, GATE-G37 readout, O-3 scope-out record,
   gap analysis, confirmation packet).
4. **Tail bookkeeping** — LEDGER `lastCompleted: DOC7.1` / `nextTicket: DONE`
   (338/338 — the manifest's terminal row; `DONE` is the recorded terminal
   nextTicket precedent 2026-11-05); OPEN FINDINGS bullet + PHASE LOG entry;
   BUILD_INDEX row 338.

## Verification

- `check-supplement-guard.sh --no-checker` — PASS `DISPATCH — DOC7.1`
  pre-write; post-write route recorded verbatim in `runs/DOC7.1.md` (terminal
  HOLD/FAIL on the DONE token = the 2026-11-05 precedent — correct posture).
- `check-site.sh --profile internal` + `--profile public` — green over the
  unchanged site layer; §8.8 banned/conditional phrases absent.
- `check-living-docs.sh` + doc-surface checkers (`check-position.sh`,
  `check-examples.sh`, `check-trace.sh`, `check-contributing.sh`) executed
  over touched surfaces; `--head` variants post-commit.
- `deferrals-open.sh` — 8-row true owed set re-verified; `git diff` reviewed
  per BL-41 before commit (python writes for LEDGER/BUILD_INDEX).
- Not run: cargo suites/emitters (records-and-docs ticket — no code or
  emitted artifact changed).

## Deferrals

None opened or closed; the 8-row true owed set carries verbatim
(DF-R7.1-2, DF-R7.3-1/-2/-4, DF-R7.5-1/-2, DF-CAP7.1-1/-2).
