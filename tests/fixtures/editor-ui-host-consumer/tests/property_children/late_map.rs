use super::*;
use nara::reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldPath,
    ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog, ComponentShapeMember,
    ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara::scene::{SceneComponentRecord, SceneDocument, SceneEntityRecord};

const MAP_COUNT: usize = 130;
const LAST_MEMBER: &str = "member-129";

#[test]
fn a_public_panel_expands_a_late_map_in_live_and_prepared_scenes() {
    expand_late_map(EditorDocumentKind::Scene);
}

#[test]
fn a_public_panel_expands_a_late_map_in_live_and_prepared_prefabs() {
    expand_late_map(EditorDocumentKind::Prefab);
}

fn expand_late_map(kind: EditorDocumentKind) {
    let project = Project::new();
    let component = ComponentTypeId::new(fixture::COMPONENT);
    let owner = PersistentComponentSchemaOwnerId::try_new(fixture::OWNER).unwrap();
    let members = (0..MAP_COUNT).map(|index| {
        ComponentShapeMember::new(
            format!("wire-{index:03}"),
            ComponentValueShape::map(ComponentValueKind::U64.into()),
        )
        .with_id(ComponentFieldId::new(format!("member-{index:03}")))
    });
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    component.clone(),
                    "Property children catalog",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("settings"),
                    "Settings",
                    ComponentFieldPath::from_fields(["wire_settings"]),
                    ComponentValueShape::record_members(members),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    fs::write(
        project.root().join("schema/authoring.bundle.json"),
        bundle.to_json_string().unwrap(),
    )
    .unwrap();
    let value = ComponentValue::map([(
        "wire_settings",
        ComponentValue::map((0..MAP_COUNT).map(|index| {
            (
                format!("wire-{index:03}"),
                ComponentValue::map([
                    ("a", ComponentValue::U64(index as u64)),
                    ("z", ComponentValue::U64(index as u64 + 1)),
                ]),
            )
        })),
    )]);
    let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity("anchor"))
        .with_component(
            component,
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner),
        )]);
    let (relative_path, source) = match kind {
        EditorDocumentKind::Scene => (SOURCE, source.to_json_string()),
        EditorDocumentKind::Prefab => (
            PREFAB_SOURCE,
            PrefabDocument::new(source.placements, source.entities).to_json_string(),
        ),
        EditorDocumentKind::Extension(_) => unreachable!(),
    };
    fs::write(project.root().join(relative_path), source.unwrap()).unwrap();

    let mut application = Application::open(&project);
    let document = open_nonactive_source(&mut application, relative_path);
    let mut host = EditorPanelHost::new();
    let evidence = Rc::new(Evidence::default());
    register(&mut host, &mut application, &evidence, fixture::OWNER);
    drop(
        host.prepare(&application, |_, context| {
            let basis = context.document_query_basis(document).unwrap();
            let target = discover_late_map(&basis, |query| context.query_document_authoring(query));
            let read = |target| {
                let mut read = selected_read(target);
                read.limits = read.limits.with_scan_limits(256, SCAN_BYTES).unwrap();
                read
            };
            assert_eq!(
                context
                    .read_document_property(read(target.clone()))
                    .unwrap()
                    .value,
                EditorPropertyValue::Present(ComponentValue::U64(129))
            );
            let candidate = context
                .prepare_authoring_property_edit(
                    &basis,
                    &[AuthoringPropertyEdit {
                        entity: target.entity,
                        component: target.component,
                        component_version: ComponentSchemaVersion::ONE,
                        property: target.property,
                        value: ComponentValue::U64(7),
                    }],
                )
                .unwrap();
            let preview = candidate.preview();
            let target = discover_late_map(preview.basis(), |query| preview.query_authoring(query));
            assert_eq!(
                preview.read_property(read(target)).unwrap().value,
                EditorPropertyValue::Present(ComponentValue::U64(7))
            );
        })
        .unwrap(),
    );
    assert!(application.close());
}

fn discover_late_map(
    basis: &EditorDocumentQueryBasis,
    query: impl Fn(EditorAuthoringQuery) -> Result<EditorAuthoringPage, EditorDocumentQueryRejection>,
) -> EditorPropertyTarget {
    let entity = entity("anchor");
    let component = ComponentTypeId::new(fixture::COMPONENT);
    let start = |property: ComponentPropertyPath| {
        EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
            basis: basis.clone(),
            entity: entity.clone(),
            projection: EditorAuthoringProjection::Properties {
                component: component.clone(),
                property: property.into(),
            },
        })
    };
    let limits = EditorAuthoringQueryLimits::default();
    let parent = query(EditorAuthoringQuery {
        position: start(ComponentPropertyPath::new(ComponentFieldId::new(
            "settings",
        ))),
        limits,
    })
    .unwrap();
    assert_eq!(parent.rows.len(), MAP_COUNT);
    assert!(parent.next.is_none());
    let map = select_property(
        &[parent],
        ComponentValueKind::Map,
        |step| matches!(step, ComponentPropertyStep::Member(id) if id.as_str() == LAST_MEMBER),
    );
    let position = start(map.clone());
    let page = query(EditorAuthoringQuery {
        position: position.clone(),
        limits,
    })
    .unwrap();
    assert_eq!(page.rows.len(), 2);
    assert!(page.next.is_none());
    assert_eq!(page.scanned_entries, MAP_COUNT + 2);
    assert_eq!(
        page.scanned_logical_bytes,
        MAP_COUNT * ("wire-129".len() + LAST_MEMBER.len()) + 2
    );
    let a = select_property(
        &[page],
        ComponentValueKind::U64,
        |step| matches!(step, ComponentPropertyStep::MapEntry(key) if key == "a"),
    );
    assert_child(&map, &a);
    let target_bytes =
        entity.as_str().len() + component.as_str().len() + map.logical_bytes().unwrap();
    assert_eq!(
        query(EditorAuthoringQuery {
            position,
            limits: EditorAuthoringQueryLimits::new(256, 1, target_bytes).unwrap(),
        }),
        Err(EditorDocumentQueryRejection::MetadataBudgetExceeded)
    );
    EditorPropertyTarget {
        basis: basis.clone(),
        entity,
        component,
        property: a,
    }
}
