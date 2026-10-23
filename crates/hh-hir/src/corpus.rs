//! The deterministic **HIR/1 document corpus** — the named fixtures the
//! cross-implementation replay bundle (`hh-xcheck`, R4.3 / DF-S1.2-2's
//! "extend to the HIR/ledger corpus" leg) serializes as replay *inputs*.
//!
//! The `hh-identity::corpus` / `hh-registry::corpus` precedent (CC1 — corpus
//! builders live in the owning crate): authored `semantic_id`s and
//! `ProvenanceRecord::minted` rows at fixed `created_at` seqs, no clock, no
//! filesystem — the same construction `tests/common` pins.

use std::collections::BTreeMap;

use hharness_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hharness_wire::json::Json;

use crate::document::{HirDocument, Node};
use crate::kinds::{EffectClass, EffectDomain, EntityKind, ToolEffects};
use crate::leaves::Text;
use crate::records::*;
use crate::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};

/// The sealed_at the corpus seals under — deterministic, never a clock.
pub const CORPUS_SEALED_AT: u64 = 1_700_000_000;

/// The corpus: `(name, document)` pairs — the `hir-documents` arm's
/// `<name>.input.json` members are each doc's `canonical_bytes()`. Every
/// pipeline leg is exercised: sealed docs (native + hosted bodies), a
/// validate-invalid, and a seal-refusal; [`missing_provenance_bytes`]
/// adds the byte-level parse-refusal case.
pub fn document_corpus() -> Vec<(&'static str, HirDocument)> {
    vec![
        ("minimal-native", base_doc()),
        ("hosted-agent", hosted_agent()),
        ("unresolved-ref", unresolved_ref()),
        ("gated-tool-no-contract", gated_tool_no_contract()),
    ]
}

/// `missing-provenance` — `base_doc()`'s canonical bytes with the first
/// node's `provenance` member stripped: `parse_document` must refuse
/// `MissingProvenance` (the record is mandatory on every node and edge).
/// The bytes stay a canonical encoding — the refusal is semantic.
pub fn missing_provenance_bytes() -> Vec<u8> {
    let bytes = base_doc().canonical_bytes();
    let mut doc = match hharness_wire::canonical::parse_canonical(&bytes) {
        Ok(j) => j,
        Err(e) => unreachable!("corpus doc must re-parse: {e}"),
    };
    if let Json::Obj(m) = &mut doc {
        if let Some(Json::Arr(nodes)) = m.get_mut("nodes") {
            if let Some(Json::Obj(n0)) = nodes.first_mut() {
                n0.remove("provenance");
            }
        }
    }
    doc.to_canonical_string().into_bytes()
}

// ── fixture builders (the tests/common spellings, corpus-pinned) ───────────

fn prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("corpus:author", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    )
}

fn sel(sid: &str) -> Ref {
    Ref::selected(sid, "latest")
}

fn sid(mut n: Node, id: &str) -> Node {
    n.version.semantic_id = Some(id.into());
    n
}

fn node(kind: EntityKind, rec: KindRecord, seq: u64) -> Node {
    Node::new(kind, rec, prov(seq))
}

fn rule_node(id: &str, seq: u64) -> Node {
    sid(
        node(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: format!("{id}.rule"),
                trigger: Json::Null,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            seq,
        ),
        id,
    )
}

fn budget_node(id: &str, dims: &[(&str, u64)], seq: u64) -> Node {
    let dimensions = dims
        .iter()
        .map(|(k, h)| {
            (
                k.to_string(),
                DimensionBound {
                    hard: Some(*h),
                    soft: None,
                },
            )
        })
        .collect();
    sid(
        node(
            EntityKind::Budget,
            KindRecord::Budget(BudgetRecord {
                dimensions,
                scope: "*".into(),
                parent: None,
                accounting: sel("corpus:rule"),
            }),
            seq,
        ),
        id,
    )
}

fn perm_node(id: &str, holder: &str, seq: u64) -> Node {
    sid(
        node(
            EntityKind::Permission,
            KindRecord::Permission(PermissionRecord {
                holder: sel(holder),
                grants: vec![],
                issuer: Issuer {
                    authority: AuthorityClass::Kernel,
                    reference: "corpus:issuer".into(),
                },
                validity: Validity::open_from(0),
                revocation: None,
            }),
            seq,
        ),
        id,
    )
}

fn agent_node(id: &str, budget: &str, perm: &str, seq: u64) -> Node {
    let mut slots = BTreeMap::new();
    for name in ["control_strategy", "context_policy"] {
        slots.insert(
            name.to_string(),
            SlotBindings::One(SlotBinding::of(ComponentVariantRef::pinned(
                name,
                format!("hh/{name}/test"),
                format!("sha256:variant-{name}"),
            ))),
        );
    }
    sid(
        node(
            EntityKind::AgentProcess,
            KindRecord::AgentProcess(AgentProcessRecord {
                body: AgentProcessBody::Native(NativeProcess {
                    harness_def: sel(id),
                    profile: ProfileRef {
                        profile: "sha256:profile".into(),
                        pinned: true,
                    },
                    slots,
                    control_boundary: Default::default(),
                    budget: sel(budget),
                    permissions: sel(perm),
                    environment: EnvironmentRef {
                        environment: "env:corpus".into(),
                    },
                }),
            }),
            seq,
        ),
        id,
    )
}

fn tool_node(id: &str, effects: ToolEffects, seq: u64) -> Node {
    sid(
        node(
            EntityKind::ToolCapability,
            KindRecord::ToolCapability(ToolCapabilityRecord {
                purpose: Text::new("a corpus tool", "corpus:owner", prov(seq)),
                input_schema: Json::Null,
                output_schema: None,
                effects,
                preconditions: vec![],
                scope_bindings: ScopeBindings::Unknown,
                resources: Resources::NoneDeclared,
                observation_contract: Json::Null,
                cost_model: None,
                execution_requirement: Json::Null,
                source: Json::Null,
                exposure_hint: Json::Null,
                postconditions: vec![],
                flow_contract: None,
                action_patterns: Vec::new(),
            }),
            seq,
        ),
        id,
    )
}

/// `minimal-native` — the minimal valid document: `{rule, budget, perm,
/// agent}` DAG-shaped except the `holder ↔ permissions` cycle (resolved
/// through the authored sids).
fn base_doc() -> HirDocument {
    let mut doc = HirDocument::new(sel("corpus:agent"));
    doc.nodes.push(rule_node("corpus:rule", 1));
    doc.nodes
        .push(budget_node("corpus:budget", &[("tokens.blended", 1000)], 2));
    doc.nodes.push(perm_node("corpus:perm", "corpus:agent", 3));
    doc.nodes.push(agent_node(
        "corpus:agent",
        "corpus:budget",
        "corpus:perm",
        4,
    ));
    doc
}

/// `hosted-agent` — the base doc with an `OpaqueProcess` body: `seal`
/// computes and pins its `version_identity` (the claim the definition
/// makes).
fn hosted_agent() -> HirDocument {
    let mut doc = base_doc();
    if let KindRecord::AgentProcess(a) = &mut doc.nodes[3].semantic {
        a.body = AgentProcessBody::Hosted(OpaqueProcess {
            participant_ref: "participant:corpus".into(),
            declared_capabilities: CapabilityDeclarationRecord::all_unknown(),
            observability_levels: Default::default(),
            participant_version: "sha256:pv".into(),
            version_identity: None,
            hosting_mechanism: hharness_ontology::participant::HostingMechanism::SessionAbi,
            supplies: Supplies::default(),
            budget: sel("corpus:budget"),
            permissions: sel("corpus:perm"),
            params: None,
        });
    }
    doc
}

/// `unresolved-ref` — the agent's `permissions` names a sid no node
/// carries: `validate` collects `UnresolvedRef`; the seal leg never runs.
fn unresolved_ref() -> HirDocument {
    let mut doc = base_doc();
    if let KindRecord::AgentProcess(a) = &mut doc.nodes[3].semantic {
        if let AgentProcessBody::Native(n) = &mut a.body {
            n.permissions = sel("corpus:missing");
        }
    }
    doc
}

/// `gated-tool-no-contract` — a `ToolCapability` declaring the gated
/// `fs_read` domain with no `flow_contract`: `validate` passes (the
/// absence gate is `seal`'s — §5g.2 / R-2.8.2), `seal` collects
/// `ContributionUndeclared`.
fn gated_tool_no_contract() -> HirDocument {
    let mut gated = std::collections::BTreeSet::new();
    gated.insert(EffectClass::domain_only(EffectDomain::FsRead));
    let mut doc = base_doc();
    doc.nodes
        .push(tool_node("corpus:tool", ToolEffects::Declared(gated), 6));
    doc
}
