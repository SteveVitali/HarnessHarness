//! Ensemble procedures (§5b.2 AC-R-2.3.2-10; ADR-0123) — **not** router
//! features: a budgeted control-plane `Procedure` whose `k` member calls are
//! each routed individually (one `model.route.decided` per member), one
//! reducer verdict, and a budget slice whose consumption equals the member
//! charges plus the reducer's. `run_ensemble` is pure over its inputs — the
//! member call and the reducer are injected ports; the router's `select`
//! runs per member inside.
//!
//! The procedure's own record is `control.ensemble.run{procedure_ref, k,
//! member_call_ids[], member_decision_refs[], reducer_verdict,
//! member_charges, reducer_charge, slice_consumption}` — every member's
//! `model_call_id` and decision ref are ledgered, and `slice_consumption`
//! is the sum, never a declared figure.

use std::collections::BTreeMap;

use hh_budget::quantity::ResourceVector;
use hh_compiler::profile::SelectorView;
use hh_gateway::router::{
    select_with, BudgetPort, HealthView, ModelRoleTable, RoutingDecision, RoutingPolicy,
    RoutingRefusal, RoutingRequest, RoutingViews,
};
use hh_wire::json::Json;
use std::collections::BTreeSet;

/// The ensemble procedure kind — the closed sum (§5b.2 names
/// `sample_k_vote`; further kinds are additive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnsembleKind {
    /// `sample_k_vote` — k member calls, a majority-vote reducer.
    SampleKVote,
}

impl EnsembleKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EnsembleKind::SampleKVote => "sample_k_vote",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<EnsembleKind> {
        match s {
            "sample_k_vote" => Some(EnsembleKind::SampleKVote),
            _ => None,
        }
    }
}

/// `EnsembleProcedure{sample_k_vote, k}` — the declared procedure.
#[derive(Debug, Clone, PartialEq)]
pub struct EnsembleProcedure {
    /// The procedure kind.
    pub kind: EnsembleKind,
    /// The member count.
    pub k: u32,
}

/// One member call's outcome — the routed decision, the member's output
/// payload (the vote), and the measured charge.
#[derive(Debug, Clone)]
pub struct MemberOutcome {
    /// The member's `model_call_id`.
    pub model_call_id: String,
    /// The member's `RoutingDecision` (its own `model.route.decided`).
    pub decision: RoutingDecision,
    /// The member's output payload — the reducer's input.
    pub output: String,
    /// The member's measured charge.
    pub charge: ResourceVector,
}

/// The member-executor port — one routed call's run: the request, its
/// routing decision, → `(output_payload, measured_charge)`.
pub type MemberExec<'a> =
    dyn FnMut(&RoutingRequest, &RoutingDecision) -> Result<(String, ResourceVector), String> + 'a;

/// The reducer port — `sample_k_vote`'s default is [`VoteReducer`]; a
/// verifier-gated reducer (a C2 `judge` oracle) implements the same seam.
pub trait EnsembleReducer {
    /// `reduce(outputs) → (verdict, charge)` — the verdict plus the
    /// reducer's own measured charge.
    fn reduce(&self, outputs: &[&str]) -> (String, ResourceVector);
}

/// `sample_k_vote`'s reducer — majority over member outputs; a tie resolves
/// to the earliest output in member order (deterministic — `BTreeMap`
/// ordering breaks it, member order preserves it). The tally is recorded
/// either way.
pub struct VoteReducer {
    /// The reducer call's measured charge.
    pub charge: ResourceVector,
}

impl EnsembleReducer for VoteReducer {
    fn reduce(&self, outputs: &[&str]) -> (String, ResourceVector) {
        let mut tally: BTreeMap<String, u32> = BTreeMap::new();
        for o in outputs {
            *tally.entry((*o).to_string()).or_insert(0) += 1;
        }
        // Majority, earliest-member-order on ties: scan outputs in order and
        // keep the first output reaching the maximal count.
        let mut best: Option<(&str, u32)> = None;
        for o in outputs {
            let c = tally[*o];
            if best.map(|(_, b)| c > b).unwrap_or(true) {
                best = Some((o, c));
            }
        }
        (
            best.map(|(o, _)| o.to_string()).unwrap_or_default(),
            self.charge.clone(),
        )
    }
}

/// The `run_ensemble` result.
#[derive(Debug, Clone)]
pub struct EnsembleOutcome {
    /// Exactly `k` member outcomes, in member order.
    pub members: Vec<MemberOutcome>,
    /// The reducer verdict.
    pub verdict: String,
    /// The vote tally the reducer counted.
    pub tally: BTreeMap<String, u32>,
    /// The reducer's own charge.
    pub reducer_charge: ResourceVector,
    /// `slice_consumption` — Σ member charges + `reducer_charge`.
    pub slice_consumption: ResourceVector,
}

impl EnsembleOutcome {
    /// The `control.ensemble.run` record payload.
    pub fn to_json(&self) -> Json {
        let mut tally = BTreeMap::new();
        for (o, c) in &self.tally {
            tally.insert(o.clone(), Json::Int(*c as i64));
        }
        let mut member_charges = ResourceVector::zero();
        for m in &self.members {
            member_charges.add_vec(&m.charge);
        }
        Json::obj([
            ("k", Json::Int(self.members.len() as i64)),
            (
                "member_call_ids",
                Json::Arr(
                    self.members
                        .iter()
                        .map(|m| Json::str(&m.model_call_id))
                        .collect(),
                ),
            ),
            (
                "member_decision_refs",
                Json::Arr(
                    self.members
                        .iter()
                        .map(|m| Json::str(&m.decision.decision_id))
                        .collect(),
                ),
            ),
            (
                "reducer_verdict",
                Json::obj([
                    ("verdict", Json::str(&self.verdict)),
                    ("tally", Json::Obj(tally)),
                    ("charge", self.reducer_charge.to_json()),
                ]),
            ),
            ("member_charges", member_charges.to_json()),
            ("slice_consumption", self.slice_consumption.to_json()),
        ])
    }
}

/// The closed ensemble refusal — never a silent short-fan-out.
#[derive(Debug, Clone, PartialEq)]
pub enum EnsembleRefusal {
    /// `k < 2`, `k > k_max`, or `k` exceeds the live fan-out cap — the
    /// procedure is inadmissible, not degraded.
    InvalidK {
        /// The requested member count.
        k: u32,
        /// The bound it violated.
        k_max: u32,
    },
    /// A member's `select` refused — the router's typed refusal rides up
    /// unchanged (R-2.3.2 typed-refusal discipline).
    SelectRefused {
        /// The member index that refused.
        member: usize,
        /// The `RoutingRefusal`.
        refusal: RoutingRefusal,
    },
    /// The member executor failed.
    MemberFailed {
        /// The member index.
        member: usize,
        /// The executor's typed reason.
        reason: String,
    },
}

/// `run_ensemble(procedure, fact, request, …, member, reducer, alloc)` —
/// AC-R-2.3.2-10's executor: exactly `k` members, each routed by `select_with`
/// under its own `model_call_id` and reservation (R-2 per member); one
/// reducer verdict; `slice_consumption` = Σ member charges + the reducer's.
/// `fact.k_max` bounds `k` — a procedure beyond the declared ceiling refuses
/// `InvalidK`, never a silent short fall.
#[allow(clippy::too_many_arguments)] // the arity is the contract's.
pub fn run_ensemble(
    procedure: &EnsembleProcedure,
    k_max: u32,
    request: &RoutingRequest,
    profile_env: &dyn SelectorView,
    account: &mut dyn BudgetPort,
    health: &dyn HealthView,
    views: &RoutingViews,
    policy: &RoutingPolicy,
    table: &ModelRoleTable,
    member: &mut MemberExec<'_>,
    reducer: &dyn EnsembleReducer,
    alloc: &mut dyn FnMut(&str) -> String,
    now_ms: u64,
) -> Result<EnsembleOutcome, EnsembleRefusal> {
    if procedure.k < 2 || procedure.k > k_max {
        return Err(EnsembleRefusal::InvalidK {
            k: procedure.k,
            k_max,
        });
    }
    let mut members = Vec::with_capacity(procedure.k as usize);
    for i in 0..procedure.k {
        let mut req = request.clone();
        req.model_call_id = Some(alloc("model_call"));
        let decision = select_with(
            &req,
            profile_env,
            account,
            health,
            views,
            policy,
            table,
            &alloc("decision"),
            now_ms,
            // Members are independent calls — a `sample_k_vote` may
            // legitimately route every member to the same target (the
            // attempted-set is `on_attempt_failed`'s, not the fan-out's).
            &BTreeSet::new(),
        )
        .map_err(|refusal| EnsembleRefusal::SelectRefused {
            member: i as usize,
            refusal,
        })?;
        let id = req.model_call_id.clone().expect("set above");
        let (output, charge) =
            member(&req, &decision).map_err(|reason| EnsembleRefusal::MemberFailed {
                member: i as usize,
                reason,
            })?;
        members.push(MemberOutcome {
            model_call_id: id,
            decision,
            output,
            charge,
        });
    }
    let outputs: Vec<&str> = members.iter().map(|m| m.output.as_str()).collect();
    let (verdict, reducer_charge) = reducer.reduce(&outputs);
    let mut tally: BTreeMap<String, u32> = BTreeMap::new();
    for o in &outputs {
        *tally.entry((*o).to_string()).or_insert(0) += 1;
    }
    let mut slice_consumption = reducer_charge.clone();
    for m in &members {
        slice_consumption.add_vec(&m.charge);
    }
    Ok(EnsembleOutcome {
        members,
        verdict,
        tally,
        reducer_charge,
        slice_consumption,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_compiler::profile::{
        self as prof, CapabilityState, DebtStatus, ExpiryCondition, ExpiryKind, ModelCoordinate,
        ModelProfile, ModelRole, ProfileCompatibility, ProfileDebtRecord, ProfileSelector,
        SelectorView, VersionPattern,
    };
    use hh_gateway::router::{
        MigrationLossBound, NoHealth, RoleBinding, RouteCandidate, RoutingPolicyKind,
    };
    use hh_gateway::ModelRef;
    use hh_ontology::dimensions::DimensionId;

    fn debt(status: DebtStatus) -> ProfileDebtRecord {
        ProfileDebtRecord {
            rule_id: "r.profile".into(),
            hypothesis: "h".into(),
            evidence_refs: vec![hh_compiler::profile::EvidenceRef::legacy("ev:1")],
            owner: "o".into(),
            reach_via: Vec::new(),
            expiry_condition: ExpiryCondition {
                kind: ExpiryKind::ModelVersionChange,
                value: None,
            },
            removal_test_ref: "t:1".into(),
            removal_test: Some(hh_compiler::profile::RemovalTest::new(
                hh_compiler::profile::RemovalTestKind::Documentation,
            )),
            status,
            debt_class: None,
            hypothesis_typed: None,
            scope: None,
            expiry: None,
            runway_ms: None,
            revalidation: None,
            created_at: None,
            supersedes: None,
        }
    }

    fn profile() -> ModelProfile {
        ModelProfile {
            profile_id: "prof.test".into(),
            version: "1".into(),
            content_hash: "hash-prof.test-1".into(),
            selector: ProfileSelector {
                provider_api_family: "fam".into(),
                model_family: "mod".into(),
                version_pattern: VersionPattern::Exact("v1".into()),
                precedence: 0,
                successor_ref: None,
                retirement_at: None,
                roles_admitted: vec![ModelRole::Primary],
            },
            extends: None,
            capabilities: prof::ProfileCapabilities {
                native_function_calling: CapabilityState::Declared,
                max_output: Some(8_192),
                ..prof::ProfileCapabilities::default()
            },
            rules: vec![],
            ext: BTreeMap::new(),
            expiry: debt(DebtStatus::Active),
            compatibility: ProfileCompatibility {
                inventory_version: "1".into(),
                min_compiler_version: "0.0.0".into(),
            },
            tests: Json::Null,
        }
    }

    struct Env(Vec<ModelProfile>);
    impl prof::ProfileView for Env {
        fn profile(&self, coordinate: &str) -> Option<ModelProfile> {
            self.0
                .iter()
                .find(|p| prof::profile_coordinate(p) == coordinate)
                .cloned()
        }
    }
    impl SelectorView for Env {
        fn registered(&self) -> Vec<ModelProfile> {
            self.0.clone()
        }
    }

    struct FakeBudget;
    impl BudgetPort for FakeBudget {
        fn reserve(&mut self, _b: &str, _m: u64, _h: &str) -> Result<String, String> {
            Ok("res".into())
        }
        fn release(&mut self, _r: &str) -> Result<(), String> {
            Ok(())
        }
    }

    fn candidate() -> RouteCandidate {
        RouteCandidate {
            model_ref: ModelRef {
                profile_ref: "prof.test@1".into(),
                provider_model_id: "m-1".into(),
                snapshot_id: None,
                serving_route: Some("route-a".into()),
                effort: None,
            },
            coordinate: ModelCoordinate {
                provider_api_family: "fam".into(),
                model_family: "mod".into(),
                model_version: "v1".into(),
            },
        }
    }

    fn table() -> ModelRoleTable {
        ModelRoleTable {
            roles: BTreeMap::from([(
                "primary".to_string(),
                RoleBinding {
                    primary: candidate(),
                    alternates: vec![],
                    policy_ref: "policy.test@1".into(),
                    profile_ref: "prof.test@1".into(),
                },
            )]),
        }
    }

    fn policy() -> RoutingPolicy {
        RoutingPolicy {
            policy_id: "policy.test".into(),
            version: "1".into(),
            content_hash: "h.policy".into(),
            role_scope: BTreeSet::new(),
            kind: RoutingPolicyKind::RoleTable,
            params: Json::obj([]),
            error_actions: BTreeMap::new(),
            max_migration_loss: MigrationLossBound {
                dropped_items: 0,
                no_in_flight_tool_call: true,
            },
            allow_unknown: None,
            conditioned_rules: vec![],
            ext: BTreeMap::new(),
        }
    }

    fn request() -> RoutingRequest {
        RoutingRequest {
            role: "primary".into(),
            required_capabilities: vec!["native_function_calling".into()],
            budget_id: "b-1".into(),
            holder: "run:1".into(),
            model_call_id: None,
            intent_ref: None,
            effort: None,
            latency_target_ms: None,
            quality_prior_ref: None,
            preferences: None,
            source_profile_ref: None,
            in_flight_effect: false,
        }
    }

    /// AC-R-2.3.2-10 — `EnsembleProcedure{sample_k_vote, k}` produces exactly
    /// k `model_call_id`s each with its own `model.route.decided`, one
    /// reducer verdict, and `slice_consumption = Σ member + reducer`.
    #[test]
    fn ensemble_runs_k_routed_calls_one_verdict_one_slice() {
        let proc = EnsembleProcedure {
            kind: EnsembleKind::SampleKVote,
            k: 3,
        };
        let env = Env(vec![profile()]);
        let mut n = 0u64;
        let mut alloc = |kind: &str| {
            n += 1;
            format!("{kind}-{n}")
        };
        let member_charge = ResourceVector::one(DimensionId::TokensOutputVisible, 100);
        let reducer_charge = ResourceVector::one(DimensionId::TokensOutputVisible, 20);
        // Members vote a, b, a → verdict a.
        let votes = ["a", "b", "a"];
        let mut vi = 0usize;
        let mut member = |_: &RoutingRequest, _: &RoutingDecision| {
            let v = votes[vi].to_string();
            vi += 1;
            Ok((v, member_charge.clone()))
        };
        let reducer = VoteReducer {
            charge: reducer_charge.clone(),
        };
        let out = run_ensemble(
            &proc,
            5,
            &request(),
            &env,
            &mut FakeBudget,
            &NoHealth,
            &RoutingViews::none(),
            &policy(),
            &table(),
            &mut member,
            &reducer,
            &mut alloc,
            0,
        )
        .expect("runs");
        assert_eq!(out.members.len(), 3);
        // Distinct call ids, one decision each.
        let ids: BTreeSet<&str> = out
            .members
            .iter()
            .map(|m| m.model_call_id.as_str())
            .collect();
        assert_eq!(ids.len(), 3);
        assert!(out
            .members
            .iter()
            .all(|m| m.decision.selected.provider_model_id == "m-1"
                && m.decision.reservation_id.is_some()));
        // Each member carries its own `model.route.decided` payload.
        let decided: Vec<Json> = out
            .members
            .iter()
            .map(|m| hh_gateway::events::route_decided(&m.decision))
            .collect();
        assert_eq!(decided.len(), 3);
        let decided_ids: BTreeSet<String> = decided
            .iter()
            .filter_map(|d| {
                d.get("decision_id")
                    .and_then(Json::as_str)
                    .map(str::to_string)
            })
            .collect();
        assert_eq!(decided_ids.len(), 3);
        // One reducer verdict — the majority.
        assert_eq!(out.verdict, "a");
        assert_eq!(out.tally.get("a"), Some(&2));
        assert_eq!(out.tally.get("b"), Some(&1));
        // The slice is the sum — never a declared figure.
        let mut expected = reducer_charge.clone();
        for _ in 0..3 {
            expected.add_vec(&member_charge);
        }
        assert_eq!(out.slice_consumption, expected);
    }

    /// `k` outside `[2, k_max]` refuses `InvalidK` — never a silent
    /// short-fan-out or an unbudgeted run.
    #[test]
    fn ensemble_k_bounds_refuse() {
        let env = Env(vec![profile()]);
        let mut alloc = |kind: &str| kind.to_string();
        let mut member =
            |_: &RoutingRequest, _: &RoutingDecision| Ok(("a".to_string(), ResourceVector::zero()));
        let reducer = VoteReducer {
            charge: ResourceVector::zero(),
        };
        for (k, k_max) in [(1, 5), (6, 5)] {
            let r = run_ensemble(
                &EnsembleProcedure {
                    kind: EnsembleKind::SampleKVote,
                    k,
                },
                k_max,
                &request(),
                &env,
                &mut FakeBudget,
                &NoHealth,
                &RoutingViews::none(),
                &policy(),
                &table(),
                &mut member,
                &reducer,
                &mut alloc,
                0,
            );
            assert!(
                matches!(r, Err(EnsembleRefusal::InvalidK { .. })),
                "{k}/{k_max}"
            );
        }
    }

    /// A member's typed `RoutingRefusal` surfaces unchanged — never a
    /// silent member skip.
    #[test]
    fn ensemble_member_refusal_is_typed() {
        // A request no profile can serve (unknown capability axis) — the
        // member select refuses `capability_unknown` on member 0.
        let mut req = request();
        req.required_capabilities = vec!["nonexistent_axis".into()];
        let env = Env(vec![profile()]);
        let mut alloc = |kind: &str| kind.to_string();
        let mut member =
            |_: &RoutingRequest, _: &RoutingDecision| Ok(("a".to_string(), ResourceVector::zero()));
        let reducer = VoteReducer {
            charge: ResourceVector::zero(),
        };
        let r = run_ensemble(
            &EnsembleProcedure {
                kind: EnsembleKind::SampleKVote,
                k: 3,
            },
            5,
            &req,
            &env,
            &mut FakeBudget,
            &NoHealth,
            &RoutingViews::none(),
            &policy(),
            &table(),
            &mut member,
            &reducer,
            &mut alloc,
            0,
        );
        match r {
            Err(EnsembleRefusal::SelectRefused { member, refusal }) => {
                assert_eq!(member, 0);
                assert!(matches!(
                    refusal,
                    RoutingRefusal::CapabilityUnknown { .. }
                        | RoutingRefusal::ChainExhausted { .. }
                ));
            }
            other => panic!("expected SelectRefused, got {other:?}"),
        }
    }
}
