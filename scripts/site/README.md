# scripts/site — results-site layer (R6.4 / F2-a)

The "Instrument — the calibrated readout" design system and static shell
(spec §9.1/§9.2; design note `39_results-site` §Design overview, F5, F13).
Python 3 standard library only; no network; nothing under `crates/` reads
this tree.

## Layout

- `assets/` — `tokens.css` (both themes, all token roles, 8-slot series
  palette), `site.css` (type/space scales, card/chip/table specs, the six
  signature elements), `site.js` (theme toggle only), `glyphs.svg`
  (8-glyph disposition sprite + the double-frame glyph), `favicon.svg`.
- `hhsite/` — the component layer: `tokparse`, `contrast` (WCAG 2
  formula), `raster` (stdlib `zlib`/`struct` PNG writer + glyph
  rasterizer), `components` (stamps, needle readout, bands, receipt
  rail, calibration block, skeleton + guards), `pages` (shell + URL map).
- `config/harnesses.json` — two-character monograms + display names
  (admission-record order; no measurement values).
- `fixtures/phase1.json` — the phase-1 fixture (labelled `fixture`;
  internal profile only).
- `tests/` — `test_tokens.py` (committed ratio test; `--selftest` is
  mutation (m)) and `test_site.py` (component guards + golden).
- `golden/` — `all-absent.manifest` (per-file sha256 + tree sha256 of the
  all-absent build) and `token-ratios.txt` (the committed
  palette/contrast validator record).

## Commands

```sh
python3 scripts/site/build_mock.py --out <dir>                  # internal mock
python3 scripts/site/build_mock.py --out <dir> --profile all-absent \
    --manifest scripts/site/golden/all-absent.manifest          # regen golden
python3 scripts/site/tests/test_tokens.py                       # ratio test
python3 scripts/site/tests/test_tokens.py --selftest            # mutation (m)
python3 scripts/site/tests/test_site.py                         # component battery
```

Two builds of one profile produce an identical tree sha256. The output is
static and relative-linked — open `<dir>/index.html` directly; `#theme=dark`
or `#theme=light` pins a theme for the mock-review viewport matrix.

## Seam

R6.5's generator calls these templates (one renderer — CC1); it adds
`inputs.json`, the `hh-site-data/1` projection and derivations D-1…D-8.
R6.6 adds the `check-site.sh` legs whose component-level predicates this
row proves: mutations (t), (p), (r) and the `no-third-party` leg are
*owed to R6.6*, not passed here.
