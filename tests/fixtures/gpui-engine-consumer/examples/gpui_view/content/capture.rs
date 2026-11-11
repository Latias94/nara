//! Read-only observation between stock Seal and Publish, with managed Runtime retirement.

use std::{
    num::NonZeroU32,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use nara_engine::{
    ProductRecipe,
    advanced_prelude::{CurrentScene, SceneSourceView},
    app::{
        App, CoreStage, FrameExecutionStart, Plugin, PluginCategory, PluginDeclaration,
        PluginError, PluginId, RuntimeGeneration,
    },
    asset::Assets,
    core::Color,
    ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs},
    image::{
        ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, PreparedImageSnapshot,
        PreparedImages, image_content_hash, image_resource_key,
    },
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOptions, HeadlessRunOutcome,
        open_explicit_product_project_root,
    },
    render::{RenderFrameTarget, ViewLoadOp},
    render_wgpu::{
        WGPU_CAPTURE_PLUGIN_ID, WGPU_STOCK_SPRITE_PLUGIN_ID, WgpuPolicyCaptureSet,
        WgpuPolicyFrameReader, WgpuRenderInput,
    },
    runtime_content::RuntimeContentBase,
    scene::{
        SceneEntityProvenanceMap, SourceDocumentKey, SourceDocumentKind, SourceDocumentOrigin,
    },
};

use super::{HEIGHT, TARGET_HEIGHT, TARGET_WIDTH, WIDTH, fixture::image_product, validate_pixels};

const CLEANUP_TIMEOUT: Duration = Duration::from_secs(15);
const DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("consumer.gpui-view.content-capture"),
    PluginCategory::Runtime,
)
.requires_plugins(&[WGPU_CAPTURE_PLUGIN_ID, WGPU_STOCK_SPRITE_PLUGIN_ID]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContentMode {
    Source,
    Processed,
}

#[derive(Clone)]
pub(super) struct CapturedFrame {
    pub(super) input: WgpuRenderInput,
    pub(super) image: PreparedImageSnapshot,
    pub(super) runtime: RuntimeGeneration,
    pub(super) source: SourceDocumentKey,
    pub(super) provenance: SceneEntityProvenanceMap,
    mode: ContentMode,
    source_view: SceneSourceView,
    pixel_address: usize,
}

impl CapturedFrame {
    pub(super) fn assert_retired(&self) -> Result<()> {
        ensure!(
            self.source_view.with_document(|_| ()).is_none(),
            "the original current-Scene authority must retire before independent replay"
        );
        ensure!(
            self.input.packet().source_generation().runtime_generation() == Some(self.runtime),
            "the escaped capture must retain its original Runtime identity"
        );
        validate_packet(&self.input)?;
        validate_prepared_image(&self.image)?;
        ensure!(
            self.image.resource().pixels().as_ptr().addr() == self.pixel_address,
            "the prepared Image must retain the original live pixel allocation after retirement"
        );
        Ok(())
    }
}

#[derive(Clone, Default, Resource)]
struct CaptureOutput {
    frame: Option<CapturedFrame>,
    failure: Option<String>,
}

#[derive(Default)]
pub(super) struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<CaptureOutput>()?;
        app.add_systems(
            CoreStage::Render,
            retain_stock_capture
                .after(WgpuPolicyCaptureSet::Seal)
                .before(WgpuPolicyCaptureSet::Publish),
        )?;
        Ok(())
    }
}

fn retain_stock_capture(
    reader: WgpuPolicyFrameReader,
    generation: Res<RuntimeGeneration>,
    frame_start: Res<FrameExecutionStart>,
    current: CurrentScene,
    images: Res<Assets<ImageAsset>>,
    prepared: Res<PreparedImages>,
    mut output: ResMut<CaptureOutput>,
) {
    if output.frame.is_some() || output.failure.is_some() {
        return;
    }
    let result = (|| -> Result<Option<CapturedFrame>> {
        let Some(input) = reader.read().map_err(|error| anyhow!("{error}"))? else {
            return Ok(None);
        };
        ensure!(
            frame_start.frame() > 0,
            "capture must belong to a complete managed frame"
        );
        ensure!(
            input.packet().source_generation().runtime_generation() == Some(*generation),
            "Seal must retain the actual running Runtime generation"
        );
        let source_view = current.source_view();
        let facts = source_view.with_compiled_scene(|scene, resolver| {
            ensure!(
                resolver.is_some(),
                "Source capture must keep its real admitted resolver"
            );
            Ok::<_, anyhow::Error>((
                ContentMode::Source,
                scene.source_key().clone(),
                scene.canonical_provenance().clone(),
            ))
        });
        let (mode, source, provenance) = match facts {
            Some(facts) => facts?,
            None => source_view
                .with_runtime_scene(|scene| {
                    (
                        ContentMode::Processed,
                        scene.source_key().clone(),
                        scene.canonical_provenance().clone(),
                    )
                })
                .context("Processed capture must come from the admitted cooked Scene")?,
        };
        ensure!(
            source.kind() == SourceDocumentKind::Scene,
            "the capture must retain a real Scene source"
        );
        let SourceDocumentOrigin::ProjectLocation { path, .. } = source.origin() else {
            bail!("the capture must preserve its admitted ProjectLocation Scene identity");
        };
        ensure!(
            path.as_str() == "scenes/startup.scene.json",
            "incorrect actual Scene source"
        );
        ensure!(
            provenance.len() == 2,
            "the capture must retain the Sprite and Camera provenance"
        );
        for (entity, origin) in &provenance {
            ensure!(
                origin.local_entity_id() == entity && origin.projection_path().is_none(),
                "incorrect stock Scene entity provenance"
            );
        }

        validate_packet(input)?;
        ensure!(
            images.len() == 1,
            "the actual Scene must own exactly one Image"
        );
        let (handle, image) = images
            .iter()
            .next()
            .context("the live Image must be published before Seal")?;
        ensure!(
            image.product() == Some(image_product()?),
            "incorrect live Image Product"
        );
        ensure!(
            image.source().is_some() == (mode == ContentMode::Source),
            "Image provenance must match the actual content mode"
        );
        if let Some(source) = image.source() {
            ensure!(
                source.path().as_str() == "assets/alpha.png",
                "Source must retain the actual imported PNG path"
            );
            ensure!(
                source.settings().color_space() == ImageColorSpace::Srgb,
                "Source must retain its sRGB import settings"
            );
        }
        validate_pixels(image.pixels())?;
        ensure!(
            image.share_retained().is_some(),
            "the live Image must retain its original acquisition reservation"
        );
        let snapshot = prepared
            .get(image_resource_key(handle))
            .context("the actual Image must have a prepared snapshot")?;
        validate_prepared_image(snapshot)?;
        ensure!(
            snapshot.identity().key() == image_resource_key(handle)
                && snapshot.identity().slot_revision() == &images.slot_revision(handle)
                && snapshot.identity().content_hash() == image_content_hash(image),
            "Seal must observe the actual live Image identity, slot and content hash"
        );
        ensure!(
            snapshot.resource().extent() == image.extent()
                && snapshot.resource().format() == image.format()
                && snapshot.resource().color_space() == image.color_space()
                && snapshot.resource().content_hash() == image.content_hash(),
            "the prepared snapshot must preserve the actual live Image facts"
        );
        ensure!(
            snapshot.resource().pixels().as_ptr() == image.pixels().as_ptr(),
            "the prepared snapshot must share the original reserved pixel allocation"
        );
        Ok(Some(CapturedFrame {
            input: input.clone(),
            image: snapshot.clone(),
            runtime: *generation,
            source,
            provenance,
            mode,
            source_view,
            pixel_address: image.pixels().as_ptr().addr(),
        }))
    })();
    match result {
        Ok(frame) => output.frame = frame,
        Err(error) => output.failure = Some(format!("{error:#}")),
    }
}

fn validate_packet(input: &WgpuRenderInput) -> Result<()> {
    ensure!(
        matches!(input.packet().target(), RenderFrameTarget::Window(_)),
        "the stock capture must target its logical Window"
    );
    let extent = input.packet().target().extent();
    ensure!(
        extent.width == TARGET_WIDTH && extent.height == TARGET_HEIGHT,
        "the logical capture target must be {TARGET_WIDTH}x{TARGET_HEIGHT}"
    );
    ensure!(
        input.packet().world_canvas().is_none(),
        "the capture must use its actual Window extent"
    );
    let [view] = input.packet().views() else {
        bail!("the stock capture must contain exactly one actual camera View");
    };
    ensure!(
        view.load == ViewLoadOp::Clear(Color::TRANSPARENT),
        "the camera must clear to transparent black"
    );
    ensure!(
        view.viewport.physical_x == 0
            && view.viewport.physical_y == 0
            && view.viewport.physical_width == TARGET_WIDTH
            && view.viewport.physical_height == TARGET_HEIGHT,
        "the camera viewport must fill the logical Window"
    );
    Ok(())
}

fn validate_prepared_image(image: &PreparedImageSnapshot) -> Result<()> {
    let resource = image.resource();
    ensure!(
        resource.extent() == ImageExtent::new(WIDTH, HEIGHT),
        "the prepared Image must preserve its source extent"
    );
    ensure!(
        resource.format() == ImageFormat::Rgba8 && resource.color_space() == ImageColorSpace::Srgb,
        "the prepared snapshot must retain the original sRGB RGBA8 Image"
    );
    ensure!(
        resource.has_retention_owner(),
        "the prepared Image must keep its original memory reservation"
    );
    validate_pixels(resource.pixels())?;
    Ok(())
}

fn execution() -> HeadlessRunOptions<CaptureOutput> {
    HeadlessRunOptions::frames(NonZeroU32::MIN, Duration::ZERO)
        .with_cleanup_timeout(Duration::from_secs(2))
}

pub(super) fn source(root: &Path, recipe: ProductRecipe) -> Result<CapturedFrame> {
    let run = HeadlessRun::new(
        open_explicit_product_project_root(root)?,
        HeadlessRunIntent::frames(NonZeroU32::MIN, Duration::ZERO)
            .with_recipe(recipe)
            .with_execution(execution()),
        Vec::new(),
    );
    finish(run, ContentMode::Source)
}

pub(super) fn processed(base: RuntimeContentBase) -> Result<CapturedFrame> {
    finish(
        HeadlessRun::from_runtime_content(base, execution(), Vec::new()),
        ContentMode::Processed,
    )
}

fn finish(mut run: HeadlessRun<CaptureOutput>, mode: ContentMode) -> Result<CapturedFrame> {
    let mut report = run.execute_bounded();
    let deadline = Instant::now() + CLEANUP_TIMEOUT;
    while matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
        ensure!(
            Instant::now() < deadline,
            "{mode:?} Runtime cleanup did not retire: {:?}",
            report.diagnostics()
        );
        std::thread::sleep(Duration::from_millis(1));
        report = run.execute_bounded();
    }
    drop(run);
    ensure!(
        !report.diagnostics().has_errors(),
        "{mode:?} Runtime failed: {:?}",
        report.diagnostics()
    );
    ensure!(
        report.startup_content_closure().is_some() == (mode == ContentMode::Source),
        "only Source may report a checked startup source closure"
    );
    let HeadlessRunOutcome::Completed(output) = report.into_outcome() else {
        bail!("{mode:?} Runtime did not complete");
    };
    if let Some(failure) = output.failure {
        bail!("{mode:?} stock capture failed: {failure}");
    }
    let frame = output
        .frame
        .with_context(|| format!("{mode:?} Runtime produced no sealed stock capture"))?;
    ensure!(
        frame.mode == mode,
        "{mode:?} Runtime captured the wrong actual content mode"
    );
    frame.assert_retired()?;
    Ok(frame)
}
