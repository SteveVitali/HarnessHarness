"""hhsite — the HarnessHarness results-site component and render layer.

"Instrument — the calibrated readout" identity (design note
39_results-site; experiments-spec §9.1/§9.2). Python 3 standard library
only (REQ-XF-117). This package is the seam R6.5's generator consumes:
it owns templates, components and the deterministic rasterizer; the
generator owns inputs, site-data projection and derivations D-1…D-8
(one renderer — CC1).

The package never reads the repository's documents or any packet itself;
callers pass data in.
"""

GENERATOR_ID = "hh-site-mock/1"
SITE_VERSION = "s1-mock"
