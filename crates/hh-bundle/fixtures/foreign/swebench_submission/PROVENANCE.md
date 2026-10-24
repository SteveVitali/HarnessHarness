# PROVENANCE — swebench_submission fixture

- capture_kind: stand-in — hand-authored per the cited schema; not a
  captured upstream artefact (no retrieval/observation dates apply).
- Format reference: SWE-bench predictions JSONL — one JSON object per
  line carrying `instance_id`, `model_name_or_path`, `model_patch`
  (github.com/SWE-bench/SWE-bench `docs/guides/evaluation.md`,
  consulted 2026-10-22; spec ticket R4.9 / DF-S4.2-2).
- Contents: two synthetic predictions (one non-empty unified diff, one
  empty `model_patch` — the schema's documented *skip* marker). The
  instance ids spell real-looking coordinates for readability only; no
  upstream bytes were copied.
