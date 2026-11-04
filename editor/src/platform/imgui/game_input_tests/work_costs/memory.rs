//! Process observations around complete repeated native lifetimes, without allocator trimming.
//!
//! PrivateUsage is process private commit, not private resident memory. WorkingSetSize includes
//! shared pages. Both include native/driver allocations and observer/setup work, not just UI.

use std::{io, mem::size_of};

use windows_sys::Win32::System::{
    ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
    Threading::GetCurrentProcess,
};
use windows_sys::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsWindow};
use winit::{
    event_loop::ControlFlow,
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
};

use super::*;

const WARMUP_CYCLES: usize = 4;
const MEASURED_CYCLES: usize = 12;
const SOURCE_PATHS: [&str; 2] = ["scenes/startup.scene.json", "prefabs/ui.prefab.json"];

#[derive(Clone, Copy, Debug)]
struct ProcessMemory {
    private_commit_bytes: u64,
    working_set_bytes: u64,
    lifetime_peak_commit_bytes: u64,
    lifetime_peak_working_set_bytes: u64,
}

impl ProcessMemory {
    fn capture() -> Result<Self, String> {
        let mut counters = PROCESS_MEMORY_COUNTERS_EX {
            cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        // SAFETY: GetCurrentProcess returns a valid borrowed pseudo-handle. The writable EX
        // structure starts with PROCESS_MEMORY_COUNTERS and the API accepts its full size;
        // it retains neither the pointer nor the handle. No CloseHandle is needed.
        let succeeded = unsafe {
            K32GetProcessMemoryInfo(GetCurrentProcess(), (&raw mut counters).cast(), counters.cb)
        };
        if succeeded == 0 {
            return Err(format!(
                "current-process memory observation: {}",
                io::Error::last_os_error()
            ));
        }
        Ok(Self {
            private_commit_bytes: counters.PrivateUsage as u64,
            working_set_bytes: counters.WorkingSetSize as u64,
            lifetime_peak_commit_bytes: counters.PeakPagefileUsage as u64,
            lifetime_peak_working_set_bytes: counters.PeakWorkingSetSize as u64,
        })
    }

    fn values(self) -> [u64; 4] {
        [
            self.private_commit_bytes,
            self.working_set_bytes,
            self.lifetime_peak_commit_bytes,
            self.lifetime_peak_working_set_bytes,
        ]
    }
}

/// Detached values only; never retain an image, window, frontend, Host or Application.
struct CycleWork {
    retirement: WorkRetirement,
    backend_instance_id: u64,
    geometry: WorkGeometry,
    before: ProcessMemory,
    live: ProcessMemory,
    // Host retained payload, candidate metadata, backend texture, instance and total payload.
    sampled_host_maxima: [u64; 5],
    live_host_values: [u64; 5],
}

struct MemoryCycle {
    work: CycleWork,
    after: ProcessMemory,
    platform_dispatch_ns: u64,
}

struct PendingCycle {
    work: CycleWork,
    window_id: WindowId,
    // Borrowed identifier for a read-only IsWindow check; no native resource ownership.
    hwnd: HWND,
    destroyed_event: bool,
    returned_to_winit: Instant,
    deadline: Instant,
}

struct MemoryHost<'a> {
    project: &'a Path,
    sources: [Vec<u8>; 2],
    cycles: Vec<MemoryCycle>,
    pending: Option<PendingCycle>,
    result: Option<Result<(), String>>,
}

impl ApplicationHandler for MemoryHost<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.cycles.is_empty()
            && self.pending.is_none()
            && self.result.is_none()
            && let Err(error) = self.begin_cycle(event_loop)
        {
            self.result = Some(Err(error));
            event_loop.exit();
        }
    }

    fn window_event(&mut self, _: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::Destroyed)
            && let Some(pending) = self
                .pending
                .as_mut()
                .filter(|pending| pending.window_id == id)
        {
            pending.destroyed_event = true;
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.advance(event_loop) {
            self.result = Some(Err(error));
            event_loop.exit();
        }
    }
}

impl<'a> MemoryHost<'a> {
    fn new(project: &'a Path) -> Result<Self, String> {
        let [scene, prefab] = SOURCE_PATHS
            .map(|path| std::fs::read(project.join(path)).map_err(|error| error.to_string()));
        Ok(Self {
            project,
            sources: [scene?, prefab?],
            // Preallocate before the first snapshot; retain only bounded detached facts.
            cycles: Vec::with_capacity(WARMUP_CYCLES + MEASURED_CYCLES),
            pending: None,
            result: None,
        })
    }

    fn begin_cycle(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let index = self.cycles.len();
        let retirement = if index.is_multiple_of(2) {
            WorkRetirement::Healthy
        } else {
            WorkRetirement::DestroyOriginalDevice
        };
        let before = ProcessMemory::capture()?;
        let (work, window_id, hwnd, window) =
            run_game_workflow(event_loop, self.project, false, |mut workflow| {
                let window = Arc::downgrade(&workflow.native.window);
                let window_id = workflow.native.window.id();
                let RawWindowHandle::Win32(handle) = workflow
                    .native
                    .window
                    .window_handle()
                    .map_err(|error| error.to_string())?
                    .as_raw()
                else {
                    return Err("the Windows memory probe needs an actual Win32 window".into());
                };
                let hwnd = handle.hwnd.get() as HWND;
                let (geometry, backend_instance_id, maxima) = {
                    let observed = workflow.inspector_work_costs(false, retirement)?;
                    let mut maxima = [0_u64; 5];
                    for sample in &observed.samples {
                        let values = [
                            sample.retained_resource_bytes,
                            sample.retained_candidate_metadata_bytes,
                            sample.resources.current_texture_bytes(),
                            sample.resources.current_instance_buffer_bytes(),
                            sample.resources.current_total_bytes(),
                        ];
                        for (maximum, value) in maxima.iter_mut().zip(values) {
                            *maximum = (*maximum).max(value);
                        }
                    }
                    if observed.samples.len() != SAMPLE_FRAMES
                        || observed.queue_completions.len() != SAMPLE_FRAMES
                        || observed.retirement != retirement
                    {
                        return Err("the memory cycle lost its complete native workload".into());
                    }
                    (
                        observed.geometry,
                        observed.backend_before.backend_instance_id,
                        maxima,
                    )
                    // Drop per-frame observer vectors before the live process observation.
                };
                let resources = workflow.native.gpu.runtime_facts().resources;
                let live_host_values = [
                    workflow.native.gpu.retained_resource_bytes(),
                    workflow.native.gpu.retained_candidate_metadata_bytes(),
                    resources.current_texture_bytes(),
                    resources.current_instance_buffer_bytes(),
                    resources.current_total_bytes(),
                ];
                let live = ProcessMemory::capture()?;
                Ok((
                    CycleWork {
                        retirement,
                        backend_instance_id,
                        geometry,
                        before,
                        live,
                        sampled_host_maxima: maxima,
                        live_host_values,
                    },
                    window_id,
                    hwnd,
                    window,
                ))
            })?;
        // The original harness requires physical graphics cleanup and Session retirement, then
        // drops Native/window/Application before returning. Rust release still posts an async
        // Win32 destroy request: return to Winit before observing platform release and memory.
        let window_released = window.upgrade().is_none();
        drop(window);
        if !window_released
            || self
                .cycles
                .iter()
                .any(|cycle| cycle.work.backend_instance_id == work.backend_instance_id)
            || self
                .cycles
                .first()
                .is_some_and(|cycle| cycle.work.geometry != work.geometry)
        {
            return Err(format!(
                "native lifetime did not release its window, use a fresh Host, or preserve geometry: cycle={index}, window_released={window_released}, backend={}, geometry={:?}",
                work.backend_instance_id, work.geometry,
            ));
        }
        let returned_to_winit = Instant::now();
        let deadline = returned_to_winit + Duration::from_secs(10);
        self.pending = Some(PendingCycle {
            work,
            window_id,
            hwnd,
            destroyed_event: false,
            returned_to_winit,
            deadline,
        });
        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        Ok(())
    }

    fn advance(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if self.result.is_some() {
            event_loop.exit();
            return Ok(());
        }
        let Some(pending) = self.pending.as_ref() else {
            return Ok(());
        };
        if !pending.destroyed_event {
            if Instant::now() >= pending.deadline {
                return Err(
                    "native memory cycle timed out awaiting its exact Destroyed event".into(),
                );
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(pending.deadline));
            return Ok(());
        }
        let pending = self.pending.take().unwrap();
        // Destroyed is dispatched inside WM_DESTROY. Inspect only after that dispatch returned
        // to Winit's about-to-wait boundary, so WM_NCDESTROY and platform-state drop can finish.
        // SAFETY: IsWindow accepts a possibly destroyed HWND as a non-owning validity query.
        if unsafe { IsWindow(pending.hwnd) } != 0 {
            return Err("the retired native HWND remains live after Destroyed dispatch".into());
        }
        let platform_dispatch_ns = elapsed_ns(pending.returned_to_winit);
        let after = ProcessMemory::capture()?;
        for (path, original) in SOURCE_PATHS.iter().zip(&self.sources) {
            if std::fs::read(self.project.join(path)).map_err(|error| error.to_string())?
                != *original
            {
                return Err(format!("the repeated native lifetime changed {path}"));
            }
        }
        self.cycles.push(MemoryCycle {
            work: pending.work,
            after,
            platform_dispatch_ns,
        });
        if self.cycles.len() == WARMUP_CYCLES + MEASURED_CYCLES {
            self.result = Some(Ok(()));
            event_loop.exit();
            Ok(())
        } else {
            self.begin_cycle(event_loop)
        }
    }
}

#[test]
#[ignore = "Windows native process-memory/lifecycle baseline; run serially in release with captured output"]
fn measure_native_repeated_lifetime_memory() -> Result<(), String> {
    let project = UiProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().map_err(|error| error.to_string())?;
    let mut host = MemoryHost::new(&project.0)?;
    event_loop
        .run_app(&mut host)
        .map_err(|error| error.to_string())?;
    host.result
        .take()
        .ok_or("the native memory baseline did not run")??;
    println!(
        "NATIVE_MEMORY warmup_cycles={WARMUP_CYCLES} measured_cycles={MEASURED_CYCLES} retirement=alternating_healthy_then_destroy_original_device per_cycle_warmup_frames={WARMUP_FRAMES} per_cycle_sampled_frames={SAMPLE_FRAMES} all_original_workload_and_health_guards=true physical_graphics_and_Session_cleanup=true Rust_window_weak_released=true exact_Destroyed_event_and_HWND_absent=true fresh_Host_each_cycle=true source_files_unchanged=true"
    );
    println!(
        "NATIVE_MEMORY debug_assertions={} allocation_probe_feature={} primary_GPU_timestamp_instrumentation=false",
        cfg!(debug_assertions),
        cfg!(feature = "ui-work-allocation-probes"),
    );
    println!(
        "NATIVE_MEMORY process_order=private_commit_bytes,working_set_bytes,lifetime_peak_commit_bytes,lifetime_peak_working_set_bytes scope=whole_process_including_native_driver_and_shared_working_set lifetime_peaks_include_setup_and_warmups=true after=first_about_to_wait_after_original_teardown_and_exact_platform_window_destruction no_added_sleep_or_trim=true host_order=retained_payload,candidate_metadata,backend_texture,backend_instance,backend_total host_scope=logical_tracked_payload_not_total_GPU_residency"
    );
    for (index, cycle) in host.cycles.iter().enumerate() {
        let warmup = index < WARMUP_CYCLES;
        println!(
            "NATIVE_MEMORY cycle={index} warmup={warmup} retirement={:?} backend={} before={:?} live={:?} after={:?} sampled_host_maxima={:?} live_host={:?} return_to_Winit_to_post_Destroyed_boundary_ns={}",
            cycle.work.retirement,
            cycle.work.backend_instance_id,
            cycle.work.before.values(),
            cycle.work.live.values(),
            cycle.after.values(),
            cycle.work.sampled_host_maxima,
            cycle.work.live_host_values,
            cycle.platform_dispatch_ns,
        );
    }
    if let Some(first) = host.cycles.first() {
        println!("NATIVE_MEMORY geometry={:?}", first.work.geometry);
    }
    Ok(())
}
