// lib/ledger.ts — ledger WAL replay (`events.wal`, DF-S1.5-3):
// the `ledger-transcript` arm.
//
//   prev = "genesis"; per `{"k":"e","v":env}` frame:
//   hash = idp_id("ledger.event", 0x00 ‖ canonical(env − {"hash"}) ‖ prev_hash)
//
// `c` frames are commits — skipped. Every frame must already be canonical
// JSON (the committed corpus is the pinned form — a drifted frame is a
// failure, never silently reserialized).

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { canonical, idpId, parseJson, sha256Hex } from "./core.ts";
import type { Json } from "./core.ts";

function isObj(v: Json | undefined): v is { [key: string]: Json } {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/** The shared `events.wal` fold — one spelling for every ledger arm
 *  (R4.4a's `ledger-scenarios` reuses it per case dir). Returns the head
 *  hash and the recorded per-event hashes; `walSha256`/`eventCount` are
 *  informational (carried, never gated — CHECKER.md §answers.json). */
export function replayWal(walPath: string): {
  headHash: string;
  eventHashes: string[];
  walSha256: string;
  eventCount: number;
} {
  const wal = readFileSync(walPath);
  let prev = "genesis";
  const eventHashes: string[] = [];
  for (const line of wal.toString("utf8").split("\n")) {
    if (line === "") continue;
    const frame = parseJson(line);
    if (!canonical(frame).equals(Buffer.from(line, "utf8"))) {
      throw new Error("WAL frame is not canonical JSON");
    }
    if (!isObj(frame)) throw new Error("WAL frame must be an object");
    const k = frame["k"];
    if (k === "c") continue;
    if (k !== "e") throw new Error(`unknown WAL frame kind ${JSON.stringify(k)}`);
    const env = frame["v"];
    if (!isObj(env)) throw new Error("WAL event frame must carry an object envelope");
    const recorded = env["hash"] as string;
    // `Object.create(null)` — see trees.ts (the `__proto__` hazard).
    const preimage: { [key: string]: Json } = Object.create(null);
    for (const [m, v] of Object.entries(env)) {
      if (m !== "hash") preimage[m] = v;
    }
    const computed = idpId(
      "ledger.event",
      Buffer.concat([
        Buffer.from([0x00]),
        canonical(preimage),
        Buffer.from(prev, "utf8"),
      ]),
    );
    if (computed !== recorded) {
      throw new Error(`recomputed ${computed} != recorded ${recorded}`);
    }
    prev = recorded;
    eventHashes.push(recorded);
  }
  return {
    headHash: prev,
    eventHashes,
    walSha256: sha256Hex(wal),
    eventCount: eventHashes.length,
  };
}

export function armLedgerTranscript(armDir: string): { [key: string]: Json } {
  const r = replayWal(join(armDir, "events.wal"));
  return {
    head_hash: r.headHash,
    event_hashes: r.eventHashes,
    event_count: r.eventCount,
    wal_sha256: r.walSha256,
  };
}
