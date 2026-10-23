// lib/hir.ts — the `hir-documents` arm (R4.4b): the E3-candidate port of
// the E1 HIR replay driver (`crates/hh-xcheck/src/hir_replay.rs`) over the
// committed five-case corpus. Pipeline order is the empirical E1 order —
// parse → canonicalize → validate → seal — with `skipped` gating and the
// first-raised, duplicate-collapsed `<K1>,<K2>` error join spelled by
// `HirError::kind_str()`.
//
// Scope (the ticket's corpus-faithful bound): the corpus exercises five
// node kinds — AgentProcess (native + hosted bodies), Budget,
// HarnessRule, Permission, ToolCapability — a root reference, and no
// edges. The port decodes and re-emits every member those kinds carry,
// ports the provenance codec end-to-end, and reproduces the validate and
// seal machinery the corpus reaches (plus the shared generic checks).
//
// Honest bound — a case is *omitted* (the bundle's honest `missing
// answer` signal, never a fabricated verdict) when it reaches surface the
// port does not carry: the eight unexercised registered kinds, any edge,
// `assumption_debt` records, or a `flow_contract` member. Registered-but-
// unexercised spellings still parse through the real vocabulary
// (`UnknownKind`, `MissingProvenance`, `SchemaViolation`, …); what the
// port cannot honestly judge it does not answer.
//
// Erasable-TypeScript only; stdlib only; reuses `lib/core.ts` (CC1) — no
// second canonical codec or idp/1 spelling lives here.

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import {
  canonical,
  decodeUtf8,
  idpId,
  parseJson,
  sha256Hex,
} from "./core.ts";
import type { Json } from "./core.ts";

// ─────────────────────────────────────────────────────────────────────────────
// Errors — the closed `HirError::kind_str()` vocabulary (§3.1.4). `detail`
// is carried for debugging; the answer joins kinds only.
// ─────────────────────────────────────────────────────────────────────────────

type HirErr = { kind: string; detail?: string };
class HirFail extends Error {
  readonly hir: HirErr;
  constructor(e: HirErr) {
    super(e.detail ?? e.kind);
    this.hir = e;
  }
}
function fail(kind: string, detail: string): never {
  throw new HirFail({ kind, detail });
}
function sv(p: string, detail: string): never {
  fail("SchemaViolation", detail === "" ? p : `${p}${detail}`);
}
/** Marker: this case needs machinery the port does not carry — the arm
 *  omits the answer (honest pending), never a fabricated verdict. */
class HirUnsupported extends Error {}

// ─────────────────────────────────────────────────────────────────────────────
// Json helpers (the `schema.rs` req/opt spellings)
// ─────────────────────────────────────────────────────────────────────────────

function isObj(v: Json | undefined): v is { [k: string]: Json } {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}
function get(j: Json | undefined, k: string): Json | undefined {
  return isObj(j) ? j[k] : undefined;
}
function has(j: Json | undefined, k: string): boolean {
  return isObj(j) && Object.prototype.hasOwnProperty.call(j, k);
}
function req(j: Json, k: string, p: string): Json {
  const v = get(j, k);
  if (v === undefined) sv(p, `.${k} missing`);
  return v;
}
function reqStr(j: Json, k: string, p: string): string {
  const v = req(j, k, p);
  if (typeof v !== "string") sv(p, `.${k} must be a string`);
  return v as string;
}
function reqInt(j: Json, k: string, p: string): number {
  const v = req(j, k, p);
  if (typeof v !== "number" || !Number.isInteger(v) || v < 0)
    sv(p, `.${k} must be a non-negative integer`);
  return v as number;
}
function optStr(j: Json, k: string): string | undefined {
  const v = get(j, k);
  return typeof v === "string" ? v : undefined;
}
function arr(j: Json, k: string, p: string): Json[] {
  const v = req(j, k, p);
  if (!Array.isArray(v)) sv(p, `.${k} must be an array`);
  return v;
}
function strArr(items: Json[], p: string): string[] {
  return items.map((s, i) => {
    if (typeof s !== "string") sv(p, `[${i}] must be a string`);
    return s;
  });
}
/** `bool_at(...).unwrap_or(default)` — a non-bool member decodes to the
 *  default exactly as the reference decoder's `unwrap_or` does. */
function boolAtOr(j: Json, k: string, p: string, dflt: boolean): boolean {
  const v = get(j, k);
  if (v === undefined) return dflt;
  if (typeof v !== "boolean") sv(p, `.${k} must be a bool`);
  return v;
}
function tagOf(j: Json, p: string): string {
  const v = get(j, "kind");
  if (typeof v !== "string") sv(p, ".kind missing");
  return v;
}

// ─────────────────────────────────────────────────────────────────────────────
// The closed registries the corpus reaches (names are the canonical
// spellings; `parse` failures answer `UnknownKind`, never a fabricated
// member).
// ─────────────────────────────────────────────────────────────────────────────

const ENTITY_KINDS = new Set([
  "Goal", "Observation", "ContextItem", "Memory", "Procedure",
  "ToolCapability", "Permission", "Effect", "Artifact", "Validator",
  "AgentProcess", "Budget", "HarnessRule",
]);
const EDGE_KINDS = new Set([
  "depends-on", "supersedes", "authorizes", "produced-by", "validates",
  "delegated-to", "derived-from",
]);
/** The kinds this port decodes — the corpus-exercised five. */
const PORTED_KINDS = new Set([
  "AgentProcess", "Budget", "HarnessRule", "Permission", "ToolCapability",
]);
const HAS_SURFACE = new Set([
  "ToolCapability", "Validator", "Procedure", "ContextItem",
]);
const DECISION_POINTS = new Set([
  "plan", "act", "retrieve", "compact", "verify", "delegate", "authorize",
  "retry", "stop", "escalate",
]);
const OWNERS = new Set(["code", "model", "human"]);
const OBSERVABILITY = new Set(["events", "model_io", "end_state", "ledger"]);
const CAPABILITY_STATES = new Set(["supported", "unsupported", "unknown"]);
const HOSTING = new Set([
  "session_abi", "model_boundary_intercept", "container_installed",
]);
const LOCALITIES = new Set(["in_process", "out_of_process", "inherit"]);
const PRECONDITION_DOMAINS = new Set([
  "filesystem", "network", "credentials", "environment", "temporal",
  "capability",
]);
const SUPERSEDE_REASONS = new Set([
  "edit", "revocation", "expiry", "migration", "consolidation", "fork",
]);
const AUTHORITY_NAME = [
  "unverified", "external", "environment", "delegate", "principal",
  "definition", "kernel",
];
const AUTHORITY: { [k: string]: number } = Object.create(null);
AUTHORITY_NAME.forEach((n, i) => {
  AUTHORITY[n] = i;
});
const SCOPES = new Set([
  "definition", "user", "project", "session", "run", "turn",
]);
const HUMAN_ROLES = new Set(["author", "principal", "reviewer"]);
const ATTESTATION_KINDS = new Set(["seal", "hash_chain", "signature", "pin"]);
const DERIVATION_KINDS = new Set([
  "summary", "compaction", "extraction", "quotation", "revision",
  "translation", "subagent_result", "projection",
]);
/** `EffectDomain` — the enum *rank* matters: `declared` sets are a
 *  `BTreeSet<EffectClass>` ordered by (domain rank, attributes). */
const EFFECT_DOMAINS = [
  "fs_read", "fs_write", "exec", "net_egress", "secret_access", "spend",
  "message_human", "spawn_process", "memory_write", "permission_request",
  "model_call",
];
const MUTABILITIES = new Set(["read_only", "additive", "destructive"]);
const REPEAT_SAFETIES = new Set(["idempotent", "non_idempotent"]);
const WORLDS = new Set(["closed", "open"]);
const REVERSIBILITIES = new Set(["compensable", "irreversible"]);
/** `DimensionKey::parse` — `DimensionId` ∪ registered `ext` names ∪
 *  derived dimensions (hharness-ontology `dimensions.rs`). */
const DIMENSION_KEYS = new Set([
  "tokens.input.uncached", "tokens.input.cache_read",
  "tokens.input.cache_write", "tokens.output.visible",
  "tokens.output.reasoning", "model_calls", "tool_calls", "turns",
  "retries", "spawns", "approvals.requested", "approvals.granted",
  "evaluator_calls", "network.calls", "network.bytes_out",
  "network.bytes_in", "time.wall_ms", "time.working_ms",
  "time.model_latency_ms", "time.human_wait_ms", "message_human",
  "env.active_ms", "env.reserved_ms", "env.suspended_ms", "spend",
  "hh.egress.decisions.allowed", "hh.egress.decisions.denied",
  "hh.egress.decisions.asked", "hh.containment.violations",
  "ext.effects.external_irreversible", "hh.env.cpu_ms",
  "hh.env.memory_bytes", "hh.env.disk_bytes", "hh.env.pids",
  "hh.env.open_files", "context.occupancy", "fan_out", "delegation_depth",
  "reconciliation.holds", "tokens.input.total", "tokens.output.total",
  "tokens.blended",
]);
const SCOPE_KINDS = new Set([
  "fs_path", "host", "recipient", "spend_amount", "secret_ref",
  "process_target", "memory_scope", "resource_key",
]);
const EXPOSURE_MODES = new Set([
  "direct", "indexed", "deferred", "code_mode", "hidden",
]);

// ─────────────────────────────────────────────────────────────────────────────
// Provenance (hharness-provenance `record.rs`/`origin.rs`/`authority.rs` — the
// corpus-reached codec, byte-faithful both directions)
// ─────────────────────────────────────────────────────────────────────────────

type Origin = { kind: string; [k: string]: Json };
type Att = {
  kind: string;
  subject_hash: string;
  anchor: { signer?: string; chain?: string };
  verified_by: string;
  verified_at: number;
};
type Der = { kind: string; inputs: string[]; deterministic: boolean };
type Prov = {
  origin: Origin;
  authority: number;
  scope: string;
  created_at: number;
  taint: string[]; // BTreeSet — deduped, byte-sorted
  readers: string[] | null; // null = Public
  derived_from: Der[];
  attestation: Att | null;
};

function authorityParse(s: string, kind: string): number {
  const a = AUTHORITY[s];
  if (a === undefined) fail("UnknownKind", kind);
  return a;
}

function taintParse(s: string): string {
  if (
    s.startsWith("tool:") ||
    s.startsWith("participant:") ||
    s.startsWith("import:") ||
    s.startsWith("extension:")
  ) {
    return s;
  }
  fail("UnknownKind", `taint tag ${s}`);
}
/** `BTreeSet` semantics on decode: dedupe + byte sort (array order is
 *  semantic — canonical JSON does not re-sort arrays). */
function sortedSet(items: string[]): string[] {
  return [...new Set(items)].sort((a, b) =>
    Buffer.compare(Buffer.from(a, "utf8"), Buffer.from(b, "utf8"))
  );
}

function originFromJson(j: Json, p: string): Origin {
  const kind = tagOf(j, p);
  const s = (k: string) => reqStr(j, k, p);
  switch (kind) {
    case "kernel":
      return { kind, component_ref: s("component_ref") };
    case "human": {
      const role = s("role");
      if (!HUMAN_ROLES.has(role)) fail("UnknownKind", `human role ${role}`);
      return { kind, author_ref: s("author_ref"), role };
    }
    case "model":
      return {
        kind,
        model_ref: s("model_ref"),
        run_ref: s("run_ref"),
        response_id: s("response_id"),
      };
    case "tool": {
      const inner = optStr(j, "inner_source");
      if (inner !== undefined) taintParse(inner);
      const o: Origin = {
        kind,
        capability: s("capability"),
        invocation_ref: s("invocation_ref"),
      };
      if (inner !== undefined) o["inner_source"] = inner;
      return o;
    }
    case "evolution":
      return {
        kind,
        candidate_id: s("candidate_id"),
        hypothesis_ref: s("hypothesis_ref"),
      };
    case "import":
      return {
        kind,
        source_system: s("source_system"),
        mapping_version: s("mapping_version"),
      };
    case "migration":
      return { kind, from_dialect: s("from_dialect") };
    case "participant":
      return {
        kind,
        participant_ref: s("participant_ref"),
        hosting_mechanism: s("hosting_mechanism"),
      };
    default:
      return fail("UnknownKind", `origin ${kind}`);
  }
}

function originJson(o: Origin): Json {
  const k = o.kind;
  const obj: { [k2: string]: Json } = { kind: k };
  switch (k) {
    case "kernel":
      obj["component_ref"] = o["component_ref"] as Json;
      break;
    case "human":
      obj["author_ref"] = o["author_ref"] as Json;
      obj["role"] = o["role"] as Json;
      break;
    case "model":
      obj["model_ref"] = o["model_ref"] as Json;
      obj["run_ref"] = o["run_ref"] as Json;
      obj["response_id"] = o["response_id"] as Json;
      break;
    case "tool":
      obj["capability"] = o["capability"] as Json;
      obj["invocation_ref"] = o["invocation_ref"] as Json;
      if (o["inner_source"] !== undefined)
        obj["inner_source"] = o["inner_source"] as Json;
      break;
    case "evolution":
      obj["candidate_id"] = o["candidate_id"] as Json;
      obj["hypothesis_ref"] = o["hypothesis_ref"] as Json;
      break;
    case "import":
      obj["source_system"] = o["source_system"] as Json;
      obj["mapping_version"] = o["mapping_version"] as Json;
      break;
    case "migration":
      obj["from_dialect"] = o["from_dialect"] as Json;
      break;
    case "participant":
      obj["participant_ref"] = o["participant_ref"] as Json;
      obj["hosting_mechanism"] = o["hosting_mechanism"] as Json;
      break;
    default:
      break;
  }
  return obj;
}

function attestationFromJson(j: Json, p: string): Att {
  const kind = reqStr(j, "kind", p);
  if (!ATTESTATION_KINDS.has(kind))
    fail("UnknownKind", `attestation kind ${kind}`);
  const aj = req(j, "anchor", p);
  const signer = get(aj, "signer");
  const chain = get(aj, "chain");
  let anchor: { signer?: string; chain?: string };
  if (typeof signer === "string") anchor = { signer };
  else if (typeof chain === "string") anchor = { chain };
  else sv(p, ".anchor must be signer|chain");
  return {
    kind,
    subject_hash: reqStr(j, "subject_hash", p),
    anchor: anchor as { signer?: string; chain?: string },
    verified_by: reqStr(j, "verified_by", p),
    verified_at: reqInt(j, "verified_at", p),
  };
}

function attestationJson(a: Att): Json {
  return {
    kind: a.kind,
    subject_hash: a.subject_hash,
    anchor: a.anchor.signer !== undefined
      ? { signer: a.anchor.signer }
      : { chain: a.anchor.chain as string },
    verified_by: a.verified_by,
    verified_at: a.verified_at,
  };
}

function selfConsistent(a: Att): boolean {
  return (
    a.subject_hash !== "" &&
    a.verified_by !== "" &&
    (a.anchor.signer ?? a.anchor.chain ?? "") !== ""
  );
}

function provenanceFromJson(j: Json, p: string): Prov {
  const origin = originFromJson(req(j, "origin", p), `${p}.origin`);
  const authority = authorityParse(reqStr(j, "authority", p), `${p}.authority`);
  const scopeStr = reqStr(j, "scope", p);
  if (!SCOPES.has(scopeStr)) fail("UnknownKind", `scope ${scopeStr}`);
  const created_at = reqInt(j, "created_at", p);
  let taint: string[] = [];
  const tj = get(j, "taint");
  if (tj !== undefined) {
    if (!Array.isArray(tj)) sv(p, ".taint must be an array");
    taint = sortedSet(
      tj.map((t) => {
        if (typeof t !== "string") sv(p, ".taint members must be strings");
        return taintParse(t);
      })
    );
  }
  let readers: string[] | null = null;
  const rj = get(j, "readers");
  if (rj !== undefined) {
    if (!Array.isArray(rj)) sv(p, ".readers must be an array");
    readers = sortedSet(strArr(rj, `${p}.readers`));
  }
  let derived_from: Der[] = [];
  const dj = get(j, "derived_from");
  if (dj !== undefined) {
    if (!Array.isArray(dj)) sv(p, ".derived_from must be an array");
    derived_from = dj.map((d, i) => {
      const dp = `${p}.derived_from[${i}]`;
      const kind = reqStr(d, "kind", dp);
      if (!DERIVATION_KINDS.has(kind))
        fail("UnknownKind", `derivation kind ${kind}`);
      const det = get(d, "deterministic");
      if (typeof det !== "boolean") sv(dp, ".deterministic must be a bool");
      return {
        kind,
        inputs: strArr(arr(d, "inputs", dp), `${dp}.inputs`),
        deterministic: det,
      };
    });
  }
  const attJ = get(j, "attestation");
  return {
    origin,
    authority,
    scope: scopeStr,
    created_at,
    taint,
    readers,
    derived_from,
    attestation: attJ === undefined ? null : attestationFromJson(attJ, `${p}.attestation`),
  };
}

function provenanceJson(p: Prov): Json {
  const out: { [k: string]: Json } = {
    origin: originJson(p.origin),
    authority: AUTHORITY_NAME[p.authority],
    scope: p.scope,
    created_at: p.created_at,
  };
  if (p.taint.length > 0) out["taint"] = [...p.taint];
  if (p.readers !== null) out["readers"] = [...p.readers];
  if (p.derived_from.length > 0) {
    out["derived_from"] = p.derived_from.map((d) => ({
      kind: d.kind,
      inputs: [...d.inputs],
      deterministic: d.deterministic,
    }));
  }
  if (p.attestation !== null) out["attestation"] = attestationJson(p.attestation);
  return out;
}

/** `default_authority(origin, scope, attestation)` under the default
 *  (empty) minting context — no closed-world tools, no vouched
 *  participants. */
function defaultAuthority(o: Origin, att: Att | null): number {
  if (att !== null && !selfConsistent(att)) return AUTHORITY["unverified"];
  switch (o.kind) {
    case "kernel":
      return AUTHORITY["kernel"];
    case "human":
      return AUTHORITY["principal"];
    case "model":
    case "evolution":
      return AUTHORITY["delegate"];
    case "tool":
      return AUTHORITY["external"];
    case "participant":
      return AUTHORITY["unverified"];
    case "import":
    case "migration":
      return AUTHORITY["unverified"];
    case "cache":
      return AUTHORITY["environment"];
    default:
      return AUTHORITY["unverified"];
  }
}

/** `minted_ceiling`: the default authority, lifted to `definition` by a
 *  self-consistent seal/pin attestation (record.rs:322). */
function mintedCeiling(p: Prov): number {
  const base = defaultAuthority(p.origin, p.attestation);
  if (
    p.attestation !== null &&
    selfConsistent(p.attestation) &&
    (p.attestation.kind === "seal" || p.attestation.kind === "pin")
  ) {
    return Math.max(base, AUTHORITY["definition"]);
  }
  return base;
}

/** `ProvenanceRecord::validate(None)` → the `HirError` spelling via
 *  `map_provenance_error`. */
function checkProvenance(p: Prov, what: string, errs: HirErr[]): void {
  if (p.taint.length > 0 && p.authority > AUTHORITY["external"]) {
    errs.push({
      kind: "TaintedAboveExternal",
      detail: `${what}: ${AUTHORITY_NAME[p.authority]}`,
    });
    return;
  }
  const ceiling = mintedCeiling(p);
  if (p.authority > ceiling) {
    errs.push({
      kind: "AuthorityExceedsOrigin",
      detail: `${what}: claimed ${AUTHORITY_NAME[p.authority]} above ceiling ${AUTHORITY_NAME[ceiling]}`,
    });
    return;
  }
  if (p.attestation !== null && !selfConsistent(p.attestation)) {
    errs.push({
      kind: "IllegitimateEndorsement",
      detail: `${what}: attestation failed: attestation is a bare self-claim (missing subject, anchor or verifier)`,
    });
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Refs (refs.rs) — `Ref`, `ProfileRef`, `ComponentVariantRef`
// ─────────────────────────────────────────────────────────────────────────────

type Ref = { sid: string; pin: string | null; sel: string | null };
type ProfileRef = { profile: string; pinned: boolean };
type Cvr = { class_id: string; variant_id: string; pin: string | null; sel: string | null };

function refFromJson(j: Json, p: string): Ref {
  const sid = optStr(j, "semantic_id");
  if (sid === undefined) sv(p, ".semantic_id missing");
  const v = optStr(j, "version_id");
  const s = optStr(j, "version_selector");
  if (v !== undefined && s === undefined) return { sid, pin: v, sel: null };
  if (v === undefined && s !== undefined) return { sid, pin: null, sel: s };
  sv(p, " must carry exactly one of version_id|version_selector");
}
function refJson(r: Ref, semantic: boolean): Json {
  if (semantic) return { semantic_id: r.sid };
  return r.pin !== null
    ? { semantic_id: r.sid, version_id: r.pin }
    : { semantic_id: r.sid, version_selector: r.sel as string };
}
function refsFromJson(j: Json, p: string): Ref[] {
  if (!Array.isArray(j)) sv(p, " must be an array of refs");
  return j.map((r, i) => refFromJson(r, `${p}[${i}]`));
}
function profileRefFromJson(j: Json, p: string): ProfileRef {
  return {
    profile: reqStr(j, "profile_ref", p),
    pinned: boolAtOr(j, "pinned", p, false),
  };
}
function profileRefJson(p: ProfileRef): Json {
  return { profile_ref: p.profile, pinned: p.pinned };
}
function cvrFromJson(j: Json, p: string): Cvr {
  if (isObj(j)) {
    for (const k of Object.keys(j)) {
      if (!["class_id", "variant_id", "version_id", "version_selector"].includes(k)) {
        sv(p, `.${k}: unknown ComponentVariantRef member`);
      }
    }
  }
  const v = optStr(j, "version_id");
  const s = optStr(j, "version_selector");
  const field = (k: string) => {
    const x = optStr(j, k);
    if (x === undefined) sv(p, `.${k} missing`);
    return x;
  };
  if (v !== undefined && s === undefined)
    return { class_id: field("class_id"), variant_id: field("variant_id"), pin: v, sel: null };
  if (v === undefined && s !== undefined)
    return { class_id: field("class_id"), variant_id: field("variant_id"), pin: null, sel: s };
  sv(p, " must carry exactly one of version_id|version_selector");
}
function cvrJson(c: Cvr, semantic: boolean): Json {
  if (semantic) return { class_id: c.class_id, variant_id: c.variant_id };
  return c.pin !== null
    ? { class_id: c.class_id, variant_id: c.variant_id, version_id: c.pin }
    : {
        class_id: c.class_id,
        variant_id: c.variant_id,
        version_selector: c.sel as string,
      };
}

// ─────────────────────────────────────────────────────────────────────────────
// Leaves (leaves.rs) — `Text`, `CompiledPayload`
// ─────────────────────────────────────────────────────────────────────────────

type TextLeaf = {
  content_hash: string;
  owner: string;
  authority: number;
  provenance: Prov;
  language?: string;
};
function textFromJson(j: Json, p: string): TextLeaf {
  const g = (k: string) => {
    const v = get(j, k);
    if (typeof v !== "string") sv(p, `.${k} missing`);
    return v;
  };
  const content_hash = g("content_hash");
  const owner = g("owner");
  const authority = authorityParse(g("authority"), `${p}.authority: bad AuthorityClass`);
  const provJ = get(j, "provenance");
  if (provJ === undefined)
    throw new HirFail({ kind: "MissingProvenance", detail: `${p} (Text leaf)` });
  const provenance = provenanceFromJson(provJ, p);
  const language = optStr(j, "language");
  const t: TextLeaf = { content_hash, owner, authority, provenance };
  if (language !== undefined) t.language = language;
  return t;
}
function textJson(t: TextLeaf, semantic: boolean): Json {
  const out: { [k: string]: Json } = {
    content_hash: t.content_hash,
    owner: t.owner,
    authority: AUTHORITY_NAME[t.authority],
  };
  if (!semantic) out["provenance"] = provenanceJson(t.provenance);
  if (t.language !== undefined) out["language"] = t.language;
  return out;
}

// `CompiledPayload` (bytes_hash + declared_interface) and its checks are
// corpus-unreached — it lives only on unexercised kinds (Procedure /
// Validator / ContextItem / Memory), so its decode/endorse/check legs are
// part of the honest bound rather than ported here.

// ─────────────────────────────────────────────────────────────────────────────
// Effect classes (kinds.rs) — `EffectClass`, `EffectAttributes`,
// `Reversibility`; the `BTreeSet<EffectClass>` order is derived-`Ord`
// (domain rank, then attributes), so `declared` re-emits sorted.
// ─────────────────────────────────────────────────────────────────────────────

type Reversibility = { pin: Ref } | { s: string };
type EffectAttributes = {
  mutability: string;
  repeat_safety: string;
  world: string;
  reversibility: Reversibility;
};
type EffectClass = { domain: string; attributes: EffectAttributes | null };

function domainRank(d: string): number {
  const i = EFFECT_DOMAINS.indexOf(d);
  if (i < 0) fail("UnknownKind", `effect domain ${d}`);
  return i;
}
function reversibilityJson(r: Reversibility, semantic: boolean): Json {
  if ("pin" in r) return { reversible: refJson(r.pin, semantic) };
  return r.s;
}
function reversibilityFromJson(j: Json, p: string): Reversibility {
  if (typeof j === "string") {
    if (!REVERSIBILITIES.has(j)) fail("UnknownKind", `reversibility ${j}`);
    return { s: j };
  }
  if (isObj(j)) {
    const r = get(j, "reversible");
    if (r === undefined) sv(p, ": bad reversibility");
    return { pin: refFromJson(r, `${p}.reversible`) };
  }
  sv(p, ": bad reversibility");
}
function attributesFromJson(j: Json, p: string): EffectAttributes {
  const g = (k: string) => {
    const v = optStr(j, k);
    if (v === undefined) sv(p, `.${k} missing`);
    return v;
  };
  const mutability = g("mutability");
  if (!MUTABILITIES.has(mutability)) fail("UnknownKind", `mutability ${mutability}`);
  const repeat_safety = g("repeat_safety");
  if (!REPEAT_SAFETIES.has(repeat_safety))
    fail("UnknownKind", `repeat_safety ${repeat_safety}`);
  const world = g("world");
  if (!WORLDS.has(world)) fail("UnknownKind", `world ${world}`);
  const rev = get(j, "reversibility");
  if (rev === undefined) sv(p, ".reversibility missing");
  return {
    mutability,
    repeat_safety,
    world,
    reversibility: reversibilityFromJson(rev, `${p}.reversibility`),
  };
}
function attributesJson(a: EffectAttributes, semantic: boolean): Json {
  return {
    mutability: a.mutability,
    repeat_safety: a.repeat_safety,
    world: a.world,
    reversibility: reversibilityJson(a.reversibility, semantic),
  };
}
function effectClassFromJson(j: Json, p: string): EffectClass {
  const d = optStr(j, "domain");
  if (d === undefined) sv(p, ".domain missing");
  domainRank(d); // UnknownKind on an unregistered domain
  const a = get(j, "attributes");
  return {
    domain: d,
    attributes: a === undefined ? null : attributesFromJson(a, `${p}.attributes`),
  };
}
function effectClassJson(e: EffectClass, semantic: boolean): Json {
  if (e.attributes === null) return { domain: e.domain };
  return {
    domain: e.domain,
    attributes: attributesJson(e.attributes, semantic),
  };
}
/** Structural compare standing in for the derived `Ord` on
 *  `EffectAttributes` — mutability/repeat_safety/world by enum rank,
 *  then reversibility (Reversible < Compensable < Irreversible by
 *  declaration order; refs compare on sid then pin<sel then value). */
const MUT_RANK: { [k: string]: number } = {
  read_only: 0,
  additive: 1,
  destructive: 2,
};
const RS_RANK: { [k: string]: number } = { idempotent: 0, non_idempotent: 1 };
const WORLD_RANK: { [k: string]: number } = { closed: 0, open: 1 };
const REV_RANK = (r: Reversibility) => ("pin" in r ? 0 : r.s === "compensable" ? 1 : 2);
function refCmp(a: Ref, b: Ref): number {
  if (a.sid !== b.sid) return a.sid < b.sid ? -1 : 1;
  const av = a.pin !== null ? a.pin : (a.sel as string);
  const bv = b.pin !== null ? b.pin : (b.sel as string);
  if ((a.pin === null) !== (b.pin === null)) return a.pin !== null ? -1 : 1;
  return av < bv ? -1 : av > bv ? 1 : 0;
}
function revCmp(a: Reversibility, b: Reversibility): number {
  const ra = REV_RANK(a);
  const rb = REV_RANK(b);
  if (ra !== rb) return ra - rb;
  if ("pin" in a && "pin" in b) return refCmp(a.pin, b.pin);
  return 0;
}
function effectCmp(a: EffectClass, b: EffectClass): number {
  const d = domainRank(a.domain) - domainRank(b.domain);
  if (d !== 0) return d;
  if (a.attributes === null || b.attributes === null)
    return a.attributes === null ? (b.attributes === null ? 0 : -1) : 1;
  const A = a.attributes;
  const B = b.attributes;
  for (const [x, y, t] of [
    [A.mutability, B.mutability, MUT_RANK],
    [A.repeat_safety, B.repeat_safety, RS_RANK],
    [A.world, B.world, WORLD_RANK],
  ] as [string, string, { [k: string]: number }][]) {
    const c = (t[x] ?? 0) - (t[y] ?? 0);
    if (c !== 0) return c;
  }
  return revCmp(A.reversibility, B.reversibility);
}
function effectSet(items: EffectClass[]): EffectClass[] {
  const out = [...items].sort(effectCmp);
  return out.filter((e, i) => i === 0 || effectCmp(e, out[i - 1]) !== 0);
}
// (effect coverage / closed-world checks — `check_effect_coverage` and
// `EffectClass::closed_world` consumers — are reachable only through the
// unported Procedure/kind surface, so they stay inside the honest bound.)

// ─────────────────────────────────────────────────────────────────────────────
// Record-member decoders (schema.rs) — validity / boundary / capability
// declaration / slots / supplies / rule action / grants / tool members.
// ─────────────────────────────────────────────────────────────────────────────

type Validity = { from: number; until: number | null; condition: string | null };
type Boundary = { assignments: { [k: string]: string }; guards: { [k: string]: string } };
type CapDecl = { [k: string]: string };
type SlotBinding = {
  variant: Cvr;
  params: { [k: string]: Json };
  enabled: boolean;
  locality?: string;
};
type SlotBindings = { one: SlotBinding } | { many: SlotBinding[] };
type Supplies = { context: Ref[]; tools: Ref[]; procedures: Ref[] };
type RuleAction = { v: string; r?: Ref; rs?: Ref[]; j?: Json; s?: string };
type Grant = {
  effect: EffectClass;
  scope: string;
  constraints: { budget?: Json; time?: number; count?: number };
  delegable: boolean;
};
type ToolEffects = "pure" | { declared: EffectClass[] };
type Resources = { declared?: string[]; state?: "none" | "unknown" };
type ScopeBindings = "unknown" | Json;

const CAP_DECL_FIELDS = [
  "streaming", "interrupt", "steer", "live_queue", "resume", "fork",
  "compaction", "images", "subagents", "permission_surface",
  "instruction_delivery", "model_family", "effort_vocabulary",
  "trajectory_export", "native_config",
];

function validityFromJson(j: Json, p: string): Validity {
  const untilJ = get(j, "until");
  if (untilJ !== undefined && (typeof untilJ !== "number" || !Number.isInteger(untilJ) || untilJ < 0))
    sv(p, ".until must be a non-negative integer");
  return {
    from: reqInt(j, "from", p),
    until: untilJ === undefined ? null : (untilJ as number),
    condition: optStr(j, "condition") ?? null,
  };
}
function validityJson(v: Validity): Json {
  const out: { [k: string]: Json } = { from: v.from };
  if (v.until !== null) out["until"] = v.until;
  if (v.condition !== null) out["condition"] = v.condition;
  return out;
}

function boundaryFromJson(j: Json, p: string): Boundary {
  const b: Boundary = { assignments: {}, guards: {} };
  const asg = get(j, "assignments");
  if (isObj(asg)) {
    for (const [k, v] of Object.entries(asg)) {
      if (typeof v !== "string") sv(p, `.assignments.${k} must be a string`);
      if (!OWNERS.has(v)) fail("UnknownKind", `owner ${v}`);
      if (!DECISION_POINTS.has(k)) fail("UnknownKind", `decision point ${k}`);
      b.assignments[k] = v;
    }
  }
  const guards = get(j, "guards");
  if (isObj(guards)) {
    for (const [k, v] of Object.entries(guards)) {
      if (typeof v !== "string") sv(p, `.guards.${k} must be a string`);
      if (!DECISION_POINTS.has(k)) fail("UnknownKind", `decision point ${k}`);
      b.guards[k] = v;
    }
  }
  return b;
}
function boundaryJson(b: Boundary): Json {
  const assignments: { [k: string]: Json } = Object.create(null);
  for (const [k, v] of Object.entries(b.assignments)) assignments[k] = v;
  const guards: { [k: string]: Json } = Object.create(null);
  for (const [k, v] of Object.entries(b.guards)) guards[k] = v;
  return { assignments, guards };
}

function capDeclFromJson(j: Json, p: string): CapDecl {
  const out: CapDecl = Object.create(null);
  for (const f of CAP_DECL_FIELDS) {
    const v = reqStr(j, f, p);
    if (!CAPABILITY_STATES.has(v)) fail("UnknownKind", `capability state ${v}`);
    out[f] = v;
  }
  return out;
}
function capDeclJson(c: CapDecl): Json {
  const out: { [k: string]: Json } = Object.create(null);
  for (const f of CAP_DECL_FIELDS) out[f] = c[f];
  return out;
}

function slotBindingFromJson(j: Json, p: string): SlotBinding {
  if (!isObj(j)) sv(p, " must be an object");
  for (const k of Object.keys(j)) {
    if (!["variant", "params", "enabled", "locality"].includes(k))
      sv(p, `.${k}: unknown SlotBinding member`);
  }
  const params = get(j, "params");
  if (params !== undefined && !isObj(params)) sv(p, ".params must be an object");
  const locality = get(j, "locality");
  if (locality !== undefined && typeof locality !== "string")
    sv(p, ".locality must be a string");
  if (locality !== undefined && !LOCALITIES.has(locality as string))
    fail("UnknownKind", `locality:${locality}`);
  const out: SlotBinding = {
    variant: cvrFromJson(req(j, "variant", p), `${p}.variant`),
    params: (params as { [k: string]: Json }) ?? {},
    enabled: boolAtOr(j, "enabled", p, true),
  };
  if (locality !== undefined) out.locality = locality as string;
  return out;
}
function slotBindingJson(b: SlotBinding, semantic: boolean): Json {
  const out: { [k: string]: Json } = {
    variant: cvrJson(b.variant, semantic),
    params: b.params,
    enabled: b.enabled,
  };
  if (b.locality !== undefined) out["locality"] = b.locality;
  return out;
}
function slotsFromJson(j: Json, p: string): { [k: string]: SlotBindings } {
  if (!isObj(j)) sv(p, " must be an object");
  const out: { [k: string]: SlotBindings } = Object.create(null);
  for (const [name, v] of Object.entries(j)) {
    const one = get(v, "one");
    const many = get(v, "many");
    if (one !== undefined) {
      out[name] = { one: slotBindingFromJson(one, `${p}.${name}.one`) };
    } else if (Array.isArray(many)) {
      out[name] = {
        many: many.map((b, i) => slotBindingFromJson(b, `${p}.${name}.many[${i}]`)),
      };
    } else {
      sv(p, `.${name} must be one|many`);
    }
  }
  return out;
}
function slotsJson(slots: { [k: string]: SlotBindings }, semantic: boolean): Json {
  const out: { [k: string]: Json } = Object.create(null);
  for (const [name, bs] of Object.entries(slots)) {
    out[name] =
      "one" in bs
        ? { one: slotBindingJson(bs.one, semantic) }
        : { many: bs.many.map((b) => slotBindingJson(b, semantic)) };
  }
  return out;
}

function suppliesFromJson(j: Json, p: string): Supplies {
  const refs = (k: string) => {
    const v = get(j, k);
    return refsFromJson(v === undefined ? [] : v, `${p}.${k}`);
  };
  return { context: refs("context"), tools: refs("tools"), procedures: refs("procedures") };
}
function suppliesJson(s: Supplies, semantic: boolean): Json {
  return {
    context: s.context.map((r) => refJson(r, semantic)),
    tools: s.tools.map((r) => refJson(r, semantic)),
    procedures: s.procedures.map((r) => refJson(r, semantic)),
  };
}

function actionFromJson(j: Json, p: string): RuleAction {
  const single = (k: string, t: string): RuleAction | null => {
    const v = get(j, k);
    return v === undefined ? null : { v: t, r: refFromJson(v, `${p}.${k}`) };
  };
  let a = single("insert_context_item", "insert_context_item");
  if (a) return a;
  const rs = get(j, "restrict_tool_set");
  if (rs !== undefined)
    return { v: "restrict_tool_set", rs: refsFromJson(rs, `${p}.restrict_tool_set`) };
  for (const k of [
    "set_compaction_policy", "set_retry_stop_policy", "request_approval",
    "flow_policy", "pre_authorize", "auto_review", "consolidate_memories",
  ]) {
    const v = get(j, k);
    if (v !== undefined) return { v: k, j: v };
  }
  a = single("require_validator", "require_validator");
  if (a) return a;
  const d = get(j, "declassify");
  if (d !== undefined) {
    if (!Array.isArray(d)) sv(p, ".declassify must be an array");
    return { v: "declassify", j: strArr(d, `${p}.declassify`) };
  }
  const san = get(j, "sanitize");
  if (san !== undefined) {
    return {
      v: "sanitize",
      s: reqStr(san, "sanitizer_ref", `${p}.sanitize`),
      j: get(san, "param") ?? null,
    };
  }
  sv(p, ": unknown rule action");
}
function actionJson(a: RuleAction, semantic: boolean): Json {
  switch (a.v) {
    case "insert_context_item":
      return { insert_context_item: refJson(a.r as Ref, semantic) };
    case "restrict_tool_set":
      return { restrict_tool_set: (a.rs as Ref[]).map((r) => refJson(r, semantic)) };
    case "require_validator":
      return { require_validator: refJson(a.r as Ref, semantic) };
    case "declassify":
      return { declassify: a.j as Json };
    case "sanitize":
      return { sanitize: { sanitizer_ref: a.s as string, param: a.j as Json } };
    default:
      return { [a.v]: a.j as Json };
  }
}

function grantFromJson(j: Json, p: string): Grant {
  const cj = req(j, "constraints", p);
  const c: { budget?: Json; time?: number; count?: number } = {};
  const bj = get(cj, "budget");
  if (bj !== undefined) c.budget = bj;
  for (const k of ["time", "count"] as const) {
    const v = get(cj, k);
    if (v === undefined) continue;
    if (typeof v !== "number" || !Number.isInteger(v) || v < 0)
      sv(p, `.constraints.${k} must be a non-negative int`);
    c[k] = v;
  }
  const delegable = get(j, "delegable");
  if (typeof delegable !== "boolean") sv(p, ".delegable must be a bool");
  return {
    effect: effectClassFromJson(req(j, "effect", p), `${p}.effect`),
    scope: reqStr(j, "scope", p),
    constraints: c,
    delegable,
  };
}
function grantJson(g: Grant, semantic: boolean): Json {
  const c: { [k: string]: Json } = {};
  if (g.constraints.budget !== undefined) c["budget"] = g.constraints.budget;
  if (g.constraints.time !== undefined) c["time"] = g.constraints.time;
  if (g.constraints.count !== undefined) c["count"] = g.constraints.count;
  return {
    effect: effectClassJson(g.effect, semantic),
    scope: g.scope,
    constraints: c,
    delegable: g.delegable,
  };
}

function toolEffectsFromJson(j: Json, p: string): ToolEffects {
  if (j === "pure") return "pure";
  if (isObj(j)) {
    const d = req(j, "declared", p);
    if (!Array.isArray(d)) sv(p, ".declared must be an array");
    return { declared: effectSet(d.map((e, i) => effectClassFromJson(e, `${p}.declared[${i}]`))) };
  }
  sv(p, '.effects must be "pure" or {declared:[...]}');
}
function toolEffectsJson(e: ToolEffects, semantic: boolean): Json {
  if (e === "pure") return "pure";
  return { declared: e.declared.map((x) => effectClassJson(x, semantic)) };
}
function resourcesFromJson(j: Json, p: string): Resources {
  const d = get(j, "declared");
  if (d !== undefined) {
    if (!Array.isArray(d)) sv(p, ".declared must be an array");
    return { declared: sortedSet(strArr(d, `${p}.declared`)) };
  }
  const s = optStr(j, "state");
  if (s === "none" || s === "unknown") return { state: s };
  sv(p, ": resources must be declared|none|unknown");
}
function resourcesJson(r: Resources): Json {
  if (r.declared !== undefined) return { declared: [...r.declared] };
  return { state: r.state as string };
}
/** `tools::scope_bindings` — decode the bindings member into rows. */
function scopeBindingsRows(s: ScopeBindings, p: string): { param_path: string; scope_kind: string }[] | null {
  if (s === "unknown") return null;
  const rows = s === null ? [] : Array.isArray(s) ? s : [s];
  return rows.map((r, i) => {
    const pp = optStr(r, "param_path");
    if (pp === undefined) fail("SchemaViolation", `scope_bindings[${i}].param_path missing`);
    const sk = optStr(r, "scope_kind");
    if (sk === undefined) fail("SchemaViolation", `scope_bindings[${i}].scope_kind missing`);
    if (!SCOPE_KINDS.has(sk)) fail("UnknownKind", `scope_kind ${sk}`);
    return { param_path: pp, scope_kind: sk };
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// Semantic records (schema.rs `semantic_record_{json,from_json}`) — the
// five exercised kinds. Unexercised registered kinds decode as `raw` and
// flag the case unported (the arm then omits the answer).
// ─────────────────────────────────────────────────────────────────────────────

type NativeBody = {
  harness_def: Ref;
  profile: ProfileRef;
  slots: { [k: string]: SlotBindings };
  control_boundary: Boundary;
  budget: Ref;
  permissions: Ref;
  environment: string;
};
type HostedBody = {
  participant_ref: string;
  declared_capabilities: CapDecl;
  observability_levels: string[];
  participant_version: string;
  version_identity?: string;
  hosting_mechanism: string;
  supplies: Supplies;
  budget: Ref;
  permissions: Ref;
  params?: Json;
};
type ToolRec = {
  purpose: TextLeaf;
  input_schema: Json;
  output_schema?: Json;
  effects: ToolEffects;
  preconditions: string[];
  scope_bindings: ScopeBindings;
  resources: Resources;
  observation_contract: Json;
  cost_model?: Json;
  execution_requirement: Json;
  source: Json;
  exposure_hint: Json;
  postconditions: Ref[];
  action_patterns: { fields: string[] }[];
};
type Sem =
  | { k: "AgentProcess"; native?: NativeBody; hosted?: HostedBody }
  | { k: "Budget"; dims: { [d: string]: { hard?: number; soft?: number } }; scope: string; parent?: Ref; accounting: Ref }
  | { k: "HarnessRule"; rule_id: string; trigger: Json; action: RuleAction; scope: Json; conditioned_on?: ProfileRef }
  | { k: "Permission"; holder: Ref; grants: Grant[]; issuer: { authority: number; reference: string }; validity: Validity; revocation?: Json }
  | { k: "ToolCapability"; t: ToolRec }
  | { k: string; raw: Json }; // unexercised kind — carried, never judged

function semanticFromJson(
  kind: string,
  j: Json,
  p: string,
  unported: string[]
): Sem {
  switch (kind) {
    case "AgentProcess": {
      const bj = req(j, "body", p);
      const n = get(bj, "native");
      const h = get(bj, "hosted");
      if (n !== undefined) {
        const np = `${p}.body.native`;
        return {
          k: "AgentProcess",
          native: {
            harness_def: refFromJson(req(n, "harness_def", np), `${np}.harness_def`),
            profile: profileRefFromJson(req(n, "profile", np), `${np}.profile`),
            slots: slotsFromJson(req(n, "slots", np), `${np}.slots`),
            control_boundary: boundaryFromJson(req(n, "control_boundary", np), `${np}.control_boundary`),
            budget: refFromJson(req(n, "budget", np), `${np}.budget`),
            permissions: refFromJson(req(n, "permissions", np), `${np}.permissions`),
            environment: reqStr(req(n, "environment", np), "environment", `${np}.environment`),
          },
        };
      }
      if (h !== undefined) {
        const hp = `${p}.body.hosted`;
        const mech = reqStr(h, "hosting_mechanism", hp);
        if (mech === "none")
          sv(hp, ".hosting_mechanism: none on OpaqueProcess (CF-351)");
        if (!HOSTING.has(mech)) fail("UnknownKind", `hosting mechanism ${mech}`);
        const obs = arr(h, "observability_levels", hp).map((o) => {
          if (typeof o !== "string") sv(hp, ".observability_levels must be strings");
          if (!OBSERVABILITY.has(o)) fail("UnknownKind", `observability ${o}`);
          return o;
        });
        const hosted: HostedBody = {
          participant_ref: reqStr(h, "participant_ref", hp),
          declared_capabilities: capDeclFromJson(req(h, "declared_capabilities", hp), `${hp}.declared_capabilities`),
          observability_levels: obs,
          participant_version: reqStr(h, "participant_version", hp),
          hosting_mechanism: mech,
          supplies: suppliesFromJson(req(h, "supplies", hp), `${hp}.supplies`),
          budget: refFromJson(req(h, "budget", hp), `${hp}.budget`),
          permissions: refFromJson(req(h, "permissions", hp), `${hp}.permissions`),
        };
        const vi = optStr(h, "version_identity");
        if (vi !== undefined) hosted.version_identity = vi;
        const params = get(h, "params");
        if (params !== undefined) hosted.params = params;
        return { k: "AgentProcess", hosted };
      }
      return sv(p, ".body must be native|hosted");
    }
    case "Budget": {
      const dj = req(j, "dimensions", p);
      if (!isObj(dj)) sv(p, ".dimensions must be an object");
      const dims: { [d: string]: { hard?: number; soft?: number } } = Object.create(null);
      for (const [dn, v] of Object.entries(dj)) {
        const bound: { hard?: number; soft?: number } = {};
        const hd = get(v, "hard");
        if (typeof hd === "number" && Number.isInteger(hd) && hd >= 0) bound.hard = hd;
        const sd = get(v, "soft");
        if (typeof sd === "number" && Number.isInteger(sd) && sd >= 0) bound.soft = sd;
        dims[dn] = bound;
      }
      const parent = get(j, "parent");
      const out: Sem = {
        k: "Budget",
        dims,
        scope: reqStr(j, "scope", p),
        accounting: refFromJson(req(j, "accounting", p), `${p}.accounting`),
      };
      if (parent !== undefined) out.parent = refFromJson(parent, `${p}.parent`);
      return out;
    }
    case "HarnessRule": {
      if (has(j, "assumption_debt")) unported.push(`${p}.assumption_debt`);
      const co = get(j, "conditioned_on");
      const out: Sem = {
        k: "HarnessRule",
        rule_id: reqStr(j, "rule_id", p),
        trigger: req(j, "trigger", p),
        action: actionFromJson(req(j, "action", p), `${p}.action`),
        scope: req(j, "scope", p),
      };
      if (co !== undefined)
        out.conditioned_on = profileRefFromJson(co, `${p}.conditioned_on`);
      return out;
    }
    case "Permission": {
      const ij = req(j, "issuer", p);
      const out: Sem = {
        k: "Permission",
        holder: refFromJson(req(j, "holder", p), `${p}.holder`),
        grants: arr(j, "grants", p).map((g, i) => grantFromJson(g, `${p}.grants[${i}]`)),
        issuer: {
          authority: authorityParse(reqStr(ij, "authority", `${p}.issuer`), `${p}.issuer.authority`),
          reference: reqStr(ij, "reference", `${p}.issuer`),
        },
        validity: validityFromJson(req(j, "validity", p), `${p}.validity`),
      };
      const rev = get(j, "revocation");
      if (rev !== undefined) out.revocation = rev;
      return out;
    }
    case "ToolCapability": {
      const sb: ScopeBindings = has(j, "scope_bindings_unknown")
        ? "unknown"
        : get(j, "scope_bindings") ?? null;
      const pre = arr(j, "preconditions", p).map((x) => {
        if (typeof x !== "string") sv(p, ".preconditions members must be strings");
        if (!PRECONDITION_DOMAINS.has(x)) fail("UnknownKind", `precondition domain ${x}`);
        return x;
      });
      const ap = get(j, "action_patterns");
      let action_patterns: { fields: string[] }[] = [];
      if (ap !== undefined) {
        if (!Array.isArray(ap)) sv(p, ".action_patterns must be an array");
        action_patterns = ap.map((r, i) => {
          const fs = get(r, "fields");
          if (!Array.isArray(fs)) sv(p, `.action_patterns[${i}].fields missing`);
          return { fields: strArr(fs, `${p}.action_patterns[${i}].fields`) };
        });
      }
      if (has(j, "flow_contract")) unported.push(`${p}.flow_contract`);
      const t: ToolRec = {
        purpose: textFromJson(req(j, "purpose", p), `${p}.purpose`),
        input_schema: req(j, "input_schema", p),
        effects: toolEffectsFromJson(req(j, "effects", p), `${p}.effects`),
        preconditions: pre,
        scope_bindings: sb,
        resources: resourcesFromJson(req(j, "resources", p), `${p}.resources`),
        observation_contract: req(j, "observation_contract", p),
        execution_requirement: req(j, "execution_requirement", p),
        source: req(j, "source", p),
        exposure_hint: req(j, "exposure_hint", p),
        postconditions: refsFromJson(req(j, "postconditions", p), `${p}.postconditions`),
        action_patterns,
      };
      const os = get(j, "output_schema");
      if (os !== undefined) t.output_schema = os;
      const cm = get(j, "cost_model");
      if (cm !== undefined) t.cost_model = cm;
      // `flow_contract` is flagged unported at decode above — the case
      // never reaches encode.
      return { k: "ToolCapability", t };
    }
    default:
      unported.push(`${p} (kind ${kind})`);
      return { k, raw: j };
  }
}

/** `semantic_record_json(rec, semantic)` — the faithful re-emit for the
 *  ported kinds; `semantic` drops provenance, selector spellings and the
 *  non-semantic members (`exposure_hint`, `cost_model.measured_ref`,
 *  `version_identity`). */
function semanticJson(rec: Sem, semantic: boolean): Json {
  switch (rec.k) {
    case "AgentProcess": {
      let body: Json;
      if (rec.native !== undefined) {
        const n = rec.native;
        body = {
          native: {
            harness_def: refJson(n.harness_def, semantic),
            profile: profileRefJson(n.profile),
            slots: slotsJson(n.slots, semantic),
            control_boundary: boundaryJson(n.control_boundary),
            budget: refJson(n.budget, semantic),
            permissions: refJson(n.permissions, semantic),
            environment: { environment: n.environment },
          },
        };
      } else {
        const h = rec.hosted as HostedBody;
        const o: { [k: string]: Json } = {
          participant_ref: h.participant_ref,
          declared_capabilities: capDeclJson(h.declared_capabilities),
          observability_levels: [...h.observability_levels],
          participant_version: h.participant_version,
          hosting_mechanism: h.hosting_mechanism,
          supplies: suppliesJson(h.supplies, semantic),
          budget: refJson(h.budget, semantic),
          permissions: refJson(h.permissions, semantic),
        };
        // `version_identity` is part of the semantic projection (it is
        // derived content), unlike the other non-semantic members.
        if (h.version_identity !== undefined)
          o["version_identity"] = h.version_identity;
        if (h.params !== undefined) o["params"] = h.params;
        body = { hosted: o };
      }
      return { body };
    }
    case "Budget": {
      const dims: { [k: string]: Json } = Object.create(null);
      for (const [d, b] of Object.entries(rec.dims)) {
        const bound: { [k: string]: Json } = {};
        if (b.hard !== undefined) bound["hard"] = b.hard;
        if (b.soft !== undefined) bound["soft"] = b.soft;
        dims[d] = bound;
      }
      const out: { [k: string]: Json } = {
        dimensions: dims,
        scope: rec.scope,
        accounting: refJson(rec.accounting, semantic),
      };
      if (rec.parent !== undefined) out["parent"] = refJson(rec.parent, semantic);
      return out;
    }
    case "HarnessRule": {
      const out: { [k: string]: Json } = {
        rule_id: rec.rule_id,
        trigger: rec.trigger,
        action: actionJson(rec.action, semantic),
        scope: rec.scope,
      };
      if (rec.conditioned_on !== undefined)
        out["conditioned_on"] = profileRefJson(rec.conditioned_on);
      return out;
    }
    case "Permission": {
      const out: { [k: string]: Json } = {
        holder: refJson(rec.holder, semantic),
        grants: rec.grants.map((g) => grantJson(g, semantic)),
        issuer: {
          authority: AUTHORITY_NAME[rec.issuer.authority],
          reference: rec.issuer.reference,
        },
        validity: validityJson(rec.validity),
      };
      if (rec.revocation !== undefined) out["revocation"] = rec.revocation;
      return out;
    }
    case "ToolCapability": {
      const t = rec.t;
      const out: { [k: string]: Json } = {
        purpose: textJson(t.purpose, semantic),
        input_schema: t.input_schema,
        effects: toolEffectsJson(t.effects, semantic),
        preconditions: [...t.preconditions],
        resources: resourcesJson(t.resources),
        observation_contract: t.observation_contract,
        execution_requirement: t.execution_requirement,
        source: t.source,
        postconditions: t.postconditions.map((r) => refJson(r, semantic)),
      };
      if (!semantic) out["exposure_hint"] = t.exposure_hint;
      if (t.output_schema !== undefined) out["output_schema"] = t.output_schema;
      if (t.scope_bindings === "unknown") out["scope_bindings_unknown"] = true;
      else out["scope_bindings"] = t.scope_bindings;
      if (t.cost_model !== undefined) {
        let cm = t.cost_model;
        if (semantic && isObj(cm)) {
          const { measured_ref: _drop, ...rest } = cm;
          cm = rest;
        }
        out["cost_model"] = cm;
      }
      if (t.action_patterns.length > 0)
        out["action_patterns"] = t.action_patterns.map((p) => ({ fields: [...p.fields] }));
      return out;
    }
    default:
      return rec.raw;
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Surface / edge / node / document decode + encode
// ─────────────────────────────────────────────────────────────────────────────

type Surface = { t: "Tool"; s: ToolSurface } | { t: string; raw: Json };
type ToolSurface = {
  name: string;
  namespace: string;
  description_template: TextLeaf;
  argument_order: string[];
  examples: Json;
  error_format: Json;
  result_renderer: Json;
  strictness: Json;
  schema_dialect_narrowing: Json;
  exposure_mode: Json;
  display_title?: string;
  icon_ref?: string;
};

function surfaceFromJson(kind: string, j: Json, p: string): Surface {
  if (!HAS_SURFACE.has(kind))
    fail("UnexpressibleSurface", `${p}: kind ${kind} carries no surface record (§3.1.5)`);
  if (kind === "ToolCapability") {
    const s: ToolSurface = {
      name: reqStr(j, "name", p),
      namespace: reqStr(j, "namespace", p),
      description_template: textFromJson(req(j, "description_template", p), `${p}.description_template`),
      argument_order: strArr(arr(j, "argument_order", p), `${p}.argument_order`),
      examples: get(j, "examples") ?? [],
      error_format: get(j, "error_format") ?? null,
      result_renderer: get(j, "result_renderer") ?? null,
      strictness: get(j, "strictness") ?? null,
      schema_dialect_narrowing: get(j, "schema_dialect_narrowing") ?? null,
      exposure_mode: get(j, "exposure_mode") ?? null,
    };
    const dt = optStr(j, "display_title");
    if (dt !== undefined) s.display_title = dt;
    const ir = optStr(j, "icon_ref");
    if (ir !== undefined) s.icon_ref = ir;
    return { t: "Tool", s };
  }
  // Validator/Procedure/ContextItem surfaces: the host kind is unported,
  // so the surface is carried raw (the case answers "missing").
  return { t: kind, raw: j };
}
function surfaceJson(s: Surface): Json {
  if (s.t !== "Tool") return s.raw;
  const t = s.s;
  const out: { [k: string]: Json } = {
    name: t.name,
    namespace: t.namespace,
    description_template: textJson(t.description_template, false),
    argument_order: [...t.argument_order],
    examples: t.examples,
    error_format: t.error_format,
    result_renderer: t.result_renderer,
    strictness: t.strictness,
    schema_dialect_narrowing: t.schema_dialect_narrowing,
    exposure_mode: t.exposure_mode,
  };
  if (t.display_title !== undefined) out["display_title"] = t.display_title;
  if (t.icon_ref !== undefined) out["icon_ref"] = t.icon_ref;
  return out;
}

type Version = {
  version_id?: string;
  semantic_id?: string;
  dialect: string;
  supersedes?: string;
  sealed: boolean;
};
type Node = {
  kind: string;
  provenance: Prov;
  semantic: Sem;
  surface?: Surface;
  ext: { [k: string]: Json };
  version: Version;
};
type Edge = {
  kind: string;
  from: string;
  to: string;
  provenance: Prov;
  fields: Json; // carried raw — edge-bearing cases are answered "missing"
  version: Version;
};
type Doc = {
  hir_version: string;
  root: Ref;
  nodes: Node[];
  edges: Edge[];
  assembly?: Json;
  unported: string[];
};

function versionFromJson(j: Json, p: string): Version {
  const out: Version = {
    dialect: "HIR/1",
    sealed: boolAtOr(j, "sealed", p, false),
  };
  const vi = optStr(j, "version_id");
  if (vi !== undefined) out.version_id = vi;
  const si = optStr(j, "semantic_id");
  if (si !== undefined) out.semantic_id = si;
  const sup = optStr(j, "supersedes");
  if (sup !== undefined) out.supersedes = sup;
  return out;
}
function versionJson(v: Version): Json {
  const out: { [k: string]: Json } = { sealed: v.sealed };
  if (v.version_id !== undefined) out["version_id"] = v.version_id;
  if (v.semantic_id !== undefined) out["semantic_id"] = v.semantic_id;
  if (v.supersedes !== undefined) out["supersedes"] = v.supersedes;
  return out;
}

function nodeFromJson(j: Json, p: string, unported: string[]): Node {
  const kindName = reqStr(j, "kind", p);
  if (!ENTITY_KINDS.has(kindName)) fail("UnknownKind", kindName);
  const dialect = reqStr(j, "dialect", p);
  if (dialect !== "HIR/1") fail("DialectUnsupported", dialect);
  const provJ = get(j, "provenance");
  if (provJ === undefined)
    throw new HirFail({
      kind: "MissingProvenance",
      detail: `${p} (node ${kindName})`,
    });
  const provenance = provenanceFromJson(provJ, `${p}.provenance`);
  const semantic = semanticFromJson(
    kindName,
    req(j, "semantic", p),
    `${p}.semantic`,
    unported
  );
  const surfaceJ = get(j, "surface");
  const surface =
    surfaceJ === undefined
      ? undefined
      : surfaceFromJson(kindName, surfaceJ, `${p}.surface`);
  const extJ = get(j, "ext");
  let ext: { [k: string]: Json } = {};
  if (extJ !== undefined) {
    if (!isObj(extJ)) sv(p, ".ext must be an object");
    ext = Object.create(null);
    for (const [k, v] of Object.entries(extJ)) ext[k] = v;
  }
  const version = versionFromJson(req(j, "version", p), `${p}.version`);
  version.dialect = dialect;
  const node: Node = { kind: kindName, provenance, semantic, ext, version };
  if (surface !== undefined) node.surface = surface;
  return node;
}

function nodeToJson(n: Node): Json {
  const out: { [k: string]: Json } = {
    kind: n.kind,
    dialect: n.version.dialect,
    provenance: provenanceJson(n.provenance),
    semantic: semanticJson(n.semantic, false),
    version: versionJson(n.version),
  };
  if (n.surface !== undefined) out["surface"] = surfaceJson(n.surface);
  if (Object.keys(n.ext).length > 0) out["ext"] = n.ext;
  return out;
}
function nodeSemanticProjection(n: Node): Json {
  return { kind: n.kind, semantic: semanticJson(n.semantic, true) };
}
function nodeIdentityBasis(n: Node): Json {
  const basis = nodeToJson(n) as { [k: string]: Json };
  const v = { ...(basis["version"] as { [k: string]: Json }) };
  delete v["version_id"];
  delete v["semantic_id"];
  basis["version"] = v;
  return basis;
}

function edgeFromJson(j: Json, p: string, unported: string[]): Edge {
  const kindName = reqStr(j, "kind", p);
  if (!EDGE_KINDS.has(kindName)) fail("UnknownKind", kindName);
  const dialect = reqStr(j, "dialect", p);
  if (dialect !== "HIR/1") fail("DialectUnsupported", dialect);
  const provJ = get(j, "provenance");
  if (provJ === undefined)
    throw new HirFail({
      kind: "MissingProvenance",
      detail: `${p} (edge ${kindName})`,
    });
  const provenance = provenanceFromJson(provJ, `${p}.provenance`);
  const fields = req(j, "fields", p); // carried raw (case is flagged)
  const version = versionFromJson(req(j, "version", p), `${p}.version`);
  version.dialect = dialect;
  return {
    kind: kindName,
    from: reqStr(j, "from", p),
    to: reqStr(j, "to", p),
    provenance,
    fields,
    version,
  };
}

function documentFromJson(j: Json): Doc {
  const hir_version = reqStr(j, "hir_version", "$");
  if (hir_version !== "HIR/1")
    fail("DialectUnsupported", hir_version);
  const nodesJ = req(j, "nodes", "$");
  if (!Array.isArray(nodesJ)) sv("$", ".nodes must be an array");
  const edgesJ = req(j, "edges", "$");
  if (!Array.isArray(edgesJ)) sv("$", ".edges must be an array");
  const unported: string[] = [];
  const nodes = nodesJ.map((n, i) => nodeFromJson(n, `$.nodes[${i}]`, unported));
  const edges = edgesJ.map((e, i) => edgeFromJson(e, `$.edges[${i}]`, unported));
  if (edges.length > 0) unported.push("$.edges (edge-bearing document)");
  const asm = get(j, "assembly");
  const doc: Doc = {
    hir_version,
    root: refFromJson(req(j, "root", "$"), "$.root"),
    nodes,
    edges,
    unported,
  };
  if (asm !== undefined) doc.assembly = asm;
  return doc;
}

function docToJson(doc: Doc): Json {
  const nodes = doc.nodes
    .map(nodeToJson)
    .sort((a, b) => Buffer.compare(canonical(a), canonical(b)));
  const edges = doc.edges
    .map((e) => {
      const out: { [k: string]: Json } = {
        kind: e.kind,
        dialect: e.version.dialect,
        from: e.from,
        to: e.to,
        provenance: provenanceJson(e.provenance),
        fields: e.fields,
        version: versionJson(e.version),
      };
      return out;
    })
    .sort((a, b) => Buffer.compare(canonical(a), canonical(b)));
  const out: { [k: string]: Json } = {
    hir_version: doc.hir_version,
    root: refJson(doc.root, false),
    nodes,
    edges,
  };
  if (doc.assembly !== undefined) out["assembly"] = doc.assembly;
  return out;
}

// ─────────────────────────────────────────────────────────────────────────────
// Identity (identity.rs) — `idp/1` over the one canonical codec (CC1).
// ─────────────────────────────────────────────────────────────────────────────

function hashTag(domain: string, j: Json): string {
  return idpId(domain, canonical(j));
}
function semanticId(n: Node): string {
  return hashTag("hir.node#semantic", nodeSemanticProjection(n));
}
function versionId(n: Node): string {
  return idpId("hir.node", canonical(nodeIdentityBasis(n)));
}
function nodeSemanticIdOf(n: Node): string {
  return n.version.semantic_id ?? semanticId(n);
}
function opaqueVersionIdentity(decl: CapDecl, participantVersion: string): string {
  return hashTag("capability_declaration", {
    declaration: capDeclJson(decl),
    participant_version: participantVersion,
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// tools.rs — scope kinds / authored exposure / capability checks
// ─────────────────────────────────────────────────────────────────────────────

function domainScopeKind(d: string): string | null {
  switch (d) {
    case "fs_read":
    case "fs_write":
      return "fs_path";
    case "net_egress":
      return "host";
    case "message_human":
      return "recipient";
    case "spend":
      return "spend_amount";
    case "secret_access":
      return "secret_ref";
    case "exec":
    case "spawn_process":
      return "process_target";
    case "memory_write":
      return "memory_scope";
    default:
      return null;
  }
}
function authoredExposure(j: Json | undefined): { hidden: boolean; pinned: boolean; admitted: Set<string> } {
  const out = { hidden: false, pinned: false, admitted: new Set(["direct"]) };
  if (j === undefined || j === null) return out;
  if (!isObj(j)) fail("SchemaViolation", "exposure_mode must be an object");
  const mode = get(j, "mode");
  if (mode === "hidden") out.hidden = true;
  else if (mode !== undefined && mode !== "primitive" && mode !== "composite_member")
    fail("UnknownKind", `exposure_mode.mode ${mode}`);
  const pinned = get(j, "pinned");
  if (pinned !== undefined) {
    if (typeof pinned !== "boolean")
      fail("SchemaViolation", "exposure_mode.pinned must be a bool");
    out.pinned = pinned;
  }
  const modes = get(j, "admitted_modes");
  if (modes !== undefined && Array.isArray(modes)) {
    out.admitted = new Set();
    for (const m of modes) {
      if (typeof m !== "string")
        fail("SchemaViolation", "exposure_mode.admitted_modes members must be strings");
      if (!EXPOSURE_MODES.has(m)) fail("UnknownKind", `admitted_modes ${m}`);
      out.admitted.add(m);
    }
  }
  return out;
}
function paramPathExists(schema: Json, path: string): boolean {
  let cur = schema;
  for (const seg of path.split(".")) {
    if (
      has(cur, "anyOf") || has(cur, "oneOf") || has(cur, "allOf") || has(cur, "$ref")
    )
      return true;
    const next = get(get(cur, "properties"), seg);
    if (next === undefined) {
      const additional =
        get(cur, "additionalProperties") === true ||
        isObj(get(cur, "additionalProperties")) ||
        get(cur, "unevaluatedProperties") === true ||
        isObj(get(cur, "unevaluatedProperties"));
      const unconstrained =
        get(cur, "properties") === undefined &&
        get(cur, "type") !== "object";
      return additional || unconstrained;
    }
    cur = next;
  }
  return true;
}
function liftedSourceKind(source: Json): string | null {
  const k = get(source, "kind");
  return k === "mcp_listing" || k === "participant_supplied" ? k : null;
}
function isDiscoveryCapability(t: ToolRec): boolean {
  return get(t.exposure_hint, "discovery") === true;
}
function capabilityNeedsFlowContract(t: ToolRec): boolean {
  if (t.effects === "pure") return false;
  for (const e of t.effects.declared) {
    const gated = ["net_egress", "message_human", "fs_read", "memory_write"].includes(e.domain);
    const open = e.attributes !== null && e.attributes.world === "open";
    if (gated || open) return true;
  }
  return false;
}
/** `tools::validate_capability` — the ToolCapability member battery. */
function validateCapability(t: ToolRec, authority: number): HirErr[] {
  const errs: HirErr[] = [];
  if (t.effects !== "pure" && t.effects.declared.length === 0)
    errs.push({ kind: "EmptyEffectSet" });
  // `scope_bindings()` errors are findings, not propagation (E1 pushes
  // the decode error into `errs`).
  let rows: { param_path: string; scope_kind: string }[] | null = null;
  if (t.scope_bindings !== "unknown") {
    try {
      rows = scopeBindingsRows(t.scope_bindings, "scope_bindings");
    } catch (e) {
      if (e instanceof HirFail) errs.push(e.hir);
      else throw e;
      // E1's `Err(e) => errs.push(e)` arm skips both loops.
      rows = null;
    }
  }
  if (rows !== null) {
    for (const b of rows) {
      if (!paramPathExists(t.input_schema, b.param_path))
        errs.push({ kind: "UnmappedParameter", detail: b.param_path });
    }
    if (t.effects !== "pure") {
      for (const e of t.effects.declared) {
        const kind = domainScopeKind(e.domain);
        if (kind !== null && !rows.some((b) => b.scope_kind === kind))
          errs.push({ kind: "UnscopedParameter", detail: e.domain });
      }
    }
  }
  const lifted = liftedSourceKind(t.source);
  if (lifted !== null && authority !== AUTHORITY["unverified"])
    errs.push({ kind: "LiftedDeclarationsNotUnverified", detail: lifted });
  if (t.cost_model !== undefined) {
    const hasMeasured = has(t.cost_model, "measured_ref");
    const declared = get(t.cost_model, "declared");
    if (isObj(declared)) {
      for (const [dim, est] of Object.entries(declared)) {
        if (get(est, "confidence") === "exact" && !hasMeasured)
          errs.push({ kind: "ExactCostUnmeasured", detail: dim });
      }
    }
  }
  // `flow_contract` presence is flagged unported at decode (its check
  // needs the FlowContract member decode); the guard here is defensive.
  if ((t as { flow_contract?: Json }).flow_contract !== undefined)
    throw new HirUnsupported("tool.flow_contract");
  return errs;
}

// ─────────────────────────────────────────────────────────────────────────────
// validate.rs — the corpus-reached check battery
// ─────────────────────────────────────────────────────────────────────────────

function checkTextLeaf(t: TextLeaf, path: string, errs: HirErr[]): void {
  // `check_authority`: authority > external requires a kernel/human origin
  // or a self-consistent seal|pin attestation.
  if (t.authority > AUTHORITY["external"]) {
    const ok =
      t.provenance.origin.kind === "kernel" ||
      t.provenance.origin.kind === "human" ||
      (t.provenance.attestation !== null &&
        selfConsistent(t.provenance.attestation) &&
        (t.provenance.attestation.kind === "seal" ||
          t.provenance.attestation.kind === "pin"));
    if (!ok)
      errs.push({
        kind: "TextAboveExternal",
        detail: `${path}: ${AUTHORITY_NAME[t.authority]}`,
      });
  }
  // `content_consistent` is vacuous — `Text::from_json` carries no
  // `content` member into the replayed record.
  checkProvenance(t.provenance, path, errs);
}

function checkMemberProvenance(p: Prov, sealed: boolean, what: string, errs: HirErr[]): void {
  checkProvenance(p, what, errs);
  if (
    sealed &&
    p.scope === "definition" &&
    p.authority < AUTHORITY["definition"]
  ) {
    errs.push({
      kind: "ScopeCeilingExceeded",
      detail: `${what}: sealed member scoped definition carries ${AUTHORITY_NAME[p.authority]}`,
    });
  }
}

function checkExt(ext: { [k: string]: Json }, errs: HirErr[]): void {
  for (const k of Object.keys(ext)) {
    if (!k.includes("/"))
      errs.push({
        kind: "SchemaViolation",
        detail: `ext key "${k}" is not a prefixed key (org.name/key)`,
      });
    else if (k.startsWith("hir/"))
      errs.push({
        kind: "SchemaViolation",
        detail: `ext key "${k}" uses the reserved hir/ namespace`,
      });
  }
}

const DENY_KEYS = new Set([
  "model", "model_id", "model_name", "model_ref", "model_identifier",
  "provider", "llm", "llm_config",
]);
function scanKeys(j: Json, hits: string[]): void {
  if (isObj(j)) {
    for (const [k, v] of Object.entries(j)) {
      if (DENY_KEYS.has(k)) hits.push(k);
      scanKeys(v, hits);
    }
  } else if (Array.isArray(j)) {
    for (const v of j) scanKeys(v, hits);
  }
}
function checkModelIdentityStatics(n: Node, errs: HirErr[]): void {
  const hits: string[] = [];
  scanKeys(semanticJson(n.semantic, false), hits);
  for (const h of hits)
    errs.push({
      kind: "UnexpressibleSurface",
      detail: `${n.kind}: model-identity field "${h}" outside ProfileRef/surface (T-LCD-01)`,
    });
}

function checkHosted(h: HostedBody, errs: HirErr[]): void {
  if (h.version_identity !== undefined) {
    const expected = opaqueVersionIdentity(
      h.declared_capabilities,
      h.participant_version
    );
    if (h.version_identity !== expected)
      errs.push({
        kind: "SchemaViolation",
        detail: "hosted version_identity != H(declaration ∥ participant_version)",
      });
  }
}

function checkKindRecord(n: Node, errs: HirErr[]): void {
  const rec = n.semantic;
  switch (rec.k) {
    case "AgentProcess":
      if (rec.native !== undefined) {
        for (const required of ["control_strategy", "context_policy"]) {
          if (!(required in rec.native.slots))
            errs.push({
              kind: "SchemaViolation",
              detail: `native process missing slot "${required}"`,
            });
        }
        for (const [point, owner] of Object.entries(rec.native.control_boundary.assignments)) {
          if (point === "authorize" && owner !== "code")
            errs.push({
              kind: "SchemaViolation",
              detail: `control_boundary: BoundaryError { point: Authorize, owner: ${owner} }`,
            });
        }
      } else if (rec.hosted !== undefined) {
        checkHosted(rec.hosted, errs);
      }
      break;
    case "Budget":
      for (const [d, bound] of Object.entries(rec.dims)) {
        if (!DIMENSION_KEYS.has(d))
          errs.push({
            kind: "SchemaViolation",
            detail: `budget dimension "${d}": not in the kernel dimension registry (§8.2)`,
          });
        if (bound.hard !== undefined && bound.soft !== undefined && bound.soft > bound.hard)
          errs.push({
            kind: "SchemaViolation",
            detail: `budget dimension "${d}": soft ${bound.soft} > hard ${bound.hard}`,
          });
      }
      break;
    case "HarnessRule":
      // `assumption_debt` presence is flagged unported at decode; a
      // `conditioned_on` rule with no complete debt refuses.
      if (rec.conditioned_on !== undefined)
        errs.push({ kind: "ConditionedRuleIncomplete", detail: rec.rule_id });
      break;
    case "Permission":
      if (rec.issuer.authority === AUTHORITY["delegate"])
        errs.push({
          kind: "IllegitimateEndorsement",
          detail: "permission issued at delegate (model-claim) authority",
        });
      if (rec.validity.until !== null && rec.validity.condition !== null)
        errs.push({
          kind: "SchemaViolation",
          detail: "permission.validity carries both until and condition",
        });
      break;
    case "ToolCapability": {
      checkTextLeaf(rec.t.purpose, "tool.purpose", errs);
      errs.push(...validateCapability(rec.t, n.provenance.authority));
      if (n.surface !== undefined && n.surface.t === "Tool") {
        // `authored_exposure` errors surface as validation findings.
        try {
          authoredExposure(n.surface.s.exposure_mode);
        } catch (e) {
          if (e instanceof HirFail) errs.push(e.hir);
          else throw e;
        }
      }
      break;
    }
    default:
      break;
  }
}

type RefSite = { path: string; allowed: string[] };
function collectRefs(rec: Sem, out: { site: RefSite; r: Ref }[]): void {
  const push = (path: string, allowed: string[], r: Ref) =>
    out.push({ site: { path, allowed }, r });
  switch (rec.k) {
    case "Permission":
      push("holder", ["AgentProcess"], rec.holder);
      break;
    case "Budget":
      if (rec.parent !== undefined) push("parent", ["Budget"], rec.parent);
      push("accounting", [], rec.accounting);
      break;
    case "AgentProcess":
      if (rec.native !== undefined) {
        const n = rec.native;
        push("harness_def", ["AgentProcess"], n.harness_def);
        push("budget", ["Budget"], n.budget);
        push("permissions", ["Permission"], n.permissions);
      } else if (rec.hosted !== undefined) {
        const h = rec.hosted;
        for (const r of h.supplies.context) push("supplies.context", ["ContextItem"], r);
        for (const r of h.supplies.tools) push("supplies.tools", ["ToolCapability"], r);
        for (const r of h.supplies.procedures) push("supplies.procedures", ["Procedure"], r);
        push("budget", ["Budget"], h.budget);
        push("permissions", ["Permission"], h.permissions);
      }
      break;
    case "HarnessRule":
      if (rec.action.v === "insert_context_item")
        push("action", ["ContextItem"], rec.action.r as Ref);
      else if (rec.action.v === "restrict_tool_set")
        for (const r of rec.action.rs as Ref[]) push("action", ["ToolCapability"], r);
      else if (rec.action.v === "require_validator")
        push("action", ["Validator"], rec.action.r as Ref);
      break;
    case "ToolCapability": {
      for (const r of rec.t.postconditions)
        push("postconditions", ["Validator", "Observation"], r);
      if (rec.t.effects !== "pure") {
        for (const e of rec.t.effects.declared) {
          if (e.attributes !== null && "pin" in e.attributes.reversibility)
            push("reversibility", ["Procedure"], e.attributes.reversibility.pin);
        }
      }
      break;
    }
    default:
      break;
  }
}

function validateDoc(doc: Doc): HirErr[] | null {
  const errs: HirErr[] = [];
  if (doc.hir_version !== "HIR/1") {
    return [{ kind: "DialectUnsupported", detail: doc.hir_version }];
  }
  const index = new Map<string, Node>();
  for (const n of doc.nodes) index.set(nodeSemanticIdOf(n), n);

  for (const n of doc.nodes) {
    // check_node_shape: kind/record agreement is by construction;
    // `classify_home_identifier` never fails on the 13 registered names;
    // the surface-placement gate lives in `surfaceFromJson` at parse.
    if (n.surface !== undefined && !HAS_SURFACE.has(n.kind))
      errs.push({
        kind: "UnexpressibleSurface",
        detail: `surface record on ${n.kind}`,
      });
    if (n.version.dialect !== "HIR/1")
      errs.push({ kind: "DialectUnsupported", detail: n.version.dialect });
    checkMemberProvenance(n.provenance, n.version.sealed, "node", errs);
    checkKindRecord(n, errs);
    checkExt(n.ext, errs);
    checkModelIdentityStatics(n, errs);
  }

  // check_refs_and_endpoints (the corpus's ref-resolution leg; edge
  // endpoints and acyclicity are unreachable — edge-bearing cases are
  // flagged unported).
  for (const n of doc.nodes) {
    const refs: { site: RefSite; r: Ref }[] = [];
    collectRefs(n.semantic, refs);
    for (const { site, r } of refs) {
      const target = index.get(r.sid);
      if (target === undefined) {
        errs.push({
          kind: "UnresolvedRef",
          detail: `${n.kind}.${site.path} -> ${r.sid} does not resolve in the document`,
        });
      } else if (site.allowed.length > 0 && !site.allowed.includes(target.kind)) {
        errs.push({
          kind: "UnresolvedRef",
          detail: `${n.kind}.${site.path} -> ${r.sid} (kind ${target.kind} not in allowed)`,
        });
      }
    }
  }

  // check_capability_procedure_sources: a procedure-sourced capability
  // whose `source.ref` resolves to a non-Procedure mismatches.
  for (const n of doc.nodes) {
    if (n.semantic.k !== "ToolCapability") continue;
    const t = n.semantic.t;
    if (get(t.source, "kind") !== "procedure") continue;
    const targetSid = optStr(t.source, "ref");
    const target = targetSid === undefined ? undefined : index.get(targetSid);
    if (target === undefined) continue;
    if (target.semantic.k !== "Procedure") {
      errs.push({
        kind: "DerivedEffectsMismatch",
        detail: `${nodeSemanticIdOf(n)}: source.ref does not resolve to a Procedure`,
      });
      continue;
    }
    // A resolved Procedure is corpus-unported — flagged at decode.
    throw new HirUnsupported("procedure-derived tool effects");
  }

  // check_discovery_capability: a deferred/indexed-admitting tool surface
  // requires a discovery capability in the document.
  // `.map(admitted.contains(deferred|indexed)).unwrap_or(false)` —
  // a malformed exposure_mode counts as not-admitting here (the error
  // itself was already recorded by check_kind_record above).
  const admitsDeferred = doc.nodes.some((n) => {
    if (n.semantic.k !== "ToolCapability") return false;
    if (n.surface === undefined || n.surface.t !== "Tool") return false;
    try {
      const a = authoredExposure(n.surface.s.exposure_mode);
      return a.admitted.has("deferred") || a.admitted.has("indexed");
    } catch {
      return false;
    }
  });
  if (admitsDeferred) {
    const hasDiscovery = doc.nodes.some(
      (n) => n.semantic.k === "ToolCapability" && isDiscoveryCapability(n.semantic.t)
    );
    if (!hasDiscovery) errs.push({ kind: "NoDiscoverySurface" });
  }

  // check_budgets: a budget parent must contain every declared bound.
  for (const n of doc.nodes) {
    if (n.semantic.k !== "Budget" || n.semantic.parent === undefined) continue;
    const pn = index.get(n.semantic.parent.sid);
    if (pn === undefined || pn.semantic.k !== "Budget") continue;
    for (const [dim, bound] of Object.entries(n.semantic.dims)) {
      const pb = pn.semantic.dims[dim];
      if (pb === undefined) continue;
      const le = (a: number | undefined, b: number | undefined) =>
        a === undefined || (b !== undefined && a <= b);
      if (!le(bound.hard, pb.hard) || !le(bound.soft, pb.soft))
        errs.push({
          kind: "BudgetExceedsParent",
          detail: `budget dimension "${dim}": child exceeds parent`,
        });
    }
  }

  // check_root.
  const rootNode = index.get(doc.root.sid);
  if (rootNode === undefined) {
    errs.push({
      kind: "UnresolvedRef",
      detail: `root ${doc.root.sid} does not resolve`,
    });
  } else if (rootNode.kind !== "AgentProcess") {
    errs.push({
      kind: "UnresolvedRef",
      detail: `root must be AgentProcess, got ${rootNode.kind}`,
    });
  }

  return errs.length === 0 ? null : errs;
}

// ─────────────────────────────────────────────────────────────────────────────
// ops.rs — `seal`: endorse → pin → identify → re-validate
// ─────────────────────────────────────────────────────────────────────────────

function endorseMember(
  prov: Prov,
  subjectHash: string,
  what: string,
  sealedAt: number
): HirErr | null {
  if (prov.scope !== "definition" || prov.authority >= AUTHORITY["definition"])
    return null;
  if (prov.taint.length > 0)
    return {
      kind: "TaintedAboveExternal",
      detail: `${what}: ${AUTHORITY_NAME[prov.authority]}`,
    };
  if (prov.origin.kind === "import" || prov.origin.kind === "migration") {
    // seal basis refuses imported/migrated content — a prior pin
    // endorsement covers it instead.
    const pinned =
      prov.attestation !== null &&
      selfConsistent(prov.attestation) &&
      prov.attestation.kind === "pin";
    if (pinned) {
      prov.authority = AUTHORITY["definition"];
      return null;
    }
    return {
      kind: "IllegitimateEndorsement",
      detail: `${what}: import/migration member requires a pin endorsement before seal`,
    };
  }
  prov.authority = AUTHORITY["definition"];
  prov.attestation = {
    kind: "seal",
    subject_hash: subjectHash,
    anchor: { chain: "hir:seal" },
    verified_by: "hir:seal",
    verified_at: sealedAt,
  };
  return null;
}

function endorseText(t: TextLeaf, what: string, sealedAt: number): HirErr | null {
  const e = endorseMember(t.provenance, t.content_hash, what, sealedAt);
  if (e !== null) return e;
  if (
    t.provenance.authority === AUTHORITY["definition"] &&
    t.authority < AUTHORITY["definition"]
  )
    t.authority = AUTHORITY["definition"];
  return null;
}
function endorseLeaves(rec: Sem, sealedAt: number): HirErr | null {
  if (rec.k === "ToolCapability")
    return endorseText(rec.t.purpose, "tool.purpose", sealedAt);
  return null;
}
function endorseSurface(s: Surface | undefined, sealedAt: number): HirErr | null {
  if (s !== undefined && s.t === "Tool")
    return endorseText(s.s.description_template, "surface.description_template", sealedAt);
  return null;
}
function endorseNode(n: Node, sealedAt: number): HirErr | null {
  const basis = versionId(n);
  const e = endorseMember(n.provenance, basis, "node", sealedAt);
  if (e !== null) return e;
  const el = endorseLeaves(n.semantic, sealedAt);
  if (el !== null) return el;
  return endorseSurface(n.surface, sealedAt);
}

/** `each_ref_mut` over the ported kinds — every ref the corpus can carry. */
function eachRefMut(rec: Sem, f: (r: Ref) => void): void {
  switch (rec.k) {
    case "Permission":
      f(rec.holder);
      break;
    case "Budget":
      if (rec.parent !== undefined) f(rec.parent);
      f(rec.accounting);
      break;
    case "AgentProcess":
      if (rec.native !== undefined) {
        f(rec.native.harness_def);
        f(rec.native.budget);
        f(rec.native.permissions);
      } else if (rec.hosted !== undefined) {
        for (const r of rec.hosted.supplies.context) f(r);
        for (const r of rec.hosted.supplies.tools) f(r);
        for (const r of rec.hosted.supplies.procedures) f(r);
        f(rec.hosted.budget);
        f(rec.hosted.permissions);
      }
      break;
    case "HarnessRule":
      if (rec.action.v === "insert_context_item" || rec.action.v === "require_validator")
        f(rec.action.r as Ref);
      else if (rec.action.v === "restrict_tool_set")
        for (const r of rec.action.rs as Ref[]) f(r);
      break;
    case "ToolCapability": {
      for (const r of rec.t.postconditions) f(r);
      if (rec.t.effects !== "pure") {
        for (const e of rec.t.effects.declared) {
          if (e.attributes !== null && "pin" in e.attributes.reversibility)
            f(e.attributes.reversibility.pin);
        }
      }
      break;
    }
    default:
      break;
  }
}

function checkProfilePins(rec: Sem, errs: HirErr[]): void {
  const check = (p: ProfileRef, what: string) => {
    const unboundRootProfile = what === "native.profile" && !p.pinned && p.profile === "unbound";
    if (!p.pinned && !unboundRootProfile)
      errs.push({
        kind: "UnresolvedRef",
        detail: `${what}: unpinned ProfileRef in a sealed form`,
      });
  };
  if (rec.k === "AgentProcess" && rec.native !== undefined)
    check(rec.native.profile, "native.profile");
  if (rec.k === "HarnessRule" && rec.conditioned_on !== undefined)
    check(rec.conditioned_on, "rule.conditioned_on");
}

function unresolvedInAssembly(j: Json, path: string): string | null {
  if (isObj(j)) {
    if (has(j, "version_selector")) return `${path}: version_selector`;
    for (const [k, v] of Object.entries(j)) {
      const p = unresolvedInAssembly(v, `${path}/${k}`);
      if (p !== null) return p;
    }
    return null;
  }
  if (Array.isArray(j)) {
    for (const [i, v] of j.entries()) {
      const p = unresolvedInAssembly(v, `${path}/${i}`);
      if (p !== null) return p;
    }
    return null;
  }
  if (typeof j === "string" && (j.startsWith("$param:") || j.startsWith("$entity:")))
    return `${path}: ${j}`;
  return null;
}
function unpinnedExtensionInAssembly(j: Json, path: string): string | null {
  if (!isObj(j)) return null;
  const refs = get(get(j, "extensions"), "refs");
  if (!Array.isArray(refs)) return null;
  for (const [i, r] of refs.entries()) {
    if (!isObj(r)) continue;
    const rp = `${path}/extensions/refs/${i}`;
    const name = optStr(r, "name") ?? "<unnamed>";
    const loc = get(r, "locator");
    if (isObj(loc)) {
      if (has(loc, "selector"))
        return `${rp} (${name}): locator.selector survived into a sealed form`;
      if (!has(loc, "resolved") || !has(loc, "fetched_at"))
        return `${rp} (${name}): locator lacks its resolved/fetched_at pin`;
    }
    if (!has(r, "content")) return `${rp} (${name}): missing the content pin`;
  }
  return null;
}

type Sealed = { doc: Doc; definition_version_id: string };

function seal(doc: Doc, sealedAt: number): { errs: HirErr[] } | { ok: Sealed } {
  let errs: HirErr[] = [];
  if (doc.assembly !== undefined) {
    const u = unresolvedInAssembly(doc.assembly, "assembly");
    if (u !== null)
      return {
        errs: [
          {
            kind: "UnresolvedRef",
            detail: `a document carrying an unresolved assembly section is not sealable (resolve first): ${u}`,
          },
        ],
      };
    const up = unpinnedExtensionInAssembly(doc.assembly, "assembly");
    if (up !== null) return { errs: [{ kind: "UnpinnedInSealedForm", detail: up }] };
  }
  const pre = validateDoc(doc);
  if (pre !== null) return { errs: pre };
  for (const n of doc.nodes) {
    if (n.semantic.k === "ToolCapability") {
      if (capabilityNeedsFlowContract(n.semantic.t) &&
        (n.semantic.t as { flow_contract?: Json }).flow_contract === undefined)
        errs.push({
          kind: "ContributionUndeclared",
          detail: nodeSemanticIdOf(n),
        });
    }
  }
  if (errs.length > 0) return { errs };

  const sealed = structuredClone(doc) as Doc;
  const sids = new Map<string, number>();
  sealed.nodes.forEach((n, i) => {
    const sid = nodeSemanticIdOf(n);
    n.version.semantic_id = sid;
    sids.set(sid, i);
  });
  errs = [];
  for (const n of sealed.nodes) {
    const e = endorseNode(n, sealedAt);
    if (e !== null) errs.push(e);
    n.version.sealed = true;
  }
  for (const e of sealed.edges) {
    // edges are flagged unported — unreachable in an answered case.
    void e;
  }
  if (errs.length > 0) return { errs };

  const pinnedVid: string[] = sealed.nodes.map(() => "");
  sealed.nodes.forEach((n, i) => {
    n.version.version_id = versionId(n);
    pinnedVid[i] = n.version.version_id;
  });
  for (const n of sealed.nodes) {
    let pinErr: HirErr | null = null;
    eachRefMut(n.semantic, (r) => {
      if (pinErr !== null) return;
      const t = sids.get(r.sid);
      if (t === undefined) {
        pinErr = {
          kind: "UnresolvedRef",
          detail: `seal: ref ${r.sid} unresolved`,
        };
      } else {
        r.pin = pinnedVid[t];
        r.sel = null;
      }
    });
    if (pinErr !== null) errs.push(pinErr);
  }
  for (const n of sealed.nodes) checkProfilePins(n.semantic, errs);
  const rt = sids.get(sealed.root.sid);
  if (rt === undefined) {
    errs.push({ kind: "UnresolvedRef", detail: "seal: root unresolved" });
  } else {
    sealed.root.pin = pinnedVid[rt];
    sealed.root.sel = null;
  }
  for (const n of sealed.nodes) {
    if (n.semantic.k === "AgentProcess" && n.semantic.hosted !== undefined) {
      n.semantic.hosted.version_identity = opaqueVersionIdentity(
        n.semantic.hosted.declared_capabilities,
        n.semantic.hosted.participant_version
      );
    }
  }
  if (errs.length > 0) return { errs };

  const post = validateDoc(sealed);
  if (post !== null) return { errs: post };

  const rootNode = sealed.nodes.find((n) => nodeSemanticIdOf(n) === sealed.root.sid);
  return {
    ok: {
      doc: sealed,
      definition_version_id: rootNode === undefined ? "" : (rootNode.version.version_id as string),
    },
  };
}

// ─────────────────────────────────────────────────────────────────────────────
// The `hir-documents` arm — `hir_replay.rs::replay` per
// `<name>.input.json`, `sealed_at` from `scenario.json`.
// ─────────────────────────────────────────────────────────────────────────────

function kindJoin(errs: HirErr[]): string {
  const seen: string[] = [];
  for (const e of errs) if (!seen.includes(e.kind)) seen.push(e.kind);
  return seen.join(",");
}

function replay(bytes: Buffer, sealedAt: number): { [k: string]: Json } {
  let parseV: Json = "ok";
  let validateV: Json = "skipped";
  let sealV: Json = "skipped";
  let canonicalSha: Json = null;
  let sealedSha: Json = null;
  let defVersion: Json = null;

  let doc: Doc;
  try {
    const text = decodeUtf8(bytes);
    const value = parseJson(text);
    if (!canonical(value).equals(bytes))
      throw new HirFail({
        kind: "NonCanonicalInput",
        detail: "document: input is not in canonical form (whitespace, member order or duplicate keys)",
      });
    doc = documentFromJson(value);
  } catch (e) {
    if (e instanceof HirFail) {
      parseV = `refuse:${kindJoin([e.hir])}`;
    } else {
      parseV = "refuse:NonCanonicalInput";
    }
    return {
      canonical_sha256: canonicalSha,
      definition_version_id: defVersion,
      parse: parseV,
      seal: sealV,
      sealed_sha256: sealedSha,
      validate: validateV,
    };
  }

  // Honest bound: corpus-unreached machinery yields no answer rather than
  // a fabricated verdict (the cell reports `missing answer`).
  if (doc.unported.length > 0) throw new HirUnsupported(doc.unported[0]);

  canonicalSha = sha256Hex(canonical(docToJson(doc)));
  const v = validateDoc(doc);
  if (v !== null) {
    validateV = `invalid:${kindJoin(v)}`;
  } else {
    validateV = "valid";
    const s = seal(doc, sealedAt);
    if ("errs" in s) {
      sealV = `refuse:${kindJoin(s.errs)}`;
    } else {
      sealV = "sealed";
      sealedSha = sha256Hex(canonical(docToJson(s.ok.doc)));
      defVersion = s.ok.definition_version_id;
    }
  }
  return {
    canonical_sha256: canonicalSha,
    definition_version_id: defVersion,
    parse: parseV,
    seal: sealV,
    sealed_sha256: sealedSha,
    validate: validateV,
  };
}

export function armHirDocuments(armDir: string): { [key: string]: Json } {
  const answers: { [key: string]: Json } = Object.create(null);
  const scenario = parseJson(readFileSync(join(armDir, "scenario.json")));
  const sealedAtJ = get(scenario, "sealed_at");
  if (typeof sealedAtJ !== "number" || !Number.isInteger(sealedAtJ))
    throw new Error("hir-documents scenario.json: sealed_at must be an integer");
  const cases = readdirSync(armDir)
    .filter((f) => f.endsWith(".input.json"))
    .sort();
  for (const f of cases) {
    const caseName = f.slice(0, -".input.json".length);
    const bytes = readFileSync(join(armDir, f));
    try {
      answers[caseName] = replay(bytes, sealedAtJ);
    } catch (e) {
      // The honest bound: a case needing machinery outside the ported
      // surface answers nothing — `verify` reports `missing answer`.
      if (e instanceof HirUnsupported) continue;
      throw e;
    }
  }
  return answers;
}
