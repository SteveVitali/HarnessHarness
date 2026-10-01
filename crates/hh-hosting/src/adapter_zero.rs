//! Adapter zero — the reference runtime presented through `hh-hosting/1`
//! (§6.6 §2.5; ADR-0005 "act as a participant"; ADR-0166). At C0/Stage 3 this
//! is a *fixture over native runs*, not a hosted runtime: [`project_native_run`]
//! projects a ledger slice into `HostedEvent`s, [`lift`](crate::proj::lift)
//! lowers them back, and the [`crate::proj::LoweringLossReport`] declares the
//! loss. The AC-R-2.10.6-2 parity claim is a test, not a service.
//!
//! The fixture's descriptor: `hosted`, `session_abi`,
//! `observability = {events, end_state}` + `model_io` (adapter zero is
//! intercepted — the reference runtime's model calls ride the Lab gateway),
//! never `ledger`.

use std::collections::BTreeMap;

use hh_hir::leaves::Text;
use hh_hir::records::{AssumptionDebtRecord, ExpiryCondition, OwnerRef};
use hh_ledger::event::EventEnvelope;
use hh_ontology::debt::{DebtStatus, ExpiryKind};
use hh_ontology::participant::{
    CapabilityVerdict, HostingMechanism, Observability, ParticipantClass, ParticipantDescriptor,
};
use hh_provenance::ProvenanceRecord;
use hh_wire::Json;

use crate::proj::{project, Projection};
use crate::records::{AdapterRecord, HostingExt, ModelIoIntercept, ProcessPlacement};

/// The adapter-zero record id (the fixture's stable name).
pub const ADAPTER_ZERO_ID: &str = "hh.adapter.zero";

/// Project a native run into the hosted presentation — the adapter-zero
/// fixture (intercepted: the descriptor's own `{events, end_state, model_io}`
/// set gates the model-I/O kinds). `events` must be the run's durable
/// envelopes in `seq` order; the projection is deterministic (a function of
/// the input bytes, never of a clock or a hashmap order).
pub fn project_native_run(events: &[EventEnvelope]) -> Projection {
    project(events, &adapter_zero_descriptor().observability_level)
}

/// The adapter-zero [`ParticipantDescriptor`] — `hosted`, `session_abi`,
/// `{events, end_state}` + `model_io` (intercepted), never `ledger` (§6.6
/// §2.5 — the Lab does not own the definition it hosts).
pub fn adapter_zero_descriptor() -> ParticipantDescriptor {
    ParticipantDescriptor {
        class: ParticipantClass::Hosted,
        hosting_mechanism: HostingMechanism::SessionAbi,
        observability_level: [
            Observability::Events,
            Observability::EndState,
            Observability::ModelIo,
        ]
        .into_iter()
        .collect(),
        capability_vector: BTreeMap::new(),
    }
}

/// The adapter-zero `AdapterRecord` — the reference adapter's conditioned
/// artefact (ADR-0166 D3: the debt record's hypothesis is the claim that
/// participants matching `participant_selector` behave per
/// `declaration_defaults`; the evidence is adapter zero's own AC-2 parity
/// fixture — a Lab-side observation, honestly labelled).
pub fn adapter_zero_record() -> AdapterRecord {
    let mut declaration_defaults = BTreeMap::new();
    // The reference runtime's own declaration — the capabilities the fixture
    // claims, as values (the probe catalogue reconciles them at S4.5a).
    for (dim, v) in [
        ("streaming", CapabilityVerdict::Supported),
        ("interrupt", CapabilityVerdict::Supported),
        ("steer", CapabilityVerdict::Supported),
        ("resume", CapabilityVerdict::Supported),
        ("fork", CapabilityVerdict::Supported),
        ("compaction", CapabilityVerdict::Supported),
        ("permission_surface", CapabilityVerdict::Supported),
        ("subagents", CapabilityVerdict::Supported),
    ] {
        declaration_defaults.insert(dim.to_string(), Json::str(v.as_str()));
    }
    AdapterRecord {
        adapter_id: ADAPTER_ZERO_ID.to_string(),
        version_id: "adapter-zero/1".to_string(),
        hosting_mechanism: HostingMechanism::SessionAbi,
        participant_selector: Json::obj([("participant_id", Json::str("hh.reference"))]),
        declaration_defaults,
        placement_supported: [ProcessPlacement::InEnvironment].into_iter().collect(),
        lowering_table_ref: None,
        loss_report_ref: None,
        debt: adapter_zero_debt(),
        ext: BTreeMap::new(),
    }
}

/// The adapter-zero `HostingExt` — the `hh.hosting/1` block the record's
/// participant candidates carry: intercepted model I/O, in-environment
/// placement, the protocol channel, exact accounting.
pub fn adapter_zero_ext() -> HostingExt {
    HostingExt {
        credential_supply: Some(vec!["proxy_injection".to_string()]),
        receipts: Some(CapabilityVerdict::Supported),
        account_exact: Some(CapabilityVerdict::Supported),
        elicitation: Some(CapabilityVerdict::Supported),
        model_io_intercept: Some(ModelIoIntercept::BaseUrl),
        process_placement: Some(ProcessPlacement::InEnvironment),
        tool_supply_channel: Some("sealed_mcp".to_string()),
        context_supply_mode: Some("delivered".to_string()),
        coordinates: None,
        budget_enforcement: None, // derived at Stage 4 — never authored
        abi_versions: Some(vec!["hh-hosting/1".to_string()]),
        event_channels: Some(
            [
                crate::events::EventChannel::Protocol,
                crate::events::EventChannel::Proxy,
                crate::events::EventChannel::Handle,
            ]
            .into_iter()
            .collect(),
        ),
        usage_mapping: Some("token_vector".to_string()),
        extra: BTreeMap::new(),
    }
}

/// Adapter zero's `AssumptionDebtRecord` — the debt hypothesis (ADR-0166 D3):
/// "the reference runtime behaves per `declaration_defaults` when presented
/// through `hh-hosting/1`". The removal test is the AC-2 parity fixture —
/// expire the adapter when the selector matches no registered participant or
/// a P0 dimension drifts.
fn adapter_zero_debt() -> AssumptionDebtRecord {
    let provenance = ProvenanceRecord::kernel("hh-hosting/adapter-zero", 0);
    AssumptionDebtRecord {
        rule_id: "hh.hosting.adapter_zero".to_string(),
        hypothesis: Text::new(
            "participants matching participant_selector behave per declaration_defaults",
            "hh.adapter.zero",
            provenance.clone(),
        ),
        evidence_refs: Vec::new(),
        owner: OwnerRef::principal("hh.adapter.zero"),
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::ProbeFailure,
            value: Some("P0 dimension drift or selector matches no participant".to_string()),
        },
        removal_test_ref: "ac-r-2.10.6-2".to_string(),
        status: DebtStatus::Active,
        debt_class: None,
        hypothesis_typed: None,
        scope: None,
        expiry: None,
        runway_ms: None,
        revalidation: None,
        removal_test: None,
        created_by: Some(provenance),
        created_at: Some(0),
        supersedes: None,
    }
}

/// The fixture's self-consistency check (AC-R-2.10.6-2's record half): the
/// record decodes, claims its mechanism/placement (I-6), and the descriptor
/// is `hosted` with a mechanism (never `none` — CF-351).
pub fn check_adapter_zero() -> Result<(), crate::records::RecordError> {
    let rec = adapter_zero_record();
    let round = AdapterRecord::from_json(&rec.to_json())?;
    // Canonical-byte equality — `debt_json` emits `Text` hash-only (content
    // addresses, never inline text), so the *value* loses `content` while the
    // canonical bytes are identical (the codec is content-addressed — CC7).
    if round.to_json() != rec.to_json() {
        return Err(crate::records::RecordError::Member {
            path: "adapter_zero".to_string(),
            detail: "codec round-trip diverged".to_string(),
        });
    }
    if !rec.claims(
        HostingMechanism::SessionAbi,
        ProcessPlacement::InEnvironment,
    ) {
        return Err(crate::records::RecordError::Member {
            path: "adapter_zero.claims".to_string(),
            detail: "the record must claim its own mechanism+placement".to_string(),
        });
    }
    Ok(())
}
