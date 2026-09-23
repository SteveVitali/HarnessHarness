//! `EmbedService` — binding (a): the in-process `hh-embed/1` service.
//!
//! `handle` is the *one* dispatch — the stdio binding (b) calls it
//! verbatim, so the two bindings are byte-identical by construction.
//! Session state lives over the ledger `Store` (one fenced writer lease
//! per writer session; attach sessions take no lease and reach no append
//! path — read-only by construction, I6).

use crate::frames::FrameAdapter;
use crate::runtime::{EmbedGate, EmbedModel, KernelAssembler, KernelSink};
use hh_control::driver::{Driver, DriverError};
use hh_control::react::ReactMinimal;
use hh_control::vocab::{Cue, DeliveryMode, WokenTrigger};
use hh_embed_schema::errors::EmbedError;
use hh_embed_schema::frames::StreamNotification;
use hh_embed_schema::negotiate;
use hh_embed_schema::ops::{self, Direction, Tier};
use hh_embed_schema::types::*;
use hh_env::driver::EnvDriver;
use hh_env::events::EventMinter;
use hh_ledger::store::{rfc3339_ms, Lease, Store, Subscription};
use hh_ledger::wakeup::{Trigger as LedgerTrigger, WokenDelivery};
use hh_provenance::ProvenanceRecord;
use hh_registry::store::RegistryStore;
use hh_wire::json::Json;
use hh_wire::jsonrpc::{err_response, ok_response, Request};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// The writer-lease TTL the service acquires under (ms).
pub(crate) const LEASE_TTL_MS: u64 = 120_000;
/// The kernel default in-flight session bound (`max_in_flight_sessions =
/// 0` in `hello` means "kernel default").
pub(crate) const DEFAULT_MAX_SESSIONS: usize = 32;

/// `EmbedService` construction facts.
pub struct ServiceConfig {
    /// The ledger store root (runs + blobs live under it).
    pub store_root: PathBuf,
    /// The kernel version id (`hh-kernel/<semver>`) reported on `hello`.
    pub kernel_version_id: String,
    /// The default workspace root a `local_host` environment binds.
    pub workspace_root: PathBuf,
    /// The lease-holder identity writer leases are taken under.
    pub holder: String,
}

/// The `hh-embed/1` service — binding (a).
pub struct EmbedService {
    pub(crate) kernel_version: String,
    pub(crate) kernel_prov: ProvenanceRecord,
    pub(crate) store: Store,
    pub(crate) registry: RegistryStore,
    pub(crate) catalog: hh_assembly::catalog::Stage1Catalog,
    pub(crate) env_drivers: BTreeMap<String, EnvDriver>,
    pub(crate) hello_done: bool,
    pub(crate) experimental: bool,
    pub(crate) client_caps: HostCapabilities,
    pub(crate) sessions: BTreeMap<String, SessionState>,
    pub(crate) subs: BTreeMap<String, SubState>,
    /// `upcall.*` notifications queued for delivery (`{method, params}`).
    pub(crate) pending_notifications: Vec<Json>,
    pub(crate) open_idem: BTreeMap<String, Json>,
    /// Sealed definitions minted at `open_session{new}` — `version_id →
    /// canonical bytes` (the bytes hash back to the id, so `get_artifact`
    /// can serve them content-addressed; in-memory at Stage 1 — durable
    /// artifact persistence is DF-S1.25-3).
    pub(crate) sealed_defs: BTreeMap<String, Vec<u8>>,
    pub(crate) next: u64,
    pub(crate) workspace_root: PathBuf,
    pub(crate) holder: String,
}

/// A declared host-executor capability (`supplies.host_capabilities[]`).
#[derive(Debug, Clone)]
pub(crate) struct HostCap {
    pub capability_id: String,
    pub surface_id: String,
    pub requires_approval: bool,
    pub options: Vec<String>,
}

/// A live permission ask (`security.permission.pending`).
#[derive(Debug, Clone)]
pub(crate) struct PendingAsk {
    pub options: Vec<String>,
    pub proposal: String,
    pub effect_id: Option<String>,
    /// When the pending was requested (the `wait_ms` member of `decided`).
    pub requested_at: u64,
    /// The pending's capability material — present when the row carries a
    /// `request{capability_ref, args_canonical_hash}` (dispatch-side asks;
    /// restored pendings). `allow_lease` mints the lease row only over this
    /// material — never fabricated.
    pub capability_ref: Option<(String, String)>,
    /// The pending's canonical-args hash (the lease key leg).
    pub args_canonical_hash: Option<String>,
    /// The requesting subject (the lease/handle `holder`).
    pub subject_ref: Option<String>,
    /// The `permission_request` proposal's `requested: [Grant]` leg — the
    /// approval mint's `requested ⊓ authority_cap` input (never fabricated;
    /// empty on an ordinary ask).
    pub requested_grants: Vec<hh_hir::records::Grant>,
}

/// Decode the `request.requested_grants` member of a durable `pending` row —
/// absent/malformed decodes to `[]` (a row the fold can't read confers
/// nothing; the mint never guesses).
pub(crate) fn decode_requested_grants(req: &Json) -> Vec<hh_hir::records::Grant> {
    match req.get("requested_grants") {
        Some(Json::Arr(items)) => items
            .iter()
            .enumerate()
            .map(|(i, g)| hh_hir::grant_from_json(g, &format!("requested_grants[{i}]")).ok())
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// A host-executor ask awaiting `report_host_effect`.
#[derive(Debug, Clone)]
pub(crate) struct HostAsk {
    pub capability_id: String,
    pub attempt_no: u64,
    pub turn_id: String,
    pub model_call_id: String,
}

/// Per-session state (writer or attach).
pub(crate) struct SessionState {
    pub run_id: String,
    pub attach: bool,
    pub lease: Option<Lease>,
    pub manifest_ref: String,
    pub realized: RealizedSettings,
    pub driver: Option<Driver<ReactMinimal>>,
    pub env_json: Json,
    pub env_handle_id: Option<String>,
    pub host_caps: Vec<HostCap>,
    pub turn_active: bool,
    pub active_turn: String,
    pub finished: bool,
    pub detached: Option<String>,
    /// The sealed definition's `authority_cap` rows (ADR-0240) — the
    /// approval mint's `requested ⊓ authority_cap` ceiling leg; computed at
    /// `open` from the sealed document (never re-resolved at respond).
    pub authority_caps: Vec<hh_monitor::mint::AuthorityCap>,
    pub pendings: BTreeMap<String, PendingAsk>,
    pub decided: BTreeMap<String, Json>,
    /// `(subscription_id, occurrence_key)` pairs already submitted as
    /// `Cue.woken` — the caller-side delivery dedup key (`wakeup_drain`
    /// is a pure projection; the durable `fired` row stays deliverable
    /// across a restart, which is exactly the at-least-once-into-the-
    /// inbox semantics AC-7/8 want).
    pub delivered_wokens: BTreeSet<String>,
    pub host_asks: BTreeMap<String, HostAsk>,
    pub idem: BTreeMap<String, Json>,
    /// The budget ceilings armed at `open` and lifted by `amend(budget)`
    /// — the boundary's own record of the driver's `budget_ceiling`
    /// (the driver's copy is private; the amend gate compares old → new).
    pub budget_ceiling: BTreeMap<String, i64>,
    /// The resume-by-leaf arm record (S2.3) — `surfaces` + the arm-time
    /// budget maps the durable-resume path re-reads from `leaf.arm`.
    pub leaf_arm: crate::open::LeafArm,
    pub scan_seq: u64,
    /// The  block staged for the next drive (a
    /// submit input).
    pub next_invoke: Option<(String, Json)>,
    /// The completion text staged for the next drive.
    pub next_completion: String,
    /// The response ref staged for the next model call.
    pub next_response_ref: String,
}

/// A live subscription — the ledger `Subscription` (taken by the stdio
/// drainer when binding (b) owns the delivery) plus the frame adapter
/// state.
pub(crate) struct SubState {
    pub sub: Option<Subscription>,
    pub adapter: FrameAdapter,
    pub head_hash: String,
}

impl EmbedService {
    /// Open the service over a store root — binding (a) entry.
    pub fn open(config: ServiceConfig) -> Result<Self, EmbedError> {
        let store = Store::open(&config.store_root).map_err(|e| EmbedError::Refused {
            reason: format!("store_open: {e:?}"),
        })?;
        Self::build(config, store)
    }

    /// Open over an injected clock / id source — the deterministic seam
    /// (`ManualClock`/`SeqIds`) the AC-R-2.11.1-2 byte-identity fixture
    /// and the golden corpus drive. `None` ids = `TimeIds` (production).
    pub fn open_with(
        config: ServiceConfig,
        clock: Box<dyn hh_ledger::ids::Clock>,
        ids: Option<Box<dyn hh_ledger::ids::IdSource>>,
    ) -> Result<Self, EmbedError> {
        let store = Store::open_with(
            &config.store_root,
            clock,
            ids,
            hh_ledger::DEFAULT_BLOB_MAX_BYTES,
        )
        .map_err(|e| EmbedError::Refused {
            reason: format!("store_open: {e:?}"),
        })?;
        Self::build(config, store)
    }

    /// Shared construction — registry seed + the service state over a
    /// ready `Store`.
    fn build(config: ServiceConfig, store: Store) -> Result<Self, EmbedError> {
        let mut registry = RegistryStore::open(
            config.store_root.join("registry"),
            &ProvenanceRecord::kernel("hh-embed", 0),
        )
        .map_err(|e| EmbedError::Refused {
            reason: format!("registry_open: {e:?}"),
        })?;
        Self::seed_registry(&mut registry);
        let kernel_version = config
            .kernel_version_id
            .rsplit('/')
            .next()
            .unwrap_or("0.0.0")
            .to_string();
        Ok(EmbedService {
            kernel_version,
            kernel_prov: ProvenanceRecord::kernel("hh-embed", 0),
            store,
            registry,
            catalog: hh_assembly::catalog::Stage1Catalog::stage1(),
            env_drivers: BTreeMap::new(),
            hello_done: false,
            experimental: false,
            client_caps: HostCapabilities::default(),
            sessions: BTreeMap::new(),
            subs: BTreeMap::new(),
            pending_notifications: Vec::new(),
            open_idem: BTreeMap::new(),
            sealed_defs: BTreeMap::new(),
            next: 0,
            workspace_root: config.workspace_root,
            holder: config.holder,
        })
    }

    /// Seed the embedded registry with the kernel's Stage-1 suite — the
    /// two hot-path classes (`control_strategy`, `context_policy`) plus
    /// one `Placement::InProcess` variant each, published under `hh/…`.
    /// Idempotent: a store that already carries a variant's version id is
    /// left alone (the check is the deterministic `version_id`, never a
    /// name probe).
    fn seed_registry(registry: &mut RegistryStore) {
        use hh_identity::idp::address;
        use hh_registry::kinds::Placement;
        use hh_registry::records::{AppliesTo, Implementation, RegistryRecord, VariantRecord};
        let prov = ProvenanceRecord::kernel("hh-embed", 0);
        for (class, name) in [
            (hh_registry::suites::control_strategy_class(), "round_robin"),
            (hh_registry::suites::context_policy_class(), "full_window"),
        ] {
            let class_rec = RegistryRecord::Class(class);
            let class_vid = hh_registry::identity::version_id(&class_rec);
            if registry.get(&class_vid).is_none() {
                let _ = registry.register(class_rec, &prov, None);
            }
            let variant = VariantRecord {
                variant_id: format!("hh/{name}"),
                class_ref: class_vid.clone(),
                contract_range: "1.0".to_string(),
                version_label: Some("1.0.0".to_string()),
                param_schema: BTreeMap::new(),
                implementation: Implementation {
                    content: address(
                        format!("impl-hh-{name}").as_bytes(),
                        "application/vnd.hh.variant",
                    ),
                    placement: Placement::InProcess,
                    host_requirements: Json::Null,
                },
                capability_declaration: BTreeMap::from([(
                    "deterministic".to_string(),
                    Json::Bool(true),
                )]),
                conditioned_rules: vec![],
                applies_to: AppliesTo {
                    participant_classes: BTreeSet::from(["native".to_string()]),
                    families: vec![],
                },
                declared_costs: None,
                summary: hh_hir::leaves::Text::new(
                    format!("kernel {name} variant"),
                    "hh-kernel",
                    prov.clone(),
                ),
                dialect_range: "registry/1".to_string(),
            };
            let variant_rec = RegistryRecord::Variant(variant);
            let variant_vid = hh_registry::identity::version_id(&variant_rec);
            if registry.get(&variant_vid).is_none() {
                let _ = registry.register(variant_rec, &prov, None);
            }
            let _ = registry.publish("hh", name, &variant_vid, None, None, &prov);
        }
    }

    /// The one dispatch — `Request → response Json` (binding (a) and (b)
    /// share it; the response is canonical bytes either way).
    pub fn handle(&mut self, req: &Request) -> Json {
        match self.dispatch(req) {
            Ok(result) => ok_response(req.id.clone(), result),
            Err(e) => err_response(req.id.clone(), e.code(), e.kind(), e.to_data_json()),
        }
    }

    /// Notifications queued for delivery — `stream.frame` frames plus
    /// `upcall.*` asks (binding (a) drains them; binding (b) writes them
    /// to the wire).
    pub fn drain_notifications(&mut self) -> Vec<Json> {
        let mut out: Vec<Json> = self.pending_notifications.drain(..).collect();
        let ids: Vec<String> = self.subs.keys().cloned().collect();
        for id in ids {
            out.extend(self.poll_frames(&id));
        }
        out
    }

    /// Queue an `upcall.*` notification (`{jsonrpc, method, params}`).
    pub(crate) fn queue_upcall(&mut self, method: &str, params: Json) {
        self.pending_notifications.push(Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("method", Json::str(method)),
            ("params", params),
        ]));
    }

    /// Poll a subscription's available frames — `stream.frame`
    /// notifications (non-blocking; binding (a) drives it explicitly).
    pub fn poll_frames(&mut self, subscription_id: &str) -> Vec<Json> {
        let head_hash = self
            .subs
            .get(subscription_id)
            .map(|s| s.head_hash.clone())
            .unwrap_or_default();
        let st = match self.subs.get_mut(subscription_id) {
            Some(s) => s,
            None => return Vec::new(),
        };
        let sub = match st.sub.as_mut() {
            Some(s) => s,
            None => return Vec::new(),
        };
        let mut out = Vec::new();
        while let Some(ef) = sub.try_next() {
            let mut frames = st.adapter.map(ef);
            FrameAdapter::fix_sync_head(&mut frames, &head_hash);
            for f in frames {
                out.push(
                    StreamNotification {
                        subscription_id: subscription_id.to_string(),
                        frame: f,
                    }
                    .to_notification(),
                );
            }
            if matches!(
                out.last()
                    .and_then(|j| j.get("params"))
                    .and_then(|p| p.get("frame"))
                    .and_then(|f| f.get("kind"))
                    .and_then(Json::as_str),
                Some("closed")
            ) {
                break;
            }
        }
        out
    }

    /// Hand a subscription to a caller-owned drainer (binding (b)'s
    /// delivery thread) — the subscription leaves the service's table
    /// and the drainer owns frame conversion + the write path.
    pub fn take_subscription(
        &mut self,
        subscription_id: &str,
    ) -> Option<(Subscription, FrameAdapter, String)> {
        let st = self.subs.get_mut(subscription_id)?;
        let sub = st.sub.take()?;
        Some((sub, std::mem::take(&mut st.adapter), st.head_hash.clone()))
    }

    /// The store — read-side access for describe/conformance checks.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The session table — read-side access for conformance checks.
    pub(crate) fn session(&self, id: &str) -> Result<&SessionState, EmbedError> {
        self.sessions.get(id).ok_or(EmbedError::UnknownSession)
    }

    pub(crate) fn session_mut(&mut self, id: &str) -> Result<&mut SessionState, EmbedError> {
        self.sessions.get_mut(id).ok_or(EmbedError::UnknownSession)
    }

    /// A live (non-detached) session — `SessionDetached` rides the
    /// recorded reason when the lease was fenced by a takeover.
    pub(crate) fn live_session(&self, id: &str) -> Result<&SessionState, EmbedError> {
        let s = self.session(id)?;
        if let Some(r) = &s.detached {
            return Err(EmbedError::SessionDetached {
                reason: r.clone(),
                event_ref: None,
            });
        }
        Ok(s)
    }

    /// A live *writer* session — mutation ops refuse attach sessions
    /// (`read-only by construction`, I6).
    pub(crate) fn writer_session(&self, id: &str) -> Result<&SessionState, EmbedError> {
        let s = self.live_session(id)?;
        if s.attach {
            return Err(EmbedError::Refused {
                reason: "session_is_read_only".to_string(),
            });
        }
        Ok(s)
    }

    // ── dispatch ────────────────────────────────────────────────────────

    fn dispatch(&mut self, req: &Request) -> Result<Json, EmbedError> {
        if req.method == "hello" {
            return self.hello(&req.params);
        }
        if !self.hello_done {
            return Err(EmbedError::NotInitialized);
        }
        let op = ops::lookup(&req.method).ok_or_else(|| EmbedError::SchemaViolation {
            path: "/method".to_string(),
            code: "unknown_method".to_string(),
        })?;
        if matches!(op.direction, Direction::Upcall) {
            return Err(EmbedError::SchemaViolation {
                path: "/method".to_string(),
                code: "upcall_direction".to_string(),
            });
        }
        // The experimental stability gate precedes the capability gate —
        // an op the client hasn't opted into is refused before its
        // capability requirement is even consulted.
        if matches!(op.tier, Tier::Experimental) && !self.experimental {
            return Err(EmbedError::ExperimentalRequired {
                reason: "opt_in".to_string(),
            });
        }
        if let Some(cap) = op.requires_capability {
            if !self.client_caps.serves(cap) {
                return Err(EmbedError::CapabilityNotDeclared {
                    capability: cap.to_string(),
                });
            }
        }
        if !op.implemented {
            return Err(EmbedError::Refused {
                reason: "stage_pending".to_string(),
            });
        }
        match req.method.as_str() {
            "open_session" => self.open_session(&req.params),
            "close" => self.close(&req.params),
            "submit" => self.submit(&req.params),
            "cancel" => self.cancel(&req.params),
            "steer" => self.steer(&req.params),
            "respond_permission" => self.respond_permission(&req.params),
            "fork" => self.fork(&req.params),
            "amend" => self.amend(&req.params),
            "report_host_effect" => self.report_host_effect(&req.params),
            "respond_elicitation" => self.respond_elicitation(&req.params),
            "stream_events" => self.stream_events(&req.params),
            "read" => self.read_op(&req.params),
            "head" => self.head_op(&req.params),
            "project" => self.project(&req.params),
            "account" => self.account(&req.params),
            "describe" => self.describe(&req.params),
            "list_leases" => self.list_leases(&req.params),
            "lineage" => self.lineage(&req.params),
            "get_artifact" => self.get_artifact(&req.params),
            "audit_view" => self.audit_view(&req.params),
            "verify" => self.verify(&req.params),
            "prove_inclusion" => self.prove_inclusion(&req.params),
            "prove_consistency" => self.prove_consistency(&req.params),
            _ => Err(EmbedError::SchemaViolation {
                path: "/method".to_string(),
                code: "unknown_method".to_string(),
            }),
        }
    }

    // ── hello ───────────────────────────────────────────────────────────

    fn hello(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = HelloParams::from_json(params)?;
        let result = negotiate(&p, &self.kernel_version)?;
        self.hello_done = true;
        self.experimental = result.negotiated.experimental;
        self.client_caps = result.negotiated.clone();
        Ok(result.to_json())
    }

    // ── shared helpers ──────────────────────────────────────────────────

    pub(crate) fn alloc(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}-{}", self.next)
    }

    /// Mint one kernel-provenance event and append it under the
    /// session's writer lease (the only append path the boundary has).
    pub(crate) fn mint(
        &mut self,
        run_id: &str,
        lease: &Lease,
        class: &str,
        payload: Json,
    ) -> Result<(), EmbedError> {
        let ev = EventMinter::new(&self.store, run_id)
            .mint(class, payload)
            .map_err(ledger_err)?;
        self.store
            .append(run_id, lease, vec![ev])
            .map_err(ledger_err)?;
        Ok(())
    }

    /// Mint an effect-scoped event (the terminal row for a host-reported
    /// effect). The argument list is the scope chain verbatim.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mint_effect_scoped(
        &mut self,
        run_id: &str,
        lease: &Lease,
        class: &str,
        payload: Json,
        effect_id: &str,
        turn_id: &str,
        model_call_id: &str,
    ) -> Result<(), EmbedError> {
        let chain = hh_env::events::ScopeChain {
            turn_id: turn_id.to_string(),
            model_call_id: model_call_id.to_string(),
            tool_call_id: String::new(),
        };
        let ev = EventMinter::new(&self.store, run_id)
            .mint_effect(class, payload, effect_id, &chain)
            .map_err(ledger_err)?;
        self.store
            .append(run_id, lease, vec![ev])
            .map_err(ledger_err)?;
        Ok(())
    }

    /// Mint a permission ask: durable `security.permission.pending` +
    /// ephemeral `security.permission.requested` + the
    /// `upcall.request_permission` notification when the host serves the
    /// permission channel. `capability` carries the ask's
    /// `request{subject_ref, capability_ref, args_canonical_hash}` material —
    /// the legs an `allow_lease` response mints the lease over (never
    /// fabricated — a capability-level ask records the declared capability
    /// coordinate and the empty args hash).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mint_permission_ask(
        &mut self,
        sess_id: &str,
        proposal: &str,
        effect_id: Option<String>,
        options: Vec<String>,
        rendering: Json,
        capability: Option<&HostCap>,
    ) -> Result<String, EmbedError> {
        let permission_id = self.alloc("perm");
        let holder = self.holder.clone();
        let (run_id, lease) = {
            let s = self.session(sess_id)?;
            (
                s.run_id.clone(),
                s.lease.clone().ok_or(EmbedError::Refused {
                    reason: "session_is_read_only".to_string(),
                })?,
            )
        };
        let request = capability
            .map(|c| {
                Json::obj([
                    ("subject_ref", Json::str(holder.clone())),
                    (
                        "capability_ref",
                        Json::obj([
                            ("semantic_id", Json::str(c.capability_id.clone())),
                            ("version_id", Json::str(c.capability_id.clone())),
                        ]),
                    ),
                    ("args_canonical_hash", Json::str("")),
                    ("reason", Json::str(proposal.to_string())),
                ])
            })
            .unwrap_or(Json::Null);
        let now = self.store.now_ms();
        self.mint(
            &run_id,
            &lease,
            "security.permission.pending",
            Json::obj([
                ("permission_id", Json::str(permission_id.clone())),
                (
                    "effect_ids",
                    Json::Arr(effect_id.iter().map(|e| Json::str(e.clone())).collect()),
                ),
                ("request", request),
                ("requested_at", Json::Int(now as i64)),
                ("mode", Json::str("sync")),
            ]),
        )?;
        self.mint(
            &run_id,
            &lease,
            "security.permission.requested",
            Json::obj([
                ("permission_id", Json::str(permission_id.clone())),
                ("proposal", Json::str(proposal.to_string())),
                ("rendering", rendering.clone()),
            ]),
        )?;
        self.session_mut(sess_id)?.pendings.insert(
            permission_id.clone(),
            PendingAsk {
                options: options.clone(),
                proposal: proposal.to_string(),
                effect_id: effect_id.clone(),
                requested_at: now,
                capability_ref: capability
                    .map(|c| (c.capability_id.clone(), c.capability_id.clone())),
                args_canonical_hash: capability.map(|_| String::new()),
                subject_ref: capability.map(|_| holder.clone()),
                requested_grants: Vec::new(),
            },
        );
        if self.client_caps.serves_permission_channel {
            self.queue_upcall(
                "upcall.request_permission",
                RequestPermission {
                    permission_id: permission_id.clone(),
                    proposal: proposal.to_string(),
                    options: options
                        .iter()
                        .map(|o| PermissionOption {
                            option_id: o.clone(),
                            label: o.clone(),
                        })
                        .collect(),
                    effect_id,
                    rendering: Some(rendering),
                }
                .to_json(),
            );
        }
        Ok(permission_id)
    }

    /// Drive the session's control loop until it parks or finishes, then
    /// scan the newly durable prefix (host asks, turn/flags).
    pub(crate) fn drive(&mut self, sess_id: &str) -> Result<(), EmbedError> {
        // Take the driver + ports out of the session for the borrow split.
        let (run_id, lease) = {
            let s = self.session(sess_id)?;
            (
                s.run_id.clone(),
                s.lease.clone().ok_or(EmbedError::Refused {
                    reason: "session_is_read_only".to_string(),
                })?,
            )
        };
        let mut driver =
            self.session_mut(sess_id)?
                .driver
                .take()
                .ok_or_else(|| EmbedError::Refused {
                    reason: "no_driver".to_string(),
                })?;
        let host_surfaces: BTreeSet<String> = self
            .session(sess_id)?
            .host_caps
            .iter()
            .map(|c| c.surface_id.clone())
            .collect();
        let invoke = self.session(sess_id)?.next_invoke.clone();
        let completion = self.session(sess_id)?.next_completion.clone();
        let response_ref = self.session(sess_id)?.next_response_ref.clone();
        // S2.3 / AC-8 — drain durable wakeups at the decision point. The
        // kernel-internal trigger pass materialises due occurrences and
        // fires them under the W-1 claim (crash-safe: an `occurred` row
        // without `fired` redelivers); `wakeup_drain` then withholds a
        // `follow_up` delivery while its `deliver_after` effect is open
        // (W-3). Each undelivered `(subscription, occurrence)` pair becomes
        // one `Cue.woken` — the loop's only input (I7); while effects are
        // open, react parks the cue on `effects_settled` (I4).
        let now = self.store.now_ms();
        self.store
            .deliver_wakeup(&run_id, &lease, now)
            .map_err(|e| EmbedError::Refused {
                reason: format!("wakeup_deliver: {e:?}"),
            })?;
        let drained = self
            .store
            .wakeup_drain(&run_id)
            .map_err(|e| EmbedError::Refused {
                reason: format!("wakeup_drain: {e:?}"),
            })?;
        for w in drained {
            let key = format!("{}\u{0}{}", w.subscription_id, w.occurrence_key);
            if self.session(sess_id)?.delivered_wokens.contains(&key) {
                continue;
            }
            driver.submit(woken_cue(&w));
            self.session_mut(sess_id)?.delivered_wokens.insert(key);
        }
        let mut sink = KernelSink {
            store: &mut self.store,
            run_id: run_id.clone(),
            lease: lease.clone(),
        };
        let mut model = EmbedModel {
            invoke,
            completion,
            response_ref,
            calls_made: 0,
        };
        let mut gate = EmbedGate {
            host_surfaces,
            host_asks: Vec::new(),
        };
        let mut asm = KernelAssembler;
        let outcome = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
        let asks = std::mem::take(&mut gate.host_asks);
        let s = self.session_mut(sess_id)?;
        s.driver = Some(driver);
        s.next_invoke = None;
        match outcome {
            Ok(_r) => {
                s.turn_active = false;
                s.finished = true;
            }
            Err(DriverError::Port { .. }) => {
                // Parked — the inbox emptied mid-turn (a wait/escalate);
                // the next cue resumes the drive.
                s.turn_active = true;
            }
            Err(DriverError::Append(e)) => {
                if e.contains("RunFinished") || e.contains("run_finished") {
                    s.finished = true;
                    s.turn_active = false;
                } else if e.contains("Fenced") {
                    s.detached = Some("fenced".to_string());
                    return Err(EmbedError::SessionDetached {
                        reason: "fenced".to_string(),
                        event_ref: None,
                    });
                } else {
                    return Err(EmbedError::Refused {
                        reason: format!("driver_append: {e}"),
                    });
                }
            }
            Err(e) => {
                return Err(EmbedError::Refused {
                    reason: format!("driver: {e:?}"),
                })
            }
        }
        // Persist the resume-by-leaf pair (S2.3; DF-S1.25-2) — the leaf
        // checkpoint + arm record under `runs/<run_id>/` so a
        // cross-process resume restores the leaf where the writer left
        // it (atomic tmp+rename — a torn write reads as `unavailable`,
        // never as a half-checkpoint).
        self.persist_leaf(sess_id);
        self.post_drive_scan(sess_id, asks);
        Ok(())
    }

    /// Fold the newly durable prefix — record host-executor asks and
    /// emit their `upcall.invoke_host_capability` notifications, learn
    /// the active turn, mark the run finished.
    pub(crate) fn post_drive_scan(&mut self, sess_id: &str, asks: Vec<(String, u64, Json)>) {
        let (run_id, caps_served) = {
            let s = match self.sessions.get(sess_id) {
                Some(s) => s,
                None => return,
            };
            (s.run_id.clone(), self.client_caps.serves_host_executor)
        };
        // Clone the scan window out of the store borrow — the loop
        // mutates `self` (session tables + upcall queue) below.
        type ScanRow = (
            u64,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<Json>,
        );
        let events: Vec<ScanRow> = match self.store.events(&run_id) {
            Ok(evs) => evs
                .iter()
                .map(|e| {
                    (
                        e.seq,
                        e.class.clone(),
                        e.hash.clone(),
                        e.scope.effect_id.clone(),
                        e.scope.model_call_id.clone(),
                        e.scope.turn_id.clone(),
                        // The permission rows ride the payload — the scan
                        // folds them into the owed-decision table so
                        // `respond_permission` stays answerable for asks the
                        // kernel minted mid-drive (a `permission_request`
                        // effect, a batch attach).
                        if e.class.starts_with("security.permission.") {
                            Some(e.payload.clone())
                        } else {
                            None
                        },
                    )
                })
                .collect(),
            Err(_) => return,
        };
        let watermark = self.sessions[sess_id].scan_seq;
        let mut asks_by_effect: BTreeMap<String, (u64, Json)> = BTreeMap::new();
        for (ef, attempt, intent) in asks {
            asks_by_effect.insert(ef, (attempt, intent));
        }
        for (_seq, class, _hash, effect_id, model_call, turn_id, payload) in
            events.iter().filter(|(seq, ..)| *seq > watermark)
        {
            let class = class.as_str();
            match class {
                "lifecycle.run.finished" => {
                    if let Some(s) = self.sessions.get_mut(sess_id) {
                        s.finished = true;
                        s.turn_active = false;
                    }
                }
                "action.effect.intended" => {
                    let ef = effect_id.clone().unwrap_or_default();
                    if let Some((attempt, intent)) = asks_by_effect.remove(&ef) {
                        let surface = intent
                            .get("surface_id")
                            .or_else(|| intent.get("surface"))
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string();
                        let cap_id = self
                            .sessions
                            .get(sess_id)
                            .and_then(|s| {
                                s.host_caps
                                    .iter()
                                    .find(|c| c.surface_id == surface)
                                    .map(|c| c.capability_id.clone())
                            })
                            .unwrap_or_else(|| surface.clone());
                        let mc = model_call.clone().unwrap_or_default();
                        let turn = turn_id.clone().unwrap_or_else(|| "turn-1".into());
                        if let Some(s) = self.sessions.get_mut(sess_id) {
                            s.host_asks.insert(
                                ef.clone(),
                                HostAsk {
                                    capability_id: cap_id.clone(),
                                    attempt_no: attempt,
                                    turn_id: turn,
                                    model_call_id: mc,
                                },
                            );
                        }
                        if caps_served {
                            self.queue_upcall(
                                "upcall.invoke_host_capability",
                                InvokeHostCapability {
                                    capability_id: cap_id,
                                    args: intent.get("args").cloned().unwrap_or(Json::Null),
                                    effect_id: ef,
                                    attempt_no: attempt as i64,
                                }
                                .to_json(),
                            );
                        }
                    }
                }
                "security.permission.pending" => {
                    // A kernel-minted owed-decision row (a `permission_request`
                    // effect's ask, a coalesced attach) — the surface table
                    // learns it so `respond_permission` answers it.
                    if let (Some(s), Some(p)) = (self.sessions.get_mut(sess_id), payload) {
                        if let Some(pid) = p.get("permission_id").and_then(Json::as_str) {
                            let pid = pid.to_string();
                            if !s.pendings.contains_key(&pid) && !s.decided.contains_key(&pid) {
                                let req = p.get("request").cloned().unwrap_or(Json::Null);
                                s.pendings.insert(
                                    pid,
                                    PendingAsk {
                                        options: vec![
                                            "allow_once".to_string(),
                                            "allow_lease".to_string(),
                                            "deny".to_string(),
                                            "more_info".to_string(),
                                        ],
                                        proposal: req
                                            .get("reason")
                                            .and_then(Json::as_str)
                                            .unwrap_or("permission request")
                                            .to_string(),
                                        effect_id: p
                                            .get("effect_id")
                                            .and_then(Json::as_str)
                                            .map(str::to_string),
                                        requested_at: p
                                            .get("requested_at")
                                            .and_then(Json::as_int)
                                            .map(|n| n.max(0) as u64)
                                            .unwrap_or(0),
                                        capability_ref: req.get("capability_ref").and_then(|c| {
                                            let s = c.get("semantic_id")?.as_str()?;
                                            let v = c.get("version_id")?.as_str()?;
                                            Some((s.to_string(), v.to_string()))
                                        }),
                                        args_canonical_hash: req
                                            .get("args_canonical_hash")
                                            .and_then(Json::as_str)
                                            .map(str::to_string),
                                        subject_ref: req
                                            .get("subject_ref")
                                            .and_then(Json::as_str)
                                            .map(str::to_string),
                                        requested_grants: decode_requested_grants(&req),
                                    },
                                );
                            }
                        }
                    }
                }
                "security.permission.decided" => {
                    // A final decision (never the non-final `ask` verdict)
                    // resolves the surface pending — e.g. a `permission_decided`
                    // wakeup's row or a co-writer's respond.
                    if let (Some(s), Some(p)) = (self.sessions.get_mut(sess_id), payload) {
                        let final_row = matches!(
                            p.get("decision").and_then(Json::as_str),
                            Some(d) if d != "ask"
                        );
                        if final_row {
                            if let Some(pid) = p.get("permission_id").and_then(Json::as_str) {
                                s.pendings.remove(pid);
                                s.decided.insert(pid.to_string(), p.clone());
                            }
                        }
                    }
                }
                "model.call.requested" => {
                    if let Some(s) = self.sessions.get_mut(sess_id) {
                        if let Some(t) = turn_id {
                            s.active_turn = t.clone();
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(s) = self.sessions.get_mut(sess_id) {
            if let Some(h) = events.last() {
                s.scan_seq = h.0;
            }
        }
    }

    /// Build the run summary ref `{run_id, seq, hash}` at the current head.
    pub(crate) fn summary_ref(&self, run_id: &str) -> RunSummaryRef {
        match self.store.head(run_id) {
            Ok(h) => RunSummaryRef {
                run_id: run_id.to_string(),
                seq: h.seq as i64,
                hash: h.hash,
            },
            Err(_) => RunSummaryRef {
                run_id: run_id.to_string(),
                seq: -1,
                hash: String::new(),
            },
        }
    }
}

/// `WokenDelivery → Cue.woken` — the ledger dialect's trigger/mode
/// rendered into the control dialect (ADR-0131 §1 closed sum; `Timer`
/// carries its instant as RFC-3339; the caller-side dedup key stays the
/// `(subscription, occurrence)` pair, never the cue itself).
fn woken_cue(w: &WokenDelivery) -> Cue {
    let trigger = match &w.trigger {
        LedgerTrigger::Timer { at_ms } => WokenTrigger::Timer {
            at: rfc3339_ms(*at_ms),
        },
        LedgerTrigger::Schedule { expr } => WokenTrigger::Schedule {
            expression: expr.clone(),
            timezone: String::new(),
            kind: "cron".to_string(),
        },
        LedgerTrigger::PermissionDecided { permission_id } => WokenTrigger::PermissionDecided {
            permission_id: permission_id.clone(),
        },
        LedgerTrigger::ChildTerminal { child_run_id } => WokenTrigger::ChildTerminal {
            child_run_id: child_run_id.clone(),
        },
        LedgerTrigger::EffectTerminal { effect_id } => WokenTrigger::EffectTerminal {
            effect_id: effect_id.clone(),
        },
        LedgerTrigger::EnvironmentReady { env_handle_id } => WokenTrigger::EnvironmentReady {
            handle: env_handle_id.clone(),
        },
        LedgerTrigger::RetryDue { scope_id } => WokenTrigger::RetryDue {
            scope_id: scope_id.clone(),
        },
        LedgerTrigger::External { kind } => WokenTrigger::External {
            source_ref: kind.clone(),
            filter: String::new(),
        },
        LedgerTrigger::Manual { principal } => WokenTrigger::Manual {
            principal: principal.clone(),
        },
        LedgerTrigger::PeerMessage { from } => WokenTrigger::PeerMessage { from: from.clone() },
    };
    Cue::Woken {
        trigger,
        payload_ref: w.payload_ref.clone().unwrap_or_default(),
        delivery_mode: match w.delivery_mode {
            hh_ledger::DeliveryMode::Steer => DeliveryMode::Steer,
            hh_ledger::DeliveryMode::FollowUp => DeliveryMode::FollowUp,
        },
    }
}

/// Map a `LedgerError` to the contract's typed surface — the ledger's
/// words, never a stringy catch-all (I-H7: `Fenced` on a writer
/// surfaces only as `SessionDetached`; `RunFinished` as `Draining`).
pub(crate) fn ledger_err(e: hh_ledger::errors::LedgerError) -> EmbedError {
    use hh_ledger::errors::LedgerError as L;
    match e {
        L::UnknownRun { run_id } => EmbedError::UnknownRun { run_id },
        L::WouldBlock { active_holder } => EmbedError::WouldBlock { active_holder },
        L::Fenced { detail, .. } => EmbedError::SessionDetached {
            reason: "fenced".to_string(),
            event_ref: Some(detail),
        },
        L::RunFinished { .. } => EmbedError::Draining,
        L::SchemaViolation { detail } => EmbedError::SchemaViolation {
            path: "/ledger".to_string(),
            code: detail,
        },
        other => EmbedError::Refused {
            reason: format!("ledger: {other:?}"),
        },
    }
}
