use nara::{
    asset::{AssetProductRef, AssetStates, Assets, LoadState},
    core::ItemLimit,
    ecs::World,
    image::ImageAsset,
    project_host::{
        ProjectSceneRequestError, ProjectSceneRequestState, ProjectSceneSourceUpdateStatus,
        ProjectSceneWatchStatus, ProjectScenes, advanced::commit_prepared_project_scene,
    },
    scene::advanced::SceneProductTransactionLimits,
    sprite::Sprite,
};

use super::{GameProgress, Step, Workflow, actor, prepared, resident};

pub(super) fn update_status(world: &mut World) -> ProjectSceneSourceUpdateStatus {
    ProjectScenes::scope(world, |scenes| scenes.source_update_status().unwrap())
}

pub(super) fn watch_status(world: &mut World) -> ProjectSceneWatchStatus {
    ProjectScenes::scope(world, |scenes| scenes.source_watch_status().unwrap())
}

fn next_result(world: &mut World, game: &mut Workflow) -> Option<ProjectSceneRequestState> {
    let update = update_status(world);
    let Some(request) = update.request else {
        assert!(game.request.is_none());
        return None;
    };
    if let Some(expected) = game.request {
        assert_eq!(request, expected);
    } else {
        assert!(!game.requests.contains(&request));
        game.requests.push(request);
        game.request = Some(request);
    }
    let state = ProjectScenes::scope(world, |scenes| scenes.state(request)).unwrap();
    if state == ProjectSceneRequestState::Preparing {
        assert_eq!(update.error, None);
        ProjectScenes::scope(world, |scenes| {
            assert!(scenes.prepared(request).is_none());
            assert!(!scenes.acknowledge(request));
        });
        None
    } else {
        Some(state)
    }
}

fn assert_content_rejection(world: &mut World, game: &Workflow, state: ProjectSceneRequestState) {
    let ProjectSceneRequestState::Rejected(error @ ProjectSceneRequestError::Content(_)) = state
    else {
        panic!("the source candidate must report a content rejection: {state:?}");
    };
    assert_eq!(update_status(world).error, Some(error));
    ProjectScenes::scope(world, |scenes| {
        let request = game.request.unwrap();
        assert!(scenes.prepared(request).is_none());
        assert!(scenes.diagnostics(request).unwrap().has_errors());
    });
}

pub(super) fn acknowledge(world: &mut World, game: &mut Workflow) {
    let request = game.request.take().unwrap();
    ProjectScenes::scope(world, |scenes| {
        assert_eq!(
            scenes.source_update_status().unwrap().request,
            Some(request)
        );
        for previous in game.requests.iter().copied().filter(|id| *id != request) {
            assert!(scenes.state(previous).is_none());
            assert!(!scenes.acknowledge(previous));
        }
        assert!(scenes.acknowledge(request));
        assert!(!scenes.acknowledge(request));
        assert!(scenes.state(request).is_none());
        assert!(scenes.prepared(request).is_none());
        assert_eq!(scenes.source_update_status().unwrap().request, None);
    });
    game.candidate = None;
}

fn discard_ready(world: &mut World, game: &mut Workflow) {
    let request = game.request.unwrap();
    ProjectScenes::scope(world, |scenes| {
        assert!(!scenes.acknowledge(request));
        assert!(scenes.cancel(request));
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Cancelled)
        );
        assert!(scenes.prepared(request).is_none());
    });
    acknowledge(world, game);
    game.duplicates += 1;
}

pub(super) fn ready(
    world: &mut World,
    game: &mut Workflow,
    path: &str,
    source: &str,
    texture: AssetProductRef,
    changed_stamp: bool,
) -> bool {
    let Some(state) = next_result(world, game) else {
        return false;
    };
    if state != ProjectSceneRequestState::Ready {
        assert_content_rejection(world, game, state);
        acknowledge(world, game);
        game.duplicates += 1;
        return false;
    }
    let request = game.request.unwrap();
    let candidate = prepared(world, request);
    let current = game.current.as_ref().unwrap();
    if candidate.material.inputs != [(path.to_owned(), source.to_owned())]
        || candidate.material.texture != texture
        || changed_stamp && candidate.stamp == current.stamp
    {
        discard_ready(world, game);
        return false;
    }
    assert_eq!(update_status(world).error, None);
    candidate.material.assert_shader(path, source);
    assert_eq!(candidate.material.interface, current.material.interface);
    assert_eq!(candidate.material.declaration, current.material.declaration);
    assert_eq!(candidate.material.defaults, current.material.defaults);
    ProjectScenes::scope(world, |scenes| assert!(!scenes.acknowledge(request)));
    game.candidate = Some(candidate);
    true
}

pub(super) fn rejected(world: &mut World, game: &mut Workflow) -> bool {
    let Some(state) = next_result(world, game) else {
        return false;
    };
    if state == ProjectSceneRequestState::Ready {
        discard_ready(world, game);
        return false;
    }
    assert_content_rejection(world, game, state);
    game.rejections += 1;
    true
}

pub(super) fn commit(world: &mut World, game: &mut Workflow) {
    let before = game.current.as_ref().unwrap().clone();
    let candidate = game.candidate.as_ref().unwrap().clone();
    let request = game.request.unwrap();
    assert_eq!(prepared(world, request), candidate);
    assert_eq!(resident(world), before);
    let gameplay = world.get::<GameProgress>(before.entity).unwrap().clone();
    let report = commit_prepared_project_scene(
        world,
        request,
        SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
        &[],
        |overlay| {
            overlay.insert_component(actor(), gameplay);
        },
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let after = resident(world);
    assert_eq!(report.instance.as_ref(), Some(&after.instance));
    assert_ne!(after.instance, before.instance);
    assert_ne!(after.entity, before.entity);
    assert!(world.get::<GameProgress>(before.entity).is_none());
    assert!(world.get::<Sprite>(before.entity).is_none());
    assert_eq!(after.gameplay, before.gameplay);
    assert_eq!(after.stamp, candidate.stamp);
    assert_eq!(after.material, candidate.material);
    assert_eq!(after.material_handle, before.material_handle);
    assert_eq!(
        after.material_state.version().raw(),
        before.material_state.version().raw() + 1
    );
    assert_ne!(after.material_revision, before.material_revision);
    for (product, old) in &before.images {
        if let Some(new) = after.images.get(product) {
            assert_eq!(new.handle, old.handle);
            assert_eq!(new.state.version().raw(), old.state.version().raw() + 1);
            assert_ne!(new.revision, old.revision);
            if game.step != Step::PalettePixels {
                assert_eq!(new.hash, old.hash);
                assert_eq!(new.pixels, old.pixels);
                assert_eq!(new.state.source_hash(), old.state.source_hash());
            }
        } else {
            assert!(
                world
                    .resource::<Assets<ImageAsset>>()
                    .get(old.handle)
                    .is_none()
            );
            let removed = world
                .resource::<AssetStates>()
                .state(old.handle.id())
                .unwrap();
            assert_eq!(removed.load_state(), &LoadState::Removed);
            assert_eq!(removed.usable_version(), None);
            assert_eq!(removed.version().raw(), old.state.version().raw() + 1);
        }
    }
    for (product, new) in &after.images {
        if !before.images.contains_key(product) {
            assert_eq!(new.state.version().raw(), 1);
        }
    }
    ProjectScenes::scope(world, |scenes| {
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Committed)
        );
        assert!(scenes.prepared(request).is_none());
        assert!(!scenes.diagnostics(request).unwrap().has_errors());
    });
    game.current = Some(after);
    game.commits += 1;
}

pub(super) fn drain_duplicate(world: &mut World, game: &mut Workflow) -> bool {
    if update_status(world).request.is_none() {
        assert!(game.request.is_none());
        return true;
    }
    if ready(
        world,
        game,
        super::NEXT_SHADER,
        &super::shader("0.125"),
        super::discovered_product(),
        false,
    ) {
        assert_eq!(
            game.candidate.as_ref().unwrap().stamp,
            game.current.as_ref().unwrap().stamp
        );
        assert_eq!(
            game.candidate.as_ref().unwrap().material,
            game.current.as_ref().unwrap().material
        );
        discard_ready(world, game);
    }
    false
}

pub(super) fn drain_failed_duplicate(world: &mut World, game: &mut Workflow) -> bool {
    if update_status(world).request.is_none() {
        assert!(game.request.is_none());
        return true;
    }
    let Some(state) = next_result(world, game) else {
        return false;
    };
    assert_content_rejection(world, game, state);
    acknowledge(world, game);
    game.duplicates += 1;
    false
}
