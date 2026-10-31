use super::*;

use crate::graph::{
    AccessRole, BufferMeaning, GraphBuilder, GraphLimits, ResourceWrite, TextureAlpha,
    TextureMeaning, TextureTransfer,
};

struct Values;

fn buffer_descriptor(size: u64) -> wgpu::BufferDescriptor<'static> {
    wgpu::BufferDescriptor {
        label: Some("program-native-test-values"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    }
}

fn buffer_resource(size: u64) -> ResourceDescriptor {
    ResourceDescriptor::Buffer {
        native: buffer_descriptor(size),
        meaning: BufferMeaning::of::<Values>(),
    }
}

fn texture_resource(width: u32, height: u32) -> ResourceDescriptor {
    ResourceDescriptor::Texture {
        native: wgpu::TextureDescriptor {
            label: Some("program-native-test-texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        },
        range: wgpu::ImageSubresourceRange::default(),
        meaning: TextureMeaning::of::<Values>(TextureTransfer::Linear, TextureAlpha::Opaque),
    }
}

#[test]
fn foreign_and_undeclared_ports_reject_before_physical_index_lookup() {
    let mut builder = GraphBuilder::new(GraphLimits::default()).unwrap();
    let first = builder.reserve_resource(buffer_resource(16)).unwrap();
    let second = builder.reserve_resource(buffer_resource(16)).unwrap();
    let first = WgpuProgramBuffer::<Values>::new(first, buffer_descriptor(16));
    let second = WgpuProgramBuffer::<Values>::new(second, buffer_descriptor(16));
    builder
        .add_operation(1, &[], &[first.zero(AccessRole::StorageWrite).0])
        .unwrap();
    builder
        .add_operation(2, &[], &[second.zero(AccessRole::StorageWrite).0])
        .unwrap();
    builder.set_output_group(&[second.version.clone()]).unwrap();
    let graph = builder.compile().unwrap();
    let resources = WgpuOperationResources {
        graph: &graph,
        operation: graph.operations().next().unwrap(),
        physical: &[],
    };
    let mut other = GraphBuilder::new(GraphLimits::default()).unwrap();
    let foreign = other.reserve_resource(buffer_resource(16)).unwrap();
    let foreign = WgpuProgramBuffer::<Values>::new(foreign, buffer_descriptor(16));
    assert_eq!(first.version.index(), foreign.version.index());
    assert!(matches!(
        resources.buffer(&foreign),
        Err(WgpuRenderProgramError::Graph(GraphError::ForeignCandidate))
    ));
    assert!(matches!(
        resources.buffer(&second),
        Err(WgpuRenderProgramError::UndeclaredResource)
    ));
    // This one reaches the absent physical slot, proving the earlier errors did not just
    // reject every request because this deliberately empty native table has no resources.
    assert!(matches!(
        resources.buffer(&first),
        Err(WgpuRenderProgramError::Graph(GraphError::UnknownResource(
            0
        )))
    ));
}

#[test]
fn texture_accounting_uses_the_exact_native_format_and_checked_products() {
    let mut descriptor = texture_resource(1920, 1080);
    assert_eq!(resource_bytes(&descriptor).unwrap(), 1920 * 1080);
    if let ResourceDescriptor::Texture { native, .. } = &mut descriptor {
        native.format = wgpu::TextureFormat::Rgba16Float;
    }
    assert_eq!(resource_bytes(&descriptor).unwrap(), 1920 * 1080 * 8);
    if let ResourceDescriptor::Texture { native, .. } = &mut descriptor {
        native.size.width = u32::MAX;
        native.size.height = u32::MAX;
    }
    assert!(matches!(
        resource_bytes(&descriptor),
        Err(WgpuRenderProgramError::ResourceCapacity)
    ));
    assert_eq!(resource_bytes(&buffer_resource(4096)).unwrap(), 4096);
}

#[test]
fn realization_rejects_buffer_sizes_above_the_enabled_device_limit() {
    let limits = wgpu::Limits {
        max_buffer_size: 1024,
        ..Default::default()
    };
    assert!(validate_device_size(&buffer_resource(1024), &limits).is_ok());
    assert!(matches!(
        validate_device_size(&buffer_resource(1028), &limits),
        Err(WgpuRenderProgramError::DeviceResourceLimit {
            name: "max_buffer_size",
            required: 1028,
            enabled: 1024,
        })
    ));
}

#[test]
fn realization_rejects_each_texture_axis_above_the_enabled_device_limit() {
    let limits = wgpu::Limits {
        max_texture_dimension_2d: 1024,
        ..Default::default()
    };
    assert!(validate_device_size(&texture_resource(1024, 1024), &limits).is_ok());
    for (width, height, expected) in [(1025, 16, 1025), (16, 1026, 1026), (1027, 1028, 1028)] {
        assert!(matches!(
            validate_device_size(&texture_resource(width, height), &limits),
            Err(WgpuRenderProgramError::DeviceResourceLimit {
                name: "max_texture_dimension_2d",
                required,
                enabled: 1024,
            }) if required == expected
        ));
    }
}

#[test]
fn local_preflight_rolls_back_refusal_and_each_guard_follows_its_last_use() {
    let mut builder = GraphBuilder::new(GraphLimits::default()).unwrap();
    let first = builder.reserve_resource(buffer_resource(64)).unwrap();
    // An import between the locals must neither acquire a guard nor consume a local's guard.
    builder.import_resource(buffer_resource(64)).unwrap();
    let second = builder.reserve_resource(buffer_resource(64)).unwrap();
    let first = WgpuProgramBuffer::<Values>::new(first, buffer_descriptor(64));
    let second = WgpuProgramBuffer::<Values>::new(second, buffer_descriptor(64));
    builder
        .add_operation(
            1,
            &[],
            &[
                first.zero(AccessRole::StorageWrite).0,
                second.zero(AccessRole::StorageWrite).0,
            ],
        )
        .unwrap();
    builder.set_output_group(&[second.version.clone()]).unwrap();
    let graph = builder.compile().unwrap();

    let budget = Arc::new(OutputBudget::new(127));
    assert!(matches!(
        reserve_local_allocations(&graph, &budget, None),
        Err(WgpuRenderProgramError::ResourceCapacity)
    ));
    assert_eq!(budget.retained(), 0);
    let independent = budget.reserve(64).unwrap();
    assert_eq!(budget.retained(), 64);
    drop(independent);
    assert_eq!(budget.retained(), 0);

    let budget = Arc::new(OutputBudget::new(128));
    let mut allocations = reserve_local_allocations(&graph, &budget, None)
        .unwrap()
        .into_iter();
    assert_eq!(allocations.len(), 2);
    assert_eq!(budget.retained(), 128);
    assert!(budget.reserve(1).is_none());
    // The real storage's Arc shares each individual guard with its original submitted use.
    let scratch = Arc::new(allocations.next().unwrap());
    let output = Arc::new(allocations.next().unwrap());
    let submitted_scratch = Arc::clone(&scratch);
    let submitted_output = Arc::clone(&output);
    drop(allocations);
    drop(scratch);
    assert_eq!(budget.retained(), 128);
    drop(submitted_scratch);
    assert_eq!(budget.retained(), 64);
    drop(submitted_output);
    assert_eq!(budget.retained(), 64);
    // Keeping only the selected output must leave room for another producer's 64-byte local.
    let independent = budget.reserve(64).unwrap();
    assert_eq!(budget.retained(), 128);
    drop(independent);
    assert_eq!(budget.retained(), 64);
    drop(output);
    assert_eq!(budget.retained(), 0);
}

#[test]
fn host_local_target_requires_a_selected_local_texture_from_the_same_graph() {
    let mut builder = GraphBuilder::new(GraphLimits::default()).unwrap();
    let selected = builder.reserve_resource(texture_resource(8, 8)).unwrap();
    let unselected = builder.reserve_resource(texture_resource(8, 8)).unwrap();
    let imported = builder.import_resource(texture_resource(8, 8)).unwrap();
    let buffer = builder.reserve_resource(buffer_resource(16)).unwrap();
    builder
        .add_operation(
            1,
            &[],
            &[
                ResourceWrite {
                    version: selected.clone(),
                    role: AccessRole::Attachment,
                    initialization: Initialization::Clear(wgpu::Color::BLACK),
                },
                ResourceWrite {
                    version: unselected.clone(),
                    role: AccessRole::Attachment,
                    initialization: Initialization::Clear(wgpu::Color::BLACK),
                },
                ResourceWrite {
                    version: buffer.clone(),
                    role: AccessRole::StorageWrite,
                    initialization: Initialization::Zero,
                },
            ],
        )
        .unwrap();
    builder
        .set_output_group(&[selected.clone(), buffer.clone(), imported.clone()])
        .unwrap();
    let graph = builder.compile().unwrap();
    assert_eq!(
        validate_local_target_version(&graph, &selected).unwrap(),
        &texture_resource(8, 8)
    );
    assert!(matches!(
        validate_local_target_version(&graph, &unselected),
        Err(WgpuRenderProgramError::UnpublishedResource)
    ));
    assert!(matches!(
        validate_local_target_version(&graph, &imported),
        Err(WgpuRenderProgramError::Graph(GraphError::ImportedWrite(2)))
    ));
    assert!(matches!(
        validate_local_target_version(&graph, &buffer),
        Err(WgpuRenderProgramError::ResourceKind)
    ));
    let mut other = GraphBuilder::new(GraphLimits::default()).unwrap();
    let foreign = other.reserve_resource(texture_resource(8, 8)).unwrap();
    assert_eq!(selected.index(), foreign.index());
    assert!(matches!(
        validate_local_target_version(&graph, &foreign),
        Err(WgpuRenderProgramError::Graph(GraphError::ForeignCandidate))
    ));
}

#[test]
fn bound_local_target_skips_only_its_existing_charge_and_preserves_guard_order() {
    let mut builder = GraphBuilder::new(GraphLimits::default()).unwrap();
    let first = builder.reserve_resource(buffer_resource(32)).unwrap();
    let target = builder.reserve_resource(texture_resource(8, 8)).unwrap();
    builder.import_resource(texture_resource(8, 8)).unwrap();
    let last = builder.reserve_resource(buffer_resource(16)).unwrap();
    builder
        .add_operation(
            1,
            &[],
            &[
                ResourceWrite {
                    version: first,
                    role: AccessRole::StorageWrite,
                    initialization: Initialization::Zero,
                },
                ResourceWrite {
                    version: target.clone(),
                    role: AccessRole::Attachment,
                    initialization: Initialization::Clear(wgpu::Color::BLACK),
                },
                ResourceWrite {
                    version: last,
                    role: AccessRole::StorageWrite,
                    initialization: Initialization::Zero,
                },
            ],
        )
        .unwrap();
    builder.set_output_group(&[target.clone()]).unwrap();
    let graph = builder.compile().unwrap();
    let budget = Arc::new(OutputBudget::new(112));
    let image_guard = Arc::new(budget.reserve(64).unwrap());
    let program_guard = Arc::clone(&image_guard);
    assert!(matches!(
        reserve_local_allocations(&graph, &budget, None),
        Err(WgpuRenderProgramError::ResourceCapacity)
    ));
    assert_eq!(
        budget.retained(),
        64,
        "refusal rolls back only fresh guards"
    );
    let mut allocations = reserve_local_allocations(&graph, &budget, Some(&target))
        .unwrap()
        .into_iter();
    assert_eq!(allocations.len(), 2);
    assert_eq!(budget.retained(), 112);
    drop(allocations.next().unwrap());
    assert_eq!(budget.retained(), 80);
    drop(allocations.next().unwrap());
    assert_eq!(budget.retained(), 64);
    drop(program_guard);
    assert_eq!(budget.retained(), 64);
    drop(image_guard);
    assert_eq!(budget.retained(), 0);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_host_local_binding_validates_provenance_and_reuses_exact_storage() {
    let mut backend = crate::WgpuRenderBackend::default();
    let namespace = backend.prepare_image_device().unwrap();
    let source = nara_render::RenderFrameSourceGeneration::try_acquire().unwrap();
    let producer = WgpuRenderProducerId::new(1);
    let mut builder =
        super::super::WgpuRenderProgramBuilder::new(source, 7, GraphLimits::default()).unwrap();
    let ResourceDescriptor::Texture { native, .. } = texture_resource(8, 8) else {
        unreachable!()
    };
    let port = builder
        .texture::<Values>(super::super::WgpuProgramTextureDescriptor {
            native: native.clone(),
            transfer: TextureTransfer::Linear,
            alpha: TextureAlpha::Opaque,
        })
        .unwrap();
    builder
        .initialize(1, &[port.clear(wgpu::Color::WHITE, AccessRole::Attachment)])
        .unwrap();
    let program = builder.finish(&[port.output()]).unwrap();
    let budget = Arc::new(OutputBudget::new(64));
    let image_guard = Arc::new(budget.reserve(64).unwrap());
    let texture = namespace.device.create_texture(&native);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut target = HostProgramTexture {
        version: port.version.clone(),
        storage: Arc::new(ProgramResourceStorage {
            backend_instance_id: namespace.backend_instance_id,
            device_epoch: namespace.device_epoch,
            epoch_lost: Arc::clone(&namespace.epoch_lost),
            producer,
            source,
            frame_index: 7,
            descriptor: texture_resource(8, 8),
            native: NativeResource::Texture { texture, view },
            _reservation: Arc::clone(&image_guard),
        }),
    };
    let reject = |target: &HostProgramTexture| {
        assert!(matches!(
            validate_local_target(&program, &namespace, producer, target),
            Err(WgpuRenderProgramError::Graph(GraphError::IncompatibleRead(
                0
            )))
        ));
    };
    assert!(validate_local_target(&program, &namespace, producer, &target).is_ok());
    Arc::get_mut(&mut target.storage)
        .unwrap()
        .backend_instance_id ^= 1;
    reject(&target);
    Arc::get_mut(&mut target.storage)
        .unwrap()
        .backend_instance_id ^= 1;
    Arc::get_mut(&mut target.storage).unwrap().device_epoch ^= 1;
    reject(&target);
    Arc::get_mut(&mut target.storage).unwrap().device_epoch ^= 1;
    Arc::get_mut(&mut target.storage).unwrap().producer = WgpuRenderProducerId::new(2);
    reject(&target);
    Arc::get_mut(&mut target.storage).unwrap().producer = producer;
    Arc::get_mut(&mut target.storage).unwrap().source =
        nara_render::RenderFrameSourceGeneration::try_acquire().unwrap();
    reject(&target);
    Arc::get_mut(&mut target.storage).unwrap().source = source;
    Arc::get_mut(&mut target.storage).unwrap().frame_index = 8;
    reject(&target);
    Arc::get_mut(&mut target.storage).unwrap().frame_index = 7;
    Arc::get_mut(&mut target.storage).unwrap().descriptor = texture_resource(4, 8);
    reject(&target);
    Arc::get_mut(&mut target.storage).unwrap().descriptor = texture_resource(8, 8);
    let wrong_texture = namespace.device.create_texture(&native);
    let NativeResource::Texture { view, .. } =
        &mut Arc::get_mut(&mut target.storage).unwrap().native
    else {
        unreachable!()
    };
    let correct_view = std::mem::replace(
        view,
        wrong_texture.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    reject(&target);
    let NativeResource::Texture { view, .. } =
        &mut Arc::get_mut(&mut target.storage).unwrap().native
    else {
        unreachable!()
    };
    *view = correct_view;
    let mut states = NativeFeatureStates::default();
    let mut attempt = FeaturePreparationAttempt::default();
    let mut reservation = WgpuSubmissionReservation::new(&namespace.device, &namespace.queue, ());
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    let mut stock = crate::stock_quad::StockQuadResources::default();
    let encoded = prepare_and_encode(
        &program,
        &namespace,
        producer,
        &mut states,
        &budget,
        &mut reservation,
        &mut attempt,
        7,
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        &mut stock,
        Some(&target),
    )
    .unwrap();
    assert!(Arc::ptr_eq(&encoded.resources[0], &target.storage));
    assert_eq!(
        budget.retained(),
        64,
        "the image target is charged exactly once"
    );
    let prefix = reservation.submitted(namespace.queue.submit([encoded.commands]));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !prefix.poll().unwrap() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    drop(encoded.resources);
    drop(target);
    assert_eq!(
        budget.retained(),
        64,
        "the image owner still holds the shared guard"
    );
    drop(image_guard);
    assert_eq!(budget.retained(), 0);
}

#[test]
fn finite_native_vectors_reject_capacity_overflow_before_allocating() {
    assert!(matches!(
        reserved::<OutputReservation>(usize::MAX),
        Err(WgpuRenderProgramError::ResourceCapacity)
    ));
    assert!(matches!(
        reserved::<Arc<ProgramResourceStorage>>(usize::MAX),
        Err(WgpuRenderProgramError::ResourceCapacity)
    ));
    assert!(matches!(
        reserved::<ProgramOperationReport>(usize::MAX),
        Err(WgpuRenderProgramError::ResourceCapacity)
    ));
}
