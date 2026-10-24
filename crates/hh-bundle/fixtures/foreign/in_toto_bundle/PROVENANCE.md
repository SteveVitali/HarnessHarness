# PROVENANCE — in_toto_bundle fixture

- capture_kind: stand-in — hand-authored per the cited schema; not a
  captured upstream artefact (no retrieval/observation dates apply).
- Format reference: in-toto attestation bundle — a JSONL file of DSSE
  envelopes `{payloadType, payload: base64(in-toto Statement/v1),
  signatures[]}` (github.com/in-toto/attestation `spec/v1/bundle.md` +
  `spec/v1/statement.md`, consulted 2026-10-22; spec ticket R4.9 /
  DF-S4.2-2).
- Contents: one synthetic envelope — a v1 statement naming two subject
  digests under the SLSA predicate type, with one *stand-in* signature
  (not a real signature — the lift records it `present_unverified` and
  never verifies; no trust roots are in scope).
