//! The §8.1 monitor — the Stage-1 checks 1/2/7 (check 5 lives in
//! [`crate::endorse::check_endorsement`]; `4` is the fold-budget/contract check owned by
//! C0/R2.2.2; `3`/`6` are Stage-2/C2 reach):
//!
//! | # | Check                                                     | Lands here?        |
//! |---|-----------------------------------------------------------|--------------------|
//! | 1 | `operand ⊆ envelope` on every `security.permission.*`     | ✅ [`check_operand_in_envelope`] |
//! | 2 | label moves only monotonically toward `unverified`        | ✅ [`check_monotonic_label`] |
//! | 5 | endorsement legitimacy at append                          | ✅ [`crate::endorse::check_endorsement`] |
//! | 7 | no delegate provenance above `delegate` after delegation  | ✅ [`check_no_delegate_above`] |
//!
//! Check 2 delegates the component-wise delta to [`classify_label_delta`] /
//! [`check_label_transition`] (P6 — [`crate::label`]); check 7 delegates to
//! [`check_delegate_attenuation`] (P2 — [`crate::derive`]). Every refusal is a typed error
//! (ADR-0033 D7) — never a warning, never a silent pass.

use std::collections::BTreeSet;

use crate::authority::{AuthorityClass, ReaderSet, TaintTag};
use crate::derive::check_delegate_attenuation;
use crate::label::{check_label_transition, Label, LabelTransitionError};
use crate::record::{ProvenanceError, ProvenanceRecord};

/// A `security.permission.*` envelope (monitor check 1 — §8.1 #4): the ceiling a
/// `permission.proposed`/`granted`/`checked`/`consumed` event keeps its operand inside.
/// `operand ⊆ envelope` means the operand's `authority` is `≤` the ceiling, its `taint` and
/// `readers` stay within the allowed sets, and every scope name it touches is permitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionEnvelope {
    /// The envelope's authority ceiling.
    pub ceiling_authority: AuthorityClass,
    /// Every taint tag the operand may carry (`operand.taint ⊆` this).
    pub allowed_taint: BTreeSet<TaintTag>,
    /// Every reader the operand may expose (`operand.readers ⊆` this).
    pub allowed_readers: ReaderSet,
    /// The capability/scope names the operand may touch; empty = all scopes permitted.
    pub allowed_scopes: BTreeSet<String>,
}

/// An operand checked against a [`PermissionEnvelope`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operand {
    /// The operand's provenance label.
    pub label: Label,
    /// The capability/scope names the operand touches.
    pub scopes: BTreeSet<String>,
}

/// The monitor's typed refusals (checks 1, 2, 7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorError {
    /// Check 1: an operand escapes its `security.permission.*` envelope.
    OperandOutsideEnvelope {
        /// Why the operand escapes.
        detail: String,
    },
    /// Check 2: a non-endorsement label mutation widened (forwarded from
    /// [`check_label_transition`]).
    WideningRefused {
        /// The underlying label-transition refusal.
        inner: LabelTransitionError,
    },
    /// Check 7: a delegate provenance above `delegate` after a delegation (forwarded from
    /// [`check_delegate_attenuation`]).
    DelegateAboveDelegate {
        /// The underlying attenuation refusal.
        inner: ProvenanceError,
    },
}

/// Monitor check 1 — `operand ⊆ envelope` on every `security.permission.*` event (§8.1 #4).
///
/// The operand's label must sit under the envelope ceiling **and** every scope it touches must
/// be in the envelope's allowed set (a non-empty `allowed_scopes` filters; an empty set allows
/// all scopes).
pub fn check_operand_in_envelope(
    operand: &Operand,
    envelope: &PermissionEnvelope,
) -> Result<(), MonitorError> {
    if operand.label.authority > envelope.ceiling_authority {
        return Err(MonitorError::OperandOutsideEnvelope {
            detail: format!(
                "operand authority {} exceeds envelope ceiling {}",
                operand.label.authority.as_str(),
                envelope.ceiling_authority.as_str()
            ),
        });
    }
    let extra_taint: Vec<String> = operand
        .label
        .taint
        .iter()
        .filter(|t| !envelope.allowed_taint.contains(*t))
        .map(|t| t.as_string())
        .collect();
    if !extra_taint.is_empty() {
        return Err(MonitorError::OperandOutsideEnvelope {
            detail: format!("operand taint {extra_taint:?} escapes the envelope"),
        });
    }
    if !readers_within(&operand.label.readers, &envelope.allowed_readers) {
        return Err(MonitorError::OperandOutsideEnvelope {
            detail: "operand readers escape the envelope's allowed readers".to_string(),
        });
    }
    if !envelope.allowed_scopes.is_empty() {
        let extra_scopes: Vec<String> = operand
            .scopes
            .iter()
            .filter(|s| !envelope.allowed_scopes.contains(*s))
            .cloned()
            .collect();
        if !extra_scopes.is_empty() {
            return Err(MonitorError::OperandOutsideEnvelope {
                detail: format!("operand scopes {extra_scopes:?} escape the envelope"),
            });
        }
    }
    Ok(())
}

/// `operand.readers ⊆ envelope.allowed_readers`.
fn readers_within(operand: &ReaderSet, allowed: &ReaderSet) -> bool {
    match (operand, allowed) {
        (ReaderSet::Public, ReaderSet::Public) => true,
        (ReaderSet::Public, ReaderSet::Restricted(_)) => false,
        (ReaderSet::Restricted(_), ReaderSet::Public) => true,
        (ReaderSet::Restricted(o), ReaderSet::Restricted(a)) => o.is_subset(a),
    }
}

/// Monitor check 2 — a label may only move **monotonically toward `unverified`** (§8.1 #4):
/// authority non-increasing, taint non-shrinking, readers non-narrowing — *outside* the sealed
/// `security.label.endorsed`/`declassified` events (the only legitimate raisers, checked by
/// [`crate::endorse::check_endorsement`]). Forwards to P6's [`check_label_transition`].
pub fn check_monotonic_label(before: &Label, after: &Label) -> Result<(), MonitorError> {
    check_label_transition(before, after).map_err(|inner| MonitorError::WideningRefused { inner })
}

/// Monitor check 7 — no delegate provenance above `delegate` after a delegation (§8.1 #4):
/// forwards to [`check_delegate_attenuation`]. The parent's `authority` must be `≤ delegate`
/// and its `taint` must cover every child's accumulated taint.
pub fn check_no_delegate_above(
    parent: &ProvenanceRecord,
    children: &[ProvenanceRecord],
) -> Result<(), MonitorError> {
    check_delegate_attenuation(parent, children)
        .map_err(|inner| MonitorError::DelegateAboveDelegate { inner })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::PersistenceScope;
    use crate::label::LabelDelta;
    use crate::origin::{HumanRole, Origin};

    fn tag(s: &str) -> TaintTag {
        TaintTag::Tool {
            capability: s.to_string(),
            inner_source: None,
        }
    }

    fn label(auth: AuthorityClass, taint: &[&str], readers: &[&str]) -> Label {
        Label {
            authority: auth,
            taint: taint.iter().map(|s| tag(s)).collect(),
            readers: if readers == ["*"] {
                ReaderSet::Public
            } else {
                ReaderSet::Restricted(readers.iter().map(|s| (*s).to_string()).collect())
            },
        }
    }

    #[test]
    fn check1_operand_inside_envelope_passes() {
        let env = PermissionEnvelope {
            ceiling_authority: AuthorityClass::Delegate,
            allowed_taint: BTreeSet::from([tag("p")]),
            allowed_readers: ReaderSet::Public,
            allowed_scopes: BTreeSet::from(["fs:read".to_string()]),
        };
        let op = Operand {
            label: label(AuthorityClass::Delegate, &["p"], &["*"]),
            scopes: BTreeSet::from(["fs:read".to_string()]),
        };
        assert!(check_operand_in_envelope(&op, &env).is_ok());
    }

    #[test]
    fn check1_authority_above_ceiling_is_refused() {
        let env = PermissionEnvelope {
            ceiling_authority: AuthorityClass::Delegate,
            allowed_taint: BTreeSet::new(),
            allowed_readers: ReaderSet::Public,
            allowed_scopes: BTreeSet::new(),
        };
        let op = Operand {
            label: label(AuthorityClass::Principal, &[], &["*"]),
            scopes: BTreeSet::new(),
        };
        assert!(matches!(
            check_operand_in_envelope(&op, &env),
            Err(MonitorError::OperandOutsideEnvelope { .. })
        ));
    }

    #[test]
    fn check1_taint_or_readers_or_scopes_escape_is_refused() {
        let env = PermissionEnvelope {
            ceiling_authority: AuthorityClass::Kernel,
            allowed_taint: BTreeSet::from([tag("ok")]),
            allowed_readers: ReaderSet::Restricted(BTreeSet::from(["alice".to_string()])),
            allowed_scopes: BTreeSet::from(["fs:read".to_string()]),
        };
        // Taint escapes.
        let op = Operand {
            label: label(AuthorityClass::Unverified, &["rogue"], &["alice"]),
            scopes: BTreeSet::from(["fs:read".to_string()]),
        };
        assert!(check_operand_in_envelope(&op, &env).is_err());
        // Readers escape (a reader the envelope does not allow).
        let op = Operand {
            label: label(AuthorityClass::Unverified, &["ok"], &["alice", "eve"]),
            scopes: BTreeSet::from(["fs:read".to_string()]),
        };
        assert!(check_operand_in_envelope(&op, &env).is_err());
        // Scopes escape.
        let op = Operand {
            label: label(AuthorityClass::Unverified, &["ok"], &["alice"]),
            scopes: BTreeSet::from(["fs:write".to_string()]),
        };
        assert!(check_operand_in_envelope(&op, &env).is_err());
    }

    #[test]
    fn check2_only_monotonic_deltas_pass() {
        let hi = label(AuthorityClass::Principal, &[], &["*"]);
        let lo = label(AuthorityClass::Unverified, &["t"], &[]);
        // Narrowing passes; widening (any upward component) is refused.
        assert!(check_monotonic_label(&hi, &lo).is_ok());
        assert!(matches!(
            check_monotonic_label(&lo, &hi),
            Err(MonitorError::WideningRefused { .. })
        ));
        // A taint drop alone is a widening.
        let tainted = label(AuthorityClass::Unverified, &["t"], &[]);
        let untainted = label(AuthorityClass::Unverified, &[], &[]);
        assert!(matches!(
            check_monotonic_label(&tainted, &untainted),
            Err(MonitorError::WideningRefused {
                inner: LabelTransitionError::WideningRefused {
                    delta: LabelDelta::Widening
                }
            })
        ));
    }

    #[test]
    fn check7_forwards_the_attenuation_check() {
        let parent = ProvenanceRecord::minted(
            Origin::participant("h1", "hh.hosting/1"),
            PersistenceScope::Run,
            0,
        );
        let child = ProvenanceRecord::minted(
            Origin::participant("h1/sub", "hh.hosting/1"),
            PersistenceScope::Run,
            0,
        );
        assert!(check_no_delegate_above(&parent, &[child]).is_ok());
        let mut bad = parent.clone();
        bad.authority = AuthorityClass::Principal; // a delegate above delegate
        assert!(matches!(
            check_no_delegate_above(&bad, &[]),
            Err(MonitorError::DelegateAboveDelegate { .. })
        ));
    }

    #[test]
    fn the_human_endorser_is_not_delegate_class() {
        // A human principal endorser is legitimate (not delegate-class) — the model is.
        assert!(!Origin::human("a", HumanRole::Principal).is_delegate_class());
        assert!(Origin::model("m", "r", "x").is_delegate_class());
    }
}
