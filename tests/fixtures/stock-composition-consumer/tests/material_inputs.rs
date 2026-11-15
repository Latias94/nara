#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

use std::{
    sync::{Arc, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

use nara::material::{Material2dBinding, MaterialParameterValue, SamplerDescriptor};
use nara_render_wgpu::{
    GraphError, GraphLimits, LimitKind, TextureAlpha, TextureInterpretation, TextureTransfer,
    WgpuMaterialTextureBinding, WgpuNativeWorkStats, WgpuProgramTextureOutput, WgpuPublishedFrame,
    WgpuRenderHost, WgpuRenderHostError, WgpuRenderProducer, WgpuRenderProgramBuilder,
    WgpuRenderProgramError, WgpuStockCapture, WgpuWorldColor,
};
use nara_stock_composition_consumer::{CompositionPolicy, readback};

#[path = "support/material_inputs/mod.rs"]
mod material_inputs;
#[path = "support/scene.rs"]
#[allow(dead_code)]
mod scene;

use material_inputs::{
    ACCENT_ID, Branch, Expected, Fixture, PALETTE_A, PALETTE_B, PALETTE_ID, Palette, SCALE,
    SCALE_ID, Scenario, Source, TOLERANCE, Wiring, descriptor, graph, publish,
};

fn generated(colors: [[f32; 4]; 4], format: wgpu::TextureFormat) -> Source {
    Source::Generated { colors, format }
}

fn case(fixture: &Fixture, material: Material2dBinding, wiring: Wiring) -> Scenario {
    Scenario::new(
        &fixture.inputs[0],
        fixture.stock(0),
        material,
        generated(PALETTE_A, wgpu::TextureFormat::Rgba8Unorm),
        wiring,
    )
}

fn validate(scenario: &Scenario) -> Result<(), WgpuRenderHostError> {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let result = host.validate_input(&producer, &scenario.input, wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.retained_candidate_metadata_bytes(), 0);
    assert_eq!(host.frame_transaction_stats().queue_submissions(), 0);
    assert_eq!(
        host.frame_transaction_stats().native_work(),
        WgpuNativeWorkStats::default()
    );
    assert_eq!(scenario.live.load(Ordering::SeqCst), 0);
    assert!(host.retire_producer(&producer).is_none());
    result
}

fn feature_refusal(error: WgpuRenderHostError, reason: &str) {
    match error {
        WgpuRenderHostError::Program(WgpuRenderProgramError::Feature { message }) => {
            assert!(
                message.contains(reason),
                "expected {reason:?}, got {message:?}"
            );
        }
        error => panic!("expected Material refusal {reason:?}, got {error}"),
    }
}

#[test]
fn graph_material_bindings_preserve_the_original_descriptor_and_semantic_texture_id() {
    let fixture = Fixture::new(SamplerDescriptor::NEAREST_CLAMP);
    let mut graph = graph();
    for alpha in [
        TextureAlpha::Opaque,
        TextureAlpha::Straight,
        TextureAlpha::Premultiplied,
    ] {
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ] {
            let mut descriptor = descriptor(format);
            let TextureInterpretation::Color {
                alpha: descriptor_alpha,
                ..
            } = &mut descriptor.interpretation
            else {
                panic!("expected a color texture");
            };
            *descriptor_alpha = alpha;
            let source = graph.texture::<Palette>(descriptor.clone()).unwrap();
            let binding = WgpuMaterialTextureBinding::new(&fixture.material, PALETTE_ID, &source)
                .expect("whole baseline float sources retain their explicit interpretation");
            assert_eq!(binding.descriptor(), &descriptor);
            assert_eq!(binding.clone().descriptor(), source.descriptor());
            for unknown in ["image.unknown", "palette_texture", "1"] {
                assert!(matches!(
                    WgpuMaterialTextureBinding::new(&fixture.material, unknown, &source),
                    Err(WgpuRenderProgramError::Feature { message }) if message.contains("unknown")
                ));
            }
        }
    }
}

#[test]
fn graph_material_bindings_refuse_integer_unfilterable_unsampled_and_mislabelled_sources() {
    let fixture = Fixture::new(SamplerDescriptor::NEAREST_CLAMP);
    let mut graph = graph();
    for format in [
        wgpu::TextureFormat::Rgba8Uint,
        wgpu::TextureFormat::Rgba8Sint,
        wgpu::TextureFormat::Rgba32Float,
    ] {
        let source = graph.texture::<Palette>(descriptor(format)).unwrap();
        assert!(
            matches!(
                WgpuMaterialTextureBinding::new(&fixture.material, PALETTE_ID, &source),
                Err(WgpuRenderProgramError::Feature { message }) if message.contains("filterable")
            ),
            "{format:?} cannot satisfy the stock filterable-float texture ABI"
        );
    }
    let mut unsampled = descriptor(wgpu::TextureFormat::Rgba8Unorm);
    unsampled
        .native
        .usage
        .remove(wgpu::TextureUsages::TEXTURE_BINDING);
    let source = graph.texture::<Palette>(unsampled).unwrap();
    assert!(WgpuMaterialTextureBinding::new(&fixture.material, PALETTE_ID, &source).is_err());
    for (format, transfer) in [
        (wgpu::TextureFormat::Rgba8Unorm, TextureTransfer::Srgb),
        (wgpu::TextureFormat::Rgba8UnormSrgb, TextureTransfer::Linear),
    ] {
        let mut mislabelled = descriptor(format);
        let TextureInterpretation::Color {
            transfer: descriptor_transfer,
            ..
        } = &mut mislabelled.interpretation
        else {
            panic!("expected a color texture");
        };
        *descriptor_transfer = transfer;
        let source = graph.texture::<Palette>(mislabelled).unwrap();
        assert!(matches!(
            WgpuMaterialTextureBinding::new(&fixture.material, PALETTE_ID, &source),
            Err(WgpuRenderProgramError::Feature { message }) if message.contains("transfer")
        ));
    }
}

#[test]
fn exact_original_generation_uniform_bytes_and_complete_image_map_select_the_stock_draw() {
    let fixture = Fixture::new(SamplerDescriptor::NEAREST_CLAMP);
    let original = &fixture.material;
    let rebound = original
        .generation()
        .bind_values(original.parameters().clone(), original.textures().clone())
        .unwrap();
    assert_eq!(original.uniform_bytes(), rebound.uniform_bytes());
    for material in [original.clone(), rebound] {
        let scenario = case(&fixture, material, Wiring::Exact);
        validate(&scenario).expect("an exact reconstructed binding selects the captured draw");
        assert_eq!(
            scenario.ports().palette.descriptor(),
            &descriptor(wgpu::TextureFormat::Rgba8Unorm)
        );
    }
    let mut changed_values = original.parameters().clone();
    changed_values.insert(
        SCALE_ID.to_owned(),
        MaterialParameterValue::Vec4([0.5, 0.5, 0.25, 0.0]),
    );
    let changed_uniform = original
        .generation()
        .bind_values(changed_values, original.textures().clone())
        .unwrap();
    let mut signed_zero = original.parameters().clone();
    signed_zero.insert(
        SCALE_ID.to_owned(),
        MaterialParameterValue::Vec4([SCALE[0], SCALE[1], SCALE[2], -0.0]),
    );
    let signed_zero = original
        .generation()
        .bind_values(signed_zero, original.textures().clone())
        .unwrap();
    assert_eq!(original.parameters(), signed_zero.parameters());
    assert_ne!(original.uniform_bytes(), signed_zero.uniform_bytes());
    assert_eq!(&original.uniform_bytes()[12..16], &0.0_f32.to_le_bytes());
    assert_eq!(
        &signed_zero.uniform_bytes()[12..16],
        &(-0.0_f32).to_le_bytes()
    );
    let foreign_generation = fixture
        .foreign_generation
        .bind_values(original.parameters().clone(), original.textures().clone())
        .unwrap();
    assert_eq!(
        original.generation().declaration(),
        foreign_generation.generation().declaration()
    );
    assert_eq!(
        original.generation().shader().source(),
        foreign_generation.generation().shader().source()
    );
    assert_eq!(original.uniform_bytes(), foreign_generation.uniform_bytes());
    assert_ne!(original.generation(), foreign_generation.generation());
    let mut mismatches = vec![changed_uniform, signed_zero, foreign_generation];
    for texture_id in [PALETTE_ID, ACCENT_ID] {
        let mut textures = original.textures().clone();
        textures.insert(texture_id.to_owned(), fixture.other_image);
        mismatches.push(
            original
                .generation()
                .bind_values(original.parameters().clone(), textures)
                .unwrap(),
        );
    }
    for material in mismatches {
        feature_refusal(
            validate(&case(&fixture, material, Wiring::Exact)).unwrap_err(),
            "no matching captured draw",
        );
    }
}

#[test]
fn duplicate_and_unused_material_texture_mappings_refuse_the_entire_cpu_group() {
    let fixture = Fixture::new(SamplerDescriptor::NEAREST_CLAMP);
    feature_refusal(
        validate(&case(&fixture, fixture.material.clone(), Wiring::Duplicate)).unwrap_err(),
        "duplicate",
    );
    let plain = scene::capture(false);
    for stock in [
        Arc::new(WgpuStockCapture::new()),
        Arc::clone(&plain.captured::<CompositionPolicy>().unwrap().stock),
    ] {
        let original = if stock.sprite_batches().is_some() {
            &plain
        } else {
            &fixture.inputs[0]
        };
        let scenario = Scenario::new(
            original,
            stock,
            fixture.material.clone(),
            generated(PALETTE_A, wgpu::TextureFormat::Rgba8Unorm),
            Wiring::Exact,
        );
        feature_refusal(validate(&scenario).unwrap_err(), "material");
    }
}

#[test]
fn foreign_graph_sources_and_missing_palette_writers_keep_the_original_graph_refusals() {
    let fixture = Fixture::new(SamplerDescriptor::NEAREST_CLAMP);
    for wiring in [Wiring::ForeignGraph, Wiring::MissingWriter] {
        let error = validate(&case(&fixture, fixture.material.clone(), wiring)).unwrap_err();
        let WgpuRenderHostError::Program(WgpuRenderProgramError::Graph(error)) = error else {
            panic!("{wiring:?} must fail in the graph compiler: {error}");
        };
        assert!(matches!(
            (wiring, error),
            (Wiring::ForeignGraph, GraphError::ForeignCandidate)
                | (Wiring::MissingWriter, GraphError::MissingProducer(_))
        ));
    }
}

#[test]
fn two_material_slots_sampling_one_exact_version_share_one_declared_graph_read() {
    let fixture = Fixture::new(SamplerDescriptor::NEAREST_CLAMP);
    let scenario = Scenario::with_limits(
        &fixture.inputs[0],
        fixture.stock(0),
        fixture.material.clone(),
        generated(PALETTE_A, wgpu::TextureFormat::Rgba8Unorm),
        Wiring::SharedSlots,
        GraphLimits {
            max_resources: 2,
            max_operations: 2,
            max_accesses: 3,
            max_outputs: 1,
            ..Default::default()
        },
    );
    validate(&scenario)
        .expect("one palette writer, one world writer and one shared sampled read fit exactly");
}

#[test]
fn cpu_policy_validation_uses_only_the_original_live_producer_without_native_work() {
    let fixture = Fixture::new(SamplerDescriptor::NEAREST_CLAMP);
    let scenario = case(&fixture, fixture.material.clone(), Wiring::Exact);
    let mut host = WgpuRenderHost::offscreen();
    let mut foreign_host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let foreign = foreign_host.create_producer().unwrap();
    assert!(matches!(
        host.validate_input(&foreign, &scenario.input, wgpu::TextureFormat::Rgba8Unorm),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    for _ in 0..2 {
        host.validate_input(&producer, &scenario.input, wgpu::TextureFormat::Rgba8Unorm)
            .unwrap();
        assert_eq!(host.retained_candidate_metadata_bytes(), 0);
        assert_eq!(host.retained_resource_bytes(), 0);
        assert_eq!(host.frame_transaction_stats().queue_submissions(), 0);
        assert_eq!(
            host.frame_transaction_stats().native_work(),
            WgpuNativeWorkStats::default()
        );
        assert_eq!(scenario.live.load(Ordering::SeqCst), 0);
    }
    assert!(host.retire_producer(&producer).is_none());
    assert!(matches!(
        host.validate_input(&producer, &scenario.input, wgpu::TextureFormat::Rgba8Unorm),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    assert!(foreign_host.retire_producer(&foreign).is_none());
}

#[test]
fn same_builder_rejected_material_selection_can_retry_at_exact_resource_and_metadata_limits() {
    let fixture = Fixture::new(SamplerDescriptor::NEAREST_CLAMP);
    let mut limits = GraphLimits {
        max_resources: 2,
        max_operations: 2,
        max_accesses: 3,
        max_outputs: 1,
        max_texture_dimension_2d: scene::WIDTH,
        max_modeled_resource_bytes: u64::from(2 * 2 * 4 + scene::WIDTH * scene::HEIGHT * 4),
        ..Default::default()
    };
    let construct = |wiring, limits| {
        Scenario::with_limits(
            &fixture.inputs[0],
            fixture.stock(0),
            fixture.material.clone(),
            generated(PALETTE_A, wgpu::TextureFormat::Rgba8Unorm),
            wiring,
            limits,
        )
    };
    let metadata_refusal = |error| {
        assert!(matches!(
            error,
            WgpuRenderHostError::Program(WgpuRenderProgramError::Graph(GraphError::LimitExceeded(
                LimitKind::MetadataBytes
            )))
        ))
    };
    let mut lower = WgpuRenderProgramBuilder::required_metadata_bytes(limits).unwrap();
    let mut upper = limits.max_modeled_metadata_bytes;
    assert!(lower <= upper);
    limits.max_modeled_metadata_bytes = upper;
    validate(&construct(Wiring::CpuOnly, limits)).unwrap();
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        limits.max_modeled_metadata_bytes = middle;
        match validate(&construct(Wiring::CpuOnly, limits)) {
            Ok(()) => upper = middle,
            Err(error) => {
                metadata_refusal(error);
                lower = middle + 1;
            }
        }
    }
    limits.max_modeled_metadata_bytes = lower;
    validate(&construct(Wiring::CpuOnly, limits)).unwrap();
    validate(&construct(Wiring::RetryUnmatched, limits)).expect(
        "the rejected selection restores resources, modeled bytes, metadata and draw facts",
    );
    validate(&construct(Wiring::RetryOperation, limits)).expect(
        "late operation refusal restores resources, metadata and already accumulated draw facts",
    );
    validate(&construct(Wiring::RetryPlainWorld, limits)).expect(
        "ordinary world operation refusal leaves the same builder usable for a corrected retry",
    );
    limits.max_modeled_metadata_bytes -= 1;
    metadata_refusal(validate(&construct(Wiring::CpuOnly, limits)).unwrap_err());
    limits.max_modeled_metadata_bytes = lower;
    limits.max_modeled_resource_bytes -= 1;
    assert!(matches!(
        validate(&construct(Wiring::CpuOnly, limits)),
        Err(WgpuRenderHostError::Program(WgpuRenderProgramError::Graph(
            GraphError::LimitExceeded(LimitKind::ResourceBytes)
        )))
    ));
}

struct Retained {
    palette: WgpuProgramTextureOutput<Palette>,
    branch: WgpuProgramTextureOutput<Branch>,
    world: WgpuProgramTextureOutput<WgpuWorldColor>,
}

impl Retained {
    fn new(scenario: &Scenario, frame: &WgpuPublishedFrame) -> Self {
        let ports = scenario.ports();
        let palette = frame.outputs().texture(&ports.palette).unwrap();
        let branch = frame.outputs().texture(&ports.branch).unwrap();
        let selected = frame.outputs().texture(&ports.world).unwrap();
        let world = frame.final_texture::<WgpuWorldColor>().unwrap();
        assert_eq!(world.producer_id(), selected.producer_id());
        assert_eq!(
            world.source_generation(),
            frame.image().receipt().source_generation()
        );
        assert_eq!(
            world.frame_index(),
            frame.image().receipt().render_transaction_frame_index()
        );
        assert_eq!(world.frame_index(), scenario.input.packet().frame_index());
        assert_eq!(branch.producer_id(), world.producer_id());
        assert_eq!(branch.frame_index(), world.frame_index());
        assert_eq!(palette.descriptor().native.size.width, 2);
        assert_eq!(palette.descriptor().native.size.height, 2);
        let TextureInterpretation::Color {
            alpha: palette_alpha,
            ..
        } = palette.descriptor().interpretation
        else {
            panic!("expected a color texture");
        };
        let TextureInterpretation::Color {
            alpha: world_alpha, ..
        } = world.descriptor().interpretation
        else {
            panic!("expected a color texture");
        };
        assert_eq!(palette_alpha, TextureAlpha::Premultiplied);
        assert_eq!(world_alpha, TextureAlpha::Premultiplied);
        Self {
            palette,
            branch,
            world,
        }
    }

    fn verify(&self, host: &mut WgpuRenderHost, expected: &Expected) {
        expected.palette.assert_matches(
            &readback::read(host, &self.palette),
            TOLERANCE,
            "original generated/imported palette",
        );
        expected.branch.assert_matches(
            &readback::read(host, &self.branch),
            TOLERANCE,
            "independently authored sampled Grade branch",
        );
        expected.world.assert_matches(
            &readback::read(host, &self.world),
            TOLERANCE,
            "original stock Material sampling the shared palette",
        );
    }
}

fn retire(host: &mut WgpuRenderHost, producer: &WgpuRenderProducer) {
    let prefix = host
        .retire_producer(producer)
        .expect("the real submitted producer prefix");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !prefix
        .poll()
        .expect("poll the producer's actual original submission")
    {
        assert!(
            Instant::now() < deadline,
            "material producer retirement exceeded ten seconds"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert!(prefix.is_complete());
}

#[test]
#[ignore = "requires an explicitly selected native GPU and the original submission completion"]
fn imported_material_output_survives_source_lease_release_and_original_submission_settlement() {
    readback::require_native_selection();
    let sampler = SamplerDescriptor::NEAREST_CLAMP;
    let expected = Expected::new(PALETTE_A, wgpu::TextureFormat::Rgba8Unorm, sampler);
    let fixture = Fixture::new(sampler);
    let source = case(&fixture, fixture.material.clone(), Wiring::Exact);
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    let producer = host.create_producer().unwrap();
    let source_frame = publish(&mut host, &producer, &source.input).unwrap();
    let palette = source_frame
        .outputs()
        .texture(&source.ports().palette)
        .unwrap();
    retire(&mut host, &producer);
    let imported = Scenario::new(
        &fixture.inputs[1],
        fixture.stock(1),
        fixture.material.clone(),
        Source::Imported(palette.clone()),
        Wiring::Exact,
    );
    host.begin_frame(2).unwrap();
    let importer = host.create_producer().unwrap();
    let frame = publish(&mut host, &importer, &imported.input).unwrap();
    let world = frame.final_texture::<WgpuWorldColor>().unwrap();
    let prefix = frame.outputs().submitted_prefix().clone();
    let retirement = host.retire_producer(&importer).unwrap();
    assert!(retirement.submission_index().is_some());
    assert!(prefix.submission_index().is_some());
    drop(retirement);
    let complete_before_drop = prefix.is_complete();
    drop((
        source_frame,
        frame,
        palette,
        imported,
        source,
        fixture,
        producer,
        importer,
    ));
    let retained_before_poll = host.retained_resource_bytes();
    let complete_after_drop = prefix.is_complete();
    eprintln!(
        "original import completion before_drop={complete_before_drop}, \
         after_drop={complete_after_drop}, retained_bytes={retained_before_poll}"
    );
    let world_bytes = u64::from(scene::WIDTH * scene::HEIGHT * 4);
    assert!(
        retained_before_poll >= world_bytes,
        "the retained destination remains charged"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !prefix.poll().unwrap() {
        assert!(
            Instant::now() < deadline,
            "original import completion exceeded ten seconds"
        );
        thread::sleep(Duration::from_millis(1));
    }
    expected.world.assert_matches(
        &readback::read(&mut host, &world),
        TOLERANCE,
        "world remains exact after source inputs and outputs have all been released",
    );
    drop((world, prefix));
    readback::finish(&mut host);
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.retained_candidate_metadata_bytes(), 0);
    assert_eq!(host.cleanup_fault_count(), 0);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; no adapter fallback or passing skip"]
fn generated_palette_drives_stock_material_and_an_independent_branch_across_successors_imports_and_retirement()
 {
    readback::require_native_selection();
    for sampler in [
        SamplerDescriptor::NEAREST_CLAMP,
        SamplerDescriptor::LINEAR_CLAMP,
    ] {
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ] {
            let expected_a = Expected::new(PALETTE_A, format, sampler);
            let expected_b = Expected::new(PALETTE_B, format, sampler);
            assert_ne!(expected_a.world.pixels, expected_b.world.pixels);
            assert_ne!(expected_a.world.pixels, Expected::ordinary(sampler).pixels);
            assert_ne!(expected_a.palette.pixels, expected_a.branch.pixels);
            let alternate_sampler = if sampler == SamplerDescriptor::NEAREST_CLAMP {
                SamplerDescriptor::LINEAR_CLAMP
            } else {
                SamplerDescriptor::NEAREST_CLAMP
            };
            assert_ne!(
                expected_a.world.pixels,
                Expected::new(PALETTE_A, format, alternate_sampler)
                    .world
                    .pixels
            );
            let fixture = Fixture::new(sampler);
            let stock_lifetimes: Vec<_> = (0..3)
                .map(|index| Arc::downgrade(&fixture.stock(index)))
                .collect();
            let first = Scenario::new(
                &fixture.inputs[0],
                fixture.stock(0),
                fixture.material.clone(),
                generated(PALETTE_A, format),
                Wiring::Exact,
            );
            let successor = Scenario::new(
                &fixture.inputs[1],
                fixture.stock(1),
                fixture.material.clone(),
                generated(PALETTE_B, format),
                Wiring::Exact,
            );
            let delayed = Scenario::new(
                &fixture.inputs[0],
                fixture.stock(0),
                fixture.material.clone(),
                generated(PALETTE_A, format),
                Wiring::Exact,
            );
            let mut host = WgpuRenderHost::offscreen();
            host.begin_frame(1).unwrap();
            let producer = host.create_producer().unwrap();
            let first_frame = publish(&mut host, &producer, &first.input).unwrap();
            let old = Retained::new(&first, &first_frame);
            let work = host.frame_transaction_stats().native_work();
            assert_eq!(
                work.issued_texture_write_calls, 1,
                "only the unreplaced accent/base Image is uploaded"
            );
            assert_eq!(
                work.issued_texture_write_bytes, 4,
                "the captured default palette must not reach native Image upload"
            );
            assert!(first.live.load(Ordering::SeqCst) > 0);
            host.begin_frame(2).unwrap();
            let next_frame = publish(&mut host, &producer, &successor.input).unwrap();
            let next = Retained::new(&successor, &next_frame);
            assert_eq!(
                host.frame_transaction_stats()
                    .native_work()
                    .issued_texture_write_calls,
                0,
                "the ordinary Image cache is reused without admitting graph textures"
            );
            assert_eq!(old.world.producer_id(), next.world.producer_id());
            assert_eq!(
                old.world.source_generation(),
                next.world.source_generation()
            );
            assert!(old.world.frame_index() < next.world.frame_index());
            retire(&mut host, &producer);
            drop(producer);
            assert!(old.palette.is_current());
            assert!(old.world.is_current());
            let imported = Scenario::new(
                &fixture.inputs[2],
                fixture.stock(2),
                fixture.material.clone(),
                Source::Imported(old.palette.clone()),
                Wiring::Exact,
            );
            host.begin_frame(3).unwrap();
            let importer = host.create_producer().unwrap();
            let imported_frame = publish(&mut host, &importer, &imported.input).unwrap();
            let prior = Retained::new(&imported, &imported_frame);
            assert_eq!(prior.palette.producer_id(), old.palette.producer_id());
            assert_eq!(
                prior.palette.source_generation(),
                old.palette.source_generation()
            );
            assert_eq!(prior.palette.frame_index(), old.palette.frame_index());
            assert_ne!(prior.world.producer_id(), old.world.producer_id());
            assert!(prior.world.frame_index() > next.world.frame_index());
            let late_producer = host.create_producer().unwrap();
            let delayed_frame = publish(&mut host, &late_producer, &delayed.input).unwrap();
            let late = Retained::new(&delayed, &delayed_frame);
            assert_eq!(late.world.frame_index(), old.world.frame_index());
            assert_ne!(late.world.producer_id(), old.world.producer_id());
            retire(&mut host, &importer);
            retire(&mut host, &late_producer);
            let live = [
                Arc::clone(&first.live),
                Arc::clone(&successor.live),
                Arc::clone(&imported.live),
                Arc::clone(&delayed.live),
            ];
            drop((first_frame, next_frame, imported_frame, delayed_frame));
            drop((
                first,
                successor,
                imported,
                delayed,
                fixture,
                importer,
                late_producer,
            ));
            for stock in stock_lifetimes {
                assert!(
                    stock.upgrade().is_none(),
                    "native output leases must not retain the source stock capture"
                );
            }
            next.verify(&mut host, &expected_b);
            prior.verify(&mut host, &expected_a);
            late.verify(&mut host, &expected_a);
            old.verify(&mut host, &expected_a);
            expected_a.world.assert_matches(
                &readback::read(&mut host, &old.world),
                TOLERANCE,
                "old stock output remains unchanged after every independent read",
            );
            drop((old, next, prior, late));
            readback::finish(&mut host);
            assert_eq!(host.retained_resource_bytes(), 0);
            assert_eq!(host.retained_candidate_metadata_bytes(), 0);
            assert_eq!(host.cleanup_fault_count(), 0);
            for count in live {
                assert_eq!(
                    count.load(Ordering::SeqCst),
                    0,
                    "the original Host retires actual Feature state and prepared native objects"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires an explicitly selected native GPU; keeps the unmodified asset-Image stock path"]
fn the_original_world_operation_keeps_the_actual_prepared_material_image_default() {
    readback::require_native_selection();
    let sampler = SamplerDescriptor::NEAREST_CLAMP;
    let expected = Expected::ordinary(sampler);
    let fixture = Fixture::new(sampler);
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    let producer = host.create_producer().unwrap();
    let frame = publish(&mut host, &producer, &fixture.inputs[0]).unwrap();
    let output = frame.final_texture::<WgpuWorldColor>().unwrap();
    let work = host.frame_transaction_stats().native_work();
    assert_eq!(work.issued_texture_write_calls, 2);
    assert_eq!(work.issued_texture_write_bytes, 8);
    expected.assert_matches(
        &readback::read(&mut host, &output),
        TOLERANCE,
        "unchanged ordinary stock world with both real Image inputs",
    );
    retire(&mut host, &producer);
    drop((output, frame, fixture, producer));
    readback::finish(&mut host);
    assert_eq!(host.retained_resource_bytes(), 0);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; refuses foreign-Host sources before and after their retirement"]
fn graph_material_imports_reject_other_hosts_before_and_after_source_retirement_without_partial_groups()
 {
    readback::require_native_selection();
    let sampler = SamplerDescriptor::NEAREST_CLAMP;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let expected = Expected::new(PALETTE_A, format, sampler);
    let fixture = Fixture::new(sampler);
    let first = Scenario::new(
        &fixture.inputs[0],
        fixture.stock(0),
        fixture.material.clone(),
        generated(PALETTE_A, format),
        Wiring::Exact,
    );
    let mut original = WgpuRenderHost::offscreen();
    original.begin_frame(1).unwrap();
    let producer = original.create_producer().unwrap();
    let frame = publish(&mut original, &producer, &first.input).unwrap();
    let retained = Retained::new(&first, &frame);
    let foreign = Scenario::new(
        &fixture.inputs[1],
        fixture.stock(1),
        fixture.material.clone(),
        Source::Imported(retained.palette.clone()),
        Wiring::Exact,
    );
    let mut other = WgpuRenderHost::offscreen();
    other.begin_frame(1).unwrap();
    let other_producer = other.create_producer().unwrap();
    for retired_epoch in [false, true] {
        let before = other.frame_transaction_stats();
        match publish(&mut other, &other_producer, &foreign.input) {
            Err(WgpuRenderHostError::Program(WgpuRenderProgramError::StaleImport)) => {}
            Err(error) => panic!("unrelated foreign/retired import refusal: {error}"),
            Ok(_) => {
                panic!("a Material import published despite foreign Host or retired source epoch")
            }
        }
        assert_eq!(
            other.frame_transaction_stats().queue_submissions(),
            before.queue_submissions()
        );
        assert_eq!(
            other.frame_transaction_stats().native_work(),
            before.native_work()
        );
        assert_eq!(foreign.live.load(Ordering::SeqCst), 0);
        if !retired_epoch {
            retained.verify(&mut original, &expected);
            retire(&mut original, &producer);
            readback::finish(&mut original);
            assert!(!retained.palette.is_current());
            let mut declaration = graph();
            declaration.import_texture(&retained.palette).unwrap();
        }
    }
    retire(&mut other, &other_producer);
    drop((
        foreign,
        first,
        frame,
        retained,
        fixture,
        producer,
        other_producer,
    ));
    readback::finish(&mut other);
    assert_eq!(original.retained_resource_bytes(), 0);
    assert_eq!(other.retained_resource_bytes(), 0);
    assert_eq!(original.cleanup_fault_count(), 0);
    assert_eq!(other.cleanup_fault_count(), 0);
}
