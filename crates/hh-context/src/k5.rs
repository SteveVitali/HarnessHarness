//! `R-2.3.4` — **K5**, the exact-match response cache (§5b.4; ADR-0129 d.2;
//! ticket S3.7).
//!
//! K5 caches a *served* model response: `value = canonical ModelMessage +
//! usage + timing`, keyed by
//! `H(plan_hash ∥ model_ref{provider_model_id, served_model?, snapshot_id?} ∥
//! replicate ∥ configuration_version_id)`. `AttemptPolicy`, deadlines,
//! observability flags and `cache_state_hint` never enter the key; a new plan
//! field defaults to *in* it (the `plan_hash` already covers the rendered
//! `ProviderRequestPlan` — one canonicalization, CC1).
//!
//! ## Admissibility (ADR-0129 d.2; AC-R-2.3.4-11)
//!
//! K5 is admissible only under `reproduce(bundle, level ≥ R1)`, the
//! recording/fake gateway (whose corpus *is* a K5 cache), or a declared
//! `deterministic_replay` variant. Enabling it in a non-reproduce arm without
//! declaring it as a factor fails `PreRegistration` validation —
//! [`admit`] returns `PreRegistrationInvalid`, never a warning.
//!
//! ## Serve semantics
//!
//! A hit stamps the terminal `model.call.completed` with
//! `served_from_cache: entry_ref` and `timing = n/a{not_run}` —
//! [`K5Cache::served_terminal_stamp`]. The stored `timing` is the entry's
//! recorded timing, never re-read as the serve's.
//!
//! ## Invalidation (IR-2/IR-4; AC-R-2.3.4-7)
//!
//! Every entry carries the contract dependencies `{model_snapshot,
//! profile_version, definition_version}` with `revalidation = never`. A
//! `RevocationRecord` over one of those dep_refs (e.g. the profile version)
//! withholds matching entries: `resolve(mode = execute)` reports
//! `stale_withheld` — never a silent `miss` — and `mode = audit` returns the
//! entry annotated. A served-model drift or fingerprint DRIFT
//! ([`K5Cache::note_drift`]) marks the snapshot's entries
//! `stale_by_dependency`; `mode = reproduce` may request them explicitly and
//! receives them annotated.
//!
//! Every lookup emits exactly one `model.cache.resolved` row
//! ([`K5Cache::resolved_payload`]) — hits, misses, withhelds, annotated
//! serves (ADR-0128 d.3: nothing unaccounted).

use std::collections::BTreeMap;

use hh_identity::idp::idp_id;
use hh_identity::names::ResolveMode;
use hh_wire::json::Json;

use crate::memory::DependencyStamp;
use crate::vocab::{DependencyKind, Granularity};

/// `k5.key.1` — the `idp` domain K5 entry keys mint under.
pub const K5_KEY_IDP: &str = "k5.key.1";

/// `response` — the `cache_kind` spelling in `model.cache.resolved`.
pub const K5_CACHE_KIND: &str = "response";

/// `k5.response/1` — the closed-schema ref a stored entry body spells.
pub const K5_SCHEMA_REF: &str = "k5.response/1";

/// The dep_ref a model snapshot dependency lives under —
/// `{model_snapshot:{snapshot_id}} → "{snapshot record ref}"` (IR-4).
pub fn snapshot_dep_ref(snapshot_id: &str) -> String {
    format!("model_snapshot:{snapshot_id}")
}

/// The dep_ref a profile version dependency lives under —
/// `{profile_version:{profile_ref}} → "{profile_version_id}"`. Revoking the
/// profile version withholds every entry pinned to it (AC-R-2.3.4-7).
pub fn profile_dep_ref(profile_ref: &str) -> String {
    format!("profile_version:{profile_ref}")
}

/// The dep_ref a definition version dependency lives under —
/// `{definition_version:{version_id}} → "{version_id}"` (IR-2).
pub fn definition_dep_ref(version_id: &str) -> String {
    format!("definition_version:{version_id}")
}

/// `K5Key` — the lookup coordinate (§5b.4 K5 entry). The members are exactly
/// the spec's closed list; `None` members spell `"none"` in the preimage so an
/// absent `served_model`/`snapshot_id` can never collide with a present one.
#[derive(Debug, Clone, PartialEq)]
pub struct K5Key {
    /// The `ProviderRequestPlan`'s content address (covers every plan field —
    /// a new plan field defaults to *in* the key, ADR-0129 d.2).
    pub plan_hash: String,
    /// The requested model's `provider_model_id`.
    pub provider_model_id: String,
    /// The served model id when the provider substituted (None ⇒ `"none"`).
    pub served_model: Option<String>,
    /// The pinned `ModelSnapshotRecord` id (None ⇒ `"none"`).
    pub snapshot_id: Option<String>,
    /// The replicate index — cached responses never fake replicate variance.
    pub replicate: u64,
    /// The run configuration's version id.
    pub configuration_version_id: String,
}

impl K5Key {
    /// The canonical preimage: `plan_hash ∥ provider_model_id ∥ served_model
    /// ∥ snapshot_id ∥ replicate ∥ configuration_version_id`.
    pub fn preimage(&self) -> String {
        [
            self.plan_hash.clone(),
            self.provider_model_id.clone(),
            self.served_model
                .clone()
                .unwrap_or_else(|| "none".to_string()),
            self.snapshot_id
                .clone()
                .unwrap_or_else(|| "none".to_string()),
            self.replicate.to_string(),
            self.configuration_version_id.clone(),
        ]
        .join("∥")
    }

    /// `entry_key` — the content-addressed lookup coordinate (`idp/1`).
    pub fn entry_key(&self) -> String {
        idp_id(K5_KEY_IDP, self.preimage().as_bytes())
    }
}

/// The three contract dependencies every K5 entry carries (§5b.4):
/// `{model_snapshot}, {profile_version}, {definition_version}` — `row`
/// granularity, `revalidation = never` (declared on
/// [`K5Entry::contract_revalidation`], which is a constant).
pub fn k5_dependencies(
    snapshot_id: &str,
    snapshot_ref: &str,
    profile_ref: &str,
    profile_version_id: &str,
    definition_version_id: &str,
) -> Vec<DependencyStamp> {
    vec![
        DependencyStamp {
            kind: DependencyKind::ModelSnapshot,
            ref_: snapshot_dep_ref(snapshot_id),
            stamp: snapshot_ref.to_string(),
            granularity: Granularity::Row,
        },
        DependencyStamp {
            kind: DependencyKind::ProfileVersion,
            ref_: profile_dep_ref(profile_ref),
            stamp: profile_version_id.to_string(),
            granularity: Granularity::Row,
        },
        DependencyStamp {
            kind: DependencyKind::DefinitionVersion,
            ref_: definition_dep_ref(definition_version_id),
            stamp: definition_version_id.to_string(),
            granularity: Granularity::Row,
        },
    ]
}

/// A stored K5 entry — `value = canonical ModelMessage + usage + timing`
/// plus its invalidation contract (§5b.4). `message`/`usage`/`timing` are the
/// *recorded* canonical documents (CC1): a served hit replays them verbatim
/// and stamps `timing = n/a{not_run}` on the terminal event — the entry's
/// timing is never re-read as the serve's.
#[derive(Debug, Clone, PartialEq)]
pub struct K5Entry {
    /// The content-addressed entry ref (`key.entry_key()`).
    pub entry_ref: String,
    /// The lookup coordinate.
    pub key: K5Key,
    /// The canonical `ModelMessage`.
    pub message: Json,
    /// The recorded usage payload (`hh-inclusive/1`).
    pub usage: Json,
    /// The recorded timing payload.
    pub timing: Json,
    /// `contract.dependencies` — `{model_snapshot, profile_version,
    /// definition_version}` (revalidation is `never` by construction).
    pub dependencies: Vec<DependencyStamp>,
    /// IR-4: set by [`K5Cache::note_drift`] when a served-model drift or
    /// fingerprint DRIFT invalidates the pinned snapshot — the reason string
    /// is a ledger fact, not a cleared entry.
    pub stale_by_dependency: Option<String>,
    /// The logical write instant.
    pub written_at: u64,
}

/// The K5 admissibility context (ADR-0129 d.2) — the closed set of contexts
/// under which serving a recorded response is honest.
#[derive(Debug, Clone, PartialEq)]
pub enum K5Context {
    /// `reproduce(bundle, level)` — admissible at `level ≥ R1` only.
    Reproduce {
        /// The declared reproduce level (`R1` = 1).
        level: u8,
    },
    /// The recording/fake gateway variant — its corpus is a K5 cache.
    RecordingGateway,
    /// A declared `deterministic_replay` component variant.
    DeclaredDeterministicReplay,
    /// An experiment arm enabling K5 — admissible only when `cache` is a
    /// declared factor of the pre-registered design (AC-R-2.3.4-11).
    ExperimentArm {
        /// `true` when K5 is declared as a factor in the `PreRegistration`.
        declared_factor: bool,
    },
}

/// `PreRegistrationInvalid` — the typed refusal for a K5 enablement outside
/// the admissible contexts (never a warning, never silently off).
#[derive(Debug, Clone, PartialEq)]
pub struct PreRegistrationInvalid {
    /// Why the enablement is inadmissible.
    pub reason: String,
}

impl std::fmt::Display for PreRegistrationInvalid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PreRegistrationInvalid: {}", self.reason)
    }
}
impl std::error::Error for PreRegistrationInvalid {}

/// `admit(context)` — the K5 admissibility check (ADR-0129 d.2). `reproduce`
/// requires `level ≥ R1`; an experiment arm requires `cache` as a declared
/// factor. Every other enablement is `PreRegistrationInvalid`.
pub fn admit(context: &K5Context) -> Result<(), PreRegistrationInvalid> {
    match context {
        K5Context::Reproduce { level } if *level >= 1 => Ok(()),
        K5Context::Reproduce { level } => Err(PreRegistrationInvalid {
            reason: format!("K5 under reproduce requires level ≥ R1 (observed level {level})"),
        }),
        K5Context::RecordingGateway | K5Context::DeclaredDeterministicReplay => Ok(()),
        K5Context::ExperimentArm {
            declared_factor: true,
        } => Ok(()),
        K5Context::ExperimentArm {
            declared_factor: false,
        } => Err(PreRegistrationInvalid {
            reason: "K5 enabled in a non-reproduce arm without `cache` declared as a factor"
                .to_string(),
        }),
    }
}

/// The K5 resolve outcome — the closed set a `model.cache.resolved` row's
/// `outcome` spells (AC-R-2.3.4-6: one row per lookup, hits and misses).
#[derive(Debug, Clone, PartialEq)]
pub enum K5Resolution {
    /// A live entry served (`served_from_cache` + `timing = n/a{not_run}`).
    Hit {
        /// The served entry.
        entry: K5Entry,
    },
    /// No entry under the key.
    Miss,
    /// The entry exists but is withheld (`stale_withheld` under
    /// `mode = execute`, or a reproduce resolve that did not explicitly
    /// request a drift-marked entry) — never a silent miss.
    StaleWithheld {
        /// The withheld entry's ref (the stale outcome is observable).
        entry_ref: String,
        /// `stale_withheld` reason (`revoked:{dep_ref}` or the IR-4 mark).
        reason: String,
    },
    /// `mode = audit` (or an explicit reproduce request) returns the withheld
    /// entry annotated with its state.
    Annotated {
        /// The annotated entry.
        entry: K5Entry,
        /// The withholding reason.
        reason: String,
    },
}

impl K5Resolution {
    /// The `model.cache.resolved` `outcome` spelling.
    pub fn outcome(&self) -> &'static str {
        match self {
            K5Resolution::Hit { .. } => "hit",
            K5Resolution::Miss => "miss",
            K5Resolution::StaleWithheld { .. } => "stale_withheld",
            K5Resolution::Annotated { .. } => "annotated",
        }
    }
}

/// `K5Cache` — the exact-match response cache. Entries are immutable once
/// written; invalidation is recorded (`stale_by_dependency`, the revoked-dep
/// index) — nothing is ever deleted (IR-2/IR-4: the stale outcome must stay
/// observable).
#[derive(Debug, Default)]
pub struct K5Cache {
    entries: BTreeMap<String, K5Entry>,
    /// `dep_ref → revoked stamp` — the `RevocationRecord → StaleIndex`
    /// projection (IR-2). An entry whose contract pins `(dep_ref, stamp)` is
    /// withheld while the revocation stands.
    revoked: BTreeMap<String, String>,
}

impl K5Cache {
    /// An empty cache.
    pub fn new() -> K5Cache {
        K5Cache::default()
    }

    /// Every stored entry (deterministic `entry_ref` order) — for the
    /// `model.cache.resolved` projection and the recording corpus.
    pub fn entries(&self) -> impl Iterator<Item = &K5Entry> {
        self.entries.values()
    }

    /// `write(key, message, usage, timing, deps, at) → entry_ref` — appends an
    /// immutable entry. A write under an existing key is idempotent: the
    /// canonical key fixes the value, so a duplicate write returns the
    /// existing ref.
    pub fn write(
        &mut self,
        key: K5Key,
        message: Json,
        usage: Json,
        timing: Json,
        dependencies: Vec<DependencyStamp>,
        at: u64,
    ) -> String {
        let entry_ref = key.entry_key();
        self.entries.entry(entry_ref.clone()).or_insert(K5Entry {
            entry_ref: entry_ref.clone(),
            key,
            message,
            usage,
            timing,
            dependencies,
            stale_by_dependency: None,
            written_at: at,
        });
        entry_ref
    }

    /// `RevocationRecord → StaleIndex` (IR-2): record that `(dep_ref, stamp)`
    /// is revoked — every entry whose contract pins that pair is withheld
    /// from `execute` resolves and annotated under `audit`.
    pub fn record_revocation(&mut self, dep_ref: &str, stamp: &str) {
        self.revoked.insert(dep_ref.to_string(), stamp.to_string());
    }

    /// IR-4 — a served-model drift or fingerprint DRIFT against the pinned
    /// snapshot marks every entry depending on `dep_ref` (usually
    /// `snapshot_dep_ref(snapshot_id)`)
    /// `stale_by_dependency`. Returns the marked `entry_ref`s (the mark is a
    /// ledger fact — the caller emits the `SnapshotClaim`).
    pub fn note_drift(&mut self, dep_ref: &str, reason: &str) -> Vec<String> {
        let mut marked = Vec::new();
        for entry in self.entries.values_mut() {
            if entry.dependencies.iter().any(|d| d.ref_ == dep_ref)
                && entry.stale_by_dependency.is_none()
            {
                entry.stale_by_dependency = Some(reason.to_string());
                marked.push(entry.entry_ref.clone());
            }
        }
        marked.sort();
        marked
    }

    /// The withholding reason for an entry, if any: a revoked contract dep
    /// (IR-2) or an IR-4 drift mark.
    fn withheld_reason(&self, entry: &K5Entry) -> Option<String> {
        for dep in &entry.dependencies {
            if let Some(revoked) = self.revoked.get(&dep.ref_) {
                if revoked == &dep.stamp {
                    return Some(format!(
                        "stale_withheld{{revoked:{}={}}}",
                        dep.ref_, dep.stamp
                    ));
                }
            }
        }
        entry
            .stale_by_dependency
            .as_ref()
            .map(|r| format!("stale_by_dependency{{{r}}}"))
    }

    /// `resolve(key, mode, explicit)` — the lookup. `mode = execute` never
    /// serves a withheld entry (`StaleWithheld`); `mode = audit` returns it
    /// `Annotated`; `mode = reproduce` serves a drift-marked entry only when
    /// `explicit = true` (the reproduce request named it) — annotated either
    /// way (AC-R-2.3.4-7; ADR-0129 d.2/d.4).
    pub fn resolve(&self, key: &K5Key, mode: ResolveMode, explicit: bool) -> K5Resolution {
        let entry_ref = key.entry_key();
        let Some(entry) = self.entries.get(&entry_ref) else {
            return K5Resolution::Miss;
        };
        match self.withheld_reason(entry) {
            None => K5Resolution::Hit {
                entry: entry.clone(),
            },
            Some(reason) => match mode {
                ResolveMode::Execute => K5Resolution::StaleWithheld { entry_ref, reason },
                ResolveMode::Audit => K5Resolution::Annotated {
                    entry: entry.clone(),
                    reason,
                },
                ResolveMode::Reproduce => {
                    if explicit {
                        K5Resolution::Annotated {
                            entry: entry.clone(),
                            reason,
                        }
                    } else {
                        K5Resolution::StaleWithheld { entry_ref, reason }
                    }
                }
            },
        }
    }

    /// The terminal-event members a served hit stamps (§5b.4):
    /// `{served_from_cache: entry_ref, timing: n/a{not_run}}`.
    pub fn served_terminal_stamp(entry_ref: &str) -> Json {
        Json::obj(vec![
            (
                "served_from_cache",
                Json::Str(format!("cache({entry_ref})")),
            ),
            ("timing", Json::Str("n/a{not_run}".to_string())),
        ])
    }

    /// The `model.cache.resolved` payload for one lookup (ADR-0128 d.3 —
    /// exactly one row per lookup, hits and misses alike).
    pub fn resolved_payload(
        &self,
        key: &K5Key,
        resolution: &K5Resolution,
        mode: ResolveMode,
        at: u64,
    ) -> Json {
        let entry_ref = match resolution {
            K5Resolution::Hit { entry } | K5Resolution::Annotated { entry, .. } => {
                Json::Str(entry.entry_ref.clone())
            }
            K5Resolution::StaleWithheld { entry_ref, .. } => Json::Str(entry_ref.clone()),
            K5Resolution::Miss => Json::Null,
        };
        let reason = match resolution {
            K5Resolution::StaleWithheld { reason, .. } | K5Resolution::Annotated { reason, .. } => {
                Json::Str(reason.clone())
            }
            _ => Json::Null,
        };
        Json::obj(vec![
            ("kind", Json::Str("model.cache.resolved".to_string())),
            ("cache_kind", Json::Str(K5_CACHE_KIND.to_string())),
            ("key", Json::Str(key.entry_key())),
            ("outcome", Json::Str(resolution.outcome().to_string())),
            ("entry_ref", entry_ref),
            ("reason", reason),
            (
                "mode",
                Json::Str(
                    match mode {
                        ResolveMode::Execute => "execute",
                        ResolveMode::Audit => "audit",
                        ResolveMode::Reproduce => "reproduce",
                    }
                    .to_string(),
                ),
            ),
            ("at", Json::Int(at as i64)),
        ])
    }
}
