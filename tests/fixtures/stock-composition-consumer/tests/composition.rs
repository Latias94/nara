#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use nara::{
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    project_host::{
        EditorPresentationImage, EditorPreviewPresentation, EditorProjectCloseProgress,
        EditorProjectSession, TrustedCompiledEditorProjectIntent,
    },
    reflect::{ComponentTypeId, ComponentValue},
    render::{
        DisplayMapping2d, DisplayRect, DisplayScale, Extent2d, RenderFrameTarget, RenderImage2d,
        RenderImagePacket,
    },
    scene::{SceneDocument, SceneDocumentCandidate, SceneEntityId},
};
use nara_render_wgpu::{
    WgpuCompositeColor, WgpuImageOutput, WgpuPolicyDefinition, WgpuProgramTexture,
    WgpuPublishedFrame, WgpuRenderHost, WgpuRenderInput, WgpuRenderProducer,
    WgpuRenderProgramError, WgpuSceneColor, WgpuWorldColor,
};
use nara_stock_composition_consumer::{
    CompositionCapture, CompositionConfiguration, CompositionPolicy, CompositionPorts, EffectSpace,
    SelectedPorts, oracle, readback,
};

#[path = "support/hud.rs"]
mod hud;
#[path = "support/scene.rs"]
#[allow(dead_code)]
mod scene;

use oracle::{EFFECT_TOLERANCE, Image};

struct Case {
    input: WgpuRenderInput,
    inspection: Arc<Mutex<Vec<CompositionPorts>>>,
    original_world: Image,
    original_scene: Option<Image>,
    blurred: Image,
    graded: Image,
    selected: Image,
}

impl Case {
    fn new(original: &WgpuRenderInput, configuration: CompositionConfiguration, ui: bool) -> Self {
        // Every CPU expectation and tolerance is fixed here, before prepare or GPU readback.
        let canvas = original.packet().world_canvas().is_some();
        let original_world = if canvas {
            Image::scene(
                scene::WORLD_WIDTH,
                scene::WORLD_HEIGHT,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            )
        } else {
            Image::scene(scene::WIDTH, scene::HEIGHT, wgpu::TextureFormat::Rgba8Unorm)
        };
        let original_scene = canvas.then(|| {
            original_world.resolve(scene::WIDTH, scene::HEIGHT, wgpu::TextureFormat::Rgba8Unorm)
        });
        let effect_source = match (configuration.space, original_scene.as_ref()) {
            (EffectSpace::Resolved, Some(scene)) => scene,
            _ => &original_world,
        };
        let blurred = effect_source.blur(configuration.radius);
        let graded = blurred.grade(configuration.grade);
        let selected = if configuration.enabled {
            if canvas && configuration.space == EffectSpace::World {
                graded.resolve(scene::WIDTH, scene::HEIGHT, wgpu::TextureFormat::Rgba8Unorm)
            } else {
                graded.clone()
            }
        } else {
            original_scene.as_ref().unwrap_or(&original_world).clone()
        };
        let selected = if ui { selected.hud() } else { selected };
        let original_capture = original.captured::<CompositionPolicy>().unwrap();
        let mut stock = (*original_capture.stock).clone();
        if ui {
            let target = original.packet().views()[0].target;
            let primary = match original.packet().target() {
                RenderFrameTarget::Window(window) => Some(window.id),
                _ => None,
            };
            stock = stock.with_ui(
                vec![hud::packet(target, scene::WIDTH, scene::HEIGHT)],
                Some(target),
                primary,
            );
        }
        let inspection = Arc::new(Mutex::new(Vec::new()));
        let installation = WgpuPolicyDefinition::new(CompositionPolicy::new())
            .install()
            .unwrap();
        let input = installation
            .capture(
                original.packet().clone(),
                Arc::new(CompositionCapture::inspected(
                    Arc::new(stock),
                    configuration,
                    Arc::clone(&inspection),
                )),
            )
            .unwrap();
        // Closing the original admission route cannot reconstruct or invalidate accepted inputs.
        drop(installation);
        Self {
            input,
            inspection,
            original_world,
            original_scene,
            blurred,
            graded,
            selected,
        }
    }

    fn submit(
        &self,
        host: &mut WgpuRenderHost,
    ) -> (WgpuRenderProducer, WgpuPublishedFrame, CompositionPorts) {
        let producer = host.create_producer().unwrap();
        let target = RenderImagePacket::new(
            RenderImage2d::allocate_handle().unwrap(),
            Extent2d::new(scene::WIDTH, scene::HEIGHT).unwrap(),
        )
        .unwrap();
        let input = self.input.with_image_target(target).unwrap();
        assert_eq!(
            input.packet().source_generation(),
            self.input.packet().source_generation()
        );
        assert_eq!(
            input.packet().frame_index(),
            self.input.packet().frame_index()
        );
        assert_eq!(input.packet().views(), self.input.packet().views());
        assert_eq!(
            input.packet().world_canvas(),
            self.input.packet().world_canvas()
        );
        let reservation = host
            .reserve_image_target(&producer, input.packet().source_generation(), target)
            .unwrap();
        host.activate_image_target(&producer, reservation).unwrap();
        let candidate = host.prepare(&producer, &input).unwrap();
        let ports = self
            .inspection
            .lock()
            .unwrap()
            .pop()
            .expect("this exact candidate's CPU ports");
        let published = host
            .submit_candidate(&producer, candidate)
            .unwrap_or_else(|failure| panic!("{:?}", failure.error));
        assert_eq!(
            published.image().receipt().source_generation(),
            self.input.packet().source_generation()
        );
        assert_eq!(
            published.image().receipt().render_transaction_frame_index(),
            self.input.packet().frame_index()
        );
        (producer, published, ports)
    }

    fn verify(
        &self,
        host: &mut WgpuRenderHost,
        frame: &WgpuPublishedFrame,
        ports: &CompositionPorts,
    ) {
        verify_port(
            host,
            frame,
            &ports.original_world,
            &self.original_world,
            0,
            "untouched ordinary Sprite world",
        );
        if let (Some(port), Some(expected)) = (&ports.original_scene, &self.original_scene) {
            verify_port(host, frame, port, expected, 0, "untouched resolved Scene");
        }
        match &ports.selected {
            SelectedPorts::World {
                blurred,
                graded,
                resolved,
            } => {
                verify_port(
                    host,
                    frame,
                    blurred,
                    &self.blurred,
                    EFFECT_TOLERANCE,
                    "world-texel blur",
                );
                verify_port(
                    host,
                    frame,
                    graded,
                    &self.graded,
                    EFFECT_TOLERANCE,
                    "independent world grade",
                );
                if let Some(port) = resolved {
                    let expected = self.graded.resolve(
                        scene::WIDTH,
                        scene::HEIGHT,
                        wgpu::TextureFormat::Rgba8Unorm,
                    );
                    verify_port(
                        host,
                        frame,
                        port,
                        &expected,
                        EFFECT_TOLERANCE,
                        "post-effect Canvas resolve",
                    );
                }
            }
            SelectedPorts::Resolved { blurred, graded } => {
                verify_port(
                    host,
                    frame,
                    blurred,
                    &self.blurred,
                    EFFECT_TOLERANCE,
                    "resolved-pixel blur",
                );
                verify_port(
                    host,
                    frame,
                    graded,
                    &self.graded,
                    EFFECT_TOLERANCE,
                    "independent resolved grade",
                );
            }
            SelectedPorts::Disabled => {}
        }
    }
}

fn verify_port<S: 'static>(
    host: &mut WgpuRenderHost,
    frame: &WgpuPublishedFrame,
    port: &WgpuProgramTexture<S>,
    expected: &Image,
    tolerance: u8,
    label: &str,
) {
    let output = frame
        .outputs()
        .texture(port)
        .expect("the original exact branch must be published");
    expected.assert_matches(&readback::read(host, &output), tolerance, label);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; no adapter fallback or passing skip"]
fn ordinary_sprites_branch_through_independent_blur_and_grade_in_both_canvas_spaces() {
    readback::require_native_selection();
    let plain = scene::capture(false);
    let canvas = scene::capture(true);
    let cases: Vec<_> = [
        (&plain, EffectSpace::World),
        (&canvas, EffectSpace::World),
        (&canvas, EffectSpace::Resolved),
    ]
    .into_iter()
    .map(|(input, space)| {
        Case::new(
            input,
            CompositionConfiguration {
                space,
                ..Default::default()
            },
            false,
        )
    })
    .collect();
    assert_ne!(
        cases[1].selected.pixels, cases[2].selected.pixels,
        "the predetermined fixture must distinguish actual world texels from resolved pixels"
    );
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    let published: Vec<_> = cases.iter().map(|case| case.submit(&mut host)).collect();
    // Read old branches only after every differently wired successor has executed.
    for (case, (_, frame, ports)) in cases.iter().zip(&published) {
        case.verify(&mut host, frame, ports);
    }
    for (case, (_, frame, ports)) in cases.iter().zip(&published).rev() {
        case.verify(&mut host, frame, ports);
    }
    // Replay each original immutable input on a different Host after the source Runtime retired.
    let mut replacement = WgpuRenderHost::offscreen();
    replacement.begin_frame(1).unwrap();
    for case in cases.iter().rev() {
        let (producer, frame, ports) = case.submit(&mut replacement);
        case.verify(&mut replacement, &frame, &ports);
        match &ports.selected {
            SelectedPorts::World { resolved: None, .. } => case.selected.assert_matches(
                &readback::read(
                    &mut replacement,
                    &frame
                        .final_texture::<nara_render_wgpu::WgpuWorldColor>()
                        .unwrap(),
                ),
                EFFECT_TOLERANCE,
                "replayed final world",
            ),
            _ => case.selected.assert_matches(
                &readback::read(
                    &mut replacement,
                    &frame
                        .final_texture::<nara_render_wgpu::WgpuSceneColor>()
                        .unwrap(),
                ),
                EFFECT_TOLERANCE,
                "replayed final Scene",
            ),
        }
        replacement.retire_producer(&producer);
    }
    for (producer, _, _) in &published {
        host.retire_producer(producer);
    }
    drop(published);
    readback::finish(&mut host);
    readback::finish(&mut replacement);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; no adapter fallback or passing skip"]
fn disabled_effect_keeps_the_exact_original_as_the_single_selected_scene() {
    readback::require_native_selection();
    let original = scene::capture(false);
    let case = Case::new(
        &original,
        CompositionConfiguration {
            enabled: false,
            ..Default::default()
        },
        false,
    );
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    let (producer, frame, ports) = case.submit(&mut host);
    case.verify(&mut host, &frame, &ports);
    let original_output = frame.outputs().texture(&ports.original_world).unwrap();
    let final_output = frame
        .final_texture::<nara_render_wgpu::WgpuWorldColor>()
        .unwrap();
    assert!(
        matches!(
            frame.final_texture::<WgpuCompositeColor>(),
            Err(WgpuRenderProgramError::ResourceSemantic)
        ),
        "without captured UI, the original world output must not acquire the composite marker"
    );
    assert_eq!(original_output.producer_id(), final_output.producer_id());
    assert_eq!(
        original_output.source_generation(),
        final_output.source_generation()
    );
    case.selected.assert_matches(
        &readback::read(&mut host, &final_output),
        0,
        "disabled exact original",
    );
    host.retire_producer(&producer);
    drop(frame);
    readback::finish(&mut host);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; reads the actual post-effect stock UI composite"]
fn full_resolution_hud_stays_exact_over_both_effect_pixel_spaces_and_retained_branches() {
    readback::require_native_selection();
    let plain = scene::capture(false);
    let canvas = scene::capture(true);
    // HUD geometry, effect-space expectations, transfer rounding and tolerance are fixed before
    // native preparation. The one-pixel stripe is compared exactly, independently of effects.
    let cases: Vec<_> = [
        (&plain, EffectSpace::World),
        (&canvas, EffectSpace::World),
        (&canvas, EffectSpace::Resolved),
    ]
    .into_iter()
    .map(|(input, space)| {
        Case::new(
            input,
            CompositionConfiguration {
                space,
                ..Default::default()
            },
            true,
        )
    })
    .collect();
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    let published: Vec<_> = cases.iter().map(|case| case.submit(&mut host)).collect();
    // Reading occurs after all three distinct compositions have executed, so this also checks
    // that their original before-effects branches and final HUD storage remain independent.
    let mut composite_pixels = Vec::with_capacity(cases.len());
    for (case, (_, frame, ports)) in cases.iter().zip(&published) {
        case.verify(&mut host, frame, ports);
        let composite = frame
            .final_texture::<WgpuCompositeColor>()
            .expect("the actual stock UI operation publishes its original composite semantic");
        assert!(matches!(
            frame.final_texture::<WgpuWorldColor>(),
            Err(WgpuRenderProgramError::ResourceSemantic)
        ));
        assert!(
            matches!(
                frame.final_texture::<WgpuSceneColor>(),
                Err(WgpuRenderProgramError::ResourceSemantic)
            ),
            "equal extent and color interpretation cannot cast the final UI output into Scene color"
        );
        let actual = readback::read(&mut host, &composite);
        case.selected.assert_matches(
            &actual,
            EFFECT_TOLERANCE,
            "complete stock blur/grade/HUD composite",
        );
        assert_eq!((actual.width, actual.height), (scene::WIDTH, scene::HEIGHT));
        let before_ui = match &ports.selected {
            SelectedPorts::World {
                graded,
                resolved: None,
                ..
            } => readback::read(&mut host, &frame.outputs().texture(graded).unwrap()),
            SelectedPorts::World {
                resolved: Some(scene),
                ..
            } => readback::read(&mut host, &frame.outputs().texture(scene).unwrap()),
            SelectedPorts::Resolved { graded, .. } => {
                readback::read(&mut host, &frame.outputs().texture(graded).unwrap())
            }
            SelectedPorts::Disabled => unreachable!("this fixed HUD matrix enables effects"),
        };
        for y in 0..scene::HEIGHT {
            for x in 0..scene::WIDTH {
                let index = (y * scene::WIDTH + x) as usize;
                if x == 1 && (1..scene::HEIGHT - 1).contains(&y) {
                    assert_eq!(
                        actual.pixels[index], [255; 4],
                        "exact one-physical-pixel HUD at ({x}, {y})"
                    );
                } else {
                    assert_eq!(
                        actual.pixels[index], before_ui.pixels[index],
                        "HUD must preserve the selected Scene's exact bytes outside its stripe at ({x}, {y})"
                    );
                }
            }
        }
        composite_pixels.push(actual);
    }
    let stock_lifetimes: Vec<_> = cases
        .iter()
        .map(|case| Arc::downgrade(&case.input.captured::<CompositionPolicy>().unwrap().stock))
        .collect();
    let images: Vec<_> = published
        .into_iter()
        .map(|(producer, frame, ports)| {
            let receipt = frame.image().receipt();
            // Consuming the complete publication drops all other outputs/reports. Only this exact
            // image remains; no original typed lease is kept from the checks above.
            let image = frame.into_image();
            drop(ports);
            assert!(
                host.retire_producer(&producer)
                    .expect("submitted producer's original prefix")
                    .poll()
                    .unwrap()
            );
            drop(producer);
            (image, receipt)
        })
        .collect();
    drop(cases);
    drop(plain);
    drop(canvas);
    for stock in stock_lifetimes {
        assert!(
            stock.upgrade().is_none(),
            "an image lease must not retain the original stock capture"
        );
    }
    // The Host stays in its original live epoch. A retired producer and disposed CPU capture
    // cannot relabel or erase the image's semantic meaning. Each read uses a fresh producer.
    for ((image, receipt), expected) in images.into_iter().zip(composite_pixels) {
        assert_eq!(image.receipt(), receipt);
        assert!(matches!(
            image.texture::<WgpuWorldColor>(),
            Err(WgpuRenderProgramError::ResourceSemantic)
        ));
        assert!(matches!(
            image.texture::<WgpuSceneColor>(),
            Err(WgpuRenderProgramError::ResourceSemantic)
        ));
        let lease = image
            .texture::<WgpuCompositeColor>()
            .expect("reduced image must retain its original exact composite semantic");
        assert_eq!(lease.producer_id(), receipt.producer_id());
        assert_eq!(lease.source_generation(), receipt.source_generation());
        assert_eq!(
            lease.frame_index(),
            receipt.render_transaction_frame_index()
        );
        assert_eq!(lease.descriptor().native.format, image.format());
        drop(image);
        expected.assert_matches(
            &readback::read(&mut host, &lease),
            0,
            "exact typed image lease after publication, producer, capture and image disposal",
        );
    }
    readback::finish(&mut host);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; reads the real 4K Editor Scene output"]
fn editor_scene_picks_pixel_centers_of_one_pixel_and_thin_quads_at_4k() {
    const PREVIEW_WORLD_HEIGHT: f64 = 720.0;

    readback::require_native_selection();
    let recipe = nara::ProductRecipe::new();
    let fixture = scene::SourceFixture::new(recipe.clone()).unwrap();
    let extent = Extent2d::new(4096, 2160).unwrap();
    let one_pixel = SceneEntityId::new("one-pixel").unwrap();
    let thin = SceneEntityId::new("thin").unwrap();
    let collapsed = SceneEntityId::new("collapsed").unwrap();
    let scene_path = fixture.root().join("scenes/startup.scene.json");
    let source = SceneDocumentCandidate::decode_json_bytes(&fs::read(&scene_path).unwrap())
        .unwrap()
        .into_composition_source()
        .unwrap();
    let template = source
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "sprite")
        .unwrap();
    let vec2 = |x, y| {
        ComponentValue::map([
            ("x", ComponentValue::f64(x).unwrap()),
            ("y", ComponentValue::f64(y).unwrap()),
        ])
    };
    let world_per_pixel = PREVIEW_WORLD_HEIGHT / f64::from(extent.height);
    let markers = [
        (&one_pixel, 2048, [1.0, 1.0], 1.0),
        (&thin, 2061, [11.0, 1.0 / 64.0], 1.0),
        (&collapsed, 2074, [11.0, 1.0], 0.0),
    ]
    .map(|(id, x, size, scale_y)| {
        let mut marker = template.clone();
        marker.id = id.clone();
        let transform = marker
            .components
            .get_mut(&ComponentTypeId::new("nara.transform.Transform2d"))
            .unwrap();
        let ComponentValue::Map(fields) = transform.value_mut() else {
            panic!("ordinary Transform fields")
        };
        let x = (f64::from(x) + 0.5 - f64::from(extent.width) / 2.0) * world_per_pixel;
        fields.insert("translation".into(), vec2(x, -0.5 * world_per_pixel));
        fields.insert("scale".into(), vec2(1.0, scale_y));
        let sprite = marker
            .components
            .get_mut(&ComponentTypeId::new("nara.sprite.Sprite"))
            .unwrap();
        let ComponentValue::Map(fields) = sprite.value_mut() else {
            panic!("ordinary Sprite fields")
        };
        fields.insert(
            "size".into(),
            vec2(size[0] * world_per_pixel, size[1] * world_per_pixel),
        );
        marker
    });
    fs::write(
        scene_path,
        SceneDocument::from_ordered_roots(markers)
            .to_json_string()
            .unwrap(),
    )
    .unwrap();

    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000);
    }
    let root = DirectoryCapability::from_host_handle(
        options.open(fixture.root()).unwrap(),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap();
    let mut editor = EditorProjectSession::open_trusted_compiled(
        root,
        TrustedCompiledEditorProjectIntent::new().with_recipe(recipe),
    )
    .unwrap();

    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        editor.configure_editor_preview(Some(extent));
        let candidate = editor.take_editor_preview_candidate().unwrap();
        assert_eq!(candidate.hits().len(), 3);
        let collapsed_hit = candidate
            .hits()
            .iter()
            .find(|hit| hit.entity() == &collapsed)
            .unwrap()
            .instance();
        assert!(collapsed_hit.x_axis.length_squared() > 0.0);
        assert_eq!(collapsed_hit.y_axis.length_squared(), 0.0);

        let mut presentation = EditorPreviewPresentation::new();
        let (accepted, input) = presentation.accept(candidate).unwrap();
        let RenderFrameTarget::Image(target) = input.packet().target() else {
            panic!("Editor Scene preview image target")
        };
        assert_eq!(target.extent(), extent);
        host.begin_frame(1).unwrap();
        let reservation = host
            .reserve_image_target(&producer, input.packet().source_generation(), target)
            .unwrap();
        host.activate_image_target(&producer, reservation).unwrap();
        let output = host.submit(&producer, &input).unwrap();
        let adapter = host.adapter_summary().unwrap();
        assert_eq!(adapter.backend(), std::env::var("WGPU_BACKEND").unwrap());
        assert_ne!(adapter.device_type(), "cpu");
        eprintln!("4K Scene preview adapter: {adapter:?}");
        let display = host.display_image(output).unwrap();
        let origin = [2047, 1079];
        let pixels = read_preview_crop(&mut host, display.output(), origin);
        let receipt = display.receipt();
        let mapping = DisplayMapping2d::fit(
            extent,
            DisplayRect {
                min: [0.0, 0.0],
                max: [extent.width as f32, extent.height as f32],
            },
            1.0,
            DisplayScale::IntegerFit,
        )
        .unwrap();
        let token = presentation
            .finish(
                accepted,
                EditorPresentationImage {
                    output_version: receipt.output_version(),
                    display_occurrence: display.occurrence().get(),
                    target: receipt.target(),
                    target_generation: receipt.target_generation(),
                    source_generation: receipt.source_generation(),
                    source_handoff_generation: receipt.source_handoff_generation(),
                    backend_instance_id: receipt.backend_instance_id(),
                    device_epoch: receipt.device_epoch(),
                    frame_index: receipt.render_transaction_frame_index(),
                },
                mapping,
            )
            .unwrap();
        let red = [255, 0, 0, 255];
        let clear = pixels[0][0];
        assert_ne!(clear, red);
        for (row, scanline) in pixels.into_iter().enumerate() {
            for (column, pixel) in scanline.into_iter().enumerate() {
                let x = origin[0] + column as u32;
                let y = origin[1] + row as u32;
                let expected = match (x, y) {
                    (2048, 1080) => Some(&one_pixel),
                    (2056..=2066, 1080) => Some(&thin),
                    _ => None,
                };
                assert_eq!(
                    pixel,
                    if expected.is_some() { red } else { clear },
                    "pixel ({x}, {y})",
                );
                let picked = presentation
                    .pick(token, [x as f32 + 0.5, y as f32 + 0.5])
                    .unwrap();
                assert_eq!(picked.as_ref(), expected, "pick at pixel ({x}, {y})");
            }
        }
    }));
    host.retire_producer(&producer);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let EditorProjectCloseProgress::Complete(receipt) = editor.advance_close() {
            assert!(!receipt.has_cleanup_failure());
            break;
        }
        assert!(Instant::now() < deadline, "Editor preview did not retire");
        thread::sleep(Duration::from_millis(1));
    }
    readback::finish(&mut host);
    if let Err(error) = outcome {
        resume_unwind(error);
    }
}

fn read_preview_crop(
    host: &mut WgpuRenderHost,
    output: &WgpuImageOutput,
    [x, y]: [u32; 2],
) -> [[[u8; 4]; 32]; 3] {
    const ROW_STRIDE: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    assert_eq!(output.format(), wgpu::TextureFormat::Rgba8Unorm);
    let ((device, buffer), prefix) = host
        .submit_image_use(std::slice::from_ref(output), |device, _, encoder, views| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scene-preview-pixel-centers"),
                size: 3 * u64::from(ROW_STRIDE),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut source = views[0].texture().as_image_copy();
            source.origin = wgpu::Origin3d { x, y, z: 0 };
            encoder.copy_texture_to_buffer(
                source,
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(ROW_STRIDE),
                        rows_per_image: Some(3),
                    },
                },
                wgpu::Extent3d {
                    width: 32,
                    height: 3,
                    depth_or_array_layers: 1,
                },
            );
            Ok((device.clone(), buffer))
        })
        .unwrap();
    let (sender, receiver) = mpsc::sync_channel(1);
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    let timeout = Duration::from_secs(10);
    device
        .poll(wgpu::PollType::Wait {
            submission_index: prefix.submission_index().cloned(),
            timeout: Some(timeout),
        })
        .unwrap();
    receiver.recv_timeout(timeout).unwrap().unwrap();
    assert!(prefix.poll().unwrap());
    let pixels = {
        let bytes = buffer.get_mapped_range(..).unwrap();
        std::array::from_fn(|row| {
            std::array::from_fn(|column| {
                let offset = row * ROW_STRIDE as usize + column * 4;
                bytes[offset..offset + 4].try_into().unwrap()
            })
        })
    };
    buffer.unmap();
    pixels
}
