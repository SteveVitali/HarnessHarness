//! Loopback integration — `PluginSourceAdapter` over a `SourceChannel`
//! wired straight into `tracker::dispatch` on a `FixtureTracker`: the
//! whole wire codec exercised records-in/records-out (T-LCD-12), no
//! process needed. The out-of-process leg is
//! `crates/hh-varhost/tests/tracker_plugin.rs`.

use hh_fleet::capabilities::CapState;
use hh_fleet::source::WorkSourceAdapter;
use hh_fleet_adapter::adapter::{PluginSourceAdapter, SourceChannel, SourceFault};
use hh_fleet_adapter::tracker::{self, FixtureTracker, TrackerLogic};
use hh_wire::json::Json;

/// The in-process channel: `invoke` = `tracker::dispatch` over the
/// logic, verbatim envelopes both ways.
struct Loopback<L: TrackerLogic> {
    logic: L,
}

impl<L: TrackerLogic> SourceChannel for Loopback<L> {
    fn invoke(&mut self, operation: &str, inputs: Vec<Json>) -> Result<Vec<Json>, SourceFault> {
        tracker::dispatch(&mut self.logic, operation, &inputs)
            .map_err(|e| SourceFault::Malformed(format!("{e:?}")))
    }
}

/// A channel that always faults — the `SourceUnavailable` latch leg.
struct Dead;
impl SourceChannel for Dead {
    fn invoke(&mut self, _op: &str, _i: Vec<Json>) -> Result<Vec<Json>, SourceFault> {
        Err(SourceFault::Transport("process gone".into()))
    }
}

fn doc() -> Json {
    Json::obj([
        ("schema_version", Json::str("hh.fleet.fixture/1")),
        (
            "occurrences",
            Json::Arr(vec![Json::obj([
                ("occurrence_id", Json::str("occ-1")),
                ("trigger", Json::str("external")),
                ("kind", Json::str("ticket.updated")),
                (
                    "item",
                    Json::obj([
                        ("item_id", Json::str("i1")),
                        ("title", Json::str("ticket i1")),
                        (
                            "source",
                            Json::obj([
                                ("source_id", Json::str("tickets")),
                                ("kind", Json::str("ticket")),
                                ("state", Json::str("open")),
                            ]),
                        ),
                        ("idempotency_key", Json::str("idem:i1")),
                        ("owner", Json::str("alice")),
                    ]),
                ),
                ("observed_at_ms", Json::Int(1_100)),
                ("actor", Json::str("plugin:tracker")),
            ])]),
        ),
        ("suspended", Json::Arr(vec![])),
        ("activate_run", Json::Arr(vec![Json::str("i1")])),
        (
            "records",
            Json::Arr(vec![Json::obj([
                ("native_id", Json::str("T-1")),
                ("state", Json::str("open")),
            ])]),
        ),
    ])
}

#[test]
fn loopback_round_trip() {
    let logic = FixtureTracker::from_doc(doc()).unwrap();
    let mut ad = PluginSourceAdapter::new(Loopback { logic }, "src:tickets");
    // `occurrences` drains the plugin's snapshot — records decoded.
    let occs = ad.occurrences(None);
    assert_eq!(occs.len(), 1);
    assert_eq!(occs[0].occurrence_id, "occ-1");
    assert_eq!(occs[0].item.item_id, "i1");
    assert_eq!(occs[0].actor, "plugin:tracker");
    // `activate_run` intersects through the wire.
    assert_eq!(
        ad.activate_run(&["i1".to_string(), "i2".to_string()]),
        vec!["i1".to_string()]
    );
    // The read minimum.
    assert_eq!(ad.list(&[]).len(), 1);
    assert_eq!(ad.get(&["T-1".to_string()]).len(), 1);
    assert!(ad.get(&["T-9".to_string()]).is_empty());
    assert!(ad.fault().is_none());
}

#[test]
fn capabilities_probe_tri_state() {
    let logic = FixtureTracker::from_doc(doc()).unwrap();
    let mut ad = PluginSourceAdapter::new(Loopback { logic }, "src:tickets");
    // Before the probe: all-unknown (never a fabricated declared).
    assert_eq!(ad.capabilities().poll, CapState::Unknown);
    ad.bind_probe();
    let caps = ad.capabilities();
    // The fixture defaults `poll` declared, the rest unknown.
    assert_eq!(caps.poll, CapState::Declared);
    assert_eq!(caps.push_delivery_id, CapState::Unknown);
}

#[test]
fn faulted_channel_latches_source_unavailable() {
    let mut ad = PluginSourceAdapter::new(Dead, "src:gone");
    // A faulted call answers the honest empty set — nothing ledgered —
    // and latches `SourceUnavailable` for the reconciler.
    assert!(ad.occurrences(None).is_empty());
    let f = ad.fault().expect("latched");
    assert!(f.contains("transport"), "{f}");
    // Sticky: subsequent calls keep answering empty, fault stays.
    assert!(ad.list(&[]).is_empty());
    assert!(ad.fault().is_some());
}

#[test]
fn malformed_reply_latches() {
    struct Bad;
    impl SourceChannel for Bad {
        fn invoke(&mut self, _o: &str, _i: Vec<Json>) -> Result<Vec<Json>, SourceFault> {
            Ok(vec![Json::obj([("bogus", Json::Int(1))])])
        }
    }
    let mut ad = PluginSourceAdapter::new(Bad, "src:bad");
    assert!(ad.occurrences(None).is_empty());
    let f = ad.fault().expect("latched");
    assert!(f.contains("malformed"), "{f}");
}

#[test]
fn reported_fault_latches() {
    struct Reporter;
    impl SourceChannel for Reporter {
        fn invoke(&mut self, _o: &str, _i: Vec<Json>) -> Result<Vec<Json>, SourceFault> {
            Ok(vec![Json::obj([("fault", Json::str("tracker 503"))])])
        }
    }
    let mut ad = PluginSourceAdapter::new(Reporter, "src:r");
    assert!(ad.occurrences(None).is_empty());
    let f = ad.fault().expect("latched");
    assert!(f.contains("tracker 503"), "{f}");
}
