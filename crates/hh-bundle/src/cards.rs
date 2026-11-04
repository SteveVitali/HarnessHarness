//! `cards` — the §6.3 isolated-replay and fresh-execution **admission
//! cards** ("Replay / optional R1", "Fresh / optional R2|R3",
//! "Independent replication/transfer"; PADR-R5-3* procedure verdicts;
//! F3's F4/F5 legs). Records-in/records-out owner interfaces over the
//! decoded bundle, beside `closure.rs` — no public RPC (C.4), no
//! provider/effect execution: the disposable copy's event stream and the
//! claim's declaration are the inputs; the live replay driver stays in
//! `hh-embed` (I-02 — one home, never a second driver).
//!
//! **Card separation (§6.3/F1):** each procedure mints its own record —
//! `replay-card/1` here, `fresh-card/1` here, `hh-repro/1`
//! (`repro.rs` — the verify path), `analysis-recompute/1`
//! (`hh-analysis` — the recompute owner). `procedure`/`level` are
//! required members; `prerequisites`, `resources` and `counters` are
//! recorded separately and never fold into `verdict`. A card verdict is
//! `admitted` or `not_admitted{failures[]}` — refusals are typed
//! [`BundleError::CardRefused`], never a silent verdict.

use std::collections::BTreeMap;

use hharness_ledger::event::EventEnvelope;
use hharness_wire::json::Json;

use crate::codec::Decoded;
use crate::error::BundleError;
use crate::export::decode_page;
use crate::levels;
use crate::manifest::{LedgerExport, MemberStatus};
use crate::repro::compute_independence;

/// The isolated-replay card's schema id.
pub const REPLAY_CARD_SCHEMA: &str = "replay-card/1";
/// The fresh-execution card's schema id.
pub const FRESH_CARD_SCHEMA: &str = "fresh-card/1";
/// The only classes a replay may append to the disposable copy — the
/// `lifecycle.replay.*` markers; anything else is suffix confusion or an
/// attempted live dispatch.
pub const REPLAY_MARKER_PREFIX: &str = "lifecycle.replay.";

/// The closed refusal-code set `BundleError::CardRefused` carries (the
/// refusal form for both cards — a named prerequisite/policy gate, never
/// a drift row).
pub mod code {
    /// A required recorded input is absent (definition, compiled inputs,
    /// recorded responses, ledger page, member bytes).
    pub const MISSING_INPUT: &str = "missing_input";
    /// The record carries no declared replay (`control.decision`) — an
    /// unsupported variant is refused, never replayed ad hoc.
    pub const UNSUPPORTED: &str = "unsupported";
    /// Provider transport or effect execution was left enabled — the
    /// attempt counters can't be trusted.
    pub const ISOLATION_DISABLED: &str = "isolation_disabled";
    /// No funded replication — $0 stays, zero dispatch.
    pub const UNFUNDED: &str = "unfunded";
    /// No new subject/calls/cost/grade declared — not a fresh execution.
    pub const NOT_FRESH: &str = "not_fresh";
    /// The reproduction instrument's charging does not retain subject
    /// usage attribution.
    pub const CHARGING_ATTRIBUTION: &str = "charging_attribution";
    /// A required input is unavailable (retired model, absent seeds /
    /// margins / validators / distribution / MatchSpec / comparison).
    pub const UNAVAILABLE: &str = "unavailable";
    /// Signer/installation material does not establish independence —
    /// an author-only clean directory or key never does.
    pub const NOT_INDEPENDENT: &str = "not_independent";
    /// The outside relationship (assistance/authority/attestation) is
    /// undeclared.
    pub const OUTSIDE_RELATIONSHIP: &str = "outside_relationship";
    /// No new phase/attempts declared for a transfer claim.
    pub const NO_NEW_PHASE: &str = "no_new_phase";
    /// An own-task/new-model transfer is a new question, never a
    /// replication of the original claim.
    pub const NEW_QUESTION: &str = "new_question";
}

fn refused(code: &'static str, detail: impl Into<String>) -> BundleError {
    BundleError::CardRefused {
        code,
        detail: detail.into(),
    }
}

/// The card's two-valued verdict — `admitted` only when every leg
/// passed; failure rows name the leg and the rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardVerdict {
    /// Every leg passed.
    Admitted,
    /// At least one leg failed — `failures[]` carries `{leg, code,
    /// detail}` rows.
    NotAdmitted,
}

impl CardVerdict {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            CardVerdict::Admitted => "admitted",
            CardVerdict::NotAdmitted => "not_admitted",
        }
    }
}

/// The level a fresh-execution claim asks admission for (the §6.3 rows —
/// R3 is not cumulative R2; transfer is its own question).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshLevel {
    /// Fresh execution under the pinned basis.
    R2,
    /// Registered seed distribution/design, n ≥ k, MatchSpec, admitted
    /// equivalence/margins, observed fingerprints when unpinned.
    R3,
    /// Independent replication/transfer — distinct installation/signer
    /// plus an actual outside relationship.
    Transfer,
}

impl FreshLevel {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            FreshLevel::R2 => "r2",
            FreshLevel::R3 => "r3",
            FreshLevel::Transfer => "transfer",
        }
    }
}

/// The disabled-boundary isolation report — both legs must be disabled;
/// the declared attempt counts are the recipient shim's own observation
/// and are **cross-checked, never trusted** (the card recounts the
/// appended attempt classes itself and requires agreement).
#[derive(Debug, Clone)]
pub struct Isolation {
    /// Provider transport was disabled for the replay.
    pub provider_transport_disabled: bool,
    /// Effect execution was disabled for the replay.
    pub effect_execution_disabled: bool,
    /// The shim's observed attempted provider dispatches — any nonzero
    /// count fails the claim **even if the transport blocked it** (§6.3:
    /// "any attempted live dispatch fails/refuses").
    pub attempted_provider_dispatches: u64,
    /// The shim's observed attempted effect executions.
    pub attempted_effect_executions: u64,
}

/// A derived object the disposable copy deleted and re-derived — the R1
/// leg's record (`authoritative` names the object: a `present` member
/// address, an envelope `hash`, `version_id`, or the run's ledger
/// `tree`; `rederived` is the hash the copy produced).
#[derive(Debug, Clone)]
pub struct RederivedObject {
    /// The authoritative identity of the deleted derived object.
    pub authoritative: String,
    /// The hash the disposable copy re-derived for it.
    pub rederived: String,
}

/// The disposable copy's replay observation — what the isolated replay
/// left behind (F4: "copy source evidence into disposable scratch;
/// inventory its prefix/head and derivations").
#[derive(Debug, Clone)]
pub struct ReplayAttempt {
    /// The disposable copy's event stream for the subject run — the
    /// original prefix followed by whatever the replay appended.
    pub copy: Vec<EventEnvelope>,
    /// The re-drive's emitted `control.decision` payloads, in order.
    pub replayed_decisions: Vec<Json>,
    /// The re-drive's emitted `context.assembled` payloads, in order.
    pub replayed_assembled: Vec<Json>,
    /// The derived objects deleted in the copy only, with their
    /// re-derived hashes (the R1 leg).
    pub rederived: Vec<RederivedObject>,
    /// The disabled-boundary isolation report.
    pub isolation: Isolation,
    /// `true` claims the optional R1 (delete + re-derive) leg — the
    /// required derived set must then be covered in full.
    pub claim_r1: bool,
}

/// The fresh-execution claim's declaration (F5's preflight + the §6.3
/// Fresh/transfer rows) — every member is a *declared* input; nothing
/// here is inferred and nothing executes.
#[derive(Debug, Clone)]
pub struct FreshClaim {
    /// The level asked: `r2` | `r3` | `transfer`.
    pub level: FreshLevel,
    /// A new subject/calls/cost/grade is declared.
    pub new_subject: bool,
    /// The replication/execution is funded (an exact quote exists) —
    /// `false` refuses, $0 and zero dispatch stay.
    pub funded: bool,
    /// The pinned/served model is retired or otherwise unavailable.
    pub model_retired: bool,
    /// The fresh run's recorded seeds (R2 requires; R3's registered
    /// distribution does not substitute for recording them).
    pub seeds: Vec<i64>,
    /// The registered seed distribution/design ref (R3).
    pub seed_distribution: Option<String>,
    /// The planned arm count `n` and the registered minimum `k` (R3:
    /// `n ≥ k`; a single fresh arm is a demo and awards no level).
    pub planned_arms: u64,
    /// See `planned_arms`.
    pub min_arms: u64,
    /// The declared MatchSpec document (R3 requires one — a missing
    /// MatchSpec blocks the claim).
    pub matchspec: Option<Json>,
    /// The preregistered margins/minimum-n declaration (R2) / the
    /// admitted equivalence margins (R3).
    pub margins: Option<Json>,
    /// The reproduction instrument's charging declaration retains
    /// subject usage attribution (§6.3 Fresh R2).
    pub charging_retains_attribution: bool,
    /// Transfer — a new phase/attempts is declared.
    pub new_phase: bool,
    /// Transfer — the fixed comparison criterion.
    pub comparison_criterion: Option<String>,
    /// Transfer — the fixed comparison target.
    pub comparison_target: Option<String>,
    /// Transfer — the outside-relationship authority/assistance ref.
    pub outside_authority: Option<String>,
    /// Transfer — outside attestation material exists.
    pub outside_attestation: bool,
    /// Transfer — the reproducer's declared `ProvenanceRecord`.
    pub reproducer: Json,
    /// Transfer — the reproducer's declared `InstrumentRecord`.
    pub reproducer_instrument: Json,
    /// Transfer — `true` marks an own-task/new-model transfer: a new
    /// question, never a replication of the original claim.
    pub new_task_or_model: bool,
}

/// The admission card — one procedure, one verdict (§6.3/F1 separation).
#[derive(Debug, Clone)]
pub struct AdmissionCard {
    /// `replay-card/1` | `fresh-card/1`.
    pub schema: &'static str,
    /// `replay` | `fresh` | `transfer` — never a joint procedure.
    pub procedure: &'static str,
    /// `replay` | `r1` | `r2` | `r3` | `transfer`.
    pub level: String,
    /// The subject bundle.
    pub bundle_id: String,
    /// The subject run the card evaluated.
    pub run_id: String,
    /// `admitted` | `not_admitted`.
    pub verdict: CardVerdict,
    /// `{leg, code, detail}` rows for every failed leg.
    pub failures: Vec<Json>,
    /// `leg → pass|fail|n_a{reason}` — one entry per evaluated leg.
    pub legs: BTreeMap<String, String>,
    /// The declared prerequisite state — recorded separately, never
    /// folded into `verdict`.
    pub prerequisites: Json,
    /// The resource/count comparisons — recorded separately.
    pub resources: Json,
    /// The independent attempt counters (replay) — `Json::Null` for
    /// fresh.
    pub counters: Json,
    /// The independence record (transfer) — `Json::Null` otherwise; the
    /// basis is recorded, never asserted.
    pub independence: Json,
    /// `idp/1` over the card minus `card_id`.
    pub card_id: String,
}

impl AdmissionCard {
    /// Canonical JSON (card_id stamped).
    pub fn to_json(&self) -> Json {
        let mut legs = BTreeMap::new();
        for (k, v) in &self.legs {
            legs.insert(k.clone(), Json::str(v.clone()));
        }
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str(self.schema));
        m.insert("procedure".into(), Json::str(self.procedure));
        m.insert("level".into(), Json::str(self.level.clone()));
        m.insert("bundle_id".into(), Json::str(self.bundle_id.clone()));
        m.insert("run_id".into(), Json::str(self.run_id.clone()));
        m.insert("verdict".into(), Json::str(self.verdict.name()));
        m.insert("failures".into(), Json::Arr(self.failures.clone()));
        m.insert("legs".into(), Json::Obj(legs));
        m.insert("prerequisites".into(), self.prerequisites.clone());
        m.insert("resources".into(), self.resources.clone());
        m.insert("counters".into(), self.counters.clone());
        m.insert("independence".into(), self.independence.clone());
        m.insert("card_id".into(), Json::str(self.card_id.clone()));
        Json::Obj(m)
    }

    /// Stamp `card_id` (`idp/1` — the same seal rule `ReproReport` uses).
    pub fn seal(&mut self) {
        self.card_id = String::new();
        let mut doc = match self.to_json() {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        doc.remove("card_id");
        self.card_id = hharness_identity::idp_id(
            "bundle.card",
            Json::Obj(doc).to_canonical_string().as_bytes(),
        );
    }
}

/// A failure row `{leg, code, detail}` — the card's drift vocabulary.
fn failure(leg: &str, code: &str, detail: impl Into<String>) -> Json {
    Json::obj([
        ("leg", Json::str(leg)),
        ("code", Json::str(code)),
        ("detail", Json::str(detail.into())),
    ])
}

/// Provider-dispatch attempt classes — a `model.call.*` row in the copy's
/// appended suffix is an attempted live provider dispatch (§6.3: the
/// attempt itself fails, blocked or not).
fn is_provider_attempt(class: &str) -> bool {
    class.starts_with("model.call.")
}
/// Effect-execution attempt classes — an `action.effect.*` row in the
/// appended suffix is an attempted live effect execution.
fn is_effect_attempt(class: &str) -> bool {
    class.starts_with("action.effect.")
}
/// The recorded-response classes — a `model.call.*` terminal carrying a
/// `response_ref` is a recorded provider response.
fn is_response_row(e: &EventEnvelope) -> bool {
    (e.class == "model.call.attempt.completed"
        || e.class == "model.call.attempt.failed"
        || e.class == "model.call.completed"
        || e.class == "model.call.failed")
        && e.payload
            .get("response_ref")
            .and_then(Json::as_str)
            .map(|r| !r.is_empty())
            .unwrap_or(false)
}

/// The member address of a role (`present`/`fetch` only — `redacted`/`gc`
/// or absent yields `None`; the same rule `levels::member_addr` uses).
fn member_addr(source: &Decoded, role: &str) -> Option<String> {
    source
        .manifest
        .members
        .iter()
        .find(|m| m.role == role && !matches!(m.status, MemberStatus::Redacted | MemberStatus::Gc))
        .map(|m| m.address.clone())
}

/// The decoded, integrity-checked original prefix for `run_id` — the
/// exported ledger pages in declared order. Missing pages decode-fail as
/// `missing_input`; a page that doesn't decode is `malformed`.
fn original_envelopes(source: &Decoded, run_id: &str) -> Result<Vec<EventEnvelope>, BundleError> {
    let export = source.manifest.traces.get(run_id).ok_or_else(|| {
        refused(
            code::MISSING_INPUT,
            format!("no ledger export for run {run_id}"),
        )
    })?;
    let mut out = Vec::new();
    for page in &export.pages {
        match source.members.get(page) {
            Some(bytes) => out.extend(decode_page(bytes).map_err(|e| {
                refused(
                    code::MISSING_INPUT,
                    format!("ledger page {page} does not decode: {e}"),
                )
            })?),
            None => {
                return Err(refused(
                    code::MISSING_INPUT,
                    format!("ledger page {page} bytes absent"),
                ));
            }
        }
    }
    Ok(out)
}

/// `admit_replay` — the §6.3 "Replay / optional R1" admission card over
/// the isolated replay's disposable-copy observation. Refusals are typed
/// `BundleError::CardRefused`; evaluated drift lands `not_admitted` with
/// per-leg failure rows.
///
/// Legs (F4 / §6.3 Replay): `inputs` — definitions, compiled inputs and
/// recorded responses must be carried, the replay must be declared and
/// both isolation flags set (missing → refusal, not a verdict);
/// `prefix` — the original envelopes recompute and chain, and the
/// copy's prefix is field-exact (`original_mutation` covers both a
/// corrupted source and a mutated copy); `suffix` — appended rows are
/// `lifecycle.replay.*` markers only, seq/chained and self-consistent;
/// `counters` — appended `model.call.*`/`action.effect.*` classes are
/// the card's own count of attempted live work and must equal the
/// shim's declared counts and be zero; `decisions` — the emitted
/// decision/assembly sequences equal the recorded ones exactly
/// (`changed_derivation`); `rederivation` — the optional R1 leg: the
/// required derived set (`version_id`, every envelope `hash`, every
/// `present` member address, the run's ledger `tree`) is covered and
/// each re-derived hash recomputes from authoritative bytes. The
/// original stays untouched — the card holds `source` immutable and
/// rederives from it; a mutated or absent original fails/refuses,
/// never a delete-then-fake.
pub fn admit_replay(
    source: &Decoded,
    run_id: &str,
    attempt: &ReplayAttempt,
) -> Result<AdmissionCard, BundleError> {
    let m = &source.manifest;
    let orig = original_envelopes(source, run_id)?;

    // ── inputs (typed refusals — never a verdict) ────────────────────
    for (role, name) in [
        (levels::roles::DEFINITION, "definition"),
        (levels::roles::COMPILED_BUNDLE, "compiled_bundle"),
    ] {
        let addr = member_addr(source, role)
            .ok_or_else(|| refused(code::MISSING_INPUT, format!("{name} member absent")))?;
        if !source.members.contains_key(&addr) {
            return Err(refused(
                code::MISSING_INPUT,
                format!("{name} member bytes absent"),
            ));
        }
    }
    if !orig.iter().any(is_response_row) {
        return Err(refused(
            code::MISSING_INPUT,
            "no recorded provider response rows",
        ));
    }
    if !orig.iter().any(|e| e.class == "control.decision") {
        return Err(refused(
            code::UNSUPPORTED,
            "replay undeclared — the record carries no control.decision",
        ));
    }
    if !attempt.isolation.provider_transport_disabled {
        return Err(refused(code::ISOLATION_DISABLED, "provider_transport"));
    }
    if !attempt.isolation.effect_execution_disabled {
        return Err(refused(code::ISOLATION_DISABLED, "effect_execution"));
    }

    let mut failures: Vec<Json> = Vec::new();
    let mut legs: BTreeMap<String, String> = BTreeMap::new();
    legs.insert("inputs".into(), "pass".into());

    // ── prefix — original integrity + copy prefix agreement ──────────
    let mut leg = "pass";
    let mut prev = String::new();
    let mut source_bad = false;
    for e in &orig {
        if e.recompute_hash() != e.hash || (!prev.is_empty() && e.prev_hash != prev) {
            source_bad = true;
            break;
        }
        prev = e.hash.clone();
    }
    if source_bad {
        leg = "fail";
        failures.push(failure(
            "prefix",
            "original_mutation",
            "original prefix hash/chain does not recompute",
        ));
    } else if attempt.copy.len() < orig.len() {
        leg = "fail";
        failures.push(failure(
            "prefix",
            "original_mutation",
            format!(
                "disposable copy carries {} events under a {}-event prefix",
                attempt.copy.len(),
                orig.len()
            ),
        ));
    } else {
        for (i, e) in orig.iter().enumerate() {
            if attempt.copy[i] != *e {
                leg = "fail";
                failures.push(failure(
                    "prefix",
                    "original_mutation",
                    format!("copy seq {} deviates from the original prefix", e.seq),
                ));
                break;
            }
        }
    }
    legs.insert("prefix".into(), leg.into());

    // ── suffix — permitted markers only; independent attempt counters ─
    let appended = if attempt.copy.len() > orig.len() {
        &attempt.copy[orig.len()..]
    } else {
        &[][..]
    };
    let mut markers: u64 = 0;
    let mut prov_attempts: u64 = 0;
    let mut effect_attempts: u64 = 0;
    // `orig` is guaranteed non-empty — the `inputs` gate required a
    // recorded `control.decision` row.
    let mut suffix_ok = true;
    let mut tip_seq = orig.last().map(|e| e.seq).unwrap_or(0);
    let mut tip_hash = orig.last().map(|e| e.hash.clone()).unwrap_or_default();
    for e in appended {
        if e.class.starts_with(REPLAY_MARKER_PREFIX) {
            markers += 1;
        } else if is_provider_attempt(&e.class) {
            prov_attempts += 1;
        } else if is_effect_attempt(&e.class) {
            effect_attempts += 1;
        } else {
            suffix_ok = false;
            failures.push(failure(
                "suffix",
                "replay_suffix",
                format!("appended non-marker class `{}` (seq {})", e.class, e.seq),
            ));
        }
        if e.seq != tip_seq + 1 || e.prev_hash != tip_hash || e.recompute_hash() != e.hash {
            suffix_ok = false;
            failures.push(failure(
                "suffix",
                "replay_suffix",
                format!("appended row seq {} does not continue the chain", e.seq),
            ));
        }
        tip_seq = e.seq;
        tip_hash = e.hash.clone();
    }
    legs.insert(
        "suffix".into(),
        if suffix_ok { "pass" } else { "fail" }.into(),
    );

    // ── counters — the card's own count vs the shim's declaration ────
    let mut counters_ok = true;
    if attempt.isolation.attempted_provider_dispatches != prov_attempts
        || attempt.isolation.attempted_effect_executions != effect_attempts
    {
        counters_ok = false;
        failures.push(failure(
            "counters",
            "counter_dishonest",
            format!(
                "declared {}/{} vs derived {}/{} attempted dispatches",
                attempt.isolation.attempted_provider_dispatches,
                attempt.isolation.attempted_effect_executions,
                prov_attempts,
                effect_attempts
            ),
        ));
    }
    if prov_attempts > 0 || effect_attempts > 0 {
        counters_ok = false;
        failures.push(failure(
            "counters",
            "attempted_live_dispatch",
            format!(
                "{} provider + {} effect dispatches attempted — an attempt fails even when the transport blocks it",
                prov_attempts, effect_attempts
            ),
        ));
    }
    legs.insert(
        "counters".into(),
        if counters_ok { "pass" } else { "fail" }.into(),
    );

    // ── decisions — declared decision/assembly sequences, exact ──────
    let recorded_decisions: Vec<&Json> = orig
        .iter()
        .filter(|e| e.class == "control.decision")
        .map(|e| &e.payload)
        .collect();
    let recorded_assembled: Vec<&Json> = orig
        .iter()
        .filter(|e| e.class == "context.assembled")
        .map(|e| &e.payload)
        .collect();
    let mut decisions_ok = true;
    if attempt.replayed_decisions.len() != recorded_decisions.len()
        || attempt
            .replayed_decisions
            .iter()
            .zip(recorded_decisions.iter())
            .any(|(a, b)| a.to_canonical_string() != b.to_canonical_string())
    {
        decisions_ok = false;
        failures.push(failure(
            "decisions",
            "changed_derivation",
            "replayed control.decision sequence differs from the record",
        ));
    }
    if attempt.replayed_assembled.len() != recorded_assembled.len()
        || attempt
            .replayed_assembled
            .iter()
            .zip(recorded_assembled.iter())
            .any(|(a, b)| a.to_canonical_string() != b.to_canonical_string())
    {
        decisions_ok = false;
        failures.push(failure(
            "decisions",
            "changed_derivation",
            "replayed context.assembled sequence differs from the record",
        ));
    }
    legs.insert(
        "decisions".into(),
        if decisions_ok { "pass" } else { "fail" }.into(),
    );

    // ── rederivation — the optional R1 leg ────────────────────────────
    // The required derived set: the manifest `version_id`, every envelope
    // `hash`, every `present` member's content address, and the run's
    // ledger `tree` — each recomputed from the authoritative bytes.
    if !attempt.claim_r1 && attempt.rederived.is_empty() {
        legs.insert("rederivation".into(), "n_a{not_claimed}".into());
    } else {
        let mut expected: BTreeMap<String, String> = BTreeMap::new();
        expected.insert("version_id".into(), m.compute_id());
        for e in &orig {
            expected.insert(e.hash.clone(), e.recompute_hash());
        }
        for mm in &m.members {
            if mm.status == MemberStatus::Present {
                match source.members.get(&mm.address) {
                    Some(b) => {
                        expected.insert(mm.address.clone(), hharness_identity::idp_id("blob", b));
                    }
                    None => {
                        return Err(refused(
                            code::MISSING_INPUT,
                            format!("present member {} bytes absent", mm.address),
                        ));
                    }
                }
            }
        }
        if let Some(export) = m.traces.get(run_id) {
            expected.insert(
                export.tree.clone(),
                LedgerExport::tree_address(&export.pages),
            );
        }
        let mut red_ok = true;
        if attempt.claim_r1 {
            for name in expected.keys() {
                if !attempt.rederived.iter().any(|r| &r.authoritative == name) {
                    red_ok = false;
                    failures.push(failure(
                        "rederivation",
                        "rederivation",
                        format!("required derived object {name} not rederived"),
                    ));
                }
            }
        }
        for r in &attempt.rederived {
            match expected.get(&r.authoritative) {
                Some(want) if *want == r.rederived => {}
                Some(want) => {
                    red_ok = false;
                    failures.push(failure(
                        "rederivation",
                        "changed_derivation",
                        format!(
                            "{} rederived {} ≠ authoritative {}",
                            r.authoritative, r.rederived, want
                        ),
                    ));
                }
                None => {
                    red_ok = false;
                    failures.push(failure(
                        "rederivation",
                        "unknown_derived",
                        format!("{} is not a required derived object", r.authoritative),
                    ));
                }
            }
        }
        legs.insert(
            "rederivation".into(),
            if red_ok { "pass" } else { "fail" }.into(),
        );
    }

    let mut card = AdmissionCard {
        schema: REPLAY_CARD_SCHEMA,
        procedure: "replay",
        level: if attempt.claim_r1 {
            "r1".to_string()
        } else {
            "replay".to_string()
        },
        bundle_id: m.version_id.clone(),
        run_id: run_id.to_string(),
        verdict: if failures.is_empty() {
            CardVerdict::Admitted
        } else {
            CardVerdict::NotAdmitted
        },
        failures,
        legs,
        prerequisites: Json::obj([
            ("provider_transport_disabled", Json::Bool(true)),
            ("effect_execution_disabled", Json::Bool(true)),
            ("replay_declared", Json::Bool(true)),
            ("definition_present", Json::Bool(true)),
            ("compiled_inputs_present", Json::Bool(true)),
            ("responses_recorded", Json::Bool(true)),
        ]),
        resources: Json::obj([
            ("prefix_events", Json::Int(orig.len() as i64)),
            (
                "present_members",
                Json::Int(
                    m.members
                        .iter()
                        .filter(|mm| mm.status == MemberStatus::Present)
                        .count() as i64,
                ),
            ),
            (
                "rederived_objects",
                Json::Int(attempt.rederived.len() as i64),
            ),
        ]),
        counters: Json::obj([
            ("appended_markers", Json::Int(markers as i64)),
            (
                "attempted_provider_dispatches",
                Json::Int(prov_attempts as i64),
            ),
            (
                "attempted_effect_executions",
                Json::Int(effect_attempts as i64),
            ),
        ]),
        independence: Json::Null,
        card_id: String::new(),
    };
    card.seal();
    Ok(card)
}

/// `admit_fresh` — the §6.3 "Fresh / optional R2|R3" and "Independent
/// replication/transfer" admission card over a declared claim plus the
/// source bundle's native basis. Funding/independence/prerequisite gates
/// are typed `CardRefused`; evaluated basis/arms shortfalls land
/// `not_admitted` failure rows.
///
/// Gates (§6.3/F5): `unfunded` — no funded replication means zero
/// dispatch; `unavailable{model_retired}`; `not_fresh` — new
/// subject/calls/cost/grade must be declared; `charging_attribution`;
/// per-level required inputs (`seeds`/`margins`/`validators` for R2,
/// `seed_distribution`/`matchspec`/`margins`/`seeds` for R3, the
/// outside-relationship/phase/comparison material for transfer);
/// `new_question` — an own-task/new-model transfer is never a
/// replication claim; `not_independent` — the landed
/// [`compute_independence`] fail-closed basis, so an author-only clean
/// directory or key admits nothing. Legs: `native_basis` — every
/// requirement entry of the level's basis set satisfied through
/// [`levels::derive`] (the one evaluator — CC1); `arms` — R3 requires
/// `n ≥ k` with `k ≥ 2` (one fresh demo awards no level).
pub fn admit_fresh(
    source: &Decoded,
    run_id: &str,
    claim: &FreshClaim,
) -> Result<AdmissionCard, BundleError> {
    let m = &source.manifest;
    // Shared gates — every fresh level.
    if !claim.funded {
        return Err(refused(
            code::UNFUNDED,
            "no funded replication — zero dispatch",
        ));
    }
    if claim.model_retired {
        return Err(refused(code::UNAVAILABLE, "model_retired"));
    }
    if !claim.new_subject {
        return Err(refused(
            code::NOT_FRESH,
            "no new subject/calls/cost/grade declared",
        ));
    }
    if !claim.charging_retains_attribution {
        return Err(refused(
            code::CHARGING_ATTRIBUTION,
            "reproduction charging must retain subject usage attribution",
        ));
    }
    // Level-scoped required inputs — absent means unavailable, never
    // silently optional.
    match claim.level {
        FreshLevel::R2 => {
            if claim.seeds.is_empty() {
                return Err(refused(code::UNAVAILABLE, "seeds"));
            }
            if claim.margins.is_none() {
                return Err(refused(code::UNAVAILABLE, "margins"));
            }
            if m.resolved_dependencies.get("validators").is_none() {
                return Err(refused(code::UNAVAILABLE, "validators"));
            }
        }
        FreshLevel::R3 => {
            if claim.seed_distribution.is_none() {
                return Err(refused(code::UNAVAILABLE, "seed_distribution"));
            }
            if claim.matchspec.is_none() {
                return Err(refused(code::UNAVAILABLE, "matchspec"));
            }
            if claim.margins.is_none() {
                return Err(refused(code::UNAVAILABLE, "margins"));
            }
            if claim.seeds.is_empty() {
                return Err(refused(code::UNAVAILABLE, "seeds"));
            }
        }
        FreshLevel::Transfer => {
            if claim.new_task_or_model {
                return Err(refused(
                    code::NEW_QUESTION,
                    "own-task/new-model transfer is a new question",
                ));
            }
            if claim.outside_authority.is_none() || !claim.outside_attestation {
                return Err(refused(
                    code::OUTSIDE_RELATIONSHIP,
                    "outside authority/assistance/attestation undeclared",
                ));
            }
            if !claim.new_phase {
                return Err(refused(
                    code::NO_NEW_PHASE,
                    "no new phase/attempts declared",
                ));
            }
            if claim.comparison_criterion.is_none() || claim.comparison_target.is_none() {
                return Err(refused(code::UNAVAILABLE, "comparison criterion/target"));
            }
        }
    }

    let mut failures: Vec<Json> = Vec::new();
    let mut legs: BTreeMap<String, String> = BTreeMap::new();
    legs.insert("inputs".into(), "pass".into());

    // ── native_basis — the level's requirement set through the one
    // evaluator (CC1): R2's set for fresh R2 and transfer, R3's own set
    // for R3 (R3 is not cumulative R2).
    let orig = original_envelopes(source, run_id)?;
    let replay_declared = orig.iter().any(|e| e.class == "control.decision");
    let (_, bases) = levels::derive(m, replay_declared);
    let want = match claim.level {
        FreshLevel::R3 => crate::manifest::ReproLevel::R3,
        FreshLevel::R2 | FreshLevel::Transfer => crate::manifest::ReproLevel::R2,
    };
    let mut basis_ok = true;
    for entry in bases.iter().filter(|e| e.level == want) {
        if matches!(
            entry.satisfied_by,
            crate::manifest::BasisSatisfaction::Missing
        ) {
            basis_ok = false;
            failures.push(failure(
                "native_basis",
                "basis_missing",
                format!("requirement {} unsatisfied", entry.requirement_id),
            ));
        }
    }
    legs.insert(
        "native_basis".into(),
        if basis_ok { "pass" } else { "fail" }.into(),
    );

    // ── arms — R3's registered design: n ≥ k, and a single fresh arm is
    // a demo, never a level.
    if claim.level == FreshLevel::R3 {
        let ok = claim.planned_arms >= claim.min_arms && claim.min_arms >= 2;
        if !ok {
            failures.push(failure(
                "arms",
                "insufficient_arms",
                format!(
                    "planned {} vs minimum {} — n ≥ k with k ≥ 2 required; one fresh demo awards no level",
                    claim.planned_arms, claim.min_arms
                ),
            ));
        }
        legs.insert("arms".into(), if ok { "pass" } else { "fail" }.into());
    }

    // ── transfer independence — kernel-computed, fail-closed; the basis
    // is recorded on the card whether the gate passed or not.
    let mut independence = Json::Null;
    if claim.level == FreshLevel::Transfer {
        let (ind, basis) = compute_independence(
            &m.producer,
            &m.instrument,
            &claim.reproducer,
            &claim.reproducer_instrument,
        );
        independence = Json::obj([
            ("independent", Json::Bool(ind)),
            ("basis", Json::str(basis)),
        ]);
        if !ind {
            return Err(refused(code::NOT_INDEPENDENT, basis));
        }
    }

    let procedure = match claim.level {
        FreshLevel::Transfer => "transfer",
        _ => "fresh",
    };
    let mut card = AdmissionCard {
        schema: FRESH_CARD_SCHEMA,
        procedure,
        level: claim.level.name().to_string(),
        bundle_id: m.version_id.clone(),
        run_id: run_id.to_string(),
        verdict: if failures.is_empty() {
            CardVerdict::Admitted
        } else {
            CardVerdict::NotAdmitted
        },
        failures,
        legs,
        prerequisites: Json::obj([
            ("funded", Json::Bool(claim.funded)),
            ("model_available", Json::Bool(!claim.model_retired)),
            ("new_subject", Json::Bool(claim.new_subject)),
            (
                "charging_attribution",
                Json::Bool(claim.charging_retains_attribution),
            ),
            ("seeds_declared", Json::Bool(!claim.seeds.is_empty())),
            (
                "distribution_declared",
                Json::Bool(claim.seed_distribution.is_some()),
            ),
            ("matchspec_declared", Json::Bool(claim.matchspec.is_some())),
            ("margins_declared", Json::Bool(claim.margins.is_some())),
            (
                "outside_authority_declared",
                Json::Bool(claim.outside_authority.is_some()),
            ),
            ("outside_attestation", Json::Bool(claim.outside_attestation)),
            ("new_phase", Json::Bool(claim.new_phase)),
            (
                "comparison_declared",
                Json::Bool(
                    claim.comparison_criterion.is_some() && claim.comparison_target.is_some(),
                ),
            ),
        ]),
        resources: Json::obj([
            ("planned_arms", Json::Int(claim.planned_arms as i64)),
            ("min_arms", Json::Int(claim.min_arms as i64)),
            ("seed_count", Json::Int(claim.seeds.len() as i64)),
        ]),
        counters: Json::Null,
        independence,
        card_id: String::new(),
    };
    card.seal();
    Ok(card)
}
