//! `hh-confirm-withheld/1` — the withheld confirmation disposition
//! record (R5.48; GATE-G14 PASSED — **quote-only confirmation**;
//! evidence spec §4.1/§4.3 and §9.2's confirmation row: "After all
//! gates, ≤300 subjects under same individual caps; ≤900 core-hours/
//! 200 GiB/14 days" — quote-only per the H8-declared worksheet;
//! full actual parity absent at GATE-G13 N=0 → dependent
//! confirmation blocked by the contract's own dependency).
//!
//! Records-in only: every leg evaluator folds *supplied* facts —
//! the gate's verbatim decision, the re-materialized manifest
//! identities, the refusal spellings the landed `central_protocol`/
//! `parity_admission` validators produced on exercised decls, the
//! quote packet's confirmation leg, the absent-tranche counts and
//! the upstream `hh-parity-n0/1` disposition — and never
//! manufactures a subject, an admission, an execution or a
//! verdict. `withheld` is the honest terminal state of the
//! quote-only admission; a definitive failure folds `failed`, an
//! undetermined leg `inconclusive` — never a fabricated cohort.

use std::collections::BTreeMap;

use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;

use crate::preflight::{Leg, LegVerdict};

/// The record version.
pub const VERSION: &str = "hh-confirm-withheld/1";
const DOMAIN: &str = "confirm_withheld";

/// The gate whose quote-only decision this packet records.
pub const GATE: &str = "GATE-G14";

/// The six legs (frozen vocabulary — the withheld scope).
pub const LEGS: [&str; 6] = [
    "gate_readout",
    "manifest_frozen",
    "admission_vocabulary",
    "quote_worksheet",
    "tranche_absent",
    "parity_blocked",
];

// ── leg 1 — gate_readout ─────────────────────────────────────────

/// GATE-G14's verbatim decision facts (the readout's signature
/// block — recorded, never re-derived).
#[derive(Debug, Clone, PartialEq)]
pub struct GateFacts {
    /// The recorded decision (`PASSED — quote-only confirmation`).
    pub decision: String,
    /// The operator's words verbatim
    /// (`confirmed, please continue fully autonomously`).
    pub operator_words: String,
    /// The decision UTC (`2026-11-04T08:04:17Z`).
    pub decided_at: String,
    /// The confirmed agent-drafted block sha256 prefix.
    pub block_sha256_prefix: String,
    /// The S4 disposition (`unmet` — stays unmet honestly).
    pub s4: String,
}

/// `evaluate_gate` — the verbatim decision check: PASSED with a
/// quote-only admission (a funded tranche is a different admission
/// and a different packet), nonempty operator words/decided-at, a
/// 12-hex block prefix and S4 unmet; anything else refuses, never
/// relabels.
pub fn evaluate_gate(f: &GateFacts) -> LegVerdict {
    if !f.decision.starts_with("PASSED") || !f.decision.contains("quote-only") {
        return LegVerdict::Refused(format!("gate_decision:{}", f.decision));
    }
    if f.operator_words.is_empty() || f.decided_at.is_empty() {
        return LegVerdict::Refused("gate_verbatim_absent".to_string());
    }
    let ok_prefix = f.block_sha256_prefix.len() == 12
        && f.block_sha256_prefix.chars().all(|c| c.is_ascii_hexdigit());
    if !ok_prefix {
        return LegVerdict::Refused(format!("block_sha256_prefix:{}", f.block_sha256_prefix));
    }
    if f.s4 != "unmet" {
        return LegVerdict::Refused(format!("s4:{}", f.s4));
    }
    LegVerdict::Verified
}

// ── leg 2 — manifest_frozen ──────────────────────────────────────

/// The frozen §4.1 identities (the R5.41 `central-protocol/1`
/// materialization's fixed shape — echoed as literals so this crate
/// adds no dependency; the battery asserts them against the landed
/// `central_protocol` constants).
pub const FROZEN_TASKS: u64 = 30;
/// `TASKS × 2 arms × 5 rounds` interleaved slots.
pub const FROZEN_SLOTS: u64 = 300;
/// The round-0 first-slot bindings (`TASKS × 2 arms`).
pub const FROZEN_FIRST_SLOTS: u64 = 60;
/// The six phase identities (diagnostic/activation/pilot/parity/
/// confirmation/replication).
pub const FROZEN_PHASES: u64 = 6;

/// The re-materialized manifest facts — the frozen decl replayed
/// through the landed `materialize_protocol` and folded, never
/// re-picked.
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestFacts {
    /// `check_id()` — `protocol_id` recomputes byte-identically.
    pub check_id: bool,
    /// The recomputed `protocol_id` (echoed verbatim — never
    /// hard-coded).
    pub protocol_id: String,
    /// The selected task count (`30`).
    pub selected_tasks: u64,
    /// The interleaved slot count (`300`).
    pub schedule_slots: u64,
    /// The first-slot bindings (`60`).
    pub first_slots: u64,
    /// The phase-identity count (`6`).
    pub phase_identities: u64,
    /// `confirmation` among the phase identities.
    pub confirmation_phase: bool,
}

/// `evaluate_manifest` — the frozen-manifest check: `check_id`
/// holds and every recomputed identity equals its frozen literal.
/// A drifted count is a different manifest — refused, never a
/// retrospective re-pick; an empty `protocol_id` is inconclusive.
pub fn evaluate_manifest(f: &ManifestFacts) -> LegVerdict {
    if f.protocol_id.is_empty() || !f.protocol_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return LegVerdict::Inconclusive("protocol_id_absent".to_string());
    }
    if !f.check_id {
        return LegVerdict::Refused("protocol_id_mismatch".to_string());
    }
    for (member, have, need) in [
        ("selected_tasks", f.selected_tasks, FROZEN_TASKS),
        ("schedule_slots", f.schedule_slots, FROZEN_SLOTS),
        ("first_slots", f.first_slots, FROZEN_FIRST_SLOTS),
        ("phase_identities", f.phase_identities, FROZEN_PHASES),
    ] {
        if have != need {
            return LegVerdict::Refused(format!("{member}:{have}"));
        }
    }
    if !f.confirmation_phase {
        return LegVerdict::Refused("confirmation_phase_absent".to_string());
    }
    LegVerdict::Verified
}

// ── leg 3 — admission_vocabulary ─────────────────────────────────

/// The exercised decl set — `exercise → expected typed refusal
/// name` (the landed `central_protocol`/`parity_admission`
/// validators' verbatim spellings, driven on declared inputs only:
/// nothing executes and nothing is admitted). The protocol legs
/// cover the §9.2 stop clause's integrity vocabulary (drift, late
/// split, post-subject change, smaller-manifest, pilot relabel,
/// outcome-selected exclusion); the admission legs cover the
/// standing parity-admission refusals.
pub const EXERCISES: [(&str, &str); 8] = [
    ("drift_observed", "Drift"),
    ("late_split", "LateSplit"),
    ("post_subject_amendment", "ChangedAfterSubjects"),
    ("retrospective_smaller_manifest", "QuotaMismatch"),
    ("pilot_relabelled_confirmation", "PilotRelabel"),
    ("outcome_selected_exclusion", "OutcomeSelectedExclusion"),
    ("forbidden_use_headline", "UnsupportedClaim"),
    ("inauthentic_recorded_model_io", "InauthenticEvidence"),
];

/// `evaluate_vocabulary` — the machinery-stands check: every
/// declared exercise produced its typed refusal verbatim. A missing
/// exercise is `inconclusive`; a wrong refusal — or `admitted`/
/// `materialized`, the machinery broken — is `refused`, never
/// reclassified.
pub fn evaluate_vocabulary(observed: &BTreeMap<String, String>) -> LegVerdict {
    for (exercise, expected) in EXERCISES {
        match observed.get(exercise).map(String::as_str) {
            Some(o) if o == expected => {}
            Some("admitted") | Some("materialized") => {
                return LegVerdict::Refused(format!("admitted:{exercise}"));
            }
            Some(o) => return LegVerdict::Refused(format!("{exercise}:{o}")),
            None => return LegVerdict::Inconclusive(format!("exercise_absent:{exercise}")),
        }
    }
    LegVerdict::Verified
}

// ── leg 4 — quote_worksheet ──────────────────────────────────────

/// The parity prerequisite name — its unmet state is the
/// confirmation leg's `blocked{parity_absent}` encoding
/// (`prerequisite:parity` verbatim from the landed quote packet).
pub const PARITY_BLOCKER: &str = "prerequisite:parity";

/// The confirmation leg of the landed quote packet over the
/// declared §9.2 minimum-admission worksheet with the parity
/// prerequisite honestly `unmet` (absent at N=0).
#[derive(Debug, Clone, PartialEq)]
pub struct QuoteFacts {
    /// The confirmation stage's dispatch (`not_requested` —
    /// quote-only, no allocated tranche).
    pub confirmation_dispatch: String,
    /// The confirmation stage's state (`blocked` — parity absent).
    pub confirmation_state: String,
    /// The confirmation stage's blockers verbatim — exactly
    /// [`PARITY_BLOCKER`]: the declared envelopes satisfy every
    /// other readiness predicate, so the absent parity prerequisite
    /// is the sole block.
    pub blockers: Vec<String>,
    /// The confirmation stage's precision state (`n/a` — non-parity
    /// stages carry no parity diagnostic).
    pub precision_state: String,
    /// The confirmation stage's quoted policies (both frozen arms).
    pub policies: Vec<String>,
    /// The packet total exposure in currency micros (`0`).
    pub total_micros: i64,
    /// Forbidden constructs observed (top-up / pooled capacity /
    /// cloud fallback / guessed margin / outcome-conditioned) —
    /// must be empty.
    pub forbidden: Vec<String>,
}

/// `evaluate_quote` — the confirmation leg stays `not_requested`
/// with `prerequisite:parity` its sole blocker: a requested or
/// gate-pending dispatch, a ready/infeasible stage, any blocker
/// besides the absent parity prerequisite, priced exposure or a
/// forbidden construct each refuse.
pub fn evaluate_quote(f: &QuoteFacts) -> LegVerdict {
    if f.confirmation_dispatch != "not_requested" {
        return LegVerdict::Refused(format!("confirmation_dispatch:{}", f.confirmation_dispatch));
    }
    if f.confirmation_state != "blocked" {
        return LegVerdict::Refused(format!("confirmation_state:{}", f.confirmation_state));
    }
    if f.blockers != [PARITY_BLOCKER.to_string()] {
        return LegVerdict::Refused(format!("blockers:{}", f.blockers.join(",")));
    }
    if f.precision_state != "n/a" {
        return LegVerdict::Refused(format!("precision_state:{}", f.precision_state));
    }
    let mut policies = f.policies.clone();
    policies.sort();
    if policies != ["clear_tool_results", "evict_oldest"] {
        return LegVerdict::Refused(format!("policies:{}", policies.join(",")));
    }
    if f.total_micros != 0 || !f.forbidden.is_empty() {
        return LegVerdict::Refused(format!(
            "exposure:{} forbidden:{}",
            f.total_micros,
            f.forbidden.join(",")
        ));
    }
    LegVerdict::Verified
}

// ── leg 5 — tranche_absent ───────────────────────────────────────

/// The absent tranche, verbatim — every count a declared zero, no
/// dispatch attempted, partial results/costs retained-but-empty
/// (the §9.2 stop clause's retention is the record) and no
/// retrospective smaller manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct TrancheFacts {
    /// `member → count` — `subjects_first`, `attempts`, `regrades`,
    /// `receipts_collected` each declared `0`.
    pub counts: BTreeMap<String, u64>,
    /// A confirmation dispatch was attempted (`false`).
    pub dispatch_attempted: bool,
    /// The partial-results retention (`retained_empty` — nothing
    /// collected, nothing fabricated, nothing lost).
    pub partial_results: String,
    /// The partial-costs retention (`retained_empty`).
    pub partial_costs: String,
    /// A retrospective smaller/re-picked manifest was substituted
    /// (`false` — the frozen 30-task cohort stands).
    pub retrospective_manifest: bool,
}

/// `evaluate_tranche` — the absent-tranche check: every count zero,
/// no dispatch, partials retained-but-empty, no retrospective
/// manifest. Any nonzero fact is a real tranche's evidence — it
/// belongs in a different packet and refuses here, never a
/// fabricated absence.
pub fn evaluate_tranche(f: &TrancheFacts) -> LegVerdict {
    for member in [
        "subjects_first",
        "attempts",
        "regrades",
        "receipts_collected",
    ] {
        match f.counts.get(member) {
            Some(0) => {}
            Some(n) => return LegVerdict::Refused(format!("{member}:{n}")),
            None => return LegVerdict::Inconclusive(format!("{member}_absent")),
        }
    }
    if f.dispatch_attempted {
        return LegVerdict::Refused("dispatch_attempted:true".to_string());
    }
    if f.partial_results != "retained_empty" {
        return LegVerdict::Refused(format!("partial_results:{}", f.partial_results));
    }
    if f.partial_costs != "retained_empty" {
        return LegVerdict::Refused(format!("partial_costs:{}", f.partial_costs));
    }
    if f.retrospective_manifest {
        return LegVerdict::Refused("retrospective_manifest:true".to_string());
    }
    LegVerdict::Verified
}

// ── leg 6 — parity_blocked ───────────────────────────────────────

/// The upstream `hh-parity-n0/1` facts — the declared N=0 withheld
/// parity record (a supplied fact, never re-derived):
/// non-equivalence/inconclusive/absent parity blocks dependent
/// confirmation per the contract's own dependency.
#[derive(Debug, Clone, PartialEq)]
pub struct ParityFacts {
    /// The upstream record version (`hh-parity-n0/1`).
    pub upstream_version: String,
    /// The upstream disposition (`withheld`).
    pub parity_disposition: String,
    /// The upstream admission shape (`n0_withheld`).
    pub parity_admission: String,
    /// The upstream declared tranche size (`0`).
    pub n_declared: u64,
    /// S3 stays unmet upstream (`true`).
    pub s3_unmet: bool,
    /// The upstream `confirmation_blocked` member (`true`).
    pub confirmation_blocked: bool,
    /// The confirmation leg's quote blocker
    /// (`prerequisite:parity` — the same fact encoded by the
    /// engine).
    pub prerequisite_blocker: String,
    /// An equivalence claim anywhere (`false`).
    pub equivalence_claimed: bool,
    /// A non-equivalence claim (`false` — withheld is not a
    /// verdict of non-equivalence).
    pub non_equivalence_claimed: bool,
}

/// `evaluate_parity` — the dependent-block check: the upstream
/// N=0 withheld record stands (`withheld`/`n0_withheld`/
/// `n_declared:0`/`s3_unmet`/`confirmation_blocked`), the quote
/// blocker is `prerequisite:parity` and no claim of either sign is
/// present. A different upstream state is a different record —
/// refused, never relabelled.
pub fn evaluate_parity(f: &ParityFacts) -> LegVerdict {
    if f.upstream_version != "hh-parity-n0/1" {
        return LegVerdict::Refused(format!("upstream:{}", f.upstream_version));
    }
    if f.parity_disposition != "withheld" || f.parity_admission != "n0_withheld" {
        return LegVerdict::Refused(format!(
            "parity:{}/{}",
            f.parity_disposition, f.parity_admission
        ));
    }
    if f.n_declared != 0 {
        return LegVerdict::Refused(format!("n_declared:{}", f.n_declared));
    }
    if !f.s3_unmet || !f.confirmation_blocked {
        return LegVerdict::Refused("upstream_block_unrecorded".to_string());
    }
    if f.prerequisite_blocker != PARITY_BLOCKER {
        return LegVerdict::Refused(format!("blocker:{}", f.prerequisite_blocker));
    }
    if f.equivalence_claimed || f.non_equivalence_claimed {
        return LegVerdict::Refused("claim_present".to_string());
    }
    LegVerdict::Verified
}

// ── the report ───────────────────────────────────────────────────

/// The packet disposition — `withheld` only when every leg
/// verified: the honest terminal state of the quote-only admission
/// (never `executed` — nothing ran).
#[derive(Debug, Clone, PartialEq)]
pub enum Disposition {
    /// Every leg `verified` — the withheld record is honest.
    Withheld,
    /// At least one `unsupported`/`refused` leg — definitive
    /// failure.
    Failed,
    /// At least one `inconclusive` leg, none failed — undetermined.
    Inconclusive,
}

impl Disposition {
    /// The closed vocabulary.
    pub fn name(&self) -> &'static str {
        match self {
            Disposition::Withheld => "withheld",
            Disposition::Failed => "failed",
            Disposition::Inconclusive => "inconclusive",
        }
    }
}

/// The `hh-confirm-withheld/1` record — the honest withheld
/// disposition: the tranche is absent, parity is absent (the
/// dependent block), S4 stays unmet; `packet_id` is minted last
/// over the canonical record.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmWithheldReport {
    /// `H(canonical(record minus packet_id))` under
    /// `confirm_withheld`.
    pub packet_id: String,
    /// The record version.
    pub version: String,
    /// The gate whose decision is recorded (`GATE-G14`).
    pub gate: String,
    /// First subjects dispatched (`0`).
    pub subjects_first: u64,
    /// The confirmation tranche is absent (`true`).
    pub tranche_absent: bool,
    /// Full actual parity is absent — the dependent block (`true`).
    pub parity_absent: bool,
    /// Dependent confirmation stays blocked (`true`).
    pub confirmation_blocked: bool,
    /// S4 stays unmet (`true` — recorded honestly).
    pub s4_unmet: bool,
    /// The six legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// The admission shape — `quote_only_withheld`, never an
    /// execution.
    pub admission: String,
}

impl ConfirmWithheldReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("packet_id", Json::str(&self.packet_id)),
            ("gate", Json::str(&self.gate)),
            ("subjects_first", Json::Int(self.subjects_first as i64)),
            ("tranche_absent", Json::Bool(self.tranche_absent)),
            ("parity_absent", Json::Bool(self.parity_absent)),
            (
                "confirmation_blocked",
                Json::Bool(self.confirmation_blocked),
            ),
            ("s4_unmet", Json::Bool(self.s4_unmet)),
            ("disposition", Json::str(self.disposition.name())),
            ("admission", Json::str(&self.admission)),
            (
                "legs",
                Json::Arr(
                    self.legs
                        .iter()
                        .map(|l| {
                            Json::obj([
                                ("name", Json::str(&l.name)),
                                ("verdict", l.verdict.to_json()),
                                (
                                    "facts",
                                    Json::Obj(
                                        l.facts
                                            .iter()
                                            .map(|(k, v)| (k.clone(), Json::str(v)))
                                            .collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

/// `assemble(legs)` — fold the six leg verdicts into the
/// disposition and mint `packet_id` last over the canonical record.
/// Every leg name must be a `LEGS` member, unique and complete — a
/// malformed leg set is a `refused` assembly, never a partial
/// packet.
pub fn assemble(legs: Vec<Leg>) -> Result<ConfirmWithheldReport, String> {
    let mut seen = std::collections::BTreeSet::new();
    for l in &legs {
        if !LEGS.contains(&l.name.as_str()) {
            return Err(format!("leg vocabulary: {}", l.name));
        }
        if !seen.insert(l.name.as_str()) {
            return Err(format!("leg duplicate: {}", l.name));
        }
    }
    for name in LEGS {
        if !seen.contains(name) {
            return Err(format!("leg missing: {name}"));
        }
    }
    let definitive = legs.iter().any(|l| {
        matches!(
            l.verdict,
            LegVerdict::Unsupported(_) | LegVerdict::Refused(_)
        )
    });
    let undetermined = legs
        .iter()
        .any(|l| matches!(l.verdict, LegVerdict::Inconclusive(_)));
    let disposition = if definitive {
        Disposition::Failed
    } else if undetermined {
        Disposition::Inconclusive
    } else {
        Disposition::Withheld
    };
    let mut out = ConfirmWithheldReport {
        packet_id: String::new(),
        version: VERSION.to_string(),
        gate: GATE.to_string(),
        subjects_first: 0,
        tranche_absent: true,
        parity_absent: true,
        confirmation_blocked: true,
        s4_unmet: true,
        legs,
        disposition,
        admission: "quote_only_withheld".to_string(),
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("packet_id");
    }
    out.packet_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
