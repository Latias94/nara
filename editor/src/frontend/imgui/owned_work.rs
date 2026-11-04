use nara_ui_kernel::UiAuthoringSnapshot;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct OwnedPanelWork {
    pub compile_calls: u64,
    pub built_snapshots: u64,
    pub authored_nodes: u64,
    pub reused_frames: u64,
}

impl OwnedPanelWork {
    pub fn record_snapshot(&mut self, snapshot: &UiAuthoringSnapshot) {
        self.built_snapshots = self.built_snapshots.saturating_add(1);
        self.authored_nodes = self
            .authored_nodes
            .saturating_add(snapshot.source_node_count() as u64);
    }
}

#[cfg(test)]
pub(super) fn measure_calls(
    _workload: &str,
    mut operation: impl FnMut(),
    samples: usize,
) -> [u128; 3] {
    let mut elapsed = Vec::with_capacity(samples);
    #[cfg(feature = "ui-work-allocation-probes")]
    let mut allocations = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = std::time::Instant::now();
        #[cfg(not(feature = "ui-work-allocation-probes"))]
        operation();
        #[cfg(feature = "ui-work-allocation-probes")]
        {
            let ((), work) = crate::allocation_probe::observe(&mut operation);
            allocations.push(work);
        }
        elapsed.push(started.elapsed().as_nanos());
    }
    #[cfg(feature = "ui-work-allocation-probes")]
    crate::allocation_probe::report(_workload, &allocations);
    elapsed.sort_unstable();
    [50, 95, 99].map(|percentile| elapsed[(samples * percentile).div_ceil(100) - 1])
}
