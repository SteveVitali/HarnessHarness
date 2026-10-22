// lib/records.ts — the `identity-records` arm: blob / text / record forms
// (CHECKER.md §Arm replay). For `record` inputs:
//   version_id  = idp_id(domain_tag, canonical(canonical_full))
//   semantic_id = idp_id(domain_tag + "#semantic", canonical(canonical_semantic))
//                 or null when canonical_semantic is null.

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import {
  addressBlob,
  canonical,
  hexDecode,
  identifyText,
  idpId,
  parseJson,
} from "./core.ts";
import type { Json } from "./core.ts";

function isObj(v: Json | undefined): v is { [key: string]: Json } {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

export function armIdentityRecords(armDir: string): { [key: string]: Json } {
  // `Object.create(null)` — a `__proto__`-named case lands as a real key.
  const answers: { [key: string]: Json } = Object.create(null);
  const cases = readdirSync(armDir)
    .filter((f) => f.endsWith(".input.json"))
    .sort();
  for (const f of cases) {
    const caseName = f.slice(0, -".input.json".length);
    const doc = parseJson(readFileSync(join(armDir, f)));
    if (!isObj(doc)) throw new Error(`${caseName}: input must be an object`);
    const arm = doc["arm"];
    if (arm === "blob") {
      const data = hexDecode(doc["bytes_hex"] as string);
      answers[caseName] = { version_id: addressBlob(data), semantic_id: null };
    } else if (arm === "text") {
      const data = hexDecode(doc["bytes_hex"] as string);
      answers[caseName] = { version_id: identifyText(data), semantic_id: null };
    } else if (arm === "record") {
      const tag = doc["domain_tag"] as string;
      const versionId = idpId(tag, canonical(doc["canonical_full"] as Json));
      const sem = doc["canonical_semantic"];
      const semanticId =
        sem === null || sem === undefined
          ? null
          : idpId(tag + "#semantic", canonical(sem));
      answers[caseName] = { version_id: versionId, semantic_id: semanticId };
    } else {
      throw new Error(`${caseName}: unknown record arm tag ${JSON.stringify(arm)}`);
    }
  }
  return answers;
}
