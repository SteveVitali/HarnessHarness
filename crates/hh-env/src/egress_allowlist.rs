//! The `allowlist_mediated` egress profile (R6.15; AR-05;
//! REQ-XB-113/114/115/116/129, absorbing ADR-C2-C) — the single paid-cell
//! network posture of experiments-spec §3.5: podman `--network none` for
//! the cell's whole life, `net.mode = mediated`, exactly the two
//! bind-mounted unix sockets `{model, egress}` that `hh-cell-relay` (PID
//! 1) bridges from in-cell loopback ports, per-phase host-side policy
//! under the sealed `set_phase` schedule, and the declared residuals
//! (`unix_socket` ×2, `tls_opaque`, `approved_host_body`, `dns_names`,
//! `side_channel`).
//!
//! First-pass allowlist (Q-X55): the agent phase reaches only the model
//! proxy and apt/PyPI/npm/crates — no `github.com` and no named-host
//! extension; the verify phase adds the static `tests/*` scan and
//! oracle-observed verify hosts; `hub.harborframework.com`, `tbench.ai`
//! and dedicated TB 2.1 mirror hosts are always denied.
//!
//! The profile is *admitted*, never ambient: `admit_allowlist_mediated`
//! mutates a policy candidate, validates, then commits — a refused
//! admission leaves the record and policy byte-identical (the
//! `declared_deps` transactional shape, ADR-0421). `declared_deps` stays
//! admitted and is never substituted silently.

use std::collections::{BTreeMap, BTreeSet};

use hharness_containment::attach::AttachError;
use hharness_containment::policy::{
    ContainmentPolicy, DefaultUnmatched, DnsResolver, EgressProtocol, EgressRule, HostPattern,
    NetMode, NonPublic, ResidualChannel, ResidualKind, RuleDecision, UnixSocketMode,
};
use hharness_hir::leaves::Text;
use hharness_identity::idp::idp_id;
use hharness_provenance::{AuthorityClass, ProvenanceRecord};
use hharness_wire::json::Json;

use crate::errors::EnvError;
use crate::record::{EnvironmentClass, EnvironmentRecord};

/// The profile's record spelling — `egress_profile: allowlist_mediated`.
pub const EGRESS_PROFILE_ALLOWLIST_MEDIATED: &str = "allowlist_mediated";
/// The podman posture — `cell_network: none` (the cell is never
/// re-networked across phases).
pub const CELL_NETWORK_NONE: &str = "none";
/// The policy mode — `policy_net_mode: mediated`.
pub const POLICY_NET_MODE_MEDIATED: &str = "mediated";
/// The cell-side model socket the relay bridges `127.0.0.1:P_model` to.
pub const MODEL_SOCK_PATH: &str = "/run/hh/model.sock";
/// The cell-side egress socket the relay bridges `127.0.0.1:3128` to.
pub const EGRESS_SOCK_PATH: &str = "/run/hh/egress.sock";
/// The registry-channel loopback port the env hints point at.
pub const EGRESS_RELAY_PORT: u16 = 3128;
/// The in-cell relay binary location (bind-mounted read-only).
pub const RELAY_CELL_BIN: &str = "/hh/bin/hh-cell-relay";
/// The `ext` member the mediated-cell spec lives under.
pub const MEDIATED_CELL_FIELD: &str = "mediated_cell";
/// The `ext` member declaring `per_phase_network_policy = supported`.
pub const PER_PHASE_FIELD: &str = "per_phase_network_policy";
/// The closed relay-pair name set (AR-05; §3.5's `{model, egress}`).
pub const RELAY_PAIR_NAMES: [&str; 2] = ["model", "egress"];
/// The result-line spelling every paid result carries (§3.5 residual
/// exposure row — "allowlisted agent network; not
/// leaderboard-comparable" plus the granted first-pass hosts).
pub const RESULT_LABEL: &str =
    "allowlisted agent network; not leaderboard-comparable — model proxy, apt, PyPI, npm, crates; no github.com";
/// The allowlist freeze record kind (schema owned here; consumer R6.17).
pub const FREEZE_SCHEMA: &str = "hh-egress-allowlist-freeze/1";

fn unsupported(detail: impl Into<String>) -> EnvError {
    EnvError::Unsupported {
        capability: "egress_profile.allowlist_mediated",
        detail: detail.into(),
    }
}

fn jobj<'a>(j: &'a Json) -> Option<&'a BTreeMap<String, Json>> {
    match j {
        Json::Obj(m) => Some(m),
        _ => None,
    }
}

fn jarr<'a>(j: &'a Json) -> Option<&'a Vec<Json>> {
    match j {
        Json::Arr(v) => Some(v),
        _ => None,
    }
}

// ── the host vocabulary (Q-X55 first pass) ───────────────────────────────────

/// The task image's apt mirror family — the base list is per image
/// family (§3.5's "base list per image family").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFamily {
    /// `deb.debian.org` + `security.debian.org` + `ftp.debian.org`.
    Debian,
    /// `archive.ubuntu.com` + `security.ubuntu.com` + `ports.ubuntu.com`.
    Ubuntu,
}

impl ImageFamily {
    /// The apt mirror hosts for the family.
    pub fn apt_hosts(self) -> BTreeSet<String> {
        match self {
            ImageFamily::Debian => ["deb.debian.org", "security.debian.org", "ftp.debian.org"],
            ImageFamily::Ubuntu => [
                "archive.ubuntu.com",
                "security.ubuntu.com",
                "ports.ubuntu.com",
            ],
        }
        .iter()
        .map(|s| s.to_string())
        .collect()
    }
}

/// The registry hosts — PyPI, npm, crates (Q-X55; §3.5).
pub fn registry_hosts() -> BTreeSet<String> {
    [
        "pypi.org",
        "files.pythonhosted.org",
        "registry.npmjs.org",
        "crates.io",
        "index.crates.io",
        "static.crates.io",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// The always-denied answer-key hosts (§3.5 — denies are host-level).
pub const PERMANENT_DENY_HOSTS: [&str; 2] = ["hub.harborframework.com", "tbench.ai"];

/// `github.com` — denied whenever it is not granted (always in the
/// first pass); named so the vocabulary has one home.
pub const GITHUB: &str = "github.com";

/// An oracle-observed agent-phase host outside the granted set —
/// withheld and logged (Q-X39a, ruling S-3); never silently granted.
#[derive(Debug, Clone, PartialEq)]
pub struct WithheldHost {
    /// The host.
    pub host: String,
    /// The recorded reason.
    pub reason: String,
}

/// The frozen host list — the profile's entire granted/denied host
/// surface (§3.5; REQ-XB-116).
#[derive(Debug, Clone, PartialEq)]
pub struct Allowlist {
    /// Agent-phase grants (first pass: apt ∪ registries only).
    pub agent: BTreeSet<String>,
    /// Verify-phase grants (agent base ∪ `tests/*` scan ∪
    /// oracle-observed verify hosts).
    pub verify: BTreeSet<String>,
    /// Always-denied hosts (answer-key + dedicated mirrors).
    pub permanent_denies: BTreeSet<String>,
    /// Withheld-and-logged hosts (denied at the mediator; recorded
    /// with their reasons).
    pub withheld: Vec<WithheldHost>,
}

impl Allowlist {
    /// The Q-X55 first-pass derivation: agent = apt ∪ registries (no
    /// `github.com`, no named-host extension); verify = agent ∪ the
    /// static `tests/*` scan ∪ oracle-observed verify hosts; denies =
    /// the answer-key hosts ∪ the declared dedicated mirrors.
    pub fn first_pass(
        family: ImageFamily,
        verify_extra: BTreeSet<String>,
        mirror_denies: &[String],
        withheld: Vec<WithheldHost>,
    ) -> Allowlist {
        let agent: BTreeSet<String> = family
            .apt_hosts()
            .union(&registry_hosts())
            .cloned()
            .collect();
        let verify: BTreeSet<String> = agent.union(&verify_extra).cloned().collect();
        let mut permanent_denies: BTreeSet<String> =
            PERMANENT_DENY_HOSTS.iter().map(|s| s.to_string()).collect();
        permanent_denies.extend(mirror_denies.iter().cloned());
        Allowlist {
            agent,
            verify,
            permanent_denies,
            withheld,
        }
    }

    /// The canonical list document (the freeze digest's basis).
    pub fn to_json(&self) -> Json {
        let strs =
            |s: &BTreeSet<String>| Json::Arr(s.iter().map(|h| Json::str(h.clone())).collect());
        Json::obj([
            ("agent", strs(&self.agent)),
            ("verify", strs(&self.verify)),
            ("permanent_denies", strs(&self.permanent_denies)),
            (
                "withheld",
                Json::Arr(
                    self.withheld
                        .iter()
                        .map(|w| {
                            Json::obj([
                                ("host", Json::str(w.host.clone())),
                                ("reason", Json::str(w.reason.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    /// The list digest — `idp` over the canonical list document.
    pub fn digest(&self) -> String {
        idp_id(
            &format!("{FREEZE_SCHEMA}.list"),
            self.to_json().to_canonical_string().as_bytes(),
        )
    }
}

/// `scan_hosts_in_text(text)` — the static host scan the verify grants
/// derive over `tests/*` bytes (ruling S-3): every `http(s)://<host>`
/// authority, normalised lowercase, no userinfo/port/path.
pub fn scan_hosts_in_text(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for token in text.split(|c: char| !c.is_ascii_alphanumeric() && !"./:-_".contains(c)) {
        for prefix in ["https://", "http://"] {
            if let Some(rest) = token.strip_prefix(prefix) {
                let authority = rest.split('/').next().unwrap_or("");
                let host = authority
                    .rsplit('@')
                    .next()
                    .unwrap_or(authority)
                    .split(':')
                    .next()
                    .unwrap_or("");
                if !host.is_empty() && host.contains('.') {
                    out.insert(host.to_lowercase());
                }
            }
        }
    }
    out
}

/// The typed exclusion basis — `runner_infeasible:allowlist_gap{host}`
/// (§3.5, ruling S-3; the outcome-blind basis family of
/// `runner_infeasible`).
pub fn allowlist_gap_basis(host: &str) -> String {
    format!("runner_infeasible:allowlist_gap{{{host}}}")
}

/// `check_agent_hosts(required, list)` — a task needing an agent-phase
/// host outside the first-pass set is excluded on the typed basis
/// (replaced from the seeded order); the first missing host names the
/// gap.
pub fn check_agent_hosts(required: &[String], list: &Allowlist) -> Result<(), AllowlistGap> {
    for h in required {
        let n = hharness_containment::policy::normalize_host(h);
        if !list.agent.contains(&n) {
            return Err(AllowlistGap {
                host: n.clone(),
                basis: allowlist_gap_basis(&n),
            });
        }
    }
    Ok(())
}

/// The exclusion — the named host plus the typed basis spelling.
#[derive(Debug, Clone, PartialEq)]
pub struct AllowlistGap {
    /// The ungranted agent-phase host.
    pub host: String,
    /// `runner_infeasible:allowlist_gap{host}`.
    pub basis: String,
}

// ── the allowlist freeze record (REQ-XB-116) ────────────────────────────────

/// `hh-egress-allowlist-freeze/1` — the frozen list + its digest,
/// recorded before any paid slot. `protocol_id` is the opaque key the
/// refusal hangs on (`comparative_protocol/1`, R6.18, mints the value
/// later — the record treats it as a verbatim string): a candidate
/// freeze under an unchanged `protocol_id` **and** `epoch` with a
/// different list digest refuses `allowlist_changed` (a later change is
/// a dated amendment and a **new epoch**, never a silent list swap).
#[derive(Debug, Clone, PartialEq)]
pub struct AllowlistFreeze {
    /// The protocol coordinate (opaque; R6.18 mints the real id).
    pub protocol_id: String,
    /// The freeze epoch (an amendment is a new epoch).
    pub epoch: u64,
    /// The frozen list.
    pub list: Allowlist,
    /// `list.digest()` recorded at freeze time.
    pub digest: String,
}

/// A freeze refusal — typed, never a relabel.
#[derive(Debug, Clone, PartialEq)]
pub struct FreezeRefusal {
    /// `allowlist_changed` — a changed list under an unchanged key.
    pub code: &'static str,
    /// The committed digest.
    pub committed: String,
    /// The candidate digest.
    pub candidate: String,
}

impl AllowlistFreeze {
    /// `freeze(protocol_id, epoch, list)` — computes the digest.
    pub fn freeze(protocol_id: &str, epoch: u64, list: Allowlist) -> AllowlistFreeze {
        let digest = list.digest();
        AllowlistFreeze {
            protocol_id: protocol_id.to_string(),
            epoch,
            list,
            digest,
        }
    }

    /// The canonical record (`{schema, protocol_id, epoch, list, digest}`).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(FREEZE_SCHEMA)),
            ("protocol_id", Json::str(self.protocol_id.clone())),
            ("epoch", Json::Int(self.epoch as i64)),
            ("list", self.list.to_json()),
            ("digest", Json::str(self.digest.clone())),
        ])
    }

    /// `check_candidate(committed, protocol_id, epoch, candidate)` — the
    /// changed-list refusal: an unchanged `protocol_id`+`epoch` with a
    /// different digest refuses. A different epoch is a dated
    /// amendment's new freeze, not this check's refusal.
    pub fn check_candidate(
        committed: &AllowlistFreeze,
        protocol_id: &str,
        epoch: u64,
        candidate: &Allowlist,
    ) -> Result<(), FreezeRefusal> {
        if committed.protocol_id == protocol_id && committed.epoch == epoch {
            let cand = candidate.digest();
            if cand != committed.digest {
                return Err(FreezeRefusal {
                    code: "allowlist_changed",
                    committed: committed.digest.clone(),
                    candidate: cand,
                });
            }
        }
        Ok(())
    }
}

// ── the cell-side spec (relay + socket mounts) ──────────────────────────────

/// `RelayPair{name, port}` — one closed relay pair: the in-cell
/// loopback port the relay listens on bridged to the named socket
/// (`model` → `/run/hh/model.sock`, `egress` → `/run/hh/egress.sock`).
#[derive(Debug, Clone, PartialEq)]
pub struct RelayPair {
    /// `model | egress` (closed set).
    pub name: String,
    /// The in-cell loopback port.
    pub port: u16,
}

impl RelayPair {
    /// The named socket path (the closed pair map).
    pub fn socket_path(&self) -> &'static str {
        match self.name.as_str() {
            "model" => MODEL_SOCK_PATH,
            _ => EGRESS_SOCK_PATH,
        }
    }
}

/// `MediatedCell` — `ext["mediated_cell"]`: the two socket bind-mounts
/// (host path per closed name) and the relay declaration (host binary +
/// pairs). Strict decode — a partial/unknown spec refuses, never a
/// half-mounted cell.
#[derive(Debug, Clone, PartialEq)]
pub struct MediatedCell {
    /// `model|egress` → the host-side socket path (exactly the closed
    /// pair).
    pub socket_mounts: BTreeMap<String, String>,
    /// The host path of the statically linked `hh-cell-relay` binary.
    pub relay_bin: String,
    /// The declared relay pairs (exactly one per closed name).
    pub pairs: Vec<RelayPair>,
}

fn closed_name(name: &str) -> Result<String, EnvError> {
    if RELAY_PAIR_NAMES.contains(&name) {
        Ok(name.to_string())
    } else {
        Err(unsupported(format!(
            "mediated_cell: `{name}` is outside the closed pair set {{model, egress}}"
        )))
    }
}

impl MediatedCell {
    /// `from_record(record) → Option<MediatedCell>` — strict decode of
    /// `ext["mediated_cell"]`.
    pub fn from_record(record: &EnvironmentRecord) -> Result<Option<MediatedCell>, EnvError> {
        match record.ext.get(MEDIATED_CELL_FIELD) {
            None => Ok(None),
            Some(j) => Self::from_json(j).map(Some),
        }
    }

    /// `from_json(j)` — the strict member decode.
    pub fn from_json(j: &Json) -> Result<MediatedCell, EnvError> {
        let m = jobj(j).ok_or_else(|| unsupported("mediated_cell must be an object"))?;
        for k in m.keys() {
            if !["socket_mounts", "relay"].contains(&k.as_str()) {
                return Err(unsupported(format!("mediated_cell: unknown member {k}")));
            }
        }
        let sm = m
            .get("socket_mounts")
            .and_then(jobj)
            .ok_or_else(|| unsupported("mediated_cell.socket_mounts must be an object"))?;
        let mut socket_mounts = BTreeMap::new();
        for (k, v) in sm {
            let name = closed_name(k)?;
            let host = v
                .as_str()
                .ok_or_else(|| unsupported("mediated_cell.socket_mounts.* must be strings"))?;
            if host.is_empty() {
                return Err(unsupported(
                    "mediated_cell.socket_mounts.* must be non-empty",
                ));
            }
            socket_mounts.insert(name, host.to_string());
        }
        for name in RELAY_PAIR_NAMES {
            if !socket_mounts.contains_key(name) {
                return Err(unsupported(format!(
                    "mediated_cell.socket_mounts: `{name}` mount required"
                )));
            }
        }
        let relay = m
            .get("relay")
            .and_then(jobj)
            .ok_or_else(|| unsupported("mediated_cell.relay must be an object"))?;
        for k in relay.keys() {
            if !["host_bin", "pairs"].contains(&k.as_str()) {
                return Err(unsupported(format!(
                    "mediated_cell.relay: unknown member {k}"
                )));
            }
        }
        let host_bin = relay
            .get("host_bin")
            .and_then(Json::as_str)
            .ok_or_else(|| unsupported("mediated_cell.relay.host_bin must be a string"))?
            .to_string();
        if host_bin.is_empty() {
            return Err(unsupported(
                "mediated_cell.relay.host_bin must be non-empty",
            ));
        }
        let pairs_j = relay
            .get("pairs")
            .and_then(jarr)
            .ok_or_else(|| unsupported("mediated_cell.relay.pairs must be an array"))?;
        let mut pairs = Vec::new();
        for p in pairs_j {
            let pm = jobj(p)
                .ok_or_else(|| unsupported("mediated_cell.relay.pairs[] must be objects"))?;
            let name = closed_name(
                pm.get("name")
                    .and_then(Json::as_str)
                    .ok_or_else(|| unsupported("relay.pairs[].name required"))?,
            )?;
            let port = pm.get("port").and_then(Json::as_int).unwrap_or(0);
            if !(1..=65535).contains(&port) {
                return Err(unsupported("relay.pairs[].port must be 1..=65535"));
            }
            pairs.push(RelayPair {
                name,
                port: port as u16,
            });
        }
        for name in RELAY_PAIR_NAMES {
            if pairs.iter().filter(|p| p.name == name).count() != 1 {
                return Err(unsupported(format!(
                    "mediated_cell.relay.pairs: exactly one `{name}` pair required"
                )));
            }
        }
        Ok(MediatedCell {
            socket_mounts,
            relay_bin: host_bin,
            pairs,
        })
    }

    /// The canonical record echo.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "socket_mounts",
                Json::Obj(
                    self.socket_mounts
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                        .collect(),
                ),
            ),
            (
                "relay",
                Json::obj([
                    ("host_bin", Json::str(self.relay_bin.clone())),
                    (
                        "pairs",
                        Json::Arr(
                            self.pairs
                                .iter()
                                .map(|p| {
                                    Json::obj([
                                        ("name", Json::str(p.name.clone())),
                                        ("port", Json::Int(p.port as i64)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                ]),
            ),
        ])
    }

    /// `helper_args()` — the spawn-argv tail: two `--mount-socket`
    /// flags, the relay binary mount, and the closed `--relay-pair`s
    /// (fixed order).
    pub fn helper_args(&self) -> Vec<String> {
        let mut a = Vec::new();
        for name in RELAY_PAIR_NAMES {
            if let Some(h) = self.socket_mounts.get(name) {
                a.push("--mount-socket".to_string());
                a.push(format!("{name}={h}"));
            }
        }
        a.push("--relay-bin".to_string());
        a.push(self.relay_bin.clone());
        for p in &self.pairs {
            a.push("--relay-pair".to_string());
            a.push(format!("{}={}", p.name, p.port));
        }
        a
    }
}

// ── the cell env projection (REQ-XB-115) ────────────────────────────────────

/// Env-member names a cell projection must never carry — the kernel
/// attribution token's only channels are the token column of the
/// dispatch request and the bridged-channel preface (X-05; the slot
/// token is a routing credential only).
const FORBIDDEN_ENV_NAMES: [&str; 3] = [
    "HH_ATTRIBUTION_TOKEN",
    "ATTRIBUTION_TOKEN",
    "HH_KERNEL_TOKEN",
];

/// `cell_env_projection(model_port, slot_token)` — the routing hints the
/// cell carries (§3.5 "Environment hints"): `http(s)_proxy` at the
/// registry relay port, the mandatory `no_proxy`, the proxy base URLs
/// and the slot token — never a kernel attribution token.
pub fn cell_env_projection(
    model_port: u16,
    slot_token: &str,
) -> Result<BTreeMap<String, String>, EnvError> {
    if slot_token.is_empty() {
        return Err(unsupported(
            "cell env: the slot token is required (the model route's routing credential)",
        ));
    }
    if !(1..=65535).contains(&model_port) {
        return Err(unsupported("cell env: model relay port out of range"));
    }
    let mut env = BTreeMap::new();
    let proxy = format!("http://127.0.0.1:{EGRESS_RELAY_PORT}");
    env.insert("http_proxy".to_string(), proxy.clone());
    env.insert("https_proxy".to_string(), proxy.clone());
    env.insert("HTTP_PROXY".to_string(), proxy.clone());
    env.insert("HTTPS_PROXY".to_string(), proxy);
    // Mandatory — without it harness clients would proxy localhost
    // (the model route itself) through the registry relay.
    env.insert("no_proxy".to_string(), "127.0.0.1,localhost".to_string());
    env.insert("NO_PROXY".to_string(), "127.0.0.1,localhost".to_string());
    env.insert(
        "OPENAI_BASE_URL".to_string(),
        format!("http://127.0.0.1:{model_port}/v1"),
    );
    env.insert(
        "ANTHROPIC_BASE_URL".to_string(),
        format!("http://127.0.0.1:{model_port}"),
    );
    env.insert("OPENAI_API_KEY".to_string(), slot_token.to_string());
    env.insert("ANTHROPIC_API_KEY".to_string(), slot_token.to_string());
    validate_cell_env(&env)?;
    Ok(env)
}

/// `validate_cell_env(env)` — the projection guard: `no_proxy` present
/// and covering loopback (a missing `no_proxy` refuses); no member
/// named like an attribution/kernel token.
pub fn validate_cell_env(env: &BTreeMap<String, String>) -> Result<(), EnvError> {
    match env.get("no_proxy") {
        Some(v) if v.contains("127.0.0.1") && v.contains("localhost") => {}
        _ => {
            return Err(unsupported(
                "cell env: mandatory no_proxy=127.0.0.1,localhost missing or incomplete",
            ))
        }
    }
    for k in env.keys() {
        if FORBIDDEN_ENV_NAMES.iter().any(|f| f == &k.as_str()) {
            return Err(unsupported(format!(
                "cell env: `{k}` is a kernel-token member — never projected into the cell"
            )));
        }
    }
    Ok(())
}

// ── the sealed phase layers (REQ-XB-113; F10) ───────────────────────────────

fn allow_rule(host: &str, phase: &str, digest: &str, at: u64) -> EgressRule {
    EgressRule {
        host: HostPattern::parse(&format!("**.{host}")).unwrap_or(HostPattern::Global),
        ports: vec![80, 443],
        protocols: vec![EgressProtocol::Http, EgressProtocol::HttpsConnect],
        methods: ["CONNECT", "GET", "POST", "PUT", "HEAD", "OPTIONS"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        decision: RuleDecision::Allow,
        credential_bindings: vec![],
        justification: Some(Text::new(
            format!("{phase}-phase registry grant — allowlist freeze {digest}"),
            "hh-env/egress_allowlist",
            ProvenanceRecord::kernel("hh-env/egress_allowlist", at),
        )),
        provenance: None,
    }
}

fn deny_rule(host: &str, at: u64) -> EgressRule {
    EgressRule {
        host: HostPattern::parse(&format!("**.{host}")).unwrap_or(HostPattern::Global),
        ports: vec![],
        protocols: vec![],
        methods: vec![],
        decision: RuleDecision::Deny,
        credential_bindings: vec![],
        justification: Some(Text::new(
            "permanent deny — answer-key/dedicated-mirror host (§3.5)".to_string(),
            "hh-env/egress_allowlist",
            ProvenanceRecord::kernel("hh-env/egress_allowlist", at),
        )),
        provenance: None,
    }
}

/// `phase_layer(base, phase, grants, sockets, digest, at)` — one sealed
/// layer document for `ext["phase_schedule"][phase]`: `principal`
/// authority (a loosening schedule entry names principal — the meet
/// entitles its allow-rule additions; its socket/rule omissions narrow
/// for free). Deny rules re-declare the base's permanent denies (deny
/// unions; the redeclaration is belt-and-suspenders).
fn phase_layer(
    base: &ContainmentPolicy,
    phase: &str,
    grants: &BTreeSet<String>,
    sockets: &[&str],
    digest: &str,
    at: u64,
) -> ContainmentPolicy {
    let mut l = base.clone();
    l.net.rules = grants
        .iter()
        .map(|h| allow_rule(h, phase, digest, at))
        .collect();
    l.net.rules.extend(
        base.net
            .rules
            .iter()
            .filter(|r| r.decision == RuleDecision::Deny)
            .cloned(),
    );
    l.net.unix_sockets.allow = sockets.iter().map(|s| s.to_string()).collect();
    l.residual_channels = base.residual_channels.clone();
    l.ext = BTreeMap::new();
    l.provenance = ProvenanceRecord {
        authority: AuthorityClass::Principal,
        ..ProvenanceRecord::kernel("hh-env/egress_allowlist.phase_layer", at)
    };
    l.compute_ids();
    l
}

/// `build_phase_schedule(base, list, digest, at)` — the sealed
/// `ext["phase_schedule"]` member: `{setup, agent, verify}` each
/// `{basis: "policy_rule", layer: <doc>}`. Setup grants no hosts and
/// keeps both sockets (the mounts exist for the cell's life; a narrower
/// setup grant would strand the agent layer's re-grant behind a
/// kernel-only entitlement). Agent grants the agent list on both
/// sockets. Verify grants the verify list on the egress socket only —
/// the model socket narrows closed.
pub fn build_phase_schedule(
    base: &ContainmentPolicy,
    list: &Allowlist,
    digest: &str,
    at: u64,
) -> Json {
    let both = [MODEL_SOCK_PATH, EGRESS_SOCK_PATH];
    let egress_only = [EGRESS_SOCK_PATH];
    let entry = |layer: ContainmentPolicy| {
        Json::obj([
            ("basis", Json::str("policy_rule")),
            ("layer", layer.to_json()),
        ])
    };
    Json::obj([
        (
            "setup",
            entry(phase_layer(
                base,
                "setup",
                &BTreeSet::new(),
                &both,
                digest,
                at,
            )),
        ),
        (
            "agent",
            entry(phase_layer(base, "agent", &list.agent, &both, digest, at)),
        ),
        (
            "verify",
            entry(phase_layer(
                base,
                "verify",
                &list.verify,
                &egress_only,
                digest,
                at,
            )),
        ),
    ])
}

// ── the admission ────────────────────────────────────────────────────────────

/// The profile declaration — the frozen allowlist plus the cell-side
/// coordinates (socket mounts, relay, model port, slot token, the
/// `protocol_id`/`epoch` coordinate of the freeze).
#[derive(Debug, Clone)]
pub struct MediatedDecl {
    /// The apt mirror family (base list per image family).
    pub family: ImageFamily,
    /// The frozen allowlist.
    pub allowlist: Allowlist,
    /// The freeze coordinate (verbatim record members).
    pub protocol_id: String,
    /// The freeze epoch.
    pub epoch: u64,
    /// The mediated-cell spec (socket mounts + relay).
    pub cell: MediatedCell,
    /// The in-cell model relay port.
    pub model_port: u16,
    /// The slot token (the model route's routing credential).
    pub slot_token: String,
}

/// `admit_allowlist_mediated(record, policy, decl, at)` — the
/// transactional admission (REQ-XB-113; AR-05): mutates a candidate to
/// `net.mode = mediated` + the closed socket grants + permanent deny
/// rules + the sealed `phase_schedule` + the declared residuals + the
/// env routing hints, validates, then commits the policy and stamps the
/// record. A refused admission leaves both byte-identical.
///
/// Refusals (`Unsupported{egress_profile.allowlist_mediated}` or the
/// validator's verbatim `Attach(PolicyInvalid)`): non-`local_container`
/// record; a relay pair or socket mount outside the closed
/// `{model, egress}` set; an env projection missing the mandatory
/// `no_proxy`; an empty slot token; a `github.com` agent-phase grant
/// (always denied in the first pass); the resulting policy failing
/// `validate()`.
pub fn admit_allowlist_mediated(
    record: &mut EnvironmentRecord,
    policy: &mut ContainmentPolicy,
    decl: &MediatedDecl,
    at: u64,
) -> Result<(), EnvError> {
    if record.class != EnvironmentClass::LocalContainer {
        return Err(unsupported(format!(
            "the allowlist_mediated profile admits a local_container cell — \
             {} has no podman socket mount",
            record.class.as_str()
        )));
    }
    if decl.allowlist.agent.contains(GITHUB)
        || decl
            .allowlist
            .agent
            .iter()
            .any(|h| h.ends_with(".github.com"))
    {
        return Err(unsupported(
            "a github.com agent-phase grant is outside the first pass (Q-X55) — \
             denied whenever it is not granted, and never granted in the first pass",
        ));
    }
    let digest = decl.allowlist.digest();
    let env = cell_env_projection(decl.model_port, &decl.slot_token)?;

    // Mutate a candidate, validate, then commit — a refused admission
    // leaves the policy byte-identical.
    let mut candidate = policy.clone();
    candidate.net.mode = NetMode::Mediated;
    candidate.net.default_unmatched = DefaultUnmatched::Deny;
    candidate.net.non_public_destinations = NonPublic::Deny;
    candidate.net.local_binding = true; // the relay binds loopback ports in-cell
    candidate.net.unix_sockets.mode = UnixSocketMode::AllowListed;
    candidate.net.unix_sockets.allow =
        vec![MODEL_SOCK_PATH.to_string(), EGRESS_SOCK_PATH.to_string()];
    candidate.net.dns.resolver = DnsResolver::Mediator;
    candidate.net.rules = decl
        .allowlist
        .permanent_denies
        .iter()
        .map(|h| deny_rule(h, at))
        .collect();
    // Routing hints — recorded as the `proc.env: proxy_env_is_not_mediation`
    // lowering loss by the backend (never claimed as mediation).
    for (k, v) in &env {
        candidate.proc.env.set.insert(k.clone(), v.clone());
    }
    candidate.residual_channels.extend([
        ResidualChannel {
            kind: ResidualKind::UnixSocket,
            statement: Text::new(
                format!(
                    "allowlist_mediated: {MODEL_SOCK_PATH} — the model route, \
                     bridged by hh-cell-relay 127.0.0.1:{} ⇄ host proxy",
                    decl.model_port
                ),
                "hh-env/egress_allowlist",
                ProvenanceRecord::kernel("hh-env/egress_allowlist", at),
            ),
            owner: "model proxy slot listener (R6.10/R6.16)".to_string(),
            closer: "slot listener closed at agent end; socket unmounted at teardown".to_string(),
        },
        ResidualChannel {
            kind: ResidualKind::UnixSocket,
            statement: Text::new(
                format!(
                    "allowlist_mediated: {EGRESS_SOCK_PATH} — the registry route, \
                     bridged by hh-cell-relay 127.0.0.1:{EGRESS_RELAY_PORT} ⇄ egress mediator"
                ),
                "hh-env/egress_allowlist",
                ProvenanceRecord::kernel("hh-env/egress_allowlist", at),
            ),
            owner: "hh-egress-mediator phase run".to_string(),
            closer: "mediator listener closed at slot end; socket unmounted at teardown"
                .to_string(),
        },
        ResidualChannel {
            kind: ResidualKind::TlsOpaque,
            statement: Text::new(
                "CONNECT tunnels are not terminated at the mediator — \
                 registry TLS bodies are opaque to the boundary"
                    .to_string(),
                "hh-env/egress_allowlist",
                ProvenanceRecord::kernel("hh-env/egress_allowlist", at),
            ),
            owner: "egress mediator (tunnel leg)".to_string(),
            closer: "tunnel close per connection; recorded bytes up/down".to_string(),
        },
        ResidualChannel {
            kind: ResidualKind::ApprovedHostBody,
            statement: Text::new(
                "plain-forward request bodies to allow-listed shared hosts are \
                 opaque at the boundary; the readers(body) closer guards every allow"
                    .to_string(),
                "hh-env/egress_allowlist",
                ProvenanceRecord::kernel("hh-env/egress_allowlist", at),
            ),
            owner: "egress mediator (forward leg)".to_string(),
            closer: "per-request readers(body) coverage check".to_string(),
        },
        ResidualChannel {
            kind: ResidualKind::DnsNames,
            statement: Text::new(
                "the mediator resolves names (dns.resolver = mediator); host \
                 names are visible on the boundary, resolved answers re-checked"
                    .to_string(),
                "hh-env/egress_allowlist",
                ProvenanceRecord::kernel("hh-env/egress_allowlist", at),
            ),
            owner: "egress mediator resolver".to_string(),
            closer: "decide-once consume-once recheck per request".to_string(),
        },
        ResidualChannel {
            kind: ResidualKind::SideChannel,
            statement: Text::new(
                "registry-channel attribution is per-socket identity: the \
                 per-slot socket binds slot and phase; the mediator presents the \
                 slot's phase effect token internally — no token enters the cell"
                    .to_string(),
                "hh-env/egress_allowlist",
                ProvenanceRecord::kernel("hh-env/egress_allowlist", at),
            ),
            owner: "hh-egress-mediator (per-slot socket identity)".to_string(),
            closer: "attribution folds over security.egress rows by slot socket".to_string(),
        },
    ]);
    candidate.ext.insert(
        "phase_schedule".to_string(),
        build_phase_schedule(&candidate, &decl.allowlist, &digest, at),
    );
    candidate.compute_ids();
    candidate
        .validate()
        .map_err(|e| EnvError::Attach(AttachError::PolicyInvalid(e)))?;
    *policy = candidate;

    // The record stamps — the honest labels plus the profile's cell
    // spec and the freeze coordinate. A mediated cell is never labelled
    // `none`/`bridge`: the stamps land only after the policy validates.
    record.ext.insert(
        "egress_profile".to_string(),
        Json::str(EGRESS_PROFILE_ALLOWLIST_MEDIATED),
    );
    record
        .ext
        .insert("cell_network".to_string(), Json::str(CELL_NETWORK_NONE));
    record.ext.insert(
        "policy_net_mode".to_string(),
        Json::str(POLICY_NET_MODE_MEDIATED),
    );
    record
        .ext
        .insert(PER_PHASE_FIELD.to_string(), Json::str("supported"));
    record
        .ext
        .insert(MEDIATED_CELL_FIELD.to_string(), decl.cell.to_json());
    record.ext.insert(
        "allowlist".to_string(),
        Json::obj([
            ("protocol_id", Json::str(decl.protocol_id.clone())),
            ("epoch", Json::Int(decl.epoch as i64)),
            ("digest", Json::str(digest)),
            ("result_label", Json::str(RESULT_LABEL)),
        ]),
    );
    Ok(())
}

/// `cell_record_section(policy, phase, mediator)` — the slot record's
/// `cell` egress members (§3.3 `cell` section): the profile, the
/// per-phase policy ids (the sealed layers' `policy_id`s), the declared
/// residuals, the mediator's per-host allow/deny/byte folds, and the
/// model-socket flag (`open` in setup/agent, `closed` in verify).
pub fn cell_record_section(
    policy: &ContainmentPolicy,
    phase: &str,
    mediator: &BTreeMap<String, Json>,
) -> Json {
    let mut phase_ids = BTreeMap::new();
    if let Some(sched) = policy.ext.get("phase_schedule").and_then(jobj) {
        for (p, entry) in sched {
            if let Some(l) = entry.get("layer") {
                if let Ok(layer) = ContainmentPolicy::from_json(l) {
                    phase_ids.insert(p.clone(), Json::str(layer.policy_id));
                }
            }
        }
    }
    Json::obj([
        (
            "egress_profile",
            Json::str(EGRESS_PROFILE_ALLOWLIST_MEDIATED),
        ),
        ("net_mode", Json::str(policy.net.mode.as_str())),
        ("phase_policy_ids", Json::Obj(phase_ids)),
        (
            "residuals",
            Json::Arr(
                policy
                    .residual_channels
                    .iter()
                    .map(|r| Json::str(r.kind.as_str()))
                    .collect(),
            ),
        ),
        (
            "model_socket",
            Json::str(if phase == "verify" { "closed" } else { "open" }),
        ),
        ("mediator", Json::Obj(mediator.clone())),
    ])
}
