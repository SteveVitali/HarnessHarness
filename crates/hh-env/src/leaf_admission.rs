//! The `labels_leaves` per-leaf admission walk (§5g.2 §2; ADR-0054 D1;
//! DF-S2.7-1 member (a); R-2.8.2).
//!
//! A `FlowContract{labels_leaves: true}` capability mints a per-leaf
//! `ProvenanceRecord` for every leaf of the shaped result — one record per
//! leaf *position* the declared `output_schema` enumerates, each carrying
//! the contract's admitted label `L(r)` (the leaf's declared label — no
//! per-leaf annotation dialect exists at this stage; a leaf-specific bound
//! would be a schema-annotation member, an owned decision this slice does
//! not take). The walk is the admission gate: every result leaf must sit
//! under a declared schema leaf position — an uncovered leaf is
//! **non-admissible** and the result is refused typed
//! (`conflict{admission_leaf}` on the `observed` row —
//! `action.effect.refused` is lifecycle-illegal post-`committed`, so the
//! typed refusal rides the observation plane's error class, the channel
//! `conflict{schema}` already owns).
//!
//! Enumeration is schema-guided and bounded: `properties`/`items`/
//! `additionalProperties` recursion over the OQ-219 interim keyword subset
//! (CC1 — the one admitted list, shared with `conforms`); any other keyword
//! makes the leaf grammar unreadable and the walk refuses rather than guess
//! (T-LCD-15).

use hh_wire::json::Json;

/// The leaf-walk budget — per-leaf records cost one payload member each
/// (OQ-146's capacity question); a result past the bound is non-admissible
/// rather than silently truncated.
pub const MAX_RESULT_LEAVES: usize = 1024;
/// The recursion bound — a degenerate schema/value pair never walks
/// unbounded.
const MAX_DEPTH: usize = 64;

/// The per-leaf walk's failure sum — each arm is the typed refusal's
/// `conflict{kind}` detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeafWalkFailure {
    /// `labels_leaves` ran with no shaped result (no `structured_result`
    /// capture and the raw output is not the result value) — the leaf
    /// grammar has nothing to bind.
    NoShapedResult,
    /// The capability declares no `output_schema` — the leaf grammar is
    /// absent entirely (distinct from an unreadable grammar).
    NoLeafGrammar,
    /// The shaped result's bytes are unreadable or not canonical JSON.
    MalformedResult,
    /// A schema node carries a keyword outside the OQ-219 admitted subset —
    /// the leaf grammar cannot be enumerated honestly.
    UncheckedKeyword(String),
    /// A result leaf position no declared schema leaf covers
    /// (`additionalProperties` member / `items`-less array element / a
    /// composite value under a leaf schema).
    UncoveredLeaf(String),
    /// The leaf or depth budget was exceeded — never a silent truncation.
    LeafBudgetExceeded,
}

impl LeafWalkFailure {
    /// The typed refusal's `conflict{kind}` spelling — names the failing
    /// leaf/keyword where one exists.
    pub fn kind(&self) -> String {
        match self {
            LeafWalkFailure::NoShapedResult => "admission_leaf:no_shaped_result".to_string(),
            LeafWalkFailure::NoLeafGrammar => "admission_leaf:no_leaf_grammar".to_string(),
            LeafWalkFailure::MalformedResult => "admission_leaf:malformed_result".to_string(),
            LeafWalkFailure::UncheckedKeyword(k) => {
                format!("admission_leaf:unchecked_keyword:{k}")
            }
            LeafWalkFailure::UncoveredLeaf(p) => format!("admission_leaf:uncovered:{p}"),
            LeafWalkFailure::LeafBudgetExceeded => "admission_leaf:budget".to_string(),
        }
    }
}

/// `walk_result_leaves(schema, value) → [leaf_path]` — enumerate the leaf
/// positions of the shaped result `value` under the declared `schema`. A
/// leaf is a position whose schema node is not `object`/`array`-structured
/// (the scalar slots per-leaf records mint against); object members walk
/// `properties[k]`, `additionalProperties: <schema>` admits the member
/// under that subschema, and any other undeclared member/element is an
/// `UncoveredLeaf`. Schema nodes carrying a keyword outside the admitted
/// subset refuse `UncheckedKeyword` — the walk never guesses a leaf grammar.
pub fn walk_result_leaves(schema: &Json, value: &Json) -> Result<Vec<String>, LeafWalkFailure> {
    let mut leaves = Vec::new();
    collect(schema, value, "", &mut leaves, 0)?;
    Ok(leaves)
}

fn collect(
    schema: &Json,
    value: &Json,
    path: &str,
    leaves: &mut Vec<String>,
    depth: usize,
) -> Result<(), LeafWalkFailure> {
    if depth > MAX_DEPTH || leaves.len() >= MAX_RESULT_LEAVES {
        return Err(LeafWalkFailure::LeafBudgetExceeded);
    }
    let Json::Obj(m) = schema else {
        // A non-object schema declares no leaf grammar for the position —
        // under `labels_leaves` the position is unlabelable.
        return Err(LeafWalkFailure::UncoveredLeaf(path.to_string()));
    };
    for k in m.keys() {
        if !hh_verification::validators::CONFORMANCE_ADMITTED_KEYWORDS.contains(&k.as_str()) {
            return Err(LeafWalkFailure::UncheckedKeyword(k.clone()));
        }
    }
    let structural = |k: &str| m.get(k).is_some();
    match value {
        Json::Obj(vm)
            if structural("properties")
                || matches!(m.get("type").and_then(Json::as_str), Some("object")) =>
        {
            let props = match m.get("properties") {
                Some(Json::Obj(p)) => Some(p),
                Some(_) => None,
                None => None,
            };
            for (k, v) in vm {
                let sub = props.and_then(|p| p.get(k));
                match sub {
                    Some(sub) => collect(sub, v, &join(path, k), leaves, depth + 1)?,
                    None => match m.get("additionalProperties") {
                        Some(sub @ Json::Obj(_)) => {
                            collect(sub, v, &join(path, k), leaves, depth + 1)?
                        }
                        // `false`/absent/other — the member sits under no
                        // declared leaf: non-admissible.
                        _ => return Err(LeafWalkFailure::UncoveredLeaf(join(path, k))),
                    },
                }
            }
            Ok(())
        }
        Json::Arr(vals)
            if structural("items")
                || matches!(m.get("type").and_then(Json::as_str), Some("array")) =>
        {
            match m.get("items") {
                Some(sub) => {
                    for (i, v) in vals.iter().enumerate() {
                        collect(sub, v, &join(path, &i.to_string()), leaves, depth + 1)?;
                    }
                    Ok(())
                }
                None => match vals.first() {
                    Some(_) => Err(LeafWalkFailure::UncoveredLeaf(join(path, "0"))),
                    None => Ok(()),
                },
            }
        }
        // A composite value at a leaf-typed schema position — the schema
        // declares no leaf structure the members could bind to.
        Json::Obj(vm) => match vm.keys().next() {
            Some(k) => Err(LeafWalkFailure::UncoveredLeaf(join(path, k))),
            None => Ok(()),
        },
        Json::Arr(vals) => match vals.first() {
            Some(_) => Err(LeafWalkFailure::UncoveredLeaf(join(path, "0"))),
            None => Ok(()),
        },
        // A scalar leaf position.
        _ => {
            leaves.push(path.to_string());
            Ok(())
        }
    }
}

fn join(path: &str, member: &str) -> String {
    if path.is_empty() {
        format!("/{member}")
    } else {
        format!("{path}/{member}")
    }
}
