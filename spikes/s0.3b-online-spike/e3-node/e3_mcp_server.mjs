#!/usr/bin/env node
// S0.3b — local reference MCP `echo` server (throwaway, E3), via the OFFICIAL MCP SDK over stdio.
// The fixed local reference peer for M-S1-7 (l1-spike-spec s3.1 item 4). No network, no model spend.
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { z } from 'zod';

const server = new McpServer({ name: 's03b-echo', version: '0.0.0' });
server.registerTool(
  'echo',
  { description: 'Echo the input back unchanged', inputSchema: { text: z.string() } },
  async ({ text }) => ({ content: [{ type: 'text', text }] })
);
await server.connect(new StdioServerTransport());
