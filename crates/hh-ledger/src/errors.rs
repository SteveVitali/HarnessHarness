//! The typed refusal set (§5a.1 §2/§6 — "every violation is a typed validation or append
//! error, never a warning"). Variants are named for the spec's error column where the spec
//! names them; the few additions (`KernelOriginRequired`, `UnresolvedEventRef`,
//! `BlobCorrupt`, `AttestationInvalid`, `RunFinished`, `Io`) are documented in ADR-0233.

use std::fmt;

use hh_provenance::{AuthorityClass, EndorsementError, PersistenceScope, ProvenanceError};

/// `Missing{reason}` (ADR-0068 R3): a blob/get failure class — never `Tampered`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingReason {
    /// The bytes were collected (or never landed — at Stage 1 there is no `gc` op, so an
    /// absent blob is reported under this reason).
    Gc,
    /// The bytes were redacted (the `redact` op lands at Stage 2; the reason exists so a
    /// later reader can distinguish `gc` from `redacted`).
    Redacted,
    /// The encryption/unpacking key is unavailable.
    KeyUnavailable,
}

impl MissingReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            MissingReason::Gc => "gc",
            MissingReason::Redacted => "redacted",
            MissingReason::KeyUnavailable => "key_unavailable",
        }
    }
}

/// The ledger error — every variant a typed refusal (§5a.1 §2 errors column).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerError {
    // ── open_run ─────────────────────────────────────────────────────────
    /// A required manifest field is absent/ill-typed, or a run-kind rule is violated.
    ManifestInvalid {
        /// What is wrong.
        detail: String,
    },
    /// The `configuration_id`/`configuration_version_id` coordinate could not be
    /// resolved (absent when `run_kind = agent`, or not a pinned identity id).
    ConfigurationUnresolvable {
        /// The offending field.
        field: &'static str,
        /// The offending value.
        value: String,
    },
    /// A manifest ref is unpinned — a mutable tag rather than an identity id
    /// (ADR-0137: the mutable image tag is the named case).
    UnresolvedRef {
        /// The offending field.
        field: &'static str,
        /// The offending value.
        value: String,
    },
    /// A `forked_from`/`continued_from`/`parent_run_id` link names a run this store
    /// does not hold.
    UnknownForkPoint {
        /// The referenced run.
        run_id: String,
    },
    /// The lineage link's `at_seq` is beyond the source run's head.
    SourceIncomplete {
        /// The source run.
        run_id: String,
        /// The requested anchor seq.
        at_seq: u64,
        /// The source run's head.
        head: u64,
    },
    /// The lineage link's `head_hash` does not equal the source run's hash at
    /// `at_seq`.
    ForkPointNotCoherent {
        /// The source run.
        run_id: String,
        /// The anchor seq.
        at_seq: u64,
    },

    // ── writer lease ─────────────────────────────────────────────────────
    /// `acquire_writer` while a live lease is held.
    WouldBlock {
        /// The current holder.
        active_holder: String,
    },
    /// The lease token is stale — generation mismatch, expired, or superseded.
    Fenced {
        /// The token's generation.
        lease_generation: u64,
        /// The current generation.
        current_generation: u64,
        /// Why the fence fired.
        detail: String,
    },

    // ── append ───────────────────────────────────────────────────────────
    /// A class/schema violation: unknown class, bad field shape, oversized inline
    /// payload, bad `ts`, an `ext` marker, post-`finished` append.
    SchemaViolation {
        /// What is wrong.
        detail: String,
    },
    /// `parent_event_id` is neither the root sentinel nor an event of this run
    /// (earlier seq or earlier in the same batch).
    UnknownParent {
        /// The dangling parent id.
        parent_event_id: String,
    },
    /// `event_id` already used in this run (or twice inside one batch).
    DuplicateEventId {
        /// The duplicated id.
        event_id: String,
    },
    /// A scope id that is not currently open (or was opened out of order).
    ScopeNotOpen {
        /// Which scope field (`turn_id`, `model_call_id`, `tool_call_id`,
        /// `effect_id`, `child_run_id`, `branch_id`).
        scope: &'static str,
        /// The offending id.
        id: String,
    },
    /// An `action.effect.*` event names an effect the fold does not know (§5a.2 —
    /// every effect id is minted by `action.effect.intended`).
    UnknownEffect {
        /// The offending effect id.
        effect_id: String,
    },
    /// An `action.effect.*` phase event that the §5a.2 lifecycle does not admit from
    /// the effect's current phase (ADR-0030 §1).
    BadEffectTransition {
        /// The effect.
        effect_id: String,
        /// The recorded phase.
        from: String,
        /// The attempted event class.
        to: String,
    },
    /// `observe`/`commit` on an `(effect_id, attempt_no)` that already has an
    /// `observed` record — exactly one per attempt (§5a.2 `observe` contract).
    AlreadyObserved {
        /// The effect.
        effect_id: String,
        /// The attempt.
        attempt_no: u64,
    },
    /// A helper/executor presented no valid `CommitToken` for a non-`read_only`
    /// effect — the write-ahead barrier (ADR-0100 I-1; AC-R-2.2.2-10).
    NotCommitted {
        /// The effect.
        effect_id: String,
    },
    /// `action.effect.committed` with no preceding
    /// `security.permission.decided{decision = allow}` for that `effect_id` and
    /// attempt cycle — complete mediation as an append rule (ADR-0052 D6;
    /// §5g.1 I-H7; Anderson "always invoked").
    Undecided {
        /// The effect.
        effect_id: String,
        /// The attempt the commit names.
        attempt_no: u64,
    },
    /// A second *final* gate decision (`decision ∈ {allow, deny}`) for one
    /// `(effect_id, attempt)` — "exactly one `decided` per attempt cycle"
    /// (ADR-0052 D6). Non-final rows (`ask`, `timed_out`, `cancelled`, and any
    /// request-closure spelling) never count toward the gate.
    DuplicateDecision {
        /// The effect.
        effect_id: String,
        /// The attempt cycle.
        attempt_no: u64,
    },
    /// The WAL write/flush failed — the batch is not visible; retry is safe.
    Durability {
        /// The io detail.
        detail: String,
    },
    /// A provenance-mandatory class carried no record.
    MissingProvenance {
        /// What lacked the record.
        what: String,
    },
    /// The claimed authority exceeds what `origin` + in-record evidence confers.
    AuthorityExceedsOrigin {
        /// The minted ceiling.
        ceiling: AuthorityClass,
        /// The claimed class.
        claimed: AuthorityClass,
    },
    /// Free text above `external` (R-TEXT).
    TextAboveExternal {
        /// The claimed class.
        authority: AuthorityClass,
    },
    /// `taint ≠ ∅` with `authority > external`.
    TaintedAboveExternal {
        /// The claimed class.
        authority: AuthorityClass,
    },
    /// An illegitimate `security.label.endorsed` (ADR-0035 §1): delegate endorser,
    /// endorser below target, basis not allowed for the content kind, no label
    /// increase, or a `pin` without a verified attestation.
    IllegitimateEndorsement {
        /// Why.
        detail: String,
    },
    /// The record's `scope` exceeds its authority's write ceiling (ADR-0035 §3).
    ScopeCeilingExceeded {
        /// The persistence scope written.
        scope: PersistenceScope,
        /// The record's authority.
        authority: AuthorityClass,
    },
    /// A non-kernel producer (or a `provenance.origin ≠ kernel`) on an audit-grade or
    /// kernel-origin class (ADR-0066 Rule P; §5a.1 §5 "origin = kernel on
    /// lifecycle/environment rows").
    AuditProducerInvalid {
        /// The class.
        class: String,
        /// The offending producer component_class.
        producer: String,
    },
    /// An audit-grade row's payload exceeds the content-free audit-fields budget
    /// (ADR-0066 Rule C; interim size cap until the per-class field whitelists land
    /// with the §05g audit catalogue — ADR-0234).
    AuditFieldsTooLarge {
        /// The class.
        class: String,
        /// The canonical payload size.
        bytes: usize,
        /// The declared cap.
        max: usize,
    },
    /// A lifecycle/environment row carrying `provenance.origin ≠ kernel`.
    KernelOriginRequired {
        /// The class.
        class: String,
    },
    /// A `causes[]`/`refs` event coordinate that does not resolve.
    UnresolvedEventRef {
        /// The referenced run.
        run_id: String,
        /// The referenced event.
        event_id: String,
    },
    /// An attestation that is not a verification record (`self_consistent` failed) or
    /// fails `verify_attestation` against the run's trust set.
    AttestationInvalid {
        /// Why.
        reason: String,
    },
    /// Append attempted on a run that has committed `lifecycle.run.finished`.
    RunFinished {
        /// The run.
        run_id: String,
    },

    // ── read / subscribe / project ───────────────────────────────────────
    /// The run is not held by this store.
    UnknownRun {
        /// The run id.
        run_id: String,
    },
    /// The cursor does not resolve against this run's durable prefix.
    UnknownCursor {
        /// Why.
        detail: String,
    },
    /// The view kind is not registered (Stage-1 set: `context_view`, `run_summary`).
    UnknownViewKind {
        /// The requested kind.
        kind: String,
    },
    /// The view kind is registered but folded by its owning crate —
    /// `Store::project` only projects the ledger's own kinds; `trace_view`,
    /// `cost_view` and `metric_view` fold in `hh-telemetry` over `read` output
    /// (ADR-0042 D2 — the owning crate owns the projection).
    OwnerProjected {
        /// The requested kind.
        kind: &'static str,
    },

    // ── blobs ────────────────────────────────────────────────────────────
    /// `put_blob` over the store's declared blob-size ceiling.
    TooLarge {
        /// The offered size.
        bytes: usize,
        /// The policy ceiling.
        max: usize,
    },
    /// `get_blob` found nothing at the address — never `Tampered` (ADR-0068 R3).
    Missing {
        /// The address.
        address: String,
        /// The reason class.
        reason: MissingReason,
    },
    /// Bytes at the address do not hash back to it — storage corruption, surfaced as
    /// a typed error rather than silently served (CC3).
    BlobCorrupt {
        /// The address.
        address: String,
    },

    // ── decode / schema gate ─────────────────────────────────────────────
    /// An envelope carrying `schema_version` newer than this build's maximum — the
    /// client-side refusal (AC-R-2.2.1-8).
    SchemaMismatch {
        /// The version found.
        found: u64,
        /// The maximum this build knows.
        known_max: u64,
    },
    /// A stored/envelope provenance record failed the canonical decode.
    ProvenanceDecode {
        /// The decode detail.
        detail: String,
    },
    /// `verify` found the durable prefix broken — the tamper report.
    Tampered(Tampered),
    /// A store-level io failure outside the append path.
    Io {
        /// The io detail.
        detail: String,
    },
}

impl From<ProvenanceError> for LedgerError {
    fn from(e: ProvenanceError) -> LedgerError {
        match e {
            ProvenanceError::MissingProvenance { what } => LedgerError::MissingProvenance { what },
            ProvenanceError::TaintedAboveExternal { authority } => {
                LedgerError::TaintedAboveExternal { authority }
            }
            ProvenanceError::AuthorityExceedsOrigin { ceiling, claimed } => {
                LedgerError::AuthorityExceedsOrigin { ceiling, claimed }
            }
            ProvenanceError::AttestationFailed { reason } => {
                LedgerError::AttestationInvalid { reason }
            }
            other => LedgerError::SchemaViolation {
                detail: format!("provenance record malformed: {other:?}"),
            },
        }
    }
}

impl From<EndorsementError> for LedgerError {
    /// Every endorsement refusal is an `IllegitimateEndorsement` at append (the spec's
    /// named error); the variant is preserved in `detail`.
    fn from(e: EndorsementError) -> LedgerError {
        LedgerError::IllegitimateEndorsement {
            detail: format!("{e:?}"),
        }
    }
}

impl From<hh_provenance::DecodeError> for LedgerError {
    fn from(e: hh_provenance::DecodeError) -> LedgerError {
        LedgerError::ProvenanceDecode { detail: e.detail }
    }
}

impl fmt::Display for LedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for LedgerError {}

/// `verify`'s tamper report — `Tampered{at_seq, kind}` (§5a.1 §6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tampered {
    /// The first seq at which the durable prefix fails verification.
    pub at_seq: u64,
    /// The tamper class.
    pub kind: TamperedKind,
}

impl fmt::Display for Tampered {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tampered at seq {}: {}", self.at_seq, self.kind.as_str())
    }
}

impl std::error::Error for Tampered {}

/// The tamper classes `verify` reports (§5a.1 §6 storage-tampering row: edit / delete /
/// reorder / truncate-within / late insert / prefix replacement all land here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TamperedKind {
    /// An event's stored bytes do not recompute to its `hash` — content was edited.
    ContentModified,
    /// `prev_hash` does not equal the previous committed event's `hash`.
    ChainBroken,
    /// A seq is missing inside the committed range — an event was deleted.
    SeqGap,
    /// A committed event's seq does not follow its predecessor in file order.
    Reordered,
    /// Two committed events share one `seq` — a late insert.
    DuplicateSeq,
    /// Two committed events share one `event_id`.
    DuplicateEventId,
    /// `parent_event_id` resolves to nothing committed.
    DanglingParent,
}

impl TamperedKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TamperedKind::ContentModified => "content_modified",
            TamperedKind::ChainBroken => "chain_broken",
            TamperedKind::SeqGap => "seq_gap",
            TamperedKind::Reordered => "reordered",
            TamperedKind::DuplicateSeq => "duplicate_seq",
            TamperedKind::DuplicateEventId => "duplicate_event_id",
            TamperedKind::DanglingParent => "dangling_parent",
        }
    }
}
