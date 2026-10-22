// lib/canonical_parse.ts — the `canonical-parse` arm: decode each
// `frames/*.json` member through the `hh-json/1` codec, re-emit, and answer
// the input bytes' SHA-256. `parse_ns`/`rehash_ns` are recorded, never
// gated (timing members — CHECKER.md §answers.json); a frame that fails
// the self-canonical round-trip is noted honestly, never hidden.

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { canonical, parseJson, sha256Hex } from "./core.ts";
import type { Json } from "./core.ts";

export function armCanonicalParse(armDir: string): { [key: string]: Json } {
  // `Object.create(null)` — a `__proto__`-named case lands as a real key.
  const answers: { [key: string]: Json } = Object.create(null);
  const framesDir = join(armDir, "frames");
  const cases = readdirSync(framesDir)
    .filter((f) => f.endsWith(".json"))
    .sort();
  for (const f of cases) {
    const caseName = f.slice(0, -".json".length);
    const data = readFileSync(join(framesDir, f));
    const t0 = process.hrtime.bigint();
    const doc = parseJson(data);
    const t1 = process.hrtime.bigint();
    const reemitted = canonical(doc);
    const t2 = process.hrtime.bigint();
    const entry: { [key: string]: Json } = {
      sha256: sha256Hex(data),
      parse_ns: Number(t1 - t0),
      rehash_ns: Number(t2 - t1),
    };
    // Self-canonical honesty check (not part of the compared answer):
    if (!reemitted.equals(data)) {
      entry["self_canonical"] = false;
    }
    answers[caseName] = entry;
  }
  return answers;
}
