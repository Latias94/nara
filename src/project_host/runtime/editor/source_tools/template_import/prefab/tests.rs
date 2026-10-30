//! Actual native capture, directory publication and retained physical outcomes.

use std::{fs, time::Duration};

use nara_asset::{
    AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId,
};
use nara_fs::{ContentDigest, DirectoryPublicationOutcome, TrustMode};
use nara_reflect::{
    ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue,
};
use nara_scene::{
    PrefabComponentIntent, PrefabDocument, PrefabDocumentCandidate, PrefabEntityOrigin,
    PrefabEntityTarget, PrefabInstance, PrefabInstanceHop, PrefabOverlay, PrefabPropertyClaim,
    PrefabPropertyIntent, SceneComponentRecord, SceneEntityId, SceneEntityRecord,
    SourceDocumentKey, SourceDocumentKind,
};
use nara_tooling::EditorSourceToolTemplateImportOutcome as Outcome;
use serde_json::json;

use super::*;
use crate::project_host::{
    EditorProjectCloseProgress,
    runtime::editor::{
        source_admission::SourceAdmissionError,
        source_tools::{
            EditorSourceToolInstallation, PreparedEditorSourceToolRelease,
            release::tests::Release,
            template_import::tests::{hold_import, register_release, root},
            tests::fixture::{Project, wait},
        },
    },
};

const DESCRIPTOR: &str = "samples/template.json";
const UNIVERSE: &str = "cc4db9bc-388c-44be-91c5-c74fa2c08147";

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.FiniteSourcePayload")
}

fn stable(index: usize) -> StableAssetId {
    [
        "0a39dd5b-1f2e-4a36-bc44-e1751da995af",
        "991c8701-077b-4c70-aaf7-c2b1b51e6c93",
        "61ed48b8-9749-4eab-8a88-f0961edae2b1",
    ][index]
        .parse()
        .unwrap()
}

fn key(index: usize, stable_source: bool) -> SourceDocumentKey {
    if stable_source {
        SourceDocumentKey::stable_asset(stable(index), SourceDocumentKind::Prefab)
    } else {
        SourceDocumentKey::project_location(
            UNIVERSE.parse().unwrap(),
            nara_asset::AssetSourcePath::new(format!(
                "prefabs/{}.prefab.json",
                ["main", "child", "leaf"][index]
            ))
            .unwrap(),
            SourceDocumentKind::Prefab,
        )
    }
}

fn reference(index: usize, stable_source: bool) -> AssetRef {
    if stable_source {
        AssetRef::StableId(stable(index))
    } else {
        AssetRef::path(format!("{}.prefab.json", ["main", "child", "leaf"][index])).unwrap()
    }
}

fn release(stable_source: bool) -> PreparedEditorSourceToolRelease {
    let mut release = Release::new();
    let mut outer = SceneEntityRecord::new(id("outer"));
    let mut overlay = PrefabOverlay::new(key(1, stable_source));
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("nested")),
                expected_base: key(2, stable_source),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("leaf")),
        },
        component: component(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("amount")),
            intent: PrefabPropertyIntent::Set(ComponentValue::I64(9)),
            bindings: Vec::new(),
        }],
    });
    outer.prefab = Some(PrefabInstance {
        source: reference(1, stable_source),
        overrides: Some(overlay),
    });
    let mut child = SceneEntityRecord::new(id("nested"));
    child.prefab = Some(PrefabInstance {
        source: reference(2, stable_source),
        overrides: None,
    });
    let mut leaf = SceneEntityRecord::new(id("leaf"));
    leaf.components.insert(
        component(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("amount", ComponentValue::I64(1))]),
        ),
    );
    for (index, (name, record)) in ["main", "child", "leaf"]
        .into_iter()
        .zip([outer, child, leaf])
        .enumerate()
    {
        let bytes = PrefabDocument::from_ordered_roots([record])
            .to_json_string()
            .unwrap();
        release.add_content(&format!("samples/{name}.prefab.json"), bytes.as_bytes());
        if stable_source {
            let meta = AssetMeta::new(
                AssetMetaIdentity::new(
                    stable(index),
                    AssetPath::new(format!("{name}.prefab.json")).unwrap(),
                    AssetSourceKind::Prefab,
                ),
                (),
            );
            release.add_content(
                &format!("samples/{name}.prefab.json.meta"),
                meta.to_json_string().unwrap().as_bytes(),
            );
        }
    }
    release.add_content(DESCRIPTOR, &serde_json::to_vec(&json!({
        "format_version": 1, "kind": "prefab_template", "source_universe": UNIVERSE, "prefabs_root": "prefabs",
        "native_document_identity": if stable_source { "stable_metadata" } else { "project_location" },
        "entry": "prefabs/main.prefab.json", "documents": [
            {"member":"samples/main.prefab.json","path":"prefabs/main.prefab.json"},
            {"member":"samples/child.prefab.json","path":"prefabs/child.prefab.json"},
            {"member":"samples/leaf.prefab.json","path":"prefabs/leaf.prefab.json"},
        ],
    })).unwrap());
    PreparedEditorSourceToolRelease::capture(
        &release.root(TrustMode::Untrusted),
        release.write_manifest(),
    )
    .unwrap()
}

fn stable_project(project: &Project) {
    let path = project.root.join("nara.toml");
    let manifest = fs::read_to_string(&path).unwrap().replace(
        "native_document_identity = 'project_location'",
        "native_document_identity = 'stable_metadata'",
    );
    fs::write(path, manifest).unwrap();
    let meta = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::try_random().unwrap(),
            AssetPath::new("startup.scene.json").unwrap(),
            AssetSourceKind::Scene,
        ),
        (),
    );
    fs::write(
        project.root.join("scenes/startup.scene.json.meta"),
        meta.to_json_string().unwrap(),
    )
    .unwrap();
}

fn quiet(editor: &mut crate::project_host::EditorProjectSession) {
    wait(editor, |editor| {
        !editor.source_tool_view().has_pending_work()
            && editor.authoring_project().is_some_and(|project| {
                !project.source_index_has_pending_work()
                    && project.source_index_snapshot().is_some()
            })
    });
}

fn installed(
    project: &Project,
    editor: &mut crate::project_host::EditorProjectSession,
    stable_source: bool,
) -> EditorSourceToolInstallation {
    quiet(editor);
    register_release(editor, project, release(stable_source), "native-template")
}

fn prepared(
    project: &Project,
    editor: &mut crate::project_host::EditorProjectSession,
    owner: &EditorSourceToolInstallation,
    destination: &str,
) -> PreparedEditorSourceToolPrefabTemplateImport {
    quiet(editor);
    editor
        .prepare_source_tool_prefab_template_import(
            &editor.source_tool_selection_view().basis,
            root(project),
            EditorSourceToolPrefabTemplateImportRequest::new(
                *owner.receipt().installation_id(),
                DESCRIPTOR,
                destination,
            ),
        )
        .unwrap()
}

fn settle(
    editor: &mut crate::project_host::EditorProjectSession,
) -> nara_tooling::EditorSourceToolTemplateImportView {
    wait(editor, |editor| {
        !editor
            .source_tool_template_import_view()
            .physical_work_pending
    });
    editor.source_tool_template_import_view()
}

#[test]
fn actual_location_and_stable_imports_remap_nested_sources_and_pins_without_prepare_writes() {
    for stable_destination in [false, true] {
        let project = Project::new();
        if stable_destination {
            stable_project(&project);
        }
        let mut editor = project.open();
        let owner = installed(&project, &mut editor, !stable_destination);
        let active = editor.current_frame().workspace().active_document;
        let copy = prepared(&project, &mut editor, &owner, "prefabs/copied");
        assert!(!project.root.join(copy.destination()).exists());
        assert_eq!(editor.current_frame().workspace().active_document, active);
        assert_eq!(copy.entry(), "prefabs/copied/main.prefab.json");
        let file = |name| {
            copy.files()
                .iter()
                .find(|member| member.path() == name)
                .unwrap()
        };
        let main = PrefabDocumentCandidate::decode_json_bytes(file("main.prefab.json").bytes())
            .unwrap()
            .into_composition_source()
            .unwrap();
        let declaration = main.entities[0].prefab.as_ref().unwrap();
        let overlay = declaration.overrides.as_ref().unwrap();
        assert_eq!(&overlay.base, file("child.prefab.json").source().unwrap());
        let PrefabComponentIntent::Properties { target, .. } = &overlay.components[0] else {
            panic!("property declaration retained");
        };
        assert_eq!(
            &target.through[0].expected_base,
            file("leaf.prefab.json").source().unwrap()
        );
        assert_eq!(target.entity.entity(), &id("leaf"));
        assert_eq!(main.entities[0].id, id("outer"));
        assert_eq!(copy.files().len(), if stable_destination { 6 } else { 3 });
        let expected: Vec<_> = copy
            .files()
            .iter()
            .map(|file| (file.path().to_owned(), file.digest()))
            .collect();
        let attempt = editor
            .commit_source_tool_prefab_template_import(copy)
            .unwrap();
        let view = settle(&mut editor);
        assert_eq!(view.attempt, Some(attempt));
        assert!(
            matches!(view.outcome, Some(Outcome::PrefabImported { .. })),
            "{view:?}"
        );
        let publication = editor.source_tool_prefab_template_publication().unwrap();
        assert!(matches!(
            publication.publication(),
            DirectoryPublicationOutcome::Published(_)
        ));
        assert_eq!(publication.retained_files(), expected.len());
        for (path, digest) in expected {
            assert_eq!(
                ContentDigest::of_bytes(
                    &fs::read(project.root.join("prefabs/copied").join(path)).unwrap()
                ),
                digest
            );
        }
        assert_eq!(editor.current_frame().workspace().active_document, active);
    }
}

#[test]
fn native_destination_competition_and_replaced_parent_preserve_reviewed_namespace() {
    for replaced_parent in [false, true] {
        let project = Project::new();
        fs::create_dir(project.root.join("prefabs/container")).unwrap();
        let mut editor = project.open();
        let owner = installed(&project, &mut editor, false);
        let copy = prepared(&project, &mut editor, &owner, "prefabs/container/copied");
        if replaced_parent {
            fs::rename(
                project.root.join("prefabs/container"),
                project.root.join("prefabs/retained-container"),
            )
            .unwrap();
            fs::create_dir(project.root.join("prefabs/container")).unwrap();
        } else {
            fs::create_dir(project.root.join("prefabs/container/copied")).unwrap();
            fs::write(
                project.root.join("prefabs/container/copied/unknown.txt"),
                b"competitor",
            )
            .unwrap();
        }
        editor
            .commit_source_tool_prefab_template_import(copy)
            .unwrap();
        let view = settle(&mut editor);
        assert_eq!(
            view.outcome,
            Some(Outcome::Failed(if replaced_parent {
                Error::Changed
            } else {
                Error::TargetExists
            }))
        );
        assert!(editor.source_tool_prefab_template_publication().is_none());
        assert!(
            !project
                .root
                .join("prefabs/container/copied/main.prefab.json")
                .exists()
        );
        if !replaced_parent {
            assert_eq!(
                fs::read(project.root.join("prefabs/container/copied/unknown.txt")).unwrap(),
                b"competitor"
            );
        }
    }
}

#[test]
fn cancellation_after_native_publication_waits_for_physical_return_and_close_keeps_actual_result() {
    let project = Project::new();
    let mut editor = project.open();
    let owner = installed(&project, &mut editor, false);
    let copy = prepared(&project, &mut editor, &owner, "prefabs/held");
    let (entrance, control) = hold_import(&mut editor);
    let attempt = editor
        .commit_source_tool_prefab_template_import(copy)
        .unwrap();
    entrance.recv_timeout(Duration::from_secs(15)).unwrap();
    assert!(project.root.join("prefabs/held/main.prefab.json").is_file());
    assert!(
        editor
            .source_tool_template_import_view()
            .physical_work_pending
    );
    assert!(editor.source_tool_template_import_view().outcome.is_none());
    assert_eq!(
        editor.exclude_project_source_aliases("prefabs/held/main.prefab.json"),
        Err(SourceAdmissionError::HostUnavailable)
    );
    editor.cancel_source_tool_template_import(&attempt).unwrap();
    assert!(matches!(
        editor.advance_close(),
        EditorProjectCloseProgress::Incomplete { .. }
    ));
    drop(control);
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if let EditorProjectCloseProgress::Complete(receipt) = editor.advance_close() {
            let view = receipt.template_import().unwrap();
            assert_eq!(view.attempt.as_ref(), Some(&attempt));
            assert!(matches!(view.outcome, Some(Outcome::PrefabImported { .. })));
            assert!(!view.physical_work_pending);
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn partial_native_staging_retains_actual_directory_members_and_close_observation() {
    let project = Project::new();
    let mut editor = project.open();
    let owner = installed(&project, &mut editor, false);
    let mut copy = prepared(&project, &mut editor, &owner, "prefabs/interrupted");
    copy.files[0].path = "nested/created.prefab.json".into();
    copy.test_stop_after_created = Some(2);
    editor
        .commit_source_tool_prefab_template_import(copy)
        .unwrap();
    let view = settle(&mut editor);
    let Some(Outcome::PrefabNotPublished {
        staging: Some(staging),
        created,
        error: Error::Write,
        ..
    }) = &view.outcome
    else {
        panic!("actual incomplete staging must be retained: {view:?}");
    };
    assert_eq!(created.len(), 2);
    let publication = editor.source_tool_prefab_template_publication().unwrap();
    assert!(matches!(
        publication.publication(),
        DirectoryPublicationOutcome::Orphaned(_)
    ));
    assert_eq!(publication.retained_files(), 2);
    assert_eq!(publication.retained_directories(), 1);
    assert_eq!(
        publication.created_directories(),
        [Arc::<str>::from("nested")]
    );
    assert!(!project.root.join("prefabs/interrupted").exists());
    assert!(project.root.join(staging.as_ref()).is_dir());
    assert!(created.iter().any(|member| {
        fs::read(
            project
                .root
                .join(staging.as_ref())
                .join(member.path.as_ref()),
        )
        .unwrap()
        .is_empty()
    }));
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if let EditorProjectCloseProgress::Complete(receipt) = editor.advance_close() {
            assert_eq!(receipt.template_import(), Some(&view));
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(project.root.join(staging.as_ref()).is_dir());
}

#[test]
fn actual_post_publication_drift_is_retained_until_explicit_owner_transfer() {
    let project = Project::new();
    let mut editor = project.open();
    let owner = installed(&project, &mut editor, false);
    let mut copy = prepared(&project, &mut editor, &owner, "prefabs/drifted");
    let path = project.root.join("prefabs/drifted/main.prefab.json");
    copy.test_after_publication = Some(Box::new(move |_| {
        fs::write(&path, b"actual external change after directory publication").unwrap();
    }));
    let attempt = editor
        .commit_source_tool_prefab_template_import(copy)
        .unwrap();
    let observed = settle(&mut editor);
    assert!(
        matches!(
            &observed.outcome,
            Some(Outcome::PrefabPublishedWithoutAdmission {
                error: Error::Changed,
                ..
            })
        ),
        "{observed:?}"
    );
    quiet(&mut editor);
    assert!(matches!(
        editor.prepare_source_tool_prefab_template_import(
            &editor.source_tool_selection_view().basis,
            root(&project),
            EditorSourceToolPrefabTemplateImportRequest::new(
                *owner.receipt().installation_id(),
                DESCRIPTOR,
                "prefabs/next"
            ),
        ),
        Err(Error::RequiresReconcile)
    ));
    assert_eq!(editor.source_tool_template_import_view(), observed);
    let publication = editor
        .take_source_tool_prefab_template_publication(&attempt)
        .unwrap();
    assert!(matches!(
        publication.publication(),
        DirectoryPublicationOutcome::Published(_)
    ));
    assert_eq!(publication.error(), Some(&Error::Changed));
    let next = prepared(&project, &mut editor, &owner, "prefabs/next");
    drop(next);
    assert!(!project.root.join("prefabs/next").exists());
    drop(publication);
    assert_eq!(
        fs::read(project.root.join("prefabs/drifted/main.prefab.json")).unwrap(),
        b"actual external change after directory publication"
    );
}

#[test]
fn final_tree_rejects_equal_byte_replacement_of_a_freshly_admitted_member() {
    let project = Project::new();
    let mut editor = project.open();
    let owner = installed(&project, &mut editor, false);
    let mut copy = prepared(&project, &mut editor, &owner, "prefabs/replaced");
    let current = project.root.join("prefabs/replaced/main.prefab.json");
    let held = project.root.join("original-held.prefab.json");
    copy.test_after_member_admission = Some(Box::new(move |_| {
        let bytes = fs::read(&current).unwrap();
        fs::rename(&current, &held).unwrap();
        fs::write(&current, bytes).unwrap();
    }));
    editor
        .commit_source_tool_prefab_template_import(copy)
        .unwrap();
    let observed = settle(&mut editor);
    assert!(
        matches!(
            observed.outcome,
            Some(Outcome::PrefabPublishedWithoutAdmission {
                error: Error::Changed,
                ..
            })
        ),
        "{observed:?}"
    );
    assert_eq!(
        fs::read(project.root.join("prefabs/replaced/main.prefab.json")).unwrap(),
        fs::read(project.root.join("original-held.prefab.json")).unwrap()
    );
    assert_eq!(
        editor
            .source_tool_prefab_template_publication()
            .unwrap()
            .retained_files(),
        3
    );
}

#[test]
fn indexed_native_sidecars_reject_duplicate_ids_and_post_review_collisions() {
    for duplicate in [false, true] {
        let project = Project::new();
        stable_project(&project);
        for name in ["existing", "other"] {
            fs::write(
                project.root.join(format!("prefabs/{name}.prefab.json")),
                PrefabDocument::default().to_json_string().unwrap(),
            )
            .unwrap();
            let metadata = AssetMeta::new(
                AssetMetaIdentity::new(
                    if duplicate {
                        stable(0)
                    } else {
                        StableAssetId::try_random().unwrap()
                    },
                    AssetPath::new(format!("{name}.prefab.json")).unwrap(),
                    AssetSourceKind::Prefab,
                ),
                (),
            );
            fs::write(
                project
                    .root
                    .join(format!("prefabs/{name}.prefab.json.meta")),
                metadata.to_json_string().unwrap(),
            )
            .unwrap();
        }
        let mut editor = project.open();
        let owner = installed(&project, &mut editor, false);
        let request = || {
            EditorSourceToolPrefabTemplateImportRequest::new(
                *owner.receipt().installation_id(),
                DESCRIPTOR,
                "prefabs/collision",
            )
        };
        let result = editor.prepare_source_tool_prefab_template_import(
            &editor.source_tool_selection_view().basis,
            root(&project),
            request(),
        );
        if duplicate {
            assert!(matches!(result, Err(Error::IdentityConflict)));
            continue;
        }
        let copy = result.unwrap();
        let new_id = copy.identities[0];
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                new_id,
                AssetPath::new("existing.prefab.json").unwrap(),
                AssetSourceKind::Prefab,
            ),
            (),
        );
        fs::write(
            project.root.join("prefabs/existing.prefab.json.meta"),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
        editor
            .commit_source_tool_prefab_template_import(copy)
            .unwrap();
        let observed = settle(&mut editor);
        assert_eq!(observed.outcome, Some(Outcome::Failed(Error::Changed)));
        assert!(!project.root.join("prefabs/collision").exists());
    }
}

#[test]
fn location_copy_does_not_require_unrelated_asset_identity_interpretation() {
    for stable_destination in [false, true] {
        let project = Project::new();
        if stable_destination {
            stable_project(&project);
        }
        fs::write(
            project.root.join("assets/unrelated.png"),
            b"uninterpreted source",
        )
        .unwrap();
        fs::write(
            project.root.join("assets/unrelated.png.meta"),
            b"uninterpretable metadata",
        )
        .unwrap();
        let mut editor = project.open();
        let owner = installed(&project, &mut editor, false);
        let result = editor.prepare_source_tool_prefab_template_import(
            &editor.source_tool_selection_view().basis,
            root(&project),
            EditorSourceToolPrefabTemplateImportRequest::new(
                *owner.receipt().installation_id(),
                DESCRIPTOR,
                "prefabs/independent",
            ),
        );
        if stable_destination {
            assert!(matches!(result, Err(Error::IdentityConflict)));
        } else {
            let copy = result.unwrap();
            assert!(copy.identities.is_empty());
            editor
                .commit_source_tool_prefab_template_import(copy)
                .unwrap();
            assert!(matches!(
                settle(&mut editor).outcome,
                Some(Outcome::PrefabImported { .. })
            ));
        }
        assert_eq!(
            fs::read(project.root.join("assets/unrelated.png.meta")).unwrap(),
            b"uninterpretable metadata"
        );
    }
}
