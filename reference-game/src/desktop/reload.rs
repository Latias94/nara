use std::{
    error::Error,
    future::Future,
    io::{self, Write},
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
    time::Instant,
};

use nara::{
    app::FrameExecutionStart,
    asset::{
        AssetEvents, AssetLoadError, AssetProductRef, AssetStates, ImportedProductId,
        NpakEntryKind, PreparedAsset, prepare_asset, set_runtime_asset_source,
    },
    audio::AudioClipLoader,
    ecs::{Mut, bevy_ecs::error::Result as SystemResult, schedule::IntoScheduleConfigs},
    prelude::*,
    render::{
        animation::SpriteAnimationLoader,
        image::ImageAssetLoader,
        material::{SpriteMaterialBindingAsset, SpriteMaterialLoader},
        tilemap::TileSetLoader,
    },
    render_wgpu::desktop::DesktopPresentation,
    scene::animation::PropertyAnimationLoader,
    tasks::{Task, io_pool},
};
use nara_asset::watch::{AssetWatcher, RELOAD_RETRY_DELAYS, is_transient_reload_error};

use crate::{
    arena::source::{SceneLayers, SourceProject},
    campaign::{Campaign, resolve_switch},
    simulation::{bind_arena, validate_arena},
};

use super::{is_game_frame, world::dress_arena};

#[derive(Resource)]
pub(super) struct Reload {
    source: Arc<ReloadSource>,
    active: AssetId,
    current: Option<SceneAsset>,
    task: Option<Task<Option<ReloadAttempt>>>,
    retry: Option<ReloadRetry>,
    last_error: Option<String>,
    trace: bool,
    report: Option<ReloadFrame>,
}

struct ReloadSource {
    root: PathBuf,
    project: SourceProject,
    layers: SceneLayers,
    watcher: AssetWatcher,
}

#[derive(Clone, Copy)]
struct ReloadRetry {
    at: Instant,
    attempt: usize,
}

struct ReloadAttempt {
    index: usize,
    result: Result<ReloadContent, ReloadFailure>,
}

struct ReloadFailure {
    message: String,
    transient: bool,
}

struct ReloadContent {
    assets: RuntimeAssetSource,
    scene: SceneAsset,
    dependencies: Vec<(AssetId, Dependency)>,
}

enum Dependency {
    Image(PreparedAsset<ImageAsset>),
    Material(PreparedAsset<SpriteMaterialBindingAsset>),
    Audio(PreparedAsset<AudioClip>),
    TileSet(PreparedAsset<TileSetAsset>),
    SpriteAnimation(PreparedAsset<SpriteAnimationSet>),
    PropertyAnimation(PreparedAsset<PropertyAnimationClip>),
}

impl ReloadSource {
    fn prepare(&self, active: AssetId, retry: Option<usize>) -> Option<ReloadAttempt> {
        let index = match self.watcher.take_changed() {
            Ok(true) => 0,
            Ok(false) => retry?,
            Err(error) => {
                return Some(ReloadAttempt {
                    index: 0,
                    result: Err(ReloadFailure {
                        message: format!("could not refresh the asset watch: {error}"),
                        transient: false,
                    }),
                });
            }
        };
        Some(ReloadAttempt {
            index,
            result: self.read_content(active).map_err(|error| ReloadFailure {
                transient: is_transient_reload_error(error.as_ref()),
                message: error.to_string(),
            }),
        })
    }

    fn read_content(&self, active: AssetId) -> Result<ReloadContent, Box<dyn Error>> {
        let assets = RuntimeAssetSource::from_project_dir(&self.root, "assets").map_err(
            |error| -> Box<dyn Error> {
                match error {
                    AssetLoadError::Scan(error) => Box::new(error),
                    error => Box::new(error),
                }
            },
        )?;
        let document = self.layers.read(&self.project, &assets, active)?;
        let scene = self.project.cook(&document)?;
        let dependencies = stage_dependencies(&assets, &scene)?;
        Ok(ReloadContent {
            assets,
            scene,
            dependencies,
        })
    }
}

fn stage_dependencies(
    source: &RuntimeAssetSource,
    scene: &SceneAsset,
) -> Result<Vec<(AssetId, Dependency)>, Box<dyn Error>> {
    use std::collections::BTreeSet;

    let mut pending: BTreeSet<_> = scene.asset_dependencies().iter().copied().collect();
    let mut staged = BTreeSet::new();
    let mut dependencies = Vec::new();
    while let Some(id) = pending.pop_first() {
        if !staged.insert(id) {
            continue;
        }
        let value = match source.backend().kind(id) {
            Some(NpakEntryKind::Image) => {
                Dependency::Image(prepare_asset(source, id, &ImageAssetLoader)?)
            }
            Some(NpakEntryKind::Material) => {
                Dependency::Material(prepare_asset(source, id, &SpriteMaterialLoader)?)
            }
            Some(NpakEntryKind::Audio) => {
                Dependency::Audio(prepare_asset(source, id, &AudioClipLoader)?)
            }
            Some(NpakEntryKind::TileSet) => {
                let prepared = prepare_asset(source, id, &TileSetLoader)?;
                let image = prepared.value().image;
                source
                    .backend()
                    .validate_kind(image, NpakEntryKind::Image)?;
                pending.insert(image);
                Dependency::TileSet(prepared)
            }
            Some(NpakEntryKind::SpriteAnimation) => {
                Dependency::SpriteAnimation(prepare_asset(source, id, &SpriteAnimationLoader)?)
            }
            Some(NpakEntryKind::PropertyAnimation) => {
                Dependency::PropertyAnimation(prepare_asset(source, id, &PropertyAnimationLoader)?)
            }
            kind => {
                return Err(format!("could not reload dependency {id} with kind {kind:?}").into());
            }
        };
        dependencies.push((id, value));
    }
    Ok(dependencies)
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
        root: PathBuf,
        project: SourceProject,
        layers: SceneLayers,
        active: AssetId,
        current: Option<SceneAsset>,
        watcher: AssetWatcher,
    ) -> Self {
        Self {
            source: Arc::new(ReloadSource {
                root,
                project,
                layers,
                watcher,
            }),
            active,
            current,
            task: None,
            retry: None,
            last_error: None,
            trace: std::env::var_os("NARA_RELOAD_TRACE").is_some_and(|value| value == "1"),
            report: None,
        }
    }

    pub fn install(self, app: &mut App) -> Result<(), Box<dyn Error>> {
        app.insert_resource(self)?
            .add_systems(
                CoreStage::TaskUpdate,
                reload_if_changed
                    .after(resolve_switch)
                    .before(nara::render::sprite::SpriteSourceHydrationSet)
                    .before(dress_arena),
            )?
            .add_systems(Last, observe_presentation)?;
        Ok(())
    }

    pub fn notified(&self) -> bool {
        self.task.as_ref().map_or_else(
            || {
                self.source.watcher.change_is_ready()
                    || self.retry.is_some_and(|retry| Instant::now() >= retry.at)
            },
            Task::is_finished,
        )
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

    fn advance(&mut self, world: &mut World, now: Instant) {
        if let Some(task) = &mut self.task {
            if let Poll::Ready(attempt) =
                Pin::new(task).poll(&mut Context::from_waker(Waker::noop()))
            {
                self.task = None;
                if let Some(attempt) = attempt {
                    self.finish_attempt(world, now, attempt);
                }
            }
            return;
        }

        let source = Arc::clone(&self.source);
        let retry = self
            .retry
            .filter(|retry| now >= retry.at)
            .map(|retry| retry.attempt);
        let active = self.active;
        self.task = Some(io_pool().spawn(async move { source.prepare(active, retry) }));
    }

    fn follow_sector(&mut self, active: AssetId, now: Instant) {
        if self.active != active {
            self.active = active;
            self.current = None;
            if self.task.take().is_some() || self.retry.is_some() {
                self.retry = Some(ReloadRetry {
                    at: now,
                    attempt: 0,
                });
            }
        }
    }

    fn finish_attempt(&mut self, world: &mut World, now: Instant, attempt: ReloadAttempt) {
        let result = attempt.result.and_then(|content| {
            self.publish(world, content).map_err(|error| ReloadFailure {
                message: error.to_string(),
                transient: false,
            })
        });
        self.retry = None;
        match result {
            Ok(()) => self.last_error = None,
            Err(error) => {
                if error.transient
                    && let Some(&delay) = RELOAD_RETRY_DELAYS.get(attempt.index)
                {
                    self.retry = Some(ReloadRetry {
                        at: now + delay,
                        attempt: attempt.index + 1,
                    });
                } else if self.last_error.as_ref() != Some(&error.message) {
                    eprintln!(
                        "Night Relay kept its current scene and assets after a reload error: {}",
                        error.message
                    );
                    self.last_error = Some(error.message);
                }
            }
        }
    }

    fn publish(&mut self, world: &mut World, content: ReloadContent) -> Result<(), Box<dyn Error>> {
        let resident = primary_handle(world, self.active)?
            .and_then(|handle| world.resource::<Assets<SceneAsset>>().get(handle));
        let previous = self.current.as_ref().or(resident);
        let scene_changed =
            previous.is_none_or(|previous| previous.bytes() != content.scene.bytes());
        world.resource::<SceneRegistry>().validate_asset(
            self.active,
            &content.scene,
            &content.assets,
            |scene| {
                validate_arena(scene)?;
                for row in scene.rows() {
                    if let Some(map) = row.get::<TilemapSource>() {
                        let tileset = content
                            .dependencies
                            .iter()
                            .find_map(|(id, dep)| match dep {
                                Dependency::TileSet(tileset) if *id == map.tileset => {
                                    Some(tileset.value())
                                }
                                _ => None,
                            })
                            .ok_or_else(|| format!("missing staged tileset {}", map.tileset))?;
                        map.validate_tile_indices(tileset.tiles.len())
                            .map_err(|error| error.to_string())?;
                    }
                    if let Some(source) = row.get::<SpriteAnimatorSource>() {
                        let set = content.dependencies.iter().find_map(|(id, dep)| match dep {
                            Dependency::SpriteAnimation(set) if *id == source.set => {
                                Some(set.value())
                            }
                            _ => None,
                        });
                        if set.and_then(|set| set.get(&source.clip)).is_none() {
                            return Err(format!(
                                "animation set {} has no clip '{}'",
                                source.set, source.clip
                            ));
                        }
                    }
                }
                Ok(())
            },
        )?;
        let server = world.resource::<AssetServer>();
        for (id, dependency) in &content.dependencies {
            let product = AssetProductRef::new((*id).into(), ImportedProductId::PRIMARY)?;
            match dependency {
                Dependency::Image(_) => server.validate_product_type::<ImageAsset>(product)?,
                Dependency::Material(_) => {
                    server.validate_product_type::<SpriteMaterialBindingAsset>(product)?;
                }
                Dependency::Audio(_) => server.validate_product_type::<AudioClip>(product)?,
                Dependency::TileSet(_) => server.validate_product_type::<TileSetAsset>(product)?,
                Dependency::SpriteAnimation(_) => {
                    server.validate_product_type::<SpriteAnimationSet>(product)?
                }
                Dependency::PropertyAnimation(_) => {
                    server.validate_product_type::<PropertyAnimationClip>(product)?
                }
            }
        }
        if scene_changed {
            let product = AssetProductRef::new(self.active.into(), ImportedProductId::PRIMARY)?;
            server.validate_product_type::<SceneAsset>(product)?;
            content
                .assets
                .backend()
                .validate_kind(self.active, NpakEntryKind::Scene)?;
        }
        let retired = previous
            .into_iter()
            .flat_map(SceneAsset::asset_dependencies)
            .filter(|id| {
                !content
                    .dependencies
                    .iter()
                    .any(|(staged, _)| *staged == **id)
            })
            .copied()
            .collect::<Vec<_>>();
        set_runtime_asset_source(world, content.assets)?;
        let mut images = 0;
        let mut materials = 0;
        for (id, dependency) in content.dependencies {
            match dependency {
                Dependency::Image(image) => {
                    images += usize::from(publish_asset(world, id, image, |old, new| {
                        old.content_hash() == new.content_hash()
                    })?);
                }
                Dependency::Material(binding) => {
                    materials += usize::from(publish_asset(world, id, binding, |old, new| {
                        old.0.uniform_bytes() == new.0.uniform_bytes()
                            && old.0.generation().shader().digest()
                                == new.0.generation().shader().digest()
                    })?);
                }
                Dependency::Audio(clip) => {
                    publish_asset(world, id, clip, |_, _| false)?;
                }
                Dependency::TileSet(tileset) => {
                    publish_asset(world, id, tileset, PartialEq::eq)?;
                }
                Dependency::SpriteAnimation(set) => {
                    publish_asset(world, id, set, PartialEq::eq)?;
                }
                Dependency::PropertyAnimation(clip) => {
                    publish_asset(world, id, clip, PartialEq::eq)?;
                }
            }
        }
        if scene_changed {
            let instance = switch_scene_asset_sync(world, self.active, &content.scene)?;
            bind_arena(world, instance, false).map_err(|error| error.to_string())?;
            publish_asset(
                world,
                self.active,
                PreparedAsset::from_value(content.scene.clone()),
                |_, _| false,
            )?;
            for id in retired {
                images += usize::from(retire_asset::<ImageAsset>(world, id)?);
                materials += usize::from(retire_asset::<SpriteMaterialBindingAsset>(world, id)?);
                retire_asset::<AudioClip>(world, id)?;
                retire_asset::<TileSetAsset>(world, id)?;
                retire_asset::<SpriteAnimationSet>(world, id)?;
                retire_asset::<PropertyAnimationClip>(world, id)?;
            }
        }
        self.current = Some(content.scene);
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

fn reload_if_changed(world: &mut World) -> SystemResult {
    let campaign = world.resource::<Campaign>();
    if campaign.pending.is_some() {
        return Ok(());
    }
    let active = campaign.sector.asset()?;
    let now = Instant::now();
    world.resource_scope(|world, mut reload: Mut<Reload>| {
        reload.follow_sector(active, now);
        if reload.notified() {
            reload.advance(world, now);
        }
    });
    Ok(())
}

fn observe_presentation(presentation: Res<DesktopPresentation>, reload: Res<Reload>) {
    reload.observe(&presentation);
}

fn primary_handle<T: Asset>(
    world: &World,
    id: AssetId,
) -> Result<Option<Handle<T>>, Box<dyn Error>> {
    let product = AssetProductRef::new(id.into(), ImportedProductId::PRIMARY)?;
    Ok(world.resource::<AssetServer>().get_product_handle(product))
}

fn publish_asset<T: Asset>(
    world: &mut World,
    id: AssetId,
    prepared: PreparedAsset<T>,
    unchanged: impl FnOnce(&T, &T) -> bool,
) -> Result<bool, Box<dyn Error>> {
    let handle = world.resource_mut::<AssetServer>().load_id::<T>(id)?;
    if world
        .resource::<Assets<T>>()
        .get(handle)
        .is_some_and(|old| unchanged(old, prepared.value()))
        && world
            .resource::<AssetStates>()
            .state(handle.id())
            .is_some_and(|state| state.dependencies() == prepared.dependencies())
    {
        return Ok(false);
    }
    prepared.publish(world, handle)?;
    Ok(true)
}

fn retire_asset<T: Asset>(world: &mut World, id: AssetId) -> Result<bool, Box<dyn Error>> {
    let Some(handle) = primary_handle::<T>(world, id)? else {
        return Ok(false);
    };
    let removed = world.resource_scope(|world, mut assets: Mut<Assets<T>>| {
        world.resource_scope(|world, mut states: Mut<AssetStates>| {
            assets.remove_with_state(
                handle,
                &mut states,
                &mut world.resource_mut::<AssetEvents>(),
            )
        })
    })?;
    Ok(removed.is_some())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use nara::scene::components::SpriteSource;

    use crate::{
        GamePlugin, Round, RunRules, arena::source, campaign::Sector,
        scene_roles::register_scene_roles,
    };

    use super::*;

    fn staged_project() -> io::Result<tempfile::TempDir> {
        let temporary = tempfile::tempdir()?;
        let assets_dir = temporary.path().join("assets");
        fs::create_dir(&assets_dir)?;
        for file in fs::read_dir(source::test_root().join("assets"))? {
            let file = file?;
            fs::copy(file.path(), assets_dir.join(file.file_name()))?;
        }
        Ok(temporary)
    }

    fn staged_app(root: &Path, snapshot: Option<&Path>) -> Result<(App, Reload), Box<dyn Error>> {
        let mut app = App::new();
        app.add_plugins(DefaultPlugins::headless())?;
        register_scene_roles(&mut app)?;
        let watcher = AssetWatcher::new(root.join("assets"))?;
        let assets = RuntimeAssetSource::from_project_dir(root, "assets")?;
        let project = SourceProject::new(&app)?;
        let active = Sector::One.asset()?;
        let document = snapshot
            .map(|path| project.read_snapshot(&assets, path))
            .transpose()?;
        let prepared = document
            .as_ref()
            .map(|scene| project.cook(scene))
            .transpose()?;
        app.set_asset_source(assets.clone())?;
        if let Some(scene) = &prepared {
            switch_scene_asset_sync(app.world_mut()?, active, scene)?;
        } else {
            switch_scene_sync(app.world_mut()?, active)?;
        }
        app.add_plugins(GamePlugin)?;
        app.step_fixed(FixedTime::DEFAULT_TIMESTEP)?;
        let layers = SceneLayers::capture(&project, &assets, active, document);
        let mut reload = Reload::new(root.to_owned(), project, layers, active, prepared, watcher);
        reload.trace = false;
        Ok((app, reload))
    }

    #[test]
    fn an_invalid_scene_reload_keeps_the_live_source_and_asset_versions()
    -> Result<(), Box<dyn Error>> {
        let temporary = staged_project()?;
        let assets = temporary.path().join("assets");
        let (mut app, mut reload) = staged_app(temporary.path(), None)?;
        let world = app.world();
        let source = world.resource::<RuntimeAssetSource>().clone();
        let instance = world.resource::<SceneManager>().main_instance();
        let round = world.resource::<Round>().clone();
        let material_id = "01a21a30-ef81-76a6-8eab-ca52f3bae4dc".parse()?;
        let material = primary_handle::<SpriteMaterialBindingAsset>(world, material_id)?
            .ok_or("missing material")?;
        let version = world.resource::<AssetStates>().version(material.id());

        let scene = assets.join("night-relay.scene.ron");
        fs::write(
            &scene,
            fs::read_to_string(&scene)?
                .replace("material: \"@gate_material\"", "image: \"@gate_material\""),
        )?;
        let binding = assets.join("gate.material.ron");
        fs::write(
            &binding,
            fs::read_to_string(&binding)?.replace(
                "SpriteMaterial(",
                "SpriteMaterial(color: (0.5, 1.0, 1.0, 1.0),",
            ),
        )?;
        let content = reload.source.read_content(reload.active)?;
        let error = reload
            .publish(app.world_mut()?, content)
            .expect_err("accepted an invalid sprite reference");
        assert!(error.to_string().contains("SpriteSource.image"), "{error}");

        let world = app.world();
        assert_eq!(world.resource::<RuntimeAssetSource>(), &source);
        assert_eq!(
            world.resource::<AssetStates>().version(material.id()),
            version
        );
        assert_eq!(world.resource::<SceneManager>().main_instance(), instance);
        assert_eq!(world.resource::<Round>(), &round);
        Ok(())
    }

    #[test]
    fn a_conflicting_asset_claim_rejects_reload_before_mounting_the_source()
    -> Result<(), Box<dyn Error>> {
        let temporary = staged_project()?;
        let assets = temporary.path().join("assets");
        let (mut app, mut reload) = staged_app(temporary.path(), None)?;
        let source = app.world().resource::<RuntimeAssetSource>().clone();
        let id: AssetId = "0193f100-2222-7000-8000-000000000004".parse()?;
        let product = AssetProductRef::new(id.into(), ImportedProductId::PRIMARY)?;
        app.world_mut()?
            .resource_mut::<AssetServer>()
            .reserve_product::<AudioClip>(product)?;

        let original = "01a219be-557a-739e-81a5-2a8c6e88aee5";
        fs::copy(
            assets.join("power-cell.png"),
            assets.join("replacement.png"),
        )?;
        let sidecar = fs::read_to_string(assets.join("power-cell.png.meta"))?;
        fs::write(
            assets.join("replacement.png.meta"),
            sidecar.replace(original, &id.to_string()),
        )?;
        let prefab = assets.join("power-cell.prefab.ron");
        fs::write(
            &prefab,
            fs::read_to_string(&prefab)?.replace(original, &id.to_string()),
        )?;

        let content = reload.source.read_content(reload.active)?;
        let error = reload
            .publish(app.world_mut()?, content)
            .expect_err("accepted conflicting claim");
        assert!(error.to_string().contains("is claimed as"), "{error}");
        assert_eq!(app.world().resource::<RuntimeAssetSource>(), &source);
        Ok(())
    }

    #[test]
    fn an_animation_reload_with_a_missing_clip_preserves_the_resident_set()
    -> Result<(), Box<dyn Error>> {
        let temporary = staged_project()?;
        let motion = temporary.path().join("assets/courier.sanim.ron");
        let original = fs::read_to_string(&motion)?;
        let (mut app, mut reload) = staged_app(temporary.path(), None)?;
        let id = "0193f100-2222-7000-8000-000000000101".parse()?;
        let handle = primary_handle::<SpriteAnimationSet>(app.world(), id)?
            .ok_or("missing courier animation")?;
        let source = app.world().resource::<RuntimeAssetSource>().clone();
        let version = app.world().resource::<AssetStates>().version(handle.id());
        let instance = app.world().resource::<SceneManager>().main_instance();

        fs::write(&motion, original.replace("\"idle\"", "\"different\""))?;
        let content = reload.source.read_content(reload.active)?;
        let error = reload
            .publish(app.world_mut()?, content)
            .expect_err("accepted a missing clip");
        assert!(error.to_string().contains("has no clip 'idle'"), "{error}");
        assert_eq!(app.world().resource::<RuntimeAssetSource>(), &source);
        assert_eq!(
            app.world().resource::<AssetStates>().version(handle.id()),
            version
        );

        fs::write(&motion, original.replace("110000000", "130000000"))?;
        let content = reload.source.read_content(reload.active)?;
        reload.publish(app.world_mut()?, content)?;
        let world = app.world();
        assert_ne!(
            world.resource::<AssetStates>().version(handle.id()),
            version
        );
        assert_eq!(world.resource::<SceneManager>().main_instance(), instance);
        assert!(
            world
                .resource::<Assets<SpriteAnimationSet>>()
                .get(handle)
                .unwrap()
                .get("idle")
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn reload_keeps_the_play_snapshot_and_last_good_world() -> Result<(), Box<dyn Error>> {
        let temporary = staged_project()?;
        let assets_dir = temporary.path().join("assets");
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

        let (mut app, mut reload) = staged_app(temporary.path(), Some(&snapshot))?;
        let round = app.world().resource::<Round>().clone();
        assert_eq!(round.shields(), 4);
        let image_id: AssetId = "01a219be-557a-739e-81a5-2a8c6e88aee5".parse()?;
        let material_id: AssetId = "01a21a30-ef81-76a6-8eab-ca52f3bae4dc".parse()?;
        let image_handle =
            primary_handle::<ImageAsset>(app.world(), image_id)?.ok_or("no image fixture")?;
        let material_handle =
            primary_handle::<SpriteMaterialBindingAsset>(app.world(), material_id)?
                .ok_or("no material fixture")?;
        let original_binding = app
            .world()
            .resource::<Assets<SpriteMaterialBindingAsset>>()
            .get(material_handle)
            .ok_or("no material fixture")?
            .clone();
        let original = app.world().resource::<SceneManager>().main_instance();

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
        let error = reload
            .source
            .read_content(reload.active)
            .err()
            .ok_or("accepted invalid PNG")?;
        reload.finish_attempt(
            app.world_mut()?,
            Instant::now(),
            ReloadAttempt {
                index: 0,
                result: Err(ReloadFailure {
                    message: error.to_string(),
                    transient: is_transient_reload_error(error.as_ref()),
                }),
            },
        );
        assert!(reload.retry.is_none());
        assert!(
            reload
                .last_error
                .as_ref()
                .is_some_and(|error| error.contains(&image_id.to_string()))
        );
        assert_eq!(
            app.world().resource::<SceneManager>().main_instance(),
            original
        );
        assert_eq!(app.world().resource::<Round>(), &round);
        assert_eq!(
            app.world()
                .resource::<Assets<SpriteMaterialBindingAsset>>()
                .get(material_handle),
            Some(&original_binding),
        );

        fs::write(&image_file, image_source)?;
        let image_meta = fs::read_to_string(&image_meta_file)?;
        fs::write(
            &image_meta_file,
            image_meta.replace("settings: ()", "settings: (srgb: false)"),
        )?;
        let content = reload.source.read_content(reload.active)?;
        reload.publish(app.world_mut()?, content)?;
        app.update_with_delta(std::time::Duration::ZERO)?;
        assert_eq!(app.world().resource::<Round>(), &round);
        assert_eq!(
            primary_handle::<ImageAsset>(app.world(), image_id)?,
            Some(image_handle)
        );
        let binding = app
            .world()
            .resource::<Assets<SpriteMaterialBindingAsset>>()
            .get(material_handle);
        assert_ne!(binding, Some(&original_binding));
        let shader = app
            .world()
            .get_resource::<RuntimeAssetSource>()
            .unwrap()
            .backend()
            .resolve_path(&nara::asset::AssetPath::new("gate.wgsl")?)
            .unwrap();
        assert_eq!(
            app.world()
                .resource::<AssetStates>()
                .state(material_handle.id())
                .unwrap()
                .dependencies(),
            &[shader]
        );
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
                .query::<&Sprite>()
                .iter(world)
                .filter(|sprite| sprite.material.image == Some(image_handle)
                    && sprite.size == Vec2::splat(16.0))
                .count(),
            3,
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn a_split_same_id_rename_and_locked_prefab_keep_the_triggered_revision()
    -> Result<(), Box<dyn Error>> {
        use std::{os::windows::fs::OpenOptionsExt, thread, time::Duration};

        fn drive_until(reload: &mut Reload, world: &mut World, reached: impl Fn(&Reload) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !reached(reload) {
                assert!(
                    Instant::now() < deadline,
                    "the reload worker did not advance"
                );
                if reload.notified() {
                    reload.advance(world, Instant::now());
                }
                thread::sleep(Duration::from_millis(3));
            }
        }

        let temporary = staged_project()?;
        let assets_dir = temporary.path().join("assets");
        let (mut app, mut reload) = staged_app(temporary.path(), None)?;
        let original = app.world().resource::<SceneManager>().main_instance();

        let old = assets_dir.join("power-cell.prefab.ron");
        let moved = assets_dir.join("moved.prefab.ron");
        fs::rename(old, &moved)?;
        drive_until(&mut reload, app.world_mut()?, |reload| {
            reload.retry.is_some()
        });
        let pending = reload.retry.ok_or("incomplete pair was not retried")?;
        assert_eq!(pending.attempt, 1);
        assert_eq!(
            app.world().resource::<SceneManager>().main_instance(),
            original
        );

        fs::write(
            &moved,
            fs::read_to_string(&moved)?.replace("(15.0, 15.0)", "(17.0, 17.0)"),
        )?;
        fs::rename(
            assets_dir.join("power-cell.prefab.ron.meta"),
            assets_dir.join("moved.prefab.ron.meta"),
        )?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&moved)?;
        let error = reload
            .source
            .read_content(reload.active)
            .err()
            .ok_or("the prefab remained readable while exclusively held")?;
        assert!(is_transient_reload_error(error.as_ref()), "{error}");
        assert!(
            error
                .to_string()
                .contains("01a219be-557a-7c47-82c3-84a2b7f7894c")
        );
        drive_until(&mut reload, app.world_mut()?, |reload| {
            reload.retry.is_some_and(|retry| retry.at != pending.at)
        });
        assert_eq!(
            app.world().resource::<SceneManager>().main_instance(),
            original
        );
        drop(lock);

        drive_until(&mut reload, app.world_mut()?, |reload| {
            reload.retry.is_none()
        });
        let world = app.world_mut()?;
        assert_ne!(world.resource::<SceneManager>().main_instance(), original);
        assert_eq!(
            world
                .query::<&SpriteSource>()
                .iter(world)
                .filter(|sprite| sprite.image.is_some() && sprite.size == Vec2::splat(17.0))
                .count(),
            3,
        );
        Ok(())
    }
}
