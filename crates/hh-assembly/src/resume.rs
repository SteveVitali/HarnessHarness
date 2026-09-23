//! `verify_resume` (§3.3.4/§3.3.10): `verify_resume(persisted, current, ledger_view) →
//! Compatible | Incompatible{reasons[{path, rule}]}` — a *compatibility check*, never a
//! merge (the persisted sealed definition is the source of truth). The four Core rules:
//! dialect equal; every slot's `class_id` equal; tool inventory add-only wherever the
//! ledger shows a `context.artefact.delivered` event referencing a removed item
//! (T-LCD-13); budgets may only decrease mid-run when accounting allows. Every other
//! change is accepted — the caller records it as `lifecycle.definition.changed{diff_ref}`
//! (`crate::events::definition_changed` + `emit`).

use hh_hir::diff::{DiffDerivation, HirDiff};
use hh_hir::document::{HirDocument, SealedDefinition};
use hh_hir::records::{AgentProcessBody, DimensionBound, KindRecord, SlotBindings, SurfaceRecord};
use hh_ledger::event::EventEnvelope;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::diagnostics::AssemblyDiagnostic;
use crate::events::ARTEFACT_DELIVERED;

/// The budget-decrease gate — "accounting allows" (§3.3.4). The run's accounting layer
/// supplies the verdict (a `hh-budget`-backed gate is the kernel's); `DenyAll` is the
/// safe default.
pub trait BudgetGate {
    /// Whether a mid-run budget decrease at `path` (`old` → `new` dimension bounds) is
    /// allowed.
    fn decrease_allowed(&self, path: &str, old: &Json, new: &Json) -> bool;
}

/// Refuse every decrease — the default posture.
pub struct DenyAll;

impl BudgetGate for DenyAll {
    fn decrease_allowed(&self, _path: &str, _old: &Json, _new: &Json) -> bool {
        false
    }
}

/// Allow every decrease (tests + the kernel's accounting-backed gate).
pub struct AllowAll;

impl BudgetGate for AllowAll {
    fn decrease_allowed(&self, _path: &str, _old: &Json, _new: &Json) -> bool {
        true
    }
}

/// The `ledger_view` `verify_resume` reads (§3.3.4): the run's committed events plus
/// the accounting gate.
pub struct LedgerView<'a> {
    /// The run's committed `EventEnvelope`s (`store.events(run_id)`).
    pub events: &'a [EventEnvelope],
    /// The accounting gate for mid-run budget decreases.
    pub budget_gate: &'a dyn BudgetGate,
}

/// The closed `rule` set an `Incompatible` reason carries (§3.3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeRule {
    /// The document dialect differs.
    DialectChanged,
    /// A slot's bound `class_id` differs.
    SlotClassChanged,
    /// A tool the ledger delivered (a `context.artefact.delivered` event references it)
    /// is absent from `current` (T-LCD-13).
    DeliveredToolRemoved,
    /// A mid-run budget decrease the accounting gate refused.
    BudgetDecreaseDenied,
}

impl ResumeRule {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ResumeRule::DialectChanged => "dialect_changed",
            ResumeRule::SlotClassChanged => "slot_class_changed",
            ResumeRule::DeliveredToolRemoved => "delivered_tool_removed",
            ResumeRule::BudgetDecreaseDenied => "budget_decrease_denied",
        }
    }
}

/// One `Incompatible` reason — `{path, rule}` + a human detail.
#[derive(Debug, Clone, PartialEq)]
pub struct ResumeReason {
    /// The document path the reason attaches to.
    pub path: String,
    /// The closed rule.
    pub rule: ResumeRule,
    /// The detail spelling.
    pub detail: String,
}

/// The `verify_resume` verdict.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ResumeVerdict {
    /// The change is admissible — `diff` is the `HirDiff` to record as
    /// `lifecycle.definition.changed{diff_ref}` (`diff_ref` is the diff's content
    /// address under the one hasher — CC1).
    Compatible {
        /// The `HirDiff` (§3.3.4 `diff` is not a separate object).
        diff: HirDiff,
        /// `address(canonical(diff))` — the `diff_ref` the event records.
        diff_ref: String,
    },
    /// The change is refused.
    Incompatible {
        /// Every reason (never fail-fast — the whole set is reported).
        reasons: Vec<ResumeReason>,
    },
}

/// `verify_resume(persisted, current, ledger_view, provenance, derivation)` (§3.3.4).
/// `provenance`/`derivation` are the edit's provenance — the `Compatible` verdict's
/// `HirDiff` runs `hh_hir::diff`'s §3.1.7 gates (authority-widening, conditioned-rule,
/// model-origin narrowing); a gated refusal surfaces as `Err` (`C-KERN-*`
/// diagnostics), not a verdict.
pub fn verify_resume(
    persisted: &SealedDefinition,
    current: &SealedDefinition,
    view: &LedgerView,
    provenance: ProvenanceRecord,
    derivation: DiffDerivation,
) -> Result<ResumeVerdict, Vec<AssemblyDiagnostic>> {
    let mut reasons = Vec::new();
    let old = &persisted.document;
    let new = &current.document;

    // Rule 1 — dialect equal.
    if old.hir_version != new.hir_version {
        reasons.push(ResumeReason {
            path: "/hir_version".into(),
            rule: ResumeRule::DialectChanged,
            detail: format!("{} → {}", old.hir_version, new.hir_version),
        });
    }

    // Rule 2 — every slot's class_id equal (per AgentProcess, keyed by node
    // semantic_id + slot name).
    slot_classes(old, new, &mut reasons);

    // Rule 3 — tool inventory add-only where the ledger delivered the artefact.
    delivered_tools(old, new, view.events, &mut reasons);

    // Rule 4 — budgets may only decrease mid-run if accounting allows.
    budget_decreases(old, new, view.budget_gate, &mut reasons);

    if !reasons.is_empty() {
        return Ok(ResumeVerdict::Incompatible { reasons });
    }
    let d = crate::diff::diff(persisted, current, provenance, derivation)?;
    let diff_ref = diff_address(&d);
    Ok(ResumeVerdict::Compatible { diff: d, diff_ref })
}

/// The diff's content address (`hh_identity::idp::address` over the canonical
/// `HirDiff` JSON — the one hasher, CC1).
fn diff_address(d: &HirDiff) -> String {
    let j = hh_hir::wire::diff_to_json(d);
    let bytes = j.to_canonical_string();
    hh_identity::idp::address(bytes.as_bytes(), "application/x-hir-diff").id()
}

/// The `class_id`s bound per slot, per `AgentProcess` node (`node_sid → slot →
/// class_id`).
fn slot_classes(old: &HirDocument, new: &HirDocument, reasons: &mut Vec<ResumeReason>) {
    let collect = |doc: &HirDocument| -> std::collections::BTreeMap<(String, String), String> {
        let mut m = std::collections::BTreeMap::new();
        for n in &doc.nodes {
            let KindRecord::AgentProcess(ap) = &n.semantic else {
                continue;
            };
            let AgentProcessBody::Native(native) = &ap.body else {
                continue;
            };
            let sid = n.semantic_id();
            for (slot, bs) in &native.slots {
                let class_of = |b: &hh_hir::records::SlotBinding| b.variant.class_id.clone();
                let cid = match bs {
                    SlotBindings::One(b) => class_of(b),
                    SlotBindings::Many(v) => v.iter().map(class_of).collect::<Vec<_>>().join(","),
                };
                m.insert((sid.clone(), slot.clone()), cid);
            }
        }
        m
    };
    let a = collect(old);
    let b = collect(new);
    for (k, cid) in &a {
        match b.get(k) {
            Some(c2) if c2 == cid => {}
            Some(c2) => reasons.push(ResumeReason {
                path: format!("/nodes/{}/native/slots/{}", k.0, k.1),
                rule: ResumeRule::SlotClassChanged,
                detail: format!("slot `{}` class {cid} → {c2}", k.1),
            }),
            None => reasons.push(ResumeReason {
                path: format!("/nodes/{}/native/slots/{}", k.0, k.1),
                rule: ResumeRule::SlotClassChanged,
                detail: format!(
                    "slot `{}` ({cid}) is unbound in the current definition",
                    k.1
                ),
            }),
        }
    }
}

/// Rule 3 — the `context.artefact.delivered` payloads name artefacts (tool surface
/// names and semantic ids); a delivered item absent from `current`'s inventory refuses
/// the resume (T-LCD-13 — add-only).
fn delivered_tools(
    old: &HirDocument,
    new: &HirDocument,
    events: &[EventEnvelope],
    reasons: &mut Vec<ResumeReason>,
) {
    let mut delivered: std::collections::BTreeSet<String> = Default::default();
    for e in events {
        if e.class != ARTEFACT_DELIVERED {
            continue;
        }
        for key in ["artefact_id", "name", "tool", "tool_name", "surface_ref"] {
            if let Some(v) = e.payload.get(key).and_then(Json::as_str) {
                delivered.insert(v.to_string());
            }
        }
    }
    if delivered.is_empty() {
        return;
    }
    let inventory = |doc: &HirDocument| -> std::collections::BTreeSet<String> {
        let mut inv = std::collections::BTreeSet::new();
        for n in &doc.nodes {
            if matches!(n.kind, hh_hir::kinds::EntityKind::ToolCapability) {
                inv.insert(n.semantic_id());
                if let Some(SurfaceRecord::Tool(t)) = &n.surface {
                    inv.insert(t.name.clone());
                }
            }
        }
        inv
    };
    let old_inv = inventory(old);
    let new_inv = inventory(new);
    for item in &delivered {
        if old_inv.contains(item) && !new_inv.contains(item) {
            reasons.push(ResumeReason {
                path: format!("/nodes/{item}"),
                rule: ResumeRule::DeliveredToolRemoved,
                detail: format!(
                    "tool `{item}` was delivered (context.artefact.delivered) and is absent from the current definition"
                ),
            });
        }
    }
}

/// Rule 4 — compare `Budget` records by node `semantic_id`; a decrease asks the
/// accounting gate.
fn budget_decreases(
    old: &HirDocument,
    new: &HirDocument,
    gate: &dyn BudgetGate,
    reasons: &mut Vec<ResumeReason>,
) {
    fn budgets(
        doc: &HirDocument,
    ) -> std::collections::BTreeMap<String, &hh_hir::records::BudgetRecord> {
        doc.nodes
            .iter()
            .filter_map(|n| match &n.semantic {
                KindRecord::Budget(b) => Some((n.semantic_id(), b)),
                _ => None,
            })
            .collect()
    }
    let a = budgets(old);
    let b = budgets(new);
    let to_json = |d: &DimensionBound| {
        Json::obj([
            ("hard", d.hard.map_or(Json::Null, |h| Json::Int(h as i64))),
            ("soft", d.soft.map_or(Json::Null, |s| Json::Int(s as i64))),
        ])
    };
    // `None` bound = unbounded — a decrease is "new strictly tighter than old".
    let tighter = |new: Option<u64>, old: Option<u64>| match (new, old) {
        (Some(n), Some(o)) => n < o,
        (Some(_), None) => true, // a bound where none existed
        _ => false,
    };
    for (sid, old_b) in &a {
        let Some(new_b) = b.get(sid) else {
            continue; // a removed Budget is a structural change — the diff records it
        };
        for (dim, old_d) in &old_b.dimensions {
            let new_d = new_b.dimensions.get(dim).copied().unwrap_or_default();
            if tighter(new_d.hard, old_d.hard) || tighter(new_d.soft, old_d.soft) {
                let path = format!("/nodes/{sid}/dimensions/{dim}");
                if !gate.decrease_allowed(&path, &to_json(old_d), &to_json(&new_d)) {
                    reasons.push(ResumeReason {
                        path,
                        rule: ResumeRule::BudgetDecreaseDenied,
                        detail: format!("budget `{dim}` decreased mid-run and accounting refused"),
                    });
                }
            }
        }
    }
}
