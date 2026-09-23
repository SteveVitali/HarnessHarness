//! `instantiate` (§3.3.4): in-process binding of a sealed definition's slots. Every
//! class receives `(profile, accounting)` — the `required_inputs` the catalog declares
//! (T-LCD-08). Each binding attempt appends a `lifecycle.component.bound` row
//! (`bind_result ∈ {bound | not_installed | trust_denied | locality_unsupported |
//! contract_mismatch | isolation_unavailable}`) — a failed bind is ledgered, never
//! silent. Hosted roots emit the single `hosting_adapter` row (their `n/a` classes bind
//! nothing). Stage-1 binds `placement = in_process` only; any other placement is
//! `locality_unsupported` (the out-of-process host is Stage 2 — DF-S1.9-1).

use hh_budget::account::Account;
use hh_hir::document::SealedDefinition;
use hh_hir::records::{AgentProcessBody, KindRecord, SlotBinding, SlotBindings};
use hh_hir::refs::RefVersion;
use hh_registry::kinds::Placement;
use hh_registry::records::{ClassRecord, RegistryRecord, VariantRecord};
use hh_registry::store::RegistryStore;

use crate::catalog::ClassCatalog;
use crate::events::{component_bound, AssemblyEvent};

/// The closed `bind_result` spellings (§3.3.4 `BindFailure.reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindReason {
    /// The variant's implementation is not installed.
    NotInstalled,
    /// The runtime refuses the record's trust posture.
    TrustDenied,
    /// The placement is not bindable in this runtime.
    LocalityUnsupported,
    /// The variant fails the contract check at bind time.
    ContractMismatch,
    /// The required isolation is unavailable.
    IsolationUnavailable,
}

impl BindReason {
    /// The canonical spelling (the `bind_result` payload value).
    pub fn as_str(self) -> &'static str {
        match self {
            BindReason::NotInstalled => "not_installed",
            BindReason::TrustDenied => "trust_denied",
            BindReason::LocalityUnsupported => "locality_unsupported",
            BindReason::ContractMismatch => "contract_mismatch",
            BindReason::IsolationUnavailable => "isolation_unavailable",
        }
    }
}

/// A `BindFailure{slot, variant, reason}` (§3.3.4).
#[derive(Debug, Clone, PartialEq)]
pub struct BindFailure {
    /// The slot whose binding failed.
    pub slot: String,
    /// The variant's pinned `version_id`.
    pub variant: String,
    /// The closed reason.
    pub reason: BindReason,
}

/// The `(profile, accounting)` every bound class receives (§3.3.4 — the declared
/// `required_inputs`, T-LCD-08).
pub struct BindContext<'a, 'b> {
    /// The run the binding events record under.
    pub run_id: &'a str,
    /// The bound profile's identity coordinate (`None` while `profile_binding` is
    /// `unbound` — the runtime decides admissibility).
    pub profile: Option<&'a str>,
    /// The run's resource account (the `ResourceAccount` required input).
    pub accounting: &'a Account<'b>,
}

/// The runtime seam — how a variant implementation is bound in-process (the stub the
/// conformance harness drives is a `VariantRuntime`; AC-CC-01's Stage-1 half).
pub trait VariantRuntime {
    /// Bind one slot's variant; the returned handle is the component's runtime token.
    fn bind(
        &self,
        slot: &str,
        class: &ClassRecord,
        binding: &SlotBinding,
        variant: &VariantRecord,
        ctx: &BindContext,
    ) -> Result<String, BindReason>;
}

/// A bound component in a `HarnessInstance`.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundComponent {
    /// The slot filled.
    pub slot: String,
    /// The class filled.
    pub class_id: String,
    /// The variant's pinned `version_id` (and `semantic_id` when the record carries it).
    pub variant_version_id: String,
    /// The variant's `semantic_id`, when known.
    pub variant_semantic_id: Option<String>,
    /// The registered placement.
    pub placement: String,
    /// The variant host, when the placement names one.
    pub host_id: Option<String>,
    /// The runtime handle `VariantRuntime::bind` returned.
    pub handle: String,
}

/// The `HarnessInstance` (§3.3.4) — the bound component set over a sealed definition.
#[derive(Debug, Clone, PartialEq)]
pub struct HarnessInstance {
    /// The definition's `{semantic_id, version_id}`.
    pub definition_ref: hh_hir::document::DefinitionVersionRef,
    /// The bound profile's coordinate, when bound.
    pub profile: Option<String>,
    /// The bound components (enabled slots only — a disabled binding is ablated, never
    /// bound and never ledgered as bound).
    pub bindings: Vec<BoundComponent>,
}

/// `instantiate`'s result: the instance (when every enabled slot bound), every
/// `lifecycle.component.bound` row (failed binds included — the run record shows the
/// attempt), and the first `BindFailure`.
#[derive(Debug)]
pub struct InstantiateOutcome {
    /// The instance, when all enabled bindings succeeded.
    pub instance: Option<HarnessInstance>,
    /// The pending `lifecycle.component.bound` rows (emit through
    /// `crate::events::emit` — the caller owns the fenced writer).
    pub events: Vec<AssemblyEvent>,
    /// The first bind failure, when any.
    pub failure: Option<BindFailure>,
}

/// `instantiate(sealed, runtime, profile, accounting) → HarnessInstance | BindFailure`
/// (§3.3.4). Resolves each pinned variant's `VariantRecord` through the store (the one
/// registry — CC7), checks `placement` (`in_process` only at Stage 1), delegates the
/// bind to the `VariantRuntime`, and collects a `component.bound` row per attempt.
pub fn instantiate<R: VariantRuntime>(
    sealed: &SealedDefinition,
    catalog: &dyn ClassCatalog,
    registry: &RegistryStore,
    runtime: &R,
    ctx: &BindContext,
) -> InstantiateOutcome {
    let mut events = Vec::new();
    let mut bindings = Vec::new();
    let mut failure: Option<BindFailure> = None;

    let doc = &sealed.document;
    let root = doc.node(&doc.root.semantic_id);
    let hosted = root
        .and_then(|n| match &n.semantic {
            KindRecord::AgentProcess(a) => Some(matches!(a.body, AgentProcessBody::Hosted(_))),
            _ => None,
        })
        .unwrap_or(false);

    if hosted {
        // The hosted path: no component classes bind; the adapter row records the
        // participant boundary (§3.3.4 instantiate note).
        events.push(component_bound(
            "hosting_adapter",
            "hosting_adapter",
            None,
            &sealed.definition_ref.version_id,
            "hosted",
            None,
            "bound",
        ));
        return InstantiateOutcome {
            instance: Some(HarnessInstance {
                definition_ref: sealed.definition_ref.clone(),
                profile: ctx.profile.map(str::to_string),
                bindings: Vec::new(),
            }),
            events,
            failure: None,
        };
    }

    let slots = root
        .and_then(|n| match &n.semantic {
            KindRecord::AgentProcess(a) => match &a.body {
                AgentProcessBody::Native(n) => Some(n.slots.clone()),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_default();

    for (slot, bs) in &slots {
        let bindings_vec: Vec<&SlotBinding> = match bs {
            SlotBindings::One(b) => vec![b],
            SlotBindings::Many(v) => v.iter().collect(),
        };
        for b in bindings_vec {
            if !b.enabled {
                continue; // ablated — never bound (§3.3.2)
            }
            let class_id = b.variant.class_id.clone();
            let Some(vid) = (match &b.variant.version {
                RefVersion::Pinned(v) => Some(v.clone()),
                RefVersion::Selector(_) => None,
            }) else {
                continue; // unsealed — `instantiate` consumes only sealed definitions
            };
            let class = catalog.class(&class_id);
            let resolved = registry.get(&vid);
            let variant = resolved.and_then(|(_, r)| match r {
                RegistryRecord::Variant(v) => Some(v.clone()),
                _ => None,
            });
            let (Some(class), Some(variant)) = (class, variant) else {
                let f = BindFailure {
                    slot: slot.clone(),
                    variant: vid.clone(),
                    reason: BindReason::NotInstalled,
                };
                events.push(component_bound(
                    slot,
                    &class_id,
                    None,
                    &vid,
                    "unknown",
                    None,
                    f.reason.as_str(),
                ));
                if failure.is_none() {
                    failure = Some(f);
                }
                continue;
            };
            let placement = variant.implementation.placement;
            if placement != Placement::InProcess {
                // Stage-1 binds in-process only (the out-of-process host is Stage 2 —
                // DF-S1.9-1); the row records `locality_unsupported`.
                let f = BindFailure {
                    slot: slot.clone(),
                    variant: vid.clone(),
                    reason: BindReason::LocalityUnsupported,
                };
                events.push(component_bound(
                    slot,
                    &class_id,
                    None,
                    &vid,
                    placement.as_str(),
                    None,
                    f.reason.as_str(),
                ));
                if failure.is_none() {
                    failure = Some(f);
                }
                continue;
            }
            match runtime.bind(slot, &class, b, &variant, ctx) {
                Ok(handle) => {
                    let sem = registry.get(&vid).and_then(|(e, _)| e.semantic_id.clone());
                    events.push(component_bound(
                        slot,
                        &class_id,
                        sem.as_deref(),
                        &vid,
                        placement.as_str(),
                        None,
                        "bound",
                    ));
                    bindings.push(BoundComponent {
                        slot: slot.clone(),
                        class_id,
                        variant_version_id: vid,
                        variant_semantic_id: sem,
                        placement: placement.as_str().to_string(),
                        host_id: None,
                        handle,
                    });
                }
                Err(reason) => {
                    events.push(component_bound(
                        slot,
                        &class_id,
                        None,
                        &vid,
                        placement.as_str(),
                        None,
                        reason.as_str(),
                    ));
                    if failure.is_none() {
                        failure = Some(BindFailure {
                            slot: slot.clone(),
                            variant: vid,
                            reason,
                        });
                    }
                }
            }
        }
    }

    InstantiateOutcome {
        instance: if failure.is_none() {
            Some(HarnessInstance {
                definition_ref: sealed.definition_ref.clone(),
                profile: ctx.profile.map(str::to_string),
                bindings,
            })
        } else {
            None
        },
        events,
        failure,
    }
}
