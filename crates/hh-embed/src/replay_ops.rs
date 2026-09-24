//! Group W — `replay` + `counterfactual` (R-2.2.4⁰ᵇ; §5a.4; ADR-0135;
//! ticket S3.6). The boundary halves: `replay` mints
//! `lifecycle.replay.started`/`finished` on the target, runs the mode's
//! driver (`reconstruct` — the ledger's view/env rebuild, nothing
//! re-executes; `deterministic` — `hh_control::replay` re-drives the
//! declaring variant over the recorded inputs with the executor count at
//! 0), and lands the `ReplayValidityReport` blob (`report_ref`).
//!
//! `counterfactual` opens the paired arms (§5a.4's hook): `n ≥ k` factual
//! re-executions + `n ≥ k` counterfactual branches off one fork point and
//! one snapshot, `branch_kind = counterfactual`, `arm_role` marked,
//! `charged_to = instrument`, seeds as the replicate axis, and the
//! `MatchSpec` blobbed + named on every arm (`UnbudgetedArm` without one).
//! The factual arms are *driven* — the deterministic replay re-runs the
//! source suffix and each arm's `lifecycle.replay.finished` carries its
//! validity; the factual dispersion (the per-arm outcome spread — the
//! noise floor) lands in the `ComparisonReport` blob the result names by
//! `comparison_ref`. The intervention-bearing arms are opened with
//! `intervention_ref` — their drive applies the intervention to the
//! recorded feed (the `replay` op on the arm run).

use hh_control::driver::DriverConfig;
use hh_control::policy::EnvelopePolicy;
use hh_control::strategy::{ControlContext, StrategyParams};
use hh_embed_schema::errors::EmbedError;
use hh_embed_schema::types::{CounterfactualParams, ReplayParams};
use hh_ledger::branch::{BranchKind, EnvBinding, ForkOpts, ReplayMode};
use hh_ledger::event::EventEnvelope;
use hh_ledger::manifest::ObservabilityLevel;
use hh_ledger::replay::{ReplayDriverMode, ValidityInput};
use hh_wire::json::Json;

use crate::open::{driver_surfaces, leaf_arm_for, steering_for, strategy_for};
use crate::runtime::{KernelAssembler, SUBMIT_SURFACE};
use crate::service::{ledger_err, EmbedService};

impl EmbedService {
    /// `replay{session_id, run_id?, driver_mode, until_seq?}` — both driver
    /// modes land `lifecycle.replay.started`/`finished{mode, report_ref}`
    /// on the target (the report blob is content-addressed — rebuildable,
    /// AC-R-2.2.4-11).
    pub(crate) fn replay(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = ReplayParams::from_json(params)?;
        let session = self.live_session(&p.session_id)?;
        let target = p.run_id.clone().unwrap_or_else(|| session.run_id.clone());
        // The run must exist in this store.
        let manifest = self.store.manifest(&target).map_err(ledger_err)?.clone();
        let driver_mode =
            ReplayDriverMode::parse(&p.driver_mode).ok_or_else(|| EmbedError::SchemaViolation {
                path: "replay/driver_mode".to_string(),
                code: "unknown_driver_mode".to_string(),
            })?;
        let until = p.until_seq.map(|u| u as u64);

        // The recorded variant — the leaf arm's `control_variant` (the
        // durable re-arm record; absent ⇒ non-declaring).
        let arm = leaf_arm_for(&self.store, &target);
        let variant = arm
            .as_ref()
            .map(|a| a.control_variant.clone())
            .unwrap_or_default();
        let variant_declares =
            !variant.is_empty() && strategy_for(&variant).capabilities().deterministic_replay;

        hh_ledger::replay::replay_started(&mut self.store, &target, driver_mode, until)
            .map_err(ledger_err)?;

        let report = match driver_mode {
            ReplayDriverMode::Reconstruct => {
                // The `reconstruct` driver — views + env binding rebuilt,
                // nothing re-executed (ADR-0028 §1). `reproduced` stays
                // `None` — this mode never claims the reproduction half.
                let _recon = hh_ledger::replay::reconstruct(&self.store, &target, until)
                    .map_err(ledger_err)?;
                let input = ValidityInput {
                    variant_declares,
                    ..Default::default()
                };
                hh_ledger::replay::validity(&self.store, &target, &input).map_err(ledger_err)?
            }
            ReplayDriverMode::Deterministic => {
                // `deterministic` is a claim the variant must be entitled
                // to — a non-declaring variant refuses `VariantNotDeclaring`
                // (a `re_executed` verdict is `reconstruct`'s honest floor,
                // never a demand the deterministic driver fabricates).
                if !variant_declares {
                    return Err(EmbedError::Refused {
                        reason: format!(
                            "variant_not_declaring: {} does not declare deterministic_replay",
                            if variant.is_empty() {
                                "<unrecorded>"
                            } else {
                                &variant
                            }
                        ),
                    });
                }
                let events = self.store.envelopes(&target).map_err(ledger_err)?;
                let until = until.unwrap_or_else(|| events.last().map(|e| e.seq).unwrap_or(0));
                let recorded = hh_control::replay::extract_recorded(events, 0, Some(until));
                let (ctx, policy, config) =
                    self.replay_driver_env(&target, &manifest, arm.as_ref());
                let policy = policy.map_err(|e| EmbedError::Refused {
                    reason: format!("envelope_policy: {e:?}"),
                })?;
                let mut assembler = KernelAssembler;
                let outcome = hh_control::replay::deterministic_replay(
                    strategy_for(&variant),
                    &ctx,
                    policy,
                    config,
                    &[],
                    &recorded,
                    &mut assembler,
                    Some(SUBMIT_SURFACE),
                    &format!("replay:{target}"),
                )
                .map_err(|e| EmbedError::Refused {
                    reason: format!("replay_driver: {e:?}"),
                })?;
                let input = ValidityInput {
                    variant_declares: true,
                    reproduced: Some(outcome.reproduced()),
                    diverged: outcome
                        .diverged
                        .as_ref()
                        .map(|d| (d.at_seq, d.expected.clone(), d.got.clone())),
                    ..Default::default()
                };
                hh_ledger::replay::validity(&self.store, &target, &input).map_err(ledger_err)?
            }
        };

        let report_ref = report.content_ref();
        hh_ledger::replay::replay_finished(&mut self.store, &target, driver_mode, &report)
            .map_err(ledger_err)?;

        Ok(Json::obj([
            ("run_id", Json::str(target)),
            ("driver_mode", Json::str(driver_mode.as_str())),
            ("mode", Json::str(report.mode.as_str())),
            ("report_ref", Json::str(report_ref)),
            ("report", report.to_json()),
        ]))
    }

    /// The `ctx`/`policy`/`config` the deterministic driver arms with —
    /// `arm_driver`'s construction replay-shaped (same variant, same
    /// surfaces/budget, fresh counters).
    fn replay_driver_env(
        &self,
        run_id: &str,
        manifest: &hh_ledger::manifest::RunManifest,
        arm: Option<&crate::open::LeafArm>,
    ) -> (
        ControlContext,
        Result<EnvelopePolicy, hh_control::policy::PolicyError>,
        DriverConfig,
    ) {
        let surfaces = arm
            .map(|a| a.surfaces.clone())
            .unwrap_or_else(|| driver_surfaces(&[]));
        let (budget_ceiling, remaining) = arm
            .map(|a| (a.budget_ceiling.clone(), a.remaining.clone()))
            .unwrap_or_default();
        let control_variant = arm.map(|a| a.control_variant.clone()).unwrap_or_default();
        let ctx = ControlContext {
            process_ref: format!("hh-embed/{}", manifest.run_kind.as_str()),
            plan: vec![],
            boundary: hh_control::react::react_preset(),
            profile: Json::Null,
            account_ref: manifest
                .budget
                .clone()
                .unwrap_or_else(|| "acct:unbudgeted".to_string()),
            budget_ref: manifest.budget.clone().unwrap_or_else(|| "b-1".to_string()),
            envelope_ref: "env-1".to_string(),
            parameters: StrategyParams::default(),
            capabilities_available: surfaces.iter().map(|s| s.surface_id.clone()).collect(),
            steering: steering_for(&control_variant),
        };
        let policy = EnvelopePolicy::stage1_default(
            &manifest.budget.clone().unwrap_or_else(|| "b-1".to_string()),
        );
        let _ = run_id;
        (
            ctx,
            policy.seal(),
            DriverConfig {
                surfaces,
                budget_ceiling,
                remaining,
                ..DriverConfig::default()
            },
        )
    }

    /// `counterfactual{session_id, run_id?, at_seq, intervention, design,
    /// env?}` — the §5a.4 hook: refuse `UnbudgetedArm` without a parseable
    /// `MatchSpec`, refuse `InterventionPrecedesForkPoint` when `at` sits
    /// after the intervention's earliest affected seq, then open `n` factual
    /// + `n` counterfactual arms off one cut/snapshot and land the
    ///   `ComparisonReport` blob (`comparison_ref`).
    pub(crate) fn counterfactual(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = CounterfactualParams::from_json(params)?;
        let session = self.live_session(&p.session_id)?;
        let source = p.run_id.clone().unwrap_or_else(|| session.run_id.clone());
        let parent = self.store.manifest(&source).map_err(ledger_err)?.clone();

        // ── the design gates (T-LCD-14; ADR-0199) ─────────────────────
        let match_spec = hh_budget::matchspec::MatchSpec::from_json(&p.design.budget)
            .ok_or(EmbedError::UnbudgetedArm)?;
        if !p.design.factual_arm {
            return Err(EmbedError::Refused {
                reason: "factual_arm_required: the noise-floor arm is mandatory".to_string(),
            });
        }
        let k = p.design.k.unwrap_or(p.design.n_seeds);
        if p.design.n_seeds < k || k < 1 {
            return Err(EmbedError::Refused {
                reason: format!(
                    "design: n_seeds {} < k {} — the replicate axis needs n ≥ k ≥ 1",
                    p.design.n_seeds, k
                ),
            });
        }
        let n = p.design.n_seeds.max(1) as u64;
        if p.at_seq > p.intervention.earliest_affected_seq {
            return Err(EmbedError::Refused {
                reason: format!(
                    "intervention_precedes_fork_point: at_seq {} > earliest_affected_seq {}",
                    p.at_seq, p.intervention.earliest_affected_seq
                ),
            });
        }
        // The intervention kind is the ADR-0135 §5 closed sum.
        const KINDS: &[&str] = &[
            "definition_diff",
            "profile_diff",
            "response_substitution",
            "observation_substitution",
            "decision_override",
            "budget_change",
            "permission_change",
        ];
        if !KINDS.contains(&p.intervention.kind.as_str()) {
            return Err(EmbedError::SchemaViolation {
                path: "counterfactual/intervention/kind".to_string(),
                code: "unknown_intervention_kind".to_string(),
            });
        }
        let env = EnvBinding::parse(p.env.as_deref().unwrap_or("snapshot")).ok_or_else(|| {
            EmbedError::SchemaViolation {
                path: "counterfactual/env".to_string(),
                code: "unknown_env_binding".to_string(),
            }
        })?;
        if env == EnvBinding::SharedLive {
            return Err(EmbedError::Refused {
                reason: "shared_mutable_env: a live environment is never shared between arms"
                    .to_string(),
            });
        }

        // ── the intervention + match-spec blobs ────────────────────────
        let intervention_ref = self
            .store
            .put_blob(
                p.intervention.to_json().to_canonical_string().as_bytes(),
                "application/hh.intervention+json",
            )
            .map_err(ledger_err)?
            .id();
        let match_ref = self
            .store
            .put_blob(
                p.design.budget.to_canonical_string().as_bytes(),
                "application/hh.match-spec+json",
            )
            .map_err(ledger_err)?
            .id();

        // ── one snapshot for every arm (the same fork point and
        // snapshot — §5a.4) ────────────────────────────────────────────
        let mut snapshot_choice = None;
        if env == EnvBinding::Snapshot {
            let drv = self.env_drivers.get(&source).ok_or_else(|| {
                EmbedError::EnvironmentUnavailable {
                    reason: "snapshot_unavailable: no env driver for the source run".to_string(),
                }
            })?;
            let chosen = drv
                .snapshot_for(&self.store, p.at_seq as u64)
                .map_err(crate::open::env_err)?
                .ok_or_else(|| EmbedError::EnvironmentUnavailable {
                    reason: format!(
                        "snapshot_unavailable: no fs_tree snapshot at seq ≤ {}",
                        p.at_seq
                    ),
                })?;
            snapshot_choice = Some(chosen);
        }

        // ── the arms ───────────────────────────────────────────────────
        // The coverage→mode ladder (the fork op's own rule): `inherited`
        // resolves against the source's observability.
        let obs = &parent.observability_level;
        let effective_replay = if obs.contains(&ObservabilityLevel::Ledger) {
            ReplayMode::Exact
        } else if obs.contains(&ObservabilityLevel::ModelIo) {
            ReplayMode::Structural
        } else {
            ReplayMode::Observational
        };
        let holder = self.holder.clone();
        let mut factual: Vec<Json> = Vec::new();
        let mut counterfactual: Vec<Json> = Vec::new();
        for i in 0..n {
            for (role, intervention) in [
                ("factual", None),
                ("counterfactual", Some(intervention_ref.clone())),
            ] {
                // The replicate axis — `H(source ∥ at ∥ role ∥ i)`
                // (ADR-0135 §2's `noise_coupling` seed, derived over the
                // recorded coordinates).
                let seed = format!(
                    "sha256:{}",
                    hh_wire::sha256::sha256_hex(
                        format!("cf-seed:{source}:{}:{role}:{i}", p.at_seq).as_bytes()
                    )
                );
                let mut child =
                    hh_ledger::manifest::RunManifest::minimal(hh_ledger::manifest::RunKind::Agent);
                child.configuration_id = parent.configuration_id.clone();
                child.configuration_version_id = parent.configuration_version_id.clone();
                child.harness_def_ref = parent.harness_def_ref.clone();
                child.attendance = parent.attendance;
                child.budget = parent.budget.clone();
                child.envelope_policy_ref = parent.envelope_policy_ref.clone();
                child.parent_run_id = Some(source.clone());
                let opts = ForkOpts {
                    env,
                    replay_mode: ReplayMode::Inherited,
                    effective_replay,
                    downgrade_reason: None,
                    policy_ref: None,
                    budget_slice_ref: Some(match_ref.clone()),
                    coerce_to_boundary: false,
                    snapshot_ref: snapshot_choice
                        .as_ref()
                        .map(|(r, _)| r.snapshot_ref.clone()),
                    snapshot_at_seq: snapshot_choice.as_ref().map(|(r, _)| r.at_seq),
                    read_only: env == EnvBinding::TraceOnly,
                    arm_role: Some(role.to_string()),
                    intervention_ref: intervention.clone(),
                    seed: Some(seed.clone()),
                    charged_to: Some("instrument".to_string()),
                };
                let (child_run, _lease, record) = self
                    .store
                    .fork(
                        &source,
                        p.at_seq as u64,
                        BranchKind::Counterfactual,
                        &opts,
                        child,
                        &holder,
                    )
                    .map_err(ledger_err)?;
                let entry = Json::obj([
                    ("run_id", Json::str(child_run.clone())),
                    ("branch_id", Json::str(record.branch_id.clone())),
                    ("arm_role", Json::str(role)),
                    ("seed", Json::str(seed)),
                    ("charged_to", Json::str("instrument")),
                ]);
                if role == "factual" {
                    factual.push(entry);
                } else {
                    counterfactual.push(entry);
                }
            }
        }

        // ── the factual arms *drive* — the deterministic replay re-runs
        // the source suffix; each arm's validity lands durable
        // (`lifecycle.replay.started/finished` on the arm run) and its
        // outcome feeds the noise floor. ───────────────────────────────
        let mut dispersion: Vec<Json> = Vec::new();
        {
            let events = self.store.envelopes(&source).map_err(ledger_err)?;
            let seed_prefix: Vec<EventEnvelope> = events
                .iter()
                .filter(|e| e.seq <= p.at_seq as u64)
                .cloned()
                .collect();
            let recorded = hh_control::replay::extract_recorded(events, p.at_seq as u64, None);
            let arm = leaf_arm_for(&self.store, &source);
            let variant = arm
                .as_ref()
                .map(|a| a.control_variant.clone())
                .unwrap_or_default();
            let variant_declares =
                !variant.is_empty() && strategy_for(&variant).capabilities().deterministic_replay;
            for arm_entry in &factual {
                let arm_run = arm_entry
                    .get("run_id")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                hh_ledger::replay::replay_started(
                    &mut self.store,
                    &arm_run,
                    ReplayDriverMode::Deterministic,
                    None,
                )
                .map_err(ledger_err)?;
                let (report, outcome_json) = if variant_declares {
                    let (ctx, policy, config) =
                        self.replay_driver_env(&arm_run, &parent, arm.as_ref());
                    let mut assembler = KernelAssembler;
                    let policy = match policy {
                        Ok(p) => p,
                        Err(e) => {
                            let report = hh_ledger::replay::validity(
                                &self.store,
                                &arm_run,
                                &ValidityInput {
                                    variant_declares: true,
                                    reproduced: Some(false),
                                    diverged: Some((
                                        0,
                                        "drive".into(),
                                        format!("envelope_policy: {e:?}"),
                                    )),
                                    ..Default::default()
                                },
                            )
                            .map_err(ledger_err)?;
                            hh_ledger::replay::replay_finished(
                                &mut self.store,
                                &arm_run,
                                ReplayDriverMode::Deterministic,
                                &report,
                            )
                            .map_err(ledger_err)?;
                            dispersion.push(Json::obj([
                                (
                                    "run_id",
                                    arm_entry.get("run_id").cloned().unwrap_or(Json::Null),
                                ),
                                ("mode", Json::str(report.mode.as_str())),
                                ("report_ref", Json::str(report.content_ref())),
                                ("outcome", Json::Null),
                            ]));
                            continue;
                        }
                    };
                    match hh_control::replay::deterministic_replay(
                        strategy_for(&variant),
                        &ctx,
                        policy,
                        config,
                        &seed_prefix,
                        &recorded,
                        &mut assembler,
                        Some(SUBMIT_SURFACE),
                        &format!("replay:{arm_run}"),
                    ) {
                        Ok(outcome) => {
                            let input = ValidityInput {
                                variant_declares: true,
                                reproduced: Some(outcome.reproduced()),
                                diverged: outcome
                                    .diverged
                                    .as_ref()
                                    .map(|d| (d.at_seq, d.expected.clone(), d.got.clone())),
                                ..Default::default()
                            };
                            let report = hh_ledger::replay::validity(&self.store, &arm_run, &input)
                                .map_err(ledger_err)?;
                            (
                                report,
                                Json::obj([
                                    ("reproduced", Json::Bool(outcome.reproduced())),
                                    ("dispatches", Json::Int(outcome.dispatches as i64)),
                                ]),
                            )
                        }
                        Err(e) => {
                            // The drive itself failed — an honest `invalid`
                            // verdict, never a fabricated reproduction.
                            let input = ValidityInput {
                                variant_declares: true,
                                reproduced: Some(false),
                                diverged: Some((0, "drive".into(), format!("{e:?}"))),
                                ..Default::default()
                            };
                            let report = hh_ledger::replay::validity(&self.store, &arm_run, &input)
                                .map_err(ledger_err)?;
                            (
                                report,
                                Json::obj([
                                    ("reproduced", Json::Bool(false)),
                                    ("error", Json::str(format!("{e:?}"))),
                                ]),
                            )
                        }
                    }
                } else {
                    // A non-declaring source variant — the arm replays
                    // `re_executed` honestly (the noise floor names the
                    // sources it could not confine).
                    let input = ValidityInput {
                        variant_declares: false,
                        ..Default::default()
                    };
                    let report = hh_ledger::replay::validity(&self.store, &arm_run, &input)
                        .map_err(ledger_err)?;
                    (report, Json::obj([("reproduced", Json::Bool(false))]))
                };
                hh_ledger::replay::replay_finished(
                    &mut self.store,
                    &arm_run,
                    ReplayDriverMode::Deterministic,
                    &report,
                )
                .map_err(ledger_err)?;
                dispersion.push(Json::obj([
                    (
                        "run_id",
                        arm_entry.get("run_id").cloned().unwrap_or(Json::Null),
                    ),
                    ("mode", Json::str(report.mode.as_str())),
                    ("report_ref", Json::str(report.content_ref())),
                    ("outcome", outcome_json),
                ]));
            }
        }

        // ── the ComparisonReport blob — the factual arm's dispersion is
        // the always-reported noise floor (AC-R-2.2.4-8). ───────────────
        let report_doc = Json::obj([
            ("kind", Json::str("comparison_report")),
            ("source_run_id", Json::str(&source)),
            ("at_seq", Json::Int(p.at_seq)),
            ("intervention_ref", Json::str(&intervention_ref)),
            ("match_ref", Json::str(&match_ref)),
            ("match", match_spec.to_json()),
            ("n_seeds", Json::Int(p.design.n_seeds)),
            ("k", Json::Int(k)),
            ("charged_to", Json::str("instrument")),
            (
                "factual_dispersion",
                Json::obj([
                    ("arms", Json::Arr(dispersion)),
                    (
                        "note",
                        Json::str(
                            "the factual arm's distribution is the noise floor — always reported",
                        ),
                    ),
                ]),
            ),
        ]);
        let comparison_ref = self
            .store
            .put_blob(
                report_doc.to_canonical_string().as_bytes(),
                "application/hh.comparison-report+json",
            )
            .map_err(ledger_err)?
            .id();

        Ok(Json::obj([
            ("source_run_id", Json::str(source)),
            ("at_seq", Json::Int(p.at_seq)),
            ("intervention_ref", Json::str(intervention_ref)),
            ("factual", Json::Arr(factual)),
            ("counterfactual", Json::Arr(counterfactual)),
            ("comparison_ref", Json::str(comparison_ref)),
        ]))
    }
}
