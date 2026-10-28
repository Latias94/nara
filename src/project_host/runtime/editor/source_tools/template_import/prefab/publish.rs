//! Stage a fixed finite copy, publish its exact directory, then freshly admit current files.

use std::{io::Write, sync::Arc};

use nara_fs::{
    DirectoryCapability, DirectoryPublicationBeginError, DirectoryPublicationOutcome,
    FileCapability, RelativeComponent, RelativePath,
};
use nara_scene::{AuthoringDocumentSource, PrefabDocumentCandidate, SourceDocumentKind};
use nara_tooling::{
    EditorSourceToolTemplateImportMember as Member,
    EditorSourceToolTemplateImportOutcome as Outcome,
};

use super::{
    EditorSourceToolPrefabTemplatePublication as Publication, Error,
    PreparedEditorSourceToolPrefabTemplateImport as Prepared, StagedPrefabMembers,
};
use crate::project_host::runtime::editor::source_tools::{template_import, tree};

pub(in crate::project_host::runtime::editor::source_tools::template_import) fn publish(
    request: &Prepared,
    cancelled: impl Fn() -> bool,
) -> Result<(Outcome, Publication), Error> {
    revalidate(request)?;
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let staging_name = staging_name()?;
    let transaction = match request
        .parent
        .begin_directory_publication(&request.target, &staging_name)
    {
        Ok(transaction) => transaction,
        Err(DirectoryPublicationBeginError::BeforeStaging(_)) => return Err(Error::Write),
        Err(DirectoryPublicationBeginError::RejectedClean(rejection)) => {
            return Ok(not_published(
                request,
                DirectoryPublicationOutcome::RejectedClean(*rejection),
                None,
                StagedPrefabMembers::default(),
                Error::Write,
            ));
        }
        Err(DirectoryPublicationBeginError::Orphaned(orphan)) => {
            let directory = orphan.staging_directory();
            return Ok(not_published(
                request,
                DirectoryPublicationOutcome::Orphaned(*orphan),
                directory,
                StagedPrefabMembers::default(),
                Error::Write,
            ));
        }
        Err(_) => return Err(Error::Write),
    };
    let directory = transaction.staging_directory();
    let mut custody = StagedPrefabMembers::default();
    let staged = stage(request, &directory, &cancelled, &mut custody)
        .and_then(|()| revalidate(request))
        .and_then(|()| {
            if cancelled() {
                Err(Error::Cancelled)
            } else {
                Ok(())
            }
        });
    if let Err(error) = staged {
        return Ok(not_published(
            request,
            transaction.abort(),
            Some(directory),
            custody,
            error,
        ));
    }
    // Windows cannot publish this populated directory while retaining child file handles.
    // Fresh member admission below is distinct from these now-closed creation observations.
    custody.files.clear();
    custody.cleanup.clear();
    custody.directories.clear();
    let publication = transaction.publish();
    let DirectoryPublicationOutcome::Published(receipt) = &publication else {
        return Ok(not_published(
            request,
            publication,
            Some(directory),
            custody,
            Error::Write,
        ));
    };
    let parent_directory_sync = receipt.durability().parent_directory_synced();
    #[cfg(test)]
    if let Some(after_publication) = &request.test_after_publication {
        after_publication(&directory);
    }
    let mut observed_files = Vec::new();
    let admitted = admit_published(request, &directory, &mut observed_files);
    let members: Arc<[Member]> = custody.members.clone().into();
    custody.files = observed_files;
    let outcome = match &admitted {
        Ok(()) => Outcome::PrefabImported {
            destination: Arc::clone(&request.destination),
            entry: Arc::clone(&request.entry),
            members,
            parent_directory_sync,
        },
        Err(error) => Outcome::PrefabPublishedWithoutAdmission {
            destination: Arc::clone(&request.destination),
            entry: Arc::clone(&request.entry),
            members,
            parent_directory_sync,
            error: error.clone(),
        },
    };
    Ok((
        outcome,
        Publication {
            publication,
            directory: Some(directory),
            custody,
            error: admitted.err(),
        },
    ))
}

fn not_published(
    request: &Prepared,
    publication: DirectoryPublicationOutcome,
    directory: Option<DirectoryCapability>,
    custody: StagedPrefabMembers,
    error: Error,
) -> (Outcome, Publication) {
    let staging = match &publication {
        DirectoryPublicationOutcome::Orphaned(orphan) => {
            let parent = request
                .destination
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .unwrap_or("");
            orphan
                .staging_name()
                .to_str()
                .map(|name| Arc::from(format!("{parent}/{name}")))
        }
        _ => None,
    };
    (
        Outcome::PrefabNotPublished {
            destination: Arc::clone(&request.destination),
            staging,
            created: custody.members.clone().into(),
            created_directories: custody.created_directories.clone().into(),
            error: error.clone(),
        },
        Publication {
            publication,
            directory,
            custody,
            error: Some(error),
        },
    )
}

fn stage(
    request: &Prepared,
    directory: &DirectoryCapability,
    cancelled: &impl Fn() -> bool,
    custody: &mut StagedPrefabMembers,
) -> Result<(), Error> {
    for member in &request.files {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let path = relative(&member.path)?;
        let (leaf, parents) = path.components().split_last().ok_or(Error::InvalidPath)?;
        let mut parent = directory.share();
        let mut prefix = String::new();
        for name in parents {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(name.to_str().ok_or(Error::InvalidPath)?);
            parent = match custody.directories.get(&prefix) {
                Some(existing) => existing.share(),
                None => {
                    let child = match parent.create_child_directory_retained(name) {
                        Ok(child) => child,
                        Err(failure) => {
                            let (error, created) = failure.into_parts();
                            if created.is_some() {
                                custody.created_directories.push(prefix.clone().into());
                            }
                            custody.unverified_directory = created;
                            custody.filesystem_error = Some(error);
                            return Err(Error::Write);
                        }
                    };
                    custody.created_directories.push(prefix.clone().into());
                    custody.directories.insert(prefix.clone(), child.share());
                    child
                }
            };
        }
        let mut file = match parent.create_temp(leaf) {
            Ok(file) => file,
            Err(failure) => {
                let (error, cleanup) = failure.into_parts();
                if let Some(cleanup) = cleanup {
                    custody.members.push(Member {
                        path: member.path.clone(),
                        digest: member.digest,
                    });
                    custody.cleanup.push(cleanup);
                }
                custody.filesystem_error = Some(error);
                return Err(Error::Write);
            }
        };
        custody.members.push(Member {
            path: member.path.clone(),
            digest: member.digest,
        });
        #[cfg(test)]
        if request.test_stop_after_created == Some(custody.members.len()) {
            custody.cleanup.push(file.into_cleanup());
            return Err(Error::Write);
        }
        let written = file
            .write_all(&member.bytes)
            .map_err(|source| nara_fs::FsError::Io {
                operation: nara_fs::FsOperation::Write,
                source,
            })
            .and_then(|()| file.sync().map(|_| ()));
        custody.cleanup.push(file.into_cleanup());
        if let Err(error) = written {
            custody.filesystem_error = Some(error);
            return Err(Error::Write);
        }
        let reader = directory
            .open_file_exact(&path)
            .map_err(|_| Error::Changed)?;
        if !custody
            .cleanup
            .last()
            .ok_or(Error::Write)?
            .same_object(&reader)
            .map_err(|_| Error::Changed)?
        {
            return Err(Error::Changed);
        }
        reader
            .verify_digest(member.digest, member.digest.length())
            .map_err(|_| Error::Changed)?;
        custody.files.push(reader);
    }
    tree::verify(
        directory,
        request.files.iter().map(|member| member.path.as_ref()),
    )
    .map_err(|_| Error::Changed)
}

fn revalidate(request: &Prepared) -> Result<(), Error> {
    let source = request
        .root
        .open_directory_exact(&relative(&request.installation.installed.directory)?)
        .map_err(|_| Error::Changed)?;
    if !source
        .same_object(&request.source_directory)
        .map_err(|_| Error::Changed)?
    {
        return Err(Error::Changed);
    }
    validate_parent(request)?;
    super::prepare::ensure_destination_absent(&request.parent, &request.target)?;
    for (path, digest) in [
        ("nara.toml", request.manifest_digest),
        (request.schema_path.as_ref(), request.schema_digest),
    ] {
        request
            .root
            .open_file_exact(&relative(path)?)
            .map_err(|_| Error::Changed)?
            .verify_digest(digest, digest.length())
            .map_err(|_| Error::SchemaUnavailable)?;
    }
    super::prepare::revalidate_metadata(request)
}

fn validate_parent(request: &Prepared) -> Result<(), Error> {
    let (parent, target) =
        template_import::exact_destination_parent(&request.root, &request.destination)
            .map_err(|_| Error::Changed)?;
    if target != request.target
        || !parent
            .same_object(&request.parent)
            .map_err(|_| Error::Changed)?
    {
        return Err(Error::Changed);
    }
    Ok(())
}

fn admit_published(
    request: &Prepared,
    original_directory: &DirectoryCapability,
    files: &mut Vec<FileCapability>,
) -> Result<(), Error> {
    validate_parent(request)?;
    let directory = request
        .parent
        .open_directory_exact(
            &RelativePath::from_components([request.target.clone()])
                .map_err(|_| Error::InvalidPath)?,
        )
        .map_err(|_| Error::Changed)?;
    if !directory
        .same_object(original_directory)
        .map_err(|_| Error::Changed)?
    {
        return Err(Error::Changed);
    }
    for member in &request.files {
        let file = directory
            .open_file_exact(&relative(&member.path)?)
            .map_err(|_| Error::Changed)?;
        let bytes = file
            .read_to_end_bounded(member.digest.length())
            .map_err(|_| Error::Changed)?;
        if nara_fs::ContentDigest::of_bytes(&bytes) != member.digest {
            return Err(Error::Changed);
        }
        if let Some(expected) = &member.source {
            let path = format!("{}/{}", request.destination, member.path);
            let native = request
                .source_policy
                .admit(
                    &request.root,
                    &path,
                    SourceDocumentKind::Prefab,
                    file.identity(),
                )
                .map_err(|_| Error::InvalidPrefab)?;
            if native.key() != expected {
                return Err(Error::IdentityConflict);
            }
            let candidate = if member.path.ends_with(".json") {
                PrefabDocumentCandidate::decode_json_bytes(&bytes)
            } else {
                PrefabDocumentCandidate::decode_ron_bytes(&bytes)
            }
            .map_err(|_| Error::InvalidPrefab)?;
            candidate
                .admit_authoring_with_basis(
                    AuthoringDocumentSource::new(
                        nara_asset::AssetRef::path(path).map_err(|_| Error::InvalidPath)?,
                    ),
                    &request.schema_basis,
                )
                .map_err(|_| Error::InvalidPrefab)?;
        }
        files.push(file);
    }
    #[cfg(test)]
    if let Some(after_admission) = &request.test_after_member_admission {
        after_admission(&directory);
    }
    tree::verify_observed(
        &directory,
        request.files.iter().map(|member| member.path.as_ref()),
        |path, identity| {
            request
                .files
                .iter()
                .position(|member| member.path.as_ref() == path)
                .and_then(|index| files.get(index))
                .is_some_and(|file| Some(file.identity()) == identity)
        },
    )
    .map_err(|_| Error::Changed)
}

fn relative(value: &str) -> Result<RelativePath, Error> {
    RelativePath::new(value).map_err(|_| Error::InvalidPath)
}

fn staging_name() -> Result<RelativeComponent, Error> {
    use std::fmt::Write as _;
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|_| Error::Write)?;
    let mut name = String::from(".nara-prefab-import-");
    for byte in random {
        write!(name, "{byte:02x}").map_err(|_| Error::Write)?;
    }
    RelativeComponent::new(name).map_err(|_| Error::InvalidPath)
}
