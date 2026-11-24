//! Validates the paired replacement generation and its affected saved dependants.

use nara_asset::{AssetProductRef, ImportedProductId};
use nara_core::ItemLimit;
use nara_scene::{
    __private::{
        SourceDocumentInput, classify_component_apply_dependants,
        try_compile_authoring_scene_with_loader, try_validate_authoring_saved_prefab_with_loader,
    },
    SceneCompilationFailure, SceneCompilationFailureKind, SceneCompilationOptions,
    SceneCompilationWorkingSet, SourceDocumentKey, SourceDocumentKind,
};

use super::super::source_inventory::{
    SourceEncoding, SourceInventory, SourceInventoryError, with_temporary_storage,
};
use super::{catalogue::ReferenceCatalogue, *};

#[cfg(feature = "tooling")]
mod live;
mod physical;
mod publication;
#[cfg(feature = "tooling")]
mod ready;
mod recovery_data;
mod resume;
pub use resume::{
    PrefabComponentApplyRecovery, PrefabComponentApplyRecoveryError,
    PrefabComponentApplyRecoveryErrorKind, PrefabComponentApplyRecoveryInspectionFailure,
    PrefabComponentApplyRecoveryOutput, PrefabComponentApplyRecoveryPhase,
    PrefabComponentApplyRecoveryReceipt, PrefabComponentApplyRecoveryReview,
    PrefabComponentApplyRecoveryView,
};
#[cfg(feature = "tooling")]
mod review;
#[cfg(all(test, feature = "tooling", any(windows, target_os = "linux")))]
mod tests;
#[cfg(feature = "tooling")]
pub(crate) use live::{
    ComponentApplyLiveError, ComponentApplyLivePublication, ComponentApplyPersistenceDestination,
};
#[cfg(feature = "tooling")]
pub(crate) use review::{ComponentApplyProjectReview, ComponentApplyProjectReviewError};

pub(super) enum ComponentApplySourceBody {
    Scene(Arc<SceneDocument>),
    Prefab(Arc<PrefabDocument>),
}

impl ComponentApplySourceBody {
    fn kind(&self) -> SourceDocumentKind {
        match self {
            Self::Scene(_) => SourceDocumentKind::Scene,
            Self::Prefab(_) => SourceDocumentKind::Prefab,
        }
    }
}

pub(super) struct ComponentApplyReplacement<'a> {
    pub(super) path: &'a str,
    pub(super) key: &'a SourceDocumentKey,
    pub(super) body: ComponentApplySourceBody,
}

enum SavedSourceBody {
    Scene(Arc<SceneDocument>),
    Prefab(ResolvedPrefabSource),
}

struct SavedSource<'a> {
    path: &'a str,
    body: SavedSourceBody,
}

impl SavedSource<'_> {
    fn document(&self) -> SourceDocumentInput<'_> {
        match &self.body {
            SavedSourceBody::Scene(document) => SourceDocumentInput::Scene(document),
            SavedSourceBody::Prefab(source) => SourceDocumentInput::Prefab(source.document()),
        }
    }
}

struct SavedSources<'a> {
    entries: Vec<SavedSource<'a>>,
    replacements: [usize; 2],
}

struct SavedEnvironment<'a> {
    profile: Option<&'a str>,
    catalogue: ReferenceCatalogue,
    members: Vec<usize>,
    aliases: HashMap<AssetRef, usize>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ComponentApplySavedGenerationError {
    #[error("the paired component Apply source differs from the admitted inventory")]
    SourceMismatch,
    #[error("the saved project inventory could not be admitted")]
    Inventory(#[from] SourceInventoryError),
    #[error("the saved project source could not be decoded")]
    Decode(#[from] SourceDecodeError),
    #[error("saved project resolution could not be admitted")]
    Catalogue(#[from] AuthoringProjectSourceAdmissionError),
    #[error("the saved generation exceeded the operation allowance")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("the affected source footprint could not be admitted: {0:?}")]
    Footprint(SceneCompilationFailureKind),
    #[error("the saved generation exceeded the reference traversal allowance")]
    ReferenceWorkLimit,
    #[error("the saved source {path} could not compose in configuration {profile:?}")]
    Composition {
        path: String,
        profile: Option<String>,
        failure: Option<Box<SceneCompilationFailure>>,
        diagnostics: DiagnosticReport,
    },
}

/// The caller retains the original inventory and two verified output allocations. Other open
/// documents contribute the same disk bytes as closed members. Fixed catalogue membership
/// allows each configuration to validate the replacements and their shared reverse closure.
pub(super) fn validate_component_apply_saved_generation(
    root: &DirectoryCapability,
    inventory: &SourceInventory,
    basis: &AuthoringSchemaBasis,
    replacements: &[ComponentApplyReplacement<'_>; 2],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), ComponentApplySavedGenerationError> {
    with_temporary_storage(budget, |budget| {
        let saved = capture_saved_sources(inventory, basis, replacements, budget)?;
        validate_saved_sources(
            root,
            inventory,
            basis,
            &saved,
            None,
            &mut SavedGenerationWork::default(),
            budget,
        )
    })
}

fn validate_saved_sources(
    root: &DirectoryCapability,
    inventory: &SourceInventory,
    basis: &AuthoringSchemaBasis,
    saved: &SavedSources<'_>,
    validated: Option<usize>,
    work: &mut SavedGenerationWork,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), ComponentApplySavedGenerationError> {
    for index in 0..inventory.configurations().len() {
        if validated == Some(index) {
            continue;
        }
        with_temporary_storage(budget, |budget| {
            SavedEnvironment::capture(root, inventory, index, saved, budget)?
                .validate(saved, basis, work, budget)
        })?;
    }
    Ok(())
}

fn capture_saved_sources<'a>(
    inventory: &'a SourceInventory,
    basis: &AuthoringSchemaBasis,
    replacements: &[ComponentApplyReplacement<'_>; 2],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<SavedSources<'a>, ComponentApplySavedGenerationError> {
    use ComponentApplySavedGenerationError::SourceMismatch;
    if replacements[0].path == replacements[1].path
        || replacements[0].key == replacements[1].key
        || replacements[1].body.kind() != SourceDocumentKind::Prefab
    {
        return Err(SourceMismatch);
    }
    for replacement in replacements {
        let member = inventory
            .participant(replacement.path)
            .ok_or(SourceMismatch)?;
        if member.source_key() != replacement.key || member.kind() != replacement.body.kind() {
            return Err(SourceMismatch);
        }
    }
    let members = inventory.participants();
    let storage = checked_product(
        members.len(),
        size_of::<SavedSource>() + 4 * size_of::<usize>(),
    )?;
    budget.charge_source_bytes(0, 0, storage, storage)?;
    let entries = members
        .map(|member| {
            let path = member.file().path();
            let body = if let Some(replacement) = replacements
                .iter()
                .find(|replacement| replacement.path == path)
            {
                match &replacement.body {
                    ComponentApplySourceBody::Scene(document) => {
                        ComponentApplySourceBody::Scene(Arc::clone(document))
                    }
                    ComponentApplySourceBody::Prefab(document) => {
                        ComponentApplySourceBody::Prefab(Arc::clone(document))
                    }
                }
            } else {
                let encoding = match member.encoding() {
                    SourceEncoding::Json => SceneFileEncoding::Json,
                    SourceEncoding::Ron => SceneFileEncoding::Ron,
                };
                match decode_source(
                    member.kind(),
                    encoding,
                    member.bytes(),
                    inventory.scene_limits(),
                    budget,
                )? {
                    SourceTree::Scene(document) => {
                        ComponentApplySourceBody::Scene(Arc::new(document))
                    }
                    SourceTree::Prefab(document) => {
                        ComponentApplySourceBody::Prefab(Arc::new(document))
                    }
                }
            };
            let body = match body {
                ComponentApplySourceBody::Scene(document) => SavedSourceBody::Scene(document),
                ComponentApplySourceBody::Prefab(document) => {
                    SavedSourceBody::Prefab(saved_prefab(
                        inventory,
                        basis,
                        path,
                        member.source_key(),
                        document,
                        budget,
                    )?)
                }
            };
            Ok::<_, ComponentApplySavedGenerationError>(SavedSource { path, body })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let slots = [
        saved_source_index(&entries, replacements[0].path)?,
        saved_source_index(&entries, replacements[1].path)?,
    ];
    Ok(SavedSources {
        entries,
        replacements: slots,
    })
}

fn saved_prefab(
    inventory: &SourceInventory,
    basis: &AuthoringSchemaBasis,
    path: &str,
    key: &SourceDocumentKey,
    document: Arc<PrefabDocument>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<ResolvedPrefabSource, ComponentApplySavedGenerationError> {
    budget.charge_source_bytes(0, 0, 0, key.retained_bytes())?;
    let source = ResolvedPrefabSource::bound_authoring_source(document, basis.clone(), key.clone());
    match inventory.metadata_for_source(path) {
        Some(metadata) => source
            .with_source_asset(metadata.record())
            .map_err(|_| ComponentApplySavedGenerationError::SourceMismatch),
        None => Ok(source),
    }
}

fn saved_source_index(
    sources: &[SavedSource<'_>],
    path: &str,
) -> Result<usize, ComponentApplySavedGenerationError> {
    sources
        .binary_search_by_key(&path, |entry| entry.path)
        .map_err(|_| ComponentApplySavedGenerationError::SourceMismatch)
}

impl<'a> SavedEnvironment<'a> {
    fn capture(
        root: &DirectoryCapability,
        inventory: &'a SourceInventory,
        index: usize,
        sources: &SavedSources<'_>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, ComponentApplySavedGenerationError> {
        use ComponentApplySavedGenerationError::SourceMismatch;
        let policy = &inventory.configurations()[index];
        let mut catalogue =
            ReferenceCatalogue::from_inventory(root.share(), inventory, index, budget)?;
        let paths = policy.participant_paths();
        let bytes = checked_product(paths.len(), size_of::<usize>())?;
        budget.charge_source_bytes(0, 0, bytes, bytes)?;
        let mut members = Vec::with_capacity(paths.len());
        let mut aliases = HashMap::new();
        for (slot, path) in paths.enumerate() {
            let index = saved_source_index(&sources.entries, path)?;
            members.push(index);
            if !matches!(&sources.entries[index].body, SavedSourceBody::Prefab(_)) {
                continue;
            }
            let canonical = AssetSourcePath::new(path).map_err(|_| SourceMismatch)?;
            let prefabs = catalogue.prefabs();
            let relative = prefabs.scope().relative_path(&canonical);
            let stable_id = catalogue
                .database()
                .record_for_path(&canonical)
                .map(AssetRecord::stable_id);
            let mut add = |reference: AssetRef| {
                if !catalogue.admits_prefab_alias(&reference, &canonical, budget)? {
                    return Ok(());
                }
                let bytes =
                    checked_sum([size_of::<(AssetRef, usize)>() + 64, alias_bytes(&reference)])?;
                budget.charge_source_bytes(0, 0, bytes, bytes)?;
                if aliases.insert(reference, slot).is_some() {
                    return Err(SourceMismatch);
                }
                Ok::<_, ComponentApplySavedGenerationError>(())
            };
            if let Some(relative) = relative {
                add(AssetRef::path(relative).map_err(|_| SourceMismatch)?)?;
            }
            if let Some(id) = stable_id {
                add(AssetRef::StableId(id))?;
                let product = AssetProductRef::new(id, ImportedProductId::PRIMARY)
                    .map_err(|_| SourceMismatch)?;
                add(AssetRef::Product(product))?;
            }
        }
        Ok(Self {
            profile: policy.profile_name(),
            catalogue,
            members,
            aliases,
        })
    }

    fn load(
        &self,
        sources: &SavedSources<'_>,
        reference: &AssetRef,
        working: &mut SceneCompilationWorkingSet,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Option<ResolvedPrefabSource>, AuthoringProjectAdmissionError> {
        budget.charge_source_bytes(0, 0, alias_bytes(reference), 0)?;
        working.restrict_working_bytes(remaining_saved_host_work(budget)?);
        let Some(&slot) = self.aliases.get(reference) else {
            return Ok(None);
        };
        let SavedSourceBody::Prefab(source) = &sources.entries[self.members[slot]].body else {
            unreachable!("only admitted Prefab members have aliases")
        };
        Ok(Some(source.clone()))
    }

    fn validate(
        &self,
        sources: &SavedSources<'_>,
        basis: &AuthoringSchemaBasis,
        work: &mut SavedGenerationWork,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), ComponentApplySavedGenerationError> {
        let replacements = sources
            .replacements
            .map(|index| self.members.binary_search(&index).ok());
        if replacements == [None, None] {
            return Ok(());
        }
        let footprint = classify_component_apply_dependants(
            self.members.len(),
            replacements,
            work.options(budget)?.limits,
            |slot| {
                self.members
                    .get(slot)
                    .map(|&index| sources.entries[index].document())
            },
            |reference| self.aliases.get(reference).copied(),
        )
        .map_err(ComponentApplySavedGenerationError::Footprint)?;
        work.settle(budget, Some(footprint.work_bytes), footprint.reference_work)?;
        budget.charge_source_bytes(0, 0, 0, footprint.retained_bytes())?;
        let assets = self.catalogue.assets();
        let resolver = self.catalogue.database().resolver(assets.scope());
        for (slot, &index) in self.members.iter().enumerate() {
            if footprint.affects(slot) {
                validate_saved_source(
                    &sources.entries[index],
                    self.profile,
                    basis,
                    resolver,
                    |reference, working, budget| self.load(sources, reference, working, budget),
                    work,
                    budget,
                )?;
            }
        }
        Ok(())
    }
}

fn validate_saved_source(
    source: &SavedSource<'_>,
    profile: Option<&str>,
    basis: &AuthoringSchemaBasis,
    assets: AssetReferenceResolver<'_>,
    mut load: impl FnMut(
        &AssetRef,
        &mut SceneCompilationWorkingSet,
        &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Option<ResolvedPrefabSource>, AuthoringProjectAdmissionError>,
    work: &mut SavedGenerationWork,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), ComponentApplySavedGenerationError> {
    let options = work.options(budget)?;
    let mut load = |reference: &AssetRef, working: &mut SceneCompilationWorkingSet| {
        load(reference, working, budget)
    };
    let (failed, diagnostics, failure, bytes, references) = match &source.body {
        SavedSourceBody::Scene(document) => {
            let report = try_compile_authoring_scene_with_loader(
                document,
                basis,
                options,
                Some(assets),
                &mut load,
            )?;
            let bytes = report.checked_work_bytes();
            let references = report.reference_work();
            let parts = report.into_parts();
            (
                parts.candidate.is_none(),
                parts.diagnostics,
                parts.failure,
                bytes,
                references,
            )
        }
        SavedSourceBody::Prefab(resolved) => {
            let report = try_validate_authoring_saved_prefab_with_loader(
                resolved.document(),
                basis,
                options,
                &mut load,
            )?;
            (
                report.failure.is_some(),
                report.diagnostics,
                report.failure,
                report.work_bytes,
                report.reference_work,
            )
        }
    };
    work.settle(budget, bytes, references)?;
    if failed || diagnostics.has_errors() {
        return Err(ComponentApplySavedGenerationError::Composition {
            path: source.path.to_owned(),
            profile: profile.map(str::to_owned),
            failure: failure.map(Box::new),
            diagnostics,
        });
    }
    Ok(())
}

#[derive(Default)]
struct SavedGenerationWork {
    reference_nodes: usize,
    reference_bytes: usize,
}

impl SavedGenerationWork {
    fn options(
        &self,
        budget: &AuthoringProjectAdmissionBudget,
    ) -> Result<SceneCompilationOptions, ComponentApplySavedGenerationError> {
        use ComponentApplySavedGenerationError::ReferenceWorkLimit;
        let default = SceneCompilationOptions::default();
        let limits = default.limits;
        let working = remaining_compiler_work(budget, 0, limits.working_bytes())?
            .min(remaining_saved_host_work(budget)?);
        let nodes = limits
            .reference_work_nodes()
            .get()
            .saturating_sub(self.reference_nodes);
        let bytes = limits
            .reference_work_bytes()
            .get()
            .saturating_sub(self.reference_bytes);
        Ok(default.with_limits(
            limits
                .with_working_bytes(working)
                .with_reference_work_nodes(ItemLimit::new(nodes).ok_or(ReferenceWorkLimit)?)
                .with_reference_work_bytes(ByteLimit::new(bytes).ok_or(ReferenceWorkLimit)?),
        ))
    }

    fn settle(
        &mut self,
        budget: &mut AuthoringProjectAdmissionBudget,
        bytes: Option<usize>,
        reference_work: (usize, usize),
    ) -> Result<(), ComponentApplySavedGenerationError> {
        budget.charge_source_bytes(
            0,
            0,
            bytes.ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
            0,
        )?;
        let limits = SceneCompilationOptions::default().limits;
        self.reference_nodes = self.reference_nodes.saturating_add(reference_work.0);
        self.reference_bytes = self.reference_bytes.saturating_add(reference_work.1);
        if self.reference_nodes > limits.reference_work_nodes().get()
            || self.reference_bytes > limits.reference_work_bytes().get()
        {
            return Err(ComponentApplySavedGenerationError::ReferenceWorkLimit);
        }
        Ok(())
    }
}

fn remaining_saved_host_work(
    budget: &AuthoringProjectAdmissionBudget,
) -> Result<ByteLimit, AuthoringProjectAdmissionError> {
    let maximum = budget.limits.decode_work().get();
    ByteLimit::new(maximum.saturating_sub(budget.decode_work))
        .ok_or(AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum })
}

fn alias_bytes(reference: &AssetRef) -> usize {
    match reference {
        AssetRef::Path(path) => path.as_str().len(),
        _ => size_of::<AssetRef>(),
    }
}
