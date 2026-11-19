use nara_asset::{AssetMeta, AssetMetaIdentity, AssetSourceKind, StableAssetId};

use super::*;
use crate::project_host::authoring::AuthoringDocumentKind;

#[test]
fn queued_scene_and_prefab_saves_reject_reidentified_sidecars_without_changing_the_draft() {
    let fixture = Fixture::new();
    let manifest = fixture.root.join("nara.toml");
    let stable_manifest = fs::read_to_string(&manifest)
        .unwrap()
        .replace("project_location", "stable_metadata");
    fs::write(manifest, stable_manifest).unwrap();
    let startup_id = "a48d60e2-8ede-4103-b721-f6a0533fb99f";
    fs::write(
        fixture.root.join("scenes/startup.scene.json.meta"),
        metadata("startup.scene.json", AssetSourceKind::Scene, startup_id),
    )
    .unwrap();
    let documents = [
        (
            "scenes/second.scene.json",
            "second.scene.json",
            AuthoringDocumentKind::Scene,
            AssetSourceKind::Scene,
            "be8c37d3-2cd3-4cdf-9479-004f3cd0866d",
            SceneDocument::default().to_json_string().unwrap(),
        ),
        (
            "prefabs/second.prefab.json",
            "second.prefab.json",
            AuthoringDocumentKind::Prefab,
            AssetSourceKind::Prefab,
            "64a2efa1-70cb-4779-8386-dda7c9541bd2",
            PrefabDocument::default().to_json_string().unwrap(),
        ),
    ];
    for (path, name, _, kind, id, bytes) in &documents {
        fs::write(fixture.root.join(path), bytes).unwrap();
        fs::write(
            fixture.root.join(format!("{path}.meta")),
            metadata(name, kind.clone(), id),
        )
        .unwrap();
    }
    let mut editor = fixture.authoring_editor();
    for (path, name, document_kind, asset_kind, original_id, original) in documents {
        let index = editor.project_source_index_snapshot().unwrap();
        let opened = editor.open_indexed_authoring_document(index, path, document_kind);
        let super::super::super::document_open::EditorDocumentOpenRequestResult::Opened {
            document,
        } = opened
        else {
            panic!("the stable native document must open: {opened:?}");
        };
        edit(&mut editor, document, "queued-draft");
        let observe_draft = |editor: &EditorProjectSession| {
            if let Some(slot) = editor.workspace.scene(document) {
                (
                    slot.session().document().to_json_string().unwrap(),
                    slot.revision(),
                    slot.saved_revision(),
                    slot.is_dirty(),
                    slot.session().history_status(),
                )
            } else {
                let slot = editor.workspace.prefab(document).unwrap();
                (
                    slot.session().document().to_json_string().unwrap(),
                    slot.revision(),
                    slot.saved_revision(),
                    slot.is_dirty(),
                    slot.session().history_status(),
                )
            }
        };
        let draft = observe_draft(&editor);
        assert!(draft.3);
        assert_eq!(draft.4.undo_depth, 1);
        let sidecar = fixture.root.join(format!("{path}.meta"));
        let replacement = metadata(name, asset_kind, "c535c4c0-8d2b-4a78-af2f-6112d26fb914");
        let original_metadata = fs::read(&sidecar).unwrap();
        assert!(String::from_utf8_lossy(&original_metadata).contains(original_id));

        let gate = editor.hold_next_save(SaveTestPhase::BeforePublication);
        let request_save = |editor: &mut EditorProjectSession| {
            assert!(matches!(
                editor.request_persistence(EditorPersistenceCommand::Save {
                    document: Some(document),
                }),
                EditorPersistenceRequestResult::Accepted { .. }
            ));
        };
        request_save(&mut editor);
        start_persistence_worker(&mut editor);
        assert!(gate.wait_until_started());
        fs::write(&sidecar, &replacement).unwrap();
        gate.release();
        assert_eq!(
            settle(&mut editor),
            EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: Some(document),
                reason: EditorPersistenceRejection::SourceBindingMismatch,
            }
        );
        assert_eq!(
            fs::read(fixture.root.join(path)).unwrap(),
            original.as_bytes()
        );
        assert_eq!(fs::read_to_string(&sidecar).unwrap(), replacement);
        assert_eq!(observe_draft(&editor), draft);
        acknowledge(&mut editor);

        fs::write(&sidecar, &original_metadata).unwrap();
        request_save(&mut editor);
        assert!(matches!(
            settle(&mut editor),
            EditorPersistenceResult::Saved { document: saved, revision, .. }
                if saved == document && revision == draft.1
        ));
        assert_eq!(fs::read(&sidecar).unwrap(), original_metadata);
        assert!(
            fs::read_to_string(fixture.root.join(path))
                .unwrap()
                .contains("\"queued-draft\"")
        );
        assert_eq!(editor.workspace.document_is_dirty(document), Some(false));
        acknowledge(&mut editor);
    }
    close(&mut editor);
}

fn metadata(name: &str, kind: AssetSourceKind, stable_id: &str) -> String {
    AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(stable_id).unwrap(),
            AssetPath::new(name).unwrap(),
            kind,
        ),
        (),
    )
    .to_json_string()
    .unwrap()
}
