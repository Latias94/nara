use thiserror::Error;

#[derive(Debug)]
enum DrainState {
    Live,
    Flush,
    Polling,
    Complete,
    Poisoned,
}

/// Failure to advance the retained device's physical queue drain.
#[doc(hidden)]
#[derive(Debug, Error)]
pub enum WgpuGpuRetirementError {
    #[error("GPU retirement cannot resume an interrupted destroy or flush")]
    Interrupted,
    #[error(transparent)]
    Poll(#[from] wgpu::PollError),
}

/// GPU drain custody independent of renderer construction and backend namespace validity.
///
/// Retain this token before initialization uploads or the first accepted submission. The owner
/// must stop all work on this device and queue before beginning retirement, and keep it stopped
/// until the dependent owners retire. A completed token proves queue completion, not native
/// presentation or OS window destruction.
#[doc(hidden)]
#[derive(Debug)]
pub struct WgpuGpuRetirement {
    device: wgpu::Device,
    queue: wgpu::Queue,
    state: DrainState,
}

impl WgpuGpuRetirement {
    /// Retains the actual device and queue returned together by one device request.
    #[must_use]
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            state: DrainState::Live,
        }
    }

    /// Stops admission by caller contract and selects the drain branch exactly once.
    ///
    /// `discard_pending` destroys this exact device, forbidding further submissions and discarding
    /// uploads not yet submitted. It does not assume physical loss or queue completion: previously
    /// accepted submissions still have to drain through the retained device's actual poll result.
    /// Repeated calls do not reset progress or change the selected branch.
    pub fn begin(&mut self, discard_pending: bool) {
        if !matches!(self.state, DrainState::Live) {
            return;
        }
        if discard_pending {
            self.state = DrainState::Poisoned;
            self.device.destroy();
            self.state = DrainState::Polling;
        } else {
            self.state = DrainState::Flush;
        }
    }

    /// Flushes initialization uploads once, then polls without waiting for the GPU.
    ///
    /// Without an explicit `begin`, the first poll selects the normal upload-flushing branch.
    /// Loss alone never completes retirement: wgpu-core tracks submitted work after device loss.
    /// If loss races the flush, WGPU reports the rejected submission through its device-loss
    /// callback; the returned submission index is deliberately not used as completion evidence.
    /// Only the retained device's `QueueEmpty` result establishes completion. Poll errors leave the
    /// token retryable. A panic during destroy/flush leaves a poisoned token that cannot claim
    /// completion on reuse; this is not an unwind recovery protocol. WGPU 30 can report HAL fence
    /// errors through its fatal handler rather than `PollError`; those are not completion.
    pub fn poll(&mut self) -> Result<bool, WgpuGpuRetirementError> {
        self.begin(false);
        if matches!(self.state, DrainState::Complete) {
            return Ok(true);
        }
        if matches!(self.state, DrainState::Poisoned) {
            return Err(WgpuGpuRetirementError::Interrupted);
        }
        if matches!(self.state, DrainState::Flush) {
            self.state = DrainState::Poisoned;
            self.queue.submit([]);
            self.state = DrainState::Polling;
        }
        let status = self.device.poll(wgpu::PollType::Poll)?;
        if status.is_queue_empty() {
            self.state = DrainState::Complete;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
