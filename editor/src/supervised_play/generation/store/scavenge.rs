use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use nara::fs::{
    CapabilityRights, DirectoryCapability, FileCapability, HostCapabilityOptions, LockMode,
    TrustMode,
};
use nara::project_host::MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES;

use super::super::format::MAX_LAUNCH_MANIFEST_BYTES;
use super::super::platform::{EXECUTABLE_NAME, open_directory_handle};
use super::{
    ExecutableGenerationStore, GenerationScavengeReceipt, GenerationStoreError,
    LAUNCH_MANIFEST_NAME, MAX_GENERATION_FILES_PER_DIRECTORY, component, is_lower_hex,
    valid_generation_directory_name, valid_work_directory_name,
};

pub(super) const STORE_NAMESPACE: &str = "nara-editor-supervised-play-v1";
const OWNER_MARKER_NAME: &str = "owner.v1";
const OWNER_MARKER_MAGIC: &str = "NARA_EDITOR_SUPERVISED_PLAY_OWNER_V1";
const MAX_OWNER_MARKER_BYTES: u64 = 256;

pub(super) const SCAVENGE_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_SCANNED_STORES: usize = 32;
const MAX_REMOVED_STORES: usize = 2;
const MAX_GENERATION_DIRECTORIES_PER_STORE: usize = 4;
const MAX_SCAVENGE_WORK_ENTRIES_PER_STORE: usize = 4_096;
pub(super) const MAX_SCAVENGE_WORK_DEPTH: usize = 32;
const MAX_SCAVENGE_STORE_BYTES: u64 = 6 * 1024 * 1024 * 1024;
const MAX_SCAVENGE_TOTAL_BYTES: u64 = 12 * 1024 * 1024 * 1024;

pub(crate) static NEXT_STORE_NONCE: AtomicU64 = AtomicU64::new(1);

pub(super) fn create_at(
    base_root: &Path,
    now: SystemTime,
) -> Result<ExecutableGenerationStore, GenerationStoreError> {
    let namespace = ensure_namespace(base_root)?;
    let created_unix_millis = unix_millis(now)?;
    let pid = std::process::id();

    for _ in 0..32 {
        let nonce = NEXT_STORE_NONCE.fetch_add(1, Ordering::Relaxed);
        let store_id = format!("run-{created_unix_millis:016x}-{pid:08x}-{nonce:016x}");
        let store_path = namespace.join(&store_id);
        match fs::create_dir(&store_path) {
            Ok(()) => {
                let root = match open_directory_capability(&store_path) {
                    Ok(root) => root,
                    Err(error) => {
                        let _ = fs::remove_dir_all(&store_path);
                        return Err(error);
                    }
                };
                let marker = match OwnerMarker::create(&root, &store_id, created_unix_millis) {
                    Ok(marker) => marker,
                    Err(error) => {
                        drop(root);
                        let _ = fs::remove_dir_all(&store_path);
                        return Err(error);
                    }
                };
                return Ok(ExecutableGenerationStore {
                    namespace,
                    store_path,
                    store_id,
                    root: Some(root),
                    owner_marker: Some(marker),
                    next_generation: 1,
                    next_work_directory: 1,
                    active: None,
                    last_good: None,
                    candidate: None,
                    work_directory: None,
                    cleanup_incomplete: false,
                    closed: false,
                });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(GenerationStoreError::Io {
                    operation: "create-run-store",
                    source,
                });
            }
        }
    }

    Err(GenerationStoreError::RunStoreNameExhausted)
}

pub(super) fn scavenge_inactive_at(
    base_root: &Path,
    now: SystemTime,
) -> Result<GenerationScavengeReceipt, GenerationStoreError> {
    let namespace = ensure_namespace(base_root)?;
    let now_millis = unix_millis(now)?;
    let minimum_age_millis = u64::try_from(SCAVENGE_MIN_AGE.as_millis())
        .map_err(|_| GenerationStoreError::ClockOutOfRange)?;
    let oldest_allowed = now_millis.saturating_sub(minimum_age_millis);
    let mut receipt = GenerationScavengeReceipt::default();
    let entries = fs::read_dir(&namespace).map_err(|source| GenerationStoreError::Io {
        operation: "enumerate-store-namespace",
        source,
    })?;

    for entry in entries {
        if receipt.examined >= MAX_SCANNED_STORES {
            receipt.limit_reached = true;
            break;
        }
        receipt.examined += 1;
        let Ok(entry) = entry else {
            receipt.skipped_unverified += 1;
            continue;
        };
        let Some(store_id) = entry.file_name().to_str().map(str::to_owned) else {
            receipt.skipped_unverified += 1;
            continue;
        };
        if !valid_store_id(&store_id) {
            receipt.skipped_unverified += 1;
            continue;
        }
        let store_path = entry.path();
        let Some(marker) = inspect_inactive_marker(&store_path, &store_id, oldest_allowed) else {
            receipt.skipped_unverified += 1;
            continue;
        };
        let marker = match marker {
            InactiveMarkerObservation::Live => {
                receipt.skipped_live += 1;
                continue;
            }
            InactiveMarkerObservation::Inactive(marker) => marker,
        };
        if !marker.named_binding_is_current() {
            receipt.skipped_unverified += 1;
            continue;
        }
        let Some(store_bytes) = measure_scavenge_store_tree(&store_path) else {
            receipt.skipped_unverified += 1;
            continue;
        };
        let Some(next_total) = receipt.removed_bytes.checked_add(store_bytes) else {
            receipt.limit_reached = true;
            break;
        };
        if receipt.removed >= MAX_REMOVED_STORES || next_total > MAX_SCAVENGE_TOTAL_BYTES {
            receipt.limit_reached = true;
            break;
        }

        match fs::remove_dir_all(&store_path) {
            Ok(()) => {
                receipt.removed += 1;
                receipt.removed_bytes = next_total;
            }
            Err(_) => receipt.cleanup_failures += 1,
        }
        drop(marker);
    }

    Ok(receipt)
}

pub(super) struct OwnerMarker {
    file: FileCapability,
    root: DirectoryCapability,
}

impl OwnerMarker {
    fn create(
        root: &DirectoryCapability,
        store_id: &str,
        created_unix_millis: u64,
    ) -> Result<Self, GenerationStoreError> {
        let bytes = owner_marker_bytes(store_id, created_unix_millis);
        let marker_name = component(OWNER_MARKER_NAME)?;
        let mut file = root.create_child_file(&marker_name).map_err(|source| {
            GenerationStoreError::Capability {
                operation: "create-owner-marker",
                source,
            }
        })?;
        {
            let mut writer = file
                .writer()
                .map_err(|source| GenerationStoreError::Capability {
                    operation: "write-owner-marker",
                    source,
                })?;
            writer
                .write_all(&bytes)
                .and_then(|()| writer.flush())
                .map_err(|source| GenerationStoreError::Io {
                    operation: "write-owner-marker",
                    source,
                })?;
        }
        file.sync()
            .map_err(|source| GenerationStoreError::Capability {
                operation: "sync-owner-marker",
                source,
            })?;
        retain_exclusive_lock(&file)?;
        Ok(Self {
            file,
            root: root.share(),
        })
    }

    fn read_bounded(&self) -> Result<Vec<u8>, GenerationStoreError> {
        self.file
            .read_to_end_bounded(MAX_OWNER_MARKER_BYTES)
            .map_err(|source| GenerationStoreError::Capability {
                operation: "read-owned-owner-marker",
                source,
            })
    }

    pub(super) fn named_binding_is_current(&self) -> bool {
        let Ok(marker_name) = component(OWNER_MARKER_NAME) else {
            return false;
        };
        let Ok(named) = self.root.open_child_file(&marker_name) else {
            return false;
        };
        self.file.identity() == named.identity()
            && self.file.parent_identity() == Some(self.root.identity())
            && named.parent_identity() == Some(self.root.identity())
            && self.file.link_count() == 1
            && named.link_count() == 1
            && matches!(
                named.try_lock(LockMode::Exclusive),
                Err(nara::fs::FsError::LockContended)
            )
    }
}

enum InactiveMarkerObservation {
    Live,
    Inactive(OwnerMarker),
}

fn inspect_inactive_marker(
    store_path: &Path,
    expected_store_id: &str,
    oldest_allowed: u64,
) -> Option<InactiveMarkerObservation> {
    let root = open_directory_capability(store_path).ok()?;
    let marker_name = component(OWNER_MARKER_NAME).ok()?;
    let capability = root.open_child_file(&marker_name).ok()?;
    match retain_exclusive_lock(&capability) {
        Ok(()) => {
            let bytes = capability
                .read_to_end_bounded(MAX_OWNER_MARKER_BYTES)
                .ok()?;
            let facts = parse_owner_marker(&bytes)?;
            if facts.store_id != expected_store_id || facts.created_unix_millis > oldest_allowed {
                return None;
            }
            Some(InactiveMarkerObservation::Inactive(OwnerMarker {
                file: capability,
                root,
            }))
        }
        Err(GenerationStoreError::Capability {
            source: nara::fs::FsError::LockContended,
            ..
        }) => Some(InactiveMarkerObservation::Live),
        Err(_) => None,
    }
}

fn retain_exclusive_lock(file: &FileCapability) -> Result<(), GenerationStoreError> {
    let lock =
        file.try_lock(LockMode::Exclusive)
            .map_err(|source| GenerationStoreError::Capability {
                operation: "lock-owner-marker",
                source,
            })?;
    // The operating-system lock is tied to the opened file. Keeping the capability alive while
    // forgetting this unlocking guard intentionally retains ownership until that file closes.
    std::mem::forget(lock);
    Ok(())
}

fn ensure_namespace(base_root: &Path) -> Result<PathBuf, GenerationStoreError> {
    fs::create_dir_all(base_root).map_err(|source| GenerationStoreError::Io {
        operation: "create-store-base",
        source,
    })?;
    let namespace = base_root.join(STORE_NAMESPACE);
    fs::create_dir_all(&namespace).map_err(|source| GenerationStoreError::Io {
        operation: "create-store-namespace",
        source,
    })?;
    let metadata = fs::symlink_metadata(&namespace).map_err(|source| GenerationStoreError::Io {
        operation: "inspect-store-namespace",
        source,
    })?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(GenerationStoreError::NamespaceNotPlainDirectory);
    }
    fs::canonicalize(namespace).map_err(|source| GenerationStoreError::Io {
        operation: "canonicalize-store-namespace",
        source,
    })
}

fn open_directory_capability(path: &Path) -> Result<DirectoryCapability, GenerationStoreError> {
    let file = open_directory_handle(path).map_err(|source| GenerationStoreError::Io {
        operation: "open-run-store",
        source,
    })?;
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .map_err(|source| GenerationStoreError::Capability {
        operation: "admit-run-store",
        source,
    })
}

pub(super) fn validate_owned_store(
    namespace: &Path,
    store_path: &Path,
    expected_store_id: &str,
    owner_marker: &OwnerMarker,
) -> Result<(), GenerationStoreError> {
    if store_path.parent() != Some(namespace)
        || store_path.file_name().and_then(|name| name.to_str()) != Some(expected_store_id)
        || !valid_store_id(expected_store_id)
    {
        return Err(GenerationStoreError::StoreIdentityInvalid);
    }
    let marker = owner_marker.read_bounded()?;
    if marker.len() as u64 > MAX_OWNER_MARKER_BYTES
        || parse_owner_marker(&marker).is_none_or(|facts| facts.store_id != expected_store_id)
        || !owner_marker.named_binding_is_current()
        || measure_valid_store_tree(store_path).is_none()
    {
        return Err(GenerationStoreError::StoreIdentityInvalid);
    }
    Ok(())
}

pub(super) fn measure_valid_store_tree(store_path: &Path) -> Option<u64> {
    measure_store_tree(store_path, false)
}

fn measure_scavenge_store_tree(store_path: &Path) -> Option<u64> {
    measure_store_tree(store_path, true)
}

fn measure_store_tree(store_path: &Path, allow_work_directory: bool) -> Option<u64> {
    let metadata = fs::symlink_metadata(store_path).ok()?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return None;
    }
    let mut total = 0_u64;
    let mut generation_directories = 0_usize;
    let mut work_directory_observed = false;
    for entry in fs::read_dir(store_path).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name = name.to_str()?;
        let metadata = fs::symlink_metadata(entry.path()).ok()?;
        if metadata.file_type().is_symlink() {
            return None;
        }
        if name == OWNER_MARKER_NAME && metadata.file_type().is_file() {
            if metadata.len() > MAX_OWNER_MARKER_BYTES {
                return None;
            }
            total = total.checked_add(metadata.len())?;
            continue;
        }
        if valid_work_directory_name(name) {
            if !allow_work_directory || work_directory_observed || !metadata.file_type().is_dir() {
                return None;
            }
            work_directory_observed = true;
            total = total.checked_add(measure_bounded_work_tree(&entry.path())?)?;
            if total > MAX_SCAVENGE_STORE_BYTES {
                return None;
            }
            continue;
        }
        if !valid_generation_directory_name(name) || !metadata.file_type().is_dir() {
            return None;
        }
        generation_directories = generation_directories.checked_add(1)?;
        if generation_directories > MAX_GENERATION_DIRECTORIES_PER_STORE {
            return None;
        }
        let mut files = 0_usize;
        for child in fs::read_dir(entry.path()).ok()? {
            let child = child.ok()?;
            let child_name = child.file_name();
            let child_name = child_name.to_str()?;
            let child_metadata = fs::symlink_metadata(child.path()).ok()?;
            if child_metadata.file_type().is_symlink() || !child_metadata.file_type().is_file() {
                return None;
            }
            files = files.checked_add(1)?;
            if files > MAX_GENERATION_FILES_PER_DIRECTORY {
                return None;
            }
            match child_name {
                EXECUTABLE_NAME
                    if child_metadata.len() <= MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES => {}
                LAUNCH_MANIFEST_NAME
                    if child_metadata.len() <= MAX_LAUNCH_MANIFEST_BYTES as u64 => {}
                _ => return None,
            }
            total = total.checked_add(child_metadata.len())?;
            if total > MAX_SCAVENGE_STORE_BYTES {
                return None;
            }
        }
    }
    Some(total)
}

fn measure_bounded_work_tree(work_directory: &Path) -> Option<u64> {
    let metadata = fs::symlink_metadata(work_directory).ok()?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return None;
    }

    let mut total = 0_u64;
    let mut entries_observed = 0_usize;
    let mut pending = vec![(work_directory.to_path_buf(), 0_usize)];
    while let Some((directory, depth)) = pending.pop() {
        for entry in fs::read_dir(directory).ok()? {
            let entry = entry.ok()?;
            entries_observed = entries_observed.checked_add(1)?;
            if entries_observed > MAX_SCAVENGE_WORK_ENTRIES_PER_STORE {
                return None;
            }
            let metadata = fs::symlink_metadata(entry.path()).ok()?;
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                return None;
            }
            if file_type.is_file() {
                total = total.checked_add(metadata.len())?;
                if total > MAX_SCAVENGE_STORE_BYTES {
                    return None;
                }
            } else if file_type.is_dir() {
                let child_depth = depth.checked_add(1)?;
                if child_depth > MAX_SCAVENGE_WORK_DEPTH {
                    return None;
                }
                pending.push((entry.path(), child_depth));
            } else {
                return None;
            }
        }
    }
    Some(total)
}

struct OwnerMarkerFacts<'a> {
    store_id: &'a str,
    created_unix_millis: u64,
}

fn owner_marker_bytes(store_id: &str, created_unix_millis: u64) -> Vec<u8> {
    format!("{OWNER_MARKER_MAGIC}\nstore_id={store_id}\ncreated_unix_ms={created_unix_millis}\n")
        .into_bytes()
}

fn parse_owner_marker(bytes: &[u8]) -> Option<OwnerMarkerFacts<'_>> {
    if bytes.len() as u64 > MAX_OWNER_MARKER_BYTES {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.split_terminator('\n');
    if lines.next()? != OWNER_MARKER_MAGIC {
        return None;
    }
    let store_id = lines.next()?.strip_prefix("store_id=")?;
    let created_unix_millis = lines
        .next()?
        .strip_prefix("created_unix_ms=")?
        .parse()
        .ok()?;
    if lines.next().is_some() || !text.ends_with('\n') || !valid_store_id(store_id) {
        return None;
    }
    Some(OwnerMarkerFacts {
        store_id,
        created_unix_millis,
    })
}

fn valid_store_id(value: &str) -> bool {
    let mut parts = value.split('-');
    matches!(parts.next(), Some("run"))
        && parts.next().is_some_and(|part| is_lower_hex(part, 16))
        && parts.next().is_some_and(|part| is_lower_hex(part, 8))
        && parts.next().is_some_and(|part| is_lower_hex(part, 16))
        && parts.next().is_none()
}

fn unix_millis(time: SystemTime) -> Result<u64, GenerationStoreError> {
    let millis = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| GenerationStoreError::ClockBeforeEpoch)?
        .as_millis();
    u64::try_from(millis).map_err(|_| GenerationStoreError::ClockOutOfRange)
}
