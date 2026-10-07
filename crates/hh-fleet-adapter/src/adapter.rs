//! `PluginSourceAdapter` — the host-side `WorkSourceAdapter` over a
//! [`SourceChannel`] (§5i.1 #3's L5 plugin lane; ADR-0180/0181). Every
//! trait method is one `invoke` of the class op; the transport supplies
//! the channel (the variant host's bound session in production, a
//! loopback in tests — records-in/records-out, T-LCD-12).
//!
//! Fault discipline (§5i.1 #5): a channel failure latches — `fault()`
//! answers `Some(source_ref)` from then on, and the reconciler's
//! `SourceUnavailable` arm skips the adapter-reading passes while
//! keeping activations. The adapter never synthesizes records out of a
//! failed call: a faulting `occurrences` answers `[]` (nothing ledgered)
//! and the *next* pass reports the latched fault.

use hh_fleet::capabilities::AdapterCapabilities;
use hh_fleet::source::{SourceOccurrence, WorkSourceAdapter};
use hh_wire::json::Json;

use crate::abi;
use crate::abi::AbiCodecError;

/// `SourceChannel` — the transport seam: `invoke(op, inputs) →
/// outputs[]` where outputs are `plugin_abi/1` envelopes
/// (`{result: "<canonical-json>"}`).
pub trait SourceChannel {
    /// One class operation.
    fn invoke(&mut self, operation: &str, inputs: Vec<Json>) -> Result<Vec<Json>, SourceFault>;
}

/// `SourceFault` — the channel's failure classes (never a domain
/// refusal — those are data inside `outputs`).
#[derive(Debug, Clone, PartialEq)]
pub enum SourceFault {
    /// The transport/session failed (timeout, crash, detach).
    Transport(String),
    /// The reply failed the strict envelope decode.
    Malformed(String),
    /// The plugin reported a domain fault (`{fault: "…"}` in outputs).
    Reported(String),
}

impl std::fmt::Display for SourceFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceFault::Transport(m) => write!(f, "transport:{m}"),
            SourceFault::Malformed(m) => write!(f, "malformed:{m}"),
            SourceFault::Reported(m) => write!(f, "reported:{m}"),
        }
    }
}

impl std::error::Error for SourceFault {}

/// `PluginSourceAdapter` — `WorkSourceAdapter` over a bound plugin
/// session. `&mut self` throughout (the channel is stateful — queue
/// drains, session seq, latching).
pub struct PluginSourceAdapter<C: SourceChannel> {
    /// The bound channel.
    pub channel: C,
    /// The adapter's `source_ref`/`source_id` label the fault answers
    /// carry (the reconciler needs the *coordinate*, not the bytes).
    pub source_label: String,
    /// The latched fault — once set, the adapter is `SourceUnavailable`.
    latched: Option<String>,
    /// The capability record cached at `bind_probe` (the trait's
    /// `capabilities` is `&self` — the probe runs once, out of process,
    /// at bind; the cached tri-states are the honest answer).
    bound_caps: AdapterCapabilities,
}

impl<C: SourceChannel> PluginSourceAdapter<C> {
    /// `new(channel, source_label)` — the label is the source coordinate
    /// the reconciler's `SourceUnavailable{source_ref}` answers carry.
    /// Capability state starts all-`unknown` until [`Self::bind_probe`].
    pub fn new(channel: C, source_label: &str) -> PluginSourceAdapter<C> {
        PluginSourceAdapter {
            channel,
            source_label: source_label.to_string(),
            latched: None,
            bound_caps: AdapterCapabilities::all_unknown(),
        }
    }

    /// One op → its decoded first document. A fault latches and answers
    /// `Err` — the caller (the trait arms) maps to the honest empty
    /// answer and leaves the latch for `fault()`.
    fn call(&mut self, op: &str, inputs: Vec<Json>) -> Result<Json, SourceFault> {
        let outs = self.channel.invoke(op, inputs).map_err(|e| {
            self.latched = Some(format!("{e}"));
            e
        })?;
        // A plugin may answer `{fault: "…"}` — a reported domain fault,
        // not a decode error.
        let first = outs.into_iter().next().unwrap_or(Json::Null);
        if let Some(f) = first.get("fault").and_then(Json::as_str) {
            let e = SourceFault::Reported(f.to_string());
            self.latched = Some(format!("{e}"));
            return Err(e);
        }
        let doc = abi::unwrap_output(&first).map_err(|e: AbiCodecError| {
            let e = SourceFault::Malformed(match e {
                AbiCodecError::Malformed { detail } => detail,
            });
            self.latched = Some(format!("{e}"));
            e
        })?;
        Ok(doc)
    }

    /// The empty answer on a faulted call — the trait arms share it.
    fn empty<T: Default>(&mut self) -> T {
        T::default()
    }
}

impl<C: SourceChannel> WorkSourceAdapter for PluginSourceAdapter<C> {
    fn occurrences(&mut self, since: Option<u64>) -> Vec<SourceOccurrence> {
        let input = Json::obj([(
            "since",
            since.map(|s| Json::Int(s as i64)).unwrap_or(Json::Null),
        )]);
        match self.call("occurrences", vec![input]) {
            Ok(Json::Arr(a)) => a
                .iter()
                .filter_map(|v| SourceOccurrence::from_json(v).ok())
                .collect(),
            Ok(_) => {
                self.latched = Some("malformed:occurrences answer is not an array".into());
                Vec::new()
            }
            Err(_) => self.empty(),
        }
    }

    fn suspended(&mut self, source_id: &str) -> bool {
        match self.call(
            "suspended",
            vec![Json::obj([("source_id", Json::str(source_id))])],
        ) {
            Ok(Json::Bool(b)) => b,
            Ok(_) => {
                self.latched = Some("malformed:suspended answer is not a bool".into());
                false
            }
            Err(_) => false,
        }
    }

    fn activate_run(&mut self, candidates: &[String]) -> Vec<String> {
        match self.call(
            "activate_run",
            vec![Json::obj([(
                "candidates",
                Json::Arr(candidates.iter().map(Json::str).collect()),
            )])],
        ) {
            Ok(Json::Arr(a)) => a
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
            Ok(_) => {
                self.latched = Some("malformed:activate_run answer is not an array".into());
                Vec::new()
            }
            Err(_) => Vec::new(),
        }
    }

    fn list(&mut self, states: &[String]) -> Vec<Json> {
        match self.call(
            "list",
            vec![Json::obj([(
                "states",
                Json::Arr(states.iter().map(Json::str).collect()),
            )])],
        ) {
            Ok(Json::Arr(a)) => a,
            Ok(_) => {
                self.latched = Some("malformed:list answer is not an array".into());
                Vec::new()
            }
            Err(_) => Vec::new(),
        }
    }

    fn get(&mut self, native_ids: &[String]) -> Vec<Json> {
        match self.call(
            "get",
            vec![Json::obj([(
                "native_ids",
                Json::Arr(native_ids.iter().map(Json::str).collect()),
            )])],
        ) {
            Ok(Json::Arr(a)) => a,
            Ok(_) => {
                self.latched = Some("malformed:get answer is not an array".into());
                Vec::new()
            }
            Err(_) => Vec::new(),
        }
    }

    fn capabilities(&self) -> AdapterCapabilities {
        // `capabilities` is `&self` on the trait — the adapter caches the
        // record at bind (`bind_probe`) and re-reads it here. The plugin
        // probe runs once out of process; the cached answer is the
        // honest "probed" record (never a re-invocation inside `&self`).
        self.bound_caps.clone()
    }

    fn fault(&self) -> Option<String> {
        self.latched.clone()
    }
}

impl<C: SourceChannel> PluginSourceAdapter<C> {
    /// `bind_probe()` — the bind-time capability probe (T-LCD-07): one
    /// `capabilities` invoke out of process; the answer's tri-states are
    /// cached verbatim (a probe failure leaves the record `unknown`,
    /// never coerced).
    pub fn bind_probe(&mut self) {
        let probed = self
            .call("capabilities", vec![])
            .ok()
            .and_then(|d| AdapterCapabilities::from_json(&d).ok());
        match probed {
            Some(c) => self.bound_caps = c,
            None => {
                // A failed probe keeps the all-unknown record — and the
                // latch is NOT set by a refused capabilities read alone
                // unless the channel itself faulted (`call` latches it).
            }
        }
    }
}
