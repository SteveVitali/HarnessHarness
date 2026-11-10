//! `hh-tmpl/1` — the std-only template subset ADR-0442 (AR-23) fixes for
//! `Text` leaves (REQ-XE-112/113; §7.3 G2).
//!
//! The grammar is deliberately tiny: `{{ ident(.ident)* }}` substitution
//! and `{% if ident(.ident)* == "literal" %} … {% elif … %} … {% else %}
//! … {% endif %}` conditionals, with Jinja's whitespace-control `-`
//! markers on either delimiter, under Jinja's default
//! `trim_blocks = false`, `lstrip_blocks = false`,
//! `keep_trailing_newline = false`. No filters, loops, comments,
//! `+` markers, `is defined` tests, or expressions beyond `==` on a
//! string literal.
//!
//! Two honest failure legs, never a silent downgrade:
//! - anything outside the subset fails **compile** as
//!   [`TmplError::Unexpressible`] — the caller maps it to
//!   `CompileError::UnexpressibleSurface` (REQ-XE-112);
//! - a variable unfilled at **run time** is [`TmplError::Undefined`] —
//!   a typed refusal that stops before any model call, never an empty
//!   string (REQ-XE-113). Compile is pure: it reads no clock, env, or
//!   host value, so `compile_template` output is host-independent
//!   (the AC-2 purity leg).

use std::collections::{BTreeMap, BTreeSet};

/// The engine's typed error sum.
#[derive(Debug, Clone, PartialEq)]
pub enum TmplError {
    /// A construct outside the `hh-tmpl/1` subset — `offset` is the
    /// byte offset of the offending tag's `{`.
    Unexpressible {
        /// The byte offset of the offending tag.
        offset: usize,
        /// What the parser saw / which construct is outside the subset.
        detail: String,
    },
    /// Render-time strict undefined — the named variable was not filled.
    Undefined {
        /// The unfilled variable's dotted name.
        name: String,
    },
}

impl std::fmt::Display for TmplError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TmplError::Unexpressible { offset, detail } => {
                write!(f, "hh-tmpl/1 unexpressible at byte {offset}: {detail}")
            }
            TmplError::Undefined { name } => {
                write!(f, "hh-tmpl/1 undefined variable `{name}`")
            }
        }
    }
}

impl std::error::Error for TmplError {}

/// One parsed node.
#[derive(Debug, Clone, PartialEq)]
enum Node {
    /// Literal text (post whitespace-control).
    Text(String),
    /// `{{ ident(.ident)* }}` — the dotted variable path.
    Subst(String),
    /// `{% if v == "lit" %}a{% elif v2 == "lit2" %}b{% else %}c{% endif %}`.
    If {
        /// `(variable, literal, body)` arms in declared order.
        arms: Vec<(String, String, Vec<Node>)>,
        /// The `{% else %}` body (`[]` when undeclared).
        else_body: Vec<Node>,
    },
}

/// A compiled `hh-tmpl/1` template — the parsed node stream plus the
/// closed variable set every tag names (substitutions **and**
/// conditional subjects; the strict-undefined check's compile half).
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    nodes: Vec<Node>,
    vars: BTreeSet<String>,
}

impl Template {
    /// The template's variable set — substitution and conditional
    /// subjects, in sorted order (the `vars ⊆ section.slots` check's
    /// left side).
    pub fn variables(&self) -> &BTreeSet<String> {
        &self.vars
    }

    /// `render(vars)` — strict undefined: a `{{ }}` variable absent from
    /// `vars` is `Undefined`, never an empty string. Conditional
    /// subjects evaluate lazily in declared order (a later arm's subject
    /// is never touched once an earlier arm holds — Jinja's own
    /// short-circuit), so an unfilled variable refuses only when the
    /// render actually needs it.
    pub fn render(&self, vars: &BTreeMap<String, String>) -> Result<String, TmplError> {
        let mut out = String::new();
        render_nodes(&self.nodes, vars, &mut out)?;
        Ok(out)
    }
}

fn render_nodes(
    nodes: &[Node],
    vars: &BTreeMap<String, String>,
    out: &mut String,
) -> Result<(), TmplError> {
    for n in nodes {
        match n {
            Node::Text(t) => out.push_str(t),
            Node::Subst(name) => {
                let v = vars
                    .get(name)
                    .ok_or_else(|| TmplError::Undefined { name: name.clone() })?;
                out.push_str(v);
            }
            Node::If { arms, else_body } => {
                let mut hit = false;
                for (name, lit, body) in arms {
                    let v = vars
                        .get(name)
                        .ok_or_else(|| TmplError::Undefined { name: name.clone() })?;
                    if v == lit {
                        render_nodes(body, vars, out)?;
                        hit = true;
                        break;
                    }
                }
                if !hit {
                    render_nodes(else_body, vars, out)?;
                }
            }
        }
    }
    Ok(())
}

/// `ident` — `[A-Za-z_][A-Za-z0-9_]*` (the subset's identifier alphabet).
fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    match c.next() {
        Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => {}
        _ => return false,
    }
    c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// `ident(.ident)*` — a variable path (`task`, `uname.system`).
fn parse_var_path(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.iter().all(|p| is_ident(p)) && !parts.is_empty() {
        Some(s.to_string())
    } else {
        None
    }
}

/// The tag terminator that closed a `parse_seq` level.
enum Marker {
    /// `elif <var> == "<lit>"`.
    Elif(String, String),
    /// `else`.
    Else,
    /// `endif`.
    Endif,
    /// The template's end (unterminated only when the caller was inside
    /// a conditional).
    End,
}

struct Parser<'a> {
    src: &'a str,
    /// Byte cursor.
    pos: usize,
    /// The last tag closed with `-%}`/`-}}` — leading whitespace of the
    /// next text segment is stripped.
    eat_right: bool,
}

impl<'a> Parser<'a> {
    fn err(&self, offset: usize, detail: impl Into<String>) -> TmplError {
        TmplError::Unexpressible {
            offset,
            detail: detail.into(),
        }
    }

    /// Parse a node sequence until `elif`/`else`/`endif`/end. The caller
    /// interprets the marker (top level admits only `End`).
    fn parse_seq(&mut self, depth: usize) -> Result<(Vec<Node>, Marker), TmplError> {
        if depth > 32 {
            return Err(self.err(self.pos, "conditional nesting depth exceeds 32"));
        }
        let mut nodes: Vec<Node> = Vec::new();
        let mut text = String::new();
        let flush = |nodes: &mut Vec<Node>, text: &mut String| {
            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(text)));
            }
        };
        loop {
            let rest = &self.src[self.pos..];
            // Find the earliest `{{`/`{%`/`{#` opener.
            let mut open: Option<(usize, char)> = None;
            for (i, ch) in rest.char_indices() {
                if ch == '{' {
                    match rest[i + 1..].chars().next() {
                        Some('{') | Some('%') | Some('#') => {
                            open = Some((i, rest[i + 1..].chars().next().unwrap()));
                            break;
                        }
                        _ => {}
                    }
                }
            }
            let Some((rel, kind)) = open else {
                // No more tags — the rest is text (whitespace control
                // still applies on the final segment).
                let seg = if self.eat_right {
                    rest.trim_start()
                } else {
                    rest
                };
                text.push_str(seg);
                self.pos = self.src.len();
                break;
            };
            let tag_start = self.pos + rel;
            // Literal text before the tag (whitespace-control applied).
            let mut seg = &self.src[self.pos..tag_start];
            if self.eat_right {
                seg = seg.trim_start();
                self.eat_right = false;
            }
            text.push_str(seg);
            self.pos = tag_start;
            // The tag: `{x` or `{x-` … `-?}` or `-?x}`.
            let mut cur = self.pos + 2;
            let mut eat_left = false;
            if self.src[cur..].starts_with('-') {
                eat_left = true;
                cur += 1;
            } else if self.src[cur..].starts_with('+') {
                return Err(self.err(
                    tag_start,
                    "`+` whitespace markers are outside the hh-tmpl/1 subset",
                ));
            }
            let (close_a, close_b) = match kind {
                '{' => ("}}", "-}}"),
                '%' => ("%}", "-%}"),
                _ => ("#}", "-#}"),
            };
            let body_start = cur;
            // Find the close delimiter (`}}`/`%}`/`#}`, `-` included).
            let mut end = None;
            let tail = &self.src[body_start..];
            for (i, ch) in tail.char_indices() {
                if ch == '}' || ch == '%' || ch == '#' {
                    // The `-` close variant is checked first — at the
                    // `%`/`}`/`#` of `-%}`/`-}}`/`–#}` the `-` precedes
                    // this char, so `close_a` would match without it.
                    if i > 0 && tail[i - 1..].starts_with(close_b) {
                        end = Some((body_start + i - 1, true));
                        break;
                    }
                    if tail[i..].starts_with(close_a) {
                        end = Some((body_start + i, false));
                        break;
                    }
                }
            }
            let Some((body_end, eat_right_now)) = end else {
                return Err(self.err(tag_start, "unterminated tag"));
            };
            let body = self.src[body_start..body_end].trim();
            self.pos = body_end + close_a.len() + if eat_right_now { 1 } else { 0 };
            self.eat_right = eat_right_now;
            if eat_left {
                // `-` on the open side strips the accumulated text's
                // trailing whitespace.
                let trimmed = text.trim_end().to_string();
                text = trimmed;
            }
            match kind {
                '#' => {
                    return Err(self.err(
                        tag_start,
                        "comments `{# … #}` are outside the hh-tmpl/1 subset",
                    ));
                }
                '{' => {
                    let Some(var) = parse_var_path(body) else {
                        return Err(self.err(
                            tag_start,
                            format!("substitution `{body}` is not `ident(.ident)*`"),
                        ));
                    };
                    flush(&mut nodes, &mut text);
                    nodes.push(Node::Subst(var));
                }
                '%' => {
                    flush(&mut nodes, &mut text);
                    let word = body.split_whitespace().next().unwrap_or("");
                    match word {
                        "if" => {
                            let (var, lit) = self.parse_cond(tag_start, &body["if".len()..])?;
                            let mut arms = vec![];
                            let (body_nodes, mut mark) = self.parse_seq(depth + 1)?;
                            arms.push((var, lit, body_nodes));
                            let mut else_body = Vec::new();
                            loop {
                                match mark {
                                    Marker::Elif(v, l) => {
                                        let (b, m) = self.parse_seq(depth + 1)?;
                                        arms.push((v, l, b));
                                        mark = m;
                                    }
                                    Marker::Else => {
                                        let (b, m) = self.parse_seq(depth + 1)?;
                                        else_body = b;
                                        match m {
                                            Marker::Endif => break,
                                            _ => {
                                                return Err(self
                                                    .err(self.pos, "`elif`/`else` after `else`"));
                                            }
                                        }
                                    }
                                    Marker::Endif => break,
                                    Marker::End => {
                                        return Err(
                                            self.err(tag_start, "`if` without a closing `endif`")
                                        );
                                    }
                                }
                            }
                            nodes.push(Node::If { arms, else_body });
                        }
                        "elif" => {
                            let (v, l) = self.parse_cond(tag_start, &body["elif".len()..])?;
                            return Ok((nodes, Marker::Elif(v, l)));
                        }
                        "else" => {
                            if body != "else" {
                                return Err(self.err(tag_start, "`else` takes no arguments"));
                            }
                            return Ok((nodes, Marker::Else));
                        }
                        "endif" => {
                            if body != "endif" {
                                return Err(self.err(tag_start, "`endif` takes no arguments"));
                            }
                            return Ok((nodes, Marker::Endif));
                        }
                        other => {
                            return Err(self.err(
                                tag_start,
                                format!("block `{other}` is outside the hh-tmpl/1 subset"),
                            ));
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
        flush(&mut nodes, &mut text);
        Ok((nodes, Marker::End))
    }

    /// ` ident == "literal"` — the conditional head's closed shape.
    fn parse_cond(&self, tag_start: usize, rest: &str) -> Result<(String, String), TmplError> {
        let toks: Vec<&str> = rest.split_whitespace().collect();
        // `ident == "literal"` — three tokens, `==` operator, one
        // double-quoted literal (single quotes/`!=`/compound tests are
        // outside the subset).
        if toks.len() != 3 || toks[1] != "==" {
            return Err(self.err(
                tag_start,
                format!(
                    "conditional `{rest}` is not `ident == \"literal\"` (no `!=`, `is`, \
                     `and`/`or`, or bare tests in the subset)"
                ),
            ));
        }
        let Some(var) = parse_var_path(toks[0]) else {
            return Err(self.err(
                tag_start,
                format!("conditional subject `{}` is not `ident(.ident)*`", toks[0]),
            ));
        };
        let lit_tok = toks[2];
        if !(lit_tok.starts_with('"') && lit_tok.ends_with('"') && lit_tok.len() >= 2) {
            return Err(self.err(
                tag_start,
                format!("conditional literal `{lit_tok}` must be double-quoted"),
            ));
        }
        let inner = &lit_tok[1..lit_tok.len() - 1];
        // The literal admits `\"`, `\\`, `\n`, `\t`, `\r` escapes —
        // anything else stays literal.
        let mut lit = String::new();
        let mut chars = inner.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => lit.push('\n'),
                    Some('t') => lit.push('\t'),
                    Some('r') => lit.push('\r'),
                    Some('"') => lit.push('"'),
                    Some('\\') => lit.push('\\'),
                    Some(other) => {
                        lit.push('\\');
                        lit.push(other);
                    }
                    None => lit.push('\\'),
                }
            } else {
                lit.push(c);
            }
        }
        Ok((var, lit))
    }
}

/// `compile_template(source) → Template` — the `hh-tmpl/1` grammar over
/// the leaf's `content`. Jinja defaults apply: `keep_trailing_newline =
/// false` strips one trailing newline from the source before parsing.
/// Pure — no clock, env, or host reads.
pub fn compile_template(source: &str) -> Result<Template, TmplError> {
    // `keep_trailing_newline = false` — one trailing newline is removed.
    let src = source
        .strip_suffix("\r\n")
        .or_else(|| source.strip_suffix('\n'))
        .unwrap_or(source);
    let mut p = Parser {
        src,
        pos: 0,
        eat_right: false,
    };
    let (nodes, mark) = p.parse_seq(0)?;
    match mark {
        Marker::End => {}
        Marker::Endif | Marker::Else | Marker::Elif(_, _) => {
            return Err(TmplError::Unexpressible {
                offset: p.pos,
                detail: "stray `elif`/`else`/`endif` outside a conditional".to_string(),
            });
        }
    }
    let mut vars = BTreeSet::new();
    fn collect(nodes: &[Node], vars: &mut BTreeSet<String>) {
        for n in nodes {
            match n {
                Node::Subst(v) => {
                    vars.insert(v.clone());
                }
                Node::If { arms, else_body } => {
                    for (v, _, body) in arms {
                        vars.insert(v.clone());
                        collect(body, vars);
                    }
                    collect(else_body, vars);
                }
                _ => {}
            }
        }
    }
    collect(&nodes, &mut vars);
    Ok(Template { nodes, vars })
}
