//! The **mandatory-provenance ledger table** (§8.1 #7): every listed event class must carry a
//! full [`crate::record::ProvenanceRecord`] of its author — and some classes additionally carry named
//! provenance fields (the endorsed `subject_ref`, the permission `operand`/`envelope`, the
//! handoff endpoints). `MissingProvenance` is the typed refusal when a compliant append omits
//! the record (§8.1 #5; never a warning — ADR-0033 D7).
//!
//! The table is a data fact later stages consult at append/stamping time (the kernel stamps
//! every ledger event); Stage 1 lands the table and the required-field shape, Stage 2/C0 wires
//! the runtime append path to it.

/// A ledger event class that must carry provenance (§8.1 #7 — the mandatory table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProvenanceEventKind {
    /// `security.permission.proposed`.
    PermissionProposed,
    /// `security.permission.granted`.
    PermissionGranted,
    /// `security.permission.checked`.
    PermissionChecked,
    /// `security.permission.consumed`.
    PermissionConsumed,
    /// `memory.handoff.sent`.
    HandoffSent,
    /// `memory.handoff.received`.
    HandoffReceived,
    /// `security.label.endorsed`.
    LabelEndorsed,
    /// `security.label.declassified`.
    LabelDeclassified,
    /// `security.label.applied`.
    LabelApplied,
    /// `memory.procedure.promoted`.
    ProcedurePromoted,
    /// `interop.shim.call`.
    ShimCall,
    /// `interop.install.*`.
    Install,
    /// `security.snapshot.*`.
    SecuritySnapshot,
    /// `security.declassify`.
    SecurityDeclassify,
    /// `security.override`.
    SecurityOverride,
    /// `memory.audit`.
    MemoryAudit,
    /// `memory.policy.*`.
    MemoryPolicy,
    /// `interop.wasm.module.trusted`.
    WasmModuleTrusted,
    /// `interop.market.imported`.
    MarketImported,
    /// `interop.market.pinned`.
    MarketPinned,
    /// `eval.maturity.grade.recorded`.
    MaturityGradeRecorded,
    /// `promotion.canary_started`.
    PromotionCanaryStarted,
    /// `memory.rollup.recorded`.
    RollupRecorded,
    /// `budget.fold`.
    BudgetFold,
}

impl ProvenanceEventKind {
    /// The canonical event spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ProvenanceEventKind::PermissionProposed => "security.permission.proposed",
            ProvenanceEventKind::PermissionGranted => "security.permission.granted",
            ProvenanceEventKind::PermissionChecked => "security.permission.checked",
            ProvenanceEventKind::PermissionConsumed => "security.permission.consumed",
            ProvenanceEventKind::HandoffSent => "memory.handoff.sent",
            ProvenanceEventKind::HandoffReceived => "memory.handoff.received",
            ProvenanceEventKind::LabelEndorsed => "security.label.endorsed",
            ProvenanceEventKind::LabelDeclassified => "security.label.declassified",
            ProvenanceEventKind::LabelApplied => "security.label.applied",
            ProvenanceEventKind::ProcedurePromoted => "memory.procedure.promoted",
            ProvenanceEventKind::ShimCall => "interop.shim.call",
            ProvenanceEventKind::Install => "interop.install.*",
            ProvenanceEventKind::SecuritySnapshot => "security.snapshot.*",
            ProvenanceEventKind::SecurityDeclassify => "security.declassify",
            ProvenanceEventKind::SecurityOverride => "security.override",
            ProvenanceEventKind::MemoryAudit => "memory.audit",
            ProvenanceEventKind::MemoryPolicy => "memory.policy.*",
            ProvenanceEventKind::WasmModuleTrusted => "interop.wasm.module.trusted",
            ProvenanceEventKind::MarketImported => "interop.market.imported",
            ProvenanceEventKind::MarketPinned => "interop.market.pinned",
            ProvenanceEventKind::MaturityGradeRecorded => "eval.maturity.grade.recorded",
            ProvenanceEventKind::PromotionCanaryStarted => "promotion.canary_started",
            ProvenanceEventKind::RollupRecorded => "memory.rollup.recorded",
            ProvenanceEventKind::BudgetFold => "budget.fold",
        }
    }

    /// Every mandatory-provenance event class (the full §8.1 #7 table, 24 classes).
    pub const ALL: [ProvenanceEventKind; 24] = [
        ProvenanceEventKind::PermissionProposed,
        ProvenanceEventKind::PermissionGranted,
        ProvenanceEventKind::PermissionChecked,
        ProvenanceEventKind::PermissionConsumed,
        ProvenanceEventKind::HandoffSent,
        ProvenanceEventKind::HandoffReceived,
        ProvenanceEventKind::LabelEndorsed,
        ProvenanceEventKind::LabelDeclassified,
        ProvenanceEventKind::LabelApplied,
        ProvenanceEventKind::ProcedurePromoted,
        ProvenanceEventKind::ShimCall,
        ProvenanceEventKind::Install,
        ProvenanceEventKind::SecuritySnapshot,
        ProvenanceEventKind::SecurityDeclassify,
        ProvenanceEventKind::SecurityOverride,
        ProvenanceEventKind::MemoryAudit,
        ProvenanceEventKind::MemoryPolicy,
        ProvenanceEventKind::WasmModuleTrusted,
        ProvenanceEventKind::MarketImported,
        ProvenanceEventKind::MarketPinned,
        ProvenanceEventKind::MaturityGradeRecorded,
        ProvenanceEventKind::PromotionCanaryStarted,
        ProvenanceEventKind::RollupRecorded,
        ProvenanceEventKind::BudgetFold,
    ];
}

/// The provenance a compliant append of one event class must carry (§8.1 #7): the author's
/// full [`crate::record::ProvenanceRecord`], plus any class-specific named fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredProvenance {
    /// The event class.
    pub event: ProvenanceEventKind,
    /// Whether the event must carry its author's full `ProvenanceRecord` (always true for the
    /// mandatory table — kept as a field so the table is self-describing).
    pub author_record: bool,
    /// Additional provenance fields the class must carry (e.g. `subject_ref`, `operand`,
    /// `from_agent`/`to_agent`).
    pub extra_fields: &'static [&'static str],
}

/// The §8.1 #7 mandatory-provenance table: every listed event requires its author's
/// `ProvenanceRecord`; endorsement/declassification additionally carry
/// `subject_ref`/`from`/`to`/`basis`/`basis_ref`; permission events carry `operand` +
/// `envelope`; handoffs carry both endpoints' provenance.
pub fn required_provenance(event: ProvenanceEventKind) -> RequiredProvenance {
    let extra_fields: &'static [&'static str] = match event {
        ProvenanceEventKind::LabelEndorsed | ProvenanceEventKind::LabelDeclassified => {
            &["subject_ref", "from", "to", "basis", "basis_ref"]
        }
        ProvenanceEventKind::LabelApplied => &["subject_ref", "label"],
        ProvenanceEventKind::PermissionProposed
        | ProvenanceEventKind::PermissionGranted
        | ProvenanceEventKind::PermissionChecked
        | ProvenanceEventKind::PermissionConsumed => &["operand", "envelope"],
        ProvenanceEventKind::HandoffSent | ProvenanceEventKind::HandoffReceived => {
            &["from_agent", "to_agent", "payload_provenance"]
        }
        _ => &[],
    };
    RequiredProvenance {
        event,
        author_record: true,
        extra_fields,
    }
}

/// Whether `event` is a mandatory-provenance class — the append-time lookup the stamping path
/// consults. `true` for every class in [`ProvenanceEventKind::ALL`].
pub fn requires_provenance(event: ProvenanceEventKind) -> bool {
    ProvenanceEventKind::ALL.contains(&event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mandatory_event_requires_its_author_record() {
        // §8.1 #7: every listed class carries the author's ProvenanceRecord.
        for event in ProvenanceEventKind::ALL {
            let req = required_provenance(event);
            assert!(
                req.author_record,
                "{} must carry its author record",
                event.as_str()
            );
            assert!(requires_provenance(event));
        }
        // The endorsement/declassification events additionally name the subject + basis.
        let endorsed = required_provenance(ProvenanceEventKind::LabelEndorsed);
        assert!(endorsed.extra_fields.contains(&"subject_ref"));
        assert!(endorsed.extra_fields.contains(&"basis"));
        // Permission events carry the operand and envelope (check 1's inputs).
        let proposed = required_provenance(ProvenanceEventKind::PermissionProposed);
        assert!(proposed.extra_fields.contains(&"operand"));
        assert!(proposed.extra_fields.contains(&"envelope"));
        // Handoffs carry both endpoints' provenance.
        let sent = required_provenance(ProvenanceEventKind::HandoffSent);
        assert!(sent.extra_fields.contains(&"from_agent"));
        assert!(sent.extra_fields.contains(&"to_agent"));
        // The table is the full §8.1 #7 list.
        assert_eq!(ProvenanceEventKind::ALL.len(), 24);
    }
}
