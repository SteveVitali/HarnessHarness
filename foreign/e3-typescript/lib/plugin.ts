// lib/plugin.ts — the `plugin-manifests` arm (R4.4a): decode a committed
// plugin manifest and replay the §8.4 §2 admission gate against the
// deterministic registry corpus' class catalog — `admit:<ns>/<name>` or
// `refuse:<kind>` per case, in the checker's posture
// (`first_party = true`, `EvalPoint::kernel`, deny-everything requests cap).
//
//   Admission order: decode → namespace → locality → tier → compatibility
//   → requests ⊆ cap.
//
// Ported against the one E2 spelling (CC1: `foreign/e2-python/foreign.py`
// is the foreign reference for this arm; E3 reuses `lib/core.ts` +
// `lib/registry.ts`'s `RegistryReplay.classCatalog`, never a second
// codec or store). Verdict kinds are the closed `PluginError::kind_str`
// set — a malformed member is `SchemaViolation`, a selector/pin that did
// not survive into the sealed form is `UnpinnedInSealedForm`, never a
// silently-admitted manifest.

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { parseJson } from "./core.ts";
import type { Json } from "./core.ts";
import { RegistryReplay } from "./registry.ts";

type Obj = { [key: string]: Json };

function isObj(v: Json | undefined): v is Obj {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/** Int ≠ bool (JS has no int/bool split — `typeof` guard). */
function isUint(v: Json | undefined): v is number {
  return typeof v === "number" && Number.isInteger(v) && v >= 0;
}

// ─────────────────────────────────────────────────────────────────────────────
// The closed vocabularies (§8.4; registry/1's record-kind + contract tables)
// ─────────────────────────────────────────────────────────────────────────────

const CONTRIBUTION_KINDS = new Set([
  "variant", "capability", "mcp_server", "procedure_profile", "hook",
  "validator", "model_profile", "surface_family", "benchmark_adapter",
  "environment_family", "metric_declaration", "instruction_file",
  "harness_rule",
]);
const CONTRACT_KINDS = new Set([
  "class_contract", "record_kind", "dialect", "protocol_binding",
]);
const ISOLATION_CLASSES = new Set([
  "in_process", "subprocess_confined", "container", "remote", "host_unconfined",
]);
const PLACEMENTS = new Set([
  "in_process", "subprocess_confined", "container", "remote", "component_model",
]);
const AUTHORITY_RANK: { [k: string]: number } = {
  unverified: 0, external: 1, environment: 2, delegate: 3,
  principal: 4, definition: 5, kernel: 6,
};
// registry/1's closed record-kind sum — each carries a `record_kind:<id>`
// contract policy with supported ["1"].
const RECORD_KINDS = [
  "class", "variant", "conformance_suite", "conformance_report", "namespace",
  "name_history_entry", "registry_snapshot", "foreign_import",
  "sealed_definition", "capability", "extension", "model_profile",
  "profile_test_report", "wire_dialect", "model_role_table",
  "procedure_profile", "surface_family", "metric_declaration", "validator",
  "environment_record", "environment_family", "trust_root_policy",
  "identity_profile", "participant", "adapter", "leaderboard_definition",
];
// The kernel's own contract table: dialects + the two boundary bindings, each
// supported ["1"], no sunsets (hh-hosting/1 is deliberately absent — AC-12).
const KERNEL_CONTRACTS: { [k: string]: string[] } = {
  "dialect\0hir/1": ["1"],
  "dialect\0idp/1": ["1"],
  "dialect\0registry/1": ["1"],
  "dialect\0hh-experiment/1": ["1"],
  "protocol_binding\0plugin_abi/1": ["1"],
  "protocol_binding\0hh-embed/1": ["1"],
};

// ─────────────────────────────────────────────────────────────────────────────
// The typed refusal — `kind` is the `refuse:<kind>` the checker compares.
// ─────────────────────────────────────────────────────────────────────────────

class ManifestError extends Error {
  kind: string;
  constructor(kind: string, detail: string) {
    super(`${kind}: ${detail}`);
    this.kind = kind;
  }
}

function sv(path: string, detail: string): ManifestError {
  return new ManifestError("SchemaViolation", `${path}: ${detail}`);
}

function req(j: Obj, k: string, path: string): Json {
  const v = j[k];
  if (v === undefined) throw sv(path, `missing member \`${k}\``);
  return v;
}

function reqStr(j: Obj, k: string, path: string): string {
  const v = req(j, k, path);
  if (typeof v !== "string") throw sv(path, `\`${k}\` is not a string`);
  return v;
}

function optStr(j: Obj, k: string): string | undefined {
  const v = j[k];
  return typeof v === "string" ? v : undefined;
}

function reqArr(j: Obj, k: string, path: string): Json[] {
  const v = req(j, k, path);
  if (!Array.isArray(v)) throw sv(path, `\`${k}\` is not an array`);
  return v;
}

function strVec(j: Obj, k: string, path: string): string[] {
  const out: string[] = [];
  reqArr(j, k, path).forEach((v, i) => {
    if (typeof v !== "string") throw sv(path, `\`${k}[${i}]\` is not a string`);
    out.push(v);
  });
  return out;
}

// ─────────────────────────────────────────────────────────────────────────────
// The contract-version range grammar: `*`/empty covers all; `v` exact;
// `a-b` inclusive; `>=v`/`<=v` open; dotted-numeric compare.
// ─────────────────────────────────────────────────────────────────────────────

function versionParts(s: string): number[] {
  return s
    .trim()
    .replace(/^v+/, "")
    .split(".")
    .map((p) => (/^\d+$/.test(p) ? Number(p) : 0));
}

function versionCmp(a: string, b: string): number {
  const pa = versionParts(a);
  const pb = versionParts(b);
  const n = Math.max(pa.length, pb.length);
  for (let i = 0; i < n; i += 1) {
    if (pa[i] === undefined) return -1; // shorter prefix is less (Rust Vec::cmp)
    if (pb[i] === undefined) return 1;
    const d = pa[i] - pb[i];
    if (d !== 0) return d;
  }
  return 0;
}

function validVersion(v: string): boolean {
  const s = v.trim().replace(/^v+/, "");
  return s !== "" && s.split(".").every((seg) => /^\d+$/.test(seg));
}

function rangeParses(s: Json | undefined): s is string {
  if (typeof s !== "string") return false;
  const r = s.trim();
  if (r === "" || r === "*") return true;
  if (r.startsWith(">=") || r.startsWith("<=")) return validVersion(r.slice(2));
  if (r.includes("-")) {
    const i = r.indexOf("-");
    return validVersion(r.slice(0, i)) && validVersion(r.slice(i + 1));
  }
  return validVersion(r);
}

function rangeCovers(rng: string, version: string): boolean {
  const r = rng.trim();
  if (r === "*" || r === "") return true;
  if (r.startsWith(">=")) return versionCmp(version, r.slice(2).trim()) >= 0;
  if (r.startsWith("<=")) return versionCmp(version, r.slice(2).trim()) <= 0;
  if (r.includes("-")) {
    const i = r.indexOf("-");
    const lo = r.slice(0, i).trim();
    const hi = r.slice(i + 1).trim();
    return versionCmp(version, lo) >= 0 && versionCmp(version, hi) <= 0;
  }
  return versionCmp(version, r) === 0;
}

function parseTier(t: Json | undefined): number | undefined {
  if (typeof t !== "string" || !t.startsWith("C")) return undefined;
  const digits = t.slice(1);
  if (digits === "") return undefined;
  const n = Number(digits);
  return Number.isInteger(n) && n >= 0 && n <= 4 ? n : undefined;
}

// ─────────────────────────────────────────────────────────────────────────────
// Manifest decode — the §8.4 shape; every check raises a typed ManifestError.
// ─────────────────────────────────────────────────────────────────────────────

interface ContractRef {
  kind: string;
  id: string;
  version_range: string;
}

interface Contribution {
  kind: string;
  declaration: Obj;
  executable: { isolation: string; placement_preference?: string } | undefined;
  contract_range: string | undefined;
}

interface Manifest {
  identity: { namespace: string; name: string; version_label?: string };
  tier: string;
  depends_on: ContractRef[];
  requires: {
    hir_dialect: string;
    registry_dialect: string;
    plugin_abi: string;
    contracts: ContractRef[];
  };
  contributions: Contribution[];
  requests: {
    effects: Json[];
    fs_roots: Json[];
    egress: Json[];
    env_keys: Json[];
    budget_share: Json | undefined;
    hot_path_operations: Json[];
  };
}

function contractRef(j: Json, path: string): ContractRef {
  if (!isObj(j)) throw sv(path, "expected object");
  for (const k of Object.keys(j)) {
    if (!["kind", "id", "version_range", "ext"].includes(k)) {
      throw sv(path, `unknown member \`${k}\``);
    }
  }
  const kind = j["kind"];
  if (typeof kind !== "string") throw sv(path, "`kind` missing or not a string");
  if (!CONTRACT_KINDS.has(kind)) {
    throw sv(path, `\`kind\` \`${kind}\` is not in the closed ContractRef kind sum`);
  }
  const id = j["id"];
  if (typeof id !== "string") throw sv(path, "`id` missing or not a string");
  if (id === "") throw sv(path, "`id` is empty");
  const vr = j["version_range"];
  if (typeof vr !== "string") throw sv(path, "`version_range` missing or not a string");
  if (!rangeParses(vr)) throw sv(path, `\`version_range\` \`${vr}\` is malformed`);
  return { kind, id, version_range: vr };
}

/** A package-member path: never empty, absolute, NUL, `:`-carrying or
 *  `..`-escaping (a manifest names package members only). */
function checkPackagePath(p: string, path: string): void {
  const outside =
    p === "" ||
    p.startsWith("/") ||
    p.startsWith("\\") ||
    p.includes("\x00") ||
    p.includes(":") ||
    p.replaceAll("\\", "/").split("/").includes("..");
  if (outside) {
    throw sv(path, `path \`${p}\` escapes the package root (a manifest names package members only)`);
  }
}

/** A locator object must be fully pinned: no surviving `selector`, and
 *  `resolved` + `fetched_at` present — `UnpinnedInSealedForm`, not a schema
 *  fault (the sealed form carries pins, never selectors). */
function locatorPinned(loc: Obj, path: string): void {
  for (const k of Object.keys(loc)) {
    if (!["scheme", "credential_free_uri", "selector", "resolved", "fetched_at", "ext"].includes(k)) {
      throw sv(path, `unknown member \`${k}\``);
    }
  }
  reqStr(loc, "scheme", path);
  const uri = reqStr(loc, "credential_free_uri", path);
  const fetched = loc["fetched_at"];
  if (fetched !== undefined && !isUint(fetched)) {
    throw sv(path, "`fetched_at` is not a u64");
  }
  // credential-free check: no userinfo, no credential-like query keys
  const authority = uri.includes("://") ? uri.split("://", 2)[1] : uri.split(":", 2)[0];
  if (authority.split("/", 2)[0].includes("@")) {
    throw sv(`${path}.credential_free_uri`, "credential userinfo in locator URI");
  }
  if (uri.includes("?")) {
    for (const kv of uri.slice(uri.indexOf("?") + 1).split("&")) {
      const k = kv.split("=", 2)[0].toLowerCase();
      if (["password", "passwd", "token", "secret", "key"].includes(k)) {
        throw sv(
          `${path}.credential_free_uri`,
          `credential-like query member \`${k}\` in locator URI`,
        );
      }
    }
  }
  if (loc["selector"] !== undefined || loc["resolved"] === undefined || fetched === undefined) {
    throw new ManifestError(
      "UnpinnedInSealedForm",
      `${path}: selector survived or resolved/fetched_at missing`,
    );
  }
}

function contribution(j: Json, path: string): Contribution {
  if (!isObj(j)) throw sv(path, "expected object");
  for (const k of Object.keys(j)) {
    if (!["kind", "path_or_locator", "declaration", "executable", "contract_range", "ext"].includes(k)) {
      throw sv(path, `unknown member \`${k}\``);
    }
  }
  const kind = reqStr(j, "kind", path);
  if (!CONTRIBUTION_KINDS.has(kind)) throw new ManifestError("UnknownRecordKind", kind);

  const pol = req(j, "path_or_locator", path);
  const lp = `${path}.path_or_locator`;
  if (typeof pol === "string") {
    checkPackagePath(pol, lp);
  } else if (isObj(pol)) {
    locatorPinned(pol, lp);
  } else {
    throw sv(path, "`path_or_locator` is a package path string or a pinned locator object");
  }

  const dp = `${path}.declaration`;
  const decl = req(j, "declaration", path);
  if (!isObj(decl)) throw sv(dp, "expected object");
  for (const k of Object.keys(decl)) {
    if (["body", "content", "closure", "code"].includes(k)) {
      throw sv(dp, `member \`${k}\` is an inline body — declarations name records, they never carry them`);
    }
  }

  let executable: Contribution["executable"];
  const ej = j["executable"];
  if (ej !== undefined) {
    const ep = `${path}.executable`;
    if (!isObj(ej)) throw sv(ep, "expected object");
    for (const k of Object.keys(ej)) {
      if (!["code_pointer", "isolation", "placement_preference", "one_shot", "host_requirements", "ext"].includes(k)) {
        throw sv(ep, `unknown member \`${k}\``);
      }
    }
    const cp = `${ep}.code_pointer`;
    const cm = ej["code_pointer"];
    if (!isObj(cm)) {
      throw new ManifestError(
        "UnpinnedInSealedForm",
        `${cp} is not a pinned ContentAddress object — a selector or bare path is a claim, never a pin`,
      );
    }
    for (const k of Object.keys(cm)) {
      if (!["algorithm", "digest", "idp", "media_type", "size", "ext"].includes(k)) {
        throw sv(cp, `unknown member \`${k}\``);
      }
    }
    const algorithm = cm["algorithm"];
    if (typeof algorithm !== "string") {
      throw new ManifestError("UnpinnedInSealedForm", `${cp}: missing \`algorithm\``);
    }
    if (algorithm !== "sha256") {
      throw new ManifestError("UnpinnedInSealedForm", `${cp}: algorithm \`${algorithm}\` is not \`sha256\``);
    }
    const digest = cm["digest"];
    if (typeof digest !== "string" || digest === "") {
      throw new ManifestError("UnpinnedInSealedForm", `${cp}: missing/empty \`digest\` — not a pin`);
    }
    if (!isUint(cm["size"])) {
      throw new ManifestError("UnpinnedInSealedForm", `${cp}: missing \`size\` — not a pin`);
    }
    const isolation = reqStr(ej, "isolation", ep);
    if (!ISOLATION_CLASSES.has(isolation)) {
      throw sv(ep, "`isolation` is outside the closed IsolationClass sum");
    }
    const placement = optStr(ej, "placement_preference");
    if (placement !== undefined && !PLACEMENTS.has(placement)) {
      throw sv(ep, `\`placement_preference\` \`${placement}\` is outside the closed sum`);
    }
    const oneShot = ej["one_shot"];
    if (oneShot !== undefined && typeof oneShot !== "boolean") {
      throw sv(ep, "`one_shot` is not a bool");
    }
    executable = { isolation, placement_preference: placement };
  }

  return {
    kind,
    declaration: decl,
    executable,
    contract_range: optStr(j, "contract_range"),
  };
}

function manifestFromBytes(data: Buffer): Manifest {
  let j: Json;
  try {
    j = parseJson(data);
  } catch {
    throw new ManifestError("NotJson", "manifest is not JSON");
  }
  if (!isObj(j)) throw new ManifestError("NotJson", "manifest is not an object");
  const path = "manifest";
  const known = new Set([
    "identity", "tier", "depends_on", "requires", "contributions",
    "requests", "claims", "conformance_claims", "parameters", "summary", "ext",
  ]);
  for (const k of Object.keys(j)) {
    if (!known.has(k)) throw sv(path, `unknown non-\`ext\` member \`${k}\``);
  }

  const ip = `${path}.identity`;
  const ij = req(j, "identity", path);
  if (!isObj(ij)) throw sv(ip, "expected object");
  for (const k of Object.keys(ij)) {
    if (!["namespace", "name", "version_label", "ext"].includes(k)) {
      throw sv(ip, `unknown member \`${k}\``);
    }
  }
  const identity = {
    namespace: reqStr(ij, "namespace", ip),
    name: reqStr(ij, "name", ip),
    version_label: optStr(ij, "version_label"),
  };

  const tier = reqStr(j, "tier", path);
  if (parseTier(tier) === undefined) {
    throw sv(path, `\`tier\` \`${tier}\` is not a C0..C4 spelling`);
  }

  const dependsOn = reqArr(j, "depends_on", path).map((v, i) =>
    contractRef(v, `${path}.depends_on[${i}]`),
  );

  const rp = `${path}.requires`;
  const rj = req(j, "requires", path);
  if (!isObj(rj)) throw sv(rp, "expected object");
  for (const k of Object.keys(rj)) {
    if (!["hir_dialect", "registry_dialect", "plugin_abi", "contracts", "records", "ext"].includes(k)) {
      throw sv(rp, `unknown member \`${k}\``);
    }
  }
  const contracts = reqArr(rj, "contracts", rp).map((v, i) =>
    contractRef(v, `${rp}.contracts[${i}]`),
  );
  reqArr(rj, "records", rp).forEach((v, i) => {
    const p = `${rp}.records[${i}]`;
    if (!isObj(v)) throw sv(p, "expected pinned record ref object");
    for (const k of Object.keys(v)) {
      if (!["version_id", "kind", "ext"].includes(k)) {
        throw sv(p, `unknown member \`${k}\` (a record ref is a pin, not a selector)`);
      }
    }
    const versionId = reqStr(v, "version_id", p);
    if (versionId === "" || !versionId.includes(":")) {
      throw new ManifestError("UnpinnedInSealedForm", `${p}.version_id \`${versionId}\` is not a pinned id`);
    }
  });
  const requires = {
    hir_dialect: reqStr(rj, "hir_dialect", rp),
    registry_dialect: reqStr(rj, "registry_dialect", rp),
    plugin_abi: reqStr(rj, "plugin_abi", rp),
    contracts,
  };

  const contributions = reqArr(j, "contributions", path).map((v, i) =>
    contribution(v, `${path}.contributions[${i}]`),
  );

  const rqp = `${path}.requests`;
  let requests: Manifest["requests"] = {
    effects: [], fs_roots: [], egress: [], env_keys: [],
    budget_share: undefined, hot_path_operations: [],
  };
  const rqj = j["requests"];
  if (rqj !== undefined) {
    if (!isObj(rqj)) throw sv(rqp, "expected object");
    for (const k of Object.keys(rqj)) {
      if (!["effects", "fs_roots", "egress", "env_keys", "budget_share", "hot_path_operations", "ext"].includes(k)) {
        throw sv(rqp, `unknown member \`${k}\``);
      }
    }
    reqArr(rqj, "effects", rqp).forEach((v, i) => {
      const p = `${rqp}.effects[${i}]`;
      if (!isObj(v)) throw sv(p, "expected object");
      for (const k of Object.keys(v)) {
        if (!["domain", "scope", "ext"].includes(k)) {
          throw sv(p, `unknown member \`${k}\``);
        }
      }
      reqStr(v, "domain", p);
      reqStr(v, "scope", p);
    });
    requests = {
      effects: reqArr(rqj, "effects", rqp),
      fs_roots: strVec(rqj, "fs_roots", rqp),
      egress: reqArr(rqj, "egress", rqp),
      env_keys: strVec(rqj, "env_keys", rqp),
      // `null` decodes to Python `None` in the reference — not a request.
      budget_share: rqj["budget_share"] ?? undefined,
      hot_path_operations: strVec(rqj, "hot_path_operations", rqp),
    };
  }

  const claims = req(j, "claims", path);
  if (!isObj(claims)) throw sv(path, "`claims` is not an object");

  reqArr(j, "conformance_claims", path).forEach((v, i) => {
    const p = `${path}.conformance_claims[${i}]`;
    const producedBy = isObj(v) ? v["produced_by"] : undefined;
    if (producedBy === undefined) throw sv(p, "missing `produced_by`");
    if (producedBy !== "publisher_claim") {
      throw sv(p, `produced_by \`${String(producedBy)}\` — manifest claims are \`publisher_claim\` only`);
    }
  });

  const parameters = j["parameters"];
  if (parameters !== undefined && !isObj(parameters)) {
    throw sv(path, "`parameters` is not an object");
  }

  const sp = `${path}.summary`;
  const summary = req(j, "summary", path);
  if (!isObj(summary)) throw sv(sp, "not a Text leaf: expected object");
  for (const member of ["content_hash", "owner", "authority"]) {
    if (typeof summary[member] !== "string") {
      throw sv(sp, `not a Text leaf: \`${member}\` missing or not a string`);
    }
  }
  const authority = summary["authority"] as string;
  if (!Object.hasOwn(AUTHORITY_RANK, authority)) {
    throw sv(sp, `not a Text leaf: bad authority \`${authority}\``);
  }
  if (!isObj(summary["provenance"])) {
    throw sv(sp, "not a Text leaf: missing provenance");
  }
  if (AUTHORITY_RANK[authority] > AUTHORITY_RANK["external"]) {
    throw sv(sp, `authority ${authority} exceeds \`external\` (a manifest never mints above external)`);
  }

  const ext = j["ext"];
  if (ext !== undefined && !isObj(ext)) throw sv(path, "`ext` is not an object");

  return { identity, tier, depends_on: dependsOn, requires, contributions, requests };
}

// ─────────────────────────────────────────────────────────────────────────────
// Admission — the §8.4 §2 gate against the corpus class catalog.
// ─────────────────────────────────────────────────────────────────────────────

type Catalog = { class_id: Json; contract_version: Json; tier: number }[];

function contractDefiningTier(ref: ContractRef, catalog: Catalog): number | undefined {
  const { kind, id } = ref;
  if (kind === "dialect" || kind === "record_kind") return 0;
  if (kind === "protocol_binding") return id === "hh-hosting/1" ? 2 : 0;
  if (kind === "class_contract") {
    for (const c of catalog) {
      if (c.class_id === id) return c.tier;
    }
    return undefined;
  }
  return undefined;
}

/** Every ref must intersect a supported version (no past sunsets in the
 *  Stage-1 tables). `false` collects into ContractIncompatible. */
function checkCompatibility(refs: ContractRef[], catalog: Catalog): boolean {
  const supportedTable: { [k: string]: string[] } = { ...KERNEL_CONTRACTS };
  for (const kind of RECORD_KINDS) {
    supportedTable[`record_kind\0${kind}`] = ["1"];
  }
  let failures = 0;
  for (const r of refs) {
    let supported: Json[];
    if (r.kind === "class_contract") {
      supported = catalog
        .filter((c) => c.class_id === r.id)
        .map((c) => c.contract_version);
    } else {
      supported = supportedTable[`${r.kind}\0${r.id}`] ?? [];
    }
    const covered = supported.filter(
      (v) => typeof v === "string" && rangeCovers(r.version_range, v),
    );
    if (covered.length === 0) failures += 1;
  }
  return failures === 0;
}

/** The §8.4 §2 admission gate → `admit:<id>` or a typed ManifestError. */
function admitPlugin(manifest: Manifest, catalog: Catalog, firstPartyCtx = true): string {
  const ns = manifest.identity.namespace;
  if (!(ns === "hh" || ns === "local" || ns.startsWith("hh/") || ns.startsWith("local/"))) {
    throw new ManifestError("NamespaceForbidden", ns);
  }
  const firstParty = firstPartyCtx && (ns === "hh" || ns.startsWith("hh/"));
  const pluginId = `${ns}/${manifest.identity.name}`;

  // locality — executable contributions only
  for (const c of manifest.contributions) {
    const exe = c.executable;
    if (exe === undefined) continue;
    if (exe.isolation === "host_unconfined") {
      throw new ManifestError("LocalityInadmissible", `${pluginId} at host_unconfined`);
    }
    if (exe.placement_preference === "in_process" && !firstParty) {
      throw new ManifestError("LocalityInadmissible", `${pluginId} at in_process`);
    }
    if (exe.placement_preference === "component_model") {
      throw new ManifestError("LocalityInadmissible", `${pluginId} at component_model`);
    }
  }

  // tier — every depends_on contract's defining tier ≤ the manifest's
  const tier = parseTier(manifest.tier);
  if (tier === undefined) {
    throw sv("tier", `\`${manifest.tier}\` is not a tier spelling (C0..C4)`);
  }
  for (const r of manifest.depends_on) {
    const dt = contractDefiningTier(r, catalog);
    if (dt === undefined) {
      throw new ManifestError("ContractIncompatible", `${r.kind}:${r.id} names no contract`);
    }
    if (dt > tier) {
      throw new ManifestError(
        "TierViolation",
        `${r.kind}:${r.id} defined at C${dt} > manifest tier C${tier}`,
      );
    }
  }

  // compatibility — requires ∪ contribution contract_ranges
  const refs: ContractRef[] = [
    { kind: "dialect", id: "hir/1", version_range: manifest.requires.hir_dialect },
    { kind: "dialect", id: "registry/1", version_range: manifest.requires.registry_dialect },
    { kind: "protocol_binding", id: "plugin_abi/1", version_range: manifest.requires.plugin_abi },
  ];
  refs.push(...manifest.requires.contracts);
  for (const c of manifest.contributions) {
    if (c.contract_range !== undefined) {
      if (c.kind === "variant") {
        const classId = c.declaration["class_id"];
        if (typeof classId !== "string") {
          throw sv(
            "contributions.declaration",
            "a variant contribution with contract_range must name declaration.class_id",
          );
        }
        refs.push({ kind: "class_contract", id: classId, version_range: c.contract_range });
      } else {
        refs.push({ kind: "record_kind", id: c.kind, version_range: c.contract_range });
      }
    }
  }
  if (!checkCompatibility(refs, catalog)) {
    throw new ManifestError("ContractIncompatible", "");
  }

  // requests ⊆ requests_cap (the bundle's posture: deny-everything cap)
  const rq = manifest.requests;
  if (
    rq.effects.length > 0 ||
    rq.fs_roots.length > 0 ||
    rq.egress.length > 0 ||
    rq.env_keys.length > 0 ||
    rq.budget_share !== undefined ||
    rq.hot_path_operations.length > 0
  ) {
    throw new ManifestError("RequestsExceedCap", "requests exceed the deny-everything cap");
  }

  return `admit:${pluginId}`;
}

/** The arm driver: every `*.json` member under the arm dir (recursively,
 *  minus `expected.json`) is a manifest case; the store is the
 *  deterministic registry corpus the `registry-snapshot` arm ships. */
export function armPluginManifests(armDir: string, bundleDir: string): { [key: string]: Json } {
  const corpusLog = parseJson(
    readFileSync(join(bundleDir, "arms/registry-snapshot/registry.json")),
  );
  const catalog = new RegistryReplay(
    (Array.isArray(corpusLog) ? corpusLog : []) as Json[],
  ).classCatalog();

  const answers: { [key: string]: Json } = Object.create(null);
  const walk = (dir: string, rel: string): void => {
    for (const e of readdirSync(dir, { withFileTypes: true }).sort((a, b) =>
      a.name < b.name ? -1 : a.name > b.name ? 1 : 0,
    )) {
      const p = join(dir, e.name);
      const r = rel === "" ? e.name : `${rel}/${e.name}`;
      if (e.isDirectory()) {
        walk(p, r);
      } else if (e.name.endsWith(".json") && e.name !== "expected.json") {
        let verdict: string;
        try {
          verdict = admitPlugin(manifestFromBytes(readFileSync(p)), catalog);
        } catch (err) {
          if (!(err instanceof ManifestError)) throw err;
          verdict = `refuse:${err.kind}`;
        }
        answers[r] = verdict;
      }
    }
  };
  walk(armDir, "");
  return answers;
}
