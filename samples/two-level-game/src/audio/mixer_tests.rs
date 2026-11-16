use std::sync::Arc;

use super::*;

const SCOPE: Scope = Scope {
    runtime: 7,
    instance: 1,
};

fn ready(pickup: &[f32], travel: &[f32]) -> Mixer {
    let clips = PreparedClips::new(Arc::from(pickup), Arc::from(travel)).unwrap();
    let mut mixer = Mixer::new(clips);
    mixer.set_scope(SCOPE).unwrap();
    mixer
}

fn cue(sequence: u64, clip: ClipKind) -> Cue {
    Cue {
        id: EffectId {
            scope: SCOPE,
            sequence,
        },
        clip,
    }
}

#[test]
fn opposite_sign_voices_are_summed_before_the_single_final_clip() {
    let mut mixer = ready(&[0.75], &[-0.75]);
    mixer.publish(cue(1, ClipKind::Pickup)).unwrap();
    mixer.publish(cue(2, ClipKind::Pickup)).unwrap();
    mixer.publish(cue(3, ClipKind::Travel)).unwrap();
    let mut output = [0.0; 2];
    mixer.render(&mut output).unwrap();
    assert_eq!(output, [0.75, 0.75]);
}

#[test]
fn explicit_quiescence_cancels_once_without_forgetting_effect_identity() {
    let mut mixer = ready(&[0.25; 4], &[0.5; 4]);
    let active = cue(1, ClipKind::Pickup);
    let pending = cue(2, ClipKind::Travel);
    mixer.publish(active).unwrap();
    mixer.render(&mut [0.0; 2]).unwrap();
    drain(&mut mixer);
    mixer.publish(pending).unwrap();
    mixer.cancel_all();
    assert_eq!(
        drain(&mut mixer),
        vec![
            Event {
                cue: active,
                frame: 1,
                kind: EventKind::Cancelled { started: true }
            },
            Event {
                cue: pending,
                frame: 1,
                kind: EventKind::Cancelled { started: false }
            },
        ]
    );
    mixer.cancel_all();
    assert!(drain(&mut mixer).is_empty());
    assert_eq!(mixer.counters().cancelled, 2);
    assert_eq!(mixer.publish(active), Ok(Publication::Duplicate));
    assert_eq!(mixer.publish(pending), Ok(Publication::Duplicate));
    let mut output = [99.0; 8];
    mixer.render(&mut output).unwrap();
    assert_eq!(output, [0.0; 8]);
}

fn drain(mixer: &mut Mixer) -> Vec<Event> {
    std::iter::from_fn(|| mixer.pop_event()).collect()
}

#[derive(Debug, PartialEq)]
struct State {
    scope: Option<Scope>,
    last_sequence: u64,
    effects: Box<[Option<Cue>]>,
    effect_count: usize,
    voices: [Option<Voice>; VOICE_CAPACITY],
    events: [Option<Event>; EVENT_CAPACITY],
    event_head: usize,
    event_count: usize,
    reserved_events: usize,
    output_frame: u64,
    counters: Counters,
}

fn state(mixer: &Mixer) -> State {
    State {
        scope: mixer.scope,
        last_sequence: mixer.last_sequence,
        effects: mixer.effects.clone(),
        effect_count: mixer.effect_count,
        voices: mixer.voices,
        events: mixer.events,
        event_head: mixer.event_head,
        event_count: mixer.event_count,
        reserved_events: mixer.reserved_events,
        output_frame: mixer.output_frame,
        counters: mixer.counters,
    }
}

#[test]
fn known_mono_mix_has_exact_stereo_samples_ends_and_before_after_counters() {
    let mut mixer = ready(&[0.25, -0.25, 0.5], &[0.5, 0.25]);
    let pickup = cue(1, ClipKind::Pickup);
    let travel = cue(2, ClipKind::Travel);
    assert_eq!(mixer.publish(pickup), Ok(Publication::Accepted));
    assert_eq!(mixer.publish(travel), Ok(Publication::Accepted));
    let mut output = [99.0; 10];
    let report = mixer.render(&mut output).unwrap();
    assert_eq!(output, [0.75, 0.75, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(report.first_frame, 0);
    assert_eq!(report.end_frame, 5);
    assert_eq!(
        report.before,
        Counters {
            accepted: 2,
            ..Counters::default()
        }
    );
    assert_eq!(
        report.after,
        Counters {
            accepted: 2,
            started: 2,
            completed: 2,
            cancelled: 0
        }
    );
    assert_eq!(mixer.output_frame(), 5);
    assert_eq!(mixer.counters(), report.after);
    assert_eq!(
        drain(&mut mixer),
        vec![
            Event {
                cue: pickup,
                frame: 0,
                kind: EventKind::Started
            },
            Event {
                cue: travel,
                frame: 0,
                kind: EventKind::Started
            },
            Event {
                cue: travel,
                frame: 2,
                kind: EventKind::Completed
            },
            Event {
                cue: pickup,
                frame: 3,
                kind: EventKind::Completed
            },
        ]
    );
}

#[test]
fn output_hard_clips_the_sum_once_and_simultaneous_starts_precede_completions() {
    let mut mixer = ready(&[0.75, -0.75], &[0.5, -0.5]);
    mixer.publish(cue(1, ClipKind::Pickup)).unwrap();
    mixer.publish(cue(2, ClipKind::Travel)).unwrap();
    let mut output = [0.0; 4];
    mixer.render(&mut output).unwrap();
    assert_eq!(output, [1.0, 1.0, -1.0, -1.0]);

    let mut mixer = ready(&[0.25], &[0.5]);
    mixer.publish(cue(1, ClipKind::Pickup)).unwrap();
    mixer.publish(cue(2, ClipKind::Travel)).unwrap();
    mixer.render(&mut [0.0; 2]).unwrap();
    let events = drain(&mut mixer);
    assert_eq!(
        events.iter().map(|event| event.frame).collect::<Vec<_>>(),
        [0, 0, 1, 1]
    );
    assert_eq!(
        events.iter().map(|event| event.kind).collect::<Vec<_>>(),
        [
            EventKind::Started,
            EventKind::Started,
            EventKind::Completed,
            EventKind::Completed,
        ]
    );
}

#[test]
fn callback_partitioning_preserves_every_sample_event_and_absolute_frame() {
    let run = |parts: &[usize]| {
        let mut mixer = ready(&[0.25, -0.125, 0.5, -0.25, 0.125], &[0.5, 0.25]);
        mixer.render(&mut [0.0; 6]).unwrap();
        mixer.publish(cue(1, ClipKind::Pickup)).unwrap();
        mixer.publish(cue(2, ClipKind::Travel)).unwrap();
        let mut output = Vec::new();
        let mut events = Vec::new();
        for &frames in parts {
            let mut part = vec![42.0; frames * 2];
            let before = mixer.counters();
            let first_frame = mixer.output_frame();
            let report = mixer.render(&mut part).unwrap();
            assert_eq!(report.before, before);
            assert_eq!(report.first_frame, first_frame);
            assert_eq!(report.end_frame, first_frame + frames as u64);
            assert_eq!(report.after, mixer.counters());
            output.extend(part);
            events.extend(drain(&mut mixer));
        }
        (output, events, mixer.counters(), mixer.output_frame())
    };
    let whole = run(&[10]);
    for parts in [
        &[1, 1, 1, 1, 1, 1, 1, 1, 1, 1][..],
        &[0, 1, 0, 1, 3, 0, 5],
        &[2, 8],
    ] {
        assert_eq!(run(parts), whole);
    }
    assert_eq!(
        whole.1.iter().map(|event| event.frame).collect::<Vec<_>>(),
        [3, 3, 5, 8]
    );
    assert_eq!(whole.3, 13);
}

#[test]
fn duplicates_and_clip_conflicts_keep_the_exact_original_identity_after_completion() {
    let mut mixer = ready(&[0.25], &[0.5]);
    let original = cue(1, ClipKind::Pickup);
    mixer.publish(original).unwrap();
    let before = state(&mixer);
    assert_eq!(mixer.publish(original), Ok(Publication::Duplicate));
    assert_eq!(state(&mixer), before);
    assert_eq!(
        mixer.publish(cue(1, ClipKind::Travel)),
        Err(Refusal::ConflictingCue {
            existing: ClipKind::Pickup
        })
    );
    assert_eq!(state(&mixer), before);
    mixer.render(&mut [0.0; 2]).unwrap();
    drain(&mut mixer);
    let completed = state(&mixer);
    assert_eq!(mixer.publish(original), Ok(Publication::Duplicate));
    assert_eq!(state(&mixer), completed);
    assert_eq!(
        mixer.publish(cue(1, ClipKind::Travel)),
        Err(Refusal::ConflictingCue {
            existing: ClipKind::Pickup
        })
    );
    assert_eq!(state(&mixer), completed);
}

#[test]
fn invalid_stale_and_foreign_publications_do_not_mutate_any_mixer_state() {
    let mut mixer = ready(&[0.25], &[0.5]);
    mixer.publish(cue(10, ClipKind::Pickup)).unwrap();
    let before = state(&mixer);
    for (scope, sequence, refusal) in [
        (
            Scope {
                runtime: 0,
                instance: 1,
            },
            11,
            Refusal::InvalidScope,
        ),
        (
            Scope {
                runtime: 7,
                instance: 0,
            },
            11,
            Refusal::InvalidScope,
        ),
        (SCOPE, 0, Refusal::InvalidSequence),
        (
            Scope {
                runtime: 8,
                instance: 1,
            },
            11,
            Refusal::ForeignRuntime,
        ),
        (
            Scope {
                instance: 2,
                ..SCOPE
            },
            11,
            Refusal::ForeignScope,
        ),
        (SCOPE, 9, Refusal::StaleSequence),
    ] {
        assert_eq!(
            mixer.publish(Cue {
                id: EffectId { scope, sequence },
                clip: ClipKind::Pickup
            }),
            Err(refusal)
        );
        assert_eq!(state(&mixer), before);
    }
    mixer
        .set_scope(Scope {
            instance: 2,
            ..SCOPE
        })
        .unwrap();
    let changed = state(&mixer);
    assert_eq!(
        mixer.publish(cue(11, ClipKind::Pickup)),
        Err(Refusal::StaleScope)
    );
    assert_eq!(state(&mixer), changed);

    let clips = PreparedClips::new(Arc::from([0.25]), Arc::from([0.5])).unwrap();
    let mut unset = Mixer::new(clips);
    let before = state(&unset);
    assert_eq!(
        unset.publish(cue(1, ClipKind::Pickup)),
        Err(Refusal::ScopeNotSet)
    );
    assert_eq!(state(&unset), before);
}

#[test]
fn voice_capacity_refuses_the_whole_operation_and_the_same_cue_can_be_retried() {
    let mut mixer = ready(&[0.25, 0.25], &[0.5]);
    for sequence in 1..=VOICE_CAPACITY as u64 {
        mixer.publish(cue(sequence, ClipKind::Pickup)).unwrap();
    }
    let before = state(&mixer);
    let refused = cue(VOICE_CAPACITY as u64 + 1, ClipKind::Travel);
    assert_eq!(mixer.publish(refused), Err(Refusal::VoiceCapacity));
    assert_eq!(state(&mixer), before);
    assert_eq!(
        mixer.publish(cue(1, ClipKind::Pickup)),
        Ok(Publication::Duplicate)
    );
    assert_eq!(state(&mixer), before);
    assert_eq!(
        mixer.publish(cue(1, ClipKind::Travel)),
        Err(Refusal::ConflictingCue {
            existing: ClipKind::Pickup
        })
    );
    assert_eq!(state(&mixer), before);
    mixer.render(&mut [0.0; 4]).unwrap();
    drain(&mut mixer);
    assert_eq!(mixer.publish(refused), Ok(Publication::Accepted));
}

#[test]
fn exact_ledger_exhaustion_never_forgets_an_id_or_resets_on_a_scope_change() {
    let mut mixer = ready(&[0.25], &[0.5]);
    for sequence in 1..=EFFECT_CAPACITY as u64 {
        mixer.publish(cue(sequence, ClipKind::Pickup)).unwrap();
        mixer.render(&mut [0.0; 2]).unwrap();
        drain(&mut mixer);
    }
    let before = state(&mixer);
    assert_eq!(
        mixer.publish(cue(EFFECT_CAPACITY as u64 + 1, ClipKind::Pickup)),
        Err(Refusal::EffectCapacity)
    );
    assert_eq!(
        mixer.publish(cue(1, ClipKind::Pickup)),
        Ok(Publication::Duplicate)
    );
    assert_eq!(
        mixer.publish(cue(1, ClipKind::Travel)),
        Err(Refusal::ConflictingCue {
            existing: ClipKind::Pickup
        })
    );
    assert_eq!(state(&mixer), before);
    let next = Scope {
        instance: 2,
        ..SCOPE
    };
    mixer.set_scope(next).unwrap();
    let before = state(&mixer);
    assert_eq!(
        mixer.publish(Cue {
            id: EffectId {
                scope: next,
                sequence: 1
            },
            clip: ClipKind::Pickup
        }),
        Err(Refusal::EffectCapacity)
    );
    assert_eq!(state(&mixer), before);
}

#[test]
fn observation_backpressure_refuses_new_cues_and_ring_wrap_keeps_all_terminal_facts() {
    let mut mixer = ready(&[0.25], &[0.5]);
    for sequence in 1..=EVENT_CAPACITY as u64 / 2 {
        mixer.publish(cue(sequence, ClipKind::Pickup)).unwrap();
        mixer.render(&mut [0.0; 2]).unwrap();
    }
    let next = cue(EVENT_CAPACITY as u64 / 2 + 1, ClipKind::Pickup);
    let full = state(&mixer);
    assert_eq!(mixer.publish(next), Err(Refusal::EventCapacity));
    assert_eq!(state(&mixer), full);
    assert_eq!(mixer.pop_event().unwrap().kind, EventKind::Started);
    let one_free = state(&mixer);
    assert_eq!(mixer.publish(next), Err(Refusal::EventCapacity));
    assert_eq!(state(&mixer), one_free);
    assert_eq!(mixer.pop_event().unwrap().kind, EventKind::Completed);
    assert_eq!(mixer.publish(next), Ok(Publication::Accepted));
    mixer.render(&mut [0.0; 2]).unwrap();
    let remaining = drain(&mut mixer);
    assert_eq!(remaining.len(), EVENT_CAPACITY);
    assert_eq!(
        remaining[EVENT_CAPACITY - 2],
        Event {
            cue: next,
            frame: 32,
            kind: EventKind::Started
        }
    );
    assert_eq!(
        remaining[EVENT_CAPACITY - 1],
        Event {
            cue: next,
            frame: 33,
            kind: EventKind::Completed
        }
    );
    assert_eq!(mixer.reserved_events, 0);
}

#[test]
fn reserved_terminal_capacity_survives_a_full_backlog_and_active_scope_cancellation() {
    let mut mixer = ready(&[0.25, 0.25], &[0.5]);
    let warmup = (EVENT_CAPACITY - 2 * VOICE_CAPACITY) / 2;
    for sequence in 1..=warmup as u64 {
        mixer.publish(cue(sequence, ClipKind::Pickup)).unwrap();
        mixer.render(&mut [0.0; 4]).unwrap();
    }
    for offset in 1..=VOICE_CAPACITY as u64 {
        mixer
            .publish(cue(warmup as u64 + offset, ClipKind::Pickup))
            .unwrap();
    }
    assert_eq!(mixer.event_count + mixer.reserved_events, EVENT_CAPACITY);
    mixer.render(&mut [0.0; 2]).unwrap();
    let cut = mixer.output_frame();
    mixer
        .set_scope(Scope {
            instance: 2,
            ..SCOPE
        })
        .unwrap();
    let events = drain(&mut mixer);
    assert_eq!(events.len(), EVENT_CAPACITY);
    let cancelled: Vec<_> = events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::Cancelled { .. }))
        .collect();
    assert_eq!(cancelled.len(), VOICE_CAPACITY);
    assert!(cancelled.iter().all(|event| event.frame == cut && event.kind == EventKind::Cancelled { started: true }));
    assert_eq!(mixer.reserved_events, 0);
    assert_eq!(mixer.counters().cancelled, VOICE_CAPACITY as u64);
}

#[test]
fn scope_cut_cancels_active_and_queued_voices_before_new_scope_output() {
    let mut mixer = ready(&[0.5, 0.5, 0.5], &[-0.25, -0.25]);
    let active = cue(1, ClipKind::Pickup);
    let queued = cue(2, ClipKind::Travel);
    mixer.publish(active).unwrap();
    let mut first = [0.0; 2];
    mixer.render(&mut first).unwrap();
    assert_eq!(first, [0.5, 0.5]);
    mixer.publish(queued).unwrap();
    let next = Scope {
        instance: 2,
        ..SCOPE
    };
    mixer.set_scope(next).unwrap();
    let fresh = Cue {
        id: EffectId {
            scope: next,
            sequence: 1,
        },
        clip: ClipKind::Travel,
    };
    mixer.publish(fresh).unwrap();
    let mut output = [42.0; 6];
    mixer.render(&mut output).unwrap();
    assert_eq!(output, [-0.25, -0.25, -0.25, -0.25, 0.0, 0.0]);
    assert_eq!(
        drain(&mut mixer),
        vec![
            Event {
                cue: active,
                frame: 0,
                kind: EventKind::Started
            },
            Event {
                cue: active,
                frame: 1,
                kind: EventKind::Cancelled { started: true }
            },
            Event {
                cue: queued,
                frame: 1,
                kind: EventKind::Cancelled { started: false }
            },
            Event {
                cue: fresh,
                frame: 1,
                kind: EventKind::Started
            },
            Event {
                cue: fresh,
                frame: 3,
                kind: EventKind::Completed
            },
        ]
    );
    assert_eq!(
        mixer.counters(),
        Counters {
            accepted: 3,
            started: 2,
            completed: 1,
            cancelled: 2
        }
    );
    assert_eq!(mixer.reserved_events, 0);
}

#[test]
fn unchanged_scope_is_idempotent_and_invalid_controls_cannot_cancel_a_voice() {
    let mut mixer = ready(&[0.5, 0.25], &[0.5]);
    mixer
        .set_scope(Scope {
            instance: 2,
            ..SCOPE
        })
        .unwrap();
    let scope = mixer.scope.unwrap();
    mixer
        .publish(Cue {
            id: EffectId {
                scope,
                sequence: 10,
            },
            clip: ClipKind::Pickup,
        })
        .unwrap();
    let before = state(&mixer);
    assert_eq!(mixer.set_scope(scope), Ok(()));
    assert_eq!(state(&mixer), before);
    for (scope, refusal) in [
        (
            Scope {
                runtime: 0,
                instance: 3,
            },
            Refusal::InvalidScope,
        ),
        (
            Scope {
                runtime: 7,
                instance: 0,
            },
            Refusal::InvalidScope,
        ),
        (
            Scope {
                runtime: 8,
                instance: 3,
            },
            Refusal::ForeignRuntime,
        ),
        (SCOPE, Refusal::StaleScope),
    ] {
        assert_eq!(mixer.set_scope(scope), Err(refusal));
        assert_eq!(state(&mixer), before);
    }
}

#[test]
fn silence_advances_only_the_output_clock_and_empty_buffers_do_not_start_voices() {
    let mut mixer = ready(&[0.25], &[0.5]);
    let mut silence = [42.0; 6];
    let report = mixer.render(&mut silence).unwrap();
    assert_eq!(silence, [0.0; 6]);
    assert_eq!(report.first_frame, 0);
    assert_eq!(report.end_frame, 3);
    assert_eq!(report.before, Counters::default());
    assert_eq!(report.after, Counters::default());
    mixer.publish(cue(1, ClipKind::Pickup)).unwrap();
    let before = state(&mixer);
    mixer.render(&mut []).unwrap();
    assert_eq!(state(&mixer), before);
    mixer.render(&mut [0.0; 2]).unwrap();
    let completed = mixer.counters();
    mixer.render(&mut silence).unwrap();
    assert_eq!(silence, [0.0; 6]);
    assert_eq!(mixer.counters(), completed);
    assert_eq!(mixer.output_frame(), 7);
}

#[test]
fn invalid_stereo_buffer_is_silenced_without_any_partial_operation() {
    let mut mixer = ready(&[0.5], &[0.25]);
    mixer.publish(cue(1, ClipKind::Pickup)).unwrap();
    let before = state(&mixer);
    let mut output = [42.0; 3];
    assert_eq!(mixer.render(&mut output), Err(Refusal::NonStereoOutput));
    assert_eq!(output, [0.0; 3]);
    assert_eq!(state(&mixer), before);
}

#[test]
fn output_cursor_overflow_refuses_atomically_and_the_last_representable_frame_finishes() {
    let mut mixer = ready(&[0.5], &[0.25, 0.25]);
    mixer.output_frame = u64::MAX - 1;
    let before = state(&mixer);
    assert_eq!(
        mixer.publish(cue(1, ClipKind::Travel)),
        Err(Refusal::FrameOverflow)
    );
    assert_eq!(state(&mixer), before);
    let pickup = cue(1, ClipKind::Pickup);
    mixer.publish(pickup).unwrap();
    let before = state(&mixer);
    let mut refused = [42.0; 4];
    assert_eq!(mixer.render(&mut refused), Err(Refusal::FrameOverflow));
    assert_eq!(refused, [0.0; 4]);
    assert_eq!(state(&mixer), before);
    let mut last = [0.0; 2];
    let report = mixer.render(&mut last).unwrap();
    assert_eq!(last, [0.5, 0.5]);
    assert_eq!(report.first_frame, u64::MAX - 1);
    assert_eq!(report.end_frame, u64::MAX);
    assert_eq!(
        drain(&mut mixer),
        vec![
            Event {
                cue: pickup,
                frame: u64::MAX - 1,
                kind: EventKind::Started
            },
            Event {
                cue: pickup,
                frame: u64::MAX,
                kind: EventKind::Completed
            },
        ]
    );
    let exhausted = state(&mixer);
    assert_eq!(
        mixer.publish(cue(2, ClipKind::Pickup)),
        Err(Refusal::FrameOverflow)
    );
    assert_eq!(state(&mixer), exhausted);
    assert_eq!(mixer.render(&mut last), Err(Refusal::FrameOverflow));
    assert_eq!(last, [0.0; 2]);
    assert_eq!(state(&mixer), exhausted);
    mixer.render(&mut []).unwrap();
    assert_eq!(state(&mixer), exhausted);
}

#[test]
fn sequence_exhaustion_is_explicit_while_exact_duplicates_remain_recognized() {
    let mut mixer = ready(&[0.5], &[0.25]);
    let final_cue = cue(u64::MAX, ClipKind::Pickup);
    mixer.publish(final_cue).unwrap();
    let before = state(&mixer);
    assert_eq!(
        mixer.publish(cue(1, ClipKind::Pickup)),
        Err(Refusal::SequenceOverflow)
    );
    assert_eq!(mixer.publish(final_cue), Ok(Publication::Duplicate));
    assert_eq!(
        mixer.publish(cue(u64::MAX, ClipKind::Travel)),
        Err(Refusal::ConflictingCue {
            existing: ClipKind::Pickup
        })
    );
    assert_eq!(state(&mixer), before);
    let next = Scope {
        instance: 2,
        ..SCOPE
    };
    mixer.set_scope(next).unwrap();
    assert_eq!(
        mixer.publish(Cue {
            id: EffectId {
                scope: next,
                sequence: 1
            },
            clip: ClipKind::Travel
        }),
        Ok(Publication::Accepted)
    );
}
