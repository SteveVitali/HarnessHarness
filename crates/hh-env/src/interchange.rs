//! `hh-interchange-unselected/1` — the unselected native-to-external
//! consumer disposition record (R5.51; GATE-G15 PASSED — **out-of-scope
//! disposition**; evidence spec §1.1 S6's optional interchange exercise
//! — "At most one external consumer/direction, preferably existing
//! Inspect JSON" — unselected this round; invariant I-08: "Optional
//! exercises award no claim if unselected"; §1.4 AC-1-2: "An unselected
//! S6 remains optional and awards no compatibility claim").
//!
//! Records-in only: every leg evaluator folds *supplied* facts —
//! the gate's verbatim decision, the pinned-input digest counts, the
//! `ForeignFormat` closed-sum/direction-table decls, the refusal
//! spellings the landed `lift_foreign`/`export_target`/`lift`
//! machinery produced on exercised cases, the absent exercise counts
//! and the withheld-claims fold — and never manufactures an artifact,
//! an export, a reader verdict or a claim. `unselected` is the honest
//! terminal state of the voided stage; a definitive failure folds
//! `failed`, an undetermined leg `inconclusive` — never a fabricated
//! artifact→format→reader exercise.

use std::collections::BTreeMap;

use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;

use crate::preflight::{Leg, LegVerdict};

/// The packet disposition — the `unselected | failed |
/// inconclusive` fold is the R5.50 contract consumed unchanged
/// (`plugin::Disposition` re-exported: one closed vocabulary,
/// never a second implementation).
pub use crate::plugin::Disposition;

/// The record version.
pub const VERSION: &str = "hh-interchange-unselected/1";
const DOMAIN: &str = "interchange_unselected";

/// The gate whose out-of-scope decision this packet records.
pub const GATE: &str = "GATE-G15";

/// The six legs (frozen vocabulary — the unselected scope).
pub const LEGS: [&str; 6] = [
    "gate_readout",
    "pins_frozen",
    "format_vocabulary",
    "refusal_vocabulary",
    "exercise_absent",
    "claims_withheld",
];

// ── leg 1 — gate_readout ─────────────────────────────────────────
//
// GATE-G15's verbatim decision facts are the R5.50 contract — the
// `plugin::GateFacts`/`evaluate_gate` predecessor is consumed
// unchanged (one gate, one evaluator, never a second copy).

// ── leg 2 — pins_frozen ──────────────────────────────────────────

/// The pinned-input count — the nine H8 hash-verified upstream
/// artifacts plus the committed `inspect_eval_log` foreign fixture
/// (S6's preferred consumer format — pin 10, declared never consumed).
pub const PINS_TOTAL: u64 = 10;

/// The pin-verification facts — `check_pins` over the ten declared
/// inputs, plus the Inspect fixture's digest echoed verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct PinsFacts {
    /// The pinned-input count (`10`).
    pub pins: u64,
    /// The `check_pins` verdict name (`verified`).
    pub pins_verdict: String,
    /// The committed `inspect_eval_log` fixture's sha256 (64-hex,
    /// echoed — never hard-coded into the evaluator).
    pub fixture_sha256: String,
}

/// `evaluate_pins` — the pin check: ten inputs, `check_pins`
/// verified and a 64-hex fixture digest supplied. A drifted pin
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
        f.fixture_sha256.len() == 64 && f.fixture_sha256.chars().all(|c| c.is_ascii_hexdigit());
    if !ok_digest {
        return LegVerdict::Inconclusive("fixture_sha256_absent".to_string());
    }
    LegVerdict::Verified
}

// ── leg 3 — format_vocabulary ────────────────────────────────────

/// The closed-sum decl, verbatim — the `ForeignFormat::ALL` canonical
/// spellings comma-joined in declaration order (the one grammar —
/// `parse` is the sum's only reader, CC1).
pub const EXPECTED_SUM: &str = "harbor_trial_dir,harbor_job_dir,interchange_trajectory,swebench_submission,inspect_eval_log,in_toto_bundle,telemetry_trace";

/// The liftable decl, verbatim — `liftable_spellings()` equal to
/// `import::FOREIGN_FORMATS` (the six import-side formats;
/// `interchange_trajectory` is known-but-export-only, ADR-0213).
pub const EXPECTED_LIFTABLE: &str =
    "harbor_trial_dir,harbor_job_dir,swebench_submission,inspect_eval_log,in_toto_bundle,telemetry_trace";

/// The preferred-candidate decl — S6's "preferably existing Inspect
/// JSON" (`inspect_eval_log`); ATIF is the admitted fallback only if
/// simpler. Declared, never selected — the stage is voided.
pub const PREFERRED_CANDIDATE: &str = "inspect_eval_log";

/// The `ForeignFormat` vocabulary facts — the closed-sum decls the
/// landed `crates/hh-bundle/src/foreign.rs` machinery carries (read,
/// never re-declared: the battery supplies what the machinery
/// actually says).
#[derive(Debug, Clone, PartialEq)]
pub struct VocabularyFacts {
    /// The closed-sum cardinality (`7`).
    pub formats: u64,
    /// `ALL.as_str()` comma-joined in declaration order.
    pub closed_sum: String,
    /// Every canonical spelling round-trips `parse` to its own
    /// member (`true`).
    pub parse_roundtrip: bool,
    /// Outside spellings parse to `None` — the grammar never
    /// guesses (`true`).
    pub parse_never_guesses: bool,
    /// The import-only members comma-joined (`harbor_trial_dir`).
    pub import_only: String,
    /// The export-only members comma-joined
    /// (`interchange_trajectory` — its import half stays
    /// OQ-030-deferred, ADR-0213).
    pub export_only: String,
    /// The bidirectional member count (`5`).
    pub bidirectional: u64,
    /// `liftable_spellings()` comma-joined (the six import formats).
    pub liftable: String,
    /// `liftable_spellings()` equals `import::FOREIGN_FORMATS`
    /// (`true` — the import list is the sum's own liftable set,
    /// never a second table).
    pub liftable_is_import_list: bool,
    /// The preferred-candidate decl (`inspect_eval_log`).
    pub preferred_candidate: String,
}

/// `evaluate_vocabulary` — the declared-vocabulary check: the
/// supplied facts equal the landed `ForeignFormat` decls verbatim. A
/// drifted fact is a different vocabulary — refused, never
/// relabelled; an absent sum is inconclusive.
pub fn evaluate_vocabulary(f: &VocabularyFacts) -> LegVerdict {
    if f.closed_sum.is_empty() {
        return LegVerdict::Inconclusive("vocabulary_absent".to_string());
    }
    if f.formats != 7 {
        return LegVerdict::Refused(format!("formats:{}", f.formats));
    }
    if f.closed_sum != EXPECTED_SUM {
        return LegVerdict::Refused(format!("closed_sum:{}", f.closed_sum));
    }
    if !f.parse_roundtrip {
        return LegVerdict::Refused("parse_roundtrip:false".to_string());
    }
    if !f.parse_never_guesses {
        return LegVerdict::Refused("parse_never_guesses:false".to_string());
    }
    if f.import_only != "harbor_trial_dir" {
        return LegVerdict::Refused(format!("import_only:{}", f.import_only));
    }
    if f.export_only != "interchange_trajectory" {
        return LegVerdict::Refused(format!("export_only:{}", f.export_only));
    }
    if f.bidirectional != 5 {
        return LegVerdict::Refused(format!("bidirectional:{}", f.bidirectional));
    }
    if f.liftable != EXPECTED_LIFTABLE {
        return LegVerdict::Refused(format!("liftable:{}", f.liftable));
    }
    if !f.liftable_is_import_list {
        return LegVerdict::Refused("liftable_is_import_list:false".to_string());
    }
    // Byte-assert the committed decl spelling — `preferred_candidate`
    // is a contract decl, not a machinery-derived fact, so it binds
    // the literal verbatim (a drifted decl is not S6's recorded
    // preferred format; refused, never relabelled).
    if f.preferred_candidate != "inspect_eval_log" {
        return LegVerdict::Refused(format!("preferred_candidate:{}", f.preferred_candidate));
    }
    LegVerdict::Verified
}

// ── leg 4 — refusal_vocabulary ───────────────────────────────────

/// The exercised case set — `exercise → expected typed refusal
/// variant` (the landed `lift_foreign`/`export_target`/`lift`
/// machinery's verbatim refusal spellings, driven on exercised cases
/// only: the refusals fire before any foreign artifact is produced
/// or consumed). The direction cases cover AC-1-2's "unsupported
/// direction" controls — `interchange_trajectory` at `import` is the
/// return-import guard itself (OQ-030/ADR-0213 — the unasked-import
/// predicate standing); the unknown-format cases cover the
/// unsupported-version/spelling grammar (`FormatUnknown`, never a
/// guess).
pub const EXERCISES: [(&str, &str); 4] = [
    ("interchange_at_import", "ForeignDirectionRefused"),
    ("harbor_trial_at_export", "ForeignDirectionRefused"),
    ("unknown_spelling_export", "FormatUnknown"),
    ("foreign_at_native_lift", "FormatUnknown"),
];

/// `evaluate_refusals` — the machinery-stands check: every declared
/// exercise produced its typed refusal verbatim. A missing exercise
/// is `inconclusive`; a wrong refusal — or `exported`/`lifted`/
/// `parsed`, the guard broken — is `refused`, never reclassified.
pub fn evaluate_refusals(observed: &BTreeMap<String, String>) -> LegVerdict {
    for (exercise, expected) in EXERCISES {
        match observed.get(exercise).map(String::as_str) {
            Some(o) if o == expected => {}
            Some("exported") | Some("lifted") | Some("parsed") | Some("admitted") => {
                return LegVerdict::Refused(format!("produced:{exercise}"));
            }
            Some(o) => return LegVerdict::Refused(format!("{exercise}:{o}")),
            None => return LegVerdict::Inconclusive(format!("exercise_absent:{exercise}")),
        }
    }
    LegVerdict::Verified
}

// ── leg 5 — exercise_absent ──────────────────────────────────────

/// The absent artifact→format→reader exercise, verbatim — every
/// §1.1/§1.4 phase and control count a declared zero, no selection,
/// no consumer contact, no return import attempted.
#[derive(Debug, Clone, PartialEq)]
pub struct ExerciseFacts {
    /// `member → count` — `native_artifacts_exported`,
    /// `foreign_files_produced`, `external_readers_invoked`,
    /// `reader_verdicts`, `fields_compared`, `losses_declared`,
    /// `direction_controls_run`, `version_controls_run`,
    /// `return_imports`, `native_authority_imports`,
    /// `participant_understanding_records` each declared `0`.
    pub counts: BTreeMap<String, u64>,
    /// A consumer/direction selection was attempted (`false`).
    pub selection_attempted: bool,
    /// An external reader/validator was contacted (`false`).
    pub consumer_contacted: bool,
    /// A return import was performed or attempted (`false` — the
    /// `interchange_trajectory` import half stays OQ-030-deferred;
    /// exercised only as the typed refusal).
    pub return_import_attempted: bool,
}

/// The eleven absent-exercise members (frozen vocabulary).
pub const EXERCISE_MEMBERS: [&str; 11] = [
    "native_artifacts_exported",
    "foreign_files_produced",
    "external_readers_invoked",
    "reader_verdicts",
    "fields_compared",
    "losses_declared",
    "direction_controls_run",
    "version_controls_run",
    "return_imports",
    "native_authority_imports",
    "participant_understanding_records",
];

/// `evaluate_exercise` — the absent-exercise check: every member
/// zero, no selection, no consumer contact, no return import. Any
/// nonzero fact is a real exercise's evidence — it belongs in a
/// different packet and refuses here, never a fabricated absence.
pub fn evaluate_exercise(f: &ExerciseFacts) -> LegVerdict {
    for member in EXERCISE_MEMBERS {
        match f.counts.get(member) {
            Some(0) => {}
            Some(n) => return LegVerdict::Refused(format!("{member}:{n}")),
            None => return LegVerdict::Inconclusive(format!("{member}_absent")),
        }
    }
    if f.selection_attempted {
        return LegVerdict::Refused("selection_attempted:true".to_string());
    }
    if f.consumer_contacted {
        return LegVerdict::Refused("consumer_contacted:true".to_string());
    }
    if f.return_import_attempted {
        return LegVerdict::Refused("return_import_attempted:true".to_string());
    }
    LegVerdict::Verified
}

// ── leg 6 — claims_withheld ──────────────────────────────────────

/// The withheld-claims facts — DF-H9-1 stays open, every
/// participant/outreach predicate withheld and no claim of any sign
/// is present (I-08: unselected awards no claim; §1.4 AC-1-2: "does
/// not establish round-trip/reverse conversion, general
/// interoperability or stronger native evidence"; REQ-B-102:
/// foreign imported claims never acquire native authority from
/// conversion).
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
    /// A round-trip claim (`false` — S12: the return import is not
    /// required and is never claimed).
    pub round_trip_claim: bool,
    /// A reverse-direction conversion claim (`false`).
    pub reverse_direction_claim: bool,
    /// A general interoperability claim (`false`).
    pub interop_claim: bool,
    /// A compatibility claim (`false` — AC-1-2: the unselected S6
    /// awards none).
    pub compat_claim: bool,
    /// A native-authority-from-conversion claim (`false` —
    /// REQ-B-102: imported foreign claims never acquire it).
    pub native_authority_claim: bool,
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
        || f.round_trip_claim
        || f.reverse_direction_claim
        || f.interop_claim
        || f.compat_claim
        || f.native_authority_claim
        || f.publication_claim
        || f.capability_claim
        || f.adoption_claim
    {
        return LegVerdict::Refused("claim_present".to_string());
    }
    LegVerdict::Verified
}

// ── the report ───────────────────────────────────────────────────

/// The `hh-interchange-unselected/1` record — the honest
/// `unselected{stage_voided}` disposition: the optional S6 exercise
/// is unselected, every artifact→format→reader phase absent, all
/// participant/outreach/compatibility claims withheld and DF-H9-1
/// open; `packet_id` is minted last over the canonical record.
#[derive(Debug, Clone, PartialEq)]
pub struct InterchangeUnselectedReport {
    /// `H(canonical(record minus packet_id))` under
    /// `interchange_unselected`.
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
    /// Interchange exercises completed (`0`).
    pub exercises_completed: u64,
    /// The six legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// The admission shape — `unselected_stage_voided`, never an
    /// execution.
    pub admission: String,
}

impl InterchangeUnselectedReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("packet_id", Json::str(&self.packet_id)),
            ("gate", Json::str(&self.gate)),
            ("unselected", Json::Bool(self.unselected)),
            ("stage_voided", Json::Bool(self.stage_voided)),
            ("df_h9_1_open", Json::Bool(self.df_h9_1_open)),
            (
                "exercises_completed",
                Json::Int(self.exercises_completed as i64),
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
/// disposition and mint `packet_id` last over the canonical
/// record. Every leg name must be a `LEGS` member, unique and
/// complete — a malformed leg set is a `refused` assembly, never a
/// partial packet.
pub fn assemble(legs: Vec<Leg>) -> Result<InterchangeUnselectedReport, String> {
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
    let mut out = InterchangeUnselectedReport {
        packet_id: String::new(),
        version: VERSION.to_string(),
        gate: GATE.to_string(),
        unselected: true,
        stage_voided: true,
        df_h9_1_open: true,
        exercises_completed: 0,
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
