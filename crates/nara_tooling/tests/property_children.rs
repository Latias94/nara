use nara_reflect::{
    ComponentFieldId, ComponentFieldPath, ComponentMemberPath, ComponentMemberPathSegment,
    ComponentPropertyError, ComponentPropertyKey, ComponentPropertyPath, ComponentPropertyStep,
    ComponentSchemaVersion, ComponentValue, ComponentValueKind,
};
use nara_scene::{
    AuthoringPropertyEdit, AuthoringReadError, PrefabDocumentCandidate, SceneAuthoringSession,
    SceneDocument, SceneDocumentCandidate, SceneEntityId, ScenePatchDocument, ScenePatchOperation,
    SchemaReclassificationKind, SchemaReclassificationWork,
};
use nara_tooling::{
    EditorAuthoringPage, EditorAuthoringProjection, EditorAuthoringPropertyStatus,
    EditorAuthoringQuery, EditorAuthoringQueryLimits, EditorAuthoringQueryPosition,
    EditorAuthoringRow, EditorDocumentDigest, EditorDocumentQueryRejection,
    EditorDocumentSourceBinding, EditorPropertyPagePosition, EditorPropertyPageRead,
    EditorPropertyReadLimits, EditorPropertyValue, EditorWorkspaceCommand,
};

#[path = "property_children/support.rs"]
mod support;

use support::{
    COMPONENT, ENTITY, PREFAB_SOURCE, component, continue_page, entity, fixture,
    fresh_schema_basis, keyed_item, limits, member, path, query, read, read_request,
};

fn available(
    property: ComponentPropertyPath,
    kind: ComponentValueKind,
    edit_capability: bool,
) -> EditorAuthoringRow {
    EditorAuthoringRow::Property {
        property,
        kind: Some(kind),
        edit_capability,
        status: EditorAuthoringPropertyStatus::Available,
    }
}

fn retired(property: ComponentPropertyPath) -> EditorAuthoringRow {
    EditorAuthoringRow::Property {
        property,
        kind: None,
        edit_capability: false,
        status: EditorAuthoringPropertyStatus::Retired,
    }
}

fn property(row: &EditorAuthoringRow) -> &ComponentPropertyPath {
    let EditorAuthoringRow::Property { property, .. } = row else {
        panic!("Properties must only return property rows: {row:?}");
    };
    property
}

fn collect(
    fixture: &support::Fixture,
    parent: &ComponentPropertyPath,
    rows: usize,
) -> Vec<EditorAuthoringRow> {
    let request = query(
        &fixture.workspace,
        fixture.document,
        parent.clone(),
        limits(rows),
    );
    let mut page = fixture
        .workspace
        .query_document_authoring(request.clone())
        .unwrap();
    let target = page.target.clone();
    let mut cursors = Vec::new();
    let mut collected = Vec::new();
    for _ in 0..64 {
        assert_eq!(page.target, target);
        assert!(page.rows.len() <= rows);
        assert!(page.scanned_entries <= request.limits.max_scan_entries());
        assert!(page.scanned_logical_bytes <= request.limits.max_scan_logical_bytes());
        for row in &page.rows {
            let child = property(row);
            assert_eq!(child.root, parent.root);
            assert_eq!(child.steps.len(), parent.steps.len() + 1);
            assert!(child.steps.starts_with(&parent.steps));
            assert!(!collected.iter().any(|prior| property(prior) == child));
        }
        collected.extend(page.rows.iter().cloned());
        let Some(cursor) = page.next.as_ref() else {
            return collected;
        };
        assert!(
            !cursors.contains(cursor),
            "a cursor must advance within its exact source"
        );
        cursors.push(cursor.clone());
        let next = continue_page(&fixture.workspace, &page, request.limits);
        assert_eq!(
            next,
            continue_page(&fixture.workspace, &page, request.limits)
        );
        page = next;
    }
    panic!("discovery did not terminate for the finite fixture");
}

fn list_item(root: &str, key: ComponentPropertyKey) -> ComponentPropertyPath {
    path(root).with_steps([ComponentPropertyStep::ListItem {
        by: ComponentFieldId::new("id"),
        key,
    }])
}

fn keyed_cases() -> [(&'static str, Vec<(ComponentPropertyKey, u64)>); 3] {
    [
        (
            "strings",
            vec![
                (ComponentPropertyKey::String("z".into()), 101),
                (ComponentPropertyKey::String("a".into()), 102),
                (ComponentPropertyKey::String(String::new()), 103),
            ],
        ),
        (
            "signed",
            vec![
                (ComponentPropertyKey::I64(-9), 201),
                (ComponentPropertyKey::I64(i64::MIN), 202),
                (ComponentPropertyKey::I64(-1), 203),
            ],
        ),
        (
            "unsigned",
            vec![
                (ComponentPropertyKey::U64(u64::MAX), 301),
                (ComponentPropertyKey::U64(0), 302),
                (ComponentPropertyKey::U64(7), 303),
            ],
        ),
    ]
}

fn key_value(key: &ComponentPropertyKey) -> ComponentValue {
    match key {
        ComponentPropertyKey::String(value) => ComponentValue::String(value.clone()),
        ComponentPropertyKey::I64(value) => ComponentValue::I64(*value),
        ComponentPropertyKey::U64(value) => ComponentValue::U64(*value),
    }
}

fn budget_refusal() -> EditorDocumentQueryRejection {
    EditorDocumentQueryRejection::Source(AuthoringReadError::Property(
        ComponentPropertyError::ReadBudgetExceeded,
    ))
}

#[test]
fn nested_records_preserve_durable_identity_and_scope_retired_members_to_their_parent() {
    for prefab in [false, true] {
        let fixture = fixture(prefab);
        let before = fixture.workspace.authoring_model();
        let record = path("record");
        let branch = member(&record, "branch");
        let expected = vec![
            available(branch.clone(), ComponentValueKind::Map, true),
            available(member(&record, "first"), ComponentValueKind::U64, true),
            available(member(&record, "last"), ComponentValueKind::String, true),
            retired(member(&record, "gone")),
        ];
        assert_eq!(collect(&fixture, &record, 1), expected);
        assert_eq!(collect(&fixture, &record, 4), expected);
        assert_eq!(
            collect(&fixture, &branch, 1),
            [
                available(member(&branch, "leaf"), ComponentValueKind::Bool, true),
                retired(member(&branch, "old_leaf")),
            ]
        );
        for (selector, expected_value) in [
            (member(&record, "first"), ComponentValue::U64(11)),
            (
                member(&record, "last"),
                ComponentValue::String("last value".into()),
            ),
            (member(&branch, "leaf"), ComponentValue::Bool(true)),
        ] {
            assert_eq!(
                read(&fixture.workspace, fixture.document, selector),
                EditorPropertyValue::Present(expected_value)
            );
        }
        for (selector, retired_path) in [
            (
                member(&record, "gone"),
                ComponentMemberPath::new([ComponentMemberPathSegment::Member(
                    ComponentFieldId::new("gone"),
                )]),
            ),
            (
                member(&branch, "old_leaf"),
                ComponentMemberPath::new([
                    ComponentMemberPathSegment::Member(ComponentFieldId::new("branch")),
                    ComponentMemberPathSegment::Member(ComponentFieldId::new("old_leaf")),
                ]),
            ),
        ] {
            assert_eq!(
                fixture.workspace.read_document_property(read_request(
                    &fixture.workspace,
                    fixture.document,
                    selector,
                )),
                Err(EditorDocumentQueryRejection::Source(
                    AuthoringReadError::Property(ComponentPropertyError::RetiredMember(
                        retired_path
                    ))
                ))
            );
        }
        for anonymous in ["wire_anonymous", "buried"] {
            assert!(matches!(
                fixture.workspace.read_document_property(read_request(
                    &fixture.workspace,
                    fixture.document,
                    member(&record, anonymous),
                )),
                Err(EditorDocumentQueryRejection::Source(
                    AuthoringReadError::Property(ComponentPropertyError::MemberNotFound { .. })
                ))
            ));
        }
        assert_eq!(fixture.workspace.authoring_model(), before);
    }
}

#[test]
fn map_pages_keep_literal_key_order_and_readable_selectors_across_page_sizes() {
    for prefab in [false, true] {
        let fixture = fixture(prefab);
        let before = fixture.workspace.authoring_model();
        let expected: Vec<_> = ["", "a", "a/b", "z", "é"]
            .into_iter()
            .map(|key| {
                available(
                    path("map").with_steps([ComponentPropertyStep::MapEntry(key.into())]),
                    ComponentValueKind::U64,
                    true,
                )
            })
            .collect();
        for page_size in [1, 2, 3, 5] {
            assert_eq!(collect(&fixture, &path("map"), page_size), expected);
        }
        for (row, value) in expected.iter().zip([1, 2, 3, 4, 5]) {
            assert_eq!(
                read(&fixture.workspace, fixture.document, property(row).clone()),
                EditorPropertyValue::Present(ComponentValue::U64(value))
            );
        }
        assert_eq!(
            collect(&fixture, &path("locked"), 1),
            [available(
                path("locked").with_steps([ComponentPropertyStep::MapEntry("value".into())]),
                ComponentValueKind::U64,
                false,
            )]
        );
        assert_eq!(fixture.workspace.authoring_model(), before);
    }
}

#[test]
fn keyed_lists_page_source_order_with_exact_string_signed_and_unsigned_identities() {
    for prefab in [false, true] {
        let fixture = fixture(prefab);
        for (root, items) in keyed_cases() {
            let expected: Vec<_> = items
                .iter()
                .map(|(key, _)| {
                    available(list_item(root, key.clone()), ComponentValueKind::Map, true)
                })
                .collect();
            assert_eq!(collect(&fixture, &path(root), 1), expected);
            assert_eq!(collect(&fixture, &path(root), 3), expected);
            for (key, score) in items {
                let selector = list_item(root, key.clone());
                assert_eq!(
                    read(&fixture.workspace, fixture.document, selector.clone()),
                    EditorPropertyValue::Present(keyed_item(key_value(&key), score))
                );
                assert_eq!(
                    collect(&fixture, &selector, 1),
                    [
                        available(member(&selector, "id"), key.value_kind(), true),
                        available(member(&selector, "score"), ComponentValueKind::U64, true),
                    ]
                );
                assert_eq!(
                    read(
                        &fixture.workspace,
                        fixture.document,
                        member(&selector, "score")
                    ),
                    EditorPropertyValue::Present(ComponentValue::U64(score))
                );
            }
        }
        for (root, wrong_key, expected_kind) in [
            (
                "strings",
                ComponentPropertyKey::U64(0),
                ComponentValueKind::String,
            ),
            (
                "signed",
                ComponentPropertyKey::String("-9".into()),
                ComponentValueKind::I64,
            ),
            (
                "unsigned",
                ComponentPropertyKey::I64(7),
                ComponentValueKind::U64,
            ),
        ] {
            let actual_kind = wrong_key.value_kind();
            assert_eq!(
                fixture.workspace.read_document_property(read_request(
                    &fixture.workspace,
                    fixture.document,
                    list_item(root, wrong_key),
                )),
                Err(EditorDocumentQueryRejection::Source(
                    AuthoringReadError::Property(ComponentPropertyError::KeyTypeMismatch {
                        step: 0,
                        expected: expected_kind,
                        actual: actual_kind,
                    })
                ))
            );
        }
    }
}

#[test]
fn admitted_keyed_list_uniqueness_rejects_colliding_identity_edits_without_mutation() {
    for prefab in [false, true] {
        let fixture = fixture(prefab);
        let before = fixture.workspace.authoring_model();
        let basis = fixture
            .workspace
            .document_query_basis(fixture.document)
            .unwrap();
        for (root, items) in keyed_cases() {
            let selector = list_item(root, items[0].0.clone());
            let new_identity = match &items[0].0 {
                ComponentPropertyKey::String(_) => ComponentValue::String("new identity".into()),
                ComponentPropertyKey::I64(_) => ComponentValue::I64(42),
                ComponentPropertyKey::U64(_) => ComponentValue::U64(42),
            };
            let edit = AuthoringPropertyEdit {
                entity: entity(),
                component: component(),
                component_version: ComponentSchemaVersion::ONE,
                property: member(&selector, "id"),
                value: key_value(&items[1].0),
            };
            let valid = AuthoringPropertyEdit {
                value: new_identity,
                ..edit.clone()
            };
            assert!(
                fixture
                    .workspace
                    .prepare_authoring_property_edit(&basis, &[valid])
                    .is_ok()
            );
            assert!(
                fixture
                    .workspace
                    .prepare_authoring_property_edit(&basis, &[edit])
                    .is_err()
            );
            assert_eq!(fixture.workspace.authoring_model(), before);
            assert_eq!(
                read(&fixture.workspace, fixture.document, selector),
                EditorPropertyValue::Present(keyed_item(key_value(&items[0].0), items[0].1))
            );
            assert_eq!(collect(&fixture, &path(root), 1).len(), items.len());
        }
    }
}

#[test]
fn keyed_edit_requires_a_fresh_revision_after_reordering_and_replays_after_schema_readmission() {
    for prefab in [false, true] {
        let mut fixture = fixture(prefab);
        let workspace = &mut fixture.workspace;
        let document = fixture.document;
        let original_item = list_item("strings", ComponentPropertyKey::String("z".into()));
        let renamed_item = list_item("strings", ComponentPropertyKey::String("renamed".into()));
        let edits = [
            AuthoringPropertyEdit {
                entity: entity(),
                component: component(),
                component_version: ComponentSchemaVersion::ONE,
                property: member(&original_item, "id"),
                value: ComponentValue::String("renamed".into()),
            },
            AuthoringPropertyEdit {
                entity: entity(),
                component: component(),
                component_version: ComponentSchemaVersion::ONE,
                property: member(&renamed_item, "score"),
                value: ComponentValue::U64(901),
            },
        ];
        let original_basis = workspace.document_query_basis(document).unwrap();
        let old_edit = workspace
            .prepare_authoring_property_edit(&original_basis, &edits)
            .unwrap();
        let EditorPropertyValue::Present(ComponentValue::List(mut values)) =
            read(workspace, document, path("strings"))
        else {
            panic!("the source list must be present");
        };
        values.rotate_left(1);
        let reordered = ComponentValue::List(values.clone());
        let reorder = workspace
            .prepare_authoring_edit(
                &original_basis,
                &ScenePatchDocument::new([ScenePatchOperation::SetField {
                    entity: entity(),
                    component: component(),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new("strings"),
                    value: reordered.clone(),
                }]),
            )
            .unwrap();
        assert!(workspace.commit_authoring_edit(reorder).applied);
        let after_reorder = workspace.authoring_model();
        let mut old_preview = read_request(workspace, document, member(&renamed_item, "score"));
        old_preview.target.basis = original_basis;
        assert_eq!(
            old_edit.preview().read_property(old_preview).unwrap().value,
            EditorPropertyValue::Present(ComponentValue::U64(901))
        );
        assert!(!workspace.commit_authoring_edit(old_edit).applied);
        assert_eq!(workspace.authoring_model(), after_reorder);

        let current = workspace.document_query_basis(document).unwrap();
        let fresh = workspace
            .prepare_authoring_property_edit(&current, &edits)
            .unwrap();
        assert!(matches!(fresh.inverse().operations.as_slice(), [
            ScenePatchOperation::ReplaceProperty { property: score, value: ComponentValue::U64(101), .. },
            ScenePatchOperation::ReplaceProperty { property: key, value: ComponentValue::String(old_key), .. },
        ] if score == &member(&renamed_item, "score") && key == &member(&renamed_item, "id") && old_key == "z"));
        assert!(workspace.commit_authoring_edit(fresh).applied);
        values[2] = keyed_item(ComponentValue::String("renamed".into()), 901);
        let edited = EditorPropertyValue::Present(ComponentValue::List(values));
        assert_eq!(read(workspace, document, path("strings")), edited);

        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::Undo {
                    document,
                    base_revision: workspace.document_revision(document).unwrap(),
                })
                .applied
        );
        assert_eq!(
            read(workspace, document, path("strings")),
            EditorPropertyValue::Present(reordered)
        );
        let at_undo = workspace.document_query_basis(document).unwrap();
        let successor = fresh_schema_basis();
        let admitted = workspace
            .prepare_schema_reclassification(
                at_undo.schema.as_ref().unwrap(),
                &successor,
                &SchemaReclassificationKind::DeclarationExtension,
                &mut SchemaReclassificationWork::default(),
            )
            .unwrap();
        workspace
            .bind_schema_reclassification(admitted)
            .unwrap()
            .commit();
        assert_eq!(
            workspace.document_revision(document),
            Some(at_undo.revision)
        );
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::Redo {
                    document,
                    base_revision: at_undo.revision,
                })
                .applied
        );
        assert_eq!(read(workspace, document, path("strings")), edited);
    }
}

#[test]
fn missing_explicit_null_and_defaulted_null_never_fabricate_collection_items() {
    for prefab in [false, true] {
        let fixture = fixture(prefab);
        let before = fixture.workspace.authoring_model();
        for suffix in ["map", "keyed"] {
            for (prefix, expected) in [
                ("absent", EditorPropertyValue::Missing),
                ("null", EditorPropertyValue::Present(ComponentValue::Null)),
                (
                    "default_null",
                    EditorPropertyValue::Defaulted(ComponentValue::Null),
                ),
            ] {
                let selector = path(&format!("{prefix}_{suffix}"));
                let page = fixture
                    .workspace
                    .query_document_authoring(query(
                        &fixture.workspace,
                        fixture.document,
                        selector.clone(),
                        limits(1),
                    ))
                    .unwrap();
                assert!(page.rows.is_empty());
                assert!(page.next.is_none());
                assert_eq!(
                    read(&fixture.workspace, fixture.document, selector),
                    expected
                );
            }
        }
        let map_child =
            path("default_map").with_steps([ComponentPropertyStep::MapEntry("default".into())]);
        assert_eq!(
            collect(&fixture, &path("default_map"), 1),
            [available(map_child.clone(), ComponentValueKind::U64, false)]
        );
        assert_eq!(
            read(&fixture.workspace, fixture.document, map_child),
            EditorPropertyValue::Defaulted(ComponentValue::U64(77))
        );
        let keyed_child = list_item(
            "default_keyed",
            ComponentPropertyKey::String("default".into()),
        );
        assert_eq!(
            collect(&fixture, &path("default_keyed"), 1),
            [available(
                keyed_child.clone(),
                ComponentValueKind::Map,
                false
            )]
        );
        assert_eq!(
            read(
                &fixture.workspace,
                fixture.document,
                member(&keyed_child, "score")
            ),
            EditorPropertyValue::Defaulted(ComponentValue::U64(88))
        );
        assert_eq!(fixture.workspace.authoring_model(), before);
    }
}

#[test]
fn record_availability_describes_declarations_while_subsequent_reads_preserve_presence() {
    for prefab in [false, true] {
        let fixture = fixture(prefab);
        let before = fixture.workspace.authoring_model();
        for (prefix, expected_root) in [
            ("absent", EditorPropertyValue::Missing),
            ("null", EditorPropertyValue::Present(ComponentValue::Null)),
            (
                "default_null",
                EditorPropertyValue::Defaulted(ComponentValue::Null),
            ),
        ] {
            let parent = path(&format!("{prefix}_record"));
            let child = member(&parent, "score");
            assert_eq!(
                collect(&fixture, &parent, 1),
                [available(child.clone(), ComponentValueKind::U64, false)]
            );
            assert_eq!(
                read(&fixture.workspace, fixture.document, parent),
                expected_root
            );
            let result = fixture.workspace.read_document_property(read_request(
                &fixture.workspace,
                fixture.document,
                child,
            ));
            if prefix == "absent" {
                assert!(matches!(
                    result,
                    Err(EditorDocumentQueryRejection::Source(
                        AuthoringReadError::Property(ComponentPropertyError::ValueUnavailable(_))
                    ))
                ));
            } else {
                assert_eq!(
                    result,
                    Err(EditorDocumentQueryRejection::Source(
                        AuthoringReadError::Property(ComponentPropertyError::OptionalAbsent {
                            step: 0
                        })
                    ))
                );
            }
        }
        let parent = path("default_record");
        let child = member(&parent, "score");
        assert_eq!(
            collect(&fixture, &parent, 1),
            [available(child.clone(), ComponentValueKind::U64, false)]
        );
        assert_eq!(
            read(&fixture.workspace, fixture.document, child),
            EditorPropertyValue::Defaulted(ComponentValue::U64(99))
        );
        assert_eq!(fixture.workspace.authoring_model(), before);
    }
}

#[test]
fn record_declarations_below_collection_steps_require_an_actual_parent_identity() {
    for prefab in [false, true] {
        let fixture = fixture(prefab);
        let before = fixture.workspace.authoring_model();
        let parent =
            path("records").with_steps([ComponentPropertyStep::MapEntry("present".into())]);
        assert_eq!(
            collect(&fixture, &parent, 1),
            [
                available(member(&parent, "score"), ComponentValueKind::U64, true),
                retired(member(&parent, "gone")),
            ]
        );
        assert_eq!(
            read(
                &fixture.workspace,
                fixture.document,
                member(&parent, "score")
            ),
            EditorPropertyValue::Present(ComponentValue::U64(55))
        );
        for (parent, expected) in [
            (
                path("records").with_steps([ComponentPropertyStep::MapEntry("missing".into())]),
                ComponentPropertyError::MapEntryNotFound {
                    step: 0,
                    key: "missing".into(),
                },
            ),
            (
                list_item("strings", ComponentPropertyKey::String("missing".into())),
                ComponentPropertyError::ListItemNotFound {
                    step: 0,
                    key: ComponentPropertyKey::String("missing".into()),
                },
            ),
        ] {
            assert_eq!(
                fixture.workspace.query_document_authoring(query(
                    &fixture.workspace,
                    fixture.document,
                    parent,
                    limits(4),
                )),
                Err(EditorDocumentQueryRejection::Source(
                    AuthoringReadError::Property(expected)
                ))
            );
        }
        assert_eq!(fixture.workspace.authoring_model(), before);
    }
}

#[test]
fn ordinary_list_children_refuse_without_exposing_positional_identity() {
    for prefab in [false, true] {
        let fixture = fixture(prefab);
        let before = fixture.workspace.authoring_model();
        assert!(matches!(
            fixture.workspace.query_document_authoring(query(
                &fixture.workspace,
                fixture.document,
                path("list"),
                limits(16),
            )),
            Err(EditorDocumentQueryRejection::Source(
                AuthoringReadError::Property(ComponentPropertyError::UnkeyedList { .. })
            ))
        ));
        assert_eq!(
            read(&fixture.workspace, fixture.document, path("list")),
            EditorPropertyValue::Present(ComponentValue::List(vec![ComponentValue::U64(1)]))
        );
        assert_eq!(fixture.workspace.authoring_model(), before);
    }
}

#[test]
fn complete_property_metadata_including_utf8_and_cursor_anchor_has_an_exact_boundary() {
    for prefab in [false, true] {
        let fixture = fixture(prefab);
        let before = fixture.workspace.authoring_model();
        let target_bytes = ENTITY.len() + COMPONENT.len() + "map".len();
        let all_row_bytes = ["", "a", "a/b", "z", "é"]
            .iter()
            .map(|key| "map".len() + key.len())
            .sum::<usize>();
        for (rows, expected_bytes) in [
            (2, 2 * target_bytes + 2 * "map".len() + 2 * "a".len()),
            (5, target_bytes + all_row_bytes),
        ] {
            let mut request = query(
                &fixture.workspace,
                fixture.document,
                path("map"),
                limits(rows),
            );
            let page = fixture
                .workspace
                .query_document_authoring(request.clone())
                .unwrap();
            assert_eq!(page.metadata_bytes, expected_bytes);
            assert_eq!(page.next.is_some(), rows == 2);
            request.limits = EditorAuthoringQueryLimits::new(256, rows, expected_bytes).unwrap();
            assert_eq!(
                fixture
                    .workspace
                    .query_document_authoring(request.clone())
                    .unwrap(),
                page
            );
            request.limits =
                EditorAuthoringQueryLimits::new(256, rows, expected_bytes - 1).unwrap();
            assert_eq!(
                fixture.workspace.query_document_authoring(request.clone()),
                Err(EditorDocumentQueryRejection::MetadataBudgetExceeded)
            );
            request.limits = limits(rows);
            assert_eq!(
                fixture.workspace.query_document_authoring(request).unwrap(),
                page
            );
        }
        assert_eq!(fixture.workspace.authoring_model(), before);
    }
}

fn assert_nonempty_progress(page: &EditorAuthoringPage, expected_first: &EditorAuthoringRow) {
    assert!(
        !page.rows.is_empty(),
        "a nonempty collection must not yield an unadvanced cursor"
    );
    assert_eq!(&page.rows[0], expected_first);
}

#[test]
fn anonymous_declarations_advance_empty_pages_without_inventing_a_durable_selector() {
    for prefab in [false, true] {
        let fixture = fixture(prefab);
        let before = fixture.workspace.authoring_model();
        let parent = path("record");
        let page_limits = EditorAuthoringQueryLimits::new(1, 1, 65_536).unwrap();
        let request = query(
            &fixture.workspace,
            fixture.document,
            parent.clone(),
            page_limits,
        );
        let mut page = fixture
            .workspace
            .query_document_authoring(request.clone())
            .unwrap();
        assert!(page.rows.is_empty());
        assert!(page.next.is_some());
        assert_eq!(
            fixture.workspace.query_document_authoring(request).unwrap(),
            page
        );
        let mut collected = Vec::new();
        let mut cursors = Vec::new();
        let mut complete = false;
        for _ in 0..32 {
            assert_eq!(page.scanned_entries, 1);
            assert!(page.scanned_logical_bytes > 0);
            assert!(page.rows.len() <= 1);
            collected.extend(page.rows.iter().cloned());
            let Some(cursor) = page.next.as_ref() else {
                complete = true;
                break;
            };
            assert!(
                !cursors.contains(cursor),
                "empty pages must still advance their cursor"
            );
            cursors.push(cursor.clone());
            page = continue_page(&fixture.workspace, &page, page_limits);
        }
        assert!(
            complete,
            "anonymous declarations and irrelevant tombstones must terminate"
        );
        assert_eq!(collected, collect(&fixture, &parent, 4));
        assert_eq!(fixture.workspace.authoring_model(), before);
    }
}

#[test]
fn exhausting_parent_lookup_or_child_scan_never_returns_a_nonprogress_cursor() {
    for prefab in [false, true] {
        let fixture = fixture(prefab);
        let before = fixture.workspace.authoring_model();
        for parent in [
            path("map"),
            list_item("strings", ComponentPropertyKey::String("z".into())),
        ] {
            let request = query(&fixture.workspace, fixture.document, parent, limits(1));
            let first = fixture
                .workspace
                .query_document_authoring(request.clone())
                .unwrap();
            for entries in 1..=first.scanned_entries {
                let mut bounded = request.clone();
                bounded.limits = EditorAuthoringQueryLimits::new(entries, 1, 65_536).unwrap();
                match fixture.workspace.query_document_authoring(bounded) {
                    Ok(page) => {
                        assert!(page.scanned_entries <= entries);
                        assert_nonempty_progress(&page, &first.rows[0]);
                    }
                    Err(error) => assert_eq!(error, budget_refusal()),
                }
            }
            let cursor = first.next.clone().unwrap();
            let second = continue_page(&fixture.workspace, &first, limits(1));
            for entries in 1..=second.scanned_entries {
                let continuation = EditorAuthoringQuery {
                    position: EditorAuthoringQueryPosition::Continue(cursor.clone()),
                    limits: EditorAuthoringQueryLimits::new(entries, 1, 65_536).unwrap(),
                };
                match fixture.workspace.query_document_authoring(continuation) {
                    Ok(page) => {
                        assert!(page.scanned_entries <= entries);
                        assert_nonempty_progress(&page, &second.rows[0]);
                        assert_ne!(page.next.as_ref(), Some(&cursor));
                    }
                    Err(error) => assert_eq!(error, budget_refusal()),
                }
            }
        }
        assert_eq!(fixture.workspace.authoring_model(), before);
    }
}

#[test]
fn logical_scan_accounting_is_inclusive_and_exhaustion_cannot_claim_a_complete_page() {
    for prefab in [false, true] {
        let fixture = fixture(prefab);
        let mut request = query(&fixture.workspace, fixture.document, path("map"), limits(5));
        let complete = fixture
            .workspace
            .query_document_authoring(request.clone())
            .unwrap();
        assert!(complete.scanned_logical_bytes > 1);
        request.limits = limits(5)
            .with_scan_logical_bytes(complete.scanned_logical_bytes)
            .unwrap();
        assert_eq!(
            fixture
                .workspace
                .query_document_authoring(request.clone())
                .unwrap(),
            complete
        );
        for bytes in [1, complete.scanned_logical_bytes - 1] {
            request.limits = limits(5).with_scan_logical_bytes(bytes).unwrap();
            match fixture.workspace.query_document_authoring(request.clone()) {
                Err(error) => assert_eq!(error, budget_refusal()),
                Ok(partial) => {
                    assert!(partial.scanned_logical_bytes <= bytes);
                    assert!(!partial.rows.is_empty());
                    assert!(partial.rows.len() < complete.rows.len());
                    assert_eq!(partial.rows, complete.rows[..partial.rows.len()]);
                    assert!(partial.next.is_some());
                    let next = continue_page(&fixture.workspace, &partial, limits(5));
                    let mut recovered = partial.rows;
                    recovered.extend(next.rows);
                    assert!(next.next.is_none());
                    assert_eq!(recovered, complete.rows);
                }
            }
        }
    }
}

#[test]
fn raw_selector_metadata_is_admitted_before_huge_or_deep_selector_diagnostics() {
    for prefab in [false, true] {
        let fixture = fixture(prefab);
        let before = fixture.workspace.authoring_model();
        let huge = "x".repeat(70_000);
        let deep = path("map").with_steps(vec![
            ComponentPropertyStep::Member(
                ComponentFieldId::new("x")
            );
            ComponentPropertyPath::MAX_STEPS + 1
        ]);
        let mut deep_and_huge = deep.clone();
        deep_and_huge
            .steps
            .push(ComponentPropertyStep::MapEntry(huge.clone()));
        let selectors = [
            path(&huge),
            path("map").with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(&huge))]),
            path("map").with_steps([ComponentPropertyStep::MapEntry(huge.clone())]),
            list_item("strings", ComponentPropertyKey::String(huge)),
            deep_and_huge,
        ];
        for selector in selectors {
            assert_eq!(
                fixture.workspace.query_document_authoring(query(
                    &fixture.workspace,
                    fixture.document,
                    selector.clone(),
                    limits(1),
                )),
                Err(EditorDocumentQueryRejection::MetadataBudgetExceeded)
            );
            assert_eq!(
                fixture.workspace.read_document_property(read_request(
                    &fixture.workspace,
                    fixture.document,
                    selector,
                )),
                Err(EditorDocumentQueryRejection::MetadataBudgetExceeded)
            );
        }
        let depth_refusal = EditorDocumentQueryRejection::Source(AuthoringReadError::Property(
            ComponentPropertyError::PathDepthLimit,
        ));
        assert_eq!(
            fixture.workspace.query_document_authoring(query(
                &fixture.workspace,
                fixture.document,
                deep.clone(),
                limits(1),
            )),
            Err(depth_refusal.clone())
        );
        assert_eq!(
            fixture.workspace.read_document_property(read_request(
                &fixture.workspace,
                fixture.document,
                deep,
            )),
            Err(depth_refusal)
        );
        assert_eq!(fixture.workspace.authoring_model(), before);
    }
}

#[test]
fn property_pages_ignore_active_selection_and_owned_target_changes_cannot_retarget_cursors() {
    for prefab in [false, true] {
        let mut fixture = fixture(prefab);
        let request = query(&fixture.workspace, fixture.document, path("map"), limits(1));
        let first = fixture
            .workspace
            .query_document_authoring(request.clone())
            .unwrap();
        let second = continue_page(&fixture.workspace, &first, limits(1));
        assert!(
            fixture
                .workspace
                .apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                    document: fixture.document,
                    entity: Some(SceneEntityId::new("other").unwrap()),
                })
                .applied
        );
        let active = fixture
            .workspace
            .open_scene_session(
                "active elsewhere",
                SceneAuthoringSession::new(SceneDocument::default()),
            )
            .unwrap()
            .opened_document
            .unwrap();
        let before = fixture.workspace.authoring_model();
        assert_eq!(
            fixture.workspace.query_document_authoring(request).unwrap(),
            first
        );
        let mut displayed = first;
        displayed.target.entity = SceneEntityId::new("other").unwrap();
        displayed.target.projection = EditorAuthoringProjection::Properties {
            component: component(),
            property: path("locked"),
        };
        assert_eq!(
            continue_page(&fixture.workspace, &displayed, limits(1)),
            second
        );
        assert_eq!(fixture.workspace.active_document(), Some(active));
        assert_eq!(fixture.workspace.authoring_model(), before);
    }
}

#[test]
fn property_cursors_refuse_foreign_workspaces_and_old_revisions_even_after_undo() {
    for prefab in [false, true] {
        let mut fixture = fixture(prefab);
        let first = fixture
            .workspace
            .query_document_authoring(query(
                &fixture.workspace,
                fixture.document,
                path("map"),
                limits(1),
            ))
            .unwrap();
        let continuation = EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Continue(first.next.clone().unwrap()),
            limits: limits(1),
        };
        let other = support::fixture(prefab);
        assert_eq!(other.document, fixture.document);
        let foreign_before = other.workspace.authoring_model();
        assert_eq!(
            other
                .workspace
                .query_document_authoring(continuation.clone()),
            Err(EditorDocumentQueryRejection::WrongWorkspace)
        );
        assert_eq!(other.workspace.authoring_model(), foreign_before);
        let old_read = read_request(&fixture.workspace, fixture.document, path("map"));
        let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: entity(),
            component: component(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("amount"),
            value: ComponentValue::U64(9),
        }]);
        let prepared = fixture
            .workspace
            .prepare_authoring_edit(&first.target.basis, &patch)
            .unwrap();
        assert!(fixture.workspace.commit_authoring_edit(prepared).applied);
        for undo in [false, true] {
            if undo {
                let revision = fixture
                    .workspace
                    .document_revision(fixture.document)
                    .unwrap();
                assert!(
                    fixture
                        .workspace
                        .apply_authoring_command(EditorWorkspaceCommand::Undo {
                            document: fixture.document,
                            base_revision: revision,
                        })
                        .applied
                );
                assert_eq!(
                    read(&fixture.workspace, fixture.document, path("amount")),
                    EditorPropertyValue::Present(ComponentValue::U64(7))
                );
            }
            let before = fixture.workspace.authoring_model();
            assert_eq!(
                fixture
                    .workspace
                    .query_document_authoring(continuation.clone()),
                Err(EditorDocumentQueryRejection::StaleRevision)
            );
            assert_eq!(
                fixture.workspace.read_document_property(old_read.clone()),
                Err(EditorDocumentQueryRejection::StaleRevision)
            );
            assert_eq!(fixture.workspace.authoring_model(), before);
        }
    }
}

#[test]
fn same_content_reopen_preserves_schema_but_invalidates_property_cursors() {
    for prefab in [false, true] {
        let mut fixture = fixture(prefab);
        let first = fixture
            .workspace
            .query_document_authoring(query(
                &fixture.workspace,
                fixture.document,
                path("map"),
                limits(1),
            ))
            .unwrap();
        let digest = EditorDocumentDigest::new(7, [0x5a; 32]);
        if prefab {
            let session = fixture
                .workspace
                .prefab(fixture.document)
                .unwrap()
                .session();
            let candidate = PrefabDocumentCandidate::decode_json_bytes(
                session.document().to_json_string().unwrap().as_bytes(),
            )
            .unwrap();
            let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
            fixture
                .authority
                .publish_reopened_prefab_session(
                    &mut fixture.workspace,
                    fixture.document,
                    first.target.basis.revision,
                    EditorDocumentSourceBinding::new(PREFAB_SOURCE),
                    reopened,
                    digest,
                )
                .unwrap();
        } else {
            let session = fixture.workspace.scene(fixture.document).unwrap().session();
            let candidate = SceneDocumentCandidate::decode_json_bytes(
                session.document().to_json_string().unwrap().as_bytes(),
            )
            .unwrap();
            let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
            fixture
                .authority
                .publish_reopened_session(
                    &mut fixture.workspace,
                    fixture.document,
                    first.target.basis.revision,
                    reopened,
                    digest,
                )
                .unwrap();
        }
        let current = fixture
            .workspace
            .document_query_basis(fixture.document)
            .unwrap();
        assert_eq!(current.schema, first.target.basis.schema);
        assert_ne!(
            current.revision.source_id(),
            first.target.basis.revision.source_id()
        );
        let before = fixture.workspace.authoring_model();
        assert_eq!(
            fixture
                .workspace
                .query_document_authoring(EditorAuthoringQuery {
                    position: EditorAuthoringQueryPosition::Continue(first.next.unwrap()),
                    limits: limits(1),
                }),
            Err(EditorDocumentQueryRejection::StaleIncarnation)
        );
        let fresh = fixture
            .workspace
            .query_document_authoring(query(
                &fixture.workspace,
                fixture.document,
                path("map"),
                limits(1),
            ))
            .unwrap();
        assert_eq!(fresh.rows, first.rows);
        assert_eq!(fixture.workspace.authoring_model(), before);
    }
}

#[test]
fn equal_catalog_readmission_invalidates_property_cursors_without_changing_the_revision() {
    for prefab in [false, true] {
        let mut fixture = fixture(prefab);
        let first = fixture
            .workspace
            .query_document_authoring(query(
                &fixture.workspace,
                fixture.document,
                path("map"),
                limits(1),
            ))
            .unwrap();
        let amount = read_request(&fixture.workspace, fixture.document, path("amount"));
        let edit = fixture
            .workspace
            .prepare_authoring_edit(
                &amount.target.basis,
                &ScenePatchDocument::new([ScenePatchOperation::SetField {
                    entity: entity(),
                    component: component(),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new("amount"),
                    value: ComponentValue::U64(70),
                }]),
            )
            .unwrap();
        let page_limits = EditorPropertyReadLimits::new(1024, 2, 32).unwrap();
        let first_preview = edit
            .preview()
            .read_property_page(EditorPropertyPageRead {
                position: EditorPropertyPagePosition::Start {
                    target: read_request(&fixture.workspace, fixture.document, path("map")).target,
                    path: ComponentFieldPath::empty(),
                },
                limits: page_limits,
            })
            .unwrap();
        let resume = EditorPropertyPageRead {
            position: EditorPropertyPagePosition::Continue(first_preview.next.unwrap()),
            limits: page_limits,
        };
        let second_preview = edit.preview().read_property_page(resume.clone()).unwrap();
        let previous = first.target.basis.schema.clone().unwrap();
        let successor = fresh_schema_basis();
        assert_ne!(previous, successor);
        let prepared = fixture
            .workspace
            .prepare_schema_reclassification(
                &previous,
                &successor,
                &SchemaReclassificationKind::DeclarationExtension,
                &mut SchemaReclassificationWork::default(),
            )
            .unwrap();
        fixture
            .workspace
            .bind_schema_reclassification(prepared)
            .unwrap()
            .commit();
        let current = fixture
            .workspace
            .document_query_basis(fixture.document)
            .unwrap();
        assert_eq!(current.revision, first.target.basis.revision);
        assert_eq!(current.schema, Some(successor));
        let before = fixture.workspace.authoring_model();
        let mut current_amount = amount.clone();
        current_amount.target.basis = current;
        assert_eq!(
            edit.preview().read_property(current_amount.clone()),
            Err(EditorDocumentQueryRejection::StaleSchema)
        );
        assert_eq!(
            edit.preview().read_property(amount).unwrap().value,
            EditorPropertyValue::Present(ComponentValue::U64(70))
        );
        assert_eq!(
            edit.preview().read_property_page(resume).unwrap(),
            second_preview
        );
        assert!(!fixture.workspace.commit_authoring_edit(edit).applied);
        assert_eq!(
            fixture
                .workspace
                .read_document_property(current_amount)
                .unwrap()
                .value,
            EditorPropertyValue::Present(ComponentValue::U64(7))
        );
        assert_eq!(
            fixture
                .workspace
                .query_document_authoring(EditorAuthoringQuery {
                    position: EditorAuthoringQueryPosition::Continue(first.next.unwrap()),
                    limits: limits(1),
                }),
            Err(EditorDocumentQueryRejection::StaleSchema)
        );
        let fresh = fixture
            .workspace
            .query_document_authoring(query(
                &fixture.workspace,
                fixture.document,
                path("map"),
                limits(1),
            ))
            .unwrap();
        assert_eq!(fresh.rows, first.rows);
        assert_eq!(fixture.workspace.authoring_model(), before);
    }
}

#[test]
fn property_logical_scan_limits_require_positive_values_with_an_inclusive_four_mib_cap() {
    assert!(limits(1).with_scan_logical_bytes(1).is_ok());
    assert!(limits(1).with_scan_logical_bytes(4 * 1024 * 1024).is_ok());
    for bytes in [0, 4 * 1024 * 1024 + 1, usize::MAX] {
        assert_eq!(
            limits(1).with_scan_logical_bytes(bytes),
            Err(EditorDocumentQueryRejection::InvalidAuthoringLimits)
        );
    }
}
