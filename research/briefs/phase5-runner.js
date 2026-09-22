export const meta = {
  name: 'hh-phase5-spec-assembly',
  description: 'HarnessHarness Phase 5: author spec sections in parallel, architect/assemble, adversarial multi-lens review, fix loop, readiness gate',
  phases: [
    { title: 'Author', detail: 'one fresh author per spec section' },
    { title: 'Assemble', detail: 'architecture overview, build ladder, CANONICAL_SPEC.md' },
    { title: 'Review', detail: 'six independent adversarial lenses' },
    { title: 'Fix', detail: 'apply findings, re-assemble, re-review blockers' },
    { title: 'Readiness', detail: 'Decompose-Readiness Gate report' },
  ],
}

// args: { sections: ["01", ...] (optional subset), skipAuthor: bool, skipAssemble: bool, maxFixRounds: 2 }
const P = args || {}
const ROOT = '/Users/stevenvitali/MetaHarness'
const B = `${ROOT}/research/briefs/phase-5`
const ALL_SECTIONS = ['01', '02', '03', '05a', '05b', '05c', '05d', '05e', '05f', '05g', '05h', '05i', '06', '07', '08', '10', '11']
const LENSES = ['contradiction', 'completeness', 'dag', 'traceability', 'lcd', 'editorial']

const AUTHOR_SCHEMA = { type: 'object', properties: { section: { type: 'string' }, path: { type: 'string' }, words: { type: 'number' }, r_ids_covered: { type: 'array', items: { type: 'string' } }, adrs_cited: { type: 'array', items: { type: 'string' } }, gaps: { type: 'array', items: { type: 'string' } }, cross_refs_needed: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' } }, required: ['section', 'path', 'words', 'r_ids_covered', 'gaps'] }
const ARCH_SCHEMA = { type: 'object', properties: { spec_path: { type: 'string' }, sections_inlined: { type: 'array', items: { type: 'string' } }, words_total: { type: 'number' }, coverage: { type: 'object', properties: { specified: { type: 'number' }, deferred: { type: 'number' }, missing: { type: 'array', items: { type: 'string' } } } }, dag_valid: { type: 'boolean' }, cross_extension_coupling_found: { type: 'array', items: { type: 'string' } }, gaps_remaining: { type: 'array', items: { type: 'string' } }, deferral_adrs_authored: { type: 'array', items: { type: 'string' } }, glossary_check: { type: 'string' } }, required: ['spec_path', 'sections_inlined', 'dag_valid', 'gaps_remaining', 'glossary_check'] }
const FINDING = { type: 'object', properties: { id: { type: 'string' }, severity: { type: 'string', enum: ['blocker', 'major', 'minor'] }, section: { type: 'string' }, quote: { type: 'string' }, defect: { type: 'string' }, authority: { type: 'string' }, fix: { type: 'string' } }, required: ['id', 'severity', 'section', 'defect', 'fix'] }
const REVIEW_SCHEMA = { type: 'object', properties: { lens: { type: 'string' }, findings: { type: 'array', items: FINDING }, sampled: { type: 'number' }, summary: { type: 'string' } }, required: ['lens', 'findings', 'summary'] }
const FIX_SCHEMA = { type: 'object', properties: { fixed: { type: 'array', items: { type: 'string' } }, rejected: { type: 'array', items: { type: 'object', properties: { id: { type: 'string' }, reason: { type: 'string' } }, required: ['id', 'reason'] } }, adrs_authored: { type: 'array', items: { type: 'string' } }, conflicts_logged: { type: 'array', items: { type: 'string' } }, reassembled: { type: 'boolean' } }, required: ['fixed', 'rejected', 'reassembled'] }
const READY_SCHEMA = { type: 'object', properties: { gate_pass: { type: 'boolean' }, failed_criteria: { type: 'array', items: { type: 'string' } }, report_path: { type: 'string' }, coverage: { type: 'object', properties: { specified: { type: 'number' }, deferred: { type: 'number' }, missing: { type: 'number' } } }, residual_minor: { type: 'number' }, summary: { type: 'string' } }, required: ['gate_pass', 'failed_criteria', 'report_path', 'summary'] }

const readBrief = (name) => `Your complete brief is the file ${B}/${name}.md — read it in full FIRST and execute it exactly. Persist every artifact it names before returning. Return ONLY the structured JSON (schema enforced).`

// ---------- Author ----------
let authored = []
if (!P.skipAuthor) {
  phase('Author')
  const secs = P.sections || ALL_SECTIONS
  const WAVE = 9
  for (let i = 0; i < secs.length; i += WAVE) {
    const wave = secs.slice(i, i + WAVE)
    log(`Authoring sections: ${wave.join(', ')}`)
    const out = await parallel(wave.map(s => () => agent(`You are a fresh section author for the HarnessHarness Canonical Spec, section ${s}. ${readBrief('section-' + s)} If the target section file already exists (an earlier interrupted run), read it, verify it against the ADRs, complete it, and do not start over.`, { label: `author:${s}`, phase: 'Author', schema: AUTHOR_SCHEMA, agentType: 'general-purpose' })))
    out.forEach((r, k) => log(r ? `§${r.section}: ${r.words} words, ${r.r_ids_covered.length} R-ids, ${r.gaps.length} gap(s)` : `WARNING: section ${wave[k]} returned null`))
    authored.push(...out.map((r, k) => r || { section: wave[k], path: '', words: 0, r_ids_covered: [], gaps: ['AUTHOR FAILED'] }))
  }
}

// ---------- Assemble ----------
let arch = null
if (!P.skipAssemble) {
  phase('Assemble')
  const digest = authored.map(a => `- §${a.section}: ${a.words} words; gaps: ${a.gaps.join(' | ') || 'none'}; cross-refs needed: ${(a.cross_refs_needed || []).join(' | ') || 'none'}`).join('\n')
  arch = await agent(`You are the fresh architect–assembler for the HarnessHarness Canonical Spec. ${readBrief('architect')}\n\n## Section-author digest\n${digest || '(sections pre-existing on disk)'}`, { label: 'architect', phase: 'Assemble', schema: ARCH_SCHEMA, agentType: 'general-purpose', effort: 'high' })
  log(`Assembled: ${arch.words_total} words; dag_valid=${arch.dag_valid}; gaps remaining ${arch.gaps_remaining.length}; glossary: ${arch.glossary_check}`)
}

// ---------- Review / Fix loop ----------
const maxRounds = P.maxFixRounds || 2
let round = 0, unresolved = []
let allFindings = []
while (round < maxRounds) {
  round++
  phase('Review')
  log(`Review round ${round}: ${LENSES.length} lenses`)
  const reviews = (await parallel(LENSES.map(l => () => agent(`You are a fresh adversarial reviewer (lens: ${l}) of the HarnessHarness Canonical Spec. ${readBrief('review-' + l)}${round > 1 ? ' This is a re-review after fixes; focus on whether earlier blocker/major defects are gone and on new defects introduced by the fixes.' : ''}`, { label: `review:${l}:r${round}`, phase: 'Review', schema: REVIEW_SCHEMA, agentType: 'general-purpose', effort: 'high' })))).filter(Boolean)
  const findings = reviews.flatMap(r => r.findings.map(f => ({ ...f, id: `${r.lens}-r${round}-${f.id}`, lens: r.lens })))
  allFindings.push(...findings)
  const serious = findings.filter(f => f.severity !== 'minor')
  log(`Round ${round}: ${findings.length} findings (${serious.length} blocker/major)`)
  if (!serious.length) { unresolved = []; break }
  phase('Fix')
  const list = serious.map(f => `- [${f.id}] (${f.severity}, ${f.section}) ${f.defect}\n    quote: ${f.quote || ''}\n    authority: ${f.authority || ''}\n    fix: ${f.fix}`).join('\n')
  const fix = await agent(`You are the fresh fixer for the HarnessHarness Canonical Spec (round ${round}). ${readBrief('fixer')}\n\n## Findings to apply (blocker/major only)\n${list}`, { label: `fixer:r${round}`, phase: 'Fix', schema: FIX_SCHEMA, agentType: 'general-purpose', effort: 'high' })
  log(`Fixer: fixed ${fix.fixed.length}, rejected ${fix.rejected.length}, ADRs authored ${(fix.adrs_authored || []).length}, reassembled=${fix.reassembled}`)
  unresolved = fix.rejected
}

// ---------- Readiness ----------
phase('Readiness')
const digest = allFindings.map(f => `- ${f.id} ${f.severity} ${f.section}: ${f.defect}`).join('\n')
const ready = await agent(`You are the fresh Decompose-Readiness auditor for the HarnessHarness Canonical Spec. ${readBrief('readiness')}\n\n## Review digest (all rounds; the fixer addressed blocker/major items — verify)\n${digest || '(no findings)'}\n\n## Fixer-rejected findings (verify each rejection was justified)\n${unresolved.map(u => `- ${u.id}: ${u.reason}`).join('\n') || '(none)'}`, { label: 'readiness', phase: 'Readiness', schema: READY_SCHEMA, agentType: 'general-purpose', effort: 'high' })

return { authored: authored.map(a => ({ section: a.section, words: a.words, gaps: a.gaps.length })), arch: arch && { words: arch.words_total, dag_valid: arch.dag_valid, gaps: arch.gaps_remaining, glossary: arch.glossary_check }, review_rounds: round, findings_total: allFindings.length, unresolved, readiness: ready }
