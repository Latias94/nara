use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use nara::fs::ContentDigest;
use nara::tooling::EditorPlayFailure;

use super::{
    build::VerifiedDesktopBuildCandidate,
    generation::{
        ExecutableGenerationStore, GenerationManifestInput, ImmutableExecutableGeneration,
    },
    store_root::generation_store_base_root,
    wake::SupervisedPlayWake,
};

pub(super) enum GenerationPreparationRequest {
    Build {
        candidate: VerifiedDesktopBuildCandidate,
        manifest: GenerationManifestInput,
    },
    LastGood {
        source_receipt: Vec<u8>,
    },
}

pub(super) struct GenerationPreparationOutcome {
    pub(super) store: Option<ExecutableGenerationStore>,
    pub(super) result: Result<ImmutableExecutableGeneration, EditorPlayFailure>,
}

struct PreparationState {
    store: Option<ExecutableGenerationStore>,
    request: Option<GenerationPreparationRequest>,
    result: Option<Result<ImmutableExecutableGeneration, EditorPlayFailure>>,
    #[cfg(test)]
    gate: Option<(
        std::sync::mpsc::SyncSender<()>,
        std::sync::mpsc::Receiver<()>,
    )>,
}

/// Owns one generation publication, including blocking filesystem calls and their cleanup state.
/// Only a finished worker may return its store to the UI thread.
pub(super) struct GenerationPreparationOwner {
    state: Arc<Mutex<PreparationState>>,
    worker: Option<JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
    last_good_source: Option<ContentDigest>,
    next_poll: Instant,
}

impl GenerationPreparationOwner {
    pub(super) fn start(
        store: Option<ExecutableGenerationStore>,
        request: GenerationPreparationRequest,
        wake: SupervisedPlayWake,
    ) -> Result<Self, Option<ExecutableGenerationStore>> {
        Self::dispatch(
            PreparationState {
                store,
                request: Some(request),
                result: None,
                #[cfg(test)]
                gate: None,
            },
            wake,
        )
    }

    fn dispatch(
        state: PreparationState,
        wake: SupervisedPlayWake,
    ) -> Result<Self, Option<ExecutableGenerationStore>> {
        let last_good_source = state
            .store
            .as_ref()
            .and_then(ExecutableGenerationStore::last_good)
            .map(|generation| ContentDigest::of_bytes(generation.source_receipt()));
        let state = Arc::new(Mutex::new(state));
        let worker_state = Arc::clone(&state);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let worker = thread::Builder::new()
            .name("nara-generation-preparation".to_owned())
            .spawn(move || {
                let mut state = worker_state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let result = catch_unwind(AssertUnwindSafe(|| {
                    #[cfg(test)]
                    if let Some((started, gate)) = state.gate.take() {
                        let _ = started.send(());
                        let _ = gate.recv();
                    }
                    prepare(&mut state, &worker_cancel)
                }))
                .unwrap_or(Err(EditorPlayFailure::Preparation));
                state.result = Some(result);
                drop(state);
                wake.notify();
            });
        match worker {
            Ok(worker) => Ok(Self {
                state,
                worker: Some(worker),
                cancel,
                last_good_source,
                next_poll: Instant::now() + Duration::from_millis(16),
            }),
            Err(_) => Err(state
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .store
                .take()),
        }
    }

    pub(super) fn request_cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub(super) fn last_good_matches(&self, source: ContentDigest) -> bool {
        self.last_good_source == Some(source)
    }

    #[cfg(test)]
    pub(super) fn held_for_test(
        store: ExecutableGenerationStore,
    ) -> (
        Self,
        std::sync::mpsc::Receiver<()>,
        std::sync::mpsc::SyncSender<()>,
    ) {
        let (started, wait_started) = std::sync::mpsc::sync_channel(1);
        let (release, gate) = std::sync::mpsc::sync_channel(1);
        let owner = Self::dispatch(
            PreparationState {
                store: Some(store),
                request: Some(GenerationPreparationRequest::LastGood {
                    source_receipt: b"held-publication".to_vec(),
                }),
                result: None,
                gate: Some((started, gate)),
            },
            SupervisedPlayWake::default(),
        )
        .unwrap_or_else(|_| panic!("the publication worker must start"));
        (owner, wait_started, release)
    }

    pub(super) const fn next_deadline(&self) -> Instant {
        // A wake can arrive just before the worker's final instructions complete.
        self.next_poll
    }

    pub(super) fn take_finished(&mut self) -> Option<GenerationPreparationOutcome> {
        if !self.worker.as_ref()?.is_finished() {
            self.next_poll = Instant::now() + Duration::from_millis(16);
            return None;
        }
        let worker = self.worker.take().expect("a finished worker remains owned");
        let joined = worker.join();
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let result = if joined.is_ok() {
            state
                .result
                .take()
                .unwrap_or(Err(EditorPlayFailure::Preparation))
        } else {
            Err(EditorPlayFailure::Preparation)
        };
        Some(GenerationPreparationOutcome {
            store: state.store.take(),
            result,
        })
    }
}

impl Drop for GenerationPreparationOwner {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            // Normal Editor closure cancels and collects incrementally. Abandonment during an
            // error/unwind uses synchronous fail-safe cleanup instead of detaching the worker.
            // It returns no physical-close receipt; an unremovable store remains scavengable.
            self.request_cancel();
            let _ = worker.join();
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(store) = state.store.as_mut() {
                if let Some(active) = store.active().map(|generation| generation.id()) {
                    let _ = store.finish_work_directory();
                    let _ = store.finish_active(active);
                }
                let _ = store.try_close();
            }
        }
    }
}

fn prepare(
    state: &mut PreparationState,
    cancel: &AtomicBool,
) -> Result<ImmutableExecutableGeneration, EditorPlayFailure> {
    if cancel.load(Ordering::Acquire) {
        return Err(EditorPlayFailure::Preparation);
    }
    let request = state.request.take().ok_or(EditorPlayFailure::Preparation)?;
    if state.store.is_none() {
        let base = generation_store_base_root().map_err(|_| EditorPlayFailure::Start)?;
        let _ = ExecutableGenerationStore::scavenge_inactive(&base);
        state.store =
            Some(ExecutableGenerationStore::create(&base).map_err(|_| EditorPlayFailure::Start)?);
    }
    let store = state.store.as_mut().ok_or(EditorPlayFailure::Start)?;
    if cancel.load(Ordering::Acquire) {
        return Err(EditorPlayFailure::Preparation);
    }
    match request {
        GenerationPreparationRequest::Build {
            candidate,
            manifest,
        } => {
            candidate
                .revalidate_for_publication()
                .map_err(|_| EditorPlayFailure::Preparation)?;
            let generation = store
                .publish_candidate_cancellable(candidate.artifact_path(), manifest, cancel)
                .map_err(|_| EditorPlayFailure::Start)?;
            store
                .activate_candidate(generation.id())
                .map_err(|_| EditorPlayFailure::Start)
        }
        GenerationPreparationRequest::LastGood { source_receipt } => {
            if !store
                .last_good()
                .is_some_and(|generation| generation.source_receipt() == source_receipt)
            {
                return Err(EditorPlayFailure::Start);
            }
            store
                .activate_last_good()
                .map_err(|_| EditorPlayFailure::Start)
        }
    }
}
