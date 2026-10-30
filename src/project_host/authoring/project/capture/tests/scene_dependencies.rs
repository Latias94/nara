//! Actual Scene data demand, nominal working-copy admission and complete file custody.

use nara_asset::{AssetProductRef, ImportedProductId};
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog, ComponentValueKind,
    PersistentComponentSchemaOwnerId, encode_asset_reference_value,
};
use nara_scene::{
    PrefabComponentIntent, PrefabEntityTarget, PrefabOverlay, SceneAuthoringSession,
    ScenePatchDocument, ScenePatchOperation,
};

use super::*;
use crate::project_host::authoring::{
    AdmittedAuthoringDocument, AuthoringDocumentKind, document::AuthoringDocumentSourceFact,
    tests::TestAuthoringProject,
};

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("nara.test.independent-capture").unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.IndependentImage")
}

pub(super) fn id(index: u8) -> StableAssetId {
    format!("c9494ad0-3f40-47ed-9df9-{index:012x}")
        .parse()
        .unwrap()
}

fn product(index: u8) -> AssetRef {
    AssetRef::Product(AssetProductRef::new(id(index), ImportedProductId::PRIMARY).unwrap())
}

pub(super) fn links(targets: &[u8]) -> SceneDocument {
    SceneDocument::from_ordered_roots(targets.iter().enumerate().map(|(index, &target)| {
        SceneEntityRecord::new(SceneEntityId::new(format!("link-{index}")).unwrap()).with_component(
            component(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("image", encode_asset_reference_value(&product(target)))]),
            )
            .with_owner(owner()),
        )
    }))
}

pub(super) fn metadata(
    project: &TestAuthoringProject,
    path: &str,
    index: u8,
    kind: AssetSourceKind,
) {
    let relative = path.split_once('/').unwrap().1;
    fs::write(
        project.root.join(format!("{path}.meta")),
        AssetMeta::new(
            AssetMetaIdentity::new(id(index), AssetPath::new(relative).unwrap(), kind),
            (),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
}

pub(super) fn fixture(root: &[u8], b: &[u8], c: &[u8], d: &[u8]) -> TestAuthoringProject {
    let project = independent_project();
    // Stable discovery observes valid sidecars, but the unused source never joins the closure.
    metadata(&project, "assets/unused.png", 99, AssetSourceKind::Image);
    for (path, index, document) in [
        (SECOND_SCENE, 1, links(root)),
        ("scenes/b.scene.json", 2, links(b)),
        ("scenes/c.scene.json", 3, links(c)),
        ("scenes/d.scene.json", 4, links(d)),
    ] {
        fs::write(project.root.join(path), document.to_json_string().unwrap()).unwrap();
        metadata(&project, path, index, AssetSourceKind::Scene);
    }
    project
}

pub(super) fn open_scene(
    snapshot: &AuthoringProjectSessionSnapshot,
    path: &str,
) -> (AuthoringDocumentSourceFact, SceneAuthoringSession) {
    let AdmittedAuthoringDocument::Scene {
        source, document, ..
    } = snapshot
        .capture_indexed_document(
            snapshot.source_index_snapshot().unwrap(),
            Arc::from(path),
            AuthoringDocumentKind::Scene,
        )
        .unwrap()
        .prepare()
        .unwrap()
    else {
        panic!("the selected document must remain a Scene")
    };
    (
        source,
        SceneAuthoringSession::from_published_authoring_document(document),
    )
}

#[test]
fn scene_cycles_self_edges_and_diamonds_retain_one_raw_and_effective_source_per_member() {
    let project = fixture(&[1, 2, 3], &[4, 1], &[4], &[]);
    let (snapshot, source, session) = open_second_scene(&project);
    let mut capture = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_eq!(capture.readiness(), AuthoringProjectReadiness::Complete);
    assert_eq!(capture.scenes().len(), 4);
    assert!(capture.prefabs().is_empty());
    assert!(capture.assets().is_empty());
    for scene in capture.scenes() {
        assert!(scene.effective_document().is_some());
        assert_eq!(
            capture
                .source_facts()
                .filter(|fact| fact.project_path() == scene.source().project_path())
                .count(),
            1
        );
        assert!(
            capture
                .source_facts()
                .any(|fact| fact.project_path().as_str()
                    == format!("{}.meta", scene.source().project_path().as_str()))
        );
    }
    assert_eq!(
        capture
            .scenes()
            .iter()
            .find(|scene| scene.source().project_path().as_str() == SECOND_SCENE)
            .unwrap()
            .document(),
        session.document()
    );
    // Dependency entities stay data; only the selected root reaches native presentation.
    let preview = capture.take_preview().unwrap();
    assert_eq!(preview.candidate.document().entities.len(), 3);
    assert!(
        preview
            .candidate
            .document()
            .entities
            .iter()
            .all(|entity| entity.components.is_empty())
    );
    capture.source_validation().validate().unwrap();
    let unrelated = project.root.join("assets/unused.png.meta");
    let mut bytes = fs::read(&unrelated).unwrap();
    bytes.push(b' ');
    fs::write(&unrelated, &bytes).unwrap();
    capture.source_validation().validate().unwrap();
    let demanded = project.root.join("scenes/d.scene.json.meta");
    let mut bytes = fs::read(&demanded).unwrap();
    bytes.push(b' ');
    fs::write(demanded, bytes).unwrap();
    assert!(capture.source_validation().validate().is_err());
}

#[test]
fn demanded_scene_working_copy_introduces_actual_transitive_demand_without_saving_it() {
    let project = fixture(&[2], &[1], &[4], &[]);
    let (snapshot, source, session) = open_second_scene(&project);
    let (b_source, mut b) = open_scene(&snapshot, "scenes/b.scene.json");
    let disk = fs::read(project.root.join("scenes/b.scene.json")).unwrap();
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: SceneEntityId::new("link-0").unwrap(),
        component: component(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("image"),
        value: encode_asset_reference_value(&product(3)),
    }]);
    assert!(b.apply_authoring_patch(&patch).applied);
    let inputs = snapshot
        .admit_source_capture_inputs(
            0,
            AuthoringSceneSessionSnapshot::capture_retained_bytes(&b_source, &b).unwrap(),
        )
        .unwrap();
    let working = AuthoringSceneSessionSnapshot::capture(&b_source, &b);
    let capture = snapshot
        .capture_scene_request(&source, &session, inputs, Box::new([]))
        .unwrap()
        .with_open_scenes(vec![working].into_boxed_slice())
        .prepare()
        .unwrap();
    assert_eq!(capture.scenes().len(), 4);
    let selected = capture
        .scenes()
        .iter()
        .find(|scene| scene.source().project_path().as_str() == "scenes/b.scene.json")
        .unwrap();
    assert_eq!(selected.session_revision(), Some(b.revision()));
    assert!(Arc::ptr_eq(&selected.document, &b.shared_document()));
    assert_eq!(selected.source().digest(), ContentDigest::of_bytes(&disk));
    assert_eq!(
        fs::read(project.root.join("scenes/b.scene.json")).unwrap(),
        disk
    );
    assert_eq!(session.document(), &links(&[2]));
    capture.source_validation().validate().unwrap();
}

#[test]
fn demanded_scene_unavailable_stale_ambiguous_or_equal_content_foreign_basis_refuses_disk_fallback()
{
    let project = fixture(&[2], &[], &[], &[]);
    let (snapshot, source, session) = open_second_scene(&project);
    let (b_source, b) = open_scene(&snapshot, "scenes/b.scene.json");
    let correct = AuthoringSceneSessionSnapshot::capture(&b_source, &b);
    let (_, _, other_root) = open_second_scene(&project);
    let mut incompatible = correct.clone();
    incompatible.basis = other_root.authoring_schema_basis();
    let mut stale = correct.clone();
    stale.source = Some(
        b_source
            .with_persistence_observation(
                b_source.identity(),
                b_source.parent_identity(),
                ContentDigest::of_bytes(b"not the accepted file"),
            )
            .unwrap(),
    );
    for snapshots in [
        vec![correct.clone().without_persistence()],
        vec![incompatible],
        vec![stale],
        vec![correct.clone(), correct.clone()],
        vec![AuthoringSceneSessionSnapshot::unavailable(
            b_source.source_key().clone(),
            Arc::from("scenes/b.scene.json"),
            &b,
        )],
    ] {
        let inputs = snapshot
            .admit_source_capture_inputs(
                0,
                scenes::open_scene_inputs_retained_bytes(&snapshots).unwrap(),
            )
            .unwrap();
        assert!(
            snapshot
                .capture_scene_request(&source, &session, inputs, Box::new([]))
                .unwrap()
                .with_open_scenes(snapshots.into_boxed_slice())
                .prepare()
                .is_err()
        );
    }
    // An unavailable unrelated open Scene is not a selected dependency.
    let (c_source, c) = open_scene(&snapshot, "scenes/c.scene.json");
    let inputs = snapshot
        .admit_source_capture_inputs(
            0,
            AuthoringSceneSessionSnapshot::capture_retained_bytes(&c_source, &c).unwrap(),
        )
        .unwrap();
    let capture = snapshot
        .capture_scene_request(&source, &session, inputs, Box::new([]))
        .unwrap()
        .with_open_scenes(
            vec![AuthoringSceneSessionSnapshot::capture(&c_source, &c).without_persistence()]
                .into_boxed_slice(),
        )
        .prepare()
        .unwrap();
    assert_eq!(capture.scenes().len(), 2);
    assert!(
        capture
            .scenes()
            .iter()
            .all(|scene| scene.session_revision().is_none())
    );
}

#[test]
fn schema_default_introduces_scene_demand_and_discarded_prefab_overlay_does_not() {
    let project = fixture(&[], &[], &[], &[]);
    let schema = ComponentSchema::new(component(), "SceneLink", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("image"),
            "Image",
            ComponentFieldPath::from_fields(["image"]),
            ComponentValueKind::AssetRef,
            encode_asset_reference_value(&product(2)),
        )
        .with_capability(ComponentCapability::AssetRef)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
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
    let root = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("defaulted").unwrap(),
    )
    .with_component(
        component(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([] as [(&str, ComponentValue); 0]),
        )
        .with_owner(owner()),
    )]);
    fs::write(
        project.root.join(SECOND_SCENE),
        root.to_json_string().unwrap(),
    )
    .unwrap();
    let (snapshot, source, session) = open_second_scene(&project);
    let capture = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_eq!(capture.scenes().len(), 2);
    assert!(
        capture
            .scenes()
            .iter()
            .any(|scene| scene.source().project_path().as_str() == "scenes/b.scene.json")
    );

    let key = nara_scene::SourceDocumentKey::project_location(
        "bb46c078-85b1-5823-8b1d-647b3b9e1f22".parse().unwrap(),
        AssetSourcePath::new("prefabs/outer.prefab.json").unwrap(),
        nara_scene::SourceDocumentKind::Prefab,
    );
    let base = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("hidden").unwrap(),
    )
    .with_component(
        component(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("image", encode_asset_reference_value(&product(88)))]),
        )
        .with_owner(owner()),
    )]);
    fs::write(
        project.root.join("prefabs/outer.prefab.json"),
        base.to_json_string().unwrap(),
    )
    .unwrap();
    let mut overlay = PrefabOverlay::new(key);
    overlay.components.push(PrefabComponentIntent::Suppress {
        target: PrefabEntityTarget::source(SceneEntityId::new("hidden").unwrap()),
        component: component(),
    });
    let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("outer.prefab.json").unwrap(),
        overrides: Some(overlay),
    });
    fs::write(
        project.root.join(SECOND_SCENE),
        SceneDocument::from_ordered_roots([anchor])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let (snapshot, source, session) = open_second_scene(&project);
    let capture = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_eq!(capture.scenes().len(), 1);
    assert!(!capture.requires_recapture());
    assert!(capture.composition_failure().is_none());
}

#[test]
fn complete_scene_closure_admits_its_exact_retained_ceiling_and_rejects_one_byte_less() {
    let project = fixture(&[1, 2, 3], &[4], &[4], &[1]);
    let capture_at = |maximum| {
        let (snapshot, source, session) = open_second_scene(&project);
        let inputs = snapshot.admit_source_capture_inputs(0, 0).unwrap();
        let mut request = snapshot
            .capture_scene_request(&source, &session, inputs, Box::new([]))
            .unwrap();
        drop(snapshot);
        Arc::get_mut(&mut request.schema)
            .expect("the request exclusively owns this schema receipt")
            .budget
            .limits
            .retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
        request.prepare()
    };
    let mut lower = 1;
    let mut upper = crate::project_host::authoring::AuthoringProjectAdmissionLimits::default()
        .retained_candidate_bytes()
        .get();
    assert_eq!(capture_at(upper).unwrap().scenes().len(), 4);
    while lower < upper {
        let midpoint = lower + (upper - lower) / 2;
        match capture_at(midpoint) {
            Ok(capture) => {
                assert_eq!(capture.scenes().len(), 4);
                upper = midpoint;
            }
            Err(
                AuthoringProjectSourceAdmissionError::Budget(_)
                | AuthoringProjectSourceAdmissionError::CompilationBudget(_),
            ) => lower = midpoint + 1,
            Err(error) => {
                panic!("an unchanged finite closure must fail only its selected budget: {error:?}")
            }
        }
    }
    let capture = capture_at(upper).unwrap();
    assert_eq!(capture.scenes().len(), 4);
    assert!(
        capture
            .source_facts()
            .any(|source| source.project_path().as_str() == "scenes/d.scene.json.meta")
    );
    assert!(matches!(
        capture_at(upper - 1),
        Err(AuthoringProjectSourceAdmissionError::Budget(_)
            | AuthoringProjectSourceAdmissionError::CompilationBudget(_))
    ));
}

#[test]
fn scene_kind_uses_its_actual_assets_or_prefabs_reader_and_canonical_identity() {
    for directory in ["assets", "prefabs"] {
        let project = fixture(&[2], &[1], &[], &[]);
        for suffix in ["", ".meta"] {
            fs::rename(
                project.root.join(format!("scenes/b.scene.json{suffix}")),
                project
                    .root
                    .join(format!("{directory}/b.scene.json{suffix}")),
            )
            .unwrap();
        }
        fs::write(
            project.root.join("scenes/b.scene.json"),
            b"malformed same-name Scene must remain unused",
        )
        .unwrap();
        let (snapshot, source, session) = open_second_scene(&project);
        let capture = snapshot
            .capture_scene_document(&source, &session, &[])
            .unwrap();
        assert_eq!(capture.scenes().len(), 2);
        let canonical = format!("{directory}/b.scene.json");
        let dependency = capture
            .scenes()
            .iter()
            .find(|scene| scene.source().project_path().as_str() == canonical)
            .unwrap();
        assert_eq!(dependency.document(), &links(&[1]));
        assert!(matches!(dependency.native_source().key().origin(),
            nara_scene::SourceDocumentOrigin::ProjectLocation { path, .. } if path.as_str() == canonical));
        assert!(
            capture
                .source_facts()
                .any(|fact| fact.project_path().as_str() == format!("{canonical}.meta"))
        );
        assert!(
            !capture
                .source_facts()
                .any(|fact| fact.project_path().as_str() == "scenes/b.scene.json")
        );
        capture.source_validation().validate().unwrap();
    }
}

#[test]
fn malformed_scene_data_duplicate_source_ids_and_auxiliary_scene_products_are_refused() {
    for corruption in ["data", "duplicate", "auxiliary"] {
        let project = fixture(&[2], &[], &[], &[]);
        match corruption {
            "data" => fs::write(project.root.join("scenes/b.scene.json"), b"{").unwrap(),
            "duplicate" => metadata(&project, "assets/unused.png", 2, AssetSourceKind::Image),
            "auxiliary" => {
                let mut root = links(&[2]);
                let auxiliary = AssetRef::Product(
                    AssetProductRef::new(
                        id(2),
                        ImportedProductId::parse_subproduct("30797625-bf5e-4bf5-a79d-30a074209d52")
                            .unwrap(),
                    )
                    .unwrap(),
                );
                root.entities[0]
                    .components
                    .get_mut(&component())
                    .unwrap()
                    .value =
                    ComponentValue::map([("image", encode_asset_reference_value(&auxiliary))]);
                fs::write(
                    project.root.join(SECOND_SCENE),
                    root.to_json_string().unwrap(),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let (snapshot, source, session) = open_second_scene(&project);
        assert!(
            snapshot
                .capture_scene_document(&source, &session, &[])
                .is_err(),
            "{corruption}"
        );
    }
}

#[test]
fn schema_successor_capture_demands_the_corresponding_prepared_scene_snapshot() {
    use crate::project_host::authoring::AuthoringSchemaOwnerPreparation;
    use nara_scene::{__private::borrow_prepared_scene_schema_source, SchemaReclassificationWork};

    let project = fixture(&[2], &[1], &[4], &[]);
    let (snapshot, source, session) = open_second_scene(&project);
    let (b_source, mut b) = open_scene(&snapshot, "scenes/b.scene.json");
    assert!(
        b.apply_authoring_patch(&ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: SceneEntityId::new("link-0").unwrap(),
            component: component(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("image"),
            value: encode_asset_reference_value(&product(3)),
        }]))
        .applied
    );
    let installation = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("nara.test.scene-successor").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new("nara.test.SceneSuccessor"),
                    "Successor",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
    .owners()[0]
        .clone();
    let AuthoringSchemaOwnerPreparation::Prepared { mut owner, .. } = snapshot
        .schema
        .prepare_owner(&installation, nara_reflect::ComponentSchemaOwnerChange::Add)
        .unwrap()
    else {
        panic!("the new data owner must prepare")
    };
    let mut work = SchemaReclassificationWork::default();
    let root_prepared = session
        .prepare_schema_reclassification(
            owner.schema_basis(),
            owner.reclassification_kind(),
            &mut work,
        )
        .unwrap();
    let b_prepared = b
        .prepare_schema_reclassification(
            owner.schema_basis(),
            owner.reclassification_kind(),
            &mut work,
        )
        .unwrap();
    let scenes = [AuthoringSceneSessionSnapshot::capture_prepared(
        &b_source,
        borrow_prepared_scene_schema_source(&b, &b_prepared).unwrap(),
    )
    .unwrap()];
    let capture = owner
        .capture_scene(
            &source,
            borrow_prepared_scene_schema_source(&session, &root_prepared).unwrap(),
            &[],
            &scenes,
        )
        .unwrap();
    assert_eq!(capture.capture.scenes().len(), 4);
    assert_eq!(
        capture
            .capture
            .scenes()
            .iter()
            .find(|scene| scene.source().project_path().as_str() == "scenes/b.scene.json")
            .unwrap()
            .session_revision(),
        Some(b.revision())
    );
    capture.validate(&mut owner).unwrap();
    assert_eq!(session.document(), &links(&[2]));
    assert_eq!(
        fs::read(project.root.join("scenes/b.scene.json")).unwrap(),
        links(&[1]).to_json_string().unwrap().as_bytes()
    );
}
