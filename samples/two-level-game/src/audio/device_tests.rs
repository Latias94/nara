use super::super::{
    AudioCause, AudioPhase, AudioPolicy, AudioRecord, AudioWitness as GameWitness, GameAudioState,
    MAX_CAUSES,
};
use super::super::{clips::ClipKind, mixer::EffectId};
use super::*;
use crate::levels::{LevelId, LevelKey};

struct CallbackControl {
    control: AudioController,
    witness: AudioWitness,
    callback: Option<CallbackState>,
    error_release: Option<ReleaseToken>,
}

impl CallbackControl {
    fn new(capture_capacity: Option<usize>) -> Self {
        let shared = Arc::new(Shared::new(7));
        let (commands, input) = RingBuffer::new(COMMAND_CAPACITY);
        let (output, records) = RingBuffer::new(CUE_RECORD_CAPACITY + SCOPE_RECORD_CAPACITY);
        let (capture_output, capture) = match capture_capacity {
            Some(capacity) => {
                let (producer, consumer) = RingBuffer::new(capacity);
                (Some(producer), Some(consumer))
            }
            None => (None, None),
        };
        let control = AudioController {
            shared: shared.clone(),
            commands: Arc::new(Mutex::new(commands)),
        };
        let witness = AudioWitness {
            shared: shared.clone(),
            observations: Arc::new(Mutex::new(Observations { records, capture })),
        };
        let clips =
            PreparedClips::new(Arc::from([0.5, 0.25, -0.5]), Arc::from([-0.25, 0.75])).unwrap();
        let callback = CallbackState {
            mixer: Mixer::new(clips),
            admission: control.commands.clone(),
            commands: input,
            records: output,
            capture: capture_output,
            gate: None,
            release: ReleaseToken::new(shared.clone(), true),
        };
        let error_release = ReleaseToken::new(shared.clone(), false);
        shared.play_admitted.store(true, Ordering::Release);
        shared.admission_open.store(true, Ordering::Release);
        Self {
            control,
            witness,
            callback: Some(callback),
            error_release: Some(error_release),
        }
    }

    fn output(&mut self, frames: usize) -> Vec<f32> {
        let mut output = vec![0.0; frames * OUTPUT_CHANNELS];
        self.callback.as_mut().unwrap().output(&mut output);
        output
    }
}

fn scope(instance: u64) -> Scope {
    Scope {
        runtime: 7,
        instance,
    }
}

fn cue(instance: u64, sequence: u64) -> Cue {
    Cue {
        id: EffectId {
            scope: scope(instance),
            sequence,
        },
        clip: ClipKind::Pickup,
    }
}

fn cue_records(witness: &AudioWitness) -> Vec<(Cue, u64, CueRecordKind)> {
    witness
        .drain_records()
        .into_iter()
        .filter_map(|record| match record {
            DeviceRecord::Cue { cue, frame, kind } => Some((cue, frame, kind)),
            DeviceRecord::ScopeApplied { .. } => None,
        })
        .collect()
}

fn game_witness(callback: &CallbackControl, policy: AudioPolicy) -> GameWitness {
    callback
        .control
        .shared
        .stream_created
        .store(true, Ordering::Release);
    GameWitness(Arc::new(Mutex::new(GameAudioState::new(
        callback.control.clone(),
        callback.witness.clone(),
        policy,
    ))))
}

fn publish_game(witness: &GameWitness, instance: u64, clip: ClipKind) {
    let level = LevelKey::main(LevelId::Meadow);
    let cause = match clip {
        ClipKind::Pickup => AudioCause::Pickup {
            level,
            entity: "gem-near".into(),
        },
        ClipKind::Travel => AudioCause::Travel { level },
    };
    witness
        .0
        .lock()
        .unwrap()
        .publish(scope(instance), clip, cause);
}

#[test]
fn development_suppression_uses_no_effect_or_queue_capacity_and_its_count_saturates() {
    let mut callback = CallbackControl::new(None);
    let game = game_witness(&callback, AudioPolicy::ResetOnRebuild);
    for _ in 0..MAX_CAUSES + 7 {
        publish_game(&game, 1, ClipKind::Pickup);
    }
    publish_game(&game, 2, ClipKind::Travel);
    assert_eq!(callback.output(3), [0.0; 6]);
    let snapshot = game.snapshot();
    assert!(snapshot.reset_on_rebuild && !snapshot.output_enabled);
    assert_eq!(snapshot.suppressed_effects, (MAX_CAUSES + 8) as u64);
    assert_eq!(snapshot.desired_instance, 2);
    assert_eq!(snapshot.applied_instance, 2);
    assert_eq!(
        (snapshot.accepted, snapshot.started, snapshot.completed),
        (0, 0, 0)
    );
    assert_eq!(snapshot.faults, 0);
    assert!(snapshot.error.is_none());
    assert!(
        snapshot
            .records
            .iter()
            .all(|record| matches!(record, AudioRecord::ScopeApplied { instance: 2, .. }))
    );
    assert_eq!(
        callback.control.commands.lock().unwrap().slots(),
        COMMAND_CAPACITY
    );
    assert_eq!(
        callback
            .control
            .shared
            .cue_record_slots
            .load(Ordering::Acquire),
        CUE_RECORD_CAPACITY
    );
    {
        let mut state = game.0.lock().unwrap();
        assert_eq!(state.next_sequence, 1);
        assert!(state.causes.is_empty());
        assert_eq!(state.journal.cues, 0);
        state.suppressed_effects = u64::MAX - 1;
    }
    publish_game(&game, 2, ClipKind::Pickup);
    publish_game(&game, 2, ClipKind::Travel);
    assert_eq!(game.snapshot().suppressed_effects, u64::MAX);
    assert!(game.snapshot().error.is_none());
}

#[test]
fn development_enablement_never_replays_and_new_cues_keep_literal_pcm_and_first_identity() {
    let mut callback = CallbackControl::new(Some(16));
    let game = game_witness(&callback, AudioPolicy::ResetOnRebuild);
    publish_game(&game, 1, ClipKind::Pickup);
    publish_game(&game, 1, ClipKind::Travel);
    assert_eq!(callback.output(2), [0.0; 4]);
    game.enable_development_output().unwrap();
    game.enable_development_output().unwrap();
    assert_eq!(callback.output(3), [0.0; 6]);
    let enabled = game.snapshot();
    assert!(enabled.output_enabled);
    assert_eq!(enabled.suppressed_effects, 2);
    assert_eq!(enabled.accepted, 0);
    publish_game(&game, 1, ClipKind::Pickup);
    assert_eq!(
        callback.output(4),
        [0.5, 0.5, 0.25, 0.25, -0.5, -0.5, 0.0, 0.0]
    );
    publish_game(&game, 1, ClipKind::Travel);
    assert_eq!(callback.output(3), [-0.25, -0.25, 0.75, 0.75, 0.0, 0.0]);
    let snapshot = game.snapshot();
    assert_eq!(
        (snapshot.accepted, snapshot.started, snapshot.completed),
        (2, 2, 2)
    );
    assert_eq!(snapshot.cancelled, 0);
    assert_eq!(snapshot.suppressed_effects, 2);
    assert_eq!(snapshot.faults, 0);
    assert!(snapshot.error.is_none());
    let sequences = snapshot
        .records
        .iter()
        .filter_map(|record| match record {
            AudioRecord::Cue {
                sequence,
                phase: AudioPhase::Accepted,
                ..
            } => Some(*sequence),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(sequences, [1, 2]);
    let captured = game
        .drain_output()
        .into_iter()
        .flat_map(|chunk| chunk.samples)
        .collect::<Vec<_>>();
    let mut expected = vec![0.0; 10];
    expected.extend([0.5, 0.5, 0.25, 0.25, -0.5, -0.5, 0.0, 0.0]);
    expected.extend([-0.25, -0.25, 0.75, 0.75, 0.0, 0.0]);
    assert_eq!(captured, expected);
}

#[test]
fn development_gate_refuses_ordinary_unstarted_faulted_and_retired_owners() {
    let mut callback = CallbackControl::new(None);
    let ordinary = game_witness(&callback, AudioPolicy::Ordinary);
    assert!(!ordinary.snapshot().reset_on_rebuild);
    assert!(ordinary.snapshot().output_enabled);
    assert!(ordinary.enable_development_output().is_err());
    publish_game(&ordinary, 1, ClipKind::Pickup);
    assert_eq!(callback.output(3), [0.5, 0.5, 0.25, 0.25, -0.5, -0.5]);
    assert_eq!(ordinary.snapshot().suppressed_effects, 0);

    let game = game_witness(&callback, AudioPolicy::ResetOnRebuild);
    callback
        .control
        .shared
        .play_admitted
        .store(false, Ordering::Release);
    assert!(game.enable_development_output().is_err());
    assert!(!game.snapshot().output_enabled);
    callback
        .control
        .shared
        .play_admitted
        .store(true, Ordering::Release);
    callback
        .control
        .shared
        .admission_open
        .store(false, Ordering::Release);
    assert!(game.enable_development_output().is_err());
    callback
        .control
        .shared
        .admission_open
        .store(true, Ordering::Release);
    game.enable_development_output().unwrap();
    callback
        .control
        .shared
        .fault(DeviceFault::NativeStreamInvalidated);
    assert!(game.enable_development_output().is_err());
    assert!(!game.snapshot().output_enabled);

    let mut callback = CallbackControl::new(None);
    let game = game_witness(&callback, AudioPolicy::ResetOnRebuild);
    game.enable_development_output().unwrap();
    drop(callback.callback.take());
    assert!(game.enable_development_output().is_err());
    assert!(!game.snapshot().output_enabled);
    assert!(game.snapshot().data_callback_released);
}

#[test]
fn development_scene_retirement_still_cancels_an_original_started_voice() {
    let mut callback = CallbackControl::new(None);
    let game = game_witness(&callback, AudioPolicy::ResetOnRebuild);
    publish_game(&game, 1, ClipKind::Travel);
    assert_eq!(callback.output(1), [0.0; 2]);
    game.enable_development_output().unwrap();
    publish_game(&game, 1, ClipKind::Pickup);
    assert_eq!(callback.output(1), [0.5, 0.5]);
    callback.control.set_scope(scope(2)).unwrap();
    assert_eq!(callback.output(2), [0.0; 4]);
    let snapshot = game.snapshot();
    assert_eq!(snapshot.cancelled, 1);
    assert!(snapshot.records.iter().any(|record| matches!(
        record,
        AudioRecord::Cue {
            instance: 1,
            sequence: 1,
            phase: AudioPhase::Cancelled { started: true },
            ..
        }
    )));
    publish_game(&game, 2, ClipKind::Travel);
    assert_eq!(callback.output(3), [-0.25, -0.25, 0.75, 0.75, 0.0, 0.0]);
    assert_eq!(game.snapshot().accepted, 2);
    assert_eq!(game.snapshot().completed, 1);
    assert_eq!(game.snapshot().suppressed_effects, 1);
}

#[test]
fn asynchronous_retirement_waits_for_checked_admission_and_refuses_its_last_queued_cue() {
    let mut original = CallbackControl::new(None);
    original.control.set_scope(scope(1)).unwrap();
    original.output(1);
    original.witness.drain_records();
    let producer = original.control.commands.clone();
    let mut checked_admission = producer.lock().unwrap();
    assert!(original.control.shared.refusal().is_none());
    assert!(!checked_admission.is_abandoned());
    let callback = original.callback.take().unwrap();
    let worker = std::thread::spawn(move || drop(callback));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !original.control.shared.quiescing.load(Ordering::Acquire) {
        if Instant::now() >= deadline {
            drop(checked_admission);
            worker.join().unwrap();
            panic!("callback retirement did not reach the admission boundary");
        }
        std::thread::yield_now();
    }
    original
        .control
        .shared
        .cue_record_slots
        .fetch_sub(3, Ordering::AcqRel);
    checked_admission.push(cue(1, 1)).unwrap();
    assert!(!original.witness.snapshot().data_callback_released);
    drop(checked_admission);
    worker.join().unwrap();
    assert_eq!(
        cue_records(&original.witness),
        vec![(
            cue(1, 1),
            1,
            CueRecordKind::Refused(SubmissionRefusal::Closed)
        ),]
    );
    assert_eq!(
        original
            .control
            .shared
            .cue_record_slots
            .load(Ordering::Acquire),
        CUE_RECORD_CAPACITY
    );
    assert!(original.witness.snapshot().data_callback_released);
    assert_eq!(
        original.control.submit(cue(1, 2)),
        Err(SubmissionRefusal::Closed)
    );
}

#[test]
fn control_and_witness_are_send_and_sync_without_the_native_owner() {
    fn require_send_sync<T: Send + Sync>() {}
    require_send_sync::<AudioController>();
    require_send_sync::<AudioWitness>();
    require_send_sync::<DeviceSettings>();
}

#[test]
fn zero_initial_instance_is_silent_on_an_independent_callback_clock() {
    let mut callback = CallbackControl::new(None);
    let initial = callback.witness.snapshot();
    assert_eq!(initial.desired_instance, 0);
    assert_eq!(initial.callback_count, 0);
    assert_eq!(callback.output(3), [0.0; 6]);
    let snapshot = callback.witness.snapshot();
    assert_eq!(snapshot.callback_count, 1);
    assert_eq!(snapshot.output_frame, 3);
    assert_eq!(snapshot.counters, Counters::default());
    assert_eq!(snapshot.faults, 0);
    assert!(callback.witness.drain_records().is_empty());
    assert!(callback.witness.drain_capture().is_empty());
}

#[test]
fn queue_admission_is_distinct_from_start_and_completion() {
    let mut callback = CallbackControl::new(None);
    callback.control.set_scope(scope(1)).unwrap();
    callback.control.submit(cue(1, 1)).unwrap();
    assert_eq!(callback.witness.snapshot().counters, Counters::default());
    assert_eq!(callback.output(1), [0.5, 0.5]);
    assert_eq!(callback.output(2), [0.25, 0.25, -0.5, -0.5]);
    assert_eq!(
        cue_records(&callback.witness),
        [
            (cue(1, 1), 0, CueRecordKind::Accepted),
            (cue(1, 1), 0, CueRecordKind::Started),
            (cue(1, 1), 3, CueRecordKind::Completed),
        ]
    );
    let snapshot = callback.witness.snapshot();
    assert_eq!(
        snapshot.counters,
        Counters {
            accepted: 1,
            started: 1,
            completed: 1,
            cancelled: 0
        }
    );
    assert_eq!(snapshot.output_frame, 3);
    assert_eq!(snapshot.faults, 0);
}

#[test]
fn full_command_queue_cannot_block_the_authoritative_scope_barrier() {
    let mut callback = CallbackControl::new(None);
    callback.control.set_scope(scope(1)).unwrap();
    for sequence in 1..=COMMAND_CAPACITY as u64 {
        callback.control.submit(cue(1, sequence)).unwrap();
    }
    assert_eq!(
        callback.control.submit(cue(1, 65)),
        Err(SubmissionRefusal::QueueFull)
    );
    callback.control.set_scope(scope(2)).unwrap();
    assert_eq!(callback.output(1), [0.0; 2]);
    let records = callback.witness.drain_records();
    assert!(records.contains(&DeviceRecord::ScopeApplied {
        scope: scope(2),
        frame: 0
    }));
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(
                record,
                DeviceRecord::Cue {
                    kind: CueRecordKind::Refused(SubmissionRefusal::Mixer(Refusal::StaleScope)),
                    ..
                }
            ))
            .count(),
        COMMAND_CAPACITY
    );
    let snapshot = callback.witness.snapshot();
    assert_eq!(snapshot.applied_instance, 2);
    assert_eq!(snapshot.counters.accepted, 0);
    assert_eq!(snapshot.faults, 0);
    assert_eq!(
        callback
            .control
            .shared
            .cue_record_slots
            .load(Ordering::Acquire),
        CUE_RECORD_CAPACITY
    );
}

#[test]
fn replacing_scope_cuts_the_old_voice_at_the_next_output_frame() {
    let mut callback = CallbackControl::new(None);
    callback.control.set_scope(scope(1)).unwrap();
    callback.control.submit(cue(1, 1)).unwrap();
    assert_eq!(callback.output(1), [0.5, 0.5]);
    callback.control.set_scope(scope(2)).unwrap();
    callback.control.submit(cue(2, 1)).unwrap();
    assert_eq!(callback.output(1), [0.5, 0.5]);
    let records = callback.witness.drain_records();
    assert!(records.contains(&DeviceRecord::Cue {
        cue: cue(1, 1),
        frame: 1,
        kind: CueRecordKind::Cancelled { started: true },
    }));
    assert!(records.contains(&DeviceRecord::ScopeApplied {
        scope: scope(2),
        frame: 1
    }));
    assert!(records.contains(&DeviceRecord::Cue {
        cue: cue(2, 1),
        frame: 1,
        kind: CueRecordKind::Started
    }));
    assert_eq!(callback.witness.snapshot().faults, 0);
}

#[test]
fn pending_terminal_reservations_survive_an_exhausted_observation_budget() {
    let mut callback = CallbackControl::new(None);
    callback
        .control
        .shared
        .cue_record_slots
        .store(6, Ordering::Release);
    callback.control.set_scope(scope(1)).unwrap();
    callback.control.submit(cue(1, 1)).unwrap();
    callback.control.submit(cue(1, 2)).unwrap();
    assert_eq!(callback.output(1), [1.0, 1.0]);
    assert_eq!(
        callback.control.submit(cue(1, 3)),
        Err(SubmissionRefusal::RecordCapacity)
    );
    callback.control.set_scope(scope(2)).unwrap();
    assert_eq!(callback.output(1), [0.0, 0.0]);
    let records = cue_records(&callback.witness);
    for sequence in 1..=2 {
        assert!(records.contains(&(
            cue(1, sequence),
            1,
            CueRecordKind::Cancelled { started: true }
        )));
    }
    assert_eq!(
        callback
            .control
            .shared
            .cue_record_slots
            .load(Ordering::Acquire),
        6
    );
    assert_eq!(callback.witness.snapshot().faults, 0);
}

#[test]
fn cancellation_before_start_releases_only_the_unused_start_reservation() {
    let mut callback = CallbackControl::new(None);
    callback.control.set_scope(scope(1)).unwrap();
    callback.control.submit(cue(1, 1)).unwrap();
    assert!(callback.output(0).is_empty());
    callback.control.set_scope(scope(2)).unwrap();
    callback.output(1);
    assert_eq!(
        cue_records(&callback.witness),
        [
            (cue(1, 1), 0, CueRecordKind::Accepted),
            (cue(1, 1), 0, CueRecordKind::Cancelled { started: false }),
        ]
    );
    assert_eq!(
        callback
            .control
            .shared
            .cue_record_slots
            .load(Ordering::Acquire),
        CUE_RECORD_CAPACITY
    );
    assert_eq!(callback.witness.snapshot().faults, 0);
}

#[test]
fn invalid_and_duplicate_cues_restore_unused_terminal_record_reservations() {
    let mut callback = CallbackControl::new(None);
    callback.control.set_scope(scope(1)).unwrap();
    callback.control.submit(cue(1, 0)).unwrap();
    callback.control.submit(cue(1, 1)).unwrap();
    callback.control.submit(cue(1, 1)).unwrap();
    callback.output(3);
    let records = cue_records(&callback.witness);
    assert!(records.contains(&(
        cue(1, 0),
        0,
        CueRecordKind::Refused(SubmissionRefusal::Mixer(Refusal::InvalidSequence))
    )));
    assert!(records.contains(&(cue(1, 1), 0, CueRecordKind::Duplicate)));
    assert_eq!(
        records
            .iter()
            .filter(|(_, _, kind)| *kind == CueRecordKind::Started)
            .count(),
        1
    );
    assert_eq!(
        callback
            .control
            .shared
            .cue_record_slots
            .load(Ordering::Acquire),
        CUE_RECORD_CAPACITY
    );
    assert_eq!(callback.witness.snapshot().faults, 0);
}

#[test]
fn captured_chunks_preserve_literal_samples_and_exact_stereo_positions() {
    let mut callback = CallbackControl::new(Some(4));
    callback.control.set_scope(scope(1)).unwrap();
    callback.control.submit(cue(1, 1)).unwrap();
    let output = callback.output(300);
    let chunks = callback.witness.drain_capture();
    assert_eq!(chunks.len(), 2);
    assert_eq!(
        (
            chunks[0].first_frame,
            chunks[0].first_channel,
            chunks[0].channels,
            chunks[0].len
        ),
        (0, 0, 2, 512)
    );
    assert_eq!(
        (
            chunks[1].first_frame,
            chunks[1].first_channel,
            chunks[1].channels,
            chunks[1].len
        ),
        (256, 0, 2, 88)
    );
    let captured: Vec<_> = chunks
        .iter()
        .flat_map(|chunk| chunk.samples[..chunk.len].iter().copied())
        .collect();
    assert_eq!(captured, output);
    assert_eq!(&captured[..6], [0.5, 0.5, 0.25, 0.25, -0.5, -0.5]);
    assert!(captured[6..].iter().all(|sample| *sample == 0.0));
    assert_eq!(callback.witness.snapshot().faults, 0);
}

#[test]
fn capture_overflow_is_a_fault_and_closes_further_admission() {
    let mut callback = CallbackControl::new(Some(1));
    callback.control.set_scope(scope(1)).unwrap();
    callback.control.submit(cue(1, 1)).unwrap();
    assert_eq!(callback.output(512), [0.0; 1024]);
    let snapshot = callback.witness.snapshot();
    assert_ne!(snapshot.faults & DeviceFault::CaptureOverflow.mask(), 0);
    assert!(!snapshot.admission_open);
    assert_eq!(
        callback.control.submit(cue(1, 2)),
        Err(SubmissionRefusal::Faulted)
    );
    assert_eq!(callback.witness.drain_capture().len(), 1);
    assert!(cue_records(&callback.witness).contains(&(
        cue(1, 1),
        0,
        CueRecordKind::Refused(SubmissionRefusal::Faulted),
    )));
}

#[test]
fn scope_record_overflow_is_explicit_and_still_cancels_the_voice() {
    let mut callback = CallbackControl::new(None);
    for instance in 1..=SCOPE_RECORD_CAPACITY as u64 {
        callback.control.set_scope(scope(instance)).unwrap();
        callback.output(0);
    }
    let previous = SCOPE_RECORD_CAPACITY as u64;
    callback.control.submit(cue(previous, 1)).unwrap();
    callback.output(1);
    callback.control.set_scope(scope(previous + 1)).unwrap();
    assert_eq!(callback.output(1), [0.0; 2]);
    let snapshot = callback.witness.snapshot();
    assert_eq!(snapshot.applied_instance, previous + 1);
    assert_eq!(snapshot.counters.cancelled, 1);
    assert_ne!(snapshot.faults & DeviceFault::RecordOverflow.mask(), 0);
    assert!(cue_records(&callback.witness).contains(&(
        cue(previous, 1),
        1,
        CueRecordKind::Cancelled { started: true },
    )));
}

#[test]
fn asynchronous_error_closes_admission_without_releasing_callback_owners() {
    let callback = CallbackControl::new(None);
    callback.control.set_scope(scope(1)).unwrap();
    callback
        .error_release
        .as_ref()
        .unwrap()
        .native_error(&cpal::Error::new(cpal::ErrorKind::StreamInvalidated));
    assert_eq!(
        callback.control.submit(cue(1, 1)),
        Err(SubmissionRefusal::Faulted)
    );
    let snapshot = callback.witness.snapshot();
    assert_ne!(
        snapshot.faults & DeviceFault::NativeStreamInvalidated.mask(),
        0
    );
    assert!(!snapshot.data_callback_released);
    assert!(!snapshot.error_callback_released);
    assert!(!snapshot.stream_drop_returned);
    assert!(!snapshot.physical_complete);
}

#[test]
fn callback_capture_retirement_keeps_exact_pending_and_active_terminal_records() {
    let mut callback = CallbackControl::new(None);
    callback.control.set_scope(scope(1)).unwrap();
    callback.control.submit(cue(1, 1)).unwrap();
    callback.output(1);
    callback.control.submit(cue(1, 2)).unwrap();
    callback
        .control
        .shared
        .admission_open
        .store(false, Ordering::Release);
    callback
        .control
        .shared
        .quiescing
        .store(true, Ordering::Release);
    drop(callback.callback.take());
    let records = cue_records(&callback.witness);
    assert!(records.contains(&(cue(1, 1), 1, CueRecordKind::Cancelled { started: true })));
    assert!(records.contains(&(
        cue(1, 2),
        1,
        CueRecordKind::Refused(SubmissionRefusal::Closed)
    )));
    assert_eq!(
        callback.control.submit(cue(1, 3)),
        Err(SubmissionRefusal::Closed)
    );
    assert!(callback.witness.snapshot().data_callback_released);
    assert!(!callback.witness.snapshot().error_callback_released);
    drop(callback.error_release.take());
    assert!(callback.witness.snapshot().error_callback_released);
    assert!(!callback.witness.snapshot().stream_drop_returned);
    assert!(!callback.witness.snapshot().stream_created);
}

#[test]
fn witness_can_outlive_both_callback_captures_without_claiming_a_native_join() {
    let callback = CallbackControl::new(None);
    let witness = callback.witness.clone();
    drop(callback);
    let snapshot = witness.snapshot();
    assert!(snapshot.data_callback_released);
    assert!(snapshot.error_callback_released);
    assert!(!snapshot.stream_created);
    assert!(!snapshot.stream_drop_returned);
    assert!(snapshot.physical_complete);
}

#[test]
fn native_gate_rejects_unbounded_configuration_and_reports_a_bounded_timeout() {
    assert!(matches!(
        NativeGate::new(Duration::ZERO),
        Err(GateError::InvalidTimeout)
    ));
    assert!(matches!(
        NativeGate::new(Duration::from_secs(11)),
        Err(GateError::InvalidTimeout)
    ));
    let gate = NativeGate::new(Duration::from_millis(1)).unwrap();
    assert!(!gate.entered());
    gate.arm().unwrap();
    assert_eq!(gate.arm(), Err(GateError::Unavailable));
    assert!(!gate.wait_in_callback());
    assert!(gate.timed_out());
    assert!(!gate.release());
}

#[test]
fn qualification_gate_waits_for_external_release_in_the_data_handler() {
    let gate = NativeGate::new(Duration::from_secs(10)).unwrap();
    gate.arm().unwrap();
    let mut data_handler = CallbackControl::new(None);
    data_handler.callback.as_mut().unwrap().gate = Some(gate.clone());
    let callback = std::thread::spawn(move || data_handler.output(1));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !gate.entered() && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(gate.entered());
    assert!(!callback.is_finished());
    assert!(gate.release());
    assert_eq!(callback.join().unwrap(), [0.0; 2]);
    assert!(!gate.timed_out());
}

#[test]
fn failed_scope_controls_do_not_apply_a_scope_or_change_native_retirement_facts() {
    let mut callback = CallbackControl::new(None);
    callback.control.set_scope(scope(2)).unwrap();
    callback.control.submit(cue(2, 1)).unwrap();
    assert_eq!(callback.output(1), [0.5, 0.5]);
    callback.witness.drain_records();
    let before = callback.witness.snapshot();
    assert_eq!(
        callback.control.set_scope(scope(1)),
        Err(SubmissionRefusal::Mixer(Refusal::StaleScope))
    );
    assert_eq!(
        callback.control.set_scope(scope(0)),
        Err(SubmissionRefusal::Mixer(Refusal::InvalidScope))
    );
    assert_eq!(
        callback.control.set_scope(Scope {
            runtime: 8,
            instance: 3
        }),
        Err(SubmissionRefusal::Mixer(Refusal::ForeignRuntime))
    );
    assert_eq!(callback.witness.snapshot(), before);
    assert_eq!(callback.output(1), [0.25, 0.25]);
    assert!(callback.witness.drain_records().is_empty());
    assert!(!callback.witness.snapshot().stream_drop_returned);
}

#[test]
fn refusal_before_native_construction_completes_without_inventing_a_stream_drop() {
    let clips = PreparedClips::new(Arc::from([0.0]), Arc::from([0.0])).unwrap();
    let error = AudioDevice::open(0, clips, DeviceSettings::default()).unwrap_err();
    assert_eq!(error.stage, "runtime");
    assert!(error.retirement.physical_complete);
    assert!(!error.retirement.stream_created);
    assert!(!error.retirement.stream_drop_returned);
    assert!(error.retirement.data_callback_released);
    assert!(error.retirement.error_callback_released);
    assert!(error.witness.is_none());
    assert!(error.retained_device.is_none());
}

#[test]
fn failed_construction_retains_observation_without_inventing_an_original_native_owner() {
    let callback = CallbackControl::new(None);
    callback
        .control
        .shared
        .fault(DeviceFault::NativeBuildRetirementUnknown);
    let error = AudioDevice::opening_error(
        &callback.control.shared,
        "build",
        "construction did not return its native owner".into(),
        Some(callback.witness.clone()),
        None,
    );
    assert!(!error.retirement.physical_complete);
    assert!(error.retained_device.is_none());
    let witness = error.witness.as_ref().unwrap();
    assert!(!witness.snapshot().data_callback_released);
    assert!(!witness.snapshot().error_callback_released);
    drop(callback);
    let snapshot = witness.snapshot();
    assert!(snapshot.data_callback_released);
    assert!(snapshot.error_callback_released);
    assert!(!snapshot.stream_drop_returned);
    assert!(!snapshot.physical_complete);
    assert_ne!(
        snapshot.faults & DeviceFault::NativeBuildRetirementUnknown.mask(),
        0
    );
}
