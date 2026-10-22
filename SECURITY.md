# Security Policy

## Supported versions

HarnessHarness is pre-1.0 (workspace `0.0.1`; the first public release
`0.1.0` is pending publication). Until then:

| Version | Supported |
|---|---|
| tip of the default branch | yes |
| latest tagged release (when one exists) | yes |
| anything older | no |

## Reporting a vulnerability

**Report privately — never a public issue.** Use GitHub Security
Advisories:

<https://github.com/SteveVitali/MetaHarness/security/advisories/new>

Include the affected commit or version, the component (crate/binary or
build tooling), a reproduction, and the impact you see. If you receive
no acknowledgement within **14 days**, reply on the advisory thread.

## Response targets

- **Acknowledge:** within 7 days of the report.
- **Triage + severity:** within 30 days, with a fix or a mitigation plan.
- **Disclosure:** coordinated with the reporter; a fix lands on the
  default branch before any public advisory is published.

## Scope notes

The build is offline/hermetic by contract: the only admitted network +
TLS surface is `hh-tls`'s closed rustls scope (ADR-0365) and the
out-of-process `hh-github-issues` tracker variant; the `ring` pin
(ADR-0359) is signature-verification + AEAD only. Secrets never enter
ledgers or logs by contract — a path that leaks one is a defect worth
reporting. Vulnerabilities in third-party components are tracked in
`THIRD_PARTY_NOTICES.md`; report them the same way.
