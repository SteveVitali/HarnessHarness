# scripts/site — results-site layer (R6.4 / F2-a; R6.5 / F2-b+F2-e; R6.6 / F2-f+F2-g)

The "Instrument — the calibrated readout" design system and static shell
(spec §9.1/§9.2; design note `39_results-site` §Design overview, F5, F13),
plus the R6.5 generator: it reads only the packets named in
`config/inputs.json`, projects them into the generated, committed
`hh-site-data/1` layer at `docs/results/site-data/` (integer units,
per-value provenance, derivations D-1…D-8 only), and renders the phase-1
and always-present pages through the same shell/components (one
renderer — CC1). Python 3 standard library only (declared floor 3.9);
no network; nothing under `crates/` reads this tree.

R6.6 adds the two render profiles (design/39 F10) and the publication
layer: `--profile internal` renders every phase its inputs carry plus
the designed absent states; `--profile public` renders a phase only
where a committed `hh-site-publication/1` record under
`docs/results/publication/` pins that phase's exact input set — an
input-set mismatch refuses the build — and refuses fixture labels,
preview banners, private dataset paths and host paths in rendered pages
(REQ-XF-128/129/142). `hhsite/publication.py` is the record validator;
`scripts/check-publication.sh` runs it plus the manifest-row checks;
`scripts/check-site.sh` runs the fourteen §9.4 legs;
`scripts/ci/test-check-site.sh` is its mutation battery.

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
- `config/origins.json` — `hh-site-origins/1`: the declared origin set
  (`asset_origins`, `dataset_origins`, `dataset_hosts`) the
  `no-third-party` leg enforces; empty while the dataset stays private.
- `fixtures/phase1.json` — the phase-1 fixture (labelled `fixture`;
  internal profile only).
- `tests/` — `test_tokens.py` (committed ratio test; `--selftest` is
  mutation (m)) and `test_site.py` (component guards + golden).
- `golden/` — `all-absent.manifest` (per-file sha256 + tree sha256 of the
  all-absent build) and `token-ratios.txt` (the committed
  palette/contrast validator record).

## Commands

```sh
python3 scripts/site/build.py --emit          # write docs/results/site-data/ (hh-site-data/1)
python3 scripts/site/build.py --check         # drift leg: rebuild byte-equals committed
python3 scripts/site/build.py --out <dir>     # render the internal profile
python3 scripts/site/build.py --out <dir> --profile public   # the deployable profile
bash scripts/check-site.sh                    # the fourteen §9.4 legs (internal)
bash scripts/check-site.sh --profile public   # the same legs on the deployable tree
bash scripts/check-publication.sh             # hh-site-publication/1 + manifest rows
python3 scripts/site/build_mock.py --out <dir>                  # internal mock
python3 scripts/site/build_mock.py --out <dir> --profile all-absent \
    --manifest scripts/site/golden/all-absent.manifest          # regen golden
python3 scripts/site/tests/test_tokens.py                       # ratio test
python3 scripts/site/tests/test_tokens.py --selftest            # mutation (m)
python3 scripts/site/tests/test_site.py                         # component battery
python3 scripts/site/tests/test_sitedata.py                     # generator + contract battery
python3 scripts/site/tests/test_sitepages.py                    # page-level battery
```

Two builds of one profile produce an identical tree sha256. The output is
static and relative-linked — open `<dir>/index.html` directly; `#theme=dark`
or `#theme=light` pins a theme for the mock-review viewport matrix.

## Seam

R6.5's generator calls these templates (one renderer — CC1); it adds
`inputs.json`, the `hh-site-data/1` projection and derivations D-1…D-8.
R6.6 landed the fourteen `check-site.sh` legs, their mutation battery
(a)–(v), the two render profiles, `hh-site-publication/1` and
`pages.yml` (ADR-0424) — the predicates this row once owed are now
*checked by R6.6's legs*, not deferred further.
