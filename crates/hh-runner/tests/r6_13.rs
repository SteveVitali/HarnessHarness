//! r6_13 — the diskwatch unit test (manifest 266/R6.13; REQ-XB-207):
//! the runner stops admitting slots below the declared free-disk floor
//! (default 20 GiB) and prunes derived harness layers after their task's
//! slots settle — never shared layers, never in-flight tasks.

use hharness_runner::diskwatch::{admit, prune_set, Admission, Layer, DEFAULT_DISK_FLOOR_GIB};

#[test]
fn admission_stops_below_the_declared_floor() {
    // default 20 GiB floor, inclusive boundary
    assert_eq!(admit(21, DEFAULT_DISK_FLOOR_GIB), Admission::Admit);
    assert_eq!(admit(20, DEFAULT_DISK_FLOOR_GIB), Admission::Admit);
    assert_eq!(admit(19, DEFAULT_DISK_FLOOR_GIB), Admission::StopBelowFloor);
    assert_eq!(admit(0, DEFAULT_DISK_FLOOR_GIB), Admission::StopBelowFloor);
    // a declared tighter/looser floor is honored
    assert_eq!(admit(5, 4), Admission::Admit);
    assert_eq!(admit(3, 4), Admission::StopBelowFloor);
}

#[test]
fn prune_targets_only_settled_tasks_derived_layers() {
    let layers = vec![
        Layer {
            digest: "sha256:base".into(),
            task: None,
        },
        Layer {
            digest: "sha256:a1".into(),
            task: Some("task-a".into()),
        },
        Layer {
            digest: "sha256:a2".into(),
            task: Some("task-a".into()),
        },
        Layer {
            digest: "sha256:b1".into(),
            task: Some("task-b".into()),
        },
    ];
    // task-a's slots settled → its derived layers prune; shared base and
    // the still-in-flight task-b layer stay.
    let gone = prune_set(&layers, &["task-a"]);
    assert_eq!(gone, vec!["sha256:a1".to_string(), "sha256:a2".to_string()]);
    // nothing settled → nothing pruned
    assert!(prune_set(&layers, &[]).is_empty());
    // all settled → shared base layer still kept
    let gone = prune_set(&layers, &["task-a", "task-b"]);
    assert!(!gone.contains(&"sha256:base".to_string()));
    assert_eq!(gone.len(), 3);
}
