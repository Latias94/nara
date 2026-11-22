use nara::{
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentMemberPath, ComponentMemberPathSegment,
        ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentShapeMember,
        ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
        PersistentComponentSchemaOwnerId,
    },
    scene::{SceneComponentRecord, SceneDocument, SceneEntityRecord},
};
use nara_editor_ui_host_consumer::project::entity;

pub const COMPONENT: &str = "consumer.children.Catalog";
pub const UNKNOWN: &str = "consumer.children.Unknown";
pub const OWNER: &str = "consumer.property-children";
pub const MAP_KEY: &str = "z.target/group[2]";
pub const ITEM_KEY: &str = "chosen/stable[1]";
pub const ORIGINAL: u64 = 41;
pub const PADDING_BYTES: usize = 48 * 1024;
pub const PARENT_KEY_BYTES: usize = 16 * 1024;
pub const LARGE_KEY_BYTES: usize = 32 * 1024;

pub fn large_keys() -> [String; 2] {
    let prefix = format!("a{}", "é".repeat((LARGE_KEY_BYTES - 2) / 2));
    [format!("{prefix}a"), format!("{prefix}b")]
}

fn member(wire: &str, stable: &str, shape: impl Into<ComponentValueShape>) -> ComponentShapeMember {
    ComponentShapeMember::new(wire, shape).with_id(ComponentFieldId::new(stable))
}

pub fn schema() -> AuthoringSchemaBundle {
    let item = ComponentValueShape::record_members([
        member("wire_token", "entry_id", ComponentValueKind::String),
        member("wire_enabled", "enabled", ComponentValueKind::Bool),
        member("wire_total", "score", ComponentValueKind::U64),
    ]);
    let settings = ComponentValueShape::record_members([
        member("wire_padding", "padding", ComponentValueKind::String),
        member(
            "wire_sections",
            "sections",
            ComponentValueShape::record_members([member(
                "wire_groups",
                "groups",
                ComponentValueShape::map(ComponentValueShape::keyed_list(
                    item,
                    ComponentFieldId::new("entry_id"),
                )),
            )]),
        ),
        member(
            "wire_scores",
            "scores",
            ComponentValueShape::map(ComponentValueShape::map(ComponentValueKind::U64.into())),
        ),
    ]);
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new(COMPONENT),
                    "Property children catalog",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("settings"),
                    "Settings",
                    ComponentFieldPath::from_fields(["wire_settings"]),
                    settings,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_member_tombstones([ComponentMemberPath::new([
                    ComponentMemberPathSegment::Member(ComponentFieldId::new("retired")),
                ])])]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn item(key: &str, score: u64) -> ComponentValue {
    ComponentValue::map([
        ("wire_token", ComponentValue::String(key.into())),
        ("wire_enabled", ComponentValue::Bool(true)),
        ("wire_total", ComponentValue::U64(score)),
    ])
}

pub fn source(score: u64, large_score: u64) -> SceneDocument {
    let [first, second] = large_keys();
    let value = ComponentValue::map([(
        "wire_settings",
        ComponentValue::map([
            (
                "wire_padding",
                ComponentValue::String("p".repeat(PADDING_BYTES)),
            ),
            (
                "wire_sections",
                ComponentValue::map([(
                    "wire_groups",
                    ComponentValue::map([
                        (
                            "a.other/group",
                            ComponentValue::List(vec![item("other/item", 777)]),
                        ),
                        (
                            MAP_KEY,
                            ComponentValue::List(vec![
                                item("first.item", 900),
                                item(ITEM_KEY, score),
                            ]),
                        ),
                    ]),
                )]),
            ),
            (
                "wire_scores",
                ComponentValue::map([(
                    "p".repeat(PARENT_KEY_BYTES),
                    ComponentValue::map([
                        (second, ComponentValue::U64(77)),
                        ("z".into(), ComponentValue::U64(99)),
                        (first, ComponentValue::U64(large_score)),
                    ]),
                )]),
            ),
        ]),
    )]);
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(entity("anchor"))
            .with_component(
                ComponentTypeId::new(COMPONENT),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, value)
                    .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
            )
            .with_component(
                ComponentTypeId::new(UNKNOWN),
                SceneComponentRecord::new(
                    ComponentSchemaVersion(7),
                    ComponentValue::map([(
                        "opaque/path[0]",
                        ComponentValue::List(vec![
                            ComponentValue::Null,
                            ComponentValue::Bool(false),
                            ComponentValue::U64(4_294_967_297),
                            ComponentValue::map([(
                                "unregistered_wire_name",
                                ComponentValue::String("preserve unknown data exactly".into()),
                            )]),
                        ]),
                    )]),
                ),
            ),
        SceneEntityRecord::new(entity("untouched")),
        SceneEntityRecord::new(entity("tail")),
    ])
}
