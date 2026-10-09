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
use hh_control::strategy::{ConcurrentInput, ControlStrategy, SteerMode};
use hh_control::vocab::{Cue, DeliveryMode, HumanInput, WokenTrigger};
use hh_embed_schema::errors::EmbedError;
use hh_embed_schema::frames::StreamNotification;
use hh_embed_schema::negotiate;
use hh_embed_schema::ops::{self, Direction, Tier};
use hh_embed_schema::types::*;
use hh_env::driver::EnvDriver;
use hh_env::events::EventMinter;
use hh_ledger::manifest::{RunKind, RunManifest};
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
    /// The kernel-held credential broker custody (R2.9a; DF-S2.4-1c) —
    /// owns the binding table a fork's `rebind_credentials_for_fork`
    /// virtualizes from and the egress sentinel seam fronts. Fail-closed
    /// by default (`DenyAllResolver`) — a composed host installs its
    /// resolver through `set_credential_resolver`; a sentinel never
    /// resolves against an absent source.
    pub(crate) credential_broker: hh_secrets::CredentialBroker,
    /// `audience → CredentialBinding id` for caller-auth tokens minted
    /// by `surface_mint_caller_token` (R2.19; DF-S4.11-2 — the kernel's
    /// authorization-server arm: one `bound` row per audience, one
    /// `used` row per mint).
    pub(crate) caller_auth_bindings: BTreeMap<String, String>,
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
    /// The kernel-internal ledger run carrying `lifecycle.registry.*` rows for
    /// Group L ops (`registry_ops::ensure_registry_run` — created lazily).
    pub(crate) registry_run: Option<(String, Lease)>,
    /// The kernel-internal ledger run carrying
    /// `lifecycle.contract.deprecated_use` rows a sessionless or read-only
    /// caller's deprecated-op use lands on (`ensure_contract_run` — lazily
    /// created like `registry_run`; the session-scoped path mints into the
    /// caller's own run under its writer lease instead).
    pub(crate) contract_run: Option<(String, Lease)>,
    /// The negotiated `ClientDescriptor` `hello` carried (the `client` member
    /// of every `lifecycle.contract.deprecated_use` row — kernel-recorded at
    /// handshake, never authored by call params; CC2).
    pub(crate) client_desc: Option<ClientDescriptor>,
    /// Experiment-run writer leases held across `lab.experiment.*` calls
    /// (`experiment_run_id → Lease`; the fence still lives in the ledger —
    /// this map only avoids release/re-acquire churn per call).
    pub(crate) experiment_engines: BTreeMap<String, Lease>,
    /// `lab.results.subscribe` pull surfaces — `subscription_id →
    /// JournalSubscription` (the results-store post-durability journal;
    /// S4.3/R-2.10.5).
    pub(crate) results_subscriptions: BTreeMap<String, hh_results::journal::JournalSubscription>,
    /// Fleet-activation engines held across `fleet.*` calls (S4.9;
    /// `fleet_run → FleetEngine` — the writer lease persists like
    /// `experiment_engines`; the fence still lives in the ledger). The
    /// field itself is tier-gated: `--no-default-features` compiles the
    /// refusal-only `fleet_dispatch` in `fleet_ops`.
    #[cfg(feature = "tier-c4")]
    pub(crate) fleet_engines: BTreeMap<String, hh_fleet::engine::FleetEngine>,
    /// Evolution-campaign engines held across `lab.evolution.*` calls
    /// (S6.1a; `campaign_run → EvolutionCampaign` — the writer lease
    /// persists like `experiment_engines`; the fence still lives in the
    /// ledger). Tier-gated like `fleet_engines` (CC6).
    #[cfg(feature = "tier-c4")]
    pub(crate) evolution_campaigns: BTreeMap<String, hh_evolution::campaign::EvolutionCampaign>,
    /// Debt-manager services held across `lab.debt.{register,sweep,
    /// settle,retire,propose,manager_open}` calls (S6.1b; `registry_run →
    /// DebtManager` — the fold rebuilds from the durable prefix on `open`,
    /// CC3). Tier-gated like `evolution_campaigns` (CC6).
    #[cfg(feature = "tier-c4")]
    pub(crate) debt_managers: BTreeMap<String, hh_debt::manager::DebtManager>,
    /// Co-evolution cycle drivers held across `lab.coevolution.cycle_*`
    /// calls (S6.4; `cycle_key → CycleDriver` — the LabDocs sidecar
    /// re-folds from the deposit ref on `restore_ref`, CC3). Tier-gated
    /// like `debt_managers` (CC6; AC-R-2.9.8-11).
    #[cfg(feature = "tier-c4")]
    pub(crate) coevolution_cycles: BTreeMap<String, hh_evolution::cycle::CycleDriver>,
    /// The removable Hosting Plane driver (S4.5a; `hosting_ops::HostingPlane`
    /// — a pure-Json seam keeping `hosting_edges = []`). `None` = the tier
    /// is absent; ops needing it refuse `hosting_plane_absent`, never fake.
    pub(crate) hosting_plane: Option<crate::hosting_ops::HostingPlane>,
    /// The transport this service instance was bound through —
    /// `embedded` (a), `stdio` (b) or `local_network` (c); stamped on the
    /// `lifecycle.session.{attached,detached}{binding}` members S4.10
    /// mints (ADR-0301 D2; §7.2 P12 — the session record names the
    /// binding, never the socket).
    pub(crate) binding_label: String,
}

/// A declared host-executor capability (`supplies.host_capabilities[]`).
#[derive(Debug, Clone)]
pub(crate) struct HostCap {
    pub capability_id: String,
    pub surface_id: String,
    pub requires_approval: bool,
    pub options: Vec<String>,
    /// The ask's declared timeout (ms — `supplies.host_capabilities[]`
    /// `timeout` member); `None` = the ask never times out.
    pub timeout_ms: Option<u64>,
    /// The declared `risk_class` (the `{reversibility, repeat_safety, scope}`
    /// object — `supplies.host_capabilities[].risk_class`). `None`/unparseable
    /// reads as `RiskClass::UNKNOWN` at the ask gate — undeclared is
    /// never-auto (ADR-0031 §2), so `bypass` cannot silently skip it
    /// (AC-R-2.11.1-7).
    pub risk_class: Option<hh_ontology::risk::RiskClass>,
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
    /// The ask's deadline (`requested_at + timeout` when the pending row
    /// declared one; `None` = no deadline). Past it the sweep resolves the
    /// ask `timed_out` — a refusal record, never `unknown` (§5g recovery).
    pub deadline_ms: Option<u64>,
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
    pub driver: Option<Driver<Box<dyn ControlStrategy>>>,
    /// The bound control strategy's declared `steer_mode`/`concurrent_input`
    /// (`steer` honours it — `Unsupported{by: control_strategy}` only when
    /// the declaration says so, AC-R-2.6.1-10).
    pub steering: (SteerMode, ConcurrentInput),
    pub env_json: Json,
    pub env_handle_id: Option<String>,
    /// The run's shared context/memory fold (R2.5 / DF-S2.8-1) — the
    /// `KernelAssembler`/`KernelMemory`/`KernelCompaction` ports share
    /// it; `None` on sessions armed before the fold existed.
    pub mem_ctx: Option<std::rc::Rc<std::cell::RefCell<crate::runtime::KernelContext>>>,
    pub host_caps: Vec<HostCap>,
    pub turn_active: bool,
    pub active_turn: String,
    pub finished: bool,
    pub detached: Option<String>,
    /// The sealed definition's `authority_cap` rows (ADR-0240) — the
    /// approval mint's `requested ⊓ authority_cap` ceiling leg; computed at
    /// `open` from the sealed document (never re-resolved at respond).
    pub authority_caps: Vec<hh_monitor::mint::AuthorityCap>,
    /// The declared Π narrowing-leaf ids (the surface preset's —
    /// ADR-0168 D3): folded into `policy_fingerprint` at every lease
    /// touch so a leaf change revokes by key construction (ADR-0071 D1).
    /// Empty on resume/attach — the manifest's `narrowing_leaves` member
    /// is the durable record a rebuild re-reads.
    pub narrowing_leaf_ids: Vec<String>,
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
    /// The `router` slot's `model_fail` script remainder (R-2.7) — each
    /// `drive` hands it to `EmbedModel` and takes back what the calls did
    /// not consume (the plan is the session's, the port only borrows it).
    pub model_fail_plan: std::collections::VecDeque<crate::runtime::StagedCall>,
    /// The session's declared client (§7.2 P8; ADR-0301 D3) — `Some` on
    /// every surface-declared session (`client{kind:"web", sink}`); the
    /// kernel gates `measurement.export.delivered` minting + session-row
    /// stamping on it. `None` for legacy/CLI attaches.
    pub client: Option<hh_embed_schema::ClientDecl>,
    /// `contract_json` — the contract-carried surface descriptor the
    /// `open_session` params declared (AC-R-2.11.4-15; ADR-0304 D2):
    /// recorded verbatim, surfaced on `describe`'s session row, never
    /// interpreted.
    pub contract_json: Option<Json>,
    /// The durable `security.permission.*`/`lifecycle.run.suspended`
    /// delivery watermark for the run's declared `notification_sink`
    /// (`sink:file:*` — §7.1 D-2; ADR-0304 D1). Each session sweeps the
    /// rows since this seq to the sink file; the durable rows are the
    /// authority — the file is a derived, at-least-once delivery view.
    pub sink_seq: i64,
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
        // `KernelDescriptor.version` is the SemVer-class label — the
        // `kernel_version_id` tail through the one canonical spelling
        // (ADR-0332 D1 — DF-DOC.1-1).
        let kernel_version =
            hh_embed_schema::kernel_version_label(&config.kernel_version_id).to_string();
        Ok(EmbedService {
            kernel_version,
            kernel_prov: ProvenanceRecord::kernel("hh-embed", 0),
            store,
            registry,
            catalog: hh_assembly::catalog::Stage1Catalog::stage1(),
            env_drivers: BTreeMap::new(),
            credential_broker: hh_secrets::CredentialBroker::new(
                Box::new(hh_secrets::DenyAllResolver),
                "hh-embed/credentials",
            ),
            caller_auth_bindings: BTreeMap::new(),
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
            registry_run: None,
            contract_run: None,
            client_desc: None,
            hosting_plane: None,
            experiment_engines: BTreeMap::new(),
            #[cfg(feature = "tier-c4")]
            fleet_engines: BTreeMap::new(),
            #[cfg(feature = "tier-c4")]
            evolution_campaigns: BTreeMap::new(),
            #[cfg(feature = "tier-c4")]
            debt_managers: BTreeMap::new(),
            #[cfg(feature = "tier-c4")]
            coevolution_cycles: BTreeMap::new(),
            results_subscriptions: BTreeMap::new(),
            binding_label: "embedded".to_string(),
        })
    }

    /// Set the binding label — the (c) server calls this before serving so
    /// session rows stamp `binding: local_network`.
    pub fn set_binding_label(&mut self, label: &str) {
        self.binding_label = label.to_string();
    }

    /// Install the composed credential-broker resolver (R2.9a; DF-S2.4-1c)
    /// — the default `DenyAllResolver` is fail-closed, so a sentinel/
    /// rebind against an unwired host answers the broker's typed refusal,
    /// never a fabricated claim.
    pub fn set_credential_resolver(&mut self, resolver: Box<dyn hh_secrets::SecretSourceResolver>) {
        self.credential_broker =
            hh_secrets::CredentialBroker::new(resolver, "hh-embed/credentials");
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
        for (class, name, extra_decl) in [
            (
                hh_registry::suites::control_strategy_class(),
                "round_robin",
                BTreeMap::new(),
            ),
            (
                hh_registry::suites::control_strategy_class(),
                "react-steerable",
                BTreeMap::from([
                    (
                        "steer_mode".to_string(),
                        Json::str("interrupt_at_decision_point"),
                    ),
                    ("concurrent_input".to_string(), Json::str("steer")),
                ]),
            ),
            (
                hh_registry::suites::control_strategy_class(),
                // R2.6: the `queue_next_turn` declaration — the same
                // `ReactSteerable` interpreter (`variant_for`), a
                // different steer-mode declaration (the durable wakeup
                // seam is the queue — DF-S2.11-1).
                "react-steerable-queue",
                BTreeMap::from([
                    ("steer_mode".to_string(), Json::str("queue_next_turn")),
                    ("concurrent_input".to_string(), Json::str("steer")),
                ]),
            ),
            (
                hh_registry::suites::context_policy_class(),
                "full_window",
                BTreeMap::new(),
            ),
            (
                // R-2.7 — the `router` slot's scripted-lane variant: the
                // arm decodes the slot's sealed `policy` document and
                // runs the offline-decidable policy arms (DF-S1.18-1).
                hh_registry::suites::routing_policy_class(),
                "router_static",
                BTreeMap::new(),
            ),
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
                capability_declaration: {
                    let mut d = BTreeMap::from([("deterministic".to_string(), Json::Bool(true))]);
                    d.extend(extra_decl);
                    d
                },
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
        let result = match req.method.as_str() {
            "open_session" => self.open_session(&req.params),
            "close" => self.close(&req.params),
            "submit" => self.submit(&req.params),
            "cancel" => self.cancel(&req.params),
            "steer" => self.steer(&req.params),
            "set_coordinate" => self.set_coordinate(&req.params),
            "respond_permission" => self.respond_permission(&req.params),
            "fork" => self.fork(&req.params),
            "replay" => self.replay(&req.params),
            "counterfactual" => self.counterfactual(&req.params),
            "navigate" => self.navigate(&req.params),
            "coherent_fork_points" => self.coherent_fork_points(&req.params),
            "rollback" => self.rollback(&req.params),
            "promote" => self.branch_promote(&req.params),
            "discard" => self.branch_discard(&req.params),
            "amend" => self.amend(&req.params),
            "report_host_effect" => self.report_host_effect(&req.params),
            "respond_elicitation" => self.respond_elicitation(&req.params),
            "stream_events" => self.stream_events(&req.params),
            "read" => self.read_op(&req.params),
            "head" => self.head_op(&req.params),
            "project" => self.project(&req.params),
            "account" => self.account(&req.params),
            "describe" => self.describe(&req.params),
            "env.snapshot" => self.env_snapshot(&req.params),
            "env.derive" => self.env_derive(&req.params),
            "env.set_phase" => self.env_set_phase(&req.params),
            // R2.4 (DF-S2.10-1) — the lifecycle family: every verb routes
            // through the driver's declared capabilities; refusals stay
            // typed (`Unsupported`/`UnknownCapability`/`InvalidState`),
            // never faked.
            "env.open" => self.env_open(&req.params),
            "env.attach" => self.env_attach(&req.params),
            "env.close" => self.env_close(&req.params),
            "env.diff" => self.env_diff(&req.params),
            "env.restore" => self.env_restore(&req.params),
            "env.upload" => self.env_upload(&req.params),
            "env.download" => self.env_download(&req.params),
            "env.suspend" => self.env_suspend(&req.params),
            "env.resume" => self.env_resume(&req.params),
            "branch.open" => self.branch_open(&req.params),
            "continue_goal" => self.continue_goal(&req.params),
            "open_inbox" => self.open_inbox(&req.params),
            "subscribe" => self.wakeup_subscribe(&req.params),
            // R2.3 (DF-S2.3-1) — the §5a.3 durable-execution protocol
            // entry points; writer-session + writer-lease only.
            "suspend" => self.run_suspend(&req.params),
            "compensate" => self.run_compensate(&req.params),
            "heal" => self.run_heal(&req.params),
            "record_occurrence" => self.record_occurrence(&req.params),
            "list_leases" => self.list_leases(&req.params),
            "lineage" => self.lineage(&req.params),
            "run_index" => self.run_index_op(&req.params),
            "get_artifact" => self.get_artifact(&req.params),
            "audit_view" => self.audit_view(&req.params),
            "verify" => self.verify(&req.params),
            "prove_inclusion" => self.prove_inclusion(&req.params),
            "prove_consistency" => self.prove_consistency(&req.params),
            // ── Group L — the `lab.registry.*` boundary over the one
            // `RegistryStore` (S2.12; records-in/records-out, CC5).
            "lab.registry.register" => self.lab_registry_register(&req.params),
            "lab.registry.import" => self.lab_registry_import(&req.params),
            "lab.registry.refresh" => self.lab_registry_refresh(&req.params),
            "lab.registry.publish" => self.lab_registry_publish(&req.params),
            "lab.registry.resolve" => self.lab_registry_resolve(&req.params),
            "lab.registry.query" => self.lab_registry_query(&req.params),
            "lab.registry.catalog" => self.lab_registry_catalog(&req.params),
            "lab.registry.slot_choices" => self.lab_registry_slot_choices(&req.params),
            "lab.registry.substitutable" => self.lab_registry_substitutable(&req.params),
            "lab.registry.snapshot" => self.lab_registry_snapshot(&req.params),
            "lab.registry.record_conformance" => self.lab_registry_record_conformance(&req.params),
            "lab.registry.deprecate" => self
                .lab_registry_name_status(&req.params, hh_identity::names::NameStatus::Deprecated),
            "lab.registry.yank" => {
                self.lab_registry_name_status(&req.params, hh_identity::names::NameStatus::Yanked)
            }
            "lab.registry.revoke" => self.lab_registry_revoke(&req.params),
            "lab.registry.export" => self.lab_registry_export(&req.params),
            "lab.registry.pin" => self.lab_registry_pin(&req.params),
            "lab.registry.publisher_claims" => self.lab_registry_publisher_claims(&req.params),
            "lab.registry.lineage" => self.lab_registry_lineage(&req.params),
            "lab.registry.sameness" => self.lab_registry_sameness(&req.params),
            "lab.registry.verify" => self.lab_registry_verify(&req.params),
            // S4.14a (§5g.5/§6.2; R-2.8.5 C1, R-2.12.2¹) — the extension
            // lifecycle surface + the `ApproverGrant` issuance ops.
            "lab.extension.discover" => self.lab_extension_discover(&req.params),
            "lab.extension.resolve" => self.lab_extension_resolve(&req.params),
            "lab.extension.review" => self.lab_extension_review(&req.params),
            "lab.extension.check_surface" => self.lab_extension_check_surface(&req.params),
            "lab.extension.update" => self.lab_extension_update(&req.params),
            "lab.extension.revoke" => self.lab_extension_revoke(&req.params),
            "lab.extension.install" => self.lab_extension_install(&req.params),
            "lab.extension.import_plugin" => self.lab_extension_import_plugin(&req.params),
            "lab.extension.export_plugin" => self.lab_extension_export_plugin(&req.params),
            "lab.permission.grant_approver" => self.lab_permission_grant_approver(&req.params),
            "lab.permission.revoke_approver" => self.lab_permission_revoke_approver(&req.params),
            // ── S3.1: Group M measurement/bundle ops + Group L
            // `lab.serve` (R-2.9.3⁰/R-2.11.3⁰; ADR-0139…0141) ──
            "measurement.emit_metric" => self.emit_metric(&req.params),
            "kernel.bundle" => self.kernel_bundle(&req.params),
            "kernel.check_completeness" => self.kernel_check_completeness(&req.params),
            "kernel.reproduce" => self.kernel_reproduce(&req.params),
            "kernel.import" => self.kernel_import(&req.params),
            // ── S4.2: the scoped kinds ride `kernel.bundle{kind}`; the
            // §5h.3 lifecycle/status surface (validate S1..S9, diff,
            // fetch, export, status/set_status, attest, audit_bundle,
            // supersede, migrate, lineage).
            "kernel.validate" => self.kernel_validate(&req.params),
            "kernel.diff" => self.kernel_diff(&req.params),
            "kernel.fetch" => self.kernel_fetch(&req.params),
            "kernel.export" => self.kernel_export(&req.params),
            "kernel.status" => self.kernel_status(&req.params),
            "kernel.set_status" => self.kernel_set_status(&req.params),
            "kernel.attest" => self.kernel_attest(&req.params),
            "kernel.audit_bundle" => self.kernel_audit_bundle(&req.params),
            "kernel.supersede" => self.kernel_supersede(&req.params),
            "kernel.migrate" => self.kernel_migrate(&req.params),
            "kernel.lineage" => self.kernel_lineage(&req.params),
            "lab.serve" => self.lab_serve(&req.params),
            // ── S4.9: the `fleet.*` surface — §5i.1's C4 organizational
            // layer over the one Store (records-in/records-out; one
            // dispatch arm routes the whole family through
            // `fleet_ops::fleet_dispatch`; a `--no-default-features`
            // build answers `Unsupported{by: "tier-c4"}` — CC6).
            // ── S6.1a: the `lab.evolution.*` surface — §05h's C4
            // evolution pipeline (`run_kind = experiment` campaign runs;
            // `evolution_ops::evolution_dispatch`; a
            // `--no-default-features` build answers
            // `Unsupported{by: "tier-c4"}` — CC6).
            m if m.starts_with("lab.evolution.") => self.evolution_dispatch(m, &req.params),
            m if m.starts_with("fleet.") => self.fleet_dispatch(m, &req.params),
            m if m.starts_with("lab.coevolution.") || m.starts_with("lab.org_policy.") => {
                self.coevolution_dispatch(m, &req.params)
            }
            // ── S3.3: `lab.eval.*` — the eval kernel boundary
            // (R-2.9.2/R-2.9.4⁰ᵇ; records-in/records-out).
            "lab.eval.catalogue" => self.lab_eval_catalogue(&req.params),
            "lab.eval.compare" => self.lab_eval_compare(&req.params),
            "lab.eval.render_scorecard" => self.lab_eval_render_scorecard(&req.params),
            "lab.eval.equivalence_run" => self.lab_eval_equivalence_run(&req.params),
            "lab.eval.loss_report" => self.lab_eval_loss_report(&req.params),
            // ── S3.4a: `lab.experiment.*` — the single-worker experiment
            // engine (R-2.10.3⁰ᵇ; records-in resolvers, ledger+LabDocs
            // durable state).
            "lab.experiment.register" => self.lab_experiment_register(&req.params),
            "lab.experiment.expand" => self.lab_experiment_expand(&req.params),
            "lab.experiment.open_experiment" => self.lab_experiment_open(&req.params),
            "lab.experiment.next" => self.lab_experiment_next(&req.params),
            "lab.experiment.claim" => self.lab_experiment_claim(&req.params),
            "lab.experiment.launch" => self.lab_experiment_launch(&req.params),
            "lab.experiment.settle" => self.lab_experiment_settle(&req.params),
            "lab.experiment.pause" => self.lab_experiment_pause(&req.params),
            "lab.experiment.resume" => self.lab_experiment_resume(&req.params),
            "lab.experiment.close" => self.lab_experiment_close(&req.params),
            // ── S3.4c: `lab.analysis.analyze` — the estimator kernel over
            // the durable results store (R-2.10.4⁰ᵇ; A1/A2/A3-contrast/A8/
            // A12 + transfer rows; the remaining `lab.analysis.*` /
            // `lab.results.*` ops land with their own tickets).
            "lab.analysis.analyze" => self.lab_analysis_analyze(&req.params),
            // ── S4.3: `lab.producer.*` (the §6.5 §2.3 producer contract),
            // `lab.results.*` (the derived-state read/verify/export
            // surface), `lab.leaderboard.*` (define/snapshot/diff/publish/
            // retract) and `lab.analysis.{render,diff_reports,power}`
            // (R-2.10.4¹ / R-2.10.5; ADR-0294).
            "lab.producer.declare" => self.lab_producer_declare(&req.params),
            "lab.producer.bind" => self.lab_producer_bind(&req.params),
            "lab.producer.exclude" => self.lab_producer_exclude(&req.params),
            "lab.producer.amend" => self.lab_producer_amend(&req.params),
            "lab.producer.record_analysis" => self.lab_producer_record_analysis(&req.params),
            "lab.analysis.render" => self.lab_analysis_render(&req.params),
            "lab.analysis.diff_reports" => self.lab_analysis_diff_reports(&req.params),
            "lab.analysis.power" => self.lab_analysis_power(&req.params),
            "lab.results.get_row" => self.lab_results_get_row(&req.params),
            "lab.results.row_history" => self.lab_results_row_history(&req.params),
            "lab.results.query_rows" => self.lab_results_query_rows(&req.params),
            "lab.results.cells" => self.lab_results_cells(&req.params),
            "lab.results.distribution" => self.lab_results_distribution(&req.params),
            "lab.results.catalogue" => self.lab_results_catalogue(&req.params),
            "lab.results.subscribe" => self.lab_results_subscribe(&req.params),
            "lab.results.verify_row" => self.lab_results_verify_row(&req.params),
            "lab.results.verify_snapshot" => self.lab_results_verify_snapshot(&req.params),
            "lab.results.verify_citation" => self.lab_results_verify_citation(&req.params),
            "lab.results.export_rows" => self.lab_results_export_rows(&req.params),
            "lab.leaderboard.define" => self.lab_leaderboard_define(&req.params),
            "lab.leaderboard.leaderboard" => self.lab_leaderboard_leaderboard(&req.params),
            "lab.leaderboard.diff_snapshots" => self.lab_leaderboard_diff_snapshots(&req.params),
            "lab.leaderboard.publish" => self.lab_leaderboard_publish(&req.params),
            "lab.leaderboard.retract_entry" => self.lab_leaderboard_retract_entry(&req.params),
            // S4.4 — §6.5 §2.2's `snapshots(definition_ref) →
            // [snapshot_id]` retained-snapshot list.
            "lab.leaderboard.snapshots" => self.lab_leaderboard_snapshots(&req.params),
            // ── S4.5a: `lab.hosting.*` — the Hosting ABI boundary
            // (R-2.10.6; records-in/records-out over registry + ledger;
            // `drive` forwards through the removable HostingPlane seam).
            "lab.hosting.describe" => self.lab_hosting_describe(&req.params),
            "lab.hosting.probe" => self.lab_hosting_probe(&req.params),
            "lab.hosting.attach" => self.lab_hosting_attach(&req.params),
            "lab.receipt.record" => self.lab_receipt_record(&req.params),
            // ── S5.4: `lab.debt.*` (the live all-home evaluator +
            // DebtIndex + the health-report/notifier surface —
            // R-2.9.6¹), `lab.model.*` (synthetic provider-drift claims +
            // the regression-suite fold — R-2.9.8¹), and
            // `lab.analysis.{component_targets,attribution_design}` (the
            // M1 design surface — R-2.9.7¹). All records-in/records-out
            // (AC-R-2.9.6-10: the manager is out-of-process by
            // construction — no private verb).
            "lab.debt.evaluate" => self.lab_debt_evaluate(&req.params),
            "lab.debt.index" => self.lab_debt_index(&req.params),
            "lab.debt.report" => self.lab_debt_report(&req.params),
            // ── S6.1b: the §5h.6 assumption-debt *manager* service ops
            // (C4-tier — `hh-debt`; records-in/records-out; a
            // `--no-default-features` build answers
            // `Unsupported{by: "tier-c4"}` — CC6).
            "lab.debt.manager_open"
            | "lab.debt.register"
            | "lab.debt.sweep"
            | "lab.debt.settle"
            | "lab.debt.retire"
            | "lab.debt.propose"
            | "lab.debt.post_import_sweep" => {
                self.debt_manager_dispatch(req.method.as_str(), &req.params)
            }
            "lab.model.snapshot_claim" => self.lab_model_snapshot_claim(&req.params),
            "lab.model.regression" => self.lab_model_regression(&req.params),
            "lab.analysis.component_targets" => self.lab_analysis_component_targets(&req.params),
            "lab.analysis.attribution_design" => self.lab_analysis_attribution_design(&req.params),
            // ── S6.3b: `lab.attribution.*` — the designed causal-
            // attribution surface (R-2.9.7 6c; §5h.7; `attribute` with
            // `open_arms` drives the real Group W counterfactual legs —
            // instrument-charged `branch_kind = counterfactual`).
            "lab.attribution.design" => self.lab_attribution_design(&req.params),
            "lab.attribution.attribute" => self.lab_attribution_attribute(&req.params),
            "lab.attribution.locus" => self.lab_attribution_locus(&req.params),
            "lab.attribution.quality" => self.lab_attribution_quality(&req.params),
            // ── S3.5: `lab.assembly.*` — the assembly service boundary
            // (R-2.10.1; §6.1). Semantics-free: records-in/records-out over
            // the one kernel resolver via `hh_lab::assembly`.
            "lab.assembly.assemble" => self.lab_assembly_assemble(&req.params),
            // S4.12 — `lab.assembly.compile` runs the full §3.2
            // pipeline (R-2.11.4; records-in/records-out).
            "lab.assembly.compile" => self.lab_assembly_compile(&req.params),
            "lab.assembly.plan" => self.lab_assembly_plan(&req.params),
            "lab.assembly.apply" => self.lab_assembly_apply(&req.params),
            "lab.assembly.validate_batch" => self.lab_assembly_validate_batch(&req.params),
            "lab.assembly.explain" => self.lab_assembly_explain(&req.params),
            "lab.assembly.diff" => self.lab_assembly_diff(&req.params),
            "lab.assembly.drift" => self.lab_assembly_drift(&req.params),
            "lab.assembly.identity" => self.lab_assembly_identity(&req.params),
            "lab.assembly.adopt" => self.lab_assembly_adopt(&req.params),
            _ => Err(EmbedError::SchemaViolation {
                path: "/method".to_string(),
                code: "unknown_method".to_string(),
            }),
        };
        // §7.4 rule 5 (ADR-0178 D5; S5.8): a *deprecated* op stays callable
        // for the rest of its major, but every admitted use is ledgered —
        // `lifecycle.contract.deprecated_use{method, client}` under the
        // caller's own run (or the contract bookkeeping run for a
        // session-less/read-only caller). The kernel mints the row; the
        // client descriptor is the `hello`-negotiated one — call params
        // never author it (CC2). The mint precedes the result: a use the
        // ledger cannot account is a refusal, never a silent pass (CC3).
        if result.is_ok() && op.deprecated.is_some() {
            self.note_deprecated_use(req, &op)?;
        }
        // §7.1 D-2 (ADR-0304 D1): a successful session call sweeps the
        // run's newly-durable `security.permission.*`/`suspended` rows
        // to its declared `notification_sink` — the deferred-ask
        // delivery a detached `--async`/`--defer` invocation relies on
        // (`attach` sessions deliver too: the sink is a derived view,
        // never a write path).
        if result.is_ok() {
            if let Some(sid) = req.params.get("session_id").and_then(Json::as_str) {
                self.flush_notification_sink(sid)?;
            }
        }
        result
    }

    // ── hello ───────────────────────────────────────────────────────────

    fn hello(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = HelloParams::from_json(params)?;
        let result = negotiate(&p, &self.kernel_version)?;
        self.hello_done = true;
        self.client_desc = Some(p.client.clone());
        self.experimental = result.negotiated.experimental;
        self.client_caps = result.negotiated.clone();
        Ok(result.to_json())
    }

    // ── shared helpers ──────────────────────────────────────────────────

    pub(crate) fn alloc(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}-{}", self.next)
    }

    /// Wire the removable Hosting Plane driver (S4.5a; CC6 — `None`
    /// removes the tier: `lab.hosting.probe{drive}` then refuses
    /// `hosting_plane_absent`; describe/attach stay records-only).
    pub fn set_hosting_plane(&mut self, plane: Option<crate::hosting_ops::HostingPlane>) {
        self.hosting_plane = plane;
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

    /// `note_deprecated_use(req, op)` — append the kernel-origin
    /// `lifecycle.contract.deprecated_use{method, client}` row for an
    /// admitted deprecated-op call (§7.4 rule 5; ADR-0178 D5). The row lands
    /// on the caller session's run under its writer lease; a session-less or
    /// `attach` (lease-less) caller's use lands on the contract bookkeeping
    /// run so no use goes unaccounted (CC3). The `client` member is the
    /// `hello`-negotiated `ClientDescriptor` — a call parameter can never
    /// author or widen it (CC2); the request's `session_id`/`run_id` are
    /// recorded as context members only.
    pub(crate) fn note_deprecated_use(
        &mut self,
        req: &Request,
        op: &hh_embed_schema::ops::OpSpec,
    ) -> Result<(), EmbedError> {
        let dep = op
            .deprecated
            .clone()
            .expect("note_deprecated_use called on an undeprecated op");
        let session_id = req
            .params
            .get("session_id")
            .and_then(Json::as_str)
            .map(str::to_string);
        let run_id = req
            .params
            .get("run_id")
            .or_else(|| req.params.get("run"))
            .and_then(Json::as_str)
            .map(str::to_string)
            .or_else(|| {
                session_id
                    .as_deref()
                    .and_then(|s| self.sessions.get(s))
                    .map(|s| s.run_id.clone())
            });
        let session_lease = session_id
            .as_deref()
            .and_then(|s| self.sessions.get(s))
            .and_then(|s| s.lease.clone());
        let client = match &self.client_desc {
            Some(c) => c.to_json(),
            None => Json::Null,
        };
        let payload = Json::obj([
            ("method", Json::str(op.name)),
            ("client", client),
            ("deprecated", dep.to_json()),
            (
                "session_id",
                session_id.map(Json::str).unwrap_or(Json::Null),
            ),
            (
                "run_id",
                run_id.clone().map(Json::str).unwrap_or(Json::Null),
            ),
        ]);
        match (run_id, session_lease) {
            (Some(r), Some(l)) => self.mint(&r, &l, "lifecycle.contract.deprecated_use", payload),
            _ => {
                let (run_id, lease) = self.ensure_contract_run()?;
                self.mint(
                    &run_id,
                    &lease,
                    "lifecycle.contract.deprecated_use",
                    payload,
                )
            }
        }
    }

    /// The kernel-internal ledger run that carries
    /// `lifecycle.contract.deprecated_use` rows for callers holding no writer
    /// lease (session-less and `attach` calls) — created lazily, re-leased on
    /// expiry exactly like `ensure_registry_run` (one fenced writer for
    /// contract-usage bookkeeping; the purpose member labels it).
    pub(crate) fn ensure_contract_run(&mut self) -> Result<(String, Lease), EmbedError> {
        if let Some((run_id, lease)) = &self.contract_run {
            match self.store.renew(lease) {
                Ok(l) => {
                    let out = (run_id.clone(), l.clone());
                    self.contract_run = Some(out.clone());
                    return Ok(out);
                }
                Err(_) => {
                    let l = self
                        .store
                        .acquire_writer(&self.holder, run_id, LEASE_TTL_MS)
                        .map_err(ledger_err)?;
                    let out = (run_id.clone(), l);
                    self.contract_run = Some(out.clone());
                    return Ok(out);
                }
            }
        }
        let mut manifest = RunManifest::minimal(RunKind::Fleet);
        manifest.configuration_id = None;
        manifest.configuration_version_id = None;
        manifest
            .extra
            .insert("purpose".to_string(), Json::str("contract"));
        let (run_id, lease) = self
            .store
            .open_run(manifest, &self.holder)
            .map_err(ledger_err)?;
        let out = (run_id, lease);
        self.contract_run = Some(out.clone());
        Ok(out)
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
        let timeout = capability.and_then(|c| c.timeout_ms);
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
                (
                    "timeout",
                    timeout.map(|t| Json::Int(t as i64)).unwrap_or(Json::Null),
                ),
            ]),
        )?;
        // R2.3 (DF-S2.3-1) — a deadline-bearing pending is a durable
        // timer: mint the `timer` subscription the deadline produces so
        // a suspended run re-cues to resolve `timed_out` (the ask's
        // pending row is the producer; the `control.wakeup.scheduled`
        // row is durable before any occurrence it synthesizes). One
        // live sub per deadline instant — a second pending at the same
        // instant rides the existing promise.
        if let Some(deadline_ms) = timeout.map(|t| now.saturating_add(t)) {
            let covered = self
                .store
                .wakeup_subscriptions(&run_id)
                .map_err(ledger_err)?
                .iter()
                .any(|s| {
                    s.state != hh_ledger::wakeup::SubscriptionState::Cancelled
                        && s.policy.expires_at_ms.is_none_or(|e| now < e)
                        && s.trigger == (LedgerTrigger::Timer { at_ms: deadline_ms })
                });
            if !covered {
                let created_by = hh_ledger::manifest::EventRef {
                    run_id: run_id.clone(),
                    event_id: self.store.head(&run_id).map_err(ledger_err)?.event_id,
                };
                self.store
                    .wakeup_subscribe(
                        &run_id,
                        &lease,
                        LedgerTrigger::Timer { at_ms: deadline_ms },
                        hh_ledger::wakeup::WakeupPolicy::default_policy(),
                        &created_by,
                    )
                    .map_err(ledger_err)?;
            }
        }
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
                deadline_ms: timeout.map(|t| now.saturating_add(t)),
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

    /// The pending-timeout sweep (§5g recovery — a `permission pending`
    /// past its declared `deadline` resolves `refused`, never `unknown`):
    /// each expired ask mints `security.permission.decided{decision:
    /// timed_out, decider: kernel}` and, when it covered an effect,
    /// `action.effect.refused{reason: timed_out}` plus the `approval{deny}`
    /// cue — the loop observes the denial at the next decision point,
    /// exactly as a principal's `deny` would deliver it.
    pub(crate) fn sweep_pending_timeouts(&mut self, sess_id: &str) -> Result<(), EmbedError> {
        let (run_id, lease, expired) = {
            let s = self.session(sess_id)?;
            let lease = match &s.lease {
                Some(l) => l.clone(),
                None => return Ok(()), // attach session — read-only, never mints
            };
            if s.finished {
                return Ok(());
            }
            let now = self.store.now_ms();
            let expired: Vec<(String, Option<String>, u64)> = s
                .pendings
                .iter()
                .filter(|(_, p)| p.deadline_ms.is_some_and(|d| now >= d))
                .map(|(pid, p)| {
                    (
                        pid.clone(),
                        p.effect_id.clone(),
                        now.saturating_sub(p.requested_at),
                    )
                })
                .collect();
            (s.run_id.clone(), lease, expired)
        };
        for (permission_id, effect_id, wait_ms) in expired {
            self.mint(
                &run_id,
                &lease,
                "security.permission.decided",
                Json::obj([
                    ("permission_id", Json::str(permission_id.clone())),
                    ("decision", Json::str("timed_out")),
                    ("decider", Json::str("kernel")),
                    ("wait_ms", Json::Int(wait_ms as i64)),
                ]),
            )?;
            if let Some(ef) = &effect_id {
                self.mint(
                    &run_id,
                    &lease,
                    "action.effect.refused",
                    Json::obj([
                        ("effect_id", Json::str(ef.clone())),
                        ("decider", Json::str("kernel")),
                        ("reason", Json::str("timed_out")),
                    ]),
                )?;
            }
            let s = self.session_mut(sess_id)?;
            s.decided.insert(
                permission_id.clone(),
                Json::obj([
                    ("kind", Json::str("selected")),
                    ("decision", Json::str("timed_out")),
                ]),
            );
            s.pendings.remove(&permission_id);
            if let Some(ef) = effect_id {
                if let Some(d) = s.driver.as_mut() {
                    d.submit(Cue::HumanInput(HumanInput::Approval {
                        effect_id: ef,
                        allow: false,
                    }));
                }
            }
        }
        Ok(())
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
        let host_surfaces: BTreeMap<String, crate::runtime::HostDecl> = self
            .session(sess_id)?
            .host_caps
            .iter()
            .map(|c| {
                (
                    c.surface_id.clone(),
                    crate::runtime::HostDecl {
                        risk_class: c.risk_class,
                        requires_approval: c.requires_approval,
                    },
                )
            })
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
        // Sweep the live `pendings` first — a past-deadline ask resolves
        // `timed_out` (the kind-fixed terminal refusal, §5g recovery:
        // `refused`, never `unknown`) before the loop is driven, so the
        // `approval{deny}` cue lands in this drain.
        self.sweep_pending_timeouts(sess_id)?;
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
        // `steer_mode = queue_next_turn` steer deliveries arrive through
        // the same drain — the `steer{mode: next_turn}` op mints a
        // `manual`+`steer` occurrence (`control.wakeup.occurred`,
        // durable-first), `deliver_wakeup` fires it, and the `Cue::Woken
        // {delivery_mode: steer}` submitted here is the cue (R2.6;
        // DF-S2.11-1 — the wakeup table is the one durable steer record;
        // a restart re-folds the same rows).
        // R2.4 (DF-S2.9-3) — the sink's cadence tail: the run's env driver
        // + the session's env handle. `turn.finished`/effect-settled appends
        // fire the declared take *inside* the sink so the row lands before
        // `lifecycle.run.finished` can seal the store.
        let env_handle = self.session(sess_id)?.env_handle_id.clone();
        // R2.5 / DF-S2.8-1 — the memory + compaction boundaries wire when
        // the session carries a context fold (open/continue arms build
        // it; legacy sessions without one run the pre-R2.5 shape — a
        // retrieve decision there still fails `UnbackedPort`, honestly).
        let mem_ctx = self.session(sess_id)?.mem_ctx.clone();
        let fail_plan = std::mem::take(&mut self.session_mut(sess_id)?.model_fail_plan);
        let mut sink = KernelSink {
            store: &mut self.store,
            run_id: run_id.clone(),
            lease: lease.clone(),
            envs: self.env_drivers.get_mut(&run_id),
            env_handle,
        };
        let mut model = EmbedModel {
            invoke,
            completion,
            response_ref,
            calls_made: 0,
            fail_plan,
        };
        let mut gate = EmbedGate {
            host_surfaces,
            host_asks: Vec::new(),
        };
        let mut asm = KernelAssembler {
            ctx: mem_ctx.clone(),
        };
        if let Some(ctx) = &mem_ctx {
            driver.set_memory_port(Box::new(crate::runtime::KernelMemory::new(ctx.clone())));
            driver
                .set_compaction_port(Box::new(crate::runtime::KernelCompaction::new(ctx.clone())));
        }
        let outcome = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
        // R2.4 — `on_idle`: a parked drive (inbox emptied mid-turn) is the
        // env's idle boundary; the declared cadence take lands durable
        // below while the run is still open.
        let parked = matches!(&outcome, Err(DriverError::Port { .. }));
        let asks = std::mem::take(&mut gate.host_asks);
        let fail_left = std::mem::take(&mut model.fail_plan);
        let s = self.session_mut(sess_id)?;
        s.driver = Some(driver);
        s.model_fail_plan = fail_left;
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
        if parked {
            let env_handle = self.session(sess_id)?.env_handle_id.clone();
            if let (Some(drv), Some(env_id)) =
                (self.env_drivers.get_mut(&run_id), env_handle.as_deref())
            {
                drv.cadence_take(
                    &mut self.store,
                    &lease,
                    env_id,
                    hh_env::driver::SnapshotTrigger::Idle,
                )
                .map_err(crate::open::env_err)?;
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
                                        deadline_ms: p.get("timeout").and_then(Json::as_int).map(
                                            |t| {
                                                (p.get("requested_at")
                                                    .and_then(Json::as_int)
                                                    .unwrap_or(0)
                                                    .max(0)
                                                    as u64)
                                                    .saturating_add(t.max(0) as u64)
                                            },
                                        ),
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
        LedgerTrigger::Schedule {
            expr,
            kind,
            timezone,
        } => WokenTrigger::Schedule {
            expression: expr.clone(),
            timezone: timezone.clone(),
            kind: kind.as_str().to_string(),
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
        LedgerTrigger::External {
            kind,
            source_ref,
            filter,
        } => WokenTrigger::External {
            source_ref: source_ref.clone().unwrap_or_else(|| kind.clone()),
            filter: filter.clone().unwrap_or_default(),
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

// ═══════════════════════════════════════════════════════════════════════
//  S4.11 — the `surface_*` seam (ADR-0303; R-2.11.3¹).
//
//  The MCP lab instrument (`hh-mcp-lab`) is a surface: it owns no ledger
//  internals. Every durable write it needs lands through this narrow
//  seam — `surface_open_run`/`surface_append`/`surface_commit_row` are
//  the ONLY ledger entry points — plus the handful of read helpers the
//  surface-session/projection code needs. Nothing here exposes mutable
//  service state (`sessions`, `drivers`, `boundary` stay private); a
//  surface can open ITS run, append under ITS lease, and mint kernel
//  rows — the same mediation the embed ops get, at the write edge.
// ═══════════════════════════════════════════════════════════════════════
impl EmbedService {
    /// Open a surface-owned run (`run_kind = surface`) under the
    /// service holder — returns `(run_id, writer lease)`. The
    /// manifest's `spawn_event`/`parent`/`incomplete` fields resolve at
    /// open exactly like a session-opened run (a dangling ref refuses).
    pub fn surface_open_run(
        &mut self,
        manifest: hh_ledger::manifest::RunManifest,
    ) -> Result<(String, hh_ledger::store::Lease), EmbedError> {
        let holder = self.holder.clone();
        self.store.open_run(manifest, &holder).map_err(ledger_err)
    }

    /// Acquire (or re-acquire) the writer lease on a run — the
    /// post-crash re-attach for a persisted surface run (`takeover`
    /// lands inside `acquire_writer` when the persisted holder
    /// differs).
    pub fn surface_acquire_writer(
        &mut self,
        run_id: &str,
        ttl_ms: u64,
    ) -> Result<hh_ledger::store::Lease, EmbedError> {
        let holder = self.holder.clone();
        self.store
            .acquire_writer(&holder, run_id, ttl_ms)
            .map_err(ledger_err)
    }

    /// Append caller-built events to `run_id` under `lease` — the full
    /// append gate (fencing, scopes, effect fold, class table) applies.
    pub fn surface_append(
        &mut self,
        run_id: &str,
        lease: &hh_ledger::store::Lease,
        events: Vec<hh_ledger::event::Event>,
    ) -> Result<hh_ledger::event::SeqRange, EmbedError> {
        self.store.append(run_id, lease, events).map_err(ledger_err)
    }

    /// Mint one kernel-provenance row on `run_id` — the durable surface
    /// records (`lifecycle.*`, `measurement.export.delivered`,
    /// `lifecycle.surface.call.minted`) that only the kernel may author.
    pub fn surface_commit_row(
        &mut self,
        producer: &str,
        run_id: &str,
        class: &str,
        payload: hh_wire::json::Json,
        refs: Vec<hh_identity::ContentAddress>,
        causes: Vec<hh_ledger::manifest::EventRef>,
    ) -> Result<hh_ledger::event::EventEnvelope, EmbedError> {
        self.store
            .commit_kernel_row_for(producer, run_id, class, payload, refs, causes)
            .map_err(ledger_err)
    }

    /// The run's committed envelopes (the surface-session's own run —
    /// turn/effect folds + the minted-handle table rebuild off this).
    pub fn surface_events(
        &self,
        run_id: &str,
    ) -> Result<&[hh_ledger::event::EventEnvelope], EmbedError> {
        self.store.events(run_id).map_err(ledger_err)
    }

    /// The run's manifest.
    pub fn surface_manifest(
        &self,
        run_id: &str,
    ) -> Result<&hh_ledger::manifest::RunManifest, EmbedError> {
        self.store.manifest(run_id).map_err(ledger_err)
    }

    /// Enumerate persisted run ids (the session-rebind scan on
    /// `run_kind = surface` + `manifest.caller_binding`).
    pub fn surface_run_ids(&self) -> Vec<String> {
        self.store.run_ids()
    }

    /// The head event id — `Event.parent_event_id` links (the seam's
    /// `mint_event` equivalent needs the tip).
    pub fn surface_head_event_id(&self, run_id: &str) -> Result<String, EmbedError> {
        Ok(self.store.head(run_id).map_err(ledger_err)?.event_id)
    }

    /// The budget `Account` over the surface run — charges project off
    /// the run's own `control.budget.*` rows.
    pub fn surface_account(
        &mut self,
        run_id: &str,
    ) -> Result<hh_budget::account::Account<'_>, EmbedError> {
        hh_budget::account::Account::open(&mut self.store, run_id).map_err(|e| {
            EmbedError::Refused {
                reason: format!("budget account: {e}"),
            }
        })
    }

    /// Arm the KP-9 durability fault — the harness test path
    /// (`durability` stays an injected-fault knob, never a flag a
    /// caller can pass).
    pub fn surface_inject_durability_faults(&mut self, n: usize) {
        self.store.inject_durability_faults(n as u32);
    }

    /// The service's id counter — surface-minted ids (`ef-*`,
    /// `turn-*`, `hnd-*` payloads) share the monotonic space.
    pub fn surface_alloc_id(&mut self, prefix: &str) -> String {
        self.alloc(prefix)
    }

    /// The store clock (ms) — a surface stamps rows with the kernel's
    /// own clock, never a second timebase.
    pub fn surface_now_ms(&self) -> i64 {
        self.store.now_ms() as i64
    }

    /// The store's RFC-3339 timestamp — `Event.ts` for surface mints.
    pub fn surface_ts_now(&self) -> String {
        self.store.ts_now()
    }

    // ── R2.19 — the caller-credential mediation seam (DF-S4.11-2;
    //   WS-H3). A surface never sees credential material beyond the
    //   presented token itself: the kernel broker's `minted_scoped` leg
    //   verifies audience + MAC + expiry + binding liveness and the
    //   mint path ledgeres `granted`/`decided`/`bound`/`used` rows —
    //   durable before the token answers (SV-5). ──

    /// Verify a presented caller credential against `audience` — the
    /// WS-H3 mediator's check (spec §5g.3; DF-S4.11-2). The verdict is
    /// the broker's typed `MintedVerdict` (`Valid` | `Invalid` |
    /// `Expired` | `Revoked`) — a token the kernel never minted is
    /// `Invalid`, never a resolved subject.
    pub fn surface_verify_caller_token(&self, token: &str, audience: &str) -> MintedVerdict {
        self.credential_broker
            .verify_minted(token, audience, self.store.now_ms())
    }

    /// Mint a caller credential token bound to `audience` — the
    /// kernel's own `minted_scoped` issue path (the authorization
    /// server's real leg at this slice; DF-S4.11-2). Every row lands
    /// durable before the token answers: the channel registers once,
    /// the per-audience binding runs
    /// `security.permission.granted` → `decided{allow}` →
    /// `security.credential.bound` under the contract run's fenced
    /// writer (the PDP gate `bind` re-verifies the recorded decision),
    /// and `mint` lands `security.credential.used` before the token is
    /// visible. The returned string is the credential — it goes to the
    /// caller, never into a ledger or record.
    pub fn surface_mint_caller_token(
        &mut self,
        audience: &str,
        ttl_ms: u64,
    ) -> Result<String, EmbedError> {
        use hh_monitor::assess::SecretTransport;
        use hh_secrets::{
            AccessClass, AuthCarrier, BindRequest, CredentialKind, DestinationBinding,
            SecretChannelSpec, SecretSource, SenderConstraint,
        };
        const CHANNEL: &str = "caller_auth";
        const HOLDER: &str = "kernel.caller_auth";
        let (run_id, lease) = self.ensure_contract_run()?;
        // The channel — registered once; the source is a coordinate
        // only (a `minted_scoped` token carries no secret material, so
        // no resolver read ever happens). `ChannelExists` on re-entry
        // is the idempotent case.
        let prov = self.kernel_prov.clone();
        let spec = SecretChannelSpec {
            kind: CredentialKind::ApiKey,
            source: SecretSource::OperatorVault {
                vault_ref: "vault:caller_auth".to_string(),
            },
            destinations: vec![DestinationBinding {
                scheme: "mcp".to_string(),
                host_pattern: "mcp://*".to_string(),
                port: None,
                path_prefix: None,
                revocation_path: None,
                auth_carrier: AuthCarrier::Header {
                    name: "Authorization".to_string(),
                    prefix: Some("Bearer ".to_string()),
                },
            }],
            allowed_env_names: None,
            delivery_modes: [SecretTransport::MintedScoped].into_iter().collect(),
            max_lifetime_ms: None,
            rotation_policy: None,
            sender_constraint: SenderConstraint::Audience,
            constraints: Default::default(),
            bindable: true,
            access_class: AccessClass::Broker,
            canary: false,
            description: "surface caller-auth tokens (the kernel's own AS leg)".to_string(),
        };
        match self.credential_broker.register_channel(CHANNEL, spec, prov) {
            Ok(_) | Err(hh_secrets::BrokerError::ChannelExists { .. }) => {}
            Err(e) => {
                return Err(EmbedError::Refused {
                    reason: format!("caller_auth_channel: {e:?}"),
                })
            }
        }
        // The per-audience binding — the fence `mint` re-checks (a
        // token can only ever claim an audience its binding names).
        if !self.caller_auth_bindings.contains_key(audience) {
            let holder = HOLDER.to_string();
            let handle_id = self.store.alloc_id("hnd");
            let granted = Json::obj([
                ("handle_id", Json::str(handle_id.clone())),
                (
                    "permission_ref",
                    Json::obj([
                        ("semantic_id", Json::str("perm.caller_auth")),
                        (
                            "version_id",
                            Json::str(format!("sha256:{}", "0".repeat(64))),
                        ),
                    ]),
                ),
                ("holder", Json::str(holder.clone())),
                ("issuer", self.kernel_prov.to_json()),
                (
                    "grants",
                    Json::Arr(vec![Json::obj([
                        (
                            "effect",
                            Json::obj([("domain", Json::str("secret_access"))]),
                        ),
                        ("scope", Json::str(format!("secret:{CHANNEL}"))),
                        ("constraints", Json::obj([])),
                        ("delegable", Json::Bool(false)),
                    ])]),
                ),
                ("ceiling", Json::str("principal")),
                (
                    "validity",
                    Json::obj([
                        ("issued_at", Json::str(self.store.ts_now())),
                        ("expires_at", Json::Null),
                    ]),
                ),
                ("parent_handle", Json::Null),
                ("delegable", Json::Bool(false)),
                ("origin_basis", Json::str("approval")),
                ("basis_ref", Json::str("perm.caller_auth")),
                ("budget_ref", Json::Null),
                ("authority_delta", Json::str("none")),
            ]);
            self.mint(&run_id, &lease, "security.permission.granted", granted)?;
            let decided = EventMinter::new(&self.store, &run_id)
                .mint(
                    "security.permission.decided",
                    Json::obj([
                        ("effect_id", Json::str(format!("eff-{handle_id}"))),
                        ("decision", Json::str("allow")),
                        ("effective_authority", Json::str("principal")),
                        (
                            "effective_risk_class",
                            Json::obj([
                                ("reversibility", Json::str("reversible")),
                                ("repeat_safety", Json::str("idempotent")),
                                ("scope", Json::str("ephemeral")),
                            ]),
                        ),
                        ("handle_ids", Json::Arr(vec![Json::str(handle_id.clone())])),
                        ("policy_ref", Json::str("pi/1")),
                        ("decider", Json::str("policy")),
                        ("attempt_no", Json::Int(1)),
                        ("proposal", Json::str("caller-auth-mint")),
                        ("taint", Json::Arr(vec![])),
                    ]),
                )
                .map_err(ledger_err)?;
            let decided_id = decided.event_id.clone();
            self.store
                .append(&run_id, &lease, vec![decided])
                .map_err(ledger_err)?;
            let binding = self
                .credential_broker
                .bind(
                    &mut self.store,
                    &run_id,
                    &lease,
                    BindRequest {
                        channel_id: CHANNEL.to_string(),
                        holder,
                        env_handle_ref: "caller-surface".to_string(),
                        env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
                        mode: SecretTransport::MintedScoped,
                        monitor_decision_ref: decided_id,
                        destinations: [audience.to_string()].into_iter().collect(),
                    },
                )
                .map_err(|e| EmbedError::Refused {
                    reason: format!("caller_auth_bind: {e:?}"),
                })?;
            self.caller_auth_bindings
                .insert(audience.to_string(), binding.binding_id);
        }
        let binding_id = self.caller_auth_bindings[audience].clone();
        let minted = self
            .credential_broker
            .mint(
                &mut self.store,
                &run_id,
                &lease,
                &binding_id,
                audience,
                ttl_ms,
            )
            .map_err(|e| EmbedError::Refused {
                reason: format!("caller_auth_mint: {e:?}"),
            })?;
        Ok(minted.token)
    }
}

/// The caller-token verdict the surface seam answers — re-exported
/// through the kernel boundary so a surface (`hh-mcp-lab`) consumes the
/// broker's own closed sum without a direct `hh-secrets` edge
/// (removability: surfaces reach the kernel only; R2.19/DF-S4.11-2).
pub use hh_secrets::MintedVerdict;

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
        // The branch-model refusals (S2.9; ADR-0271) — the ledger's typed
        // words, mapped to the contract's closed sum.
        L::ForkPointNotCoherent {
            at_seq,
            open_scopes,
            ..
        } => EmbedError::Refused {
            reason: format!(
                "fork_point_not_coherent: seq {at_seq} blocked by [{}]",
                open_scopes.join(", ")
            ),
        },
        L::SourceIncomplete {
            run_id,
            at_seq,
            head,
        } => EmbedError::Refused {
            reason: format!("source_incomplete: {run_id} seq {at_seq} beyond tip {head}"),
        },
        L::SnapshotUnavailable { kind, detail } => EmbedError::EnvironmentUnavailable {
            reason: format!("snapshot_unavailable:{kind}: {detail}"),
        },
        L::AuthorityWidening { detail } => EmbedError::AuthorityViolation {
            layer: "policy".to_string(),
            detail,
        },
        L::CoverageInsufficient {
            required,
            available,
        } => EmbedError::Refused {
            reason: format!("coverage_insufficient: requires {required}, have {available}"),
        },
        L::PolicyForbids { detail } => EmbedError::Refused {
            reason: format!("policy_forbids: {detail}"),
        },
        L::Pinned { address, reason } => EmbedError::Refused {
            reason: format!("pinned: {address} ({reason})"),
        },
        other => EmbedError::Refused {
            reason: format!("ledger: {other:?}"),
        },
    }
}

// ── S5.8 unit tests — deprecation machinery (§7.4 rule 5; ADR-0178 D5) ──────
// No live op is deprecated in contract major 1, so these legs drive
// `note_deprecated_use` with a *fabricated* deprecated `OpSpec` — the same
// function the dispatch hook calls for a real one. The dispatch hook itself
// is a two-clause guard (`result.is_ok() && op.deprecated.is_some()`) checked
// by review; the durable-fold legs live in `hh-ledger/tests/s5_8.rs`.
#[cfg(test)]
mod tests {
    use super::*;
    use hh_embed_schema::ops::{registry, OpDeprecation};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn test_dir(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "hh-embed-s58-{}-{tag}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn service() -> EmbedService {
        let root = test_dir("svc");
        EmbedService::open(ServiceConfig {
            store_root: root.join("store"),
            kernel_version_id: "hh-kernel/0.1.0".into(),
            workspace_root: root.join("ws"),
            holder: "s58".into(),
        })
        .unwrap()
    }

    /// A deprecated `OpSpec` fabricated off a live registry row — the same
    /// members `deprecated{…}` would carry on a real declaration.
    fn deprecated_spec() -> hh_embed_schema::ops::OpSpec {
        let mut op = registry()
            .into_iter()
            .find(|o| o.name == "env.resume")
            .unwrap();
        op.deprecated = Some(OpDeprecation {
            since: "1.0",
            removal_major: 2,
            replacement: Some("read"),
        });
        op
    }

    fn req(params: Json) -> Request {
        Request {
            id: Json::str("t-dep"),
            method: "env.resume".into(),
            params,
        }
    }

    fn hello(svc: &mut EmbedService, name: &str, kind: &str) {
        let resp = svc.handle(&Request {
            id: Json::str("t-hello"),
            method: "hello".into(),
            params: Json::obj([
                ("contract_major", Json::Int(1)),
                (
                    "client",
                    Json::obj([
                        ("name", Json::str(name)),
                        ("version", Json::str("2.1")),
                        ("kind", Json::str(kind)),
                    ]),
                ),
                ("capabilities", Json::Obj(BTreeMap::new())),
            ]),
        });
        assert!(resp.get("result").is_some(), "hello refused: {resp:?}");
    }

    #[test]
    fn deprecated_use_sessionless_lands_on_contract_run() {
        let mut svc = service();
        // No `hello` — no negotiated client, no session. The use is still
        // accounted: the kernel mints onto the contract bookkeeping run
        // rather than dropping the count (CC3).
        svc.note_deprecated_use(&req(Json::obj([])), &deprecated_spec())
            .unwrap();
        let (crun, _) = svc.contract_run.as_ref().unwrap();
        let events = svc.store.events(crun).unwrap();
        let row = events
            .iter()
            .find(|e| e.class == "lifecycle.contract.deprecated_use")
            .expect("deprecated_use row durable");
        assert_eq!(row.payload.get("method"), Some(&Json::str("env.resume")));
        // Client unknown without a negotiated hello — folded as "unknown".
        assert_eq!(
            svc.store.deprecated_use_totals()[&("env.resume".into(), "unknown".into())],
            1
        );
        // A second use re-uses the bookkeeping run and increments.
        svc.note_deprecated_use(&req(Json::obj([])), &deprecated_spec())
            .unwrap();
        assert_eq!(
            svc.store.deprecated_use_totals()[&("env.resume".into(), "unknown".into())],
            2
        );
    }

    #[test]
    fn deprecated_use_records_hello_negotiated_client() {
        let mut svc = service();
        hello(&mut svc, "web-frontend", "web");
        svc.note_deprecated_use(&req(Json::obj([])), &deprecated_spec())
            .unwrap();
        // The count key is the kernel-recorded negotiated descriptor
        // (`name@version:kind`) — a call parameter can never author it (CC2).
        let totals = svc.store.deprecated_use_totals();
        assert_eq!(
            totals[&("env.resume".into(), "web-frontend@2.1:web".into())],
            1
        );
        assert_eq!(totals.len(), 1);
    }

    #[test]
    fn deprecated_use_panics_on_undeprecated_spec() {
        // The dispatch hook guards `op.deprecated.is_some()`; the mint
        // itself asserts the invariant rather than minting a lie.
        let mut svc = service();
        let op = registry()
            .into_iter()
            .find(|o| o.name == "env.resume")
            .unwrap();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            svc.note_deprecated_use(&req(Json::obj([])), &op)
        }));
        assert!(r.is_err());
    }

    #[test]
    fn stability_entry_emits_deprecation_member() {
        // The map entry and the schema export share `deprecated` member
        // form via `OpDeprecation::to_json` — assert both carriers here.
        let op = deprecated_spec();
        let entry = hh_embed_schema::ops::stability_entry(&op);
        let dep = entry.get("deprecated").expect("deprecated member");
        assert_eq!(dep.get("since"), Some(&Json::str("1.0")));
        assert_eq!(dep.get("removal_major"), Some(&Json::Int(2)));
        assert_eq!(dep.get("replacement"), Some(&Json::str("read")));
        // Live contract: nothing deprecated today — no stability entry and
        // no exported method carries the member (CC8 additive discipline).
        let map = hh_embed_schema::ops::stability_map();
        if let Json::Obj(m) = &map {
            assert!(m.values().all(|e| e.get("deprecated").is_none()));
        } else {
            panic!("stability_map is not an object");
        }
        let schema = hh_embed_schema::export_schema();
        let Json::Obj(methods) = schema.get("methods").unwrap() else {
            panic!("methods member is not an object");
        };
        for m in methods.values() {
            assert!(m.get("deprecated").is_none());
        }
    }
}
