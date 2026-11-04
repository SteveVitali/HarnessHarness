//! `hh-parity-n0/1` — the declared N=0 parity disposition record
//! (R5.47; GATE-G13 PASSED — **N=0 declared**; evidence spec §4.4/§4.5
//! and §9.2's parity row: "No allocated tranche").
//!
//! Records-in only: every leg evaluator folds *supplied* facts — the
//! gate's verbatim decision, the pinned-artifact digests, the refusal
//! spellings the landed `parity_admission` produced on exercised
//! decls, the quote packet's parity leg and the absent-tranche
//! counts — and never manufactures a pair, an admission, an
//! execution or a verdict. `withheld` is the honest terminal state
//! of the declared N=0 admission; a definitive failure folds
//! `failed`, an undetermined leg `inconclusive` — never a
//! fabricated tranche.

use std::collections::BTreeMap;

use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;

use crate::preflight::{Leg, LegVerdict};

/// The record version.
pub const VERSION: &str = "hh-parity-n0/1";
const DOMAIN: &str = "parity_n0";

/// The gate whose N=0 decision this packet records.
pub const GATE: &str = "GATE-G13";

/// The six legs (frozen vocabulary — the N=0 scope).
pub const LEGS: [&str; 6] = [
    "gate_readout",
    "pins_frozen",
    "admission_vocabulary",
    "quote_worksheet",
    "tranche_absent",
    "dependent_blocked",
];

// ── leg 1 — gate_readout ─────────────────────────────────────────

/// GATE-G13's verbatim decision facts (the readout's signature
/// block — recorded, never re-derived).
#[derive(Debug, Clone, PartialEq)]
pub struct GateFacts {
    /// The recorded decision (`PASSED — N=0 declared`).
    pub decision: String,
    /// The operator's words verbatim (`confirm, proceed`).
    pub operator_words: String,
    /// The decision UTC (`2026-11-04T07:12:23Z`).
    pub decided_at: String,
    /// The confirmed agent-drafted block sha256 prefix.
    pub block_sha256_prefix: String,
    /// The declared tranche size — `0` only (a nonzero N is a
    /// different admission and a different packet).
    pub n_declared: u64,
    /// The S3 disposition (`unmet` — stays unmet honestly).
    pub s3: String,
}

/// `evaluate_gate` — the verbatim decision check: PASSED with
/// `n_declared == 0`, nonempty operator words/decided-at, a 12-hex
/// block prefix and S3 unmet; anything else refuses, never
/// relabels.
pub fn evaluate_gate(f: &GateFacts) -> LegVerdict {
    if f.n_declared != 0 {
        return LegVerdict::Refused(format!("n_declared:{}", f.n_declared));
    }
    if !f.decision.starts_with("PASSED") || !f.decision.contains("N=0") {
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
    if f.s3 != "unmet" {
        return LegVerdict::Refused(format!("s3:{}", f.s3));
    }
    LegVerdict::Verified
}

// ── leg 3 — admission_vocabulary ─────────────────────────────────

/// The exercised decl set — `exercise → expected typed refusal
/// name` (the landed `parity_admission` machinery's verbatim
/// refusal spellings, driven on declared inputs only: nothing
/// executes and nothing is admitted).
pub const EXERCISES: [(&str, &str); 6] = [
    ("forbidden_use_headline", "UnsupportedClaim"),
    ("inauthentic_recorded_model_io", "InauthenticEvidence"),
    ("empty_participant_ref", "MissingKey"),
    ("absent_tranche_manifest", "UnequalDesign"),
    ("absent_dimension_map", "MissingMap"),
    ("baseline_only_applicability", "PolicyScope"),
];

/// `evaluate_vocabulary` — the machinery-stands check: every
/// declared exercise produced its typed refusal verbatim. A missing
/// exercise is `inconclusive`; a wrong refusal — or `admitted`, the
/// machinery broken — is `refused`, never reclassified.
pub fn evaluate_vocabulary(observed: &BTreeMap<String, String>) -> LegVerdict {
    for (exercise, expected) in EXERCISES {
        match observed.get(exercise).map(String::as_str) {
            Some(o) if o == expected => {}
            Some("admitted") => return LegVerdict::Refused(format!("admitted:{exercise}")),
            Some(o) => return LegVerdict::Refused(format!("{exercise}:{o}")),
            None => return LegVerdict::Inconclusive(format!("exercise_absent:{exercise}")),
        }
    }
    LegVerdict::Verified
}

// ── leg 4 — quote_worksheet ──────────────────────────────────────

/// The parity leg of the landed quote packet over the declared §9.2
/// minimum-admission worksheet.
#[derive(Debug, Clone, PartialEq)]
pub struct QuoteFacts {
    /// The parity stage's dispatch (`not_requested` — quote-only,
    /// no allocated tranche).
    pub parity_dispatch: String,
    /// The parity stage's state (`blocked` — design unfixed at N=0).
    pub parity_state: String,
    /// The parity precision state (`unresolved`).
    pub parity_precision: String,
    /// `precision:design_unfixed` among the parity blockers.
    pub design_unfixed_blocker: bool,
    /// The parity stage's quoted runners (both — verbatim).
    pub runners: Vec<String>,
    /// The packet total exposure in currency micros (`0`).
    pub total_micros: i64,
    /// Forbidden constructs observed (top-up / pooled capacity /
    /// cloud fallback / guessed margin) — must be empty.
    pub forbidden: Vec<String>,
}

/// `evaluate_quote` — the parity leg stays `not_requested` with the
/// design unfixed at N=0: a requested/gate-pending dispatch, an
/// unblocked parity stage, a missing runner, priced exposure or a
/// forbidden construct each refuse.
pub fn evaluate_quote(f: &QuoteFacts) -> LegVerdict {
    if f.parity_dispatch != "not_requested" {
        return LegVerdict::Refused(format!("parity_dispatch:{}", f.parity_dispatch));
    }
    if f.parity_state != "blocked" {
        return LegVerdict::Refused(format!("parity_state:{}", f.parity_state));
    }
    if f.parity_precision != "unresolved" || !f.design_unfixed_blocker {
        return LegVerdict::Refused(format!("precision:{}", f.parity_precision));
    }
    let mut runners = f.runners.clone();
    runners.sort();
    if runners != ["hh-runner", "original-runner"] {
        return LegVerdict::Refused(format!("runners:{}", runners.join(",")));
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

// ── legs 5+6 — tranche_absent / dependent_blocked ────────────────

/// The absent tranche, verbatim — every count a declared zero and
/// the KL verdict set `none` (no tranche, no verdict; reported,
/// never fabricated).
#[derive(Debug, Clone, PartialEq)]
pub struct TrancheFacts {
    /// `member → count` — `n_declared`, `pairs_collected`,
    /// `h_attempts`, `o_attempts` each declared `0`.
    pub counts: BTreeMap<String, u64>,
    /// A parity dispatch was attempted (`false`).
    pub dispatch_attempted: bool,
    /// The KL verdict count (`none` — none exist to report).
    pub kl_verdicts: String,
}

/// `evaluate_tranche` — the absent-tranche check: every count zero,
/// no dispatch, `kl_verdicts=none`. Any nonzero fact is a real
/// tranche's evidence — it belongs in a different packet and
/// refuses here, never a fabricated absence.
pub fn evaluate_tranche(f: &TrancheFacts) -> LegVerdict {
    for member in ["n_declared", "pairs_collected", "h_attempts", "o_attempts"] {
        match f.counts.get(member) {
            Some(0) => {}
            Some(n) => return LegVerdict::Refused(format!("{member}:{n}")),
            None => return LegVerdict::Inconclusive(format!("{member}_absent")),
        }
    }
    if f.dispatch_attempted {
        return LegVerdict::Refused("dispatch_attempted:true".to_string());
    }
    if f.kl_verdicts != "none" {
        return LegVerdict::Refused(format!("kl_verdicts:{}", f.kl_verdicts));
    }
    LegVerdict::Verified
}

/// The dependent-confirmation facts — withheld/inconclusive/
/// non-equivalence each block dependent confirmation per the
/// contract scope; nothing claims equivalence or non-equivalence
/// (withheld is not a verdict of either).
#[derive(Debug, Clone, PartialEq)]
pub struct DependentFacts {
    /// The confirmation stage's dispatch (`not_requested`).
    pub confirmation_dispatch: String,
    /// An equivalence claim anywhere (`false`).
    pub equivalence_claimed: bool,
    /// A non-equivalence claim (`false`).
    pub non_equivalence_claimed: bool,
    /// Withheld blocks dependent confirmation (`true`).
    pub withheld_blocks: bool,
    /// Inconclusive blocks dependent confirmation (`true`).
    pub inconclusive_blocks: bool,
}

/// `evaluate_dependent` — confirmation stays `not_requested`, both
/// block dispositions recorded `true`, no claim of either sign. A
/// requested confirmation or any claim refuses.
pub fn evaluate_dependent(f: &DependentFacts) -> LegVerdict {
    if f.confirmation_dispatch != "not_requested" {
        return LegVerdict::Refused(format!("confirmation_dispatch:{}", f.confirmation_dispatch));
    }
    if f.equivalence_claimed || f.non_equivalence_claimed {
        return LegVerdict::Refused("claim_present".to_string());
    }
    if !f.withheld_blocks || !f.inconclusive_blocks {
        return LegVerdict::Refused("block_disposition_unrecorded".to_string());
    }
    LegVerdict::Verified
}

// ── the report ───────────────────────────────────────────────────

/// The packet disposition — `withheld` only when every leg
/// verified: the honest terminal state of the declared N=0
/// admission (never `executed` — nothing ran).
#[derive(Debug, Clone, PartialEq)]
pub enum Disposition {
    /// Every leg `verified` — the withheld N=0 record is honest.
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

/// The `hh-parity-n0/1` record — the honest N=0 disposition: the
/// tranche is absent, S3 stays unmet and dependent confirmation
/// stays blocked; `packet_id` is minted last over the canonical
/// record.
#[derive(Debug, Clone, PartialEq)]
pub struct ParityN0Report {
    /// `H(canonical(record minus packet_id))` under `parity_n0`.
    pub packet_id: String,
    /// The record version.
    pub version: String,
    /// The gate whose decision is recorded (`GATE-G13`).
    pub gate: String,
    /// The declared tranche size (`0`).
    pub n_declared: u64,
    /// S3 stays unmet (`true` — recorded honestly).
    pub s3_unmet: bool,
    /// Dependent confirmation stays blocked (`true`).
    pub confirmation_blocked: bool,
    /// The six legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// The admission shape — `n0_withheld`, never an execution.
    pub admission: String,
}

impl ParityN0Report {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("packet_id", Json::str(&self.packet_id)),
            ("gate", Json::str(&self.gate)),
            ("n_declared", Json::Int(self.n_declared as i64)),
            ("s3_unmet", Json::Bool(self.s3_unmet)),
            (
                "confirmation_blocked",
                Json::Bool(self.confirmation_blocked),
            ),
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
pub fn assemble(legs: Vec<Leg>) -> Result<ParityN0Report, String> {
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
    let mut out = ParityN0Report {
        packet_id: String::new(),
        version: VERSION.to_string(),
        gate: GATE.to_string(),
        n_declared: 0,
        s3_unmet: true,
        confirmation_blocked: true,
        legs,
        disposition,
        admission: "n0_withheld".to_string(),
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("packet_id");
    }
    out.packet_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
