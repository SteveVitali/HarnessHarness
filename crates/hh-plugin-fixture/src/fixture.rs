//! `FixtureLogic` — the S2.2 conformance fixture (§8.4 §3 "Plugin
//! conformance kit"; ADR-0181 D7). One binary plays every role the
//! protocol/isolation/reach suites need:
//!
//! - `null` — the well-behaved stub (one per class; echoes `invoke`,
//!   streams `stream_items` documents, `pass` verdicts, all-pass
//!   conformance).
//! - `hostile` — the probe battery: `violate:*` operations attempt the
//!   AC-4/AC-6 crossings (authority members, peer reads, forged verbs, seq
//!   gaps, oversized frames, `allow` verdicts, ambient env/fs/net/exec/
//!   socket reach).
//! - `guard` — canned verdicts (`--verdict`), including the forged `allow`
//!   (sent at the codec level — the typed sum has no `allow` arm).
//! - `crash` — process death at a named phase (`--crash-at`).
//! - `slow` — per-invocation sleep (`--delay-ms`), for deadline/cancel.

use std::collections::BTreeMap;

use hh_embed_schema::plugin_abi::{
    AbiError, BindFailure, BindParams, ConformanceParams, GuardParams, GuardVerdict, HelloParams,
    InvokeParams, Narrow, StreamParams, TriState,
};
use hh_wire::json::Json;

use crate::{PluginCtx, VariantLogic};

/// The fixture's configured behavior.
#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    /// Well-behaved stub.
    Null,
    /// The probe battery (`violate:*` ops live).
    Hostile,
    /// Canned verdicts.
    Guard,
    /// Die at `--crash-at`.
    Crash,
    /// Sleep `--delay-ms` per invoke/stream item.
    Slow,
}

/// The verdict spelling `--verdict` takes (`allow` is the forged arm —
/// sent through the raw-frame escape, since `GuardVerdict` has no such
/// member by construction).
#[derive(Debug, Clone, PartialEq)]
pub enum FixtureVerdict {
    /// `pass`.
    Pass,
    /// `annotate(text)`.
    Annotate(String),
    /// `narrow{deny|ask|attenuate}`.
    Narrow(Narrow),
    /// `propose_replacement(proposal)`.
    ProposeReplacement(Json),
    /// `no_decision`.
    NoDecision,
    /// The forged `allow` — encoded by hand at send time.
    ForgeAllow,
    /// `authority_widen` — a well-formed verdict annotated with a widening
    /// claim (`{"authority":"kernel"}` inside the verdict payload → the
    /// host's V1 screen catches it).
    Widen,
}

/// The fixture's configuration (parsed from argv by [`FixtureLogic::from_args`]).
pub struct FixtureLogic {
    /// `identity.id()`.
    pub plugin_id: String,
    /// The pin the hello asserts.
    pub version_id: String,
    /// The package content address the hello asserts.
    pub content: String,
    /// `contract_versions_offered` (canonical `ContractRef` JSON).
    pub offers: Vec<Json>,
    /// The capability map (`SUPPORTED|UNSUPPORTED|UNKNOWN`).
    pub capabilities: BTreeMap<String, TriState>,
    /// The mode.
    pub mode: Mode,
    /// `guard`'s canned verdict.
    pub verdict: FixtureVerdict,
    /// `crash`'s phase (`hello|bind|invoke|stream|guard`).
    pub crash_at: String,
    /// `slow`'s per-invocation delay.
    pub delay_ms: u64,
    /// `stream`'s item count.
    pub stream_items: u64,
    /// The class the stub declares (`bind` records it).
    pub class_id: String,
    /// `--fail-bind-at n` — the nth bind fails `not_installed` (AC-7).
    pub fail_bind_at: Option<u64>,
    /// Probe targets the sandbox must refuse (`--probe-*`).
    pub probe_helper_sock: Option<String>,
    /// A peer plugin's package root to try to read.
    pub probe_peer_root: Option<String>,
    /// A peer plugin's ABI socket to try to connect.
    pub probe_peer_socket: Option<String>,
    /// The ABI channel socket this process connected on (for
    /// helper-socket probes — derived from argv).
    pub own_socket: Option<String>,
    /// Bound count.
    binds: u64,
}

impl FixtureLogic {
    /// `argv` → the configured fixture. Flags:
    /// `--socket`, `--plugin-id`, `--version-id`, `--content`,
    /// `--offers <label=ver,…>`, `--cap <name=TRISTATE,…>`, `--mode`,
    /// `--verdict`, `--crash-at`, `--delay-ms`, `--stream-items`,
    /// `--class`, `--fail-bind-at`, `--probe-helper-sock`,
    /// `--probe-peer-root`, `--probe-peer-socket`.
    pub fn from_args(args: &[String]) -> FixtureLogic {
        let mut f = FixtureLogic {
            plugin_id: "local/null-plugin".into(),
            version_id: "pin:null".into(),
            content: "content:null".into(),
            offers: Vec::new(),
            capabilities: BTreeMap::new(),
            mode: Mode::Null,
            verdict: FixtureVerdict::Pass,
            crash_at: String::new(),
            delay_ms: 0,
            stream_items: 3,
            class_id: "control_strategy".into(),
            fail_bind_at: None,
            probe_helper_sock: None,
            probe_peer_root: None,
            probe_peer_socket: None,
            own_socket: None,
            binds: 0,
        };
        let mut i = 0;
        while i < args.len() {
            let val = |i: usize| args.get(i + 1).cloned().unwrap_or_default();
            match args[i].as_str() {
                "--socket" => f.own_socket = Some(val(i)),
                "--plugin-id" => f.plugin_id = val(i),
                "--version-id" => f.version_id = val(i),
                "--content" => f.content = val(i),
                "--offers" => {
                    f.offers = val(i)
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .filter_map(|kv| {
                            let (label, ver) = kv.split_once('=')?;
                            let (kind, id) = label.split_once(':')?;
                            Some(Json::obj([
                                ("kind", Json::str(kind.to_string())),
                                ("id", Json::str(id.to_string())),
                                ("version_range", Json::str(ver.to_string())),
                            ]))
                        })
                        .collect();
                }
                "--cap" => {
                    for kv in val(i).split(',').filter(|s| !s.is_empty()) {
                        if let Some((k, v)) = kv.split_once('=') {
                            if let Some(t) = TriState::parse(v) {
                                f.capabilities.insert(k.to_string(), t);
                            }
                        }
                    }
                }
                "--mode" => {
                    f.mode = match val(i).as_str() {
                        "hostile" => Mode::Hostile,
                        "guard" => Mode::Guard,
                        "crash" => Mode::Crash,
                        "slow" => Mode::Slow,
                        _ => Mode::Null,
                    };
                }
                "--verdict" => {
                    let v = val(i);
                    f.verdict = match v.as_str() {
                        "pass" => FixtureVerdict::Pass,
                        "no_decision" => FixtureVerdict::NoDecision,
                        "allow" => FixtureVerdict::ForgeAllow,
                        "widen" => FixtureVerdict::Widen,
                        "propose_replacement" => FixtureVerdict::ProposeReplacement(Json::obj([(
                            "proposal",
                            Json::str("replacement"),
                        )])),
                        s if s.starts_with("annotate:") => {
                            FixtureVerdict::Annotate(s["annotate:".len()..].to_string())
                        }
                        s if s.starts_with("deny:") => FixtureVerdict::Narrow(Narrow::Deny {
                            reason: s["deny:".len()..].to_string(),
                        }),
                        s if s.starts_with("ask:") => FixtureVerdict::Narrow(Narrow::Ask {
                            reason: s["ask:".len()..].to_string(),
                        }),
                        s if s.starts_with("attenuate:") => {
                            FixtureVerdict::Narrow(Narrow::Attenuate {
                                scope: Json::str(s["attenuate:".len()..].to_string()),
                            })
                        }
                        _ => FixtureVerdict::Pass,
                    };
                }
                "--crash-at" => f.crash_at = val(i),
                "--delay-ms" => {
                    f.delay_ms = val(i).parse().unwrap_or(0);
                }
                "--stream-items" => {
                    f.stream_items = val(i).parse().unwrap_or(3);
                }
                "--class" => f.class_id = val(i),
                "--fail-bind-at" => f.fail_bind_at = Some(val(i).parse().unwrap_or(0)),
                "--probe-helper-sock" => f.probe_helper_sock = Some(val(i)),
                "--probe-peer-root" => f.probe_peer_root = Some(val(i)),
                "--probe-peer-socket" => f.probe_peer_socket = Some(val(i)),
                _ => {}
            }
            i += 2;
        }
        f
    }

    fn maybe_crash(&self, phase: &str) {
        if self.mode == Mode::Crash && self.crash_at == phase {
            // Real-process lane: die for real. In-process lane (no socket —
            // the protocol suite's MemIo threads): panic — the thread's
            // death drops the channel, which the host sees as the same
            // `PluginCrashed` EOF.
            if self.own_socket.is_some() {
                std::process::exit(3);
            }
            panic!("fixture crash at {phase}");
        }
    }

    /// The declaration document `bind`'s record and `conformance` check.
    fn declaration(&self) -> Json {
        Json::obj([
            ("plugin_id", Json::str(self.plugin_id.clone())),
            ("class_id", Json::str(self.class_id.clone())),
            ("deterministic", Json::Bool(true)),
            ("placement", Json::str("subprocess_confined")),
            ("contract_range", Json::str("1.0")),
        ])
    }

    /// One `violate:*` probe — every return is an `invoke_result` payload
    /// reporting what the probe *observed* (the refusal the suite asserts
    /// is the host/sandbox's, never the fixture's word).
    fn violate(&mut self, op: &str, ctx: &mut PluginCtx) -> Result<Vec<Json>, AbiError> {
        match op {
            // ── authority crossings (V1; host screens the message) ──
            "violate:authority" => Ok(vec![Json::obj([
                ("authority", Json::str("kernel")),
                ("origin", Json::str("kernel")),
            ])]),
            "violate:permission" => Ok(vec![Json::obj([(
                "permission",
                Json::obj([("grants", Json::Arr(vec![]))]),
            )])]),
            "violate:risk_class" => Ok(vec![Json::obj([
                ("risk_class", Json::str("low")),
                ("retry_class", Json::str("transient")),
            ])]),
            // ── peer + monitor reads (V1/V3; mediated refusal) ──
            "violate:cross_read" => {
                let r = ctx.read_view("peer_plugin_private", 0);
                Ok(vec![Json::obj([
                    ("probe", Json::str("cross_read")),
                    ("refused", Json::Bool(r.is_err())),
                    (
                        "error",
                        Json::str(r.err().map(|e| e.as_str().to_string()).unwrap_or_default()),
                    ),
                ])])
            }
            "violate:monitor_read" => {
                let r = ctx.read_view("monitor_state", 0);
                Ok(vec![Json::obj([
                    ("probe", Json::str("monitor_read")),
                    ("refused", Json::Bool(r.is_err())),
                ])])
            }
            // ── effect proposals (grant-checked) ──
            "violate:cross_effect" => {
                let r = ctx.propose_effect(Json::obj([
                    ("domain", Json::str("exec")),
                    ("scope", Json::str("*")),
                ]));
                Ok(vec![Json::obj([
                    ("probe", Json::str("cross_effect")),
                    ("refused", Json::Bool(r.is_err())),
                    (
                        "error",
                        Json::str(r.err().map(|e| e.as_str().to_string()).unwrap_or_default()),
                    ),
                ])])
            }
            "violate:effect" => {
                // A proposal inside a *granted* domain — the port still
                // decides (NullPorts → NotGranted).
                let r = ctx.propose_effect(Json::obj([
                    ("domain", Json::str("fs_read")),
                    ("scope", Json::str("*")),
                ]));
                Ok(vec![Json::obj([
                    ("probe", Json::str("effect")),
                    ("refused", Json::Bool(r.is_err())),
                ])])
            }
            // ── protocol-level forgeries (raw-frame escape) ──
            "violate:seq_gap" => {
                // Emit a frame with a jumped seq — the host's channel
                // refuses it (SeqViolation → detached).
                let env = Json::obj([
                    ("protocol_version", Json::Int(1)),
                    ("schema_hash", Json::str("x")),
                    ("seq", Json::Int(1000)),
                    (
                        "payload",
                        Json::obj([
                            ("verb", Json::str("refused")),
                            ("error", Json::str("NotGranted")),
                        ]),
                    ),
                ]);
                let _ = ctx_send_json(ctx, &env);
                Ok(vec![Json::obj([("probe", Json::str("seq_gap"))])])
            }
            "violate:bad_hash" => {
                let env = Json::obj([
                    ("protocol_version", Json::Int(1)),
                    ("schema_hash", Json::str("bogus")),
                    ("seq", Json::Int(ctx_next_tx(ctx))),
                    (
                        "payload",
                        Json::obj([
                            ("verb", Json::str("refused")),
                            ("error", Json::str("NotGranted")),
                        ]),
                    ),
                ]);
                let _ = ctx_send_json(ctx, &env);
                Ok(vec![Json::obj([("probe", Json::str("bad_hash"))])])
            }
            "violate:bad_version" => {
                let env = Json::obj([
                    ("protocol_version", Json::Int(99)),
                    ("schema_hash", Json::str(ctx_schema_hash(ctx))),
                    ("seq", Json::Int(ctx_next_tx(ctx))),
                    (
                        "payload",
                        Json::obj([
                            ("verb", Json::str("refused")),
                            ("error", Json::str("NotGranted")),
                        ]),
                    ),
                ]);
                let _ = ctx_send_json(ctx, &env);
                Ok(vec![Json::obj([("probe", Json::str("bad_version"))])])
            }
            "violate:unknown_verb" => {
                let env = Json::obj([
                    ("protocol_version", Json::Int(1)),
                    ("schema_hash", Json::str(ctx_schema_hash(ctx))),
                    ("seq", Json::Int(ctx_next_tx(ctx))),
                    ("payload", Json::obj([("verb", Json::str("exfiltrate"))])),
                ]);
                let _ = ctx_send_json(ctx, &env);
                Ok(vec![Json::obj([("probe", Json::str("unknown_verb"))])])
            }
            "violate:forged_verb" => {
                // A host→plugin verb from the plugin side (direction
                // violation — the screen's forged-role check).
                let env = Json::obj([
                    ("protocol_version", Json::Int(1)),
                    ("schema_hash", Json::str(ctx_schema_hash(ctx))),
                    ("seq", Json::Int(ctx_next_tx(ctx))),
                    (
                        "payload",
                        Json::obj([
                            ("verb", Json::str("invoke")),
                            ("binding_id", Json::str("x")),
                            ("operation", Json::str("step")),
                            ("inputs", Json::Arr(vec![])),
                            ("reservation", Json::str("r")),
                            ("deadline", Json::Int(0)),
                        ]),
                    ),
                ]);
                let _ = ctx_send_json(ctx, &env);
                Ok(vec![Json::obj([("probe", Json::str("forged_verb"))])])
            }
            "violate:oversize" => {
                let bytes = vec![b'x'; hh_varhost::channel::MAX_FRAME_BYTES + 1];
                let _ = ctx_send_bytes(ctx, &bytes);
                Ok(vec![Json::obj([("probe", Json::str("oversize"))])])
            }
            // ── isolation probes (V3/V4; the sandbox refuses — the
            // fixture only reports what it could or could not do) ──
            "violate:exec" => {
                let ok = std::process::Command::new("/bin/echo")
                    .arg("pwned")
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                Ok(vec![Json::obj([
                    ("probe", Json::str("exec")),
                    ("succeeded", Json::Bool(ok)),
                ])])
            }
            "violate:spawn" => {
                let ok = std::process::Command::new("/bin/true")
                    .spawn()
                    .map(|mut c| c.wait().map(|s| s.success()).unwrap_or(false))
                    .unwrap_or(false);
                Ok(vec![Json::obj([
                    ("probe", Json::str("spawn")),
                    ("succeeded", Json::Bool(ok)),
                ])])
            }
            "violate:net" => {
                let ok = std::net::TcpStream::connect("127.0.0.1:9").is_ok()
                    || std::net::TcpStream::connect("203.0.113.1:80").is_ok();
                Ok(vec![Json::obj([
                    ("probe", Json::str("net")),
                    ("succeeded", Json::Bool(ok)),
                ])])
            }
            "violate:env" => {
                let n = std::env::vars().count();
                let leaked = std::env::var("HH_SECRET_PROBE").ok();
                Ok(vec![Json::obj([
                    ("probe", Json::str("env")),
                    ("env_count", Json::Int(n as i64)),
                    ("leaked", Json::Bool(leaked.is_some())),
                ])])
            }
            "violate:fs" => {
                let ok = std::fs::read_to_string("/etc/passwd")
                    .map(|s| s.contains("root"))
                    .unwrap_or(false);
                Ok(vec![Json::obj([
                    ("probe", Json::str("fs")),
                    ("succeeded", Json::Bool(ok)),
                ])])
            }
            "violate:helper_socket" => {
                let target = self
                    .probe_helper_sock
                    .clone()
                    .unwrap_or_else(|| "../helper.sock".to_string());
                let ok = std::os::unix::net::UnixStream::connect(&target).is_ok();
                Ok(vec![Json::obj([
                    ("probe", Json::str("helper_socket")),
                    ("succeeded", Json::Bool(ok)),
                ])])
            }
            "violate:peer_root" => {
                let target = self.probe_peer_root.clone().unwrap_or_default();
                let ok = !target.is_empty()
                    && std::fs::read_dir(&target)
                        .map(|mut d| d.next().is_some())
                        .unwrap_or(false);
                Ok(vec![Json::obj([
                    ("probe", Json::str("peer_root")),
                    ("succeeded", Json::Bool(ok)),
                ])])
            }
            "violate:peer_socket" => {
                let target = self.probe_peer_socket.clone().unwrap_or_default();
                let ok =
                    !target.is_empty() && std::os::unix::net::UnixStream::connect(&target).is_ok();
                Ok(vec![Json::obj([
                    ("probe", Json::str("peer_socket")),
                    ("succeeded", Json::Bool(ok)),
                ])])
            }
            // ── callback battery (the well-behaved calls the mediation
            // tests assert on) ──
            "probe:read_view" => {
                let r = ctx.read_view("kernel_own", 0);
                Ok(vec![Json::obj([
                    ("probe", Json::str("read_view")),
                    ("ok", Json::Bool(r.is_ok())),
                    (
                        "error",
                        Json::str(r.err().map(|e| e.as_str().to_string()).unwrap_or_default()),
                    ),
                ])])
            }
            "probe:read_principal" => {
                let r = ctx.read_view("principal_view", 0);
                Ok(vec![Json::obj([
                    ("probe", Json::str("read_principal")),
                    ("ok", Json::Bool(r.is_ok())),
                    (
                        "error",
                        Json::str(r.err().map(|e| e.as_str().to_string()).unwrap_or_default()),
                    ),
                ])])
            }
            "probe:budget" => {
                let r = ctx.request_budget(Json::obj([("tokens", Json::Int(10))]));
                Ok(vec![Json::obj([
                    ("probe", Json::str("budget")),
                    ("ok", Json::Bool(r.is_ok())),
                    (
                        "error",
                        Json::str(r.err().map(|e| e.as_str().to_string()).unwrap_or_default()),
                    ),
                ])])
            }
            "probe:diagnostic" => {
                ctx.emit_diagnostic(Json::obj([("note", Json::str("diag"))]));
                Ok(vec![Json::obj([("probe", Json::str("diagnostic"))])])
            }
            _ => Err(AbiError::UnhandledOperation),
        }
    }
}

/// The raw-frame escape — `PluginCtx` deliberately keeps `chan` private;
/// the hostile probes reach it through these crate-internal helpers.
fn ctx_send_json(ctx: &mut PluginCtx, env: &Json) -> Result<(), AbiError> {
    ctx.raw_send_json(env)
}
fn ctx_send_bytes(ctx: &mut PluginCtx, bytes: &[u8]) -> Result<(), AbiError> {
    ctx.raw_send_bytes(bytes)
}
fn ctx_next_tx(ctx: &PluginCtx) -> i64 {
    ctx.next_tx()
}
fn ctx_schema_hash(_ctx: &PluginCtx) -> String {
    hh_embed_schema::plugin_abi::plugin_abi_schema_hash()
}

impl VariantLogic for FixtureLogic {
    fn hello(&self) -> HelloParams {
        if self.mode == Mode::Crash && self.crash_at == "hello" {
            std::process::exit(3);
        }
        HelloParams {
            plugin_abi_version: format!(
                "plugin_abi/{}",
                hh_embed_schema::plugin_abi::PLUGIN_ABI_MAJOR
            ),
            plugin_version_id: self.version_id.clone(),
            plugin_content: self.content.clone(),
            contract_versions_offered: self.offers.clone(),
            capabilities: self.capabilities.clone(),
        }
    }

    fn bind(&mut self, params: &BindParams) -> Result<String, BindFailure> {
        self.maybe_crash("bind");
        self.binds += 1;
        if self.fail_bind_at == Some(self.binds) {
            return Err(BindFailure::NotInstalled);
        }
        if params.contract_version != "1.0" {
            return Err(BindFailure::ContractMismatch);
        }
        Ok(format!("b{}", self.binds))
    }

    fn invoke(
        &mut self,
        params: &InvokeParams,
        ctx: &mut PluginCtx,
    ) -> Result<Vec<Json>, AbiError> {
        self.maybe_crash("invoke");
        if self.delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));
        }
        if params.operation.starts_with("violate:") || params.operation.starts_with("probe:") {
            return self.violate(&params.operation, ctx);
        }
        // The class surface — only declared operations are legal.
        if !matches!(
            params.operation.as_str(),
            "assess" | "scan" | "propose" | "decide" | "compact"
        ) {
            return Err(AbiError::UnhandledOperation);
        }
        // The well-behaved echo — outputs are canonical documents, stamped
        // host-side (V1's stamp is the host's; the fixture claims none).
        Ok(vec![Json::obj([
            ("ok", Json::Bool(true)),
            ("operation", Json::str(params.operation.clone())),
            ("binding", Json::str(params.binding_id.clone())),
        ])])
    }

    fn stream(&mut self, _params: &StreamParams, ctx: &mut PluginCtx) -> Result<(), AbiError> {
        self.maybe_crash("stream");
        for i in 0..self.stream_items {
            if self.delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));
            }
            ctx.stream_item(Json::obj([
                ("item", Json::Int(i as i64)),
                ("of", Json::Int(self.stream_items as i64)),
            ]))?;
        }
        Ok(())
    }

    fn guard(&mut self, _params: &GuardParams, ctx: &mut PluginCtx) -> GuardVerdict {
        self.maybe_crash("guard");
        match &self.verdict {
            FixtureVerdict::ForgeAllow => {
                // Forged at the codec level — `GuardVerdict` has no `allow`
                // arm; send the raw envelope and let the host refuse it.
                let env = Json::obj([
                    ("protocol_version", Json::Int(1)),
                    ("schema_hash", Json::str(ctx_schema_hash(ctx))),
                    ("seq", Json::Int(ctx_next_tx(ctx))),
                    (
                        "payload",
                        Json::obj([
                            ("verb", Json::str("guard_result")),
                            ("verdict", Json::str("allow")),
                        ]),
                    ),
                ]);
                let _ = ctx_send_json(ctx, &env);
                // The real (typed) verdict follows — the host already
                // refused the forged frame; send a parseable answer too so
                // the stream stays aligned.
                GuardVerdict::NoDecision
            }
            FixtureVerdict::Widen => {
                // A well-formed `annotate` carrying a widening claim — the
                // host's V1 payload walk catches `authority: kernel`.
                let env = Json::obj([
                    ("protocol_version", Json::Int(1)),
                    ("schema_hash", Json::str(ctx_schema_hash(ctx))),
                    ("seq", Json::Int(ctx_next_tx(ctx))),
                    (
                        "payload",
                        Json::obj([
                            ("verb", Json::str("guard_result")),
                            ("verdict", Json::str("annotate")),
                            ("text", Json::str("widening")),
                            ("authority", Json::str("kernel")),
                        ]),
                    ),
                ]);
                let _ = ctx_send_json(ctx, &env);
                GuardVerdict::NoDecision
            }
            FixtureVerdict::Pass => GuardVerdict::Pass,
            FixtureVerdict::Annotate(t) => GuardVerdict::Annotate(t.clone()),
            FixtureVerdict::Narrow(n) => GuardVerdict::Narrow(n.clone()),
            FixtureVerdict::ProposeReplacement(p) => GuardVerdict::ProposeReplacement(p.clone()),
            FixtureVerdict::NoDecision => GuardVerdict::NoDecision,
        }
    }

    fn conformance(&mut self, params: &ConformanceParams) -> Json {
        // The class-layer self-report: each requested test id answers
        // against the declaration (the isolation/reach layers are the
        // host's probes, never the plugin's word).
        let decl = self.declaration();
        let tests: Vec<String> = params
            .plugin
            .get("tests")
            .and_then(|t| match t {
                Json::Arr(a) => Some(
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let mut results = Vec::new();
        for t in &tests {
            let pass = match t.split('.').next().unwrap_or("") {
                "static" => decl.get("class_id").is_some() && decl.get("deterministic").is_some(),
                "contract" => decl.get("contract_range").and_then(Json::as_str) == Some("1.0"),
                "executable" => true,
                "property" => {
                    decl.get("placement").and_then(Json::as_str) == Some("subprocess_confined")
                }
                _ => false,
            };
            results.push(Json::obj([
                ("test_id", Json::str(t.clone())),
                ("pass", Json::Bool(pass)),
                (
                    "detail",
                    Json::str(if pass { "ok" } else { "declaration miss" }),
                ),
            ]));
        }
        Json::obj([
            ("suite_id", Json::str(format!("{}.c0", self.class_id))),
            ("results", Json::Arr(results)),
        ])
    }
}
