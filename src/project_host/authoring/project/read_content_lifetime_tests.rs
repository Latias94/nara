//! Read-only dependency count does not become a permanently open native-file count.

use std::fs;

use nara_core::ItemLimit;
use nara_scene::{PrefabInstance, SceneEntityId, SceneEntityRecord};

use super::*;
use crate::project_host::authoring::tests::independent_capture::{
    CaptureBoundScene, independent_project, open_second_scene,
};

#[test]
fn eighty_fresh_dependencies_and_repeated_validation_fit_the_original_handle_ceiling() {
    let project = independent_project();
    let mut entities = Vec::new();
    for index in 0..80 {
        let name = format!("read-{index}");
        fs::write(
            project.root.join(format!("prefabs/{name}.prefab.json")),
            PrefabDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        let mut entity =
            SceneEntityRecord::new(SceneEntityId::new(format!("anchor-{index}")).unwrap());
        entity.prefab = Some(PrefabInstance {
            source: AssetRef::path(format!("{name}.prefab.json")).unwrap(),
            overrides: None,
        });
        entities.push(entity);
    }
    fs::write(
        project.root.join("scenes/second.scene.json"),
        SceneDocument::from_ordered_roots(entities)
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let (mut snapshot, source, scene) = open_second_scene(&project);
    // The test intentionally performs repeated complete validation. Increase only cumulative
    // work, leaving the production handle ceiling unchanged at 64.
    let schema =
        Arc::get_mut(&mut snapshot.schema).expect("only this test owns its Schema receipt");
    schema.budget.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
    let maximum = schema.budget.limits.peak_open_handles().get();
    assert_eq!(maximum, 64);
    let capture = snapshot
        .capture_scene_from_bound_source(&source, &scene, &[])
        .unwrap();
    assert!(capture.prefabs().len() >= 80);
    assert!(capture.usage().peak_open_handles() <= maximum);
    assert_eq!(
        capture.usage().retained_open_handles(),
        FileSourceObservation::retained_handle_capacity()
    );
    let validation = capture.source_validation();
    drop(capture);
    drop(snapshot);
    for _ in 0..2 {
        validation.clone().validate().unwrap();
    }
    fs::write(
        project.root.join("prefabs/read-79.prefab.json"),
        b"changed content",
    )
    .unwrap();
    assert!(validation.validate().is_err());
}
