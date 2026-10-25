use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringReadError,
    AuthoringRecordReadiness, PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate,
    SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
    SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
};
use nara_tooling::{
    EditorAuthoringPage, EditorAuthoringProjection, EditorAuthoringQuery,
    EditorAuthoringQueryLimits, EditorAuthoringQueryPosition, EditorAuthoringQueryTarget,
    EditorAuthoringRow, EditorDocumentId, EditorDocumentQueryBasis, EditorDocumentQueryRejection,
    EditorDocumentSourceBinding, EditorRootRead, EditorRootReadLimits, EditorRootTarget,
    EditorRootValue, EditorWorkspace, EditorWorkspaceCommand,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn fixture(prefab: bool) -> (EditorWorkspace, EditorDocumentId) {
    fixture_size(prefab, 0)
}

fn tag_schema() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("game.Tag"),
        "Tag",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
}

fn tag_catalog() -> Result<ComponentSchemaCatalog, nara_reflect::ComponentSchemaProviderSourceError>
{
    Ok(ComponentSchemaCatalog {
        components: vec![tag_schema()],
        ..Default::default()
    })
}

fn register_tag_catalog(
    registry: &mut nara_reflect::ComponentRegistry,
) -> Result<(), nara_reflect::ComponentRegistryError> {
    registry.register_component_schema(tag_schema())?;
    Ok(())
}

fn fixture_size(prefab: bool, extra_entities: usize) -> (EditorWorkspace, EditorDocumentId) {
    let field = |name: &str, kind| {
        ComponentFieldSchema::required(
            ComponentFieldId::new(name),
            name,
            ComponentFieldPath::from_fields([name]),
            kind,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    };
    let schema = ComponentSchema::new(
        ComponentTypeId::new("game.Data"),
        "Data",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        field("amount", ComponentValueKind::U64),
        field("blob", ComponentValueKind::String),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("defaulted"),
            "Defaulted",
            ComponentFieldPath::from_fields(["defaulted"]),
            ComponentValueKind::U64,
            ComponentValue::U64(11),
        ),
        ComponentFieldSchema::optional(
            ComponentFieldId::new("missing"),
            "Missing",
            ComponentFieldPath::from_fields(["missing"]),
            ComponentValueKind::U64,
        ),
        ComponentFieldSchema::optional(
            ComponentFieldId::new("nullable"),
            "Nullable",
            ComponentFieldPath::from_fields(["nullable"]),
            ComponentValueKind::U64,
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("null_default"),
            "Null default",
            ComponentFieldPath::from_fields(["null_default"]),
            ComponentValueKind::U64,
            ComponentValue::Null,
        ),
        ComponentFieldSchema::required(
            ComponentFieldId::new("tree"),
            "Tree",
            ComponentFieldPath::from_fields(["tree"]),
            ComponentValueShape::list(ComponentValueShape::scalar(ComponentValueKind::U64)),
        ),
    ]);
    let tag = nara_reflect::__private::resolve_schema_provider(
        nara_reflect::ComponentSchemaProviderDefinition::new(
            nara_reflect::ComponentSchemaOwnerId::new("game.tags"),
            nara_app::PluginSchemaProviderId::new("game.tags.provider"),
            nara_reflect::ComponentSchemaProviderBindingId::new("game.tags.authoring", 1),
            tag_catalog,
            register_tag_catalog,
        ),
    )
    .unwrap();
    let bundle = AuthoringSchemaBundle::new([
        AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("game.reads").unwrap(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        ),
        AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("game.tags").unwrap(),
            tag.current().catalog().clone(),
            None,
        ),
    ])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    let adapters = [
        adapter,
        AuthoringAdapterReceipt::from_trusted_provider(
            tag.contribution_receipt(),
            "game.tags.authoring-adapter",
            1,
            1,
            [ComponentTypeId::new("game.Tag")],
        )
        .unwrap(),
    ];
    let mut records = vec![
        SceneEntityRecord::new(id("item"))
            .with_component(
                ComponentTypeId::new("game.Data"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([
                        ("amount", ComponentValue::U64(7)),
                        ("blob", ComponentValue::String("x".repeat(10_000))),
                        ("nullable", ComponentValue::Null),
                        (
                            "tree",
                            ComponentValue::List(vec![
                                ComponentValue::U64(1),
                                ComponentValue::U64(2),
                            ]),
                        ),
                    ]),
                ),
            )
            .with_component(
                ComponentTypeId::new("game.Unknown"),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
            ),
        SceneEntityRecord::new(id("empty")),
        SceneEntityRecord::new(id("tag")).with_component(
            ComponentTypeId::new("game.Tag"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([] as [(&str, ComponentValue); 0]),
            ),
        ),
    ];
    records.extend(
        (0..extra_entities).map(|index| SceneEntityRecord::new(id(&format!("empty-{index:05}")))),
    );
    let source = AuthoringDocumentSource::new(AssetRef::path("scenes/read.scene.json").unwrap());
    let mut workspace = EditorWorkspace::new();
    let opened = if prefab {
        let published = PrefabDocumentCandidate::decode_json_bytes(
            PrefabDocument::from_ordered_roots(records)
                .to_json_string()
                .unwrap()
                .as_bytes(),
        )
        .unwrap()
        .admit_authoring(source, &bundle, &adapters)
        .unwrap()
        .publish();
        workspace
            .open_prefab_session(
                "read",
                EditorDocumentSourceBinding::new("prefabs/read.prefab.json"),
                PrefabAuthoringSession::from_published_authoring_document(published),
            )
            .unwrap()
            .opened_document
            .unwrap()
    } else {
        let published = SceneDocumentCandidate::decode_json_bytes(
            SceneDocument::from_ordered_roots(records)
                .to_json_string()
                .unwrap()
                .as_bytes(),
        )
        .unwrap()
        .admit_authoring(source, &bundle, &adapters)
        .unwrap()
        .publish();
        workspace
            .open_scene_session(
                "read",
                SceneAuthoringSession::from_published_authoring_document(published),
            )
            .unwrap()
            .opened_document
            .unwrap()
    };
    (workspace, opened)
}

fn basis(workspace: &EditorWorkspace, document: EditorDocumentId) -> EditorDocumentQueryBasis {
    let model = workspace.authoring_model();
    let tab = model.document(document).unwrap();
    EditorDocumentQueryBasis {
        document,
        revision: tab.revision,
        schema: tab.authoring_schema_basis.clone(),
    }
}
fn query(
    workspace: &EditorWorkspace,
    document: EditorDocumentId,
    projection: EditorAuthoringProjection,
) -> EditorAuthoringQuery {
    EditorAuthoringQuery {
        position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
            basis: basis(workspace, document),
            entity: id("item"),
            projection,
        }),
        limits: EditorAuthoringQueryLimits::new(2, 1, 1024).unwrap(),
    }
}
fn roots() -> EditorAuthoringProjection {
    EditorAuthoringProjection::Roots {
        component: ComponentTypeId::new("game.Data"),
    }
}
fn read(
    workspace: &EditorWorkspace,
    document: EditorDocumentId,
    field: &str,
    bytes: usize,
) -> EditorRootRead {
    EditorRootRead {
        target: EditorRootTarget {
            basis: basis(workspace, document),
            entity: id("item"),
            component: ComponentTypeId::new("game.Data"),
            field: ComponentFieldId::new(field),
        },
        limits: EditorRootReadLimits::new(1024, 32, bytes).unwrap(),
    }
}
fn next(workspace: &EditorWorkspace, page: EditorAuthoringPage) -> EditorAuthoringPage {
    workspace
        .query_document_authoring(EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Continue(page.next.unwrap()),
            limits: EditorAuthoringQueryLimits::new(1, 1, 1024).unwrap(),
        })
        .unwrap()
}

#[test]
fn discovery_pages_actual_components_and_roots_without_reading_values_or_changing_state() {
    for prefab in [false, true] {
        let (mut workspace, document) = fixture(prefab);
        let active = workspace
            .open_scene_session(
                "active",
                SceneAuthoringSession::new(SceneDocument::default()),
            )
            .unwrap()
            .opened_document
            .unwrap();
        let before = workspace.authoring_model();
        let first = workspace
            .query_document_authoring(query(
                &workspace,
                document,
                EditorAuthoringProjection::Components,
            ))
            .unwrap();
        assert_eq!(first.scanned_entries, 1);
        assert_eq!(
            first.metadata_bytes, 26,
            "page item + Data row + cursor item + Data anchor"
        );
        let mut scan_bound = query(&workspace, document, EditorAuthoringProjection::Components);
        scan_bound.limits = EditorAuthoringQueryLimits::new(1, 2, 1024).unwrap();
        assert_eq!(
            workspace.query_document_authoring(scan_bound).unwrap(),
            first
        );
        assert!(
            matches!(&first.rows[..], [EditorAuthoringRow::Component { component, readiness: AuthoringRecordReadiness::Available, .. }] if component.as_str() == "game.Data")
        );
        let second = next(&workspace, first.clone());
        assert!(
            matches!(&second.rows[..], [EditorAuthoringRow::Component { component, readiness: AuthoringRecordReadiness::UnknownSchema, .. }] if component.as_str() == "game.Unknown")
        );
        assert!(second.next.is_none());
        assert_eq!(second, next(&workspace, first));
        let mut page = workspace
            .query_document_authoring(query(&workspace, document, roots()))
            .unwrap();
        let mut fields = Vec::new();
        loop {
            assert_eq!(page.scanned_entries, 1);
            assert!(matches!(&page.rows[..], [EditorAuthoringRow::Root { .. }]));
            let EditorAuthoringRow::Root { field, .. } = &page.rows[0] else {
                unreachable!()
            };
            fields.push(field.as_str().to_owned());
            if page.next.is_none() {
                break;
            }
            page = next(&workspace, page);
        }
        assert_eq!(
            fields,
            [
                "amount",
                "blob",
                "defaulted",
                "missing",
                "null_default",
                "nullable",
                "tree"
            ]
        );
        for (entity, projection) in [
            ("empty", EditorAuthoringProjection::Components),
            (
                "tag",
                EditorAuthoringProjection::Roots {
                    component: ComponentTypeId::new("game.Tag"),
                },
            ),
        ] {
            let mut empty = query(&workspace, document, projection);
            let EditorAuthoringQueryPosition::Start(target) = &mut empty.position else {
                unreachable!()
            };
            target.entity = id(entity);
            let page = workspace.query_document_authoring(empty).unwrap();
            assert!(page.rows.is_empty());
            assert_eq!(page.scanned_entries, 0);
            assert!(page.next.is_none());
        }
        assert_eq!(workspace.authoring_model(), before);
        assert_eq!(workspace.active_document(), Some(active));
    }
}

#[test]
fn selected_root_reads_preserve_origin_and_enforce_exact_payload_and_metadata_budgets() {
    for prefab in [false, true] {
        let (workspace, document) = fixture(prefab);
        let before = workspace.authoring_model();
        let amount = workspace
            .read_document_root(read(&workspace, document, "amount", 8))
            .unwrap();
        assert_eq!(
            amount.value,
            EditorRootValue::Present(ComponentValue::U64(7))
        );
        assert_eq!((amount.value_nodes, amount.value_logical_bytes), (1, 8));
        assert_eq!(amount.metadata_bytes, 19, "item + game.Data + amount");
        assert_eq!(
            workspace.read_document_root(read(&workspace, document, "amount", 7)),
            Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
        );
        assert_eq!(
            workspace.read_document_root(read(&workspace, document, "blob", 1024)),
            Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
        );
        assert_eq!(
            workspace
                .read_document_root(read(&workspace, document, "defaulted", 8))
                .unwrap()
                .value,
            EditorRootValue::Defaulted(ComponentValue::U64(11))
        );
        assert_eq!(
            workspace
                .read_document_root(read(&workspace, document, "missing", 8))
                .unwrap()
                .value,
            EditorRootValue::Missing
        );
        assert_eq!(
            workspace
                .read_document_root(read(&workspace, document, "nullable", 8))
                .unwrap()
                .value,
            EditorRootValue::Present(ComponentValue::Null)
        );
        assert_eq!(
            workspace
                .read_document_root(read(&workspace, document, "null_default", 8))
                .unwrap()
                .value,
            EditorRootValue::Defaulted(ComponentValue::Null)
        );
        let mut tree = read(&workspace, document, "tree", 16);
        tree.limits = EditorRootReadLimits::new(1024, 2, 16).unwrap();
        assert_eq!(
            workspace.read_document_root(tree.clone()),
            Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
        );
        tree.limits = EditorRootReadLimits::new(1024, 3, 16).unwrap();
        let tree = workspace.read_document_root(tree).unwrap();
        assert_eq!((tree.value_nodes, tree.value_logical_bytes), (3, 16));
        assert_eq!(
            tree.value,
            EditorRootValue::Present(ComponentValue::List(vec![
                ComponentValue::U64(1),
                ComponentValue::U64(2)
            ]))
        );
        let mut exact = read(&workspace, document, "amount", 8);
        exact.limits = EditorRootReadLimits::new(19, 1, 8).unwrap();
        assert_eq!(workspace.read_document_root(exact.clone()).unwrap(), amount);
        exact.limits = EditorRootReadLimits::new(18, 1, 8).unwrap();
        assert_eq!(
            workspace.read_document_root(exact),
            Err(EditorDocumentQueryRejection::MetadataBudgetExceeded)
        );
        let mut unavailable = read(&workspace, document, "amount", 8);
        unavailable.target.component = ComponentTypeId::new("game.Unknown");
        assert_eq!(
            workspace.read_document_root(unavailable),
            Err(EditorDocumentQueryRejection::Source(
                AuthoringReadError::UnavailableComponent {
                    readiness: AuthoringRecordReadiness::UnknownSchema
                }
            ))
        );
        assert_eq!(workspace.authoring_model(), before);
    }
}

#[test]
fn continuations_bind_workspace_target_and_basis_and_charge_the_complete_observation() {
    for prefab in [false, true] {
        let (mut workspace, document) = fixture(prefab);
        let request = query(&workspace, document, roots());
        let first = workspace.query_document_authoring(request.clone()).unwrap();
        assert_eq!(
            first.metadata_bytes, 32,
            "two copies of item + game.Data, plus amount"
        );
        let mut exact = request.clone();
        exact.limits = EditorAuthoringQueryLimits::new(2, 1, 32).unwrap();
        assert_eq!(
            workspace.query_document_authoring(exact.clone()).unwrap(),
            first
        );
        exact.limits = EditorAuthoringQueryLimits::new(2, 1, 31).unwrap();
        assert_eq!(
            workspace.query_document_authoring(exact),
            Err(EditorDocumentQueryRejection::MetadataBudgetExceeded)
        );
        let mut changed_page = first.clone();
        changed_page.target.entity = id("empty");
        changed_page.target.projection = EditorAuthoringProjection::Components;
        let second = next(&workspace, changed_page);
        assert_eq!(second.target.entity, id("item"));
        assert_eq!(second.target.projection, roots());
        let continuation = EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Continue(first.next.unwrap()),
            limits: request.limits,
        };
        let (other, _) = fixture(prefab);
        assert_eq!(
            other.query_document_authoring(continuation.clone()),
            Err(EditorDocumentQueryRejection::WrongWorkspace)
        );
        let old_read = read(&workspace, document, "amount", 8);
        let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: id("item"),
            component: ComponentTypeId::new("game.Data"),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("amount"),
            value: ComponentValue::U64(9),
        }]);
        let edit = workspace
            .prepare_authoring_edit(document, old_read.target.basis.revision, &patch)
            .unwrap();
        assert!(workspace.commit_authoring_edit(edit).applied);
        assert_eq!(
            workspace.query_document_authoring(continuation.clone()),
            Err(EditorDocumentQueryRejection::StaleRevision)
        );
        assert_eq!(
            workspace.read_document_root(old_read.clone()),
            Err(EditorDocumentQueryRejection::StaleRevision)
        );
        let revision = workspace.document_revision(document).unwrap();
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::Undo {
                    document,
                    base_revision: revision
                })
                .applied
        );
        assert_eq!(
            workspace.query_document_authoring(continuation),
            Err(EditorDocumentQueryRejection::StaleRevision)
        );
        assert_eq!(
            workspace.read_document_root(old_read),
            Err(EditorDocumentQueryRejection::StaleRevision)
        );
    }
}

#[test]
fn multi_row_discovery_charges_aggregate_metadata_before_copying() {
    for prefab in [false, true] {
        let (workspace, document) = fixture(prefab);
        let before = workspace.authoring_model();
        for (projection, metadata_bytes, expected_ids, next_field) in [
            (
                EditorAuthoringProjection::Components,
                25,
                ["game.Data", "game.Unknown"],
                None,
            ),
            (roots(), 36, ["amount", "blob"], Some("defaulted")),
        ] {
            let mut request = query(&workspace, document, projection.clone());
            request.limits = EditorAuthoringQueryLimits::new(2, 2, metadata_bytes).unwrap();
            let page = workspace.query_document_authoring(request.clone()).unwrap();
            assert_eq!(page.scanned_entries, 2);
            assert_eq!(page.metadata_bytes, metadata_bytes);
            let ids: Vec<_> = page
                .rows
                .iter()
                .map(|row| match (&projection, row) {
                    (
                        EditorAuthoringProjection::Components,
                        EditorAuthoringRow::Component { component, .. },
                    ) => component.as_str(),
                    (
                        EditorAuthoringProjection::Roots { .. },
                        EditorAuthoringRow::Root { field, .. },
                    ) => field.as_str(),
                    _ => panic!("query returned a row from another projection"),
                })
                .collect();
            assert_eq!(ids, expected_ids);
            match next_field {
                Some(expected) => {
                    let continued = next(&workspace, page.clone());
                    assert_eq!(continued.target, page.target);
                    assert_eq!(continued.scanned_entries, 1);
                    assert!(matches!(
                        &continued.rows[..],
                        [EditorAuthoringRow::Root { field, .. }] if field.as_str() == expected
                    ));
                }
                None => assert!(page.next.is_none()),
            }
            request.limits = EditorAuthoringQueryLimits::new(2, 2, metadata_bytes - 1).unwrap();
            assert_eq!(
                workspace.query_document_authoring(request),
                Err(EditorDocumentQueryRejection::MetadataBudgetExceeded)
            );
        }
        assert_eq!(workspace.authoring_model(), before);
    }
}

#[test]
fn independent_limits_accept_exact_caps_and_reject_zero_or_over_cap_values() {
    for (scan, rows, bytes) in [
        (0, 1, 1),
        (1, 0, 1),
        (1, 1, 0),
        (257, 1, 1),
        (1, 257, 1),
        (1, 1, 65_537),
    ] {
        assert_eq!(
            EditorAuthoringQueryLimits::new(scan, rows, bytes),
            Err(EditorDocumentQueryRejection::InvalidAuthoringLimits)
        );
    }
    assert!(EditorAuthoringQueryLimits::new(256, 256, 65_536).is_ok());
    for (metadata, nodes, bytes) in [
        (0, 1, 1),
        (1, 0, 1),
        (1, 1, 0),
        (65_537, 1, 1),
        (1, 4_097, 1),
        (1, 1, 262_145),
    ] {
        assert_eq!(
            EditorRootReadLimits::new(metadata, nodes, bytes),
            Err(EditorDocumentQueryRejection::InvalidRootReadLimits)
        );
    }
    assert!(EditorRootReadLimits::new(65_536, 4_096, 262_144).is_ok());
}

#[test]
#[ignore = "diagnostic owner-read baseline, not a latency or allocator acceptance gate"]
fn measure_selected_root_read_with_large_unrequested_sibling_and_entity_growth() {
    use std::{hint::black_box, time::Instant};

    for prefab in [false, true] {
        for extra in [256, 4_096, 16_384] {
            let (workspace, document) = fixture_size(prefab, extra);
            let request = read(&workspace, document, "amount", 8);
            for _ in 0..8 {
                black_box(workspace.read_document_root(request.clone()).unwrap());
            }
            let mut samples = Vec::with_capacity(64);
            for _ in 0..64 {
                let started = Instant::now();
                let result = workspace
                    .read_document_root(black_box(request.clone()))
                    .unwrap();
                samples.push(started.elapsed().as_nanos());
                assert_eq!(
                    result.value,
                    EditorRootValue::Present(ComponentValue::U64(7))
                );
                black_box(result);
            }
            samples.sort_unstable();
            println!(
                "authoring-root-read kind={} entities={} sibling_bytes=10000 warmups=8 samples=64 p50_ns={} p95_ns={}",
                if prefab { "Prefab" } else { "Scene" },
                extra + 3,
                samples[31],
                samples[60]
            );
        }
    }
}
