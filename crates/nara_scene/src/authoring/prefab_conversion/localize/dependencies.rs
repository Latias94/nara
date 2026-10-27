//! Actual demanded Prefab snapshots and their authored lookup bindings.

use std::{
    collections::{BTreeMap, HashMap},
    mem::size_of,
};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};

use super::PrefabConversionError;
use crate::{
    AuthoringSchemaBasis, PrefabDocument, ResolvedPrefabSource, SceneEntityId,
    ScenePatchApplyLimits, ScenePatchReport, SourceDocumentKey, SourceDocumentKind,
    authoring::history_error_report, patch::bounded_scene_entities_source_cost,
    prefab::PrefabExpansionWorkingSet, provenance::provenance_asset_ref_bytes,
    schema_authority::SchemaAuthority,
};

/// Retains each canonical body once, including dependencies with no surviving entities.
///
/// The logical model includes the container, snapshot handles, both retained canonical
/// keys, request bindings and index overhead. Body costs use the compiler's raw-copy
/// model: 512 bytes per structural work item, 96 per value node, content bytes and
/// root identifiers. Shared bodies are charged once here even though cloning their
/// immutable handles does not copy them. This is not an allocator or RSS bound;
/// loader caches, decoding and file/metadata evidence retain their existing owners.
#[derive(Debug)]
pub(in crate::authoring::prefab_conversion) struct CapturedPrefabSources {
    sources: BTreeMap<SourceDocumentKey, ResolvedPrefabSource>,
    requests: HashMap<AssetRef, SourceDocumentKey>,
    retained_bytes: usize,
    maximum: usize,
}

impl CapturedPrefabSources {
    pub(in crate::authoring::prefab_conversion) fn new(maximum: usize) -> Self {
        Self {
            sources: BTreeMap::new(),
            requests: HashMap::new(),
            retained_bytes: size_of::<Self>(),
            maximum,
        }
    }

    pub(in crate::authoring::prefab_conversion) const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    /// Admits borrowed content before comparing it recursively or retaining handles.
    pub(in crate::authoring::prefab_conversion) fn capture(
        &mut self,
        request: &AssetRef,
        source: &ResolvedPrefabSource,
        basis: &AuthoringSchemaBasis,
        working: &mut PrefabExpansionWorkingSet,
    ) -> Result<(), ScenePatchReport> {
        if source.key().kind() != SourceDocumentKind::Prefab
            || !source.matches_authority(SchemaAuthority::Authoring(basis))
        {
            return Err(super::refused(
                "Conversion dependencies require Prefab identity and the exact authoring basis",
            ));
        }
        let source_bytes = snapshot_bytes(source, self.maximum).ok_or_else(source_budget)?;
        let request_bytes = request_binding_bytes(request, source.key())
            .filter(|bytes| *bytes <= self.maximum)
            .ok_or_else(source_budget)?;
        let previous_request = self.requests.get(request);
        if previous_request.is_some_and(|key| key != source.key()) {
            return Err(super::refused(
                "A conversion dependency request changed its canonical binding during preparation",
            ));
        }
        let previous_source = self.sources.get(source.key());
        let retained_bytes = self
            .retained_bytes
            .checked_add(if previous_source.is_none() {
                source_bytes
            } else {
                0
            })
            .and_then(|bytes| {
                bytes.checked_add(if previous_request.is_none() {
                    request_bytes
                } else {
                    0
                })
            })
            .ok_or_else(source_budget)?;
        let additional = retained_bytes
            .checked_sub(self.retained_bytes)
            .ok_or_else(source_budget)?;
        let remaining = self
            .maximum
            .checked_sub(self.retained_bytes)
            .map(|bytes| bytes.min(working.working_byte_limit().get()))
            .and_then(|bytes| bytes.checked_sub(additional))
            .and_then(ByteLimit::new)
            .ok_or_else(source_budget)?;
        if working
            .checked_work_bytes()
            .is_none_or(|bytes| bytes > remaining.get())
        {
            return Err(source_budget());
        }
        if previous_source.is_some_and(|previous| {
            !previous.matches_authority(SchemaAuthority::Authoring(basis))
                || previous.document() != source.document()
        }) {
            return Err(super::refused(
                "A canonical conversion dependency changed its document or authority during preparation",
            ));
        }

        // The compiler enforces this reduced ceiling immediately when its loader resumes.
        working.restrict_working_bytes(remaining);
        if previous_source.is_none() {
            self.sources.insert(source.key().clone(), source.clone());
        }
        if previous_request.is_none() {
            self.requests.insert(request.clone(), source.key().clone());
        }
        self.retained_bytes = retained_bytes;
        Ok(())
    }

    /// Checks semantic snapshots only; the Host separately revalidates physical evidence.
    pub(in crate::authoring::prefab_conversion) fn validate<F, E>(
        &self,
        basis: &AuthoringSchemaBasis,
        mut loader: F,
    ) -> Result<(), PrefabConversionError<E>>
    where
        F: FnMut(&AssetRef) -> Result<Option<ResolvedPrefabSource>, E>,
    {
        for (request, key) in &self.requests {
            let previous = self
                .sources
                .get(key)
                .ok_or_else(|| PrefabConversionError::Source(source_changed()))?;
            if !previous.matches_authority(SchemaAuthority::Authoring(basis)) {
                return Err(PrefabConversionError::Source(source_changed()));
            }
            let current = loader(request)
                .map_err(PrefabConversionError::Loader)?
                .ok_or_else(|| PrefabConversionError::Source(source_changed()))?;

            // Admit the loader's returned body before derived equality, without copying
            // it into this capture. Its acquisition/decoding budget belongs to the loader.
            snapshot_bytes(&current, self.maximum)
                .ok_or_else(|| PrefabConversionError::Source(source_budget()))?;
            if current.key() != key
                || current.key().kind() != SourceDocumentKind::Prefab
                || !current.matches_authority(SchemaAuthority::Authoring(basis))
                || current.document() != previous.document()
            {
                return Err(PrefabConversionError::Source(source_changed()));
            }
        }
        Ok(())
    }
}

fn snapshot_bytes(source: &ResolvedPrefabSource, maximum: usize) -> Option<usize> {
    let document = source.document();
    let key_bytes = source.key().checked_retained_bytes()?;
    let root_headers = document
        .roots
        .len()
        .checked_mul(size_of::<SceneEntityId>().checked_add(2 * size_of::<usize>())?)?;
    let metadata = size_of::<ResolvedPrefabSource>()
        .checked_add(size_of::<PrefabDocument>())?
        .checked_add(6 * size_of::<usize>())?
        // One map key and the key retained inside the actual source snapshot.
        .checked_add(key_bytes.checked_mul(2)?)?
        .checked_add(root_headers)?;
    if metadata > maximum {
        return None;
    }
    let defaults = ScenePatchApplyLimits::default();
    let limits = defaults
        .with_validation_work(ItemLimit::new(
            maximum.min(defaults.validation_work().get()),
        )?)
        .with_validation_value_nodes(ItemLimit::new(
            maximum.min(defaults.validation_value_nodes().get()),
        )?)
        .with_validation_value_bytes(ByteLimit::new(
            maximum.min(defaults.validation_value_bytes().get()),
        )?);
    let cost = bounded_scene_entities_source_cost(&document.entities, limits)?;
    let bytes = metadata
        .checked_add(cost.operation_count().checked_mul(512)?)?
        .checked_add(cost.value_cost().nodes().checked_mul(96)?)?
        .checked_add(cost.logical_bytes())?;
    if bytes > maximum {
        return None;
    }
    document.roots.iter().try_fold(bytes, |bytes, root| {
        // The identifier/string headers were admitted above before visiting any roots.
        bytes
            .checked_add(root.as_str().len())
            .filter(|bytes| *bytes <= maximum)
    })
}

fn request_binding_bytes(request: &AssetRef, key: &SourceDocumentKey) -> Option<usize> {
    size_of::<AssetRef>()
        .checked_add(4 * size_of::<usize>())?
        .checked_add(provenance_asset_ref_bytes(request)?)?
        .checked_add(key.checked_retained_bytes()?)
}

pub(super) fn source_budget() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-prefab-conversion-source-budget-exceeded",
        "Conversion dependency snapshots and compiler work exceed their working byte allowance",
    )
}

fn source_changed() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-prefab-conversion-source-changed",
        "Conversion requires every captured dependency lookup, document and authoring basis to remain current",
    )
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, sync::Arc};

    use nara_reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
        ComponentValueMaterializationLimits, PersistentComponentSchemaOwnerId,
    };

    use super::*;
    use crate::{
        __private::AuthoringAdapterReceipt,
        PrefabExpansionOptions, PrefabInstance, SceneComponentRecord, SceneDocument,
        SceneEntityRecord,
        prefab::{SelectedInstanceSource, try_compile_selected_authoring_instance_with_loader},
        provenance::provenance_string_bytes,
    };

    const MAXIMUM: usize = 1024 * 1024;

    fn make_basis() -> AuthoringSchemaBasis {
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("test.conversion-dependencies").unwrap(),
            ComponentSchemaCatalog {
                components: vec![
                    ComponentSchema::new(
                        ComponentTypeId::new("test.ConversionDependency"),
                        "Dependency",
                        ComponentSchemaVersion::ONE,
                    )
                    .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("value"),
                        "Value",
                        ComponentFieldPath::from_fields(["value"]),
                        ComponentValueKind::U64,
                    )
                    .with_capability(ComponentCapability::Scene)
                    .with_capability(ComponentCapability::Inspect)]),
                ],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        let adapter =
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap();
        AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
    }

    fn key(name: &str) -> SourceDocumentKey {
        SourceDocumentKey::named(
            "e758bd18-2f06-4736-9752-dcfe038c3289".parse().unwrap(),
            name,
            SourceDocumentKind::Prefab,
        )
        .unwrap()
    }

    fn request(name: &str) -> AssetRef {
        AssetRef::path(format!("prefabs/{name}.prefab.json")).unwrap()
    }

    fn make_document(value: ComponentValue) -> Arc<PrefabDocument> {
        Arc::new(PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(SceneEntityId::new("root").unwrap()).with_component(
                ComponentTypeId::new("test.ConversionDependency"),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
            ),
        ]))
    }

    fn source(
        document: &Arc<PrefabDocument>,
        basis: &AuthoringSchemaBasis,
        name: &str,
    ) -> ResolvedPrefabSource {
        ResolvedPrefabSource::bound_authoring_source(Arc::clone(document), basis.clone(), key(name))
    }

    fn working(basis: &AuthoringSchemaBasis) -> PrefabExpansionWorkingSet {
        // Obtain a real suspended authoring compiler model without exposing its fields.
        let anchor = SceneEntityId::new("anchor").unwrap();
        let mut record = SceneEntityRecord::new(anchor.clone());
        record.prefab = Some(PrefabInstance {
            source: request("base"),
            overrides: None,
        });
        let scene = SceneDocument::new([anchor.clone()], [record]);
        let mut captured = None;
        try_compile_selected_authoring_instance_with_loader(
            SelectedInstanceSource::Scene(&scene),
            &anchor,
            basis,
            PrefabExpansionOptions::default(),
            None,
            |_, working| {
                captured = Some(*working);
                Ok::<_, Infallible>(None)
            },
        )
        .unwrap();
        captured.expect("the selected instance must suspend for its actual source")
    }

    fn is_budget(report: &ScenePatchReport) -> bool {
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-prefab-conversion-source-budget-exceeded"
        })
    }

    #[test]
    fn aliases_and_repeated_requests_retain_one_actual_canonical_snapshot() {
        let basis = make_basis();
        let document = make_document(ComponentValue::U64(7));
        let source = source(&document, &basis, "canonical");
        let mut capture = CapturedPrefabSources::new(MAXIMUM);
        let mut working = working(&basis);
        capture
            .capture(&request("path"), &source, &basis, &mut working)
            .unwrap();
        let first_bytes = capture.retained_bytes;
        let references = Arc::strong_count(&document);
        let alias = AssetRef::stable_id("5d76d4ee-71bf-4f76-ac5a-c860fc77a1ef").unwrap();
        let equal_body = make_document(ComponentValue::U64(7));
        let equal_source = ResolvedPrefabSource::bound_authoring_source(
            Arc::clone(&equal_body),
            basis.clone(),
            source.key().clone(),
        );
        capture
            .capture(&alias, &equal_source, &basis, &mut working)
            .unwrap();
        assert_eq!(
            capture.retained_bytes,
            first_bytes + request_binding_bytes(&alias, source.key()).unwrap()
        );
        let alias_bytes = capture.retained_bytes;
        capture
            .capture(&alias, &source, &basis, &mut working)
            .unwrap();
        assert_eq!(capture.sources.len(), 1);
        assert_eq!(capture.requests.len(), 2);
        assert_eq!(capture.retained_bytes, alias_bytes);
        assert_eq!(Arc::strong_count(&document), references);
        assert!(std::ptr::eq(
            capture.sources.get(source.key()).unwrap().document(),
            document.as_ref()
        ));
        assert_eq!(Arc::strong_count(&equal_body), 2);
    }

    #[test]
    fn preparation_rejects_changed_canonical_document_and_rebound_request() {
        let basis = make_basis();
        let first = source(&make_document(ComponentValue::U64(7)), &basis, "canonical");
        let changed = source(&make_document(ComponentValue::U64(8)), &basis, "canonical");
        let rebound = source(&make_document(ComponentValue::U64(7)), &basis, "other");
        let mut capture = CapturedPrefabSources::new(MAXIMUM);
        let mut working = working(&basis);
        capture
            .capture(&request("path"), &first, &basis, &mut working)
            .unwrap();
        let retained = capture.retained_bytes;
        assert!(
            capture
                .capture(&request("alias"), &changed, &basis, &mut working)
                .is_err()
        );
        assert!(
            capture
                .capture(&request("path"), &rebound, &basis, &mut working)
                .is_err()
        );
        assert_eq!(capture.retained_bytes, retained);
        assert_eq!(capture.sources.len(), 1);
        assert_eq!(capture.requests.len(), 1);
    }

    #[test]
    fn capture_requires_exact_authoring_admission_and_prefab_kind() {
        let first_basis = make_basis();
        let second_basis = make_basis();
        let document = make_document(ComponentValue::U64(7));
        let first = source(&document, &first_basis, "canonical");
        let second = source(&document, &second_basis, "canonical");
        let native = ResolvedPrefabSource::bound(Arc::clone(&document), key("canonical"));
        let wrong_kind = ResolvedPrefabSource::bound_authoring_source(
            Arc::clone(&document),
            first_basis.clone(),
            SourceDocumentKey::named(
                "e758bd18-2f06-4736-9752-dcfe038c3289".parse().unwrap(),
                "canonical",
                SourceDocumentKind::Scene,
            )
            .unwrap(),
        );
        let mut capture = CapturedPrefabSources::new(MAXIMUM);
        let mut working = working(&first_basis);
        for invalid in [&native, &second, &wrong_kind] {
            assert!(
                capture
                    .capture(&request("path"), invalid, &first_basis, &mut working)
                    .is_err()
            );
        }
        capture
            .capture(&request("path"), &first, &first_basis, &mut working)
            .unwrap();
        assert!(
            capture
                .capture(&request("alias"), &second, &second_basis, &mut working)
                .is_err()
        );
        assert_eq!(capture.sources.len(), 1);
        assert_eq!(capture.requests.len(), 1);
    }

    #[test]
    fn validation_reloads_every_alias_even_for_an_empty_dependency() {
        let basis = make_basis();
        let document = Arc::new(PrefabDocument::default());
        let source = source(&document, &basis, "empty");
        let mut capture = CapturedPrefabSources::new(MAXIMUM);
        let mut working = working(&basis);
        let requests = [request("path"), request("alias")];
        for request in &requests {
            capture
                .capture(request, &source, &basis, &mut working)
                .unwrap();
        }
        let mut reads = Vec::new();
        capture
            .validate(&basis, |request| {
                reads.push(request.clone());
                // Equal source content may be reacquired into another immutable allocation.
                Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
                    Arc::new(PrefabDocument::default()),
                    basis.clone(),
                    source.key().clone(),
                )))
            })
            .unwrap();
        assert_eq!(reads.len(), requests.len());
        assert!(requests.iter().all(|request| reads.contains(request)));
        assert!(matches!(
            capture.validate(&basis, |request| {
                Ok::<_, Infallible>((request != &requests[1]).then(|| source.clone()))
            }),
            Err(PrefabConversionError::Source(_))
        ));
    }

    #[test]
    fn validation_rejects_missing_rebound_changed_and_wrong_authority_sources() {
        let basis = make_basis();
        let document = make_document(ComponentValue::U64(7));
        let first = source(&document, &basis, "canonical");
        let changed = source(&make_document(ComponentValue::U64(8)), &basis, "canonical");
        let rebound = source(&document, &basis, "other");
        let other_basis = make_basis();
        let other_authority = source(&document, &other_basis, "canonical");
        let native = ResolvedPrefabSource::bound(Arc::clone(&document), key("canonical"));
        let mut capture = CapturedPrefabSources::new(MAXIMUM);
        let mut working = working(&basis);
        capture
            .capture(&request("path"), &first, &basis, &mut working)
            .unwrap();
        for current in [
            None,
            Some(rebound),
            Some(changed),
            Some(other_authority),
            Some(native),
        ] {
            assert!(matches!(
                capture.validate(&basis, |_| Ok::<_, Infallible>(current.clone())),
                Err(PrefabConversionError::Source(_))
            ));
        }
        assert!(matches!(
            capture.validate(&other_basis, |_| Ok::<_, Infallible>(Some(first.clone()))),
            Err(PrefabConversionError::Source(_))
        ));
    }

    #[test]
    fn validation_preserves_the_concrete_loader_error() {
        #[derive(Debug, PartialEq)]
        struct LoadFailure(u32);

        let basis = make_basis();
        let source = source(&make_document(ComponentValue::U64(7)), &basis, "canonical");
        let mut capture = CapturedPrefabSources::new(MAXIMUM);
        capture
            .capture(&request("path"), &source, &basis, &mut working(&basis))
            .unwrap();
        assert!(matches!(
            capture.validate(&basis, |_| Err(LoadFailure(73))),
            Err(PrefabConversionError::Loader(LoadFailure(73)))
        ));
    }

    #[test]
    fn source_budget_precedes_retention_and_zero_remaining_is_a_budget_error() {
        let basis = make_basis();
        let document = make_document(ComponentValue::U64(7));
        let source = source(&document, &basis, "canonical");
        let request = request("path");
        let exact_retained = size_of::<CapturedPrefabSources>()
            + snapshot_bytes(&source, MAXIMUM).unwrap()
            + request_binding_bytes(&request, source.key()).unwrap();
        let mut working = working(&basis);
        let references = Arc::strong_count(&document);
        for maximum in [
            0,
            exact_retained,
            exact_retained + working.checked_work_bytes().unwrap() - 1,
        ] {
            let mut capture = CapturedPrefabSources::new(maximum);
            let error = capture
                .capture(&request, &source, &basis, &mut working)
                .unwrap_err();
            assert!(is_budget(&error));
            assert!(capture.sources.is_empty());
            assert!(capture.requests.is_empty());
            assert_eq!(capture.retained_bytes, size_of::<CapturedPrefabSources>());
            assert_eq!(Arc::strong_count(&document), references);
        }
    }

    #[test]
    fn loader_reservation_and_dependency_retention_share_the_remaining_allowance() {
        let basis = make_basis();
        let document = make_document(ComponentValue::U64(7));
        let source = source(&document, &basis, "canonical");
        let mut capture = CapturedPrefabSources::new(MAXIMUM);
        let mut working = working(&basis);
        let used = working.checked_work_bytes().unwrap();
        working.restrict_working_bytes(ByteLimit::new(used).unwrap());
        let references = Arc::strong_count(&document);
        let error = capture
            .capture(&request("path"), &source, &basis, &mut working)
            .unwrap_err();
        assert!(is_budget(&error));
        assert!(capture.sources.is_empty());
        assert!(capture.requests.is_empty());
        assert_eq!(Arc::strong_count(&document), references);
    }

    #[test]
    fn successive_loader_growth_and_alias_retention_preserve_both_debits() {
        let basis = make_basis();
        let source = source(&make_document(ComponentValue::U64(7)), &basis, "canonical");
        let mut capture = CapturedPrefabSources::new(MAXIMUM);
        let mut working = working(&basis);
        working.restrict_working_bytes(ByteLimit::new(MAXIMUM - capture.retained_bytes()).unwrap());
        // The second loader grows by only one byte, less than the retained first snapshot.
        // A min of independent absolute ceilings would hide that growth.
        for (name, loader_retained) in [("path", 4096), ("alias", 4097), ("alias", 4097)] {
            let mut loading = working
                .including_retained_working_bytes(capture.retained_bytes())
                .unwrap();
            assert_eq!(
                loading.checked_work_bytes(),
                working
                    .checked_work_bytes()
                    .unwrap()
                    .checked_add(capture.retained_bytes()),
            );
            loading.restrict_working_bytes(ByteLimit::new(MAXIMUM - loader_retained).unwrap());
            working.restrict_working_bytes(
                ByteLimit::new(loading.working_byte_limit().get() - capture.retained_bytes())
                    .unwrap(),
            );
            capture
                .capture(&request(name), &source, &basis, &mut working)
                .unwrap();
            assert_eq!(
                working.working_byte_limit().get(),
                MAXIMUM - loader_retained - capture.retained_bytes(),
            );
        }
        assert_eq!(capture.sources.len(), 1);
        assert_eq!(capture.requests.len(), 2);
    }

    #[test]
    fn changed_oversized_documents_are_admitted_before_recursive_equality() {
        let basis = make_basis();
        let first = source(&make_document(ComponentValue::U64(7)), &basis, "canonical");
        let oversized = source(
            &make_document(ComponentValue::String("x".repeat(MAXIMUM))),
            &basis,
            "canonical",
        );
        let mut capture = CapturedPrefabSources::new(MAXIMUM);
        let mut working = working(&basis);
        capture
            .capture(&request("path"), &first, &basis, &mut working)
            .unwrap();
        assert!(is_budget(
            &capture
                .capture(&request("alias"), &oversized, &basis, &mut working)
                .unwrap_err()
        ));
        let Err(PrefabConversionError::Source(report)) =
            capture.validate(&basis, |_| Ok::<_, Infallible>(Some(oversized.clone())))
        else {
            panic!("oversized revalidation must produce a source budget error");
        };
        assert!(is_budget(&report));
        assert_eq!(capture.sources.len(), 1);
        assert_eq!(capture.requests.len(), 1);
    }

    #[test]
    fn deeply_nested_current_values_are_rejected_before_recursive_equality() {
        let basis = make_basis();
        let first = source(&make_document(ComponentValue::Null), &basis, "canonical");
        let mut value = ComponentValue::Null;
        for _ in 0..=ComponentValueMaterializationLimits::default().depth().get() {
            value = ComponentValue::List(vec![value]);
        }
        let deep = source(&make_document(value), &basis, "canonical");
        let mut capture = CapturedPrefabSources::new(MAXIMUM);
        let mut working = working(&basis);
        capture
            .capture(&request("path"), &first, &basis, &mut working)
            .unwrap();
        assert!(is_budget(
            &capture
                .capture(&request("alias"), &deep, &basis, &mut working)
                .unwrap_err()
        ));
        let Err(PrefabConversionError::Source(report)) =
            capture.validate(&basis, |_| Ok::<_, Infallible>(Some(deep.clone())))
        else {
            panic!("deep revalidation must produce a source budget error");
        };
        assert!(is_budget(&report));
    }

    #[test]
    fn root_copies_and_value_nodes_contribute_to_the_snapshot_model() {
        let basis = make_basis();
        let empty = source(&Arc::new(PrefabDocument::default()), &basis, "canonical");
        let root = SceneEntityId::new("root").unwrap();
        let rooted = source(
            &Arc::new(PrefabDocument::new([root.clone()], [])),
            &basis,
            "canonical",
        );
        assert_eq!(
            snapshot_bytes(&rooted, MAXIMUM).unwrap() - snapshot_bytes(&empty, MAXIMUM).unwrap(),
            size_of::<SceneEntityId>() + provenance_string_bytes(root.as_str()).unwrap()
        );
        let scalar = source(&make_document(ComponentValue::Null), &basis, "canonical");
        let list = source(
            &make_document(ComponentValue::List(vec![ComponentValue::Null; 4])),
            &basis,
            "canonical",
        );
        assert_eq!(
            snapshot_bytes(&list, MAXIMUM).unwrap() - snapshot_bytes(&scalar, MAXIMUM).unwrap(),
            4 * 96
        );
    }
}
