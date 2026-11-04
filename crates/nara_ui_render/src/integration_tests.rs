use super::*;
use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiImageId, UiLayer, UiLocalKey,
    UiNode, UiRoot, UiRootKey, UiStyle, UiTheme, UiVisual,
};

fn frame(surface: u64, source_revision: u64, visual: UiVisual) -> Arc<UiFrame> {
    let node = UiNode::new(UiStyle::absolute(0.0, 0.0, 32.0, 32.0))
        .with_key(UiLocalKey::runtime("packet-retention", "node").unwrap())
        .with_visual(visual);
    let snapshot = UiAuthoringSnapshotBuilder::new(source_revision)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("packet-retention", "root").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_child(node),
        )
        .build()
        .unwrap();
    let environment = UiEnvironment::new(
        UiSurfaceId::new(surface).unwrap(),
        64.0,
        64.0,
        1.0,
        source_revision,
        1,
        1,
        1,
    )
    .unwrap();
    UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap()
}

fn packet_world() -> World {
    let surface_id = UiSurfaceId::new(1).unwrap();
    let route = UiPresentationRoute::new(surface_id, WindowId::PRIMARY).unwrap();
    let mut routes = UiPresentationRoutes::default();
    routes.routes.insert(surface_id, route);
    let mut world = World::new();
    world.insert_resource(UiPresentationState(
        UiPresentationPlanner::default(),
        Arc::new(()),
    ));
    world.insert_resource(routes);
    world.insert_resource(UiPresentationOutcomes::default());
    let receiver = world.resource::<UiPresentationOutcomes>().receiver.clone();
    world.insert_resource(UiBackendSubmissionQueue::new(receiver));
    world
}

fn prepared_submission(
    packet: Arc<UiPaintPacket>,
    admission_id: u64,
    presentation_epoch: u64,
) -> UiPreparedBackendSubmission {
    UiPreparedBackendSubmission::new(
        crate::UiPresentationAttemptId::new(admission_id).unwrap(),
        packet,
        crate::UiPresentationEpoch::new(presentation_epoch).unwrap(),
        crate::UiPresentationRuntimeGeneration::new(3).unwrap(),
        crate::UiPresentationCapabilityGeneration::new(5).unwrap(),
    )
}

fn bind_backend_for_test(
    submission: UiPreparedBackendSubmission,
    generation: crate::UiPresentationBackendGeneration,
) -> UiSubmittedBackendPresentation {
    submission.bind_backend_unchecked(generation)
}

fn stage_submission(
    world: &mut World,
    packet: Arc<UiPaintPacket>,
    admission_id: u64,
    presentation_epoch: u64,
) -> UiPreparedBackendSubmission {
    assert!(
        replace_ui_backend_frame(
            world,
            vec![Arc::clone(&packet)],
            vec![prepared_submission(
                packet,
                admission_id,
                presentation_epoch
            )],
        )
        .unwrap()
        .is_empty()
    );
    let (_, mut submissions) = world.resource_mut::<UiBackendSubmissionQueue>().take();
    submissions
        .pop()
        .expect("the staged test submission should be available exactly once")
}

#[test]
fn rejected_unpresented_candidate_is_removed_without_last_presented_fallback() {
    let mut world = packet_world();
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    let admitted = frame(1, 1, UiVisual::solid(white));
    compile_ui_paint_packets(&mut world, &[admitted]).unwrap();

    let missing_image = UiImageId::new("project://ui/missing.png").unwrap();
    let rejected = frame(1, 2, UiVisual::image(missing_image, white));
    let stats = compile_ui_paint_packets(&mut world, &[rejected]).unwrap();

    assert_eq!(stats.rejected_packets, 1);
    assert!(
        world
            .resource::<UiPresentationState>()
            .0
            .snapshot_planned_packets()
            .is_empty()
    );
}

#[test]
fn two_surfaces_cannot_compete_for_one_target() {
    let mut world = packet_world();
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
    let original = world
        .resource::<UiPresentationState>()
        .0
        .snapshot_planned_packets()[0]
        .clone();
    replace_ui_presentation_routes(
        &mut world,
        [
            UiPresentationRoute::new(UiSurfaceId::new(1).unwrap(), WindowId::PRIMARY).unwrap(),
            UiPresentationRoute::new(UiSurfaceId::new(2).unwrap(), WindowId::PRIMARY).unwrap(),
        ],
    )
    .unwrap();
    let stats = compile_ui_paint_packets(
        &mut world,
        &[
            frame(1, 2, UiVisual::solid(white)),
            frame(2, 2, UiVisual::solid(white)),
        ],
    )
    .unwrap();

    assert_eq!(stats.admitted_packets, 0);
    assert_eq!(stats.rejected_packets, 2);
    assert!(Arc::ptr_eq(
        &world
            .resource::<UiPresentationState>()
            .0
            .snapshot_planned_packets()[0],
        &original
    ));
}

#[test]
fn retiring_a_conflicting_route_does_not_release_the_target_owner() {
    let mut world = packet_world();
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
    let original = world
        .resource::<UiPresentationState>()
        .0
        .snapshot_planned_packets()[0]
        .clone();
    let second_surface = UiSurfaceId::new(2).unwrap();
    replace_ui_presentation_routes(
        &mut world,
        [
            UiPresentationRoute::new(UiSurfaceId::new(1).unwrap(), WindowId::PRIMARY).unwrap(),
            UiPresentationRoute::new(second_surface, WindowId::PRIMARY).unwrap(),
        ],
    )
    .unwrap();
    replace_ui_presentation_routes(
        &mut world,
        [UiPresentationRoute::new(UiSurfaceId::new(1).unwrap(), WindowId::PRIMARY).unwrap()],
    )
    .unwrap();

    let packets = world
        .resource::<UiPresentationState>()
        .0
        .snapshot_planned_packets();
    assert_eq!(packets.len(), 1);
    assert!(Arc::ptr_eq(&packets[0], &original));
}

#[test]
fn duplicate_surface_frames_reject_the_whole_surface_attempt() {
    let mut world = packet_world();
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
    let original = world
        .resource::<UiPresentationState>()
        .0
        .snapshot_planned_packets()[0]
        .clone();

    let stats = compile_ui_paint_packets(
        &mut world,
        &[
            frame(1, 2, UiVisual::solid(white)),
            frame(1, 3, UiVisual::solid(white)),
        ],
    )
    .unwrap();

    assert_eq!(stats.admitted_packets, 0);
    assert_eq!(stats.rejected_packets, 2);
    assert!(Arc::ptr_eq(
        &world
            .resource::<UiPresentationState>()
            .0
            .snapshot_planned_packets()[0],
        &original
    ));
    let outcomes = drain_ui_presentation_outcomes(&mut world).unwrap();
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes.iter().all(|outcome| {
        outcome
            .message()
            .is_some_and(|message| message.contains("more than once"))
    }));
}

#[test]
fn outcome_journal_does_not_silently_truncate_backend_results() {
    let mut world = packet_world();
    compile_ui_paint_packets(
        &mut world,
        &[frame(
            1,
            1,
            UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
        )],
    )
    .unwrap();
    let packet = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    let submissions = (1..=65)
        .map(|index| prepared_submission(Arc::clone(&packet), index, index + 100))
        .collect::<Vec<_>>();
    let mut outcomes = UiPresentationOutcomes::default();
    let permits = outcomes.reserve_backend(&submissions).unwrap();
    for (index, (submission, permit)) in submissions.into_iter().zip(permits).enumerate() {
        let submission = UiBackendSubmission::pair(submission, permit).unwrap();
        outcomes.commit_backend(submission.content_rejected(index.to_string()));
    }

    assert_eq!(outcomes.drain().unwrap().len(), 65);
}

#[test]
fn backend_outcome_reservation_blocks_early_drain_until_every_attempt_commits() {
    let mut world = packet_world();
    compile_ui_paint_packets(
        &mut world,
        &[frame(
            1,
            1,
            UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
        )],
    )
    .unwrap();
    let packet = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    let submissions = vec![
        prepared_submission(Arc::clone(&packet), 11, 7),
        prepared_submission(packet, 12, 8),
    ];
    let mut outcomes = UiPresentationOutcomes::default();
    let permits = outcomes.reserve_backend(&submissions).unwrap();
    let mut submissions = submissions
        .into_iter()
        .zip(permits)
        .map(|(submission, permit)| UiBackendSubmission::pair(submission, permit).unwrap());
    assert_eq!(
        outcomes.drain().unwrap_err(),
        UiPresentationIntegrationError::OutstandingBackendOutcomeReservations { count: 2 }
    );
    outcomes.commit_backend(
        submissions
            .next()
            .unwrap()
            .content_rejected("first".to_owned()),
    );
    assert_eq!(
        outcomes.drain().unwrap_err(),
        UiPresentationIntegrationError::OutstandingBackendOutcomeReservations { count: 1 }
    );
    outcomes.commit_backend(
        submissions
            .next()
            .unwrap()
            .content_rejected("second".to_owned()),
    );
    assert_eq!(outcomes.drain().unwrap().len(), 2);
}

#[test]
fn host_external_backend_frame_returns_each_outcome_to_its_owning_runtime_once() {
    let mut runtime = packet_world();
    compile_ui_paint_packets(
        &mut runtime,
        &[frame(
            1,
            1,
            UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
        )],
    )
    .unwrap();
    let packet = runtime
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    assert!(
        replace_ui_backend_frame(
            &mut runtime,
            vec![Arc::clone(&packet)],
            vec![prepared_submission(packet, 21, 9)],
        )
        .unwrap()
        .is_empty()
    );

    let frame = take_ui_backend_presentation_frame(&mut runtime).unwrap();
    assert_eq!(
        take_ui_backend_presentation_frame(&mut runtime).unwrap_err(),
        UiPresentationIntegrationError::OutstandingBackendOutcomeReservations { count: 1 }
    );
    let (_, submissions) = frame.into_parts();
    let mut submitted = bind_backend_submission_batch(
        submissions,
        UiPresentationBackendGeneration::new(7, 3).unwrap(),
    )
    .unwrap();
    let completed = submitted
        .pop()
        .expect("the external frame should retain its exact submission")
        .deferred("the Editor window is temporarily unavailable".to_owned());

    let mut foreign_runtime = packet_world();
    let completed = commit_ui_backend_outcome(&mut foreign_runtime, completed)
        .expect_err("a foreign runtime must not consume the outcome permit");
    commit_ui_backend_outcome(&mut runtime, completed)
        .expect("the owning runtime should consume its exact permit");

    let outcomes = drain_ui_presentation_outcomes(&mut runtime).unwrap();
    assert_eq!(outcomes.len(), 1);
    assert!(matches!(
        outcomes[0],
        UiPresentationOutcome::Deferred { .. }
    ));
}

#[test]
fn identical_submission_identities_cannot_return_into_a_replacement_outcome_owner() {
    let mut world = packet_world();
    compile_ui_paint_packets(
        &mut world,
        &[frame(
            1,
            1,
            UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
        )],
    )
    .unwrap();
    let packet = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    let original = prepared_submission(Arc::clone(&packet), 91, 9);
    let replacement = prepared_submission(packet, 91, 9);
    assert_eq!(original.identity(), replacement.identity());
    let mut old = world.remove_resource::<UiPresentationOutcomes>().unwrap();
    let mut new = UiPresentationOutcomes::default();
    let mut old_permits = old
        .reserve_backend(std::slice::from_ref(&original))
        .unwrap();
    let mut new_permits = new
        .reserve_backend(std::slice::from_ref(&replacement))
        .unwrap();
    let old_submission = UiBackendSubmission::pair(original, old_permits.pop().unwrap()).unwrap();
    let new_submission =
        UiBackendSubmission::pair(replacement, new_permits.pop().unwrap()).unwrap();
    world.insert_resource(new);

    let returned = commit_ui_backend_outcome(
        &mut world,
        old_submission.content_rejected("old".to_owned()),
    )
    .expect_err("equal value identities do not authorize a different reserved storage owner");
    assert_eq!(old.pending_backend_count(), 1);
    assert_eq!(
        world
            .resource::<UiPresentationOutcomes>()
            .pending_backend_count(),
        1
    );
    drop(returned);
    let mut abandoned = old.drain().unwrap();
    assert_eq!(abandoned.len(), 1);
    assert!(
        matches!(abandoned.pop().unwrap(), UiPresentationOutcome::Aborted { attempt, .. }
        if attempt.admission_id().unwrap().get() == 91)
    );
    assert!(drain_ui_presentation_outcomes(&mut world).is_err());
    commit_ui_backend_outcome(
        &mut world,
        new_submission.content_rejected("new".to_owned()),
    )
    .unwrap();
    assert!(matches!(
        drain_ui_presentation_outcomes(&mut world)
            .unwrap()
            .pop()
            .unwrap(),
        UiPresentationOutcome::ContentRejected { .. }
    ));
}

#[test]
fn dropping_an_external_frame_returns_its_original_reserved_outcomes_after_resource_removal() {
    let mut world = packet_world();
    compile_ui_paint_packets(
        &mut world,
        &[frame(
            1,
            1,
            UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
        )],
    )
    .unwrap();
    let packet = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    replace_ui_backend_frame(
        &mut world,
        vec![Arc::clone(&packet)],
        vec![prepared_submission(packet, 92, 9)],
    )
    .unwrap();
    let external = take_ui_backend_presentation_frame(&mut world).unwrap();
    let mut original = world.remove_resource::<UiPresentationOutcomes>().unwrap();
    world.insert_resource(UiPresentationOutcomes::default());
    drop(external);
    assert_eq!(original.pending_backend_count(), 0);
    let original = original.drain().unwrap();
    assert_eq!(original.len(), 1);
    assert!(
        matches!(&original[0], UiPresentationOutcome::Aborted { attempt, .. }
        if attempt.admission_id().unwrap().get() == 92)
    );
    assert!(
        drain_ui_presentation_outcomes(&mut world)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn outcome_storage_survives_its_world_resource_until_the_last_abandoned_permission() {
    let mut world = packet_world();
    compile_ui_paint_packets(
        &mut world,
        &[frame(
            1,
            1,
            UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
        )],
    )
    .unwrap();
    let packet = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    let submissions = [
        prepared_submission(Arc::clone(&packet), 93, 9),
        prepared_submission(packet, 94, 10),
    ];
    let permits = world
        .resource_mut::<UiPresentationOutcomes>()
        .reserve_backend(&submissions)
        .unwrap();
    let mut external = submissions
        .into_iter()
        .zip(permits)
        .map(|(submission, permit)| UiBackendSubmission::pair(submission, permit).unwrap())
        .collect::<Vec<_>>();
    let retained = Arc::downgrade(&external[0].permit.owner);
    drop(world.remove_resource::<UiPresentationOutcomes>().unwrap());
    drop(world);
    drop(external.pop());
    assert!(retained.upgrade().is_some());
    drop(external);
    assert!(retained.upgrade().is_none());
}

#[test]
fn original_receiver_keeps_returned_outcomes_after_world_ingress_and_candidate_are_dropped() {
    let mut world = packet_world();
    let receiver = backend_outcome_receiver(&world).unwrap();
    compile_ui_paint_packets(
        &mut world,
        &[frame(
            1,
            1,
            UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
        )],
    )
    .unwrap();
    let packet = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    replace_ui_backend_frame(
        &mut world,
        vec![Arc::clone(&packet)],
        vec![prepared_submission(packet, 98, 9)],
    )
    .unwrap();
    let external = take_ui_backend_presentation_frame(&mut world).unwrap();
    drop(world.remove_resource::<UiPresentationOutcomes>().unwrap());
    drop(world);

    receiver.begin_close();
    assert_eq!(receiver.pending_backend_count(), 1);
    assert_eq!(
        receiver.drain().unwrap_err(),
        UiPresentationIntegrationError::OutstandingBackendOutcomeReservations { count: 1 }
    );
    drop(external);
    assert_eq!(receiver.pending_backend_count(), 0);
    let returned = receiver.drain().unwrap();
    assert_eq!(returned.len(), 1);
    assert!(
        matches!(&returned[0], UiPresentationOutcome::Aborted { attempt, .. }
        if attempt.admission_id().unwrap().get() == 98)
    );
    assert!(receiver.drain().unwrap().is_empty());
}

#[test]
fn original_receiver_close_refuses_new_reservations_through_its_surviving_ingress() {
    let mut world = packet_world();
    let receiver = backend_outcome_receiver(&world).unwrap();
    receiver.begin_close();
    assert!(matches!(
        world
            .resource_mut::<UiPresentationOutcomes>()
            .reserve_backend(&[]),
        Err(UiPresentationIntegrationError::BackendOutcomeAdmissionClosed)
    ));
    assert_eq!(receiver.pending_backend_count(), 0);
    assert!(receiver.drain().unwrap().is_empty());
}

#[test]
fn replacing_outcome_ingress_before_transfer_refuses_without_consuming_the_original_queue() {
    let mut world = packet_world();
    let receiver = backend_outcome_receiver(&world).unwrap();
    compile_ui_paint_packets(
        &mut world,
        &[frame(
            1,
            1,
            UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
        )],
    )
    .unwrap();
    let packet = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    replace_ui_backend_frame(
        &mut world,
        vec![Arc::clone(&packet)],
        vec![prepared_submission(packet, 99, 9)],
    )
    .unwrap();
    let original = world.remove_resource::<UiPresentationOutcomes>().unwrap();
    world.insert_resource(UiPresentationOutcomes::default());
    let planned = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    assert!(matches!(
        compile_ui_paint_packets(
            &mut world,
            &[frame(
                1,
                2,
                UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0))
            )],
        ),
        Err(UiPresentationIntegrationError::BackendOutcomeOwnerChanged)
    ));
    assert!(Arc::ptr_eq(
        &planned,
        &world
            .resource::<UiPresentationState>()
            .0
            .planned(RenderTarget::Window(WindowId::PRIMARY))
            .unwrap()
    ));
    assert!(matches!(
        take_ui_backend_presentation_frame(&mut world),
        Err(UiPresentationIntegrationError::BackendOutcomeOwnerChanged)
    ));
    assert_eq!(
        world
            .resource::<UiBackendSubmissionQueue>()
            .submissions
            .len(),
        1
    );
    assert_eq!(receiver.pending_backend_count(), 0);
    assert_eq!(
        world
            .resource::<UiPresentationOutcomes>()
            .pending_backend_count(),
        0
    );

    world.insert_resource(original);
    let external = take_ui_backend_presentation_frame(&mut world).unwrap();
    assert_eq!(receiver.pending_backend_count(), 1);
    drop(external);
    assert!(matches!(
        receiver.drain().unwrap().pop().unwrap(),
        UiPresentationOutcome::Aborted { .. }
    ));
}

#[test]
fn last_permit_return_cannot_admit_a_new_compile_before_its_original_outcome_is_drained() {
    use std::sync::Barrier;

    let mut world = packet_world();
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
    let original = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    let next = frame(1, 2, UiVisual::solid(white));
    for admission in 100..132 {
        let submissions = [prepared_submission(
            Arc::clone(&original),
            admission,
            admission,
        )];
        let mut permits = world
            .resource_mut::<UiPresentationOutcomes>()
            .reserve_backend(&submissions)
            .unwrap();
        let permit = permits.pop().unwrap();
        let barrier = Arc::new(Barrier::new(2));
        std::thread::scope(|scope| {
            let releasing = Arc::clone(&barrier);
            let release = scope.spawn(move || {
                releasing.wait();
                drop(permit);
            });
            barrier.wait();
            assert!(matches!(
                compile_ui_paint_packets(&mut world, &[Arc::clone(&next)]),
                Err(
                    UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
                        count: 1
                    } | UiPresentationIntegrationError::PendingPresentationOutcomes { count: 1 }
                )
            ));
            release.join().unwrap();
        });
        let retained = world
            .resource::<UiPresentationState>()
            .0
            .planned(RenderTarget::Window(WindowId::PRIMARY))
            .unwrap();
        assert!(Arc::ptr_eq(&retained, &original));
        assert_eq!(drain_ui_presentation_outcomes(&mut world).unwrap().len(), 1);
    }
}

#[test]
fn mixed_owner_batch_refuses_before_any_commit_and_abandonment_remains_owner_local() {
    let mut world = packet_world();
    compile_ui_paint_packets(
        &mut world,
        &[frame(
            1,
            1,
            UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
        )],
    )
    .unwrap();
    let packet = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    let a = [
        prepared_submission(Arc::clone(&packet), 95, 9),
        prepared_submission(Arc::clone(&packet), 96, 10),
    ];
    let b = [prepared_submission(packet, 97, 11)];
    let mut original = UiPresentationOutcomes::default();
    let mut foreign = UiPresentationOutcomes::default();
    let permits = original.reserve_backend(&a).unwrap();
    let mut a = a
        .into_iter()
        .zip(permits)
        .map(|(submission, permit)| UiBackendSubmission::pair(submission, permit).unwrap());
    let mut permits = foreign.reserve_backend(&b).unwrap();
    let [b] = b;
    let b = UiBackendSubmission::pair(b, permits.pop().unwrap()).unwrap();
    let returned = original
        .try_commit_backend_batch(vec![
            a.next().unwrap().content_rejected("a".to_owned()),
            b.content_rejected("b".to_owned()),
        ])
        .unwrap_err();
    assert_eq!(original.pending_backend_count(), 2);
    assert_eq!(foreign.pending_backend_count(), 1);
    drop(returned);
    original.commit_backend(a.next().unwrap().content_rejected("second".to_owned()));
    let original = original.drain().unwrap();
    assert_eq!(original.len(), 2);
    assert!(matches!(original[0], UiPresentationOutcome::Aborted { .. }));
    assert!(matches!(
        original[1],
        UiPresentationOutcome::ContentRejected { .. }
    ));
    assert!(matches!(
        foreign.drain().unwrap().pop().unwrap(),
        UiPresentationOutcome::Aborted { .. }
    ));
}

#[test]
fn undrained_outcomes_block_the_next_compile_before_planner_mutation() {
    let mut world = packet_world();
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    let missing_image = UiImageId::new("project://ui/missing.png").unwrap();
    let rejected = frame(1, 1, UiVisual::image(missing_image, white));
    let rejected_attempt = UiPresentationAttemptIdentity::routed(
        UiPaintPacketIdentity::from_frame(&rejected),
        RenderTarget::Window(WindowId::PRIMARY),
    );
    assert_eq!(
        compile_ui_paint_packets(&mut world, &[rejected])
            .unwrap()
            .rejected_packets,
        1
    );
    let rejected_stats = world.resource::<UiPresentationState>().0.work_stats();
    assert_eq!(rejected_stats.plan_attempts, 1);
    assert_eq!(rejected_stats.planned_packets, 0);
    assert_eq!(rejected_stats.lowered_primitives, 0);
    assert_eq!(rejected_stats.text, nara_text::TextWorkStats::default());

    let error =
        compile_ui_paint_packets(&mut world, &[frame(1, 2, UiVisual::solid(white))]).unwrap_err();

    assert_eq!(
        error,
        UiPresentationIntegrationError::PendingPresentationOutcomes { count: 1 }
    );
    assert_eq!(
        world.resource::<UiPresentationState>().0.work_stats(),
        rejected_stats
    );
    assert!(
        world
            .resource::<UiPresentationState>()
            .0
            .planned(RenderTarget::Window(WindowId::PRIMARY))
            .is_none()
    );
    let outcomes = drain_ui_presentation_outcomes(&mut world).unwrap();
    assert_eq!(outcomes.len(), 1);
    assert!(matches!(
        &outcomes[0],
        UiPresentationOutcome::ContentRejected { .. }
    ));
    assert_eq!(outcomes[0].attempt(), Some(rejected_attempt));
    let expected_message = UiPresentationContentError::MissingImage {
        id: "project://ui/missing.png".to_owned(),
    }
    .to_string();
    assert_eq!(outcomes[0].message(), Some(expected_message.as_str()));
}

#[test]
fn planner_advances_presented_only_after_consuming_exact_backend_acceptance() {
    let mut world = packet_world();
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
    let packet = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    assert!(
        world
            .resource::<UiPresentationState>()
            .0
            .last_presented(RenderTarget::Window(WindowId::PRIMARY))
            .is_none()
    );

    let prepared = stage_submission(&mut world, Arc::clone(&packet), 11, 7);
    let submitted = bind_backend_for_test(
        prepared,
        crate::UiPresentationBackendGeneration::new(13, 17).unwrap(),
    );
    let accepted =
        commit_ui_backend_presentation(&mut world, OneShotPresentationCommit::issue(submitted));

    assert_eq!(accepted.identity().admission_id().get(), 11);
    assert!(Arc::ptr_eq(accepted.packet(), &packet));
    assert!(Arc::ptr_eq(
        &world
            .resource::<UiPresentationState>()
            .0
            .last_presented(RenderTarget::Window(WindowId::PRIMARY))
            .unwrap(),
        &packet
    ));
}

#[test]
fn stale_backend_acceptance_is_discarded_before_planner_commit() {
    let mut world = packet_world();
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
    let first = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    let generation = crate::UiPresentationBackendGeneration::new(13, 17).unwrap();
    let stale_prepared = stage_submission(&mut world, Arc::clone(&first), 21, 8);
    let stale_identity = stale_prepared.identity();
    let stale = OneShotPresentationCommit::issue(bind_backend_for_test(stale_prepared, generation));
    assert_eq!(
        abort_ui_planned_packet(
            &mut world,
            UiPresentationAttemptIdentity::backend(stale_identity),
        ),
        UiPlannedPacketAbortOutcome::RemovedUnpresentedTarget,
    );

    let discarded = discard_ui_backend_acceptance(stale);
    assert_eq!(discarded.identity().admission_id().get(), 21);
    assert!(Arc::ptr_eq(discarded.packet(), &first));
    assert!(
        world
            .resource::<UiPresentationState>()
            .0
            .last_presented(RenderTarget::Window(WindowId::PRIMARY))
            .is_none()
    );
}

#[test]
fn active_backend_submission_cannot_be_replaced_by_a_new_admission() {
    let mut world = packet_world();
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
    let packet = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    let first = stage_submission(&mut world, Arc::clone(&packet), 21, 8);
    let error = replace_ui_backend_frame(
        &mut world,
        vec![Arc::clone(&packet)],
        vec![prepared_submission(Arc::clone(&packet), 22, 9)],
    )
    .unwrap_err();

    assert_eq!(
        error,
        UiPresentationIntegrationError::SubmissionPlannerMismatch { admission_id: 22 }
    );

    let generation = crate::UiPresentationBackendGeneration::new(13, 17).unwrap();
    let submitted = bind_backend_for_test(first, generation);
    let accepted =
        commit_ui_backend_presentation(&mut world, OneShotPresentationCommit::issue(submitted));
    assert_eq!(accepted.identity().admission_id().get(), 21);
}

#[test]
fn rejected_exact_plan_restores_presented_or_removes_unpresented_target() {
    let mut world = packet_world();
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
    let first = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    let generation = crate::UiPresentationBackendGeneration::new(13, 17).unwrap();
    let first_submission = bind_backend_for_test(
        stage_submission(&mut world, Arc::clone(&first), 31, 10),
        generation,
    );
    let _accepted = commit_ui_backend_presentation(
        &mut world,
        OneShotPresentationCommit::issue(first_submission),
    );

    compile_ui_paint_packets(&mut world, &[frame(1, 2, UiVisual::solid(white))]).unwrap();
    let rejected = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    let rejected_attempt =
        UiPresentationAttemptIdentity::routed(rejected.identity(), rejected.target());
    assert_eq!(
        abort_ui_planned_packet(&mut world, rejected_attempt),
        UiPlannedPacketAbortOutcome::RestoredLastPresented
    );
    assert!(Arc::ptr_eq(
        &world
            .resource::<UiPresentationState>()
            .0
            .planned(RenderTarget::Window(WindowId::PRIMARY))
            .unwrap(),
        &first
    ));

    let mut fresh = packet_world();
    compile_ui_paint_packets(&mut fresh, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
    let unpresented = fresh
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    let attempt =
        UiPresentationAttemptIdentity::routed(unpresented.identity(), unpresented.target());
    assert_eq!(
        abort_ui_planned_packet(&mut fresh, attempt),
        UiPlannedPacketAbortOutcome::RemovedUnpresentedTarget
    );
    assert!(
        fresh
            .resource::<UiPresentationState>()
            .0
            .planned(RenderTarget::Window(WindowId::PRIMARY))
            .is_none()
    );
}

#[test]
fn backend_frame_exchange_preserves_exact_attempts_for_supersession() {
    let mut world = packet_world();
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
    let packet = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();

    let first = prepared_submission(Arc::clone(&packet), 41, 13);
    assert!(
        replace_ui_backend_frame(&mut world, vec![Arc::clone(&packet)], vec![first])
            .unwrap()
            .is_empty()
    );

    let successor = prepared_submission(Arc::clone(&packet), 42, 14);
    let mut displaced =
        replace_ui_backend_frame(&mut world, vec![Arc::clone(&packet)], vec![successor]).unwrap();
    assert_eq!(displaced.len(), 1);
    let queued = world.resource::<UiBackendSubmissionQueue>();
    let successor = queued.submissions.first().unwrap();
    let outcome = crate::__private::supersede_prepared(
        displaced.remove(0),
        successor,
        "a newer Host admission replaced the unsubmitted attempt".to_owned(),
    );
    assert_eq!(outcome.attempt().unwrap().admission_id().unwrap().get(), 41);
    assert_eq!(
        outcome.successor().unwrap().admission_id().unwrap().get(),
        42
    );
}

#[test]
fn backend_frame_exchange_rejects_a_submission_without_its_exact_packet_arc() {
    let mut world = packet_world();
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
    let packet = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();
    let submission = prepared_submission(packet, 51, 15);

    assert!(matches!(
        replace_ui_backend_frame(&mut world, Vec::new(), vec![submission]),
        Err(UiPresentationIntegrationError::SubmissionPacketMissing { admission_id: 51 })
    ));
}

#[test]
fn backend_frame_exchange_rejects_two_admissions_for_one_target() {
    let mut world = packet_world();
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
    let packet = world
        .resource::<UiPresentationState>()
        .0
        .planned(RenderTarget::Window(WindowId::PRIMARY))
        .unwrap();

    let error = replace_ui_backend_frame(
        &mut world,
        vec![Arc::clone(&packet)],
        vec![
            prepared_submission(Arc::clone(&packet), 61, 16),
            prepared_submission(Arc::clone(&packet), 62, 17),
        ],
    )
    .unwrap_err();

    assert_eq!(
        error,
        UiPresentationIntegrationError::DuplicateBackendTarget {
            target: RenderTarget::Window(WindowId::PRIMARY),
            first_admission_id: 61,
            second_admission_id: 62,
        }
    );
    assert!(
        world
            .resource::<UiBackendSubmissionQueue>()
            .submissions
            .is_empty()
    );
}
