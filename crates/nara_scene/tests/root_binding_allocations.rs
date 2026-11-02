//! Root lookup observes allocation requests independently of source construction and decoding.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

use nara_asset::AssetRef;
use nara_core::ByteLimit;
use nara_reflect::ComponentRegistryBuilder;
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabAddition, PrefabDocument, PrefabEntityTarget,
    PrefabInstance, PrefabOverlay, SceneCompilationFailureKind, SceneCompilationLimits,
    SceneCompilationOptions, SceneCompilationReport, SceneDocument, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, ScenePlacement, ScenePlacementParent,
    SourceDocumentKey, SourceDocumentKind,
};

const WORKING_BYTES: usize = 1024 * 1024;
const INVALID_ROOT_BYTES: usize = 16 * WORKING_BYTES;

struct ObservingAllocator;

#[global_allocator]
static ALLOCATOR: ObservingAllocator = ObservingAllocator;

thread_local! {
    static MAX_REQUEST: Cell<Option<usize>> = const { Cell::new(None) };
}

fn observe(size: usize) {
    let _ = MAX_REQUEST.try_with(|maximum| {
        if let Some(previous) = maximum.get() {
            maximum.set(Some(previous.max(size)));
        }
    });
}

// SAFETY: All allocation operations are delegated unchanged to System. Observation uses only
// an allocation-free thread-local Cell, and never accesses the allocated memory.
unsafe impl GlobalAlloc for ObservingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        observe(layout.size());
        // SAFETY: The caller's valid allocation layout is forwarded unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        observe(layout.size());
        // SAFETY: The caller's valid allocation layout is forwarded unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        observe(size);
        // SAFETY: The live pointer, original layout and requested size are forwarded unchanged.
        unsafe { System.realloc(pointer, layout, size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The caller's live pointer and original layout are forwarded unchanged.
        unsafe { System.dealloc(pointer, layout) }
    }
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn source() -> AssetRef {
    AssetRef::path("prefabs/root-allocation.prefab.ron").unwrap()
}

fn key() -> SourceDocumentKey {
    SourceDocumentKey::named(
        "aefb0ada-7b91-4861-ae01-391e423bd6f2".parse().unwrap(),
        "root-allocation",
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn compile_observing_allocations(
    document: &SceneDocument,
    sources: &InMemoryPrefabSourceResolver,
) -> (SceneCompilationReport, usize) {
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let options = SceneCompilationOptions::default().with_limits(
        SceneCompilationLimits::default()
            .with_working_bytes(ByteLimit::new(WORKING_BYTES).unwrap()),
    );
    // Only the synchronous public compilation runs inside this thread's observation window.
    assert!(MAX_REQUEST.replace(Some(0)).is_none());
    let report = document.compile_with_options(&registry, sources, options);
    let maximum = MAX_REQUEST.replace(None).unwrap();
    assert!(
        maximum > 0,
        "compilation must participate in allocation observation"
    );
    (report, maximum)
}

#[test]
fn oversized_missing_root_is_refused_without_allocating_its_lookup_key() {
    let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("q"))]);
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(source(), key(), prefab);
    // The caller already owns this identifier. Compilation must borrow it, even when it misses.
    let root = SceneEntityId::new("x".repeat(INVALID_ROOT_BYTES)).unwrap();
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance::root(source(), key(), root));
    let document = SceneDocument::from_ordered_roots([anchor]);

    let (report, maximum) = compile_observing_allocations(&document, &sources);

    assert!(
        maximum <= WORKING_BYTES,
        "a {INVALID_ROOT_BYTES}-byte missing root requested an unadmitted {maximum}-byte allocation"
    );
    assert_eq!(
        report.failure.as_ref().map(|failure| failure.reason()),
        Some(&SceneCompilationFailureKind::RootBinding),
        "{:?}",
        report.diagnostics,
    );
    assert!(report.candidate.is_none());
    assert!(report.diagnostics.has_errors());
}

#[test]
fn borrowed_root_lookup_preserves_source_and_addition_namespaces() {
    let prefab = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("q")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("child")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("q"),
                ))),
            ),
        ],
        [
            SceneEntityRecord::new(id("q")),
            SceneEntityRecord::new(id("child")),
        ],
    );
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(source(), key(), prefab);
    let mut overlay = PrefabOverlay::new(key());
    overlay.additions.push(PrefabAddition::new(id("q")));
    overlay.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::addition(id("q")),
        ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id("q"),
        ))),
    ));
    let mut instance = PrefabInstance::root(source(), key(), id("q"));
    instance.overrides = Some(overlay);
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(instance);
    let document = SceneDocument::from_ordered_roots([anchor]);

    let (report, maximum) = compile_observing_allocations(&document, &sources);

    assert!(maximum <= WORKING_BYTES);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let compiled = report.candidate.expect("valid binding must compile");
    assert_eq!(compiled.roots(), &[id("anchor")]);
    assert_eq!(compiled.entities().len(), 3);
    let anchor = compiled
        .entities()
        .iter()
        .find(|entity| entity.id() == &id("anchor"))
        .unwrap();
    assert_eq!(anchor.children().len(), 2);
    assert_ne!(anchor.children()[0], anchor.children()[1]);
    for (child, local_id) in anchor.children().iter().zip([id("child"), id("q")]) {
        let provenance = compiled.canonical_provenance().get(child).unwrap();
        assert_eq!(provenance.local_entity_id(), &local_id);
        assert!(provenance.projection_path().is_some());
    }
}
