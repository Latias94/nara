#![cfg(feature = "serde")]

//! Decision-local requested-allocation observations, in a separate executable from timing tests.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentNumericDomain, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, PrefabAuthoringSession,
    PrefabDocument, PrefabDocumentCandidate, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, SceneFileLimits,
    ScenePatchDocument, ScenePatchOperation,
};

const POPULATION: usize = 4_096;
const WARMUPS: usize = 2;
const SAMPLES: usize = 4;
const OWNER: &str = "test.scalar-allocation";
const COMPONENT: &str = "test.scalar-allocation.Value";

#[global_allocator]
static ALLOCATOR: MeasuringAllocator = MeasuringAllocator;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static CALLS: [AtomicUsize; 3] = [const { AtomicUsize::new(0) }; 3];
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static FREED: AtomicUsize = AtomicUsize::new(0);

struct MeasuringAllocator;

fn record(kind: usize, allocated: usize, freed: usize) {
    // Track live requests from process startup, including outside observation windows.
    let live = if allocated >= freed {
        let increase = allocated - freed;
        LIVE.fetch_add(increase, Ordering::Relaxed) + increase
    } else {
        let decrease = freed - allocated;
        LIVE.fetch_sub(decrease, Ordering::Relaxed) - decrease
    };
    if ACTIVE.load(Ordering::SeqCst) {
        CALLS[kind].fetch_add(1, Ordering::Relaxed);
        ALLOCATED.fetch_add(allocated, Ordering::Relaxed);
        FREED.fetch_add(freed, Ordering::Relaxed);
        PEAK.fetch_max(live, Ordering::Relaxed);
    }
}

// SAFETY: Allocation semantics are delegated unchanged to System. Recording uses only atomics
// and cannot allocate or inspect the allocated memory.
unsafe impl GlobalAlloc for MeasuringAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc supplies the valid allocation layout forwarded to System.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record(0, layout.size(), 0);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc supplies the valid allocation layout forwarded to System.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record(0, layout.size(), 0);
        }
        pointer
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: The caller's live pointer, original layout and new size are forwarded unchanged.
        let replacement = unsafe { System.realloc(pointer, layout, new_size) };
        if !replacement.is_null() {
            record(1, new_size, layout.size());
        }
        replacement
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The caller's live pointer and its allocation layout are forwarded unchanged.
        unsafe { System.dealloc(pointer, layout) };
        record(2, 0, layout.size());
    }
}

struct Window {
    before_live: usize,
    after_live: usize,
    peak_live: usize,
    calls: [usize; 3],
    allocated: usize,
    freed: usize,
}

fn start_window() -> usize {
    assert!(!ACTIVE.load(Ordering::SeqCst));
    for counter in &CALLS {
        counter.store(0, Ordering::Relaxed);
    }
    ALLOCATED.store(0, Ordering::Relaxed);
    FREED.store(0, Ordering::Relaxed);
    let before = LIVE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::SeqCst);
    before
}

fn end_window(before_live: usize) -> Window {
    ACTIVE.store(false, Ordering::SeqCst);
    Window {
        before_live,
        after_live: LIVE.load(Ordering::Relaxed),
        peak_live: PEAK.load(Ordering::Relaxed),
        calls: std::array::from_fn(|index| CALLS[index].load(Ordering::Relaxed)),
        allocated: ALLOCATED.load(Ordering::Relaxed),
        freed: FREED.load(Ordering::Relaxed),
    }
}

fn emit(kind: &str, sample: usize, phase: &str, window: Window, settled_live: usize) {
    let net = window.allocated as i128 - window.freed as i128;
    let live_delta = window.after_live as i128 - window.before_live as i128;
    assert_eq!(
        net, live_delta,
        "allocation window crossed unrelated activity"
    );
    println!(
        "{}",
        serde_json::json!({
            "measurement": "scalar-edit-requested-allocation-v1",
            "kind": kind, "population": POPULATION, "sample": sample, "phase": phase,
            "allocation_calls": window.calls[0], "reallocation_calls": window.calls[1],
            "deallocation_calls": window.calls[2], "allocated_requested_bytes": window.allocated,
            "freed_requested_bytes": window.freed, "net_requested_bytes": net,
            "before_live_requested_bytes": window.before_live,
            "after_live_requested_bytes": window.after_live,
            "peak_above_start_requested_bytes": window.peak_live.saturating_sub(window.before_live),
            "settled_live_requested_bytes": settled_live,
            "settled_live_delta_requested_bytes": settled_live as i128 - window.before_live as i128,
            "counter_delta_matches": true,
        })
    );
}

fn fixture() -> (
    AuthoringSchemaBundle,
    Vec<AuthoringAdapterReceipt>,
    SceneDocument,
) {
    let owner = PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap();
    let schema = ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Allocation scalar",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::U64,
    )
    .with_numeric_domain(Some(ComponentNumericDomain::U8))
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    let document = SceneDocument::from_ordered_roots((0..POPULATION).map(|index| {
        SceneEntityRecord::new(SceneEntityId::new(format!("entity-{index:05}")).unwrap())
            .with_component(
                ComponentTypeId::new(COMPONENT),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([("value", ComponentValue::U64(0))]),
                )
                .with_owner(owner.clone()),
            )
    }));
    (bundle, vec![adapter], document)
}

// The two concrete owners share the same test sequence, without adding an owner abstraction.
macro_rules! observe_session {
    ($kind:literal, $session:expr, $baseline:expr) => {{
        let mut session = $session;
        let baseline = $baseline;
        let component = ComponentTypeId::new(COMPONENT);
        let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: SceneEntityId::new("entity-00000").unwrap(),
            component: component.clone(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("value"),
            value: ComponentValue::U64(7),
        }]);
        let baseline_state = session.content_state();
        for iteration in 0..WARMUPS + SAMPLES {
            let revision = session.revision();
            let before = start_window();
            let report = session.apply_authoring_patch(&patch);
            let edit = end_window(before);
            assert!(report.applied, "{:?}", report.diagnostics);
            assert_ne!(session.revision(), revision);
            assert_eq!(session.document().entities.len(), POPULATION);
            assert_eq!(session.document().roots, baseline.roots);
            assert_eq!(
                session.document().entities[0].components[&component]
                    .value
                    .field("value")
                    .unwrap(),
                &ComponentValue::U64(7)
            );
            assert_eq!(session.document().entities[1..], baseline.entities[1..]);
            assert_eq!(session.history_status().undo_depth, 1);
            assert_eq!(session.history_status().redo_depth, 0);
            drop(report);
            let edit_settled = LIVE.load(Ordering::Relaxed);

            let before = start_window();
            let report = session.undo_authoring();
            let undo = end_window(before);
            assert!(report.applied, "{:?}", report.diagnostics);
            assert_eq!(session.document(), &baseline);
            assert_eq!(session.content_state(), baseline_state);
            assert_eq!(session.history_status().undo_depth, 0);
            assert_eq!(session.history_status().redo_depth, 1);
            drop(report);
            let undo_settled = LIVE.load(Ordering::Relaxed);

            let before = start_window();
            let snapshot = black_box(session.document()).clone();
            let clone = end_window(before);
            assert_eq!(snapshot, baseline);
            let clone_settled = LIVE.load(Ordering::Relaxed);
            let before = start_window();
            drop(black_box(snapshot));
            let destroy = end_window(before);
            let destroy_settled = LIVE.load(Ordering::Relaxed);
            if iteration >= WARMUPS {
                let sample = iteration - WARMUPS;
                emit($kind, sample, "edit-through-report", edit, edit_settled);
                emit($kind, sample, "undo-through-report", undo, undo_settled);
                emit(
                    $kind,
                    sample,
                    "document-clone-retained",
                    clone,
                    clone_settled,
                );
                emit(
                    $kind,
                    sample,
                    "document-clone-drop",
                    destroy,
                    destroy_settled,
                );
            }
        }
    }};
}

#[test]
#[ignore = "decision-local allocation observation; run this exact test alone in release mode"]
fn scalar_edit_and_snapshot_requested_allocations() {
    println!(
        "{}",
        serde_json::json!({
            "measurement": "scalar-edit-allocation-method-v1", "warmups": WARMUPS,
            "samples_per_kind": SAMPLES, "population": POPULATION, "timing_measured": false,
            "scope": "process-wide successful System GlobalAlloc requests; alloc_zeroed counts as allocation; realloc releases old size and allocates full new size",
            "windows": "edit/Undo include returned report; settled checkpoints drop report; document clone/drop are separate windows; retained baseline and prepared patch exist before windows",
            "limits": "requested bytes are not allocator usable bytes, RSS, GPU residency or complete owner heap; unrelated thread activity is not filtered; no App/workers are created; no timing or leak claim",
        })
    );
    let (bundle, adapters, document) = fixture();
    let limits = SceneFileLimits::default()
        .with_encoded_bytes(ByteLimit::new(8 * 1024 * 1024).unwrap())
        .with_entities(ItemLimit::new(POPULATION).unwrap())
        .with_components(ItemLimit::new(POPULATION).unwrap());
    let published = SceneDocumentCandidate::decode_json_bytes_with_limits(
        document.to_json_string().unwrap().as_bytes(),
        limits,
    )
    .unwrap()
    .admit_authoring(
        AuthoringDocumentSource::new(AssetRef::path("scenes/allocation.scene.json").unwrap()),
        &bundle,
        &adapters,
    )
    .unwrap()
    .publish();
    observe_session!(
        "scene",
        SceneAuthoringSession::from_published_authoring_document(published),
        document.clone()
    );

    let prefab = PrefabDocument::new(document.roots, document.entities);
    let published = PrefabDocumentCandidate::decode_json_bytes_with_limits(
        prefab.to_json_string().unwrap().as_bytes(),
        limits,
    )
    .unwrap()
    .admit_authoring(
        AuthoringDocumentSource::new(AssetRef::path("prefabs/allocation.prefab.json").unwrap()),
        &bundle,
        &adapters,
    )
    .unwrap()
    .publish();
    observe_session!(
        "prefab",
        PrefabAuthoringSession::from_published_authoring_document(published),
        prefab
    );
}
