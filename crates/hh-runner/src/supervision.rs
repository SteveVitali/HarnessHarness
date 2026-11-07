//! `supervision` — the runner's process-boundary supervision of the
//! model proxy and the egress mediator (manifest 269/R6.16 Stage 2;
//! RUNNER-SUPERVISION; spec §4.1 REQ-XC-101, §4.3 REQ-XC-109, §4.4,
//! §4.9/§4.10 REQ-XH-103, §10.1 REQ-XH-120; design/31 F4/F18;
//! design/33 §1.1/§1.4).
//!
//! Two separate sole-writer processes: `hh-model-proxy` writes its
//! phase proxy store root (the runner never writes it — reads are the
//! cross-process fold), `hh_egress_mediator` writes its phase egress
//! root. The runner drives the proxy over the Json stdio control
//! channel — `configure` first, then `register_slot` (a fresh slot
//! token at slot open, REQ-XC-109's per-slot routing credential),
//! `revoke_slot` at every close (`security.credential.revoked`; X-05 —
//! no token outlives its slot), `catalog_boundary` at each declared
//! block boundary (REQ-XC-119/REQ-XD-111), `status`, `stop`.
//!
//! The carried root pool is mechanical (X-02): the runner folds each
//! declared prior window's ledger **read-only** (`verify` + Σ
//! `measurement.cost.attributed` micro-units + `proxy_aborted` orphans
//! at bounded-hi — the `harvest_window_spend` fold reimplemented since
//! `hh-hosting` is an unreachable dependency for the runner, CC1/AR-08)
//! and passes the folded rows to `configure.authorization.prior_windows`.
//! An absent run is an `unharvested ledger` refusal; a `verify` failure
//! is `unverified ledger` — never an estimate.
//!
//! The upstream Ollama credential is never projected into a cell (the
//! DF-R6.7-1 cell-environment leg): the packet names `credential_env`
//! — the variable the *proxy process* resolves — while the cell env
//! projection (`hh_env::egress_allowlist::cell_env_projection`) carries
//! only the slot token and loopback `no_proxy`.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use hharness_ledger::store::Store;
use hharness_wire::json::Json;

use crate::json_ext::JsonExt;

/// The supervision error — a typed refusal string carried as
/// `RunnerError::Io` at the drive boundary.
#[derive(Debug)]
pub struct SupError {
    /// What refused or failed.
    pub detail: String,
}

fn err(detail: impl Into<String>) -> SupError {
    SupError {
        detail: detail.into(),
    }
}

/// Env override for the proxy binary (the declared `proxy.bin` member
/// wins; this is the host-tool seam — never a PATH guess).
pub const PROXY_BIN_ENV: &str = "HH_MODEL_PROXY_BIN";
/// Env override for the mediator binary.
pub const MEDIATOR_BIN_ENV: &str = "HH_EGRESS_MEDIATOR_BIN";
/// The proxy binary's own name (sibling-of-`current_exe` fallback).
pub const PROXY_BIN_NAME: &str = "hh-model-proxy";
/// The mediator binary's own name.
pub const MEDIATOR_BIN_NAME: &str = "hh-egress-mediator";

/// `resolve_bin(declared, env_var, name)` — declared path → env
/// override → sibling of `current_exe`'s parent dir (`target/debug/` —
/// the dev-layout home the sibling binaries land in).
pub fn resolve_bin(declared: Option<&str>, env_var: &str, name: &str) -> Result<PathBuf, SupError> {
    if let Some(d) = declared {
        return Ok(PathBuf::from(d));
    }
    if let Ok(e) = std::env::var(env_var) {
        return Ok(PathBuf::from(e));
    }
    let exe = std::env::current_exe().map_err(|e| err(format!("current_exe: {e}")))?;
    Ok(exe
        .parent()
        .and_then(|d| d.parent())
        .unwrap_or_else(|| exe.parent().unwrap_or(exe.as_path()))
        .join(name))
}

// ── the proxy's Json control channel ───────────────────────────────────

/// `ProxyCtl` — the runner's handle to a spawned `hh-model-proxy`:
/// `configure` was the first op and the reply's `listen` is bound; ops
/// are one Json line in, one `{ok|error}` line out (the binary is the
/// only speaker of its store root).
pub struct ProxyCtl {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// The bound listen address from the configure reply.
    pub listen: String,
    /// The declared store root the proxy sole-writes.
    pub store_root: PathBuf,
    /// The declared run id inside that root.
    pub run_id: String,
}

impl ProxyCtl {
    /// Spawn `bin` with piped stdio, write the `configure` line, and
    /// read the `{ok:{listen}}` handshake. `child_env` overrides the
    /// inherited environment member-by-member (the packet's declared
    /// `env` member — fixture credentials never reach a record).
    pub fn spawn(
        bin: &Path,
        configure: &Json,
        child_env: &BTreeMap<String, String>,
    ) -> Result<ProxyCtl, SupError> {
        let mut child = Command::new(bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .envs(child_env)
            .spawn()
            .map_err(|e| err(format!("proxy spawn {}: {e}", bin.display())))?;
        let mut stdin = child.stdin.take().ok_or_else(|| err("proxy stdin"))?;
        let stdout = BufReader::new(child.stdout.take().ok_or_else(|| err("proxy stdout"))?);
        let mut line = configure.to_canonical_string();
        line.push('\n');
        stdin
            .write_all(line.as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|e| err(format!("proxy configure write: {e}")))?;
        let mut ctl = ProxyCtl {
            child,
            stdin,
            stdout,
            listen: String::new(),
            store_root: PathBuf::new(),
            run_id: String::new(),
        };
        let reply = ctl.read_reply()?;
        let ok = reply
            .get("ok")
            .ok_or_else(|| err(format!("configure refused: {}", err_text(&reply))))?;
        ctl.listen = ok
            .get("listen")
            .and_then(Json::as_str)
            .map(str::to_string)
            .ok_or_else(|| err("configure reply lacks ok.listen"))?;
        Ok(ctl)
    }

    fn read_reply(&mut self) -> Result<Json, SupError> {
        let mut line = String::new();
        let n = self
            .stdout
            .read_line(&mut line)
            .map_err(|e| err(format!("proxy reply read: {e}")))?;
        if n == 0 {
            return Err(err("proxy control channel closed"));
        }
        hharness_wire::json::parse(line.trim()).map_err(|e| err(format!("proxy reply parse: {e}")))
    }

    /// `op(doc)` — one control op; the reply's `error` member is the
    /// typed refusal, `ok` is returned.
    pub fn op(&mut self, doc: &Json) -> Result<Json, SupError> {
        let mut line = doc.to_canonical_string();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.flush())
            .map_err(|e| err(format!("proxy op write: {e}")))?;
        let reply = self.read_reply()?;
        if reply.get("error").is_some() {
            return Err(err(format!("op refused: {}", err_text(&reply))));
        }
        Ok(reply.get("ok").cloned().unwrap_or_else(|| Json::obj([])))
    }

    /// `register_slot` → the fresh slot token (minted per attempt;
    /// `security.credential.bound` lands in the proxy's ledger).
    pub fn register_slot(
        &mut self,
        slot_id: &str,
        effort: &str,
        slice_micros: i64,
    ) -> Result<String, SupError> {
        let ok = self.op(&Json::obj([
            ("op", Json::str("register_slot")),
            ("slot_id", Json::str(slot_id)),
            ("effort", Json::str(effort)),
            ("slice_micros", Json::Int(slice_micros)),
        ]))?;
        ok.get("token")
            .and_then(Json::as_str)
            .map(str::to_string)
            .ok_or_else(|| err("register_slot reply lacks ok.token"))
    }

    /// `revoke_slot` — the token dies (`security.credential.revoked`
    /// lands; the slice's remainder returns to the pool).
    pub fn revoke_slot(&mut self, slot_id: &str) -> Result<(), SupError> {
        self.op(&Json::obj([
            ("op", Json::str("revoke_slot")),
            ("slot_id", Json::str(slot_id)),
        ]))?;
        Ok(())
    }

    /// `catalog_boundary` — the §4.6 re-probe the runner issues at each
    /// declared block boundary (REQ-XC-119).
    pub fn catalog_boundary(&mut self, label: &str) -> Result<(), SupError> {
        self.op(&Json::obj([
            ("op", Json::str("catalog_boundary")),
            ("label", Json::str(label)),
        ]))?;
        Ok(())
    }

    /// `status` — the run id / phase-stop / slot table reply.
    pub fn status(&mut self) -> Result<Json, SupError> {
        self.op(&Json::obj([("op", Json::str("status"))]))
    }

    /// `stop` — the catalog end-probe then process exit.
    pub fn stop(mut self) {
        let _ = self.op(&Json::obj([("op", Json::str("stop"))]));
        let _ = self.child.wait();
    }
}

fn err_text(reply: &Json) -> String {
    reply
        .get("error")
        .and_then(|e| e.get("detail"))
        .and_then(Json::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| reply.to_canonical_string())
}

// ── the egress mediator process ────────────────────────────────────────

/// `MediatorProc` — the spawned `hh_egress_mediator`: sole writer of
/// its phase egress root, serving the declared egress socket; its
/// `ready socket=.. run=..` first line is the supervision handshake.
pub struct MediatorProc {
    child: Child,
    /// The socket path the mediator bound (from the ready line).
    pub socket: PathBuf,
    /// The run id the mediator reported.
    pub run_id: String,
    /// The declared store root the mediator sole-writes.
    pub store_root: PathBuf,
}

impl MediatorProc {
    /// Spawn `bin` with the declared argv; read the `ready` line.
    pub fn spawn(bin: &Path, args: &[String]) -> Result<MediatorProc, SupError> {
        let mut child = Command::new(bin)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| err(format!("mediator spawn {}: {e}", bin.display())))?;
        let mut out = BufReader::new(child.stdout.take().ok_or_else(|| err("mediator stdout"))?);
        let mut line = String::new();
        let n = out
            .read_line(&mut line)
            .map_err(|e| err(format!("mediator ready read: {e}")))?;
        if n == 0 || !line.starts_with("ready ") {
            return Err(err(format!("mediator ready handshake failed: {line:?}")));
        }
        let mut socket = PathBuf::new();
        let mut run_id = String::new();
        let mut store_root = PathBuf::new();
        for part in line.split_whitespace().skip(1) {
            if let Some((k, v)) = part.split_once('=') {
                match k {
                    "socket" => socket = PathBuf::from(v),
                    "run" => run_id = v.to_string(),
                    "root" => store_root = PathBuf::from(v),
                    _ => {}
                }
            }
        }
        // Keep the reader alive on a thread so the child's stdout never
        // blocks the accept loop.
        std::thread::spawn(move || {
            let mut sink = String::new();
            while out.read_line(&mut sink).is_ok_and(|n| n > 0) {
                sink.clear();
            }
        });
        Ok(MediatorProc {
            child,
            socket,
            run_id,
            store_root,
        })
    }

    /// Kill and reap the mediator (SIGKILL — the window end never waits
    /// on it; its ledger rows are already durable-before-visible).
    pub fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ── the carry-in fold (X-02; harvest_window_spend reimplemented) ───────

/// One folded prior window — the `prior_windows[]` row the proxy's
/// `configure.authorization` decodes.
#[derive(Debug, Clone, PartialEq)]
pub struct PriorSpend {
    /// The window id (= its proxy ledger's run id).
    pub window_id: String,
    /// Σ `measurement.cost.attributed` micro-units.
    pub consumed_micros: i64,
    /// `model.call.failed{proxy_aborted}` terminal count.
    pub orphan_calls: u64,
    /// The orphans' `bounded{lo:0,hi}` charges at hi.
    pub orphan_bounded_hi_micros: i64,
}

impl PriorSpend {
    /// The `prior_windows[]` member spelling (`harvested`/`verified`
    /// ride the fold — a row only reaches the doc when both hold).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("window_id", Json::str(&self.window_id)),
            ("consumed_micros", Json::Int(self.consumed_micros)),
            ("orphan_calls", Json::Int(self.orphan_calls as i64)),
            (
                "orphan_bounded_hi_micros",
                Json::Int(self.orphan_bounded_hi_micros),
            ),
            ("harvested", Json::Bool(true)),
            ("verified", Json::Bool(true)),
        ])
    }
}

/// `fold_prior_window(store_root, run_id) → PriorSpend` — the
/// mechanical carry-in (X-02, REQ-XH-120): open the prior proxy ledger
/// read-only, refuse `unharvested ledger` when the run is absent and
/// `unverified ledger` when the byte-exact WAL replay fails; fold Σ
/// `measurement.cost.attributed` micro-units with `proxy_aborted`
/// terminals charged at bounded-hi. No estimate ever substitutes.
pub fn fold_prior_window(store_root: &Path, run_id: &str) -> Result<PriorSpend, SupError> {
    let store = Store::open(store_root).map_err(|e| err(format!("prior ledger open: {e}")))?;
    if !store.has_run(run_id) {
        return Err(err(format!("unharvested ledger: no run {run_id}")));
    }
    store
        .verify(run_id)
        .map_err(|e| err(format!("unverified ledger {run_id}: {e}")))?;
    let envs = store
        .envelopes(run_id)
        .map_err(|e| err(format!("prior ledger read: {e}")))?;
    let aborted: std::collections::BTreeSet<String> = envs
        .iter()
        .filter(|e| {
            e.class == "model.call.failed"
                && e.payload.get("disposition").and_then(Json::as_str) == Some("proxy_aborted")
        })
        .map(|e| e.event_id.clone())
        .collect();
    let mut consumed = 0i64;
    let mut orphan_hi = 0i64;
    for e in envs {
        if e.class != "measurement.cost.attributed" {
            continue;
        }
        let micros = e
            .payload
            .get("money")
            .and_then(|m| m.get("micro_units"))
            .and_then(Json::as_int)
            .unwrap_or(0);
        consumed += micros;
        if aborted.contains(
            e.payload
                .get("source_event")
                .and_then(|s| s.get("event_id"))
                .and_then(Json::as_str)
                .unwrap_or(""),
        ) {
            orphan_hi += micros;
        }
    }
    Ok(PriorSpend {
        window_id: run_id.to_string(),
        consumed_micros: consumed,
        orphan_calls: aborted.len() as u64,
        orphan_bounded_hi_micros: orphan_hi,
    })
}

// ── the declared supervision member ────────────────────────────────────

/// A declared prior-window ledger ref `{store_root, run_id}`.
#[derive(Debug, Clone, PartialEq)]
pub struct PriorLedgerDecl {
    /// The proxy store root.
    pub store_root: String,
    /// The run id inside it.
    pub run_id: String,
}

/// The declared proxy member (`supervision.proxy`).
#[derive(Debug, Clone, PartialEq)]
pub struct ProxyDecl {
    /// The binary path (else `HH_MODEL_PROXY_BIN` → sibling fallback).
    pub bin: Option<String>,
    /// The phase proxy store root the proxy sole-writes.
    pub store_root: String,
    /// The proxy run id.
    pub run_id: String,
    /// The listen binding (`127.0.0.1:0` default).
    pub listen: String,
    /// The verbatim `lane` member (lane_id/endpoint_ref/upstream_host/
    /// model/reservation as the binary decodes).
    pub lane: Json,
    /// The upstream credential's env-var *name* — the proxy resolves
    /// it; the value never enters a record or a cell.
    pub credential_env: Option<String>,
    /// The verbatim `authorization` members (`verbatim`,
    /// `gate_record_ref`, `authorized_micros`,
    /// `available_credit_micros`, `m_ppm`) — `prior_windows` is the
    /// runner's fold, never declared.
    pub authorization: Json,
    /// The prior-window ledger refs to fold (X-02).
    pub prior_ledgers: Vec<PriorLedgerDecl>,
    /// The per-slice budget (micro-units) registered per slot.
    pub slice_micros: i64,
    /// The slot effort label.
    pub effort: String,
    /// Extra child-environment pairs (fixture seam — the credential
    /// env *value* lives here when a fixture needs it; production
    /// inherits the runner's own env).
    pub child_env: BTreeMap<String, String>,
}

/// The declared mediator member (`supervision.mediator`).
#[derive(Debug, Clone, PartialEq)]
pub struct MediatorDecl {
    /// The binary path (else `HH_EGRESS_MEDIATOR_BIN` → sibling).
    pub bin: Option<String>,
    /// The phase egress store root the mediator sole-writes.
    pub store_root: String,
    /// The Unix socket path the mediator serves.
    pub socket: String,
    /// The egress policy document path.
    pub policy: String,
    /// The declared env handle.
    pub env_handle: String,
    /// The declared effect id.
    pub effect_id: String,
    /// The declared participant id.
    pub participant: String,
    /// The optional run id.
    pub run_id: Option<String>,
    /// The optional seed.
    pub seed: Option<i64>,
    /// Optional `host=ip` resolver pins.
    pub resolve: Vec<String>,
}

/// `supervision{proxy, mediator?, phase_schedule}` — the declared
/// process-boundary members (drive packet `supervision`).
#[derive(Debug, Clone, PartialEq)]
pub struct SupervisionDecl {
    /// The model proxy.
    pub proxy: ProxyDecl,
    /// The egress mediator (absent = the window runs unsupervised by a
    /// mediator — the socket grants still record).
    pub mediator: Option<MediatorDecl>,
    /// The sealed `phase_schedule` — verbatim, the per-phase socket
    /// grant source (`{setup,agent,verify}.{layer.net.unix_sockets.allow}`).
    pub phase_schedule: Json,
}

impl SupervisionDecl {
    /// Strict decode of the drive packet's `supervision` member.
    pub fn from_json(j: &Json) -> Result<SupervisionDecl, SupError> {
        let m = j
            .as_obj()
            .ok_or_else(|| err("supervision must be an object"))?;
        for k in m.keys() {
            if !["proxy", "mediator", "phase_schedule"].contains(&k.as_str()) {
                return Err(err(format!("supervision: unknown member {k}")));
            }
        }
        let pm = m
            .get("proxy")
            .and_then(JsonExt::as_obj)
            .ok_or_else(|| err("supervision.proxy required"))?;
        for k in pm.keys() {
            if ![
                "bin",
                "store_root",
                "run_id",
                "listen",
                "lane",
                "credential_env",
                "authorization",
                "prior_ledgers",
                "slice_micros",
                "effort",
                "child_env",
            ]
            .contains(&k.as_str())
            {
                return Err(err(format!("supervision.proxy: unknown member {k}")));
            }
        }
        let prior_ledgers = pm
            .get("prior_ledgers")
            .and_then(JsonExt::as_arr)
            .map(|ws| {
                ws.iter()
                    .map(|w| {
                        let wm = w
                            .as_obj()
                            .ok_or_else(|| err("prior_ledgers[] must be objects"))?;
                        for k in wm.keys() {
                            if !["store_root", "run_id"].contains(&k.as_str()) {
                                return Err(err(format!("prior_ledgers[]: unknown member {k}")));
                            }
                        }
                        Ok(PriorLedgerDecl {
                            store_root: wm
                                .get("store_root")
                                .and_then(Json::as_str)
                                .map(str::to_string)
                                .ok_or_else(|| err("prior_ledgers[].store_root required"))?,
                            run_id: wm
                                .get("run_id")
                                .and_then(Json::as_str)
                                .map(str::to_string)
                                .ok_or_else(|| err("prior_ledgers[].run_id required"))?,
                        })
                    })
                    .collect::<Result<Vec<_>, SupError>>()
            })
            .transpose()?
            .unwrap_or_default();
        let proxy = ProxyDecl {
            bin: pm.get("bin").and_then(Json::as_str).map(str::to_string),
            store_root: pm
                .get("store_root")
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| err("supervision.proxy.store_root required"))?,
            run_id: pm
                .get("run_id")
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| err("supervision.proxy.run_id required"))?,
            listen: pm
                .get("listen")
                .and_then(Json::as_str)
                .unwrap_or("127.0.0.1:0")
                .to_string(),
            lane: pm
                .get("lane")
                .cloned()
                .ok_or_else(|| err("supervision.proxy.lane required"))?,
            credential_env: pm
                .get("credential_env")
                .and_then(Json::as_str)
                .map(str::to_string),
            authorization: pm
                .get("authorization")
                .cloned()
                .ok_or_else(|| err("supervision.proxy.authorization required"))?,
            prior_ledgers,
            slice_micros: pm
                .get("slice_micros")
                .and_then(Json::as_int)
                .ok_or_else(|| err("supervision.proxy.slice_micros required"))?,
            effort: pm
                .get("effort")
                .and_then(Json::as_str)
                .unwrap_or("high")
                .to_string(),
            child_env: pm
                .get("child_env")
                .and_then(JsonExt::as_obj)
                .map(|o| {
                    o.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default(),
        };
        let mediator = m
            .get("mediator")
            .map(|mj| -> Result<MediatorDecl, SupError> {
                let mm = mj
                    .as_obj()
                    .ok_or_else(|| err("supervision.mediator must be an object"))?;
                for k in mm.keys() {
                    if ![
                        "bin",
                        "store_root",
                        "socket",
                        "policy",
                        "env_handle",
                        "effect_id",
                        "participant",
                        "run_id",
                        "seed",
                        "resolve",
                    ]
                    .contains(&k.as_str())
                    {
                        return Err(err(format!("supervision.mediator: unknown member {k}")));
                    }
                }
                let req = |k: &str| -> Result<String, SupError> {
                    mm.get(k)
                        .and_then(Json::as_str)
                        .map(str::to_string)
                        .ok_or_else(|| err(format!("supervision.mediator.{k} required")))
                };
                Ok(MediatorDecl {
                    bin: mm.get("bin").and_then(Json::as_str).map(str::to_string),
                    store_root: req("store_root")?,
                    socket: req("socket")?,
                    policy: req("policy")?,
                    env_handle: req("env_handle")?,
                    effect_id: req("effect_id")?,
                    participant: req("participant")?,
                    run_id: mm.get("run_id").and_then(Json::as_str).map(str::to_string),
                    seed: mm.get("seed").and_then(Json::as_int),
                    resolve: mm
                        .get("resolve")
                        .and_then(JsonExt::as_arr)
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default(),
                })
            })
            .transpose()?;
        let phase_schedule = m
            .get("phase_schedule")
            .cloned()
            .ok_or_else(|| err("supervision.phase_schedule required"))?;
        Ok(SupervisionDecl {
            proxy,
            mediator,
            phase_schedule,
        })
    }
}

// ── the live supervision ───────────────────────────────────────────────

/// `Supervision` — the live pair: the proxy on its Json control
/// channel, the mediator on its ready handshake, the phase grant table
/// extracted from the sealed schedule, and the folded carry-in already
/// landed inside `configure.authorization.prior_windows`.
pub struct Supervision {
    /// The proxy process handle.
    pub proxy: ProxyCtl,
    /// The mediator process handle (when declared).
    pub mediator: Option<MediatorProc>,
    /// `phase → granted socket paths` from the sealed schedule.
    pub phase_grants: BTreeMap<String, Vec<String>>,
    /// The model relay port the cell env projection binds.
    pub model_port: u16,
    /// The declared per-slot effort.
    pub effort: String,
    /// The declared per-slice micro-units.
    pub slice_micros: i64,
    /// The folded prior windows (as passed to configure).
    pub prior_windows: Vec<PriorSpend>,
}

fn extract_grants(phase_schedule: &Json) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for phase in ["setup", "agent", "verify"] {
        let allow = phase_schedule
            .get(phase)
            .and_then(|p| p.get("layer"))
            .and_then(|l| l.get("net"))
            .and_then(|n| n.get("unix_sockets"))
            .and_then(|u| u.get("allow"))
            .and_then(JsonExt::as_arr)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        out.insert(phase.to_string(), allow);
    }
    out
}

impl Supervision {
    /// `open(decl, proxy_bin, mediator_bin)` — fold the declared prior
    /// ledgers read-only, spawn the proxy with the completed
    /// `configure` doc, spawn the mediator, and extract the phase
    /// grants. A fold refusal fires before either process spawns.
    pub fn open(
        decl: &SupervisionDecl,
        proxy_bin: &Path,
        mediator_bin: Option<&Path>,
    ) -> Result<Supervision, SupError> {
        // X-02 — the carried root pool is mechanical: fold every
        // declared prior ledger first; a missing run is `unharvested`,
        // a verify failure `unverified`, and neither is ever estimated.
        let mut prior = Vec::with_capacity(decl.proxy.prior_ledgers.len());
        for p in &decl.proxy.prior_ledgers {
            prior.push(fold_prior_window(Path::new(&p.store_root), &p.run_id)?);
        }
        let mut auth = match decl.proxy.authorization.clone() {
            Json::Obj(m) => m,
            _ => BTreeMap::new(),
        };
        auth.insert(
            "prior_windows".to_string(),
            Json::Arr(prior.iter().map(PriorSpend::to_json).collect()),
        );
        let mut cfg = vec![
            ("op", Json::str("configure")),
            ("store_root", Json::str(&decl.proxy.store_root)),
            ("run_id", Json::str(&decl.proxy.run_id)),
            ("listen", Json::str(&decl.proxy.listen)),
            ("lane", decl.proxy.lane.clone()),
            ("authorization", Json::Obj(auth)),
        ];
        if let Some(c) = &decl.proxy.credential_env {
            cfg.push(("credential_env", Json::str(c)));
        }
        let mut proxy = ProxyCtl::spawn(proxy_bin, &Json::obj(cfg), &decl.proxy.child_env)?;
        proxy.store_root = PathBuf::from(&decl.proxy.store_root);
        proxy.run_id = decl.proxy.run_id.clone();
        let model_port = proxy
            .listen
            .rsplit(':')
            .next()
            .and_then(|p| p.parse::<u16>().ok())
            .ok_or_else(|| err("proxy listen has no port"))?;
        let mediator = decl
            .mediator
            .as_ref()
            .map(|md| -> Result<MediatorProc, SupError> {
                let bin =
                    mediator_bin.ok_or_else(|| err("mediator declared but no binary resolves"))?;
                let mut args = vec![
                    "--store-root".to_string(),
                    md.store_root.clone(),
                    "--socket".to_string(),
                    md.socket.clone(),
                    "--policy".to_string(),
                    md.policy.clone(),
                    "--env-handle".to_string(),
                    md.env_handle.clone(),
                    "--effect-id".to_string(),
                    md.effect_id.clone(),
                    "--participant".to_string(),
                    md.participant.clone(),
                ];
                if let Some(r) = &md.run_id {
                    args.push("--run-id".to_string());
                    args.push(r.clone());
                }
                if let Some(s) = md.seed {
                    args.push("--seed".to_string());
                    args.push(s.to_string());
                }
                for r in &md.resolve {
                    args.push("--resolve".to_string());
                    args.push(r.clone());
                }
                MediatorProc::spawn(bin, &args)
            })
            .transpose()?;
        Ok(Supervision {
            proxy,
            mediator,
            phase_grants: extract_grants(&decl.phase_schedule),
            model_port,
            effort: decl.proxy.effort.clone(),
            slice_micros: decl.proxy.slice_micros,
            prior_windows: prior,
        })
    }

    /// `slot_open(slot_id)` — mint the attempt's token and project the
    /// cell environment + phase grants. The returned stamp lands in the
    /// slot packet's `cell` member (`env`, `grants`) — the upstream
    /// credential is never in it (DF-R6.7-1's cell-environment leg).
    pub fn slot_open(&mut self, slot_id: &str) -> Result<Json, SupError> {
        let token = self
            .proxy
            .register_slot(slot_id, &self.effort, self.slice_micros)?;
        let env = hharness_env::egress_allowlist::cell_env_projection(self.model_port, &token)
            .map_err(|e| err(format!("cell env projection: {e}")))?;
        Ok(Json::obj([
            (
                "env",
                Json::Obj(env.iter().map(|(k, v)| (k.clone(), Json::str(v))).collect()),
            ),
            (
                "grants",
                Json::Obj(
                    self.phase_grants
                        .iter()
                        .map(|(p, ss)| (p.clone(), Json::Arr(ss.iter().map(Json::str).collect())))
                        .collect(),
                ),
            ),
        ]))
    }

    /// `slot_close(slot_id)` — revoke the attempt's token
    /// (`security.credential.revoked`; X-05 — no token outlives its
    /// slot). Errors degrade to a recorded miss, never a panic: the
    /// slot already closed.
    pub fn slot_close(&mut self, slot_id: &str) -> Result<(), SupError> {
        self.proxy.revoke_slot(slot_id)
    }

    /// `boundary(label)` — the §4.6 catalog re-probe at a declared
    /// block boundary (REQ-XC-119).
    pub fn boundary(&mut self, label: &str) {
        let _ = self.proxy.catalog_boundary(label);
    }

    /// `close` — `stop` the proxy (the catalog end-probe), then the
    /// mediator. The proxy ledger stays a finished window for the next
    /// window's carry-in fold.
    pub fn close(self) {
        self.proxy.stop();
        if let Some(m) = self.mediator {
            m.stop();
        }
    }
}
