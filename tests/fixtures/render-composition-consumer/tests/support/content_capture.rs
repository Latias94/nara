//! Observe ordinary stock capture without taking its transport or presentation obligations.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use nara::{
    advanced_prelude::{CurrentScene, SceneProductTransactionLimits, SceneSourceView},
    app::{
        App, AppExit, CoreStage, FrameExecutionStart, Plugin, PluginCategory, PluginDeclaration,
        PluginError, PluginId, RuntimeGeneration,
    },
    asset::Assets,
    core::ItemLimit,
    ecs::{Res, ResMut, Resource, World, schedule::IntoScheduleConfigs},
    image::{ImageAsset, PreparedImages, image_resource_key},
    project_host::{
        DesktopPresentationControl, DesktopPresentationObservation, DesktopRunOptions,
        ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes,
        advanced::begin_prepared_project_scene_travel,
    },
    render::RenderFrameTarget,
    render_wgpu::{
        WGPU_CAPTURE_PLUGIN_ID, WGPU_STOCK_SPRITE_PLUGIN_ID, WgpuBackendState,
        WgpuPolicyCaptureSet, WgpuPolicyFrameReader, WgpuRenderInput, WgpuStockPolicy,
    },
    scene::{
        SceneEntityProvenanceMap, SourceDocumentKey, SourceDocumentKind, SourceDocumentOrigin,
        SpawnedSceneInstance,
    },
};

use super::content_fixture::{GREEN, RED, image_product, scene_product};

const PRESENT_TIMEOUT: Duration = Duration::from_secs(20);
const DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("consumer.render-composition.content-capture"),
    PluginCategory::Runtime,
)
.requires_plugins(&[WGPU_CAPTURE_PLUGIN_ID, WGPU_STOCK_SPRITE_PLUGIN_ID]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentMode {
    Source,
    Processed,
}

#[derive(Clone)]
pub struct CapturedFrame {
    pub input: WgpuRenderInput,
    pub runtime: RuntimeGeneration,
    pub app_frame: u64,
    pub mode: ContentMode,
    pub source: SourceDocumentKey,
    pub provenance: SceneEntityProvenanceMap,
    pub instance: SpawnedSceneInstance,
    pub source_view: SceneSourceView,
}

impl CapturedFrame {
    pub fn assert_retired(&self) {
        assert!(
            self.source_view.with_document(|_| ()).is_none(),
            "the original current-Scene authority must retire before independent replay"
        );
        assert_eq!(
            self.input.packet().source_generation().runtime_generation(),
            Some(self.runtime)
        );
        assert_stock_pixels(&self.input, self.color());
    }

    pub fn color(&self) -> [u8; 4] {
        match self.source.origin() {
            SourceDocumentOrigin::ProjectLocation { path, .. }
                if path.as_str() == "scenes/startup.scene.json" =>
            {
                RED
            }
            SourceDocumentOrigin::ProjectLocation { path, .. }
                if path.as_str() == "scenes/later.scene.json" =>
            {
                GREEN
            }
            origin => panic!("unexpected actual Scene source: {origin:?}"),
        }
    }
}

#[derive(Default, Resource)]
struct CaptureState {
    // At most two original inputs live in the real World. Red is replaced only until its first
    // successful native presentation; green is sealed once before its live stores are emptied.
    frames: [Option<CapturedFrame>; 2],
    red_presented: AtomicBool,
    stores_emptied: bool,
}

#[derive(Default, Resource)]
struct TravelState {
    request: Option<ProjectSceneRequestId>,
    submitted: bool,
    committed: bool,
}

#[derive(Default)]
pub struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<CaptureState>()?;
        app.init_resource::<TravelState>()?;
        app.add_systems(CoreStage::Update, travel_after_red_presentation)?;
        app.add_systems(
            CoreStage::Render,
            retain_stock_capture
                .after(WgpuPolicyCaptureSet::Seal)
                .before(WgpuPolicyCaptureSet::Publish),
        )?;
        Ok(())
    }
}

fn travel_after_red_presentation(world: &mut World) {
    if !world
        .resource::<CaptureState>()
        .red_presented
        .load(Ordering::Acquire)
        || world.resource::<TravelState>().committed
    {
        return;
    }
    let Some(request) = world.resource::<TravelState>().request else {
        // Both modes select the actual declared Product. Source resolves its real Scene sidecar;
        // Processed uses the original admitted base, without a Source fallback.
        let request = ProjectScenes::scope(world, |scenes| scenes.request(scene_product(true)))
            .expect("request the actual declared later Scene after red was presented");
        world.resource_mut::<TravelState>().request = Some(request);
        return;
    };
    match ProjectScenes::scope(world, |scenes| scenes.state(request)) {
        Some(ProjectSceneRequestState::Preparing | ProjectSceneRequestState::Retiring) => {}
        Some(ProjectSceneRequestState::Ready) => {
            assert!(!world.resource::<TravelState>().submitted);
            let mode = world_mode(world);
            ProjectScenes::scope(world, |scenes| {
                let prepared = scenes
                    .prepared(request)
                    .expect("borrow the original ready Scene");
                assert_eq!(prepared.selected_scene_product(), Some(scene_product(true)));
                match mode {
                    ContentMode::Source => assert_eq!(
                        prepared.selected_scene_path().unwrap().as_str(),
                        "later.scene.json",
                    ),
                    ContentMode::Processed => assert!(prepared.selected_scene_path().is_none()),
                }
            });
            begin_prepared_project_scene_travel(
                world,
                request,
                SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
                &[],
                |_| {},
            )
            .expect("queue the ordinary managed Scene commit with its native domain owners");
            world.resource_mut::<TravelState>().submitted = true;
        }
        Some(ProjectSceneRequestState::Committed) => {
            assert!(world.resource::<TravelState>().submitted);
            let red = world.resource::<CaptureState>().frames[0].as_ref().unwrap();
            let current = CurrentScene::get(world).unwrap();
            assert_ne!(current.receipt().instance_id(), red.instance.instance_id());
            assert_eq!(current.receipt().domain_id(), red.instance.domain_id());
            assert!(ProjectScenes::scope(world, |scenes| scenes.acknowledge(request)));
            world.resource_mut::<TravelState>().committed = true;
        }
        state => panic!("the real later Scene request did not commit: {state:?}"),
    }
}

fn world_mode(world: &World) -> ContentMode {
    let view = CurrentScene::get(world).unwrap().source_view();
    if view
        .with_compiled_scene(|_, resolver| {
            assert!(
                resolver.is_some(),
                "Source must retain its actual admitted resolver"
            );
        })
        .is_some()
    {
        ContentMode::Source
    } else {
        assert!(view.with_runtime_scene(|_| ()).is_some());
        ContentMode::Processed
    }
}

fn retain_stock_capture(
    reader: WgpuPolicyFrameReader,
    generation: Res<RuntimeGeneration>,
    frame_start: Res<FrameExecutionStart>,
    current: CurrentScene,
    travel: Res<TravelState>,
    mut state: ResMut<CaptureState>,
    mut images: ResMut<Assets<ImageAsset>>,
    mut prepared: ResMut<PreparedImages>,
) {
    let Some(input) = reader.read().expect("the original stock frame must seal") else {
        return;
    };
    let later = travel.committed;
    if state.stores_emptied || (!later && state.red_presented.load(Ordering::Acquire)) {
        return;
    }
    let source_view = current.source_view();
    let (mode, source, provenance) = if let Some(facts) =
        source_view.with_compiled_scene(|scene, resolver| {
            assert!(
                resolver.is_some(),
                "Source capture must retain its actual source resolver"
            );
            (
                ContentMode::Source,
                scene.source_key().clone(),
                scene.canonical_provenance().clone(),
            )
        }) {
        facts
    } else {
        source_view
            .with_runtime_scene(|scene| {
                (
                    ContentMode::Processed,
                    scene.source_key().clone(),
                    scene.canonical_provenance().clone(),
                )
            })
            .expect("Processed capture must come from the actual cooked Scene owner")
    };
    assert_eq!(source.kind(), SourceDocumentKind::Scene);
    let SourceDocumentOrigin::ProjectLocation { path, .. } = source.origin() else {
        panic!("the fixture must keep its admitted ProjectLocation identity");
    };
    assert_eq!(
        path.as_str(),
        if later {
            "scenes/later.scene.json"
        } else {
            "scenes/startup.scene.json"
        }
    );
    assert_eq!(provenance.len(), 2);
    for (entity, origin) in &provenance {
        assert_eq!(origin.local_entity_id(), entity);
        assert!(origin.projection_path().is_none());
    }
    assert_eq!(
        input.packet().source_generation().runtime_generation(),
        Some(*generation)
    );
    assert!(matches!(
        input.packet().target(),
        RenderFrameTarget::Window(_)
    ));
    assert_eq!(input.packet().views().len(), 1);
    assert!(input.packet().world_canvas().is_none());
    let color = if later { GREEN } else { RED };
    assert_stock_pixels(input, color);
    let captured_image = input
        .captured::<WgpuStockPolicy>()
        .unwrap()
        .sprite_batches()
        .unwrap()
        .as_slice()[0]
        .image
        .as_ref()
        .unwrap();
    let handles: Vec<_> = images
        .iter()
        .map(|(handle, image)| {
            assert_eq!(image.product(), Some(image_product(later)));
            assert_eq!(image.source().is_some(), mode == ContentMode::Source);
            assert_eq!(image.pixels(), color);
            let live = prepared
                .get(image_resource_key(handle))
                .expect("the actual prepared Image entry");
            assert_eq!(
                captured_image, live,
                "Seal retains the exact live prepared Image and its identity"
            );
            assert_eq!(live.resource().pixels(), color);
            assert!(live.resource().has_retention_owner());
            handle
        })
        .collect();
    assert_eq!(
        handles.len(),
        1,
        "the committed Scene owns its actual one-Image residency"
    );
    state.frames[usize::from(later)] = Some(CapturedFrame {
        input: input.clone(),
        runtime: *generation,
        app_frame: frame_start.frame(),
        mode,
        source,
        provenance,
        instance: current.receipt().clone(),
        source_view,
    });
    if later {
        assert!(state.red_presented.load(Ordering::Acquire));
        for handle in handles {
            assert!(images.remove(handle).is_some());
        }
        let keys: Vec<_> = prepared.keys().collect();
        assert!(!keys.is_empty());
        for key in keys {
            assert!(prepared.remove(key).is_some());
        }
        // Keep both installed resources and their store identities. Only their actual live
        // entries disappear, after Seal and before the ordinary native consumer executes.
        assert!(images.is_empty() && prepared.is_empty());
        assert_stock_pixels(&state.frames[1].as_ref().unwrap().input, GREEN);
        state.stores_emptied = true;
    }
}

/// Read exact retained output pixels using a Host-owned native copy and its original prefix.
pub fn assert_output_pixels(
    host: &mut nara::render_wgpu::WgpuRenderHost,
    output: &nara::render_wgpu::WgpuImageOutput,
    expected: [u8; 4],
) -> Result<usize, Box<dyn std::error::Error>> {
    use std::{io, sync::mpsc, thread};

    assert_eq!(output.format(), wgpu::TextureFormat::Rgba8Unorm);
    let extent = output.extent();
    let row_bytes = extent.width * 4;
    let padded_row =
        row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let (readback, prefix) = host.submit_image_use(
        std::slice::from_ref(output),
        |device, _queue, encoder, views| {
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("content-window-original-stock-readback"),
                size: u64::from(padded_row) * u64::from(extent.height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                views[0].texture().as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row),
                        rows_per_image: Some(extent.height),
                    },
                },
                wgpu::Extent3d {
                    width: extent.width,
                    height: extent.height,
                    depth_or_array_layers: 1,
                },
            );
            Ok(readback)
        },
    )?;
    let (sender, receiver) = mpsc::sync_channel(1);
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut mapped = false;
    loop {
        let complete = prefix.poll()?;
        host.with_image(output, |device, _, _| device.poll(wgpu::PollType::Poll))??;
        if !mapped {
            match receiver.try_recv() {
                Ok(result) => {
                    result?;
                    mapped = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(
                        io::Error::other("the stock readback map callback disappeared").into(),
                    );
                }
            }
        }
        if mapped && complete {
            break;
        }
        if Instant::now() >= deadline {
            return Err(io::Error::other("the stock native readback exceeded ten seconds").into());
        }
        thread::sleep(Duration::from_millis(1));
    }
    {
        let pixels = readback.get_mapped_range(..)?;
        assert_eq!(pixels.len(), padded_row as usize * extent.height as usize);
        for (y, row) in pixels.chunks_exact(padded_row as usize).enumerate() {
            for (x, pixel) in row[..row_bytes as usize].chunks_exact(4).enumerate() {
                assert_eq!(
                    pixel,
                    expected.as_slice(),
                    "exact retained stock pixel at ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    Ok(extent.width as usize * extent.height as usize)
}

pub fn assert_stock_pixels(input: &WgpuRenderInput, expected: [u8; 4]) {
    let stock = input
        .captured::<WgpuStockPolicy>()
        .expect("the actual selected stock policy");
    let batches = stock
        .sprite_batches()
        .expect("actual stock Sprite extraction");
    assert_eq!(batches.len(), 1);
    assert_eq!(batches.total_instances(), 1);
    let batch = &batches.as_slice()[0];
    let view = &input.packet().views()[batch.view_index];
    assert_eq!(batch.target, view.target);
    assert_eq!(batch.view_order, view.order);
    let image = batch
        .image
        .as_ref()
        .expect("the original captured Image snapshot");
    assert_eq!(image.resource().pixels(), expected);
    assert!(image.resource().has_retention_owner());
}

#[derive(Default)]
pub struct CaptureResult {
    pub frames: [Option<CapturedFrame>; 2],
    pub failure: Option<String>,
    pub backend_instance: Option<u64>,
    pub device_epoch: Option<u64>,
    pub stores_emptied_before_present: bool,
}

pub fn options(mode: ContentMode) -> (DesktopRunOptions, Rc<RefCell<CaptureResult>>) {
    let result = Rc::new(RefCell::new(CaptureResult::default()));
    let observed = Rc::clone(&result);
    let deadline = Instant::now() + PRESENT_TIMEOUT;
    let options = DesktopRunOptions::new().with_presentation_observer(move |observation| {
        if observation.now() >= deadline {
            observed.borrow_mut().failure = Some(format!(
                "{mode:?} did not present actual red and green within twenty seconds"
            ));
            return DesktopPresentationControl::Exit(AppExit::Requested);
        }
        observe_presented(observation, mode, &observed)
    });
    (options, result)
}

fn observe_presented(
    observation: DesktopPresentationObservation<'_>,
    mode: ContentMode,
    result: &RefCell<CaptureResult>,
) -> DesktopPresentationControl {
    let Some(receipt) = observation.presented_frame() else {
        return DesktopPresentationControl::Continue;
    };
    let world = observation
        .world()
        .expect("a presented frame has its original live World");
    let state = world.resource::<CaptureState>();
    let mut result = result.borrow_mut();
    let index = usize::from(result.frames[0].is_some());
    let Some(frame) = &state.frames[index] else {
        return DesktopPresentationControl::Continue;
    };
    if receipt.app_frame_index() != frame.app_frame
        || receipt.render_transaction_frame_index() != frame.input.packet().frame_index()
    {
        return DesktopPresentationControl::Continue;
    }
    assert_eq!(frame.mode, mode);
    assert_eq!(receipt.runtime_generation(), frame.runtime);
    assert_eq!(observation.runtime_generation(), frame.runtime);
    assert_eq!(world.resource::<RuntimeGeneration>(), &frame.runtime);
    assert_eq!(
        world.resource::<FrameExecutionStart>().frame(),
        frame.app_frame
    );
    assert_eq!(
        frame
            .input
            .packet()
            .source_generation()
            .runtime_generation(),
        Some(frame.runtime)
    );
    let current = CurrentScene::get(world).unwrap();
    assert_eq!(current.receipt(), &frame.instance);
    let source = current.source_view();
    match mode {
        ContentMode::Source => source
            .with_compiled_scene(|scene, resolver| {
                assert!(resolver.is_some());
                assert_eq!(scene.source_key(), &frame.source);
                assert_eq!(scene.canonical_provenance(), &frame.provenance);
            })
            .expect("the exact presented Source frame keeps its admitted source owner"),
        ContentMode::Processed => source
            .with_runtime_scene(|scene| {
                assert_eq!(scene.source_key(), &frame.source);
                assert_eq!(scene.canonical_provenance(), &frame.provenance);
            })
            .expect("the exact presented Processed frame keeps its admitted cooked Scene owner"),
    }
    let RenderFrameTarget::Window(window) = frame.input.packet().target() else {
        panic!("ordinary desktop capture must retain its actual window target");
    };
    assert_eq!(receipt.window_id(), window.id);
    assert!(receipt.surface_generation() > 0);
    let wgpu = observation.wgpu();
    assert_eq!(wgpu.state(), WgpuBackendState::Ready);
    assert_eq!(receipt.backend_instance_id(), wgpu.instance_id());
    assert_eq!(receipt.device_epoch(), wgpu.device_epoch());
    assert_eq!(
        wgpu.configured_surface_extent(window.id),
        Some(frame.input.packet().target().extent())
    );
    let transaction = wgpu.frame_transaction_stats();
    assert_eq!(transaction.app_frame_index(), Some(frame.app_frame));
    assert_eq!(
        transaction.render_transaction_frame_index(),
        Some(frame.input.packet().frame_index())
    );
    assert!(transaction.queue_submissions() > 0 && transaction.presents() > 0);
    let adapter = observation
        .adapter_summary()
        .expect("native hardware is a prerequisite");
    assert_ne!(
        adapter.device_type(),
        "cpu",
        "software adapters cannot qualify native stock capture"
    );
    assert_eq!(adapter.backend(), std::env::var("WGPU_BACKEND").unwrap());
    assert_stock_pixels(&frame.input, if index == 0 { RED } else { GREEN });
    if index == 0 {
        assert!(!state.stores_emptied);
        result.backend_instance = Some(receipt.backend_instance_id());
        result.device_epoch = Some(receipt.device_epoch());
        result.frames[0] = Some(frame.clone());
        // Only this actual post-Host red receipt allows normal gameplay to request the later Scene.
        state.red_presented.store(true, Ordering::Release);
        DesktopPresentationControl::Continue
    } else {
        assert!(state.stores_emptied && world.resource::<TravelState>().committed);
        assert!(world.resource::<Assets<ImageAsset>>().is_empty());
        assert!(world.resource::<PreparedImages>().is_empty());
        let red = result.frames[0].as_ref().unwrap();
        assert_eq!(frame.runtime, red.runtime);
        assert_ne!(frame.instance.instance_id(), red.instance.instance_id());
        assert!(frame.app_frame > red.app_frame);
        assert!(frame.input.packet().frame_index() > red.input.packet().frame_index());
        assert_ne!(frame.source, red.source);
        assert_eq!(Some(receipt.backend_instance_id()), result.backend_instance);
        assert_eq!(Some(receipt.device_epoch()), result.device_epoch);
        result.frames[1] = Some(frame.clone());
        result.stores_emptied_before_present = true;
        DesktopPresentationControl::Exit(AppExit::Success)
    }
}
