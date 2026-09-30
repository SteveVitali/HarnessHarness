//! The infrastructure detector (spec §5h.4 §8; AC-R-2.9.4-6; S3.3).
//!
//! Failure classification is **typed and closed**:
//! - verifier-tier failure (crash/timeout/refusal) → `oracle_failure`;
//! - environment-tier failure (setup/materialize/apply infrastructure) →
//!   `infrastructure_failure`;
//! - an *ambiguous* signature → the run still scores, flagged
//!   `infra_suspected` (ADR-0144 — ambiguity never silently reclasses a
//!   failure as the participant's).
//!
//! The detector is deterministic over the grader/environment signature —
//! the same bytes always classify the same way.

use hh_wire::Json;

/// The closed infrastructure-signature taxonomy (§5h.4 §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InfraClass {
    /// The environment could not be materialized/attached (image pull,
    /// mount, sandbox setup) — `infrastructure_failure`.
    EnvironmentSetupFailed,
    /// The environment died mid-run (channel reset, container exit) —
    /// `infrastructure_failure`.
    EnvironmentDied,
    /// The verifier process crashed — `oracle_failure`.
    VerifierCrash,
    /// The verifier timed out — `oracle_failure`.
    VerifierTimeout,
    /// The verifier refused the submission — `oracle_failure`.
    VerifierRefusal,
    /// The submission artifact was never produced — a *scored* failure
    /// (apply-failure class; not infrastructure).
    ArtifactMissing,
    /// A signature that could be either tier — scored, flagged
    /// `infra_suspected`.
    Ambiguous,
    /// No infrastructure signature detected — clean.
    Clean,
}

impl InfraClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            InfraClass::EnvironmentSetupFailed => "environment_setup_failed",
            InfraClass::EnvironmentDied => "environment_died",
            InfraClass::VerifierCrash => "verifier_crash",
            InfraClass::VerifierTimeout => "verifier_timeout",
            InfraClass::VerifierRefusal => "verifier_refusal",
            InfraClass::ArtifactMissing => "artifact_missing",
            InfraClass::Ambiguous => "ambiguous",
            InfraClass::Clean => "clean",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<InfraClass> {
        [
            InfraClass::EnvironmentSetupFailed,
            InfraClass::EnvironmentDied,
            InfraClass::VerifierCrash,
            InfraClass::VerifierTimeout,
            InfraClass::VerifierRefusal,
            InfraClass::ArtifactMissing,
            InfraClass::Ambiguous,
            InfraClass::Clean,
        ]
        .into_iter()
        .find(|c| c.as_str() == s)
    }

    /// The outcome class this signature maps to — `Ambiguous`/`ArtifactMissing`
    /// `Clean` score normally (`None` here; the run's `outcome_class` stands).
    pub fn outcome_class(self) -> Option<hh_ontology::control::OutcomeClass> {
        use hh_ontology::control::OutcomeClass as O;
        match self {
            InfraClass::EnvironmentSetupFailed | InfraClass::EnvironmentDied => {
                Some(O::InfrastructureFailure)
            }
            InfraClass::VerifierCrash
            | InfraClass::VerifierTimeout
            | InfraClass::VerifierRefusal => Some(O::OracleFailure),
            InfraClass::ArtifactMissing | InfraClass::Ambiguous | InfraClass::Clean => None,
        }
    }
}

/// `infra_report/1` — the detector's output on one grader/environment
/// outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct InfraReport {
    /// The detected class.
    pub class: InfraClass,
    /// `infra_suspected` — set when the signature is ambiguous (the run
    /// scores, flagged).
    pub infra_suspected: bool,
    /// The signature evidence (the exit status/stderr fragment classifying —
    /// recorded, never silent).
    pub evidence: String,
}

impl InfraReport {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("infra_report/1")),
            ("class", Json::str(self.class.as_str())),
            ("infra_suspected", Json::Bool(self.infra_suspected)),
            ("evidence", Json::str(&self.evidence)),
        ])
    }
}

/// The detector input — the verifier process's exit signature plus the
/// environment-side error, when one exists. Deterministic over bytes.
pub struct InfraSignal<'a> {
    /// The verifier process exit code (`None` = killed/no exit).
    pub verifier_exit: Option<i32>,
    /// The verifier stderr tail (signature source).
    pub verifier_stderr: &'a str,
    /// Whether the verifier hit its wall-time bound.
    pub verifier_timed_out: bool,
    /// The environment-side error text, when the env itself reported one.
    pub environment_error: Option<&'a str>,
    /// Whether the submission artifact was produced.
    pub submission_produced: bool,
}

/// `detect_infrastructure_failure(signal) → InfraReport` — the closed
/// classifier. Order matters: environment-tier signatures outrank verifier
/// signatures (an env that died takes its verifier down with it), and an
/// *unclassifiable* nonzero exit is `Ambiguous` — scored, flagged.
pub fn detect_infrastructure_failure(sig: &InfraSignal) -> InfraReport {
    // Environment tier first.
    if let Some(e) = sig.environment_error {
        let lower = e.to_ascii_lowercase();
        let class = if lower.contains("pull")
            || lower.contains("image")
            || lower.contains("mount")
            || lower.contains("sandbox")
        {
            InfraClass::EnvironmentSetupFailed
        } else if lower.contains("died") || lower.contains("reset") || lower.contains("exit") {
            InfraClass::EnvironmentDied
        } else {
            InfraClass::Ambiguous
        };
        return InfraReport {
            class,
            infra_suspected: class == InfraClass::Ambiguous,
            evidence: e.chars().take(200).collect(),
        };
    }
    if !sig.submission_produced {
        return InfraReport {
            class: InfraClass::ArtifactMissing,
            infra_suspected: false,
            evidence: "no submission artifact".into(),
        };
    }
    // Verifier tier.
    if sig.verifier_timed_out {
        return InfraReport {
            class: InfraClass::VerifierTimeout,
            infra_suspected: false,
            evidence: "verifier wall-time bound".into(),
        };
    }
    match sig.verifier_exit {
        Some(0) => InfraReport {
            class: InfraClass::Clean,
            infra_suspected: false,
            evidence: String::new(),
        },
        Some(code) => {
            let lower = sig.verifier_stderr.to_ascii_lowercase();
            let class = if lower.contains("refus") {
                InfraClass::VerifierRefusal
            } else if lower.contains("panic") || lower.contains("crash") || code < 0 {
                InfraClass::VerifierCrash
            } else if code == 2 {
                // Exit 2 = the oracle_failure envelope the verifier emitted —
                // a typed failure, not a crash.
                InfraClass::VerifierRefusal
            } else {
                InfraClass::Ambiguous
            };
            InfraReport {
                class,
                infra_suspected: class == InfraClass::Ambiguous,
                evidence: format!("exit={code} stderr={}", &lower[..lower.len().min(160)]),
            }
        }
        None => InfraReport {
            class: InfraClass::VerifierCrash,
            infra_suspected: false,
            evidence: "no exit status (killed)".into(),
        },
    }
}
