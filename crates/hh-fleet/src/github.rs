//! The GitHub Issues dialect for the fleet work-source lane (R4.7;
//! DF-S5.6-1; HUMAN-H2 "GitHub Issues + tunnel"; AC-R-2.12.6-9). Two
//! halves share one canonical mapping (CC1):
//!
//! - **Ingress** — `verify_sig256` enforces GitHub's `X-Hub-Signature-256`
//!   (`sha256=<hex>` HMAC over the *raw* request body — the bytes on the
//!   wire, never a re-encoded projection) and `event_item` maps an
//!   `issues`/`issue_comment` payload to the `WorkItemInit` the ingress
//!   queues. Every failure is a typed [`IngressError`] — unsigned,
//!   unsupported-scheme and unverifying deliveries fail closed.
//! - **Records** — `issue_record` normalizes a raw GitHub issue object
//!   into the `WorkSourceRecord` `list`/`get` serve; the same mapping the
//!   `hh-github-issues` plugin variant uses (records-in/records-out —
//!   the plugin never reaches the network).
//!
//! Credentials never enter this module: the HMAC key arrives per-call
//! (the broker resolves `key_ref` at the boundary) and the REST token
//! stays in the mediated caller.

use std::collections::BTreeMap;

use hharness_wire::json::Json;
use hharness_wire::sha256::hmac_sha256_hex;

use crate::errors::FleetError;
use crate::ingress::{constant_time_eq, IngressError};

/// The `X-Hub-Signature-256` scheme spelling — `sha256=<hex>`.
pub const GITHUB_SIG_PREFIX: &str = "sha256=";

/// The `X-GitHub-Event` values the mapping admits.
pub const GITHUB_EVENTS: &[&str] = &["issues", "issue_comment"];

/// The `github` member's schema spelling on a fleet source doc.
pub const GITHUB_SOURCE_SCHEMA: &str = "hh.fleet.github_source/1";

/// `GithubConfig` — the `github` member of `hh.fleet.webhook_source/1`
/// (and the `hh.fleet.github_source/1` plugin doc): the repository
/// coordinate + the mapping knobs. Secrets never appear here — the
/// signing secret is `webhook.key_ref` and the REST token stays with the
/// mediated caller.
#[derive(Debug, Clone, PartialEq)]
pub struct GithubConfig {
    /// `owner/repo` — the bound repository coordinate (the only target
    /// the live legs are authorized to touch: the private sandbox).
    pub repo: String,
    /// The work-source coordinate deliveries bind to (the doc's
    /// `webhook.source_id` namespaces the occurrence keys).
    pub source_id: String,
    /// The `external{kind}` a delivery triggers (`github.issue` default).
    pub external_kind: String,
    /// GitHub login → fleet `agents[]` principal (OQ-462's mapping slot).
    pub owner_map: BTreeMap<String, String>,
    /// Fallback principal when the assignee is absent/unmapped.
    pub owner_default: Option<String>,
    /// `repo.archived` — an archived repository suspends its source
    /// (RC-4's source-suspension leg for the GitHub dialect).
    pub repo_archived: bool,
}

impl GithubConfig {
    /// Strict decode — unknown members refuse (one doc shape; the same
    /// record the plugin's bind parameter carries).
    pub fn from_json(j: &Json) -> Result<GithubConfig, FleetError> {
        let Json::Obj(o) = j else {
            return Err(FleetError::SchemaViolation {
                detail: "github config must be an object".into(),
            });
        };
        const KNOWN: &[&str] = &[
            "repo",
            "source_id",
            "external_kind",
            "owner_map",
            "owner_default",
            "repo_archived",
        ];
        for k in o.keys() {
            if !KNOWN.contains(&k.as_str()) {
                return Err(FleetError::SchemaViolation {
                    detail: format!("github config unknown member {k}"),
                });
            }
        }
        let s = |k: &str| -> Result<String, FleetError> {
            o.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| FleetError::SchemaViolation {
                    detail: format!("github config {k} required"),
                })
        };
        let owner_map = match o.get("owner_map") {
            None | Some(Json::Null) => BTreeMap::new(),
            Some(Json::Obj(m)) => {
                let mut out = BTreeMap::new();
                for (login, principal) in m {
                    let Some(p) = principal.as_str() else {
                        return Err(FleetError::SchemaViolation {
                            detail: "github owner_map values must be strings".into(),
                        });
                    };
                    out.insert(login.clone(), p.to_string());
                }
                out
            }
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "github owner_map must be an object".into(),
                })
            }
        };
        let repo_archived = match o.get("repo_archived") {
            None | Some(Json::Null) => false,
            Some(Json::Bool(b)) => *b,
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "github repo_archived must be a bool".into(),
                })
            }
        };
        Ok(GithubConfig {
            repo: s("repo")?,
            source_id: s("source_id")?,
            external_kind: o
                .get("external_kind")
                .and_then(Json::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| "github.issue".to_string()),
            owner_map,
            owner_default: o
                .get("owner_default")
                .and_then(Json::as_str)
                .map(str::to_string),
            repo_archived,
        })
    }
}

/// `gh-<repo-slug>-<number>` — the fleet-scoped item id a GitHub issue
/// maps to (repo namespaced so two bound repositories never collide).
pub fn github_item_id(repo: &str, number: i64) -> String {
    format!("gh-{}-{}", repo.replace('/', "-"), number)
}

/// `verify_sig256(raw_body, signature, key)` — the `X-Hub-Signature-256`
/// check. `signature` is the header value verbatim; anything but
/// `sha256=<hex>` is `UnsupportedAlg` (an absent header reaches the
/// boundary as an empty/absent member — the closed fail set applies the
/// same way). The compare never short-circuits on a prefix.
pub fn verify_sig256(raw_body: &str, signature: &str, key: &[u8]) -> Result<(), IngressError> {
    let Some(hex_sig) = signature.strip_prefix(GITHUB_SIG_PREFIX) else {
        return Err(IngressError::UnsupportedAlg {
            alg: signature
                .split('=')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("unsigned")
                .to_string(),
        });
    };
    let expected = hmac_sha256_hex(key, raw_body.as_bytes());
    if !constant_time_eq(expected.as_bytes(), hex_sig.as_bytes()) {
        return Err(IngressError::BadSignature);
    }
    Ok(())
}

/// `event_item(event, payload, cfg)` — map a GitHub `issues` /
/// `issue_comment` payload to the `WorkItemInit` document the ingress
/// queues (the item's `idempotency_key` is a placeholder — the ingress
/// stamps the volatile-stripped content hash; `origin`/`source_ref`/
/// `authority` on `source` likewise). `ping` and every other event are a
/// typed refusal — the listener answers `ping` locally, never forwards.
pub fn event_item(event: &str, payload: &Json, cfg: &GithubConfig) -> Result<Json, IngressError> {
    if !GITHUB_EVENTS.contains(&event) {
        return Err(IngressError::MalformedPayload {
            detail: format!("github event {event} carries no issue"),
        });
    }
    let issue = payload
        .get("issue")
        .ok_or_else(|| IngressError::MalformedPayload {
            detail: "github payload requires the issue member".into(),
        })?;
    let Json::Obj(io) = issue else {
        return Err(IngressError::MalformedPayload {
            detail: "github issue must be an object".into(),
        });
    };
    let number =
        io.get("number")
            .and_then(Json::as_int)
            .ok_or_else(|| IngressError::MalformedPayload {
                detail: "github issue.number required".into(),
            })?;
    let title = io
        .get("title")
        .and_then(Json::as_str)
        .unwrap_or("(untitled)")
        .to_string();
    let state = io
        .get("state")
        .and_then(Json::as_str)
        .unwrap_or("open")
        .to_string();
    let url = io
        .get("html_url")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();
    // `assignee` (singular — GitHub's primary) maps through `owner_map`;
    // an unmapped/absent login falls to `owner_default`, else owner-less.
    let owner = io
        .get("assignee")
        .and_then(|a| a.get("login"))
        .and_then(Json::as_str)
        .and_then(|login| cfg.owner_map.get(login).cloned())
        .or_else(|| cfg.owner_default.clone());
    Ok(Json::obj([
        ("item_id", Json::str(github_item_id(&cfg.repo, number))),
        ("title", Json::str(title)),
        (
            "source",
            Json::obj([
                ("source_id", Json::str(&cfg.source_id)),
                ("kind", Json::str("github.issue")),
                ("native_id", Json::str(number.to_string())),
                ("state", Json::str(state)),
                ("url", Json::str(url)),
            ]),
        ),
        ("idempotency_key", Json::str("")),
        ("owner", owner.map(Json::str).unwrap_or(Json::Null)),
    ]))
}

/// `issue_record(issue)` — a raw GitHub issue object normalized to the
/// `WorkSourceRecord` `list`/`get` serve (`native_id` is the issue
/// number spelled as a string — the adapter's identity). Volatile
/// members (`updated_at`) live here, never on the item — the idempotency
/// content hash must not move on a redelivery.
pub fn issue_record(issue: &Json) -> Json {
    let number = issue.get("number").and_then(Json::as_int).unwrap_or(0);
    let state = issue.get("state").and_then(Json::as_str).unwrap_or("open");
    let title = issue
        .get("title")
        .and_then(Json::as_str)
        .unwrap_or("(untitled)");
    let url = issue
        .get("html_url")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let labels = issue
        .get("labels")
        .and_then(|l| match l {
            Json::Arr(a) => Some(a.clone()),
            _ => None,
        })
        .unwrap_or_default()
        .iter()
        .filter_map(|l| {
            l.get("name")
                .and_then(Json::as_str)
                .map(Json::str)
                .or_else(|| l.as_str().map(Json::str))
        })
        .collect::<Vec<Json>>();
    let assignees = issue
        .get("assignees")
        .and_then(|a| match a {
            Json::Arr(a) => Some(a.clone()),
            _ => None,
        })
        .unwrap_or_default()
        .iter()
        .filter_map(|a| a.get("login").and_then(Json::as_str).map(Json::str))
        .collect::<Vec<Json>>();
    Json::obj([
        ("native_id", Json::str(number.to_string())),
        ("state", Json::str(state)),
        ("title", Json::str(title)),
        ("url", Json::str(url)),
        ("labels", Json::Arr(labels)),
        ("assignees", Json::Arr(assignees)),
        (
            "updated_at",
            issue
                .get("updated_at")
                .and_then(Json::as_str)
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
    ])
}
