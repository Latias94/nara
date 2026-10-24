use nara_core::SourceUniverseId;
use nara_scene::{SourceDocumentKey, SourceDocumentKind};

pub fn key(name: &str) -> SourceDocumentKey {
    let universe: SourceUniverseId = "e758bd18-2f06-4736-9752-dcfe038c3289".parse().unwrap();
    SourceDocumentKey::named(universe, name, SourceDocumentKind::Prefab).unwrap()
}
