// L4.6 — the `capstone-mcp` official-client arm (ADR-0363): drive
// `hh-mcp-serve` through the official `@modelcontextprotocol/sdk`
// client over stdio — `initialize` → `tools/list` → `tools/call`. The
// SDK is installed runtime-only into a temp dir (npx/npm — never a repo
// dep, never committed). Usage: `node live-capstone-mcp.mjs <server-bin>
// [server-args...]` from inside the SDK install dir so ESM resolves
// `node_modules`. Exit 0 on pass, non-zero otherwise; prints MCP_*
// markers the leg folds into the evidence record.
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";

const [bin, ...args] = process.argv.slice(2);
if (!bin) {
  console.error("usage: node live-capstone-mcp.mjs <server-bin> [args...]");
  process.exit(2);
}

const transport = new StdioClientTransport({ command: bin, args, stderr: "pipe" });
const client = new Client({ name: "l4.6-capstone", version: "1" }, { capabilities: {} });

try {
  await client.connect(transport);
  const serverInfo = client.getServerVersion();
  console.log(`MCP_BINDING: official-sdk version=${client.getProtocolVersion?.() ?? "?"} server=${serverInfo?.name ?? "?"}`);
  const listed = await client.listTools();
  const names = (listed.tools ?? []).map((t) => t.name);
  console.log(`MCP_TOOLS: ${names.join(",")}`);
  if (!names.includes("l46.echo")) {
    console.error(`tools/list missing the served tool: ${names}`);
    process.exit(3);
  }
  const res = await client.callTool({ name: "l46.echo", arguments: { p: "capstone" } });
  const text = JSON.stringify(res).slice(0, 200);
  console.log(`MCP_CALL: ${text}`);
  console.log("MCP_RESULT: official-sdk pass");
  await client.close();
  process.exit(0);
} catch (e) {
  console.error(`official-sdk driver failed: ${e}`);
  process.exit(4);
}
