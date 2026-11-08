//! `hh-harness-render/1` — the first-pass harness renderers (R6.24;
//! §5.3 REQ-XC-307/308/309/310/320, §5.4 REQ-XC-327, §6.10 REQ-XD-082's
//! client half, §7.1 REQ-XE-001 absorbing REQ-XD-122, §7.5 REQ-XD-121
//! absorbing REQ-XE-003; design/34 §1.4/§1.5 + F6–F8/F13/F17).
//!
//! One pure renderer per first-pass hosted harness (Q-X48):
//! `admission × slot → {argv, env, files}` mirroring the Harbor adapter
//! at [`crate::harbor::HARBOR_PIN`]. Each renderer reproduces the
//! adapter's exec shape verbatim — `["bash", "-c", "set -o pipefail; …"]`
//! (installed/base `_exec` prefix + `exec_shell_args`) — with the slot
//! token as the only credential and the native effort mechanism set to
//! the protocol pin (Q-X22 `high`) beside the proxy pin.
//!
//! The projected environment is a *closed set* (REQ-XC-308): the
//! harness's base-URL and key variables, the egress routing hints, the
//! mandatory `no_proxy`, the declared `NAME=value` disable switches, and
//! the image's `HOME`/`USER` — nothing else. Rendered *files* are
//! slot-invariant (the instruction and the token never land in them) so
//! the ST2 composite equals `config.template_hash` on every slot.
//!
//! Mirroring evidence (adapter source at `ff2a1ef4`, read offline):
//! `mini_swe_agent.py` `run()` :849–1017 (research/28 A.1 records the
//! same), `pi.py` `run()` :437–525 + `_build_custom_models_json`
//! :260–312, `claude_code.py` `_resolve_auth_env` :1995–2074 +
//! `_model_alias_env` :1965–1981 + `run()` :2100–2215, `options.py`
//! `compile_cli_from_options` :135–153. HH's O surface passes the key by
//! name (`--api-key-name`), so the adapters' env-var key channels fire:
//! `MINI_SWE_AGENT_MODEL_API_KEY_JSON` expands into the `-c` kwarg,
//! Pi's `models.json` carries the `$OPENAI_API_KEY` reference.

use std::collections::BTreeMap;

use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

use crate::admission::{
    self, AdmissionError, ValidationContext, CLAUDE_CODE, MINI_SWE_AGENT, MINI_SWE_AGENT_VERSION,
};
use crate::json_ext::JsonExt;

/// The record kind spelling.
pub const RENDER_SCHEMA: &str = "hh-harness-render/1";
/// The renderer-version every first-pass record carries.
pub const RENDERER_VERSION: &str = "1";
/// mini-swe-agent's renderer id (`config.renderer_id`).
pub const RID_MINI: &str = "adpt.mini_swe_agent";
/// Pi's renderer id.
pub const RID_PI: &str = "adpt.pi";
/// Claude Code's renderer id.
pub const RID_CLAUDE: &str = "adpt.claude_code";
/// Pi's record name (`harness.name`).
pub const PI: &str = "pi";
/// The elision tombstone for the slot token in recorded forms (X-05).
pub const ELIDED: &str = "<secret:slot-token>";
/// The mandatory loopback `no_proxy` value (REQ-XC-308; F8).
pub const NO_PROXY_VALUE: &str = "127.0.0.1,localhost";
/// builtin `mini.yaml` sha256 (REQ-XE-001; research/28 A.3).
pub const MINI_YAML_SHA256: &str =
    "sha256:b539a8965f5bf41dd0f32daafa5d2db22581923d13812fa378fd09569b9d6b0c";
/// The proxy route Claude Code is admitted for (`/v1/messages`).
pub const MESSAGES_ROUTE: &str = "\u{2f}v1\u{2f}messages";
/// The declared refused route Claude Code falls back from (§5.4; the
/// proxy answers `refused_route`, Claude Code estimates — never served).
pub const COUNT_TOKENS_ROUTE: &str = "\u{2f}v1\u{2f}messages\u{2f}count_tokens";

const LOGS_AGENT: &str = "\u{2f}logs/agent";
const INSTALLED_AGENT: &str = "\u{2f}installed-agent";
const TESTS_DIR: &str = "\u{2f}tests";
const SOLUTION_DIR: &str = "\u{2f}solution";
const MINI_TRAJECTORY: &str = "\u{2f}logs\u{2f}agent\u{2f}mini-swe-agent.trajectory.json";
const MINI_TXT: &str = "\u{2f}logs\u{2f}agent\u{2f}mini-swe-agent.txt";
const PI_SESSIONS: &str = "\u{2f}logs\u{2f}agent\u{2f}pi\u{2f}sessions";
const PI_TXT: &str = "\u{2f}logs\u{2f}agent\u{2f}pi.txt";
const PI_CONFIG_DIR: &str = "\u{2f}tmp\u{2f}harbor-pi-agent";
const CLAUDE_CONFIG_DIR: &str = "\u{2f}logs\u{2f}agent\u{2f}sessions";
const CLAUDE_TXT: &str = "\u{2f}logs\u{2f}agent\u{2f}claude-code.txt";

/// A typed render refusal — every leg names its rule (honest refusal;
/// a missing capability is never fabricated).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderError {
    /// The refusal rule.
    pub rule: &'static str,
    /// The member path / verbatim detail.
    pub detail: String,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{RENDER_SCHEMA}: {}: {}", self.rule, self.detail)
    }
}
impl std::error::Error for RenderError {}

impl From<AdmissionError> for RenderError {
    fn from(e: AdmissionError) -> RenderError {
        RenderError {
            rule: "admission",
            detail: e.to_string(),
        }
    }
}

fn err(rule: &'static str, detail: impl Into<String>) -> RenderError {
    RenderError {
        rule,
        detail: detail.into(),
    }
}

fn sfield(m: &BTreeMap<String, Json>, k: &str, at: &str) -> Result<String, RenderError> {
    m.get(k)
        .and_then(Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| err("member_missing", format!("{at}.{k} must be a string")))
}

/// An instruction hazard the deterministic pre-check flags (REQ-XC-320)
/// before selection freezes — flagged hazards refuse at render too.
#[derive(Debug, Clone, PartialEq)]
pub struct InstructionHazard {
    /// The hazard code (`leading_dash` | `nul_byte` | `non_utf8`).
    pub code: &'static str,
    /// The verbatim detail.
    pub detail: String,
}

/// The use a render is for — Claude Code is a black-box arm only
/// (Q-X26 term 3): an HIR-target use refuses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderUse {
    /// A hosted-arm slot (the only use admitted for Claude Code).
    HostedArm,
    /// An HIR/parity target — refused for Claude Code (REQ-XC-327).
    HirTarget,
}

/// The slot-side inputs a render consumes (admission × slot).
#[derive(Debug, Clone)]
pub struct SlotInputs {
    /// The slot token — the only credential rendered (X-05).
    pub slot_token: String,
    /// The cell's model relay port (`127.0.0.1:P_model`; the declared
    /// cell-spec constant of design/34 §1.4).
    pub model_port: u16,
    /// The verbatim instruction bytes (`instruction.md` content).
    pub instruction: Vec<u8>,
    /// The image-declared `HOME`.
    pub home: String,
    /// The image-declared user.
    pub user: String,
    /// The declared agent wall timeout, seconds (Q-X29 external cap —
    /// enforced outside the harness; recorded, never a `-c` flag).
    pub agent_wall_timeout_s: u64,
    /// The declared proxy budget ceiling, USD micros (the slot slice;
    /// a binding ceiling is `infra_uncertain`, REQ-XD-122 alias).
    pub proxy_budget_ceiling_usd_micros: u64,
    /// The slot id (derives Claude Code's per-run instruction var name,
    /// mirroring the adapter's uuid suffix deterministically).
    pub slot_id: String,
}

/// One rendered file (path + bytes — slot-invariant by construction).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedFile {
    /// The cell path (a member of `config.rendered_paths`).
    pub path: String,
    /// The file bytes.
    pub bytes: Vec<u8>,
}

/// The renderer output `{argv, env, files}` (REQ-XC-307) plus the
/// adapter's pre-run exec where one exists.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedConfig {
    /// The agent exec argv (`["bash","-c","set -o pipefail; …"]` —
    /// the adapter's `_exec` prefix + `exec_shell_args` verbatim).
    pub argv: Vec<String>,
    /// The adapter's setup exec (Claude Code's config-dir prep), run
    /// before the agent exec; `None` where the adapter has none.
    pub setup_argv: Option<Vec<String>>,
    /// The closed cell environment (REQ-XC-308).
    pub env: BTreeMap<String, String>,
    /// The rendered files (slot-invariant; ST2's composite hash input).
    pub files: Vec<RenderedFile>,
    /// The verbatim instruction bytes (the record's `instruction_sha256`
    /// input — never re-read from a file, REQ-XC-320).
    pub instruction: Vec<u8>,
}

fn bash_exec(command: String) -> Vec<String> {
    vec![
        "bash".to_string(),
        "-c".to_string(),
        format!("set -o pipefail; {command}"),
    ]
}

/// `py_shlex_quote(s)` — Python `shlex.quote` at the pin (`[^\w@%+=:,./-]`
/// re.ASCII is unsafe; `'` → `'"'"'`; empty → `''`). The instruction
/// travels inside the adapter's shell string byte for byte.
pub fn py_shlex_quote(s: &str) -> String {
    fn safe(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_' || b"@%+=:,./-".contains(&b)
    }
    if s.is_empty() {
        return "''".to_string();
    }
    if s.bytes().all(safe) {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// `py_json_str(s)` — Python `json.dumps(s)` for a string
/// (`ensure_ascii=True`: `"`/`\\`/controls escaped, non-ASCII → `\uXXXX`
/// with surrogate pairs). Backs the adapter's
/// `MINI_SWE_AGENT_MODEL_API_KEY_JSON = json.dumps(api_key)` member.
pub fn py_json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if c.is_ascii() => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{:04x}", u));
                }
            }
        }
    }
    out.push('"');
    out
}

/// `precheck_instruction(name, bytes)` — the REQ-XC-320 deterministic
/// hazard pre-check over instruction bytes, called before selection
/// freezes. Pi exits on a leading `-` (positional argv without `--`;
/// F13). A NUL byte cannot travel in argv, an env var or the shell
/// string on any surface. The adapter's instruction is a Python `str`,
/// so non-UTF-8 bytes can never reach it faithfully.
pub fn precheck_instruction(name: &str, bytes: &[u8]) -> Vec<InstructionHazard> {
    let mut out = Vec::new();
    if bytes.contains(&0) {
        out.push(InstructionHazard {
            code: "nul_byte",
            detail: "instruction contains NUL — cannot travel argv/env/shell".into(),
        });
    }
    if std::str::from_utf8(bytes).is_err() {
        out.push(InstructionHazard {
            code: "non_utf8",
            detail: "instruction is not UTF-8 — the adapter passes a str".into(),
        });
    }
    if name == PI && bytes.first() == Some(&b'-') {
        out.push(InstructionHazard {
            code: "leading_dash",
            detail: "pi passes the instruction positionally without `--` (F13)".into(),
        });
    }
    out
}

fn cfg(rec: &Json) -> Result<&BTreeMap<String, Json>, RenderError> {
    rec.get("config")
        .and_then(Json::as_obj)
        .ok_or_else(|| err("member_missing", "config must be an object"))
}

fn base_url(slot: &SlotInputs, suffix: &str) -> String {
    format!("http://127.0.0.1:{}{}", slot.model_port, suffix)
}

/// `closed_env(harness, slot)` — the REQ-XC-308 closed set: the
/// harness's own members, then the egress routing hints, the mandatory
/// `no_proxy`, and the image's `HOME`/`USER`. Guarded by the shared
/// `validate_cell_env` (CC1 — R6.15's guard, not a second check).
fn closed_env(
    mut env: BTreeMap<String, String>,
    slot: &SlotInputs,
) -> Result<BTreeMap<String, String>, RenderError> {
    let proxy = format!(
        "http://127.0.0.1:{}",
        hharness_env::egress_allowlist::EGRESS_RELAY_PORT
    );
    env.insert("http_proxy".into(), proxy.clone());
    env.insert("https_proxy".into(), proxy.clone());
    env.insert("HTTP_PROXY".into(), proxy.clone());
    env.insert("HTTPS_PROXY".into(), proxy);
    env.insert("no_proxy".into(), NO_PROXY_VALUE.to_string());
    env.insert("NO_PROXY".into(), NO_PROXY_VALUE.to_string());
    env.insert("HOME".into(), slot.home.clone());
    env.insert("USER".into(), slot.user.clone());
    hharness_env::egress_allowlist::validate_cell_env(&env)
        .map_err(|e| err("closed_env", e.to_string()))?;
    Ok(env)
}

/// `apply_disable_switches(rec, env, argv)` — the declared
/// `config.disable_switches` land on the closed env (`NAME=value`,
/// value possibly empty) or are asserted present in argv (`--…`). A
/// `NAME=` member colliding with a different rendered value refuses —
/// a declared Q-X26 empty can never be silently overridden (X-05).
/// `compat.*` members are declared-not-rendered (Pi's `supportsDeveloperRole`
/// is an HF repair under smoke evaluation — design/34 §1.5 note).
fn apply_disable_switches(
    rec: &Json,
    env: &mut BTreeMap<String, String>,
    argv: &[String],
    unrendered: &mut Vec<String>,
) -> Result<(), RenderError> {
    let c = cfg(rec)?;
    let switches = c
        .get("disable_switches")
        .and_then(Json::as_arr)
        .ok_or_else(|| err("member_missing", "config.disable_switches required"))?;
    for s in switches {
        let sw = s
            .as_obj()
            .and_then(|m| m.get("switch"))
            .and_then(Json::as_str)
            .ok_or_else(|| err("member_missing", "disable_switches[].switch required"))?;
        if let Some(sw) = sw.strip_prefix("compat.") {
            unrendered.push(format!("compat.{sw}"));
            continue;
        }
        if sw.starts_with("--") {
            if !argv.iter().any(|a| a.contains(sw)) {
                return Err(err(
                    "switch_unrendered",
                    format!("declared argv switch {sw} absent from the render"),
                ));
            }
            continue;
        }
        let (name, value) = sw
            .split_once('=')
            .map(|(n, v)| (n, v.to_string()))
            .unwrap_or((sw, String::new()));
        if let Some(cur) = env.get(name) {
            if cur != &value {
                return Err(err(
                    "switch_conflict",
                    format!("disable switch {name} collides with a rendered value"),
                ));
            }
        }
        env.insert(name.to_string(), value);
    }
    Ok(())
}

fn effort_of(rec: &Json, mechanism: &str) -> Result<String, RenderError> {
    let c = cfg(rec)?;
    let eff = c
        .get("effort")
        .and_then(Json::as_obj)
        .ok_or_else(|| err("member_missing", "config.effort required"))?;
    let mech = sfield(eff, "mechanism", "config.effort")?;
    if mech != mechanism {
        return Err(err(
            "effort_mechanism",
            format!("config.effort.mechanism {mech} — expected {mechanism}"),
        ));
    }
    sfield(eff, "value", "config.effort")
}

fn rendered_paths(rec: &Json) -> Result<Vec<String>, RenderError> {
    cfg(rec)?
        .get("rendered_paths")
        .and_then(Json::as_arr)
        .map(|a| {
            a.iter()
                .map(|x| x.as_str().unwrap_or("").to_string())
                .collect()
        })
        .ok_or_else(|| err("member_missing", "config.rendered_paths required"))
}

fn expect_files(got: Vec<RenderedFile>, want: &[String]) -> Result<Vec<RenderedFile>, RenderError> {
    let have: Vec<&str> = got.iter().map(|f| f.path.as_str()).collect();
    if have != want {
        return Err(err(
            "rendered_paths",
            format!("rendered files {have:?} do not equal config.rendered_paths {want:?}"),
        ));
    }
    Ok(got)
}

fn canonical_bytes(j: &Json) -> Vec<u8> {
    j.to_canonical_string().into_bytes()
}

// ── mini-swe-agent 2.4.6 (adpt.mini_swe_agent/1) ─────────────────────────────
//
// `mini_swe_agent.py` `run()` :849–1017 at the pin: env =
// `{**access.env, MSWEA_CONFIGURED, MSWEA_COST_TRACKING}` with
// `MINI_SWE_AGENT_MODEL_API_KEY_JSON=json.dumps(api_key)` under
// `--api-key-name` (:984–987); flags `--yolo --model --task --output
// --cost-limit 0` + `-c mini` + timeout/effort/api_key/custom_llm_provider/
// model_class/api_base `-c` specs + `--exit-immediately` (:886–1010),
// `2>&1 </dev/null | tee` (:1011–1015). HH adds
// `LITELLM_LOCAL_MODEL_COST_MAP=True` on both surfaces (REQ-XC-310).
fn render_mini(rec: &Json, slot: &SlotInputs) -> Result<RenderedConfig, RenderError> {
    let c = cfg(rec)?;
    if sfield(c, "renderer_id", "config")? != RID_MINI {
        return Err(err(
            "renderer_id",
            "config.renderer_id is not adpt.mini_swe_agent",
        ));
    }
    let effort = effort_of(rec, "model_kwargs.reasoning_effort")?;
    let model = sfield(c, "model_selector", "config")?;
    let url = base_url(slot, &sfield(c, "base_url_suffix", "config")?);
    let instruction = std::str::from_utf8(&slot.instruction)
        .map_err(|_| err("instruction_encoding", "instruction is not UTF-8"))?;
    // env — `{**access.env}` + the adapter's two members + the
    // `--api-key-name` JSON-kwarg channel + the declared switches.
    let mut env = BTreeMap::new();
    env.insert("OPENAI_API_KEY".into(), slot.slot_token.clone());
    env.insert("OPENAI_BASE_URL".into(), url.clone());
    env.insert(
        "MINI_SWE_AGENT_MODEL_API_KEY_JSON".into(),
        py_json_str(&slot.slot_token),
    );
    let command = format!(
        "if [ -f \"$HOME/.local/bin/env\" ]; then . \"$HOME/.local/bin/env\"; else export PATH=\"$HOME/.local/bin:$PATH\"; fi; \
         mini-swe-agent --yolo --model={} --task={} --output={} --cost-limit 0 \
         -c mini -c model.model_kwargs.timeout=3600 -c model.model_kwargs.reasoning_effort={} \
         -c \"model.model_kwargs.api_key=$MINI_SWE_AGENT_MODEL_API_KEY_JSON\" \
         -c model.model_kwargs.custom_llm_provider=openai -c model.model_class=litellm \
         -c model.model_kwargs.api_base={} \
         --exit-immediately 2>&1 </dev/null | tee {}",
        py_shlex_quote(&model),
        py_shlex_quote(instruction),
        MINI_TRAJECTORY,
        effort,
        url,
        MINI_TXT
    );
    let argv = bash_exec(command);
    let mut unrendered = Vec::new();
    apply_disable_switches(rec, &mut env, &argv, &mut unrendered)?;
    let env = closed_env(env, slot)?;
    // The slot-invariant render manifest at the declared path (the
    // harness reads argv/env, never this file).
    let manifest = canonical_bytes(&Json::obj([
        ("api_base", Json::str(&url)),
        (
            "effort",
            Json::obj([
                ("mechanism", Json::str("model_kwargs.reasoning_effort")),
                ("value", Json::str(&effort)),
            ]),
        ),
        ("instruction_passing", Json::str("argv")),
        ("model", Json::str(&model)),
        ("renderer_id", Json::str(RID_MINI)),
        ("renderer_version", Json::str(RENDERER_VERSION)),
        (
            "unrendered_switches",
            Json::Arr(unrendered.iter().map(Json::str).collect()),
        ),
    ]));
    let paths = rendered_paths(rec)?;
    let files = expect_files(
        vec![RenderedFile {
            path: paths[0].clone(),
            bytes: manifest,
        }],
        &paths,
    )?;
    Ok(RenderedConfig {
        argv,
        setup_argv: None,
        env,
        files,
        instruction: slot.instruction.clone(),
    })
}

// ── Pi (adpt.pi/1) ──────────────────────────────────────────────────────────
//
// `pi.py` `run()` :437–525 at the pin: `env=dict(access.env)` (the
// `OPENAI_API_KEY` destination only — `base_url_destination_names` is
// `{"default": None}`, so no base-url env projects); `models.json`
// written to `/tmp/harbor-pi-agent/models.json` via `json.dumps(…,
// indent=2)+"\n"` (:312–327, `_build_custom_models_json` :260–310 —
// `apiKey: "$<key-env-name>"`, `api: "openai-completions"`,
// `model["reasoning"]=True` for effort ∉ {None,"off"}, `thinkingLevelMap`
// only for xhigh|max); exec `. ~/.nvm/nvm.sh; mkdir -p <sessions> &&
// PI_CODING_AGENT_DIR=… pi --print --mode json --session-dir …
// --provider harbor-endpoint --model <id> <cli_flags> <quoted>` — the
// instruction positional, no `--` (:505–518).
fn render_pi(rec: &Json, slot: &SlotInputs) -> Result<RenderedConfig, RenderError> {
    let c = cfg(rec)?;
    if sfield(c, "renderer_id", "config")? != RID_PI {
        return Err(err("renderer_id", "config.renderer_id is not adpt.pi"));
    }
    let effort = effort_of(rec, "--thinking")?;
    let model = sfield(c, "model_selector", "config")?;
    let url = base_url(slot, &sfield(c, "base_url_suffix", "config")?);
    let instruction = std::str::from_utf8(&slot.instruction)
        .map_err(|_| err("instruction_encoding", "instruction is not UTF-8"))?;
    let mut env = BTreeMap::new();
    env.insert("OPENAI_API_KEY".into(), slot.slot_token.clone());
    let command = format!(
        ". ~/.nvm/nvm.sh; mkdir -p {} && {}={} pi --print --mode json \
         --session-dir {} --provider harbor-endpoint --model {} --thinking {} \
         {} 2>&1 </dev/null | grep -v '\"type\":\"message_update\"' \
         | stdbuf -oL tee {}",
        PI_SESSIONS,
        "PI_CODING_AGENT_DIR",
        PI_CONFIG_DIR,
        PI_SESSIONS,
        py_shlex_quote(&model),
        py_shlex_quote(&effort),
        py_shlex_quote(instruction),
        PI_TXT
    );
    let argv = bash_exec(command);
    let mut unrendered = Vec::new();
    apply_disable_switches(rec, &mut env, &argv, &mut unrendered)?;
    let env = closed_env(env, slot)?;
    // `json.dumps(models_json, indent=2) + "\n"` — Python's pretty form,
    // insertion-ordered keys, byte for byte (`_write_custom_models_json`).
    let models_json = format!(
        "{{\n  \"providers\": {{\n    \"harbor-endpoint\": {{\n      \"baseUrl\": {},\n      \"apiKey\": \"$OPENAI_API_KEY\",\n      \"api\": \"openai-completions\",\n      \"models\": [\n        {{\n          \"id\": {},\n          \"reasoning\": true\n        }}\n      ]\n    }}\n  }}\n}}\n",
        py_json_str(&url),
        py_json_str(&model)
    );
    let paths = rendered_paths(rec)?;
    let files = expect_files(
        vec![RenderedFile {
            path: paths[0].clone(),
            bytes: models_json.into_bytes(),
        }],
        &paths,
    )?;
    Ok(RenderedConfig {
        argv,
        setup_argv: None,
        env,
        files,
        instruction: slot.instruction.clone(),
    })
}

// ── Claude Code (adpt.claude_code/1; Q-X26 five terms; REQ-XC-327) ───────────
//
// `claude_code.py` at the pin: `_resolve_auth_env` :1995–2023 (external
// provider → `ANTHROPIC_AUTH_TOKEN=<key>`, `ANTHROPIC_API_KEY=""`,
// `CLAUDE_CODE_OAUTH_TOKEN=""`, `ANTHROPIC_BASE_URL=<candidate>`);
// `run()` :2100–2215 — `ANTHROPIC_MODEL=<resolved id>` +
// `_model_alias_env(all_aliases=True)` :1965–1981 (five alias vars),
// `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, `IS_SANDBOX=1`,
// `CLAUDE_CONFIG_DIR=<logs>/sessions`, the config-dir setup exec, then
// `printf "%s" "$VAR" | claude --verbose --output-format=stream-json
// <flags> --print 2>&1 | tee <txt>` with the instruction carried in an
// env var named `claude_code_instruction_<uuid-hex>` (:2184–2210).
// `_disallow_web_tools` (:91–94) renders `--disallowedTools
// WebSearch,WebFetch`; `permission_mode` defaults to
// `--permission-mode=bypassPermissions` (:95–103); the effort pin is the
// `--effort <pin>` flag (:66–69).
fn render_claude(rec: &Json, slot: &SlotInputs) -> Result<RenderedConfig, RenderError> {
    let c = cfg(rec)?;
    if sfield(c, "renderer_id", "config")? != RID_CLAUDE {
        return Err(err(
            "renderer_id",
            "config.renderer_id is not adpt.claude_code",
        ));
    }
    let effort = effort_of(rec, "--effort")?;
    let model = sfield(c, "model_selector", "config")?;
    let url = base_url(slot, &sfield(c, "base_url_suffix", "config")?);
    let instruction = std::str::from_utf8(&slot.instruction)
        .map_err(|_| err("instruction_encoding", "instruction is not UTF-8"))?;
    // The adapter's uuid suffix is per-run entropy; the deterministic
    // render derives the same <name>_<32hex> shape from the slot id.
    let suffix = sha256_hex(slot.slot_id.as_bytes())[..32].to_string();
    let shell_var = format!("claude_code_instruction_{suffix}");
    let env_var = shell_var.to_uppercase();
    // `_resolve_auth_env` external-provider branch (:2005–2023) — Q-X26
    // term 2: the slot token is `ANTHROPIC_AUTH_TOKEN` only.
    let mut env = BTreeMap::new();
    env.insert("ANTHROPIC_AUTH_TOKEN".into(), slot.slot_token.clone());
    env.insert("ANTHROPIC_API_KEY".into(), String::new());
    env.insert("CLAUDE_CODE_OAUTH_TOKEN".into(), String::new());
    env.insert("ANTHROPIC_BASE_URL".into(), url);
    env.insert("ANTHROPIC_MODEL".into(), model.clone());
    // `_model_alias_env(all_aliases=True)` (:1965–1981) — every alias
    // model variable carries the participant model (mediation
    // completeness — subagent calls must not name a different model).
    for alias in [
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_FABLE_MODEL",
        "CLAUDE_CODE_SUBAGENT_MODEL",
    ] {
        env.insert(alias.into(), model.clone());
    }
    env.insert("CLAUDE_CONFIG_DIR".into(), CLAUDE_CONFIG_DIR.to_string());
    let setup = "mkdir -p $CLAUDE_CONFIG_DIR/debug $CLAUDE_CONFIG_DIR/projects/-app \
         $CLAUDE_CONFIG_DIR/shell-snapshots $CLAUDE_CONFIG_DIR/statsig \
         $CLAUDE_CONFIG_DIR/todos $CLAUDE_CONFIG_DIR/skills && \
         if [ -d ~/.claude/skills ]; then cp -r ~/.claude/skills/. \
         $CLAUDE_CONFIG_DIR/skills/ 2>/dev/null || true; fi"
        .to_string();
    let command = format!(
        "export PATH=\"$HOME/.local/bin:$PATH\"; {shell_var}=\"${env_var}\"; \
         unset {env_var}; printf \"%s\" \"${shell_var}\" | \
         claude --verbose --output-format=stream-json \
         --effort {eff} --disallowedTools WebSearch,WebFetch \
         --permission-mode=bypassPermissions --print 2>&1 | tee {txt}",
        shell_var = shell_var,
        env_var = env_var,
        eff = py_shlex_quote(&effort),
        txt = CLAUDE_TXT
    );
    let argv = bash_exec(command);
    let mut unrendered = Vec::new();
    apply_disable_switches(rec, &mut env, &argv, &mut unrendered)?;
    env.insert(env_var, instruction.to_string());
    let env = closed_env(env, slot)?;
    let paths = rendered_paths(rec)?;
    if paths.len() != 2 {
        return Err(err(
            "rendered_paths",
            "claude-code renders .claude.json + .hh-env",
        ));
    }
    // `.claude.json` — a real Claude state key; seeds the closed cell's
    // onboarding state (the adapter writes the file only for MCP, so
    // this is the HH-declared member under `rendered_paths`).
    let claude_json = canonical_bytes(&Json::obj([("hasCompletedOnboarding", Json::Bool(true))]));
    // `.hh-env` — the env manifest the ST2 composite verifies: sorted
    // `NAME=value` lines, the slot token elided, the exec-time
    // instruction var excluded (both are slot-varying — files are not).
    let mut lines = vec![
        "# hh-harness-render/1 env manifest — token elided, exec-time instruction var excluded"
            .to_string(),
    ];
    for (k, v) in env
        .iter()
        .filter(|(k, _)| !k.starts_with("CLAUDE_CODE_INSTRUCTION_"))
    {
        let v = if v.contains(&slot.slot_token) {
            v.replace(&slot.slot_token, ELIDED)
        } else {
            v.clone()
        };
        lines.push(format!("{k}={v}"));
    }
    let hh_env = format!("{}\n", lines.join("\n")).into_bytes();
    let files = expect_files(
        vec![
            RenderedFile {
                path: paths[0].clone(),
                bytes: claude_json,
            },
            RenderedFile {
                path: paths[1].clone(),
                bytes: hh_env,
            },
        ],
        &paths,
    )?;
    Ok(RenderedConfig {
        argv,
        setup_argv: Some(bash_exec(setup)),
        env,
        files,
        instruction: slot.instruction.clone(),
    })
}

/// `render(rec, slot, use_)` — validate the admission, flag hazards,
/// dispatch on `harness.name`. Claude Code refuses an `HirTarget` use
/// (Q-X26 term 3 — black-box arm only). A harness with no renderer is a
/// typed `renderer_unknown` (later passes' harnesses land theirs then).
pub fn render(
    rec: &Json,
    slot: &SlotInputs,
    use_: RenderUse,
) -> Result<RenderedConfig, RenderError> {
    admission::validate(rec, &ValidationContext::first_pass())?;
    let name = rec
        .get("harness")
        .and_then(|h| h.get("name"))
        .and_then(Json::as_str)
        .ok_or_else(|| err("member_missing", "harness.name required"))?;
    if use_ == RenderUse::HirTarget && name == CLAUDE_CODE {
        return Err(err(
            "qx26_black_box",
            "claude-code is a black-box arm only — never an HIR target (Q-X26 term 3)",
        ));
    }
    let hazards = precheck_instruction(name, &slot.instruction);
    if !hazards.is_empty() {
        return Err(err(
            "instruction_hazard",
            format!("{} hazards: {:?}", hazards.len(), hazards),
        ));
    }
    match name {
        MINI_SWE_AGENT => render_mini(rec, slot),
        PI => render_pi(rec, slot),
        CLAUDE_CODE => render_claude(rec, slot),
        other => Err(err(
            "renderer_unknown",
            format!("no first-pass renderer for {other}"),
        )),
    }
}

// ── recorded forms (secrets elided; X-05) ───────────────────────────────────

/// `elided_form(rc, token)` — the recorded render: argv/env with every
/// token occurrence elided, files by content digest (never their bytes —
/// the byte-exact file content is slot-invariant but stays out of the
/// recorded form anyway; ST2 recomputes the composite in the cell).
pub fn elided_form(rc: &RenderedConfig, token: &str) -> Json {
    let elide = |s: &str| s.replace(token, ELIDED);
    Json::obj([
        (
            "argv",
            Json::Arr(rc.argv.iter().map(|a| Json::str(elide(a))).collect()),
        ),
        (
            "setup_argv",
            rc.setup_argv
                .as_ref()
                .map(|a| Json::Arr(a.iter().map(|x| Json::str(elide(x))).collect()))
                .unwrap_or(Json::Null),
        ),
        (
            "env",
            Json::Obj(
                rc.env
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(elide(v))))
                    .collect(),
            ),
        ),
        (
            "files",
            Json::Arr(
                rc.files
                    .iter()
                    .map(|f| {
                        Json::obj([
                            ("path", Json::str(&f.path)),
                            (
                                "sha256",
                                Json::str(format!("sha256:{}", sha256_hex(&f.bytes))),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

/// `rendered_config_hash(rc, token)` — the slot record's rendered-config
/// hash: sha256 over the canonical secret-elided form (REQ-XC-307). Any
/// non-secret input change moves it; the token value never does.
pub fn rendered_config_hash(rc: &RenderedConfig, token: &str) -> String {
    format!(
        "sha256:{}",
        sha256_hex(elided_form(rc, token).to_canonical_string().as_bytes())
    )
}

/// `rendered_files_digest(files)` — the ST2 composite verbatim:
/// `sha256sum <paths> | sha256sum | cut -d' ' -f1` (R6.22's probe recipe).
/// Equals `config.template_hash` on every slot because files are
/// slot-invariant.
pub fn rendered_files_digest(files: &[RenderedFile]) -> String {
    let mut listing = String::new();
    for f in files {
        listing.push_str(&format!("{}  {}\n", sha256_hex(&f.bytes), f.path));
    }
    format!("sha256:{}", sha256_hex(listing.as_bytes()))
}

/// `rendered_record(rc, rec, slot)` — the `hh-harness-render/1` member
/// stamped into the slot record's `participant.render` (REQ-XC-307's
/// slot-record hash; AC-7's participant identity for mini-swe-agent:
/// the Harbor commit, version, full `-c` spec list, `mini.yaml` sha256
/// and the frozen `litellm_version` — REQ-XD-121).
pub fn rendered_record(
    rc: &RenderedConfig,
    rec: &Json,
    slot: &SlotInputs,
) -> Result<Json, RenderError> {
    let name = rec
        .get("harness")
        .and_then(|h| h.get("name"))
        .and_then(Json::as_str)
        .ok_or_else(|| err("member_missing", "harness.name required"))?;
    let c = cfg(rec)?;
    let mut m = vec![
        ("schema", Json::str(RENDER_SCHEMA)),
        (
            "renderer_id",
            Json::str(sfield(c, "renderer_id", "config")?),
        ),
        (
            "renderer_version",
            Json::str(sfield(c, "renderer_version", "config")?),
        ),
        ("admission_ref", Json::str(admission::admission_id(rec)?)),
        (
            "config_hash",
            Json::str(rendered_config_hash(rc, &slot.slot_token)),
        ),
        ("files_digest", Json::str(rendered_files_digest(&rc.files))),
        (
            "instruction_sha256",
            Json::str(format!("sha256:{}", sha256_hex(&rc.instruction))),
        ),
        (
            "env_names",
            Json::Arr(rc.env.keys().map(Json::str).collect()),
        ),
        (
            "limits",
            Json::obj([
                (
                    "agent_wall_timeout_s",
                    Json::Int(slot.agent_wall_timeout_s as i64),
                ),
                (
                    "proxy_budget_ceiling_usd_micros",
                    Json::Int(slot.proxy_budget_ceiling_usd_micros as i64),
                ),
                // Q-X29/REQ-XD-122: the ceiling is declared; when it
                // binds the outcome is infra-uncertain, never a task loss.
                ("ceiling_binding", Json::str("infra_uncertain")),
            ]),
        ),
        ("participant", participant_identity(rec, slot)?),
    ];
    if let Some(v) = rec
        .get("runtime")
        .and_then(|r| r.get("litellm"))
        .and_then(|l| l.get("version"))
        .and_then(Json::as_str)
    {
        m.push(("litellm_version", Json::str(v)));
    }
    let _ = name;
    Ok(Json::Obj(
        m.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
    ))
}

fn participant_identity(rec: &Json, slot: &SlotInputs) -> Result<Json, RenderError> {
    let h = rec
        .get("harness")
        .and_then(Json::as_obj)
        .ok_or_else(|| err("member_missing", "harness required"))?;
    let name = sfield(h, "name", "harness")?;
    let url = base_url(
        slot,
        rec.get("config")
            .and_then(|c| c.get("base_url_suffix"))
            .and_then(Json::as_str)
            .unwrap_or(""),
    );
    let c_specs = if name == MINI_SWE_AGENT {
        vec![
            "mini".to_string(),
            "model.model_kwargs.timeout=3600".to_string(),
            format!(
                "model.model_kwargs.reasoning_effort={}",
                admission::FIRST_PASS_EFFORT
            ),
            "model.model_kwargs.api_key=$MINI_SWE_AGENT_MODEL_API_KEY_JSON".to_string(),
            "model.model_kwargs.custom_llm_provider=openai".to_string(),
            "model.model_class=litellm".to_string(),
            format!("model.model_kwargs.api_base={url}"),
        ]
    } else {
        Vec::new()
    };
    let mut p = vec![
        ("name", Json::str(&name)),
        ("version", Json::str(sfield(h, "version", "harness")?)),
        ("harbor_commit", Json::str(crate::harbor::HARBOR_PIN)),
    ];
    // REQ-XD-121 — one image built once: digest and dependency lock
    // recorded on every participant identity (REQ-XE-001's record).
    if let Some(b) = rec
        .get("install")
        .and_then(|i| i.get("bundle"))
        .and_then(Json::as_obj)
    {
        if let (Some(d), Some(l)) = (
            b.get("digest").and_then(Json::as_str),
            b.get("resolved_manifest_ref").and_then(Json::as_str),
        ) {
            p.push((
                "image",
                Json::obj([("digest", Json::str(d)), ("lock", Json::str(l))]),
            ));
        }
    }
    if name == MINI_SWE_AGENT {
        p.push(("mini_version", Json::str(MINI_SWE_AGENT_VERSION)));
        p.push(("mini_yaml_sha256", Json::str(MINI_YAML_SHA256)));
        p.push((
            "c_spec_list",
            Json::Arr(c_specs.iter().map(Json::str).collect()),
        ));
        p.push((
            "harbor_flags",
            Json::Arr(
                [
                    "--yolo",
                    "--cost-limit 0",
                    "--exit-immediately",
                    "--model",
                    "--task",
                    "--output",
                ]
                .into_iter()
                .map(Json::str)
                .collect(),
            ),
        ));
    }
    if name == CLAUDE_CODE {
        // §5.4's record realization: cost is an upper-bound `estimate`
        // (count_tokens refused by the proxy; caching presumed absent).
        p.push((
            "cost_basis",
            Json::obj([
                ("basis", Json::str("upper_bound_estimate")),
                ("count_tokens", Json::str("refused_route")),
                ("cached_share", Json::str("presumed_absent")),
            ]),
        ));
    }
    Ok(Json::Obj(
        p.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
    ))
}

/// `check_cell_listing(paths)` — the REQ-XC-308 filesystem fixture:
/// `/logs/agent` and `/installed-agent` present (the latter is the
/// adapter's `setup()` mkdir — installed/base :1173), `/tests` and
/// `/solution` absent in the agent phase (REQ-XB-106's boundary).
pub fn check_cell_listing(paths: &[String]) -> Result<(), RenderError> {
    for need in [LOGS_AGENT, INSTALLED_AGENT] {
        if !paths.iter().any(|p| p == need) {
            return Err(err(
                "cell_listing",
                format!("{need} absent — the cell contract requires it"),
            ));
        }
    }
    for banned in [TESTS_DIR, SOLUTION_DIR] {
        if paths.iter().any(|p| p == banned) {
            return Err(err(
                "cell_listing",
                format!("{banned} present before its phase — a boundary leak"),
            ));
        }
    }
    Ok(())
}

/// `check_recording_key(recording, rec)` — REQ-XD-121: an L1 recording
/// keyed by `litellm_version` admits only the version the slot's
/// installed environment carries (the admission's `runtime.litellm`).
/// A mismatch — or a key on a non-LiteLLM harness — refuses.
pub fn check_recording_key(recording: &Json, rec: &Json) -> Result<(), RenderError> {
    let want = rec
        .get("runtime")
        .and_then(|r| r.get("litellm"))
        .and_then(|l| l.get("version"))
        .and_then(Json::as_str);
    match (
        recording.get("litellm_version").and_then(Json::as_str),
        want,
    ) {
        (Some(got), Some(w)) if got == w => Ok(()),
        (Some(got), _) => Err(err(
            "recording_key",
            format!("recording litellm_version {got} is not the installed-environment version"),
        )),
        (None, Some(w)) => Err(err(
            "recording_key",
            format!("recording lacks litellm_version — the installed environment carries {w}"),
        )),
        (None, None) => Ok(()),
    }
}

/// `receipt_effort(receipt)` — read the `hh.model_io/1` receipt's
/// `mutations.effort{requested,applied}` (R6.10's `GenParamPolicy` fold —
/// REQ-XD-082's proxy half recorded per call).
pub fn receipt_effort(receipt: &Json) -> Result<(Option<String>, Option<String>), RenderError> {
    let eff = receipt
        .get("mutations")
        .and_then(|m| m.get("effort"))
        .ok_or_else(|| err("receipt_shape", "receipt lacks mutations.effort"))?;
    if eff == &Json::Null {
        return Ok((None, None));
    }
    let m = eff
        .as_obj()
        .ok_or_else(|| err("receipt_shape", "mutations.effort must be an object"))?;
    let get = |k: &str| m.get(k).and_then(Json::as_str).map(str::to_string);
    Ok((get("requested"), get("applied")))
}

/// `anthropic_host(host)` — `anthropic.com` or a subdomain (the §5.4
/// host class every one of whose attempts is denied and counted).
pub fn anthropic_host(host: &str) -> bool {
    host == "anthropic.com" || host.ends_with(".anthropic.com")
}

/// `pattern_covers(pattern, host)` — an `expected_denied` pattern covers
/// `host` (`*.s` = the suffix itself or a subdomain). Exported for the
/// R6.25 smoke/mediator legs (CC1 — one coverage rule).
pub fn pattern_covers(pattern: &str, host: &str) -> bool {
    match pattern.strip_prefix("*.") {
        Some(suffix) => host == suffix || host.ends_with(&format!(".{suffix}")),
        None => pattern == host,
    }
}

/// `sm_pos_assert(rec, mediator_rows, receipts)` — the pure
/// records-level REQ-XC-327 assertion R6.25's SM-pos checker reuses
/// (CC1): every Anthropic-host mediator row is denied *and* its host is
/// covered by `egress.expected_denied` (logged residuals, never
/// granted); every receipt's downstream path is `/v1/messages` on the
/// proxy — or a `refused_route` disposition at the declared
/// `count_tokens` route (§5.4's estimate fallback).
pub fn sm_pos_assert(
    rec: &Json,
    mediator_rows: &[Json],
    receipts: &[Json],
) -> Result<(), RenderError> {
    let expected: Vec<String> = rec
        .get("egress")
        .and_then(|e| e.get("expected_denied"))
        .and_then(Json::as_arr)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    x.as_obj()
                        .and_then(|m| m.get("host_or_pattern"))
                        .and_then(Json::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .ok_or_else(|| err("member_missing", "egress.expected_denied required"))?;
    for row in mediator_rows {
        let m = row
            .as_obj()
            .ok_or_else(|| err("mediator_row", "mediator row must be an object"))?;
        let host = sfield(m, "host", "mediator_row")?;
        if !anthropic_host(&host) {
            continue;
        }
        let decision = sfield(m, "decision", "mediator_row")?;
        if !matches!(decision.as_str(), "deny" | "denied") {
            return Err(err(
                "anthropic_attempt",
                format!("anthropic host {host} decision {decision} — must be denied"),
            ));
        }
        if !expected.iter().any(|p| pattern_covers(p, &host)) {
            return Err(err(
                "expected_denied",
                format!("denied anthropic host {host} not counted in expected_denied"),
            ));
        }
    }
    for receipt in receipts {
        let path = receipt
            .get("downstream")
            .and_then(|d| d.get("path"))
            .and_then(Json::as_str)
            .ok_or_else(|| err("receipt_shape", "receipt lacks downstream.path"))?;
        if path == MESSAGES_ROUTE {
            continue;
        }
        let disposition = receipt
            .get("disposition")
            .and_then(Json::as_str)
            .unwrap_or("");
        if disposition == "refused_route" && path == COUNT_TOKENS_ROUTE {
            continue;
        }
        return Err(err(
            "receipt_route",
            format!("receipt path {path} — every served route is /v1/messages; count_tokens is refused_route"),
        ));
    }
    Ok(())
}
