//! Public source transactions retain declaration intent independently of effective layout.

#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, ComponentRegistryBuilder, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue,
};

use nara_scene::{
    AuthoringDocumentSource, PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate,
    PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceHop,
    SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
    ScenePlacement, ScenePlacementParent, SourceDocumentKey, SourceDocumentKind,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn q_key() -> SourceDocumentKey {
    SourceDocumentKey::named(
        "b5d6747b-b7a4-4fbc-a63e-29bd1cb48c55".parse().unwrap(),
        "Q",
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn nested(value: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("k")),
            expected_base: q_key(),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id(value)),
    }
}

fn parent() -> ScenePlacementParent {
    ScenePlacementParent::Entity(PrefabEntityTarget::source(id("k")))
}

fn p() -> PrefabDocument {
    let mut anchor = SceneEntityRecord::new(id("k"));
    anchor.prefab = Some(PrefabInstance::root(
        AssetRef::path("Q.prefab.json").unwrap(),
        q_key(),
        PrefabEntityTarget::source(id("q")),
    ));
    PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("b")),
                ScenePlacement::last(parent()),
            ),
            SceneEntityPlacement::root(id("z")),
        ],
        [
            anchor,
            SceneEntityRecord::new(id("b")).with_component(
                ComponentTypeId::new("missing.Opaque"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([("retained", ComponentValue::String("unchanged".into()))]),
                ),
            ),
            SceneEntityRecord::new(id("z")),
        ],
    )
}

fn session(document: &PrefabDocument) -> PrefabAuthoringSession {
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    let published = PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("P.prefab.json").unwrap()),
            &bundle,
            &[],
        )
        .unwrap()
        .publish();
    PrefabAuthoringSession::from_published_authoring_document(published)
}

#[test]
fn local_between_nested_siblings_keeps_ownership_opaque_records_and_exact_history() {
    let original = p();
    let mut session = session(&original);
    let manifest = session.unavailable_record_manifest().unwrap().clone();
    let content = session.content_state();
    let patch = ScenePatchDocument::set_placement(
        &original.placements,
        PrefabEntityTarget::source(id("b")),
        ScenePlacement::before(parent(), nested("d2")),
    );
    let report = session.apply_authoring_patch(&patch);
    assert!(report.applied, "{:#?}", report.diagnostics);
    let placed = session.document().clone();
    assert_eq!(placed.entities, original.entities);
    assert_eq!(placed.placements[0], original.placements[0]);
    assert_eq!(placed.placements[2], original.placements[2]);
    assert_eq!(
        placed.placements[1].target,
        PrefabEntityTarget::source(id("b"))
    );
    assert_eq!(
        placed.placements[1].placement,
        ScenePlacement::before(parent(), nested("d2"))
    );
    assert_eq!(session.history_status().undo_depth, 1);

    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &original);
    assert_eq!(session.content_state(), content);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &placed);

    let reopened = self::session(&placed);
    assert_eq!(reopened.document(), &placed);
    assert_eq!(
        reopened.unavailable_record_manifest(),
        session.unavailable_record_manifest()
    );
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    let ron = PrefabDocumentCandidate::decode_ron_str(&placed.to_ron_string().unwrap())
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("P.prefab.json").unwrap()),
            &bundle,
            &[],
        )
        .unwrap()
        .publish();
    assert_eq!(ron.document(), &placed);
}

#[test]
fn inherited_revert_restores_its_original_declaration_ordinal_on_undo() {
    let mut original = p();
    original.placements.insert(
        1,
        SceneEntityPlacement::new(nested("d1"), ScenePlacement::first(parent())),
    );
    let mut session = session(&original);
    let patch = ScenePatchDocument::revert_placement(&original.placements, &nested("d1")).unwrap();
    let report = session.apply_authoring_patch(&patch);
    assert!(report.applied, "{:#?}", report.diagnostics);
    assert!(
        !session
            .document()
            .placements
            .iter()
            .any(|entry| entry.target == nested("d1"))
    );
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &original);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document().placements, p().placements);
    assert!(
        ScenePatchDocument::revert_placement(
            &original.placements,
            &PrefabEntityTarget::source(id("b"))
        )
        .is_err()
    );
}

#[test]
fn stale_vector_and_missing_local_placement_refuse_without_publication() {
    let original = p();
    let mut session = session(&original);
    let revision = session.revision();
    let content = session.content_state();
    let history = session.history_status();
    let mut reordered = original.placements.clone();
    reordered.swap(0, 2);
    for patch in [
        ScenePatchDocument::new([ScenePatchOperation::SetPlacements {
            expected: reordered,
            value: original.placements.clone(),
        }]),
        ScenePatchDocument::new([ScenePatchOperation::SetPlacements {
            expected: original.placements.clone(),
            value: original
                .placements
                .iter()
                .filter(|entry| entry.target != PrefabEntityTarget::source(id("b")))
                .cloned()
                .collect(),
        }]),
    ] {
        let report = session.apply_authoring_patch(&patch);
        assert!(!report.applied);
        assert_eq!(session.document(), &original);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.content_state(), content);
        assert_eq!(session.history_status(), history);
    }
}

#[test]
fn native_record_and_placement_batches_validate_only_the_complete_candidate() {
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let original = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("a"))]);
    let mut session = SceneAuthoringSession::new(original.clone());
    let mut placements = original.placements.clone();
    placements.push(SceneEntityPlacement::root(id("b")));
    let added = session.apply_patch(
        &ScenePatchDocument::new([
            ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(id("b")),
            },
            ScenePatchOperation::SetPlacements {
                expected: original.placements.clone(),
                value: placements.clone(),
            },
        ]),
        &registry,
    );
    assert!(added.applied, "{:#?}", added.diagnostics);
    let complete = session.document().clone();
    assert!(session.undo(&registry).applied);
    assert_eq!(session.document(), &original);
    assert!(session.redo(&registry).applied);
    assert_eq!(session.document(), &complete);

    let removed = session.apply_patch(
        &ScenePatchDocument::new([
            ScenePatchOperation::RemoveEntity { entity: id("b") },
            ScenePatchOperation::SetPlacements {
                expected: placements,
                value: original.placements.clone(),
            },
        ]),
        &registry,
    );
    assert!(removed.applied, "{:#?}", removed.diagnostics);
    assert_eq!(session.document(), &original);
    assert!(session.undo(&registry).applied);
    assert_eq!(session.document(), &complete);
}

#[test]
fn public_native_and_authoring_transactions_reject_local_cycles_without_losing_history() {
    use nara_scene::{SceneDocumentCandidate, ScenePlacementSlot};
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let original = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("a")),
        SceneEntityRecord::new(id("b")),
    ]);
    let target = |name| PrefabEntityTarget::source(id(name));
    let invalid = [
        vec![
            SceneEntityPlacement::new(
                target("a"),
                ScenePlacement::last(ScenePlacementParent::Entity(target("a"))),
            ),
            SceneEntityPlacement::root(id("b")),
        ],
        vec![
            SceneEntityPlacement::new(
                target("a"),
                ScenePlacement::last(ScenePlacementParent::Entity(target("b"))),
            ),
            SceneEntityPlacement::new(
                target("b"),
                ScenePlacement::last(ScenePlacementParent::Entity(target("a"))),
            ),
        ],
        vec![
            SceneEntityPlacement::new(
                target("a"),
                ScenePlacement {
                    parent: ScenePlacementParent::Root,
                    slot: ScenePlacementSlot::Before(target("b")),
                },
            ),
            SceneEntityPlacement::new(
                target("b"),
                ScenePlacement {
                    parent: ScenePlacementParent::Root,
                    slot: ScenePlacementSlot::Before(target("a")),
                },
            ),
        ],
    ];
    for value in invalid {
        let mut native = SceneAuthoringSession::new(original.clone());
        let bundle = AuthoringSchemaBundle::new([]).unwrap();
        let published =
            SceneDocumentCandidate::decode_json_str(&original.to_json_string().unwrap())
                .unwrap()
                .admit_authoring(
                    AuthoringDocumentSource::new(AssetRef::path("local.scene.json").unwrap()),
                    &bundle,
                    &[],
                )
                .unwrap()
                .publish();
        let mut authoring = SceneAuthoringSession::from_published_authoring_document(published);
        let patch = ScenePatchDocument::new([ScenePatchOperation::SetPlacements {
            expected: original.placements.clone(),
            value,
        }]);
        for (session, raw) in [(&mut native, false), (&mut authoring, true)] {
            let revision = session.revision();
            let content = session.content_state();
            let history = session.history_status();
            let rejected = if raw {
                session.apply_authoring_patch(&patch)
            } else {
                session.apply_patch(&patch, &registry)
            };
            assert!(!rejected.applied);
            assert_eq!(session.document(), &original);
            assert_eq!(session.revision(), revision);
            assert_eq!(session.content_state(), content);
            assert_eq!(session.history_status(), history);
            let valid = ScenePatchDocument::set_placement(
                &original.placements,
                target("b"),
                ScenePlacement::first(ScenePlacementParent::Root),
            );
            let accepted = if raw {
                session.apply_authoring_patch(&valid)
            } else {
                session.apply_patch(&valid, &registry)
            };
            assert!(accepted.applied, "{:#?}", accepted.diagnostics);
            let undone = if raw {
                session.undo_authoring()
            } else {
                session.undo(&registry)
            };
            assert!(undone.applied, "{:#?}", undone.diagnostics);
            assert_eq!(session.document(), &original);
            let redone = if raw {
                session.redo_authoring()
            } else {
                session.redo(&registry)
            };
            assert!(redone.applied, "{:#?}", redone.diagnostics);
        }
    }
}

#[test]
fn routed_placement_intent_cannot_hide_provable_local_cycles_or_membership_conflicts() {
    use nara_scene::{SceneDocumentCandidate, ScenePlacementSlot};

    let target = |name| PrefabEntityTarget::source(id(name));
    let mut anchor = SceneEntityRecord::new(id("k"));
    anchor.prefab = Some(PrefabInstance::forest(
        AssetRef::path("Q.prefab.json").unwrap(),
    ));
    let original = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("a")),
        SceneEntityRecord::new(id("b")),
        anchor,
    ]);
    let declarations = |a, b| {
        vec![
            SceneEntityPlacement::new(target("a"), a),
            SceneEntityPlacement::new(target("b"), b),
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::new(
                nested("q"),
                ScenePlacement::last(ScenePlacementParent::Root),
            ),
        ]
    };
    let at = |parent, slot| ScenePlacement { parent, slot };
    let under = |name| ScenePlacementParent::Entity(target(name));
    let root = || ScenePlacement::last(ScenePlacementParent::Root);
    let unknown_parent = || ScenePlacementParent::Entity(nested("q"));
    let invalid = [
        declarations(ScenePlacement::last(under("a")), root()),
        declarations(
            ScenePlacement::last(under("b")),
            ScenePlacement::last(under("a")),
        ),
        declarations(
            at(
                ScenePlacementParent::Root,
                ScenePlacementSlot::Before(target("b")),
            ),
            at(
                ScenePlacementParent::Root,
                ScenePlacementSlot::Before(target("a")),
            ),
        ),
        declarations(
            at(under("k"), ScenePlacementSlot::Before(target("b"))),
            root(),
        ),
        declarations(
            at(unknown_parent(), ScenePlacementSlot::Before(target("b"))),
            root(),
        ),
        declarations(
            at(
                ScenePlacementParent::Root,
                ScenePlacementSlot::After(target("b")),
            ),
            ScenePlacement::last(unknown_parent()),
        ),
        declarations(
            at(unknown_parent(), ScenePlacementSlot::Before(target("b"))),
            at(unknown_parent(), ScenePlacementSlot::After(target("a"))),
        ),
        declarations(
            at(unknown_parent(), ScenePlacementSlot::Before(target("a"))),
            root(),
        ),
    ];
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    for value in invalid {
        let published =
            SceneDocumentCandidate::decode_json_str(&original.to_json_string().unwrap())
                .unwrap()
                .admit_authoring(
                    AuthoringDocumentSource::new(AssetRef::path("mixed.scene.json").unwrap()),
                    &bundle,
                    &[],
                )
                .unwrap()
                .publish();
        let mut authored = SceneAuthoringSession::from_published_authoring_document(published);
        let mut native = SceneAuthoringSession::new(original.clone());
        let patch = ScenePatchDocument::new([ScenePatchOperation::SetPlacements {
            expected: original.placements.clone(),
            value: value.clone(),
        }]);
        for (session, authoring) in [(&mut authored, true), (&mut native, false)] {
            let before = (
                session.revision(),
                session.content_state(),
                session.history_status(),
            );
            let rejected = if authoring {
                session.apply_authoring_patch(&patch)
            } else {
                session.apply_patch(&patch, &registry)
            };
            assert!(!rejected.applied, "{value:?}");
            assert_eq!(session.document(), &original);
            assert_eq!(
                (
                    session.revision(),
                    session.content_state(),
                    session.history_status()
                ),
                before
            );
            assert!(
                rejected.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "scene.patch-local-layout-invalid"
                }),
                "{:#?}",
                rejected.diagnostics
            );
        }

        let prefab = PrefabDocument::new(original.placements.clone(), original.entities.clone());
        let mut authored = session(&prefab);
        let mut native = PrefabAuthoringSession::new(prefab.clone());
        for (session, authoring) in [(&mut authored, true), (&mut native, false)] {
            let before = (
                session.revision(),
                session.content_state(),
                session.history_status(),
            );
            let rejected = if authoring {
                session.apply_authoring_patch(&patch)
            } else {
                session.apply_patch(&patch, &registry)
            };
            assert!(!rejected.applied, "{value:?}");
            assert_eq!(session.document(), &prefab);
            assert_eq!(
                (
                    session.revision(),
                    session.content_state(),
                    session.history_status()
                ),
                before
            );
            assert!(
                rejected.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "scene.patch-local-layout-invalid"
                }),
                "{:#?}",
                rejected.diagnostics
            );
        }
    }
}

#[test]
fn unknown_projected_membership_keeps_valid_local_constraints_editable_and_compilable() {
    use nara_scene::{InMemoryPrefabSourceResolver, ScenePlacementSlot};

    let mut anchor = SceneEntityRecord::new(id("k"));
    anchor.prefab = Some(PrefabInstance::forest(
        AssetRef::path("Q.prefab.json").unwrap(),
    ));
    let original = PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("a")),
        SceneEntityRecord::new(id("b")),
        anchor,
    ]);
    let value = vec![
        SceneEntityPlacement::new(
            PrefabEntityTarget::source(id("a")),
            ScenePlacement {
                parent: ScenePlacementParent::Entity(nested("q")),
                slot: ScenePlacementSlot::Before(PrefabEntityTarget::source(id("b"))),
            },
        ),
        SceneEntityPlacement::new(
            PrefabEntityTarget::source(id("b")),
            ScenePlacement::last(ScenePlacementParent::Entity(nested("q"))),
        ),
        SceneEntityPlacement::root(id("k")),
    ];
    let mut session = session(&original);
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetPlacements {
        expected: original.placements.clone(),
        value: value.clone(),
    }]);
    let accepted = session.apply_authoring_patch(&patch);
    assert!(accepted.applied, "{:#?}", accepted.diagnostics);
    assert_eq!(session.document().placements, value);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &original);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document().placements, value);
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(
        AssetRef::path("Q.prefab.json").unwrap(),
        q_key(),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("q"))]),
    );
    let report = session.document().compile(&registry, &sources);
    assert!(
        !report.diagnostics.has_errors(),
        "{:#?}",
        report.diagnostics
    );
    let compiled = report.candidate.unwrap();
    let parent = compiled
        .entities()
        .iter()
        .find(|entity| entity.children() == [id("a"), id("b")]);
    assert!(
        parent.is_some(),
        "local sibling intent must resolve under the actual projected parent"
    );
    assert_eq!(compiled.entities().len(), 4);
}
