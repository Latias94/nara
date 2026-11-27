use std::{
    collections::BTreeMap,
    error::Error,
    io::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
};

use nara::{
    app::FrameExecutionStart,
    asset::{AssetEvents, AssetStates, Assets, add_asset},
    core::AssetId,
    ecs::{Mut, bevy_ecs::query::With},
    prelude::*,
    render::material::Material2dBinding,
    render_wgpu::desktop::DesktopPresentation,
    scene::{authoring::SceneDocument, spatial::propagate_spatial},
};
use nara_asset::{ProjectAssetDatabase, watch::AssetWatcher};

use crate::{
    arena::source::{SceneLayers, SourceProject, SourceScene},
    simulation::{ArenaEntities, PilotCharacter, sync_actors},
};

use super::{
    is_game_frame,
    world::{self, SceneDecoration, SceneVisuals},
};

#[derive(Resource)]
pub(super) struct Reload {
    project: SourceProject,
    layers: SceneLayers,
    current: SceneDocument,
    watcher: AssetWatcher,
    changed: AtomicBool,
    last_error: Option<String>,
    trace: bool,
    report: Option<ReloadFrame>,
}

struct ReloadFrame {
    applied: u64,
    scene: bool,
    images: usize,
    materials: usize,
    reported: AtomicBool,
}

impl Reload {
    pub fn new(
        project: SourceProject,
        assets: &ProjectAssetDatabase,
        current: SceneDocument,
        snapshot: bool,
        watcher: AssetWatcher,
    ) -> Self {
        let layers = SceneLayers::capture(&project, assets, &current, snapshot);
        Self {
            project,
            layers,
            current,
            watcher,
            changed: AtomicBool::new(false),
            last_error: None,
            trace: std::env::var_os("NARA_RELOAD_TRACE").is_some_and(|value| value == "1"),
            report: None,
        }
    }

    pub fn install(self, app: &mut App) -> Result<(), Box<dyn Error>> {
        app.insert_resource(self)?
            .add_systems(CoreStage::TaskUpdate, reload_if_changed)?
            .add_systems(Last, observe_presentation)?;
        Ok(())
    }

    pub fn notified(&self) -> bool {
        if self.watcher.take_changed() {
            self.changed.store(true, Ordering::Relaxed);
        }
        self.changed.load(Ordering::Relaxed)
    }

    pub fn observe(&self, presentation: &DesktopPresentation) {
        let Some(report) = &self.report else { return };
        let Some(frame) = presentation.latest().filter(is_game_frame) else {
            return;
        };
        if frame.frame_index < report.applied || report.reported.swap(true, Ordering::Relaxed) {
            return;
        }
        let _ = writeln!(
            io::stdout().lock(),
            "NIGHT_RELAY_RELOAD applied_frame={} presented_frame={} scene={} images={} materials={}",
            report.applied,
            frame.frame_index,
            report.scene,
            report.images,
            report.materials,
        );
        let _ = io::stdout().flush();
    }

    fn refresh(&mut self, world: &mut World) -> Result<(), Box<dyn Error>> {
        let assets = self.project.assets()?;
        let scene = self.layers.read(&self.project, &assets)?;
        let content = self.project.prepare_content(&assets, scene)?;
        let replacement = if self.current.entities != content.scene.entities {
            let scene = SourceScene::prepare(&content.scene, None)?;
            let character = PilotCharacter::new(scene.arena().pilot.radius)?;
            Some((scene, character))
        } else {
            None
        };

        let (images, materials) =
            world.resource_scope(|world, mut visuals: Mut<SceneVisuals>| {
                let images = update_images(world, &mut visuals.images, content.images)?;
                let materials = update_materials(&mut visuals.materials, content.materials);
                Ok::<_, Box<dyn Error>>((images, materials))
            })?;
        let scene_changed = replacement.is_some();
        if let Some((prepared, character)) = replacement {
            replace_scene(world, prepared, character)?;
        } else if materials != 0 {
            world::bind_materials(world)?;
        }
        self.current = content.scene;
        if self.trace && (scene_changed || images != 0 || materials != 0) {
            self.report = Some(ReloadFrame {
                applied: world.resource::<FrameExecutionStart>().frame(),
                scene: scene_changed,
                images,
                materials,
                reported: AtomicBool::new(false),
            });
        }
        Ok(())
    }
}

fn reload_if_changed(world: &mut World) {
    let reload = world.resource::<Reload>();
    if !reload.notified() || !reload.changed.swap(false, Ordering::Relaxed) {
        return;
    }
    world.resource_scope(|world, mut reload: Mut<Reload>| {
        match reload.refresh(world) {
            Ok(()) => reload.last_error = None,
            Err(error) => {
                let message = error.to_string();
                if reload.last_error.as_ref() != Some(&message) {
                    eprintln!("Night Relay kept its current scene and assets after a reload error: {message}");
                    reload.last_error = Some(message);
                }
            }
        }
    });
}

fn observe_presentation(presentation: Res<DesktopPresentation>, reload: Res<Reload>) {
    reload.observe(&presentation);
}

fn replace_scene(
    world: &mut World,
    scene: SourceScene<'_>,
    character: PilotCharacter,
) -> Result<(), Box<dyn Error>> {
    let previous = std::mem::take(&mut world.resource_mut::<ArenaEntities>().0);
    let decorations = world
        .query_filtered::<Entity, With<SceneDecoration>>()
        .iter(world)
        .collect::<Vec<_>>();
    for entity in previous.into_iter().chain(decorations) {
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
    }
    let (arena, _, entities) = scene.spawn(world);
    world.insert_resource(ArenaEntities(entities));
    world.insert_resource(arena);
    world.insert_resource(character);
    world::install_sprites(world)?;
    world.run_system_cached(sync_actors)?;
    world.run_system_cached(world::update_appearance)?;
    world.run_system_cached(propagate_spatial)?;
    Ok(())
}

fn update_images(
    world: &mut World,
    handles: &mut BTreeMap<AssetId, Handle<ImageAsset>>,
    updated: BTreeMap<AssetId, ImageAsset>,
) -> Result<usize, Box<dyn Error>> {
    let removed = handles
        .keys()
        .filter(|id| !updated.contains_key(id))
        .copied()
        .collect::<Vec<_>>();
    let mut changed = removed.len();
    for (id, image) in updated {
        if let Some(&handle) = handles.get(&id) {
            let current = world.resource::<Assets<ImageAsset>>().get(handle);
            if current.is_some_and(|current| current.content_hash() == image.content_hash()) {
                continue;
            }
            world.resource_scope(|world, mut images: Mut<Assets<ImageAsset>>| {
                world.resource_scope(|world, mut states: Mut<AssetStates>| {
                    images.commit_loaded(
                        handle,
                        image,
                        &mut states,
                        &mut world.resource_mut::<AssetEvents>(),
                    )
                })
            })?;
        } else {
            handles.insert(id, add_asset(world, image)?);
        }
        changed += 1;
    }
    for id in removed {
        let handle = handles[&id];
        world.resource_scope(|world, mut images: Mut<Assets<ImageAsset>>| {
            world.resource_scope(|world, mut states: Mut<AssetStates>| {
                images.remove_with_state(
                    handle,
                    &mut states,
                    &mut world.resource_mut::<AssetEvents>(),
                )
            })
        })?;
        handles.remove(&id);
    }
    Ok(changed)
}

fn update_materials(
    current: &mut BTreeMap<AssetId, Material2dBinding>,
    updated: BTreeMap<AssetId, Material2dBinding>,
) -> usize {
    let previous = current.len();
    current.retain(|id, _| updated.contains_key(id));
    let mut changed = previous - current.len();
    for (id, binding) in updated {
        if current.get(&id).is_some_and(|old| {
            old.uniform_bytes() == binding.uniform_bytes()
                && old.generation().shader().digest() == binding.generation().shader().digest()
        }) {
            continue;
        }
        current.insert(id, binding);
        changed += 1;
    }
    changed
}

#[cfg(test)]
mod tests {
    use std::fs;

    use nara::scene::components::SpriteSource;

    use crate::{GamePlugin, Round, RunRules, arena::source};

    use super::*;

    #[test]
    fn reload_keeps_the_play_snapshot_and_last_good_world() -> Result<(), Box<dyn Error>> {
        let temporary = tempfile::tempdir()?;
        let assets_dir = temporary.path().join("assets");
        fs::create_dir(&assets_dir)?;
        for file in fs::read_dir(source::test_root().join("assets"))? {
            let file = file?;
            fs::copy(file.path(), assets_dir.join(file.file_name()))?;
        }
        let scene_file = assets_dir.join("night-relay.scene.ron");
        let prefab_file = assets_dir.join("power-cell.prefab.ron");
        let material_file = assets_dir.join("gate.material.ron");
        let image_file = assets_dir.join("power-cell.png");
        let image_meta_file = assets_dir.join("power-cell.png.meta");
        let scene_source = fs::read_to_string(&scene_file)?;
        let prefab_source = fs::read_to_string(&prefab_file)?;
        let material_source = fs::read_to_string(&material_file)?;
        let image_source = fs::read(&image_file)?;
        let snapshot = temporary.path().join("snapshot.scene.ron");
        fs::write(
            &snapshot,
            scene_source.replace(
                "\"night-relay.RunRules\": ()",
                "\"night-relay.RunRules\": (shield_charges: 4)",
            ),
        )?;

        let mut app = App::new();
        app.add_plugins(DefaultPlugins::headless())?;
        let project = SourceProject::new(&mut app, temporary.path())?;
        let assets = project.assets()?;
        let saved = project.read_snapshot(&assets, &snapshot)?;
        let content = project.prepare_content(&assets, saved)?;
        SceneVisuals::install(app.world_mut()?, content.images, content.materials)?;
        let watcher = AssetWatcher::new(&assets_dir)?;
        let mut reload = Reload::new(project, &assets, content.scene.clone(), true, watcher);
        reload.trace = false;
        app.add_plugins(GamePlugin::from_scene(content.scene))?;
        world::setup(app.world_mut()?).map_err(|error| error.to_string())?;
        app.step_fixed(FixedTime::DEFAULT_TIMESTEP)?;
        let round = app.world().resource::<Round>().clone();
        assert_eq!(round.shields(), 4);
        let image_handle = *app
            .world()
            .resource::<SceneVisuals>()
            .images
            .values()
            .next()
            .ok_or("no image fixture")?;
        let original_binding = app
            .world()
            .resource::<SceneVisuals>()
            .materials
            .values()
            .next()
            .ok_or("no material fixture")?
            .clone();
        let original_entities = app.world().resource::<ArenaEntities>().0.clone();

        fs::write(
            &scene_file,
            scene_source.replace(
                "\"night-relay.RunRules\": ()",
                "\"night-relay.RunRules\": (shield_charges: 2, move_speed: 210.0)",
            ),
        )?;
        fs::write(
            &prefab_file,
            prefab_source.replace("(15.0, 15.0)", "(16.0, 16.0)"),
        )?;
        fs::write(
            &material_file,
            material_source.replace(
                "SpriteMaterial(",
                "SpriteMaterial(color: (0.5, 1.0, 1.0, 1.0),",
            ),
        )?;
        fs::write(&image_file, b"invalid PNG")?;
        assert!(reload.refresh(app.world_mut()?).is_err());
        assert_eq!(app.world().resource::<ArenaEntities>().0, original_entities);
        assert_eq!(app.world().resource::<Round>(), &round);
        assert_eq!(
            app.world()
                .resource::<SceneVisuals>()
                .materials
                .values()
                .next(),
            Some(&original_binding),
        );

        fs::write(&image_file, image_source)?;
        let image_meta = fs::read_to_string(&image_meta_file)?;
        fs::write(
            &image_meta_file,
            image_meta.replace("settings: ()", "settings: (srgb: false)"),
        )?;
        reload.refresh(app.world_mut()?)?;
        assert_eq!(app.world().resource::<Round>(), &round);
        let visuals = app.world().resource::<SceneVisuals>();
        assert_eq!(visuals.images.values().copied().next(), Some(image_handle));
        assert_ne!(visuals.materials.values().next(), Some(&original_binding));
        let image = app
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(image_handle)
            .ok_or("missing image")?;
        assert_eq!(image.color_space(), ImageColorSpace::Linear);
        let world = app.world_mut()?;
        let rules = world.query::<&RunRules>().single(world)?;
        assert_eq!(rules.shield_charges, 4);
        assert_eq!(rules.move_speed, 210.0);
        assert_eq!(
            world
                .query::<&SpriteSource>()
                .iter(world)
                .filter(|sprite| sprite.image.is_some() && sprite.size == Vec2::splat(16.0))
                .count(),
            3,
        );
        Ok(())
    }
}
