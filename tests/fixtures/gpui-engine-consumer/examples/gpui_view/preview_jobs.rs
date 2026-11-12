//! Finite captured evaluation on the already installed platform background executor.

use anyhow::{Result, ensure};
use async_channel::{Receiver, Sender};
use nara_gpui_engine_consumer::graph::{
    GraphView,
    preview::{
        GraphPreviewBasis, GraphPreviewCompletion, GraphPreviewDisposition, GraphPreviewOwner,
        GraphPreviewRequestId, GraphPreviewState,
    },
};
use open_gpui::{BackgroundExecutor, Task};
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
};

struct Job {
    task: Task<GraphPreviewCompletion>,
    release: Option<Sender<()>>,
}

#[derive(Default)]
pub(super) struct PreviewJobs {
    owner: GraphPreviewOwner,
    jobs: Vec<Job>,
    hold_next: bool,
    pub accepted: u64,
    pub stale: u64,
    pub failed: u64,
}

impl PreviewJobs {
    pub fn observe(&mut self, basis: Option<GraphPreviewBasis>) -> bool {
        let changed = self.owner.state().current != basis;
        self.owner.observe(basis);
        changed
    }

    pub fn state(&self) -> GraphPreviewState {
        self.owner.state()
    }

    pub fn pending_jobs(&self) -> usize {
        self.jobs.len()
    }

    pub fn held_jobs(&self) -> usize {
        self.jobs.iter().filter(|job| job.release.is_some()).count()
    }

    pub fn hold_next(&mut self) {
        self.hold_next = true;
    }

    pub fn release_held(&mut self) -> Result<()> {
        for job in &mut self.jobs {
            if let Some(release) = job.release.take() {
                release.try_send(())?;
            }
        }
        Ok(())
    }

    pub fn start(
        &mut self,
        view: GraphView,
        executor: &BackgroundExecutor,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<GraphPreviewRequestId> {
        ensure!(
            self.jobs.len() < 4,
            "four graph evaluations are still returning"
        );
        let work = self.owner.request(view)?;
        let request = work.id();
        let (release, receiver): (Option<Sender<()>>, Option<Receiver<()>>) =
            if std::mem::take(&mut self.hold_next) {
                let (sender, receiver) = async_channel::bounded(1);
                (Some(sender), Some(receiver))
            } else {
                (None, None)
            };
        let task = executor.spawn(async move {
            if let Some(receiver) = receiver {
                receiver
                    .recv()
                    .await
                    .expect("a held job retains its release sender");
            }
            let result = work.execute();
            wake();
            result
        });
        self.jobs.push(Job { task, release });
        Ok(request)
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        let mut index = 0;
        while index < self.jobs.len() {
            if !self.jobs[index].task.is_ready() {
                index += 1;
                continue;
            }
            let mut job = self.jobs.remove(index);
            let mut context = Context::from_waker(Waker::noop());
            let Poll::Ready(completion) = Pin::new(&mut job.task).poll(&mut context) else {
                self.jobs.insert(index, job);
                index += 1;
                continue;
            };
            match self.owner.complete(completion) {
                GraphPreviewDisposition::Accepted => {
                    self.accepted += 1;
                    changed = true;
                }
                GraphPreviewDisposition::Failed => {
                    self.failed += 1;
                    changed = true;
                }
                GraphPreviewDisposition::Stale => self.stale += 1,
            }
        }
        changed
    }

    pub fn close(&mut self) -> Result<bool> {
        self.owner.observe(None);
        self.release_held()?;
        self.poll();
        Ok(self.jobs.is_empty())
    }
}
