use nara_reflect::{
    ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentRegistryError,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion,
    ComponentShapeErrorKind, ComponentTypeId, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};

fn catalog(ids: &[&str]) -> ComponentSchemaCatalog {
    ComponentSchemaCatalog {
        components: ids
            .iter()
            .map(|id| {
                ComponentSchema::new(
                    ComponentTypeId::new(*id),
                    *id,
                    ComponentSchemaVersion::new(1).unwrap(),
                )
            })
            .collect(),
        ..Default::default()
    }
}

fn admit(catalog: ComponentSchemaCatalog) -> ComponentSchemaOwnerSnapshot {
    ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new("game.behaviors").unwrap(),
        catalog,
        None,
    )
    .unwrap()
}

#[test]
fn code_authored_owner_is_canonical_without_native_registration() {
    let owner = admit(catalog(&["game.Sensor", "game.Mover"]));
    assert_eq!(owner.owner_id().as_str(), "game.behaviors");
    assert_eq!(owner.current().components()[0].id().as_str(), "game.Mover");
    assert!(owner.schema(&ComponentTypeId::new("game.Sensor")).is_some());
    assert!(
        owner
            .schema(&ComponentTypeId::new("game.Missing"))
            .is_none()
    );
    assert!(owner.predecessor().is_none());
}

#[test]
fn equivalent_content_does_not_forge_owner_admission_identity() {
    let first = admit(catalog(&["game.Mover"]));
    let retained = first.clone();
    let foreign = admit(first.current().clone());
    assert!(first.ptr_eq(&retained));
    assert_eq!(
        first.current().fingerprint(),
        foreign.current().fingerprint()
    );
    assert!(!first.ptr_eq(&foreign));
    drop(first);
    assert!(
        retained
            .schema(&ComponentTypeId::new("game.Mover"))
            .is_some()
    );
}

#[test]
fn owner_admission_reuses_catalog_validation() {
    let error = ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new("game.behaviors").unwrap(),
        catalog(&["game.Mover", "game.Mover"]),
        None,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ComponentRegistryError::DuplicateComponentId(_)
    ));
}

#[test]
fn owner_admission_checks_lineage_and_retains_the_predecessor() {
    let first = admit(catalog(&["game.Mover"]));
    let successor = ComponentSchemaCatalog {
        generation: 2,
        predecessor: Some(first.current().fingerprint()),
        components: first.current().components().to_vec(),
        type_tombstones: Vec::new(),
    };
    let admitted = ComponentSchemaOwnerSnapshot::admit(
        first.owner_id().clone(),
        successor.clone(),
        Some(first.current().clone()),
    )
    .unwrap();
    assert_eq!(admitted.predecessor(), Some(first.current()));
    assert!(!first.ptr_eq(&admitted));
    assert!(matches!(
        ComponentSchemaOwnerSnapshot::admit(first.owner_id().clone(), successor, None),
        Err(ComponentRegistryError::InvalidCatalogGeneration { .. })
    ));
}

#[test]
fn invalid_deep_shapes_reject_before_creating_a_snapshot() {
    let mut input = catalog(&["game.Deep"]);
    let mut shape = ComponentValueShape::scalar(ComponentValueKind::Bool);
    for _ in 0..512 {
        shape = ComponentValueShape::list(shape);
    }
    input.components[0]
        .fields
        .push(ComponentFieldSchema::required(
            ComponentFieldId::new("data"),
            "Data",
            ComponentFieldPath::from_fields(["data"]),
            shape,
        ));
    assert!(matches!(
        ComponentSchemaOwnerSnapshot::admit(
            PersistentComponentSchemaOwnerId::try_new("game.behaviors").unwrap(), input, None
        ),
        Err(ComponentRegistryError::InvalidComponentFieldShape { source, .. })
            if source.kind == ComponentShapeErrorKind::DepthLimit
    ));
}

#[cfg(feature = "serde")]
#[test]
fn bounded_file_admission_exposes_the_same_backend_free_owner() {
    use nara_reflect::{AuthoringSchemaBundle, AuthoringSchemaBundleOwner};

    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.behaviors").unwrap(),
        catalog(&["game.Mover"]),
        None,
    )])
    .unwrap();
    let retained = bundle.owners()[0].clone();
    assert!(retained.ptr_eq(bundle.owner(retained.owner_id()).unwrap()));
    let reopened =
        AuthoringSchemaBundle::from_json_bytes(bundle.to_json_string().unwrap().as_bytes())
            .unwrap();
    assert_eq!(bundle, reopened);
    assert!(!retained.ptr_eq(&reopened.owners()[0]));
}

#[cfg(feature = "serde")]
#[test]
fn bundle_content_equality_includes_lineage_not_only_the_fingerprint() {
    use nara_reflect::{AuthoringSchemaBundle, AuthoringSchemaBundleOwner};

    let owner = PersistentComponentSchemaOwnerId::try_new("game.behaviors").unwrap();
    let first_catalog = catalog(&["game.Mover"]);
    let successor = ComponentSchemaCatalog {
        generation: 2,
        predecessor: Some(first_catalog.fingerprint()),
        components: first_catalog.components.clone(),
        type_tombstones: Vec::new(),
    };
    let first = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        first_catalog.clone(),
        None,
    )])
    .unwrap();
    let next = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner,
        successor,
        Some(first_catalog),
    )])
    .unwrap();
    assert_eq!(first.fingerprint(), next.fingerprint());
    assert_ne!(first, next);
}
