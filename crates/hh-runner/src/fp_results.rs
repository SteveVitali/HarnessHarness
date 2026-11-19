//! R6.48 — FP-RESULTS (`hh-fp-seal-open/1`, `hh-fp-answer-scan-report/1`,
//! the `hh-comparative-results/1` withheld packets, `hh-fp-l2/1`,
//! `hh-fp-l2-audit/1`, `hh-fp-l3/1`).
//!
//! The outcome seal opens exactly once, here (REQ-XH-121): every
//! W2/W4 agent-participant slot's outcome fields stayed sealed until
//! FP-RESULTS, and this module's [`seal_open_record`] emits the one
//! recorded open act. The act is a firewall, not a formality — every
//! committed record that consumed first-pass evidence before this
//! point is re-checked through [`crate::fp_seal`]'s class checks, and
//! a record carrying an unmasked outcome member refuses the open.
//!
//! Under `re-scope{end-first-pass-early}` (GATE-G30's verbatim
//! disposition) the W4 tranche is `unused{scope_end}` — no claim
//! packet computes. Every emitted packet keeps the computed packet's
//! members verbatim and replaces each uncomputable leaf with a typed
//! marker (`{"state": <absent-state>, "reason": "withheld{…}"}` —
//! REQ-XH-020/REQ-XF-122: equal structure and prominence, never
//! `null`, never a dropped family member). The typed reasons are the
//! FP-E record's verbatim rows — `withheld{budget}`,
//! `withheld{window_deadline}`, `withheld{admission:AG3}` for the
//! dropped arms and
//! `withheld{window_deadline}:protected_pair_truncated{k<5,R<5}` for
//! every claim refusal.
//!
//! The answer scan ([`crate::answer_scan`]) runs over every scored
//! trial that exists in the retained harvest — the 37 `hh-fp-trials/1`
//! subject legs plus the 20 `l10-*` L1-corpus replay recordings (their
//! verifier ran; they are scored cells, and they are exactly the
//! W2-corpus-class recordings the L1-on-L2 audit must refuse to
//! sample). The REQ-XD-071 flagged-pass review handoff folds through
//! [`crate::answer_scan::flagged_report`] — a missing scan is
//! `unresolved`, never clean, and only excerpt hashes leave the
//! module.

use std::collections::{BTreeMap, BTreeSet};

use hharness_eval::comparative_estimators::{
    LABEL_CONFIRMED_MIN, LABEL_EXPLORATORY_LEADER, LABEL_FIRST_PASS, LABEL_MODEL_SPEND,
    LABEL_SECONDARY_POP, PRIMARY_METRIC, REQUIRED_LABELS, REQUIRED_POPULATIONS, RESULTS_SCHEMA,
};
use hharness_eval::l2_wire::{dimension_map_record, typed_unknowns, L2_SCHEMA};
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::answer_scan::{
    flagged_report, FingerprintCorpus, ScanError, ScanInput, ANSWER_SCAN_VERSION,
};
use crate::fp_seal::{check_sealed, check_unsealed, SealClass, SealError};

/// `hh-fp-seal-open/1` — the one recorded outcome-seal open act
/// (REQ-XH-121, AC-1 `AC-6.5-1`).
pub const SEAL_OPEN_SCHEMA: &str = "hh-fp-seal-open/1";
/// `hh-fp-answer-scan-report/1` — the folded scan record + the
/// REQ-XD-071 review handoff (AC-2).
pub const SCAN_REPORT_SCHEMA: &str = "hh-fp-answer-scan-report/1";
/// `hh-fp-l2-audit/1` — the L1-on-L2 audit record (REQ-XD-119).
pub const L2_AUDIT_SCHEMA: &str = "hh-fp-l2-audit/1";
/// `hh-fp-l3/1` — the per-pair L3 report (REQ-XD-120).
pub const L3_SCHEMA: &str = "hh-fp-l3/1";

/// The verbatim FP-E claim-refusal spelling (the protected pair ran
/// `k = 0` trials of `k = 5` — `protected_pair_truncated{k<5,R<5}`).
pub const CLAIM_REFUSAL: &str = "withheld{window_deadline}:protected_pair_truncated{k<5,R<5}";
/// The first-pass label every claim packet carries (REQ-XH-123).
pub const FP_CLAIM_LABEL: &str =
    "first pass — capability demonstration on 10 held-out TB 2.1 tasks at k = 5; \
     below the declared precision target; not leaderboard-comparable";
/// The declared emission timestamp (the ticket is clock-free).
pub const RECORDED_AT: &str = "2026-11-19T00:00Z (declared; the ticket is clock-free)";

const FAMILY_ALPHA_PPM: i64 = 50_000;
const L2_EPSILON_PPM: i64 = 200_000;
const L2_PLANNED_N: i64 = 50;
const FP_MIN_LEN: usize = 24;
const SHINGLE_LINES: usize = 5;

/// A typed leaf marker — `{"state": <absent-state>, "reason": …}`; an
/// `ABSENT_STATES` member so every consumer that accepts the computed
/// packet's typed-absent leaves accepts the withheld form identically.
pub fn marker(state: &str, reason: &str) -> Json {
    Json::obj([("state", Json::str(state)), ("reason", Json::str(reason))])
}

fn refused(reason: &str) -> Json {
    marker("refused", reason)
}

fn not_computed(reason: &str) -> Json {
    marker("not_computed", reason)
}

/// `Json::obj` takes `&'static str` keys — dynamic keys (participant
/// spellings, arm names) build through the map directly.
fn obj_dyn(pairs: impl IntoIterator<Item = (String, Json)>) -> Json {
    Json::Obj(pairs.into_iter().collect())
}

// ── AC-1: the seal-open record ───────────────────────────────────────

/// One pre-existing record re-checked under the open act — `class` is
/// its [`SealClass`]; an `AgentSlot` projection goes through
/// [`check_sealed`], every carve-out through [`check_unsealed`].
pub struct SealCheck {
    /// The cited path (repo-relative, or `retained:<rel>`).
    pub path: String,
    /// The parsed record.
    pub record: Json,
    /// Its declared seal class.
    pub class: SealClass,
}

/// `seal_open_record(checks, readout_cites)` — the one recorded open
/// act. `checks` is every committed/retained record that consumed
/// first-pass evidence while sealed; each must pass its class check or
/// the open refuses (the AC-1 firewall: an outcome field read before
/// this act fails). `cited` carries `(path, masking_leg)` for records
/// and readouts the mechanical member-name check does not apply to —
/// the reason each is cited rather than checked is recorded verbatim.
pub fn seal_open_record(
    checks: &[SealCheck],
    cited: &[(String, String)],
) -> Result<Json, SealError> {
    let mut firewall: Vec<Json> = Vec::new();
    for c in checks {
        let result = match c.class {
            SealClass::AgentSlot => check_sealed(&c.record),
            other => check_unsealed(&c.record, other),
        };
        result?;
        firewall.push(Json::obj([
            ("path", Json::str(&c.path)),
            ("class", Json::str(c.class.as_str())),
            (
                "sha256",
                Json::str(format!(
                    "sha256:{}",
                    sha256_hex(c.record.to_canonical_string().as_bytes())
                )),
            ),
            ("check", Json::str("pass")),
        ]));
    }
    let mut cites: Vec<Json> = Vec::new();
    for (path, leg) in cited {
        cites.push(Json::obj([
            ("path", Json::str(path)),
            ("masking_leg", Json::str(leg)),
        ]));
    }
    Ok(Json::obj([
        ("schema", Json::str(SEAL_OPEN_SCHEMA)),
        ("act", Json::str("seal_open")),
        ("req", Json::str("REQ-XH-121")),
        ("opened_classes", Json::Arr(vec![Json::str("agent_slot")])),
        (
            "scope",
            Json::str(
                "W2/W4 agent-participant slot outcome fields — verdict, reward, \
                 ctrf counts, executed disposition, verifier output and derivatives",
            ),
        ),
        ("opens", Json::Int(1)),
        (
            "precedence",
            Json::str(
                "no outcome field read precedes this act — every cited record \
                 passes its fp_seal class check; a record carrying an unmasked \
                 outcome member refuses the open",
            ),
        ),
        (
            "precedence_check",
            Json::obj([
                ("records", Json::Arr(firewall)),
                ("cited", Json::Arr(cites)),
            ]),
        ),
        (
            "unsealed_since",
            Json::str(
                "agent-slot outcome fields are reportable from this record's \
                 emission onward — the seal was never opened earlier",
            ),
        ),
        ("seal_marker", Json::str(crate::fp_seal::seal_marker())),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str("R6.48")),
    ]))
}

// ── AC-2: the folded answer-scan report ──────────────────────────────

/// A normalized line fingerprint input: `(kind, file_name, bytes)`
/// where `kind` is `solution` | `test` | `readme`.
pub type CorpusFile = (String, String, Vec<u8>);

/// `fingerprint_corpus(task_id, files)` — deterministic corpus build:
/// lines = whitespace-collapsed non-empty lines ≥ `answer-scan`'s
/// fingerprint length floor; shingles = `SHINGLE_LINES` consecutive
/// normalized lines joined by ` ⏎ `.
pub fn fingerprint_corpus(task_id: &str, files: &[CorpusFile]) -> FingerprintCorpus {
    let norm = |b: &[u8]| -> Vec<String> {
        String::from_utf8_lossy(b)
            .lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|l| !l.is_empty())
            .collect()
    };
    let mut lines: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut shingles: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut kinds: BTreeSet<String> = BTreeSet::new();
    for (kind, _name, _bytes) in files {
        kinds.insert(kind.clone());
    }
    for kind in kinds {
        let mut ls: Vec<String> = Vec::new();
        let mut file_lines: Vec<String> = Vec::new();
        for (_k, _name, bytes) in files.iter().filter(|(k, _, _)| k == &kind) {
            for l in norm(bytes) {
                if l.len() >= FP_MIN_LEN {
                    ls.push(l.clone());
                }
                file_lines.push(l);
            }
        }
        ls.sort();
        ls.dedup();
        let sh: Vec<String> = file_lines
            .windows(SHINGLE_LINES)
            .map(|w| w.join(" ⏎ "))
            .filter(|s| s.len() >= FP_MIN_LEN)
            .collect();
        lines.insert(kind.clone(), ls);
        shingles.insert(kind, sh);
    }
    FingerprintCorpus {
        task_id: task_id.to_string(),
        lines,
        shingles,
    }
}

/// One scored trial's scan bundle plus the post-seal verdict fact.
pub struct SlotScan {
    /// The slot (`ws1-corpus-01`, `ws1-pp0-3`, `l10-ws1-corpus-04-a`).
    pub slot_ref: String,
    /// The task.
    pub task_id: String,
    /// The arm.
    pub arm: String,
    /// The opened verdict fact (`reward ≥ 1` → pass).
    pub pass: bool,
    /// `subject` | `l1_corpus_recording`.
    pub kind: String,
    /// The decoded `hh-integrity-bundle/1` roles.
    pub input: ScanInput,
    /// The fingerprint corpus for this slot's task.
    pub corpus: Option<FingerprintCorpus>,
    /// Content-addressed receipt references (digests, never bodies).
    pub receipt_refs: Vec<String>,
    /// Per-bundle-member digests the record cites.
    pub member_digests: Vec<(String, String)>,
}

/// `scan_report(slots, sources)` — the folded `hh-fp-answer-scan-report/1`:
/// one `answer-scan/1` record per scored trial, coverage, the flagged-pass
/// list (masked identity + excerpt hashes), the REQ-XD-071 review handoff
/// and the `hh-flagged-review-report/1` fold (no review rows exist yet —
/// HUMAN-H16 reviews later; `reviewed_flags`/`confirmed`/`cleared` = 0).
/// A slot whose bundle is incomplete lands on `unresolved`, never clean.
pub fn scan_report(
    slots: &[SlotScan],
    sources: Vec<(String, String)>,
) -> Result<Json, Vec<String>> {
    let mut records: Vec<Json> = Vec::new();
    let mut flagged_passes: Vec<Json> = Vec::new();
    let mut unresolved: Vec<Json> = Vec::new();
    let mut scanned: u64 = 0;
    let mut flagged_n: u64 = 0;
    let mut hit_n: u64 = 0;
    for s in slots {
        match crate::answer_scan::scan(&s.input, s.corpus.as_ref(), None) {
            Ok(rec) => {
                scanned += 1;
                if rec.state == "flagged" {
                    flagged_n += 1;
                }
                hit_n += rec.hits.len() as u64;
                // the module's own scoring fold — a flagged pass is
                // veto-pending until a recorded review clears it.
                let st = crate::answer_scan::scored_state(
                    true,
                    s.pass && !rec.hits.is_empty(),
                    false,
                    false,
                );
                if s.pass && rec.state == "flagged" {
                    flagged_passes.push(Json::obj([
                        ("slot_ref", Json::str(&s.slot_ref)),
                        ("harness_masked", Json::Bool(true)),
                        (
                            "excerpt_sha256s",
                            Json::Arr(
                                rec.hits
                                    .iter()
                                    .map(|h| Json::str(&h.excerpt_sha256))
                                    .collect(),
                            ),
                        ),
                    ]));
                }
                records.push(Json::obj([
                    ("slot_ref", Json::str(&rec.slot_ref)),
                    ("task_id", Json::str(&rec.task_id)),
                    ("arm", Json::str(&s.arm)),
                    ("kind", Json::str(&s.kind)),
                    ("pass", Json::Bool(s.pass)),
                    ("state", Json::str(&rec.state)),
                    ("scored_state", Json::str(st)),
                    ("detector", Json::str(&rec.detector)),
                    (
                        "hits",
                        Json::Arr(
                            rec.hits
                                .iter()
                                .map(|h| {
                                    Json::obj([
                                        ("signal", Json::str(&h.signal)),
                                        ("source", Json::str(&h.source)),
                                        ("excerpt_sha256", Json::str(&h.excerpt_sha256)),
                                        ("scored", Json::Bool(h.scored)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    (
                        "requested_only",
                        Json::Arr(rec.requested_only.iter().map(Json::str).collect()),
                    ),
                    (
                        "receipt_refs",
                        Json::Arr(s.receipt_refs.iter().map(Json::str).collect()),
                    ),
                    (
                        "bundle_members",
                        Json::Arr(
                            s.member_digests
                                .iter()
                                .map(|(role, d)| {
                                    Json::obj([("role", Json::str(role)), ("sha256", Json::str(d))])
                                })
                                .collect(),
                        ),
                    ),
                ]));
            }
            Err(ScanError { rule, detail }) => {
                unresolved.push(Json::obj([
                    ("slot_ref", Json::str(&s.slot_ref)),
                    ("state", Json::str("unresolved")),
                    ("rule", Json::str(rule)),
                    ("detail", Json::str(&detail)),
                ]));
            }
        }
    }
    let report = flagged_report((scanned, flagged_n), &[]).map_err(|e| {
        e.iter()
            .map(|x| format!("flagged_report: {x}"))
            .collect::<Vec<_>>()
    })?;
    let src: Vec<Json> = sources
        .iter()
        .map(|(p, d)| Json::obj([("path", Json::str(p)), ("sha256", Json::str(d))]))
        .collect();
    Ok(Json::obj([
        ("schema", Json::str(SCAN_REPORT_SCHEMA)),
        ("detector", Json::str(ANSWER_SCAN_VERSION)),
        ("req", Json::str("REQ-XD-070/REQ-XD-071")),
        (
            "coverage",
            Json::obj([
                ("scored_trials", Json::Int(slots.len() as i64)),
                ("scanned", Json::Int(scanned as i64)),
                ("unresolved", Json::Int(unresolved.len() as i64)),
                ("flagged", Json::Int(flagged_n as i64)),
                ("scored_hits", Json::Int(hit_n as i64)),
            ]),
        ),
        ("records", Json::Arr(records)),
        ("unresolved", Json::Arr(unresolved)),
        (
            "flagged_pass_list",
            Json::obj([
                ("rows", Json::Arr(flagged_passes)),
                ("identity_masking", Json::str("harness_masked")),
                (
                    "excerpts",
                    Json::str("excerpt_sha256 only — never matched bytes"),
                ),
            ]),
        ),
        (
            "review_handoff",
            Json::obj([
                ("schema", Json::str("hh-flagged-review-input/1")),
                ("input_to", Json::str("HUMAN-H16")),
                ("records", Json::Arr(Vec::new())),
                (
                    "note",
                    Json::str(
                        "the review rows are HUMAN-H16's act — this handoff carries \
                         the flagged-pass list; until review lands, confirmed stays 0 \
                         and every flagged pass is veto-pending, never cleared",
                    ),
                ),
            ]),
        ),
        ("review_report", report),
        (
            "request_fingerprint_leg",
            Json::obj([
                ("status", Json::str("not_checked")),
                (
                    "reason",
                    Json::str(
                        "the retained proxy receipts carry request bodies only — no \
                         user-agent or header-name facts were captured by the WAL; \
                         the admission-declared fingerprint check (REQ-XC-313) lands \
                         when the receipt schema records header facts",
                    ),
                ),
            ]),
        ),
        ("label", Json::str(LABEL_CONFIRMED_MIN)),
        ("sources", Json::Arr(src)),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str("R6.48")),
    ]))
}

// ── the withheld `hh-comparative-results/1` packet ───────────────────

/// One arm's withheld row — the same members a computed row carries,
/// every uncomputable leaf a typed marker.
#[derive(Debug, Clone)]
pub struct WithheldArm {
    /// The harness key (`pi`, `claude-code`, `v5`, `hh_native`, `mini-swe-agent`).
    pub key: String,
    /// The participant spelling in `counts.served` (`hh/pi@0.52.3`).
    pub participant: String,
    /// The intended-assignment participant spelling (`pi@0.52.3`).
    pub intended_participant: String,
    /// The verbatim FP-E arm disposition (`withheld{admission:AG3}`, …).
    pub disposition: String,
    /// Planned trials (the tranche's k × tasks).
    pub planned: i64,
    /// The ws1-measured per-trial cost p̂ where a measurement exists
    /// (cost fields were never sealed), else `None`.
    pub measured_cost_micros: Option<i64>,
    /// The reference arms `swap_vs`/`harness_excess` names.
    pub swap_refs: Vec<String>,
}

fn withheld_arm_row(a: &WithheldArm, is_ref: bool, claim_refusal: &str) -> Json {
    let arm_reason = a.disposition.clone();
    let finite_label =
        "finite-manifest rate over the 10-task tranche — internal record — not outside-verified";

    let served = obj_dyn([(a.participant.clone(), Json::Int(0))]);
    let intended = obj_dyn([(a.intended_participant.clone(), Json::Int(a.planned))]);
    let mut flips_swap: Vec<(String, Json)> = Vec::new();
    let mut flips_excess: Vec<(String, Json)> = Vec::new();
    for r in &a.swap_refs {
        flips_swap.push((
            r.clone(),
            Json::obj([
                ("interval", refused(&arm_reason)),
                ("label", Json::str("deterministic resample pair-flip")),
            ]),
        ));
        flips_excess.push((
            r.clone(),
            Json::obj([
                ("ppm", refused(&arm_reason)),
                ("interval", refused(&arm_reason)),
                ("label", Json::str("harness excess vs the resampled pair")),
            ]),
        ));
    }
    let per_trial = match a.measured_cost_micros {
        Some(m) => Json::obj([
            ("micros", Json::Int(m)),
            (
                "interval",
                not_computed(
                    "a tranche interval over k=50 trials is not computable \
                     from the ≤9 development-task legs the measurement ran",
                ),
            ),
            ("basis", Json::str("ws1_measured_p_hat")),
            (
                "label",
                Json::str(format!("descriptive; {LABEL_MODEL_SPEND}")),
            ),
        ]),
        None => Json::obj([
            ("micros", refused(&arm_reason)),
            ("interval", refused(&arm_reason)),
            ("basis", Json::str("no executed tranche")),
            (
                "label",
                Json::str(format!("descriptive; {LABEL_MODEL_SPEND}")),
            ),
        ]),
    };
    Json::obj([
        ("disposition", Json::str(&a.disposition)),
        (
            "theta",
            Json::obj([
                ("ppm", refused(&arm_reason)),
                ("lo_ppm", refused(&arm_reason)),
                ("hi_ppm", refused(&arm_reason)),
                ("interval", refused(&arm_reason)),
                (
                    "bounds",
                    Json::obj([
                        ("lo_ppm", Json::Int(0)),
                        ("hi_ppm", Json::Int(1_000_000)),
                        ("kind", Json::str("all_planned")),
                    ]),
                ),
                ("label", Json::str(finite_label)),
            ]),
        ),
        (
            "secondary_populations",
            Json::obj([
                (
                    "raw",
                    Json::obj([
                        ("rate_ppm", refused(&arm_reason)),
                        ("label", Json::str(LABEL_SECONDARY_POP)),
                    ]),
                ),
                (
                    "errors_zero",
                    Json::obj([
                        ("bound_lo_ppm", Json::Int(0)),
                        ("bound_hi_ppm", Json::Int(1_000_000)),
                        (
                            "label",
                            Json::str(format!(
                                "{LABEL_SECONDARY_POP} — errors=0 all-planned bound"
                            )),
                        ),
                    ]),
                ),
            ]),
        ),
        (
            "delta_vs_ref",
            if is_ref {
                Json::obj([
                    ("ppm", Json::Int(0)),
                    (
                        "interval",
                        Json::obj([
                            ("lo_ppm", Json::Int(0)),
                            ("hi_ppm", Json::Int(0)),
                            ("kind", Json::str("degenerate_reference")),
                        ]),
                    ),
                    ("p_raw_ppm", Json::Int(1_000_000)),
                    ("p_holm_ppm", Json::Int(1_000_000)),
                    ("label", Json::str("reference arm — Δ ≡ 0 by construction")),
                ])
            } else {
                refused(claim_refusal)
            },
        ),
        (
            "leader_group",
            Json::obj([
                ("member", Json::Bool(false)),
                ("leader_id", Json::str("unavailable{no_served_arm}")),
                ("diff_interval", refused(&arm_reason)),
                ("label", Json::str(LABEL_EXPLORATORY_LEADER)),
            ]),
        ),
        (
            "pass_at",
            Json::obj([
                ("k", Json::Int(5)),
                ("all_ppm", refused(&arm_reason)),
                ("any_ppm", refused(&arm_reason)),
                (
                    "label",
                    Json::str("pass^k / pass@k over the tranche's k=5 repeats"),
                ),
            ]),
        ),
        (
            "flips",
            Json::obj([
                ("rerun", refused(&arm_reason)),
                ("swap_vs", obj_dyn(flips_swap)),
                ("harness_excess", obj_dyn(flips_excess)),
                (
                    "label",
                    Json::str("descriptive flip diagnostics — ignores task clustering"),
                ),
            ]),
        ),
        (
            "cost",
            Json::obj([
                ("per_trial", per_trial),
                (
                    "per_solve",
                    marker(
                        "estimator_undefined",
                        "0 solved of 0 served — the tranche never executed",
                    ),
                ),
                ("provenance", Json::str("model_spend_only")),
            ]),
        ),
        (
            "counts",
            Json::obj([
                ("analysis_basis", Json::str("assignment")),
                ("intended", intended),
                ("served", served),
            ]),
        ),
        (
            "completion",
            Json::obj([
                ("planned", Json::Int(a.planned)),
                ("first", Json::Int(0)),
                ("recovered", Json::Int(0)),
                ("unresolved", Json::Int(0)),
                ("withheld", Json::Int(a.planned)),
            ]),
        ),
        (
            "integrity",
            Json::obj([
                ("flagged", Json::Int(0)),
                ("confirmed", Json::Int(0)),
                ("cleared", Json::Int(0)),
                ("coverage_ppm", Json::Int(0)),
                ("detector", Json::str(ANSWER_SCAN_VERSION)),
                (
                    "label",
                    Json::str(format!(
                        "integrity panel — {LABEL_CONFIRMED_MIN}; 0 tranche slots \
                         executed, so coverage is 0 (the ws1 development legs' scan \
                         lands in the committed answer-scan report)"
                    )),
                ),
            ]),
        ),
    ])
}

/// One Holm/BH family member row.
#[derive(Debug, Clone)]
pub struct FamilyMember {
    /// The member name (the arm or the contrast).
    pub member: String,
    /// `available` | `unavailable`.
    pub status: String,
    /// The retained typed reason when unavailable.
    pub disposition: Option<String>,
    /// The computed members when available (`raw_p_ppm`, `adjusted_ppm`).
    pub computed: Option<(i64, i64)>,
}

fn family_row(m: &FamilyMember) -> Json {
    let mut row: Vec<(&str, Json)> = Vec::new();
    row.push(("member", Json::str(&m.member)));
    row.push(("status", Json::str(&m.status)));
    match (&m.disposition, m.computed) {
        (Some(d), _) => {
            row.push(("raw_p_ppm", Json::Null));
            row.push(("adjusted_ppm", Json::Int(1_000_000)));
            row.push(("disposition", Json::str(d)));
        }
        (None, Some((raw, adj))) => {
            row.push(("raw_p_ppm", Json::Int(raw)));
            row.push(("adjusted_ppm", Json::Int(adj)));
        }
        (None, None) => {
            row.push(("raw_p_ppm", Json::Null));
            row.push(("adjusted_ppm", Json::Int(1_000_000)));
        }
    }
    Json::obj(row)
}

/// `withheld_results_packet(...)` — the `hh-comparative-results/1`
/// withheld form: every member the computed packet carries, each
/// uncomputable leaf a typed marker, the Holm family retaining every
/// unavailable member (REQ-XD-030), and `packet_disposition` carrying
/// the verbatim claim refusal.
pub fn withheld_results_packet(
    claim: &str,
    reference: &str,
    arms: &[WithheldArm],
    family_members: &[FamilyMember],
    expl_members: &[FamilyMember],
    expected_precision: &str,
    sources: Json,
) -> Json {
    let labels: Vec<(String, Json)> = REQUIRED_LABELS
        .iter()
        .map(|(k, subs)| {
            (
                k.to_string(),
                Json::str(subs.join("; ") + " — " + FP_CLAIM_LABEL),
            )
        })
        .collect();
    let mut harnesses: Vec<(String, Json)> = Vec::new();
    for a in arms {
        harnesses.push((
            a.key.clone(),
            withheld_arm_row(a, a.key == reference, CLAIM_REFUSAL),
        ));
    }
    Json::obj([
        ("schema", Json::str(RESULTS_SCHEMA)),
        ("packet_disposition", Json::str(CLAIM_REFUSAL)),
        ("claim", Json::str(claim)),
        (
            "tranche",
            Json::obj([
                ("id", Json::str("fp-w4")),
                ("state", Json::str("unused{scope_end}")),
                ("tasks", Json::Int(10)),
                ("k", Json::Int(5)),
            ]),
        ),
        (
            "family",
            Json::obj([
                ("id", Json::str(claim)),
                ("method", Json::str("holm")),
                ("alpha_ppm", Json::Int(FAMILY_ALPHA_PPM)),
                ("reference", Json::str(reference)),
                (
                    "members",
                    Json::Arr(family_members.iter().map(family_row).collect()),
                ),
            ]),
        ),
        (
            "exploratory_family",
            Json::obj([
                ("id", Json::str(format!("{claim}-expl"))),
                ("method", Json::str("benjamini_hochberg")),
                (
                    "members",
                    Json::Arr(expl_members.iter().map(family_row).collect()),
                ),
            ]),
        ),
        ("metric", Json::str(PRIMARY_METRIC)),
        (
            "populations",
            Json::Arr(REQUIRED_POPULATIONS.iter().map(|s| Json::str(*s)).collect()),
        ),
        ("labels", obj_dyn(labels)),
        ("harnesses", obj_dyn(harnesses)),
        (
            "executed_evidence",
            Json::str(
                "no tranche trial executed — W4 is unused{scope_end}; the ws1 \
                 subject legs were development-task admission/machinery evidence \
                 and never enter a claim population",
            ),
        ),
        ("expected_precision", Json::str(expected_precision)),
        (
            "seal_open_ref",
            Json::str("docs/build/reports/2026-11-19-fp-seal-open.json"),
        ),
        ("sources", sources),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str("R6.48")),
    ])
}

/// `key_shape(j)` — the recursive member-path set; the structural
/// twin check (`same shape, typed leaves`) compares two packets'
/// shapes while ignoring leaf values.
pub fn key_shape(j: &Json) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    fn walk(j: &Json, p: &str, out: &mut BTreeSet<String>) {
        match j {
            Json::Obj(m) => {
                for (k, v) in m {
                    let q = if p.is_empty() {
                        k.clone()
                    } else {
                        format!("{p}.{k}")
                    };
                    out.insert(q.clone());
                    walk(v, &q, out);
                }
            }
            Json::Arr(a) => {
                for v in a {
                    walk(v, p, out);
                }
            }
            _ => {}
        }
    }
    walk(j, "", &mut out);
    out
}

/// `{"state": <ABSENT_STATES member>, "reason": <string>}` — the typed
/// marker every withheld leaf carries.
pub fn is_typed_marker(v: &Json) -> bool {
    v.get("state")
        .and_then(Json::as_str)
        .map(|s| hharness_eval::comparative_estimators::ABSENT_STATES.contains(&s))
        .unwrap_or(false)
        && v.get("reason").and_then(Json::as_str).is_some()
}

fn leaf_int(v: &Json, path: &str, errs: &mut Vec<String>) {
    if v.as_int().is_none() && !is_typed_marker(v) {
        errs.push(format!("{path}: int or typed marker required"));
    }
}

fn leaf_interval(v: &Json, path: &str, errs: &mut Vec<String>) {
    if is_typed_marker(v) {
        return;
    }
    match v {
        Json::Obj(m) => {
            for k in ["lo_ppm", "hi_ppm"] {
                match m.get(k) {
                    Some(x) if x.as_int().is_some() || is_typed_marker(x) => {}
                    _ => errs.push(format!("{path}.{k}: int or typed marker required")),
                }
            }
            if m.get("kind").and_then(Json::as_str).is_none() {
                errs.push(format!("{path}.kind: required"));
            }
        }
        _ => errs.push(format!("{path}: interval or typed marker required")),
    }
}

/// `validate_withheld_results_packet(j)` — the withheld-form twin of
/// `validate_results_packet`: identical required members where each
/// leaf admits the computed type OR a typed marker, plus the withheld
/// packet's own required members (`packet_disposition`, per-arm
/// `disposition`, every family member's `status`, mandatory labels).
pub fn validate_withheld_results_packet(j: &Json) -> Result<(), Vec<String>> {
    let mut errs: Vec<String> = Vec::new();
    // ── top level ──────────────────────────────────────────────────
    if j.get("schema").and_then(Json::as_str) != Some(RESULTS_SCHEMA) {
        errs.push(format!("schema: must be {RESULTS_SCHEMA}"));
    }
    match j.get("packet_disposition").and_then(Json::as_str) {
        Some(s) if s.starts_with("withheld{") || s.starts_with("refused{") => {}
        other => errs.push(format!(
            "packet_disposition: a typed withheld spelling required — got {other:?}"
        )),
    }
    for (obj, path) in [(j, "")] {
        if obj.get("claim").and_then(Json::as_str).is_none() {
            errs.push(format!("{path}claim: required"));
        }
    }
    if let Some(t) = j.get("tranche") {
        if t.get("id").and_then(Json::as_str).is_none() {
            errs.push("tranche.id: required".into());
        }
    } else {
        errs.push("tranche: required".into());
    }
    match j.get("family") {
        Some(f) => {
            if f.get("method").and_then(Json::as_str) != Some("holm") {
                errs.push("family.method: must be \"holm\"".into());
            }
            if f.get("alpha_ppm").and_then(Json::as_int).is_none() {
                errs.push("family.alpha_ppm: required".into());
            }
            match f.get("members") {
                Some(Json::Arr(ms)) if !ms.is_empty() => {
                    for (i, m) in ms.iter().enumerate() {
                        let p = format!("family.members[{i}]");
                        for k in ["member", "status"] {
                            if m.get(k).and_then(Json::as_str).is_none() {
                                errs.push(format!("{p}.{k}: required"));
                            }
                        }
                        if m.get("status").and_then(Json::as_str) == Some("unavailable")
                            && m.get("disposition").and_then(Json::as_str).is_none()
                        {
                            errs.push(format!(
                                "{p}.disposition: an unavailable member retains its typed reason"
                            ));
                        }
                    }
                }
                _ => errs
                    .push("family.members: non-empty set required (unavailable retained)".into()),
            }
        }
        None => errs.push("family: required".into()),
    }
    match j.get("exploratory_family") {
        Some(e) => {
            if e.get("method").and_then(Json::as_str) != Some("benjamini_hochberg") {
                errs.push("exploratory_family.method: must be \"benjamini_hochberg\"".into());
            }
            match e.get("members") {
                Some(Json::Arr(_)) => {}
                _ => errs.push("exploratory_family.members: array required".into()),
            }
        }
        None => errs.push("exploratory_family: required".into()),
    }
    if j.get("metric").and_then(Json::as_str) != Some(PRIMARY_METRIC) {
        errs.push(format!("metric: must be {PRIMARY_METRIC}"));
    }
    match j.get("populations") {
        Some(Json::Arr(ps)) => {
            let have: Vec<&str> = ps.iter().filter_map(Json::as_str).collect();
            for w in REQUIRED_POPULATIONS {
                if !have.contains(w) {
                    errs.push(format!("populations: missing {w}"));
                }
            }
        }
        _ => errs.push("populations: array required".into()),
    }
    match j.get("labels") {
        Some(labels) => {
            for (k, subs) in REQUIRED_LABELS {
                match labels.get(k).and_then(Json::as_str) {
                    Some(s) => {
                        for sub in *subs {
                            if !s.contains(sub) {
                                errs.push(format!("labels.{k}: must carry \"{sub}\""));
                            }
                        }
                    }
                    None => errs.push(format!("labels.{k}: mandatory label absent")),
                }
            }
        }
        None => errs.push("labels: required".into()),
    }
    match j.get("harnesses") {
        Some(Json::Obj(hs)) if hs.len() >= 2 => {
            for (arm, h) in hs {
                let p = format!("harnesses.{arm}");
                if h.get("disposition").and_then(Json::as_str).is_none() {
                    errs.push(format!(
                        "{p}.disposition: the arm's typed disposition required"
                    ));
                }
                match h.get("theta") {
                    Some(th) => {
                        for k in ["ppm", "lo_ppm", "hi_ppm"] {
                            match th.get(k) {
                                Some(v) => leaf_int(v, &format!("{p}.theta.{k}"), &mut errs),
                                None => errs.push(format!("{p}.theta.{k}: required")),
                            }
                        }
                        for k in ["interval", "bounds"] {
                            match th.get(k) {
                                Some(v) => leaf_interval(v, &format!("{p}.theta.{k}"), &mut errs),
                                None => errs.push(format!("{p}.theta.{k}: required")),
                            }
                        }
                        if th.get("label").and_then(Json::as_str).is_none() {
                            errs.push(format!("{p}.theta.label: required"));
                        }
                    }
                    None => errs.push(format!("{p}.theta: required")),
                }
                match h.get("secondary_populations") {
                    Some(sp) => {
                        for pop in ["raw", "errors_zero"] {
                            match sp.get(pop) {
                                Some(v) => {
                                    if v.get("label").and_then(Json::as_str).is_none() {
                                        errs.push(format!(
                                            "{p}.secondary_populations.{pop}.label: required"
                                        ));
                                    }
                                }
                                None => {
                                    errs.push(format!("{p}.secondary_populations.{pop}: required"))
                                }
                            }
                        }
                    }
                    None => errs.push(format!("{p}.secondary_populations: required")),
                }
                match h.get("delta_vs_ref") {
                    Some(d) => {
                        if !is_typed_marker(d) {
                            leaf_int(
                                d.get("ppm").unwrap_or(&Json::Null),
                                &format!("{p}.delta_vs_ref.ppm"),
                                &mut errs,
                            );
                        }
                    }
                    None => errs.push(format!("{p}.delta_vs_ref: required")),
                }
                for k in [
                    "leader_group",
                    "pass_at",
                    "flips",
                    "cost",
                    "counts",
                    "completion",
                    "integrity",
                ] {
                    if h.get(k).is_none() {
                        errs.push(format!("{p}.{k}: required"));
                    }
                }
            }
        }
        _ => errs.push("harnesses: object of >= 2 rows required".into()),
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

// ── P3-L2 / the L1-on-L2 audit / the L3 report ───────────────────────

/// `l2_withheld_packet(gl1, admission_refusal)` — the `hh-fp-l2/1`
/// withheld form: the L2 machinery's packet skeleton (gl1 pass id,
/// H identity digest, typed unknowns, dimension map) plus the
/// admission/KL/conjunction leaves as typed markers under the verbatim
/// claim refusal. `served_n: 0`, `planned_n: 50`.
pub fn l2_withheld_packet(
    gl1_pass_id: &str,
    h_identity_digest: &str,
    corpus_digest: &str,
    expected_precision: &str,
    sources: Json,
) -> Json {
    let refusal = CLAIM_REFUSAL;
    let base = hharness_eval::l2_wire::l2_packet(Some(gl1_pass_id), h_identity_digest)
        .unwrap_or_else(|e| panic!("l2_packet skeleton: {e:?}"));
    Json::obj([
        ("schema", Json::str(L2_SCHEMA)),
        ("packet_disposition", Json::str(refusal)),
        ("claim", Json::str(hharness_eval::l2_wire::CLAIM_ID)),
        (
            "gl1",
            Json::obj([
                ("pass_id", Json::str(gl1_pass_id)),
                ("h_identity_digest", Json::str(h_identity_digest)),
                ("corpus_digest", Json::str(corpus_digest)),
                ("result", Json::str("pass")),
            ]),
        ),
        ("l2_packet", base),
        (
            "admission",
            Json::obj([
                ("state", Json::str("refused")),
                ("reason", Json::str(refusal)),
                ("kind", Json::str("parity_admission")),
                ("version", Json::str("parity-admission/1")),
                (
                    "expected",
                    Json::str(
                        "0/0 pair members observed — the protected pair truncated at \
                         k<5, R<5 under end-first-pass-early; admission is an IUT \
                         member and truncates with it",
                    ),
                ),
            ]),
        ),
        (
            "kl_report",
            Json::obj([
                ("state", Json::str("refused")),
                ("reason", Json::str(refusal)),
                ("epsilon_ppm", Json::Int(L2_EPSILON_PPM)),
                ("planned_n", Json::Int(L2_PLANNED_N)),
                ("served_n", Json::Int(0)),
                ("rule", Json::str("L_Δ > −ε ∧ U_Δ < ε")),
                (
                    "expected_precision",
                    Json::str(
                        "the preregistration's pre-stated read at N=50, ε=±.20 is \
                         `inconclusive` — an admissible result; under the recorded \
                         truncation the inference itself stays withheld, never \
                         synthesized",
                    ),
                ),
            ]),
        ),
        (
            "conjunction",
            Json::obj([
                ("l1_gate_pass", Json::Bool(true)),
                ("admission", refused(refusal)),
                ("iut_conjunction_equivalent", refused(refusal)),
                ("vetoes_empty", refused(refusal)),
            ]),
        ),
        (
            "veto_layer",
            Json::obj([
                ("tripped", Json::Arr(Vec::new())),
                (
                    "not_checked",
                    Json::Arr(vec![Json::obj([
                        ("veto_id", Json::str("l2_pairs")),
                        ("status", Json::str("not_checked")),
                        (
                            "reason",
                            Json::str(
                                "no W4 pair executed — the window is unused{scope_end}; \
                                 the veto layer over zero pairs is uninformative, not clean",
                            ),
                        ),
                    ])]),
                ),
            ]),
        ),
        ("verdict", Json::str(refusal)),
        ("epsilon_ppm", Json::Int(L2_EPSILON_PPM)),
        ("planned_n", Json::Int(L2_PLANNED_N)),
        ("served_n", Json::Int(0)),
        ("typed_unknowns", typed_unknowns()),
        ("dimension_map", dimension_map_record(L2_EPSILON_PPM)),
        (
            "label",
            Json::str(format!(
                "first-pass L2 packet — {refusal}; {LABEL_FIRST_PASS}"
            )),
        ),
        ("expected_precision", Json::str(expected_precision)),
        (
            "seal_open_ref",
            Json::str("docs/build/reports/2026-11-19-fp-seal-open.json"),
        ),
        ("sources", sources),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str("R6.48")),
    ])
}

/// The audit refusal — a sample member outside the W4-O population.
#[derive(Debug)]
pub enum AuditError {
    /// A W2-corpus-class recording (or any non-W4-O id) reached the
    /// sample — REQ-XD-119's refuse rule.
    NotW4O {
        /// The refused id.
        id: String,
    },
}

/// `l2_audit_packet(sample, w4_ids, w2_ids)` — the L1-on-L2 audit.
/// The sample population is W4 O recordings only: a `w2_ids` member in
/// the sample refuses (the W2-corpus guard, REQ-XD-119); a member in
/// neither set refuses as undeclared. With zero W4 O recordings the
/// packet emits the typed refused form with the guard leg recorded.
pub fn l2_audit_packet(
    sample: &[String],
    w4_o_ids: &BTreeSet<String>,
    w2_corpus_ids: &BTreeSet<String>,
    seed: u64,
    sources: Json,
) -> Result<Json, AuditError> {
    for id in sample {
        if w2_corpus_ids.contains(id) {
            return Err(AuditError::NotW4O { id: id.clone() });
        }
        if !w4_o_ids.contains(id) {
            return Err(AuditError::NotW4O { id: id.clone() });
        }
    }
    let verdict = if sample.is_empty() {
        CLAIM_REFUSAL
    } else {
        "executed"
    };
    Ok(Json::obj([
        ("schema", Json::str(L2_AUDIT_SCHEMA)),
        ("audit_of", Json::str("P3-L2")),
        ("req", Json::str("REQ-XD-119")),
        ("seed", Json::Int(seed as i64)),
        (
            "population",
            Json::str("w4_o_recordings_only — every other recording class refuses"),
        ),
        (
            "planned_sample",
            Json::str("10% of the 50 planned pairs → 5 O-side recordings"),
        ),
        ("sampled_n", Json::Int(sample.len() as i64)),
        (
            "sampled",
            Json::Arr(sample.iter().map(Json::str).collect()),
        ),
        (
            "guard",
            Json::obj([
                (
                    "rule",
                    Json::str(
                        "a W2-corpus recording (or any non-W4-O id) in the sample refuses at decode",
                    ),
                ),
                ("result", Json::str(if sample.is_empty() {
                    "no sample — nothing admitted or refused"
                } else {
                    "sample all-W4-O"
                })),
            ]),
        ),
        (
            "audit",
            if sample.is_empty() {
                refused(CLAIM_REFUSAL)
            } else {
                Json::str("executed")
            },
        ),
        ("verdict", Json::str(verdict)),
        (
            "label",
            Json::str(
                "the audit is machinery evidence over executed W4 O recordings; \
                 under end-first-pass-early the population is empty and the audit \
                 withholds — never samples a W2 corpus recording",
            ),
        ),
        ("sources", sources),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str("R6.48")),
    ]))
}

/// `l3_report(per_pair)` — the REQ-XD-120 L3 packet: per-arm and
/// per-pair distributions, every discordant pair listed, and no
/// equivalence language. With zero served pairs every distributional
/// leaf is a typed marker and `discordant_pairs` is the empty list.
pub fn l3_report(sources: Json) -> Json {
    let per_arm = Json::obj([
        (
            "o",
            Json::obj([
                ("served", Json::Int(0)),
                ("steps", refused(CLAIM_REFUSAL)),
                ("tool_calls", refused(CLAIM_REFUSAL)),
                ("errors", refused(CLAIM_REFUSAL)),
                ("wall_ms", refused(CLAIM_REFUSAL)),
                (
                    "label",
                    Json::str("descriptive — per-arm distribution over the O side"),
                ),
            ]),
        ),
        (
            "h",
            Json::obj([
                ("served", Json::Int(0)),
                ("steps", refused(CLAIM_REFUSAL)),
                ("tool_calls", refused(CLAIM_REFUSAL)),
                ("errors", refused(CLAIM_REFUSAL)),
                ("wall_ms", refused(CLAIM_REFUSAL)),
                (
                    "label",
                    Json::str("descriptive — per-arm distribution over the H side"),
                ),
            ]),
        ),
    ]);
    Json::obj([
        ("schema", Json::str(L3_SCHEMA)),
        ("of", Json::str("P3-L2")),
        ("req", Json::str("REQ-XD-120")),
        (
            "population",
            Json::str("all 50 planned pairs; uncertain pairs as a separate class"),
        ),
        ("planned_pairs", Json::Int(L2_PLANNED_N)),
        ("served_pairs", Json::Int(0)),
        ("per_arm", per_arm),
        (
            "per_pair",
            Json::obj([
                ("diffs", refused(CLAIM_REFUSAL)),
                ("uncertain_class", Json::Arr(Vec::new())),
                (
                    "label",
                    Json::str("descriptive — the pair-difference distribution"),
                ),
            ]),
        ),
        (
            "discordant_pairs",
            Json::obj([
                ("listed", Json::Arr(Vec::new())),
                ("count", Json::Int(0)),
                (
                    "note",
                    Json::str(
                        "every discordant pair is listed under REQ-XD-120 — the \
                         served set is empty, so none exist; the list is never \
                         summarized away",
                    ),
                ),
            ]),
        ),
        ("verdict", Json::str(CLAIM_REFUSAL)),
        (
            "label",
            Json::str(
                "L3 descriptive report — distributions and discordance only; the \
                 schema carries no equivalence wording and none is asserted",
            ),
        ),
        ("sources", sources),
        ("recorded_at", Json::str(RECORDED_AT)),
        ("ticket", Json::str("R6.48")),
    ])
}

// ── golden fixtures (AC-7) ───────────────────────────────────────────

/// The four disposition branches the golden set covers.
pub const GOLDEN_BRANCHES: [&str; 4] = [
    "run{full}",
    "fallback",
    "end_after_w2",
    "re-scope{end-first-pass-early}",
];

fn fixture_arm(key: &str, participant: &str, disposition: &str) -> WithheldArm {
    WithheldArm {
        key: key.into(),
        participant: participant.into(),
        intended_participant: participant.replace("hh/", ""),
        disposition: disposition.into(),
        planned: 50,
        measured_cost_micros: None,
        swap_refs: vec!["hh/mini-swe-agent@2.4.6".into()],
    }
}

fn fixture_sources() -> Json {
    Json::Arr(vec![Json::obj([
        (
            "path",
            Json::str("fixture — declared values, never a result"),
        ),
        ("sha256", Json::str("sha256:fixture")),
    ])])
}

/// `golden_packet_set(branch)` — the AC-7 fixture sets:
/// - `run{full}` → computed-form packets over declared fixture values
///   (validated by `validate_results_packet`; `golden_fixture: true`
///   marks them as never-a-result).
/// - `fallback` → P2-FP computed; P3-L2/audit/L3/P4-FP
///   `withheld{l1_not_passed}`.
/// - `end_after_w2` → every claim withheld by the hold's typed reason
///   (`withheld{admission:pp0_infra_hold}` stands for the held-mini
///   branch).
/// - `re-scope{end-first-pass-early}` → the real emitted set's shape.
pub fn golden_packet_set(branch: &str) -> Result<Vec<Json>, String> {
    match branch {
        "run{full}" => Ok(vec![golden_computed_packet()]),
        "fallback" => Ok(vec![
            golden_computed_packet(),
            withheld_results_packet(
                "P4-FP",
                "hh_native",
                &[
                    fixture_arm("v5", "hh/v5@x", "withheld{l1_not_passed}"),
                    fixture_arm("hh_native", "hh/hh_native@x", "withheld{l1_not_passed}"),
                ],
                &[FamilyMember {
                    member: "v5".into(),
                    status: "unavailable".into(),
                    disposition: Some("withheld{l1_not_passed}".into()),
                    computed: None,
                }],
                &[FamilyMember {
                    member: "v5".into(),
                    status: "unavailable".into(),
                    disposition: Some("withheld{l1_not_passed}".into()),
                    computed: None,
                }],
                "fixture — declared",
                fixture_sources(),
            ),
        ]),
        "end_after_w2" => Ok(vec![withheld_results_packet(
            "P2-FP",
            "mini-swe-agent",
            &[
                fixture_arm("pi", "hh/pi@0.52.3", "withheld{admission:pp0_infra_hold}"),
                fixture_arm(
                    "claude-code",
                    "hh/claude-code@2.1.50",
                    "withheld{admission:pp0_infra_hold}",
                ),
                fixture_arm(
                    "mini-swe-agent",
                    "hh/mini-swe-agent@2.4.6",
                    "withheld{admission:pp0_infra_hold}",
                ),
            ],
            &[
                FamilyMember {
                    member: "pi".into(),
                    status: "unavailable".into(),
                    disposition: Some("withheld{admission:pp0_infra_hold}".into()),
                    computed: None,
                },
                FamilyMember {
                    member: "claude-code".into(),
                    status: "unavailable".into(),
                    disposition: Some("withheld{admission:pp0_infra_hold}".into()),
                    computed: None,
                },
            ],
            &[FamilyMember {
                member: "pi|claude-code".into(),
                status: "unavailable".into(),
                disposition: Some("withheld{admission:pp0_infra_hold}".into()),
                computed: None,
            }],
            "fixture — declared",
            fixture_sources(),
        )]),
        "re-scope{end-first-pass-early}" => Ok(real_packets(fixture_sources())),
        other => Err(format!("golden branch {other:?} is not declared")),
    }
}

/// A computed-form golden packet over declared fixture values — the
/// `run{full}` shape leg. `golden_fixture: true` marks it as a
/// structure fixture that is never a result.
pub fn golden_computed_packet() -> Json {
    Json::obj([
        ("schema", Json::str(RESULTS_SCHEMA)),
        ("golden_fixture", Json::Bool(true)),
        (
            "fixture_note",
            Json::str("declared fixture values — validates the computed shape, never a result"),
        ),
        ("claim", Json::str("P2-FP")),
        ("tranche", Json::obj([("id", Json::str("fp-w4"))])),
        (
            "family",
            Json::obj([
                ("id", Json::str("P2-FP")),
                ("method", Json::str("holm")),
                ("alpha_ppm", Json::Int(50_000)),
                ("reference", Json::str("mini-swe-agent")),
                (
                    "members",
                    Json::Arr(vec![
                        Json::obj([
                            ("member", Json::str("fixture-arm")),
                            ("status", Json::str("available")),
                            ("raw_p_ppm", Json::Int(400_000)),
                            ("adjusted_ppm", Json::Int(400_000)),
                        ]),
                        Json::obj([
                            ("member", Json::str("fixture-arm-b")),
                            ("status", Json::str("available")),
                            ("raw_p_ppm", Json::Int(600_000)),
                            ("adjusted_ppm", Json::Int(1_000_000)),
                        ]),
                    ]),
                ),
            ]),
        ),
        (
            "exploratory_family",
            Json::obj([
                ("id", Json::str("P2-FP-expl")),
                ("method", Json::str("benjamini_hochberg")),
                ("members", Json::Arr(Vec::new())),
            ]),
        ),
        ("metric", Json::str(PRIMARY_METRIC)),
        (
            "populations",
            Json::Arr(REQUIRED_POPULATIONS.iter().map(|s| Json::str(*s)).collect()),
        ),
        (
            "labels",
            obj_dyn(
                REQUIRED_LABELS
                    .iter()
                    .map(|(k, subs)| (k.to_string(), Json::str(subs.join("; ")))),
            ),
        ),
        (
            "harnesses",
            Json::obj([
                ("fixture-arm", golden_computed_arm(false)),
                ("hh/mini-swe-agent@2.4.6", golden_computed_arm(true)),
            ]),
        ),
    ])
}

fn golden_computed_arm(is_ref: bool) -> Json {
    let interval = Json::obj([
        ("lo_ppm", Json::Int(100_000)),
        ("hi_ppm", Json::Int(900_000)),
        ("kind", Json::str("fm")),
    ]);
    Json::obj([
        (
            "theta",
            Json::obj([
                ("ppm", Json::Int(500_000)),
                ("lo_ppm", Json::Int(100_000)),
                ("hi_ppm", Json::Int(900_000)),
                ("interval", interval.clone()),
                ("bounds", interval.clone()),
                ("label", Json::str("finite-manifest")),
            ]),
        ),
        (
            "secondary_populations",
            Json::obj([
                (
                    "raw",
                    Json::obj([
                        ("rate_ppm", Json::Int(500_000)),
                        ("label", Json::str(LABEL_SECONDARY_POP)),
                    ]),
                ),
                (
                    "errors_zero",
                    Json::obj([
                        ("bound_lo_ppm", Json::Int(0)),
                        ("bound_hi_ppm", Json::Int(1_000_000)),
                        ("label", Json::str(LABEL_SECONDARY_POP)),
                    ]),
                ),
            ]),
        ),
        (
            "delta_vs_ref",
            if is_ref {
                Json::obj([
                    ("ppm", Json::Int(0)),
                    ("interval", interval.clone()),
                    ("p_raw_ppm", Json::Int(1_000_000)),
                    ("p_holm_ppm", Json::Int(1_000_000)),
                    ("label", Json::str("reference")),
                ])
            } else {
                Json::obj([
                    ("ppm", Json::Int(0)),
                    ("interval", interval.clone()),
                    ("p_raw_ppm", Json::Int(400_000)),
                    ("p_holm_ppm", Json::Int(400_000)),
                    ("label", Json::str("confirmatory")),
                ])
            },
        ),
        (
            "leader_group",
            Json::obj([
                ("member", Json::Bool(true)),
                ("leader_id", Json::str("hh/mini-swe-agent@2.4.6")),
                ("diff_interval", interval.clone()),
                ("label", Json::str(LABEL_EXPLORATORY_LEADER)),
            ]),
        ),
        (
            "pass_at",
            Json::obj([
                ("k", Json::Int(5)),
                ("all_ppm", Json::Int(100_000)),
                ("any_ppm", Json::Int(900_000)),
            ]),
        ),
        (
            "flips",
            Json::obj([
                (
                    "rerun",
                    Json::obj([("ppm", Json::Int(10_000)), ("interval", interval.clone())]),
                ),
                (
                    "swap_vs",
                    Json::obj([(
                        "hh/mini-swe-agent@2.4.6",
                        Json::obj([("interval", interval.clone())]),
                    )]),
                ),
                (
                    "harness_excess",
                    Json::obj([(
                        "hh/mini-swe-agent@2.4.6",
                        Json::obj([("ppm", Json::Int(0)), ("interval", interval.clone())]),
                    )]),
                ),
            ]),
        ),
        (
            "cost",
            Json::obj([
                (
                    "per_trial",
                    Json::obj([
                        ("micros", Json::Int(100_000)),
                        ("interval", interval.clone()),
                    ]),
                ),
                (
                    "per_solve",
                    Json::obj([("micros", Json::Int(200_000)), ("interval", interval)]),
                ),
                ("provenance", Json::str("model_spend_only")),
            ]),
        ),
        (
            "counts",
            Json::obj([
                ("analysis_basis", Json::str("assignment")),
                ("intended", Json::obj([("v1.0", Json::Int(50))])),
                ("served", Json::obj([("hh/x@v1.0", Json::Int(50))])),
            ]),
        ),
        (
            "completion",
            Json::obj([
                ("planned", Json::Int(50)),
                ("first", Json::Int(50)),
                ("recovered", Json::Int(0)),
                ("unresolved", Json::Int(0)),
            ]),
        ),
        (
            "integrity",
            Json::obj([
                ("flagged", Json::Int(0)),
                ("confirmed", Json::Int(0)),
                ("cleared", Json::Int(0)),
                ("coverage_ppm", Json::Int(1_000_000)),
                ("detector", Json::str(ANSWER_SCAN_VERSION)),
                ("label", Json::str("integrity")),
            ]),
        ),
    ])
}

/// The real `re-scope{end-first-pass-early}` packet set (the emitted
/// records' builders, driven by the verbatim FP-E dispositions).
pub fn real_packets(sources: Json) -> Vec<Json> {
    vec![
        withheld_results_packet(
            "P2-FP",
            "mini-swe-agent",
            &p2_arms(),
            &p2_family(),
            &p2_exploratory(),
            "below the declared precision target — first pass, near-zero power pre-stated",
            sources.clone(),
        ),
        withheld_results_packet(
            "P4-FP",
            "hh_native",
            &p4_arms(),
            &p4_family(),
            &p4_exploratory(),
            "single contrast — descriptive precision only",
            sources,
        ),
    ]
}

/// The P2-FP arm set under the verbatim FP-E dispositions.
pub fn p2_arms() -> Vec<WithheldArm> {
    vec![
        WithheldArm {
            key: "pi".into(),
            participant: "hh/pi@0.52.3".into(),
            intended_participant: "pi@0.52.3".into(),
            disposition: "withheld{admission:AG3}".into(),
            planned: 50,
            measured_cost_micros: Some(123_000),
            swap_refs: vec!["hh/mini-swe-agent@2.4.6".into()],
        },
        WithheldArm {
            key: "claude-code".into(),
            participant: "hh/claude-code@2.1.50".into(),
            intended_participant: "claude-code@2.1.50".into(),
            disposition: "withheld{admission:AG3}".into(),
            planned: 50,
            measured_cost_micros: Some(210_000),
            swap_refs: vec!["hh/mini-swe-agent@2.4.6".into()],
        },
        WithheldArm {
            key: "mini-swe-agent".into(),
            participant: "hh/mini-swe-agent@2.4.6".into(),
            intended_participant: "mini-swe-agent@2.4.6".into(),
            disposition: "withheld{window_deadline}".into(),
            planned: 50,
            measured_cost_micros: Some(386_163),
            swap_refs: vec!["hh/pi@0.52.3".into(), "hh/claude-code@2.1.50".into()],
        },
    ]
}

/// The P2-FP Holm family — both contrasts retained as unavailable
/// members with their typed reasons (REQ-XD-030).
pub fn p2_family() -> Vec<FamilyMember> {
    vec![
        FamilyMember {
            member: "pi".into(),
            status: "unavailable".into(),
            disposition: Some("withheld{admission:AG3}".into()),
            computed: None,
        },
        FamilyMember {
            member: "claude-code".into(),
            status: "unavailable".into(),
            disposition: Some("withheld{admission:AG3}".into()),
            computed: None,
        },
    ]
}

/// The P2-FP exploratory family.
pub fn p2_exploratory() -> Vec<FamilyMember> {
    vec![FamilyMember {
        member: "pi|claude-code".into(),
        status: "unavailable".into(),
        disposition: Some(CLAIM_REFUSAL.into()),
        computed: None,
    }]
}

/// The P4-FP arm set.
pub fn p4_arms() -> Vec<WithheldArm> {
    vec![
        WithheldArm {
            key: "v5".into(),
            participant: "hh/v5@fp".into(),
            intended_participant: "v5@fp".into(),
            disposition: "withheld{budget}".into(),
            planned: 50,
            measured_cost_micros: None,
            swap_refs: vec!["hh/hh_native@fp".into()],
        },
        WithheldArm {
            key: "hh_native".into(),
            participant: "hh/hh_native@fp".into(),
            intended_participant: "hh_native@fp".into(),
            disposition: "withheld{window_deadline}".into(),
            planned: 50,
            measured_cost_micros: Some(374_000),
            swap_refs: vec!["hh/v5@fp".into()],
        },
    ]
}

/// The P4-FP family (a family of one — v5 retained unavailable).
pub fn p4_family() -> Vec<FamilyMember> {
    vec![FamilyMember {
        member: "v5".into(),
        status: "unavailable".into(),
        disposition: Some("withheld{budget}".into()),
        computed: None,
    }]
}

/// The P4-FP exploratory family.
pub fn p4_exploratory() -> Vec<FamilyMember> {
    vec![FamilyMember {
        member: "v5".into(),
        status: "unavailable".into(),
        disposition: Some("withheld{budget}".into()),
        computed: None,
    }]
}
