//! Opt-in current-thread requested-allocation observations, separate from timing runs.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;

thread_local! {
    static ACTIVE: Cell<Option<AllocationWork>> = const { Cell::new(None) };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AllocationWork {
    // Successful alloc, alloc_zeroed, realloc and dealloc requests, respectively.
    pub(crate) calls: [u64; 4],
    pub(crate) allocated_bytes: u64,
    pub(crate) freed_bytes: u64,
    pub(crate) net_bytes: i128,
    pub(crate) peak_growth_bytes: i128,
}

struct ObservedAllocator;

fn record(kind: usize, allocated: usize, freed: usize) {
    // Constant-initialized TLS and Cell access neither allocate nor borrow allocated memory.
    // Thread teardown can make TLS unavailable; it is outside an active observation window.
    let _ = ACTIVE.try_with(|active| {
        if let Some(mut work) = active.get() {
            work.calls[kind] = work.calls[kind].saturating_add(1);
            work.allocated_bytes = work.allocated_bytes.saturating_add(allocated as u64);
            work.freed_bytes = work.freed_bytes.saturating_add(freed as u64);
            work.net_bytes = work
                .net_bytes
                .saturating_add(allocated as i128 - freed as i128);
            work.peak_growth_bytes = work.peak_growth_bytes.max(work.net_bytes);
            active.set(Some(work));
        }
    });
}

// SAFETY: The original arguments and results are delegated unchanged to System. Observation
// uses only constant-initialized TLS/Cell and integer arithmetic, without allocation or unwinding.
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc's valid layout is forwarded unchanged.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record(0, layout.size(), 0);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc's valid layout is forwarded unchanged.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record(1, layout.size(), 0);
        }
        pointer
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: The caller's live allocation, original layout and new size are forwarded.
        let replacement = unsafe { System.realloc(pointer, layout, size) };
        if !replacement.is_null() {
            record(2, size, layout.size());
        }
        replacement
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The caller's live allocation and original layout are forwarded unchanged.
        unsafe { System.dealloc(pointer, layout) };
        record(3, 0, layout.size());
    }
}

struct Observation;

impl Drop for Observation {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.set(None));
    }
}

/// Observe only synchronous requests on this thread. The returned value remains alive at the
/// end boundary; its later destruction is excluded. Realloc releases the full old request and
/// allocates the full new request. Negative net bytes can release pre-existing allocations.
pub(crate) fn observe<T>(operation: impl FnOnce() -> T) -> (T, AllocationWork) {
    ACTIVE.with(|active| {
        assert!(
            active.get().is_none(),
            "allocation observations cannot nest"
        );
        active.set(Some(AllocationWork::default()));
    });
    let observation = Observation;
    let value = operation();
    let work = ACTIVE.with(|active| active.get().expect("the observation is active"));
    drop(observation);
    (value, work)
}

pub(crate) fn report(workload: &str, observations: &[AllocationWork]) {
    let mut calls = [0_u128; 4];
    let mut allocated = 0_u128;
    let mut freed = 0_u128;
    let mut net = 0_i128;
    let mut peak_growth = 0_i128;
    for work in observations {
        for (total, count) in calls.iter_mut().zip(work.calls) {
            *total += u128::from(count);
        }
        allocated += u128::from(work.allocated_bytes);
        freed += u128::from(work.freed_bytes);
        net += work.net_bytes;
        peak_growth = peak_growth.max(work.peak_growth_bytes);
    }
    println!(
        "UI_ALLOC workload={workload} samples={} calls_order=alloc,alloc_zeroed,realloc,dealloc totals={calls:?} allocated_bytes={allocated} freed_bytes={freed} summed_net_bytes={net} max_call_peak_growth_bytes={peak_growth} scope=current_thread_successful_System_requests timing_instrumented=true_not_timing_baseline",
        observations.len(),
    );
}

#[test]
fn observes_requested_sizes_and_releases_preexisting_storage() {
    use std::alloc::{alloc, alloc_zeroed, dealloc, realloc};

    let small = Layout::from_size_align(128, 8).unwrap();
    let large = Layout::from_size_align(256, 8).unwrap();
    // SAFETY: Each non-null allocation is resized/deallocated with its matching layout.
    unsafe {
        let existing = alloc(small);
        assert!(!existing.is_null());
        std::hint::black_box(existing);
        let ((), work) = observe(|| {
            dealloc(existing, small);
            let pointer = alloc_zeroed(small);
            assert!(!pointer.is_null());
            let pointer = realloc(pointer, small, large.size());
            assert!(!pointer.is_null());
            std::hint::black_box(pointer);
            dealloc(pointer, large);
        });
        assert_eq!(work.calls, [0, 1, 1, 2]);
        assert_eq!(work.allocated_bytes, 384);
        assert_eq!(work.freed_bytes, 512);
        assert_eq!(work.net_bytes, -128);
        assert_eq!(work.peak_growth_bytes, 128);
    }
}

#[test]
fn excludes_another_threads_requests() {
    use std::sync::atomic::{AtomicU8, Ordering};

    let phase = AtomicU8::new(0);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            while phase.load(Ordering::Acquire) != 1 {
                std::hint::spin_loop();
            }
            drop(std::hint::black_box(vec![0_u8; 4096]));
            phase.store(2, Ordering::Release);
        });
        let ((), work) = observe(|| {
            phase.store(1, Ordering::Release);
            while phase.load(Ordering::Acquire) != 2 {
                std::hint::spin_loop();
            }
        });
        worker.join().unwrap();
        assert_eq!(work, AllocationWork::default());
    });
}
