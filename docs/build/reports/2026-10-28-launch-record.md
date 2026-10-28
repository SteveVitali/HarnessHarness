# Launch record — HarnessHarness v0.1.0 (R4.18, 2026-10-28)

The record of the public launch the operator performed at **HUMAN-H7**
(`docs/build/readouts/HUMAN-H7.md`) under the operator's recorded
authorization (verbatim: "you can then execute the remaining steps
through 9. as well. You are authorized to do all these steps"; publish
legs: "ok, saved login token. proceed" / "email confirmed. proceed").
Every fact below was re-verified by this ticket's worker at run time on
2026-10-28 (UTC) — nothing is carried on trust.

## Merged-main SHA and its green run

- **Merged-main SHA (as tagged):** `d1325a63bab7bf0836250d9df871e2bfdd76dea2`
  — the PR #203 merge commit (the dependabot tail of the bottom-up chain
  merge, #192–#203).
- **CI run on that SHA:** `40525512308` (event `push`, branch `main`) —
  **success** on all four required jobs, verified via
  `gh run view 40525512308`:
  - `build · test · fmt · drift (ubuntu-latest)` — success
  - `build · test · fmt · drift (macos-latest)` — success
  - `cargo-deny (licenses · advisories · bans · sources)` — success
  - `gitleaks (full history + PR diff)` — success (no non-allowlisted
    finding on `main` history — no rotation or rewrite needed)
- `main` has since advanced past the tag: `eb7611d8` (#205 crate
  descriptions), `7d95a381` (#206 public-docs refresh), `66b8b3a2`
  (#207 HUMAN-H7 closeout records). The tag stays on the recorded SHA —
  that is what a release tag is for.

## Tag proof — `v0.1.0` → the recorded SHA

Verified via `git ls-remote origin` + `gh api .../git/refs/tags/v0.1.0`
on 2026-10-28:

| Ref | Object |
|---|---|
| `refs/tags/v0.1.0` | `93d8f79ceeee39b19158bac0829d83cc1870d813` (annotated tag object) |
| `refs/tags/v0.1.0^{}` (peeled commit) | `d1325a63bab7bf0836250d9df871e2bfdd76dea2` |

The annotated tag's commit equals the merged-main SHA that ran green —
**the proof the contract asks for holds**.

## GitHub Release

- **URL (live, not draft, not prerelease):**
  `https://github.com/SteveVitali/HarnessHarness/releases/tag/v0.1.0`
- **Release created:** 2026-10-27T15:06:43Z · **tag:** `v0.1.0` ·
  **target:** `main`
- **Assets (5):** the four ADR-0380 target tarballs + `SHA256SUMS`
  (444 B). `SHA256SUMS` downloaded at run time; verbatim content:

```
10eebfdb3ec2b3375ee46c99d687573a1f5e6bfaa0da2b1a76b8cb4996743d2b  hharness-0.1.0-aarch64-apple-darwin.tar.gz
4cde4a417951e5df89cb85d0bf7675797642fb8ef54cacf4bd1849770328417f  hharness-0.1.0-aarch64-unknown-linux-gnu.tar.gz
b3bae16b2f675589bec442ceabcfedde93347684ca8143bc020201db4c307394  hharness-0.1.0-x86_64-apple-darwin.tar.gz
bb11abdd839eced525aa2d9441ef4030dc362bc07189a4b3374e3c706934ed87  hharness-0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

| Asset | Bytes |
|---|---|
| `hharness-0.1.0-aarch64-apple-darwin.tar.gz` | 12,380,500 |
| `hharness-0.1.0-aarch64-unknown-linux-gnu.tar.gz` | 12,948,836 |
| `hharness-0.1.0-x86_64-apple-darwin.tar.gz` | 13,588,474 |
| `hharness-0.1.0-x86_64-unknown-linux-gnu.tar.gz` | 14,840,265 |

## crates.io — 49/49 publishable crates at 0.1.0

Each name re-verified at run time (2026-10-28T02:1xZ) via
`GET https://crates.io/api/v1/crates/<name>` — response `200` and
`versions[]` contains `0.1.0`. One request per crate, User-Agent header
set per crates.io policy, ~1 s spacing. Result: **49 verified, 0
missing**. The list is `scripts/publish-order.sh --check-only`'s publish
set (its topological order shown — the order the operator published in):

| # | Crate | Version | # | Crate | Version |
|---|---|---|---|---|---|
| 1 | hharness-tls | 0.1.0 | 26 | hharness-eval | 0.1.0 |
| 2 | hharness-wire | 0.1.0 | 27 | hharness-bench | 0.1.0 |
| 3 | hharness-embed-client-generated | 0.1.0 | 28 | hharness-experiment | 0.1.0 |
| 4 | hharness-provenance | 0.1.0 | 29 | hharness-evolution | 0.1.0 |
| 5 | hharness-identity | 0.1.0 | 30 | hharness-telemetry | 0.1.0 |
| 6 | hharness-embed-schema | 0.1.0 | 31 | hharness-bundle | 0.1.0 |
| 7 | hharness-codegen | 0.1.0 | 32 | hharness-context | 0.1.0 |
| 8 | hharness-ontology | 0.1.0 | 33 | hharness-env | 0.1.0 |
| 9 | hharness-hir | 0.1.0 | 34 | hharness-gateway | 0.1.0 |
| 10 | hharness-ledger | 0.1.0 | 35 | hharness-control | 0.1.0 |
| 11 | hharness-budget | 0.1.0 | 36 | hharness-mcp | 0.1.0 |
| 12 | hharness-hosting | 0.1.0 | 37 | hharness-acp | 0.1.0 |
| 13 | hharness-plugin | 0.1.0 | 38 | hharness-cli | 0.1.0 |
| 14 | hharness-registry | 0.1.0 | 39 | hharness-results | 0.1.0 |
| 15 | hharness-assembly | 0.1.0 | 40 | hharness-analysis | 0.1.0 |
| 16 | hharness-compiler | 0.1.0 | 41 | hharness-embed | 0.1.0 |
| 17 | hharness-lab | 0.1.0 | 42 | hharness-kernel | 0.1.0 |
| 18 | hharness-debt | 0.1.0 | 43 | hharness-mcp-lab | 0.1.0 |
| 19 | hharness-monitor | 0.1.0 | 44 | hharness-subagent | 0.1.0 |
| 20 | hharness-containment | 0.1.0 | 45 | hharness-orchestrator | 0.1.0 |
| 21 | hharness-fleet | 0.1.0 | 46 | hharness-varhost | 0.1.0 |
| 22 | hharness-fleet-adapter | 0.1.0 | 47 | hharness-web | 0.1.0 |
| 23 | hharness-helper | 0.1.0 | 48 | hharness-xcheck | 0.1.0 |
| 24 | hharness-secrets | 0.1.0 | 49 | hharness | 0.1.0 |
| 25 | hharness-verification | 0.1.0 | | | |

**The five `publish = false` crates are correctly absent** — each
returns HTTP 404 from the same API at run time (verified 2026-10-28):
`hharness-compact-evict-oldest`, `hharness-github-issues`,
`hharness-memory-store`, `hharness-plugin-fixture`,
`hharness-tracker-fixture` (the packaged-plugin variants — dev/fixture
crates, never meant for the registry).

## `main` protection — read via `gh api` at run time

- **Ruleset `23836544` ("main"):** `enforcement: active`, `target:
  branch`, rules:
  - `deletion` — active
  - `non_fast_forward` — active
  - `required_status_checks` — `strict_required_status_checks_policy:
    false` (non-strict — stacked heads did not need rebasing), required
    contexts:
    - `build · test · fmt · drift (ubuntu-latest)`
    - `build · test · fmt · drift (macos-latest)`
    - `cargo-deny (licenses · advisories · bans · sources)`
    - `gitleaks (full history + PR diff)`
  Verified via `gh api
  repos/SteveVitali/HarnessHarness/rulesets/23836544` on 2026-10-28.
- **Environment `live-verify`:** `required_reviewers` protection active
  (reviewer `SteveVitali`); secrets present **by name only** —
  `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` (read via
  `gh api .../environments/live-verify/secrets`; values never read,
  never recorded).
- **Private vulnerability reporting:** `enabled: true`
  (`gh api .../private-vulnerability-reporting`).
- **Repository:** `visibility: PUBLIC`; description: "Research
  instrument and reference runtime that makes the agent harness itself a
  decomposable, comparable experimental variable"; topics:
  `agent-evaluation`, `agent-harness`, `harness-engineering`, `rust`,
  `sandboxing`; homepage set to the repository URL.

## Incident log — recorded honestly

1. **Release-workflow defect (fixed by PR #204).** The tag-triggered
   `release.yml` run `40527732513` on `d1325a63` built and
   checksum-verified all four target archives — `dist
   x86_64-unknown-linux-gnu`, `dist aarch64-apple-darwin`, `dist
   x86_64-apple-darwin`, `dist aarch64-unknown-linux-gnu` all
   **success** — then the `GitHub Release (tag only)` job **failed**:
   `gh release create --verify-tag` ran without a checkout (`fatal: not
   a git repository`). The release was created from that run's verified
   artifacts by hand under the operator's delegation; PR #204 added the
   missing `actions/checkout` so future tags release without manual
   repair.
2. **crates.io description-length limit (fixed by PR #205).** Six
   crates' `description` fields exceeded crates.io's 1000-char server
   limit — a gap cargo's local `publish --dry-run` verify does not
   catch. Descriptions trimmed under the limit and those crates
   published; all 49 verified at 0.1.0 above.
3. **New-crate rate limit.** crates.io throttles new-crate publishes to
   ~1 crate per ~10 min after an initial ~5-crate burst; the publish ran
   as a wait-and-retry loop in topological order — total wall-clock
   ~5.5 h for 49 crates.
4. **Transient crates.io 5xx** — one transient server-side outage mid
   loop; the affected publish was retried and landed.
5. **Stale local build cache.** An L4.5-era stale `tmp-registry` rmeta
   (pre-L4.6b trait) forced a `cargo clean` before the loop started —
   nothing had uploaded yet; no partial-state cleanup needed.
6. **Verified-email requirement.** The crates.io account needed a
   verified email before first publish — the operator completed it
   ("email confirmed. proceed").

## Spend note

This ticket spent **USD 0** (records/reconcile, `live_verification=false`,
offline — the crates.io/GitHub reads above are public API reads, not
model calls). The round-4 live-evidence total stands at **USD 25.2598 of
the USD 100 cap** across 34 committed `hh-live-evidence/1` records —
`scripts/check-live-evidence.sh` green at run time (2026-10-28).

## What this record does *not* claim

- No claim that the release workflow ran end-to-end — it did not (item 1
  above); the release exists and its artifacts are the CI-verified ones.
- No claim beyond the verified rows: the `public` layer is earned for the
  published crate set, the tagged release, protected `main`, PVR, and
  the `live-verify` environment — and only for those. Everything else in
  `OPERATIONAL_READINESS.md` keeps its prior highest layer.
