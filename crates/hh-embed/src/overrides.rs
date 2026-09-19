//! `overrides[]` desugar + the I-1 authority gate (§7.1 I-1; ADR-0147 D2;
//! ADR-0168 D4; ADR-0025 `set` grammar).
//!
//! Every `overrides[]` list materialises as **exactly one** content-addressed
//! `experiment` layer — `overrides_layer_id = H(idp ∥ "override-layer" ∥
//! canonical(overrides))` (ADR-0147 D2 verbatim). The id is a function of the
//! override list alone: the same list always yields the same layer id
//! (AC-R-2.11.1-9), and the layer never changes the configuration identity
//! (ADR-0025 excludes `layers[]` from `semantic_id`).
//!
//! **The Stage-1 mount (interim — full `compose` is Stage 3, DF-S1.9-2).**
//! The tunable coordinate space the client's layer may name is
//! `/model`, `/mode`, `/thought_level`, `/profile` (ADR-0168 D4; the session
//! variables every participant carries). At Stage 1 these mount on the
//! resolved document's declared parameter surface — `assembly.values` for the
//! three named parameters and `assembly.profile_binding` for `/profile`. A
//! pointer outside that space applies literally as a JSON pointer into the
//! document: the patch pair `classify_pair(base, target)` then classifies
//! what the edit actually does (I-1's `HirDiff.classification`).
//!
//! **The gate.** `authority_delta = widening` or `budget_delta = loosening`
//! requires a human at the terminal — `attendance.value = "interactive"`
//! (declared or TTY-inferred). Anything less is refused
//! `AuthorityWideningRequiresHuman` **before any run opens** (AC-R-2.11.1-8).
//! An out-of-space pointer whose patch does not widen is a reach outside the
//! declared tunable surface — `AuthorityViolation{layer: "override"}` (the
//! S1.25 rule, preserved). A widening patch admitted at a TTY mints the layer
//! record with `provenance.origin = "human"` (I-1's recorded override).
//!
//! The layer record lands in the boundary's artifact table addressed by the
//! layer id — `get_artifact(overrides_layer_id)` serves the materialised
//! `{layer_id, provenance, fragment}` record.

use hh_embed_schema::errors::EmbedError;
use hh_embed_schema::types::{AttendanceDeclaration, Override};
use hh_hir::diff::{AuthorityDelta, Delta};
use hh_wire::json::Json;

use crate::service::EmbedService;

/// The coordinate prefixes an `overrides[]` pointer may name — the
/// definition's tunable surface (ADR-0168 D4; `inject::OVERRIDE_ALLOWED`'s
/// semantic home). `/model`, `/mode`, `/thought_level` mount on
/// `assembly.values.<name>`; `/profile` mounts on
/// `assembly.profile_binding`.
const TUNABLE_VALUES: &[&str] = &["/model", "/mode", "/thought_level"];

impl EmbedService {
    /// Desugar `overrides[]` → the one `experiment` layer; enforce I-1.
    /// Returns `Some(overrides_layer_id)` when a layer materialised.
    /// Called before `open_run` — a refusal means no run ever existed
    /// (the exit-class table's pre-ledger `refused`/`invocation_error`
    /// cases must not open a run).
    pub(crate) fn materialise_overrides(
        &mut self,
        sealed: &hh_hir::document::SealedDefinition,
        overrides: &[Override],
        attendance: &AttendanceDeclaration,
    ) -> Result<Option<String>, EmbedError> {
        if overrides.is_empty() {
            return Ok(None);
        }
        let fragment = Json::Arr(overrides.iter().map(Override::to_json).collect());
        // ADR-0147 D2 — `id = H(idp ∥ "override-layer" ∥ canonical(overrides))`.
        let layer_id = hh_identity::address(
            format!("override-layer{}", fragment.to_canonical_string()).as_bytes(),
            "application/json",
        )
        .id();

        // ── apply the patch pair ─────────────────────────────────────
        let mut doc_json = sealed.document.to_json();
        let mut out_of_space = false;
        for o in overrides {
            let before = doc_json.clone();
            if !apply_override(&mut doc_json, o) {
                out_of_space = true;
                continue;
            }
            if doc_json == before || in_coordinate_space(o) {
                continue;
            }
            // A literal pointer must reach a member the schema can see —
            // a write that materialises schema-invisible members diffs
            // to nothing and is the same `AuthorityViolation` the closed
            // coordinate-prefix check gave (S1.25's contract,
            // preserved). Classify the single-edit pair so one bogus
            // pointer can't hide behind another override's real diff.
            let visible = match (
                hh_hir::document::parse_document(&before.to_canonical_string().into_bytes()),
                hh_hir::document::parse_document(&doc_json.to_canonical_string().into_bytes()),
            ) {
                (Ok(b), Ok(t)) => {
                    let c = hh_hir::classify_pair(&b, &t);
                    c.semantic_ops + c.surface_ops + c.provenance_only_ops + c.ext_ops > 0
                }
                // The aggregate parse below reports the unreadable
                // patch as `InvalidDefinition`.
                _ => true,
            };
            if !visible {
                out_of_space = true;
            }
        }
        // AC-R-2.2.4-12 — the speculation-policy floor (§5a.4; ADR-0134
        // §5): wherever a `speculation_policy` member appears in the
        // patched document it must be a narrowing of the sealed
        // document's policy at the same path (the §5a.4 default when the
        // parent declares none). The check is over the policy pair, not
        // the vehicle — a layer, an `authority_cap` or a diff all meet
        // the same floor (CC1). A widening — or a removal — is refused
        // before the layer materialises.
        gate_speculation_floor(&sealed.document.to_json(), &doc_json)?;

        let patched_bytes = doc_json.to_canonical_string().into_bytes();
        let patched = hh_hir::document::parse_document(&patched_bytes).map_err(|e| {
            EmbedError::InvalidDefinition {
                diagnostics: vec![format!(
                    "override patch produced an unreadable document: {e}"
                )],
            }
        })?;

        // ── I-1 — the HirDiff classification decides ────────────────
        let c = hh_hir::classify_pair(&sealed.document, &patched);
        let widening =
            c.authority_delta == AuthorityDelta::Widening || c.budget_delta == Delta::Loosening;
        if widening {
            // A widening override requires human attendance at the
            // terminal (I-1): `interactive` declared or TTY-inferred is
            // the human's declaration; anything else — `async`,
            // `unattended`, a piped invocation — is refused before the
            // run exists.
            if attendance.value != "interactive" {
                return Err(EmbedError::AuthorityWideningRequiresHuman {
                    detail: format!(
                        "override patch widens authority or loosens budget \
                         (authority_delta={:?}, budget_delta={:?}) under attendance {}",
                        c.authority_delta, c.budget_delta, attendance.value
                    ),
                });
            }
        } else if out_of_space {
            return Err(EmbedError::AuthorityViolation {
                layer: "override".to_string(),
                detail: "override pointer names a member outside the tunable coordinate space"
                    .to_string(),
            });
        }

        // ── materialise the layer record ─────────────────────────────
        // `origin` records who the patch is attributable to: the I-1
        // human gate produces `human`; an ordinary tunable patch is the
        // principal's declared override either way (`human` at a TTY,
        // `automation` from a piped/unattended invocation — the record
        // never claims a human who was not there).
        let origin = if attendance.value == "interactive" {
            "human"
        } else {
            "automation"
        };
        let layer = Json::obj([
            ("layer_id", Json::str(layer_id.clone())),
            (
                "provenance",
                Json::obj([
                    ("source_kind", Json::str("experiment")),
                    ("id", Json::str(layer_id.clone())),
                    ("origin", Json::str(origin)),
                ]),
            ),
            ("fragment", fragment),
        ]);
        let layer_bytes = layer.to_canonical_string().into_bytes();
        self.sealed_defs
            .insert(layer_id.clone(), layer_bytes.clone());
        // DF-S1.25-3 (S2.3): the layer record lands in the blob pool too —
        // `layer_id` is a blob-domain content address (`idp/1`), so
        // `get_artifact(layer_id)` resolves through `get_blob` after a
        // service restart.
        self.store
            .put_blob(&layer_bytes, "application/json")
            .map_err(crate::service::ledger_err)?;
        Ok(Some(layer_id))
    }
}

/// Apply one `Override{pointer, value}` to the document JSON. Returns
/// `false` when the pointer names a member outside the tunable
/// coordinate space *and* the literal patch could not be applied (the
/// classification still decides — a literal patch that applies is a
/// real document edit, widened or not).
/// Whether the pointer names the declared tunable coordinate space
/// (`/model`, `/mode`, `/thought_level`, `/profile`) — anything else
/// takes the literal document-pointer path and must still produce a
/// schema-visible diff (`AuthorityViolation` otherwise).
fn in_coordinate_space(o: &Override) -> bool {
    TUNABLE_VALUES
        .iter()
        .any(|p| o.pointer == *p || o.pointer.starts_with(&format!("{p}/")))
        || o.pointer == "/profile"
        || o.pointer.starts_with("/profile/")
}

fn apply_override(doc: &mut Json, o: &Override) -> bool {
    for prefix in TUNABLE_VALUES {
        if o.pointer == *prefix || o.pointer.starts_with(&format!("{prefix}/")) {
            let name = prefix.trim_start_matches('/');
            let tail = o.pointer[prefix.len()..].trim_start_matches('/');
            // Mount: `assembly.values.<name>[.<tail>]` — the declared
            // parameter surface (ADR-0025 `set` grammar's `values`).
            let assembly = match doc.get_mut("assembly") {
                Some(a @ Json::Obj(_)) => a,
                _ => return false,
            };
            let values = obj_member(assembly, "values");
            if tail.is_empty() {
                return set_member(values, name, o.value.clone());
            }
            let target = obj_member(values, name);
            return set_json_pointer(target, tail, o.value.clone());
        }
    }
    if o.pointer == "/profile" || o.pointer.starts_with("/profile/") {
        let assembly = match doc.get_mut("assembly") {
            Some(a @ Json::Obj(_)) => a,
            _ => return false,
        };
        let tail = o.pointer["/profile".len()..].trim_start_matches('/');
        if tail.is_empty() {
            return set_member(assembly, "profile_binding", o.value.clone());
        }
        let target = obj_member(assembly, "profile_binding");
        return set_json_pointer(target, tail, o.value.clone());
    }
    // Out of the tunable space — apply the pointer literally into the
    // document so `classify_pair` can read what the edit actually does.
    let ptr = o.pointer.trim_start_matches('/');
    set_json_pointer(doc, ptr, o.value.clone())
}

/// The `Json::Obj` member `name` as a mutable object slot (created when
/// absent — `values`/`profile_binding` mounts may introduce the member).
fn obj_member<'a>(obj: &'a mut Json, name: &str) -> &'a mut Json {
    match obj {
        Json::Obj(m) => m
            .entry(name.to_string())
            .or_insert_with(|| Json::Obj(Default::default())),
        _ => unreachable!("obj_member on a non-object"),
    }
}

/// Set `obj[name] = v` — the leaf mount (`values.model = …`).
fn set_member(obj: &mut Json, name: &str, v: Json) -> bool {
    match obj {
        Json::Obj(m) => {
            m.insert(name.to_string(), v);
            true
        }
        _ => false,
    }
}

/// A JSON-pointer set with `~0`/`~1` unescaping (RFC 6901's object subset —
/// arrays index by position; intermediate members materialise as objects).
fn set_json_pointer(doc: &mut Json, pointer: &str, v: Json) -> bool {
    let segs: Vec<String> = pointer
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect();
    if segs.is_empty() {
        return false;
    }
    let mut cur = doc;
    for (i, seg) in segs.iter().enumerate() {
        let last = i + 1 == segs.len();
        match cur {
            Json::Obj(m) => {
                if last {
                    m.insert(seg.clone(), v);
                    return true;
                }
                cur = m
                    .entry(seg.clone())
                    .or_insert_with(|| Json::Obj(Default::default()));
            }
            Json::Arr(a) => {
                let idx: usize = match seg.parse() {
                    Ok(n) if n <= a.len() => n,
                    _ => return false,
                };
                if last {
                    if idx == a.len() {
                        a.push(v);
                    } else {
                        a[idx] = v;
                    }
                    return true;
                }
                cur = &mut a[idx];
            }
            _ => return false,
        }
    }
    false
}

// `get_mut` is not a `Json` method — the small helper keeps the mount
// reads honest (no invented accessor).
trait JsonGetMut {
    fn get_mut(&mut self, key: &str) -> Option<&mut Json>;
}
impl JsonGetMut for Json {
    fn get_mut(&mut self, key: &str) -> Option<&mut Json> {
        match self {
            Json::Obj(m) => m.get_mut(key),
            _ => None,
        }
    }
}

// ── the speculation-policy floor (AC-R-2.2.4-12) ─────────────────────────────

/// Every `speculation_policy` member in a document's JSON, keyed by its
/// RFC-6901 pointer path (the check is over the policy pair, not the
/// vehicle — wherever the member lands, the floor applies).
fn speculation_members(doc: &Json) -> std::collections::BTreeMap<String, Json> {
    fn walk(j: &Json, path: &str, out: &mut std::collections::BTreeMap<String, Json>) {
        match j {
            Json::Obj(m) => {
                for (k, v) in m {
                    let esc = k.replace('~', "~0").replace('/', "~1");
                    let p = format!("{path}/{esc}");
                    if k == "speculation_policy" {
                        out.insert(p.clone(), v.clone());
                    }
                    walk(v, &p, out);
                }
            }
            Json::Arr(a) => {
                for (i, v) in a.iter().enumerate() {
                    walk(v, &format!("{path}/{i}"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(doc, "", &mut out);
    out
}

/// `SpeculationPolicy::check_override` over every `speculation_policy`
/// member of the `(sealed, patched)` pair — widening or removal is
/// `AuthorityViolation`; an unparsable member is `InvalidDefinition`.
fn gate_speculation_floor(before: &Json, after: &Json) -> Result<(), EmbedError> {
    use hh_hir::speculation::SpeculationPolicy;
    let old = speculation_members(before);
    let new = speculation_members(after);
    let parse = |path: &str, j: &Json| -> Result<SpeculationPolicy, EmbedError> {
        SpeculationPolicy::from_json(j).map_err(|e| EmbedError::InvalidDefinition {
            diagnostics: vec![format!("speculation_policy at {path}: {e}")],
        })
    };
    for (path, pj) in &new {
        let parent = match old.get(path) {
            Some(oj) => parse(path, oj)?,
            // The parent declared none — the §5a.4 default is the floor
            // the write narrows from.
            None => SpeculationPolicy::default_policy(),
        };
        let proposed = parse(path, pj)?;
        SpeculationPolicy::check_override(&parent, &proposed, None, false).map_err(|e| {
            EmbedError::AuthorityViolation {
                layer: "override".to_string(),
                detail: format!("speculation_policy at {path}: {e}"),
            }
        })?;
    }
    for path in old.keys() {
        if !new.contains_key(path) {
            return Err(EmbedError::AuthorityViolation {
                layer: "override".to_string(),
                detail: format!(
                    "speculation_policy at {path} removed — the floor admits narrowing only"
                ),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ov(pointer: &str, value: Json) -> Override {
        Override {
            pointer: pointer.into(),
            value,
        }
    }

    #[test]
    fn tunable_pointer_mounts_on_assembly_values() {
        let mut doc = Json::obj([
            ("dialect", Json::str("HIR/1")),
            ("assembly", Json::obj([("values", Json::obj([]))])),
        ]);
        assert!(apply_override(&mut doc, &ov("/model", Json::str("m-2"))));
        assert_eq!(
            doc.get("assembly")
                .and_then(|a| a.get("values"))
                .and_then(|v| v.get("model")),
            Some(&Json::str("m-2"))
        );
    }

    #[test]
    fn profile_pointer_mounts_on_profile_binding() {
        let mut doc = Json::obj([("assembly", Json::obj([]))]);
        assert!(apply_override(
            &mut doc,
            &ov("/profile", Json::str("sha256:p"))
        ));
        assert_eq!(
            doc.get("assembly").and_then(|a| a.get("profile_binding")),
            Some(&Json::str("sha256:p"))
        );
    }

    #[test]
    fn deep_tunable_tail_nests_inside_the_value() {
        let mut doc = Json::obj([("assembly", Json::obj([]))]);
        assert!(apply_override(&mut doc, &ov("/mode/extra", Json::Int(3))));
        assert_eq!(
            doc.get("assembly")
                .and_then(|a| a.get("values"))
                .and_then(|v| v.get("mode"))
                .and_then(|m| m.get("extra")),
            Some(&Json::Int(3))
        );
    }

    #[test]
    fn literal_pointer_patches_document_members() {
        let mut doc = Json::obj([(
            "nodes",
            Json::Arr(vec![Json::obj([("hard", Json::Int(1))])]),
        )]);
        assert!(apply_override(&mut doc, &ov("/nodes/0/hard", Json::Int(9))));
        assert_eq!(
            doc.get("nodes").and_then(|n| match n {
                Json::Arr(a) => a[0].get("hard"),
                _ => None,
            }),
            Some(&Json::Int(9))
        );
    }

    #[test]
    fn unreachable_pointer_reports_out_of_space() {
        let mut doc = Json::obj([("dialect", Json::str("HIR/1"))]);
        assert!(!apply_override(&mut doc, &ov("/model", Json::str("x"))));
    }
}
