use super::*;
use nara_reflect::{
    ComponentFieldPath, ComponentFieldSchema, ComponentSchemaCatalog, ComponentValueKind,
};

const OWNER: &str = "test.creation.owner";
const OTHER: &str = "test.creation.other";

fn owner(name: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(name).unwrap()
}

fn schema(name: &str) -> ComponentSchema {
    let mut field = ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::U64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    field.default_value = Some(ComponentValue::U64(7));
    ComponentSchema::new(
        ComponentTypeId::new(name),
        "Creation",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([field])
    .with_authoring_initial_value(ComponentValue::map([("value", ComponentValue::U64(1))]))
}

fn first() -> AuthoringSchemaBundle {
    AuthoringSchemaBundle::from_owner_snapshots([
        ComponentSchemaOwnerSnapshot::admit(
            owner(OWNER),
            ComponentSchemaCatalog {
                components: vec![schema("test.Creation"), schema("test.Companion")],
                ..Default::default()
            },
            None,
        )
        .unwrap(),
        ComponentSchemaOwnerSnapshot::admit(
            owner(OTHER),
            ComponentSchemaCatalog {
                components: vec![schema("test.Unrelated")],
                ..Default::default()
            },
            None,
        )
        .unwrap(),
    ])
    .unwrap()
}

fn evolve(
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

fn change_initial(current: &mut ComponentSchemaCatalog) {
    current.components[0].authoring_initial_value =
        Some(ComponentValue::map([("value", ComponentValue::U64(2))]));
}

fn replace(
    bundle: &AuthoringSchemaBundle,
    replacement: &ComponentSchemaOwnerSnapshot,
) -> AuthoringSchemaBundle {
    AuthoringSchemaBundle::from_owner_snapshots(bundle.owners().iter().map(|snapshot| {
        if snapshot.owner_id() == replacement.owner_id() {
            replacement.clone()
        } else {
            snapshot.clone()
        }
    }))
    .unwrap()
}

fn data_basis(
    bundle: &AuthoringSchemaBundle,
    excluded: &BTreeSet<ComponentTypeId>,
) -> AuthoringSchemaBasis {
    let adapters: Vec<_> = bundle
        .owners()
        .iter()
        .filter_map(|owner| AuthoringAdapterReceipt::from_schema_owner(owner, excluded))
        .collect();
    AuthoringSchemaBasis::admit(bundle, &adapters).unwrap()
}

fn kind() -> SchemaReclassificationKind {
    SchemaReclassificationKind::CreationMetadata {
        owner: owner(OWNER),
    }
}

#[test]
fn exact_admitted_creation_successor_and_nominal_noop_are_distinct() {
    let bundle = first();
    let previous = data_basis(&bundle, &BTreeSet::new());
    let snapshot = evolve(bundle.owner(&owner(OWNER)).unwrap(), change_initial);
    let next_bundle = replace(&bundle, &snapshot);
    let next = data_basis(&next_bundle, &BTreeSet::new());
    assert!(
        next.require_reclassification_successor_of(&previous, &kind())
            .is_ok()
    );
    assert!(
        next.require_reclassification_successor_of(&next.clone(), &kind())
            .is_ok()
    );
    assert!(
        data_basis(&bundle, &BTreeSet::new())
            .require_reclassification_successor_of(&previous, &kind())
            .is_err()
    );
    assert!(
        data_basis(&next_bundle, &BTreeSet::new())
            .require_reclassification_successor_of(&next, &kind())
            .is_err()
    );
    assert!(
        next.require_reclassification_successor_of(
            &previous,
            &SchemaReclassificationKind::DeclarationExtension,
        )
        .is_err()
    );
    assert!(
        next.require_reclassification_successor_of(
            &previous,
            &SchemaReclassificationKind::CreationMetadata {
                owner: owner(OTHER)
            },
        )
        .is_err()
    );
    let third = evolve(&snapshot, |catalog| {
        catalog.components[0].authoring_initial_value = None;
    });
    let third = data_basis(&replace(&next_bundle, &third), &BTreeSet::new());
    assert!(
        third
            .require_reclassification_successor_of(&next, &kind())
            .is_ok()
    );
    assert!(
        third
            .require_reclassification_successor_of(&previous, &kind())
            .is_err()
    );
}

#[test]
fn admitted_definition_coverage_tombstone_and_other_owner_changes_refuse() {
    let bundle = first();
    let previous = data_basis(&bundle, &BTreeSet::new());
    let changes: [fn(&mut ComponentSchemaCatalog); 7] = [
        |catalog| catalog.components[0].aliases.push("Another alias".into()),
        |catalog| catalog.components[0].version = ComponentSchemaVersion::new(2).unwrap(),
        |catalog| {
            catalog.components[0].version = ComponentSchemaVersion::new(2).unwrap();
            catalog.components[0].fields[0].default_value = Some(ComponentValue::U64(9));
        },
        |catalog| {
            catalog.components[0].version = ComponentSchemaVersion::new(2).unwrap();
            catalog.components[0].fields[0].shape =
                ComponentValueShape::scalar(ComponentValueKind::I64);
            catalog.components[0].fields[0].default_value = Some(ComponentValue::I64(7));
            catalog.components[0].authoring_initial_value =
                Some(ComponentValue::map([("value", ComponentValue::I64(2))]));
        },
        |catalog| {
            catalog
                .type_tombstones
                .push(ComponentTypeId::new("test.Retired"))
        },
        |catalog| catalog.components.push(schema("test.Added")),
        |catalog| {
            let removed = catalog.components.pop().unwrap();
            catalog.type_tombstones.push(removed.id().clone());
        },
    ];
    for edit in changes {
        let snapshot = evolve(bundle.owner(&owner(OWNER)).unwrap(), |catalog| {
            change_initial(catalog);
            edit(catalog);
        });
        let next = data_basis(&replace(&bundle, &snapshot), &BTreeSet::new());
        assert!(
            next.require_reclassification_successor_of(&previous, &kind())
                .is_err()
        );
    }
    let selected = evolve(bundle.owner(&owner(OWNER)).unwrap(), change_initial);
    let next_bundle = replace(&bundle, &selected);
    // A catalog-only change in an unaffected owner must not disappear in the selected entries.
    let other = evolve(bundle.owner(&owner(OTHER)).unwrap(), |catalog| {
        catalog
            .type_tombstones
            .push(ComponentTypeId::new("test.OtherRetired"));
    });
    let next = data_basis(&replace(&next_bundle, &other), &BTreeSet::new());
    assert!(
        next.require_reclassification_successor_of(&previous, &kind())
            .is_err()
    );
    let mut unrelated_predecessor = bundle.owner(&owner(OWNER)).unwrap().current().clone();
    unrelated_predecessor.components[0]
        .aliases
        .push("Different predecessor".into());
    let unrelated =
        ComponentSchemaOwnerSnapshot::admit(owner(OWNER), unrelated_predecessor, None).unwrap();
    let unrelated = evolve(&unrelated, change_initial);
    let next = data_basis(&replace(&bundle, &unrelated), &BTreeSet::new());
    assert!(
        next.require_reclassification_successor_of(&previous, &kind())
            .is_err()
    );
}

#[test]
fn incomplete_coverage_stale_receipts_and_catalog_only_churn_refuse() {
    let bundle = first();
    let previous = data_basis(&bundle, &BTreeSet::new());
    let next_owner = evolve(bundle.owner(&owner(OWNER)).unwrap(), change_initial);
    let next_bundle = replace(&bundle, &next_owner);
    let next = data_basis(&next_bundle, &BTreeSet::new());
    for component in ["test.Companion", "test.Unrelated"] {
        let excluded = BTreeSet::from([ComponentTypeId::new(component)]);
        assert!(
            data_basis(&next_bundle, &excluded)
                .require_reclassification_successor_of(&previous, &kind())
                .is_err()
        );
        assert!(
            next.require_reclassification_successor_of(&data_basis(&bundle, &excluded), &kind())
                .is_err()
        );
    }
    let stale_receipts: Vec<_> = bundle
        .owners()
        .iter()
        .filter_map(|owner| AuthoringAdapterReceipt::from_schema_owner(owner, &BTreeSet::new()))
        .collect();
    let stale = AuthoringSchemaBasis::admit(&next_bundle, &stale_receipts).unwrap();
    assert!(
        stale
            .require_reclassification_successor_of(&previous, &kind())
            .is_err()
    );
    let churn = evolve(bundle.owner(&owner(OWNER)).unwrap(), |_| {});
    let churn = data_basis(&replace(&bundle, &churn), &BTreeSet::new());
    assert!(
        churn
            .require_reclassification_successor_of(&previous, &kind())
            .is_err()
    );
}

#[test]
fn creation_metadata_never_rebinds_native_receipts_even_with_the_data_adapter_name() {
    let provider =
        nara_reflect::__private::resolve_schema_provider(crate::SCENE_COMPONENTS_SCHEMA_PROVIDER)
            .unwrap();
    let id = owner(crate::SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str());
    let previous = ComponentSchemaOwnerSnapshot::admit(
        id.clone(),
        provider.predecessor().unwrap().catalog().clone(),
        None,
    )
    .unwrap();
    let current = ComponentSchemaOwnerSnapshot::admit(
        id.clone(),
        provider.current().catalog().clone(),
        Some(previous.current().clone()),
    )
    .unwrap();
    assert!(current.is_creation_metadata_successor_of(&previous));
    let previous_bundle = AuthoringSchemaBundle::from_owner_snapshots([previous]).unwrap();
    let current_bundle = AuthoringSchemaBundle::from_owner_snapshots([current.clone()]).unwrap();
    let previous = data_basis(&previous_bundle, &BTreeSet::new());
    let kind = SchemaReclassificationKind::CreationMetadata { owner: id };
    for (identity, version, generation) in [
        ("nara.schema-data-authoring", 1, 1),
        ("test.changed-implementation", 1, 1),
        ("nara.schema-data-authoring", 2, 1),
        ("nara.schema-data-authoring", 1, 2),
    ] {
        let receipt = AuthoringAdapterReceipt::from_trusted_provider(
            provider.contribution_receipt(),
            identity,
            version,
            generation,
            current
                .current()
                .components()
                .iter()
                .map(|schema| schema.id().clone()),
        )
        .unwrap();
        let next = AuthoringSchemaBasis::admit(&current_bundle, &[receipt]).unwrap();
        assert!(
            next.require_reclassification_successor_of(&previous, &kind)
                .is_err()
        );
    }
}
