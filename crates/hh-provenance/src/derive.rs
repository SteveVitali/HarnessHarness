//! P1 — `derive` (§8.1 #2; ADR-0034 P1 as amended ADR-0055/0076; CF-124/164): the **only**
//! derivation path, and P2's result-ingestion half ([`ingest_child_result`]).
//!
//! `label(out) = ⊔ label(inputs) ⊔ label(deriver)` — join takes the **minimum** authority, the
//! union taint and the intersection readers, so a derivation can never exceed its least
//! authoritative input (the laundering failure mode: a summary of `external` content stays
//! `≤ external` and carries the union taint). A sanitizer's lowering is never the projection
//! itself — it appears only as a `policy_rule` endorsement appended *with* the projection
//! (see [`crate::endorse`]).

use crate::authority::{AuthorityClass, PersistenceScope};
use crate::label::Label;
use crate::origin::Origin;
use crate::record::{Derivation, DerivationKind, ProvenanceError, ProvenanceRecord};

/// One derivation input: the input's identity coordinate and its provenance record.
/// `derived_from` lists the ids — for compaction, the forgotten ids (§8.1 #3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationInput {
    /// The input's identity coordinate (`version_id` / content address).
    pub input_ref: String,
    /// The input's provenance record.
    pub record: ProvenanceRecord,
}

/// `derive(kind, inputs, deriver, deterministic) → ProvenanceRecord` (P1; §8.1 #2).
///
/// - `label(out) = ⊔ label(inputs) ⊔ label(deriver)`;
/// - `deriver = delegate` for any model-produced kind (summary/compaction/extraction/
///   translation): the result is capped at `min(⊔ inputs, delegate)`;
/// - `kernel` is the deriver for deterministic projections (truncation, redaction, structured
///   projection) — and a `deterministic` derivation requires a kernel deriver;
/// - every child result enters the parent at `≤ delegate` (`kind = subagent_result`),
///   carrying the union taint;
/// - `derived_from` lists the inputs.
///
/// `MissingProvenance` when `inputs` is empty (a derivation with no provenance inputs cannot
/// produce a provenance-bearing output).
pub fn derive(
    kind: DerivationKind,
    inputs: &[DerivationInput],
    deriver: Origin,
    deterministic: bool,
    scope: PersistenceScope,
    created_at: u64,
) -> Result<ProvenanceRecord, ProvenanceError> {
    if inputs.is_empty() {
        return Err(ProvenanceError::MissingProvenance {
            what: format!("derive({}) with no input records", kind.as_str()),
        });
    }
    if deterministic && !matches!(deriver, Origin::Kernel { .. }) {
        // A deterministic projection is a kernel derivation — no other deriver may claim it.
        return Err(ProvenanceError::AttestationFailed {
            reason: "deterministic projection requires a kernel deriver".to_string(),
        });
    }
    let deriver_record = ProvenanceRecord::minted(deriver.clone(), scope, created_at);
    let mut label = inputs
        .iter()
        .fold(Label::top(), |acc, i| acc.join(&i.record.label()))
        .join(&deriver_record.label());
    // Model-produced kinds and subagent results are capped at `delegate` (P1/P2).
    if kind.is_model_produced() || kind == DerivationKind::SubagentResult {
        label.authority = label.authority.min(AuthorityClass::Delegate);
    }
    Ok(ProvenanceRecord {
        origin: deriver,
        authority: label.authority,
        taint: label.taint,
        readers: label.readers,
        scope,
        derived_from: vec![Derivation {
            kind,
            inputs: inputs.iter().map(|i| i.input_ref.clone()).collect(),
            deriver: deriver_record.origin.clone(),
            deterministic,
        }],
        created_at,
        attestation: None,
    })
}

/// P2 result ingestion (§8.1 #2 `attenuate_delegation` second half): every child result enters
/// the parent at `≤ delegate`, `taint ⊇` the child's accumulated taint, `derived_from.kind =
/// subagent_result`. The child's own taint and readers survive the join unchanged-or-stricter;
/// the cap is on *authority* only.
pub fn ingest_child_result(
    child_ref: impl Into<String>,
    child: &ProvenanceRecord,
    parent_run_scope: PersistenceScope,
    created_at: u64,
) -> ProvenanceRecord {
    let input = DerivationInput {
        input_ref: child_ref.into(),
        record: child.clone(),
    };
    // The parent-side AgentProcess that ingests the result is a delegate-class deriver; the
    // kind performs the ≤-delegate cap regardless.
    let deriver = Origin::kernel("kernel:spawn");
    derive(
        DerivationKind::SubagentResult,
        &[input],
        deriver,
        false,
        parent_run_scope,
        created_at,
    )
    .expect("subagent-result ingestion always has one input")
}

/// Monitor check 7 — **no delegate provenance above `delegate` after a delegation**
/// (§8.1 #4 monitor table; ADR-0034 P2). After a delegation boundary the parent's record must
/// satisfy `authority ≤ delegate` and `taint ⊇` every child's accumulated taint. A violation
/// is a typed `AuthorityExceedsOrigin`/`AttestationFailed` refusal — never a warning.
pub fn check_delegate_attenuation(
    parent: &ProvenanceRecord,
    children: &[ProvenanceRecord],
) -> Result<(), ProvenanceError> {
    if parent.authority > AuthorityClass::Delegate {
        return Err(ProvenanceError::AuthorityExceedsOrigin {
            ceiling: AuthorityClass::Delegate,
            claimed: parent.authority,
        });
    }
    for child in children {
        if !child.taint.is_subset(&parent.taint) {
            return Err(ProvenanceError::AttestationFailed {
                reason: "check 7: parent taint does not cover a child's accumulated taint"
                    .to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::origin::HumanRole;

    fn rec(origin: Origin, taint: &[&str]) -> ProvenanceRecord {
        let mut r = ProvenanceRecord::minted(origin, PersistenceScope::Run, 0);
        for t in taint {
            r.taint.insert(crate::authority::TaintTag::Import {
                source_system: t.to_string(),
            });
            // Taint forces authority ≤ external (TaintedAboveExternal) — keep records valid.
            r.authority = r.authority.min(AuthorityClass::External);
        }
        r
    }

    fn input(id: &str, r: ProvenanceRecord) -> DerivationInput {
        DerivationInput {
            input_ref: id.to_string(),
            record: r,
        }
    }

    #[test]
    fn derive_is_the_join_of_inputs_and_deriver() {
        // A human doc and an external tool result summarized by a model:
        // authority = min(principal, external, delegate) = external; taint unions.
        let human_doc = rec(Origin::human("alice", HumanRole::Principal), &[]);
        let tool_out = rec(Origin::tool("tool:shell", "inv:1"), &["web"]);
        let out = derive(
            DerivationKind::Summary,
            &[
                input("sha256:doc", human_doc),
                input("sha256:tool", tool_out),
            ],
            Origin::model("m", "r", "resp"),
            false,
            PersistenceScope::Run,
            10,
        )
        .unwrap();
        assert_eq!(out.authority, AuthorityClass::External);
        assert_eq!(out.taint.len(), 1);
        assert_eq!(out.derived_from[0].kind, DerivationKind::Summary);
        assert_eq!(
            out.derived_from[0].inputs,
            vec!["sha256:doc".to_string(), "sha256:tool".to_string()]
        );
    }

    #[test]
    fn model_summary_of_kernel_content_is_capped_at_delegate() {
        // P1: a model summary of kernel facts derives at min(⊔ inputs, delegate) — a model
        // summary never launders into kernel authority.
        let kernel_fact = rec(Origin::kernel("ledger"), &[]);
        let out = derive(
            DerivationKind::Summary,
            &[input("sha256:k", kernel_fact)],
            Origin::model("m", "r", "resp"),
            false,
            PersistenceScope::Run,
            1,
        )
        .unwrap();
        assert_eq!(out.authority, AuthorityClass::Delegate);
    }

    #[test]
    fn deterministic_projection_is_a_kernel_derivation() {
        let ext = rec(Origin::tool("t", "i"), &[]);
        // A deterministic projection requires a kernel deriver.
        assert!(derive(
            DerivationKind::Projection,
            &[input("x", ext.clone())],
            Origin::model("m", "r", "x"),
            true,
            PersistenceScope::Run,
            0,
        )
        .is_err());
        // With a kernel deriver a projection preserves the input's floor (join min).
        let out = derive(
            DerivationKind::Projection,
            &[input("x", ext)],
            Origin::kernel("kernel:redactor"),
            true,
            PersistenceScope::Run,
            0,
        )
        .unwrap();
        assert_eq!(out.authority, AuthorityClass::External);
    }

    #[test]
    fn derive_requires_at_least_one_input() {
        assert!(matches!(
            derive(
                DerivationKind::Summary,
                &[],
                Origin::model("m", "r", "x"),
                false,
                PersistenceScope::Run,
                0
            ),
            Err(ProvenanceError::MissingProvenance { .. })
        ));
    }

    #[test]
    fn child_results_enter_at_delegate_with_union_taint() {
        // P2: ingestion caps at delegate and keeps the child's accumulated taint.
        let child = rec(
            Origin::participant("child:sub1", "hh.hosting/1"),
            &["childtaint"],
        );
        let ingested = ingest_child_result("sha256:child", &child, PersistenceScope::Run, 9);
        assert!(ingested.authority <= AuthorityClass::Delegate);
        assert!(ingested
            .taint
            .iter()
            .any(|t| t.as_string().contains("childtaint")));
        assert_eq!(
            ingested.derived_from[0].kind,
            DerivationKind::SubagentResult
        );
    }
}
