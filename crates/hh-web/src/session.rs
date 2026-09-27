//! The surface session manager (ADR-0302 D3) — the surface's own
//! kernel-side handles. The browser names a *run*, never a session; the
//! surface lazily opens an **attach** session per inspected run
//! (`session_ref.attach` — read-only, gates-blocked) and a **resume**
//! writer session only when a declared write needs it. Every session
//! carries the `client` declaration (`kind: web` + the declared
//! `SinkPolicy` — P8; the kernel appends the session rows that name the
//! surface binding + policy — `lifecycle.session.{created,resumed,
//! attached}`) and the surface mints `idempotency_key`s + `responder`
//! provenance on every write (P12).
//!
//! The manager does not persist anything — sessions live for the process
//! life (the durable record is the kernel's).

use std::collections::BTreeMap;

use hh_embed_client_generated::{ClientError, NetClient};
use hh_telemetry::sinks::SinkPolicy;
use hh_wire::json::Json;

use crate::ops;

/// The `client.kind` the surface declares — the closed `web` spelling.
pub const CLIENT_KIND: &str = "web";

/// The surface's session handles + the kernel connection.
pub struct Sessions {
    /// The binding (c) client — one TCP connection to `hh serve --http`.
    pub client: NetClient,
    /// `run_id → attach session_id` (read-only).
    attach: BTreeMap<String, String>,
    /// `run_id → writer session_id` (resume; only when a write needs it).
    writer: BTreeMap<String, String>,
    /// The declared SinkPolicy (P8) — sent as `client.sink` on every
    /// `open_session`, enforced again surface-side by [`crate::sink`].
    pub sink: SinkPolicy,
    /// The principal the writes attribute — `responder.subject_ref`.
    pub subject_ref: String,
    /// The request-id counter — idempotency keys are
    /// `web:<monotonic>` and every write carries one (P12's
    /// "a request id" is the resubmit dedup key).
    request_seq: u64,
    /// The `hello` handshake flag — the kernel refuses every op until
    /// `hello` negotiates identity; the surface greets lazily on its
    /// first call (`kind:"web"` — the same identity the session rows
    /// carry, one spelling, CC1).
    greeted: bool,
}

impl Sessions {
    pub fn new(client: NetClient, sink: SinkPolicy, subject_ref: impl Into<String>) -> Sessions {
        Sessions {
            client,
            attach: BTreeMap::new(),
            writer: BTreeMap::new(),
            sink,
            subject_ref: subject_ref.into(),
            request_seq: 0,
            greeted: false,
        }
    }

    /// The `hello` handshake — once, before any op; the typed refusal
    /// (`SchemaMismatch`, `KernelBelowFloor`) propagates verbatim.
    fn ensure_hello(&mut self) -> Result<(), ClientError> {
        if self.greeted {
            return Ok(());
        }
        let r = self.client.call(
            "hello",
            Json::obj([
                ("contract_major", Json::Int(1)),
                (
                    "client",
                    Json::obj([
                        ("name", Json::str("hh-web")),
                        ("version", Json::str("1")),
                        ("kind", Json::str(CLIENT_KIND)),
                    ]),
                ),
                (
                    "capabilities",
                    Json::obj([
                        ("accepts_ephemeral_frames", Json::Bool(false)),
                        // The C1 read catalogue reaches experimental-tier
                        // reads (`list_leases`, `lab.*` envelopes) — the
                        // opt-in is declared at handshake, never assumed.
                        ("experimental", Json::Bool(true)),
                    ]),
                ),
            ]),
        )?;
        let _ = r;
        self.greeted = true;
        Ok(())
    }

    /// A raw session-free kernel call (`run_index`, `lab.*`,
    /// `kernel.*`) — the result is the canonical op result verbatim.
    pub fn call(&mut self, op: &str, params: Json) -> Result<Json, ClientError> {
        self.ensure_hello()?;
        self.client.call(op, params)
    }

    /// The `client` declaration for `open_session` — `kind: web`, the
    /// surface's own `surface_ref`, and the declared SinkPolicy (P8).
    fn client_decl(&self) -> Json {
        Json::obj([
            ("kind", Json::str(CLIENT_KIND)),
            ("surface_ref", Json::str(self.client.authority())),
            ("sink", self.sink.to_json()),
            (
                "ui_caps",
                Json::Arr(vec![Json::str("read_only"), Json::str("declared_writes")]),
            ),
        ])
    }

    /// The attach (read-only) session for `run_id` — opened lazily,
    /// cached for the process life.
    pub fn attach_for(&mut self, run_id: &str) -> Result<String, ClientError> {
        if let Some(s) = self.attach.get(run_id) {
            return Ok(s.clone());
        }
        self.request_seq += 1;
        let r = self.call(
            "open_session",
            Json::obj([
                (
                    "spec",
                    Json::obj([
                        ("kind", Json::str("attach")),
                        ("run_id", Json::str(run_id)),
                        ("read_only", Json::Bool(true)),
                    ]),
                ),
                (
                    "idempotency_key",
                    Json::str(format!("web:open:{}", self.request_seq)),
                ),
                ("client", self.client_decl()),
            ]),
        )?;
        let sid = obj_str(&r, "session_id").unwrap_or_default().to_string();
        self.attach.insert(run_id.into(), sid.clone());
        Ok(sid)
    }

    /// The writer (resume) session for `run_id` — opened only when a
    /// declared write needs it (P12: the surface opens; a second writer
    /// gets the kernel's `Held` refusal, which passes through verbatim).
    pub fn writer_for(&mut self, run_id: &str) -> Result<String, ClientError> {
        if let Some(s) = self.writer.get(run_id) {
            return Ok(s.clone());
        }
        self.request_seq += 1;
        let r = self.call(
            "open_session",
            Json::obj([
                (
                    "spec",
                    Json::obj([
                        ("kind", Json::str("resume")),
                        ("run_id", Json::str(run_id)),
                        ("mode", Json::str("continue")),
                    ]),
                ),
                (
                    "idempotency_key",
                    Json::str(format!("web:open:{}", self.request_seq)),
                ),
                ("client", self.client_decl()),
            ]),
        )?;
        let sid = obj_str(&r, "session_id").unwrap_or_default().to_string();
        self.writer.insert(run_id.into(), sid.clone());
        Ok(sid)
    }

    /// The forwarded browser call — injects `session_id` (attach for
    /// reads, writer for the session-scoped writes), `responder` +
    /// `idempotency_key` on `respond_permission`/`amend`, and the
    /// invocation envelope fields the schema takes
    /// (`invocation.surface`). Browser params may not override any of
    /// the injected members (P12 — the surface owns provenance).
    pub fn call_for_run(
        &mut self,
        run_id: &str,
        op: &str,
        params: Json,
    ) -> Result<Json, ClientError> {
        let class = ops::classify(op).unwrap_or(ops::OpClass::Read);
        let mut m = match &params {
            Json::Obj(m) => m.clone(),
            _ => BTreeMap::new(),
        };
        // The browser never supplies the session/provenance members.
        m.remove("session_id");
        m.remove("responder");
        m.remove("idempotency_key");
        if ops::session_scoped(op) {
            let sid = match class {
                ops::OpClass::Read => self.attach_for(run_id)?,
                ops::OpClass::Write => self.writer_for(run_id)?,
            };
            m.insert("session_id".into(), Json::str(&sid));
            if class == ops::OpClass::Write {
                self.request_seq += 1;
                m.insert(
                    "responder".into(),
                    Json::obj([
                        ("subject_ref", Json::str(&self.subject_ref)),
                        ("surface_session_ref", Json::str(&sid)),
                        ("surface_ref", Json::str(self.client.authority())),
                    ]),
                );
                m.insert(
                    "idempotency_key".into(),
                    Json::str(format!("web:{}", self.request_seq)),
                );
            }
        }
        self.call(op, Json::Obj(m))
    }
}

fn obj_str<'a>(j: &'a Json, k: &str) -> Option<&'a str> {
    match j {
        Json::Obj(m) => match m.get(k) {
            Some(Json::Str(s)) => Some(s.as_str()),
            _ => None,
        },
        _ => None,
    }
}
