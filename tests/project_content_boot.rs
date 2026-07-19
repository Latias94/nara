#![cfg(all(feature = "serde", feature = "runtime-2d"))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::sync::Arc;

use nara::project_host::{ProjectContentBudgetKind, ProjectContentLoader};
use nara::scene::{PrefabDocument, SceneEntityRecord};
use project_content_fixture::{TestProject, scene_id};

struct EscapeOwner;

#[test]
fn authorized_startup_closure_publishes_an_immutable_snapshot() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let snapshot = loader.load(&candidate, &plan).unwrap();
    let cloned = snapshot.clone();

    assert_eq!(snapshot.lineage(), candidate.lineage());
    assert_eq!(
        snapshot.schema_fingerprint(),
        plan.schema_validation().fingerprint()
    );
    assert_eq!(snapshot.prefabs().len(), 1);
    assert_eq!(snapshot.images().len(), 1);
    assert_eq!(snapshot.images()[0].path().as_str(), "textures/player.png");
    assert_eq!(snapshot.images()[0].image().extent().width, 1);
    assert_eq!(snapshot.images()[0].image().extent().height, 1);
    assert_eq!(snapshot.images()[0].image().pixels(), &[24, 120, 220, 255]);
    assert!(
        snapshot
            .expanded_startup_scene()
            .entities
            .iter()
            .any(|entity| entity.id == scene_id("enemy-anchor/enemy"))
    );
    assert!(std::ptr::eq(
        snapshot.images()[0].image().pixels(),
        cloned.images()[0].image().pixels(),
    ));
    let mut escaped = snapshot.images()[0].image().share_retained().unwrap();
    assert!(!escaped.try_attach_retention_owner(Arc::new(EscapeOwner)));

    let leased = loader.budget_snapshot();
    assert!(leased.active(ProjectContentBudgetKind::ArtifactBytes) > 0);
    assert!(leased.active(ProjectContentBudgetKind::RetainedBytes) > 0);
    assert_eq!(leased.active_reservations(), 1);

    drop(snapshot);
    assert_eq!(loader.budget_snapshot(), leased);
    drop(cloned);
    assert_eq!(loader.budget_snapshot(), leased);
    drop(escaped);
    let released = loader.budget_snapshot();
    assert_eq!(released.active(ProjectContentBudgetKind::ArtifactBytes), 0);
    assert_eq!(released.active(ProjectContentBudgetKind::RetainedBytes), 0);
    assert_eq!(released.active_reservations(), 0);
}

#[test]
fn source_changes_publish_a_distinct_revision_without_mutating_old_consumers() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let first = loader.load(&candidate, &plan).unwrap();
    let first_revision = first.revision();
    let first_digest = first.content_digest();
    let first_prefab = first.prefabs()[0].document().clone();
    let first_pixels = first.images()[0].image().pixels().to_vec();
    let first_pixel_ptr = first.images()[0].image().pixels().as_ptr();
    let first_charge = loader.budget_snapshot();
    assert_eq!(first_charge.active_reservations(), 1);

    let mut changed_entities = first_prefab.entities.clone();
    changed_entities.push(SceneEntityRecord::new(scene_id("variant")));
    project.write_prefab_source(&PrefabDocument::new(changed_entities));

    let second = loader.load(&candidate, &plan).unwrap();
    assert_ne!(second.revision(), first_revision);
    assert_ne!(second.content_digest(), first_digest);
    assert_eq!(second.schema_fingerprint(), first.schema_fingerprint());
    assert_ne!(
        second.images()[0].image().pixels().as_ptr(),
        first_pixel_ptr,
        "a new source revision must own a distinct residency charge",
    );
    assert_eq!(first.prefabs()[0].document(), &first_prefab);
    assert_eq!(first.images()[0].image().pixels(), first_pixels);
    assert_eq!(first.revision(), first_revision);
    assert_eq!(first.content_digest(), first_digest);

    let overlapped = loader.budget_snapshot();
    assert_eq!(overlapped.active_reservations(), 2);
    assert_eq!(
        overlapped.active(ProjectContentBudgetKind::ArtifactBytes),
        first_charge
            .active(ProjectContentBudgetKind::ArtifactBytes)
            .checked_mul(2)
            .unwrap(),
    );

    drop(first);
    assert_eq!(loader.budget_snapshot().active_reservations(), 1);
    drop(second);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}
