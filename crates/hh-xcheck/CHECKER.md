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

`identify_text(bytes)`: `idp_id("text", normalized)` where `normalized` is
`bytes` with each `\r\n` and lone `\r` rewritten to `\n`.

### `tree/1` addressing

A tree is a manifest of named entries. Canonical tree document: a JSON
object `{name: node}` where `node` is

- `{ "kind":"file", "bytes_hex":<hex>, "exec":<bool> }`
- `{ "kind":"symlink", "target":<string> }`
- `{ "kind":"dir", "entries":{…} }` — recurse

— i.e. exactly this bundle's `<name>.input.json` shape. Tree address:
`idp_id("tree", canonical(tree_doc))`. The `.canonical.json` members carry
the implementation's own canonical rendering — the foreign fold must
reproduce the *id*; the canonical document bytes are the E1 reference
rendering.

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
store, then run `scenario.json`'s request vector over snapshot
`snapshot_id`: resolve each `variant_version_ids` entry (Audit mode), the
selectors and `query`/`slot_choices` enumerated in the scenario —
`expected-outputs.json` is the canonical byte target; answer its digest.

## Arm replay

| arm | input | expected answer |
|---|---|---|
| `identity-trees` | `<n>.input.json` manifest | `id` string per case |
| `identity-records` | `<n>.input.json` (`arm`: blob/text/record) | `{"version_id":…,"semantic_id":…}` |
| `ledger-transcript` | `events.wal` | `{"head_hash":…,"event_hashes":[…]}` (skip `wal_sha256`) |
| `registry-snapshot` | `registry.json` + `scenario.json` | `{"outputs_sha256":…}` |
| `plugin-manifests` | `valid/…`, `invalid/…` bytes | verdict string per relative path |
| `canonical-parse` | `frames/*.json` | `{"sha256":…}` per case (`bytes` is informational) |

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
    "canonical-parse":  { "<case>": {"sha256":"…","parse_ns":N,"rehash_ns":N} }
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
