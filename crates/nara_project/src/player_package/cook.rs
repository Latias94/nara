use std::{
    io::{self, Write},
    panic::{AssertUnwindSafe, catch_unwind},
};

use nara_asset::pack::{CapturedPack, PackError};
use nara_core::vfs::capabilities::{
    DirectoryCapability, DirectoryPublicationBeginError, DirectoryPublicationOrphan,
    DirectoryPublicationOutcome, DirectoryPublicationReceipt, DirectoryPublicationRejection,
    FsError, RelativeComponent,
};
use nara_scene::runtime_catalog::RuntimeContentExpectation;
use thiserror::Error;

use super::{PLAYER_PACK_FILE, staging_name, sync_staged_directory};

#[derive(Debug, Error)]
pub enum CookedPackStagingError<E: std::error::Error + 'static> {
    #[error("the selected project content changed: {0}")]
    Selection(#[source] E),
    #[error("the source selection check panicked")]
    SelectionPanicked,
    #[error(transparent)]
    Pack(#[from] PackError),
    #[error(transparent)]
    Filesystem(#[from] FsError),
    #[error("cooked pack writer I/O failed: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Error)]
pub enum CookedPackPublicationError<E: std::error::Error + 'static> {
    #[error("captured pack catalog differs from the selected content expectation")]
    CatalogMismatch,
    #[error("cooked pack staging name entropy was unavailable")]
    Entropy(#[from] getrandom::Error),
    #[error("cooked pack publication was rejected before staging: {0}")]
    BeforeStaging(#[source] CookedPackStagingError<E>),
    #[error(transparent)]
    Begin(#[from] DirectoryPublicationBeginError),
    #[error("cooked pack publication was rejected and removed: {cause:?}; {publication:?}")]
    Rejected {
        #[source]
        cause: Option<CookedPackStagingError<E>>,
        publication: DirectoryPublicationRejection,
    },
    #[error("cooked pack publication retained a recoverable orphan: {cause:?}; {publication:?}")]
    Orphaned {
        #[source]
        cause: Option<CookedPackStagingError<E>>,
        publication: DirectoryPublicationOrphan,
    },
}

impl<E: std::error::Error + 'static> CookedPackPublicationError<E> {
    /// Returns the original staging evidence when exact cleanup could not complete.
    #[must_use]
    pub fn orphan(&self) -> Option<&DirectoryPublicationOrphan> {
        match self {
            Self::Begin(error) => error.orphan(),
            Self::Orphaned { publication, .. } => Some(publication),
            _ => None,
        }
    }
}

/// Publishes the selected `content.pack` in a fresh child directory.
///
/// The caller supplies its independently admitted content expectation and revalidates the
/// source selection before staging and again immediately before publication. The pack bytes
/// are streamed from the original capture. An existing destination is never replaced.
pub fn publish_cooked_pack<E: std::error::Error + 'static>(
    pack: &CapturedPack,
    expected: &RuntimeContentExpectation,
    parent: &DirectoryCapability,
    destination: &RelativeComponent,
    mut verify_selection: impl FnMut() -> Result<(), E>,
) -> Result<DirectoryPublicationReceipt, CookedPackPublicationError<E>> {
    if pack.catalog_digest() != expected.catalog_digest() {
        return Err(CookedPackPublicationError::CatalogMismatch);
    }
    let mut verify = || {
        catch_unwind(AssertUnwindSafe(&mut verify_selection))
            .map_err(|_| CookedPackStagingError::SelectionPanicked)?
            .map_err(CookedPackStagingError::Selection)
    };
    verify().map_err(CookedPackPublicationError::BeforeStaging)?;
    let staging_name = staging_name(".nara-cook-")?;
    let transaction = parent.begin_directory_publication(destination, &staging_name)?;
    let cause = stage(&transaction.staging_directory(), pack)
        .and_then(|()| verify())
        .err();
    let outcome = if cause.is_some() {
        transaction.abort()
    } else {
        transaction.publish()
    };
    match outcome {
        DirectoryPublicationOutcome::Published(receipt) => Ok(receipt),
        DirectoryPublicationOutcome::RejectedClean(publication) => {
            Err(CookedPackPublicationError::Rejected { cause, publication })
        }
        DirectoryPublicationOutcome::Orphaned(publication) => {
            Err(CookedPackPublicationError::Orphaned { cause, publication })
        }
    }
}

fn stage<E: std::error::Error + 'static>(
    directory: &DirectoryCapability,
    pack: &CapturedPack,
) -> Result<(), CookedPackStagingError<E>> {
    let name = RelativeComponent::new(PLAYER_PACK_FILE).expect("fixed pack filename");
    let mut file = directory.create_child_file(&name)?;
    {
        let mut writer = file.writer()?;
        pack.write(&mut writer)?;
        writer.flush()?;
    }
    file.sync()?;
    drop(file);
    sync_staged_directory(directory)?;
    Ok(())
}
