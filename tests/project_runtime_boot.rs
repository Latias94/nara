#![cfg(all(feature = "serde", feature = "runtime-2d"))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

#[cfg(feature = "presentation-ui")]
#[path = "support/profile_font_boot.rs"]
mod profile_font_boot;

#[cfg(feature = "presentation-ui")]
#[path = "support/profile_font_product.rs"]
mod profile_font_product;

#[cfg(all(feature = "presentation-ui", feature = "tooling"))]
#[path = "support/profile_font_editor_play.rs"]
mod profile_font_editor_play;

#[cfg(all(
    feature = "presentation-ui",
    feature = "project-workflow-cli",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
#[path = "support/profile_font_native_trajectory.rs"]
mod profile_font_native_trajectory;

use std::{
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use nara::{
    ProductConfiguration, ProductRecipe,
    app::{
        App, AppExitRequests, CoreStage, FixedTime, FixedUpdateSet, Plugin, PluginCategory,
        PluginDeclaration, PluginDefinition, PluginDefinitionId, PluginDomainError, PluginError,
        PluginHook, PluginId, PluginPreflightContext, PluginShutdownContext,
        PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
        RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
    },
    diagnostic::{DiagnosticReport, DiagnosticValueRef},
    ecs::{Query, Res, ResMut, Resource, schedule::IntoScheduleConfigs},
    gameplay::{
        GAMEPLAY_COMMAND_PLUGIN_ID, GameplayCommandDraft, GameplayCommandIngressSource,
        GameplayCommandSet, GameplayCommandSourceSequence, GameplayCommandSubmission,
        GameplayCommandTick, GameplayCommandTypeId,
    },
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, HeadlessRunReport,
        advanced::{RuntimeProbe, headless_run_with_probe},
    },
    scene::SceneEntitySource,
};
use project_content_fixture::{
    TestProject, image_meta, player_image_meta, scene_id, sprite_value, valid_png_bytes,
};

const BOOT_OUTCOME_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-boot-outcome");
const BOOT_OUTCOME_REQUIREMENTS: &[PluginId] = &[GAMEPLAY_COMMAND_PLUGIN_ID];
const BOOT_OUTCOME_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(BOOT_OUTCOME_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(BOOT_OUTCOME_REQUIREMENTS);
const TILESET_PUBLICATION_PLUGIN_ID: PluginId =
    PluginId::new("nara.test.tileset-publication-outcome");
const TILESET_PUBLICATION_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TILESET_PUBLICATION_PLUGIN_ID, PluginCategory::Runtime);
const EXIT_AFTER_TICK_PLUGIN_ID: PluginId = PluginId::new("nara.test.exit-after-tick");
const EXIT_AFTER_TICK_REQUIREMENTS: &[PluginId] = &[BOOT_OUTCOME_PLUGIN_ID];
const EXIT_AFTER_TICK_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(EXIT_AFTER_TICK_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(EXIT_AFTER_TICK_REQUIREMENTS);
const STALLED_CLOSE_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-stalled-close");
const STALLED_CLOSE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-stalled-close", 1);
const STALLED_CLOSE_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.test.project-stalled-close");
const STALLED_CLOSE_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.test.project-stalled-close");
const STALLED_CLOSE_REQUIREMENTS: &[PluginId] = &[BOOT_OUTCOME_PLUGIN_ID];
const STALLED_CLOSE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(STALLED_CLOSE_PLUGIN_ID, PluginCategory::Service)
        .requires_plugins(STALLED_CLOSE_REQUIREMENTS)
        .shutdown_obligations(&[STALLED_CLOSE_OBLIGATION]);
const FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-domain-failure");
const FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(FAILURE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[BOOT_OUTCOME_PLUGIN_ID]);
const PRIVATE_PLUGIN_CANARY: &str = "qvM7k2p8Nx4J";
const PRIVATE_PLUGIN_PATH: &str = r"C:\Users\PrivateOwner\Documents\private-runtime-state.json";
const FORGED_PLUGIN_ID: PluginId = PluginId::new("nara.test.forged-plugin");

#[derive(Debug, Clone)]
struct FailureConfiguration {
    hook: PluginHook,
    legacy: bool,
    shutdowns: Arc<AtomicUsize>,
}

impl ProductConfiguration for FailureConfiguration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"project-domain-failure-v1");
        output.push(match self.hook {
            PluginHook::Preflight => 0,
            PluginHook::Build => 1,
            PluginHook::Finish => 2,
            PluginHook::Shutdown => 3,
        });
        output.push(u8::from(self.legacy));
    }
}

#[derive(Debug)]
struct FailurePlugin(FailureConfiguration);

impl FailurePlugin {
    fn fail_at(&self, hook: PluginHook) -> Result<(), PluginError> {
        // A build failure followed by a distinct shutdown failure must retain both facts.
        if self.0.hook != hook
            && !(self.0.hook == PluginHook::Build && hook == PluginHook::Shutdown)
        {
            return Ok(());
        }
        if self.0.legacy {
            return Err(PluginError::SetupFailed {
                plugin: FORGED_PLUGIN_ID,
                message: format!("{PRIVATE_PLUGIN_CANARY}: {PRIVATE_PLUGIN_PATH}"),
            });
        }
        Err(PluginDomainError::new(
            domain_code(hook),
            "Project plugin cannot complete this operation",
        )
        .unwrap()
        .into())
    }
}

impl Plugin for FailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &FAILURE_DECLARATION
    }

    fn preflight(&self, _context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        self.fail_at(PluginHook::Preflight)
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        self.fail_at(PluginHook::Build)
    }

    fn finish(&self, _app: &mut App) -> Result<(), PluginError> {
        self.fail_at(PluginHook::Finish)
    }

    fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        self.0.shutdowns.fetch_add(1, Ordering::SeqCst);
        self.fail_at(PluginHook::Shutdown)
    }
}

fn domain_code(hook: PluginHook) -> &'static str {
    match hook {
        PluginHook::Preflight => "demo.project.preflight-unavailable",
        PluginHook::Build => "demo.project.build-unavailable",
        PluginHook::Finish => "demo.project.finish-unavailable",
        PluginHook::Shutdown => "demo.project.shutdown-unavailable",
    }
}

fn assert_private_plugin_details_are_absent(report: &DiagnosticReport) {
    for published in [
        format!("{report:#?}"),
        serde_json::to_string(report).unwrap(),
    ] {
        for private in [
            PRIVATE_PLUGIN_CANARY,
            PRIVATE_PLUGIN_PATH,
            "PrivateOwner",
            "private-runtime-state.json",
            FORGED_PLUGIN_ID.as_str(),
        ] {
            assert!(
                !published.contains(private),
                "private plugin detail was published"
            );
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Resource)]
struct BootOutcome {
    tick: u64,
    scene_entities: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Resource)]
struct TileSetPublicationOutcome {
    tileset_count: usize,
    image_count: usize,
    resolved_tilemap_count: usize,
    coherent_tileset_count: usize,
    #[cfg(feature = "presentation-2d")]
    rendered_tile_cells: u32,
    #[cfg(feature = "presentation-2d")]
    prepared_pixels: Vec<u8>,
}

#[derive(Debug, Default)]
struct BootOutcomePlugin;

impl Plugin for BootOutcomePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &BOOT_OUTCOME_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(BootOutcome::default())?.add_systems(
            CoreStage::FixedUpdate,
            capture_boot_outcome
                .in_set(FixedUpdateSet::Finalize)
                .after(GameplayCommandSet::Capture),
        )?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct TileSetPublicationPlugin;

impl Plugin for TileSetPublicationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TILESET_PUBLICATION_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(TileSetPublicationOutcome::default())?
            .add_systems(
                CoreStage::FixedUpdate,
                capture_tileset_publication
                    .in_set(FixedUpdateSet::Finalize)
                    .after(GameplayCommandSet::Capture),
            )?;
        #[cfg(feature = "presentation-2d")]
        app.add_systems(CoreStage::Last, capture_rendered_tiles)?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct ExitAfterTickPlugin;

impl Plugin for ExitAfterTickPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &EXIT_AFTER_TICK_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::FixedUpdate,
            request_exit_after_tick.in_set(FixedUpdateSet::Finalize),
        )?;
        Ok(())
    }
}

#[derive(Debug)]
struct StalledClosePlugin {
    released: Arc<AtomicBool>,
}

impl Plugin for StalledClosePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &STALLED_CLOSE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.register_plugin_runtime_close_participant(
            STALLED_CLOSE_OBLIGATION,
            STALLED_CLOSE_PARTICIPANT,
            StalledCloseParticipant {
                released: Arc::clone(&self.released),
            },
        )?;
        Ok(())
    }
}

struct StalledCloseParticipant {
    released: Arc<AtomicBool>,
}

impl RuntimeCloseParticipant for StalledCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.progress())
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.progress())
    }
}

impl StalledCloseParticipant {
    fn progress(&self) -> RuntimeCloseProgress {
        if self.released.load(Ordering::SeqCst) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        }
    }
}

fn capture_boot_outcome(
    fixed_time: Res<FixedTime>,
    scene_entities: Query<&SceneEntitySource>,
    mut outcome: ResMut<BootOutcome>,
) {
    outcome.tick = fixed_time.tick();
    outcome.scene_entities = scene_entities
        .iter()
        .map(|source| source.provenance().local_entity_id().as_str().to_owned())
        .collect();
    outcome.scene_entities.sort();
}

fn capture_tileset_publication(
    tilesets: Res<nara::asset::Assets<nara::tilemap::TileSet>>,
    images: Res<nara::asset::Assets<nara::image::ImageAsset>>,
    tilemaps: Query<&nara::tilemap::Tilemap>,
    mut outcome: ResMut<TileSetPublicationOutcome>,
) {
    outcome.tileset_count = tilesets.iter().count();
    outcome.image_count = images.iter().count();
    outcome.coherent_tileset_count = tilesets
        .iter()
        .filter(|(_, tileset)| {
            tileset.image_content().is_some_and(|image| {
                tileset.material().image == Some(image.handle())
                    && tileset.atlas().is_some_and(|atlas| {
                        atlas.atlas_size()
                            == nara::core::Vec2::new(
                                image.extent().width as f32,
                                image.extent().height as f32,
                            )
                    })
            })
        })
        .count();
    outcome.resolved_tilemap_count = tilemaps
        .iter()
        .filter_map(|tilemap| tilemap.tileset)
        .filter_map(|handle| tilesets.get(handle))
        .filter_map(|tileset| tileset.material().image)
        .filter(|handle| images.get(*handle).is_some())
        .count();
}

#[cfg(feature = "presentation-2d")]
fn capture_rendered_tiles(
    batches: Res<nara::sprite_render::__private::SpriteBatches>,
    stats: Res<nara::sprite_render::SpriteRenderStats>,
    mut outcome: ResMut<TileSetPublicationOutcome>,
) {
    outcome.rendered_tile_cells = stats.extracted_tile_cells;
    outcome.prepared_pixels = batches
        .as_slice()
        .iter()
        .filter(|batch| !batch.instances.is_empty())
        .filter_map(|batch| batch.image.as_ref())
        .next()
        .map_or_else(Vec::new, |image| image.resource().pixels().to_vec());
}

fn request_exit_after_tick(mut exit: ResMut<AppExitRequests>) {
    exit.request_exit();
}

fn stalled_close_plugin(
    released: &Arc<AtomicBool>,
    instances: &Arc<AtomicUsize>,
) -> PluginDefinition {
    let released = Arc::clone(released);
    let instances = Arc::clone(instances);
    PluginDefinition::infallible::<StalledClosePlugin, _>(
        STALLED_CLOSE_DEFINITION_ID,
        b"project-stalled-close-v1",
        move || {
            instances.fetch_add(1, Ordering::SeqCst);
            StalledClosePlugin {
                released: Arc::clone(&released),
            }
        },
    )
}

fn project_run(project: &TestProject) -> HeadlessRun<BootOutcome> {
    HeadlessRun::from_recipe(
        project.root_capability(),
        boot_recipe(),
        NonZeroU32::new(1).unwrap(),
        Vec::new(),
    )
}

fn boot_recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_plugin::<BootOutcomePlugin>()
        .expect("the boot recipe is statically valid")
}

fn tileset_probe_recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_plugin::<TileSetPublicationPlugin>()
        .expect("the TileSet publication probe recipe is statically valid")
}

fn execute_to_terminal<O>(run: &mut HeadlessRun<O>) -> HeadlessRunReport<O>
where
    O: Resource + Clone,
{
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let report = run.execute_bounded();
        if !matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
            return report;
        }
        assert!(Instant::now() < deadline, "product cleanup did not finish");
        std::thread::yield_now();
    }
}

#[test]
fn recipe_lifecycle_domain_failures_preserve_public_facts_through_terminal_cleanup() {
    for hook in [
        PluginHook::Preflight,
        PluginHook::Build,
        PluginHook::Finish,
        PluginHook::Shutdown,
    ] {
        let project = TestProject::with_prefab_startup();
        project.select_local_headless_profile();
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let recipe = boot_recipe()
            .add_configured_plugin(
                FailureConfiguration {
                    hook,
                    legacy: false,
                    shutdowns: Arc::clone(&shutdowns),
                },
                |configuration: &FailureConfiguration| FailurePlugin(configuration.clone()),
            )
            .unwrap();
        let mut run: HeadlessRun<BootOutcome> = HeadlessRun::from_recipe(
            project.root_capability(),
            recipe,
            NonZeroU32::new(1).unwrap(),
            Vec::new(),
        );

        let report = execute_to_terminal(&mut run);

        assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed, "{report:#?}");
        let diagnostics = report.diagnostics();
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code().as_str() == domain_code(hook))
            .unwrap_or_else(|| panic!("missing {hook} domain failure: {report:#?}"));
        assert_eq!(
            diagnostic.summary().as_str(),
            "Project plugin cannot complete this operation"
        );
        let context = if hook == PluginHook::Shutdown {
            "project.run.cleanup-plugin-shutdown"
        } else {
            "project.run.plugin-hook-failed"
        };
        let hook_name = hook.to_string();
        for (key, value) in [
            ("plugin", FAILURE_PLUGIN_ID.as_str()),
            ("hook", hook_name.as_str()),
            ("project-context-code", context),
        ] {
            assert!(
                diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == key
                        && field.value() == DiagnosticValueRef::Identifier(value)
                }),
                "missing {key}={value} in {diagnostic:#?}"
            );
        }
        assert_private_plugin_details_are_absent(diagnostics);
        let completed_shutdowns = shutdowns.load(Ordering::SeqCst);
        if hook == PluginHook::Build {
            assert!(diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == domain_code(PluginHook::Shutdown)
                    && diagnostic.fields().iter().any(|field| {
                        field.key().as_str() == "hook"
                            && field.value() == DiagnosticValueRef::Identifier("shutdown")
                    })
            }));
        }
        if hook != PluginHook::Preflight {
            assert_eq!(completed_shutdowns, 1);
        } else {
            assert_eq!(completed_shutdowns, 0);
        }

        let repeated = run.execute_bounded();
        assert_eq!(repeated.outcome(), report.outcome());
        assert_eq!(repeated.diagnostics(), report.diagnostics());
        assert_eq!(shutdowns.load(Ordering::SeqCst), completed_shutdowns);
    }
}

#[test]
fn recipe_legacy_failure_publishes_actual_hook_identity_without_dynamic_error_text() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let recipe = boot_recipe()
        .add_configured_plugin(
            FailureConfiguration {
                hook: PluginHook::Build,
                legacy: true,
                shutdowns: Arc::new(AtomicUsize::new(0)),
            },
            |configuration: &FailureConfiguration| FailurePlugin(configuration.clone()),
        )
        .unwrap();
    let mut run: HeadlessRun<BootOutcome> = HeadlessRun::from_recipe(
        project.root_capability(),
        recipe,
        NonZeroU32::new(1).unwrap(),
        Vec::new(),
    );

    let report = execute_to_terminal(&mut run);

    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed, "{report:#?}");
    let diagnostic = report
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "project.run.plugin-hook-failed")
        .unwrap();
    for (key, value) in [("plugin", FAILURE_PLUGIN_ID.as_str()), ("hook", "build")] {
        assert!(
            diagnostic.fields().iter().any(|field| {
                field.key().as_str() == key
                    && field.value() == DiagnosticValueRef::Identifier(value)
            }),
            "missing {key}={value} in {diagnostic:#?}"
        );
    }
    assert_private_plugin_details_are_absent(report.diagnostics());
}

#[test]
fn ordinary_recipe_headless_facade_owns_project_start_and_close() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let recipe = boot_recipe();
    let mut run = HeadlessRun::from_recipe(
        project.root_capability(),
        recipe,
        NonZeroU32::new(1).unwrap(),
        Vec::new(),
    );

    let report = execute_to_terminal(&mut run);

    assert_eq!(
        report.outcome(),
        &HeadlessRunOutcome::Completed(BootOutcome {
            tick: 1,
            scene_entities: vec!["enemy".to_owned(), "enemy-anchor".to_owned()],
        })
    );
}

#[test]
fn headless_run_materializes_a_stable_image_reference_after_source_rename() {
    use nara::{
        asset::AssetRef,
        reflect::{ComponentSchemaVersion, ComponentTypeId},
        scene::{PrefabDocument, SceneComponentRecord, SceneEntityRecord},
    };

    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let identity = player_image_meta().identity().stable_id();
    project.write_prefab_source(&PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("enemy")).with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                sprite_value(&AssetRef::StableId(identity)),
            ),
        ),
    ]));
    let mut before_run = project_run(&project);
    let before = execute_to_terminal(&mut before_run);
    assert!(
        matches!(before.outcome(), HeadlessRunOutcome::Completed(_)),
        "{before:#?}"
    );
    drop(before_run);

    std::fs::rename(
        project.path().join("assets/textures/player.png"),
        project.path().join("assets/textures/renamed.png"),
    )
    .unwrap();
    std::fs::rename(
        project.path().join("assets/textures/player.png.meta"),
        project.path().join("assets/textures/renamed.png.meta"),
    )
    .unwrap();
    std::fs::write(
        project.path().join("assets/textures/renamed.png.meta"),
        image_meta(&identity.to_string(), "textures/renamed.png")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut after_run = project_run(&project);
    let after = execute_to_terminal(&mut after_run);

    assert_eq!(after.outcome(), before.outcome(), "{after:#?}");
    assert!(!after.diagnostics().has_errors());
    let closure = after.startup_content_closure().unwrap();
    assert!(
        closure
            .members()
            .iter()
            .any(|member| member.path().as_str() == "assets/textures/renamed.png")
    );
}

#[test]
fn headless_run_publishes_file_backed_tilesets_after_image_dependencies() {
    use nara::{
        asset::AssetRef,
        reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue},
        scene::{SceneComponentRecord, SceneDocument, SceneEntityRecord},
        tilemap::{TileAtlasLayout, TileSetSource},
    };

    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    #[cfg(feature = "presentation-2d")]
    {
        let manifest_path = project.path().join("nara.toml");
        let manifest = std::fs::read_to_string(&manifest_path).unwrap();
        std::fs::write(
            manifest_path,
            manifest.replace("[\"runtime-2d\"]", "[\"runtime-2d\", \"presentation-2d\"]"),
        )
        .unwrap();
    }
    let tileset_id = "d3d3f9a6-f7a9-4db0-ae3c-e1d4264d6f12";
    let tilemap = SceneEntityRecord::new(scene_id("terrain"))
        .with_component(
            ComponentTypeId::new("nara.tilemap.Tilemap"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    (
                        "tile_size",
                        ComponentValue::map([
                            ("x", ComponentValue::f64(1.0).unwrap()),
                            ("y", ComponentValue::f64(1.0).unwrap()),
                        ]),
                    ),
                    (
                        "tileset",
                        ComponentValue::map([
                            ("kind", ComponentValue::String("path".to_owned())),
                            (
                                "value",
                                ComponentValue::String("tilesets/terrain.tileset.ron".to_owned()),
                            ),
                        ]),
                    ),
                    (
                        "cells",
                        ComponentValue::List(vec![ComponentValue::map([
                            (
                                "coord",
                                ComponentValue::map([
                                    ("x", ComponentValue::I64(0)),
                                    ("y", ComponentValue::I64(0)),
                                ]),
                            ),
                            (
                                "cell",
                                ComponentValue::map([
                                    ("tile", ComponentValue::U64(0)),
                                    (
                                        "color",
                                        ComponentValue::map(["r", "g", "b", "a"].map(|channel| {
                                            (channel, ComponentValue::f64(1.0).unwrap())
                                        })),
                                    ),
                                ]),
                            ),
                        ])]),
                    ),
                ]),
            ),
        )
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            project_content_fixture::identity_transform_record(),
        );
    let mut other_tilemap = tilemap.clone();
    other_tilemap.id = scene_id("other-terrain");
    let ComponentValue::Map(fields) = &mut other_tilemap
        .components
        .get_mut(&ComponentTypeId::new("nara.tilemap.Tilemap"))
        .unwrap()
        .value
    else {
        panic!("Tilemap fixture must have a map payload");
    };
    fields.insert(
        "tileset".to_owned(),
        ComponentValue::map([
            ("kind", ComponentValue::String("path".to_owned())),
            (
                "value",
                ComponentValue::String("tilesets/z-terrain.tileset.ron".to_owned()),
            ),
        ]),
    );
    let mut scene = SceneDocument::from_ordered_roots([tilemap, other_tilemap]);
    #[cfg(feature = "presentation-2d")]
    {
        let camera = SceneEntityRecord::new(scene_id("map-camera"))
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                project_content_fixture::identity_transform_record(),
            )
            .with_component(
                ComponentTypeId::new("nara.render.Camera2d"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([
                        ("viewport_height", ComponentValue::f64(16.0).unwrap()),
                        (
                            "viewport",
                            ComponentValue::map([
                                ("physical_x", ComponentValue::U64(0)),
                                ("physical_y", ComponentValue::U64(0)),
                                ("physical_width", ComponentValue::U64(16)),
                                ("physical_height", ComponentValue::U64(16)),
                            ]),
                        ),
                    ]),
                ),
            );
        scene = SceneDocument::from_ordered_roots(scene.entities.into_iter().chain([camera]));
    }
    project.write_scene_source(&scene);
    project.write_tileset_source(
        "tilesets/terrain.tileset.ron",
        &TileSetSource::new(
            AssetRef::path("textures/player.png").unwrap(),
            TileAtlasLayout::grid(nara::core::Vec2::new(1.0, 1.0), 1, 1),
        ),
        &project_content_fixture::tileset_meta(tileset_id, "tilesets/terrain.tileset.ron"),
    );
    project.write_tileset_source(
        "tilesets/z-terrain.tileset.ron",
        &TileSetSource::new(
            AssetRef::StableId(player_image_meta().identity().stable_id()),
            TileAtlasLayout::grid(nara::core::Vec2::ONE, 1, 1),
        ),
        &project_content_fixture::tileset_meta(
            "f3d3f9a6-f7a9-4db0-ae3c-e1d4264d6f13",
            "tilesets/z-terrain.tileset.ron",
        ),
    );

    let mut run: HeadlessRun<TileSetPublicationOutcome> = HeadlessRun::from_recipe(
        project.root_capability(),
        tileset_probe_recipe(),
        NonZeroU32::new(1).unwrap(),
        Vec::new(),
    );
    let report = execute_to_terminal(&mut run);

    assert_eq!(
        report.outcome(),
        &HeadlessRunOutcome::Completed(TileSetPublicationOutcome {
            tileset_count: 2,
            image_count: 1,
            resolved_tilemap_count: 2,
            coherent_tileset_count: 2,
            #[cfg(feature = "presentation-2d")]
            rendered_tile_cells: 2,
            #[cfg(feature = "presentation-2d")]
            prepared_pixels: vec![24, 120, 220, 255],
        })
    );
    drop(run);

    #[cfg(feature = "tooling")]
    {
        use nara::project_host::{
            EditorProjectCloseProgress, EditorProjectSession, TrustedCompiledEditorProjectIntent,
        };

        let mut editor = EditorProjectSession::open_trusted_compiled(
            project.root_capability(),
            TrustedCompiledEditorProjectIntent::new(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !matches!(
            editor.advance_close(),
            EditorProjectCloseProgress::Complete(_)
        ) {
            assert!(
                Instant::now() < deadline,
                "Editor TileSet runtime must retire"
            );
            std::thread::yield_now();
        }
    }
}

#[test]
fn product_action_boots_authorized_content_runs_one_tick_and_closes() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut run = project_run(&project);

    let first = execute_to_terminal(&mut run);
    assert_eq!(
        first.outcome(),
        &HeadlessRunOutcome::Completed(BootOutcome {
            tick: 1,
            scene_entities: vec!["enemy".to_owned(), "enemy-anchor".to_owned()],
        }),
        "{first:?}"
    );
    assert!(!first.diagnostics().has_errors());
    let first_source = first
        .startup_content_closure()
        .expect("successful project check publishes startup content evidence");
    assert!(
        first_source
            .members()
            .iter()
            .any(|member| member.path().as_str() == "nara.toml")
    );

    let terminal_repeat = run.execute_bounded();
    assert_eq!(terminal_repeat.outcome(), first.outcome());
    assert_eq!(terminal_repeat.diagnostics(), first.diagnostics());
    let repeated_source = terminal_repeat
        .startup_content_closure()
        .expect("terminal reports retain startup content evidence");
    assert_eq!(
        repeated_source.manifest_digest(),
        first_source.manifest_digest()
    );
    assert_eq!(
        repeated_source.content_digest(),
        first_source.content_digest()
    );
    assert_eq!(
        repeated_source.schema_fingerprint(),
        first_source.schema_fingerprint()
    );
    assert_eq!(repeated_source.members(), first_source.members());
}

#[test]
fn unreferenced_import_preserves_the_headless_startup_closure() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut baseline_run = project_run(&project);
    let baseline = execute_to_terminal(&mut baseline_run);
    let baseline_closure = baseline
        .startup_content_closure()
        .expect("the baseline Headless Run must publish its startup closure");
    let baseline_content = baseline_closure.content_digest();
    let baseline_plan = baseline_closure.closure_plan_digest();
    let baseline_members = baseline_closure.members().to_vec();

    project.write_asset(
        "textures/unreferenced.png",
        valid_png_bytes(),
        &image_meta(
            "92fca27e-d243-46bd-9cb4-6506f9f888de",
            "textures/unreferenced.png",
        ),
    );

    let mut successor_run = project_run(&project);
    let successor = execute_to_terminal(&mut successor_run);
    let successor_closure = successor
        .startup_content_closure()
        .expect("the successor Headless Run must publish its startup closure");

    assert_eq!(successor_closure.content_digest(), baseline_content);
    assert_eq!(successor_closure.closure_plan_digest(), baseline_plan);
    assert_eq!(successor_closure.members(), baseline_members);
    assert!(successor_closure.members().iter().all(|member| {
        !member
            .path()
            .as_str()
            .starts_with("assets/textures/unreferenced.png")
    }));
}

#[test]
fn project_content_failure_returns_without_a_runtime_outcome() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    project.write_scene_bytes(b"{");
    let mut run = project_run(&project);

    let report = run.execute_bounded();

    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(report.diagnostics().has_errors());
    assert!(report.startup_content_closure().is_none());
}

#[test]
fn incomplete_cleanup_retries_the_same_owner_without_restarting_the_project() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let released = Arc::new(AtomicBool::new(false));
    let instances = Arc::new(AtomicUsize::new(0));
    let intent = HeadlessRunIntent::new(NonZeroU32::new(1).unwrap())
        .with_cleanup_timeout(Duration::ZERO)
        .with_recipe(boot_recipe());
    let mut run = headless_run_with_probe(
        project.root_capability(),
        intent,
        RuntimeProbe::new(stalled_close_plugin(&released, &instances)),
        Vec::new(),
    );

    let incomplete = run.execute_bounded();
    assert_eq!(incomplete.outcome(), &HeadlessRunOutcome::CleanupIncomplete);
    let incomplete_source = incomplete
        .startup_content_closure()
        .expect("cleanup retains the already-admitted startup closure");
    assert_eq!(instances.load(Ordering::SeqCst), 1);

    released.store(true, Ordering::SeqCst);
    let completed = execute_to_terminal(&mut run);
    assert_eq!(
        completed.outcome(),
        &HeadlessRunOutcome::Completed(BootOutcome {
            tick: 1,
            scene_entities: vec!["enemy".to_owned(), "enemy-anchor".to_owned()],
        })
    );
    assert_eq!(
        completed
            .startup_content_closure()
            .unwrap()
            .content_digest(),
        incomplete_source.content_digest()
    );
    assert_eq!(instances.load(Ordering::SeqCst), 1);
}

#[test]
fn product_action_stops_on_the_first_terminal_snapshot() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let intent = HeadlessRunIntent::new(NonZeroU32::new(4).unwrap())
        .stop_when(|outcome: &BootOutcome| outcome.tick >= 2)
        .with_recipe(boot_recipe());
    let mut run = HeadlessRun::new(project.root_capability(), intent, Vec::new());

    let report = execute_to_terminal(&mut run);

    let HeadlessRunOutcome::Completed(outcome) = report.outcome() else {
        panic!("terminal project action failed: {report:#?}");
    };
    assert_eq!(outcome.tick, 2);
    assert!(!report.diagnostics().has_errors());
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Resource)]
struct FrameStages(Vec<&'static str>);

#[derive(Debug, Default)]
struct FrameStagesPlugin;

impl Plugin for FrameStagesPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.headless-frame-stages"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(FrameStages::default())?
            .add_systems(CoreStage::TaskUpdate, |mut stages: ResMut<FrameStages>| {
                stages.0.push("task")
            })?
            .add_systems(CoreStage::PreUpdate, |mut stages: ResMut<FrameStages>| {
                stages.0.push("pre")
            })?
            .add_systems(CoreStage::FixedUpdate, |mut stages: ResMut<FrameStages>| {
                stages.0.push("fixed")
            })?
            .add_systems(CoreStage::Update, |mut stages: ResMut<FrameStages>| {
                stages.0.push("update")
            })?
            .add_systems(CoreStage::Last, |mut stages: ResMut<FrameStages>| {
                stages.0.push("last")
            })?;
        Ok(())
    }
}

#[test]
fn product_action_distinguishes_complete_frames_from_exact_fixed_ticks() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    for (intent, expected) in [
        (
            HeadlessRunIntent::new(NonZeroU32::new(2).unwrap()),
            vec!["task", "last", "fixed", "fixed"],
        ),
        (
            HeadlessRunIntent::frames(NonZeroU32::new(2).unwrap(), Duration::ZERO),
            vec![
                "task", "pre", "update", "last", "task", "pre", "update", "last",
            ],
        ),
        (
            HeadlessRunIntent::frames(NonZeroU32::new(1).unwrap(), Duration::from_millis(20)),
            vec!["task", "pre", "fixed", "update", "last"],
        ),
    ] {
        let recipe = ProductRecipe::new()
            .add_plugin::<FrameStagesPlugin>()
            .unwrap();
        let mut run = HeadlessRun::<FrameStages>::new(
            project.root_capability(),
            intent.with_recipe(recipe),
            Vec::new(),
        );
        let report = execute_to_terminal(&mut run);
        assert_eq!(
            report.outcome(),
            &HeadlessRunOutcome::Completed(FrameStages(expected)),
            "{report:#?}"
        );
    }
}

#[test]
fn product_frame_action_observes_the_complete_terminal_frame() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let recipe = ProductRecipe::new()
        .add_plugin::<FrameStagesPlugin>()
        .unwrap();
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(4).unwrap(), Duration::ZERO)
        .with_recipe(recipe)
        .stop_when(|stages: &FrameStages| stages.0.len() >= 4);
    let mut run = HeadlessRun::new(project.root_capability(), intent, Vec::new());
    let report = execute_to_terminal(&mut run);
    assert_eq!(
        report.outcome(),
        &HeadlessRunOutcome::Completed(FrameStages(vec!["task", "pre", "update", "last"]))
    );
}

#[test]
fn product_frame_action_preserves_zero_time_scale_while_advancing_frame_services() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let path = project.path().join("nara.toml");
    let manifest = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        manifest.replace("[runtime]", "[runtime]\ntime_scale = 0.0"),
    )
    .unwrap();
    let recipe = ProductRecipe::new()
        .add_plugin::<FrameStagesPlugin>()
        .unwrap();
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(1).unwrap(), Duration::from_secs(1))
        .with_recipe(recipe);
    let mut run = HeadlessRun::new(project.root_capability(), intent, Vec::new());
    let report = execute_to_terminal(&mut run);
    assert_eq!(
        report.outcome(),
        &HeadlessRunOutcome::Completed(FrameStages(vec!["task", "pre", "update", "last"]))
    );
}

#[test]
fn product_frame_action_reports_its_own_bound_without_a_terminal_match() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(2).unwrap(), Duration::ZERO)
        .with_recipe(boot_recipe())
        .stop_when(|_: &BootOutcome| false);
    let mut run = HeadlessRun::new(project.root_capability(), intent, Vec::new());
    let report = execute_to_terminal(&mut run);
    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "project.run.frame-limit")
    );
}

#[test]
fn product_action_propagates_an_app_exit_from_the_completed_fixed_tick() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let recipe = boot_recipe()
        .add_plugin::<ExitAfterTickPlugin>()
        .expect("the exit recipe is statically valid");
    let intent = HeadlessRunIntent::new(NonZeroU32::new(4).unwrap())
        .stop_when(|_: &BootOutcome| false)
        .with_recipe(recipe);
    let mut run = HeadlessRun::new(project.root_capability(), intent, Vec::new());

    let report = execute_to_terminal(&mut run);

    assert_eq!(
        report.outcome(),
        &HeadlessRunOutcome::Completed(BootOutcome {
            tick: 1,
            scene_entities: vec!["enemy".to_owned(), "enemy-anchor".to_owned()],
        }),
        "{report:?}"
    );
    assert!(!report.diagnostics().has_errors());
}

#[test]
fn product_action_reports_a_tick_limit_before_publishing_success() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let intent = HeadlessRunIntent::new(NonZeroU32::new(2).unwrap())
        .stop_when(|_: &BootOutcome| false)
        .with_recipe(boot_recipe());
    let mut run = HeadlessRun::new(project.root_capability(), intent, Vec::new());

    let report = execute_to_terminal(&mut run);

    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(report.startup_content_closure().is_some());
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.run.tick-limit"
            && diagnostic.summary().as_str() == "Headless project run reached its fixed-tick limit"
    }));
}

#[test]
fn source_label_cannot_downgrade_a_trusted_product_command_rejection() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let commands = (1..=4_097)
        .map(|sequence| {
            GameplayCommandSubmission::new(
                GameplayCommandTick::new(1).unwrap(),
                GameplayCommandIngressSource::external("nara.test.external").unwrap(),
                GameplayCommandSourceSequence::new(sequence).unwrap(),
                GameplayCommandDraft::new(
                    GameplayCommandTypeId::new("nara.test.external-command").unwrap(),
                ),
            )
        })
        .collect::<Vec<_>>();
    let mut run: HeadlessRun<BootOutcome> = HeadlessRun::from_recipe(
        project.root_capability(),
        boot_recipe(),
        NonZeroU32::new(1).unwrap(),
        commands,
    );

    let report = execute_to_terminal(&mut run);

    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(report.startup_content_closure().is_some());
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "project.run.command-rejected" })
    );
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "project.run.runtime-faulted" })
    );
}
