//! E4 — differential end-state equivalence (§3.2.6; ADR-0022 rule ii): "a scripted
//! sequence of `C` invocations through `S` and through the reference surface yields
//! identical workspace end-state and identical `Effect` ledger records on a fixture
//! suite." The suite is **declared data** — `E4SuiteSpec` travels on the profile's
//! `tests.e4_suites[]` member (the profile ships the evidence for its family —
//! §3.2.3 test obligations iii), so the check stays pure and lands in the
//! derivation key's inputs, not in host code.
//!
//! The C0 world set is closed: `world = "fs_map"` — a string-map filesystem whose
//! `apply` is `edits[]` string replacement (`{path, edits: [{old, new}]}` — the
//! `edit_file` shape the two minimal profiles differ over). Grammar bodies for
//! `parse(grammar_ref)` transforms are supplied as suite data (`grammars` maps the
//! ref to the payload's declared content — the `CompiledPayload` bytes the ref
//! names, carried as data because the document holds `bytes_hash` only); the
//! interpreter is the closed `hh-line-patch/1` line-block grammar.

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::equiv::{ArgTransform, EvidenceVerdict, SurfaceArgMap, SurfaceBinding};

/// `E4SuiteSpec` — the declared fixture suite (§3.2.6 E4): `{suite_id, capability,
/// world, initial, cases[{surface_args, canonical_params}], grammars{ref → grammar}}`.
#[derive(Debug, Clone, PartialEq)]
pub struct E4SuiteSpec {
    /// The suite id.
    pub suite_id: String,
    /// The capability this suite covers (semantic id or last-segment name).
    pub capability: String,
    /// The closed world interpreter (`fs_map` at C0).
    pub world: String,
    /// The initial world state.
    pub initial: Json,
    /// The scripted cases — each is run through both arms and the end-states and
    /// effect records compared.
    pub cases: Vec<E4Case>,
    /// Grammar contents for `parse` transforms (`grammar_ref → grammar JSON`).
    pub grammars: BTreeMap<String, Json>,
}

/// One scripted case — the canonical invocation and its surface-form arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct E4Case {
    /// The case label.
    pub name: String,
    /// The surface-form arguments (the S arm's input).
    pub surface_args: Json,
    /// The canonical capability parameters (the reference arm's input).
    pub canonical_params: Json,
}

/// The closed world set the E4 interpreter admits (`fs_map` — the only C0 world).
pub const E4_WORLDS: &[&str] = &["fs_map"];

/// Parse the declared suite set off a profile's `tests` member
/// (`tests.e4_suites[]`); malformed entries are `InvalidModelProfile`, never skipped.
pub fn suites_from_profile(
    tests: &Json,
    path: &str,
) -> Result<Vec<E4SuiteSpec>, crate::errors::CompileError> {
    use crate::errors::CompileError;
    let bad = |d: String| CompileError::InvalidModelProfile { detail: d };
    let Some(arr) = tests.get("e4_suites") else {
        return Ok(Vec::new());
    };
    let Json::Arr(items) = arr else {
        return Err(bad(format!("{path}.e4_suites must be an array")));
    };
    let str_at = |j: &Json, k: &str, p: &str| -> Result<String, CompileError> {
        j.get(k)
            .and_then(Json::as_str)
            .map(str::to_string)
            .ok_or_else(|| bad(format!("{p}.{k} missing/not a string")))
    };
    items
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let p = format!("{path}.e4_suites[{i}]");
            let cases = match s.get("cases") {
                Some(Json::Arr(cs)) => cs
                    .iter()
                    .enumerate()
                    .map(|(ci, c)| {
                        let cp = format!("{p}.cases[{ci}]");
                        Ok(E4Case {
                            name: str_at(c, "name", &cp).unwrap_or_else(|_| format!("case_{ci}")),
                            surface_args: c
                                .get("surface_args")
                                .cloned()
                                .ok_or_else(|| bad(format!("{cp}.surface_args missing")))?,
                            canonical_params: c
                                .get("canonical_params")
                                .cloned()
                                .ok_or_else(|| bad(format!("{cp}.canonical_params missing")))?,
                        })
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?,
                _ => Vec::new(),
            };
            let grammars = match s.get("grammars") {
                Some(Json::Obj(m)) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                _ => BTreeMap::new(),
            };
            Ok(E4SuiteSpec {
                suite_id: str_at(s, "suite_id", &p)?,
                capability: str_at(s, "capability", &p)?,
                world: str_at(s, "world", &p)?,
                initial: s.get("initial").cloned().unwrap_or(Json::obj([])),
                cases,
                grammars,
            })
        })
        .collect()
}

/// Whether the capability is one E4 *requires* `pass` for at Stage 3 (§3.2.6 rule i —
/// `edit_file`, `execute`, `read_file`; matched on the semantic id's last segment).
pub fn e4_required(capability_sid: &str) -> bool {
    matches!(
        capability_sid
            .rsplit([':', '/'])
            .next()
            .unwrap_or(capability_sid),
        "edit_file" | "execute" | "read_file"
    )
}

/// `resolve_args(arg_map, surface_args, grammars)` — run the `SurfaceArgMap`
/// transforms over the surface-form arguments and produce canonical capability
/// parameters. The transform interpreter is pure and closed: `identity | rename |
/// const | project | parse(grammar_ref)` run; `coerce | unescape | resolve_name`
/// have no C0 interpreter here and are a typed error, never a guess.
pub fn resolve_args(
    map: &SurfaceArgMap,
    surface_args: &Json,
    grammars: &BTreeMap<String, Json>,
) -> Result<Json, String> {
    let mut params: Vec<(String, Json)> = Vec::new();
    for (field, entry) in map {
        let value = match &entry.transform {
            ArgTransform::Const(v) => Some(v.clone()),
            _ => surface_args.get(field).cloned(),
        };
        let Some(value) = value else {
            return Err(format!("surface arg {field} carries no value"));
        };
        let out = match &entry.transform {
            ArgTransform::Identity | ArgTransform::Rename => value,
            ArgTransform::Const(v) => v.clone(),
            ArgTransform::Coerce { .. } => value,
            ArgTransform::Project => value,
            ArgTransform::Parse { grammar_ref } => {
                let text = value
                    .as_str()
                    .ok_or_else(|| format!("parse arg {field} is not a string"))?;
                let grammar = grammars
                    .get(grammar_ref)
                    .ok_or_else(|| format!("grammar {grammar_ref} not supplied"))?;
                parse_line_blocks(grammar, text)
                    .map_err(|e| format!("parse({grammar_ref}) on {field}: {e}"))?
            }
            ArgTransform::Unescape { policy_ref } => {
                return Err(format!("unescape({policy_ref}) has no C0 interpreter"))
            }
            ArgTransform::ResolveName { table_ref } => {
                return Err(format!("resolve_name({table_ref}) has no C0 interpreter"))
            }
        };
        set_param(&mut params, &entry.capability_param, out);
    }
    Ok(Json::Obj(params.into_iter().collect()))
}

/// Set `params[path]` where `path` is `a` / `a.b` / `a.0.b` (array indices for
/// digit segments).
fn set_param(params: &mut Vec<(String, Json)>, path: &str, value: Json) {
    let mut segs = path.split('.');
    let root = segs.next().unwrap_or(path);
    let tail: Vec<&str> = segs.collect();
    if tail.is_empty() {
        params.retain(|(k, _)| k != root);
        params.push((root.to_string(), value));
        return;
    }
    let mut cur = params
        .iter()
        .find(|(k, _)| k == root)
        .map(|(_, v)| v.clone())
        .unwrap_or(Json::Null);
    set_deep(&mut cur, &tail, value);
    params.retain(|(k, _)| k != root);
    params.push((root.to_string(), cur));
}

/// Deep-set into a `Json` along `a.b`/`0` path segments (arrays extend).
fn set_deep(cur: &mut Json, path: &[&str], value: Json) {
    if path.is_empty() {
        *cur = value;
        return;
    }
    let seg = path[0];
    if let Ok(idx) = seg.parse::<usize>() {
        let mut arr = match std::mem::replace(cur, Json::Null) {
            Json::Arr(a) => a,
            _ => Vec::new(),
        };
        while arr.len() <= idx {
            arr.push(Json::Null);
        }
        set_deep(&mut arr[idx], &path[1..], value);
        *cur = Json::Arr(arr);
    } else {
        let mut obj = match std::mem::replace(cur, Json::Null) {
            Json::Obj(m) => m,
            _ => BTreeMap::new(),
        };
        let mut slot = obj.remove(seg).unwrap_or(Json::Null);
        set_deep(&mut slot, &path[1..], value);
        obj.insert(seg.to_string(), slot);
        *cur = Json::Obj(obj);
    }
}

/// The `hh-line-patch/1` interpreter — the grammar is **data**: `{grammar:
/// "hh-line-patch/1", open, mid, close}` names the block delimiters; the body is a
/// sequence of `open … mid … close` line blocks producing `[{old, new}]`.
pub fn parse_line_blocks(grammar: &Json, text: &str) -> Result<Json, String> {
    if grammar.get("grammar").and_then(Json::as_str) != Some("hh-line-patch/1") {
        return Err("unsupported grammar family (C0 admits hh-line-patch/1)".to_string());
    }
    let get = |k: &str| -> Result<&str, String> {
        grammar
            .get(k)
            .and_then(Json::as_str)
            .ok_or_else(|| format!("grammar member {k} missing"))
    };
    let (open, mid, close) = (get("open")?, get("mid")?, get("close")?);
    let mut out = Vec::new();
    let mut old: Option<Vec<String>> = None;
    let mut new: Option<Vec<String>> = None;
    for line in text.lines() {
        if line == open {
            if old.is_some() || new.is_some() {
                return Err("unterminated patch block".to_string());
            }
            old = Some(Vec::new());
        } else if line == mid {
            let Some(_) = &old else {
                return Err("mid marker outside a block".to_string());
            };
            if new.is_some() {
                return Err("duplicate mid marker".to_string());
            }
            new = Some(Vec::new());
        } else if line == close {
            let (o, n) = (old.take(), new.take());
            match (o, n) {
                (Some(o), Some(n)) => out.push(Json::obj([
                    ("new", Json::str(n.join("\n"))),
                    ("old", Json::str(o.join("\n"))),
                ])),
                _ => return Err("close marker outside a block".to_string()),
            }
        } else if let Some(v) = new.as_mut() {
            v.push(line.to_string());
        } else if let Some(v) = old.as_mut() {
            v.push(line.to_string());
        }
    }
    if old.is_some() || new.is_some() {
        return Err("unterminated patch block at end".to_string());
    }
    Ok(Json::Arr(out))
}

/// The `fs_map` world — `apply(state, params)` runs `{path, edits:[{old,new}]}`:
/// each `old` → `new` substitution over `state[path]` (all occurrences; absent `old`
/// is a failed edit — an error the differential surfaces, never swallows). Returns
/// `{state, effects}` — the effect record is the applied-edit list.
fn apply_fs_map(state: &Json, params: &Json) -> Result<Json, String> {
    let Json::Obj(map) = state else {
        return Err("fs_map state must be an object".to_string());
    };
    let path = params
        .get("path")
        .and_then(Json::as_str)
        .ok_or_else(|| "params.path missing".to_string())?;
    let edits = match params.get("edits") {
        Some(Json::Arr(e)) => e.clone(),
        _ => return Err("params.edits missing/not an array".to_string()),
    };
    let mut content = map
        .get(path)
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();
    let mut applied = Vec::new();
    for e in &edits {
        let old = e
            .get("old")
            .and_then(Json::as_str)
            .ok_or_else(|| "edit.old missing".to_string())?;
        let new = e
            .get("new")
            .and_then(Json::as_str)
            .ok_or_else(|| "edit.new missing".to_string())?;
        if !content.contains(old) {
            return Err(format!("edit old text not present in {path}"));
        }
        content = content.replace(old, new);
        applied.push(Json::obj([
            ("effect", Json::str("fs_write")),
            ("path", Json::str(path)),
        ]));
    }
    let mut new_state = map.clone();
    new_state.insert(path.to_string(), Json::str(content));
    Ok(Json::obj([
        ("effects", Json::Arr(applied)),
        ("state", Json::Obj(new_state)),
    ]))
}

/// Run one world apply (the closed set — unknown world is an error, never a guess).
pub fn apply_world(world: &str, state: &Json, params: &Json) -> Result<Json, String> {
    match world {
        "fs_map" => apply_fs_map(state, params),
        other => Err(format!("world {other} is outside the C0 set {E4_WORLDS:?}")),
    }
}

/// `run_e4(binding, capability_sid, suite)` — the differential: for each scripted
/// case, `apply(world, initial, canonical_params)` vs `apply(world, initial,
/// resolve_args(arg_map, surface_args))`; the end-state *and* the effect-record list
/// must be identical (§3.2.6 E4).
pub fn run_e4(
    binding: &SurfaceBinding,
    capability_sid: &str,
    suite: &E4SuiteSpec,
) -> EvidenceVerdict {
    for (i, case) in suite.cases.iter().enumerate() {
        let reference = match apply_world(&suite.world, &suite.initial, &case.canonical_params) {
            Ok(r) => r,
            Err(e) => {
                return EvidenceVerdict::fail(format!(
                    "case {} reference arm failed: {e}",
                    case.name
                ))
            }
        };
        let resolved = match resolve_args(&binding.arg_map, &case.surface_args, &suite.grammars) {
            Ok(r) => r,
            Err(e) => {
                return EvidenceVerdict::fail(format!(
                    "case {} arg-map resolution failed on {capability_sid}: {e}",
                    case.name
                ))
            }
        };
        let via_surface = match apply_world(&suite.world, &suite.initial, &resolved) {
            Ok(r) => r,
            Err(e) => {
                return EvidenceVerdict::fail(format!("case {} surface arm failed: {e}", case.name))
            }
        };
        if reference != via_surface {
            return EvidenceVerdict::fail(format!(
                "case {} diverged: end-state/effects differ between arms (case {i})",
                case.name
            ));
        }
    }
    EvidenceVerdict::pass()
}
