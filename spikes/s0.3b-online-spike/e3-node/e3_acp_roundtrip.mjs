#!/usr/bin/env node
// S0.3b — M-S1-7 ACP session setup through the OFFICIAL ACP SDK (E3), against the local reference
// agent. Drives initialize -> session/new -> session/prompt -> cancel, timing session setup
// (initialize + newSession). No network egress, no model calls.
import { ClientSideConnection, ndJsonStream, PROTOCOL_VERSION } from '@zed-industries/agent-client-protocol';
import { spawn } from 'node:child_process';
import { Writable, Readable } from 'node:stream';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));

class ReferenceClient {
  async requestPermission() {
    return { outcome: { outcome: 'cancelled' } };
  }
  async sessionUpdate() {}
}

const child = spawn(process.execPath, [join(HERE, 'e3_acp_agent.mjs')], {
  stdio: ['pipe', 'pipe', 'inherit'],
});
const stream = ndJsonStream(Writable.toWeb(child.stdin), Readable.toWeb(child.stdout));
const conn = new ClientSideConnection(() => new ReferenceClient(), stream);

const tSetup = performance.now();
const init = await conn.initialize({ protocolVersion: PROTOCOL_VERSION, clientCapabilities: {} });
const sess = await conn.newSession({ cwd: HERE, mcpServers: [] });
const setupMs = performance.now() - tSetup;

const tPrompt = performance.now();
const resp = await conn.prompt({ sessionId: sess.sessionId, prompt: [{ type: 'text', text: 'ping' }] });
const promptMs = performance.now() - tPrompt;

let cancelOk = false;
try {
  await conn.cancel({ sessionId: sess.sessionId });
  cancelOk = true;
} catch (e) {
  process.stderr.write('acp_cancel_note ' + e.name + '\n');
}
child.stdin.end();
child.kill();

console.log('candidate E3');
console.log('m_s1_7_acp_transport acp_stdio_official_sdk');
console.log('m_s1_7_acp_setup_ms ' + setupMs.toFixed(3));
console.log('m_s1_7_acp_prompt_ms ' + promptMs.toFixed(3));
console.log('m_s1_7_acp_protocol_version ' + init.protocolVersion);
console.log('m_s1_7_acp_stop_reason ' + resp.stopReason);
console.log('m_s1_7_acp_cancel_ok ' + (cancelOk ? 'true' : 'false'));
console.log('m_s1_7_acp_session_verified ' + (sess.sessionId && resp.stopReason === 'end_turn' ? 'true' : 'false'));
process.exit(0);
