//! Group L — `lab.experiment.*` dispatch (S3.4a; R-2.10.3⁰ᵇ). The boundary is
//! records-in/records-out like `lab.eval.*`: the caller supplies the spec and
//! the resolver material the engine's context-parameterized checks need
//! (`budgets{ref → BudgetSpec}`, `suite_tasks[]`, `arm_configs{arm_id →
//! {configuration_id, configuration_version_id}}`, `ineligible_levels`,
//! `fingerprints`, `drifted_capabilities`, `sealed_artifacts`,
//! `min_replicates`, `retirement_diff`). What the params do not supply, the
//! engine resolves against `LabDocs` (`<store_root>/lab_docs` — `register`
//! deposits the `budgets{}` map there as named docs so later ops resolve the
//! same refs without resupplying them).
//!
//! The boundary never guesses: an absent resolver surfaces as the engine's
//! `Unresolvable` (e.g. `expand` without `suite_tasks`), and the closed E-1
//! refusal set renders verbatim as `Refused{reason: <code>}` (T-LCD-14).
//!
//! Writer leases persist across calls in `self.experiment_engines`
//! (`experiment_run_id → Lease`); a restart re-acquires through the normal
//! fence (ADR-0130).

use std::collections::{BTreeMap, BTreeSet};

use hh_budget::spec::BudgetSpec;
use hh_budget::{BudgetEnforcement, DimensionId, EnforcementLevel};
use hh_experiment::docs::{kind as doc_kind, LabDocs};
use hh_experiment::engine::{
    ClaimTicket, EngineContext, ExperimentEngine, HostedLaunchOutcome, HostedLaunchRequest,
    NextVerdict,
};
use hh_experiment::errors::ExperimentError;
use hh_experiment::events::PauseReason;
use hh_lab::expand::{ArmConfiguration, ExpandError, ExpandTask};
use hh_lab::experiment::{ArmSpec, BudgetRelevantParam, ExperimentSpec};
use hh_ledger::store::{Lease, Store};
use hh_ontology::compliance::NaReason;
use hh_ontology::lab::SplitLabel;
use hh_wire::json::Json;

use crate::service::EmbedService;
use hh_embed_schema::errors::EmbedError;

fn bad(path: &str, code: &str) -> EmbedError {
    EmbedError::SchemaViolation {
        path: path.to_string(),
        code: code.to_string(),
    }
}

fn req<'a>(j: &'a Json, k: &str) -> Result<&'a Json, EmbedError> {
    j.get(k)
        .ok_or_else(|| bad(&format!("/{k}"), "missing_field"))
}

fn req_str<'a>(j: &'a Json, k: &str) -> Result<&'a str, EmbedError> {
    req(j, k)?
        .as_str()
        .ok_or_else(|| bad(&format!("/{k}"), "type_mismatch"))
}

fn opt_str(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_string)
}

/// `ExperimentError` → the boundary's typed surface — the refusal set renders
/// as `Refused{reason: <code>}`; `WouldBlock`/`InsufficientBudget` map to
/// their native variants; everything else is `Refused` with the stable code.
fn xerr(e: ExperimentError) -> EmbedError {
    match e {
        ExperimentError::WouldBlock { holder } => EmbedError::WouldBlock {
            active_holder: holder,
        },
        ExperimentError::InsufficientBudget { .. } | ExperimentError::Budget(_) => {
            EmbedError::InsufficientBudget { dimension: None }
        }
        other => EmbedError::Refused {
            reason: other.code().to_string(),
        },
    }
}

/// The resolver bag `EngineContext`'s closures own — every member decoded
/// from `params` (records-in; absent members defer to `LabDocs`/refuse).
struct Bag {
    budgets: BTreeMap<String, BudgetSpec>,
    suite_tasks: Option<Vec<ExpandTask>>,
    arm_configs: Option<BTreeMap<String, ArmConfiguration>>,
    ineligible: BTreeMap<String, NaReason>,
    fingerprints: BTreeMap<String, String>,
    drifted: BTreeSet<String>,
    sealed: Option<BTreeSet<String>>,
    retirement_diff: Option<bool>,
    min_replicates: u32,
    /// `arm_id → BudgetEnforcement` — the E-1 `matched_cap` enforcement
    /// check's per-dimension view (`{arm → {dimension → level}}`; absent
    /// member = the check defers, per the engine's resolver contract).
    enforcement: Option<BTreeMap<String, BudgetEnforcement>>,
    /// `level_ref → {param → {value, affects[]}}` — the AC-R-2.10.2-12
    /// `budget_relevant` coverage check's variant projection.
    budget_params: Option<BTreeMap<String, BTreeMap<String, BudgetRelevantParam>>>,
    /// `arm_id → cache_state_visible` — the arm's bound dialect visibility
    /// (AC-R-2.3.4-10; absent member = the check defers).
    cache_visibility: Option<BTreeMap<String, bool>>,
    /// `participant ref → registry descriptor body` — the hosted-level
    /// admissibility check's capability vector source (AC-R-2.10.3-3).
    participant_descriptors: BTreeMap<String, Json>,
    /// `environment level ref → environment class` — the
    /// `environment_class:<class>` pool membership resolver.
    environment_classes: BTreeMap<String, String>,
    /// `hosted_session{session_ref?, hosting_mechanism, limits_enforced}` —
    /// the adapter's session report (the caller ran the Hosting ABI verbs
    /// on its plane and reports the outcome; AC-R-2.10.3-14). `error`
    /// carries a session-open failure the launch must refuse with.
    hosted_session: Option<HostedLaunchOutcome>,
    /// `hosted_session.error` — the session-open failure.
    hosted_session_error: Option<String>,
}

impl Bag {
    fn from_params(p: &Json) -> Result<Bag, EmbedError> {
        let mut budgets = BTreeMap::new();
        if let Some(Json::Obj(m)) = p.get("budgets") {
            for (k, v) in m {
                budgets.insert(
                    k.clone(),
                    BudgetSpec::from_json(v)
                        .ok_or_else(|| bad(&format!("/budgets/{k}"), "type_mismatch"))?,
                );
            }
        }
        let suite_tasks = match p.get("suite_tasks") {
            Some(Json::Arr(items)) => Some(
                items
                    .iter()
                    .map(|t| {
                        let task_id = t
                            .get("task_id")
                            .and_then(Json::as_str)
                            .ok_or_else(|| bad("/suite_tasks.task_id", "missing_field"))?;
                        let split = t
                            .get("split_label")
                            .and_then(Json::as_str)
                            .and_then(SplitLabel::parse)
                            .ok_or_else(|| bad("/suite_tasks.split_label", "type_mismatch"))?;
                        Ok(ExpandTask {
                            task_id: task_id.to_string(),
                            split_label: split,
                        })
                    })
                    .collect::<Result<Vec<_>, EmbedError>>()?,
            ),
            Some(_) => return Err(bad("/suite_tasks", "type_mismatch")),
            None => None,
        };
        let arm_configs = match p.get("arm_configs") {
            Some(Json::Obj(m)) => {
                let mut out = BTreeMap::new();
                for (k, v) in m {
                    let cid = v
                        .get("configuration_id")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad(&format!("/arm_configs/{k}"), "missing_field"))?;
                    let cvid = v
                        .get("configuration_version_id")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad(&format!("/arm_configs/{k}"), "missing_field"))?;
                    out.insert(
                        k.clone(),
                        ArmConfiguration {
                            configuration_id: cid.to_string(),
                            configuration_version_id: cvid.to_string(),
                        },
                    );
                }
                Some(out)
            }
            Some(_) => return Err(bad("/arm_configs", "type_mismatch")),
            None => None,
        };
        let mut ineligible = BTreeMap::new();
        if let Some(Json::Obj(m)) = p.get("ineligible_levels") {
            for (k, v) in m {
                let reason = v
                    .as_str()
                    .and_then(NaReason::parse)
                    .ok_or_else(|| bad(&format!("/ineligible_levels/{k}"), "type_mismatch"))?;
                ineligible.insert(k.clone(), reason);
            }
        }
        let mut fingerprints = BTreeMap::new();
        if let Some(Json::Obj(m)) = p.get("fingerprints") {
            for (k, v) in m {
                if let Some(fp) = v.as_str() {
                    fingerprints.insert(k.clone(), fp.to_string());
                }
            }
        }
        let mut drifted = BTreeSet::new();
        if let Some(Json::Arr(items)) = p.get("drifted_capabilities") {
            for i in items {
                if let Some(s) = i.as_str() {
                    drifted.insert(s.to_string());
                }
            }
        }
        let sealed = match p.get("sealed_artifacts") {
            Some(Json::Arr(items)) => Some(
                items
                    .iter()
                    .filter_map(Json::as_str)
                    .map(str::to_string)
                    .collect(),
            ),
            Some(_) => return Err(bad("/sealed_artifacts", "type_mismatch")),
            None => None,
        };
        let retirement_diff = p.get("retirement_diff").and_then(|v| match v {
            Json::Bool(b) => Some(*b),
            _ => None,
        });
        let min_replicates = p
            .get("min_replicates")
            .and_then(Json::as_int)
            .map(|v| v.max(1) as u32)
            .unwrap_or(1);
        // `{arm_id → {dimension → enforced|advisory|unenforceable}}` — the
        // E-1 matched-dimension enforcement map (ADR-0165 D3).
        let mut enforcement = None;
        if let Some(v) = p.get("budget_enforcement") {
            let Json::Obj(m) = v else {
                return Err(bad("/budget_enforcement", "type_mismatch"));
            };
            let mut out = BTreeMap::new();
            for (arm, levels) in m {
                let Json::Obj(lm) = levels else {
                    return Err(bad(&format!("/budget_enforcement/{arm}"), "type_mismatch"));
                };
                let mut pairs = Vec::new();
                for (d, l) in lm {
                    let dim = DimensionId::parse(d).ok_or_else(|| {
                        bad(&format!("/budget_enforcement/{arm}/{d}"), "type_mismatch")
                    })?;
                    let level = match l.as_str() {
                        Some("enforced") => EnforcementLevel::Enforced,
                        Some("advisory") => EnforcementLevel::Advisory,
                        Some("unenforceable") => EnforcementLevel::Unenforceable,
                        _ => {
                            return Err(bad(
                                &format!("/budget_enforcement/{arm}/{d}"),
                                "type_mismatch",
                            ))
                        }
                    };
                    pairs.push((dim, level));
                }
                out.insert(arm.clone(), BudgetEnforcement::hosted(&pairs));
            }
            enforcement = Some(out);
        }
        // `{level_ref → {param → {value, affects[]}}}` — the variants'
        // `budget_relevant` projections (AC-R-2.10.2-12).
        let mut budget_params = None;
        if let Some(v) = p.get("budget_relevant_params") {
            let Json::Obj(m) = v else {
                return Err(bad("/budget_relevant_params", "type_mismatch"));
            };
            let mut out = BTreeMap::new();
            for (lref, params) in m {
                let Json::Obj(pm) = params else {
                    return Err(bad(
                        &format!("/budget_relevant_params/{lref}"),
                        "type_mismatch",
                    ));
                };
                let mut pmap = BTreeMap::new();
                for (name, pv) in pm {
                    let value = pv.get("value").cloned().unwrap_or(Json::Null);
                    let affects = match pv.get("affects") {
                        Some(Json::Arr(items)) => {
                            let mut ds = Vec::new();
                            for d in items {
                                let s = d.as_str().ok_or_else(|| {
                                    bad(
                                        &format!("/budget_relevant_params/{lref}/{name}/affects"),
                                        "type_mismatch",
                                    )
                                })?;
                                ds.push(DimensionId::parse(s).ok_or_else(|| {
                                    bad(
                                        &format!("/budget_relevant_params/{lref}/{name}/affects"),
                                        "type_mismatch",
                                    )
                                })?);
                            }
                            ds
                        }
                        _ => {
                            return Err(bad(
                                &format!("/budget_relevant_params/{lref}/{name}/affects"),
                                "type_mismatch",
                            ))
                        }
                    };
                    pmap.insert(name.clone(), BudgetRelevantParam { value, affects });
                }
                out.insert(lref.clone(), pmap);
            }
            budget_params = Some(out);
        }
        // `{arm_id → cache_state_visible}` — each arm's bound dialect's
        // visibility bit (AC-R-2.3.4-10).
        let mut cache_visibility = None;
        if let Some(v) = p.get("cache_visibility") {
            let Json::Obj(m) = v else {
                return Err(bad("/cache_visibility", "type_mismatch"));
            };
            let mut out = BTreeMap::new();
            for (arm, vis) in m {
                let Json::Bool(b) = vis else {
                    return Err(bad(&format!("/cache_visibility/{arm}"), "type_mismatch"));
                };
                out.insert(arm.clone(), *b);
            }
            cache_visibility = Some(out);
        }
        // `{participant_ref → descriptor}` — the registry's participant
        // records (the hosted-level capability vector source).
        let mut participant_descriptors = BTreeMap::new();
        if let Some(Json::Obj(m)) = p.get("participant_descriptors") {
            for (k, v) in m {
                participant_descriptors.insert(k.clone(), v.clone());
            }
        }
        // `{environment_level_ref → environment_class}` — the pool
        // membership map (AC-R-2.10.3-10).
        let mut environment_classes = BTreeMap::new();
        if let Some(Json::Obj(m)) = p.get("environment_classes") {
            for (k, v) in m {
                if let Some(c) = v.as_str() {
                    environment_classes.insert(k.clone(), c.to_string());
                }
            }
        }
        // `hosted_session{session_ref?, hosting_mechanism, limits_enforced}`
        // — the Hosting ABI session verbs' report (the caller ran them on
        // its plane; the engine stamps the reported outcome, never a
        // guessed one). `hosted_session.error` is the session-open failure.
        let hosted_session_error = p
            .get("hosted_session")
            .and_then(|h| h.get("error"))
            .and_then(Json::as_str)
            .map(str::to_string);
        let hosted_session = p.get("hosted_session").and_then(|h| {
            if hosted_session_error.is_some() {
                return None;
            }
            Some(HostedLaunchOutcome {
                hosting_mechanism: h
                    .get("hosting_mechanism")
                    .and_then(Json::as_str)
                    .unwrap_or("session-abi")
                    .to_string(),
                session_ref: h
                    .get("session_ref")
                    .and_then(Json::as_str)
                    .map(str::to_string),
                limits_enforced: h
                    .get("limits_enforced")
                    .and_then(Json::as_str)
                    .unwrap_or("partial")
                    .to_string(),
            })
        });
        Ok(Bag {
            budgets,
            suite_tasks,
            arm_configs,
            ineligible,
            fingerprints,
            drifted,
            sealed,
            retirement_diff,
            min_replicates,
            enforcement,
            budget_params,
            cache_visibility,
            participant_descriptors,
            environment_classes,
            hosted_session,
            hosted_session_error,
        })
    }

    /// The engine context over `self` (the boxed closures borrow `bag`; the
    /// engine never outlives the handler scope).
    fn ctx(&self) -> EngineContext<'_> {
        EngineContext {
            resolve_budget: Some(Box::new(move |r: &str| self.budgets.get(r).cloned())),
            artifact_sealed: self.sealed.is_some().then(|| {
                Box::new(move |r: &str| self.sealed.as_ref().map(|s| s.contains(r)).unwrap_or(true))
                    as Box<dyn Fn(&str) -> bool + '_>
            }),
            capability_drifted: Some(Box::new(move |r: &str| self.drifted.contains(r))),
            retirement_diff: self.retirement_diff,
            min_replicates: self.min_replicates,
            suite_tasks: Some(Box::new(move |_: &ExperimentSpec| self.suite_tasks.clone())),
            arm_config: Some(Box::new(move |a: &ArmSpec| match &self.arm_configs {
                Some(m) => m
                    .get(&a.arm_id)
                    .cloned()
                    .ok_or_else(|| ExpandError::Assembly {
                        arm: a.arm_id.clone(),
                        detail: "no arm_configs member for this arm".to_string(),
                    }),
                None => Err(ExpandError::Assembly {
                    arm: a.arm_id.clone(),
                    detail: "arm_configs absent — the sealed configuration is \
                             unresolved at this boundary"
                        .to_string(),
                }),
            })),
            level_ineligible: Some(Box::new(move |r: &str| self.ineligible.get(r).copied())),
            fingerprint: Some(Box::new(move |r: &str| self.fingerprints.get(r).cloned())),
            budget_enforcement: self.enforcement.is_some().then(|| {
                Box::new(move |a: &ArmSpec| {
                    self.enforcement
                        .as_ref()
                        .and_then(|m| m.get(&a.arm_id))
                        .cloned()
                        .unwrap_or_else(BudgetEnforcement::native)
                }) as Box<dyn Fn(&ArmSpec) -> BudgetEnforcement + '_>
            }),
            budget_relevant_params: self.budget_params.is_some().then(|| {
                Box::new(move |r: &str| {
                    self.budget_params
                        .as_ref()
                        .and_then(|m| m.get(r))
                        .cloned()
                        .unwrap_or_default()
                })
                    as Box<dyn Fn(&str) -> BTreeMap<String, BudgetRelevantParam> + '_>
            }),
            cache_state_visible: self.cache_visibility.is_some().then(|| {
                Box::new(move |a: &ArmSpec| {
                    self.cache_visibility
                        .as_ref()
                        .and_then(|m| m.get(&a.arm_id).copied())
                }) as Box<dyn Fn(&ArmSpec) -> Option<bool> + '_>
            }),
            participant_descriptor: Some(Box::new(move |r: &str| {
                self.participant_descriptors.get(r).cloned()
            })),
            environment_class: Some(Box::new(move |r: &str| {
                self.environment_classes.get(r).cloned()
            })),
            hosted_launcher: (self.hosted_session.is_some() || self.hosted_session_error.is_some())
                .then(|| {
                    Box::new(move |req: &HostedLaunchRequest| {
                        if let Some(e) = &self.hosted_session_error {
                            return Err(e.clone());
                        }
                        self.hosted_session.clone().ok_or_else(|| {
                            format!("no hosted session established for {}", req.run_plan_id)
                        })
                    })
                        as Box<
                            dyn Fn(&HostedLaunchRequest) -> Result<HostedLaunchOutcome, String>
                                + '_,
                        >
                }),
        }
    }
}

/// Bind an engine to the experiment run — the held lease when the map has
/// one, else `attach` (fence-acquires; `WouldBlock` while another holder
/// lives). Disjoint field borrows: `store` and `leases` are separate
/// `EmbedService` members.
fn engine_for<'a>(
    store: &'a mut Store,
    leases: &mut BTreeMap<String, Lease>,
    docs: LabDocs,
    bag: &'a Bag,
    run_id: &str,
) -> Result<ExperimentEngine<'a>, EmbedError> {
    match leases.remove(run_id) {
        Some(lease) => Ok(ExperimentEngine::bind_lease(
            store,
            docs,
            bag.ctx(),
            run_id.to_string(),
            lease,
        )),
        None => {
            let mut eng = ExperimentEngine::new(store, docs, bag.ctx());
            eng.attach_run(run_id).map_err(xerr)?;
            Ok(eng)
        }
    }
}

/// Hand the (possibly renewed) binding back to the lease map — consumes the
/// engine so its `&mut Store` borrow ends before the map is touched.
fn park(leases: &mut BTreeMap<String, Lease>, mut eng: ExperimentEngine<'_>) {
    if let Some((run_id, lease)) = eng.take_binding() {
        leases.insert(run_id, lease);
    }
}

impl EmbedService {
    /// The `LabDocs` view over this store root.
    pub(crate) fn lab_docs(&self) -> Result<LabDocs, EmbedError> {
        LabDocs::open(self.store.root()).map_err(xerr)
    }

    /// The experiment run id for an `experiment_id`-addressed call (or an
    /// `experiment_run_id`-addressed one).
    fn experiment_run_id(&self, docs: &LabDocs, params: &Json) -> Result<String, EmbedError> {
        if let Some(run_id) = opt_str(params, "experiment_run_id") {
            return Ok(run_id);
        }
        let eid = req_str(params, "experiment_id")?;
        let entry = docs
            .index_entry(eid)
            .map_err(xerr)?
            .and_then(|e| e.run_id)
            .ok_or_else(|| EmbedError::Refused {
                reason: format!("PlanNotFound: no open experiment run for {eid}"),
            })?;
        Ok(entry)
    }

    /// `lab.experiment.register{spec, budgets?, …resolvers, dry_run?}` — the
    /// E-1 gate + the durable spec deposit. `dry_run` runs admission only.
    pub(crate) fn lab_experiment_register(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let mut spec = ExperimentSpec::from_json(req(params, "spec")?)
            .map_err(|e| bad("/spec", &format!("{e:?}")))?;
        if spec.experiment_id.is_empty() {
            spec.experiment_id = spec.experiment_id();
        }
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let dry = matches!(params.get("dry_run"), Some(Json::Bool(true)));
        {
            let mut eng = ExperimentEngine::new(&mut self.store, docs.clone(), bag.ctx());
            if dry {
                eng.validate(&spec).map_err(xerr)?;
            } else {
                eng.register(&spec).map_err(xerr)?;
            }
        }
        if !dry {
            // Deposit the resolver budgets — later ops resolve the spec's
            // budget refs without resupplying the bodies.
            if let Some(Json::Obj(m)) = params.get("budgets") {
                for (k, v) in m {
                    docs.put_named(doc_kind::BUDGET, k, v).map_err(xerr)?;
                }
            }
        }
        Ok(Json::obj([
            ("experiment_id", Json::str(spec.experiment_id)),
            ("registered", Json::Bool(!dry)),
            ("dry_run", Json::Bool(dry)),
        ]))
    }

    /// `lab.experiment.expand{experiment_id, suite_tasks?, arm_configs?, …}` —
    /// the pure expansion + the `CellPlan` preview.
    pub(crate) fn lab_experiment_expand(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let eid = req_str(params, "experiment_id")?.to_string();
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let plan_id = {
            let mut eng = ExperimentEngine::new(&mut self.store, docs.clone(), bag.ctx());
            eng.expand(&eid).map_err(xerr)?
        };
        let plan = docs
            .plan(&plan_id)
            .map_err(xerr)?
            .map(|p| p.to_json())
            .unwrap_or(Json::Null);
        Ok(Json::obj([
            ("experiment_id", Json::str(eid)),
            ("plan_id", Json::str(plan_id)),
            ("plan", plan),
        ]))
    }

    /// `lab.experiment.open_experiment{experiment_id}` — mint the
    /// `run_kind = experiment` run, allocate the pool, commit `declared` +
    /// `run_planned` + the `opened` drift bracket.
    pub(crate) fn lab_experiment_open(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let eid = req_str(params, "experiment_id")?.to_string();
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let mut eng = ExperimentEngine::new(&mut self.store, docs, bag.ctx());
        let opened = eng.open_experiment(&eid).map_err(xerr);
        let run_id = opened?;
        park(&mut self.experiment_engines, eng);
        Ok(Json::obj([
            ("experiment_id", Json::str(eid)),
            ("experiment_run_id", Json::str(run_id)),
        ]))
    }

    /// `lab.experiment.next{experiment_id|experiment_run_id}` → the verdict.
    pub(crate) fn lab_experiment_next(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let run_id = self.experiment_run_id(&docs, params)?;
        let mut eng = engine_for(
            &mut self.store,
            &mut self.experiment_engines,
            docs,
            &bag,
            &run_id,
        )?;
        let verdict = eng.next();
        park(&mut self.experiment_engines, eng);
        match verdict.map_err(xerr)? {
            NextVerdict::Plan { run_plan_id } => Ok(Json::obj([
                ("verdict", Json::str("plan")),
                ("run_plan_id", Json::str(run_plan_id)),
            ])),
            NextVerdict::Done => Ok(Json::obj([("verdict", Json::str("done"))])),
            NextVerdict::Backoff { not_before_ms } => Ok(Json::obj([
                ("verdict", Json::str("backoff")),
                ("not_before_ms", Json::Int(not_before_ms as i64)),
            ])),
            NextVerdict::BudgetExhausted => {
                Ok(Json::obj([("verdict", Json::str("budget_exhausted"))]))
            }
            NextVerdict::Wait => Ok(Json::obj([("verdict", Json::str("wait"))])),
        }
    }

    /// `lab.experiment.claim{experiment_id, run_plan_id, holder?}` → ticket.
    pub(crate) fn lab_experiment_claim(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let run_id = self.experiment_run_id(&docs, params)?;
        let rpid = req_str(params, "run_plan_id")?.to_string();
        let holder = opt_str(params, "holder").unwrap_or_else(|| self.holder.clone());
        let mut eng = engine_for(
            &mut self.store,
            &mut self.experiment_engines,
            docs,
            &bag,
            &run_id,
        )?;
        let t = eng.claim(&rpid, &holder);
        park(&mut self.experiment_engines, eng);
        let t: ClaimTicket = t.map_err(xerr)?;
        Ok(Json::obj([
            ("run_plan_id", Json::str(t.run_plan_id)),
            ("holder", Json::str(t.holder)),
            ("lease_id", Json::str(t.lease_id)),
            ("expires_at_ms", Json::Int(t.expires_at_ms as i64)),
        ]))
    }

    /// `lab.experiment.launch{experiment_id, run_plan_id, lease_id, holder?}`
    /// → the opened subject run + its writer lease (the driver writes with
    /// it).
    pub(crate) fn lab_experiment_launch(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let run_id = self.experiment_run_id(&docs, params)?;
        let ticket = ClaimTicket {
            run_plan_id: req_str(params, "run_plan_id")?.to_string(),
            holder: opt_str(params, "holder").unwrap_or_else(|| self.holder.clone()),
            lease_id: req_str(params, "lease_id")?.to_string(),
            expires_at_ms: 0,
        };
        let subject_holder =
            opt_str(params, "subject_holder").unwrap_or_else(|| "experiment-subject".to_string());
        let mut eng = engine_for(
            &mut self.store,
            &mut self.experiment_engines,
            docs,
            &bag,
            &run_id,
        )?;
        let out = eng.launch(&ticket, &subject_holder);
        park(&mut self.experiment_engines, eng);
        let out = out.map_err(xerr)?;
        let w = &out.subject_writer;
        Ok(Json::obj([
            ("run_plan_id", Json::str(out.run_plan_id)),
            ("run_id", Json::str(out.run_id)),
            ("attempt_no", Json::Int(out.attempt_no as i64)),
            ("budget_id", Json::str(out.budget_id)),
            (
                "subject_writer",
                Json::obj([
                    ("lease_id", Json::str(&w.lease_id)),
                    ("holder", Json::str(&w.holder)),
                    ("generation", Json::Int(w.generation as i64)),
                    ("expires_at_ms", Json::Int(w.expires_at_ms as i64)),
                ]),
            ),
        ]))
    }

    /// `lab.experiment.settle{experiment_id, run_plan_id}` → `RunOutcome`.
    pub(crate) fn lab_experiment_settle(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let run_id = self.experiment_run_id(&docs, params)?;
        let rpid = req_str(params, "run_plan_id")?.to_string();
        let mut eng = engine_for(
            &mut self.store,
            &mut self.experiment_engines,
            docs,
            &bag,
            &run_id,
        )?;
        let out = eng.settle(&rpid);
        park(&mut self.experiment_engines, eng);
        let o = out.map_err(xerr)?;
        let mut m = BTreeMap::new();
        m.insert("run_plan_id".into(), Json::str(o.run_plan_id));
        m.insert("run_id".into(), Json::str(o.run_id));
        m.insert("outcome_class".into(), Json::str(o.outcome_class.as_str()));
        m.insert("accepted".into(), Json::Bool(o.accepted));
        m.insert("superseded".into(), Json::Bool(o.superseded));
        m.insert("plan_final".into(), Json::Bool(o.plan_final));
        m.insert("attempt_no".into(), Json::Int(o.attempt_no as i64));
        m.insert("budget_utilization".into(), o.budget_utilization);
        m.insert(
            "veto_tripped".into(),
            Json::Arr(o.veto_tripped.iter().map(Json::str).collect()),
        );
        m.insert("replanned".into(), Json::Bool(o.replanned));
        if let Some(p) = o.paused {
            m.insert("paused".into(), Json::str(p));
        }
        Ok(Json::Obj(m))
    }

    /// `lab.experiment.pause{experiment_id, reason?, probe?}` — the closed
    /// reason set; `probe` is the caller's audit tag (`paused{probe}`).
    pub(crate) fn lab_experiment_pause(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let reason = opt_str(params, "reason").unwrap_or_else(|| "operator".to_string());
        let reason =
            PauseReason::parse(&reason).ok_or_else(|| bad("/reason", "unknown_pause_reason"))?;
        let probe = opt_str(params, "probe");
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let run_id = self.experiment_run_id(&docs, params)?;
        let mut eng = engine_for(
            &mut self.store,
            &mut self.experiment_engines,
            docs,
            &bag,
            &run_id,
        )?;
        let r = eng.pause(reason, probe.as_deref());
        park(&mut self.experiment_engines, eng);
        r.map_err(xerr)?;
        Ok(Json::obj([("paused", Json::Bool(true))]))
    }

    /// `lab.experiment.resume{experiment_id, probe?}` — `resumed{probe}`.
    pub(crate) fn lab_experiment_resume(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let probe = opt_str(params, "probe");
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let run_id = self.experiment_run_id(&docs, params)?;
        let mut eng = engine_for(
            &mut self.store,
            &mut self.experiment_engines,
            docs,
            &bag,
            &run_id,
        )?;
        let r = eng.resume(probe.as_deref());
        park(&mut self.experiment_engines, eng);
        r.map_err(xerr)?;
        Ok(Json::obj([("resumed", Json::Bool(true))]))
    }

    /// `lab.experiment.close{experiment_id, partial?, bundle_id?}` →
    /// `ExperimentReport`. `bundle_id` names the experiment bundle the
    /// caller assembled (`kernel.bundle{kind: experiment}` over the arm
    /// bundles); the engine records it (`bundle_assembled`) so the
    /// report's `bundle_id` names the close's bundle (AC-R-2.10.3-11).
    pub(crate) fn lab_experiment_close(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let partial = matches!(params.get("partial"), Some(Json::Bool(true)));
        let bundle_id = opt_str(params, "bundle_id");
        let bag = Bag::from_params(params)?;
        let docs = self.lab_docs()?;
        let run_id = self.experiment_run_id(&docs, params)?;
        let mut eng = engine_for(
            &mut self.store,
            &mut self.experiment_engines,
            docs,
            &bag,
            &run_id,
        )?;
        let r = bundle_id
            .as_deref()
            .map(|b| eng.record_bundle(b))
            .transpose()
            .and_then(|_| eng.close(partial));
        park(&mut self.experiment_engines, eng);
        let report = r.map_err(xerr)?;
        let mut m = BTreeMap::new();
        m.insert("status".into(), Json::str(report.status.as_str()));
        m.insert("coverage".into(), report.coverage);
        m.insert(
            "outcome_counts".into(),
            Json::Obj(
                report
                    .outcome_counts
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                    .collect(),
            ),
        );
        m.insert("provider_drift".into(), Json::Bool(report.provider_drift));
        m.insert("watermark_set".into(), report.watermark_set);
        if let Some(b) = report.bundle_id {
            m.insert("bundle_id".into(), Json::str(b));
        }
        Ok(Json::Obj(m))
    }
}
