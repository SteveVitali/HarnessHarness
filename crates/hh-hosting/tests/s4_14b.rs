//! S4.14b acceptance — the LT-07 hosted arms of AC-R-2.8.3-7 (§5g.3 §8;
//! ADR-0057/0058 boundary rule; ADR-0165 D7): a hosted participant
//! receives a credential binding only over a channel in its declared
//! `credential_supply` — undeclared is `unknown` and *nothing* is bound
//! (CF-131: whole-registry injection forbidden). The subagent arm
//! (`ScopeExceedsChannel`) lives in `hh-secrets/tests/s4_14b.rs`.

use std::collections::BTreeMap;

use hh_hir::leaves::Text;
use hh_hir::records::{AssumptionDebtRecord, EvidenceRef, ExpiryCondition, OwnerRef};
use hh_ontology::debt::{DebtStatus, ExpiryKind};
use hh_ontology::participant::{
    CapabilityVerdict, HostingMechanism, Observability, ParticipantClass, ParticipantDescriptor,
};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use hh_hosting::abi::{HostedRunSpec, HostingError};
use hh_hosting::adapter_a::{adapter_a_record, allow_all_decider, AdapterA};
use hh_hosting::fixture::FixtureParticipant;
use hh_hosting::records::{HostingExt, ModelIoIntercept, ParticipantRecord, ProcessPlacement};
use hh_hosting::service::HostingService;

fn complete_debt() -> AssumptionDebtRecord {
    let provenance = ProvenanceRecord::kernel("hh-hosting/test", 0);
    AssumptionDebtRecord {
        rule_id: "hh.hosting.adapter_a".into(),
        hypothesis: Text::new(
            "participants matching participant_selector behave per declaration_defaults",
            "hh.adapter.a",
            provenance.clone(),
        ),
        evidence_refs: vec![EvidenceRef::legacy("adapterA-fixture")],
        owner: OwnerRef::principal("hh.adapter.a"),
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::ProbeFailure,
            value: Some("P0 dimension drift".into()),
        },
        removal_test_ref: "tests/s4_14b.rs".into(),
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

/// A participant record declaring (or not declaring) `credential_supply`.
fn participant(supply: Option<Vec<String>>) -> ParticipantRecord {
    ParticipantRecord::new(
        "p:test",
        "1.0.0",
        ParticipantDescriptor {
            class: ParticipantClass::Hosted,
            hosting_mechanism: HostingMechanism::SessionAbi,
            observability_level: [Observability::Events, Observability::EndState]
                .into_iter()
                .collect(),
            capability_vector: BTreeMap::from([(
                "streaming".to_string(),
                CapabilityVerdict::Supported,
            )]),
        },
        BTreeMap::from([("streaming".to_string(), Json::str("supported"))]),
        HostingExt {
            abi_versions: Some(vec!["hh-hosting/1".into()]),
            credential_supply: supply,
            ..Default::default()
        },
        BTreeMap::new(),
    )
    .expect("participant")
}

fn service(participant: ParticipantRecord) -> HostingService {
    let adapter = AdapterA::new(
        Box::new(FixtureParticipant::default()),
        adapter_a_record("a/1", ModelIoIntercept::None, complete_debt()),
        Some(allow_all_decider()),
    );
    HostingService::attach(
        participant,
        adapter,
        BTreeMap::from([("streaming".to_string(), Json::str("supported"))]),
        true,
        BTreeMap::new(),
    )
    .expect("attach")
}

fn spec(channels: &[&str]) -> HostedRunSpec {
    HostedRunSpec {
        definition_ref: "def:1".into(),
        params: Json::obj([]),
        placement: ProcessPlacement::InEnvironment,
        connection_info: Json::obj([("socket", Json::str("/tmp/x"))]),
        context_items: Vec::new(),
        budget_view: None,
        credential_channels: channels.iter().map(|s| s.to_string()).collect(),
        resume_cursor: None,
    }
}

/// LT-07 prong 2 — a hosted participant that declares no
/// `credential_supply` receives no binding: any `credential_channels`
/// entry is `CredentialChannelUndeclared` at `open`, before the adapter
/// ever sees the spec.
#[test]
fn lt07_hosted_without_credential_supply_receives_no_binding() {
    let mut svc = service(participant(None));
    match svc.open(spec(&["env"])) {
        Err(HostingError::CredentialChannelUndeclared { channel }) => {
            assert_eq!(channel, "env");
        }
        other => panic!("expected CredentialChannelUndeclared, got {other:?}"),
    }
    // A run asking for no channels still opens — absence is not a refusal
    // of the run, only of the credential supply.
    assert!(svc.open(spec(&[])).is_ok());
}

/// LT-07 prong 3 — with a declaration, only the declared channels bind:
/// `open` refuses a superset, `deliver_credential` refuses an undeclared
/// channel and delivers (a ref — never a value) over a declared one.
#[test]
fn lt07_hosted_binds_only_declared_credential_supply_channels() {
    let mut svc = service(participant(Some(vec!["env".into()])));
    // Superset request → refused at open.
    match svc.open(spec(&["env", "mcp"])) {
        Err(HostingError::CredentialChannelUndeclared { channel }) => {
            assert_eq!(channel, "mcp");
        }
        other => panic!("expected CredentialChannelUndeclared, got {other:?}"),
    }
    // The declared subset opens.
    let opened = svc.open(spec(&["env"])).expect("open");
    // `deliver_credential` on an undeclared channel is refused at the
    // boundary — the value never approaches the verb surface.
    match svc.deliver_credential(&opened.session_ref, "mcp", "cred-ref-1") {
        Err(HostingError::CredentialChannelUndeclared { channel }) => {
            assert_eq!(channel, "mcp");
        }
        other => panic!("expected CredentialChannelUndeclared, got {other:?}"),
    }
    // The declared channel accepts the *ref*.
    assert!(svc
        .deliver_credential(&opened.session_ref, "env", "cred-ref-1")
        .is_ok());
}
