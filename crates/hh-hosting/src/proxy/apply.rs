//! `proxy::apply` — header filtering, the declared field policy, and
//! the recorded-mutation application (§4.2; REQ-XC-103/105/106/107).
//!
//! Every transformation is *declared* on the lane document and
//! *recorded* on the receipt with its JSON path and requested/applied
//! values — nothing else changes (same-dialect pass-through, F6).

use std::collections::BTreeMap;

use hharness_wire::json::PJson;
use hharness_wire::sha256::sha256_hex;

use super::lane::{FieldOp, FieldRule, ProxyLane, Surface};

/// The closed inbound-header forward allowlist (§4.2 Headers). Auth
/// headers are never forwarded and never recorded; `set-cookie` and
/// `*-key|*-token|*-secret` are never forwarded or recorded; every
/// other name is recorded with its value dropped.
const FORWARD_HEADERS: &[&str] = &[
    "content-type",
    "accept",
    "anthropic-version",
    "anthropic-beta",
    "user-agent",
];

/// The inbound-header filter's output: `kept` rides `forward_raw`'s
/// `kept_headers` verbatim; `recorded_names` is every other header's
/// name (values dropped). Auth names are absent from both.
#[derive(Debug, Clone, Default)]
pub struct FilteredHeaders {
    /// `(name, value)` pairs admitted for forwarding.
    pub kept: Vec<(String, String)>,
    /// Names seen and dropped (auth/secret-shaped names excluded).
    pub recorded_names: Vec<String>,
}

/// `filter_headers(inbound)` — the closed allowlist split.
pub fn filter_headers(headers: &BTreeMap<String, String>) -> FilteredHeaders {
    let mut out = FilteredHeaders::default();
    for (name, value) in headers {
        let n = name.to_ascii_lowercase();
        if matches!(
            n.as_str(),
            "authorization" | "x-api-key" | "proxy-authorization"
        ) || n == "set-cookie"
            || n.ends_with("-key")
            || n.ends_with("-token")
            || n.ends_with("-secret")
            || n == "host"
            || n == "content-length"
        {
            continue;
        }
        if FORWARD_HEADERS.contains(&n.as_str()) {
            out.kept.push((n.clone(), value.clone()));
        } else {
            out.recorded_names.push(n);
        }
    }
    out
}

/// One recorded refusal (the receipt's `mutations.refused` and the
/// surface-shaped error body's inputs).
#[derive(Debug, Clone)]
pub struct Refusal {
    /// The downstream HTTP status.
    pub status: u16,
    /// The refusal kind (a `hh_proxy` error code).
    pub kind: &'static str,
    /// The JSON path that caused it, when one applies.
    pub path: Option<String>,
    /// The human-readable detail.
    pub detail: String,
}

impl Refusal {
    /// The recorded form.
    pub fn to_json(&self) -> hharness_wire::json::Json {
        hharness_wire::json::Json::obj([
            ("kind", hharness_wire::json::Json::str(self.kind)),
            (
                "path",
                self.path
                    .as_deref()
                    .map(harness_str)
                    .unwrap_or(hharness_wire::json::Json::Null),
            ),
            ("detail", hharness_wire::json::Json::str(&self.detail)),
        ])
    }
}

fn harness_str(s: &str) -> hharness_wire::json::Json {
    hharness_wire::json::Json::str(s.to_owned())
}

/// The recorded mutation set (§4.3 `mutations` — every transformation
/// the proxy applied, versioned by `proxy_version` on the receipt).
#[derive(Debug, Clone, Default)]
pub struct Mutations {
    /// The model pin (`requested`, `applied`).
    pub model: Option<(Option<String>, String)>,
    /// The effort pin (`requested`, `applied`, `path`).
    pub effort: Option<(Option<String>, String, String)>,
    /// Stripped members `(path, value_sha256)`.
    pub stripped: Vec<(String, String)>,
    /// Injected members `(path, value)`.
    pub injected: Vec<(String, PJson)>,
    /// Forwarded-but-recorded-not-enforced paths (`budget_tokens`).
    pub not_enforced: Vec<String>,
}

impl Mutations {
    /// The receipt's `mutations` member.
    pub fn to_json(&self) -> hharness_wire::json::Json {
        use hharness_wire::json::Json;
        let opt = |v: &Option<String>| v.as_ref().map(Json::str).unwrap_or(Json::Null);
        Json::obj([
            (
                "model",
                self.model
                    .as_ref()
                    .map(|(req, app)| {
                        Json::obj([
                            ("requested", opt(req)),
                            ("applied", Json::str(app)),
                            ("path", Json::str("model")),
                        ])
                    })
                    .unwrap_or(Json::Null),
            ),
            (
                "effort",
                self.effort
                    .as_ref()
                    .map(|(req, app, path)| {
                        Json::obj([
                            ("requested", opt(req)),
                            ("applied", Json::str(app)),
                            ("path", Json::str(path)),
                        ])
                    })
                    .unwrap_or(Json::Null),
            ),
            (
                "stripped",
                Json::Arr(
                    self.stripped
                        .iter()
                        .map(|(p, s)| {
                            Json::obj([("path", Json::str(p)), ("value_sha256", Json::str(s))])
                        })
                        .collect(),
                ),
            ),
            (
                "injected",
                Json::Arr(
                    self.injected
                        .iter()
                        .map(|(p, v)| {
                            Json::obj([
                                ("path", Json::str(p)),
                                ("value", Json::str(v.to_provider_string())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "not_enforced",
                Json::Arr(self.not_enforced.iter().map(Json::str).collect()),
            ),
        ])
    }
}

/// `get_path(value, "a.b.c")` — dotted member lookup (path-qualified,
/// AR-10).
pub fn get_path<'a>(v: &'a PJson, path: &str) -> Option<&'a PJson> {
    let mut cur = v;
    for seg in path.split('.') {
        cur = cur.get(seg)?;
    }
    Some(cur)
}

/// `set_path(value, "a.b.c", v)` — overwrite-or-insert, creating
/// intermediate objects (a pin may insert `output_config.effort` under
/// an absent `output_config`).
pub fn set_path(v: &mut PJson, path: &str, val: PJson) {
    let mut segs = path.split('.').peekable();
    let mut cur = v;
    while let Some(seg) = segs.next() {
        if segs.peek().is_none() {
            if let PJson::Obj(m) = cur {
                m.insert(seg.to_string(), val);
            }
            return;
        }
        if !matches!(cur, PJson::Obj(_)) {
            *cur = PJson::Obj(BTreeMap::new());
        }
        let PJson::Obj(m) = cur else { return };
        cur = m
            .entry(seg.to_string())
            .or_insert_with(|| PJson::Obj(BTreeMap::new()));
    }
}

/// `remove_member_recursive(value, member)` — strip every object member
/// named `member` at any depth, recording `(path, value_sha256)` per
/// removal (`cache_control` rides this — §4.2 "all paths").
fn strip_recursive(v: &mut PJson, member: &str, at: &str, out: &mut Vec<(String, String)>) {
    match v {
        PJson::Obj(m) => {
            if let Some(gone) = m.remove(member) {
                out.push((
                    if at.is_empty() {
                        member.to_string()
                    } else {
                        format!("{at}.{member}")
                    },
                    sha256_hex(gone.to_provider_string().as_bytes()),
                ));
            }
            for (k, child) in m.iter_mut() {
                strip_recursive(child, member, &format!("{at}.{k}"), out);
            }
        }
        PJson::Arr(items) => {
            for (i, child) in items.iter_mut().enumerate() {
                strip_recursive(child, member, &format!("{at}[{i}]"), out);
            }
        }
        _ => {}
    }
}

/// Apply one declared [`FieldRule`] to a top-level member.
fn apply_rule(body: &mut PJson, rule: &FieldRule, out: &mut Mutations) -> Result<(), Refusal> {
    let member = rule.member.as_str();
    let refuse = |path: Option<String>, detail: String| Refusal {
        status: 400,
        kind: "refused_policy",
        path,
        detail,
    };
    match &rule.op {
        FieldOp::Strip => {
            if let PJson::Obj(m) = body {
                if let Some(v) = m.remove(member) {
                    out.stripped.push((
                        member.to_string(),
                        sha256_hex(v.to_provider_string().as_bytes()),
                    ));
                }
            }
        }
        FieldOp::StripRecursive => {
            strip_recursive(body, member, "", &mut out.stripped);
        }
        FieldOp::StripAutoElseRefuse => {
            if let PJson::Obj(m) = body {
                if let Some(v) = m.get(member) {
                    let is_auto = match v {
                        PJson::Str(s) => s == "auto",
                        PJson::Obj(o) => o.get("type").and_then(PJson::as_str) == Some("auto"),
                        _ => false,
                    };
                    if is_auto {
                        let v = m.remove(member).unwrap();
                        out.stripped.push((
                            member.to_string(),
                            sha256_hex(v.to_provider_string().as_bytes()),
                        ));
                    } else {
                        return Err(refuse(
                            Some(member.to_string()),
                            format!("{member} values other than auto are unsupported upstream"),
                        ));
                    }
                }
            }
        }
        FieldOp::RefuseIfGt(bound) => {
            if let PJson::Obj(m) = body {
                if let Some(v) = m.get(member) {
                    let n = v.as_i64().unwrap_or(i64::MAX);
                    if n > *bound {
                        return Err(refuse(
                            Some(member.to_string()),
                            format!("{member} > {bound} is unsupported upstream"),
                        ));
                    }
                    // `n:1`-style provider-default equivalents strip
                    // with notice rather than forward (the upstream
                    // cannot honour the member at all).
                    let v = m.remove(member).unwrap();
                    out.stripped.push((
                        member.to_string(),
                        sha256_hex(v.to_provider_string().as_bytes()),
                    ));
                }
            }
        }
        FieldOp::Refuse => {
            if let PJson::Obj(m) = body {
                if m.contains_key(member) {
                    return Err(refuse(
                        Some(member.to_string()),
                        format!("{member} is unsupported upstream"),
                    ));
                }
            }
        }
        FieldOp::ForwardRecorded => {
            if get_path(body, member).is_some() {
                out.not_enforced.push(member.to_string());
            }
        }
    }
    Ok(())
}

/// `apply_policy(body, surface, lane, slot_effort) → Mutations |
/// Refusal` — the whole declared transformation for one request:
/// per-surface field rules (top-level + `*`), then the `model` and
/// effort-path pins (`GenParamPolicy.pin` is the AR-10 carrier — the
/// effort pin's value is slot-declared, recorded requested + applied).
/// Anything not named by the rules passes through verbatim.
pub fn apply_policy(
    body: &mut PJson,
    surface: Surface,
    lane: &ProxyLane,
    slot_effort: &str,
) -> Result<Mutations, Refusal> {
    let mut out = Mutations::default();
    let key = surface.as_str();
    for rules in [lane.field_policy.get("*"), lane.field_policy.get(key)]
        .into_iter()
        .flatten()
    {
        for rule in rules {
            apply_rule(body, rule, &mut out)?;
        }
    }
    // ── pins (AR-10 path-qualified): `model` and the surface's
    // effort path are pinned to the declared values — overwrite or
    // insert, requested recorded either way. ──
    if let PJson::Obj(m) = body {
        let requested_model = m
            .get("model")
            .and_then(PJson::as_str)
            .map(|s| s.to_string());
        if requested_model.as_deref() != Some(lane.model.as_str()) {
            if let Some(pin) = lane.gen_param_policy.pin_at("model") {
                m.insert(
                    "model".to_string(),
                    PJson::Str(pin.as_str().unwrap_or(&lane.model).to_string()),
                );
                if requested_model.is_none() {
                    out.injected
                        .push(("model".to_string(), PJson::Str(lane.model.clone())));
                }
            }
            out.model = Some((requested_model, lane.model.clone()));
        }
        let effort_path = surface.effort_path();
        let requested_effort = get_path(body, effort_path)
            .and_then(PJson::as_str)
            .map(|s| s.to_string());
        let absent = requested_effort.is_none();
        set_path(body, effort_path, PJson::Str(slot_effort.to_string()));
        if absent {
            out.injected
                .push((effort_path.to_string(), PJson::Str(slot_effort.to_string())));
        }
        out.effort = Some((
            requested_effort,
            slot_effort.to_string(),
            effort_path.to_string(),
        ));
    }
    Ok(out)
}

/// `error_body(surface, err_type, code, message)` — the surface-shaped
/// error document (§4.2 "typed, surface-shaped"): OpenAI
/// `{error:{type,message,code}}`, Anthropic `{type:error,
/// error:{type,message}}`; a route-less refusal uses the generic
/// `not_found_error` envelope.
pub fn error_body(surface: Option<Surface>, err_type: &str, code: &str, message: &str) -> String {
    match surface {
        Some(Surface::AnthropicMessages) => format!(
            "{{\"type\":\"error\",\"error\":{{\"type\":\"{err_type}\",\"message\":\"{message} ({code})\"}}}}"
        ),
        _ => format!(
            "{{\"error\":{{\"type\":\"{err_type}\",\"message\":\"{message}\",\"code\":\"{code}\"}}}}"
        ),
    }
}
