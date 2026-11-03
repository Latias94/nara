use std::{sync::mpsc, time::Duration};

use motion_audio::AudioError;

use super::*;

fn input(tick: u64, delta: f32) -> Input {
    Input {
        tick,
        event: 50 + tick,
        delta: Vec2::new(delta, 0.0),
    }
}

fn run(session: &mut Session, input: Input, reverse: bool) -> Attempt {
    let attempt = session.begin(input).unwrap();
    let mut outcomes = [
        Outcome {
            attempt,
            key: 0,
            value: 3,
        },
        Outcome {
            attempt,
            key: 1,
            value: -1,
        },
    ];
    if reverse {
        outcomes.reverse();
    }
    for outcome in outcomes {
        session.admit(outcome).unwrap();
    }
    session.simulate(attempt).unwrap();
    attempt
}

#[test]
fn corrected_attempt_restores_complete_solver_entities_rng_commands_and_continues_in_same_generation()
 {
    for mode in [SolverMode::Direct, SolverMode::Borrowed] {
        let mut clean = Session::new(mode, None).unwrap();
        let mut revised = Session::new(mode, None).unwrap();
        let original_generation = revised.runtime.generation();
        run(&mut clean, input(1, 0.25), false);
        run(&mut revised, input(1, 0.25), true);
        let checkpoint = revised.checkpoint().unwrap();
        assert_eq!(checkpoint.digest(), clean.checkpoint().unwrap().digest());
        assert_eq!(checkpoint.particles.len(), 1);
        assert!(!checkpoint.model.pending.is_empty());
        run(&mut clean, input(2, 0.75), false);
        run(&mut revised, input(2, -2.0), true);
        let wrong = revised.checkpoint().unwrap();
        assert_ne!(wrong.digest(), clean.checkpoint().unwrap().digest());
        revised.restore(&checkpoint).unwrap();
        assert_eq!(revised.runtime.generation(), original_generation);
        assert_eq!(revised.checkpoint().unwrap().digest(), checkpoint.digest());
        run(&mut revised, input(2, 0.75), true);
        let corrected = revised.checkpoint().unwrap();
        let expected = clean.checkpoint().unwrap();
        assert_eq!(
            corrected.solver.encoded().unwrap(),
            expected.solver.encoded().unwrap()
        );
        assert_eq!(corrected.digest(), expected.digest());
        for tick in 3..=MAX_TICK {
            run(&mut clean, input(tick, 0.25), false);
            run(&mut revised, input(tick, 0.25), true);
            assert_eq!(
                revised.checkpoint().unwrap().digest(),
                clean.checkpoint().unwrap().digest(),
                "{mode:?}, continuation tick {tick}"
            );
        }
        let final_state = revised.checkpoint().unwrap();
        assert!(
            !final_state
                .particles
                .iter()
                .any(|particle| particle.logical == 100)
        );
        assert!(
            final_state
                .particles
                .iter()
                .any(|particle| particle.logical == 100 + MAX_TICK - 1)
        );
        assert_eq!(final_state.model.admitted.len(), MAX_TICK as usize * 2);
        assert_eq!(final_state.history.len(), MAX_TICK as usize + 1);
        assert_eq!(
            revised.begin(input(MAX_TICK + 1, 0.25)),
            Err(Refusal::InvalidInput)
        );
        assert_eq!(
            revised
                .runtime
                .world()
                .resource::<nara_app::FixedTime>()
                .tick(),
            0
        );
        clean.finish();
        revised.finish();
    }
}

#[test]
fn old_same_tick_worker_result_is_refused_after_replacement_before_any_state_or_effect_apply() {
    let mut session = Session::new(SolverMode::Borrowed, None).unwrap();
    let baseline = session.checkpoint().unwrap();
    let old = session.begin(input(1, -4.0)).unwrap();
    let (started_sender, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (sender, received) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        started_sender.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(5)).unwrap();
        sender
            .send(Outcome {
                attempt: old,
                key: 0,
                value: 10_000,
            })
            .unwrap();
    });
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    session.abandon(old).unwrap();
    let replacement = session.begin(input(1, 0.25)).unwrap();
    assert_eq!(old.tick, replacement.tick);
    assert_ne!(old.ordinal, replacement.ordinal);
    release.send(()).unwrap();
    let late = received.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.join().unwrap();
    assert_eq!(session.admit(late), Err(Refusal::StaleAttempt));
    assert!(session.outcomes.is_empty());
    assert_eq!(session.runtime.world().resource::<Model>().tick, 0);
    assert!(
        session
            .runtime
            .world()
            .resource::<Model>()
            .effects
            .is_empty()
    );
    assert_eq!(
        session
            .runtime
            .world()
            .resource::<Solver>()
            .checkpoint()
            .unwrap()
            .encoded()
            .unwrap(),
        baseline.solver.encoded().unwrap()
    );
    assert_eq!(session.simulate(replacement), Err(Refusal::MissingOutcome));
    session
        .admit(Outcome {
            attempt: replacement,
            key: 1,
            value: -1,
        })
        .unwrap();
    session
        .admit(Outcome {
            attempt: replacement,
            key: 0,
            value: 3,
        })
        .unwrap();
    session.simulate(replacement).unwrap();
    let mut clean = Session::new(SolverMode::Borrowed, None).unwrap();
    run(&mut clean, input(1, 0.25), false);
    assert_eq!(
        session.checkpoint().unwrap().digest(),
        clean.checkpoint().unwrap().digest()
    );
    assert!(
        session
            .audio
            .lock()
            .unwrap()
            .render(800)
            .unwrap()
            .iter()
            .all(|sample| *sample == 0.0)
    );
    clean.finish();
    session.finish();
}

#[test]
fn foreign_session_attempts_cannot_admit_simulate_abandon_or_confirm_matching_tick_and_ordinal() {
    let mut source = Session::new(SolverMode::Direct, None).unwrap();
    let mut target = Session::new(SolverMode::Direct, None).unwrap();
    let foreign = run(&mut source, input(1, 0.25), false);
    let before = target.checkpoint().unwrap();
    let own = target.begin(input(1, 0.25)).unwrap();
    assert_eq!(foreign.tick, own.tick);
    assert_eq!(foreign.ordinal, own.ordinal);
    assert_eq!(
        target.admit(Outcome {
            attempt: foreign,
            key: 0,
            value: 10_000,
        }),
        Err(Refusal::StaleAttempt)
    );
    assert_eq!(target.simulate(foreign), Err(Refusal::StaleAttempt));
    assert_eq!(target.abandon(foreign), Err(Refusal::StaleAttempt));
    assert!(target.outcomes.is_empty());
    assert_eq!(target.runtime.world().resource::<Model>().tick, 0);
    assert_eq!(
        target
            .runtime
            .world()
            .resource::<Solver>()
            .checkpoint()
            .unwrap()
            .encoded()
            .unwrap(),
        before.solver.encoded().unwrap()
    );
    for (key, value) in [(0, 3), (1, -1)] {
        target
            .admit(Outcome {
                attempt: own,
                key,
                value,
            })
            .unwrap();
    }
    target.simulate(own).unwrap();
    assert_eq!(
        target.checkpoint().unwrap().digest(),
        source.checkpoint().unwrap().digest()
    );
    assert_eq!(target.confirm(foreign), Err(Refusal::StaleAttempt));
    assert!(
        target
            .audio
            .lock()
            .unwrap()
            .render(800)
            .unwrap()
            .iter()
            .all(|sample| *sample == 0.0)
    );
    assert_eq!(target.confirm(own), Ok(1));
    source.finish();
    target.finish();
}

#[test]
fn future_checkpoint_from_discarded_branch_cannot_replace_a_confirmed_ancestor() {
    let mut session = Session::new(SolverMode::Borrowed, None).unwrap();
    let genesis = session.checkpoint().unwrap();
    run(&mut session, input(1, -3.0), false);
    run(&mut session, input(2, 0.25), false);
    let discarded_future = session.checkpoint().unwrap();
    session.restore(&genesis).unwrap();
    let corrected = run(&mut session, input(1, 0.25), false);
    assert_eq!(session.confirm(corrected), Ok(1));
    let confirmed = session.checkpoint().unwrap();
    let cursor = session.audio.lock().unwrap().cursor();
    assert_eq!(
        session.restore(&discarded_future),
        Err(Refusal::ConfirmedHistory)
    );
    assert_eq!(session.checkpoint().unwrap().digest(), confirmed.digest());
    assert_eq!(session.audio.lock().unwrap().cursor(), cursor);
    assert_eq!(session.confirm(corrected), Ok(0));
    run(&mut session, input(2, 0.25), false);
    let valid_future = session.checkpoint().unwrap();
    session.restore(&confirmed).unwrap();
    run(&mut session, input(2, -2.0), false);
    session.restore(&valid_future).unwrap();
    assert_eq!(
        session.checkpoint().unwrap().digest(),
        valid_future.digest()
    );
    session.finish();
}

#[test]
fn failed_audio_publication_does_not_confirm_speculative_history() {
    let mut session = Session::new(SolverMode::Direct, None).unwrap();
    let genesis = session.checkpoint().unwrap();
    let attempt = run(&mut session, input(1, 0.25), false);
    assert!(
        session
            .audio
            .lock()
            .unwrap()
            .render(801)
            .unwrap()
            .iter()
            .all(|sample| *sample == 0.0)
    );
    assert!(matches!(session.confirm(attempt), Err(Refusal::Audio(_))));
    session.restore(&genesis).unwrap();
    assert_eq!(session.checkpoint().unwrap().digest(), genesis.digest());
    assert_eq!(session.audio.lock().unwrap().cursor(), 801);
    session.finish();
}

#[test]
fn physical_frame_grouping_and_real_worker_completion_order_do_not_change_classified_state() {
    let mut fine = Session::new(SolverMode::Direct, None).unwrap();
    let mut grouped = Session::new(SolverMode::Borrowed, None).unwrap();
    for tick in 1..=8 {
        fine.runtime
            .drive(nara_app::FixedTime::DEFAULT_TIMESTEP)
            .unwrap();
        run(&mut fine, input(tick, 0.25), false);
        if tick % 2 == 0 {
            grouped
                .runtime
                .drive(nara_app::FixedTime::DEFAULT_TIMESTEP * 2)
                .unwrap();
            for grouped_tick in [tick - 1, tick] {
                let attempt = grouped.begin(input(grouped_tick, 0.25)).unwrap();
                let (sender, receiver) = mpsc::channel();
                let (release, released) = mpsc::channel();
                let first_sender = sender.clone();
                let first = std::thread::spawn(move || {
                    released.recv_timeout(Duration::from_secs(5)).unwrap();
                    first_sender
                        .send(Outcome {
                            attempt,
                            key: 0,
                            value: 3,
                        })
                        .unwrap();
                });
                let second = std::thread::spawn(move || {
                    sender
                        .send(Outcome {
                            attempt,
                            key: 1,
                            value: -1,
                        })
                        .unwrap()
                });
                let earlier = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
                assert_eq!(earlier.key, 1);
                grouped.admit(earlier).unwrap();
                release.send(()).unwrap();
                grouped
                    .admit(receiver.recv_timeout(Duration::from_secs(5)).unwrap())
                    .unwrap();
                first.join().unwrap();
                second.join().unwrap();
                grouped.simulate(attempt).unwrap();
            }
            assert_eq!(
                fine.checkpoint().unwrap().digest(),
                grouped.checkpoint().unwrap().digest()
            );
        }
    }
    assert_ne!(
        fine.runtime.world().resource::<nara_app::RealTime>().frame,
        grouped
            .runtime
            .world()
            .resource::<nara_app::RealTime>()
            .frame
    );
    fine.finish();
    grouped.finish();
}

#[test]
fn audio_confirmation_uses_semantic_identity_across_attempts_and_managed_close_stops_the_owner() {
    let mut session = Session::new(SolverMode::Borrowed, None).unwrap();
    let before = session.checkpoint().unwrap();
    let audio = Arc::clone(&session.audio);
    let first = run(&mut session, input(1, 0.25), false);
    assert!(
        audio
            .lock()
            .unwrap()
            .render(800)
            .unwrap()
            .iter()
            .all(|sample| *sample == 0.0)
    );
    assert_eq!(session.confirm(first), Ok(1));
    let first_pcm = audio.lock().unwrap().render(128).unwrap();
    assert!(first_pcm.iter().any(|sample| sample.abs() > 0.1));
    assert_eq!(session.confirm(first), Ok(0));
    let checkpoint = session.checkpoint().unwrap();
    let discarded = run(&mut session, input(2, -3.0), true);
    assert!(
        audio
            .lock()
            .unwrap()
            .render(100)
            .unwrap()
            .iter()
            .all(|sample| *sample == 0.0)
    );
    session.restore(&checkpoint).unwrap();
    let corrected = run(&mut session, input(2, 0.25), false);
    assert_eq!(discarded.tick, corrected.tick);
    assert_ne!(discarded.ordinal, corrected.ordinal);
    assert_eq!(session.confirm(discarded), Err(Refusal::StaleAttempt));
    assert_eq!(session.confirm(corrected), Ok(1));
    assert!(
        audio
            .lock()
            .unwrap()
            .render(572)
            .unwrap()
            .iter()
            .all(|sample| *sample == 0.0)
    );
    let second_pcm = audio.lock().unwrap().render(128).unwrap();
    assert!(second_pcm.iter().any(|sample| sample.abs() > 0.1));
    assert_eq!(session.confirm(corrected), Ok(0));
    assert_eq!(session.restore(&before), Err(Refusal::ConfirmedHistory));
    assert_eq!(audio.lock().unwrap().cursor(), 1728);
    session.finish();
    assert!(audio.lock().unwrap().is_closed());
    assert_eq!(audio.lock().unwrap().render(1), Err(AudioError::Closed));
    assert_eq!(audio.lock().unwrap().publish(&[]), Err(AudioError::Closed));
}

#[test]
fn admission_refuses_uncovered_state_unclassified_outcomes_and_foreign_checkpoints() {
    for unsupported in [
        Unsupported::SystemLocal,
        Unsupported::NativeOperation,
        Unsupported::ExternalIo,
    ] {
        let systems = (|| {}).into_configs();
        assert!(
            matches!(Session::new(SolverMode::Direct, Some((unsupported, systems))), Err(Refusal::Unsupported(actual)) if actual == unsupported)
        );
    }
    let mut session = Session::new(SolverMode::Borrowed, None).unwrap();
    let other = Session::new(SolverMode::Borrowed, None).unwrap();
    assert_eq!(
        session.restore(&other.checkpoint().unwrap()),
        Err(Refusal::ForeignCheckpoint)
    );
    let attempt = session.begin(input(1, 0.25)).unwrap();
    assert!(matches!(session.checkpoint(), Err(Refusal::NotCompleted)));
    assert_eq!(session.confirm(attempt), Err(Refusal::NotCompleted));
    assert_eq!(
        session.admit(Outcome {
            attempt,
            key: 2,
            value: 0
        }),
        Err(Refusal::UnclassifiedOutcome)
    );
    session
        .admit(Outcome {
            attempt,
            key: 0,
            value: 3,
        })
        .unwrap();
    assert_eq!(
        session.admit(Outcome {
            attempt,
            key: 0,
            value: 3
        }),
        Err(Refusal::DuplicateOutcome)
    );
    assert_eq!(session.simulate(attempt), Err(Refusal::MissingOutcome));
    assert!(
        session
            .runtime
            .world()
            .resource::<Model>()
            .effects
            .is_empty()
    );
    session.abandon(attempt).unwrap();
    session.finish();
    other.finish();
}

#[test]
fn pcm_batches_validate_atomically_and_keep_absolute_sample_phase_across_chunking() {
    let mut audio = PcmAudio::new(48_000).unwrap();
    let valid = Cue {
        id: SemanticEffectId {
            input_event: 77,
            occurrence: 1,
            cue: 1,
            sequence: 0,
        },
        at_sample: 0,
        sample_count: 128,
        frequency_hz: 440,
    };
    let invalid = Cue {
        id: SemanticEffectId {
            input_event: 78,
            ..valid.id
        },
        frequency_hz: 0,
        ..valid
    };
    assert_eq!(
        audio.publish(&[valid, invalid]),
        Err(AudioError::InvalidFrequency)
    );
    assert!(audio.render(1).unwrap().iter().all(|sample| *sample == 0.0));
    assert_eq!(
        audio.publish(&[valid]),
        Err(AudioError::LateCue { id: valid.id })
    );
    let valid = Cue {
        at_sample: 1,
        ..valid
    };
    assert_eq!(audio.publish(&[valid]), Ok(1));
    let conflicting = Cue {
        frequency_hz: 220,
        ..valid
    };
    assert_eq!(
        audio.publish(&[conflicting]),
        Err(AudioError::Conflict { id: valid.id })
    );
    let mut reference = PcmAudio::new(48_000).unwrap();
    reference.render(1).unwrap();
    reference.publish(&[valid]).unwrap();
    let expected = reference.render(128).unwrap();
    let mut chunked = audio.render(17).unwrap();
    chunked.extend(audio.render(111).unwrap());
    assert_eq!(chunked, expected);
    audio.shutdown();
    audio.shutdown();
    assert!(audio.is_closed());
}

#[test]
fn actual_native_write_and_system_local_contributions_refuse_before_registration_or_execution() {
    use nara_ecs::Local;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let path = std::env::temp_dir().join(format!(
        "nara-prediction-unmodeled-native-{}.txt",
        std::process::id()
    ));
    assert!(!path.exists());
    let destination = path.clone();
    let native =
        (move || std::fs::write(&destination, b"unmodeled native effect").unwrap()).into_configs();
    assert!(matches!(
        Session::new(
            SolverMode::Borrowed,
            Some((Unsupported::NativeOperation, native))
        ),
        Err(Refusal::Unsupported(Unsupported::NativeOperation))
    ));
    assert!(!path.exists());
    let executions = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&executions);
    let local = (move |mut hidden: Local<u64>| {
        *hidden += 1;
        observed.fetch_add(1, Ordering::AcqRel);
    })
    .into_configs();
    assert!(matches!(
        Session::new(SolverMode::Direct, Some((Unsupported::SystemLocal, local))),
        Err(Refusal::Unsupported(Unsupported::SystemLocal))
    ));
    assert_eq!(executions.load(Ordering::Acquire), 0);
}

#[test]
fn solver_checkpoint_refuses_external_worker_gate_state_without_changing_the_sample() {
    let mut solver = Solver::new(SolverMode::Borrowed);
    let snapshot = solver.checkpoint().unwrap();
    solver = solver.with_gate(motion_physics::WorkerGate::new(Duration::from_millis(1)));
    let before = solver.sample();
    assert!(solver.checkpoint().is_err());
    assert!(solver.restore(&snapshot).is_err());
    assert_eq!(solver.sample(), before);
}
