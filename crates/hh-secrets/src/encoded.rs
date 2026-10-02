//! The `known_value_encoded` detector's encoding forms (§5g.3 §8 LT-12;
//! ADR-0059; S3.11b) — the C1/Stage-3 slice of the detector family: the
//! encodings an exfiltrating tool output reaches for when a verbatim value
//! would mask — base64 (standard + URL alphabets, padded), lowercase and
//! uppercase hex, and `%XX` percent-encoding.
//!
//! The forms are *pure functions of a known value* — the detector never
//! guesses encodings on untrusted text, it enumerates the canonical encodings
//! of each mask-set value and scans for those spellings. That keeps the
//! detector deterministic and corpus-measurable: `secret_detector_miss_rate`
//! (suite level, `veto: false`) is folded over a seeded corpus by
//! [`miss_rate`], and the canary remains the encoding-independent backstop
//! (ADR-0059 D3 — a miss is a measured fact, never a silent clean).
//!
//! False-positive discipline: encoded forms are only produced for values of
//! at least [`ENCODED_MIN_LEN`] bytes — a short value's hex/base64 spelling is
//! too close to ordinary tokens to be evidence (the corpus measures the rest).

/// The minimum value length an encoded form is produced for — shorter
/// spellings are too close to ordinary tokens (base64 of a 4-byte value is a
/// plausible word). The verbatim `known_value` detector still covers them.
pub const ENCODED_MIN_LEN: usize = 8;

const B64_STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64(alphabet: &[u8; 64], bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(alphabet[(n >> 18) as usize & 63] as char);
        out.push(alphabet[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            alphabet[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            alphabet[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn hex(bytes: &[u8], upper: bool) -> String {
    const LO: &[u8; 16] = b"0123456789abcdef";
    const HI: &[u8; 16] = b"0123456789ABCDEF";
    let table = if upper { HI } else { LO };
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(table[(b >> 4) as usize] as char);
        out.push(table[(b & 0x0f) as usize] as char);
    }
    out
}

/// Percent-encode — every byte becomes `%XX` (uppercase hex digits). The
/// conservative form: a mixed-encoding attacker would still trip on the
/// verbatim/hex/base64 arms; the corpus measures what this form misses.
fn percent(bytes: &[u8]) -> String {
    const HI: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 3);
    for &b in bytes {
        out.push('%');
        out.push(HI[(b >> 4) as usize] as char);
        out.push(HI[(b & 0x0f) as usize] as char);
    }
    out
}

/// Every canonical encoded spelling of `value` the `known_value_encoded`
/// detector scans for — `{base64_std, base64_url, hex_lower, hex_upper,
/// percent}` (§5g.3 §8's "(base64/hex/URL) variants"). Empty for values under
/// [`ENCODED_MIN_LEN`].
pub fn encoded_forms(value: &str) -> Vec<String> {
    let bytes = value.as_bytes();
    if bytes.len() < ENCODED_MIN_LEN {
        return Vec::new();
    }
    vec![
        base64(B64_STD, bytes),
        base64(B64_URL, bytes),
        hex(bytes, false),
        hex(bytes, true),
        percent(bytes),
    ]
}

/// One seeded corpus item for the `secret_detector_miss_rate` fold — the
/// planted text plus the count of secret spellings the detectors are expected
/// to catch inside it (`planted` — the corpus author counts placements, never
/// assumes the detector will).
#[derive(Debug, Clone, PartialEq)]
pub struct CorpusItem {
    /// The corpus text (tool-output shaped).
    pub text: String,
    /// The number of planted secret spellings expected to be detected.
    pub planted: usize,
}

/// The `secret_detector_miss_rate` report — `planted` expected detections,
/// `detected` caught, `misses = planted − detected` (a hit *beyond* the
/// planted count is reported under `extra`, never netted against misses — a
/// detector that over-fires is a different anomaly). `miss_rate_ppm` is
/// `misses / planted` in ppm (`0..=1_000_000`; `None` on an empty corpus —
/// `n/a{estimator_undefined}`, never 0).
#[derive(Debug, Clone, PartialEq)]
pub struct MissRateReport {
    /// Spellings the corpus planted.
    pub planted: u64,
    /// Planted spellings the detectors caught.
    pub detected: u64,
    /// Planted spellings the detectors missed.
    pub misses: u64,
    /// Detector hits beyond the planted expectation (over-fire signal).
    pub extra: u64,
    /// `misses / planted` in ppm — `None` when `planted = 0`.
    pub miss_rate_ppm: Option<i64>,
}

/// `secret_detector_miss_rate` — the suite-level corpus fold (§5g.3 §8:
/// "`secret_detector_miss_rate` on a seeded corpus is reported"; veto:
/// false, level: suite). `detect_fn` is the caller's detector sweep
/// (`crate::redact::detect`) so the fold stays pure over its inputs.
pub fn miss_rate(corpus: &[CorpusItem], detect_fn: impl Fn(&str) -> usize) -> MissRateReport {
    let mut planted = 0u64;
    let mut detected = 0u64;
    let mut extra = 0u64;
    for item in corpus {
        planted += item.planted as u64;
        let hits = detect_fn(&item.text) as u64;
        let p = item.planted as u64;
        detected += hits.min(p);
        extra += hits.saturating_sub(p);
    }
    let misses = planted.saturating_sub(detected);
    MissRateReport {
        planted,
        detected,
        misses,
        extra,
        miss_rate_ppm: if planted == 0 {
            None
        } else {
            Some((misses as u128 * 1_000_000 / planted as u128) as i64)
        },
    }
}
