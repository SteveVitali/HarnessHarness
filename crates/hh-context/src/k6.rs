//! `R-2.3.4²` — **K6**, the similarity-keyed semantic cache (§5b.4;
//! ADR-0129 d.3; ticket S5.1; C2).
//!
//! K6 caches a *served* model response keyed by request semantics:
//! `lookup(request_embedding, threshold, scope{principal, readers}) →
//! exact_hit | marginal{entry, similarity} | miss`, with
//! `verify(entry, request) → pass | fail` a **mandatory** gate on every
//! marginal. An exact hit (`request_key` equal, or a promoted key) serves
//! without a verifier call; a marginal is served **only** after a registered
//! `Validator` passes — the verdict is a ledger fact on the
//! `model.cache.resolved` row (`verifier_verdict{validator_ref, verdict}`)
//! and the passed key is *promoted* onto the entry so the identical request
//! is an exact hit thereafter.
//!
//! ## Scoping (ADR-0129 d.1/d.3; AC-R-2.3.4-9/-12)
//!
//! Every entry carries `principal`, `readers` and the writer's authority:
//!
//! - **principal scope** — entries are namespaced by writer principal; a
//!   cross-principal near-key is `refused{readers_violation{principal}}`,
//!   never served and never a silent miss (the poisoning fixture must be
//!   *observable*);
//! - **readers** — `entry.readers ⊇ scope.readers` or the best candidate
//!   refuses `readers_violation`;
//! - **narrow-or-preserve** (ADR-0034 P1) — a `delegate`/`external` writer's
//!   entry is never served into a higher-authority context
//!   (`refused{authority_narrowed}`); a served hit is delivered at the
//!   entry's label, never raised.
//!
//! ## Metrics (ADR-0128 d.4; AC-R-2.3.4-12)
//!
//! `cache.false_hit_rate[semantic]` is mandatory and per-profile: every
//! marginal verdict lands in [`K6Cache::verdict_log`];
//! [`K6Cache::false_hit_rate_ppm`] folds `fail / (pass + fail)` per
//! `profile_ref` — the similarity index's measured false-positive rate the
//! verifier caught (a served marginal is by definition verifier-passed).
//!
//! ## Invalidation (IR-2)
//!
//! Entries carry the same `{model_snapshot, profile_version,
//! definition_version}` contract stamps as K5 ([`k6_dependencies`]); a
//! `RevocationRecord` withholds matching entries (`stale_withheld` under
//! `mode = execute`, `annotated` under `audit`) and
//! [`K6Cache::note_drift`] marks drift — nothing is ever deleted.
//!
//! Every lookup emits exactly one `model.cache.resolved` row
//! ([`K6Cache::resolved_payload`]) — hits, misses, refusals and withhelds
//! alike (ADR-0128 d.3: nothing unaccounted).

use std::collections::{BTreeMap, BTreeSet};

use hh_identity::idp::idp_id;
use hh_identity::names::ResolveMode;
use hh_provenance::authority::{AuthorityClass, ReaderSet};
use hh_wire::json::Json;

use crate::k5::{definition_dep_ref, profile_dep_ref, snapshot_dep_ref};
use crate::memory::DependencyStamp;
use crate::vocab::{DependencyKind, Granularity};

/// `k6.key.1` — the `idp` domain K6 entry refs mint under.
pub const K6_KEY_IDP: &str = "k6.key.1";

/// `semantic` — the `cache_kind` spelling in `model.cache.resolved`.
pub const K6_CACHE_KIND: &str = "semantic";

/// `k6.semantic/1` — the closed-schema ref a stored entry body spells.
pub const K6_SCHEMA_REF: &str = "k6.semantic/1";

/// `1_000_000` — the ppm base similarities and thresholds spell in
/// (deterministic integer math — no floats in the resource model, CC1).
pub const PPM: i64 = 1_000_000;

/// Integer square root (`⌊√n⌋`) — Newton's method over `u128`. The cosine
/// computation needs a real root, and floats are not ledger data.
fn isqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// `cosine_similarity_ppm(a, b)` — the integer cosine of two quantized
/// embeddings, spelled in ppm (`PPM` = identical direction, `0` =
/// orthogonal, `-PPM` = opposite). `None` when the vectors are incomparable
/// (different ranks, or either is the zero vector) — an incomparable entry
/// is simply not a candidate. Components are `i64`; accumulation is `i128`
/// so a 2⁶³-scale component cannot overflow the dot product.
pub fn cosine_similarity_ppm(a: &[i64], b: &[i64]) -> Option<i64> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let mut dot: i128 = 0;
    let mut da: i128 = 0;
    let mut db: i128 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        let x = *x as i128;
        let y = *y as i128;
        dot += x * y;
        da += x * x;
        db += y * y;
    }
    if da == 0 || db == 0 {
        return None;
    }
    // sim = dot / √(da·db); report in ppm with the sign preserved:
    //   sim_ppm = sign(dot) · isqrt(|dot|² · PPM² / (da·db)).
    // All-positive arithmetic in u128 (`⌊⌊x/a⌋/b⌋ = ⌊x/(a·b)⌋` keeps the
    // two-step division exact); saturating mul + a `min(PPM)` clamp bound
    // the artifacts of pathological magnitudes.
    let mag = dot.unsigned_abs();
    let scaled = mag
        .saturating_mul(mag)
        .saturating_mul(PPM as u128)
        .saturating_mul(PPM as u128);
    let root = isqrt(scaled / (da as u128) / (db as u128)).min(PPM as u128) as i64;
    Some(if dot < 0 { -root } else { root })
}

/// `K6Write` — the immutable write record. `threshold_ppm` and
/// `verifier_ref` are part of the sealed definition (§5b.4 K6 entry row:
/// "threshold and verifier part of the sealed definition") — the cache
/// stores what the write was bound under; the *lookup* threshold comes from
/// the request.
#[derive(Debug, Clone, PartialEq)]
pub struct K6Write {
    /// The canonical request key (`H(canonical_request)`) — the exact-hit
    /// leg. Two principals may write the same request key; entries are
    /// namespaced `(principal, request_key)`.
    pub request_key: String,
    /// The request embedding (quantized, `i64` components).
    pub embedding: Vec<i64>,
    /// The canonical served response (`{message, usage, timing}` — the same
    /// recorded documents a K5 entry stores).
    pub response: Json,
    /// The bound `ModelProfile` ref — the `cache.false_hit_rate[semantic]`
    /// metric axis (AC-R-2.3.4-12: "per profile").
    pub profile_ref: String,
    /// The writer principal — the scope namespace.
    pub principal: String,
    /// The admitted `readers` (carried on the entry's label).
    pub readers: ReaderSet,
    /// The writer's `AuthorityClass` — narrow-or-preserve enforcement
    /// happens at lookup against the *context* authority.
    pub writer_authority: AuthorityClass,
    /// The writer ref (the `Delegate`/`AgentProcess` id) — recorded, never
    /// trusted for authority.
    pub writer_ref: String,
    /// The `Validator` ref the sealed definition binds for this entry — the
    /// lookup refuses a marginal when it is not a *registered* validator of
    /// the serving context.
    pub verifier_ref: String,
    /// The threshold the entry was written under (contract record).
    pub threshold_ppm: i64,
    /// The IR-2 contract dependencies.
    pub dependencies: Vec<DependencyStamp>,
}

/// `k6_dependencies` — the same `{model_snapshot, profile_version,
/// definition_version}` contract trio K5 carries (IR-2: every entry carries
/// a D4 `InvalidationContract` whose stamps let a `RevocationRecord →
/// StaleIndex` withhold matching keys).
pub fn k6_dependencies(
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
            validator_ref: None,
        },
        DependencyStamp {
            kind: DependencyKind::ProfileVersion,
            ref_: profile_dep_ref(profile_ref),
            stamp: profile_version_id.to_string(),
            granularity: Granularity::Row,
            validator_ref: None,
        },
        DependencyStamp {
            kind: DependencyKind::DefinitionVersion,
            ref_: definition_dep_ref(definition_version_id),
            stamp: definition_version_id.to_string(),
            granularity: Granularity::Row,
            validator_ref: None,
        },
    ]
}

/// A stored K6 entry. `promoted_keys` records every marginal request key a
/// registered verifier passed — a promotion is a ledger-visible fact, and
/// the promoted request resolves on the exact leg thereafter.
#[derive(Debug, Clone, PartialEq)]
pub struct K6Entry {
    /// The content-addressed entry ref
    /// (`idp/1` over `request_key ∥ principal`).
    pub entry_ref: String,
    /// The canonical request key the entry was written under.
    pub request_key: String,
    /// The stored embedding.
    pub embedding: Vec<i64>,
    /// The canonical served response.
    pub response: Json,
    /// The metric axis profile.
    pub profile_ref: String,
    /// The scope namespace principal.
    pub principal: String,
    /// The admitted readers.
    pub readers: ReaderSet,
    /// The writer's authority — a served hit is delivered at this label,
    /// never raised.
    pub writer_authority: AuthorityClass,
    /// The writer ref.
    pub writer_ref: String,
    /// The bound `Validator` ref.
    pub verifier_ref: String,
    /// The sealed threshold at write time (contract record).
    pub threshold_ppm: i64,
    /// The IR-2 contract dependencies.
    pub dependencies: Vec<DependencyStamp>,
    /// IR-4-style drift mark (served-model drift / fingerprint DRIFT
    /// against a pinned dep).
    pub stale_by_dependency: Option<String>,
    /// Request keys promoted by a passing verifier verdict.
    pub promoted_keys: BTreeSet<String>,
    /// The logical write instant.
    pub written_at: u64,
}

/// `K6Scope` — `scope{principal, readers}` plus the serving context's
/// authority and registered validators (§5b.4 K6 row).
#[derive(Debug, Clone, PartialEq)]
pub struct K6Scope {
    /// The calling context's principal — entries are scoped by principal.
    pub principal: String,
    /// The calling context's readers — `entry.readers ⊇ scope.readers`.
    pub readers: ReaderSet,
    /// The calling context's authority ceiling — an entry written below it
    /// refuses (`authority_narrowed`).
    pub authority: AuthorityClass,
    /// The validator refs registered for this serving context — a marginal
    /// whose bound verifier is not registered refuses (`verifier_failed`).
    pub validators: BTreeSet<String>,
}

/// `K6Request` — `lookup(request_embedding, threshold, scope)`'s request
/// half: the canonical key (exact leg), the embedding (marginal leg), the
/// lookup threshold, and the canonical request document the verifier reads.
#[derive(Debug, Clone, PartialEq)]
pub struct K6Request {
    /// `H(canonical_request)` — the exact-hit coordinate.
    pub request_key: String,
    /// The request embedding.
    pub embedding: Vec<i64>,
    /// The lookup threshold in ppm (from the sealed definition).
    pub threshold_ppm: i64,
    /// The canonical request document — handed to the verifier verbatim.
    pub request: Json,
}

/// `K6Verdict` — `{validator_ref, verdict}` — the `verifier_verdict` member
/// on the `model.cache.resolved` row.
#[derive(Debug, Clone, PartialEq)]
pub struct K6Verdict {
    /// The `Validator` ref that judged the marginal.
    pub validator_ref: String,
    /// `pass` | `fail`.
    pub verdict: String,
}

impl K6Verdict {
    /// The event payload spelling: `{validator_ref, verdict}`.
    pub fn payload(&self) -> Json {
        Json::obj(vec![
            ("validator_ref", Json::str(self.validator_ref.clone())),
            ("verdict", Json::str(self.verdict.clone())),
        ])
    }
}

/// `K6Verifier` — the registered-`Validator` seam. The dialect/registry
/// resolves `verifier_ref → impl`; the cache checks the impl's ref against
/// the entry's bound verifier and the context's registered set before
/// trusting `verify`.
pub trait K6Verifier {
    /// The `Validator` ref this impl answers for.
    fn validator_ref(&self) -> &str;
    /// `verify(entry, request) → pass | fail`.
    fn verify(&self, entry: &K6Entry, request: &Json) -> bool;
}

/// `K6Refusal` — the closed refusal vocabulary a K6 lookup spells
/// (`outcome = refused`; §5b.4 failure-mode column names `VerifierFailed`
/// and `ReadersViolation`).
#[derive(Debug, Clone, PartialEq)]
pub enum K6Refusal {
    /// The best candidate fails the scope check — cross-principal key or
    /// `readers` not admitted (`readers_violation{...}`).
    ReadersViolation {
        /// `principal` | `readers`.
        axis: String,
    },
    /// The best candidate's writer authority sits below the context's —
    /// narrow-or-preserve (`authority_narrowed{writer<x}`).
    AuthorityNarrowed {
        /// The writer's authority spelling.
        writer: String,
        /// The context's authority spelling.
        context: String,
    },
    /// The mandatory verifier refused the marginal — unregistered validator
    /// or a `fail` verdict (`verifier_failed{...}`).
    VerifierFailed {
        /// `unregistered:{validator_ref}` or `verdict:fail`.
        detail: String,
    },
}

impl K6Refusal {
    /// The `reason` spelling on the `model.cache.resolved` row.
    pub fn reason(&self) -> String {
        match self {
            K6Refusal::ReadersViolation { axis } => {
                format!("readers_violation{{{axis}}}")
            }
            K6Refusal::AuthorityNarrowed { writer, context } => {
                format!("authority_narrowed{{writer:{writer}<context:{context}}}")
            }
            K6Refusal::VerifierFailed { detail } => {
                format!("verifier_failed{{{detail}}}")
            }
        }
    }
}

/// `K6Resolution` — the closed outcome set a `model.cache.resolved` row's
/// `outcome` spells (one row per lookup — AC-R-2.3.4-6).
#[derive(Debug, Clone, PartialEq)]
pub enum K6Resolution {
    /// Served: an exact hit (verifier-free) or a marginal the registered
    /// verifier passed (`verifier_verdict` present on the latter).
    Hit {
        /// The served entry.
        entry: K6Entry,
        /// `Some` when a marginal passed the verifier (and promoted);
        /// `None` on an exact hit.
        verdict: Option<K6Verdict>,
        /// `true` when the serve promoted a new key onto the entry.
        promoted: bool,
        /// The marginal similarity in ppm (`None` on the exact leg).
        similarity_ppm: Option<i64>,
    },
    /// No eligible entry (no exact key and no candidate at/above the
    /// lookup threshold).
    Miss,
    /// The best candidate exists but is withheld (revoked dep or drift
    /// mark) under `mode = execute` — never a silent miss.
    StaleWithheld {
        /// The withheld entry's ref.
        entry_ref: String,
        /// `stale_withheld{...}` reason.
        reason: String,
    },
    /// `mode = audit` returns a withheld entry annotated.
    Annotated {
        /// The annotated entry.
        entry: K6Entry,
        /// The withholding reason.
        reason: String,
    },
    /// The best candidate was visible but not servable — scope violation
    /// or verifier failure. The refusal is a ledger fact (the poisoning
    /// fixture must refuse, not silently miss — AC-R-2.3.4-12).
    Refused {
        /// The candidate's entry ref.
        entry_ref: String,
        /// The typed refusal.
        refusal: K6Refusal,
    },
}

impl K6Resolution {
    /// The `outcome` spelling.
    pub fn outcome(&self) -> &'static str {
        match self {
            K6Resolution::Hit { .. } => "hit",
            K6Resolution::Miss => "miss",
            K6Resolution::StaleWithheld { .. } => "stale_withheld",
            K6Resolution::Annotated { .. } => "annotated",
            K6Resolution::Refused { .. } => "refused",
        }
    }
}

/// `K6Cache` — the similarity-keyed semantic cache. Entries are immutable;
/// invalidation and promotion are recorded facts — nothing is deleted.
#[derive(Debug, Default)]
pub struct K6Cache {
    /// `entry_ref → entry`.
    entries: BTreeMap<String, K6Entry>,
    /// `(principal, request_key) → entry_ref` — the exact-hit index,
    /// covering both written keys and verifier-promoted keys.
    exact: BTreeMap<(String, String), String>,
    /// `dep_ref → revoked stamp` — the `RevocationRecord → StaleIndex`
    /// projection (IR-2).
    revoked: BTreeMap<String, String>,
    /// `(profile_ref, passed)` — one row per marginal verdict; the
    /// `cache.false_hit_rate[semantic]` metric source.
    verdict_log: Vec<(String, bool)>,
}

impl K6Cache {
    /// An empty cache.
    pub fn new() -> K6Cache {
        K6Cache::default()
    }

    /// Every stored entry (deterministic `entry_ref` order).
    pub fn entries(&self) -> impl Iterator<Item = &K6Entry> {
        self.entries.values()
    }

    /// The marginal-verdict log — `(profile_ref, passed)` per verifier call
    /// (deterministic append order). The `cache.false_hit_rate[semantic]`
    /// projection folds this.
    pub fn verdict_log(&self) -> &[(String, bool)] {
        &self.verdict_log
    }

    /// `cache.false_hit_rate[semantic]` per profile (AC-R-2.3.4-12) —
    /// `fails · PPM / (pass + fails)` over the verdict log. `None` when the
    /// profile has seen no marginal verdict (rate undefined, not zero).
    pub fn false_hit_rate_ppm(&self, profile_ref: &str) -> Option<i64> {
        let mut pass = 0i64;
        let mut fail = 0i64;
        for (p, ok) in &self.verdict_log {
            if p == profile_ref {
                if *ok {
                    pass += 1;
                } else {
                    fail += 1;
                }
            }
        }
        let total = pass + fail;
        (total > 0).then(|| fail * PPM / total)
    }

    /// `write(write, at) → entry_ref` — appends an immutable entry and
    /// indexes `(principal, request_key) → entry_ref`. A write under an
    /// existing coordinate is idempotent (returns the existing ref).
    pub fn write(&mut self, write: K6Write, at: u64) -> String {
        let entry_ref = idp_id(
            K6_KEY_IDP,
            format!("{}∥{}", write.request_key, write.principal).as_bytes(),
        );
        if !self.entries.contains_key(&entry_ref) {
            self.entries.insert(
                entry_ref.clone(),
                K6Entry {
                    entry_ref: entry_ref.clone(),
                    request_key: write.request_key.clone(),
                    embedding: write.embedding,
                    response: write.response,
                    profile_ref: write.profile_ref,
                    principal: write.principal.clone(),
                    readers: write.readers,
                    writer_authority: write.writer_authority,
                    writer_ref: write.writer_ref,
                    verifier_ref: write.verifier_ref,
                    threshold_ppm: write.threshold_ppm,
                    dependencies: write.dependencies,
                    stale_by_dependency: None,
                    promoted_keys: BTreeSet::new(),
                    written_at: at,
                },
            );
        }
        self.exact.insert(
            (write.principal.clone(), write.request_key),
            entry_ref.clone(),
        );
        entry_ref
    }

    /// `RevocationRecord → StaleIndex` (IR-2).
    pub fn record_revocation(&mut self, dep_ref: &str, stamp: &str) {
        self.revoked.insert(dep_ref.to_string(), stamp.to_string());
    }

    /// Mark every entry depending on `dep_ref` `stale_by_dependency`
    /// (served-model drift / fingerprint DRIFT). Returns the marked refs.
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

    /// The withholding reason for an entry: a revoked contract dep (IR-2)
    /// or a drift mark.
    fn withheld_reason(&self, entry: &K6Entry) -> Option<String> {
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

    /// The scope check (`scope{principal, readers}` + narrow-or-preserve).
    /// `Err` carries the typed refusal; `Ok` admits the candidate to the
    /// serve path.
    fn scope_check(entry: &K6Entry, scope: &K6Scope) -> Result<(), K6Refusal> {
        if entry.principal != scope.principal {
            return Err(K6Refusal::ReadersViolation {
                axis: "principal".to_string(),
            });
        }
        if !entry.readers.is_superset_of(&scope.readers) {
            return Err(K6Refusal::ReadersViolation {
                axis: "readers".to_string(),
            });
        }
        if entry.writer_authority < scope.authority {
            return Err(K6Refusal::AuthorityNarrowed {
                writer: entry.writer_authority.as_str().to_string(),
                context: scope.authority.as_str().to_string(),
            });
        }
        Ok(())
    }

    /// `resolve(request, scope, mode, verifier)` — the lookup. Exact leg
    /// first (`(principal, request_key)` — written or promoted); marginal
    /// leg picks the best-scoring entry at/above `request.threshold_ppm`.
    ///
    /// A marginal is **never** served without `verify` — the verifier is a
    /// required argument, its ref must match the entry's bound verifier and
    /// be registered in `scope.validators`, and a `fail` verdict refuses
    /// (`verifier_failed`) and lands in the `false_hit_rate` log. A `pass`
    /// promotes the request key onto the entry.
    ///
    /// `verifier` may be `None` only when the caller has proven the exact
    /// leg is the only possible outcome — passing `None` where a marginal
    /// arises refuses `verifier_failed{unregistered:none}` rather than
    /// silently serving.
    pub fn resolve(
        &mut self,
        request: &K6Request,
        scope: &K6Scope,
        mode: ResolveMode,
        verifier: Option<&dyn K6Verifier>,
    ) -> K6Resolution {
        // ── Exact leg: (principal, request_key) — written or promoted.
        if let Some(entry_ref) = self
            .exact
            .get(&(scope.principal.clone(), request.request_key.clone()))
            .cloned()
        {
            let entry = self.entries[&entry_ref].clone();
            if let Some(reason) = self.withheld_reason(&entry) {
                return match mode {
                    ResolveMode::Execute | ResolveMode::Reproduce => {
                        K6Resolution::StaleWithheld { entry_ref, reason }
                    }
                    ResolveMode::Audit => K6Resolution::Annotated { entry, reason },
                };
            }
            if let Err(refusal) = Self::scope_check(&entry, scope) {
                return K6Resolution::Refused { entry_ref, refusal };
            }
            return K6Resolution::Hit {
                entry,
                verdict: None,
                promoted: false,
                similarity_ppm: None,
            };
        }

        // ── Marginal leg: best-scoring candidate ≥ threshold. Score over
        // *all* entries so a scope-excluded near-key refuses observably
        // instead of silently missing (AC-R-2.3.4-12 poisoning fixture).
        let mut best: Option<(&K6Entry, i64)> = None;
        for entry in self.entries.values() {
            let Some(sim) = cosine_similarity_ppm(&request.embedding, &entry.embedding) else {
                continue;
            };
            if sim < request.threshold_ppm {
                continue;
            }
            match best {
                Some((_, best_sim)) if sim <= best_sim => {}
                _ => best = Some((entry, sim)),
            }
        }
        let Some((candidate, similarity)) = best else {
            return K6Resolution::Miss;
        };
        let entry = candidate.clone();
        let entry_ref = entry.entry_ref.clone();

        // Withholding precedes scoping: a stale entry is never served and
        // never verified.
        if let Some(reason) = self.withheld_reason(&entry) {
            return match mode {
                ResolveMode::Execute | ResolveMode::Reproduce => {
                    K6Resolution::StaleWithheld { entry_ref, reason }
                }
                ResolveMode::Audit => K6Resolution::Annotated { entry, reason },
            };
        }
        if let Err(refusal) = Self::scope_check(&entry, scope) {
            return K6Resolution::Refused { entry_ref, refusal };
        }

        // ── The mandatory verifier (§5b.4: "a marginal is served only
        // after a registered Validator passes, then promoted").
        let Some(v) = verifier else {
            return K6Resolution::Refused {
                entry_ref,
                refusal: K6Refusal::VerifierFailed {
                    detail: "unregistered:none".to_string(),
                },
            };
        };
        if v.validator_ref() != entry.verifier_ref
            || !scope.validators.contains(&entry.verifier_ref)
        {
            return K6Resolution::Refused {
                entry_ref,
                refusal: K6Refusal::VerifierFailed {
                    detail: format!("unregistered:{}", entry.verifier_ref),
                },
            };
        }
        let passed = v.verify(&entry, &request.request);
        self.verdict_log.push((entry.profile_ref.clone(), passed));
        if !passed {
            return K6Resolution::Refused {
                entry_ref,
                refusal: K6Refusal::VerifierFailed {
                    detail: "verdict:fail".to_string(),
                },
            };
        }
        let verdict = K6Verdict {
            validator_ref: entry.verifier_ref.clone(),
            verdict: "pass".to_string(),
        };
        // Promotion: the passed key joins the exact index.
        let entry_mut = self.entries.get_mut(&entry_ref).expect("entry present");
        entry_mut.promoted_keys.insert(request.request_key.clone());
        let entry = entry_mut.clone();
        self.exact.insert(
            (scope.principal.clone(), request.request_key.clone()),
            entry_ref,
        );
        K6Resolution::Hit {
            entry,
            verdict: Some(verdict),
            promoted: true,
            similarity_ppm: Some(similarity),
        }
    }

    /// The `model.cache.resolved` payload for one lookup (ADR-0128 d.3).
    /// `cache_kind = semantic`; `scope` spells `{principal, readers}`;
    /// `verifier_verdict{validator_ref, verdict}` is present on a verified
    /// marginal serve; `similarity_ppm`/`promoted` make the marginal legs
    /// auditable.
    pub fn resolved_payload(
        &self,
        request: &K6Request,
        scope: &K6Scope,
        resolution: &K6Resolution,
        mode: ResolveMode,
        at: u64,
    ) -> Json {
        let (entry_ref, reason, verdict, promoted, similarity) = match resolution {
            K6Resolution::Hit {
                entry,
                verdict,
                promoted,
                similarity_ppm,
            } => (
                Json::Str(entry.entry_ref.clone()),
                Json::Null,
                verdict
                    .as_ref()
                    .map(K6Verdict::payload)
                    .unwrap_or(Json::Null),
                Json::Bool(*promoted),
                similarity_ppm.map(Json::Int).unwrap_or(Json::Null),
            ),
            K6Resolution::Annotated { entry, reason } => (
                Json::Str(entry.entry_ref.clone()),
                Json::Str(reason.clone()),
                Json::Null,
                Json::Bool(false),
                Json::Null,
            ),
            K6Resolution::StaleWithheld { entry_ref, reason } => (
                Json::Str(entry_ref.clone()),
                Json::Str(reason.clone()),
                Json::Null,
                Json::Bool(false),
                Json::Null,
            ),
            K6Resolution::Refused { entry_ref, refusal } => (
                Json::Str(entry_ref.clone()),
                Json::Str(refusal.reason()),
                Json::Null,
                Json::Bool(false),
                Json::Null,
            ),
            K6Resolution::Miss => (
                Json::Null,
                Json::Null,
                Json::Null,
                Json::Bool(false),
                Json::Null,
            ),
        };
        let readers = match &scope.readers {
            ReaderSet::Public => Json::str("public".to_string()),
            ReaderSet::Restricted(s) => Json::Arr(s.iter().map(|r| Json::str(r.clone())).collect()),
        };
        Json::obj(vec![
            ("kind", Json::Str("model.cache.resolved".to_string())),
            ("cache_kind", Json::Str(K6_CACHE_KIND.to_string())),
            ("key", Json::str(request.request_key.clone())),
            (
                "scope",
                Json::obj(vec![
                    ("principal", Json::str(scope.principal.clone())),
                    ("readers", readers),
                ]),
            ),
            ("outcome", Json::Str(resolution.outcome().to_string())),
            ("entry_ref", entry_ref),
            ("reason", reason),
            ("verifier_verdict", verdict),
            ("promoted", promoted),
            ("similarity_ppm", similarity),
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
