#![allow(dead_code)]

use nara_asset::AssetReferenceResolver;
use nara_reflect::ComponentRegistrySnapshot;
use nara_scene::{
    CompiledSceneCandidate, InMemoryPrefabSourceResolver, SceneComponentBindings, SceneDocument,
    SourceDocumentKey,
};

pub fn native(
    registry: &ComponentRegistrySnapshot,
    document: &SceneDocument,
    source: SourceDocumentKey,
    assets: Option<AssetReferenceResolver<'_>>,
) -> CompiledSceneCandidate {
    let resolver = InMemoryPrefabSourceResolver::new();
    let report = match assets {
        Some(assets) => document.compile_with_asset_resolution(registry, &resolver, assets),
        None => document.compile(registry, &resolver),
    };
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report.candidate.unwrap().bind_source(source)
}

pub fn mixed(
    bindings: &SceneComponentBindings,
    document: &SceneDocument,
    source: SourceDocumentKey,
    assets: Option<AssetReferenceResolver<'_>>,
) -> CompiledSceneCandidate {
    let report = bindings
        .try_compile_scene_with_loader(document, Default::default(), assets, |_, _| {
            Ok::<_, std::convert::Infallible>(None)
        })
        .unwrap();
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report.candidate.unwrap().bind_source(source)
}

pub fn assert_record(
    actual: &nara_scene::CompiledSceneComponent,
    expected: &nara_scene::SceneComponentRecord,
    component: &nara_reflect::ComponentTypeId,
    registry: &ComponentRegistrySnapshot,
) {
    let owner = expected.owner().map(|owner| owner.as_str()).or_else(|| {
        registry
            .component_schema_owner(component)
            .map(|owner| owner.as_str())
    });
    assert_eq!(actual.schema_owner().map(|owner| owner.as_str()), owner);
    assert_eq!(actual.version(), expected.version);
    assert_eq!(actual.value(), &expected.value);
}

pub fn assert_components(
    actual: &std::collections::BTreeMap<
        nara_reflect::ComponentTypeId,
        nara_scene::CompiledSceneComponent,
    >,
    expected: &std::collections::BTreeMap<
        nara_reflect::ComponentTypeId,
        nara_scene::SceneComponentRecord,
    >,
    registry: &ComponentRegistrySnapshot,
) {
    assert_eq!(actual.len(), expected.len());
    for (component, expected) in expected {
        assert_record(&actual[component], expected, component, registry);
    }
}
