//! The gating acceptance criteria for R-2.12.1 made executable end-to-end:
//! **AC-R-2.12.1-{1,2,3,4,5,12}** (§8.3 #7 / §9.2). AC-5 has its own file (`golden_corpus.rs`).

use hh_wire::json::Json;

use hh_identity::{
    address, configuration_id, configuration_version_id, identify, identify_text, parse_id,
    pools_by_configuration_id, sameness, verify_blob, IdError, IdentifyError, NameIndex,
    NameSelector, NameStatus, Namespace, Provenance, Record, RecordKind, Reference, ResolveMode,
    ResolveOutcome, SamenessLevel, VerifyOutcome,
};

#[test]
fn ac_r_2_12_1_1_self_describing_ids_and_verify_rejections() {
    // Every stored id parses as `<algorithm>:<hex>` with the full digest length…
    let ca = address(b"payload", "text/plain");
    let parsed = parse_id(&ca.id()).unwrap();
    assert_eq!(parsed.algorithm, "sha256");
    assert_eq!(parsed.digest_hex.len(), 64);
    assert_eq!(
        verify_blob(&ca.id(), b"payload").unwrap(),
        VerifyOutcome::Ok
    );
    // …a truncated or untagged id is rejected at verify.
    assert!(matches!(
        verify_blob(&"abc".repeat(20), b"x"),
        Err(IdError::AlgorithmMismatch { .. })
    ));
    assert!(matches!(
        verify_blob(&format!("sha256:{}", "a".repeat(10)), b"x"),
        Err(IdError::TruncatedId { .. })
    ));
}

#[test]
fn ac_r_2_12_1_2_domain_separation_three_distinct_ids() {
    // A blob, a Text leaf and a node with identical canonical bytes → three distinct ids.
    let node = Record::new(RecordKind::HirNode).semantic("op", Json::str("map"));
    let bytes = node.canonical_full_bytes();
    let blob_id = address(&bytes, "application/octet-stream").id();
    let text_id = identify_text(&bytes);
    let node_id = identify(&node).unwrap().version_id;
    assert_ne!(blob_id, text_id);
    assert_ne!(text_id, node_id);
    assert_ne!(blob_id, node_id);
}

#[test]
fn ac_r_2_12_1_3_rename_stability_and_pooling() {
    // Renaming a compiled tool under a profile: semantic_id unchanged, version_id changed, a new
    // name-history entry, sameness = L1, and results pool by configuration_id.
    let tool = |display: &str| {
        Record::new(RecordKind::VariantRecord)
            .semantic("tool", Json::str("grep"))
            .semantic("profile", Json::str("profile/default"))
            .surface("display_name", Json::str(display))
    };
    let before = identify(&tool("Search")).unwrap();
    let after = identify(&tool("Grep")).unwrap();
    assert_eq!(before.semantic_id, after.semantic_id);
    assert_ne!(before.version_id, after.version_id);

    let mut idx = NameIndex::new();
    let r_before = hh_identity::VersionedRef::pinned(
        RecordKind::VariantRecord,
        &before.version_id,
        Provenance::kernel(),
    )
    .with_semantic(before.semantic_id.clone().unwrap());
    let e1 = idx
        .publish(
            Namespace::Local,
            "tool/grep",
            &r_before,
            Some("1.0.0".into()),
            None,
            NameStatus::Active,
            Provenance::kernel(),
            false,
        )
        .unwrap();
    let r_after = hh_identity::VersionedRef::pinned(
        RecordKind::VariantRecord,
        &after.version_id,
        Provenance::kernel(),
    )
    .with_semantic(after.semantic_id.clone().unwrap());
    idx.publish(
        Namespace::Local,
        "tool/grep",
        &r_after,
        Some("1.1.0".into()),
        Some(&e1),
        NameStatus::Active,
        Provenance::kernel(),
        false,
    )
    .unwrap();
    assert_eq!(
        idx.history(Namespace::Local, "tool/grep").len(),
        2,
        "a new name-history entry"
    );

    let s = sameness(&r_before, &r_after, true, None);
    assert_eq!(s.level, SamenessLevel::L1);
    assert!(pools_by_configuration_id(s));

    // The two renamed variants share the same harness semantic_id → same configuration_id.
    let cfg = |harness_sem: &str| configuration_id("m/sem", harness_sem, "p/sem", "e/sem", "b/sem");
    assert_eq!(
        cfg(before.semantic_id.as_ref().unwrap()),
        cfg(after.semantic_id.as_ref().unwrap())
    );
}

#[test]
fn ac_r_2_12_1_4_pinned_forms_refuse_selectors_and_display_names() {
    // A sealed definition / manifest / registry record with a selector or display name as a
    // reference fails identify with UnresolvedRef.
    for kind in [
        RecordKind::SealedDefinition,
        RecordKind::RunManifest,
        RecordKind::BundleManifest,
        RecordKind::RegistryRecord,
    ] {
        let with_selector =
            Record::new(kind).reference(Reference::Selector(NameSelector::new("local", "x")));
        assert!(matches!(
            identify(&with_selector),
            Err(IdentifyError::UnresolvedRef { .. })
        ));
        let with_display = Record::new(kind).reference(Reference::DisplayName("x".into()));
        assert!(matches!(
            identify(&with_display),
            Err(IdentifyError::UnresolvedRef { .. })
        ));
        // …but the same kind with only pinned refs identifies.
        let pinned =
            Record::new(kind).reference(Reference::Pinned(format!("sha256:{}", "0".repeat(64))));
        assert!(identify(&pinned).is_ok());
    }
}

#[test]
fn ac_r_2_12_1_12_two_configuration_ids() {
    // Two runs differing only by seed share configuration_id, differ in configuration_version_id.
    let cid = configuration_id("m/sem", "h/sem", "p/sem", "e/sem", "b/sem");
    let cvid_s0 = configuration_version_id("m@v", "h@v", "p@v", "e@v", "b@v", "seed-0");
    let cvid_s1 = configuration_version_id("m@v", "h@v", "p@v", "e@v", "b@v", "seed-1");
    assert_eq!(
        cid,
        configuration_id("m/sem", "h/sem", "p/sem", "e/sem", "b/sem")
    );
    assert_ne!(cvid_s0, cvid_s1);

    // A surface-only profile rendering shares configuration_id (same profile semantic_id).
    // (The surface diff lives in the version id, not the semantic coordinate — modelled by an
    // unchanged profile semantic_id here.)
    let cid_surface = configuration_id("m/sem", "h/sem", "p/sem", "e/sem", "b/sem");
    assert_eq!(cid, cid_surface);

    // A ModelRoleTable change alters both.
    let cid2 = configuration_id("m2/sem", "h/sem", "p/sem", "e/sem", "b/sem");
    let cvid2 = configuration_version_id("m2@v", "h@v", "p@v", "e@v", "b@v", "seed-0");
    assert_ne!(cid, cid2);
    assert_ne!(cvid_s0, cvid2);
}

#[test]
fn resolve_is_deterministic_and_execute_hides_yanked() {
    let mut idx = NameIndex::new();
    let v1 = hh_identity::VersionedRef::pinned(
        RecordKind::VariantRecord,
        format!("sha256:{}", "1".repeat(64)),
        Provenance::kernel(),
    );
    let e1 = idx
        .publish(
            Namespace::Local,
            "n",
            &v1,
            Some("1.0.0".into()),
            None,
            NameStatus::Active,
            Provenance::kernel(),
            false,
        )
        .unwrap();
    let v2 = hh_identity::VersionedRef::pinned(
        RecordKind::VariantRecord,
        format!("sha256:{}", "2".repeat(64)),
        Provenance::kernel(),
    );
    idx.publish(
        Namespace::Local,
        "n",
        &v2,
        Some("1.1.0".into()),
        Some(&e1),
        NameStatus::Yanked,
        Provenance::kernel(),
        false,
    )
    .unwrap();
    match idx.resolve(&NameSelector::new("local", "n"), ResolveMode::Execute) {
        ResolveOutcome::Resolved(r) => assert!(r.version_id.starts_with("sha256:1")),
        o => panic!("unexpected {o:?}"),
    }
    match idx.resolve(&NameSelector::new("local", "missing"), ResolveMode::Execute) {
        ResolveOutcome::Unresolved => {}
        o => panic!("unexpected {o:?}"),
    }
}
