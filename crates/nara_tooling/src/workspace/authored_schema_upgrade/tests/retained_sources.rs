use super::*;

use nara_scene::{
    __private::admit_authored_upgrade_prefab_source, PrefabAuthoringSession, SceneFileLimits,
};

fn prepare_retained(
    workspace: &EditorWorkspace,
    upgrade: &AuthoredFieldUpgrade,
    proposals: Vec<EditorAuthoredSchemaUpgradeProposal>,
    retained: RetainedWorkspaceSources,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<PreparedEditorWorkspaceAuthoredSchemaUpgrade, EditorWorkspaceCommandReport> {
    work.with_preparation_attempt(|work| {
        workspace
            .capture_retained_authored_upgrade_input(retained, work)?
            .prepare(upgrade, proposals, work)
    })
}

#[test]
fn retained_open_sources_reuse_the_ordinary_kernel_without_charging_originals_twice() {
    let fixture = Fixture::new();
    let (workspace, _, _) = fixture.open();
    let before = snapshot(&workspace);
    let mut ordinary_work = AuthoredSchemaUpgradeWork::default();
    let ordinary = fixture.prepare(&workspace, &mut ordinary_work);
    let required = ordinary_work.usage();
    for (maximum_work, maximum_retained, succeeds) in [
        (required.work_bytes, required.peak_retained_bytes, true),
        (required.work_bytes - 1, required.peak_retained_bytes, false),
        (required.work_bytes, required.peak_retained_bytes - 1, false),
    ] {
        let mut work = work_with_limits(maximum_work, maximum_retained);
        let prepared = workspace
            .retain_authored_upgrade_sources(&fixture.previous, &mut work)
            .and_then(|retained| {
                assert_eq!(
                    retained
                        .sources
                        .iter()
                        .map(|source| source.document)
                        .collect::<Vec<_>>(),
                    workspace.documents.keys().copied().collect::<Vec<_>>()
                );
                let mut proposed = proposals(&workspace);
                proposed.reverse();
                prepare_retained(&workspace, &fixture.upgrade, proposed, retained, &mut work)
            });
        assert_eq!(prepared.is_ok(), succeeds);
        if let Ok(prepared) = prepared {
            assert_eq!(work.usage(), required);
            for document in ordinary.documents() {
                assert_eq!(
                    prepared.source_binding(document),
                    ordinary.source_binding(document)
                );
                if let Some(original) = ordinary.scene(document) {
                    let source = prepared.scene(document).unwrap();
                    assert_eq!(source.document(), original.document());
                    assert_eq!(source.previous_basis(), fixture.previous);
                    assert_eq!(source.schema_basis(), fixture.successor);
                } else {
                    let original = ordinary.prefab(document).unwrap();
                    let source = prepared.prefab(document).unwrap();
                    assert_eq!(source.document(), original.document());
                    assert_eq!(source.previous_basis(), fixture.previous);
                    assert_eq!(source.schema_basis(), fixture.successor);
                }
            }
        }
        assert_eq!(snapshot(&workspace), before);
    }
}

#[test]
fn retained_source_token_rejects_another_workspace_or_equal_independent_basis() {
    let fixture = Fixture::new();
    let (workspace, _, _) = fixture.open();
    let (other, _, _) = fixture.open();
    let foreign = Fixture::new();
    let before = snapshot(&workspace);
    let other_before = snapshot(&other);
    for wrong_workspace in [false, true] {
        let mut work = AuthoredSchemaUpgradeWork::default();
        let retained = workspace
            .retain_authored_upgrade_sources(&fixture.previous, &mut work)
            .unwrap();
        let (target, upgrade) = if wrong_workspace {
            (&other, &fixture.upgrade)
        } else {
            (&workspace, &foreign.upgrade)
        };
        assert_refused(
            &prepare_retained(target, upgrade, proposals(target), retained, &mut work).unwrap_err(),
        );
    }
    assert_refused(
        &workspace
            .retain_authored_upgrade_sources(
                &foreign.previous,
                &mut AuthoredSchemaUpgradeWork::default(),
            )
            .unwrap_err(),
    );
    assert_eq!(snapshot(&workspace), before);
    assert_eq!(snapshot(&other), other_before);
}

#[test]
fn source_owner_rejects_retention_from_a_different_cumulative_work_allowance() {
    let fixture = Fixture::new();
    let (workspace, _, _) = fixture.open();
    let before = snapshot(&workspace);
    let mut original_work = AuthoredSchemaUpgradeWork::default();
    let retained = workspace
        .retain_authored_upgrade_sources(&fixture.previous, &mut original_work)
        .unwrap();
    let original_usage = original_work.usage();
    let mut foreign_work = AuthoredSchemaUpgradeWork::default();
    let report = prepare_retained(
        &workspace,
        &fixture.upgrade,
        proposals(&workspace),
        retained,
        &mut foreign_work,
    )
    .unwrap_err();
    assert_refused(&report);
    assert!(
        report
            .patch_report
            .as_ref()
            .unwrap()
            .diagnostics
            .iter()
            .any(|diagnostic| {
                diagnostic.code().as_str() == "scene.authored-schema-upgrade-stale"
            })
    );
    assert_eq!(original_work.usage(), original_usage);
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn retained_complete_set_refuses_current_proposals_after_any_incumbent_changes() {
    for change in [
        "scene-edit",
        "prefab-edit",
        "undo-redo",
        "open",
        "close",
        "replace",
        "scene-binding",
        "prefab-binding",
        "reopen",
    ] {
        let fixture = Fixture::new();
        let (mut workspace, mut authority, documents) = fixture.open();
        let mut work = AuthoredSchemaUpgradeWork::default();
        let retained = workspace
            .retain_authored_upgrade_sources(&fixture.previous, &mut work)
            .unwrap();
        match change {
            "scene-edit" => set_armor(&mut workspace, documents[1], 10, 1),
            "prefab-edit" => set_armor(&mut workspace, documents[2], 10, 1),
            "undo-redo" => {
                replay(&mut workspace, documents[1], true);
                replay(&mut workspace, documents[1], false);
            }
            "open" => {
                open(&mut workspace, EditorDocumentKind::Scene, &fixture.previous);
            }
            "close" => assert!(workspace.__discard_document(documents[2]).applied),
            "replace" => {
                assert!(workspace.__discard_document(documents[2]).applied);
                open(
                    &mut workspace,
                    EditorDocumentKind::Prefab,
                    &fixture.previous,
                );
            }
            "scene-binding" => {
                if let EditorDocumentSlot::Scene(slot) =
                    workspace.documents.get_mut(&documents[1]).unwrap()
                {
                    slot.source_binding =
                        Some(EditorDocumentSourceBinding::new("scenes/moved.json"));
                }
            }
            "prefab-binding" => {
                if let EditorDocumentSlot::Prefab(slot) =
                    workspace.documents.get_mut(&documents[2]).unwrap()
                {
                    slot.source_binding = EditorDocumentSourceBinding::new("prefabs/moved.json");
                }
            }
            "reopen" => {
                let slot = workspace.scene(documents[1]).unwrap();
                let session = slot
                    .session()
                    .try_reopen_authoring_candidate(
                        SceneDocumentCandidate::decode_json_str(
                            &slot.session().document().to_json_string().unwrap(),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                let revision = slot.revision();
                authority
                    .publish_reopened_session(
                        &mut workspace,
                        documents[1],
                        revision,
                        session,
                        EditorDocumentDigest::new(19, [0x31; 32]),
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        // Fresh proposals intentionally describe the current set, not the retained originals.
        let proposed = proposals(&workspace);
        let before = snapshot(&workspace);
        assert_refused(
            &prepare_retained(&workspace, &fixture.upgrade, proposed, retained, &mut work)
                .unwrap_err(),
        );
        assert_eq!(snapshot(&workspace), before, "{change}");
    }
}

#[test]
fn source_retention_admits_collection_before_allocating_and_keeps_failed_work_cumulative() {
    let fixture = Fixture::new();
    let (workspace, _, _) = fixture.open();
    let before = snapshot(&workspace);
    for (maximum_work, maximum_retained) in [(1, 256 * 1024 * 1024), (512 * 1024 * 1024, 1)] {
        let mut work = work_with_limits(maximum_work, maximum_retained);
        let report = workspace
            .retain_authored_upgrade_sources(&fixture.previous, &mut work)
            .unwrap_err();
        assert_refused(&report);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.fields().iter().any(|field| {
                field.key().as_str() == "preparation-stage"
                    && field.display_value() == "workspace-source-retention-validation"
            })
        }));
        assert_eq!(work.usage().work_bytes, 0);
    }
    let mut measured = AuthoredSchemaUpgradeWork::default();
    drop(
        workspace
            .retain_authored_upgrade_sources(&fixture.previous, &mut measured)
            .unwrap(),
    );
    let required = measured.usage();
    let mut work = work_with_limits(required.work_bytes - 1, required.peak_retained_bytes);
    assert!(
        workspace
            .retain_authored_upgrade_sources(&fixture.previous, &mut work)
            .is_err()
    );
    let refused_usage = work.usage();
    assert!(refused_usage.work_bytes > 0);
    assert!(
        workspace
            .retain_authored_upgrade_sources(&fixture.previous, &mut work)
            .is_err()
    );
    assert!(work.usage().work_bytes >= refused_usage.work_bytes);
    assert_eq!(snapshot(&workspace), before);
}

fn padded_source(bytes: usize) -> SceneDocument {
    let mut source = document();
    source
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("opaque"))
        .unwrap()
        .components
        .get_mut(&ComponentTypeId::new("missing.Opaque"))
        .unwrap()
        .value = ComponentValue::String("x".repeat(bytes));
    source
}

fn closed_prefab(
    fixture: &Fixture,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<
    (PrefabAuthoringSession, RetainedAuthoredSchemaUpgradeSource),
    nara_scene::ScenePatchReport,
> {
    let source = padded_source(1024 * 1024);
    let source = PrefabDocument::new(source.placements, source.entities);
    let candidate = PrefabDocumentCandidate::decode_json_bytes_with_limits(
        source.to_json_string().unwrap().as_bytes(),
        SceneFileLimits::default(),
    )
    .unwrap();
    admit_authored_upgrade_prefab_source(
        candidate,
        AuthoringDocumentSource::new(AssetRef::path("prefabs/closed.prefab.json").unwrap()),
        &fixture.previous,
        work,
    )
}

#[test]
fn large_open_incumbent_is_charged_during_actual_closed_prefab_classification() {
    let fixture = Fixture::new();
    let mut workspace = EditorWorkspace::new();
    let path = "scenes/large.scene.json";
    let source = SceneDocumentCandidate::decode_json_str(
        &padded_source(256 * 1024).to_json_string().unwrap(),
    )
    .unwrap()
    .admit_authoring_with_basis(
        AuthoringDocumentSource::new(AssetRef::path(path).unwrap()),
        &fixture.previous,
    )
    .unwrap()
    .publish();
    workspace
        .open_authoring_scene_document(
            "Large scene",
            EditorDocumentSourceBinding::new(path),
            source,
        )
        .unwrap();
    let before = snapshot(&workspace);
    let mut measured = AuthoredSchemaUpgradeWork::default();
    let retained = workspace
        .retain_authored_upgrade_sources(&fixture.previous, &mut measured)
        .unwrap();
    let open_peak = measured.usage().peak_retained_bytes;
    let closed = closed_prefab(&fixture, &mut measured).unwrap();
    let required = measured.usage();
    assert!(required.peak_retained_bytes > open_peak);
    drop((retained, closed));
    for (maximum_retained, succeeds) in [
        (required.peak_retained_bytes, true),
        (required.peak_retained_bytes - 1, false),
    ] {
        // The same closed classification alone fits: only the live open incumbent makes the
        // one-short combined allowance refuse. Its bytes must be charged before the factory.
        let mut alone = work_with_limits(required.work_bytes, maximum_retained);
        drop(closed_prefab(&fixture, &mut alone).unwrap());
        let mut work = work_with_limits(required.work_bytes, maximum_retained);
        let retained = workspace
            .retain_authored_upgrade_sources(&fixture.previous, &mut work)
            .unwrap();
        let result = closed_prefab(&fixture, &mut work);
        assert_eq!(result.is_ok(), succeeds);
        if succeeds {
            assert_eq!(work.usage(), required);
        } else {
            assert!(result.unwrap_err().diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.authored-schema-upgrade-limit-exceeded"
            }));
        }
        drop(retained);
        assert_eq!(snapshot(&workspace), before);
    }
}

#[test]
fn repeated_confirmation_releases_only_scan_scratch_on_success_and_refusal() {
    const SCANS: usize = 6;
    let fixture = Fixture::new();
    let (mut workspace, _, documents) = fixture.open();
    let mut measured = AuthoredSchemaUpgradeWork::default();
    let prepared = fixture.prepare(&workspace, &mut measured);
    let preparation_work = measured.usage().work_bytes;
    prepared.validate(&workspace, &mut measured).unwrap();
    let scan_work = measured.usage().work_bytes - preparation_work;
    let peak = measured.usage().peak_retained_bytes;
    assert!(scan_work > 0);
    drop(prepared);
    let mut work = work_with_limits(preparation_work + SCANS * scan_work, peak);
    let prepared = fixture.prepare(&workspace, &mut work);
    let before = snapshot(&workspace);
    for scan in 0..SCANS {
        if scan == 2 {
            let original_binding = if let EditorDocumentSlot::Scene(slot) =
                workspace.documents.get_mut(&documents[1]).unwrap()
            {
                slot.source_binding
                    .replace(EditorDocumentSourceBinding::new(
                        "scenes/competing.scene.json",
                    ))
            } else {
                unreachable!()
            };
            assert_refused(&prepared.validate(&workspace, &mut work).unwrap_err());
            if let EditorDocumentSlot::Scene(slot) =
                workspace.documents.get_mut(&documents[1]).unwrap()
            {
                slot.source_binding = original_binding;
            }
        } else {
            prepared.validate(&workspace, &mut work).unwrap();
        }
        assert_eq!(
            work.usage().work_bytes,
            preparation_work + (scan + 1) * scan_work
        );
        assert_eq!(work.usage().peak_retained_bytes, peak);
        assert_eq!(snapshot(&workspace), before);
    }
    let report = prepared.validate(&workspace, &mut work).unwrap_err();
    assert_refused(&report);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.authored-schema-upgrade-limit-exceeded"
    }));
    assert_eq!(
        work.usage().work_bytes,
        preparation_work + SCANS * scan_work
    );
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn empty_complete_open_set_can_surround_unopened_preparation() {
    let fixture = Fixture::new();
    let workspace = EditorWorkspace::new();
    let mut work = AuthoredSchemaUpgradeWork::default();
    let retained = workspace
        .retain_authored_upgrade_sources(&fixture.previous, &mut work)
        .unwrap();
    assert!(retained.sources.is_empty());
    let closed = closed_prefab(&fixture, &mut work).unwrap();
    let prepared = prepare_retained(
        &workspace,
        &fixture.upgrade,
        Vec::new(),
        retained,
        &mut work,
    )
    .unwrap();
    assert_eq!(prepared.documents().len(), 0);
    assert_eq!(prepared.schema_basis(), &fixture.successor);
    prepared.validate(&workspace, &mut work).unwrap();
    assert!(
        closed
            .0
            .document()
            .entities
            .iter()
            .any(|entity| entity.id == id("opaque"))
    );
}
