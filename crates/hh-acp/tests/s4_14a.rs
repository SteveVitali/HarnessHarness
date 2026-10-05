//! S4.14a — the `session/request_permission` round-trip's preserved members
//! and structured losses (AC-R-2.8.7-10; §5d.4 P).
//!
//! Coverage:
//! - `permission_id`, `effect_id`, `effective_risk_class`, `reason_code`
//!   preserve verbatim under the namespaced `params._hh` member.
//! - Every member the ACP permission vocabulary cannot express —
//!   `modify`, `escalate`, `abort_run`, `max_uses`, `deadline` — lowers with a
//!   **named** loss (`hh-edge-loss/1`), never a silent narrowing.
//! - Inbound losses (`options[].kind` spellings, `maxUses`, `deadline`)
//!   surface on `PermissionRequest::losses` for the Π gate to name.
//! - The outcome parse is deny-by-default — an unrecognised outcome spelling
//!   never defaults to allow.

use hh_acp::permission::{
    inbound_losses, permission_loss_report, permission_losses, preserved_wire_member,
    PermissionOutcome, PermissionRequest, UNSUPPORTED_REQUEST_MEMBERS,
};
use hh_wire::json::Json;

/// The kernel `security.permission.requested` payload the tests scan.
fn requested_payload() -> Json {
    Json::obj([
        ("permission_id", Json::str("perm-7")),
        ("effect_id", Json::str("eff-7")),
        ("effective_risk_class", Json::str("reversible")),
        ("reason", Json::str("writes outside the declared root")),
        ("tool_name", Json::str("fs.write")),
    ])
}

#[test]
fn preserved_members_ride_the_namespaced_hh_member() {
    let hh = preserved_wire_member(&requested_payload());
    assert_eq!(
        hh.get("permissionId").and_then(Json::as_str),
        Some("perm-7")
    );
    assert_eq!(hh.get("effectId").and_then(Json::as_str), Some("eff-7"));
    assert_eq!(
        hh.get("effectiveRiskClass").and_then(Json::as_str),
        Some("reversible")
    );
    assert_eq!(
        hh.get("reasonCode").and_then(Json::as_str),
        Some("writes outside the declared root"),
        "the payload's `reason` member preserves under the wire `reasonCode` name"
    );
    assert!(
        hh.get("lossReport").is_none(),
        "a clean request carries no loss report"
    );
}

#[test]
fn unexpressible_members_lower_as_named_losses_never_dropped() {
    let mut payload = requested_payload();
    if let Json::Obj(m) = &mut payload {
        m.insert("modify".into(), Json::Bool(true));
        m.insert("abort_run".into(), Json::Bool(true));
        m.insert(
            "options_presented".into(),
            Json::Arr(vec![Json::obj([
                ("id", Json::str("escalate")),
                ("max_uses", Json::Int(3)),
                ("deadline", Json::Int(99)),
            ])]),
        );
    }
    let losses = permission_losses(&payload);
    for member in UNSUPPORTED_REQUEST_MEMBERS {
        assert!(
            losses.iter().any(|l| l.contains(member)),
            "loss for `{member}` is named: {losses:?}"
        );
    }
    let report = permission_loss_report(&payload).expect("losses produce a report");
    assert_eq!(
        report.get("schema").and_then(Json::as_str),
        Some(hh_compiler::acp::EDGE_LOSS_SCHEMA),
        "the report is `hh-edge-loss/1`-shaped"
    );
    assert_eq!(
        report.get("permission_id").and_then(Json::as_str),
        Some("perm-7"),
        "the report correlates on the durable permission coordinate"
    );
    // The report rides the wire member.
    let hh = preserved_wire_member(&payload);
    assert!(hh.get("lossReport").is_some(), "losses ride `params._hh`");
}

#[test]
fn inbound_losses_name_agent_side_members() {
    let params = Json::obj([(
        "options",
        Json::Arr(vec![
            Json::obj([("optionId", Json::str("allow")), ("maxUses", Json::Int(2))]),
            Json::obj([("kind", Json::str("abort_run"))]),
            Json::obj([("deadline", Json::Int(5))]),
        ]),
    )]);
    let losses = inbound_losses(&params);
    assert!(losses.iter().any(|l| l.contains("maxUses")));
    assert!(losses.iter().any(|l| l.contains("abort_run")));
    assert!(losses.iter().any(|l| l.contains("deadline")));
    // The request surface exposes them to the Π gate.
    let req = PermissionRequest {
        request_id: Json::Int(1),
        session_id: "s-1".into(),
        params,
    };
    assert_eq!(req.losses(), losses);
}

#[test]
fn an_unrecognised_outcome_is_never_an_allow() {
    for spelling in ["allowed_once", "ALLOWED", "", "granted"] {
        let wire = Json::obj([("outcome", Json::obj([("outcome", Json::str(spelling))]))]);
        assert_eq!(
            PermissionOutcome::from_wire(&wire),
            PermissionOutcome::Deny,
            "spelling {spelling:?} must parse to deny"
        );
    }
    // A malformed body (no `outcome` member at all) denies too.
    assert_eq!(
        PermissionOutcome::from_wire(&Json::obj([])),
        PermissionOutcome::Deny
    );
}
