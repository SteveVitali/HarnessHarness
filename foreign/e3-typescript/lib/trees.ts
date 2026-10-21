// lib/trees.ts — `tree/1` addressing + the `identity-trees` arm (R4.4).
//
// The committed `.canonical.json` members show the canonical tree document
// is the *Merkelized* form of the manifest input: a file's `bytes_hex`
// becomes `content` (the blob content-address) and a dir's `entries`
// becomes `tree` (the recursive tree id). Tree address =
// `idp_id("tree", canonical(doc))`.

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import {
  addressBlob,
  canonical,
  hexDecode,
  idpId,
  parseJson,
} from "./core.ts";
import type { Json } from "./core.ts";

function isObj(v: Json | undefined): v is { [key: string]: Json } {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/** Manifest → canonical tree document (and its id, computed bottom-up). */
export function treeCanonicalDoc(manifest: Json): [{ [key: string]: Json }, string] {
  if (!isObj(manifest)) throw new Error("tree manifest must be an object");
  // `Object.create(null)`: a `__proto__`-named entry must land as a real
  // own property (the prototype setter would silently drop it).
  const out: { [key: string]: Json } = Object.create(null);
  for (const [name, node] of Object.entries(manifest)) {
    const kind = isObj(node) ? node["kind"] : undefined;
    if (kind === "file" && isObj(node)) {
      out[name] = {
        content: addressBlob(hexDecode(node["bytes_hex"] as string)),
        exec: node["exec"] === true,
        kind: "file",
      };
    } else if (kind === "symlink" && isObj(node)) {
      out[name] = { kind: "symlink", target: node["target"] };
    } else if (kind === "dir" && isObj(node)) {
      const [, subId] = treeCanonicalDoc(node["entries"] as Json);
      out[name] = { kind: "dir", tree: subId };
    } else {
      throw new Error(`${name}: unknown entry kind ${JSON.stringify(kind)}`);
    }
  }
  const docBytes = canonical(out);
  return [out, idpId("tree", docBytes)];
}

/** `identity-trees`: `id` string per `<n>.input.json` case. */
export function armIdentityTrees(armDir: string): { [key: string]: Json } {
  const answers: { [key: string]: Json } = {};
  const cases = readdirSync(armDir)
    .filter((f) => f.endsWith(".input.json"))
    .sort();
  for (const f of cases) {
    const caseName = f.slice(0, -".input.json".length);
    const manifest = parseJson(readFileSync(join(armDir, f)));
    const [, treeId] = treeCanonicalDoc(manifest);
    answers[caseName] = treeId;
  }
  return answers;
}
