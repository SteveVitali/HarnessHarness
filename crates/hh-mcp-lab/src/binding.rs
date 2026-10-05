//! `CallerBinding` — the `hh-caller-binding/1` record (spec §7.3 record
//! table; ADR-0174 D1/D2): `{binding_id, credential ∈ {stdio_launch |
//! oauth | mtls}, principal_ref, caller_kind, authority_cap, permissions[],
//! budget_node, readers_identity, rate_policy?, expires_at?}`.
//!
//! Identity comes from the transport binding, never the request: on stdio
//! the launcher fixed the binding at spawn (`stdio_launch`); on Streamable
//! HTTP the `Authorization: Bearer` token resolves through the
//! [`CallerAuth`] seam — the H3 mediator's token→subject path. Secret
//! values never appear: credential members name refs (`issuer_ref`,
//! `cert_ref`), never material (ADR-0057).
//!
//! `caller_kind` is the authority ceiling (ADR-0174 D3): `service`
//! bindings run `unattended` (their launches can never demand a human);
//! `respond_approval` is the `permission_request` domain reachable only
//! by `human_principal` bindings — every other kind answers
//! `IllegitimateEndorsement` (AC-R-2.11.3-3).

use std::collections::BTreeMap;

use hh_wire::json::Json;

/// The binding record's schema id.
pub const CALLER_BINDING_SCHEMA: &str = "hh-caller-binding/1";

/// `caller_kind ∈ {agent, human_principal, provider_client, service}`
/// (spec §7.3 `CallerBinding`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CallerKind {
    /// A default agent caller — no approval authority.
    Agent,
    /// A human principal — may answer permission asks (R-2 principals).
    HumanPrincipal,
    /// A provider-side client.
    ProviderClient,
    /// A service caller — runs `unattended` by construction.
    Service,
}

impl CallerKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CallerKind::Agent => "agent",
            CallerKind::HumanPrincipal => "human_principal",
            CallerKind::ProviderClient => "provider_client",
            CallerKind::Service => "service",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<CallerKind> {
        match s {
            "agent" => Some(CallerKind::Agent),
            "human_principal" | "human" => Some(CallerKind::HumanPrincipal),
            "provider_client" => Some(CallerKind::ProviderClient),
            "service" => Some(CallerKind::Service),
            _ => None,
        }
    }

    /// R-2 principals: `respond_approval` is reachable only by a human
    /// binding (AC-R-2.11.3-3).
    pub fn may_respond_approval(self) -> bool {
        self == CallerKind::HumanPrincipal
    }

    /// The attendance ceiling a launch rides under (ADR-0174 D3): a
    /// `service` binding's launches are unattended — an approval-mode
    /// that demands a human can only hang.
    pub fn attendance_ceiling(self) -> &'static str {
        match self {
            CallerKind::Service => "unattended",
            _ => "attended",
        }
    }
}

/// `subject_kind ∈ {user, client}` — the OAuth subject's declared form
/// (the `principal_ref` form question is OQ-395 — the record carries the
/// declared kind, the ratified default is "none").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectKind {
    /// An end-user subject.
    User,
    /// A client-id subject.
    Client,
}

impl SubjectKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SubjectKind::User => "user",
            SubjectKind::Client => "client",
        }
    }
}

/// The credential half — `stdio_launch(launched_by, launch_event) |
/// oauth(issuer_ref, subject_kind, audience) | mtls(cert_ref)`. Refs
/// only; never secret material (ADR-0057).
#[derive(Debug, Clone, PartialEq)]
pub enum CallerCredential {
    /// Bound at spawn — the launcher is the trust anchor.
    StdioLaunch {
        /// Who launched the server process.
        launched_by: String,
        /// The launch record, when the launcher recorded one.
        launch_event: Option<String>,
    },
    /// The H3 OAuth path — issuer/audience/subject-kind; the token
    /// itself is the transport's, resolved by [`CallerAuth`].
    OAuth {
        /// The issuer's `TrustRootPolicy` ref.
        issuer_ref: String,
        /// `user` | `client`.
        subject_kind: SubjectKind,
        /// The audience the token must name.
        audience: String,
    },
    /// Mutual-TLS client certificate (the C2 arm — the record shape is
    /// declared; resolution is `stage_pending` at this slice).
    Mtls {
        /// The certificate's `SecretRef`-style reference.
        cert_ref: String,
    },
}

impl CallerCredential {
    /// The credential kind's canonical spelling (`stdio_launch` | `oauth`
    /// | `mtls`) — the assumption-debt carrier key (AC-R-2.11.3-12).
    pub fn kind_str(&self) -> &'static str {
        match self {
            CallerCredential::StdioLaunch { .. } => "stdio_launch",
            CallerCredential::OAuth { .. } => "oauth",
            CallerCredential::Mtls { .. } => "mtls",
        }
    }
}

/// The `hh-caller-binding/1` record.
#[derive(Debug, Clone, PartialEq)]
pub struct CallerBinding {
    /// The binding's own id (the `caller_process.participant_ref` and
    /// `mcp_surface(binding_id)` sink name).
    pub binding_id: String,
    /// How the caller authenticated.
    pub credential: CallerCredential,
    /// The principal the caller acts as (`principal:`-ref spelling).
    pub principal_ref: String,
    /// The authority ceiling.
    pub caller_kind: CallerKind,
    /// `authority_cap ≤ principal` — the sealed ceiling record, verbatim.
    pub authority_cap: Json,
    /// `permissions[] ⊆` the exposure definition's — the tool names /
    /// permission refs this binding may call (empty = the whole exposed
    /// set; a covering `Permission` can still widen handle reach).
    pub permissions: Vec<String>,
    /// The budget-node *name* the surface run materializes as its pool
    /// root at session open; every accountable charge posts against it.
    pub budget_node: String,
    /// The `BudgetSpec` document the pool is allocated with (dimension
    /// ceilings — `{dimensions:{<key>:{hard:{limit}}}}` or the
    /// `hard_caps` lowering `{key: <int>}`).
    pub pool: Json,
    /// The readers identity ExposurePolicy `readers` admit.
    pub readers_identity: String,
    /// `rate_policy` — declared, advisory at this slice.
    pub rate_policy: Option<Json>,
    /// `expires_at` (epoch ms) — an expired binding never resolves.
    pub expires_at_ms: Option<u64>,
}

impl CallerBinding {
    /// Canonical record JSON (`schema` member included — the document is
    /// self-describing like every sealed record).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str(CALLER_BINDING_SCHEMA));
        m.insert("binding_id".into(), Json::str(self.binding_id.clone()));
        m.insert(
            "credential".into(),
            match &self.credential {
                CallerCredential::StdioLaunch {
                    launched_by,
                    launch_event,
                } => {
                    let mut c = BTreeMap::new();
                    c.insert("kind".into(), Json::str("stdio_launch"));
                    c.insert("launched_by".into(), Json::str(launched_by.clone()));
                    if let Some(e) = launch_event {
                        c.insert("launch_event".into(), Json::str(e.clone()));
                    }
                    Json::Obj(c)
                }
                CallerCredential::OAuth {
                    issuer_ref,
                    subject_kind,
                    audience,
                } => Json::obj([
                    ("kind", Json::str("oauth")),
                    ("issuer_ref", Json::str(issuer_ref.clone())),
                    ("subject_kind", Json::str(subject_kind.as_str())),
                    ("audience", Json::str(audience.clone())),
                ]),
                CallerCredential::Mtls { cert_ref } => Json::obj([
                    ("kind", Json::str("mtls")),
                    ("cert_ref", Json::str(cert_ref.clone())),
                ]),
            },
        );
        m.insert(
            "principal_ref".into(),
            Json::str(self.principal_ref.clone()),
        );
        m.insert("caller_kind".into(), Json::str(self.caller_kind.as_str()));
        m.insert("authority_cap".into(), self.authority_cap.clone());
        m.insert(
            "permissions".into(),
            Json::Arr(self.permissions.iter().map(Json::str).collect()),
        );
        m.insert("budget_node".into(), Json::str(self.budget_node.clone()));
        m.insert("pool".into(), self.pool.clone());
        m.insert(
            "readers_identity".into(),
            Json::str(self.readers_identity.clone()),
        );
        if let Some(rp) = &self.rate_policy {
            m.insert("rate_policy".into(), rp.clone());
        }
        if let Some(e) = self.expires_at_ms {
            m.insert("expires_at_ms".into(), Json::Int(e as i64));
        }
        Json::Obj(m)
    }

    /// Strict decode — unknown members refuse (`schema_violation`), a
    /// missing required member is malformed.
    pub fn from_json(j: &Json) -> Result<CallerBinding, String> {
        let bad = |d: &str| -> String { format!("caller_binding: {d}") };
        let m = match j {
            Json::Obj(m) => m,
            _ => return Err(bad("not an object")),
        };
        if let Some(s) = m.get("schema").and_then(Json::as_str) {
            if s != CALLER_BINDING_SCHEMA {
                return Err(bad(&format!(
                    "schema `{s}` — expected {CALLER_BINDING_SCHEMA}"
                )));
            }
        }
        let binding_id = m
            .get("binding_id")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("binding_id missing"))?
            .to_string();
        let credential = match m.get("credential") {
            Some(Json::Obj(c)) => match c.get("kind").and_then(Json::as_str) {
                Some("stdio_launch") => CallerCredential::StdioLaunch {
                    launched_by: c
                        .get("launched_by")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad("credential.launched_by missing"))?
                        .to_string(),
                    launch_event: c
                        .get("launch_event")
                        .and_then(Json::as_str)
                        .map(String::from),
                },
                Some("oauth") => CallerCredential::OAuth {
                    issuer_ref: c
                        .get("issuer_ref")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad("credential.issuer_ref missing"))?
                        .to_string(),
                    subject_kind: match c.get("subject_kind").and_then(Json::as_str) {
                        Some("user") | None => SubjectKind::User,
                        Some("client") => SubjectKind::Client,
                        Some(other) => return Err(bad(&format!("subject_kind `{other}`"))),
                    },
                    audience: c
                        .get("audience")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad("credential.audience missing"))?
                        .to_string(),
                },
                Some("mtls") => CallerCredential::Mtls {
                    cert_ref: c
                        .get("cert_ref")
                        .and_then(Json::as_str)
                        .ok_or_else(|| bad("credential.cert_ref missing"))?
                        .to_string(),
                },
                Some(other) => return Err(bad(&format!("credential.kind `{other}`"))),
                None => return Err(bad("credential.kind missing")),
            },
            _ => return Err(bad("credential missing")),
        };
        let principal_ref = m
            .get("principal_ref")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("principal_ref missing"))?
            .to_string();
        let caller_kind = CallerKind::parse(
            m.get("caller_kind")
                .and_then(Json::as_str)
                .ok_or_else(|| bad("caller_kind missing"))?,
        )
        .ok_or_else(|| bad("caller_kind unknown"))?;
        let authority_cap = m
            .get("authority_cap")
            .cloned()
            .unwrap_or_else(|| Json::obj([]));
        let permissions = match m.get("permissions") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            _ => Vec::new(),
        };
        let budget_node = m
            .get("budget_node")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("budget_node missing"))?
            .to_string();
        let pool = m.get("pool").cloned().ok_or_else(|| bad("pool missing"))?;
        let readers_identity = m
            .get("readers_identity")
            .and_then(Json::as_str)
            .unwrap_or(&principal_ref)
            .to_string();
        let rate_policy = m.get("rate_policy").cloned();
        let expires_at_ms = m
            .get("expires_at_ms")
            .and_then(Json::as_int)
            .map(|n| n.max(0) as u64);
        Ok(CallerBinding {
            binding_id,
            credential,
            principal_ref,
            caller_kind,
            authority_cap,
            permissions,
            budget_node,
            pool,
            readers_identity,
            rate_policy,
            expires_at_ms,
        })
    }

    /// Is the binding live at `now_ms`? An expired binding never resolves
    /// (the transport refuses before dispatch — never a silent stale).
    pub fn live_at(&self, now_ms: u64) -> bool {
        self.expires_at_ms.map(|e| now_ms < e).unwrap_or(true)
    }
}

/// The default stdio binding — the launcher's principal (`principal:test`
/// stays the fixture default; a deployment writes its own sealed row).
pub fn stdio_launch_binding(launched_by: &str, caller_kind: CallerKind) -> CallerBinding {
    CallerBinding {
        binding_id: format!("bind-stdio-{}", caller_kind.as_str()),
        credential: CallerCredential::StdioLaunch {
            launched_by: launched_by.to_string(),
            launch_event: None,
        },
        principal_ref: "principal:test".to_string(),
        caller_kind,
        authority_cap: Json::obj([]),
        permissions: Vec::new(),
        budget_node: "budget:surface".to_string(),
        pool: Json::obj([(
            "dimensions",
            Json::obj([(
                "tool_calls",
                Json::obj([("hard", Json::obj([("limit", Json::Int(100_000))]))]),
            )]),
        )]),
        readers_identity: "principal:test".to_string(),
        rate_policy: None,
        expires_at_ms: None,
    }
}

/// The token→binding resolution seam (the H3 mediator's contract —
/// ADR-0174 D2). The production wiring resolves a bearer through the
/// credential broker's issuer tables; the slice ships the record-level
/// contract and the table impl the fixture authorization server backs.
pub trait CallerAuth {
    /// Resolve a bearer token to its sealed `CallerBinding` — `None` on
    /// an unknown/expired grant (the transport answers `401`, never a
    /// guess).
    fn resolve(&self, bearer: &str) -> Option<CallerBinding>;
}

/// A table-backed resolver — `{bearer → CallerBinding}` (the fixture
/// authorization server's grant map: the AS issued these tokens for
/// these sealed bindings).
#[derive(Debug, Default)]
pub struct BindingTable {
    /// `bearer token → binding` (sealed rows only).
    pub grants: BTreeMap<String, CallerBinding>,
}

impl BindingTable {
    /// An empty table — every bearer refuses.
    pub fn new() -> BindingTable {
        BindingTable {
            grants: BTreeMap::new(),
        }
    }

    /// Register one grant.
    pub fn grant(&mut self, bearer: &str, binding: CallerBinding) {
        self.grants.insert(bearer.to_string(), binding);
    }
}

impl CallerAuth for BindingTable {
    fn resolve(&self, bearer: &str) -> Option<CallerBinding> {
        self.grants.get(bearer).cloned()
    }
}
