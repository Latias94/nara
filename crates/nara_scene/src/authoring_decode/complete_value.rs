//! Complete value validation shared by source classification and derived projection admission.

use std::borrow::Cow;

use super::*;

pub(super) fn validate_complete_value<'value>(
    schema: &ComponentSchema,
    owner: &PersistentComponentSchemaOwnerId,
    component: &ComponentTypeId,
    value: &'value ComponentValue,
    limits: SceneFileLimits,
) -> Result<Cow<'value, ComponentValue>, AuthoringDocumentPublicationError> {
    let invalid = |summary, error| {
        rejection(with_codec_error(
            with_public_locator(
                with_public_locator(
                    diagnostic_error("scene.authoring-component-value-invalid", summary),
                    "schema-owner",
                    owner.as_str(),
                ),
                "component-id",
                component.as_str(),
            ),
            &error,
            Some(schema),
        ))
    };
    let effective =
        effective_component_value(schema, value, limits.component_materialization_limits())
            .map_err(|error| {
                invalid(
                    "Persistent authoring record exceeds its effective-value policy",
                    error,
                )
            })?;
    nara_reflect::validate_component_value(schema, &effective).map_err(|error| {
        invalid(
            "Persistent authoring record does not satisfy its declared schema",
            error,
        )
    })?;
    Ok(effective)
}

// The declarative compiler is still a private comparison. Keep its whole-document entry here
// until a production compiler establishes the actual captured base/Schema publication holder.
#[cfg(test)]
impl AuthoringSchemaBasis {
    pub(crate) fn validate_effective_document(
        &self,
        document: &SceneDocument,
        limits: SceneFileLimits,
    ) -> Result<(), AuthoringDocumentPublicationError> {
        crate::format::validate_scene_value_budget(document, limits).map_err(|_| {
            rejection(diagnostic_error(
                "scene.effective-document-budget-exceeded",
                "Effective Scene candidate exceeds its value budget",
            ))
        })?;
        let structure = validate_scene_structure(document);
        if structure.has_errors() {
            return Err(AuthoringDocumentPublicationError::new(structure));
        }
        let namespace = document
            .entities
            .iter()
            .map(|record| &record.id)
            .collect::<BTreeSet<_>>();
        let mut cost = nara_reflect::ComponentValueCost::ZERO;
        for entity in &document.entities {
            if entity.prefab.is_some() {
                return Err(rejection(diagnostic_error(
                    "scene.effective-document-not-expanded",
                    "Effective Scene candidate must already have expanded all Prefab instances",
                )));
            }
            for (component, record) in &entity.components {
                let classified = self.0.classify_record(component, record)?;
                if !matches!(
                    classified.readiness,
                    AuthoringRecordReadiness::Available | AuthoringRecordReadiness::KnownUnbound
                ) {
                    return Err(rejection(with_public_locator(
                        diagnostic_error(
                            "scene.effective-component-unavailable",
                            "Effective Scene candidate requires an admitted current component Schema",
                        ),
                        "component-id",
                        component.as_str(),
                    )));
                }
                let schema = classified
                    .schema
                    .as_ref()
                    .expect("current classification retains Schema");
                let owner = classified
                    .resolved_owner
                    .as_ref()
                    .expect("current classification retains its owner");
                let effective =
                    validate_complete_value(schema, owner, component, &record.value, limits)?;
                cost = cost.saturating_add(effective.cost());
                crate::format::validate_component_value_cost(cost, limits).map_err(|_| {
                    rejection(diagnostic_error(
                        "scene.effective-document-budget-exceeded",
                        "Effective Scene candidate exceeds its value budget after defaults",
                    ))
                })?;
                validate_effective_local_references(
                    schema,
                    &effective,
                    |entity| namespace.contains(entity),
                    reference_traversal_limits(limits.component_materialization_limits()),
                )
                .map_err(|_| {
                    rejection(with_public_locator(
                        diagnostic_error(
                            "scene.effective-local-reference-invalid",
                            "Effective Scene candidate references must close over its final namespace",
                        ),
                        "component-id",
                        component.as_str(),
                    ))
                })?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_core::{ByteLimit, ItemLimit};
    use nara_reflect::{ComponentFieldSchema, ComponentSchemaCatalog, ComponentValueKind};

    fn basis() -> AuthoringSchemaBasis {
        let schema = ComponentSchema::new(
            ComponentTypeId::new("game.Value"),
            "Value",
            ComponentSchemaVersion(2),
        )
        .with_capabilities([ComponentCapability::Scene])
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("note"),
            "Note",
            nara_reflect::ComponentFieldPath::from_fields(["note"]),
            ComponentValueKind::String,
            ComponentValue::String("hi".into()),
        )]);
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("game.values").unwrap(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        AuthoringSchemaBasis(Arc::new(
            AuthoringClassificationBasis::new(&bundle, &[]).unwrap(),
        ))
    }

    fn document() -> SceneDocument {
        SceneDocument::from_ordered_roots(["first", "last"].map(|name| {
            let mut entity = SceneEntityRecord::new(SceneEntityId::new(name).unwrap());
            entity.components.insert(
                ComponentTypeId::new("game.Value"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion(2),
                    ComponentValue::Map(BTreeMap::new()),
                ),
            );
            entity
        }))
    }

    #[test]
    fn complete_candidate_checks_aggregate_default_cost_without_changing_authored_values() {
        let basis = basis();
        let document = document();
        // Two maps, each with one "note" key and "hi" string: 4 nodes and 12 logical bytes.
        let exact = SceneFileLimits::default()
            .with_component_value_nodes(ItemLimit::new(4).unwrap())
            .with_component_value_bytes(ByteLimit::new(12).unwrap());
        basis.validate_effective_document(&document, exact).unwrap();
        for limits in [
            exact.with_component_value_nodes(ItemLimit::new(3).unwrap()),
            exact.with_component_value_bytes(ByteLimit::new(11).unwrap()),
        ] {
            let error = basis
                .validate_effective_document(&document, limits)
                .unwrap_err();
            assert!(
                error
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str()
                        == "scene.effective-document-budget-exceeded")
            );
        }
        assert!(document.entities.iter().all(|entity| {
            entity.components[&ComponentTypeId::new("game.Value")].value
                == ComponentValue::Map(BTreeMap::new())
        }));
    }

    #[test]
    fn source_preservation_does_not_grant_projection_admission_to_unknown_or_wrong_version_records()
    {
        let basis = basis();
        let original = document();
        for version in [ComponentSchemaVersion::ONE, ComponentSchemaVersion(3)] {
            let mut document = original.clone();
            document.entities[1]
                .components
                .get_mut(&ComponentTypeId::new("game.Value"))
                .unwrap()
                .version = version;
            let source = classify_scene_document(
                &document,
                AuthoringDocumentSource::new(AssetRef::path("scene.json").unwrap()),
                AuthoringDocumentKind::Scene,
                Arc::clone(&basis.0),
                SceneFileLimits::default(),
            )
            .unwrap();
            assert_eq!(source.readiness, AuthoringDocumentReadiness::Degraded);
            assert!(
                basis
                    .validate_effective_document(&document, SceneFileLimits::default())
                    .is_err()
            );
        }
        let mut document = original;
        let component = ComponentTypeId::new("game.Value");
        document.entities[1]
            .components
            .get_mut(&component)
            .unwrap()
            .owner = Some(PersistentComponentSchemaOwnerId::try_new("game.other").unwrap());
        assert!(
            basis
                .validate_effective_document(&document, SceneFileLimits::default())
                .is_err()
        );
        let record = document.entities[1].components.get_mut(&component).unwrap();
        record.owner = None;
        record.value = ComponentValue::map([("undeclared", ComponentValue::Null)]);
        assert!(
            basis
                .validate_effective_document(&document, SceneFileLimits::default())
                .is_err()
        );
        let record = document.entities[1].components.remove(&component).unwrap();
        document.entities[1]
            .components
            .insert(ComponentTypeId::new("game.Unknown"), record);
        assert!(
            basis
                .validate_effective_document(&document, SceneFileLimits::default())
                .is_err()
        );
    }
}
