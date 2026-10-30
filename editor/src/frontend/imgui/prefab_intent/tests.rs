use std::{fs, time::Duration};

use nara::{
    asset::{AssetRef, AssetSourcePath},
    reflect::{ComponentRegistry, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{
        InMemoryPrefabSourceResolver, PrefabAddition, PrefabComponentSetMode, PrefabDocument,
        PrefabEntityTarget, PrefabExpansionFailureKind, PrefabInstanceClaim, SceneComponentRecord,
        SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePlacement,
        ScenePlacementOwner, SourceDocumentKey, SourceDocumentKind,
    },
    tooling::{EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation},
};

use super::super::{interaction_tests::InteractionProject, submission_tests::Harness};
use super::*;

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn key() -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "c3504a11-271b-59f3-b385-4810c9a5c5a2".parse().unwrap(),
        AssetSourcePath::new("blueprints/ordinary.prefab.json").unwrap(),
        SourceDocumentKind::Prefab,
    )
}

fn instance(overlay: Option<PrefabOverlay>) -> PrefabInstance {
    PrefabInstance {
        source: AssetRef::path("ordinary.prefab.json").unwrap(),
        overrides: overlay,
    }
}

fn project(extra_entities: usize) -> InteractionProject {
    let project = InteractionProject::new();
    let mut overlay = PrefabOverlay::new(key());
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(id("missing")));
    let mut anchor = SceneEntityRecord::new(id("intent-anchor"));
    anchor.prefab = Some(instance(Some(overlay)));
    let mut entities = vec![anchor];
    entities.extend(
        (0..extra_entities).map(|index| SceneEntityRecord::new(id(&format!("direct-{index}")))),
    );
    fs::write(
        project.root.join("worlds/entry.scene.json"),
        SceneDocument::from_ordered_roots(entities)
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    project
}

fn basis(harness: &Harness) -> EditorDocumentQueryBasis {
    let frame = harness.application.current_frame().unwrap();
    let document = frame
        .workspace()
        .document(frame.workspace().active_document.unwrap())
        .unwrap();
    EditorDocumentQueryBasis {
        document: document.document,
        revision: document.revision,
        schema: document.authoring_schema_basis.clone(),
    }
}

fn prepare(harness: &mut Harness) {
    let mut once = false;
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        if once {
            return;
        }
        once = true;
        let document = context.model().document().unwrap().document;
        let basis = context.document_query_basis(document).unwrap();
        let mut state = ImguiPrefabIntent::default();
        state.refresh(context, basis);
        state.prepare(
            context,
            PrefabRevert::Intent(PrefabOverlaySlot::Suppression { index: 0 }),
        );
        assert!(state.review.is_some(), "{:?}", state.notice);
        context.widgets.prefab_intent = state;
    }));
    drop(harness.draw());
}

#[test]
fn review_retains_exact_before_after_and_cancel_publishes_nothing() {
    let mut harness = Harness::with_project(project(0), 1);
    let before = harness.application.current_frame().unwrap();
    prepare(&mut harness);
    let panel = harness.panels[0].0;
    let state = &mut harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap()
        .prefab_intent;
    let review = state.review.as_ref().unwrap();
    assert!(review.expected.contains("suppressions"));
    assert!(review.expected.contains("missing"));
    assert!(!review.value.contains("missing"));
    let [
        ScenePatchOperation::SetPrefabInstance {
            expected, value, ..
        },
    ] = review.prepared.patch().operations.as_slice()
    else {
        panic!("exact replacement required")
    };
    assert_eq!(expected, &state.observation.as_ref().unwrap().instance);
    let after = value.as_ref().unwrap();
    assert!(after.overrides.as_ref().unwrap().suppressions.is_empty());
    assert_eq!(after.source, expected.as_ref().unwrap().source);
    state.cancel_review();
    assert!(state.review.is_none());
    assert!(state.pending.is_none());
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    assert!(harness.panels[0].1.borrow().finished.is_empty());
}

#[test]
fn reviewed_token_is_consumed_once_and_receives_its_own_exact_result() {
    let mut harness = Harness::with_project(project(0), 1);
    let before = harness.application.current_frame().unwrap();
    let query_basis = basis(&harness);
    let depth = before
        .workspace()
        .document(query_basis.document)
        .unwrap()
        .history
        .undo_depth;
    prepare(&mut harness);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(|context| {
        let mut state = std::mem::take(&mut context.widgets.prefab_intent);
        if state.review.is_some() {
            state.commit(context);
            assert!(state.review.is_none());
            assert!(state.pending.is_some());
        }
        context.widgets.prefab_intent = state;
    }));
    let candidate = harness.draw();
    let panel = harness.panels[0].0;
    let pending = harness.frontend.presentation.widgets[&panel]
        .prefab_intent
        .pending
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    harness.submit(candidate);
    let state = &mut harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap()
        .prefab_intent;
    assert!(state.pending.is_none());
    assert!(matches!(
        state.result.as_ref(),
        Some((actual, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))))
            if *actual == pending && report.applied
    ));
    let original = state.result.clone();
    state.action_finished(
        pending,
        PanelActionDispatch::NotDispatched(super::super::PanelActionNotDispatched::Superseded),
    );
    assert_eq!(
        state.result, original,
        "duplicate or unrelated settlement cannot overwrite the exact outcome"
    );
    assert!(
        harness.panels[0].1.borrow().finished.is_empty(),
        "built-in result must not route to contribution callbacks"
    );
    let after = harness.application.current_frame().unwrap();
    assert_eq!(
        after
            .workspace()
            .document(query_basis.document)
            .unwrap()
            .history
            .undo_depth,
        depth + 1
    );
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            after.revision(),
            [EditorAction::Mutate(EditorDocumentMutation::Undo {
                document: query_basis.document,
                base_revision: after
                    .workspace()
                    .document(query_basis.document)
                    .unwrap()
                    .revision,
            })],
        ),
        Duration::ZERO,
    );
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .document(query_basis.document)
            .unwrap()
            .history
            .undo_depth,
        depth,
    );
}

#[test]
fn source_revision_change_does_not_reprepare_the_reviewed_token_at_commit() {
    let mut harness = Harness::with_project(project(0), 1);
    prepare(&mut harness);
    let captured = basis(&harness);
    let frame = harness.application.current_frame().unwrap();
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyScenePatch {
                    document: captured.document,
                    base_revision: captured.revision,
                    patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                        entity: SceneEntityRecord::new(id("later-direct")),
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    }]),
                },
            )],
        ),
        Duration::ZERO,
    );
    let updated = harness.application.current_frame().unwrap();
    assert_ne!(
        updated
            .workspace()
            .document(captured.document)
            .unwrap()
            .revision,
        captured.revision
    );
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(|context| {
        let mut state = std::mem::take(&mut context.widgets.prefab_intent);
        state.commit(context);
        context.widgets.prefab_intent = state;
    }));
    let candidate = harness.draw();
    harness.submit(candidate);
    let panel = harness.panels[0].0;
    assert!(matches!(
        harness.frontend.presentation.widgets[&panel].prefab_intent.result.as_ref(),
        Some((_, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)))) if !report.applied
    ));
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .document(captured.document),
        updated.workspace().document(captured.document),
    );
}

#[test]
fn stale_schema_and_read_only_cancel_review_and_require_explicit_re_review() {
    let mut harness = Harness::with_project(project(0), 1);
    prepare(&mut harness);
    let mut current = basis(&harness);
    let panel = harness.panels[0].0;
    let state = &mut harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap()
        .prefab_intent;
    assert!(state.review.is_some());
    current.schema = None;
    assert!(!state.reconcile_review(&current, true));
    assert!(state.review.is_none());
    assert!(
        state.observation.is_some(),
        "bounded historical source observation remains visible"
    );
    prepare(&mut harness);
    let current = basis(&harness);
    let state = &mut harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap()
        .prefab_intent;
    assert!(state.reconcile_review(&current, false));
    assert!(state.review.is_none());
}

#[test]
fn source_entities_are_paged_without_selection_or_document_mutation() {
    let mut harness = Harness::with_project(project(PAGE_ROWS + 3), 1);
    let before = harness.application.current_frame().unwrap();
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(|context| {
        let document = context.model().document().unwrap().document;
        let basis = context.document_query_basis(document).unwrap();
        let mut state = ImguiPrefabIntent::default();
        state.refresh(context, basis);
        let page = state.page.as_ref().unwrap();
        assert_eq!(page.entities.len(), PAGE_ROWS);
        let next = page.next.clone().unwrap();
        state.load_page(context, EditorEntityQueryPosition::Continue(next));
        let page = state.page.as_ref().unwrap();
        assert_eq!(page.entities.len(), 4);
        assert!(page.next.is_none());
        assert!(state.review.is_none());
    }));
    drop(harness.draw());
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
}

#[test]
fn source_observation_retains_nested_paths_and_every_intent_kind_including_detach() {
    let project = project(0);
    let mut overlay = PrefabOverlay::new(key());
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(id("missing")));
    let mut addition = PrefabAddition::new(id("nested-addition"));
    addition.prefab = Some(instance(None));
    overlay.additions.push(addition);
    overlay.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("detached")),
        value: None,
    });
    overlay.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Replace,
        target: PrefabEntityTarget::source(id("unknown")),
        component: ComponentTypeId::new("missing.Record"),
        record: SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String("opaque value".into()),
        ),
        bindings: Vec::new(),
    });
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("unknown")),
        component: ComponentTypeId::new("missing.Properties"),
        version: ComponentSchemaVersion::ONE,
        claims: vec![nara::scene::PrefabPropertyClaim {
            path: nara::reflect::ComponentPropertyPath::new(nara::reflect::ComponentFieldId::new(
                "value",
            )),
            intent: nara::scene::PrefabPropertyIntent::Absent,
            bindings: Vec::new(),
        }],
    });
    overlay.placements.push(nara::scene::PrefabPlacement {
        target: PrefabEntityTarget::addition(id("nested-addition")),
        parent: nara::scene::PrefabPlacementParent::Root,
        slot: nara::scene::PrefabPlacementSlot::Last,
    });
    let mut anchor = SceneEntityRecord::new(id("intent-anchor"));
    anchor.prefab = Some(instance(Some(overlay)));
    fs::write(
        project.root.join("worlds/entry.scene.json"),
        SceneDocument::from_ordered_roots([anchor])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut harness = Harness::with_project(project, 1);
    let query_basis = basis(&harness);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let mut state = ImguiPrefabIntent::default();
        state.refresh(context, query_basis.clone());
        let observation = state.observation.as_ref().unwrap();
        let overlay = observation
            .instance
            .as_ref()
            .unwrap()
            .overrides
            .as_ref()
            .unwrap();
        let rows = &state.rows;
        assert_eq!(rows.len(), 7);
        assert!(rows.iter().any(|(slot, label)| matches!(
            slot,
            PrefabOverlaySlot::InstanceClaim { .. }
        ) && label.contains("explicit detach")));
        for (slot, _) in rows {
            assert!(!intent_detail(overlay, *slot).unwrap().is_empty());
        }
        assert_eq!(
            observation.nested_instances,
            [
                PrefabInstancePath {
                    anchor: id("intent-anchor"),
                    steps: vec![PrefabInstancePathSegment::Addition { index: 0 }],
                },
                PrefabInstancePath {
                    anchor: id("intent-anchor"),
                    steps: vec![PrefabInstancePathSegment::InstanceClaim { index: 0 }],
                },
            ]
        );
        state.instance_index = 1;
        assert!(state.selected_instance().is_some());
        state.instance_index = 2;
        assert!(
            state.selected_instance().is_none(),
            "explicit detach must not be displayed as an inherited instance"
        );
        assert!(state.selected_path().is_some());
    }));
    drop(harness.draw());
}

#[test]
fn composition_status_distinguishes_foreign_source_and_never_infers_success() {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let mut overlay = PrefabOverlay::new(key());
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(id("missing")));
    let mut bad = SceneEntityRecord::new(id("bad-anchor"));
    bad.prefab = Some(instance(Some(overlay)));
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        AssetRef::path("ordinary.prefab.json").unwrap(),
        key(),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf"))]),
    );
    let report =
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("unrelated")), bad.clone()])
            .expand_prefabs(&registry, &resolver);
    let failure = report.failure.unwrap();
    assert_eq!(failure.reason(), &PrefabExpansionFailureKind::MissingTarget);
    let path = PrefabInstancePath::new(id("bad-anchor"));
    assert_eq!(
        composition_status(
            Some(&failure),
            Some(&path),
            Some(PrefabOverlaySlot::Suppression { index: 0 })
        )
        .0,
        "This stored intent is the first reported composition failure.",
    );
    assert!(
        composition_status(None, Some(&path), None)
            .0
            .starts_with("Resolution not checked")
    );
    let outer_key = SourceDocumentKey::named(
        "c3504a11-271b-59f3-b385-4810c9a5c5a2".parse().unwrap(),
        "outer",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let resolver = resolver.with_prefab(
        AssetRef::path("outer.prefab.json").unwrap(),
        outer_key,
        PrefabDocument::from_ordered_roots([bad]),
    );
    let mut anchor = SceneEntityRecord::new(id("containing-anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("outer.prefab.json").unwrap(),
        overrides: None,
    });
    let failure = SceneDocument::from_ordered_roots([anchor])
        .expand_prefabs(&registry, &resolver)
        .failure
        .unwrap();
    assert!(
        composition_status(
            Some(&failure),
            Some(&path),
            Some(PrefabOverlaySlot::Suppression { index: 0 })
        )
        .1
    );
}

#[test]
fn detail_output_is_bounded_without_building_an_unbounded_debug_string() {
    assert!(bounded_debug(&"x".repeat(DETAIL_BYTES * 2), DETAIL_BYTES).is_none());
    assert!(
        bounded_debug(&"small", DETAIL_BYTES)
            .unwrap()
            .contains("small")
    );
}

#[test]
fn inspector_prefab_tab_paints_without_selection_and_retains_its_bounded_observation() {
    let mut harness = Harness::with_project(project(0), 0);
    let before = harness.application.current_frame().unwrap();
    let inspector = harness
        .application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "nara.inspector")
        .unwrap()
        .id();
    assert!(
        harness.frontend.presentation.widgets[&inspector].inspector_tab
            == super::super::workspace::InspectorTab::Document
    );
    for (left, right) in [
        ("Document", "Source asset"),
        ("Source asset", "Prefab Intent"),
    ] {
        let left =
            super::super::ui_interaction_tests::control_rect(&format!("Inspector tab: {left}"))
                .unwrap();
        let right =
            super::super::ui_interaction_tests::control_rect(&format!("Inspector tab: {right}"))
                .unwrap();
        assert!(
            left.max[0] <= right.min[0],
            "adjacent Inspector tabs must not have overlapping mouse hitboxes"
        );
    }
    harness.click_control("Inspector tab: Prefab Intent");
    assert!(
        harness.frontend.presentation.widgets[&inspector].inspector_tab
            == super::super::workspace::InspectorTab::PrefabIntent
    );
    assert!(
        super::super::ui_interaction_tests::control_rect("Inspector tab: Prefab Intent").is_some()
    );
    assert!(
        super::super::ui_interaction_tests::control_rect("Prefab Intent: Review selected Revert")
            .is_some()
    );
    let state = &harness.frontend.presentation.widgets[&inspector].prefab_intent;
    assert_eq!(
        state.observation.as_ref().unwrap().path.anchor,
        id("intent-anchor")
    );
    let allocation = state
        .observation
        .as_ref()
        .unwrap()
        .instance
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap()
        .suppressions
        .as_ptr();
    drop(harness.draw());
    let state = &harness.frontend.presentation.widgets[&inspector].prefab_intent;
    assert_eq!(
        state
            .observation
            .as_ref()
            .unwrap()
            .instance
            .as_ref()
            .unwrap()
            .overrides
            .as_ref()
            .unwrap()
            .suppressions
            .as_ptr(),
        allocation,
        "a repaint must not clone or reload the complete declaration",
    );
    let after = harness.application.current_frame().unwrap();
    assert_eq!(after.workspace().documents, before.workspace().documents);
    assert_eq!(
        after.workspace().active_document,
        before.workspace().active_document
    );
    assert_eq!(
        after.workspace().active_scene.as_ref().unwrap().selection,
        before.workspace().active_scene.as_ref().unwrap().selection
    );
}

#[test]
fn real_inspector_controls_retain_the_same_review_across_tabs_then_cancel_or_commit() {
    let mut harness = Harness::with_project(project(0), 0);
    let before = harness.application.current_frame().unwrap();
    let basis = basis(&harness);
    let inspector = harness
        .application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "nara.inspector")
        .unwrap()
        .id();
    let navigate_tabs = |harness: &mut Harness| {
        let review = harness.frontend.presentation.widgets[&inspector]
            .prefab_intent
            .review
            .as_ref()
            .unwrap();
        let storage = (
            review.prepared.patch().operations.as_ptr(),
            review.prepared.inverse().operations.as_ptr(),
        );
        for (label, tab) in [
            ("Document", super::super::workspace::InspectorTab::Document),
            (
                "Source asset",
                super::super::workspace::InspectorTab::SourceAsset,
            ),
        ] {
            harness.click_control(&format!("Inspector tab: {label}"));
            assert!(harness.frontend.presentation.widgets[&inspector].inspector_tab == tab);
            assert!(
                harness.frontend.presentation.widgets[&inspector]
                    .prefab_intent
                    .review
                    .is_some(),
                "same-document navigation must not discard a prepared review"
            );
            harness.click_control("Inspector tab: Prefab Intent");
            assert!(
                harness.frontend.presentation.widgets[&inspector].inspector_tab
                    == super::super::workspace::InspectorTab::PrefabIntent
            );
            let retained = harness.frontend.presentation.widgets[&inspector]
                .prefab_intent
                .review
                .as_ref()
                .unwrap();
            assert_eq!(
                (
                    retained.prepared.patch().operations.as_ptr(),
                    retained.prepared.inverse().operations.as_ptr(),
                ),
                storage,
                "navigation must retain the original move-only candidate, not reprepare it"
            );
        }
    };
    let original = before.workspace().document(basis.document).unwrap().clone();
    harness.click_control("Inspector tab: Prefab Intent");
    harness.click_control("Prefab Intent: Review selected Revert");
    let state = &harness.frontend.presentation.widgets[&inspector].prefab_intent;
    let candidate = state.review.as_ref().unwrap();
    assert_eq!(candidate.prepared.base_revision(), basis.revision);
    assert!(candidate.expected.contains("missing"));
    assert!(!candidate.value.contains("missing"));
    navigate_tabs(&mut harness);
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .document(basis.document),
        Some(&original)
    );
    harness.click_control("Prefab Intent: Cancel Revert review");
    assert!(
        harness.frontend.presentation.widgets[&inspector]
            .prefab_intent
            .review
            .is_none()
    );
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .document(basis.document),
        Some(&original)
    );
    harness.click_control("Prefab Intent: Review selected Revert");
    navigate_tabs(&mut harness);
    harness.click_control("Prefab Intent: Commit reviewed Revert");
    let state = &harness.frontend.presentation.widgets[&inspector].prefab_intent;
    assert!(state.review.is_none());
    assert!(state.pending.is_none());
    assert!(matches!(
        state.result.as_ref(),
        Some((_, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)))) if report.applied
    ));
    let frame = harness.application.current_frame().unwrap();
    let after = frame.workspace().document(basis.document).unwrap();
    assert_eq!(after.history.undo_depth, original.history.undo_depth + 1);
    assert!(after.dirty);
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(EditorDocumentMutation::Undo {
                document: basis.document,
                base_revision: after.revision,
            })],
        ),
        Duration::ZERO,
    );
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .document(basis.document)
            .unwrap()
            .history
            .undo_depth,
        original.history.undo_depth,
    );
}
