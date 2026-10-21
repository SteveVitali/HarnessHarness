// lib/core.ts — the E3-candidate shared core (R4.4; the one spelling R4.4a
// imports, never re-implements — the ticket's owned shared decision).
//
//   Primitive 1 — canonical JSON (`hh-json/1`, CC1)
//   Primitive 2 — `idp/1` identity
//   Primitive 3 — `tree/1` addressing over the `<n>.input.json` manifest shape
//
// Implemented from CHECKER.md's spec text plus the committed fixture
// shapes — no Rust code was transliterated. Stdlib only: `node:crypto`
// (+ the `Buffer`/`TextDecoder` globals). Erasable-TypeScript only (node
// type stripping): type annotations and `import type` — never enums,
// namespaces or parameter properties.
//
// Honesty note: this is a *candidate* implementation. Its answers are
// evidence the foreign run produced; a green `hh-xcheck verify` does not
// mint E1 truth.

import { createHash } from "node:crypto";

// ─────────────────────────────────────────────────────────────────────────────
// Primitive 1 — canonical JSON (`hh-json/1`)
//
// Byte-sorted object keys (UTF-8 byte order), `,` member separator, `:` pair
// separator, no other whitespace. Integers only. Strings are UTF-8; escapes
// are `\uXXXX` (lowercase hex) for U+0000–U+001F plus `\"`, `\\`, `\n`, `\r`,
// `\t`; every other character is verbatim UTF-8. `true`/`false`/`null`
// lowercase.
// ─────────────────────────────────────────────────────────────────────────────

/** The canonical-JSON value model — `hh-json/1` admits integers only. */
export type Json =
  | null
  | boolean
  | number
  | string
  | Json[]
  | { [key: string]: Json };

const SHORT_ESCAPES: { [cp: number]: string } = {
  0x22: '\\"',
  0x5c: "\\\\",
  0x0a: "\\n",
  0x0d: "\\r",
  0x09: "\\t",
};

function escapeString(s: string): Buffer {
  const parts: Buffer[] = [];
  for (const ch of s) {
    // `for…of` iterates code points; a lone surrogate still surfaces as a
    // one-code-unit string whose cp is in 0xD800–0xDFFF.
    const cp = ch.codePointAt(0) as number;
    const esc = SHORT_ESCAPES[cp];
    if (esc !== undefined) {
      parts.push(Buffer.from(esc, "ascii"));
    } else if (cp < 0x20) {
      parts.push(Buffer.from("\\u" + cp.toString(16).padStart(4, "0"), "ascii"));
    } else if (cp >= 0xd800 && cp <= 0xdfff) {
      // A lone surrogate has no UTF-8 spelling — refuse it (honest
      // failure, never a silently-minted U+FFFD).
      throw new Error(`unpaired surrogate U+${cp.toString(16)} is outside hh-json/1`);
    } else {
      parts.push(Buffer.from(ch, "utf8")); // verbatim UTF-8
    }
  }
  return Buffer.concat(parts);
}

/** UTF-8 byte order — the `hh-json/1` key sort. */
function compareUtf8(a: string, b: string): number {
  return Buffer.compare(Buffer.from(a, "utf8"), Buffer.from(b, "utf8"));
}

/** `to_canonical_string` — the `hh-json/1` serialization, as bytes. */
export function canonical(value: Json): Buffer {
  if (value === null) return Buffer.from("null", "ascii");
  if (value === true) return Buffer.from("true", "ascii");
  if (value === false) return Buffer.from("false", "ascii");
  if (typeof value === "number") {
    if (!Number.isInteger(value)) {
      throw new Error(`non-integer number ${value} is outside hh-json/1`);
    }
    return Buffer.from(String(value), "ascii");
  }
  if (typeof value === "string") {
    return Buffer.concat([Buffer.from('"'), escapeString(value), Buffer.from('"')]);
  }
  if (Array.isArray(value)) {
    const parts: Buffer[] = [Buffer.from("[")];
    value.forEach((v, i) => {
      if (i > 0) parts.push(Buffer.from(","));
      parts.push(canonical(v));
    });
    parts.push(Buffer.from("]"));
    return Buffer.concat(parts);
  }
  if (typeof value === "object") {
    const keys = Object.keys(value).sort(compareUtf8);
    const parts: Buffer[] = [Buffer.from("{")];
    keys.forEach((k, i) => {
      if (i > 0) parts.push(Buffer.from(","));
      parts.push(Buffer.from('"'));
      parts.push(escapeString(k));
      parts.push(Buffer.from('":'));
      parts.push(canonical(value[k]));
    });
    parts.push(Buffer.from("}"));
    return Buffer.concat(parts);
  }
  throw new Error(`not a canonical-JSON value: ${typeof value}`);
}

/** JSON.parse joins `\uD83D\uDE00`-style escapes into real code points
 *  already; this is a pure validator — an unpaired surrogate (in a key or
 *  a value) has no UTF-8 spelling and is refused honestly. (In-place:
 *  never rebuild a parsed object — assigning a `"__proto__"` key through
 *  `out[k] = v` would hit the prototype setter and silently drop it.) */
function rejectSurrogates(value: Json): void {
  if (typeof value === "string") {
    for (let i = 0; i < value.length; i += 1) {
      const cp = value.codePointAt(i) as number;
      if (cp >= 0xd800 && cp <= 0xdfff) {
        throw new Error(
          `unpaired surrogate U+${cp.toString(16)} is outside hh-json/1`,
        );
      }
      if (cp > 0xffff) i += 1;
    }
    return;
  }
  if (Array.isArray(value)) {
    value.forEach(rejectSurrogates);
    return;
  }
  if (typeof value === "object" && value !== null) {
    for (const [k, v] of Object.entries(value)) {
      rejectSurrogates(k);
      rejectSurrogates(v);
    }
  }
}

/** Reject `NaN`/`Infinity` spellings (outside JSON, let alone hh-json/1)
 *  and non-integer values (`hh-json/1` admits integers only). */
function checkNumbers(value: Json): Json {
  if (typeof value === "number") {
    if (!Number.isInteger(value)) {
      throw new Error(`non-integer number ${value} is outside hh-json/1`);
    }
    if (!Number.isSafeInteger(value)) {
      throw new Error(`integer ${value} exceeds the safe range`);
    }
    return value;
  }
  if (Array.isArray(value)) {
    value.forEach(checkNumbers);
    return value;
  }
  if (typeof value === "object" && value !== null) {
    Object.values(value).forEach(checkNumbers);
    return value;
  }
  return value;
}

/** Parse a JSON document into the canonical value model. Non-integer
 *  numbers and unpaired surrogates are refused, never silently coerced. */
/** Bytes → text: `fatal: true` refuses invalid UTF-8 honestly
 *  (`Buffer.toString("utf8")` would silently mint U+FFFD replacements —
 *  a degrade, never allowed). The one decoding spelling (CC1). */
const UTF8_FATAL = new TextDecoder("utf-8", { fatal: true });
export function decodeUtf8(data: Buffer): string {
  return UTF8_FATAL.decode(data);
}

export function parseJson(data: Buffer | string): Json {
  const text = Buffer.isBuffer(data) ? decodeUtf8(data) : data;
  // JSON.parse already rejects non-JSON literals (NaN, Infinity) and
  // joins `\uXXXX` surrogate pairs into real code points.
  const value = JSON.parse(text) as Json;
  rejectSurrogates(value);
  return checkNumbers(value);
}

// ─────────────────────────────────────────────────────────────────────────────
// Primitive 2 — `idp/1` identity
//
//   idp_id(tag, payload) = "sha256:" + hex(SHA256("idp/1" ‖ 0x1f ‖ tag ‖ 0x1f ‖ payload))
// ─────────────────────────────────────────────────────────────────────────────

const SEP = Buffer.from([0x1f]);

export function sha256Hex(data: Buffer): string {
  return createHash("sha256").update(data).digest("hex");
}

export function idpId(tag: string, payload: Buffer): string {
  const h = createHash("sha256");
  h.update(Buffer.from("idp/1", "utf8"));
  h.update(SEP);
  h.update(Buffer.from(tag, "utf8"));
  h.update(SEP);
  h.update(payload);
  return "sha256:" + h.digest("hex");
}

/** `address(bytes, media_type).id` — media_type is carried, never hashed. */
export function addressBlob(data: Buffer): string {
  return idpId("blob", data);
}

/** `identify_text` — the text-leaf content id: `idp_id("hir.text", bytes)`
 *  over the *verbatim* content bytes (the `RecordKind::HirTextLeaf` domain
 *  tag; no line-ending normalization exists on the reference path). The
 *  spec-text defect E2 recorded — an earlier CHECKER.md spelling of
 *  `idp_id("text", normalized)` — is resolved in the current contract text:
 *  CHECKER.md §idp/1 now spells `hir.text` over verbatim bytes itself. */
export function identifyText(data: Buffer): string {
  return idpId("hir.text", data);
}

/** `<n>.input.json`'s `bytes_hex` member → raw content bytes. */
export function hexDecode(s: string): Buffer {
  return Buffer.from(s, "hex");
}
