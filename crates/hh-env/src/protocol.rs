//! The `hh-helper/1` schema — **single source in `hh-helper`** (S2.1; §5d.5
//! §4 helper row; ADR-0049/OQ-045 + ADR-0050). This module re-exports the
//! codec so existing `crate::protocol::*` paths keep working; the schema,
//! the closed verb sum, the `commit_proof` member, and the version check are
//! defined once, in `hh_helper::protocol`.
//!
//! Kernel posture (I-3): the helper receives resolved execution members +
//! the attribution token + the `commit_proof` — never a `Proposal` or
//! `KernelDecision`. Anything outside the closed sum is `protocol_error`
//! (transport origin).

pub use hh_helper::protocol::{
    mint_commit_proof, verify_commit_proof, CommitProof, HelperFrame, HelperRequest,
    HelperResponse, OnKernelLoss, ProtoError, HELPER_PROTOCOL, OPS,
};
