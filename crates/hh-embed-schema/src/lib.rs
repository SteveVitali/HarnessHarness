//! `hh-embed-schema` — **the single schema source** for the `hh-embed/1`
//! contract (R-2.11.4; §7.4; ADR-0176/0178/0179/0180/0183; CC7: "every
//! class-document schema is exported from the kernel's single schema
//! source and shared with the §7 kernel↔surfaces binding; plugin/client
//! bindings are generated and CI-checked; schema drift is a build
//! failure, never a runtime negotiation").
//!
//! Stage-1 slice (S1.25): Groups **H** (`hello` negotiation),
//! **S** (`open_session` new|resume|attach, `close`), **W** (`submit`,
//! `cancel`, `respond_permission`), **R** (`stream_events`, `read`,
//! `head`, `project`, `account`, `describe`, `list_leases`), the full
//! **frame model**, the **closed `EmbedError` sum**, the operation
//! registry (incl. the declared-now/served-later Stage-2+ ops and the
//! **Group L** signature set, ADR-0183), the **stability map**, and
//! strict `UnknownField{path}` decoding (I2).
//!
//! Every wire type is defined once here; [`export_schema`] derives the
//! canonical export the codegen pipeline consumes — the schema and the
//! Rust types cannot drift (tests assert the registry, the error sum,
//! and the export agree).

use hh_wire::json::Json;

pub mod errors;
pub mod frames;
pub mod ops;
/// The `plugin_abi/1` kernel↔extension binding schema (spec §8.4, R-2.12.2;
/// ticket S1.27) — the same single schema source (V6/CC7); its export is the
/// `schema/plugin-abi-1.schema.json` artifact. The Hosting ABI is a different
/// binding — this module never references it (AC-R-2.12.2-12).
pub mod plugin_abi;
pub mod strict;
pub mod types;

pub use errors::{EmbedError, ALL_ERROR_KINDS};
pub use frames::{ephemeral_kind_of, Frame, StreamNotification, CLOSED_REASONS, EPHEMERAL_KINDS};
pub use ops::{registry, stability_map, Direction, OpDebt, OpSpec, Tier};
pub use types::*;

/// The contract major version. A `contract_major` bump is an ADR
/// (ADR-0178 D3); additive evolution within a major never bumps it (CC8).
pub const CONTRACT_MAJOR: i64 = 1;

/// The contract name, as it appears in the exported schema `$id`.
pub const CONTRACT_NAME: &str = "hh-embed/1";

/// The IDP `idp/1` is the contract's only content-addressing scheme
/// (CC1) — surfaced on `KernelDescriptor.idp`.
pub const IDP: &str = "idp/1";

// ---------------------------------------------------------------------------
// Contract identity + negotiation (Group H).
// ---------------------------------------------------------------------------

/// `ContractIdentity{contract_major, schema_hash, kernel_version_id}`
/// (§7.4; ADR-0178 D2). `schema_hash` is a content address over the
/// canonical schema export; a generated client asserts
/// `(contract_major, schema_hash)`; mismatch is typed, never a silent
/// fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractIdentity {
    pub contract_major: i64,
    pub schema_hash: String,
    pub kernel_version_id: String,
}

impl ContractIdentity {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("contract_major", Json::Int(self.contract_major)),
            ("schema_hash", Json::str(self.schema_hash.clone())),
            (
                "kernel_version_id",
                Json::str(self.kernel_version_id.clone()),
            ),
        ])
    }
}

/// Schema hashes this kernel still recognizes — the basis for the
/// `SchemaMismatch.direction` member (ADR-0178 D8): an asserted hash in
/// the retired list means `kernel_newer`; anything else unrecognized
/// means `client_newer`. Retired hashes are appended here when the
/// schema moves — append-only.
fn retired_schema_hashes() -> &'static [&'static str] {
    // Stage-0 hash retired when S1.25 landed — kept so a Stage-0 client
    // gets `kernel_newer`, not `client_newer`. (The Stage-0 export is
    // reproduced verbatim by the test that pins this value.)
    &["sha256:83c5b4b415f5b01f2c9ca87d53c9df68a021b6c6e51a7d7be37a544c66b95ce1"]
}

/// Negotiate `hello` against the kernel (ADR-0178 D2/D8; the AC-R-2.11.4-8
/// compatibility matrix). `Ok(HelloResult)` when compatible; a typed
/// [`EmbedError`] otherwise — never a silent fallback.
///
/// Order matters: major first, then schema assertion, then floor —
/// the matrix's refusal precedence.
pub fn negotiate(params: &HelloParams, kernel_version_id: &str) -> Result<HelloResult, EmbedError> {
    if params.contract_major != CONTRACT_MAJOR {
        return Err(EmbedError::ContractMajorUnsupported {
            requested: params.contract_major,
            supported: vec![CONTRACT_MAJOR],
        });
    }
    let kernel_hash = schema_hash();
    if let Some(asserted) = &params.schema_hash {
        if asserted != &kernel_hash {
            let direction = if retired_schema_hashes().contains(&asserted.as_str()) {
                "kernel_newer"
            } else {
                "client_newer"
            };
            return Err(EmbedError::SchemaMismatch {
                client: asserted.clone(),
                kernel: kernel_hash,
                direction: direction.to_string(),
            });
        }
    }
    if let Some(floor) = &params.kernel_floor {
        if version_lt(kernel_version_id, floor) {
            return Err(EmbedError::KernelBelowFloor {
                version: kernel_version_id.to_string(),
                floor: floor.clone(),
            });
        }
    }
    Ok(HelloResult {
        kernel: KernelDescriptor {
            version: kernel_version_id.to_string(),
            schema_hash: kernel_hash,
            contract_major: CONTRACT_MAJOR,
            idp: IDP.to_string(),
        },
        negotiated: params.capabilities.clone(),
        stability: stability_map(),
        experimental_enabled: params.capabilities.experimental,
    })
}

/// A conservative `a < b` over `x.y.z` version ids (the `kernel_floor`
/// comparison). Non-semver strings compare equal (never below floor —
/// refusing on an unparseable floor would break forward clients).
fn version_lt(a: &str, b: &str) -> bool {
    fn triple(s: &str) -> Option<(u64, u64, u64)> {
        let mut it = s.split('.');
        let p = |x: Option<&str>| x.and_then(|v| v.parse::<u64>().ok());
        match (p(it.next()), p(it.next()), p(it.next())) {
            (Some(x), Some(y), Some(z)) => Some((x, y, z)),
            _ => None,
        }
    }
    match (triple(a), triple(b)) {
        (Some(x), Some(y)) => x < y,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Schema export + content address (the codegen pipeline's input).
// ---------------------------------------------------------------------------

/// The canonical export of the `hh-embed/1` contract — the *single
/// source* every binding is generated from. Pure function of the
/// definitions in this crate; carries no clock/environment value, so it
/// is safe to content-address.
pub fn export_schema() -> Json {
    Json::obj([
        ("$schema", Json::str("hh-embed/schema-export/1")),
        ("$id", Json::str(CONTRACT_NAME)),
        ("contract_major", Json::Int(CONTRACT_MAJOR)),
        ("methods", methods_schema()),
        ("types", types_schema()),
        ("frames", frames_schema()),
        ("errors", errors_schema()),
        (
            "stability_tiers",
            Json::Arr(vec![Json::str("stable"), Json::str("experimental")]),
        ),
        (
            "bindings",
            Json::Arr(vec![Json::str("in_process"), Json::str("stdio")]),
        ),
    ])
}

fn methods_schema() -> Json {
    let mut m = std::collections::BTreeMap::new();
    for op in registry() {
        let mut e = std::collections::BTreeMap::new();
        e.insert("group".into(), Json::str(op.group));
        e.insert(
            "direction".into(),
            Json::str(match op.direction {
                Direction::Call => "call",
                Direction::Upcall => "upcall",
            }),
        );
        e.insert("params".into(), Json::str(op.params));
        e.insert("result".into(), Json::str(op.result));
        e.insert(
            "errors".into(),
            Json::Arr(op.errors.iter().map(|s| Json::str(*s)).collect()),
        );
        e.insert("tier".into(), Json::str(op.tier.as_str()));
        e.insert("implemented".into(), Json::Bool(op.implemented));
        if let Some(cap) = op.requires_capability {
            e.insert("requires_capability".into(), Json::str(cap));
        }
        if let Some(d) = &op.debt {
            e.insert(
                "debt".into(),
                Json::obj([
                    ("assumption_id", Json::str(d.assumption_id)),
                    ("debt_id", Json::str(d.debt_id)),
                    ("owner_layer", Json::str(d.owner_layer)),
                    ("stage_gate", Json::str(d.stage_gate)),
                    ("status", Json::str("open")),
                    ("detail", Json::str(d.detail)),
                ]),
            );
        }
        m.insert(op.name.to_string(), Json::Obj(e));
    }
    Json::Obj(m)
}

/// The declarative type table the codegen emitter reads. Field-type
/// vocabulary: `string | integer | bool | json | TypeName | [T] |
/// map<string,json>`; `required:false` members decode absent (the
/// HostCapabilities absent⇒false rule is handled by the emitter for
/// bools).
fn types_schema() -> Json {
    let mut m = std::collections::BTreeMap::new();

    let strct = |fields: &[(&'static str, &'static str, bool)]| {
        Json::obj([
            ("kind", Json::str("struct")),
            (
                "fields",
                Json::Arr(
                    fields
                        .iter()
                        .map(|(name, ty, required)| {
                            Json::obj([
                                ("name", Json::str(*name)),
                                ("type", Json::str(*ty)),
                                ("required", Json::Bool(*required)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    };
    let str_enum = |variants: &[&'static str]| {
        Json::obj([
            ("kind", Json::str("string_enum")),
            (
                "variants",
                Json::Arr(variants.iter().map(|s| Json::str(*s)).collect()),
            ),
        ])
    };
    // `{name, type, required}` field rows for a tagged-sum variant.
    type VariantField = (&'static str, &'static str, bool);
    type Variant = (&'static str, &'static [VariantField]);
    let tagged = |variants: &[Variant]| {
        Json::obj([
            ("kind", Json::str("tagged_sum")),
            ("tag", Json::str("kind")),
            (
                "variants",
                Json::Arr(
                    variants
                        .iter()
                        .map(|(name, fs)| {
                            Json::obj([
                                ("name", Json::str(*name)),
                                (
                                    "fields",
                                    Json::Arr(
                                        fs.iter()
                                            .map(|(n, t, r)| {
                                                Json::obj([
                                                    ("name", Json::str(*n)),
                                                    ("type", Json::str(*t)),
                                                    ("required", Json::Bool(*r)),
                                                ])
                                            })
                                            .collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    };

    // ── Negotiation ──────────────────────────────────────────────────
    m.insert(
        "ClientKind".into(),
        str_enum(&[
            "cli",
            "web",
            "ide",
            "sdk",
            "lab",
            "mcp_server",
            "acp_bridge",
            "test",
        ]),
    );
    m.insert(
        "ClientDescriptor".into(),
        strct(&[
            ("name", "string", true),
            ("version", "string", true),
            ("kind", "ClientKind", true),
        ]),
    );
    m.insert(
        "HostCapabilities".into(),
        strct(&[
            ("experimental", "bool", false),
            ("opt_out_notifications", "[string]", false),
            ("serves_permission_channel", "bool", false),
            ("serves_host_executor", "bool", false),
            ("serves_hook_observer", "bool", false),
            ("serves_elicitation", "bool", false),
            ("serves_measurement", "bool", false),
            ("serves_principal_channel", "bool", false),
            ("accepts_ephemeral_frames", "bool", false),
            ("max_in_flight_sessions", "integer", false),
            ("extensions", "map<string,json>", false),
        ]),
    );
    m.insert(
        "HelloParams".into(),
        strct(&[
            ("contract_major", "integer", true),
            ("client", "ClientDescriptor", true),
            ("capabilities", "HostCapabilities", true),
            ("schema_hash", "string", false),
            ("kernel_floor", "string", false),
        ]),
    );
    m.insert(
        "KernelDescriptor".into(),
        strct(&[
            ("version", "string", true),
            ("schema_hash", "string", true),
            ("contract_major", "integer", true),
            ("idp", "string", true),
        ]),
    );
    m.insert(
        "HelloResult".into(),
        strct(&[
            ("kernel", "KernelDescriptor", true),
            ("negotiated", "HostCapabilities", true),
            ("stability", "json", true),
            ("experimental_enabled", "bool", false),
        ]),
    );
    m.insert(
        "ContractIdentity".into(),
        strct(&[
            ("contract_major", "integer", true),
            ("schema_hash", "string", true),
            ("kernel_version_id", "string", true),
        ]),
    );
    m.insert(
        "AssumptionDebtRecord".into(),
        strct(&[
            ("assumption_id", "string", true),
            ("debt_id", "string", true),
            ("owner_layer", "string", true),
            ("stage_gate", "string", true),
            ("status", "string", true),
            ("detail", "string", true),
        ]),
    );

    // ── Session ──────────────────────────────────────────────────────
    m.insert(
        "AttendanceValue".into(),
        str_enum(&["interactive", "async", "unattended"]),
    );
    m.insert(
        "AttendanceSource".into(),
        str_enum(&["declared", "tty_inferred", "forced"]),
    );
    m.insert(
        "AttendanceDeclaration".into(),
        strct(&[
            ("value", "AttendanceValue", true),
            ("source", "AttendanceSource", true),
        ]),
    );
    m.insert("OutputFormat".into(), str_enum(&["human", "json", "jsonl"]));
    m.insert(
        "InvocationRecord".into(),
        strct(&[
            ("argv_canonical", "[string]", true),
            ("cwd_ref", "string", true),
            ("principal", "string", true),
            ("attendance", "AttendanceDeclaration", true),
            ("output_format", "OutputFormat", true),
            ("stdin_digest", "string", false),
            ("overrides_layer_id", "string", false),
            ("instrument_record", "json", true),
            ("idempotency_key", "string", true),
        ]),
    );
    m.insert(
        "DefinitionInput".into(),
        tagged(&[
            ("document", &[("document", "json", true)]),
            ("ref", &[("ref", "string", true)]),
        ]),
    );
    m.insert(
        "EnvironmentInput".into(),
        tagged(&[
            ("connection_info", &[("connection_info", "json", true)]),
            ("ref", &[("ref", "string", true)]),
        ]),
    );
    m.insert(
        "BudgetInput".into(),
        tagged(&[
            ("ref", &[("ref", "string", true)]),
            ("node", &[("node", "json", true)]),
        ]),
    );
    m.insert(
        "Override".into(),
        strct(&[("pointer", "string", true), ("value", "json", true)]),
    );
    m.insert(
        "Supplies".into(),
        strct(&[
            ("context", "[json]", false),
            ("host_capabilities", "[json]", false),
            ("mcp_servers", "[json]", false),
            ("procedures", "[json]", false),
        ]),
    );
    m.insert("ResumeMode".into(), str_enum(&["continue", "takeover"]));
    m.insert(
        "OpenSpec".into(),
        tagged(&[
            (
                "new",
                &[
                    ("definition", "DefinitionInput", true),
                    ("overrides", "[Override]", false),
                    ("profile_binding", "json", false),
                    ("environment", "EnvironmentInput", true),
                    ("budget", "BudgetInput", false),
                    ("participant", "json", false),
                    ("supplies", "Supplies", false),
                    ("attendance", "AttendanceDeclaration", true),
                    ("approval_mode", "string", false),
                ],
            ),
            (
                "resume",
                &[
                    ("run_id", "string", true),
                    ("mode", "ResumeMode", true),
                    ("from_seq", "integer", false),
                    ("definition", "DefinitionInput", false),
                ],
            ),
            (
                "attach",
                &[("run_id", "string", true), ("read_only", "bool", false)],
            ),
        ]),
    );
    m.insert(
        "OpenSessionParams".into(),
        strct(&[
            ("spec", "OpenSpec", true),
            ("idempotency_key", "string", true),
            ("invocation", "InvocationRecord", false),
        ]),
    );
    m.insert(
        "RealizedSettings".into(),
        strct(&[
            ("model_role_table_realized", "json", true),
            ("cwd", "string", true),
            ("containment_effective", "json", true),
            ("policy_mode", "string", true),
            ("profile_bindings", "json", true),
            ("protocol_bindings", "[json]", false),
            ("secrets_declared", "[json]", false),
            ("attendance", "AttendanceDeclaration", true),
        ]),
    );
    m.insert("SeqCursor".into(), strct(&[("seq", "integer", true)]));
    m.insert(
        "Session".into(),
        strct(&[
            ("session_id", "string", true),
            ("run_id", "string", true),
            ("attachment_id", "string", true),
            ("manifest_ref", "string", true),
            ("configuration_id", "string", true),
            ("configuration_version_id", "string", true),
            ("realized", "RealizedSettings", true),
            ("cursor", "SeqCursor", true),
        ]),
    );
    m.insert(
        "CloseReason".into(),
        str_enum(&["done", "abandon", "host_shutdown"]),
    );
    m.insert(
        "ClosedReason".into(),
        str_enum(&[
            "complete",
            "cancelled",
            "slow_consumer",
            "run_ended",
            "session_detached",
            "kernel_shutdown",
        ]),
    );
    m.insert(
        "ItemKind".into(),
        str_enum(&["context_item", "context_delta"]),
    );
    m.insert(
        "EphemeralKind".into(),
        str_enum(&[
            "delta",
            "progress",
            "permission_rendering",
            "guard_notice",
            "log",
        ]),
    );
    m.insert(
        "CloseParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("reason", "CloseReason", true),
        ]),
    );

    // ── Work ─────────────────────────────────────────────────────────
    m.insert(
        "SubmitParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("input", "[json]", true),
            ("idempotency_key", "string", true),
        ]),
    );
    m.insert("Accepted".into(), strct(&[("turn_id", "string", true)]));
    m.insert(
        "Acknowledged".into(),
        strct(&[("acknowledged", "bool", true)]),
    );
    m.insert(
        "Recorded".into(),
        strct(&[
            ("recorded", "bool", true),
            ("permission_id", "string", true),
        ]),
    );
    m.insert(
        "CancelScope".into(),
        tagged(&[("turn", &[("turn_id", "string", true)]), ("run", &[])]),
    );
    m.insert(
        "CancelParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("scope", "CancelScope", true),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "PermissionOutcome".into(),
        tagged(&[
            ("selected", &[("option_id", "string", true)]),
            ("cancelled", &[]),
        ]),
    );
    m.insert(
        "RespondPermissionParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("permission_id", "string", true),
            ("outcome", "PermissionOutcome", true),
            ("idempotency_key", "string", true),
        ]),
    );

    // ── Read ─────────────────────────────────────────────────────────
    m.insert(
        "ReadCursor".into(),
        tagged(&[
            ("seq", &[("seq", "integer", true)]),
            ("event_id", &[("event_id", "string", true)]),
            ("now", &[]),
        ]),
    );
    m.insert(
        "ClassFilter".into(),
        strct(&[("classes", "[string]", false)]),
    );
    m.insert(
        "StreamEventsParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("from", "ReadCursor", false),
            ("filter", "ClassFilter", false),
        ]),
    );
    m.insert(
        "StreamTicket".into(),
        strct(&[("subscription_id", "string", true)]),
    );
    m.insert("ReadDirection".into(), str_enum(&["fwd", "rev"]));
    m.insert(
        "ReadParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("cursor", "ReadCursor", true),
            ("direction", "ReadDirection", false),
            ("limit", "integer", false),
            ("filter", "ClassFilter", false),
        ]),
    );
    m.insert(
        "Page".into(),
        strct(&[("events", "[json]", true), ("next", "ReadCursor", false)]),
    );
    m.insert(
        "HeadParams".into(),
        strct(&[("session_id", "string", true)]),
    );
    m.insert(
        "Head".into(),
        strct(&[
            ("seq", "integer", true),
            ("event_id", "string", true),
            ("hash", "string", true),
        ]),
    );
    m.insert(
        "ViewKind".into(),
        str_enum(&["context_view", "run_summary", "checkpoint"]),
    );
    m.insert(
        "ProjectParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("view_kind", "ViewKind", true),
            ("until_seq", "integer", false),
        ]),
    );
    m.insert(
        "DerivedFrom".into(),
        strct(&[("seq", "integer", true), ("hash", "string", true)]),
    );
    m.insert(
        "View".into(),
        strct(&[
            ("payload", "json", true),
            ("derived_from", "DerivedFrom", true),
            ("view_hash", "string", true),
        ]),
    );
    m.insert(
        "AccountParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("until_seq", "integer", false),
        ]),
    );
    m.insert("AccountView".into(), strct(&[("account", "json", true)]));
    m.insert(
        "DescribeParams".into(),
        strct(&[("session_id", "string", true)]),
    );
    m.insert(
        "EnvDescribe".into(),
        strct(&[
            ("connection_info", "json", true),
            ("health", "string", true),
            ("meters", "[json]", false),
        ]),
    );
    m.insert(
        "DescribeResult".into(),
        strct(&[
            ("manifest_ref", "string", true),
            ("participant_descriptor", "json", true),
            ("protocol_bindings", "[json]", false),
            ("realized", "RealizedSettings", true),
            ("environment", "EnvDescribe", true),
        ]),
    );
    m.insert(
        "ListLeasesParams".into(),
        strct(&[("session_id", "string", true)]),
    );
    m.insert(
        "ListLeasesResult".into(),
        strct(&[("leases", "[json]", true)]),
    );
    m.insert(
        "StreamNotification".into(),
        strct(&[
            ("subscription_id", "string", true),
            ("frame", "Frame", true),
        ]),
    );

    // ── close result + implemented staged/experimental signatures ──────
    m.insert(
        "RunSummaryRef".into(),
        strct(&[
            ("run_id", "string", true),
            ("seq", "integer", true),
            ("hash", "string", true),
        ]),
    );
    m.insert("Closed".into(), strct(&[("final", "RunSummaryRef", true)]));
    m.insert(
        "SteerParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("expected_turn_id", "string", false),
            ("input", "[json]", true),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "ForkPoint".into(),
        tagged(&[
            ("seq", &[("seq", "integer", true)]),
            (
                "event_ref",
                &[("run_id", "string", true), ("event_id", "string", true)],
            ),
        ]),
    );
    m.insert(
        "ForkParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("at", "ForkPoint", true),
            ("kind", "string", false),
            ("env", "string", false),
            ("replay_mode", "string", false),
            ("policy_ref", "string", false),
            ("budget_slice_ref", "string", false),
            ("coerce_to_boundary", "bool", false),
            ("snapshot_ref", "string", false),
            ("manifest_delta", "json", false),
            ("idempotency_key", "string", false),
            ("invocation", "InvocationRecord", false),
        ]),
    );
    m.insert(
        "HostEffectOutcome".into(),
        tagged(&[
            (
                "observed",
                &[
                    ("outcome", "string", true),
                    ("output_artifacts", "[string]", false),
                ],
            ),
            ("refused", &[("reason", "string", true)]),
            ("unknown", &[("detail", "string", false)]),
        ]),
    );
    m.insert(
        "ReportHostEffectParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("effect_id", "string", true),
            ("attempt_no", "integer", true),
            ("outcome", "HostEffectOutcome", true),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "ElicitOutcome".into(),
        tagged(&[("answer", &[("value", "json", true)]), ("cancelled", &[])]),
    );
    m.insert(
        "RespondElicitationParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("elicitation_id", "string", true),
            ("outcome", "ElicitOutcome", true),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "LineageParams".into(),
        strct(&[("session_id", "string", true)]),
    );
    m.insert(
        "GetArtifactParams".into(),
        strct(&[("session_id", "string", true), ("address", "string", true)]),
    );
    m.insert(
        "PermissionOption".into(),
        strct(&[("option_id", "string", true), ("label", "string", true)]),
    );
    m.insert(
        "RequestPermission".into(),
        strct(&[
            ("permission_id", "string", true),
            ("proposal", "string", true),
            ("options", "[PermissionOption]", true),
            ("effect_id", "string", false),
            ("rendering", "json", false),
        ]),
    );
    m.insert(
        "InvokeHostCapability".into(),
        strct(&[
            ("capability_id", "string", true),
            ("args", "json", true),
            ("effect_id", "string", true),
            ("attempt_no", "integer", true),
        ]),
    );
    m.insert(
        "HookNotification".into(),
        strct(&[
            ("hook_id", "string", true),
            ("event_class", "string", true),
            ("event", "json", true),
        ]),
    );
    // `HookResult` — `allow` is *not* a member (raise-only; the schema
    // refuses a hook `allow` outright — I7/CC8).
    m.insert(
        "HookResult".into(),
        tagged(&[
            ("raise", &[("reason", "string", true)]),
            ("deny", &[("reason", "string", true)]),
            ("annotate", &[("annotations", "json", true)]),
            ("none", &[]),
        ]),
    );
    m.insert(
        "ElicitParams".into(),
        strct(&[
            ("elicitation_id", "string", true),
            ("prompt", "json", true),
            ("options", "[json]", false),
        ]),
    );
    m.insert(
        "ElicitResult".into(),
        strct(&[("outcome", "ElicitOutcome", true)]),
    );
    m.insert("HostResult".into(), strct(&[("outcome", "json", true)]));

    // ── Staged-op signatures (declared now; refused at serve time) ────
    m.insert(
        "NavigateParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("to", "json", false),
            ("reason", "string", false),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "AmendParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("target", "string", true),
            ("value", "json", true),
            ("attestation", "json", false),
            ("idempotency_key", "string", false),
            ("invocation", "InvocationRecord", false),
        ]),
    );
    m.insert(
        "ArchiveParams".into(),
        strct(&[("session_id", "string", true)]),
    );
    m.insert(
        "AuditViewParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("until_seq", "integer", false),
        ]),
    );
    m.insert(
        "CoherentForkPointsParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("from_seq", "integer", false),
            ("to_seq", "integer", false),
        ]),
    );
    m.insert(
        "CounterfactualParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("run_id", "string", false),
            ("at_seq", "integer", true),
            ("intervention", "json", true),
            ("design", "json", true),
            ("env", "string", false),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "DiscardParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("branch_id", "string", true),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "GrantParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("grant", "json", true),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "PromoteParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("branch_id", "string", true),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "ProveConsistencyParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("first_size", "integer", true),
            ("second_size", "integer", true),
        ]),
    );
    m.insert(
        "ProveInclusionParams".into(),
        strct(&[("session_id", "string", true), ("seq", "integer", true)]),
    );
    m.insert(
        "ReplayParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("run_id", "string", false),
            ("driver_mode", "string", true),
            ("until_seq", "integer", false),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "ReplayResult".into(),
        strct(&[
            ("run_id", "string", true),
            ("driver_mode", "string", true),
            ("mode", "string", true),
            ("report_ref", "string", true),
            ("reproduced", "bool", false),
            ("divergence", "json", false),
        ]),
    );
    m.insert(
        "CounterfactualResult".into(),
        strct(&[
            ("source_run_id", "string", true),
            ("at_seq", "integer", true),
            ("intervention_ref", "string", true),
            ("factual", "[json]", true),
            ("counterfactual", "[json]", true),
            ("comparison_ref", "string", true),
        ]),
    );
    m.insert(
        "RequestRedactionParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("refs", "[string]", true),
            ("reason", "string", true),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "RevokeLeaseParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("lease_id", "string", true),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "RollbackParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("to_seq", "integer", true),
            ("reason", "string", false),
            ("restore_env", "bool", false),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "SetCoordinateParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("coordinate", "string", true),
            ("value", "json", true),
            ("idempotency_key", "string", false),
        ]),
    );
    m.insert(
        "VerifyParams".into(),
        strct(&[
            ("session_id", "string", true),
            ("until_seq", "integer", false),
        ]),
    );

    Json::Obj(m)
}

/// The frame sum's schema — emitted into `frames` so codegen produces the
/// `Frame` enum and clients verify `durable.hash` (AC-R-2.11.4-3).
fn frames_schema() -> Json {
    let field = |name: &str, ty: &str, required: bool| {
        Json::obj([
            ("name", Json::str(name)),
            ("type", Json::str(ty)),
            ("required", Json::Bool(required)),
        ])
    };
    let dur = field("durability", "string", true);
    let variant = |name: &str, mut fields: Vec<Json>| {
        fields.push(dur.clone());
        Json::obj([("name", Json::str(name)), ("fields", Json::Arr(fields))])
    };
    Json::obj([(
        "Frame",
        Json::obj([
            ("kind", Json::str("tagged_sum")),
            ("tag", Json::str("kind")),
            (
                "variants",
                Json::Arr(vec![
                    variant(
                        "durable",
                        vec![
                            field("seq", "integer", true),
                            field("hash", "string", true),
                            field("event", "json", true),
                        ],
                    ),
                    variant(
                        "sync",
                        vec![
                            field("from_seq", "integer", true),
                            field("through_seq", "integer", true),
                            field("head_hash", "string", true),
                        ],
                    ),
                    variant(
                        "ephemeral",
                        vec![
                            field("ephemeral_kind", "EphemeralKind", true),
                            field("scope", "json", true),
                            field("attempt", "integer", true),
                            field("order", "integer", true),
                            field("payload", "json", true),
                        ],
                    ),
                    variant(
                        "item_started",
                        vec![
                            field("item_id", "string", true),
                            field("attempt", "integer", true),
                            field("anchor_seq", "integer", true),
                        ],
                    ),
                    variant(
                        "item_aborted",
                        vec![
                            field("item_id", "string", true),
                            field("attempt", "integer", true),
                            field("reason", "string", true),
                        ],
                    ),
                    variant(
                        "lagged",
                        vec![
                            field("dropped", "json", true),
                            field("resume_from_seq", "integer", true),
                        ],
                    ),
                    variant(
                        "closed",
                        vec![
                            field("reason", "ClosedReason", true),
                            field("detail", "string", false),
                        ],
                    ),
                    variant(
                        "rewind",
                        vec![
                            field("to_seq", "integer", true),
                            field("to_event_id", "string", true),
                            field("reason", "string", true),
                        ],
                    ),
                ]),
            ),
        ]),
    )])
}

/// The closed error sum — `{kind, code, fields}` per variant (code is the
/// stable JSON-RPC code; `fields` names the typed `data` members).
fn errors_schema() -> Json {
    let entries: &[(&str, i64, &[&str])] = &[
        ("NotInitialized", 1001, &[]),
        (
            "ContractMajorUnsupported",
            1002,
            &["requested", "supported"],
        ),
        ("SchemaMismatch", 1003, &["client", "kernel", "direction"]),
        ("KernelBelowFloor", 1004, &["version", "floor"]),
        ("ExperimentalRequired", 1005, &["reason"]),
        ("CapabilityNotDeclared", 1006, &["capability"]),
        ("UnknownField", 1100, &["path"]),
        ("SchemaViolation", 1101, &["path", "code"]),
        ("SecretInPayload", 1102, &[]),
        ("UnknownSession", 1200, &[]),
        ("UnknownRun", 1201, &["run_id"]),
        ("WouldBlock", 1202, &["active_holder"]),
        ("Draining", 1203, &[]),
        ("SessionDetached", 1204, &["reason", "event_ref"]),
        ("DefinitionChanged", 1205, &["reasons"]),
        ("TurnMismatch", 1206, &["active_turn_id"]),
        ("TurnActive", 1207, &[]),
        ("InvalidDefinition", 1300, &["diagnostics"]),
        ("UnresolvedRef", 1301, &["reference"]),
        ("AmbiguousVersion", 1302, &["reference"]),
        ("AuthorityViolation", 1303, &["layer", "detail"]),
        ("UnexpressibleSurface", 1304, &["detail"]),
        ("LinkError", 1305, &["detail"]),
        ("AuthorityWideningRequiresHuman", 1306, &["detail"]),
        ("InsufficientBudget", 1400, &["dimension"]),
        ("UnbudgetedArm", 1401, &[]),
        ("UnattendedRequiresInput", 1402, &[]),
        ("Refused", 1403, &["reason"]),
        ("Unsupported", 1404, &["by"]),
        ("UnknownEffect", 1500, &["effect_id"]),
        ("AlreadyDecided", 1501, &["permission_id"]),
        ("OptionNotOffered", 1502, &["option_id"]),
        ("UnknownPermission", 1503, &["permission_id"]),
        ("Fenced", 1504, &["detail"]),
        ("EnvironmentUnavailable", 1600, &["reason"]),
        ("UnknownCapability", 1601, &["capability"]),
        ("Overloaded", 1700, &[]),
        ("Disconnected", 1701, &[]),
        ("Timeout", 1702, &[]),
    ];
    Json::Arr(
        entries
            .iter()
            .map(|(kind, code, fs)| {
                Json::obj([
                    ("kind", Json::str(*kind)),
                    ("code", Json::Int(*code)),
                    (
                        "fields",
                        Json::Arr(fs.iter().map(|s| Json::str(*s)).collect()),
                    ),
                ])
            })
            .collect(),
    )
}

/// The canonical bytes of the schema export — the exact input to the
/// content address.
pub fn canonical_schema_bytes() -> String {
    export_schema().to_canonical_string()
}

/// The content address of the canonical schema export: an `idp/1`
/// [`hh_identity::ContentAddress`] over the canonical schema bytes,
/// rendered `sha256:<hex>` (§7.4; ADR-0036; CC1 — the one
/// content-addressing scheme in the tree).
pub fn schema_hash() -> String {
    hh_identity::address(
        canonical_schema_bytes().as_bytes(),
        "application/schema+json",
    )
    .id()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello() -> HelloParams {
        HelloParams {
            contract_major: CONTRACT_MAJOR,
            client: ClientDescriptor {
                name: "t".into(),
                version: "0".into(),
                kind: "test".into(),
            },
            capabilities: HostCapabilities::default(),
            schema_hash: None,
            kernel_floor: None,
        }
    }

    #[test]
    fn schema_hash_is_idp1_content_address() {
        assert_eq!(schema_hash(), schema_hash());
        assert!(schema_hash().starts_with("sha256:"));
        let expect = hh_identity::address(
            canonical_schema_bytes().as_bytes(),
            "application/schema+json",
        )
        .id();
        assert_eq!(schema_hash(), expect);
    }

    #[test]
    fn negotiate_matrix() {
        // matching → Ok
        assert!(negotiate(&hello(), "0.1.0").is_ok());
        // wrong major → ContractMajorUnsupported
        let mut p = hello();
        p.contract_major = 99;
        assert!(matches!(
            negotiate(&p, "0.1.0"),
            Err(EmbedError::ContractMajorUnsupported { requested: 99, .. })
        ));
        // stale schema → SchemaMismatch
        let mut p = hello();
        p.schema_hash = Some("sha256:stale".into());
        assert!(matches!(
            negotiate(&p, "0.1.0"),
            Err(EmbedError::SchemaMismatch { .. })
        ));
        // retired hash → kernel_newer direction
        let mut p = hello();
        p.schema_hash = Some(retired_schema_hashes()[0].to_string());
        match negotiate(&p, "0.1.0") {
            Err(EmbedError::SchemaMismatch { direction, .. }) => {
                assert_eq!(direction, "kernel_newer")
            }
            other => panic!("expected SchemaMismatch, got {other:?}"),
        }
        // below floor → KernelBelowFloor
        let mut p = hello();
        p.kernel_floor = Some("9.9.9".into());
        assert!(matches!(
            negotiate(&p, "0.1.0"),
            Err(EmbedError::KernelBelowFloor { .. })
        ));
        // above floor → Ok
        let mut p = hello();
        p.kernel_floor = Some("0.0.1".into());
        assert!(negotiate(&p, "0.1.0").is_ok());
    }

    #[test]
    fn every_registry_error_names_a_sum_member() {
        let kinds: std::collections::BTreeSet<&str> = ALL_ERROR_KINDS.iter().copied().collect();
        for op in registry() {
            for e in op.errors {
                assert!(
                    kinds.contains(e),
                    "op {} names error {e} absent from the closed sum",
                    op.name
                );
            }
        }
    }

    #[test]
    fn error_sum_and_export_agree() {
        let schema = export_schema();
        let errors = match schema.get("errors").unwrap() {
            Json::Arr(v) => v,
            _ => panic!("errors not an array"),
        };
        assert_eq!(
            errors.len(),
            ALL_ERROR_KINDS.len(),
            "closed error sum drifted from the export"
        );
        for (entry, kind) in errors.iter().zip(ALL_ERROR_KINDS.iter()) {
            assert_eq!(entry.get("kind").and_then(Json::as_str), Some(*kind));
        }
    }

    #[test]
    fn unknown_field_is_refused_with_path() {
        let mut m = match hello().to_json() {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        m.insert("bogus".to_string(), Json::Int(2));
        match HelloParams::from_json(&Json::Obj(m)) {
            Err(EmbedError::UnknownField { path }) => {
                assert_eq!(path, "hello/bogus")
            }
            other => panic!("expected UnknownField, got {other:?}"),
        }
    }

    #[test]
    fn frame_round_trips_and_durability_checked() {
        let f = Frame::Durable {
            seq: 7,
            hash: "sha256:x".into(),
            event: Json::obj([("class", Json::str("lifecycle.run.opened"))]),
        };
        let j = f.to_json();
        assert_eq!(Frame::from_json(&j, "f").unwrap(), f);
        // tampered durability is refused
        let mut m = match j {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        m.insert("durability".into(), Json::str("ephemeral"));
        assert!(matches!(
            Frame::from_json(&Json::Obj(m), "f"),
            Err(EmbedError::SchemaViolation { .. })
        ));
    }

    #[test]
    fn attach_is_read_only_by_construction() {
        let mut m = std::collections::BTreeMap::new();
        m.insert("kind".to_string(), Json::str("attach"));
        m.insert("run_id".to_string(), Json::str("r1"));
        m.insert("read_only".to_string(), Json::Bool(false));
        assert!(matches!(
            OpenSpec::from_json(&Json::Obj(m), "spec"),
            Err(EmbedError::SchemaViolation { .. })
        ));
    }

    #[test]
    fn group_l_signatures_present() {
        let names: Vec<&str> = registry().iter().map(|o| o.name).collect();
        for want in [
            "lab.assembly.assemble",
            "lab.registry.publish",
            "lab.experiment.open_experiment",
            "lab.results.get_row",
            "lab.leaderboard.define",
            "lab.hosting.attach",
            "lab.serve",
            "upcall.request_permission",
        ] {
            assert!(names.contains(&want), "registry missing {want}");
        }
    }
}
