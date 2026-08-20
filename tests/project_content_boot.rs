#![cfg(all(feature = "serde", feature = "runtime-2d"))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::sync::Arc;

use nara::asset::AssetMeta;
use nara::image::{ImageColorSpace, ImageImportSettings};
use nara::project_host::{
    ProjectContentBudgetKind, ProjectContentLoader, StartupContentMemberKind,
};
use nara::scene::{PrefabDocument, SceneComponentRecord, SceneEntityRecord};
use project_content_fixture::{TestProject, player_image_meta, scene_id};

struct EscapeOwner;

#[test]
fn strict_project_content_load_uses_the_persisted_linear_recipe() {
    let project = TestProject::with_prefab_startup();
    let meta = player_image_meta();
    let (identity, _) = meta.into_parts();
    let meta = AssetMeta::new(identity, ImageImportSettings::new(ImageColorSpace::Linear));
    project.write_player_image_meta(&meta);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let snapshot = loader.load(&candidate, &plan).unwrap();
    let image = snapshot.images()[0].image();

    assert_eq!(image.color_space(), ImageColorSpace::Linear);
    assert_eq!(image.source().settings(), *meta.settings());
    assert_eq!(
        image.source().artifact().key().settings_hash(),
        meta.settings().settings_hash()
    );
}

#[test]
fn authorized_startup_closure_publishes_an_immutable_snapshot() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let snapshot = loader.load(&candidate, &plan).unwrap();
    let cloned = snapshot.clone();
    let closure = snapshot.startup_content_closure();

    assert_eq!(snapshot.lineage(), candidate.lineage());
    assert_eq!(
        snapshot.schema_fingerprint(),
        plan.schema_validation().composition_fingerprint()
    );
    assert_eq!(closure.settings_lineage(), snapshot.lineage());
    assert_eq!(closure.manifest_digest(), candidate.manifest_digest());
    assert_eq!(closure.schema_fingerprint(), snapshot.schema_fingerprint());
    assert_eq!(closure.content_revision(), snapshot.revision());
    assert_eq!(closure.content_digest(), snapshot.content_digest());
    let members = closure
        .members()
        .iter()
        .map(|member| {
            (
                member.path().as_str(),
                member.kind(),
                member.byte_len(),
                member.digest(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        members
            .iter()
            .map(|(path, kind, _, _)| (*path, *kind))
            .collect::<Vec<_>>(),
        vec![
            (
                "assets/textures/player.png",
                StartupContentMemberKind::AssetSource,
            ),
            (
                "assets/textures/player.png.meta",
                StartupContentMemberKind::AssetMetadata,
            ),
            ("nara.toml", StartupContentMemberKind::ProjectManifest),
            (
                "prefabs/enemy.prefab.json",
                StartupContentMemberKind::Prefab,
            ),
            ("scenes/startup.scene.json", StartupContentMemberKind::Scene,),
        ]
    );
    assert!(
        members
            .iter()
            .all(|(_, _, byte_len, digest)| { *byte_len > 0 && *byte_len == digest.length() })
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
    assert_eq!(leased.active_reservations(), 2);

    drop(snapshot);
    assert_eq!(loader.budget_snapshot(), leased);
    drop(cloned);
    assert_eq!(loader.budget_snapshot(), leased);
    drop(escaped);
    let closure_only = loader.budget_snapshot();
    assert_eq!(
        closure_only.active(ProjectContentBudgetKind::ArtifactBytes),
        0
    );
    assert!(closure_only.active(ProjectContentBudgetKind::RetainedBytes) > 0);
    assert_eq!(closure_only.active_reservations(), 1);
    drop(closure);
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
    assert_eq!(first_charge.active_reservations(), 2);

    let mut changed_entities = first_prefab.entities.clone();
    changed_entities.push(SceneEntityRecord::new(scene_id("variant")));
    project.write_prefab_source(&PrefabDocument::from_ordered_roots(changed_entities));

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
    assert_eq!(overlapped.active_reservations(), 4);
    assert_eq!(
        overlapped.active(ProjectContentBudgetKind::ArtifactBytes),
        first_charge
            .active(ProjectContentBudgetKind::ArtifactBytes)
            .checked_mul(2)
            .unwrap(),
    );

    drop(first);
    assert_eq!(loader.budget_snapshot().active_reservations(), 2);
    drop(second);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[cfg(feature = "runtime-ui")]
#[test]
fn ui_visual_asset_reference_alone_enters_the_startup_content_closure() {
    use std::fs;

    use nara::{
        ecs::World,
        reflect::{ComponentRegistry, ComponentTypeId},
        ui::{UiColor, UiImageId, UiLayer, UiNode, UiRoot, UiStyle, UiVisual},
    };

    let project = TestProject::with_prefab_startup();
    fs::write(
        project.path().join("nara.toml"),
        r#"schema_version = 1

[project]
name = "UI Content Test"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[capabilities]
requested = ["runtime-2d", "runtime-ui"]
"#,
    )
    .unwrap();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let registry = plan.schema_validation().registry();
    let mut world = World::new();
    let root_source = world
        .spawn((UiRoot::new(UiLayer::Base), UiNode::new(UiStyle::fill())))
        .id();
    let image_source = world
        .spawn((
            UiNode::new(UiStyle::absolute(24.0, 16.0, 128.0, 128.0)),
            UiVisual::image(
                UiImageId::new("textures/player.png").unwrap(),
                UiColor::rgba(1.0, 1.0, 1.0, 1.0),
            ),
        ))
        .id();
    let root_id = scene_id("ui-root");
    let image_id = scene_id("ui-image");
    let root_record = SceneEntityRecord::new(root_id.clone())
        .with_children([image_id.clone()])
        .with_component(
            ComponentTypeId::new("nara.ui.UiRoot"),
            encoded_component(registry, &world, root_source, "nara.ui.UiRoot"),
        )
        .with_component(
            ComponentTypeId::new("nara.ui.UiNode"),
            encoded_component(registry, &world, root_source, "nara.ui.UiNode"),
        );
    let image_record = SceneEntityRecord::new(image_id)
        .with_component(
            ComponentTypeId::new("nara.ui.UiNode"),
            encoded_component(registry, &world, image_source, "nara.ui.UiNode"),
        )
        .with_component(
            ComponentTypeId::new("nara.ui.UiVisual"),
            encoded_component(registry, &world, image_source, "nara.ui.UiVisual"),
        );
    project.write_prefab_source(&PrefabDocument::new([root_id], [root_record, image_record]));

    let snapshot = ProjectContentLoader::new(root)
        .unwrap()
        .load(&candidate, &plan)
        .unwrap();
    assert_eq!(snapshot.images().len(), 1);
    assert_eq!(snapshot.images()[0].path().as_str(), "textures/player.png");
    let members = snapshot.startup_content_closure();
    assert!(members.members().iter().any(|member| {
        member.kind() == StartupContentMemberKind::AssetSource
            && member.path().as_str() == "assets/textures/player.png"
    }));

    fn encoded_component(
        registry: &ComponentRegistry,
        world: &World,
        entity: nara::ecs::Entity,
        component: &str,
    ) -> SceneComponentRecord {
        let component = ComponentTypeId::new(component);
        let schema = registry.schema(&component).unwrap();
        let value = registry
            .encode_component(&component, world, entity)
            .expect("the selected runtime plan registers the UI component codec")
            .expect("the first-party UI component must encode")
            .expect("the fixture entity contains the selected UI component");
        SceneComponentRecord::new(schema.version, value)
    }
}
