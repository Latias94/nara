//! Borrowed logical ownership of admitted conversion snapshots, not allocator capacity or RSS.
//!
//! Raw documents and both patch directions use the compiler dependency model: 512 bytes per
//! structural work item, 96 per value node and actual logical content bytes. Typed metadata
//! uses the existing provenance/string model. Shared Schema/Adapter catalogs are excluded;
//! handles and independently retained classification/reference projections are charged.

use std::mem::size_of;

use nara_core::ItemLimit;

use super::LocalizedPrefabEntity;
use crate::{
    SceneEntityId, SceneEntityRecord, SceneFileLimits, ScenePatchApplyLimits, ScenePatchDocument,
    patch::{
        ScenePatchCost, bounded_scene_entities_source_cost,
        bounded_scene_entities_source_cost_with_admission, bounded_scene_patch_cost,
        bounded_scene_patch_cost_with_admission,
    },
    provenance::{PrefabProjectionStep, provenance_string_bytes},
};

pub(crate) fn document_dynamic_bytes(
    roots: &[SceneEntityId],
    entities: &[SceneEntityRecord],
    limits: SceneFileLimits,
) -> Option<usize> {
    let cost = bounded_scene_entities_source_cost(entities, source_cost_limits(limits)?)?;
    document_bytes_from_cost(roots, cost)
}

/// Admits raw walk scratch independently of the returned ownership bound.
pub(crate) fn document_dynamic_bytes_with_admission<E>(
    roots: &[SceneEntityId],
    entities: &[SceneEntityRecord],
    limits: SceneFileLimits,
    admit: impl FnMut(Option<usize>) -> Result<(), E>,
) -> Result<Option<usize>, E> {
    let Some(limits) = source_cost_limits(limits) else {
        return Ok(None);
    };
    Ok(
        bounded_scene_entities_source_cost_with_admission(entities, limits, admit)?
            .and_then(|cost| document_bytes_from_cost(roots, cost)),
    )
}

fn source_cost_limits(limits: SceneFileLimits) -> Option<ScenePatchApplyLimits> {
    // Classification owns these limits. Do not replace them with an unbounded raw-input walk.
    let work = limits
        .entities()
        .get()
        .checked_add(limits.placements().get())?
        .checked_add(limits.components().get())?
        .checked_add(limits.shape().nodes().get())?;
    Some(
        ScenePatchApplyLimits::default()
            .with_validation_work(ItemLimit::new(work)?)
            .with_validation_value_nodes(limits.component_value_nodes())
            .with_validation_value_bytes(limits.component_value_bytes()),
    )
}

fn document_bytes_from_cost(roots: &[SceneEntityId], cost: ScenePatchCost) -> Option<usize> {
    let bytes = cost
        .operation_count()
        .checked_mul(512)?
        .checked_add(cost.value_cost().nodes().checked_mul(96)?)?
        .checked_add(cost.logical_bytes())?
        .checked_add(roots.len().checked_mul(size_of::<SceneEntityId>())?)?;
    roots.iter().try_fold(bytes, |bytes, root| {
        bytes.checked_add(provenance_string_bytes(root.as_str())?)
    })
}

pub(crate) fn patch_dynamic_bytes(patch: &ScenePatchDocument) -> Option<usize> {
    let cost = bounded_scene_patch_cost(patch, ScenePatchApplyLimits::default())?;
    patch_bytes_from_cost(cost)
}

pub(crate) fn patch_dynamic_bytes_with_admission<E>(
    patch: &ScenePatchDocument,
    admit: impl FnMut(Option<usize>) -> Result<(), E>,
) -> Result<Option<usize>, E> {
    Ok(
        bounded_scene_patch_cost_with_admission(patch, ScenePatchApplyLimits::default(), admit)?
            .and_then(patch_bytes_from_cost),
    )
}

fn patch_bytes_from_cost(cost: ScenePatchCost) -> Option<usize> {
    cost.operation_count()
        .checked_mul(512)?
        .checked_add(cost.value_cost().nodes().checked_mul(96)?)?
        .checked_add(cost.logical_bytes())
}

fn review_map_bytes_inner(mapping: &LocalizedPrefabEntity) -> Option<usize> {
    let provenance = mapping.provenance();
    let mut bytes = size_of::<LocalizedPrefabEntity>()
        .checked_add(provenance_string_bytes(mapping.entity().as_str())?)?
        .checked_add(provenance_string_bytes(
            provenance.local_entity_id().as_str(),
        )?)?;
    if let Some(path) = provenance.projection_path() {
        bytes = bytes.checked_add(provenance_string_bytes(path.root_anchor().as_str())?)?;
        for step in path.steps() {
            bytes = bytes.checked_add(match step {
                PrefabProjectionStep::SourceEntity {
                    source_document,
                    canonical_source,
                    entity,
                } => PrefabProjectionStep::source_allocation_bytes(
                    source_document,
                    canonical_source,
                    entity,
                )?,
                PrefabProjectionStep::InstanceAddition { entity } => {
                    PrefabProjectionStep::addition_allocation_bytes(entity)?
                }
            })?;
        }
    }
    Some(bytes)
}

pub(super) fn review_map_bytes(mappings: &[LocalizedPrefabEntity]) -> Option<usize> {
    mappings.iter().try_fold(0usize, |bytes, mapping| {
        bytes.checked_add(review_map_bytes_inner(mapping)?)
    })
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, sync::Arc};

    use nara_asset::AssetRef;
    use nara_core::ByteLimit;
    use nara_reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind, EntityReference,
    };

    use super::*;
    use crate::{
        AuthoringDocumentSource, AuthoringSchemaBasis, PrefabAuthoringSession, PrefabDocument,
        PrefabDocumentCandidate, PrefabEntityTarget, PrefabExpansionWorkingSet, PrefabInstance,
        PrefabInstanceClaim, PrefabOverlay, PreparedScenePrefabConversion, ResolvedPrefabSource,
        SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
        ScenePatchOperation, SourceDocumentKey, SourceDocumentKind,
        authoring::prefab_conversion::tests::{
            admit_schema, basis, component, id, instance, schema_owner,
        },
    };

    fn key() -> SourceDocumentKey {
        SourceDocumentKey::named(
            "941c649a-3d67-4f5b-a62c-689d5e09c0f0".parse().unwrap(),
            "retained-base",
            SourceDocumentKind::Prefab,
        )
        .unwrap()
    }

    fn document(declaration: PrefabInstance) -> SceneDocument {
        let mut anchor = SceneEntityRecord::new(id("anchor"));
        anchor.prefab = Some(declaration);
        SceneDocument::from_ordered_roots([anchor])
    }

    fn open_scene(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
        SceneAuthoringSession::from_published_authoring_document(
            SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
                .unwrap()
                .admit_authoring_with_basis(
                    AuthoringDocumentSource::new(
                        AssetRef::path("scenes/retention.scene.json").unwrap(),
                    ),
                    basis,
                )
                .unwrap()
                .publish(),
        )
    }

    fn open_prefab(
        document: &SceneDocument,
        basis: &AuthoringSchemaBasis,
    ) -> PrefabAuthoringSession {
        let document = PrefabDocument::new(document.roots.clone(), document.entities.clone());
        PrefabAuthoringSession::from_published_authoring_document(
            PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
                .unwrap()
                .admit_authoring_with_basis(
                    AuthoringDocumentSource::new(
                        AssetRef::path("prefabs/retention.prefab.json").unwrap(),
                    ),
                    basis,
                )
                .unwrap()
                .publish(),
        )
    }

    #[test]
    fn retained_defaults_include_asset_projections_absent_from_the_captured_raw_document() {
        let path = format!("textures/{}.png", "default".repeat(128));
        let asset_basis = |path: &str| {
            let field = ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("image"),
                "Image",
                ComponentFieldPath::from_fields(["image"]),
                ComponentValueKind::AssetRef,
                crate::patch::asset_ref_value(&AssetRef::path(path).unwrap()),
            )
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::AssetRef]);
            admit_schema(
                ComponentSchema::new(component(), "Image", ComponentSchemaVersion::ONE)
                    .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
                    .with_fields([field]),
            )
        };
        let basis = asset_basis(&path);
        let mut source = document(instance("base"));
        source.entities[0].components.insert(
            component(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map(std::iter::empty::<(&str, ComponentValue)>()),
            )
            .with_owner(schema_owner()),
        );
        let scene = open_scene(&source, &basis);
        let prefab = open_prefab(&source, &basis);
        let mut request = scene
            .request_complete_prefab_conversion(scene.revision(), &id("anchor"), Default::default())
            .unwrap();
        let retained = request.checked_retained_bytes().unwrap();
        request.restrict_working_bytes(ByteLimit::new(1024 * 1024).unwrap());
        assert_eq!(request.checked_retained_bytes(), Some(retained));
        assert!(retained < 1024 * 1024);

        let state = scene.authority.published().unwrap();
        let projections = state.record_facts()[0].schema_declared_asset_references();
        assert_eq!(projections.len(), 1);
        assert_eq!(projections[0].asset_ref(), &AssetRef::path(&path).unwrap());
        assert!(
            source.entities[0].components[&component()]
                .value
                .get("image")
                .is_none()
        );
        let short_path = "textures/a.png";
        let short = open_scene(&source, &asset_basis(short_path));
        let short_retained = short
            .request_complete_prefab_conversion(short.revision(), &id("anchor"), Default::default())
            .unwrap()
            .checked_retained_bytes()
            .unwrap();
        assert_eq!(
            retained - short_retained,
            path.len() - short_path.len(),
            "only the copied default observation grows; raw source and shared catalogs are excluded",
        );

        let source = Arc::new(PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("leaf")).with_component(
                component(),
                source.entities[0].components[&component()].clone(),
            ),
        ]));
        let resolved = ResolvedPrefabSource::bound_authoring_source(source, basis, key());
        let prepared = std::thread::spawn(move || {
            assert_eq!(request.checked_retained_bytes(), Some(retained));
            request.prepare_with_loader(None, |_, _| Ok::<_, Infallible>(Some(resolved.clone())))
        })
        .join()
        .unwrap()
        .unwrap();
        assert_eq!(prepared.effective_asset_references().len(), 1);
        assert!(prepared.checked_retained_bytes().unwrap() > retained);

        let request = prefab
            .request_complete_prefab_conversion(
                prefab.revision(),
                &id("anchor"),
                Default::default(),
            )
            .unwrap();
        assert!(request.checked_retained_bytes().unwrap() < 1024 * 1024);
        let basis = prefab.authoring_schema_basis().unwrap();
        let resolved = ResolvedPrefabSource::bound_authoring_source(
            Arc::new(PrefabDocument::default()),
            basis,
            key(),
        );
        let prepared = request
            .prepare_with_loader(None, |_, _| Ok::<_, Infallible>(Some(resolved.clone())))
            .unwrap();
        assert!(prepared.checked_retained_bytes().is_some());
    }

    #[test]
    fn retained_classification_covers_unavailable_values_and_complete_dormant_declarations() {
        let basis = basis();
        let plain = open_scene(&document(instance("base")), &basis);
        let plain_bytes = plain
            .authority
            .published()
            .unwrap()
            .checked_retained_bytes()
            .unwrap();
        let mut overlay = PrefabOverlay::new(key());
        overlay.instances.push(PrefabInstanceClaim {
            target: PrefabEntityTarget::source(id(&"nested".repeat(64))),
            value: None,
        });
        let mut declaration = instance("base");
        declaration.overrides = Some(overlay);
        let mut source = document(declaration);
        let dormant = open_scene(&source, &basis);
        let state = dormant.authority.published().unwrap();
        assert!(state.unavailable_manifest().entries().is_empty());
        assert_eq!(
            state.unavailable_manifest().overlay_declarations().count(),
            1
        );
        assert!(state.checked_retained_bytes().unwrap() > plain_bytes + 384);

        let payload = "preserved".repeat(1024);
        source.entities[0].components.insert(
            ComponentTypeId::new("test.UnknownRetention"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    ("payload", ComponentValue::String(payload.clone())),
                    (
                        "reference",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: id("anchor"),
                        }),
                    ),
                ]),
            ),
        );
        let unavailable = open_scene(&source, &basis);
        let state = unavailable.authority.published().unwrap();
        assert_eq!(state.unavailable_manifest().record_count(), 1);
        assert!(
            state.checked_retained_bytes().unwrap()
                > dormant
                    .authority
                    .published()
                    .unwrap()
                    .checked_retained_bytes()
                    .unwrap()
                    + payload.len()
        );
        assert!(
            unavailable
                .request_complete_prefab_conversion(
                    unavailable.revision(),
                    &id("anchor"),
                    Default::default(),
                )
                .unwrap()
                .checked_retained_bytes()
                .is_some()
        );
    }

    #[test]
    fn retained_candidate_charges_suppressed_dependency_data_and_typed_review_mapping() {
        let basis = basis();
        let mut overlay = PrefabOverlay::new(key());
        overlay
            .suppressions
            .push(PrefabEntityTarget::source(id("discarded")));
        let mut declaration = instance("base");
        declaration.overrides = Some(overlay);
        let scene = open_scene(&document(declaration), &basis);
        let prepare = |length| -> PreparedScenePrefabConversion {
            let dependency = Arc::new(PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("leaf")),
                SceneEntityRecord::new(id("discarded")).with_component(
                    ComponentTypeId::new("test.SuppressedRetention"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::String("x".repeat(length)),
                    ),
                ),
            ]));
            let resolved =
                ResolvedPrefabSource::bound_authoring_source(dependency, basis.clone(), key());
            scene
                .request_complete_prefab_conversion(
                    scene.revision(),
                    &id("anchor"),
                    Default::default(),
                )
                .unwrap()
                .prepare_with_loader(None, |_: &AssetRef, _: &mut PrefabExpansionWorkingSet| {
                    Ok::<_, Infallible>(Some(resolved.clone()))
                })
                .unwrap()
        };
        let short = prepare(32);
        let long = prepare(64 * 1024);
        assert_eq!(short.entities().len(), 1);
        assert_eq!(long.entities().len(), 1);
        assert_eq!(
            long.entities()[0].provenance().local_entity_id(),
            &id("leaf")
        );
        let path = long.entities()[0].provenance().projection_path().unwrap();
        assert_eq!(path.root_anchor(), &id("anchor"));
        assert!(matches!(
            path.steps(),
            [PrefabProjectionStep::SourceEntity { .. }]
        ));
        assert!(review_map_bytes(long.entities()).unwrap() > size_of::<LocalizedPrefabEntity>());
        assert_eq!(
            long.checked_retained_bytes().unwrap() - short.checked_retained_bytes().unwrap(),
            64 * 1024 - 32,
            "discarded dependency data has no candidate record but remains retained for commit",
        );
    }

    #[test]
    fn retained_edit_counts_both_exact_directions_and_the_conversion_scope() {
        let basis = basis();
        let prepare = |length| {
            let mut old = instance("base");
            old.source =
                AssetRef::path(format!("prefabs/{}.prefab.json", "x".repeat(length))).unwrap();
            let scene = open_scene(&document(old.clone()), &basis);
            let patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
                entity: id("anchor"),
                expected: Some(old),
                value: Some(instance("next")),
            }]);
            scene
                .prepare_authoring_edit(scene.revision(), &patch)
                .unwrap()
        };
        let short = prepare(16);
        let long = prepare(1024);
        assert_eq!(short.document(), long.document());
        assert_eq!(
            long.checked_retained_bytes().unwrap() - short.checked_retained_bytes().unwrap(),
            2 * (1024 - 16),
            "the old declaration is independently retained by each exact patch direction",
        );

        let scene = open_scene(&document(instance("base")), &basis);
        let patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
            entity: id("anchor"),
            expected: Some(instance("base")),
            value: None,
        }]);
        let ordinary = scene
            .prepare_authoring_edit(scene.revision(), &patch)
            .unwrap();
        let footprint = super::super::PrefabConversionFootprint::new(
            &id("anchor"),
            &[],
            scene.history_limits().max_entry_logical_bytes(),
        )
        .unwrap();
        let retained_scope = footprint.retained_bytes();
        let conversion = scene
            .prepare_scoped_authoring_edit(
                scene.revision(),
                &patch,
                crate::authoring::AuthoringHistoryScope::PrefabConversion(footprint),
            )
            .unwrap();
        assert_eq!(ordinary.document(), conversion.document());
        assert_eq!(
            conversion.checked_retained_bytes().unwrap()
                - ordinary.checked_retained_bytes().unwrap(),
            retained_scope,
        );
    }

    #[test]
    fn published_dependency_classification_is_counted_beside_the_same_raw_body() {
        let basis = basis();
        let dependency = PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("leaf")),
            SceneEntityRecord::new(id("discarded")).with_component(
                ComponentTypeId::new("test.UnavailableDependency"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("x".repeat(64 * 1024)),
                ),
            ),
        ]);
        let published = Arc::new(
            PrefabDocumentCandidate::decode_json_str(&dependency.to_json_string().unwrap())
                .unwrap()
                .admit_authoring_with_basis(
                    AuthoringDocumentSource::new(
                        AssetRef::path("prefabs/base.prefab.json").unwrap(),
                    ),
                    &basis,
                )
                .unwrap()
                .publish(),
        );
        let raw = ResolvedPrefabSource::bound_authoring_source(
            Arc::new(dependency),
            basis.clone(),
            key(),
        );
        let published = ResolvedPrefabSource::bound_authoring(published, key());
        assert_eq!(raw.document(), published.document());
        assert_eq!(raw.checked_classification_retained_bytes(), Some(0));
        let classification = published.checked_classification_retained_bytes().unwrap();
        assert!(classification > 64 * 1024);
        let mut overlay = PrefabOverlay::new(key());
        overlay
            .suppressions
            .push(PrefabEntityTarget::source(id("discarded")));
        let mut declaration = instance("base");
        declaration.overrides = Some(overlay);
        let scene = open_scene(&document(declaration), &basis);
        let prepare = |resolved: &ResolvedPrefabSource| {
            scene
                .request_complete_prefab_conversion(
                    scene.revision(),
                    &id("anchor"),
                    Default::default(),
                )
                .unwrap()
                .prepare_with_loader(None, |_, _| Ok::<_, Infallible>(Some(resolved.clone())))
                .unwrap()
        };
        let raw = prepare(&raw);
        let published = prepare(&published);
        assert_eq!(raw.entities().len(), 1);
        assert_eq!(published.entities().len(), 1);
        assert_eq!(
            published.checked_retained_bytes().unwrap() - raw.checked_retained_bytes().unwrap(),
            classification,
        );
    }
}
