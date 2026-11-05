# DOC5.1 — READER_DOCS_CHECK (round 5)

> The DOC.1 record: reader-facing documentation refreshed **from the
> accepted round-5 evidence** — the GATE-G17 accepted record (readouts/
> GATE-G17.md, PASSED 2026-11-05T00:13:07Z) and the committed
> disposition packets it folds. Executed things are documented as
> executed, withheld/unmet things as withheld/unmet; nothing here is a
> claim beyond what the records support. Offline-only; USD $0.

- **Ticket:** `docs/tickets/245_DOC5.1__doc1.md` (seq 245 of 246)
- **Base:** `42e169d1` (codex/harnessharness-evidence-rec5.3 tip)
- **Run ledger:** `docs/build/runs/DOC5.1.md`
- **Mode:** living-docs refresh; frozen/historical/append-only
  surfaces report-only (no edits to spec/, research/, docs/tickets/,
  docs/adr/, docs/build/ records, or the frozen founding docs).

## Accepted record applied (verified against committed bytes)

GATE-G17 layer verdicts — engineering `executed` · research
`inconclusive` · portability/public/adoption `withheld` ·
`build_status: not_done` (eng closed 107 / req satisfied 78).
Dispositions read from the packets themselves (not from summaries):

| packet | disposition | reader-doc consequence |
|---|---|---|
| `2026-11-04-r5.43-s1-useful.json` (`hh-s1-useful/1`) | `failed` | useful-task card: attempted and failed; machinery repaired at R5.44, no re-execution admitted; DF-CAP5.1-1 open |
| `2026-11-04-r5.44-ops-live-second.json` (`hh-ops-live/1`) | `executed` | operations card: 24/24 attempts invoked/boundary-observed, 0 violations; bounded podman execution, not `live-executed` |
| `2026-11-04-r5.46-activation-podman.json` / `…-pilot-podman.json` | `executed` | study card: mechanism legs executed (10/10 pilot first-subjects, $0 exposure) |
| `2026-11-04-r5.47-parity-n0.json` | `withheld` | study card: parity `withheld{n_declared:0}`, s3_unmet |
| `2026-11-04-r5.48-confirm-withheld.json` | `withheld` | operations card: confirmation `withheld{tranche_absent}`, s4_unmet |
| `2026-11-04-r5.49-study-report.json` | `reported` | study card: `unmet{efficacy_unavailable;parity_withheld}`, s1/s3/s4 unmet verbatim |
| `2026-11-04-r5.50-plugin-unselected.json` | `unselected` | component card + CONTRIBUTING gated lane: `unselected{stage_voided}` |
| `2026-11-04-r5.51-interchange-unselected.json` | `unselected` | interchange card: `unselected{stage_voided}` |
| `2026-11-04-r5.52-outside-withheld.json` | `withheld` | evidence-access card: `withheld{stage_voided}`; DF-H9-1 open |
| `2026-11-04-r5.53-formative-withheld.json` / `…-r5.54-publication-…` / `…-r5.55-value-…` | `withheld` | partial-release note + status note: participant/publication/value stages withheld |

## Stale claims found and corrected

| surface | stale claim | refreshed to |
|---|---|---|
| `README.md` spine | "two versioned example cards"; useful-task bullet silent on the attempt | four cards (EX-01–EX-04, all labelled fixtures); useful-task bullet names the recorded failed bounded attempt |
| `docs/getting-started.md` | "Two committed `example-card/1` records" | four records with the four families; executed packets retain honest dispositions |
| `docs/evidence.md` useful-task card | "None of that has been executed … R5.43 skeleton waits on GATE-G11" | the `hh-s1-useful/1` packet ran and recorded `failed`; residual carried by DF-CAP5.1-1; state stays `Not yet available` |
| `docs/evidence.md` study card | "planned; collection not ready … no execution has occurred" | `hh-study-report/1` `reported` — `unmet{efficacy_unavailable;parity_withheld}`; bounded legs executed; parity/confirmation withheld; state stays `Not yet available` |
| `docs/evidence.md` operations card | "engineered apparatus only … 24-trial execution owed" | `hh-ops-live/1` `executed` (24/24, 0 violations); confirmation withheld → S4 unmet; state stays `Not yet available` |
| `docs/evidence.md` component / interchange / evidence-access cards | pre-recorded dispositions | `unselected{stage_voided}` / `withheld{stage_voided}` records cited verbatim; states stay `Restricted` |
| `docs/evidence.md` scenarios table + partial-release note | pre-execution planning language | accepted dispositions + GATE-G17 verdicts with the three open deferrals named |
| `docs/status.md` | no round-5 account | dated 2026-11-05 round-5 acceptance section |
| `docs/experiments.md` EX-03 | "the executed hh-s1-useful/1 packet" | the retained packet `disposition: failed`, kept verbatim |
| `CONTRIBUTING.md` gated routes | component "stays gated until its checked recipe lands" | gated + recorded unselected (stage voided); reopens only under a fresh named selection |

## Checks executed (working tree, actual counts)

- `bash scripts/check-living-docs.sh` → **OK** — 456 living files, no personal paths.
- `bash scripts/check-position.sh` → **OK** — 13 legs green (13 cards, closed states held, S1/S3/S4 unmet preserved, honesty leg clean, links resolve).
- `bash scripts/check-examples.sh` → **OK** — 4 cards, 6 pins recomputed, 29 runner check ids, displayed block equivalent.
- `bash scripts/check-trace.sh` → **OK** — excerpts/packets/families/honesty/links/nav green.
- `bash scripts/check-contributing.sh` → **OK** — 12 check ids green (lanes, gated routes, links).
- `bash scripts/check-bm-tools.sh` → **OK**.
- `cargo fmt --all --check` → **clean** (docs-only; no Rust touched).
- Documented commands executed on scratch stores (`HH_STORE_ROOT`/`HH_WORKSPACE_ROOT` under `mktemp -d`): `cargo test -p hharness --test r5_29 ex01` **1/1** · `ex02` **5/5** · `--test r5_45 ex03` **3/3** · `ex04` **2/2**.
- Supplemental guard `bash docs/build/tools/check-supplement-guard.sh` → **PASS, route DISPATCH — DOC5.1** (82 rows / 180 candidates / 0 violations / 8 baseline warnings), pre- and post-artifacts.

## Committed-HEAD scope (explicit)

`check-examples.sh --head`, `check-trace.sh --head`, `check-contributing.sh --head`,
`check-position.sh --head` and `check-quickstart.sh` read **committed
HEAD** in a clean clone — they validate the committed tree, never the
uncommitted working tree. Their `--head`/clean-clone results for this
change are recorded in the run ledger's evidence log after the change
commit; a working-tree pass does not imply the HEAD variants passed.

## What this refresh does not claim

- No card state was promoted: useful-task/study/operations stay
  `Not yet available`; component/interchange/evidence-access stay
  `Restricted` — matching GATE-G17's accepted `not_done` record.
- No `live-executed`, `staging-verified` or `public` claim is made for
  the bounded podman packets; they are executions, recorded honestly.
- The failed S1 packet, the withheld parity/confirmation/outside/
  formative/publication/value records, and the unselected component/
  interchange records are **retained verbatim**, never promoted.
- Historical negative results (the Anthropic replay failure, the
  quickstart-live `format_failure`, the first ops-live packet's
  `failed` disposition) stay on the record.
- The ordinary contributor path is unchanged — no build-ticket ritual
  is imposed on contributors (check-contributing `no_ritual` leg).
