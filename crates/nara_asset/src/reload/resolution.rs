use std::{
    collections::{BTreeMap, BTreeSet, VecDeque, btree_map::Entry},
    sync::Arc,
};

use nara_ecs::{Res, ResMut};

use crate::{
    AssetDependencyGraph, AssetError, AssetEventKind, AssetEvents, AssetId, AssetPath, AssetRecord,
    AssetServer, AssetSourceKind, AssetStates, AssetVersion, ProjectAssetDatabase, StableAssetId,
};

use super::diagnostics::{
    asset_error_identifier, source_change_resolution_failure_diagnostic,
    unclaimed_reload_request_diagnostic,
};
use super::requests::{
    AssetLoadGenerations, AssetReloadRequestAdmissionError, AssetReloadRequestKind,
    AssetReloadRequestLimitKind, AssetReloadRequests, ImageReloadRegistration,
};
use super::{AssetReloadDiagnostics, AssetSourceChange, AssetSourceChangeKind, AssetSourceChanges};

pub(super) const UNCLAIMED_RELOAD_REQUEST_CODE: &str = "asset.reload-consumer-did-not-claim";

#[derive(Debug)]
pub(super) enum AssetReloadResolutionError {
    RecordMissing {
        path: AssetPath,
        change_kind: AssetSourceChangeKind,
    },
    DependentRecordMissing {
        path: AssetPath,
        change_kind: AssetSourceChangeKind,
    },
    SourceConsumerMissing {
        path: AssetPath,
        source_kind: AssetSourceKind,
        change_kind: AssetSourceChangeKind,
    },
    RequestAdmission {
        path: AssetPath,
        source_kind: AssetSourceKind,
        change_kind: AssetSourceChangeKind,
        source: AssetReloadRequestAdmissionError,
    },
    Asset {
        path: AssetPath,
        change_kind: AssetSourceChangeKind,
        source: AssetError,
    },
}

#[derive(Clone, Copy)]
pub(super) struct AssetReloadDiagnosticContext<'a> {
    pub(super) code: &'static str,
    pub(super) summary: &'static str,
    pub(super) path: &'a AssetPath,
    pub(super) change_kind: AssetSourceChangeKind,
    pub(super) source_kind: Option<&'a AssetSourceKind>,
    pub(super) reason: &'static str,
    pub(super) admission: Option<&'a AssetReloadRequestAdmissionError>,
}

impl AssetReloadResolutionError {
    pub(super) fn diagnostic_context(&self) -> AssetReloadDiagnosticContext<'_> {
        match self {
            Self::RecordMissing { path, change_kind } => AssetReloadDiagnosticContext {
                code: "asset.reload-record-missing",
                summary: "Asset reload source record was missing",
                path,
                change_kind: *change_kind,
                source_kind: None,
                reason: "record-missing",
                admission: None,
            },
            Self::DependentRecordMissing { path, change_kind } => AssetReloadDiagnosticContext {
                code: "asset.reload-dependent-record-missing",
                summary: "Dependent asset reload record was missing",
                path,
                change_kind: *change_kind,
                source_kind: None,
                reason: "dependent-record-missing",
                admission: None,
            },
            Self::SourceConsumerMissing {
                path,
                source_kind,
                change_kind,
            } => AssetReloadDiagnosticContext {
                code: "asset.reload-source-consumer-missing",
                summary: "Asset reload source consumer was missing",
                path,
                change_kind: *change_kind,
                source_kind: Some(source_kind),
                reason: "source-consumer-missing",
                admission: None,
            },
            Self::RequestAdmission {
                path,
                source_kind,
                change_kind,
                source,
            } => AssetReloadDiagnosticContext {
                code: source.kind().diagnostic_code(),
                summary: source.kind().safe_summary(),
                path,
                change_kind: *change_kind,
                source_kind: Some(source_kind),
                reason: match source.kind() {
                    AssetReloadRequestLimitKind::Items => "request-item-limit-exceeded",
                    AssetReloadRequestLimitKind::RetainedBytes => "request-byte-limit-exceeded",
                },
                admission: Some(source),
            },
            Self::Asset {
                path,
                change_kind,
                source,
            } => AssetReloadDiagnosticContext {
                code: "asset.reload-source-change-resolve-failed",
                summary: "Asset source change resolution failed",
                path,
                change_kind: *change_kind,
                source_kind: None,
                reason: asset_error_identifier(source),
                admission: None,
            },
        }
    }

    fn diagnostic_code(&self) -> &'static str {
        self.diagnostic_context().code
    }
}

struct AssetReloadResolutionContext<'a> {
    database: &'a ProjectAssetDatabase,
    dependencies: &'a AssetDependencyGraph,
    asset_server: &'a mut AssetServer,
    states: &'a mut AssetStates,
    generations: &'a mut AssetLoadGenerations,
    requests: &'a mut AssetReloadRequests,
    image_registration: Option<&'a ImageReloadRegistration>,
    events: &'a mut AssetEvents,
    diagnostics: &'a mut AssetReloadDiagnostics,
}

impl AssetReloadResolutionContext<'_> {
    fn resolve_changes(&mut self, changes: Vec<AssetSourceChange>) {
        let mut dependency_roots = BTreeMap::new();

        for change in changes {
            let Some(record) = self.database.record_for_path(change.path()).cloned() else {
                let asset_id = self.asset_server.id_for_path(change.path());
                if let Some(source_stable_id) =
                    asset_id.and_then(|asset_id| self.asset_server.stable_id(asset_id))
                {
                    dependency_roots.insert(source_stable_id, change.path().clone());
                }
                let error = AssetReloadResolutionError::RecordMissing {
                    path: change.path().clone(),
                    change_kind: change.kind(),
                };
                self.reject_known_record_gap(asset_id, error);
                continue;
            };

            dependency_roots.insert(record.stable_id(), record.path().clone());
            self.resolve_record(record, change.kind());
        }

        for (dependent, source) in
            transitive_dependent_origins(&dependency_roots, self.dependencies)
        {
            match self.database.record_for_stable_id(dependent).cloned() {
                Some(dependent_record) => {
                    self.resolve_record(dependent_record, AssetSourceChangeKind::Modified);
                }
                None => {
                    let error = AssetReloadResolutionError::DependentRecordMissing {
                        path: dependency_roots
                            .get(&source)
                            .expect("dependent origin comes from a direct source")
                            .clone(),
                        change_kind: AssetSourceChangeKind::Modified,
                    };
                    let asset_id = self.asset_server.id_for_stable_id(dependent);
                    self.reject_known_record_gap(asset_id, error);
                }
            }
        }
    }

    fn resolve_record(&mut self, record: AssetRecord, change_kind: AssetSourceChangeKind) {
        if let Err(error) = self.enqueue_record(record, change_kind) {
            self.diagnostics
                .push(source_change_resolution_failure_diagnostic(&error));
        }
    }

    fn enqueue_record(
        &mut self,
        record: AssetRecord,
        change_kind: AssetSourceChangeKind,
    ) -> Result<(), AssetReloadResolutionError> {
        let known_ids = [
            self.asset_server.id_for_path(record.path()),
            self.asset_server.id_for_stable_id(record.stable_id()),
        ];
        let asset_id = match self.asset_server.reserve_record_id(&record) {
            Ok(asset_id) => asset_id,
            Err(source) => {
                let error = AssetReloadResolutionError::Asset {
                    path: record.path().clone(),
                    change_kind,
                    source,
                };
                let diagnostic_code = error.diagnostic_code();
                for known_id in known_ids.into_iter().flatten().collect::<BTreeSet<_>>() {
                    self.invalidate_and_reject(known_id, diagnostic_code);
                }
                return Err(error);
            }
        };
        let generation = self.generations.begin_request(asset_id);
        let image_authority = match (record.source_kind(), self.image_registration) {
            (AssetSourceKind::Image, Some(registration)) => Arc::clone(&registration.authority),
            _ => {
                let error = AssetReloadResolutionError::SourceConsumerMissing {
                    path: record.path().clone(),
                    source_kind: record.source_kind().clone(),
                    change_kind,
                };
                return Err(self.reject_request(asset_id, error));
            }
        };

        let request_kind = if change_kind == AssetSourceChangeKind::Removed {
            AssetReloadRequestKind::Remove
        } else {
            AssetReloadRequestKind::LoadOrReload
        };
        if let Err(source) = self.requests.check_item_capacity() {
            let error = AssetReloadResolutionError::RequestAdmission {
                path: record.path().clone(),
                source_kind: record.source_kind().clone(),
                change_kind,
                source,
            };
            return Err(self.reject_request(asset_id, error));
        }

        let expected_version = self.states.version(asset_id).unwrap_or(AssetVersion::ZERO);
        let affected_artifacts = self.dependencies.affected_artifacts(record.stable_id());
        let request_path = record.path().clone();
        let request_source_kind = record.source_kind().clone();

        if let Err(source) = self.requests.try_push_resolved(
            asset_id,
            record,
            request_kind,
            change_kind,
            expected_version,
            generation,
            affected_artifacts,
            image_authority,
        ) {
            let error = AssetReloadResolutionError::RequestAdmission {
                path: request_path,
                source_kind: request_source_kind,
                change_kind,
                source,
            };
            return Err(self.reject_request(asset_id, error));
        }
        let loading_version = self.states.set_loading(asset_id);
        debug_assert_eq!(loading_version, expected_version);
        Ok(())
    }

    fn reject_known_record_gap(
        &mut self,
        asset_id: Option<AssetId>,
        error: AssetReloadResolutionError,
    ) {
        let error = match asset_id {
            Some(asset_id) => {
                self.generations.begin_request(asset_id);
                self.reject_request(asset_id, error)
            }
            None => error,
        };
        self.diagnostics
            .push(source_change_resolution_failure_diagnostic(&error));
    }

    fn invalidate_and_reject(&mut self, asset_id: AssetId, diagnostic_code: &'static str) {
        self.generations.begin_request(asset_id);
        self.reject_asset(asset_id, diagnostic_code);
    }

    fn reject_request(
        &mut self,
        asset_id: AssetId,
        error: AssetReloadResolutionError,
    ) -> AssetReloadResolutionError {
        self.reject_asset(asset_id, error.diagnostic_code());
        error
    }

    fn reject_asset(&mut self, asset_id: AssetId, diagnostic_code: &'static str) {
        reject_asset_reload(self.states, self.events, asset_id, diagnostic_code);
    }
}

fn reject_asset_reload(
    states: &mut AssetStates,
    events: &mut AssetEvents,
    asset_id: AssetId,
    diagnostic_code: &'static str,
) {
    let version = states.reject_reload_request(asset_id, diagnostic_code);
    let _ = events.push(asset_id, version, AssetEventKind::ReloadRejected);
}

fn transitive_dependent_origins(
    sources: &BTreeMap<StableAssetId, AssetPath>,
    dependencies: &AssetDependencyGraph,
) -> BTreeMap<StableAssetId, StableAssetId> {
    let mut queue = VecDeque::new();
    let mut origins = BTreeMap::new();
    for source in sources.keys().copied() {
        for dependent in dependencies.dependents_for_source(source) {
            enqueue_dependent_origin(dependent, source, sources, &mut origins, &mut queue);
        }
    }

    while let Some(dependent) = queue.pop_front() {
        let source = origins[&dependent];
        for next in dependencies.dependents_for_source(dependent) {
            enqueue_dependent_origin(next, source, sources, &mut origins, &mut queue);
        }
    }

    origins
}

fn enqueue_dependent_origin(
    dependent: StableAssetId,
    source: StableAssetId,
    sources: &BTreeMap<StableAssetId, AssetPath>,
    origins: &mut BTreeMap<StableAssetId, StableAssetId>,
    queue: &mut VecDeque<StableAssetId>,
) {
    if sources.contains_key(&dependent) {
        return;
    }
    if let Entry::Vacant(origin) = origins.entry(dependent) {
        origin.insert(source);
        queue.push_back(dependent);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_asset_source_changes(
    mut changes: ResMut<AssetSourceChanges>,
    database: Res<ProjectAssetDatabase>,
    dependencies: Res<AssetDependencyGraph>,
    mut asset_server: ResMut<AssetServer>,
    mut states: ResMut<AssetStates>,
    mut generations: ResMut<AssetLoadGenerations>,
    mut requests: ResMut<AssetReloadRequests>,
    image_registration: Option<Res<ImageReloadRegistration>>,
    mut events: ResMut<AssetEvents>,
    mut diagnostics: ResMut<AssetReloadDiagnostics>,
) {
    diagnostics.clear();
    let changes = changes.drain_coalesced();
    let mut context = AssetReloadResolutionContext {
        database: &database,
        dependencies: &dependencies,
        asset_server: &mut asset_server,
        states: &mut states,
        generations: &mut generations,
        requests: &mut requests,
        image_registration: image_registration.as_deref(),
        events: &mut events,
        diagnostics: &mut diagnostics,
    };
    context.resolve_changes(changes);
}

pub(super) fn reject_unclaimed_asset_reload_requests(
    mut requests: ResMut<AssetReloadRequests>,
    generations: Res<AssetLoadGenerations>,
    mut states: ResMut<AssetStates>,
    mut events: ResMut<AssetEvents>,
    mut diagnostics: ResMut<AssetReloadDiagnostics>,
) {
    if requests.is_empty() {
        return;
    }
    for request in requests.drain_unclaimed() {
        if !generations.is_current(request.asset_id(), request.generation())
            || states.version(request.asset_id()) != Some(request.expected_version())
        {
            continue;
        }
        reject_asset_reload(
            &mut states,
            &mut events,
            request.asset_id(),
            UNCLAIMED_RELOAD_REQUEST_CODE,
        );
        diagnostics.push(unclaimed_reload_request_diagnostic(&request));
    }
}
