use super::*;

const OWNER: &str = "test.owner.creation";
const COMPONENT: &str = "test.owner.CreationValue";
const OTHER_COMPONENT: &str = "test.owner.CreationOther";

fn value(initial: u64) -> ComponentValue {
    ComponentValue::map([("value", ComponentValue::U64(initial))])
}

fn successor_catalog(previous: &ComponentSchemaCatalog, initial: u64) -> ComponentSchemaCatalog {
    let mut current = ComponentSchemaCatalog::successor_of(previous).unwrap();
    current.components = previous.components().to_vec();
    current
        .components
        .iter_mut()
        .find(|component| component.id().as_str() == COMPONENT)
        .unwrap()
        .authoring_initial_value = Some(value(initial));
    current
}

fn successor(
    previous: &ComponentSchemaOwnerSnapshot,
    initial: u64,
) -> ComponentSchemaOwnerSnapshot {
    ComponentSchemaOwnerSnapshot::admit(
        previous.owner_id().clone(),
        successor_catalog(previous.current(), initial),
        Some(previous.current().clone()),
    )
    .unwrap()
}

fn bundle() -> AuthoringSchemaBundle {
    let first = ComponentSchemaOwnerSnapshot::admit(
        owner_id(OWNER),
        ComponentSchemaCatalog {
            components: vec![
                schema(COMPONENT)
                    .with_field_tombstones([ComponentFieldId::new("retired")])
                    .with_authoring_initial_value(value(7)),
                schema(OTHER_COMPONENT).with_authoring_initial_value(value(19)),
            ],
            type_tombstones: vec![ComponentTypeId::new("test.owner.CreationRetired")],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    // Start from an already evolved owner so replacement must preserve complete lineage.
    AuthoringSchemaBundle::from_owner_snapshots(
        initial_bundle()
            .owners()
            .iter()
            .cloned()
            .chain([successor(&first, 11)]),
    )
    .unwrap()
}

fn prepare_creation(
    incumbent: &Arc<AuthoringSchemaReceipt>,
    incoming: &ComponentSchemaOwnerSnapshot,
) -> (PreparedAuthoringSchemaOwner, FilePublicationHost) {
    match incumbent
        .prepare_owner(incoming, ComponentSchemaOwnerChange::CreationMetadata)
        .unwrap()
    {
        AuthoringSchemaOwnerPreparation::Prepared { owner, publication } => (owner, publication),
        AuthoringSchemaOwnerPreparation::AlreadyAdmitted => panic!("the successor is not admitted"),
    }
}

fn source(basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("root").unwrap(),
    )
    .with_component(
        ComponentTypeId::new(COMPONENT),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value(5))
            .with_owner(owner_id(OWNER)),
    )]);
    SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("scenes/creation.json").unwrap()),
                basis,
            )
            .unwrap()
            .publish(),
    )
}

#[test]
fn immediate_creation_successor_preserves_definitions_other_owners_and_actual_leaf_authority() {
    let project = Project::new(&bundle());
    let incumbent = project.admit(Default::default());
    let before = project.disk();
    let previous = incumbent.bundle().owner(&owner_id(OWNER)).unwrap();
    assert_eq!(previous.current().generation(), 2);
    let incoming = successor(previous, 23);
    let previous_adapters = project_authoring_adapters(incumbent.bundle()).unwrap();
    let previous_basis =
        AuthoringSchemaBasis::admit(incumbent.bundle(), &previous_adapters).unwrap();
    let (prepared, mut publication) = prepare_creation(&incumbent, &incoming);
    assert!(Arc::ptr_eq(prepared.incumbent(), &incumbent));
    assert!(Arc::ptr_eq(prepared.observation(), &incumbent));
    assert!(matches!(
        prepared.reclassification_kind(),
        SchemaReclassificationKind::CreationMetadata { owner } if owner == previous.owner_id()
    ));
    assert_eq!(
        prepared.bundle().owners().len(),
        incumbent.bundle().owners().len()
    );
    let next = prepared.bundle().owner(previous.owner_id()).unwrap();
    assert_eq!(next.current(), incoming.current());
    assert_eq!(next.predecessor(), Some(previous.current()));
    assert_eq!(
        next.current().generation(),
        previous.current().generation() + 1
    );
    assert_eq!(
        next.current().predecessor(),
        Some(&previous.current().fingerprint())
    );
    assert_ne!(
        next.current().fingerprint(),
        previous.current().fingerprint()
    );
    assert_eq!(
        next.current().type_tombstones(),
        previous.current().type_tombstones()
    );
    for previous_schema in previous.current().components() {
        let mut definition = next.schema(previous_schema.id()).unwrap().clone();
        definition.authoring_initial_value = previous_schema.authoring_initial_value().cloned();
        assert_eq!(&definition, previous_schema);
    }
    assert_eq!(
        next.schema(&ComponentTypeId::new(COMPONENT))
            .unwrap()
            .authoring_initial_value(),
        Some(&value(23))
    );
    for other in incumbent.bundle().owners() {
        if other.owner_id() != previous.owner_id() {
            let retained = prepared.bundle().owner(other.owner_id()).unwrap();
            assert!(retained.ptr_eq(other));
            assert_eq!(retained.current(), other.current());
            assert_eq!(retained.predecessor(), other.predecessor());
        }
    }

    let old_adapter =
        AuthoringAdapterReceipt::from_schema_owner(previous, &Default::default()).unwrap();
    let next_adapter =
        AuthoringAdapterReceipt::from_schema_owner(next, &Default::default()).unwrap();
    assert!(previous_adapters.contains(&old_adapter));
    assert!(prepared.adapters().contains(&next_adapter));
    assert!(!prepared.adapters().contains(&old_adapter));
    assert_ne!(next_adapter, old_adapter);
    assert_eq!(next_adapter.identity(), old_adapter.identity());
    assert_eq!(
        next_adapter.adapter_generation(),
        old_adapter.adapter_generation()
    );
    assert_eq!(
        next_adapter.components().collect::<Vec<_>>(),
        old_adapter.components().collect::<Vec<_>>()
    );
    assert_eq!(prepared.adapters().len(), previous_adapters.len());
    for adapter in previous_adapters
        .iter()
        .filter(|adapter| *adapter != &old_adapter)
    {
        assert!(prepared.adapters().contains(adapter));
    }
    // Exact catalog evidence matters even though the Adapter implementation is unchanged.
    let stale_basis = AuthoringSchemaBasis::admit(prepared.bundle(), &previous_adapters).unwrap();
    let stale_source = source(&stale_basis);
    assert_eq!(
        stale_source.authoring_record_facts().unwrap()[0].readiness(),
        AuthoringRecordReadiness::KnownUnbound
    );
    let previous_source = source(&previous_basis);
    let next_source = source(prepared.schema_basis());
    let previous_fact = &previous_source.authoring_record_facts().unwrap()[0];
    let next_fact = &next_source.authoring_record_facts().unwrap()[0];
    assert_eq!(
        previous_fact.readiness(),
        AuthoringRecordReadiness::Available
    );
    assert_eq!(next_fact.readiness(), AuthoringRecordReadiness::Available);
    assert_ne!(next_fact.adapter(), previous_fact.adapter());
    assert_eq!(
        next_fact.adapter().unwrap().identity(),
        previous_fact.adapter().unwrap().identity()
    );
    assert!(!next_fact.adapter().unwrap().supports_native_preview());
    assert!(next_fact.adapter().unwrap().contribution().is_none());
    assert_eq!(next_source.document(), previous_source.document());
    prepared.validate().unwrap();
    assert_eq!(project.disk(), before);
    assert_eq!(incumbent.bundle_identity(), before.0);
    assert_eq!(incumbent.bundle_bytes(), before.1);
    assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);

    let basis = prepared.schema_basis().clone();
    let bytes = prepared.bytes();
    let reviewed_bundle = Arc::clone(&prepared.bundle);
    let accepted = publish(&mut publication, &bytes);
    let identity = accepted.source().identity();
    assert_ne!(identity, before.0);
    let admitted = prepared.attach(accepted).unwrap();
    assert_eq!(admitted.basis, basis);
    assert!(Arc::ptr_eq(&admitted.schema.bundle.bytes, &bytes));
    assert!(Arc::ptr_eq(
        &admitted.schema.bundle.bundle,
        &reviewed_bundle
    ));
    assert_eq!(admitted.schema.bundle_identity(), identity);
    assert_eq!(
        admitted.schema.bundle_parent_identity(),
        incumbent.bundle_parent_identity()
    );
    assert_eq!(
        admitted.schema.manifest_identity(),
        incumbent.manifest_identity()
    );
    assert_eq!(admitted.schema.root_identity(), incumbent.root_identity());
    assert_eq!(
        admitted.schema.attempt_generation(),
        incumbent.attempt_generation()
    );
    assert_eq!(
        admitted.publication.receipt().previous_identity(),
        Some(before.0)
    );
    assert_eq!(
        admitted.publication.receipt().published_identity(),
        Some(identity)
    );
    assert_eq!(project.disk(), (identity, bytes.to_vec()));
    assert_eq!(
        incumbent
            .bundle
            .capability()
            .read_to_end_bounded(bundle_byte_limit())
            .unwrap(),
        before.1
    );
}

#[test]
fn additive_intent_and_creation_intent_for_a_missing_owner_refuse_without_writing() {
    let project = Project::new(&bundle());
    let incumbent = project.admit(Default::default());
    let before = project.disk();
    let incoming = successor(incumbent.bundle().owner(&owner_id(OWNER)).unwrap(), 23);
    assert!(matches!(
        incumbent.prepare_owner(&incoming, ComponentSchemaOwnerChange::Add),
        Err(AuthoringSchemaOwnerError::ExistingOwnerChanged)
    ));
    assert!(matches!(
        incumbent.prepare_observed_owner(&incoming, ComponentSchemaOwnerChange::Add),
        Err(AuthoringSchemaOwnerError::ExistingOwnerChanged)
    ));
    for missing in [generation_one(NEW_OWNER, NEW_COMPONENT), evolved(NEW_OWNER)] {
        assert!(matches!(
            incumbent.prepare_owner(&missing, ComponentSchemaOwnerChange::CreationMetadata),
            Err(AuthoringSchemaOwnerError::InvalidCreationSuccessor)
        ));
        assert!(matches!(
            incumbent
                .prepare_observed_owner(&missing, ComponentSchemaOwnerChange::CreationMetadata),
            Err(AuthoringSchemaOwnerError::InvalidCreationSuccessor)
        ));
        assert_eq!(project.disk(), before);
    }
    assert_eq!(incumbent.bundle_identity(), before.0);
    assert_eq!(incumbent.bundle_bytes(), before.1);
    assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
}

#[test]
fn self_consistent_wrong_predecessors_and_skipped_generations_refuse_the_actual_owner() {
    let project = Project::new(&bundle());
    let incumbent = project.admit(Default::default());
    let before = project.disk();
    let previous = incumbent.bundle().owner(&owner_id(OWNER)).unwrap();
    let mut different_content = previous.current().clone();
    different_content.components[0].authoring_initial_value = Some(value(97));
    let mut different_lineage = previous.current().clone();
    different_lineage.predecessor = Some(different_content.fingerprint());
    assert_eq!(
        different_lineage.fingerprint(),
        previous.current().fingerprint()
    );
    assert_ne!(&different_lineage, previous.current());
    let mut incoming = [different_content, different_lineage]
        .map(|predecessor| {
            ComponentSchemaOwnerSnapshot::admit(
                previous.owner_id().clone(),
                successor_catalog(&predecessor, 23),
                Some(predecessor),
            )
            .unwrap()
        })
        .to_vec();
    incoming.push(successor(&successor(previous, 23), 29));
    for candidate in incoming {
        assert_ne!(candidate.predecessor(), Some(previous.current()));
        assert!(matches!(
            incumbent.prepare_owner(&candidate, ComponentSchemaOwnerChange::CreationMetadata),
            Err(AuthoringSchemaOwnerError::InvalidCreationSuccessor)
        ));
        assert!(matches!(
            incumbent
                .prepare_observed_owner(&candidate, ComponentSchemaOwnerChange::CreationMetadata),
            Err(AuthoringSchemaOwnerError::InvalidCreationSuccessor)
        ));
        assert_eq!(project.disk(), before);
        assert_eq!(incumbent.bundle_identity(), before.0);
        assert_eq!(incumbent.bundle_bytes(), before.1);
    }
    assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
}

#[test]
fn declaration_changes_and_generation_only_successors_refuse_before_publication() {
    let project = Project::new(&bundle());
    let incumbent = project.admit(Default::default());
    let before = project.disk();
    let previous = incumbent.bundle().owner(&owner_id(OWNER)).unwrap();
    for change in [
        "generation",
        "alias",
        "field-alias",
        "version",
        "field",
        "shape",
        "tombstone",
    ] {
        let mut current = successor_catalog(previous.current(), 23);
        let selected = current
            .components
            .iter_mut()
            .find(|component| component.id().as_str() == COMPONENT)
            .unwrap();
        match change {
            "generation" => {
                selected.authoring_initial_value = previous
                    .schema(&ComponentTypeId::new(COMPONENT))
                    .unwrap()
                    .authoring_initial_value()
                    .cloned();
            }
            "alias" => selected.aliases.push("Renamed component".into()),
            "field-alias" => selected.fields[0].aliases.push("Renamed field".into()),
            "version" => selected.version = ComponentSchemaVersion::new(2).unwrap(),
            "field" => {
                selected.version = ComponentSchemaVersion::new(2).unwrap();
                selected.fields.push(
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("added"),
                        "Added",
                        ComponentFieldPath::from_fields(["added"]),
                        ComponentValueKind::U64,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                );
                selected.authoring_initial_value = Some(ComponentValue::map([
                    ("value", ComponentValue::U64(23)),
                    ("added", ComponentValue::U64(1)),
                ]));
            }
            "shape" => {
                selected.version = ComponentSchemaVersion::new(2).unwrap();
                selected.fields[0].shape = ComponentValueKind::String.into();
                selected.authoring_initial_value = Some(ComponentValue::map([(
                    "value",
                    ComponentValue::String("changed".into()),
                )]));
            }
            "tombstone" => selected
                .field_tombstones
                .push(ComponentFieldId::new("another-retired")),
            _ => unreachable!(),
        }
        // These are valid catalogs; creation-only admission owns this stronger refusal.
        let incoming = ComponentSchemaOwnerSnapshot::admit(
            previous.owner_id().clone(),
            current,
            Some(previous.current().clone()),
        )
        .unwrap();
        assert!(
            matches!(
                incumbent.prepare_owner(&incoming, ComponentSchemaOwnerChange::CreationMetadata),
                Err(AuthoringSchemaOwnerError::InvalidCreationSuccessor)
            ),
            "unsupported creation change: {change}"
        );
        assert_eq!(
            project.disk(),
            before,
            "unsupported creation change: {change}"
        );
    }
    assert_eq!(incumbent.bundle_identity(), before.0);
    assert_eq!(incumbent.bundle_bytes(), before.1);
    assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
}

#[test]
fn observed_creation_successor_reuses_the_actual_leaf_and_preserves_exact_noop_bytes() {
    let project = Project::new(&bundle());
    let incumbent = project.admit(Default::default());
    let before = project.disk();
    let incoming = successor(incumbent.bundle().owner(&owner_id(OWNER)).unwrap(), 23);
    let AuthoringSchemaOwnerReconciliation::Publication(
        AuthoringSchemaOwnerPreparation::Prepared {
            owner: original,
            mut publication,
        },
    ) = incumbent
        .prepare_observed_owner(&incoming, ComponentSchemaOwnerChange::CreationMetadata)
        .unwrap()
    else {
        panic!("the incumbent leaf requires ordinary publication");
    };
    let original_basis = original.schema_basis().clone();
    let canonical_bytes = original.bytes();
    assert_eq!(project.disk(), before);
    let original = match original.attach_observed() {
        Err(prepared) => prepared,
        Ok(_) => panic!("an incumbent observation cannot attach as the successor"),
    };
    let mut actual_bytes = canonical_bytes.to_vec();
    actual_bytes.extend_from_slice(b"\n \t");
    let accepted = publish(&mut publication, &actual_bytes);
    let identity = accepted.source().identity();
    let digest = accepted.source().digest();
    assert_ne!(identity, before.0);
    let failure = match original.attach(accepted) {
        Err(failure) => failure,
        Ok(_) => panic!("different bytes must retain the actual publication for reconciliation"),
    };
    assert_eq!(failure.prepared.schema_basis(), &original_basis);
    assert!(matches!(
        failure.prepared.validate(),
        Err(AuthoringSchemaOwnerError::BundleChanged)
    ));
    assert!(matches!(
        incumbent.prepare_owner(&incoming, ComponentSchemaOwnerChange::CreationMetadata),
        Err(AuthoringSchemaOwnerError::BundleChanged)
    ));
    let (evidence, published_leaf) = failure.publication.into_parts();
    let AuthoringSchemaOwnerReconciliation::Observed(observed) = incumbent
        .prepare_observed_owner(&incoming, ComponentSchemaOwnerChange::CreationMetadata)
        .unwrap()
    else {
        panic!("the published successor requires a fresh observation");
    };
    assert!(Arc::ptr_eq(observed.incumbent(), &incumbent));
    assert!(!Arc::ptr_eq(observed.incumbent(), observed.observation()));
    assert_ne!(observed.schema_basis(), &original_basis);
    assert_eq!(observed.observation().bundle_identity(), identity);
    assert_eq!(observed.observation().bundle_digest(), digest);
    assert_eq!(observed.bytes().as_ref(), actual_bytes);
    assert_ne!(observed.bytes(), canonical_bytes);
    assert!(Arc::ptr_eq(
        &observed.bytes(),
        &observed.observation().bundle.bytes
    ));
    assert!(matches!(
        observed.reclassification_kind(),
        SchemaReclassificationKind::CreationMetadata { owner } if owner == incoming.owner_id()
    ));
    observed.validate().unwrap();
    let actual_observation = Arc::clone(observed.observation());
    let basis = observed.schema_basis().clone();
    let attached = observed.attach_observed().unwrap();
    assert!(Arc::ptr_eq(&attached.schema, &actual_observation));
    assert_eq!(attached.basis, basis);
    assert_eq!(attached.schema.bundle_identity(), identity);
    assert_eq!(attached.schema.bundle_bytes(), actual_bytes);
    assert_eq!(evidence.receipt().previous_identity(), Some(before.0));
    assert_eq!(evidence.receipt().published_identity(), Some(identity));
    assert_eq!(evidence.receipt().verified_content_digest(), Some(digest));
    assert_eq!(published_leaf.identity(), identity);

    let equal = ComponentSchemaOwnerSnapshot::admit(
        incoming.owner_id().clone(),
        incoming.current().clone(),
        incoming.predecessor().cloned(),
    )
    .unwrap();
    assert!(!equal.ptr_eq(&incoming));
    for change in [
        ComponentSchemaOwnerChange::Add,
        ComponentSchemaOwnerChange::CreationMetadata,
    ] {
        assert!(matches!(
            attached.schema.prepare_owner(&equal, change),
            Ok(AuthoringSchemaOwnerPreparation::AlreadyAdmitted)
        ));
        assert!(matches!(
            attached.schema.prepare_observed_owner(&equal, change),
            Ok(AuthoringSchemaOwnerReconciliation::Publication(
                AuthoringSchemaOwnerPreparation::AlreadyAdmitted
            ))
        ));
        assert_eq!(project.disk(), (identity, actual_bytes.clone()));
        assert_eq!(attached.schema.bundle_bytes(), actual_bytes);
    }
    assert_eq!(incumbent.bundle_identity(), before.0);
    assert_eq!(incumbent.bundle_bytes(), before.1);
    assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
    drop(publication);
    fs::rename(
        project.0.join(BUNDLE),
        project.0.join("schema/held.bundle.json"),
    )
    .unwrap();
    fs::remove_file(project.0.join("schema/held.bundle.json")).unwrap();
    assert_eq!(
        attached
            .schema
            .bundle
            .capability()
            .read_to_end_bounded(bundle_byte_limit())
            .unwrap(),
        actual_bytes
    );
    attached
        .schema
        .bundle
        .capability()
        .verify_digest(digest, bundle_byte_limit())
        .unwrap();
}

#[test]
fn creation_successor_cannot_hide_another_owners_changed_complete_snapshot() {
    for dropped in [false, true] {
        let project = Project::new(&bundle());
        let incumbent = project.admit(Default::default());
        let before = project.disk();
        let incoming = successor(incumbent.bundle().owner(&owner_id(OWNER)).unwrap(), 23);
        let (prepared, mut publication) = prepare_creation(&incumbent, &incoming);
        let other = prepared
            .bundle()
            .owner(&owner_id("test.owner.evolved-a"))
            .unwrap();
        let mut predecessor = other.predecessor().unwrap().clone();
        predecessor.components[0]
            .aliases
            .push("Changed historical alias".into());
        let mut current = other.current().clone();
        current.predecessor = Some(predecessor.fingerprint());
        let changed = ComponentSchemaOwnerSnapshot::admit(
            other.owner_id().clone(),
            current,
            Some(predecessor),
        )
        .unwrap();
        assert_eq!(changed.current().components(), other.current().components());
        assert_ne!(changed.predecessor(), other.predecessor());
        let unexpected = AuthoringSchemaBundle::from_owner_snapshots(
            prepared.bundle().owners().iter().filter_map(|owner| {
                if owner.owner_id() != other.owner_id() {
                    Some(owner.clone())
                } else if dropped {
                    None
                } else {
                    Some(changed.clone())
                }
            }),
        )
        .unwrap();
        let bytes = unexpected.to_json_string().unwrap().into_bytes();
        let accepted = publish(&mut publication, &bytes);
        let identity = accepted.source().identity();
        assert_ne!(identity, before.0);
        assert!(matches!(
            incumbent
                .prepare_observed_owner(&incoming, ComponentSchemaOwnerChange::CreationMetadata),
            Err(AuthoringSchemaOwnerError::BundleChanged)
        ));
        assert_eq!(project.disk(), (identity, bytes));
        assert_eq!(incumbent.bundle_identity(), before.0);
        assert_eq!(incumbent.bundle_bytes(), before.1);
        assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
    }
}

#[test]
fn creation_successor_respects_the_incumbent_encoded_budget_without_writing() {
    let project = Project::new(&bundle());
    let before = project.disk();
    let maximum = fs::read(project.0.join("nara.toml")).unwrap().len() + before.1.len();
    let incumbent = project.admit(AuthoringProjectAdmissionLimits::new(
        ByteLimit::new(maximum).unwrap(),
    ));
    let incoming = successor(incumbent.bundle().owner(&owner_id(OWNER)).unwrap(), 23);
    assert!(matches!(
        incumbent.prepare_owner(&incoming, ComponentSchemaOwnerChange::CreationMetadata),
        Err(AuthoringSchemaOwnerError::Budget(
            AuthoringProjectAdmissionError::AggregateEncodedBytesExceeded { maximum: observed }
        )) if observed == maximum
    ));
    assert_eq!(project.disk(), before);
    assert_eq!(incumbent.bundle_identity(), before.0);
    assert_eq!(incumbent.bundle_bytes(), before.1);
    assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
}
