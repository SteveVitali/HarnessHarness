## Summary

<!-- What changed and why. Name the ticket/spec row it implements. -->

## Verification

<!-- Commands run + headline results. Cite the numbers. -->

- [ ] `cargo fmt --all --check` clean
- [ ] Scoped `cargo test -p hh-<crate>` green (or `--workspace` if cross-cutting)
- [ ] `bash scripts/check-drift.sh` in sync (if the schema source changed)
- [ ] `bash scripts/check-licenses.sh` + `scripts/check-living-docs.sh` green
- [ ] New behaviour has a test that fails if it is removed

## Chain notes

<!-- Stacked PRs: base branch this PR targets. Deferrals opened/closed
     (docs/tickets/DEFERRALS.md rows). Requirement ids stamped
     (R-*/BL-*/OQ-*). Any MET-DIFFERENTLY or size deviation recorded. -->

## Licensing

<!-- By submitting this PR you agree the contribution is licensed
     MIT OR Apache-2.0 — see CONTRIBUTING.md. -->
