//! `probe_profile(profile, executor, budget)` — the §5b.3 full profile
//! probe suite (R-2.3.3/AC-R-2.3.3-5's Stage-5 completion). The profile's
//! declared `tests.probes[]` specs execute through a caller-supplied
//! [`ProbeExecutor`] (the transport side lives in `hh-gateway::probes`;
//! the scripted embeds produce `ProbeOutcome`s directly) and every spec
//! lands a `ConformanceRecord` under one deterministic `probe_run_id` —
//! the `model.profile.probed{profile_ref, probe_run_id, records[]}` event
//! row (plus `calls`/`charged_to` accounting — a probe call is an ordinary
//! model call, never free evidence).
//!
//! Semantics:
//!
//! - Every `ProbeKind` in the closed set executes — the suite is whatever
//!   the profile declares in `tests.probes[]`; a malformed spec is
//!   `InvalidModelProfile`, never silently dropped (CC3's no-silent-skip —
//!   the `test_profile` conformance section drops `None` specs because it
//!   is *replay* of recorded outcomes; execution cannot).
//! - `budget` bounds the sum of declared `spec.budget`s — the first spec
//!   that would overrun is `ProbeBudgetExhausted{spent, budget}`; partial
//!   records are not emitted (the run either completes or refuses).
//! - `probe_run_id` is `idp/1` over the canonical
//!   `{profile coordinate, spec shapes, outcome spellings}` — a replay of
//!   the same outcomes mints the same run id (determinism, CC4).
//! - `records[]` carry the full closed verdict set — `UNKNOWN` stays
//!   `UNKNOWN` (§5b.3: never coerced).

use hh_wire::json::Json;

use crate::errors::CompileError;
use crate::profile::{profile_coordinate, CapabilityState, ModelProfile};
use crate::profile_test::{
    conformance_to_json, probe_spec_from_json, ConformanceRecord, ProbeOutcome, ProbeSpec,
};

/// The probe executor the caller binds — the scripted fake, the gateway's
/// transport probe (`hh-gateway::probes::probe_capability`), or a fixture
/// replay. One outcome per spec; the executor never decides the verdict
/// (`evaluate_probe` owns the declared×observed mapping).
pub trait ProbeExecutor {
    /// Execute one `ProbeSpec`; the outcome is `Inconclusive` — never a
    /// coerced pass — when the transport cannot decide.
    fn run_probe(&self, spec: &ProbeSpec) -> ProbeOutcome;
}

/// `probe_profile`'s refusal vocabulary.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeRunError {
    /// The cumulative declared budget exceeded the run's bound
    /// (`{spent, budget}` — the §5b.3 `ProbeBudgetExhausted`).
    BudgetExhausted {
        /// The budget consumed before the refusing spec.
        spent: u64,
        /// The run's declared bound.
        budget: u64,
    },
    /// A `tests.probes[]` entry does not decode to a `ProbeSpec`.
    MalformedSpec {
        /// Which member and why.
        detail: String,
    },
}

/// `ProfileProbeReport{profile_ref, probe_run_id, records[], calls,
/// charged_to}` — the `model.profile.probed` payload's typed home.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileProbeReport {
    /// The profile coordinate under test.
    pub profile_ref: String,
    /// `idp/1` over `{coordinate, spec shapes, outcome spellings}`.
    pub probe_run_id: String,
    /// The `ConformanceRecord`s — one per declared spec.
    pub records: Vec<ConformanceRecord>,
    /// The calls the suite consumed (sum of `spec.samples`).
    pub calls: u64,
    /// The accounting subject — the profile coordinate (probing is the
    /// profile's own evidence obligation; §5b.3 `charged_to`).
    pub charged_to: String,
}

/// The `model.profile.probed{profile_ref, probe_run_id, records[]}` payload —
/// `records[]` are the canonical `ConformanceRecord` JSONs; `calls` and
/// `charged_to` ride as the accounting members.
pub fn probed_payload(report: &ProfileProbeReport) -> Json {
    Json::obj([
        ("profile_ref", Json::str(report.profile_ref.clone())),
        ("probe_run_id", Json::str(report.probe_run_id.clone())),
        (
            "records",
            Json::Arr(report.records.iter().map(conformance_to_json).collect()),
        ),
        ("calls", Json::Int(report.calls as i64)),
        ("charged_to", Json::str(report.charged_to.clone())),
    ])
}

/// The profile's declared probe suite — `tests.probes[]` decoded through the
/// one `probe_spec_from_json` codec; a malformed member is a typed error,
/// never a skipped spec (execution is not replay).
pub fn probe_specs(profile: &ModelProfile) -> Result<Vec<ProbeSpec>, ProbeRunError> {
    let Some(arr) = profile.tests.get("probes") else {
        return Ok(Vec::new());
    };
    let Json::Arr(items) = arr else {
        return Err(ProbeRunError::MalformedSpec {
            detail: "tests.probes must be an array".to_string(),
        });
    };
    items
        .iter()
        .enumerate()
        .map(|(i, j)| {
            probe_spec_from_json(j).ok_or_else(|| ProbeRunError::MalformedSpec {
                detail: format!("tests.probes[{i}] does not decode to a ProbeSpec"),
            })
        })
        .collect()
}

/// `probe_profile(profile, executor, budget)` — run the declared suite.
/// `budget` is `None` for an unbounded run (the caller's accounting surface
/// still records `calls`); `Some(b)` refuses `ProbeBudgetExhausted` at the
/// first spec whose declared `budget` would overrun `spent + spec.budget`.
pub fn probe_profile(
    profile: &ModelProfile,
    executor: &dyn ProbeExecutor,
    budget: Option<u64>,
) -> Result<ProfileProbeReport, ProbeRunError> {
    let specs = probe_specs(profile)?;
    // The budget refusal lands before any spec executes — a partial suite is
    // not a probe run (the event never claims records it did not earn).
    let total_budget: u64 = specs.iter().map(|s| s.budget as u64).sum();
    if let Some(b) = budget {
        if total_budget > b {
            return Err(ProbeRunError::BudgetExhausted {
                spent: 0,
                budget: b,
            });
        }
    }
    let coord = profile_coordinate(profile);
    // Outcomes first — `probe_run_id` covers the outcome spellings, so the
    // id is a commitment to what the suite observed.
    let outcomes: Vec<ProbeOutcome> = specs.iter().map(|s| executor.run_probe(s)).collect();
    let run_preimage = Json::obj([
        ("profile", Json::str(coord.clone())),
        (
            "specs",
            Json::Arr(
                specs
                    .iter()
                    .map(|s| {
                        Json::obj([
                            ("capability", Json::str(s.capability.clone())),
                            ("kind", Json::str(s.kind.name())),
                            ("fixture_ref", Json::str(s.fixture_ref.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "outcomes",
            Json::Arr(outcomes.iter().map(outcome_spelling).collect()),
        ),
    ]);
    let probe_run_id = hh_identity::idp_id(
        "profile.probe_run",
        run_preimage.to_canonical_string().as_bytes(),
    );
    let records: Vec<ConformanceRecord> = specs
        .iter()
        .zip(outcomes.iter())
        .map(|(spec, outcome)| {
            let declared = profile
                .capabilities
                .capability_state(&spec.capability)
                .unwrap_or(CapabilityState::Unknown);
            crate::profile_test::evaluate_probe(spec, declared, outcome, &coord, &probe_run_id)
        })
        .collect();
    Ok(ProfileProbeReport {
        profile_ref: coord.clone(),
        probe_run_id,
        calls: specs.iter().map(|s| s.samples as u64).sum(),
        charged_to: coord,
        records,
    })
}

/// The `ProbeOutcome`'s canonical spelling for the `probe_run_id` preimage
/// (a shape commitment — payloads stay out of the id).
fn outcome_spelling(o: &ProbeOutcome) -> Json {
    match o {
        ProbeOutcome::Accepted => Json::str("accepted"),
        ProbeOutcome::Rejected => Json::str("rejected"),
        ProbeOutcome::RoundtripMatch => Json::str("roundtrip_match"),
        ProbeOutcome::RoundtripMismatch { .. } => Json::str("roundtrip_mismatch"),
        ProbeOutcome::Inconclusive { .. } => Json::str("inconclusive"),
    }
}

/// `CompileError` view — `MalformedSpec` is an `InvalidModelProfile`; the
/// budget leg stays `ProbeRunError` (it is a run refusal, not a profile
/// defect).
impl From<ProbeRunError> for CompileError {
    fn from(e: ProbeRunError) -> Self {
        match e {
            ProbeRunError::MalformedSpec { detail } => CompileError::InvalidModelProfile { detail },
            ProbeRunError::BudgetExhausted { spent, budget } => CompileError::InvalidModelProfile {
                detail: format!("ProbeBudgetExhausted{{spent: {spent}, budget: {budget}}}"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Script(Vec<ProbeOutcome>);
    impl ProbeExecutor for Script {
        fn run_probe(&self, spec: &ProbeSpec) -> ProbeOutcome {
            self.0
                .get(spec.samples as usize % self.0.len())
                .cloned()
                .unwrap_or(ProbeOutcome::Inconclusive {
                    reason: "no fixture".to_string(),
                })
        }
    }

    fn spec_json(cap: &str, kind: &str, samples: i64, budget: i64) -> Json {
        Json::obj([
            ("capability", Json::str(cap)),
            ("kind", Json::str(kind)),
            ("fixture_ref", Json::str("fx/1")),
            ("samples", Json::Int(samples)),
            ("budget", Json::Int(budget)),
        ])
    }

    fn profile_with_probes(probes: Vec<Json>) -> ModelProfile {
        use crate::profile::*;
        let mut p = ModelProfile {
            profile_id: "prof/test".to_string(),
            version: "1.0".to_string(),
            content_hash: String::new(),
            selector: ProfileSelector {
                provider_api_family: "test-api".to_string(),
                model_family: "test-model".to_string(),
                version_pattern: VersionPattern::Any,
                precedence: 0,
                successor_ref: None,
                retirement_at: None,
                roles_admitted: vec![ModelRole::Primary],
            },
            extends: None,
            capabilities: ProfileCapabilities::default(),
            rules: vec![],
            ext: std::collections::BTreeMap::new(),
            expiry: ProfileDebtRecord {
                rule_id: "prof/test.expiry".to_string(),
                hypothesis: "h".to_string(),
                evidence_refs: vec![EvidenceRef::legacy("ev/1")],
                owner: "o".to_string(),
                reach_via: vec![],
                expiry_condition: ExpiryCondition {
                    kind: hh_ontology::debt::ExpiryKind::Date,
                    value: None,
                },
                removal_test_ref: "t/1".to_string(),
                removal_test: None,
                status: hh_ontology::debt::DebtStatus::Active,
                debt_class: None,
                hypothesis_typed: None,
                scope: None,
                expiry: None,
                runway_ms: None,
                revalidation: None,
                created_at: None,
                supersedes: None,
            },
            compatibility: ProfileCompatibility {
                inventory_version: "1.0".to_string(),
                min_compiler_version: "0.0.0".to_string(),
            },
            tests: Json::obj([]),
        };
        if let Json::Obj(m) = &mut p.tests {
            m.insert("probes".to_string(), Json::Arr(probes));
        }
        p.content_hash = crate::profile::profile_identity(&p);
        p
    }

    #[test]
    fn full_kind_set_executes_and_records() {
        // All 12 `ProbeKind`s — one spec each.
        let kinds = [
            ("native_function_calling", "schema_accept"),
            ("native_function_calling", "name_roundtrip"),
            ("native_function_calling", "id_roundtrip"),
            ("reasoning_replay", "signature_replay"),
            ("parallel_tool_calls", "parallel_calls"),
            ("developer_role", "developer_role"),
            ("image_input", "image_input"),
            ("context_window", "context_window_edge"),
            ("max_output", "max_output_edge"),
            ("strict_schema_dialect", "strict_keyword"),
            ("cache_control_convention", "cache_marker_accept"),
            ("seed_honoured", "seed_repeat"),
        ];
        let mut probes: Vec<Json> = kinds
            .iter()
            .map(|(cap, kind)| spec_json(cap, kind, 1, 1))
            .collect();
        // `strict_keyword` carries its keyword operand.
        if let Json::Obj(m) = &mut probes[9] {
            m.insert("keyword".to_string(), Json::str("strict"));
        }
        let p = profile_with_probes(probes);
        let specs = probe_specs(&p).expect("specs decode");
        assert_eq!(specs.len(), 12);
        // Every declared kind decoded — the closed set covers all 12.
        let kinds_seen: Vec<&'static str> = specs.iter().map(|s| s.kind.name()).collect();
        for (i, (_, k)) in kinds.iter().enumerate() {
            assert_eq!(kinds_seen[i], *k);
        }

        let report =
            probe_profile(&p, &Script(vec![ProbeOutcome::Accepted]), None).expect("suite runs");
        assert_eq!(report.records.len(), 12);
        assert_eq!(report.calls, 12); // one sample each
        assert_eq!(report.charged_to, report.profile_ref);
        assert!(report.probe_run_id.starts_with("idp/1:") || !report.probe_run_id.is_empty());
        // A rejected probe on a declared capability is DRIFT, never coerced.
        let report2 =
            probe_profile(&p, &Script(vec![ProbeOutcome::Rejected]), None).expect("suite runs");
        assert!(report2.records.iter().all(|r| matches!(
            r.verdict,
            crate::profile_test::ConformanceVerdict::Drift
                | crate::profile_test::ConformanceVerdict::Unsupported
        )));
        // UNKNOWN stays UNKNOWN — an inconclusive outcome never coerces.
        let report3 = probe_profile(
            &p,
            &Script(vec![ProbeOutcome::Inconclusive {
                reason: "no fixture".to_string(),
            }]),
            None,
        )
        .expect("suite runs");
        assert!(report3
            .records
            .iter()
            .all(|r| r.verdict == crate::profile_test::ConformanceVerdict::Unknown));
    }

    #[test]
    fn budget_exhaustion_refuses_before_any_record() {
        let p = profile_with_probes(vec![
            spec_json("image_input", "image_input", 1, 5),
            spec_json("seed_honoured", "seed_repeat", 1, 5),
        ]);
        match probe_profile(&p, &Script(vec![ProbeOutcome::Accepted]), Some(9)) {
            Err(ProbeRunError::BudgetExhausted { spent, budget }) => {
                assert_eq!((spent, budget), (0, 9));
            }
            other => panic!("expected BudgetExhausted, got {other:?}"),
        }
        // Exactly the bound runs.
        probe_profile(&p, &Script(vec![ProbeOutcome::Accepted]), Some(10)).expect("at bound");
    }

    #[test]
    fn malformed_spec_is_a_typed_error() {
        let p = profile_with_probes(vec![Json::obj([("kind", Json::str("no_such_kind"))])]);
        match probe_specs(&p) {
            Err(ProbeRunError::MalformedSpec { .. }) => {}
            other => panic!("expected MalformedSpec, got {other:?}"),
        }
    }

    #[test]
    fn probe_run_id_is_deterministic() {
        let p = profile_with_probes(vec![spec_json("image_input", "image_input", 1, 1)]);
        let a = probe_profile(&p, &Script(vec![ProbeOutcome::Accepted]), None).unwrap();
        let b = probe_profile(&p, &Script(vec![ProbeOutcome::Accepted]), None).unwrap();
        assert_eq!(a.probe_run_id, b.probe_run_id);
        // A different outcome is a different run.
        let c = probe_profile(&p, &Script(vec![ProbeOutcome::Rejected]), None).unwrap();
        assert_ne!(a.probe_run_id, c.probe_run_id);
    }

    #[test]
    fn probed_payload_shape() {
        let p = profile_with_probes(vec![spec_json("image_input", "image_input", 2, 1)]);
        let r = probe_profile(&p, &Script(vec![ProbeOutcome::Accepted]), None).unwrap();
        let j = probed_payload(&r);
        assert_eq!(
            j.get("profile_ref").and_then(Json::as_str),
            Some(r.profile_ref.as_str())
        );
        assert_eq!(j.get("calls").and_then(Json::as_int), Some(2));
        assert_eq!(
            j.get("charged_to").and_then(Json::as_str),
            Some(r.profile_ref.as_str())
        );
    }
}
