//! The event vocabulary (§5a.1 §3): the input `Event`, the durable `EventEnvelope`, the
//! `EphemeralRecord`, cursors/filters/pages, and the `subscribe` `EventFrame` sum.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_identity::idp::ContentAddress;
use hh_identity::rowkeys::IrRef;
use hh_ontology::planes::EventFamily;
use hh_provenance::{ContentKind, ProvenanceRecord};
use hh_wire::json::Json;

use crate::classes::Durability;
use crate::manifest::{EventRef, ObservabilityLevel, ParticipantClass};

/// `plane` — the §5a.1 envelope vocabulary `{observation, action, control,
/// verification, security, measurement, model, lifecycle}`; derived from the class
/// through `hh-ontology`'s prefix→family→home rule (CC10 — the mapping lives at the
/// ontology; this is its §5a.1 spelling).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventPlane {
    /// `context.*` (P1).
    Observation,
    /// `action.*` (P2).
    Action,
    /// `control.*` (P3).
    Control,
    /// `verification.*` (P4).
    Verification,
    /// `security.*` (P6).
    Security,
    /// `measurement.*` (P7).
    Measurement,
    /// `model.*` (the model boundary).
    Model,
    /// `lifecycle.*` (run lifecycle).
    Lifecycle,
}

impl EventPlane {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EventPlane::Observation => "observation",
            EventPlane::Action => "action",
            EventPlane::Control => "control",
            EventPlane::Verification => "verification",
            EventPlane::Security => "security",
            EventPlane::Measurement => "measurement",
            EventPlane::Model => "model",
            EventPlane::Lifecycle => "lifecycle",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<EventPlane> {
        Some(match s {
            "observation" => EventPlane::Observation,
            "action" => EventPlane::Action,
            "control" => EventPlane::Control,
            "verification" => EventPlane::Verification,
            "security" => EventPlane::Security,
            "measurement" => EventPlane::Measurement,
            "model" => EventPlane::Model,
            "lifecycle" => EventPlane::Lifecycle,
            _ => return None,
        })
    }

    /// The plane a class belongs to, by the identifier-prefix rule — `None` for an
    /// unregistered family prefix.
    pub fn of_class(class: &str) -> Option<EventPlane> {
        let prefix = class.split('.').next()?;
        EventFamily::from_prefix(prefix).map(|f| match f {
            EventFamily::Context => EventPlane::Observation,
            EventFamily::Action => EventPlane::Action,
            EventFamily::Control => EventPlane::Control,
            EventFamily::Verification => EventPlane::Verification,
            EventFamily::Security => EventPlane::Security,
            EventFamily::Measurement => EventPlane::Measurement,
            EventFamily::Model => EventPlane::Model,
            EventFamily::Lifecycle => EventPlane::Lifecycle,
        })
    }
}

/// `producer {component_class, component_variant_ref, participant_ref}` — which kernel
/// component (or declared participant component) emitted the row. For kernel-issued rows
/// (`lifecycle.*`, audit-grade classes) `component_class = "kernel"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Producer {
    /// The component class (`"kernel"` for kernel-authored rows).
    pub component_class: String,
    /// The component variant ref (content address / version id, or `none`).
    pub component_variant_ref: String,
    /// The participant ref.
    pub participant_ref: String,
}

/// The kernel producer — the component spelling for ledger-authored rows.
pub const KERNEL_COMPONENT: &str = "kernel";

impl Producer {
    /// A kernel row's producer (`component_class = "kernel"`).
    pub fn kernel(component: impl Into<String>) -> Producer {
        Producer {
            component_class: KERNEL_COMPONENT.into(),
            component_variant_ref: component.into(),
            participant_ref: "kernel".into(),
        }
    }
}

/// `scope {turn_id?, model_call_id?, tool_call_id?, effect_id?, child_run_id?,
/// branch_id?}` — the structural chain `run ⊃ turn ⊃ model_call ⊃ tool_call`
/// (ADR-0027 §4); scope ids must refer to opened scopes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    /// The open turn.
    pub turn_id: Option<String>,
    /// The open model call.
    pub model_call_id: Option<String>,
    /// The open tool call.
    pub tool_call_id: Option<String>,
    /// The open effect.
    pub effect_id: Option<String>,
    /// A child run (no Stage-1 opener — set only by Stage-4 spawns).
    pub child_run_id: Option<String>,
    /// A branch (no Stage-1 opener — `navigate` lands at Stage 2).
    pub branch_id: Option<String>,
}

impl Scope {
    /// The id carried for `kind`, if any.
    pub fn get(&self, kind: crate::classes::ScopeKind) -> Option<&str> {
        use crate::classes::ScopeKind::*;
        match kind {
            Turn => self.turn_id.as_deref(),
            ModelCall => self.model_call_id.as_deref(),
            ToolCall => self.tool_call_id.as_deref(),
            Effect => self.effect_id.as_deref(),
            ChildRun => self.child_run_id.as_deref(),
            Branch => self.branch_id.as_deref(),
        }
    }

    /// Whether every field is unset.
    pub fn is_empty(&self) -> bool {
        self.turn_id.is_none()
            && self.model_call_id.is_none()
            && self.tool_call_id.is_none()
            && self.effect_id.is_none()
            && self.child_run_id.is_none()
            && self.branch_id.is_none()
    }
}

/// The caller-supplied event — the ledger stamps `run_id`, `seq`, `plane`,
/// `schema_version`, `participant_class`, `observability_level`, `durability`,
/// `lease_generation`, `prev_hash` and `hash`.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Allocated, creation-time-sortable, unique within the run (validated at append).
    pub event_id: String,
    /// A registered class (the persistence-policy table is the registry).
    pub class: String,
    /// `ts` — RFC 3339 UTC, millisecond precision.
    pub ts: String,
    /// `hlc` — optional at C0; required when a class's row demands it (none at this
    /// stage).
    pub hlc: Option<String>,
    /// Who emitted the row.
    pub producer: Producer,
    /// The enclosing scopes.
    pub scope: Scope,
    /// `parent_event_id` — the branch-tree parent, or [`crate::ids::ROOT_EVENT`].
    pub parent_event_id: String,
    /// `causes[]` — cross-run causal links (EventRef, resolvable at append).
    pub causes: Vec<EventRef>,
    /// `refs[]` — offloaded content addresses (blobs written via `put_blob`).
    pub refs: Vec<ContentAddress>,
    /// `ir_refs[]` — `{semantic_id, version_id}` pairs (the rebuildable index).
    pub ir_refs: Vec<IrRef>,
    /// `surface_ids{}` — provider-native ids, aliases only, never join keys.
    pub surface_ids: BTreeMap<String, String>,
    /// The provenance record — mandatory per the class's declared rule.
    pub provenance: Option<ProvenanceRecord>,
    /// The payload's declared content kind — the R-TEXT check reads it (`Text`
    /// leaves declare `free_text`).
    pub content_kind: Option<ContentKind>,
    /// The class payload (canonical `Json`; declared `ephemeral_fields` are stripped
    /// for the durable form and delivered to subscribers).
    pub payload: Json,
}

/// The durable envelope — the hash-chained record (§5a.1 §3 field table).
#[derive(Debug, Clone, PartialEq)]
pub struct EventEnvelope {
    /// Allocated, creation-time-sortable, unique within the run.
    pub event_id: String,
    /// The run.
    pub run_id: String,
    /// Dense per-run sequence (seq 0 = `lifecycle.run.created`).
    pub seq: u64,
    /// `ts` — RFC 3339 UTC, ms.
    pub ts: String,
    /// `hlc` — optional at C0.
    pub hlc: Option<String>,
    /// Derived from `class` (prefix rule — CC10).
    pub plane: EventPlane,
    /// The event class.
    pub class: String,
    /// The envelope schema version (the evolution gate — AC-8).
    pub schema_version: u64,
    /// The emitting component.
    pub producer: Producer,
    /// `native` | `hosted` — stamped from the manifest.
    pub participant_class: ParticipantClass,
    /// The run's declared observability subset (`ledger` entailed by `native`).
    pub observability_level: BTreeSet<ObservabilityLevel>,
    /// The class's declared persistence (ephemeral records never reach this struct —
    /// an envelope is always `ledger`).
    pub durability: Durability,
    /// The structural scopes.
    pub scope: Scope,
    /// The fencing token — the writer lease's generation.
    pub lease_generation: u64,
    /// The branch-tree parent (or [`crate::ids::ROOT_EVENT`]).
    pub parent_event_id: String,
    /// Cross-run causal links.
    pub causes: Vec<EventRef>,
    /// Offloaded content addresses.
    pub refs: Vec<ContentAddress>,
    /// The `{semantic_id, version_id}` pairs.
    pub ir_refs: Vec<IrRef>,
    /// Provider-native aliases — never join keys.
    pub surface_ids: BTreeMap<String, String>,
    /// The provenance record (mandatory per class rule).
    pub provenance: Option<ProvenanceRecord>,
    /// The previous committed event's `hash` (or [`crate::ids::GENESIS_HASH`]/a lineage
    /// anchor at seq 0).
    pub prev_hash: String,
    /// The class payload (canonical; declared ephemeral members stripped).
    pub payload: Json,
    /// `H(leaf_tag ∥ canonical(envelope − {hash}) ∥ prev_hash)` — the `idp/1` digest
    /// under the `ledger.event` domain (ADR-0029 §2; the one hasher — CC1).
    pub hash: String,
}

/// An ephemeral record — delivered on `subscribe`, never durable, never in `read`
/// (ADR-0026 §3/§5). Carries the envelope's stamps minus `seq`/`prev_hash`/`hash`.
#[derive(Debug, Clone, PartialEq)]
pub struct EphemeralRecord {
    /// The event id (for a stripped-member fragment: the durable event's id).
    pub event_id: String,
    /// The run.
    pub run_id: String,
    /// `ts`.
    pub ts: String,
    /// `hlc` — optional.
    pub hlc: Option<String>,
    /// Derived plane.
    pub plane: EventPlane,
    /// The class.
    pub class: String,
    /// The schema version.
    pub schema_version: u64,
    /// The producer.
    pub producer: Producer,
    /// Stamps.
    pub participant_class: ParticipantClass,
    /// Stamps.
    pub observability_level: BTreeSet<ObservabilityLevel>,
    /// Scopes.
    pub scope: Scope,
    /// The writer generation.
    pub lease_generation: u64,
    /// Parent.
    pub parent_event_id: String,
    /// Causes.
    pub causes: Vec<EventRef>,
    /// Refs.
    pub refs: Vec<ContentAddress>,
    /// Ir refs.
    pub ir_refs: Vec<IrRef>,
    /// Aliases.
    pub surface_ids: BTreeMap<String, String>,
    /// Provenance.
    pub provenance: Option<ProvenanceRecord>,
    /// The ephemeral payload (for a fragment: `{"<field>": <value>}`).
    pub payload: Json,
    /// For a stripped-member fragment: the durable event's `seq` it belongs to.
    pub durable_event_seq: Option<u64>,
}

/// `head(run)` — `{seq, event_id, hash}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// The highest committed seq.
    pub seq: u64,
    /// The head event id.
    pub event_id: String,
    /// The head hash.
    pub hash: String,
}

/// `SeqRange` — `append`'s return (the committed `[first, last]` inclusive range).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeqRange {
    /// First committed seq.
    pub first: u64,
    /// Last committed seq.
    pub last: u64,
    /// Durable events committed (a pure-ephemeral batch commits none).
    pub count: u64,
}

/// The inclusive cursor (ADR-0026 §5; WS-B1 §6.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cursor {
    /// From `seq` (inclusive).
    Seq(u64),
    /// From `event_id` (inclusive — resolves through the rebuildable index).
    EventId(String),
    /// Live tail only (`subscribe` only — `read` refuses it as `UnknownCursor`).
    Now,
}

/// Read direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Ascending seq.
    Fwd,
    /// Descending seq — the reconstruction scan is O(tail).
    Rev,
}

/// `read`'s filter — `{plane, class, scope, ir_refs}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadFilter {
    /// The plane.
    pub plane: Option<EventPlane>,
    /// Exact class, or a `prefix.*` pattern.
    pub class: Option<String>,
    /// Every set field must equal the event's scope field.
    pub scope: Option<Scope>,
    /// The event's `ir_refs` must contain every requested ref.
    pub ir_refs: Option<Vec<IrRef>>,
}

/// `Page{events, next_cursor}` — `next_cursor` is `None` at the durable end.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// The durable events (filtered, ordered).
    pub events: Vec<EventEnvelope>,
    /// The cursor a follow-up `read`/`subscribe` resumes from (`seq + 1` of the last
    /// returned event, or `None` when the page reached the durable end).
    pub next_cursor: Option<Cursor>,
}

/// `subscribe`'s frame sum — `sync | durable | ephemeral | rewind | lagged | closed`
/// (§5a.1 §2).
#[derive(Debug, Clone, PartialEq)]
pub enum EventFrame {
    /// Replay caught up — live frames follow.
    Sync {
        /// The durable head the catch-up reached.
        at_seq: u64,
    },
    /// A durable event (post-commit).
    Durable {
        /// Its seq.
        seq: u64,
        /// The envelope.
        event: Box<EventEnvelope>,
        /// Its hash.
        hash: String,
    },
    /// An ephemeral record (or a stripped-member fragment of a durable row).
    Ephemeral {
        /// The record.
        event: Box<EphemeralRecord>,
    },
    /// A `head.moved` rewind — dormant at Stage 1 (lands with `navigate`, Stage 2).
    Rewind {
        /// The seq the head moved back to.
        to_seq: u64,
    },
    /// The subscription's bounded buffer overflowed — frames were dropped.
    Lagged {
        /// The seq the subscriber resumes missing-frames from.
        missed_from_seq: u64,
    },
    /// The run committed `lifecycle.run.finished` — the stream's terminal frame.
    Closed {
        /// Why.
        reason: CloseReason,
    },
}

/// Why a subscription closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// `lifecycle.run.finished` committed.
    RunFinished,
    /// The store was dropped.
    StoreClosed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plane_of_class_uses_the_ontology_prefix_rule() {
        assert_eq!(
            EventPlane::of_class("context.observation.recorded"),
            Some(EventPlane::Observation)
        );
        assert_eq!(
            EventPlane::of_class("model.call.requested"),
            Some(EventPlane::Model)
        );
        assert_eq!(
            EventPlane::of_class("lifecycle.run.created"),
            Some(EventPlane::Lifecycle)
        );
        assert_eq!(EventPlane::of_class("bogus.class"), None);
    }
}
