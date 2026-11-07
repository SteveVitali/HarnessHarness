//! The $0 oracle re-proof driver (manifest 270/R6.17, REPROOF;
//! contributing REQ-XB-125 — the owner R6.32 executes it in W1): the
//! §3.8 observe → freeze → prove sequence over surfaces O, H-pub and
//! H-allow at R each; the admit rule (R/R on every surface); the
//! discordance classes `o_fail` (incl. `flaky`), `machinery` (repaired,
//! never excluded) and `allowlist_gap`; the append-only discordance
//! register (`hh-reproof-register/1`); and the packet
//! (`hh-reproof-packet/1`) carrying the pinned label and
//! `parity_admission`'s committed typed refusal (REQ-XB-126).
//!
//! The REQ-XB-110 fault-injection leg (one task's verify allowlist
//! without `astral.sh`) is declared here and proven by the battery
//! against R6.9's classifier; R6.32 executes it in W1.

use std::collections::BTreeMap;

use hharness_env::egress_allowlist::{allowlist_gap_basis, AllowlistFreeze};
use hharness_eval::parity_admission::{
    parity_admission, AdmissionError, AdmissionEvidence, ParityAdmissionDecl, PolicyApplicability,
    SourcePins,
};
use hharness_wire::json::Json;

use crate::harbor::{image_identical_label, HarborPin};
use crate::json_ext::JsonExt;
use crate::verdict::{classify, MediatorRow, RewardFileSet};

/// `hh-reproof-packet/1` — the re-proof result record.
pub const PACKET_SCHEMA: &str = "hh-reproof-packet/1";
/// `hh-reproof-register/1` — the append-only discordance register.
pub const REGISTER_SCHEMA: &str = "hh-reproof-register/1";
/// The pinned packet label (§3.8; REQ-XB-126 — a phrase pin).
pub const REPROOF_LABEL: &str =
    "Oracle execution-surface re-proof (internal, recorded execution, `parity: none`)";

/// A typed refusal — never a silent downgrade.
#[derive(Debug, Clone, PartialEq)]
pub struct ReproofError {
    /// The refusal spelling.
    pub detail: String,
}

fn err(d: impl Into<String>) -> ReproofError {
    ReproofError { detail: d.into() }
}

impl std::fmt::Display for ReproofError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "reproof: {}", self.detail)
    }
}

impl std::error::Error for ReproofError {}

fn need<'a>(m: &'a BTreeMap<String, Json>, k: &str) -> Result<&'a Json, ReproofError> {
    m.get(k).ok_or_else(|| err(format!("{k} required")))
}

fn need_str(m: &BTreeMap<String, Json>, k: &str) -> Result<String, ReproofError> {
    need(m, k)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| err(format!("{k} must be a string")))
}

fn no_unknown(m: &BTreeMap<String, Json>, allowed: &[&str], at: &str) -> Result<(), ReproofError> {
    for k in m.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(err(format!("{at}: unknown member {k}")));
        }
    }
    Ok(())
}

// ── surfaces, the observe pass and the admit rule ─────────────────────────────

/// The three surfaces the re-proof proves (§3.8).
pub const SURFACES: [&str; 3] = ["O", "H-pub", "H-allow"];

/// How the observe pass recorded candidate hosts (§3.8 pass 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservePass {
    /// H-pub with the mediator in record-only mode.
    MediatorRecordOnly,
    /// H-allow with a permissive `ask→log` rule (no admitted host-side
    /// capture).
    AskToLog,
}

impl ObservePass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ObservePass::MediatorRecordOnly => "h_pub_mediator_record_only",
            ObservePass::AskToLog => "h_allow_ask_to_log",
        }
    }
}

/// The exclusion basis — `runner_infeasible:oracle_fails_on_O{<cause>}`
/// (§3.8; outcome-blind — O's recorded cause, never the task's content).
pub fn oracle_fails_basis(harbor_cause: &str) -> String {
    format!("runner_infeasible:oracle_fails_on_O{{{harbor_cause}}}")
}

/// The discordance classes (§3.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscordClass {
    /// `o_fail` — O < R/R: excluded (outcome-blind basis).
    OFail,
    /// `machinery` — O R/R, H-pub < R/R: repaired, never excluded.
    Machinery,
    /// `allowlist_gap` — H-pub R/R, H-allow < R/R: excluded, replaced.
    AllowlistGap,
}

impl DiscordClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DiscordClass::OFail => "o_fail",
            DiscordClass::Machinery => "machinery",
            DiscordClass::AllowlistGap => "allowlist_gap",
        }
    }
}

/// One task's re-proof verdict.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskVerdict {
    /// `admit` — R/R on all three surfaces.
    Admit,
    /// A discordance — `exclude` names the outcome-blind basis;
    /// `machinery` rows carry none (repaired, never excluded).
    Discord {
        /// The discordance class.
        class: DiscordClass,
        /// The outcome-blind exclusion basis (`None` for `machinery`).
        basis: Option<String>,
        /// `o_fail`'s `flaky` sub-reason (0 < O < R).
        flaky: bool,
        /// The gap's denied host (`allowlist_gap`).
        host: Option<String>,
    },
}

impl TaskVerdict {
    /// The canonical spelling for the packet's task row.
    pub fn as_str(&self) -> String {
        match self {
            TaskVerdict::Admit => "admit".to_string(),
            TaskVerdict::Discord { class, basis, .. } => match basis {
                Some(b) => format!("exclude:{b}"),
                None => format!("repair:{}", class.as_str()),
            },
        }
    }
}

/// `classify_task(o, h_pub, h_allow, harbor_cause, denied_hosts)` — the
/// §3.8 admit rule, evaluated in table order:
///
/// 1. O < R/R → `o_fail` (excluded; `flaky` when 0 < O < R);
/// 2. O R/R, H-pub < R/R → `machinery` (repair, never excluded);
/// 3. O R/R, H-pub R/R, H-allow < R/R → `allowlist_gap{host}`;
/// 4. else `admit`.
pub fn classify_task(
    o: &[bool],
    h_pub: &[bool],
    h_allow: &[bool],
    harbor_cause: &str,
    denied_hosts: &[String],
) -> TaskVerdict {
    let all = |v: &[bool]| !v.is_empty() && v.iter().all(|b| *b);
    if !all(o) {
        return TaskVerdict::Discord {
            class: DiscordClass::OFail,
            basis: Some(oracle_fails_basis(harbor_cause)),
            flaky: o.iter().any(|b| *b),
            host: None,
        };
    }
    if !all(h_pub) {
        return TaskVerdict::Discord {
            class: DiscordClass::Machinery,
            basis: None,
            flaky: false,
            host: None,
        };
    }
    if !all(h_allow) {
        return TaskVerdict::Discord {
            class: DiscordClass::AllowlistGap,
            basis: Some(allowlist_gap_basis(
                denied_hosts
                    .first()
                    .map(String::as_str)
                    .unwrap_or("unknown"),
            )),
            flaky: false,
            host: denied_hosts.first().cloned(),
        };
    }
    TaskVerdict::Admit
}

// ── the append-only discordance register (§3.8 fields) ────────────────────────

/// One register row — the §3.8 field set.
#[derive(Debug, Clone, PartialEq)]
pub struct RegisterRow {
    /// The append sequence (monotone, unique).
    pub seq: u64,
    /// The discordant task.
    pub task: String,
    /// `surface → (passed, total)`.
    pub surfaces: BTreeMap<String, (u32, u32)>,
    /// R — repeats per surface.
    pub repeats: u32,
    /// `kind → sha256` — reward/ctrf/stdout digests per surface.
    pub digests: BTreeMap<String, String>,
    /// The discordance class spelling.
    pub classification: String,
    /// The mediator's denied hosts (H-allow rows).
    pub denied_hosts: Vec<String>,
    /// The Harbor exception, verbatim.
    pub harbor_exception: Option<String>,
    /// Image-digest equality across surfaces (declared only).
    pub image_digest_equal: Option<bool>,
    /// Per-surface wall times (ms).
    pub walls: BTreeMap<String, u64>,
    /// The recorded hypothesis.
    pub hypothesis: Option<String>,
    /// `repair{class, location, commit}` when recorded.
    pub repair: Option<BTreeMap<String, Json>>,
    /// The re-run epoch.
    pub rerun_epoch: Option<String>,
}

impl RegisterRow {
    /// The canonical row JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("seq".into(), Json::Int(self.seq as i64));
        m.insert("task".into(), Json::str(&self.task));
        m.insert(
            "surfaces".into(),
            Json::Obj(
                self.surfaces
                    .iter()
                    .map(|(s, (p, t))| {
                        (
                            s.clone(),
                            Json::obj([
                                ("passed", Json::Int(*p as i64)),
                                ("total", Json::Int(*t as i64)),
                            ]),
                        )
                    })
                    .collect(),
            ),
        );
        m.insert("repeats".into(), Json::Int(self.repeats as i64));
        m.insert(
            "digests".into(),
            Json::Obj(
                self.digests
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                    .collect(),
            ),
        );
        m.insert("classification".into(), Json::str(&self.classification));
        m.insert(
            "denied_hosts".into(),
            Json::Arr(
                self.denied_hosts
                    .iter()
                    .map(|h| Json::str(h.clone()))
                    .collect(),
            ),
        );
        if let Some(e) = &self.harbor_exception {
            m.insert("harbor_exception".into(), Json::str(e));
        }
        if let Some(eq) = self.image_digest_equal {
            m.insert("image_digest_equal".into(), Json::Bool(eq));
        }
        m.insert(
            "walls".into(),
            Json::Obj(
                self.walls
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                    .collect(),
            ),
        );
        if let Some(h) = &self.hypothesis {
            m.insert("hypothesis".into(), Json::str(h));
        }
        if let Some(r) = &self.repair {
            m.insert("repair".into(), Json::Obj(r.clone()));
        }
        if let Some(e) = &self.rerun_epoch {
            m.insert("rerun_epoch".into(), Json::str(e));
        }
        Json::Obj(m)
    }
}

/// `hh-reproof-register/1` — the append-only register. The type admits
/// `append` only; decode requires unique monotone `seq` (a re-ordered
/// or rewritten register refuses).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiscordanceRegister {
    /// The rows, in append order.
    pub rows: Vec<RegisterRow>,
}

impl DiscordanceRegister {
    /// Append one row — the only mutation path; `seq` is assigned.
    pub fn append(&mut self, mut row: RegisterRow) {
        row.seq = self.rows.len() as u64 + 1;
        self.rows.push(row);
    }

    /// The canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(REGISTER_SCHEMA)),
            (
                "rows",
                Json::Arr(self.rows.iter().map(|r| r.to_json()).collect()),
            ),
        ])
    }

    /// Strict decode — monotone unique `seq` required.
    pub fn from_json(j: &Json) -> Result<DiscordanceRegister, ReproofError> {
        let m = j
            .as_obj()
            .ok_or_else(|| err("register must be an object"))?;
        no_unknown(m, &["schema", "rows"], "register")?;
        if need_str(m, "schema")? != REGISTER_SCHEMA {
            return Err(err("schema member is not hh-reproof-register/1"));
        }
        let mut rows = Vec::new();
        let mut last = 0u64;
        for rj in need(m, "rows")?.as_arr().cloned().unwrap_or_default() {
            let rm = rj.as_obj().ok_or_else(|| err("rows[] must be objects"))?;
            no_unknown(
                rm,
                &[
                    "seq",
                    "task",
                    "surfaces",
                    "repeats",
                    "digests",
                    "classification",
                    "denied_hosts",
                    "harbor_exception",
                    "image_digest_equal",
                    "walls",
                    "hypothesis",
                    "repair",
                    "rerun_epoch",
                ],
                "rows[]",
            )?;
            let seq = need(rm, "seq")?
                .as_int()
                .map(|i| i as u64)
                .ok_or_else(|| err("rows[].seq must be an integer"))?;
            if seq != last + 1 {
                return Err(err(format!(
                    "rows[].seq {seq} breaks append order (expected {})",
                    last + 1
                )));
            }
            last = seq;
            let surf = |k: &str| -> Result<BTreeMap<String, (u32, u32)>, ReproofError> {
                let mut out = BTreeMap::new();
                for (s, v) in need(rm, k)?
                    .as_obj()
                    .ok_or_else(|| err(format!("rows[].{k} must be an object")))?
                {
                    let vm = v.as_obj().ok_or_else(|| err("surfaces.* objects"))?;
                    let i = |kk: &str| -> Result<u32, ReproofError> {
                        vm.get(kk)
                            .and_then(Json::as_int)
                            .map(|x| x as u32)
                            .ok_or_else(|| err(format!("surfaces.{s}.{kk} integer")))
                    };
                    out.insert(s.clone(), (i("passed")?, i("total")?));
                }
                Ok(out)
            };
            let map_str = |k: &str| -> BTreeMap<String, String> {
                rm.get(k)
                    .and_then(Json::as_obj)
                    .map(|o| {
                        o.iter()
                            .filter_map(|(kk, vv)| vv.as_str().map(|s| (kk.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let map_int = |k: &str| -> BTreeMap<String, u64> {
                rm.get(k)
                    .and_then(Json::as_obj)
                    .map(|o| {
                        o.iter()
                            .filter_map(|(kk, vv)| vv.as_int().map(|i| (kk.clone(), i as u64)))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            rows.push(RegisterRow {
                seq,
                task: need_str(rm, "task")?,
                surfaces: surf("surfaces")?,
                repeats: need(rm, "repeats")?
                    .as_int()
                    .map(|i| i as u32)
                    .ok_or_else(|| err("rows[].repeats integer"))?,
                digests: map_str("digests"),
                classification: need_str(rm, "classification")?,
                denied_hosts: rm
                    .get("denied_hosts")
                    .and_then(Json::as_arr)
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|h| h.as_str().map(str::to_string))
                    .collect(),
                harbor_exception: rm
                    .get("harbor_exception")
                    .and_then(Json::as_str)
                    .map(str::to_string),
                image_digest_equal: rm.get("image_digest_equal").and_then(Json::as_bool),
                walls: map_int("walls"),
                hypothesis: rm
                    .get("hypothesis")
                    .and_then(Json::as_str)
                    .map(str::to_string),
                repair: rm.get("repair").and_then(Json::as_obj).cloned(),
                rerun_epoch: rm
                    .get("rerun_epoch")
                    .and_then(Json::as_str)
                    .map(str::to_string),
            });
        }
        Ok(DiscordanceRegister { rows })
    }
}

/// Strict `Allowlist` decode (the codec has no `from_json` — the
/// packet decodes the freeze's own members and refuses drift).
fn allowlist_from_json(
    j: &Json,
) -> Result<hharness_env::egress_allowlist::Allowlist, ReproofError> {
    let m = j
        .as_obj()
        .ok_or_else(|| err("freeze.list must be an object"))?;
    no_unknown(
        m,
        &["agent", "verify", "permanent_denies", "withheld"],
        "freeze.list",
    )?;
    let hosts = |k: &str| -> Result<std::collections::BTreeSet<String>, ReproofError> {
        need(m, k)?
            .as_arr()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|h| {
                h.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| err(format!("freeze.list.{k}[] strings")))
            })
            .collect()
    };
    let withheld = need(m, "withheld")?
        .as_arr()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|w| {
            let wm = w.as_obj().ok_or_else(|| err("withheld[] objects"))?;
            Ok(hharness_env::egress_allowlist::WithheldHost {
                host: need_str(wm, "host")?,
                reason: need_str(wm, "reason")?,
            })
        })
        .collect::<Result<Vec<_>, ReproofError>>()?;
    Ok(hharness_env::egress_allowlist::Allowlist {
        agent: hosts("agent")?,
        verify: hosts("verify")?,
        permanent_denies: hosts("permanent_denies")?,
        withheld,
    })
}

// ── the parity refusal committed beside the packet (REQ-XB-126) ───────────────

/// The committed `parity_admission` refusal — the packet submits the
/// `oracle_transport_equality` evidence shape and records the typed
/// refusal verbatim (A1 §4; the spec's
/// `InauthenticEvidence::OracleTransportEquality` names this refusal;
/// the code spelling is `InauthenticEvidence{shape}`).
#[derive(Debug, Clone, PartialEq)]
pub struct ParityRefusal {
    /// The submitted evidence shape.
    pub submitted: String,
    /// The refusal recorded — `inauthentic_evidence{<shape>}`.
    pub refusal: String,
}

/// `committed_parity_refusal(transport_ref)` — runs the real
/// `parity_admission` over `AdmissionEvidence::OracleTransportEquality`
/// and records its typed refusal. Any other outcome is recorded
/// verbatim — the packet never claims the refusal it didn't get.
pub fn committed_parity_refusal(transport_ref: &str) -> ParityRefusal {
    let decl = ParityAdmissionDecl {
        participant_ref: "hh-reproof-packet/1".to_string(),
        policies: vec!["reproof_execution_surface".to_string()],
        evidence: AdmissionEvidence::OracleTransportEquality {
            transport_ref: transport_ref.to_string(),
        },
        dimension_map: None,
        applicability: PolicyApplicability::BothPolicies { evidence: vec![] },
        transfer: None,
        pins: SourcePins {
            source_ref: "hh-reproof-packet/1".to_string(),
            verifier_ref: "hh-reproof-packet/1".to_string(),
            rights_ref: "hh-reproof-packet/1".to_string(),
        },
        use_as: None,
    };
    match parity_admission(&decl, &[]) {
        Err(AdmissionError::InauthenticEvidence { shape }) => ParityRefusal {
            submitted: shape.clone(),
            refusal: format!("inauthentic_evidence{{{shape}}}"),
        },
        Err(e) => ParityRefusal {
            submitted: "oracle_transport_equality".to_string(),
            refusal: format!("unexpected_refusal{{{e}}}"),
        },
        Ok(_) => ParityRefusal {
            submitted: "oracle_transport_equality".to_string(),
            refusal: "unexpected_outcome{admitted}".to_string(),
        },
    }
}

// ── REQ-XB-110's fault-injection leg ──────────────────────────────────────────

/// The declared fault-injection leg: one task's verify allowlist
/// without `astral.sh` — expected `oracle_failure`'s
/// `verifier_unavailable:bootstrap:egress_denied` with the mediator
/// deny row (R6.9's classifier; executed by R6.32 in W1).
#[derive(Debug, Clone, PartialEq)]
pub struct FaultInjection {
    /// The task the leg removes the host from.
    pub task: String,
    /// The removed verify-phase host (`astral.sh`).
    pub removed_host: String,
    /// The expected classification spelling.
    pub expected: String,
    /// The observed spelling when the leg ran (fixtures record it).
    pub observed: Option<String>,
}

/// `run_fault_injection(files, mediator_rows)` — the leg's fixture leg:
/// a zero reward without a pytest report under a verify-phase deny of
/// the removed host classifies `verifier_unavailable:bootstrap:
/// egress_denied` (R6.9's `classify`, a pure function).
pub fn run_fault_injection(files: &RewardFileSet, mediator_rows: &[MediatorRow]) -> String {
    let v = classify(files, mediator_rows, false);
    format!("oracle_failure{{{}}}", v.classification)
}

// ── the driver: observe → freeze → prove ──────────────────────────────────────

/// One surface's recorded outcomes for a task (records-in).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SurfaceOutcome {
    /// Per-repeat success booleans (decided reward = 1).
    pub rewards: Vec<bool>,
    /// `kind → sha256` digests (reward/ctrf/stdout, image).
    pub digests: BTreeMap<String, String>,
    /// The mediator's denied hosts (H-allow rows).
    pub denied_hosts: Vec<String>,
    /// The Harbor exception, verbatim (O rows).
    pub harbor_exception: Option<String>,
    /// Wall time (ms).
    pub wall_ms: u64,
}

/// The surface driver seam — the fixture supplies per-(task, surface)
/// outcomes; production binds the real lanes (R6.32).
pub trait SurfaceDrive {
    /// Run `task` on `surface` for `r` repeats; return the recorded outcome.
    fn run_surface(&mut self, surface: &str, task: &str, r: u32) -> SurfaceOutcome;
}

/// The re-proof plan — the observe pass's mode, R6.15's committed
/// freeze (the prove pass's allowlist), R and the task list.
#[derive(Debug, Clone, PartialEq)]
pub struct ReproofPlan {
    /// The program-wide pin.
    pub pin: HarborPin,
    /// R — repeats per surface per task (≥ 2).
    pub r: u32,
    /// The observe pass's mode.
    pub observe: ObservePass,
    /// The committed allowlist freeze (R6.15's `hh-egress-allowlist-freeze/1`).
    pub freeze: AllowlistFreeze,
    /// The tasks to prove.
    pub tasks: Vec<String>,
    /// The declared fault-injection leg (REQ-XB-110).
    pub fault_injection: Option<FaultInjection>,
}

/// The evidence label — `recorded_execution` unless an
/// `hh-live-evidence/1` record exists (the registered `reproof` leg;
/// REQ-XH-027/Q-X39b). `live-executed` is constructible only with the
/// record's ref.
#[derive(Debug, Clone, PartialEq)]
pub enum EvidenceLabel {
    /// `recorded_execution` — the default.
    RecordedExecution,
    /// `live-executed` — only with the `hh-live-evidence/1` ref.
    LiveExecuted(String),
}

impl EvidenceLabel {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            EvidenceLabel::RecordedExecution => "recorded_execution",
            EvidenceLabel::LiveExecuted(_) => "live-executed",
        }
    }
}

/// `task → (verdict spelling, surface → (passed, total))`.
pub type TaskRows = BTreeMap<String, (String, BTreeMap<String, (u32, u32)>)>;

/// `hh-reproof-packet/1` — the packet: label, surfaces, R, per-task
/// rows, the register, the freeze, the committed refusal, the
/// fault-injection declaration and the evidence label.
#[derive(Debug, Clone, PartialEq)]
pub struct ReproofPacket {
    /// The program-wide pin.
    pub harbor_pin: String,
    /// The observe pass's mode.
    pub observe: ObservePass,
    /// The committed freeze (verbatim record + digest).
    pub freeze: AllowlistFreeze,
    /// R per surface.
    pub r: u32,
    /// `task → (verdict spelling, surfaces map)`.
    pub tasks: TaskRows,
    /// The append-only register.
    pub register: DiscordanceRegister,
    /// The committed parity refusal.
    pub parity_refusal: ParityRefusal,
    /// The REQ-XB-110 leg declaration.
    pub fault_injection: Option<FaultInjection>,
    /// The evidence label.
    pub evidence: EvidenceLabel,
    /// REQ-XC-306's image-identity label for the runner checks.
    pub image_identity: String,
}

impl ReproofPacket {
    /// The canonical record — the pinned label is emitted, never
    /// caller-supplied; `parity: none` rides beside it.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str(PACKET_SCHEMA));
        m.insert("label".into(), Json::str(REPROOF_LABEL));
        m.insert("parity".into(), Json::str("none"));
        m.insert("harbor_pin".into(), Json::str(&self.harbor_pin));
        m.insert("observe".into(), Json::str(self.observe.as_str()));
        m.insert("freeze".into(), self.freeze.to_json());
        m.insert("r".into(), Json::Int(self.r as i64));
        m.insert(
            "surfaces".into(),
            Json::Arr(SURFACES.iter().map(|s| Json::str(*s)).collect()),
        );
        m.insert("image_identity".into(), Json::str(&self.image_identity));
        m.insert(
            "tasks".into(),
            Json::Obj(
                self.tasks
                    .iter()
                    .map(|(t, (v, s))| {
                        (
                            t.clone(),
                            Json::obj([
                                ("verdict", Json::str(v.clone())),
                                (
                                    "surfaces",
                                    Json::Obj(
                                        s.iter()
                                            .map(|(su, (p, tt))| {
                                                (
                                                    su.clone(),
                                                    Json::obj([
                                                        ("passed", Json::Int(*p as i64)),
                                                        ("total", Json::Int(*tt as i64)),
                                                    ]),
                                                )
                                            })
                                            .collect(),
                                    ),
                                ),
                            ]),
                        )
                    })
                    .collect(),
            ),
        );
        m.insert("register".into(), self.register.to_json());
        m.insert(
            "parity_refusal".into(),
            Json::obj([
                ("submitted", Json::str(&self.parity_refusal.submitted)),
                ("refusal", Json::str(&self.parity_refusal.refusal)),
            ]),
        );
        if let Some(fi) = &self.fault_injection {
            let mut f = vec![
                ("task", Json::str(&fi.task)),
                ("removed_host", Json::str(&fi.removed_host)),
                ("expected", Json::str(&fi.expected)),
            ];
            if let Some(o) = &fi.observed {
                f.push(("observed", Json::str(o)));
            }
            m.insert("fault_injection".into(), Json::obj(f));
        }
        m.insert("evidence_label".into(), Json::str(self.evidence.as_str()));
        if let EvidenceLabel::LiveExecuted(r) = &self.evidence {
            m.insert("live_evidence_ref".into(), Json::str(r));
        }
        Json::Obj(m)
    }

    /// Strict decode — the pinned phrase and the evidence label are
    /// verified (`live-executed` refuses without `live_evidence_ref`).
    pub fn from_json(j: &Json) -> Result<ReproofPacket, ReproofError> {
        let m = j.as_obj().ok_or_else(|| err("packet must be an object"))?;
        no_unknown(
            m,
            &[
                "schema",
                "label",
                "parity",
                "harbor_pin",
                "observe",
                "freeze",
                "r",
                "surfaces",
                "image_identity",
                "tasks",
                "register",
                "parity_refusal",
                "fault_injection",
                "evidence_label",
                "live_evidence_ref",
            ],
            "packet",
        )?;
        if need_str(m, "schema")? != PACKET_SCHEMA {
            return Err(err("schema member is not hh-reproof-packet/1"));
        }
        let label = need_str(m, "label")?;
        if label != REPROOF_LABEL {
            return Err(err(format!(
                "label drifted — pinned phrase required, got {label:?}"
            )));
        }
        if need_str(m, "parity")? != "none" {
            return Err(err("parity member must be \"none\""));
        }
        let evidence = match need_str(m, "evidence_label")?.as_str() {
            "recorded_execution" => EvidenceLabel::RecordedExecution,
            "live-executed" => {
                let r = m
                    .get("live_evidence_ref")
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        err("live-executed without live_evidence_ref — an \
                             hh-live-evidence/1 record is required (REQ-XH-027)")
                    })?;
                EvidenceLabel::LiveExecuted(r)
            }
            other => return Err(err(format!("evidence_label {other} unknown"))),
        };
        let observe = match need_str(m, "observe")?.as_str() {
            "h_pub_mediator_record_only" => ObservePass::MediatorRecordOnly,
            "h_allow_ask_to_log" => ObservePass::AskToLog,
            other => return Err(err(format!("observe {other} unknown"))),
        };
        let freeze_j = need(m, "freeze")?.clone();
        let fm = freeze_j
            .as_obj()
            .ok_or_else(|| err("freeze must be an object"))?;
        let list = allowlist_from_json(need(fm, "list")?)?;
        let freeze = AllowlistFreeze {
            protocol_id: need_str(fm, "protocol_id")?,
            epoch: need(fm, "epoch")?
                .as_int()
                .map(|i| i as u64)
                .ok_or_else(|| err("freeze.epoch integer"))?,
            list,
            digest: need_str(fm, "digest")?,
        };
        let mut tasks = BTreeMap::new();
        for (t, v) in need(m, "tasks")?
            .as_obj()
            .ok_or_else(|| err("tasks must be an object"))?
        {
            let vm = v.as_obj().ok_or_else(|| err("tasks.* objects"))?;
            no_unknown(vm, &["verdict", "surfaces"], "tasks.*")?;
            let mut surfaces = BTreeMap::new();
            for (s, sv) in need(vm, "surfaces")?
                .as_obj()
                .ok_or_else(|| err("tasks.*.surfaces object"))?
            {
                let sm = sv.as_obj().ok_or_else(|| err("surfaces.* objects"))?;
                let i = |k: &str| -> Result<u32, ReproofError> {
                    sm.get(k)
                        .and_then(Json::as_int)
                        .map(|x| x as u32)
                        .ok_or_else(|| err(format!("surfaces.{s}.{k} integer")))
                };
                surfaces.insert(s.clone(), (i("passed")?, i("total")?));
            }
            tasks.insert(
                t.clone(),
                (
                    vm.get("verdict")
                        .and_then(Json::as_str)
                        .map(str::to_string)
                        .ok_or_else(|| err("tasks.*.verdict"))?,
                    surfaces,
                ),
            );
        }
        let prm = need(m, "parity_refusal")?
            .as_obj()
            .ok_or_else(|| err("parity_refusal must be an object"))?;
        no_unknown(prm, &["submitted", "refusal"], "parity_refusal")?;
        let parity_refusal = ParityRefusal {
            submitted: need_str(prm, "submitted")?,
            refusal: need_str(prm, "refusal")?,
        };
        let fault_injection = match m.get("fault_injection") {
            None | Some(Json::Null) => None,
            Some(fj) => {
                let f = fj
                    .as_obj()
                    .ok_or_else(|| err("fault_injection must be an object"))?;
                no_unknown(
                    f,
                    &["task", "removed_host", "expected", "observed"],
                    "fault_injection",
                )?;
                Some(FaultInjection {
                    task: need_str(f, "task")?,
                    removed_host: need_str(f, "removed_host")?,
                    expected: need_str(f, "expected")?,
                    observed: f.get("observed").and_then(Json::as_str).map(str::to_string),
                })
            }
        };
        Ok(ReproofPacket {
            harbor_pin: need_str(m, "harbor_pin")?,
            observe,
            freeze,
            r: need(m, "r")?
                .as_int()
                .map(|i| i as u32)
                .ok_or_else(|| err("r integer"))?,
            tasks,
            register: DiscordanceRegister::from_json(need(m, "register")?)?,
            parity_refusal,
            fault_injection,
            evidence,
            image_identity: need_str(m, "image_identity")?,
        })
    }
}

/// `prove(plan, drive)` — the prove pass: every task runs O, H-pub and
/// H-allow at R each; the admit rule classifies; discordances append to
/// the register (one row per discordant task); the packet seals with
/// the committed parity refusal and `recorded_execution` (the
/// `live-executed` label is never minted here — R6.32's registered leg
/// produces the `hh-live-evidence/1` record).
pub fn prove(
    plan: &ReproofPlan,
    drive: &mut dyn SurfaceDrive,
) -> Result<ReproofPacket, ReproofError> {
    if plan.r < 2 {
        return Err(err("R < 2 refused — the declared minimum is 2 (§3.8)"));
    }
    let mut tasks = BTreeMap::new();
    let mut register = DiscordanceRegister::default();
    for task in &plan.tasks {
        let mut counts = BTreeMap::new();
        let mut outcomes = BTreeMap::new();
        for surface in SURFACES {
            let out = drive.run_surface(surface, task, plan.r);
            counts.insert(
                surface.to_string(),
                (out.rewards.iter().filter(|b| **b).count() as u32, plan.r),
            );
            outcomes.insert(surface, out);
        }
        let verdict = classify_task(
            &outcomes["O"].rewards,
            &outcomes["H-pub"].rewards,
            &outcomes["H-allow"].rewards,
            outcomes["O"]
                .harbor_exception
                .as_deref()
                .unwrap_or("unclassified"),
            &outcomes["H-allow"].denied_hosts,
        );
        if let TaskVerdict::Discord {
            class,
            flaky,
            host: _,
            ..
        } = &verdict
        {
            let mut digests = BTreeMap::new();
            let mut walls = BTreeMap::new();
            let o = &outcomes["O"];
            let hp = &outcomes["H-pub"];
            let ha = &outcomes["H-allow"];
            for (s, out) in [("O", o), ("H-pub", hp), ("H-allow", ha)] {
                for (k, v) in &out.digests {
                    digests.insert(format!("{s}.{k}"), v.clone());
                }
                walls.insert(s.to_string(), out.wall_ms);
            }
            // Image-digest equality across O/H — recorded only when
            // both surfaces declared an image digest.
            let image_digest_equal = match (
                o.digests.get("image"),
                hp.digests.get("image"),
                ha.digests.get("image"),
            ) {
                (Some(a), Some(b), Some(c)) => Some(a == b && b == c),
                _ => None,
            };
            register.append(RegisterRow {
                seq: 0,
                task: task.clone(),
                surfaces: counts.clone(),
                repeats: plan.r,
                digests,
                classification: format!("{}{}", class.as_str(), if *flaky { ":flaky" } else { "" }),
                denied_hosts: ha.denied_hosts.clone(),
                harbor_exception: o.harbor_exception.clone(),
                image_digest_equal,
                walls,
                hypothesis: None,
                repair: None,
                rerun_epoch: None,
            });
        }
        tasks.insert(task.clone(), (verdict.as_str(), counts));
    }
    Ok(ReproofPacket {
        harbor_pin: plan.pin.pin.clone(),
        observe: plan.observe,
        freeze: plan.freeze.clone(),
        r: plan.r,
        tasks,
        register,
        parity_refusal: committed_parity_refusal(&format!(
            "reproof:{}:{}",
            plan.pin.pin, plan.freeze.digest
        )),
        fault_injection: plan.fault_injection.clone(),
        evidence: EvidenceLabel::RecordedExecution,
        image_identity: image_identical_label(&plan.pin).to_string(),
    })
}
