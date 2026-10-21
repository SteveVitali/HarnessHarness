#!/usr/bin/env node
// foreign.ts — the E3-candidate (TypeScript, node type-stripping, stdlib
// only) replay of the `hh-xcheck-bundle/1` contract (CHECKER.md, spec
// §10.7, R2.21, ADR-0353) — part 1 (R4.4): the shared `lib/` core plus
// the identity-trees/records, ledger-transcript, registry-snapshot and
// canonical-parse arms. R4.4a completes the remaining arms against the
// same core.
//
//   node foreign/e3-typescript/foreign.ts <bundle-dir> <answers.json>
//
// Reads a cross-impl bundle directory, replays the arms present, and
// writes an `answers.json` that `hh-xcheck verify <bundle> <answers>`
// compares against the bundle's committed E1 expectations. The arms R4.4a
// owns are absent — `verify` reports their cells `missing answer` by
// name (honest pending, never a fabricated or vacuous answer).
//
// Honesty note: this is a *candidate* implementation. Its answers are
// evidence the foreign run produced; a green `verify` does not mint E1
// truth.

import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { canonical } from "./lib/core.ts";
import type { Json } from "./lib/core.ts";
import { armIdentityTrees } from "./lib/trees.ts";
import { armIdentityRecords } from "./lib/records.ts";
import { armLedgerTranscript } from "./lib/ledger.ts";
import { armRegistrySnapshot } from "./lib/registry.ts";
import { armCanonicalParse } from "./lib/canonical_parse.ts";

function main(argv: string[]): number {
  if (argv.length !== 2) {
    console.error("usage: node foreign.ts <bundle-dir> <answers.json>");
    return 2;
  }
  const [bundle, out] = argv;
  const armsDir = join(bundle, "arms");
  const answers: { [key: string]: Json } = {
    candidate: "E3-candidate",
    arms: {
      "identity-trees": armIdentityTrees(join(armsDir, "identity-trees")),
      "identity-records": armIdentityRecords(join(armsDir, "identity-records")),
      "ledger-transcript": armLedgerTranscript(join(armsDir, "ledger-transcript")),
      "registry-snapshot": armRegistrySnapshot(join(armsDir, "registry-snapshot")),
      "canonical-parse": armCanonicalParse(join(armsDir, "canonical-parse")),
    },
  };
  writeFileSync(out, Buffer.concat([canonical(answers as Json), Buffer.from("\n")]));
  const armCount = Object.keys(answers["arms"] as { [key: string]: Json }).length;
  console.log(`wrote ${out} (candidate E3-candidate, ${armCount} arm answers)`);
  return 0;
}

process.exitCode = main(process.argv.slice(2));
