//! `judge_adapters` — the live `CriticDeclaration{kind = judge}` binding
//! (R4.7e; DF-S1.21-3's live half; BL-17's evidence leg). `link_judge`
//! arms a [`LiveJudge`] — a `JudgePort` whose answers are real provider
//! calls over the lane's `GatewayModel` (the `model_port` machinery — the
//! ticket's own rule: bind at `link` through `GatewayModel`, never a
//! parallel provider abstraction). The offline `RecordedJudge` stays the
//! replay binding: every live consult buffers the durable trail the
//! caller's fenced writer lands — `verification.validator.invoked` +
//! `verification.validator.verdict` rows in the canonical
//! `hharness_verification::events` shape (the `RecordedJudge` fold reads
//! `detector: "judged"`/`validator_ref`/the graded `value` verbatim) plus
//! the `hh.model_io/1` blobs they cite. The port computes; the caller
//! appends — durable-before-visible stays the fence owner's job (the
//! `ModelPort::take_model_io` convention).
//!
//! Link runs the critic's own checks (ADR-0300's calibration fold,
//! ADR-0311's deterministic-first `RedundantJudge`, ADR-0348's
//! independence vector): the snapshot axis is *derived* from the caller's
//! family facts and re-based onto the declaration — a declared
//! `different_family` under a derived `different_snapshot_same_family`
//! is an overclaim, refused (the same rule `admit_judge` runs at
//! emission). `declare_critic` then enforces every remaining minimum —
//! `same_context`, `RedundantJudge`, `GateWithoutCalibration`,
//! `IndependenceBelowMinimum`. `calibration` is folded, never supplied:
//! the caller resolves the `CalibrationRecord` the declaration's
//! `calibration_ref` names plus the fold's now-facts; `None` is the
//! honest uncalibrated state — `report` binds exploratory (`expired` —
//! never-active is never `active`), `gate` refuses at `declare_critic`.
//!
//! `link_judged_pair` arms the two-member [`PanelJudge`] — the ticket's
//! different-family pair (a local lane + a hosted lane). A same-family
//! pair is the independence refusal, surfaced typed at link. The panel's
//! loop-detector answer is the *minimum* decided member confidence —
//! recorded under the bound `validator_ref` with a `panel{members,
//! withheld, decision_rule, dissent_ppm}` member — so `RecordedJudge`
//! replays exactly what the live panel answered; member rows land under
//! each member's own critic ref beside it.
//!
//! `calibrate` runs a declared labelled case set (the benchset's
//! `stage3_v1` corpus halves) over a lane and mints the `CalibrationRecord`
//! — the measured agreement/error facts, never a claimed one (ADR-0347's
//! corpus rule). Uncalibrated bindings stay `exploratory` by the record's
//! honest absence — their consults still land `verification.validator.*`
//! rows (marked `exploratory`) but answer `calibration_ref: None` to the
//! reconciler arm, which declines `reconcile_judged` for them
//! (`JudgedWithoutCalibration`, never a fabricated ref).

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use hharness_control::loops::{JudgePort, JudgedConsult};
use hharness_control::policy::JudgedSpec;
use hharness_identity::idp::{address, idp_id};
use hharness_identity::kinds::RecordKind;
use hharness_identity::refs::VersionedRef;
use hharness_provenance::authority::PersistenceScope;
use hharness_provenance::origin::Origin;
use hharness_provenance::record::ProvenanceRecord;
use hharness_verification::claims::Claim;
use hharness_verification::critic_rt::{calibration_status_at, CalibrationContext};
use hharness_verification::critics::{
    declare_critic, CalibrationAgreement, CalibrationRecord, CriticDeclaration,
};
use hharness_verification::reconciler::JudgedAssessment;
use hharness_verification::validators::{Finding, Verdict};
use hharness_verification::vocab::{
    CalibrationStatus, ChargedTo, CriterionRole, Detector, DivergenceClass, Isolation, OracleCause,
    OracleClass, SeverityLevel, SnapshotIndependence, VerdictPhase, VerdictStatus,
};
use hharness_wire::json::Json;

use hharness_embed_schema::errors::EmbedError;

use crate::context_adapters::AdapterLane;
use crate::gateway_model::GatewayModel;

/// The judge call's output bound — judge answers are small closed-schema
/// JSON (`{"confidence_ppm": N}` / `{"agree": bool, "classes": [...]}` —
/// a few hundred tokens at most).
const JUDGE_MAX_TOKENS: u64 = 512;

/// `now_ms` — wall time for a live consult's `measured_at` (the replay
/// half never re-mints — the recorded rows carry their own stamps).
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `JudgeBindSpec` — what `link_judge` consumes: the declaration, the
/// lane its calls ride (the caller resolves profile → endpoint exactly as
/// `decode_model_lane` does — the adapter never dereferences a ref
/// itself), the resolved rubric text, and the family/calibration facts
/// the link-time checks derive from.
#[derive(Debug, Clone)]
pub struct JudgeBindSpec {
    /// The `CriticDeclaration` to bind (`critic_kind = judge`; the
    /// declaration's `deterministic_coverage_at_placement` member is the
    /// `RedundantJudge` input — ADR-0311's deterministic-first rule).
    pub declaration: CriticDeclaration,
    /// The lane the judge's calls ride (`AdapterLane`'s `model_port`
    /// members).
    pub lane: AdapterLane,
    /// The resolved rubric text (`rubric_ref` names it — resolved by the
    /// caller like `resolved_texts` in `context_adapters`).
    pub rubric_text: String,
    /// The judge's own snapshot family (the `family` fact the bound
    /// `model_snapshots["judge"]` resolves to — the snapshot axis's
    /// numerator).
    pub family: String,
    /// The judge snapshot fingerprint in effect now (the calibration
    /// fold's now-fact — `judge_fingerprint` derives it).
    pub judge_snapshot_fingerprint: String,
    /// Every *other* bound role's resolved family — the snapshot axis's
    /// denominator (the beneficiaries' families, never the caller's claim
    /// about them).
    pub beneficiary_families: Vec<String>,
    /// The folded `(CalibrationRecord, CalibrationContext)` when the
    /// declaration's `calibration_ref` resolves — `None` is the honest
    /// uncalibrated state.
    pub calibration: Option<(CalibrationRecord, CalibrationContext)>,
    /// The binding coordinate `Origin::model`'s `run_ref` member stamps.
    pub run_ref: String,
}

/// `json_bool(v)` — the `Json` accessors cover `as_str`/`as_int`;
/// a bool reads by match.
fn json_bool(v: &Json) -> Option<bool> {
    match v {
        Json::Bool(b) => Some(*b),
        _ => None,
    }
}

/// `judge_fingerprint(lane)` — the judge snapshot fingerprint derivation:
/// the bound model identity ({dialect, provider_model_id, profile_ref}),
/// content-id'd under `judge.snapshot`. The same lane derives the same
/// fingerprint at `calibrate` and at `link` — the fold's change detector
/// is then an identity comparison, never a string guess.
pub fn judge_fingerprint(lane: &AdapterLane) -> String {
    idp_id(
        "judge.snapshot",
        Json::obj([
            ("dialect_ref", Json::str(lane.dialect_ref.clone())),
            (
                "provider_model_id",
                Json::str(lane.provider_model_id.clone()),
            ),
            ("profile_ref", Json::str(lane.profile_ref.clone())),
        ])
        .to_canonical_string()
        .as_bytes(),
    )
}

/// `derive_snapshot_axis(family, beneficiaries)` — the snapshot axis from
/// family facts: the judge's family shared with any beneficiary ⇒
/// `different_snapshot_same_family`; an unrelated family ⇒
/// `different_family` (`same_snapshot` is a declaration fact, never
/// derivable from families alone — a declared `same_snapshot` survives
/// the re-base and `declare_critic` rules on it).
fn derive_snapshot_axis(family: &str, beneficiaries: &[String]) -> SnapshotIndependence {
    if beneficiaries.iter().any(|f| f == family) {
        SnapshotIndependence::DifferentSnapshotSameFamily
    } else {
        SnapshotIndependence::DifferentFamily
    }
}

/// `critic_refusal(e)` — the critic checks' typed refusal rendering (a
/// `CriticError` is a policy refusal — `Refused`, never `InvalidDefinition`).
fn critic_refusal(e: hharness_verification::critics::CriticError) -> EmbedError {
    EmbedError::Refused {
        reason: format!("judge link: declare_critic: {e}"),
    }
}

/// `link_judge(spec)` — arm a live `CriticDeclaration{kind = judge}`
/// binding: the snapshot axis re-based from family facts (an overclaiming
/// declaration refused), the folded `CalibrationRecord`'s status stamped,
/// `declare_critic` run in full (independence, `RedundantJudge`,
/// `GateWithoutCalibration`), the lane armed (dialect + credential
/// coordinate resolved at link — a missing `env:NAME` is a typed
/// refusal here, never a mid-call surprise).
pub fn link_judge(mut spec: JudgeBindSpec) -> Result<LiveJudge, EmbedError> {
    if spec.declaration.critic_kind != hharness_verification::vocab::CriticKind::Judge {
        return Err(EmbedError::InvalidDefinition {
            diagnostics: vec![format!(
                "judge link: critic_kind `{}` is not `judge`",
                spec.declaration.critic_kind.as_str()
            )],
        });
    }
    // Re-base the snapshot axis onto the derived value — the declaration
    // keeps its other axes; a declared axis *stronger* than the
    // derivation is an inflated independence basis (refused, the same
    // rule `admit_judge` enforces).
    let derived = derive_snapshot_axis(&spec.family, &spec.beneficiary_families);
    if spec.declaration.independence.snapshot > derived {
        return Err(EmbedError::Refused {
            reason: format!(
                "judge link: declared snapshot axis `{}` overclaims the derived `{}`",
                spec.declaration.independence.snapshot.as_str(),
                derived.as_str()
            ),
        });
    }
    spec.declaration.independence.snapshot = derived;
    // The calibration fold — never supplied: `calibration_status_at`
    // computes the status; no record is the honest `expired`
    // (never-active is never `active` — report binds exploratory, gate
    // refuses at `declare_critic`). A supplied record the declaration's
    // `calibration_ref` does not name is an inconsistency, surfaced.
    spec.declaration.calibration_status = match &spec.calibration {
        Some((record, ctx)) => {
            if spec.declaration.calibration_ref.as_deref() != Some(record.calibration_id.as_str()) {
                return Err(EmbedError::InvalidDefinition {
                    diagnostics: vec![format!(
                        "judge link: calibration_ref `{:?}` does not name the resolved record `{}`",
                        spec.declaration.calibration_ref, record.calibration_id
                    )],
                });
            }
            calibration_status_at(record, ctx)
        }
        None => CalibrationStatus::Expired,
    };
    declare_critic(&spec.declaration).map_err(critic_refusal)?;
    // Arm the lane: dialect document + credential coordinate resolved at
    // link — the value resolves inside the transport at send, never here.
    let (dialect, _transport) = spec.lane.arm_transport()?;
    let _credential = spec.lane.bind_credential(&dialect.auth_kinds)?;
    Ok(LiveJudge {
        lane: spec.lane,
        critic_ref: spec.declaration.critic_ref.version_id.clone(),
        oracle_id: spec.declaration.oracle_id.clone(),
        rubric: spec.rubric_text,
        family: spec.family,
        charged_to: spec.declaration.charged_to,
        calibration_ref: spec.declaration.calibration_ref.clone(),
        exploratory: spec.declaration.calibration_status != CalibrationStatus::Active,
        independence_summary: spec.declaration.independence.summary(),
        run_ref: spec.run_ref,
        pending: Mutex::new(Vec::new()),
        pending_io: Mutex::new(Vec::new()),
        usages: Mutex::new(Vec::new()),
        last_status: Mutex::new(None),
        call_seq: std::sync::atomic::AtomicU64::new(0),
    })
}

/// `link_judged_pair(a, b)` — the ticket's two-critic binding: two judges
/// from *different* families (a local lane + a hosted lane). A
/// same-family pair is the independence refusal — surfaced typed at
/// link, never weakened post-hoc. Each member links under its own
/// `link_judge` checks.
pub fn link_judged_pair(a: JudgeBindSpec, b: JudgeBindSpec) -> Result<PanelJudge, EmbedError> {
    if a.family == b.family {
        return Err(EmbedError::Refused {
            reason: format!(
                "judge link: a judged pair requires different families — `{}` names both",
                a.family
            ),
        });
    }
    Ok(PanelJudge {
        members: vec![link_judge(a)?, link_judge(b)?],
    })
}

/// `JudgeShot` — one armed call's answer: the decoded text plus the
/// charge/identity members the durable trail and spend accounting read.
struct JudgeShot {
    /// The completed message text ("" on a failed/no-text call).
    text: String,
    /// The terminal error class, when the call failed/refused typed.
    error_class: Option<String>,
    /// `usage{…}` — the normalized usage member.
    usage: Json,
    /// The provider's verbatim `usage` document.
    usage_raw: Json,
    /// The provider-reported served model (drift fact).
    served_model: Option<String>,
    /// The drained `hh.model_io/1` blobs `(media, bytes)`.
    model_io: Vec<(String, Vec<u8>)>,
    /// The content addresses of `model_io` (the refs the verdict cites —
    /// computable from the bytes, never a fabricated pointer).
    io_refs: Vec<String>,
}

/// `call_judge(lane, prompt, call_id)` — one `GatewayModel` one-shot:
/// the judge's prompt is the staged instruction (a single user message —
/// a fresh context per consult, the `context = fresh` axis's runtime
/// face). The dialect-admitted output bound lands like
/// `LaneSummarizer`'s; a call failure answers inside the shot's
/// `error_class`, typed — never a fabricated text.
fn call_judge(lane: &AdapterLane, prompt: &str, call_id: &str) -> Result<JudgeShot, EmbedError> {
    let mut model_lane = lane.model_lane();
    let dialect =
        crate::gateway_model::dialect_by_id(&model_lane.dialect_ref).ok_or_else(|| {
            EmbedError::InvalidDefinition {
                diagnostics: vec![format!(
                    "judge lane dialect `{}` unbindable",
                    model_lane.dialect_ref
                )],
            }
        })?;
    let cap_key = if dialect.plan_schema.admits("max_completion_tokens") {
        "max_completion_tokens"
    } else {
        "max_tokens"
    };
    model_lane
        .provider_params
        .insert(cap_key.to_string(), Json::Int(JUDGE_MAX_TOKENS as i64));
    let mut port = GatewayModel::new(model_lane, prompt.to_string(), None, Vec::new())?;
    let shot = port.one_shot(call_id);
    let io_refs = shot
        .model_io
        .iter()
        .map(|(media, bytes)| address(bytes, media.as_str()).id())
        .collect();
    Ok(JudgeShot {
        text: shot.text,
        error_class: shot.outcome.error_class,
        usage: shot.usage,
        usage_raw: shot.usage_raw,
        served_model: shot.served_model,
        model_io: shot.model_io,
        io_refs,
    })
}

/// `parse_judge_json(text)` — the first balanced `{…}` object in the
/// answer, `Json::parse`d; `None` is the honest unparseable (an
/// `oracle_failure{unparseable}` verdict, never a guessed confidence).
fn parse_judge_json(text: &str) -> Option<Json> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    hharness_wire::json::parse(&text[start..=end]).ok()
}

/// `error_to_oracle_cause(class)` — the wire error class mapped onto the
/// `OracleCause` sum (`timeout`/`refused` keep their names; everything
/// else is `crash` — the provider side failed, not the check).
fn error_to_oracle_cause(class: &str) -> OracleCause {
    let c = class.to_ascii_lowercase();
    if c.contains("timeout") || c.contains("deadline") {
        OracleCause::Timeout
    } else if c.contains("refus") || c.contains("auth") || c.contains("credential") {
        OracleCause::Refused
    } else {
        OracleCause::Crash
    }
}

/// `mint_verdict` — the judge's `Verdict` (delegate-authority mint —
/// `Verdict::expected_authority(Judged)` enforced by `validate`): the
/// canonical record `validator_invoked`/`validator_verdict` render, with
/// `bundle_id` content-anchored on the consult's `model_io` refs and
/// `inputs_digest` folded through the R1 rule (`H(addresses ∥ target ∥
/// validator version_id)` — equal inputs give equal digests).
#[allow(clippy::too_many_arguments)]
fn mint_verdict(
    validator_ref: &VersionedRef,
    target: &str,
    phase: VerdictPhase,
    value: hharness_verification::vocab::VerdictValue,
    status: VerdictStatus,
    findings: Vec<Finding>,
    evidence_refs: Vec<String>,
    io_refs: &[String],
    calibration_ref: Option<String>,
    independence_summary: String,
    charged_to: ChargedTo,
    provenance: ProvenanceRecord,
    measured_at: u64,
    evidence_head_seq: u64,
) -> Verdict {
    let inputs_digest =
        hharness_verification::evidence::inputs_digest(io_refs, target, &validator_ref.version_id);
    let bundle_id = idp_id(
        "verification.bundle",
        Json::obj([
            (
                "handles",
                Json::Arr(io_refs.iter().map(|r| Json::str(r.clone())).collect()),
            ),
            ("omitted", Json::Arr(vec![])),
            ("target", Json::str(target)),
        ])
        .to_canonical_string()
        .as_bytes(),
    );
    let verdict = Verdict {
        verdict_id: idp_id(
            "verification.verdict",
            format!("{}:{}:{}", validator_ref.version_id, target, inputs_digest).as_bytes(),
        ),
        validator_ref: validator_ref.clone(),
        oracle_class: OracleClass::Judge,
        target: target.to_string(),
        criterion_ref: None,
        contract_id: None,
        phase,
        role: CriterionRole::Invariant,
        value,
        status,
        detector: Detector::Judged,
        evidence_refs,
        inputs_digest,
        evidence_head_seq,
        freshness_ok: true,
        findings,
        cost_ppm: 0,
        charged_to,
        veto_tripped: vec![],
        bundle_id: Some(bundle_id),
        calibration_ref,
        independence_summary: Some(independence_summary),
        uncited_findings: 0,
        provenance,
        measured_at,
    };
    debug_assert!(
        verdict.validate().is_ok(),
        "a judged verdict mints at delegate authority"
    );
    verdict
}

/// `LiveJudge` — the armed `CriticDeclaration{kind = judge}` binding: a
/// `JudgePort` over the lane's `GatewayModel`. Consults buffer
/// `(class, payload, provenance)` triples + `hh.model_io/1` blobs the
/// caller drains (`take_records`/`take_model_io`) and lands under the
/// run's fenced writer; the verbatim `usage` members ride `usages` for
/// the leg's spend accounting. A consult that fails typed still mints
/// its `verification.validator.*` rows — `oracle_failure{cause}` is a
/// durable answer, never a silently-dropped call.
#[derive(Debug)]
pub struct LiveJudge {
    lane: AdapterLane,
    /// The bound critic `version_id` — the verdict rows' `validator_ref`.
    critic_ref: String,
    oracle_id: String,
    rubric: String,
    family: String,
    charged_to: ChargedTo,
    /// The resolved `CalibrationRecord` id — `None` is the honest
    /// uncalibrated state (exploratory; never gate evidence).
    calibration_ref: Option<String>,
    /// `calibration_status != active` — the report-only marker stamped on
    /// the emitted rows.
    exploratory: bool,
    independence_summary: String,
    run_ref: String,
    /// Buffered `(class, payload, provenance)` rows for the fenced writer.
    pending: Mutex<Vec<(String, Json, ProvenanceRecord)>>,
    /// Buffered `hh.model_io/1` blobs `(media, bytes)`.
    pending_io: Mutex<Vec<(String, Vec<u8>)>>,
    /// Per-call `{kind, call_id, critic_ref, usage{view,raw}, served_model}` —
    /// the leg's spend accounting reads it verbatim.
    usages: Mutex<Vec<Json>>,
    /// The last consult's verdict status (`PanelJudge`'s `withheld` fold).
    last_status: Mutex<Option<VerdictStatus>>,
    /// The per-judge call counter (verdict/call ids).
    call_seq: std::sync::atomic::AtomicU64,
}

impl LiveJudge {
    /// The bound critic `version_id`.
    pub fn critic_ref(&self) -> &str {
        &self.critic_ref
    }

    /// The declaration's registered oracle id.
    pub fn oracle_id(&self) -> &str {
        &self.oracle_id
    }

    /// The judge's snapshot family.
    pub fn family(&self) -> &str {
        &self.family
    }

    /// The last consult's verdict status tag (`decided`/`inconclusive`/
    /// `oracle_failure`) — `None` before the first call.
    pub fn last_status(&self) -> Option<String> {
        self.last_status
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.kind_tag().to_string())
    }

    /// The per-call usage records (drained view — `{kind, call_id,
    /// critic_ref, usage{view,raw}, served_model}`).
    pub fn usages(&self) -> Vec<Json> {
        self.usages.lock().unwrap().clone()
    }

    /// `buffer_call` — the shared consult tail: usage account, `model_io`
    /// buffering, verdict mint, `invoked`/`verdict` rows buffered for the
    /// fenced writer.
    #[allow(clippy::too_many_arguments)]
    fn buffer_call(
        &self,
        kind: &str,
        call_id: &str,
        shot: JudgeShot,
        target: &str,
        phase: VerdictPhase,
        value: hharness_verification::vocab::VerdictValue,
        status: VerdictStatus,
        findings: Vec<Finding>,
        evidence_refs: Vec<String>,
        evidence_head_seq: u64,
    ) {
        *self.last_status.lock().unwrap() = Some(status);
        self.usages.lock().unwrap().push(Json::obj([
            ("kind", Json::str(kind)),
            ("call_id", Json::str(call_id)),
            ("critic_ref", Json::str(self.critic_ref.clone())),
            ("view", shot.usage.clone()),
            ("raw", shot.usage_raw.clone()),
            (
                "served_model",
                shot.served_model
                    .clone()
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
        ]));
        let now = now_ms();
        let prov = ProvenanceRecord::minted(
            Origin::model(
                self.lane.provider_model_id.clone(),
                self.run_ref.clone(),
                call_id.to_string(),
            ),
            PersistenceScope::Run,
            now,
        );
        let vref =
            VersionedRef::pinned(RecordKind::Validator, self.critic_ref.clone(), prov.clone());
        let verdict = mint_verdict(
            &vref,
            target,
            phase,
            value,
            status,
            findings,
            evidence_refs,
            &shot.io_refs,
            self.calibration_ref.clone(),
            self.independence_summary.clone(),
            self.charged_to,
            prov,
            now,
            evidence_head_seq,
        );
        let mut verdict_payload = hharness_verification::events::validator_verdict(&verdict);
        if self.exploratory {
            if let Json::Obj(m) = &mut verdict_payload {
                m.insert("exploratory".to_string(), Json::Bool(true));
            }
        }
        self.pending_io.lock().unwrap().extend(shot.model_io);
        self.pending.lock().unwrap().push((
            "verification.validator.invoked".to_string(),
            hharness_verification::events::validator_invoked(&verdict, &Isolation::External),
            verdict.provenance.clone(),
        ));
        self.pending.lock().unwrap().push((
            "verification.validator.verdict".to_string(),
            verdict_payload,
            verdict.provenance,
        ));
    }
}

/// `render_loop_prompt` — the loop-detector consult: rubric + the
/// observation window (canonical JSON — data, never surface text) + the
/// closed answer contract.
fn render_loop_prompt(rubric: &str, window: &[Json]) -> String {
    format!(
        "You are a loop-detection judge. Rubric:\n{rubric}\n\n\
         Observations (JSON):\n{}\n\n\
         Answer with exactly one JSON object \
         {{\"confidence_ppm\": N}} — your confidence 0..1000000 that the \
         window exhibits the pattern the rubric names.",
        Json::Arr(window.to_vec()).to_canonical_string()
    )
}

/// `render_assess_prompt` — the reconciler consult: rubric + the claim +
/// its bound authoritative handles + the closed answer contract (closed
/// `DivergenceClass` spellings only — the parse refuses unknown names).
fn render_assess_prompt(rubric: &str, claim: &Claim, evidence: &[Json]) -> String {
    let classes = DivergenceClass::ALL
        .iter()
        .map(|c| c.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "You are a reconciliation judge. Rubric:\n{rubric}\n\n\
         Claim under assessment (JSON):\n{}\n\n\
         Bound authoritative evidence (JSON — closed-schema handles, never \
         the claim's own text):\n{}\n\n\
         Answer with exactly one JSON object \
         {{\"agree\": bool, \"confidence_ppm\": N, \"classes\": [..]}} — \
         `agree` whether the claim holds; `classes` the subset of \
         {classes} the evidence supports (empty when none).",
        claim.to_json().to_canonical_string(),
        Json::Arr(evidence.to_vec()).to_canonical_string()
    )
}

/// `render_case_prompt` — the calibration-case consult: rubric + the
/// rendered labelled case + the binary answer contract.
fn render_case_prompt(rubric: &str, rendered: &str) -> String {
    format!(
        "You are a judgement oracle. Rubric:\n{rubric}\n\n\
         Case (JSON):\n{rendered}\n\n\
         Answer with exactly one JSON object {{\"agree\": bool}} — whether \
         the case satisfies the rubric."
    )
}

impl JudgePort for LiveJudge {
    fn judge(&self, spec: &JudgedSpec, window: &[Json]) -> u64 {
        let seq = self
            .call_seq
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let call_id = format!("judge.{seq}");
        let prompt = render_loop_prompt(&self.rubric, window);
        let head_seq = window
            .iter()
            .filter_map(|w| w.get("seq").and_then(Json::as_int))
            .max()
            .unwrap_or(0)
            .max(0) as u64;
        let evidence_refs: Vec<String> = window
            .iter()
            .filter_map(|w| w.get("event_id").and_then(Json::as_str).map(str::to_string))
            .collect();
        match call_judge(&self.lane, &prompt, &call_id) {
            Ok(shot) => {
                let (value, status, ppm) = match &shot.error_class {
                    Some(class) => (
                        hharness_verification::vocab::VerdictValue::Graded(0),
                        VerdictStatus::OracleFailure(error_to_oracle_cause(class)),
                        0,
                    ),
                    None => match parse_judge_json(&shot.text)
                        .and_then(|d| d.get("confidence_ppm").and_then(Json::as_int))
                    {
                        Some(c) if (0..=1_000_000).contains(&c) => (
                            hharness_verification::vocab::VerdictValue::Graded(c.max(0) as u64),
                            VerdictStatus::Decided,
                            c.max(0) as u64,
                        ),
                        _ => (
                            hharness_verification::vocab::VerdictValue::Graded(0),
                            VerdictStatus::OracleFailure(OracleCause::Unparseable),
                            0,
                        ),
                    },
                };
                let target = spec.validator_ref.clone();
                self.buffer_call(
                    "judge",
                    &call_id,
                    shot,
                    &target,
                    VerdictPhase::Instrument,
                    value,
                    status,
                    vec![],
                    evidence_refs,
                    head_seq,
                );
                ppm
            }
            Err(e) => {
                // An arm-time failure (TLS/credential) still lands its
                // oracle_failure row — the consult happened, the answer is
                // typed, never a silent zero.
                self.buffer_call(
                    "judge",
                    &call_id,
                    JudgeShot {
                        text: String::new(),
                        error_class: Some(format!("{e:?}")),
                        usage: Json::Null,
                        usage_raw: Json::Null,
                        served_model: None,
                        model_io: vec![],
                        io_refs: vec![],
                    },
                    &spec.validator_ref.clone(),
                    VerdictPhase::Instrument,
                    hharness_verification::vocab::VerdictValue::Graded(0),
                    VerdictStatus::OracleFailure(OracleCause::Crash),
                    vec![],
                    evidence_refs,
                    head_seq,
                );
                0
            }
        }
    }

    fn assess(&self, claim: &Claim, evidence: &[Json]) -> Vec<JudgedConsult> {
        let seq = self
            .call_seq
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let call_id = format!("assess.{seq}");
        let prompt = render_assess_prompt(&self.rubric, claim, evidence);
        let shot = match call_judge(&self.lane, &prompt, &call_id) {
            Ok(s) => s,
            Err(e) => JudgeShot {
                text: String::new(),
                error_class: Some(format!("{e:?}")),
                usage: Json::Null,
                usage_raw: Json::Null,
                served_model: None,
                model_io: vec![],
                io_refs: vec![],
            },
        };
        let io_refs = shot.io_refs.clone();
        let (status, confidence, agree, classes) = match &shot.error_class {
            Some(class) => (
                VerdictStatus::OracleFailure(error_to_oracle_cause(class)),
                0u64,
                false,
                vec![],
            ),
            None => match parse_judge_json(&shot.text) {
                Some(d) => {
                    let confidence = d
                        .get("confidence_ppm")
                        .and_then(Json::as_int)
                        .map(|c| c.clamp(0, 1_000_000) as u64)
                        .unwrap_or(0);
                    let agree = d.get("agree").and_then(json_bool).unwrap_or(false);
                    let classes = d
                        .get("classes")
                        .and_then(|c| {
                            if let Json::Arr(a) = c {
                                Some(a.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|c| c.as_str().and_then(DivergenceClass::parse))
                        .collect();
                    (VerdictStatus::Decided, confidence, agree, classes)
                }
                None => (
                    VerdictStatus::OracleFailure(OracleCause::Unparseable),
                    0,
                    false,
                    vec![],
                ),
            },
        };
        let classes: Vec<DivergenceClass> = classes;
        let findings: Vec<Finding> = classes
            .iter()
            .map(|c| Finding {
                code: c.code().to_string(),
                severity: SeverityLevel::Medium,
                location: None,
                message: format!("judged divergence {}", c.as_str()),
                evidence_ref: io_refs.first().cloned(),
            })
            .collect();
        let status_for_buffer = status;
        self.buffer_call(
            "assess",
            &call_id,
            shot,
            &claim.claim_id,
            VerdictPhase::Completion,
            hharness_verification::vocab::VerdictValue::Graded(confidence),
            status_for_buffer,
            findings,
            evidence
                .iter()
                .filter_map(|e| {
                    e.get("handle_ref")
                        .and_then(Json::as_str)
                        .map(str::to_string)
                })
                .collect(),
            claim.at_seq,
        );
        // An oracle-failed consult answers no assessment — its rows land
        // durable (the failure is evidence) but the reconciler arm sees
        // the abstention, never a fabricated assessment.
        if status_for_buffer != VerdictStatus::Decided {
            return Vec::new();
        }
        vec![JudgedConsult {
            judge_ref: self.critic_ref.clone(),
            calibration_ref: self.calibration_ref.clone(),
            independence_summary: self.independence_summary.clone(),
            assessment: JudgedAssessment {
                classes,
                evidence_refs: io_refs,
                confidence_ppm: confidence,
                agree,
            },
        }]
    }

    fn take_records(&self) -> Vec<(String, Json, ProvenanceRecord)> {
        std::mem::take(&mut *self.pending.lock().unwrap())
    }

    fn take_model_io(&self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut *self.pending_io.lock().unwrap())
    }
}

/// `PanelJudge` — the different-family pair's `JudgePort` (R4.7e): the
/// loop-detector answer is the *minimum* decided member confidence —
/// each member consult mints its own `verification.validator.*` rows
/// under its own critic ref; the panel then mints the bound
/// `validator_ref`'s verdict carrying `panel{members, withheld,
/// decision_rule, dissent_ppm}` so `RecordedJudge` replays the panel's
/// own recorded answer byte-identically. A member that fails typed is
/// `withheld`, never a guessed confidence. The reconciler arm returns
/// each member's own `JudgedConsult` — per-member `claim.reconciled`
/// rows under per-member `detector_ref`s.
#[derive(Debug)]
pub struct PanelJudge {
    members: Vec<LiveJudge>,
}

impl PanelJudge {
    /// The bound critics' refs (member order).
    pub fn critic_refs(&self) -> Vec<String> {
        self.members
            .iter()
            .map(|m| m.critic_ref().to_string())
            .collect()
    }

    /// The members' families (member order).
    pub fn families(&self) -> Vec<String> {
        self.members
            .iter()
            .map(|m| m.family().to_string())
            .collect()
    }

    /// The aggregated per-call usage records (the leg's spend accounting).
    pub fn usages(&self) -> Vec<Json> {
        self.members.iter().flat_map(|m| m.usages()).collect()
    }
}

impl JudgePort for PanelJudge {
    fn judge(&self, spec: &JudgedSpec, window: &[Json]) -> u64 {
        let mut answers: Vec<(String, u64)> = Vec::new();
        let mut withheld: Vec<String> = Vec::new();
        for member in &self.members {
            let ppm = member.judge(spec, window);
            if member.last_status().as_deref() == Some("decided") {
                answers.push((member.critic_ref.clone(), ppm));
            } else {
                withheld.push(member.critic_ref.clone());
            }
        }
        // The panel answer: the *minimum* decided confidence — the
        // panel's claim is never stronger than its weakest member. No
        // decided member is a whole-panel abstention.
        let panel_ppm = answers.iter().map(|(_, p)| *p).min().unwrap_or(0);
        let decided = answers.len() as u64;
        let dissent_ppm = if decided > 0 {
            (answers
                .iter()
                .filter(|(_, p)| *p < spec.confidence_threshold_ppm)
                .count() as u64)
                .saturating_mul(1_000_000)
                / decided
        } else {
            0
        };
        // The panel verdict under the bound `validator_ref` — the
        // `RecordedJudge` key — carrying the reduction's members; a
        // `detector: judged` verdict mints at delegate authority (its
        // answer inherits the members' delegate provenance).
        let seq = self
            .members
            .iter()
            .map(|m| m.call_seq.load(std::sync::atomic::Ordering::Relaxed))
            .max()
            .unwrap_or(0);
        let now = now_ms();
        let prov = ProvenanceRecord::minted(
            Origin::model(
                format!(
                    "panel:{}",
                    answers
                        .iter()
                        .map(|(r, _)| r.as_str())
                        .collect::<Vec<_>>()
                        .join("+")
                ),
                "panel".to_string(),
                format!("judge.panel.{seq}"),
            ),
            PersistenceScope::Run,
            now,
        );
        let vref = VersionedRef::pinned(
            RecordKind::Validator,
            spec.validator_ref.clone(),
            prov.clone(),
        );
        let status = if answers.is_empty() {
            VerdictStatus::OracleFailure(OracleCause::Crash)
        } else {
            VerdictStatus::Decided
        };
        let verdict = mint_verdict(
            &vref,
            &spec.validator_ref,
            VerdictPhase::Instrument,
            hharness_verification::vocab::VerdictValue::Graded(panel_ppm),
            status,
            vec![],
            window
                .iter()
                .filter_map(|w| w.get("event_id").and_then(Json::as_str).map(str::to_string))
                .collect(),
            &[],
            None,
            format!(
                "panel members={} withheld={} decision_rule=min",
                answers.len(),
                withheld.len()
            ),
            ChargedTo::Instrument,
            prov.clone(),
            now,
            window
                .iter()
                .filter_map(|w| w.get("seq").and_then(Json::as_int))
                .max()
                .unwrap_or(0)
                .max(0) as u64,
        );
        let mut payload = hharness_verification::events::validator_verdict(&verdict);
        if let Json::Obj(m) = &mut payload {
            m.insert(
                "panel".to_string(),
                Json::obj([
                    (
                        "members",
                        Json::Arr(answers.iter().map(|(r, _)| Json::str(r.clone())).collect()),
                    ),
                    (
                        "withheld",
                        Json::Arr(withheld.iter().map(|r| Json::str(r.clone())).collect()),
                    ),
                    ("decision_rule", Json::str("min")),
                    ("dissent_ppm", Json::Int(dissent_ppm as i64)),
                ]),
            );
        }
        let _ = status;
        // The panel row buffers on the first member's drain queue — the
        // caller drains the panel's aggregate buffers, member rows
        // included, through `take_records`.
        if let Some(m) = self.members.first() {
            m.pending.lock().unwrap().push((
                "verification.validator.verdict".to_string(),
                payload,
                verdict.provenance,
            ));
        }
        panel_ppm
    }

    fn assess(&self, claim: &Claim, evidence: &[Json]) -> Vec<JudgedConsult> {
        self.members
            .iter()
            .flat_map(|m| m.assess(claim, evidence))
            .collect()
    }

    fn take_records(&self) -> Vec<(String, Json, ProvenanceRecord)> {
        self.members.iter().flat_map(|m| m.take_records()).collect()
    }

    fn take_model_io(&self) -> Vec<(String, Vec<u8>)> {
        self.members
            .iter()
            .flat_map(|m| m.take_model_io())
            .collect()
    }
}

/// `CalibrationCase` — one labelled calibration case (the benchset
/// `stage3_v1` corpus half): the caller renders the case text; the gold
/// `label_agree` is the reference oracle's answer (deterministic —
/// `ReferenceOracle::Executable`, never a judge).
#[derive(Debug, Clone)]
pub struct CalibrationCase {
    /// The case id.
    pub case_id: String,
    /// The rendered case text the judge assesses.
    pub rendered: String,
    /// The gold label — the case satisfies the rubric.
    pub label_agree: bool,
    /// The verdict class the case belongs to (`n_per_verdict_class`
    /// accounting).
    pub verdict_class: String,
}

/// `CalibrationRun` — the `calibrate` fold's product: the minted
/// `CalibrationRecord`, the per-case observations (the leg's evidence
/// view), and the per-call usage records (the spend accounting).
#[derive(Debug)]
pub struct CalibrationRun {
    /// The minted record (`status: active` at mint — `calibration_status_at`
    /// owns every later status).
    pub record: CalibrationRecord,
    /// `{case_id, label_agree, answered_agree, status}` per case.
    pub observations: Vec<Json>,
    /// `{kind: "calibrate", call_id, critic_ref, view, raw, served_model}`
    /// per call — verbatim provider usage.
    pub usages: Vec<Json>,
}

/// `calibrate(lane, …)` — run the declared labelled set over the lane and
/// mint the `CalibrationRecord` (ADR-0300; the ticket's `judge-calibrate`
/// leg). Each case is judged twice — the second pass measures
/// self-consistency (`position_consistency_ppm` reports it — the only
/// consistency a single-order non-comparative judge exhibits, labelled as
/// `consistency_basis` in `error_rates`, never an unmeasured claim).
/// Agreement folds as Cohen's κ over the binary labels; a degenerate
/// margin (κ undefined) records `statistic: "raw_agreement"` with the
/// measured value — never a fabricated statistic.
#[allow(clippy::too_many_arguments)]
pub fn calibrate(
    lane: &AdapterLane,
    critic_ref: &str,
    labelled_set_ref: &str,
    cases: &[CalibrationCase],
    rubric_text: &str,
    rubric_ref: &str,
    task_family: &str,
    now_ms_at: u64,
) -> Result<CalibrationRun, EmbedError> {
    if cases.is_empty() {
        return Err(EmbedError::InvalidDefinition {
            diagnostics: vec!["calibrate: an empty labelled set measures nothing".to_string()],
        });
    }
    let mut observations = Vec::new();
    let mut usages = Vec::new();
    let mut seq = 0u64;
    let mut tp = 0u64;
    let mut fp = 0u64;
    let mut fn_ = 0u64;
    let mut tn = 0u64;
    let mut consistent = 0u64;
    for case in cases {
        let prompt = render_case_prompt(rubric_text, &case.rendered);
        let mut answers: Vec<Option<bool>> = Vec::new();
        for pass in 0..2 {
            let call_id = format!("calibrate.{seq}.{pass}");
            seq += 1;
            let shot = call_judge(lane, &prompt, &call_id)?;
            usages.push(Json::obj([
                ("kind", Json::str("calibrate")),
                ("call_id", Json::str(call_id)),
                ("critic_ref", Json::str(critic_ref)),
                ("view", shot.usage),
                ("raw", shot.usage_raw),
                (
                    "served_model",
                    shot.served_model.map(Json::str).unwrap_or(Json::Null),
                ),
            ]));
            let answered = match &shot.error_class {
                Some(_) => None,
                None => {
                    parse_judge_json(&shot.text).and_then(|d| d.get("agree").and_then(json_bool))
                }
            };
            answers.push(answered);
        }
        if answers.len() == 2 && answers[0].is_some() && answers[0] == answers[1] {
            consistent += 1;
        }
        let answered = answers[0];
        match (case.label_agree, answered) {
            (true, Some(true)) => tp += 1,
            (false, Some(true)) => fp += 1,
            (true, Some(false)) => fn_ += 1,
            (false, Some(false)) => tn += 1,
            (_, None) => {}
        }
        observations.push(Json::obj([
            ("case_id", Json::str(case.case_id.clone())),
            ("label_agree", Json::Bool(case.label_agree)),
            (
                "answered_agree",
                answered.map(Json::Bool).unwrap_or(Json::Null),
            ),
            (
                "status",
                Json::str(if answered.is_some() {
                    "decided"
                } else {
                    "oracle_failure"
                }),
            ),
        ]));
    }
    let n = (tp + fp + fn_ + tn) as f64;
    let agreement_ppm = if n > 0.0 {
        (((tp + tn) as f64 / n) * 1_000_000.0).round() as u64
    } else {
        0
    };
    // Cohen's κ over the binary margins; a degenerate margin records the
    // raw agreement honestly.
    let (statistic, value_ppm) = if n > 0.0 {
        let po = (tp + tn) as f64 / n;
        let pe = (((tp + fp) as f64) * ((tp + fn_) as f64)
            + ((fn_ + tn) as f64) * ((fp + tn) as f64))
            / (n * n);
        if (1.0 - pe).abs() > f64::EPSILON {
            let kappa = ((po - pe) / (1.0 - pe)).clamp(-1.0, 1.0);
            ("cohen_kappa", (kappa * 1_000_000.0).round().max(0.0) as u64)
        } else {
            ("raw_agreement", agreement_ppm)
        }
    } else {
        ("raw_agreement", 0)
    };
    let mut class_counts: BTreeMap<&str, u64> = BTreeMap::new();
    for case in cases {
        *class_counts.entry(case.verdict_class.as_str()).or_default() += 1;
    }
    let self_consistency = (consistent as f64 / cases.len() as f64 * 1_000_000.0).round() as u64;
    let record = CalibrationRecord {
        calibration_id: idp_id(
            "verification.calibration",
            Json::obj([
                ("critic_ref", Json::str(critic_ref)),
                ("labelled_set_ref", Json::str(labelled_set_ref)),
                ("at", Json::Int(now_ms_at as i64)),
            ])
            .to_canonical_string()
            .as_bytes(),
        ),
        critic_ref: critic_ref.to_string(),
        judge_snapshot_fingerprint: judge_fingerprint(lane),
        labelled_set_ref: labelled_set_ref.to_string(),
        n_per_verdict_class: class_counts.values().copied().min().unwrap_or(0),
        reference_oracle: hharness_verification::vocab::ReferenceOracle::Executable,
        agreement: CalibrationAgreement {
            statistic: statistic.to_string(),
            value_ppm,
            interval: "point".to_string(),
            method: format!("two-class labelled agreement over {} cases", cases.len()),
        },
        error_rates: Json::obj([
            (
                "fn_rate",
                Json::Int(if tp + fn_ > 0 {
                    (fn_ * 1_000_000) / (tp + fn_)
                } else {
                    0
                } as i64),
            ),
            (
                "fp_rate",
                Json::Int(if fp + tn > 0 {
                    (fp * 1_000_000) / (fp + tn)
                } else {
                    0
                } as i64),
            ),
            ("uncited_finding_rate", Json::Int(0)),
            (
                "consistency_basis",
                Json::str("self_consistency(single-order, non-comparative)"),
            ),
        ]),
        position_consistency_ppm: self_consistency,
        self_consistency_ppm: Some(self_consistency),
        self_preference_check: None,
        evidence_ablation_ppm: None,
        calibrated_at: now_ms_at,
        expiry: vec![
            format!("rubric_version:{rubric_ref}"),
            format!("task_family:{task_family}"),
        ],
        status: CalibrationStatus::Active,
    };
    Ok(CalibrationRun {
        record,
        observations,
        usages,
    })
}
