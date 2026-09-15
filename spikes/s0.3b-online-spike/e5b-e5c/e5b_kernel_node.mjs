#!/usr/bin/env node
// S0.3b — E5b KERNEL (E3 ecosystem) driven by an E2 lab over the kernel<->lab boundary
// (ADR-0050 M3a: subprocess + newline-delimited JSON-RPC 2.0 over stdio, the WS-L1 s6.5 verb shape).
// Modes:
//   native <n>          build n sessions in-process, print internal build wall (baseline arm)
//   serve <corpus>      answer JSON-RPC {hello, run_session} on stdin, one JSON object per line
// Throwaway; reuses e5_chain (byte-identical to the E1 reference head, gate G1).
import { runSession } from './e5_chain.mjs';
import { readFileSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
import { createInterface } from 'node:readline';

const args = process.argv.slice(2);
const mode = args[0];

if (mode === 'native') {
  const corpus = JSON.parse(readFileSync(args[1]).toString('utf8'));
  const n = parseInt(args[2], 10);
  const t0 = performance.now();
  for (let i = 0; i < n; i++) runSession(corpus, 'run-' + i);
  const wall = (performance.now() - t0) * 1000; // micros
  console.log('native_build_wall_us ' + Math.round(wall));
  process.exit(0);
}

if (mode === 'serve') {
  const corpus = JSON.parse(readFileSync(args[1]).toString('utf8'));
  const rl = createInterface({ input: process.stdin });
  rl.on('line', (line) => {
    line = line.trim();
    if (!line) return;
    let req;
    try {
      req = JSON.parse(line);
    } catch {
      return;
    }
    let result;
    if (req.method === 'hello') {
      result = { contract: 'hh-embed/1', kernel_candidate: 'E3', protocol: 'jsonrpc-2.0-stdio' };
    } else if (req.method === 'run_session') {
      result = runSession(corpus, req.params.run_id);
    } else {
      process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: req.id, error: { code: -32601, message: 'method not found' } }) + '\n');
      return;
    }
    process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: req.id, result }) + '\n');
  });
}
