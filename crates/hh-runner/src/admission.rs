//! `hh-harness-admission/1` — one record per hosted harness, checked by
//! one pure validator before any slot (§5.1; REQ-XC-302 absorbing
//! REQ-XB-014/REQ-XC-201/REQ-XD-080; REQ-XC-202/214/301; REQ-XD-081;
//! AR-18/ADR-0436). Runner-crate record over `hharness-wire` canonical
//! JSON — never an `hh-embed/1` member (CC7); slot-facing refusals ride
//! R6.8's closed `withheld{…}` set, never a second vocabulary (X-04).
//!
//! Values in the committed first-pass records are `fixture-verified`
//! until W1 measures them (X-08): bundle digests, layer digests, the
//! template hash and the smoke-borne gate evidence are declared shapes,
//! not live claims.

use std::collections::BTreeMap;

use hharness_identity::idp::idp_id;
use hharness_wire::json::Json;

use crate::json_ext::JsonExt;

/// The record kind spelling.
pub const ADMISSION_SCHEMA: &str = "hh-harness-admission/1";
/// The `hh.hosting/1` ext-block key on the emitted declaration.
pub const HOSTING_EXT_KEY: &str = "hh.hosting/1";
/// The idp domain for `admission_id`.
pub const ADMISSION_DOMAIN: &str = "hh.harness.admission";
/// The first-pass effort pin (Q-X22).
pub const FIRST_PASS_EFFORT: &str = "high";
/// The only `must_reach` member — the cell's model relay.
pub const MODEL_RELAY: &str = "model_relay";
/// `admission_timing` — admission precedes any outcome read.
pub const OUTCOME_BLIND: &str = "outcome_blind";
/// The REQ-XC-313 residual every `in_environment` placement declares.
pub const SIDE_CHANNEL_MODEL_ROUTE: &str = "side_channel{model_route_reachable_from_agent_tools}";
/// The admitted program-wide Harbor pin (re-export for the validator).
pub const MINI_SWE_AGENT: &str = "mini-swe-agent";
/// mini-swe-agent's fixed first-pass version (REQ-XD-081).
pub const MINI_SWE_AGENT_VERSION: &str = "2.4.6";
/// Claude Code's record name (the Q-X26 posture).
pub const CLAUDE_CODE: &str = "claude-code";
/// Anthropic's disclosed statement (Q-X26 term 5, verbatim).
pub const UNSUPPORTED_STATEMENT: &str =
    "Anthropic doesn't support routing Claude Code to non-Claude models through any gateway";

/// A typed validation refusal — the truth-table rule names stay stable.
#[derive(Debug, Clone, PartialEq)]
pub struct AdmissionError {
    /// The refusal rule (the truth-table leg).
    pub rule: &'static str,
    /// The member path / verbatim detail.
    pub detail: String,
}

impl std::fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{ADMISSION_SCHEMA}: {}: {}", self.rule, self.detail)
    }
}
impl std::error::Error for AdmissionError {}

fn err(rule: &'static str, detail: impl Into<String>) -> AdmissionError {
    AdmissionError {
        rule,
        detail: detail.into(),
    }
}

fn obj<'a>(j: &'a Json, member: &str) -> Result<&'a BTreeMap<String, Json>, AdmissionError> {
    j.get(member)
        .and_then(Json::as_obj)
        .ok_or_else(|| err("member_missing", format!("{member} must be an object")))
}

fn mobj<'a>(
    m: &'a BTreeMap<String, Json>,
    k: &str,
    at: &str,
) -> Result<&'a BTreeMap<String, Json>, AdmissionError> {
    m.get(k)
        .and_then(Json::as_obj)
        .ok_or_else(|| err("member_missing", format!("{at}.{k} must be an object")))
}
fn sfield(m: &BTreeMap<String, Json>, k: &str, at: &str) -> Result<String, AdmissionError> {
    m.get(k)
        .and_then(Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| err("member_missing", format!("{at}.{k} must be a string")))
}
fn no_unknown(
    m: &BTreeMap<String, Json>,
    allowed: &[&str],
    at: &str,
) -> Result<(), AdmissionError> {
    for k in m.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(err("member_unknown", format!("{at}: unknown member {k}")));
        }
    }
    Ok(())
}
fn sha_ref(s: &str) -> bool {
    s.strip_prefix("sha256:")
        .map(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or(false)
}
/// `…@sha256:<64hex>` — an image ref pinned by digest (REQ-XC-304's
/// digest-only identity; the REQ-XB-014 alias refuses `@latest`/`@stable`
/// tag refs).
pub fn image_ref_by_digest(s: &str) -> bool {
    match s.rsplit_once('@') {
        Some((name, d)) => !name.is_empty() && sha_ref(d) && !name.contains(':'),
        None => false,
    }
}
/// A floating/`@latest`/`stable`/prerelease/wildcard/unlocked spelling
/// refuses (REQ-XD-081 + the REQ-XB-014 alias).
pub fn version_is_unpinned(v: &str) -> bool {
    let t = v.trim();
    if t.is_empty() {
        return true;
    }
    if t.starts_with('@') || t.starts_with(['^', '~', '>', '<', '=']) || t.contains('*') {
        return true;
    }
    let bare = t.strip_prefix('v').unwrap_or(t);
    let low = bare.to_ascii_lowercase();
    if [
        "latest", "stable", "nightly", "main", "master", "head", "dev", "next", "edge", "canary",
    ]
    .contains(&low.as_str())
    {
        return true;
    }
    if low.split('.').any(|seg| seg == "x") || !bare.starts_with(|c: char| c.is_ascii_digit()) {
        return true;
    }
    // A `-` suffix is a prerelease marker (2.4.6-rc1, 0.52.0-next.3).
    bare.contains('-')
}
fn check_pinned(v: &str, at: &str) -> Result<(), AdmissionError> {
    if version_is_unpinned(v) {
        return Err(err(
            "version_unpinned",
            format!("{at} {v:?} is floating/prerelease/unlocked — an exact pin is required"),
        ));
    }
    Ok(())
}

/// The validator's declared inputs — the program Harbor pin and the
/// protocol effort pin the record must match.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationContext {
    /// The admitted program-wide pin (`harbor::HARBOR_PIN`).
    pub harbor_pin: String,
    /// The protocol's effort pin (Q-X22 — `high` first pass).
    pub effort_pin: String,
}

impl ValidationContext {
    /// The first-pass context (the admitted commit fallback).
    pub fn first_pass() -> ValidationContext {
        ValidationContext {
            harbor_pin: crate::harbor::HARBOR_PIN.to_string(),
            effort_pin: FIRST_PASS_EFFORT.to_string(),
        }
    }
}

fn check_legal(m: &BTreeMap<String, Json>) -> Result<(), AdmissionError> {
    no_unknown(
        m,
        &[
            "posture",
            "license_spdx",
            "terms_quote",
            "terms_source_ref",
            "operator_ack_ref",
            "branding",
            "qx26",
        ],
        "legal",
    )?;
    let posture = sfield(m, "posture", "legal")?;
    if !["open", "proprietary", "source_available"].contains(&posture.as_str()) {
        return Err(err("legal_posture", format!("posture {posture} unknown")));
    }
    let branding = sfield(m, "branding", "legal")?;
    if !["name_only", "logo_permitted"].contains(&branding.as_str()) {
        return Err(err(
            "legal_branding",
            format!("branding {branding} unknown"),
        ));
    }
    if posture != "open" {
        // REQ-XC-202 — quoted terms + recorded operator acknowledgement.
        for k in ["terms_quote", "terms_source_ref", "operator_ack_ref"] {
            let v = m
                .get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .unwrap_or_default();
            if v.is_empty() {
                return Err(err(
                    "legal_terms",
                    format!("legal.{k} empty — a proprietary/source-available harness needs quoted terms and the operator ack"),
                ));
            }
        }
        if branding != "name_only" {
            return Err(err(
                "legal_branding",
                "a proprietary harness is name-only — no logo without recorded permission",
            ));
        }
    }
    Ok(())
}

fn check_qx26(m: &BTreeMap<String, Json>) -> Result<(), AdmissionError> {
    let q = m.get("qx26").and_then(Json::as_obj).ok_or_else(|| {
        err(
            "qx26_terms",
            "claude-code requires the Q-X26 terms verbatim (legal.qx26)",
        )
    })?;
    no_unknown(
        q,
        &[
            "unmodified_pinned_binary",
            "no_claude_account",
            "black_box_arm_only",
            "name_without_logo",
            "unsupported_statement_disclosed",
            "disclosure",
            "binary_sha256",
        ],
        "legal.qx26",
    )?;
    for k in [
        "unmodified_pinned_binary",
        "no_claude_account",
        "black_box_arm_only",
        "name_without_logo",
        "unsupported_statement_disclosed",
    ] {
        if q.get(k).and_then(Json::as_bool) != Some(true) {
            return Err(err(
                "qx26_terms",
                format!("legal.qx26.{k} must be true — Q-X26 term refused"),
            ));
        }
    }
    let d = sfield(q, "disclosure", "legal.qx26")?;
    if d != UNSUPPORTED_STATEMENT {
        return Err(err(
            "qx26_terms",
            "legal.qx26.disclosure must carry Anthropic's statement verbatim",
        ));
    }
    let sha = sfield(q, "binary_sha256", "legal.qx26")?;
    if !sha_ref(&sha) {
        return Err(err(
            "qx26_terms",
            "legal.qx26.binary_sha256 must be sha256:<64-hex> (checked at ST2/ST4)",
        ));
    }
    Ok(())
}

fn check_gates(
    g: &BTreeMap<String, Json>,
    harbor_agent: Option<&str>,
    ctx: &ValidationContext,
) -> Result<(), AdmissionError> {
    for n in 1..=9u8 {
        let key = format!("AG{n}");
        let want = harbor_agent.is_some() || n < 9;
        match g.get(&key) {
            None if want => return Err(err("gate_missing", format!("gates.{key} required"))),
            None => continue,
            Some(v) => {
                let gm = v
                    .as_obj()
                    .ok_or_else(|| err("gate_missing", format!("gates.{key} must be an object")))?;
                no_unknown(gm, &["result", "evidence_ref", "evidence"], "gates.*")?;
                let result = sfield(gm, "result", &key)?;
                match (result.as_str(), n == 9 && harbor_agent.is_none()) {
                    ("pass", _) => {}
                    ("n/a", true) => continue,
                    _ => {
                        return Err(err(
                            "gate_failed",
                            format!("gates.{key} result {result} — a failed gate never admits"),
                        ))
                    }
                }
                if n == 9 {
                    // REQ-XC-214 — the AG9 evidence member: the Harbor
                    // adapter at the pinned commit on the same image.
                    let ev = gm
                        .get("evidence")
                        .and_then(Json::as_obj)
                        .ok_or_else(|| err("ag9_evidence", "gates.AG9.evidence required"))?;
                    no_unknown(
                        ev,
                        &["harbor_adapter", "harbor_commit", "same_image"],
                        "gates.AG9.evidence",
                    )?;
                    if sfield(ev, "harbor_adapter", "AG9.evidence")? != harbor_agent.unwrap_or("") {
                        return Err(err(
                            "ag9_evidence",
                            "AG9 evidence names a different adapter than harness.harbor_agent",
                        ));
                    }
                    if sfield(ev, "harbor_commit", "AG9.evidence")? != ctx.harbor_pin {
                        return Err(err(
                            "ag9_evidence",
                            "AG9 harbor_commit is not the admitted pin — parity is not attempted",
                        ));
                    }
                    if ev.get("same_image").and_then(Json::as_bool) != Some(true) {
                        return Err(err(
                            "ag9_evidence",
                            "AG9 evidence must record same_image=true (O and H share the layer)",
                        ));
                    }
                } else if sfield(gm, "evidence_ref", &key)?.is_empty() {
                    return Err(err(
                        "gate_missing",
                        format!("gates.{key}.evidence_ref empty"),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// `validate(rec, ctx)` — the REQ-XC-302 truth table: every refusal
/// leg is a typed `AdmissionError`; a complete record admits.
pub fn validate(rec: &Json, ctx: &ValidationContext) -> Result<(), AdmissionError> {
    let m = rec
        .as_obj()
        .ok_or_else(|| err("member_missing", "record must be an object"))?;
    no_unknown(
        m,
        &[
            "schema",
            "admission_timing",
            "harness",
            "install",
            "runtime",
            "config",
            "egress",
            "client",
            "completion",
            "artifacts",
            "gates",
            "posture",
            "request_fingerprint",
            "attributes",
            "verification",
        ],
        "record",
    )?;
    if sfield(m, "schema", "record")? != ADMISSION_SCHEMA {
        return Err(err(
            "member_missing",
            "schema must be hh-harness-admission/1",
        ));
    }
    if sfield(m, "admission_timing", "record")? != OUTCOME_BLIND {
        return Err(err(
            "admission_timing",
            "admission is outcome-blind — the member must read outcome_blind",
        ));
    }
    // ── harness ────────────────────────────────────────────────────────
    let h = obj(rec, "harness")?;
    no_unknown(
        h,
        &[
            "name",
            "role",
            "harbor_agent",
            "version",
            "version_source",
            "freeze_at",
            "legal",
        ],
        "harness",
    )?;
    let name = sfield(h, "name", "harness")?;
    let role = sfield(h, "role", "harness")?;
    if !["reference", "comparator"].contains(&role.as_str()) {
        return Err(err(
            "role",
            format!("role {role} outside reference|comparator"),
        ));
    }
    let version = sfield(h, "version", "harness")?;
    check_pinned(&version, "harness.version")?;
    if !["release_tag", "pypi", "npm", "binary"]
        .contains(&sfield(h, "version_source", "harness")?.as_str())
    {
        return Err(err(
            "version_source",
            "version_source outside the closed set",
        ));
    }
    if sfield(h, "freeze_at", "harness")?.is_empty() {
        return Err(err("lock_missing", "harness.freeze_at required"));
    }
    let harbor_agent = match h.get("harbor_agent") {
        Some(Json::Null) | None => None,
        Some(Json::Str(s)) => Some(s.clone()),
        _ => {
            return Err(err(
                "member_missing",
                "harbor_agent must be a string or null",
            ))
        }
    };
    if name == MINI_SWE_AGENT && version != MINI_SWE_AGENT_VERSION {
        return Err(err(
            "mini_swe_version",
            format!(
                "mini-swe-agent stays at {MINI_SWE_AGENT_VERSION} (REQ-XD-081), found {version}"
            ),
        ));
    }
    let legal = mobj(h, "legal", "harness")?;
    check_legal(legal)?;
    if name == CLAUDE_CODE {
        check_qx26(legal)?;
    }
    // ── install ────────────────────────────────────────────────────────
    let i = obj(rec, "install")?;
    no_unknown(
        i,
        &[
            "recipe_ref",
            "bundle",
            "system_deps",
            "layers",
            "install_tree_paths",
            "version_probe",
        ],
        "install",
    )?;
    let rr = mobj(i, "recipe_ref", "install")?;
    no_unknown(rr, &["derived_from", "deviations"], "install.recipe_ref")?;
    let df = mobj(rr, "derived_from", "install.recipe_ref")?;
    no_unknown(
        df,
        &["harbor_commit", "adapter", "member", "attribution"],
        "recipe_ref.derived_from",
    )?;
    if sfield(df, "harbor_commit", "derived_from")? != ctx.harbor_pin {
        return Err(err(
            "attribution_pin",
            "recipe_ref must derive from the pinned Harbor commit's install()",
        ));
    }
    if sfield(df, "attribution", "derived_from")?.is_empty() {
        return Err(err("attribution_pin", "attribution required"));
    }
    match i.get("bundle").and_then(Json::as_obj) {
        None => return Err(err("digest_missing", "install.bundle required")),
        Some(b) => {
            no_unknown(
                b,
                &[
                    "digest",
                    "builder_base",
                    "resolved_manifest_ref",
                    "build_log_ref",
                    "built_at",
                ],
                "install.bundle",
            )?;
            if !sha_ref(&sfield(b, "digest", "bundle")?) {
                return Err(err(
                    "digest_missing",
                    "bundle.digest must be sha256:<64-hex>",
                ));
            }
            if !image_ref_by_digest(&sfield(b, "builder_base", "bundle")?) {
                return Err(err(
                    "digest_missing",
                    "bundle.builder_base must be digest-pinned",
                ));
            }
            for k in ["resolved_manifest_ref", "build_log_ref", "built_at"] {
                if sfield(b, k, "bundle")?.is_empty() {
                    return Err(err("member_missing", format!("bundle.{k} required")));
                }
            }
        }
    }
    let sd = mobj(i, "system_deps", "install")?;
    no_unknown(sd, &["set", "per_image_delta_ref"], "install.system_deps")?;
    match i
        .get("system_deps")
        .and_then(|j| j.get("set"))
        .and_then(Json::as_arr)
    {
        Some(a) if !a.is_empty() => {}
        _ => {
            return Err(err(
                "member_missing",
                "system_deps.set must be a non-empty array",
            ))
        }
    }
    if sfield(sd, "per_image_delta_ref", "system_deps")?.is_empty() {
        return Err(err(
            "member_missing",
            "system_deps.per_image_delta_ref required",
        ));
    }
    let layers = mobj(i, "layers", "install")?;
    if layers.is_empty() {
        return Err(err(
            "layer_digest",
            "install.layers must carry ≥ 1 (harness × task) layer",
        ));
    }
    for (task, layer) in layers {
        if !image_ref_by_digest(task) {
            return Err(err(
                "layer_digest",
                format!("layers key {task} is not a digest-pinned task image"),
            ));
        }
        let lv = layer.as_str().unwrap_or("");
        if !image_ref_by_digest(lv) {
            return Err(err(
                "layer_digest",
                format!("layers[{task}] {lv} is not digest-pinned — cells run it by digest only"),
            ));
        }
    }
    match i.get("install_tree_paths").and_then(Json::as_arr) {
        Some(a) if !a.is_empty() => {}
        _ => {
            return Err(err(
                "member_missing",
                "install_tree_paths must be non-empty",
            ))
        }
    }
    match i.get("version_probe").and_then(Json::as_arr) {
        Some(a) if !a.is_empty() && a.iter().all(|x| x.as_str().is_some()) => {}
        _ => {
            return Err(err(
                "member_missing",
                "version_probe must be a non-empty argv array",
            ))
        }
    }
    // ── runtime ────────────────────────────────────────────────────────
    let rt = obj(rec, "runtime")?;
    no_unknown(rt, &["node", "python", "litellm"], "runtime")?;
    for k in ["node", "python", "litellm"] {
        if let Some(rm) = rt.get(k) {
            let rm = rm
                .as_obj()
                .ok_or_else(|| err("member_missing", format!("runtime.{k} must be an object")))?;
            no_unknown(rm, &["version"], "runtime.*")?;
            check_pinned(
                &sfield(rm, "version", "runtime")?,
                &format!("runtime.{k}.version"),
            )?;
        }
    }
    if name == MINI_SWE_AGENT && rt.get("litellm").is_none() {
        return Err(err(
            "litellm_unpinned",
            "mini-swe-agent requires a pinned runtime.litellm (REQ-XD-081)",
        ));
    }
    // ── config ─────────────────────────────────────────────────────────
    let c = obj(rec, "config")?;
    no_unknown(
        c,
        &[
            "renderer_id",
            "renderer_version",
            "template_hash",
            "effort",
            "limits_ref",
            "disable_switches",
            "instruction_passing",
            "model_selector",
            "surface",
            "base_url_suffix",
            "rendered_paths",
        ],
        "config",
    )?;
    sfield(c, "renderer_id", "config")?;
    sfield(c, "renderer_version", "config")?;
    if !c
        .get("template_hash")
        .and_then(Json::as_str)
        .map(sha_ref)
        .unwrap_or(false)
    {
        return Err(err(
            "digest_missing",
            "config.template_hash must be sha256:<64-hex>",
        ));
    }
    let eff = mobj(c, "effort", "config")?;
    no_unknown(eff, &["mechanism", "value"], "config.effort")?;
    if sfield(eff, "mechanism", "effort")?.is_empty() {
        return Err(err("effort_pin", "config.effort.mechanism required"));
    }
    if sfield(eff, "value", "effort")? != ctx.effort_pin {
        return Err(err(
            "effort_pin",
            format!(
                "effort.value must equal the protocol pin {}",
                ctx.effort_pin
            ),
        ));
    }
    if sfield(c, "limits_ref", "config")?.is_empty() {
        return Err(err(
            "member_missing",
            "config.limits_ref (the Q-X29 set) required",
        ));
    }
    match c.get("disable_switches").and_then(Json::as_arr) {
        Some(a) if !a.is_empty() => {
            for (n, s) in a.iter().enumerate() {
                let sm = s.as_obj().ok_or_else(|| {
                    err("member_missing", format!("disable_switches[{n}] object"))
                })?;
                no_unknown(sm, &["switch", "purpose"], "disable_switches")?;
                sfield(sm, "switch", "disable_switches")?;
            }
        }
        _ => {
            return Err(err(
                "member_missing",
                "disable_switches must be a non-empty array",
            ))
        }
    }
    if !["argv", "env", "stdin"].contains(&sfield(c, "instruction_passing", "config")?.as_str()) {
        return Err(err(
            "member_missing",
            "instruction_passing outside argv|env|stdin",
        ));
    }
    sfield(c, "model_selector", "config")?;
    let surface = sfield(c, "surface", "config")?;
    if !["openai.chat", "anthropic.messages"].contains(&surface.as_str()) {
        return Err(err(
            "surface",
            format!("surface {surface} outside the closed set"),
        ));
    }
    if name == CLAUDE_CODE && surface != "anthropic.messages" {
        return Err(err("surface", "claude-code is anthropic.messages"));
    }
    match c.get("rendered_paths").and_then(Json::as_arr) {
        Some(a) if !a.is_empty() => {}
        _ => {
            return Err(err(
                "member_missing",
                "config.rendered_paths must be non-empty",
            ))
        }
    }
    sfield(c, "base_url_suffix", "config")?;
    // ── egress ─────────────────────────────────────────────────────────
    let e = obj(rec, "egress")?;
    no_unknown(e, &["expected_denied", "must_reach", "probe"], "egress")?;
    let mut expected: Vec<String> = Vec::new();
    match e.get("expected_denied").and_then(Json::as_arr) {
        Some(a) if !a.is_empty() => {
            for (n, x) in a.iter().enumerate() {
                let xm = x.as_obj().ok_or_else(|| {
                    err("expected_denied", format!("expected_denied[{n}] object"))
                })?;
                no_unknown(xm, &["host_or_pattern", "purpose"], "expected_denied")?;
                expected.push(sfield(xm, "host_or_pattern", "expected_denied")?);
            }
        }
        _ => {
            return Err(err(
                "expected_denied",
                "egress.expected_denied must be non-empty",
            ))
        }
    }
    match e.get("must_reach").and_then(Json::as_arr) {
        Some(a) if a.len() == 1 && a[0].as_str() == Some(MODEL_RELAY) => {}
        _ => return Err(err("must_reach", "egress.must_reach must be [model_relay]")),
    }
    // REQ-XC-311 — the admission includes an egress probe: `probe` names
    // the recorded probe log and its observed non-model attempts, each
    // covered by `expected_denied` (a probe the admission never logged
    // refuses — AC-5.3-6).
    match e.get("probe").and_then(Json::as_obj) {
        Some(p) => {
            no_unknown(p, &["log_ref", "observed"], "egress.probe")?;
            if sfield(p, "log_ref", "egress.probe")?.is_empty() {
                return Err(err(
                    "egress_probe",
                    "egress.probe.log_ref empty — the probe log is required",
                ));
            }
            let host_ok = |h: &str| {
                expected.iter().any(|pat| match pat.strip_prefix("*.") {
                    Some(sfx) => h == sfx || h.ends_with(&format!(".{sfx}")),
                    None => pat == h,
                })
            };
            for (n, x) in p
                .get("observed")
                .and_then(Json::as_arr)
                .cloned()
                .unwrap_or_default()
                .iter()
                .enumerate()
            {
                let h = x.as_str().unwrap_or("");
                if !host_ok(h) {
                    return Err(err(
                        "egress_probe",
                        format!("probe.observed[{n}] {h} not covered by expected_denied"),
                    ));
                }
            }
        }
        None => {
            return Err(err(
                "egress_probe",
                "egress.probe required — every admission includes an egress probe log (REQ-XC-311)",
            ))
        }
    }
    // ── client (REQ-XC-322 contributing — known or typed unknown) ──────
    let cl = obj(rec, "client")?;
    no_unknown(
        cl,
        &["model_timeout_s", "retry_policy", "streaming", "tolerates"],
        "client",
    )?;
    match cl.get("model_timeout_s") {
        Some(Json::Int(_)) => {}
        Some(Json::Str(s)) if s == "unknown" => {}
        _ => {
            return Err(err(
                "member_missing",
                "client.model_timeout_s must be an int or \"unknown\"",
            ))
        }
    }
    match cl.get("retry_policy").and_then(Json::as_str) {
        Some(s) if !s.is_empty() => {}
        _ => {
            return Err(err(
                "member_missing",
                "client.retry_policy must be a string (\"unknown\" allowed)",
            ))
        }
    }
    if cl.get("streaming").and_then(Json::as_bool).is_none() {
        return Err(err("member_missing", "client.streaming must be a bool"));
    }
    mobj(cl, "tolerates", "client")?;
    // ── completion / artifacts ─────────────────────────────────────────
    let cp = obj(rec, "completion")?;
    no_unknown(cp, &["detector", "native_status_parser"], "completion")?;
    if sfield(cp, "detector", "completion")? != "process_exit" {
        return Err(err(
            "member_missing",
            "completion.detector must be process_exit",
        ));
    }
    let ar = obj(rec, "artifacts")?;
    no_unknown(ar, &["native_paths", "atif"], "artifacts")?;
    match ar.get("native_paths").and_then(Json::as_arr) {
        Some(a) if !a.is_empty() => {}
        _ => {
            return Err(err(
                "member_missing",
                "artifacts.native_paths must be non-empty",
            ))
        }
    }
    match ar.get("atif") {
        Some(Json::Str(s)) if s == "none" => {}
        Some(v) => {
            let am = v.as_obj().ok_or_else(|| {
                err(
                    "member_missing",
                    "artifacts.atif must be \"none\" or an object",
                )
            })?;
            no_unknown(am, &["converter_at"], "artifacts.atif")?;
            if sfield(am, "converter_at", "artifacts.atif")? != ctx.harbor_pin {
                return Err(err(
                    "attribution_pin",
                    "artifacts.atif.converter_at must name the pinned Harbor commit",
                ));
            }
        }
        _ => return Err(err("member_missing", "artifacts.atif required")),
    }
    // ── gates ──────────────────────────────────────────────────────────
    let g = obj(rec, "gates")?;
    no_unknown(
        g,
        &[
            "AG1", "AG2", "AG3", "AG4", "AG5", "AG6", "AG7", "AG8", "AG9",
        ],
        "gates",
    )?;
    check_gates(g, harbor_agent.as_deref(), ctx)?;
    // ── posture ────────────────────────────────────────────────────────
    let p = obj(rec, "posture")?;
    no_unknown(p, &["placement", "residuals"], "posture")?;
    let placement = sfield(p, "placement", "posture")?;
    if !["in_environment", "lab_host"].contains(&placement.as_str()) {
        return Err(err(
            "placement",
            format!("placement {placement} outside the closed set"),
        ));
    }
    if placement == "in_environment" {
        let res = p
            .get("residuals")
            .and_then(Json::as_arr)
            .ok_or_else(|| err("posture_residual", "posture.residuals must be an array"))?;
        if !res
            .iter()
            .any(|r| r.as_str() == Some(SIDE_CHANNEL_MODEL_ROUTE))
        {
            return Err(err(
                "posture_residual",
                format!("an in_environment harness declares {SIDE_CHANNEL_MODEL_ROUTE}"),
            ));
        }
    }
    // ── request fingerprint (REQ-XC-313) ────────────────────────────
    let fp = obj(rec, "request_fingerprint")?;
    no_unknown(fp, &["user_agent", "header_names"], "request_fingerprint")?;
    if sfield(fp, "user_agent", "request_fingerprint")?.is_empty() {
        return Err(err(
            "member_missing",
            "request_fingerprint.user_agent must be non-empty",
        ));
    }
    match fp.get("header_names").and_then(Json::as_arr) {
        Some(a)
            if !a.is_empty()
                && a.iter()
                    .all(|x| x.as_str().map(|h| !h.is_empty()).unwrap_or(false)) => {}
        _ => {
            return Err(err(
                "member_missing",
                "request_fingerprint.header_names must be a non-empty string array",
            ))
        }
    }
    // ── attributes A1–A7 ───────────────────────────────────────────────
    let a = obj(rec, "attributes")?;
    for k in [
        "a1_wire_format",
        "a2_cached_share",
        "a3_trajectory",
        "a4_install",
        "a5_known_issues",
        "a6_priors",
        "a7_maintenance",
    ] {
        if a.get(k).is_none() {
            return Err(err("member_missing", format!("attributes.{k} required")));
        }
    }
    // ── verification (X-08 — fixture values, never live claims) ───────
    let v = obj(rec, "verification")?;
    no_unknown(v, &["class", "unverified_until", "members"], "verification")?;
    match sfield(v, "class", "verification")?.as_str() {
        "fixture-verified" | "recorded execution" => {}
        other => {
            return Err(err(
                "verification_class",
                format!("verification.class {other} — live-executed needs ADR-0363 evidence"),
            ))
        }
    }
    Ok(())
}

/// `admission_id(rec)` — the derived participant identity: a post-freeze
/// version change (or any identity-member change) is a new participant
/// (REQ-XD-081, Q-X27's recorded default). Derived, never stored.
pub fn admission_id(rec: &Json) -> Result<String, AdmissionError> {
    let h = obj(rec, "harness")?;
    let i = obj(rec, "install")?;
    let b = mobj(i, "bundle", "install")?;
    let key = Json::obj([
        ("name", Json::str(sfield(h, "name", "harness")?)),
        ("role", Json::str(sfield(h, "role", "harness")?)),
        ("version", Json::str(sfield(h, "version", "harness")?)),
        ("freeze_at", Json::str(sfield(h, "freeze_at", "harness")?)),
        ("bundle_digest", Json::str(sfield(b, "digest", "bundle")?)),
    ]);
    Ok(idp_id(
        ADMISSION_DOMAIN,
        key.to_canonical_string().as_bytes(),
    ))
}

/// `ovsh_eligible(rec, ctx)` — REQ-XC-214: O-vs-H comparison is attempted
/// only with AG9 evidence (the Harbor adapter at the pinned commit on
/// the same image). A record without `harbor_agent` is never eligible.
pub fn ovsh_eligible(rec: &Json, ctx: &ValidationContext) -> Result<bool, AdmissionError> {
    validate(rec, ctx)?;
    let h = obj(rec, "harness")?;
    if h.get("harbor_agent").and_then(Json::as_str).is_none() {
        return Ok(false);
    }
    let g = obj(rec, "gates")?;
    let ag9 = g
        .get("AG9")
        .and_then(Json::as_obj)
        .ok_or_else(|| err("ag9_evidence", "gates.AG9 missing"))?;
    Ok(ag9.get("result").and_then(Json::as_str) == Some("pass"))
}

/// `admit_slot_placement(rec)` — `lab_host` is declared now (adapter C's
/// `placement_supported` gained it under REQ-XC-301) but no `lab_host`
/// slot runs until AR-19: the placement declares, the slot refuses typed.
pub fn admit_slot_placement(rec: &Json) -> Result<(), AdmissionError> {
    let p = obj(rec, "posture")?;
    match sfield(p, "placement", "posture")?.as_str() {
        "in_environment" => Ok(()),
        other => Err(err(
            "placement_unsupported",
            format!("posture.placement {other} — declared only; a {other} slot refuses typed until AR-19"),
        )),
    }
}

/// `harness_declaration(rec)` — the REQ-XC-301 declaration the composite
/// participant carries: `container_installed` +
/// `HostingExt{model_io_intercept: base_url, process_placement}` +
/// observability `{end_state, model_io}` where `model_io` is earned only
/// from proxy receipts (a native-log lift never upgrades it).
pub fn harness_declaration(rec: &Json) -> Result<Json, AdmissionError> {
    let p = obj(rec, "posture")?;
    let placement = sfield(p, "placement", "posture")?;
    if !["in_environment", "lab_host"].contains(&placement.as_str()) {
        return Err(err("placement", "posture.placement outside the closed set"));
    }
    Ok(Json::obj([
        ("kind", Json::str("participant")),
        (
            "descriptor",
            Json::obj([
                ("class", Json::str("hosted")),
                ("hosting_mechanism", Json::str("container_installed")),
                (
                    "observability_level",
                    Json::Arr(vec![Json::str("end_state"), Json::str("model_io")]),
                ),
            ]),
        ),
        (
            "ext",
            Json::Obj(BTreeMap::from([(
                HOSTING_EXT_KEY.to_string(),
                Json::obj([
                    ("model_io_intercept", Json::str("base_url")),
                    ("process_placement", Json::str(placement)),
                ]),
            )])),
        ),
        ("model_io_evidence", Json::str("proxy_receipts_only")),
        ("admission_ref", Json::str(admission_id(rec)?)),
    ]))
}
