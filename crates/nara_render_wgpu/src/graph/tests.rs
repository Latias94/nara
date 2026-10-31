//! Production logical-admission cases. The parent graph module includes this with `#[cfg(test)]`.
//! Physical leases, enabled device capabilities and native execution are tested by the Host owner.
//! Calls to import_resource below stand for imports already validated by that owner.

use super::model::*;

struct SceneColor;
struct FinalComposite;
struct RawCounts;
struct Frequencies;

fn builder() -> GraphBuilder {
    GraphBuilder::new(GraphLimits::default()).unwrap()
}

fn texture<T: 'static>() -> ResourceDescriptor {
    ResourceDescriptor::Texture {
        native: wgpu::TextureDescriptor {
            label: Some("graph-color"),
            size: wgpu::Extent3d {
                width: 320,
                height: 180,
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
        meaning: TextureMeaning::of::<T>(TextureTransfer::Linear, TextureAlpha::Premultiplied),
    }
}

fn buffer_as<T: 'static>() -> ResourceDescriptor {
    ResourceDescriptor::Buffer {
        native: wgpu::BufferDescriptor {
            label: Some("graph-data"),
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
        meaning: BufferMeaning::of::<T>(),
    }
}

fn buffer() -> ResourceDescriptor {
    buffer_as::<RawCounts>()
}

fn read(
    version: &ResourceVersion,
    role: AccessRole,
    expected: &ResourceDescriptor,
) -> ResourceRead {
    ResourceRead {
        version: version.clone(),
        role,
        expected: expected.clone(),
    }
}

fn write(
    version: &ResourceVersion,
    role: AccessRole,
    initialization: Initialization,
) -> ResourceWrite {
    ResourceWrite {
        version: version.clone(),
        role,
        initialization,
    }
}

fn clear(version: &ResourceVersion) -> ResourceWrite {
    write(
        version,
        AccessRole::Attachment,
        Initialization::Clear(wgpu::Color::TRANSPARENT),
    )
}

fn zero(version: &ResourceVersion) -> ResourceWrite {
    write(version, AccessRole::StorageReadWrite, Initialization::Zero)
}

fn keys(graph: &CompiledGraph) -> Vec<u64> {
    graph.operations().map(|operation| operation.key).collect()
}

#[test]
fn branching_versions_ignore_resource_and_operation_declaration_order() {
    for reservations in [[0, 1, 2, 3, 4], [4, 2, 0, 3, 1]] {
        for declarations in [[0, 1, 2, 3, 4], [4, 3, 2, 1, 0], [2, 4, 1, 3, 0]] {
            let mut graph = builder();
            let mut versions: [Option<ResourceVersion>; 5] = std::array::from_fn(|_| None);
            for index in reservations {
                let desc = if index == 4 {
                    texture::<FinalComposite>()
                } else {
                    texture::<SceneColor>()
                };
                versions[index] = Some(graph.reserve_resource(desc).unwrap());
            }
            let v = versions.map(Option::unwrap);
            let desc = texture::<SceneColor>();
            let operations = [
                (90, vec![], vec![clear(&v[0])]),
                (
                    70,
                    vec![read(&v[0], AccessRole::Sampled, &desc)],
                    vec![clear(&v[1])],
                ),
                (
                    20,
                    vec![read(&v[1], AccessRole::Sampled, &desc)],
                    vec![clear(&v[2])],
                ),
                (
                    40,
                    vec![read(&v[0], AccessRole::Sampled, &desc)],
                    vec![clear(&v[3])],
                ),
                (
                    10,
                    vec![
                        read(&v[0], AccessRole::Sampled, &desc),
                        read(&v[2], AccessRole::Sampled, &desc),
                        read(&v[3], AccessRole::Sampled, &desc),
                    ],
                    vec![clear(&v[4])],
                ),
            ];
            graph
                .set_output_group(&[v[0].clone(), v[4].clone()])
                .unwrap();
            for index in declarations {
                let (key, reads, writes) = &operations[index];
                graph.add_operation(*key, reads, writes).unwrap();
            }
            let graph = graph.compile().unwrap();
            assert_eq!(keys(&graph), [90, 40, 70, 20, 10]);
            assert_eq!(graph.outputs(), &[v[0].clone(), v[4].clone()]);
            assert_eq!(graph.resources().len(), 5);
            assert_eq!(graph.modeled_resource_bytes(), 5 * 320 * 180 * 16);
        }
    }
}

#[test]
fn ready_keys_include_zero_and_u64_max_and_sort_independently_of_insertion() {
    for insertion in [[u64::MAX, 10, 0], [10, 0, u64::MAX], [0, u64::MAX, 10]] {
        let mut graph = builder();
        let input = graph.import_resource(buffer()).unwrap();
        for key in insertion {
            graph
                .add_operation(key, &[read(&input, AccessRole::Uniform, &buffer())], &[])
                .unwrap();
        }
        graph.set_output_group(&[input]).unwrap();
        assert_eq!(keys(&graph.compile().unwrap()), [0, 10, u64::MAX]);
    }
}

#[test]
fn newly_ready_lower_key_preempts_an_already_ready_operation() {
    let mut graph = builder();
    let first = graph.reserve_resource(buffer()).unwrap();
    let second = graph.reserve_resource(buffer()).unwrap();
    graph.add_operation(50, &[], &[zero(&second)]).unwrap();
    graph.add_operation(20, &[], &[zero(&first)]).unwrap();
    graph
        .add_operation(1, &[read(&first, AccessRole::Vertex, &buffer())], &[])
        .unwrap();
    graph.set_output_group(&[first, second]).unwrap();
    assert_eq!(keys(&graph.compile().unwrap()), [20, 1, 50]);
}

#[test]
fn buffer_pipeline_keeps_repeated_edges_and_preserved_raw_counts() {
    let mut graph = builder();
    let raw = graph.reserve_resource(buffer()).unwrap();
    let normalized = graph.reserve_resource(buffer_as::<Frequencies>()).unwrap();
    let histogram = graph.reserve_resource(texture::<FinalComposite>()).unwrap();
    graph
        .add_operation(
            30,
            &[
                read(&raw, AccessRole::Vertex, &buffer()),
                read(&normalized, AccessRole::Vertex, &buffer_as::<Frequencies>()),
            ],
            &[clear(&histogram)],
        )
        .unwrap();
    graph
        .add_operation(
            20,
            &[
                read(&raw, AccessRole::StorageRead, &buffer()),
                read(&raw, AccessRole::CopySource, &buffer()),
            ],
            &[write(
                &normalized,
                AccessRole::StorageReadWrite,
                Initialization::Preserve(raw.clone()),
            )],
        )
        .unwrap();
    graph.add_operation(10, &[], &[zero(&raw)]).unwrap();
    graph
        .set_output_group(&[raw.clone(), normalized.clone(), histogram])
        .unwrap();
    let graph = graph.compile().unwrap();
    assert_eq!(keys(&graph), [10, 20, 30]);
    assert_ne!(raw.index(), normalized.index());
    assert_eq!(graph.resource(&raw).unwrap().desc, buffer());
    assert_eq!(
        graph.resource(&normalized).unwrap().desc,
        buffer_as::<Frequencies>()
    );
    assert_eq!(
        graph.copy_dependencies().collect::<Vec<_>>(),
        [CopyDependency {
            source: raw,
            destination: normalized,
            before: 20,
        }]
    );
}

#[test]
fn validated_import_can_be_an_output_without_operations_or_access_capacity() {
    let mut graph = GraphBuilder::new(GraphLimits {
        max_operations: 0,
        max_accesses: 0,
        ..GraphLimits::default()
    })
    .unwrap();
    let input = graph.import_resource(buffer()).unwrap();
    graph.set_output_group(&[input.clone()]).unwrap();
    let graph = graph.compile().unwrap();
    assert_eq!(graph.operations().len(), 0);
    assert_eq!(graph.outputs(), &[input.clone()]);
    assert!(graph.resource(&input).unwrap().is_imported());
    assert_eq!(graph.resource(&input).unwrap().version, input);
}

#[test]
fn foreign_handles_reject_in_all_positions_after_the_original_builder_drops() {
    let foreign = builder().import_resource(buffer()).unwrap();
    let mut graph = builder();
    let input = graph.import_resource(buffer()).unwrap();
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
        graph.add_operation(1, &[], &[zero(&foreign)]),
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
    graph.add_operation(1, &[], &[zero(&output)]).unwrap();
    graph.set_output_group(&[output]).unwrap();
    assert_eq!(
        graph.compile().unwrap().resource(&foreign).unwrap_err(),
        GraphError::ForeignCandidate
    );
}

#[test]
fn cross_candidate_use_requires_a_new_host_validated_import() {
    let mut first = builder();
    let old = first.import_resource(texture::<SceneColor>()).unwrap();
    first.set_output_group(&[old.clone()]).unwrap();
    let first = first.compile().unwrap();
    let mut next = builder();
    let input = next
        .import_resource(first.resource(&old).unwrap().desc.clone())
        .unwrap();
    assert_ne!(old, input);
    next.set_output_group(&[input.clone()]).unwrap();
    let next = next.compile().unwrap();
    assert!(next.resource(&input).unwrap().is_imported());
    assert_eq!(
        next.resource(&old).unwrap_err(),
        GraphError::ForeignCandidate
    );
}

#[test]
fn duplicate_writes_in_one_operation_do_not_claim_the_version() {
    let mut graph = builder();
    let output = graph.reserve_resource(buffer()).unwrap();
    assert_eq!(
        graph.add_operation(1, &[], &[zero(&output), zero(&output)]),
        Err(GraphError::RepeatedProducer(output.index()))
    );
    graph.add_operation(1, &[], &[zero(&output)]).unwrap();
    graph.set_output_group(&[output]).unwrap();
    assert_eq!(keys(&graph.compile().unwrap()), [1]);
}

#[test]
fn a_second_operation_cannot_write_an_already_produced_version() {
    let mut graph = builder();
    let output = graph.reserve_resource(buffer()).unwrap();
    graph.add_operation(1, &[], &[zero(&output)]).unwrap();
    assert_eq!(
        graph.add_operation(2, &[], &[zero(&output)]),
        Err(GraphError::RepeatedProducer(output.index()))
    );
    graph.set_output_group(&[output]).unwrap();
    assert_eq!(keys(&graph.compile().unwrap()), [1]);
}

#[test]
fn duplicate_keys_do_not_claim_another_output_or_consume_its_budget() {
    let mut graph = GraphBuilder::new(GraphLimits {
        max_operations: 2,
        max_accesses: 2,
        ..GraphLimits::default()
    })
    .unwrap();
    let first = graph.reserve_resource(buffer()).unwrap();
    let second = graph.reserve_resource(buffer()).unwrap();
    graph.add_operation(7, &[], &[zero(&first)]).unwrap();
    assert_eq!(
        graph.add_operation(7, &[], &[zero(&second)]),
        Err(GraphError::DuplicateOperationKey(7))
    );
    graph.add_operation(6, &[], &[zero(&second)]).unwrap();
    graph.set_output_group(&[first, second]).unwrap();
    assert_eq!(keys(&graph.compile().unwrap()), [6, 7]);
}

#[test]
fn imports_are_immutable_for_every_write_role() {
    for role in [
        AccessRole::StorageWrite,
        AccessRole::StorageReadWrite,
        AccessRole::CopyDestination,
    ] {
        let mut graph = builder();
        let input = graph.import_resource(buffer()).unwrap();
        assert_eq!(
            graph.add_operation(1, &[], &[write(&input, role, Initialization::Zero)]),
            Err(GraphError::ImportedWrite(input.index()))
        );
    }
    let mut graph = builder();
    let input = graph.import_resource(texture::<SceneColor>()).unwrap();
    assert_eq!(
        graph.add_operation(1, &[], &[clear(&input)]),
        Err(GraphError::ImportedWrite(input.index()))
    );
}

#[test]
fn same_version_read_write_feedback_rejects_even_with_zero_initialization() {
    for role in [AccessRole::StorageWrite, AccessRole::StorageReadWrite] {
        let mut graph = builder();
        let output = graph.reserve_resource(buffer()).unwrap();
        assert_eq!(
            graph.add_operation(
                1,
                &[read(&output, AccessRole::StorageRead, &buffer())],
                &[write(&output, role, Initialization::Zero)],
            ),
            Err(GraphError::ReadWriteAlias(output.index()))
        );
    }
}

#[test]
fn a_write_cannot_preserve_itself() {
    let mut graph = builder();
    let output = graph.reserve_resource(buffer()).unwrap();
    assert_eq!(
        graph.add_operation(
            1,
            &[],
            &[write(
                &output,
                AccessRole::StorageReadWrite,
                Initialization::Preserve(output.clone())
            ),]
        ),
        Err(GraphError::ReadWriteAlias(output.index()))
    );
    graph.add_operation(1, &[], &[zero(&output)]).unwrap();
}

#[test]
fn preservation_cannot_depend_on_a_sibling_write_of_the_same_operation() {
    let mut graph = builder();
    let first = graph.reserve_resource(buffer()).unwrap();
    let second = graph.reserve_resource(buffer()).unwrap();
    assert_eq!(
        graph.add_operation(
            1,
            &[],
            &[
                zero(&first),
                write(
                    &second,
                    AccessRole::StorageWrite,
                    Initialization::Preserve(first.clone())
                ),
            ]
        ),
        Err(GraphError::ReadWriteAlias(first.index()))
    );
    graph.add_operation(1, &[], &[zero(&first)]).unwrap();
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
    assert_eq!(keys(&graph.compile().unwrap()), [1, 2]);
}

#[test]
fn ordinary_read_cycles_reject_the_candidate() {
    let mut graph = builder();
    let first = graph.reserve_resource(buffer()).unwrap();
    let second = graph.reserve_resource(buffer()).unwrap();
    graph
        .add_operation(
            1,
            &[read(&second, AccessRole::Uniform, &buffer())],
            &[zero(&first)],
        )
        .unwrap();
    graph
        .add_operation(
            2,
            &[read(&first, AccessRole::Uniform, &buffer())],
            &[zero(&second)],
        )
        .unwrap();
    graph.set_output_group(&[first, second]).unwrap();
    assert_eq!(graph.compile().unwrap_err(), GraphError::Cycle);
}

#[test]
fn implicit_preservation_edges_participate_in_cycle_detection() {
    let mut graph = builder();
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
fn a_healthy_output_does_not_hide_a_cycle_elsewhere_in_the_candidate() {
    let mut graph = builder();
    let independent = graph.import_resource(buffer()).unwrap();
    let first = graph.reserve_resource(buffer()).unwrap();
    let second = graph.reserve_resource(buffer()).unwrap();
    graph
        .add_operation(
            1,
            &[read(&second, AccessRole::StorageRead, &buffer())],
            &[zero(&first)],
        )
        .unwrap();
    graph
        .add_operation(
            2,
            &[],
            &[write(
                &second,
                AccessRole::StorageReadWrite,
                Initialization::Preserve(first),
            )],
        )
        .unwrap();
    graph.set_output_group(&[independent]).unwrap();
    assert_eq!(graph.compile().unwrap_err(), GraphError::Cycle);
}

#[test]
fn read_before_declaration_is_legal_but_a_missing_producer_is_not() {
    let mut graph = builder();
    let missing = graph.reserve_resource(buffer()).unwrap();
    let output = graph.reserve_resource(buffer()).unwrap();
    graph
        .add_operation(
            1,
            &[read(&missing, AccessRole::Vertex, &buffer())],
            &[zero(&output)],
        )
        .unwrap();
    graph.set_output_group(&[output]).unwrap();
    assert_eq!(
        graph.compile().unwrap_err(),
        GraphError::MissingProducer(missing.index())
    );
}

#[test]
fn unused_reservations_still_require_a_producer() {
    let mut graph = builder();
    let input = graph.import_resource(buffer()).unwrap();
    let unused = graph.reserve_resource(buffer()).unwrap();
    graph.set_output_group(&[input]).unwrap();
    assert_eq!(
        graph.compile().unwrap_err(),
        GraphError::MissingProducer(unused.index())
    );
}

#[test]
fn publication_group_is_required_and_nonempty() {
    let mut graph = builder();
    graph.import_resource(buffer()).unwrap();
    assert_eq!(
        graph.set_output_group(&[]),
        Err(GraphError::MissingOutputGroup)
    );
    assert_eq!(graph.compile().unwrap_err(), GraphError::MissingOutputGroup);
}

#[test]
fn invalid_group_replacement_keeps_the_previous_complete_outputs() {
    let mut graph = builder();
    let first = graph.import_resource(buffer()).unwrap();
    let second = graph.import_resource(buffer()).unwrap();
    graph
        .set_output_group(&[first.clone(), second.clone()])
        .unwrap();
    assert_eq!(
        graph.set_output_group(&[second.clone(), second.clone()]),
        Err(GraphError::DuplicateOutput(second.index()))
    );
    assert_eq!(
        graph.set_output_group(&[]),
        Err(GraphError::MissingOutputGroup)
    );
    let foreign = builder().import_resource(buffer()).unwrap();
    assert_eq!(
        graph.set_output_group(&[second.clone(), foreign]),
        Err(GraphError::ForeignCandidate)
    );
    assert_eq!(graph.compile().unwrap().outputs(), &[first, second]);
}

#[test]
fn texture_reads_require_type_identity_transfer_and_alpha_meaning() {
    for mismatch in 0..4 {
        let mut graph = builder();
        let input = graph.import_resource(texture::<SceneColor>()).unwrap();
        let mut expected = texture::<SceneColor>();
        let ResourceDescriptor::Texture { meaning, .. } = &mut expected else {
            unreachable!()
        };
        match mismatch {
            0 => meaning.semantic = std::any::TypeId::of::<FinalComposite>(),
            1 => meaning.transfer = TextureTransfer::Srgb,
            2 => meaning.alpha = TextureAlpha::Straight,
            _ => meaning.alpha = TextureAlpha::Opaque,
        }
        assert_eq!(
            graph.add_operation(1, &[read(&input, AccessRole::Sampled, &expected)], &[]),
            Err(GraphError::IncompatibleRead(input.index()))
        );
    }
}

#[test]
fn buffer_meanings_distinguish_counts_from_equal_sized_frequencies() {
    for role in [AccessRole::StorageRead, AccessRole::Vertex] {
        let mut graph = builder();
        let input = graph.import_resource(buffer()).unwrap();
        assert_eq!(
            graph.add_operation(1, &[read(&input, role, &buffer_as::<Frequencies>())], &[]),
            Err(GraphError::IncompatibleRead(input.index()))
        );
        graph
            .add_operation(1, &[read(&input, role, &buffer())], &[])
            .unwrap();
        graph.set_output_group(&[input]).unwrap();
        assert_eq!(keys(&graph.compile().unwrap()), [1]);
    }
}

#[test]
fn copy_initialization_can_change_the_port_without_weakening_semantic_reads() {
    let mut graph = builder();
    let scene = graph.import_resource(texture::<SceneColor>()).unwrap();
    let composite = graph.reserve_resource(texture::<FinalComposite>()).unwrap();
    assert_eq!(
        graph.add_operation(
            1,
            &[read(
                &composite,
                AccessRole::Sampled,
                &texture::<SceneColor>()
            ),],
            &[]
        ),
        Err(GraphError::IncompatibleRead(composite.index()))
    );
    graph
        .add_operation(
            1,
            &[read(
                &composite,
                AccessRole::Sampled,
                &texture::<FinalComposite>(),
            )],
            &[],
        )
        .unwrap();
    graph
        .add_operation(
            2,
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
    assert_eq!(keys(&graph), [2, 1]);
    assert_eq!(
        graph.copy_dependencies().collect::<Vec<_>>(),
        [CopyDependency {
            source: scene,
            destination: composite,
            before: 2,
        }]
    );
}

#[test]
fn preservation_requires_matching_texture_shape_format_transfer_and_alpha() {
    for mismatch in 0..4 {
        let mut graph = builder();
        let source = graph.import_resource(texture::<SceneColor>()).unwrap();
        let mut desc = texture::<SceneColor>();
        let ResourceDescriptor::Texture {
            native, meaning, ..
        } = &mut desc
        else {
            unreachable!()
        };
        match mismatch {
            0 => native.size.height -= 1,
            1 => native.format = wgpu::TextureFormat::Rgba16Float,
            2 => meaning.transfer = TextureTransfer::Srgb,
            _ => meaning.alpha = TextureAlpha::Straight,
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
                ),]
            ),
            Err(GraphError::IncompatiblePreserve(output.index()))
        );
    }
}

#[test]
fn preservation_rejects_buffer_size_and_resource_kind_changes() {
    for to_texture in [false, true] {
        let mut graph = builder();
        let source = graph.import_resource(buffer()).unwrap();
        let mut desc = if to_texture {
            texture::<SceneColor>()
        } else {
            buffer()
        };
        if let ResourceDescriptor::Buffer { native, .. } = &mut desc {
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
                &[write(&output, role, Initialization::Preserve(source)),]
            ),
            Err(GraphError::IncompatiblePreserve(output.index()))
        );
    }
}

#[test]
fn read_compatibility_checks_shape_kind_and_the_complete_requested_usage() {
    for mismatch in 0..4 {
        let mut graph = builder();
        let mut actual = texture::<SceneColor>();
        let ResourceDescriptor::Texture { native, .. } = &mut actual else {
            unreachable!()
        };
        native.usage = wgpu::TextureUsages::TEXTURE_BINDING;
        let input = graph.import_resource(actual.clone()).unwrap();
        let mut expected = actual;
        if mismatch == 2 {
            expected = buffer();
        } else {
            let ResourceDescriptor::Texture { native, .. } = &mut expected else {
                unreachable!()
            };
            match mismatch {
                0 => native.size.width -= 1,
                1 => native.format = wgpu::TextureFormat::Rgba16Float,
                _ => native.usage |= wgpu::TextureUsages::COPY_SRC,
            }
        }
        assert_eq!(
            graph.add_operation(1, &[read(&input, AccessRole::Sampled, &expected)], &[]),
            Err(GraphError::IncompatibleRead(input.index()))
        );
    }
}

#[test]
fn full_range_spellings_labels_and_usage_subsets_preserve_read_compatibility() {
    let mut graph = builder();
    let original = texture::<SceneColor>();
    let input = graph.import_resource(original.clone()).unwrap();
    let mut expected = original.clone();
    let ResourceDescriptor::Texture { native, range, .. } = &mut expected else {
        unreachable!()
    };
    native.label = Some("consumer-port");
    native.usage = wgpu::TextureUsages::TEXTURE_BINDING;
    range.mip_level_count = Some(1);
    range.array_layer_count = Some(1);
    graph
        .add_operation(1, &[read(&input, AccessRole::Sampled, &expected)], &[])
        .unwrap();
    graph.set_output_group(&[input.clone()]).unwrap();
    assert_eq!(
        graph.compile().unwrap().resource(&input).unwrap().desc,
        original
    );
}

#[test]
fn unsupported_texture_extents_layers_dimensions_mips_and_samples_reject() {
    for mismatch in 0..12 {
        let mut graph = builder();
        let mut desc = texture::<SceneColor>();
        let ResourceDescriptor::Texture { native, .. } = &mut desc else {
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
            8 => native.dimension = wgpu::TextureDimension::D1,
            9 => native.dimension = wgpu::TextureDimension::D3,
            10 => native.view_formats = &[wgpu::TextureFormat::Rgba32Float],
            _ => native.view_formats = &[wgpu::TextureFormat::Rgba8UnormSrgb],
        }
        assert_eq!(
            graph.reserve_resource(desc),
            Err(GraphError::UnsupportedTextureScope)
        );
        assert_eq!(graph.import_resource(buffer()).unwrap().index(), 0);
    }
}

#[test]
fn invalid_ranges_reject_for_reservations_imports_and_expected_reads() {
    for mismatch in 0..9 {
        let mut graph = builder();
        let input = graph.import_resource(texture::<SceneColor>()).unwrap();
        let mut desc = texture::<SceneColor>();
        let ResourceDescriptor::Texture { range, .. } = &mut desc else {
            unreachable!()
        };
        match mismatch {
            0 => range.aspect = wgpu::TextureAspect::DepthOnly,
            1 => range.aspect = wgpu::TextureAspect::StencilOnly,
            2 => range.aspect = wgpu::TextureAspect::Plane0,
            3 => range.base_mip_level = u32::MAX,
            4 => range.mip_level_count = Some(0),
            5 => range.mip_level_count = Some(2),
            6 => range.base_array_layer = u32::MAX,
            7 => range.array_layer_count = Some(0),
            _ => range.array_layer_count = Some(u32::MAX),
        }
        assert_eq!(
            graph.reserve_resource(desc.clone()),
            Err(GraphError::UnsupportedTextureScope)
        );
        assert_eq!(
            graph.import_resource(desc.clone()),
            Err(GraphError::UnsupportedTextureScope)
        );
        assert_eq!(
            graph.add_operation(1, &[read(&input, AccessRole::Sampled, &desc)], &[]),
            Err(GraphError::UnsupportedTextureScope)
        );
    }
}

#[test]
fn unsupported_formats_unknown_flags_and_invalid_format_usages_reject() {
    for format in [
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureFormat::Bc1RgbaUnorm,
        wgpu::TextureFormat::NV12,
    ] {
        let mut desc = texture::<SceneColor>();
        let ResourceDescriptor::Texture { native, .. } = &mut desc else {
            unreachable!()
        };
        native.format = format;
        assert_eq!(
            builder().reserve_resource(desc),
            Err(GraphError::UnsupportedTextureFormat)
        );
    }
    for usage in [
        wgpu::TextureUsages::empty(),
        wgpu::TextureUsages::from_bits_retain(1 << 31),
        wgpu::TextureUsages::TRANSIENT_ATTACHMENT,
    ] {
        let mut desc = texture::<SceneColor>();
        let ResourceDescriptor::Texture { native, .. } = &mut desc else {
            unreachable!()
        };
        native.usage = usage;
        assert_eq!(
            builder().reserve_resource(desc),
            Err(GraphError::UnsupportedUsage)
        );
    }
    let mut desc = texture::<SceneColor>();
    let ResourceDescriptor::Texture { native, .. } = &mut desc else {
        unreachable!()
    };
    native.format = wgpu::TextureFormat::Rgba8UnormSrgb;
    native.usage = wgpu::TextureUsages::STORAGE_BINDING;
    assert_eq!(
        builder().reserve_resource(desc),
        Err(GraphError::UnsupportedUsage)
    );
}

#[test]
fn zero_unaligned_mapped_mappable_and_unknown_usage_buffers_reject() {
    for mismatch in 0..8 {
        let mut desc = buffer();
        let ResourceDescriptor::Buffer { native, .. } = &mut desc else {
            unreachable!()
        };
        match mismatch {
            0 => native.size = 0,
            1 => native.size = 15,
            2 => native.mapped_at_creation = true,
            3 => native.usage = wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            4 => native.usage = wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
            5 => native.usage = wgpu::BufferUsages::empty(),
            6 => native.usage = wgpu::BufferUsages::QUERY_RESOLVE,
            _ => native.usage = wgpu::BufferUsages::from_bits_retain(1 << 31),
        }
        let expected = if mismatch < 3 {
            GraphError::UnsupportedBufferScope
        } else {
            GraphError::UnsupportedUsage
        };
        let mut graph = builder();
        assert_eq!(graph.import_resource(desc), Err(expected));
        assert_eq!(graph.import_resource(buffer()).unwrap().index(), 0);
    }
}

#[test]
fn supported_texture_roles_use_native_flags_and_distinct_initialized_writes() {
    let mut graph = builder();
    let mut desc = texture::<SceneColor>();
    let ResourceDescriptor::Texture { native, .. } = &mut desc else {
        unreachable!()
    };
    native.format = wgpu::TextureFormat::R32Float;
    let input = graph.import_resource(desc.clone()).unwrap();
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
                &[write(
                    &output,
                    role,
                    Initialization::Clear(wgpu::Color::TRANSPARENT),
                )],
            )
            .unwrap();
    }
    graph.set_output_group(&[input]).unwrap();
    assert_eq!(graph.compile().unwrap().operations().len(), 7);
}

#[test]
fn supported_buffer_roles_include_vertex_index_uniform_and_indirect() {
    let mut graph = builder();
    let input = graph.import_resource(buffer()).unwrap();
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
            .add_operation(key as u64, &[read(&input, role, &buffer())], &[])
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
        let output = graph.reserve_resource(buffer()).unwrap();
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
fn every_role_requires_its_corresponding_native_usage() {
    for texture_resource in [false, true] {
        let read_roles: &[AccessRole] = if texture_resource {
            &[
                AccessRole::Sampled,
                AccessRole::StorageRead,
                AccessRole::CopySource,
            ]
        } else {
            &[
                AccessRole::StorageRead,
                AccessRole::CopySource,
                AccessRole::Uniform,
                AccessRole::Vertex,
                AccessRole::Index,
                AccessRole::Indirect,
            ]
        };
        for role in read_roles {
            let mut graph = builder();
            let mut desc = if texture_resource {
                texture::<SceneColor>()
            } else {
                buffer()
            };
            match &mut desc {
                ResourceDescriptor::Texture { native, .. } => {
                    native.usage = wgpu::TextureUsages::COPY_DST
                }
                ResourceDescriptor::Buffer { native, .. } => {
                    native.usage = wgpu::BufferUsages::COPY_DST
                }
            }
            let input = graph.import_resource(desc.clone()).unwrap();
            assert_eq!(
                graph.add_operation(1, &[read(&input, *role, &desc)], &[]),
                Err(GraphError::MissingUsage {
                    version: input.index(),
                    role: *role
                })
            );
        }
        let write_roles: &[AccessRole] = if texture_resource {
            &[
                AccessRole::Attachment,
                AccessRole::StorageWrite,
                AccessRole::StorageReadWrite,
                AccessRole::CopyDestination,
            ]
        } else {
            &[
                AccessRole::StorageWrite,
                AccessRole::StorageReadWrite,
                AccessRole::CopyDestination,
            ]
        };
        for role in write_roles {
            let mut graph = builder();
            let mut desc = if texture_resource {
                texture::<SceneColor>()
            } else {
                buffer()
            };
            match &mut desc {
                ResourceDescriptor::Texture { native, .. } => {
                    native.format = wgpu::TextureFormat::R32Float;
                    native.usage = wgpu::TextureUsages::TEXTURE_BINDING;
                }
                ResourceDescriptor::Buffer { native, .. } => {
                    native.usage = wgpu::BufferUsages::COPY_SRC
                }
            }
            let output = graph.reserve_resource(desc).unwrap();
            let initialization = if texture_resource {
                Initialization::Clear(wgpu::Color::TRANSPARENT)
            } else {
                Initialization::Zero
            };
            assert_eq!(
                graph.add_operation(1, &[], &[write(&output, *role, initialization)]),
                Err(GraphError::MissingUsage {
                    version: output.index(),
                    role: *role
                })
            );
        }
    }
}

#[test]
fn access_direction_does_not_turn_storage_read_write_into_an_input_role() {
    for role in [
        AccessRole::Attachment,
        AccessRole::StorageWrite,
        AccessRole::StorageReadWrite,
        AccessRole::CopyDestination,
    ] {
        let mut graph = builder();
        let input = graph.import_resource(buffer()).unwrap();
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
        let mut graph = builder();
        let output = graph.reserve_resource(buffer()).unwrap();
        assert_eq!(
            graph.add_operation(1, &[], &[write(&output, role, Initialization::Zero)]),
            Err(GraphError::IllegalAccess(role))
        );
    }
}

#[test]
fn texture_and_buffer_roles_do_not_cross_resource_kinds() {
    for role in [
        AccessRole::Uniform,
        AccessRole::Vertex,
        AccessRole::Index,
        AccessRole::Indirect,
    ] {
        let mut graph = builder();
        let input = graph.import_resource(texture::<SceneColor>()).unwrap();
        assert_eq!(
            graph.add_operation(1, &[read(&input, role, &texture::<SceneColor>())], &[]),
            Err(GraphError::IllegalKind(role))
        );
    }
    let mut graph = builder();
    let input = graph.import_resource(buffer()).unwrap();
    let output = graph.reserve_resource(buffer()).unwrap();
    assert_eq!(
        graph.add_operation(1, &[read(&input, AccessRole::Sampled, &buffer())], &[]),
        Err(GraphError::IllegalKind(AccessRole::Sampled))
    );
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
fn texture_storage_read_write_requires_baseline_format_support() {
    for format in [
        wgpu::TextureFormat::R32Float,
        wgpu::TextureFormat::Rgba32Float,
    ] {
        let mut graph = builder();
        let mut desc = texture::<SceneColor>();
        let ResourceDescriptor::Texture { native, .. } = &mut desc else {
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
fn initialization_checks_resource_kind_and_every_clear_component_for_finiteness() {
    let mut graph = builder();
    let image = graph.reserve_resource(texture::<SceneColor>()).unwrap();
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
            ),]
        ),
        Err(GraphError::InvalidInitialization(data.index()))
    );
    for channel in 0..4 {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut color = wgpu::Color::TRANSPARENT;
            match channel {
                0 => color.r = value,
                1 => color.g = value,
                2 => color.b = value,
                _ => color.a = value,
            }
            assert_eq!(
                graph.add_operation(
                    1,
                    &[],
                    &[write(
                        &image,
                        AccessRole::Attachment,
                        Initialization::Clear(color)
                    ),]
                ),
                Err(GraphError::InvalidInitialization(image.index()))
            );
        }
    }
    graph
        .add_operation(1, &[], &[clear(&image), zero(&data)])
        .unwrap();
    graph.set_output_group(&[image, data]).unwrap();
    graph.compile().unwrap();
}

#[test]
fn clear_and_zero_require_native_initialization_usages_in_addition_to_kernel_usages() {
    for image in [false, true] {
        let mut graph = builder();
        let mut desc = if image {
            texture::<SceneColor>()
        } else {
            buffer()
        };
        match &mut desc {
            ResourceDescriptor::Texture { native, .. } => {
                native.usage = wgpu::TextureUsages::STORAGE_BINDING
            }
            ResourceDescriptor::Buffer { native, .. } => native.usage = wgpu::BufferUsages::STORAGE,
        }
        let output = graph.reserve_resource(desc).unwrap();
        let initialization = if image {
            Initialization::Clear(wgpu::Color::TRANSPARENT)
        } else {
            Initialization::Zero
        };
        let role = if image {
            AccessRole::Attachment
        } else {
            AccessRole::CopyDestination
        };
        assert_eq!(
            graph.add_operation(
                1,
                &[],
                &[write(&output, AccessRole::StorageWrite, initialization)]
            ),
            Err(GraphError::MissingUsage {
                version: output.index(),
                role
            })
        );
    }
}

#[test]
fn preserve_requires_both_copy_flags_without_mutating_native_descriptors() {
    for missing_source in [false, true] {
        let mut graph = builder();
        let mut source_desc = buffer();
        let mut output_desc = buffer();
        let desc = if missing_source {
            &mut source_desc
        } else {
            &mut output_desc
        };
        let ResourceDescriptor::Buffer { native, .. } = desc else {
            unreachable!()
        };
        native.usage = wgpu::BufferUsages::STORAGE;
        let source = graph.import_resource(source_desc).unwrap();
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
fn shared_attachment_outputs_always_require_store_after_clear_and_preserve() {
    let mut graph = builder();
    let source = graph.import_resource(texture::<SceneColor>()).unwrap();
    let first = graph.reserve_resource(texture::<SceneColor>()).unwrap();
    let second = graph.reserve_resource(texture::<FinalComposite>()).unwrap();
    graph.add_operation(1, &[], &[clear(&first)]).unwrap();
    graph
        .add_operation(
            2,
            &[],
            &[write(
                &second,
                AccessRole::Attachment,
                Initialization::Preserve(source.clone()),
            )],
        )
        .unwrap();
    graph.set_output_group(&[first, second]).unwrap();
    for operation in graph.compile().unwrap().operations() {
        for write in &operation.writes {
            assert_eq!(write.store_op(), wgpu::StoreOp::Store);
        }
    }
}

#[test]
fn failure_in_a_later_write_does_not_claim_earlier_writers_or_accesses() {
    let mut graph = GraphBuilder::new(GraphLimits {
        max_operations: 1,
        max_accesses: 2,
        ..GraphLimits::default()
    })
    .unwrap();
    let first = graph.reserve_resource(buffer()).unwrap();
    let second = graph.reserve_resource(buffer()).unwrap();
    assert_eq!(
        graph.add_operation(
            1,
            &[],
            &[
                zero(&first),
                write(
                    &second,
                    AccessRole::StorageWrite,
                    Initialization::Clear(wgpu::Color::BLACK)
                ),
            ]
        ),
        Err(GraphError::InvalidInitialization(second.index()))
    );
    graph
        .add_operation(1, &[], &[zero(&first), zero(&second)])
        .unwrap();
    graph.set_output_group(&[first, second]).unwrap();
    assert_eq!(keys(&graph.compile().unwrap()), [1]);
}

#[test]
fn resource_count_capacity_is_exact_and_one_short_rejects_before_issuing_a_handle() {
    for maximum in [1, 2] {
        let mut graph = GraphBuilder::new(GraphLimits {
            max_resources: maximum,
            ..GraphLimits::default()
        })
        .unwrap();
        let first = graph.import_resource(buffer()).unwrap();
        let second = graph.import_resource(buffer());
        if maximum == 1 {
            assert_eq!(second, Err(GraphError::LimitExceeded(LimitKind::Resources)));
        } else {
            assert_eq!(second.unwrap().index(), 1);
        }
        graph.set_output_group(&[first]).unwrap();
        let graph = graph.compile().unwrap();
        assert_eq!(graph.resources().len(), maximum);
        assert_eq!(graph.modeled_resource_bytes(), maximum as u64 * 16);
    }
}

#[test]
fn operation_capacity_is_exact_and_one_short_does_not_admit_extra_work() {
    for maximum in [1, 2] {
        let mut graph = GraphBuilder::new(GraphLimits {
            max_operations: maximum,
            max_accesses: 2,
            ..GraphLimits::default()
        })
        .unwrap();
        let input = graph.import_resource(buffer()).unwrap();
        graph
            .add_operation(2, &[read(&input, AccessRole::Uniform, &buffer())], &[])
            .unwrap();
        let second = graph.add_operation(1, &[read(&input, AccessRole::Uniform, &buffer())], &[]);
        if maximum == 1 {
            assert_eq!(
                second,
                Err(GraphError::LimitExceeded(LimitKind::Operations))
            );
        } else {
            second.unwrap();
        }
        graph.set_output_group(&[input]).unwrap();
        let graph = graph.compile().unwrap();
        assert_eq!(graph.operations().len(), maximum);
        assert_eq!(
            keys(&graph),
            if maximum == 1 { vec![2] } else { vec![1, 2] }
        );
    }
}

#[test]
fn access_capacity_counts_reads_writes_and_implicit_preservation_reads_exactly() {
    for maximum in [2, 3] {
        let mut graph = GraphBuilder::new(GraphLimits {
            max_operations: 1,
            max_accesses: maximum,
            ..GraphLimits::default()
        })
        .unwrap();
        let input = graph.import_resource(buffer()).unwrap();
        let output = graph.reserve_resource(buffer()).unwrap();
        let result = graph.add_operation(
            9,
            &[read(&input, AccessRole::StorageRead, &buffer())],
            &[write(
                &output,
                AccessRole::StorageReadWrite,
                Initialization::Preserve(input.clone()),
            )],
        );
        if maximum == 2 {
            assert_eq!(result, Err(GraphError::LimitExceeded(LimitKind::Accesses)));
            // Reusing the key, output and all remaining access capacity proves atomic rejection.
            graph
                .add_operation(
                    9,
                    &[read(&input, AccessRole::StorageRead, &buffer())],
                    &[zero(&output)],
                )
                .unwrap();
        } else {
            result.unwrap();
        }
        assert_eq!(
            graph.add_operation(10, &[], &[]),
            Err(GraphError::LimitExceeded(LimitKind::Operations))
        );
        graph.set_output_group(&[output]).unwrap();
        let graph = graph.compile().unwrap();
        assert_eq!(keys(&graph), [9]);
        assert_eq!(graph.copy_dependencies().count(), usize::from(maximum == 3));
    }
}

#[test]
fn output_capacity_is_exact_and_one_short_preserves_the_old_group() {
    for maximum in [1, 2] {
        let mut graph = GraphBuilder::new(GraphLimits {
            max_outputs: maximum,
            ..GraphLimits::default()
        })
        .unwrap();
        let first = graph.import_resource(buffer()).unwrap();
        let second = graph.import_resource(buffer()).unwrap();
        graph.set_output_group(&[first.clone()]).unwrap();
        let result = graph.set_output_group(&[second.clone(), first.clone()]);
        if maximum == 1 {
            assert_eq!(result, Err(GraphError::LimitExceeded(LimitKind::Outputs)));
        } else {
            result.unwrap();
        }
        let expected = if maximum == 1 {
            vec![first]
        } else {
            vec![second, first]
        };
        assert_eq!(graph.compile().unwrap().outputs(), expected);
    }
}

#[test]
fn logical_byte_allowance_counts_imports_and_locals_and_rejections_leave_capacity() {
    for maximum in [31, 32] {
        let mut graph = GraphBuilder::new(GraphLimits {
            max_modeled_resource_bytes: maximum,
            ..GraphLimits::default()
        })
        .unwrap();
        let input = graph.import_resource(buffer()).unwrap();
        let result = graph.reserve_resource(buffer());
        let (output, expected_bytes) = if maximum == 31 {
            assert_eq!(
                result,
                Err(GraphError::LimitExceeded(LimitKind::ResourceBytes))
            );
            let mut small = buffer();
            let ResourceDescriptor::Buffer { native, .. } = &mut small else {
                unreachable!()
            };
            native.size = 12;
            (graph.reserve_resource(small).unwrap(), 28)
        } else {
            (result.unwrap(), 32)
        };
        assert_eq!(output.index(), 1);
        graph.add_operation(1, &[], &[zero(&output)]).unwrap();
        graph.set_output_group(&[input, output]).unwrap();
        assert_eq!(
            graph.compile().unwrap().modeled_resource_bytes(),
            expected_bytes
        );
    }
}

#[test]
fn metadata_envelope_accepts_exact_bytes_and_rejects_one_short_before_construction() {
    let mut limits = GraphLimits::default();
    let envelope = limits.modeled_metadata_bytes().unwrap();
    limits.max_modeled_metadata_bytes = envelope - 1;
    assert_eq!(
        GraphBuilder::new(limits).unwrap_err(),
        GraphError::LimitExceeded(LimitKind::MetadataBytes)
    );
    limits.max_modeled_metadata_bytes = envelope;
    let mut graph = GraphBuilder::new(limits).unwrap();
    let input = graph.import_resource(buffer()).unwrap();
    graph.set_output_group(&[input]).unwrap();
    assert_eq!(graph.compile().unwrap().modeled_metadata_bytes(), envelope);
}

#[test]
fn metadata_overflow_and_impossible_single_allocations_reject_for_every_capacity() {
    for field in 0..5 {
        let mut limits = GraphLimits {
            max_modeled_metadata_bytes: usize::MAX,
            ..GraphLimits::default()
        };
        match field {
            0 => limits.max_resources = usize::MAX,
            1 => limits.max_operations = usize::MAX,
            2 => limits.max_accesses = usize::MAX,
            3 => limits.max_outputs = usize::MAX,
            _ => {
                limits.max_operations =
                    (isize::MAX as usize) / std::mem::size_of::<GraphOperation>() + 1
            }
        }
        assert_eq!(
            GraphBuilder::new(limits).unwrap_err(),
            GraphError::LimitExceeded(LimitKind::MetadataBytes)
        );
    }
}

#[test]
fn texture_texel_byte_multiplication_overflow_does_not_issue_a_resource() {
    let mut graph = GraphBuilder::new(GraphLimits {
        max_texture_dimension_2d: u32::MAX,
        max_modeled_resource_bytes: u64::MAX,
        ..GraphLimits::default()
    })
    .unwrap();
    let mut desc = texture::<SceneColor>();
    let ResourceDescriptor::Texture { native, .. } = &mut desc else {
        unreachable!()
    };
    native.size.width = u32::MAX;
    native.size.height = u32::MAX;
    assert_eq!(
        graph.reserve_resource(desc),
        Err(GraphError::LimitExceeded(LimitKind::ResourceBytes))
    );
    let input = graph.import_resource(buffer()).unwrap();
    assert_eq!(input.index(), 0);
    graph.set_output_group(&[input]).unwrap();
    assert_eq!(graph.compile().unwrap().modeled_resource_bytes(), 16);
}

#[test]
fn aggregate_logical_byte_overflow_cannot_wrap_into_an_available_budget() {
    let mut graph = GraphBuilder::new(GraphLimits {
        max_buffer_bytes: u64::MAX,
        max_modeled_resource_bytes: u64::MAX,
        ..GraphLimits::default()
    })
    .unwrap();
    let mut desc = buffer();
    let ResourceDescriptor::Buffer { native, .. } = &mut desc else {
        unreachable!()
    };
    native.size = u64::MAX - 3;
    let input = graph.import_resource(desc).unwrap();
    let mut tiny = buffer();
    let ResourceDescriptor::Buffer { native, .. } = &mut tiny else {
        unreachable!()
    };
    native.size = 4;
    assert_eq!(
        graph.import_resource(tiny),
        Err(GraphError::LimitExceeded(LimitKind::ResourceBytes))
    );
    graph.set_output_group(&[input]).unwrap();
    let graph = graph.compile().unwrap();
    assert_eq!(graph.resources().len(), 1);
    assert_eq!(graph.modeled_resource_bytes(), u64::MAX - 3);
}

#[test]
fn configured_texture_dimensions_and_buffer_lengths_are_inclusive() {
    let mut graph = GraphBuilder::new(GraphLimits {
        max_texture_dimension_2d: 512,
        max_buffer_bytes: 16,
        ..GraphLimits::default()
    })
    .unwrap();
    let mut image = texture::<SceneColor>();
    let ResourceDescriptor::Texture { native, .. } = &mut image else {
        unreachable!()
    };
    native.size.width = 513;
    assert_eq!(
        graph.import_resource(image.clone()),
        Err(GraphError::LimitExceeded(LimitKind::TextureDimension))
    );
    let ResourceDescriptor::Texture { native, .. } = &mut image else {
        unreachable!()
    };
    native.size.width = 512;
    native.size.height = 512;
    let image = graph.import_resource(image).unwrap();
    let mut data = buffer();
    let ResourceDescriptor::Buffer { native, .. } = &mut data else {
        unreachable!()
    };
    native.size = 20;
    assert_eq!(
        graph.reserve_resource(data),
        Err(GraphError::LimitExceeded(LimitKind::BufferBytes))
    );
    let data = graph.import_resource(buffer()).unwrap();
    assert_eq!(data.index(), 1);
    graph.set_output_group(&[image, data]).unwrap();
    assert_eq!(
        graph.compile().unwrap().modeled_resource_bytes(),
        512 * 512 * 16 + 16
    );
}

#[test]
fn impossible_limits_reject_but_import_only_zero_work_is_supported() {
    for field in 0..5 {
        let mut limits = GraphLimits::default();
        match field {
            0 => limits.max_resources = 0,
            1 => limits.max_outputs = 0,
            2 => limits.max_texture_dimension_2d = 0,
            3 => limits.max_buffer_bytes = 0,
            _ => limits.max_modeled_resource_bytes = 0,
        }
        assert_eq!(
            GraphBuilder::new(limits).unwrap_err(),
            GraphError::InvalidLimits
        );
    }
    let limits = GraphLimits {
        max_operations: 0,
        max_accesses: 0,
        ..GraphLimits::default()
    };
    assert!(GraphBuilder::new(limits).is_ok());
}

#[test]
fn dense_declared_dependencies_remain_bounded_by_the_access_capacity() {
    const COUNT: usize = 32;
    const ACCESSES: usize = COUNT * (COUNT - 1) / 2 + COUNT;
    let mut limits = GraphLimits {
        max_resources: COUNT,
        max_operations: COUNT,
        max_accesses: ACCESSES,
        max_outputs: 1,
        ..GraphLimits::default()
    };
    limits.max_modeled_metadata_bytes = limits.modeled_metadata_bytes().unwrap();
    let mut graph = GraphBuilder::new(limits).unwrap();
    let versions: Vec<_> = (0..COUNT)
        .map(|_| graph.reserve_resource(buffer()).unwrap())
        .collect();
    for index in (0..COUNT).rev() {
        let reads: Vec<_> = versions[..index]
            .iter()
            .map(|version| read(version, AccessRole::Uniform, &buffer()))
            .collect();
        graph
            .add_operation((COUNT - index) as u64, &reads, &[zero(&versions[index])])
            .unwrap();
    }
    graph
        .set_output_group(&[versions[COUNT - 1].clone()])
        .unwrap();
    let graph = graph.compile().unwrap();
    assert_eq!(keys(&graph), (1..=COUNT as u64).rev().collect::<Vec<_>>());
    assert_eq!(
        graph.modeled_metadata_bytes(),
        limits.max_modeled_metadata_bytes
    );
}
