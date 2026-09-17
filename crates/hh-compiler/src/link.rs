//! Stage 1 — `link(sealed, profile_refs[], target_refs[], variant_view?)` (§3.2.2).
//!
//! Runs `hh_assembly::link_precheck` **first** — any surviving `version_selector` is a
//! `C-LINK-1` `LinkError{unbound_slot}` (this is the DF-S1.9-3 consumer half: the
//! compiler never re-resolves, it *refuses*). Then: binds the profile chain, binds the
//! declared targets, checks `capability_requires` preconditions (ADR-0087), conditioned-
//! rule debt completeness (T-LCD-05), expired-rule diagnostics, and that every slot is
//! bound. It resolves **nothing** §3.3 owns — the variant view is pinned-`version_id`
//! lookup only.

use std::collections::BTreeMap;

use hh_assembly::{detail_text, AssemblyDiagnostic, Code, Severity, Stage};
use hh_hir::{records::KindRecord, refs::RefVersion, SealedDefinition};
use hh_provenance::ProvenanceRecord;
use hh_registry::records::{RegistryRecord, VariantRecord};
use hh_registry::RegistryStore;
use hh_wire::json::Json;

use crate::errors::{CompileError, LinkErrorKind};
use crate::profile::{
    owned_fields, profile_coordinate, resolve_chain, DebtStatus, ExtBlock, ModelProfile,
    ProfileRule, ProfileView,
};

/// A `TargetSpec` — the bound compilation target (§3.2.2: `target_refs[]` bind at link;
/// the `TargetArtefact`s they produce are a stage-4 concern). A target spec is a
/// versioned, content-pinned coordinate — the compiler never reads a live registry.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetSpec {
    /// The target's registered name (`mcp`, `a2a`, `openai-tools`, `gemini-tools`,
    /// `bedrock-converse` at Stage 1's target table).
    pub target_id: String,
    /// The target-spec version.
    pub spec_version: String,
    /// The content address of the spec record.
    pub content_hash: String,
}

/// The registered target table — the §3.2.5 rows: `provider_tool_api`, `mcp`, `acp`,
/// `a2a`, `agent_spec` (the native runtime is not a target spec — its artefact is the
/// `RuntimePlan` itself). Per-target lowering is a stage-4 concern (S3.2); the *set* is
/// registered now — `unknown_target` is a typed refusal.
pub const REGISTERED_TARGETS: &[&str] = &["a2a", "acp", "agent_spec", "mcp", "provider_tool_api"];

/// The C0-admitted surface families a `tool_shape` rule may name (§3.2.3/§5d: at C0 the
/// only compiled shape is the primitive `native_fc` family — `SurfaceFamily` records are
/// C1/Stage-5 registry kinds; a family id outside this set is `UnexpressibleSurface`,
/// never a warning — AC-CP-05).
pub const C0_SURFACE_FAMILIES: &[&str] = &["native_fc"];

/// The pinned-`version_id`-only variant read seam (DF-S1.9-3's "zero resolutions": the
/// signature carries no resolver — a `VariantView` reads by *pinned* `version_id` and
/// nothing else).
pub trait VariantView {
    /// The `VariantRecord` at a pinned `version_id`; `None` when the bound view doesn't
    /// carry it (`version_conflict`).
    fn variant(&self, version_id: &str) -> Option<VariantRecord>;
}

/// A `VariantView` over a `RegistryStore` — the store's `get(version_id)` is already a
/// pinned-coordinate read.
impl VariantView for RegistryStore {
    fn variant(&self, version_id: &str) -> Option<VariantRecord> {
        match self.get(version_id).map(|(_, r)| r) {
            Some(RegistryRecord::Variant(v)) => Some(v.clone()),
            _ => None,
        }
    }
}

/// A `VariantView` that admits nothing — for compiles that bind no variants and want the
/// refusal loud if one is named.
pub struct NoVariants;

impl VariantView for NoVariants {
    fn variant(&self, _version_id: &str) -> Option<VariantRecord> {
        None
    }
}

/// A conditioned rule surfaced at link — home-tagged for `lcd_report.conditioned_rules`
/// (`profile | definition | variant`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionedRule {
    /// The rule's id (its home's coordinate space).
    pub rule_id: String,
    /// The rule's home.
    pub home: ConditionedRuleHome,
    /// The debt status spelling (`active|expiring|expired|retired` on the profile
    /// vocabulary; `open|discharged|violated` on the §3.1 one).
    pub status: String,
    /// The owning profile coordinate / variant version / definition rule id.
    pub owner: String,
}

/// Where a conditioned rule lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionedRuleHome {
    /// A `ModelProfile/1` rule.
    Profile,
    /// A definition `HarnessRule` (`conditioned_on ≠ null`).
    Definition,
    /// A variant's `conditioned_rules` entry.
    Variant,
}

impl ConditionedRuleHome {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ConditionedRuleHome::Profile => "profile",
            ConditionedRuleHome::Definition => "definition",
            ConditionedRuleHome::Variant => "variant",
        }
    }
}

/// The bound profile chain — base→version, with the effective (merged) view.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundProfile {
    /// The chain, base first.
    pub chain: Vec<ModelProfile>,
    /// `true` when the chain was bound as the explicit `fallback_profile` escape
    /// (ADR-0124 §5).
    pub is_fallback: bool,
}

/// The linked graph — stage 1's output.
#[derive(Debug, Clone)]
pub struct LinkedGraph {
    /// The validated sealed definition.
    pub sealed: SealedDefinition,
    /// The bound profile chain (base→version order).
    pub profile: BoundProfile,
    /// The bound target specs.
    pub targets: Vec<TargetSpec>,
    /// Every conditioned rule across the three homes.
    pub conditioned_rules: Vec<ConditionedRule>,
    /// The diagnostics link produced (expired-rule warnings etc.) — these ride into the
    /// bundle's `diagnostics[]`.
    pub diagnostics: Vec<AssemblyDiagnostic>,
    /// The bound variants the link resolved-by-lookup: `{slot → (class_id, variant_id,
    /// version_id)}` — for the derivation key.
    pub variant_pins: BTreeMap<String, (String, String, String)>,
}

fn diag(
    code: Code,
    severity: Severity,
    path: &str,
    subject: &str,
    detail: &str,
    remedy: &str,
    kernel: &ProvenanceRecord,
) -> AssemblyDiagnostic {
    AssemblyDiagnostic {
        code,
        class: None,
        severity,
        path: path.to_string(),
        source_layer: None,
        subject: subject.to_string(),
        stage: Stage::Link,
        detail: detail_text(detail, kernel),
        remedy: remedy.to_string(),
        owner_adr: "ADR-0019".to_string(),
    }
}

/// `link(sealed, profile_refs[], target_refs[], variant_view?)` (§3.2.2).
///
/// `profile_refs` are the bound coordinates (`profile_id@version` or `content_hash`);
/// `fallback_profile` is the explicit ADR-0124 §5 escape — it binds only when it carries
/// a *dated* debt hypothesis.
#[allow(clippy::too_many_arguments)] // the arity is the §3.2 link-input record's.
pub fn link(
    sealed: &SealedDefinition,
    profile_refs: &[String],
    fallback_profile: Option<&str>,
    target_refs: &[TargetSpec],
    profiles: &dyn ProfileView,
    variants: &dyn VariantView,
    kernel: &ProvenanceRecord,
    compile_for_expired: bool,
) -> Result<LinkedGraph, CompileError> {
    let mut diagnostics: Vec<AssemblyDiagnostic> = Vec::new();

    // ── link_precheck FIRST — a surviving selector is C-LINK-1 (DF-S1.9-3). ───────
    if let Err(diags) = hh_assembly::link_precheck(sealed) {
        return Err(CompileError::LinkError {
            kind: LinkErrorKind::UnboundSlot,
            detail: "an unpinned selector reaches link — the compiler never re-resolves"
                .to_string(),
            diagnostics: diags,
        });
    }

    let doc = &sealed.document;

    // ── Targets — every declared target_ref must name a registered target. ────────
    for t in target_refs {
        if !REGISTERED_TARGETS.contains(&t.target_id.as_str()) {
            return Err(CompileError::LinkError {
                kind: LinkErrorKind::UnknownTarget,
                detail: format!(
                    "target {} is not in the registered target table",
                    t.target_id
                ),
                diagnostics: vec![diag(
                    Code::LinkUnknownTarget,
                    Severity::Error,
                    "/targets",
                    &t.target_id,
                    &format!(
                        "target {} is not in the registered target table",
                        t.target_id
                    ),
                    "bind a registered target (provider_tool_api, mcp, acp, a2a, agent_spec)",
                    kernel,
                )],
            });
        }
    }

    // ── Profile binding ──────────────────────────────────────────────────────────
    let chain = if profile_refs.is_empty() {
        match fallback_profile {
            Some(coord) => {
                let p = profiles
                    .profile(coord)
                    .ok_or_else(|| CompileError::NoProfile {
                        detail: format!(
                            "fallback_profile {coord} is not in the bound profile view"
                        ),
                    })?;
                // ADR-0124 §5: admissible only with a *dated* debt hypothesis.
                if !p.expiry.is_dated() {
                    return Err(CompileError::NoProfile {
                        detail: format!(
                            "fallback_profile {} carries no dated debt hypothesis",
                            profile_coordinate(&p)
                        ),
                    });
                }
                vec![p]
            }
            None => {
                return Err(CompileError::NoProfile {
                    detail: "no profile bound and no admissible fallback_profile (ADR-0124 §5)"
                        .to_string(),
                })
            }
        }
    } else {
        resolve_chain(profile_refs, profiles)?
    };
    let is_fallback = profile_refs.is_empty();

    // The definition's pinned profile must be the bound primary (version_conflict).
    if let Some(pinned) = definition_pinned_profile(doc) {
        let head = chain.last().expect("non-empty chain");
        if pinned != profile_coordinate(head)
            && pinned != head.content_hash
            && pinned != head.profile_id
        {
            return Err(CompileError::LinkError {
                kind: LinkErrorKind::VersionConflict,
                detail: format!(
                    "definition pins profile {pinned} but the bound chain's head is {}",
                    profile_coordinate(head)
                ),
                diagnostics: vec![diag(
                    Code::LinkVersionConflict,
                    Severity::Error,
                    "/native/profile",
                    &pinned,
                    &format!(
                        "definition pins profile {pinned} but the bound chain's head is {}",
                        profile_coordinate(head)
                    ),
                    "bind the pinned profile (or repin the definition)",
                    kernel,
                )],
            });
        }
    }

    // Chain merge: a child's overrides must touch only its owned fields (per-field merge
    // policy — `PROFILE_MERGE_POLICIES`; `forbid_override` members refuse any child set).
    for w in chain.windows(2) {
        let (parent, child) = (&w[0], &w[1]);
        let owned = owned_fields(child);
        for member in crate::profile::member_diff(parent, child) {
            let policy = crate::profile::profile_merge_policy(&member);
            if policy == crate::profile::ProfileMergePolicy::ForbidOverride {
                return Err(CompileError::InvalidModelProfile {
                    detail: format!(
                        "{} overrides /{member} which is forbid_override",
                        profile_coordinate(child)
                    ),
                });
            }
            // Identity members a version profile always owns (id/version/hash/selector/
            // extends); the rest must be rule-owned.
            if !owned.contains(&member) {
                return Err(CompileError::InvalidModelProfile {
                    detail: format!(
                        "{} overrides /{member} outside its rules' owned_fields",
                        profile_coordinate(child)
                    ),
                });
            }
        }
    }

    // ── Profile-rule expressibility (AC-CP-05): a bound rule may require only what
    // C0 can express. `interaction_mode` admits `native_fc` only (§3.2.3); `tool_shape`
    // may name only the C0 primitive family set. `UnexpressibleSurface` is an *error*,
    // never a warning. ─────────────────────────────────────────────────────────────
    for p in &chain {
        let coord = profile_coordinate(p);
        for r in &p.rules {
            match r.kind {
                crate::profile::ProfileRuleKind::InteractionMode => {
                    for member in ["mode", "fallback"] {
                        if let Some(m) = r.params.get(member).and_then(Json::as_str) {
                            if m != "native_fc" {
                                return Err(CompileError::UnexpressibleSurface {
                                    entity: coord.clone(),
                                    profile: coord.clone(),
                                    reason: format!(
                                        "interaction_mode.{member} = {m} — C0 admits `native_fc` only"
                                    ),
                                });
                            }
                        }
                    }
                }
                crate::profile::ProfileRuleKind::ToolShape => {
                    // `{capability_class → SurfaceFamilyRef{family_id, variant_id}}` —
                    // every named family must be in the C0 set (CF-199: by id, never an
                    // inline surface).
                    if let Json::Obj(m) = &r.params {
                        for (capability_class, family_ref) in m {
                            let family_id = family_ref
                                .get("family_id")
                                .and_then(Json::as_str)
                                .unwrap_or("");
                            if !C0_SURFACE_FAMILIES.contains(&family_id) {
                                return Err(CompileError::UnexpressibleSurface {
                                    entity: capability_class.clone(),
                                    profile: coord.clone(),
                                    reason: format!(
                                        "tool_shape names family {family_id} outside the C0 set {C0_SURFACE_FAMILIES:?}"
                                    ),
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // ── Conditioned rules across the three homes ─────────────────────────────────
    let mut conditioned: Vec<ConditionedRule> = Vec::new();
    let mut missing_debt: Vec<AssemblyDiagnostic> = Vec::new();
    let mut expired: Vec<AssemblyDiagnostic> = Vec::new();

    // (a) profile rules — each needs a complete ProfileDebtRecord.
    for p in &chain {
        for r in &p.rules {
            check_profile_rule_debt(r, p, kernel, &mut missing_debt, &mut expired);
            conditioned.push(ConditionedRule {
                rule_id: r.rule_id.clone(),
                home: ConditionedRuleHome::Profile,
                status: r.debt.status.name().to_string(),
                owner: profile_coordinate(p),
            });
        }
        for (k, ext) in &p.ext {
            check_ext_debt(k, ext, p, kernel, &mut missing_debt, &mut expired);
            // A metered ext block is a conditioned rule whose home is the profile —
            // inventory it so `lcd_report.conditioned_rules` is complete.
            conditioned.push(ConditionedRule {
                rule_id: format!("ext/{k}"),
                home: ConditionedRuleHome::Profile,
                status: ext.debt.status.name().to_string(),
                owner: profile_coordinate(p),
            });
        }
    }

    // (b) definition HarnessRules — `conditioned_on ≠ null` ⇒ complete
    // AssumptionDebtRecord (validated upstream as ConditionedRuleIncomplete; link
    // re-checks completeness + status for the compile record).
    for node in &doc.nodes {
        if let KindRecord::HarnessRule(r) = &node.semantic {
            if r.conditioned_on.is_some() {
                match &r.assumption_debt {
                    Some(d) if debt_complete(d) => {
                        if d.status == hh_hir::DebtStatus::Violated {
                            expired.push(diag(
                                Code::LinkExpiredRule,
                                Severity::Warning,
                                &format!("/nodes/{}", node.semantic_id()),
                                &r.rule_id,
                                &format!(
                                    "definition rule {} carries a violated debt record",
                                    r.rule_id
                                ),
                                "discharge or retire the rule before relying on the bundle",
                                kernel,
                            ));
                        }
                        conditioned.push(ConditionedRule {
                            rule_id: r.rule_id.clone(),
                            home: ConditionedRuleHome::Definition,
                            status: d.status.name().to_string(),
                            owner: node.semantic_id(),
                        });
                    }
                    _ => missing_debt.push(diag(
                        Code::LinkMissingDebtRecord,
                        Severity::Error,
                        &format!("/nodes/{}", node.semantic_id()),
                        &r.rule_id,
                        &format!(
                            "conditioned rule {} lacks a complete debt record",
                            r.rule_id
                        ),
                        "author the {rule_id, hypothesis, evidence_refs, owner, expiry_condition, removal_test_ref, status} record",
                        kernel,
                    )),
                }
            }
        }
    }

    // (c) bound variants — conditioned_rules on each bound variant record.
    let mut variant_pins: BTreeMap<String, (String, String, String)> = BTreeMap::new();
    for (slot, binding) in all_slot_bindings(doc) {
        let version_id = match &binding.variant.version {
            RefVersion::Pinned(v) => v.clone(),
            RefVersion::Selector(s) => {
                // link_precheck owns this refusal — reaching it here means a selector
                // lives outside its walk; refuse the same way (defence in depth).
                return Err(CompileError::LinkError {
                    kind: LinkErrorKind::UnboundSlot,
                    detail: format!("slot {slot} carries unpinned selector {s}"),
                    diagnostics: vec![diag(
                        Code::LinkUnboundSlot,
                        Severity::Error,
                        &format!("/slots/{slot}"),
                        &slot,
                        &format!("slot {slot} carries unpinned selector {s}"),
                        "resolve the definition before link",
                        kernel,
                    )],
                });
            }
        };
        let record = variants
            .variant(&version_id)
            .ok_or_else(|| CompileError::LinkError {
                kind: LinkErrorKind::VersionConflict,
                detail: format!(
                    "slot {slot} pins variant {}@{} which the bound view does not carry",
                    binding.variant.variant_id, version_id
                ),
                diagnostics: vec![diag(
                    Code::LinkVersionConflict,
                    Severity::Error,
                    &format!("/slots/{slot}"),
                    &version_id,
                    &format!(
                        "slot {slot} pins variant {}@{} absent from the bound view",
                        binding.variant.variant_id, version_id
                    ),
                    "bind a view that covers the pinned variant set",
                    kernel,
                )],
            })?;
        variant_pins.insert(
            slot.clone(),
            (
                binding.variant.class_id.clone(),
                binding.variant.variant_id.clone(),
                version_id.clone(),
            ),
        );
        for (rule_id, debt) in &record.conditioned_rules {
            if debt_complete(debt) {
                if debt.status == hh_hir::DebtStatus::Violated {
                    expired.push(diag(
                        Code::LinkExpiredRule,
                        Severity::Warning,
                        &format!("/slots/{slot}"),
                        rule_id,
                        &format!(
                            "variant rule {rule_id} on {} carries a violated debt record",
                            binding.variant.variant_id
                        ),
                        "discharge or retire the rule before relying on the bundle",
                        kernel,
                    ));
                }
                conditioned.push(ConditionedRule {
                    rule_id: rule_id.clone(),
                    home: ConditionedRuleHome::Variant,
                    status: debt.status.name().to_string(),
                    owner: version_id.clone(),
                });
            } else {
                missing_debt.push(diag(
                    Code::LinkMissingDebtRecord,
                    Severity::Error,
                    &format!("/slots/{slot}"),
                    rule_id,
                    &format!("variant conditioned rule {rule_id} lacks a complete debt record"),
                    "register the variant with a complete AssumptionDebtRecord",
                    kernel,
                ));
            }
        }
    }

    // ── capability_requires (ADR-0087): a PreconditionDomain::Capability needs a
    // `depends-on` edge to another ToolCapability. ─────────────────────────────────
    for node in &doc.nodes {
        if let KindRecord::ToolCapability(t) = &node.semantic {
            let needs = t
                .preconditions
                .contains(&hh_hir::PreconditionDomain::Capability);
            if needs {
                let satisfied = doc.edges_from(&node.semantic_id()).any(|e| {
                    e.kind == hh_hir::EdgeKind::DependsOn
                        && doc
                            .node(&e.to)
                            .map(|n| matches!(n.semantic, KindRecord::ToolCapability(_)))
                            .unwrap_or(false)
                });
                if !satisfied {
                    missing_debt.push(diag(
                        Code::LinkCapabilityRequires,
                        Severity::Error,
                        &format!("/nodes/{}", node.semantic_id()),
                        &node.semantic_id(),
                        &format!(
                            "capability {} declares a `capability` precondition with no `depends-on` edge to another ToolCapability",
                            node.semantic_id()
                        ),
                        "author the `depends-on` edge declaring the required capability",
                        kernel,
                    ));
                }
            }
        }
    }

    // ── dialect narrowing: a surface narrowing must be declared and admitted by the
    // bound profile (a `schema_dialect` rule names the admitted set; absent rule → the
    // OQ-219 default only). ─────────────────────────────────────────────────────────
    let admitted_dialects: std::collections::BTreeSet<String> = {
        let mut s = std::collections::BTreeSet::new();
        s.insert(crate::equiv::DEFAULT_SCHEMA_DIALECT.to_string());
        for p in &chain {
            for r in &p.rules {
                if r.kind == crate::profile::ProfileRuleKind::SchemaDialect {
                    if let Some(d) = r.params.get("dialect").and_then(Json::as_str) {
                        s.insert(d.to_string());
                    }
                }
            }
        }
        s
    };
    for node in &doc.nodes {
        if let Some(hh_hir::SurfaceRecord::Tool(ts)) = &node.surface {
            if let Some(d) = ts
                .as_ref()
                .schema_dialect_narrowing
                .get("dialect")
                .and_then(Json::as_str)
            {
                if !admitted_dialects.contains(d) {
                    return Err(CompileError::DialectNarrowingUndeclared {
                        detail: format!(
                            "surface on {} narrows to dialect {d} which no bound profile's schema_dialect rule admits",
                            node.semantic_id()
                        ),
                    });
                }
            }
        }
    }

    // ── Every slot bound: the mandatory slots (control_strategy, context_policy)
    // must carry a binding on the root native process (§3.1.4 — validate owns the
    // *presence* check; link re-asserts the *binding* is non-empty and pinned). ────
    let slots = all_slot_bindings(doc);
    for required in ["control_strategy", "context_policy"] {
        if !slots.contains_key(required) {
            missing_debt.push(diag(
                Code::LinkUnboundSlot,
                Severity::Error,
                &format!("/slots/{required}"),
                required,
                &format!("mandatory slot {required} carries no binding at link"),
                "bind the slot (the §3.3 resolver pins it)",
                kernel,
            ));
        }
    }

    if !missing_debt.is_empty() {
        // The typed kind from the first diagnostic's code.
        let kind = match missing_debt[0].code {
            Code::LinkUnboundSlot => LinkErrorKind::UnboundSlot,
            Code::LinkMissingDebtRecord => LinkErrorKind::MissingDebtRecord,
            Code::LinkVersionConflict => LinkErrorKind::VersionConflict,
            Code::LinkUnknownTarget => LinkErrorKind::UnknownTarget,
            // An unsatisfied `capability_requires` is an unbound requirement — the
            // closed kind set carries it as `unbound_slot` (the required capability is
            // not bound into the linked graph).
            Code::LinkCapabilityRequires => LinkErrorKind::UnboundSlot,
            _ => LinkErrorKind::MissingDebtRecord,
        };
        return Err(CompileError::LinkError {
            kind,
            detail: format!("{} link refusal(s)", missing_debt.len()),
            diagnostics: missing_debt,
        });
    }

    // Expired rules: admissible only under an explicit recorded intent (ADR-0020 §7) —
    // otherwise an error.
    if !expired.is_empty() && !compile_for_expired {
        return Err(CompileError::LinkError {
            kind: LinkErrorKind::MissingDebtRecord,
            detail: format!(
                "{} conditioned rule(s) carry expired/violated debt and no recorded intent was supplied",
                expired.len()
            ),
            diagnostics: expired,
        });
    }
    diagnostics.extend(expired);

    Ok(LinkedGraph {
        sealed: sealed.clone(),
        profile: BoundProfile { chain, is_fallback },
        targets: target_refs.to_vec(),
        conditioned_rules: conditioned,
        diagnostics,
        variant_pins,
    })
}

/// The definition's pinned profile coordinate, when `native.profile` or
/// `assembly.profile_binding` pins one.
fn definition_pinned_profile(doc: &hh_hir::HirDocument) -> Option<String> {
    for node in &doc.nodes {
        if let KindRecord::AgentProcess(ap) = &node.semantic {
            if let hh_hir::AgentProcessBody::Native(n) = &ap.body {
                if n.profile.pinned && !n.profile.is_unbound() {
                    return Some(n.profile.profile.clone());
                }
            }
        }
    }
    None
}

/// All slot bindings across the document — `native.slots` on every `AgentProcess` plus
/// the sealed `assembly.slots` (both pinned post-`link_precheck`). Keyed `slot` (a
/// duplicate slot name across scopes collides by design — the root's slots are the
/// bound set).
fn all_slot_bindings(doc: &hh_hir::HirDocument) -> BTreeMap<String, hh_hir::records::SlotBinding> {
    let mut out = BTreeMap::new();
    for node in &doc.nodes {
        if let KindRecord::AgentProcess(ap) = &node.semantic {
            if let hh_hir::AgentProcessBody::Native(n) = &ap.body {
                for (slot, bindings) in &n.slots {
                    let all: Vec<&hh_hir::records::SlotBinding> = match bindings {
                        hh_hir::records::SlotBindings::One(b) => vec![b],
                        hh_hir::records::SlotBindings::Many(bs) => bs.iter().collect(),
                    };
                    for b in all {
                        out.insert(slot.clone(), b.clone());
                    }
                }
            }
        }
    }
    // The sealed assembly section carries the resolved slot grammar too — read it for
    // slots bound only there (the root process's slots after `materialise` mirror it;
    // reading both is belt-and-braces, pinned either way).
    if let Some(j) = &doc.assembly {
        if let Some(Json::Obj(slots)) = j.get("slots") {
            for (slot, v) in slots {
                if let Ok(b) =
                    hh_hir::wire::slot_binding_from_json(v, &format!("/assembly/slots/{slot}"))
                {
                    out.entry(slot.clone()).or_insert(b);
                } else if let Ok(bs) = parse_slot_list(v, slot) {
                    for b in bs {
                        out.entry(slot.clone()).or_insert(b);
                    }
                }
            }
        }
    }
    out
}

fn parse_slot_list(
    v: &Json,
    slot: &str,
) -> Result<Vec<hh_hir::records::SlotBinding>, hh_hir::HirError> {
    match v {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                hh_hir::wire::slot_binding_from_json(item, &format!("/assembly/slots/{slot}[{i}]"))
            })
            .collect(),
        _ => Ok(Vec::new()),
    }
}

/// The §3.1 `AssumptionDebtRecord` completeness check (definition/variant home):
/// every field populated.
fn debt_complete(d: &hh_hir::records::AssumptionDebtRecord) -> bool {
    !d.rule_id.is_empty()
        && !d.hypothesis.content_hash.is_empty()
        && !d.owner.is_empty()
        && !d.expiry_condition.is_empty()
        && !d.removal_test_ref.is_empty()
}

fn check_profile_rule_debt(
    r: &ProfileRule,
    p: &ModelProfile,
    kernel: &ProvenanceRecord,
    missing: &mut Vec<AssemblyDiagnostic>,
    expired: &mut Vec<AssemblyDiagnostic>,
) {
    if !r.debt.is_complete() {
        missing.push(diag(
            Code::LinkMissingDebtRecord,
            Severity::Error,
            &format!("/profiles/{}", profile_coordinate(p)),
            &r.rule_id,
            &format!(
                "profile rule {} on {} carries an incomplete debt record",
                r.rule_id,
                profile_coordinate(p)
            ),
            "complete the debt record's {rule_id, hypothesis, evidence_refs, owner, expiry_condition, removal_test_ref, status}",
            kernel,
        ));
    } else if r.debt.status == DebtStatus::Expired || r.debt.status == DebtStatus::Expiring {
        expired.push(diag(
            Code::LinkExpiredRule,
            Severity::Warning,
            &format!("/profiles/{}", profile_coordinate(p)),
            &r.rule_id,
            &format!(
                "profile rule {} on {} carries {} debt",
                r.rule_id,
                profile_coordinate(p),
                r.debt.status.name()
            ),
            "discharge or retire the rule before relying on the bundle",
            kernel,
        ));
    }
}

fn check_ext_debt(
    key: &str,
    ext: &ExtBlock,
    p: &ModelProfile,
    kernel: &ProvenanceRecord,
    missing: &mut Vec<AssemblyDiagnostic>,
    expired: &mut Vec<AssemblyDiagnostic>,
) {
    if !ext.debt.is_complete() {
        missing.push(diag(
            Code::LinkMissingDebtRecord,
            Severity::Error,
            &format!("/profiles/{}/ext/{key}", profile_coordinate(p)),
            key,
            &format!(
                "metered ext block {key} on {} lacks a complete debt record",
                profile_coordinate(p)
            ),
            "complete the block's debt record",
            kernel,
        ));
    } else if ext.debt.status == DebtStatus::Expired {
        expired.push(diag(
            Code::LinkExpiredRule,
            Severity::Warning,
            &format!("/profiles/{}/ext/{key}", profile_coordinate(p)),
            key,
            &format!(
                "metered ext block {key} on {} carries expired debt",
                profile_coordinate(p)
            ),
            "discharge or retire the block before relying on the bundle",
            kernel,
        ));
    }
}
