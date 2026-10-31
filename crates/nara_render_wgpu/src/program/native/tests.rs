use super::*;

use crate::graph::{
    AccessRole, BufferMeaning, GraphBuilder, GraphLimits, TextureAlpha, TextureMeaning,
    TextureTransfer,
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
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
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
        reserve_local_allocations(&graph, &budget),
        Err(WgpuRenderProgramError::ResourceCapacity)
    ));
    assert_eq!(budget.retained(), 0);
    let independent = budget.reserve(64).unwrap();
    assert_eq!(budget.retained(), 64);
    drop(independent);
    assert_eq!(budget.retained(), 0);

    let budget = Arc::new(OutputBudget::new(128));
    let mut allocations = reserve_local_allocations(&graph, &budget)
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
