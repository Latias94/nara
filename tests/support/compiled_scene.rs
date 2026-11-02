use nara::{
    reflect::ComponentRegistrySnapshot,
    scene::{
        CompiledSceneCandidate, InMemoryPrefabSourceResolver, PrefabDocument, SceneDocument,
        SourceDocumentKey,
    },
};

pub fn scene(
    registry: &ComponentRegistrySnapshot,
    document: &SceneDocument,
    source: SourceDocumentKey,
) -> CompiledSceneCandidate {
    let report = document.compile(registry, &InMemoryPrefabSourceResolver::new());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report
        .candidate
        .expect("the source compiles")
        .bind_source(source)
}

#[allow(dead_code)]
pub fn prefab(
    registry: &ComponentRegistrySnapshot,
    document: &PrefabDocument,
    source: SourceDocumentKey,
) -> CompiledSceneCandidate {
    let report = document.compile(registry, &InMemoryPrefabSourceResolver::new());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report
        .candidate
        .expect("the Prefab source compiles")
        .bind_source(source)
}
