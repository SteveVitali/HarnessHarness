// clients/ts/smoke.ts — the E3 (TypeScript) live leg (R4.2; HUMAN-H1's
// "Confirm TypeScript" ruling). Drives the *generated* client
// (`clients/ts/hh-embed.ts` — never hand-edited) against a
// `hh-kernel serve --http` (binding (c)) instance that
// `scripts/e3-smoke.sh` spawns on a free loopback port.
//
// The leg asserts the same facts `hh doctor` reports — connectivity,
// the negotiated `KernelDescriptor` (`version`/`schema_hash`/
// `contract_major`), negotiated capabilities — plus one run's
// open/submit/read and an event-stream read, all through the generated
// client's typed methods. stdlib only, no npm dependencies (node >= 22
// type stripping); every failure exits non-zero — nothing is fabricated.
//
// Usage: node clients/ts/smoke.ts --addr 127.0.0.1:PORT --token T
//        --expect-version V
import { readFileSync } from "node:fs";
import {
  NetClient,
  CONTRACT_MAJOR,
  EXPECTED_SCHEMA_HASH,
  verifyDurableHash,
} from "./hh-embed.ts";
import type { Frame, Json } from "./hh-embed.ts";

function arg(name: string): string {
  const i = process.argv.indexOf(name);
  const v = i >= 0 ? process.argv[i + 1] : undefined;
  if (v === undefined) {
    console.error(`smoke: missing ${name}`);
    process.exit(2);
  }
  return v;
}

const ADDR = arg("--addr");
const TOKEN = arg("--token");
const EXPECT_VERSION = arg("--expect-version");

let failures = 0;
function check(name: string, cond: boolean, detail = ""): void {
  if (cond) {
    console.log(`ok   ${name}${detail ? ` — ${detail}` : ""}`);
  } else {
    failures += 1;
    console.log(`FAIL ${name}${detail ? ` — ${detail}` : ""}`);
  }
}

// The HIR/1 fixture document — the same minimal conforming shape the
// hharness-embed conformance suite resolves (agent + rule + budget +
// permission + Stage-1 assembly slots hh/round_robin + hh/full_window;
// regenerate: print `document_json()` from conformance.rs).
const DEFINITION = JSON.parse(
  readFileSync(new URL("./fixture-definition.json", import.meta.url), "utf8"),
) as Json;

const c = new NetClient(ADDR, TOKEN);

// ── doctor-equivalent: hello negotiation + identity facts ────────────
const h = await c.hello({
  contract_major: CONTRACT_MAJOR,
  client: { name: "e3-smoke", version: "0.0.0", kind: "sdk" },
  capabilities: {},
});
check("hello.connectivity", true, "hello returned a result");
check(
  "hello.kernel.version",
  h.kernel.version === EXPECT_VERSION,
  `kernel=${h.kernel.version} expect=${EXPECT_VERSION}`,
);
check(
  "hello.kernel.schema_hash",
  h.kernel.schema_hash === EXPECTED_SCHEMA_HASH,
  h.kernel.schema_hash,
);
check(
  "hello.kernel.contract_major",
  h.kernel.contract_major === CONTRACT_MAJOR,
  `${h.kernel.contract_major}`,
);
check("hello.kernel.idp", h.kernel.idp.length > 0, h.kernel.idp);
check("hello.negotiated", typeof h.negotiated === "object");
check(
  "hello.stored",
  c.hello_result !== undefined && c.negotiated() === h.negotiated,
);

// ── one run: open_session{new} → stream_events → submit ─────────────
const sess = await c.open_session({
  spec: {
    kind: "new",
    definition: { kind: "document", document: DEFINITION },
    environment: {
      kind: "connection_info",
      connection_info: { class: "local_host" },
    },
    attendance: { value: "async", source: "declared" },
  },
  idempotency_key: "e3-smoke:open",
});
check("open_session.session_id", sess.session_id.length > 0, sess.session_id);
check("open_session.run_id", sess.run_id.length > 0, sess.run_id);

const ticket = await c.stream_events({
  session_id: sess.session_id,
  from: { kind: "seq", seq: 1 },
});
check("stream_events.subscription_id", ticket.subscription_id.length > 0);

const acc = await c.submit({
  session_id: sess.session_id,
  input: [
    { kind: "text", text: "e3 smoke turn", authority: "principal", origin: "human" },
  ],
  idempotency_key: "e3-smoke:submit",
});
check("submit.turn_id", acc.turn_id.length > 0, acc.turn_id);

// ── event-stream read over the same binding ──────────────────────────
await c.drain(2000);
const notes = c
  .takeNotifications()
  .filter((n) => n.subscription_id === ticket.subscription_id);
check("stream.frames", notes.length > 0, `${notes.length} frames`);
const durables = notes
  .map((n) => n.frame)
  .filter((f) => f.kind === "durable") as Array<Extract<Frame, { kind: "durable" }>>;
check("stream.durable", durables.length > 0, `${durables.length} durable`);

// ── read + head — the durable page the CLI's `run events` reads ──────
const head = await c.head({ session_id: sess.session_id });
check("head.seq", head.seq >= 1, `seq=${head.seq}`);
const page = await c.read({
  session_id: sess.session_id,
  cursor: { kind: "seq", seq: 1 },
  limit: 10,
});
check("read.events", page.events.length >= 1, `${page.events.length} events`);
const headFrame = durables.find((f) => f.seq === head.seq);
check(
  "stream.durable_hash",
  headFrame === undefined || verifyDurableHash(headFrame, head.hash),
  headFrame === undefined ? "head not yet in drained frames" : "hash matches",
);

// ── close ────────────────────────────────────────────────────────────
const closed = await c.close({ session_id: sess.session_id, reason: "done" });
check("close.run_id", closed.final.run_id === sess.run_id);

if (failures > 0) {
  console.error(`smoke: ${failures} check(s) failed`);
  process.exit(1);
}
console.log("smoke: all checks passed");
