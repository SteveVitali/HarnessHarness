//! `hh-monitor/authorize-input/1` — the canonical encoding of every recorded
//! input [`Monitor::authorize`] reads (AC-R-2.8.1-16; T-LCD-12 — every §5g.1
//! operation runs out-of-process over the canonical encoding).
//!
//! The record carries the **recorded inputs**, never derived conclusions: the
//! handle-table rows (`security.permission.granted` payloads), the Π table in
//! force, the registered proposer labels, the registered capabilities
//! (record + compiled `SurfaceBinding`), the approval fold (`pending`,
//! `decided`, leases, the I-P5 counters, denial counts), the run's live
//! coordinates, the sealed `auto_review`/`denial` policy legs and the
//! `Proposal` itself. `model_profile` is an *envelope* member — recorded
//! context for the run that produced the proposal; the decoder reads it and
//! the decision never consumes it (the profile-invariance half of AC-16 is
//! the byte-equality test over two profiles).
//!
//! One spelling per record (CC1/CC7): handles encode as
//! [`events::granted_payload`], leases as [`approval::lease_granted_payload`],
//! pending requests as [`approval::request_payload`], capability records via
//! `hh_hir::wire::semantic_record_json`, bindings via
//! `hh_compiler::schema::surface_binding_json` — the schema sources own the
//! member spellings; this module owns only the envelope.

use std::collections::BTreeMap;

use hh_compiler::plan::PinnedRef;
use hh_hir::kinds::EntityKind;
use hh_hir::records::{Grant, KindRecord};
use hh_ontology::risk::RiskClass;
use hh_provenance::authority::{AuthorityClass, ReaderSet};
use hh_provenance::Label;
use hh_wire::json::Json;

use crate::approval::{
    self, ApprovalMode, ApprovalPending, ApprovalState, ApprovalStats, AutoReviewRule,
    DenialFallback, DenialPolicy, RecordedDecision, ReviewVerdictKind,
};
use crate::decision::{Decision, DenyReason};
use crate::events;
use crate::handle::AuthorityHandle;
use crate::monitor::{CapabilityEntry, ContainmentGate, Monitor, Proposal};
use crate::policy::{Cond, Mode, PiVerdict, PolicyRow, PolicyTable};
use crate::table::{GrantUse, HandleTable};

/// The document kind tag (`kind` member).
pub const AUTHORIZE_INPUT_KIND: &str = "hh-monitor/authorize-input/1";

/// `AuthorizeInput` — the whole recorded input surface of one `authorize`
/// call (§5g.1 §2.1 `authorize(p, at)`).
#[derive(Debug, Clone)]
pub struct AuthorizeInput {
    /// The run's live coordinates (`{run_id, turn_id, effect_id, session}`).
    pub run_id: String,
    /// The current turn id.
    pub turn_id: String,
    /// The current effect-scope id (liveness for `effect`-expiry handles).
    pub effect_id: String,
    /// The session ref (empty when none).
    pub session: String,
    /// The declared approval mode (I-P4).
    pub approval_mode: ApprovalMode,
    /// The `approvals.requested` hard ceiling (`None` = unbudgeted).
    pub approvals_max: Option<u64>,
    /// The lease key's policy leg (ADR-0071 D1) — the fingerprint in force,
    /// carried as the recorded value (the leaf fold isn't invertible).
    pub policy_fingerprint: String,
    /// The handle-table rows (encoded as `granted` payloads under their
    /// `granted` event ids — the fold's input).
    pub handles: Vec<(String, AuthorityHandle)>,
    /// The per-handle usage meters (`decided{handle_ids[]}` folds).
    pub uses: BTreeMap<String, u64>,
    /// The Π table in force.
    pub policy: PolicyTable,
    /// The registered proposers (`semantic_id → label`).
    pub proposers: BTreeMap<String, Label>,
    /// The registered capabilities (`semantic_id → CapabilityEntry` — the
    /// map key is the record's own coordinate).
    pub capabilities: Vec<(String, CapabilityEntry)>,
    /// The sealed `auto_review` rule set (`auto_review_rules(sealed)`).
    pub auto_review_rules: Vec<AutoReviewRule>,
    /// The sealed repeated-denial policy.
    pub denial_policy: Option<DenialPolicy>,
    /// The approval fold (pending/decided/leases/counters).
    pub approvals: ApprovalState,
    /// The proposal under decision.
    pub proposal: Proposal,
    /// The model-profile coordinate the producing run carried — envelope
    /// metadata only; `authorize` has no profile input and never reads it
    /// (AC-R-2.8.1-16's invariance clause).
    pub model_profile: Option<String>,
}

impl AuthorizeInput {
    /// Rebuild the `Monitor` the decision runs over (the out-of-process half's
    /// state reconstruction — identical semantics to the in-process setup).
    pub fn monitor(&self) -> Monitor {
        let mut table = HandleTable::default();
        for (_, h) in &self.handles {
            table.handles.insert(h.handle_id.clone(), h.clone());
        }
        for (id, n) in &self.uses {
            if let Some(hid) = crate::handle::HandleId::parse(id) {
                table.uses.insert(hid, GrantUse { decisions: *n });
            }
        }
        let mut m = Monitor::new(table, self.policy.clone());
        m.proposers = self.proposers.clone();
        m.capabilities = self
            .capabilities
            .iter()
            .map(|(id, c)| (id.clone(), c.clone()))
            .collect();
        m.effect_id = self.effect_id.clone();
        m.turn_id = self.turn_id.clone();
        m.run_id = self.run_id.clone();
        m.session = self.session.clone();
        m.auto_review_rules = self.auto_review_rules.clone();
        m.denial_policy = self.denial_policy;
        m.approvals = self.approvals.clone();
        m.approvals_max = self.approvals_max;
        m.approval_mode = self.approval_mode;
        m.policy_fingerprint = self.policy_fingerprint.clone();
        m
    }

    /// `capture` — snapshot a live `Monitor`'s recorded state plus the
    /// `Proposal` into the wire form (the encode direction — the dispatcher's
    /// out-of-process call and the byte-equality test both emit this).
    /// `model_profile` is envelope metadata (the producing run's coordinate);
    /// it never reaches `authorize`.
    pub fn capture(m: &Monitor, proposal: &Proposal, model_profile: Option<String>) -> Self {
        AuthorizeInput {
            run_id: m.run_id.clone(),
            turn_id: m.turn_id.clone(),
            effect_id: m.effect_id.clone(),
            session: m.session.clone(),
            approval_mode: m.approval_mode,
            approvals_max: m.approvals_max,
            policy_fingerprint: m.policy_fingerprint.clone(),
            handles: m
                .table
                .handles
                .values()
                .map(|h| (h.validity.issued_at.clone(), h.clone()))
                .collect(),
            uses: m
                .table
                .uses
                .iter()
                .map(|(k, v)| (k.0.clone(), v.decisions))
                .collect(),
            policy: m.policy.clone(),
            proposers: m.proposers.clone(),
            capabilities: m
                .capabilities
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            auto_review_rules: m.auto_review_rules.clone(),
            denial_policy: m.denial_policy,
            approvals: m.approvals.clone(),
            proposal: proposal.clone(),
            model_profile,
        }
    }

    /// Canonical JSON (`to_canonical_string` on the result is the wire form).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("kind", Json::str(AUTHORIZE_INPUT_KIND)),
            (
                "model_profile",
                self.model_profile
                    .as_ref()
                    .map(|p| Json::str(p.clone()))
                    .unwrap_or(Json::Null),
            ),
            (
                "run",
                Json::obj([
                    ("run_id", Json::str(self.run_id.clone())),
                    ("turn_id", Json::str(self.turn_id.clone())),
                    ("effect_id", Json::str(self.effect_id.clone())),
                    ("session", Json::str(self.session.clone())),
                ]),
            ),
            ("approval_mode", Json::str(self.approval_mode.as_str())),
            (
                "approvals_max",
                self.approvals_max
                    .map(|m| Json::Int(m as i64))
                    .unwrap_or(Json::Null),
            ),
            (
                "policy_fingerprint",
                Json::str(self.policy_fingerprint.clone()),
            ),
            (
                "handles",
                Json::Arr(
                    self.handles
                        .iter()
                        .map(|(eid, h)| {
                            Json::obj([
                                ("granted_event_id", Json::str(eid.clone())),
                                ("payload", events::granted_payload(h)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "uses",
                Json::Obj(
                    self.uses
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                        .collect(),
                ),
            ),
            ("policy", policy_json(&self.policy)),
            (
                "proposers",
                Json::Arr(
                    self.proposers
                        .iter()
                        .map(|(id, l)| {
                            Json::obj([
                                ("semantic_id", Json::str(id.clone())),
                                ("label", label_json(l)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "capabilities",
                Json::Arr(
                    self.capabilities
                        .iter()
                        .map(|(id, c)| {
                            Json::obj([
                                ("semantic_id", Json::str(id.clone())),
                                (
                                    "record",
                                    hh_hir::wire::semantic_record_json(
                                        &KindRecord::ToolCapability(c.record.clone()),
                                        false,
                                    ),
                                ),
                                (
                                    "binding",
                                    hh_compiler::schema::surface_binding_json(&c.binding),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "auto_review_rules",
                Json::Arr(
                    self.auto_review_rules
                        .iter()
                        .map(review_rule_json)
                        .collect(),
                ),
            ),
            (
                "denial_policy",
                self.denial_policy
                    .as_ref()
                    .map(denial_policy_json)
                    .unwrap_or(Json::Null),
            ),
            ("approvals", approvals_json(&self.approvals)),
            ("proposal", proposal_json(&self.proposal)),
        ])
    }

    /// Parse the wire form (fails closed on every malformed member).
    pub fn from_json(j: &Json) -> Result<AuthorizeInput, String> {
        let kind = req_str(j, "kind")?;
        if kind != AUTHORIZE_INPUT_KIND {
            return Err(format!("kind: expected {AUTHORIZE_INPUT_KIND}, got {kind}"));
        }
        let run = req(j, "run")?;
        let handles = match req(j, "handles")? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    let eid = req_str(row, "granted_event_id")
                        .map_err(|e| format!("handles[{i}].{e}"))?;
                    let payload = row
                        .get("payload")
                        .ok_or_else(|| format!("handles[{i}].payload missing"))?;
                    let h = events::handle_from_granted_payload(payload, &eid)
                        .ok_or_else(|| format!("handles[{i}].payload malformed"))?;
                    Ok((eid, h))
                })
                .collect::<Result<Vec<_>, String>>()?,
            _ => return Err("handles must be an array".to_string()),
        };
        let uses = match req(j, "uses")? {
            Json::Obj(m) => m
                .iter()
                .map(|(k, v)| {
                    v.as_int()
                        .map(|n| (k.clone(), n.max(0) as u64))
                        .ok_or_else(|| format!("uses.{k} must be an int"))
                })
                .collect::<Result<BTreeMap<_, _>, String>>()?,
            _ => return Err("uses must be an object".to_string()),
        };
        let capabilities = match req(j, "capabilities")? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let semantic_id =
                        req_str(c, "semantic_id").map_err(|e| format!("capabilities[{i}].{e}"))?;
                    let rec_j = c
                        .get("record")
                        .ok_or_else(|| format!("capabilities[{i}].record missing"))?;
                    let record = match hh_hir::wire::semantic_record_from_json(
                        EntityKind::ToolCapability,
                        rec_j,
                        &format!("capabilities[{i}].record"),
                    )
                    .map_err(|e| format!("capabilities[{i}].record: {e}"))?
                    {
                        KindRecord::ToolCapability(t) => t,
                        _ => {
                            return Err(format!("capabilities[{i}].record: not a tool_capability"))
                        }
                    };
                    let binding = hh_compiler::schema::surface_binding_from_json(
                        c.get("binding")
                            .ok_or_else(|| format!("capabilities[{i}].binding missing"))?,
                        &format!("capabilities[{i}].binding"),
                    )
                    .map_err(|e| format!("capabilities[{i}].binding: {e}"))?;
                    Ok((semantic_id, CapabilityEntry { record, binding }))
                })
                .collect::<Result<Vec<_>, String>>()?,
            _ => return Err("capabilities must be an array".to_string()),
        };
        let proposers = match req(j, "proposers")? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let id =
                        req_str(p, "semantic_id").map_err(|e| format!("proposers[{i}].{e}"))?;
                    let label = Label::from_json(
                        p.get("label")
                            .ok_or_else(|| format!("proposers[{i}].label missing"))?,
                    )
                    .map_err(|e| format!("proposers[{i}].label: {e}"))?;
                    Ok((id, label))
                })
                .collect::<Result<BTreeMap<_, _>, String>>()?,
            _ => return Err("proposers must be an array".to_string()),
        };
        let auto_review_rules = match req(j, "auto_review_rules")? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    review_rule_from_json(r).map_err(|e| format!("auto_review_rules[{i}]: {e}"))
                })
                .collect::<Result<Vec<_>, String>>()?,
            _ => return Err("auto_review_rules must be an array".to_string()),
        };
        Ok(AuthorizeInput {
            run_id: req_str(run, "run_id")?,
            turn_id: req_str(run, "turn_id")?,
            effect_id: req_str(run, "effect_id")?,
            session: run
                .get("session")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            approval_mode: ApprovalMode::parse(&req_str(j, "approval_mode")?)
                .ok_or_else(|| "approval_mode unknown".to_string())?,
            approvals_max: j
                .get("approvals_max")
                .and_then(Json::as_int)
                .map(|n| n.max(0) as u64),
            policy_fingerprint: j
                .get("policy_fingerprint")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            handles,
            uses,
            policy: policy_from_json(req(j, "policy")?)?,
            proposers,
            capabilities,
            auto_review_rules,
            denial_policy: match j.get("denial_policy") {
                Some(Json::Null) | None => None,
                Some(d) => Some(denial_policy_from_json(d)?),
            },
            approvals: approvals_from_json(req(j, "approvals")?)?,
            proposal: proposal_from_json(req(j, "proposal")?)?,
            model_profile: j
                .get("model_profile")
                .and_then(Json::as_str)
                .map(String::from),
        })
    }
}

// ── shared scalar codecs ─────────────────────────────────────────────────────

fn req<'a>(j: &'a Json, k: &str) -> Result<&'a Json, String> {
    j.get(k).ok_or_else(|| format!("{k} missing"))
}

fn req_str(j: &Json, k: &str) -> Result<String, String> {
    j.get(k)
        .and_then(Json::as_str)
        .map(String::from)
        .ok_or_else(|| format!("{k} missing/not a string"))
}

fn req_int(j: &Json, k: &str) -> Result<i64, String> {
    j.get(k)
        .and_then(Json::as_int)
        .ok_or_else(|| format!("{k} missing/not an int"))
}

fn str_arr(j: &Json, k: &str) -> Result<Vec<String>, String> {
    match j.get(k) {
        Some(Json::Arr(items)) => items
            .iter()
            .map(|i| i.as_str().map(String::from))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| format!("{k} members must be strings")),
        _ => Err(format!("{k} must be an array")),
    }
}

/// The canonical `Label` spelling `{authority, taint[], readers[]}` — the
/// decode side is `Label::from_json` (hh-provenance); this emit matches it.
fn label_json(l: &Label) -> Json {
    Json::obj([
        ("authority", Json::str(l.authority.as_str())),
        (
            "taint",
            Json::Arr(l.taint.iter().map(|t| Json::str(t.as_string())).collect()),
        ),
        (
            "readers",
            match &l.readers {
                ReaderSet::Public => Json::Arr(vec![]),
                ReaderSet::Restricted(rs) => {
                    Json::Arr(rs.iter().map(|r| Json::str(r.clone())).collect())
                }
            },
        ),
    ])
}

fn pinned_ref_json(r: &PinnedRef) -> Json {
    Json::obj([
        ("semantic_id", Json::str(r.semantic_id.clone())),
        ("version_id", Json::str(r.version_id.clone())),
    ])
}

fn pinned_ref_from_json(j: &Json, path: &str) -> Result<PinnedRef, String> {
    Ok(PinnedRef {
        semantic_id: req_str(j, "semantic_id").map_err(|e| format!("{path}.{e}"))?,
        version_id: req_str(j, "version_id").map_err(|e| format!("{path}.{e}"))?,
    })
}

// ── PolicyTable (Π) ──────────────────────────────────────────────────────────

fn cond_json(c: &Cond) -> Json {
    match c {
        Cond::Always => Json::str("always"),
        Cond::DomainIs(d) => Json::obj([("domain_is", Json::str(d.name()))]),
        Cond::RevIs(r) => Json::obj([("rev_is", Json::str(r.name()))]),
        Cond::ScopeIs(s) => Json::obj([("scope_is", Json::str(s.name()))]),
        Cond::EffAtMost(a) => Json::obj([("eff_at_most", Json::str(a.as_str()))]),
        Cond::Tainted => Json::str("tainted"),
        Cond::InsideWritableRoots => Json::str("inside_writable_roots"),
        Cond::CompensationCovered => Json::str("compensation_covered"),
        Cond::ChildContained => Json::str("child_contained"),
        Cond::SecretDestInScope => Json::str("secret_dest_in_scope"),
        Cond::SecretTransportIs(t) => Json::obj([("secret_transport_is", Json::str(t.as_str()))]),
        Cond::ModelVisibleSink => Json::str("model_visible_sink"),
        Cond::SpendOverSoft => Json::str("spend_over_soft"),
        Cond::SoleRecipientPrincipal => Json::str("sole_recipient_principal"),
        Cond::HostAllowlisted => Json::str("host_allowlisted"),
        Cond::MemoryScopeAtMost(s) => Json::obj([("memory_scope_at_most", Json::str(s.as_str()))]),
        Cond::PreAuthorized => Json::str("pre_authorized"),
        Cond::Not(inner) => Json::obj([("not", cond_json(inner))]),
    }
}

fn cond_from_json(j: &Json, path: &str) -> Result<Cond, String> {
    if let Some(s) = j.as_str() {
        return match s {
            "always" => Ok(Cond::Always),
            "tainted" => Ok(Cond::Tainted),
            "inside_writable_roots" => Ok(Cond::InsideWritableRoots),
            "compensation_covered" => Ok(Cond::CompensationCovered),
            "child_contained" => Ok(Cond::ChildContained),
            "secret_dest_in_scope" => Ok(Cond::SecretDestInScope),
            "model_visible_sink" => Ok(Cond::ModelVisibleSink),
            "spend_over_soft" => Ok(Cond::SpendOverSoft),
            "sole_recipient_principal" => Ok(Cond::SoleRecipientPrincipal),
            "host_allowlisted" => Ok(Cond::HostAllowlisted),
            "pre_authorized" => Ok(Cond::PreAuthorized),
            other => Err(format!("{path}: unknown condition {other}")),
        };
    }
    if let Some(v) = j.get("domain_is").and_then(Json::as_str) {
        return hh_hir::kinds::EffectDomain::parse(v)
            .map(Cond::DomainIs)
            .map_err(|_| format!("{path}.domain_is: unknown {v}"));
    }
    if let Some(v) = j.get("rev_is").and_then(Json::as_str) {
        return hh_ontology::risk::RiskReversibility::parse(v)
            .map(Cond::RevIs)
            .ok_or_else(|| format!("{path}.rev_is: unknown {v}"));
    }
    if let Some(v) = j.get("scope_is").and_then(Json::as_str) {
        return hh_ontology::risk::RiskScope::parse(v)
            .map(Cond::ScopeIs)
            .ok_or_else(|| format!("{path}.scope_is: unknown {v}"));
    }
    if let Some(v) = j.get("eff_at_most").and_then(Json::as_str) {
        return AuthorityClass::parse(v)
            .map(Cond::EffAtMost)
            .ok_or_else(|| format!("{path}.eff_at_most: unknown {v}"));
    }
    if let Some(v) = j.get("secret_transport_is").and_then(Json::as_str) {
        return crate::assess::SecretTransport::parse(v)
            .map(Cond::SecretTransportIs)
            .ok_or_else(|| format!("{path}.secret_transport_is: unknown {v}"));
    }
    if let Some(v) = j.get("memory_scope_at_most").and_then(Json::as_str) {
        return crate::assess::MemoryScope::parse(v)
            .map(Cond::MemoryScopeAtMost)
            .ok_or_else(|| format!("{path}.memory_scope_at_most: unknown {v}"));
    }
    if let Some(inner) = j.get("not") {
        return Ok(Cond::Not(Box::new(cond_from_json(
            inner,
            &format!("{path}.not"),
        )?)));
    }
    Err(format!("{path}: malformed condition"))
}

fn policy_json(p: &PolicyTable) -> Json {
    Json::obj([
        ("version_id", Json::str(p.version_id.clone())),
        (
            "mode",
            Json::str(match p.mode {
                Mode::Attended => "attended",
                Mode::Unattended => "unattended",
            }),
        ),
        (
            "unattended_policy",
            Json::str(match p.unattended_policy {
                crate::policy::UnattendedPolicy::Deny => "deny",
                crate::policy::UnattendedPolicy::Defer => "defer",
                crate::policy::UnattendedPolicy::AutoReview => "auto_review",
            }),
        ),
        (
            "rows",
            Json::Arr(
                p.rows
                    .iter()
                    .map(|r| {
                        Json::obj([
                            ("id", Json::str(r.id.clone())),
                            (
                                "conditions",
                                Json::Arr(r.conditions.iter().map(cond_json).collect()),
                            ),
                            ("verdict", Json::str(r.verdict.as_str())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn policy_from_json(j: &Json) -> Result<PolicyTable, String> {
    let rows = match req(j, "rows")? {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let conditions = match r.get("conditions") {
                    Some(Json::Arr(cs)) => cs
                        .iter()
                        .enumerate()
                        .map(|(k, c)| {
                            cond_from_json(c, &format!("policy.rows[{i}].conditions[{k}]"))
                        })
                        .collect::<Result<Vec<_>, String>>()?,
                    _ => return Err(format!("policy.rows[{i}].conditions must be an array")),
                };
                Ok(PolicyRow {
                    id: req_str(r, "id").map_err(|e| format!("policy.rows[{i}].{e}"))?,
                    conditions,
                    verdict: match req_str(r, "verdict")
                        .map_err(|e| format!("policy.rows[{i}].{e}"))?
                        .as_str()
                    {
                        "allow" => PiVerdict::Allow,
                        "ask" => PiVerdict::Ask,
                        "deny" => PiVerdict::Deny,
                        other => return Err(format!("policy.rows[{i}].verdict unknown {other}")),
                    },
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        _ => return Err("policy.rows must be an array".to_string()),
    };
    Ok(PolicyTable {
        version_id: req_str(j, "version_id")?,
        mode: match req_str(j, "mode")?.as_str() {
            "attended" => Mode::Attended,
            "unattended" => Mode::Unattended,
            other => return Err(format!("policy.mode unknown {other}")),
        },
        unattended_policy: match req_str(j, "unattended_policy")?.as_str() {
            "deny" => crate::policy::UnattendedPolicy::Deny,
            "defer" => crate::policy::UnattendedPolicy::Defer,
            "auto_review" => crate::policy::UnattendedPolicy::AutoReview,
            other => return Err(format!("policy.unattended_policy unknown {other}")),
        },
        rows,
    })
}

// ── approval fold ────────────────────────────────────────────────────────────

fn review_rule_json(r: &AutoReviewRule) -> Json {
    Json::obj([
        ("rule_ref", Json::str(r.rule_ref.clone())),
        (
            "domains",
            Json::Arr(r.domains.iter().map(|d| Json::str(d.name())).collect()),
        ),
        ("max_risk", r.max_risk.to_json()),
        (
            "eff_at_most",
            r.eff_at_most
                .map(|a| Json::str(a.as_str()))
                .unwrap_or(Json::Null),
        ),
        (
            "verdict",
            Json::str(match r.verdict {
                ReviewVerdictKind::Allow => "allow",
                ReviewVerdictKind::Deny => "deny",
                ReviewVerdictKind::Amend => "amend",
            }),
        ),
    ])
}

fn review_rule_from_json(j: &Json) -> Result<AutoReviewRule, String> {
    let domains = match j.get("domains") {
        Some(Json::Arr(items)) => items
            .iter()
            .map(|d| {
                d.as_str()
                    .and_then(|s| hh_hir::kinds::EffectDomain::parse(s).ok())
                    .ok_or_else(|| "auto_review_rule.domains member unknown".to_string())
            })
            .collect::<Result<Vec<_>, String>>()?,
        _ => Vec::new(),
    };
    Ok(AutoReviewRule {
        rule_ref: req_str(j, "rule_ref")?,
        domains,
        max_risk: RiskClass::from_json(
            j.get("max_risk")
                .ok_or_else(|| "auto_review_rule.max_risk missing".to_string())?,
        )
        .ok_or_else(|| "auto_review_rule.max_risk malformed".to_string())?,
        eff_at_most: j
            .get("eff_at_most")
            .and_then(Json::as_str)
            .map(|s| {
                AuthorityClass::parse(s)
                    .ok_or_else(|| format!("auto_review_rule.eff_at_most unknown {s}"))
            })
            .transpose()?,
        verdict: match req_str(j, "verdict")?.as_str() {
            "allow" => ReviewVerdictKind::Allow,
            "deny" => ReviewVerdictKind::Deny,
            "amend" => ReviewVerdictKind::Amend,
            other => return Err(format!("auto_review_rule.verdict unknown {other}")),
        },
    })
}

fn denial_policy_json(d: &DenialPolicy) -> Json {
    Json::obj([
        ("max_denials", Json::Int(d.max_denials as i64)),
        ("fallback", Json::str(d.fallback.as_str())),
    ])
}

fn denial_policy_from_json(j: &Json) -> Result<DenialPolicy, String> {
    Ok(DenialPolicy {
        max_denials: req_int(j, "max_denials")?.max(0) as u64,
        fallback: match req_str(j, "fallback")?.as_str() {
            "escalate" => DenialFallback::Escalate,
            "stop_run" => DenialFallback::StopRun,
            "refuse_class" => DenialFallback::RefuseClass,
            other => return Err(format!("denial_policy.fallback unknown {other}")),
        },
    })
}

fn decision_json(d: &Decision) -> Json {
    match d {
        Decision::Allow => Json::obj([("tag", Json::str("allow"))]),
        Decision::Ask { options, remedies } => Json::obj([
            ("tag", Json::str("ask")),
            (
                "options",
                Json::Arr(options.iter().map(|o| Json::str(o.clone())).collect()),
            ),
            (
                "remedies",
                Json::Arr(remedies.iter().map(|r| Json::str(r.clone())).collect()),
            ),
        ]),
        Decision::Deny { reason, remedies } => Json::obj([
            ("tag", Json::str("deny")),
            ("reason", Json::str(reason.as_str())),
            (
                "remedies",
                Json::Arr(remedies.iter().map(|r| Json::str(r.clone())).collect()),
            ),
        ]),
    }
}

fn deny_reason_parse(s: &str) -> Result<DenyReason, String> {
    Ok(match s {
        "MissingProvenance" => DenyReason::MissingProvenance,
        "UnmappedArgument" => DenyReason::UnmappedArgument,
        "UnscopedParameter" => DenyReason::UnscopedParameter,
        "UnknownProposer" => DenyReason::UnknownProposer,
        "NoCoveringGrant" => DenyReason::NoCoveringGrant,
        "GrantConstraintExhausted" => DenyReason::GrantConstraintExhausted,
        "HandleRevoked" => DenyReason::HandleRevoked,
        "PolicyDenied" => DenyReason::PolicyDenied,
        "ScopeCeilingExceeded" => DenyReason::ScopeCeilingExceeded,
        "AuthorityWidening" => DenyReason::AuthorityWidening,
        "NotDelegable" => DenyReason::NotDelegable,
        "BudgetExceedsParent" => DenyReason::BudgetExceedsParent,
        "ApprovalsExhausted" => DenyReason::ApprovalsExhausted,
        "UnattendedAsk" => DenyReason::UnattendedAsk,
        "ApprovalTimedOut" => DenyReason::ApprovalTimedOut,
        "ContainmentUnverified" => DenyReason::ContainmentUnverified,
        "containment" => DenyReason::Containment,
        other => return Err(format!("deny reason unknown {other}")),
    })
}

fn decision_from_json(j: &Json, path: &str) -> Result<Decision, String> {
    let remedies = match j.get("remedies") {
        Some(Json::Arr(items)) => items
            .iter()
            .map(|i| i.as_str().map(String::from))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| format!("{path}.remedies members must be strings"))?,
        _ => Vec::new(),
    };
    match req_str(j, "tag")
        .map_err(|e| format!("{path}.{e}"))?
        .as_str()
    {
        "allow" => Ok(Decision::Allow),
        "ask" => Ok(Decision::Ask {
            options: str_arr(j, "options").map_err(|e| format!("{path}.{e}"))?,
            remedies,
        }),
        "deny" => Ok(Decision::Deny {
            reason: deny_reason_parse(&req_str(j, "reason").map_err(|e| format!("{path}.{e}"))?)
                .map_err(|e| format!("{path}.{e}"))?,
            remedies,
        }),
        other => Err(format!("{path}.tag unknown {other}")),
    }
}

fn recorded_decision_json(d: &RecordedDecision) -> Json {
    Json::obj([
        ("decision", decision_json(&d.decision)),
        ("decided_by", approval::endorser_json(&d.decided_by)),
        ("decided_at", Json::Int(d.decided_at as i64)),
        (
            "lease_id",
            d.lease_id
                .as_ref()
                .map(|l| Json::str(l.clone()))
                .unwrap_or(Json::Null),
        ),
        (
            "effect_ids",
            Json::Arr(d.effect_ids.iter().map(|e| Json::str(e.clone())).collect()),
        ),
    ])
}

fn recorded_decision_from_json(j: &Json, path: &str) -> Result<RecordedDecision, String> {
    Ok(RecordedDecision {
        decision: decision_from_json(
            j.get("decision")
                .ok_or_else(|| format!("{path}.decision missing"))?,
            &format!("{path}.decision"),
        )?,
        decided_by: approval::decode_endorser(
            j.get("decided_by")
                .ok_or_else(|| format!("{path}.decided_by missing"))?,
        )
        .map_err(|e| format!("{path}.decided_by: {e}"))?,
        decided_at: req_int(j, "decided_at")
            .map_err(|e| format!("{path}.{e}"))?
            .max(0) as u64,
        lease_id: j.get("lease_id").and_then(Json::as_str).map(String::from),
        effect_ids: str_arr(j, "effect_ids").map_err(|e| format!("{path}.{e}"))?,
    })
}

fn pending_json(p: &ApprovalPending) -> Json {
    Json::obj([
        ("permission_id", Json::str(p.permission_id.clone())),
        ("request", approval::request_payload(&p.request)),
        ("requested_at", Json::Int(p.requested_at as i64)),
        (
            "effect_ids",
            Json::Arr(p.effect_ids.iter().map(|e| Json::str(e.clone())).collect()),
        ),
    ])
}

fn pending_from_json(j: &Json, path: &str) -> Result<ApprovalPending, String> {
    Ok(ApprovalPending {
        permission_id: req_str(j, "permission_id").map_err(|e| format!("{path}.{e}"))?,
        request: approval::request_from_payload(
            j.get("request")
                .ok_or_else(|| format!("{path}.request missing"))?,
            &format!("{path}.request"),
        )?,
        requested_at: req_int(j, "requested_at")
            .map_err(|e| format!("{path}.{e}"))?
            .max(0) as u64,
        effect_ids: str_arr(j, "effect_ids").map_err(|e| format!("{path}.{e}"))?,
    })
}

fn approvals_json(s: &ApprovalState) -> Json {
    Json::obj([
        (
            "stats",
            Json::obj([
                ("requested", Json::Int(s.stats.requested as i64)),
                ("granted", Json::Int(s.stats.granted as i64)),
                ("human_wait_ms", Json::Int(s.stats.human_wait_ms as i64)),
            ]),
        ),
        (
            "denial_counts",
            Json::Obj(
                s.denial_counts
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                    .collect(),
            ),
        ),
        (
            "leases",
            Json::Obj(
                s.leases
                    .iter()
                    .map(|(k, l)| (k.clone(), approval::lease_granted_payload(l)))
                    .collect(),
            ),
        ),
        (
            "pending",
            Json::Arr(s.pending.values().map(pending_json).collect()),
        ),
        (
            "decisions",
            Json::Obj(
                s.decisions
                    .iter()
                    .map(|(k, d)| (k.clone(), recorded_decision_json(d)))
                    .collect(),
            ),
        ),
    ])
}

fn approvals_from_json(j: &Json) -> Result<ApprovalState, String> {
    let stats_j = req(j, "stats")?;
    let denial_counts = match req(j, "denial_counts")? {
        Json::Obj(m) => m
            .iter()
            .map(|(k, v)| {
                v.as_int()
                    .map(|n| (k.clone(), n.max(0) as u64))
                    .ok_or_else(|| format!("approvals.denial_counts.{k} must be an int"))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?,
        _ => return Err("approvals.denial_counts must be an object".to_string()),
    };
    let leases = match req(j, "leases")? {
        Json::Obj(m) => m
            .iter()
            .map(|(k, v)| {
                approval::lease_from_granted(v)
                    .map(|l| (k.clone(), l))
                    .ok_or_else(|| format!("approvals.leases.{k} malformed"))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?,
        _ => return Err("approvals.leases must be an object".to_string()),
    };
    let pending = match req(j, "pending")? {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let p = pending_from_json(p, &format!("approvals.pending[{i}]"))?;
                Ok((p.permission_id.clone(), p))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?,
        _ => return Err("approvals.pending must be an array".to_string()),
    };
    let decisions = match req(j, "decisions")? {
        Json::Obj(m) => m
            .iter()
            .enumerate()
            .map(|(i, (k, v))| {
                recorded_decision_from_json(v, &format!("approvals.decisions[{i}]"))
                    .map(|d| (k.clone(), d))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?,
        _ => return Err("approvals.decisions must be an object".to_string()),
    };
    Ok(ApprovalState {
        pending,
        decisions,
        leases,
        stats: ApprovalStats {
            requested: req_int(stats_j, "requested")?.max(0) as u64,
            granted: req_int(stats_j, "granted")?.max(0) as u64,
            human_wait_ms: req_int(stats_j, "human_wait_ms")?.max(0) as u64,
        },
        denial_counts,
    })
}

// ── proposal ─────────────────────────────────────────────────────────────────

fn inputs_json(i: &crate::assess::AssessmentInputs) -> Json {
    fn tri(t: crate::assess::Tri) -> Json {
        Json::str(match t {
            crate::assess::Tri::Yes => "yes",
            crate::assess::Tri::No => "no",
            crate::assess::Tri::Unknown => "unknown",
        })
    }
    Json::obj([
        ("inside_writable_roots", tri(i.inside_writable_roots)),
        (
            "command_parse",
            i.command_parse
                .map(|p| {
                    Json::str(match p {
                        crate::assess::ParseOutcome::Parsed => "parsed",
                        crate::assess::ParseOutcome::Partial => "partial",
                        crate::assess::ParseOutcome::Failed => "failed",
                    })
                })
                .unwrap_or(Json::Null),
        ),
        ("host_allowlisted", tri(i.host_allowlisted)),
        ("compensation_registered", tri(i.compensation_registered)),
        ("compensator_covered", tri(i.compensator_covered)),
        ("sole_recipient_principal", tri(i.sole_recipient_principal)),
        (
            "secret_transport",
            i.secret_transport
                .map(|t| Json::str(t.as_str()))
                .unwrap_or(Json::Null),
        ),
        ("destination_in_scope", tri(i.destination_in_scope)),
        ("model_visible_sink", tri(i.model_visible_sink)),
        ("spend_over_soft", tri(i.spend_over_soft)),
        ("child_contained", tri(i.child_contained)),
        ("pre_authorized", tri(i.pre_authorized)),
        (
            "memory_scope",
            i.memory_scope
                .map(|s| Json::str(s.as_str()))
                .unwrap_or(Json::Null),
        ),
    ])
}

fn tri_from_json(j: Option<&Json>, path: &str) -> Result<crate::assess::Tri, String> {
    match j.and_then(Json::as_str) {
        Some("yes") => Ok(crate::assess::Tri::Yes),
        Some("no") => Ok(crate::assess::Tri::No),
        Some("unknown") | None => Ok(crate::assess::Tri::Unknown),
        Some(other) => Err(format!("{path}: unknown tri {other}")),
    }
}

fn inputs_from_json(j: &Json, path: &str) -> Result<crate::assess::AssessmentInputs, String> {
    let t = |k: &str| tri_from_json(j.get(k), &format!("{path}.{k}"));
    Ok(crate::assess::AssessmentInputs {
        inside_writable_roots: t("inside_writable_roots")?,
        command_parse: match j.get("command_parse").and_then(Json::as_str) {
            Some("parsed") => Some(crate::assess::ParseOutcome::Parsed),
            Some("partial") => Some(crate::assess::ParseOutcome::Partial),
            Some("failed") => Some(crate::assess::ParseOutcome::Failed),
            None => None,
            Some(other) => return Err(format!("{path}.command_parse unknown {other}")),
        },
        host_allowlisted: t("host_allowlisted")?,
        compensation_registered: t("compensation_registered")?,
        compensator_covered: t("compensator_covered")?,
        sole_recipient_principal: t("sole_recipient_principal")?,
        secret_transport: match j.get("secret_transport").and_then(Json::as_str) {
            Some(s) => Some(
                crate::assess::SecretTransport::parse(s)
                    .ok_or_else(|| format!("{path}.secret_transport unknown {s}"))?,
            ),
            None => None,
        },
        destination_in_scope: t("destination_in_scope")?,
        model_visible_sink: t("model_visible_sink")?,
        spend_over_soft: t("spend_over_soft")?,
        child_contained: t("child_contained")?,
        pre_authorized: t("pre_authorized")?,
        memory_scope: match j.get("memory_scope").and_then(Json::as_str) {
            Some(s) => Some(
                crate::assess::MemoryScope::parse(s)
                    .ok_or_else(|| format!("{path}.memory_scope unknown {s}"))?,
            ),
            None => None,
        },
    })
}

fn containment_json(c: &ContainmentGate) -> Json {
    match c {
        ContainmentGate::Clear => Json::obj([("kind", Json::str("clear"))]),
        ContainmentGate::Denied { detail } => Json::obj([
            ("kind", Json::str("denied")),
            ("detail", Json::str(detail.clone())),
        ]),
        ContainmentGate::Unverified { group } => Json::obj([
            ("kind", Json::str("unverified")),
            ("group", Json::str(group.clone())),
        ]),
    }
}

fn containment_from_json(j: &Json, path: &str) -> Result<ContainmentGate, String> {
    match req_str(j, "kind")
        .map_err(|e| format!("{path}.{e}"))?
        .as_str()
    {
        "clear" => Ok(ContainmentGate::Clear),
        "denied" => Ok(ContainmentGate::Denied {
            detail: req_str(j, "detail").map_err(|e| format!("{path}.{e}"))?,
        }),
        "unverified" => Ok(ContainmentGate::Unverified {
            group: req_str(j, "group").map_err(|e| format!("{path}.{e}"))?,
        }),
        other => Err(format!("{path}.kind unknown {other}")),
    }
}

fn proposal_json(p: &Proposal) -> Json {
    Json::obj([
        ("effect_id", Json::str(p.effect_id.clone())),
        ("attempt_no", Json::Int(p.attempt_no as i64)),
        ("proposer", Json::str(p.proposer.clone())),
        ("capability_ref", pinned_ref_json(&p.capability_ref)),
        ("effect", p.effect.to_json()),
        ("surface_args", p.surface_args.clone()),
        (
            "args_provenance",
            p.args_provenance
                .as_ref()
                .map(|r| r.to_json())
                .unwrap_or(Json::Null),
        ),
        ("context_label", label_json(&p.context_label)),
        (
            "self_report",
            p.self_report.map(|r| r.to_json()).unwrap_or(Json::Null),
        ),
        ("inputs", inputs_json(&p.inputs)),
        (
            "requested_grants",
            Json::Arr(
                p.requested_grants
                    .iter()
                    .map(|g| hh_hir::grant_json(g, false))
                    .collect(),
            ),
        ),
        ("containment", containment_json(&p.containment)),
        ("at", Json::Int(p.at as i64)),
    ])
}

fn proposal_from_json(j: &Json) -> Result<Proposal, String> {
    let requested_grants = match j.get("requested_grants") {
        Some(Json::Arr(items)) => items
            .iter()
            .enumerate()
            .map(|(i, g)| {
                hh_hir::grant_from_json(g, &format!("proposal.requested_grants[{i}]"))
                    .map_err(|e| format!("proposal.requested_grants[{i}]: {e}"))
            })
            .collect::<Result<Vec<Grant>, String>>()?,
        _ => Vec::new(),
    };
    Ok(Proposal {
        effect_id: req_str(j, "effect_id")?,
        attempt_no: req_int(j, "attempt_no")?.max(0) as u64,
        proposer: req_str(j, "proposer")?,
        capability_ref: pinned_ref_from_json(
            j.get("capability_ref")
                .ok_or_else(|| "proposal.capability_ref missing".to_string())?,
            "proposal.capability_ref",
        )?,
        effect: hh_hir::kinds::EffectClass::from_json(
            j.get("effect")
                .ok_or_else(|| "proposal.effect missing".to_string())?,
            "proposal.effect",
        )
        .map_err(|e| format!("proposal.effect: {e}"))?,
        surface_args: j
            .get("surface_args")
            .cloned()
            .ok_or_else(|| "proposal.surface_args missing".to_string())?,
        args_provenance: match j.get("args_provenance") {
            Some(Json::Null) | None => None,
            Some(r) => Some(
                hh_provenance::ProvenanceRecord::from_json(r)
                    .map_err(|e| format!("proposal.args_provenance: {e}"))?,
            ),
        },
        context_label: Label::from_json(
            j.get("context_label")
                .ok_or_else(|| "proposal.context_label missing".to_string())?,
        )
        .map_err(|e| format!("proposal.context_label: {e}"))?,
        self_report: match j.get("self_report") {
            Some(Json::Null) | None => None,
            Some(r) => Some(
                RiskClass::from_json(r)
                    .ok_or_else(|| "proposal.self_report malformed".to_string())?,
            ),
        },
        inputs: inputs_from_json(
            j.get("inputs")
                .ok_or_else(|| "proposal.inputs missing".to_string())?,
            "proposal.inputs",
        )?,
        requested_grants,
        containment: containment_from_json(
            j.get("containment")
                .ok_or_else(|| "proposal.containment missing".to_string())?,
            "proposal.containment",
        )?,
        at: req_int(j, "at")?.max(0) as u64,
    })
}
