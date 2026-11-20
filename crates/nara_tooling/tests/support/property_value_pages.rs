use nara_reflect::{ComponentFieldPathSegment, ComponentPropertyError};
use nara_tooling::{
    EditorPropertyPage, EditorPropertyPageData, EditorPropertyPageItem, EditorPropertyPagePosition,
    EditorPropertyPageRead, EditorPropertyPageValue,
};

use super::*;

fn start(
    workspace: &EditorWorkspace,
    document: EditorDocumentId,
    field: &str,
    limits: EditorPropertyReadLimits,
) -> EditorPropertyPageRead {
    EditorPropertyPageRead {
        position: EditorPropertyPagePosition::Start {
            target: read(workspace, document, field, 8).target,
            path: ComponentFieldPath::empty(),
        },
        limits,
    }
}

fn resume(page: &EditorPropertyPage, limits: EditorPropertyReadLimits) -> EditorPropertyPageRead {
    EditorPropertyPageRead {
        position: EditorPropertyPagePosition::Continue(page.next.clone().unwrap()),
        limits,
    }
}

fn scan_exhausted() -> EditorDocumentQueryRejection {
    EditorDocumentQueryRejection::Source(AuthoringReadError::from(
        ComponentPropertyError::ReadBudgetExceeded,
    ))
}

#[test]
fn prepared_property_reads_keep_the_original_basis_and_presence() {
    for prefab in [false, true] {
        let (mut workspace, document) = fixture(prefab);
        let original = read(&workspace, document, "amount", 8);
        let patch = |value| {
            ScenePatchDocument::new([ScenePatchOperation::SetField {
                entity: original.target.entity.clone(),
                component: original.target.component.clone(),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("amount"),
                value: ComponentValue::U64(value),
            }])
        };
        let prepared = workspace
            .prepare_authoring_edit(&original.target.basis, &patch(70))
            .unwrap();
        let model = workspace.authoring_model();
        for (field, expected) in [
            (
                "defaulted",
                EditorPropertyValue::Defaulted(ComponentValue::U64(11)),
            ),
            ("missing", EditorPropertyValue::Missing),
            (
                "nullable",
                EditorPropertyValue::Present(ComponentValue::Null),
            ),
        ] {
            let request = read(&workspace, document, field, 8);
            let preview = prepared.preview_property(request.clone()).unwrap();
            assert_eq!(preview.value, expected);
            assert_eq!(preview, workspace.read_document_property(request).unwrap());
        }
        assert_eq!(workspace.authoring_model(), model);

        let mut invalid = original.clone();
        invalid.target.basis.schema = None;
        invalid.limits = EditorPropertyReadLimits::new(1, 1, 1).unwrap();
        assert_eq!(
            prepared.preview_property(invalid),
            Err(EditorDocumentQueryRejection::StaleSchema)
        );
        let mut invalid = original.clone();
        invalid.limits = EditorPropertyReadLimits::new(1024, 1, 7).unwrap();
        assert_eq!(
            prepared.preview_property(invalid),
            Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
        );

        let successor = workspace
            .prepare_authoring_edit(&original.target.basis, &patch(80))
            .unwrap();
        assert!(workspace.commit_authoring_edit(successor).applied);
        let current = read(&workspace, document, "amount", 8);
        assert_eq!(
            prepared.preview_property(current.clone()),
            Err(EditorDocumentQueryRejection::StaleRevision)
        );
        assert_eq!(
            prepared.preview_property(original).unwrap().value,
            EditorPropertyValue::Present(ComponentValue::U64(70))
        );
        assert!(!workspace.commit_authoring_edit(prepared).applied);
        assert_eq!(
            workspace.read_document_property(current).unwrap().value,
            EditorPropertyValue::Present(ComponentValue::U64(80))
        );
    }
}

#[test]
fn large_unkeyed_list_pages_visit_only_the_current_slice() {
    const COUNT: usize = 5_001;
    for prefab in [false, true] {
        let (workspace, document) = fixture_with_values(
            prefab,
            0,
            String::new(),
            (0..COUNT as u64).map(ComponentValue::U64).collect(),
            None,
        );
        let limits = EditorPropertyReadLimits::new(1024, 128, 127 * 8)
            .unwrap()
            .with_scan_limits(127, 1)
            .unwrap();
        let mut whole = read(&workspace, document, "tree", 8);
        whole.limits = limits;
        assert_eq!(
            workspace.read_document_property(whole),
            Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
        );
        let mut request = start(&workspace, document, "tree", limits);
        let before = workspace.authoring_model();
        let mut consumed = 0;
        loop {
            let page = workspace.read_document_property_page(request).unwrap();
            let EditorPropertyPageValue::Present(EditorPropertyPageData::List {
                total_items,
                start,
                items,
            }) = &page.value
            else {
                panic!("expected list page")
            };
            let length = (COUNT - consumed).min(127);
            assert_eq!(
                (*total_items, *start, items.len()),
                (COUNT, consumed, length)
            );
            assert_eq!(
                (page.value_nodes, page.value_logical_bytes),
                (1 + length, length * 8)
            );
            assert_eq!(
                (page.scanned_entries, page.scanned_logical_bytes),
                (length, 0)
            );
            for item in items {
                assert_eq!(
                    item,
                    &EditorPropertyPageItem::Scalar(ComponentValue::U64(consumed as u64))
                );
                consumed += 1;
            }
            let Some(next) = page.next else { break };
            request = EditorPropertyPageRead {
                position: EditorPropertyPagePosition::Continue(next),
                limits,
            };
        }
        assert_eq!(consumed, COUNT);
        assert_eq!(workspace.authoring_model(), before);
    }
}

#[test]
fn utf8_pages_replay_and_never_return_a_nonprogress_cursor() {
    let expected = "aé🦀中".repeat(1_250);
    let (workspace, document) = fixture_with_values(false, 0, expected.clone(), Vec::new(), None);
    let tiny = EditorPropertyReadLimits::new(1024, 1, 3).unwrap();
    let empty = workspace
        .read_document_property_page(start(&workspace, document, "tree", tiny))
        .unwrap();
    assert_eq!(
        empty.value,
        EditorPropertyPageValue::Present(EditorPropertyPageData::List {
            total_items: 0,
            start: 0,
            items: Vec::new(),
        })
    );
    assert_eq!(
        (empty.value_nodes, empty.value_logical_bytes, empty.next),
        (1, 0, None)
    );
    let first = workspace
        .read_document_property_page(start(&workspace, document, "blob", tiny))
        .unwrap();
    assert_eq!(
        first.value,
        EditorPropertyPageValue::Present(EditorPropertyPageData::Text {
            total_bytes: expected.len(),
            start: 0,
            text: "aé".into(),
        })
    );
    assert_eq!(
        workspace.read_document_property_page(resume(&first, tiny)),
        Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
    );
    let chunked = EditorPropertyReadLimits::new(1024, 1, 121).unwrap();
    let mut request = resume(&first, chunked);
    let mut reconstructed = String::from("aé");
    loop {
        let page = workspace
            .read_document_property_page(request.clone())
            .unwrap();
        assert_eq!(
            workspace.read_document_property_page(request).unwrap(),
            page
        );
        let EditorPropertyPageValue::Present(EditorPropertyPageData::Text {
            total_bytes,
            start,
            text,
        }) = &page.value
        else {
            panic!("expected text page")
        };
        assert_eq!(
            (*total_bytes, *start),
            (expected.len(), reconstructed.len())
        );
        assert!(!text.is_empty());
        assert_eq!(page.value_logical_bytes, text.len());
        assert_eq!(page.value_nodes, 1);
        reconstructed.push_str(text);
        let Some(next) = page.next else { break };
        request = EditorPropertyPageRead {
            position: EditorPropertyPagePosition::Continue(next),
            limits: chunked,
        };
    }
    assert_eq!(reconstructed, expected);
}

#[test]
fn map_resume_and_relative_read_charge_only_the_selected_path_and_children() {
    let (workspace, document) = fixture(true);
    let first_limits = EditorPropertyReadLimits::new(1024, 2, "wire_first".len() + 8)
        .unwrap()
        .with_scan_limits(1, "wire_first".len())
        .unwrap();
    let first = workspace
        .read_document_property_page(start(&workspace, document, "record", first_limits))
        .unwrap();
    assert_eq!(
        first.value,
        EditorPropertyPageValue::Present(EditorPropertyPageData::Map {
            total_entries: 4,
            entries: vec![(
                "wire_first".into(),
                EditorPropertyPageItem::Scalar(ComponentValue::U64(13))
            )],
        })
    );
    assert_eq!(
        (first.scanned_entries, first.scanned_logical_bytes),
        (1, "wire_first".len())
    );
    assert_eq!(
        (first.value_nodes, first.value_logical_bytes),
        (2, "wire_first".len() + 8)
    );
    let only_one_probe = EditorPropertyReadLimits::new(1024, 20, 1024)
        .unwrap()
        .with_scan_limits(1, 1024)
        .unwrap();
    assert_eq!(
        first,
        workspace
            .read_document_property_page(start(&workspace, document, "record", only_one_probe))
            .unwrap()
    );
    let too_few_bytes = EditorPropertyReadLimits::new(1024, 2, "wire_first".len() + 7).unwrap();
    assert_eq!(
        workspace.read_document_property_page(start(&workspace, document, "record", too_few_bytes)),
        Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
    );
    assert_eq!(
        workspace.read_document_property_page(resume(&first, first_limits)),
        Err(scan_exhausted())
    );
    let branch_limits = EditorPropertyReadLimits::new(1024, 2, "wire_nested".len())
        .unwrap()
        .with_scan_limits(1, "wire_nested".len())
        .unwrap();
    let second = workspace
        .read_document_property_page(resume(&first, branch_limits))
        .unwrap();
    assert_eq!(
        second.value,
        EditorPropertyPageValue::Present(EditorPropertyPageData::Map {
            total_entries: 4,
            entries: vec![("wire_nested".into(), EditorPropertyPageItem::Map { len: 1 })],
        })
    );
    assert_eq!(
        (second.scanned_entries, second.scanned_logical_bytes),
        (1, "wire_nested".len())
    );
    assert_eq!(
        (second.value_nodes, second.value_logical_bytes),
        (2, "wire_nested".len())
    );
    let rest = workspace
        .read_document_property_page(resume(
            &second,
            EditorPropertyReadLimits::new(1024, 3, 1024).unwrap(),
        ))
        .unwrap();
    assert_eq!(
        rest.value,
        EditorPropertyPageValue::Present(EditorPropertyPageData::Map {
            total_entries: 4,
            entries: vec![
                (
                    "wire_only".into(),
                    EditorPropertyPageItem::Scalar(ComponentValue::Bool(false))
                ),
                (
                    "wire_second".into(),
                    EditorPropertyPageItem::Text { bytes: 6 }
                ),
            ],
        })
    );
    assert!(rest.next.is_none());
    let exact_path = ComponentFieldPath::from_fields(["wire_nested", "wire_leaf"]);
    let path_bytes = "wire_nested".len() + "wire_leaf".len();
    let metadata = first.metadata_bytes;
    let limits = EditorPropertyReadLimits::new(metadata, 1, 1)
        .unwrap()
        .with_scan_limits(2, path_bytes)
        .unwrap();
    let request = EditorPropertyPageRead {
        position: EditorPropertyPagePosition::Start {
            target: first.target().clone(),
            path: exact_path.clone(),
        },
        limits,
    };
    let leaf = workspace
        .read_document_property_page(request.clone())
        .unwrap();
    assert_eq!(leaf.path(), &exact_path);
    assert_eq!(
        leaf.value,
        EditorPropertyPageValue::Present(EditorPropertyPageData::Scalar(ComponentValue::Bool(
            true
        )))
    );
    assert_eq!(
        (
            leaf.metadata_bytes,
            leaf.scanned_entries,
            leaf.scanned_logical_bytes
        ),
        (metadata, 2, path_bytes)
    );
    assert_eq!(
        workspace.read_document_property_page(EditorPropertyPageRead {
            limits: EditorPropertyReadLimits::new(metadata - 1, 1, 1).unwrap(),
            ..request
        }),
        Err(EditorDocumentQueryRejection::MetadataBudgetExceeded)
    );
    assert_eq!(
        workspace.read_document_property_page(EditorPropertyPageRead {
            position: EditorPropertyPagePosition::Start {
                target: first.target().clone(),
                path: ComponentFieldPath::new([ComponentFieldPathSegment::index(0)])
            },
            limits,
        }),
        Err(EditorDocumentQueryRejection::InvalidValuePath { step: 0 })
    );
}

#[test]
fn scalar_presence_and_continuation_preserve_exact_source_basis() {
    use EditorPropertyPageValue::{Defaulted, Missing, Present};
    let (mut workspace, document) = fixture(false);
    let limits = EditorPropertyReadLimits::new(1024, 1, 8).unwrap();
    for (field, expected, nodes, bytes) in [
        ("missing", Missing, 0, 0),
        (
            "nullable",
            Present(EditorPropertyPageData::Scalar(ComponentValue::Null)),
            1,
            0,
        ),
        (
            "null_default",
            Defaulted(EditorPropertyPageData::Scalar(ComponentValue::Null)),
            1,
            0,
        ),
        (
            "defaulted",
            Defaulted(EditorPropertyPageData::Scalar(ComponentValue::U64(11))),
            1,
            8,
        ),
    ] {
        let page = workspace
            .read_document_property_page(start(&workspace, document, field, limits))
            .unwrap();
        assert_eq!(page.value, expected);
        assert_eq!((page.value_nodes, page.value_logical_bytes), (nodes, bytes));
        assert!(page.next.is_none());
    }
    let limits = EditorPropertyReadLimits::new(1024, 2, 8).unwrap();
    let request = start(&workspace, document, "tree", limits);
    let first = workspace
        .read_document_property_page(request.clone())
        .unwrap();
    let continuation = resume(&first, limits);
    let (other, _) = fixture(false);
    assert_eq!(
        other.read_document_property_page(continuation.clone()),
        Err(EditorDocumentQueryRejection::WrongWorkspace)
    );
    let mut wrong_schema = request;
    let EditorPropertyPagePosition::Start { target, .. } = &mut wrong_schema.position else {
        unreachable!()
    };
    target.basis.schema = None;
    assert_eq!(
        workspace.read_document_property_page(wrong_schema),
        Err(EditorDocumentQueryRejection::StaleSchema)
    );
    let terminal = workspace
        .read_document_property_page(continuation.clone())
        .unwrap();
    workspace
        .open_scene_session(
            "other",
            SceneAuthoringSession::new(SceneDocument::default()),
        )
        .unwrap();
    assert_eq!(
        workspace
            .read_document_property_page(continuation.clone())
            .unwrap(),
        terminal
    );
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id("item"),
        component: ComponentTypeId::new("game.Data"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("amount"),
        value: ComponentValue::U64(9),
    }]);
    let prepared = workspace
        .prepare_authoring_edit(&first.target().basis, &patch)
        .unwrap();
    assert!(workspace.commit_authoring_edit(prepared).applied);
    assert_eq!(
        workspace.read_document_property_page(continuation.clone()),
        Err(EditorDocumentQueryRejection::StaleRevision)
    );
    assert!(
        workspace
            .apply_authoring_command(EditorWorkspaceCommand::Undo {
                document,
                base_revision: workspace.document_revision(document).unwrap()
            })
            .applied
    );
    assert_eq!(
        workspace.read_document_property_page(continuation),
        Err(EditorDocumentQueryRejection::StaleRevision)
    );
    assert_eq!(
        terminal.value,
        Present(EditorPropertyPageData::List {
            total_items: 2,
            start: 1,
            items: vec![EditorPropertyPageItem::Scalar(ComponentValue::U64(2))]
        })
    );
}

#[test]
fn a_returned_long_map_key_can_open_text_beyond_the_complete_copy_limit() {
    let key = "辻".repeat(24_000);
    let text = "a🦀".repeat(60_000);
    let (workspace, document) = fixture_with_values(
        false,
        0,
        String::new(),
        Vec::new(),
        Some((
            ComponentValueShape::map(ComponentValueKind::String.into()),
            ComponentValue::map([(key.clone(), ComponentValue::String(text.clone()))]),
        )),
    );
    let limits = EditorPropertyReadLimits::new(1024, 2, key.len()).unwrap();
    let page = workspace
        .read_document_property_page(start(&workspace, document, "paged", limits))
        .unwrap();
    assert_eq!(page.value_logical_bytes, key.len());
    let EditorPropertyPageValue::Present(EditorPropertyPageData::Map {
        total_entries: 1,
        entries,
    }) = &page.value
    else {
        panic!("expected the admitted long Map key");
    };
    let [(actual, EditorPropertyPageItem::Text { bytes })] = entries.as_slice() else {
        panic!("expected a text header through the returned key");
    };
    assert_eq!(actual.as_ref(), &key);
    assert_eq!(*bytes, text.len());
    let path = ComponentFieldPath::from_fields([actual.as_ref()]);
    let request = EditorPropertyPageRead {
        position: EditorPropertyPagePosition::Start {
            target: page.target().clone(),
            path,
        },
        limits: EditorPropertyReadLimits::new(page.metadata_bytes, 1, 50_000)
            .unwrap()
            .with_scan_limits(1, key.len())
            .unwrap(),
    };
    let mut too_few = request.clone();
    too_few.limits = request.limits.with_scan_limits(1, key.len() - 1).unwrap();
    assert_eq!(
        workspace.read_document_property_page(too_few),
        Err(scan_exhausted())
    );
    let mut result = String::new();
    let mut chunk = workspace
        .read_document_property_page(request.clone())
        .unwrap();
    loop {
        let EditorPropertyPageValue::Present(EditorPropertyPageData::Text {
            total_bytes,
            start,
            text: part,
        }) = &chunk.value
        else {
            panic!("expected text through the admitted long key");
        };
        assert_eq!((*total_bytes, *start), (text.len(), result.len()));
        assert!(!part.is_empty());
        assert_eq!(
            (chunk.scanned_entries, chunk.scanned_logical_bytes),
            (1, key.len())
        );
        result.push_str(part);
        if chunk.next.is_none() {
            break;
        }
        chunk = workspace
            .read_document_property_page(resume(&chunk, request.limits))
            .unwrap();
    }
    assert_eq!(result, text);
}

#[test]
fn first_page_indexes_the_key_and_continuation_reuses_the_admitted_list_position() {
    use nara_reflect::ComponentPropertyKey;

    const COUNT: u64 = 32_768;
    let key = ComponentFieldId::new("durable-key");
    let list = ComponentValueShape::keyed_list(
        ComponentValueShape::record_members([
            ComponentShapeMember::new("a_id", ComponentValueKind::U64).with_id(key.clone()),
            ComponentShapeMember::new("b_value", ComponentValueKind::U64)
                .with_id(ComponentFieldId::new("value")),
        ]),
        key.clone(),
    );
    let items = (0..COUNT)
        .map(|n| {
            ComponentValue::map([
                ("a_id", ComponentValue::U64(n)),
                ("b_value", ComponentValue::U64(n + 1)),
            ])
        })
        .collect();
    let (workspace, document) = fixture_with_values(
        true,
        0,
        String::new(),
        Vec::new(),
        Some((
            ComponentValueShape::optional(list),
            ComponentValue::List(items),
        )),
    );
    let root = read(&workspace, document, "paged", 8).target;
    let target = EditorPropertyTarget {
        property: root.property.with_steps([ComponentPropertyStep::ListItem {
            by: key,
            key: ComponentPropertyKey::U64(COUNT - 1),
        }]),
        ..root
    };
    let limits = EditorPropertyReadLimits::new(1024, 2, 1024).unwrap();
    let first = workspace
        .read_document_property_page(EditorPropertyPageRead {
            position: EditorPropertyPagePosition::Start {
                target: target.clone(),
                path: ComponentFieldPath::empty(),
            },
            limits: limits.with_scan_limits(5, 1024).unwrap(),
        })
        .unwrap();
    assert_eq!(
        first.value,
        EditorPropertyPageValue::Present(EditorPropertyPageData::Map {
            total_entries: 2,
            entries: vec![(
                "a_id".into(),
                EditorPropertyPageItem::Scalar(ComponentValue::U64(COUNT - 1))
            )],
        })
    );
    assert_eq!(first.scanned_entries, 5);
    let resume_bytes = size_of::<u32>() + "b_value".len();
    let rest = workspace
        .read_document_property_page(resume(
            &first,
            limits.with_scan_limits(2, resume_bytes).unwrap(),
        ))
        .unwrap();
    assert_eq!(rest.target(), &target);
    assert_eq!(
        rest.value,
        EditorPropertyPageValue::Present(EditorPropertyPageData::Map {
            total_entries: 2,
            entries: vec![(
                "b_value".into(),
                EditorPropertyPageItem::Scalar(ComponentValue::U64(COUNT))
            )],
        })
    );
    assert!(rest.next.is_none());
    assert_eq!(
        (rest.scanned_entries, rest.scanned_logical_bytes),
        (2, resume_bytes)
    );
    assert_eq!(
        workspace.read_document_property_page(resume(
            &first,
            limits.with_scan_limits(2, resume_bytes - 1).unwrap()
        )),
        Err(scan_exhausted())
    );
}

#[test]
fn the_next_map_key_cannot_reject_an_already_admitted_row() {
    let path_key = "x".repeat(100);
    let payload = ComponentValue::map([(
        path_key.as_str(),
        ComponentValue::map([
            ("a", ComponentValue::U64(1)),
            ("bbbbbbb", ComponentValue::U64(2)),
            ("cccccccccc", ComponentValue::U64(3)),
        ]),
    )]);
    let (workspace, document) = fixture_with_values(
        false,
        0,
        String::new(),
        Vec::new(),
        Some((
            ComponentValueShape::map(ComponentValueShape::map(ComponentValueKind::U64.into())),
            payload,
        )),
    );
    let limits = EditorPropertyReadLimits::new(1024, 2, 64)
        .unwrap()
        .with_scan_limits(16, path_key.len() + 16)
        .unwrap();
    let mut request = EditorPropertyPageRead {
        position: EditorPropertyPagePosition::Start {
            target: read(&workspace, document, "paged", 8).target,
            path: ComponentFieldPath::from_fields([path_key.as_str()]),
        },
        limits,
    };
    for (key, value) in [("a", 1), ("bbbbbbb", 2), ("cccccccccc", 3)] {
        let page = workspace
            .read_document_property_page(request.clone())
            .unwrap();
        let EditorPropertyPageValue::Present(EditorPropertyPageData::Map {
            total_entries,
            entries,
        }) = &page.value
        else {
            panic!("expected a nested Map page");
        };
        assert_eq!(*total_entries, 3);
        assert_eq!(
            entries.as_slice(),
            &[(
                key.into(),
                EditorPropertyPageItem::Scalar(ComponentValue::U64(value))
            )]
        );
        assert_eq!(page.scanned_logical_bytes, path_key.len() + key.len());
        assert_eq!(page.scanned_entries, 2);
        if value == 3 {
            assert!(page.next.is_none());
            return;
        }
        request = resume(&page, limits);
    }
    unreachable!("the last page settled");
}
