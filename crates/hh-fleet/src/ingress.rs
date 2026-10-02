//! Signed-webhook ingress — the generic push half of a `WorkSourceAdapter`
//! (§5i.1 #3 `Occurrence` key row / OQ-313; ADR-0205 D5; AC-R-2.12.6-9 /
//! AC-L8-09). A *signed webhook delivery* is a `hh.fleet.webhook/1` record
//! verified out-of-process: `signature = "hmac-sha256:" ∥ hex` over the
//! delivery's canonical form under a key the adapter resolves by `key_ref`
//! (credentials are broker-held — the key never enters a ledger row, an
//! observation payload, or the FleetView).
//!
//! Occurrence keys (OQ-313, verbatim):
//!
//! - push with a delivery id ⇒ `occurrence_key` = the delivery id
//!   *namespaced by `source_ref`* — spelled `push:{source_ref}:{id}`;
//! - push without ⇒ `H(canonical(payload − volatile_fields))` —
//!   spelled `push:{source_ref}:{idp}` where `volatile_fields` is the
//!   adapter's declared set;
//! - poll ⇒ `H(native_id ∥ normalized(state) ∥ updated_at)` —
//!   [`poll_occurrence_id`] is the poll lane's key derivation.
//!
//! The replay window (`max(2 × poll_interval, 24h)`, declared on the
//! policy) bounds the ingress's *process-side* duplicate table — never
//! the authoritative dedup: the durable fold's `observed_occurrence_ids`
//! and `control.wakeup.occurred` are the ledgered dedup, so a duplicate
//! that reaches the engine always lands the audited
//! `skipped{duplicate_occurrence}` row, and a stale re-delivery outside
//! the window is a `spec_hash` no-op (the item's content-hash
//! `idempotency_key` admits `Known`). The ingress's own table only
//! *labels* the delivery (`Outcome::Duplicate`); it never drops it —
//! dropping would erase the audited-skip evidence (CC3).

//!
//! `authority`: a webhook payload is external content — the admitted
//! item's `source` carries `origin: ingress(source_ref)` and
//! `authority: external` (§5i.1 #6; never mints, never endorses).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use hh_wire::json::Json;
use hh_wire::sha256::hmac_sha256_hex;

use crate::capabilities::AdapterCapabilities;
use crate::errors::FleetError;
use crate::identity;
use crate::source::{SourceOccurrence, WorkSourceAdapter};
use crate::work_item::WorkItemInit;

/// The delivery record's schema spelling.
pub const WEBHOOK_SCHEMA: &str = "hh.fleet.webhook/1";

/// The signature scheme's single supported spelling (one scheme — CC1).
/// `hmac-sha256:<hex>` over the payload's canonical string.
pub const SIG_ALG: &str = "hmac-sha256";

/// The default replay window — `max(2 × poll_interval, 24h)` (OQ-313);
/// the fixture's poll interval is its reconciliation cadence, so the
/// floor of 24h applies unless the policy declares larger.
pub const DEFAULT_REPLAY_WINDOW_MS: u64 = 86_400_000;

/// `IngressError` — the typed ingress refusals (AC-9's fail-closed set;
/// never a silent drop).
#[derive(Debug, Clone, PartialEq)]
pub enum IngressError {
    /// The signature scheme is not `hmac-sha256`.
    UnsupportedAlg {
        /// The presented `alg` member.
        alg: String,
    },
    /// The signature did not match `HMAC(key, canonical(payload))`.
    BadSignature,
    /// The payload failed strict decode (schema_version/member/item).
    MalformedPayload {
        /// Where the decode failed.
        detail: String,
    },
}

impl IngressError {
    /// The closed refusal spelling (boundary `reason`).
    pub fn code(&self) -> &'static str {
        match self {
            IngressError::UnsupportedAlg { .. } => "unsupported_alg",
            IngressError::BadSignature => "bad_signature",
            IngressError::MalformedPayload { .. } => "malformed_payload",
        }
    }
}

/// `IngressOutcome` — `receive`'s answer.
#[derive(Debug, Clone, PartialEq)]
pub enum IngressOutcome {
    /// A delivery the window table has not seen — enqueued for
    /// `occurrences()`.
    Received {
        /// The derived OQ-313 occurrence key.
        occurrence_id: String,
    },
    /// A delivery whose occurrence key was already seen inside the
    /// replay window — **still enqueued** (the durable fold emits the
    /// audited `skipped{duplicate_occurrence}`; the label is the
    /// ingress's own accounting, never a drop).
    Duplicate {
        /// The derived OQ-313 occurrence key.
        occurrence_id: String,
    },
}

/// `IngressPolicy` — the declared ingress configuration (MUST-data; the
/// fixture doc's `webhook` member is this record verbatim).
#[derive(Debug, Clone, PartialEq)]
pub struct IngressPolicy {
    /// The work-source coordinate (`source_id`) the deliveries bind to —
    /// `source_ref = H(fleet_run ∥ source_id)` namespaces every derived
    /// occurrence key.
    pub source_id: String,
    /// The key coordinate (`broker`/`vault` ref) — the *name* of the
    /// credential, never the bytes (those arrive per `receive` call).
    pub key_ref: String,
    /// The replay window (ms) — `max(2 × poll_interval, 24h)`.
    pub replay_window_ms: u64,
    /// The adapter's `volatile_fields` declaration for the id-less
    /// content hash.
    pub volatile_fields: Vec<String>,
    /// The `external{kind}` the deliveries trigger.
    pub external_kind: String,
}

impl IngressPolicy {
    /// Strict decode from the `webhook` member.
    pub fn from_json(j: &Json) -> Result<IngressPolicy, FleetError> {
        let Json::Obj(o) = j else {
            return Err(FleetError::SchemaViolation {
                detail: "webhook policy must be an object".into(),
            });
        };
        const KNOWN: &[&str] = &[
            "source_id",
            "key_ref",
            "replay_window_ms",
            "volatile_fields",
            "external_kind",
        ];
        for k in o.keys() {
            if !KNOWN.contains(&k.as_str()) {
                return Err(FleetError::SchemaViolation {
                    detail: format!("webhook policy unknown member {k}"),
                });
            }
        }
        let s = |k: &str| -> Result<String, FleetError> {
            o.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| FleetError::SchemaViolation {
                    detail: format!("webhook policy {k} required"),
                })
        };
        let volatile_fields = match o.get("volatile_fields") {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Arr(a)) => a
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| FleetError::SchemaViolation {
                            detail: "webhook volatile_fields members must be strings".into(),
                        })
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "webhook volatile_fields must be an array".into(),
                })
            }
        };
        Ok(IngressPolicy {
            source_id: s("source_id")?,
            key_ref: s("key_ref")?,
            replay_window_ms: o
                .get("replay_window_ms")
                .and_then(Json::as_int)
                .map(|i| i.max(0) as u64)
                .unwrap_or(DEFAULT_REPLAY_WINDOW_MS),
            volatile_fields,
            external_kind: o
                .get("external_kind")
                .and_then(Json::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| "ticket.updated".to_string()),
        })
    }
}

/// The occurrence key a delivery resolves to (OQ-313 — push lane).
///
/// - `delivery_id` present ⇒ `{source_ref}:{delivery_id}` namespaced;
/// - absent ⇒ `{source_ref}:{H(canonical(payload − volatile_fields))}`.
pub fn push_occurrence_id(
    source_ref: &str,
    delivery_id: Option<&str>,
    payload: &Json,
    volatile_fields: &[String],
) -> String {
    match delivery_id {
        Some(id) => format!("push:{source_ref}:{id}"),
        None => {
            let stripped = strip_volatile(payload, volatile_fields);
            let h =
                hh_identity::idp::idp_id("hh.webhook", stripped.to_canonical_string().as_bytes());
            format!("push:{source_ref}:{h}")
        }
    }
}

/// The poll lane's occurrence key — `H(native_id ∥ normalized(state) ∥
/// updated_at)` namespaced by `source_ref` (OQ-313; `normalized` is
/// trim+lowercase — the state-map rule at key time).
pub fn poll_occurrence_id(
    source_ref: &str,
    native_id: &str,
    state: &str,
    updated_at: &str,
) -> String {
    let normalized = state.trim().to_lowercase();
    format!(
        "poll:{}:{}",
        source_ref,
        hh_identity::idp::idp_id(
            "hh.webhook_poll",
            format!("{native_id}\u{0}{normalized}\u{0}{updated_at}").as_bytes(),
        )
    )
}

/// `payload − volatile_fields` — a deep member-strip (top-level member
/// names only; nested `volatile` members are adapter-declared per the
/// doc shape and stripped at their declared dotted paths — this stage
/// supports flat member names, the declared set's interim form).
fn strip_volatile(j: &Json, fields: &[String]) -> Json {
    match j {
        Json::Obj(o) => {
            let mut m = o.clone();
            for f in fields {
                m.remove(f.as_str());
            }
            for v in m.values_mut() {
                *v = strip_volatile(v, fields);
            }
            Json::Obj(m)
        }
        Json::Arr(a) => Json::Arr(a.iter().map(|v| strip_volatile(v, fields)).collect()),
        other => other.clone(),
    }
}

/// `SignedWebhookIngress` — the verifier + keyer + window table. Pure
/// process machinery: every delivery it accepts is *enqueued* for the
/// engine to ledger; nothing here is authoritative (RC-8).
pub struct SignedWebhookIngress {
    /// The declared policy.
    pub policy: IngressPolicy,
    /// The fleet-scoped `source_ref` the keys namespace under.
    pub source_ref: String,
    /// `occurrence_key → first-seen ms` — the window table (process-side
    /// only; the ledger's `observed_occurrence_ids` is authoritative).
    seen: BTreeMap<String, u64>,
    /// The accepted deliveries `occurrences()` drains.
    queue: VecDeque<SourceOccurrence>,
    /// Whether a delivery carrying `delivery_id` has been seen (the
    /// `push_delivery_id` probe's runtime evidence).
    pub saw_delivery_id: bool,
}

impl SignedWebhookIngress {
    /// `new(policy, fleet_run)` — derives the namespacing `source_ref`.
    pub fn new(policy: IngressPolicy, fleet_run: &str) -> SignedWebhookIngress {
        let source_ref = identity::source_ref(fleet_run, &policy.source_id);
        SignedWebhookIngress {
            policy,
            source_ref,
            seen: BTreeMap::new(),
            queue: VecDeque::new(),
            saw_delivery_id: false,
        }
    }

    /// `receive(delivery, signature, key, now_ms)` — verify, key, enqueue.
    /// `key` is the broker-resolved secret — used for verification only;
    /// it is never copied into a record.
    pub fn receive(
        &mut self,
        delivery: &Json,
        signature: &str,
        key: &[u8],
        now_ms: u64,
    ) -> Result<IngressOutcome, IngressError> {
        // 1. Signature — `hmac-sha256:<hex>` over canonical(payload).
        let Some(hex_sig) = signature.strip_prefix(&format!("{SIG_ALG}:")) else {
            return Err(IngressError::UnsupportedAlg {
                alg: signature.split(':').next().unwrap_or(signature).to_string(),
            });
        };
        let expected = hmac_sha256_hex(key, delivery.to_canonical_string().as_bytes());
        if !constant_time_eq(expected.as_bytes(), hex_sig.as_bytes()) {
            return Err(IngressError::BadSignature);
        }
        // 2. Strict decode — `hh.fleet.webhook/1{schema_version,
        //    delivery_id?, item, occurred_at_ms?}`.
        let Json::Obj(o) = delivery else {
            return Err(IngressError::MalformedPayload {
                detail: "delivery must be an object".into(),
            });
        };
        for k in o.keys() {
            if !matches!(
                k.as_str(),
                "schema_version" | "delivery_id" | "item" | "occurred_at_ms"
            ) {
                return Err(IngressError::MalformedPayload {
                    detail: format!("delivery unknown member {k}"),
                });
            }
        }
        match o.get("schema_version").and_then(Json::as_str) {
            Some(WEBHOOK_SCHEMA) => {}
            Some(other) => {
                return Err(IngressError::MalformedPayload {
                    detail: format!("schema_version {other}"),
                })
            }
            None => {
                return Err(IngressError::MalformedPayload {
                    detail: "schema_version required".into(),
                })
            }
        }
        let item_json = o
            .get("item")
            .ok_or_else(|| IngressError::MalformedPayload {
                detail: "item required".into(),
            })?;
        let mut init =
            WorkItemInit::from_json(item_json).map_err(|e| IngressError::MalformedPayload {
                detail: format!("item: {e:?}"),
            })?;
        let delivery_id = o
            .get("delivery_id")
            .and_then(Json::as_str)
            .map(str::to_string);
        if delivery_id.is_some() {
            self.saw_delivery_id = true;
        }
        // 3. The OQ-313 key — delivery-id namespaced, else the volatile-
        //    stripped content hash.
        let occurrence_id = push_occurrence_id(
            &self.source_ref,
            delivery_id.as_deref(),
            item_json,
            &self.policy.volatile_fields,
        );
        // The content-hash idempotency key — a stale re-delivery under a
        // *fresh* delivery id but unchanged content still lands `Known`
        // at admit (the spec_hash no-op, OQ-313's outside-window leg).
        let content_hash = hh_identity::idp::idp_id(
            "hh.fleet_idem",
            strip_volatile(item_json, &self.policy.volatile_fields)
                .to_canonical_string()
                .as_bytes(),
        );
        init.idempotency_key = content_hash;
        // The ingress provenance — `origin: ingress(source_ref)`,
        // `authority: external`; the item's `source` names it (§5i.1 #6).
        if let Json::Obj(src) = &mut init.source {
            src.insert("origin".to_string(), Json::str("ingress"));
            src.insert("source_ref".to_string(), Json::str(&self.source_ref));
            src.insert("authority".to_string(), Json::str("external"));
        }
        // 4. The window table — labels only; every accepted delivery is
        //    enqueued (the durable layer audits the duplicate).
        self.seen
            .retain(|_, first| now_ms.saturating_sub(*first) <= self.policy.replay_window_ms);
        let dup = self.seen.contains_key(&occurrence_id);
        if !dup {
            self.seen.insert(occurrence_id.clone(), now_ms);
        }
        let occurred_at = o
            .get("occurred_at_ms")
            .and_then(Json::as_int)
            .map(|i| i.max(0) as u64)
            .unwrap_or(now_ms);
        self.queue.push_back(SourceOccurrence {
            occurrence_id: occurrence_id.clone(),
            trigger_kind: "external".to_string(),
            external_kind: Some(self.policy.external_kind.clone()),
            item: init,
            observed_at_ms: occurred_at,
            actor: "webhook".to_string(),
        });
        Ok(if dup {
            IngressOutcome::Duplicate { occurrence_id }
        } else {
            IngressOutcome::Received { occurrence_id }
        })
    }

    /// Drain the queue — the adapter's `occurrences()` seam.
    pub fn drain(&mut self) -> Vec<SourceOccurrence> {
        self.queue.drain(..).collect()
    }
}

/// Constant-time hex compare (a signature check never short-circuits on
/// a prefix — the timing channel is real even in-process).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// `WebhookAdapter` — a `WorkSourceAdapter` whose occurrence stream is
/// the signed-webhook ingress. `suspended`/`activate_run`/`list`/`get`
/// ride the same pinned doc members the fixture adapter serves
/// (`suspended`, `activate_run`, `records`) — one doc shape, two lanes.
pub struct WebhookAdapter {
    /// The ingress (verifier + queue).
    pub ingress: SignedWebhookIngress,
    /// The declared capability set (`push_delivery_id` starts
    /// `Declared`/`Unknown` by config; a delivered id probes it).
    pub caps: AdapterCapabilities,
    /// The source-suspended set (RC-4).
    pub suspended_set: BTreeSet<String>,
    /// The declared dispatchable set — `None` = absent member, admit
    /// every candidate (the same rule the fixture adapter applies).
    pub runnable: Option<BTreeSet<String>>,
    /// The pinned records `list`/`get` serve (the adapter's read
    /// minimum — normalized `WorkSourceRecord`s).
    pub records: Vec<Json>,
}

impl WebhookAdapter {
    /// `from_doc(doc, fleet_run)` — the pinned document the boundary
    /// carries: `webhook{…}` (the [`IngressPolicy`]) plus the shared
    /// `suspended`/`activate_run`/`records`/`capabilities`/`probe`
    /// members (one doc shape, two lanes).
    pub fn from_doc(doc: &Json, fleet_run: &str) -> Result<WebhookAdapter, FleetError> {
        let Json::Obj(o) = doc else {
            return Err(FleetError::SchemaViolation {
                detail: "webhook adapter doc must be an object".into(),
            });
        };
        const KNOWN: &[&str] = &[
            "schema_version",
            "webhook",
            "suspended",
            "activate_run",
            "records",
            "capabilities",
            "probe",
        ];
        for k in o.keys() {
            if !KNOWN.contains(&k.as_str()) {
                return Err(FleetError::SchemaViolation {
                    detail: format!("webhook adapter doc unknown member {k}"),
                });
            }
        }
        let policy = IngressPolicy::from_json(o.get("webhook").ok_or_else(|| {
            FleetError::SchemaViolation {
                detail: "webhook adapter doc requires the webhook member".into(),
            }
        })?)?;
        let suspended_set = match o.get("suspended") {
            None | Some(Json::Null) => BTreeSet::new(),
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "webhook adapter suspended must be an array".into(),
                })
            }
        };
        let runnable = match o.get("activate_run") {
            None | Some(Json::Null) => None,
            Some(Json::Arr(a)) => Some(
                a.iter()
                    .filter_map(Json::as_str)
                    .map(str::to_string)
                    .collect(),
            ),
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "webhook adapter activate_run must be an array".into(),
                })
            }
        };
        let records = match o.get("records") {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Arr(a)) => a.clone(),
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "webhook adapter records must be an array".into(),
                })
            }
        };
        let mut caps = match o.get("capabilities") {
            None | Some(Json::Null) => AdapterCapabilities {
                // The push lane's own capability starts `declared` — the
                // adapter *is* the signed-webhook path.
                push_delivery_id: crate::capabilities::CapState::Declared,
                ..AdapterCapabilities::all_unknown()
            },
            Some(c) => {
                AdapterCapabilities::from_json(c).map_err(|d| FleetError::SchemaViolation {
                    detail: format!("webhook adapter capabilities: {d}"),
                })?
            }
        };
        if let Some(Json::Obj(p)) = o.get("probe") {
            for (name, v) in p {
                if let Json::Bool(b) = v {
                    caps.probe_with(name, *b);
                }
            }
        }
        Ok(WebhookAdapter {
            ingress: SignedWebhookIngress::new(policy, fleet_run),
            caps,
            suspended_set,
            runnable,
            records,
        })
    }

    /// `receive(delivery, signature, key, now)` — the push entry point.
    /// The capability probe folds here: a delivered `delivery_id` turns
    /// `push_delivery_id` `unknown → probed` (T-LCD-07 — only `unknown`
    /// resolves; `declared` is never downgraded).
    pub fn receive(
        &mut self,
        delivery: &Json,
        signature: &str,
        key: &[u8],
        now_ms: u64,
    ) -> Result<IngressOutcome, IngressError> {
        let out = self.ingress.receive(delivery, signature, key, now_ms)?;
        if self.ingress.saw_delivery_id {
            self.caps.probe_with("push_delivery_id", true);
        }
        Ok(out)
    }
}

impl WorkSourceAdapter for WebhookAdapter {
    fn occurrences(&mut self, since: Option<u64>) -> Vec<SourceOccurrence> {
        self.ingress
            .drain()
            .into_iter()
            .filter(|o| since.map(|s| o.observed_at_ms > s).unwrap_or(true))
            .collect()
    }

    fn suspended(&mut self, source_id: &str) -> bool {
        self.suspended_set.contains(source_id)
    }

    fn activate_run(&mut self, candidates: &[String]) -> Vec<String> {
        match &self.runnable {
            None => candidates.to_vec(),
            Some(set) => candidates
                .iter()
                .filter(|c| set.contains(*c))
                .cloned()
                .collect(),
        }
    }

    fn capabilities(&self) -> AdapterCapabilities {
        self.caps.clone()
    }

    fn list(&mut self, states: &[String]) -> Vec<Json> {
        self.records
            .iter()
            .filter(|r| {
                states.is_empty()
                    || r.get("state")
                        .and_then(Json::as_str)
                        .map(|s| states.iter().any(|w| w == s))
                        .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    fn get(&mut self, native_ids: &[String]) -> Vec<Json> {
        self.records
            .iter()
            .filter(|r| {
                r.get("native_id")
                    .and_then(Json::as_str)
                    .map(|id| native_ids.iter().any(|w| w == id))
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }
}
