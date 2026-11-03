//! `evidence-package/1` — the additive, versioned typed member/role
//! profile inside `hh-bundle/1` (spec §6.1; PADR-R5-3∗ Packet C,
//! admitted GATE-G9): a **derived index** over the §6.1 member groups,
//! recomputable from the named authorities at the recorded watermark —
//! never a second authoritative store, never a stand-alone receipt
//! authority. Create-if-absent identities refuse conflicting bytes; a
//! regrade/correction/export retry mints a new snapshot — it never
//! rewrites the old one. `closure_walk`/`closure_report` (Packet C
//! items 3–4) and population assembly are later owners' surfaces —
//! not this module's.
//!
//! Requiredness is reviewable field by field (C.3): the R-group
//! members (`profile_version`, `source_versions`, `claims`,
//! `protocol_ref`, `phase`, `slot_set`, `attempts`, `grade_attempts`,
//! `exports`, `native_heads`, `watermarks`, `pins`, `access`) are
//! required *even when partial* — an empty list is allowed, an absent
//! member is `missing_required_member`. The C-group reference sets
//! (`oracle_observation_refs`, `treatment_joins`, `rows_analysis_refs`,
//! `operational_evidence_refs`) belong to claim classes this leg does
//! not exercise and decode as tolerated additive members.

use std::collections::{BTreeMap, BTreeSet};

use hharness_wire::json::Json;

use crate::error::BundleError;

/// The one profile schema id (PADR-R5-3∗ C.1 item 1).
pub const PROFILE_SCHEMA: &str = "evidence-package/1";
/// The admitted profile major — an unknown mandatory version refuses
/// new claims while legacy inspection is preserved (C.5/C.6).
pub const PROFILE_VERSION: u64 = 1;
/// The member role + manifest section the profile occupies.
pub const PROFILE_ROLE: &str = "evidence_profile";
/// The profile member's media type.
pub const PROFILE_MEDIA: &str = "application/vnd.hh.evidence-package+json";
/// The WAL-plane kinds a `native_heads[]` coordinate may carry — a
/// build-note is a labelled build-memory snapshot (§6.5), never a
/// native WAL head.
pub const WAL_HEAD_KINDS: [&str; 5] = ["subject", "experiment", "instrument", "overlay", "lineage"];
/// The one label a `legacy_refs[]` row may carry (C.3 O-group).
pub const BUILD_NOTE_LABEL: &str = "build-note";

/// The closed refusal-code set `BundleError::ProfileRefused.code`
/// carries — the §6.1/§6.5 refusal vocabulary.
pub mod code {
    /// Same identity, different bytes (C.5 create-if-absent).
    pub const CONFLICTING_IDENTITY: &str = "conflicting_identity";
    /// A required member is absent — empty lists are allowed, the
    /// member is not (§6.1 "required even when partial").
    pub const MISSING_REQUIRED: &str = "missing_required_member";
    /// A declared head or exported last event ≠ the recorded WAL
    /// coordinate.
    pub const HEAD_MISMATCH: &str = "head_mismatch";
    /// A declared coordinate names a run with no recorded head.
    pub const MISSING_HEAD: &str = "missing_head";
    /// A projection watermark runs ahead of its recorded head.
    pub const WATERMARK_MISMATCH: &str = "watermark_mismatch";
    /// A grade retry/supersession that would add subject n (I-03).
    pub const GRADE_RETRY_ADDS_N: &str = "grade_retry_adds_n";
    /// A build-note record standing in a WAL coordinate slot (§6.5).
    pub const BUILD_NOTE_AS_WAL: &str = "build_note_as_wal";
    /// A mandatory profile version this codec does not know (C.6).
    pub const UNKNOWN_PROFILE_VERSION: &str = "unknown_profile_version";
    /// A record that does not decode.
    pub const MALFORMED: &str = "profile_malformed";
}

fn refused(code: &'static str, detail: impl Into<String>) -> BundleError {
    BundleError::ProfileRefused {
        code,
        detail: detail.into(),
    }
}

/// `check()`'s finding — one closed-code violation.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileViolation {
    /// The closed code.
    pub code: &'static str,
    /// The offending member/identity.
    pub detail: String,
}

impl ProfileViolation {
    /// The operation-level refusal form.
    pub fn to_error(&self) -> BundleError {
        refused(self.code, self.detail.clone())
    }
}

// ── decode helpers — required means *present*; empty values allowed ──

fn member<'a>(j: &'a Json, k: &str, ctx: &str) -> Result<&'a Json, BundleError> {
    j.get(k)
        .ok_or_else(|| refused(code::MISSING_REQUIRED, format!("{ctx}.{k}")))
}

fn str_member(j: &Json, k: &str, ctx: &str) -> Result<String, BundleError> {
    member(j, k, ctx)?
        .as_str()
        .map(String::from)
        .ok_or_else(|| refused(code::MALFORMED, format!("{ctx}.{k} not a string")))
}

fn opt_str_member(j: &Json, k: &str, ctx: &str) -> Result<Option<String>, BundleError> {
    match member(j, k, ctx)? {
        Json::Null => Ok(None),
        Json::Str(s) => Ok(Some(s.clone())),
        _ => Err(refused(
            code::MALFORMED,
            format!("{ctx}.{k} not a string|null"),
        )),
    }
}

fn int_member(j: &Json, k: &str, ctx: &str) -> Result<u64, BundleError> {
    member(j, k, ctx)?
        .as_int()
        .map(|i| i as u64)
        .ok_or_else(|| refused(code::MALFORMED, format!("{ctx}.{k} not an int")))
}

fn arr_member<'a>(j: &'a Json, k: &str, ctx: &str) -> Result<&'a Vec<Json>, BundleError> {
    match member(j, k, ctx)? {
        Json::Arr(a) => Ok(a),
        _ => Err(refused(code::MALFORMED, format!("{ctx}.{k} not an array"))),
    }
}

fn bool_member(j: &Json, k: &str, ctx: &str) -> Result<bool, BundleError> {
    match member(j, k, ctx)? {
        Json::Bool(b) => Ok(*b),
        _ => Err(refused(code::MALFORMED, format!("{ctx}.{k} not a bool"))),
    }
}

fn str_list(j: &Json, k: &str, ctx: &str) -> Result<Vec<String>, BundleError> {
    Ok(arr_member(j, k, ctx)?
        .iter()
        .filter_map(|s| s.as_str().map(String::from))
        .collect())
}

/// `{seq, event_id, hash}` — one WAL coordinate (§6.1 "native
/// heads/watermarks" — the same triple `SubjectSection.heads` and
/// `LedgerExport.head` carry).
#[derive(Debug, Clone, PartialEq)]
pub struct Coord {
    /// The committed seq.
    pub seq: u64,
    /// The head event id.
    pub event_id: String,
    /// The head hash.
    pub hash: String,
}

impl Coord {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("seq", Json::Int(self.seq as i64)),
            ("event_id", Json::str(self.event_id.clone())),
            ("hash", Json::str(self.hash.clone())),
        ])
    }
    /// Decode (required members).
    pub fn from_json(j: &Json, ctx: &str) -> Result<Coord, BundleError> {
        Ok(Coord {
            seq: int_member(j, "seq", ctx)?,
            event_id: str_member(j, "event_id", ctx)?,
            hash: str_member(j, "hash", ctx)?,
        })
    }
}

/// `native_heads[]` — `{run_id, kind, seq, event_id, hash}` for every
/// included subject/experiment/instrument/overlay/lineage prefix
/// (§6.1; C.1 item 2). Later appends never mutate a minted coordinate.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeHead {
    /// The run the head binds.
    pub run_id: String,
    /// The WAL-plane kind (`WAL_HEAD_KINDS` only — never a build-note).
    pub kind: String,
    /// The coordinate.
    pub coord: Coord,
}

impl NativeHead {
    /// Canonical JSON (flat — the dossier's `{run_id, seq, event_id,
    /// hash}` spelling plus `kind`).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("run_id", Json::str(self.run_id.clone())),
            ("kind", Json::str(self.kind.clone())),
            ("seq", Json::Int(self.coord.seq as i64)),
            ("event_id", Json::str(self.coord.event_id.clone())),
            ("hash", Json::str(self.coord.hash.clone())),
        ])
    }
    /// Decode (required members; the kind legality is `check()`'s —
    /// decode preserves the record for inspection).
    pub fn from_json(j: &Json, ctx: &str) -> Result<NativeHead, BundleError> {
        Ok(NativeHead {
            run_id: str_member(j, "run_id", ctx)?,
            kind: str_member(j, "kind", ctx)?,
            coord: Coord::from_json(j, ctx)?,
        })
    }
}

/// `watermarks[]` — `{run_id, projection, seq}` — the projection
/// watermark the snapshot was minted at.
#[derive(Debug, Clone, PartialEq)]
pub struct Watermark {
    /// The run the projection folds.
    pub run_id: String,
    /// The projection name.
    pub projection: String,
    /// The folded-through seq.
    pub seq: u64,
}

impl Watermark {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("run_id", Json::str(self.run_id.clone())),
            ("projection", Json::str(self.projection.clone())),
            ("seq", Json::Int(self.seq as i64)),
        ])
    }
    /// Decode (required members).
    pub fn from_json(j: &Json, ctx: &str) -> Result<Watermark, BundleError> {
        Ok(Watermark {
            run_id: str_member(j, "run_id", ctx)?,
            projection: str_member(j, "projection", ctx)?,
            seq: int_member(j, "seq", ctx)?,
        })
    }
}

/// `attempts[]` — the native plan/attempt/launch binding (§6.1
/// "Attempts"; C.1 item 2). Identity is the native binding's, never a
/// date/leg filename or provider id; a prelaunch refusal carries no
/// fictitious subject (`subject_run`/`launch_binding` null).
#[derive(Debug, Clone, PartialEq)]
pub struct AttemptRef {
    /// The native attempt identity.
    pub attempt_id: String,
    /// The plan this attempt binds.
    pub plan_id: String,
    /// The subject run when launched — `None` on a prelaunch refusal.
    pub subject_run: Option<String>,
    /// The `launch_bound` record ref — `None` on a prelaunch refusal.
    pub launch_binding: Option<String>,
    /// The attempt ordinal (1-based).
    pub ordinal: u64,
    /// The predecessor attempt (a retry names it) — `None` on the first.
    pub predecessor: Option<String>,
    /// The launch/retry reason.
    pub reason: String,
    /// The settlement ref (`run_settled`/`execution_settled`) — `None`
    /// while unsettled.
    pub settlement: Option<String>,
}

impl AttemptRef {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("attempt_id", Json::str(self.attempt_id.clone())),
            ("plan_id", Json::str(self.plan_id.clone())),
            (
                "subject_run",
                self.subject_run
                    .clone()
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
            (
                "launch_binding",
                self.launch_binding
                    .clone()
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
            ("ordinal", Json::Int(self.ordinal as i64)),
            (
                "predecessor",
                self.predecessor
                    .clone()
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
            ("reason", Json::str(self.reason.clone())),
            (
                "settlement",
                self.settlement.clone().map(Json::str).unwrap_or(Json::Null),
            ),
        ])
    }
    /// Decode (required members — nullable members must still be
    /// present).
    pub fn from_json(j: &Json, ctx: &str) -> Result<AttemptRef, BundleError> {
        Ok(AttemptRef {
            attempt_id: str_member(j, "attempt_id", ctx)?,
            plan_id: str_member(j, "plan_id", ctx)?,
            subject_run: opt_str_member(j, "subject_run", ctx)?,
            launch_binding: opt_str_member(j, "launch_binding", ctx)?,
            ordinal: int_member(j, "ordinal", ctx)?,
            predecessor: opt_str_member(j, "predecessor", ctx)?,
            reason: str_member(j, "reason", ctx)?,
            settlement: opt_str_member(j, "settlement", ctx)?,
        })
    }
}

/// `grade_attempts[]` — the instrument grade attempt: sealed
/// subject/submission digests, oracle/version, ordinals, the frozen
/// selection and supersession/revision refs (§6.1 "Grade
/// attempts/selections"). `counts_subject` is the first-subject-n
/// declaration — a retry can never carry it (I-03 "no … n inflation").
#[derive(Debug, Clone, PartialEq)]
pub struct GradeAttemptRef {
    /// The native grade-attempt identity.
    pub grade_attempt_id: String,
    /// The attempt this grading binds.
    pub attempt_id: String,
    /// The instrument event/run ref.
    pub instrument_event: Option<String>,
    /// The sealed subject digest.
    pub sealed_subject_digest: Option<String>,
    /// The sealed submission digest.
    pub sealed_submission_digest: Option<String>,
    /// The oracle ref.
    pub oracle: Option<String>,
    /// The oracle version.
    pub oracle_version: Option<String>,
    /// The grade ordinal (1-based; >1 names a retry).
    pub ordinal: u64,
    /// The frozen selection ref.
    pub selection: Option<String>,
    /// The supersession/revision refs this grade names.
    pub supersedes: Vec<String>,
    /// Whether this grade contributes subject n — only the first
    /// counting grade of an `attempt_id` may.
    pub counts_subject: bool,
}

impl GradeAttemptRef {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let opt = |o: &Option<String>| o.clone().map(Json::str).unwrap_or(Json::Null);
        Json::obj([
            ("grade_attempt_id", Json::str(self.grade_attempt_id.clone())),
            ("attempt_id", Json::str(self.attempt_id.clone())),
            ("instrument_event", opt(&self.instrument_event)),
            ("sealed_subject_digest", opt(&self.sealed_subject_digest)),
            (
                "sealed_submission_digest",
                opt(&self.sealed_submission_digest),
            ),
            ("oracle", opt(&self.oracle)),
            ("oracle_version", opt(&self.oracle_version)),
            ("ordinal", Json::Int(self.ordinal as i64)),
            ("selection", opt(&self.selection)),
            (
                "supersedes",
                Json::Arr(self.supersedes.iter().map(Json::str).collect()),
            ),
            ("counts_subject", Json::Bool(self.counts_subject)),
        ])
    }
    /// Decode (required members).
    pub fn from_json(j: &Json, ctx: &str) -> Result<GradeAttemptRef, BundleError> {
        Ok(GradeAttemptRef {
            grade_attempt_id: str_member(j, "grade_attempt_id", ctx)?,
            attempt_id: str_member(j, "attempt_id", ctx)?,
            instrument_event: opt_str_member(j, "instrument_event", ctx)?,
            sealed_subject_digest: opt_str_member(j, "sealed_subject_digest", ctx)?,
            sealed_submission_digest: opt_str_member(j, "sealed_submission_digest", ctx)?,
            oracle: opt_str_member(j, "oracle", ctx)?,
            oracle_version: opt_str_member(j, "oracle_version", ctx)?,
            ordinal: int_member(j, "ordinal", ctx)?,
            selection: opt_str_member(j, "selection", ctx)?,
            supersedes: str_list(j, "supersedes", ctx)?,
            counts_subject: bool_member(j, "counts_subject", ctx)?,
        })
    }
}

/// `exports[]` — an export snapshot coordinate: `{export_id, run_id,
/// target, last_event{seq,event_id,hash}, watermark}`. The exported
/// last event agrees with the recorded head (§6.1).
#[derive(Debug, Clone, PartialEq)]
pub struct ExportRef {
    /// The native export identity.
    pub export_id: String,
    /// The exported run.
    pub run_id: String,
    /// The export target/sink.
    pub target: String,
    /// The exported prefix's last event — equal to the recorded head.
    pub last_event: Coord,
    /// The export watermark.
    pub watermark: u64,
}

impl ExportRef {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("export_id", Json::str(self.export_id.clone())),
            ("run_id", Json::str(self.run_id.clone())),
            ("target", Json::str(self.target.clone())),
            ("last_event", self.last_event.to_json()),
            ("watermark", Json::Int(self.watermark as i64)),
        ])
    }
    /// Decode (required members).
    pub fn from_json(j: &Json, ctx: &str) -> Result<ExportRef, BundleError> {
        Ok(ExportRef {
            export_id: str_member(j, "export_id", ctx)?,
            run_id: str_member(j, "run_id", ctx)?,
            target: str_member(j, "target", ctx)?,
            last_event: Coord::from_json(member(j, "last_event", ctx)?, "exports[].last_event")?,
            watermark: int_member(j, "watermark", ctx)?,
        })
    }
}

/// `EvidenceProfile` — the `evidence-package/1` record (C.3's field
/// map). The metadata groups are required-present Json preserved
/// verbatim; the identity/coordinate records are typed.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceProfile {
    /// The profile major — `PROFILE_VERSION` only; anything else is an
    /// `unknown_profile_version` refusal at decode/admit.
    pub profile_version: u64,
    /// `source_versions{code_commit, dirty_patch_member|dirty_unknown,
    /// runtime, toolchain, features}` — required even when partial.
    pub source_versions: Json,
    /// `claims[]{id, text, scope, class, population, native_refs,
    /// report_refs}` — required, may be `[]`.
    pub claims: Vec<Json>,
    /// `protocol_ref` — required, may be `null`.
    pub protocol_ref: Json,
    /// `phase` — required, may be `""`.
    pub phase: String,
    /// `slot_set{plans, hash, count, close_reason, watermark}` —
    /// required even when partial.
    pub slot_set: Json,
    /// `attempts[]` — create-if-absent identities.
    pub attempts: Vec<AttemptRef>,
    /// `grade_attempts[]` — create-if-absent identities.
    pub grade_attempts: Vec<GradeAttemptRef>,
    /// `exports[]` — create-if-absent identities.
    pub exports: Vec<ExportRef>,
    /// `native_heads[]` — the immutable coordinates.
    pub native_heads: Vec<NativeHead>,
    /// `watermarks[]` — the projection watermarks.
    pub watermarks: Vec<Watermark>,
    /// `pins{task, models, treatment, usage}` — required even when
    /// partial.
    pub pins: Json,
    /// `access{supersedes, losses, policy, rights, recipient_class,
    /// retention, owner}` — required; empty lists allowed.
    pub access: Json,
    /// `legacy_refs[]` (O) — `{label:"build-note", member, revision?,
    /// digest}` rows; exact bytes labelled build-note, never a native
    /// head (§6.5).
    pub legacy_refs: Vec<Json>,
}

impl EvidenceProfile {
    /// Canonical JSON — every R member always emitted (`[]`/`null`
    /// included); `legacy_refs` only when non-empty (O-group).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(PROFILE_SCHEMA)),
            ("profile_version", Json::Int(self.profile_version as i64)),
            ("source_versions", self.source_versions.clone()),
            ("claims", Json::Arr(self.claims.to_vec())),
            ("protocol_ref", self.protocol_ref.clone()),
            ("phase", Json::str(self.phase.clone())),
            ("slot_set", self.slot_set.clone()),
            (
                "attempts",
                Json::Arr(self.attempts.iter().map(|a| a.to_json()).collect()),
            ),
            (
                "grade_attempts",
                Json::Arr(self.grade_attempts.iter().map(|g| g.to_json()).collect()),
            ),
            (
                "exports",
                Json::Arr(self.exports.iter().map(|e| e.to_json()).collect()),
            ),
            (
                "native_heads",
                Json::Arr(self.native_heads.iter().map(|h| h.to_json()).collect()),
            ),
            (
                "watermarks",
                Json::Arr(self.watermarks.iter().map(|w| w.to_json()).collect()),
            ),
            ("pins", self.pins.clone()),
            ("access", self.access.clone()),
            ("legacy_refs", Json::Arr(self.legacy_refs.to_vec())),
        ])
    }

    /// Decode — strict on required members (absent is
    /// `missing_required_member` even when the legal value is empty),
    /// tolerant of unknown members (additive decode — optional
    /// additions never strengthen old evidence). An
    /// `evidence-package/<n>` family record with a version/major this
    /// codec does not admit refuses `unknown_profile_version`; a
    /// record outside the family refuses `profile_malformed`.
    pub fn from_json(j: &Json) -> Result<EvidenceProfile, BundleError> {
        const CTX: &str = "evidence-package/1";
        match j.get("schema").and_then(Json::as_str) {
            Some(PROFILE_SCHEMA) => {}
            Some(other) if other.starts_with("evidence-package/") => {
                return Err(refused(code::UNKNOWN_PROFILE_VERSION, other.to_string()));
            }
            _ => {
                return Err(refused(
                    code::MALFORMED,
                    "schema is not evidence-package/<n>",
                ));
            }
        }
        let profile_version = int_member(j, "profile_version", CTX)?;
        if profile_version != PROFILE_VERSION {
            return Err(refused(
                code::UNKNOWN_PROFILE_VERSION,
                format!("profile_version {profile_version}"),
            ));
        }
        // R-group presence — the Json is preserved verbatim so a later
        // owner decodes the same bytes.
        let source_versions = member(j, "source_versions", CTX)?;
        for k in [
            "code_commit",
            "dirty_patch_member",
            "dirty_unknown",
            "runtime",
            "toolchain",
            "features",
        ] {
            member(source_versions, k, "source_versions")?;
        }
        let claims = arr_member(j, "claims", CTX)?.clone();
        for c in &claims {
            for k in [
                "id",
                "text",
                "scope",
                "class",
                "population",
                "native_refs",
                "report_refs",
            ] {
                member(c, k, "claims[]")?;
            }
        }
        let protocol_ref = member(j, "protocol_ref", CTX)?.clone();
        let phase = str_member(j, "phase", CTX)?;
        let slot_set = member(j, "slot_set", CTX)?;
        for k in ["plans", "hash", "count", "close_reason", "watermark"] {
            member(slot_set, k, "slot_set")?;
        }
        let pins = member(j, "pins", CTX)?;
        for k in ["task", "models", "treatment", "usage"] {
            member(pins, k, "pins")?;
        }
        let access = member(j, "access", CTX)?;
        for k in [
            "supersedes",
            "losses",
            "policy",
            "rights",
            "recipient_class",
            "retention",
            "owner",
        ] {
            member(access, k, "access")?;
        }
        let legacy_refs = match j.get("legacy_refs") {
            Some(Json::Arr(ls)) => ls.clone(),
            _ => Vec::new(),
        };
        Ok(EvidenceProfile {
            profile_version,
            source_versions: source_versions.clone(),
            claims,
            protocol_ref,
            phase,
            slot_set: slot_set.clone(),
            attempts: arr_member(j, "attempts", CTX)?
                .iter()
                .map(|a| AttemptRef::from_json(a, "attempts[]"))
                .collect::<Result<_, _>>()?,
            grade_attempts: arr_member(j, "grade_attempts", CTX)?
                .iter()
                .map(|g| GradeAttemptRef::from_json(g, "grade_attempts[]"))
                .collect::<Result<_, _>>()?,
            exports: arr_member(j, "exports", CTX)?
                .iter()
                .map(|e| ExportRef::from_json(e, "exports[]"))
                .collect::<Result<_, _>>()?,
            native_heads: arr_member(j, "native_heads", CTX)?
                .iter()
                .map(|h| NativeHead::from_json(h, "native_heads[]"))
                .collect::<Result<_, _>>()?,
            watermarks: arr_member(j, "watermarks", CTX)?
                .iter()
                .map(|w| Watermark::from_json(w, "watermarks[]"))
                .collect::<Result<_, _>>()?,
            pins: pins.clone(),
            access: access.clone(),
            legacy_refs,
        })
    }

    /// The profile's own closed checks — the §6.1/§6.5 refusal
    /// semantics a minted or decoded record must satisfy. Pure over the
    /// record; bundle-level agreement (subject heads, export heads,
    /// watermarks) is `validate`'s S2 block.
    pub fn check(&self) -> Vec<ProfileViolation> {
        let mut out = Vec::new();
        let fail = |out: &mut Vec<ProfileViolation>, code: &'static str, d: String| {
            out.push(ProfileViolation { code, detail: d });
        };
        // Create-if-absent uniqueness — one identity owns one record.
        // A repeated id inside the record is the same-ID surface; the
        // differing-bytes case reaches `ProfileIndex` at mint time.
        for (family, ids) in [
            (
                "attempts",
                self.attempts
                    .iter()
                    .map(|a| a.attempt_id.clone())
                    .collect::<Vec<_>>(),
            ),
            (
                "grade_attempts",
                self.grade_attempts
                    .iter()
                    .map(|g| g.grade_attempt_id.clone())
                    .collect::<Vec<_>>(),
            ),
            (
                "exports",
                self.exports
                    .iter()
                    .map(|e| e.export_id.clone())
                    .collect::<Vec<_>>(),
            ),
        ] {
            let mut seen = BTreeSet::new();
            for id in ids {
                if !seen.insert(id.clone()) {
                    fail(
                        &mut out,
                        code::CONFLICTING_IDENTITY,
                        format!("{family}:{id}"),
                    );
                }
            }
        }
        // A grade retry never adds n: at most one counting grade per
        // attempt_id, and a retry (ordinal>1 or superseding) never
        // counts (I-03 "no … n inflation").
        let mut counting = BTreeSet::new();
        for g in &self.grade_attempts {
            if g.counts_subject
                && (g.ordinal > 1
                    || !g.supersedes.is_empty()
                    || !counting.insert(g.attempt_id.clone()))
            {
                fail(
                    &mut out,
                    code::GRADE_RETRY_ADDS_N,
                    format!("grade_attempts:{}", g.grade_attempt_id),
                );
            }
        }
        // Heads carry WAL-plane kinds only — a build-note is a labelled
        // snapshot, never a WAL coordinate (§6.5).
        for h in &self.native_heads {
            if h.kind == BUILD_NOTE_LABEL {
                fail(
                    &mut out,
                    code::BUILD_NOTE_AS_WAL,
                    format!("native_heads:{}", h.run_id),
                );
            } else if !WAL_HEAD_KINDS.contains(&h.kind.as_str()) {
                fail(
                    &mut out,
                    code::MALFORMED,
                    format!("native_heads kind {} is not a WAL plane", h.kind),
                );
            }
        }
        let head = |run: &str| self.native_heads.iter().find(|h| h.run_id == run);
        // The exported last event agrees with the recorded head.
        for e in &self.exports {
            match head(&e.run_id) {
                None => fail(
                    &mut out,
                    code::MISSING_HEAD,
                    format!("exports:{}", e.run_id),
                ),
                Some(h) if h.coord != e.last_event => fail(
                    &mut out,
                    code::HEAD_MISMATCH,
                    format!(
                        "exports:{} last_event ({},{}) ≠ recorded head ({},{})",
                        e.run_id, e.last_event.seq, e.last_event.hash, h.coord.seq, h.coord.hash
                    ),
                ),
                _ => {}
            }
        }
        // A projection watermark binds a recorded head — it never runs
        // ahead of it and never names a run with no coordinate.
        for w in &self.watermarks {
            match head(&w.run_id) {
                None => fail(
                    &mut out,
                    code::MISSING_HEAD,
                    format!("watermarks:{}", w.run_id),
                ),
                Some(h) if w.seq > h.coord.seq => {
                    fail(
                        &mut out,
                        code::WATERMARK_MISMATCH,
                        format!(
                            "watermarks:{} seq {} > head {}",
                            w.run_id, w.seq, h.coord.seq
                        ),
                    );
                }
                _ => {}
            }
        }
        // Every subject a record names has its coordinate.
        for a in &self.attempts {
            if let Some(run) = &a.subject_run {
                if head(run).is_none() {
                    fail(&mut out, code::MISSING_HEAD, format!("attempts:{run}"));
                }
            }
        }
        // Legacy rows are labelled build-note — never native heads.
        for l in &self.legacy_refs {
            if l.get("label").and_then(Json::as_str) != Some(BUILD_NOTE_LABEL) {
                fail(
                    &mut out,
                    code::MALFORMED,
                    "legacy_refs[].label is not build-note".to_string(),
                );
            }
        }
        out
    }
}

/// `create_if_absent`'s outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityOutcome {
    /// The identity was absent — minted.
    Created,
    /// The identity existed with byte-identical content — the existing
    /// record stands (a same-day re-mint retains one record).
    Retained,
}

/// The create-if-absent identity map (C.5): minting an already-present
/// id against byte-identical content is idempotent; an id re-minted
/// against different bytes refuses `conflicting_identity` — never an
/// overwrite, never a second record. One map serves the
/// attempts/grades/exports families — their identities are disjoint
/// value spaces and the byte test is the conflict oracle either way.
#[derive(Debug, Default)]
pub struct ProfileIndex {
    records: BTreeMap<String, String>,
}

impl ProfileIndex {
    /// `create-if-absent(id, record)` — the record's canonical bytes are
    /// the conflict test.
    pub fn create_if_absent(
        &mut self,
        id: &str,
        record: &Json,
    ) -> Result<IdentityOutcome, BundleError> {
        let bytes = record.to_canonical_string();
        match self.records.get(id) {
            None => {
                self.records.insert(id.to_string(), bytes);
                Ok(IdentityOutcome::Created)
            }
            Some(existing) if *existing == bytes => Ok(IdentityOutcome::Retained),
            _ => Err(refused(
                code::CONFLICTING_IDENTITY,
                format!("identity {id} re-minted against different bytes"),
            )),
        }
    }
    /// Mint an attempt identity.
    pub fn create_attempt(&mut self, a: &AttemptRef) -> Result<IdentityOutcome, BundleError> {
        self.create_if_absent(&a.attempt_id, &a.to_json())
    }
    /// Mint a grade-attempt identity.
    pub fn create_grade_attempt(
        &mut self,
        g: &GradeAttemptRef,
    ) -> Result<IdentityOutcome, BundleError> {
        self.create_if_absent(&g.grade_attempt_id, &g.to_json())
    }
    /// Mint an export identity.
    pub fn create_export(&mut self, e: &ExportRef) -> Result<IdentityOutcome, BundleError> {
        self.create_if_absent(&e.export_id, &e.to_json())
    }
    /// The recorded identities.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.records.keys().map(String::as_str)
    }
}

/// `evidence-package/<n>` — the version a member's bytes declare, for
/// legacy inspection (unknown-version reporting preserves the label;
/// it never admits the record).
pub fn peek_version(j: &Json) -> Option<u64> {
    match j.get("schema").and_then(Json::as_str) {
        Some(s) if s.starts_with("evidence-package/") => j
            .get("profile_version")
            .and_then(Json::as_int)
            .map(|i| i as u64)
            .or_else(|| s.rsplit('/').next()?.parse().ok()),
        _ => None,
    }
}
