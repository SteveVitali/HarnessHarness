# GATE-G21 — Site identity mock review: approve "Instrument" or redirect · readout
Status: PASSED

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion (verbatim)

§10.3, "Gates and HUMAN rows" table:

> | Site identity mock review | first | F2-a | the rendered mock in both themes at the four viewports; `test_tokens.py` record; approve "Instrument" or redirect (Q-X43) |

§9.1, "Mock-review gate":

> Whether the result is beautiful is unestablished until the operator views a rendered mock at the mock-review gate after F2-a, in both themes at F2 F13's viewports (Q-X43 identity). Redirection toward F1's "Ledger" or "Title block" directions remains open there.

REQ-XF-101 (last sentence): "Approval is reserved to the operator at the mock-review gate (Q-X43)." Q-X43 (Appendix B): "pending — default: identity at the mock gate; …".

## Pre-registered thresholds

**Technical prerequisites (blockedOn if unmet):** R6.4 landed (pending #297 merge at presentation); `test_tokens.py` passes (every text/status token ≥ 4.5:1, non-text ≥ 3:1, panel and paper, both themes — REQ-XF-102); two mock builds give the same tree sha256.

**What is shown:** (1) the mock tree built at the R6.4 merge-candidate commit, tree sha256 as recorded; (2) home page on phase-1 fixtures, the fixture specimen page (every REQ-XF-101 element), and the all-absent site; (3) both themes + toggle + no-JS fallback; (4) viewports 1440×900, 1280×800, 390×844, 320×640; (5) glyph favicon.svg/png + OG + wordmark + monograms; (6) test_tokens.py ratio record + palette validator; (7) the REQ-XF-107 originality check — for the operator's judgement.

## Evidence (per item: source, `date -u` of the read, layer reached)

- Mock build (internal profile) — `python3 scripts/site/build_mock.py --out /tmp/hh-mock-a` at `0d7aebbe` — 20 pages, two builds → tree sha256 `ca66c6994057b03beec69f0d2b65c7bb6aa4a841aed1923a73e197bbc266b312` — read 2026-11-07T09:45Z — layer: engineered
- Mock build (all-absent profile) — `--profile all-absent` — 19 pages, tree sha256 `472ae59f25ab905e472bcd0a1b1ac25b59340bde1ec39743b3c3944d62c085ef` — read 2026-11-07T09:45Z — layer: engineered
- Token validator — `python3 scripts/site/tests/test_tokens.py` → `OK — 40 token×surface pairs, all above floor` — read 2026-11-07T09:45Z — layer: engineered
- Site checks — `python3 scripts/site/tests/test_site.py` → `OK` (golden equality, mutation refs, no digits in skeletons, no font requests) — read 2026-11-07T09:45Z — layer: engineered
- Serving — `python3 -m http.server 8765` over the internal tree; browser preview open — read 2026-11-07T09:45Z — layer: engineered
- R6.4 run record — `docs/build/runs/R6.4.md` (packet + tree sha256s agree verbatim) — read 2026-11-07T09:45Z — layer: engineered
- Originality reference — `docs/research/29_reference-sites.md` F4/F7 (HF, Artificial Analysis) — read 2026-11-07T09:45Z — layer: engineered

<!-- agent-drafted:begin sha256=5aa64df94dafbd4bafbace6a0d2dbc7d419e71adf209ba7443fa19bfe303fc24 -->
The presented mock (identical bytes in both themes across all four viewports — served at http://127.0.0.1:8765 via the browser preview):

- Internal profile (phase-1 fixtures): 20 pages, two builds one tree sha256 `ca66c6994057b03beec69f0d2b65c7bb6aa4a841aed1923a73e197bbc266b312`, built at merge-candidate commit `0d7aebbe` with `python3 scripts/site/build_mock.py --out /tmp/hh-mock-a`.
- All-absent profile: 19 pages, tree sha256 `472ae59f25ab905e472bcd0a1b1ac25b59340bde1ec39743b3c3944d62c085ef` — every §9.2 path incl. `/method/` renders its designed absent state (typed reason, awaited schema, gate id; no digits).
- Specimen page: every REQ-XF-101 element — needle readouts, noise-floor band, all eight disposition stamps (pinned light/dark), receipt rail, calibration block, graph-paper ground; type/space scales, card/chip/table specs; the 8-slot series palette in adjacent-pair form.
- Glyph: favicon.svg verbatim (double frame + needle) + stdlib-rasterized favicon.png + 1200x630 og.png, byte-identical; 8 harness monograms (harnesses.json).
- `test_tokens.py` record: OK — 40 token×surface pairs, all above floor (text ≥4.5:1, non-text ≥3:1, both themes); palette ratios equal design/39 pair-for-pair; `test_site.py` OK incl. golden equality, no digits in skeletons, no font requests.
- Originality check (REQ-XF-107): tokens, typefaces and motifs set against HF house style + Artificial Analysis assets per `docs/research/29_reference-sites.md` F4/F7 — Instrument's needle readout + noise-floor band + receipt rail read as a lab instrument, not a product landing page; operator judgement is the gate.

Q-X43 question for the operator: approve "Instrument — the calibrated readout" as the site identity, or redirect (toward F1 I-A "Ledger" / I-C "Title block", or a named revision).
<!-- agent-drafted:end -->

## Signature
Operator decision (verbatim, received 2026-11-07T09:20Z via interactive operator chat): "approve and proceed"
GATE DECISIONS row: 2026-11-07T09:20Z | GATE-G21
Operator confirmation of agent-drafted text (verbatim, 2026-11-07T09:20Z): "approve and proceed" — covers sha256:5aa64df94daf
Signed by: the operator. Recorded by devin-desktop/swe-2-high, which does not sign.
