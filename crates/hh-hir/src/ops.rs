//! The §3.1.7 operation surface over canonical records — `canonicalize`, `seal`,
//! `migrate`, `project` — plus the canonical-bytes seam every op exposes for
//! out-of-process use (AC-IR-10; the `hh-ir-op` binary drives it).
//!
//! `seal` (§3.1.6) is where definition authority is *conferred*: every member gets a
//! `seal`-basis endorsement ([`hh_provenance::endorse::seal`]) raising it to `definition`
//! with a `Seal` attestation bound to the pre-seal identity basis — the record carries its
//! own evidence so `validate` accepts the raised class without a ledger (§8.1 #3). Members
//! whose origin is `import`/`migration` may not rise on the seal basis — they need a `pin`
//! attestation already on the record ("imported instructions/skills may be sealed only with
//! pin"). `seal` also computes the **closed-world tool set** — the `MintingContext` through
//! which tool-origin records mint `environment` (DF-S1.3-3) — and pins every `Ref` to the
//! sealed version ids (ref graph topological order; a ref cycle is `CycleDetected`).

use std::collections::{BTreeMap, BTreeSet};

use hh_ontology::planes::Plane;
use hh_provenance::{
    Attestation, AttestationAnchor, AttestationKind, AuthorityClass, PersistenceScope,
    ProvenanceRecord,
};

use crate::document::{DefinitionVersionRef, Edge, HirDocument, Node, SealedDefinition};
use crate::errors::{HirError, ValidationReport};
use crate::kinds::{EntityKind, ToolEffects, ValidatorExecutable, ValidatorKind};
use crate::leaves::{CompiledPayload, Text};
use crate::records::*;
use crate::refs::{Ref, RefVersion};
use crate::validate;

/// `canonicalize(doc) → canonical bytes` (§3.1.7) — the sorted-key compact form with
/// order-normalized `nodes`/`edges`. Pure; computes nothing.
pub fn canonicalize(doc: &HirDocument) -> Vec<u8> {
    doc.canonical_bytes()
}

/// `validate` — the §3.1.4 battery ([`crate::validate::validate`]).
pub fn validate_doc(doc: &HirDocument) -> Result<ValidationReport, Vec<HirError>> {
    validate::validate(doc)
}

// ─────────────────────────────────────────────────────────────────────────────
// seal
// ─────────────────────────────────────────────────────────────────────────────

/// The kernel provenance `seal` endorses under (`Origin::kernel("hir:seal")` — the kernel
/// seals a definition on the harness author's behalf; §8.1 #9).
fn sealing_component(sealed_at: u64) -> ProvenanceRecord {
    ProvenanceRecord::kernel("hir:seal", sealed_at)
}

/// Run the seal-basis endorsement on one provenance record. On success the record carries
/// `authority = definition` and a `Seal` attestation whose `subject_hash` is the member's
/// pre-seal identity basis — the attestation is *inside* the record, so it cannot bind the
/// post-endorsement version id (it is part of it); binding the pre-endorsement basis is the
/// honest subject (the content the endorsement raised).
fn endorse_member(
    prov: &mut ProvenanceRecord,
    subject_hash: &str,
    what: &str,
    sealed_at: u64,
) -> Result<(), HirError> {
    if prov.scope != PersistenceScope::Definition || prov.authority >= AuthorityClass::Definition {
        return Ok(());
    }
    if !prov.taint.is_empty() {
        return Err(HirError::TaintedAboveExternal {
            authority: format!("{what}: {}", prov.authority.as_str()),
        });
    }
    let kernel = sealing_component(sealed_at);
    match hh_provenance::seal(prov, subject_hash, &kernel, None) {
        Ok(_) => {
            prov.authority = AuthorityClass::Definition;
            prov.attestation = Some(Attestation {
                kind: AttestationKind::Seal,
                subject_hash: subject_hash.to_string(),
                anchor: AttestationAnchor::Chain("hir:seal".into()),
                verified_by: "hir:seal".into(),
                verified_at: sealed_at,
            });
            Ok(())
        }
        Err(hh_provenance::EndorsementError::BasisNotAllowed { .. }) => {
            // import/migration content may not seal on the seal basis — it needs a `pin`
            // attestation already on the record (§8.1 #3).
            let pinned = prov
                .attestation
                .as_ref()
                .is_some_and(|a| a.self_consistent() && a.kind == AttestationKind::Pin);
            if pinned {
                prov.authority = AuthorityClass::Definition;
                Ok(())
            } else {
                Err(HirError::IllegitimateEndorsement {
                    detail: format!(
                        "{what}: import/migration member requires a pin endorsement before seal"
                    ),
                })
            }
        }
        Err(e) => Err(HirError::IllegitimateEndorsement {
            detail: format!("{what}: seal endorsement refused: {e:?}"),
        }),
    }
}

/// Endorse every provenance record embedded in a node — the node's own, plus each
/// `Text`/`CompiledPayload` leaf's (the leaves are members of the sealed definition).
fn endorse_node(node: &mut Node, sealed_at: u64) -> Result<(), HirError> {
    let basis = crate::identity::version_id(node);
    endorse_member(&mut node.provenance, &basis, "node", sealed_at)?;
    endorse_leaves(&mut node.semantic, sealed_at)?;
    if let Some(s) = &mut node.surface {
        endorse_surface(s, sealed_at)?;
    }
    Ok(())
}

fn endorse_text(t: &mut Text, what: &str, sealed_at: u64) -> Result<(), HirError> {
    let basis = t.content_hash.clone();
    endorse_member(&mut t.provenance, &basis, what, sealed_at)?;
    // The leaf's own authority field rises with its provenance (they mint together —
    // `Text::new` sets `authority = provenance.authority`).
    if t.provenance.authority == AuthorityClass::Definition
        && t.authority < AuthorityClass::Definition
    {
        t.authority = AuthorityClass::Definition;
    }
    Ok(())
}

fn endorse_payload(p: &mut CompiledPayload, what: &str, sealed_at: u64) -> Result<(), HirError> {
    let basis = p.bytes_hash.clone();
    endorse_member(&mut p.provenance, &basis, what, sealed_at)
}

fn endorse_steps(steps: &mut [ProcedureStep], sealed_at: u64) -> Result<(), HirError> {
    for s in steps {
        match s {
            ProcedureStep::Instruction(t) => endorse_text(t, "instruction", sealed_at)?,
            ProcedureStep::Opaque(p) => endorse_payload(p, "opaque step", sealed_at)?,
            ProcedureStep::Branch {
                then_body,
                else_body,
                ..
            } => {
                endorse_steps(then_body, sealed_at)?;
                endorse_steps(else_body, sealed_at)?;
            }
            ProcedureStep::Loop { body, .. } => endorse_steps(body, sealed_at)?,
            _ => {}
        }
    }
    Ok(())
}

fn endorse_leaves(rec: &mut KindRecord, sealed_at: u64) -> Result<(), HirError> {
    match rec {
        KindRecord::Goal(g) => {
            endorse_text(&mut g.statement, "goal.statement", sealed_at)?;
            if let Some(r) = &mut g.unverifiable_reason {
                endorse_text(r, "goal.unverifiable_reason", sealed_at)?;
            }
        }
        KindRecord::Observation(o) => {
            if let ObservationContent::Text(t) = &mut o.content {
                endorse_text(t, "observation.content", sealed_at)?;
            }
        }
        KindRecord::Memory(m) => endorse_text(&mut m.content, "memory.content", sealed_at)?,
        KindRecord::Procedure(p) => endorse_steps(&mut p.steps, sealed_at)?,
        KindRecord::ToolCapability(t) => endorse_text(&mut t.purpose, "tool.purpose", sealed_at)?,
        KindRecord::Validator(v) => {
            if let ValidatorKind::Judge(j) = &mut v.kind {
                endorse_text(&mut j.rubric, "validator.judge.rubric", sealed_at)?;
            }
            if let ValidatorKind::Executable(ValidatorExecutable::Payload(p)) = &mut v.kind {
                endorse_payload(p, "validator.executable", sealed_at)?;
            }
            if let Some(d) = &mut v.assumption_debt {
                endorse_text(&mut d.hypothesis, "validator.debt.hypothesis", sealed_at)?;
            }
        }
        KindRecord::HarnessRule(r) => {
            if let Some(d) = &mut r.assumption_debt {
                endorse_text(&mut d.hypothesis, "rule.debt.hypothesis", sealed_at)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn endorse_surface(s: &mut SurfaceRecord, sealed_at: u64) -> Result<(), HirError> {
    if let SurfaceRecord::Tool(t) = s {
        endorse_text(
            &mut t.description_template,
            "surface.description_template",
            sealed_at,
        )?;
    }
    Ok(())
}

/// Every `Ref` field in a record (with the paths `seal` rewrites) — mutable access for the
/// pinning pass.
fn each_ref_mut(rec: &mut KindRecord, f: &mut dyn FnMut(&mut Ref)) {
    match rec {
        KindRecord::Goal(g) => {
            g.success_criteria.iter_mut().for_each(&mut *f);
            f(&mut g.budget);
            if let Some(p) = &mut g.parent {
                f(p);
            }
        }
        KindRecord::Observation(o) => {
            if let ObservationContent::Artifact(r) = &mut o.content {
                f(r);
            }
        }
        KindRecord::ContextItem(c) => {
            f(&mut c.payload);
            if let Some(p) = &mut c.placement_policy {
                f(p);
            }
        }
        KindRecord::Procedure(p) => {
            p.allowed_capabilities.iter_mut().for_each(&mut *f);
            each_step_ref_mut(&mut p.steps, f);
        }
        KindRecord::ToolCapability(t) => {
            t.postconditions.iter_mut().for_each(&mut *f);
            if let ToolEffects::Declared(effects) = &mut t.effects {
                // BTreeSet has no iter_mut — rebuild the set with refs rewritten.
                let mut rebuilt = BTreeSet::new();
                for mut e in std::mem::take(effects) {
                    each_effect_ref_mut(&mut e, f);
                    rebuilt.insert(e);
                }
                *effects = rebuilt;
            }
        }
        KindRecord::Permission(p) => f(&mut p.holder),
        KindRecord::Effect(e) => each_effect_ref_mut(&mut e.declared, f),
        KindRecord::Artifact(a) => f(&mut a.produced_by),
        KindRecord::Validator(v) => {
            v.inputs.iter_mut().for_each(&mut *f);
            if let ValidatorKind::Executable(ValidatorExecutable::Invoke(r)) = &mut v.kind {
                f(r);
            }
        }
        KindRecord::AgentProcess(a) => match &mut a.body {
            AgentProcessBody::Native(n) => {
                f(&mut n.harness_def);
                f(&mut n.budget);
                f(&mut n.permissions);
            }
            AgentProcessBody::Hosted(h) => {
                h.supplies.context.iter_mut().for_each(&mut *f);
                h.supplies.tools.iter_mut().for_each(&mut *f);
                h.supplies.procedures.iter_mut().for_each(&mut *f);
                f(&mut h.budget);
                f(&mut h.permissions);
            }
        },
        KindRecord::Budget(b) => {
            if let Some(p) = &mut b.parent {
                f(p);
            }
            f(&mut b.accounting);
        }
        KindRecord::HarnessRule(r) => match &mut r.action {
            RuleAction::InsertContextItem(r) => f(r),
            RuleAction::RestrictToolSet(rs) => rs.iter_mut().for_each(&mut *f),
            RuleAction::RequireValidator(r) => f(r),
            _ => {}
        },
        _ => {}
    }
}

fn each_step_ref_mut(steps: &mut [ProcedureStep], f: &mut dyn FnMut(&mut Ref)) {
    for s in steps {
        match s {
            ProcedureStep::Invoke { tool, .. } => f(tool),
            ProcedureStep::Delegate {
                budget, permission, ..
            } => {
                f(budget);
                f(permission);
            }
            ProcedureStep::Branch {
                then_body,
                else_body,
                ..
            } => {
                each_step_ref_mut(then_body, f);
                each_step_ref_mut(else_body, f);
            }
            ProcedureStep::Loop { bound, body } => {
                f(bound);
                each_step_ref_mut(body, f);
            }
            ProcedureStep::Verify { validator } => f(validator),
            ProcedureStep::Opaque(p) => {
                if let Some(i) = &mut p.declared_interface {
                    let mut rebuilt = BTreeSet::new();
                    for mut e in std::mem::take(&mut i.effects) {
                        each_effect_ref_mut(&mut e, f);
                        rebuilt.insert(e);
                    }
                    i.effects = rebuilt;
                }
            }
            ProcedureStep::Instruction(_) => {}
        }
    }
}

fn each_effect_ref_mut(e: &mut crate::kinds::EffectClass, f: &mut dyn FnMut(&mut Ref)) {
    if let Some(a) = &mut e.attributes {
        if let crate::kinds::Reversibility::Reversible(r) = &mut a.reversibility {
            f(r);
        }
    }
}

fn each_edge_ref_mut(e: &mut EdgeRecord, f: &mut dyn FnMut(&mut Ref)) {
    match e {
        EdgeRecord::DelegatedTo { permission, budget } => {
            f(permission);
            f(budget);
        }
        EdgeRecord::Authorizes {
            effect_class: Some(ec),
            ..
        } => each_effect_ref_mut(ec, f),
        _ => {}
    }
}

/// `seal(doc, sealed_at) → SealedDefinition` (§3.1.6/§3.1.7). `UnresolvedRef` when a ref's
/// `semantic_id` doesn't resolve, or a document carrying an **unresolved** assembly section
/// (§3.1.3: only resolved documents are sealable — a resolved section carries no
/// `version_selector` and no `$param:`/`$entity:` binding form anywhere; the §3.3 `resolve`
/// produces it, S1.9); every validation error collected.
pub fn seal(doc: &HirDocument, sealed_at: u64) -> Result<SealedDefinition, Vec<HirError>> {
    let mut errs = Vec::new();
    if let Some(a) = &doc.assembly {
        if let Some(what) = unresolved_in_assembly(a, "assembly") {
            errs.push(HirError::UnresolvedRef {
                detail: format!(
                    "a document carrying an unresolved assembly section is not sealable (resolve first): {what}"
                ),
            });
            return Err(errs);
        }
        // §5g.5 L4 (S1.23): a sealed form carries only pinned extension refs —
        // a surviving `locator.selector` or a missing `resolved`/`fetched_at`/
        // `content` pin fails `UnpinnedInSealedForm`.
        if let Some(what) = unpinned_extension_in_assembly(a, "assembly") {
            errs.push(HirError::UnpinnedInSealedForm { detail: what });
            return Err(errs);
        }
    }
    validate::validate(doc)?;

    // §5g.2 (R-2.8.2; AC-R-2.8.2-1): a gated `ToolCapability` — `world = open`
    // on any declared effect, or a domain in `{net_egress, message_human,
    // fs_read, memory_write}` — may not seal without a `flow_contract`. The
    // contract's well-formedness is `validate_capability`'s check; this gate
    // is the *absence* refusal, collected with every other violation.
    for n in &doc.nodes {
        if let KindRecord::ToolCapability(rec) = &n.semantic {
            if crate::tools::capability_needs_flow_contract(rec) && rec.flow_contract.is_none() {
                errs.push(HirError::ContributionUndeclared {
                    capability: n.semantic_id(),
                });
            }
        }
    }
    if !errs.is_empty() {
        return Err(errs);
    }

    let mut sealed = doc.clone();

    // Semantic ids are stable under sealing (provenance/version/ext are excluded). A member
    // carrying an authored `semantic_id` keeps it — it is the key refs already resolve
    // against; a member without one gets the computed id.
    let mut sids: BTreeMap<String, usize> = BTreeMap::new();
    for (i, n) in sealed.nodes.iter_mut().enumerate() {
        let sid = n.semantic_id();
        n.version.semantic_id = Some(sid.clone());
        sids.insert(sid, i);
    }
    for e in sealed.edges.iter_mut() {
        e.version.semantic_id = Some(crate::identity::edge_semantic_id(e));
    }

    // Endorse every member (definition scope, below definition authority).
    for n in sealed.nodes.iter_mut() {
        if let Err(e) = endorse_node(n, sealed_at) {
            errs.push(e);
        }
        n.version.sealed = true;
    }
    for e in sealed.edges.iter_mut() {
        let basis = crate::identity::edge_version_id(e);
        if let Err(err) = endorse_member(&mut e.provenance, &basis, "edge", sealed_at) {
            errs.push(err);
        }
        if let EdgeRecord::DerivedFrom { hypothesis, .. } = &mut e.fields {
            let hb = hypothesis.content_hash.clone();
            if let Err(err) =
                endorse_member(&mut hypothesis.provenance, &hb, "derived-from", sealed_at)
            {
                errs.push(err);
            }
        }
        e.version.sealed = true;
    }
    if !errs.is_empty() {
        return Err(errs);
    }

    // Pin every ref. Member `version_id`s are computed **before** pinning — a member's
    // version identity is the identity of its sealed content as authored; the pin is the
    // definition's binding of a `Ref` to a member identity, not part of the member's own
    // content (the record-ref graph is not a DAG — `Permission.holder ↔
    // AgentProcess.permissions` is cyclic by construction, CF-076). A ref whose
    // `semantic_id` names no member is `UnresolvedRef`; the pin target is the member's
    // sealed `version_id`.
    let mut pinned_vid: Vec<String> = vec![String::new(); sealed.nodes.len()];
    for (i, n) in sealed.nodes.iter_mut().enumerate() {
        n.version.version_id = Some(crate::identity::version_id(n));
        pinned_vid[i] = n.version.version_id.clone().unwrap();
    }
    for n in sealed.nodes.iter_mut() {
        let mut pin_err: Option<HirError> = None;
        each_ref_mut(&mut n.semantic, &mut |r: &mut Ref| {
            if pin_err.is_some() {
                return;
            }
            match sids.get(&r.semantic_id) {
                None => {
                    pin_err = Some(HirError::UnresolvedRef {
                        detail: format!("seal: ref {} unresolved", r.semantic_id),
                    });
                }
                Some(&t) => {
                    r.version = RefVersion::Pinned(pinned_vid[t].clone());
                }
            }
        });
        if let Some(e) = pin_err {
            errs.push(e);
        }
    }
    // Profile pins: a sealed form's `ProfileRef`s must be pinned (N5).
    for n in sealed.nodes.iter_mut() {
        check_profile_pins(&n.semantic, &mut errs);
    }

    // Edges: pin field refs, then compute ids.
    for e in sealed.edges.iter_mut() {
        let mut pin_err: Option<HirError> = None;
        each_edge_ref_mut(&mut e.fields, &mut |r: &mut Ref| {
            if pin_err.is_some() {
                return;
            }
            match sids.get(&r.semantic_id) {
                None => {
                    pin_err = Some(HirError::UnresolvedRef {
                        detail: format!("seal: edge ref {} unresolved", r.semantic_id),
                    });
                }
                Some(&t) => {
                    r.version = RefVersion::Pinned(pinned_vid[t].clone());
                }
            }
        });
        if let Some(e) = pin_err {
            errs.push(e);
        }
        e.version.semantic_id = Some(crate::identity::edge_semantic_id(e));
        e.version.version_id = Some(crate::identity::edge_version_id(e));
    }
    // Root pin.
    match sids.get(&sealed.root.semantic_id) {
        None => errs.push(HirError::UnresolvedRef {
            detail: "seal: root unresolved".into(),
        }),
        Some(&t) => {
            sealed.root.version = RefVersion::Pinned(pinned_vid[t].clone());
        }
    }
    // `OpaqueProcess.version_identity = H(declaration ∥ participant_version)` — computed at
    // seal (a claim the definition pins).
    for n in sealed.nodes.iter_mut() {
        if let KindRecord::AgentProcess(a) = &mut n.semantic {
            if let AgentProcessBody::Hosted(h) = &mut a.body {
                h.version_identity = Some(crate::identity::opaque_version_identity(
                    &h.declared_capabilities,
                    &h.participant_version,
                ));
            }
        }
    }
    if !errs.is_empty() {
        return Err(errs);
    }

    // The sealed doc must still validate (sealed members at definition scope now carry the
    // seal-conferred authority — the scope ceiling holds by construction).
    validate::validate(&sealed)?;

    // The closed-world tool set — the minting context `environment` authority confers
    // through (DF-S1.3-3): `pure` capabilities, or every declared effect `world = closed`.
    let closed_world_tools = closed_world_tools(&sealed);

    let root_node = sealed
        .node(&sealed.root.semantic_id)
        .expect("root resolved above");
    Ok(SealedDefinition {
        definition_ref: DefinitionVersionRef {
            semantic_id: root_node.semantic_id(),
            version_id: root_node.version_id(),
        },
        document: sealed,
        closed_world_tools,
    })
}

/// The closed-world tool set of a sealed document — the minting context `environment`
/// authority confers through (DF-S1.3-3): `pure` capabilities, or capabilities whose
/// every declared effect is `world = closed`. The single rule home (CC1) — `seal` and the
/// compiler's `accept_bytes` (§3.2 stage 0) share it.
pub fn closed_world_tools(
    doc: &crate::document::HirDocument,
) -> std::collections::BTreeSet<String> {
    doc.nodes
        .iter()
        .filter_map(|n| match &n.semantic {
            KindRecord::ToolCapability(t) => match &t.effects {
                ToolEffects::Pure => Some(n.semantic_id()),
                ToolEffects::Declared(set)
                    if !set.is_empty() && set.iter().all(|e| e.is_closed_world()) =>
                {
                    Some(n.semantic_id())
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The opacity report over a document's leaves — `opacity(sealed, profile)` (§3.1.7;
/// T-02). The single producer (CC1; CF-050): `validate_assembly` stage 7-O and the
/// bundle's `opacity_ratio` both consume this, never recompute.
///
/// `profile` is the coordinate the report was computed under — informational only; which
/// leaves are *visible* to a profile is decided at `select_surfaces` (§5d), never here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirOpacity {
    /// Per-authority-class leaf counts (`Text` + `CompiledPayload` leaves, semantic and
    /// surface records alike).
    pub by_class: hh_provenance::OpacityReport,
    /// Leaves whose body the compiler cannot inspect — `CompiledPayload` leaves carrying
    /// no `declared_interface` (`Opaque` steps included: their payload is inline).
    pub opaque_without_interface: usize,
    /// The profile coordinate under which the report was computed.
    pub profile: Option<String>,
}

/// `opacity(doc, profile)` — count every leaf by authority class and every opaque body.
/// Walks the canonical JSON projection of each node and edge (a `Text` leaf is a
/// `{content_hash, authority}` object; a `CompiledPayload` is `{bytes_hash, provenance}`),
/// so any new leaf-bearing member is counted without the walker changing (CC7).
pub fn opacity(doc: &HirDocument, profile: Option<&str>) -> HirOpacity {
    let mut classes: Vec<AuthorityClass> = Vec::new();
    let mut without_interface = 0usize;
    for n in &doc.nodes {
        let j = crate::wire::node_to_json(n);
        collect_leaf_authorities(&j, &mut classes, &mut without_interface);
    }
    for e in &doc.edges {
        let j = crate::wire::edge_to_json(e);
        collect_leaf_authorities(&j, &mut classes, &mut without_interface);
    }
    HirOpacity {
        by_class: hh_provenance::OpacityReport::over(classes),
        opaque_without_interface: without_interface,
        profile: profile.map(str::to_string),
    }
}

/// Collect leaf authority classes from a canonical-record JSON — a `Text` leaf carries
/// `content_hash` + `authority`; a `CompiledPayload` carries `bytes_hash` + `provenance`
/// (whose `authority` is the leaf's class) and is *opaque without interface* when no
/// `declared_interface` member is present. Objects matching neither shape are walked.
fn collect_leaf_authorities(
    j: &hh_wire::json::Json,
    out: &mut Vec<AuthorityClass>,
    without_interface: &mut usize,
) {
    use hh_wire::json::Json;
    match j {
        Json::Obj(m) => {
            if m.contains_key("content_hash") {
                if let Some(a) = m
                    .get("authority")
                    .and_then(Json::as_str)
                    .and_then(AuthorityClass::parse)
                {
                    out.push(a);
                }
                return;
            }
            if m.contains_key("bytes_hash") {
                let auth = m
                    .get("provenance")
                    .and_then(|p| p.get("authority"))
                    .and_then(Json::as_str)
                    .and_then(AuthorityClass::parse);
                if let Some(a) = auth {
                    out.push(a);
                }
                if m.get("interface").is_none() && m.get("declared_interface").is_none() {
                    *without_interface += 1;
                }
                return;
            }
            for v in m.values() {
                collect_leaf_authorities(v, out, without_interface);
            }
        }
        Json::Arr(items) => {
            for v in items {
                collect_leaf_authorities(v, out, without_interface);
            }
        }
        _ => {}
    }
}

/// `ablate(sealed, target)` (§3.1.7) — produce the sealed definition with `target`
/// removed, for counterfactual/ablation arms:
///
/// - `target` = a node `semantic_id` → the node and every incident edge are removed and a
///   `derived-from{hypothesis: "ablation:<target>", candidate_id: <target>}` self-edge is
///   appended to the root (the lineage record, §3.1.6).
/// - `target` = a `Text` leaf's `content_hash` → every leaf with that hash is replaced by
///   a kernel-authored placeholder of matched content length (the byte-length proxy keeps
///   layout/budget estimates honest; the placeholder is `Text`, never a bare string —
///   T-LCD-02).
///
/// The result is re-sealed (`seal` re-mints identities over the changed graph), so the
/// ablated definition is an ordinary sealed input downstream. `UnresolvedRef` when
/// `target` names neither a node nor a leaf.
pub fn ablate(
    sealed: &SealedDefinition,
    target: &str,
    sealed_at: u64,
) -> Result<SealedDefinition, Vec<HirError>> {
    let mut doc = sealed.document.clone();
    let kernel_prov = ProvenanceRecord::kernel("hh-hir:ablate", sealed_at);

    if doc.node(target).is_some() {
        // Node ablation — drop the node and every incident edge, stamp the lineage edge.
        doc.nodes.retain(|n| n.semantic_id() != target);
        doc.edges.retain(|e| e.from != target && e.to != target);
        let root = doc.root.semantic_id.clone();
        let hypothesis = Text::new(format!("ablation:{target}"), "hh-hir", kernel_prov.clone());
        doc.edges.push(Edge::new(
            crate::kinds::EdgeKind::DerivedFrom,
            root.clone(),
            root,
            EdgeRecord::DerivedFrom {
                hypothesis: Box::new(hypothesis),
                trajectories: Vec::new(),
                candidate_id: Some(target.to_string()),
            },
            kernel_prov,
        ));
        return seal(&doc, sealed_at);
    }

    // Leaf ablation — replace every `Text` leaf carrying `target` as its content_hash.
    let mut replaced = 0usize;
    for leaf in doc.text_leaves_mut() {
        if leaf.content_hash == target {
            let len = leaf.content.as_deref().map(str::len).unwrap_or(0);
            let marker = format!("[ablated:{}]", &target[..target.len().min(16)]);
            let content = if len <= marker.len() {
                marker
            } else {
                let mut c = marker;
                c.push_str(&".".repeat(len - c.len()));
                c
            };
            *leaf = Text::new(content, leaf.owner.clone(), kernel_prov.clone());
            replaced += 1;
        }
    }
    if replaced == 0 {
        return Err(vec![HirError::UnresolvedRef {
            detail: format!(
                "ablate: `{target}` is neither a node semantic_id nor a Text leaf content_hash"
            ),
        }]);
    }
    seal(&doc, sealed_at)
}

/// The grammar-neutral "is this assembly section resolved?" walk (§3.1.3): any object
/// carrying a `version_selector` member, or any string in the `$param:`/`$entity:` binding
/// forms, is unresolved. Returns the first offending path.
fn unresolved_in_assembly(j: &hh_wire::json::Json, path: &str) -> Option<String> {
    use hh_wire::json::Json;
    match j {
        Json::Obj(m) => {
            if m.contains_key("version_selector") {
                return Some(format!("{path}: version_selector"));
            }
            for (k, v) in m {
                if let Some(p) = unresolved_in_assembly(v, &format!("{path}/{k}")) {
                    return Some(p);
                }
            }
            None
        }
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .find_map(|(i, v)| unresolved_in_assembly(v, &format!("{path}/{i}"))),
        Json::Str(s) if s.starts_with("$param:") || s.starts_with("$entity:") => {
            Some(format!("{path}: {s}"))
        }
        _ => None,
    }
}

/// The §5g.5 L4 walk over `assembly.extensions.refs[]`: a ref is pinned when
/// its locator carries `resolved` + `fetched_at` with no surviving `selector`
/// and `content` is present (S1.23). Returns the first offending path/member.
fn unpinned_extension_in_assembly(j: &hh_wire::json::Json, path: &str) -> Option<String> {
    use hh_wire::json::Json;
    let refs = match j {
        Json::Obj(m) => m.get("extensions")?.get("refs")?,
        _ => return None,
    };
    let items = match refs {
        Json::Arr(items) => items,
        _ => return None,
    };
    for (i, r) in items.iter().enumerate() {
        let Json::Obj(rm) = r else { continue };
        let rp = format!("{path}/extensions/refs/{i}");
        let name = rm.get("name").and_then(Json::as_str).unwrap_or("<unnamed>");
        if let Some(Json::Obj(loc)) = rm.get("locator") {
            if loc.contains_key("selector") {
                return Some(format!(
                    "{rp} ({name}): locator.selector survived into a sealed form"
                ));
            }
            if !loc.contains_key("resolved") || !loc.contains_key("fetched_at") {
                return Some(format!(
                    "{rp} ({name}): locator lacks its resolved/fetched_at pin"
                ));
            }
        }
        if !rm.contains_key("content") {
            return Some(format!("{rp} ({name}): missing the content pin"));
        }
    }
    None
}

/// A `ProfileRef` must be pinned inside a sealed form (N5) — except the `unbound` sentinel
/// on `native.profile` (§3.3.2: the profile is a configuration coordinate `link` binds;
/// T-LCD-04). Returns false if any selector remains.
fn check_profile_pins(rec: &KindRecord, errs: &mut Vec<HirError>) {
    let check = |p: &crate::refs::ProfileRef, what: &str, errs: &mut Vec<HirError>| {
        let unbound_root_profile = what == "native.profile" && p.is_unbound();
        if !p.pinned && !unbound_root_profile {
            errs.push(HirError::UnresolvedRef {
                detail: format!("{what}: unpinned ProfileRef in a sealed form"),
            });
        }
    };
    match rec {
        KindRecord::AgentProcess(a) => {
            if let AgentProcessBody::Native(n) = &a.body {
                check(&n.profile, "native.profile", errs);
            }
        }
        KindRecord::Validator(v) => {
            if let ValidatorKind::Judge(j) = &v.kind {
                check(&j.profile, "judge.profile", errs);
            }
        }
        KindRecord::HarnessRule(r) => {
            if let Some(p) = &r.conditioned_on {
                check(p, "rule.conditioned_on", errs);
            }
        }
        _ => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// migrate / project
// ─────────────────────────────────────────────────────────────────────────────

/// `migrate(doc, from, to)` — **identity-only at HIR/1** (§3.1.7): `migrate(doc, "HIR/1",
/// "HIR/1")` returns the document. Any other pair is `DialectUnsupported` until a later
/// dialect lands; a real dialect bump must migrate or return `MigrationLoss` — never a
/// silent drop (CC3).
pub fn migrate(doc: &HirDocument, from: &str, to: &str) -> Result<HirDocument, Vec<HirError>> {
    let mut errs = Vec::new();
    for d in [from, to] {
        if d != crate::DIALECT {
            errs.push(HirError::DialectUnsupported {
                dialect: d.to_string(),
            });
        }
    }
    if !errs.is_empty() {
        return Err(errs);
    }
    if doc.hir_version != from {
        return Err(vec![HirError::DialectIncompatible {
            detail: format!("document is {}, migrate asked from {from}", doc.hir_version),
        }]);
    }
    Ok(doc.clone())
}

/// The `project` selector — a plane or an entity kind (§3.1.7 `project(sealed, plane|kind)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSelector {
    /// A home plane.
    Plane(Plane),
    /// An entity kind.
    Kind(EntityKind),
}

/// The read-only sub-DAG `project` returns (§3.1.7).
#[derive(Debug, Clone, PartialEq)]
pub struct SubGraph {
    /// The nodes matching the selector.
    pub nodes: Vec<Node>,
    /// The edges with both endpoints kept.
    pub edges: Vec<Edge>,
}

/// `project(doc, plane|kind) → sub-DAG` — the nodes whose home/kind matches, plus the edges
/// whose endpoints are both kept. Read-only: returns clones; never mutates the document.
pub fn project(doc: &HirDocument, selector: ProjectSelector) -> SubGraph {
    let keep = |n: &Node| match selector {
        ProjectSelector::Plane(p) => n.home_plane() == p,
        ProjectSelector::Kind(k) => n.kind == k,
    };
    let nodes: Vec<Node> = doc.nodes.iter().filter(|n| keep(n)).cloned().collect();
    let ids: BTreeSet<String> = nodes.iter().map(|n| n.semantic_id()).collect();
    let edges: Vec<Edge> = doc
        .edges
        .iter()
        .filter(|e| ids.contains(&e.from) && ids.contains(&e.to))
        .cloned()
        .collect();
    SubGraph { nodes, edges }
}

// ─────────────────────────────────────────────────────────────────────────────
// Canonical-bytes seam (AC-IR-10): every op over canonical encoding, out-of-process.
// ─────────────────────────────────────────────────────────────────────────────

/// `canonicalize_bytes(bytes) → canonical bytes` — parse + re-emit; the byte-identity check
/// is `parse_canonical` itself (`NonCanonicalInput` on a non-canonical input).
pub fn canonicalize_bytes(bytes: &[u8]) -> Result<Vec<u8>, Vec<HirError>> {
    crate::document::parse_document(bytes)
        .map(|d| canonicalize(&d))
        .map_err(|e| vec![e])
}

/// `validate_bytes(bytes) → report | [error]`.
pub fn validate_bytes(bytes: &[u8]) -> Result<ValidationReport, Vec<HirError>> {
    match crate::document::parse_document(bytes) {
        Ok(d) => validate_doc(&d),
        Err(e) => Err(vec![e]),
    }
}

/// `seal_bytes(bytes, sealed_at) → sealed definition bytes | [error]`.
pub fn seal_bytes(bytes: &[u8], sealed_at: u64) -> Result<Vec<u8>, Vec<HirError>> {
    match crate::document::parse_document(bytes) {
        Ok(d) => seal(&d, sealed_at).map(|s| s.canonical_bytes()),
        Err(e) => Err(vec![e]),
    }
}

/// `diff_bytes(base, target, provenance_json, sealed_at) → HirDiff bytes | [error]`.
/// `provenance_json` is the canonical form of a `ProvenanceRecord`.
pub fn diff_bytes(
    base: &[u8],
    target: &[u8],
    provenance: ProvenanceRecord,
    derivation: crate::diff::DiffDerivation,
) -> Result<Vec<u8>, Vec<HirError>> {
    let b = crate::document::parse_document(base).map_err(|e| vec![e])?;
    let t = crate::document::parse_document(target).map_err(|e| vec![e])?;
    crate::diff::diff(&b, &t, provenance, derivation).map(|d| {
        crate::schema::diff_to_json(&d)
            .to_canonical_string()
            .into_bytes()
    })
}

/// `apply_bytes(base, diff_bytes) → document bytes | [error]`.
pub fn apply_bytes(base: &[u8], diff: &[u8]) -> Result<Vec<u8>, Vec<HirError>> {
    let b = crate::document::parse_document(base).map_err(|e| vec![e])?;
    let dj = hh_wire::canonical::parse_canonical(diff).map_err(|e| {
        vec![HirError::NonCanonicalInput {
            detail: format!("diff: {e}"),
        }]
    })?;
    let d = crate::schema::diff_from_json(&dj).map_err(|e| vec![e])?;
    crate::diff::apply(&b, &d).map(|t| canonicalize(&t))
}

/// `migrate_bytes(bytes, from, to) → document bytes | [error]`.
pub fn migrate_bytes(bytes: &[u8], from: &str, to: &str) -> Result<Vec<u8>, Vec<HirError>> {
    let d = crate::document::parse_document(bytes).map_err(|e| vec![e])?;
    migrate(&d, from, to).map(|m| canonicalize(&m))
}

/// `project_bytes(bytes, selector) → sub-DAG JSON bytes | [error]`. `selector` is a plane tag
/// (`P1`..`P7`) or an entity kind name.
pub fn project_bytes(bytes: &[u8], selector: &str) -> Result<Vec<u8>, Vec<HirError>> {
    let d = crate::document::parse_document(bytes).map_err(|e| vec![e])?;
    let sel = parse_selector(selector)?;
    let g = project(&d, sel);
    let out = hh_wire::json::Json::obj([
        (
            "nodes",
            hh_wire::json::Json::Arr(g.nodes.iter().map(|n| n.to_json()).collect()),
        ),
        (
            "edges",
            hh_wire::json::Json::Arr(g.edges.iter().map(|e| e.to_json()).collect()),
        ),
    ]);
    Ok(out.to_canonical_string().into_bytes())
}

fn parse_selector(s: &str) -> Result<ProjectSelector, Vec<HirError>> {
    use hh_ontology::planes::Plane::*;
    let plane = match s {
        "P1" => Some(Observation),
        "P2" => Some(Action),
        "P3" => Some(Control),
        "P4" => Some(Verification),
        "P5" => Some(State),
        "P6" => Some(Security),
        "P7" => Some(Measurement),
        _ => None,
    };
    if let Some(p) = plane {
        return Ok(ProjectSelector::Plane(p));
    }
    EntityKind::parse(s)
        .map(ProjectSelector::Kind)
        .map_err(|e| vec![e])
}
