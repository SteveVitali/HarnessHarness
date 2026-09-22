//! [`Label`] — `(authority, taint, readers)` — and the product-lattice operations
//! `join`/`meet`/`leq` (§8.1 #2; ADR-0033 D3), the P6 no-upward-move guard
//! ([`classify_label_delta`]/[`check_label_transition`]), `context_label` and
//! [`effective_authority`] (P3; ADR-0034 P3).
//!
//! Conventions (§8.1 #2): `join = (min authority, taint ∪, readers ∩)` with `Public` the
//! identity of `∩`; `meet = (max, ∩, ∪)`; `leq(l₁, l₂) ⇔ authority₁ ≥ authority₂ ∧
//! taint₁ ⊆ taint₂ ∧ readers₁ ⊇ readers₂`; top `(kernel, ∅, Public)`; bottom
//! `(unverified, U, ∅)`. `leq(l₁, l₂)` reads "l₁ may flow to l₂" — l₂ is at least as
//! restrictive. C0 **enforces** `authority`; `taint`/`readers` are carried and joined at C0,
//! enforced at C2 (ADR-0033 D4).

use std::collections::BTreeSet;

use crate::authority::{AuthorityClass, ReaderSet, TaintTag};

/// `Label = (authority, taint, readers)` (§8.1 #3). One label in one product lattice — never
/// two schemes (CC1; ADR-0033 D4; ADR-0048 §C CF-078).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// How much the item may command the harness.
    pub authority: AuthorityClass,
    /// Taint carried (enforced at C2).
    pub taint: BTreeSet<TaintTag>,
    /// Who may read the item (enforced at C2).
    pub readers: ReaderSet,
}

impl Label {
    /// A label at `authority` with no taint and public readers.
    pub fn at(authority: AuthorityClass) -> Label {
        Label {
            authority,
            taint: BTreeSet::new(),
            readers: ReaderSet::Public,
        }
    }

    /// The lattice top: `(kernel, ∅, Public)` — flows to everything.
    pub fn top() -> Label {
        Label::at(AuthorityClass::Kernel)
    }

    /// The lattice bottom over the taint universe `u`: `(unverified, U, ∅)` — nothing flows
    /// out of it. `u` is supplied by the caller because the `TaintTag` domain is open-ended.
    pub fn bottom(taint_universe: BTreeSet<TaintTag>) -> Label {
        Label {
            authority: AuthorityClass::Unverified,
            taint: taint_universe,
            readers: ReaderSet::Restricted(BTreeSet::new()),
        }
    }

    /// `join(l₁, l₂) = (min authority, taint ∪, readers ∩)` — the least upper bound in the
    /// flows-to order: the most restrictive combination of the two.
    pub fn join(&self, other: &Label) -> Label {
        Label {
            authority: self.authority.min(other.authority),
            taint: self.taint.union(&other.taint).cloned().collect(),
            readers: self.readers.intersect(&other.readers),
        }
    }

    /// `meet(l₁, l₂) = (max authority, taint ∩, readers ∪)` — the greatest lower bound.
    pub fn meet(&self, other: &Label) -> Label {
        Label {
            authority: self.authority.max(other.authority),
            taint: self.taint.intersection(&other.taint).cloned().collect(),
            readers: self.readers.union(&other.readers),
        }
    }

    /// `leq(l₁, l₂) ⇔ authority₁ ≥ authority₂ ∧ taint₁ ⊆ taint₂ ∧ readers₁ ⊇ readers₂` —
    /// "l₁ may flow to l₂".
    pub fn leq(&self, other: &Label) -> bool {
        self.authority >= other.authority
            && self.taint.is_subset(&other.taint)
            && self.readers.is_superset_of(&other.readers)
    }

    /// `self ⊑ other` spelling of [`Label::leq`].
    pub fn flows_to(&self, other: &Label) -> bool {
        self.leq(other)
    }
}

/// How a label changed between two states (P6; §8.1 #2 postconditions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelDelta {
    /// No component changed.
    None,
    /// Authority fell, taint grew and/or readers narrowed — a lawful move.
    Narrowing,
    /// Authority rose, taint shrank or readers widened — lawful **only** through a
    /// `security.label.endorsed`/`declassified` event on the closed basis list.
    Widening,
    /// A mix of narrowing and widening components — still a widening for P6 purposes
    /// (any upward component requires endorsement).
    MixedWidening,
}

/// Classify the move `from → to` (P6). Any component that rises — higher authority, a taint
/// tag dropped, a reader added — makes the move a widening (mixed deltas count as widening:
/// the upward component still requires endorsement).
pub fn classify_label_delta(from: &Label, to: &Label) -> LabelDelta {
    let widened = to.authority > from.authority
        || !from.taint.is_subset(&to.taint) // a taint tag was dropped
        || !from.readers.is_superset_of(&to.readers); // a reader was added
    let narrowed = to.authority < from.authority
        || !to.taint.is_subset(&from.taint) // a taint tag was added
        || !to.readers.is_superset_of(&from.readers); // a reader was removed
    match (widened, narrowed) {
        (false, false) => LabelDelta::None,
        (false, true) => LabelDelta::Narrowing,
        (true, false) => LabelDelta::Widening,
        (true, true) => LabelDelta::MixedWidening,
    }
}

/// The error a non-endorsement label mutation raises (P6: "the ledger refuses such an
/// append"; §8.1 #5 `AuthorityExceedsOrigin`-family).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelTransitionError {
    /// A widening move attempted outside `endorse`/`declassify`.
    WideningRefused {
        /// The classified delta (`Widening` or `MixedWidening`).
        delta: LabelDelta,
    },
}

/// P6: **no operation outside `endorse`/`declassify` may increase `authority`, shrink `taint`
/// or widen `readers`** — the ledger refuses such an append. Every non-endorsement label
/// mutation passes through this guard.
pub fn check_label_transition(from: &Label, to: &Label) -> Result<(), LabelTransitionError> {
    match classify_label_delta(from, to) {
        LabelDelta::None | LabelDelta::Narrowing => Ok(()),
        delta => Err(LabelTransitionError::WideningRefused { delta }),
    }
}

/// How an item was delivered into a call's context (P3). Items delivered `as handle` do **not**
/// join the `context_label` (CF-311).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Content delivered inline — joins the context label.
    Inline,
    /// Delivered as a handle (reference only) — excluded from the label (CF-311).
    AsHandle,
}

/// `context_label(items delivered in call c) → Label` (P3; §8.1 #2): the join over the labels
/// of items **delivered inline**; `as handle` items are excluded. Recomputed per call from what
/// is delivered — a new principal message never raises it by itself (the label moves only by
/// derivation, delivery, isolation or a legitimate endorsement). At Stage 1 this is the pure
/// function; the kernel-stamping on `context.assembled` lands at C0/Stage 2 (§8.1 #9).
pub fn context_label(delivered: &[(Label, Delivery)]) -> Label {
    delivered
        .iter()
        .filter(|(_, d)| *d == Delivery::Inline)
        .fold(Label::top(), |acc, (l, _)| acc.join(l))
}

/// The quantity the monitor decides on (P3): `{authority, taint}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveAuthority {
    /// `min(authority(proposer), ctx.authority)`.
    pub authority: AuthorityClass,
    /// `taint(ctx) ∪ taint(args)`.
    pub taint: BTreeSet<TaintTag>,
}

/// `effective_authority(proposer, ctx, args) → {authority, taint}` (P3; §8.1 #2): the quantity
/// check 1 hands to `authorize`.
pub fn effective_authority(proposer: &Label, ctx: &Label, args: &Label) -> EffectiveAuthority {
    EffectiveAuthority {
        authority: proposer.authority.min(ctx.authority),
        taint: ctx.taint.union(&args.taint).cloned().collect(),
    }
}

/// The `HirDiff.authority_delta` classification consumed by evolution checks (P6 second half;
/// ADR-0017, ADR-0034 P6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityDelta {
    /// No authority change.
    None,
    /// The diff narrows authority.
    Narrowing,
    /// The diff widens authority.
    Widening,
}

/// The context a `HirDiff` is applied in (P6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffContext {
    /// An evolution context — `authority_delta = widening` is **rejected**.
    Evolution,
    /// Any other context — `widening` requires `human` origin.
    Other,
}

/// The failure a widening diff raises (P6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffAuthorityError {
    /// `authority_delta = widening` in an evolution context (rejected outright).
    WideningInEvolution,
    /// `authority_delta = widening` without a human origin.
    WideningRequiresHuman,
}

/// `HirDiff.authority_delta` gate (P6; §8.1 #2 postconditions): a widening diff is rejected in
/// evolution contexts and requires `human` origin otherwise.
pub fn check_diff_authority_delta(
    delta: AuthorityDelta,
    origin: &crate::origin::Origin,
    context: DiffContext,
) -> Result<(), DiffAuthorityError> {
    if delta != AuthorityDelta::Widening {
        return Ok(());
    }
    match context {
        DiffContext::Evolution => Err(DiffAuthorityError::WideningInEvolution),
        DiffContext::Other => match origin {
            crate::origin::Origin::Human { .. } => Ok(()),
            _ => Err(DiffAuthorityError::WideningRequiresHuman),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::origin::Origin;

    fn tag(s: &str) -> TaintTag {
        TaintTag::Import {
            source_system: s.to_string(),
        }
    }

    fn readers(rs: &[&str]) -> ReaderSet {
        if rs.is_empty() {
            ReaderSet::Restricted(BTreeSet::new())
        } else if rs == ["*"] {
            ReaderSet::Public
        } else {
            ReaderSet::Restricted(rs.iter().map(|s| s.to_string()).collect())
        }
    }

    fn label(a: AuthorityClass, t: &[&str], r: &[&str]) -> Label {
        Label {
            authority: a,
            taint: t.iter().map(|s| tag(s)).collect(),
            readers: readers(r),
        }
    }

    /// Every label in the small test universe (authorities × taint subsets × reader sets).
    fn universe() -> Vec<Label> {
        let auths = [
            AuthorityClass::Unverified,
            AuthorityClass::External,
            AuthorityClass::Delegate,
            AuthorityClass::Principal,
            AuthorityClass::Definition,
            AuthorityClass::Kernel,
        ];
        let taints: Vec<BTreeSet<TaintTag>> = vec![
            BTreeSet::new(),
            BTreeSet::from([tag("a")]),
            BTreeSet::from([tag("b")]),
            BTreeSet::from([tag("a"), tag("b")]),
        ];
        let reader_sets = vec![
            ReaderSet::Public,
            readers(&["alice"]),
            readers(&["bob"]),
            readers(&["alice", "bob"]),
            readers(&[]),
        ];
        let mut out = Vec::new();
        for a in auths {
            for t in &taints {
                for r in &reader_sets {
                    out.push(Label {
                        authority: a,
                        taint: t.clone(),
                        readers: r.clone(),
                    });
                }
            }
        }
        out
    }

    #[test]
    fn join_meet_leq_match_the_spec_table() {
        let l1 = label(AuthorityClass::Principal, &["a"], &["alice", "bob"]);
        let l2 = label(AuthorityClass::External, &["b"], &["alice"]);
        let j = l1.join(&l2);
        assert_eq!(j.authority, AuthorityClass::External); // min
        assert_eq!(j.taint.len(), 2); // ∪
        assert_eq!(j.readers, readers(&["alice"])); // ∩
        let m = l1.meet(&l2);
        assert_eq!(m.authority, AuthorityClass::Principal); // max
        assert!(m.taint.is_empty()); // ∩
        assert_eq!(m.readers, readers(&["alice", "bob"])); // ∪
                                                           // leq: l1 ⊑ l2 iff a1 ≥ a2 ∧ t1 ⊆ t2 ∧ r1 ⊇ r2.
        assert!(label(AuthorityClass::Principal, &[], &["*"]).leq(&label(
            AuthorityClass::External,
            &["a"],
            &["alice"]
        )));
        assert!(!label(AuthorityClass::External, &[], &["*"]).leq(&label(
            AuthorityClass::Principal,
            &[],
            &["*"]
        )));
    }

    #[test]
    fn lattice_laws_hold_over_the_universe() {
        // AC-R-2.1.5-2 lattice half: idempotent, commutative, associative; absorption;
        // top/bottom identities; leq is a partial order consistent with join.
        let u = universe();
        for a in &u {
            assert_eq!(&a.join(a), a, "join idempotent");
            assert_eq!(&a.meet(a), a, "meet idempotent");
            assert!(a.leq(a), "leq reflexive");
            assert!(Label::top().leq(a), "top flows to everything");
            assert!(
                a.leq(&Label::bottom(BTreeSet::from([tag("a"), tag("b")]))),
                "everything flows to bottom"
            );
        }
        for a in &u {
            for b in &u {
                assert_eq!(a.join(b), b.join(a), "join commutative");
                assert_eq!(a.meet(b), b.meet(a), "meet commutative");
                // leq consistent with join: a ⊑ b iff join(a,b) == b.
                assert_eq!(a.leq(b), a.join(b) == *b, "leq⇔join");
                if a.leq(b) && b.leq(a) {
                    assert_eq!(a, b, "leq antisymmetric");
                }
            }
        }
        for a in &u {
            for b in &u {
                for c in &u {
                    assert_eq!(a.join(b).join(c), a.join(&b.join(c)), "join assoc");
                    assert_eq!(a.meet(b).meet(c), a.meet(&b.meet(c)), "meet assoc");
                    // Transitivity of leq.
                    if a.leq(b) && b.leq(c) {
                        assert!(a.leq(c), "leq transitive");
                    }
                }
                // Absorption.
                assert_eq!(&a.join(&a.meet(b)), a);
                assert_eq!(&a.meet(&a.join(b)), a);
            }
        }
    }

    #[test]
    fn p6_refuses_widening_outside_endorsement() {
        let low = label(AuthorityClass::External, &["a"], &["alice"]);
        let high = label(AuthorityClass::Principal, &[], &["*"]);
        // Up in authority: refused.
        assert!(check_label_transition(&low, &high).is_err());
        // Taint dropped: refused.
        assert!(
            check_label_transition(&low, &label(AuthorityClass::External, &[], &["alice"]))
                .is_err()
        );
        // Readers widened: refused.
        assert!(
            check_label_transition(&low, &label(AuthorityClass::External, &["a"], &["*"])).is_err()
        );
        // Narrowing and no-op: allowed.
        assert!(check_label_transition(&high, &low).is_ok());
        assert!(check_label_transition(&low, &low).is_ok());
        // Mixed (authority up, taint up): refused.
        assert!(check_label_transition(
            &low,
            &label(AuthorityClass::Principal, &["a", "b"], &["alice"])
        )
        .is_err());
    }

    #[test]
    fn context_label_joins_inline_items_and_skips_handles() {
        let items = vec![
            (
                label(AuthorityClass::Definition, &[], &["*"]),
                Delivery::Inline,
            ),
            (
                label(AuthorityClass::External, &["a"], &["*"]),
                Delivery::Inline,
            ),
            // Delivered as handle — must not join.
            (
                label(AuthorityClass::Unverified, &["z"], &[]),
                Delivery::AsHandle,
            ),
        ];
        let cl = context_label(&items);
        assert_eq!(cl.authority, AuthorityClass::External);
        assert_eq!(cl.taint.len(), 1);
        // Empty context: the join identity — top.
        assert_eq!(context_label(&[]), Label::top());
    }

    #[test]
    fn effective_authority_is_min_and_taint_union() {
        let proposer = label(AuthorityClass::Delegate, &[], &["*"]);
        let ctx = label(AuthorityClass::External, &["c"], &["alice"]);
        let args = label(AuthorityClass::Principal, &["x"], &["bob"]);
        let eff = effective_authority(&proposer, &ctx, &args);
        assert_eq!(eff.authority, AuthorityClass::External); // min(delegate, external)
        assert_eq!(eff.taint.len(), 2); // {c, x}
    }

    #[test]
    fn widening_diff_rules() {
        let human = Origin::human("h", crate::origin::HumanRole::Principal);
        let model = Origin::model("m", "r", "x");
        // Evolution context rejects widening outright.
        assert_eq!(
            check_diff_authority_delta(AuthorityDelta::Widening, &human, DiffContext::Evolution),
            Err(DiffAuthorityError::WideningInEvolution)
        );
        // Otherwise requires human origin.
        assert_eq!(
            check_diff_authority_delta(AuthorityDelta::Widening, &model, DiffContext::Other),
            Err(DiffAuthorityError::WideningRequiresHuman)
        );
        assert!(
            check_diff_authority_delta(AuthorityDelta::Widening, &human, DiffContext::Other)
                .is_ok()
        );
        // Narrowing/None always pass.
        assert!(check_diff_authority_delta(
            AuthorityDelta::Narrowing,
            &model,
            DiffContext::Evolution
        )
        .is_ok());
        assert!(
            check_diff_authority_delta(AuthorityDelta::None, &model, DiffContext::Other).is_ok()
        );
    }
}
