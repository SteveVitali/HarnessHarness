# DOC7.1 — Round-7 docs refresh (accepted evidence only; manifest 338, final row)

Disposition: **done — executed, offline (USD $0.00)**. Docs leg: refresh the
reader/agent surfaces from the round's accepted evidence, keep the
publication-pin state as a typed absence, and land the tail bookkeeping.
No evidence, claims, publication, or live act in scope.

- Manifest: `docs/tickets/338_DOC7.1__docs.md` (338/338 — the terminal row).
- Branch `codex/harnessharness-r6.53`; depends on GATE-G37 (PASSED `accept`,
  verbatim_sha256 `ea9ac3e0…`, readout `docs/build/readouts/GATE-G37.md`).
- Harness: devin-cli / swe-2-high / subagent (LEDGER `harness:` verbatim).

## What landed

- **`docs/status.md`** — new dated `## Round-7 experiments program` section:
  GATE-G37 PASSED `accept`; CAP7.1's terminal determination `complete` (16
  satisfied + 2 typed_terminal — REQ-XD-205 `withheld{inert_at_admissible_window}`,
  REQ-XD-214 `not_admitted{m_eq_1}` — 0 unsatisfied/pending/absent over the 18
  routed §8.x ids); P4-C `inconclusive{unresolved_cell}` verbatim (367/370
  resolved, V1 Δ̂ `refused{UnresolvedCell}`, Holm α = .05 not rejected); phase
  cumulative $68.828421 with HUMAN-H21's `within`/`unavailable`/`pending{clock}`
  cited; the 8-row open DF set and the O-3 record named verbatim.
- **`docs/results/README.md`** — phase-4 second-pass row in the dated-record
  table citing `docs/build/reports/2026-11-26-fp-p4c-confirmation.json`
  (`hh-p4-c-confirmation/1`), plus a dedicated subsection carrying the
  **publication-pin state verbatim as a typed absence**: "not yet published —
  publication gate pending". The packet's §8.8 two-statement claims are quoted
  packet-side verbatim (confirmatory "Not confirmed on 37 held-out tasks…" +
  the selection-labelled exploratory form with the expected-optimism label);
  outcome + descriptive cost fields (baseline 10,398,498 / V1 6,239,536 µ$)
  cited verbatim.
- **`docs/README.md`** — documentation-map navigation row for the Round-7
  record set: decision memo `2026-11-19_round7-decision-memo.md`, decomposition
  `2026-11-20_round7-decomposition.md`, GATE-G37 readout,
  `reports/round7/O3-scope-out.md`, `2026-11-26-round7-gap-analysis`,
  `2026-11-26-fp-p4c-confirmation.json`; the site absence is linked from the
  results README anchor.
- **`docs/build/LEDGER.md`** — `lastCompleted: DOC7.1`, `nextTicket: DONE`
  (338/338 — the manifest's terminal row; `DONE` is the recorded terminal
  nextTicket precedent 2026-11-05), OPEN FINDINGS dated bullet, PHASE LOG
  Round-7 entry.
- **`docs/build/BUILD_INDEX.md`** — row 338; **`docs/build/pr/DOC7.1.md`**.

## The publication pin (verbatim posture)

Site-data integration was **not** landed — the R7.6 ruling stands verbatim:
adding the phase-4 packet as a `phase-4` input to `docs/results/site-data`
trips the publication-record pin ("current phase-4 input … is not pinned"), and
minting a `hh-site-publication/1` record is a publication act **outside this
chain's authorization** — and outside this ticket's. `docs/results/site-data/`
and `docs/results/publication/` are byte-untouched: no input added, no record
minted, no generated file patched. The packet's only site-adjacent surface is
the typed absence in `docs/results/README.md` — the same class the site itself
uses for withheld/unpublished evidence ("not yet published — publication gate
pending").

## AC mapping

- **AC-1 (claims checks green, banned/conditional phrases absent):**
  `bash scripts/check-site.sh --profile internal` + `--profile public` — green
  over the unchanged site layer (the typed absence lives in the living README,
  outside the scanned site-data/page surface). §8.8 two-statement claims
  appear only as packet-verbatim quotations.
- **AC-2 (every claim surface cites its accepted record; typed absences
  preserved):** status/results/README rows cite `GATE-G37.md`,
  `2026-11-26-fp-p4c-confirmation.json`, `O3-scope-out.md`,
  `readouts/HUMAN-H21.md`; every unavailable leg keeps its typed disposition
  (`pending{clock}`, `unavailable`, the publication pin itself).
- **AC-3 (universal phase gate):** run/PR/BUILD_INDEX/LEDGER agree — row 338,
  `lastCompleted: DOC7.1`, `nextTicket: DONE`; nothing not automatically
  verifiable left unrowed (residuals enumerated below).

## Verification (executed)

- `bash docs/build/tools/check-supplement-guard.sh --no-checker` — **PASS**,
  route `DISPATCH — DOC7.1` pre-write; **PASS** post-write, route verbatim
  `HOLD — active round fully landed` (the terminal-state posture — the
  expected route on `nextTicket: DONE`; the 2026-11-05 retained-guard
  precedent's possible DONE-token FAIL leg did not fire this pass).
- `bash scripts/check-site.sh --profile internal` — **15/15 legs OK**;
  `--profile public` — **15/15 legs OK** (site layer byte-unchanged; the
  typed absence lives in the living README, outside the scanned surface).
- `bash scripts/check-living-docs.sh` — OK, 595 living files scanned.
- `bash scripts/check-position.sh` — **14/14 legs OK** (working tree).
- `bash scripts/check-examples.sh` — OK (4 cards, 6 pins recomputed, 29
  runner check ids, displayed block equivalent).
- `bash scripts/check-trace.sh` — OK, all legs green.
- `bash scripts/check-contributing.sh` — OK, 12 check ids green.
- `bash scripts/check-bm-tools.sh` — OK.
- `bash scripts/deferrals-open.sh` — 8 open rows re-verified post-write
  (unchanged — a docs leg opens/closes none).
- `git diff` reviewed on every shared-register write (BL-41); python writes
  for LEDGER/BUILD_INDEX; `git show --stat` covers every claimed file.
- Not run: cargo suites / emitters (records-and-docs ticket — no code or
  emitted artifact changed); external `check-build-memory.sh`
  (`AGENT_SKILLS_ROOT` run recorded where executed); `--head` variants of
  the doc-surface checkers recorded post-commit.

## Deferrals

- Opened: none. Closed: none (a docs leg — no owed row unblocked; DF-R7.3-2's
  named unblocker HUMAN-H21 landed at 334 but its owner's disposition is
  CAP7.2's, which already carried it OPEN). Carried OPEN — the true owed set
  verbatim: DF-R7.1-2, DF-R7.3-1/-2/-4, DF-R7.5-1/-2, DF-CAP7.1-1/-2.

## Residuals (verbatim, carried)

- The publication pin itself — a future `hh-site-publication/1` mint is the
  gated act that pins the packet's input set and lands phase-4 second-pass on
  the site proper.
- `pending{clock}` — the P4-C ≥24 h provider confirming read (due ≥2026-11-27
  ~07:26Z) — a confirming check, not a routed-id dependency.
- `compute-open.sh` `p4 | p4c` — receipt-less-teardown posture (operator-side
  delete verified zero billable resources; no `hh-compute-receipt/1` minted).
- Terminal guard leg on `nextTicket: DONE` — the recorded 2026-11-05
  retained-guard precedent (terminal-state refusal = correct posture).

## Deviations / notes

- LEDGER/BUILD_INDEX written by python per BL-41; `git diff` verified before
  commit.
- No production mutations, no live leg, no spend: USD $0.00; phase cumulative
  $68.828421 unchanged.
