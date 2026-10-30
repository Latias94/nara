use std::fs;

use nara_asset::{AssetMeta, AssetMetaIdentity, StableAssetId};
use nara_core::ItemLimit;
use nara_reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue};
use nara_scene::{
    __private::{AuthoringPreviewOmission, AuthoringPreviewOmissionLocation},
    SceneComponentRecord, SceneEntityId,
};

use super::*;
use crate::project_host::authoring::tests::independent_capture::{
    independent_project, open_second_scene,
};

mod composition_failure;
mod host_retention;
mod prefab_acquisition;
mod scene_dependencies;
mod source_admission;
mod stable_lookup;

const SECOND_SCENE: &str = "scenes/second.scene.json";

fn unavailable_scene(entities: usize, payload_bytes: usize) -> SceneDocument {
    SceneDocument::from_ordered_roots((0..entities).map(|index| {
        SceneEntityRecord::new(SceneEntityId::new(format!("unavailable-{index}")).unwrap())
            .with_component(
                ComponentTypeId::new("test.UnavailableCapturePayload"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("x".repeat(payload_bytes)),
                ),
            )
    }))
}

#[test]
fn shared_root_input_must_fit_the_retained_budget_even_when_its_preview_is_small() {
    let project = independent_project();
    let document = unavailable_scene(1, 512 * 1024);
    let encoded = document.to_json_string().unwrap();
    fs::write(project.root.join(SECOND_SCENE), &encoded).unwrap();
    let (snapshot, source, session) = open_second_scene(&project);
    let revision = session.revision();
    let held_document = session.shared_document();

    let mut unrestricted = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert_eq!(
        unrestricted.readiness(),
        AuthoringProjectReadiness::Degraded
    );
    assert_eq!(unrestricted.root_revision(), Some(revision));
    let preview = unrestricted.take_preview().unwrap();
    assert_eq!(preview.omissions.len(), 1);
    assert_eq!(preview.candidate.document().entities.len(), 1);
    assert!(
        preview.candidate.document().entities[0]
            .components
            .is_empty()
    );

    let mut request = snapshot
        .capture_scene_request(
            &source,
            &session,
            snapshot.admit_source_capture_inputs(0, 0).unwrap(),
            Box::new([]),
        )
        .unwrap();
    let root_retained = request.document.retained_bytes().unwrap();
    let preview_retained =
        crate::scene_retention::direct_startup_scene_retained_bytes(&preview.candidate).unwrap();
    assert!(preview_retained < root_retained / 4);
    let maximum = request.schema.budget.retained_candidate_bytes + root_retained / 2;
    drop(unrestricted);
    drop(snapshot);
    Arc::get_mut(&mut request.schema)
        .expect("the owned request must be the only remaining Schema receipt owner")
        .budget
        .limits
        .retained_candidate_bytes = ByteLimit::new(maximum).unwrap();

    let error = request
        .prepare()
        .err()
        .expect("a small derived preview must not hide its large held source document");
    assert!(
        matches!(
            &error,
            AuthoringProjectSourceAdmissionError::Budget(
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                    maximum: observed
                }
            ) if *observed == maximum
        ),
        "{error:?}"
    );
    assert_eq!(session.revision(), revision);
    assert!(Arc::ptr_eq(&held_document, &session.shared_document()));
    assert_eq!(
        fs::read(project.root.join(SECOND_SCENE)).unwrap(),
        encoded.as_bytes()
    );
}

#[test]
fn stable_root_metadata_counts_toward_encoded_bytes_and_unique_file_limits() {
    for encoded_limit in [false, true] {
        let project = independent_project();
        fs::write(
            project.root.join(SECOND_SCENE),
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                SceneEntityId::new("local").unwrap(),
            )])
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        let manifest_path = project.root.join("nara.toml");
        let manifest = fs::read_to_string(&manifest_path).unwrap();
        fs::write(
            manifest_path,
            manifest.replace("'project_location'", "'stable_metadata'"),
        )
        .unwrap();
        for (logical_path, stable_id) in [
            ("startup.scene.json", "560c808a-32a8-43cf-928d-61558545317c"),
            ("second.scene.json", "15677b7c-fd8c-48e3-960f-1338ef9c22d1"),
        ] {
            let mut metadata = AssetMeta::new(
                AssetMetaIdentity::new(
                    StableAssetId::parse_str(stable_id).unwrap(),
                    AssetPath::new(logical_path).unwrap(),
                    AssetSourceKind::Scene,
                ),
                (),
            )
            .to_json_string()
            .unwrap();
            if logical_path == "second.scene.json" {
                metadata.push_str(&" ".repeat(8192));
            }
            fs::write(
                project
                    .root
                    .join("scenes")
                    .join(format!("{logical_path}.meta")),
                metadata,
            )
            .unwrap();
        }

        let (snapshot, source, session) = open_second_scene(&project);
        let baseline = snapshot.schema.usage();
        let metadata_bytes = fs::read(project.root.join(format!("{SECOND_SCENE}.meta")))
            .unwrap()
            .len();
        let captured = snapshot
            .capture_scene_document(&source, &session, &[])
            .unwrap();
        assert_eq!(captured.readiness(), AuthoringProjectReadiness::Complete);
        assert!(captured.prefabs().is_empty());
        assert!(captured.assets().is_empty());
        assert_eq!(captured.source_facts().len(), 2);
        assert_eq!(captured.usage().unique_files(), baseline.unique_files() + 2);
        assert_eq!(
            captured.usage().aggregate_encoded_bytes(),
            baseline.aggregate_encoded_bytes() + source.encoded_bytes() + metadata_bytes,
        );

        let mut request = snapshot
            .capture_scene_request(
                &source,
                &session,
                snapshot.admit_source_capture_inputs(0, 0).unwrap(),
                Box::new([]),
            )
            .unwrap();
        drop(captured);
        drop(snapshot);
        let budget = &mut Arc::get_mut(&mut request.schema)
            .expect("the owned request must be the only remaining Schema receipt owner")
            .budget;
        let encoded_maximum =
            baseline.aggregate_encoded_bytes() + source.encoded_bytes() + metadata_bytes - 1;
        let file_maximum = baseline.unique_files() + 1;
        if encoded_limit {
            budget.limits.aggregate_encoded_bytes = ByteLimit::new(encoded_maximum).unwrap();
        } else {
            budget.limits.unique_files = ItemLimit::new(file_maximum).unwrap();
        }
        let error = request
            .prepare()
            .err()
            .expect("the root metadata must cross the same aggregate limits as other sources");
        if encoded_limit {
            assert!(
                matches!(
                    &error,
                    AuthoringProjectSourceAdmissionError::Budget(
                        AuthoringProjectAdmissionError::AggregateEncodedBytesExceeded {
                            maximum
                        }
                    ) if *maximum == encoded_maximum
                ),
                "{error:?}"
            );
        } else {
            assert!(
                matches!(
                    &error,
                    AuthoringProjectSourceAdmissionError::Budget(
                        AuthoringProjectAdmissionError::UniqueFileLimitExceeded {
                            maximum
                        }
                    ) if *maximum == file_maximum
                ),
                "{error:?}"
            );
        }
    }
}

#[test]
fn preview_omissions_retain_vector_capacity_and_both_owned_strings_per_record() {
    let project = independent_project();
    let count = 64;
    fs::write(
        project.root.join(SECOND_SCENE),
        unavailable_scene(count, 4).to_json_string().unwrap(),
    )
    .unwrap();
    let (snapshot, source, session) = open_second_scene(&project);
    let mut capture = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    let preview = capture.take_preview().unwrap();
    let mut omissions = preview.omissions;
    assert_eq!(omissions.len(), count);
    assert!(
        preview
            .candidate
            .document()
            .entities
            .iter()
            .all(|entity| entity.components.is_empty())
    );
    omissions.reserve(count);
    assert!(omissions.capacity() > omissions.len());
    let string_bytes = omissions
        .iter()
        .map(|fact| {
            let AuthoringPreviewOmissionLocation::DirectEntityComponent { entity, component } =
                fact.location()
            else {
                panic!("the fixture contains only direct source components");
            };
            entity.as_str().len() + component.as_str().len()
        })
        .sum::<usize>();
    let expected = omissions.capacity() * size_of::<AuthoringPreviewOmission>()
        + string_bytes
        + 2 * count * crate::scene_retention::STRING_ALLOCATION_OVERHEAD;
    assert_eq!(preview_omissions_retained_bytes(&omissions), Some(expected));
}
