//! Real GPU execution of the private graph with the same kernels as viable slot/order controls.

#![forbid(unsafe_code)]

use std::{sync::Arc, time::Duration};

#[path = "support/effects.rs"]
mod effects;
#[path = "support/gpu.rs"]
mod gpu;
#[path = "support/graph.rs"]
mod graph;
#[path = "support/history.rs"]
mod history;
#[path = "support/native.rs"]
mod native;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/workload.rs"]
mod workload;

fn execute(
    gpu: &gpu::Gpu,
    effects: &effects::Effects,
    order: &[graph::OperationKey],
    pixels: &[[f32; 4]],
) -> (native::SubmittedGraph, workload::Versions) {
    let workload = workload::build(order).unwrap();
    let versions = workload.versions;
    let graph::ResourceDesc::Texture {
        native: descriptor, ..
    } = &workload.graph.resource(&versions.original).unwrap().desc
    else {
        panic!("the selected Scene input is a texture")
    };
    let input = Arc::new(gpu.device.create_texture(descriptor));
    gpu.upload_rgba32(&input, workload::WIDTH, workload::HEIGHT, pixels);
    let imports = [(
        versions.original.clone(),
        native::NativeResource::Texture(input),
    )];
    let submitted = native::execute(
        gpu,
        workload.graph,
        &imports,
        |operation, resources, encoder| {
            encode_operation(gpu, effects, &versions, operation, resources, encoder)
        },
    )
    .unwrap();
    (submitted, versions)
}

fn encode_operation(
    gpu: &gpu::Gpu,
    effects: &effects::Effects,
    versions: &workload::Versions,
    operation: &graph::Operation,
    resources: &native::NativeResources,
    encoder: &mut wgpu::CommandEncoder,
) -> Result<(), native::NativeError> {
    let view = |version: &graph::Version| {
        resources
            .texture(version)
            .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
    };
    match operation.key {
        workload::BLUR_HORIZONTAL => effects.encode_blur_horizontal(
            &gpu.device,
            encoder,
            &view(&versions.original)?,
            &view(&versions.horizontal)?,
        ),
        workload::BLUR_VERTICAL => effects.encode_blur_vertical(
            &gpu.device,
            encoder,
            &view(&versions.horizontal)?,
            &view(&versions.blurred)?,
        ),
        workload::COMPLEMENT => effects.encode_complement(
            &gpu.device,
            encoder,
            &view(&versions.blurred)?,
            &view(&versions.complement)?,
        ),
        workload::HUD => effects.encode_hud(
            &gpu.device,
            encoder,
            &view(&versions.complement)?,
            &view(&versions.composite)?,
        ),
        workload::LUMINANCE => effects.encode_luminance(
            &gpu.device,
            encoder,
            &view(&versions.original)?,
            resources.buffer(&versions.raw)?,
        ),
        workload::NORMALIZE => effects.encode_normalize(
            &gpu.device,
            encoder,
            resources.buffer(&versions.normalized)?,
        ),
        workload::HISTOGRAM => effects.encode_histogram(
            encoder,
            resources.buffer(&versions.normalized)?,
            &view(&versions.histogram)?,
        ),
        _ => return Err(native::NativeError::Callback("unknown concrete operation")),
    }
    Ok(())
}

fn assert_output(
    gpu: &gpu::Gpu,
    output: &native::SubmittedGraph,
    versions: &workload::Versions,
    pixels: &[[f32; 4]],
) {
    let horizontal = oracle::blur(pixels, workload::WIDTH, workload::HEIGHT, true);
    let blurred = oracle::blur(&horizontal, workload::WIDTH, workload::HEIGHT, false);
    let complement = oracle::complement(&blurred);
    let composite = oracle::hud(&complement, workload::WIDTH);
    for (version, expected) in [
        (&versions.original, pixels),
        (&versions.horizontal, horizontal.as_slice()),
        (&versions.blurred, blurred.as_slice()),
        (&versions.complement, complement.as_slice()),
        (&versions.composite, composite.as_slice()),
    ] {
        let texture = output.resources().texture(version).unwrap();
        oracle::assert_pixels(
            &gpu.read_rgba32(texture, workload::WIDTH, workload::HEIGHT),
            expected,
            1.0e-6,
        );
    }
    let counts = oracle::luminance_counts(pixels);
    let raw = gpu.read_buffer(output.resources().buffer(&versions.raw).unwrap());
    let actual: [u32; 4] = std::array::from_fn(|index| {
        u32::from_le_bytes(raw[index * 4..index * 4 + 4].try_into().unwrap())
    });
    assert_eq!(actual, counts);
    let normalized = oracle::normalized(counts);
    let bytes = gpu.read_buffer(output.resources().buffer(&versions.normalized).unwrap());
    for (encoded, expected) in bytes.chunks_exact(4).zip(normalized) {
        assert!((f32::from_le_bytes(encoded.try_into().unwrap()) - expected).abs() <= 1.0e-6);
    }
    let histogram = gpu.read_rgba32(
        output.resources().texture(&versions.histogram).unwrap(),
        workload::WIDTH,
        workload::HEIGHT,
    );
    for y in 0..workload::HEIGHT {
        for x in 0..workload::WIDTH {
            let bin = (x * 4 / workload::WIDTH) as usize;
            let covered = y as f32 + 0.5 >= workload::HEIGHT as f32 * (1.0 - normalized[bin]);
            let expected = if covered { [1.0; 4] } else { [0.0; 4] };
            assert_eq!(histogram[(y * workload::WIDTH + x) as usize], expected);
        }
    }
}

#[test]
#[ignore = "real native GPU branching/compute comparison; explicitly select and run serially"]
fn permuted_graph_executes_shared_images_preserved_statistics_and_vertex_histogram() {
    let gpu = gpu::Gpu::new();
    let effects = effects::Effects::new(&gpu.device);
    let first = oracle::scene(workload::WIDTH, workload::HEIGHT, 0);
    let next = oracle::scene(workload::WIDTH, workload::HEIGHT, 1);
    let mut reverse = workload::OPERATION_KEYS;
    reverse.reverse();
    let shuffled = [
        workload::HISTOGRAM,
        workload::HUD,
        workload::NORMALIZE,
        workload::BLUR_VERTICAL,
        workload::LUMINANCE,
        workload::COMPLEMENT,
        workload::BLUR_HORIZONTAL,
    ];
    let mut retained = Vec::new();
    for order in [&workload::OPERATION_KEYS, &reverse, &shuffled] {
        let (output, versions) = execute(&gpu, &effects, order, &first);
        assert_eq!(output.graph().outputs().len(), 8);
        assert_output(&gpu, &output, &versions, &first);
        retained.push((output, versions));
    }
    let (successor, versions) = execute(&gpu, &effects, &reverse, &next);
    assert_output(&gpu, &successor, &versions, &next);
    // Old outputs and original raw stats survive successor execution and reordered declaration.
    for (output, versions) in retained {
        assert_output(&gpu, &output, &versions, &first);
        gpu.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(output.submission_index().clone()),
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
        assert!(output.is_complete());
    }
}

#[test]
#[ignore = "real native GPU late failures and encoder validation; run serially"]
fn late_candidate_failures_submit_no_canary_work_and_preserve_independent_progress() {
    let gpu = gpu::Gpu::new();
    let effects = effects::Effects::new(&gpu.device);
    let pixels = oracle::scene(workload::WIDTH, workload::HEIGHT, 0);
    const PREFIX: graph::OperationKey = 20;
    const LATER: graph::OperationKey = 10;
    const INITIAL: [u8; 16] = [0x3c; 16];
    const WRITTEN: [u8; 16] = [0xa5; 16];
    #[derive(Clone, Copy, Debug)]
    enum Fault {
        DropReady,
        Reject,
        Panic,
        InvalidCopy,
    }

    // The successful control establishes that the very same canary command changes bytes.
    // Preparing without submission, dropping ReadyGraph, and each encoding failure must all
    // leave INITIAL intact despite having recorded the same earlier canary command.
    for fault in [
        None,
        Some(Fault::DropReady),
        Some(Fault::Reject),
        Some(Fault::Panic),
        Some(Fault::InvalidCopy),
    ] {
        let source = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("trusted-fault-injection-source"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let canary = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("trusted-fault-injection-canary"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue.write_buffer(&source, 0, &WRITTEN);
        gpu.queue.write_buffer(&canary, 0, &INITIAL);
        assert_eq!(gpu.read_buffer(&canary).as_slice(), INITIAL.as_slice());

        let mut candidate = graph::Candidate::new(graph::Limits::default()).unwrap();
        let desc = workload::buffer_desc(false);
        let prefix = candidate.reserve_resource(desc.clone()).unwrap();
        let later = candidate.reserve_resource(desc.clone()).unwrap();
        // The later operation's smaller key cannot bypass its explicit dependency.
        candidate
            .add_operation(
                LATER,
                &[graph::Read {
                    version: prefix.clone(),
                    role: graph::AccessRole::StorageRead,
                    expected: desc,
                }],
                &[graph::Write {
                    version: later.clone(),
                    role: graph::AccessRole::StorageWrite,
                    initialization: graph::Initialization::Zero,
                }],
            )
            .unwrap();
        candidate
            .add_operation(
                PREFIX,
                &[],
                &[graph::Write {
                    version: prefix,
                    role: graph::AccessRole::StorageWrite,
                    initialization: graph::Initialization::Zero,
                }],
            )
            .unwrap();
        candidate.set_output_group(&[later.clone()]).unwrap();
        let mut visited = Vec::with_capacity(2);
        let mut returned_ok = 0;
        let outcome = native::prepare(
            &gpu,
            candidate.compile().unwrap(),
            &[],
            |operation, _, encoder| {
                visited.push(operation.key);
                match operation.key {
                    PREFIX => {
                        // Deliberately undeclared access to a test-only external canary: trusted
                        // native fault injection, not an ordinary adapter or a public API seam.
                        // Only commands are recorded here; the callback never receives a Queue.
                        encoder.copy_buffer_to_buffer(&source, 0, &canary, 0, 16);
                    }
                    LATER => match fault {
                        Some(Fault::Reject) => {
                            return Err(native::NativeError::Callback(
                                "injected later encode rejection",
                            ));
                        }
                        Some(Fault::Panic) => panic!("injected later encode failure"),
                        Some(Fault::InvalidCopy) => {
                            // Both ranges are in bounds, but offset 1 violates native copy alignment.
                            // Return Ok so only the actual wgpu error scope can reject this encoder.
                            encoder.copy_buffer_to_buffer(&source, 1, &canary, 0, 4);
                        }
                        None | Some(Fault::DropReady) => {}
                    },
                    _ => {
                        return Err(native::NativeError::Callback(
                            "unknown fault-injection operation",
                        ));
                    }
                }
                returned_ok += 1;
                Ok(())
            },
        );
        assert_eq!(visited, [PREFIX, LATER]);
        match fault {
            None => {
                let submitted = native::submit(&gpu, outcome.unwrap()).unwrap();
                assert_eq!(returned_ok, 2);
                assert_eq!(gpu.read_buffer(&canary).as_slice(), WRITTEN.as_slice());
                gpu.device
                    .poll(wgpu::PollType::Wait {
                        submission_index: Some(submitted.submission_index().clone()),
                        timeout: Some(Duration::from_secs(10)),
                    })
                    .unwrap();
                assert!(submitted.is_complete());
                continue;
            }
            Some(Fault::DropReady) => {
                let ready = outcome.unwrap();
                assert_eq!(returned_ok, 2);
                let local = match ready.resources().get(&later).unwrap() {
                    native::NativeResource::Buffer(buffer) => Arc::downgrade(buffer),
                    _ => panic!("the ready output must be a buffer"),
                };
                assert_eq!(
                    gpu.read_buffer(&canary).as_slice(),
                    INITIAL.as_slice(),
                    "successful preparation submitted the encoded canary write"
                );
                drop(ready);
                assert!(
                    local.upgrade().is_none(),
                    "dropped ready work left a physical resource lease"
                );
            }
            Some(Fault::Reject) => {
                assert_eq!(returned_ok, 1);
                assert!(matches!(
                    outcome,
                    Err(native::NativeError::Callback(
                        "injected later encode rejection"
                    ))
                ));
            }
            Some(Fault::Panic) => {
                assert_eq!(returned_ok, 1);
                assert!(matches!(
                    outcome,
                    Err(native::NativeError::CallbackPanicked { operation: LATER })
                ));
            }
            Some(Fault::InvalidCopy) => {
                assert_eq!(
                    returned_ok, 2,
                    "the invalid-encoder callback itself returned Ok"
                );
                assert!(matches!(
                    outcome,
                    Err(native::NativeError::DeviceErrors {
                        stage: "candidate encoding",
                        validation: Some(_),
                        ..
                    })
                ));
            }
        }
        assert_eq!(
            gpu.read_buffer(&canary).as_slice(),
            INITIAL.as_slice(),
            "{fault:?} submitted an encoded prefix despite rejecting the candidate"
        );
        let (healthy, versions) = execute(&gpu, &effects, &workload::OPERATION_KEYS, &pixels);
        assert_output(&gpu, &healthy, &versions, &pixels);
    }
}

#[test]
#[ignore = "real native GPU resource custody after result drop; run serially"]
fn dropped_submission_releases_import_and_local_resources_only_after_its_prefix() {
    let gpu = gpu::Gpu::new();
    let effects = effects::Effects::new(&gpu.device);
    let pixels = oracle::scene(workload::WIDTH, workload::HEIGHT, 0);
    let workload = workload::build(&workload::OPERATION_KEYS).unwrap();
    let versions = workload.versions;
    let graph::ResourceDesc::Texture {
        native: descriptor, ..
    } = &workload.graph.resource(&versions.original).unwrap().desc
    else {
        unreachable!()
    };
    let input = Arc::new(gpu.device.create_texture(descriptor));
    gpu.upload_rgba32(&input, workload::WIDTH, workload::HEIGHT, &pixels);
    let imported = Arc::downgrade(&input);
    let imports = [(
        versions.original.clone(),
        native::NativeResource::Texture(input),
    )];
    let mut encoded_operations = 0;
    let accepted = native::execute(
        &gpu,
        workload.graph,
        &imports,
        |operation, resources, encoder| {
            encode_operation(&gpu, &effects, &versions, operation, resources, encoder)?;
            encoded_operations += 1;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(encoded_operations, workload::OPERATION_KEYS.len());
    let local_texture = match accepted.resources().get(&versions.composite).unwrap() {
        native::NativeResource::Texture(texture) => Arc::downgrade(texture),
        _ => panic!("the local composite must be a texture"),
    };
    let local_buffer = match accepted.resources().get(&versions.normalized).unwrap() {
        native::NativeResource::Buffer(buffer) => Arc::downgrade(buffer),
        _ => panic!("the local normalized vertex input must be a buffer"),
    };
    let completed = accepted.completion_observation();
    let index = accepted.submission_index().clone();
    drop((accepted, imports));
    // Do not read back or poll before observing custody: either would finish this small workload.
    // Source establishes the completion callback as the owner. This GPU case additionally checks
    // pending Arc custody only if pending was actually observed; it never fabricates that state.
    let observed_pending = !completed.is_complete();
    if observed_pending {
        assert!(
            imported.upgrade().is_some(),
            "pending prefix lost its imported texture Arc"
        );
        assert!(
            local_texture.upgrade().is_some(),
            "pending prefix lost its local texture Arc"
        );
        assert!(
            local_buffer.upgrade().is_some(),
            "pending prefix lost its local buffer Arc"
        );
    }
    eprintln!(
        "render-comparison drop-custody observed_pending={observed_pending} pending_arc_checks={}",
        if observed_pending {
            "performed"
        } else {
            "not exercised: already complete"
        }
    );
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(index),
            timeout: Some(Duration::from_secs(10)),
        })
        .unwrap();
    assert!(completed.is_complete());
    assert!(
        imported.upgrade().is_none(),
        "completed callback retained its imported texture Arc"
    );
    assert!(
        local_texture.upgrade().is_none(),
        "completed callback retained its local texture Arc"
    );
    assert!(
        local_buffer.upgrade().is_none(),
        "completed callback retained its local buffer Arc"
    );
}

struct AccumulatedOutput {
    submission: native::SubmittedGraph,
    output: graph::Version,
}

fn prepare_accumulation(
    gpu: &gpu::Gpu,
    effects: &effects::Effects,
    pixels: &[[f32; 4]],
    previous: Option<&Arc<AccumulatedOutput>>,
) -> (native::ReadyGraph, graph::Version) {
    let mut graph = graph::Candidate::new(graph::Limits::default()).unwrap();
    let current = graph
        .import_resource(workload::texture_desc(100), graph::ImportState::Initialized)
        .unwrap();
    let history = graph
        .import_resource(workload::texture_desc(101), graph::ImportState::Initialized)
        .unwrap();
    let output = graph.reserve_resource(workload::texture_desc(101)).unwrap();
    graph
        .add_operation(
            1,
            &[
                graph::Read {
                    version: current.clone(),
                    role: graph::AccessRole::Sampled,
                    expected: workload::texture_desc(100),
                },
                graph::Read {
                    version: history.clone(),
                    role: graph::AccessRole::Sampled,
                    expected: workload::texture_desc(101),
                },
            ],
            &[graph::Write {
                version: output.clone(),
                role: graph::AccessRole::StorageWrite,
                initialization: graph::Initialization::Clear(wgpu::Color::TRANSPARENT),
            }],
        )
        .unwrap();
    graph.set_output_group(&[output.clone()]).unwrap();
    let graph = graph.compile().unwrap();
    let graph::ResourceDesc::Texture {
        native: descriptor, ..
    } = &graph.resource(&current).unwrap().desc
    else {
        unreachable!()
    };
    let current_texture = Arc::new(gpu.device.create_texture(descriptor));
    gpu.upload_rgba32(&current_texture, workload::WIDTH, workload::HEIGHT, pixels);
    let history_texture = match previous {
        Some(previous) => previous
            .submission
            .resources()
            .get(&previous.output)
            .unwrap()
            .clone(),
        None => {
            let texture = Arc::new(gpu.device.create_texture(descriptor));
            gpu.upload_rgba32(
                &texture,
                workload::WIDTH,
                workload::HEIGHT,
                &vec![[0.0; 4]; pixels.len()],
            );
            native::NativeResource::Texture(texture)
        }
    };
    let imports = [
        (
            current.clone(),
            native::NativeResource::Texture(current_texture),
        ),
        (history.clone(), history_texture),
    ];
    let ready = native::prepare(gpu, graph, &imports, |_, resources, encoder| {
        let view = |version| {
            resources
                .texture(version)
                .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
        };
        effects.encode_accumulate(
            &gpu.device,
            encoder,
            &view(&current)?,
            &view(&history)?,
            &view(&output)?,
        );
        Ok(())
    })
    .unwrap();
    (ready, output)
}

fn submit_accumulation(
    gpu: &gpu::Gpu,
    prepared: (native::ReadyGraph, graph::Version),
) -> Arc<AccumulatedOutput> {
    let (ready, output) = prepared;
    let submission = native::submit(gpu, ready).unwrap();
    Arc::new(AccumulatedOutput { submission, output })
}

fn history_pixels(slot: usize, capture: u32) -> Vec<[f32; 4]> {
    let mut pixels = oracle::scene(workload::WIDTH, workload::HEIGHT, capture);
    for pixel in &mut pixels {
        // Distinct premultiplied channel values identify both slot and capture, independently of
        // their registration/list positions. Keep alpha-zero controls and the spatial blue pattern.
        pixel[0] = pixel[3] * (slot as f32 + 1.0) / 8.0;
        pixel[1] = pixel[3] * capture as f32 / 8.0;
    }
    pixels
}

fn expected_accumulation(current: &[[f32; 4]], previous: Option<&[[f32; 4]]>) -> Vec<[f32; 4]> {
    if let Some(previous) = previous {
        assert_eq!(current.len(), previous.len());
    }
    current
        .iter()
        .enumerate()
        .map(|(index, pixel)| {
            std::array::from_fn(|channel| {
                let old = previous.map_or(0.0, |previous| previous[index][channel]);
                0.5 * (pixel[channel] + old)
            })
        })
        .collect()
}

fn assert_accumulated_output(gpu: &gpu::Gpu, output: &AccumulatedOutput, expected: &[[f32; 4]]) {
    let texture = output
        .submission
        .resources()
        .texture(&output.output)
        .unwrap();
    oracle::assert_pixels(
        &gpu.read_rgba32(texture, workload::WIDTH, workload::HEIGHT),
        expected,
        1.0e-6,
    );
}

#[test]
#[ignore = "real native GPU exact history inputs and atomic output publication; run serially"]
fn accumulation_pins_predecessors_across_reset_reorder_late_and_failed_publication() {
    let gpu = gpu::Gpu::new();
    let effects = effects::Effects::new(&gpu.device);
    let mut histories = history::History::new();
    let keys: [_; 4] = std::array::from_fn(|index| history::SlotKey {
        producer: 1,
        view: index as u64 / 2,
        instance: index as u64 % 2,
        incarnation: 1,
        slot: 0,
    });
    const ORIGINAL_GROUP: u64 = 1;
    const REORDERED_GROUP: u64 = 2;
    for key in keys {
        histories.register(key).unwrap();
    }
    histories.register_group(ORIGINAL_GROUP, &keys).unwrap();
    let first_pixels: [_; 4] = std::array::from_fn(|slot| history_pixels(slot, 1));
    let first_expected: [_; 4] =
        std::array::from_fn(|slot| expected_accumulation(&first_pixels[slot], None));
    let first = histories.prepare(1, ORIGINAL_GROUP).unwrap();
    let first_ready: Vec<_> = (0..4)
        .map(|slot| {
            assert!(first.previous(slot).unwrap().is_none());
            prepare_accumulation(
                &gpu,
                &effects,
                &first_pixels[slot],
                first.previous(slot).unwrap(),
            )
        })
        .collect();
    histories.validate_ticket(&first).unwrap();
    let outputs: Vec<_> = first_ready
        .into_iter()
        .map(|ready| submit_accumulation(&gpu, ready))
        .collect();
    let first_retired = histories.publish(first, &outputs).unwrap();
    assert!(first_retired.is_empty());
    for slot in 0..4 {
        assert!(Arc::ptr_eq(
            histories.current(keys[slot]).unwrap().unwrap(),
            &outputs[slot]
        ));
        assert_accumulated_output(&gpu, &outputs[slot], &first_expected[slot]);
    }
    drop(first_retired);

    let order = [3, 0, 2, 1];
    let reordered = order.map(|slot| keys[slot]);
    let retired_ticket = histories.prepare(2, ORIGINAL_GROUP).unwrap();
    histories.retire_group(ORIGINAL_GROUP).unwrap();
    histories
        .register_group(REORDERED_GROUP, &reordered)
        .unwrap();
    assert!(matches!(
        histories.validate_ticket(&retired_ticket),
        Err(history::HistoryError::Retired)
    ));
    assert!(matches!(
        histories.publish(retired_ticket, &outputs),
        Err(history::HistoryError::Retired)
    ));
    assert!(matches!(
        histories.prepare(1, REORDERED_GROUP),
        Err(history::HistoryError::RepeatedCapture)
    ));
    for slot in 0..4 {
        assert!(
            Arc::ptr_eq(
                histories.current(keys[slot]).unwrap().unwrap(),
                &outputs[slot]
            ),
            "retiring and reordering a group changed slot {slot}"
        );
    }

    // Competing captures share the same immutable reordered group and the same exact basis.
    // Submit both while that basis is valid, then publish capture 3 before capture 2.
    let old = histories.prepare(2, REORDERED_GROUP).unwrap();
    let later = histories.prepare(3, REORDERED_GROUP).unwrap();
    for (position, slot) in order.into_iter().enumerate() {
        assert!(Arc::ptr_eq(
            old.previous(position).unwrap().unwrap(),
            &outputs[slot]
        ));
        assert!(Arc::ptr_eq(
            later.previous(position).unwrap().unwrap(),
            &outputs[slot]
        ));
    }
    let old_pixels: [_; 4] = std::array::from_fn(|slot| history_pixels(slot, 2));
    let old_expected: [_; 4] = std::array::from_fn(|slot| {
        expected_accumulation(&old_pixels[slot], Some(first_expected[slot].as_slice()))
    });
    let old_ready: Vec<_> = order
        .iter()
        .enumerate()
        .map(|(position, slot)| {
            prepare_accumulation(
                &gpu,
                &effects,
                &old_pixels[*slot],
                old.previous(position).unwrap(),
            )
        })
        .collect();
    histories.validate_ticket(&old).unwrap();
    let late_outputs: Vec<_> = old_ready
        .into_iter()
        .map(|ready| submit_accumulation(&gpu, ready))
        .collect();

    let later_pixels: [_; 4] = std::array::from_fn(|slot| history_pixels(slot, 3));
    let later_expected: [_; 4] = std::array::from_fn(|slot| {
        expected_accumulation(&later_pixels[slot], Some(first_expected[slot].as_slice()))
    });
    let later_ready: Vec<_> = order
        .iter()
        .enumerate()
        .map(|(position, slot)| {
            prepare_accumulation(
                &gpu,
                &effects,
                &later_pixels[*slot],
                later.previous(position).unwrap(),
            )
        })
        .collect();
    histories.validate_ticket(&later).unwrap();
    let next: Vec<_> = later_ready
        .into_iter()
        .map(|ready| submit_accumulation(&gpu, ready))
        .collect();
    let later_retired = histories.publish(later, &next).unwrap();
    assert_eq!(later_retired.len(), 4);
    let published: [_; 4] = std::array::from_fn(|slot| {
        let position = order.iter().position(|ordered| *ordered == slot).unwrap();
        assert!(Arc::ptr_eq(
            histories.current(keys[slot]).unwrap().unwrap(),
            &next[position]
        ));
        Arc::clone(histories.current(keys[slot]).unwrap().unwrap())
    });
    for slot in 0..4 {
        assert_accumulated_output(&gpu, &published[slot], &later_expected[slot]);
    }
    drop(later_retired);

    // These GPU submissions were admitted before the newer publication; only their publication
    // is late. This is not a claim of out-of-order physical completion on the serial queue.
    for (position, slot) in order.into_iter().enumerate() {
        assert!(Arc::ptr_eq(
            old.previous(position).unwrap().unwrap(),
            &outputs[slot]
        ));
        assert_accumulated_output(&gpu, &late_outputs[position], &old_expected[slot]);
    }
    assert!(matches!(
        histories.validate_ticket(&old),
        Err(history::HistoryError::StaleBasis)
    ));
    assert!(matches!(
        histories.publish(old, &late_outputs),
        Err(history::HistoryError::StaleBasis)
    ));
    for slot in 0..4 {
        assert!(
            Arc::ptr_eq(
                histories.current(keys[slot]).unwrap().unwrap(),
                &published[slot]
            ),
            "rejected late publication changed slot {slot}"
        );
        assert_accumulated_output(&gpu, &published[slot], &later_expected[slot]);
    }
    assert!(matches!(
        histories.prepare(3, REORDERED_GROUP),
        Err(history::HistoryError::RepeatedCapture)
    ));

    let stale_before_cut = histories.prepare(4, REORDERED_GROUP).unwrap();
    let reset_pixels: [_; 4] = std::array::from_fn(|slot| history_pixels(slot, 4));
    let fenced_ready: Vec<_> = order
        .iter()
        .enumerate()
        .map(|(position, slot)| {
            prepare_accumulation(
                &gpu,
                &effects,
                &reset_pixels[*slot],
                stale_before_cut.previous(position).unwrap(),
            )
        })
        .collect();
    let fenced_outputs: Vec<_> = fenced_ready
        .iter()
        .map(
            |(ready, output)| match ready.resources().get(output).unwrap() {
                native::NativeResource::Texture(texture) => Arc::downgrade(texture),
                _ => panic!("a prepared accumulation output must be a texture"),
            },
        )
        .collect();
    histories.validate_ticket(&stale_before_cut).unwrap();
    let reset_retired = histories.reset(&[keys[1]]).unwrap();
    assert_eq!(reset_retired.len(), 1);
    assert!(histories.current(keys[1]).unwrap().is_none());
    for slot in [0, 2, 3] {
        assert!(
            Arc::ptr_eq(
                histories.current(keys[slot]).unwrap().unwrap(),
                &published[slot]
            ),
            "reset changed an unrelated slot {slot}"
        );
    }
    assert!(matches!(
        histories.validate_ticket(&stale_before_cut),
        Err(history::HistoryError::StaleBasis)
    ));
    // Fence the complete encoded group before any submit. Dropping the ready candidates leaves
    // no completion lease; the independent canary test also verifies ready-drop has no GPU effect.
    drop(fenced_ready);
    assert!(
        fenced_outputs
            .iter()
            .all(|output| output.upgrade().is_none())
    );
    // Supply different valid submitted outputs so partial writes before finding the reset slot
    // cannot be hidden by writing the same current Arc back into the earlier slots. The reset
    // slot is last in the registered group, so the three unaffected members precede rejection.
    let rejected_outputs: Vec<_> = order
        .iter()
        .map(|slot| Arc::clone(&outputs[*slot]))
        .collect();
    assert!(matches!(
        histories.publish(stale_before_cut, &rejected_outputs),
        Err(history::HistoryError::StaleBasis)
    ));
    assert!(histories.current(keys[1]).unwrap().is_none());
    for slot in [0, 2, 3] {
        assert!(
            Arc::ptr_eq(
                histories.current(keys[slot]).unwrap().unwrap(),
                &published[slot]
            ),
            "rejected reset-basis publication partially changed slot {slot}"
        );
        assert_accumulated_output(&gpu, &published[slot], &later_expected[slot]);
    }
    // Reset/publication outcomes and all current pointers were observed before cleanup.
    drop(reset_retired);

    // A one-slot reset never permits a one-slot publication: reprepare every registered member.
    assert!(matches!(
        histories.prepare(3, REORDERED_GROUP),
        Err(history::HistoryError::RepeatedCapture)
    ));
    let reset = histories.prepare(4, REORDERED_GROUP).unwrap();
    for (position, slot) in order.into_iter().enumerate() {
        let previous = reset.previous(position).unwrap();
        if slot == 1 {
            assert!(previous.is_none());
        } else {
            assert!(Arc::ptr_eq(previous.unwrap(), &published[slot]));
        }
    }
    let reset_expected: [_; 4] = std::array::from_fn(|slot| {
        let previous = if slot == 1 {
            None
        } else {
            Some(later_expected[slot].as_slice())
        };
        expected_accumulation(&reset_pixels[slot], previous)
    });
    let reset_ready: Vec<_> = order
        .iter()
        .enumerate()
        .map(|(position, slot)| {
            prepare_accumulation(
                &gpu,
                &effects,
                &reset_pixels[*slot],
                reset.previous(position).unwrap(),
            )
        })
        .collect();
    histories.validate_ticket(&reset).unwrap();
    let reset_outputs: Vec<_> = reset_ready
        .into_iter()
        .map(|ready| submit_accumulation(&gpu, ready))
        .collect();
    let reset_publication_retired = histories.publish(reset, &reset_outputs).unwrap();
    assert_eq!(reset_publication_retired.len(), 3);
    for (position, slot) in order.into_iter().enumerate() {
        assert!(
            Arc::ptr_eq(
                histories.current(keys[slot]).unwrap().unwrap(),
                &reset_outputs[position]
            ),
            "complete reset successor group has the wrong current for slot {slot}"
        );
        assert_accumulated_output(&gpu, &reset_outputs[position], &reset_expected[slot]);
    }
    assert!(matches!(
        histories.prepare(4, REORDERED_GROUP),
        Err(history::HistoryError::RepeatedCapture)
    ));
    drop(reset_publication_retired);
    for slot in 0..4 {
        assert_accumulated_output(&gpu, &outputs[slot], &first_expected[slot]);
        assert_accumulated_output(&gpu, &published[slot], &later_expected[slot]);
    }
}
