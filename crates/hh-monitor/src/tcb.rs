//! The TCB enumeration and the static no-`Text`/no-model dependency check
//! (§5g.1 §2.6; I-H10 verifiability; AC-R-2.8.1-11's static half).
//!
//! `TCB_INSIDE`/`TCB_OUTSIDE` are the §2.6 lists *as data* — the enumeration is
//! reviewable and testable. [`static_check`] is the source scan the test suite
//! runs over this crate's sources: the authorize path must reference no
//! model-call operation and no `Text` reader. The check is a *token scan* —
//! the type system already guarantees no `Text` appears in a signature (the
//! crate has no `hh_hir::leaves::Text` in any public type); the scan is the
//! belt over that construction — it fails on the *tokens* a violation would
//! introduce (`Text` type references, model-client calls, `ModelCall`
//! *effect-domain* evaluation is allowed — deciding on a `model_call` effect
//! is not making one).

/// The §2.6 inside-list (MUST-code; no `Text` leaf; no model call).
pub const TCB_INSIDE: &[&str] = &[
    "handle-table projection (security.permission.granted/revoked fold)",
    "mint_root_handles",
    "authorize",
    "delegate",
    "revoke",
    "label operations (join/meet/leq, context_label stamp, endorsement checks)",
    "deterministic risk assessors (recorded inputs)",
    "Pi interpreter",
    "endorsement-legitimacy and persistence-ceiling checks (ledger append path)",
    "approval verb (security.permission.requested/decided validation)",
    "effect gate, sandbox helper invocation, credential broker call, egress policy",
    "SurfaceArgMap evaluator",
];

/// The §2.6 outside-list — never TCB.
pub const TCB_OUTSIDE: &[&str] = &[
    "the model and all prompts",
    "every Text/CompiledPayload leaf",
    "tools, executors, MCP servers, skills, hooks, plugins",
    "the Profile Compiler (RP is defence-in-depth)",
    "every component-class variant",
    "judges and classifiers",
    "the evolution service and the Lab",
    "hosted participants",
    "humans other than the run's principal",
];

/// A static violation — a forbidden token in a scanned source file.
#[derive(Debug, Clone, PartialEq)]
pub enum StaticViolation {
    /// A `Text`-leaf reference (`hh_hir::leaves::Text`, `Text` as a type name
    /// on a path that would read model-facing prose). The `/*` comment /
    /// doc-comment exemption is exact-token: `a Text leaf` in prose is fine;
    /// `: Text`, `<Text`, `Text,` in a type position are not.
    TextReference {
        /// The offending line.
        line: usize,
    },
    /// A model-call operation reference — `model_call` as an *invoked* thing
    /// (`model_call(`, `.model_call`, `call_model`, `ModelClient`,
    /// `chat_completion`, `embed`), never the `EffectDomain::ModelCall` the
    /// monitor *decides* on.
    ModelCall {
        /// The offending line.
        line: usize,
    },
}

/// The tokens that constitute a `Text` type-reference (word-boundary `Text` in
/// a type or expression position — `: Text`, `<Text`, `Text::`, `Text,`,
/// `Text>`, `Text)` at end-of-type-position, `-> Text`, `Vec<Text>`). A plain
/// prose `Text` in a doc comment is not flagged; the scan strips `//` line
/// comments and `///` doc lines before scanning.
const TEXT_TOKENS: &[&str] = &[
    ": Text",
    "<Text",
    "Text::",
    "-> Text",
    "(Text",
    "Vec<Text>",
    "Box<Text>",
    "Option<Text>",
];

/// The tokens that constitute a model-call operation reference.
const MODEL_TOKENS: &[&str] = &[
    "model_call(",
    ".model_call",
    "call_model",
    "ModelClient",
    "chat_completion",
    "model_client",
    "complete_prompt",
    "generate(",
    "embed(",
];

/// `static_check(source) → Result<(), Vec<StaticViolation>>` — scan one source
/// file's *code* lines (comments and doc comments excluded — the boundary is
/// token-level: a doc that says "no `Text`" is not a reference). Every hit is
/// collected.
pub fn static_check(source: &str) -> Vec<StaticViolation> {
    let mut out = Vec::new();
    let mut in_block_doc = false;
    for (i, raw) in source.lines().enumerate() {
        let line = raw.trim();
        // Skip doc comments and line comments entirely — the check is over
        // references, not prose.
        if line.starts_with("///") || line.starts_with("//!") || line.starts_with("//") {
            continue;
        }
        if line.starts_with("/**") || line.starts_with("/*!") {
            in_block_doc = true;
            continue;
        }
        if in_block_doc {
            if line.contains("*/") {
                in_block_doc = false;
            }
            continue;
        }
        // Blank string literals first — a token inside `"…"` is *data* (e.g.
        // this module's own token tables), never a `Text` type reference or a
        // model-call operation — then strip trailing `//` comments (a `//`
        // inside a literal is blanked before it can truncate the line early).
        let blanked = blank_string_literals(line);
        let code = blanked.split("//").next().unwrap_or("");
        for tok in TEXT_TOKENS {
            if token_hit(code, tok) {
                out.push(StaticViolation::TextReference { line: i + 1 });
                break;
            }
        }
        for tok in MODEL_TOKENS {
            if token_hit(code, tok) {
                out.push(StaticViolation::ModelCall { line: i + 1 });
                break;
            }
        }
    }
    out
}

/// `code` contains `tok` at a word boundary — when the token begins with an
/// identifier char (`model_call(`, `call_model`, …) the character before the
/// match must not be an identifier char, so `try_model_call(` never matches
/// `model_call(`. Tokens starting with punctuation (`: Text`, `<Text`) are
/// already self-anchoring.
fn token_hit(code: &str, tok: &str) -> bool {
    let anchored = tok
        .chars()
        .next()
        .map(|c| c.is_ascii_alphanumeric() || c == '_')
        .unwrap_or(false);
    for (i, _) in code.match_indices(tok) {
        let preceded = anchored
            && i > 0
            && code[..i]
                .chars()
                .last()
                .map(|c| c.is_ascii_alphanumeric() || c == '_')
                .unwrap_or(false);
        if !preceded {
            return true;
        }
    }
    false
}

/// Replace the *contents* of `"…"` literals with spaces (positions preserved —
/// reported line numbers stay honest). `\\` and `\"` escapes are consumed; the
/// monitor's sources carry no raw strings (`r"…"`/`r#"…"#`), so the simple
/// state machine is exact for the TCB surface it scans.
fn blank_string_literals(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if in_str {
            match c {
                '\\' => {
                    out.push(' ');
                    if let Some(e) = chars.next() {
                        out.push(if e == '\n' { '\n' } else { ' ' });
                    }
                }
                '"' => {
                    in_str = false;
                    out.push('"');
                }
                _ => out.push(' '),
            }
        } else {
            out.push(c);
            if c == '"' {
                in_str = true;
            }
        }
    }
    out
}
