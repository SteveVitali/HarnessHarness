//! The compact-range audit tree — the Stage-2 checkpoint/proof layer of
//! R-2.8.6 (§5g.6; ADR-0067 §5(b)). The ledger's durable event hashes are the
//! leaves; the tree head is a single digest committing to the whole ordered
//! prefix, so a signed head detects truncation, rewrite, and equivocation that
//! a per-event hash chain does not.
//!
//! Construction (the RFC-6962-class "compact range" Merkle tree, specialized
//! to the spec's two-tag tree hash §5g.6):
//!
//! ```text
//! leaf = the event's stored `hash`              (the canonical leaf tag IS
//!                                                the ledger.event hash —
//!                                                no second hash family)
//! node = idp digest("idp:ledger-tree/1", 0x01 ‖ left ‖ right)
//! MTH(l[0..n)) = n == 0 ? empty_head
//!              : n == 1 ? l[0]
//!              : node(MTH(l[0..k)), MTH(l[k..n)))   where k = 2^⌊log2 n⌋ < n
//! empty_head = idp digest("idp:ledger-tree/1", 0x01 ‖ H("") ‖ H(""))
//! ```
//!
//! `node`/`empty_head` carry the interior tag byte `0x01` inside the canonical
//! preimage — distinct from every leaf (whose `ledger.event` hash is already a
//! canonical idp digest) so no leaf can ever collide with an interior node.
//!
//! An append-only frontier (§5g.6: "the range append step cannot reorder")
//! maintains the roots of the maximal perfect subtrees covering `leaves[0..n)`
//! in O(log n) per append; `head_at`/`prove_*` replay the fold over a slice —
//! both derive identically (see `compact_range_is_derived` in tests), so no
//! mutable frontier is load-bearing for correctness.

use hh_identity::idp::{idp_digest, idp_id};
use hh_wire::json::Json;
use hh_wire::sha256::sha256_hex;

/// The idp tag anchoring every interior-node digest in the audit tree.
const TREE_TAG: &str = "idp:ledger-tree/1";

/// The canonical interior-node hash: `idp` over `0x01 ‖ left ‖ right`
/// (domain-separated from event-hash leaves, which are `ledger.event`
/// digests — leaf bytes can never be mistaken for node bytes).
fn node_hash(left: &str, right: &str) -> String {
    let mut preimage = Vec::with_capacity(1 + left.len() + right.len());
    preimage.push(0x01);
    preimage.extend_from_slice(left.as_bytes());
    preimage.extend_from_slice(right.as_bytes());
    idp_digest(TREE_TAG, &preimage)
}

/// The deterministic empty-tree head: `idp` over `0x01 ‖ H("") ‖ H("")`
/// (§5g.6). `H` is the canonical hash function — raw SHA-256 bytes of the
/// empty string, rendered hex — so `empty_head` is a constant every
/// implementation can reproduce without any run state.
pub fn empty_head() -> String {
    let h = sha256_hex(b"");
    node_hash(&h, &h)
}

/// The largest power of two strictly less than `n` (n ≥ 2) — the RFC-6962
/// split `k` of `MTH(l[0..n))`.
fn largest_pow2_lt(n: usize) -> usize {
    let mut k = 1;
    while k * 2 < n {
        k *= 2;
    }
    k
}

/// `MTH(leaves)` — the Merkle tree head over a leaf slice. `leaves` are the
/// canonical leaf hashes (the event `hash` strings), already digests; the
/// leaf tag is folded into that upstream `ledger.event` hash.
pub fn mth(leaves: &[String]) -> String {
    match leaves.len() {
        0 => empty_head(),
        1 => leaves[0].clone(),
        n => {
            let k = largest_pow2_lt(n);
            node_hash(&mth(&leaves[..k]), &mth(&leaves[k..]))
        }
    }
}

/// `MTH(l[0..size))` — the head of the prefix of `leaves`. `size` may exceed
/// `leaves.len()` only in the degenerate case (clamped) — callers verify
/// bounds first; kept total so proof verification cannot panic.
pub fn mth_prefix(leaves: &[String], size: usize) -> String {
    mth(&leaves[..size.min(leaves.len())])
}

/// The maintained compact-range state for one run: the list of
/// `(subtree_size, subtree_root)` pairs, left-to-right, covering
/// `leaves[0..n)` as a sum of distinct powers of two (the binary expansion
/// of `n`, largest-first). Append-only: `push` adds one leaf without ever
/// reordering existing roots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompactRange {
    /// `(size, root)` — strictly decreasing sizes, maximal perfect subtrees.
    pub frontier: Vec<(usize, String)>,
    /// The number of leaves folded so far.
    pub n: usize,
}

impl CompactRange {
    /// Rebuild the range from scratch over `leaves` — O(n log n) in the worst
    /// case via the incremental fold; used on store open/recovery and by
    /// `head_at` for arbitrary historical sizes.
    pub fn over(leaves: &[String]) -> Self {
        let mut r = CompactRange::default();
        for l in leaves {
            r.push(l.clone());
        }
        r
    }

    /// Append one leaf hash. The carry fold merges equal-size adjacent roots
    /// right-to-left — the compact-range append step (cannot reorder: roots
    /// are only ever combined pairwise, left before right).
    pub fn push(&mut self, leaf: String) {
        self.frontier.push((1, leaf));
        loop {
            let i = self.frontier.len();
            if i < 2 || self.frontier[i - 2].0 != self.frontier[i - 1].0 {
                break;
            }
            let (size_l, left) = self.frontier[i - 2].clone();
            let right = self.frontier[i - 1].1.clone();
            self.frontier.truncate(i - 2);
            self.frontier.push((size_l * 2, node_hash(&left, &right)));
        }
        self.n += 1;
    }

    /// The current tree head `(tree_size, tree_head)` — folds the frontier
    /// right-to-left so the result equals `mth(leaves)` exactly.
    pub fn head(&self) -> (usize, String) {
        (self.n, frontier_mth(&self.frontier))
    }
}

/// `MTH` of a frontier's leaves — right-fold combining each subtree root with
/// the accumulated right-hand head (the frontier's rightmost subtree is the
/// deepest/rightmost leaves).
fn frontier_mth(frontier: &[(usize, String)]) -> String {
    if frontier.is_empty() {
        return empty_head();
    }
    // Combine left→right as MTH does: the frontier covers l[0..n) as
    // concatenated perfect subtrees; MTH splits at k = largest pow2 < n,
    // which is exactly frontier[0]. Recompute the split fold directly:
    // MTH over concatenated subtrees = node(head(frontier[..j]), head(frontier[j..]))
    // applied recursively — equivalently, fold right-to-left:
    //   acc = last root; for each earlier root: acc = node(root, acc).
    // This equals MTH because at every MTH split the left operand covers a
    // perfect subtree aligned on a power-of-two boundary — which is exactly
    // the frontier decomposition.
    let mut acc = frontier.last().unwrap().1.clone();
    for (size, root) in frontier[..frontier.len() - 1].iter().rev() {
        let _ = size;
        acc = node_hash(root, &acc);
    }
    acc
}

/// A Merkle inclusion proof (RFC-6962 AUDIT_PATH): the sibling subtree heads
/// along the path from leaf `index` to the root of the `tree_size`-leaf tree.
/// Verifier folds the path against the claimed leaf hash; equality with the
/// claimed head proves the leaf sits at `index` in the tree the head
/// commits to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InclusionProof {
    /// The 0-based leaf index (`seq` for ledger leaves).
    pub leaf_index: u64,
    /// The tree size the proof was generated against.
    pub tree_size: u64,
    /// The claimed leaf hash (the event `hash` string).
    pub leaf_hash: String,
    /// Sibling subtree heads, leaf-to-root order.
    pub path: Vec<String>,
}

/// `AUDIT_PATH(index, l[0..size))` — RFC 6962 §2.1.1.
pub fn prove_inclusion(leaves: &[String], index: usize, size: usize) -> Option<InclusionProof> {
    if index >= size || size == 0 || size > leaves.len() {
        return None;
    }
    let mut path = Vec::new();
    audit_path(&leaves[..size], index, &mut path);
    Some(InclusionProof {
        leaf_index: index as u64,
        tree_size: size as u64,
        leaf_hash: leaves[index].clone(),
        path,
    })
}

fn audit_path(leaves: &[String], index: usize, path: &mut Vec<String>) {
    let n = leaves.len();
    if n == 1 {
        return;
    }
    let k = largest_pow2_lt(n);
    if index < k {
        audit_path(&leaves[..k], index, path);
        path.push(mth(&leaves[k..]));
    } else {
        audit_path(&leaves[k..], index - k, path);
        path.push(mth(&leaves[..k]));
    }
}

/// Verify `proof` binds `leaf_hash` to `tree_head`. Pure — no store state;
/// the caller supplies the head it independently holds (e.g. from a signed
/// checkpoint), which is what makes the proof meaningful.
pub fn verify_inclusion(proof: &InclusionProof, leaf_hash: &str, tree_head: &str) -> bool {
    if proof.tree_size == 0 || proof.leaf_index >= proof.tree_size {
        return false;
    }
    if proof.leaf_hash != leaf_hash {
        return false;
    }
    // RFC 6962 §2.1.1 verification fold.
    let mut fn_ = proof.leaf_index;
    let mut sn = proof.tree_size - 1;
    let mut r = proof.leaf_hash.clone();
    for p in &proof.path {
        if sn == 0 {
            return false;
        }
        if fn_ % 2 == 1 || fn_ == sn {
            r = node_hash(p, &r);
            while fn_ != 0 && fn_.is_multiple_of(2) {
                fn_ /= 2;
                sn /= 2;
            }
        } else {
            r = node_hash(&r, p);
        }
        fn_ /= 2;
        sn /= 2;
    }
    sn == 0 && r == tree_head
}

/// A consistency proof that the `from_size`-leaf tree is a prefix of the
/// `to_size`-leaf tree (RFC-6962 SUBPROOF): a list of subtree heads whose
/// fold against the old head yields the new head iff the two trees share
/// the `from_size` prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsistencyProof {
    /// The old (smaller) tree size.
    pub from_size: u64,
    /// The new (larger) tree size.
    pub to_size: u64,
    /// The proof node list (empty when `from_size == to_size` — the heads
    /// must then be equal).
    pub path: Vec<String>,
}

/// `SUBPROOF(m, l[0..n))` — RFC 6962 §2.1.2 with the exact-subtree
/// short-circuit. Returns `None` for `m == 0`, `m > n`, or `n > leaves.len()`.
pub fn prove_consistency(
    leaves: &[String],
    from_size: usize,
    to_size: usize,
) -> Option<ConsistencyProof> {
    if from_size == 0 || from_size > to_size || to_size > leaves.len() {
        return None;
    }
    let path = if from_size == to_size {
        Vec::new()
    } else {
        subproof(&leaves[..to_size], from_size, true)
    };
    Some(ConsistencyProof {
        from_size: from_size as u64,
        to_size: to_size as u64,
        path,
    })
}

/// The SUBPROOF recursion over `leaves[0..n)` for a claimed prefix size `m`.
/// `complete_subtree` (`b` in the RFC) is true when `m` divides the current
/// range exactly — the m-leaf prefix then IS the range and contributes no
/// node.
fn subproof(leaves: &[String], m: usize, complete_subtree: bool) -> Vec<String> {
    let n = leaves.len();
    if m == n {
        return if complete_subtree {
            Vec::new()
        } else {
            vec![mth(leaves)]
        };
    }
    let k = largest_pow2_lt(n);
    if m <= k {
        let mut p = subproof(&leaves[..k], m, complete_subtree);
        p.push(mth(&leaves[k..]));
        p
    } else {
        let mut p = subproof(&leaves[k..], m - k, false);
        p.push(mth(&leaves[..k]));
        p
    }
}

/// Verify `proof` binds `head1` (the `from_size`-leaf tree) and `head2` (the
/// `to_size`-leaf tree) as prefix-consistent — the fold of RFC 6962 §2.1.2,
/// including the "first is a power of two ⇒ prepend `first_hash`" rule.
pub fn verify_consistency(proof: &ConsistencyProof, head1: &str, head2: &str) -> bool {
    let m = proof.from_size;
    let n = proof.to_size;
    if m == 0 || m > n {
        return false;
    }
    if m == n {
        return proof.path.is_empty() && head1 == head2;
    }
    // RFC step 1: when `first` is an exact power of two the transmitted
    // proof omits the old head — prepend it locally.
    let mut nodes: Vec<&str> = Vec::with_capacity(proof.path.len() + 1);
    if m.is_power_of_two() {
        nodes.push(head1);
    }
    nodes.extend(proof.path.iter().map(|s| &**s));
    if nodes.is_empty() {
        return false;
    }
    // RFC steps 2–3.
    let mut fn_ = m - 1;
    let mut sn = n - 1;
    while fn_ % 2 == 1 {
        fn_ /= 2;
        sn /= 2;
    }
    // RFC steps 4–5: `fr` folds toward the old head, `sr` toward the new.
    let mut fr = nodes[0].to_string();
    let mut sr = nodes[0].to_string();
    for c in &nodes[1..] {
        if sn == 0 {
            return false;
        }
        if fn_ % 2 == 1 || fn_ == sn {
            fr = node_hash(c, &fr);
            sr = node_hash(c, &sr);
            while fn_ != 0 && fn_.is_multiple_of(2) {
                fn_ /= 2;
                sn /= 2;
            }
        } else {
            sr = node_hash(&sr, c);
        }
        fn_ /= 2;
        sn /= 2;
    }
    sn == 0 && fr == head1 && sr == head2
}

/// `j.as_int()` narrowed to `u64` — negative/non-integer members are absent
/// values for proof sizes and seqs.
fn json_u64(j: &Json) -> Option<u64> {
    j.as_int().and_then(|v| u64::try_from(v).ok())
}

/// Render an `InclusionProof` as the canonical Group-R result object
/// (`prove_inclusion` returns this verbatim — the proof must be canonical JSON
/// so cross-implementation verifiers agree).
impl InclusionProof {
    /// Canonical wire form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("leaf_index", Json::Int(self.leaf_index as i64)),
            ("tree_size", Json::Int(self.tree_size as i64)),
            ("leaf_hash", Json::str(&self.leaf_hash)),
            ("path", Json::Arr(self.path.iter().map(Json::str).collect())),
        ])
    }

    /// Parse the canonical wire form back (auditor/verifier side).
    pub fn from_json(j: &Json) -> Option<Self> {
        Some(Self {
            leaf_index: json_u64(j.get("leaf_index")?)?,
            tree_size: json_u64(j.get("tree_size")?)?,
            leaf_hash: j.get("leaf_hash")?.as_str()?.to_string(),
            path: match j.get("path")? {
                Json::Arr(items) => items
                    .iter()
                    .map(|i| i.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()?,
                _ => return None,
            },
        })
    }
}

impl ConsistencyProof {
    /// Canonical wire form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("from_size", Json::Int(self.from_size as i64)),
            ("to_size", Json::Int(self.to_size as i64)),
            ("path", Json::Arr(self.path.iter().map(Json::str).collect())),
        ])
    }

    /// Parse the canonical wire form back.
    pub fn from_json(j: &Json) -> Option<Self> {
        Some(Self {
            from_size: json_u64(j.get("from_size")?)?,
            to_size: json_u64(j.get("to_size")?)?,
            path: match j.get("path")? {
                Json::Arr(items) => items
                    .iter()
                    .map(|i| i.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()?,
                _ => return None,
            },
        })
    }
}

/// A summary `checkpoint` entry for `audit_view` (`{seq, tree_size, tree_head,
/// signatures, verification_status}` — the auditor-facing readout).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointSummary {
    /// The checkpoint event's own seq.
    pub seq: u64,
    /// The covered tree size.
    pub tree_size: u64,
    /// The claimed tree head.
    pub tree_head: String,
    /// Signature entries verbatim from the claim.
    pub signatures: Json,
    /// `"verified"` | `"failed"` | `"unverified"` — recomputed head and
    /// linkage agree with the claim; signatures are shape-checked only
    /// unless a key resolver ran (`verify_run` upgrades to `verified`).
    pub verification_status: &'static str,
}

/// One signed-checkpoint claim parsed from a `security.audit.checkpoint`
/// event payload. Parsing is deliberately permissive (`Option` fields) — the
/// *verifier* decides what's missing/malformed, so a forged or truncated
/// claim surfaces as a named fault rather than a parse panic.
#[derive(Debug, Clone)]
pub struct CheckpointClaim {
    /// `{periodic, effect_terminal, final, rotation, on_demand}`.
    pub kind: String,
    /// Who raised it (`"kernel"` or a participant ref).
    pub origin: String,
    /// The claimed covered tree size (== the checkpoint event's own seq).
    pub tree_size: Option<u64>,
    /// The claimed `MTH(leaves[0..tree_size))`.
    pub tree_head: Option<String>,
    /// `leaves[tree_size-1]` — the event-hash chain tip at coverage.
    pub chain_hash: Option<String>,
    /// The prior checkpoint linkage: `{event_id, tree_size, tree_head}`.
    pub prev_checkpoint: Option<Json>,
    /// Cross-run anchor claims `[{other_run, other_head{tree_size,
    /// tree_head}, relation}]`.
    pub cross_run_anchors: Vec<Json>,
    /// The checkpoint's canonical identity (`idp` digest of the claim minus
    /// `signatures`/`idp`).
    pub idp: Option<String>,
    /// `[{key_id, alg_ref, sig}]`.
    pub signatures: Vec<Json>,
    /// The run's `audit_policy_ref` echoed into the claim.
    pub audit_policy_ref: Option<String>,
    /// The raw payload (for re-deriving `idp`/signing preimage).
    pub payload: Json,
}

/// Parse a checkpoint claim out of an event payload. `None` when the payload
/// can't even spell the class's mandatory shape (the verifier treats `None`
/// as `checkpoint_invalid`).
pub fn parse_checkpoint(payload: &Json) -> Option<CheckpointClaim> {
    let kind = payload.get("kind")?.as_str()?.to_string();
    let origin = payload.get("origin")?.as_str()?.to_string();
    let signatures = match payload.get("signatures") {
        Some(Json::Arr(items)) => items.clone(),
        _ => Vec::new(),
    };
    let cross_run_anchors = match payload.get("cross_run_anchors") {
        Some(Json::Arr(items)) => items.clone(),
        _ => Vec::new(),
    };
    Some(CheckpointClaim {
        kind,
        origin,
        tree_size: payload.get("tree_size").and_then(json_u64),
        tree_head: payload
            .get("tree_head")
            .and_then(|j| j.as_str())
            .map(str::to_string),
        chain_hash: payload
            .get("chain_hash")
            .and_then(|j| j.as_str())
            .map(str::to_string),
        prev_checkpoint: payload.get("prev_checkpoint").cloned(),
        cross_run_anchors,
        idp: payload
            .get("idp")
            .and_then(|j| j.as_str())
            .map(str::to_string),
        signatures,
        audit_policy_ref: payload
            .get("audit_policy_ref")
            .and_then(|j| j.as_str())
            .map(str::to_string),
        payload: payload.clone(),
    })
}

/// The canonical unsigned-checkpoint preimage: the claim object with `idp`,
/// `signatures`, `witness_cosignatures`, `rehash`, and `consistency_proof`
/// removed — the members that depend on the signature itself or are
/// out-of-scope for the signed claim. `idp` and `sig` are both digests over
/// exactly these bytes, so sign/verify recompute the same input.
pub fn unsigned_checkpoint(payload: &Json) -> Json {
    match payload {
        Json::Obj(members) => {
            let mut out = std::collections::BTreeMap::new();
            for (k, v) in members {
                match k.as_str() {
                    "idp"
                    | "signatures"
                    | "witness_cosignatures"
                    | "rehash"
                    | "consistency_proof" => {}
                    _ => {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }
            Json::Obj(out)
        }
        _ => payload.clone(),
    }
}

/// The checkpoint claim's `idp` — `idp/1` over the canonical bytes of the
/// unsigned claim (one identity scheme — CC1; same preimage the signer signs).
pub fn checkpoint_idp(payload: &Json) -> String {
    idp_id(
        "idp:ledger-checkpoint/1",
        unsigned_checkpoint(payload)
            .to_canonical_string()
            .as_bytes(),
    )
}

/// The canonical byte string the checkpoint signature covers: the unsigned
/// claim's canonical JSON (CC1 — the signer and every verifier compute the
/// identical preimage).
pub fn checkpoint_sig_preimage(payload: &Json) -> Vec<u8> {
    unsigned_checkpoint(payload)
        .to_canonical_string()
        .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaves(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("leaf{i:03}")).collect()
    }

    #[test]
    fn empty_head_is_deterministic() {
        assert_eq!(empty_head(), empty_head());
        assert_ne!(empty_head(), "");
    }

    #[test]
    fn mth_single_leaf_is_the_leaf() {
        let l = leaves(1);
        assert_eq!(mth(&l), "leaf000");
    }

    #[test]
    fn compact_range_matches_mth() {
        for n in 1..64 {
            let l = leaves(n);
            let range = CompactRange::over(&l);
            assert_eq!(range.head().0, n);
            assert_eq!(range.head().1, mth(&l), "n={n}");
        }
    }

    #[test]
    fn head_grows_with_appends() {
        let l = leaves(8);
        let mut r = CompactRange::default();
        let mut heads = Vec::new();
        for (i, leaf) in l.iter().enumerate() {
            r.push(leaf.clone());
            let (n, head) = r.head();
            assert_eq!(n, i + 1);
            heads.push(head);
        }
        // Every head distinct — extension changes the commitment.
        for i in 0..heads.len() {
            for j in i + 1..heads.len() {
                assert_ne!(heads[i], heads[j]);
            }
        }
    }

    #[test]
    fn inclusion_roundtrip() {
        for n in [1usize, 2, 3, 5, 8, 13, 16, 100] {
            let l = leaves(n);
            let head = mth(&l);
            for i in 0..n {
                let p = prove_inclusion(&l, i, n).unwrap();
                assert!(verify_inclusion(&p, &l[i], &head), "n={n} i={i}");
            }
        }
    }

    #[test]
    fn inclusion_rejects_wrong_leaf_index_and_head() {
        let l = leaves(10);
        let head = mth(&l);
        let p = prove_inclusion(&l, 3, 10).unwrap();
        assert!(!verify_inclusion(&p, "bogus", &head));
        assert!(!verify_inclusion(&p, &l[4], &head));
        let other = mth(&leaves(9));
        assert!(!verify_inclusion(&p, &l[3], &other));
        let mut bad = p.clone();
        bad.leaf_index = 4;
        assert!(!verify_inclusion(&bad, &l[3], &head));
        // A wrong tree_size fails when the *head* is the one for that size —
        // the signed (tree_size, tree_head) pair is what binds the proof.
        let mut bad2 = p.clone();
        bad2.tree_size = 9;
        assert!(!verify_inclusion(&bad2, &l[3], &other));
        let mut bad3 = p.clone();
        bad3.path.pop();
        assert!(!verify_inclusion(&bad3, &l[3], &head));
    }

    #[test]
    fn consistency_roundtrip() {
        for n in [2usize, 3, 5, 8, 9, 16, 31, 100] {
            let l = leaves(n);
            for m in 1..n {
                let p = prove_consistency(&l, m, n).unwrap();
                assert_eq!(p.from_size, m as u64);
                assert_eq!(p.to_size, n as u64);
                let h1 = mth_prefix(&l, m);
                let h2 = mth(&l);
                assert!(verify_consistency(&p, &h1, &h2), "m={m} n={n}");
            }
        }
    }

    #[test]
    fn consistency_rejects_forked_prefix() {
        let l = leaves(10);
        let p = prove_consistency(&l, 5, 10).unwrap();
        let good_h1 = mth_prefix(&l, 5);
        let h2 = mth(&l);
        // A different head at size 5 (the writer forked the prefix).
        let mut forked = leaves(5);
        forked[2] = "different".to_string();
        let bad_h1 = mth(&forked);
        assert!(!verify_consistency(&p, &bad_h1, &h2));
        assert!(verify_consistency(&p, &good_h1, &h2));
        // Truncated proof.
        let mut short = p.clone();
        if !short.path.is_empty() {
            short.path.pop();
        }
        assert!(!verify_consistency(&short, &good_h1, &h2));
        // Same-size degenerate.
        let same = prove_consistency(&l, 10, 10).unwrap();
        assert!(verify_consistency(&same, &h2, &h2));
        assert!(!verify_consistency(&same, &good_h1, &h2));
    }

    #[test]
    fn proof_json_roundtrips() {
        let l = leaves(7);
        let p = prove_inclusion(&l, 4, 7).unwrap();
        let j = p.to_json();
        let p2 = InclusionProof::from_json(&j).unwrap();
        assert_eq!(p, p2);
    }
}
