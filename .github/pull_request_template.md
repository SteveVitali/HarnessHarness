## Summary

<!-- Explain the problem and resulting behavior. Link an issue or ticket when applicable. -->

## Verification

<!-- List commands run and headline results, including test counts. Mark checks
     that do not apply and explain any checks you could not run. -->

- [ ] `cargo fmt --all --check`
- [ ] Tests for affected packages (`cargo test -p hharness-<crate>`, or `--workspace`)
- [ ] New or changed behavior has a regression test
- [ ] Schema changes: `bash scripts/check-drift.sh`
- [ ] Metadata changes: `bash scripts/check-licenses.sh`
- [ ] Documentation changes: `bash scripts/check-living-docs.sh` and updated examples checked

## Build-ticket notes (if applicable)

<!-- For manifest-chain work: ticket and requirement IDs (R-*/BL-*/OQ-*),
     predecessor/base branch, deferrals opened or closed (DF-*), and recorded
     MET-DIFFERENTLY or size deviations. Omit this section for other PRs. -->

## Licensing

<!-- By submitting this PR you agree the contribution is licensed
     MIT OR Apache-2.0 — see CONTRIBUTING.md. -->
