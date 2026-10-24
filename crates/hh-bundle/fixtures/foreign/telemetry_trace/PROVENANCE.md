# PROVENANCE — telemetry_trace fixture

- capture_kind: stand-in — hand-authored per the cited schema; not a
  captured upstream artefact (no retrieval/observation dates apply).
- Format reference: OTLP/JSON `ExportTraceServiceRequest` —
  `{resourceSpans[{resource{attributes[]}, scopeSpans[{scope, spans[]}]}]}`
  with hex trace/span ids and unix-nanos times as decimal strings
  (opentelemetry.io — OTLP specification 1.11.1 + the JSON Protobuf
  encoding rules, consulted 2026-10-22; spec ticket R4.9 / DF-S4.2-2).
- Contents: one synthetic resource span block, two spans on one trace,
  one `hh.standin` attribute so the lift's `narrowed` loss row is
  exercised.
