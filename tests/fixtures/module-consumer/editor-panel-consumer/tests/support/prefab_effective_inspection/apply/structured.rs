use super::super::structured::{
    SOCKET, authored, base_source, inherited_row, inspect_row, local, occurrence_target,
    pinned_row, projected_inheritance, row_property, structured_schema,
};
use super::*;

const ROW_KEY: &str = "primary";
const ROOT: &str = "base-root";

#[test]
fn a_registered_panel_applies_a_structured_override_from_a_nonactive_scene() {
    apply_row(EditorDocumentKind::Scene, SCENE);
}

#[test]
fn a_registered_panel_applies_a_structured_override_from_a_nonactive_prefab() {
    apply_row(EditorDocumentKind::Prefab, PREFAB);
}

#[test]
fn a_registered_panel_refuses_a_structured_claim_with_one_sibling_owned_reference() {
    let mut fixture = new_fixture(EditorDocumentKind::Scene, SCENE, "right");
    let original = fixture.stored_sources();
    let containing_before = basis(&fixture.app, fixture.containing);
    let destination_before = basis(&fixture.app, fixture.destination);
    let effective = pinned_row(
        ROW_KEY,
        authored(PrefabEntityTarget::source(id("left"))),
        authored(occurrence_target("right", SOCKET)),
    );
    let (inspection, address) = inspect(&mut fixture, "left", &effective, true);
    assert_eq!(
        fixture.prepare(inspection, address, fixture.destination),
        Outcome::Rejected {
            inspection: Some(inspection),
            reason: Rejection::InvalidProperty,
        }
    );
    expect_phase(&mut fixture.app, inspection, Phase::Ready);
    assert!(
        frame(&fixture.app)
            .prefab_inspection()
            .apply_destination
            .is_none()
    );
    cancel(&mut fixture.panel, &mut fixture.app, inspection);
    fixture.assert_containing(true, (false, 0, 0));
    fixture.assert_destination_history((false, 0, 0));
    assert_eq!(basis(&fixture.app, fixture.containing), containing_before);
    assert_eq!(basis(&fixture.app, fixture.destination), destination_before);
    assert_eq!(fixture.stored_sources(), original);
    assert!(fixture.app.close());
}

fn apply_row(kind: EditorDocumentKind, source: &'static str) {
    let mut fixture = new_fixture(kind, source, "left");
    let containing = fixture.containing;
    let destination = fixture.destination;
    let original = fixture.stored_sources();
    let containing_before = basis(&fixture.app, containing);
    let destination_before = basis(&fixture.app, destination);
    let initial_row = inherited_row(INITIAL_SCORE, local(ROOT), local(SOCKET));
    let localized = pinned_row(
        ROW_KEY,
        authored(PrefabEntityTarget::source(id(ROOT))),
        authored(PrefabEntityTarget::source(id(SOCKET))),
    );
    let expected = row_patch(localized.clone());
    let inverse = row_patch(initial_row.clone());
    assert_effective(
        &mut fixture,
        "right",
        &projected_inheritance("right", INITIAL_SCORE),
        false,
    );

    let inspection = review(&mut fixture, Some((&expected, &inverse, &localized)));
    fixture.assert_containing(true, (false, 0, 0));
    fixture.assert_destination_history((false, 0, 0));
    assert_eq!(basis(&fixture.app, containing), containing_before);
    assert_eq!(basis(&fixture.app, destination), destination_before);
    assert_eq!(fixture.stored_sources(), original);

    let published = fixture.commit_destination(inspection, true);
    assert_ne!(published.revision, destination_before.revision);
    fixture.require_destination_save(inspection, published);
    fixture.assert_containing(true, (false, 0, 0));
    assert_destination(&mut fixture, &localized, (true, 1, 0));
    assert_eq!(fixture.stored_sources(), original);

    save(&mut fixture.app, destination);
    let checkpoint = fixture.stored_sources();
    assert_eq!(checkpoint.0, original.0);
    assert_ne!(checkpoint.1, original.1);
    let mut expected_source = decode_prefab(&original.1);
    expected_source.entities[0]
        .components
        .get_mut(&ComponentTypeId::new(COMPONENT))
        .unwrap()
        .value_mut()
        .set_path(
            &ComponentFieldPath::from_fields(["wire_routes"]),
            ComponentValue::List(vec![localized.clone()]),
        )
        .unwrap();
    assert_eq!(decode_prefab(&checkpoint.1), expected_source);
    assert_eq!(basis(&fixture.app, containing), containing_before);
    fixture.assert_containing(true, (false, 0, 0));

    let (inspection, published) = if kind == EditorDocumentKind::Prefab {
        assert_eq!(
            fixture
                .panel
                .dispatch(&mut fixture.app, |_| Action::Cancel { inspection }),
            Outcome::Accepted {
                inspection,
                phase: Phase::Terminal
            }
        );
        assert_eq!(
            frame(&fixture.app).prefab_inspection().result,
            Some(EditorPrefabInspectionResult::ApplyStopped {
                inspection,
                destination: published,
                failure: None,
                diagnostics: Default::default(),
            })
        );
        acknowledge(&mut fixture.panel, &mut fixture.app, inspection);
        assert_eq!(basis(&fixture.app, containing), containing_before);
        assert_eq!(fixture.stored_sources(), checkpoint);
        reopen(&mut fixture.app, containing);
        fixture.assert_containing(true, (false, 0, 0));
        let retry = review(&mut fixture, None);
        let reused = fixture.commit_destination(retry, false);
        assert_eq!(reused.revision, published.revision);
        (retry, reused)
    } else {
        (inspection, published)
    };
    fixture.finish(inspection, published);
    fixture.assert_containing(false, (true, 1, 0));
    assert_destination(&mut fixture, &localized, (false, 1, 0));
    assert_eq!(fixture.stored_sources(), checkpoint);

    replay(&mut fixture.app, containing, false);
    fixture.assert_containing(true, (false, 0, 1));
    assert_eq!(
        basis(&fixture.app, destination).revision,
        published.revision
    );
    let source_undo = basis(&fixture.app, containing);
    replay(&mut fixture.app, destination, false);
    assert_destination(&mut fixture, &initial_row, (true, 0, 1));
    assert_effective(&mut fixture, "left", &projected_row("left"), true);
    assert_effective(
        &mut fixture,
        "right",
        &projected_inheritance("right", INITIAL_SCORE),
        false,
    );
    replay(&mut fixture.app, destination, true);
    assert_eq!(basis(&fixture.app, containing), source_undo);
    assert_destination(&mut fixture, &localized, (false, 1, 0));
    replay(&mut fixture.app, containing, true);
    fixture.assert_containing(false, (true, 1, 0));
    assert_eq!(fixture.stored_sources(), checkpoint);

    fixture.save_containing(&checkpoint.1);
    reopen(&mut fixture.app, destination);
    reopen(&mut fixture.app, containing);
    fixture.assert_containing(false, (false, 0, 0));
    assert_destination(&mut fixture, &localized, (false, 0, 0));
    for anchor in ["left", "right"] {
        assert_effective(&mut fixture, anchor, &projected_row(anchor), false);
    }
    assert!(fixture.app.close());
}

fn new_fixture(kind: EditorDocumentKind, source: &'static str, second: &str) -> ApplyFixture {
    let project = text_project::TextProject::new();
    let sibling = PrefabInstance::root(
        AssetRef::path("inspection-base.prefab.json").unwrap(),
        base_key(),
        PrefabEntityTarget::source(id(ROOT)),
    );
    let cleared = with_claim_intents(
        &sibling,
        [(
            "note",
            PrefabPropertyIntent::Set(ComponentValue::String(INSTANCE_NOTE.into())),
        )],
    );
    let mut pinned = cleared.clone();
    let [PrefabComponentIntent::Properties { claims, .. }] =
        pinned.overrides.as_mut().unwrap().components.as_mut_slice()
    else {
        unreachable!();
    };
    claims.push(PrefabPropertyClaim {
        path: row_property(),
        intent: PrefabPropertyIntent::Set(pinned_row(
            ROW_KEY,
            authored(occurrence_target("left", ROOT)),
            authored(occurrence_target(second, SOCKET)),
        )),
    });
    write_containing_source(&project, kind, source, two_mount_source(&pinned, &sibling));
    for (path, bytes) in [
        (BASE, base_source(INITIAL_SCORE)),
        (
            "schema/authoring.bundle.json",
            structured_schema().to_json_string().unwrap(),
        ),
    ] {
        fs::write(project.root.join(path), bytes).unwrap();
    }
    ApplyFixture::open(project, kind, source, pinned, cleared, sibling)
}

fn review(
    fixture: &mut ApplyFixture,
    expected: Option<(&ScenePatchDocument, &ScenePatchDocument, &ComponentValue)>,
) -> EditorPrefabInspectionId {
    let (inspection, address) = inspect(fixture, "left", &projected_row("left"), true);
    fixture.prepare_apply_review(inspection, address);
    let containing_basis = basis(&fixture.app, fixture.containing);
    let destination_basis = basis(&fixture.app, fixture.destination);
    fixture.panel.observe(&mut fixture.app, |context| {
        let review = context.prefab_apply_review(inspection).unwrap();
        assert_eq!(review.destination, &destination_basis);
        assert_eq!(review.containing.document_kind(), fixture.kind);
        assert_eq!(review.containing.preview().basis(), &containing_basis);
        assert_eq!(
            review.containing.patch(),
            &instance_patch(&fixture.pinned, &fixture.cleared)
        );
        assert_eq!(
            review.containing.inverse(),
            &instance_patch(&fixture.cleared, &fixture.pinned)
        );
        assert_remaining_note(review.containing.preview());
        if let Some((patch, inverse, value)) = expected {
            let change = review.destination_edit.unwrap();
            assert_eq!(change.document_kind(), EditorDocumentKind::Prefab);
            assert_eq!(change.patch(), patch);
            assert_eq!(change.inverse(), inverse);
            let preview = change.preview();
            assert_eq!(preview.basis(), &destination_basis);
            assert_eq!(
                preview
                    .read_property(row_read(destination_basis.clone()))
                    .unwrap()
                    .value,
                EditorPropertyValue::Present(value.clone())
            );
        } else {
            assert!(review.destination_edit.is_none());
        }
    });
    inspection
}

fn inspect(
    fixture: &mut ApplyFixture,
    anchor: &str,
    value: &ComponentValue,
    pinned: bool,
) -> (EditorPrefabInspectionId, EditorPrefabPropertyAddress) {
    inspect_row(
        &mut fixture.panel,
        &mut fixture.app,
        fixture.containing,
        anchor,
        value,
        pinned,
    )
}

fn assert_effective(
    fixture: &mut ApplyFixture,
    anchor: &str,
    value: &ComponentValue,
    pinned: bool,
) {
    let (inspection, _) = inspect(fixture, anchor, value, pinned);
    cancel(&mut fixture.panel, &mut fixture.app, inspection);
}

fn assert_destination(
    fixture: &mut ApplyFixture,
    expected: &ComponentValue,
    history: (bool, usize, usize),
) {
    fixture.assert_destination_history(history);
    fixture.panel.observe(&mut fixture.app, |context| {
        let basis = context.document_query_basis(fixture.destination).unwrap();
        assert_eq!(
            context
                .read_document_property(row_read(basis))
                .unwrap()
                .value,
            EditorPropertyValue::Present(expected.clone())
        );
    });
}

fn row_read(basis: EditorDocumentQueryBasis) -> EditorPropertyRead {
    EditorPropertyRead {
        target: EditorPropertyTarget {
            basis,
            entity: id(ROOT),
            component: ComponentTypeId::new(COMPONENT),
            property: row_property(),
        },
        limits: EditorPropertyReadLimits::new(1024, 32, 4096).unwrap(),
    }
}

fn row_patch(value: ComponentValue) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::ReplaceProperty {
        entity: id(ROOT),
        component: ComponentTypeId::new(COMPONENT),
        component_version: ComponentSchemaVersion::ONE,
        property: row_property(),
        value,
    }])
}

fn projected_row(anchor: &str) -> ComponentValue {
    pinned_row(
        ROW_KEY,
        authored(PrefabEntityTarget::source(id(anchor))),
        authored(occurrence_target(anchor, SOCKET)),
    )
}

fn decode_prefab(bytes: &[u8]) -> PrefabDocument {
    PrefabDocumentCandidate::decode_json_bytes(bytes)
        .unwrap()
        .into_composition_source()
        .unwrap()
}
