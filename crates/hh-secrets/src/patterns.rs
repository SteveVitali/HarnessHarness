//! The registered `pattern` detector set (§5g.3 §2 `redact`'s detector family;
//! ADR-0057 D5): a small, closed, deterministic list of provider-credential
//! shapes. Each pattern reports match **spans**, never captures — the matched
//! bytes are replaced before they can travel.
//!
//! The set is deliberately Stage-1-shaped: fixed prefix + character-class +
//! minimum-length matchers (no regex engine — the workspace is std-only).
//! Encoded-variant detection (base64/rot13/…) is the Stage-3 canary row
//! (ADR-0059; DF-S1.13-*) — a pattern only matches the *spelling* it declares.

/// A credential-shape detector — `find` returns every `(start, end)` byte span
/// in `text` matching the shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretPattern {
    /// The registered pattern name (the `Hit.detector` detail and the
    /// tombstone label).
    pub name: &'static str,
    /// The literal prefix that anchors a candidate match.
    pub prefix: &'static str,
    /// The character class the token body must satisfy.
    pub charset: CharClass,
    /// The minimum body length after the prefix.
    pub min_len: usize,
}

/// The byte-class a token body must satisfy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharClass {
    /// `[A-Za-z0-9]` plus `_` and `-`.
    Token,
    /// `[0-9A-Z]` only (e.g. AWS access-key bodies).
    UpperAlnum,
    /// Any non-whitespace, non-delimiter run (for `-----BEGIN … PRIVATE
    /// KEY-----`-style blocks the matcher is literal-block, handled specially).
    Literal,
}

impl SecretPattern {
    /// Every match span of this pattern in `text` (deterministic, leftmost).
    pub fn find_all(&self, text: &str) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let bytes = text.as_bytes();
        let mut from = 0;
        while let Some(rel) = text[from..].find(self.prefix) {
            let start = from + rel;
            if self.charset == CharClass::Literal {
                spans.push((start, start + self.prefix.len()));
                from = start + self.prefix.len();
                continue;
            }
            let body_start = start + self.prefix.len();
            let mut end = body_start;
            while end < bytes.len() && self.charset.admits(bytes[end]) {
                end += 1;
            }
            if end - body_start >= self.min_len {
                // The prefix must not be part of a longer identifier
                // (`Xsk-…` is not a token).
                if start == 0 || !is_ident_char(bytes[start - 1]) {
                    spans.push((start, end));
                }
            }
            from = body_start.max(start + 1);
        }
        spans
    }
}

impl CharClass {
    fn admits(self, b: u8) -> bool {
        match self {
            CharClass::Token => b.is_ascii_alphanumeric() || b == b'_' || b == b'-',
            CharClass::UpperAlnum => b.is_ascii_uppercase() || b.is_ascii_digit(),
            CharClass::Literal => false,
        }
    }
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The Stage-1 registered pattern set — the provider-token shapes `redact`'s
/// `pattern` detector sweeps. Ordered by name; the set is closed (growth is a
/// new registered row, not a caller-supplied pattern).
pub const REGISTERED_PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "google_api_key",
        prefix: "AIza",
        charset: CharClass::Token,
        min_len: 35,
    },
    SecretPattern {
        name: "aws_access_key",
        prefix: "AKIA",
        charset: CharClass::UpperAlnum,
        min_len: 16,
    },
    SecretPattern {
        name: "github_token",
        prefix: "ghp_",
        charset: CharClass::Token,
        min_len: 20,
    },
    SecretPattern {
        name: "github_oauth",
        prefix: "gho_",
        charset: CharClass::Token,
        min_len: 20,
    },
    SecretPattern {
        name: "github_pat",
        prefix: "github_pat_",
        charset: CharClass::Token,
        min_len: 20,
    },
    SecretPattern {
        name: "openai_key",
        prefix: "sk-",
        charset: CharClass::Token,
        min_len: 20,
    },
    SecretPattern {
        name: "pem_private_key",
        prefix: "-----BEGIN ",
        charset: CharClass::Literal,
        min_len: 0,
    },
    SecretPattern {
        name: "slack_token",
        prefix: "xox",
        charset: CharClass::Token,
        min_len: 12,
    },
];

/// Whether a `-----BEGIN <…> PRIVATE KEY-----` block actually follows a
/// `pem_private_key` prefix hit — the literal pattern's second-stage check.
/// `find_all` for `Literal` returns the prefix span; callers that want the
/// *semantic* "private key block" check this tail. `redact` treats a bare
/// `-----BEGIN ` as a hit regardless — a PEM header alone is already a
/// credential-shaped string.
pub fn pem_block_is_private_key(text_after_prefix: &str) -> bool {
    text_after_prefix.contains("PRIVATE KEY-----")
}
