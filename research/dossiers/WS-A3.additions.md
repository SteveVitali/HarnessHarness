# WS-A3 — register additions sidecar

**Temporary ids; synthesis renumbers and folds into `registers/*.md`. Do not edit shared registers directly.** Product name per ADR-0011: HarnessHarness.

## Sources

| temp-id | title | kind | tier | syllabus | P/S | provisional? | url | feeds |
|---|---|---|---|---|---|---|---|---|
| S-WS-A3-01 | ShengranHu/ADAS — reference code for S-023 (audited HEAD 2026-09-09): `_arc/search.py` L237–332 (archive `{thought, name, code, fitness, generation}`), `_arc/arc_prompt.py` (init archive) | repo | B | 0 | P | no | https://github.com/ShengranHu/ADAS | A3, I5 |
| S-WS-A3-02 | Unison — "The big idea": definitions identified by hash of their syntax tree; names as separately stored metadata; dependencies referenced by hash | doc | B | 0 | P | no | https://www.unison-lang.org/docs/the-big-idea/ | A3, L4 |
| S-WS-A3-03 | RFC 8785 — JSON Canonicalization Scheme (JCS): deterministic serialization for hashing/signing; I-JSON constraints; no Unicode normalization | spec | A | 0 | P | no | https://www.rfc-editor.org/rfc/rfc8785.html | A3, B1, L4, H6 |
| S-WS-A3-04 | MLIR Language Reference — operations/attributes/regions, dialects, unregistered operations, per-operation verifiers | doc | B | 0 | P | no | https://mlir.llvm.org/docs/LangRef/ | A3, A4 |
| S-WS-A3-05 | Falleri, Morandat, Blanc, Martinez, Monperrus (ASE 2014), "Fine-grained and Accurate Source Code Differencing" (GumTree): AST edit scripts with move actions; two-phase matching with structure hashes | paper | A | 0 | P (search summary + project page) | no | https://github.com/GumTreeDiff/gumtree ; https://www.labri.fr/perso/xblanc/data/papers/ASE14.pdf | A3, I7, L4 |
| S-WS-A3-06 | Leijen (MSFP 2014), "Koka: Programming with Row-polymorphic Effect Types" | paper | A | 0 | P (search summary) | no | https://arxiv.org/abs/1406.2061 | A3, L1 |

**Existing S-ids opened directly by WS-A3 (promote S→P where still `S`):** S-020 (GPTSwarm arXiv abstract — was `S`), S-025 (AFlow code — was `S`), S-095 (Procedural Graphs arXiv abstract — was `S`), S-099 (CaMeL arXiv abstract — was `S`), S-024 (AgentSquare abstract), S-144 (DSPy source, pinned ca54a85), S-110 (OpenHands SDK source, pinned 3fc7b22), S-139 (agent-spec source, pinned 6f0b6ae), S-113 (pi-mono source, pinned 400d690), S-153 (MCP schema repo, pinned aa8ce04). S-023 (ADAS paper) was read via its repository README and code only — leave the paper row as is; the repo is S-WS-A3-01. S-013 read via S-144 code. Cited only via WS-A1/WS-L7 (not re-opened): S-035, S-047, S-049, S-107, S-109, S-118, S-132, S-138, S-146, S-152.

## Open questions

| temp-id | question | resolver | due | blocking? |
|---|---|---|---|---|
| OQ-WS-A3-01 | Final `AuthorityClass` values (the single authority/label scheme of CF-006). The IR fixes the *shape* (closed sum, on every Observation/ContextItem/Memory/Permission issuer/Text leaf); values must be agreed before any of L3/H1/H2/D1 ADRs ratify. | WS-L3 with WS-H1, WS-H2, WS-D1 (Phase 2 synthesis) | Phase 2 | yes (HIR/1 freeze of enum values) |
| OQ-WS-A3-02 | Final `EffectClass` list (initial: read_fs, write_fs, exec, net_egress, secret_access, spend, message_human, spawn_process, memory_write, permission_request, model_call) and the extension policy (dialect bump only vs registered ext). | WS-H1 with WS-B2, WS-E1 | Phase 2 | yes (V-EFF check) |
| OQ-WS-A3-03 | Hash-function and canonical-form-version pinning per HIR dialect; rotation/migration of `semantic_id`/`version_id` when either changes. | WS-L4 | Phase 2 | no |
| OQ-WS-A3-04 | Are runtime `Observation`s stored as IR nodes inside the ledger (with ids) or only referenced by events by value? Affects whether `produced-by` edges live in the ledger or in definitions. | WS-B1 (with WS-A3) | Phase 1 | yes (event schema) |
| OQ-WS-A3-05 | Ablation semantics for `Text` leaves in the Lab: remove the leaf, or replace with a neutral placeholder of matched length? Changes what the opacity-ablation experiment measures. | WS-I2 with WS-D1 | Phase 3 | no |
| OQ-WS-A3-06 | Judge-kind Validators carry their own `ProfileRef`; how are their model calls, opacity and cost attributed (to the harness under test, or to the instrument)? | WS-G3 with WS-I2, WS-L2 | Phase 2 | no |
| OQ-WS-A3-07 | Canonical number representation in the canonical form (integers only; decimals as strings; or IEEE-754 doubles per JCS) — must be identical for IR documents and ledger events. | WS-A3 + WS-B1 + WS-L4 | Phase 1 synthesis | yes (OQ-041 completion) |
| OQ-WS-A3-08 | `rebase(HirDiff, definition')` across id changes (re-import, migration): adopt GumTree-class matching as a C1 extension, or require diffs to be regenerated from snapshot pairs? | WS-L4 with WS-I5 | Phase 4 | no |

Answered in the WS-A3 dossier (record as resolved-by-dossier pending ADR ratification): OQ-002, OQ-006, OQ-031, OQ-036, OQ-039 (small anchor Stage 3; rich anchor OpenHands SDK default agent at Phase 3 as a reported opacity figure), OQ-040 (C6 hard gates do not fire), OQ-041 (jointly with WS-B1: hash over a JCS-class canonical form; storage encoding free if lossless round-trip).

## Conflicts

| temp-id | parties | contradiction | proposed resolution |
|---|---|---|---|
| CF-WS-A3-01 | Ontology v0.1 `surface vs semantic identity` ("identity is content-addressed over its semantics") vs undecidability of semantic equivalence | Read literally, "semantic identity" promises equivalence, which no hash can deliver | Reword: *semantic identity* = identity over the canonical form of the semantic projection (`semantic_id`: surface, provenance, ext, version excluded; refs by semantic id; Text leaves by content hash). T-LCD-10's test (rename ⇒ identity unchanged) is unaffected. WS-A2 amends the row at v1. |
| CF-WS-A3-02 | doc 2 §11 / ontology §4b `Effect` entity (WS-A3) vs R-2.2.2 "Effect & transaction model" (WS-B2) | Two owners for one word | Split: `EffectClass` (closed sum, IR type, WS-A3) + `Effect` record shape `{class, target, idempotency_key, reversibility, transaction?, produced_by}` (WS-A3) with transaction/idempotency/compensation *semantics* owned by WS-B2. |
| CF-WS-A3-03 | `Observation` as IR entity (WS-A3) vs `ObservationEvent` in the event taxonomy (WS-B1) | Same noun at two layers | IR owns the payload type (source, producer, content, authority, taint, risk assessment); WS-B1 owns the ledger envelope (`participant_class`, `observability_level`, seq, timestamps). Envelope carries payload by value or by id (OQ-WS-A3-04). |
| CF-WS-A3-04 | `ToolCapability` (IR), `capability declaration` / `capability vector` (hosted, ADR-0005/0007), object-capability "capability" in WS-H1's reference monitor | One word, three meanings | Spec prose never uses bare "capability". `ToolCapability` (plane 2 entity), `Permission` (plane 6 grant), `capability declaration`/`capability vector` (hosted participants, unchanged), and **`authority handle`** for the runtime object-capability (WS-H1). WS-A2 records the rule at v1. |
| CF-WS-A3-05 | WS-A3 `Procedure` kernel (closed `ProcedureStepKind`) vs WS-D5 rich Procedure IR (OQ-023, C1/C2) | D5 may need step kinds the C0 kernel lacks | D5 extends only through `ext` slots or an HIR/2 dialect with migration; never by making `ProcedureStep` an open union. Pre-registered for Phase 2 synthesis. |
| CF-WS-A3-06 | DSPy `Signature.equals` includes instructions and field prefix/desc in identity (S-144) vs T-LCD-10 (identity excludes surface) | The strongest prior art puts wording in identity | Resolved by the content/surface split: a `Text` leaf's *content* is hashed into `semantic_id`; its *rendering* (placement, framing, name, argument order, description template) is `surface`. A different instruction is a different leaf; a reworded template is a profile edit. |
| CF-WS-A3-07 | ADR-0005 "interoperate, don't duplicate" vs Agent Spec's entity coverage (S-139: no permission/effect/validator/budget/memory-validity/provenance) | Export cannot be faithful | Agent Spec is a **declared-lossy** lowering/lifting target at C2 with a mandatory loss report (T-LCD-11 pattern); import yields default-deny, `authority = imported` definitions. Interop obligation satisfied; fidelity not claimed. |

## Ontology terms

| term | definition | notes |
|---|---|---|
| **Text leaf** | The only kernel construct for model-facing prose: `Text{content, owner, authority, provenance, language?}`; individually addressable; hashed by content into `semantic_id`; counted by the opacity ratio. | WS-A3; ratifies the T-LCD-02 "typed Text leaf". |
| **CompiledPayload** | The only kernel construct for executable bodies: `{format_tag, bytes_hash, declared_interface{inputs, outputs, effects, deterministic, target}, owner, provenance}`; bound into component-variant slots or `Opaque` procedure steps; never model-facing unless rendered through a profile. | WS-A3 / WS-A5. Answers doc 2 §7 "code vs configuration". |
| **OpaqueProcess** | `AgentProcess.hosted`: a black-box participant as an IR node with a kernel-defined tri-state `CapabilityDeclarationRecord`, observability levels, hosting mechanism, supplied context/tools/procedures, and boundary-enforced Budget and Permission; no component sub-entities. | WS-A3 / WS-J6. Typed form of *hosted participant*. |
| **semantic_id / version_id** | `version_id = H(canonical(node))`; `semantic_id = H(canonical(kind ∥ semantic ∥ refs-by-semantic_id))` excluding surface, provenance, ext and version records. | WS-A3 / WS-L4. Defines *semantic identity* (CF-WS-A3-01). |
| **surface record** | The profile-owned sub-record of a node (model-facing name, description template, argument naming/order, error rendering, placement/format); excluded from `semantic_id`; writable only by the Profile Compiler. | WS-A3 / WS-C3. Typed form of "surface" in T-LCD-01/-10. |
| **extension slot** | `ext: map<PrefixedKey, TypedValue>` with reverse-DNS prefixes and a reserved `hir/` namespace; registered schemas; preserved byte-for-byte on round-trip; never decides authority, budget or validity. | WS-A3 / WS-L5 / WS-A4. MCP `_meta` / MLIR dialect pattern. |
| **HIR dialect** | A version of the kernel schema (`HIR/1`, `HIR/2` …) pinned on every document with hash function and canonical-form version; kinds carry `since/until`; `migrate` is total or returns `MigrationLoss`. | WS-A3 / WS-L4. Extends ADR-0008's "dialects". |
| **sealed definition** | A Harness Definition whose references are all pinned to `version_id`s and whose ids are computed; the only form a run may execute or a diff may be taken over. | WS-A3 / WS-A5 / WS-J1. |
| **HirDiff (typed diff)** | The unit of harness edit: identity-keyed op list with classification `{semantic_ops, surface_ops, provenance_only_ops, ext_ops, authority_delta, budget_delta}` and `derived-from` provenance; invertible; `apply` reproduces the target canonical bytes. | WS-A3 / WS-I5 / WS-L4. |
| **derived-from (edge)** | Edit provenance: new node version → `{hypothesis: Text, trajectories: [RunRef], candidate_id?}`. Seventh edge kind. | WS-A3 / WS-I5 / WS-I7. |
| **effect class** | A value of the closed sum `EffectClass`; declared on ToolCapability, derived on Procedure, granted by Permission; distinct from the runtime `Effect` record. | WS-A3 / WS-H1 / WS-B2 (CF-WS-A3-02). |
| **authority handle** | WS-H1's runtime object-capability conferred out-of-band from model text; the word "capability" is not used bare in spec prose. | Proposed name only; WS-H1 defines (CF-WS-A3-04). |
| **opacity report** | `{opacity_ratio, opaque_leaf_count, per_owner_class breakdown, compiled_payload_count}` per sealed definition per profile, stored in the bundle; the static form of the *opacity ratio* (OQ-036). | WS-A3 / WS-I2 / WS-I3. |
| **OpaqueWithoutInterface · EffectUncovered · AuthorityWidening · ConditionedRuleIncomplete** | First-class `validate`/`classify` error classes (never warnings): an opaque leaf without declared interface; a Procedure effect no in-scope Permission authorizes; a diff that widens a Permission grant; a conditioned HarnessRule lacking its assumption-debt record. | WS-A3; siblings of `UnexpressibleSurface` (ADR-0007). |
| **surface vs semantic identity** (amendment) | Reword the ratified row: "identity is content-addressed over the *semantic projection* (canonical form of kind + semantic fields + references), excluding surface, provenance and extension fields; surface is a versioned, profile-owned rendering." | Amendment to a ratified v0.1 term; by ADR at v1 (CF-WS-A3-01). |
