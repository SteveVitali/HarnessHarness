//! The **C2 information-flow slice** (§5g.2; R-2.8.2; ADR-0054/0055/0056) — the
//! one typed home for the `FlowContract` member on `ToolCapability`, the
//! prospective label `L⁺(p)`, result admission, `EnforcementClass`, the
//! per-basis effects on `taint`/`readers`, D-ROBUST, the closed `Remedy` set,
//! and the closed, total, bounded flow-policy language.
//!
//! One schema source (CC7): [`FlowContract::to_json`]/[`FlowContract::from_json`]
//! own the `flow_contract` member's shape — `hh-hir` stores the canonical
//! `Json` and validates through this module; the monitor evaluates through it.
//! Every refusal is a typed [`FlowError`] — never a warning (ADR-0033 D7).
//!
//! Language contract (I-F7): conditions are quantifier-free over atoms with
//! bounded `∀` iteration only, non-recursive, total, and **constant-time per
//! atom** — `And`/`Or`/`ForAll` evaluate every operand (no secret-dependent
//! early exit; AC-R-2.8.2-14 measures it). A malformed or unevaluable atom
//! fails the whole evaluation as [`FlowError::EvaluationError`] (the caller
//! maps it to `deny`).

use std::collections::{BTreeMap, BTreeSet};

use hh_wire::json::Json;

use crate::authority::{AuthorityClass, ReaderSet, TaintTag};
use crate::decode::DecodeError;
use crate::endorse::{EndorsementBasis, EndorsementError};
use crate::label::Label;

// ── EnforcementClass (I-F1; ADR-0054 D5) ─────────────────────────────────────

/// `EnforcementClass ∈ {deterministic, advisory}` — carried on every check and
/// every decision input (I-F1). An `advisory` input may only *raise*
/// restriction (no-sensitive-upgrade): a deny applies directly; an
/// allow/declassify/sanitize under advisory evidence is a declassification and
/// must pass an endorsement point — the evaluator refuses to honor it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnforcementClass {
    /// Lattice ops, `ctx`, prospective label, admission, per-parameter labels,
    /// endorsement legitimacy — recorded inputs computed by the kernel.
    Deterministic,
    /// Model self-reports, judge verdicts, permissive label minimization,
    /// semantic-taint detectors, model-drafted policies.
    Advisory,
}

impl EnforcementClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EnforcementClass::Deterministic => "deterministic",
            EnforcementClass::Advisory => "advisory",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<EnforcementClass> {
        match s {
            "deterministic" => Some(EnforcementClass::Deterministic),
            "advisory" => Some(EnforcementClass::Advisory),
            _ => None,
        }
    }

    /// Whether a decision of this class may *lower* a label / widen a flow
    /// (no-sensitive-upgrade: advisory evidence never widens).
    pub fn may_widen(self) -> bool {
        matches!(self, EnforcementClass::Deterministic)
    }
}

// ── TaintTagPattern (contribution.taint_tags / bounds.removes / taint ⊆ T) ───

/// A `TaintTag` pattern — the closed pattern sum used by `contribution.taint_
/// tags`, `taint ⊆ T` atoms and sanitizer `removes` sets. `SelfTool` is the
/// `{tool}` marker: the tag instantiated with the calling capability's id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaintTagPattern {
    /// `{tool}` — the invoking capability's own `tool:<cap>` tag.
    SelfTool,
    /// An exact `TaintTag::as_string()` spelling (`tool:<cap>[+<inner>]`,
    /// `participant:<p>`, `import:<sys>`, `extension:<id>`).
    Exact(String),
    /// A `prefix:*` family (`import:*`, `secret:*`, `tool:*`, …) — suffix `:*`
    /// on the wire, matches any `as_string()` carrying the prefix.
    Prefix(String),
}

impl TaintTagPattern {
    /// The canonical JSON (`"self"` | `"<exact-spelling>"` | `"<prefix>:*"`).
    pub fn to_json(&self) -> Json {
        match self {
            TaintTagPattern::SelfTool => Json::str("self"),
            TaintTagPattern::Exact(t) => Json::str(t.clone()),
            TaintTagPattern::Prefix(p) => Json::str(format!("{p}:*")),
        }
    }

    /// Parse the pattern spelling; malformed members are refused.
    pub fn from_json(j: &Json, path: &str) -> Result<TaintTagPattern, DecodeError> {
        let s = j.as_str().ok_or_else(|| DecodeError {
            detail: format!("{path} must be a string"),
        })?;
        if s == "self" {
            return Ok(TaintTagPattern::SelfTool);
        }
        if let Some(p) = s.strip_suffix(":*") {
            if p.is_empty() || !is_tag_family(p) {
                // A prefix must name a tag family — refuse patterns that can
                // never match a tag spelling.
                return Err(DecodeError {
                    detail: format!("{path}: unknown taint family {p}"),
                });
            }
            return Ok(TaintTagPattern::Prefix(p.to_string()));
        }
        if TaintTag::parse(s).is_none() {
            return Err(DecodeError {
                detail: format!("{path}: unparseable taint tag {s}"),
            });
        }
        Ok(TaintTagPattern::Exact(s.to_string()))
    }

    /// Whether the pattern matches `tag` under `capability` (the `self`
    /// instantiation — `capability` is the capability's version id).
    pub fn matches(&self, tag: &TaintTag, capability: &str) -> bool {
        match self {
            TaintTagPattern::SelfTool => matches!(
                tag,
                TaintTag::Tool {
                    capability: c,
                    ..
                } if c == capability
            ),
            TaintTagPattern::Exact(t) => tag.as_string() == *t,
            TaintTagPattern::Prefix(p) => tag.as_string().starts_with(&format!("{p}:")),
        }
    }

    /// Instantiate the pattern into the concrete `TaintTag`s it contributes.
    /// `SelfTool` mints `tool:<capability>`; `Exact` yields its tag; a `Prefix`
    /// contributes nothing (a family is a *match* form, not a minted tag).
    pub fn instantiate(&self, capability: &str) -> Option<TaintTag> {
        match self {
            TaintTagPattern::SelfTool => Some(TaintTag::Tool {
                capability: capability.to_string(),
                inner_source: None,
            }),
            TaintTagPattern::Exact(t) => TaintTag::parse(t),
            TaintTagPattern::Prefix(_) => None,
        }
    }
}

/// The tag families a `prefix:*` pattern may name — `tool`, `participant`,
/// `import`, `extension` (the four §8.1 sources) plus `secret` (the R-2.8.3
/// `secret:*` family a sanitizer's `removes` set may name ahead of the
/// semantic-taint stage; it matches no `TaintTag` variant yet).
fn is_tag_family(p: &str) -> bool {
    matches!(
        p,
        "tool" | "participant" | "import" | "extension" | "secret"
    )
}

// ── ReadersFrom (contribution.readers_from) ──────────────────────────────────

/// `readers_from ∈ {reads | declared(set<PrincipalRef>) | Public}` — where the
/// declared result's readers derive from (§5g.2 §3 `FlowContract`). `Reads`
/// derives from the joined input labels (the resource-reader join lands with
/// resource provenance — OQ-103's single-principal stage carries it as the
/// input join).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadersFrom {
    /// Readers derive from what the tool read — the joined input readers.
    Reads,
    /// A declared `PrincipalRef` set the result is addressed to.
    Declared(BTreeSet<String>),
    /// Public — the join identity (no narrowing).
    Public,
}

impl ReadersFrom {
    /// The canonical JSON (`"reads"` | `{"declared":[…]}` | `"public"`).
    pub fn to_json(&self) -> Json {
        match self {
            ReadersFrom::Reads => Json::str("reads"),
            ReadersFrom::Public => Json::str("public"),
            ReadersFrom::Declared(rs) => Json::obj([(
                "declared",
                Json::Arr(rs.iter().map(|r| Json::str(r.clone())).collect()),
            )]),
        }
    }

    /// Parse the canonical form.
    pub fn from_json(j: &Json, path: &str) -> Result<ReadersFrom, DecodeError> {
        match j.as_str() {
            Some("reads") => Ok(ReadersFrom::Reads),
            Some("public") => Ok(ReadersFrom::Public),
            _ => {
                if let Some(Json::Arr(items)) = j.get("declared") {
                    let mut out = BTreeSet::new();
                    for i in items {
                        out.insert(i.as_str().map(str::to_string).ok_or_else(|| DecodeError {
                            detail: format!("{path}.declared members must be strings"),
                        })?);
                    }
                    Ok(ReadersFrom::Declared(out))
                } else {
                    Err(DecodeError {
                        detail: format!("{path} must be reads|public|{{declared:[…]}}"),
                    })
                }
            }
        }
    }

    /// The `ReaderSet` this contributes to the join — `Reads`/`Public` are the
    /// ∩ identity (`Public`); `Declared(s)` narrows to `s`.
    pub fn reader_set(&self) -> ReaderSet {
        match self {
            ReadersFrom::Reads | ReadersFrom::Public => ReaderSet::Public,
            ReadersFrom::Declared(s) => ReaderSet::Restricted(s.clone()),
        }
    }
}

/// `contribution{taint_tags, readers_from}` — the declared `d_τ` a composite /
/// open-world capability contributes to the prospective label (§5g.2 §2.2;
/// ADR-0054 D2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contribution {
    /// The taint patterns the tool's result carries (`self` = `tool:<cap>`).
    pub taint_tags: Vec<TaintTagPattern>,
    /// Where the result's readers derive from.
    pub readers_from: ReadersFrom,
}

impl Contribution {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "taint_tags",
                Json::Arr(self.taint_tags.iter().map(|t| t.to_json()).collect()),
            ),
            ("readers_from", self.readers_from.to_json()),
        ])
    }

    /// Parse the canonical form — fails closed on a non-object or an unknown
    /// member (the grammar is closed).
    pub fn from_json(j: &Json, path: &str) -> Result<Contribution, DecodeError> {
        if let Json::Obj(m) = j {
            for k in m.keys() {
                if k != "taint_tags" && k != "readers_from" {
                    return Err(DecodeError {
                        detail: format!("{path}: unknown member {k}"),
                    });
                }
            }
        } else {
            return Err(DecodeError {
                detail: format!("{path} must be an object"),
            });
        }
        let tags = match j.get("taint_tags") {
            Some(Json::Arr(items)) => items
                .iter()
                .enumerate()
                .map(|(i, t)| TaintTagPattern::from_json(t, &format!("{path}.taint_tags[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => {
                return Err(DecodeError {
                    detail: format!("{path}.taint_tags must be an array"),
                })
            }
            None => Vec::new(),
        };
        let readers_from = match j.get("readers_from") {
            Some(r) => ReadersFrom::from_json(r, &format!("{path}.readers_from"))?,
            None => ReadersFrom::Reads,
        };
        Ok(Contribution {
            taint_tags: tags,
            readers_from,
        })
    }
}

/// `L⁺ = L(p) ⊔ d_τ` — the contribution join (ADR-0054 D1). `d_τ` never
/// carries authority (a contribution cannot raise or lower it): the join is
/// `taint ∪ instantiated(taint_tags)` and `readers ∩ readers_from`, authority
/// unchanged — the `kernel`-authority identity under `min`.
pub fn apply_contribution(l: &Label, d: &Contribution, capability: &str) -> Label {
    let taint: BTreeSet<TaintTag> = d
        .taint_tags
        .iter()
        .filter_map(|t| t.instantiate(capability))
        .collect();
    Label {
        authority: l.authority,
        taint: l.taint.union(&taint).cloned().collect(),
        readers: l.readers.intersect(&d.readers_from.reader_set()),
    }
}

/// `L(p) = ctx ⊔ ⊔ L(args)` then `L⁺(p) = L(p) ⊔ d_τ` (§5g.2 §2). `arg_labels`
/// is the per-parameter label join over the canonical (post-`SurfaceArgMap`)
/// parameters — a handle argument contributes its handle's label.
pub fn prospective_label(
    ctx: &Label,
    arg_labels: impl IntoIterator<Item = Label>,
    contribution: &Contribution,
    capability: &str,
) -> Label {
    let l_p = arg_labels
        .into_iter()
        .fold(ctx.clone(), |acc, l| acc.join(&l));
    apply_contribution(&l_p, contribution, capability)
}

// ── The closed flow-policy language (I-F7; ADR-0056 D3/D4) ───────────────────

/// The depth bound the decoder enforces on `FlowCond` nesting (grammar
/// totality — recursion depth is bounded at parse, never at eval).
pub const COND_MAX_DEPTH: usize = 16;
/// The node bound on a decoded `FlowCond` (atoms + connectives).
pub const COND_MAX_NODES: usize = 256;

/// Which label a label-reading atom evaluates — the prospective label `L⁺`
/// (default), a named canonical parameter, or the innermost `∀`-bound param.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// `L⁺(p)` — the prospective label.
    Proposal,
    /// `L(param)` — a named canonical parameter's label.
    Param(String),
    /// The innermost `forall_param`-bound parameter.
    Bound,
}

/// `FlowCond` — the closed predicate grammar (I-F7): atoms over the
/// prospective label, per-parameter labels, canonical arguments, the effect
/// class, the committed-effect projection and recorded detector verdicts;
/// connectives `∧ ∨ ¬`; iteration only the built-in `∀` over named params and
/// over the committed projection. No variables beyond `p`, no user functions,
/// no recursion, no string concatenation, no arithmetic beyond comparison and
/// bounded counts.
#[derive(Debug, Clone, PartialEq)]
pub enum FlowCond {
    /// `authority(x) ≥ c`.
    AuthorityAtLeast {
        /// The subject label.
        subject: Subject,
        /// The floor class.
        class: AuthorityClass,
    },
    /// `authority(x) ≤ c`.
    AuthorityAtMost {
        /// The subject label.
        subject: Subject,
        /// The ceiling class.
        class: AuthorityClass,
    },
    /// `taint(x) ⊆ T`.
    TaintSubset {
        /// The subject label.
        subject: Subject,
        /// The admitted tag patterns.
        tags: Vec<TaintTagPattern>,
    },
    /// `taint(x) = ∅`.
    TaintEmpty {
        /// The subject label.
        subject: Subject,
    },
    /// `readers(x) ⊇ recipients(p)`.
    ReadersCoverRecipients {
        /// The subject label.
        subject: Subject,
    },
    /// `readers(x) = Public`.
    ReadersPublic {
        /// The subject label.
        subject: Subject,
    },
    /// `arg(param) = literal` — canonical-value equality.
    ArgEq {
        /// The canonical parameter path.
        param: String,
        /// The literal.
        value: Json,
    },
    /// `arg(param) ∈ literals`.
    ArgIn {
        /// The canonical parameter path.
        param: String,
        /// The literal set.
        values: Vec<Json>,
    },
    /// `arg(param) like glob` — `*`/`?` glob over a string argument (no regex —
    /// the grammar forbids embedded code).
    ArgLike {
        /// The canonical parameter path.
        param: String,
        /// The glob pattern.
        pattern: String,
    },
    /// `arg(param) < n`.
    ArgLt {
        /// The canonical parameter path.
        param: String,
        /// The integer bound (exclusive).
        value: i64,
    },
    /// `arg(param) ≤ n`.
    ArgLe {
        /// The canonical parameter path.
        param: String,
        /// The integer bound (inclusive).
        value: i64,
    },
    /// `EffectClass(p) matches {domain?, world?}`.
    EffectMatches {
        /// The required domain spelling (`None` = any).
        domain: Option<String>,
        /// The required world (`open`/`closed`; `None` = any).
        world: Option<String>,
    },
    /// `committed(pattern)` — ∃ a committed effect matching `{domain?}` in the
    /// run's `EffectProjection`.
    Committed {
        /// The required domain spelling (`None` = any committed effect).
        domain: Option<String>,
    },
    /// `count(committed(pattern)) ≤ n`.
    CountCommittedAtMost {
        /// The counted domain (`None` = all committed effects).
        domain: Option<String>,
        /// The bound.
        n: u64,
    },
    /// `detector(validator_ref, param)` — reads the *recorded* deterministic
    /// detector verdict (`true` = the detector fired on `param`'s value). A
    /// missing verdict is an [`FlowError::EvaluationError`], never `false`.
    Detector {
        /// The validator's ref.
        validator_ref: String,
        /// The canonical parameter the verdict covers.
        param: String,
    },
    /// `∧` — all operands evaluated (no secret-dependent early exit).
    And(Vec<FlowCond>),
    /// `∨` — all operands evaluated.
    Or(Vec<FlowCond>),
    /// `¬`.
    Not(Box<FlowCond>),
    /// `∀ param ∈ params` — the bound param's label is the inner subject
    /// (`Subject::Bound`); `arg`-atoms may still read named params.
    ForAllParam {
        /// The canonical parameters iterated (the declared list).
        params: Vec<String>,
        /// The body.
        cond: Box<FlowCond>,
    },
    /// `∀ e ∈ effects(r)` matching `domain` — the bound effect's domain is
    /// read by [`FlowCond::BoundEffectDomain`].
    EveryCommitted {
        /// The domain filter (`None` = every committed effect).
        domain: Option<String>,
        /// The body.
        cond: Box<FlowCond>,
    },
    /// The bound committed effect's domain ∈ `domains` — valid only inside
    /// `EveryCommitted`.
    BoundEffectDomain {
        /// The admitted domain spellings.
        domains: Vec<String>,
    },
}

/// The evaluation-time failure of a condition — every atom that cannot
/// produce a definite verdict (an unbound `arg`, a missing detector verdict,
/// a `Bound` subject outside `∀`) yields this, and the caller denies with
/// `EvaluationError` (§5g.2 §5 "totality makes any evaluation error
/// `deny(EvaluationError)`").
#[derive(Debug, Clone, PartialEq)]
pub struct EvalError {
    /// The atom and why it failed.
    pub detail: String,
}

/// The recorded input surface `eval`/`check_flow` read — every member a
/// canonical record (I-H1: no `Text`, no re-read of tool output).
#[derive(Debug)]
pub struct FlowInput<'a> {
    /// `L⁺(p)` — the prospective label.
    pub l_plus: &'a Label,
    /// Canonical parameter path → label (post-`SurfaceArgMap`).
    pub param_labels: &'a BTreeMap<String, Label>,
    /// Canonical parameter path → bound value.
    pub args: &'a BTreeMap<String, Json>,
    /// `recipients(p)` — resolved through `resolve_recipients`.
    pub recipients: &'a BTreeSet<String>,
    /// The declared effect domain spelling.
    pub domain: &'a str,
    /// The declared `world` (`open`/`closed` spelling).
    pub world: &'a str,
    /// `project(run, effects, until_seq)` — the committed projection.
    pub committed: &'a [CommittedEffect],
    /// Recorded deterministic detector verdicts — `"<validator_ref>:<param>"`.
    pub detectors: &'a BTreeMap<String, bool>,
    /// The capability's identity (the `self` taint-pattern instantiation).
    pub capability: &'a str,
}

/// One member of the committed-effect projection
/// `project(run, effects, until_seq)` (§5g.2 §3 `EffectProjection`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedEffect {
    /// The committed class's domain spelling.
    pub domain: String,
    /// The committed `args_canonical_hash`.
    pub args_hash: String,
}

impl CommittedEffect {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("domain", Json::str(self.domain.clone())),
            ("args_hash", Json::str(self.args_hash.clone())),
        ])
    }

    /// Parse the canonical form.
    pub fn from_json(j: &Json, path: &str) -> Result<CommittedEffect, DecodeError> {
        let domain = j
            .get("domain")
            .and_then(Json::as_str)
            .ok_or_else(|| DecodeError {
                detail: format!("{path}.domain missing"),
            })?
            .to_string();
        let args_hash = j
            .get("args_hash")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        Ok(CommittedEffect { domain, args_hash })
    }
}

/// The subject the evaluator resolves `Subject`/`Bound` against — the inner
/// bound param's label and the inner bound committed effect.
struct Bound<'a> {
    param_label: Option<&'a Label>,
    effect: Option<&'a CommittedEffect>,
}

fn subject_label<'a>(
    s: &Subject,
    input: &'a FlowInput<'a>,
    bound: &Bound<'a>,
) -> Result<&'a Label, EvalError> {
    match s {
        Subject::Proposal => Ok(input.l_plus),
        Subject::Param(p) => input.param_labels.get(p).ok_or_else(|| EvalError {
            detail: format!("param label {p} not recorded"),
        }),
        Subject::Bound => bound.param_label.ok_or_else(|| EvalError {
            detail: "bound subject outside forall_param".to_string(),
        }),
    }
}

/// `eval(cond, input)` — total over the closed grammar. Every operand of a
/// connective is evaluated (no early exit — the policy-timing channel is a
/// secret-dependent shortcut, I-F7); the first error encountered in document
/// order is the result's error.
pub fn eval_cond(cond: &FlowCond, input: &FlowInput) -> Result<bool, EvalError> {
    eval_inner(
        cond,
        input,
        &Bound {
            param_label: None,
            effect: None,
        },
    )
}

fn eval_inner(cond: &FlowCond, input: &FlowInput, bound: &Bound) -> Result<bool, EvalError> {
    match cond {
        FlowCond::AuthorityAtLeast { subject, class } => {
            Ok(subject_label(subject, input, bound)?.authority >= *class)
        }
        FlowCond::AuthorityAtMost { subject, class } => {
            Ok(subject_label(subject, input, bound)?.authority <= *class)
        }
        FlowCond::TaintSubset { subject, tags } => {
            let l = subject_label(subject, input, bound)?;
            Ok(l.taint
                .iter()
                .all(|t| tags.iter().any(|p| p.matches(t, input.capability))))
        }
        FlowCond::TaintEmpty { subject } => {
            Ok(subject_label(subject, input, bound)?.taint.is_empty())
        }
        FlowCond::ReadersCoverRecipients { subject } => {
            let l = subject_label(subject, input, bound)?;
            Ok(match &l.readers {
                ReaderSet::Public => true,
                ReaderSet::Restricted(rs) => input.recipients.iter().all(|r| rs.contains(r)),
            })
        }
        FlowCond::ReadersPublic { subject } => Ok(matches!(
            subject_label(subject, input, bound)?.readers,
            ReaderSet::Public
        )),
        FlowCond::ArgEq { param, value } => {
            let v = input.args.get(param).ok_or_else(|| EvalError {
                detail: format!("arg {param} unbound"),
            })?;
            Ok(v == value)
        }
        FlowCond::ArgIn { param, values } => {
            let v = input.args.get(param).ok_or_else(|| EvalError {
                detail: format!("arg {param} unbound"),
            })?;
            Ok(values.iter().any(|x| x == v))
        }
        FlowCond::ArgLike { param, pattern } => {
            let v = input.args.get(param).ok_or_else(|| EvalError {
                detail: format!("arg {param} unbound"),
            })?;
            let s = v.as_str().ok_or_else(|| EvalError {
                detail: format!("arg {param} not a string"),
            })?;
            Ok(glob_match(pattern, s))
        }
        FlowCond::ArgLt { param, value } => {
            let v = input
                .args
                .get(param)
                .and_then(Json::as_int)
                .ok_or_else(|| EvalError {
                    detail: format!("arg {param} not an int"),
                })?;
            Ok(v < *value)
        }
        FlowCond::ArgLe { param, value } => {
            let v = input
                .args
                .get(param)
                .and_then(Json::as_int)
                .ok_or_else(|| EvalError {
                    detail: format!("arg {param} not an int"),
                })?;
            Ok(v <= *value)
        }
        FlowCond::EffectMatches { domain, world } => {
            Ok(domain.as_deref().map(|d| d == input.domain).unwrap_or(true)
                && world.as_deref().map(|w| w == input.world).unwrap_or(true))
        }
        FlowCond::Committed { domain } => Ok(input
            .committed
            .iter()
            .any(|e| domain.as_deref().map(|d| d == e.domain).unwrap_or(true))),
        FlowCond::CountCommittedAtMost { domain, n } => {
            let c = input
                .committed
                .iter()
                .filter(|e| domain.as_deref().map(|d| d == e.domain).unwrap_or(true))
                .count() as u64;
            Ok(c <= *n)
        }
        FlowCond::Detector {
            validator_ref,
            param,
        } => input
            .detectors
            .get(&format!("{validator_ref}:{param}"))
            .copied()
            .ok_or_else(|| EvalError {
                detail: format!("detector verdict {validator_ref}:{param} not recorded"),
            }),
        FlowCond::And(parts) | FlowCond::Or(parts) => {
            let is_and = matches!(cond, FlowCond::And(_));
            let mut err: Option<EvalError> = None;
            let mut acc = is_and;
            for p in parts {
                match eval_inner(p, input, bound) {
                    Ok(v) => acc = if is_and { acc && v } else { acc || v },
                    Err(e) => {
                        if err.is_none() {
                            err = Some(e);
                        }
                    }
                }
            }
            match err {
                Some(e) => Err(e),
                None => Ok(acc),
            }
        }
        FlowCond::Not(inner) => eval_inner(inner, input, bound).map(|v| !v),
        FlowCond::ForAllParam { params, cond } => {
            let mut err: Option<EvalError> = None;
            let mut acc = true;
            for p in params {
                let l = input.param_labels.get(p).ok_or_else(|| EvalError {
                    detail: format!("param label {p} not recorded"),
                });
                match l {
                    Ok(l) => {
                        let b = Bound {
                            param_label: Some(l),
                            effect: bound.effect,
                        };
                        match eval_inner(cond, input, &b) {
                            Ok(v) => acc = acc && v,
                            Err(e) => {
                                if err.is_none() {
                                    err = Some(e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if err.is_none() {
                            err = Some(e);
                        }
                    }
                }
            }
            match err {
                Some(e) => Err(e),
                None => Ok(acc),
            }
        }
        FlowCond::EveryCommitted { domain, cond } => {
            let mut err: Option<EvalError> = None;
            let mut acc = true;
            for e in input.committed {
                if !domain.as_deref().map(|d| d == e.domain).unwrap_or(true) {
                    continue;
                }
                let b = Bound {
                    param_label: bound.param_label,
                    effect: Some(e),
                };
                match eval_inner(cond, input, &b) {
                    Ok(v) => acc = acc && v,
                    Err(e) => {
                        if err.is_none() {
                            err = Some(e);
                        }
                    }
                }
            }
            match err {
                Some(e) => Err(e),
                None => Ok(acc),
            }
        }
        FlowCond::BoundEffectDomain { domains } => {
            let e = bound.effect.ok_or_else(|| EvalError {
                detail: "bound effect outside every_committed".to_string(),
            })?;
            Ok(domains.contains(&e.domain))
        }
    }
}

/// The bounded glob matcher for `arg like` — `*` (any run) and `?` (one
/// char); literal otherwise. Iterative, no recursion on pattern structure.
fn glob_match(pattern: &str, s: &str) -> bool {
    // Classic two-pointer glob with backtracking — O(|pattern|·|s|) worst
    // case, both bounded by the record's size limits.
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = s.chars().collect();
    let (mut i, mut j) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while j < t.len() {
        if i < p.len() && (p[i] == '?' || p[i] == t[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = i;
            mark = j;
            i += 1;
        } else if star != usize::MAX {
            i = star + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}

impl FlowCond {
    /// The canonical JSON — one member key per atom/connective.
    pub fn to_json(&self) -> Json {
        let subj = |s: &Subject| -> Option<(&'static str, Json)> {
            match s {
                Subject::Proposal => None,
                Subject::Param(p) => Some(("subject", Json::str(format!("param:{p}")))),
                Subject::Bound => Some(("subject", Json::str("bound"))),
            }
        };
        let with = |k: &'static str, v: Json, s: &Subject| -> Json {
            let mut pairs = vec![(k, v)];
            if let Some((sk, sv)) = subj(s) {
                pairs.push((sk, sv));
            }
            Json::obj(pairs)
        };
        match self {
            FlowCond::AuthorityAtLeast { subject, class } => {
                with("authority_at_least", Json::str(class.as_str()), subject)
            }
            FlowCond::AuthorityAtMost { subject, class } => {
                with("authority_at_most", Json::str(class.as_str()), subject)
            }
            FlowCond::TaintSubset { subject, tags } => with(
                "taint_subset",
                Json::Arr(tags.iter().map(|t| t.to_json()).collect()),
                subject,
            ),
            FlowCond::TaintEmpty { subject } => with("taint_empty", Json::Bool(true), subject),
            FlowCond::ReadersCoverRecipients { subject } => {
                with("readers_cover_recipients", Json::Bool(true), subject)
            }
            FlowCond::ReadersPublic { subject } => {
                with("readers_public", Json::Bool(true), subject)
            }
            FlowCond::ArgEq { param, value } => Json::obj([(
                "arg_eq",
                Json::obj([
                    ("param", Json::str(param.clone())),
                    ("value", value.clone()),
                ]),
            )]),
            FlowCond::ArgIn { param, values } => Json::obj([(
                "arg_in",
                Json::obj([
                    ("param", Json::str(param.clone())),
                    ("values", Json::Arr(values.clone())),
                ]),
            )]),
            FlowCond::ArgLike { param, pattern } => Json::obj([(
                "arg_like",
                Json::obj([
                    ("param", Json::str(param.clone())),
                    ("pattern", Json::str(pattern.clone())),
                ]),
            )]),
            FlowCond::ArgLt { param, value } => Json::obj([(
                "arg_lt",
                Json::obj([
                    ("param", Json::str(param.clone())),
                    ("value", Json::Int(*value)),
                ]),
            )]),
            FlowCond::ArgLe { param, value } => Json::obj([(
                "arg_le",
                Json::obj([
                    ("param", Json::str(param.clone())),
                    ("value", Json::Int(*value)),
                ]),
            )]),
            FlowCond::EffectMatches { domain, world } => {
                let mut m = Vec::new();
                if let Some(d) = domain {
                    m.push(("domain", Json::str(d.clone())));
                }
                if let Some(w) = world {
                    m.push(("world", Json::str(w.clone())));
                }
                Json::obj([("effect_matches", Json::obj(m))])
            }
            FlowCond::Committed { domain } => {
                let mut m = Vec::new();
                if let Some(d) = domain {
                    m.push(("domain", Json::str(d.clone())));
                }
                Json::obj([("committed", Json::obj(m))])
            }
            FlowCond::CountCommittedAtMost { domain, n } => {
                let mut m = vec![("n", Json::Int(*n as i64))];
                if let Some(d) = domain {
                    m.push(("domain", Json::str(d.clone())));
                }
                Json::obj([("count_committed_at_most", Json::obj(m))])
            }
            FlowCond::Detector {
                validator_ref,
                param,
            } => Json::obj([(
                "detector",
                Json::obj([
                    ("validator_ref", Json::str(validator_ref.clone())),
                    ("param", Json::str(param.clone())),
                ]),
            )]),
            FlowCond::And(parts) => Json::obj([(
                "and",
                Json::Arr(parts.iter().map(|c| c.to_json()).collect()),
            )]),
            FlowCond::Or(parts) => {
                Json::obj([("or", Json::Arr(parts.iter().map(|c| c.to_json()).collect()))])
            }
            FlowCond::Not(inner) => Json::obj([("not", inner.to_json())]),
            FlowCond::ForAllParam { params, cond } => Json::obj([(
                "forall_param",
                Json::obj([
                    (
                        "params",
                        Json::Arr(params.iter().map(|p| Json::str(p.clone())).collect()),
                    ),
                    ("cond", cond.to_json()),
                ]),
            )]),
            FlowCond::EveryCommitted { domain, cond } => {
                let mut m = vec![("cond", cond.to_json())];
                if let Some(d) = domain {
                    m.push(("domain", Json::str(d.clone())));
                }
                Json::obj([("every_committed", Json::obj(m))])
            }
            FlowCond::BoundEffectDomain { domains } => Json::obj([(
                "bound_effect_domain",
                Json::Arr(domains.iter().map(|d| Json::str(d.clone())).collect()),
            )]),
        }
    }

    /// Parse the canonical form — fails closed on any unknown member, with
    /// `COND_MAX_DEPTH`/`COND_MAX_NODES` enforced (grammar totality: an
    /// over-deep or over-wide condition is refused at decode, never at eval).
    pub fn from_json(j: &Json) -> Result<FlowCond, DecodeError> {
        let mut nodes = 0usize;
        Self::decode(j, "cond", 0, &mut nodes)
    }

    fn decode(
        j: &Json,
        path: &str,
        depth: usize,
        nodes: &mut usize,
    ) -> Result<FlowCond, DecodeError> {
        *nodes += 1;
        if depth > COND_MAX_DEPTH {
            return Err(DecodeError {
                detail: format!("{path}: condition depth exceeds {COND_MAX_DEPTH}"),
            });
        }
        if *nodes > COND_MAX_NODES {
            return Err(DecodeError {
                detail: format!("condition size exceeds {COND_MAX_NODES} nodes"),
            });
        }
        let err = |m: String| DecodeError {
            detail: format!("{path}: {m}"),
        };
        let subject_of = |j: &Json| -> Result<Subject, DecodeError> {
            match j.get("subject").and_then(Json::as_str) {
                None => Ok(Subject::Proposal),
                Some("bound") => Ok(Subject::Bound),
                Some(s) => s
                    .strip_prefix("param:")
                    .map(|p| Subject::Param(p.to_string()))
                    .ok_or_else(|| DecodeError {
                        detail: format!("{path}.subject must be param:<path>|bound"),
                    }),
            }
        };
        if let Some(v) = j.get("authority_at_least").and_then(Json::as_str) {
            let class = AuthorityClass::parse(v)
                .ok_or_else(|| err(format!("authority_at_least unknown {v}")))?;
            return Ok(FlowCond::AuthorityAtLeast {
                subject: subject_of(j)?,
                class,
            });
        }
        if let Some(v) = j.get("authority_at_most").and_then(Json::as_str) {
            let class = AuthorityClass::parse(v)
                .ok_or_else(|| err(format!("authority_at_most unknown {v}")))?;
            return Ok(FlowCond::AuthorityAtMost {
                subject: subject_of(j)?,
                class,
            });
        }
        if let Some(Json::Arr(items)) = j.get("taint_subset") {
            let tags = items
                .iter()
                .enumerate()
                .map(|(i, t)| TaintTagPattern::from_json(t, &format!("{path}.taint_subset[{i}]")))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(FlowCond::TaintSubset {
                subject: subject_of(j)?,
                tags,
            });
        }
        if j.get("taint_empty").is_some() {
            return Ok(FlowCond::TaintEmpty {
                subject: subject_of(j)?,
            });
        }
        if j.get("readers_cover_recipients").is_some() {
            return Ok(FlowCond::ReadersCoverRecipients {
                subject: subject_of(j)?,
            });
        }
        if j.get("readers_public").is_some() {
            return Ok(FlowCond::ReadersPublic {
                subject: subject_of(j)?,
            });
        }
        for (key, mk) in [
            ("arg_eq", 0u8),
            ("arg_in", 1u8),
            ("arg_like", 2u8),
            ("arg_lt", 3u8),
            ("arg_le", 4u8),
        ] {
            if let Some(inner) = j.get(key) {
                let param = inner
                    .get("param")
                    .and_then(Json::as_str)
                    .ok_or_else(|| err(format!("{key}.param missing")))?
                    .to_string();
                return Ok(match mk {
                    0 => FlowCond::ArgEq {
                        param,
                        value: inner
                            .get("value")
                            .cloned()
                            .ok_or_else(|| err("arg_eq.value missing".to_string()))?,
                    },
                    1 => FlowCond::ArgIn {
                        param,
                        values: match inner.get("values") {
                            Some(Json::Arr(vs)) => vs.clone(),
                            _ => return Err(err("arg_in.values must be an array".to_string())),
                        },
                    },
                    2 => FlowCond::ArgLike {
                        param,
                        pattern: inner
                            .get("pattern")
                            .and_then(Json::as_str)
                            .ok_or_else(|| err("arg_like.pattern missing".to_string()))?
                            .to_string(),
                    },
                    3 => FlowCond::ArgLt {
                        param,
                        value: inner
                            .get("value")
                            .and_then(Json::as_int)
                            .ok_or_else(|| err("arg_lt.value must be an int".to_string()))?,
                    },
                    _ => FlowCond::ArgLe {
                        param,
                        value: inner
                            .get("value")
                            .and_then(Json::as_int)
                            .ok_or_else(|| err("arg_le.value must be an int".to_string()))?,
                    },
                });
            }
        }
        if let Some(inner) = j.get("effect_matches") {
            return Ok(FlowCond::EffectMatches {
                domain: inner.get("domain").and_then(Json::as_str).map(String::from),
                world: inner.get("world").and_then(Json::as_str).map(String::from),
            });
        }
        if let Some(inner) = j.get("committed") {
            return Ok(FlowCond::Committed {
                domain: inner.get("domain").and_then(Json::as_str).map(String::from),
            });
        }
        if let Some(inner) = j.get("count_committed_at_most") {
            let n = inner
                .get("n")
                .and_then(Json::as_int)
                .ok_or_else(|| err("count_committed_at_most.n missing".to_string()))?;
            if n < 0 {
                return Err(err("count_committed_at_most.n must be ≥ 0".to_string()));
            }
            return Ok(FlowCond::CountCommittedAtMost {
                domain: inner.get("domain").and_then(Json::as_str).map(String::from),
                n: n as u64,
            });
        }
        if let Some(inner) = j.get("detector") {
            return Ok(FlowCond::Detector {
                validator_ref: inner
                    .get("validator_ref")
                    .and_then(Json::as_str)
                    .ok_or_else(|| err("detector.validator_ref missing".to_string()))?
                    .to_string(),
                param: inner
                    .get("param")
                    .and_then(Json::as_str)
                    .ok_or_else(|| err("detector.param missing".to_string()))?
                    .to_string(),
            });
        }
        for (key, is_and) in [("and", true), ("or", false)] {
            if let Some(Json::Arr(items)) = j.get(key) {
                let parts = items
                    .iter()
                    .enumerate()
                    .map(|(i, c)| Self::decode(c, &format!("{path}.{key}[{i}]"), depth + 1, nodes))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(if is_and {
                    FlowCond::And(parts)
                } else {
                    FlowCond::Or(parts)
                });
            }
        }
        if let Some(inner) = j.get("not") {
            return Ok(FlowCond::Not(Box::new(Self::decode(
                inner,
                &format!("{path}.not"),
                depth + 1,
                nodes,
            )?)));
        }
        if let Some(inner) = j.get("forall_param") {
            let params = match inner.get("params") {
                Some(Json::Arr(items)) => items
                    .iter()
                    .map(|i| {
                        i.as_str().map(str::to_string).ok_or_else(|| DecodeError {
                            detail: format!("{path}.forall_param.params members must be strings"),
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => return Err(err("forall_param.params must be an array".to_string())),
            };
            let cond = inner
                .get("cond")
                .ok_or_else(|| err("forall_param.cond missing".to_string()))?;
            return Ok(FlowCond::ForAllParam {
                params,
                cond: Box::new(Self::decode(
                    cond,
                    &format!("{path}.forall_param.cond"),
                    depth + 1,
                    nodes,
                )?),
            });
        }
        if let Some(inner) = j.get("every_committed") {
            let cond = inner
                .get("cond")
                .ok_or_else(|| err("every_committed.cond missing".to_string()))?;
            return Ok(FlowCond::EveryCommitted {
                domain: inner.get("domain").and_then(Json::as_str).map(String::from),
                cond: Box::new(Self::decode(
                    cond,
                    &format!("{path}.every_committed.cond"),
                    depth + 1,
                    nodes,
                )?),
            });
        }
        if let Some(Json::Arr(items)) = j.get("bound_effect_domain") {
            let domains = items
                .iter()
                .map(|i| {
                    i.as_str().map(str::to_string).ok_or_else(|| DecodeError {
                        detail: format!("{path}.bound_effect_domain members must be strings"),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(FlowCond::BoundEffectDomain { domains });
        }
        Err(err("malformed condition".to_string()))
    }
}

// ── FlowRule / FlowSelector / FlowDecision ───────────────────────────────────

/// `FlowRule.selector` — `{domain?, capability?, params?}`: which proposals
/// the rule's condition is evaluated against (§5g.2 §3; ADR-0056 D1). All
/// present members must match; absent members match anything.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FlowSelector {
    /// The effect domain the rule selects (`None` = every domain).
    pub domain: Option<String>,
    /// The capability semantic id the rule selects.
    pub capability: Option<String>,
    /// Canonical parameters that must be bound for the rule to apply.
    pub params: Vec<String>,
}

/// `decision ∈ {deny, allow, declassify(readers_to), sanitize(sanitizer_ref,
/// param)}` (ADR-0056 D1).
#[derive(Debug, Clone, PartialEq)]
pub enum FlowDecision {
    /// Deny — the closed `DenyReason` spelling the rule names.
    Deny {
        /// The recorded reason (a `DenyReason` spelling — the monitor maps it).
        reason: String,
    },
    /// Allow — a definition-issued positive authorization.
    Allow,
    /// `declassify(readers_to)` — widen content-param readers to the named
    /// set (`recipients` = `recipients(p)`; `named` = a literal set).
    Declassify {
        /// The target reader set.
        readers_to: ReadersTo,
    },
    /// `sanitize(sanitizer_ref, param)` — the remedy: the param's value is
    /// re-supplied through the named sanitizer before resubmission.
    Sanitize {
        /// The sanitizer `Ref` (Validator/Procedure).
        sanitizer_ref: String,
        /// The canonical parameter to sanitize.
        param: String,
    },
}

/// The `readers_to` of a `declassify` decision.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadersTo {
    /// `recipients(p)` — the resolved recipient set.
    Recipients,
    /// A literal `PrincipalRef` set.
    Named(BTreeSet<String>),
}

/// `FlowRule{rule_id, selector, condition, decision, enforcement?,
/// remedies_hint?}` (ADR-0056 D1 — carried on `FlowContract.rules` at this
/// stage; the `HarnessRule{action: flow_policy}` wrapper is the §03 dialect
/// row).
#[derive(Debug, Clone, PartialEq)]
pub struct FlowRule {
    /// The rule id (the check trail's row spelling).
    pub rule_id: String,
    /// The selector.
    pub selector: FlowSelector,
    /// The condition — `None` = unconditional (the selector alone decides).
    pub condition: Option<FlowCond>,
    /// The decision.
    pub decision: FlowDecision,
    /// The rule's enforcement class (`deterministic` default — a definition-
    /// issued rule; `advisory` = model-drafted, raise-only by I-F1).
    pub enforcement: EnforcementClass,
    /// The remedies the rule's deny/ask carries (`remedies_hint`).
    pub remedies_hint: Vec<Remedy>,
}

// ── Remedy (ADR-0055 D5; the closed sum) ─────────────────────────────────────

/// `Remedy` — the closed recoverable-denial set (§5g.2 §3; R1–R5 in I-F6):
/// `approval(effect_id) | sanitize(sanitizer_ref, param) |
/// shape_endorse(validator_ref, param) | branch(spec) | substitute(param,
/// min_authority) | prerequisite(EffectClass)`.
#[derive(Debug, Clone, PartialEq)]
pub enum Remedy {
    /// `approval` — the human decides one `effect_id` (I-F6 R4: counts against
    /// `approvals.requested`; R5: omitted in `unattended` runs).
    Approval {
        /// The effect the approval endorses (never content, never a class).
        effect_id: String,
    },
    /// `sanitize` — re-supply `param` through the sanitizer.
    Sanitize {
        /// The sanitizer `Ref` (`None` = any registered sanitizer).
        sanitizer_ref: Option<String>,
        /// The canonical parameter.
        param: String,
    },
    /// `shape_endorse` — a capacity-bounded `validator` endorsement of
    /// `param`'s value.
    ShapeEndorse {
        /// The validator `Ref`.
        validator_ref: Option<String>,
        /// The canonical parameter.
        param: String,
    },
    /// `branch` — isolate the flow in a label-seeded branch (the Stage-4
    /// member; enumerated never at this stage).
    Branch {
        /// The branch spec coordinate.
        spec: String,
    },
    /// `substitute` — the caller re-supplies `param` at `min_authority`
    /// (default `principal`).
    Substitute {
        /// The canonical parameter.
        param: String,
        /// The minimum authority the substitute must carry.
        min_authority: AuthorityClass,
    },
    /// `prerequisite` — commit `EffectClass` first.
    Prerequisite {
        /// The prerequisite class's domain spelling.
        domain: String,
    },
}

impl Remedy {
    /// The remedy kind spelling.
    pub fn kind(&self) -> &'static str {
        match self {
            Remedy::Approval { .. } => "approval",
            Remedy::Sanitize { .. } => "sanitize",
            Remedy::ShapeEndorse { .. } => "shape_endorse",
            Remedy::Branch { .. } => "branch",
            Remedy::Substitute { .. } => "substitute",
            Remedy::Prerequisite { .. } => "prerequisite",
        }
    }

    /// The canonical JSON — `{kind, …members}`.
    pub fn to_json(&self) -> Json {
        let mut m = vec![("kind", Json::str(self.kind()))];
        match self {
            Remedy::Approval { effect_id } => {
                m.push(("effect_id", Json::str(effect_id.clone())));
            }
            Remedy::Sanitize {
                sanitizer_ref,
                param,
            } => {
                if let Some(r) = sanitizer_ref {
                    m.push(("sanitizer_ref", Json::str(r.clone())));
                }
                m.push(("param", Json::str(param.clone())));
            }
            Remedy::ShapeEndorse {
                validator_ref,
                param,
            } => {
                if let Some(r) = validator_ref {
                    m.push(("validator_ref", Json::str(r.clone())));
                }
                m.push(("param", Json::str(param.clone())));
            }
            Remedy::Branch { spec } => {
                m.push(("spec", Json::str(spec.clone())));
            }
            Remedy::Substitute {
                param,
                min_authority,
            } => {
                m.push(("param", Json::str(param.clone())));
                m.push(("min_authority", Json::str(min_authority.as_str())));
            }
            Remedy::Prerequisite { domain } => {
                m.push(("domain", Json::str(domain.clone())));
            }
        }
        Json::obj(m)
    }

    /// Parse the canonical form.
    pub fn from_json(j: &Json, path: &str) -> Result<Remedy, DecodeError> {
        let kind = j
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| DecodeError {
                detail: format!("{path}.kind missing"),
            })?;
        let str_member = |k: &str| -> Result<String, DecodeError> {
            j.get(k)
                .and_then(Json::as_str)
                .map(String::from)
                .ok_or_else(|| DecodeError {
                    detail: format!("{path}.{k} missing"),
                })
        };
        Ok(match kind {
            "approval" => Remedy::Approval {
                effect_id: str_member("effect_id")?,
            },
            "sanitize" => Remedy::Sanitize {
                sanitizer_ref: j
                    .get("sanitizer_ref")
                    .and_then(Json::as_str)
                    .map(String::from),
                param: str_member("param")?,
            },
            "shape_endorse" => Remedy::ShapeEndorse {
                validator_ref: j
                    .get("validator_ref")
                    .and_then(Json::as_str)
                    .map(String::from),
                param: str_member("param")?,
            },
            "branch" => Remedy::Branch {
                spec: str_member("spec")?,
            },
            "substitute" => Remedy::Substitute {
                param: str_member("param")?,
                min_authority: j
                    .get("min_authority")
                    .and_then(Json::as_str)
                    .and_then(AuthorityClass::parse)
                    .unwrap_or(AuthorityClass::Principal),
            },
            "prerequisite" => Remedy::Prerequisite {
                domain: str_member("domain")?,
            },
            other => {
                return Err(DecodeError {
                    detail: format!("{path}.kind {other} unknown"),
                })
            }
        })
    }
}

// ── FlowContract (§5g.2 §3; ADR-0054 D2; §05d E1 owns the field) ─────────────

/// `FlowContract` — the `flow_contract` member on `ToolCapability`:
/// `{contribution{taint_tags, readers_from}, reads, recipient_params,
/// content_params, labels_leaves, resolve_recipients?, enforcement, rules}`.
/// Required at `seal` for `world = open` or `domain ∈ {net_egress,
/// message_human, fs_read, memory_write}` capabilities
/// (`ContributionUndeclared` — AC-R-2.8.2-1).
#[derive(Debug, Clone, PartialEq)]
pub struct FlowContract {
    /// `d_τ` — the declared contribution to `L⁺(p)`.
    pub contribution: Contribution,
    /// `reads` — the declared `ResourcePattern`s the tool reads (the result's
    /// `readers` source for unannotated open-world results).
    pub reads: Vec<String>,
    /// `recipient_params` — the canonical parameters `recipients(p)` resolves
    /// from (the endorsement-surface parameters D-ROBUST guards).
    pub recipient_params: Vec<String>,
    /// `content_params` — the parameters check 3's `readers(x)` covers.
    pub content_params: Vec<String>,
    /// `labels_leaves` — admission produces per-leaf `ProvenanceRecord`s.
    pub labels_leaves: bool,
    /// `resolve_recipients` — the declared canonicalizer ref; its *application*
    /// is the recipient resolution (`resolve_recipients` below).
    pub resolve_recipients: Option<String>,
    /// The contract's enforcement class — `deterministic` (definition-issued)
    /// or `advisory` (a model-drafted contract — its rules may only raise).
    pub enforcement: EnforcementClass,
    /// The contract's flow rules — evaluated in tier order (deny →
    /// allow/declassify/sanitize → Π's default row).
    pub rules: Vec<FlowRule>,
}

impl FlowContract {
    /// The canonical JSON — `to_canonical_string` is the stored member's
    /// byte form (the record keeps the `Json`; this module owns the shape).
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("contribution", self.contribution.to_json()),
            (
                "reads",
                Json::Arr(self.reads.iter().map(|r| Json::str(r.clone())).collect()),
            ),
            (
                "recipient_params",
                Json::Arr(
                    self.recipient_params
                        .iter()
                        .map(|p| Json::str(p.clone()))
                        .collect(),
                ),
            ),
            (
                "content_params",
                Json::Arr(
                    self.content_params
                        .iter()
                        .map(|p| Json::str(p.clone()))
                        .collect(),
                ),
            ),
            ("labels_leaves", Json::Bool(self.labels_leaves)),
        ];
        if let Some(r) = &self.resolve_recipients {
            m.push(("resolve_recipients", Json::str(r.clone())));
        }
        if self.enforcement != EnforcementClass::Deterministic {
            m.push(("enforcement", Json::str(self.enforcement.as_str())));
        }
        if !self.rules.is_empty() {
            m.push((
                "rules",
                Json::Arr(self.rules.iter().map(flow_rule_json).collect()),
            ));
        }
        Json::obj(m)
    }

    /// Parse the canonical member — fails closed (`Err`) on any malformed or
    /// unknown member (the grammar is closed; a misspelled member is a schema
    /// violation, never ignored — CC3).
    pub fn from_json(j: &Json) -> Result<FlowContract, DecodeError> {
        let err = |m: String| DecodeError {
            detail: format!("flow_contract.{m}"),
        };
        let str_arr_member = |k: &str| -> Result<Vec<String>, DecodeError> {
            match j.get(k) {
                Some(Json::Arr(items)) => items
                    .iter()
                    .map(|i| {
                        i.as_str().map(str::to_string).ok_or_else(|| DecodeError {
                            detail: format!("flow_contract.{k} members must be strings"),
                        })
                    })
                    .collect::<Result<Vec<_>, _>>(),
                Some(_) => Err(err(format!("{k} must be an array"))),
                None => Ok(Vec::new()),
            }
        };
        // Closed-member check — the contract's members are the declared set.
        if let Json::Obj(m) = j {
            for k in m.keys() {
                match k.as_str() {
                    "contribution" | "reads" | "recipient_params" | "content_params"
                    | "labels_leaves" | "resolve_recipients" | "enforcement" | "rules" => {}
                    other => return Err(err(format!("unknown member {other}"))),
                }
            }
        } else {
            return Err(err("must be an object".to_string()));
        }
        let contribution = match j.get("contribution") {
            Some(c) => Contribution::from_json(c, "flow_contract.contribution")?,
            None => {
                return Err(err("contribution missing".to_string()));
            }
        };
        let labels_leaves = match j.get("labels_leaves") {
            Some(Json::Bool(b)) => *b,
            Some(_) => return Err(err("labels_leaves must be a bool".to_string())),
            None => false,
        };
        let enforcement = match j.get("enforcement").and_then(Json::as_str) {
            Some(s) => {
                EnforcementClass::parse(s).ok_or_else(|| err(format!("enforcement {s} unknown")))?
            }
            None => EnforcementClass::Deterministic,
        };
        let rules = match j.get("rules") {
            Some(Json::Arr(items)) => items
                .iter()
                .enumerate()
                .map(|(i, r)| flow_rule_from_json(r, &format!("flow_contract.rules[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => return Err(err("rules must be an array".to_string())),
            None => Vec::new(),
        };
        Ok(FlowContract {
            contribution,
            reads: str_arr_member("reads")?,
            recipient_params: str_arr_member("recipient_params")?,
            content_params: str_arr_member("content_params")?,
            labels_leaves,
            resolve_recipients: j
                .get("resolve_recipients")
                .and_then(Json::as_str)
                .map(String::from),
            enforcement,
            rules,
        })
    }
}

fn flow_rule_json(r: &FlowRule) -> Json {
    let mut sel = Vec::new();
    if let Some(d) = &r.selector.domain {
        sel.push(("domain", Json::str(d.clone())));
    }
    if let Some(c) = &r.selector.capability {
        sel.push(("capability", Json::str(c.clone())));
    }
    if !r.selector.params.is_empty() {
        sel.push((
            "params",
            Json::Arr(
                r.selector
                    .params
                    .iter()
                    .map(|p| Json::str(p.clone()))
                    .collect(),
            ),
        ));
    }
    let decision = match &r.decision {
        FlowDecision::Deny { reason } => Json::obj([
            ("kind", Json::str("deny")),
            ("reason", Json::str(reason.clone())),
        ]),
        FlowDecision::Allow => Json::obj([("kind", Json::str("allow"))]),
        FlowDecision::Declassify { readers_to } => Json::obj([
            ("kind", Json::str("declassify")),
            (
                "readers_to",
                match readers_to {
                    ReadersTo::Recipients => Json::str("recipients"),
                    ReadersTo::Named(rs) => {
                        Json::Arr(rs.iter().map(|r| Json::str(r.clone())).collect())
                    }
                },
            ),
        ]),
        FlowDecision::Sanitize {
            sanitizer_ref,
            param,
        } => Json::obj([
            ("kind", Json::str("sanitize")),
            ("sanitizer_ref", Json::str(sanitizer_ref.clone())),
            ("param", Json::str(param.clone())),
        ]),
    };
    let mut m = vec![
        ("id", Json::str(r.rule_id.clone())),
        ("selector", Json::obj(sel)),
        ("decision", decision),
    ];
    if let Some(c) = &r.condition {
        m.push(("condition", c.to_json()));
    }
    if r.enforcement != EnforcementClass::Deterministic {
        m.push(("enforcement", Json::str(r.enforcement.as_str())));
    }
    if !r.remedies_hint.is_empty() {
        m.push((
            "remedies_hint",
            Json::Arr(r.remedies_hint.iter().map(|r| r.to_json()).collect()),
        ));
    }
    Json::obj(m)
}

fn flow_rule_from_json(j: &Json, path: &str) -> Result<FlowRule, DecodeError> {
    let err = |m: String| DecodeError {
        detail: format!("{path}.{m}"),
    };
    let rule_id = j
        .get("id")
        .and_then(Json::as_str)
        .ok_or_else(|| err("id missing".to_string()))?
        .to_string();
    let mut selector = FlowSelector::default();
    if let Some(s) = j.get("selector") {
        selector.domain = s.get("domain").and_then(Json::as_str).map(String::from);
        selector.capability = s.get("capability").and_then(Json::as_str).map(String::from);
        if let Some(Json::Arr(items)) = s.get("params") {
            selector.params = items
                .iter()
                .map(|i| {
                    i.as_str().map(str::to_string).ok_or_else(|| DecodeError {
                        detail: format!("{path}.selector.params members must be strings"),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
        }
    }
    let decision_j = j
        .get("decision")
        .ok_or_else(|| err("decision missing".to_string()))?;
    let decision = match decision_j
        .get("kind")
        .and_then(Json::as_str)
        .ok_or_else(|| err("decision.kind missing".to_string()))?
    {
        "deny" => FlowDecision::Deny {
            reason: decision_j
                .get("reason")
                .and_then(Json::as_str)
                .unwrap_or("PolicyDenied")
                .to_string(),
        },
        "allow" => FlowDecision::Allow,
        "declassify" => FlowDecision::Declassify {
            readers_to: match decision_j.get("readers_to") {
                Some(Json::Str(s)) if s == "recipients" => ReadersTo::Recipients,
                Some(Json::Arr(items)) => ReadersTo::Named(
                    items
                        .iter()
                        .map(|i| {
                            i.as_str().map(str::to_string).ok_or_else(|| DecodeError {
                                detail: format!(
                                    "{path}.decision.readers_to members must be strings"
                                ),
                            })
                        })
                        .collect::<Result<BTreeSet<_>, _>>()?,
                ),
                _ => {
                    return Err(err(
                        "decision.readers_to must be \"recipients\" or an array".to_string(),
                    ))
                }
            },
        },
        "sanitize" => FlowDecision::Sanitize {
            sanitizer_ref: decision_j
                .get("sanitizer_ref")
                .and_then(Json::as_str)
                .ok_or_else(|| err("decision.sanitizer_ref missing".to_string()))?
                .to_string(),
            param: decision_j
                .get("param")
                .and_then(Json::as_str)
                .ok_or_else(|| err("decision.param missing".to_string()))?
                .to_string(),
        },
        other => return Err(err(format!("decision.kind {other} unknown"))),
    };
    let condition = match j.get("condition") {
        Some(c) => Some(FlowCond::from_json(c).map_err(|e| DecodeError {
            detail: format!("{path}.condition: {}", e.detail),
        })?),
        None => None,
    };
    let enforcement = match j.get("enforcement").and_then(Json::as_str) {
        Some(s) => {
            EnforcementClass::parse(s).ok_or_else(|| err(format!("enforcement {s} unknown")))?
        }
        None => EnforcementClass::Deterministic,
    };
    let remedies_hint = match j.get("remedies_hint") {
        Some(Json::Arr(items)) => items
            .iter()
            .enumerate()
            .map(|(i, r)| Remedy::from_json(r, &format!("{path}.remedies_hint[{i}]")))
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(err("remedies_hint must be an array".to_string())),
        None => Vec::new(),
    };
    Ok(FlowRule {
        rule_id,
        selector,
        condition,
        decision,
        enforcement,
        remedies_hint,
    })
}

// ── Admission (ADR-0054 D1) ──────────────────────────────────────────────────

/// The `admission` member's spelling on `context.observation.recorded`
/// (`as_declared | as_realized | narrowed | unverified` — the spec's
/// `{as_declared, narrowed}` plus the two honest outcomes: a realized-more-
/// restrictive admission and an unannotated open-world result).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionKind {
    /// Admitted at the declared/computed label.
    AsDeclared,
    /// The realized label was more restrictive — admitted as realized.
    AsRealized,
    /// The realized label was less restrictive — admitted at the declared
    /// label (`AdmissionNarrowed` recorded).
    Narrowed,
    /// An unannotated open-world result — `unverified`, `taint = {tool}`,
    /// readers from the declared `reads`.
    Unverified,
}

impl AdmissionKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AdmissionKind::AsDeclared => "as_declared",
            AdmissionKind::AsRealized => "as_realized",
            AdmissionKind::Narrowed => "narrowed",
            AdmissionKind::Unverified => "unverified",
        }
    }
}

/// `admit`'s result — the admitted label plus the recorded kind.
#[derive(Debug, Clone, PartialEq)]
pub struct Admission {
    /// `L(r)` — the label the result's `ProvenanceRecord` carries.
    pub label: Label,
    /// The admission kind (the `admission` audit member).
    pub kind: AdmissionKind,
}

/// `admit(result, contract, L⁺)` → the admitted label (§5g.2 §2; ADR-0054
/// D1). `L(r) = L⁺(p) ⊔ default_authority(tool, world)` — the tool's default
/// label is `external` for open-world, `environment` for closed. A realized
/// label more restrictive than the declared bound is admitted as realized; a
/// less-restrictive (or incomparable) realized label is admitted at the
/// declared bound and recorded `narrowed` — admission never reads a class
/// from the payload. An unannotated open-world result is `unverified`,
/// `taint = {tool}`, readers from the declared `reads`.
pub fn admit(
    realized: Option<&Label>,
    contract: &FlowContract,
    l_plus: &Label,
    capability: &str,
    world_open: bool,
) -> Admission {
    let tool_default = Label::at(if world_open {
        AuthorityClass::External
    } else {
        AuthorityClass::Environment
    });
    let declared = l_plus.join(&tool_default);
    match realized {
        Some(r) => {
            if declared.leq(r) {
                // Declared flows to realized — realized is at least as
                // restrictive; admit as realized.
                Admission {
                    label: r.clone(),
                    kind: AdmissionKind::AsRealized,
                }
            } else {
                Admission {
                    label: declared,
                    kind: AdmissionKind::Narrowed,
                }
            }
        }
        None => {
            if world_open {
                let mut taint = BTreeSet::new();
                taint.insert(TaintTag::Tool {
                    capability: capability.to_string(),
                    inner_source: None,
                });
                Admission {
                    label: Label {
                        authority: AuthorityClass::Unverified,
                        taint,
                        readers: match &contract.contribution.readers_from {
                            ReadersFrom::Declared(s) => ReaderSet::Restricted(s.clone()),
                            ReadersFrom::Public => ReaderSet::Public,
                            ReadersFrom::Reads => declared.readers.clone(),
                        },
                    },
                    kind: AdmissionKind::Unverified,
                }
            } else {
                Admission {
                    label: declared,
                    kind: AdmissionKind::AsDeclared,
                }
            }
        }
    }
}

// ── D-ROBUST (I-F2; ADR-0055 D2) ─────────────────────────────────────────────

/// The D-ROBUST inputs — one `(param, label, shape_endorsed)` triple per
/// endorsement-surface parameter (recipient parameters of `policy_rule (a)`,
/// sanitizer selector arguments, an approval's `subject_ref`).
#[derive(Debug, Clone)]
pub struct RobustnessInput<'a> {
    /// The parameter's name (the check trail's detail).
    pub param: String,
    /// Its label.
    pub label: &'a Label,
    /// Whether the value carries a validator shape endorsement.
    pub shape_endorsed: bool,
}

/// `d_robust` — an endorsement/declassification decision is refused
/// (`RobustnessViolated`) when any input has `authority ≤ external` or
/// `taint ≠ ∅` and was not itself shape-endorsed (I-F2). Runs before any
/// `ask`. Returns the offending parameter names.
pub fn d_robust(inputs: &[RobustnessInput]) -> Result<(), Vec<String>> {
    let bad: Vec<String> = inputs
        .iter()
        .filter(|i| {
            !i.shape_endorsed
                && (i.label.authority <= AuthorityClass::External || !i.label.taint.is_empty())
        })
        .map(|i| i.param.clone())
        .collect();
    if bad.is_empty() {
        Ok(())
    } else {
        Err(bad)
    }
}

// ── Recipient resolution + check 3 (I-F4; ADR-0055 D4) ───────────────────────

/// `recipients(p)` — resolve `recipient_params` through the canonical args.
/// A param's value contributes its string leaves (a string, or the string
/// members of an array/record). `resolve_recipients`'s declared ref is the
/// canonicalizer; at this stage the canonical spelling is the string value
/// itself (the resolver's application is recorded, not re-run — I-H1).
/// Returns `None` when a declared `recipient_param` is absent or yields no
/// recipient — the caller treats that as an evaluation failure on a
/// check-3-relevant effect.
pub fn resolve_recipients(
    contract: &FlowContract,
    args: &BTreeMap<String, Json>,
) -> Option<BTreeSet<String>> {
    if contract.recipient_params.is_empty() {
        return Some(BTreeSet::new());
    }
    let mut out = BTreeSet::new();
    for p in &contract.recipient_params {
        let v = args.get(p)?;
        collect_strings(v, &mut out);
    }
    if contract
        .recipient_params
        .iter()
        .all(|p| args.get(p).is_none())
    {
        return None;
    }
    Some(out)
}

fn collect_strings(j: &Json, out: &mut BTreeSet<String>) {
    match j {
        Json::Str(s) => {
            out.insert(s.clone());
        }
        Json::Arr(items) => {
            for i in items {
                collect_strings(i, out);
            }
        }
        Json::Obj(m) => {
            for v in m.values() {
                collect_strings(v, out);
            }
        }
        _ => {}
    }
}

/// Whether check 3 (I-F4) applies to this effect: `world = open` or domain ∈
/// `{net_egress, message_human, spend}` or `memory_write` to scope ≥
/// `project` (`fs_write ∧ open` is subsumed by `world = open`).
pub fn check3_relevant(
    domain: &str,
    world_open: bool,
    memory_scope_at_least_project: bool,
) -> bool {
    world_open
        || matches!(domain, "net_egress" | "message_human" | "spend")
        || (domain == "memory_write" && memory_scope_at_least_project)
}

/// Check 3 (I-F4): `recipients(p) ⊆ readers(x)` for every `content_param`
/// `x`. `Public` readers always pass. Returns the failing content params.
/// `readers_of` resolves a param's effective readers (post-declassification).
pub fn check_reader_coverage<'a>(
    contract: &FlowContract,
    recipients: &BTreeSet<String>,
    readers_of: impl Fn(&str) -> Option<&'a ReaderSet>,
) -> Vec<String> {
    if recipients.is_empty() {
        return Vec::new();
    }
    contract
        .content_params
        .iter()
        .filter(|x| match readers_of(x) {
            Some(ReaderSet::Public) => false,
            Some(ReaderSet::Restricted(rs)) => !recipients.iter().all(|r| rs.contains(r)),
            None => true,
        })
        .cloned()
        .collect()
}

// ── check_flow — the tiered flow gate (ADR-0056 D4; ADR-0055 D4) ─────────────

/// What the flow stage concluded.
#[derive(Debug, Clone, PartialEq)]
pub enum FlowVerdict {
    /// A deny-tier rule matched (or evaluation failed) — deny.
    Deny {
        /// The rule id or `evaluation_error`.
        detail: String,
        /// The recorded reason spelling.
        reason: String,
        /// The rule's remedies hint.
        remedies: Vec<Remedy>,
    },
    /// A declassify rule matched — readers widened for `params` (the caller
    /// applies the widened readers to check 3 and emits `label.declassified`).
    Declassified {
        /// The rule id.
        rule_id: String,
        /// The widened reader set per content param.
        readers_to: ReaderSet,
    },
    /// The flow policy asks (a sanitize rule matched, or reader coverage
    /// failed with remedies) — remedies attached.
    Ask {
        /// Why the flow stage asks.
        detail: String,
        /// The enumerated remedies.
        remedies: Vec<Remedy>,
    },
    /// An allow-tier rule matched — the flow policy authorizes (Π's default
    /// row is not consulted for this flow).
    Allow {
        /// The matched rule id.
        rule_id: String,
    },
    /// No flow rule decided — Π's default row decides.
    Fallthrough,
}

/// `check_flow(p, L⁺, params, recipients, policies, effects, Π)` — the
/// rule-evaluation half (§5g.2 §2): deny rules first (first match within a
/// tier), then `allow`/`declassify`/`sanitize` rules, then `Fallthrough`
/// (the caller runs Π's default row). Advisory rules may only *raise* —
/// a non-deny decision under `advisory` enforcement is skipped (I-F1
/// no-sensitive-upgrade). Any atom evaluation failure ⇒ `Deny{EvaluationError}`.
pub fn check_flow(rules: &[FlowRule], input: &FlowInput) -> Result<FlowVerdict, EvalError> {
    // Tier 1 — every deny rule, first match.
    for rule in rules {
        if !matches!(rule.decision, FlowDecision::Deny { .. }) {
            continue;
        }
        if !selector_matches(&rule.selector, input) {
            continue;
        }
        let hit = match &rule.condition {
            Some(c) => eval_cond(c, input)?,
            None => true,
        };
        if hit {
            let reason = match &rule.decision {
                FlowDecision::Deny { reason } => reason.clone(),
                _ => unreachable!(),
            };
            return Ok(FlowVerdict::Deny {
                detail: format!("flow_deny:{}", rule.rule_id),
                reason,
                remedies: rule.remedies_hint.clone(),
            });
        }
    }
    // Tier 2 — allow / declassify / sanitize, first match. Advisory rules
    // cannot widen (I-F1): a non-deny advisory decision never applies.
    for rule in rules {
        if matches!(rule.decision, FlowDecision::Deny { .. }) {
            continue;
        }
        if !selector_matches(&rule.selector, input) {
            continue;
        }
        let hit = match &rule.condition {
            Some(c) => eval_cond(c, input)?,
            None => true,
        };
        if !hit {
            continue;
        }
        if !rule.enforcement.may_widen() {
            continue; // advisory: raise-only — never an allow/declassify/sanitize
        }
        return Ok(match &rule.decision {
            FlowDecision::Allow => FlowVerdict::Allow {
                rule_id: rule.rule_id.clone(),
            },
            FlowDecision::Declassify { readers_to } => FlowVerdict::Declassified {
                rule_id: rule.rule_id.clone(),
                readers_to: match readers_to {
                    ReadersTo::Recipients => ReaderSet::Restricted(input.recipients.clone()),
                    ReadersTo::Named(rs) => ReaderSet::Restricted(rs.clone()),
                },
            },
            FlowDecision::Sanitize {
                sanitizer_ref,
                param,
            } => FlowVerdict::Ask {
                detail: format!("flow_sanitize:{}", rule.rule_id),
                remedies: vec![Remedy::Sanitize {
                    sanitizer_ref: Some(sanitizer_ref.clone()),
                    param: param.clone(),
                }],
            },
            FlowDecision::Deny { .. } => unreachable!(),
        });
    }
    Ok(FlowVerdict::Fallthrough)
}

fn selector_matches(sel: &FlowSelector, input: &FlowInput) -> bool {
    sel.domain
        .as_deref()
        .map(|d| d == input.domain)
        .unwrap_or(true)
        && sel
            .capability
            .as_deref()
            .map(|c| c == input.capability)
            .unwrap_or(true)
        && sel.params.iter().all(|p| input.args.contains_key(p))
}

/// `enumerate_remedies` (ADR-0055 D5; R1–R5) — the bounded, deterministic
/// enumeration over the finite `(Label, effect projection)` state. For a
/// reader-coverage failure: `approval` (absent in `unattended` — R5) +
/// `sanitize` for each uncovered content param + `shape_endorse` for each
/// content param the contract can bound. For a robustness failure:
/// `substitute` on the offending param at `min_authority = principal`. R1:
/// every enumerated remedy is a narrowing or an existing basis; R2: bounded
/// (`≤ 1 + 2·|content_params| + |recipient_params|`) and deterministic order;
/// R3: a consumed remedy is recorded by the caller as its own event.
pub fn enumerate_remedies(
    effect_id: &str,
    coverage_failures: &[String],
    robustness_failures: &[String],
    contract: &FlowContract,
    unattended: bool,
) -> Vec<Remedy> {
    let mut out = Vec::new();
    if !coverage_failures.is_empty() {
        if !unattended {
            out.push(Remedy::Approval {
                effect_id: effect_id.to_string(),
            });
        }
        // Sanitize remedies — one per uncovered content param; the sanitizer
        // ref comes from a declared sanitize rule for that param when present.
        for x in coverage_failures {
            let sanitizer_ref = contract.rules.iter().find_map(|r| match &r.decision {
                FlowDecision::Sanitize {
                    sanitizer_ref,
                    param,
                } if param == x => Some(sanitizer_ref.clone()),
                _ => None,
            });
            out.push(Remedy::Sanitize {
                sanitizer_ref,
                param: x.clone(),
            });
        }
    }
    for p in robustness_failures {
        out.push(Remedy::Substitute {
            param: p.clone(),
            min_authority: AuthorityClass::Principal,
        });
    }
    out
}

// ── The per-basis effects table (§5g.2 §2.2; ADR-0055 D1) ────────────────────

/// The default `cap_max` — the per-definition capacity ceiling for shape
/// endorsement (§5g.2 §3 `cap_max`; the declared default is 8 bits).
pub const CAP_MAX_DEFAULT: u64 = 8;

/// The per-basis effect check — what a basis may do to `taint`/`readers` on
/// top of ADR-0035's authority rules (`endorse` still runs its full gate; this
/// is the C2 componentwise table):
///
/// - `seal`/`pin`/`promotion`: raise authority (per ADR-0035), **clear taint**,
///   readers as declared;
/// - `approval`: **never changes a component** (it endorses one
///   `(effect_id, args_canonical_hash)` — an `approval`-basis `endorse` is
///   always `BasisEffectError::NoLabelChange`);
/// - `validator`: `external → environment` and `taint → ∅` iff `capacity_bits
///   ≤ cap_max` (shape endorsement);
/// - `policy_rule`: readers-declassification or bounded sanitizer — never
///   authority, never taint beyond the declared `removes`.
pub fn check_basis_effect(
    basis: EndorsementBasis,
    from: &Label,
    to: &Label,
) -> Result<(), BasisEffectError> {
    match basis {
        EndorsementBasis::Seal | EndorsementBasis::Pin | EndorsementBasis::Promotion => {
            if to.authority <= from.authority {
                return Err(BasisEffectError::NoAuthorityRaise);
            }
            if !to.taint.is_empty() {
                return Err(BasisEffectError::TaintNotCleared);
            }
            Ok(())
        }
        EndorsementBasis::Approval => Err(BasisEffectError::NoLabelChange),
        EndorsementBasis::Validator => {
            if from.authority != AuthorityClass::External
                || to.authority != AuthorityClass::Environment
            {
                return Err(BasisEffectError::ValidatorRange);
            }
            if !to.taint.is_empty() {
                return Err(BasisEffectError::TaintNotCleared);
            }
            Ok(())
        }
        EndorsementBasis::PolicyRule => {
            // Readers declassification only: identical authority and taint,
            // strictly wider readers.
            if to.authority != from.authority || to.taint != from.taint {
                return Err(BasisEffectError::PolicyRuleTouchesNonReaders);
            }
            if !(to.readers.is_superset_of(&from.readers) && to.readers != from.readers) {
                return Err(BasisEffectError::NoLabelIncrease);
            }
            Ok(())
        }
    }
}

/// The per-basis effect failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BasisEffectError {
    /// `seal`/`pin`/`promotion` did not raise authority.
    NoAuthorityRaise,
    /// The basis must clear taint (`seal`/`pin`/`promotion`/`validator`).
    TaintNotCleared,
    /// `approval` never changes a component — a label endorsement under it is
    /// never emitted.
    NoLabelChange,
    /// `validator` raises only `external → environment`.
    ValidatorRange,
    /// `policy_rule` may widen readers only — authority/taint untouched.
    PolicyRuleTouchesNonReaders,
    /// The `to` does not actually rise (`endorse`'s laundering guard).
    NoLabelIncrease,
}

/// `capacity_bits(schema)` — `⌈log₂|values|⌉` for a **capacity-bounded**
/// schema (§5g.2 §2.2): booleans (1), closed enums (`⌈log₂ n⌉`), bounded
/// integers (`⌈log₂ (max-min+1)⌉`), `const` (0). Unbounded strings, free
/// text, URLs, paths, open-alphabet identifiers, unbounded arrays/objects
/// return `None` — never qualifying (`BasisNotAllowed`). Fixed-shape records
/// and bounded arrays multiply/sum member capacities.
pub fn capacity_bits(schema: &Json) -> Option<u64> {
    if schema.get("const").is_some() {
        return Some(0);
    }
    if let Some(Json::Arr(items)) = schema.get("enum") {
        let n = items.len() as u64;
        return Some(ceil_log2(n.max(1)));
    }
    match schema.get("type").and_then(Json::as_str) {
        Some("boolean") => Some(1),
        Some("integer") | Some("number") => {
            let min = schema.get("minimum").and_then(Json::as_int)?;
            let max = schema.get("maximum").and_then(Json::as_int)?;
            if max < min {
                return None;
            }
            Some(ceil_log2((max - min + 1).max(1) as u64))
        }
        Some("object") => {
            // Fixed-shape record: every declared property capacity-bounded,
            // no additional properties — capacity is the sum of member bits.
            let props = schema.get("properties")?;
            if !matches!(schema.get("additionalProperties"), Some(Json::Bool(false)))
                && schema.get("additionalProperties").is_some()
            {
                return None;
            }
            schema.get("additionalProperties")?;
            let Json::Obj(m) = props else {
                return None;
            };
            let mut total: u64 = 0;
            for v in m.values() {
                total = total.checked_add(capacity_bits(v)?)?;
            }
            Some(total)
        }
        Some("array") => {
            // Bounded array of capacity-bounded members:
            // bits = max_items · member_bits + length bits.
            let max_items = schema.get("maxItems").and_then(Json::as_int)?;
            if max_items < 0 {
                return None;
            }
            let member = capacity_bits(schema.get("items")?)?;
            let len_bits = ceil_log2((max_items + 1).max(1) as u64);
            (member.checked_mul(max_items as u64)?).checked_add(len_bits)
        }
        _ => None, // strings, free text, URLs, paths — never qualify
    }
}

fn ceil_log2(n: u64) -> u64 {
    if n <= 1 {
        return 0;
    }
    let mut bits = 0;
    let mut v = n - 1;
    while v > 0 {
        v >>= 1;
        bits += 1;
    }
    bits
}

/// `SanitizerBounds{from, to, removes}` — the declared bound a bounded
/// sanitizer runs under (§5g.2 §3 `Sanitizer`; ADR-0055 D1). `from` is a
/// label pattern the source must satisfy; `to` is the label the output is
/// endorsed at; `removes` the taint patterns the sanitizer claims to strip.
#[derive(Debug, Clone, PartialEq)]
pub struct SanitizerBounds {
    /// The source-side constraint: the realized output's authority must not
    /// exceed this ceiling (an output claiming more is exceeded).
    pub to: Label,
    /// The taint patterns the sanitizer claims to remove.
    pub removes: Vec<TaintTagPattern>,
}

/// The bounded-sanitizer bound check (§5g.2 §2.2 `policy_rule (b)`):
/// `SanitizerBoundExceeded` when the realized output's measured label shows a
/// tag the sanitizer claimed to remove, or `to` claims an authority the
/// realized label does not already carry (policy_rule never touches
/// authority), or `to.taint` adds a tag the realized output lacks. Returns
/// the endorsed label for the new projection item (`bounds.to`).
pub fn apply_sanitizer_bounds(
    bounds: &SanitizerBounds,
    realized: &Label,
    capability: &str,
) -> Result<Label, FlowError> {
    // A realized tag matching `removes` means the measured output still
    // carries what the sanitizer claimed to strip — the bound is exceeded.
    let surviving = realized
        .taint
        .iter()
        .filter(|t| bounds.removes.iter().any(|p| p.matches(t, capability)))
        .map(|t| t.as_string())
        .collect::<Vec<_>>();
    if !surviving.is_empty() {
        return Err(FlowError::SanitizerBoundExceeded {
            detail: format!("removed taint survived: {}", surviving.join(",")),
        });
    }
    if bounds.to.authority != realized.authority {
        // policy_rule (b) never touches authority — a `to` claiming a
        // different class is out of bounds.
        return Err(FlowError::SanitizerBoundExceeded {
            detail: "bounds.to.authority ≠ realized.authority".to_string(),
        });
    }
    if !bounds.to.taint.is_subset(&realized.taint) {
        return Err(FlowError::SanitizerBoundExceeded {
            detail: "bounds.to.taint adds tags the realized output lacks".to_string(),
        });
    }
    Ok(bounds.to.clone())
}

/// The flow layer's typed failures — every refusal is a member, never a
/// warning (CC3; the monitor maps them onto `DenyReason`s).
#[derive(Debug, Clone, PartialEq)]
pub enum FlowError {
    /// An atom could not produce a verdict — `deny(EvaluationError)`.
    EvaluationError {
        /// The failing atom.
        detail: String,
    },
    /// D-ROBUST refused — an endorsement-surface input is `≤ external` or
    /// tainted and not shape-endorsed.
    RobustnessViolated {
        /// The offending parameters.
        params: Vec<String>,
    },
    /// Check 3 failed — `recipients(p) ⊄ readers(x)` for the named params.
    ReaderCoverage {
        /// The uncovered content parameters.
        params: Vec<String>,
    },
    /// A sanitizer's realized output violated its declared bounds.
    SanitizerBoundExceeded {
        /// What exceeded.
        detail: String,
    },
}

// ── Size bounds (AC-R-2.8.2-14) ──────────────────────────────────────────────

/// The declared per-event label-bytes bound — a `Label`'s canonical encoding
/// with non-default components (authority spelling + taint tags + readers)
/// never exceeds this on the wire (AC-R-2.8.2-14's measured quantity).
pub const LABEL_MAX_BYTES: usize = 4 * 1024;

/// The canonical byte size of `l`'s encoding — the AC-14 measurement hook.
pub fn label_bytes(l: &Label) -> usize {
    label_json_full(l).to_canonical_string().len()
}

/// The canonical `Label` spelling `{authority, taint[], readers[]}` — shared
/// with `hh_monitor::wire`'s proposal codec (one spelling; emitters keep the
/// non-default-members-only convention by omitting empty `taint`/`readers`
/// where the receiving schema allows).
pub fn label_json_full(l: &Label) -> Json {
    Json::obj([
        ("authority", Json::str(l.authority.as_str())),
        (
            "taint",
            Json::Arr(l.taint.iter().map(|t| Json::str(t.as_string())).collect()),
        ),
        (
            "readers",
            match &l.readers {
                ReaderSet::Public => Json::Arr(vec![]),
                ReaderSet::Restricted(rs) => {
                    Json::Arr(rs.iter().map(|r| Json::str(r.clone())).collect())
                }
            },
        ),
    ])
}

/// Decode the `label_json_full` spelling.
pub fn label_from_json(j: &Json) -> Result<Label, DecodeError> {
    Label::from_json(j)
}

// ── Endorsement helpers (the C2 endorse inputs) ──────────────────────────────

/// `shape_endorse` — the `validator` basis's C2 form (§5g.2 §2.2): an
/// `external` closed-schema value rises to `environment`, `taint → ∅`, iff
/// `capacity_bits(schema) ≤ cap_max`. The schema is the capability's declared
/// output schema — the kernel computes the capacity, never the validator
/// (OQ-145's ratified default: kernel-computed). Delegates the authority-side
/// rules to [`crate::endorse::endorse`]; this adds the capacity gate and the
/// taint-clear requirement.
pub fn check_shape_endorsement(schema: &Json, cap_max: u64) -> Result<u64, FlowError> {
    match capacity_bits(schema) {
        Some(bits) if bits <= cap_max => Ok(bits),
        Some(bits) => Err(FlowError::EvaluationError {
            detail: format!("capacity_bits {bits} exceeds cap_max {cap_max}"),
        }),
        None => Err(FlowError::EvaluationError {
            detail: "schema is not capacity-bounded (BasisNotAllowed)".to_string(),
        }),
    }
}

/// Whether the per-basis table admits `(basis, from, to)` — wraps
/// [`check_basis_effect`] with the `EndorsementError` mapping the ledger's
/// append-time check consumes.
pub fn basis_effect_endorsement_error(
    basis: EndorsementBasis,
    e: &BasisEffectError,
) -> EndorsementError {
    match e {
        BasisEffectError::NoLabelChange | BasisEffectError::NoLabelIncrease => {
            EndorsementError::NoLabelIncrease
        }
        BasisEffectError::NoAuthorityRaise
        | BasisEffectError::TaintNotCleared
        | BasisEffectError::ValidatorRange
        | BasisEffectError::PolicyRuleTouchesNonReaders => EndorsementError::BasisNotAllowed {
            basis,
            kind: "per-basis effect violated (§5g.2 §2.2)",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tainted(a: AuthorityClass, cap: &str) -> Label {
        let mut l = Label::at(a);
        l.taint.insert(TaintTag::Tool {
            capability: cap.into(),
            inner_source: None,
        });
        l
    }

    fn with_readers(mut l: Label, rs: &[&str]) -> Label {
        l.readers = ReaderSet::Restricted(rs.iter().map(|s| s.to_string()).collect());
        l
    }

    fn contract() -> FlowContract {
        FlowContract {
            contribution: Contribution {
                taint_tags: vec![TaintTagPattern::SelfTool],
                readers_from: ReadersFrom::Reads,
            },
            reads: vec!["fs:workspace/**".into()],
            recipient_params: vec!["to".into()],
            content_params: vec!["body".into()],
            labels_leaves: false,
            resolve_recipients: None,
            enforcement: EnforcementClass::Deterministic,
            rules: vec![],
        }
    }

    fn input<'a>(
        l_plus: &'a Label,
        params: &'a BTreeMap<String, Label>,
        args: &'a BTreeMap<String, Json>,
        recipients: &'a BTreeSet<String>,
        committed: &'a [CommittedEffect],
        detectors: &'a BTreeMap<String, bool>,
    ) -> FlowInput<'a> {
        FlowInput {
            l_plus,
            param_labels: params,
            args,
            recipients,
            domain: "net_egress",
            world: "open",
            committed,
            detectors,
            capability: "cap-1",
        }
    }

    // ── FlowContract codec ───────────────────────────────────────────────

    #[test]
    fn contract_round_trips_byte_identically() {
        let mut c = contract();
        c.labels_leaves = true;
        c.resolve_recipients = Some("val:resolve".into());
        c.enforcement = EnforcementClass::Advisory;
        c.rules.push(FlowRule {
            rule_id: "r1".into(),
            selector: FlowSelector {
                domain: Some("net_egress".into()),
                capability: Some("cap-1".into()),
                params: vec!["to".into()],
            },
            condition: Some(FlowCond::AuthorityAtLeast {
                subject: Subject::Proposal,
                class: AuthorityClass::Principal,
            }),
            decision: FlowDecision::Deny {
                reason: "ReaderCoverage".into(),
            },
            enforcement: EnforcementClass::Deterministic,
            remedies_hint: vec![Remedy::Approval {
                effect_id: "e1".into(),
            }],
        });
        let j = c.to_json();
        let back = FlowContract::from_json(&j).unwrap();
        assert_eq!(back, c);
        assert_eq!(
            j.to_canonical_string(),
            back.to_json().to_canonical_string()
        );
    }

    #[test]
    fn contract_decode_fails_closed() {
        // Unknown member — the grammar is closed.
        let j = Json::obj([
            ("contribution", contract().contribution.to_json()),
            ("bogus", Json::Bool(true)),
        ]);
        assert!(FlowContract::from_json(&j).is_err());
        // Missing `contribution`.
        assert!(FlowContract::from_json(&Json::obj([])).is_err());
        // Unknown enforcement spelling.
        let mut c = contract().to_json();
        if let Json::Obj(ref mut m) = c {
            m.insert("enforcement".to_string(), Json::str("magic"));
        }
        assert!(FlowContract::from_json(&c).is_err());
        // `rules` must be an array.
        let mut c = contract().to_json();
        if let Json::Obj(ref mut m) = c {
            m.insert("rules".to_string(), Json::str("nope"));
        }
        assert!(FlowContract::from_json(&c).is_err());
    }

    // ── prospective label ────────────────────────────────────────────────

    #[test]
    fn prospective_label_joins_ctx_args_and_contribution() {
        let ctx = Label::at(AuthorityClass::Principal);
        let arg = tainted(AuthorityClass::External, "imported-1");
        let c = contract();
        let l_plus = prospective_label(&ctx, vec![arg], &c.contribution, "cap-1");
        // join: min authority, taint ∪, readers ∩ (contribution adds `tool:cap-1`).
        assert_eq!(l_plus.authority, AuthorityClass::External);
        assert!(l_plus.taint.contains(&TaintTag::Tool {
            capability: "imported-1".into(),
            inner_source: None
        }));
        assert!(l_plus.taint.contains(&TaintTag::Tool {
            capability: "cap-1".into(),
            inner_source: None
        }));
    }

    #[test]
    fn prospective_label_is_never_less_restrictive_than_ctx() {
        // Conservativity (AC-2): joining args and a contribution only moves
        // toward greater restriction — `ctx ⊑ L⁺` for any args/contribution.
        let ctx = with_readers(Label::at(AuthorityClass::Principal), &["alice"]);
        let arg = Label::at(AuthorityClass::Kernel); // even a kernel arg
        let l_plus = prospective_label(&ctx, vec![arg], &contract().contribution, "cap-1");
        assert!(ctx.leq(&l_plus));
    }

    // ── admission ────────────────────────────────────────────────────────

    #[test]
    fn admit_as_realized_when_realized_is_more_restrictive() {
        let c = contract();
        let l_plus = Label::at(AuthorityClass::Environment);
        let realized = with_readers(tainted(AuthorityClass::External, "x"), &["alice"]);
        let a = admit(Some(&realized), &c, &l_plus, "cap-1", true);
        assert_eq!(a.kind, AdmissionKind::AsRealized);
        assert_eq!(a.label, realized);
    }

    #[test]
    fn admit_narrowed_when_realized_is_less_restrictive() {
        // Conservativity (AC-2): a realized label claiming *more* authority
        // than the declared bound is never honored — admission narrows to the
        // declared label and records `narrowed`.
        let c = contract();
        let l_plus = tainted(AuthorityClass::External, "cap-1");
        let realized = Label::at(AuthorityClass::Principal); // claims widening
        let a = admit(Some(&realized), &c, &l_plus, "cap-1", true);
        assert_eq!(a.kind, AdmissionKind::Narrowed);
        assert!(a.label.authority < AuthorityClass::Principal);
    }

    #[test]
    fn admit_unverified_for_unannotated_open_world() {
        let c = contract();
        let l_plus = Label::at(AuthorityClass::Principal);
        let a = admit(None, &c, &l_plus, "cap-1", true);
        assert_eq!(a.kind, AdmissionKind::Unverified);
        assert_eq!(a.label.authority, AuthorityClass::Unverified);
        assert_eq!(a.label.taint.len(), 1);
    }

    #[test]
    fn admit_as_declared_for_closed_world() {
        let c = contract();
        let l_plus = Label::at(AuthorityClass::Principal);
        let a = admit(None, &c, &l_plus, "cap-1", false);
        assert_eq!(a.kind, AdmissionKind::AsDeclared);
        // `environment` is the closed-world tool default — the join's min.
        assert_eq!(a.label.authority, AuthorityClass::Environment);
    }

    // ── D-ROBUST ─────────────────────────────────────────────────────────

    #[test]
    fn d_robust_refuses_unendorsed_external_or_tainted() {
        let external = Label::at(AuthorityClass::External);
        let tainted_principal = tainted(AuthorityClass::Principal, "x");
        let inputs = [
            RobustnessInput {
                param: "to".into(),
                label: &external,
                shape_endorsed: false,
            },
            RobustnessInput {
                param: "alias".into(),
                label: &tainted_principal,
                shape_endorsed: false,
            },
        ];
        let bad = d_robust(&inputs).unwrap_err();
        assert_eq!(bad, vec!["to".to_string(), "alias".to_string()]);
    }

    #[test]
    fn d_robust_passes_endorsed_and_clean_high_authority() {
        let external_endorsed = Label::at(AuthorityClass::External);
        let clean = Label::at(AuthorityClass::Environment);
        let inputs = [
            RobustnessInput {
                param: "to".into(),
                label: &external_endorsed,
                shape_endorsed: true,
            },
            RobustnessInput {
                param: "alias".into(),
                label: &clean,
                shape_endorsed: false,
            },
        ];
        assert!(d_robust(&inputs).is_ok());
    }

    // ── recipients + check 3 ─────────────────────────────────────────────

    #[test]
    fn resolve_recipients_collects_string_leaves() {
        let c = contract();
        let mut args = BTreeMap::new();
        args.insert("to".to_string(), Json::str("bob@corp"));
        let rs = resolve_recipients(&c, &args).unwrap();
        assert!(rs.contains("bob@corp"));
        // A declared-but-absent recipient param is an evaluation failure.
        let absent = BTreeMap::new();
        assert!(resolve_recipients(&c, &absent).is_none());
        // No declared recipient params → the empty set (never a failure).
        let mut c2 = contract();
        c2.recipient_params = vec![];
        assert_eq!(resolve_recipients(&c2, &absent), Some(BTreeSet::new()));
    }

    #[test]
    fn reader_coverage_requires_recipients_subset_of_readers() {
        let c = contract();
        let recipients: BTreeSet<String> = ["alice".into()].into_iter().collect();
        let covered = with_readers(Label::at(AuthorityClass::External), &["alice", "bob"]);
        let uncovered = with_readers(Label::at(AuthorityClass::External), &["carol"]);
        let params: BTreeMap<String, Label> = [("body".to_string(), covered)].into_iter().collect();
        let fails = check_reader_coverage(&c, &recipients, |p| params.get(p).map(|l| &l.readers));
        assert!(fails.is_empty());
        let params: BTreeMap<String, Label> =
            [("body".to_string(), uncovered)].into_iter().collect();
        let fails = check_reader_coverage(&c, &recipients, |p| params.get(p).map(|l| &l.readers));
        assert_eq!(fails, vec!["body".to_string()]);
        // Public readers always pass.
        let params: BTreeMap<String, Label> =
            [("body".to_string(), Label::at(AuthorityClass::External))]
                .into_iter()
                .collect();
        let fails = check_reader_coverage(&c, &recipients, |p| params.get(p).map(|l| &l.readers));
        assert!(fails.is_empty());
    }

    #[test]
    fn check3_domains() {
        assert!(check3_relevant("net_egress", false, false));
        assert!(check3_relevant("message_human", false, false));
        assert!(check3_relevant("spend", false, false));
        assert!(check3_relevant("memory_write", false, true));
        assert!(!check3_relevant("memory_write", false, false));
        assert!(check3_relevant("fs_read", true, false));
        assert!(!check3_relevant("fs_read", false, false));
    }

    // ── check_flow — ordering, enforcement class, totality ───────────────

    fn deny_rule(id: &str) -> FlowRule {
        FlowRule {
            rule_id: id.into(),
            selector: FlowSelector::default(),
            condition: None,
            decision: FlowDecision::Deny {
                reason: "ReaderCoverage".into(),
            },
            enforcement: EnforcementClass::Deterministic,
            remedies_hint: vec![],
        }
    }

    #[test]
    fn deny_rules_run_before_allow_rules() {
        let l = Label::at(AuthorityClass::Principal);
        let params = BTreeMap::new();
        let args = BTreeMap::new();
        let rec = BTreeSet::new();
        let committed = vec![];
        let det = BTreeMap::new();
        let i = input(&l, &params, &args, &rec, &committed, &det);
        // The allow is listed first in the vec — the deny tier still wins.
        let rules = vec![
            FlowRule {
                decision: FlowDecision::Allow,
                ..deny_rule("allow-first")
            },
            deny_rule("deny-second"),
        ];
        match check_flow(&rules, &i).unwrap() {
            FlowVerdict::Deny { detail, reason, .. } => {
                assert_eq!(detail, "flow_deny:deny-second");
                assert_eq!(reason, "ReaderCoverage");
            }
            v => panic!("expected Deny, got {v:?}"),
        }
    }

    #[test]
    fn advisory_rules_never_widen() {
        let l = Label::at(AuthorityClass::Principal);
        let params = BTreeMap::new();
        let args = BTreeMap::new();
        let rec = BTreeSet::new();
        let committed = vec![];
        let det = BTreeMap::new();
        let i = input(&l, &params, &args, &rec, &committed, &det);
        let advisory_allow = FlowRule {
            rule_id: "adv".into(),
            selector: FlowSelector::default(),
            condition: None,
            decision: FlowDecision::Allow,
            enforcement: EnforcementClass::Advisory,
            remedies_hint: vec![],
        };
        // An advisory allow falls through to Π — I-F1 no-sensitive-upgrade.
        assert_eq!(
            check_flow(&[advisory_allow], &i).unwrap(),
            FlowVerdict::Fallthrough
        );
        // …but an advisory *deny* applies directly (raising restriction).
        let advisory_deny = FlowRule {
            decision: FlowDecision::Deny {
                reason: "EvaluationError".into(),
            },
            enforcement: EnforcementClass::Advisory,
            ..deny_rule("adv-deny")
        };
        assert!(matches!(
            check_flow(&[advisory_deny], &i).unwrap(),
            FlowVerdict::Deny { .. }
        ));
    }

    #[test]
    fn an_unevaluable_atom_denies_with_evaluation_error() {
        let l = Label::at(AuthorityClass::Principal);
        let params = BTreeMap::new();
        let args = BTreeMap::new();
        let rec = BTreeSet::new();
        let committed = vec![];
        let det = BTreeMap::new();
        let i = input(&l, &params, &args, &rec, &committed, &det);
        // `detector` on an unrecorded key is an evaluation error.
        let rule = FlowRule {
            condition: Some(FlowCond::Detector {
                validator_ref: "v1".into(),
                param: "body".into(),
            }),
            ..deny_rule("det")
        };
        assert!(check_flow(&[rule], &i).is_err());
    }

    #[test]
    fn and_evaluates_every_operand_no_early_exit() {
        // Constant-time-per-atom evidence (AC-14): `And[false, error]` must
        // evaluate the second operand — an early exit would return Ok(false).
        let l = Label::at(AuthorityClass::Principal);
        let params = BTreeMap::new();
        let args = BTreeMap::new();
        let rec = BTreeSet::new();
        let committed = vec![];
        let det = BTreeMap::new();
        let i = input(&l, &params, &args, &rec, &committed, &det);
        let cond = FlowCond::And(vec![
            FlowCond::ArgEq {
                param: "missing".into(),
                value: Json::str("x"),
            },
            FlowCond::TaintEmpty {
                subject: Subject::Proposal,
            },
        ]);
        // The unbound arg atom errors even though a short-circuit could skip it.
        assert!(eval_cond(&cond, &i).is_err());
        // And with a false first operand and an erroring second still surfaces
        // the error (every operand evaluated, first error in document order).
        let mut args2 = BTreeMap::new();
        args2.insert("p".to_string(), Json::str("a"));
        let i2 = input(&l, &params, &args2, &rec, &committed, &det);
        let cond = FlowCond::And(vec![
            FlowCond::ArgEq {
                param: "p".into(),
                value: Json::str("b"), // false
            },
            FlowCond::ArgEq {
                param: "missing".into(), // unbound → error
                value: Json::str("x"),
            },
        ]);
        assert!(eval_cond(&cond, &i2).is_err());
    }

    #[test]
    fn sanitize_rule_asks_with_the_sanitize_remedy() {
        let l = Label::at(AuthorityClass::Principal);
        let params = BTreeMap::new();
        let args = BTreeMap::new();
        let rec = BTreeSet::new();
        let committed = vec![];
        let det = BTreeMap::new();
        let i = input(&l, &params, &args, &rec, &committed, &det);
        let rule = FlowRule {
            decision: FlowDecision::Sanitize {
                sanitizer_ref: "val:strip".into(),
                param: "body".into(),
            },
            ..deny_rule("san")
        };
        match check_flow(&[rule], &i).unwrap() {
            FlowVerdict::Ask { remedies, .. } => {
                assert_eq!(
                    remedies,
                    vec![Remedy::Sanitize {
                        sanitizer_ref: Some("val:strip".into()),
                        param: "body".into()
                    }]
                );
            }
            v => panic!("expected Ask, got {v:?}"),
        }
    }

    #[test]
    fn declassify_rule_widens_to_recipients() {
        let l = Label::at(AuthorityClass::Principal);
        let params = BTreeMap::new();
        let args = BTreeMap::new();
        let rec: BTreeSet<String> = ["alice".into()].into_iter().collect();
        let committed = vec![];
        let det = BTreeMap::new();
        let i = input(&l, &params, &args, &rec, &committed, &det);
        let rule = FlowRule {
            decision: FlowDecision::Declassify {
                readers_to: ReadersTo::Recipients,
            },
            ..deny_rule("decl")
        };
        match check_flow(&[rule], &i).unwrap() {
            FlowVerdict::Declassified { readers_to, .. } => {
                assert_eq!(readers_to, ReaderSet::Restricted(rec.clone()));
            }
            v => panic!("expected Declassified, got {v:?}"),
        }
    }

    // ── per-basis effects (§5g.2 §2.2) ───────────────────────────────────

    #[test]
    fn basis_effects_match_the_table() {
        let from = tainted(AuthorityClass::External, "x");
        // seal/pin/promotion: authority must rise AND taint must clear.
        let mut to = Label::at(AuthorityClass::Definition);
        to.taint.insert(TaintTag::Import {
            source_system: "sys".into(),
        });
        assert_eq!(
            check_basis_effect(EndorsementBasis::Seal, &from, &to),
            Err(BasisEffectError::TaintNotCleared)
        );
        let clean_to = Label::at(AuthorityClass::Definition);
        assert!(check_basis_effect(EndorsementBasis::Seal, &from, &clean_to).is_ok());
        // No authority raise → refused even with taint cleared.
        let same_auth = Label::at(AuthorityClass::External);
        assert_eq!(
            check_basis_effect(EndorsementBasis::Pin, &from, &same_auth),
            Err(BasisEffectError::NoAuthorityRaise)
        );
        // approval never changes a component.
        assert_eq!(
            check_basis_effect(EndorsementBasis::Approval, &from, &clean_to),
            Err(BasisEffectError::NoLabelChange)
        );
        // validator is exactly external → environment with taint cleared.
        assert!(check_basis_effect(
            EndorsementBasis::Validator,
            &Label::at(AuthorityClass::External),
            &Label::at(AuthorityClass::Environment)
        )
        .is_ok());
        assert_eq!(
            check_basis_effect(
                EndorsementBasis::Validator,
                &Label::at(AuthorityClass::External),
                &Label::at(AuthorityClass::Principal)
            ),
            Err(BasisEffectError::ValidatorRange)
        );
        // policy_rule widens readers only — touching authority is refused.
        let base = with_readers(Label::at(AuthorityClass::External), &["alice"]);
        let wider = with_readers(Label::at(AuthorityClass::External), &["alice", "bob"]);
        assert!(check_basis_effect(EndorsementBasis::PolicyRule, &base, &wider).is_ok());
        // Narrowing readers (Public → a restricted set) is not a declassification.
        assert_eq!(
            check_basis_effect(
                EndorsementBasis::PolicyRule,
                &Label::at(AuthorityClass::External),
                &base
            ),
            Err(BasisEffectError::NoLabelIncrease)
        );
        let mut bad = wider.clone();
        bad.authority = AuthorityClass::Principal;
        assert_eq!(
            check_basis_effect(EndorsementBasis::PolicyRule, &base, &bad),
            Err(BasisEffectError::PolicyRuleTouchesNonReaders)
        );
    }

    // ── capacity bits ────────────────────────────────────────────────────

    #[test]
    fn capacity_bits_bounds_only_bounded_schemas() {
        assert_eq!(
            capacity_bits(&Json::obj([("const", Json::Int(1))])),
            Some(0)
        );
        assert_eq!(
            capacity_bits(&Json::obj([("type", Json::str("boolean"))])),
            Some(1)
        );
        assert_eq!(
            capacity_bits(&Json::obj([(
                "enum",
                Json::Arr(vec![Json::str("a"), Json::str("b"), Json::str("c")])
            )])),
            Some(2)
        );
        // Unbounded strings / free text never qualify.
        assert_eq!(
            capacity_bits(&Json::obj([("type", Json::str("string"))])),
            None
        );
        // An open record never qualifies.
        assert_eq!(
            capacity_bits(&Json::obj([
                ("type", Json::str("object")),
                (
                    "properties",
                    Json::obj([("b", Json::obj([("type", Json::str("boolean"))]))])
                ),
            ])),
            None
        );
        // A fixed-shape record sums member capacities.
        assert_eq!(
            capacity_bits(&Json::obj([
                ("type", Json::str("object")),
                ("additionalProperties", Json::Bool(false)),
                (
                    "properties",
                    Json::obj([
                        ("b", Json::obj([("type", Json::str("boolean"))])),
                        ("c", Json::obj([("const", Json::Int(0))])),
                    ])
                ),
            ])),
            Some(1)
        );
        // Bounded integer: ceil(log2(max-min+1)).
        assert_eq!(
            capacity_bits(&Json::obj([
                ("type", Json::str("integer")),
                ("minimum", Json::Int(0)),
                ("maximum", Json::Int(7)),
            ])),
            Some(3)
        );
    }

    // ── remedies ─────────────────────────────────────────────────────────

    #[test]
    fn remedy_round_trips_every_variant() {
        let remedies = vec![
            Remedy::Approval {
                effect_id: "e1".into(),
            },
            Remedy::Sanitize {
                sanitizer_ref: Some("v".into()),
                param: "body".into(),
            },
            Remedy::ShapeEndorse {
                validator_ref: None,
                param: "to".into(),
            },
            Remedy::Branch { spec: "s".into() },
            Remedy::Substitute {
                param: "to".into(),
                min_authority: AuthorityClass::Principal,
            },
            Remedy::Prerequisite {
                domain: "fs_read".into(),
            },
        ];
        for r in remedies {
            let back = Remedy::from_json(&r.to_json(), "t").unwrap();
            assert_eq!(back, r);
        }
        assert!(Remedy::from_json(&Json::obj([("kind", Json::str("nope"))]), "t").is_err());
    }

    #[test]
    fn enumerate_remedies_is_bounded_and_omits_approval_when_unattended() {
        let c = contract();
        let coverage = vec!["body".to_string()];
        let rs = enumerate_remedies("e1", &coverage, &[], &c, false);
        assert!(rs.iter().any(|r| matches!(r, Remedy::Approval { .. })));
        assert!(rs.iter().any(|r| matches!(r, Remedy::Sanitize { .. })));
        let rs = enumerate_remedies("e1", &coverage, &[], &c, true);
        assert!(!rs.iter().any(|r| matches!(r, Remedy::Approval { .. })));
        // Robustness failure → substitute at principal.
        let rs = enumerate_remedies("e1", &[], &["to".to_string()], &c, false);
        assert!(rs.iter().any(|r| matches!(
            r,
            Remedy::Substitute {
                min_authority: AuthorityClass::Principal,
                ..
            }
        )));
    }

    // ── size bound evidence (AC-14) ──────────────────────────────────────

    #[test]
    fn label_bytes_stay_under_the_declared_bound() {
        let mut l = tainted(AuthorityClass::External, "cap-1");
        l.readers = ReaderSet::Restricted((0..8).map(|i| format!("principal-{i}")).collect());
        assert!(label_bytes(&l) <= LABEL_MAX_BYTES);
    }

    #[test]
    fn deeply_nested_conditions_refuse_at_decode() {
        // `Not` nested past COND_MAX_DEPTH must fail closed at decode —
        // depth is bounded at parse, never at eval.
        let mut j = FlowCond::TaintEmpty {
            subject: Subject::Proposal,
        }
        .to_json();
        for _ in 0..COND_MAX_DEPTH + 1 {
            j = Json::obj([("not", j)]);
        }
        assert!(FlowCond::from_json(&j).is_err());
    }
}

#[cfg(test)]
mod timing_tests {
    //! AC-R-2.8.2-14 — decision-cost evidence: `check_flow` cost is linear in
    //! `params + rules + effects`, atoms evaluate without secret-dependent
    //! early exit (the `and_evaluates_every_operand` test above pins the
    //! semantics; this measures the scaling), and a `Label`'s canonical bytes
    //! stay under `LABEL_MAX_BYTES`.
    use super::*;
    use std::time::Instant;

    #[test]
    fn decision_cost_is_linear_in_rules_and_params() {
        let l = Label::at(AuthorityClass::External);
        let mut params = BTreeMap::new();
        let mut args = BTreeMap::new();
        for i in 0..64 {
            let p = format!("p{i}");
            params.insert(p.clone(), Label::at(AuthorityClass::External));
            args.insert(p, Json::str("v"));
        }
        let rec = BTreeSet::new();
        let committed: Vec<CommittedEffect> = (0..32)
            .map(|i| CommittedEffect {
                domain: format!("d{i}"),
                args_hash: format!("sha256:{i}"),
            })
            .collect();
        let det = BTreeMap::new();
        let mk = |n_rules: usize| -> (Vec<FlowRule>, FlowInput<'_>) {
            let rules = (0..n_rules)
                .map(|i| FlowRule {
                    rule_id: format!("r{i}"),
                    selector: FlowSelector {
                        domain: Some("never-matches".into()),
                        capability: None,
                        params: vec![],
                    },
                    condition: Some(FlowCond::And(vec![
                        FlowCond::TaintEmpty {
                            subject: Subject::Proposal,
                        },
                        FlowCond::AuthorityAtLeast {
                            subject: Subject::Proposal,
                            class: AuthorityClass::External,
                        },
                    ])),
                    decision: FlowDecision::Allow,
                    enforcement: EnforcementClass::Deterministic,
                    remedies_hint: vec![],
                })
                .collect();
            let input = FlowInput {
                l_plus: &l,
                param_labels: &params,
                args: &args,
                recipients: &rec,
                domain: "net_egress",
                world: "open",
                committed: &committed,
                detectors: &det,
                capability: "cap-1",
            };
            (rules, input)
        };
        // Warm-up + measurement: 8 vs 128 rules (16×) — the elapsed ratio
        // must stay well inside quadratic (we assert < 64×; observed is
        // ~linear). Wall-clock evidence is recorded in the run ledger.
        let (r8, i8) = mk(8);
        let (r128, i128) = mk(128);
        let t8 = {
            let s = Instant::now();
            for _ in 0..10 {
                let _ = check_flow(&r8, &i8).unwrap();
            }
            s.elapsed()
        };
        let t128 = {
            let s = Instant::now();
            for _ in 0..10 {
                let _ = check_flow(&r128, &i128).unwrap();
            }
            s.elapsed()
        };
        eprintln!("AC-14 evidence: 8 rules {t8:?} · 128 rules {t128:?} (10 iters)");
        assert!(
            t128 < t8
                .saturating_mul(64)
                .max(std::time::Duration::from_millis(50)),
            "decision cost is not linear-bounded: 8r {t8:?} vs 128r {t128:?}"
        );
        // A representative label's canonical bytes stay under the bound.
        let mut lb = Label::at(AuthorityClass::External);
        lb.taint.insert(TaintTag::Tool {
            capability: "cap-1".into(),
            inner_source: None,
        });
        let bytes = label_bytes(&lb);
        eprintln!("AC-14 evidence: label_bytes = {bytes} (bound {LABEL_MAX_BYTES})");
        assert!(bytes <= LABEL_MAX_BYTES);
    }
}
