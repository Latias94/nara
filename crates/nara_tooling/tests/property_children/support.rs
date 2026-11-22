use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentMemberPath, ComponentMemberPathSegment,
    ComponentPropertyPath, ComponentPropertyStep, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentShapeMember, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringSchemaBasis,
    PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate, SceneAuthoringSession,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
};
use nara_tooling::{
    EditorAuthoringPage, EditorAuthoringProjection, EditorAuthoringQuery,
    EditorAuthoringQueryLimits, EditorAuthoringQueryPosition, EditorAuthoringQueryTarget,
    EditorDocumentId, EditorDocumentSourceBinding, EditorPersistenceAuthority, EditorPropertyRead,
    EditorPropertyReadLimits, EditorPropertyTarget, EditorPropertyValue, EditorWorkspace,
};

pub const COMPONENT: &str = "game.Children";
pub const ENTITY: &str = "item";
pub const PREFAB_SOURCE: &str = "prefabs/property-children.prefab.json";

pub struct Fixture {
    pub workspace: EditorWorkspace,
    pub document: EditorDocumentId,
    pub authority: EditorPersistenceAuthority,
}

pub fn entity() -> SceneEntityId {
    SceneEntityId::new(ENTITY).unwrap()
}

pub fn component() -> ComponentTypeId {
    ComponentTypeId::new(COMPONENT)
}

pub fn path(root: &str) -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new(root))
}

pub fn member(parent: &ComponentPropertyPath, id: &str) -> ComponentPropertyPath {
    parent
        .clone()
        .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(id))])
}

fn identified(name: &str, id: &str, shape: impl Into<ComponentValueShape>) -> ComponentShapeMember {
    ComponentShapeMember::new(name, shape).with_id(ComponentFieldId::new(id))
}

fn field(name: &str, shape: impl Into<ComponentValueShape>) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new(name),
        name,
        ComponentFieldPath::from_fields([name]),
        shape,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
}

fn keyed_shape(kind: ComponentValueKind) -> ComponentValueShape {
    ComponentValueShape::keyed_list(
        ComponentValueShape::record_members([
            identified("wire_id", "id", kind),
            identified("wire_score", "score", ComponentValueKind::U64),
        ]),
        ComponentFieldId::new("id"),
    )
}

pub fn keyed_item(key: ComponentValue, score: u64) -> ComponentValue {
    ComponentValue::map([("wire_id", key), ("wire_score", ComponentValue::U64(score))])
}

fn declarations() -> ComponentSchema {
    let record = ComponentValueShape::record_members([
        identified("wire_first", "first", ComponentValueKind::U64),
        ComponentShapeMember::new(
            "wire_anonymous",
            ComponentValueShape::record_members([identified(
                "wire_buried",
                "buried",
                ComponentValueKind::Bool,
            )]),
        ),
        identified(
            "wire_branch",
            "branch",
            ComponentValueShape::record_members([
                identified("wire_leaf", "leaf", ComponentValueKind::Bool),
                ComponentShapeMember::new("wire_anonymous_leaf", ComponentValueKind::String),
            ]),
        ),
        identified("wire_last", "last", ComponentValueKind::String),
    ]);
    let record = field("record", record).with_member_tombstones([
        ComponentMemberPath::new([ComponentMemberPathSegment::Member(ComponentFieldId::new(
            "gone",
        ))]),
        ComponentMemberPath::new([
            ComponentMemberPathSegment::Member(ComponentFieldId::new("branch")),
            ComponentMemberPathSegment::Member(ComponentFieldId::new("old_leaf")),
        ]),
    ]);
    let score_record = ComponentValueShape::record_members([identified(
        "wire_score",
        "score",
        ComponentValueKind::U64,
    )]);
    let mut fields = vec![
        field("amount", ComponentValueKind::U64),
        record,
        field(
            "map",
            ComponentValueShape::map(ComponentValueKind::U64.into()),
        ),
        field("records", ComponentValueShape::map(score_record.clone())).with_member_tombstones([
            ComponentMemberPath::new([
                ComponentMemberPathSegment::MapEntry,
                ComponentMemberPathSegment::Member(ComponentFieldId::new("gone")),
            ]),
        ]),
        field("strings", keyed_shape(ComponentValueKind::String)),
        field("signed", keyed_shape(ComponentValueKind::I64)),
        field("unsigned", keyed_shape(ComponentValueKind::U64)),
        field(
            "list",
            ComponentValueShape::list(ComponentValueKind::U64.into()),
        ),
        ComponentFieldSchema::required(
            ComponentFieldId::new("locked"),
            "Locked",
            ComponentFieldPath::from_fields(["locked"]),
            ComponentValueShape::map(ComponentValueKind::U64.into()),
        ),
    ];
    for (suffix, shape, default) in [
        (
            "map",
            ComponentValueShape::map(ComponentValueKind::U64.into()),
            ComponentValue::map([("default", ComponentValue::U64(77))]),
        ),
        (
            "keyed",
            keyed_shape(ComponentValueKind::String),
            ComponentValue::List(vec![keyed_item(
                ComponentValue::String("default".into()),
                88,
            )]),
        ),
        (
            "record",
            score_record,
            ComponentValue::map([("wire_score", ComponentValue::U64(99))]),
        ),
    ] {
        for prefix in ["absent", "null"] {
            let name = format!("{prefix}_{suffix}");
            fields.push(ComponentFieldSchema::optional(
                ComponentFieldId::new(&name),
                &name,
                ComponentFieldPath::from_fields([name.as_str()]),
                shape.clone(),
            ));
        }
        for (prefix, value) in [("default_null", ComponentValue::Null), ("default", default)] {
            let name = format!("{prefix}_{suffix}");
            fields.push(ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new(&name),
                &name,
                ComponentFieldPath::from_fields([name.as_str()]),
                shape.clone(),
                value,
            ));
        }
    }
    ComponentSchema::new(
        component(),
        "Property children",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields(fields)
}

fn bundle() -> (AuthoringSchemaBundle, Vec<AuthoringAdapterReceipt>) {
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.property-children").unwrap(),
        ComponentSchemaCatalog {
            components: vec![declarations()],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    (bundle, vec![adapter])
}

pub fn fresh_schema_basis() -> AuthoringSchemaBasis {
    let (bundle, adapters) = bundle();
    AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap()
}

pub fn fixture(prefab: bool) -> Fixture {
    let value = ComponentValue::map([
        ("amount", ComponentValue::U64(7)),
        (
            "record",
            ComponentValue::map([
                ("wire_first", ComponentValue::U64(11)),
                (
                    "wire_anonymous",
                    ComponentValue::map([("wire_buried", ComponentValue::Bool(false))]),
                ),
                (
                    "wire_branch",
                    ComponentValue::map([
                        ("wire_leaf", ComponentValue::Bool(true)),
                        (
                            "wire_anonymous_leaf",
                            ComponentValue::String("hidden".into()),
                        ),
                    ]),
                ),
                ("wire_last", ComponentValue::String("last value".into())),
            ]),
        ),
        (
            "map",
            ComponentValue::map([
                ("z", ComponentValue::U64(4)),
                ("é", ComponentValue::U64(5)),
                ("a/b", ComponentValue::U64(3)),
                ("", ComponentValue::U64(1)),
                ("a", ComponentValue::U64(2)),
            ]),
        ),
        (
            "strings",
            ComponentValue::List(vec![
                keyed_item(ComponentValue::String("z".into()), 101),
                keyed_item(ComponentValue::String("a".into()), 102),
                keyed_item(ComponentValue::String(String::new()), 103),
            ]),
        ),
        (
            "records",
            ComponentValue::map([(
                "present",
                ComponentValue::map([("wire_score", ComponentValue::U64(55))]),
            )]),
        ),
        (
            "signed",
            ComponentValue::List(vec![
                keyed_item(ComponentValue::I64(-9), 201),
                keyed_item(ComponentValue::I64(i64::MIN), 202),
                keyed_item(ComponentValue::I64(-1), 203),
            ]),
        ),
        (
            "unsigned",
            ComponentValue::List(vec![
                keyed_item(ComponentValue::U64(u64::MAX), 301),
                keyed_item(ComponentValue::U64(0), 302),
                keyed_item(ComponentValue::U64(7), 303),
            ]),
        ),
        ("list", ComponentValue::List(vec![ComponentValue::U64(1)])),
        (
            "locked",
            ComponentValue::map([("value", ComponentValue::U64(19))]),
        ),
        ("null_map", ComponentValue::Null),
        ("null_keyed", ComponentValue::Null),
        ("null_record", ComponentValue::Null),
    ]);
    let records = [
        SceneEntityRecord::new(entity()).with_component(
            component(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
        ),
        SceneEntityRecord::new(SceneEntityId::new("other").unwrap()),
    ];
    let (bundle, adapters) = bundle();
    let (mut workspace, authority) = EditorWorkspace::new_hosted();
    let document = if prefab {
        let document = PrefabDocument::from_ordered_roots(records);
        let published = PrefabDocumentCandidate::decode_json_bytes(
            document.to_json_string().unwrap().as_bytes(),
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path(PREFAB_SOURCE).unwrap()),
            &bundle,
            &adapters,
        )
        .unwrap()
        .publish();
        workspace
            .open_prefab_session(
                "property children",
                EditorDocumentSourceBinding::new(PREFAB_SOURCE),
                PrefabAuthoringSession::from_published_authoring_document(published),
            )
            .unwrap()
            .opened_document
            .unwrap()
    } else {
        let document = SceneDocument::from_ordered_roots(records);
        let published = SceneDocumentCandidate::decode_json_bytes(
            document.to_json_string().unwrap().as_bytes(),
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(
                AssetRef::path("scenes/property-children.scene.json").unwrap(),
            ),
            &bundle,
            &adapters,
        )
        .unwrap()
        .publish();
        workspace
            .open_scene_session(
                "property children",
                SceneAuthoringSession::from_published_authoring_document(published),
            )
            .unwrap()
            .opened_document
            .unwrap()
    };
    Fixture {
        workspace,
        document,
        authority,
    }
}

pub fn limits(rows: usize) -> EditorAuthoringQueryLimits {
    EditorAuthoringQueryLimits::new(256, rows, 65_536).unwrap()
}

pub fn query(
    workspace: &EditorWorkspace,
    document: EditorDocumentId,
    property: ComponentPropertyPath,
    limits: EditorAuthoringQueryLimits,
) -> EditorAuthoringQuery {
    EditorAuthoringQuery {
        position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
            basis: workspace.document_query_basis(document).unwrap(),
            entity: entity(),
            projection: EditorAuthoringProjection::Properties {
                component: component(),
                property: property.into(),
            },
        }),
        limits,
    }
}

pub fn read_request(
    workspace: &EditorWorkspace,
    document: EditorDocumentId,
    property: ComponentPropertyPath,
) -> EditorPropertyRead {
    EditorPropertyRead {
        target: EditorPropertyTarget {
            basis: workspace.document_query_basis(document).unwrap(),
            entity: entity(),
            component: component(),
            property,
        },
        limits: EditorPropertyReadLimits::new(65_536, 128, 4096).unwrap(),
    }
}

pub fn read(
    workspace: &EditorWorkspace,
    document: EditorDocumentId,
    property: ComponentPropertyPath,
) -> EditorPropertyValue {
    workspace
        .read_document_property(read_request(workspace, document, property))
        .unwrap()
        .value
}

pub fn continue_page(
    workspace: &EditorWorkspace,
    page: &EditorAuthoringPage,
    limits: EditorAuthoringQueryLimits,
) -> EditorAuthoringPage {
    workspace
        .query_document_authoring(EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Continue(page.next.clone().unwrap()),
            limits,
        })
        .unwrap()
}
