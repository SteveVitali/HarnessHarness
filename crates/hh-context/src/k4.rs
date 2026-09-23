//! `R-2.3.4¹` — **K4**, the tool-result/retrieval cache as
//! `Memory{kind: tool_result}` (§5b.4; ADR-0129 d.1/d.4; ticket S2.10).
//!
//! K4 is a thin, honest layer over the C0 [`MemoryStore`]: every entry is an
//! ordinary `MemoryVersion` of kind `tool_result` whose invalidation contract
//! pins `{tool_capability_version, environment_epoch}` dependency stamps —
//! nothing about an entry is special to the store, so every C0 mechanism
//! (lifecycle, supersession, scope-ended expiry, leases, R-TEXT caps, the
//! `resolve(mode = execute)` filter) applies unmodified.
//!
//! ## The key and the epoch (§5b.4 K4 entry; IR-3)
//!
//! The canonical key is
//! `H(semantic_id ∥ version_id ∥ canonical(args) ∥ environment_stamp(environment_ref, mutation_epoch) ∥ readers)`
//! — recorded on the entry as `entry_key` ([`K4Key::entry_key`]). The *bound
//! name* ([`K4Key::member_name`]) deliberately omits the epoch: a post-mutation
//! lookup must still find the prior-epoch entry so it can be reported
//! **`stale_withheld`** (never silently `miss` — IR-3 requires the stale
//! outcome to be observable). The epoch itself is carried as the
//! `environment_epoch` dependency stamp: [`K4Cache::note_committed`] bumps
//! `stamps[environment_epoch:{environment_ref}]` on every committed effect in
//! `MUTATION_DOMAINS`, and `check_contract`'s stamp comparison expires every
//! entry pinned to the earlier epoch.
//!
//! ## Admissibility (ADR-0129 d.1)
//!
//! Only `effective_risk_class = {reversibility: read_only, repeat_safety:
//! idempotent}` with `cacheable = true` on the capability record qualifies;
//! `unknown` projects to the most dangerous class upstream and therefore
//! never qualifies. Non-qualifying calls are a `bypass`, not an error — the
//! dispatch pipeline executes them normally.
//!
//! ## Serve semantics (§5b.4 security & provenance)
//!
//! A hit is delivered as an ordinary observation carrying the *entry's*
//! label verbatim, `origin = cache(entry_ref)`, `derived_from = [entry_ref]`
//! ([`K4Cache::served_provenance`]) — authority is copied, never raised
//! (`Origin::Cache` mints at the `environment` ceiling so a forged record
//! above it fails `AuthorityExceedsOrigin`). `readers` is a key member, so an
//! entry written under `readers = {A}` can never hit for principal B
//! (AC-R-2.3.4-9), and the resolve additionally refuses an entry whose label
//! does not admit the query's reader set.
//!
//! Every lookup emits exactly one `model.cache.resolved` row
//! ([`K4Cache::resolved_payload`]) — hits, misses, bypasses, withhelds,
//! refusals (ADR-0128 d.3).

use hh_hir::kinds::EffectDomain;
use hh_identity::idp::idp_id;
use hh_identity::names::ResolveMode;
use hh_ontology::risk::{RepeatSafety, RiskClass, RiskReversibility};
use hh_provenance::authority::ReaderSet;
use hh_provenance::origin::Origin;
use hh_provenance::record::{Derivation, DerivationKind, ProvenanceRecord};
use hh_provenance::PersistenceScope;
use hh_wire::json::Json;

use crate::lifecycle::lifecycle_state;
use crate::memory::{
    DependencyStamp, InvalidationContract, MemoryDraft, MemoryError, MemoryStore, MemoryVersion,
    ResolveOutcome, WriteContext,
};
use crate::vocab::{
    CacheHint, DependencyKind, Granularity, InvalidationCondition, LifecycleStateKind,
    MemoryContent, MemoryKind, Revalidation,
};

/// `k4.key.1` — the `idp` domain K4 names and entry keys mint under.
pub const K4_KEY_IDP: &str = "k4.key.1";

/// `k4.tool_result/1` — the closed-schema ref the stored `Structured` content
/// spells (the entry body's own identity, distinct from the memory record's).
pub const K4_SCHEMA_REF: &str = "k4.tool_result/1";

/// `tool_result` — the `cache_kind` spelling in `model.cache.resolved`.
pub const K4_CACHE_KIND: &str = "tool_result";

/// IR-3's mutation domains: `mutation_epoch(environment_ref)` increments on
/// every committed effect with `domain ∈ {fs_write, exec, net_egress,
/// spawn_process}` (ADR-0129 d.4). The set is deliberately the spec's closed
/// list — conservative over-invalidation is the declared trade (the
/// `hit_rate[tool_result]` metric measures the benefit lost).
pub const MUTATION_DOMAINS: [EffectDomain; 4] = [
    EffectDomain::FsWrite,
    EffectDomain::Exec,
    EffectDomain::NetEgress,
    EffectDomain::SpawnProcess,
];

/// The `stamps` dep_ref an environment's mutation epoch lives under —
/// `{environment_epoch:{environment_ref}} → "{epoch}"` (decimal).
pub fn epoch_dep_ref(environment_ref: &str) -> String {
    format!("environment_epoch:{environment_ref}")
}

/// The `stamps` dep_ref a tool capability's current version lives under —
/// `{tool_capability:{semantic_id}} → "{version_id}"`. A capability version
/// change bumps this stamp and withholds every entry pinned to the old
/// version (IR-2).
pub fn capability_dep_ref(semantic_id: &str) -> String {
    format!("tool_capability:{semantic_id}")
}

/// The canonical `readers` member spelling inside the key preimage
/// (`Public` ⇒ `"public"`; a restricted set spells its sorted members).
fn readers_key(readers: &ReaderSet) -> String {
    match readers {
        ReaderSet::Public => "public".to_string(),
        ReaderSet::Restricted(s) => {
            format!(
                "restricted:{}",
                s.iter().cloned().collect::<Vec<_>>().join(",")
            )
        }
    }
}

/// `K4Key` — the lookup coordinate (§5b.4). `canonical_args_hash` is the
/// dispatcher's hash of the evaluated canonical arguments (the same value the
/// `action.tool.proposed` payload carries — one canonicalization, CC1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct K4Key {
    /// The capability's `semantic_id` (a rename alone is not a miss — T-LCD-10).
    pub capability_semantic_id: String,
    /// The capability's `version_id` (IR-2: a version change is a miss).
    pub capability_version_id: String,
    /// `H(canonical(args))` — the canonical-args content address.
    pub canonical_args_hash: String,
    /// The environment coordinate `"{semantic_id}:{version_id}"`.
    pub environment_ref: String,
    /// The calling context's readers — a key member (AC-R-2.3.4-9).
    pub readers: ReaderSet,
}

impl K4Key {
    /// The epoch-free key members — the `member_name` preimage.
    fn base_json(&self) -> Json {
        Json::obj([
            (
                "capability_semantic_id",
                Json::str(self.capability_semantic_id.clone()),
            ),
            (
                "capability_version_id",
                Json::str(self.capability_version_id.clone()),
            ),
            (
                "canonical_args",
                Json::str(self.canonical_args_hash.clone()),
            ),
            ("environment_ref", Json::str(self.environment_ref.clone())),
            ("readers", Json::str(readers_key(&self.readers))),
        ])
    }

    /// The bound name — `H(semantic_id ∥ version_id ∥ canonical(args) ∥
    /// environment_ref ∥ readers)`, **without** the epoch. A lookup after a
    /// mutation still resolves this name so the stale head is found and
    /// reported `stale_withheld` rather than silently `miss`ed (IR-3).
    pub fn member_name(&self) -> String {
        format!(
            "k4:{}",
            idp_id(
                K4_KEY_IDP,
                self.base_json().to_canonical_string().as_bytes()
            )
        )
    }

    /// The recorded `key` — `H(∥ … ∥ environment_stamp(environment_ref,
    /// mutation_epoch))`, the canonical K4 key spelled on the entry for
    /// audit (§5b.4). Distinct epochs mint distinct keys under one name.
    pub fn entry_key(&self, mutation_epoch: u64) -> String {
        let mut members = match self.base_json() {
            Json::Obj(m) => m,
            _ => unreachable!("base_json is always an object"),
        };
        members.insert(
            "environment_stamp".to_string(),
            Json::str(format!("{}:{}", self.environment_ref, mutation_epoch)),
        );
        format!(
            "k4:{}",
            idp_id(
                K4_KEY_IDP,
                Json::Obj(members).to_canonical_string().as_bytes()
            )
        )
    }
}

/// `K4Entry` — the write-side payload: what a successful, admissible tool
/// call deposits (§5b.4 `K4 entry`). `label` is the observed result's full
/// label; the store's `join(context_label, writer_label)` may only narrow it.
#[derive(Debug, Clone)]
pub struct K4Entry {
    /// `value` — the content address of the captured result (the
    /// `EffectCaptureManifest`/blob ref the fresh observation would carry).
    pub value_ref: String,
    /// The result payload served back verbatim on a hit.
    pub value: Json,
    /// The producing call's provenance (`Origin::Tool{…}` — the *write's*
    /// origin; a served hit re-origins to `cache(entry_ref)`).
    pub provenance: ProvenanceRecord,
    /// `recorded_usage` — the usage the recorded call posted; the hit's
    /// `avoided` estimate is derived from it under the current pricing table.
    pub recorded_usage: Option<Json>,
    /// An optional validator-gated external-resource dependency
    /// (`{external_resource{validator_ref}}` — §5b.4 contract member 3).
    pub external_dep: Option<(DependencyStamp, String)>,
    /// An optional absolute TTL — `validity.until` on the stored version.
    pub valid_until: Option<u64>,
}

/// `NotCacheable` — the admissibility refusal (ADR-0129 d.1). A bypass, never
/// an error: the dispatch executes the call normally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotCacheable {
    /// `effective_risk_class` is not `{read_only, idempotent}` — `unknown`
    /// never qualifies (it projects to the most dangerous class upstream).
    RiskClass {
        /// The effective reversibility that failed `read_only`.
        reversibility: &'static str,
        /// The effective repeat safety that failed `idempotent`.
        repeat_safety: &'static str,
    },
    /// The capability record does not declare `cacheable = true`
    /// (the default is false — opt-in only).
    CapabilityOptOut,
}

impl NotCacheable {
    /// The `reason` spelling for `model.cache.resolved{outcome: bypass}`.
    pub fn reason(&self) -> String {
        match self {
            NotCacheable::RiskClass {
                reversibility,
                repeat_safety,
            } => format!("not_cacheable:risk_class={{{reversibility},{repeat_safety}}}"),
            NotCacheable::CapabilityOptOut => "not_cacheable:capability_opt_out".to_string(),
        }
    }
}

/// `K4Error` — the write-side failures (the lookup path is total — every
/// outcome is a [`K4Resolution`]).
#[derive(Debug, Clone, PartialEq)]
pub enum K4Error {
    /// The call is not cache-admissible.
    NotCacheable(NotCacheable),
    /// The capability stamp already names a different version — the write
    /// would be born `stale_withheld`; refused loudly instead (IR-2).
    CapabilityVersionDrift {
        /// The stamp ref.
        dep_ref: String,
        /// The version the write pins.
        pinned: String,
        /// The version the stamp currently names.
        current: String,
    },
    /// The store refused the write (`put`/`bind`).
    Store(MemoryError),
}

/// `K4Resolution` — the lookup outcome; maps 1:1 onto
/// `model.cache.resolved.outcome ∈ {hit, miss, bypass, stale_withheld,
/// refused}` (the `bypass` outcome is produced by the admissibility gate —
/// the caller emits it without consulting the store).
#[derive(Debug, Clone)]
pub enum K4Resolution {
    /// A `valid` head whose label admits the query's readers.
    Hit {
        /// The bound name.
        name: String,
        /// The served version (the entry — label, contract, content).
        version: Box<MemoryVersion>,
    },
    /// No entry is bound under this name.
    Miss {
        /// The bound name.
        name: String,
    },
    /// A head exists but is not `valid` — epoch drift, supersession,
    /// revocation, expiry, or a transitive stale dependency (IR-2/IR-3;
    /// `resolve(mode = execute)` never serves it and K4 admits `valid`
    /// only — `stale_by_dependency`/`unknown` heads are withheld too).
    StaleWithheld {
        /// The bound name.
        name: String,
        /// The withheld head.
        version_id: String,
        /// The lifecycle state kind (`expired`, `stale_by_dependency`, …).
        reason: String,
    },
    /// The entry's label does not admit the query's readers — the key
    /// matched but the label gate refused (defense in depth under AC-9).
    Refused {
        /// The bound name.
        name: String,
        /// Why it refused.
        reason: String,
    },
}

impl K4Resolution {
    /// The `outcome` spelling for `model.cache.resolved`.
    pub fn outcome(&self) -> &'static str {
        match self {
            K4Resolution::Hit { .. } => "hit",
            K4Resolution::Miss { .. } => "miss",
            K4Resolution::StaleWithheld { .. } => "stale_withheld",
            K4Resolution::Refused { .. } => "refused",
        }
    }

    /// The `reason` member (`hit` carries none — `reason` = `ok`).
    pub fn reason(&self) -> String {
        match self {
            K4Resolution::Hit { .. } => "ok".to_string(),
            K4Resolution::Miss { .. } => "no_entry".to_string(),
            K4Resolution::StaleWithheld { reason, .. } => reason.clone(),
            K4Resolution::Refused { reason, .. } => reason.clone(),
        }
    }

    /// The `entry_ref` member — the served/withheld version id, when a head
    /// was found.
    pub fn entry_ref(&self) -> Option<String> {
        match self {
            K4Resolution::Hit { version, .. } => Some(version.version_id.clone()),
            K4Resolution::StaleWithheld { version_id, .. } => Some(version_id.clone()),
            _ => None,
        }
    }
}

/// The decoded content of a stored entry — [`K4Cache::hit_entry`]'s view
/// back out of `MemoryContent::Structured`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct K4EntryView {
    /// The recorded canonical key (epoch-bound).
    pub key: String,
    /// `value` — the cached result's content address.
    pub value_ref: String,
    /// The cached payload.
    pub value: Json,
    /// `recorded_usage` — the `avoided` estimate's input.
    pub recorded_usage: Option<Json>,
}

/// `K4Write` — what a successful deposit returns.
#[derive(Debug, Clone)]
pub struct K4Write {
    /// The bound name (the lookup coordinate).
    pub name: String,
    /// The recorded canonical key.
    pub entry_key: String,
    /// The minted version (the `entry_ref` future hits serve).
    pub version_id: String,
    /// The epoch the entry pinned.
    pub epoch: u64,
    /// The `context.memory.written` payload row the caller appends.
    pub memory_event: (String, Json),
}

/// `K4Cache` — the K4 slice over a `MemoryStore` (§5b.4; S2.10). Owns no
/// second store: `store` is the run's ordinary memory store and the epoch
/// counter lives in its `stamps` map (the same `check_contract` environment
/// every other dependency stamp reads — CC1, no parallel invalidation
/// mechanism).
pub struct K4Cache {
    /// The backing memory store.
    store: MemoryStore,
    /// The entry scope — `turn | run | session` (§5b.4 `scope` member).
    /// `run` is the default: an entry outlives its turn but never its run
    /// unless the operator declares `session` (and the label admits it).
    scope: PersistenceScope,
}

/// `cacheable` on the E1 capability record (§5b.4; ADR-0129 d.1 — default
/// `false`). The flag lives in the §5b-owned `observation_contract` slot
/// (`observation_contract.cacheable: true`); absent or non-boolean reads as
/// `false` — never coerced (T-LCD-07).
pub fn declared_cacheable(observation_contract: &Json) -> bool {
    matches!(
        observation_contract.get("cacheable"),
        Some(Json::Bool(true))
    )
}

impl K4Cache {
    /// Wrap a store — `run`-scoped entries by default.
    pub fn new(store: MemoryStore) -> K4Cache {
        K4Cache {
            store,
            scope: PersistenceScope::Run,
        }
    }

    /// The entry scope lookups/writes bind under.
    pub fn scope(&self) -> PersistenceScope {
        self.scope
    }

    /// Set the entry scope (`turn | run | session` — `session` entries
    /// survive `mark_scope_ended(Run)` only when the label admits it, IR-3).
    pub fn set_scope(&mut self, scope: PersistenceScope) {
        self.scope = scope;
    }

    /// The backing store (the ordinary memory ops still apply — a K4 entry
    /// is an ordinary `MemoryVersion`).
    pub fn store(&self) -> &MemoryStore {
        &self.store
    }

    /// The backing store, mutably (lease take, `mark_scope_ended`, …).
    pub fn store_mut(&mut self) -> &mut MemoryStore {
        &mut self.store
    }

    /// Unwrap the store.
    pub fn into_store(self) -> MemoryStore {
        self.store
    }

    /// `mutation_epoch(environment_ref)` — the current epoch (0 until the
    /// first recorded mutation).
    pub fn current_epoch(&self, environment_ref: &str) -> u64 {
        self.store
            .stamp_of(&epoch_dep_ref(environment_ref))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
    }

    /// Pin an epoch explicitly (attach-time — an environment opened onto
    /// already-mutated state may start above 0; honest, never inferred).
    pub fn set_epoch(&mut self, environment_ref: &str, epoch: u64) {
        self.store
            .set_stamp(&epoch_dep_ref(environment_ref), &epoch.to_string());
    }

    /// IR-3 — record a committed effect: `domain ∈ {fs_write, exec,
    /// net_egress, spawn_process}` bumps `mutation_epoch(environment_ref)`.
    /// Returns `Some(new_epoch)` when the epoch moved. Every entry pinned to
    /// the earlier epoch becomes `expired{dependency_changed}` on the next
    /// `check_contract` — the `stale_withheld` mechanism, with nothing
    /// deleted and nothing cleared (the reason is a ledger fact).
    pub fn note_committed(&mut self, environment_ref: &str, domain: EffectDomain) -> Option<u64> {
        if !MUTATION_DOMAINS.contains(&domain) {
            return None;
        }
        let next = self.current_epoch(environment_ref) + 1;
        self.set_epoch(environment_ref, next);
        Some(next)
    }

    /// The admissibility gate (ADR-0129 d.1): `effective_risk_class =
    /// {reversibility: read_only, repeat_safety: idempotent}` **and**
    /// `cacheable = true` on the E1 capability record. Unknown risk arrives
    /// as `RiskClass::UNKNOWN` — `{irreversible, non_idempotent, external}` —
    /// and fails the first conjunct, so unknown never qualifies.
    pub fn admissible(risk: &RiskClass, cacheable: bool) -> Result<(), NotCacheable> {
        if risk.reversibility != RiskReversibility::ReadOnly
            || risk.repeat_safety != RepeatSafety::Idempotent
        {
            return Err(NotCacheable::RiskClass {
                reversibility: risk.reversibility.name(),
                repeat_safety: risk.repeat_safety.name(),
            });
        }
        if !cacheable {
            return Err(NotCacheable::CapabilityOptOut);
        }
        Ok(())
    }

    /// `lookup(key, scope)` — name-resolve then strict-validate (§5b.4
    /// "validity before authority"). `resolve(mode = execute)` already
    /// withholds revoked/superseded/expired heads; K4 additionally withholds
    /// `stale_by_dependency` and `unknown` heads — a cache entry is served
    /// only when its lifecycle is `valid` (a cache is not the place for
    /// "servable-but-suspect" memory semantics).
    pub fn resolve(&self, key: &K4Key, scope: PersistenceScope) -> K4Resolution {
        let name = key.member_name();
        match self
            .store
            .resolve(scope, Some(&name), None, ResolveMode::Execute)
        {
            Err(MemoryError::UnknownName { .. }) => K4Resolution::Miss { name },
            Err(e) => K4Resolution::Refused {
                name,
                reason: format!("store_error:{e:?}"),
            },
            Ok(ResolveOutcome::Unservable { version_id, state }) => K4Resolution::StaleWithheld {
                name,
                version_id,
                reason: state.as_str().to_string(),
            },
            Ok(ResolveOutcome::Live { version }) => {
                let state =
                    lifecycle_state(&self.store, &version.version_id, self.store.applied_seq());
                match state.kind() {
                    LifecycleStateKind::Valid => {
                        if version.label.readers.is_superset_of(&key.readers) {
                            K4Resolution::Hit {
                                name,
                                version: Box::new(version),
                            }
                        } else {
                            K4Resolution::Refused {
                                name,
                                reason: "readers_not_admitted".to_string(),
                            }
                        }
                    }
                    other => K4Resolution::StaleWithheld {
                        name,
                        version_id: version.version_id.clone(),
                        reason: other.as_str().to_string(),
                    },
                }
            }
            Ok(ResolveOutcome::Annotated { version, state }) => {
                // Execute mode never yields Annotated — unreachable, but a
                // cache never serves a record it cannot fully explain.
                K4Resolution::StaleWithheld {
                    name,
                    version_id: version.version_id,
                    reason: state.as_str().to_string(),
                }
            }
        }
    }

    /// Decode a hit's stored entry (`MemoryContent::Structured` → the K4
    /// fields). `None` when the version is not a well-formed K4 entry —
    /// callers treat it as `refused{malformed_entry}` (never served raw).
    pub fn hit_entry(version: &MemoryVersion) -> Option<K4EntryView> {
        let MemoryContent::Structured(j) = &version.content else {
            return None;
        };
        Some(K4EntryView {
            key: j.get("key")?.as_str()?.to_string(),
            value_ref: j.get("value_ref")?.as_str()?.to_string(),
            value: j.get("value")?.clone(),
            recorded_usage: j.get("recorded_usage").cloned(),
        })
    }

    /// The served hit's provenance (§5b.4): `origin = cache(entry_ref)`,
    /// `derived_from = [entry_ref]`, authority/taint/readers **copied** from
    /// the entry's label — the copy can only carry what the entry holds, and
    /// `Origin::Cache`'s mint ceiling (`environment`) makes a forged
    /// above-ceiling record fail `AuthorityExceedsOrigin` downstream
    /// (I-NOAUTH's cache face).
    pub fn served_provenance(version: &MemoryVersion, at: u64) -> ProvenanceRecord {
        ProvenanceRecord {
            origin: Origin::cache(version.version_id.clone()),
            authority: version.label.authority,
            taint: version.label.taint.clone(),
            readers: version.label.readers.clone(),
            scope: version.scope,
            derived_from: vec![Derivation {
                kind: DerivationKind::Projection,
                inputs: vec![version.version_id.clone()],
                deriver: Origin::kernel("k4:tool_result_cache"),
                deterministic: true,
            }],
            created_at: at,
            attestation: None,
        }
    }

    /// `write(key, entry, scope, ctx)` — the deposit at
    /// `action.tool.completed` (§5b.4: "written by the kernel … when
    /// admissible"). Steps: read the epoch → pin the contract stamps → `put`
    /// the `Memory{kind: tool_result}` → `bind` the name (superseding the
    /// prior binding, so a re-execution's write never leaves two live heads).
    ///
    /// The environment-epoch stamp is initialised on first write (an
    /// environment with no recorded mutation sits at epoch 0). The
    /// capability stamp is initialised likewise, but a **mismatched**
    /// existing stamp is `CapabilityVersionDrift` — the write would be born
    /// withheld and a loud refusal beats a silent dead entry (IR-2).
    pub fn write(
        &mut self,
        key: &K4Key,
        entry: &K4Entry,
        scope: PersistenceScope,
        ctx: &WriteContext,
    ) -> Result<K4Write, K4Error> {
        let epoch_dep = epoch_dep_ref(&key.environment_ref);
        let cap_dep = capability_dep_ref(&key.capability_semantic_id);
        let epoch = self.current_epoch(&key.environment_ref);
        // Initial pins — unset stamps would make every entry born
        // `expired{dependency_changed}` (check_contract fires on `None`).
        if self.store.stamp_of(&epoch_dep).is_none() {
            self.store.set_stamp(&epoch_dep, &epoch.to_string());
        }
        match self.store.stamp_of(&cap_dep) {
            None => self.store.set_stamp(&cap_dep, &key.capability_version_id),
            Some(cur) if cur != key.capability_version_id => {
                return Err(K4Error::CapabilityVersionDrift {
                    dep_ref: cap_dep,
                    pinned: key.capability_version_id.clone(),
                    current: cur,
                });
            }
            Some(_) => {}
        }
        let entry_key = key.entry_key(epoch);
        let mut dependencies = vec![
            DependencyStamp {
                kind: DependencyKind::ToolCapabilityVersion,
                ref_: cap_dep,
                stamp: key.capability_version_id.clone(),
                granularity: Granularity::Row,
            },
            DependencyStamp {
                kind: DependencyKind::EnvironmentEpoch,
                ref_: epoch_dep,
                stamp: epoch.to_string(),
                granularity: Granularity::Row,
            },
        ];
        let mut validator_ref = None;
        if let Some((dep, vr)) = &entry.external_dep {
            dependencies.push(dep.clone());
            validator_ref = Some(vr.clone());
        }
        let contract = InvalidationContract {
            dependencies,
            cache_hint: CacheHint::Cacheable,
            validator_ref,
            freshness: entry.valid_until.map(crate::memory::Freshness::ValidUntil),
            invalidation_condition: Some(InvalidationCondition::DependencyChanged),
            revalidation: Revalidation::MustRevalidate,
        };
        let content = MemoryContent::Structured(Json::obj([
            ("schema", Json::str(K4_SCHEMA_REF)),
            ("key", Json::str(entry_key.clone())),
            ("value_ref", Json::str(entry.value_ref.clone())),
            ("value", entry.value.clone()),
            (
                "recorded_usage",
                entry.recorded_usage.clone().unwrap_or(Json::Null),
            ),
        ]));
        let draft = MemoryDraft {
            kind: MemoryKind::ToolResult,
            subject_key: None,
            content,
            contract: Some(contract),
            scope,
            declared_inputs: Vec::new(),
            justifications: Vec::new(),
            supersedes: None,
            validity: entry.valid_until.map(|until| hh_hir::records::Validity {
                from: ctx.at_seq,
                until: Some(until),
                condition: None,
            }),
            provenance: Some(entry.provenance.clone()),
            // One supersession line per member set — a re-execution's write
            // joins the same line.
            semantic_id: Some(key.member_name()),
            validator_endorsed: false,
        };
        let name = key.member_name();
        let prior = self.store.name_lookup(scope, &name).map(str::to_string);
        let outcome = self.store.put(draft, ctx).map_err(K4Error::Store)?;
        self.store
            .bind(
                scope,
                &name,
                &outcome.version.version_id,
                prior.as_deref(),
                "k4_write",
                ctx.at_seq,
            )
            .map_err(K4Error::Store)?;
        Ok(K4Write {
            name,
            entry_key,
            version_id: outcome.version.version_id.clone(),
            epoch,
            memory_event: outcome.event,
        })
    }

    /// The `model.cache.resolved` payload row (§5b.4 schema; ADR-0128 d.3):
    /// `{cache_kind, key, scope, outcome, reason, entry_ref?, avoided?,
    /// attribution}` — one per lookup, hits and misses alike. `bypass` is
    /// emitted by the caller with the `NotCacheable` reason (the store is
    /// never consulted on a bypass).
    pub fn resolved_payload(
        key: &K4Key,
        scope: PersistenceScope,
        outcome: &str,
        reason: &str,
        entry_ref: Option<&str>,
        avoided: Option<Json>,
        attribution: Json,
    ) -> Json {
        let mut members = match Json::obj([
            ("cache_kind", Json::str(K4_CACHE_KIND)),
            ("key", Json::str(key.member_name())),
            ("scope", Json::str(scope.as_str())),
            ("outcome", Json::str(outcome)),
            ("reason", Json::str(reason)),
        ]) {
            Json::Obj(m) => m,
            _ => unreachable!("Json::obj always builds an object"),
        };
        if let Some(er) = entry_ref {
            members.insert("entry_ref".to_string(), Json::str(er));
        }
        members.insert("avoided".to_string(), avoided.unwrap_or(Json::Null));
        members.insert("attribution".to_string(), attribution);
        Json::Obj(members)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    use hh_provenance::authority::{AuthorityClass, PersistenceScope};
    use hh_provenance::label::Label;

    fn ctx_label() -> Label {
        Label::at(AuthorityClass::Principal)
    }

    fn write_ctx(at: u64) -> WriteContext {
        WriteContext {
            context_label: ctx_label(),
            lease_generation: 0,
            at_seq: at,
            run_id: "run:test".to_string(),
        }
    }

    fn tool_provenance(readers: ReaderSet) -> ProvenanceRecord {
        ProvenanceRecord {
            origin: Origin::tool("tool:read_file", "inv:1"),
            authority: AuthorityClass::External,
            taint: BTreeSet::new(),
            readers,
            scope: PersistenceScope::Run,
            derived_from: Vec::new(),
            created_at: 1,
            attestation: None,
        }
    }

    fn key(readers: ReaderSet) -> K4Key {
        K4Key {
            capability_semantic_id: "tool:read_file".to_string(),
            capability_version_id: "capv:1".to_string(),
            canonical_args_hash: "argshash:abc".to_string(),
            environment_ref: "env:main:v1".to_string(),
            readers,
        }
    }

    fn entry(readers: ReaderSet) -> K4Entry {
        K4Entry {
            value_ref: "ecm:result1".to_string(),
            value: Json::str("file contents"),
            provenance: tool_provenance(readers),
            recorded_usage: Some(Json::obj([("tool_calls", Json::Int(1))])),
            external_dep: None,
            valid_until: None,
        }
    }

    fn cache() -> K4Cache {
        K4Cache::new(MemoryStore::new("mem:test"))
    }

    #[test]
    fn miss_then_write_then_hit() {
        let mut c = cache();
        let k = key(ReaderSet::Public);
        assert!(matches!(
            c.resolve(&k, PersistenceScope::Run),
            K4Resolution::Miss { .. }
        ));
        let w = c
            .write(
                &k,
                &entry(ReaderSet::Public),
                PersistenceScope::Run,
                &write_ctx(10),
            )
            .unwrap();
        match c.resolve(&k, PersistenceScope::Run) {
            K4Resolution::Hit { version, .. } => {
                assert_eq!(version.version_id, w.version_id);
                assert_eq!(version.kind, MemoryKind::ToolResult);
                let v = K4Cache::hit_entry(&version).unwrap();
                assert_eq!(v.key, w.entry_key);
                assert_eq!(v.value_ref, "ecm:result1");
                assert_eq!(v.value, Json::str("file contents"));
            }
            other => panic!("expected hit, got {}", other.outcome()),
        }
    }

    #[test]
    fn admissibility_gate() {
        let ro = RiskClass::READ_ONLY;
        assert!(K4Cache::admissible(&ro, true).is_ok());
        assert_eq!(
            K4Cache::admissible(&ro, false),
            Err(NotCacheable::CapabilityOptOut)
        );
        // Unknown risk never qualifies.
        assert!(matches!(
            K4Cache::admissible(&RiskClass::UNKNOWN, true),
            Err(NotCacheable::RiskClass { .. })
        ));
        // Non-idempotent read-only fails too.
        let risky = RiskClass {
            reversibility: RiskReversibility::ReadOnly,
            repeat_safety: RepeatSafety::NonIdempotent,
            scope: hh_ontology::risk::RiskScope::WorkspaceLocal,
        };
        assert!(matches!(
            K4Cache::admissible(&risky, true),
            Err(NotCacheable::RiskClass { .. })
        ));
    }

    #[test]
    fn mutation_epoch_withholds_prior_entries() {
        let mut c = cache();
        let k = key(ReaderSet::Public);
        c.write(
            &k,
            &entry(ReaderSet::Public),
            PersistenceScope::Run,
            &write_ctx(10),
        )
        .unwrap();
        assert!(matches!(
            c.resolve(&k, PersistenceScope::Run),
            K4Resolution::Hit { .. }
        ));
        // A committed fs_write bumps the epoch → the entry is stale_withheld.
        assert_eq!(
            c.note_committed("env:main:v1", EffectDomain::FsWrite),
            Some(1)
        );
        match c.resolve(&k, PersistenceScope::Run) {
            K4Resolution::StaleWithheld { reason, .. } => {
                assert_eq!(reason, "expired");
            }
            other => panic!("expected stale_withheld, got {}", other.outcome()),
        }
        // Non-mutating domains do not bump.
        assert_eq!(c.note_committed("env:main:v1", EffectDomain::FsRead), None);
        // A fresh write at the new epoch hits again.
        c.write(
            &k,
            &entry(ReaderSet::Public),
            PersistenceScope::Run,
            &write_ctx(20),
        )
        .unwrap();
        assert!(matches!(
            c.resolve(&k, PersistenceScope::Run),
            K4Resolution::Hit { .. }
        ));
    }

    #[test]
    fn readers_are_a_key_member() {
        let mut c = cache();
        let a = BTreeSet::from(["principal:a".to_string()]);
        let b = BTreeSet::from(["principal:b".to_string()]);
        let ka = key(ReaderSet::Restricted(a.clone()));
        let kb = key(ReaderSet::Restricted(b));
        c.write(
            &ka,
            &entry(ReaderSet::Restricted(a)),
            PersistenceScope::Run,
            &write_ctx(10),
        )
        .unwrap();
        // AC-R-2.3.4-9: an entry written under readers={A} never hits for B.
        assert!(matches!(
            c.resolve(&kb, PersistenceScope::Run),
            K4Resolution::Miss { .. }
        ));
        assert!(matches!(
            c.resolve(&ka, PersistenceScope::Run),
            K4Resolution::Hit { .. }
        ));
    }

    #[test]
    fn served_provenance_is_cache_origined_and_never_raised() {
        let mut c = cache();
        let k = key(ReaderSet::Public);
        let w = c
            .write(
                &k,
                &entry(ReaderSet::Public),
                PersistenceScope::Run,
                &write_ctx(10),
            )
            .unwrap();
        let K4Resolution::Hit { version, .. } = c.resolve(&k, PersistenceScope::Run) else {
            panic!("expected hit")
        };
        let prov = K4Cache::served_provenance(&version, 42);
        assert_eq!(
            prov.origin,
            Origin::Cache {
                entry_ref: w.version_id.clone()
            }
        );
        assert_eq!(prov.derived_from[0].inputs, vec![w.version_id.clone()]);
        // authority copied from the entry — never raised.
        assert_eq!(prov.authority, version.label.authority);
        assert!(prov.authority <= AuthorityClass::Environment);
    }

    #[test]
    fn capability_version_is_a_key_member_and_stamp() {
        let mut c = cache();
        let mut k1 = key(ReaderSet::Public);
        c.write(
            &k1,
            &entry(ReaderSet::Public),
            PersistenceScope::Run,
            &write_ctx(10),
        )
        .unwrap();
        // Same everything, bumped version → miss (IR-2).
        k1.capability_version_id = "capv:2".to_string();
        assert!(matches!(
            c.resolve(&k1, PersistenceScope::Run),
            K4Resolution::Miss { .. }
        ));
        // Writing under the new version while the stamp names the old is a
        // loud drift refusal, not a born-stale entry.
        assert!(matches!(
            c.write(
                &k1,
                &entry(ReaderSet::Public),
                PersistenceScope::Run,
                &write_ctx(20)
            ),
            Err(K4Error::CapabilityVersionDrift { .. })
        ));
    }

    #[test]
    fn resolved_payload_shape() {
        let k = key(ReaderSet::Public);
        let p = K4Cache::resolved_payload(
            &k,
            PersistenceScope::Run,
            "hit",
            "ok",
            Some("mem:v1"),
            Some(Json::obj([("tool_calls", Json::Int(1))])),
            Json::obj([("run_ref", Json::str("run:test"))]),
        );
        assert_eq!(
            p.get("cache_kind").and_then(|x| x.as_str()),
            Some("tool_result")
        );
        assert_eq!(p.get("outcome").and_then(|x| x.as_str()), Some("hit"));
        assert_eq!(p.get("entry_ref").and_then(|x| x.as_str()), Some("mem:v1"));
    }
}
