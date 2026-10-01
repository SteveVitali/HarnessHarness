//! `resolve` (§3.3.4/§3.3.6): the ONE resolver feeding `seal` (CF-056). It pins every
//! `ComponentVariantRef` through the registry (snapshot-confined — R8), substitutes every
//! `$param`/`$entity` binding form, leaves `$secret:` channel names untouched, pins every
//! entity `Ref`, records `{registry_snapshot_id, resolved_at}` in the `resolved` member,
//! materialises `assembly.slots` onto the root `native` process, and hands the document
//! to `hh_hir::seal`. The output is **closed** (no selector survives), **secret-free**
//! (channel names only), and **idempotent** — `resolve(sealed)` re-verifies the pins and
//! returns byte-identical output. `execute` mode refuses revoked/stale pinned closures
//! (`C-REF-4 StaleIndex`).

use hh_hir::document::{HirDocument, SealedDefinition};
use hh_hir::records::{AgentProcessBody, KindRecord, SlotBindings};
use hh_hir::refs::{ComponentVariantRef, Ref, RefVersion};
use hh_identity::names::ResolveMode;
use hh_provenance::ProvenanceRecord;
use hh_registry::store::{RegistryStore, ResolveInput, ResolveRequest, SlotConstraints};
use hh_registry::RegistryError;
use hh_wire::json::Json;

use crate::catalog::ClassCatalog;
use crate::diagnostics::{detail_text, kern_code, AssemblyDiagnostic, Code, Severity, Stage};
use crate::grammar::{
    Assembly, EntityBinding, ResolvedInfo, ENTITY_MARKER, PARAM_MARKER, SECRET_MARKER,
};
use crate::validate::{benchmark_hits, materialise};

/// The resolve environment — the registry view, snapshot confinement, mode, the
/// registrar's kernel provenance, and the `resolved_at` stamp (a *parameter*: identity
/// is stable across machines for one snapshot because `resolved_at` never enters the
/// identity-bearing members — §3.3.6).
pub struct ResolveEnv<'a> {
    /// The registry store (the one store — CC7).
    pub registry: &'a mut RegistryStore,
    /// The class catalog (`slot_key → ClassRecord`).
    pub catalog: &'a dyn ClassCatalog,
    /// The snapshot to resolve inside; `None` cuts a fresh one (recorded either way).
    pub snapshot_id: Option<String>,
    /// `Audit | Execute` — `execute` refuses revoked/stale pinned closures (R7).
    pub mode: ResolveMode,
    /// The kernel provenance — mints diagnostic `detail` leaves and the snapshot's
    /// registrar record.
    pub registrar: ProvenanceRecord,
    /// The `resolved_at` stamp (ms).
    pub resolved_at: u64,
    /// Optional sink for the *non-error* diagnostics the run produced (info /
    /// warning notices such as `C-REF-5 DenyListNoop`). On `Err` the diags
    /// are returned as before; on `Ok` they land here — a service that must
    /// surface the complete picture (§6.1 V-3) sets the sink; callers that
    /// don't leave it `None`.
    pub notices: Option<&'a mut Vec<AssemblyDiagnostic>>,
}

/// `resolve(definition, registry_view, environment, mode) → SealedDefinition` — or the
/// complete diagnostic set (every failure collected across the section before the
/// refusal; the seal never sees a half-resolved document).
pub fn resolve(
    doc: &HirDocument,
    env: &mut ResolveEnv,
) -> Result<SealedDefinition, Vec<AssemblyDiagnostic>> {
    let kernel = env.registrar.clone();
    let mut diags: Vec<AssemblyDiagnostic> = Vec::new();

    // Decode the section (member-wise; every malformed member is a diagnostic).
    let mut assembly = match &doc.assembly {
        Some(j) => match Assembly::from_json(j, "/assembly", &kernel, &mut diags) {
            Some(a) => a,
            None => return Err(diags),
        },
        None => {
            diags.push(rdiag(
                Code::LoadParse,
                "/assembly",
                "assembly",
                "the document carries no `assembly` section",
                "author the `assembly` member before resolve",
                &kernel,
            ));
            return Err(diags);
        }
    };

    // 7-L2 at resolve (pre-seal) — a benchmark-conditioned reference is an error even
    // when nobody ran validate_assembly first.
    for hit in benchmark_hits(doc, Some(&assembly)) {
        diags.push(rdiag(
            Code::LcdBenchmarkConditionedRule,
            &hit,
            &hit,
            "a benchmark coordinate reference reaches `resolve` (rules condition on declared profiles, never on the benchmark)",
            "remove the benchmark reference",
            &kernel,
        ));
    }

    // The snapshot — supplied or cut now; recorded either way.
    let snapshot_id = match &env.snapshot_id {
        Some(s) => s.clone(),
        None => match env.registry.snapshot(&kernel) {
            Ok(s) => s.snapshot_id,
            Err(e) => {
                diags.push(reg_err(&e, "/assembly", "snapshot", &kernel));
                return Err(diags);
            }
        },
    };

    let already_resolved = assembly.is_resolved();

    // Substitute `$param:`/`$entity:` in slot params (and flag stray markers in values).
    substitute_params(&mut assembly, &mut diags, &kernel);

    // Pin every entity `Ref` against the document (doc-internal at Stage 1).
    pin_entities(&mut assembly, doc, &mut diags, &kernel);

    // Pin / re-verify every slot-variant version — `assembly.slots` and every native
    // process's `native.slots` (nested natives bind through the same resolver).
    resolve_slots(&mut assembly, env, &snapshot_id, &mut diags, &kernel);

    // Extension refs pin against the snapshot's `extension` records (§5g.5 §3 —
    // the resolve-time pin; a selector matching nothing is `C-EXT-4`, an
    // ambiguous match is `C-REF-2`, and an authored pin that no longer resolves
    // is `C-REF-4`).
    resolve_extensions(&mut assembly, env, &snapshot_id, &mut diags, &kernel);

    // Secrets resolve to channel names only — an inline value where a `$secret:` name
    // belongs is `C-SEC-1`.
    secret_scan(&assembly, &mut diags, &kernel);

    if !diags.iter().any(|d| d.severity == Severity::Error) || already_resolved {
        // Record the resolve provenance only on a fresh resolve (idempotence:
        // `resolve(sealed)` keeps the recorded snapshot/stamp).
        if !already_resolved {
            assembly.resolved = Some(ResolvedInfo {
                registry_snapshot_id: snapshot_id.clone(),
                resolved_at: env.resolved_at,
            });
        }
    }

    if diags.iter().any(|d| d.severity == Severity::Error) {
        return Err(diags);
    }

    // Materialise + pin the native slots + seal.
    let mut out = doc.clone();
    out.assembly = Some(assembly.to_json());
    out = materialise(&out, Some(&assembly));
    resolve_native_slots(&mut out, env, &snapshot_id, &mut diags, &kernel);
    if diags.iter().any(|d| d.severity == Severity::Error) {
        return Err(diags);
    }
    // Idempotence (§3.3.4 `resolve` is idempotent): a document that is already
    // resolved *and* sealed returns as itself — re-sealing would re-endorse member
    // provenance and mint fresh version_ids. Reconstruct the SealedDefinition from
    // the document's existing coordinates (the closed-world fold is a pure
    // projection — the same predicate `seal` runs).
    let fully_sealed =
        already_resolved && !out.nodes.is_empty() && out.nodes.iter().all(|n| n.version.sealed);
    if fully_sealed {
        let closed_world_tools = out
            .nodes
            .iter()
            .filter_map(|n| match &n.semantic {
                KindRecord::ToolCapability(t) => match &t.effects {
                    hh_hir::kinds::ToolEffects::Pure => Some(n.semantic_id()),
                    hh_hir::kinds::ToolEffects::Declared(set)
                        if !set.is_empty() && set.iter().all(|e| e.is_closed_world()) =>
                    {
                        Some(n.semantic_id())
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let definition_ref = {
            let root_node = out
                .node(&out.root.semantic_id)
                .expect("the root resolves — the document came in sealed");
            hh_hir::document::DefinitionVersionRef {
                semantic_id: root_node.semantic_id(),
                version_id: root_node.version_id(),
            }
        };
        if let Some(n) = env.notices.as_deref_mut() {
            n.append(&mut diags);
        }
        return Ok(SealedDefinition {
            document: out,
            definition_ref,
            closed_world_tools,
        });
    }
    match hh_hir::ops::seal(&out, env.resolved_at) {
        Ok(sealed) => {
            if let Some(n) = env.notices.as_deref_mut() {
                n.append(&mut diags);
            }
            Ok(sealed)
        }
        Err(errs) => {
            for e in errs {
                diags.push(AssemblyDiagnostic {
                    code: kern_code(&e),
                    class: None,
                    severity: Severity::Error,
                    path: "/".into(),
                    source_layer: None,
                    subject: format!("{e:?}"),
                    stage: Stage::Resolve,
                    detail: detail_text(
                        format!("seal refused the resolved document: {e:?}"),
                        &kernel,
                    ),
                    remedy: "fix the underlying HIR member".into(),
                    owner_adr: "ADR-0148".into(),
                });
            }
            Err(diags)
        }
    }
}

// ── substitution ──────────────────────────────────────────────────────────────

/// Substitute `$param:<id>` and `$entity:<id>` markers inside slot `params`; `$secret:`
/// channel names pass through untouched.
fn substitute_params(
    a: &mut Assembly,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) {
    let mut bind_paths = Vec::new();
    for (slot, bs) in &a.slots {
        match bs {
            SlotBindings::One(_) => bind_paths.push((slot.clone(), 0usize)),
            SlotBindings::Many(v) => {
                for i in 0..v.len() {
                    bind_paths.push((slot.clone(), i));
                }
            }
        }
    }
    for (slot, i) in bind_paths {
        let path = format!("/assembly/slots/{slot}[{i}]/params");
        // Pass 1 — read the binding's params (immutable) and compute each key's
        // substitution, so `a` isn't mutably borrowed while `substitute_markers`
        // reads `values`/`parameters`.
        let keys: Vec<String> = match a.slots.get(&slot) {
            Some(SlotBindings::One(b)) if i == 0 => b.params.keys().cloned().collect(),
            Some(SlotBindings::Many(v)) => match v.get(i) {
                Some(b) => b.params.keys().cloned().collect(),
                None => continue,
            },
            _ => continue,
        };
        let mut substitutions: Vec<(String, Json)> = Vec::new();
        for k in &keys {
            let v = match a.slots.get(&slot) {
                Some(SlotBindings::One(b)) if i == 0 => b.params.get(k).cloned(),
                Some(SlotBindings::Many(bs)) => bs.get(i).and_then(|b| b.params.get(k).cloned()),
                _ => None,
            }
            .unwrap_or(Json::Null);
            match &v {
                Json::Str(s) if s.starts_with(PARAM_MARKER) => {
                    let id = &s[PARAM_MARKER.len()..];
                    let value = a
                        .values
                        .get(id)
                        .cloned()
                        .or_else(|| a.parameters.get(id).and_then(|p| p.default.clone()));
                    match value {
                        Some(vv) => substitutions.push((k.clone(), vv)),
                        None => diags.push(rdiag(
                            Code::ParamUndeclaredRef,
                            &format!("{path}/{k}"),
                            id,
                            &format!("`$param:{id}` has no value and no default"),
                            "declare the parameter's value or default",
                            kernel,
                        )),
                    }
                }
                Json::Str(s) if s.starts_with(ENTITY_MARKER) => {
                    // resolved after entity pinning — see `substitute_entities`.
                }
                _ => {
                    let subst = substitute_markers(&v, a, &format!("{path}/{k}"), diags, kernel);
                    substitutions.push((k.clone(), subst));
                }
            }
        }
        // Pass 2 — write the computed substitutions.
        let binding = match a.slots.get_mut(&slot) {
            Some(SlotBindings::One(b)) if i == 0 => b,
            Some(SlotBindings::Many(v)) => match v.get_mut(i) {
                Some(b) => b,
                None => continue,
            },
            _ => continue,
        };
        for (k, v) in substitutions {
            binding.params.insert(k, v);
        }
    }
    // A `$param:`/`$entity:` marker in `values` itself is malformed (values are the
    // point in the space, not a reference to one).
    let vkeys: Vec<String> = a.values.keys().cloned().collect();
    for k in vkeys {
        if let Some(v) = a.values.get(&k).cloned() {
            let mut markers = Vec::new();
            crate::grammar::markers_in_json(&v, &format!("/assembly/values/{k}"), &mut markers);
            for (p, m) in markers {
                if !m.starts_with(SECRET_MARKER) {
                    diags.push(rdiag(
                        Code::ParamUnknown,
                        &p,
                        &m,
                        "a binding form inside `values` — values carry data, never references",
                        "move the reference to a slot `params` member",
                        kernel,
                    ));
                }
            }
        }
    }
}

/// Recursively substitute markers inside an arbitrary param value.
fn substitute_markers(
    v: &Json,
    a: &Assembly,
    path: &str,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) -> Json {
    match v {
        Json::Str(s) if s.starts_with(PARAM_MARKER) => {
            let id = &s[PARAM_MARKER.len()..];
            match a
                .values
                .get(id)
                .cloned()
                .or_else(|| a.parameters.get(id).and_then(|p| p.default.clone()))
            {
                Some(vv) => vv,
                None => {
                    diags.push(rdiag(
                        Code::ParamUndeclaredRef,
                        path,
                        id,
                        &format!("`$param:{id}` has no value and no default"),
                        "declare the parameter's value or default",
                        kernel,
                    ));
                    v.clone()
                }
            }
        }
        Json::Str(s) if s.starts_with(ENTITY_MARKER) => {
            let id = &s[ENTITY_MARKER.len()..];
            match a.entities.get(id) {
                Some(eb) => entity_json(eb),
                None => {
                    diags.push(rdiag(
                        Code::RefUnresolved,
                        path,
                        id,
                        &format!("`$entity:{id}` names no declared entity"),
                        "declare the entity or fix the marker",
                        kernel,
                    ));
                    v.clone()
                }
            }
        }
        Json::Obj(m) => Json::Obj(
            m.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        substitute_markers(v, a, &format!("{path}/{k}"), diags, kernel),
                    )
                })
                .collect(),
        ),
        Json::Arr(items) => Json::Arr(
            items
                .iter()
                .enumerate()
                .map(|(i, v)| substitute_markers(v, a, &format!("{path}[{i}]"), diags, kernel))
                .collect(),
        ),
        _ => v.clone(),
    }
}

fn entity_json(e: &EntityBinding) -> Json {
    match e {
        EntityBinding::Ref(r) => r.to_json(),
        EntityBinding::Inline { kind, record } => {
            Json::obj([("kind", Json::str(kind)), ("record", record.clone())])
        }
    }
}

// ── entity pinning ────────────────────────────────────────────────────────────

/// Pin every `Ref`-form entity (doc-internal at Stage 1 — `semantic_id` → the node's
/// `version_id`; external entity refs are `C-REF-1`) and substitute remaining
/// `$entity:` markers in slot params.
fn pin_entities(
    a: &mut Assembly,
    doc: &HirDocument,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) {
    let ids: Vec<String> = a.entities.keys().cloned().collect();
    for id in ids {
        let path = format!("/assembly/entities/{id}");
        if let Some(EntityBinding::Ref(r)) = a.entities.get(&id) {
            let mut r = r.clone();
            if matches!(r.version, RefVersion::Selector(_)) {
                match doc.node(&r.semantic_id) {
                    Some(n) => r.version = RefVersion::Pinned(n.version_id()),
                    None => diags.push(rdiag(
                        Code::RefUnresolved,
                        &path,
                        &r.semantic_id,
                        "entity ref names no document node (external entity refs are Stage 2+)",
                        "reference a node of this document",
                        kernel,
                    )),
                }
            }
            a.entities.insert(id.clone(), EntityBinding::Ref(r));
        }
    }
    // `$entity:` markers in slot params → the pinned entity's JSON.
    let mut edits: Vec<(String, usize, String, Json)> = Vec::new();
    for (slot, bs) in &a.slots {
        let bs_vec: Vec<&hh_hir::records::SlotBinding> = match bs {
            SlotBindings::One(b) => vec![b],
            SlotBindings::Many(v) => v.iter().collect(),
        };
        for (i, b) in bs_vec.iter().enumerate() {
            for (k, v) in &b.params {
                let subst = substitute_markers(
                    v,
                    a,
                    &format!("/assembly/slots/{slot}[{i}]/params/{k}"),
                    diags,
                    kernel,
                );
                if subst != *v {
                    edits.push((slot.clone(), i, k.clone(), subst));
                }
            }
        }
    }
    for (slot, i, k, v) in edits {
        let binding = match a.slots.get_mut(&slot) {
            Some(SlotBindings::One(b)) if i == 0 => Some(b),
            Some(SlotBindings::Many(vv)) => vv.get_mut(i),
            _ => None,
        };
        if let Some(b) = binding {
            b.params.insert(k, v);
        }
    }
    // Pin any `{semantic_id, version_selector}` object inside values/params/entities
    // payloads (a `ref<entity_kind>` parameter's authored value, etc.).
    let mut val_edits = Vec::new();
    for (k, v) in &a.values {
        let pinned = pin_json_refs(v, doc, &format!("/assembly/values/{k}"), diags, kernel);
        if pinned != *v {
            val_edits.push((k.clone(), pinned));
        }
    }
    for (k, v) in val_edits {
        a.values.insert(k, v);
    }
}

/// Pin embedded `{semantic_id, version_selector}` objects against document nodes.
fn pin_json_refs(
    j: &Json,
    doc: &HirDocument,
    path: &str,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) -> Json {
    match j {
        Json::Obj(m) => {
            if m.contains_key("semantic_id") && m.contains_key("version_selector") {
                let sid = m.get("semantic_id").and_then(Json::as_str).unwrap_or("");
                return match doc.node(sid) {
                    Some(n) => Json::obj([
                        ("semantic_id", Json::str(sid)),
                        ("version_id", Json::str(n.version_id())),
                    ]),
                    None => {
                        diags.push(rdiag(
                            Code::RefUnresolved,
                            path,
                            sid,
                            "embedded ref names no document node",
                            "reference a node of this document",
                            kernel,
                        ));
                        j.clone()
                    }
                };
            }
            Json::Obj(
                m.iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            pin_json_refs(v, doc, &format!("{path}/{k}"), diags, kernel),
                        )
                    })
                    .collect(),
            )
        }
        Json::Arr(items) => Json::Arr(
            items
                .iter()
                .enumerate()
                .map(|(i, v)| pin_json_refs(v, doc, &format!("{path}[{i}]"), diags, kernel))
                .collect(),
        ),
        _ => j.clone(),
    }
}

// ── variant resolution ────────────────────────────────────────────────────────

/// Parse `variant_id` → `(namespace, name)` (`<namespace>/<name>`; a bare name is a
/// `C-REF-1` — the namespace is never defaulted, ADR-0240).
pub fn split_variant_id(variant_id: &str) -> Option<(String, String)> {
    let (ns, name) = variant_id.split_once('/')?;
    if ns.is_empty() || name.is_empty() {
        return None;
    }
    Some((ns.to_string(), name.to_string()))
}

/// The version-selector grammar (§3.3.2): `latest | label:<l> | version:<vid> |
/// range:<lo>-<hi> | deny:<vid[,vid…]>` (ADR-0240).
enum Selector {
    Latest,
    Label(String),
    Version(String),
    Range(String, String),
    Deny(Vec<String>),
}

fn parse_selector(s: &str) -> Result<Selector, String> {
    if s == "latest" {
        return Ok(Selector::Latest);
    }
    if let Some(l) = s.strip_prefix("label:") {
        return Ok(Selector::Label(l.to_string()));
    }
    if let Some(v) = s.strip_prefix("version:") {
        return Ok(Selector::Version(v.to_string()));
    }
    if let Some(r) = s.strip_prefix("range:") {
        let (lo, hi) = r
            .split_once('-')
            .ok_or("`range:` wants `range:<lo>-<hi>`")?;
        return Ok(Selector::Range(lo.to_string(), hi.to_string()));
    }
    if let Some(d) = s.strip_prefix("deny:") {
        return Ok(Selector::Deny(d.split(',').map(str::to_string).collect()));
    }
    Err(format!(
        "unknown selector `{s}` (latest | label:<l> | version:<vid> | range:<lo>-<hi> | deny:<ids>)"
    ))
}

/// SemVer-class label compare — dotted-numeric, shorter-is-less on equal prefixes.
fn semver_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let pa: Vec<u64> = a.split('.').filter_map(|x| x.parse().ok()).collect();
    let pb: Vec<u64> = b.split('.').filter_map(|x| x.parse().ok()).collect();
    pa.cmp(&pb)
}

/// Resolve one `ComponentVariantRef` inside the snapshot; write back the pinned
/// `version_id` on success.
#[allow(clippy::too_many_arguments)]
fn resolve_variant(
    vref: &mut ComponentVariantRef,
    slot: &str,
    class_id: Option<&str>,
    env: &ResolveEnv,
    snapshot_id: &str,
    path: &str,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) {
    let requirements = ResolveRequest {
        class_id: class_id.map(str::to_string),
        contract_version: None,
        conformance_floor: None,
    };
    let (ns, name) = match split_variant_id(&vref.variant_id) {
        Some(nn) => nn,
        None => {
            diags.push(rdiag(
                Code::RefUnresolved,
                path,
                &vref.variant_id,
                "variant_id must spell `<namespace>/<name>`",
                "spell the variant as `<namespace>/<name>` (e.g. `hh/round_robin`)",
                kernel,
            ));
            return;
        }
    };
    let input = match &vref.version {
        RefVersion::Pinned(vid) => ResolveInput::Version(vid.clone()),
        RefVersion::Selector(sel) => match parse_selector(sel) {
            Ok(Selector::Latest) => ResolveInput::Selector {
                namespace: ns.clone(),
                name: name.clone(),
                label: None,
                snapshot_id: Some(snapshot_id.to_string()),
            },
            Ok(Selector::Label(l)) => ResolveInput::Selector {
                namespace: ns.clone(),
                name: name.clone(),
                label: Some(l),
                snapshot_id: Some(snapshot_id.to_string()),
            },
            Ok(Selector::Version(vid)) => ResolveInput::Version(vid),
            Ok(Selector::Range(lo, hi)) => {
                match resolve_range(env, class_id, &ns, &name, &lo, &hi, snapshot_id) {
                    Ok(vid) => ResolveInput::Version(vid),
                    Err(d) => {
                        diags.push(d);
                        return;
                    }
                }
            }
            Ok(Selector::Deny(deny)) => {
                match resolve_deny(env, class_id, &ns, &name, &deny, snapshot_id, path) {
                    Ok((vid, noop)) => {
                        if noop {
                            let mut d = rdiag(
                                Code::RefDenyListNoop,
                                path,
                                &vref.variant_id,
                                "the deny-list never bit — the resolved head was admissible",
                                "drop the deny list or pin explicitly",
                                kernel,
                            );
                            d.severity = Severity::Info;
                            diags.push(d);
                        }
                        ResolveInput::Version(vid)
                    }
                    Err(d) => {
                        diags.push(d);
                        return;
                    }
                }
            }
            Err(detail) => {
                diags.push(rdiag(
                    Code::RefUnresolved,
                    path,
                    sel,
                    &detail,
                    "spell the selector as latest | label:<l> | version:<vid> | range:<lo>-<hi> | deny:<ids>",
                    kernel,
                ));
                return;
            }
        },
    };
    match env.registry.resolve(&input, env.mode, &requirements) {
        Ok(resolved) => {
            vref.version = RefVersion::Pinned(resolved.versioned_ref.version_id);
        }
        Err(e) => diags.push(reg_err(&e, path, &vref.variant_id, kernel)),
    }
    let _ = slot;
}

/// `range:<lo>-<hi>` — enumerate the class's admissible variants for this name
/// (`slot_choices` — the ONE enumeration projection), filter by `version_label`, take
/// the highest.
#[allow(clippy::result_large_err)]
fn resolve_range(
    env: &ResolveEnv,
    class_id: Option<&str>,
    ns: &str,
    name: &str,
    lo: &str,
    hi: &str,
    snapshot_id: &str,
) -> Result<String, AssemblyDiagnostic> {
    let wanted = format!("{ns}/{name}");
    let choices = env
        .registry
        .slot_choices(
            class_id.unwrap_or(""),
            &SlotConstraints {
                snapshot_id: Some(snapshot_id.to_string()),
                ..Default::default()
            },
        )
        .map_err(|e| RegistryError::Unresolved {
            detail: format!("{e:?}"),
        });
    let choices = match choices {
        Ok(c) => c,
        Err(_) => {
            return Err(mkdiag(
                Code::RefUnresolved,
                "",
                &wanted,
                "no admissible variant in range",
            ))
        }
    };
    let mut best: Option<(String, String)> = None; // (label, version_id)
    for c in choices {
        if c.variant_id != wanted {
            continue;
        }
        let label = env
            .registry
            .get(&c.version_id)
            .and_then(|(_, r)| match r {
                hh_registry::records::RegistryRecord::Variant(v) => v.version_label.clone(),
                _ => None,
            })
            .unwrap_or_default();
        if semver_cmp(&label, lo) == std::cmp::Ordering::Less
            || semver_cmp(&label, hi) == std::cmp::Ordering::Greater
        {
            continue;
        }
        let better = match &best {
            None => true,
            Some((bl, bv)) => {
                semver_cmp(&label, bl) == std::cmp::Ordering::Greater
                    || (semver_cmp(&label, bl) == std::cmp::Ordering::Equal
                        && c.version_id.as_str() > bv.as_str())
            }
        };
        if better {
            best = Some((label, c.version_id));
        }
    }
    best.map(|(_, v)| v).ok_or_else(|| {
        mkdiag(
            Code::RefUnresolved,
            "",
            &wanted,
            "no admissible variant in the declared range",
        )
    })
}

/// `deny:<vid[,vid…]>` — the head resolve, then (when the head is denied) the
/// highest-labelled admissible non-denied candidate. Returns `(version_id, noop)`.
#[allow(clippy::result_large_err)]
fn resolve_deny(
    env: &ResolveEnv,
    class_id: Option<&str>,
    ns: &str,
    name: &str,
    deny: &[String],
    snapshot_id: &str,
    _path: &str,
) -> Result<(String, bool), AssemblyDiagnostic> {
    let requirements = ResolveRequest {
        class_id: class_id.map(str::to_string),
        ..Default::default()
    };
    let head = env.registry.resolve(
        &ResolveInput::Selector {
            namespace: ns.to_string(),
            name: name.to_string(),
            label: None,
            snapshot_id: Some(snapshot_id.to_string()),
        },
        env.mode,
        &requirements,
    );
    match head {
        Ok(r) if !deny.contains(&r.versioned_ref.version_id) => {
            Ok((r.versioned_ref.version_id, true))
        }
        Ok(r) => {
            // head denied — pick the highest-labelled admissible non-denied candidate.
            let vid = resolve_range_excluding(env, class_id, ns, name, deny, snapshot_id)
                .ok_or_else(|| {
                    mkdiag(
                        Code::RefUnresolved,
                        "",
                        &format!("{ns}/{name}"),
                        "every admissible candidate is deny-listed",
                    )
                })?;
            let _ = r;
            Ok((vid, false))
        }
        Err(e) => Err(reg_err(
            &e,
            "",
            &format!("{ns}/{name}"),
            &ProvenanceRecord::kernel("assembly", 0),
        )),
    }
}

fn resolve_range_excluding(
    env: &ResolveEnv,
    class_id: Option<&str>,
    ns: &str,
    name: &str,
    deny: &[String],
    snapshot_id: &str,
) -> Option<String> {
    let wanted = format!("{ns}/{name}");
    let choices = env
        .registry
        .slot_choices(
            class_id.unwrap_or(""),
            &SlotConstraints {
                snapshot_id: Some(snapshot_id.to_string()),
                ..Default::default()
            },
        )
        .ok()?;
    let mut best: Option<(String, String)> = None;
    for c in choices {
        if c.variant_id != wanted || deny.contains(&c.version_id) {
            continue;
        }
        let label = env
            .registry
            .get(&c.version_id)
            .and_then(|(_, r)| match r {
                hh_registry::records::RegistryRecord::Variant(v) => v.version_label.clone(),
                _ => None,
            })
            .unwrap_or_default();
        let better = match &best {
            None => true,
            Some((bl, bv)) => {
                semver_cmp(&label, bl) == std::cmp::Ordering::Greater
                    || (semver_cmp(&label, bl) == std::cmp::Ordering::Equal
                        && c.version_id.as_str() > bv.as_str())
            }
        };
        if better {
            best = Some((label, c.version_id));
        }
    }
    best.map(|(_, v)| v)
}

/// §5g.5 §3 resolve-time pin (S1.23): every `assembly.extensions.refs[]` member
/// is pinned against the store's `extension` records. Selector spellings are
/// `version:<version_id>` (exact) or `latest`/absent (the most recently
/// registered matching record — deterministic on `(registered_at, version_id)`);
/// an absent selector matching more than one record is `C-REF-2`. The final
/// pick always re-enters the ONE governed path (`RegistryStore::resolve` —
/// revoked/stale still refuses under `execute`). A ref's locator scheme must be
/// covered by `extensions.sources[]` (`C-EXT-2` — declared sources only,
/// CF-079); an authored pin whose record is absent or whose `content` no longer
/// matches is `C-EXT-4` — the pin never silently moves.
fn resolve_extensions(
    a: &mut Assembly,
    env: &ResolveEnv,
    _snapshot_id: &str,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) {
    let Some(block) = a.extensions.as_mut() else {
        return;
    };
    let declared: std::collections::BTreeSet<&str> =
        block.sources.iter().map(|s| s.kind_str()).collect();
    for (i, r) in block.refs.iter_mut().enumerate() {
        let path = format!("/assembly/extensions/refs/{i}");
        if !declared.contains(r.locator.scheme.as_str()) {
            diags.push(rdiag(
                Code::ExtUndeclaredSource,
                &path,
                &r.name,
                "the ref's locator scheme is not covered by `extensions.sources[]`",
                "declare the source kind in `extensions.sources[]` — sources are declared, never implicit",
                kernel,
            ));
        }
        // The target record: authored pin / `version:` selector → exact lookup;
        // `latest`/absent → newest matching `extension` record.
        let target: Option<String> = if let Some(vid) = &r.extension_id {
            Some(vid.clone())
        } else {
            match r.locator.selector.as_deref() {
                Some(sel) if sel.starts_with("version:") => {
                    Some(sel.trim_start_matches("version:").to_string())
                }
                None | Some("latest") => {
                    let mut best: Option<(u64, String)> = None;
                    for vid in env.registry.version_ids() {
                        let Some((env_r, rec)) = env.registry.get(vid) else {
                            continue;
                        };
                        let hh_registry::records::RegistryRecord::Extension(e) = rec else {
                            continue;
                        };
                        if e.name != r.name || e.kind != r.kind {
                            continue;
                        }
                        let cand = (env_r.registered_at, vid.clone());
                        if best.as_ref().map(|b| cand > *b).unwrap_or(true) {
                            best = Some(cand);
                        }
                    }
                    match r.locator.selector.as_deref() {
                        Some("latest") => best.map(|(_, v)| v),
                        None => best.map(|(_, v)| v),
                        _ => unreachable!(),
                    }
                }
                Some(other) => {
                    diags.push(rdiag(
                        Code::ExtUnresolved,
                        &path,
                        other,
                        "unknown extension selector spelling",
                        "spell the selector as `latest` or `version:<version_id>`",
                        kernel,
                    ));
                    continue;
                }
            }
        };
        let Some(vid) = target else {
            diags.push(rdiag(
                Code::ExtUnresolved,
                &path,
                &r.name,
                "no `extension` record matches the ref",
                "register the extension record or fix `name`/`kind`",
                kernel,
            ));
            continue;
        };
        // The governed path — revoked/stale refuses under `execute`.
        let resolved = match env.registry.resolve(
            &ResolveInput::Version(vid.clone()),
            env.mode,
            &ResolveRequest::default(),
        ) {
            Ok(r) => r,
            Err(e) => {
                diags.push(reg_err(&e, &path, &r.name, kernel));
                continue;
            }
        };
        let hh_registry::records::RegistryRecord::Extension(rec) = &resolved.record else {
            diags.push(rdiag(
                Code::ExtUnresolved,
                &path,
                &r.name,
                "the pin names a non-extension record",
                "pin an `extension` record",
                kernel,
            ));
            continue;
        };
        if rec.name != r.name || rec.kind != r.kind {
            diags.push(rdiag(
                Code::ExtUnresolved,
                &path,
                &r.name,
                "the pinned record's name/kind disagrees with the ref",
                "fix the ref's `name`/`kind` or the pin",
                kernel,
            ));
            continue;
        }
        // An authored `content` pin that disagrees with the record is a refusal —
        // the pin never silently moves.
        if let Some(c) = &r.content {
            if *c != rec.content {
                diags.push(rdiag(
                    Code::ExtUnresolved,
                    &path,
                    &r.name,
                    "the authored content pin disagrees with the resolved record",
                    "re-resolve or correct the pin",
                    kernel,
                ));
                continue;
            }
        }
        r.locator.selector = None;
        r.locator.resolved = Some(vid.clone());
        r.locator.fetched_at = Some(env.resolved_at);
        r.content = Some(rec.content.clone());
        r.extension_id = Some(vid);
    }
}

/// Resolve every binding in `assembly.slots`.
fn resolve_slots(
    a: &mut Assembly,
    env: &ResolveEnv,
    snapshot_id: &str,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) {
    let slot_names: Vec<String> = a.slots.keys().cloned().collect();
    for slot in slot_names {
        let class_id = slot_class(env.catalog, &slot);
        if class_id.is_none() {
            diags.push(rdiag(
                Code::ClassUnknown,
                &format!("/assembly/slots/{slot}"),
                &slot,
                "no catalog class owns the slot",
                "bind only slots named by a catalog class's `slot_key`",
                kernel,
            ));
            continue;
        }
        let path = format!("/assembly/slots/{slot}");
        if let Some(bs) = a.slots.get_mut(&slot) {
            match bs {
                SlotBindings::One(b) => resolve_variant(
                    &mut b.variant,
                    &slot,
                    class_id.as_deref(),
                    env,
                    snapshot_id,
                    &path,
                    diags,
                    kernel,
                ),
                SlotBindings::Many(v) => {
                    for (i, b) in v.iter_mut().enumerate() {
                        resolve_variant(
                            &mut b.variant,
                            &slot,
                            class_id.as_deref(),
                            env,
                            snapshot_id,
                            &format!("{path}[{i}]"),
                            diags,
                            kernel,
                        );
                    }
                }
            }
        }
    }
}

/// Resolve selector-carrying variant refs inside every native process's `native.slots`
/// **in place** (nested natives bind through the same resolver; pinned versions are
/// re-verified — a stale pin under `execute` still refuses).
fn resolve_native_slots(
    doc: &mut HirDocument,
    env: &ResolveEnv,
    snapshot_id: &str,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) {
    for n in &mut doc.nodes {
        let node_id = n.semantic_id();
        let KindRecord::AgentProcess(ap) = &mut n.semantic else {
            continue;
        };
        let AgentProcessBody::Native(native) = &mut ap.body else {
            continue;
        };
        for (slot, bs) in native.slots.iter_mut() {
            let class_id = slot_class(env.catalog, slot);
            let path = format!("/nodes/{node_id}/native/slots/{slot}");
            match bs {
                SlotBindings::One(b) => resolve_variant(
                    &mut b.variant,
                    slot,
                    class_id.as_deref(),
                    env,
                    snapshot_id,
                    &path,
                    diags,
                    kernel,
                ),
                SlotBindings::Many(v) => {
                    for (i, b) in v.iter_mut().enumerate() {
                        resolve_variant(
                            &mut b.variant,
                            slot,
                            class_id.as_deref(),
                            env,
                            snapshot_id,
                            &format!("{path}[{i}]"),
                            diags,
                            kernel,
                        );
                    }
                }
            }
        }
    }
}

/// The class a slot binds — `slot_key` first, `class_id` second (mirrors validate).
fn slot_class(catalog: &dyn ClassCatalog, slot: &str) -> Option<String> {
    for id in catalog.class_ids() {
        if let Some(c) = catalog.class(&id) {
            if c.slot_key == slot || c.class_id == slot {
                return Some(c.class_id);
            }
        }
    }
    None
}

// ── secrets ───────────────────────────────────────────────────────────────────

/// The inline-secret heuristic (ADR-0240): a param/value member *named* a secret
/// (`secret`, `token`, `api_key`, `password`, `*_secret`) carrying a non-`$secret:`
/// value, or any object carrying `secret_value`/`secret` inline payload members, is
/// `C-SEC-1` — secrets resolve to channel names, never values (§3.3.4).
fn secret_scan(a: &Assembly, diags: &mut Vec<AssemblyDiagnostic>, kernel: &ProvenanceRecord) {
    let slots_json = hh_hir::wire::slots_json(&a.slots, false);
    secret_scan_json(&slots_json, "/assembly/slots", diags, kernel);
    secret_scan_json(
        &Json::Obj(a.values.clone()),
        "/assembly/values",
        diags,
        kernel,
    );
    secret_scan_json(&Json::Obj(a.ext.clone()), "/assembly/ext", diags, kernel);
}

fn looks_secret_key(k: &str) -> bool {
    let kl = k.to_ascii_lowercase();
    kl == "secret"
        || kl == "token"
        || kl == "api_key"
        || kl == "password"
        || kl == "secret_value"
        || kl.ends_with("_secret")
        || kl.ends_with("_token")
        || kl.ends_with("_api_key")
        || kl.ends_with("_password")
}

fn secret_scan_json(
    j: &Json,
    path: &str,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                let p = format!("{path}/{k}");
                if looks_secret_key(k) {
                    let is_channel = matches!(v, Json::Str(s) if s.starts_with(SECRET_MARKER));
                    if !is_channel {
                        diags.push(rdiag(
                            Code::SecInlineSecret,
                            &p,
                            k,
                            "an inline secret where a `$secret:<name>` channel name is required",
                            "bind the secret by channel name — never inline a value",
                            kernel,
                        ));
                    }
                }
                secret_scan_json(v, &p, diags, kernel);
            }
        }
        Json::Arr(items) => {
            for (i, v) in items.iter().enumerate() {
                secret_scan_json(v, &format!("{path}[{i}]"), diags, kernel);
            }
        }
        _ => {}
    }
}

// ── diagnostics ───────────────────────────────────────────────────────────────

/// Map a `RegistryError` onto the closed taxonomy (never a string — `C-INT-1` is a
/// defect, so every registry failure lands on a named code).
pub fn reg_err(
    e: &RegistryError,
    path: &str,
    subject: &str,
    kernel: &ProvenanceRecord,
) -> AssemblyDiagnostic {
    use RegistryError::*;
    let (code, remedy) = match e {
        Unresolved { .. } => (
            Code::RefUnresolved,
            "check the name/label, the snapshot, and that the record is registered",
        ),
        Ambiguous { .. } => (
            Code::RefAmbiguous,
            "pin a `version_id` or a `label:` selector",
        ),
        NameCollision { .. } => (
            Code::RefNameCollision,
            "resolve inside the owning namespace's snapshot binding",
        ),
        Revoked { .. } | Stale { .. } | UnknownSnapshot { .. } => (
            Code::RefStaleIndex,
            "re-resolve under a fresh snapshot — the pinned closure moved",
        ),
        DialectIncompatible { .. } => (
            Code::ClassDialectIncompatible,
            "bind a variant whose dialect range covers the document dialect",
        ),
        ContractIncompatible { .. } => (
            Code::ClassVariantMismatch,
            "bind a variant of the slot's class and contract version",
        ),
        _ => (
            // Never `C-INT-1` — map the residual onto the closest named code.
            Code::RefUnresolved,
            "inspect the registry diagnostic on the store",
        ),
    };
    rdiag(code, path, subject, &format!("{e:?}"), remedy, kernel)
}

fn mkdiag(code: Code, path: &str, subject: &str, detail: &str) -> AssemblyDiagnostic {
    rdiag(
        code,
        path,
        subject,
        detail,
        "fix the reference",
        &ProvenanceRecord::kernel("assembly", 0),
    )
}

fn rdiag(
    code: Code,
    path: &str,
    subject: &str,
    detail: &str,
    remedy: &str,
    kernel: &ProvenanceRecord,
) -> AssemblyDiagnostic {
    AssemblyDiagnostic {
        code,
        class: None,
        severity: Severity::Error,
        path: path.to_string(),
        source_layer: None,
        subject: subject.to_string(),
        stage: Stage::Resolve,
        detail: detail_text(detail, kernel),
        remedy: remedy.to_string(),
        owner_adr: "ADR-0148".into(),
    }
}

// ── link_precheck (AC-CC-10; CF-056) ──────────────────────────────────────────

/// `link`'s Stage-1 refusal — the compiler consumes the sealed definition without
/// re-resolving selectors: any surviving `version_selector`/`Selector` is
/// `C-LINK-1 LinkError{unbound_slot}` (§3.3.4 `link`; the full link op is S1.10's —
/// DF-S1.9-3).
pub fn link_precheck(sealed: &SealedDefinition) -> Result<(), Vec<AssemblyDiagnostic>> {
    let kernel = ProvenanceRecord::kernel("assembly", 0);
    let mut diags = Vec::new();
    if let Some(a) = &sealed.document.assembly {
        collect_selectors(a, "/assembly", &mut diags, &kernel);
    }
    for n in &sealed.document.nodes {
        let j = hh_hir::wire::node_to_json(n);
        collect_selectors(
            &j,
            &format!("/nodes/{}", n.semantic_id()),
            &mut diags,
            &kernel,
        );
    }
    if diags.is_empty() {
        Ok(())
    } else {
        Err(diags)
    }
}

fn collect_selectors(
    j: &Json,
    path: &str,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) {
    match j {
        Json::Obj(m) => {
            if m.contains_key("version_selector") {
                let mut d = rdiag(
                    Code::LinkUnboundSlot,
                    path,
                    path,
                    "an unpinned selector reaches link — the compiler never re-resolves",
                    "resolve the definition before link",
                    kernel,
                );
                d.stage = Stage::Link;
                diags.push(d);
                return;
            }
            for (k, v) in m {
                collect_selectors(v, &format!("{path}/{k}"), diags, kernel);
            }
        }
        Json::Arr(items) => {
            for (i, v) in items.iter().enumerate() {
                collect_selectors(v, &format!("{path}[{i}]"), diags, kernel);
            }
        }
        _ => {}
    }
}

/// The `Ref` behind an `entities` member, when it is a ref.
pub fn entity_ref<'a>(a: &'a Assembly, entity_id: &str) -> Option<&'a Ref> {
    match a.entities.get(entity_id) {
        Some(EntityBinding::Ref(r)) => Some(r),
        _ => None,
    }
}

/// A resolved-assembly accessor — the `resolved` member.
pub fn resolved_info(a: &Assembly) -> Option<&ResolvedInfo> {
    a.resolved.as_ref()
}
