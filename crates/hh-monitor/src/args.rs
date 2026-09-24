//! The `SurfaceArgMap` evaluator (§5g.1 §2.6 TCB item (viii); ADR-0090; ADR-0022
//! E2) and the canonical-parameter → scope-resolution half of `resolve_handles`
//! (I-H5; ADR-0087 D3).
//!
//! A proposal carries **surface** arguments (`{surface_field → value}` — what
//! the model saw). `eval` projects them onto **canonical** parameters through
//! the compiled [`SurfaceBinding`]'s `arg_map`:
//!
//! - an argument absent from the map ⇒ [`ArgError::UnmappedArgument`] (a deny —
//!   AC-R-2.8.1-10's runtime half);
//! - `identity`/`rename`/`coerce`/`project` bind the value to
//!   `capability_param` (for `project`, the entry names the parameter path —
//!   the value lands at the root parameter); `const{v}` contributes `v` with no
//!   surface argument; `parse`/`unescape`/`resolve_name` bind the surface value
//!   to the canonical parameter — the transform's *application* is the
//!   executor's (the monitor needs the binding, i.e. which canonical parameter
//!   the value feeds, for scope and coverage — never the re-rendered text);
//! - a *scope-bearing* canonical parameter — one a [`ScopeBindings`] entry
//!   selects through `param_path` — must be bound; an unbound scope-bearing
//!   parameter ⇒ [`ArgError::UnscopedParameter`] (ADR-0087 D3);
//! - `scope_bindings_unknown` ⇒ no canonical scope exists — only a `*` grant
//!   covers (`scope_covers` returns the `*`-only verdict on `None`).
//!
//! `CanonicalArgs` is the record `authorize` step 2's coverage and step 3's
//! assessors read; surface field *names* never reach the decision (surface
//! renaming is decision-invariant — AC-R-2.8.1-10).

use std::collections::BTreeMap;

use hh_compiler::equiv::{ArgTransform, SurfaceBinding};
use hh_hir::records::ScopeBindings;
use hh_wire::json::Json;

/// The arg-resolution error sum — both members are `DenyReason`s verbatim
/// (§5g.1 §3; the `DenyReason` spelling is the refusal identity, ADR-0052 D3).
#[derive(Debug, Clone, PartialEq)]
pub enum ArgError {
    /// A surface argument absent from the `SurfaceArgMap` (I-H5).
    UnmappedArgument {
        /// The offending surface field.
        field: String,
    },
    /// A scope-bearing parameter absent from `scope_bindings` (ADR-0087 D3) or
    /// declared but unbound in the call.
    UnscopedParameter {
        /// The parameter path that needed a binding.
        param_path: String,
    },
}

// `ScopeBinding`, `ScopeKind` and the `scope_bindings` parser live in
// `hh_hir::tools` — the one schema source (CC7; V-E1-3/4 read the same
// definition at validate/register as the monitor reads at dispatch).
pub use hh_hir::tools::{ScopeBinding, ScopeKind};

/// Parse a `ScopeBindings` record into the binding list. `Bindings(json)` is
/// `[{param_path, scope_kind, canonicalization?}]`; `Unknown` ⇒ `None` (the
/// `scope_bindings_unknown` member — `resolve_scope` then matches `*` only).
/// A malformed member also yields `None` at run time — fail-closed (`*`-only
/// coverage); V-E1 refuses the malformed member at validate/register, so this
/// branch is defence in depth, not a silent drop (CC3 is enforced upstream).
pub fn scope_bindings(s: &ScopeBindings) -> Option<Vec<ScopeBinding>> {
    hh_hir::tools::scope_bindings(s).ok().flatten()
}

/// The canonical-parameter projection of a call — `param_path → value` (the
/// value *bound* through the map; transform application is the executor's).
#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalArgs {
    /// `capability_param → surface value` for every mapped argument.
    pub params: BTreeMap<String, Json>,
    /// The scope-bearing parameters a `scope_bindings` row selected —
    /// `param_path → canonical value` (the coverage matcher's operands).
    pub scoped: BTreeMap<String, Json>,
    /// `true` when the capability declared `scope_bindings_unknown`.
    pub scope_unknown: bool,
}

/// `eval(binding, scope_bindings, surface_args) → CanonicalArgs` — the
/// SurfaceArgMap evaluator. Every member of `surface_args` must be a key of
/// `binding.arg_map` (`UnmappedArgument`); every scope-bearing `param_path`
/// must resolve to a bound canonical parameter (`UnscopedParameter`).
pub fn eval(
    binding: &SurfaceBinding,
    bindings: &ScopeBindings,
    surface_args: &Json,
) -> Result<CanonicalArgs, ArgError> {
    let Json::Obj(args) = surface_args else {
        // A non-object argument record is a malformed proposal — every member
        // is unmapped (the first unnamed field reported).
        return Err(ArgError::UnmappedArgument {
            field: "<args>".to_string(),
        });
    };
    let mut params: BTreeMap<String, Json> = BTreeMap::new();
    // `const` entries contribute their fixed value with no surface argument.
    for (field, entry) in &binding.arg_map {
        if let ArgTransform::Const(v) = &entry.transform {
            params.insert(entry.capability_param.clone(), v.clone());
        }
        let _ = field;
    }
    for (field, value) in args {
        let Some(entry) = binding.arg_map.get(field) else {
            return Err(ArgError::UnmappedArgument {
                field: field.clone(),
            });
        };
        let root = entry
            .capability_param
            .split('.')
            .next()
            .unwrap_or(entry.capability_param.as_str())
            .to_string();
        match &entry.transform {
            // The value-preserving binds: identity, rename, coerce (lossless or
            // declared-loss — the loss is declared at compile, not evaluated),
            // and the executor-applied transforms (parse/unescape/resolve_name)
            // — the binding is what the decision reads.
            ArgTransform::Identity
            | ArgTransform::Rename
            | ArgTransform::Coerce { .. }
            | ArgTransform::Project
            | ArgTransform::Parse { .. }
            | ArgTransform::Unescape { .. }
            | ArgTransform::ResolveName { .. } => {
                params.insert(root, value.clone());
            }
            ArgTransform::Const(_) => {
                // A `const` entry takes no surface argument — the field is
                // mapped but its value is fixed; a supplied value is ignored
                // (the map fixed it at compile).
            }
        }
    }
    // Scope-bearing parameters: each `scope_bindings` row's `param_path` must
    // resolve in `params` (root-segment match — a `project` path binds its
    // root parameter).
    let mut scoped = BTreeMap::new();
    let mut scope_unknown = false;
    match scope_bindings(bindings) {
        None => scope_unknown = true,
        Some(rows) => {
            for b in rows {
                let root = b
                    .param_path
                    .split('.')
                    .next()
                    .unwrap_or(b.param_path.as_str());
                match params.get(root) {
                    Some(v) => {
                        scoped.insert(b.param_path.clone(), v.clone());
                    }
                    None => {
                        return Err(ArgError::UnscopedParameter {
                            param_path: b.param_path.clone(),
                        })
                    }
                }
            }
        }
    }
    Ok(CanonicalArgs {
        params,
        scoped,
        scope_unknown,
    })
}

/// `scope_covers(grant_scope, canonical_scope)` — the interim `ResourcePattern`
/// matcher (ADR-0212/OQ-132: the closed grammar is open; the ratified interim
/// is literal equality, trailing-`/*` prefix containment, and `*` covering
/// every canonical scope). `canonical_scope = None` (a `scope_bindings_unknown`
/// capability, or a scope-bearing parameter that didn't resolve) matches `*`
/// **only** (ADR-0087 D3 — "coverable only by unscoped grants").
pub fn scope_covers(grant_scope: &str, canonical_scope: Option<&str>) -> bool {
    if grant_scope == "*" {
        return true;
    }
    let Some(scope) = canonical_scope else {
        return false;
    };
    if grant_scope == scope {
        return true;
    }
    if let Some(prefix) = grant_scope.strip_suffix("/*") {
        return scope.starts_with(&format!("{prefix}/")) || scope == prefix;
    }
    false
}

/// The grant's scope covers the effect when **every** scope-bearing parameter's
/// canonical value is covered — a scope-binding set selects the resource tuple
/// the grant must dominate (I-H5). With no scope bindings (and not `unknown`),
/// the grant's scope matches only `*` or the absence of a scoped parameter.
pub fn grant_scope_covers(grant_scope: &str, args: &CanonicalArgs) -> bool {
    if args.scope_unknown {
        return grant_scope == "*";
    }
    if args.scoped.is_empty() {
        // No scope-bearing parameter bound — the grant's scope must be the
        // unscoped `*` to cover (an empty resource tuple matches nothing
        // narrower).
        return grant_scope == "*";
    }
    args.scoped
        .values()
        .all(|v| scope_covers(grant_scope, v.as_str()))
}
