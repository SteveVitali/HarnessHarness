# Security Policy

## Supported versions

HarnessHarness is pre-1.0. Security fixes are supported for:

| Version | Supported |
|---|---|
| Tip of the default branch | Yes |
| Latest tagged release | Yes |
| Older releases | No |

See [GitHub Releases](https://github.com/SteveVitali/HarnessHarness/releases)
for available releases and [project status](docs/status.md) for capability and
verification limits.

## Reporting a vulnerability

**Report privately; do not open a public issue.** Use
[GitHub's private vulnerability reporting form](https://github.com/SteveVitali/HarnessHarness/security/advisories/new).

Include the affected commit or version, component (crate, binary, or build
tooling), a reproduction, and the impact you see. Do not include real credentials
or sensitive production data. If you receive no acknowledgement within **14 days**,
reply on the advisory thread.

## Response targets

- **Acknowledge:** within 7 days of the report.
- **Triage and severity:** within 30 days, with a fix or a mitigation plan.
- **Disclosure:** coordinated with the reporter; a fix lands on the default
  branch before any public advisory is published.

## Scope

Security reports may cover the runtime, sandbox/helper boundary, authorization
and credential handling, network integrations, plugins, or build and release
tooling. The fixture-based development workflow does not imply a network-free
runtime: configured model providers and integrations can make network requests,
and the kernel has an HTTP binding.

TLS implementation lives in `hharness-tls` under
[ADR-0365](docs/adr/ADR-0365-r4.5c-rustls-dependency-admission-the-hh-tls-home-and-the-closed-https-mtls-scope.md).
The admitted `ring` use in `hharness-secrets` covers the DPoP verifier and blob-pool
AEAD. Secrets must not enter run ledgers or logs; a path that exposes one is a
defect worth reporting.

[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) lists admitted third-party
components and their licenses. Report vulnerabilities involving those components
through the same private channel.
