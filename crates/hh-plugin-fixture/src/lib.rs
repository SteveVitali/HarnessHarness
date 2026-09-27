//! `hh-plugin-fixture` — the plugin-side `plugin_abi/1` runtime plus the
//! S2.2 conformance fixture (§8.4; ADR-0181 D7's kit half).
//!
//! [`PluginRuntime`] is the shared loop every out-of-process variant runs:
//! framed channel → strict decode → dispatch to a [`VariantLogic`] →
//! typed replies. It owns nothing else — the *logic* is the variant.
//!
//! [`FixtureLogic`] is the conformance fixture: `null` (well-behaved),
//! `hostile` (the AC-4/AC-6 probe set — authority crossings, peer reads,
//! forged verbs, seq gaps, oversized frames, `allow` verdicts), `guard`
//! (canned verdicts), `crash` (die at a named phase), `slow` (sleep past
//! deadlines). Modes compose through CLI flags so one binary plays every
//! role the suites need.

use hh_embed_schema::plugin_abi::{
    plugin_abi_schema_hash, AbiError, AbiPayload, BindFailure, BindParams, BindResult,
    CallbackCall, CallbackResult, ConformanceParams, GuardParams, GuardVerdict, HelloParams,
    InvokeParams, InvokeResult, StreamParams, PLUGIN_ABI_MAJOR,
};
use hh_varhost::channel::{AbiChannel, ChannelError, FrameIo};
use hh_wire::json::Json;

pub mod fixture;

/// The callback interface a [`VariantLogic`] gets during `invoke`/`stream`/
/// `guard` — the plugin's only kernel reach (the closed `HostCallback`
/// set; the runtime blocks on the host's `callback_result`).
pub struct PluginCtx<'a> {
    chan: &'a mut AbiChannel,
    schema_hash: &'a str,
}

impl PluginCtx<'_> {
    /// Send a `callback` and wait for its `callback_result` (the host's
    /// answer — a `Refused` maps to the `AbiError` it carries).
    pub fn callback(&mut self, call: CallbackCall) -> Result<CallbackResult, AbiError> {
        self.chan
            .send(self.schema_hash, AbiPayload::Callback(call))
            .map_err(|_| AbiError::PluginCrashed)?;
        loop {
            match self.chan.recv(None) {
                Ok(env) => match env.payload {
                    AbiPayload::CallbackResult(r) => return Ok(r),
                    AbiPayload::Cancel { .. } => return Err(AbiError::SessionDetached),
                    _ => continue,
                },
                Err(_) => return Err(AbiError::PluginCrashed),
            }
        }
    }

    /// `propose_effect(proposal)` — the only way plugin code causes a
    /// world effect.
    pub fn propose_effect(&mut self, proposal: Json) -> Result<String, AbiError> {
        match self.callback(CallbackCall {
            callback: hh_embed_schema::plugin_abi::HostCallback::ProposeEffect,
            args: proposal,
        })? {
            CallbackResult::EffectRef(r) => Ok(r),
            CallbackResult::Refused(e) => Err(e),
            _ => Err(AbiError::SchemaViolation),
        }
    }

    /// `read_view(view_kind, until_seq)`.
    pub fn read_view(&mut self, view_kind: &str, until_seq: i64) -> Result<Json, AbiError> {
        match self.callback(CallbackCall {
            callback: hh_embed_schema::plugin_abi::HostCallback::ReadView,
            args: Json::obj([
                ("view_kind", Json::str(view_kind.to_string())),
                ("until_seq", Json::Int(until_seq)),
            ]),
        })? {
            CallbackResult::Projection(p) => Ok(p),
            CallbackResult::Refused(e) => Err(e),
            _ => Err(AbiError::SchemaViolation),
        }
    }

    /// `request_budget(extent)`.
    pub fn request_budget(&mut self, extent: Json) -> Result<String, AbiError> {
        match self.callback(CallbackCall {
            callback: hh_embed_schema::plugin_abi::HostCallback::RequestBudget,
            args: extent,
        })? {
            CallbackResult::Reservation(r) => Ok(r),
            CallbackResult::Refused(e) => Err(e),
            _ => Err(AbiError::SchemaViolation),
        }
    }

    /// `emit_diagnostic(record)`.
    pub fn emit_diagnostic(&mut self, record: Json) {
        let _ = self.callback(CallbackCall {
            callback: hh_embed_schema::plugin_abi::HostCallback::EmitDiagnostic,
            args: record,
        });
    }

    /// Emit one `stream_item` (the open stream's document).
    pub fn stream_item(&mut self, doc: Json) -> Result<(), AbiError> {
        self.chan
            .send(self.schema_hash, AbiPayload::StreamItem(doc))
            .map(|_| ())
            .map_err(|_| AbiError::PluginCrashed)
    }

    /// The next seq a typed `send` would stamp (the forgery probes use it
    /// to build seq-correct-but-content-hostile envelopes).
    pub fn next_tx(&self) -> i64 {
        self.chan.next_tx()
    }

    /// Send a pre-formed envelope verbatim — the hostile fixture's forgery
    /// knob (bad hash/version, forged verbs, `allow` verdicts, seq gaps).
    /// Nothing about this is well-formedness-checked: the *host's* job is
    /// the refusal, and that is exactly what the suites assert.
    pub fn raw_send_json(&mut self, env: &Json) -> Result<(), AbiError> {
        self.chan
            .send_json(env)
            .map(|_| ())
            .map_err(|_| AbiError::PluginCrashed)
    }

    /// Write raw frame bytes (the oversized-frame probe).
    pub fn raw_send_bytes(&mut self, bytes: &[u8]) -> Result<(), AbiError> {
        self.chan
            .send_frame_bytes(bytes)
            .map_err(|_| AbiError::PluginCrashed)
    }
}

/// The variant's logic — the contract surface the runtime dispatches to.
/// Every method is the ABI verb of the same name; defaults are the
/// well-behaved minimum (`UnhandledOperation`/`no_decision`/empty report)
/// so a fixture overrides only what it probes.
pub trait VariantLogic {
    /// `hello` params — identity, pins, offered contracts, capabilities.
    fn hello(&self) -> HelloParams;

    /// `bind` — returns the binding id or a typed `BindFailure`.
    fn bind(&mut self, params: &BindParams) -> Result<String, BindFailure>;

    /// `invoke` — one class operation.
    fn invoke(&mut self, params: &InvokeParams, ctx: &mut PluginCtx)
        -> Result<Vec<Json>, AbiError>;

    /// `stream` — emit items through `ctx.stream_item`, then the runtime
    /// sends the terminating `invoke_result`.
    fn stream(&mut self, params: &StreamParams, ctx: &mut PluginCtx) -> Result<(), AbiError>;

    /// `guard` — the verdict (the closed sum — there is no `allow` arm to
    /// return; the hostile fixture forges one at the codec level).
    fn guard(&mut self, params: &GuardParams, ctx: &mut PluginCtx) -> GuardVerdict;

    /// `run_conformance` — the report document.
    fn conformance(&mut self, params: &ConformanceParams) -> Json;

    /// `unbind` / `close` hooks (default: no-op).
    fn unbind(&mut self, _binding_id: &str) {}
    /// `close` hook.
    fn on_close(&mut self) {}
}

/// The plugin-side loop — run until `close` or the channel dies.
pub struct PluginRuntime<L: VariantLogic> {
    /// The variant's logic.
    pub logic: L,
    /// The negotiated `hello_ack` record (set after handshake).
    pub negotiated: Option<Json>,
    chan: AbiChannel,
    schema_hash: String,
}

impl<L: VariantLogic> PluginRuntime<L> {
    /// Connect the transport, send `hello`, await `hello_ack`.
    pub fn connect(
        io: Box<dyn FrameIo>,
        logic: L,
        timeout: std::time::Duration,
    ) -> Result<PluginRuntime<L>, ChannelError> {
        let schema_hash = plugin_abi_schema_hash();
        let mut chan = AbiChannel::new(io);
        chan.send(&schema_hash, AbiPayload::Hello(logic.hello()))?;
        // The host's reply — protocol_version/schema_hash are screened by
        // the codec+envelope decode; `hello_ack` is the only legal first
        // inbound message.
        let env = chan.recv(Some(timeout))?;
        let AbiPayload::HelloAck(ack) = env.payload else {
            return Err(ChannelError::Schema(AbiError::SchemaViolation));
        };
        // V5 both ways — the plugin refuses a host outside its declared
        // range (the fixture speaks plugin_abi/1 exactly).
        if ack.plugin_abi_version != format!("plugin_abi/{PLUGIN_ABI_MAJOR}") {
            return Err(ChannelError::Schema(AbiError::ProtocolVersionMismatch));
        }
        let negotiated = ack.to_json_ack();
        Ok(PluginRuntime {
            logic,
            negotiated: Some(negotiated),
            chan,
            schema_hash,
        })
    }

    /// The dispatch loop — runs until `close` or the channel drops.
    pub fn run(&mut self) -> Result<(), ChannelError> {
        loop {
            let env = match self.chan.recv(None) {
                Ok(e) => e,
                Err(ChannelError::Eof) | Err(ChannelError::Io(_)) => return Ok(()),
                Err(e) => return Err(e),
            };
            let mut ctx = PluginCtx {
                chan: &mut self.chan,
                schema_hash: &self.schema_hash,
            };
            match env.payload {
                AbiPayload::Bind(p) => {
                    let r = match self.logic.bind(&p) {
                        Ok(binding_id) => BindResult::Bound { binding_id },
                        Err(f) => BindResult::Failed(f),
                    };
                    ctx.chan.send(ctx.schema_hash, AbiPayload::BindResult(r))?;
                }
                AbiPayload::Invoke(p) => {
                    let r = match self.logic.invoke(&p, &mut ctx) {
                        Ok(outputs) => InvokeResult::Outputs(outputs),
                        Err(e) => InvokeResult::Failed(e),
                    };
                    ctx.chan
                        .send(ctx.schema_hash, AbiPayload::InvokeResult(r))?;
                }
                AbiPayload::Stream(p) => {
                    let r = match self.logic.stream(&p, &mut ctx) {
                        Ok(()) => InvokeResult::Outputs(vec![]),
                        Err(e) => InvokeResult::Failed(e),
                    };
                    ctx.chan
                        .send(ctx.schema_hash, AbiPayload::InvokeResult(r))?;
                }
                AbiPayload::Cancel { .. } => {
                    // Cooperative cancel — the fixture checks nothing; the
                    // host's deadline is authoritative.
                }
                AbiPayload::Unbind { binding_id } => {
                    self.logic.unbind(&binding_id);
                }
                AbiPayload::Close { .. } => {
                    self.logic.on_close();
                    return Ok(());
                }
                AbiPayload::GuardInvoke(p) => {
                    let v = self.logic.guard(&p, &mut ctx);
                    ctx.chan.send(ctx.schema_hash, AbiPayload::GuardResult(v))?;
                }
                AbiPayload::RunConformance(p) => {
                    let report = self.logic.conformance(&p);
                    ctx.chan
                        .send(ctx.schema_hash, AbiPayload::ConformanceResult { report })?;
                }
                _ => {
                    // A plugin-bound verb that isn't ours (hello_ack
                    // mid-session, another callback_result) — refused.
                    ctx.chan.send(
                        ctx.schema_hash,
                        AbiPayload::Refused(AbiError::SchemaViolation),
                    )?;
                }
            }
        }
    }
}

/// `HelloResult`'s canonical record form (the negotiated session doc the
/// fixture reports in `declare`).
trait AckJson {
    fn to_json_ack(&self) -> Json;
}
impl AckJson for hh_embed_schema::plugin_abi::HelloResult {
    fn to_json_ack(&self) -> Json {
        Json::obj([
            ("host_id", Json::str(self.host_id.clone())),
            (
                "plugin_abi_version",
                Json::str(self.plugin_abi_version.clone()),
            ),
            (
                "registry_snapshot_id",
                Json::str(self.registry_snapshot_id.clone()),
            ),
        ])
    }
}
