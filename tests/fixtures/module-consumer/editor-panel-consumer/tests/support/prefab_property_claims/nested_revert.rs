use super::*;

use nara::scene::{PrefabInstanceClaim, PrefabInstancePathSegment};

const LEAF_SOURCE: &str = "prefabs/nested-claim-leaf.prefab.json";

fn source_key(path: &str) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "24ed5e36-1b37-5fa3-9c42-232066b17916".parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        SourceDocumentKind::Prefab,
    )
}

fn nested_instances() -> PrefabInstance {
    let mut leaf = PrefabInstance::forest(AssetRef::path("nested-claim-leaf.prefab.json").unwrap());
    let mut leaf_overlay = PrefabOverlay::new(source_key(LEAF_SOURCE));
    leaf_overlay.components.push(properties(
        "leaf",
        COMPONENT,
        ComponentSchemaVersion::ONE,
        [("score", score(77))],
    ));
    leaf.overrides = Some(leaf_overlay);
    let mut outer = PrefabOverlay::new(source_key(BASE));
    outer.instances = ["guard", "leaf"]
        .map(|name| PrefabInstanceClaim {
            target: PrefabEntityTarget::source(id(name)),
            value: Some(leaf.clone()),
        })
        .into();
    let mut root = PrefabInstance::forest(AssetRef::path("claims.prefab.json").unwrap());
    root.overrides = Some(outer);
    root
}

fn nested_overlay(root: &PrefabInstance, index: usize) -> &PrefabOverlay {
    root.overrides.as_ref().unwrap().instances[index]
        .value
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap()
}

fn page_query(basis: &EditorDocumentQueryBasis, path: &PrefabInstancePath) -> EditorAuthoringQuery {
    EditorAuthoringQuery {
        position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
            basis: basis.clone(),
            entity: path.anchor.clone(),
            projection: EditorAuthoringProjection::PrefabPropertyClaims {
                steps: path.steps.clone(),
            },
        }),
        limits: query_limits(),
    }
}

fn leaf_score(page: &EditorAuthoringPage) -> EditorPrefabPropertyClaimTarget {
    assert_eq!(page.rows.len(), 1);
    assert!(page.next.is_none());
    find_claim(
        &page.rows,
        "leaf",
        COMPONENT,
        ComponentSchemaVersion::ONE,
        "score",
        true,
    )
}

#[test]
fn nested_preview_revert_cannot_remove_an_identical_claim_from_the_live_scene() {
    round_trip_nested_revert(EditorDocumentKind::Scene, SCENE);
}

#[test]
fn nested_preview_revert_cannot_remove_an_identical_claim_from_the_live_prefab() {
    round_trip_nested_revert(EditorDocumentKind::Prefab, PREFAB);
}

fn round_trip_nested_revert(kind: EditorDocumentKind, source: &str) {
    let project = text_project::TextProject::new();
    let original = nested_instances();
    let document_source = |instance: &PrefabInstance| {
        let mut anchor = SceneEntityRecord::new(id("mount"));
        anchor.prefab = Some(instance.clone());
        match kind {
            EditorDocumentKind::Scene => {
                SceneDocument::from_ordered_roots([anchor]).to_json_string()
            }
            EditorDocumentKind::Prefab => {
                PrefabDocument::from_ordered_roots([anchor]).to_json_string()
            }
            EditorDocumentKind::Extension(_) => unreachable!("Scene and Prefab claim fixtures"),
        }
        .unwrap()
    };
    let original_bytes = document_source(&original);
    let source_file = project.root.join(source);
    let outer_bytes = PrefabDocument::from_ordered_roots(
        ["guard", "leaf"].map(|name| SceneEntityRecord::new(id(name))),
    )
    .to_json_string()
    .unwrap();
    let leaf_bytes = base().to_json_string().unwrap();
    fs::write(project.root.join(BASE), &outer_bytes).unwrap();
    fs::write(project.root.join(LEAF_SOURCE), &leaf_bytes).unwrap();
    fs::write(&source_file, &original_bytes).unwrap();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        schemas().to_json_string().unwrap(),
    )
    .unwrap();

    let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
        &project.root,
    )));
    app.native_shell_ready();
    wait(&mut app, |current| {
        !current.project_browser().has_pending_work()
    });
    let document = open_source(&mut app, source);
    let before = frame(&app);
    let initial = before.workspace().document(document).unwrap();
    assert_eq!(initial.kind, kind);
    assert!(!initial.dirty);
    assert_eq!(
        (initial.history.undo_depth, initial.history.redo_depth),
        (0, 0)
    );

    let captured = basis(&app, document);
    let root = PrefabInstancePath::new(id("mount"));
    let nested = |index| PrefabInstancePath {
        anchor: root.anchor.clone(),
        steps: vec![PrefabInstancePathSegment::InstanceClaim { index }],
    };
    let guard = nested(0);
    let leaf = nested(1);
    let read_live = |app: &EditorApplication, path| {
        let claim = leaf_score(
            &app.query_document_authoring(page_query(&captured, path))
                .unwrap(),
        );
        assert_eq!(
            app.read_prefab_property_claim(&claim, read_limits()),
            Ok(score(77))
        );
        claim
    };
    let live_guard = read_live(&app, &guard);
    let live_leaf = read_live(&app, &leaf);
    assert_eq!(live_guard.slot(), live_leaf.slot());
    assert_ne!(live_guard, live_leaf);

    let preview_edit = app
        .prepare_prefab_revert(
            &captured,
            &root,
            PrefabRevert::Intent(PrefabOverlaySlot::InstanceClaim { index: 0 }),
            Default::default(),
        )
        .unwrap();
    let preview = preview_edit.preview();
    let preview_claim = leaf_score(
        &preview
            .query_authoring(page_query(&captured, &guard))
            .unwrap(),
    );
    assert_eq!(preview_claim.path(), live_guard.path());
    assert_eq!(preview_claim.slot(), live_guard.slot());
    assert_eq!(
        preview.read_prefab_property_claim(&preview_claim, read_limits()),
        Ok(score(77))
    );
    assert_eq!(
        app.prepare_prefab_property_revert(&preview_claim, Default::default())
            .unwrap_err(),
        EditorEditPreparationError::QueryBasis(EditorDocumentQueryRejection::WrongPreparedEdit)
    );

    let [
        ScenePatchOperation::SetPrefabInstance {
            value: Some(preview_root),
            ..
        },
    ] = preview_edit.patch().operations.as_slice()
    else {
        panic!("removing one nested claim must prepare one exact instance replacement");
    };
    let [remaining] = preview_root
        .overrides
        .as_ref()
        .unwrap()
        .instances
        .as_slice()
    else {
        panic!("the preview must contain exactly the second instance");
    };
    assert_eq!(remaining.target, PrefabEntityTarget::source(id("leaf")));
    let preview_declaration = nested_overlay(preview_root, 0)
        .property_claim_at(preview_claim.slot())
        .unwrap();
    assert_eq!(
        preview_declaration,
        nested_overlay(&original, 0)
            .property_claim_at(live_guard.slot())
            .unwrap(),
        "the observed payload alone cannot distinguish the two parent instances"
    );
    let observed = app
        .observe_prefab_instance(&captured, &root, Default::default())
        .unwrap();
    assert_eq!(observed.nested_instances(), &[guard.clone(), leaf.clone()]);
    let live_removal = observed.revert(2, Some(live_leaf.slot())).unwrap();
    let live_component = observed
        .revert(2, Some(PrefabOverlaySlot::Component { index: 0 }))
        .unwrap();
    for action in [
        PrefabRevert::Property(preview_declaration),
        live_removal,
        live_component,
    ] {
        let error = app
            .prepare_prefab_revert(
                preview.basis(),
                preview_claim.path(),
                action,
                Default::default(),
            )
            .unwrap_err();
        let EditorEditPreparationError::Document(report) = error else {
            panic!("the source owner must refuse the rebound observation: {error:?}");
        };
        assert!(!report.applied);
        assert_eq!(report.revision, Some(captured.revision));
        assert!(report.patch_report.as_ref().unwrap().inverse.is_none());
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-prefab-stale-observed-intent"
        }));
    }
    assert_eq!(frame(&app).workspace(), before.workspace());
    assert_eq!(fs::read_to_string(&source_file).unwrap(), original_bytes);

    let expected = {
        let mut expected = original.clone();
        expected.overrides.as_mut().unwrap().instances[1]
            .value
            .as_mut()
            .unwrap()
            .overrides
            .as_mut()
            .unwrap()
            .components
            .clear();
        expected
    };
    let removal = app
        .prepare_prefab_property_revert(&live_leaf, Default::default())
        .unwrap();
    let observed_removal = app
        .prepare_prefab_revert(&captured, &leaf, live_removal, Default::default())
        .unwrap();
    assert_eq!(observed_removal.patch(), removal.patch());
    assert_eq!(observed_removal.inverse(), removal.inverse());
    let replacement = |old: &PrefabInstance, new: &PrefabInstance| {
        ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
            entity: root.anchor.clone(),
            expected: Some(old.clone()),
            value: Some(new.clone()),
        }])
    };
    assert_eq!(removal.patch(), &replacement(&original, &expected));
    assert_eq!(removal.inverse(), &replacement(&expected, &original));
    let committed = app.commit_authoring_edit(observed_removal, Duration::ZERO);
    assert!(matches!(committed.actions(), [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(outcome))
    ] if outcome.applied && outcome.document == Some(document)));

    let assert_source = |app: &EditorApplication, instance: &PrefabInstance, status| {
        let current = frame(app);
        let tab = current.workspace().document(document).unwrap();
        assert_eq!(
            (tab.dirty, tab.history.undo_depth, tab.history.redo_depth),
            status
        );
        let observed = app
            .observe_prefab_instance(&basis(app, document), &root, Default::default())
            .unwrap();
        assert_eq!(observed.instance(), Some(instance));
    };
    assert_source(&app, &expected, (true, 1, 0));
    let base_revision = basis(&app, document).revision;
    let result = submit(
        &mut app,
        EditorAction::Mutate(EditorDocumentMutation::Undo {
            document,
            base_revision,
        }),
    );
    assert!(matches!(result.actions(), [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(outcome))
    ] if outcome.applied));
    assert_source(&app, &original, (false, 0, 1));
    let base_revision = basis(&app, document).revision;
    let result = submit(
        &mut app,
        EditorAction::Mutate(EditorDocumentMutation::Redo {
            document,
            base_revision,
        }),
    );
    assert!(matches!(result.actions(), [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(outcome))
    ] if outcome.applied));
    assert_source(&app, &expected, (true, 1, 0));
    assert_eq!(fs::read_to_string(&source_file).unwrap(), original_bytes);

    let base_revision = basis(&app, document).revision;
    let (_, saved) = persist(
        &mut app,
        EditorPersistenceAction::Save {
            document,
            base_revision,
        },
    );
    assert!(
        matches!(saved, EditorPersistenceResult::Saved { document: actual, revision, .. }
        if actual == document && revision == base_revision)
    );
    assert_source(&app, &expected, (false, 1, 0));
    assert_eq!(
        fs::read_to_string(&source_file).unwrap(),
        document_source(&expected)
    );
    assert_eq!(
        fs::read_to_string(project.root.join(BASE)).unwrap(),
        outer_bytes
    );
    assert_eq!(
        fs::read_to_string(project.root.join(LEAF_SOURCE)).unwrap(),
        leaf_bytes
    );
    assert!(app.close());
}
