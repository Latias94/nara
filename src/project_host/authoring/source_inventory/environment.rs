//! Retained source configuration facts, independent of any particular authoring operation.

use std::{collections::BTreeSet, fmt};

use nara_core::SourceUniverseId;
use nara_project::{NativeDocumentIdentity, ProjectPath};

use super::*;

/// One actual resolved default or named configuration. Physical source identity stays in the
/// inventory's unique participant table; these paths describe its interpretation contexts.
pub(in crate::project_host::authoring) struct ConfigurationPolicy {
    pub(super) native: NativeDocumentSourcePolicy,
    pub(super) native_scopes: [String; 4],
    pub(super) catalogue_bindings: Vec<SourceDirectoryBinding>,
    pub(super) participant_paths: BTreeSet<String>,
    profile_name: Option<String>,
    native_identity: NativeDocumentIdentity,
    source_universe: SourceUniverseId,
}

impl ConfigurationPolicy {
    pub(super) fn new(
        settings: &EffectiveProjectSettings,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceInventoryError> {
        let paths = [
            &settings.paths.assets,
            &settings.paths.scenes,
            &settings.paths.prefabs,
            &settings.paths.scripts,
        ];
        let path_bytes = paths
            .iter()
            .try_fold(0_usize, |total, path| sum(total, path.as_str().len()))?;
        // The configuration table's inline slots are admitted before resolution. Admit all
        // copied strings, the policy's paths and the catalogue owner's Vec/Arc allocations
        // before invoking their existing constructors. Unrelated runtime settings stay borrowed.
        let bytes = sum(
            settings.profile_name.as_deref().map_or(0, str::len),
            sum(
                product(path_bytes, 5)?,
                sum(
                    settings.paths.import_cache.as_str().len(),
                    product(
                        3,
                        size_of::<SourceDirectoryBinding>()
                            + size_of::<ProjectPath>()
                            + size_of::<AssetPathScope>()
                            + 4 * size_of::<usize>(),
                    )?,
                )?,
            )?,
        )?;
        reserve(budget, bytes)?;
        Ok(Self {
            native: NativeDocumentSourcePolicy::new(settings),
            native_scopes: paths.map(|path| path.as_str().to_owned()),
            catalogue_bindings: project_reference_bindings(settings),
            participant_paths: BTreeSet::new(),
            profile_name: settings.profile_name.clone(),
            native_identity: settings.project.native_document_identity,
            source_universe: settings.project.source_universe,
        })
    }

    /// None identifies the independently resolved default configuration.
    pub(in crate::project_host::authoring) fn profile_name(&self) -> Option<&str> {
        self.profile_name.as_deref()
    }

    pub(in crate::project_host::authoring) fn catalogue_bindings(
        &self,
    ) -> &[SourceDirectoryBinding] {
        &self.catalogue_bindings
    }

    /// Assets, Scenes, Prefabs and Scripts, in the native policy owner's scope order.
    pub(in crate::project_host::authoring) fn native_scopes(
        &self,
    ) -> impl ExactSizeIterator<Item = &str> {
        self.native_scopes.iter().map(String::as_str)
    }

    pub(in crate::project_host::authoring) fn native_policy(&self) -> &NativeDocumentSourcePolicy {
        &self.native
    }

    pub(in crate::project_host::authoring) fn participant_paths(
        &self,
    ) -> impl ExactSizeIterator<Item = &str> {
        self.participant_paths.iter().map(String::as_str)
    }

    pub(super) fn uses_stable_metadata(&self) -> bool {
        self.native_identity == NativeDocumentIdentity::StableMetadata
    }

    pub(super) fn include_participant(
        &mut self,
        path: &str,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceInventoryError> {
        reserve(
            budget,
            sum(MAP_NODE_BYTES + size_of::<String>(), path.len())?,
        )?;
        self.participant_paths.insert(path.to_owned());
        Ok(())
    }

    pub(super) fn same_definition(&self, other: &Self) -> bool {
        // These are exactly the source-relevant inputs of NativeDocumentSourcePolicy::new.
        // Import-cache/runtime settings do not affect native source admission or visibility.
        self.profile_name == other.profile_name
            && self.native_identity == other.native_identity
            && self.source_universe == other.source_universe
            && self.native_scopes == other.native_scopes
            && self.catalogue_bindings == other.catalogue_bindings
    }

    pub(super) fn definition_comparison_work(&self) -> Result<usize, SourceInventoryError> {
        let mut bytes = sum(size_of::<Self>(), self.profile_name().map_or(0, str::len))?;
        for path in &self.native_scopes {
            bytes = sum(bytes, path.len())?;
        }
        for binding in &self.catalogue_bindings {
            bytes = sum(bytes, size_of::<SourceDirectoryBinding>())?;
            bytes = sum(bytes, binding.directory().as_str().len())?;
            bytes = sum(
                bytes,
                binding
                    .scope()
                    .prefix()
                    .map_or(0, |path| path.as_str().len()),
            )?;
        }
        Ok(bytes)
    }

    pub(super) fn comparison_work(&self) -> Result<usize, SourceInventoryError> {
        let mut bytes = self.definition_comparison_work()?;
        for path in &self.participant_paths {
            bytes = sum(bytes, sum(size_of::<String>(), path.len())?)?;
        }
        Ok(bytes)
    }
}

impl PartialEq for ConfigurationPolicy {
    fn eq(&self, other: &Self) -> bool {
        self.same_definition(other) && self.participant_paths == other.participant_paths
    }
}

impl Eq for ConfigurationPolicy {}

impl fmt::Debug for ConfigurationPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigurationPolicy")
            .field("profile_name", &self.profile_name)
            .field("native_identity", &self.native_identity)
            .field("source_universe", &self.source_universe)
            .field("native_scopes", &self.native_scopes)
            .field("catalogue_bindings", &self.catalogue_bindings)
            .field("participant_paths", &self.participant_paths)
            .finish_non_exhaustive()
    }
}

/// An actual sidecar declaration's admission in one configuration, retaining its precise alias
/// scope. A native-only Scripts admission does not become a catalogue binding.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::project_host::authoring) struct ConfigurationRecordPolicy {
    pub(super) configuration: usize,
    pub(super) scope: AssetPathScope,
    pub(super) catalogue_binding: Option<usize>,
}

impl ConfigurationRecordPolicy {
    /// Index into this inventory's configuration table, never a persistent identity.
    pub(in crate::project_host::authoring) const fn configuration_index(&self) -> usize {
        self.configuration
    }

    pub(in crate::project_host::authoring) const fn scope(&self) -> &AssetPathScope {
        &self.scope
    }

    /// Index into the configuration's actual catalogue bindings; None is native-only visibility.
    pub(in crate::project_host::authoring) const fn catalogue_binding_index(
        &self,
    ) -> Option<usize> {
        self.catalogue_binding
    }
}

impl InventoryCapture<'_> {
    pub(super) fn admit_participant_contexts(
        &mut self,
        path: &str,
        kind: SourceDocumentKind,
        identity: FileIdentity,
    ) -> Result<AdmittedNativeDocumentSource, SourceInventoryError> {
        let open = self.open_sources.contains_key(path);
        let external = open || self.explicit_native_claim(path)?;
        let selected_profile = self
            .input
            .selected_settings
            .and_then(|settings| settings.profile_name.as_deref());
        let mut selected = 0;
        for (index, policy) in self.policies.iter().enumerate() {
            self.budget.charge_source_bytes(
                0,
                0,
                sum(
                    policy.profile_name().map_or(0, str::len),
                    selected_profile.map_or(0, str::len),
                )?,
                0,
            )?;
            if policy.profile_name() == selected_profile {
                selected = index;
                break;
            }
        }
        let owner = self
            .metadata
            .get(path)
            .map_or(selected, |metadata| metadata.policy);
        let file = Arc::clone(
            self.custody
                .files
                .get(path)
                .ok_or(SourceInventoryError::SourceChanged)?,
        );
        if file.identity() != identity {
            return Err(SourceInventoryError::SourceChanged);
        }
        let native = admit_configuration_source(
            &self.policies[owner].native,
            &self.root,
            path,
            kind,
            &file,
            self.budget,
        )?;
        let mut included = false;
        for (index, policy) in self.policies.iter_mut().enumerate() {
            // This is membership in already resolved roots, not reference resolution. Scripts
            // outside the catalogue participate only through open facts or explicit native claims.
            let scope_count = if external { 4 } else { 3 };
            let mut in_scope = false;
            for scope in &policy.native_scopes[..scope_count] {
                self.budget
                    .charge_source_bytes(0, 0, sum(path.len(), scope.len())?, 0)?;
                in_scope |= below(path, scope).is_some();
            }
            // Preserve the selected-policy context of an explicitly admitted location source,
            // including recovery of an originally open Guard. Discovery in another profile's
            // root does not replace this original selection, and a physical claim adds none.
            let selected_native = external && index == selected && !policy.uses_stable_metadata();
            if !in_scope && !selected_native {
                continue;
            }
            if policy.uses_stable_metadata() {
                let Some(metadata) = self.metadata.get(path) else {
                    continue;
                };
                self.budget.charge_source_bytes(
                    0,
                    0,
                    product(metadata.visibility.len(), size_of::<usize>())?,
                    0,
                )?;
                if !metadata
                    .visibility
                    .iter()
                    .any(|policy| policy.configuration == index)
                {
                    continue;
                }
            }
            if index != owner {
                // Admit through every actual participating native policy. Only the unique
                // physical source keeps a native fact; additional observations are temporary.
                with_temporary_storage(self.budget, |budget| {
                    let actual = admit_configuration_source(
                        &policy.native,
                        &self.root,
                        path,
                        kind,
                        &file,
                        budget,
                    )?;
                    if actual.key() != native.key() || !same_native_metadata(&actual, &native) {
                        return Err(SourceInventoryError::IdentityConflict);
                    }
                    Ok(())
                })?;
            }
            policy.include_participant(path, self.budget)?;
            included = true;
        }
        if !included {
            if !external {
                return Err(SourceInventoryError::MembershipChanged);
            }
            // Project-location admission can accept an explicitly open file outside every
            // configured root. Bind only the receipt's actual selected policy. An input without
            // a selected-profile receipt uses the existing default native admission as the
            // sole context; no profile visibility is inferred from an unscoped location key.
            self.policies[owner].include_participant(path, self.budget)?;
        }
        Ok(native)
    }
}

fn admit_configuration_source(
    policy: &NativeDocumentSourcePolicy,
    root: &DirectoryCapability,
    path: &str,
    kind: SourceDocumentKind,
    current: &FileCapability,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<AdmittedNativeDocumentSource, SourceInventoryError> {
    let scratch = product(AssetMetaFileLimits::default().encoded_bytes().get(), 8)?;
    reserve(budget, scratch)?;
    let result = with_native_open_scratch(budget, |budget| {
        policy
            .admit_opened_budgeted(root, path, kind, current, 0, 3, budget)
            .map_err(SourceInventoryError::Native)
    });
    let released = release(budget, scratch);
    let native = result?;
    released?;
    Ok(native)
}
