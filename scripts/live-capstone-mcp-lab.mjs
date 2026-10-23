// L4.6a — the `mcp-lab-live` leg's official-client arm (ADR-0363): drive
// `hh-mcp-lab`'s `LabServer` HTTP binding through the official
// `@modelcontextprotocol/sdk` client — `initialize` → `tools/list` →
// `tools/call launch_run` → `tools/call submit_input` → `resources/list`
// → `resources/read`. The SDK is installed runtime-only into a temp dir
// (npm --prefix — never a repo dep, never committed).
//
// Usage (from inside the SDK install dir so ESM resolves node_modules):
//   node live-capstone-mcp-lab.mjs <url> <bearer> <launch-args.json>
//
// Exit 0 on pass, non-zero otherwise; prints MCP_LAB_* markers the leg
// folds into the evidence record (MCP_LAB_LAUNCH carries the child run id
// the verify+replay half re-reads off the durable store).

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { readFileSync } from "node:fs";

const [url, bearer, argsFile] = process.argv.slice(2);
if (!url || !bearer || !argsFile) {
  console.error("usage: node live-capstone-mcp-lab.mjs <url> <bearer> <launch-args.json>");
  process.exit(2);
}
const launchArgs = JSON.parse(readFileSync(argsFile, "utf8"));

const transport = new StreamableHTTPClientTransport(new URL(url), {
  requestInit: { headers: { Authorization: `Bearer ${bearer}` } },
});
const client = new Client(
  { name: "l4.6a-mcp-lab", version: "1" },
  { capabilities: {} },
);

const structured = (res) => res?.structuredContent ?? null;

try {
  await client.connect(transport);
  const info = client.getServerVersion?.();
  console.log(
    `MCP_LAB_BINDING: official-sdk version=${client.getProtocolVersion?.() ?? "?"} server=${info?.name ?? "?"}`,
  );

  const listed = await client.listTools();
  const names = (listed.tools ?? []).map((t) => t.name);
  for (const want of ["launch_run", "submit_input", "cancel_run", "respond_approval"]) {
    if (!names.includes(want)) {
      console.error(`tools/list missing ${want}: ${names}`);
      process.exit(3);
    }
  }
  console.log(`MCP_LAB_TOOLS: ${names.join(",")}`);

  const launch = await client.callTool({ name: "launch_run", arguments: launchArgs });
  if (launch?.isError) {
    console.error(`launch_run isError: ${JSON.stringify(launch).slice(0, 400)}`);
    process.exit(4);
  }
  const sc = structured(launch) ?? {};
  const runId = sc?.launched?.run_id;
  const handle = sc?.handles?.[0]?.handle;
  if (!runId || !handle) {
    console.error(`launch_run missing launched.run_id/handle: ${JSON.stringify(sc).slice(0, 400)}`);
    process.exit(5);
  }
  console.log(`MCP_LAB_LAUNCH: run=${runId} handle=${handle}`);

  const submit = await client.callTool({
    name: "submit_input",
    arguments: {
      run: handle,
      content:
        'You have exactly one tool available: hh_submit, which takes a string parameter named text. Call hh_submit now with text "done". If you cannot call tools, reply with the single word done.',
      idempotency_key: "l46a-live-sub",
    },
  });
  if (submit?.isError) {
    console.error(`submit_input isError: ${JSON.stringify(submit).slice(0, 400)}`);
    process.exit(6);
  }
  console.log("MCP_LAB_SUBMIT: ok");

  const resources = await client.listResources();
  const uris = (resources.resources ?? []).map((r) => r.uri);
  if (uris.length === 0) {
    console.error("resources/list empty");
    process.exit(7);
  }
  console.log(`MCP_LAB_RESOURCES: ${uris.length}`);
  const ledgerUri = uris.find((u) => u.includes("ledger")) ?? uris[0];
  const read = await client.readResource({ uri: ledgerUri });
  if (!read || JSON.stringify(read).length <= 2) {
    console.error(`resources/read empty for ${ledgerUri}`);
    process.exit(8);
  }
  console.log(`MCP_LAB_READ: uri=${ledgerUri}`);
  console.log("MCP_LAB_RESULT: official-sdk pass");
  await client.close();
  process.exit(0);
} catch (e) {
  console.error(`official-sdk driver failed: ${e}`);
  process.exit(9);
}
