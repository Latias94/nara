use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs::File,
    mem::size_of,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Duration,
};

use nara_app::RuntimeInstance;
use nara_core::Vec2;
use nara_ecs::Entity;
use nara_hierarchy::HierarchyOperation;

#[path = "scale_measurement.rs"]
mod fixture;

const BLOCKS: usize = 4;
const SAMPLES_PER_BLOCK: usize = 64;

#[global_allocator]
static ALLOCATOR: MeasuringAllocator = MeasuringAllocator;

static LIVE_REQUESTED_BYTES: AtomicUsize = AtomicUsize::new(0);
static WINDOW_ACTIVE: AtomicBool = AtomicBool::new(false);
static WINDOW_PEAK: AtomicUsize = AtomicUsize::new(0);
static PROCESS_COUNTS: AtomicCounts = AtomicCounts::new();
static WINDOW_COUNTS: AtomicCounts = AtomicCounts::new();

struct MeasuringAllocator;

#[derive(Clone, Copy)]
enum AllocationKind {
    Allocate,
    Reallocate,
    Deallocate,
}

struct AtomicCounts {
    allocations: AtomicUsize,
    reallocations: AtomicUsize,
    deallocations: AtomicUsize,
    allocated_bytes: AtomicUsize,
    deallocated_bytes: AtomicUsize,
}

impl AtomicCounts {
    const fn new() -> Self {
        Self {
            allocations: AtomicUsize::new(0),
            reallocations: AtomicUsize::new(0),
            deallocations: AtomicUsize::new(0),
            allocated_bytes: AtomicUsize::new(0),
            deallocated_bytes: AtomicUsize::new(0),
        }
    }

    fn record(&self, kind: AllocationKind, allocated: usize, deallocated: usize) {
        let calls = match kind {
            AllocationKind::Allocate => &self.allocations,
            AllocationKind::Reallocate => &self.reallocations,
            AllocationKind::Deallocate => &self.deallocations,
        };
        calls.fetch_add(1, Ordering::Relaxed);
        self.allocated_bytes.fetch_add(allocated, Ordering::Relaxed);
        self.deallocated_bytes
            .fetch_add(deallocated, Ordering::Relaxed);
    }

    fn reset(&self) {
        self.allocations.store(0, Ordering::Relaxed);
        self.reallocations.store(0, Ordering::Relaxed);
        self.deallocations.store(0, Ordering::Relaxed);
        self.allocated_bytes.store(0, Ordering::Relaxed);
        self.deallocated_bytes.store(0, Ordering::Relaxed);
    }

    fn snapshot(&self) -> Counts {
        Counts {
            allocations: self.allocations.load(Ordering::Relaxed),
            reallocations: self.reallocations.load(Ordering::Relaxed),
            deallocations: self.deallocations.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            deallocated_bytes: self.deallocated_bytes.load(Ordering::Relaxed),
        }
    }
}

struct Counts {
    allocations: usize,
    reallocations: usize,
    deallocations: usize,
    allocated_bytes: usize,
    deallocated_bytes: usize,
}

struct Window {
    before_live: usize,
    after_live: usize,
    peak_live: usize,
    counts: Counts,
}

fn record_allocation(kind: AllocationKind, allocated: usize, deallocated: usize) {
    let live = if allocated >= deallocated {
        let increase = allocated - deallocated;
        LIVE_REQUESTED_BYTES.fetch_add(increase, Ordering::Relaxed) + increase
    } else {
        let decrease = deallocated - allocated;
        LIVE_REQUESTED_BYTES.fetch_sub(decrease, Ordering::Relaxed) - decrease
    };
    PROCESS_COUNTS.record(kind, allocated, deallocated);
    if WINDOW_ACTIVE.load(Ordering::SeqCst) {
        WINDOW_COUNTS.record(kind, allocated, deallocated);
        WINDOW_PEAK.fetch_max(live, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for MeasuringAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(AllocationKind::Allocate, layout.size(), 0);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_allocation(AllocationKind::Allocate, layout.size(), 0);
        }
        pointer
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, layout, new_size) };
        if !replacement.is_null() {
            record_allocation(AllocationKind::Reallocate, new_size, layout.size());
        }
        replacement
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        record_allocation(AllocationKind::Deallocate, 0, layout.size());
    }
}

fn start_window() -> usize {
    assert!(!WINDOW_ACTIVE.load(Ordering::SeqCst));
    WINDOW_COUNTS.reset();
    let before_live = LIVE_REQUESTED_BYTES.load(Ordering::Relaxed);
    WINDOW_PEAK.store(before_live, Ordering::Relaxed);
    WINDOW_ACTIVE.store(true, Ordering::SeqCst);
    before_live
}

fn end_window(before_live: usize) -> Window {
    WINDOW_ACTIVE.store(false, Ordering::SeqCst);
    Window {
        before_live,
        after_live: LIVE_REQUESTED_BYTES.load(Ordering::Relaxed),
        peak_live: WINDOW_PEAK.load(Ordering::Relaxed),
        counts: WINDOW_COUNTS.snapshot(),
    }
}

fn write_window(
    file: &mut File,
    population: usize,
    scenario: &str,
    block: usize,
    window: Window,
    input_requested_bytes: usize,
) {
    let net_requested_bytes =
        window.counts.allocated_bytes as i128 - window.counts.deallocated_bytes as i128;
    let observed_live_delta = window.after_live as i128 - window.before_live as i128;
    let counter_delta_matches = net_requested_bytes == observed_live_delta;
    fixture::write_row(
        file,
        &format!(
            "{{\"type\":\"allocation-block\",\"population\":{population},\"scenario\":\"{scenario}\",\"block\":{block},\"samples\":{SAMPLES_PER_BLOCK},\"allocation_calls\":{},\"reallocation_calls\":{},\"deallocation_calls\":{},\"allocated_requested_bytes\":{},\"deallocated_requested_bytes\":{},\"net_requested_bytes\":{net_requested_bytes},\"before_live_requested_bytes\":{},\"after_live_requested_bytes\":{},\"observed_live_delta\":{observed_live_delta},\"counter_delta_matches\":{counter_delta_matches},\"peak_live_requested_bytes\":{},\"peak_above_start_requested_bytes\":{},\"preallocated_input_requested_bytes\":{input_requested_bytes}}}",
            window.counts.allocations,
            window.counts.reallocations,
            window.counts.deallocations,
            window.counts.allocated_bytes,
            window.counts.deallocated_bytes,
            window.before_live,
            window.after_live,
            window.peak_live,
            window.peak_live.saturating_sub(window.before_live),
        ),
    );
}

fn drive_transform_frame(
    runtime: &mut RuntimeInstance,
    actor: Entity,
    local: &mut [Vec2],
    moving: bool,
) {
    if moving {
        let delta = Vec2::new(1.0, 0.0);
        fixture::submit_edit(
            runtime,
            fixture::Edit::Translate {
                entity: actor,
                delta,
            },
        );
        let actor_index = local.len() - 1;
        local[actor_index] += delta;
    }
    let status = runtime
        .drive(if moving {
            fixture::FIXED_DELTA
        } else {
            Duration::ZERO
        })
        .expect("allocation measurement frame should succeed")
        .frame()
        .expect("allocation measurement should produce a frame")
        .status;
    assert_eq!(status.fixed_steps, u32::from(moving));
    assert!(!status.fixed_steps_capped);
    assert_eq!(status.fixed_debt, Duration::ZERO);
    assert_eq!(status.fixed_discarded, Duration::ZERO);
}

fn measure_transform(file: &mut File, population: usize, moving: bool) {
    let mut app = fixture::spatial_app(false);
    let forest = fixture::make_fixture(&mut app, population);
    let actor = forest.entities[population - 1];
    let mut local = vec![Vec2::ZERO; population];
    let mut runtime = fixture::start_runtime(app);
    for _ in 0..fixture::WARMUP_SAMPLES {
        drive_transform_frame(&mut runtime, actor, &mut local, moving);
    }
    fixture::check_globals(&runtime, &forest, &local);
    let scenario = if moving {
        "one_leaf_one_fixed_step"
    } else {
        "static_zero_steps"
    };
    for block in 0..BLOCKS {
        let before_live = start_window();
        for _ in 0..SAMPLES_PER_BLOCK {
            drive_transform_frame(&mut runtime, actor, &mut local, moving);
        }
        let window = end_window(before_live);
        fixture::check_globals(&runtime, &forest, &local);
        write_window(file, population, scenario, block, window, 0);
    }
    fixture::stop_runtime(runtime);
}

fn measure_hierarchy(file: &mut File, independent: bool) {
    let population = 4_096;
    let mut app = fixture::spatial_app(false);
    let mut forest = fixture::make_fixture(&mut app, population);
    let actors = (0..population)
        .rev()
        .filter(|index| forest.children[*index].is_empty())
        .take(8)
        .collect::<Vec<_>>();
    assert_eq!(actors.len(), 8);
    let mut runtime = fixture::start_runtime(app);
    let mut iteration = 0usize;
    for _ in 0..fixture::WARMUP_SAMPLES {
        let destination = forest.roots[iteration % 2];
        let operations = fixture::prepare_leaf_reparent(&mut forest, &actors, destination);
        fixture::apply_leaf_reparent(&mut runtime, operations, independent);
        iteration += 1;
    }
    let local = vec![Vec2::ZERO; population];
    fixture::check_globals(&runtime, &forest, &local);
    let scenario = if independent {
        "independent_leaf_reparent8"
    } else {
        "batch_leaf_reparent8"
    };
    for block in 0..BLOCKS {
        let mut inputs = Vec::with_capacity(SAMPLES_PER_BLOCK);
        for _ in 0..SAMPLES_PER_BLOCK {
            let destination = forest.roots[iteration % 2];
            inputs.push(fixture::prepare_leaf_reparent(
                &mut forest,
                &actors,
                destination,
            ));
            iteration += 1;
        }
        let input_requested_bytes = inputs.capacity() * size_of::<Vec<HierarchyOperation>>()
            + inputs
                .iter()
                .map(|operations| operations.capacity() * size_of::<HierarchyOperation>())
                .sum::<usize>();
        let before_live = start_window();
        for operations in inputs {
            fixture::apply_leaf_reparent(&mut runtime, operations, independent);
        }
        let window = end_window(before_live);
        fixture::check_globals(&runtime, &forest, &local);
        write_window(
            file,
            population,
            scenario,
            block,
            window,
            input_requested_bytes,
        );
    }
    fixture::stop_runtime(runtime);
}

#[test]
#[ignore = "U12 allocation evidence; run this exact test alone in release mode"]
fn allocation_and_memory_baseline() {
    let mut file = fixture::output_file("allocation_and_memory_baseline");
    fixture::write_row(
        &mut file,
        &format!(
            "{{\"type\":\"allocation-method\",\"allocator\":\"System delegate\",\"warmup_samples\":{},\"blocks\":{BLOCKS},\"samples_per_block\":{SAMPLES_PER_BLOCK},\"timing_population\":\"separate executable and runs\",\"percentiles\":false,\"scope\":\"process-wide successful Rust GlobalAlloc requests; alloc_zeroed is an allocation; realloc counts old size released and full new size allocated\",\"exclusions\":\"setup, input construction, correctness oracles and JSONL output are outside active windows; input Vec destruction is inside hierarchy windows\",\"limits\":\"requested bytes are not allocator usable bytes, RSS, GPU memory or a leak proof; unrelated process allocator activity is not filtered by thread\"}}",
            fixture::WARMUP_SAMPLES,
        ),
    );
    for population in [64, 4_096, 65_536] {
        for moving in [false, true] {
            measure_transform(&mut file, population, moving);
        }
    }
    for independent in [false, true] {
        measure_hierarchy(&mut file, independent);
    }
    let counts = PROCESS_COUNTS.snapshot();
    let live = LIVE_REQUESTED_BYTES.load(Ordering::Relaxed);
    fixture::write_row(
        &mut file,
        &format!(
            "{{\"type\":\"process-observation\",\"allocation_calls\":{},\"reallocation_calls\":{},\"deallocation_calls\":{},\"allocated_requested_bytes\":{},\"deallocated_requested_bytes\":{},\"live_requested_bytes\":{live},\"live_counter_reset\":false}}",
            counts.allocations,
            counts.reallocations,
            counts.deallocations,
            counts.allocated_bytes,
            counts.deallocated_bytes,
        ),
    );
}
