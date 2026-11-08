//! Harbor as the O surface (manifest 270/R6.17, HARBOR-O; ADR-0427
//! dated amendment — the AR-04 tool admission completed): the
//! program-wide pin (REQ-XH-014, §5.9's Q-X41 fallback rule), the
//! `uv.lock` host-venv recipe (REQ-XC-315), the runner-written
//! committed job config (`-r 0`, never `--upload`; REQ-XB-123), image
//! identity (`pull by digest → local tag → --no-force-build →
//! post-job tag→digest assert`; REQ-XB-124) and the read-only job-tree
//! ingest (`hh-harbor-ingest/1` + one `hh-harbor-trial/1` per trial).
//!
//! The `reproof` module owns the observe → freeze → prove driver these
//! records feed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_bench::adapter::is_pinned_image_digest;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::disposition::{Disposition, EndCause, InconclusiveCause, WithheldReason};
use crate::facts::{harbor_cause_map, HarborErrorCause};
use crate::json_ext::JsonExt;
use crate::verdict::{parse_reward_files, ChannelOutcome, RewardFileSet};

// ── the record schemas ───────────────────────────────────────────────────────

/// `hh-harbor-pin/1` — the program-wide pin record (AR-04's record).
pub const PIN_SCHEMA: &str = "hh-harbor-pin/1";
/// `hh-harbor-venv/1` — the host-venv record (REQ-XC-315).
pub const VENV_SCHEMA: &str = "hh-harbor-venv/1";
/// `hh-harbor-job-config/1` — the runner-written committed job config.
pub const JOB_CONFIG_SCHEMA: &str = "hh-harbor-job-config/1";
/// `hh-harbor-ingest/1` — one record per ingested Harbor job tree.
pub const INGEST_SCHEMA: &str = "hh-harbor-ingest/1";
/// `hh-harbor-trial/1` — one record per ingested trial directory.
pub const TRIAL_SCHEMA: &str = "hh-harbor-trial/1";

/// The admitted program-wide pin (Q-X41 fallback, dispatch live-read
/// 2026-11-07 — see the committed `hh-harbor-pin/1` instance).
pub const HARBOR_PIN: &str = "ff2a1ef48070a8df374669138e4d4db79b77f013";
/// `skip_install`'s landing commit (§5.9; design/34 F3).
pub const SKIP_INSTALL_COMMIT: &str = "0a576fa5";
/// `v0.25.0`'s peeled commit — the newest stable tag at the live read.
pub const V0_25_0_COMMIT: &str = "fdba28fc411e47e8765420a1a063b4918fce3def";
/// The upstream repository the dispatch read queried.
pub const HARBOR_REPO: &str = "github.com/harbor-framework/harbor";

/// A typed refusal — never a silent downgrade.
#[derive(Debug, Clone, PartialEq)]
pub struct HarborError {
    /// The refusal spelling.
    pub detail: String,
}

fn err(d: impl Into<String>) -> HarborError {
    HarborError { detail: d.into() }
}

impl std::fmt::Display for HarborError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "harbor: {}", self.detail)
    }
}

impl std::error::Error for HarborError {}

fn need<'a>(m: &'a BTreeMap<String, Json>, k: &str) -> Result<&'a Json, HarborError> {
    m.get(k).ok_or_else(|| err(format!("{k} required")))
}

fn need_str(m: &BTreeMap<String, Json>, k: &str) -> Result<String, HarborError> {
    need(m, k)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| err(format!("{k} must be a string")))
}

fn no_unknown(m: &BTreeMap<String, Json>, allowed: &[&str], at: &str) -> Result<(), HarborError> {
    for k in m.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(err(format!("{at}: unknown member {k}")));
        }
    }
    Ok(())
}

// ── §5.9 the pin (REQ-XH-014; Q-X41 fallback rule) ─────────────────────────────

/// Which branch of the Q-X41 fallback rule the dispatch read took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinBranch {
    /// `stable_tag` — the first stable tag containing `skip_install`.
    StableTag,
    /// `commit_fallback` — the admitted commit `ff2a1ef4`.
    CommitFallback,
    /// The rule's inputs were unread (the lookup was unavailable) — the
    /// pin is never assumed (contract Caps).
    Unread,
}

impl PinBranch {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            PinBranch::StableTag => "stable_tag",
            PinBranch::CommitFallback => "commit_fallback",
            PinBranch::Unread => "unread",
        }
    }
}

/// The dispatch live-read — the one read-only lookup the Q-X41 rule is
/// evaluated on (tags + HEAD; USD $0, no provisioning).
#[derive(Debug, Clone, PartialEq)]
pub struct LiveRead {
    /// ISO date-time the lookup ran.
    pub queried_at: String,
    /// The read performed (`git ls-remote --tags + HEAD`).
    pub method: String,
    /// The repository queried.
    pub repo: String,
    /// The newest stable tag by semver order (`None` = unread).
    pub newest_stable_tag: Option<String>,
    /// Its peeled commit.
    pub newest_stable_tag_commit: Option<String>,
    /// HEAD at the read.
    pub head: Option<String>,
    /// The rule's verdict: `no_stable_tag_contains_skip_install` |
    /// `tag:<v>` | `unread`.
    pub result: String,
}

impl LiveRead {
    /// The canonical member JSON.
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("queried_at", Json::str(&self.queried_at)),
            ("method", Json::str(&self.method)),
            ("repo", Json::str(&self.repo)),
            ("result", Json::str(&self.result)),
        ];
        if let Some(t) = &self.newest_stable_tag {
            m.push(("newest_stable_tag", Json::str(t)));
        }
        if let Some(c) = &self.newest_stable_tag_commit {
            m.push(("newest_stable_tag_commit", Json::str(c)));
        }
        if let Some(h) = &self.head {
            m.push(("head", Json::str(h)));
        }
        Json::obj(m)
    }
}

/// Semver ordering on `vX.Y.Z` spellings (`None` for non-conforming).
fn semver_key(tag: &str) -> Option<(u64, u64, u64)> {
    let t = tag.strip_prefix('v')?;
    let mut parts = t.split('.');
    let maj = parts.next()?.parse().ok()?;
    let min = parts.next()?.parse().ok()?;
    let pat = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((maj, min, pat))
}

/// `hh-harbor-pin/1` — the program-wide pin, its rule branch and the
/// dated live read it was chosen on. Carried into AR-04's record by the
/// dated amendment; slot and trial records name `pin`.
#[derive(Debug, Clone, PartialEq)]
pub struct HarborPin {
    /// The pin — a 40-hex commit or a `vX.Y.Z` tag.
    pub pin: String,
    /// `commit | tag`.
    pub kind: String,
    /// Whether `skip_install` exists at the pin.
    pub contains_skip_install: bool,
    /// The rule branch taken.
    pub branch: PinBranch,
    /// The dispatch live-read evidence.
    pub live_read: LiveRead,
    /// The E1 F13 agent-relevant diff record ref — attached when the
    /// rule requires one ([`Self::diff_required`]), absent otherwise.
    pub diff_record_ref: Option<String>,
    /// The AR-04 record this pin amends (`ADR-0427`).
    pub adr: String,
}

impl HarborPin {
    /// The pinned commit's record shape at the 2026-11-07 dispatch read
    /// (Q-X41 fallback; no stable tag contains `skip_install`).
    pub fn admitted(queried_at: &str, head: &str) -> HarborPin {
        HarborPin {
            pin: HARBOR_PIN.to_string(),
            kind: "commit".to_string(),
            contains_skip_install: true,
            branch: PinBranch::CommitFallback,
            live_read: LiveRead {
                queried_at: queried_at.to_string(),
                method: "git ls-remote --tags + HEAD".to_string(),
                repo: HARBOR_REPO.to_string(),
                newest_stable_tag: Some("v0.25.0".to_string()),
                newest_stable_tag_commit: Some(V0_25_0_COMMIT.to_string()),
                head: Some(head.to_string()),
                result: "no_stable_tag_contains_skip_install".to_string(),
            },
            diff_record_ref: None,
            adr: "ADR-0427".to_string(),
        }
    }

    /// REQ-XH-014's diff obligation: a *tag later than v0.25.0* requires
    /// the E1 F13 agent-relevant diff re-run from v0.25.0 for every
    /// admitted adapter. A commit pin or a tag ≤ v0.25.0 does not.
    pub fn diff_required(&self) -> bool {
        if self.kind != "tag" {
            return false;
        }
        match (semver_key(&self.pin), semver_key("v0.25.0")) {
            (Some(k), Some(base)) => k > base,
            _ => false,
        }
    }

    /// Whether the rule was honestly evaluated (a live read exists and
    /// names its verdict). An `unread` branch never claims a pin choice.
    pub fn read_evaluated(&self) -> bool {
        self.branch != PinBranch::Unread && self.live_read.result != "unread"
    }

    /// The canonical record.
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("schema", Json::str(PIN_SCHEMA)),
            ("pin", Json::str(&self.pin)),
            ("kind", Json::str(&self.kind)),
            (
                "contains_skip_install",
                Json::Bool(self.contains_skip_install),
            ),
            ("branch", Json::str(self.branch.as_str())),
            ("live_read", self.live_read.to_json()),
            ("adr", Json::str(&self.adr)),
            ("diff_required", Json::Bool(self.diff_required())),
        ];
        if let Some(d) = &self.diff_record_ref {
            m.push(("diff_record_ref", Json::str(d)));
        }
        Json::obj(m)
    }

    /// Strict decode — unknown members and a non-rule branch refuse.
    pub fn from_json(j: &Json) -> Result<HarborPin, HarborError> {
        let m = j.as_obj().ok_or_else(|| err("record must be an object"))?;
        no_unknown(
            m,
            &[
                "schema",
                "pin",
                "kind",
                "contains_skip_install",
                "branch",
                "live_read",
                "adr",
                "diff_required",
                "diff_record_ref",
            ],
            "record",
        )?;
        if need_str(m, "schema")? != PIN_SCHEMA {
            return Err(err("schema member is not hh-harbor-pin/1"));
        }
        let branch = match need_str(m, "branch")?.as_str() {
            "stable_tag" => PinBranch::StableTag,
            "commit_fallback" => PinBranch::CommitFallback,
            "unread" => PinBranch::Unread,
            other => return Err(err(format!("branch {other} outside the rule"))),
        };
        let kind = need_str(m, "kind")?;
        if kind != "commit" && kind != "tag" {
            return Err(err("kind must be commit|tag"));
        }
        let lm = need(m, "live_read")?
            .as_obj()
            .ok_or_else(|| err("live_read must be an object"))?;
        no_unknown(
            lm,
            &[
                "queried_at",
                "method",
                "repo",
                "result",
                "newest_stable_tag",
                "newest_stable_tag_commit",
                "head",
            ],
            "live_read",
        )?;
        let opt_str = |k: &str| lm.get(k).and_then(Json::as_str).map(str::to_string);
        Ok(HarborPin {
            pin: need_str(m, "pin")?,
            kind,
            contains_skip_install: m
                .get("contains_skip_install")
                .and_then(Json::as_bool)
                .ok_or_else(|| err("contains_skip_install must be a bool"))?,
            branch,
            live_read: LiveRead {
                queried_at: need_str(lm, "queried_at")?,
                method: need_str(lm, "method")?,
                repo: need_str(lm, "repo")?,
                newest_stable_tag: opt_str("newest_stable_tag"),
                newest_stable_tag_commit: opt_str("newest_stable_tag_commit"),
                head: opt_str("head"),
                result: need_str(lm, "result")?,
            },
            diff_record_ref: m
                .get("diff_record_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
            adr: need_str(m, "adr")?,
        })
    }
}

// ── REQ-XC-315 — the host venv from the pin's `uv.lock` ────────────────────────

/// The pending marker family — recorded, never fabricated. The lock
/// hash and resolved LiteLLM version land at the first build (W1); a
/// pending member never compares equal to computed bytes.
pub const PENDING_FIRST_BUILD: &str = "pending{first_build_W1}";

/// `hh-harbor-venv/1` — the host Harbor venv recipe and, once built,
/// its recorded facts: the `uv.lock` hash and the resolved LiteLLM
/// version (`uv sync --frozen`; `uv` is AR-04's admitted tool).
#[derive(Debug, Clone, PartialEq)]
pub struct VenvRecipe {
    /// The pin the venv serves (the same commit supplies Terminus-2's
    /// driver — its half is Round 8).
    pub pin: String,
    /// The recorded `uv.lock` hash, or `pending{first_build_W1}`.
    pub uv_lock_sha256: String,
    /// The resolved LiteLLM version, or `pending{first_build_W1}`.
    pub resolved_litellm: String,
    /// The verbatim sync command (`uv sync --frozen`).
    pub command: String,
}

impl VenvRecipe {
    /// The declared recipe — facts pending the first build.
    pub fn declared(pin: &str) -> VenvRecipe {
        VenvRecipe {
            pin: pin.to_string(),
            uv_lock_sha256: PENDING_FIRST_BUILD.to_string(),
            resolved_litellm: PENDING_FIRST_BUILD.to_string(),
            command: "uv sync --frozen".to_string(),
        }
    }

    /// The built record — computes the lock hash from the real bytes
    /// and names the resolved LiteLLM version verbatim.
    pub fn built(pin: &str, uv_lock: &[u8], litellm_version: &str) -> VenvRecipe {
        VenvRecipe {
            pin: pin.to_string(),
            uv_lock_sha256: format!("sha256:{}", sha256_hex(uv_lock)),
            resolved_litellm: litellm_version.to_string(),
            command: "uv sync --frozen".to_string(),
        }
    }

    /// The canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(VENV_SCHEMA)),
            ("pin", Json::str(&self.pin)),
            ("uv_lock_sha256", Json::str(&self.uv_lock_sha256)),
            ("resolved_litellm", Json::str(&self.resolved_litellm)),
            ("command", Json::str(&self.command)),
        ])
    }

    /// `check_lock(recipe, bytes)` — the lock the pin's record admits
    /// must hash to the recorded digest. A pending or mismatched hash
    /// refuses — typed, never a silent skip.
    pub fn check_lock(&self, uv_lock: &[u8]) -> Result<(), HarborError> {
        if self.uv_lock_sha256.starts_with("pending{") {
            return Err(err("uv.lock hash unrecorded — the venv was never built"));
        }
        let computed = format!("sha256:{}", sha256_hex(uv_lock));
        if computed != self.uv_lock_sha256 {
            return Err(err(format!(
                "uv.lock hash mismatch — declared {} computed {computed}",
                self.uv_lock_sha256
            )));
        }
        Ok(())
    }
}

// ── REQ-XB-123/REQ-XC-306 — the runner-written job config ─────────────────────

/// The `skip_install` leg — admitted only at a pin containing the knob;
/// carries the HH layer digest the O surface runs unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct SkipInstall {
    /// The (harness × task image) layer digest O runs.
    pub layer_digest: String,
}

/// `ORoute` — the O-surface model route (REQ-XC-326, R6.26): one
/// Harbor job per O slot, model calls only through the HH proxy via
/// the host relay on the podman bridge gateway — never Ollama
/// directly, and no key material in any Harbor environment (the slot
/// token travels by env NAME only).
#[derive(Debug, Clone, PartialEq)]
pub struct ORoute {
    /// The host-relay base URL the job's model calls target
    /// (`http://<bridge-gateway>:<port>/v1` — never an upstream host).
    pub relay_base_url: String,
    /// The env NAME carrying the per-slot token (value never in the
    /// config — `provided: yes` in records, X-05).
    pub api_key_name: String,
    /// `openai.chat` | `anthropic.messages` (the `--api-format` spelling).
    pub api_format: String,
    /// The slot token's record ref (identity, never the value).
    pub slot_token_ref: String,
    /// The slot's proxy slice in µ$ (per-slot budget — one job per slot).
    pub slice_micros: i64,
}

/// The upstream model hosts the O surface must never reach directly —
/// a host matching any marker refuses (REQ-XC-326: only the HH proxy).
pub const UPSTREAM_HOST_MARKERS: [&str; 7] = [
    "ollama.com",
    "api.ollama.com",
    "api.openai.com",
    "api.anthropic.com",
    "openai.com",
    "anthropic.com",
    ":11434",
];

/// `sweep_no_upstream(j)` — every string leaf of the job config (and,
/// via [`JobConfig::check_no_upstream`], its emitted argv): no upstream
/// model host and no credential material — an env spelling
/// `NAME=<nonempty>` on a `*_API_KEY`|`*_AUTH_TOKEN`|`*_SECRET` member
/// flags unless the value is a `$NAME` reference or the elision
/// tombstone; a bare `--api-key` flag or `api_key=<literal>` flags.
pub fn sweep_no_upstream(j: &Json) -> Vec<String> {
    fn looks_secret_name(n: &str) -> bool {
        n.ends_with("_API_KEY") || n.ends_with("_AUTH_TOKEN") || n.ends_with("_SECRET")
    }
    fn name_shaped(v: &str) -> bool {
        !v.is_empty()
            && v.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    }
    let mut out = Vec::new();
    let walk = |j: &Json, path: &str, out: &mut Vec<String>| {
        let mut stack = vec![(j, path.to_string())];
        while let Some((j, path)) = stack.pop() {
            match j {
                Json::Str(s) => {
                    if UPSTREAM_HOST_MARKERS.iter().any(|m| s.contains(m)) {
                        out.push(format!("{path}: upstream host marker in {s:?}"));
                    }
                    if s == "--api-key" || s == "--api-key=" || s.starts_with("api_key=") {
                        out.push(format!("{path}: direct key flag {s:?}"));
                    }
                    if let Some((n, v)) = s.split_once('=') {
                        if looks_secret_name(n)
                            && !v.is_empty()
                            && !v.starts_with('$')
                            && !v.starts_with("<secret")
                            && !name_shaped(v)
                        {
                            out.push(format!("{path}: key material in env {n}"));
                        }
                    }
                }
                Json::Arr(a) => {
                    for (i, v) in a.iter().enumerate() {
                        stack.push((v, format!("{path}.{i}")));
                    }
                }
                Json::Obj(m) => {
                    for (k, v) in m {
                        stack.push((v, format!("{path}.{k}")));
                    }
                }
                _ => {}
            }
        }
    };
    walk(j, "$", &mut out);
    out
}

/// `hh-harbor-job-config/1` — the committed config the runner writes
/// for each `harbor run`. `max_retries` is 0 by construction-class
/// refusal (a nonzero value decodes, then refuses — the golden carries
/// the enforced member).
#[derive(Debug, Clone, PartialEq)]
pub struct JobConfig {
    /// The program-wide Harbor pin the job runs under.
    pub pin: String,
    /// The job name (protocol/epoch-derived).
    pub job_name: String,
    /// The declared `-p` tasks directory (relative).
    pub tasks_dir: String,
    /// The `-i` task globs.
    pub tasks: Vec<String>,
    /// The agent (`oracle` for the re-proof).
    pub agent: String,
    /// The environment (`podman`).
    pub env: String,
    /// `-k` — attempts per task.
    pub n_attempts: u32,
    /// `-n` — concurrent trials.
    pub n_concurrent: u32,
    /// `-r` — retries; the pinned value is 0 (F13: retries delete the
    /// failed attempt's files; infra re-runs are new HH slots).
    pub max_retries: u32,
    /// `--no-force-build` — pinned images only.
    pub no_force_build: bool,
    /// The `skip_install` leg (REQ-XC-306) — present only at a pin
    /// containing the knob and only with the declared layer digest.
    pub skip_install: Option<SkipInstall>,
    /// The O-surface model route (REQ-XC-326) — present on the
    /// per-slot O jobs only; `None` on oracle/re-proof jobs.
    pub o_route: Option<ORoute>,
    /// `--jobs-dir` — the scratch jobs root ingest walks.
    pub jobs_dir: String,
}

/// `check_o_route(o)` — the REQ-XC-326 route rules.
fn check_o_route(o: &ORoute) -> Result<(), HarborError> {
    if !crate::smoke::PROXIED_SURFACES.contains(&o.api_format.as_str()) {
        return Err(err(format!(
            "api_format {} outside the proxied surfaces",
            o.api_format
        )));
    }
    if o.slice_micros <= 0 {
        return Err(err("slice_micros must be positive — per-slot slice"));
    }
    if o.api_key_name.is_empty()
        || !o
            .api_key_name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(err(format!(
            "api_key_name {:?} is not an env NAME — the token travels by name only",
            o.api_key_name
        )));
    }
    let u = &o.relay_base_url;
    if !u.starts_with("http://") || u.contains('@') {
        return Err(err(format!(
            "relay_base_url {u:?} — http://host:port only, no userinfo"
        )));
    }
    if UPSTREAM_HOST_MARKERS.iter().any(|m| u.contains(m)) {
        return Err(err(format!(
            "relay_base_url {u:?} carries an upstream host — O reaches the model only through the HH proxy"
        )));
    }
    Ok(())
}

/// The external-write argv the program never emits (REQ-XB-123).
pub const FORBIDDEN_ARGV: [&str; 2] = ["--upload", "--public"];

impl JobConfig {
    /// `new(…)` — the constructor-level refusals: nonzero retries,
    /// force-build, `skip_install` at a pin lacking the knob.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pin: &HarborPin,
        job_name: &str,
        tasks_dir: &str,
        tasks: Vec<String>,
        agent: &str,
        n_attempts: u32,
        n_concurrent: u32,
        jobs_dir: &str,
        skip_install: Option<SkipInstall>,
    ) -> Result<JobConfig, HarborError> {
        if skip_install.is_some() && !pin.contains_skip_install {
            return Err(err(format!(
                "skip_install declared but pin {} lacks the knob",
                pin.pin
            )));
        }
        Ok(JobConfig {
            pin: pin.pin.clone(),
            job_name: job_name.to_string(),
            tasks_dir: tasks_dir.to_string(),
            tasks,
            agent: agent.to_string(),
            env: "podman".to_string(),
            n_attempts,
            n_concurrent,
            max_retries: 0,
            no_force_build: true,
            skip_install,
            o_route: None,
            jobs_dir: jobs_dir.to_string(),
        })
    }

    /// `with_o_route(self, o)` — attach the REQ-XC-326 O-surface route:
    /// one Harbor job per O slot, the slot's token channel and slice,
    /// model calls through the host relay only. Refuses typed: the O
    /// surface runs the same layer, so `skip_install` is required; the
    /// relay URL must not carry an upstream host or credential
    /// material; `api_key_name` is a NAME, never a value; `api_format`
    /// is a proxied surface; the slice is positive.
    pub fn with_o_route(mut self, o: ORoute) -> Result<JobConfig, HarborError> {
        if self.skip_install.is_none() {
            return Err(err(
                "o_route requires skip_install — the O surface runs the same layer digest",
            ));
        }
        check_o_route(&o)?;
        self.o_route = Some(o);
        Ok(self)
    }

    /// `check_no_upstream(&self)` — the REQ-XC-326 sweep over the
    /// committed record and the emitted argv: no Ollama key or
    /// upstream host anywhere.
    pub fn check_no_upstream(&self) -> Result<(), HarborError> {
        let mut hits = sweep_no_upstream(&self.to_json());
        for a in self.to_argv("harbor")? {
            hits.extend(sweep_no_upstream(&Json::str(a)));
        }
        if hits.is_empty() {
            Ok(())
        } else {
            Err(err(format!("upstream sweep: {}", hits.join("; "))))
        }
    }

    /// `check(cfg)` — the decode-time refusals (a hand-edited record
    /// never mints a non-zero `-r` or a force-build).
    pub fn check(&self) -> Result<(), HarborError> {
        if self.max_retries != 0 {
            return Err(err(format!(
                "max_retries {} refused — Harbor retries delete the failed \
                 attempt's files; infra re-runs are new HH slots (-r 0)",
                self.max_retries
            )));
        }
        if !self.no_force_build {
            return Err(err(
                "no_force_build=false refused — images are pinned by digest",
            ));
        }
        if self.env != "podman" {
            return Err(err(format!("env {} refused — podman only", self.env)));
        }
        if let Some(o) = &self.o_route {
            check_o_route(o)?;
            if self.skip_install.is_none() {
                return Err(err(
                    "o_route without skip_install refused — same layer digest required",
                ));
            }
        }
        Ok(())
    }

    /// `check_argv(argv)` — an argv containing an external-write flag
    /// refuses; the program never emits `--upload`/`--public`.
    pub fn check_argv(argv: &[String]) -> Result<(), HarborError> {
        for a in argv {
            if FORBIDDEN_ARGV.contains(&a.as_str()) {
                return Err(err(format!(
                    "argv contains {a} — external write, never emitted \
                     (REQ-XB-123)"
                )));
            }
        }
        Ok(())
    }

    /// `to_argv(harbor_bin)` — the pinned invocation:
    /// `harbor run -p <dir> -i <task>… -a <agent> -e podman -k <R>
    /// -n <conc> -r 0 --no-force-build --jobs-dir <dir> --job-name <n>`
    /// plus `--ak skip_install=true` when the leg is admitted.
    pub fn to_argv(&self, harbor_bin: &str) -> Result<Vec<String>, HarborError> {
        self.check()?;
        let mut argv = vec![
            harbor_bin.to_string(),
            "run".to_string(),
            "-p".to_string(),
            self.tasks_dir.clone(),
        ];
        for t in &self.tasks {
            argv.push("-i".to_string());
            argv.push(t.clone());
        }
        argv.extend([
            "-a".to_string(),
            self.agent.clone(),
            "-e".to_string(),
            self.env.clone(),
            "-k".to_string(),
            self.n_attempts.to_string(),
            "-n".to_string(),
            self.n_concurrent.to_string(),
            "-r".to_string(),
            self.max_retries.to_string(),
            "--no-force-build".to_string(),
            "--jobs-dir".to_string(),
            self.jobs_dir.clone(),
            "--job-name".to_string(),
            self.job_name.clone(),
        ]);
        if self.skip_install.is_some() {
            argv.push("--ak".to_string());
            argv.push("skip_install=true".to_string());
        }
        if let Some(o) = &self.o_route {
            // design/34 §1.8 + B1 F12: the host-relay route as Harbor's
            // own spellings — the slot token travels by env NAME only.
            argv.extend([
                "--base-url".to_string(),
                o.relay_base_url.clone(),
                "--api-key-name".to_string(),
                o.api_key_name.clone(),
                "--api-format".to_string(),
                o.api_format.clone(),
            ]);
        }
        JobConfig::check_argv(&argv)?;
        Ok(argv)
    }

    /// The canonical record — committed into the ingest/job record.
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("schema", Json::str(JOB_CONFIG_SCHEMA)),
            ("pin", Json::str(&self.pin)),
            ("job_name", Json::str(&self.job_name)),
            ("tasks_dir", Json::str(&self.tasks_dir)),
            (
                "tasks",
                Json::Arr(self.tasks.iter().map(|t| Json::str(t.clone())).collect()),
            ),
            ("agent", Json::str(&self.agent)),
            ("env", Json::str(&self.env)),
            ("n_attempts", Json::Int(self.n_attempts as i64)),
            ("n_concurrent", Json::Int(self.n_concurrent as i64)),
            ("max_retries", Json::Int(self.max_retries as i64)),
            ("no_force_build", Json::Bool(self.no_force_build)),
            ("jobs_dir", Json::str(&self.jobs_dir)),
        ];
        if let Some(s) = &self.skip_install {
            m.push((
                "skip_install",
                Json::obj([
                    ("enabled", Json::Bool(true)),
                    ("layer_digest", Json::str(&s.layer_digest)),
                ]),
            ));
        }
        if let Some(o) = &self.o_route {
            m.push((
                "o_route",
                Json::obj([
                    ("relay_base_url", Json::str(&o.relay_base_url)),
                    ("api_key_name", Json::str(&o.api_key_name)),
                    ("api_format", Json::str(&o.api_format)),
                    ("slot_token_ref", Json::str(&o.slot_token_ref)),
                    ("slice_micros", Json::Int(o.slice_micros)),
                ]),
            ));
        }
        Json::obj(m)
    }

    /// Strict decode — then [`JobConfig::check`].
    pub fn from_json(j: &Json) -> Result<JobConfig, HarborError> {
        let m = j.as_obj().ok_or_else(|| err("config must be an object"))?;
        no_unknown(
            m,
            &[
                "schema",
                "pin",
                "job_name",
                "tasks_dir",
                "tasks",
                "agent",
                "env",
                "n_attempts",
                "n_concurrent",
                "max_retries",
                "no_force_build",
                "skip_install",
                "o_route",
                "jobs_dir",
            ],
            "config",
        )?;
        if need_str(m, "schema")? != JOB_CONFIG_SCHEMA {
            return Err(err("schema member is not hh-harbor-job-config/1"));
        }
        let int = |k: &str| -> Result<u32, HarborError> {
            need(m, k)?
                .as_int()
                .map(|i| i as u32)
                .ok_or_else(|| err(format!("{k} must be an integer")))
        };
        let skip_install = match m.get("skip_install") {
            None | Some(Json::Null) => None,
            Some(s) => {
                let sm = s
                    .as_obj()
                    .ok_or_else(|| err("skip_install must be an object"))?;
                no_unknown(sm, &["enabled", "layer_digest"], "skip_install")?;
                if sm.get("enabled").and_then(Json::as_bool) != Some(true) {
                    return Err(err("skip_install.enabled must be true when present"));
                }
                Some(SkipInstall {
                    layer_digest: need_str(sm, "layer_digest")?,
                })
            }
        };
        let cfg = JobConfig {
            pin: need_str(m, "pin")?,
            job_name: need_str(m, "job_name")?,
            tasks_dir: need_str(m, "tasks_dir")?,
            tasks: need(m, "tasks")?
                .as_arr()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|t| {
                    t.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| err("tasks[] must be strings"))
                })
                .collect::<Result<_, _>>()?,
            agent: need_str(m, "agent")?,
            env: need_str(m, "env")?,
            n_attempts: int("n_attempts")?,
            n_concurrent: int("n_concurrent")?,
            max_retries: int("max_retries")?,
            no_force_build: m
                .get("no_force_build")
                .and_then(Json::as_bool)
                .ok_or_else(|| err("no_force_build must be a bool"))?,
            skip_install,
            o_route: match m.get("o_route") {
                None | Some(Json::Null) => None,
                Some(oj) => {
                    let om = oj
                        .as_obj()
                        .ok_or_else(|| err("o_route must be an object"))?;
                    no_unknown(
                        om,
                        &[
                            "relay_base_url",
                            "api_key_name",
                            "api_format",
                            "slot_token_ref",
                            "slice_micros",
                        ],
                        "o_route",
                    )?;
                    Some(ORoute {
                        relay_base_url: need_str(om, "relay_base_url")?,
                        api_key_name: need_str(om, "api_key_name")?,
                        api_format: need_str(om, "api_format")?,
                        slot_token_ref: need_str(om, "slot_token_ref")?,
                        slice_micros: need(om, "slice_micros")?
                            .as_int()
                            .ok_or_else(|| err("o_route.slice_micros must be an integer"))?,
                    })
                }
            },
            jobs_dir: need_str(m, "jobs_dir")?,
        };
        cfg.check()?;
        Ok(cfg)
    }
}

/// `image_identical_label(pin)` — REQ-XC-306's runner-check label: at an
/// admitted pin containing `skip_install` the surfaces can be
/// image-identical; otherwise every runner check is labelled
/// `not-image-identical`.
pub fn image_identical_label(pin: &HarborPin) -> &'static str {
    if pin.contains_skip_install {
        "image-identical"
    } else {
        "not-image-identical"
    }
}

// ── REQ-XB-124 — image identity (identical bytes on both surfaces) ─────────────

/// One task image's identity: pull by digest only, tag locally with the
/// `task.toml` tag, run `--no-force-build`, assert the tag still
/// resolves to the pinned digest after each job.
#[derive(Debug, Clone, PartialEq)]
pub struct ImagePin {
    /// The task.
    pub task: String,
    /// The repository (the `task.toml` image name without its tag).
    pub repo: String,
    /// The `task.toml` tag (the local tag the job resolves).
    pub tag: String,
    /// The pinned `sha256:` digest.
    pub digest: String,
    /// The platform (`linux/amd64` family — recorded with the digest).
    pub platform: String,
}

/// The drift fact — a tag that resolved to another digest after the
/// job. The task's trials withhold `withheld{image_drift}` (the closed
/// R6.8 set — consumed, never extended).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageDrift {
    /// The task whose image drifted.
    pub task: String,
    /// The pinned digest.
    pub expected: String,
    /// What the tag resolved to after the job.
    pub observed: String,
}

impl ImageDrift {
    /// The withheld disposition spelling.
    pub fn disposition(&self) -> Disposition {
        Disposition::Withheld {
            reason: WithheldReason::ImageDrift,
        }
    }
}

impl ImagePin {
    /// `new(…)` — a tag-only image reference refuses (digest-only, the
    /// R6.9 `image_not_local` family).
    pub fn new(
        task: &str,
        repo: &str,
        tag: &str,
        digest: &str,
        platform: &str,
    ) -> Result<ImagePin, HarborError> {
        if !is_pinned_image_digest(digest) {
            return Err(err(format!(
                "image digest {digest} is not a pinned sha256 — a tag-only \
                 reference refuses"
            )));
        }
        Ok(ImagePin {
            task: task.to_string(),
            repo: repo.to_string(),
            tag: tag.to_string(),
            digest: digest.to_string(),
            platform: platform.to_string(),
        })
    }

    /// The pre-pull argv pair: `pull <repo>@<digest>` then
    /// `tag <digest> <repo>:<tag>` (identical bytes on both surfaces —
    /// the local tag is the `task.toml` spelling the job resolves).
    pub fn prepull_argv(&self, podman_bin: &str) -> Vec<Vec<String>> {
        vec![
            vec![
                podman_bin.to_string(),
                "pull".to_string(),
                format!("{}@{}", self.repo, self.digest),
            ],
            vec![
                podman_bin.to_string(),
                "tag".to_string(),
                self.digest.clone(),
                format!("{}:{}", self.repo, self.tag),
            ],
        ]
    }

    /// The post-job assertion: `podman image inspect <repo>:<tag>`'s
    /// resolved digest must equal the pin. Drift → [`ImageDrift`].
    pub fn post_job_assert(&self, observed_digest: &str) -> Result<(), ImageDrift> {
        let observed = observed_digest.trim();
        if observed != self.digest {
            return Err(ImageDrift {
                task: self.task.clone(),
                expected: self.digest.clone(),
                observed: observed.to_string(),
            });
        }
        Ok(())
    }
}

/// A per-task tree-digest mismatch against `dataset.toml` — refuses
/// before any slot (`withheld{admission:task_digest_mismatch}`).
#[derive(Debug, Clone, PartialEq)]
pub struct TreeDigestMismatch {
    /// The task.
    pub task: String,
    /// The `dataset.toml` declared digest.
    pub declared: String,
    /// The computed digest.
    pub computed: String,
}

impl TreeDigestMismatch {
    /// The withheld disposition spelling.
    pub fn disposition(&self) -> Disposition {
        Disposition::Withheld {
            reason: WithheldReason::Admission(
                crate::disposition::AdmissionReason::TaskDigestMismatch,
            ),
        }
    }
}

/// `task_tree_digest(task_dir)` — the task bytes' digest: sha256 over
/// the sorted `(relative path, file bytes)` pairs (directories
/// excluded; symlinks read verbatim as link names, never followed).
/// Read-only.
pub fn task_tree_digest(task_dir: &Path) -> Result<String, HarborError> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut stack = vec![task_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let rd = std::fs::read_dir(&dir)
            .map_err(|e| err(format!("task dir {} unreadable: {e}", dir.display())))?;
        for entry in rd.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(task_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let ft = entry
                .file_type()
                .map_err(|e| err(format!("{rel}: file_type: {e}")))?;
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_symlink() {
                let target =
                    std::fs::read_link(&path).map_err(|e| err(format!("{rel}: read_link: {e}")))?;
                entries.push((
                    rel,
                    format!("symlink:{}", target.to_string_lossy()).into_bytes(),
                ));
            } else {
                let bytes = std::fs::read(&path).map_err(|e| err(format!("{rel}: read: {e}")))?;
                entries.push((rel, bytes));
            }
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut acc = Vec::new();
    for (rel, bytes) in entries {
        acc.extend_from_slice(rel.as_bytes());
        acc.push(0);
        acc.extend_from_slice(&bytes);
        acc.push(0);
    }
    Ok(format!("sha256:{}", sha256_hex(&acc)))
}

/// `check_task_digests(declared, computed)` — every task's `dataset.toml`
/// digest must equal its computed tree digest before any slot runs.
pub fn check_task_digests(
    declared: &BTreeMap<String, String>,
    computed: &BTreeMap<String, String>,
) -> Result<(), TreeDigestMismatch> {
    for (task, decl) in declared {
        let got = computed.get(task).cloned().unwrap_or_default();
        if got != *decl {
            return Err(TreeDigestMismatch {
                task: task.clone(),
                declared: decl.clone(),
                computed: if got.is_empty() {
                    "missing".to_string()
                } else {
                    got
                },
            });
        }
    }
    Ok(())
}

// ── REQ-XB-123 — read-only ingest into hh-harbor-ingest/1 + hh-harbor-trial/1 ──

/// One ingested file's identity — its sha256, length and blob-pool
/// address (bytes were copied, the tree never mutated).
#[derive(Debug, Clone, PartialEq)]
pub struct FileDigest {
    /// `sha256:<hex>`.
    pub sha256: String,
    /// Byte length.
    pub bytes: u64,
    /// The blob-pool ref the bytes were deposited at.
    pub blob_ref: String,
}

impl FileDigest {
    /// The canonical member JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("sha256", Json::str(&self.sha256)),
            ("bytes", Json::Int(self.bytes as i64)),
            ("blob_ref", Json::str(&self.blob_ref)),
        ])
    }
}

/// The blob-pool seam — the ledger `Store::put_blob` in production, an
/// in-memory map in fixtures.
pub trait BlobSink {
    /// Deposit bytes; return the content ref (`<alg>:<digest>`).
    fn put(&mut self, bytes: &[u8], mime: &str) -> Result<String, HarborError>;
}

/// `hh-harbor-trial/1` — one trial directory's ingest view: the same
/// `task`/`verdict`/`disposition` vocabulary as `hh-experiment-slot/1`
/// with O-surface identity (`harbor_pin`, `trial`, `attempt_index`).
#[derive(Debug, Clone, PartialEq)]
pub struct HarborTrial {
    /// The program-wide pin the job ran under.
    pub harbor_pin: String,
    /// The trial directory name (`<task[:32]>__<id7>`).
    pub trial: String,
    /// The task (dir-name parse).
    pub task: String,
    /// The declared attempt index when `result.json` carries one.
    pub attempt_index: Option<u32>,
    /// Every file under the trial dir (rel path → digest).
    pub files: BTreeMap<String, FileDigest>,
    /// The reward-file digests (`verifier/reward.{json,txt}`).
    pub reward_files: BTreeMap<String, FileDigest>,
    /// `verifier/ctrf.json`.
    pub ctrf: Option<FileDigest>,
    /// `verifier/test-stdout.txt`.
    pub test_stdout: Option<FileDigest>,
    /// `exception.txt`.
    pub exception: Option<FileDigest>,
    /// `agent/trajectory.json` + its declared ATIF version.
    pub trajectory: Option<(FileDigest, Option<String>)>,
    /// The declared Harbor cause (from `result.json`), verbatim.
    pub cause: Option<String>,
    /// Whether `cause` sits inside the pinned `error_policy` set.
    pub cause_mapped: bool,
    /// The decided reward ppm when the reward files parse.
    pub reward_ppm: Option<u64>,
    /// The mapped disposition spelling.
    pub disposition: String,
}

impl HarborTrial {
    /// The canonical record.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str(TRIAL_SCHEMA));
        m.insert("harbor_pin".into(), Json::str(&self.harbor_pin));
        m.insert("trial".into(), Json::str(&self.trial));
        m.insert("task".into(), Json::str(&self.task));
        if let Some(a) = self.attempt_index {
            m.insert("attempt_index".into(), Json::Int(a as i64));
        }
        let files = |f: &BTreeMap<String, FileDigest>| {
            Json::Obj(
                f.iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect::<BTreeMap<_, _>>(),
            )
        };
        m.insert("files".into(), files(&self.files));
        m.insert("reward_files".into(), files(&self.reward_files));
        if let Some(c) = &self.ctrf {
            m.insert("ctrf".into(), c.to_json());
        }
        if let Some(t) = &self.test_stdout {
            m.insert("test_stdout".into(), t.to_json());
        }
        if let Some(e) = &self.exception {
            m.insert("exception".into(), e.to_json());
        }
        if let Some((d, ver)) = &self.trajectory {
            let mut t = vec![("digest", d.to_json())];
            if let Some(v) = ver {
                t.push(("atif_version", Json::str(v)));
            }
            m.insert("trajectory".into(), Json::obj(t));
        }
        if let Some(c) = &self.cause {
            m.insert("cause".into(), Json::str(c));
            m.insert("cause_mapped".into(), Json::Bool(self.cause_mapped));
        }
        if let Some(p) = self.reward_ppm {
            m.insert("reward_ppm".into(), Json::Int(p as i64));
        }
        m.insert("disposition".into(), Json::str(&self.disposition));
        Json::Obj(m)
    }
}

/// `hh-harbor-ingest/1` — the job record: the runner-written committed
/// job config, the named job-file digests, every file's sha256 + blob
/// ref, and one [`HarborTrial`] per trial directory. The job tree is
/// never mutated (read-only walk — the battery pins byte-identity).
#[derive(Debug, Clone, PartialEq)]
pub struct HarborIngest {
    /// The program-wide pin.
    pub harbor_pin: String,
    /// The job name.
    pub job_name: String,
    /// The runner-written job config (committed into the record).
    pub job_config: JobConfig,
    /// The job config's blob ref.
    pub job_config_ref: String,
    /// The named job-level files (`config.json`/`lock.json`/
    /// `result.json`/`job.log`) — those present.
    pub job_files: BTreeMap<String, FileDigest>,
    /// Every file under the job dir (rel path → digest).
    pub files: BTreeMap<String, FileDigest>,
    /// One per trial directory.
    pub trials: Vec<HarborTrial>,
}

impl HarborIngest {
    /// The canonical record.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str(INGEST_SCHEMA));
        m.insert("harbor_pin".into(), Json::str(&self.harbor_pin));
        m.insert("job_name".into(), Json::str(&self.job_name));
        m.insert("job_config".into(), self.job_config.to_json());
        m.insert("job_config_ref".into(), Json::str(&self.job_config_ref));
        let files = |f: &BTreeMap<String, FileDigest>| {
            Json::Obj(
                f.iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect::<BTreeMap<_, _>>(),
            )
        };
        m.insert("job_files".into(), files(&self.job_files));
        m.insert("files".into(), files(&self.files));
        m.insert(
            "trials".into(),
            Json::Arr(self.trials.iter().map(|t| t.to_json()).collect()),
        );
        Json::Obj(m)
    }
}

/// The trial dir's declared cause — `result.json`'s `cause` /
/// `error.cause` / `exception.cause` member, verbatim (never inferred
/// from the traceback text).
fn declared_cause(result: Option<&Json>) -> Option<String> {
    let r = result?;
    for path in [
        &["cause"][..],
        &["error", "cause"][..],
        &["exception", "cause"][..],
        &["error_policy", "cause"][..],
    ] {
        let mut cur = r;
        let mut found = true;
        for k in path {
            match cur.get(k) {
                Some(v) => cur = v,
                None => {
                    found = false;
                    break;
                }
            }
        }
        if found {
            if let Some(s) = cur.as_str() {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// The ATIF version member spellings — `atif_version` | `version` |
/// `schema`, verbatim, first found.
fn atif_version(trajectory: Option<&Json>) -> Option<String> {
    let t = trajectory?;
    for k in ["atif_version", "version", "schema"] {
        if let Some(s) = t.get(k).and_then(Json::as_str) {
            return Some(s.to_string());
        }
    }
    None
}

/// `ingest_job(jobs_root, job_name, config, sink)` — the read-only
/// walk: hash every file, copy bytes into the blob pool, emit
/// `hh-harbor-ingest/1` with one `hh-harbor-trial/1` per trial dir.
/// The runner-written `config` is committed into the record (its own
/// blob). Infra causes map to `inconclusive{infra:<cause>}` — the
/// failed trial is preserved verbatim; a re-run is a new HH slot,
/// never a retry inside this job (F13, `-r 0`).
pub fn ingest_job(
    jobs_root: &Path,
    job_name: &str,
    config: &JobConfig,
    pin: &HarborPin,
    sink: &mut dyn BlobSink,
) -> Result<HarborIngest, HarborError> {
    let job_dir = jobs_root.join(job_name);
    // The committed runner-written config lands first.
    let config_bytes = config.to_json().to_canonical_string().into_bytes();
    let job_config_ref = sink.put(&config_bytes, "application/json")?;

    // Walk the tree — read-only; sorted relative paths.
    let mut rels: Vec<PathBuf> = Vec::new();
    let mut stack = vec![job_dir.clone()];
    while let Some(dir) = stack.pop() {
        let rd = std::fs::read_dir(&dir)
            .map_err(|e| err(format!("job dir {} unreadable: {e}", dir.display())))?;
        for entry in rd.flatten() {
            let path = entry.path();
            let ft = entry
                .file_type()
                .map_err(|e| err(format!("{}: file_type: {e}", path.display())))?;
            if ft.is_dir() {
                stack.push(path);
            } else {
                rels.push(path);
            }
        }
    }
    rels.sort();
    let mut files = BTreeMap::new();
    let mut blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for path in &rels {
        let bytes =
            std::fs::read(path).map_err(|e| err(format!("{}: read: {e}", path.display())))?;
        let rel = path
            .strip_prefix(&job_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        let blob_ref = sink.put(&bytes, "application/octet-stream")?;
        files.insert(
            rel.clone(),
            FileDigest {
                sha256: format!("sha256:{}", sha256_hex(&bytes)),
                bytes: bytes.len() as u64,
                blob_ref,
            },
        );
        blobs.insert(rel, bytes);
    }
    let json_at = |rel: &str| -> Option<Json> {
        blobs
            .get(rel)
            .and_then(|b| hharness_wire::json::parse(&String::from_utf8_lossy(b)).ok())
    };

    let mut job_files = BTreeMap::new();
    for name in ["config.json", "lock.json", "result.json", "job.log"] {
        if let Some(d) = files.get(name) {
            job_files.insert(name.to_string(), d.clone());
        }
    }

    // One hh-harbor-trial/1 per immediate trial directory.
    let mut trial_dirs: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&job_dir) {
        for entry in rd.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if let Some(name) = entry.file_name().to_str() {
                    trial_dirs.push(name.to_string());
                }
            }
        }
    }
    trial_dirs.sort();
    let mut trials = Vec::new();
    for tdir in trial_dirs {
        let prefix = format!("{tdir}/");
        let tfiles: BTreeMap<String, FileDigest> = files
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(k, v)| (k[prefix.len()..].to_string(), v.clone()))
            .collect();
        let bytes_at = |rel: &str| blobs.get(&format!("{prefix}{rel}"));
        let ctrf = tfiles.get("verifier/ctrf.json").cloned();
        let test_stdout = tfiles.get("verifier/test-stdout.txt").cloned();
        let exception = tfiles.get("exception.txt").cloned();
        let trajectory_d = tfiles.get("agent/trajectory.json").cloned();

        let mut reward_files = BTreeMap::new();
        for (short, rel) in [
            ("reward.json", "verifier/reward.json"),
            ("reward.txt", "verifier/reward.txt"),
        ] {
            if let Some(d) = tfiles.get(rel) {
                reward_files.insert(short.to_string(), d.clone());
            }
        }
        let fset = RewardFileSet {
            reward_json: bytes_at("verifier/reward.json").cloned(),
            reward_txt: bytes_at("verifier/reward.txt").cloned(),
            ctrf_json: bytes_at("verifier/ctrf.json").cloned(),
            test_stdout: bytes_at("verifier/test-stdout.txt").cloned(),
        };
        let reward_ppm = match parse_reward_files(&fset) {
            ChannelOutcome::Decided(p) => Some(p),
            ChannelOutcome::Unavailable(_) => None,
        };
        let result = json_at(&format!("{prefix}result.json"));
        let cause = declared_cause(result.as_ref());
        let attempt_index = result
            .as_ref()
            .and_then(|r| r.get("attempt_index").or_else(|| r.get("attempt")))
            .and_then(Json::as_int)
            .map(|i| i as u32);
        let cause_mapped = cause.as_deref().and_then(HarborErrorCause::parse).is_some();
        // The mapped disposition (the pinned cause map; the §3.4 reward
        // codec for the decided/unavailable legs; never fabricated).
        let disposition = if let Some(c) = &cause {
            match HarborErrorCause::parse(c) {
                Some(hc) => harbor_cause_map(hc, reward_ppm.map(|p| p > 0).unwrap_or(false))
                    .disposition
                    .as_str(),
                None => format!("unmapped_cause{{{c}}}"),
            }
        } else {
            match parse_reward_files(&fset) {
                ChannelOutcome::Decided(p) => {
                    let exit = result
                        .as_ref()
                        .and_then(|r| r.get("agent_exit_code"))
                        .and_then(Json::as_int)
                        .unwrap_or(0);
                    Disposition::Executed {
                        resolved: p > 0,
                        end_cause: EndCause::Exited(exit),
                    }
                    .as_str()
                }
                ChannelOutcome::Unavailable(c) => Disposition::Inconclusive {
                    cause: InconclusiveCause::VerifierUnavailable(c),
                }
                .as_str(),
            }
        };
        let task = tdir
            .rsplit_once("__")
            .map(|(t, _)| t)
            .unwrap_or(&tdir)
            .to_string();
        let trajectory = trajectory_d.map(|d| {
            (
                d,
                atif_version(json_at(&format!("{prefix}agent/trajectory.json")).as_ref()),
            )
        });
        trials.push(HarborTrial {
            harbor_pin: pin.pin.clone(),
            trial: tdir,
            task,
            attempt_index,
            files: tfiles,
            reward_files,
            ctrf,
            test_stdout,
            exception,
            trajectory,
            cause,
            cause_mapped,
            reward_ppm,
            disposition,
        });
    }
    Ok(HarborIngest {
        harbor_pin: pin.pin.clone(),
        job_name: job_name.to_string(),
        job_config: config.clone(),
        job_config_ref,
        job_files,
        files,
        trials,
    })
}
