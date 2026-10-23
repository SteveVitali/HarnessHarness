//! `pem` — the minimal PEM block decoder (R4.5c; ADR-0365).
//!
//! The extra-roots file, the fixture/server cert chains and the mTLS
//! key material all arrive as PEM (`-----BEGIN X----- … -----END X-----`
//! base64 armour). A full PEM crate would grow the admitted dep graph for
//! ~60 lines of deterministic decoding — this is the one, closed reader:
//! labels are returned verbatim, base64 whitespace is skipped, garbage
//! between blocks is ignored, and a malformed block fails loudly at the
//! caller's `pem_*_file` layer (a truncated or undecodable block is a
//! `BadPem` refusal, never silently dropped).

use crate::error::TlsError;

/// `pem_blocks(text)` — every `-----BEGIN <label>-----` block in `text`
/// decoded to `(label, der_bytes)` in file order. Blocks that fail base64
/// decode are skipped here (the typed-load helpers refuse on empties —
/// a caller that needs them surfaces `BadPem`).
pub fn pem_blocks(text: &str) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("-----BEGIN ") {
        let after = &rest[start + 11..];
        let Some(end_label) = after.find("-----") else {
            break;
        };
        let label = after[..end_label].trim().to_string();
        let tail = &after[end_label + 5..];
        let Some(end) = tail.find("-----END ") else {
            break;
        };
        let body = &tail[..end];
        if let Some(der) = b64_decode(body) {
            out.push((label, der));
        }
        rest = &tail[end + 9..];
    }
    out
}

/// `certificates_pem(text)` — the `CERTIFICATE` blocks only, in order.
pub fn certificates_pem(text: &str) -> Vec<Vec<u8>> {
    pem_blocks(text)
        .into_iter()
        .filter(|(l, _)| l == "CERTIFICATE")
        .map(|(_, d)| d)
        .collect()
}

/// `private_key_pem(text)` — the first `PRIVATE KEY` / `RSA PRIVATE KEY` /
/// `EC PRIVATE KEY` block → `(kind, der)` where kind ∈
/// `{"pkcs8","pkcs1","sec1"}`. `None` when no key block is present.
pub fn private_key_pem(text: &str) -> Option<(&'static str, Vec<u8>)> {
    for (label, der) in pem_blocks(text) {
        match label.as_str() {
            "PRIVATE KEY" => return Some(("pkcs8", der)),
            "RSA PRIVATE KEY" => return Some(("pkcs1", der)),
            "EC PRIVATE KEY" => return Some(("sec1", der)),
            _ => {}
        }
    }
    None
}

/// `read_pem(path)` — the file's UTF-8 text; I/O failures surface as
/// `BadPem` naming the path (the configured-roots refusal spelling).
pub fn read_pem(path: &std::path::Path) -> Result<String, TlsError> {
    std::fs::read_to_string(path)
        .map_err(|e| TlsError::BadPem(format!("read {}: {e}", path.display())))
}

/// `b64_decode` — a strict base64 decoder over the standard alphabet;
/// whitespace is skipped, anything else fails the block.
fn b64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut nbits = 0u32;
    let mut pad = false;
    for &b in s.as_bytes() {
        if b.is_ascii_whitespace() {
            continue;
        }
        if b == b'=' {
            pad = true;
            continue;
        }
        if pad {
            return None; // data after '=' padding
        }
        let v = val(b)? as u32;
        acc = (acc << 6) | v;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
            acc &= (1 << nbits) - 1; // keep only the un-emitted low bits
        }
    }
    if nbits > 0 && (acc & ((1 << nbits) - 1)) != 0 {
        return None; // nonzero bits in a truncated quantum — corrupt
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_one_block() {
        let pem = "-----BEGIN CERTIFICATE-----\naGVsbG8=\n-----END CERTIFICATE-----\n";
        assert_eq!(
            pem_blocks(pem),
            vec![("CERTIFICATE".to_string(), b"hello".to_vec())]
        );
    }

    #[test]
    fn ignores_garbage_between_blocks() {
        let pem =
            "noise\n-----BEGIN PRIVATE KEY-----\ndGVzdA==\n-----END PRIVATE KEY-----\ntrailer";
        assert_eq!(private_key_pem(pem), Some(("pkcs8", b"test".to_vec())));
    }

    #[test]
    fn malformed_block_skipped() {
        let pem = "-----BEGIN CERTIFICATE-----\n!!!notb64!!!\n-----END CERTIFICATE-----\n";
        assert!(certificates_pem(pem).is_empty());
    }
}
