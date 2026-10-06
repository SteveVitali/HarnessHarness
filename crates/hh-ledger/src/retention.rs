//! Tiered retention (R-2.2.1; BL-11; ADR-0068 R4; ADR-0333) — the declared
//! `RetentionPolicy`, the hot/warm/cold residency classes, and the `HHZ1`
//! cold-tier codec. Transitions land as durable
//! `lifecycle.ledger.tier_transition` rows *before* the byte mutation
//! (durable-before-mutation, same as `gc`); the fold over those rows is the
//! only tier state (restart/replay-rebuildable). `cold` is absorbing at
//! rest — a late citation never silently rewrites a compacted file. The
//! codec keeps the plaintext digest as the file name — `get_blob`
//! decompresses then verifies; corrupt frames answer `BlobCorrupt`.

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::errors::LedgerError;

// ── tier ───────────────────────────────────────────────────────────────

/// The three residency classes a `RetentionPolicy` schedules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RetentionTier {
    /// Default residency — bytes served uncompressed.
    Hot,
    /// Retention-floor residency — bound/pinned bytes held for `bundle_retention_ms`.
    Warm,
    /// Compacted residency — compressed at rest; the last rung before `gc`.
    Cold,
}

impl RetentionTier {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RetentionTier::Hot => "hot",
            RetentionTier::Warm => "warm",
            RetentionTier::Cold => "cold",
        }
    }
}

impl std::str::FromStr for RetentionTier {
    /// `Option`-free callers treat parse failure as a spelling error; the
    /// `"none"` sentinel (first transition's `from_tier`) is not a tier and
    /// fails — callers compare against [`UNTRACKED_TIER`] first.
    type Err = String;
    fn from_str(s: &str) -> Result<RetentionTier, String> {
        match s {
            "hot" => Ok(RetentionTier::Hot),
            "warm" => Ok(RetentionTier::Warm),
            "cold" => Ok(RetentionTier::Cold),
            other => Err(format!("unknown retention tier {other:?}")),
        }
    }
}

/// The `from_tier` spelling on a blob's first transition row — the address
/// was observed but carried no prior tier fact.
pub const UNTRACKED_TIER: &str = "none";

// ── policy ─────────────────────────────────────────────────────────────

/// The declared retention policy — the `policy_ref` every
/// `lifecycle.ledger.tier_transition`/`gc` row names (ADR-0068).
/// Residencies are milliseconds against the store clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionPolicy {
    /// The policy document id — non-empty, stamped on every row.
    pub policy_ref: String,
    /// How long an unbound blob stays `hot` before cooling to `cold`
    /// (the blob-tier floor — ADR-0068 R4's lowest rung).
    pub blob_residency_ms: u64,
    /// How long *bound* content (retained-bundle / leaderboard citations;
    /// store-visible pins) is held `warm` before cooling. The floor
    /// ordering puts bundle manifests above blob tiers — must be
    /// `≥ blob_residency_ms`.
    pub bundle_retention_ms: u64,
    /// Whether `cold` bytes are `HHZ1`-compressed at rest (off ⇒ the tier
    /// is an accounting fact only — the byte stays raw but readable).
    pub compress_cold: bool,
}

impl RetentionPolicy {
    /// The rule set (ADR-0068 R4): `policy_ref` declared;
    /// `bundle_retention_ms ≥ blob_residency_ms`. Violations refuse
    /// `SchemaViolation`, never a silently re-ordered policy.
    pub fn validate(&self) -> Result<(), LedgerError> {
        if self.policy_ref.is_empty() {
            return Err(LedgerError::SchemaViolation {
                detail: "RetentionPolicy.policy_ref is empty — every gc/tier row \
                         must name its policy"
                    .into(),
            });
        }
        if self.bundle_retention_ms < self.blob_residency_ms {
            return Err(LedgerError::SchemaViolation {
                detail: format!(
                    "RetentionPolicy floor ordering violated: bundle_retention_ms {} < \
                     blob_residency_ms {} (bundle manifests sit above blob tiers — R4)",
                    self.bundle_retention_ms, self.blob_residency_ms
                ),
            });
        }
        Ok(())
    }

    /// The canonical form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("policy_ref", Json::str(&self.policy_ref)),
            (
                "blob_residency_ms",
                Json::Int(self.blob_residency_ms as i64),
            ),
            (
                "bundle_retention_ms",
                Json::Int(self.bundle_retention_ms as i64),
            ),
            ("compress_cold", Json::Bool(self.compress_cold)),
        ])
    }

    /// Read a policy back from its canonical form.
    pub fn from_json(j: &Json) -> Result<RetentionPolicy, LedgerError> {
        let bad = |d: &str| LedgerError::SchemaViolation {
            detail: format!("RetentionPolicy: {d}"),
        };
        let m = match j {
            Json::Obj(m) => m,
            _ => return Err(bad("not an object")),
        };
        let str_of = |k: &str| m.get(k).and_then(Json::as_str).ok_or_else(|| bad(k));
        let int_of = |k: &str| -> Result<u64, LedgerError> {
            match m.get(k) {
                Some(Json::Int(n)) if *n >= 0 => Ok(*n as u64),
                _ => Err(bad(k)),
            }
        };
        let p = RetentionPolicy {
            policy_ref: str_of("policy_ref")?.to_string(),
            blob_residency_ms: int_of("blob_residency_ms")?,
            bundle_retention_ms: int_of("bundle_retention_ms")?,
            compress_cold: matches!(m.get("compress_cold"), Some(Json::Bool(true))),
        };
        p.validate()?;
        Ok(p)
    }
}

// ── fold / report types ────────────────────────────────────────────────

/// The folded tier fact — rebuilt at `load_all` (durable, never a cache).
#[derive(Debug, Clone, Copy)]
pub struct TierEntry {
    /// The current residency class.
    pub tier: RetentionTier,
    /// The `at_ms` the current class was entered — the residency clock.
    pub entered_ms: u64,
}

/// One performed transition — the committed row in commit order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TierTransition {
    /// The blob address (`<algorithm>:<hex>`).
    pub address: String,
    /// The previous tier (`hot` placeholder when `from_tracked` is false).
    pub from_tier: RetentionTier,
    /// Whether `from_tier` was a real tier fact (false ⇒ `from_tier: "none"`).
    pub from_tracked: bool,
    /// The new tier.
    pub to_tier: RetentionTier,
    /// The `lifecycle.ledger.tier_transition` event id — the audit fact.
    pub event_id: String,
}

/// The scheduler's report — the rows are durable whether it is kept or not.
#[derive(Debug, Clone, Default)]
pub struct RetentionReport {
    /// The policy stamped on every row.
    pub policy_ref: String,
    /// How many candidate addresses were classified this pass.
    pub evaluated: usize,
    /// The transitions committed (each durable before its byte mutation).
    pub transitions: Vec<TierTransition>,
    /// How many files ended the pass `HHZ1`-compressed (incl. repair).
    pub compressed: usize,
}

// ── HHZ1 codec ─────────────────────────────────────────────────────────

/// The cold-tier frame magic (`HHZ1` + u64-LE raw length + token stream).
const MAGIC: &[u8; 4] = b"HHZ1";

/// Match window — 64 KiB back-reference distance.
const WINDOW: usize = 1 << 16;
/// Minimum match length (4-byte prefix hash keys the table).
const MIN_MATCH: usize = 4;
/// Maximum match length (`0x7F + MIN_MATCH`).
const MAX_MATCH: usize = 0x7F + MIN_MATCH;
/// The literal-run cap (`flag + 1` literals, flag ≤ 0x7F).
const MAX_LIT: usize = 0x80;
/// Prefix-hash table size (power of two).
const TABLE: usize = 1 << 12;

/// Is this file an `HHZ1` frame? (`get_blob`/`evaluate_retention` consult
/// it — a raw cold-tier file is a repairable state, not corruption.)
pub fn is_compressed(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// `compress(raw) → HHZ1 frame` — a deterministic LZ pass: literal runs
/// are flag `0x00..=0x7F` (`flag + 1` bytes); matches are flag
/// `0x80 | (len - MIN_MATCH)` + u16-LE distance. Same input ⇒ same frame.
pub fn compress(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() / 2 + 16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(raw.len() as u64).to_le_bytes());
    let mut table = vec![usize::MAX; TABLE];
    let hash = |b: &[u8], i: usize| -> usize {
        let v = (b[i] as u32)
            | ((b[i + 1] as u32) << 8)
            | ((b[i + 2] as u32) << 16)
            | ((b[i + 3] as u32) << 24);
        (v.wrapping_mul(0x9E37_79B1) >> 20) as usize & (TABLE - 1)
    };
    let mut lit_start = 0usize;
    let mut i = 0usize;
    let flush_lit = |out: &mut Vec<u8>, raw: &[u8], from: usize, to: usize| {
        let mut s = from;
        while s < to {
            let n = (to - s).min(MAX_LIT);
            out.push((n - 1) as u8);
            out.extend_from_slice(&raw[s..s + n]);
            s += n;
        }
    };
    while i + MIN_MATCH <= raw.len() {
        let h = hash(raw, i);
        let cand = table[h];
        table[h] = i;
        if cand != usize::MAX
            && i - cand < WINDOW
            && raw[cand..cand + MIN_MATCH] == raw[i..i + MIN_MATCH]
        {
            let mut len = MIN_MATCH;
            while len < MAX_MATCH && i + len < raw.len() && raw[cand + len] == raw[i + len] {
                len += 1;
            }
            flush_lit(&mut out, raw, lit_start, i);
            out.push(0x80 | ((len - MIN_MATCH) as u8));
            out.extend_from_slice(&((i - cand) as u16).to_le_bytes());
            // Seed the table inside the match — deterministic because
            // positions are consumed in order.
            let end = i + len;
            i += 1;
            while i < end && i + MIN_MATCH <= raw.len() {
                table[hash(raw, i)] = i;
                i += 1;
            }
            i = end;
            lit_start = i;
        } else {
            i += 1;
        }
    }
    flush_lit(&mut out, raw, lit_start, raw.len());
    out
}

/// `decompress(frame) → raw` — a malformed frame refuses
/// `SchemaViolation` (the caller maps it to `BlobCorrupt`).
pub fn decompress(frame: &[u8]) -> Result<Vec<u8>, LedgerError> {
    let bad = |d: &str| LedgerError::SchemaViolation {
        detail: format!("HHZ1 frame: {d}"),
    };
    if frame.len() < 12 || !frame.starts_with(MAGIC) {
        return Err(bad("missing magic/length header"));
    }
    let raw_len = u64::from_le_bytes(frame[4..12].try_into().unwrap()) as usize;
    let mut out = Vec::with_capacity(raw_len);
    let mut i = 12usize;
    while i < frame.len() {
        let flag = frame[i];
        i += 1;
        if flag < 0x80 {
            let n = flag as usize + 1;
            if i + n > frame.len() {
                return Err(bad("literal run overruns the frame"));
            }
            out.extend_from_slice(&frame[i..i + n]);
            i += n;
        } else {
            let len = (flag as usize & 0x7F) + MIN_MATCH;
            if i + 2 > frame.len() {
                return Err(bad("match missing its distance"));
            }
            let dist = u16::from_le_bytes(frame[i..i + 2].try_into().unwrap()) as usize;
            i += 2;
            if dist == 0 || dist > out.len() {
                return Err(bad("match distance outside the emitted prefix"));
            }
            let start = out.len() - dist;
            for k in 0..len {
                let b = out[start + k];
                out.push(b);
            }
        }
    }
    if out.len() != raw_len {
        return Err(bad("decoded length != declared raw length"));
    }
    Ok(out)
}

// ── internals shared with the store ────────────────────────────────────

/// Fold one `lifecycle.ledger.tier_transition` payload into `tiers` — the
/// row's `at_ms` is the residency clock (durable, restart-safe).
pub(crate) fn fold_tier_row(tiers: &mut BTreeMap<String, TierEntry>, payload: &Json) {
    let (Some(addr), Some(to), Some(at)) = (
        payload.get("address").and_then(Json::as_str),
        payload
            .get("to_tier")
            .and_then(Json::as_str)
            .and_then(|s| s.parse().ok()),
        payload.get("at_ms").and_then(|j| match j {
            Json::Int(n) if *n >= 0 => Some(*n as u64),
            _ => None,
        }),
    ) else {
        return;
    };
    tiers.insert(
        addr.to_string(),
        TierEntry {
            tier: to,
            entered_ms: at,
        },
    );
}

/// The `lifecycle.ledger.tier_transition` payload (`{address, from_tier,
/// to_tier, policy_ref, at_ms, compressed}` — all audit fields).
pub(crate) fn transition_payload(
    address: &str,
    from_tier: &str,
    to_tier: RetentionTier,
    policy_ref: &str,
    at_ms: u64,
    compressed: bool,
) -> Json {
    Json::obj([
        ("address", Json::str(address)),
        ("from_tier", Json::str(from_tier)),
        ("to_tier", Json::str(to_tier.as_str())),
        ("policy_ref", Json::str(policy_ref)),
        ("at_ms", Json::Int(at_ms as i64)),
        ("compressed", Json::Bool(compressed)),
    ])
}
