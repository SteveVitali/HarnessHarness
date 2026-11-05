//! `hh-plugin-unselected/1` — the unselected existing-component
//! disposition record (R5.50; GATE-G15 PASSED — **out-of-scope
//! disposition**; evidence spec §8.2's optional component exercise:
//! "At most one existing `hharness-compact-evict-oldest` source
//! exercise" — unselected this round; invariant I-08: "Optional
//! exercises award no claim if unselected").
//!
//! Records-in only: every leg evaluator folds *supplied* facts —
//! the gate's verbatim decision, the pinned-input digest counts,
//! the committed manifest template's declared vocabulary and the
//! real `admit_plugin` outcome, the refusal spellings the landed
//! manifest codec/admission produced on mutated decls, the absent
//! walkthrough counts and the withheld-claims fold — and never
//! manufactures a build, a seal, a load, an invocation, an
//! admission or a verdict. `unselected` is the honest terminal
//! state of the voided stage; a definitive failure folds `failed`,
//! an undetermined leg `inconclusive` — never a fabricated
//! walkthrough.

use std::collections::BTreeMap;

use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;

use crate::preflight::{Leg, LegVerdict};

/// The record version.
pub const VERSION: &str = "hh-plugin-unselected/1";
const DOMAIN: &str = "plugin_unselected";

/// The gate whose out-of-scope decision this packet records.
pub const GATE: &str = "GATE-G15";

/// The six legs (frozen vocabulary — the unselected scope).
pub const LEGS: [&str; 6] = [
    "gate_readout",
    "pins_frozen",
    "manifest_decl",
    "refusal_vocabulary",
    "walkthrough_absent",
    "claims_withheld",
];

// ── leg 1 — gate_readout ─────────────────────────────────────────

/// GATE-G15's verbatim decision facts (the readout's signature
/// block — recorded, never re-derived).
#[derive(Debug, Clone, PartialEq)]
pub struct GateFacts {
    /// The recorded decision (`PASSED — out-of-scope disposition`).
    pub decision: String,
    /// The operator's words verbatim (`accept, continue`).
    pub operator_words: String,
    /// The decision UTC (`2026-11-04T14:32:29Z`).
    pub decided_at: String,
    /// The confirmed agent-drafted block sha256 prefix.
    pub block_sha256_prefix: String,
    /// DF-H9-1's recorded state (`open` — the deferral is not
    /// discharged by voiding the stage).
    pub df_h9_1: String,
}

/// `evaluate_gate` — the verbatim decision check: PASSED with an
/// out-of-scope disposition (a procedures-admitted decision is a
/// different admission and a different packet), nonempty operator
/// words/decided-at, a 12-hex block prefix and DF-H9-1 open;
/// anything else refuses, never relabels.
pub fn evaluate_gate(f: &GateFacts) -> LegVerdict {
    if !f.decision.starts_with("PASSED") || !f.decision.contains("out-of-scope") {
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
    if f.df_h9_1 != "open" {
        return LegVerdict::Refused(format!("df_h9_1:{}", f.df_h9_1));
    }
    LegVerdict::Verified
}

// ── leg 2 — pins_frozen ──────────────────────────────────────────

/// The pinned-input count — the nine H8 hash-verified upstream
/// artifacts plus the committed plugin manifest template (pin 10).
pub const PINS_TOTAL: u64 = 10;

/// The pin-verification facts — `check_pins` over the ten declared
/// inputs, plus the manifest template's digest echoed verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct PinsFacts {
    /// The pinned-input count (`10`).
    pub pins: u64,
    /// The `check_pins` verdict name (`verified`).
    pub pins_verdict: String,
    /// The manifest template's sha256 (64-hex, echoed — never
    /// hard-coded into the evaluator).
    pub manifest_sha256: String,
}

/// `evaluate_pins` — the pin check: ten inputs, `check_pins`
/// verified and a 64-hex manifest digest supplied. A drifted pin
/// refuses; an unreadable pin is inconclusive upstream — an
/// empty/short digest here is inconclusive, never guessed.
pub fn evaluate_pins(f: &PinsFacts) -> LegVerdict {
    if f.pins != PINS_TOTAL {
        return LegVerdict::Refused(format!("pins:{}", f.pins));
    }
    if f.pins_verdict != "verified" {
        return LegVerdict::Refused(format!("pins_verdict:{}", f.pins_verdict));
    }
    let ok_digest =
        f.manifest_sha256.len() == 64 && f.manifest_sha256.chars().all(|c| c.is_ascii_hexdigit());
    if !ok_digest {
        return LegVerdict::Inconclusive("manifest_sha256_absent".to_string());
    }
    LegVerdict::Verified
}

// ── leg 3 — manifest_decl ────────────────────────────────────────

/// The committed template's declared vocabulary — the manifest
/// decode + admission facts (the real `manifest_from_json` +
/// `admit_plugin` over the corpus store — a pure read-side check:
/// nothing is fetched, launched or sealed).
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestDeclFacts {
    /// The committed manifest decoded strictly (`true`).
    pub decoded: bool,
    /// `namespace/name` (`hh/hh-compact-evict-oldest`).
    pub plugin_id: String,
    /// The declared version label (`0.1.0`).
    pub version_label: String,
    /// The declared tier (`C0`).
    pub tier: String,
    /// The contribution count (`1`).
    pub contributions: u64,
    /// The contribution kind (`variant`).
    pub kind: String,
    /// The bound class (`compaction_strategy`).
    pub class_id: String,
    /// The contribution contract range (`1.0`).
    pub contract_range: String,
    /// `declaration.deterministic` (`true`).
    pub deterministic: bool,
    /// `declaration.model_call` (`false`).
    pub model_call: bool,
    /// `declaration.op_kinds` (`evict` — the single op).
    pub op_kinds: String,
    /// `executable.isolation` (`subprocess_confined`).
    pub isolation: String,
    /// `executable.placement_preference` (`subprocess_confined`).
    pub placement_preference: String,
    /// Every `requests` member empty (`true` — claims, never
    /// grants).
    pub requests_empty: bool,
    /// `claims`/`conformance_claims` empty (`true`).
    pub claims_empty: bool,
    /// `code_pointer` is the zero placeholder with `size:0`
    /// (`true` — the template is unsealed by construction; sealing
    /// is a principal act, never automatic).
    pub code_pointer_zero: bool,
    /// The real `admit_plugin` outcome over the corpus store,
    /// verbatim — `ContractIncompatible`: the template's declared
    /// `"1.0"` requires meet the kernel policies' supported `["1"]`
    /// (`Exact("1.0")` covers only `1.0`), so the pure read-side
    /// admission check refuses the raw template typed — never an
    /// `admitted` relabel (the packaged walkthrough would run the
    /// seal/load path, never this surface; recorded honestly).
    pub admit: String,
    /// The first `ContractIncompatible` failure's `detail()`
    /// verbatim (`dialect:hir/1 offered 1.0 vs supported [1]`).
    pub admit_detail: String,
}

/// `evaluate_decl` — the declared-vocabulary check: strict decode
/// holds, the identity/contribution/declaration/request/pin facts
/// equal the committed template's, and `admit_plugin`'s honest
/// typed outcome is `ContractIncompatible` with the `hir/1`
/// failure verbatim. A drifted fact is a different manifest —
/// refused, never relabelled; a missing member is inconclusive.
pub fn evaluate_decl(f: &ManifestDeclFacts) -> LegVerdict {
    if !f.decoded || f.plugin_id.is_empty() {
        return LegVerdict::Inconclusive("manifest_absent".to_string());
    }
    if f.plugin_id != "hh/hh-compact-evict-oldest" {
        return LegVerdict::Refused(format!("plugin_id:{}", f.plugin_id));
    }
    for (member, have, need) in [
        ("version_label", f.version_label.as_str(), "0.1.0"),
        ("tier", f.tier.as_str(), "C0"),
        ("kind", f.kind.as_str(), "variant"),
        ("class_id", f.class_id.as_str(), "compaction_strategy"),
        ("contract_range", f.contract_range.as_str(), "1.0"),
        ("op_kinds", f.op_kinds.as_str(), "evict"),
        ("isolation", f.isolation.as_str(), "subprocess_confined"),
        (
            "placement_preference",
            f.placement_preference.as_str(),
            "subprocess_confined",
        ),
    ] {
        if have != need {
            return LegVerdict::Refused(format!("{member}:{have}"));
        }
    }
    if f.contributions != 1 {
        return LegVerdict::Refused(format!("contributions:{}", f.contributions));
    }
    if !f.deterministic || f.model_call {
        return LegVerdict::Refused(format!(
            "deterministic:{} model_call:{}",
            f.deterministic, f.model_call
        ));
    }
    if !f.requests_empty || !f.claims_empty {
        return LegVerdict::Refused(format!(
            "requests_empty:{} claims_empty:{}",
            f.requests_empty, f.claims_empty
        ));
    }
    if !f.code_pointer_zero {
        // A sealed pin on the committed tree is a different package
        // state — sealing is a principal act, never inferred.
        return LegVerdict::Refused("code_pointer_sealed".to_string());
    }
    if f.admit != "ContractIncompatible" {
        // `admitted` — or any other outcome — is a different
        // admission surface than the raw template's; refused,
        // never relabelled.
        return LegVerdict::Refused(format!("admit:{}", f.admit));
    }
    // Byte-assert the committed verbatim spelling — a drifted
    // detail (a different supported set or phrasing) is not the
    // machinery's recorded outcome on this tree; refused, never
    // relabelled.
    if f.admit_detail != "dialect:hir/1 offered 1.0 vs supported [1]" {
        return LegVerdict::Refused(format!("admit_detail:{}", f.admit_detail));
    }
    LegVerdict::Verified
}

// ── leg 4 — refusal_vocabulary ───────────────────────────────────

/// The exercised decl set — `exercise → expected typed refusal
/// kind_str` (the landed `manifest_from_json`/`admit_plugin`
/// machinery's verbatim spellings, driven on mutated decls only:
/// nothing executes and nothing is admitted). The decode legs
/// cover the sealed-form pin/grant discipline (`UnpinnedInSealedForm`,
/// `SchemaViolation`, `UnknownRecordKind`); the admission legs
/// cover locality, requests-as-claims, compatibility and namespace.
pub const EXERCISES: [(&str, &str); 7] = [
    ("unknown_member", "SchemaViolation"),
    ("unknown_kind", "UnknownRecordKind"),
    ("empty_digest", "UnpinnedInSealedForm"),
    ("local_in_process", "LocalityInadmissible"),
    ("env_key_over_cap", "RequestsExceedCap"),
    ("plugin_abi_nine", "ContractIncompatible"),
    ("forbidden_namespace", "NamespaceForbidden"),
];

/// `evaluate_vocabulary` — the machinery-stands check: every
/// declared exercise produced its typed refusal verbatim. A
/// missing exercise is `inconclusive`; a wrong refusal — or
/// `decoded`/`admitted`, the machinery broken — is `refused`,
/// never reclassified.
pub fn evaluate_vocabulary(observed: &BTreeMap<String, String>) -> LegVerdict {
    for (exercise, expected) in EXERCISES {
        match observed.get(exercise).map(String::as_str) {
            Some(o) if o == expected => {}
            Some("decoded") | Some("admitted") => {
                return LegVerdict::Refused(format!("admitted:{exercise}"));
            }
            Some(o) => return LegVerdict::Refused(format!("{exercise}:{o}")),
            None => return LegVerdict::Inconclusive(format!("exercise_absent:{exercise}")),
        }
    }
    LegVerdict::Verified
}

// ── leg 5 — walkthrough_absent ───────────────────────────────────

/// The absent walkthrough, verbatim — every §8.2 phase and adverse
/// count a declared zero, no selection and no reseal attempted.
#[derive(Debug, Clone, PartialEq)]
pub struct WalkthroughFacts {
    /// `member → count` — `built`, `sealed`, `loaded`, `invoked`,
    /// `helper_spawns`, `binds`, `controls_executed`,
    /// `items_processed`, `provenance_records`, `tamper_cases`,
    /// `wrong_pin_cases`, `missing_helper_cases`,
    /// `kernel_identity_checks`, `client_versions_recorded`,
    /// `callbacks`, `notifications`,
    /// `participant_understanding_records` each declared `0`.
    pub counts: BTreeMap<String, u64>,
    /// A package selection was attempted (`false`).
    pub selection_attempted: bool,
    /// A reseal was attempted (`false` — never implicit reseal).
    pub reseal_attempted: bool,
}

/// The seventeen absent-walkthrough members (frozen vocabulary).
pub const WALKTHROUGH_MEMBERS: [&str; 17] = [
    "built",
    "sealed",
    "loaded",
    "invoked",
    "helper_spawns",
    "binds",
    "controls_executed",
    "items_processed",
    "provenance_records",
    "tamper_cases",
    "wrong_pin_cases",
    "missing_helper_cases",
    "kernel_identity_checks",
    "client_versions_recorded",
    "callbacks",
    "notifications",
    "participant_understanding_records",
];

/// `evaluate_walkthrough` — the absent-walkthrough check: every
/// member zero, no selection, no reseal. Any nonzero fact is a
/// real walkthrough's evidence — it belongs in a different packet
/// and refuses here, never a fabricated absence.
pub fn evaluate_walkthrough(f: &WalkthroughFacts) -> LegVerdict {
    for member in WALKTHROUGH_MEMBERS {
        match f.counts.get(member) {
            Some(0) => {}
            Some(n) => return LegVerdict::Refused(format!("{member}:{n}")),
            None => return LegVerdict::Inconclusive(format!("{member}_absent")),
        }
    }
    if f.selection_attempted {
        return LegVerdict::Refused("selection_attempted:true".to_string());
    }
    if f.reseal_attempted {
        return LegVerdict::Refused("reseal_attempted:true".to_string());
    }
    LegVerdict::Verified
}

// ── leg 6 — claims_withheld ──────────────────────────────────────

/// The withheld-claims facts — DF-H9-1 stays open, every
/// participant/outreach predicate withheld and no claim of any
/// sign is present (I-08: unselected awards no claim; §8.2: no
/// third-arm/provider-treatment claim follows from ABI
/// conformance — let alone a voided stage).
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimsFacts {
    /// DF-H9-1's recorded state (`open`).
    pub df_h9_1: String,
    /// The participant/outreach predicate set (`withheld`).
    pub participant_predicates: String,
    /// Any outreach performed (`none`).
    pub outreach: String,
    /// A claim awarded by the unselected disposition (`false` —
    /// I-08).
    pub claim_awarded: bool,
    /// A third-treatment-arm claim (`false`).
    pub third_arm_claim: bool,
    /// A provider-treatment/efficacy claim (`false`).
    pub provider_efficacy_claim: bool,
    /// A publication claim (`false`).
    pub publication_claim: bool,
    /// A capability claim (`false`).
    pub capability_claim: bool,
    /// An adoption claim (`false`).
    pub adoption_claim: bool,
}

/// `evaluate_claims` — the withheld check: DF-H9-1 `open`,
/// participant predicates `withheld`, outreach `none`, no claim
/// of any sign. A claim — or a discharged deferral — refuses,
/// never relabels.
pub fn evaluate_claims(f: &ClaimsFacts) -> LegVerdict {
    if f.df_h9_1 != "open" {
        return LegVerdict::Refused(format!("df_h9_1:{}", f.df_h9_1));
    }
    if f.participant_predicates != "withheld" {
        return LegVerdict::Refused(format!(
            "participant_predicates:{}",
            f.participant_predicates
        ));
    }
    if f.outreach != "none" {
        return LegVerdict::Refused(format!("outreach:{}", f.outreach));
    }
    if f.claim_awarded
        || f.third_arm_claim
        || f.provider_efficacy_claim
        || f.publication_claim
        || f.capability_claim
        || f.adoption_claim
    {
        return LegVerdict::Refused("claim_present".to_string());
    }
    LegVerdict::Verified
}

// ── the report ───────────────────────────────────────────────────

/// The packet disposition — `unselected` only when every leg
/// verified: the honest terminal state of the voided stage (never
/// `executed`, never `selected` — nothing ran).
#[derive(Debug, Clone, PartialEq)]
pub enum Disposition {
    /// Every leg `verified` — the unselected record is honest.
    Unselected,
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
            Disposition::Unselected => "unselected",
            Disposition::Failed => "failed",
            Disposition::Inconclusive => "inconclusive",
        }
    }
}

/// The `hh-plugin-unselected/1` record — the honest
/// `unselected{stage_voided}` disposition: the optional exercise
/// is unselected, every walkthrough phase absent, all
/// participant/outreach claims withheld and DF-H9-1 open;
/// `packet_id` is minted last over the canonical record.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginUnselectedReport {
    /// `H(canonical(record minus packet_id))` under
    /// `plugin_unselected`.
    pub packet_id: String,
    /// The record version.
    pub version: String,
    /// The gate whose decision is recorded (`GATE-G15`).
    pub gate: String,
    /// The optional exercise is unselected (`true`).
    pub unselected: bool,
    /// The participation stage is voided this round (`true`).
    pub stage_voided: bool,
    /// DF-H9-1 stays open (`true` — recorded honestly).
    pub df_h9_1_open: bool,
    /// Walkthrough phases completed (`0`).
    pub phases_completed: u64,
    /// The six legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// The admission shape — `unselected_stage_voided`, never an
    /// execution.
    pub admission: String,
}

impl PluginUnselectedReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("packet_id", Json::str(&self.packet_id)),
            ("gate", Json::str(&self.gate)),
            ("unselected", Json::Bool(self.unselected)),
            ("stage_voided", Json::Bool(self.stage_voided)),
            ("df_h9_1_open", Json::Bool(self.df_h9_1_open)),
            ("phases_completed", Json::Int(self.phases_completed as i64)),
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
/// disposition and mint `packet_id` last over the canonical
/// record. Every leg name must be a `LEGS` member, unique and
/// complete — a malformed leg set is a `refused` assembly, never a
/// partial packet.
pub fn assemble(legs: Vec<Leg>) -> Result<PluginUnselectedReport, String> {
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
        Disposition::Unselected
    };
    let mut out = PluginUnselectedReport {
        packet_id: String::new(),
        version: VERSION.to_string(),
        gate: GATE.to_string(),
        unselected: true,
        stage_voided: true,
        df_h9_1_open: true,
        phases_completed: 0,
        legs,
        disposition,
        admission: "unselected_stage_voided".to_string(),
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("packet_id");
    }
    out.packet_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
