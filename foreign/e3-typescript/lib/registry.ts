// lib/registry.ts — the `registry-snapshot` arm (DF-S1.8-1): replay
// `registry.json` (the canonical append-only registry log) into the
// record/name/revocation/stale projections, then fold the corpus' fixed
// output vector to the `expected-outputs.json` bytes; the answer is their
// SHA-256.
//
// Line kinds (append-only log): `record` (envelope+body), `name_history`,
// `supersedes`, `revocation`, `policy`. Rebuilt here: the record table,
// the name-entry sequence, the revocation set and the stale-by-dependency
// index (dependant → [revoked members]) — the contract the snapshot fold
// consumes.
//
// The request vector is fixed corpus contract, not enumerated by the
// scenario (`scenario.json` carries only `snapshot_id` +
// `variant_version_ids`): Audit resolves over the pinned members, the six
// pinned Execute selectors, the `kind = variant` query, and
// `slot_choices` for the three pinned class ids under
// `contract_version: "1.0"` (CHECKER.md §Registry snapshot).

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { canonical, parseJson, sha256Hex } from "./core.ts";
import type { Json } from "./core.ts";

function isObj(v: Json | undefined): v is { [key: string]: Json } {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/** The pinned `version_id`s a record references (the snapshot-closure
 *  edge set; names/labels/selectors are never pins). */
function referencedVersionIds(kind: Json | undefined, body: Json | undefined): string[] {
  if (!isObj(body)) return [];
  const out: string[] = [];
  if (kind === "class") {
    if (typeof body["conformance_suite_ref"] === "string") {
      out.push(body["conformance_suite_ref"]);
    }
    for (const dep of (body["depends_on"] as Json[]) ?? []) {
      if (isObj(dep) && typeof dep["version_id"] === "string") {
        out.push(dep["version_id"]);
      }
    }
  } else if (kind === "variant") {
    if (typeof body["class_ref"] === "string") out.push(body["class_ref"]);
  } else if (kind === "conformance_suite") {
    if (typeof body["class_ref"] === "string") out.push(body["class_ref"]);
  } else if (kind === "conformance_report") {
    for (const k of ["subject_ref", "suite_ref"]) {
      if (typeof body[k] === "string") out.push(body[k]);
    }
  } else if (kind === "registry_snapshot") {
    for (const m of (body["members"] as Json[]) ?? []) {
      out.push(m as string);
    }
  } else if (kind === "extension") {
    for (const r of (body["contributes"] as Json[]) ?? []) {
      if (isObj(r) && typeof r["version_id"] === "string") {
        out.push(r["version_id"]);
      }
    }
    const trust = body["trust"];
    if (isObj(trust)) {
      for (const r of (trust["grants"] as Json[]) ?? []) {
        if (isObj(r) && typeof r["version_id"] === "string") {
          out.push(r["version_id"]);
        }
      }
    }
  } else if (kind === "environment_record") {
    const semantic = body["semantic"];
    const v = isObj(semantic) && isObj(semantic["containment_policy"])
      ? (semantic["containment_policy"] as { [key: string]: Json })["version_id"]
      : undefined;
    if (typeof v === "string") out.push(v);
  }
  return out;
}

interface RecordEntry {
  env: { [key: string]: Json };
  body: Json;
}

interface Snapshot {
  members: Set<string>;
  nameBindings: Map<string, string>; // "ns\0name" → pinned version_id
}

/** The store state the scenario's request vector reads: records by
 *  version_id, the ordered name entries, the revocation set and the
 *  derived stale-by-dependency index. */
export class RegistryReplay {
  records = new Map<string, RecordEntry>(); // version_id → {env, body}
  nameEntries: { [key: string]: Json }[] = []; // name_history lines, in order
  revoked: string[] = []; // revoked version_ids, in order
  deps = new Map<string, string[]>(); // dependant → [member version_ids]
  stale = new Map<string, string[]>(); // dependant → [revoked member ids]

  constructor(log: Json[]) {
    for (const line of log) {
      if (!isObj(line)) continue;
      const ty = line["type"];
      if (ty === "record") {
        const env = line["envelope"] as { [key: string]: Json };
        const vid = env["version_id"] as string;
        const body = line["body"] ?? null;
        this.records.set(vid, { env, body });
        for (const member of referencedVersionIds(env["kind"], body)) {
          this.declareDependency(vid, member);
        }
      } else if (ty === "name_history") {
        this.nameEntries.push(line);
      } else if (ty === "supersedes") {
        // lineage edge — not read by this scenario's fold
      } else if (ty === "revocation") {
        this.revoke(line["revokes"] as string);
      } else if (ty === "policy") {
        // corpus policy is the Stage-1 default (declared floor)
      }
    }
  }

  private staleMark(dependant: string, member: string): void {
    const lst = this.stale.get(dependant) ?? [];
    if (!lst.includes(member)) {
      lst.push(member);
      this.stale.set(dependant, lst);
    }
  }

  private revokedCauses(member: string): string[] {
    const causes: string[] = [];
    if (this.revoked.includes(member)) causes.push(member);
    causes.push(...(this.stale.get(member) ?? []));
    return causes;
  }

  private declareDependency(dependant: string, member: string): void {
    const lst = this.deps.get(dependant) ?? [];
    lst.push(member);
    this.deps.set(dependant, lst);
    for (const cause of this.revokedCauses(member)) {
      this.staleMark(dependant, cause);
    }
  }

  private revoke(vid: string): void {
    if (this.revoked.includes(vid)) return;
    this.revoked.push(vid);
    const causes = [vid, ...(this.stale.get(vid) ?? [])];
    // Every transitive dependant of `vid` becomes stale on all causes.
    const queue = [vid];
    const seen = new Set([vid]);
    while (queue.length > 0) {
      const node = queue.pop() as string;
      for (const [dep, members] of this.deps) {
        if (members.includes(node) && !seen.has(dep)) {
          seen.add(dep);
          queue.push(dep);
          for (const cause of causes) {
            this.staleMark(dep, cause);
          }
        }
      }
    }
  }

  isRevoked(vid: string): boolean {
    return this.revoked.includes(vid);
  }

  staleFor(vid: string): string[] {
    return [...(this.stale.get(vid) ?? [])];
  }

  /** The first name-history entry binding this version (a pinned id
   *  resolves with its status attached). */
  firstNameEntry(vid: string): { [key: string]: Json } | null {
    for (const e of this.nameEntries) {
      if (e["version_id"] === vid) return e;
    }
    return null;
  }

  snapshot(snapId: string): Snapshot {
    const rec = this.records.get(snapId);
    if (rec === undefined || rec.env["kind"] !== "registry_snapshot") {
      throw new Error(`UnknownSnapshot: ${snapId}`);
    }
    const body = rec.body;
    const bindings = new Map<string, string>();
    if (isObj(body) && isObj(body["name_bindings"])) {
      for (const [k, v] of Object.entries(body["name_bindings"])) {
        const sep = k.indexOf("/"); // partition on the FIRST "/" (e2's tuple key)
        bindings.set(
          (sep < 0 ? k : k.slice(0, sep)) + "\0" + (sep < 0 ? "" : k.slice(sep + 1)),
          v as string,
        );
      }
    }
    const members = new Set<string>(
      isObj(body) ? ((body["members"] as Json[]) ?? []).map((m) => m as string) : [],
    );
    return { members, nameBindings: bindings };
  }
}

function resolvedJson(
  vid: string,
  env: { [key: string]: Json },
  store: RegistryReplay,
  nameStatus: Json,
): { [key: string]: Json } {
  const admission = store.isRevoked(vid)
    ? "revoked"
    : (env["admission"] ?? null);
  return {
    admission,
    depends_on_revoked: store.staleFor(vid),
    name_status: nameStatus,
    semantic_id: env["semantic_id"] ?? null,
    version_id: vid,
  };
}

// The corpus' output vector — fixed by the arm's replay contract (the
// scenario carries `snapshot_id` + `variant_version_ids`; the selector
// names and the class list are the corpus contract CHECKER.md points at).
const OUTPUT_SELECTORS: [string, string][] = [
  ["hh", "control_strategy/control_strategy-v0"],
  ["local", "control_strategy/control_strategy-v1"],
  ["hh", "context_policy/context_policy-v0"],
  ["local", "context_policy/context_policy-v1"],
  ["hh", "export_sink/export_sink-v0"],
  ["local", "export_sink/export_sink-v1"],
];
const OUTPUT_CLASSES = ["control_strategy", "context_policy", "export_sink"];

/** The one contract-version range grammar: `*`/empty covers all; `v`
 *  exact; `a-b` inclusive; `>=v`/`<=v` open; dotted-numeric compare. */
function rangeCovers(rng: string, version: string): boolean {
  const r = rng.trim();
  if (r === "*" || r === "") return true;
  if (r.startsWith(">=")) return versionCmp(version, r.slice(2).trim()) >= 0;
  if (r.startsWith("<=")) return versionCmp(version, r.slice(2).trim()) <= 0;
  if (r.includes("-")) {
    const i = r.indexOf("-");
    return (
      versionCmp(version, r.slice(0, i).trim()) >= 0 &&
      versionCmp(version, r.slice(i + 1).trim()) <= 0
    );
  }
  return versionCmp(version, r) === 0;
}

function versionCmp(a: string, b: string): number {
  const parts = (s: string): number[] =>
    s
      .trim()
      .replace(/^v/, "")
      .split(".")
      .map((p) => (/^\d+$/.test(p) ? parseInt(p, 10) : 0));
  const pa = parts(a);
  const pb = parts(b);
  // Python list-compare semantics (the e2 spelling): first differing
  // component decides; a prefix-equal shorter vector is *smaller*
  // ([1,0] < [1,0,0]) — never pad with zeros.
  for (let i = 0; i < Math.min(pa.length, pb.length); i += 1) {
    const d = pa[i] - pb[i];
    if (d !== 0) return d < 0 ? -1 : 1;
  }
  return pa.length < pb.length ? -1 : pa.length > pb.length ? 1 : 0;
}

export function armRegistrySnapshot(armDir: string): { [key: string]: Json } {
  const registryBytes = readFileSync(join(armDir, "registry.json"));
  const log = parseJson(registryBytes);
  if (!Array.isArray(log)) throw new Error("registry.json must be an array of lines");
  const store = new RegistryReplay(log);
  const scenario = parseJson(readFileSync(join(armDir, "scenario.json")));
  if (!isObj(scenario)) throw new Error("scenario.json must be an object");
  const snap = store.snapshot(scenario["snapshot_id"] as string);
  const members = snap.members;

  const out: Json[] = [];
  // 1. resolve(version_id, audit) for each pinned member.
  for (const vid of scenario["variant_version_ids"] as Json[]) {
    const rec = store.records.get(vid as string);
    if (rec === undefined) {
      out.push({ error: "Unresolved" });
      continue;
    }
    out.push(resolvedJson(vid as string, rec.env, store, null));
  }

  // 2. resolve(selector, execute) over the snapshot's name bindings.
  for (const [ns, name] of OUTPUT_SELECTORS) {
    const pinned = snap.nameBindings.get(ns + "\0" + name);
    const rec = pinned === undefined ? undefined : store.records.get(pinned);
    if (pinned === undefined || !members.has(pinned) || rec === undefined) {
      out.push({ error: "Unresolved" });
      continue;
    }
    const env = rec.env;
    if (store.isRevoked(pinned)) {
      out.push({ error: "Revoked" });
    } else if (env["admission"] !== "resolved" && env["admission"] !== "sealed") {
      out.push({ error: "Unresolved" });
    } else if (store.staleFor(pinned).length > 0) {
      out.push({ error: "Stale" });
    } else if (
      env["kind"] === "variant" &&
      isObj(rec.body) &&
      !rec.body["capability_declaration"]
    ) {
      // declared floor: a schema-valid capability_declaration must exist
      out.push({ error: "ConformanceBelowFloor" });
    } else {
      out.push(resolvedJson(pinned, env, store, null));
    }
  }

  // 3. query(kind = variant) over snapshot members, sorted by version_id.
  const rows: { [key: string]: Json }[] = [];
  for (const [vid, rec] of store.records) {
    if (!members.has(vid) || rec.env["kind"] !== "variant") continue;
    const entry = store.firstNameEntry(vid);
    rows.push(
      resolvedJson(
        vid,
        rec.env,
        store,
        entry === null ? null : (entry["status"] ?? null),
      ),
    );
  }
  // Python's `sorted` on strings is codepoint order — mirror it exactly
  // (the `sha256:` ids are ASCII; a comparator beats locale collation).
  rows.sort((a, b) => {
    const va = a["version_id"] as string;
    const vb = b["version_id"] as string;
    return va < vb ? -1 : va > vb ? 1 : 0;
  });
  out.push(rows);

  // 4. slot_choices(class_id, {contract_version: "1.0"}, snapshot) per class.
  for (const cid of OUTPUT_CLASSES) {
    // declared floor: a class without a suite is SuiteMissing.
    let hasSuite = false;
    for (const rec of store.records.values()) {
      if (
        rec.env["kind"] === "class" &&
        isObj(rec.body) &&
        rec.body["class_id"] === cid &&
        typeof rec.body["conformance_suite_ref"] === "string"
      ) {
        hasSuite = true;
        break;
      }
    }
    if (!hasSuite) {
      out.push({ error: "SuiteMissing" });
      continue;
    }
    const choices: { [key: string]: Json }[] = [];
    for (const [vid, rec] of store.records) {
      const env = rec.env;
      const body = rec.body;
      if (!members.has(vid) || env["kind"] !== "variant" || !isObj(body)) continue;
      const cls = store.records.get(body["class_ref"] as string);
      const belongs =
        cls !== undefined &&
        cls.env["kind"] === "class" &&
        isObj(cls.body) &&
        cls.body["class_id"] === cid;
      if (!belongs) continue;
      if (env["admission"] !== "resolved" && env["admission"] !== "sealed") continue;
      if (store.isRevoked(vid) || store.staleFor(vid).length > 0) continue;
      if (!rangeCovers((body["contract_range"] as string) ?? "", "1.0")) continue;
      if (!body["capability_declaration"]) continue;
      choices.push({ variant_id: body["variant_id"], version_id: vid });
    }
    choices.sort((a, b) => {
      const va = a["variant_id"] as string;
      const vb = b["variant_id"] as string;
      if (va !== vb) return va < vb ? -1 : 1;
      const ia = a["version_id"] as string;
      const ib = b["version_id"] as string;
      return ia < ib ? -1 : ia > ib ? 1 : 0;
    });
    out.push(choices);
  }

  const outputs = canonical(out);
  return {
    outputs_sha256: sha256Hex(outputs),
    registry_sha256: sha256Hex(registryBytes),
  };
}
