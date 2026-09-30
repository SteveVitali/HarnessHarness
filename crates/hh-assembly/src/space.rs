//! `space(sealed) → ParameterSpace` and `enumerate(space, design) →
//! [EnumeratedPoint]` (§3.3.4; C0/Stage 3; R-2.1.4) — the sweep vocabulary the
//! experiment engine consumes: the declared parameter space (never inferred
//! from code), the per-class bound variants, and the override grammar of
//! ADR-0025 (precedent S-185; CF-475):
//!
//! | spelling        | op                                              |
//! |-----------------|-------------------------------------------------|
//! | `path=v`        | set `values/path` (or a slot's `enabled`/`params`) |
//! | `+path=v`       | append `v` to the set at `path`                  |
//! | `++path=v`      | append-or-override at `path`                     |
//! | `~path`         | delete `path`                                    |
//! | `path=v1,v2`    | choice sweep — one point per value               |
//! | `class_id=variant_id` | **group override** — retarget the slot's variant |
//!
//! Every enumerated point passes `validate_assembly` before budget is spent —
//! [`enumerate`] runs it per point and routes failures to `rejected` (the
//! `validate_batch` stages 1–6a gate of ADR-0147; the full battery is run —
//! stronger, never weaker). Each point carries `search_budget`/`eval_budget`
//! **placeholders** (`None`) — no point is runnable until the experiment engine
//! fills them (AC-CC-08; T-LCD-14).

use std::collections::BTreeMap;
use std::fmt;

use hh_hir::document::HirDocument;
use hh_hir::records::{KindRecord, SlotBinding, SlotBindings};
use hh_hir::refs::{ComponentVariantRef, RefVersion};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::catalog::ClassCatalog;
use crate::diagnostics::ValidationReport;
use crate::grammar::{Assembly, Constraint, ParameterSpec};
use crate::validate::{validate_assembly, ProfileView, Subject};

/// `ParameterSpace{params, slot_choices, constraints}` (§3.3.4) — `space`'s
/// output: the declared parameters (sweepable ones carry their domains), the
/// per-class candidate variants the sealed point binds, the constraint set, and
/// the base document enumerate applies overrides to.
#[derive(Debug, Clone)]
pub struct ParameterSpace {
    /// Every declared parameter (`sweepable` marks the sweepable subset).
    pub params: BTreeMap<String, ParameterSpec>,
    /// `map<class_id, [ComponentVariantRef]>` — the bound variants per slot.
    pub slot_choices: BTreeMap<String, Vec<ComponentVariantRef>>,
    /// The constraint set the sweep must satisfy.
    pub constraints: Vec<Constraint>,
    /// The base document the points derive from.
    pub base: HirDocument,
}

/// `space` cannot read the subject (an authored document without a resolved
/// assembly section has no declared space).
#[derive(Debug)]
pub enum SpaceError {
    /// The document carries no parseable `assembly` member.
    NoAssembly(String),
}

impl fmt::Display for SpaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpaceError::NoAssembly(d) => write!(f, "SpaceError: {d}"),
        }
    }
}
impl std::error::Error for SpaceError {}

/// `space(sealed) → ParameterSpace` (§3.3.4).
pub fn space(
    sealed: &HirDocument,
    kernel: &ProvenanceRecord,
) -> Result<ParameterSpace, SpaceError> {
    let mut sink = Vec::new();
    let assembly = sealed
        .assembly
        .as_ref()
        .and_then(|j| Assembly::from_json(j, "/assembly", kernel, &mut sink))
        .ok_or_else(|| {
            SpaceError::NoAssembly("the document carries no readable `assembly` member".into())
        })?;
    let mut slot_choices: BTreeMap<String, Vec<ComponentVariantRef>> = BTreeMap::new();
    for (class_id, sb) in &assembly.slots {
        let variants = match sb {
            SlotBindings::One(b) => vec![b.variant.clone()],
            SlotBindings::Many(v) => v.iter().map(|b| b.variant.clone()).collect(),
        };
        slot_choices.insert(class_id.clone(), variants);
    }
    Ok(ParameterSpace {
        params: assembly.parameters.clone(),
        slot_choices,
        constraints: assembly.constraints.clone(),
        base: sealed.clone(),
    })
}

/// `Override.op` — the ADR-0025 override operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideOp {
    /// `path=v` — set.
    Set,
    /// `+path=v` — append to the set at `path`.
    Append,
    /// `++path=v` — append if absent, override the entry if present.
    AppendOrSet,
    /// `~path` — delete.
    Delete,
}

/// An `Override` (ADR-0025; CF-475): one path plus its operand value(s).
/// `path=v1,v2` parses to `Set` with two `values` — a *choice sweep* that
/// `enumerate` expands into one point per value.
#[derive(Debug, Clone, PartialEq)]
pub struct Override {
    /// The operator.
    pub op: OverrideOp,
    /// The path — a `values` param id, `slots/<class_id>/enabled`,
    /// `slots/<class_id>/params/<name>`, or a bare `class_id` (group override —
    /// the value is the retargeted `variant_id`).
    pub path: String,
    /// The operand values (`~path` carries none; a choice sweep carries ≥2).
    pub values: Vec<Json>,
    /// The authored spelling (recorded on the point for attribution).
    pub spelling: String,
}

/// `Override` parse failures — a typed refusal, never a guess.
#[derive(Debug)]
pub enum OverrideError {
    /// The spelling matched no grammar production.
    Malformed(String),
    /// A value failed to parse as canonical JSON scalar/array.
    BadValue(String),
    /// The override named nothing applicable in the declared space.
    Unapplied(String),
}

impl fmt::Display for OverrideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OverrideError::Malformed(s) => write!(f, "OverrideError{{malformed}}: {s}"),
            OverrideError::BadValue(d) => write!(f, "OverrideError{{bad_value}}: {d}"),
            OverrideError::Unapplied(d) => write!(f, "OverrideError{{unapplied}}: {d}"),
        }
    }
}
impl std::error::Error for OverrideError {}

impl Override {
    /// Parse one override spelling (the ADR-0025 grammar row).
    pub fn parse(s: &str) -> Result<Override, OverrideError> {
        let spelling = s.to_string();
        if let Some(path) = s.strip_prefix('~') {
            if path.is_empty() {
                return Err(OverrideError::Malformed(spelling));
            }
            return Ok(Override {
                op: OverrideOp::Delete,
                path: path.to_string(),
                values: Vec::new(),
                spelling,
            });
        }
        let (op, rest) = if let Some(r) = s.strip_prefix("++") {
            (OverrideOp::AppendOrSet, r)
        } else if let Some(r) = s.strip_prefix('+') {
            (OverrideOp::Append, r)
        } else {
            (OverrideOp::Set, s)
        };
        let Some((path, raw)) = rest.split_once('=') else {
            return Err(OverrideError::Malformed(spelling));
        };
        if path.is_empty() || raw.is_empty() {
            return Err(OverrideError::Malformed(spelling));
        }
        // `path=v1,v2` — a comma-split choice sweep (the `,` is unescaped at C0).
        let values: Result<Vec<Json>, OverrideError> = raw
            .split(',')
            .map(|v| parse_value(v).map_err(|_| OverrideError::BadValue(v.to_string())))
            .collect();
        Ok(Override {
            op,
            path: path.to_string(),
            values: values?,
            spelling,
        })
    }
}

/// A scalar override value — canonical JSON (`"s"`, `42`, `true`, `[…]`) or a
/// bare token treated as a string.
fn parse_value(raw: &str) -> Result<Json, String> {
    match hh_wire::canonical::parse_canonical(raw.as_bytes()) {
        Ok(j) => Ok(j),
        Err(_) => Ok(Json::str(raw)),
    }
}

/// The sweep design (`§3.3.4`): `FullFactorial` crosses every sweepable
/// parameter's domain; `Fractional{sample}` takes the first `sample` points of
/// that product (deterministic order); `OverrideList` applies each override
/// (a choice sweep expands to one point per value).
#[derive(Debug, Clone, PartialEq)]
pub enum SweepDesign {
    /// The full cross-product over `sweepable` parameter domains.
    FullFactorial,
    /// The first `sample` points of the factorial (deterministic truncation).
    Fractional {
        /// The number of points to emit.
        sample: usize,
    },
    /// One arm per override spelling.
    OverrideList(Vec<Override>),
}

/// One enumerated sweep point — the base document with the override applied,
/// the applied spellings, and the **`search_budget`/`eval_budget`
/// placeholders** the experiment engine must fill before the point is runnable
/// (AC-CC-08; T-LCD-14).
#[derive(Debug)]
pub struct EnumeratedPoint {
    /// The candidate definition (the base document with the overrides applied).
    pub document: HirDocument,
    /// The overrides applied to produce this point.
    pub overrides: Vec<String>,
    /// `search_budget` — `None` until the experiment engine fills it.
    pub search_budget: Option<Json>,
    /// `eval_budget` — `None` until the experiment engine fills it.
    pub eval_budget: Option<Json>,
    /// The point's `validate_assembly` report (status `ok` — failures route to
    /// `rejected`, never silently).
    pub validation: ValidationReport,
}

/// A candidate that failed the pre-spend validation gate.
#[derive(Debug)]
pub struct RejectedPoint {
    /// The overrides that produced the point.
    pub overrides: Vec<String>,
    /// The failing validation report.
    pub validation: ValidationReport,
}

/// `enumerate`'s result — passing points and refused candidates, both reported.
#[derive(Debug)]
pub struct EnumerateOutcome {
    /// The admissible points (validation `ok`; budget placeholders unfilled).
    pub points: Vec<EnumeratedPoint>,
    /// The candidates `validate_assembly` refused (never silently dropped).
    pub rejected: Vec<RejectedPoint>,
}

/// `enumerate(space, design) → EnumerateOutcome` (§3.3.4; AC-CC-08). Applies the
/// overrides to the base document, runs `validate_assembly` per point (the
/// pre-spend gate — ADR-0147), and returns passing points with unfilled budget
/// placeholders.
pub fn enumerate(
    space: &ParameterSpace,
    design: &SweepDesign,
    catalog: &dyn ClassCatalog,
    profiles: Option<&dyn ProfileView>,
    kernel: &ProvenanceRecord,
) -> EnumerateOutcome {
    let arms: Vec<(Vec<Override>, Vec<String>)> = match design {
        SweepDesign::FullFactorial => factorial(space, usize::MAX),
        SweepDesign::Fractional { sample } => factorial(space, *sample),
        SweepDesign::OverrideList(list) => {
            // Each override is one arm; a choice sweep expands into one arm per
            // value.
            let mut arms = Vec::new();
            for ov in list {
                if ov.values.len() > 1 && ov.op == OverrideOp::Set {
                    for v in &ov.values {
                        let mut single = ov.clone();
                        single.values = vec![v.clone()];
                        single.spelling = format!("{}={}", ov.path, render_value(v));
                        arms.push((vec![single.clone()], vec![single.spelling.clone()]));
                    }
                } else {
                    arms.push((vec![ov.clone()], vec![ov.spelling.clone()]));
                }
            }
            arms
        }
    };
    let mut out = EnumerateOutcome {
        points: Vec::new(),
        rejected: Vec::new(),
    };
    for (ovs, spellings) in arms {
        let mut doc = space.base.clone();
        let mut ok = true;
        for ov in &ovs {
            if apply_override(&mut doc, ov, kernel).is_err() {
                ok = false;
            }
        }
        let validation = if ok {
            validate_assembly(Subject::Authored(&doc), catalog, profiles, kernel)
        } else {
            let mut r = ValidationReport {
                diagnostics: vec![crate::diagnostics::AssemblyDiagnostic {
                    code: crate::diagnostics::Code::ParamUnknown,
                    class: None,
                    severity: crate::diagnostics::Severity::Error,
                    path: "/assembly".into(),
                    source_layer: None,
                    subject: spellings.join(", "),
                    stage: crate::diagnostics::Stage::Compose,
                    detail: crate::diagnostics::detail_text(
                        "an override named a path outside the declared space",
                        kernel,
                    ),
                    remedy: "sweep only declared parameters/slots".into(),
                    owner_adr: "ADR-0148".into(),
                }],
                ..Default::default()
            };
            r.finalize();
            r
        };
        if matches!(
            validation.status,
            crate::diagnostics::ReportStatus::Pass
                | crate::diagnostics::ReportStatus::PassWithWarnings
        ) {
            out.points.push(EnumeratedPoint {
                document: doc,
                overrides: spellings,
                search_budget: None,
                eval_budget: None,
                validation,
            });
        } else {
            out.rejected.push(RejectedPoint {
                overrides: spellings,
                validation,
            });
        }
    }
    out
}

/// The deterministic cross-product over `sweepable` parameters' `domain` lists
/// (BTreeMap order — byte-stable). `limit` truncates (`Fractional`).
fn factorial(space: &ParameterSpace, limit: usize) -> Vec<(Vec<Override>, Vec<String>)> {
    let sweepable: Vec<(&String, &ParameterSpec)> =
        space.params.iter().filter(|(_, p)| p.sweepable).collect();
    let mut arms: Vec<(Vec<Override>, Vec<String>)> = vec![(Vec::new(), Vec::new())];
    for (name, spec) in &sweepable {
        let domain: Vec<Json> = match &spec.domain {
            Some(Json::Arr(items)) => items.clone(),
            _ => Vec::new(), // a sweepable parameter without a list domain is stage-3's diagnostic
        };
        let mut next = Vec::new();
        for (ovs, sp) in &arms {
            for v in &domain {
                let mut ovs2 = ovs.clone();
                let mut sp2 = sp.clone();
                let ov = Override {
                    op: OverrideOp::Set,
                    path: (*name).clone(),
                    values: vec![v.clone()],
                    spelling: format!("{name}={}", render_value(v)),
                };
                sp2.push(ov.spelling.clone());
                ovs2.push(ov);
                next.push((ovs2, sp2));
            }
        }
        arms = next;
    }
    arms.truncate(limit);
    arms
}

/// Apply one override to the document's `assembly` member. Returns `Err` when
/// the path names nothing in the declared space (the caller then routes the
/// point to `rejected`).
pub fn apply_override(
    doc: &mut HirDocument,
    ov: &Override,
    kernel: &ProvenanceRecord,
) -> Result<(), OverrideError> {
    let mut sink = Vec::new();
    let mut a = doc
        .assembly
        .as_ref()
        .and_then(|j| Assembly::from_json(j, "/assembly", kernel, &mut sink))
        .ok_or_else(|| {
            OverrideError::Unapplied("the document carries no readable `assembly` member".into())
        })?;
    let touched_slots = ov.path.starts_with("slots/") || a.slots.contains_key(&ov.path);
    apply_to_assembly(&mut a, ov)?;
    if touched_slots {
        // A slot override un-resolves the section — the point is an authored
        // candidate (re-resolved before seal), and the `resolved` member must
        // not claim a stale pinning. The materialised `native.slots` mirror on
        // the root process moves with the desired-state record (a disagreeing
        // double binding is `C-COMP-2`).
        a.resolved = None;
        if let Some(root) = doc
            .nodes
            .iter_mut()
            .find(|n| n.semantic_id() == doc.root.semantic_id)
        {
            if let KindRecord::AgentProcess(ap) = &mut root.semantic {
                if let hh_hir::records::AgentProcessBody::Native(n) = &mut ap.body {
                    n.slots = a.slots.clone();
                }
            }
        }
    }
    doc.assembly = Some(a.to_json());
    Ok(())
}

/// The override application on the typed section.
fn apply_to_assembly(a: &mut Assembly, ov: &Override) -> Result<(), OverrideError> {
    // Group override — `<slot class_id>=<variant_id>` retargets the slot's
    // binding set (the retarget unpins the version to a selector: a sweep point
    // is re-resolved before it is sealed — §3.3.4 `resolve`).
    if let Some(sb) = a.slots.get_mut(&ov.path) {
        let variant_id = ov
            .values
            .first()
            .and_then(Json::as_str)
            .ok_or_else(|| OverrideError::Unapplied(ov.spelling.clone()))?
            .to_string();
        let class_id = match sb {
            SlotBindings::One(b) => b.variant.class_id.clone(),
            SlotBindings::Many(v) => v
                .first()
                .map(|b| b.variant.class_id.clone())
                .unwrap_or_default(),
        };
        let new_ref = ComponentVariantRef {
            class_id,
            variant_id,
            version: RefVersion::Selector("*".into()),
        };
        match sb {
            SlotBindings::One(b) => b.variant = new_ref,
            SlotBindings::Many(v) => {
                for b in v.iter_mut() {
                    b.variant = new_ref.clone();
                }
            }
        }
        return Ok(());
    }
    // `slots/<class>/<field>` paths — `enabled` and `params/<name>`.
    if let Some(rest) = ov.path.strip_prefix("slots/") {
        let mut segs = rest.split('/');
        let unapplied = || OverrideError::Unapplied(ov.spelling.clone());
        let class = segs.next().ok_or_else(unapplied)?;
        let field = segs.next().ok_or_else(unapplied)?;
        let sb = a.slots.get_mut(class).ok_or_else(unapplied)?;
        match field {
            "enabled" => {
                let v = match ov.op {
                    OverrideOp::Delete => return Err(unapplied()),
                    _ => ov.values.first().cloned().unwrap_or(Json::Bool(true)),
                };
                let flag = match v {
                    Json::Bool(b) => b,
                    _ => return Err(unapplied()),
                };
                match sb {
                    SlotBindings::One(b) => b.enabled = flag,
                    SlotBindings::Many(vs) => {
                        for b in vs.iter_mut() {
                            b.enabled = flag;
                        }
                    }
                }
                Ok(())
            }
            "params" => {
                let name = segs.next().ok_or_else(unapplied)?;
                for_binding_mut(sb, |b| match ov.op {
                    OverrideOp::Set | OverrideOp::AppendOrSet => {
                        b.params.insert(
                            name.to_string(),
                            ov.values.first().cloned().unwrap_or(Json::Null),
                        );
                        Ok(())
                    }
                    OverrideOp::Append => {
                        // Append onto the array at the param (creating it).
                        let entry = b
                            .params
                            .entry(name.to_string())
                            .or_insert_with(|| Json::Arr(Vec::new()));
                        match entry {
                            Json::Arr(items) => {
                                items.extend(ov.values.clone());
                                Ok(())
                            }
                            _ => Err(unapplied()),
                        }
                    }
                    OverrideOp::Delete => {
                        b.params.remove(name);
                        Ok(())
                    }
                })
            }
            _ => Err(unapplied()),
        }
    } else {
        // `values/<param>` — the parameter-space path. A bare name is shorthand
        // for `values/<name>`; `values/` prefix is also accepted.
        let unapplied = || OverrideError::Unapplied(ov.spelling.clone());
        let name = ov.path.strip_prefix("values/").unwrap_or(&ov.path);
        if !a.parameters.contains_key(name) && !a.values.contains_key(name) {
            return Err(unapplied()); // undeclared path — refused, never guessed
        }
        match ov.op {
            OverrideOp::Set | OverrideOp::AppendOrSet => {
                a.values.insert(
                    name.to_string(),
                    ov.values.first().cloned().unwrap_or(Json::Null),
                );
            }
            OverrideOp::Append => {
                let entry = a
                    .values
                    .entry(name.to_string())
                    .or_insert_with(|| Json::Arr(Vec::new()));
                match entry {
                    Json::Arr(items) => items.extend(ov.values.clone()),
                    _ => return Err(unapplied()),
                }
            }
            OverrideOp::Delete => {
                a.values.remove(name);
            }
        }
        Ok(())
    }
}

/// Apply `f` to every `SlotBinding` under the binding set.
fn for_binding_mut(
    sb: &mut SlotBindings,
    f: impl Fn(&mut SlotBinding) -> Result<(), OverrideError>,
) -> Result<(), OverrideError> {
    match sb {
        SlotBindings::One(b) => f(b),
        SlotBindings::Many(v) => {
            for b in v.iter_mut() {
                f(b)?;
            }
            Ok(())
        }
    }
}

/// Render a `Json` operand back into override-spelling form (for the recorded
/// `spelling`).
fn render_value(v: &Json) -> String {
    match v {
        Json::Str(s) => s.clone(),
        _ => v.to_canonical_string(),
    }
}

/// `validate_batch(subjects, catalog, profiles)` — the sweep engine's pre-spend
/// gate (ADR-0147): `validate_assembly` over each point, one report per point,
/// in input order.
pub fn validate_batch<'a>(
    subjects: impl IntoIterator<Item = Subject<'a>>,
    catalog: &dyn ClassCatalog,
    profiles: Option<&dyn ProfileView>,
    kernel: &ProvenanceRecord,
) -> Vec<ValidationReport> {
    subjects
        .into_iter()
        .map(|s| validate_assembly(s, catalog, profiles, kernel))
        .collect()
}
