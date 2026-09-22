#!/usr/bin/env node
// S0.3b — M-S1-7 MCP call round-trip through the OFFICIAL MCP SDK (E3), against the local
// reference echo server. Emits key/value lines. No network egress, no model calls.
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const N_CALLS = 20;

const transport = new StdioClientTransport({
  command: process.execPath,
  args: [join(HERE, 'e3_mcp_server.mjs')],
});
const client = new Client({ name: 's03b-e3-client', version: '0.0.0' });

const tSetup = performance.now();
await client.connect(transport);
const tools = await client.listTools();
const setupMs = performance.now() - tSetup;
if (!tools.tools.some((t) => t.name === 'echo')) throw new Error('echo tool missing');

await client.callTool({ name: 'echo', arguments: { text: 'warm' } });
const samples = [];
for (let i = 0; i < N_CALLS; i++) {
  const s = performance.now();
  const res = await client.callTool({ name: 'echo', arguments: { text: 'ping-' + i } });
  samples.push(performance.now() - s);
  const got = res.content && res.content[0] ? res.content[0].text : null;
  if (got !== 'ping-' + i) throw new Error('echo mismatch: ' + got);
}
await client.close();
samples.sort((a, b) => a - b);
const median = samples[Math.floor(samples.length / 2)];
const p95 = samples[Math.floor(samples.length * 0.95)];
console.log('candidate E3');
console.log('m_s1_7_transport mcp_stdio_official_sdk');
console.log('m_s1_7_mcp_setup_ms ' + setupMs.toFixed(3));
console.log('m_s1_7_mcp_call_median_ms ' + median.toFixed(3));
console.log('m_s1_7_mcp_call_p95_ms ' + p95.toFixed(3));
console.log('m_s1_7_mcp_calls ' + N_CALLS);
console.log('m_s1_7_mcp_echo_verified true');
process.exit(0);
