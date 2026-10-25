use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskReturnObserver, TaskReturnState,
    TaskSpawnRequest,
};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(5);
const HOLD: Duration = Duration::from_secs(30);

pub(super) struct HeldDrop {
    started: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}

impl Drop for HeldDrop {
    fn drop(&mut self) {
        let _ = self.started.send(());
        self.release
            .recv_timeout(HOLD)
            .expect("release the Compute task's late result");
    }
}

pub(super) struct HeldCompute {
    release_body: Option<mpsc::Sender<()>>,
    release_drop: Option<mpsc::Sender<()>>,
    dropping: mpsc::Receiver<()>,
    pub(super) handle: TaskHandle<HeldDrop>,
    pub(super) returned: TaskReturnObserver,
}

impl HeldCompute {
    pub(super) fn start(pools: &TaskPools, after_release: impl FnOnce() + Send + 'static) -> Self {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (dropping_tx, dropping_rx) = mpsc::channel();
        let (release_drop_tx, release_drop_rx) = mpsc::channel();
        let handle = pools
            .spawn(
                TaskPoolKind::Compute,
                TaskSpawnRequest::new(0, TaskDomainKey::new(1)),
                move |_| {
                    let _ = started_tx.send(());
                    release_rx.recv_timeout(HOLD).expect("release Compute body");
                    after_release();
                    HeldDrop {
                        started: dropping_tx,
                        release: release_drop_rx,
                    }
                },
            )
            .into_handle()
            .expect("the configured TaskPools must admit the held Compute task");
        let returned = handle.return_observer();
        let held = Self {
            release_body: Some(release_tx),
            release_drop: Some(release_drop_tx),
            dropping: dropping_rx,
            handle,
            returned,
        };
        // Arm both release gates before any worker-start assertion can unwind.
        started_rx.recv_timeout(WAIT).expect("Compute must start");
        held
    }

    pub(super) fn release_body_until_drop(&mut self) {
        if let Some(release) = self.release_body.take() {
            let _ = release.send(());
        }
        self.dropping
            .recv_timeout(WAIT)
            .expect("the cancelled Compute task must start disposing its late result");
    }

    pub(super) fn release(&mut self) {
        for release in [&mut self.release_body, &mut self.release_drop] {
            if let Some(release) = release.take() {
                let _ = release.send(());
            }
        }
    }

    pub(super) fn release_and_wait(&mut self, expected: TaskReturnState) {
        self.release();
        let deadline = Instant::now() + WAIT;
        while self.returned.state() == TaskReturnState::Pending {
            assert!(Instant::now() < deadline, "the Compute task did not return");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(self.returned.state(), expected);
        assert!(self.handle.is_finished());
    }
}

impl Drop for HeldCompute {
    fn drop(&mut self) {
        self.release();
    }
}
