// L4.6a — the `capstone-acp` official-client arm (ADR-0363): drive the
// shipped `hh acp` verb through the official `@zed-industries/
// agent-client-protocol` `ClientSideConnection`. `hh acp` needs a TTY
// stdin (the verb's serve loop never runs under the piped-stdin
// pre-read), so the driver spawns BSD `script -q /dev/null` with
// `stty -echo` — a real pty, request bytes never echoed onto the frame
// stream — and wraps the child's stdio pipes as web streams for
// `ndJsonStream`. The SDK is installed runtime-only into a temp dir
// (never a repo dep, never committed). Usage:
//   node live-capstone-acp.mjs <hh-bin> <def.json> <profile-coord> <workdir>
// from inside the SDK install dir so ESM resolves node_modules.
// Exit 0 on a completed exchange, non-zero otherwise; ACP_* markers
// record which dialect divergence (if any) the pin negotiation hit.
import { spawn } from "node:child_process";
import { Readable } from "node:stream";
import { Writable } from "node:stream";
import * as acp from "@zed-industries/agent-client-protocol";

const [hh, def, coord, workdir, capability] = process.argv.slice(2);
if (!hh || !def || !coord || !workdir) {
  console.error("usage: node live-capstone-acp.mjs <hh-bin> <def.json> <profile-coord> <workdir> [capability-json]");
  process.exit(2);
}

// Identical to the in-repo cell's REFERENCE_TASK — `hh.submit` declares an
// empty parameter spec; a `text` argument is a schema_violation under the
// closed-world output check.
const TASK =
  'You have exactly one tool available: hh_submit, which takes no arguments. Call hh_submit now with an empty arguments object {}. If you cannot call tools, reply with the single word done.';

// The pty spawn — the identical transport shape the in-repo arm uses:
// `script`'s stdin/stdout pipes carry the newline-delimited frames;
// the child's stderr lands in a file (progress lines never poison the
// frame stream). node gives spawned children a socketpair for stdio —
// BSD `script` cannot tcgetattr a socket (`Operation not supported on
// socket` → dead pty → EPIPE on the first frame), so `cat` interposes a
// real pipe between the socketpair and script's stdin.
const capArg = capability ? ` --capability "$4"` : "";
const inner = `stty -echo; exec "$0" acp "$1" --profile "$2"${capArg} 2>"$3"`;
const child = spawn(
  "sh",
  [
    "-c",
    'cat | script -q /dev/null sh -c "$1" "$2" "$3" "$4" "$5" "$6"',
    "acp-driver",
    inner,
    hh,
    def,
    coord,
    `${workdir}/acp-official.stderr.log`,
    capability ?? "",
  ],
  {
    env: process.env,
    stdio: ["pipe", "pipe", "inherit"],
  }
);

child.on("error", (e) => {
  console.error(`pty spawn failed: ${e}`);
  process.exit(5);
});

// Node streams → web streams (the SDK's ndJsonStream wants the WHATWG
// pair — Uint8Array chunks both directions).
const toWeb = new ReadableStream({
  start(c) {
    child.stdout.on("data", (d) => c.enqueue(new Uint8Array(d)));
    child.stdout.on("end", () => c.close());
    child.stdout.on("error", (e) => c.error(e));
  },
});
const fromWeb = new WritableStream({
  write(chunk) {
    return new Promise((resolve, reject) => {
      child.stdin.write(Buffer.from(chunk), (e) => (e ? reject(e) : resolve()));
    });
  },
  close() {
    child.stdin.end();
  },
});
// Let `script`'s pty settle: the inner `stty -echo` races our first
// write — a frame that lands before it applies echoes back and the
// SDK's duplex reads it as an incoming request (its methodNotFound
// reply then rejects the pending initialize). A bounded settle window
// closes the race deterministically (the in-repo client ignores
// echoed request frames; the SDK does not).
await new Promise((r) => setTimeout(r, 500));

const stream = acp.ndJsonStream(fromWeb, toWeb);

let sawUpdate = 0;
let sawPermission = 0;
const client = {
  sessionUpdate: async (params) => {
    sawUpdate += 1;
    if (sawUpdate <= 6) {
      console.log(`ACP_UPDATE: ${params?.update?.sessionUpdate ?? "?"}`);
    }
  },
  requestPermission: async (params) => {
    sawPermission += 1;
    console.log(`ACP_PERMISSION: ${JSON.stringify(params?.options ?? params).slice(0, 200)}`);
    return { outcome: { outcome: "selected", optionId: params?.options?.[0]?.optionId ?? "allow_once" } };
  },
  readTextFile: async () => ({ content: "" }),
  writeTextFile: async () => ({}),
};

const conn = new acp.ClientSideConnection(() => client, stream);

async function tryInitialize(offered) {
  try {
    const r = await conn.initialize({
      protocolVersion: offered,
      clientInfo: { name: "l46a-official", version: "1" },
      clientCapabilities: {},
    });
    console.log(`ACP_INIT: offered=${JSON.stringify(offered)} negotiated=${JSON.stringify(r.protocolVersion)}`);
    return true;
  } catch (e) {
    console.log(`ACP_INIT_REFUSED: offered=${JSON.stringify(offered)} ${e}`);
    return false;
  }
}

try {
  // The official pin is integer 1 (ACP v1); the hh-acp dialect pin set
  // is ["0.1.0","v1-compat"] — try the spec-correct offer first, then
  // the dialect's own label so the record says exactly which offer the
  // peer admitted (a refusal of both is the honest pin-divergence row).
  let ok = await tryInitialize(acp.PROTOCOL_VERSION ?? 1);
  if (!ok) ok = await tryInitialize("v1-compat");
  if (!ok) ok = await tryInitialize("0.1.0");
  if (!ok) {
    console.error("ACP_PIN: no offered version negotiated — official client cannot bind this dialect");
    child.kill();
    process.exit(6);
  }

  const session = await conn.newSession({ cwd: workdir, mcpServers: [] });
  const sid = session.sessionId;
  console.log(`ACP_SESSION: ${sid}`);

  // The official PromptRequest member is `prompt`; the hh-acp dialect
  // reads `content` — a dialect divergence the record names (the turn
  // still drives; the permission round-trip is the in-repo arm's
  // assertion when the member lowering differs).
  const prompt = await conn.prompt({
    sessionId: sid,
    prompt: [{ type: "text", text: TASK }],
  });
  console.log(`ACP_PROMPT: stop=${prompt?.stopReason ?? "?"} updates=${sawUpdate} permissions=${sawPermission}`);
  // `extMethod` prepends `_` itself — pass the bare member name.
  try {
    const acct = await conn.extMethod("hh/account", {});
    console.log(`ACP_ACCOUNT: ${JSON.stringify(acct).slice(0, 200)}`);
  } catch (e) {
    console.log(`ACP_ACCOUNT: ${JSON.stringify(e)}`);
  }
  try {
    const tail = await conn.extMethod("hh/ledger/read", { from_seq: 0 });
    console.log(`ACP_LEDGER_READ: ${JSON.stringify(tail).length} bytes`);
  } catch (e) {
    console.log(`ACP_LEDGER_READ: ${JSON.stringify(e)}`);
  }
  child.kill();
  process.exit(0);
} catch (e) {
  console.error(`official-sdk driver failed: ${e}`);
  child.kill();
  process.exit(4);
}
