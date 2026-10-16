use std::{
    fs, io,
    path::{Path, PathBuf},
};

use super::{
    ExecutableGenerationId, ExecutableGenerationStore, GenerationStoreCloseReceipt,
    GenerationStoreError, ImmutableExecutableGeneration, WORK_DIRECTORY_PREFIX,
    scavenge::validate_owned_store, valid_generation_directory_name, valid_work_directory_name,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RetentionSlot {
    Active,
    LastGood,
    Candidate,
}

pub(super) fn activate_candidate(
    store: &mut ExecutableGenerationStore,
    generation: ExecutableGenerationId,
) -> Result<ImmutableExecutableGeneration, GenerationStoreError> {
    store.ensure_operational()?;
    if store.active.is_some() {
        return Err(GenerationStoreError::ActiveGenerationExists);
    }
    let candidate = store
        .candidate
        .as_ref()
        .filter(|candidate| candidate.id == generation)
        .cloned()
        .ok_or(GenerationStoreError::CandidateMismatch)?;
    store.candidate = None;
    store.active = Some(candidate.clone());
    Ok(candidate)
}

pub(super) fn activate_last_good(
    store: &mut ExecutableGenerationStore,
) -> Result<ImmutableExecutableGeneration, GenerationStoreError> {
    store.ensure_operational()?;
    if store.active.is_some() {
        return Err(GenerationStoreError::ActiveGenerationExists);
    }
    let generation = store
        .last_good
        .clone()
        .ok_or(GenerationStoreError::LastGoodUnavailable)?;
    store.active = Some(generation.clone());
    Ok(generation)
}

pub(super) fn admit_ready(
    store: &mut ExecutableGenerationStore,
    generation: ExecutableGenerationId,
) -> Result<(), GenerationStoreError> {
    store.ensure_operational()?;
    let active = store
        .active
        .as_ref()
        .filter(|active| active.id == generation)
        .cloned()
        .ok_or(GenerationStoreError::GenerationNotActive)?;
    if store
        .last_good
        .as_ref()
        .is_some_and(|value| value.id == generation)
    {
        return Ok(());
    }
    if let Some(previous) = store.last_good.as_ref()
        && !is_retained_elsewhere(store, previous.id, RetentionSlot::LastGood)
    {
        remove_generation(&store.store_path, previous)?;
    }
    store.last_good = Some(active);
    Ok(())
}

pub(super) fn finish_active(
    store: &mut ExecutableGenerationStore,
    generation: ExecutableGenerationId,
) -> Result<(), GenerationStoreError> {
    store.ensure_operational()?;
    let active = store
        .active
        .as_ref()
        .filter(|active| active.id == generation)
        .ok_or(GenerationStoreError::GenerationNotActive)?;
    if !is_retained_elsewhere(store, active.id, RetentionSlot::Active) {
        remove_generation(&store.store_path, active)?;
    }
    store.active = None;
    Ok(())
}

pub(super) fn begin_work_directory(
    store: &mut ExecutableGenerationStore,
) -> Result<PathBuf, GenerationStoreError> {
    store.ensure_operational()?;
    if store.active.is_none() {
        return Err(GenerationStoreError::WorkDirectoryRequiresActiveGeneration);
    }
    if store.work_directory.is_some() {
        return Err(GenerationStoreError::ActiveWorkDirectoryExists);
    }
    let sequence = store.next_work_directory;
    let next = sequence
        .checked_add(1)
        .ok_or(GenerationStoreError::WorkDirectorySequenceExhausted)?;
    let name = format!("{WORK_DIRECTORY_PREFIX}{sequence:016x}");
    let path = store.store_path.join(&name);
    if path.parent() != Some(store.store_path.as_path()) || !valid_work_directory_name(&name) {
        return Err(GenerationStoreError::WorkDirectoryIdentityInvalid);
    }
    fs::create_dir(&path).map_err(|source| GenerationStoreError::Io {
        operation: "create-work-directory",
        source,
    })?;
    store.next_work_directory = next;
    store.work_directory = Some(path.clone());
    let metadata = fs::symlink_metadata(&path).map_err(|source| GenerationStoreError::Io {
        operation: "inspect-work-directory",
        source,
    })?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(GenerationStoreError::WorkDirectoryIdentityInvalid);
    }
    Ok(path)
}

pub(super) fn finish_work_directory(
    store: &mut ExecutableGenerationStore,
) -> Result<(), GenerationStoreError> {
    let Some(path) = store.work_directory.as_ref() else {
        return Ok(());
    };
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return Err(GenerationStoreError::WorkDirectoryIdentityInvalid);
    };
    if path.parent() != Some(store.store_path.as_path()) || !valid_work_directory_name(name) {
        return Err(GenerationStoreError::WorkDirectoryIdentityInvalid);
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path).map_err(|source| GenerationStoreError::Io {
                operation: "remove-work-directory",
                source,
            })?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        _ => return Err(GenerationStoreError::WorkDirectoryIdentityInvalid),
    }
    store.work_directory = None;
    Ok(())
}

pub(super) fn try_close(
    store: &mut ExecutableGenerationStore,
) -> Result<GenerationStoreCloseReceipt, GenerationStoreError> {
    if store.closed {
        return Err(GenerationStoreError::StoreClosed);
    }
    if store.active.is_some() {
        return Err(GenerationStoreError::ActiveGenerationExists);
    }
    if store.work_directory.is_some() {
        return Err(GenerationStoreError::ActiveWorkDirectoryExists);
    }
    let owner_marker = store
        .owner_marker
        .as_ref()
        .ok_or(GenerationStoreError::StoreClosed)?;
    validate_owned_store(
        &store.namespace,
        &store.store_path,
        &store.store_id,
        owner_marker,
    )?;
    fs::remove_dir_all(&store.store_path).map_err(|source| GenerationStoreError::Io {
        operation: "remove-run-store",
        source,
    })?;
    store.candidate = None;
    store.last_good = None;
    store.root = None;
    store.owner_marker = None;
    store.closed = true;
    Ok(GenerationStoreCloseReceipt)
}

pub(super) fn evict_candidate(
    store: &mut ExecutableGenerationStore,
) -> Result<(), GenerationStoreError> {
    let Some(candidate) = store.candidate.as_ref() else {
        return Ok(());
    };
    remove_generation(&store.store_path, candidate)?;
    store.candidate = None;
    Ok(())
}

fn is_retained_elsewhere(
    store: &ExecutableGenerationStore,
    generation: ExecutableGenerationId,
    excluding: RetentionSlot,
) -> bool {
    (excluding != RetentionSlot::Active
        && store
            .active
            .as_ref()
            .is_some_and(|value| value.id == generation))
        || (excluding != RetentionSlot::LastGood
            && store
                .last_good
                .as_ref()
                .is_some_and(|value| value.id == generation))
        || (excluding != RetentionSlot::Candidate
            && store
                .candidate
                .as_ref()
                .is_some_and(|value| value.id == generation))
}

fn remove_generation(
    store_path: &Path,
    generation: &ImmutableExecutableGeneration,
) -> Result<(), GenerationStoreError> {
    if remove_exact_generation_tree(
        store_path,
        generation
            .directory
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(GenerationStoreError::GenerationPathInvalid)?,
    ) {
        Ok(())
    } else {
        Err(GenerationStoreError::GenerationCleanupIncomplete)
    }
}

pub(super) fn remove_exact_generation_tree(store_path: &Path, child_name: &str) -> bool {
    if !valid_generation_directory_name(child_name) {
        return false;
    }
    let child = store_path.join(child_name);
    if child.parent() != Some(store_path) {
        return false;
    }
    match fs::symlink_metadata(&child) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(child).is_ok()
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        _ => false,
    }
}
