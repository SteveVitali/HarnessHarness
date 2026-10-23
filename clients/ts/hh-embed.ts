// generated — do not edit.
//
// Source of truth: the `hharness-embed-schema` crate (the single schema source, CC7). Regenerate
// with `cargo run -p hharness-codegen -- --root <repo>`; `scripts/check-drift.sh` fails the build
// if this file drifts from the schema. Runs under node >= 22 (type stripping — erasable
// syntax only), the `node:http` stdlib transport only, no npm dependencies. A generated
// client contains no runtime logic beyond the transport binding (ADR-0179): it asserts `(contract_major,
// schema_hash)` at `hello` and lowers a mismatch to a typed error — never a silent
// fallback (ADR-0178 D2).

import { request } from "node:http";

/// The canonical-JSON value lattice (integers + decimal-free — the
/// dialect admits no doubles; `number` decodes only via `needInt`).
export type Json = null | boolean | number | string | Json[] | JsonObj;
export interface JsonObj {
  [key: string]: Json;
}

/// A typed client error. Transport failures and RPC errors are distinct;
/// the `rpc` arm carries the decoded `EmbedError` sum (never free text —
/// ADR-0176 D5). `identity_mismatch` means the kernel's returned
/// `(contract_major, schema_hash)` does not match the constants this
/// client was generated against — never a silent fallback (ADR-0178 D2).
export type ClientErrorTag = "transport" | "rpc" | "decode" | "identity_mismatch";
export class ClientError extends Error {
  tag: ClientErrorTag;
  rpc?: EmbedError;
  field?: string;
  constructor(tag: ClientErrorTag, message: string, rpc?: EmbedError, field?: string) {
    super(message);
    this.tag = tag;
    this.rpc = rpc;
    this.field = field;
  }
  static transport(message: string): ClientError {
    return new ClientError("transport", message);
  }
  static rpcError(e: EmbedError): ClientError {
    return new ClientError("rpc", `${e.kind}: ${e.message}`, e);
  }
}

function fail(message: string): never {
  throw new ClientError("decode", message);
}

function isObj(v: Json | undefined): v is JsonObj {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}
function needObj(v: Json): JsonObj {
  if (isObj(v)) return v;
  return fail("expected object");
}
function needArr(v: Json): Json[] {
  if (Array.isArray(v)) return v;
  return fail("expected array");
}
function needStr(v: Json): string {
  if (typeof v === "string") return v;
  return fail("expected string");
}
function needInt(v: Json): number {
  if (typeof v === "number" && Number.isInteger(v)) return v;
  return fail("expected integer");
}
function needBool(v: Json): boolean {
  if (typeof v === "boolean") return v;
  return fail("expected bool");
}
function need(o: JsonObj, k: string): Json {
  const v = o[k];
  if (v === undefined) return fail(`missing '${k}'`);
  return v;
}

/// Serialize to canonical (sorted-key, compact) JSON — the same wire
/// spelling `hharness_wire::json::Json::to_canonical_string` emits: keys sorted
/// (UTF-16 code-unit order; identical to the codec's UTF-8 byte order for
/// the contract's ASCII key space), minimal escapes, integers only.
export function canon(v: Json): string {
  if (v === null) return "null";
  if (v === true) return "true";
  if (v === false) return "false";
  if (typeof v === "number") return String(v);
  if (typeof v === "string") return qstr(v);
  if (Array.isArray(v)) return "[" + v.map(canon).join(",") + "]";
  return (
    "{" +
    Object.keys(v)
      .sort()
      .map((k) => qstr(k) + ":" + canon(v[k]))
      .join(",") +
    "}"
  );
}

function qstr(s: string): string {
  let out = '"';
  for (const c of s) {
    const cp = c.codePointAt(0) as number;
    if (c === '"') out += '\\"';
    else if (c === "\\") out += "\\\\";
    else if (c === "\n") out += "\\n";
    else if (c === "\r") out += "\\r";
    else if (c === "\t") out += "\\t";
    else if (cp < 0x20) out += "\\u" + cp.toString(16).padStart(4, "0");
    else out += c;
  }
  return out + '"';
}

/// The contract major this client was generated against.
export const CONTRACT_MAJOR = 1;

/// The schema content address this client was generated against.
export const EXPECTED_SCHEMA_HASH = "sha256:26a5d76ad5ad22af62024a9d9ab6cfe13d24129229221b9023c4eb4abaa7393c";

export interface Accepted {
  turn_id: string,
}

export function acceptedToJson(v: Accepted): Json {
  const o: JsonObj = {};
  o["turn_id"] = v.turn_id;
  return o;
}

export function acceptedFromJson(v: Json): Accepted {
  const o = needObj(v);
  return {
    turn_id: needStr(need(o, "turn_id")),
  };
}

export interface AccountParams {
  session_id: string,
  until_seq?: number,
}

export function accountParamsToJson(v: AccountParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  if (v.until_seq !== undefined) o["until_seq"] = v.until_seq;
  return o;
}

export function accountParamsFromJson(v: Json): AccountParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    until_seq: o["until_seq"] === undefined ? undefined : needInt(o["until_seq"]),
  };
}

export interface AccountView {
  account: Json,
}

export function accountViewToJson(v: AccountView): Json {
  const o: JsonObj = {};
  o["account"] = v.account;
  return o;
}

export function accountViewFromJson(v: Json): AccountView {
  const o = needObj(v);
  return {
    account: need(o, "account"),
  };
}

export interface Acknowledged {
  acknowledged: boolean,
}

export function acknowledgedToJson(v: Acknowledged): Json {
  const o: JsonObj = {};
  o["acknowledged"] = v.acknowledged;
  return o;
}

export function acknowledgedFromJson(v: Json): Acknowledged {
  const o = needObj(v);
  return {
    acknowledged: needBool(need(o, "acknowledged")),
  };
}

export interface AmendParams {
  session_id: string,
  target: string,
  value: Json,
  attestation?: Json,
  idempotency_key?: string,
  invocation?: InvocationRecord,
}

export function amendParamsToJson(v: AmendParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["target"] = v.target;
  o["value"] = v.value;
  if (v.attestation !== undefined) o["attestation"] = v.attestation;
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  if (v.invocation !== undefined) o["invocation"] = invocationRecordToJson(v.invocation);
  return o;
}

export function amendParamsFromJson(v: Json): AmendParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    target: needStr(need(o, "target")),
    value: need(o, "value"),
    attestation: o["attestation"] === undefined ? undefined : o["attestation"],
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
    invocation: o["invocation"] === undefined ? undefined : invocationRecordFromJson(o["invocation"]),
  };
}

export interface ArchiveParams {
  session_id: string,
}

export function archiveParamsToJson(v: ArchiveParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  return o;
}

export function archiveParamsFromJson(v: Json): ArchiveParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
  };
}

export interface AssumptionDebtRecord {
  assumption_id: string,
  debt_id: string,
  owner_layer: string,
  stage_gate: string,
  status: string,
  detail: string,
}

export function assumptionDebtRecordToJson(v: AssumptionDebtRecord): Json {
  const o: JsonObj = {};
  o["assumption_id"] = v.assumption_id;
  o["debt_id"] = v.debt_id;
  o["owner_layer"] = v.owner_layer;
  o["stage_gate"] = v.stage_gate;
  o["status"] = v.status;
  o["detail"] = v.detail;
  return o;
}

export function assumptionDebtRecordFromJson(v: Json): AssumptionDebtRecord {
  const o = needObj(v);
  return {
    assumption_id: needStr(need(o, "assumption_id")),
    debt_id: needStr(need(o, "debt_id")),
    owner_layer: needStr(need(o, "owner_layer")),
    stage_gate: needStr(need(o, "stage_gate")),
    status: needStr(need(o, "status")),
    detail: needStr(need(o, "detail")),
  };
}

export interface AttendanceDeclaration {
  value: AttendanceValue,
  source: AttendanceSource,
}

export function attendanceDeclarationToJson(v: AttendanceDeclaration): Json {
  const o: JsonObj = {};
  o["value"] = attendanceValueToJson(v.value);
  o["source"] = attendanceSourceToJson(v.source);
  return o;
}

export function attendanceDeclarationFromJson(v: Json): AttendanceDeclaration {
  const o = needObj(v);
  return {
    value: attendanceValueFromJson(need(o, "value")),
    source: attendanceSourceFromJson(need(o, "source")),
  };
}

export type AttendanceSource = "declared" | "tty_inferred" | "forced";
export function attendanceSourceToJson(v: AttendanceSource): Json { return v; }
export function attendanceSourceFromJson(v: Json): AttendanceSource {
  switch (v) {
    case "declared": return v;
    case "tty_inferred": return v;
    case "forced": return v;
    default: return fail(`unknown AttendanceSource variant ${canon(v)}`);
  }
}

export type AttendanceValue = "interactive" | "async" | "unattended";
export function attendanceValueToJson(v: AttendanceValue): Json { return v; }
export function attendanceValueFromJson(v: Json): AttendanceValue {
  switch (v) {
    case "interactive": return v;
    case "async": return v;
    case "unattended": return v;
    default: return fail(`unknown AttendanceValue variant ${canon(v)}`);
  }
}

export interface AuditViewParams {
  session_id: string,
  until_seq?: number,
}

export function auditViewParamsToJson(v: AuditViewParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  if (v.until_seq !== undefined) o["until_seq"] = v.until_seq;
  return o;
}

export function auditViewParamsFromJson(v: Json): AuditViewParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    until_seq: o["until_seq"] === undefined ? undefined : needInt(o["until_seq"]),
  };
}

export interface BranchOpenParams {
  session_id: string,
  kind?: string,
  read_only?: boolean,
  policy?: Json,
  budget_slice_id?: string,
  env_binding?: string,
  evidence_path?: string,
  fork_seq?: number,
  permissions?: string[],
}

export function branchOpenParamsToJson(v: BranchOpenParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  if (v.kind !== undefined) o["kind"] = v.kind;
  o["read_only"] = v.read_only === true;
  if (v.policy !== undefined) o["policy"] = v.policy;
  if (v.budget_slice_id !== undefined) o["budget_slice_id"] = v.budget_slice_id;
  if (v.env_binding !== undefined) o["env_binding"] = v.env_binding;
  if (v.evidence_path !== undefined) o["evidence_path"] = v.evidence_path;
  if (v.fork_seq !== undefined) o["fork_seq"] = v.fork_seq;
  o["permissions"] = (v.permissions ?? []).map((x) => x);
  return o;
}

export function branchOpenParamsFromJson(v: Json): BranchOpenParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    kind: o["kind"] === undefined ? undefined : needStr(o["kind"]),
    read_only: o["read_only"] === true,
    policy: o["policy"] === undefined ? undefined : o["policy"],
    budget_slice_id: o["budget_slice_id"] === undefined ? undefined : needStr(o["budget_slice_id"]),
    env_binding: o["env_binding"] === undefined ? undefined : needStr(o["env_binding"]),
    evidence_path: o["evidence_path"] === undefined ? undefined : needStr(o["evidence_path"]),
    fork_seq: o["fork_seq"] === undefined ? undefined : needInt(o["fork_seq"]),
    permissions: o["permissions"] === undefined ? [] : needArr(o["permissions"]).map((x) => needStr(x)),
  };
}

export type BudgetInput =
  | { kind: "ref"; ref: string }
  | { kind: "node"; node: Json }

export function budgetInputToJson(v: BudgetInput): Json {
  const o: JsonObj = {};
  switch (v["kind"]) {
    case "ref": {
      o["kind"] = "ref";
      o["ref"] = v.ref;
      return o;
    }
    case "node": {
      o["kind"] = "node";
      o["node"] = v.node;
      return o;
    }
  }
}

export function budgetInputFromJson(v: Json): BudgetInput {
  const o = needObj(v);
  const tag = needStr(need(o, "kind"));
  switch (tag) {
    case "ref": return { kind: "ref", ref: needStr(need(o, "ref")) };
    case "node": return { kind: "node", node: need(o, "node") };
    default: return fail(`unknown BudgetInput variant ${canon(tag)}`);
  }
}

export interface CancelParams {
  session_id: string,
  scope: CancelScope,
  idempotency_key?: string,
}

export function cancelParamsToJson(v: CancelParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["scope"] = cancelScopeToJson(v.scope);
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function cancelParamsFromJson(v: Json): CancelParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    scope: cancelScopeFromJson(need(o, "scope")),
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export type CancelScope =
  | { kind: "turn"; turn_id: string }
  | { kind: "run" }

export function cancelScopeToJson(v: CancelScope): Json {
  const o: JsonObj = {};
  switch (v["kind"]) {
    case "turn": {
      o["kind"] = "turn";
      o["turn_id"] = v.turn_id;
      return o;
    }
    case "run": {
      o["kind"] = "run";
      return o;
    }
  }
}

export function cancelScopeFromJson(v: Json): CancelScope {
  const o = needObj(v);
  const tag = needStr(need(o, "kind"));
  switch (tag) {
    case "turn": return { kind: "turn", turn_id: needStr(need(o, "turn_id")) };
    case "run": return { kind: "run" };
    default: return fail(`unknown CancelScope variant ${canon(tag)}`);
  }
}

export interface ClassFilter {
  classes?: string[],
}

export function classFilterToJson(v: ClassFilter): Json {
  const o: JsonObj = {};
  o["classes"] = (v.classes ?? []).map((x) => x);
  return o;
}

export function classFilterFromJson(v: Json): ClassFilter {
  const o = needObj(v);
  return {
    classes: o["classes"] === undefined ? [] : needArr(o["classes"]).map((x) => needStr(x)),
  };
}

export interface ClientDecl {
  kind: string,
  sink?: Json,
  surface_ref?: string,
  ui_caps?: string[],
}

export function clientDeclToJson(v: ClientDecl): Json {
  const o: JsonObj = {};
  o["kind"] = v.kind;
  if (v.sink !== undefined) o["sink"] = v.sink;
  if (v.surface_ref !== undefined) o["surface_ref"] = v.surface_ref;
  o["ui_caps"] = (v.ui_caps ?? []).map((x) => x);
  return o;
}

export function clientDeclFromJson(v: Json): ClientDecl {
  const o = needObj(v);
  return {
    kind: needStr(need(o, "kind")),
    sink: o["sink"] === undefined ? undefined : o["sink"],
    surface_ref: o["surface_ref"] === undefined ? undefined : needStr(o["surface_ref"]),
    ui_caps: o["ui_caps"] === undefined ? [] : needArr(o["ui_caps"]).map((x) => needStr(x)),
  };
}

export interface ClientDescriptor {
  name: string,
  version: string,
  kind: ClientKind,
}

export function clientDescriptorToJson(v: ClientDescriptor): Json {
  const o: JsonObj = {};
  o["name"] = v.name;
  o["version"] = v.version;
  o["kind"] = clientKindToJson(v.kind);
  return o;
}

export function clientDescriptorFromJson(v: Json): ClientDescriptor {
  const o = needObj(v);
  return {
    name: needStr(need(o, "name")),
    version: needStr(need(o, "version")),
    kind: clientKindFromJson(need(o, "kind")),
  };
}

export type ClientKind = "cli" | "web" | "ide" | "sdk" | "lab" | "mcp_server" | "acp_bridge" | "test";
export function clientKindToJson(v: ClientKind): Json { return v; }
export function clientKindFromJson(v: Json): ClientKind {
  switch (v) {
    case "cli": return v;
    case "web": return v;
    case "ide": return v;
    case "sdk": return v;
    case "lab": return v;
    case "mcp_server": return v;
    case "acp_bridge": return v;
    case "test": return v;
    default: return fail(`unknown ClientKind variant ${canon(v)}`);
  }
}

export interface CloseParams {
  session_id: string,
  reason: CloseReason,
}

export function closeParamsToJson(v: CloseParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["reason"] = closeReasonToJson(v.reason);
  return o;
}

export function closeParamsFromJson(v: Json): CloseParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    reason: closeReasonFromJson(need(o, "reason")),
  };
}

export type CloseReason = "done" | "abandon" | "host_shutdown";
export function closeReasonToJson(v: CloseReason): Json { return v; }
export function closeReasonFromJson(v: Json): CloseReason {
  switch (v) {
    case "done": return v;
    case "abandon": return v;
    case "host_shutdown": return v;
    default: return fail(`unknown CloseReason variant ${canon(v)}`);
  }
}

export interface Closed {
  final: RunSummaryRef,
}

export function closedToJson(v: Closed): Json {
  const o: JsonObj = {};
  o["final"] = runSummaryRefToJson(v.final);
  return o;
}

export function closedFromJson(v: Json): Closed {
  const o = needObj(v);
  return {
    final: runSummaryRefFromJson(need(o, "final")),
  };
}

export type ClosedReason = "complete" | "cancelled" | "slow_consumer" | "run_ended" | "session_detached" | "kernel_shutdown";
export function closedReasonToJson(v: ClosedReason): Json { return v; }
export function closedReasonFromJson(v: Json): ClosedReason {
  switch (v) {
    case "complete": return v;
    case "cancelled": return v;
    case "slow_consumer": return v;
    case "run_ended": return v;
    case "session_detached": return v;
    case "kernel_shutdown": return v;
    default: return fail(`unknown ClosedReason variant ${canon(v)}`);
  }
}

export interface CoherentForkPointsParams {
  session_id: string,
  from_seq?: number,
  to_seq?: number,
}

export function coherentForkPointsParamsToJson(v: CoherentForkPointsParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  if (v.from_seq !== undefined) o["from_seq"] = v.from_seq;
  if (v.to_seq !== undefined) o["to_seq"] = v.to_seq;
  return o;
}

export function coherentForkPointsParamsFromJson(v: Json): CoherentForkPointsParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    from_seq: o["from_seq"] === undefined ? undefined : needInt(o["from_seq"]),
    to_seq: o["to_seq"] === undefined ? undefined : needInt(o["to_seq"]),
  };
}

export interface CompensateParams {
  session_id: string,
  after_seq?: number,
  outcomes?: Json,
}

export function compensateParamsToJson(v: CompensateParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  if (v.after_seq !== undefined) o["after_seq"] = v.after_seq;
  if (v.outcomes !== undefined) o["outcomes"] = v.outcomes;
  return o;
}

export function compensateParamsFromJson(v: Json): CompensateParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    after_seq: o["after_seq"] === undefined ? undefined : needInt(o["after_seq"]),
    outcomes: o["outcomes"] === undefined ? undefined : o["outcomes"],
  };
}

export interface ContinueCarried {
  resume_set_heads?: string[],
  budget_id?: string,
}

export function continueCarriedToJson(v: ContinueCarried): Json {
  const o: JsonObj = {};
  o["resume_set_heads"] = (v.resume_set_heads ?? []).map((x) => x);
  if (v.budget_id !== undefined) o["budget_id"] = v.budget_id;
  return o;
}

export function continueCarriedFromJson(v: Json): ContinueCarried {
  const o = needObj(v);
  return {
    resume_set_heads: o["resume_set_heads"] === undefined ? [] : needArr(o["resume_set_heads"]).map((x) => needStr(x)),
    budget_id: o["budget_id"] === undefined ? undefined : needStr(o["budget_id"]),
  };
}

export interface ContinueGoalParams {
  session_id: string,
  goal_ref: string,
  carried?: ContinueCarried,
  manifest_delta?: Json,
}

export function continueGoalParamsToJson(v: ContinueGoalParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["goal_ref"] = v.goal_ref;
  if (v.carried !== undefined) o["carried"] = continueCarriedToJson(v.carried);
  if (v.manifest_delta !== undefined) o["manifest_delta"] = v.manifest_delta;
  return o;
}

export function continueGoalParamsFromJson(v: Json): ContinueGoalParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    goal_ref: needStr(need(o, "goal_ref")),
    carried: o["carried"] === undefined ? undefined : continueCarriedFromJson(o["carried"]),
    manifest_delta: o["manifest_delta"] === undefined ? undefined : o["manifest_delta"],
  };
}

export interface ContractIdentity {
  contract_major: number,
  schema_hash: string,
  kernel_version_id: string,
}

export function contractIdentityToJson(v: ContractIdentity): Json {
  const o: JsonObj = {};
  o["contract_major"] = v.contract_major;
  o["schema_hash"] = v.schema_hash;
  o["kernel_version_id"] = v.kernel_version_id;
  return o;
}

export function contractIdentityFromJson(v: Json): ContractIdentity {
  const o = needObj(v);
  return {
    contract_major: needInt(need(o, "contract_major")),
    schema_hash: needStr(need(o, "schema_hash")),
    kernel_version_id: needStr(need(o, "kernel_version_id")),
  };
}

export interface CounterfactualParams {
  session_id: string,
  run_id?: string,
  at_seq: number,
  intervention: Json,
  design: Json,
  env?: string,
  idempotency_key?: string,
}

export function counterfactualParamsToJson(v: CounterfactualParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  if (v.run_id !== undefined) o["run_id"] = v.run_id;
  o["at_seq"] = v.at_seq;
  o["intervention"] = v.intervention;
  o["design"] = v.design;
  if (v.env !== undefined) o["env"] = v.env;
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function counterfactualParamsFromJson(v: Json): CounterfactualParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    run_id: o["run_id"] === undefined ? undefined : needStr(o["run_id"]),
    at_seq: needInt(need(o, "at_seq")),
    intervention: need(o, "intervention"),
    design: need(o, "design"),
    env: o["env"] === undefined ? undefined : needStr(o["env"]),
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export interface CounterfactualResult {
  source_run_id: string,
  at_seq: number,
  intervention_ref: string,
  factual: Json[],
  counterfactual: Json[],
  comparison_ref: string,
}

export function counterfactualResultToJson(v: CounterfactualResult): Json {
  const o: JsonObj = {};
  o["source_run_id"] = v.source_run_id;
  o["at_seq"] = v.at_seq;
  o["intervention_ref"] = v.intervention_ref;
  o["factual"] = v.factual.map((x) => x);
  o["counterfactual"] = v.counterfactual.map((x) => x);
  o["comparison_ref"] = v.comparison_ref;
  return o;
}

export function counterfactualResultFromJson(v: Json): CounterfactualResult {
  const o = needObj(v);
  return {
    source_run_id: needStr(need(o, "source_run_id")),
    at_seq: needInt(need(o, "at_seq")),
    intervention_ref: needStr(need(o, "intervention_ref")),
    factual: needArr(need(o, "factual")).map((x) => x),
    counterfactual: needArr(need(o, "counterfactual")).map((x) => x),
    comparison_ref: needStr(need(o, "comparison_ref")),
  };
}

export type DefinitionInput =
  | { kind: "document"; document: Json }
  | { kind: "ref"; ref: string }

export function definitionInputToJson(v: DefinitionInput): Json {
  const o: JsonObj = {};
  switch (v["kind"]) {
    case "document": {
      o["kind"] = "document";
      o["document"] = v.document;
      return o;
    }
    case "ref": {
      o["kind"] = "ref";
      o["ref"] = v.ref;
      return o;
    }
  }
}

export function definitionInputFromJson(v: Json): DefinitionInput {
  const o = needObj(v);
  const tag = needStr(need(o, "kind"));
  switch (tag) {
    case "document": return { kind: "document", document: need(o, "document") };
    case "ref": return { kind: "ref", ref: needStr(need(o, "ref")) };
    default: return fail(`unknown DefinitionInput variant ${canon(tag)}`);
  }
}

export interface DerivedFrom {
  seq: number,
  hash: string,
}

export function derivedFromToJson(v: DerivedFrom): Json {
  const o: JsonObj = {};
  o["seq"] = v.seq;
  o["hash"] = v.hash;
  return o;
}

export function derivedFromFromJson(v: Json): DerivedFrom {
  const o = needObj(v);
  return {
    seq: needInt(need(o, "seq")),
    hash: needStr(need(o, "hash")),
  };
}

export interface DescribeParams {
  session_id: string,
}

export function describeParamsToJson(v: DescribeParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  return o;
}

export function describeParamsFromJson(v: Json): DescribeParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
  };
}

export interface DescribeResult {
  manifest_ref: string,
  participant_descriptor: Json,
  protocol_bindings?: Json[],
  realized: RealizedSettings,
  environment: EnvDescribe,
}

export function describeResultToJson(v: DescribeResult): Json {
  const o: JsonObj = {};
  o["manifest_ref"] = v.manifest_ref;
  o["participant_descriptor"] = v.participant_descriptor;
  o["protocol_bindings"] = (v.protocol_bindings ?? []).map((x) => x);
  o["realized"] = realizedSettingsToJson(v.realized);
  o["environment"] = envDescribeToJson(v.environment);
  return o;
}

export function describeResultFromJson(v: Json): DescribeResult {
  const o = needObj(v);
  return {
    manifest_ref: needStr(need(o, "manifest_ref")),
    participant_descriptor: need(o, "participant_descriptor"),
    protocol_bindings: o["protocol_bindings"] === undefined ? [] : needArr(o["protocol_bindings"]).map((x) => x),
    realized: realizedSettingsFromJson(need(o, "realized")),
    environment: envDescribeFromJson(need(o, "environment")),
  };
}

export interface DiscardParams {
  session_id: string,
  branch_id: string,
  idempotency_key?: string,
}

export function discardParamsToJson(v: DiscardParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["branch_id"] = v.branch_id;
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function discardParamsFromJson(v: Json): DiscardParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    branch_id: needStr(need(o, "branch_id")),
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export type ElicitOutcome =
  | { kind: "answer"; value: Json }
  | { kind: "cancelled" }

export function elicitOutcomeToJson(v: ElicitOutcome): Json {
  const o: JsonObj = {};
  switch (v["kind"]) {
    case "answer": {
      o["kind"] = "answer";
      o["value"] = v.value;
      return o;
    }
    case "cancelled": {
      o["kind"] = "cancelled";
      return o;
    }
  }
}

export function elicitOutcomeFromJson(v: Json): ElicitOutcome {
  const o = needObj(v);
  const tag = needStr(need(o, "kind"));
  switch (tag) {
    case "answer": return { kind: "answer", value: need(o, "value") };
    case "cancelled": return { kind: "cancelled" };
    default: return fail(`unknown ElicitOutcome variant ${canon(tag)}`);
  }
}

export interface ElicitParams {
  elicitation_id: string,
  prompt: Json,
  options?: Json[],
}

export function elicitParamsToJson(v: ElicitParams): Json {
  const o: JsonObj = {};
  o["elicitation_id"] = v.elicitation_id;
  o["prompt"] = v.prompt;
  o["options"] = (v.options ?? []).map((x) => x);
  return o;
}

export function elicitParamsFromJson(v: Json): ElicitParams {
  const o = needObj(v);
  return {
    elicitation_id: needStr(need(o, "elicitation_id")),
    prompt: need(o, "prompt"),
    options: o["options"] === undefined ? [] : needArr(o["options"]).map((x) => x),
  };
}

export interface ElicitResult {
  outcome: ElicitOutcome,
}

export function elicitResultToJson(v: ElicitResult): Json {
  const o: JsonObj = {};
  o["outcome"] = elicitOutcomeToJson(v.outcome);
  return o;
}

export function elicitResultFromJson(v: Json): ElicitResult {
  const o = needObj(v);
  return {
    outcome: elicitOutcomeFromJson(need(o, "outcome")),
  };
}

export interface EnvDescribe {
  connection_info: Json,
  health: string,
  meters?: Json[],
}

export function envDescribeToJson(v: EnvDescribe): Json {
  const o: JsonObj = {};
  o["connection_info"] = v.connection_info;
  o["health"] = v.health;
  o["meters"] = (v.meters ?? []).map((x) => x);
  return o;
}

export function envDescribeFromJson(v: Json): EnvDescribe {
  const o = needObj(v);
  return {
    connection_info: need(o, "connection_info"),
    health: needStr(need(o, "health")),
    meters: o["meters"] === undefined ? [] : needArr(o["meters"]).map((x) => x),
  };
}

export type EnvironmentInput =
  | { kind: "connection_info"; connection_info: Json }
  | { kind: "ref"; ref: string }

export function environmentInputToJson(v: EnvironmentInput): Json {
  const o: JsonObj = {};
  switch (v["kind"]) {
    case "connection_info": {
      o["kind"] = "connection_info";
      o["connection_info"] = v.connection_info;
      return o;
    }
    case "ref": {
      o["kind"] = "ref";
      o["ref"] = v.ref;
      return o;
    }
  }
}

export function environmentInputFromJson(v: Json): EnvironmentInput {
  const o = needObj(v);
  const tag = needStr(need(o, "kind"));
  switch (tag) {
    case "connection_info": return { kind: "connection_info", connection_info: need(o, "connection_info") };
    case "ref": return { kind: "ref", ref: needStr(need(o, "ref")) };
    default: return fail(`unknown EnvironmentInput variant ${canon(tag)}`);
  }
}

export type EphemeralKind = "delta" | "progress" | "permission_rendering" | "guard_notice" | "log";
export function ephemeralKindToJson(v: EphemeralKind): Json { return v; }
export function ephemeralKindFromJson(v: Json): EphemeralKind {
  switch (v) {
    case "delta": return v;
    case "progress": return v;
    case "permission_rendering": return v;
    case "guard_notice": return v;
    case "log": return v;
    default: return fail(`unknown EphemeralKind variant ${canon(v)}`);
  }
}

export interface ForkParams {
  session_id: string,
  at: ForkPoint,
  kind?: string,
  env?: string,
  replay_mode?: string,
  policy_ref?: string,
  budget_slice_ref?: string,
  coerce_to_boundary?: boolean,
  snapshot_ref?: string,
  manifest_delta?: Json,
  idempotency_key?: string,
  invocation?: InvocationRecord,
}

export function forkParamsToJson(v: ForkParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["at"] = forkPointToJson(v.at);
  if (v.kind !== undefined) o["kind"] = v.kind;
  if (v.env !== undefined) o["env"] = v.env;
  if (v.replay_mode !== undefined) o["replay_mode"] = v.replay_mode;
  if (v.policy_ref !== undefined) o["policy_ref"] = v.policy_ref;
  if (v.budget_slice_ref !== undefined) o["budget_slice_ref"] = v.budget_slice_ref;
  o["coerce_to_boundary"] = v.coerce_to_boundary === true;
  if (v.snapshot_ref !== undefined) o["snapshot_ref"] = v.snapshot_ref;
  if (v.manifest_delta !== undefined) o["manifest_delta"] = v.manifest_delta;
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  if (v.invocation !== undefined) o["invocation"] = invocationRecordToJson(v.invocation);
  return o;
}

export function forkParamsFromJson(v: Json): ForkParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    at: forkPointFromJson(need(o, "at")),
    kind: o["kind"] === undefined ? undefined : needStr(o["kind"]),
    env: o["env"] === undefined ? undefined : needStr(o["env"]),
    replay_mode: o["replay_mode"] === undefined ? undefined : needStr(o["replay_mode"]),
    policy_ref: o["policy_ref"] === undefined ? undefined : needStr(o["policy_ref"]),
    budget_slice_ref: o["budget_slice_ref"] === undefined ? undefined : needStr(o["budget_slice_ref"]),
    coerce_to_boundary: o["coerce_to_boundary"] === true,
    snapshot_ref: o["snapshot_ref"] === undefined ? undefined : needStr(o["snapshot_ref"]),
    manifest_delta: o["manifest_delta"] === undefined ? undefined : o["manifest_delta"],
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
    invocation: o["invocation"] === undefined ? undefined : invocationRecordFromJson(o["invocation"]),
  };
}

export type ForkPoint =
  | { kind: "seq"; seq: number }
  | { kind: "event_ref"; run_id: string; event_id: string }

export function forkPointToJson(v: ForkPoint): Json {
  const o: JsonObj = {};
  switch (v["kind"]) {
    case "seq": {
      o["kind"] = "seq";
      o["seq"] = v.seq;
      return o;
    }
    case "event_ref": {
      o["kind"] = "event_ref";
      o["run_id"] = v.run_id;
      o["event_id"] = v.event_id;
      return o;
    }
  }
}

export function forkPointFromJson(v: Json): ForkPoint {
  const o = needObj(v);
  const tag = needStr(need(o, "kind"));
  switch (tag) {
    case "seq": return { kind: "seq", seq: needInt(need(o, "seq")) };
    case "event_ref": return { kind: "event_ref", run_id: needStr(need(o, "run_id")), event_id: needStr(need(o, "event_id")) };
    default: return fail(`unknown ForkPoint variant ${canon(tag)}`);
  }
}

export interface GetArtifactParams {
  session_id: string,
  address: string,
}

export function getArtifactParamsToJson(v: GetArtifactParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["address"] = v.address;
  return o;
}

export function getArtifactParamsFromJson(v: Json): GetArtifactParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    address: needStr(need(o, "address")),
  };
}

export interface GrantParams {
  session_id: string,
  grant: Json,
  idempotency_key?: string,
}

export function grantParamsToJson(v: GrantParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["grant"] = v.grant;
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function grantParamsFromJson(v: Json): GrantParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    grant: need(o, "grant"),
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export interface Head {
  seq: number,
  event_id: string,
  hash: string,
}

export function headToJson(v: Head): Json {
  const o: JsonObj = {};
  o["seq"] = v.seq;
  o["event_id"] = v.event_id;
  o["hash"] = v.hash;
  return o;
}

export function headFromJson(v: Json): Head {
  const o = needObj(v);
  return {
    seq: needInt(need(o, "seq")),
    event_id: needStr(need(o, "event_id")),
    hash: needStr(need(o, "hash")),
  };
}

export interface HeadParams {
  session_id: string,
}

export function headParamsToJson(v: HeadParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  return o;
}

export function headParamsFromJson(v: Json): HeadParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
  };
}

export interface HealParams {
  session_id: string,
  policy_ref?: string,
  policy?: Json,
}

export function healParamsToJson(v: HealParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  if (v.policy_ref !== undefined) o["policy_ref"] = v.policy_ref;
  if (v.policy !== undefined) o["policy"] = v.policy;
  return o;
}

export function healParamsFromJson(v: Json): HealParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    policy_ref: o["policy_ref"] === undefined ? undefined : needStr(o["policy_ref"]),
    policy: o["policy"] === undefined ? undefined : o["policy"],
  };
}

export interface HelloParams {
  contract_major: number,
  client: ClientDescriptor,
  capabilities: HostCapabilities,
  schema_hash?: string,
  kernel_floor?: string,
}

export function helloParamsToJson(v: HelloParams): Json {
  const o: JsonObj = {};
  o["contract_major"] = v.contract_major;
  o["client"] = clientDescriptorToJson(v.client);
  o["capabilities"] = hostCapabilitiesToJson(v.capabilities);
  if (v.schema_hash !== undefined) o["schema_hash"] = v.schema_hash;
  if (v.kernel_floor !== undefined) o["kernel_floor"] = v.kernel_floor;
  return o;
}

export function helloParamsFromJson(v: Json): HelloParams {
  const o = needObj(v);
  return {
    contract_major: needInt(need(o, "contract_major")),
    client: clientDescriptorFromJson(need(o, "client")),
    capabilities: hostCapabilitiesFromJson(need(o, "capabilities")),
    schema_hash: o["schema_hash"] === undefined ? undefined : needStr(o["schema_hash"]),
    kernel_floor: o["kernel_floor"] === undefined ? undefined : needStr(o["kernel_floor"]),
  };
}

export interface HelloResult {
  kernel: KernelDescriptor,
  negotiated: HostCapabilities,
  stability: Json,
  experimental_enabled?: boolean,
}

export function helloResultToJson(v: HelloResult): Json {
  const o: JsonObj = {};
  o["kernel"] = kernelDescriptorToJson(v.kernel);
  o["negotiated"] = hostCapabilitiesToJson(v.negotiated);
  o["stability"] = v.stability;
  o["experimental_enabled"] = v.experimental_enabled === true;
  return o;
}

export function helloResultFromJson(v: Json): HelloResult {
  const o = needObj(v);
  return {
    kernel: kernelDescriptorFromJson(need(o, "kernel")),
    negotiated: hostCapabilitiesFromJson(need(o, "negotiated")),
    stability: need(o, "stability"),
    experimental_enabled: o["experimental_enabled"] === true,
  };
}

export interface HookNotification {
  hook_id: string,
  event_class: string,
  event: Json,
}

export function hookNotificationToJson(v: HookNotification): Json {
  const o: JsonObj = {};
  o["hook_id"] = v.hook_id;
  o["event_class"] = v.event_class;
  o["event"] = v.event;
  return o;
}

export function hookNotificationFromJson(v: Json): HookNotification {
  const o = needObj(v);
  return {
    hook_id: needStr(need(o, "hook_id")),
    event_class: needStr(need(o, "event_class")),
    event: need(o, "event"),
  };
}

export type HookResult =
  | { kind: "raise"; reason: string }
  | { kind: "deny"; reason: string }
  | { kind: "annotate"; annotations: Json }
  | { kind: "none" }

export function hookResultToJson(v: HookResult): Json {
  const o: JsonObj = {};
  switch (v["kind"]) {
    case "raise": {
      o["kind"] = "raise";
      o["reason"] = v.reason;
      return o;
    }
    case "deny": {
      o["kind"] = "deny";
      o["reason"] = v.reason;
      return o;
    }
    case "annotate": {
      o["kind"] = "annotate";
      o["annotations"] = v.annotations;
      return o;
    }
    case "none": {
      o["kind"] = "none";
      return o;
    }
  }
}

export function hookResultFromJson(v: Json): HookResult {
  const o = needObj(v);
  const tag = needStr(need(o, "kind"));
  switch (tag) {
    case "raise": return { kind: "raise", reason: needStr(need(o, "reason")) };
    case "deny": return { kind: "deny", reason: needStr(need(o, "reason")) };
    case "annotate": return { kind: "annotate", annotations: need(o, "annotations") };
    case "none": return { kind: "none" };
    default: return fail(`unknown HookResult variant ${canon(tag)}`);
  }
}

export interface HostCapabilities {
  experimental?: boolean,
  opt_out_notifications?: string[],
  serves_permission_channel?: boolean,
  serves_host_executor?: boolean,
  serves_hook_observer?: boolean,
  serves_elicitation?: boolean,
  serves_measurement?: boolean,
  serves_principal_channel?: boolean,
  accepts_ephemeral_frames?: boolean,
  max_in_flight_sessions?: number,
  extensions?: JsonObj,
}

export function hostCapabilitiesToJson(v: HostCapabilities): Json {
  const o: JsonObj = {};
  o["experimental"] = v.experimental === true;
  o["opt_out_notifications"] = (v.opt_out_notifications ?? []).map((x) => x);
  o["serves_permission_channel"] = v.serves_permission_channel === true;
  o["serves_host_executor"] = v.serves_host_executor === true;
  o["serves_hook_observer"] = v.serves_hook_observer === true;
  o["serves_elicitation"] = v.serves_elicitation === true;
  o["serves_measurement"] = v.serves_measurement === true;
  o["serves_principal_channel"] = v.serves_principal_channel === true;
  o["accepts_ephemeral_frames"] = v.accepts_ephemeral_frames === true;
  if (v.max_in_flight_sessions !== undefined) o["max_in_flight_sessions"] = v.max_in_flight_sessions;
  if (v.extensions !== undefined && Object.keys(v.extensions).length > 0) o["extensions"] = v.extensions;
  return o;
}

export function hostCapabilitiesFromJson(v: Json): HostCapabilities {
  const o = needObj(v);
  return {
    experimental: o["experimental"] === true,
    opt_out_notifications: o["opt_out_notifications"] === undefined ? [] : needArr(o["opt_out_notifications"]).map((x) => needStr(x)),
    serves_permission_channel: o["serves_permission_channel"] === true,
    serves_host_executor: o["serves_host_executor"] === true,
    serves_hook_observer: o["serves_hook_observer"] === true,
    serves_elicitation: o["serves_elicitation"] === true,
    serves_measurement: o["serves_measurement"] === true,
    serves_principal_channel: o["serves_principal_channel"] === true,
    accepts_ephemeral_frames: o["accepts_ephemeral_frames"] === true,
    max_in_flight_sessions: o["max_in_flight_sessions"] === undefined ? undefined : needInt(o["max_in_flight_sessions"]),
    extensions: isObj(o["extensions"]) ? o["extensions"] as JsonObj : {},
  };
}

export type HostEffectOutcome =
  | { kind: "observed"; outcome: string; output_artifacts?: string[] }
  | { kind: "refused"; reason: string }
  | { kind: "unknown"; detail?: string }

export function hostEffectOutcomeToJson(v: HostEffectOutcome): Json {
  const o: JsonObj = {};
  switch (v["kind"]) {
    case "observed": {
      o["kind"] = "observed";
      o["outcome"] = v.outcome;
      o["output_artifacts"] = (v.output_artifacts ?? []).map((x) => x);
      return o;
    }
    case "refused": {
      o["kind"] = "refused";
      o["reason"] = v.reason;
      return o;
    }
    case "unknown": {
      o["kind"] = "unknown";
      if (v.detail !== undefined) o["detail"] = v.detail;
      return o;
    }
  }
}

export function hostEffectOutcomeFromJson(v: Json): HostEffectOutcome {
  const o = needObj(v);
  const tag = needStr(need(o, "kind"));
  switch (tag) {
    case "observed": return { kind: "observed", outcome: needStr(need(o, "outcome")), output_artifacts: o["output_artifacts"] === undefined ? [] : needArr(o["output_artifacts"]).map((x) => needStr(x)) };
    case "refused": return { kind: "refused", reason: needStr(need(o, "reason")) };
    case "unknown": return { kind: "unknown", detail: o["detail"] === undefined ? undefined : needStr(o["detail"]) };
    default: return fail(`unknown HostEffectOutcome variant ${canon(tag)}`);
  }
}

export interface HostResult {
  outcome: Json,
}

export function hostResultToJson(v: HostResult): Json {
  const o: JsonObj = {};
  o["outcome"] = v.outcome;
  return o;
}

export function hostResultFromJson(v: Json): HostResult {
  const o = needObj(v);
  return {
    outcome: need(o, "outcome"),
  };
}

export interface InvocationRecord {
  argv_canonical: string[],
  cwd_ref: string,
  principal: string,
  attendance: AttendanceDeclaration,
  output_format: OutputFormat,
  stdin_digest?: string,
  overrides_layer_id?: string,
  instrument_record: Json,
  idempotency_key: string,
}

export function invocationRecordToJson(v: InvocationRecord): Json {
  const o: JsonObj = {};
  o["argv_canonical"] = v.argv_canonical.map((x) => x);
  o["cwd_ref"] = v.cwd_ref;
  o["principal"] = v.principal;
  o["attendance"] = attendanceDeclarationToJson(v.attendance);
  o["output_format"] = outputFormatToJson(v.output_format);
  if (v.stdin_digest !== undefined) o["stdin_digest"] = v.stdin_digest;
  if (v.overrides_layer_id !== undefined) o["overrides_layer_id"] = v.overrides_layer_id;
  o["instrument_record"] = v.instrument_record;
  o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function invocationRecordFromJson(v: Json): InvocationRecord {
  const o = needObj(v);
  return {
    argv_canonical: needArr(need(o, "argv_canonical")).map((x) => needStr(x)),
    cwd_ref: needStr(need(o, "cwd_ref")),
    principal: needStr(need(o, "principal")),
    attendance: attendanceDeclarationFromJson(need(o, "attendance")),
    output_format: outputFormatFromJson(need(o, "output_format")),
    stdin_digest: o["stdin_digest"] === undefined ? undefined : needStr(o["stdin_digest"]),
    overrides_layer_id: o["overrides_layer_id"] === undefined ? undefined : needStr(o["overrides_layer_id"]),
    instrument_record: need(o, "instrument_record"),
    idempotency_key: needStr(need(o, "idempotency_key")),
  };
}

export interface InvokeHostCapability {
  capability_id: string,
  args: Json,
  effect_id: string,
  attempt_no: number,
}

export function invokeHostCapabilityToJson(v: InvokeHostCapability): Json {
  const o: JsonObj = {};
  o["capability_id"] = v.capability_id;
  o["args"] = v.args;
  o["effect_id"] = v.effect_id;
  o["attempt_no"] = v.attempt_no;
  return o;
}

export function invokeHostCapabilityFromJson(v: Json): InvokeHostCapability {
  const o = needObj(v);
  return {
    capability_id: needStr(need(o, "capability_id")),
    args: need(o, "args"),
    effect_id: needStr(need(o, "effect_id")),
    attempt_no: needInt(need(o, "attempt_no")),
  };
}

export type ItemKind = "context_item" | "context_delta";
export function itemKindToJson(v: ItemKind): Json { return v; }
export function itemKindFromJson(v: Json): ItemKind {
  switch (v) {
    case "context_item": return v;
    case "context_delta": return v;
    default: return fail(`unknown ItemKind variant ${canon(v)}`);
  }
}

export interface KernelDescriptor {
  version: string,
  schema_hash: string,
  contract_major: number,
  idp: string,
}

export function kernelDescriptorToJson(v: KernelDescriptor): Json {
  const o: JsonObj = {};
  o["version"] = v.version;
  o["schema_hash"] = v.schema_hash;
  o["contract_major"] = v.contract_major;
  o["idp"] = v.idp;
  return o;
}

export function kernelDescriptorFromJson(v: Json): KernelDescriptor {
  const o = needObj(v);
  return {
    version: needStr(need(o, "version")),
    schema_hash: needStr(need(o, "schema_hash")),
    contract_major: needInt(need(o, "contract_major")),
    idp: needStr(need(o, "idp")),
  };
}

export interface LineageParams {
  session_id: string,
}

export function lineageParamsToJson(v: LineageParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  return o;
}

export function lineageParamsFromJson(v: Json): LineageParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
  };
}

export interface ListLeasesParams {
  session_id: string,
}

export function listLeasesParamsToJson(v: ListLeasesParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  return o;
}

export function listLeasesParamsFromJson(v: Json): ListLeasesParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
  };
}

export interface ListLeasesResult {
  leases: Json[],
}

export function listLeasesResultToJson(v: ListLeasesResult): Json {
  const o: JsonObj = {};
  o["leases"] = v.leases.map((x) => x);
  return o;
}

export function listLeasesResultFromJson(v: Json): ListLeasesResult {
  const o = needObj(v);
  return {
    leases: needArr(need(o, "leases")).map((x) => x),
  };
}

export interface NavigateParams {
  session_id: string,
  to?: Json,
  reason?: string,
  idempotency_key?: string,
}

export function navigateParamsToJson(v: NavigateParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  if (v.to !== undefined) o["to"] = v.to;
  if (v.reason !== undefined) o["reason"] = v.reason;
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function navigateParamsFromJson(v: Json): NavigateParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    to: o["to"] === undefined ? undefined : o["to"],
    reason: o["reason"] === undefined ? undefined : needStr(o["reason"]),
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export interface OpenInboxParams {
  goal_ref: string,
}

export function openInboxParamsToJson(v: OpenInboxParams): Json {
  const o: JsonObj = {};
  o["goal_ref"] = v.goal_ref;
  return o;
}

export function openInboxParamsFromJson(v: Json): OpenInboxParams {
  const o = needObj(v);
  return {
    goal_ref: needStr(need(o, "goal_ref")),
  };
}

export interface OpenSessionParams {
  spec: OpenSpec,
  idempotency_key: string,
  invocation?: InvocationRecord,
  client?: ClientDecl,
  contract_json?: Json,
}

export function openSessionParamsToJson(v: OpenSessionParams): Json {
  const o: JsonObj = {};
  o["spec"] = openSpecToJson(v.spec);
  o["idempotency_key"] = v.idempotency_key;
  if (v.invocation !== undefined) o["invocation"] = invocationRecordToJson(v.invocation);
  if (v.client !== undefined) o["client"] = clientDeclToJson(v.client);
  if (v.contract_json !== undefined) o["contract_json"] = v.contract_json;
  return o;
}

export function openSessionParamsFromJson(v: Json): OpenSessionParams {
  const o = needObj(v);
  return {
    spec: openSpecFromJson(need(o, "spec")),
    idempotency_key: needStr(need(o, "idempotency_key")),
    invocation: o["invocation"] === undefined ? undefined : invocationRecordFromJson(o["invocation"]),
    client: o["client"] === undefined ? undefined : clientDeclFromJson(o["client"]),
    contract_json: o["contract_json"] === undefined ? undefined : o["contract_json"],
  };
}

export type OpenSpec =
  | { kind: "new"; definition: DefinitionInput; overrides?: Override[]; profile_binding?: Json; environment: EnvironmentInput; budget?: BudgetInput; participant?: Json; supplies?: Supplies; attendance: AttendanceDeclaration; approval_mode?: string; spawn_event?: SpawnEventRef; notification_sink?: string }
  | { kind: "resume"; run_id: string; mode: ResumeMode; from_seq?: number; definition?: DefinitionInput; cause?: ResumeCause }
  | { kind: "attach"; run_id: string; read_only?: boolean }

export function openSpecToJson(v: OpenSpec): Json {
  const o: JsonObj = {};
  switch (v["kind"]) {
    case "new": {
      o["kind"] = "new";
      o["definition"] = definitionInputToJson(v.definition);
      o["overrides"] = (v.overrides ?? []).map((x) => overrideToJson(x));
      if (v.profile_binding !== undefined) o["profile_binding"] = v.profile_binding;
      o["environment"] = environmentInputToJson(v.environment);
      if (v.budget !== undefined) o["budget"] = budgetInputToJson(v.budget);
      if (v.participant !== undefined) o["participant"] = v.participant;
      if (v.supplies !== undefined) o["supplies"] = suppliesToJson(v.supplies);
      o["attendance"] = attendanceDeclarationToJson(v.attendance);
      if (v.approval_mode !== undefined) o["approval_mode"] = v.approval_mode;
      if (v.spawn_event !== undefined) o["spawn_event"] = spawnEventRefToJson(v.spawn_event);
      if (v.notification_sink !== undefined) o["notification_sink"] = v.notification_sink;
      return o;
    }
    case "resume": {
      o["kind"] = "resume";
      o["run_id"] = v.run_id;
      o["mode"] = resumeModeToJson(v.mode);
      if (v.from_seq !== undefined) o["from_seq"] = v.from_seq;
      if (v.definition !== undefined) o["definition"] = definitionInputToJson(v.definition);
      if (v.cause !== undefined) o["cause"] = resumeCauseToJson(v.cause);
      return o;
    }
    case "attach": {
      o["kind"] = "attach";
      o["run_id"] = v.run_id;
      o["read_only"] = v.read_only === true;
      return o;
    }
  }
}

export function openSpecFromJson(v: Json): OpenSpec {
  const o = needObj(v);
  const tag = needStr(need(o, "kind"));
  switch (tag) {
    case "new": return { kind: "new", definition: definitionInputFromJson(need(o, "definition")), overrides: o["overrides"] === undefined ? [] : needArr(o["overrides"]).map((x) => overrideFromJson(x)), profile_binding: o["profile_binding"] === undefined ? undefined : o["profile_binding"], environment: environmentInputFromJson(need(o, "environment")), budget: o["budget"] === undefined ? undefined : budgetInputFromJson(o["budget"]), participant: o["participant"] === undefined ? undefined : o["participant"], supplies: o["supplies"] === undefined ? undefined : suppliesFromJson(o["supplies"]), attendance: attendanceDeclarationFromJson(need(o, "attendance")), approval_mode: o["approval_mode"] === undefined ? undefined : needStr(o["approval_mode"]), spawn_event: o["spawn_event"] === undefined ? undefined : spawnEventRefFromJson(o["spawn_event"]), notification_sink: o["notification_sink"] === undefined ? undefined : needStr(o["notification_sink"]) };
    case "resume": return { kind: "resume", run_id: needStr(need(o, "run_id")), mode: resumeModeFromJson(need(o, "mode")), from_seq: o["from_seq"] === undefined ? undefined : needInt(o["from_seq"]), definition: o["definition"] === undefined ? undefined : definitionInputFromJson(o["definition"]), cause: o["cause"] === undefined ? undefined : resumeCauseFromJson(o["cause"]) };
    case "attach": return { kind: "attach", run_id: needStr(need(o, "run_id")), read_only: o["read_only"] === true };
    default: return fail(`unknown OpenSpec variant ${canon(tag)}`);
  }
}

export type OutputFormat = "human" | "json" | "jsonl";
export function outputFormatToJson(v: OutputFormat): Json { return v; }
export function outputFormatFromJson(v: Json): OutputFormat {
  switch (v) {
    case "human": return v;
    case "json": return v;
    case "jsonl": return v;
    default: return fail(`unknown OutputFormat variant ${canon(v)}`);
  }
}

export interface Override {
  pointer: string,
  value: Json,
}

export function overrideToJson(v: Override): Json {
  const o: JsonObj = {};
  o["pointer"] = v.pointer;
  o["value"] = v.value;
  return o;
}

export function overrideFromJson(v: Json): Override {
  const o = needObj(v);
  return {
    pointer: needStr(need(o, "pointer")),
    value: need(o, "value"),
  };
}

export interface Page {
  events: Json[],
  next?: ReadCursor,
}

export function pageToJson(v: Page): Json {
  const o: JsonObj = {};
  o["events"] = v.events.map((x) => x);
  if (v.next !== undefined) o["next"] = readCursorToJson(v.next);
  return o;
}

export function pageFromJson(v: Json): Page {
  const o = needObj(v);
  return {
    events: needArr(need(o, "events")).map((x) => x),
    next: o["next"] === undefined ? undefined : readCursorFromJson(o["next"]),
  };
}

export interface PermissionOption {
  option_id: string,
  label: string,
}

export function permissionOptionToJson(v: PermissionOption): Json {
  const o: JsonObj = {};
  o["option_id"] = v.option_id;
  o["label"] = v.label;
  return o;
}

export function permissionOptionFromJson(v: Json): PermissionOption {
  const o = needObj(v);
  return {
    option_id: needStr(need(o, "option_id")),
    label: needStr(need(o, "label")),
  };
}

export type PermissionOutcome =
  | { kind: "selected"; option_id: string }
  | { kind: "cancelled" }
  | { kind: "modified"; amended_args: Json }
  | { kind: "remedy"; remedy: Json }

export function permissionOutcomeToJson(v: PermissionOutcome): Json {
  const o: JsonObj = {};
  switch (v["kind"]) {
    case "selected": {
      o["kind"] = "selected";
      o["option_id"] = v.option_id;
      return o;
    }
    case "cancelled": {
      o["kind"] = "cancelled";
      return o;
    }
    case "modified": {
      o["kind"] = "modified";
      o["amended_args"] = v.amended_args;
      return o;
    }
    case "remedy": {
      o["kind"] = "remedy";
      o["remedy"] = v.remedy;
      return o;
    }
  }
}

export function permissionOutcomeFromJson(v: Json): PermissionOutcome {
  const o = needObj(v);
  const tag = needStr(need(o, "kind"));
  switch (tag) {
    case "selected": return { kind: "selected", option_id: needStr(need(o, "option_id")) };
    case "cancelled": return { kind: "cancelled" };
    case "modified": return { kind: "modified", amended_args: need(o, "amended_args") };
    case "remedy": return { kind: "remedy", remedy: need(o, "remedy") };
    default: return fail(`unknown PermissionOutcome variant ${canon(tag)}`);
  }
}

export interface ProjectParams {
  session_id: string,
  view_kind: ViewKind,
  until_seq?: number,
}

export function projectParamsToJson(v: ProjectParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["view_kind"] = viewKindToJson(v.view_kind);
  if (v.until_seq !== undefined) o["until_seq"] = v.until_seq;
  return o;
}

export function projectParamsFromJson(v: Json): ProjectParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    view_kind: viewKindFromJson(need(o, "view_kind")),
    until_seq: o["until_seq"] === undefined ? undefined : needInt(o["until_seq"]),
  };
}

export interface PromoteParams {
  session_id: string,
  branch_id: string,
  idempotency_key?: string,
}

export function promoteParamsToJson(v: PromoteParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["branch_id"] = v.branch_id;
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function promoteParamsFromJson(v: Json): PromoteParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    branch_id: needStr(need(o, "branch_id")),
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export interface ProveConsistencyParams {
  session_id: string,
  first_size: number,
  second_size: number,
}

export function proveConsistencyParamsToJson(v: ProveConsistencyParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["first_size"] = v.first_size;
  o["second_size"] = v.second_size;
  return o;
}

export function proveConsistencyParamsFromJson(v: Json): ProveConsistencyParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    first_size: needInt(need(o, "first_size")),
    second_size: needInt(need(o, "second_size")),
  };
}

export interface ProveInclusionParams {
  session_id: string,
  seq: number,
}

export function proveInclusionParamsToJson(v: ProveInclusionParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["seq"] = v.seq;
  return o;
}

export function proveInclusionParamsFromJson(v: Json): ProveInclusionParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    seq: needInt(need(o, "seq")),
  };
}

export type ReadCursor =
  | { kind: "seq"; seq: number }
  | { kind: "event_id"; event_id: string }
  | { kind: "now" }

export function readCursorToJson(v: ReadCursor): Json {
  const o: JsonObj = {};
  switch (v["kind"]) {
    case "seq": {
      o["kind"] = "seq";
      o["seq"] = v.seq;
      return o;
    }
    case "event_id": {
      o["kind"] = "event_id";
      o["event_id"] = v.event_id;
      return o;
    }
    case "now": {
      o["kind"] = "now";
      return o;
    }
  }
}

export function readCursorFromJson(v: Json): ReadCursor {
  const o = needObj(v);
  const tag = needStr(need(o, "kind"));
  switch (tag) {
    case "seq": return { kind: "seq", seq: needInt(need(o, "seq")) };
    case "event_id": return { kind: "event_id", event_id: needStr(need(o, "event_id")) };
    case "now": return { kind: "now" };
    default: return fail(`unknown ReadCursor variant ${canon(tag)}`);
  }
}

export type ReadDirection = "fwd" | "rev";
export function readDirectionToJson(v: ReadDirection): Json { return v; }
export function readDirectionFromJson(v: Json): ReadDirection {
  switch (v) {
    case "fwd": return v;
    case "rev": return v;
    default: return fail(`unknown ReadDirection variant ${canon(v)}`);
  }
}

export interface ReadParams {
  session_id: string,
  cursor: ReadCursor,
  direction?: ReadDirection,
  limit?: number,
  filter?: ClassFilter,
}

export function readParamsToJson(v: ReadParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["cursor"] = readCursorToJson(v.cursor);
  if (v.direction !== undefined) o["direction"] = readDirectionToJson(v.direction);
  if (v.limit !== undefined) o["limit"] = v.limit;
  if (v.filter !== undefined) o["filter"] = classFilterToJson(v.filter);
  return o;
}

export function readParamsFromJson(v: Json): ReadParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    cursor: readCursorFromJson(need(o, "cursor")),
    direction: o["direction"] === undefined ? undefined : readDirectionFromJson(o["direction"]),
    limit: o["limit"] === undefined ? undefined : needInt(o["limit"]),
    filter: o["filter"] === undefined ? undefined : classFilterFromJson(o["filter"]),
  };
}

export interface RealizedSettings {
  model_role_table_realized: Json,
  cwd: string,
  containment_effective: Json,
  policy_mode: string,
  profile_bindings: Json,
  protocol_bindings?: Json[],
  secrets_declared?: Json[],
  attendance: AttendanceDeclaration,
}

export function realizedSettingsToJson(v: RealizedSettings): Json {
  const o: JsonObj = {};
  o["model_role_table_realized"] = v.model_role_table_realized;
  o["cwd"] = v.cwd;
  o["containment_effective"] = v.containment_effective;
  o["policy_mode"] = v.policy_mode;
  o["profile_bindings"] = v.profile_bindings;
  o["protocol_bindings"] = (v.protocol_bindings ?? []).map((x) => x);
  o["secrets_declared"] = (v.secrets_declared ?? []).map((x) => x);
  o["attendance"] = attendanceDeclarationToJson(v.attendance);
  return o;
}

export function realizedSettingsFromJson(v: Json): RealizedSettings {
  const o = needObj(v);
  return {
    model_role_table_realized: need(o, "model_role_table_realized"),
    cwd: needStr(need(o, "cwd")),
    containment_effective: need(o, "containment_effective"),
    policy_mode: needStr(need(o, "policy_mode")),
    profile_bindings: need(o, "profile_bindings"),
    protocol_bindings: o["protocol_bindings"] === undefined ? [] : needArr(o["protocol_bindings"]).map((x) => x),
    secrets_declared: o["secrets_declared"] === undefined ? [] : needArr(o["secrets_declared"]).map((x) => x),
    attendance: attendanceDeclarationFromJson(need(o, "attendance")),
  };
}

export interface RecordOccurrenceParams {
  session_id: string,
  subscription_id: string,
  occurrence_key: string,
  payload_ref?: string,
  observed_at_ms?: number,
}

export function recordOccurrenceParamsToJson(v: RecordOccurrenceParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["subscription_id"] = v.subscription_id;
  o["occurrence_key"] = v.occurrence_key;
  if (v.payload_ref !== undefined) o["payload_ref"] = v.payload_ref;
  if (v.observed_at_ms !== undefined) o["observed_at_ms"] = v.observed_at_ms;
  return o;
}

export function recordOccurrenceParamsFromJson(v: Json): RecordOccurrenceParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    subscription_id: needStr(need(o, "subscription_id")),
    occurrence_key: needStr(need(o, "occurrence_key")),
    payload_ref: o["payload_ref"] === undefined ? undefined : needStr(o["payload_ref"]),
    observed_at_ms: o["observed_at_ms"] === undefined ? undefined : needInt(o["observed_at_ms"]),
  };
}

export interface Recorded {
  recorded: boolean,
  permission_id: string,
}

export function recordedToJson(v: Recorded): Json {
  const o: JsonObj = {};
  o["recorded"] = v.recorded;
  o["permission_id"] = v.permission_id;
  return o;
}

export function recordedFromJson(v: Json): Recorded {
  const o = needObj(v);
  return {
    recorded: needBool(need(o, "recorded")),
    permission_id: needStr(need(o, "permission_id")),
  };
}

export interface ReplayParams {
  session_id: string,
  run_id?: string,
  driver_mode: string,
  until_seq?: number,
  idempotency_key?: string,
}

export function replayParamsToJson(v: ReplayParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  if (v.run_id !== undefined) o["run_id"] = v.run_id;
  o["driver_mode"] = v.driver_mode;
  if (v.until_seq !== undefined) o["until_seq"] = v.until_seq;
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function replayParamsFromJson(v: Json): ReplayParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    run_id: o["run_id"] === undefined ? undefined : needStr(o["run_id"]),
    driver_mode: needStr(need(o, "driver_mode")),
    until_seq: o["until_seq"] === undefined ? undefined : needInt(o["until_seq"]),
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export interface ReplayResult {
  run_id: string,
  driver_mode: string,
  mode: string,
  report_ref: string,
  reproduced?: boolean,
  divergence?: Json,
}

export function replayResultToJson(v: ReplayResult): Json {
  const o: JsonObj = {};
  o["run_id"] = v.run_id;
  o["driver_mode"] = v.driver_mode;
  o["mode"] = v.mode;
  o["report_ref"] = v.report_ref;
  o["reproduced"] = v.reproduced === true;
  if (v.divergence !== undefined) o["divergence"] = v.divergence;
  return o;
}

export function replayResultFromJson(v: Json): ReplayResult {
  const o = needObj(v);
  return {
    run_id: needStr(need(o, "run_id")),
    driver_mode: needStr(need(o, "driver_mode")),
    mode: needStr(need(o, "mode")),
    report_ref: needStr(need(o, "report_ref")),
    reproduced: o["reproduced"] === true,
    divergence: o["divergence"] === undefined ? undefined : o["divergence"],
  };
}

export interface ReportHostEffectParams {
  session_id: string,
  effect_id: string,
  attempt_no: number,
  outcome: HostEffectOutcome,
  idempotency_key?: string,
}

export function reportHostEffectParamsToJson(v: ReportHostEffectParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["effect_id"] = v.effect_id;
  o["attempt_no"] = v.attempt_no;
  o["outcome"] = hostEffectOutcomeToJson(v.outcome);
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function reportHostEffectParamsFromJson(v: Json): ReportHostEffectParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    effect_id: needStr(need(o, "effect_id")),
    attempt_no: needInt(need(o, "attempt_no")),
    outcome: hostEffectOutcomeFromJson(need(o, "outcome")),
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export interface RequestPermission {
  permission_id: string,
  proposal: string,
  options: PermissionOption[],
  effect_id?: string,
  rendering?: Json,
}

export function requestPermissionToJson(v: RequestPermission): Json {
  const o: JsonObj = {};
  o["permission_id"] = v.permission_id;
  o["proposal"] = v.proposal;
  o["options"] = v.options.map((x) => permissionOptionToJson(x));
  if (v.effect_id !== undefined) o["effect_id"] = v.effect_id;
  if (v.rendering !== undefined) o["rendering"] = v.rendering;
  return o;
}

export function requestPermissionFromJson(v: Json): RequestPermission {
  const o = needObj(v);
  return {
    permission_id: needStr(need(o, "permission_id")),
    proposal: needStr(need(o, "proposal")),
    options: needArr(need(o, "options")).map((x) => permissionOptionFromJson(x)),
    effect_id: o["effect_id"] === undefined ? undefined : needStr(o["effect_id"]),
    rendering: o["rendering"] === undefined ? undefined : o["rendering"],
  };
}

export interface RequestRedactionParams {
  session_id: string,
  refs: string[],
  reason: string,
  idempotency_key?: string,
}

export function requestRedactionParamsToJson(v: RequestRedactionParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["refs"] = v.refs.map((x) => x);
  o["reason"] = v.reason;
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function requestRedactionParamsFromJson(v: Json): RequestRedactionParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    refs: needArr(need(o, "refs")).map((x) => needStr(x)),
    reason: needStr(need(o, "reason")),
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export interface RespondElicitationParams {
  session_id: string,
  elicitation_id: string,
  outcome: ElicitOutcome,
  idempotency_key?: string,
}

export function respondElicitationParamsToJson(v: RespondElicitationParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["elicitation_id"] = v.elicitation_id;
  o["outcome"] = elicitOutcomeToJson(v.outcome);
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function respondElicitationParamsFromJson(v: Json): RespondElicitationParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    elicitation_id: needStr(need(o, "elicitation_id")),
    outcome: elicitOutcomeFromJson(need(o, "outcome")),
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export interface RespondPermissionParams {
  session_id: string,
  permission_id: string,
  outcome: PermissionOutcome,
  idempotency_key: string,
  responder?: ResponderDecl,
}

export function respondPermissionParamsToJson(v: RespondPermissionParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["permission_id"] = v.permission_id;
  o["outcome"] = permissionOutcomeToJson(v.outcome);
  o["idempotency_key"] = v.idempotency_key;
  if (v.responder !== undefined) o["responder"] = responderDeclToJson(v.responder);
  return o;
}

export function respondPermissionParamsFromJson(v: Json): RespondPermissionParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    permission_id: needStr(need(o, "permission_id")),
    outcome: permissionOutcomeFromJson(need(o, "outcome")),
    idempotency_key: needStr(need(o, "idempotency_key")),
    responder: o["responder"] === undefined ? undefined : responderDeclFromJson(o["responder"]),
  };
}

export interface ResponderDecl {
  subject_ref: string,
  surface_session_ref?: string,
}

export function responderDeclToJson(v: ResponderDecl): Json {
  const o: JsonObj = {};
  o["subject_ref"] = v.subject_ref;
  if (v.surface_session_ref !== undefined) o["surface_session_ref"] = v.surface_session_ref;
  return o;
}

export function responderDeclFromJson(v: Json): ResponderDecl {
  const o = needObj(v);
  return {
    subject_ref: needStr(need(o, "subject_ref")),
    surface_session_ref: o["surface_session_ref"] === undefined ? undefined : needStr(o["surface_session_ref"]),
  };
}

export type ResumeCause = "crash" | "takeover" | "wakeup" | "operator" | "continuation";
export function resumeCauseToJson(v: ResumeCause): Json { return v; }
export function resumeCauseFromJson(v: Json): ResumeCause {
  switch (v) {
    case "crash": return v;
    case "takeover": return v;
    case "wakeup": return v;
    case "operator": return v;
    case "continuation": return v;
    default: return fail(`unknown ResumeCause variant ${canon(v)}`);
  }
}

export type ResumeMode = "continue" | "takeover";
export function resumeModeToJson(v: ResumeMode): Json { return v; }
export function resumeModeFromJson(v: Json): ResumeMode {
  switch (v) {
    case "continue": return v;
    case "takeover": return v;
    default: return fail(`unknown ResumeMode variant ${canon(v)}`);
  }
}

export interface RevokeLeaseParams {
  session_id: string,
  lease_id: string,
  idempotency_key?: string,
}

export function revokeLeaseParamsToJson(v: RevokeLeaseParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["lease_id"] = v.lease_id;
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function revokeLeaseParamsFromJson(v: Json): RevokeLeaseParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    lease_id: needStr(need(o, "lease_id")),
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export interface RollbackParams {
  session_id: string,
  to_seq: number,
  reason?: string,
  restore_env?: boolean,
  idempotency_key?: string,
}

export function rollbackParamsToJson(v: RollbackParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["to_seq"] = v.to_seq;
  if (v.reason !== undefined) o["reason"] = v.reason;
  o["restore_env"] = v.restore_env === true;
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function rollbackParamsFromJson(v: Json): RollbackParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    to_seq: needInt(need(o, "to_seq")),
    reason: o["reason"] === undefined ? undefined : needStr(o["reason"]),
    restore_env: o["restore_env"] === true,
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export interface RunIndexEntry {
  run_id: string,
  run_kind: string,
  participant_class: string,
  status: string,
  head_seq: number,
  outcome_class?: string,
  configuration_id?: string,
  experiment_ref?: string,
  opened_ts?: string,
}

export function runIndexEntryToJson(v: RunIndexEntry): Json {
  const o: JsonObj = {};
  o["run_id"] = v.run_id;
  o["run_kind"] = v.run_kind;
  o["participant_class"] = v.participant_class;
  o["status"] = v.status;
  o["head_seq"] = v.head_seq;
  if (v.outcome_class !== undefined) o["outcome_class"] = v.outcome_class;
  if (v.configuration_id !== undefined) o["configuration_id"] = v.configuration_id;
  if (v.experiment_ref !== undefined) o["experiment_ref"] = v.experiment_ref;
  if (v.opened_ts !== undefined) o["opened_ts"] = v.opened_ts;
  return o;
}

export function runIndexEntryFromJson(v: Json): RunIndexEntry {
  const o = needObj(v);
  return {
    run_id: needStr(need(o, "run_id")),
    run_kind: needStr(need(o, "run_kind")),
    participant_class: needStr(need(o, "participant_class")),
    status: needStr(need(o, "status")),
    head_seq: needInt(need(o, "head_seq")),
    outcome_class: o["outcome_class"] === undefined ? undefined : needStr(o["outcome_class"]),
    configuration_id: o["configuration_id"] === undefined ? undefined : needStr(o["configuration_id"]),
    experiment_ref: o["experiment_ref"] === undefined ? undefined : needStr(o["experiment_ref"]),
    opened_ts: o["opened_ts"] === undefined ? undefined : needStr(o["opened_ts"]),
  };
}

export interface RunIndexFilter {
  run_kind?: string,
  participant_class?: string,
  status?: string,
  outcome_class?: string,
  configuration_id?: string,
  experiment_ref?: string,
  text?: string,
}

export function runIndexFilterToJson(v: RunIndexFilter): Json {
  const o: JsonObj = {};
  if (v.run_kind !== undefined) o["run_kind"] = v.run_kind;
  if (v.participant_class !== undefined) o["participant_class"] = v.participant_class;
  if (v.status !== undefined) o["status"] = v.status;
  if (v.outcome_class !== undefined) o["outcome_class"] = v.outcome_class;
  if (v.configuration_id !== undefined) o["configuration_id"] = v.configuration_id;
  if (v.experiment_ref !== undefined) o["experiment_ref"] = v.experiment_ref;
  if (v.text !== undefined) o["text"] = v.text;
  return o;
}

export function runIndexFilterFromJson(v: Json): RunIndexFilter {
  const o = needObj(v);
  return {
    run_kind: o["run_kind"] === undefined ? undefined : needStr(o["run_kind"]),
    participant_class: o["participant_class"] === undefined ? undefined : needStr(o["participant_class"]),
    status: o["status"] === undefined ? undefined : needStr(o["status"]),
    outcome_class: o["outcome_class"] === undefined ? undefined : needStr(o["outcome_class"]),
    configuration_id: o["configuration_id"] === undefined ? undefined : needStr(o["configuration_id"]),
    experiment_ref: o["experiment_ref"] === undefined ? undefined : needStr(o["experiment_ref"]),
    text: o["text"] === undefined ? undefined : needStr(o["text"]),
  };
}

export interface RunIndexParams {
  filters?: RunIndexFilter,
  cursor?: string,
  limit?: number,
}

export function runIndexParamsToJson(v: RunIndexParams): Json {
  const o: JsonObj = {};
  if (v.filters !== undefined) o["filters"] = runIndexFilterToJson(v.filters);
  if (v.cursor !== undefined) o["cursor"] = v.cursor;
  if (v.limit !== undefined) o["limit"] = v.limit;
  return o;
}

export function runIndexParamsFromJson(v: Json): RunIndexParams {
  const o = needObj(v);
  return {
    filters: o["filters"] === undefined ? undefined : runIndexFilterFromJson(o["filters"]),
    cursor: o["cursor"] === undefined ? undefined : needStr(o["cursor"]),
    limit: o["limit"] === undefined ? undefined : needInt(o["limit"]),
  };
}

export interface RunSummaryRef {
  run_id: string,
  seq: number,
  hash: string,
}

export function runSummaryRefToJson(v: RunSummaryRef): Json {
  const o: JsonObj = {};
  o["run_id"] = v.run_id;
  o["seq"] = v.seq;
  o["hash"] = v.hash;
  return o;
}

export function runSummaryRefFromJson(v: Json): RunSummaryRef {
  const o = needObj(v);
  return {
    run_id: needStr(need(o, "run_id")),
    seq: needInt(need(o, "seq")),
    hash: needStr(need(o, "hash")),
  };
}

export interface SeqCursor {
  seq: number,
}

export function seqCursorToJson(v: SeqCursor): Json {
  const o: JsonObj = {};
  o["seq"] = v.seq;
  return o;
}

export function seqCursorFromJson(v: Json): SeqCursor {
  const o = needObj(v);
  return {
    seq: needInt(need(o, "seq")),
  };
}

export interface Session {
  session_id: string,
  run_id: string,
  attachment_id: string,
  manifest_ref: string,
  configuration_id: string,
  configuration_version_id: string,
  realized: RealizedSettings,
  cursor: SeqCursor,
}

export function sessionToJson(v: Session): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["run_id"] = v.run_id;
  o["attachment_id"] = v.attachment_id;
  o["manifest_ref"] = v.manifest_ref;
  o["configuration_id"] = v.configuration_id;
  o["configuration_version_id"] = v.configuration_version_id;
  o["realized"] = realizedSettingsToJson(v.realized);
  o["cursor"] = seqCursorToJson(v.cursor);
  return o;
}

export function sessionFromJson(v: Json): Session {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    run_id: needStr(need(o, "run_id")),
    attachment_id: needStr(need(o, "attachment_id")),
    manifest_ref: needStr(need(o, "manifest_ref")),
    configuration_id: needStr(need(o, "configuration_id")),
    configuration_version_id: needStr(need(o, "configuration_version_id")),
    realized: realizedSettingsFromJson(need(o, "realized")),
    cursor: seqCursorFromJson(need(o, "cursor")),
  };
}

export interface SetCoordinateParams {
  session_id: string,
  coordinate: string,
  value: Json,
  idempotency_key?: string,
}

export function setCoordinateParamsToJson(v: SetCoordinateParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["coordinate"] = v.coordinate;
  o["value"] = v.value;
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function setCoordinateParamsFromJson(v: Json): SetCoordinateParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    coordinate: needStr(need(o, "coordinate")),
    value: need(o, "value"),
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export interface SpawnEventRef {
  run_id: string,
  event_id: string,
}

export function spawnEventRefToJson(v: SpawnEventRef): Json {
  const o: JsonObj = {};
  o["run_id"] = v.run_id;
  o["event_id"] = v.event_id;
  return o;
}

export function spawnEventRefFromJson(v: Json): SpawnEventRef {
  const o = needObj(v);
  return {
    run_id: needStr(need(o, "run_id")),
    event_id: needStr(need(o, "event_id")),
  };
}

export interface SteerParams {
  session_id: string,
  expected_turn_id?: string,
  input: Json[],
  idempotency_key?: string,
}

export function steerParamsToJson(v: SteerParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  if (v.expected_turn_id !== undefined) o["expected_turn_id"] = v.expected_turn_id;
  o["input"] = v.input.map((x) => x);
  if (v.idempotency_key !== undefined) o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function steerParamsFromJson(v: Json): SteerParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    expected_turn_id: o["expected_turn_id"] === undefined ? undefined : needStr(o["expected_turn_id"]),
    input: needArr(need(o, "input")).map((x) => x),
    idempotency_key: o["idempotency_key"] === undefined ? undefined : needStr(o["idempotency_key"]),
  };
}

export interface StreamEventsParams {
  session_id: string,
  from?: ReadCursor,
  filter?: ClassFilter,
}

export function streamEventsParamsToJson(v: StreamEventsParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  if (v.from !== undefined) o["from"] = readCursorToJson(v.from);
  if (v.filter !== undefined) o["filter"] = classFilterToJson(v.filter);
  return o;
}

export function streamEventsParamsFromJson(v: Json): StreamEventsParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    from: o["from"] === undefined ? undefined : readCursorFromJson(o["from"]),
    filter: o["filter"] === undefined ? undefined : classFilterFromJson(o["filter"]),
  };
}

export interface StreamNotification {
  subscription_id: string,
  frame: Frame,
}

export function streamNotificationToJson(v: StreamNotification): Json {
  const o: JsonObj = {};
  o["subscription_id"] = v.subscription_id;
  o["frame"] = frameToJson(v.frame);
  return o;
}

export function streamNotificationFromJson(v: Json): StreamNotification {
  const o = needObj(v);
  return {
    subscription_id: needStr(need(o, "subscription_id")),
    frame: frameFromJson(need(o, "frame")),
  };
}

export interface StreamTicket {
  subscription_id: string,
}

export function streamTicketToJson(v: StreamTicket): Json {
  const o: JsonObj = {};
  o["subscription_id"] = v.subscription_id;
  return o;
}

export function streamTicketFromJson(v: Json): StreamTicket {
  const o = needObj(v);
  return {
    subscription_id: needStr(need(o, "subscription_id")),
  };
}

export interface SubmitParams {
  session_id: string,
  input: Json[],
  idempotency_key: string,
}

export function submitParamsToJson(v: SubmitParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["input"] = v.input.map((x) => x);
  o["idempotency_key"] = v.idempotency_key;
  return o;
}

export function submitParamsFromJson(v: Json): SubmitParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    input: needArr(need(o, "input")).map((x) => x),
    idempotency_key: needStr(need(o, "idempotency_key")),
  };
}

export interface SubscribeParams {
  session_id: string,
  trigger: Json,
  policy?: Json,
}

export function subscribeParamsToJson(v: SubscribeParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["trigger"] = v.trigger;
  if (v.policy !== undefined) o["policy"] = v.policy;
  return o;
}

export function subscribeParamsFromJson(v: Json): SubscribeParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    trigger: need(o, "trigger"),
    policy: o["policy"] === undefined ? undefined : o["policy"],
  };
}

export interface Supplies {
  context?: Json[],
  host_capabilities?: Json[],
  mcp_servers?: Json[],
  procedures?: Json[],
}

export function suppliesToJson(v: Supplies): Json {
  const o: JsonObj = {};
  o["context"] = (v.context ?? []).map((x) => x);
  o["host_capabilities"] = (v.host_capabilities ?? []).map((x) => x);
  o["mcp_servers"] = (v.mcp_servers ?? []).map((x) => x);
  o["procedures"] = (v.procedures ?? []).map((x) => x);
  return o;
}

export function suppliesFromJson(v: Json): Supplies {
  const o = needObj(v);
  return {
    context: o["context"] === undefined ? [] : needArr(o["context"]).map((x) => x),
    host_capabilities: o["host_capabilities"] === undefined ? [] : needArr(o["host_capabilities"]).map((x) => x),
    mcp_servers: o["mcp_servers"] === undefined ? [] : needArr(o["mcp_servers"]).map((x) => x),
    procedures: o["procedures"] === undefined ? [] : needArr(o["procedures"]).map((x) => x),
  };
}

export interface SuspendParams {
  session_id: string,
  reasons: Json[],
  subscription_ids?: string[],
  resume_policy?: Json,
  release_lease?: boolean,
}

export function suspendParamsToJson(v: SuspendParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  o["reasons"] = v.reasons.map((x) => x);
  o["subscription_ids"] = (v.subscription_ids ?? []).map((x) => x);
  if (v.resume_policy !== undefined) o["resume_policy"] = v.resume_policy;
  o["release_lease"] = v.release_lease === true;
  return o;
}

export function suspendParamsFromJson(v: Json): SuspendParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    reasons: needArr(need(o, "reasons")).map((x) => x),
    subscription_ids: o["subscription_ids"] === undefined ? [] : needArr(o["subscription_ids"]).map((x) => needStr(x)),
    resume_policy: o["resume_policy"] === undefined ? undefined : o["resume_policy"],
    release_lease: o["release_lease"] === true,
  };
}

export interface VerifyParams {
  session_id: string,
  until_seq?: number,
}

export function verifyParamsToJson(v: VerifyParams): Json {
  const o: JsonObj = {};
  o["session_id"] = v.session_id;
  if (v.until_seq !== undefined) o["until_seq"] = v.until_seq;
  return o;
}

export function verifyParamsFromJson(v: Json): VerifyParams {
  const o = needObj(v);
  return {
    session_id: needStr(need(o, "session_id")),
    until_seq: o["until_seq"] === undefined ? undefined : needInt(o["until_seq"]),
  };
}

export interface View {
  payload: Json,
  derived_from: DerivedFrom,
  view_hash: string,
}

export function viewToJson(v: View): Json {
  const o: JsonObj = {};
  o["payload"] = v.payload;
  o["derived_from"] = derivedFromToJson(v.derived_from);
  o["view_hash"] = v.view_hash;
  return o;
}

export function viewFromJson(v: Json): View {
  const o = needObj(v);
  return {
    payload: need(o, "payload"),
    derived_from: derivedFromFromJson(need(o, "derived_from")),
    view_hash: needStr(need(o, "view_hash")),
  };
}

export type ViewKind = "context_view" | "run_summary" | "checkpoint" | "trace_view" | "cost_view" | "effect_ledger" | "branch_tree";
export function viewKindToJson(v: ViewKind): Json { return v; }
export function viewKindFromJson(v: Json): ViewKind {
  switch (v) {
    case "context_view": return v;
    case "run_summary": return v;
    case "checkpoint": return v;
    case "trace_view": return v;
    case "cost_view": return v;
    case "effect_ledger": return v;
    case "branch_tree": return v;
    default: return fail(`unknown ViewKind variant ${canon(v)}`);
  }
}

export type Frame =
  | { kind: "durable"; seq: number; hash: string; event: Json; durability: string }
  | { kind: "sync"; from_seq: number; through_seq: number; head_hash: string; durability: string }
  | { kind: "ephemeral"; ephemeral_kind: EphemeralKind; scope: Json; attempt: number; order: number; payload: Json; durability: string }
  | { kind: "item_started"; item_id: string; attempt: number; anchor_seq: number; durability: string }
  | { kind: "item_aborted"; item_id: string; attempt: number; reason: string; durability: string }
  | { kind: "lagged"; dropped: Json; resume_from_seq: number; durability: string }
  | { kind: "closed"; reason: ClosedReason; detail?: string; durability: string }
  | { kind: "rewind"; to_seq: number; to_event_id: string; reason: string; durability: string }

export function frameToJson(v: Frame): Json {
  const o: JsonObj = {};
  switch (v["kind"]) {
    case "durable": {
      o["kind"] = "durable";
      o["seq"] = v.seq;
      o["hash"] = v.hash;
      o["event"] = v.event;
      o["durability"] = v.durability;
      return o;
    }
    case "sync": {
      o["kind"] = "sync";
      o["from_seq"] = v.from_seq;
      o["through_seq"] = v.through_seq;
      o["head_hash"] = v.head_hash;
      o["durability"] = v.durability;
      return o;
    }
    case "ephemeral": {
      o["kind"] = "ephemeral";
      o["ephemeral_kind"] = ephemeralKindToJson(v.ephemeral_kind);
      o["scope"] = v.scope;
      o["attempt"] = v.attempt;
      o["order"] = v.order;
      o["payload"] = v.payload;
      o["durability"] = v.durability;
      return o;
    }
    case "item_started": {
      o["kind"] = "item_started";
      o["item_id"] = v.item_id;
      o["attempt"] = v.attempt;
      o["anchor_seq"] = v.anchor_seq;
      o["durability"] = v.durability;
      return o;
    }
    case "item_aborted": {
      o["kind"] = "item_aborted";
      o["item_id"] = v.item_id;
      o["attempt"] = v.attempt;
      o["reason"] = v.reason;
      o["durability"] = v.durability;
      return o;
    }
    case "lagged": {
      o["kind"] = "lagged";
      o["dropped"] = v.dropped;
      o["resume_from_seq"] = v.resume_from_seq;
      o["durability"] = v.durability;
      return o;
    }
    case "closed": {
      o["kind"] = "closed";
      o["reason"] = closedReasonToJson(v.reason);
      if (v.detail !== undefined) o["detail"] = v.detail;
      o["durability"] = v.durability;
      return o;
    }
    case "rewind": {
      o["kind"] = "rewind";
      o["to_seq"] = v.to_seq;
      o["to_event_id"] = v.to_event_id;
      o["reason"] = v.reason;
      o["durability"] = v.durability;
      return o;
    }
  }
}

export function frameFromJson(v: Json): Frame {
  const o = needObj(v);
  const tag = needStr(need(o, "kind"));
  switch (tag) {
    case "durable": return { kind: "durable", seq: needInt(need(o, "seq")), hash: needStr(need(o, "hash")), event: need(o, "event"), durability: needStr(need(o, "durability")) };
    case "sync": return { kind: "sync", from_seq: needInt(need(o, "from_seq")), through_seq: needInt(need(o, "through_seq")), head_hash: needStr(need(o, "head_hash")), durability: needStr(need(o, "durability")) };
    case "ephemeral": return { kind: "ephemeral", ephemeral_kind: ephemeralKindFromJson(need(o, "ephemeral_kind")), scope: need(o, "scope"), attempt: needInt(need(o, "attempt")), order: needInt(need(o, "order")), payload: need(o, "payload"), durability: needStr(need(o, "durability")) };
    case "item_started": return { kind: "item_started", item_id: needStr(need(o, "item_id")), attempt: needInt(need(o, "attempt")), anchor_seq: needInt(need(o, "anchor_seq")), durability: needStr(need(o, "durability")) };
    case "item_aborted": return { kind: "item_aborted", item_id: needStr(need(o, "item_id")), attempt: needInt(need(o, "attempt")), reason: needStr(need(o, "reason")), durability: needStr(need(o, "durability")) };
    case "lagged": return { kind: "lagged", dropped: need(o, "dropped"), resume_from_seq: needInt(need(o, "resume_from_seq")), durability: needStr(need(o, "durability")) };
    case "closed": return { kind: "closed", reason: closedReasonFromJson(need(o, "reason")), detail: o["detail"] === undefined ? undefined : needStr(o["detail"]), durability: needStr(need(o, "durability")) };
    case "rewind": return { kind: "rewind", to_seq: needInt(need(o, "to_seq")), to_event_id: needStr(need(o, "to_event_id")), reason: needStr(need(o, "reason")), durability: needStr(need(o, "durability")) };
    default: return fail(`unknown Frame variant ${canon(tag)}`);
  }
}

/// The closed `hh-embed/1` error sum — decoded from `error.data`
/// (`kind` tag + members). A variant absent from the table is
/// impossible by construction (CC1; a response-position sum grows
/// only by a dialect bump).
export interface EmbedError {
  code: number;
  kind: string;
  retryable: boolean;
  message: string;
  /// The typed `error.data` members (`kind`/`retryable` re-keyed out).
  data: Json;
}

/// `kind` → code, for client-side construction in tests/fixtures.
export function errorCodeFor(kind: string): number | undefined {
  switch (kind) {
    case "NotInitialized": return 1001;
    case "ContractMajorUnsupported": return 1002;
    case "SchemaMismatch": return 1003;
    case "KernelBelowFloor": return 1004;
    case "ExperimentalRequired": return 1005;
    case "CapabilityNotDeclared": return 1006;
    case "UnknownField": return 1100;
    case "SchemaViolation": return 1101;
    case "SecretInPayload": return 1102;
    case "UnknownSession": return 1200;
    case "UnknownRun": return 1201;
    case "WouldBlock": return 1202;
    case "Draining": return 1203;
    case "SessionDetached": return 1204;
    case "DefinitionChanged": return 1205;
    case "TurnMismatch": return 1206;
    case "TurnActive": return 1207;
    case "InvalidDefinition": return 1300;
    case "UnresolvedRef": return 1301;
    case "AmbiguousVersion": return 1302;
    case "AuthorityViolation": return 1303;
    case "UnexpressibleSurface": return 1304;
    case "LinkError": return 1305;
    case "AuthorityWideningRequiresHuman": return 1306;
    case "InsufficientBudget": return 1400;
    case "UnbudgetedArm": return 1401;
    case "UnattendedRequiresInput": return 1402;
    case "Refused": return 1403;
    case "Unsupported": return 1404;
    case "UnknownEffect": return 1500;
    case "AlreadyDecided": return 1501;
    case "OptionNotOffered": return 1502;
    case "UnknownPermission": return 1503;
    case "Fenced": return 1504;
    case "EnvironmentUnavailable": return 1600;
    case "UnknownCapability": return 1601;
    case "UnknownCoordinate": return 1602;
    case "Overloaded": return 1700;
    case "Disconnected": return 1701;
    case "Timeout": return 1702;
    default: return undefined;
  }
}

/// Whether `kind` is retryable (safe to retry with the same
/// `idempotency_key`).
export function errorRetryable(kind: string): boolean {
  return kind === "WouldBlock" || kind === "Draining" || kind === "Overloaded" || kind === "Disconnected" || kind === "Timeout";
}

/// Decode a JSON-RPC `error` object into the typed `EmbedError`.
function decodeError(err: Json): EmbedError {
  const o = needObj(err);
  const data = isObj(o["data"]) ? o["data"] as JsonObj : {};
  const kind = typeof data["kind"] === "string" ? data["kind"] as string : "Unknown";
  const retryable = typeof data["retryable"] === "boolean" ? data["retryable"] as boolean : errorRetryable(kind);
  return {
    code: typeof o["code"] === "number" ? o["code"] as number : 0,
    kind,
    retryable,
    message: typeof o["message"] === "string" ? o["message"] as string : "",
    data: o["data"] === undefined ? null : o["data"],
  };
}

/// The binding-(c) client (the `local_network` surface transport —
/// §7.2; S4.10): the same canonical request/response contract over
/// loopback HTTP — `POST /hh-embed/1` is one JSON-RPC request →
/// response, and `GET /hh-embed/1/events?wait_ms=` drains `stream.frame`
/// + `upcall.*` notifications as NDJSON. It speaks the exact wire shape
/// `hharness-embed`'s (c) server emits (bearer + exact-Host + no
/// Origin/`Sec-Fetch-*`; the token never rides a URL — P9) and mirrors
/// the generated Rust `NetClient`'s deliberately dumb transport: one
/// `node:http` request per call (`Connection: close`), canonical bytes
/// and nothing else. `fetch` is deliberately not used — undici always
/// appends `Sec-Fetch-Mode`, which the gate's P3/P4 posture refuses.
export class NetClient {
  addr: string;
  token: string;
  next_id = 0;
  /** `stream.frame` notifications buffered by `drain`. */
  pending: StreamNotification[] = [];
  /** `upcall.*` notifications buffered by `drain`, `[method, params]`. */
  pending_upcalls: [string, Json][] = [];
  /** The negotiated `HelloResult` (set by `hello`). */
  hello_result?: HelloResult;

  /// The bound socket + capability token (`hh-kernel serve --http`
  /// prints both; `addr` is `127.0.0.1:port` or `[::1]:port`).
  constructor(addr: string, token: string) {
    this.addr = addr;
    this.token = token;
  }

  /// The bound authority — the `Host` the gate requires.
  authority(): string {
    return this.addr;
  }

  /// One HTTP/1.1 request → `(status, body)`. The `host` header is set
  /// to the bound authority verbatim (the gate compares it exactly, P3
  /// — `[::1]:port` keeps its brackets); `content-length` is set
  /// explicitly because the server's codec is Content-Length only (no
  /// chunked). A socket failure lowers to `transport`, never a
  /// fabricated result.
  roundTrip(method: string, path: string, body?: string): Promise<{ status: number; text: string }> {
    const i = this.addr.lastIndexOf(":");
    const host = this.addr.startsWith("[")
      ? this.addr.slice(1, this.addr.indexOf("]"))
      : this.addr.slice(0, i);
    const port = Number(this.addr.slice(i + 1));
    return new Promise((resolve, reject) => {
      const req = request(
        {
          hostname: host,
          port,
          path,
          method,
          agent: false,
          headers: {
            host: this.addr,
            connection: "close",
            authorization: `Bearer ${this.token}`,
            ...(body === undefined ? {} : { "content-type": "application/json", "content-length": String(Buffer.byteLength(body)) }),
          },
        },
        (res) => {
          res.setEncoding("utf8");
          let text = "";
          res.on("data", (chunk: string) => {
            text += chunk;
          });
          res.on("end", () => {
            resolve({ status: res.statusCode === undefined ? 0 : res.statusCode, text });
          });
        },
      );
      req.on("error", (e: Error) => {
        reject(ClientError.transport(e.message));
      });
      req.setTimeout(30_000, () => {
        req.destroy(ClientError.transport("request timeout"));
      });
      if (body !== undefined) req.write(body);
      req.end();
    });
  }

  /// `POST /hh-embed/1` — one canonical request → one canonical
  /// response (`result` or the decoded `EmbedError` sum).
  async call(method: string, params: Json): Promise<Json> {
    this.next_id += 1;
    const id = this.next_id;
    const req: JsonObj = { jsonrpc: "2.0", id, method, params };
    const res = await this.roundTrip("POST", "/hh-embed/1", canon(req));
    if (res.status === 401) throw ClientError.transport("401 unauthorized");
    if (res.status !== 200) throw ClientError.transport(`http ${res.status}`);
    const msg = needObj(JSON.parse(res.text) as Json);
    if (msg["id"] !== id) {
      throw ClientError.transport(`response id mismatch: got ${canon(msg["id"] === undefined ? null : msg["id"])} want ${id}`);
    }
    const err = msg["error"];
    if (err !== undefined) throw ClientError.rpcError(decodeError(err));
    const result = msg["result"];
    if (result === undefined) throw ClientError.transport("response has no result");
    return result;
  }

  /// `GET /hh-embed/1/events?wait_ms=` — the notification drain;
  /// `stream.frame` frames buffer on `pending`, `upcall.*` on
  /// `pending_upcalls` (same shapes as the Rust client's buffers).
  async drain(wait_ms: number): Promise<void> {
    const res = await this.roundTrip("GET", `/hh-embed/1/events?wait_ms=${wait_ms}`);
    if (res.status === 401) throw ClientError.transport("401 unauthorized");
    if (res.status !== 200) throw ClientError.transport(`http ${res.status}`);
    const body = res.text;
    for (const line of body.split("\n")) {
      const t = line.trim();
      if (t === "") continue;
      const msg = needObj(JSON.parse(t) as Json);
      const method = typeof msg["method"] === "string" ? msg["method"] as string : "";
      const params = msg["params"];
      if (method === "stream.frame") {
        if (params !== undefined) {
          try {
            this.pending.push(streamNotificationFromJson(params));
          } catch {
            // An undecodable frame is dropped, never fatal to the drain.
          }
        }
      } else if (method.startsWith("upcall.")) {
        this.pending_upcalls.push([method, params === undefined ? null : params]);
      }
    }
  }

  /// The buffered `stream.frame` notifications (oldest first).
  takeNotifications(): StreamNotification[] {
    const n = this.pending;
    this.pending = [];
    return n;
  }

  /// The buffered `upcall.*` notifications (arrival order).
  takeUpcalls(): [string, Json][] {
    const u = this.pending_upcalls;
    this.pending_upcalls = [];
    return u;
  }

  /// `account` → `AccountView` (see the contract registry).
  async account(params: AccountParams): Promise<AccountView> {
    const raw = await this.call("account", accountParamsToJson(params));
    return accountViewFromJson(raw);
  }

  /// `amend` → `Session` (see the contract registry).
  async amend(params: AmendParams): Promise<Session> {
    const raw = await this.call("amend", amendParamsToJson(params));
    return sessionFromJson(raw);
  }

  /// `audit_view` → `json` (see the contract registry).
  async audit_view(params: AuditViewParams): Promise<Json> {
    const raw = await this.call("audit_view", auditViewParamsToJson(params));
    return raw;
  }

  /// `branch.open` → `json` (see the contract registry).
  async branch_open(params: BranchOpenParams): Promise<Json> {
    const raw = await this.call("branch.open", branchOpenParamsToJson(params));
    return raw;
  }

  /// `cancel` → `Acknowledged` (see the contract registry).
  async cancel(params: CancelParams): Promise<Acknowledged> {
    const raw = await this.call("cancel", cancelParamsToJson(params));
    return acknowledgedFromJson(raw);
  }

  /// `close` → `Closed` (see the contract registry).
  async close(params: CloseParams): Promise<Closed> {
    const raw = await this.call("close", closeParamsToJson(params));
    return closedFromJson(raw);
  }

  /// `coherent_fork_points` → `json` (see the contract registry).
  async coherent_fork_points(params: CoherentForkPointsParams): Promise<Json> {
    const raw = await this.call("coherent_fork_points", coherentForkPointsParamsToJson(params));
    return raw;
  }

  /// `compensate` → `json` (see the contract registry).
  async compensate(params: CompensateParams): Promise<Json> {
    const raw = await this.call("compensate", compensateParamsToJson(params));
    return raw;
  }

  /// `continue_goal` → `json` (see the contract registry).
  async continue_goal(params: ContinueGoalParams): Promise<Json> {
    const raw = await this.call("continue_goal", continueGoalParamsToJson(params));
    return raw;
  }

  /// `counterfactual` → `CounterfactualResult` (see the contract registry).
  async counterfactual(params: CounterfactualParams): Promise<CounterfactualResult> {
    const raw = await this.call("counterfactual", counterfactualParamsToJson(params));
    return counterfactualResultFromJson(raw);
  }

  /// `describe` → `DescribeResult` (see the contract registry).
  async describe(params: DescribeParams): Promise<DescribeResult> {
    const raw = await this.call("describe", describeParamsToJson(params));
    return describeResultFromJson(raw);
  }

  /// `discard` → `json` (see the contract registry).
  async discard(params: DiscardParams): Promise<Json> {
    const raw = await this.call("discard", discardParamsToJson(params));
    return raw;
  }

  /// `env.attach` → `json` (see the contract registry).
  async env_attach(params: Json): Promise<Json> {
    const raw = await this.call("env.attach", params);
    return raw;
  }

  /// `env.close` → `json` (see the contract registry).
  async env_close(params: Json): Promise<Json> {
    const raw = await this.call("env.close", params);
    return raw;
  }

  /// `env.derive` → `json` (see the contract registry).
  async env_derive(params: Json): Promise<Json> {
    const raw = await this.call("env.derive", params);
    return raw;
  }

  /// `env.diff` → `json` (see the contract registry).
  async env_diff(params: Json): Promise<Json> {
    const raw = await this.call("env.diff", params);
    return raw;
  }

  /// `env.download` → `json` (see the contract registry).
  async env_download(params: Json): Promise<Json> {
    const raw = await this.call("env.download", params);
    return raw;
  }

  /// `env.open` → `json` (see the contract registry).
  async env_open(params: Json): Promise<Json> {
    const raw = await this.call("env.open", params);
    return raw;
  }

  /// `env.restore` → `json` (see the contract registry).
  async env_restore(params: Json): Promise<Json> {
    const raw = await this.call("env.restore", params);
    return raw;
  }

  /// `env.resume` → `json` (see the contract registry).
  async env_resume(params: Json): Promise<Json> {
    const raw = await this.call("env.resume", params);
    return raw;
  }

  /// `env.set_phase` → `json` (see the contract registry).
  async env_set_phase(params: Json): Promise<Json> {
    const raw = await this.call("env.set_phase", params);
    return raw;
  }

  /// `env.snapshot` → `json` (see the contract registry).
  async env_snapshot(params: Json): Promise<Json> {
    const raw = await this.call("env.snapshot", params);
    return raw;
  }

  /// `env.suspend` → `json` (see the contract registry).
  async env_suspend(params: Json): Promise<Json> {
    const raw = await this.call("env.suspend", params);
    return raw;
  }

  /// `env.upload` → `json` (see the contract registry).
  async env_upload(params: Json): Promise<Json> {
    const raw = await this.call("env.upload", params);
    return raw;
  }

  /// `fleet.accountability_record` → `json` (see the contract registry).
  async fleet_accountability_record(params: Json): Promise<Json> {
    const raw = await this.call("fleet.accountability_record", params);
    return raw;
  }

  /// `fleet.acknowledge_owner` → `json` (see the contract registry).
  async fleet_acknowledge_owner(params: Json): Promise<Json> {
    const raw = await this.call("fleet.acknowledge_owner", params);
    return raw;
  }

  /// `fleet.annotate` → `json` (see the contract registry).
  async fleet_annotate(params: Json): Promise<Json> {
    const raw = await this.call("fleet.annotate", params);
    return raw;
  }

  /// `fleet.audit_link` → `json` (see the contract registry).
  async fleet_audit_link(params: Json): Promise<Json> {
    const raw = await this.call("fleet.audit_link", params);
    return raw;
  }

  /// `fleet.bind_source` → `json` (see the contract registry).
  async fleet_bind_source(params: Json): Promise<Json> {
    const raw = await this.call("fleet.bind_source", params);
    return raw;
  }

  /// `fleet.block` → `json` (see the contract registry).
  async fleet_block(params: Json): Promise<Json> {
    const raw = await this.call("fleet.block", params);
    return raw;
  }

  /// `fleet.cancel` → `json` (see the contract registry).
  async fleet_cancel(params: Json): Promise<Json> {
    const raw = await this.call("fleet.cancel", params);
    return raw;
  }

  /// `fleet.check_activation_delta` → `json` (see the contract registry).
  async fleet_check_activation_delta(params: Json): Promise<Json> {
    const raw = await this.call("fleet.check_activation_delta", params);
    return raw;
  }

  /// `fleet.claim` → `json` (see the contract registry).
  async fleet_claim(params: Json): Promise<Json> {
    const raw = await this.call("fleet.claim", params);
    return raw;
  }

  /// `fleet.create_work_item` → `json` (see the contract registry).
  async fleet_create_work_item(params: Json): Promise<Json> {
    const raw = await this.call("fleet.create_work_item", params);
    return raw;
  }

  /// `fleet.dispatch` → `json` (see the contract registry).
  async fleet_dispatch(params: Json): Promise<Json> {
    const raw = await this.call("fleet.dispatch", params);
    return raw;
  }

  /// `fleet.dispatch_note` → `json` (see the contract registry).
  async fleet_dispatch_note(params: Json): Promise<Json> {
    const raw = await this.call("fleet.dispatch_note", params);
    return raw;
  }

  /// `fleet.ensure` → `json` (see the contract registry).
  async fleet_ensure(params: Json): Promise<Json> {
    const raw = await this.call("fleet.ensure", params);
    return raw;
  }

  /// `fleet.escalate` → `json` (see the contract registry).
  async fleet_escalate(params: Json): Promise<Json> {
    const raw = await this.call("fleet.escalate", params);
    return raw;
  }

  /// `fleet.fleet_view` → `json` (see the contract registry).
  async fleet_fleet_view(params: Json): Promise<Json> {
    const raw = await this.call("fleet.fleet_view", params);
    return raw;
  }

  /// `fleet.handoff` → `json` (see the contract registry).
  async fleet_handoff(params: Json): Promise<Json> {
    const raw = await this.call("fleet.handoff", params);
    return raw;
  }

  /// `fleet.list` → `json` (see the contract registry).
  async fleet_list(params: Json): Promise<Json> {
    const raw = await this.call("fleet.list", params);
    return raw;
  }

  /// `fleet.observe` → `json` (see the contract registry).
  async fleet_observe(params: Json): Promise<Json> {
    const raw = await this.call("fleet.observe", params);
    return raw;
  }

  /// `fleet.open` → `json` (see the contract registry).
  async fleet_open(params: Json): Promise<Json> {
    const raw = await this.call("fleet.open", params);
    return raw;
  }

  /// `fleet.reconcile` → `json` (see the contract registry).
  async fleet_reconcile(params: Json): Promise<Json> {
    const raw = await this.call("fleet.reconcile", params);
    return raw;
  }

  /// `fleet.resolve_escalation` → `json` (see the contract registry).
  async fleet_resolve_escalation(params: Json): Promise<Json> {
    const raw = await this.call("fleet.resolve_escalation", params);
    return raw;
  }

  /// `fleet.restore` → `json` (see the contract registry).
  async fleet_restore(params: Json): Promise<Json> {
    const raw = await this.call("fleet.restore", params);
    return raw;
  }

  /// `fleet.resume_from_handoff` → `json` (see the contract registry).
  async fleet_resume_from_handoff(params: Json): Promise<Json> {
    const raw = await this.call("fleet.resume_from_handoff", params);
    return raw;
  }

  /// `fleet.set_owner` → `json` (see the contract registry).
  async fleet_set_owner(params: Json): Promise<Json> {
    const raw = await this.call("fleet.set_owner", params);
    return raw;
  }

  /// `fleet.settle` → `json` (see the contract registry).
  async fleet_settle(params: Json): Promise<Json> {
    const raw = await this.call("fleet.settle", params);
    return raw;
  }

  /// `fleet.source_capabilities` → `json` (see the contract registry).
  async fleet_source_capabilities(params: Json): Promise<Json> {
    const raw = await this.call("fleet.source_capabilities", params);
    return raw;
  }

  /// `fleet.source_records` → `json` (see the contract registry).
  async fleet_source_records(params: Json): Promise<Json> {
    const raw = await this.call("fleet.source_records", params);
    return raw;
  }

  /// `fleet.state_map` → `json` (see the contract registry).
  async fleet_state_map(params: Json): Promise<Json> {
    const raw = await this.call("fleet.state_map", params);
    return raw;
  }

  /// `fleet.stop` → `json` (see the contract registry).
  async fleet_stop(params: Json): Promise<Json> {
    const raw = await this.call("fleet.stop", params);
    return raw;
  }

  /// `fleet.transfer_owner` → `json` (see the contract registry).
  async fleet_transfer_owner(params: Json): Promise<Json> {
    const raw = await this.call("fleet.transfer_owner", params);
    return raw;
  }

  /// `fleet.unblock` → `json` (see the contract registry).
  async fleet_unblock(params: Json): Promise<Json> {
    const raw = await this.call("fleet.unblock", params);
    return raw;
  }

  /// `fleet.webhook_ingress` → `json` (see the contract registry).
  async fleet_webhook_ingress(params: Json): Promise<Json> {
    const raw = await this.call("fleet.webhook_ingress", params);
    return raw;
  }

  /// `fleet.work_item` → `json` (see the contract registry).
  async fleet_work_item(params: Json): Promise<Json> {
    const raw = await this.call("fleet.work_item", params);
    return raw;
  }

  /// `fork` → `Session` (see the contract registry).
  async fork(params: ForkParams): Promise<Session> {
    const raw = await this.call("fork", forkParamsToJson(params));
    return sessionFromJson(raw);
  }

  /// `get_artifact` → `json` (see the contract registry).
  async get_artifact(params: GetArtifactParams): Promise<Json> {
    const raw = await this.call("get_artifact", getArtifactParamsToJson(params));
    return raw;
  }

  /// `head` → `Head` (see the contract registry).
  async head(params: HeadParams): Promise<Head> {
    const raw = await this.call("head", headParamsToJson(params));
    return headFromJson(raw);
  }

  /// `heal` → `json` (see the contract registry).
  async heal(params: HealParams): Promise<Json> {
    const raw = await this.call("heal", healParamsToJson(params));
    return raw;
  }

  /// `hello` → `HelloResult` (see the contract registry).
  async hello(params: HelloParams): Promise<HelloResult> {
    const raw = await this.call("hello", helloParamsToJson(params));
    const r = helloResultFromJson(raw); if (r.kernel.contract_major !== CONTRACT_MAJOR) throw new ClientError("identity_mismatch", "contract_major mismatch", undefined, "contract_major"); if (r.kernel.schema_hash !== EXPECTED_SCHEMA_HASH) throw new ClientError("identity_mismatch", "schema_hash mismatch", undefined, "schema_hash"); this.hello_result = r; return r;
  }

  /// `kernel.attest` → `json` (see the contract registry).
  async kernel_attest(params: Json): Promise<Json> {
    const raw = await this.call("kernel.attest", params);
    return raw;
  }

  /// `kernel.audit_bundle` → `json` (see the contract registry).
  async kernel_audit_bundle(params: Json): Promise<Json> {
    const raw = await this.call("kernel.audit_bundle", params);
    return raw;
  }

  /// `kernel.bundle` → `json` (see the contract registry).
  async kernel_bundle(params: Json): Promise<Json> {
    const raw = await this.call("kernel.bundle", params);
    return raw;
  }

  /// `kernel.check_completeness` → `json` (see the contract registry).
  async kernel_check_completeness(params: Json): Promise<Json> {
    const raw = await this.call("kernel.check_completeness", params);
    return raw;
  }

  /// `kernel.diff` → `json` (see the contract registry).
  async kernel_diff(params: Json): Promise<Json> {
    const raw = await this.call("kernel.diff", params);
    return raw;
  }

  /// `kernel.export` → `json` (see the contract registry).
  async kernel_export(params: Json): Promise<Json> {
    const raw = await this.call("kernel.export", params);
    return raw;
  }

  /// `kernel.fetch` → `json` (see the contract registry).
  async kernel_fetch(params: Json): Promise<Json> {
    const raw = await this.call("kernel.fetch", params);
    return raw;
  }

  /// `kernel.import` → `json` (see the contract registry).
  async kernel_import(params: Json): Promise<Json> {
    const raw = await this.call("kernel.import", params);
    return raw;
  }

  /// `kernel.lineage` → `json` (see the contract registry).
  async kernel_lineage(params: Json): Promise<Json> {
    const raw = await this.call("kernel.lineage", params);
    return raw;
  }

  /// `kernel.migrate` → `json` (see the contract registry).
  async kernel_migrate(params: Json): Promise<Json> {
    const raw = await this.call("kernel.migrate", params);
    return raw;
  }

  /// `kernel.reproduce` → `json` (see the contract registry).
  async kernel_reproduce(params: Json): Promise<Json> {
    const raw = await this.call("kernel.reproduce", params);
    return raw;
  }

  /// `kernel.set_status` → `json` (see the contract registry).
  async kernel_set_status(params: Json): Promise<Json> {
    const raw = await this.call("kernel.set_status", params);
    return raw;
  }

  /// `kernel.status` → `json` (see the contract registry).
  async kernel_status(params: Json): Promise<Json> {
    const raw = await this.call("kernel.status", params);
    return raw;
  }

  /// `kernel.supersede` → `json` (see the contract registry).
  async kernel_supersede(params: Json): Promise<Json> {
    const raw = await this.call("kernel.supersede", params);
    return raw;
  }

  /// `kernel.validate` → `json` (see the contract registry).
  async kernel_validate(params: Json): Promise<Json> {
    const raw = await this.call("kernel.validate", params);
    return raw;
  }

  /// `lab.analysis.analyze` → `json` (see the contract registry).
  async lab_analysis_analyze(params: Json): Promise<Json> {
    const raw = await this.call("lab.analysis.analyze", params);
    return raw;
  }

  /// `lab.analysis.attribution_design` → `json` (see the contract registry).
  async lab_analysis_attribution_design(params: Json): Promise<Json> {
    const raw = await this.call("lab.analysis.attribution_design", params);
    return raw;
  }

  /// `lab.analysis.component_targets` → `json` (see the contract registry).
  async lab_analysis_component_targets(params: Json): Promise<Json> {
    const raw = await this.call("lab.analysis.component_targets", params);
    return raw;
  }

  /// `lab.analysis.diff_reports` → `json` (see the contract registry).
  async lab_analysis_diff_reports(params: Json): Promise<Json> {
    const raw = await this.call("lab.analysis.diff_reports", params);
    return raw;
  }

  /// `lab.analysis.power` → `json` (see the contract registry).
  async lab_analysis_power(params: Json): Promise<Json> {
    const raw = await this.call("lab.analysis.power", params);
    return raw;
  }

  /// `lab.analysis.render` → `json` (see the contract registry).
  async lab_analysis_render(params: Json): Promise<Json> {
    const raw = await this.call("lab.analysis.render", params);
    return raw;
  }

  /// `lab.assembly.adopt` → `json` (see the contract registry).
  async lab_assembly_adopt(params: Json): Promise<Json> {
    const raw = await this.call("lab.assembly.adopt", params);
    return raw;
  }

  /// `lab.assembly.apply` → `json` (see the contract registry).
  async lab_assembly_apply(params: Json): Promise<Json> {
    const raw = await this.call("lab.assembly.apply", params);
    return raw;
  }

  /// `lab.assembly.assemble` → `json` (see the contract registry).
  async lab_assembly_assemble(params: Json): Promise<Json> {
    const raw = await this.call("lab.assembly.assemble", params);
    return raw;
  }

  /// `lab.assembly.compile` → `json` (see the contract registry).
  async lab_assembly_compile(params: Json): Promise<Json> {
    const raw = await this.call("lab.assembly.compile", params);
    return raw;
  }

  /// `lab.assembly.diff` → `json` (see the contract registry).
  async lab_assembly_diff(params: Json): Promise<Json> {
    const raw = await this.call("lab.assembly.diff", params);
    return raw;
  }

  /// `lab.assembly.drift` → `json` (see the contract registry).
  async lab_assembly_drift(params: Json): Promise<Json> {
    const raw = await this.call("lab.assembly.drift", params);
    return raw;
  }

  /// `lab.assembly.explain` → `json` (see the contract registry).
  async lab_assembly_explain(params: Json): Promise<Json> {
    const raw = await this.call("lab.assembly.explain", params);
    return raw;
  }

  /// `lab.assembly.identity` → `json` (see the contract registry).
  async lab_assembly_identity(params: Json): Promise<Json> {
    const raw = await this.call("lab.assembly.identity", params);
    return raw;
  }

  /// `lab.assembly.plan` → `json` (see the contract registry).
  async lab_assembly_plan(params: Json): Promise<Json> {
    const raw = await this.call("lab.assembly.plan", params);
    return raw;
  }

  /// `lab.assembly.validate_batch` → `json` (see the contract registry).
  async lab_assembly_validate_batch(params: Json): Promise<Json> {
    const raw = await this.call("lab.assembly.validate_batch", params);
    return raw;
  }

  /// `lab.attribution.attribute` → `json` (see the contract registry).
  async lab_attribution_attribute(params: Json): Promise<Json> {
    const raw = await this.call("lab.attribution.attribute", params);
    return raw;
  }

  /// `lab.attribution.design` → `json` (see the contract registry).
  async lab_attribution_design(params: Json): Promise<Json> {
    const raw = await this.call("lab.attribution.design", params);
    return raw;
  }

  /// `lab.attribution.locus` → `json` (see the contract registry).
  async lab_attribution_locus(params: Json): Promise<Json> {
    const raw = await this.call("lab.attribution.locus", params);
    return raw;
  }

  /// `lab.attribution.quality` → `json` (see the contract registry).
  async lab_attribution_quality(params: Json): Promise<Json> {
    const raw = await this.call("lab.attribution.quality", params);
    return raw;
  }

  /// `lab.coevolution.consolidation_candidates` → `json` (see the contract registry).
  async lab_coevolution_consolidation_candidates(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.consolidation_candidates", params);
    return raw;
  }

  /// `lab.coevolution.consolidation_retirement_record` → `json` (see the contract registry).
  async lab_coevolution_consolidation_retirement_record(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.consolidation_retirement_record", params);
    return raw;
  }

  /// `lab.coevolution.cycle_begin_phase` → `json` (see the contract registry).
  async lab_coevolution_cycle_begin_phase(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.cycle_begin_phase", params);
    return raw;
  }

  /// `lab.coevolution.cycle_complete_phase` → `json` (see the contract registry).
  async lab_coevolution_cycle_complete_phase(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.cycle_complete_phase", params);
    return raw;
  }

  /// `lab.coevolution.cycle_next` → `json` (see the contract registry).
  async lab_coevolution_cycle_next(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.cycle_next", params);
    return raw;
  }

  /// `lab.coevolution.cycle_open` → `json` (see the contract registry).
  async lab_coevolution_cycle_open(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.cycle_open", params);
    return raw;
  }

  /// `lab.coevolution.cycle_record` → `json` (see the contract registry).
  async lab_coevolution_cycle_record(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.cycle_record", params);
    return raw;
  }

  /// `lab.coevolution.cycle_stop` → `json` (see the contract registry).
  async lab_coevolution_cycle_stop(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.cycle_stop", params);
    return raw;
  }

  /// `lab.coevolution.export_compatibility_tags` → `json` (see the contract registry).
  async lab_coevolution_export_compatibility_tags(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.export_compatibility_tags", params);
    return raw;
  }

  /// `lab.coevolution.export_regression_suite` → `json` (see the contract registry).
  async lab_coevolution_export_regression_suite(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.export_regression_suite", params);
    return raw;
  }

  /// `lab.coevolution.export_training` → `json` (see the contract registry).
  async lab_coevolution_export_training(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.export_training", params);
    return raw;
  }

  /// `lab.coevolution.guard_at_bind` → `json` (see the contract registry).
  async lab_coevolution_guard_at_bind(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.guard_at_bind", params);
    return raw;
  }

  /// `lab.coevolution.import_snapshot` → `json` (see the contract registry).
  async lab_coevolution_import_snapshot(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.import_snapshot", params);
    return raw;
  }

  /// `lab.coevolution.propose_consolidation` → `json` (see the contract registry).
  async lab_coevolution_propose_consolidation(params: Json): Promise<Json> {
    const raw = await this.call("lab.coevolution.propose_consolidation", params);
    return raw;
  }

  /// `lab.debt.evaluate` → `json` (see the contract registry).
  async lab_debt_evaluate(params: Json): Promise<Json> {
    const raw = await this.call("lab.debt.evaluate", params);
    return raw;
  }

  /// `lab.debt.index` → `json` (see the contract registry).
  async lab_debt_index(params: Json): Promise<Json> {
    const raw = await this.call("lab.debt.index", params);
    return raw;
  }

  /// `lab.debt.manager_open` → `json` (see the contract registry).
  async lab_debt_manager_open(params: Json): Promise<Json> {
    const raw = await this.call("lab.debt.manager_open", params);
    return raw;
  }

  /// `lab.debt.post_import_sweep` → `json` (see the contract registry).
  async lab_debt_post_import_sweep(params: Json): Promise<Json> {
    const raw = await this.call("lab.debt.post_import_sweep", params);
    return raw;
  }

  /// `lab.debt.propose` → `json` (see the contract registry).
  async lab_debt_propose(params: Json): Promise<Json> {
    const raw = await this.call("lab.debt.propose", params);
    return raw;
  }

  /// `lab.debt.register` → `json` (see the contract registry).
  async lab_debt_register(params: Json): Promise<Json> {
    const raw = await this.call("lab.debt.register", params);
    return raw;
  }

  /// `lab.debt.report` → `json` (see the contract registry).
  async lab_debt_report(params: Json): Promise<Json> {
    const raw = await this.call("lab.debt.report", params);
    return raw;
  }

  /// `lab.debt.retire` → `json` (see the contract registry).
  async lab_debt_retire(params: Json): Promise<Json> {
    const raw = await this.call("lab.debt.retire", params);
    return raw;
  }

  /// `lab.debt.settle` → `json` (see the contract registry).
  async lab_debt_settle(params: Json): Promise<Json> {
    const raw = await this.call("lab.debt.settle", params);
    return raw;
  }

  /// `lab.debt.sweep` → `json` (see the contract registry).
  async lab_debt_sweep(params: Json): Promise<Json> {
    const raw = await this.call("lab.debt.sweep", params);
    return raw;
  }

  /// `lab.eval.catalogue` → `json` (see the contract registry).
  async lab_eval_catalogue(params: Json): Promise<Json> {
    const raw = await this.call("lab.eval.catalogue", params);
    return raw;
  }

  /// `lab.eval.compare` → `json` (see the contract registry).
  async lab_eval_compare(params: Json): Promise<Json> {
    const raw = await this.call("lab.eval.compare", params);
    return raw;
  }

  /// `lab.eval.equivalence_run` → `json` (see the contract registry).
  async lab_eval_equivalence_run(params: Json): Promise<Json> {
    const raw = await this.call("lab.eval.equivalence_run", params);
    return raw;
  }

  /// `lab.eval.loss_report` → `json` (see the contract registry).
  async lab_eval_loss_report(params: Json): Promise<Json> {
    const raw = await this.call("lab.eval.loss_report", params);
    return raw;
  }

  /// `lab.eval.render_scorecard` → `json` (see the contract registry).
  async lab_eval_render_scorecard(params: Json): Promise<Json> {
    const raw = await this.call("lab.eval.render_scorecard", params);
    return raw;
  }

  /// `lab.evolution.campaign_ensure` → `json` (see the contract registry).
  async lab_evolution_campaign_ensure(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.campaign_ensure", params);
    return raw;
  }

  /// `lab.evolution.campaign_open` → `json` (see the contract registry).
  async lab_evolution_campaign_open(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.campaign_open", params);
    return raw;
  }

  /// `lab.evolution.canary` → `json` (see the contract registry).
  async lab_evolution_canary(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.canary", params);
    return raw;
  }

  /// `lab.evolution.canary_settle` → `json` (see the contract registry).
  async lab_evolution_canary_settle(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.canary_settle", params);
    return raw;
  }

  /// `lab.evolution.close` → `json` (see the contract registry).
  async lab_evolution_close(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.close", params);
    return raw;
  }

  /// `lab.evolution.expire` → `json` (see the contract registry).
  async lab_evolution_expire(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.expire", params);
    return raw;
  }

  /// `lab.evolution.held_out_eval` → `json` (see the contract registry).
  async lab_evolution_held_out_eval(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.held_out_eval", params);
    return raw;
  }

  /// `lab.evolution.hypothesize` → `json` (see the contract registry).
  async lab_evolution_hypothesize(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.hypothesize", params);
    return raw;
  }

  /// `lab.evolution.lineage` → `json` (see the contract registry).
  async lab_evolution_lineage(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.lineage", params);
    return raw;
  }

  /// `lab.evolution.matched_eval` → `json` (see the contract registry).
  async lab_evolution_matched_eval(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.matched_eval", params);
    return raw;
  }

  /// `lab.evolution.monitor_assign` → `json` (see the contract registry).
  async lab_evolution_monitor_assign(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.monitor_assign", params);
    return raw;
  }

  /// `lab.evolution.monitor_veto` → `json` (see the contract registry).
  async lab_evolution_monitor_veto(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.monitor_veto", params);
    return raw;
  }

  /// `lab.evolution.propose` → `json` (see the contract registry).
  async lab_evolution_propose(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.propose", params);
    return raw;
  }

  /// `lab.evolution.reactivate` → `json` (see the contract registry).
  async lab_evolution_reactivate(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.reactivate", params);
    return raw;
  }

  /// `lab.evolution.rebase` → `json` (see the contract registry).
  async lab_evolution_rebase(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.rebase", params);
    return raw;
  }

  /// `lab.evolution.retire` → `json` (see the contract registry).
  async lab_evolution_retire(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.retire", params);
    return raw;
  }

  /// `lab.evolution.revalidate` → `json` (see the contract registry).
  async lab_evolution_revalidate(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.revalidate", params);
    return raw;
  }

  /// `lab.evolution.revert` → `json` (see the contract registry).
  async lab_evolution_revert(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.revert", params);
    return raw;
  }

  /// `lab.evolution.screen` → `json` (see the contract registry).
  async lab_evolution_screen(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.screen", params);
    return raw;
  }

  /// `lab.evolution.seal` → `json` (see the contract registry).
  async lab_evolution_seal(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.seal", params);
    return raw;
  }

  /// `lab.evolution.security_check` → `json` (see the contract registry).
  async lab_evolution_security_check(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.security_check", params);
    return raw;
  }

  /// `lab.evolution.stop` → `json` (see the contract registry).
  async lab_evolution_stop(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.stop", params);
    return raw;
  }

  /// `lab.evolution.transfer` → `json` (see the contract registry).
  async lab_evolution_transfer(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.transfer", params);
    return raw;
  }

  /// `lab.evolution.view` → `json` (see the contract registry).
  async lab_evolution_view(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.view", params);
    return raw;
  }

  /// `lab.evolution.withdraw` → `json` (see the contract registry).
  async lab_evolution_withdraw(params: Json): Promise<Json> {
    const raw = await this.call("lab.evolution.withdraw", params);
    return raw;
  }

  /// `lab.experiment.claim` → `json` (see the contract registry).
  async lab_experiment_claim(params: Json): Promise<Json> {
    const raw = await this.call("lab.experiment.claim", params);
    return raw;
  }

  /// `lab.experiment.close` → `json` (see the contract registry).
  async lab_experiment_close(params: Json): Promise<Json> {
    const raw = await this.call("lab.experiment.close", params);
    return raw;
  }

  /// `lab.experiment.expand` → `json` (see the contract registry).
  async lab_experiment_expand(params: Json): Promise<Json> {
    const raw = await this.call("lab.experiment.expand", params);
    return raw;
  }

  /// `lab.experiment.launch` → `json` (see the contract registry).
  async lab_experiment_launch(params: Json): Promise<Json> {
    const raw = await this.call("lab.experiment.launch", params);
    return raw;
  }

  /// `lab.experiment.next` → `json` (see the contract registry).
  async lab_experiment_next(params: Json): Promise<Json> {
    const raw = await this.call("lab.experiment.next", params);
    return raw;
  }

  /// `lab.experiment.open_experiment` → `json` (see the contract registry).
  async lab_experiment_open_experiment(params: Json): Promise<Json> {
    const raw = await this.call("lab.experiment.open_experiment", params);
    return raw;
  }

  /// `lab.experiment.pause` → `json` (see the contract registry).
  async lab_experiment_pause(params: Json): Promise<Json> {
    const raw = await this.call("lab.experiment.pause", params);
    return raw;
  }

  /// `lab.experiment.register` → `json` (see the contract registry).
  async lab_experiment_register(params: Json): Promise<Json> {
    const raw = await this.call("lab.experiment.register", params);
    return raw;
  }

  /// `lab.experiment.resume` → `json` (see the contract registry).
  async lab_experiment_resume(params: Json): Promise<Json> {
    const raw = await this.call("lab.experiment.resume", params);
    return raw;
  }

  /// `lab.experiment.settle` → `json` (see the contract registry).
  async lab_experiment_settle(params: Json): Promise<Json> {
    const raw = await this.call("lab.experiment.settle", params);
    return raw;
  }

  /// `lab.extension.activate` → `json` (see the contract registry).
  async lab_extension_activate(params: Json): Promise<Json> {
    const raw = await this.call("lab.extension.activate", params);
    return raw;
  }

  /// `lab.extension.check_surface` → `json` (see the contract registry).
  async lab_extension_check_surface(params: Json): Promise<Json> {
    const raw = await this.call("lab.extension.check_surface", params);
    return raw;
  }

  /// `lab.extension.discover` → `json` (see the contract registry).
  async lab_extension_discover(params: Json): Promise<Json> {
    const raw = await this.call("lab.extension.discover", params);
    return raw;
  }

  /// `lab.extension.export_plugin` → `json` (see the contract registry).
  async lab_extension_export_plugin(params: Json): Promise<Json> {
    const raw = await this.call("lab.extension.export_plugin", params);
    return raw;
  }

  /// `lab.extension.import_plugin` → `json` (see the contract registry).
  async lab_extension_import_plugin(params: Json): Promise<Json> {
    const raw = await this.call("lab.extension.import_plugin", params);
    return raw;
  }

  /// `lab.extension.install` → `json` (see the contract registry).
  async lab_extension_install(params: Json): Promise<Json> {
    const raw = await this.call("lab.extension.install", params);
    return raw;
  }

  /// `lab.extension.resolve` → `json` (see the contract registry).
  async lab_extension_resolve(params: Json): Promise<Json> {
    const raw = await this.call("lab.extension.resolve", params);
    return raw;
  }

  /// `lab.extension.review` → `json` (see the contract registry).
  async lab_extension_review(params: Json): Promise<Json> {
    const raw = await this.call("lab.extension.review", params);
    return raw;
  }

  /// `lab.extension.revoke` → `json` (see the contract registry).
  async lab_extension_revoke(params: Json): Promise<Json> {
    const raw = await this.call("lab.extension.revoke", params);
    return raw;
  }

  /// `lab.extension.scan` → `json` (see the contract registry).
  async lab_extension_scan(params: Json): Promise<Json> {
    const raw = await this.call("lab.extension.scan", params);
    return raw;
  }

  /// `lab.extension.seal` → `json` (see the contract registry).
  async lab_extension_seal(params: Json): Promise<Json> {
    const raw = await this.call("lab.extension.seal", params);
    return raw;
  }

  /// `lab.extension.update` → `json` (see the contract registry).
  async lab_extension_update(params: Json): Promise<Json> {
    const raw = await this.call("lab.extension.update", params);
    return raw;
  }

  /// `lab.hosting.attach` → `json` (see the contract registry).
  async lab_hosting_attach(params: Json): Promise<Json> {
    const raw = await this.call("lab.hosting.attach", params);
    return raw;
  }

  /// `lab.hosting.describe` → `json` (see the contract registry).
  async lab_hosting_describe(params: Json): Promise<Json> {
    const raw = await this.call("lab.hosting.describe", params);
    return raw;
  }

  /// `lab.hosting.probe` → `json` (see the contract registry).
  async lab_hosting_probe(params: Json): Promise<Json> {
    const raw = await this.call("lab.hosting.probe", params);
    return raw;
  }

  /// `lab.leaderboard.define` → `json` (see the contract registry).
  async lab_leaderboard_define(params: Json): Promise<Json> {
    const raw = await this.call("lab.leaderboard.define", params);
    return raw;
  }

  /// `lab.leaderboard.diff_snapshots` → `json` (see the contract registry).
  async lab_leaderboard_diff_snapshots(params: Json): Promise<Json> {
    const raw = await this.call("lab.leaderboard.diff_snapshots", params);
    return raw;
  }

  /// `lab.leaderboard.leaderboard` → `json` (see the contract registry).
  async lab_leaderboard_leaderboard(params: Json): Promise<Json> {
    const raw = await this.call("lab.leaderboard.leaderboard", params);
    return raw;
  }

  /// `lab.leaderboard.publish` → `json` (see the contract registry).
  async lab_leaderboard_publish(params: Json): Promise<Json> {
    const raw = await this.call("lab.leaderboard.publish", params);
    return raw;
  }

  /// `lab.leaderboard.retract_entry` → `json` (see the contract registry).
  async lab_leaderboard_retract_entry(params: Json): Promise<Json> {
    const raw = await this.call("lab.leaderboard.retract_entry", params);
    return raw;
  }

  /// `lab.leaderboard.snapshots` → `json` (see the contract registry).
  async lab_leaderboard_snapshots(params: Json): Promise<Json> {
    const raw = await this.call("lab.leaderboard.snapshots", params);
    return raw;
  }

  /// `lab.model.regression` → `json` (see the contract registry).
  async lab_model_regression(params: Json): Promise<Json> {
    const raw = await this.call("lab.model.regression", params);
    return raw;
  }

  /// `lab.model.snapshot_claim` → `json` (see the contract registry).
  async lab_model_snapshot_claim(params: Json): Promise<Json> {
    const raw = await this.call("lab.model.snapshot_claim", params);
    return raw;
  }

  /// `lab.org_policy.default_removal_tests` → `json` (see the contract registry).
  async lab_org_policy_default_removal_tests(params: Json): Promise<Json> {
    const raw = await this.call("lab.org_policy.default_removal_tests", params);
    return raw;
  }

  /// `lab.org_policy.recipe` → `json` (see the contract registry).
  async lab_org_policy_recipe(params: Json): Promise<Json> {
    const raw = await this.call("lab.org_policy.recipe", params);
    return raw;
  }

  /// `lab.permission.grant_approver` → `json` (see the contract registry).
  async lab_permission_grant_approver(params: Json): Promise<Json> {
    const raw = await this.call("lab.permission.grant_approver", params);
    return raw;
  }

  /// `lab.permission.revoke_approver` → `json` (see the contract registry).
  async lab_permission_revoke_approver(params: Json): Promise<Json> {
    const raw = await this.call("lab.permission.revoke_approver", params);
    return raw;
  }

  /// `lab.producer.amend` → `json` (see the contract registry).
  async lab_producer_amend(params: Json): Promise<Json> {
    const raw = await this.call("lab.producer.amend", params);
    return raw;
  }

  /// `lab.producer.bind` → `json` (see the contract registry).
  async lab_producer_bind(params: Json): Promise<Json> {
    const raw = await this.call("lab.producer.bind", params);
    return raw;
  }

  /// `lab.producer.declare` → `json` (see the contract registry).
  async lab_producer_declare(params: Json): Promise<Json> {
    const raw = await this.call("lab.producer.declare", params);
    return raw;
  }

  /// `lab.producer.exclude` → `json` (see the contract registry).
  async lab_producer_exclude(params: Json): Promise<Json> {
    const raw = await this.call("lab.producer.exclude", params);
    return raw;
  }

  /// `lab.producer.record_analysis` → `json` (see the contract registry).
  async lab_producer_record_analysis(params: Json): Promise<Json> {
    const raw = await this.call("lab.producer.record_analysis", params);
    return raw;
  }

  /// `lab.receipt.record` → `json` (see the contract registry).
  async lab_receipt_record(params: Json): Promise<Json> {
    const raw = await this.call("lab.receipt.record", params);
    return raw;
  }

  /// `lab.registry.catalog` → `json` (see the contract registry).
  async lab_registry_catalog(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.catalog", params);
    return raw;
  }

  /// `lab.registry.deprecate` → `json` (see the contract registry).
  async lab_registry_deprecate(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.deprecate", params);
    return raw;
  }

  /// `lab.registry.export` → `json` (see the contract registry).
  async lab_registry_export(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.export", params);
    return raw;
  }

  /// `lab.registry.import` → `json` (see the contract registry).
  async lab_registry_import(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.import", params);
    return raw;
  }

  /// `lab.registry.lineage` → `json` (see the contract registry).
  async lab_registry_lineage(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.lineage", params);
    return raw;
  }

  /// `lab.registry.pin` → `json` (see the contract registry).
  async lab_registry_pin(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.pin", params);
    return raw;
  }

  /// `lab.registry.publish` → `json` (see the contract registry).
  async lab_registry_publish(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.publish", params);
    return raw;
  }

  /// `lab.registry.publisher_claims` → `json` (see the contract registry).
  async lab_registry_publisher_claims(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.publisher_claims", params);
    return raw;
  }

  /// `lab.registry.query` → `json` (see the contract registry).
  async lab_registry_query(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.query", params);
    return raw;
  }

  /// `lab.registry.record_conformance` → `json` (see the contract registry).
  async lab_registry_record_conformance(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.record_conformance", params);
    return raw;
  }

  /// `lab.registry.refresh` → `json` (see the contract registry).
  async lab_registry_refresh(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.refresh", params);
    return raw;
  }

  /// `lab.registry.register` → `json` (see the contract registry).
  async lab_registry_register(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.register", params);
    return raw;
  }

  /// `lab.registry.resolve` → `json` (see the contract registry).
  async lab_registry_resolve(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.resolve", params);
    return raw;
  }

  /// `lab.registry.revoke` → `json` (see the contract registry).
  async lab_registry_revoke(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.revoke", params);
    return raw;
  }

  /// `lab.registry.sameness` → `json` (see the contract registry).
  async lab_registry_sameness(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.sameness", params);
    return raw;
  }

  /// `lab.registry.slot_choices` → `json` (see the contract registry).
  async lab_registry_slot_choices(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.slot_choices", params);
    return raw;
  }

  /// `lab.registry.snapshot` → `json` (see the contract registry).
  async lab_registry_snapshot(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.snapshot", params);
    return raw;
  }

  /// `lab.registry.substitutable` → `json` (see the contract registry).
  async lab_registry_substitutable(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.substitutable", params);
    return raw;
  }

  /// `lab.registry.verify` → `json` (see the contract registry).
  async lab_registry_verify(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.verify", params);
    return raw;
  }

  /// `lab.registry.yank` → `json` (see the contract registry).
  async lab_registry_yank(params: Json): Promise<Json> {
    const raw = await this.call("lab.registry.yank", params);
    return raw;
  }

  /// `lab.results.catalogue` → `json` (see the contract registry).
  async lab_results_catalogue(params: Json): Promise<Json> {
    const raw = await this.call("lab.results.catalogue", params);
    return raw;
  }

  /// `lab.results.cells` → `json` (see the contract registry).
  async lab_results_cells(params: Json): Promise<Json> {
    const raw = await this.call("lab.results.cells", params);
    return raw;
  }

  /// `lab.results.distribution` → `json` (see the contract registry).
  async lab_results_distribution(params: Json): Promise<Json> {
    const raw = await this.call("lab.results.distribution", params);
    return raw;
  }

  /// `lab.results.export_rows` → `json` (see the contract registry).
  async lab_results_export_rows(params: Json): Promise<Json> {
    const raw = await this.call("lab.results.export_rows", params);
    return raw;
  }

  /// `lab.results.get_row` → `json` (see the contract registry).
  async lab_results_get_row(params: Json): Promise<Json> {
    const raw = await this.call("lab.results.get_row", params);
    return raw;
  }

  /// `lab.results.query_rows` → `json` (see the contract registry).
  async lab_results_query_rows(params: Json): Promise<Json> {
    const raw = await this.call("lab.results.query_rows", params);
    return raw;
  }

  /// `lab.results.row_history` → `json` (see the contract registry).
  async lab_results_row_history(params: Json): Promise<Json> {
    const raw = await this.call("lab.results.row_history", params);
    return raw;
  }

  /// `lab.results.subscribe` → `json` (see the contract registry).
  async lab_results_subscribe(params: Json): Promise<Json> {
    const raw = await this.call("lab.results.subscribe", params);
    return raw;
  }

  /// `lab.results.verify_citation` → `json` (see the contract registry).
  async lab_results_verify_citation(params: Json): Promise<Json> {
    const raw = await this.call("lab.results.verify_citation", params);
    return raw;
  }

  /// `lab.results.verify_row` → `json` (see the contract registry).
  async lab_results_verify_row(params: Json): Promise<Json> {
    const raw = await this.call("lab.results.verify_row", params);
    return raw;
  }

  /// `lab.results.verify_snapshot` → `json` (see the contract registry).
  async lab_results_verify_snapshot(params: Json): Promise<Json> {
    const raw = await this.call("lab.results.verify_snapshot", params);
    return raw;
  }

  /// `lab.serve` → `json` (see the contract registry).
  async lab_serve(params: Json): Promise<Json> {
    const raw = await this.call("lab.serve", params);
    return raw;
  }

  /// `lineage` → `Page` (see the contract registry).
  async lineage(params: LineageParams): Promise<Page> {
    const raw = await this.call("lineage", lineageParamsToJson(params));
    return pageFromJson(raw);
  }

  /// `list_leases` → `ListLeasesResult` (see the contract registry).
  async list_leases(params: ListLeasesParams): Promise<ListLeasesResult> {
    const raw = await this.call("list_leases", listLeasesParamsToJson(params));
    return listLeasesResultFromJson(raw);
  }

  /// `measurement.emit_metric` → `json` (see the contract registry).
  async measurement_emit_metric(params: Json): Promise<Json> {
    const raw = await this.call("measurement.emit_metric", params);
    return raw;
  }

  /// `navigate` → `json` (see the contract registry).
  async navigate(params: NavigateParams): Promise<Json> {
    const raw = await this.call("navigate", navigateParamsToJson(params));
    return raw;
  }

  /// `open_inbox` → `json` (see the contract registry).
  async open_inbox(params: OpenInboxParams): Promise<Json> {
    const raw = await this.call("open_inbox", openInboxParamsToJson(params));
    return raw;
  }

  /// `open_session` → `Session` (see the contract registry).
  async open_session(params: OpenSessionParams): Promise<Session> {
    const raw = await this.call("open_session", openSessionParamsToJson(params));
    return sessionFromJson(raw);
  }

  /// `project` → `View` (see the contract registry).
  async project(params: ProjectParams): Promise<View> {
    const raw = await this.call("project", projectParamsToJson(params));
    return viewFromJson(raw);
  }

  /// `promote` → `json` (see the contract registry).
  async promote(params: PromoteParams): Promise<Json> {
    const raw = await this.call("promote", promoteParamsToJson(params));
    return raw;
  }

  /// `prove_consistency` → `json` (see the contract registry).
  async prove_consistency(params: ProveConsistencyParams): Promise<Json> {
    const raw = await this.call("prove_consistency", proveConsistencyParamsToJson(params));
    return raw;
  }

  /// `prove_inclusion` → `json` (see the contract registry).
  async prove_inclusion(params: ProveInclusionParams): Promise<Json> {
    const raw = await this.call("prove_inclusion", proveInclusionParamsToJson(params));
    return raw;
  }

  /// `read` → `Page` (see the contract registry).
  async read(params: ReadParams): Promise<Page> {
    const raw = await this.call("read", readParamsToJson(params));
    return pageFromJson(raw);
  }

  /// `record_occurrence` → `json` (see the contract registry).
  async record_occurrence(params: RecordOccurrenceParams): Promise<Json> {
    const raw = await this.call("record_occurrence", recordOccurrenceParamsToJson(params));
    return raw;
  }

  /// `replay` → `ReplayResult` (see the contract registry).
  async replay(params: ReplayParams): Promise<ReplayResult> {
    const raw = await this.call("replay", replayParamsToJson(params));
    return replayResultFromJson(raw);
  }

  /// `report_host_effect` → `Recorded` (see the contract registry).
  async report_host_effect(params: ReportHostEffectParams): Promise<Recorded> {
    const raw = await this.call("report_host_effect", reportHostEffectParamsToJson(params));
    return recordedFromJson(raw);
  }

  /// `respond_elicitation` → `Recorded` (see the contract registry).
  async respond_elicitation(params: RespondElicitationParams): Promise<Recorded> {
    const raw = await this.call("respond_elicitation", respondElicitationParamsToJson(params));
    return recordedFromJson(raw);
  }

  /// `respond_permission` → `Recorded` (see the contract registry).
  async respond_permission(params: RespondPermissionParams): Promise<Recorded> {
    const raw = await this.call("respond_permission", respondPermissionParamsToJson(params));
    return recordedFromJson(raw);
  }

  /// `rollback` → `json` (see the contract registry).
  async rollback(params: RollbackParams): Promise<Json> {
    const raw = await this.call("rollback", rollbackParamsToJson(params));
    return raw;
  }

  /// `run_index` → `json` (see the contract registry).
  async run_index(params: RunIndexParams): Promise<Json> {
    const raw = await this.call("run_index", runIndexParamsToJson(params));
    return raw;
  }

  /// `set_coordinate` → `Recorded` (see the contract registry).
  async set_coordinate(params: SetCoordinateParams): Promise<Recorded> {
    const raw = await this.call("set_coordinate", setCoordinateParamsToJson(params));
    return recordedFromJson(raw);
  }

  /// `steer` → `Accepted` (see the contract registry).
  async steer(params: SteerParams): Promise<Accepted> {
    const raw = await this.call("steer", steerParamsToJson(params));
    return acceptedFromJson(raw);
  }

  /// `stream_events` → `StreamTicket` (see the contract registry).
  async stream_events(params: StreamEventsParams): Promise<StreamTicket> {
    const raw = await this.call("stream_events", streamEventsParamsToJson(params));
    return streamTicketFromJson(raw);
  }

  /// `submit` → `Accepted` (see the contract registry).
  async submit(params: SubmitParams): Promise<Accepted> {
    const raw = await this.call("submit", submitParamsToJson(params));
    return acceptedFromJson(raw);
  }

  /// `subscribe` → `json` (see the contract registry).
  async subscribe(params: SubscribeParams): Promise<Json> {
    const raw = await this.call("subscribe", subscribeParamsToJson(params));
    return raw;
  }

  /// `suspend` → `json` (see the contract registry).
  async suspend(params: SuspendParams): Promise<Json> {
    const raw = await this.call("suspend", suspendParamsToJson(params));
    return raw;
  }

  /// `verify` → `json` (see the contract registry).
  async verify(params: VerifyParams): Promise<Json> {
    const raw = await this.call("verify", verifyParamsToJson(params));
    return raw;
  }

  /// The negotiated capabilities (from the completed `hello`).
  negotiated(): HostCapabilities | undefined {
    return this.hello_result === undefined ? undefined : this.hello_result.negotiated;
  }
}

/// AC-R-2.11.4-3 client-side check: a `durable` frame's `hash` must equal
/// the ledger hash of `seq`. The client verifies by re-reading `seq`
/// through `read`/`head` and comparing — this helper performs the
/// comparison given the kernel's word.
export function verifyDurableHash(frame: Frame, ledgerHashForSeq: string): boolean {
  return frame.kind === "durable" ? frame.hash === ledgerHashForSeq : true;
}
