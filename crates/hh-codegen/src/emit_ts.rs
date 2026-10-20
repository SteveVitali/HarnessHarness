//! TypeScript emission for the E3 surface binding (R4.2; ADR-0050 D1's
//! late-bound surface ecosystem — the operator's HUMAN-H1 ruling bound
//! E3 = TypeScript, LEDGER GATE DECISIONS 2026-10-20). Fully
//! schema-driven like [`crate::emit`]: every record, string-enum and
//! tagged sum in the exported schema's `types` table (plus
//! `frames.Frame`) is emitted with `*ToJson`/`*FromJson`; the transport
//! glue (`ClientError`, the `EmbedError` decode, `NetClient` over the
//! `node:http` stdlib transport) is a fixed template plus one typed
//! method per `implemented` `call` op in the `methods` table.
//!
//! Deterministic output — the same schema always produces byte-identical
//! bytes, so the CI drift check is stable (CC7). The artifact targets
//! node ≥ 22's type stripping: **erasable syntax only** (interfaces,
//! type aliases, `as` — no enums, namespaces, parameter properties or
//! decorators), the `node:http` stdlib transport only, no npm
//! dependencies. `fetch` is deliberately NOT used: undici always appends
//! `Sec-Fetch-Mode`, which the binding-(c) gate's P3/P4 posture refuses
//! (R4.2, found by the executed leg). `scripts/e3-smoke.sh` +
//! `clients/ts/smoke.ts` are the executable leg.

use hh_wire::json::Json;
use std::fmt::Write as _;

const HEADER: &str = "\
// generated — do not edit.
//
// Source of truth: the `hh-embed-schema` crate (the single schema source, CC7). Regenerate
// with `cargo run -p hh-codegen -- --root <repo>`; `scripts/check-drift.sh` fails the build
// if this file drifts from the schema. Runs under node >= 22 (type stripping — erasable
// syntax only), the `node:http` stdlib transport only, no npm dependencies. A generated
// client contains no runtime logic beyond the transport binding (ADR-0179): it asserts `(contract_major,
// schema_hash)` at `hello` and lowers a mismatch to a typed error — never a silent
// fallback (ADR-0178 D2).
";

/// Emit the full `hh-embed.ts` for `clients/ts/` — the E3 binding.
pub fn generated_ts_client(schema: &Json, schema_hash: &str) -> Result<String, String> {
    let contract_major = schema
        .get("contract_major")
        .and_then(Json::as_int)
        .ok_or("schema missing contract_major")?;

    let mut s = String::new();
    s.push_str(HEADER);
    s.push('\n');
    s.push_str(PRELUDE);

    writeln!(
        s,
        "/// The contract major this client was generated against.\nexport const CONTRACT_MAJOR = {contract_major};\n"
    )
    .unwrap();
    writeln!(
        s,
        "/// The schema content address this client was generated against.\nexport const EXPECTED_SCHEMA_HASH = {schema_hash:?};\n"
    )
    .unwrap();

    let types = match schema.get("types") {
        Some(Json::Obj(m)) => m,
        _ => return Err("schema missing types".into()),
    };
    for (name, def) in types {
        emit_type(&mut s, name, def)?;
    }
    if let Some(Json::Obj(frames)) = schema.get("frames") {
        for (name, def) in frames {
            emit_type(&mut s, name, def)?;
        }
    }

    emit_error_sum(&mut s, schema)?;
    emit_client(&mut s, schema)?;
    Ok(s)
}

// ── Type emission ───────────────────────────────────────────────────────

fn emit_type(out: &mut String, name: &str, def: &Json) -> Result<(), String> {
    match def.get("kind").and_then(Json::as_str) {
        Some("struct") => emit_struct(out, name, def),
        Some("string_enum") => emit_string_enum(out, name, def),
        Some("tagged_sum") => emit_tagged_sum(out, name, def),
        other => Err(format!("type {name}: unknown kind {other:?}")),
    }
}

fn fields_of(def: &Json) -> Result<&Vec<Json>, String> {
    match def.get("fields") {
        Some(Json::Arr(v)) => Ok(v),
        _ => Err("fields not an array".into()),
    }
}

fn field_parts(f: &Json) -> Result<(String, String, bool), String> {
    let name = f
        .get("name")
        .and_then(Json::as_str)
        .ok_or("field missing name")?
        .to_string();
    let ty = f
        .get("type")
        .and_then(Json::as_str)
        .ok_or("field missing type")?
        .to_string();
    let required = matches!(f.get("required"), Some(Json::Bool(true)));
    Ok((name, ty, required))
}

/// `snake_name` → `SnakeName`; mirrors `emit::pascal` (schema type names
/// are already PascalCase — this is the same normalization both ways).
fn pascal(s: &str) -> String {
    let mut out = String::new();
    for part in s.split('_') {
        let mut c = part.chars();
        if let Some(f) = c.next() {
            out.extend(f.to_uppercase());
            out.push_str(c.as_str());
        }
    }
    out
}

/// `openSessionParams` — the codec-function prefix for a type name.
fn camel(s: &str) -> String {
    let p = pascal(s);
    let mut c = p.chars();
    match c.next() {
        Some(f) => format!("{}{}", f.to_lowercase(), c.as_str()),
        None => p,
    }
}

/// The TS type for a schema type-expression. Optionality is the member
/// `?`, never part of the type (mirrors `emit::rust_type`'s split).
fn ts_base(ty: &str) -> String {
    match ty {
        "string" => "string".into(),
        "integer" => "number".into(),
        "bool" => "boolean".into(),
        "json" => "Json".into(),
        t if t.starts_with('[') && t.ends_with(']') => {
            format!("{}[]", ts_base(&t[1..t.len() - 1]))
        }
        "map<string,json>" => "JsonObj".into(),
        named => pascal(named),
    }
}

/// Wire field names that are TS/JS reserved words get a trailing `_`
/// (`ref` is legal; `type`, `var`, `function`, `new` members are quoted
/// on encode anyway — the binding name must still parse). The wire key
/// is unaffected — only the TS member name.
fn tsident(name: &str) -> String {
    match name {
        "class" | "delete" | "export" | "extends" | "function" | "import" | "in" | "new"
        | "super" | "this" | "typeof" | "var" | "void" | "with" | "yield" | "enum"
        | "implements" | "interface" | "package" | "private" | "protected" | "public"
        | "static" | "let" | "const" | "null" | "true" | "false" | "if" | "else" | "do"
        | "while" | "for" | "switch" | "case" | "default" | "break" | "continue" | "return"
        | "throw" | "try" | "catch" | "finally" | "instanceof" | "debugger" => {
            format!("{name}_")
        }
        _ => name.to_string(),
    }
}

/// `member` access on `v` with the TS-safe binding name.
fn access(name: &str) -> String {
    let id = tsident(name);
    if is_plain_ident(&id) {
        format!("v.{id}")
    } else {
        format!("v[{id:?}]")
    }
}

fn is_plain_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .next()
            .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && s.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// The encode expression for a value of schema type `ty` held in `a`.
fn ts_to_json_expr(a: &str, ty: &str) -> String {
    match ty {
        "string" | "integer" | "bool" | "json" | "map<string,json>" => a.to_string(),
        t if t.starts_with('[') && t.ends_with(']') => {
            let inner = &t[1..t.len() - 1];
            format!("{a}.map((x) => {})", ts_to_json_expr("x", inner))
        }
        named => format!("{}ToJson({a})", camel(named)),
    }
}

/// The decode expression for a `Json` value `e` into schema type `ty`.
fn ts_decode_expr(e: &str, ty: &str) -> String {
    match ty {
        "string" => format!("needStr({e})"),
        "integer" => format!("needInt({e})"),
        "bool" => format!("needBool({e})"),
        "json" => e.to_string(),
        t if t.starts_with('[') && t.ends_with(']') => {
            let inner = &t[1..t.len() - 1];
            format!("needArr({e}).map((x) => {})", ts_decode_expr("x", inner))
        }
        "map<string,json>" => format!("needObj({e})"),
        named => format!("{}FromJson({e})", camel(named)),
    }
}

/// Read member `name` out of `o` (a `JsonObj`) into `ty` — mirrors
/// `emit::read_field`'s required/optional lattice (absent⇒false,
/// absent⇒`[]`, absent⇒`{}`, absent⇒`undefined`).
fn ts_read_field(name: &str, ty: &str, required: bool) -> String {
    if required {
        return ts_decode_expr(&format!("need(o, {name:?})"), ty);
    }
    match ty {
        "bool" => format!("o[{name:?}] === true"),
        t if t.starts_with('[') => {
            format!(
                "o[{name:?}] === undefined ? [] : {}",
                ts_decode_expr(&format!("o[{name:?}]"), ty)
            )
        }
        "map<string,json>" => format!("isObj(o[{name:?}]) ? o[{name:?}] as JsonObj : {{}}"),
        _ => format!(
            "o[{name:?}] === undefined ? undefined : {}",
            ts_decode_expr(&format!("o[{name:?}]"), ty)
        ),
    }
}

/// One `o[...] = …` encode statement — mirrors `emit::push_field`.
fn ts_push_field(name: &str, ty: &str, required: bool) -> String {
    let a = access(name);
    if required {
        return format!("o[{name:?}] = {};", ts_to_json_expr(&a, ty));
    }
    match ty {
        // absent⇒false: the wire always carries a real bool (the Rust
        // emitter pushes `Json::Bool` unconditionally — same bytes).
        "bool" => format!("o[{name:?}] = {a} === true;"),
        t if t.starts_with('[') => {
            format!(
                "o[{name:?}] = ({a} ?? []).map((x) => {});",
                ts_to_json_expr("x", &t[1..t.len() - 1])
            )
        }
        "map<string,json>" => {
            format!("if ({a} !== undefined && Object.keys({a}).length > 0) o[{name:?}] = {a};")
        }
        _ => format!(
            "if ({a} !== undefined) o[{name:?}] = {};",
            ts_to_json_expr(&a, ty)
        ),
    }
}

fn emit_struct(out: &mut String, name: &str, def: &Json) -> Result<(), String> {
    let name = pascal(name);
    let fields = fields_of(def)?;
    writeln!(out, "export interface {name} {{").unwrap();
    for f in fields {
        let (fname, ty, required) = field_parts(f)?;
        let opt = if required { "" } else { "?" };
        writeln!(out, "  {}{}: {},", tsident(&fname), opt, ts_base(&ty)).unwrap();
    }
    out.push_str("}\n\n");

    let fnc = camel(&name);
    writeln!(out, "export function {fnc}ToJson(v: {name}): Json {{").unwrap();
    out.push_str("  const o: JsonObj = {};\n");
    for f in fields {
        let (fname, ty, required) = field_parts(f)?;
        writeln!(out, "  {}", ts_push_field(&fname, &ty, required)).unwrap();
    }
    out.push_str("  return o;\n}\n\n");

    writeln!(out, "export function {fnc}FromJson(v: Json): {name} {{").unwrap();
    out.push_str("  const o = needObj(v);\n  return {\n");
    for f in fields {
        let (fname, ty, required) = field_parts(f)?;
        writeln!(
            out,
            "    {}: {},",
            tsident(&fname),
            ts_read_field(&fname, &ty, required)
        )
        .unwrap();
    }
    out.push_str("  };\n}\n\n");
    Ok(())
}

fn emit_string_enum(out: &mut String, name: &str, def: &Json) -> Result<(), String> {
    let name = pascal(name);
    let variants = match def.get("variants") {
        Some(Json::Arr(v)) => v,
        _ => return Err(format!("{name}: variants not an array")),
    };
    let members: Vec<String> = variants
        .iter()
        .map(|v| format!("{:?}", v.as_str().unwrap_or("?")))
        .collect();
    writeln!(out, "export type {name} = {};", members.join(" | ")).unwrap();
    let fnc = camel(&name);
    writeln!(
        out,
        "export function {fnc}ToJson(v: {name}): Json {{ return v; }}"
    )
    .unwrap();
    writeln!(out, "export function {fnc}FromJson(v: Json): {name} {{").unwrap();
    out.push_str("  switch (v) {\n");
    for v in variants {
        let s = v.as_str().unwrap();
        writeln!(out, "    case {s:?}: return v;").unwrap();
    }
    writeln!(
        out,
        "    default: return fail(`unknown {name} variant ${{canon(v)}}`);\n  }}\n}}\n"
    )
    .unwrap();
    Ok(())
}

fn emit_tagged_sum(out: &mut String, name: &str, def: &Json) -> Result<(), String> {
    let name = pascal(name);
    let tag = def.get("tag").and_then(Json::as_str).unwrap_or("kind");
    let variants = match def.get("variants") {
        Some(Json::Arr(v)) => v,
        _ => return Err(format!("{name}: variants not an array")),
    };

    writeln!(out, "export type {name} =").unwrap();
    for v in variants {
        let vname = v
            .get("name")
            .and_then(Json::as_str)
            .ok_or("variant missing name")?;
        let fs = fields_of(v)?;
        write!(out, "  | {{ {tag}: {vname:?}").unwrap();
        for f in fs {
            let (fname, ty, required) = field_parts(f)?;
            let opt = if required { "" } else { "?" };
            write!(out, "; {}{}: {}", tsident(&fname), opt, ts_base(&ty)).unwrap();
        }
        out.push_str(" }\n");
    }
    out.push('\n');

    let fnc = camel(&name);
    writeln!(out, "export function {fnc}ToJson(v: {name}): Json {{").unwrap();
    writeln!(out, "  const o: JsonObj = {{}};\n  switch (v[{tag:?}]) {{").unwrap();
    for v in variants {
        let vname = v.get("name").and_then(Json::as_str).unwrap();
        let fs = fields_of(v)?;
        writeln!(out, "    case {vname:?}: {{").unwrap();
        writeln!(out, "      o[{tag:?}] = {vname:?};").unwrap();
        for f in fs {
            let (fname, ty, required) = field_parts(f)?;
            writeln!(out, "      {}", ts_push_field(&fname, &ty, required)).unwrap();
        }
        out.push_str("      return o;\n    }\n");
    }
    out.push_str("  }\n}\n\n");

    writeln!(out, "export function {fnc}FromJson(v: Json): {name} {{").unwrap();
    out.push_str("  const o = needObj(v);\n");
    writeln!(
        out,
        "  const tag = needStr(need(o, {tag:?}));\n  switch (tag) {{"
    )
    .unwrap();
    for v in variants {
        let vname = v.get("name").and_then(Json::as_str).unwrap();
        let fs = fields_of(v)?;
        write!(out, "    case {vname:?}: return {{ {tag}: {vname:?}").unwrap();
        for f in fs {
            let (fname, ty, required) = field_parts(f)?;
            write!(
                out,
                ", {}: {}",
                tsident(&fname),
                ts_read_field(&fname, &ty, required)
            )
            .unwrap();
        }
        out.push_str(" };\n");
    }
    writeln!(
        out,
        "    default: return fail(`unknown {name} variant ${{canon(tag)}}`);\n  }}\n}}\n"
    )
    .unwrap();
    Ok(())
}

// ── Error sum emission ──────────────────────────────────────────────────

/// `EmbedError` + `errorCodeFor`/`errorRetryable`/`decodeError` — the
/// closed error sum, mirroring `emit::emit_error_sum` +
/// `lib.rs::decode_error_net`.
fn emit_error_sum(out: &mut String, schema: &Json) -> Result<(), String> {
    let errors = match schema.get("errors") {
        Some(Json::Arr(v)) => v,
        _ => return Err("schema missing errors".into()),
    };
    out.push_str(
        "/// The closed `hh-embed/1` error sum — decoded from `error.data`\n\
         /// (`kind` tag + members). A variant absent from the table is\n\
         /// impossible by construction (CC1; a response-position sum grows\n\
         /// only by a dialect bump).\n\
         export interface EmbedError {\n  code: number;\n  kind: string;\n  retryable: boolean;\n  message: string;\n  /// The typed `error.data` members (`kind`/`retryable` re-keyed out).\n  data: Json;\n}\n\n",
    );
    out.push_str(
        "/// `kind` → code, for client-side construction in tests/fixtures.\n\
         export function errorCodeFor(kind: string): number | undefined {\n  switch (kind) {\n",
    );
    for e in errors {
        let kind = e.get("kind").and_then(Json::as_str).unwrap();
        let code = e.get("code").and_then(Json::as_int).unwrap();
        writeln!(out, "    case {kind:?}: return {code};").unwrap();
    }
    out.push_str("    default: return undefined;\n  }\n}\n\n");
    out.push_str(
        "/// Whether `kind` is retryable (safe to retry with the same\n\
         /// `idempotency_key`).\n\
         export function errorRetryable(kind: string): boolean {\n  return kind === \"WouldBlock\" || kind === \"Draining\" || kind === \"Overloaded\" || kind === \"Disconnected\" || kind === \"Timeout\";\n}\n\n",
    );
    out.push_str(
        "/// Decode a JSON-RPC `error` object into the typed `EmbedError`.\n\
         function decodeError(err: Json): EmbedError {\n  const o = needObj(err);\n  const data = isObj(o[\"data\"]) ? o[\"data\"] as JsonObj : {};\n  const kind = typeof data[\"kind\"] === \"string\" ? data[\"kind\"] as string : \"Unknown\";\n  const retryable = typeof data[\"retryable\"] === \"boolean\" ? data[\"retryable\"] as boolean : errorRetryable(kind);\n  return {\n    code: typeof o[\"code\"] === \"number\" ? o[\"code\"] as number : 0,\n    kind,\n    retryable,\n    message: typeof o[\"message\"] === \"string\" ? o[\"message\"] as string : \"\",\n    data: o[\"data\"] === undefined ? null : o[\"data\"],\n  };\n}\n\n",
    );
    Ok(())
}

// ── Client glue emission ────────────────────────────────────────────────

/// `NetClient` (the binding-(c) `node:http` transport) + one typed method per
/// `implemented` `call` op — the same op surface the generated Rust
/// client carries.
fn emit_client(out: &mut String, schema: &Json) -> Result<(), String> {
    out.push_str(CLIENT_HEAD);

    let methods = match schema.get("methods") {
        Some(Json::Obj(m)) => m,
        _ => return Err("schema missing methods".into()),
    };
    // Implemented calls, in registry (BTreeMap = sorted) order.
    let mut emitted = Vec::new();
    for (mname, mdef) in methods {
        let implemented = matches!(mdef.get("implemented"), Some(Json::Bool(true)));
        let direction = mdef.get("direction").and_then(Json::as_str).unwrap_or("");
        if implemented && direction == "call" {
            emitted.push((mname.clone(), mdef));
        }
    }
    for (mname, mdef) in emitted {
        let params = mdef.get("params").and_then(Json::as_str).unwrap_or("json");
        let result = mdef.get("result").and_then(Json::as_str).unwrap_or("json");
        emit_method(out, &mname, params, result)?;
    }
    out.push_str(CLIENT_TAIL);
    Ok(())
}

fn emit_method(
    out: &mut String,
    mname: &str,
    params_ty: &str,
    result_ty: &str,
) -> Result<(), String> {
    let params_ts = if params_ty == "json" {
        "Json".to_string()
    } else {
        pascal(params_ty)
    };
    let result_ts = if result_ty == "json" {
        "Json".to_string()
    } else {
        pascal(result_ty)
    };
    let params_expr = if params_ty == "json" {
        "params".to_string()
    } else {
        format!("{}ToJson(params)", camel(params_ty))
    };
    let result_expr = if result_ty == "json" {
        "return raw;".to_string()
    } else if mname == "hello" {
        // `hello` asserts the kernel's `(contract_major, schema_hash)`
        // against the constants this client was generated against, then
        // stores the negotiated result — a mismatch lowers to a typed
        // error, never a silent fallback (ADR-0178 D2).
        "const r = helloResultFromJson(raw); if (r.kernel.contract_major !== CONTRACT_MAJOR) throw new ClientError(\"identity_mismatch\", \"contract_major mismatch\", undefined, \"contract_major\"); if (r.kernel.schema_hash !== EXPECTED_SCHEMA_HASH) throw new ClientError(\"identity_mismatch\", \"schema_hash mismatch\", undefined, \"schema_hash\"); this.hello_result = r; return r;".to_string()
    } else {
        format!("return {}FromJson(raw);", camel(result_ty))
    };
    writeln!(
        out,
        "  /// `{mname}` → `{result_ty}` (see the contract registry)."
    )
    .unwrap();
    writeln!(
        out,
        "  async {}(params: {params_ts}): Promise<{result_ts}> {{",
        mname.replace('.', "_")
    )
    .unwrap();
    writeln!(
        out,
        "    const raw = await this.call({mname:?}, {params_expr});"
    )
    .unwrap();
    writeln!(out, "    {result_expr}").unwrap();
    out.push_str("  }\n\n");
    Ok(())
}

/// The transport prelude: `Json`/`JsonObj`, `canon` (the one wire
/// spelling — CC1), the `need*` decode guards, `fail`, `ClientError`,
/// and the `NetClient` head through `call`/`drain`/buffer drains.
const PRELUDE: &str = r##"import { request } from "node:http";

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
/// spelling `hh_wire::json::Json::to_canonical_string` emits: keys sorted
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

"##;

const CLIENT_HEAD: &str = r##"/// The binding-(c) client (the `local_network` surface transport —
/// §7.2; S4.10): the same canonical request/response contract over
/// loopback HTTP — `POST /hh-embed/1` is one JSON-RPC request →
/// response, and `GET /hh-embed/1/events?wait_ms=` drains `stream.frame`
/// + `upcall.*` notifications as NDJSON. It speaks the exact wire shape
/// `hh-embed`'s (c) server emits (bearer + exact-Host + no
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

"##;

/// The tail after typed methods: `negotiated` accessor + the impl
/// close + `verifyDurableHash` (the AC-R-2.11.4-3 client-side
/// durable-hash check).
const CLIENT_TAIL: &str = r##"  /// The negotiated capabilities (from the completed `hello`).
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
"##;
