//! A bounded, std-only regex engine — the `lexical_regex` `catalog_index`
//! back end (§5d.3; ADR-0094 D4; the S2.10 slice). The workspace is hermetic
//! (pure standard library — see the workspace manifest), so the engine lives
//! here rather than behind a crate dependency.
//!
//! Supported syntax (a deliberate, documented subset):
//!
//! - literals and `.` (any char);
//! - character classes `[...]` with ranges (`a-z`), negation (`[^...]`) and
//!   the class escapes `\w \W \d \D \s \S`;
//! - the escapes `\w \W \d \D \s \S \n \t \r` plus any escaped metachar
//!   (`\.` `\*` `\+` `\?` `\(` `\)` `\[` `\]` `\{` `\}` `\|` `\^` `\$` `\\`);
//! - repetition `*` `+` `?` and bounded `{m}` `{m,}` `{m,n}` (`m, n ≤ 64`);
//! - grouping `(...)` and alternation `|`;
//! - whole-text anchors `^` (start) and `$` (end).
//!
//! Compilation is a Thompson NFA; matching is an ε-closure simulation —
//! `O(states × text)` with no backtracking, so pathological patterns cannot
//! blow up. Two resource bounds are enforced: pattern length ≤ 2048 chars
//! and compiled states ≤ 4096 (a violation is [`RegexError`], never a
//! partial match). Matching is *search* semantics — an unanchored pattern
//! may match anywhere in the text; `^`/`$` pin it.
//!
//! Unsupported constructs are refused, never silently reinterpreted:
//! back-references, look-around, lazy modifiers (`*?`), named classes
//! (`[:alpha:]`), inline flags and Unicode property escapes are all
//! [`RegexError::Syntax`].

/// The closed error sum for `compile`.
#[derive(Debug, Clone, PartialEq)]
pub enum RegexError {
    /// The pattern exceeded a resource bound.
    TooComplex {
        /// `pattern_len` | `states`.
        what: &'static str,
    },
    /// A syntax error at `pos` (byte offset).
    Syntax {
        /// The byte offset of the offending construct.
        pos: usize,
        /// The closed reason tag.
        reason: &'static str,
    },
}

impl std::fmt::Display for RegexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegexError::TooComplex { what } => write!(f, "regex too complex: {what}"),
            RegexError::Syntax { pos, reason } => write!(f, "regex syntax at {pos}: {reason}"),
        }
    }
}

impl std::error::Error for RegexError {}

/// The pattern-length bound.
pub const MAX_PATTERN_CHARS: usize = 2048;
/// The compiled-state bound.
pub const MAX_STATES: usize = 4096;
/// The `{m,n}` repetition bound.
const MAX_REPEAT: u32 = 64;

// ── AST ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Ast {
    /// Alternation of branches.
    Alt(Vec<Ast>),
    /// Concatenation.
    Cat(Vec<Ast>),
    /// `atom` repeated `min..=max` (`max = None` is unbounded).
    Rep {
        atom: Box<Ast>,
        min: u32,
        max: Option<u32>,
    },
    /// A single character.
    Char(char),
    /// `.` — any character.
    Any,
    /// A character class.
    Class(Class),
    /// `^`.
    Start,
    /// `$`.
    End,
    /// The empty concat.
    Empty,
}

/// A character class — `negated` plus a sorted range list.
#[derive(Debug, Clone)]
struct Class {
    negated: bool,
    ranges: Vec<(char, char)>,
}

impl Class {
    fn contains(&self, c: char) -> bool {
        let hit = self.ranges.iter().any(|(lo, hi)| *lo <= c && c <= *hi);
        hit != self.negated
    }
}

fn word_ranges() -> Vec<(char, char)> {
    vec![('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')]
}

fn digit_ranges() -> Vec<(char, char)> {
    vec![('0', '9')]
}

fn space_ranges() -> Vec<(char, char)> {
    vec![
        (' ', ' '),
        ('\t', '\t'),
        ('\n', '\n'),
        ('\r', '\r'),
        (0x0b as char, 0x0c as char),
    ]
}

/// The escape classes usable both bare and inside `[...]`.
fn escape_class(e: char) -> Option<Class> {
    match e {
        'w' => Some(Class {
            negated: false,
            ranges: word_ranges(),
        }),
        'W' => Some(Class {
            negated: true,
            ranges: word_ranges(),
        }),
        'd' => Some(Class {
            negated: false,
            ranges: digit_ranges(),
        }),
        'D' => Some(Class {
            negated: true,
            ranges: digit_ranges(),
        }),
        's' => Some(Class {
            negated: false,
            ranges: space_ranges(),
        }),
        'S' => Some(Class {
            negated: true,
            ranges: space_ranges(),
        }),
        _ => None,
    }
}

// ── Parser (recursive descent over chars) ────────────────────────────────────

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(pat: &str) -> Parser {
        Parser {
            chars: pat.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn err<T>(&self, reason: &'static str) -> Result<T, RegexError> {
        Err(RegexError::Syntax {
            pos: self.pos,
            reason,
        })
    }

    /// `alt := concat ('|' concat)*`
    fn parse_alt(&mut self) -> Result<Ast, RegexError> {
        let mut branches = vec![self.parse_concat()?];
        while self.peek() == Some('|') {
            self.pos += 1;
            branches.push(self.parse_concat()?);
        }
        if branches.len() == 1 {
            Ok(branches.pop().unwrap())
        } else {
            Ok(Ast::Alt(branches))
        }
    }

    /// `concat := repeat*` — stops at `)` or `|` or end.
    fn parse_concat(&mut self) -> Result<Ast, RegexError> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == ')' || c == '|' {
                break;
            }
            items.push(self.parse_repeat()?);
        }
        match items.len() {
            0 => Ok(Ast::Empty),
            1 => Ok(items.pop().unwrap()),
            _ => Ok(Ast::Cat(items)),
        }
    }

    /// `repeat := atom quantifier*` — stacked quantifiers are refused.
    fn parse_repeat(&mut self) -> Result<Ast, RegexError> {
        let mut atom = self.parse_atom()?;
        let mut quantified = false;
        loop {
            let (min, max) = match self.peek() {
                Some('*') => {
                    self.pos += 1;
                    (0, None)
                }
                Some('+') => {
                    self.pos += 1;
                    (1, None)
                }
                Some('?') => {
                    self.pos += 1;
                    (0, Some(1))
                }
                Some('{') => {
                    // A `{` that does not parse as a quantifier is a literal —
                    // standard-compatible behaviour (e.g. `a{` matches "a{").
                    match self.try_parse_brace()? {
                        Some(mm) => mm,
                        None => break,
                    }
                }
                _ => break,
            };
            if quantified {
                return self.err("stacked_quantifier");
            }
            // A lazy modifier (`*?`, `+?`, `??`, `{m,n}?`) is unsupported —
            // refused, never coerced to a greedy quantifier.
            if self.peek() == Some('?') {
                return self.err("lazy_quantifier");
            }
            quantified = true;
            atom = Ast::Rep {
                atom: Box::new(atom),
                min,
                max,
            };
        }
        Ok(atom)
    }

    /// Try `{m}` `{m,}` `{m,n}` at `pos`; `Ok(None)` means `{` is a literal.
    fn try_parse_brace(&mut self) -> Result<Option<(u32, Option<u32>)>, RegexError> {
        debug_assert_eq!(self.peek(), Some('{'));
        let save = self.pos;
        self.pos += 1;
        let mut m = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                m.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        if m.is_empty() {
            self.pos = save;
            return Ok(None);
        }
        let lo: u32 = m.parse().map_err(|_| RegexError::Syntax {
            pos: save,
            reason: "bad_repeat",
        })?;
        let hi: Option<u32>;
        match self.peek() {
            Some('}') => {
                self.pos += 1;
                hi = Some(lo);
            }
            Some(',') => {
                self.pos += 1;
                let mut n = String::new();
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        n.push(c);
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                if self.peek() != Some('}') {
                    self.pos = save;
                    return Ok(None);
                }
                self.pos += 1;
                hi = if n.is_empty() {
                    None
                } else {
                    Some(n.parse().map_err(|_| RegexError::Syntax {
                        pos: save,
                        reason: "bad_repeat",
                    })?)
                };
            }
            _ => {
                self.pos = save;
                return Ok(None);
            }
        }
        if lo > MAX_REPEAT || hi.is_some_and(|h| h > MAX_REPEAT) {
            return Err(RegexError::Syntax {
                pos: save,
                reason: "repeat_too_large",
            });
        }
        if hi.is_some_and(|h| h < lo) {
            return Err(RegexError::Syntax {
                pos: save,
                reason: "repeat_inverted",
            });
        }
        Ok(Some((lo, hi)))
    }

    /// `atom := '(' alt ')' | '[' class ']' | '.' | '^' | '$' | escape | lit`
    fn parse_atom(&mut self) -> Result<Ast, RegexError> {
        match self.next() {
            None => self.err("empty_atom"),
            Some('(') => {
                // `(?:` is refused — no flag/group extensions.
                if self.peek() == Some('?') {
                    return self.err("group_extension");
                }
                let inner = self.parse_alt()?;
                if self.next() != Some(')') {
                    return self.err("unclosed_group");
                }
                Ok(inner)
            }
            Some('[') => self.parse_class(),
            Some(')') | Some('|') => self.err("stray_delimiter"),
            Some('.') => Ok(Ast::Any),
            Some('^') => Ok(Ast::Start),
            Some('$') => Ok(Ast::End),
            Some('\\') => self.parse_escape(),
            Some(c) => Ok(Ast::Char(c)),
        }
    }

    /// A `\`-escape outside a class.
    fn parse_escape(&mut self) -> Result<Ast, RegexError> {
        match self.next() {
            None => self.err("dangling_escape"),
            Some('n') => Ok(Ast::Char('\n')),
            Some('t') => Ok(Ast::Char('\t')),
            Some('r') => Ok(Ast::Char('\r')),
            Some(c) if escape_class(c).is_some() => Ok(Ast::Class(escape_class(c).unwrap())),
            // Escaped metachars and punctuation are literals.
            Some(c) if !c.is_alphanumeric() => Ok(Ast::Char(c)),
            // `\b`, `\A`, back-references, `\p{...}` — unsupported.
            Some(_) => self.err("unsupported_escape"),
        }
    }

    /// `[...]` — ranges, `^` negation, escapes.
    fn parse_class(&mut self) -> Result<Ast, RegexError> {
        let negated = if self.peek() == Some('^') {
            self.pos += 1;
            true
        } else {
            false
        };
        let mut ranges: Vec<(char, char)> = Vec::new();
        let mut first = true;
        loop {
            let Some(c) = self.next() else {
                return self.err("unclosed_class");
            };
            if c == ']' && !first {
                break;
            }
            first = false;
            // A `]` as the first member is a literal (POSIX-compatible).
            let lo = if c == '\\' {
                match self.next() {
                    None => return self.err("dangling_escape"),
                    Some('n') => ClassItem::Char('\n'),
                    Some('t') => ClassItem::Char('\t'),
                    Some('r') => ClassItem::Char('\r'),
                    Some(e) if escape_class(e).is_some() => {
                        // A class escape contributes its ranges directly.
                        let cls = escape_class(e).unwrap();
                        if cls.negated {
                            // Negated class inside a class — unsupported
                            // (the composition is ambiguous); refuse.
                            return self.err("nested_negated_class");
                        }
                        ranges.extend(cls.ranges);
                        continue;
                    }
                    Some(e) if !e.is_alphanumeric() => ClassItem::Char(e),
                    Some(_) => return self.err("unsupported_escape"),
                }
            } else {
                ClassItem::Char(c)
            };
            match lo {
                ClassItem::Char(lo) => {
                    // `a-z` range?
                    if self.peek() == Some('-')
                        && self.chars.get(self.pos + 1).copied() != Some(']')
                        && self.chars.get(self.pos + 1).is_some()
                    {
                        self.pos += 1; // consume '-'
                        let hc = self.next().unwrap();
                        let hi = if hc == '\\' {
                            match self.next() {
                                Some('n') => '\n',
                                Some('t') => '\t',
                                Some('r') => '\r',
                                Some(e) if !e.is_alphanumeric() => e,
                                _ => return self.err("bad_range"),
                            }
                        } else {
                            hc
                        };
                        if hi < lo {
                            return self.err("range_inverted");
                        }
                        ranges.push((lo, hi));
                    } else {
                        ranges.push((lo, lo));
                    }
                }
            }
        }
        if ranges.is_empty() && !negated {
            return self.err("empty_class");
        }
        ranges.sort();
        Ok(Ast::Class(Class { negated, ranges }))
    }
}

enum ClassItem {
    Char(char),
}

// ── NFA compile (Thompson construction) ──────────────────────────────────────

#[derive(Debug)]
enum Edge {
    /// ε-transition.
    Eps(usize),
    /// Consume one char in the class.
    Class(Class, usize),
    /// Assert position 0.
    Start(usize),
    /// Assert end-of-text.
    End(usize),
}

struct Compiler {
    states: Vec<Vec<Edge>>,
}

impl Compiler {
    fn new() -> Compiler {
        Compiler { states: Vec::new() }
    }

    fn add_state(&mut self) -> Result<usize, RegexError> {
        if self.states.len() >= MAX_STATES {
            return Err(RegexError::TooComplex { what: "states" });
        }
        self.states.push(Vec::new());
        Ok(self.states.len() - 1)
    }

    /// Compile `ast`; returns `(start, accept)` state ids.
    fn compile(&mut self, ast: &Ast) -> Result<(usize, usize), RegexError> {
        match ast {
            Ast::Empty => {
                let s = self.add_state()?;
                Ok((s, s))
            }
            Ast::Char(c) => {
                let s = self.add_state()?;
                let t = self.add_state()?;
                self.states[s].push(Edge::Class(
                    Class {
                        negated: false,
                        ranges: vec![(*c, *c)],
                    },
                    t,
                ));
                Ok((s, t))
            }
            Ast::Any => {
                let s = self.add_state()?;
                let t = self.add_state()?;
                self.states[s].push(Edge::Class(
                    Class {
                        negated: true,
                        ranges: vec![],
                    },
                    t,
                ));
                Ok((s, t))
            }
            Ast::Class(cls) => {
                let s = self.add_state()?;
                let t = self.add_state()?;
                self.states[s].push(Edge::Class(cls.clone(), t));
                Ok((s, t))
            }
            Ast::Start => {
                let s = self.add_state()?;
                let t = self.add_state()?;
                self.states[s].push(Edge::Start(t));
                Ok((s, t))
            }
            Ast::End => {
                let s = self.add_state()?;
                let t = self.add_state()?;
                self.states[s].push(Edge::End(t));
                Ok((s, t))
            }
            Ast::Cat(items) => {
                let mut start: Option<usize> = None;
                let mut prev_accept: Option<usize> = None;
                for item in items {
                    let (s, t) = self.compile(item)?;
                    if let Some(p) = prev_accept {
                        self.states[p].push(Edge::Eps(s));
                    } else {
                        start = Some(s);
                    }
                    prev_accept = Some(t);
                }
                match (start, prev_accept) {
                    (Some(s), Some(t)) => Ok((s, t)),
                    _ => {
                        let s = self.add_state()?;
                        Ok((s, s))
                    }
                }
            }
            Ast::Alt(branches) => {
                let s = self.add_state()?;
                let t = self.add_state()?;
                for b in branches {
                    let (bs, bt) = self.compile(b)?;
                    self.states[s].push(Edge::Eps(bs));
                    self.states[bt].push(Edge::Eps(t));
                }
                Ok((s, t))
            }
            Ast::Rep { atom, min, max } => self.compile_rep(atom, *min, *max),
        }
    }

    /// `atom{min,max}` — expanded Thompson fragments (bounded by the state
    /// cap; `max = None` appends one `atom*` star).
    fn compile_rep(
        &mut self,
        atom: &Ast,
        min: u32,
        max: Option<u32>,
    ) -> Result<(usize, usize), RegexError> {
        let s = self.add_state()?;
        let mut tail = s;
        for _ in 0..min {
            let (as_, at) = self.compile(atom)?;
            self.states[tail].push(Edge::Eps(as_));
            tail = at;
        }
        match max {
            None => {
                // `atom*` tail: loop back over the atom.
                let loop_in = self.add_state()?;
                let accept = self.add_state()?;
                self.states[tail].push(Edge::Eps(loop_in));
                self.states[tail].push(Edge::Eps(accept));
                let (as_, at) = self.compile(atom)?;
                self.states[loop_in].push(Edge::Eps(as_));
                self.states[loop_in].push(Edge::Eps(accept));
                self.states[at].push(Edge::Eps(loop_in));
                Ok((s, accept))
            }
            Some(max) => {
                // `min` required copies are already in place; `max - min`
                // optional copies each bypassable.
                let accept = self.add_state()?;
                self.states[tail].push(Edge::Eps(accept));
                for _ in min..max {
                    let (as_, at) = self.compile(atom)?;
                    self.states[tail].push(Edge::Eps(as_));
                    self.states[at].push(Edge::Eps(accept));
                    tail = at;
                }
                Ok((s, accept))
            }
        }
    }
}

/// A compiled, bounded regex — the `lexical_regex` index's query executable.
#[derive(Debug)]
pub struct Regex {
    states: Vec<Vec<Edge>>,
    start: usize,
    accept: usize,
}

impl Regex {
    /// `compile(pattern)` — parse + Thompson-compile under the resource
    /// bounds (`MAX_PATTERN_CHARS`, `MAX_STATES`).
    pub fn compile(pattern: &str) -> Result<Regex, RegexError> {
        if pattern.chars().count() > MAX_PATTERN_CHARS {
            return Err(RegexError::TooComplex {
                what: "pattern_len",
            });
        }
        let mut p = Parser::new(pattern);
        let ast = p.parse_alt()?;
        if p.peek().is_some() {
            // A `)` or `|` left over — the parser only stops on those.
            return Err(RegexError::Syntax {
                pos: p.pos,
                reason: "unclosed_group",
            });
        }
        let mut c = Compiler::new();
        let (start, accept) = c.compile(&ast)?;
        Ok(Regex {
            states: c.states,
            start,
            accept,
        })
    }

    /// `is_match(text)` — ε-closure NFA simulation, search semantics: at
    /// every position the live set includes the start state's closure, so an
    /// unanchored pattern matches anywhere; `^`/`$` assertions evaluate
    /// against position `0`/`len`.
    pub fn is_match(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        let n = chars.len();
        let mut current: Vec<bool> = vec![false; self.states.len()];
        self.closure_into(&mut current, self.start, 0, n);
        for (pos, ch) in chars.iter().enumerate() {
            if current[self.accept] {
                return true;
            }
            let mut stepped: Vec<bool> = vec![false; self.states.len()];
            for (s, on) in current.iter().enumerate() {
                if !on {
                    continue;
                }
                for e in &self.states[s] {
                    if let Edge::Class(cls, t) = e {
                        if cls.contains(*ch) {
                            stepped[*t] = true;
                        }
                    }
                }
            }
            let mut next: Vec<bool> = vec![false; self.states.len()];
            for (s, on) in stepped.iter().enumerate() {
                if *on {
                    self.closure_into(&mut next, s, pos + 1, n);
                }
            }
            // Unanchored search: re-seed the start state at this position.
            self.closure_into(&mut next, self.start, pos + 1, n);
            current = next;
        }
        current[self.accept]
    }

    /// ε-closure of `state` into `set`, evaluating `^`/`$` assertions at
    /// `pos` against a text of length `n` (chars). Iterative — no recursion.
    fn closure_into(&self, set: &mut [bool], state: usize, pos: usize, n: usize) {
        let mut stack = vec![state];
        while let Some(s) = stack.pop() {
            if set[s] {
                continue;
            }
            set[s] = true;
            for e in &self.states[s] {
                match e {
                    Edge::Eps(t) => {
                        if !set[*t] {
                            stack.push(*t);
                        }
                    }
                    Edge::Start(t) => {
                        if pos == 0 && !set[*t] {
                            stack.push(*t);
                        }
                    }
                    Edge::End(t) => {
                        if pos == n && !set[*t] {
                            stack.push(*t);
                        }
                    }
                    Edge::Class(..) => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_and_search() {
        let r = Regex::compile("read").unwrap();
        assert!(r.is_match("read_file"));
        assert!(r.is_match("a read b"));
        assert!(!r.is_match("write"));
    }

    #[test]
    fn anchors() {
        let r = Regex::compile("^read").unwrap();
        assert!(r.is_match("read_file"));
        assert!(!r.is_match("a read"));
        let r = Regex::compile("file$").unwrap();
        assert!(r.is_match("read_file"));
        assert!(!r.is_match("file_read"));
        let r = Regex::compile("^a.c$").unwrap();
        assert!(r.is_match("abc"));
        assert!(!r.is_match("abcd"));
    }

    #[test]
    fn classes_and_quantifiers() {
        let r = Regex::compile("^[a-z]+_[0-9]+$").unwrap();
        assert!(r.is_match("read_12"));
        assert!(!r.is_match("read_file"));
        let r = Regex::compile("\\d{2,4}").unwrap();
        assert!(r.is_match("x123y"));
        assert!(!r.is_match("x1y"));
        let r = Regex::compile("colou?r").unwrap();
        assert!(r.is_match("color"));
        assert!(r.is_match("colour"));
        let r = Regex::compile("ab*c").unwrap();
        assert!(r.is_match("ac"));
        assert!(r.is_match("abbbbc"));
    }

    #[test]
    fn alternation_and_groups() {
        let r = Regex::compile("^(cat|dog)s?$").unwrap();
        assert!(r.is_match("cats"));
        assert!(r.is_match("dog"));
        assert!(!r.is_match("bird"));
    }

    #[test]
    fn refuses_unsupported() {
        assert!(matches!(
            Regex::compile("a(b|c"),
            Err(RegexError::Syntax { .. })
        ));
        assert!(matches!(
            Regex::compile("(a)\\1"),
            Err(RegexError::Syntax { .. })
        ));
        assert!(matches!(
            Regex::compile("a*?"),
            Err(RegexError::Syntax {
                reason: "lazy_quantifier",
                ..
            })
        ));
        assert!(matches!(
            Regex::compile("a{200}"),
            Err(RegexError::Syntax {
                reason: "repeat_too_large",
                ..
            })
        ));
        assert!(matches!(
            Regex::compile("[z-a]"),
            Err(RegexError::Syntax {
                reason: "range_inverted",
                ..
            })
        ));
    }

    #[test]
    fn pathological_is_bounded() {
        // The classic exponential backtracker — an NFA handles it linearly.
        let r = Regex::compile("^(a+)+$").unwrap();
        let text = "a".repeat(64) + "b";
        assert!(!r.is_match(text.as_str()));
    }

    #[test]
    fn word_escapes() {
        let r = Regex::compile("^\\w+$").unwrap();
        assert!(r.is_match("read_file_9"));
        assert!(!r.is_match("read file"));
        let r = Regex::compile("\\S+\\.rs$").unwrap();
        assert!(r.is_match("open main.rs"));
    }
}
