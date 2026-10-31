//! Source-level contract cases. Native buffer/texture execution is qualified by the parent tracer.

use super::graph::*;

const SCENE: u64 = 1;
const COMPOSITE: u64 = 2;
const RAW_COUNTS: u64 = 3;
const NORMALIZED_FREQUENCIES: u64 = 4;

fn candidate() -> Candidate {
    Candidate::new(Limits::default()).unwrap()
}

fn texture(semantic: u64) -> ResourceDesc {
    ResourceDesc::Texture {
        native: wgpu::TextureDescriptor {
            label: Some("graph-contract-texture"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        range: wgpu::ImageSubresourceRange::default(),
        meaning: TextureMeaning {
            semantic,
            transfer: ColorTransfer::Linear,
            alpha: AlphaMode::Premultiplied,
        },
    }
}

fn buffer() -> ResourceDesc {
    buffer_with_semantic(RAW_COUNTS)
}

fn buffer_with_semantic(semantic: u64) -> ResourceDesc {
    ResourceDesc::Buffer {
        native: wgpu::BufferDescriptor {
            label: Some("graph-contract-buffer"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::UNIFORM
                | wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::INDEX
                | wgpu::BufferUsages::INDIRECT,
            mapped_at_creation: false,
        },
        meaning: BufferMeaning { semantic },
    }
}

fn read(version: &Version, role: AccessRole, expected: &ResourceDesc) -> Read {
    Read {
        version: version.clone(),
        role,
        expected: expected.clone(),
    }
}

fn write(version: &Version, role: AccessRole, initialization: Initialization) -> Write {
    Write {
        version: version.clone(),
        role,
        initialization,
    }
}

fn clear(version: &Version) -> Write {
    write(
        version,
        AccessRole::Attachment,
        Initialization::Clear(wgpu::Color::TRANSPARENT),
    )
}

fn keys(graph: &CompiledGraph) -> Vec<OperationKey> {
    graph.operations().map(|operation| operation.key).collect()
}

#[test]
fn branching_exact_versions_survive_resource_and_operation_declaration_reordering() {
    let declarations = [
        [0, 1, 2, 3, 4],
        [4, 3, 2, 1, 0],
        [2, 4, 1, 3, 0],
        [3, 1, 4, 0, 2],
        [4, 0, 3, 2, 1],
        [1, 3, 0, 4, 2],
    ];
    for reservation_order in [[0, 1, 2, 3, 4], [4, 2, 0, 3, 1]] {
        for declaration_order in declarations {
            let mut graph = candidate();
            let mut versions: [Option<Version>; 5] = std::array::from_fn(|_| None);
            for index in reservation_order {
                let semantic = if index == 4 { COMPOSITE } else { SCENE };
                versions[index] = Some(graph.reserve_resource(texture(semantic)).unwrap());
            }
            let versions = versions.map(Option::unwrap);
            let scene = texture(SCENE);
            let operations = [
                (90, vec![], vec![clear(&versions[0])]),
                (
                    70,
                    vec![read(&versions[0], AccessRole::Sampled, &scene)],
                    vec![clear(&versions[1])],
                ),
                (
                    20,
                    vec![read(&versions[1], AccessRole::Sampled, &scene)],
                    vec![clear(&versions[2])],
                ),
                (
                    40,
                    vec![read(&versions[0], AccessRole::Sampled, &scene)],
                    vec![clear(&versions[3])],
                ),
                (
                    10,
                    vec![
                        read(&versions[0], AccessRole::Sampled, &scene),
                        read(&versions[2], AccessRole::Sampled, &scene),
                        read(&versions[3], AccessRole::Sampled, &scene),
                    ],
                    vec![clear(&versions[4])],
                ),
            ];
            // Set publication intent before any producer is declared as a further order challenge.
            graph
                .set_output_group(&[versions[0].clone(), versions[4].clone()])
                .unwrap();
            for index in declaration_order {
                let (key, reads, writes) = &operations[index];
                graph.add_operation(*key, reads, writes).unwrap();
            }
            let graph = graph.compile().unwrap();
            assert_eq!(keys(&graph), [90, 40, 70, 20, 10]);
            assert_eq!(graph.outputs(), &[versions[0].clone(), versions[4].clone()]);
            assert_eq!(graph.copy_dependencies().count(), 0);
            assert_eq!(graph.resources().len(), 5);
            assert_eq!(graph.resource(&versions[0]).unwrap().desc, scene);
            assert_eq!(graph.modeled_resource_bytes(), 5 * 64 * 64 * 16);
        }
    }
}

#[test]
fn ready_operations_use_stable_keys_instead_of_insertion_order() {
    for insertion in [[30, 10, 20], [20, 30, 10], [10, 20, 30]] {
        let mut graph = candidate();
        let desc = texture(SCENE);
        let input = graph
            .import_resource(desc.clone(), ImportState::Initialized)
            .unwrap();
        for key in insertion {
            graph
                .add_operation(key, &[read(&input, AccessRole::Sampled, &desc)], &[])
                .unwrap();
        }
        graph.set_output_group(&[input]).unwrap();
        assert_eq!(keys(&graph.compile().unwrap()), [10, 20, 30]);
    }
}

#[test]
fn compute_statistics_preserve_raw_counts_and_feed_a_vertex_consumer() {
    let mut graph = candidate();
    let image = texture(SCENE);
    let raw_data = buffer();
    let normalized_data = buffer_with_semantic(NORMALIZED_FREQUENCIES);
    let source = graph
        .import_resource(image.clone(), ImportState::Initialized)
        .unwrap();
    let raw = graph.reserve_resource(raw_data.clone()).unwrap();
    let normalized = graph.reserve_resource(normalized_data.clone()).unwrap();
    let histogram = graph.reserve_resource(texture(COMPOSITE)).unwrap();
    graph
        .add_operation(
            30,
            &[
                read(&normalized, AccessRole::Vertex, &normalized_data),
                read(&raw, AccessRole::Vertex, &raw_data),
            ],
            &[clear(&histogram)],
        )
        .unwrap();
    graph
        .add_operation(
            20,
            &[
                read(&raw, AccessRole::StorageRead, &raw_data),
                // Repeated dependencies, including the implicit Preserve read, must all decrement.
                read(&raw, AccessRole::CopySource, &raw_data),
            ],
            &[write(
                &normalized,
                AccessRole::StorageReadWrite,
                Initialization::Preserve(raw.clone()),
            )],
        )
        .unwrap();
    graph
        .add_operation(
            10,
            &[read(&source, AccessRole::Sampled, &image)],
            &[write(
                &raw,
                AccessRole::StorageReadWrite,
                Initialization::Zero,
            )],
        )
        .unwrap();
    graph
        .set_output_group(&[raw.clone(), normalized.clone(), histogram])
        .unwrap();
    let graph = graph.compile().unwrap();
    assert_eq!(keys(&graph), [10, 20, 30]);
    assert_ne!(raw, normalized);
    assert_ne!(raw.index(), normalized.index());
    assert_eq!(graph.resource(&raw).unwrap().desc, raw_data);
    assert_eq!(graph.resource(&normalized).unwrap().desc, normalized_data);
    assert_eq!(
        graph.copy_dependencies().collect::<Vec<_>>(),
        [CopyDependency {
            source: raw.clone(),
            destination: normalized,
            before: 20,
        }]
    );
    assert_eq!(
        graph
            .operations()
            .filter(|operation| { operation.writes.iter().any(|write| write.version == raw) })
            .count(),
        1
    );
    assert!(graph.resource(&source).unwrap().is_imported());
    assert!(!graph.resource(&raw).unwrap().is_imported());
}

#[test]
fn preserve_imported_texture_is_an_explicit_copy_to_a_distinct_allocation() {
    let mut graph = candidate();
    let mut imported = texture(SCENE);
    let ResourceDesc::Texture { native, .. } = &mut imported else {
        unreachable!()
    };
    native.usage = wgpu::TextureUsages::COPY_SRC;
    let source = graph
        .import_resource(imported, ImportState::Initialized)
        .unwrap();
    let mut output = texture(SCENE);
    let ResourceDesc::Texture { native, .. } = &mut output else {
        unreachable!()
    };
    native.usage = wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::STORAGE_BINDING;
    let destination = graph.reserve_resource(output).unwrap();
    graph
        .add_operation(
            7,
            &[],
            &[write(
                &destination,
                AccessRole::StorageWrite,
                Initialization::Preserve(source.clone()),
            )],
        )
        .unwrap();
    graph
        .set_output_group(&[source.clone(), destination.clone()])
        .unwrap();
    let graph = graph.compile().unwrap();
    assert_eq!(
        graph.copy_dependencies().collect::<Vec<_>>(),
        [CopyDependency {
            source,
            destination,
            before: 7,
        }]
    );
}

#[test]
fn preservation_chain_is_order_independent_and_does_not_create_in_place_writes() {
    let mut graph = candidate();
    let first = graph.reserve_resource(buffer()).unwrap();
    let second = graph.reserve_resource(buffer()).unwrap();
    let third = graph.reserve_resource(buffer()).unwrap();
    for (key, output, initialization) in [
        (1, &third, Initialization::Preserve(second.clone())),
        (2, &second, Initialization::Preserve(first.clone())),
        (3, &first, Initialization::Zero),
    ] {
        graph
            .add_operation(
                key,
                &[],
                &[write(output, AccessRole::StorageReadWrite, initialization)],
            )
            .unwrap();
    }
    graph
        .set_output_group(&[first.clone(), second.clone(), third.clone()])
        .unwrap();
    let graph = graph.compile().unwrap();
    assert_eq!(keys(&graph), [3, 2, 1]);
    assert_eq!(
        graph.copy_dependencies().collect::<Vec<_>>(),
        [
            CopyDependency {
                source: first,
                destination: second.clone(),
                before: 2
            },
            CopyDependency {
                source: second,
                destination: third,
                before: 1
            },
        ]
    );
}

#[test]
fn initialized_import_can_be_the_complete_output_without_local_operations() {
    let limits = Limits {
        max_operations: 0,
        max_accesses: 0,
        ..Limits::default()
    };
    let mut graph = Candidate::new(limits).unwrap();
    let input = graph
        .import_resource(buffer(), ImportState::Initialized)
        .unwrap();
    graph.set_output_group(&[input.clone()]).unwrap();
    let graph = graph.compile().unwrap();
    assert_eq!(graph.operations().len(), 0);
    assert_eq!(graph.outputs(), &[input]);
}

#[test]
fn uninitialized_import_is_rejected_without_reserving_a_version() {
    let mut graph = candidate();
    assert_eq!(
        graph.import_resource(buffer(), ImportState::Uninitialized),
        Err(GraphError::UninitializedImport)
    );
    let input = graph
        .import_resource(buffer(), ImportState::Initialized)
        .unwrap();
    assert_eq!(input.index(), 0);
    graph.set_output_group(&[input]).unwrap();
    graph.compile().unwrap();
}

#[test]
fn foreign_handles_reject_in_every_position_even_after_original_candidate_drops() {
    let foreign = {
        let mut original = candidate();
        original
            .import_resource(buffer(), ImportState::Initialized)
            .unwrap()
    };
    // The retained Arc keeps the original identity allocation alive. Slot zero is reusable;
    // candidate ownership is not, regardless of allocator address reuse after other candidates drop.
    for _ in 0..16 {
        let mut graph = candidate();
        let input = graph
            .import_resource(buffer(), ImportState::Initialized)
            .unwrap();
        let output = graph.reserve_resource(buffer()).unwrap();
        assert_eq!(foreign.index(), input.index());
        assert_ne!(foreign, input);
        assert_eq!(
            graph.add_operation(
                1,
                &[read(&foreign, AccessRole::StorageRead, &buffer())],
                &[]
            ),
            Err(GraphError::ForeignCandidate)
        );
        assert_eq!(
            graph.add_operation(
                1,
                &[],
                &[write(
                    &foreign,
                    AccessRole::StorageWrite,
                    Initialization::Zero
                )]
            ),
            Err(GraphError::ForeignCandidate)
        );
        assert_eq!(
            graph.add_operation(
                1,
                &[],
                &[write(
                    &output,
                    AccessRole::StorageWrite,
                    Initialization::Preserve(foreign.clone())
                ),]
            ),
            Err(GraphError::ForeignCandidate)
        );
        assert_eq!(
            graph.set_output_group(&[foreign.clone()]),
            Err(GraphError::ForeignCandidate)
        );
        graph
            .add_operation(
                1,
                &[],
                &[write(
                    &output,
                    AccessRole::StorageWrite,
                    Initialization::Zero,
                )],
            )
            .unwrap();
        graph.set_output_group(&[output]).unwrap();
        let graph = graph.compile().unwrap();
        assert_eq!(
            graph.resource(&foreign).unwrap_err(),
            GraphError::ForeignCandidate
        );
    }
}

#[test]
fn cross_candidate_reuse_requires_an_explicit_new_initialized_import() {
    let mut first = candidate();
    let previous = first
        .import_resource(texture(SCENE), ImportState::Initialized)
        .unwrap();
    first.set_output_group(&[previous.clone()]).unwrap();
    let first = first.compile().unwrap();
    let mut next = candidate();
    let imported = next
        .import_resource(
            first.resource(&previous).unwrap().desc.clone(),
            ImportState::Initialized,
        )
        .unwrap();
    assert_ne!(previous, imported);
    next.set_output_group(&[imported.clone()]).unwrap();
    let next = next.compile().unwrap();
    assert!(next.resource(&imported).unwrap().is_imported());
    assert_eq!(
        next.resource(&previous).unwrap_err(),
        GraphError::ForeignCandidate
    );
}

#[test]
fn repeated_output_in_one_operation_rejects_without_claiming_a_producer() {
    let mut graph = candidate();
    let output = graph.reserve_resource(texture(SCENE)).unwrap();
    assert_eq!(
        graph.add_operation(1, &[], &[clear(&output), clear(&output)]),
        Err(GraphError::RepeatedProducer(output.index()))
    );
    graph.add_operation(1, &[], &[clear(&output)]).unwrap();
    graph.set_output_group(&[output]).unwrap();
    assert_eq!(keys(&graph.compile().unwrap()), [1]);
}

#[test]
fn exactly_one_operation_can_produce_a_reserved_version() {
    let mut graph = candidate();
    let output = graph.reserve_resource(texture(SCENE)).unwrap();
    graph.add_operation(1, &[], &[clear(&output)]).unwrap();
    assert_eq!(
        graph.add_operation(2, &[], &[clear(&output)]),
        Err(GraphError::RepeatedProducer(output.index()))
    );
    graph.set_output_group(&[output]).unwrap();
    assert_eq!(keys(&graph.compile().unwrap()), [1]);
}

#[test]
fn duplicate_stable_key_rejects_without_claiming_the_second_output() {
    let mut graph = candidate();
    let first = graph.reserve_resource(texture(SCENE)).unwrap();
    let second = graph.reserve_resource(texture(SCENE)).unwrap();
    graph.add_operation(9, &[], &[clear(&first)]).unwrap();
    assert_eq!(
        graph.add_operation(9, &[], &[clear(&second)]),
        Err(GraphError::DuplicateOperationKey(9))
    );
    graph.add_operation(8, &[], &[clear(&second)]).unwrap();
    graph.set_output_group(&[first, second]).unwrap();
    assert_eq!(keys(&graph.compile().unwrap()), [8, 9]);
}

#[test]
fn imported_resources_never_accept_writes_including_storage_read_write() {
    for role in [
        AccessRole::StorageWrite,
        AccessRole::StorageReadWrite,
        AccessRole::CopyDestination,
    ] {
        let mut graph = candidate();
        let input = graph
            .import_resource(buffer(), ImportState::Initialized)
            .unwrap();
        assert_eq!(
            graph.add_operation(1, &[], &[write(&input, role, Initialization::Zero)]),
            Err(GraphError::ImportedWrite(input.index()))
        );
    }
    let mut graph = candidate();
    let input = graph
        .import_resource(texture(SCENE), ImportState::Initialized)
        .unwrap();
    assert_eq!(
        graph.add_operation(1, &[], &[clear(&input)]),
        Err(GraphError::ImportedWrite(input.index()))
    );
}

#[test]
fn same_version_read_and_write_is_feedback_even_with_a_clear_or_storage_role() {
    for role in [AccessRole::StorageWrite, AccessRole::StorageReadWrite] {
        let mut graph = candidate();
        let output = graph.reserve_resource(buffer()).unwrap();
        assert_eq!(
            graph.add_operation(
                1,
                &[read(&output, AccessRole::StorageRead, &buffer())],
                &[write(&output, role, Initialization::Zero),]
            ),
            Err(GraphError::ReadWriteAlias(output.index()))
        );
    }
}

#[test]
fn preservation_cannot_read_any_output_of_the_same_operation() {
    let mut graph = candidate();
    let first = graph.reserve_resource(buffer()).unwrap();
    let second = graph.reserve_resource(buffer()).unwrap();
    assert_eq!(
        graph.add_operation(
            1,
            &[],
            &[write(
                &first,
                AccessRole::StorageReadWrite,
                Initialization::Preserve(first.clone())
            ),]
        ),
        Err(GraphError::ReadWriteAlias(first.index()))
    );
    assert_eq!(
        graph.add_operation(
            1,
            &[],
            &[
                write(&first, AccessRole::StorageWrite, Initialization::Zero),
                write(
                    &second,
                    AccessRole::StorageWrite,
                    Initialization::Preserve(first.clone())
                ),
            ]
        ),
        Err(GraphError::ReadWriteAlias(first.index()))
    );
    graph
        .add_operation(
            1,
            &[],
            &[write(
                &first,
                AccessRole::StorageWrite,
                Initialization::Zero,
            )],
        )
        .unwrap();
    graph
        .add_operation(
            2,
            &[],
            &[write(
                &second,
                AccessRole::StorageWrite,
                Initialization::Preserve(first.clone()),
            )],
        )
        .unwrap();
    graph.set_output_group(&[first, second]).unwrap();
    graph.compile().unwrap();
}

#[test]
fn ordinary_read_cycle_rejects_the_complete_candidate() {
    let mut graph = candidate();
    let desc = texture(SCENE);
    let first = graph.reserve_resource(desc.clone()).unwrap();
    let second = graph.reserve_resource(desc.clone()).unwrap();
    let independent = graph
        .import_resource(desc.clone(), ImportState::Initialized)
        .unwrap();
    graph
        .add_operation(
            1,
            &[read(&second, AccessRole::Sampled, &desc)],
            &[clear(&first)],
        )
        .unwrap();
    graph
        .add_operation(
            2,
            &[read(&first, AccessRole::Sampled, &desc)],
            &[clear(&second)],
        )
        .unwrap();
    graph
        .set_output_group(&[independent, first, second])
        .unwrap();
    assert_eq!(graph.compile().unwrap_err(), GraphError::Cycle);
}

#[test]
fn implicit_copy_dependencies_participate_in_cycle_detection() {
    let mut graph = candidate();
    let first = graph.reserve_resource(buffer()).unwrap();
    let second = graph.reserve_resource(buffer()).unwrap();
    graph
        .add_operation(
            1,
            &[],
            &[write(
                &first,
                AccessRole::StorageReadWrite,
                Initialization::Preserve(second.clone()),
            )],
        )
        .unwrap();
    graph
        .add_operation(
            2,
            &[],
            &[write(
                &second,
                AccessRole::StorageReadWrite,
                Initialization::Preserve(first.clone()),
            )],
        )
        .unwrap();
    graph.set_output_group(&[first, second]).unwrap();
    assert_eq!(graph.compile().unwrap_err(), GraphError::Cycle);
}

#[test]
fn missing_producer_is_not_repaired_by_declaration_order_or_an_independent_output() {
    for missing_is_output in [true, false] {
        let mut graph = candidate();
        let missing = graph.reserve_resource(texture(SCENE)).unwrap();
        let input = graph
            .import_resource(texture(SCENE), ImportState::Initialized)
            .unwrap();
        graph
            .add_operation(
                1,
                &[read(&missing, AccessRole::Sampled, &texture(SCENE))],
                &[],
            )
            .unwrap();
        graph
            .set_output_group(&[if missing_is_output {
                missing.clone()
            } else {
                input
            }])
            .unwrap();
        assert_eq!(
            graph.compile().unwrap_err(),
            GraphError::MissingProducer(missing.index())
        );
    }
}

#[test]
fn even_unused_reservations_require_a_producer_in_the_compiled_candidate() {
    let mut graph = candidate();
    let unused = graph.reserve_resource(buffer()).unwrap();
    let complete = graph
        .import_resource(buffer(), ImportState::Initialized)
        .unwrap();
    graph.set_output_group(&[complete]).unwrap();
    assert_eq!(
        graph.compile().unwrap_err(),
        GraphError::MissingProducer(unused.index())
    );
}

#[test]
fn output_group_is_explicit_nonempty_unique_and_replaced_only_after_validation() {
    assert_eq!(
        candidate().compile().unwrap_err(),
        GraphError::MissingOutputGroup
    );
    let mut graph = candidate();
    let input = graph
        .import_resource(buffer(), ImportState::Initialized)
        .unwrap();
    graph.set_output_group(&[input.clone()]).unwrap();
    assert_eq!(
        graph.set_output_group(&[]),
        Err(GraphError::MissingOutputGroup)
    );
    assert_eq!(
        graph.set_output_group(&[input.clone(), input.clone()]),
        Err(GraphError::DuplicateOutput(input.index()))
    );
    assert_eq!(graph.compile().unwrap().outputs(), &[input]);
}

#[test]
fn semantic_transfer_and_alpha_mismatches_reject_identical_native_descriptors() {
    for mismatch in 0..3 {
        let mut graph = candidate();
        let actual = texture(SCENE);
        let input = graph
            .import_resource(actual.clone(), ImportState::Initialized)
            .unwrap();
        let mut expected = actual;
        let ResourceDesc::Texture { meaning, .. } = &mut expected else {
            unreachable!()
        };
        match mismatch {
            0 => meaning.semantic = COMPOSITE,
            1 => meaning.transfer = ColorTransfer::Srgb,
            _ => meaning.alpha = AlphaMode::Straight,
        }
        assert_eq!(
            graph.add_operation(1, &[read(&input, AccessRole::Sampled, &expected)], &[]),
            Err(GraphError::IncompatibleRead(input.index()))
        );
    }
}

#[test]
fn buffer_reads_reject_raw_counts_and_normalized_frequencies_with_identical_native_shape() {
    for (actual_semantic, expected_semantic) in [
        (RAW_COUNTS, NORMALIZED_FREQUENCIES),
        (NORMALIZED_FREQUENCIES, RAW_COUNTS),
    ] {
        for role in [AccessRole::StorageRead, AccessRole::Vertex] {
            let mut graph = candidate();
            // Both ports are 16 bytes and admit both usages; only their meanings differ.
            let actual = buffer_with_semantic(actual_semantic);
            let expected = buffer_with_semantic(expected_semantic);
            let input = graph
                .import_resource(actual.clone(), ImportState::Initialized)
                .unwrap();
            assert_eq!(
                graph.add_operation(1, &[read(&input, role, &expected)], &[]),
                Err(GraphError::IncompatibleRead(input.index()))
            );
            graph
                .add_operation(1, &[read(&input, role, &actual)], &[])
                .unwrap();
            graph.set_output_group(&[input]).unwrap();
            assert_eq!(keys(&graph.compile().unwrap()), [1]);
        }
    }
}

#[test]
fn consumers_name_scene_color_even_when_final_composite_has_the_same_native_shape() {
    let mut graph = candidate();
    let final_color = graph
        .import_resource(texture(COMPOSITE), ImportState::Initialized)
        .unwrap();
    let scene = graph
        .import_resource(texture(SCENE), ImportState::Initialized)
        .unwrap();
    assert_eq!(
        graph.add_operation(
            1,
            &[read(&final_color, AccessRole::Sampled, &texture(SCENE))],
            &[]
        ),
        Err(GraphError::IncompatibleRead(final_color.index()))
    );
    graph
        .add_operation(
            1,
            &[read(&scene, AccessRole::Sampled, &texture(SCENE))],
            &[],
        )
        .unwrap();
    graph
        .set_output_group(&[scene.clone(), final_color])
        .unwrap();
    let graph = graph.compile().unwrap();
    assert_eq!(graph.operations().next().unwrap().reads[0].version, scene);
}

#[test]
fn consumer_shape_kind_and_permitted_usage_are_checked() {
    for mismatch in 0..4 {
        let mut graph = candidate();
        let mut actual = texture(SCENE);
        let ResourceDesc::Texture { native, .. } = &mut actual else {
            unreachable!()
        };
        native.usage = wgpu::TextureUsages::TEXTURE_BINDING;
        let input = graph
            .import_resource(actual.clone(), ImportState::Initialized)
            .unwrap();
        let mut expected = actual;
        match mismatch {
            0 => {
                let ResourceDesc::Texture { native, .. } = &mut expected else {
                    unreachable!()
                };
                native.size.width = 32;
            }
            1 => {
                let ResourceDesc::Texture { native, .. } = &mut expected else {
                    unreachable!()
                };
                native.format = wgpu::TextureFormat::Rgba16Float;
            }
            2 => expected = buffer(),
            _ => {
                let ResourceDesc::Texture { native, .. } = &mut expected else {
                    unreachable!()
                };
                native.usage |= wgpu::TextureUsages::COPY_SRC;
            }
        }
        assert_eq!(
            graph.add_operation(1, &[read(&input, AccessRole::Sampled, &expected)], &[]),
            Err(GraphError::IncompatibleRead(input.index()))
        );
    }
}

#[test]
fn full_range_spellings_and_debug_labels_do_not_change_compatibility() {
    let mut graph = candidate();
    let actual = texture(SCENE);
    let input = graph
        .import_resource(actual.clone(), ImportState::Initialized)
        .unwrap();
    let mut expected = actual;
    let ResourceDesc::Texture { native, range, .. } = &mut expected else {
        unreachable!()
    };
    native.label = Some("different-debug-label");
    native.usage = wgpu::TextureUsages::TEXTURE_BINDING;
    range.mip_level_count = Some(1);
    range.array_layer_count = Some(1);
    graph
        .add_operation(1, &[read(&input, AccessRole::Sampled, &expected)], &[])
        .unwrap();
    graph.set_output_group(&[input]).unwrap();
    graph.compile().unwrap();
}

#[test]
fn preserve_scene_pre_ui_into_final_composite_keeps_consumer_meanings_strict() {
    let mut graph = candidate();
    let scene_desc = texture(SCENE);
    let composite_desc = texture(COMPOSITE);
    let scene = graph
        .import_resource(scene_desc.clone(), ImportState::Initialized)
        .unwrap();
    let composite = graph.reserve_resource(composite_desc.clone()).unwrap();

    assert_eq!(
        graph.add_operation(
            10,
            &[read(&composite, AccessRole::Sampled, &scene_desc),],
            &[]
        ),
        Err(GraphError::IncompatibleRead(composite.index()))
    );
    graph
        .add_operation(
            10,
            &[read(&composite, AccessRole::Sampled, &composite_desc)],
            &[],
        )
        .unwrap();
    // Copy the pre-UI image, then let this operation add the final composite's content.
    graph
        .add_operation(
            20,
            &[],
            &[write(
                &composite,
                AccessRole::Attachment,
                Initialization::Preserve(scene.clone()),
            )],
        )
        .unwrap();
    graph
        .set_output_group(&[scene.clone(), composite.clone()])
        .unwrap();
    let graph = graph.compile().unwrap();
    assert_eq!(keys(&graph), [20, 10]);
    assert_eq!(
        graph.copy_dependencies().collect::<Vec<_>>(),
        [CopyDependency {
            source: scene.clone(),
            destination: composite.clone(),
            before: 20,
        }]
    );
    assert_eq!(graph.resource(&scene).unwrap().desc, scene_desc);
    assert_eq!(graph.resource(&composite).unwrap().desc, composite_desc);
    assert!(graph.resource(&scene).unwrap().is_imported());
    assert!(!graph.resource(&composite).unwrap().is_imported());
}

#[test]
fn preservation_cannot_change_texture_shape_format_transfer_or_alpha() {
    for mismatch in 0..4 {
        let mut graph = candidate();
        let source = graph
            .import_resource(texture(SCENE), ImportState::Initialized)
            .unwrap();
        let mut desc = texture(SCENE);
        let ResourceDesc::Texture {
            native, meaning, ..
        } = &mut desc
        else {
            unreachable!()
        };
        match mismatch {
            0 => native.size.height = 32,
            1 => native.format = wgpu::TextureFormat::Rgba16Float,
            2 => meaning.transfer = ColorTransfer::Srgb,
            _ => meaning.alpha = AlphaMode::Opaque,
        }
        let output = graph.reserve_resource(desc).unwrap();
        assert_eq!(
            graph.add_operation(
                1,
                &[],
                &[write(
                    &output,
                    AccessRole::Attachment,
                    Initialization::Preserve(source)
                )]
            ),
            Err(GraphError::IncompatiblePreserve(output.index()))
        );
    }
}

#[test]
fn preservation_cannot_change_buffer_size_or_resource_kind() {
    for to_texture in [false, true] {
        let mut graph = candidate();
        let source = graph
            .import_resource(buffer(), ImportState::Initialized)
            .unwrap();
        let mut desc = if to_texture { texture(SCENE) } else { buffer() };
        if let ResourceDesc::Buffer { native, .. } = &mut desc {
            native.size = 32;
        }
        let output = graph.reserve_resource(desc).unwrap();
        let role = if to_texture {
            AccessRole::Attachment
        } else {
            AccessRole::StorageWrite
        };
        assert_eq!(
            graph.add_operation(
                1,
                &[],
                &[write(&output, role, Initialization::Preserve(source))]
            ),
            Err(GraphError::IncompatiblePreserve(output.index()))
        );
    }
}

#[test]
fn unsupported_texture_dimensions_samples_mips_layers_and_view_formats_reject() {
    for mismatch in 0..10 {
        let mut graph = candidate();
        let mut desc = texture(SCENE);
        let ResourceDesc::Texture { native, .. } = &mut desc else {
            unreachable!()
        };
        match mismatch {
            0 => native.size.width = 0,
            1 => native.size.height = 0,
            2 => native.size.depth_or_array_layers = 0,
            3 => native.size.depth_or_array_layers = 2,
            4 => native.sample_count = 0,
            5 => native.sample_count = 4,
            6 => native.mip_level_count = 0,
            7 => native.mip_level_count = 2,
            8 => native.dimension = wgpu::TextureDimension::D3,
            _ => native.view_formats = &[wgpu::TextureFormat::Rgba32Float],
        }
        assert_eq!(
            graph.reserve_resource(desc),
            Err(GraphError::UnsupportedTextureScope)
        );
    }
}

#[test]
fn partial_or_unsupported_subresource_ranges_reject_for_imports_and_consumers() {
    for mismatch in 0..7 {
        let mut graph = candidate();
        let input = graph
            .import_resource(texture(SCENE), ImportState::Initialized)
            .unwrap();
        let mut desc = texture(SCENE);
        let ResourceDesc::Texture { range, .. } = &mut desc else {
            unreachable!()
        };
        match mismatch {
            0 => range.aspect = wgpu::TextureAspect::DepthOnly,
            1 => range.base_mip_level = 1,
            2 => range.mip_level_count = Some(0),
            3 => range.mip_level_count = Some(2),
            4 => range.base_array_layer = 1,
            5 => range.array_layer_count = Some(0),
            _ => range.array_layer_count = Some(2),
        }
        assert_eq!(
            graph.import_resource(desc.clone(), ImportState::Initialized),
            Err(GraphError::UnsupportedTextureScope)
        );
        assert_eq!(
            graph.add_operation(1, &[read(&input, AccessRole::Sampled, &desc)], &[]),
            Err(GraphError::UnsupportedTextureScope)
        );
    }
}

#[test]
fn unsupported_formats_and_usage_flags_reject_before_resource_admission() {
    for format in [
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureFormat::Bc1RgbaUnorm,
        wgpu::TextureFormat::NV12,
    ] {
        let mut desc = texture(SCENE);
        let ResourceDesc::Texture { native, .. } = &mut desc else {
            unreachable!()
        };
        native.format = format;
        assert_eq!(
            candidate().reserve_resource(desc),
            Err(GraphError::UnsupportedTextureFormat)
        );
    }
    for usage in [
        wgpu::TextureUsages::empty(),
        wgpu::TextureUsages::from_bits_retain(1 << 31),
    ] {
        let mut desc = texture(SCENE);
        let ResourceDesc::Texture { native, .. } = &mut desc else {
            unreachable!()
        };
        native.usage = usage;
        assert_eq!(
            candidate().reserve_resource(desc),
            Err(GraphError::UnsupportedUsage)
        );
    }
    let mut desc = texture(SCENE);
    let ResourceDesc::Texture { native, .. } = &mut desc else {
        unreachable!()
    };
    native.format = wgpu::TextureFormat::Rgba8UnormSrgb;
    native.usage = wgpu::TextureUsages::STORAGE_BINDING;
    assert_eq!(
        candidate().reserve_resource(desc),
        Err(GraphError::UnsupportedUsage)
    );
}

#[test]
fn zero_unaligned_mapped_or_mappable_buffers_are_outside_the_admitted_scope() {
    for mismatch in 0..6 {
        let mut desc = buffer();
        let ResourceDesc::Buffer { native, .. } = &mut desc else {
            unreachable!()
        };
        match mismatch {
            0 => native.size = 0,
            1 => native.size = 15,
            2 => native.mapped_at_creation = true,
            3 => native.usage = wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            4 => native.usage = wgpu::BufferUsages::empty(),
            _ => native.usage = wgpu::BufferUsages::from_bits_retain(1 << 31),
        }
        let expected = if mismatch < 3 {
            GraphError::UnsupportedBufferScope
        } else {
            GraphError::UnsupportedUsage
        };
        assert_eq!(candidate().reserve_resource(desc), Err(expected));
    }
}

#[test]
fn each_supported_texture_read_and_write_role_is_admitted_with_native_flags() {
    let mut graph = candidate();
    let desc = texture(SCENE);
    let input = graph
        .import_resource(desc.clone(), ImportState::Initialized)
        .unwrap();
    for (key, role) in [
        AccessRole::Sampled,
        AccessRole::StorageRead,
        AccessRole::CopySource,
    ]
    .into_iter()
    .enumerate()
    {
        graph
            .add_operation(key as u64, &[read(&input, role, &desc)], &[])
            .unwrap();
    }
    for (key, role) in [
        AccessRole::Attachment,
        AccessRole::StorageWrite,
        AccessRole::CopyDestination,
    ]
    .into_iter()
    .enumerate()
    {
        let output = graph.reserve_resource(desc.clone()).unwrap();
        graph
            .add_operation(
                10 + key as u64,
                &[],
                &[write(
                    &output,
                    role,
                    Initialization::Clear(wgpu::Color::TRANSPARENT),
                )],
            )
            .unwrap();
    }
    graph.set_output_group(&[input]).unwrap();
    assert_eq!(graph.compile().unwrap().operations().len(), 6);
}

#[test]
fn each_supported_buffer_role_is_admitted_with_native_flags() {
    let mut graph = candidate();
    let desc = buffer();
    let input = graph
        .import_resource(desc.clone(), ImportState::Initialized)
        .unwrap();
    for (key, role) in [
        AccessRole::StorageRead,
        AccessRole::CopySource,
        AccessRole::Uniform,
        AccessRole::Vertex,
        AccessRole::Index,
        AccessRole::Indirect,
    ]
    .into_iter()
    .enumerate()
    {
        graph
            .add_operation(key as u64, &[read(&input, role, &desc)], &[])
            .unwrap();
    }
    for (key, role) in [
        AccessRole::StorageWrite,
        AccessRole::StorageReadWrite,
        AccessRole::CopyDestination,
    ]
    .into_iter()
    .enumerate()
    {
        let output = graph.reserve_resource(desc.clone()).unwrap();
        graph
            .add_operation(
                10 + key as u64,
                &[],
                &[write(&output, role, Initialization::Zero)],
            )
            .unwrap();
    }
    graph.set_output_group(&[input]).unwrap();
    assert_eq!(graph.compile().unwrap().operations().len(), 9);
}

#[test]
fn every_role_requires_its_native_usage_flag() {
    for role in [
        AccessRole::Sampled,
        AccessRole::StorageRead,
        AccessRole::CopySource,
    ] {
        let mut graph = candidate();
        let mut desc = texture(SCENE);
        let ResourceDesc::Texture { native, .. } = &mut desc else {
            unreachable!()
        };
        native.usage = wgpu::TextureUsages::COPY_DST;
        let input = graph
            .import_resource(desc.clone(), ImportState::Initialized)
            .unwrap();
        assert_eq!(
            graph.add_operation(1, &[read(&input, role, &desc)], &[]),
            Err(GraphError::MissingUsage {
                version: input.index(),
                role
            })
        );
    }
    for role in [
        AccessRole::Attachment,
        AccessRole::StorageWrite,
        AccessRole::StorageReadWrite,
        AccessRole::CopyDestination,
    ] {
        let mut graph = candidate();
        let mut desc = texture(SCENE);
        let ResourceDesc::Texture { native, .. } = &mut desc else {
            unreachable!()
        };
        native.format = wgpu::TextureFormat::R32Float;
        native.usage = wgpu::TextureUsages::TEXTURE_BINDING;
        let output = graph.reserve_resource(desc).unwrap();
        assert_eq!(
            graph.add_operation(
                1,
                &[],
                &[write(
                    &output,
                    role,
                    Initialization::Clear(wgpu::Color::TRANSPARENT)
                )]
            ),
            Err(GraphError::MissingUsage {
                version: output.index(),
                role
            })
        );
    }
    for role in [
        AccessRole::StorageRead,
        AccessRole::CopySource,
        AccessRole::Uniform,
        AccessRole::Vertex,
        AccessRole::Index,
        AccessRole::Indirect,
    ] {
        let mut graph = candidate();
        let mut desc = buffer();
        let ResourceDesc::Buffer { native, .. } = &mut desc else {
            unreachable!()
        };
        native.usage = wgpu::BufferUsages::COPY_DST;
        let input = graph
            .import_resource(desc.clone(), ImportState::Initialized)
            .unwrap();
        assert_eq!(
            graph.add_operation(1, &[read(&input, role, &desc)], &[]),
            Err(GraphError::MissingUsage {
                version: input.index(),
                role
            })
        );
    }
    for role in [
        AccessRole::StorageWrite,
        AccessRole::StorageReadWrite,
        AccessRole::CopyDestination,
    ] {
        let mut graph = candidate();
        let mut desc = buffer();
        let ResourceDesc::Buffer { native, .. } = &mut desc else {
            unreachable!()
        };
        native.usage = wgpu::BufferUsages::COPY_SRC;
        let output = graph.reserve_resource(desc).unwrap();
        assert_eq!(
            graph.add_operation(1, &[], &[write(&output, role, Initialization::Zero)]),
            Err(GraphError::MissingUsage {
                version: output.index(),
                role
            })
        );
    }
}

#[test]
fn access_direction_is_explicit_and_storage_read_write_is_only_a_new_output() {
    for role in [
        AccessRole::Attachment,
        AccessRole::StorageWrite,
        AccessRole::StorageReadWrite,
        AccessRole::CopyDestination,
    ] {
        let mut graph = candidate();
        let input = graph
            .import_resource(buffer(), ImportState::Initialized)
            .unwrap();
        assert_eq!(
            graph.add_operation(1, &[read(&input, role, &buffer())], &[]),
            Err(GraphError::IllegalAccess(role))
        );
    }
    for role in [
        AccessRole::Sampled,
        AccessRole::StorageRead,
        AccessRole::CopySource,
        AccessRole::Uniform,
        AccessRole::Vertex,
        AccessRole::Index,
        AccessRole::Indirect,
    ] {
        let mut graph = candidate();
        let output = graph.reserve_resource(buffer()).unwrap();
        assert_eq!(
            graph.add_operation(1, &[], &[write(&output, role, Initialization::Zero)]),
            Err(GraphError::IllegalAccess(role))
        );
    }
}

#[test]
fn texture_and_buffer_kinds_reject_each_others_roles() {
    for role in [
        AccessRole::Uniform,
        AccessRole::Vertex,
        AccessRole::Index,
        AccessRole::Indirect,
    ] {
        let mut graph = candidate();
        let desc = texture(SCENE);
        let input = graph
            .import_resource(desc.clone(), ImportState::Initialized)
            .unwrap();
        assert_eq!(
            graph.add_operation(1, &[read(&input, role, &desc)], &[]),
            Err(GraphError::IllegalKind(role))
        );
    }
    let mut graph = candidate();
    let input = graph
        .import_resource(buffer(), ImportState::Initialized)
        .unwrap();
    assert_eq!(
        graph.add_operation(1, &[read(&input, AccessRole::Sampled, &buffer())], &[]),
        Err(GraphError::IllegalKind(AccessRole::Sampled))
    );
    let output = graph.reserve_resource(buffer()).unwrap();
    assert_eq!(
        graph.add_operation(
            1,
            &[],
            &[write(&output, AccessRole::Attachment, Initialization::Zero)]
        ),
        Err(GraphError::IllegalKind(AccessRole::Attachment))
    );
}

#[test]
fn texture_storage_read_write_requires_native_format_support_and_a_fresh_initialized_output() {
    for format in [
        wgpu::TextureFormat::R32Float,
        wgpu::TextureFormat::Rgba32Float,
    ] {
        let mut graph = candidate();
        let mut desc = texture(SCENE);
        let ResourceDesc::Texture { native, .. } = &mut desc else {
            unreachable!()
        };
        native.format = format;
        let output = graph.reserve_resource(desc).unwrap();
        let result = graph.add_operation(
            1,
            &[],
            &[write(
                &output,
                AccessRole::StorageReadWrite,
                Initialization::Clear(wgpu::Color::TRANSPARENT),
            )],
        );
        if format == wgpu::TextureFormat::R32Float {
            result.unwrap();
            graph.set_output_group(&[output]).unwrap();
            graph.compile().unwrap();
        } else {
            assert_eq!(
                result,
                Err(GraphError::IllegalAccess(AccessRole::StorageReadWrite))
            );
        }
    }
}

#[test]
fn clear_and_zero_must_match_the_resource_kind_and_clear_values_must_be_finite() {
    let mut graph = candidate();
    let image = graph.reserve_resource(texture(SCENE)).unwrap();
    let data = graph.reserve_resource(buffer()).unwrap();
    assert_eq!(
        graph.add_operation(
            1,
            &[],
            &[write(&image, AccessRole::Attachment, Initialization::Zero)]
        ),
        Err(GraphError::InvalidInitialization(image.index()))
    );
    assert_eq!(
        graph.add_operation(
            1,
            &[],
            &[write(
                &data,
                AccessRole::StorageWrite,
                Initialization::Clear(wgpu::Color::TRANSPARENT)
            )]
        ),
        Err(GraphError::InvalidInitialization(data.index()))
    );
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            graph.add_operation(
                1,
                &[],
                &[write(
                    &image,
                    AccessRole::Attachment,
                    Initialization::Clear(wgpu::Color {
                        r: value,
                        ..wgpu::Color::TRANSPARENT
                    })
                ),]
            ),
            Err(GraphError::InvalidInitialization(image.index()))
        );
    }
    graph
        .add_operation(
            1,
            &[],
            &[
                clear(&image),
                write(&data, AccessRole::StorageWrite, Initialization::Zero),
            ],
        )
        .unwrap();
    graph.set_output_group(&[image, data]).unwrap();
    graph.compile().unwrap();
}

#[test]
fn initialization_has_native_usage_obligations_independent_of_the_kernel_role() {
    let mut graph = candidate();
    let mut desc = buffer();
    let ResourceDesc::Buffer { native, .. } = &mut desc else {
        unreachable!()
    };
    native.usage = wgpu::BufferUsages::STORAGE;
    let data = graph.reserve_resource(desc).unwrap();
    assert_eq!(
        graph.add_operation(
            1,
            &[],
            &[write(&data, AccessRole::StorageWrite, Initialization::Zero)]
        ),
        Err(GraphError::MissingUsage {
            version: data.index(),
            role: AccessRole::CopyDestination
        })
    );
    let mut desc = texture(SCENE);
    let ResourceDesc::Texture { native, .. } = &mut desc else {
        unreachable!()
    };
    native.usage = wgpu::TextureUsages::STORAGE_BINDING;
    let image = graph.reserve_resource(desc).unwrap();
    assert_eq!(
        graph.add_operation(
            1,
            &[],
            &[write(
                &image,
                AccessRole::StorageWrite,
                Initialization::Clear(wgpu::Color::TRANSPARENT)
            ),]
        ),
        Err(GraphError::MissingUsage {
            version: image.index(),
            role: AccessRole::Attachment
        })
    );
}

#[test]
fn preserve_requires_copy_source_and_destination_flags_without_adding_them_implicitly() {
    for missing_source in [true, false] {
        let mut graph = candidate();
        let mut source_desc = buffer();
        let mut output_desc = buffer();
        if missing_source {
            let ResourceDesc::Buffer { native, .. } = &mut source_desc else {
                unreachable!()
            };
            native.usage = wgpu::BufferUsages::STORAGE;
        } else {
            let ResourceDesc::Buffer { native, .. } = &mut output_desc else {
                unreachable!()
            };
            native.usage = wgpu::BufferUsages::STORAGE;
        }
        let source = graph
            .import_resource(source_desc, ImportState::Initialized)
            .unwrap();
        let output = graph.reserve_resource(output_desc).unwrap();
        assert_eq!(
            graph.add_operation(
                1,
                &[],
                &[write(
                    &output,
                    AccessRole::StorageReadWrite,
                    Initialization::Preserve(source.clone())
                ),]
            ),
            Err(GraphError::MissingUsage {
                version: if missing_source {
                    source.index()
                } else {
                    output.index()
                },
                role: if missing_source {
                    AccessRole::CopySource
                } else {
                    AccessRole::CopyDestination
                },
            })
        );
    }
}

#[test]
fn resource_and_operation_count_limits_reject_without_consuming_slots_or_keys() {
    let limits = Limits {
        max_resources: 1,
        max_operations: 1,
        ..Limits::default()
    };
    let mut graph = Candidate::new(limits).unwrap();
    let input = graph
        .import_resource(buffer(), ImportState::Initialized)
        .unwrap();
    assert_eq!(
        graph.reserve_resource(buffer()),
        Err(GraphError::LimitExceeded(LimitKind::Resources))
    );
    graph
        .add_operation(1, &[read(&input, AccessRole::Vertex, &buffer())], &[])
        .unwrap();
    assert_eq!(
        graph.add_operation(2, &[read(&input, AccessRole::Index, &buffer())], &[]),
        Err(GraphError::LimitExceeded(LimitKind::Operations))
    );
    graph.set_output_group(&[input]).unwrap();
    assert_eq!(keys(&graph.compile().unwrap()), [1]);
}

#[test]
fn access_limits_reject_before_copying_caller_inputs_or_claiming_a_producer() {
    let limits = Limits {
        max_accesses: 1,
        ..Limits::default()
    };
    let mut graph = Candidate::new(limits).unwrap();
    let source = graph
        .import_resource(buffer(), ImportState::Initialized)
        .unwrap();
    let output = graph.reserve_resource(buffer()).unwrap();
    let reads = vec![read(&source, AccessRole::StorageRead, &buffer())];
    let writes = vec![write(
        &output,
        AccessRole::StorageWrite,
        Initialization::Zero,
    )];
    assert_eq!(
        graph.add_operation(1, &reads, &writes),
        Err(GraphError::LimitExceeded(LimitKind::Accesses))
    );
    assert_eq!(reads[0].version, source);
    assert_eq!(writes[0].version, output);
    assert_eq!(
        graph.add_operation(
            1,
            &[],
            &[write(
                &output,
                AccessRole::StorageWrite,
                Initialization::Preserve(source)
            ),]
        ),
        Err(GraphError::LimitExceeded(LimitKind::Accesses))
    );
    graph.add_operation(1, &[], &writes).unwrap();
    graph.set_output_group(&[output]).unwrap();
    graph.compile().unwrap();
}

#[test]
fn output_count_limit_keeps_the_previous_complete_group() {
    let limits = Limits {
        max_outputs: 1,
        ..Limits::default()
    };
    let mut graph = Candidate::new(limits).unwrap();
    let first = graph
        .import_resource(buffer(), ImportState::Initialized)
        .unwrap();
    let second = graph
        .import_resource(buffer(), ImportState::Initialized)
        .unwrap();
    graph.set_output_group(&[first.clone()]).unwrap();
    assert_eq!(
        graph.set_output_group(&[first.clone(), second]),
        Err(GraphError::LimitExceeded(LimitKind::Outputs))
    );
    assert_eq!(graph.compile().unwrap().outputs(), &[first]);
}

#[test]
fn resource_byte_allowance_counts_imports_and_locals_and_rejection_preserves_remaining_capacity() {
    let limits = Limits {
        max_modeled_resource_bytes: 20,
        ..Limits::default()
    };
    let mut graph = Candidate::new(limits).unwrap();
    let input = graph
        .import_resource(buffer(), ImportState::Initialized)
        .unwrap();
    assert_eq!(
        graph.reserve_resource(buffer()),
        Err(GraphError::LimitExceeded(LimitKind::ResourceBytes))
    );
    let mut smaller = buffer();
    let ResourceDesc::Buffer { native, .. } = &mut smaller else {
        unreachable!()
    };
    native.size = 4;
    let output = graph.reserve_resource(smaller).unwrap();
    assert_eq!(output.index(), 1);
    graph
        .add_operation(
            1,
            &[],
            &[write(
                &output,
                AccessRole::StorageWrite,
                Initialization::Zero,
            )],
        )
        .unwrap();
    graph.set_output_group(&[input, output]).unwrap();
    assert_eq!(graph.compile().unwrap().modeled_resource_bytes(), 20);
}

#[test]
fn dimension_and_buffer_length_limits_apply_before_admission() {
    let limits = Limits {
        max_texture_dimension_2d: 32,
        max_buffer_bytes: 8,
        ..Limits::default()
    };
    let mut graph = Candidate::new(limits).unwrap();
    assert_eq!(
        graph.reserve_resource(texture(SCENE)),
        Err(GraphError::LimitExceeded(LimitKind::TextureDimension))
    );
    assert_eq!(
        graph.reserve_resource(buffer()),
        Err(GraphError::LimitExceeded(LimitKind::BufferBytes))
    );
}

#[test]
fn metadata_capacity_envelope_is_checked_before_allocation_and_includes_compile_scratch() {
    let mut limits = Limits::default();
    let envelope = limits.modeled_metadata_bytes().unwrap();
    limits.max_modeled_metadata_bytes = envelope - 1;
    assert_eq!(
        Candidate::new(limits).unwrap_err(),
        GraphError::LimitExceeded(LimitKind::MetadataBytes)
    );
    limits.max_modeled_metadata_bytes = envelope;
    let mut graph = Candidate::new(limits).unwrap();
    let input = graph
        .import_resource(buffer(), ImportState::Initialized)
        .unwrap();
    graph.set_output_group(&[input]).unwrap();
    assert_eq!(graph.compile().unwrap().modeled_metadata_bytes(), envelope);
    let overflow = Limits {
        max_resources: usize::MAX,
        max_modeled_metadata_bytes: usize::MAX,
        ..Limits::default()
    };
    assert_eq!(
        Candidate::new(overflow).unwrap_err(),
        GraphError::LimitExceeded(LimitKind::MetadataBytes)
    );
}

#[test]
fn impossible_empty_capacity_limits_reject_before_candidate_creation() {
    for limits in [
        Limits {
            max_resources: 0,
            ..Limits::default()
        },
        Limits {
            max_outputs: 0,
            ..Limits::default()
        },
        Limits {
            max_texture_dimension_2d: 0,
            ..Limits::default()
        },
        Limits {
            max_buffer_bytes: 0,
            ..Limits::default()
        },
        Limits {
            max_modeled_resource_bytes: 0,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            Candidate::new(limits).unwrap_err(),
            GraphError::InvalidLimits
        );
    }
}
