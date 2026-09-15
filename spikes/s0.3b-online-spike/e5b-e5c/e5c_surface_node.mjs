#!/usr/bin/env node
// S0.3b — E5c SURFACE side (E3 ecosystem) driving the E2 kernel over the kernel<->surfaces boundary
// (localhost HTTP). Matched E2<->E2 for the transport cost (CC9): per-run crossing overhead =
// (split wall driving the E2 kernel over HTTP) - (the SAME E2 kernel run natively), /N. Verifies
// far-side (E3) hash-equality of every event (M-S2-7): the surface re-canonicalises the chain with
// its OWN serializer and every event hash must match. No network egress (127.0.0.1), no model spend.
import { runSession } from './e5_chain.mjs';
import { spawn, execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const N = 50;
const KERNEL = join(HERE, 'e5c_kernel_python.py');
const corpusPath = process.argv[2];

async function post(port, path, body) {
  const res = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json();
}

function verifyFarSide(corpus, runId, kernelEvents) {
  // far-side (E3) re-canonicalisation: recompute and compare every event hash.
  const { events } = runSession(corpus, runId);
  let matched = 0;
  for (let i = 0; i < events.length; i++) {
    if (kernelEvents[i].seq === events[i].seq && kernelEvents[i].hash === events[i].hash) matched++;
  }
  return matched;
}

async function main() {
  const corpus = JSON.parse(readFileSync(corpusPath).toString('utf8'));

  // native baseline (E2 kernel, no boundary)
  const nativeOut = execFileSync('python3', [KERNEL, 'native', corpusPath, String(N)]).toString('utf8');
  const nativeUs = parseInt(nativeOut.match(/native_build_wall_us (\d+)/)[1], 10);

  // start the E2 kernel HTTP server
  const port = 8731;
  const kernel = spawn('python3', [KERNEL, 'serve', corpusPath, String(port)], { stdio: ['ignore', 'ignore', 'pipe'] });
  await new Promise((resolve, reject) => {
    kernel.stderr.on('data', (d) => {
      if (d.toString().includes('LISTENING')) resolve();
    });
    setTimeout(() => reject(new Error('kernel start timeout')), 5000);
  });

  const hello = await post(port, '/hello', {});
  if (hello.kernel_candidate !== 'E2') throw new Error('handshake');
  await post(port, '/run_session', { run_id: 'warm' });

  // TIMED: transport only
  const results = [];
  const t0 = performance.now();
  for (let i = 0; i < N; i++) {
    const runId = 'run-' + i;
    const res = await post(port, '/run_session', { run_id: runId });
    results.push([runId, res]);
  }
  const splitUs = (performance.now() - t0) * 1000;

  // UNTIMED: far-side hash-equality (M-S2-7)
  let total = 0;
  let matched = 0;
  for (const [runId, res] of results) {
    matched += verifyFarSide(corpus, runId, res.events);
    total += res.events.length;
  }

  kernel.kill();

  const perRunUs = (splitUs - nativeUs) / N;
  const nativePerRunUs = nativeUs / N;
  const pct = nativePerRunUs ? 100 * (perRunUs / nativePerRunUs) : 0;
  const hashPct = total ? (100 * matched) / total : 0;

  console.log('split E5c');
  console.log('m_s2_1_split_wall_us ' + Math.round(splitUs));
  console.log('m_s2_1_native_wall_us ' + nativeUs);
  console.log('m_s2_1_per_run_overhead_us ' + perRunUs.toFixed(1));
  console.log('m_s2_1_per_run_overhead_pct ' + pct.toFixed(3));
  console.log('m_s2_7_hash_equal_pct ' + hashPct.toFixed(1));
  console.log('m_s2_7_events_verified ' + total);
  console.log('e5c_far_side_candidate E3');
  console.log('e5c_kernel_candidate E2');
  process.exit(0);
}

main();
