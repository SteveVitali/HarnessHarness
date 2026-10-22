# PROVENANCE — inspect_eval_log fixture

- capture_kind: stand-in — hand-authored per the cited schema; not a
  captured upstream artefact (no retrieval/observation dates apply).
- Format reference: Inspect AI `EvalLog` `.json` spelling —
  `{version, status, eval{task,model,created,…}, plan, results{scores[]},
  stats, samples[]}` (inspect.aisi.org.uk/eval-logs.html + the
  `inspect_ai.log` pydantic schema, consulted 2026-10-22; spec ticket
  R4.9 / DF-S4.2-2). The `.eval` zip carrier is a transport container,
  not a second schema — the artefact here is the inner JSON document.
- Contents: one synthetic single-sample eval log with a `plan`/`stats`
  section so the lift's `untyped_slot` loss rows are exercised.
