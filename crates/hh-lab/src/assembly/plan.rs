//! `plan` (§6.1 §2.1; ADR-0147 D2, ADR-0149) — the `AssemblyPlan` record:
//! `{diff, defaulted_paths[], authored_paths[], layer_map: path →
//! LayerProvenance, drift?, exit ∈ {no_changes, changes, error}, sameness}`.
//! `plan` is `assemble(…, plan)` plus a diff against `base` or the named
//! lineage head; `exit = error` carries the diagnostics.

use std::collections::BTreeMap;

use hh_assembly::diagnostics::AssemblyDiagnostic;
use hh_assembly::grammar::{Assembly, LayerProvenance};
use hh_identity::sameness::SamenessLevel;
use hh_wire::json::Json;

use crate::assembly::desugar::{Desugared, ExperimentLayer};
use crate::assembly::diff_view::{diff_json, sameness_name, AssemblyDiff};

/// `plan`'s exit status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanExit {
    /// No semantic difference against the comparison target.
    NoChanges,
    /// The plan introduces changes.
    Changes,
    /// The pipeline failed — the diagnostics say why.
    Error,
}

impl PlanExit {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            PlanExit::NoChanges => "no_changes",
            PlanExit::Changes => "changes",
            PlanExit::Error => "error",
        }
    }
}

/// The `AssemblyPlan` record (§6.1 §2.1).
#[derive(Debug, Clone)]
pub struct AssemblyPlan {
    /// The projected diff (vs `base` / lineage head, or `(sealed_a, sealed_b)`).
    pub diff: Option<AssemblyDiff>,
    /// Paths `default`ing filled (S-8: values only — never a slot).
    pub defaulted_paths: Vec<String>,
    /// Paths an authored layer/override set (pre-default).
    pub authored_paths: Vec<String>,
    /// `path → LayerProvenance` — the winning writer per assembly path.
    pub layer_map: BTreeMap<String, LayerProvenance>,
    /// `C-REF-6` evidence: the drift projection (present when re-resolving
    /// under a newer snapshot changes a pin).
    pub drift: Option<AssemblyDiff>,
    /// `no_changes | changes | error`.
    pub exit: PlanExit,
    /// `L0..L4` against the comparison target, when computable.
    pub sameness: Option<SamenessLevel>,
    /// The diagnostics the plan pipeline produced (V-1: every failure typed).
    pub diagnostics: Vec<AssemblyDiagnostic>,
}

impl AssemblyPlan {
    /// An errored plan carrying diagnostics only.
    pub fn error(diagnostics: Vec<AssemblyDiagnostic>) -> AssemblyPlan {
        AssemblyPlan {
            diff: None,
            defaulted_paths: Vec::new(),
            authored_paths: Vec::new(),
            layer_map: BTreeMap::new(),
            drift: None,
            exit: PlanExit::Error,
            sameness: None,
            diagnostics,
        }
    }

    /// The plan's canonical JSON (the wire form `lab.assembly.plan` returns).
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "diff",
                self.diff.as_ref().map(diff_json).unwrap_or(Json::Null),
            ),
            (
                "defaulted_paths",
                Json::Arr(
                    self.defaulted_paths
                        .iter()
                        .map(|p| Json::str(p.clone()))
                        .collect(),
                ),
            ),
            (
                "authored_paths",
                Json::Arr(
                    self.authored_paths
                        .iter()
                        .map(|p| Json::str(p.clone()))
                        .collect(),
                ),
            ),
            (
                "layer_map",
                Json::Obj(
                    self.layer_map
                        .iter()
                        .map(|(k, v)| (k.clone(), hh_assembly::grammar::layer_json(v)))
                        .collect(),
                ),
            ),
            (
                "drift",
                self.drift.as_ref().map(diff_json).unwrap_or(Json::Null),
            ),
            ("exit", Json::str(self.exit.name())),
            (
                "sameness",
                self.sameness
                    .map(sameness_name)
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
            (
                "diagnostics",
                Json::Arr(
                    self.diagnostics
                        .iter()
                        .map(hh_assembly::diagnostics::diagnostic_json)
                        .collect(),
                ),
            ),
        ])
    }
}

/// The assembly-member paths a fragment sets (`slots.<c>`, `values.<p>`, …) —
/// the `layer_map`'s key space.
pub fn fragment_paths(a: &Assembly) -> Vec<String> {
    let mut out = Vec::new();
    for c in a.slots.keys() {
        out.push(format!("slots.{c}"));
    }
    for p in a.parameters.keys() {
        out.push(format!("parameters.{p}"));
    }
    for v in a.values.keys() {
        out.push(format!("values.{v}"));
    }
    for e in a.entities.keys() {
        out.push(format!("entities.{e}"));
    }
    if !matches!(
        a.profile_binding,
        hh_assembly::grammar::ProfileBinding::Unbound
    ) {
        out.push("profile_binding".to_string());
    }
    for _ in &a.constraints {
        out.push("constraints".to_string());
    }
    for k in a.ext.keys() {
        out.push(format!("ext.{k}"));
    }
    out
}

/// `layer_map` — ascending precedence, last writer wins per path; the
/// experiment layer's touched paths join at top precedence.
pub fn layer_map(d: &Desugared) -> BTreeMap<String, LayerProvenance> {
    let mut order: Vec<&hh_assembly::compose::Layer> = d.layers.iter().collect();
    order.sort_by_key(|l| l.provenance.precedence);
    let mut map = BTreeMap::new();
    for l in order {
        for p in fragment_paths(&l.fragment) {
            map.insert(p, l.provenance.clone());
        }
    }
    if let Some(exp) = &d.experiment {
        for p in experiment_paths(exp) {
            map.insert(p, exp.provenance.clone());
        }
    }
    map
}

/// The paths an experiment layer's ops touch.
fn experiment_paths(exp: &ExperimentLayer) -> Vec<String> {
    let mut out = Vec::new();
    for op in &exp.ops {
        match op {
            crate::assembly::desugar::OverrideOp::Bind { class_id, .. } => {
                out.push(format!("slots.{class_id}"));
            }
            crate::assembly::desugar::OverrideOp::Set { path, .. }
            | crate::assembly::desugar::OverrideOp::Append { path, .. }
            | crate::assembly::desugar::OverrideOp::AppendOrOverride { path, .. }
            | crate::assembly::desugar::OverrideOp::Delete { path }
            | crate::assembly::desugar::OverrideOp::Choice { path, .. } => out.push(path.clone()),
        }
    }
    out
}

/// `defaulted_paths` (S-8) — `values.<p>` the composed assembly's parameter
/// `default` fills where no layer authored a value.
pub fn defaulted_paths(composed: &Assembly, authored: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for (p, spec) in &composed.parameters {
        if spec.default.is_some() {
            let path = format!("values.{p}");
            if !composed.values.contains_key(p) && !authored.iter().any(|a| a == &path) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}
