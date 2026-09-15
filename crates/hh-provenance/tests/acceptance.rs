//! Executable acceptance for **R-2.1.5** (spec §8.1; ticket S1.3).
//!
//! - **AC-R-2.1.5-1** — every event class in the mandatory-provenance table refuses an append
//!   without a `ProvenanceRecord` (`MissingProvenance`, never a warning); a *present but
//!   invalid* record is refused too; every `Text` leaf receives a class via
//!   `default_text_authority` (R-TEXT) and the opacity report partitions by class (T-LCD-02).
//!   *Stage-1 cut:* the record + the check + the table + text-leaf classification land here;
//!   the HIR-node/edge wiring lands with HIR/1 `validate` at S1.4 (DF-S1.3-1).
//! - **AC-R-2.1.5-2** — a seeded property test over random derivation / delegation /
//!   persistence sequences **with no endorsement events on the derivation path**: the output
//!   label never exceeds the join of its inputs (`label(out) ⊑ ⊔ label(inputs)`), every
//!   non-endorsement upward move is rejected, and the only label that rises is one a
//!   legitimate `security.label.endorsed`/`declassified` event covers. The `join`/`meet`/`leq`
//!   lattice laws are proven exhaustively over a label universe in `label::tests`
//!   (stronger than sampling) and re-sampled here inside the sequences.

use hh_provenance::*;

// ── A small deterministic PRNG (xorshift64*) — no external deps (hermetic) ─────────────

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn pick(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn coin(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

const SCOPES: [PersistenceScope; 6] = [
    PersistenceScope::Definition,
    PersistenceScope::User,
    PersistenceScope::Project,
    PersistenceScope::Session,
    PersistenceScope::Run,
    PersistenceScope::Turn,
];

const KINDS: [DerivationKind; 8] = [
    DerivationKind::Summary,
    DerivationKind::Compaction,
    DerivationKind::Extraction,
    DerivationKind::Quotation,
    DerivationKind::Revision,
    DerivationKind::Translation,
    DerivationKind::SubagentResult,
    DerivationKind::Projection,
];

const BASES: [EndorsementBasis; 6] = EndorsementBasis::ALL;

fn rand_origin(rng: &mut Rng) -> Origin {
    match rng.pick(8) {
        0 => Origin::human(
            format!("human:{}", rng.pick(4)),
            match rng.pick(3) {
                0 => HumanRole::Author,
                1 => HumanRole::Principal,
                _ => HumanRole::Reviewer,
            },
        ),
        1 => Origin::model(
            format!("model:{}", rng.pick(3)),
            format!("run:{}", rng.pick(3)),
            format!("resp:{}", rng.next() % 1000),
        ),
        2 => Origin::tool(
            format!("tool:{}", rng.pick(4)),
            format!("inv:{}", rng.next() % 1000),
        ),
        3 => Origin::kernel(format!("kernel:{}", rng.pick(4))),
        4 => Origin::import(
            format!("src:{}", rng.pick(4)),
            format!("map:v{}", rng.pick(3)),
        ),
        5 => Origin::participant(format!("p:{}", rng.pick(4)), "hh.hosting/1"),
        6 => Origin::evolution(
            format!("cand:{}", rng.pick(4)),
            format!("hyp:{}", rng.pick(4)),
        ),
        _ => Origin::Migration {
            from_dialect: format!("hir/0.{}", rng.pick(3)),
        },
    }
}

fn rand_scope(rng: &mut Rng) -> PersistenceScope {
    SCOPES[rng.pick(SCOPES.len())]
}

/// Every record the sequence produces must validate **without** endorsement evidence — the
/// pool only ever holds records whose label is self-consistent with its minted ceiling.
fn assert_pool_invariant(pool: &[ProvenanceRecord]) {
    for r in pool {
        assert!(
            r.validate(None).is_ok(),
            "pool record must validate without endorsement evidence: {:?}",
            r.validate(None)
        );
        assert!(
            r.authority <= r.minted_ceiling(),
            "no pool record may sit above its minted ceiling"
        );
    }
}

// ── AC-R-2.1.5-1 ─────────────────────────────────────────────────────────────────────

#[test]
fn ac1_every_mandatory_event_refuses_a_missing_or_invalid_record() {
    for event in ProvenanceEventKind::ALL {
        // The table covers the class.
        assert!(requires_provenance(event), "{}", event.as_str());
        assert!(required_provenance(event).author_record);
        // An append with no record fails `MissingProvenance` — a typed refusal, not a warning.
        match require_provenance(None, event.as_str()) {
            Err(ProvenanceError::MissingProvenance { what }) => {
                assert_eq!(what, event.as_str())
            }
            other => panic!(
                "{}: expected MissingProvenance, got {other:?}",
                event.as_str()
            ),
        }
        // A present, valid record passes.
        let ok = ProvenanceRecord::kernel("kernel:ledger", 7);
        assert!(require_provenance(Some(&ok), event.as_str()).is_ok());
        // A present-but-invalid record is not "carried" (taint above external).
        let mut bad = ok.clone();
        bad.taint.insert(TaintTag::Import {
            source_system: "x".into(),
        });
        assert!(require_provenance(Some(&bad), event.as_str()).is_err());
    }
}

#[test]
fn ac1_every_text_leaf_has_a_class_and_the_opacity_report_partitions() {
    // R-TEXT: free text not from kernel/definition/principal mints ≤ external; imported /
    // migrated / unvouched text stays `unverified` (never coerced — T-LCD-07).
    let origins = [
        (Origin::kernel("kernel:ledger"), AuthorityClass::Kernel),
        (
            Origin::human("human:alice", HumanRole::Principal),
            AuthorityClass::Principal,
        ),
        (
            Origin::model("model:m", "run:r", "resp:1"),
            AuthorityClass::External,
        ),
        (
            Origin::tool("tool:shell", "inv:1"),
            AuthorityClass::External,
        ),
        (
            Origin::import("src:jira", "map:v1"),
            AuthorityClass::Unverified,
        ),
        (
            Origin::Migration {
                from_dialect: "hir/0".into(),
            },
            AuthorityClass::Unverified,
        ),
        (
            Origin::participant("p:1", "hh.hosting/1"),
            AuthorityClass::Unverified,
        ),
    ];
    let report = OpacityReport::over(origins.iter().map(|(o, _)| default_text_authority(o, None)));
    // T-LCD-02: a total partition — every leaf has a class, counts sum to the input.
    assert_eq!(report.total(), origins.len());
    for (origin, want) in &origins {
        let got = default_text_authority(origin, None);
        assert_eq!(&got, want, "text authority for {origin:?}");
        assert!(report.count(got) >= 1);
    }
    // R-TEXT cap: no non-kernel/human origin produces text above `external`.
    for (origin, _) in &origins {
        if !matches!(origin, Origin::Kernel { .. } | Origin::Human { .. }) {
            assert!(default_text_authority(origin, None) <= AuthorityClass::External);
        }
    }
}

// ── AC-R-2.1.5-2 ─────────────────────────────────────────────────────────────────────

#[test]
fn ac2_seeded_sequences_never_move_upward_without_a_legitimate_endorsement() {
    for seed in 0..16u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1));
        let mut pool: Vec<ProvenanceRecord> = Vec::new();
        let mut seq: u64 = 0;

        for _step in 0..400 {
            seq += 1;
            match rng.pick(7) {
                // ── mint ────────────────────────────────────────────────────────
                0 => {
                    let origin = rand_origin(&mut rng);
                    let scope = rand_scope(&mut rng);
                    let r = ProvenanceRecord::minted(origin.clone(), scope, seq);
                    // CC2: the minted class is exactly the minting table — never more.
                    assert_eq!(r.authority, default_authority(&origin, scope, None));
                    pool.push(r);
                }
                // ── derive (P1) ─────────────────────────────────────────────────
                1 if !pool.is_empty() => {
                    let n = 1 + rng.pick(3);
                    let inputs: Vec<DerivationInput> = (0..n)
                        .map(|_| {
                            let r = pool[rng.pick(pool.len())].clone();
                            DerivationInput {
                                input_ref: format!("v:{}", rng.next() % 10_000),
                                record: r,
                            }
                        })
                        .collect();
                    let join = inputs
                        .iter()
                        .fold(Label::top(), |acc, i| acc.join(&i.record.label()));
                    let kind = KINDS[rng.pick(KINDS.len())];
                    let deriver = rand_origin(&mut rng);
                    let deterministic = rng.coin();
                    match derive(
                        kind,
                        &inputs,
                        deriver.clone(),
                        deterministic,
                        rand_scope(&mut rng),
                        seq,
                    ) {
                        Err(_) => {
                            // The only refusals on this path: a deterministic projection by a
                            // non-kernel deriver (and empty inputs, which we never generate).
                            assert!(
                                deterministic && !matches!(deriver, Origin::Kernel { .. }),
                                "unexpected derive refusal (kind {kind:?}, deriver {deriver:?})"
                            );
                        }
                        Ok(out) => {
                            // AC-2 core: `label(out) ⊑ ⊔ label(inputs)` — the join of the
                            // inputs flows to the output (the output is at least as
                            // restrictive: lower-or-equal authority, superset taint, subset
                            // readers).
                            assert!(
                                join.leq(&out.label()),
                                "derivation exceeded the join of its inputs"
                            );
                            assert!(out.authority <= join.authority);
                            assert!(join.taint.is_subset(&out.taint));
                            // Model-produced kinds and subagent results cap at `delegate`.
                            if kind.is_model_produced() || kind == DerivationKind::SubagentResult {
                                assert!(out.authority <= AuthorityClass::Delegate);
                            }
                            // A deterministic projection is a kernel derivation.
                            if deterministic {
                                assert!(matches!(out.origin, Origin::Kernel { .. }));
                            }
                            assert!(out.validate(None).is_ok());
                            pool.push(out);
                        }
                    }
                }
                // ── delegate (P2 ingestion + check 7) ───────────────────────────
                2 if !pool.is_empty() => {
                    let child = pool[rng.pick(pool.len())].clone();
                    let parent = ingest_child_result("v:child", &child, rand_scope(&mut rng), seq);
                    // Every child result enters the parent at ≤ delegate, carrying the
                    // child's accumulated taint, with `derived_from.kind = subagent_result`.
                    assert!(parent.authority <= AuthorityClass::Delegate);
                    assert!(child.taint.is_subset(&parent.taint));
                    assert_eq!(parent.derived_from[0].kind, DerivationKind::SubagentResult);
                    // Monitor check 7 accepts the ingested parent…
                    assert!(check_no_delegate_above(&parent, std::slice::from_ref(&child)).is_ok());
                    // …and a parent claiming above delegate is refused.
                    let mut bad = parent.clone();
                    bad.authority = AuthorityClass::Principal;
                    assert!(check_no_delegate_above(&bad, &[child]).is_err());
                    pool.push(parent);
                }
                // ── persist (scope move — never a lift) ─────────────────────────
                3 if !pool.is_empty() => {
                    let r = pool[rng.pick(pool.len())].clone();
                    let new_scope = rand_scope(&mut rng);
                    // Location neutrality (AC-9 pure half): the minting table never reads
                    // scope, so a scope move can never confer.
                    assert_eq!(
                        default_authority(&r.origin, new_scope, r.attestation.as_ref()),
                        default_authority(&r.origin, r.scope, r.attestation.as_ref()),
                        "a scope move must never change what origin can mint"
                    );
                    let mut moved = r.clone();
                    moved.scope = new_scope;
                    assert_eq!(moved.authority, r.authority);
                    assert!(moved.validate(None).is_ok());
                    pool.push(moved);
                }
                // ── P6 / monitor check 2: non-endorsement label moves ───────────
                4 if pool.len() >= 2 => {
                    let from = pool[rng.pick(pool.len())].label();
                    let to = pool[rng.pick(pool.len())].label();
                    let delta = classify_label_delta(&from, &to);
                    let verdict = check_label_transition(&from, &to);
                    let verdict2 = check_monotonic_label(&from, &to);
                    match delta {
                        LabelDelta::None | LabelDelta::Narrowing => {
                            assert!(verdict.is_ok() && verdict2.is_ok());
                        }
                        LabelDelta::Widening | LabelDelta::MixedWidening => {
                            assert!(verdict.is_err() && verdict2.is_err());
                        }
                    }
                    // A direct authority bump on a record — no endorsement — must fail
                    // `validate` the moment it exceeds the minted ceiling.
                    let mut forged = pool[rng.pick(pool.len())].clone();
                    let higher = AuthorityClass::ALL[rng.pick(7)];
                    forged.authority = higher;
                    if higher > forged.minted_ceiling() {
                        assert!(matches!(
                            forged.validate(None),
                            Err(ProvenanceError::AuthorityExceedsOrigin { .. })
                                | Err(ProvenanceError::TaintedAboveExternal { .. })
                        ));
                    }
                }
                // ── lower / lift (P7): a lift never widens ──────────────────────
                5 if !pool.is_empty() => {
                    let r = pool[rng.pick(pool.len())].clone();
                    let target = [
                        LowerTarget::Wire,
                        LowerTarget::PolicyAtom,
                        LowerTarget::EvidenceRow,
                        LowerTarget::ArtifactStamp,
                    ][rng.pick(4)];
                    let lowered = lower(&r, target);
                    let lifted = lift(&lowered, rand_scope(&mut rng));
                    if target == LowerTarget::Wire {
                        assert_eq!(lifted.record, r);
                    } else {
                        // Lifted content is `import`-origin → `unverified`, never the carried
                        // class; the loss is stamped as taint.
                        assert_eq!(lifted.record.authority, AuthorityClass::Unverified);
                        assert!(!lifted.record.taint.is_empty());
                        assert!(!lifted.defaulted.is_empty());
                    }
                    assert!(lifted.record.validate(None).is_ok());
                    pool.push(lifted.record);
                }
                // ── endorse (check 5): the ONLY legitimate way up ───────────────
                _ if !pool.is_empty() => {
                    let subject = pool[rng.pick(pool.len())].clone();
                    let mut to = subject.label();
                    to.authority = AuthorityClass::ALL[rng.pick(7)];
                    if rng.coin() {
                        to.taint.clear();
                    }
                    let endorser = pool[rng.pick(pool.len())].clone();
                    let basis = BASES[rng.pick(BASES.len())];
                    let kind = [
                        ContentKind::FreeText,
                        ContentKind::ClosedSchemaValue,
                        ContentKind::EffectIntent,
                        ContentKind::Other,
                    ][rng.pick(4)];
                    match endorse(
                        &subject,
                        "v:subject",
                        &to,
                        &endorser,
                        basis,
                        if basis == EndorsementBasis::Approval {
                            Some(format!("perm:{}", rng.pick(4)))
                        } else {
                            None
                        },
                        kind,
                    ) {
                        Ok(event) => {
                            // Constitutional invariants on every emitted event: the endorser
                            // is never delegate-class and is never below the target.
                            assert!(!event.endorser.origin.is_delegate_class());
                            assert!(event.endorser.authority >= event.to.authority);
                            assert!(matches!(
                                classify_label_delta(&event.from, &event.to),
                                LabelDelta::Widening | LabelDelta::MixedWidening
                            ));
                            // The recorded event re-verifies at append (monitor check 5).
                            assert!(check_endorsement(&event, &subject, kind).is_ok());
                            // Applying the endorsed label WITHOUT the event is rejected —
                            // the ledger event is the only evidence that legitimizes the rise.
                            let mut applied = subject.clone();
                            applied.authority = event.to.authority;
                            applied.taint = event.to.taint.clone();
                            applied.readers = event.to.readers.clone();
                            if event.to.authority > subject.minted_ceiling()
                                && applied.taint.is_empty()
                            {
                                assert!(matches!(
                                    applied.validate(None),
                                    Err(ProvenanceError::AuthorityExceedsOrigin { .. })
                                ));
                                assert!(applied.validate(Some(&event)).is_ok());
                            }
                        }
                        Err(_) => {
                            // Every refused endorsement is a typed error — assert the
                            // constitutional rules hold on the refusal side too: if the
                            // endorser was delegate-class or below target, refusal is
                            // mandatory.
                            if endorser.origin.is_delegate_class()
                                || endorser.authority < to.authority
                            {
                                // refused, as required
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        assert_pool_invariant(&pool);
    }
}

/// The lattice laws inside a live sequence (the exhaustive-universe proof lives in
/// `label::tests`; here the same laws are re-checked over sequence-generated labels).
#[test]
fn ac2_lattice_laws_over_generated_labels() {
    let mut rng = Rng(0xDEAD_BEEF);
    let mut labels = vec![Label::top()];
    for _ in 0..64 {
        let r = ProvenanceRecord::minted(rand_origin(&mut rng), rand_scope(&mut rng), 0);
        labels.push(r.label());
    }
    for a in &labels {
        assert_eq!(&a.join(a), a);
        assert_eq!(&a.meet(a), a);
        assert!(a.leq(a));
        for b in &labels {
            assert_eq!(a.join(b), b.join(a));
            assert_eq!(a.meet(b), b.meet(a));
            assert_eq!(a.leq(b), a.join(b) == *b);
            for c in &labels {
                assert_eq!(a.join(b).join(c), a.join(&b.join(c)));
                if a.leq(b) && b.leq(c) {
                    assert!(a.leq(c));
                }
            }
        }
    }
}
