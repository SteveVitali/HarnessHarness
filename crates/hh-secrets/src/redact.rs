//! `redact` — the kernel-derived, source-side masker (§5g.3 §2; ADR-0057 D5)
//! and `leak_scan` — the **test-battery** verifier (ADR-0059 D2: a static/
//! contract test mechanism, never a runtime detector on a live path).
//!
//! Detectors, in order: `placeholder_passthrough` (a `mh_secret:`/`${SECRET:…}`
//! spelling already in the text is preserved and *marked* — it is evidence the
//! boundary held, not a leak), `known_value` (every mask-set value — every live
//! channel, every retained rotated revision), `pattern` (the registered
//! provider-token set), `canary` (registered canary values — the
//! encoding-independent tripwire; Stage-3 suites extend it to encoded forms).
//!
//! Redaction replaces a hit with a [`RedactionTombstone`] —
//! `REDACTED[<detector>:<fingerprint|label>]` — preserving the label so the
//! audit row (`security.secret.redacted{sink, detector, fingerprint?}`) can name
//! *what class* was masked without the bytes.

use hh_wire::json::Json;

use crate::mask::MaskSet;
use crate::patterns::{SecretPattern, REGISTERED_PATTERNS};
use crate::types::Placeholder;

/// The detector kind that produced a hit (§5g.3 §2's detector set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DetectorKind {
    /// An existing placeholder spelling passed through unmodified.
    PlaceholderPassthrough,
    /// A mask-set value (live or rotated revision).
    KnownValue,
    /// A registered provider-token pattern.
    Pattern,
    /// A registered canary value.
    Canary,
    /// An encoded form (base64/hex/percent) of a mask-set value — the
    /// C1/Stage-3 LT-12 detector (§5g.3 §2 `known_value_encoded`; §8's
    /// "encoded variants defeat known-value masking" row).
    KnownValueEncoded,
}

impl DetectorKind {
    /// The canonical tag.
    pub fn as_str(self) -> &'static str {
        match self {
            DetectorKind::PlaceholderPassthrough => "placeholder_passthrough",
            DetectorKind::KnownValue => "known_value",
            DetectorKind::Pattern => "pattern",
            DetectorKind::Canary => "canary",
            DetectorKind::KnownValueEncoded => "known_value_encoded",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<DetectorKind> {
        Some(match s {
            "placeholder_passthrough" => DetectorKind::PlaceholderPassthrough,
            "known_value" => DetectorKind::KnownValue,
            "pattern" => DetectorKind::Pattern,
            "canary" => DetectorKind::Canary,
            "known_value_encoded" => DetectorKind::KnownValueEncoded,
            _ => return None,
        })
    }
}

/// A canary — a planted, encoding-independent tripwire value (ADR-0059;
/// §5g.3 §1's canary row). The `value` is kernel-side (registered at test time
/// or by the kernel); a hit means the value crossed a boundary it never should
/// have reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canary {
    /// The canary's id (audit label).
    pub id: String,
    /// The planted value (kernel-side only).
    pub value: String,
}

/// The detector set `redact`/`leak_scan` run — the mask set plus the registered
/// patterns and canaries. Kernel-derived (built by the broker), never caller-
/// constructed ad hoc at a boundary.
#[derive(Debug, Clone, Default)]
pub struct DetectorSet {
    /// The known-value mask set (SV-1: every live channel + retained rotations).
    pub mask_set: MaskSet,
    /// The registered patterns (defaults to [`REGISTERED_PATTERNS`]).
    pub patterns: Vec<SecretPattern>,
    /// The registered canaries.
    pub canaries: Vec<Canary>,
}

impl DetectorSet {
    /// The standard Stage-1 detector set: the mask set + the registered
    /// patterns + no canaries (callers add canaries explicitly).
    pub fn standard(mask_set: MaskSet) -> DetectorSet {
        DetectorSet {
            mask_set,
            patterns: REGISTERED_PATTERNS.to_vec(),
            canaries: Vec::new(),
        }
    }
}

/// `RedactionTombstone{detector, fingerprint?}` — the replacement label a redacted
/// span leaves (§5g.3 §2): the hit is *named*, never echoed. Renders
/// `REDACTED[<detector>:<fingerprint|pattern-name|canary-id>]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionTombstone {
    /// The detector that fired.
    pub detector: DetectorKind,
    /// The `sf:` fingerprint when the hit was a known value (correlation
    /// coordinate — not a reconstruction path).
    pub fingerprint: Option<String>,
    /// The label — pattern name or canary id when no fingerprint applies.
    pub label: String,
}

impl RedactionTombstone {
    /// The rendered replacement text.
    pub fn render(&self) -> String {
        match &self.fingerprint {
            Some(fp) => format!("REDACTED[{}:{}]", self.detector.as_str(), fp),
            None => format!("REDACTED[{}:{}]", self.detector.as_str(), self.label),
        }
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("detector", Json::str(self.detector.as_str())),
            (
                "fingerprint",
                self.fingerprint
                    .as_ref()
                    .map(|f| Json::str(f.clone()))
                    .unwrap_or(Json::Null),
            ),
            ("label", Json::str(self.label.clone())),
        ])
    }
}

/// A redaction `Hit` — where a detector fired and what replaced it (the audit
/// row's content-free summary; `span` indexes into the *pre-redaction* text and
/// is a coordinate, not the bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The byte span in the original text.
    pub span: (usize, usize),
    /// The detector.
    pub detector: DetectorKind,
    /// The tombstone recorded in `security.secret.redacted` — `None` for a
    /// `placeholder_passthrough` mark (the placeholder stays verbatim).
    pub tombstone: Option<RedactionTombstone>,
    /// The replacement text applied to the span — the channel's *placeholder
    /// spelling* when the mask-set entry is binding-scoped (§5g.3 §2: "each
    /// mask maps to the channel's placeholder"), else the tombstone render.
    /// `None` = the span is left untouched (passthrough).
    pub replacement: Option<String>,
}

/// `redact(text)` — replace every hit's span with its replacement (the
/// channel's placeholder for a binding-scoped mask; the tombstone otherwise)
/// and return `(redacted_text, hits)`. Deterministic: longest-span-first,
/// left-to-right; overlapping hits merge to the leftmost-longest.
pub fn redact(text: &str, detectors: &DetectorSet) -> (String, Vec<Hit>) {
    let hits = detect(text, detectors);
    // Apply replacements right-to-left so spans stay valid.
    let mut out = text.to_string();
    for h in hits.iter().rev() {
        if let Some(r) = &h.replacement {
            out.replace_range(h.span.0..h.span.1, r);
        }
    }
    (out, hits)
}

/// The pure detector sweep — every hit without applying replacements.
/// `detect` is the shared core of `redact`, `leak_scan` and the definition scan
/// (`defscan`) — one sweep, three verdicts.
pub fn detect(text: &str, detectors: &DetectorSet) -> Vec<Hit> {
    let mut hits: Vec<Hit> = Vec::new();

    // placeholder_passthrough — a placeholder spelling is marked, never
    // rewritten (it is the *safe* form; rewriting it would corrupt the binding
    // coordinate it names).
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(rel) = text[i..].find(Placeholder::PREFIX) {
            let start = i + rel;
            if start == 0 || !is_ident(bytes[start - 1]) {
                let mut end = start + Placeholder::PREFIX.len();
                while end < bytes.len() && !bytes[end].is_ascii_whitespace() && bytes[end] != b'"' {
                    end += 1;
                }
                hits.push(Hit {
                    span: (start, end),
                    detector: DetectorKind::PlaceholderPassthrough,
                    tombstone: None,
                    replacement: None,
                });
                i = end;
                continue;
            }
            i = start + Placeholder::PREFIX.len();
        } else {
            break;
        }
    }
    // The Stage-0 placeholder spelling too (`${SECRET:<name>}`).
    let mut j = 0;
    while let Some(rel) = text[j..].find("${SECRET:") {
        let start = j + rel;
        let end = text[start..]
            .find('}')
            .map(|e| start + e + 1)
            .unwrap_or(text.len());
        hits.push(Hit {
            span: (start, end),
            detector: DetectorKind::PlaceholderPassthrough,
            tombstone: None,
            replacement: None,
        });
        j = end;
    }

    // known_value — every mask-set value (longest first; overlaps are removed
    // below).
    for entry in detectors.mask_set.all_entries() {
        if entry.value.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(rel) = text[from..].find(entry.value.as_str()) {
            let start = from + rel;
            hits.push(Hit {
                span: (start, start + entry.value.len()),
                detector: DetectorKind::KnownValue,
                tombstone: Some(RedactionTombstone {
                    detector: DetectorKind::KnownValue,
                    fingerprint: Some(entry.fingerprint.render()),
                    label: format!("${{SECRET:{}}}", entry.channel_id),
                }),
                // §5g.3 §2: "each mask maps to the channel's placeholder or a
                // RedactionTombstone" — a binding-scoped mask leaves the
                // placeholder spelling itself in the text.
                replacement: Some(match &entry.placeholder {
                    Some(p) => p.spelling.clone(),
                    None => RedactionTombstone {
                        detector: DetectorKind::KnownValue,
                        fingerprint: Some(entry.fingerprint.render()),
                        label: format!("${{SECRET:{}}}", entry.channel_id),
                    }
                    .render(),
                }),
            });
            from = start + entry.value.len();
        }
    }

    // pattern — the registered provider-token set.
    for pat in &detectors.patterns {
        for (s, e) in pat.find_all(text) {
            hits.push(Hit {
                span: (s, e),
                detector: DetectorKind::Pattern,
                tombstone: Some(RedactionTombstone {
                    detector: DetectorKind::Pattern,
                    fingerprint: None,
                    label: pat.name.to_string(),
                }),
                replacement: Some(
                    RedactionTombstone {
                        detector: DetectorKind::Pattern,
                        fingerprint: None,
                        label: pat.name.to_string(),
                    }
                    .render(),
                ),
            });
        }
    }

    // known_value_encoded — the canonical encodings of each mask-set value
    // (§5g.3 §8 LT-12: "encoded forms of a known value in tool output are
    // masked"). The same placeholder/tombstone replacement as `known_value` —
    // a leak is a leak regardless of spelling.
    for entry in detectors.mask_set.all_entries() {
        if entry.value.len() < crate::encoded::ENCODED_MIN_LEN {
            continue;
        }
        for form in crate::encoded::encoded_forms(&entry.value) {
            let mut from = 0;
            while let Some(rel) = text[from..].find(form.as_str()) {
                let start = from + rel;
                hits.push(Hit {
                    span: (start, start + form.len()),
                    detector: DetectorKind::KnownValueEncoded,
                    tombstone: Some(RedactionTombstone {
                        detector: DetectorKind::KnownValueEncoded,
                        fingerprint: Some(entry.fingerprint.render()),
                        label: format!("${{SECRET:{}}}", entry.channel_id),
                    }),
                    replacement: Some(match &entry.placeholder {
                        Some(p) => p.spelling.clone(),
                        None => RedactionTombstone {
                            detector: DetectorKind::KnownValueEncoded,
                            fingerprint: Some(entry.fingerprint.render()),
                            label: format!("${{SECRET:{}}}", entry.channel_id),
                        }
                        .render(),
                    }),
                });
                from = start + form.len();
            }
        }
    }

    // canary — planted values (Stage-1 verbatim form; encoded forms are Stage 3).
    for c in &detectors.canaries {
        if c.value.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(rel) = text[from..].find(c.value.as_str()) {
            let start = from + rel;
            hits.push(Hit {
                span: (start, start + c.value.len()),
                detector: DetectorKind::Canary,
                tombstone: Some(RedactionTombstone {
                    detector: DetectorKind::Canary,
                    fingerprint: None,
                    label: c.id.clone(),
                }),
                replacement: Some(
                    RedactionTombstone {
                        detector: DetectorKind::Canary,
                        fingerprint: None,
                        label: c.id.clone(),
                    }
                    .render(),
                ),
            });
            from = start + c.value.len();
        }
    }

    dedup_hits(hits)
}

/// Remove overlapping hits — a span covered by a `placeholder_passthrough` mark
/// suppresses nothing (it isn't a leak), but a `known_value`/`canary` hit inside
/// a `pattern` hit (or vice versa) keeps the *leftmost-longest* real hit. Two
/// different detectors on the same bytes are both real signals, but the
/// replacement can only be applied once — keep the known_value/canary hit
/// (more specific) over a pattern hit at an overlapping span.
fn dedup_hits(mut hits: Vec<Hit>) -> Vec<Hit> {
    // Stable sort by start, then by detector priority (passthrough last —
    // it marks rather than replaces).
    hits.sort_by(|a, b| {
        a.span
            .0
            .cmp(&b.span.0)
            .then(b.span.1.cmp(&a.span.1))
            .then_with(|| det_rank(a.detector).cmp(&det_rank(b.detector)))
    });
    let mut out: Vec<Hit> = Vec::new();
    for h in hits {
        if h.detector == DetectorKind::PlaceholderPassthrough {
            out.push(h);
            continue;
        }
        // Drop a replacement hit fully inside a passthrough mark (the
        // placeholder's own bytes may look like a token).
        if out.iter().any(|o| {
            o.detector == DetectorKind::PlaceholderPassthrough
                && o.span.0 <= h.span.0
                && h.span.1 <= o.span.1
        }) {
            continue;
        }
        // Drop a replacement hit overlapping an earlier (leftmost-longest,
        // higher-priority) replacement hit.
        if out.iter().any(|o| {
            o.detector != DetectorKind::PlaceholderPassthrough
                && o.span.0 < h.span.1
                && h.span.0 < o.span.1
        }) {
            continue;
        }
        out.push(h);
    }
    out.sort_by(|a, b| a.span.0.cmp(&b.span.0));
    out
}

fn det_rank(d: DetectorKind) -> u8 {
    match d {
        DetectorKind::KnownValue => 0,
        DetectorKind::KnownValueEncoded => 0,
        DetectorKind::Canary => 1,
        DetectorKind::Pattern => 2,
        DetectorKind::PlaceholderPassthrough => 3,
    }
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `leak_scan`'s target sum (§5g.3 §4: `{event, blob, request_view,
/// definition, sink_delivery}`) — the surfaces a Stage-1 verifier sweeps —
/// plus `mediation`, the live-path location a canary refusal is reported at
/// (ADR-0059 D3: a canary presented to `bind`/`mediate`/`kernel_use` is a
/// `leak_detected{location = "mediation:<verb>:<channel>"}`, not a scan hit).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScanTarget {
    /// A ledger event (`event_id` when known).
    Event(Option<String>),
    /// A blob (its content address).
    Blob(String),
    /// A staged request view (what the mediator would put on the wire).
    RequestView,
    /// A definition's canonical bytes.
    Definition(String),
    /// A sink delivery (a model-facing / external surface).
    SinkDelivery(String),
    /// The broker's live-path boundary (`bind`/`mediate`/`kernel_use`) — the
    /// detail names the verb and channel (canary attempts only).
    Mediation(String),
}

impl ScanTarget {
    /// The canonical tag.
    pub fn as_str(&self) -> &'static str {
        match self {
            ScanTarget::Event(_) => "event",
            ScanTarget::Blob(_) => "blob",
            ScanTarget::RequestView => "request_view",
            ScanTarget::Definition(_) => "definition",
            ScanTarget::SinkDelivery(_) => "sink_delivery",
            ScanTarget::Mediation(_) => "mediation",
        }
    }

    /// The `leak_detected{location}` spelling — `<tag>` or `<tag>:<detail>`.
    pub fn location(&self) -> String {
        let detail = match self {
            ScanTarget::Event(id) => id.clone().unwrap_or_default(),
            ScanTarget::Blob(a) => a.clone(),
            ScanTarget::RequestView => String::new(),
            ScanTarget::Definition(d) => d.clone(),
            ScanTarget::SinkDelivery(s) => s.clone(),
            ScanTarget::Mediation(d) => d.clone(),
        };
        if detail.is_empty() {
            self.as_str().to_string()
        } else {
            format!("{}:{}", self.as_str(), detail)
        }
    }
}

/// A leak — a `known_value`/`canary`/`pattern` hit in a scanned target (the
/// `Leak` record the `security.secret.leak_detected` row carries: `{target,
/// detector, fingerprint?}` — content-free; the *bytes* are never copied into
/// the record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leak {
    /// Where it was found.
    pub target: ScanTarget,
    /// Which detector fired.
    pub detector: DetectorKind,
    /// The `sf:` fingerprint for a known-value hit.
    pub fingerprint: Option<String>,
    /// The pattern name / canary id for a label hit.
    pub label: Option<String>,
}

impl Leak {
    /// The `leak_detected{location}` spelling (the target's `location()`).
    pub fn location_string(&self) -> String {
        self.target.location()
    }

    /// A live-path leak — e.g. a canary channel presented to `mediate`
    /// (`ScanTarget::Mediation`).
    pub fn at_mediation(
        detail: impl Into<String>,
        detector: DetectorKind,
        label: Option<String>,
    ) -> Leak {
        Leak {
            target: ScanTarget::Mediation(detail.into()),
            detector,
            fingerprint: None,
            label,
        }
    }
}

/// `leak_scan(run)` — the **test-battery** verifier (ADR-0059 D2): sweep the
/// named targets with the detector set and return every leak. `∅` is the LT-01
/// pass condition; any hit is a veto metric's numerator
/// (`security.secret.leak_detected` is the audit+triage row — this function is
/// the scan, never a live-path interceptor).
pub fn leak_scan(items: &[(ScanTarget, &str)], detectors: &DetectorSet) -> Vec<Leak> {
    let mut leaks = Vec::new();
    for (target, text) in items {
        for h in detect(text, detectors) {
            match h.detector {
                DetectorKind::PlaceholderPassthrough => {} // the safe form — not a leak
                DetectorKind::KnownValue | DetectorKind::KnownValueEncoded => {
                    leaks.push(Leak {
                        target: target.clone(),
                        detector: h.detector,
                        fingerprint: h.tombstone.and_then(|t| t.fingerprint),
                        label: None,
                    });
                }
                DetectorKind::Pattern => leaks.push(Leak {
                    target: target.clone(),
                    detector: DetectorKind::Pattern,
                    fingerprint: None,
                    label: h.tombstone.map(|t| t.label),
                }),
                DetectorKind::Canary => leaks.push(Leak {
                    target: target.clone(),
                    detector: DetectorKind::Canary,
                    fingerprint: None,
                    label: h.tombstone.map(|t| t.label),
                }),
            }
        }
    }
    leaks
}
