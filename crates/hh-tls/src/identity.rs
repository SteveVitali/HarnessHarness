//! `identity` — verified peer-identity extraction (R4.5c; ADR-0365 D3).
//!
//! The handshake's webpki validation already *proved* the peer chain;
//! this module only *decodes* the verified end-entity certificate into
//! the coordinates a consumer records: the subject common name and the
//! SAN `dNSName`/`uniformResourceIdentifier` sets (the R4.7a mediator's
//! mTLS peer-identity input, and the wire side's observed server
//! identity). A certificate that failed verification never reaches this
//! code — extraction runs on `peer_certificates()` post-handshake only.

use rustls::pki_types::CertificateDer;
use webpki::EndEntityCert;

/// The verified peer's identity coordinates — `None`-bearing members
/// report absence honestly (a subject with no CN, a cert with no SANs).
#[derive(Debug, Clone, PartialEq)]
pub struct PeerIdentity {
    /// The subject's common name (`OID 2.5.4.3`), when present.
    pub subject_common_name: Option<String>,
    /// The SAN `dNSName` entries (verified against the dialed name on the
    /// client side; the asserted identity set on the server side).
    pub sans_dns: Vec<String>,
    /// The SAN `uniformResourceIdentifier` entries.
    pub sans_uri: Vec<String>,
}

/// `peer_identity(der)` — decode the verified end-entity certificate.
/// `None` when the DER cannot be parsed as an X.509 cert at all (a
/// post-verification impossibility on the rustls path — reported, never
/// guessed).
pub fn peer_identity(der: &CertificateDer<'_>) -> Option<PeerIdentity> {
    let cert = EndEntityCert::try_from(der).ok()?;
    Some(PeerIdentity {
        subject_common_name: common_name(cert.subject()),
        sans_dns: cert.valid_dns_names().map(str::to_string).collect(),
        sans_uri: cert.valid_uri_names().map(str::to_string).collect(),
    })
}

/// `common_name(name_der)` — the CN RDN value out of a DER `Name`.
/// `Name ::= SEQUENCE OF RDN`; `RDN ::= SET OF AttributeTypeAndValue`;
/// `ATV ::= SEQUENCE { OID 2.5.4.3, DirectoryString }` — a minimal,
/// total walk: any member that isn't a CN is skipped, truncation ends
/// the walk. String tags admitted: UTF8String / PrintableString /
/// IA5String / BMPString / TeletexString / UniversalString (decoded
/// UTF-8-lossy for the 2-byte/4-byte forms — a CN is display text here,
/// never a verification input).
fn common_name(name_der: &[u8]) -> Option<String> {
    // `webpki::EndEntityCert::subject()` returns the `Name`'s content
    // octets (the RDN SETs, no outer SEQUENCE) — but accept the wrapped
    // TLV too so the walk is total over either spelling.
    let body = match tlv(name_der) {
        Some((0x30, inner)) => inner,
        _ => name_der,
    };
    let mut rest = body;
    while !rest.is_empty() {
        let ((tag, rdn), used) = tlv_used(rest)?;
        rest = &rest[used..];
        if tag != 0x31 {
            continue; // not a SET
        }
        let mut rrest = rdn;
        while !rrest.is_empty() {
            let ((atag, atv), aused) = tlv_used(rrest)?;
            rrest = &rrest[aused..];
            if atag != 0x30 {
                continue; // not a SEQUENCE (AttributeTypeAndValue)
            }
            let ((otag, oid), oused) = tlv_used(atv)?;
            if otag != 0x06 || oid != [0x55, 0x04, 0x03] {
                continue; // not OID 2.5.4.3 (commonName)
            }
            let ((vtag, vstr), _vused) = tlv_used(&atv[oused..])?;
            return Some(decode_dir_string(vtag, vstr));
        }
    }
    None
}

/// `(tag, content)` at the head of `der` — length-definite only.
fn tlv(der: &[u8]) -> Option<(u8, &[u8])> {
    tlv_used(der).map(|((tag, body), _)| (tag, body))
}

/// `tlv_used` — `(tag, content)` + bytes consumed.
fn tlv_used(der: &[u8]) -> Option<((u8, &[u8]), usize)> {
    if der.len() < 2 {
        return None;
    }
    let tag = der[0];
    let lb = der[1] as usize;
    let (len, hdr) = if lb < 0x80 {
        (lb, 2)
    } else {
        let n = lb & 0x7f;
        if n == 0 || n > 3 || der.len() < 2 + n {
            return None;
        }
        let mut l = 0usize;
        for i in 0..n {
            l = (l << 8) | der[2 + i] as usize;
        }
        (l, 2 + n)
    };
    if der.len() < hdr + len {
        return None;
    }
    Some(((tag, &der[hdr..hdr + len]), hdr + len))
}

/// `decode_dir_string(tag, bytes)` — the DirectoryString renderings we
/// admit (see `common_name`); UTF-8 for the multi-byte forms.
fn decode_dir_string(tag: u8, bytes: &[u8]) -> String {
    match tag {
        // UTF8String / PrintableString / IA5String / TeletexString — the
        // ASCII-compatible spellings decode as UTF-8 directly.
        0x0c | 0x13 | 0x16 | 0x14 => String::from_utf8_lossy(bytes).into_owned(),
        // BMPString — UTF-16BE.
        0x1e => String::from_utf16_lossy(
            &bytes
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        ),
        // UniversalString — UTF-32BE.
        0x1c => bytes
            .chunks_exact(4)
            .filter_map(|c| char::from_u32(u32::from_be_bytes([c[0], c[1], c[2], c[3]])))
            .collect(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}
