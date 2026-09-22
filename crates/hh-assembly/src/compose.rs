//! `compose(layers[], merge_policy) → Assembly` (§3.3.4; C0/Stage 3; R-2.1.4) —
//! the layered-merge op of ADR-0024 decision 5: deterministic, order-defined,
//! per-field policy from [`crate::schema::MERGE_POLICIES`] (never generic
//! deep-merge), `authority_cap` monotone (a lower-precedence layer may only
//! narrow the cap — `C-COMP-1 AuthorityViolation` naming the capping layer,
//! AC-CC-05), a disablement from any layer wins (`stricter-wins` on
//! `/slots/*/enabled`), equal-precedence disagreement is `C-COMP-2
//! LayerConflict`, never silent last-wins.
//!
//! The composed document carries `layers[]` — every input layer's
//! `LayerProvenance`, precedence-ordered — and each composed constraint's
//! `source` names its authoring layer, so attribution (and `source_layer` on
//! diagnostics) survives the merge (OQ-076).

use std::collections::BTreeMap;

use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::diagnostics::{detail_text, AssemblyDiagnostic, Code, Severity, Stage};
use crate::grammar::{
    parameter_spec_from_json, parameter_spec_json, Assembly, ConstraintKind, ExtensionBlock,
    LayerProvenance, ProfileBinding, ASSEMBLY_DIALECT,
};
use crate::schema::merge_policy;
use hh_hir::records::SlotBindings;

/// The `/slots` merge accumulator: the winning binding set, the winning
/// precedence + authoring layer id, and per-position disablement flags with the
/// layer that set each (`stricter-wins` — any layer's `enabled=false` wins).
type SlotMerge<'a> = (
    SlotBindings,
    i64,
    String,
    Vec<Option<(bool, &'a LayerProvenance)>>,
);

/// One composition input — an authored assembly fragment plus the layer
/// provenance that attribution and the precedence order consult.
#[derive(Debug, Clone)]
pub struct Layer {
    /// The layer's provenance (`source_kind`, `id`, `version`, `precedence`).
    pub provenance: LayerProvenance,
    /// The layer's authored fragment (a partial `Assembly` — unset members are
    /// the grammar's empty forms).
    pub fragment: Assembly,
}

/// `compose(layers) → Assembly` — the merge is deterministic and order-defined:
/// layers apply in ascending `precedence` (ties are a conflict where both set a
/// `replace` path differently), each field merges by its declared
/// [`MergePolicy`], and the result's `layers[]` records the participating
/// sources. `Err` carries the complete diagnostic set (never fail-fast —
/// §3.3.5).
pub fn compose(
    layers: &[Layer],
    kernel: &ProvenanceRecord,
) -> Result<Assembly, Vec<AssemblyDiagnostic>> {
    let mut diags: Vec<AssemblyDiagnostic> = Vec::new();
    // Apply in ascending precedence; index keeps the authored order for
    // equal-precedence conflict reporting.
    let mut order: Vec<&Layer> = layers.iter().collect();
    order.sort_by_key(|l| l.provenance.precedence);

    let mut out = Assembly::empty();

    // ── `/dialect` — ForbiddenBelow(u64::MAX): the grammar dialect is not a
    // layer-settable field; a layer naming a different dialect is a conflict.
    let mut dialect: Option<(&str, &LayerProvenance)> = None;
    for l in &order {
        let d = l.fragment.dialect.as_str();
        match dialect {
            None => dialect = Some((d, &l.provenance)),
            Some((have, src)) if have != d => diags.push(comp_diag(
                Code::CompLayerConflict,
                "/assembly/dialect",
                d,
                &format!(
                    "layer `{}` authors dialect `{d}` where layer `{}` authored `{have}` — `dialect` is `forbidden-below`",
                    l.provenance.id, src.id
                ),
                "author one dialect across the layer stack",
                kernel,
                Some(&l.provenance),
            )),
            _ => {}
        }
    }
    out.dialect = dialect
        .map(|(d, _)| d.to_string())
        .unwrap_or_else(|| ASSEMBLY_DIALECT.into());

    // ── `/profile_binding` — Replace. `Unbound` is the unset marker; the
    // highest-precedence bound layer wins; equal-precedence disagreement is a
    // conflict.
    {
        let mut winner: Option<(&ProfileBinding, &LayerProvenance)> = None;
        for l in &order {
            if matches!(l.fragment.profile_binding, ProfileBinding::Unbound) {
                continue;
            }
            match winner {
                None => winner = Some((&l.fragment.profile_binding, &l.provenance)),
                Some((have, src)) => {
                    if have != &l.fragment.profile_binding {
                        if src.precedence == l.provenance.precedence {
                            diags.push(comp_diag(
                                Code::CompLayerConflict,
                                "/assembly/profile_binding",
                                &l.provenance.id,
                                &format!(
                                    "equal-precedence layers `{}` and `{}` set `profile_binding` differently",
                                    src.id, l.provenance.id
                                ),
                                "raise one layer's precedence or reconcile the bindings",
                                kernel,
                                Some(&l.provenance),
                            ));
                        } else {
                            winner = Some((&l.fragment.profile_binding, &l.provenance));
                        }
                    }
                }
            }
        }
        if let Some((b, _)) = winner {
            out.profile_binding = b.clone();
        }
    }

    // ── map fields — per-key merge under the declared policy. ────────────────
    merge_map(
        &order,
        "/parameters",
        &mut diags,
        kernel,
        |l| {
            l.fragment
                .parameters
                .iter()
                .map(|(k, v)| (k, parameter_spec_json(v)))
                .collect()
        },
        |j, path| parameter_spec_from_json(j, path).ok(),
        &mut out.parameters,
    );
    merge_map_json(
        &order,
        "/values",
        &mut diags,
        kernel,
        |l| l.fragment.values.iter().collect(),
        &mut out.values,
    );

    // entities — AppendSet: union; a key set differently by two layers is a
    // LayerConflict (a set union cannot hold both).
    {
        for l in &order {
            for (k, v) in &l.fragment.entities {
                match out.entities.get(k) {
                    None => {
                        out.entities.insert(k.clone(), v.clone());
                    }
                    Some(have) if have != v => diags.push(comp_diag(
                        Code::CompLayerConflict,
                        &format!("/assembly/entities/{k}"),
                        k,
                        &format!(
                            "entity `{k}` is bound differently by layer `{}` — `entities` is `append-set`",
                            l.provenance.id
                        ),
                        "give the two bindings one identity or rename one",
                        kernel,
                        Some(&l.provenance),
                    )),
                    _ => {}
                }
            }
        }
    }

    // ── `/slots` — per-key Replace for the binding set; `/slots/*/enabled`
    // StricterWins: a disablement (`enabled=false`) from *any* layer wins.
    {
        // Collect per-key (highest-precedence binding, any-disabled flags).
        let mut per_key: BTreeMap<String, SlotMerge<'_>> = BTreeMap::new();
        for l in &order {
            for (k, sb) in &l.fragment.slots {
                let flags: Vec<bool> = match sb {
                    SlotBindings::One(b) => vec![b.enabled],
                    SlotBindings::Many(v) => v.iter().map(|b| b.enabled).collect(),
                };
                match per_key.get_mut(k) {
                    None => {
                        let flags = flags
                            .into_iter()
                            .map(|f| Some((f, &l.provenance)))
                            .collect::<Vec<_>>();
                        per_key.insert(
                            k.clone(),
                            (
                                sb.clone(),
                                l.provenance.precedence,
                                l.provenance.id.clone(),
                                flags,
                            ),
                        );
                    }
                    Some(entry) => {
                        // Merge disablement flags (by position; missing positions are unset).
                        for (i, f) in flags.iter().enumerate() {
                            if i < entry.3.len() {
                                if entry.3[i].is_none() {
                                    entry.3[i] = Some((*f, &l.provenance));
                                } else if !*f {
                                    entry.3[i] = Some((false, &l.provenance));
                                }
                            } else {
                                entry.3.push(Some((*f, &l.provenance)));
                            }
                        }
                        if l.provenance.precedence > entry.1 {
                            entry.0 = sb.clone();
                            entry.1 = l.provenance.precedence;
                            entry.2 = l.provenance.id.clone();
                        } else if l.provenance.precedence == entry.1 && entry.0 != *sb {
                            // Equal-precedence disagreement on a `replace` path —
                            // compare ignoring `enabled` (stricter-wins owns it).
                            let mut a = entry.0.clone();
                            let mut b = sb.clone();
                            strip_enabled(&mut a);
                            strip_enabled(&mut b);
                            if a != b {
                                diags.push(comp_diag(
                                    Code::CompLayerConflict,
                                    &format!("/assembly/slots/{k}"),
                                    k,
                                    &format!(
                                        "equal-precedence layers `{}` and `{}` bind slot `{k}` differently",
                                        entry.2, l.provenance.id
                                    ),
                                    "raise one layer's precedence or reconcile the bindings",
                                    kernel,
                                    Some(&l.provenance),
                                ));
                            }
                        }
                    }
                }
            }
        }
        for (k, (mut sb, _, _, flags)) in per_key {
            // StricterWins: `enabled = false` from any layer wins, per element.
            match &mut sb {
                SlotBindings::One(b) => {
                    if flags.iter().flatten().any(|(f, _)| !*f) {
                        b.enabled = false;
                    }
                }
                SlotBindings::Many(v) => {
                    for (i, b) in v.iter_mut().enumerate() {
                        if flags.get(i).copied().flatten().is_some_and(|(f, _)| !f) {
                            b.enabled = false;
                        }
                    }
                }
            }
            out.slots.insert(k, sb);
        }
    }

    // ── `/constraints` — AppendSet; every constraint is stamped with its
    // authoring layer; then the authority-cap monotone check. ─────────────────
    {
        let mut seen: BTreeMap<String, ()> = BTreeMap::new();
        for l in &order {
            for c in &l.fragment.constraints {
                let mut stamped = c.clone();
                if stamped.source.is_none() {
                    stamped.source = Some(l.provenance.clone());
                }
                let key = format!(
                    "{:?}∥{}",
                    stamped.kind,
                    stamped.subject.to_canonical_string()
                );
                if seen.insert(key, ()).is_none() {
                    out.constraints.push(stamped);
                }
            }
        }
        check_authority_caps(&order, &mut diags, kernel);
    }

    // ── `extensions` — AppendSet over `sources`/`refs`; a merge-policy
    // disagreement between layers is a conflict. ──────────────────────────────
    {
        let mut merged: Option<ExtensionBlock> = None;
        for l in &order {
            let Some(eb) = &l.fragment.extensions else {
                continue;
            };
            match &mut merged {
                None => merged = Some(eb.clone()),
                Some(m) => {
                    if m.merge_policy != eb.merge_policy {
                        diags.push(comp_diag(
                            Code::CompLayerConflict,
                            "/assembly/extensions/merge_policy",
                            &l.provenance.id,
                            &format!(
                                "layer `{}` declares a different extension `merge_policy`",
                                l.provenance.id
                            ),
                            "one merge policy per composed document",
                            kernel,
                            Some(&l.provenance),
                        ));
                    }
                    for s in &eb.sources {
                        if !m.sources.iter().any(|x| x == s) {
                            m.sources.push(s.clone());
                        }
                    }
                    for r in &eb.refs {
                        if !m.refs.iter().any(|x| x == r) {
                            m.refs.push(r.clone());
                        }
                    }
                }
            }
        }
        out.extensions = merged;
    }

    // ── `ext` — AppendSet: union; a key set differently is a conflict. ────────
    for l in &order {
        for (k, v) in &l.fragment.ext {
            match out.ext.get(k) {
                None => {
                    out.ext.insert(k.clone(), v.clone());
                }
                Some(have) if have != v => diags.push(comp_diag(
                    Code::CompLayerConflict,
                    &format!("/assembly/ext/{k}"),
                    k,
                    &format!(
                        "`ext.{k}` is set differently by layer `{}` — `ext` is `append-set`",
                        l.provenance.id
                    ),
                    "rename one key or reconcile the values",
                    kernel,
                    Some(&l.provenance),
                )),
                _ => {}
            }
        }
    }

    // ── `layers[]` — every participant, precedence-ordered (highest first). ──
    let mut provs: Vec<LayerProvenance> = order.iter().map(|l| l.provenance.clone()).collect();
    provs.sort_by(|a, b| b.precedence.cmp(&a.precedence));
    out.layers = (!provs.is_empty()).then_some(provs);

    if diags.iter().any(|d| d.severity == Severity::Error) {
        Err(diags)
    } else {
        Ok(out)
    }
}

/// `merge_map` — per-key `Replace` over a typed map: the highest-precedence
/// layer carrying the key wins; equal-precedence disagreement is a conflict.
/// `to_json`/`from_json` keep the value in its canonical record form for the
/// equality check.
fn merge_map<T: PartialEq>(
    order: &[&Layer],
    path: &str,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
    entries: impl Fn(&Layer) -> Vec<(&String, Json)>,
    parse: impl Fn(&Json, &str) -> Option<T>,
    out: &mut BTreeMap<String, T>,
) {
    let _ = merge_policy(path); // the table declares the policy; Replace below.
    let mut staged: BTreeMap<String, (Json, i64, String)> = BTreeMap::new();
    for l in order {
        for (k, v) in entries(l) {
            match staged.get_mut(k) {
                None => {
                    staged.insert(
                        k.clone(),
                        (v, l.provenance.precedence, l.provenance.id.clone()),
                    );
                }
                Some((have, prec, src)) => {
                    if l.provenance.precedence > *prec {
                        *have = v;
                        *prec = l.provenance.precedence;
                        *src = l.provenance.id.clone();
                    } else if l.provenance.precedence == *prec && *have != v {
                        diags.push(comp_diag(
                            Code::CompLayerConflict,
                            &format!("{path}/{k}"),
                            k,
                            &format!(
                                "equal-precedence layers `{}` and `{}` set `{k}` differently",
                                src, l.provenance.id
                            ),
                            "raise one layer's precedence or reconcile the entries",
                            kernel,
                            Some(&l.provenance),
                        ));
                    }
                }
            }
        }
    }
    for (k, (j, _, _)) in staged {
        if let Some(v) = parse(&j, &format!("{path}/{k}")) {
            out.insert(k, v);
        }
    }
}

/// The `Json`-valued specialisation of [`merge_map`] (`values`).
fn merge_map_json(
    order: &[&Layer],
    path: &str,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
    entries: impl Fn(&Layer) -> Vec<(&String, &Json)>,
    out: &mut BTreeMap<String, Json>,
) {
    let mut staged: BTreeMap<String, (Json, i64, String)> = BTreeMap::new();
    for l in order {
        for (k, v) in entries(l) {
            match staged.get_mut(k) {
                None => {
                    staged.insert(
                        k.clone(),
                        (v.clone(), l.provenance.precedence, l.provenance.id.clone()),
                    );
                }
                Some((have, prec, src)) => {
                    if l.provenance.precedence > *prec {
                        *have = v.clone();
                        *prec = l.provenance.precedence;
                        *src = l.provenance.id.clone();
                    } else if l.provenance.precedence == *prec && *have != *v {
                        diags.push(comp_diag(
                            Code::CompLayerConflict,
                            &format!("{path}/{k}"),
                            k,
                            &format!(
                                "equal-precedence layers `{}` and `{}` set `{k}` differently",
                                src, l.provenance.id
                            ),
                            "raise one layer's precedence or reconcile the values",
                            kernel,
                            Some(&l.provenance),
                        ));
                    }
                }
            }
        }
    }
    for (k, (v, _, _)) in staged {
        out.insert(k, v);
    }
}

/// **Authority caps are monotone** (§3.3.3; AC-CC-05): an `authority_cap` from a
/// lower-precedence layer may only *narrow* the ceiling higher-precedence
/// layers set over the same `subject.of`; a widening attempt is `C-COMP-1
/// `AuthorityViolation` naming the capping layer in `source_layer`/`detail`.
fn check_authority_caps(
    order: &[&Layer],
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) {
    // Effective caps set by layers of strictly higher precedence, per subject.
    let mut caps: BTreeMap<String, (hh_provenance::AuthorityClass, i64, String)> = BTreeMap::new();
    for l in order.iter().rev() {
        for (i, c) in l.fragment.constraints.iter().enumerate() {
            if c.kind != ConstraintKind::AuthorityCap {
                continue;
            }
            let subject = c
                .subject
                .get("of")
                .and_then(Json::as_str)
                .unwrap_or("<unspecified>")
                .to_string();
            let ceiling = c
                .subject
                .get("ceiling")
                .and_then(Json::as_str)
                .and_then(hh_provenance::AuthorityClass::parse);
            let Some(ceiling) = ceiling else {
                continue; // a malformed ceiling is stage-4's diagnostic, not compose's
            };
            match caps.get(&subject) {
                Some((cap, cap_prec, cap_layer)) if ceiling > *cap => {
                    // A lower-precedence layer attempts a *looser* cap — refused,
                    // naming the capping layer (AC-CC-05).
                    diags.push(comp_diag(
                        Code::CompAuthorityViolation,
                        &format!("/assembly/constraints/{i}/subject"),
                        &subject,
                        &format!(
                            "layer `{}` (precedence {}) attempts `authority_cap` ceiling `{}` over `{subject}`, widening the `{}` cap set by layer `{}` (precedence {})",
                            l.provenance.id,
                            l.provenance.precedence,
                            ceiling.as_str(),
                            cap.as_str(),
                            cap_layer,
                            cap_prec
                        ),
                        "narrow the cap or raise the layer's precedence above the capping layer's",
                        kernel,
                        Some(&l.provenance),
                    ));
                }
                _ => {
                    // Narrower-or-equal, or the first cap seen at this precedence —
                    // the cap is admissible; the effective ceiling is the minimum.
                    match caps.get_mut(&subject) {
                        Some((cap, prec, layer))
                            if ceiling < *cap
                                || (ceiling == *cap && l.provenance.precedence >= *prec) =>
                        {
                            if ceiling < *cap || l.provenance.precedence > *prec {
                                *cap = ceiling;
                                *prec = l.provenance.precedence;
                                *layer = l.provenance.id.clone();
                            }
                        }
                        None => {
                            caps.insert(
                                subject,
                                (ceiling, l.provenance.precedence, l.provenance.id.clone()),
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Clear `enabled` flags before the equal-precedence `replace` comparison —
/// `/slots/*/enabled` is `stricter-wins`, not part of the replaceable payload.
fn strip_enabled(sb: &mut SlotBindings) {
    match sb {
        SlotBindings::One(b) => b.enabled = true,
        SlotBindings::Many(v) => {
            for b in v.iter_mut() {
                b.enabled = true;
            }
        }
    }
}

/// A compose-stage diagnostic (`Stage::Compose`, `source_layer` set).
fn comp_diag(
    code: Code,
    path: &str,
    subject: &str,
    detail: &str,
    remedy: &str,
    kernel: &ProvenanceRecord,
    layer: Option<&LayerProvenance>,
) -> AssemblyDiagnostic {
    AssemblyDiagnostic {
        code,
        class: None,
        severity: Severity::Error,
        path: path.to_string(),
        source_layer: layer.map(|l| l.id.clone()),
        subject: subject.to_string(),
        stage: Stage::Compose,
        detail: detail_text(detail, kernel),
        remedy: remedy.to_string(),
        owner_adr: "ADR-0148".into(),
    }
}
