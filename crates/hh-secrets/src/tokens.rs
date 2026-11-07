//! `tokens` — the one home for capability-token helpers (design/33 F4;
//! R6.10; ADR-C2-A / AR-08): `token_matches` (constant-time compare) and
//! `mint_token` (128-bit `/dev/urandom` mint) lived in `hh-embed::net`
//! and `hh-web::gate`, which no hosting-tier crate may name. They moved
//! here verbatim — `hh-embed::net` and `hh-web::gate` re-export this
//! implementation byte-identically (CC1: one implementation, two
//! re-export points; there is no second token codec).

/// The `Authorization: Bearer` acceptance check — constant-time over the
/// padded bytes so the compare does not leak length or prefix (P2).
pub fn token_matches(expected: &str, presented: &str) -> bool {
    let e = expected.as_bytes();
    let p = presented.as_bytes();
    // Fold the length mismatch into the accumulator — comparing
    // `max(len)` bytes with out-of-range reads mapped to a fixed
    // nonmatching byte keeps the scan length-independent.
    let n = e.len().max(p.len());
    let mut acc = (e.len() ^ p.len()) as u8;
    for i in 0..n {
        let a = e.get(i).copied().unwrap_or(0xFF);
        let b = p.get(i).copied().unwrap_or(0x00);
        acc |= a ^ b;
    }
    acc == 0
}

/// Mint a 128-bit capability token (P2 — the kernel's id-source
/// discipline doesn't cover a per-process secret; `/dev/urandom` is the
/// std-only portable seam and the generation is one-shot per server
/// start).
pub fn mint_token() -> std::io::Result<String> {
    use std::io::Read;
    let mut buf = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}
