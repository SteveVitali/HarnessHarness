//! The policy table Π — the typed interpreter and the default rows (§5g.1
//! §2.4; ADR-0052 D5 amended by ADR-0069 Π-1…12 and ADR-0059 D4).
//!
//! `PolicyTable` is a **MUST-data** artifact of the sealed definition at
//! `definition` authority, interpreted by MUST-code. The interpreter is the
//! closed `Cond`/`PolicyRow` machinery below — `deny` rows dominate `ask`,
//! `ask` dominates `allow`; the `unknown` class always reads the
//! `irreversible` column (Π-11); every `ask` is `deny` in `unattended` mode
//! (Π-12, `UnattendedPolicy = deny` at Stage 1 — `defer`/`auto_review` are
//! R-2.8.7's); and `permission_request` is never `allow` (Π-8). Policy may
//! narrow any cell; a move toward `allow` is `authority_delta = widening`
//! ([`policy_delta`] enumerates the probe grid).

use hh_hir::EffectDomain;
use hh_ontology::risk::{RiskClass, RiskReversibility, RiskScope};
use hh_provenance::AuthorityClass;

use crate::assess::{AssessmentInputs, MemoryScope, SecretTransport, Tri};

/// `mode ∈ {attended, unattended}` (§5g.1 §2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A human principal can answer `ask`.
    Attended,
    /// `ask → deny` unless a pre-authorization covers the effect (Π-12).
    Unattended,
}

/// `PiVerdict` — the interpreter's output before the mode transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PiVerdict {
    /// The effect may commit.
    Allow,
    /// A human of class `principal` must decide.
    Ask,
    /// Refused.
    Deny,
}

impl PiVerdict {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            PiVerdict::Allow => "allow",
            PiVerdict::Ask => "ask",
            PiVerdict::Deny => "deny",
        }
    }
}

/// `PiContext` — the closed fact record the rows predicate on. Every field is
/// structured; no `Text` (I-H1).
#[derive(Debug, Clone)]
pub struct PiContext {
    /// The effect domain.
    pub domain: EffectDomain,
    /// The effective risk class (step 3's `max_by_danger` result).
    pub risk: RiskClass,
    /// `min(authority(proposer), ctx)` — step 1.
    pub eff: AuthorityClass,
    /// `taint(context_label) ∪ taint(args)` non-empty — step 1.
    pub tainted: bool,
    /// The recorded assessment inputs (the row predicates read these).
    pub inputs: AssessmentInputs,
}

/// `Cond` — the closed row-predicate sum. `Tri` members hold only on `Yes` —
/// `unknown` never satisfies a condition (fail-closed, OQ-133's ratified half).
#[derive(Debug, Clone, PartialEq)]
pub enum Cond {
    /// Always holds.
    Always,
    /// The domain is.
    DomainIs(EffectDomain),
    /// `risk.reversibility` is.
    RevIs(RiskReversibility),
    /// `risk.scope` is.
    ScopeIs(RiskScope),
    /// `eff ≤` the class (the Π-7 trigger).
    EffAtMost(AuthorityClass),
    /// The proposal is tainted.
    Tainted,
    /// `inside_writable_roots` is `yes`.
    InsideWritableRoots,
    /// `compensation_registered ∧ compensator_covered` (Π-4's full condition).
    CompensationCovered,
    /// `child_contained` is `yes` (Π-9's pass half).
    ChildContained,
    /// `destination_in_scope` is `yes` (ADR-0059 D4).
    SecretDestInScope,
    /// `secret_transport` is.
    SecretTransportIs(SecretTransport),
    /// `model_visible_sink` is `yes` (Π-10 first half).
    ModelVisibleSink,
    /// `spend_over_soft` is `yes` (Π-10 second half).
    SpendOverSoft,
    /// `sole_recipient_principal` is `yes` (the `message_human` exception).
    SoleRecipientPrincipal,
    /// `host_allowlisted` is `yes` (the `net_egress` allow half).
    HostAllowlisted,
    /// `memory_scope ≤` the bound (the `memory_write` domain row).
    MemoryScopeAtMost(MemoryScope),
    /// `pre_authorized` is `yes` (the unattended exception — a `definition`
    /// pre-authorization handle covers the effect; for `wrapped_long_lived`
    /// secret transport it is the sealed `HarnessRule` naming channel and
    /// destination — ADR-0059 D4).
    PreAuthorized,
    /// Negation — the closed sum stays decidable (bounded, one level of
    /// structure; `Not` of `Not` is still a finite fold).
    Not(Box<Cond>),
}

impl Cond {
    fn holds(&self, ctx: &PiContext) -> bool {
        let i = &ctx.inputs;
        match self {
            Cond::Not(c) => !c.holds(ctx),
            Cond::Always => true,
            Cond::DomainIs(d) => ctx.domain == *d,
            Cond::RevIs(r) => ctx.risk.reversibility == *r,
            Cond::ScopeIs(s) => ctx.risk.scope == *s,
            Cond::EffAtMost(a) => ctx.eff <= *a,
            Cond::Tainted => ctx.tainted,
            Cond::InsideWritableRoots => i.inside_writable_roots.is_yes(),
            Cond::CompensationCovered => {
                i.compensation_registered.is_yes() && i.compensator_covered.is_yes()
            }
            Cond::ChildContained => i.child_contained.is_yes(),
            Cond::SecretDestInScope => i.destination_in_scope.is_yes(),
            Cond::SecretTransportIs(t) => i.secret_transport == Some(*t),
            Cond::ModelVisibleSink => i.model_visible_sink.is_yes(),
            Cond::SpendOverSoft => i.spend_over_soft.is_yes(),
            Cond::SoleRecipientPrincipal => i.sole_recipient_principal.is_yes(),
            Cond::HostAllowlisted => i.host_allowlisted.is_yes(),
            Cond::MemoryScopeAtMost(m) => i.memory_scope.map(|s| s <= *m).unwrap_or(false),
            Cond::PreAuthorized => i.pre_authorized.is_yes(),
        }
    }
}

/// `PolicyRow{id, conditions, verdict}` — a row holds when **every** condition
/// does (conjunctive; the closed `Cond` sum keeps the grammar decidable —
/// OQ-132's interim).
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyRow {
    /// The row id (`pi_1`…`pi_11`, `dom_<domain>`, `secret_*`, `floor_*` —
    /// recorded on the decision's `checks[]`).
    pub id: String,
    /// The conjunctive conditions.
    pub conditions: Vec<Cond>,
    /// The verdict when the row matches.
    pub verdict: PiVerdict,
}

/// `PolicyTable{version_id, mode, rows[], default: deny}` (§5g.1 §2.4). The
/// `UnattendedPolicy` is fixed to `deny` at Stage 1 (Π-12's `defer`/
/// `auto_review` members are R-2.8.7's — declared, never silently absent).
#[derive(Debug, Clone)]
pub struct PolicyTable {
    /// The table's version coordinate — lands in `policy_fingerprint`.
    pub version_id: String,
    /// The attendance mode.
    pub mode: Mode,
    /// The rows (floor rows first, then domain defaults; narrowing rows may be
    /// appended — they can only ever *lower* a verdict, the precedence fold
    /// makes widening rows inert).
    pub rows: Vec<PolicyRow>,
    /// Π-12's `UnattendedPolicy` — `deny` at Stage 1.
    pub unattended_policy: UnattendedPolicy,
}

/// Π-12's `UnattendedPolicy ∈ {deny, defer, auto_review}` — `deny` is the
/// Stage-1 default; the other members exist so the recorded sum is the closed
/// one (R-2.8.7 owns their semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnattendedPolicy {
    /// `ask → deny{UnattendedAsk}` (the default).
    Deny,
    /// Defer the decision (Stage 2).
    Defer,
    /// Auto-review (Stage 2; the never-auto set excluded).
    AutoReview,
}

/// The verdicts matched by at least one row — `deny > ask > allow` precedence.
fn fold(matches: impl Iterator<Item = PiVerdict>) -> Option<PiVerdict> {
    matches.max()
}

impl PolicyTable {
    /// `Π(ctx) → verdict` — match rows, `deny > ask > allow`, default `deny`;
    /// then the Π-7 raise (`eff ≤ external`: `allow → ask`; `ask → deny` when
    /// the class is `irreversible` or the scope `external`); then Π-12's
    /// unattended transform (`ask → deny` unless `pre_authorized`, which leaves
    /// the pre-transform verdict — the covering handle still had to exist at
    /// step 2).
    ///
    /// **Two tiers.** The domain-default rows (ADR-0052 D5) carry per-domain
    /// `reversible / compensable / irreversible` columns — they *are* the
    /// domain's verdicts, not restrictions added on top of the floor. When any
    /// matched row is conditioned on `DomainIs(ctx.domain)`, the floor rows are
    /// superseded and only the domain rows fold; otherwise the floor rows
    /// decide. Without this, ask-dominance would make every domain `allow` row
    /// dead code (`Π-3 reversible ⇒ ask` would shadow `model_call ⇒ allow`,
    /// ADR-0059's scoped-secret allows, `dom_spawn_allow`, …).
    pub fn evaluate(&self, ctx: &PiContext) -> (PiVerdict, Vec<String>) {
        let matched: Vec<&PolicyRow> = self
            .rows
            .iter()
            .filter(|r| r.conditions.iter().all(|c| c.holds(ctx)))
            .collect();
        // The deciding set: domain-conditioned rows when any matched, else the
        // floor rows (a `DomainIs(other)` row can't match — every condition
        // must hold — so "contains `DomainIs`" selects this domain's rows).
        let domain_matched: Vec<&PolicyRow> = matched
            .iter()
            .copied()
            .filter(|r| r.conditions.iter().any(|c| matches!(c, Cond::DomainIs(_))))
            .collect();
        let deciding: &[&PolicyRow] = if domain_matched.is_empty() {
            &matched
        } else {
            &domain_matched
        };
        let ids: Vec<String> = deciding.iter().map(|r| r.id.clone()).collect();
        // Π-8: `permission_request` is never `allow` — enforced as a floor,
        // independent of row order (a narrowing-only discipline could still
        // author an allow row; the floor vetoes it).
        let base = if ctx.domain == EffectDomain::PermissionRequest {
            fold(
                deciding
                    .iter()
                    .map(|r| r.verdict)
                    .map(|v| v.max(PiVerdict::Ask)),
            )
            .unwrap_or(PiVerdict::Deny)
        } else {
            fold(deciding.iter().map(|r| r.verdict)).unwrap_or(PiVerdict::Deny)
        };
        // Π-7 — the authority raise: `allow → ask`; `ask → deny` for
        // `irreversible` or `external` scope.
        let raised = if ctx.eff <= AuthorityClass::External {
            match base {
                PiVerdict::Allow => PiVerdict::Ask,
                PiVerdict::Ask
                    if ctx.risk.reversibility == RiskReversibility::Irreversible
                        || ctx.risk.scope == RiskScope::External =>
                {
                    PiVerdict::Deny
                }
                v => v,
            }
        } else {
            base
        };
        // Π-12 — unattended: `ask → deny` unless a `definition` pre-authorization
        // covers the effect (ADR-0053 D5; the pre-authorized cell keeps `base`).
        // The firing is recorded — `pi_12` joins the matched-row ids so the
        // `DenyReason` attribution reads `UnattendedAsk`, not `PolicyDenied`.
        let mut ids = ids;
        let verdict = if self.mode == Mode::Unattended
            && raised == PiVerdict::Ask
            && self.unattended_policy == UnattendedPolicy::Deny
            && !ctx.inputs.pre_authorized.is_yes()
        {
            ids.push("pi_12".to_string());
            PiVerdict::Deny
        } else {
            raised
        };
        (verdict, ids)
    }
}

fn row(id: &str, conds: Vec<Cond>, verdict: PiVerdict) -> PolicyRow {
    PolicyRow {
        id: id.to_string(),
        conditions: conds,
        verdict,
    }
}

/// `default_table(version_id, mode)` — the floor rows Π-1…Π-11 (Π-12 is the
/// mode transform), the ADR-0059 D4 `secret_access` rows, the domain defaults
/// (ADR-0052 D5) and the ADR-0031 untainted-`≥ principal` floor. Deny-by-
/// default: no row matching ⇒ `deny`.
pub fn default_table(version_id: &str, mode: Mode) -> PolicyTable {
    use Cond::*;
    use PiVerdict::*;
    let mut rows: Vec<PolicyRow> = vec![
        // Π-1 — read_only ∧ closed world ⇒ allow (the check-4 exemption).
        row("pi_1", vec![RevIs(RiskReversibility::ReadOnly)], Allow),
        // Π-2 — reversible ∧ workspace_local ∧ inside roots ⇒ allow.
        row(
            "pi_2",
            vec![
                RevIs(RiskReversibility::Reversible),
                ScopeIs(RiskScope::WorkspaceLocal),
                InsideWritableRoots,
            ],
            Allow,
        ),
        // Π-3 — reversible ∧ ¬inside ⇒ ask ("otherwise" — Π-2's complement).
        row(
            "pi_3",
            vec![
                RevIs(RiskReversibility::Reversible),
                Not(Box::new(InsideWritableRoots)),
            ],
            Ask,
        ),
        // Π-4 — compensable ∧ registered ∧ compensator covered ⇒ allow.
        row(
            "pi_4",
            vec![RevIs(RiskReversibility::Compensable), CompensationCovered],
            Allow,
        ),
        // Π-5 — compensable otherwise ⇒ ask (Π-4's complement — the covering
        // grant must reach the compensator for the allow to stand).
        row(
            "pi_5",
            vec![
                RevIs(RiskReversibility::Compensable),
                Not(Box::new(CompensationCovered)),
            ],
            Ask,
        ),
        // Π-6 — irreversible ⇒ ask.
        row("pi_6", vec![RevIs(RiskReversibility::Irreversible)], Ask),
        // Π-9 — spawn_process with child permissions ⊄ parent ⇒ deny.
        row(
            "pi_9",
            vec![
                DomainIs(EffectDomain::SpawnProcess),
                Not(Box::new(ChildContained)),
            ],
            Deny,
        ),
        // Π-8 — permission_request ⇒ ask (never allow — enforced as floor too).
        row("pi_8", vec![DomainIs(EffectDomain::PermissionRequest)], Ask),
        // Π-10 — secret_access to a model-visible sink ⇒ ask (Π-7 may deny).
        row(
            "pi_10a",
            vec![DomainIs(EffectDomain::SecretAccess), ModelVisibleSink],
            Ask,
        ),
        // Π-10 — spend over the soft threshold ⇒ ask.
        row(
            "pi_10b",
            vec![DomainIs(EffectDomain::Spend), SpendOverSoft],
            Ask,
        ),
    ];
    // ADR-0059 D4 — secret_access rows (both modes; no `ask` in unattended —
    // the Π-12 transform applies). `deny > ask > allow` precedence means the
    // restrictive rows must be conditioned so the allow rows' domains don't
    // overlap them.
    rows.extend([
        // destination ∉ grant.scope ⇒ deny, any transport.
        row(
            "secret_dest_out",
            vec![
                DomainIs(EffectDomain::SecretAccess),
                Not(Box::new(SecretDestInScope)),
            ],
            Deny,
        ),
        // `wrapped_long_lived` ⇒ deny unless a sealed `HarnessRule{issuer =
        // definition}` names channel and destination (the `pre_authorized`
        // input at Stage 1).
        row(
            "secret_wrapped_deny",
            vec![
                DomainIs(EffectDomain::SecretAccess),
                SecretTransportIs(SecretTransport::WrappedLongLived),
                Not(Box::new(PreAuthorized)),
            ],
            Deny,
        ),
        row(
            "secret_wrapped_rule",
            vec![
                DomainIs(EffectDomain::SecretAccess),
                SecretTransportIs(SecretTransport::WrappedLongLived),
                PreAuthorized,
            ],
            Allow,
        ),
        row(
            "secret_allow_proxy",
            vec![
                DomainIs(EffectDomain::SecretAccess),
                SecretTransportIs(SecretTransport::ProxyInjected),
                SecretDestInScope,
            ],
            Allow,
        ),
        row(
            "secret_allow_minted",
            vec![
                DomainIs(EffectDomain::SecretAccess),
                SecretTransportIs(SecretTransport::MintedScoped),
                SecretDestInScope,
            ],
            Allow,
        ),
        // `secret_access … / ask / deny` — the irreversible column is a hard
        // deny inside the domain tier (ADR-0052 D5 column; the ADR-0059 allows
        // never lift an irreversible class).
        row(
            "secret_irrev_deny",
            vec![
                DomainIs(EffectDomain::SecretAccess),
                RevIs(RiskReversibility::Irreversible),
            ],
            Deny,
        ),
        // Any other in-scope transport ⇒ ask (attended); unattended `ask →
        // deny` via Π-12 — "no ask row in unattended mode" (ADR-0059 D4).
        row(
            "secret_rest",
            vec![
                DomainIs(EffectDomain::SecretAccess),
                SecretDestInScope,
                Not(Box::new(SecretTransportIs(SecretTransport::ProxyInjected))),
                Not(Box::new(SecretTransportIs(SecretTransport::MintedScoped))),
                Not(Box::new(SecretTransportIs(
                    SecretTransport::WrappedLongLived,
                ))),
            ],
            Ask,
        ),
    ]);
    // Domain default rows for tainted or ≤external proposals (ADR-0052 D5 —
    // after the Π-1 exemption). Columns: the verdict when the floor rows didn't
    // already decide — these rows only ever *restrict* relative to the floor
    // because deny > ask > allow.
    rows.extend([
        // fs_read — the floor covers it (Π-1 read_only ⇒ allow; reversible ⇒
        // Π-3 ask; irreversible ⇒ Π-6 ask). No extra row needed.
        // fs_write — Π-2/Π-3 cover; nothing extra.
        // exec — Π floor applies; an `allow` requires inside-sandbox which the
        // floor's Π-2 already gates on writable roots; the domain row records
        // the sandbox requirement is folded into `inside_writable_roots`.
        // net_egress — ask unless the host is allowlisted.
        row(
            "dom_net_egress_allow",
            vec![
                DomainIs(EffectDomain::NetEgress),
                RevIs(RiskReversibility::Reversible),
                HostAllowlisted,
            ],
            Allow,
        ),
        row(
            "dom_net_egress",
            vec![
                DomainIs(EffectDomain::NetEgress),
                Not(Box::new(HostAllowlisted)),
            ],
            Ask,
        ),
        // spend — ask; irreversible ⇒ deny (the domain row's `irreversible`
        // column; Π-7 denies it for `eff ≤ external` regardless).
        row(
            "dom_spend_irrev",
            vec![
                DomainIs(EffectDomain::Spend),
                RevIs(RiskReversibility::Irreversible),
            ],
            Deny,
        ),
        row("dom_spend", vec![DomainIs(EffectDomain::Spend)], Ask),
        // message_human — ask; allow when the sole recipient is the principal.
        row(
            "dom_msg_principal",
            vec![DomainIs(EffectDomain::MessageHuman), SoleRecipientPrincipal],
            Allow,
        ),
        row(
            "dom_msg",
            vec![
                DomainIs(EffectDomain::MessageHuman),
                Not(Box::new(SoleRecipientPrincipal)),
            ],
            Ask,
        ),
        // spawn_process — allow iff attenuation holds (child contained) at the
        // `reversible` column; compensable/irreversible columns ask (ADR-0052
        // D5's `allow iff attenuation / ask / ask`).
        row(
            "dom_spawn_allow",
            vec![
                DomainIs(EffectDomain::SpawnProcess),
                ChildContained,
                RevIs(RiskReversibility::Reversible),
            ],
            Allow,
        ),
        row(
            "dom_spawn_ask",
            vec![
                DomainIs(EffectDomain::SpawnProcess),
                ChildContained,
                Not(Box::new(RevIs(RiskReversibility::Reversible))),
            ],
            Ask,
        ),
        // memory_write — allow at scope ≤ run, ask at session, deny above.
        row(
            "dom_mem_allow",
            vec![
                DomainIs(EffectDomain::MemoryWrite),
                MemoryScopeAtMost(MemoryScope::Run),
            ],
            Allow,
        ),
        row(
            "dom_mem_session",
            vec![
                DomainIs(EffectDomain::MemoryWrite),
                MemoryScopeAtMost(MemoryScope::Session),
                Not(Box::new(MemoryScopeAtMost(MemoryScope::Run))),
            ],
            Ask,
        ),
        // `deny` above `session` — and an undeclared scope (`None`) is above.
        row(
            "dom_mem_deny",
            vec![
                DomainIs(EffectDomain::MemoryWrite),
                Not(Box::new(MemoryScopeAtMost(MemoryScope::Session))),
            ],
            Deny,
        ),
        // model_call — allow subject to budget (the budget check is R-2.1.6's;
        // the monitor never *makes* the call).
        row(
            "dom_model_call",
            vec![DomainIs(EffectDomain::ModelCall)],
            Allow,
        ),
    ]);
    PolicyTable {
        version_id: version_id.to_string(),
        mode,
        rows,
        unattended_policy: UnattendedPolicy::Deny,
    }
}

/// `policy_delta(base, narrowed)` — the narrowing diagnostic (§5g.1 §2.4:
/// "a change that moves any cell toward `allow` is `authority_delta =
/// widening`"). Enumerates a probe grid of contexts — every domain ×
/// reversibility × scope × {tainted, eff≤external, and each `Tri` input}; a
/// cell where `narrowed` is *less* restrictive than `base` is a widening.
/// Conservative: a widening found is sound; a clean bill is bounded by the
/// grid (the full classifier is C2's).
pub fn policy_delta(base: &PolicyTable, narrowed: &PolicyTable) -> PolicyDelta {
    let mut widenings = Vec::new();
    for ctx in probe_grid() {
        let (b, _) = base.evaluate(&ctx);
        let (n, _) = narrowed.evaluate(&ctx);
        if n < b {
            widenings.push(format!(
                "{}:{}/{}/{}",
                ctx.domain.name(),
                ctx.risk.reversibility.name(),
                ctx.risk.scope.name(),
                ctx.eff.as_str()
            ));
        }
    }
    if widenings.is_empty() {
        PolicyDelta::Narrowing
    } else {
        PolicyDelta::Widening { cells: widenings }
    }
}

/// The `policy_delta` verdict.
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyDelta {
    /// No probed cell moved toward `allow`.
    Narrowing,
    /// At least one probed cell moved toward `allow` — `authority_delta =
    /// widening` (rejected in evolution contexts; human origin only).
    Widening {
        /// The moved cells (domain/reversibility/scope/eff coordinates).
        cells: Vec<String>,
    },
}

/// The probe grid — every domain × reversibility × scope × eff ∈
/// {external, principal} × taint, with the `Tri` inputs swept over
/// {yes, no, unknown} on the members the rows read.
fn probe_grid() -> Vec<PiContext> {
    use hh_hir::EFFECT_DOMAINS;
    let revs = [
        RiskReversibility::ReadOnly,
        RiskReversibility::Reversible,
        RiskReversibility::Compensable,
        RiskReversibility::Irreversible,
    ];
    let scopes = [RiskScope::WorkspaceLocal, RiskScope::External];
    let effs = [AuthorityClass::External, AuthorityClass::Principal];
    let tris = [Tri::Yes, Tri::No, Tri::Unknown];
    let mut out = Vec::new();
    for &domain in EFFECT_DOMAINS.iter() {
        for rev in revs {
            for scope in scopes {
                for eff in effs {
                    for tainted in [false, true] {
                        for t in tris {
                            let inputs = AssessmentInputs {
                                inside_writable_roots: t,
                                host_allowlisted: t,
                                compensation_registered: t,
                                compensator_covered: t,
                                sole_recipient_principal: t,
                                destination_in_scope: t,
                                model_visible_sink: t,
                                spend_over_soft: t,
                                child_contained: t,
                                pre_authorized: t,
                                memory_scope: Some(MemoryScope::Run),
                                ..Default::default()
                            };
                            out.push(PiContext {
                                domain,
                                risk: RiskClass {
                                    reversibility: rev,
                                    repeat_safety: hh_ontology::risk::RepeatSafety::NonIdempotent,
                                    scope,
                                },
                                eff,
                                tainted,
                                inputs,
                            });
                        }
                    }
                }
            }
        }
    }
    out
}
