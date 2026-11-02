//! ReadContent finish uses the captured native key and sidecar content, not its old leaf ID.

use std::fs;

use nara_asset::{AssetMeta, AssetMetaIdentity, AssetProductRef, ImportedProductId, StableAssetId};
use nara_scene::{
    PrefabExpansionOptions, SceneEntityId, SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
};

use super::*;
use crate::project_host::authoring::{
    document::AuthoringDocumentAdmissionError,
    tests::independent_capture::{bound_source_host, independent_project, open_second_scene},
};

const OUTER_ID: &str = "ae3483d7-bca6-4dbc-84f2-5c38809879dc";
const OUTER_METADATA: &str = "prefabs/outer.prefab.json.meta";

#[test]
fn prefab_finish_accepts_an_equal_content_sidecar_replaced_after_actual_load() {
    finish_after_sidecar_replacement(MetadataReplacement::EqualContent);
}

#[test]
fn prefab_finish_rejects_changed_sidecar_content_after_actual_load() {
    finish_after_sidecar_replacement(MetadataReplacement::ChangedContent);
}

#[test]
fn prefab_finish_rejects_a_changed_stable_key_after_actual_load() {
    finish_after_sidecar_replacement(MetadataReplacement::ChangedStableId);
}

#[derive(Clone, Copy)]
enum MetadataReplacement {
    EqualContent,
    ChangedContent,
    ChangedStableId,
}

fn finish_after_sidecar_replacement(replacement: MetadataReplacement) {
    let project = independent_project();
    let manifest = project.root.join("nara.toml");
    let manifest_bytes = fs::read_to_string(&manifest).unwrap();
    assert!(manifest_bytes.contains("'project_location'"));
    fs::write(
        manifest,
        manifest_bytes.replace("'project_location'", "'stable_metadata'"),
    )
    .unwrap();
    // The primary Product lookup scans the real catalogue. Replace its unused malformed
    // canary with valid metadata rather than bypassing global identity admission.
    for (path, id, kind) in [
        (
            "scenes/startup.scene.json",
            "0facf3d4-d937-45df-a423-74aa8efe51bd",
            AssetSourceKind::Scene,
        ),
        (
            "scenes/second.scene.json",
            "4cd34edc-4423-4ca4-a36d-9121b4f5e6fb",
            AssetSourceKind::Scene,
        ),
        (
            "prefabs/outer.prefab.json",
            OUTER_ID,
            AssetSourceKind::Prefab,
        ),
        (
            "prefabs/leaf.prefab.json",
            "9bdc322d-693f-4628-a349-729bf8a6bd22",
            AssetSourceKind::Prefab,
        ),
        (
            "assets/unused.png",
            "be025cc9-3dcc-4e64-bf04-d785ea160c20",
            AssetSourceKind::Image,
        ),
    ] {
        let logical = path.split_once('/').unwrap().1;
        let bytes = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str(id).unwrap(),
                AssetPath::new(logical).unwrap(),
                kind,
            ),
            (),
        )
        .to_json_string()
        .unwrap();
        fs::write(project.root.join(format!("{path}.meta")), bytes).unwrap();
    }
    let outer_id = StableAssetId::parse_str(OUTER_ID).unwrap();
    let reference =
        AssetRef::Product(AssetProductRef::new(outer_id, ImportedProductId::PRIMARY).unwrap());
    let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
    anchor.prefab = Some(PrefabInstance {
        arguments: None,
        source: reference.clone(),
        overrides: None,
    });
    fs::write(
        project.root.join("scenes/second.scene.json"),
        SceneDocument::from_ordered_roots([anchor])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();

    let (snapshot, source, session) = open_second_scene(&project);
    let basis = session.authoring_schema_basis().unwrap();
    let mut request = session
        .request_complete_prefab_conversion(
            session.revision(),
            &SceneEntityId::new("anchor").unwrap(),
            PrefabExpansionOptions::default(),
        )
        .unwrap();
    let mut inputs = snapshot
        .admit_source_capture_inputs(0, source.checked_retained_bytes().unwrap())
        .unwrap();
    let host = bound_source_host(&snapshot, &source).unwrap();
    let expected = host.accepted_source_observation().unwrap();
    let observation = inputs
        .capture_source_observation(host.source_observation_cost().unwrap(), || {
            host.capture_source_observation(expected)
                .map_err(|_| AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)
        })
        .unwrap();
    let mut context = snapshot
        .capture_prefab_conversion_request(
            &source,
            observation,
            &basis,
            SourceDocumentKind::Scene,
            inputs,
            Box::new([]),
        )
        .unwrap();
    context.retain_root_input(request.checked_retained_bytes().unwrap());
    let mut acquisition = context.acquire().unwrap();
    request.restrict_working_bytes(acquisition.initial_compiler_limit());
    let expected_key = SourceDocumentKey::stable_asset(outer_id, SourceDocumentKind::Prefab);
    let mut first_load = true;
    let prepared = request
        .prepare_with_loader(None, |requested, working| {
            let loaded = acquisition.load(requested, working)?;
            if first_load {
                first_load = false;
                assert_eq!(requested, &reference);
                assert_eq!(loaded.as_ref().unwrap().key(), &expected_key);
            }
            Ok::<_, AuthoringProjectSourceAdmissionError>(loaded)
        })
        .unwrap();
    assert!(!first_load);
    assert_eq!(acquisition.loaded_prefabs().count(), 2);

    // Replacement happens after production loading and before production finish. Keep the
    // old sidecar live so this proves a different object, independently of native ID reuse.
    let root = snapshot.project_root();
    let relative = RelativePath::new(OUTER_METADATA).unwrap();
    let old_sidecar = root.open_file_exact(&relative).unwrap();
    let mut bytes = fs::read(project.root.join(OUTER_METADATA)).unwrap();
    match replacement {
        MetadataReplacement::EqualContent => {}
        MetadataReplacement::ChangedContent => bytes.push(b' '),
        MetadataReplacement::ChangedStableId => {
            bytes = String::from_utf8(bytes)
                .unwrap()
                .replace(OUTER_ID, "f50500f2-8559-44e6-8ef0-e947f2e3f3bb")
                .into_bytes();
        }
    }
    fs::rename(
        project.root.join(OUTER_METADATA),
        project.root.join("prefabs/previous-outer-metadata"),
    )
    .unwrap();
    fs::write(project.root.join(OUTER_METADATA), bytes).unwrap();
    let current_sidecar = root.open_file_exact(&relative).unwrap();
    assert!(!old_sidecar.same_object(&current_sidecar).unwrap());
    drop((old_sidecar, current_sidecar, root));

    let finished = acquisition.finish_source_capture(prepared.checked_retained_bytes().unwrap());
    match replacement {
        MetadataReplacement::EqualContent => {
            let captured =
                finished.expect("equal sidecar content keeps the admitted source meaning");
            assert_eq!(captured.resolve(&reference).unwrap().key(), &expected_key);
            assert!(!captured.requires_recapture());
            captured.source_validation().validate().unwrap();
            assert!(captured.usage().peak_open_handles() <= 64);
        }
        MetadataReplacement::ChangedContent | MetadataReplacement::ChangedStableId => {
            let error = finished
                .err()
                .expect("changed sidecar evidence must reject finish");
            assert!(
                matches!(
                    &error,
                    AuthoringProjectSourceAdmissionError::NativeSource(
                        AuthoringDocumentAdmissionError::SourceIdentityChanged
                    )
                ) || matches!(
                    &error,
                    AuthoringProjectSourceAdmissionError::SourceChanged { path, .. }
                        if path.as_str() == OUTER_METADATA
                ),
                "{error:?}",
            );
        }
    }
}
