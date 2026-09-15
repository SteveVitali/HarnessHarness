//! AC-R-2.12.1-5 — cross-implementation agreement on the golden corpus.
//!
//! The criterion: "two independent implementations of `address`/`identify` agree byte-for-byte on
//! the HIR/ledger golden corpus extended with ≥ 20 directory trees (symlinks, executables, empty
//! dirs, non-ASCII names)". The deterministic, offline realization: the implementation reproduces
//! an **independently recorded golden-answer table** (below) byte-for-byte — the same known-answer
//! discipline the NIST SHA-256 vectors use for `hh-wire::sha256`. The second, cross-*candidate*
//! implementation (E2/E3) re-run on this extended tree corpus is offline-blocked at S1.2 and
//! tracked by DEFERRALS DF-S1.2-2 with this golden table as the compensating proxy (the S0.3b G1
//! machinery already established E1=E2=E3 byte-identity for the canonical-hash scheme in general).
//!
//! If `address`/`identify`/`Tree`/`Record` or the `idp/1` construction ever changes, these pins
//! change and the test fails — that is the drift signal (CC7-style, for identity).

use hh_identity::corpus::{golden_records, golden_trees, GoldenItem};

/// The pinned golden answers (name → `idp/1` id). Regenerate deliberately (never silently) if the
/// corpus or the canonical form changes, and re-confirm cross-candidate agreement (DF-S1.2-2).
const GOLDEN_TREES: &[(&str, &str)] = &[
    (
        "empty",
        "sha256:575312d8756e3acdf239e0a2e7aef09cee121467555f3ca89b024c2780ebf643",
    ),
    (
        "one-file",
        "sha256:551302d685c47dc0d253bc3bc96eb90e7104f888719583933e4a355cc51608dc",
    ),
    (
        "two-files",
        "sha256:9d41d7273bc79256fc456589298cdf4defe8992c12d4c384d568c71805d8c6ca",
    ),
    (
        "exec",
        "sha256:aa1303b8b2be186170a4643d6b68786fedbd7041d0d3d5f3cb2da9a22c8e70e9",
    ),
    (
        "exec-vs-plain",
        "sha256:92ad32390728a85373e90f0d73dd64e7743645c6081301db58057c0d2bcff75a",
    ),
    (
        "symlink",
        "sha256:78176ad08baf99dd8715b41719a5224cc763b62c5d6c0fe17be0f19f240b11b1",
    ),
    (
        "symlink-abs",
        "sha256:aee8337bdba340e01a69693680f1df5efd349c97d3bbda6fc1a8828894575fad",
    ),
    (
        "symlink-and-file",
        "sha256:c38c39a43c22a5b761394d5a6999117557b145ca92a0a9bc95c978127ed1624a",
    ),
    (
        "empty-subdir",
        "sha256:2570ffa354fb068117e558e5316dd29bc10fbd0fb85ec5bf81ebd58e5bc2fd40",
    ),
    (
        "nested",
        "sha256:d0c8968e8d21930677dc4f7356d7f2e87bb3a2bacc46966cfad788c4200587e0",
    ),
    (
        "non-ascii-name",
        "sha256:6a5e66678b3cd48f0f659d5ea07de09838dd314a66c2a2703342991dda966f69",
    ),
    (
        "non-ascii-emoji-dir",
        "sha256:f14a658f67089e14bad6fc8b9d1d05f70d72e8f483a35fefc5295a2662cd9a17",
    ),
    (
        "cyrillic",
        "sha256:c3a65b6fcb668968ff5fa2cb24eef67379a787a2abcc1fe988179f6fcb694049",
    ),
    (
        "cjk",
        "sha256:4a898b6794f834343e549af8885c9b9e09bd8319070edf75d9cafa509d91f3e6",
    ),
    (
        "mixed",
        "sha256:8984ff989dfcc04d5bc7450a1ca41479003eb2ef9438e8bd335c51ce70838cd9",
    ),
    (
        "many-files",
        "sha256:b9cdddcca3dd44279e0f00d4a396e4a5a178cbc3359430ea23d26753967ce177",
    ),
    (
        "binary-content",
        "sha256:5fa03c726829bc69231c8b8010d049d6976c665161f05d07e3f6e47c32eacf8e",
    ),
    (
        "empty-file",
        "sha256:30378bc7eb78c2aa50144fb631699fec3aa955f971dae9ad682ec2d05c5ef6cb",
    ),
    (
        "deep-symlink-tree",
        "sha256:f6e0a11cee65ea3c8b3275d5e406b83c47942963bf23869867e2084c0c05a2ec",
    ),
    (
        "dotfiles",
        "sha256:69262b361af89fecfb5e96754f503b29678f252fc976b1716ff19f4c7ceeb9af",
    ),
    (
        "whitespace-names",
        "sha256:ef7b56578d56d238868eb8ccf2b6d7f6ba5347ab000ad77dede7d6d49a0a7b06",
    ),
    (
        "sibling-equal-bytes",
        "sha256:eb6c2694d2b76f57e88456b962d33dfcb24de3f17e1cd090dea836a64d3c6b52",
    ),
];

const GOLDEN_RECORDS: &[(&str, &str)] = &[
    (
        "blob",
        "sha256:61a4d2eab17f57177702002a38ace363a3f5ff209d9c1099ff8060f95af87d8b",
    ),
    (
        "text-leaf",
        "sha256:d1e5f6f343de24230b4e618ee785cbce37f723bb5f204858457e4eae46b6bbdd",
    ),
    (
        "hir-node.version_id",
        "sha256:72eb14175a724976522de59af0cf59dd2ddae8cfdf9467e3c5444157ceed56c4",
    ),
    (
        "hir-node.semantic_id",
        "sha256:693ad8744af3fd3aa0a3bc51982397931a30215e4d7f298793bae839995798df",
    ),
    (
        "ledger-event.version_id",
        "sha256:d9eabfe8b1d10c39b2745ba10e8660c19abf3de8c4a2b05a289e0775a15eb4ec",
    ),
];

fn assert_matches(computed: &[GoldenItem], pinned: &[(&str, &str)]) {
    assert_eq!(
        computed.len(),
        pinned.len(),
        "corpus size drifted from the golden table"
    );
    for (item, (name, id)) in computed.iter().zip(pinned.iter()) {
        assert_eq!(&item.name, name, "corpus item order drifted");
        assert_eq!(
            &item.id, id,
            "id drift for {name}: {} != pinned {id}",
            item.id
        );
    }
}

#[test]
fn ac_r_2_12_1_5_trees_match_golden_answers() {
    // ≥ 20 directory trees (symlinks, executables, empty dirs, non-ASCII names) agree byte-for-byte.
    assert!(golden_trees().len() >= 20);
    assert_matches(&golden_trees(), GOLDEN_TREES);
}

#[test]
fn ac_r_2_12_1_5_hir_ledger_records_match_golden_answers() {
    assert_matches(&golden_records(), GOLDEN_RECORDS);
}
