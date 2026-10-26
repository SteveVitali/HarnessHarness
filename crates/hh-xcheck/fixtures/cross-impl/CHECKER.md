# hh-xcheck-bundle/1 — the cross-implementation checker contract

Spec §10.7 · ticket R2.21 · ADR-0353. This file is **part of the bundle** —
the same bytes a foreign implementation (E2, E3, …) is checked against.
It defines the primitives, each arm's replay, and the `answers.json` shape
`hh-xcheck verify <bundle> <answers>` compares.

Every hash is SHA-256 over bytes, spelled lowercase-hex. Every `sha256:*`
or `id` value below is `sha256:` + lowercase-hex digest.

## Primitives

### Canonical JSON (`hh-json/1`, CC1)

The bundle's canonical JSON: object keys sorted by **byte order** of the
UTF-8 key; members separated `,`, pair separator `:`; no other whitespace.
Numbers are integers only. Strings are UTF-8; escapes only `\uXXXX` for
U+0000–U+001F (lowercase hex), `\"`, `\\`, plus `\n`, `\r`, `\t`; all other
characters, including non-ASCII, are verbatim UTF-8. `true`/`false`/`null`
lowercase. `to_canonical_string` means exactly this serialization.

Every committed `*.json` bundle member is already canonical: `parse(bytes)`
then re-serializing must reproduce the bytes exactly.

### `idp/1` identity

```
idp_id(tag, payload) = "sha256:" + hex(SHA256("idp/1" ‖ 0x1f ‖ tag ‖ 0x1f ‖ payload))
```

(`tag`, `payload` are the UTF-8/raw byte strings; 0x1f is one byte.)

`address(bytes, media_type)`: `id` = `idp_id("blob", bytes)` — the media
type is carried alongside, never hashed.

`identify_text(bytes)`: `idp_id("hir.text", bytes)` — the `hir.text` domain
tag (the `Text` leaf's `RecordKind` tag) over the **verbatim** content bytes.
No line-ending normalization is applied on the reference path; the committed
corpus exercises no `\r` byte, so this spelling is pinned by contract, not by
a fixture.

### `tree/1` addressing

A tree is a manifest of named entries. The bundle's `<name>.input.json`
members carry the **manifest input shape** — a JSON object `{name: node}`
where `node` is

- `{ "kind":"file", "bytes_hex":<hex>, "exec":<bool> }`
- `{ "kind":"symlink", "target":<string> }`
- `{ "kind":"dir", "entries":{…} }` — recurse

The manifest spelling is the input, not the hashed document. The canonical
tree document is its **Merkelized rendering**: a file's `bytes_hex` content
is replaced by `"content":<blob-id>` (the `address(content, media_type)` id
over the raw bytes — the media type is never hashed), and a dir's `entries`
object is replaced by `"tree":<subtree-id>`, the recursively computed tree
address of that dir's rendered entries:

- `{ "kind":"file", "content":<blob-id>, "exec":<bool> }`
- `{ "kind":"symlink", "target":<string> }` — addressed by target, verbatim
- `{ "kind":"dir", "tree":<subtree-id> }` — recurse at the manifest level

Tree address: `idp_id("tree", canonical(tree_doc))` over the Merkelized
document. Each `<name>.canonical.json` member is the E1 reference rendering
of exactly this document — the foreign fold must reproduce the *id*; the
committed canonical bytes pin the document that yields it.

### Ledger WAL (`events.wal`, DF-S1.5-3)

NDJSON frames, one per line, LF-terminated. `{"k":"e","v":<envelope>}` is an
event frame; `{"k":"c",…}` is a commit frame. All frames are canonical JSON.

Replay: `prev = "genesis"`; for each `e` frame in order compute

```
hash = idp_id("ledger.event", 0x00 ‖ canonical(envelope − {"hash"}) ‖ prev_hash)
```

(the 0x00 byte prefix separates the preimage from the domain; `prev_hash` is
the previous computed hash's UTF-8 bytes) — check `hash == envelope.hash`,
then `prev = hash`. The folded last `hash` is `head_hash`.

### Plugin-manifest verdicts (DF-S1.27-1)

`PluginManifest/1` decode + admission under the bundle's posture
(`first_party=true`, `requests_cap` = deny-everything). Answer per fixture:
`"admit:<plugin_id>"` or `"refuse:<kind>"` where `<kind>` is the
`ManifestError` kind spelling (`NotJson`, `BadShape`, `BadRequests`,
`RequestsExceedCap`, …) — see `hh-registry::extension::plugin`.

### Registry snapshot (DF-S1.8-1)

`registry.json` is the canonical append-only registry log. Rebuild the
store, then run the request vector over snapshot `snapshot_id`. The vector
is **fixed corpus contract**, not enumerated by the scenario —
`scenario.json` carries only `snapshot_id` + `variant_version_ids`. In
order (the E1 driver is `hh-registry::corpus::outputs`):

1. resolve each `variant_version_ids` entry — `ResolveMode::Audit`;
2. resolve the six pinned selectors — namespace `hh` name
   `control_strategy/control_strategy-v0`, namespace `local` name
   `control_strategy/control_strategy-v1`, then the same `hh`-v0/`local`-v1
   pair for `context_policy` and `export_sink` — `ResolveMode::Execute`,
   snapshot-pinned (a refusal records `{"error":<reason>}`);
3. `query` the single clause `kind = "variant"`, snapshot-pinned;
4. `slot_choices` for the three pinned class ids `control_strategy`,
   `context_policy`, `export_sink` — constraints `supports:∅`,
   `contract_version:"1.0"`, snapshot-pinned.

`expected-outputs.json` is the canonical byte target; answer its digest.

### HIR/1 documents (`hir-documents`, DF-S1.2-2 — R4.3 extension)

Each `<name>.input.json` member is a canonical HIR/1 document (the §3.1
schema — the same `hh-json/1` encoding every bundle member uses). The
replay is the §3.1.7 operation pipeline, in order:

1. **parse** — decode the bytes as a HIR/1 document. A malformed
   encoding, an unknown kind, or a missing mandatory member (e.g. a node
   without `provenance`) is a refusal: `parse` answers
   `"refuse:<Kind>"`; every later leg answers `"skipped"` and every
   digest member `null`.
2. **canonicalize** — re-emit the parsed document's canonical bytes.
   `canonical_sha256` answers the SHA-256 of those bytes (for a parse-ok
   case they equal the input — the input IS the canonical form).
3. **validate** — run the §3.1.4 invariant set. `validate` answers
   `"valid"` or `"invalid:<Kind[,Kind…]>"` — the collected
   `HirError::kind_str()` spellings, first-raised order, duplicates
   collapsed. An invalid document's seal leg is `"skipped"`.
4. **seal** — `seal(doc, sealed_at)` under the arm's `scenario.json`
   `sealed_at`: resolve and pin every ref, keep authored semantic ids
   (compute them where absent), endorse every member at definition
   scope, stamp `sealed`, pin a hosted body's `version_identity`,
   re-validate. `seal` answers `"sealed"` or
   `"refuse:<Kind[,Kind…]>"`. On `"sealed"`, `sealed_sha256` answers the
   SHA-256 of the sealed document's canonical bytes and
   `definition_version_id` the sealed root's `version_id`; both are
   `null` otherwise.

`<Kind>` is the closed §3.1.4 `HirError` spelling — `UnresolvedRef`,
`MissingProvenance`, `ContributionUndeclared`, `NonCanonicalInput`, … —
the variant name with no detail text.

### Ledger WAL scenarios (`ledger-scenarios`, DF-S1.5-3 — R4.3 extension)

Additional transcript/WAL scenarios under `<case>/` — the same replay
contract as `ledger-transcript` (`events.wal` canonical NDJSON `e`/`c`
frames, the `0x00 ‖ canonical(envelope − {"hash"}) ‖ prev_hash` fold,
`prev = "genesis"`), one WAL per case dir. `envelopes.json` decodes the
`e` frames for reference; `scenario.json` describes the recorded recipe.

`expected.json` answers per case name: `{ "event_hashes": […],
"head_hash": "sha256:…" }`. The `<case>.event_count` and
`<case>.wal_sha256` members are informational (integrity pins a foreign
checker never answers — the same class as `ledger-transcript`'s
`wal_sha256`/`event_count`).

## Arm replay

| arm | input | expected answer |
|---|---|---|
| `identity-trees` | `<n>.input.json` manifest | `id` string per case |
| `identity-records` | `<n>.input.json` (`arm`: blob/text/record) | `{"version_id":…,"semantic_id":…}` |
| `ledger-transcript` | `events.wal` | `{"head_hash":…,"event_hashes":[…]}` (skip `wal_sha256`) |
| `registry-snapshot` | `registry.json` + `scenario.json` | `{"outputs_sha256":…}` |
| `plugin-manifests` | `valid/…`, `invalid/…` bytes | verdict string per relative path |
| `canonical-parse` | `frames/*.json` | `{"sha256":…}` per case (`bytes` is informational) |
| `hir-documents` | `<name>.input.json` + `scenario.json` | the pipeline answer object per case |
| `ledger-scenarios` | `<case>/events.wal` | `{"head_hash":…,"event_hashes":[…]}` per case |

For `record` inputs: `version_id = idp_id(domain_tag, canonical(canonical_full))`;
when `canonical_semantic` is non-null, `semantic_id = idp_id(domain_tag + "#semantic",
canonical(canonical_semantic))`, else `null`.

## `answers.json`

```json
{
  "candidate": "E2",
  "arms": {
    "identity-trees":   { "<case>": "sha256:…" },
    "identity-records": { "<case>": {"version_id":"…","semantic_id":"…"} },
    "ledger-transcript":{ "head_hash":"…","event_hashes":["…",…] },
    "registry-snapshot":{"outputs_sha256":"…" },
    "plugin-manifests": { "<relpath>": "admit:<id>"|"refuse:<kind>" },
    "canonical-parse":  { "<case>": {"sha256":"…","parse_ns":N,"rehash_ns":N} },
    "hir-documents":    { "<case>": {"parse":"ok"|"refuse:<Kind>","validate":"valid"|"invalid:<Kind>"|"skipped","canonical_sha256":"…"|null,"seal":"sealed"|"refuse:<Kind>"|"skipped","sealed_sha256":"…"|null,"definition_version_id":"…"|null} },
    "ledger-scenarios": { "<case>": {"head_hash":"…","event_hashes":["…",…]} }
  }
}
```

Timing members (`parse_ns`, `rehash_ns`, `*_ms`) are recorded, never gated —
the compare keys on the digest/verdict members only. A missing or
disagreeing case fails by name; `hh-xcheck verify` exits 1.

## Honesty boundary

The bundle's E1 self-check (`hh-xcheck selfcheck`) is fixture-verified — it
proves the packaging reproduces E1's answers. A foreign implementation's
green `verify` run is evidence **the foreign run** produced; nothing in this
bundle mints it. `bundle.json`'s `pending_cells[]` lists the cells still
gated on the HUMAN-H3 foreign-toolchain environment.

`bundle.json` `verified_by[]` (CF-493): every `arms[]` entry and every
`pending_cells[]` entry carries the candidate/evidence record —
`{candidate, evidence_class, evidence_ref}` members naming `E1`
(`fixture-verified`, the in-tree self-check) and the foreign candidates
whose recorded replay verifies that arm or partially discharges that
cell (`foreign-run-verified` — a class *below* the fixture-verified
ceiling, never conflated with it). A pending cell's `verified_by[]` is
partial discharge only: `status` stays `environment-pending` while the
named `needs` prerequisite stands; an empty `verified_by[]` records that
no candidate's evidence covers the cell (the committed
`foreign/e2-python` tree cannot answer the R4.3 arms — their
`*.e2-replay` cells carry no E2 entry).
