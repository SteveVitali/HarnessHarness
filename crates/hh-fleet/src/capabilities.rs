//! `AdapterCapabilities` — the `WorkSourceAdapter` capability record
//! (§5i.1 #3; ADR-0205 D4; T-LCD-07). Each capability member is the
//! tri-state `{declared, probed, unknown}`:
//!
//! - `declared` — the adapter asserts support (unverified at bind);
//! - `probed`   — the host verified it by probing (a `capabilities`
//!   invoke at bind for a plugin; a delivered `delivery_id` for the
//!   webhook lane);
//! - `unknown`  — unanswerable, **never coerced** (an adapter that
//!   cannot say stays `unknown`; a probe that fails leaves it
//!   `unknown`, never `declared`).
//!
//! `volatile_fields[]` is the adapter's declaration for the OQ-313
//! content hash (members stripped before `H(canonical(payload −
//! volatile_fields))` derives the id-less push occurrence key).
//!
//! The record is data — it serializes through `to_json`/`from_json` and
//! rides the `capabilities` member of the fixture doc, the plugin's
//! `capabilities` verb, and the FleetView `source.capabilities` member.
//! One spelling, one codec (CC1/CC7).

use hh_wire::json::Json;

/// One capability member's state — `{state, supported}`; `unknown`
/// carries no `supported` member (unanswerable is a third value, never
/// a hidden `false`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapState {
    /// The adapter declares support (unverified).
    Declared,
    /// The adapter declares the capability absent (declared
    /// unsupported — an honest negative, not `unknown`).
    DeclaredAbsent,
    /// The host probed and support is confirmed.
    Probed,
    /// The host probed and the capability is confirmed absent.
    ProbedAbsent,
    /// Unanswerable — never coerced (T-LCD-07).
    Unknown,
}

impl Default for CapState {
    /// `unknown` is the default — never a hidden `false`.
    fn default() -> Self {
        CapState::Unknown
    }
}

impl CapState {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CapState::Declared => "declared",
            CapState::DeclaredAbsent => "declared_absent",
            CapState::Probed => "probed",
            CapState::ProbedAbsent => "probed_absent",
            CapState::Unknown => "unknown",
        }
    }

    /// Parse — strict; anything else fails closed (never coerced).
    pub fn parse(s: &str) -> Option<CapState> {
        Some(match s {
            "declared" => CapState::Declared,
            "declared_absent" => CapState::DeclaredAbsent,
            "probed" => CapState::Probed,
            "probed_absent" => CapState::ProbedAbsent,
            "unknown" => CapState::Unknown,
            _ => return None,
        })
    }

    /// Whether the capability is usable (declared or probed support).
    pub fn supported(self) -> bool {
        matches!(self, CapState::Declared | CapState::Probed)
    }
}

/// The capability member names (the closed set — ADR-0205 D4).
pub const CAPABILITY_NAMES: &[&str] = &[
    "push_delivery_id",
    "poll",
    "native_write_tools",
    "assignee_mapping",
    "blocker_metadata",
];

/// `AdapterCapabilities` — the full record.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AdapterCapabilities {
    /// `push_delivery_id` — the source supplies a durable delivery id on
    /// pushes (the OQ-313 namespaced-key leg).
    pub push_delivery_id: CapState,
    /// `poll` — the source supports `list(states[])`/`get(native_ids[])`.
    pub poll: CapState,
    /// `native_write_tools` — the source exposes lifted write
    /// capabilities (agent effects through Π; never reconciler verbs).
    pub native_write_tools: CapState,
    /// `assignee_mapping` — `assignee` maps to a `PrincipalRef` through
    /// the adapter's mapping (OQ-462 / deferral D-1's slot).
    pub assignee_mapping: CapState,
    /// `blocker_metadata` — the source carries `blocked_by[]` the
    /// adapter can surface (display-only at the reconciler).
    pub blocker_metadata: CapState,
    /// `volatile_fields[]` — members stripped before the content hash.
    pub volatile_fields: Vec<String>,
}

impl AdapterCapabilities {
    /// The all-`unknown` record (the trait default — an adapter that
    /// declares nothing answers `unknown` for every member).
    pub fn all_unknown() -> AdapterCapabilities {
        AdapterCapabilities {
            push_delivery_id: CapState::Unknown,
            poll: CapState::Unknown,
            native_write_tools: CapState::Unknown,
            assignee_mapping: CapState::Unknown,
            blocker_metadata: CapState::Unknown,
            volatile_fields: Vec::new(),
        }
    }

    /// Read one member by its canonical name (`None` on an unknown
    /// name — the member set is closed).
    pub fn get(&self, name: &str) -> Option<CapState> {
        Some(match name {
            "push_delivery_id" => self.push_delivery_id,
            "poll" => self.poll,
            "native_write_tools" => self.native_write_tools,
            "assignee_mapping" => self.assignee_mapping,
            "blocker_metadata" => self.blocker_metadata,
            _ => return None,
        })
    }

    /// Write one member by its canonical name.
    pub fn set(&mut self, name: &str, state: CapState) -> bool {
        match name {
            "push_delivery_id" => self.push_delivery_id = state,
            "poll" => self.poll = state,
            "native_write_tools" => self.native_write_tools = state,
            "assignee_mapping" => self.assignee_mapping = state,
            "blocker_metadata" => self.blocker_metadata = state,
            _ => return false,
        }
        true
    }

    /// The probe fold — a member at `unknown` that the probe answered
    /// becomes `probed`/`probed_absent`; every other member is
    /// unchanged (a `declared` member is never *downgraded* by a probe
    /// — probing only resolves `unknown`, T-LCD-07).
    pub fn probe_with(&mut self, name: &str, supported: bool) -> bool {
        let Some(cur) = self.get(name) else {
            return false;
        };
        if cur == CapState::Unknown {
            self.set(
                name,
                if supported {
                    CapState::Probed
                } else {
                    CapState::ProbedAbsent
                },
            );
        }
        true
    }

    /// The canonical record `{push_delivery_id, …, volatile_fields[]}`.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "push_delivery_id",
                Json::str(self.push_delivery_id.as_str()),
            ),
            ("poll", Json::str(self.poll.as_str())),
            (
                "native_write_tools",
                Json::str(self.native_write_tools.as_str()),
            ),
            (
                "assignee_mapping",
                Json::str(self.assignee_mapping.as_str()),
            ),
            (
                "blocker_metadata",
                Json::str(self.blocker_metadata.as_str()),
            ),
            (
                "volatile_fields",
                Json::Arr(self.volatile_fields.iter().map(Json::str).collect()),
            ),
        ])
    }

    /// Strict decode — unknown members / unknown state spellings fail
    /// closed (a capability record is declaration data; silence is a
    /// misparse, never a default).
    pub fn from_json(j: &Json) -> Result<AdapterCapabilities, String> {
        let Json::Obj(o) = j else {
            return Err("capabilities must be an object".into());
        };
        for k in o.keys() {
            if !CAPABILITY_NAMES.contains(&k.as_str()) && k != "volatile_fields" {
                return Err(format!("capabilities unknown member {k}"));
            }
        }
        let mut caps = AdapterCapabilities::all_unknown();
        for name in CAPABILITY_NAMES {
            if let Some(v) = o.get(*name) {
                let s = v
                    .as_str()
                    .ok_or_else(|| format!("capabilities.{name} must be a string"))?;
                let st = CapState::parse(s)
                    .ok_or_else(|| format!("capabilities.{name} unknown state {s}"))?;
                caps.set(name, st);
            }
        }
        if let Some(v) = o.get("volatile_fields") {
            match v {
                Json::Arr(a) => {
                    for x in a {
                        caps.volatile_fields.push(
                            x.as_str()
                                .ok_or("capabilities.volatile_fields members must be strings")?
                                .to_string(),
                        );
                    }
                }
                Json::Null => {}
                _ => return Err("capabilities.volatile_fields must be an array".into()),
            }
        }
        Ok(caps)
    }
}
