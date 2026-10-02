//! The **only** label increases: `endorse` → `security.label.endorsed` and `declassify` →
//! `security.label.declassified` (§8.1 #2; ADR-0035 D1/D2 as amended ADR-0115; CF-150) — plus
//! `security.label.applied` (the stamp event) and the `seal` basis.
//!
//! The endorsement basis is a **closed list** (§8.1 #3; adding a basis is a dialect bump):
//! `seal` · `approval` · `validator` · `promotion` · `pin` · `policy_rule`. The constitutional
//! rules (ADR-0035 Reversibility): **the endorser class must be ≥ the target** and **the model
//! never endorses** — an endorser whose origin is `model`/`evolution`/`participant` (the
//! `delegate` class) is never valid. Verdicts never endorse or confer.
//!
//! Stage-1 reach (§8.1 #9): the `seal` basis is exercised here; `approval`, `promotion`,
//! `validator`, `pin`, `policy_rule` land their *rules* now and their machinery at Stage 2/C2
//! (`approval`-basis handles S2.6, `pin` via `TrustRootPolicy` C2, `policy_rule` declassify
//! C2). Every refusal below is a typed error — never a warning (ADR-0033 D7).

use hh_wire::json::Json;

use crate::authority::{AuthorityClass, ReaderSet};
use crate::label::{classify_label_delta, Label, LabelDelta};
use crate::origin::Origin;
use crate::record::{AttestationKind, ProvenanceRecord};

/// `EndorsementBasis` — the **closed** list (§8.1 #3; ADR-0035 D2 as amended
/// ADR-0063/0069/0053/0115; CF-139/142/147/150/154).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EndorsementBasis {
    /// `seal` — the harness author at `seal` → `definition`. Imported instructions/skills may
    /// be sealed **only with `pin`**.
    Seal,
    /// `approval` — the human principal endorses **one Effect intent** `(effect_id,
    /// args_canonical_hash)` — never content, never a class. `basis_ref = permission_id`.
    Approval,
    /// `validator` — a deterministic `kernel` Validator raises an `external` value validated
    /// against a **closed schema** declared in the sealed definition → `environment`. Free
    /// text never.
    Validator,
    /// `promotion` — a human reviewer promotes a memory/procedure → `principal`/`definition`.
    Promotion,
    /// `pin` — a verified signature/hash under `TrustRootPolicy` → `definition` or `principal`
    /// per policy.
    Pin,
    /// `policy_rule` — the kernel executing a `HarnessRule` issued at `definition` declassifies
    /// readers to a named recipient set. Valid only in [`declassify`], never `endorse`.
    PolicyRule,
}

impl EndorsementBasis {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EndorsementBasis::Seal => "seal",
            EndorsementBasis::Approval => "approval",
            EndorsementBasis::Validator => "validator",
            EndorsementBasis::Promotion => "promotion",
            EndorsementBasis::Pin => "pin",
            EndorsementBasis::PolicyRule => "policy_rule",
        }
    }

    /// All six bases (the list is closed — a new basis is a dialect bump).
    pub const ALL: [EndorsementBasis; 6] = [
        EndorsementBasis::Seal,
        EndorsementBasis::Approval,
        EndorsementBasis::Validator,
        EndorsementBasis::Promotion,
        EndorsementBasis::Pin,
        EndorsementBasis::PolicyRule,
    ];
}

/// What the endorsement applies to — the `kind` in `BasisNotAllowed(basis, kind)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    /// Free text (a `Text` leaf) — `validator` can never apply (R-TEXT).
    FreeText,
    /// A closed-schema structured value — the only thing `validator` may raise to
    /// `environment`.
    ClosedSchemaValue,
    /// An Effect intent `(effect_id, args_canonical_hash)` — the only thing `approval`
    /// endorses.
    EffectIntent,
    /// Any other content-bearing record.
    Other,
}

/// `security.label.endorsed` payload: `{subject_ref, from, to, endorser, basis, basis_ref}`
/// (§8.1 #3 — the renamed `security.taint.*` family, CF-077).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelEndorsed {
    /// The record whose label rose.
    pub subject_ref: String,
    /// The label before.
    pub from: Label,
    /// The label after.
    pub to: Label,
    /// The endorser's provenance record.
    pub endorser: ProvenanceRecord,
    /// The basis — closed list.
    pub basis: EndorsementBasis,
    /// The basis anchor (`permission_id` for `approval`; pin/policy refs for others).
    pub basis_ref: Option<String>,
    /// The measured `capacity_bits` of the subject's shape schema when the
    /// endorsement ran under a shape bound (§5g.2 §3 `cap_max`; absent when
    /// the basis needed no shape evidence).
    pub capacity_bits: Option<u64>,
    /// The bounded sanitizer the endorsement ran under (`policy_rule (b)`),
    /// when one applied.
    pub sanitizer_ref: Option<String>,
    /// The D-ROBUST inputs the endorsement covered — the parameter paths the
    /// shape endorsement discharged (§5g.2 §4).
    pub robustness_inputs: Vec<String>,
}

/// `security.label.declassified` payload — widens **readers only**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelDeclassified {
    /// The record whose readers widened.
    pub subject_ref: String,
    /// The label before.
    pub from: Label,
    /// The label after — identical `authority`/`taint`, wider `readers`.
    pub to: Label,
    /// The declassifier's provenance record.
    pub endorser: ProvenanceRecord,
    /// The basis (only `policy_rule` declassifies).
    pub basis: EndorsementBasis,
    /// The basis anchor (the `HarnessRule` / policy ref).
    pub basis_ref: Option<String>,
}

/// `security.label.applied` payload — the stamping event for a label application that is not
/// an increase (initial stamping; §8.1 #3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelApplied {
    /// The record stamped.
    pub subject_ref: String,
    /// The label applied.
    pub label: Label,
    /// Who applied it.
    pub applied_by: ProvenanceRecord,
}

impl LabelEndorsed {
    /// Canonical JSON (one canonicalizer — `hh-wire` sorted-key compact).
    /// `capacity_bits`/`sanitizer_ref`/`robustness_inputs` are honestly
    /// absent unless the endorsement ran under the shape/sanitizer bound.
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("subject_ref", Json::str(self.subject_ref.clone())),
            ("from", label_json(&self.from)),
            ("to", label_json(&self.to)),
            ("basis", Json::str(self.basis.as_str())),
        ];
        if let Some(bits) = self.capacity_bits {
            m.push(("capacity_bits", Json::Int(bits as i64)));
        }
        if let Some(s) = &self.sanitizer_ref {
            m.push(("sanitizer_ref", Json::str(s.clone())));
        }
        if !self.robustness_inputs.is_empty() {
            m.push((
                "robustness_inputs",
                Json::Arr(
                    self.robustness_inputs
                        .iter()
                        .map(|p| Json::str(p.clone()))
                        .collect(),
                ),
            ));
        }
        Json::obj(m)
    }
}

fn label_json(l: &Label) -> Json {
    Json::obj([
        ("authority", Json::str(l.authority.as_str())),
        (
            "taint",
            Json::Arr(l.taint.iter().map(|t| Json::str(t.as_string())).collect()),
        ),
    ])
}

/// Endorsement/declassification failure modes (§8.1 #2/#5) — typed append refusals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndorsementError {
    /// The endorser's origin is `model`/`evolution`/`participant` (the `delegate` class) —
    /// a delegate may propose but never endorse ("the model never endorses").
    IllegitimateEndorsement {
        /// Why the endorsement is illegitimate.
        detail: String,
    },
    /// The basis may not apply to this content kind (`validator` on free text, `approval` on
    /// content, `seal` on imported instructions, `policy_rule` in `endorse`, …).
    BasisNotAllowed {
        /// The basis attempted.
        basis: EndorsementBasis,
        /// What it was attempted on.
        kind: &'static str,
    },
    /// `authority(endorser) < to.authority` — the endorser-class rule.
    EndorserBelowTarget {
        /// The endorser's class.
        endorser: AuthorityClass,
        /// The target class.
        target: AuthorityClass,
    },
    /// The `to` label does not rise (`endorse` only emits when a label actually rises — the
    /// approval-laundering guard, §8.1 #5).
    NoLabelIncrease,
    /// A `pin` endorsement without a verified attestation on the subject.
    PinRequiresVerifiedAttestation,
    /// A bounded sanitizer's realized output violated its declared
    /// `SanitizerBounds` — the `policy_rule (b)` endorsement is never emitted
    /// (`SanitizerBoundExceeded`; §5g.2 §5 failure row).
    SanitizerBoundExceeded {
        /// What exceeded the bound.
        detail: String,
    },
    /// A shape endorsement on a schema whose `capacity_bits` exceeds the
    /// definition's `cap_max` (§5g.2 §3).
    ShapeCapacityExceeded {
        /// The measured capacity.
        capacity_bits: u64,
        /// The declared ceiling.
        cap_max: u64,
    },
}

/// `endorse(subject, to, endorser, basis, basis_ref) → security.label.endorsed` (§8.1 #2):
/// valid iff `authority(endorser) ≥ to.authority ∧ basis ∈ closed list`, plus the per-basis
/// rules of §8.1 #3. Every label increase is this ledger event; a refused attempt is itself a
/// ledger fact.
///
/// `subject_ref` is the identity coordinate of the record being endorsed; `subject` is its
/// current provenance record (the `from` label and the origin rules read from it).
pub fn endorse(
    subject: &ProvenanceRecord,
    subject_ref: impl Into<String>,
    to: &Label,
    endorser: &ProvenanceRecord,
    basis: EndorsementBasis,
    basis_ref: Option<String>,
    content_kind: ContentKind,
) -> Result<LabelEndorsed, EndorsementError> {
    let from = subject.label();
    // The label must actually rise — at least one upward component (higher authority, a taint
    // tag dropped, a reader added). `None`/`Narrowing` deltas are not endorsements.
    match classify_label_delta(&from, to) {
        LabelDelta::None | LabelDelta::Narrowing => {
            return Err(EndorsementError::NoLabelIncrease);
        }
        LabelDelta::Widening | LabelDelta::MixedWidening => {}
    }
    // Constitutional rule 1: a delegate-class endorser is never valid.
    if endorser.origin.is_delegate_class() {
        return Err(EndorsementError::IllegitimateEndorsement {
            detail: "a delegate-class origin (model/evolution/participant) never endorses"
                .to_string(),
        });
    }
    // Constitutional rule 2: endorser class ≥ target.
    if endorser.authority < to.authority {
        return Err(EndorsementError::EndorserBelowTarget {
            endorser: endorser.authority,
            target: to.authority,
        });
    }
    // Per-basis rules (the closed list).
    match basis {
        EndorsementBasis::Seal => {
            // → definition only; imported/migrated instructions may seal only with `pin`.
            if to.authority != AuthorityClass::Definition {
                return Err(EndorsementError::BasisNotAllowed {
                    basis,
                    kind: "non-definition target",
                });
            }
            if matches!(
                subject.origin,
                Origin::Import { .. } | Origin::Migration { .. }
            ) {
                return Err(EndorsementError::BasisNotAllowed {
                    basis,
                    kind: "imported/migrated content (requires pin)",
                });
            }
            if !matches!(
                endorser.origin,
                Origin::Kernel { .. } | Origin::Human { .. }
            ) {
                return Err(EndorsementError::IllegitimateEndorsement {
                    detail: "seal is executed by the kernel on the harness author's behalf"
                        .to_string(),
                });
            }
        }
        EndorsementBasis::Approval => {
            // `approval` endorses one Effect intent — never content, never a class — and is
            // emitted only when a label actually rises (checked above) with
            // `basis_ref = permission_id`.
            if content_kind != ContentKind::EffectIntent {
                return Err(EndorsementError::BasisNotAllowed {
                    basis,
                    kind: "non-effect-intent content",
                });
            }
            if basis_ref.is_none() {
                return Err(EndorsementError::BasisNotAllowed {
                    basis,
                    kind: "approval without permission_id basis_ref",
                });
            }
        }
        EndorsementBasis::Validator => {
            // A deterministic kernel Validator raises an `external` closed-schema value to
            // `environment` — free text never.
            if content_kind != ContentKind::ClosedSchemaValue {
                return Err(EndorsementError::BasisNotAllowed {
                    basis,
                    kind: "free text / non-closed-schema content",
                });
            }
            if from.authority != AuthorityClass::External
                || to.authority != AuthorityClass::Environment
            {
                return Err(EndorsementError::BasisNotAllowed {
                    basis,
                    kind: "validator raises only external → environment",
                });
            }
            if !matches!(endorser.origin, Origin::Kernel { .. }) {
                return Err(EndorsementError::IllegitimateEndorsement {
                    detail: "the validator basis requires a deterministic kernel Validator"
                        .to_string(),
                });
            }
        }
        EndorsementBasis::Promotion => {
            // A human reviewer promotes a memory/procedure → principal/definition.
            if !matches!(
                to.authority,
                AuthorityClass::Principal | AuthorityClass::Definition
            ) {
                return Err(EndorsementError::BasisNotAllowed {
                    basis,
                    kind: "promotion target outside {principal, definition}",
                });
            }
            if !matches!(
                endorser.origin,
                Origin::Human { .. } | Origin::Kernel { .. }
            ) {
                return Err(EndorsementError::IllegitimateEndorsement {
                    detail: "promotion requires a human reviewer (or the kernel executing it)"
                        .to_string(),
                });
            }
        }
        EndorsementBasis::Pin => {
            // Verified signature/hash under TrustRootPolicy → definition or principal.
            if !matches!(
                to.authority,
                AuthorityClass::Definition | AuthorityClass::Principal
            ) {
                return Err(EndorsementError::BasisNotAllowed {
                    basis,
                    kind: "pin target outside {definition, principal}",
                });
            }
            let ok = subject.attestation.as_ref().is_some_and(|a| {
                a.self_consistent()
                    && matches!(
                        a.kind,
                        AttestationKind::Pin | AttestationKind::Signature | AttestationKind::Seal
                    )
            });
            if !ok {
                return Err(EndorsementError::PinRequiresVerifiedAttestation);
            }
        }
        EndorsementBasis::PolicyRule => {
            // `policy_rule` declassifies readers to a named recipient set — it never raises a
            // label in `endorse`.
            return Err(EndorsementError::BasisNotAllowed {
                basis,
                kind: "endorse (policy_rule declassifies readers only)",
            });
        }
    }
    // The C2 per-basis effects table (§5g.2 §2.2): on top of the authority rules above,
    // `seal`/`pin`/`promotion`/`validator` must satisfy their componentwise effect — a
    // strict authority raise with `taint` cleared (`validator` fixed to
    // `external → environment`). `approval` is exempt: it endorses one Effect intent, never
    // a content label — the table's `approval → NoLabelChange` row is why no
    // `approval`-basis endorsement of *content* is ever emitted. `policy_rule` never
    // reaches here (refused above — it declassifies readers only).
    if !matches!(
        basis,
        EndorsementBasis::Approval | EndorsementBasis::PolicyRule
    ) {
        crate::flow::check_basis_effect(basis, &from, to)
            .map_err(|e| crate::flow::basis_effect_endorsement_error(basis, &e))?;
    }
    Ok(LabelEndorsed {
        subject_ref: subject_ref.into(),
        from,
        to: to.clone(),
        endorser: endorser.clone(),
        basis,
        basis_ref,
        capacity_bits: None,
        sanitizer_ref: None,
        robustness_inputs: Vec::new(),
    })
}

/// The `seal` basis helper (§8.1 #9 lands `seal` at Stage 1): the kernel seals a definition on
/// the harness author's behalf — `to.authority = definition`, `taint` must be empty (a tainted
/// record can never reach `definition` — `TaintedAboveExternal`), readers unchanged.
pub fn seal(
    subject: &ProvenanceRecord,
    subject_ref: impl Into<String>,
    sealing_component: &ProvenanceRecord,
    basis_ref: Option<String>,
) -> Result<LabelEndorsed, EndorsementError> {
    let mut to = subject.label();
    to.authority = AuthorityClass::Definition;
    // The `seal` basis clears taint (§5g.2 §2.2) — the endorsed label is
    // `definition`/untainted; a tainted subject is *cleared by the endorsement*, and the
    // resulting record satisfies `TaintedAboveExternal` at `definition`.
    to.taint.clear();
    endorse(
        subject,
        subject_ref,
        &to,
        sealing_component,
        EndorsementBasis::Seal,
        basis_ref,
        ContentKind::Other,
    )
}

/// `declassify(subject, to, endorser, basis, basis_ref) → security.label.declassified`
/// (§8.1 #2): widens **readers only** — `authority` and `taint` are unchanged and the readers
/// set strictly widens. Only `policy_rule` declassifies (the kernel executing a
/// `definition`-issued `HarnessRule`).
pub fn declassify(
    subject: &ProvenanceRecord,
    subject_ref: impl Into<String>,
    to_readers: ReaderSet,
    endorser: &ProvenanceRecord,
    basis: EndorsementBasis,
    basis_ref: Option<String>,
) -> Result<LabelDeclassified, EndorsementError> {
    let from = subject.label();
    if basis != EndorsementBasis::PolicyRule {
        return Err(EndorsementError::BasisNotAllowed {
            basis,
            kind: "declassify (only policy_rule widens readers)",
        });
    }
    if !matches!(endorser.origin, Origin::Kernel { .. }) {
        return Err(EndorsementError::IllegitimateEndorsement {
            detail: "policy_rule declassification is executed by the kernel".to_string(),
        });
    }
    let mut to = from.clone();
    to.readers = to_readers;
    // Widens readers only: `authority`/`taint` are copied unchanged; the readers set must be
    // strictly wider (an equal-or-narrower "declassification" is refused).
    if !(to.readers.is_superset_of(&from.readers) && to.readers != from.readers) {
        return Err(EndorsementError::NoLabelIncrease);
    }
    Ok(LabelDeclassified {
        subject_ref: subject_ref.into(),
        from,
        to,
        endorser: endorser.clone(),
        basis,
        basis_ref,
    })
}

/// `security.label.applied` — the stamping event for a label application that is not an
/// increase (initial stamping on a ledger class).
pub fn apply_label(
    subject_ref: impl Into<String>,
    label: Label,
    applied_by: &ProvenanceRecord,
) -> LabelApplied {
    LabelApplied {
        subject_ref: subject_ref.into(),
        label,
        applied_by: applied_by.clone(),
    }
}

/// `shape_endorse(subject, subject_ref, schema, cap_max, validator,
/// basis_ref, robustness_inputs) → security.label.endorsed` — the
/// `validator` basis's C2 form (§5g.2 §2.2; the `shape_endorse` remedy's
/// consumption-side emitter): a deterministic kernel `Validator` raises an
/// `external` value to `environment` (`taint → ∅`) iff the declared output
/// `schema` is capacity-bounded with `capacity_bits ≤ cap_max` (the kernel
/// computes the capacity, never the validator — OQ-145's ratified default;
/// an unbounded string is `BasisNotAllowed`, AC-R-2.8.2-6c).
///
/// The emitted event carries `capacity_bits` and the `robustness_inputs`
/// (the parameter paths the endorsement discharges for D-ROBUST — the
/// payload extension §5g.2 §3 declares).
pub fn shape_endorse(
    subject: &ProvenanceRecord,
    subject_ref: impl Into<String>,
    schema: &Json,
    cap_max: u64,
    validator: &ProvenanceRecord,
    basis_ref: Option<String>,
    robustness_inputs: Vec<String>,
) -> Result<LabelEndorsed, EndorsementError> {
    let bits = match crate::flow::capacity_bits(schema) {
        Some(b) if b <= cap_max => b,
        Some(b) => {
            return Err(EndorsementError::ShapeCapacityExceeded {
                capacity_bits: b,
                cap_max,
            })
        }
        None => {
            return Err(EndorsementError::BasisNotAllowed {
                basis: EndorsementBasis::Validator,
                kind: "schema not capacity-bounded (free text never endorses)",
            })
        }
    };
    let mut to = subject.label();
    to.authority = AuthorityClass::Environment;
    to.taint.clear();
    let mut event = endorse(
        subject,
        subject_ref,
        &to,
        validator,
        EndorsementBasis::Validator,
        basis_ref,
        ContentKind::ClosedSchemaValue,
    )?;
    event.capacity_bits = Some(bits);
    event.robustness_inputs = robustness_inputs;
    Ok(event)
}

/// `sanitize_endorse(source, source_ref, subject_ref, bounds, realized,
/// sanitizer_ref, capability, endorser, scope, at) → (security.label.endorsed,
/// projected_record)` — the bounded sanitizer's `policy_rule (b)`
/// endorsement (§5g.2 §2.2; ADR-0055 D1; AC-R-2.8.2-7).
///
/// The sanitizer's output is a deterministic projection of the source — the
/// returned record is `derive(Projection, [source], kernel)` with
/// `derived_from` pointing at the source, its label then endorsed at
/// `bounds.to` *in the same append* (the lowering is the endorsement, never
/// the bare projection — P1 as amended). Refusals:
///
/// - `SanitizerBoundExceeded` — the *realized* output label violates the
///   declared bound (a removed taint pattern survived, `bounds.to` claims
///   an authority the output lacks, or `bounds.to.taint` adds tags) —
///   measured by [`crate::flow::apply_sanitizer_bounds`];
/// - `BasisNotAllowed`/`PolicyRuleTouchesNonReaders` (via
///   [`crate::flow::check_sanitizer_effect`]) — the endorsement delta would
///   touch authority, drop taint the bound never declared, or land off
///   `bounds.to`;
/// - `NoLabelIncrease` — nothing rises (a sanitizer that endorses nothing
///   emits nothing);
/// - `IllegitimateEndorsement` — `policy_rule` is executed by the kernel; a
///   non-kernel endorser never emits it.
#[allow(clippy::too_many_arguments)] // the emitter's operands are the append's legs — the arity is the event's.
pub fn sanitize_endorse(
    source: &ProvenanceRecord,
    source_ref: impl Into<String>,
    subject_ref: impl Into<String>,
    bounds: &crate::flow::SanitizerBounds,
    realized: &crate::label::Label,
    sanitizer_ref: &str,
    capability: &str,
    endorser: &ProvenanceRecord,
    scope: crate::authority::PersistenceScope,
    at: u64,
) -> Result<(LabelEndorsed, ProvenanceRecord), EndorsementError> {
    // The realized output must satisfy the declared bound.
    let to = crate::flow::apply_sanitizer_bounds(bounds, realized, capability).map_err(|e| {
        let detail = match &e {
            crate::flow::FlowError::SanitizerBoundExceeded { detail } => detail.clone(),
            other => format!("{other:?}"),
        };
        EndorsementError::SanitizerBoundExceeded { detail }
    })?;
    if !matches!(endorser.origin, Origin::Kernel { .. }) {
        return Err(EndorsementError::IllegitimateEndorsement {
            detail: "policy_rule endorsement is executed by the kernel".to_string(),
        });
    }
    if endorser.authority < to.authority {
        return Err(EndorsementError::EndorserBelowTarget {
            endorser: endorser.authority,
            target: to.authority,
        });
    }
    // The projection record — a deterministic kernel derivation with
    // `derived_from` → the source (its inherited label is the `from`).
    let source_ref = source_ref.into();
    let mut projected = crate::derive::derive(
        crate::record::DerivationKind::Projection,
        &[crate::derive::DerivationInput {
            input_ref: source_ref.clone(),
            record: source.clone(),
        }],
        endorser.origin.clone(),
        true,
        scope,
        at,
    )
    .map_err(|e| EndorsementError::IllegitimateEndorsement {
        detail: format!("sanitizer projection failed: {e:?}"),
    })?;
    let from = projected.label();
    // The endorsement delta must stay inside the declared bound.
    crate::flow::check_sanitizer_effect(bounds, &from, &to, capability).map_err(|e| {
        crate::flow::basis_effect_endorsement_error(EndorsementBasis::PolicyRule, &e)
    })?;
    match classify_label_delta(&from, &to) {
        LabelDelta::None | LabelDelta::Narrowing => {
            return Err(EndorsementError::NoLabelIncrease);
        }
        LabelDelta::Widening | LabelDelta::MixedWidening => {}
    }
    projected.authority = to.authority;
    projected.taint = to.taint.clone();
    projected.readers = to.readers.clone();
    let event = LabelEndorsed {
        subject_ref: subject_ref.into(),
        from,
        to,
        endorser: endorser.clone(),
        basis: EndorsementBasis::PolicyRule,
        basis_ref: Some(sanitizer_ref.to_string()),
        capacity_bits: None,
        sanitizer_ref: Some(sanitizer_ref.to_string()),
        robustness_inputs: Vec::new(),
    };
    Ok((event, projected))
}

/// Monitor check 5 — endorsement legitimacy **at append** (§8.1 #2 monitor table; ADR-0035
/// D5): re-runs the [`endorse`] rules over the event before it lands, so an illegitimate
/// `security.label.endorsed` never reaches the ledger. Returns the typed refusal.
pub fn check_endorsement(
    event: &LabelEndorsed,
    subject: &ProvenanceRecord,
    content_kind: ContentKind,
) -> Result<(), EndorsementError> {
    // Recompute: the recorded `from` must equal the subject's current label, and the event
    // must satisfy the full endorsement rules.
    if event.from != subject.label() {
        return Err(EndorsementError::IllegitimateEndorsement {
            detail: "recorded `from` does not equal the subject's current label".to_string(),
        });
    }
    endorse(
        subject,
        event.subject_ref.clone(),
        &event.to,
        &event.endorser,
        event.basis,
        event.basis_ref.clone(),
        content_kind,
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::PersistenceScope;
    use crate::origin::HumanRole;
    use crate::record::{Attestation, AttestationAnchor, AttestationKind};
    use std::collections::BTreeSet;

    fn human() -> ProvenanceRecord {
        ProvenanceRecord::minted(
            Origin::human("alice", HumanRole::Principal),
            PersistenceScope::User,
            0,
        )
    }
    fn kernel() -> ProvenanceRecord {
        ProvenanceRecord::kernel("kernel:monitor", 0)
    }
    fn model_content() -> ProvenanceRecord {
        ProvenanceRecord::minted(Origin::model("m1", "r1", "resp1"), PersistenceScope::Run, 0)
    }

    #[test]
    fn a_delegate_endorser_is_never_valid() {
        // Fault injection (§8.1 #8): a `delegate` endorser → IllegitimateEndorsement.
        let subject = model_content();
        let mut to = subject.label();
        to.authority = AuthorityClass::Principal;
        let endorser = model_content();
        assert!(matches!(
            endorse(
                &subject,
                "s",
                &to,
                &endorser,
                EndorsementBasis::Promotion,
                None,
                ContentKind::Other
            ),
            Err(EndorsementError::IllegitimateEndorsement { .. })
        ));
    }

    #[test]
    fn endorser_below_target_is_refused() {
        let subject = model_content(); // delegate
        let mut to = subject.label();
        to.authority = AuthorityClass::Definition;
        let endorser = human(); // principal < definition
        assert!(matches!(
            endorse(
                &subject,
                "s",
                &to,
                &endorser,
                EndorsementBasis::Promotion,
                None,
                ContentKind::Other
            ),
            Err(EndorsementError::EndorserBelowTarget { .. })
        ));
    }

    #[test]
    fn a_label_that_does_not_rise_is_not_an_endorsement() {
        let subject = model_content();
        let to = subject.label();
        assert!(matches!(
            endorse(
                &subject,
                "s",
                &to,
                &kernel(),
                EndorsementBasis::Promotion,
                None,
                ContentKind::Other
            ),
            Err(EndorsementError::NoLabelIncrease)
        ));
    }

    #[test]
    fn validator_on_free_text_is_basis_not_allowed() {
        // Fault injection (§8.1 #8): `validator` basis on a free-text leaf.
        let mut subject =
            ProvenanceRecord::minted(Origin::tool("t", "i"), PersistenceScope::Run, 0);
        subject.authority = AuthorityClass::External;
        let mut to = subject.label();
        to.authority = AuthorityClass::Environment;
        assert!(matches!(
            endorse(
                &subject,
                "s",
                &to,
                &kernel(),
                EndorsementBasis::Validator,
                None,
                ContentKind::FreeText
            ),
            Err(EndorsementError::BasisNotAllowed {
                basis: EndorsementBasis::Validator,
                ..
            })
        ));
        // Closed-schema external → environment by a kernel validator: legitimate.
        assert!(endorse(
            &subject,
            "s",
            &to,
            &kernel(),
            EndorsementBasis::Validator,
            None,
            ContentKind::ClosedSchemaValue
        )
        .is_ok());
    }

    #[test]
    fn seal_confers_definition_and_refuses_imported_subjects() {
        let subject = human(); // principal
        let ev = seal(&subject, "def:react", &kernel(), None).unwrap();
        assert_eq!(ev.to.authority, AuthorityClass::Definition);
        assert_eq!(ev.basis, EndorsementBasis::Seal);
        // Imported instructions/skills seal only with pin.
        let imported = ProvenanceRecord::minted(
            Origin::import("market", "v1"),
            PersistenceScope::Definition,
            0,
        );
        let mut to = imported.label();
        to.authority = AuthorityClass::Definition;
        assert!(matches!(
            endorse(
                &imported,
                "i",
                &to,
                &kernel(),
                EndorsementBasis::Seal,
                None,
                ContentKind::Other
            ),
            Err(EndorsementError::BasisNotAllowed {
                basis: EndorsementBasis::Seal,
                ..
            })
        ));
        // A tainted record can never validate above `external` (TaintedAboveExternal), so it
        // cannot legitimately hold the `definition` a seal would confer — the record-level
        // guard lives in `record::tests`.
    }

    #[test]
    fn approval_endorses_one_effect_intent_only() {
        let subject = model_content();
        let mut to = subject.label();
        to.authority = AuthorityClass::Delegate; // no rise → approval laundering guard
        assert!(matches!(
            endorse(
                &subject,
                "e",
                &to,
                &human(),
                EndorsementBasis::Approval,
                Some("perm:1".into()),
                ContentKind::EffectIntent
            ),
            Err(EndorsementError::NoLabelIncrease)
        ));
        // Approval on content (not an effect intent) is refused.
        to.authority = AuthorityClass::Principal;
        assert!(matches!(
            endorse(
                &subject,
                "e",
                &to,
                &human(),
                EndorsementBasis::Approval,
                Some("perm:1".into()),
                ContentKind::FreeText
            ),
            Err(EndorsementError::BasisNotAllowed {
                basis: EndorsementBasis::Approval,
                ..
            })
        ));
        // Approval without a permission_id basis_ref is refused.
        assert!(matches!(
            endorse(
                &subject,
                "e",
                &to,
                &human(),
                EndorsementBasis::Approval,
                None,
                ContentKind::EffectIntent
            ),
            Err(EndorsementError::BasisNotAllowed { .. })
        ));
        // A legitimate approval on an effect intent, human endorser ≥ target.
        let mut to_eff = subject.label();
        to_eff.readers = ReaderSet::Restricted(BTreeSet::from(["alice".to_string()]));
        // Narrow readers is not a rise — rise authority instead is refused by <principal?
        // human = principal ≥ delegate already; make the intent's readers the rise? readers
        // *narrowing* is not a rise. Give the intent a taint-drop? Also not a rise.
        // Rise: authority delegate → principal on the EFFECT INTENT (allowed for approval).
        to_eff.readers = ReaderSet::Public;
        to_eff.authority = AuthorityClass::Principal;
        let ev = endorse(
            &subject,
            "effect:1",
            &to_eff,
            &human(),
            EndorsementBasis::Approval,
            Some("perm:1".into()),
            ContentKind::EffectIntent,
        )
        .unwrap();
        assert_eq!(ev.basis_ref.as_deref(), Some("perm:1"));
    }

    #[test]
    fn policy_rule_never_endorses_but_declassifies_readers() {
        let mut subject = human();
        subject.readers = ReaderSet::Restricted(BTreeSet::from(["alice".to_string()]));
        // In endorse: refused.
        let mut to = subject.label();
        to.authority = AuthorityClass::Principal.max(AuthorityClass::Definition);
        assert!(matches!(
            endorse(
                &subject,
                "s",
                &to,
                &kernel(),
                EndorsementBasis::PolicyRule,
                None,
                ContentKind::Other
            ),
            Err(EndorsementError::BasisNotAllowed {
                basis: EndorsementBasis::PolicyRule,
                ..
            })
        ));
        // In declassify: readers widen only, by the kernel executing a rule.
        let wider = ReaderSet::Restricted(BTreeSet::from(["alice".to_string(), "bob".to_string()]));
        let ev = declassify(
            &subject,
            "s",
            wider,
            &kernel(),
            EndorsementBasis::PolicyRule,
            Some("rule:7".into()),
        )
        .unwrap();
        assert_eq!(ev.to.authority, ev.from.authority);
        assert_eq!(ev.to.taint, ev.from.taint);
        // A narrowing "declassification" is refused.
        let narrower = ReaderSet::Restricted(BTreeSet::from(["alice".to_string()]));
        assert!(declassify(
            &subject,
            "s",
            narrower,
            &kernel(),
            EndorsementBasis::PolicyRule,
            None
        )
        .is_err());
        // A non-kernel declassifier is refused.
        assert!(declassify(
            &subject,
            "s",
            ReaderSet::Public,
            &human(),
            EndorsementBasis::PolicyRule,
            None
        )
        .is_err());
    }

    #[test]
    fn pin_requires_a_verified_attestation() {
        let subject = ProvenanceRecord::minted(
            Origin::import("vendor", "v2"),
            PersistenceScope::Definition,
            0,
        ); // unverified
        let mut to = subject.label();
        to.authority = AuthorityClass::Principal;
        // No attestation → refused.
        assert!(matches!(
            endorse(
                &subject,
                "s",
                &to,
                &kernel(),
                EndorsementBasis::Pin,
                None,
                ContentKind::Other
            ),
            Err(EndorsementError::PinRequiresVerifiedAttestation)
        ));
        // With a signature attestation → allowed (target principal ≤ kernel endorser).
        let mut pinned = subject.clone();
        pinned.attestation = Some(Attestation {
            kind: AttestationKind::Signature,
            subject_hash: "sha256:s".into(),
            anchor: AttestationAnchor::Signer("root:vendor".into()),
            verified_by: "kernel:trust".into(),
            verified_at: 3,
        });
        assert!(endorse(
            &pinned,
            "s",
            &to,
            &kernel(),
            EndorsementBasis::Pin,
            Some("pin:1".into()),
            ContentKind::Other
        )
        .is_ok());
    }

    #[test]
    fn check5_revalidates_at_append() {
        let subject = human();
        let ev = seal(&subject, "d", &kernel(), None).unwrap();
        // Consistent append passes.
        assert!(check_endorsement(&ev, &subject, ContentKind::Other).is_ok());
        // A forged event whose `from` doesn't match the subject's real label is refused.
        let mut forged = ev.clone();
        forged.from.authority = AuthorityClass::Unverified;
        assert!(check_endorsement(&forged, &subject, ContentKind::Other).is_err());
    }
}
