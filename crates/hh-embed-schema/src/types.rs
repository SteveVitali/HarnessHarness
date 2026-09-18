//! The `hh-embed/1` record and sum types (§7.4 §2.3–§2.4; ADR-0176
//! D2/D3/D4, ADR-0180). Every type decodes strictly — `from_json` is
//! `UnknownField{path}`-refusing via [`crate::strict::StrictObj`].
//!
//! Canonical JSON representation notes (ADRs 0176/0178): ids, refs,
//! content addresses, and hashes are strings; seqs and counts are
//! integers; opaque kernel records (`ContextItem`, dossier rows,
//! `NodeAddress`, kernel details) travel as canonical `Json` values
//! decoded elsewhere — the boundary never interprets them.

use crate::errors::EmbedError;
use crate::strict::{closed_str, StrictObj};
use hh_wire::json::Json;
use std::collections::BTreeMap;

// ── Negotiation (Group H) ───────────────────────────────────────────────

/// `ClientDescriptor.kind` closed sum (§7.4 §2.3; the `mcp_server` member
/// is the declared MCP translation precedent — same closed sum when MCP
/// lands).
pub const CLIENT_KINDS: &[&str] = &[
    "cli",
    "web",
    "ide",
    "sdk",
    "lab",
    "mcp_server",
    "acp_bridge",
    "test",
];

/// Who is calling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientDescriptor {
    pub name: String,
    pub version: String,
    pub kind: String,
}

impl ClientDescriptor {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("name", Json::str(self.name.clone())),
            ("version", Json::str(self.version.clone())),
            ("kind", Json::str(self.kind.clone())),
        ])
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let name = s.req_str("name")?;
        let version = s.req_str("version")?;
        let kind_v = s.req("kind")?.clone();
        let kind = closed_str(&kind_v, CLIENT_KINDS, &format!("{path}/kind"))?;
        s.finish()?;
        Ok(ClientDescriptor {
            name,
            version,
            kind,
        })
    }
}

/// What the host can serve. **Absent member ⇒ `false`** — HostCapabilities
/// is the one place capability absence means "not served", never
/// "unknown" (§7.4 §2.3; ADR-0176 D3). `extensions` is the registered
/// prefixed-id slot — preserved byte-for-byte, never interpreted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostCapabilities {
    /// Opt in to `experimental`-tier items.
    pub experimental: bool,
    /// Event classes (prefixes) the host declines on subscriptions —
    /// `opt_out_notifications[]` (§7.4 §2.3). Durable rows are still
    /// committed; the declined classes are filtered at delivery.
    pub opt_out_notifications: Vec<String>,
    /// Can answer `request_permission` upcalls.
    pub serves_permission_channel: bool,
    /// Can execute `invoke_host_capability` upcalls.
    pub serves_host_executor: bool,
    /// Can observe `notify_hook` upcalls.
    pub serves_hook_observer: bool,
    /// Can answer `elicit` upcalls.
    pub serves_elicitation: bool,
    /// Can consume Group M/L payloads.
    pub serves_measurement: bool,
    /// Can author `principal`-labelled context items (the injection
    /// table's only authoring channel).
    pub serves_principal_channel: bool,
    /// Will accept `ephemeral` frames on subscriptions.
    pub accepts_ephemeral_frames: bool,
    /// Declared bound on in-flight sessions; `0` = kernel default.
    pub max_in_flight_sessions: i64,
    /// Prefixed-id extension members — preserved byte-for-byte.
    pub extensions: BTreeMap<String, Json>,
}

impl HostCapabilities {
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("experimental".into(), Json::Bool(self.experimental));
        m.insert(
            "opt_out_notifications".into(),
            Json::Arr(self.opt_out_notifications.iter().map(Json::str).collect()),
        );
        m.insert(
            "serves_permission_channel".into(),
            Json::Bool(self.serves_permission_channel),
        );
        m.insert(
            "serves_host_executor".into(),
            Json::Bool(self.serves_host_executor),
        );
        m.insert(
            "serves_hook_observer".into(),
            Json::Bool(self.serves_hook_observer),
        );
        m.insert(
            "serves_elicitation".into(),
            Json::Bool(self.serves_elicitation),
        );
        m.insert(
            "serves_measurement".into(),
            Json::Bool(self.serves_measurement),
        );
        m.insert(
            "serves_principal_channel".into(),
            Json::Bool(self.serves_principal_channel),
        );
        m.insert(
            "accepts_ephemeral_frames".into(),
            Json::Bool(self.accepts_ephemeral_frames),
        );
        m.insert(
            "max_in_flight_sessions".into(),
            Json::Int(self.max_in_flight_sessions),
        );
        if !self.extensions.is_empty() {
            m.insert("extensions".into(), Json::Obj(self.extensions.clone()));
        }
        Json::Obj(m)
    }

    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let b = |r: Result<Option<bool>, EmbedError>| r.map(|o| o.unwrap_or(false));
        let experimental = b(s.opt_bool("experimental"))?;
        let opt_out_notifications = s
            .opt_arr("opt_out_notifications")?
            .map(|a| {
                a.iter()
                    .enumerate()
                    .map(|(i, v)| {
                        v.as_str()
                            .map(String::from)
                            .ok_or(EmbedError::SchemaViolation {
                                path: format!("{path}/opt_out_notifications/{i}"),
                                code: "expected_string".to_string(),
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let serves_permission_channel = b(s.opt_bool("serves_permission_channel"))?;
        let serves_host_executor = b(s.opt_bool("serves_host_executor"))?;
        let serves_hook_observer = b(s.opt_bool("serves_hook_observer"))?;
        let serves_elicitation = b(s.opt_bool("serves_elicitation"))?;
        let serves_measurement = b(s.opt_bool("serves_measurement"))?;
        let serves_principal_channel = b(s.opt_bool("serves_principal_channel"))?;
        let accepts_ephemeral_frames = b(s.opt_bool("accepts_ephemeral_frames"))?;
        let max_in_flight_sessions = s.opt_int("max_in_flight_sessions")?.unwrap_or(0);
        let extensions = match s.take("extensions") {
            Some(Json::Obj(m)) => m.clone(),
            _ => BTreeMap::new(),
        };
        s.finish()?;
        Ok(HostCapabilities {
            experimental,
            opt_out_notifications,
            serves_permission_channel,
            serves_host_executor,
            serves_hook_observer,
            serves_elicitation,
            serves_measurement,
            serves_principal_channel,
            accepts_ephemeral_frames,
            max_in_flight_sessions,
            extensions,
        })
    }

    /// True iff `cap` (a capability member name) is served.
    pub fn serves(&self, cap: &str) -> bool {
        match cap {
            "serves_permission_channel" => self.serves_permission_channel,
            "serves_host_executor" => self.serves_host_executor,
            "serves_hook_observer" => self.serves_hook_observer,
            "serves_elicitation" => self.serves_elicitation,
            "serves_measurement" => self.serves_measurement,
            "serves_principal_channel" => self.serves_principal_channel,
            _ => false,
        }
    }
}

/// `hello` params (§7.4 §2.3). `schema_hash` is an optional assertion —
/// asserting it triggers the ADR-0178 D8 compatibility matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloParams {
    pub contract_major: i64,
    pub client: ClientDescriptor,
    pub capabilities: HostCapabilities,
    pub schema_hash: Option<String>,
    /// Optional kernel floor — `KernelBelowFloor` when this kernel is
    /// below it (the AC-R-2.11.4-8 matrix row).
    pub kernel_floor: Option<String>,
}

impl HelloParams {
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("contract_major".into(), Json::Int(self.contract_major));
        m.insert("client".into(), self.client.to_json());
        m.insert("capabilities".into(), self.capabilities.to_json());
        if let Some(h) = &self.schema_hash {
            m.insert("schema_hash".into(), Json::str(h.clone()));
        }
        if let Some(f) = &self.kernel_floor {
            m.insert("kernel_floor".into(), Json::str(f.clone()));
        }
        Json::Obj(m)
    }

    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "hello")?;
        let contract_major = s.req_int("contract_major")?;
        let client = ClientDescriptor::from_json(s.req("client")?, "hello/client")?;
        let capabilities =
            HostCapabilities::from_json(s.req("capabilities")?, "hello/capabilities")?;
        let schema_hash = s.opt_str("schema_hash")?;
        let kernel_floor = s.opt_str("kernel_floor")?;
        s.finish()?;
        Ok(HelloParams {
            contract_major,
            client,
            capabilities,
            schema_hash,
            kernel_floor,
        })
    }
}

/// Who answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelDescriptor {
    pub version: String,
    pub schema_hash: String,
    pub contract_major: i64,
    pub idp: String,
}

impl KernelDescriptor {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(self.version.clone())),
            ("schema_hash", Json::str(self.schema_hash.clone())),
            ("contract_major", Json::Int(self.contract_major)),
            ("idp", Json::str(self.idp.clone())),
        ])
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let version = s.req_str("version")?;
        let schema_hash = s.req_str("schema_hash")?;
        let contract_major = s.req_int("contract_major")?;
        let idp = s.req_str("idp")?;
        s.finish()?;
        Ok(KernelDescriptor {
            version,
            schema_hash,
            contract_major,
            idp,
        })
    }
}

/// An `AssumptionDebtRecord` — the staged-implementation ledger row
/// (§5.3.7): one record per assumed bridge between the emitting layer and
/// its currently-assumed realization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssumptionDebtRecord {
    pub assumption_id: String,
    pub debt_id: String,
    /// The layer that owns the assumption.
    pub owner_layer: String,
    /// The stage that must discharge it (`stage_gate`).
    pub stage_gate: String,
    /// `open` | `discharged` | `superseded`.
    pub status: String,
    /// The assumed bridge, in words.
    pub detail: String,
}

impl AssumptionDebtRecord {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("assumption_id", Json::str(self.assumption_id.clone())),
            ("debt_id", Json::str(self.debt_id.clone())),
            ("owner_layer", Json::str(self.owner_layer.clone())),
            ("stage_gate", Json::str(self.stage_gate.clone())),
            ("status", Json::str(self.status.clone())),
            ("detail", Json::str(self.detail.clone())),
        ])
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let assumption_id = s.req_str("assumption_id")?;
        let debt_id = s.req_str("debt_id")?;
        let owner_layer = s.req_str("owner_layer")?;
        let stage_gate = s.req_str("stage_gate")?;
        let status = s.req_str("status")?;
        let detail = s.req_str("detail")?;
        s.finish()?;
        Ok(AssumptionDebtRecord {
            assumption_id,
            debt_id,
            owner_layer,
            stage_gate,
            status,
            detail,
        })
    }
}

/// `hello` result — negotiated caps, kernel descriptor, and the
/// machine-readable `stability` map: `method → {tier, deprecated?,
/// debt?}` (§7.4 §2.3, §7.4 §6; ADR-0178 D7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloResult {
    pub kernel: KernelDescriptor,
    /// The negotiated capabilities (request ∧ what the kernel honors —
    /// Stage 1: the request verbatim; the field exists so narrowing can
    /// land without a dialect bump).
    pub negotiated: HostCapabilities,
    /// `method → {tier, deprecated?, debt?}`.
    pub stability: Json,
    /// Whether experimental items are live on this connection.
    pub experimental_enabled: bool,
}

impl HelloResult {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("kernel", self.kernel.to_json()),
            ("negotiated", self.negotiated.to_json()),
            ("stability", self.stability.clone()),
            (
                "experimental_enabled",
                Json::Bool(self.experimental_enabled),
            ),
        ])
    }
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "hello_result")?;
        let kernel = KernelDescriptor::from_json(s.req("kernel")?, "hello_result/kernel")?;
        let negotiated =
            HostCapabilities::from_json(s.req("negotiated")?, "hello_result/negotiated")?;
        let stability = s.req("stability")?.clone();
        let experimental_enabled = s.opt_bool("experimental_enabled")?.unwrap_or(false);
        s.finish()?;
        Ok(HelloResult {
            kernel,
            negotiated,
            stability,
            experimental_enabled,
        })
    }
}

// ── Session (Group S) ───────────────────────────────────────────────────

/// `AttendanceDeclaration` (ADR-0176 D4): `{value, source}` — the kernel
/// must see attendance is declared, never infer silence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttendanceDeclaration {
    /// `interactive` | `async` | `unattended`.
    pub value: String,
    /// `declared` | `tty_inferred` | `forced`.
    pub source: String,
}

pub const ATTENDANCE_VALUES: &[&str] = &["interactive", "async", "unattended"];
pub const ATTENDANCE_SOURCES: &[&str] = &["declared", "tty_inferred", "forced"];

impl AttendanceDeclaration {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("value", Json::str(self.value.clone())),
            ("source", Json::str(self.source.clone())),
        ])
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let value = closed_str(s.req("value")?, ATTENDANCE_VALUES, &format!("{path}/value"))?;
        let source = closed_str(
            s.req("source")?,
            ATTENDANCE_SOURCES,
            &format!("{path}/source"),
        )?;
        s.finish()?;
        Ok(AttendanceDeclaration { value, source })
    }
}

/// A definition input — inline `Document`, or a registry ref
/// (`{ref: "name@x.y.z"}` — names and published semver identity
/// resolve equally per §5.2.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionInput {
    /// `{kind:"document", document:{…}}` — a HIR Document.
    Document(Json),
    /// `{kind:"ref", ref:"name@semver"}`.
    Ref(String),
}

impl DefinitionInput {
    pub fn to_json(&self) -> Json {
        match self {
            DefinitionInput::Document(d) => {
                Json::obj([("kind", Json::str("document")), ("document", d.clone())])
            }
            DefinitionInput::Ref(r) => {
                Json::obj([("kind", Json::str("ref")), ("ref", Json::str(r.clone()))])
            }
        }
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let kind = closed_str(
            s.req("kind")?,
            &["document", "ref"],
            &format!("{path}/kind"),
        )?;
        let out = match kind.as_str() {
            "document" => DefinitionInput::Document(s.req("document")?.clone()),
            _ => DefinitionInput::Ref(s.req_str("ref")?),
        };
        s.finish()?;
        Ok(out)
    }
}

/// An environment input — connection info for the local/subprocess
/// backend, or a ref to a published environment profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentInput {
    /// `{kind:"connection_info", connection_info:{…}}` — passed to
    /// `EnvHandle::from_connection_info` (a `RuntimeRef` like
    /// `"local"`, plus optional `root`).
    ConnectionInfo(Json),
    /// `{kind:"ref", ref:"…"}` — a published environment profile.
    Ref(String),
}

impl EnvironmentInput {
    pub fn to_json(&self) -> Json {
        match self {
            EnvironmentInput::ConnectionInfo(c) => Json::obj([
                ("kind", Json::str("connection_info")),
                ("connection_info", c.clone()),
            ]),
            EnvironmentInput::Ref(r) => {
                Json::obj([("kind", Json::str("ref")), ("ref", Json::str(r.clone()))])
            }
        }
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let kind = closed_str(
            s.req("kind")?,
            &["connection_info", "ref"],
            &format!("{path}/kind"),
        )?;
        let out = match kind.as_str() {
            "connection_info" => {
                EnvironmentInput::ConnectionInfo(s.req("connection_info")?.clone())
            }
            _ => EnvironmentInput::Ref(s.req_str("ref")?),
        };
        s.finish()?;
        Ok(out)
    }
}

/// A budget input — inline `ResourceBudget`/`NodeAddress` or a ref.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetInput {
    /// `{kind:"ref", ref:"…"}`.
    Ref(String),
    /// `{kind:"node", node:{…}}` — an inline budget node.
    Node(Json),
}

impl BudgetInput {
    pub fn to_json(&self) -> Json {
        match self {
            BudgetInput::Ref(r) => {
                Json::obj([("kind", Json::str("ref")), ("ref", Json::str(r.clone()))])
            }
            BudgetInput::Node(n) => Json::obj([("kind", Json::str("node")), ("node", n.clone())]),
        }
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let kind = closed_str(s.req("kind")?, &["ref", "node"], &format!("{path}/kind"))?;
        let out = match kind.as_str() {
            "node" => BudgetInput::Node(s.req("node")?.clone()),
            _ => BudgetInput::Ref(s.req_str("ref")?),
        };
        s.finish()?;
        Ok(out)
    }
}

/// An ADR-0025 override — `{pointer, value}`: a JSON-pointer path into
/// the resolved definition plus the replacement value. Members an
/// `authority_cap` of a higher-precedence layer covers refuse
/// `AuthorityViolation` naming the capping layer (ADR-0024 D6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    pub pointer: String,
    pub value: Json,
}

impl Override {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("pointer", Json::str(self.pointer.clone())),
            ("value", self.value.clone()),
        ])
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let pointer = s.req_str("pointer")?;
        let value = s.req("value")?.clone();
        s.finish()?;
        Ok(Override { pointer, value })
    }
}

/// A per-run supplies bundle — each item is an opaque kernel record
/// (`ContextItem`, capability descriptor, …) whose *provenance claim*
/// travels inside it; the kernel computes the real label per the
/// injection table (I8).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Supplies {
    pub context: Vec<Json>,
    pub host_capabilities: Vec<Json>,
    pub mcp_servers: Vec<Json>,
    pub procedures: Vec<Json>,
}

impl Supplies {
    pub fn to_json(&self) -> Json {
        let arr = |v: &[Json]| Json::Arr(v.to_vec());
        Json::obj([
            ("context", arr(&self.context)),
            ("host_capabilities", arr(&self.host_capabilities)),
            ("mcp_servers", arr(&self.mcp_servers)),
            ("procedures", arr(&self.procedures)),
        ])
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let vec = |o: Option<&Vec<Json>>| o.cloned().unwrap_or_default();
        let context = vec(s.opt_arr("context")?);
        let host_capabilities = vec(s.opt_arr("host_capabilities")?);
        let mcp_servers = vec(s.opt_arr("mcp_servers")?);
        let procedures = vec(s.opt_arr("procedures")?);
        s.finish()?;
        Ok(Supplies {
            context,
            host_capabilities,
            mcp_servers,
            procedures,
        })
    }
}

/// The `OpenSpec` sum (§7.4 §2.4). `attach` is always read-only
/// (I6: the read-only kind cannot mutate — enforced at `submit`,
/// `respond_permission`, `cancel`). The `new` arm is large by
/// construction — it is the schema record verbatim, not a hot path.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenSpec {
    /// `{kind:"new", definition, overrides?, profile_binding?, environment,
    ///  budget?, participant?, supplies?, attendance, approval_mode?}`
    New {
        definition: DefinitionInput,
        overrides: Vec<Override>,
        /// Optional `ProfileBinding` record (link-time defaults otherwise).
        profile_binding: Option<Json>,
        environment: EnvironmentInput,
        budget: Option<BudgetInput>,
        /// `ParticipantDescriptor` — `{class:"native"}` only at Stage 1.
        participant: Option<Json>,
        supplies: Option<Supplies>,
        attendance: AttendanceDeclaration,
        /// `sync` | `out_of_process` (OQ-435) — absent ⇒ policy default.
        approval_mode: Option<String>,
    },
    /// `{kind:"resume", run_id, mode, from_seq?}` — `mode` ∈
    /// {continue (WouldBlock while the writer lives), takeover
    /// (fence the old writer)}.
    Resume {
        run_id: String,
        mode: String,
        from_seq: Option<i64>,
    },
    /// `{kind:"attach", run_id, read_only:true}` — read-only always;
    /// `read_only:false` is a `SchemaViolation`.
    Attach { run_id: String },
}

impl OpenSpec {
    pub fn to_json(&self) -> Json {
        match self {
            OpenSpec::New {
                definition,
                overrides,
                profile_binding,
                environment,
                budget,
                participant,
                supplies,
                attendance,
                approval_mode,
            } => {
                let mut m = BTreeMap::new();
                m.insert("kind".into(), Json::str("new"));
                m.insert("definition".into(), definition.to_json());
                m.insert(
                    "overrides".into(),
                    Json::Arr(overrides.iter().map(|o| o.to_json()).collect()),
                );
                if let Some(p) = profile_binding {
                    m.insert("profile_binding".into(), p.clone());
                }
                m.insert("environment".into(), environment.to_json());
                if let Some(b) = budget {
                    m.insert("budget".into(), b.to_json());
                }
                if let Some(p) = participant {
                    m.insert("participant".into(), p.clone());
                }
                if let Some(sp) = supplies {
                    m.insert("supplies".into(), sp.to_json());
                }
                m.insert("attendance".into(), attendance.to_json());
                if let Some(am) = approval_mode {
                    m.insert("approval_mode".into(), Json::str(am.clone()));
                }
                Json::Obj(m)
            }
            OpenSpec::Resume {
                run_id,
                mode,
                from_seq,
            } => {
                let mut m = BTreeMap::new();
                m.insert("kind".into(), Json::str("resume"));
                m.insert("run_id".into(), Json::str(run_id.clone()));
                m.insert("mode".into(), Json::str(mode.clone()));
                if let Some(s) = from_seq {
                    m.insert("from_seq".into(), Json::Int(*s));
                }
                Json::Obj(m)
            }
            OpenSpec::Attach { run_id } => Json::obj([
                ("kind", Json::str("attach")),
                ("run_id", Json::str(run_id.clone())),
                ("read_only", Json::Bool(true)),
            ]),
        }
    }

    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let kind = closed_str(
            s.req("kind")?,
            &["new", "resume", "attach"],
            &format!("{path}/kind"),
        )?;
        let out = match kind.as_str() {
            "new" => {
                let definition = DefinitionInput::from_json(
                    s.req("definition")?,
                    &format!("{path}/definition"),
                )?;
                let overrides = s
                    .opt_arr("overrides")?
                    .map(|a| {
                        a.iter()
                            .enumerate()
                            .map(|(i, o)| Override::from_json(o, &format!("{path}/overrides/{i}")))
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default();
                let profile_binding = s.take("profile_binding").cloned();
                let environment = EnvironmentInput::from_json(
                    s.req("environment")?,
                    &format!("{path}/environment"),
                )?;
                let budget = s
                    .take("budget")
                    .map(|b| BudgetInput::from_json(b, &format!("{path}/budget")))
                    .transpose()?;
                let participant = s.take("participant").cloned();
                let supplies = s
                    .take("supplies")
                    .map(|sp| Supplies::from_json(sp, &format!("{path}/supplies")))
                    .transpose()?;
                let attendance = AttendanceDeclaration::from_json(
                    s.req("attendance")?,
                    &format!("{path}/attendance"),
                )?;
                let approval_mode = s.opt_str("approval_mode")?;
                OpenSpec::New {
                    definition,
                    overrides,
                    profile_binding,
                    environment,
                    budget,
                    participant,
                    supplies,
                    attendance,
                    approval_mode,
                }
            }
            "resume" => {
                let run_id = s.req_str("run_id")?;
                let mode = closed_str(
                    s.req("mode")?,
                    &["continue", "takeover"],
                    &format!("{path}/mode"),
                )?;
                let from_seq = s.opt_int("from_seq")?;
                OpenSpec::Resume {
                    run_id,
                    mode,
                    from_seq,
                }
            }
            _ => {
                let run_id = s.req_str("run_id")?;
                // `attach` is read-only by construction; an explicit
                // `read_only:false` is a schema violation, not a mode.
                if s.opt_bool("read_only")? == Some(false) {
                    return Err(EmbedError::SchemaViolation {
                        path: format!("{path}/read_only"),
                        code: "attach_is_read_only".to_string(),
                    });
                }
                OpenSpec::Attach { run_id }
            }
        };
        s.finish()?;
        Ok(out)
    }
}

/// `open_session` params.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSessionParams {
    pub spec: OpenSpec,
    pub idempotency_key: String,
}

impl OpenSessionParams {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("spec", self.spec.to_json()),
            ("idempotency_key", Json::str(self.idempotency_key.clone())),
        ])
    }
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "open_session")?;
        let spec = OpenSpec::from_json(s.req("spec")?, "open_session/spec")?;
        let idempotency_key = s.req_str("idempotency_key")?;
        s.finish()?;
        Ok(OpenSessionParams {
            spec,
            idempotency_key,
        })
    }
}

/// What `open_session` realized — the `settings_realized` projection the
/// manifest fixes (I4: RealizedSettings == manifest-fixpoint; the
/// projection is recorded on `lifecycle.run.opened` so `describe` and
/// `resume` report identical bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealizedSettings {
    /// The realized model role table (`ModelRoleTable`), canonical.
    pub model_role_table_realized: Json,
    /// The effective working directory (environment-visible).
    pub cwd: String,
    /// The containment profile actually in force (canonical record).
    pub containment_effective: Json,
    /// `observe_only` | `sync` | `out_of_process` — realized policy mode.
    pub policy_mode: String,
    /// Profile bindings applied, canonical.
    pub profile_bindings: Json,
    /// The sealed `protocol_bindings` (realized).
    pub protocol_bindings: Vec<Json>,
    /// The declared secrets the run binds (names/paths only — never
    /// values; I9).
    pub secrets_declared: Vec<Json>,
    /// The realized `AttendanceDeclaration`.
    pub attendance: AttendanceDeclaration,
}

impl RealizedSettings {
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "model_role_table_realized",
                self.model_role_table_realized.clone(),
            ),
            ("cwd", Json::str(self.cwd.clone())),
            ("containment_effective", self.containment_effective.clone()),
            ("policy_mode", Json::str(self.policy_mode.clone())),
            ("profile_bindings", self.profile_bindings.clone()),
            (
                "protocol_bindings",
                Json::Arr(self.protocol_bindings.clone()),
            ),
            ("secrets_declared", Json::Arr(self.secrets_declared.clone())),
            ("attendance", self.attendance.to_json()),
        ])
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let model_role_table_realized = s.req("model_role_table_realized")?.clone();
        let cwd = s.req_str("cwd")?;
        let containment_effective = s.req("containment_effective")?.clone();
        let policy_mode = s.req_str("policy_mode")?;
        let profile_bindings = s.req("profile_bindings")?.clone();
        let protocol_bindings = s.opt_arr("protocol_bindings")?.cloned().unwrap_or_default();
        let secrets_declared = s.opt_arr("secrets_declared")?.cloned().unwrap_or_default();
        let attendance =
            AttendanceDeclaration::from_json(s.req("attendance")?, &format!("{path}/attendance"))?;
        s.finish()?;
        Ok(RealizedSettings {
            model_role_table_realized,
            cwd,
            containment_effective,
            policy_mode,
            profile_bindings,
            protocol_bindings,
            secrets_declared,
            attendance,
        })
    }
}

/// `open_session` result — the `Session` record (§7.4 §2.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub session_id: String,
    pub run_id: String,
    /// The `attachment_id` this session's writer lease holds.
    pub attachment_id: String,
    /// The sealed manifest's content address.
    pub manifest_ref: String,
    /// `{configuration_id, configuration_version_id}` — realized at
    /// open and immutable thereafter (CC9).
    pub configuration_id: String,
    pub configuration_version_id: String,
    pub realized: RealizedSettings,
    /// The resume cursor — the run's head seq at open.
    pub cursor_seq: i64,
}

impl Session {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("session_id", Json::str(self.session_id.clone())),
            ("run_id", Json::str(self.run_id.clone())),
            ("attachment_id", Json::str(self.attachment_id.clone())),
            ("manifest_ref", Json::str(self.manifest_ref.clone())),
            ("configuration_id", Json::str(self.configuration_id.clone())),
            (
                "configuration_version_id",
                Json::str(self.configuration_version_id.clone()),
            ),
            ("realized", self.realized.to_json()),
            ("cursor", Json::obj([("seq", Json::Int(self.cursor_seq))])),
        ])
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let session_id = s.req_str("session_id")?;
        let run_id = s.req_str("run_id")?;
        let attachment_id = s.req_str("attachment_id")?;
        let manifest_ref = s.req_str("manifest_ref")?;
        let configuration_id = s.req_str("configuration_id")?;
        let configuration_version_id = s.req_str("configuration_version_id")?;
        let realized =
            RealizedSettings::from_json(s.req("realized")?, &format!("{path}/realized"))?;
        let cursor_seq = {
            let cur = s.req("cursor")?;
            let mut cs = StrictObj::new(cur, &format!("{path}/cursor"))?;
            let seq = cs.req_int("seq")?;
            cs.finish()?;
            seq
        };
        s.finish()?;
        Ok(Session {
            session_id,
            run_id,
            attachment_id,
            manifest_ref,
            configuration_id,
            configuration_version_id,
            realized,
            cursor_seq,
        })
    }
}

/// `close` params — `reason` ∈ {done, abandon, host_shutdown}.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseParams {
    pub session_id: String,
    pub reason: String,
}

pub const CLOSE_REASONS: &[&str] = &["done", "abandon", "host_shutdown"];

impl CloseParams {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("session_id", Json::str(self.session_id.clone())),
            ("reason", Json::str(self.reason.clone())),
        ])
    }
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "close")?;
        let session_id = s.req_str("session_id")?;
        let reason = closed_str(s.req("reason")?, CLOSE_REASONS, "close/reason")?;
        s.finish()?;
        Ok(CloseParams { session_id, reason })
    }
}

// ── Work (Group W) ──────────────────────────────────────────────────────

/// `submit` params — `input` is `ContentBlock[] | ContextItem-ref[]`
/// (opaque records; the injection table fixes their labels, never the
/// host).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitParams {
    pub session_id: String,
    pub input: Vec<Json>,
    pub idempotency_key: String,
}

impl SubmitParams {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("session_id", Json::str(self.session_id.clone())),
            ("input", Json::Arr(self.input.clone())),
            ("idempotency_key", Json::str(self.idempotency_key.clone())),
        ])
    }
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "submit")?;
        let session_id = s.req_str("session_id")?;
        let input = s.req_arr("input")?.clone();
        let idempotency_key = s.req_str("idempotency_key")?;
        s.finish()?;
        Ok(SubmitParams {
            session_id,
            input,
            idempotency_key,
        })
    }
}

/// `submit`/`cancel`/`close`/`respond_permission` result shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    pub turn_id: String,
}
impl Accepted {
    pub fn to_json(&self) -> Json {
        Json::obj([("turn_id", Json::str(self.turn_id.clone()))])
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let turn_id = s.req_str("turn_id")?;
        s.finish()?;
        Ok(Accepted { turn_id })
    }
}

/// `cancel` params — `scope` ∈ {{kind:"turn", turn_id} | {kind:"run"}}.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelParams {
    pub session_id: String,
    pub scope: CancelScope,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelScope {
    Turn { turn_id: String },
    Run,
}

impl CancelScope {
    pub fn to_json(&self) -> Json {
        match self {
            CancelScope::Turn { turn_id } => Json::obj([
                ("kind", Json::str("turn")),
                ("turn_id", Json::str(turn_id.clone())),
            ]),
            CancelScope::Run => Json::obj([("kind", Json::str("run"))]),
        }
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let kind = closed_str(s.req("kind")?, &["turn", "run"], &format!("{path}/kind"))?;
        let out = match kind.as_str() {
            "turn" => CancelScope::Turn {
                turn_id: s.req_str("turn_id")?,
            },
            _ => CancelScope::Run,
        };
        s.finish()?;
        Ok(out)
    }
}

impl CancelParams {
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("session_id".into(), Json::str(self.session_id.clone()));
        m.insert("scope".into(), self.scope.to_json());
        if let Some(k) = &self.idempotency_key {
            m.insert("idempotency_key".into(), Json::str(k.clone()));
        }
        Json::Obj(m)
    }
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "cancel")?;
        let session_id = s.req_str("session_id")?;
        let scope = CancelScope::from_json(s.req("scope")?, "cancel/scope")?;
        let idempotency_key = s.opt_str("idempotency_key")?;
        s.finish()?;
        Ok(CancelParams {
            session_id,
            scope,
            idempotency_key,
        })
    }
}

/// The `respond_permission` outcome sum — `cancelled` ledgeres
/// `security.permission.decided{decision:cancelled}`, never a silent
/// missing option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionOutcome {
    Selected { option_id: String },
    Cancelled,
}

impl PermissionOutcome {
    pub fn to_json(&self) -> Json {
        match self {
            PermissionOutcome::Selected { option_id } => Json::obj([
                ("kind", Json::str("selected")),
                ("option_id", Json::str(option_id.clone())),
            ]),
            PermissionOutcome::Cancelled => Json::obj([("kind", Json::str("cancelled"))]),
        }
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let kind = closed_str(
            s.req("kind")?,
            &["selected", "cancelled"],
            &format!("{path}/kind"),
        )?;
        let out = match kind.as_str() {
            "selected" => PermissionOutcome::Selected {
                option_id: s.req_str("option_id")?,
            },
            _ => PermissionOutcome::Cancelled,
        };
        s.finish()?;
        Ok(out)
    }
}

/// `respond_permission` params.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespondPermissionParams {
    pub session_id: String,
    pub permission_id: String,
    pub outcome: PermissionOutcome,
    pub idempotency_key: String,
}

impl RespondPermissionParams {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("session_id", Json::str(self.session_id.clone())),
            ("permission_id", Json::str(self.permission_id.clone())),
            ("outcome", self.outcome.to_json()),
            ("idempotency_key", Json::str(self.idempotency_key.clone())),
        ])
    }
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "respond_permission")?;
        let session_id = s.req_str("session_id")?;
        let permission_id = s.req_str("permission_id")?;
        let outcome =
            PermissionOutcome::from_json(s.req("outcome")?, "respond_permission/outcome")?;
        let idempotency_key = s.req_str("idempotency_key")?;
        s.finish()?;
        Ok(RespondPermissionParams {
            session_id,
            permission_id,
            outcome,
            idempotency_key,
        })
    }
}

// ── Read (Group R) ──────────────────────────────────────────────────────

/// `stream_events` params — `{session_id, from, filter?}` →
/// `{subscription_id}` + `frame` notifications (§7.4 §2.4, §5). `from` is
/// the cursor sum — `now` is legal here (live-only replay-free tail).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamEventsParams {
    pub session_id: String,
    /// Replay start (absent ⇒ `seq:0`, full history).
    pub from: ReadCursor,
    /// `{classes?}` — a class-prefix filter (canonical-class strings).
    pub filter: Option<Vec<String>>,
}

impl StreamEventsParams {
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("session_id".into(), Json::str(self.session_id.clone()));
        m.insert("from".into(), self.from.to_json());
        if let Some(f) = &self.filter {
            m.insert(
                "filter".into(),
                Json::obj([(
                    "classes",
                    Json::Arr(f.iter().map(|c| Json::str(c.clone())).collect()),
                )]),
            );
        }
        Json::Obj(m)
    }
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "stream_events")?;
        let session_id = s.req_str("session_id")?;
        let from = match s.take("from") {
            Some(c) => ReadCursor::from_json(c, "stream_events/from")?,
            None => ReadCursor::Seq(0),
        };
        let filter = match s.take("filter") {
            Some(f) => {
                let mut fs = StrictObj::new(f, "stream_events/filter")?;
                let classes = fs
                    .opt_arr("classes")?
                    .map(|a| {
                        a.iter()
                            .map(|c| {
                                c.as_str().map(|s| s.to_string()).ok_or_else(|| {
                                    EmbedError::SchemaViolation {
                                        path: "stream_events/filter/classes".to_string(),
                                        code: "expected_string".to_string(),
                                    }
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?;
                fs.finish()?;
                classes
            }
            None => None,
        };
        s.finish()?;
        Ok(StreamEventsParams {
            session_id,
            from,
            filter,
        })
    }
}

/// `stream_events` result — the subscription ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamTicket {
    pub subscription_id: String,
}
impl StreamTicket {
    pub fn to_json(&self) -> Json {
        Json::obj([("subscription_id", Json::str(self.subscription_id.clone()))])
    }
}

/// A `read`/`stream_events` cursor — the tagged sum
/// `{kind:"seq",seq} | {kind:"event_id",event_id} | {kind:"now"}`
/// (`now` is live-only: `SchemaViolation` on `read`, legal on
/// `stream_events.from` — §7.4 §2.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadCursor {
    Seq(i64),
    EventId(String),
    Now,
}

impl ReadCursor {
    pub fn to_json(&self) -> Json {
        match self {
            ReadCursor::Seq(s) => Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(*s))]),
            ReadCursor::EventId(id) => Json::obj([
                ("kind", Json::str("event_id")),
                ("event_id", Json::str(id.clone())),
            ]),
            ReadCursor::Now => Json::obj([("kind", Json::str("now"))]),
        }
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let kind = closed_str(
            s.req("kind")?,
            &["seq", "event_id", "now"],
            &format!("{path}/kind"),
        )?;
        let out = match kind.as_str() {
            "seq" => ReadCursor::Seq(s.req_int("seq")?),
            "event_id" => ReadCursor::EventId(s.req_str("event_id")?),
            _ => ReadCursor::Now,
        };
        s.finish()?;
        Ok(out)
    }
}

/// `read` params.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadParams {
    pub session_id: String,
    pub cursor: ReadCursor,
    /// `fwd` | `rev` (absent ⇒ `fwd`).
    pub direction: String,
    /// Page bound — `0` = server default.
    pub limit: i64,
    /// `{classes?}` class-prefix filter.
    pub filter: Option<Vec<String>>,
}

impl ReadParams {
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("session_id".into(), Json::str(self.session_id.clone()));
        m.insert("cursor".into(), self.cursor.to_json());
        m.insert("direction".into(), Json::str(self.direction.clone()));
        m.insert("limit".into(), Json::Int(self.limit));
        if let Some(f) = &self.filter {
            m.insert(
                "filter".into(),
                Json::obj([(
                    "classes",
                    Json::Arr(f.iter().map(|c| Json::str(c.clone())).collect()),
                )]),
            );
        }
        Json::Obj(m)
    }
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "read")?;
        let session_id = s.req_str("session_id")?;
        let cursor = ReadCursor::from_json(s.req("cursor")?, "read/cursor")?;
        let direction = match s.opt_str("direction")? {
            Some(d) => closed_str(&Json::str(d), &["fwd", "rev"], "read/direction")?,
            None => "fwd".to_string(),
        };
        let limit = s.opt_int("limit")?.unwrap_or(0);
        let filter = match s.take("filter") {
            Some(f) => {
                let mut fs = StrictObj::new(f, "read/filter")?;
                let classes = fs
                    .opt_arr("classes")?
                    .map(|a| {
                        a.iter()
                            .map(|c| {
                                c.as_str().map(|s| s.to_string()).ok_or_else(|| {
                                    EmbedError::SchemaViolation {
                                        path: "read/filter/classes".to_string(),
                                        code: "expected_string".to_string(),
                                    }
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?;
                fs.finish()?;
                classes
            }
            None => None,
        };
        s.finish()?;
        Ok(ReadParams {
            session_id,
            cursor,
            direction,
            limit,
            filter,
        })
    }
}

/// `read` result — `{events: Event[], next?: cursor}`; `events` are the
/// canonical ledger event records (opaque to the boundary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub events: Vec<Json>,
    /// Absent ⇒ no more.
    pub next: Option<ReadCursor>,
}

impl Page {
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("events".into(), Json::Arr(self.events.clone()));
        if let Some(n) = &self.next {
            m.insert("next".into(), n.to_json());
        }
        Json::Obj(m)
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let events = s.req_arr("events")?.clone();
        let next = s
            .take("next")
            .map(|n| ReadCursor::from_json(n, &format!("{path}/next")))
            .transpose()?;
        s.finish()?;
        Ok(Page { events, next })
    }
}

/// `head` result — `{seq, event_id, hash}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub seq: i64,
    pub event_id: String,
    pub hash: String,
}
impl Head {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("seq", Json::Int(self.seq)),
            ("event_id", Json::str(self.event_id.clone())),
            ("hash", Json::str(self.hash.clone())),
        ])
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let seq = s.req_int("seq")?;
        let event_id = s.req_str("event_id")?;
        let hash = s.req_str("hash")?;
        s.finish()?;
        Ok(Head {
            seq,
            event_id,
            hash,
        })
    }
}

/// `project` params — `view_kind` ∈ {context_view, run_summary,
/// checkpoint} at Stage 1 (CC7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectParams {
    pub session_id: String,
    pub view_kind: String,
    /// Project through this seq (absent ⇒ head).
    pub until_seq: Option<i64>,
}

pub const PROJECT_VIEW_KINDS: &[&str] = &["context_view", "run_summary", "checkpoint"];

impl ProjectParams {
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("session_id".into(), Json::str(self.session_id.clone()));
        m.insert("view_kind".into(), Json::str(self.view_kind.clone()));
        if let Some(u) = self.until_seq {
            m.insert("until_seq".into(), Json::Int(u));
        }
        Json::Obj(m)
    }
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "project")?;
        let session_id = s.req_str("session_id")?;
        let view_kind = closed_str(s.req("view_kind")?, PROJECT_VIEW_KINDS, "project/view_kind")?;
        let until_seq = s.opt_int("until_seq")?;
        s.finish()?;
        Ok(ProjectParams {
            session_id,
            view_kind,
            until_seq,
        })
    }
}

/// `project` result — `{payload, derived_from:{seq, hash}, view_hash}`
/// (§7.4 §2.4: `view_hash = H(canonical(payload) || derived_from)`;
/// cross-binding identical by construction — AC-9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub payload: Json,
    pub derived_from_seq: i64,
    pub derived_from_hash: String,
    pub view_hash: String,
}

impl View {
    /// Compute `view_hash` (§7.4 §2.4).
    pub fn compute_view_hash(payload: &Json, seq: i64, hash: &str) -> String {
        let material = Json::obj([
            ("payload", payload.clone()),
            ("seq", Json::Int(seq)),
            ("hash", Json::str(hash.to_string())),
        ]);
        format!(
            "sha256:{}",
            hh_wire::sha256::sha256_hex(material.to_canonical_string().as_bytes())
        )
    }

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("payload", self.payload.clone()),
            (
                "derived_from",
                Json::obj([
                    ("seq", Json::Int(self.derived_from_seq)),
                    ("hash", Json::str(self.derived_from_hash.clone())),
                ]),
            ),
            ("view_hash", Json::str(self.view_hash.clone())),
        ])
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let payload = s.req("payload")?.clone();
        let (derived_from_seq, derived_from_hash) = {
            let df = s.req("derived_from")?;
            let mut ds = StrictObj::new(df, &format!("{path}/derived_from"))?;
            let seq = ds.req_int("seq")?;
            let hash = ds.req_str("hash")?;
            ds.finish()?;
            (seq, hash)
        };
        let view_hash = s.req_str("view_hash")?;
        s.finish()?;
        Ok(View {
            payload,
            derived_from_seq,
            derived_from_hash,
            view_hash,
        })
    }
}

/// `account` params.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountParams {
    pub session_id: String,
    pub until_seq: Option<i64>,
}
impl AccountParams {
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("session_id".into(), Json::str(self.session_id.clone()));
        if let Some(u) = self.until_seq {
            m.insert("until_seq".into(), Json::Int(u));
        }
        Json::Obj(m)
    }
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "account")?;
        let session_id = s.req_str("session_id")?;
        let until_seq = s.opt_int("until_seq")?;
        s.finish()?;
        Ok(AccountParams {
            session_id,
            until_seq,
        })
    }
}

/// `account` result — the `ResourceAccount` projection (§3.6.2), opaque
/// canonical record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountView {
    pub account: Json,
}
impl AccountView {
    pub fn to_json(&self) -> Json {
        Json::obj([("account", self.account.clone())])
    }
}

/// `describe` result — what the manifest fixed, plus environment
/// connection info/health/meters (§7.4 §2.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescribeResult {
    pub manifest_ref: String,
    pub participant_descriptor: Json,
    pub protocol_bindings: Vec<Json>,
    pub realized: RealizedSettings,
    /// `env_handle.connection_info` verbatim — never the handle id (I7).
    pub environment_connection_info: Json,
    /// `attached | healthy | degraded | stopped` — the EnvHandle health.
    pub environment_health: String,
    /// Live environment meters (canonical records).
    pub environment_meters: Vec<Json>,
}

impl DescribeResult {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("manifest_ref", Json::str(self.manifest_ref.clone())),
            (
                "participant_descriptor",
                self.participant_descriptor.clone(),
            ),
            (
                "protocol_bindings",
                Json::Arr(self.protocol_bindings.clone()),
            ),
            ("realized", self.realized.to_json()),
            (
                "environment",
                Json::obj([
                    ("connection_info", self.environment_connection_info.clone()),
                    ("health", Json::str(self.environment_health.clone())),
                    ("meters", Json::Arr(self.environment_meters.clone())),
                ]),
            ),
        ])
    }
}

/// `list_leases` result — live `ApprovalLease` records (canonical; empty
/// until `allow_lease` decisions land — the experimental op is honest
/// about its Stage-1 vacuity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListLeasesResult {
    pub leases: Vec<Json>,
}
impl ListLeasesResult {
    pub fn to_json(&self) -> Json {
        Json::obj([("leases", Json::Arr(self.leases.clone()))])
    }
}

/// The `Acknowledged` result — close/cancel.
pub fn acknowledged_json() -> Json {
    Json::obj([("acknowledged", Json::Bool(true))])
}

/// The `Recorded` result — `respond_permission` (decision recorded; the
/// dispatch outcome arrives via `stream_events`, not the response).
pub fn recorded_json(permission_id: &str) -> Json {
    Json::obj([
        ("recorded", Json::Bool(true)),
        ("permission_id", Json::str(permission_id.to_string())),
    ])
}

// ── close result + staged-op signatures + upcall records ─────────────────

/// `RunSummaryRef` — the head coordinate `close` returns
/// (`Closed{final}`): `{run_id, seq, hash}` at the moment of close
/// (§7.4 §2.4 — a *ref*, not the folded summary payload).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummaryRef {
    pub run_id: String,
    pub seq: i64,
    pub hash: String,
}
impl RunSummaryRef {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("run_id", Json::str(self.run_id.clone())),
            ("seq", Json::Int(self.seq)),
            ("hash", Json::str(self.hash.clone())),
        ])
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let run_id = s.req_str("run_id")?;
        let seq = s.req_int("seq")?;
        let hash = s.req_str("hash")?;
        s.finish()?;
        Ok(RunSummaryRef { run_id, seq, hash })
    }
}

/// `close` result — `Closed{final: RunSummaryRef}` (§7.4 §2.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closed {
    pub final_summary: RunSummaryRef,
}
impl Closed {
    pub fn to_json(&self) -> Json {
        Json::obj([("final", self.final_summary.to_json())])
    }
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let final_summary = RunSummaryRef::from_json(s.req("final")?, &format!("{path}/final"))?;
        s.finish()?;
        Ok(Closed { final_summary })
    }
}

/// `steer` params — `{session_id, expected_turn_id?, input, idempotency_key?}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteerParams {
    pub session_id: String,
    pub expected_turn_id: Option<String>,
    pub input: Vec<Json>,
    pub idempotency_key: Option<String>,
}
impl SteerParams {
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "steer")?;
        let session_id = s.req_str("session_id")?;
        let expected_turn_id = s.opt_str("expected_turn_id")?;
        let input = s.opt_arr("input")?.cloned().unwrap_or_default();
        let idempotency_key = s.opt_str("idempotency_key")?;
        s.finish()?;
        Ok(SteerParams {
            session_id,
            expected_turn_id,
            input,
            idempotency_key,
        })
    }
}

/// `ForkPoint` — `fork`'s `at` coordinate: `{kind:"seq",seq}` |
/// `{kind:"event_ref",run_id,event_id}` (§7.4 §2.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkPoint {
    Seq(i64),
    EventRef { run_id: String, event_id: String },
}
impl ForkPoint {
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let kind = closed_str(
            s.req("kind")?,
            &["seq", "event_ref"],
            &format!("{path}/kind"),
        )?;
        let out = match kind.as_str() {
            "seq" => ForkPoint::Seq(s.req_int("seq")?),
            _ => ForkPoint::EventRef {
                run_id: s.req_str("run_id")?,
                event_id: s.req_str("event_id")?,
            },
        };
        s.finish()?;
        Ok(out)
    }
}

/// `fork` params — `{session_id, at, manifest_delta?, idempotency_key?}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkParams {
    pub session_id: String,
    pub at: ForkPoint,
    pub manifest_delta: Option<Json>,
    pub idempotency_key: Option<String>,
}
impl ForkParams {
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "fork")?;
        let session_id = s.req_str("session_id")?;
        let at = ForkPoint::from_json(s.req("at")?, "fork/at")?;
        let manifest_delta = s.take("manifest_delta").cloned();
        let idempotency_key = s.opt_str("idempotency_key")?;
        s.finish()?;
        Ok(ForkParams {
            session_id,
            at,
            manifest_delta,
            idempotency_key,
        })
    }
}

/// `HostEffectOutcome` — the host's report on an `invoke_host_capability`
/// ask (§7.4 §2.5): `observed{outcome, output_artifacts[]?}` |
/// `refused{reason}` | `unknown{detail?}` — one row per effect terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEffectOutcome {
    Observed {
        outcome: String,
        output_artifacts: Vec<String>,
    },
    Refused {
        reason: String,
    },
    Unknown {
        detail: Option<String>,
    },
}
impl HostEffectOutcome {
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let kind = closed_str(
            s.req("kind")?,
            &["observed", "refused", "unknown"],
            &format!("{path}/kind"),
        )?;
        let out = match kind.as_str() {
            "observed" => {
                let outcome = closed_str(
                    s.req("outcome")?,
                    &["applied", "not_applied", "partial"],
                    &format!("{path}/outcome"),
                )?;
                let output_artifacts = s
                    .opt_arr("output_artifacts")?
                    .map(|a| {
                        a.iter()
                            .enumerate()
                            .map(|(i, v)| {
                                v.as_str()
                                    .map(String::from)
                                    .ok_or(EmbedError::SchemaViolation {
                                        path: format!("{path}/output_artifacts/{i}"),
                                        code: "expected_string".to_string(),
                                    })
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default();
                HostEffectOutcome::Observed {
                    outcome,
                    output_artifacts,
                }
            }
            "refused" => HostEffectOutcome::Refused {
                reason: s.req_str("reason")?,
            },
            _ => HostEffectOutcome::Unknown {
                detail: s.opt_str("detail")?,
            },
        };
        s.finish()?;
        Ok(out)
    }
}

/// `report_host_effect` params — `{session_id, effect_id, attempt_no,
/// outcome, idempotency_key?}` (§7.4 §2.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportHostEffectParams {
    pub session_id: String,
    pub effect_id: String,
    pub attempt_no: i64,
    pub outcome: HostEffectOutcome,
    pub idempotency_key: Option<String>,
}
impl ReportHostEffectParams {
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "report_host_effect")?;
        let session_id = s.req_str("session_id")?;
        let effect_id = s.req_str("effect_id")?;
        let attempt_no = s.req_int("attempt_no")?;
        let outcome =
            HostEffectOutcome::from_json(s.req("outcome")?, "report_host_effect/outcome")?;
        let idempotency_key = s.opt_str("idempotency_key")?;
        s.finish()?;
        Ok(ReportHostEffectParams {
            session_id,
            effect_id,
            attempt_no,
            outcome,
            idempotency_key,
        })
    }
}

/// `ElicitOutcome` — `respond_elicitation`'s answer:
/// `{kind:"answer",value}` | `{kind:"cancelled"}` (§7.4 §2.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElicitOutcome {
    Answer(Json),
    Cancelled,
}
impl ElicitOutcome {
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let kind = closed_str(
            s.req("kind")?,
            &["answer", "cancelled"],
            &format!("{path}/kind"),
        )?;
        let out = match kind.as_str() {
            "answer" => ElicitOutcome::Answer(s.req("value")?.clone()),
            _ => ElicitOutcome::Cancelled,
        };
        s.finish()?;
        Ok(out)
    }
}

/// `respond_elicitation` params.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespondElicitationParams {
    pub session_id: String,
    pub elicitation_id: String,
    pub outcome: ElicitOutcome,
    pub idempotency_key: Option<String>,
}
impl RespondElicitationParams {
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "respond_elicitation")?;
        let session_id = s.req_str("session_id")?;
        let elicitation_id = s.req_str("elicitation_id")?;
        let outcome = ElicitOutcome::from_json(s.req("outcome")?, "respond_elicitation/outcome")?;
        let idempotency_key = s.opt_str("idempotency_key")?;
        s.finish()?;
        Ok(RespondElicitationParams {
            session_id,
            elicitation_id,
            outcome,
            idempotency_key,
        })
    }
}

/// `lineage` params.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineageParams {
    pub session_id: String,
}
impl LineageParams {
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "lineage")?;
        let session_id = s.req_str("session_id")?;
        s.finish()?;
        Ok(LineageParams { session_id })
    }
}

/// `get_artifact` params — `{session_id, address}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetArtifactParams {
    pub session_id: String,
    pub address: String,
}
impl GetArtifactParams {
    pub fn from_json(v: &Json) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, "get_artifact")?;
        let session_id = s.req_str("session_id")?;
        let address = s.req_str("address")?;
        s.finish()?;
        Ok(GetArtifactParams {
            session_id,
            address,
        })
    }
}

// ── Group U upcall records (kernel → host; emitted as notifications) ────

/// `PermissionOption` — one offered option on a `request_permission`
/// upcall: `{option_id, label}` (OQ-246's interim set is
/// `allow_once | deny_once`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionOption {
    pub option_id: String,
    pub label: String,
}
impl PermissionOption {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("option_id", Json::str(self.option_id.clone())),
            ("label", Json::str(self.label.clone())),
        ])
    }
}

/// `upcall.request_permission` params — the ask the host answers with
/// `respond_permission` (§7.4 §2.6). `rendering` is advisory — the host
/// renders its own prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestPermission {
    pub permission_id: String,
    pub proposal: String,
    pub options: Vec<PermissionOption>,
    pub effect_id: Option<String>,
    pub rendering: Option<Json>,
}
impl RequestPermission {
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert(
            "permission_id".into(),
            Json::str(self.permission_id.clone()),
        );
        m.insert("proposal".into(), Json::str(self.proposal.clone()));
        m.insert(
            "options".into(),
            Json::Arr(self.options.iter().map(|o| o.to_json()).collect()),
        );
        if let Some(e) = &self.effect_id {
            m.insert("effect_id".into(), Json::str(e.clone()));
        }
        if let Some(r) = &self.rendering {
            m.insert("rendering".into(), r.clone());
        }
        Json::Obj(m)
    }
}

/// `upcall.invoke_host_capability` params — the kernel asks the declared
/// host executor to run the effect (§7.4 §2.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvokeHostCapability {
    pub capability_id: String,
    pub args: Json,
    pub effect_id: String,
    pub attempt_no: i64,
}
impl InvokeHostCapability {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("capability_id", Json::str(self.capability_id.clone())),
            ("args", self.args.clone()),
            ("effect_id", Json::str(self.effect_id.clone())),
            ("attempt_no", Json::Int(self.attempt_no)),
        ])
    }
}

/// `upcall.notify_hook` params — the hook observer's event notice
/// (§7.4 §2.6). `event` is the canonical event record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookNotification {
    pub hook_id: String,
    pub event_class: String,
    pub event: Json,
}
impl HookNotification {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("hook_id", Json::str(self.hook_id.clone())),
            ("event_class", Json::str(self.event_class.clone())),
            ("event", self.event.clone()),
        ])
    }
}

/// `HookResult` — the host's reply to `upcall.notify_hook`:
/// `raise | deny | annotate | none` (§7.4 §2.6). **`allow` is not a
/// member** — a hook cannot admit a dispatch (raise-only); decoding
/// `{kind:"allow"}` is a `SchemaViolation{code:"unknown_variant"}` — the
/// schema-refused hook allow (I7/CC8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookResult {
    Raise { reason: String },
    Deny { reason: String },
    Annotate { annotations: Json },
    None,
}
impl HookResult {
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let kind = closed_str(
            s.req("kind")?,
            &["raise", "deny", "annotate", "none"],
            &format!("{path}/kind"),
        )?;
        let out = match kind.as_str() {
            "raise" => HookResult::Raise {
                reason: s.req_str("reason")?,
            },
            "deny" => HookResult::Deny {
                reason: s.req_str("reason")?,
            },
            "annotate" => HookResult::Annotate {
                annotations: s.req("annotations")?.clone(),
            },
            _ => HookResult::None,
        };
        s.finish()?;
        Ok(out)
    }
}

/// `upcall.elicit` params — the kernel's structured ask
/// (`serves_elicitation` hosts answer via `respond_elicitation`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElicitParams {
    pub elicitation_id: String,
    pub prompt: Json,
    pub options: Vec<Json>,
}
impl ElicitParams {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("elicitation_id", Json::str(self.elicitation_id.clone())),
            ("prompt", self.prompt.clone()),
            ("options", Json::Arr(self.options.to_vec())),
        ])
    }
}

/// `upcall.elicit` result — the answered outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElicitResult {
    pub outcome: ElicitOutcome,
}
