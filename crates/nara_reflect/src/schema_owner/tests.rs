use super::*;
use crate::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentMemberPath, ComponentMemberPathSegment, ComponentSchemaVersion, ComponentValue,
    ComponentValueKind, ComponentValueShape,
};

fn owner(name: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(name).unwrap()
}

fn schema() -> ComponentSchema {
    let mut field = ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::U64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    field.default_value = Some(ComponentValue::U64(7));
    ComponentSchema::new(
        ComponentTypeId::new("test.CreationMetadata"),
        "Creation metadata",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([field])
    .with_authoring_initial_value(ComponentValue::map([("value", ComponentValue::U64(1))]))
}

fn first() -> ComponentSchemaOwnerSnapshot {
    ComponentSchemaOwnerSnapshot::admit(
        owner("test.creation"),
        ComponentSchemaCatalog {
            components: vec![schema()],
            type_tombstones: vec![ComponentTypeId::new("test.Retired")],
            ..Default::default()
        },
        None,
    )
    .unwrap()
}

fn successor(
    previous: &ComponentSchemaOwnerSnapshot,
    edit: impl FnOnce(&mut ComponentSchemaCatalog),
) -> ComponentSchemaOwnerSnapshot {
    let mut current = ComponentSchemaCatalog::successor_of(previous.current()).unwrap();
    current.components = previous.current().components().to_vec();
    edit(&mut current);
    ComponentSchemaOwnerSnapshot::admit(
        previous.owner_id().clone(),
        current,
        Some(previous.current().clone()),
    )
    .unwrap()
}

fn change_initial(catalog: &mut ComponentSchemaCatalog) {
    catalog.components[0].authoring_initial_value =
        Some(ComponentValue::map([("value", ComponentValue::U64(2))]));
}

#[test]
fn strict_definition_comparison_excludes_only_the_initial_value() {
    let previous = schema();
    let mut current = previous.clone();
    current.authoring_initial_value = None;
    assert!(current.same_definition_except_initial_value(&previous));
    assert!(previous.same_definition_except_initial_value(&current));

    let changes: [fn(&mut ComponentSchema); 15] = [
        |schema| schema.id = ComponentTypeId::new("test.Other"),
        |schema| schema.aliases.push("Other alias".into()),
        |schema| schema.version = ComponentSchemaVersion::new(2).unwrap(),
        |schema| schema.capabilities.clear(),
        |schema| schema.fields.clear(),
        |schema| {
            schema
                .field_tombstones
                .push(ComponentFieldId::new("retired"))
        },
        |schema| schema.fields[0].id = ComponentFieldId::new("other"),
        |schema| schema.fields[0].aliases.push("Other field alias".into()),
        |schema| schema.fields[0].path = ComponentFieldPath::from_fields(["other"]),
        |schema| schema.fields[0].shape = ComponentValueShape::scalar(ComponentValueKind::String),
        |schema| schema.fields[0].required = false,
        |schema| schema.fields[0].capabilities.clear(),
        |schema| schema.fields[0].default_value = Some(ComponentValue::U64(8)),
        |schema| {
            schema.fields[0]
                .member_tombstones
                .push(ComponentMemberPath::new([
                    ComponentMemberPathSegment::Member(ComponentFieldId::new("retired")),
                ]));
        },
        |schema| schema.fields.reverse(),
    ];
    // Add a second field so declaration order is also tested without canonicalization.
    let previous = previous.with_fields([ComponentFieldSchema::optional(
        ComponentFieldId::new("second"),
        "Second",
        ComponentFieldPath::from_fields(["second"]),
        ComponentValueKind::U64,
    )]);
    for change in changes {
        let mut current = previous.clone();
        change(&mut current);
        assert!(!current.same_definition_except_initial_value(&previous));
        assert!(!previous.same_definition_except_initial_value(&current));
    }
}

#[test]
fn creation_successor_uses_actual_complete_predecessor_at_every_generation() {
    let previous = first();
    let current = successor(&previous, change_initial);
    assert!(current.is_creation_metadata_successor_of(&previous));
    assert!(!previous.is_creation_metadata_successor_of(&previous));
    assert!(!previous.is_creation_metadata_successor_of(&current));
    let third = successor(&current, |catalog| {
        catalog.components[0].authoring_initial_value = None;
    });
    assert!(third.is_creation_metadata_successor_of(&current));
    assert!(!third.is_creation_metadata_successor_of(&previous));

    let foreign = ComponentSchemaOwnerSnapshot::admit(
        owner("test.foreign"),
        current.current().clone(),
        current.predecessor().cloned(),
    )
    .unwrap();
    assert!(!foreign.is_creation_metadata_successor_of(&previous));

    let mut altered = previous.current().clone();
    altered.components[0]
        .aliases
        .push("Different predecessor".into());
    let altered =
        ComponentSchemaOwnerSnapshot::admit(previous.owner_id().clone(), altered, None).unwrap();
    let unrelated = successor(&altered, change_initial);
    assert!(!unrelated.is_creation_metadata_successor_of(&previous));
}

#[test]
fn admitted_stored_definition_changes_and_generation_only_churn_are_not_creation_successors() {
    let previous = first();
    let changes: [fn(&mut ComponentSchemaCatalog); 7] = [
        |_| {},
        |catalog| catalog.components[0].aliases.push("New alias".into()),
        |catalog| {
            catalog.components[0].version = ComponentSchemaVersion::new(2).unwrap();
        },
        |catalog| {
            catalog.components[0].version = ComponentSchemaVersion::new(2).unwrap();
            catalog.components[0].fields[0].default_value = Some(ComponentValue::U64(9));
        },
        |catalog| {
            catalog.components[0].version = ComponentSchemaVersion::new(2).unwrap();
            catalog.components[0].fields[0].required = false;
        },
        |catalog| {
            catalog
                .type_tombstones
                .push(ComponentTypeId::new("test.OtherRetired"))
        },
        |catalog| {
            catalog.components.push(ComponentSchema::new(
                ComponentTypeId::new("test.Added"),
                "Added",
                ComponentSchemaVersion::ONE,
            ));
        },
    ];
    assert!(!successor(&previous, |_| {}).is_creation_metadata_successor_of(&previous));
    for edit in changes.into_iter().skip(1) {
        let current = successor(&previous, |catalog| {
            change_initial(catalog);
            edit(catalog);
        });
        assert!(!current.is_creation_metadata_successor_of(&previous));
    }
}
