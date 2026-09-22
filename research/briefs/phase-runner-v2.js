export const meta = {
  name: 'hh-phase-runner-v2',
  description: 'HarnessHarness research phase: file-based briefs, research agents in waves, then synthesis + gate report',
  phases: [
    { title: 'Research', detail: 'one fresh research agent per workstream, waves' },
    { title: 'Synthesis', detail: 'fold registers, disposition ADRs, converge contracts, gate check' },
  ],
}

// args: { phase: "2", wave: 8, ids: ["WS-H1", ...], precompleted: ["WS-A4", ...] (optional), synthOnly: false }
const P = args
const ROOT = '/Users/stevenvitali/MetaHarness'
const BRIEFS = `${ROOT}/research/briefs/phase-${P.phase}`

const RESEARCH_SCHEMA = {
  type: 'object',
  properties: {
    ws_id: { type: 'string' },
    dossier_written: { type: 'boolean' },
    adrs_proposed: { type: 'array', items: { type: 'string' } },
    open_questions: { type: 'number' },
    conflicts_flagged: { type: 'number' },
    sources_added: { type: 'number' },
    confidence: { type: 'string', enum: ['high', 'medium', 'low'] },
    language_neutral: { type: 'boolean' },
    notes_for_synthesis: { type: 'string' },
  },
  required: ['ws_id', 'dossier_written', 'adrs_proposed', 'confidence', 'language_neutral', 'notes_for_synthesis'],
}

const SYNTH_SCHEMA = {
  type: 'object',
  properties: {
    phase: { type: 'string' },
    gate_pass: { type: 'boolean' },
    blockers: { type: 'array', items: { type: 'string' } },
    workstreams_done: { type: 'array', items: { type: 'string' } },
    adrs_ratified: { type: 'array', items: { type: 'string' } },
    adrs_amended: { type: 'array', items: { type: 'string' } },
    adrs_rejected: { type: 'array', items: { type: 'string' } },
    conflicts_open: { type: 'array', items: { type: 'string' } },
    language_leaks_found: { type: 'array', items: { type: 'string' } },
    summary_path: { type: 'string' },
    progress_summary: { type: 'string' },
  },
  required: ['phase', 'gate_pass', 'blockers', 'workstreams_done', 'adrs_ratified', 'summary_path', 'progress_summary'],
}

function researchPrompt(id) {
  return `You are a fresh research subagent in the HarnessHarness program (Phase ${P.phase}). You own workstream **${id}**.
Your complete brief is the file ${BRIEFS}/${id}.md — read it in full FIRST and execute it exactly. It names every file to read, every artifact to persist (dossier, proposed ADRs, register-additions sidecar), the evidence discipline, and the return contract.
If ${ROOT}/research/dossiers/${id}.md already exists, a previous interrupted run wrote it: read it, verify and complete it rather than starting over, and do not duplicate proposed ADR files.
Persist all artifacts before returning. Return ONLY the structured JSON (schema enforced): adrs_proposed = list of proposed ADR file paths; notes_for_synthesis ≤ 6 bullets.`
}

function synthesisPrompt(results) {
  const digest = results.map(r => `- ${r.ws_id}: dossier_written=${r.dossier_written}; ADRs proposed: ${(r.adrs_proposed || []).join(', ') || 'none'}; confidence ${r.confidence}; notes: ${r.notes_for_synthesis || ''}`).join('\n')
  return `You are the Phase ${P.phase} synthesis agent for the HarnessHarness program. Your complete brief is the file ${BRIEFS}/synthesis.md — read it in full FIRST and execute it exactly (fold registers, reconcile conflicts, disposition every proposed ADR, audit neutrality/naming, update LEDGER/scope, write the synthesis memo, enforce the gate).

## Per-workstream results digest (from the research agents)
${digest}

Return ONLY the structured JSON (schema enforced). progress_summary ≈ 150 words for the human sponsor.`
}

const results = []
if (!P.synthOnly) {
  phase('Research')
  const WAVE = P.wave || 8
  const ids = P.ids || []
  for (let i = 0; i < ids.length; i += WAVE) {
    const wave = ids.slice(i, i + WAVE)
    log(`Phase ${P.phase} research wave ${Math.floor(i / WAVE) + 1}/${Math.ceil(ids.length / WAVE)}: ${wave.join(', ')}`)
    const out = await parallel(wave.map(id => () =>
      agent(researchPrompt(id), { label: `research:${id}`, phase: 'Research', schema: RESEARCH_SCHEMA, agentType: 'general-purpose' })
    ))
    out.forEach((r, k) => {
      if (!r) log(`WARNING: ${wave[k]} returned null (failed/skipped)`)
      else log(`${r.ws_id}: dossier=${r.dossier_written}, ${r.adrs_proposed.length} ADR(s), confidence ${r.confidence}`)
    })
    results.push(...out.map((r, k) => r || { ws_id: wave[k], dossier_written: false, adrs_proposed: [], confidence: 'low', language_neutral: true, notes_for_synthesis: 'AGENT FAILED — no result; check disk for partial artifacts' }))
  }
}
for (const id of (P.precompleted || [])) {
  results.push({ ws_id: id, dossier_written: true, adrs_proposed: ['see research/decisions/proposed/' + id + '-adr-*.md'], confidence: 'see dossier §10', language_neutral: true, notes_for_synthesis: 'Completed in an earlier run; read dossier §10–§12 and sidecar for its notes.' })
}
const failed = results.filter(r => !r.dossier_written).map(r => r.ws_id)
if (failed.length) log(`Workstreams without a dossier: ${failed.join(', ')}`)

phase('Synthesis')
const synth = await agent(synthesisPrompt(results), { label: `synthesis:phase-${P.phase}`, phase: 'Synthesis', schema: SYNTH_SCHEMA, agentType: 'general-purpose', effort: 'high' })

return { phase: P.phase, research: results.map(r => ({ ws_id: r.ws_id, dossier_written: r.dossier_written, adrs: (r.adrs_proposed || []).length, confidence: r.confidence })), failed, synthesis: synth }
