use super::*;
use crate::{
    EditorAuthoringProjection, EditorAuthoringQuery, EditorAuthoringQueryLimits,
    EditorAuthoringQueryPosition, EditorAuthoringQueryTarget, EditorAuthoringRow,
    EditorPropertyReadLimits,
};
use nara_scene::{
    PrefabAddition, PrefabComponentIntent, PrefabDeclarationError, PrefabDeclarationLimits,
    PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceBinding,
    PrefabInstanceClaim, PrefabInstanceHop, PrefabInstancePath, PrefabInstancePathSegment,
    PrefabOverlay, PrefabOverlaySlot, PrefabPropertyClaim, PrefabPropertyIntent, PrefabRevert,
    SourceDocumentKey, SourceDocumentKind,
};

fn root_path() -> PrefabInstancePath {
    PrefabInstancePath::new(entity_id("anchor"))
}

fn source_document() -> SceneDocument {
    let base = SourceDocumentKey::named(
        "5e7c0f41-a1f7-4ce4-bb69-e4927f4f661b".parse().unwrap(),
        "base",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let mut overlay = PrefabOverlay::new(base.clone());
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(entity_id("inherited")));
    let mut addition = PrefabAddition::new(entity_id("nested"));
    addition.prefab = Some(PrefabInstance {
        binding: PrefabInstanceBinding::Forest,
        arguments: None,
        source: AssetRef::path("prefabs/nested.prefab.json").unwrap(),
        overrides: Some(PrefabOverlay::new(base)),
    });
    overlay.additions.push(addition);
    overlay.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(entity_id("detached")),
        value: None,
    });
    let mut anchor = SceneEntityRecord::new(entity_id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        binding: PrefabInstanceBinding::Forest,
        arguments: None,
        source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
        overrides: Some(overlay),
    });
    SceneDocument::from_ordered_roots([anchor, SceneEntityRecord::new(entity_id("peer"))])
}

fn schema_basis() -> AuthoringSchemaBasis {
    AuthoringSchemaBasis::admit(&authoring_basis_bundle("Prefab Revert"), &[]).unwrap()
}

fn open_document(
    workspace: &mut EditorWorkspace,
    kind: EditorDocumentKind,
    title: &str,
    basis: &AuthoringSchemaBasis,
) -> EditorDocumentId {
    open_source(workspace, kind, title, basis, source_document())
}

fn open_source(
    workspace: &mut EditorWorkspace,
    kind: EditorDocumentKind,
    title: &str,
    basis: &AuthoringSchemaBasis,
    source: SceneDocument,
) -> EditorDocumentId {
    let report = match kind {
        EditorDocumentKind::Scene => {
            let path = format!("scenes/{title}.scene.json");
            let published = SceneDocumentCandidate::decode_json_bytes(
                source.to_json_string().unwrap().as_bytes(),
            )
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path(&path).unwrap()),
                basis,
            )
            .unwrap()
            .publish();
            workspace
                .open_authoring_scene_document(
                    title,
                    EditorDocumentSourceBinding::new(path),
                    published,
                )
                .unwrap()
        }
        EditorDocumentKind::Prefab => {
            let path = format!("prefabs/{title}.prefab.json");
            let published = PrefabDocumentCandidate::decode_json_bytes(
                PrefabDocument::new(source.placements, source.entities)
                    .to_json_string()
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path(&path).unwrap()),
                basis,
            )
            .unwrap()
            .publish();
            workspace
                .open_authoring_prefab_document(
                    title,
                    EditorDocumentSourceBinding::new(path),
                    published,
                )
                .unwrap()
        }
        EditorDocumentKind::Extension(_) => unreachable!("entity-bearing source fixture"),
    };
    report.opened_document.unwrap()
}

fn document_value(workspace: &EditorWorkspace, document: EditorDocumentId) -> SceneDocument {
    match &workspace.documents[&document] {
        EditorDocumentSlot::Scene(slot) => slot.session.document().clone(),
        EditorDocumentSlot::Prefab(slot) => slot.session.document().instantiate(),
        EditorDocumentSlot::Extension(_) => unreachable!("entity-bearing source fixture"),
    }
}

fn assert_basis_rejected(
    workspace: &EditorWorkspace,
    basis: &EditorDocumentQueryBasis,
    rejection: EditorDocumentQueryRejection,
    preparation_code: &str,
) {
    // A malformed selector and exhausted content limits must not mask stale authority.
    let path = PrefabInstancePath::new(entity_id("absent"));
    let limits = PrefabDeclarationLimits::new(1, 1, 1).unwrap();
    assert_eq!(
        workspace.observe_prefab_instance(basis, &path, limits),
        Err(rejection)
    );
    let report = workspace
        .prepare_prefab_revert(basis, &path, PrefabRevert::Overlay, limits)
        .unwrap_err();
    assert!(!report.applied);
    assert_eq!(report.document, Some(basis.document));
    assert!(report.patch_report.is_none());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == preparation_code),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn bounded_claim_pages_preserve_typed_targets_and_skip_unrequested_payloads() {
    use nara_reflect::{ComponentPropertyPath, ComponentPropertyStep};

    let version = ComponentSchemaVersion::ONE;
    let component = ComponentTypeId::new("test.MissingSettings");
    let property = ComponentPropertyPath::new(ComponentFieldId::new("settings")).with_steps([
        ComponentPropertyStep::Member(ComponentFieldId::new("score")),
    ]);
    let payload = ComponentValue::String("x".repeat(257 * 1024));
    let mut source = source_document();
    let overlay = source.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap();
    let target = PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::InstanceAddition(entity_id("nested")),
            expected_base: overlay.base.clone(),
        }],
        entity: PrefabEntityOrigin::SourceEntity(entity_id("inherited")),
    };
    overlay.components = vec![
        PrefabComponentIntent::Suppress {
            target: PrefabEntityTarget::source(entity_id("inherited")),
            component: component.clone(),
        },
        PrefabComponentIntent::Properties {
            target: target.clone(),
            component: component.clone(),
            version,
            claims: vec![
                PrefabPropertyClaim {
                    path: property.clone(),
                    intent: PrefabPropertyIntent::Set(ComponentValue::U64(27)),
                },
                PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("optional")),
                    intent: PrefabPropertyIntent::Absent,
                },
                PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("nullable")),
                    intent: PrefabPropertyIntent::Set(ComponentValue::Null),
                },
                PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("large")),
                    intent: PrefabPropertyIntent::Set(payload),
                },
            ],
        },
    ];
    let nested = overlay.additions[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap();
    nested.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(entity_id("inherited")),
        component: component.clone(),
        version,
        claims: vec![PrefabPropertyClaim {
            path: property.clone(),
            intent: PrefabPropertyIntent::Set(ComponentValue::U64(82)),
        }],
    });
    let target_bytes = "nested".len() + "inherited".len() + 37 + "base".len();
    let row_bytes = target_bytes + component.as_str().len() + "settings".len() + "score".len();
    let after_row = root_path().anchor.as_str().len() * 2 + row_bytes;

    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut workspace = EditorWorkspace::new();
        let document = open_source(
            &mut workspace,
            kind,
            "subject",
            &schema_basis(),
            source.clone(),
        );
        let _ = open_document(
            &mut workspace,
            EditorDocumentKind::Scene,
            "background",
            &schema_basis(),
        );
        let before = workspace.authoring_model();
        let basis = workspace.document_query_basis(document).unwrap();
        let path = root_path();
        assert_eq!(
            workspace.observe_prefab_instance(&basis, &path, PrefabDeclarationLimits::default()),
            Err(PrefabDeclarationError::BudgetExceeded.into()),
        );
        let start = |path: &PrefabInstancePath| {
            EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                basis: basis.clone(),
                entity: path.anchor.clone(),
                projection: EditorAuthoringProjection::PrefabPropertyClaims {
                    steps: path.steps.clone(),
                },
            })
        };
        let first = workspace
            .query_document_authoring(EditorAuthoringQuery {
                position: start(&path),
                limits: EditorAuthoringQueryLimits::new(1, 1, 64).unwrap(),
            })
            .unwrap();
        assert_eq!(first.scanned_entries, 1);
        assert!(first.rows.is_empty());
        let next = EditorAuthoringQueryPosition::Continue(first.next.unwrap());
        let query = |metadata, scan_bytes| EditorAuthoringQuery {
            position: next.clone(),
            limits: EditorAuthoringQueryLimits::new(4, 4, metadata)
                .unwrap()
                .with_scan_logical_bytes(scan_bytes)
                .unwrap(),
        };
        assert_eq!(
            workspace.query_document_authoring(query(after_row, row_bytes - 1)),
            Err(EditorDocumentQueryRejection::PrefabDeclaration(
                PrefabDeclarationError::BudgetExceeded
            )),
        );
        assert_eq!(
            workspace.query_document_authoring(query(after_row - 1, row_bytes)),
            Err(EditorDocumentQueryRejection::MetadataBudgetExceeded),
        );
        assert_eq!(
            EditorWorkspace::new().query_document_authoring(query(after_row, row_bytes)),
            Err(EditorDocumentQueryRejection::WrongWorkspace)
        );
        let page = workspace
            .query_document_authoring(query(after_row, row_bytes))
            .unwrap();
        assert_eq!(
            workspace
                .query_document_authoring(query(after_row, row_bytes))
                .unwrap(),
            page
        );
        let score_slot = PrefabOverlaySlot::Property {
            component_index: 1,
            claim_index: 0,
        };
        assert_eq!(page.metadata_bytes, after_row);
        assert_eq!(page.scanned_entries, 1);
        assert_eq!(page.scanned_logical_bytes, row_bytes);
        let [
            EditorAuthoringRow::PrefabPropertyClaim {
                claim: score,
                target: found_target,
                component: found_component,
                version: found_version,
                property: found_property,
                has_value: true,
            },
        ] = page.rows.as_slice()
        else {
            panic!("expected the exact routed score claim");
        };
        assert_eq!(score.basis(), &basis);
        assert_eq!(score.path(), &path);
        assert_eq!(score.slot(), score_slot);
        assert_eq!(found_target, &target);
        assert_eq!(found_component, &component);
        assert_eq!(*found_version, version);
        assert_eq!(found_property, &property);
        let rest = workspace
            .query_document_authoring(EditorAuthoringQuery {
                position: EditorAuthoringQueryPosition::Continue(page.next.unwrap()),
                limits: EditorAuthoringQueryLimits::new(4, 4, 1024).unwrap(),
            })
            .unwrap();
        assert_eq!(rest.rows.len(), 3);
        assert!(rest.next.is_none());
        let read = |claim, bytes| {
            workspace.read_prefab_property_claim(
                claim,
                EditorPropertyReadLimits::new(64, 1, bytes).unwrap(),
            )
        };
        assert_eq!(
            read(score, 8).unwrap(),
            PrefabPropertyIntent::Set(ComponentValue::U64(27))
        );
        assert_eq!(
            read(score, 7),
            Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
        );
        assert_eq!(
            EditorWorkspace::new().read_prefab_property_claim(
                score,
                EditorPropertyReadLimits::new(64, 1, 8).unwrap()
            ),
            Err(EditorDocumentQueryRejection::WrongWorkspace)
        );
        for (index, row) in rest.rows.iter().enumerate() {
            let EditorAuthoringRow::PrefabPropertyClaim {
                claim, has_value, ..
            } = row
            else {
                panic!("expected a property claim")
            };
            assert_eq!(*has_value, index != 0);
            assert_eq!(
                read(claim, 8),
                match index {
                    0 => Ok(PrefabPropertyIntent::Absent),
                    1 => Ok(PrefabPropertyIntent::Set(ComponentValue::Null)),
                    _ => Err(EditorDocumentQueryRejection::ValueBudgetExceeded),
                }
            );
        }
        let mut nested = path.clone();
        nested
            .steps
            .push(PrefabInstancePathSegment::Addition { index: 0 });
        let nested_page = workspace
            .query_document_authoring(EditorAuthoringQuery {
                position: start(&nested),
                limits: EditorAuthoringQueryLimits::new(1, 1, 256).unwrap(),
            })
            .unwrap();
        let [EditorAuthoringRow::PrefabPropertyClaim { claim, target, .. }] =
            nested_page.rows.as_slice()
        else {
            panic!("expected one nested claim")
        };
        assert_eq!(target, &PrefabEntityTarget::source(entity_id("inherited")));
        assert_eq!(claim.path(), &nested);
        assert_eq!(
            read(claim, 8).unwrap(),
            PrefabPropertyIntent::Set(ComponentValue::U64(82))
        );
        assert_eq!(
            workspace.read_prefab_property_claim(
                claim,
                EditorPropertyReadLimits::new(14, 1, 8).unwrap()
            ),
            Err(EditorDocumentQueryRejection::MetadataBudgetExceeded)
        );
        nested.steps[0] = PrefabInstancePathSegment::InstanceClaim { index: 0 };
        assert_eq!(
            workspace.query_document_authoring(EditorAuthoringQuery {
                position: start(&nested),
                limits: EditorAuthoringQueryLimits::new(1, 1, 256).unwrap(),
            }),
            Err(PrefabDeclarationError::InstanceAbsent.into())
        );
        assert_eq!(workspace.authoring_model(), before);
        assert_eq!(document_value(&workspace, document), source);
    }
}

#[test]
fn oversized_claim_header_keeps_its_original_handle_and_does_not_block_the_next_claim() {
    use nara_reflect::{ComponentPropertyPath, ComponentPropertyStep};

    let mut source = source_document();
    let anchor = &mut source.entities[0];
    anchor.id = entity_id(&"a".repeat(22 * 1024));
    let overlay = anchor.prefab.as_mut().unwrap().overrides.as_mut().unwrap();
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(entity_id("inherited")),
        component: ComponentTypeId::new("test.Unavailable"),
        version: ComponentSchemaVersion::ONE,
        claims: vec![
            PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("settings"))
                    .with_steps([ComponentPropertyStep::MapEntry("m".repeat(70 * 1024))]),
                intent: PrefabPropertyIntent::Set(ComponentValue::U64(9)),
            },
            PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("overdeep")).with_steps(
                    std::iter::repeat_n(
                        ComponentPropertyStep::MapEntry(String::new()),
                        ComponentPropertyPath::MAX_STEPS + 2,
                    ),
                ),
                intent: PrefabPropertyIntent::Set(ComponentValue::U64(11)),
            },
            PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("score")),
                intent: PrefabPropertyIntent::Set(ComponentValue::U64(10)),
            },
        ],
    });
    let path = PrefabInstancePath::new(anchor.id.clone());
    let source = SceneDocument::from_ordered_roots(source.entities);
    let mut workspace = EditorWorkspace::new();
    let document = open_source(
        &mut workspace,
        EditorDocumentKind::Scene,
        "large-selector",
        &schema_basis(),
        source,
    );
    let basis = workspace.document_query_basis(document).unwrap();
    let start = EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
        basis,
        entity: path.anchor.clone(),
        projection: EditorAuthoringProjection::PrefabPropertyClaims { steps: Vec::new() },
    });
    let limits = EditorAuthoringQueryLimits::new(2, 1, 64 * 1024).unwrap();
    let page = workspace
        .query_document_authoring(EditorAuthoringQuery {
            position: start,
            limits,
        })
        .unwrap();
    let [EditorAuthoringRow::OversizedPrefabPropertyClaim { claim }] = page.rows.as_slice() else {
        panic!("expected an exact handle for the oversized claim");
    };
    assert_eq!(claim.path(), &path);
    assert_eq!(page.metadata_bytes, path.anchor.as_str().len() * 2);
    assert_eq!(page.scanned_logical_bytes, 64 * 1024 - page.metadata_bytes);
    let read_limits = EditorPropertyReadLimits::new(64 * 1024, 1, 8).unwrap();
    assert_eq!(
        workspace
            .read_prefab_property_claim(claim, read_limits)
            .unwrap(),
        PrefabPropertyIntent::Set(ComponentValue::U64(9))
    );
    let next = workspace
        .query_document_authoring(EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Continue(page.next.unwrap()),
            limits,
        })
        .unwrap();
    let [EditorAuthoringRow::OversizedPrefabPropertyClaim { claim: overdeep }] =
        next.rows.as_slice()
    else {
        panic!("a preserved over-depth selector cannot bypass the copy budget using empty keys");
    };
    assert_eq!(next.scanned_logical_bytes, 0);
    assert_eq!(
        workspace
            .read_prefab_property_claim(overdeep, read_limits)
            .unwrap(),
        PrefabPropertyIntent::Set(ComponentValue::U64(11))
    );
    let next = workspace
        .query_document_authoring(EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Continue(next.next.unwrap()),
            limits,
        })
        .unwrap();
    let [
        EditorAuthoringRow::PrefabPropertyClaim {
            claim: following,
            property,
            ..
        },
    ] = next.rows.as_slice()
    else {
        panic!("the next authored selector remains discoverable");
    };
    assert_eq!(property.root.as_str(), "score");
    assert_eq!(
        workspace
            .read_prefab_property_claim(following, read_limits)
            .unwrap(),
        PrefabPropertyIntent::Set(ComponentValue::U64(10))
    );
    assert!(next.next.is_none());
    let prepared = workspace
        .prepare_prefab_property_revert(claim, PrefabDeclarationLimits::default())
        .unwrap();
    assert!(workspace.commit_authoring_edit(prepared).applied);
    assert_eq!(
        workspace.read_prefab_property_claim(following, read_limits),
        Err(EditorDocumentQueryRejection::StaleRevision)
    );
}

#[test]
fn observe_and_prepare_non_active_sources_preserve_navigation_and_one_undo_owner() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut workspace = EditorWorkspace::new();
        let document = open_document(&mut workspace, kind, "source", &schema_basis());
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                    document,
                    entity: Some(entity_id("peer")),
                })
                .applied
        );
        let background = open_document(
            &mut workspace,
            EditorDocumentKind::Scene,
            "background",
            &schema_basis(),
        );
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                    document: background,
                    entity: Some(entity_id("peer")),
                })
                .applied
        );
        let before = workspace.authoring_model();
        let original = document_value(&workspace, document);
        let basis = workspace.document_query_basis(document).unwrap();
        let observation = workspace
            .observe_prefab_instance(&basis, &root_path(), PrefabDeclarationLimits::default())
            .unwrap();
        assert_eq!(observation.revision, basis.revision);
        assert_eq!(Some(observation.schema), basis.schema);
        assert_eq!(observation.instance, original.entities[0].prefab);
        assert_eq!(observation.nested_instances.len(), 2);
        let nested = PrefabInstancePath {
            anchor: entity_id("anchor"),
            steps: vec![PrefabInstancePathSegment::Addition { index: 0 }],
        };
        assert!(
            workspace
                .observe_prefab_instance(&basis, &nested, PrefabDeclarationLimits::default(),)
                .unwrap()
                .instance
                .unwrap()
                .overrides
                .is_some()
        );
        let prepared = workspace
            .prepare_prefab_revert(
                &basis,
                &root_path(),
                PrefabRevert::Overlay,
                PrefabDeclarationLimits::default(),
            )
            .unwrap();
        assert_eq!(prepared.document(), document);
        assert_eq!(prepared.document_kind(), kind);
        assert_eq!(prepared.base_revision(), basis.revision);
        assert!(prepared.is_prefab_revert());
        assert!(matches!(&prepared.patch().operations[..],
            [ScenePatchOperation::SetPrefabInstance { entity, expected, value }]
            if entity == &entity_id("anchor")
                && expected == &observation.instance
                && value.as_ref().unwrap().overrides.is_none()));
        assert_eq!(workspace.authoring_model(), before);
        assert_eq!(document_value(&workspace, document), original);

        let committed = workspace.commit_authoring_edit(prepared);
        assert!(committed.applied, "{:?}", committed.diagnostics);
        assert_eq!(workspace.active_document(), Some(background));
        assert_eq!(
            workspace.documents[&document].selection().top_entity(),
            Some(&entity_id("peer"))
        );
        assert_eq!(
            workspace.documents[&background].selection().top_entity(),
            Some(&entity_id("peer"))
        );
        assert_eq!(
            workspace.documents[&document].history_status().undo_depth,
            1
        );
        assert_eq!(
            workspace.documents[&background].history_status().undo_depth,
            0
        );
        let changed = document_value(&workspace, document);
        assert_ne!(changed, original);
        let undo = workspace.apply_authoring_command(EditorWorkspaceCommand::Undo {
            document,
            base_revision: committed.revision.unwrap(),
        });
        assert!(undo.applied, "{:?}", undo.diagnostics);
        assert_eq!(document_value(&workspace, document), original);
        assert_eq!(
            workspace.documents[&document].history_status().undo_depth,
            0
        );
        assert!(!workspace.documents[&document].is_dirty());
        let redo = workspace.apply_authoring_command(EditorWorkspaceCommand::Redo {
            document,
            base_revision: undo.revision.unwrap(),
        });
        assert!(redo.applied, "{:?}", redo.diagnostics);
        assert_eq!(document_value(&workspace, document), changed);
        assert_eq!(
            workspace.documents[&document].history_status().undo_depth,
            1
        );
        assert_eq!(workspace.active_document(), Some(background));
    }
}

#[test]
fn different_schema_admissions_and_missing_schema_reject_before_source_reads() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut workspace = EditorWorkspace::new();
        let schema = schema_basis();
        let document = open_document(&mut workspace, kind, "source", &schema);
        let basis = workspace.document_query_basis(document).unwrap();
        let before = workspace.authoring_model();
        let original = document_value(&workspace, document);
        let independent = schema_basis();
        assert_ne!(schema, independent);
        for schema in [None, Some(independent)] {
            let wrong_schema = EditorDocumentQueryBasis {
                schema,
                ..basis.clone()
            };
            assert_basis_rejected(
                &workspace,
                &wrong_schema,
                EditorDocumentQueryRejection::StaleSchema,
                "tooling.workspace-stale-schema-basis",
            );
        }
        assert_eq!(workspace.authoring_model(), before);
        assert_eq!(document_value(&workspace, document), original);
    }
}

#[test]
fn matching_paths_and_shared_schema_do_not_transfer_scene_or_prefab_ownership() {
    let mut workspace = EditorWorkspace::new();
    let schema = schema_basis();
    let documents = [
        open_document(&mut workspace, EditorDocumentKind::Scene, "first", &schema),
        open_document(&mut workspace, EditorDocumentKind::Scene, "second", &schema),
        open_document(&mut workspace, EditorDocumentKind::Prefab, "first", &schema),
        open_document(
            &mut workspace,
            EditorDocumentKind::Prefab,
            "second",
            &schema,
        ),
    ];
    let before = workspace.authoring_model();
    for owner in documents {
        let basis = workspace.document_query_basis(owner).unwrap();
        for document in documents {
            if document == owner {
                continue;
            }
            let wrong_owner = EditorDocumentQueryBasis {
                document,
                ..basis.clone()
            };
            assert_basis_rejected(
                &workspace,
                &wrong_owner,
                EditorDocumentQueryRejection::StaleIncarnation,
                "tooling.workspace-stale-document-incarnation",
            );
        }
        assert_eq!(document_value(&workspace, owner), source_document());
    }
    assert_eq!(workspace.authoring_model(), before);
}

#[test]
fn stale_prepared_revert_and_captured_basis_reject_even_after_undo_restores_content() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut workspace = EditorWorkspace::new();
        let document = open_document(&mut workspace, kind, "source", &schema_basis());
        let original = document_value(&workspace, document);
        let basis = workspace.document_query_basis(document).unwrap();
        let pending = workspace
            .prepare_prefab_revert(
                &basis,
                &root_path(),
                PrefabRevert::Overlay,
                PrefabDeclarationLimits::default(),
            )
            .unwrap();
        let accepted = workspace
            .prepare_prefab_revert(
                &basis,
                &root_path(),
                PrefabRevert::Overlay,
                PrefabDeclarationLimits::default(),
            )
            .unwrap();
        assert!(workspace.commit_authoring_edit(accepted).applied);
        let before = workspace.authoring_model();
        assert_basis_rejected(
            &workspace,
            &basis,
            EditorDocumentQueryRejection::StaleRevision,
            "tooling.workspace-stale-document-revision",
        );
        assert!(!workspace.commit_authoring_edit(pending).applied);
        assert_eq!(workspace.authoring_model(), before);
        let revision = workspace.document_revision(document).unwrap();
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::Undo {
                    document,
                    base_revision: revision,
                })
                .applied
        );
        assert_eq!(document_value(&workspace, document), original);
        assert_basis_rejected(
            &workspace,
            &basis,
            EditorDocumentQueryRejection::StaleRevision,
            "tooling.workspace-stale-document-revision",
        );
        assert_eq!(
            workspace.documents[&document].history_status().undo_depth,
            0
        );
    }
}

#[test]
fn reopen_same_content_and_schema_invalidates_observation_and_pending_revert() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
        let document = open_document(&mut workspace, kind, "source", &schema_basis());
        let basis = workspace.document_query_basis(document).unwrap();
        let pending = workspace
            .prepare_prefab_revert(
                &basis,
                &root_path(),
                PrefabRevert::Overlay,
                PrefabDeclarationLimits::default(),
            )
            .unwrap();
        let digest = EditorDocumentDigest::new(7, [0x5a; 32]);
        match kind {
            EditorDocumentKind::Scene => {
                let session = workspace.scene(document).unwrap().session();
                let candidate = SceneDocumentCandidate::decode_json_bytes(
                    session.document().to_json_string().unwrap().as_bytes(),
                )
                .unwrap();
                let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
                authority
                    .publish_reopened_session(
                        &mut workspace,
                        document,
                        basis.revision,
                        reopened,
                        digest,
                    )
                    .unwrap();
            }
            EditorDocumentKind::Prefab => {
                let source = workspace.documents[&document]
                    .source_binding()
                    .unwrap()
                    .clone();
                let session = workspace.prefab(document).unwrap().session();
                let candidate = PrefabDocumentCandidate::decode_json_bytes(
                    session.document().to_json_string().unwrap().as_bytes(),
                )
                .unwrap();
                let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
                authority
                    .publish_reopened_prefab_session(
                        &mut workspace,
                        document,
                        basis.revision,
                        source,
                        reopened,
                        digest,
                    )
                    .unwrap();
            }
            EditorDocumentKind::Extension(_) => unreachable!("entity-bearing source fixture"),
        }
        let current = workspace.document_query_basis(document).unwrap();
        assert_eq!(basis.schema, current.schema);
        assert_ne!(basis.revision.source_id(), current.revision.source_id());
        let before = workspace.authoring_model();
        assert_basis_rejected(
            &workspace,
            &basis,
            EditorDocumentQueryRejection::StaleIncarnation,
            "tooling.workspace-stale-document-incarnation",
        );
        assert!(!workspace.commit_authoring_edit(pending).applied);
        assert_eq!(workspace.authoring_model(), before);
        assert_eq!(document_value(&workspace, document), source_document());
    }
}

#[test]
fn identical_generic_patch_does_not_acquire_source_owned_revert_origin() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut workspace = EditorWorkspace::new();
        let document = open_document(&mut workspace, kind, "source", &schema_basis());
        let basis = workspace.document_query_basis(document).unwrap();
        let before = workspace.authoring_model();
        let revert = workspace
            .prepare_prefab_revert(
                &basis,
                &root_path(),
                PrefabRevert::Overlay,
                PrefabDeclarationLimits::default(),
            )
            .unwrap();
        let lookalike = workspace
            .prepare_authoring_edit(&basis, revert.patch())
            .unwrap();
        assert_eq!(revert.patch(), lookalike.patch());
        assert_eq!(revert.inverse(), lookalike.inverse());
        assert!(revert.is_prefab_revert());
        assert!(!lookalike.is_prefab_revert());
        assert_eq!(workspace.authoring_model(), before);
        assert_eq!(
            workspace.documents[&document].history_status().undo_depth,
            0
        );
    }
}

#[test]
fn source_observation_and_preparation_refusals_preserve_owner_results_and_state() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut workspace = EditorWorkspace::new();
        let document = open_document(&mut workspace, kind, "source", &schema_basis());
        let basis = workspace.document_query_basis(document).unwrap();
        let before = workspace.authoring_model();
        let original = document_value(&workspace, document);
        let missing = PrefabInstancePath::new(entity_id("absent"));
        for (path, limits, error, code) in [
            (
                missing,
                PrefabDeclarationLimits::default(),
                PrefabDeclarationError::AnchorNotFound,
                "scene.authoring-prefab-anchor-not-found",
            ),
            (
                root_path(),
                PrefabDeclarationLimits::new(1, 1, 1).unwrap(),
                PrefabDeclarationError::BudgetExceeded,
                "scene.authoring-prefab-budget-exceeded",
            ),
        ] {
            let rejection = EditorDocumentQueryRejection::PrefabDeclaration(error);
            assert_eq!(rejection.to_string(), error.to_string());
            assert_eq!(
                std::error::Error::source(&rejection)
                    .unwrap()
                    .downcast_ref::<PrefabDeclarationError>(),
                Some(&error)
            );
            assert_eq!(
                workspace.observe_prefab_instance(&basis, &path, limits),
                Err(rejection)
            );
            let report = workspace
                .prepare_prefab_revert(&basis, &path, PrefabRevert::Overlay, limits)
                .unwrap_err();
            assert!(!report.applied);
            assert_eq!(report.document, Some(document));
            let source_report = report.patch_report.as_ref().unwrap();
            assert!(
                source_report
                    .diagnostics
                    .iter()
                    .any(|entry| entry.code().as_str() == code)
            );
            assert_eq!(report.diagnostics, source_report.diagnostics);
        }
        assert_eq!(workspace.authoring_model(), before);
        assert_eq!(document_value(&workspace, document), original);
    }
}
