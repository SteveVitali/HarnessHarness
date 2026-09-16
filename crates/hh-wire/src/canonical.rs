//! Strict canonical-form parsing (S1.4; CC1). [`crate::json::parse`] is the *lenient*
//! reader — it accepts insignificant whitespace, unsorted members and duplicate keys. The
//! canonical encoding admits none of those: `parse_canonical` parses with the lenient
//! reader, re-serializes with the one canonicalizer ([`Json::to_canonical_string`]) and
//! accepts only when the re-serialization is byte-identical to the input. Anything else —
//! whitespace, unsorted members, duplicate keys, a non-minimal form — is rejected, so a
//! consumer at the trust boundary never silently normalizes authored bytes.

use crate::json::{parse, Json, JsonError};

/// Parse `bytes` as canonical JSON — UTF-8, then the sorted-key compact form, byte-exact.
///
/// Errors are `JsonError` with a description of where the input departed from canonical
/// form; callers at a spec boundary map this to `NonCanonicalInput`.
pub fn parse_canonical(bytes: &[u8]) -> Result<Json, JsonError> {
    let text =
        std::str::from_utf8(bytes).map_err(|e| JsonError(format!("input is not UTF-8: {e}")))?;
    let value = parse(text)?;
    let reserialized = value.to_canonical_string();
    if reserialized.as_bytes() != bytes {
        return Err(JsonError(
            "input is not in canonical form (whitespace, member order or duplicate keys)"
                .to_string(),
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_bytes_parse() {
        let v = parse_canonical(br#"{"a":1,"b":[true,null]}"#).unwrap();
        assert_eq!(v.to_canonical_string(), r#"{"a":1,"b":[true,null]}"#);
    }

    #[test]
    fn whitespace_is_not_canonical() {
        assert!(parse_canonical(b"{ \"a\": 1 }").is_err());
        assert!(parse_canonical(b"{\"a\":1} ").is_err());
        assert!(parse_canonical(b" {\"a\":1}").is_err());
        assert!(parse_canonical(b"{\"a\":1}\n").is_err());
    }

    #[test]
    fn unsorted_members_are_not_canonical() {
        assert!(parse_canonical(br#"{"b":2,"a":1}"#).is_err());
    }

    #[test]
    fn duplicate_keys_are_not_canonical() {
        assert!(parse_canonical(br#"{"a":1,"a":2}"#).is_err());
    }

    #[test]
    fn non_utf8_is_not_canonical() {
        assert!(parse_canonical(&[0xff, 0xfe]).is_err());
    }
}
