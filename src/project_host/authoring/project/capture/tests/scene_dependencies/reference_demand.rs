//! Deferred references keep actual source bindings outside the current working-copy closure.

use nara_asset::AssetReferenceDemand;
use nara_reflect::ComponentValueShape;

use super::*;

fn write_demand_schema(project: &TestAuthoringProject) {
    let schema = ComponentSchema::new(component(), "SceneLinks", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::optional(
                ComponentFieldId::new("image"),
                "Future Scene",
                ComponentFieldPath::from_fields(["image"]),
                ComponentValueShape::asset_ref(AssetReferenceDemand::Deferred),
            )
            .with_capability(ComponentCapability::AssetRef)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional(
                ComponentFieldId::new("rules"),
                "Current Scene Data",
                ComponentFieldPath::from_fields(["rules"]),
                ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate),
            )
            .with_capability(ComponentCapability::AssetRef)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    fs::write(
        project.root.join("schemas/authoring.bundle.json"),
        bundle.to_json_string().unwrap(),
    )
    .unwrap();
}

fn demand_links(targets: impl IntoIterator<Item = (&'static str, AssetRef)>) -> SceneDocument {
    SceneDocument::from_ordered_roots(targets.into_iter().enumerate().map(
        |(index, (field, reference))| {
            SceneEntityRecord::new(SceneEntityId::new(format!("link-{index}")).unwrap())
                .with_component(
                    component(),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::map([(field, encode_asset_reference_value(&reference))]),
                    )
                    .with_owner(owner()),
                )
        },
    ))
}

fn write_scene(project: &TestAuthoringProject, path: &str, document: &SceneDocument) {
    fs::write(project.root.join(path), document.to_json_string().unwrap()).unwrap();
}

fn edit_link(session: &mut SceneAuthoringSession, field: &str, target: u8) {
    assert!(
        session
            .apply_authoring_patch(&ScenePatchDocument::new([ScenePatchOperation::SetField {
                entity: SceneEntityId::new("link-0").unwrap(),
                component: component(),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new(field),
                value: encode_asset_reference_value(&product(target)),
            }]))
            .applied
    );
}

fn capture_with_scenes(
    snapshot: &AuthoringProjectSessionSnapshot,
    source: &AuthoringDocumentSourceFact,
    root: &SceneAuthoringSession,
    sessions: Vec<AuthoringSceneSessionSnapshot>,
) -> Result<AuthoringDocumentCapture, AuthoringProjectSourceAdmissionError> {
    let inputs = snapshot.admit_source_capture_inputs(
        0,
        scenes::open_scene_inputs_retained_bytes(&sessions).unwrap(),
    )?;
    snapshot
        .capture_scene_request(source, root, inputs, Box::new([]))?
        .with_open_scenes(sessions.into_boxed_slice())
        .prepare()
}

#[test]
fn dirty_deferred_scene_is_not_a_participant_while_dirty_immediate_scene_owns_its_closure() {
    let project = fixture(&[], &[3], &[], &[]);
    write_demand_schema(&project);
    write_scene(
        &project,
        SECOND_SCENE,
        &demand_links([("image", product(2)), ("rules", product(4))]),
    );
    write_scene(
        &project,
        "scenes/d.scene.json",
        &demand_links([("rules", product(1))]),
    );
    let (snapshot, source, root) = open_second_scene(&project);
    let (b_source, mut b) = open_scene(&snapshot, "scenes/b.scene.json");
    let (d_source, mut d) = open_scene(&snapshot, "scenes/d.scene.json");
    let b_disk = fs::read(project.root.join("scenes/b.scene.json")).unwrap();
    let d_disk = fs::read(project.root.join("scenes/d.scene.json")).unwrap();
    edit_link(&mut b, "image", 99);
    edit_link(&mut d, "rules", 3);
    let capture = capture_with_scenes(
        &snapshot,
        &source,
        &root,
        vec![
            AuthoringSceneSessionSnapshot::capture(&b_source, &b),
            AuthoringSceneSessionSnapshot::capture(&d_source, &d),
        ],
    )
    .unwrap();
    assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
    assert_eq!(capture.scenes().len(), 3);
    assert!(capture.assets().is_empty());
    let d_captured = capture
        .scenes()
        .iter()
        .find(|scene| scene.source().project_path().as_str() == "scenes/d.scene.json")
        .unwrap();
    assert_eq!(d_captured.session_revision(), Some(d.revision()));
    assert!(Arc::ptr_eq(&d_captured.document, &d.shared_document()));
    assert!(
        capture
            .scenes()
            .iter()
            .any(|scene| { scene.source().project_path().as_str() == "scenes/c.scene.json" })
    );
    assert!(
        capture
            .scenes()
            .iter()
            .all(|scene| { scene.source().project_path().as_str() != "scenes/b.scene.json" })
    );
    assert!(
        capture
            .source_facts()
            .any(|fact| { fact.project_path().as_str() == "scenes/b.scene.json.meta" })
    );
    assert!(!capture.source_facts().any(|fact| {
        fact.project_path().as_str() == "scenes/b.scene.json"
            || fact.project_path().as_str() == "assets/unused.png"
    }));
    assert_eq!(
        fs::read(project.root.join("scenes/b.scene.json")).unwrap(),
        b_disk
    );
    assert_eq!(
        fs::read(project.root.join("scenes/d.scene.json")).unwrap(),
        d_disk
    );
    capture.source_validation().validate().unwrap();

    // Current preview captures no body or Session revision for B. In-place changes to that
    // body therefore do not invalidate its source binding or certify either body as valid.
    edit_link(&mut b, "image", 3);
    fs::write(
        project.root.join("scenes/b.scene.json"),
        b"malformed deferred body",
    )
    .unwrap();
    capture.source_validation().validate().unwrap();
    let next = capture_with_scenes(
        &snapshot,
        &source,
        &root,
        vec![
            AuthoringSceneSessionSnapshot::capture(&b_source, &b).without_persistence(),
            AuthoringSceneSessionSnapshot::capture(&d_source, &d),
        ],
    )
    .unwrap();
    assert_eq!(next.scenes().len(), 3);
    assert!(next.assets().is_empty());
}

#[test]
fn deferred_source_body_is_not_decoded_but_metadata_and_actual_file_rebinding_revoke() {
    for change in ["metadata", "replacement", "missing"] {
        let project = fixture(&[2], &[], &[], &[]);
        write_demand_schema(&project);
        // The source is readable and bound, while its payload is deliberately invalid and
        // larger than the entire current capture's observed encoded-byte inventory.
        fs::write(
            project.root.join("scenes/b.scene.json"),
            vec![b'!'; 1024 * 1024],
        )
        .unwrap();
        let (snapshot, source, root) = open_second_scene(&project);
        let capture = snapshot
            .capture_scene_document(&source, &root, &[])
            .unwrap();
        assert_eq!(capture.scenes().len(), 1);
        assert!(capture.assets().is_empty());
        assert!(capture.usage().aggregate_encoded_bytes() < 1024 * 1024);
        capture.source_validation().validate().unwrap();
        let path = project.root.join("scenes/b.scene.json");
        match change {
            "metadata" => metadata(&project, "scenes/b.scene.json", 22, AssetSourceKind::Scene),
            "replacement" => {
                fs::rename(&path, project.root.join("scenes/retired-b.scene.json")).unwrap();
                fs::write(&path, b"replacement is a different source object").unwrap();
            }
            "missing" => fs::remove_file(&path).unwrap(),
            _ => unreachable!(),
        }
        assert!(capture.source_validation().validate().is_err(), "{change}");
    }
}

#[test]
fn deferred_path_observation_uses_its_actual_reader_without_scanning_unrelated_metadata() {
    let project = independent_project();
    write_demand_schema(&project);
    write_scene(
        &project,
        SECOND_SCENE,
        &demand_links([("image", AssetRef::path("future.scene.json").unwrap())]),
    );
    fs::write(
        project.root.join("assets/future.scene.json"),
        b"unparsed future Scene",
    )
    .unwrap();
    metadata(
        &project,
        "assets/future.scene.json",
        2,
        AssetSourceKind::Scene,
    );
    // independent_project retains malformed unused metadata as an explicit catalogue canary.
    let (snapshot, source, root) = open_second_scene(&project);
    let capture = snapshot
        .capture_scene_document(&source, &root, &[])
        .unwrap();
    assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
    assert_eq!(capture.scenes().len(), 1);
    assert!(capture.assets().is_empty());
    assert!(
        capture
            .source_facts()
            .any(|fact| { fact.project_path().as_str() == "assets/future.scene.json.meta" })
    );
    assert!(
        !capture
            .source_facts()
            .any(|fact| { fact.project_path().as_str() == "assets/future.scene.json" })
    );
    capture.source_validation().validate().unwrap();
    fs::write(
        project.root.join("assets/future.scene.json"),
        b"another unparsed body",
    )
    .unwrap();
    capture.source_validation().validate().unwrap();
}

#[test]
fn a_later_immediate_use_upgrades_an_already_observed_deferred_scene() {
    let project = fixture(&[], &[], &[], &[]);
    write_demand_schema(&project);
    write_scene(
        &project,
        SECOND_SCENE,
        &demand_links([("image", product(2)), ("rules", product(4))]),
    );
    write_scene(
        &project,
        "scenes/b.scene.json",
        &demand_links([("rules", product(1))]),
    );
    write_scene(
        &project,
        "scenes/d.scene.json",
        &demand_links([("rules", product(2))]),
    );
    let (snapshot, source, root) = open_second_scene(&project);
    let (b_source, mut b) = open_scene(&snapshot, "scenes/b.scene.json");
    edit_link(&mut b, "rules", 3);
    let capture = capture_with_scenes(
        &snapshot,
        &source,
        &root,
        vec![AuthoringSceneSessionSnapshot::capture(&b_source, &b)],
    )
    .unwrap();
    assert_eq!(capture.scenes().len(), 4);
    let selected = capture
        .scenes()
        .iter()
        .find(|scene| scene.source().project_path().as_str() == "scenes/b.scene.json")
        .unwrap();
    assert_eq!(selected.session_revision(), Some(b.revision()));
    assert!(Arc::ptr_eq(&selected.document, &b.shared_document()));
    assert_eq!(
        capture
            .source_facts()
            .filter(|fact| { fact.project_path().as_str() == "scenes/b.scene.json" })
            .count(),
        1
    );
    let record = AssetRecord::new(
        id(2),
        AssetSourcePath::new("scenes/b.scene.json").unwrap(),
        AssetSourceKind::Scene,
    );
    assert_eq!(
        capture.asset_database.record_for_path(record.path()),
        Some(&record)
    );
    assert_eq!(
        capture.asset_database.record_for_stable_id(id(2)),
        Some(&record)
    );
    assert_eq!(
        capture
            .asset_database
            .records()
            .filter(|entry| entry.path() == record.path())
            .count(),
        1
    );
    capture.source_validation().validate().unwrap();
}

#[test]
fn duplicate_deferred_and_immediate_leaves_prepare_the_same_scene_once() {
    for targets in [
        [("image", product(2)), ("rules", product(2))],
        [("rules", product(2)), ("image", product(2))],
    ] {
        let project = fixture(&[], &[], &[], &[]);
        write_demand_schema(&project);
        write_scene(&project, SECOND_SCENE, &demand_links(targets));
        let (snapshot, source, root) = open_second_scene(&project);
        let (b_source, b) = open_scene(&snapshot, "scenes/b.scene.json");
        let capture = capture_with_scenes(
            &snapshot,
            &source,
            &root,
            vec![AuthoringSceneSessionSnapshot::capture(&b_source, &b)],
        )
        .unwrap();
        assert_eq!(capture.scenes().len(), 2);
        assert_eq!(
            capture
                .scenes()
                .iter()
                .filter(|scene| { scene.session_revision() == Some(b.revision()) })
                .count(),
            1
        );
        assert_eq!(
            capture
                .source_facts()
                .filter(|fact| { fact.project_path().as_str() == "scenes/b.scene.json" })
                .count(),
            1
        );
        capture.source_validation().validate().unwrap();
    }
}

#[test]
fn deferred_auxiliary_resolution_retains_source_binding_without_inventing_product_membership() {
    let project = fixture(&[], &[], &[], &[]);
    write_demand_schema(&project);
    fs::write(
        project.root.join("assets/compound.source"),
        b"not a decoded domain payload",
    )
    .unwrap();
    metadata(
        &project,
        "assets/compound.source",
        5,
        AssetSourceKind::Other("deferred-capture-domain".into()),
    );
    let auxiliary = AssetRef::Product(
        AssetProductRef::new(
            id(5),
            ImportedProductId::parse_subproduct("30797625-bf5e-4bf5-a79d-30a074209d52").unwrap(),
        )
        .unwrap(),
    );
    write_scene(
        &project,
        SECOND_SCENE,
        &demand_links([("image", auxiliary)]),
    );
    let (snapshot, source, root) = open_second_scene(&project);
    let capture = snapshot
        .capture_scene_document(&source, &root, &[])
        .unwrap();
    assert_eq!(capture.scenes().len(), 1);
    assert!(capture.assets().is_empty());
    assert!(
        capture
            .source_facts()
            .any(|fact| { fact.project_path().as_str() == "assets/compound.source.meta" })
    );
    assert!(
        !capture
            .source_facts()
            .any(|fact| { fact.project_path().as_str() == "assets/compound.source" })
    );
    capture.source_validation().validate().unwrap();
}

#[test]
fn deferred_source_binding_reservation_precedes_observation_and_identity_index_insertion() {
    let project = fixture(&[], &[], &[], &[]);
    let (snapshot, _, _) = open_second_scene(&project);
    let capture_at = |allowance: Option<usize>| {
        let root = snapshot.project_root();
        let mut budget = AuthoringProjectAdmissionBudget::new(
            crate::project_host::authoring::AuthoringProjectAdmissionLimits::default(),
        );
        let mut catalogue = ReferenceCatalogue::new(
            root.share(),
            ReferenceCatalogue::project_bindings(snapshot.settings()),
            &mut budget,
        )
        .unwrap();
        let path = ProjectPath::new("scenes/b.scene.json").unwrap();
        let opened = open_source_file(
            &root,
            path,
            "scenes/b.scene.json",
            AuthoringSourceKind::Scene,
            1,
            &mut budget,
        )
        .unwrap();
        let record = AssetRecord::new(
            id(2),
            AssetSourcePath::new("scenes/b.scene.json").unwrap(),
            AssetSourceKind::Scene,
        );
        let before = budget.retained_candidate_bytes;
        if let Some(allowance) = allowance {
            budget.limits.retained_candidate_bytes = ByteLimit::new(before + allowance).unwrap();
        }
        let result = catalogue.observe_resolved_target(opened, &record, &mut budget);
        let charged = budget.retained_candidate_bytes - before;
        let mut transfer_budget = AuthoringProjectAdmissionBudget::new(
            crate::project_host::authoring::AuthoringProjectAdmissionLimits::default(),
        );
        let retained = catalogue
            .into_reference_observations(&mut transfer_budget)
            .unwrap();
        (result, charged, retained.len())
    };
    let (result, bytes, count) = capture_at(None);
    result.unwrap();
    assert!(bytes > 0);
    assert_eq!(count, 1);
    let (exact, _, count) = capture_at(Some(bytes));
    exact.unwrap();
    assert_eq!(count, 1);
    let (one_short, _, count) = capture_at(Some(bytes - 1));
    assert!(matches!(
        one_short,
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        ))
    ));
    assert_eq!(
        count, 0,
        "rejected admission must leave no retained target or identity claim"
    );
}
