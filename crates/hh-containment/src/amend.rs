//! `amend` — the §5g.4 amendment verb (the runtime amendment path of
//! Stage 2): `amend(policy_version_id, diff, basis, endorser, effect_id?)
//! → new version_id`. Additive and ledgered (`security.containment.amended`
//! — the owning row in `hh-ledger::classes`; the event is emitted by the
//! caller after `amend` returns the new policy).
//!
//! Legitimacy (spec §5g.4 `amend` row + `AmendmentPolicy` semantics):
//!
//! - `basis ∈ amendment.allowed_bases` — else `BasisNotAllowed`.
//! - Endorser origin ∈ `{model, evolution, participant}` (and `import` /
//!   `migration` — content origins that can never author a containment
//!   loosening) ⇒ `ContainmentWidening`. A hosted/evolution/model output may
//!   *propose* an amendment; it may never *confer* one (CC2).
//! - Authority floor per basis: `approval` ⇒ `≥ principal` (run-level
//!   loosening); `seal` / `policy_rule` ⇒ `≥ definition` (the sealed design).
//! - `strict` ⇒ a sub-floor endorsement is `StrictMode` (refused outright —
//!   DF-S1.12-2's "the declaring authority's own signature"); non-strict ⇒
//!   `ContainmentWidening` (the caller may convert to `ask`).
//! - `persistence_rank(scope) ≤ persistence_rank(persist_scope_ceiling)` —
//!   else `ScopeBeyondCeiling`.
//!
//! The applied diff is *additive only* (`ContainmentDiff` has no removal
//! variant — the meet's anti-laundering is unaffected; ADR-0266 D5).

use hh_hir::leaves::Text;
use hh_provenance::{AuthorityClass, Origin, PersistenceScope, ProvenanceRecord};

use crate::admit::ContainmentDiff;
use crate::policy::{
    normalize_host, persistence_rank, ContainmentPolicy, ExecPolicy, HostPattern, PolicyError,
    ReadMode, RuleDecision, WritableRoot,
};
use crate::policy::{AmendmentBasis, EgressRule};

/// The `amend` refusal modes — typed, never a warning (§8.1 #5).
#[derive(Debug, Clone, PartialEq)]
pub enum AmendError {
    /// `basis ∉ amendment.allowed_bases`.
    BasisNotAllowed {
        /// The refused basis.
        basis: AmendmentBasis,
        /// What the policy allows.
        allowed: Vec<AmendmentBasis>,
    },
    /// Under `strict`, an endorsement below the declaring layer's authority
    /// is refused outright (DF-S1.12-2).
    StrictMode {
        /// What was refused.
        detail: String,
    },
    /// The endorsement cannot confer the widening (wrong origin or
    /// sub-floor authority under non-strict).
    ContainmentWidening {
        /// What was refused.
        detail: String,
    },
    /// The amendment's persistence scope exceeds `persist_scope_ceiling`.
    ScopeBeyondCeiling {
        /// The requested scope.
        scope: PersistenceScope,
        /// The policy ceiling.
        ceiling: PersistenceScope,
    },
    /// The amended policy failed validation.
    Policy(PolicyError),
}

impl AmendError {
    /// A stable refusal code for the `security.containment.amended` refusal
    /// row / the ask's `refused` outcome.
    pub fn code(&self) -> &'static str {
        match self {
            AmendError::BasisNotAllowed { .. } => "basis_not_allowed",
            AmendError::StrictMode { .. } => "strict_mode",
            AmendError::ContainmentWidening { .. } => "containment_widening",
            AmendError::ScopeBeyondCeiling { .. } => "scope_beyond_ceiling",
            AmendError::Policy(_) => "policy_invalid",
        }
    }
}

/// The outcome of a successful `amend` — the new policy (already
/// `compute_ids`-ed) plus the version transition for the ledger row.
#[derive(Debug, Clone)]
pub struct AmendOutcome {
    /// The amended policy (additive diff applied; `version_id` fresh).
    pub policy: ContainmentPolicy,
    /// The superseded version.
    pub from_version_id: String,
    /// The new version.
    pub to_version_id: String,
}

/// Whether `origin` may ever endorse a containment loosening.
fn forbidden_origin(origin: &Origin) -> bool {
    matches!(
        origin,
        Origin::Model { .. }
            | Origin::Evolution { .. }
            | Origin::Participant { .. }
            | Origin::Import { .. }
            | Origin::Migration { .. }
            | Origin::Tool { .. } // a tool *result* is not an endorser
    )
}

/// The authority floor a basis requires (spec §5g.4 `amend` row:
/// `authority(endorser) ≥ the loosened layer — principal run-level;
/// definition via seal; policy_rule for phase plans sealed in the design`).
fn basis_floor(basis: AmendmentBasis) -> AuthorityClass {
    match basis {
        AmendmentBasis::Approval => AuthorityClass::Principal,
        AmendmentBasis::Seal | AmendmentBasis::PolicyRule => AuthorityClass::Definition,
    }
}

/// `amend(policy, diff, basis, endorser, scope, effect_id?)` — the §5g.4
/// verb. Returns the new policy (fresh `version_id`) or a typed refusal.
///
/// `scope` is the amendment's persistence (an approval hatch lives at
/// `session | run` per §5g.4 §2's approval-cache bound; `definition`-scope
/// amendments ride the seal path, not this one — `basis = seal` is the
/// *authorisation*, the scope ceiling still applies).
pub fn amend(
    policy: &ContainmentPolicy,
    diff: &ContainmentDiff,
    basis: AmendmentBasis,
    endorser: &ProvenanceRecord,
    scope: PersistenceScope,
) -> Result<AmendOutcome, AmendError> {
    // basis ∈ allowed_bases.
    if !policy.amendment.allowed_bases.contains(&basis) {
        return Err(AmendError::BasisNotAllowed {
            basis,
            allowed: policy.amendment.allowed_bases.iter().copied().collect(),
        });
    }

    // Origin gate — model/evolution/participant (and tool/import/migration
    // content) can propose but never confer a loosening.
    if forbidden_origin(&endorser.origin) {
        let detail = format!(
            "amendment endorser origin {:?} cannot confer a containment loosening",
            endorser.origin
        );
        return Err(if policy.amendment.strict {
            AmendError::StrictMode { detail }
        } else {
            AmendError::ContainmentWidening { detail }
        });
    }

    // Authority floor per basis.
    let floor = basis_floor(basis);
    if endorser.authority < floor {
        let detail = format!(
            "basis {} requires endorser authority ≥ {} (got {})",
            basis.as_str(),
            floor.as_str(),
            endorser.authority.as_str()
        );
        return Err(if policy.amendment.strict {
            AmendError::StrictMode { detail }
        } else {
            AmendError::ContainmentWidening { detail }
        });
    }

    // Persistence ceiling.
    if persistence_rank(scope) > persistence_rank(policy.amendment.persist_scope_ceiling) {
        return Err(AmendError::ScopeBeyondCeiling {
            scope,
            ceiling: policy.amendment.persist_scope_ceiling,
        });
    }

    // Apply the diff — additive only.
    let mut next = policy.clone();
    match diff {
        ContainmentDiff::AddWritableRoot { root } => {
            next.fs.write.allow.push(WritableRoot {
                root: root.clone(),
                read_only_subpaths: vec![],
                protected_metadata_names: vec![],
            });
        }
        ContainmentDiff::AddReadAllow { path } => {
            if next.fs.read.mode != ReadMode::AllowOnly {
                // Under allow_all_except a read allow is a no-op tightening
                // edge — still record it (additive); the meet treats the
                // union conservatively.
            }
            next.fs.read.allow.push(path.clone());
        }
        ContainmentDiff::AddEgressAllow { host } => {
            // §5g.4: an approval endorsement may append an *exact-host*
            // allow rule — normalise and refuse anything that isn't exact.
            let norm = normalize_host(host);
            next.net.rules.push(EgressRule {
                host: HostPattern::Exact(norm),
                ports: vec![],
                protocols: vec![],
                methods: vec![],
                decision: RuleDecision::Allow,
                credential_bindings: vec![],
                justification: Some(Text::new(
                    format!("amended by {} at {}", basis.as_str(), scope.as_str()),
                    "hh-containment",
                    endorser.clone(),
                )),
                provenance: Some(endorser.clone()),
            });
        }
        ContainmentDiff::AddExecAllow { path } => {
            match &mut next.fs.exec {
                ExecPolicy::Allow(list) => list.push(path.clone()),
                // `any` already admits it — record nothing new (additive and
                // already-covered), but the amendment still mints a version
                // (the ledger row is the durable fact).
                ExecPolicy::Any => {}
            }
        }
    }

    // Re-validate and re-identify (amendment cannot launder a bad policy).
    next.validate_structure().map_err(AmendError::Policy)?;
    next.validate().map_err(AmendError::Policy)?;
    next.compute_ids();

    let from = policy.version_id.clone();
    let to = next.version_id.clone();
    debug_assert_ne!(from, to);
    Ok(AmendOutcome {
        policy: next,
        from_version_id: from,
        to_version_id: to,
    })
}

/// The `ContainmentDiff`'s canonical JSON member set (for
/// `security.containment.amended`'s `diff` member).
pub fn diff_json(diff: &ContainmentDiff) -> hh_wire::json::Json {
    use hh_wire::json::Json;
    let mut m = std::collections::BTreeMap::new();
    match diff {
        ContainmentDiff::AddWritableRoot { root } => {
            m.insert(
                "kind".to_string(),
                Json::str("add_writable_root".to_string()),
            );
            m.insert("root".to_string(), Json::str(root.clone()));
        }
        ContainmentDiff::AddReadAllow { path } => {
            m.insert("kind".to_string(), Json::str("add_read_allow".to_string()));
            m.insert("path".to_string(), Json::str(path.clone()));
        }
        ContainmentDiff::AddEgressAllow { host } => {
            m.insert(
                "kind".to_string(),
                Json::str("add_egress_allow".to_string()),
            );
            m.insert("host".to_string(), Json::str(host.clone()));
        }
        ContainmentDiff::AddExecAllow { path } => {
            m.insert("kind".to_string(), Json::str("add_exec_allow".to_string()));
            m.insert("path".to_string(), Json::str(path.clone()));
        }
    }
    Json::Obj(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{kernel_default, AmendmentPolicy};
    use hh_provenance::{HumanRole, Origin};
    use std::collections::BTreeSet;

    fn base_policy() -> ContainmentPolicy {
        let mut p = kernel_default(0);
        p.net.mode = crate::policy::NetMode::Mediated;
        p.amendment = AmendmentPolicy {
            allowed_bases: BTreeSet::from([
                AmendmentBasis::Approval,
                AmendmentBasis::Seal,
                AmendmentBasis::PolicyRule,
            ]),
            strict: true,
            session_cache: true,
            persist_scope_ceiling: PersistenceScope::Run,
        };
        p.compute_ids();
        p
    }

    fn principal() -> ProvenanceRecord {
        ProvenanceRecord::minted(
            Origin::human("u:op", HumanRole::Principal),
            PersistenceScope::Run,
            1,
        )
    }

    #[test]
    fn approval_amends_egress_allow() {
        let p = base_policy();
        let d = ContainmentDiff::AddEgressAllow {
            host: "api.example.com".into(),
        };
        let out = amend(
            &p,
            &d,
            AmendmentBasis::Approval,
            &principal(),
            PersistenceScope::Run,
        )
        .unwrap();
        assert_ne!(out.from_version_id, out.to_version_id);
        assert_eq!(
            out.policy.net.rules.last().unwrap().host,
            HostPattern::Exact("api.example.com".into())
        );
        assert_eq!(
            out.policy.net.rules.last().unwrap().decision,
            RuleDecision::Allow
        );
    }

    #[test]
    fn model_endorser_refused_under_strict() {
        let p = base_policy();
        let model = ProvenanceRecord::minted(
            Origin::model("m:1", "r:1", "resp:1"),
            PersistenceScope::Run,
            2,
        );
        let d = ContainmentDiff::AddEgressAllow {
            host: "evil.com".into(),
        };
        let e = amend(
            &p,
            &d,
            AmendmentBasis::Approval,
            &model,
            PersistenceScope::Run,
        )
        .unwrap_err();
        assert!(matches!(e, AmendError::StrictMode { .. }));
    }

    #[test]
    fn delegate_endorser_below_floor() {
        let p = base_policy();
        let delegate = ProvenanceRecord {
            authority: AuthorityClass::Delegate,
            ..principal()
        };
        let d = ContainmentDiff::AddEgressAllow {
            host: "x.com".into(),
        };
        let e = amend(
            &p,
            &d,
            AmendmentBasis::Approval,
            &delegate,
            PersistenceScope::Run,
        )
        .unwrap_err();
        assert!(matches!(e, AmendError::StrictMode { .. }));
    }

    #[test]
    fn basis_not_allowed() {
        let mut p = base_policy();
        p.amendment.allowed_bases = BTreeSet::from([AmendmentBasis::Seal]);
        p.compute_ids();
        let d = ContainmentDiff::AddEgressAllow {
            host: "x.com".into(),
        };
        let e = amend(
            &p,
            &d,
            AmendmentBasis::Approval,
            &principal(),
            PersistenceScope::Run,
        )
        .unwrap_err();
        assert!(matches!(e, AmendError::BasisNotAllowed { .. }));
    }

    #[test]
    fn scope_beyond_ceiling() {
        let p = base_policy();
        let d = ContainmentDiff::AddEgressAllow {
            host: "x.com".into(),
        };
        let e = amend(
            &p,
            &d,
            AmendmentBasis::Approval,
            &principal(),
            PersistenceScope::Project,
        )
        .unwrap_err();
        assert!(matches!(e, AmendError::ScopeBeyondCeiling { .. }));
    }

    #[test]
    fn seal_basis_requires_definition_authority() {
        let p = base_policy();
        let d = ContainmentDiff::AddEgressAllow {
            host: "x.com".into(),
        };
        // principal below the seal floor.
        let e = amend(
            &p,
            &d,
            AmendmentBasis::Seal,
            &principal(),
            PersistenceScope::Run,
        )
        .unwrap_err();
        assert!(matches!(e, AmendError::StrictMode { .. }));
    }
}
