use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use nara::{
    app::{RuntimeCandidateRetirementState, RuntimeInstance, StartupStage},
    ecs::{World, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs},
    prelude::App,
};

use super::{AudioOptions, AudioPlugin, AudioWitness, device::NativeGate, start_audio, witness};

fn isolated_audio(gate: Option<NativeGate>) -> AudioPlugin {
    AudioPlugin {
        options: AudioOptions::default(),
        native_gate: gate,
        test_clips: Some(super::PreparedClips::new(Arc::from([0.0]), Arc::from([0.0])).unwrap()),
    }
}

fn start(app: App) -> Result<RuntimeInstance, String> {
    match RuntimeInstance::start(app) {
        Ok(runtime) => Ok(runtime),
        Err(mut failure) => {
            let message = failure.to_string();
            let deadline = Instant::now() + Duration::from_secs(10);
            while failure.drive_retirement() != RuntimeCandidateRetirementState::Retired {
                if Instant::now() >= deadline {
                    std::mem::forget(failure);
                    return Err(format!(
                        "{message}; original startup cleanup remains incomplete"
                    ));
                }
                thread::yield_now();
            }
            Err(message)
        }
    }
}

fn finish(runtime: RuntimeInstance) -> Result<(), String> {
    match runtime.finish() {
        Ok(evidence) => {
            if evidence.has_terminal_failure() {
                Err(format!("{evidence:?}"))
            } else {
                Ok(())
            }
        }
        Err(mut retirement) => {
            let deadline = Instant::now() + Duration::from_secs(10);
            while retirement.drive_retirement() != RuntimeCandidateRetirementState::Retired {
                if Instant::now() >= deadline {
                    std::mem::forget(retirement);
                    return Err("original audio Runtime cleanup remains incomplete".into());
                }
                thread::yield_now();
            }
            if retirement.close_evidence().has_terminal_failure() {
                Err(format!("{:?}", retirement.close_evidence()))
            } else {
                Ok(())
            }
        }
    }
}

struct NativeRuntime {
    gate: NativeGate,
    close: mpsc::Sender<()>,
    worker: Option<thread::JoinHandle<Result<(), String>>>,
}

struct ManagedRuntime(Option<RuntimeInstance>);

impl Drop for ManagedRuntime {
    fn drop(&mut self) {
        if let Some(runtime) = self.0.take() {
            let result = finish(runtime);
            assert!(result.is_ok() || std::thread::panicking(), "{result:?}");
        }
    }
}

impl Drop for NativeRuntime {
    fn drop(&mut self) {
        self.gate.release();
        let _ = self.close.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn wait(mut condition: impl FnMut() -> bool, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "{label}");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
#[ignore = "requires an actual Windows stereo 48 kHz f32 output device; run serially"]
fn a_held_real_callback_prevents_managed_audio_retirement_and_a_fresh_runtime_recovers() {
    let gate = NativeGate::new(Duration::from_secs(8)).unwrap();
    let (ready, receive_ready) = mpsc::sync_channel::<Result<AudioWitness, String>>(1);
    let (close, receive_close) = mpsc::channel();
    let (done, receive_done) = mpsc::sync_channel(1);
    let callback_gate = gate.clone();
    let worker = thread::spawn(move || {
        let mut app = App::new();
        app.add_plugin(isolated_audio(Some(callback_gate)))
            .map_err(|error| error.to_string())?;
        let runtime = match start(app) {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = ready.send(Err(error.clone()));
                return Err(error);
            }
        };
        let observation = witness(runtime.world()).ok_or("audio witness was not published")?;
        ready
            .send(Ok(observation))
            .map_err(|error| error.to_string())?;
        let _ = receive_close.recv();
        let result = finish(runtime);
        let _ = done.send(result.clone());
        result
    });
    let mut original = NativeRuntime {
        gate,
        close,
        worker: Some(worker),
    };
    let observed = receive_ready
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    wait(
        || observed.snapshot().callbacks > 0,
        "the actual native callback never started",
    );
    let generation = observed.snapshot().runtime;
    original.gate.arm().unwrap();
    wait(
        || original.gate.entered(),
        "the actual native callback did not enter its held gate",
    );
    original.close.send(()).unwrap();
    wait(
        || !observed.snapshot().admission_open,
        "managed close did not quiesce original audio admission",
    );
    assert!(!observed.snapshot().stream_drop_returned);
    assert!(!observed.snapshot().physical_complete);
    assert!(matches!(
        receive_done.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    assert!(original.gate.release());
    receive_done
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    original.worker.take().unwrap().join().unwrap().unwrap();
    let closed = observed.snapshot();
    assert!(closed.physical_complete && closed.stream_drop_returned);
    assert!(closed.data_callback_released && closed.error_callback_released);
    assert!(!closed.admission_open && closed.error.is_none());
    assert_eq!(closed.faults, 0);
    assert!(!original.gate.timed_out());

    let mut app = App::new();
    app.add_plugin(isolated_audio(None)).unwrap();
    let mut runtime = ManagedRuntime(Some(start(app).unwrap()));
    let fresh = witness(runtime.0.as_ref().unwrap().world()).unwrap();
    wait(
        || fresh.snapshot().callbacks > 0,
        "fresh native callbacks never started",
    );
    assert_ne!(fresh.snapshot().runtime, generation);
    assert_eq!(fresh.snapshot().accepted, 0);
    finish(runtime.0.take().unwrap()).unwrap();
    assert!(fresh.snapshot().physical_complete);
    assert!(observed.snapshot().physical_complete);
}

#[test]
#[ignore = "requires an actual Windows stereo 48 kHz f32 output device; run serially"]
fn a_later_startup_failure_retires_the_original_opened_audio_device() {
    let captured = Arc::new(Mutex::new(None::<AudioWitness>));
    let observe = captured.clone();
    let mut app = App::new();
    app.add_plugin(isolated_audio(None)).unwrap();
    app.add_systems(
        StartupStage::Tooling,
        (move |world: &mut World| -> Result<(), BevyError> {
            *observe.lock().unwrap() = witness(world);
            Err(BevyError::error(std::io::Error::other(
                "intentional failure after original audio acquisition",
            )))
        })
        .after(start_audio),
    )
    .unwrap();
    let mut failure = RuntimeInstance::start(app).expect_err("the later startup system must fail");
    let observed = captured
        .lock()
        .unwrap()
        .clone()
        .expect("native audio opened before the injected failure");
    assert!(observed.snapshot().play_admitted);
    let deadline = Instant::now() + Duration::from_secs(10);
    while failure.drive_retirement() != RuntimeCandidateRetirementState::Retired {
        if Instant::now() >= deadline {
            std::mem::forget(failure);
            panic!("failed startup still owns incomplete native audio retirement");
        }
        thread::yield_now();
    }
    let closed = observed.snapshot();
    assert!(closed.physical_complete && closed.stream_drop_returned);
    assert!(closed.data_callback_released && closed.error_callback_released);
    assert!(!closed.admission_open && closed.error.is_none());
    assert_eq!(closed.faults, 0);
}

#[test]
#[ignore = "requires an actual Windows stereo 48 kHz f32 output device; run serially"]
fn a_native_wait_assertion_unwinds_only_after_its_original_runtime_is_physically_retired() {
    let mut app = App::new();
    app.add_plugin(isolated_audio(None)).unwrap();
    let runtime = start(app).unwrap();
    let observed = witness(runtime.world()).unwrap();
    let result = catch_unwind(AssertUnwindSafe(move || {
        let _original = ManagedRuntime(Some(runtime));
        panic!("intentional native callback wait assertion");
    }));
    assert!(result.is_err());
    let closed = observed.snapshot();
    assert!(closed.physical_complete && closed.stream_drop_returned);
    assert!(closed.data_callback_released && closed.error_callback_released);
    assert!(!closed.admission_open);
    assert_eq!(closed.faults, 0);
}
