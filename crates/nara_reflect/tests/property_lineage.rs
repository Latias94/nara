use nara_reflect::{
    ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentMemberPath,
    ComponentMemberPathSegment, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaVersion, ComponentShapeMember,
    ComponentTypeId, ComponentValueKind, ComponentValueShape,
};

fn id(value: &str) -> ComponentFieldId {
    ComponentFieldId::new(value)
}

fn member(name: &str, identity: &str, shape: ComponentValueShape) -> ComponentShapeMember {
    ComponentShapeMember::new(name, shape).with_id(id(identity))
}

fn scalar() -> ComponentValueShape {
    ComponentValueShape::scalar(ComponentValueKind::U64)
}

fn path(ids: &[&str]) -> ComponentMemberPath {
    ComponentMemberPath::new(
        ids.iter()
            .map(|value| ComponentMemberPathSegment::Member(id(value))),
    )
}

fn field(shape: ComponentValueShape) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        id("data"),
        "Data",
        ComponentFieldPath::from_fields(["data"]),
        shape,
    )
}

fn catalog(version: u32, fields: Vec<ComponentFieldSchema>) -> ComponentSchemaCatalog {
    ComponentSchemaCatalog {
        components: vec![
            ComponentSchema::new(
                ComponentTypeId::new("game.ScopedMembers"),
                "Scoped members",
                ComponentSchemaVersion::new(version).unwrap(),
            )
            .with_fields(fields),
        ],
        ..Default::default()
    }
}

fn admit(
    catalog: ComponentSchemaCatalog,
    predecessor: Option<ComponentSchemaCatalog>,
) -> Result<ComponentRegistry, ComponentRegistryError> {
    ComponentRegistry::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new("game.scoped-members"),
        catalog,
        predecessor,
    )
}

fn successor(
    previous: &ComponentSchemaCatalog,
    version: u32,
    field: ComponentFieldSchema,
) -> ComponentSchemaCatalog {
    let mut next = ComponentSchemaCatalog::successor_of(previous).unwrap();
    next.components = catalog(version, vec![field]).components;
    next
}

#[test]
fn an_identified_nested_shape_is_reusable_under_distinct_root_fields_and_parent_members() {
    let axes = ComponentValueShape::record_members([
        member("x", "x", scalar()),
        member("y", "y", scalar()),
    ]);
    let position = ComponentFieldSchema::required(
        id("position"),
        "Position",
        ComponentFieldPath::from_fields(["position"]),
        axes.clone(),
    );
    let velocity = ComponentFieldSchema::required(
        id("velocity"),
        "Velocity",
        ComponentFieldPath::from_fields(["velocity"]),
        axes.clone(),
    );
    let nested = field(ComponentValueShape::record_members([
        member("left", "left", axes.clone()),
        member("right", "right", axes),
    ]));
    assert!(admit(catalog(1, vec![position, velocity, nested]), None).is_ok());
}

#[test]
fn sibling_id_collisions_and_active_retired_prefixes_are_rejected() {
    let duplicated = field(ComponentValueShape::record_members([
        member("first", "same", scalar()),
        member("second", "same", scalar()),
    ]));
    assert!(matches!(
        admit(catalog(1, vec![duplicated]), None),
        Err(ComponentRegistryError::InvalidComponentFieldShape { .. })
    ));
    let active = field(ComponentValueShape::record_members([member(
        "x",
        "x",
        scalar(),
    )]))
    .with_member_tombstones([path(&["x"])]);
    assert!(matches!(
        admit(catalog(1, vec![active]), None),
        Err(ComponentRegistryError::ActiveMemberIsTombstoned { .. })
    ));
    let duplicated_retirement =
        field(scalar()).with_member_tombstones([path(&["x"]), path(&["x"])]);
    assert!(matches!(
        admit(catalog(1, vec![duplicated_retirement]), None),
        Err(ComponentRegistryError::DuplicateMemberTombstone { .. })
    ));
}

#[test]
fn moving_an_id_to_another_scope_does_not_retire_the_original_path() {
    let before = catalog(
        1,
        vec![field(ComponentValueShape::record_members([
            member(
                "left",
                "left",
                ComponentValueShape::record_members([member("x", "x", scalar())]),
            ),
            member("right", "right", ComponentValueShape::record_members([])),
        ]))],
    );
    let next_shape = ComponentValueShape::record_members([
        member("left", "left", ComponentValueShape::record_members([])),
        member(
            "right",
            "right",
            ComponentValueShape::record_members([member("x", "x", scalar())]),
        ),
    ]);
    let missing = successor(&before, 2, field(next_shape.clone()));
    assert!(
        matches!(admit(missing, Some(before.clone())), Err(ComponentRegistryError::MissingMemberTombstone { path: missing, .. }) if missing == path(&["left", "x"]))
    );
    let next = successor(
        &before,
        2,
        field(next_shape).with_member_tombstones([path(&["left", "x"])]),
    );
    assert!(admit(next.clone(), Some(before)).is_ok());
    let restored = successor(
        &next,
        3,
        field(ComponentValueShape::record_members([
            member(
                "left",
                "left",
                ComponentValueShape::record_members([member("x", "x", scalar())]),
            ),
            member(
                "right",
                "right",
                ComponentValueShape::record_members([member("x", "x", scalar())]),
            ),
        ])),
    );
    assert!(matches!(
        admit(restored, Some(next)),
        Err(ComponentRegistryError::ReactivatedMember { .. })
    ));
}

#[test]
fn retiring_a_parent_covers_descendants_and_survives_an_intermediate_scalar_shape() {
    let before = catalog(
        1,
        vec![field(ComponentValueShape::record_members([member(
            "parent",
            "parent",
            ComponentValueShape::record_members([member("child", "child", scalar())]),
        )]))],
    );
    let retired = successor(
        &before,
        2,
        field(scalar()).with_member_tombstones([path(&["parent"])]),
    );
    assert!(admit(retired.clone(), Some(before)).is_ok());
    let dropped = successor(&retired, 3, field(scalar()));
    assert!(matches!(
        admit(dropped, Some(retired.clone())),
        Err(ComponentRegistryError::MissingMemberTombstone { .. })
    ));
    let restored = successor(
        &retired,
        3,
        field(ComponentValueShape::record_members([member(
            "renamed",
            "parent",
            scalar(),
        )])),
    );
    assert!(matches!(
        admit(restored, Some(retired)),
        Err(ComponentRegistryError::ReactivatedMember { .. })
    ));
}

#[test]
fn collection_declaration_scopes_are_tagged_and_do_not_contain_value_keys() {
    let shape = ComponentValueShape::record_members([member("x", "x", scalar())]);
    let before = catalog(
        1,
        vec![field(ComponentValueShape::keyed_list(shape, id("x")))],
    );
    let wrong_scope = ComponentMemberPath::new([
        ComponentMemberPathSegment::MapEntry,
        ComponentMemberPathSegment::Member(id("x")),
    ]);
    let next = successor(
        &before,
        2,
        field(scalar()).with_member_tombstones([wrong_scope]),
    );
    assert!(matches!(
        admit(next, Some(before.clone())),
        Err(ComponentRegistryError::MissingMemberTombstone { .. })
    ));
    let retired = ComponentMemberPath::new([
        ComponentMemberPathSegment::ListItem,
        ComponentMemberPathSegment::Member(id("x")),
    ]);
    let next = successor(
        &before,
        2,
        field(scalar()).with_member_tombstones([retired]),
    );
    assert!(admit(next, Some(before)).is_ok());
}
