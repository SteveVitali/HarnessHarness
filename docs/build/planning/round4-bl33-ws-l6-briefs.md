# WS-L6 briefs — BL-33 package (R4.8, 2026-10-22)

<!-- Brief-file convention OWNED BY R4.8 (157 Notes; manifest row): R4.10–R4.15 reuse
     this file's table columns, adopt-all-except line and the four disposition tokens verbatim.
     This ticket never rules — `recording-act` rows only cite an operator ruling already in
     LEDGER GATE DECISIONS or a landed ADR; everything else is a proposal for GATE-G7. -->

**Operator act: "adopt all except: <ids>"**

Disposition tokens (closed set): `resolve-with-evidence` (landed evidence answers it — cite tests/ADRs) ·
`adopt-interim-as-final` (the interim rule in force becomes the ratified answer — quoted) ·
`descope-by-ADR` (permanently out of the build — ADR text + authorising operator words named) ·
`needs-code-leg(<row>)` (post-GATE-G7 conform row + the exact change). A `recording-act` row is not a
proposal: the ruling already determined the cell and this ticket only records it — nothing to adopt.

## Batch-adoptable table

| id | proposed disposition | one-line basis | conform row |
|---|---|---|---|
| R-2.12.4 | recording-act (landed) | the eight launch rulings recorded verbatim in §ruled-postures; implementation legs routed to L4.1/L4.2/L4.4/L4.5, never pre-answered here | — (records only) |
| OQ-035 | recording-act (landed) | "Keep name, distinct crate prefix" + the dated collision re-check in §market-check + qualifier line = none; ADR-0011 amendment log appended | — (records only) |
| OQ-068 | needs-code-leg(R4.15a) | one allocation record: `<hh-prefix>` = `hharness` (collision-checked §market-check), `_hh/` methods, `hh://` scheme, the A2A extension URI | R4.15a |
| OQ-093 | needs-code-leg(R4.15a) | pricing governance rule: `hh/` publishes via the instrument release process; `max_age` default; superseded-`version_id` retirement test | R4.15a |
| OQ-162 | needs-code-leg(R4.15a) | custody rule: key-based `accepted_signers` + keyless trusted-publishing class; maintainer-held roots, two-key rotation bridge; registry self-signs `hh/` name-history | R4.15a |
| OQ-173 | needs-code-leg(R4.15a) | `RetentionPolicy` MUST-data: indefinite audit-envelope floor; `legal_hold` at authority ≥ principal blocks every gc path incl. lab-wide | R4.15a (+R4.15c for the gc enforcement leg) |
| OQ-218 | needs-code-leg(R4.15a) | carried keys `hharness/…` named in the OQ-068 allocation record | R4.15a |
| OQ-332 | needs-code-leg(R4.15d) | trust fields for `locations[]` remote fetch: three admission classes + expiry enforced + ledgered arrival + S3 re-verify — §trust-fields | R4.15d |
| OQ-383 | needs-code-leg(R4.15a) | executable name ruled ("binaries keep short names (hh, hh-kernel, hh-helper)" — recording-act); record flat Lab nouns + precedent-style flag spellings as MUST-data | R4.15a |
| OQ-384 | needs-code-leg(R4.15a) | ratify the landed `ExitClass::code()` table (0 + 64–73, BSD-sysexits band, avoids 1/2/126–165/255) as the MUST-data numeral table; success-with-veto: distinct code only on request | R4.15a |
| OQ-389 | needs-code-leg(R4.15a) | ratify the landed delivery set (ADR-0301 D8): owner-only mode-0600 token file primary, stderr fallback, never URL; per-launch mint = rotation; one token per surface | R4.15a |
| OQ-394 | needs-code-leg(R4.15a) | `hh://` + `_meta["hharness/account"]` allocated in the same record as OQ-068 | R4.15a |
| OQ-411 | needs-code-leg(R4.15a) | `ContractVersionPolicy` sunset MUST-data: ≥2 minor cycles per contract kind; shortening is a narrowing at authority ≥ definition under `authority_cap` | R4.15a |
| OQ-445 | needs-code-leg(R4.15a) | post-Phase-5 revalidation owner = the L4.2 maintainer policy (RK-06 cadence); `evidence_max_age` = one model generation; register-header MUST-data | R4.15a (+L4.2 for the policy doc) |

## Ruled postures — recording acts (verbatim GATE DECISIONS `### Round 4`, 2026-10-20)

Every row below is quoted verbatim from `docs/build/LEDGER.md` GATE DECISIONS `### Round 4`
(`answer (verbatim)` + `consequence` cells). These are recording acts — the ticket adds no ruling.

- **License (R-2.12.4 licensing leg):** `"MIT OR Apache-2.0"` — consequence: "dual license replaces
  Cargo.toml's Apache-2.0; ADR-0210's licensing leg (R-2.12.4) is ruled — the launch ticket lands
  LICENSE-MIT + LICENSE-APACHE + per-crate `license = "MIT OR Apache-2.0"` + third-party notices
  (ring/rustls)". Implementation → **L4.1** (164p).
- **Distribution (R-2.12.4 packaging leg):** `"Also publish to crates.io"` — consequence: "GitHub
  Release binaries + crates.io publication; crate metadata, publish ordering and dry-run land as
  tickets; the actual publish is an operator act at launch". Plus `"Libraries + installable bins"` —
  consequence: "publish = true only for reusable libraries + crates carrying installable binaries;
  fixtures/plugins/test-support crates renamed for consistency but `publish = false`".
  Implementation → **L4.4** (rename, 164s) + **L4.5** (publish readiness/dry-run, 164t).
- **Naming — project name:** `"Keep name, distinct crate prefix"` — consequence: "project/repo stay
  HarnessHarness; same-domain crates.io collision (`harnessharness`, rellfy) recorded; published
  crates take a distinct uniform prefix (prefix choice pending — see next row)".
- **Naming — crate names:** `"New uniform prefix"` — consequence: "every published crate renames to
  one available prefix (`hh-cli`/`hh` are taken on crates.io); binaries may keep short names; the
  prefix itself is pending the operator's pick".
- **Naming — crate naming scheme:** `"Yes — adopt it"` to: umbrella crate `hharness`, components
  `hharness-<component>` (hh-wire → hharness-wire, …), binaries keep short names (hh, hh-kernel,
  hh-helper) — consequence: "the uniform prefix pending in the 'crate names' row above is `hharness`;
  a rename ticket lands package/lib renames workspace-wide before the publish dry-run; binary names
  unchanged". Implementation → **L4.4**.
- **Provenance (public build records):** `"Keep provenance, fix living docs"` — consequence:
  "docs/build + research stay as append-only provenance; personal paths scrubbed from living
  human-facing docs only; README explains the build record". Implementation → **L4.1**
  (living-doc scrub leg).
- **Standing rows (OSS governance leg):** `"Convert to maintainer policy"` — consequence: "the
  standing write-integrity rules and the ADR revisit-trigger catalogues land as maintainer policy
  docs; BL-41/42/43 close as converted-to-policy — no standing debt rows at launch".
  Implementation → **L4.2** (164q).

**Recording-act closures landed by this ticket** (the rulings fully determine the cell):
R-2.12.4's licensing/naming/distribution/provenance/standing-rows content is **ruled** (rows above) —
recorded in `research/registers/scope.md`'s 2026-10-22 log row + the `COVERAGE_MATRIX.csv` note cell
(status stays `deferred(ADR-0210)` / MET-DIFFERENTLY — re-entry as `specified` is a scope ADR after
the L4.x legs land, per ADR-0210 D3). **OQ-035's WS-L6 follow-up** is discharged: the collision
re-check ran dated (§market-check) and the qualifier line is **none** ("Keep name" — no qualifier was
ever ruled or needed); `research/decisions/ADR-0011.md` gains the amendment-log dossier entry its
Consequences row requires. **OQ-383's executable-name half** is ruled by the crate-naming-scheme row
("binaries keep short names (hh, hh-kernel, hh-helper)"); its presentation halves remain a proposal
below.

## Market check — naming (R-2.12.5 evidence; OQ-035 re-check; amend-(b) MAJOR-1)

Method: `cargo metadata --no-deps --format-version 1` lists **53** workspace packages at this HEAD
(a6c1eb9) — the run-time set is authoritative (the planning-time baseline said 51; the workspace grew
by `hh-tls` (R4.5c) and `hh-github-issues` (R4.7)). Publish-set spelling per the ruled scheme:
umbrella `hharness` + `hharness-<x>` per package = **54 names**, plus the three binary names
(`hh`, `hh-kernel`, `hh-helper` — binaries keep short names; the crates carrying them publish as
`hharness-cli`/`hharness-kernel`/`hharness-helper`) and the two recorded taken names (`hh-cli`,
`harnessharness`). One `GET https://crates.io/api/v1/crates/<name>` per name, descriptive User-Agent
(`HarnessHarness-R4.8-naming-check`), ≥ 1 s spacing, read-only. **Result: 54/54 publish-set names
HTTP 404 (free) — zero collisions in the to-be-published set; no STOP-and-ask triggered.**

| queried_at (UTC) | name | HTTP |
|---|---|---|
| 2026-10-22T07:21:58Z | hharness | 404 |
| 2026-10-22T07:21:59Z | hharness-acp | 404 |
| 2026-10-22T07:22:00Z | hharness-analysis | 404 |
| 2026-10-22T07:22:02Z | hharness-assembly | 404 |
| 2026-10-22T07:22:03Z | hharness-bench | 404 |
| 2026-10-22T07:22:04Z | hharness-budget | 404 |
| 2026-10-22T07:22:06Z | hharness-bundle | 404 |
| 2026-10-22T07:22:07Z | hharness-cli | 404 |
| 2026-10-22T07:22:09Z | hharness-codegen | 404 |
| 2026-10-22T07:22:10Z | hharness-compact-evict-oldest | 404 |
| 2026-10-22T07:22:11Z | hharness-compiler | 404 |
| 2026-10-22T07:22:13Z | hharness-containment | 404 |
| 2026-10-22T07:22:14Z | hharness-context | 404 |
| 2026-10-22T07:22:15Z | hharness-control | 404 |
| 2026-10-22T07:22:17Z | hharness-debt | 404 |
| 2026-10-22T07:22:18Z | hharness-embed | 404 |
| 2026-10-22T07:22:19Z | hharness-embed-client-generated | 404 |
| 2026-10-22T07:22:21Z | hharness-embed-schema | 404 |
| 2026-10-22T07:22:22Z | hharness-env | 404 |
| 2026-10-22T07:22:23Z | hharness-eval | 404 |
| 2026-10-22T07:22:25Z | hharness-evolution | 404 |
| 2026-10-22T07:22:26Z | hharness-experiment | 404 |
| 2026-10-22T07:22:27Z | hharness-fleet | 404 |
| 2026-10-22T07:22:29Z | hharness-fleet-adapter | 404 |
| 2026-10-22T07:22:30Z | hharness-gateway | 404 |
| 2026-10-22T07:22:31Z | hharness-github-issues | 404 |
| 2026-10-22T07:22:33Z | hharness-helper | 404 |
| 2026-10-22T07:22:34Z | hharness-hir | 404 |
| 2026-10-22T07:22:36Z | hharness-hosting | 404 |
| 2026-10-22T07:22:37Z | hharness-identity | 404 |
| 2026-10-22T07:22:39Z | hharness-kernel | 404 |
| 2026-10-22T07:22:40Z | hharness-lab | 404 |
| 2026-10-22T07:22:41Z | hharness-ledger | 404 |
| 2026-10-22T07:22:43Z | hharness-mcp | 404 |
| 2026-10-22T07:22:44Z | hharness-mcp-lab | 404 |
| 2026-10-22T07:22:45Z | hharness-memory-store | 404 |
| 2026-10-22T07:22:47Z | hharness-monitor | 404 |
| 2026-10-22T07:22:48Z | hharness-ontology | 404 |
| 2026-10-22T07:22:50Z | hharness-orchestrator | 404 |
| 2026-10-22T07:22:51Z | hharness-plugin | 404 |
| 2026-10-22T07:22:52Z | hharness-plugin-fixture | 404 |
| 2026-10-22T07:22:54Z | hharness-provenance | 404 |
| 2026-10-22T07:22:55Z | hharness-registry | 404 |
| 2026-10-22T07:22:57Z | hharness-results | 404 |
| 2026-10-22T07:22:58Z | hharness-secrets | 404 |
| 2026-10-22T07:22:59Z | hharness-subagent | 404 |
| 2026-10-22T07:23:01Z | hharness-telemetry | 404 |
| 2026-10-22T07:23:02Z | hharness-tls | 404 |
| 2026-10-22T07:23:03Z | hharness-tracker-fixture | 404 |
| 2026-10-22T07:23:05Z | hharness-varhost | 404 |
| 2026-10-22T07:23:06Z | hharness-verification | 404 |
| 2026-10-22T07:23:08Z | hharness-web | 404 |
| 2026-10-22T07:23:09Z | hharness-wire | 404 |
| 2026-10-22T07:23:10Z | hharness-xcheck | 404 |
| 2026-10-22T07:23:12Z | hh | 200 |
| 2026-10-22T07:23:13Z | hh-kernel | 404 |
| 2026-10-22T07:23:14Z | hh-helper | 404 |
| 2026-10-22T07:23:16Z | hh-cli | 200 |
| 2026-10-22T07:23:17Z | harnessharness | 200 |

Collision detail on the taken names (read-only `GET`s, same session): `harnessharness` — crate 0.1.0
"Declarative builder for AI agent harnesses", created 2026-09-06, owner **rellfy** (Lorenzo Fontoura;
`/owners` endpoint, `github_username_matches`), repo `github.com/rellfy/harnessharness`, Apache-2.0 —
the same-domain collision the naming ruling already records; the `hharness` prefix decision stands
and this check re-confirms it (54/54 free). `hh` — "highlight the exif photo info" (created
2026-01-16, 89 downloads). `hh-cli` — "A terminal-based coding agent runtime" (created 2026-03-01,
44 downloads). `hh`/`hh-cli` taken is the recorded evidence the prefix ruling cited; the binary names
`hh`/`hh-kernel`/`hh-helper` are unaffected (installed command names, not crates.io package names).

**GitHub leg** (`api.github.com/search/repositories?q=harnessharness`, 2026-10-22): total_count 4 —
`rellfy/harnessharness` (the crate's repo, created 2026-08-28), `SferrellaA/harnessharness` and
`ddalggack/harnessharness` (no description; unrelated same-name repos), and
`SteveVitali/HarnessHarness` (this project). **Web leg** (search, 2026-10-22): `"HarnessHarness"`
returns only rellfy's crate/docs.rs pages and this project's repo; no other product, company or
trademark hit found (`"HarnessHarness" trademark` — no registration hits; the MetaHarness collisions
OQ-035 recorded — ruvnet/metaharness, SuperagenticAI/metaharness, the Stanford Meta-Harness paper —
are distinct-name, resolved by ADR-0011's rename). The trademark leg is a best-effort web check —
USPTO TESS has no read-only API usable offline; recorded honestly.

## Proposals — the remaining OQs (final dispositions; adopt/amend/decline at GATE-G7)

- **OQ-068 / OQ-218 / OQ-394 — the one allocation record** (`needs-code-leg(R4.15a)`): ADR-0210's
  entry test wants one record naming the reverse-DNS prefix, method namespace, resource-URI scheme
  and A2A extension URI, collision-checked. Proposed allocation: `<hh-prefix>` = **`hharness`**
  (`_meta["hharness/…"]` keys — e.g. `hharness/hir`, `hharness/account`; OQ-218's carried set:
  `hharness/semantic_id`, `hharness/version_id`, `hharness/authority`, `hharness/provenance_ref` —
  identity + authority + provenance triple, CC3); method namespace **`_hh/`** (interim, kept);
  resource-URI scheme **`hh://`** (interim, kept — `hh://run/{handle}/ledger|status`,
  `hh://results/{configuration_id}`); A2A `AgentExtension.uri` =
  **`https://github.com/SteveVitali/HarnessHarness/a2a/hir`** (identifier-stable URL, never fetched —
  no owned domain is ruled; amend at GATE-G7 if the operator prefers a URN). Collision basis:
  `hharness` is 404 on crates.io (§market-check) and carries no known product; `hh://` and `_hh/`
  are the spec's own interim spellings. The R4.15a change: append the allocation record to
  ADR-0011's log / the ADR-0210 package records, then substitute the placeholders in spec text and
  code constants (the substitution half is R4.15a's mechanical conform).
- **OQ-093 — pricing-table governance** (`needs-code-leg(R4.15a)`): proposed governance rule —
  `PricingTable` versions publish into the reserved `hh/` namespace **by the instrument release
  process only** (the namespace's own rule, §6); `max_age` default **90 days** (a stale pin refuses
  at `resolve` under `RegistryPolicy.max_age` — the ratified TUF-shaped freshness); retirement test:
  a table is retired when a newer `version_id` in the same namespace supersedes it (revocation never
  edits history). Currency: USD-denominated rows; provider-reported units (credits, nano-AIU) get
  their own `ResourceQuantity` rows keyed `(provider, unit)` — yes, per the OQ's own question, since
  unconvertible units stay distinct (CC3: nothing silently lost). The R4.15a change: the governance
  rule written as the PricingTable debt-record MUST-data.
- **OQ-162 — signing infrastructure & trust-root custody** (`needs-code-leg(R4.15a)`): proposed
  custody rule — extension signing stays **key-based** (`SignerRef` under
  `TrustRootPolicy.accepted_signers`, ADR-0063); the **keyless trusted-publishing class** (OIDC
  publisher identity, S-249) is admitted as a second signer class for first-party crates.io
  publication — a `TrustRootPolicy` row naming issuer + subject pattern; the program's trust roots
  are **maintainer-held keys under the L4.2 maintainer policy** (custody/rotation documented there —
  the OQ-170 signer-key engineering stays R4.15e's); rotation = **two-key bridge**: the outgoing root
  countersigns the incoming root's `TrustRootPolicy` row, anchored by the ADR-0036 D4 rotation
  checkpoint (the audit anchor ADR-0210 names); the registry **signs its own `NameHistoryEntry`
  rows** under the namespace owner's key (`require_signature` per `NamespaceRecord.policy`; `hh/`
  entries under the kernel release key). The R4.15a change: the custody rule + the `TrustRootPolicy`
  rows recorded as MUST-data.
- **OQ-173 — audit-envelope retention** (`needs-code-leg(R4.15a)`): proposed `RetentionPolicy`
  MUST-data — durations per envelope class: audit envelopes default **indefinite** (floor: a whole
  run deletes only by a lab-wide `gc` record at authority ≥ principal — ADR-0163 D4, already ruled);
  experiment-ledger envelopes sit below audit envelopes and above rows in the retention order (ruled);
  **legal-hold semantics**: `RetentionPolicy.legal_hold` settable/clearable only at authority ≥
  principal, and while set **every** gc path refuses `Refused{gc = legal_hold}` — including the
  lab-wide record (a hold is a hold). The R4.15a change: the MUST-data record + the ADR-0068 R2
  authorization row; the gc-path enforcement leg (a `legal_hold` check in the gc admission path)
  rides R4.15c if code is needed.
- **OQ-332 — detached-payload trust & expiry** (`needs-code-leg(R4.15d)`): the trust-field proposal
  below is this OQ's answer; R4.15d lands the transports + the expiry/re-verification debt record on
  the bundle format.
- **OQ-383 — executable name + presentation** (executable name: **recording-act** — "binaries keep
  short names (hh, hh-kernel, hh-helper)" rules it; the publish-crates carry them as
  `hharness-cli`/`hharness-kernel`/`hharness-helper` packages whose installed binaries keep
  `hh`/`hh-kernel`/`hh-helper`. Presentation halves: `needs-code-leg(R4.15a)`): proposed MUST-data —
  Lab nouns presented **flat** (the landed CLI's convention — `hh lab …`-style groupings, not a
  single `lab` mega-prefix); flags lower to precedent-style presets spelled as landed. The R4.15a
  change: the presentation MUST-data recorded; no code change (the landed spelling is the answer).
- **OQ-384 — exit-code numerals** (`needs-code-leg(R4.15a)`): proposed — **ratify the landed table**
  (`crates/hh-cli/src/exit_class.rs` `ExitClass::code()`): `ok`=0, `invocation_error`=64 …
  `tampered`=73 — the BSD `sysexits` band 64–73, avoiding 1, 2, 126–165, 255 by construction;
  `result.exit_class` stays the authoritative machine field. Success-with-veto: **distinct code only
  on request** — default 0 (the veto is legible in `result.exit_class`; a numeric channel for it is
  opt-in, never default). The R4.15a change: the numeral table fixed as MUST-data.
- **OQ-389 — capability-token delivery** (`needs-code-leg(R4.15a)`): proposed — ratify the landed
  set (ADR-0301 D8): **owner-only mode-0600 token file** (`--token-file`) as the primary delivery for
  every launch kind (desktop/terminal/SSH-forwarded — the file survives forwarding where env vars do
  not); **spawning-shell stderr** as the interactive fallback (the minted token prints once to stderr
  when no file path is given); **URL-fragment delivery rejected** (P9: the token never rides a URL);
  **rotation on re-launch** = a fresh ≥128-bit mint per `serve` launch bound to the `CallerBinding`'s
  `expires_at` — landed behaviour; **multi-surface rule**: one token per surface instance — concurrent
  surfaces each mint their own binding, never share. The R4.15a change: the per-launch-kind delivery
  table recorded as MUST-data.
- **OQ-411 — `ContractVersionPolicy` sunset defaults** (`needs-code-leg(R4.15a)`): proposed MUST-data —
  a superseded contract version stays supported **≥2 minor cycles** per contract kind (schema/dialect
  classes: ≥4 — the additive-only rule already forbids in-place breaks, ADR-0180 D3); a window may be
  shortened only by a `lifecycle.definition.changed` at authority ≥ definition — shortening is a
  narrowing, never a widening, so it composes under `authority_cap` (a cap below the window narrows;
  it can never extend). The R4.15a change: the sunset table + layering rule as `hh/` MUST-data.
- **OQ-445 — spec-debt revalidation ownership** (`needs-code-leg(R4.15a)`): proposed — the
  post-Phase-5 revalidation owner is **the maintainer per the L4.2 maintainer policy** (RK-06's
  cadence lands as a policy clause — the same destination BL-41/42/43's standing rows take);
  `evidence_max_age` = **one model generation** (the proposed default, ratified); a refuted
  `provisional` source **flags `expiring`** — never auto-reopens an ADR (a re-open is a human act
  with a dated note). The R4.15a change: the register header gains `owner` + `evidence_max_age`
  MUST-data; L4.2's policy doc carries the cadence clause.

## Trust fields — `manifest.locations[]` remote fetch (DF-S4.2-1 / R4.15d contract input)

OQ-332's question — "trust and expiry of `fetch[]` locations (mirrors, registries, Hub-style stores),
whether a fetched member's arrival is ledgered, and the interaction with OQ-112 (GC'd members keep
addresses) and OQ-085 (retention tiers)" — answered as the proposed field set R4.15d conforms against
(the skeleton's named input: "R4.8's trust-field section + the operator's adopted rows").

- **`FetchTrustPolicy` record (new MUST-data, `hh/` namespace):**
  `{admission_classes[], admitted_origins[], accepted_signers: SignerRef[], require_ledgered_arrival:
  true}` — one record per fetch site, version_id-pinned into the bundle's `resolved_dependencies`
  (a fetched member is only as trustworthy as the policy that admitted it — CC3).
- **Admission classes for `FetchEntry.locations[]`** (the URI stays `credential_free_uri` —
  credentials ride `hh-secrets` `SecretRef`s at the fetch site / mediated egress, never the URI):
  1. `first_party` — `https:` origins on the `admitted_origins` allowlist (project-controlled hosts:
     the launch repo's release assets and named project hosts). Trust root: TLS PKI (the hh-tls
     rustls/webpki graph, ADR-0365) **plus** the allowlist row — no application-level signature
     needed: integrity is by-construction (below).
  2. `signed_url` — presigned-URL carriers (`https:` with embedded signature params); admitted with
     `FetchEntry.expires` **enforced**: an expired location refuses `Refused{fetch =
     location_expired}`; an attestation over the location set may ride the bundle's
     `attestations.jsonl` sidecar, verified under `accepted_signers` (`pin`-class endorsement,
     ADR-0063 shape).
  3. `content_addressed` — `ipfs:`/CID-class locations: admitted unconditionally — integrity is
     by-construction (the member re-hashes to its address at S3; wrong bytes fail `MemberMismatch`,
     never silently land). `ipfs:` rides only if adopted at GATE-G7 (the ruling is pending).
  4. Cleartext `http:` — **never admitted** (`Refused{fetch = transport_refused}`; the egress floor
     + ADR-0365 TLS scope).
- **Verification & ledgering:** `fetch` verifies on arrival (S3 re-hash — "never trusts a location's
  digest claim", §5h, already ratified) **and** every arrival is ledgered:
  `bundle.member.fetched{address, location_uri, fetched_at, transport, outcome}` appended before the
  member reads `present` (durable-before-visible); the fetch itself is a `net_egress` effect under
  Π mediation (`hh-env` egress rules; an unmediated fetch refuses typed).
- **Expiry & re-verification rule (OQ-332's entry test):** `FetchEntry.expires` enforced per
  location; a fetched member re-verifies on every materialization (no "fetched once, trusted
  forever"); a GC'd member keeps its addresses (OQ-112) and may re-materialize while a live location
  serves — the `gc` status is a retention fact, never an availability guarantee; retention tiers
  (OQ-085) are unaffected — fetch changes where bytes come from, never how long they live.
- **`transport_pending` removal:** the typed refusal `Refused{fetch = transport_pending}` is
  replaced by the admission-class refusals above (`transport_refused` / `location_expired` /
  `origin_not_admitted`) — honest refusal preserved, the placeholder gone.

## What this brief does not decide

No LICENSE file, crate metadata or rename lands here (L4.1/L4.4/L4.5); no ruling the operator has
not already made; `deferred(ADR-0210)` / `MET-DIFFERENTLY` statuses unchanged (re-entry is a scope
ADR citing satisfied entry tests); BL-33's flip is R4.16's accounting after GATE-G7 adoption and the
conforms land.
