//! Capture one finite native Prefab closure without publishing files or document state.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_asset::{
    AssetMeta, AssetMetaCandidate, AssetMetaFileLimits, AssetMetaIdentity,
    AssetMetaIdentityCandidate, AssetPath, AssetRef, AssetSourceKind, StableAssetId,
};
use nara_core::SourceUniverseId;
use nara_fs::{
    CapabilityRights, ContentDigest, DirectoryCapability, DirectoryEnumerationLimits, FsError,
    RelativeComponent, RelativePath, TrustMode,
};
use nara_project::NativeDocumentIdentity;
use nara_scene::{
    PrefabCopyError, PrefabCopyLimits, PrefabCopyRequest, PrefabCopySource, PrefabDocument,
    PrefabDocumentCandidate, SceneFormatError, SourceDocumentKey, SourceDocumentKind,
};
use nara_tooling::EditorSourceToolSelectionBasis;
use serde::Deserialize;

use super::super::{
    EditorProjectSession, EditorSourceToolInstallationInspection,
    EditorSourceToolReleaseMemberKind, Error, artifact_store, exact_destination_parent,
};
use super::{
    EditorSourceToolPrefabTemplateFile, EditorSourceToolPrefabTemplateImportRequest,
    PreparedEditorSourceToolPrefabTemplateImport,
};
use crate::project_host::{
    authoring::{
        ProjectSourceIndexSnapshot, ProjectSourceKind, native_source::NativeDocumentSourcePolicy,
    },
    runtime::editor::source_tools::EditorSourceToolReleaseMember,
};

const DESCRIPTOR_BYTES: usize = 64 * 1024;
const AGGREGATE_BYTES: usize = 64 * 1024 * 1024;
const METADATA_ENTRIES: usize = 4_096;
const PATH_BYTES: usize = 512;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    format_version: u32,
    kind: String,
    source_universe: SourceUniverseId,
    native_document_identity: NativeDocumentIdentity,
    prefabs_root: String,
    entry: String,
    documents: Vec<Document>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    member: String,
    path: String,
}

struct Input {
    original: SourceDocumentKey,
    references: Vec<AssetRef>,
    document: PrefabDocument,
}

impl EditorProjectSession {
    /// Captures declared Content, fixes copied identities and freezes all output bytes.
    ///
    /// This performs bounded code-free IO, using the Session's admitted authoring Schema and
    /// original source-index observation. It never opens a working document or changes history.
    pub fn prepare_source_tool_prefab_template_import(
        &self,
        basis: &EditorSourceToolSelectionBasis,
        project: DirectoryCapability,
        request: EditorSourceToolPrefabTemplateImportRequest,
    ) -> Result<PreparedEditorSourceToolPrefabTemplateImport, Error> {
        self.require_template_import(basis)?;
        if project.options().rights() != CapabilityRights::ReadWrite
            || project.options().trust() == TrustMode::TrustedLocal
        {
            return Err(Error::Capability);
        }
        if !project
            .same_object(&self.source_project_root())
            .map_err(|_| Error::RootMismatch)?
        {
            return Err(Error::RootMismatch);
        }

        let authoring = self.authoring_project().ok_or(Error::SchemaUnavailable)?;
        if authoring.source_index_has_pending_work() {
            return Err(Error::Busy);
        }
        let source_index = authoring
            .source_index_snapshot()
            .ok_or(Error::SchemaUnavailable)?;
        let project_input = authoring.schema_owner_input();
        let schema_basis = project_input.schema_basis().clone();
        let settings = authoring.settings();
        let schema_path = settings
            .authoring_schema_bundle
            .as_ref()
            .ok_or(Error::SchemaUnavailable)?;
        if source_index.root_identity() != authoring.project_root().identity()
            || source_index.manifest_digest() != authoring.manifest_digest()
            || source_index.bundle_digest() != authoring.authoring_bundle_digest()
        {
            return Err(Error::StaleBasis);
        }
        let source_roots = [
            &settings.paths.assets,
            &settings.paths.scenes,
            &settings.paths.prefabs,
            &settings.paths.scripts,
        ];
        for configured in source_roots {
            if !source_index
                .effective_roots()
                .iter()
                .any(|root| root.relative_path() == configured.as_str())
            {
                return Err(Error::SchemaUnavailable);
            }
        }
        // Index identities belong to the original Authoring capability session, even when the
        // caller imports a distinct write capability for that same native project directory.
        let read_root = authoring
            .project_root()
            .strict_read_only()
            .map_err(|_| Error::Capability)?;
        validate_index_roots(&read_root, &source_index)?;
        let prefab_root = settings.paths.prefabs.as_str();
        validate_path(prefab_root).map_err(|_| Error::InvalidPath)?;
        validate_path(&request.destination).map_err(|_| Error::InvalidPath)?;
        let destination_relative =
            descendant(&request.destination, prefab_root).ok_or(Error::InvalidPath)?;
        // The configured Prefabs root and every existing prefix must have exact stored spelling.
        read_root
            .open_directory_exact(&relative(prefab_root)?)
            .map_err(|_| Error::InvalidPath)?;
        let (parent, target) = exact_destination_parent(&project, &request.destination)?;
        ensure_destination_absent(&parent, &target)?;

        let installation = self
            .source_tool_selection_view()
            .installed
            .into_iter()
            .find(|entry| entry.installed.installation == request.installation)
            .ok_or(Error::Installation)?;
        if !artifact_store::validate_path(&installation.installed.directory) {
            return Err(Error::Installation);
        }
        let source_directory = read_root
            .open_directory_exact(&relative(&installation.installed.directory)?)
            .map_err(|_| Error::Installation)?;
        let inspection = EditorSourceToolInstallationInspection::read(
            &source_directory,
            installation.installed.receipt,
        )
        .map_err(|_| Error::Installation)?;
        if inspection.receipt().installation_id() != &request.installation
            || inspection.release().manifest_digest() != installation.installed.manifest
            || inspection.release().package() != installation.installed.package.as_ref()
        {
            return Err(Error::Installation);
        }
        let members = inspection.release().members();
        validate_path(&request.descriptor)?;
        let descriptor_member = content(members, &request.descriptor)?;
        let descriptor = decode_descriptor(descriptor_member.bytes(), &request.descriptor)?;
        let mut input_bytes = descriptor_member.bytes().len();
        let mut original_ids = BTreeSet::new();
        let mut inputs = Vec::with_capacity(descriptor.documents.len());
        for source in &descriptor.documents {
            let member = content(members, &source.member)?;
            charge_bytes(&mut input_bytes, member.bytes().len())?;
            let candidate = if source.member.ends_with(".prefab.json") {
                PrefabDocumentCandidate::decode_json_bytes(member.bytes())
            } else {
                PrefabDocumentCandidate::decode_ron_bytes(member.bytes())
            }
            .map_err(format_error)?;
            let document = candidate
                .into_composition_source()
                .map_err(|_| Error::InvalidPrefab)?;
            let logical_path = original_relative(&source.path, &descriptor.prefabs_root)?;
            let mut references = vec![AssetRef::Path(asset_path(logical_path)?)];
            let metadata_path = format!("{}.meta", source.member);
            let metadata = members.iter().find(|member| member.path() == metadata_path);
            let stable_id = if let Some(metadata) = metadata {
                if metadata.kind() != EditorSourceToolReleaseMemberKind::Content {
                    return Err(Error::UnsupportedTemplate);
                }
                charge_bytes(&mut input_bytes, metadata.bytes().len())?;
                let metadata = AssetMetaCandidate::<()>::decode_json_bytes(metadata.bytes())
                    .map_err(|_| Error::InvalidPrefab)?;
                let identity = metadata.identity();
                if identity.path().as_str() != logical_path
                    || identity.source_kind() != &AssetSourceKind::Prefab
                {
                    return Err(Error::InvalidPrefab);
                }
                let id = identity.stable_id();
                if !original_ids.insert(id) {
                    return Err(Error::IdentityConflict);
                }
                references.push(AssetRef::StableId(id));
                Some(id)
            } else {
                None
            };
            let original = match descriptor.native_document_identity {
                NativeDocumentIdentity::ProjectLocation => SourceDocumentKey::project_location(
                    descriptor.source_universe,
                    nara_asset::AssetSourcePath::new(source.path.as_str())
                        .map_err(|_| Error::InvalidPath)?,
                    SourceDocumentKind::Prefab,
                ),
                NativeDocumentIdentity::StableMetadata => SourceDocumentKey::stable_asset(
                    stable_id.ok_or(Error::InvalidPrefab)?,
                    SourceDocumentKind::Prefab,
                ),
            };
            inputs.push(Input {
                original,
                references,
                document,
            });
        }

        let (mut reserved_ids, metadata) = if settings.project.native_document_identity
            == NativeDocumentIdentity::StableMetadata
        {
            indexed_metadata(&read_root, &source_index)?
        } else {
            // Location copies allocate no stable IDs and must not interpret unrelated assets.
            (BTreeSet::new(), Vec::new())
        };
        reserved_ids.extend(original_ids);
        let mut identities = Vec::new();
        let mut sources = Vec::with_capacity(inputs.len());
        let mut sidecars = Vec::with_capacity(inputs.len());
        let mut output_bytes = 0;
        for (source, input) in descriptor.documents.iter().zip(inputs) {
            let relative_path = original_relative(&source.path, &descriptor.prefabs_root)?;
            let destination_path = format!("{}/{relative_path}", request.destination);
            validate_path(&destination_path).map_err(|_| Error::InvalidPath)?;
            // Stable native admission chooses the most specific configured source root. A copy
            // must retain the declared Prefabs-root-relative meaning under that same policy.
            if settings.project.native_document_identity == NativeDocumentIdentity::StableMetadata
                && source_roots
                    .into_iter()
                    .filter(|root| descendant(&destination_path, root.as_str()).is_some())
                    .max_by_key(|root| root.as_str().len())
                    .map(|root| root.as_str())
                    != Some(prefab_root)
            {
                return Err(Error::InvalidPath);
            }
            let logical_path = asset_path(&format!("{destination_relative}/{relative_path}"))?;
            let (destination, reference, sidecar) = match settings.project.native_document_identity
            {
                NativeDocumentIdentity::ProjectLocation => (
                    SourceDocumentKey::project_location(
                        settings.project.source_universe,
                        nara_asset::AssetSourcePath::new(destination_path.as_str())
                            .map_err(|_| Error::InvalidPath)?,
                        SourceDocumentKind::Prefab,
                    ),
                    AssetRef::Path(logical_path),
                    None,
                ),
                NativeDocumentIdentity::StableMetadata => {
                    let id = fresh_id(&mut reserved_ids)?;
                    identities.push(id);
                    let bytes = AssetMeta::new(
                        AssetMetaIdentity::new(id, logical_path, AssetSourceKind::Prefab),
                        (),
                    )
                    .to_json_string()
                    .map_err(|_| Error::InvalidPrefab)?;
                    charge_bytes(&mut output_bytes, bytes.len())?;
                    (
                        SourceDocumentKey::stable_asset(id, SourceDocumentKind::Prefab),
                        AssetRef::StableId(id),
                        Some(bytes),
                    )
                }
            };
            sources.push(PrefabCopySource::new(
                input.original,
                destination,
                input.references,
                reference,
                input.document,
            ));
            sidecars.push(sidecar);
        }
        let copied = PrefabCopyRequest::new(sources)
            .prepare(&schema_basis, PrefabCopyLimits::default())
            .map_err(copy_error)?;
        let mut files = Vec::with_capacity(descriptor.documents.len() + identities.len());
        for ((source, copied), sidecar) in descriptor
            .documents
            .iter()
            .zip(copied.into_sources())
            .zip(sidecars)
        {
            let path = original_relative(&source.path, &descriptor.prefabs_root)?;
            let bytes = if source.path.ends_with(".prefab.json") {
                copied.document().to_json_string()
            } else {
                copied.document().to_ron_string()
            }
            .map_err(format_error)?;
            charge_bytes(&mut output_bytes, bytes.len())?;
            files.push(frozen_file(
                path.into(),
                bytes,
                Some(copied.destination().clone()),
            ));
            if let Some(bytes) = sidecar {
                files.push(frozen_file(format!("{path}.meta").into(), bytes, None));
            }
        }
        files.sort_by(|left, right| left.path.cmp(&right.path));
        let entry: Arc<str> = format!(
            "{}/{}",
            request.destination,
            original_relative(&descriptor.entry, &descriptor.prefabs_root)?,
        )
        .into();
        Ok(PreparedEditorSourceToolPrefabTemplateImport {
            root: project,
            basis: basis.clone(),
            installation,
            source_directory,
            parent,
            target,
            destination: request.destination,
            entry,
            files,
            manifest_digest: authoring.manifest_digest(),
            schema_path: Arc::from(schema_path.as_str()),
            schema_digest: authoring.authoring_bundle_digest(),
            identities,
            metadata,
            project_input,
            source_index,
            schema_basis,
            source_policy: NativeDocumentSourcePolicy::new(settings),
            #[cfg(test)]
            test_stop_after_created: None,
            #[cfg(test)]
            test_after_publication: None,
            #[cfg(test)]
            test_after_member_admission: None,
        })
    }
}

fn decode_descriptor(bytes: &[u8], descriptor_path: &str) -> Result<Descriptor, Error> {
    if bytes.len() > DESCRIPTOR_BYTES {
        return Err(Error::BudgetExceeded);
    }
    let descriptor: Descriptor =
        serde_json::from_slice(bytes).map_err(|_| Error::InvalidDescriptor)?;
    if descriptor.format_version != 1 || descriptor.kind != "prefab_template" {
        return Err(Error::InvalidDescriptor);
    }
    if descriptor.documents.len() > PrefabCopyLimits::MAX_DOCUMENTS {
        return Err(Error::BudgetExceeded);
    }
    if descriptor.documents.is_empty() {
        return Err(Error::InvalidDescriptor);
    }
    validate_path(&descriptor.prefabs_root)?;
    let mut members = BTreeMap::new();
    let mut paths = BTreeMap::new();
    admit_file_path(&mut members, descriptor_path)?;
    for document in &descriptor.documents {
        validate_prefab_path(&document.member)?;
        validate_prefab_path(&document.path)?;
        original_relative(&document.path, &descriptor.prefabs_root)?;
        admit_file_path(&mut members, &document.member)?;
        admit_file_path(&mut members, &format!("{}.meta", document.member))?;
        admit_file_path(&mut paths, &document.path)?;
        admit_file_path(&mut paths, &format!("{}.meta", document.path))?;
    }
    if !descriptor
        .documents
        .iter()
        .any(|document| document.path == descriptor.entry)
    {
        return Err(Error::InvalidDescriptor);
    }
    Ok(descriptor)
}

/// Records both file leaves and directory prefixes, preserving their exact portable spelling.
fn admit_file_path(
    namespace: &mut BTreeMap<String, (String, bool)>,
    path: &str,
) -> Result<(), Error> {
    validate_path(path)?;
    let mut prefix = String::new();
    let mut components = path.split('/').peekable();
    while let Some(component) = components.next() {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(component);
        let file = components.peek().is_none();
        let folded = prefix.to_lowercase();
        if let Some((existing, existing_file)) = namespace.get(&folded) {
            if existing != &prefix || *existing_file || file {
                return Err(Error::InvalidDescriptor);
            }
        } else {
            namespace.insert(folded, (prefix.clone(), file));
        }
    }
    Ok(())
}

fn validate_path(path: &str) -> Result<(), Error> {
    if path.len() > PATH_BYTES
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
        || path
            .split('/')
            .next()
            .is_some_and(|part| part.eq_ignore_ascii_case(".nara"))
    {
        return Err(Error::InvalidDescriptor);
    }
    AssetPath::new(path).map_err(|_| Error::InvalidDescriptor)?;
    RelativePath::preflight(path).map_err(|_| Error::InvalidDescriptor)?;
    Ok(())
}

fn validate_prefab_path(path: &str) -> Result<(), Error> {
    validate_path(path)?;
    if !path.ends_with(".prefab.json") && !path.ends_with(".prefab.ron") {
        return Err(Error::InvalidDescriptor);
    }
    Ok(())
}

fn original_relative<'a>(path: &'a str, root: &str) -> Result<&'a str, Error> {
    descendant(path, root).ok_or(Error::InvalidDescriptor)
}

fn descendant<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    path.strip_prefix(root)?
        .strip_prefix('/')
        .filter(|path| !path.is_empty())
}

fn asset_path(path: &str) -> Result<AssetPath, Error> {
    AssetPath::new(path).map_err(|_| Error::InvalidPath)
}

fn relative(path: &str) -> Result<RelativePath, Error> {
    RelativePath::new(path).map_err(|_| Error::InvalidPath)
}

fn content<'a>(
    members: &'a [EditorSourceToolReleaseMember],
    path: &str,
) -> Result<&'a EditorSourceToolReleaseMember, Error> {
    members
        .iter()
        .find(|member| {
            member.path() == path && member.kind() == EditorSourceToolReleaseMemberKind::Content
        })
        .ok_or(Error::UnsupportedTemplate)
}

fn fresh_id(reserved: &mut BTreeSet<StableAssetId>) -> Result<StableAssetId, Error> {
    for _ in 0..16 {
        let id = StableAssetId::try_random().map_err(|_| Error::IdentityConflict)?;
        if reserved.insert(id) {
            return Ok(id);
        }
    }
    Err(Error::IdentityConflict)
}

fn frozen_file(
    path: Arc<str>,
    bytes: String,
    source: Option<SourceDocumentKey>,
) -> EditorSourceToolPrefabTemplateFile {
    EditorSourceToolPrefabTemplateFile {
        path,
        digest: ContentDigest::of_bytes(bytes.as_bytes()),
        bytes: Arc::from(bytes.into_bytes()),
        source,
    }
}

fn charge_bytes(total: &mut usize, amount: usize) -> Result<(), Error> {
    let next = total.checked_add(amount).ok_or(Error::BudgetExceeded)?;
    if next > AGGREGATE_BYTES {
        return Err(Error::BudgetExceeded);
    }
    *total = next;
    Ok(())
}

fn format_error(error: SceneFormatError) -> Error {
    if matches!(
        error,
        SceneFormatError::EncodedBytesExceeded { .. } | SceneFormatError::Budget(_)
    ) || error.shape_error().is_some()
    {
        Error::BudgetExceeded
    } else {
        Error::InvalidPrefab
    }
}

fn copy_error(error: PrefabCopyError) -> Error {
    match error {
        PrefabCopyError::BudgetExceeded | PrefabCopyError::DocumentLimit => Error::BudgetExceeded,
        PrefabCopyError::Schema { .. } => Error::SchemaUnavailable,
        PrefabCopyError::DuplicateOriginal { .. }
        | PrefabCopyError::DuplicateDestination { .. }
        | PrefabCopyError::DestinationNotIndependent { .. }
        | PrefabCopyError::DuplicateAlias { .. }
        | PrefabCopyError::DestinationAliasCollision { .. } => Error::IdentityConflict,
        _ => Error::InvalidPrefab,
    }
}

/// Pins the original nominal project/Schema admission and exact source-index observation.
/// Equal file bytes or a numerically equal generation cannot admit a replacement Session.
pub(in crate::project_host::runtime::editor::source_tools::template_import) fn validate_session(
    session: &EditorProjectSession,
    prepared: &PreparedEditorSourceToolPrefabTemplateImport,
) -> Result<(), Error> {
    session.require_template_import(&prepared.basis)?;
    let project = session
        .authoring_project()
        .ok_or(Error::SchemaUnavailable)?;
    if !prepared.project_input.matches(project)
        || &prepared.schema_basis != prepared.project_input.schema_basis()
        || project.manifest_digest() != prepared.manifest_digest
        || project.authoring_bundle_digest() != prepared.schema_digest
        || project
            .settings()
            .authoring_schema_bundle
            .as_ref()
            .map(|path| path.as_str())
            != Some(prepared.schema_path.as_ref())
    {
        return Err(Error::SchemaUnavailable);
    }
    if !prepared
        .root
        .same_object(&project.project_root())
        .map_err(|_| Error::RootMismatch)?
    {
        return Err(Error::RootMismatch);
    }
    if project.source_index_has_pending_work() {
        return Err(Error::Busy);
    }
    let current_index = project.source_index_snapshot().ok_or(Error::StaleBasis)?;
    if !Arc::ptr_eq(&current_index, &prepared.source_index)
        || current_index.generation() != prepared.source_index.generation()
    {
        return Err(Error::StaleBasis);
    }
    Ok(())
}

/// Rechecks precisely the original indexed metadata set; it does not discover unindexed files.
/// The Session separately pins the nominal project/Schema and this same source-index snapshot.
pub(super) fn revalidate_metadata(
    request: &PreparedEditorSourceToolPrefabTemplateImport,
) -> Result<(), Error> {
    let root = request
        .project_input
        .project_root()
        .strict_read_only()
        .map_err(|_| Error::Capability)?;
    if !root
        .same_object(&request.root)
        .map_err(|_| Error::RootMismatch)?
    {
        return Err(Error::RootMismatch);
    }
    validate_index_roots(&root, &request.source_index)?;
    if request.identities.is_empty() {
        return Ok(());
    }
    let (known_ids, metadata) = indexed_metadata(&root, &request.source_index)?;
    if metadata != request.metadata {
        return Err(Error::Changed);
    }
    let mut candidate_ids = BTreeSet::new();
    for id in &request.identities {
        if known_ids.contains(id) || !candidate_ids.insert(*id) {
            return Err(Error::IdentityConflict);
        }
    }
    Ok(())
}

fn validate_index_roots(
    root: &DirectoryCapability,
    index: &ProjectSourceIndexSnapshot,
) -> Result<(), Error> {
    if root.identity() != index.root_identity() {
        return Err(Error::StaleBasis);
    }
    // Missing declared roots are observed as missing, never silently treated as scanned trees.
    // Any other indexing diagnostic leaves the relevant identity observation incomplete.
    if index.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() != "project.source-index.declared-root-missing"
    }) {
        return Err(Error::SchemaUnavailable);
    }
    for fact in index.effective_roots() {
        match (
            fact.identity(),
            root.open_directory_exact(&relative(fact.relative_path())?),
        ) {
            (Some(expected), Ok(current)) if current.identity() == expected => {}
            (None, Err(error))
                if not_found(&error)
                    && index.diagnostics().iter().any(|diagnostic| {
                        diagnostic.relative_source() == Some(fact.relative_path())
                            && diagnostic.code().as_str()
                                == "project.source-index.declared-root-missing"
                    }) => {}
            _ => return Err(Error::Changed),
        }
    }
    Ok(())
}

fn indexed_metadata(
    root: &DirectoryCapability,
    index: &ProjectSourceIndexSnapshot,
) -> Result<(BTreeSet<StableAssetId>, Vec<(Arc<str>, ContentDigest)>), Error> {
    let mut identities = BTreeSet::new();
    let mut observations = Vec::new();
    let mut encoded_bytes = 0;
    for entry in index.entries().iter().filter(|entry| {
        entry.kind() == ProjectSourceKind::AssetMeta
            || entry
                .relative_path()
                .strip_suffix(".meta")
                .is_some_and(|path| {
                    index.entry(path).is_some_and(|source| {
                        matches!(
                            source.kind(),
                            ProjectSourceKind::Scene | ProjectSourceKind::Prefab
                        )
                    })
                })
    }) {
        let path = entry.relative_path();
        let source = path.strip_suffix(".meta").ok_or(Error::IdentityConflict)?;
        let logical = index
            .effective_roots()
            .iter()
            .filter_map(|fact| {
                descendant(source, fact.relative_path())
                    .map(|logical| (fact.relative_path().len(), logical))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, logical)| logical)
            .ok_or(Error::IdentityConflict)?;
        if observations.len() >= METADATA_ENTRIES {
            return Err(Error::BudgetExceeded);
        }
        let file = root
            .open_file_exact(&relative(path)?)
            .map_err(|_| Error::Changed)?;
        if Some(file.identity()) != entry.scoped_identity() {
            return Err(Error::Changed);
        }
        let remaining = AGGREGATE_BYTES - encoded_bytes;
        let limit = AssetMetaFileLimits::default()
            .encoded_bytes()
            .get()
            .min(remaining);
        let bytes = file.read_to_end_bounded(limit as u64).map_err(read_error)?;
        charge_bytes(&mut encoded_bytes, bytes.len())?;
        // Only the identity envelope belongs here; settings remain owned by their concrete domain.
        // An invalid identity or metadata path is a refusal, never an empty identity set.
        let candidate = AssetMetaIdentityCandidate::decode_json_bytes(&bytes)
            .map_err(|_| Error::IdentityConflict)?;
        if index.entry(source).is_some_and(|entry| {
            matches!(entry.kind(), ProjectSourceKind::Scene)
                && candidate.identity().source_kind() != &AssetSourceKind::Scene
                || matches!(entry.kind(), ProjectSourceKind::Prefab)
                    && candidate.identity().source_kind() != &AssetSourceKind::Prefab
        }) {
            return Err(Error::IdentityConflict);
        }
        if candidate.identity().path().as_str() != logical
            || !identities.insert(candidate.identity().stable_id())
        {
            return Err(Error::IdentityConflict);
        }
        observations.push((
            Arc::clone(entry.relative_path_arc()),
            ContentDigest::of_bytes(&bytes),
        ));
    }
    Ok((identities, observations))
}

/// A bounded direct-parent observation also refuses differently cased existing names on Linux.
/// This is one destination check, not a recursive project/source-index scan.
pub(super) fn ensure_destination_absent(
    parent: &DirectoryCapability,
    target: &RelativeComponent,
) -> Result<(), Error> {
    let folded = target.to_str().ok_or(Error::InvalidPath)?.to_lowercase();
    let mut entries = parent
        .begin_child_enumeration(DirectoryEnumerationLimits::default())
        .map_err(read_error)?;
    loop {
        let batch = entries.next_batch(256).map_err(read_error)?;
        if !batch.rejected_entry_facts().is_empty() {
            return Err(Error::Read);
        }
        if batch.entries().iter().any(|entry| {
            entry
                .name()
                .to_str()
                .is_none_or(|name| name.to_lowercase() == folded)
        }) {
            return Err(Error::TargetExists);
        }
        if batch.is_complete() {
            return Ok(());
        }
    }
}

fn not_found(error: &FsError) -> bool {
    matches!(error, FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
}

fn read_error(error: FsError) -> Error {
    match error {
        FsError::ByteLimitExceeded { .. }
        | FsError::DirectoryEnumerationEntryLimitExceeded { .. }
        | FsError::DirectoryEnumerationRejectedFactLimitExceeded { .. } => Error::BudgetExceeded,
        _ => Error::Read,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn descriptor() -> Value {
        json!({
            "format_version": 1,
            "kind": "prefab_template",
            "source_universe": "4d05474d-b6bc-48bf-a710-45d60ded6d59",
            "native_document_identity": "project_location",
            "prefabs_root": "prefabs",
            "entry": "prefabs/main.prefab.json",
            "documents": [{"member": "samples/main.prefab.json", "path": "prefabs/main.prefab.json"}],
        })
    }

    fn decode(value: &Value) -> Result<Descriptor, Error> {
        decode_descriptor(&serde_json::to_vec(value).unwrap(), "template.json")
    }

    #[test]
    fn descriptor_preserves_complete_project_coordinates_for_both_encodings() {
        let mut value = descriptor();
        value["documents"].as_array_mut().unwrap().push(json!({
            "member": "samples/nested/child.prefab.ron",
            "path": "prefabs/nested/child.prefab.ron",
        }));
        let admitted = decode(&value).unwrap();
        assert_eq!(admitted.entry, "prefabs/main.prefab.json");
        assert_eq!(
            admitted.documents[1].path,
            "prefabs/nested/child.prefab.ron"
        );
        assert_eq!(
            original_relative(&admitted.documents[1].path, &admitted.prefabs_root).unwrap(),
            "nested/child.prefab.ron"
        );
    }

    #[test]
    fn descriptor_refuses_unknown_and_duplicate_fields() {
        let mut root = descriptor();
        root["script"] = json!("import.rs");
        assert!(matches!(decode(&root), Err(Error::InvalidDescriptor)));
        let mut document = descriptor();
        document["documents"][0]["optional"] = json!(true);
        assert!(matches!(decode(&document), Err(Error::InvalidDescriptor)));
        let encoded = serde_json::to_string(&descriptor()).unwrap();
        let duplicate = format!("{{\"format_version\":1,{}", &encoded[1..]);
        assert!(matches!(
            decode_descriptor(duplicate.as_bytes(), "template.json"),
            Err(Error::InvalidDescriptor),
        ));
    }

    #[test]
    fn descriptor_refuses_missing_entry_and_other_original_roots() {
        for entry in ["prefabs/Main.prefab.json", "main.prefab.json"] {
            let mut value = descriptor();
            value["entry"] = json!(entry);
            assert!(matches!(decode(&value), Err(Error::InvalidDescriptor)));
        }
        for path in [
            "scenes/main.prefab.json",
            "Prefabs/main.prefab.json",
            "prefabs/../main.prefab.json",
            "prefabs//main.prefab.json",
            "prefabs/./main.prefab.json",
            "prefabs/main.scene.json",
        ] {
            let mut value = descriptor();
            value["entry"] = json!(path);
            value["documents"][0]["path"] = json!(path);
            assert!(
                matches!(decode(&value), Err(Error::InvalidDescriptor)),
                "{path}"
            );
        }
    }

    #[test]
    fn descriptor_refuses_duplicate_case_and_file_prefix_conflicts() {
        for (member, path) in [
            ("samples/main.prefab.json", "prefabs/child.prefab.json"),
            ("samples/child.prefab.json", "prefabs/main.prefab.json"),
            ("Samples/child.prefab.json", "prefabs/child.prefab.json"),
            ("samples/child.prefab.json", "prefabs/Main.prefab.json"),
            (
                "samples/main.prefab.json/child.prefab.json",
                "prefabs/child.prefab.json",
            ),
            (
                "samples/child.prefab.json",
                "prefabs/main.prefab.json/child.prefab.json",
            ),
            (
                "samples/main.prefab.json.meta/child.prefab.json",
                "prefabs/child.prefab.json",
            ),
        ] {
            let mut value = descriptor();
            value["documents"]
                .as_array_mut()
                .unwrap()
                .push(json!({"member": member, "path": path}));
            assert!(
                matches!(decode(&value), Err(Error::InvalidDescriptor)),
                "{member}: {path}"
            );
        }
        let mut value = descriptor();
        value["documents"][0]["member"] = json!("template.json/main.prefab.json");
        assert!(matches!(decode(&value), Err(Error::InvalidDescriptor)));
    }

    #[test]
    fn descriptor_enforces_document_and_encoded_limits_before_copying() {
        let mut value = descriptor();
        value["documents"] = json!([]);
        assert!(matches!(decode(&value), Err(Error::InvalidDescriptor)));
        value["documents"] = json!(vec![descriptor()["documents"][0].clone(); 33]);
        assert!(matches!(decode(&value), Err(Error::BudgetExceeded)));
        assert!(matches!(
            decode_descriptor(&vec![b' '; DESCRIPTOR_BYTES + 1], "template.json"),
            Err(Error::BudgetExceeded),
        ));
    }
}
