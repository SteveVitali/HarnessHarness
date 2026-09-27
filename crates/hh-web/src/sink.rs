//! The surface-side sink enforcement (P8) — the kernel lowers rows by
//! the declared `SinkPolicy` and mints the delivery record; the surface
//! applies the *serving* half again here: view payloads whose declared
//! classes exceed the policy's `content_classes` are withheld (a
//! `{"withheld": "<class>"}` marker replaces the member — the browser
//! sees the shape, never the bytes), and string members longer than
//! `max_field_bytes` truncate with a marker.
//!
//! What the surface withholds vs. what the kernel withholds: the kernel
//! owns the *row* stream (`read`/`stream` events lower per class); the
//! surface owns the *assembled view* — e.g. an artifact's `content` or a
//! context item's payload is a `content`-class member of the ViewModel,
//! so a `{accounting}`-only policy withholds it even though the row
//! listing (ids, sizes, hashes — accounting) still serves.

use hh_telemetry::sinks::{ContentClass, SinkPolicy};
use hh_wire::json::Json;

/// Which view-member classes each returned shape declares — checked
/// against `policy.content_classes`; a disallowed member is replaced by
/// the withheld marker (the member key stays so the shape is stable).
pub fn enforce_object(policy: &SinkPolicy, obj: Json) -> Json {
    let mut m = match obj {
        Json::Obj(m) => m,
        other => return other,
    };
    let admits_content = policy.content_classes.contains(&ContentClass::Content);
    let admits_diag = policy.content_classes.contains(&ContentClass::Diagnostic);
    if !admits_content || !admits_diag {
        for (k, v) in m.iter_mut() {
            let class = match k.as_str() {
                // Members the view catalogue marks content-bearing —
                // artifact/context payloads, message bodies, tool args.
                "content" | "payload" | "body" | "items" | "artifact_content" => {
                    Some(ContentClass::Content)
                }
                // Members that carry raw diagnostics.
                "diagnostic" | "stderr_tail" | "raw_trace" => Some(ContentClass::Diagnostic),
                _ => None,
            };
            if let Some(c) = class {
                let admitted = match c {
                    ContentClass::Content => admits_content,
                    ContentClass::Diagnostic => admits_diag,
                    _ => true,
                };
                if !admitted {
                    *v = Json::obj([("withheld", Json::str(c.as_str()))]);
                }
            }
        }
    }
    // The field cap applies to every surviving string member.
    let cap = policy.max_field_bytes as usize;
    let _ = cap;
    truncate_obj(&mut m, cap);
    Json::Obj(m)
}

fn truncate_obj(m: &mut std::collections::BTreeMap<String, Json>, cap: usize) {
    for v in m.values_mut() {
        truncate(v, cap);
    }
}

fn truncate(j: &mut Json, cap: usize) {
    match j {
        Json::Str(s) if s.len() > cap => {
            let cut = s.len() - cap;
            *s = format!("{}…(+{cut} bytes withheld)", &s[..cap]);
        }
        Json::Obj(m) => {
            for v in m.values_mut() {
                truncate(v, cap);
            }
        }
        Json::Arr(a) => {
            for v in a.iter_mut() {
                truncate(v, cap);
            }
        }
        _ => {}
    }
}

/// Decode + validate the declared policy (`--sink` JSON); an invalid
/// declaration refuses at startup (P8 — the sink declares truthfully or
/// not at all).
pub fn parse_policy(j: &Json) -> Result<SinkPolicy, String> {
    let p = SinkPolicy::from_json(j).map_err(|e| format!("invalid --sink policy: {e}"))?;
    p.validate()
        .map_err(|e| format!("invalid --sink policy: {e}"))?;
    Ok(p)
}
