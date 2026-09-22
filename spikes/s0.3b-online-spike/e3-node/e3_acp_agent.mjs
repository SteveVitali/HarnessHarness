#!/usr/bin/env node
// S0.3b — local reference ACP agent (throwaway, E3), via the OFFICIAL ACP SDK over stdio ndjson.
// Fixed local reference peer for M-S1-7 (l1-spike-spec s3.1 item 5). No model spend: prompt returns
// a fixed end_turn stop immediately.
import { AgentSideConnection, ndJsonStream, PROTOCOL_VERSION } from '@zed-industries/agent-client-protocol';
import { Writable, Readable } from 'node:stream';

class ReferenceAgent {
  async initialize(params) {
    return { protocolVersion: params.protocolVersion ?? PROTOCOL_VERSION, agentCapabilities: {} };
  }
  async newSession() {
    return { sessionId: 's03b-e3-ref-1' };
  }
  async prompt() {
    return { stopReason: 'end_turn' };
  }
  async cancel() {}
}

const stream = ndJsonStream(
  Writable.toWeb(process.stdout),
  Readable.toWeb(process.stdin)
);
new AgentSideConnection(() => new ReferenceAgent(), stream);
