//! Real native imports must agree with the graph before allocation or adapter encoding.

#![forbid(unsafe_code)]

use std::sync::Arc;

#[path = "support/gpu.rs"]
mod gpu;
#[path = "support/graph.rs"]
mod graph;
#[path = "support/native.rs"]
mod native;
#[path = "support/workload.rs"]
mod workload;

#[test]
#[ignore = "requires a real native GPU adapter; explicitly select and run serially"]
fn imports_reject_wrong_actual_shape_usage_and_foreign_candidate_before_encoding() {
    let gpu = gpu::Gpu::new();
    for wrong_shape in [true, false] {
        let workload = workload::build(&workload::OPERATION_KEYS).unwrap();
        let graph::ResourceDesc::Texture {
            native: expected, ..
        } = &workload
            .graph
            .resource(&workload.versions.original)
            .unwrap()
            .desc
        else {
            unreachable!()
        };
        let mut descriptor = expected.clone();
        if wrong_shape {
            descriptor.size.width = 32;
        } else {
            descriptor.usage.remove(wgpu::TextureUsages::COPY_SRC);
        }
        let actual = Arc::new(gpu.device.create_texture(&descriptor));
        let imports = [(
            workload.versions.original,
            native::NativeResource::Texture(actual),
        )];
        let mut encoded = false;
        let result = native::prepare(&gpu, workload.graph, &imports, |_, _, _| {
            encoded = true;
            Ok(())
        });
        assert!(!encoded);
        assert!(matches!(
            (wrong_shape, result),
            (true, Err(native::NativeError::ImportShape(_)))
                | (false, Err(native::NativeError::ImportUsage(_)))
        ));
    }
    let selected = workload::build(&workload::OPERATION_KEYS).unwrap();
    let foreign = workload::build(&workload::OPERATION_KEYS).unwrap();
    let graph::ResourceDesc::Texture {
        native: descriptor, ..
    } = &selected
        .graph
        .resource(&selected.versions.original)
        .unwrap()
        .desc
    else {
        unreachable!()
    };
    let actual = Arc::new(gpu.device.create_texture(descriptor));
    let imports = [(
        foreign.versions.original,
        native::NativeResource::Texture(actual),
    )];
    let mut encoded = false;
    let result = native::prepare(&gpu, selected.graph, &imports, |_, _, _| {
        encoded = true;
        Ok(())
    });
    assert!(!encoded);
    assert!(matches!(
        result,
        Err(native::NativeError::Graph(
            graph::GraphError::ForeignCandidate
        ))
    ));
}

#[test]
#[ignore = "real native GPU submission validation failure after Ready; run serially"]
fn destroyed_exact_input_after_ready_rejects_submission_and_keeps_the_device_usable() {
    let gpu = gpu::Gpu::new();
    let descriptor = wgpu::BufferDescriptor {
        label: Some("comparison-post-ready-input"),
        size: 16,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    };
    let input = Arc::new(gpu.device.create_buffer(&descriptor));
    gpu.queue.write_buffer(&input, 0, &[0x3c; 16]);
    let initial = gpu.queue.submit([]);
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(initial),
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    let mut candidate = graph::Candidate::new(graph::Limits::default()).unwrap();
    let desc = graph::ResourceDesc::Buffer {
        native: descriptor.clone(),
        meaning: graph::BufferMeaning { semantic: 1 },
    };
    let source = candidate
        .import_resource(desc.clone(), graph::ImportState::Initialized)
        .unwrap();
    let destination = candidate.reserve_resource(desc.clone()).unwrap();
    candidate
        .add_operation(
            1,
            &[graph::Read {
                version: source.clone(),
                role: graph::AccessRole::CopySource,
                expected: desc,
            }],
            &[graph::Write {
                version: destination.clone(),
                role: graph::AccessRole::CopyDestination,
                initialization: graph::Initialization::Zero,
            }],
        )
        .unwrap();
    candidate.set_output_group(&[destination.clone()]).unwrap();
    let imports = [(
        source.clone(),
        native::NativeResource::Buffer(Arc::clone(&input)),
    )];
    let ready = native::prepare(
        &gpu,
        candidate.compile().unwrap(),
        &imports,
        |_, resources, encoder| {
            encoder.copy_buffer_to_buffer(
                resources.buffer(&source)?,
                0,
                resources.buffer(&destination)?,
                0,
                16,
            );
            Ok(())
        },
    )
    .unwrap();
    // Deliberately violate the trusted import lifetime after complete Ready admission. This is
    // submission-time failure coverage, not evidence that failed submission had no physical uses.
    input.destroy();
    let failed = native::submit(&gpu, ready);
    assert!(
        matches!(
            failed,
            Err(native::NativeError::DeviceErrors {
                stage: "queue submission",
                validation: Some(_),
                ..
            })
        ),
        "{failed:?}"
    );
    drop((imports, input));
    // Drive the actual queue prefix after the failed attempt so conservatively retained native
    // custody can return. A content protocol failure does not require destroying the healthy device.
    let healthy = gpu.device.create_buffer(&descriptor);
    gpu.queue.write_buffer(&healthy, 0, &[0xa5; 16]);
    assert_eq!(gpu.read_buffer(&healthy), [0xa5; 16]);
}
