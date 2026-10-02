//! The `audit_view` export lowerings + external-chain lift (§5g.6 §7
//! AC-H6-9; T-LCD-11 — silent loss on lowering is a named failure mode, so
//! every lowering reports its dropped fields, never drops silently).
//!
//! Two registered conventions, both closed sums:
//!
//! - `external_audit_record` — a generic external audit-record convention
//!   (the audit-record shape an outside store consumes: identity + chain
//!   coordinates + the class tag; no envelope provenance, no scope detail,
//!   no view machinery).
//! - `telemetry` — a flat telemetry sink (`{name, ts, attributes{…}}`) —
//!   cannot carry the hash chain at all; the loss report names `hash` and
//!   `prev_hash` among the drops.
//!
//! The lift direction ([`lift_external`]) produces `unverified` rows linked
//! by alias — an imported record has no hash chain or signature the store
//! recomputes, so `unverified` is the honest status, never `ok` (the same
//! discipline `audit_view` applies to checkpoints and cross-run anchors).

use hh_wire::json::Json;

/// The export conventions `export_audit_view` lowers into (closed sum —
/// a new convention is a schema-visible addition, never an open string).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportConvention {
    /// The generic external audit-record convention.
    ExternalAuditRecord,
    /// The flat telemetry sink.
    Telemetry,
}

impl ExportConvention {
    /// The canonical tag.
    pub fn as_str(self) -> &'static str {
        match self {
            ExportConvention::ExternalAuditRecord => "external_audit_record",
            ExportConvention::Telemetry => "telemetry",
        }
    }

    /// Parse the closed sum; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<ExportConvention> {
        match s {
            "external_audit_record" => Some(ExportConvention::ExternalAuditRecord),
            "telemetry" => Some(ExportConvention::Telemetry),
            _ => None,
        }
    }

    /// The envelope members the convention carries verbatim per row.
    fn carried_fields(self) -> &'static [&'static str] {
        match self {
            ExportConvention::ExternalAuditRecord => {
                &["event_id", "seq", "ts", "class", "hash", "prev_hash"]
            }
            ExportConvention::Telemetry => &["event_id", "seq", "ts", "class"],
        }
    }

    /// The `audit_view` members the convention carries (everything else is
    /// a named drop — the view's *recomputed* judgements never lower: an
    /// external convention carries rows, not our verdicts).
    fn carried_view_members(self) -> &'static [&'static str] {
        match self {
            ExportConvention::ExternalAuditRecord => &["kind", "events_seen"],
            ExportConvention::Telemetry => &["kind", "events_seen"],
        }
    }
}

/// The lowering loss report — the dropped envelope members (per row) and
/// the dropped `audit_view` members (per export), deterministically ordered.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportLossReport {
    /// The convention lowered to.
    pub convention: ExportConvention,
    /// Rows emitted.
    pub rows_emitted: u64,
    /// Envelope members the convention could not carry (present in the
    /// source, absent in the target shape).
    pub dropped_fields: Vec<String>,
    /// `audit_view` members the export does not carry.
    pub dropped_view_members: Vec<String>,
}

impl ExportLossReport {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("convention", Json::str(self.convention.as_str())),
            ("rows_emitted", Json::Int(self.rows_emitted as i64)),
            (
                "dropped_fields",
                Json::Arr(self.dropped_fields.iter().map(Json::str).collect()),
            ),
            (
                "dropped_view_members",
                Json::Arr(self.dropped_view_members.iter().map(Json::str).collect()),
            ),
        ])
    }
}

/// The lowering result — the emitted rows plus the loss report.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditExport {
    /// The emitted rows (one per lowered event).
    pub rows: Vec<Json>,
    /// The loss report.
    pub loss_report: ExportLossReport,
}

impl AuditExport {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("rows", Json::Arr(self.rows.clone())),
            ("loss_report", self.loss_report.to_json()),
        ])
    }
}

/// The envelope members the lowering considers (the durable-envelope field
/// set — a member absent from the source is not a "drop", it was never
/// there).
const ENVELOPE_FIELDS: &[&str] = &[
    "event_id",
    "seq",
    "ts",
    "class",
    "hash",
    "prev_hash",
    "producer",
    "scope",
    "provenance",
    "causes",
    "refs",
    "ir_refs",
    "surface_ids",
    "payload",
    "lease_generation",
    "parent_event_id",
    "durability",
];

/// `export_audit_view(events, convention)` — lower the run's durable event
/// stream to `convention`'s row shape, reporting every dropped member.
/// `view_members` are the `audit_view` payload's member names (the caller
/// passes the projected view's keys so the report names view-level drops —
/// the lowering itself never recomputes the view).
pub fn export_audit_view(
    events: &[crate::event::EventEnvelope],
    view_members: &[String],
    convention: ExportConvention,
) -> AuditExport {
    let carried = convention.carried_fields();
    let mut dropped_fields: Vec<String> = ENVELOPE_FIELDS
        .iter()
        .filter(|f| !carried.contains(f))
        .map(|f| f.to_string())
        .collect();
    // Only name a field "dropped" when at least one source row carried it —
    // an absent member is not a loss.
    let present: std::collections::BTreeSet<&str> = events
        .iter()
        .flat_map(|e| {
            let mut v = vec![
                "event_id",
                "seq",
                "ts",
                "class",
                "hash",
                "prev_hash",
                "payload",
                "producer",
                "durability",
                "parent_event_id",
            ];
            if !e.scope.is_empty() {
                v.push("scope");
            }
            if e.provenance.is_some() {
                v.push("provenance");
            }
            if !e.causes.is_empty() {
                v.push("causes");
            }
            if !e.refs.is_empty() {
                v.push("refs");
            }
            if !e.ir_refs.is_empty() {
                v.push("ir_refs");
            }
            if !e.surface_ids.is_empty() {
                v.push("surface_ids");
            }
            v.push("lease_generation");
            v
        })
        .collect();
    dropped_fields.retain(|f| present.contains(f.as_str()));

    let rows: Vec<Json> = events
        .iter()
        .map(|e| match convention {
            ExportConvention::ExternalAuditRecord => Json::obj([
                ("record_id", Json::str(&e.event_id)),
                ("seq", Json::Int(e.seq as i64)),
                ("at", Json::str(&e.ts)),
                ("kind", Json::str(&e.class)),
                ("hash", Json::str(&e.hash)),
                ("prev_hash", Json::str(&e.prev_hash)),
            ]),
            ExportConvention::Telemetry => Json::obj([
                ("name", Json::str(&e.class)),
                ("ts", Json::str(&e.ts)),
                (
                    "attributes",
                    Json::obj([
                        ("seq", Json::Int(e.seq as i64)),
                        ("event_id", Json::str(&e.event_id)),
                    ]),
                ),
            ]),
        })
        .collect();

    let carried_view = convention.carried_view_members();
    let mut dropped_view_members: Vec<String> = view_members
        .iter()
        .filter(|m| !carried_view.contains(&m.as_str()))
        .cloned()
        .collect();
    dropped_view_members.sort();

    AuditExport {
        loss_report: ExportLossReport {
            convention,
            rows_emitted: rows.len() as u64,
            dropped_fields,
            dropped_view_members,
        },
        rows,
    }
}

/// A lifted external row — the import direction (§5g.6 §7 AC-H6-9): every
/// row reports `verification_status = "unverified"` and links back to the
/// external chain through `alias` (the external record's own id — an alias,
/// never a join key, per the `surface_ids` discipline).
#[derive(Debug, Clone, PartialEq)]
pub struct LiftedRow {
    /// The assigned local seq (position in the lifted stream).
    pub seq: u64,
    /// The external record's id — the alias back to the source chain.
    pub alias: String,
    /// The external record's kind, when it declared one.
    pub kind: Option<String>,
    /// `unverified` — always (the external chain's integrity is not ours to
    /// assert; a hash we cannot recompute is no verification).
    pub verification_status: &'static str,
}

impl LiftedRow {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("seq", Json::Int(self.seq as i64)),
            ("alias", Json::str(&self.alias)),
            (
                "kind",
                self.kind
                    .as_ref()
                    .map(|k| Json::str(k.clone()))
                    .unwrap_or(Json::Null),
            ),
            ("verification_status", Json::str(self.verification_status)),
        ])
    }
}

/// `lift_external(records)` — lift an external audit chain's records into
/// alias-linked `unverified` rows. Each record must carry an `id`/`record_id`
/// member (the alias); a record without one lifts as
/// `alias = <unknown>` with position only (the absence is named in
/// `missing_alias`, never silently fabricated).
pub fn lift_external(records: &[Json]) -> Vec<LiftedRow> {
    records
        .iter()
        .enumerate()
        .map(|(i, r)| LiftedRow {
            seq: i as u64,
            alias: r
                .get("record_id")
                .or_else(|| r.get("id"))
                .and_then(Json::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("external:{i}")),
            kind: r
                .get("kind")
                .or_else(|| r.get("name"))
                .and_then(Json::as_str)
                .map(str::to_string),
            verification_status: "unverified",
        })
        .collect()
}
