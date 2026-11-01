use nara_reflect::{ComponentRegistryBuilder, ComponentRegistryError, ComponentRegistrySnapshot};
use nara_scene::register_scene_components;

/// Builds the admitted snapshot used by the direct scene-module example.
///
/// A mutable candidate cannot satisfy a Scene reader's admitted-definition input.
///
/// ```compile_fail,E0308
/// use nara_reflect::ComponentRegistryBuilder;
/// use nara_scene::SceneDocument;
///
/// let builder = ComponentRegistryBuilder::new();
/// SceneDocument::default().validate(&builder);
/// ```
///
/// Inspector projection also requires an admitted definition.
///
/// ```compile_fail,E0308
/// use nara_engine::{scene::{SceneAuthoringSession, SceneDocument}, tooling::SceneInspectorState};
/// use nara_reflect::ComponentRegistryBuilder;
///
/// let builder = ComponentRegistryBuilder::new();
/// let session = SceneAuthoringSession::new(SceneDocument::default());
/// SceneInspectorState::new().model_with_selection(&session, &builder, None, None);
/// ```
///
/// Admission consumes the builder; snapshots cannot register new declarations.
///
/// ```compile_fail,E0599
/// use nara_reflect::{
///     ComponentRegistryBuilder, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
/// };
///
/// let mut snapshot = ComponentRegistryBuilder::new().freeze().unwrap();
/// snapshot.register_component_schema(ComponentSchema::new(
///     ComponentTypeId::new("consumer.Forbidden"), "Forbidden", ComponentSchemaVersion::ONE,
/// ));
/// ```
pub fn frozen_scene_registry() -> Result<ComponentRegistrySnapshot, ComponentRegistryError> {
    let mut registry = ComponentRegistryBuilder::new();
    register_scene_components(&mut registry)?;
    registry.freeze().map_err(|error| error.into_error())
}
