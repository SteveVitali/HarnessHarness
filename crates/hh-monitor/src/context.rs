//! The kernel `context_label` stamp (I-H9; ADR-0052 D8; ADR-0034 P2 log,
//! CF-118): the kernel computes the context label over the delivered-item
//! list the context-builder variant returns. The variant is *never* in the
//! TCB — this module is the kernel's stamp: the delivered list arrives as
//! `(Label, Delivery)` pairs (structured records, no `Text`), the label is
//! the join, and the stamped record is what step 1 of `authorize` reads.
//!
//! The function is the thin kernel-owned wrapper over
//! [`hh_provenance::context_label`] — a separate function (not a re-export)
//! because the stamp is the TCB boundary: the variant's output is *input*, the
//! kernel's label is *output*, and a call that skipped the stamp is
//! distinguishable in the audit trail (`context.assembled` carries the stamped
//! label as a kernel record).

use hh_provenance::{Delivery, Label};

/// `stamp_context_label(delivered) → Label` — the kernel stamp (I-H9). The
/// delivered-item list is the context-builder variant's *recorded* output:
/// `(label, delivery)` per item; `as_handle` items are excluded by the join
/// (CF-311). The kernel — not the variant — owns the resulting label.
pub fn stamp_context_label(delivered: &[(Label, Delivery)]) -> Label {
    hh_provenance::context_label(delivered)
}
