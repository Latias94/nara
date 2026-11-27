use std::collections::BTreeMap;

use nara_core::EntityUuid;
use nara_reflect::{
    ComponentCapability, ComponentFieldSchema, ComponentRegistryError, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
};
use nara_scene::authoring::{SceneComponent, SceneDocument};

use crate::{EditorError, EditorSession, value_draft};

/// Owns the engine and last successfully discovered project schemas.
#[derive(Clone, Default)]
pub struct EditorRegistry {
    catalog: ComponentSchemaCatalog,
}

impl EditorRegistry {
    pub fn new(
        engine: impl IntoIterator<Item = ComponentSchema>,
        project: ComponentSchemaCatalog,
    ) -> Result<Self, ComponentRegistryError> {
        let engine = ComponentSchemaCatalog::from_components(engine)?;
        let mut schemas: BTreeMap<_, _> = engine
            .components()
            .iter()
            .map(|schema| (schema.id().clone(), schema.clone()))
            .collect();
        for schema in project.components() {
            match schemas.get(schema.id()) {
                Some(built_in) if built_in != schema => {
                    return Err(ComponentRegistryError::DuplicateComponentId(
                        schema.id().clone(),
                    ));
                }
                Some(_) => {}
                None => {
                    schemas.insert(schema.id().clone(), schema.clone());
                }
            }
        }
        Ok(Self {
            catalog: ComponentSchemaCatalog::from_components(schemas.into_values())?,
        })
    }

    pub fn schema(&self, id: &ComponentTypeId) -> Option<&ComponentSchema> {
        find(&self.catalog, id)
    }

    pub fn schemas(&self) -> impl Iterator<Item = &ComponentSchema> {
        self.catalog.components().iter()
    }

    pub fn catalog(&self) -> &ComponentSchemaCatalog {
        &self.catalog
    }

    pub(crate) fn editable(&self, id: &ComponentTypeId) -> Result<&ComponentSchema, EditorError> {
        self.schema(id)
            .filter(|schema| schema.has_capability(ComponentCapability::Edit))
            .ok_or_else(|| EditorError::ReadOnlyComponent(id.clone()))
    }
}

fn find<'a>(
    catalog: &'a ComponentSchemaCatalog,
    id: &ComponentTypeId,
) -> Option<&'a ComponentSchema> {
    let components = catalog.components();
    let index = components
        .binary_search_by(|schema| schema.id().cmp(id))
        .ok()?;
    components.get(index)
}

#[derive(Debug, Clone, PartialEq)]
pub enum InspectorValue {
    Missing,
    Shared(ComponentValue),
    Mixed,
}

#[derive(Debug, Clone)]
pub struct InspectorField {
    pub schema: ComponentFieldSchema,
    pub value: InspectorValue,
    pub editable: bool,
}

#[derive(Debug, Clone)]
pub struct InspectorComponent {
    pub id: ComponentTypeId,
    pub version: ComponentSchemaVersion,
    pub fields: Vec<InspectorField>,
    /// Stored data retained even when the current catalog cannot interpret it.
    pub opaque: Option<InspectorValue>,
}

#[derive(Debug, Clone, Default)]
pub struct InspectorModel {
    pub targets: Vec<EntityUuid>,
    pub components: Vec<InspectorComponent>,
}

impl InspectorModel {
    pub fn for_session(session: &EditorSession) -> Self {
        Self::new(
            session.preview_scene(),
            session.selection().entities(),
            session.registry(),
        )
    }

    pub fn new(scene: &SceneDocument, requested: &[EntityUuid], registry: &EditorRegistry) -> Self {
        let selected: Vec<_> = requested
            .iter()
            .filter_map(|id| scene.entities.iter().find(|row| row.id == *id))
            .collect();
        let mut model = Self {
            targets: selected.iter().map(|row| row.id).collect(),
            ..Self::default()
        };
        let Some(first) = selected.first() else {
            return model;
        };
        for (id, record) in &first.components {
            let shared = selected
                .iter()
                .map(|row| row.components.get(id))
                .collect::<Option<Vec<_>>>();
            if let Some(values) = shared {
                let schema = registry.schema(id).filter(|schema| {
                    schema.has_capability(ComponentCapability::Inspect)
                        && values.iter().all(|value| {
                            value.version() == schema.version() && value.value().is_some()
                        })
                });
                model
                    .components
                    .push(component(id, record, &values, schema));
            }
        }
        model
    }
}

fn component(
    id: &ComponentTypeId,
    record: &SceneComponent,
    values: &[&SceneComponent],
    schema: Option<&ComponentSchema>,
) -> InspectorComponent {
    let mut component = InspectorComponent {
        id: id.clone(),
        version: record.version(),
        fields: Vec::new(),
        opaque: None,
    };
    let Some(schema) = schema else {
        let raw_values: Vec<_> = values
            .iter()
            .map(|record| match record {
                SceneComponent::Unknown(unknown) => {
                    Some(ComponentValue::String(unknown.raw.as_ron().to_owned()))
                }
                SceneComponent::Known { .. } => None,
            })
            .collect();
        component.opaque = Some(summarize(raw_values.iter().map(Option::as_ref)));
        return component;
    };
    for field in schema.fields() {
        if !field.has_capability(ComponentCapability::Inspect) {
            continue;
        }
        component.fields.push(InspectorField {
            schema: field.clone(),
            value: summarize(values.iter().map(|record| {
                record
                    .value()
                    .and_then(|value| value.get_path(field.path()).ok())
                    .or_else(|| field.default_value())
            })),
            editable: schema.has_capability(ComponentCapability::Edit)
                && field.has_capability(ComponentCapability::Edit)
                && value_draft::supported(field.shape()),
        });
    }
    component
}

fn summarize<'a>(mut values: impl Iterator<Item = Option<&'a ComponentValue>>) -> InspectorValue {
    let first = values.next().flatten();
    if values.any(|value| value != first) {
        InspectorValue::Mixed
    } else if let Some(value) = first {
        InspectorValue::Shared(value.clone())
    } else {
        InspectorValue::Missing
    }
}
