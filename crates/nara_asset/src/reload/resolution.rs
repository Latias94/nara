use std::{
    collections::{BTreeMap, BTreeSet, VecDeque, btree_map::Entry},
    sync::Arc,
};

use nara_ecs::{Res, ResMut};

use crate::{
    AssetDependencyGraph, AssetError, AssetEventKind, AssetEvents, AssetId, AssetRecord,
    AssetServer, AssetSourceKind, AssetSourcePath, AssetStates, AssetVersion, ProjectAssetDatabase,
    StableAssetId,
};

use super::diagnostics::{
    asset_error_identifier, source_change_resolution_failure_diagnostic,
    unclaimed_reload_request_diagnostic,
};
use super::requests::{
    AssetLoadGenerations, AssetReloadRequestAdmissionError, AssetReloadRequestKind,
    AssetReloadRequestLimitKind, AssetReloadRequests, SourceReloadRegistrations,
};
use super::{AssetReloadDiagnostics, AssetSourceChange, AssetSourceChangeKind, AssetSourceChanges};

pub(super) const UNCLAIMED_RELOAD_REQUEST_CODE: &str = "asset.reload-consumer-did-not-claim";

#[derive(Debug)]
pub(super) enum AssetReloadResolutionError {
    RecordMissing {
        path: AssetSourcePath,
        change_kind: AssetSourceChangeKind,
    },
    DependentRecordMissing {
        path: AssetSourcePath,
        change_kind: AssetSourceChangeKind,
    },
    SourceConsumerMissing {
        path: AssetSourcePath,
        source_kind: AssetSourceKind,
        change_kind: AssetSourceChangeKind,
    },
    RequestAdmission {
        path: AssetSourcePath,
        source_kind: AssetSourceKind,
        change_kind: AssetSourceChangeKind,
        source: AssetReloadRequestAdmissionError,
    },
    Asset {
        path: AssetSourcePath,
        change_kind: AssetSourceChangeKind,
        source: AssetError,
    },
}

#[derive(Clone, Copy)]
pub(super) struct AssetReloadDiagnosticContext<'a> {
    pub(super) code: &'static str,
    pub(super) summary: &'static str,
    pub(super) path: &'a AssetSourcePath,
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
    registrations: Option<&'a SourceReloadRegistrations>,
    events: &'a mut AssetEvents,
    diagnostics: &'a mut AssetReloadDiagnostics,
}

impl AssetReloadResolutionContext<'_> {
    fn resolve_changes(&mut self, changes: Vec<AssetSourceChange>) {
        let mut dependency_roots = BTreeMap::new();
        let mut raw_roots = BTreeMap::new();
        let mut resolved_roots = BTreeSet::new();

        for change in changes {
            let mut has_raw_subscribers = false;
            let metadata = change.raw_metadata_changed().then(|| {
                AssetSourcePath::new(format!("{}.meta", change.path().as_str()))
                    .expect("a metadata suffix preserves a canonical source path")
            });
            for path in [
                change.raw_source_changed().then_some(change.path()),
                metadata.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                for dependent in self.dependencies.raw_dependents_for_path(path) {
                    has_raw_subscribers = true;
                    raw_roots.entry(dependent).or_insert_with(|| path.clone());
                }
            }
            let Some(record) = self.database.record_for_path(change.path()).cloned() else {
                let asset_id = self.asset_server.id_for_path(change.path());
                if let Some(source_stable_id) =
                    asset_id.and_then(|asset_id| self.asset_server.stable_id(asset_id))
                {
                    dependency_roots.insert(source_stable_id, change.path().clone());
                    resolved_roots.insert(source_stable_id);
                }
                if !has_raw_subscribers || asset_id.is_some() {
                    let error = AssetReloadResolutionError::RecordMissing {
                        path: change.path().clone(),
                        change_kind: change.kind(),
                    };
                    self.reject_known_record_gap(asset_id, error);
                }
                continue;
            };

            dependency_roots.insert(record.stable_id(), record.path().clone());
            if resolved_roots.insert(record.stable_id()) {
                self.resolve_record_observed(
                    record,
                    change.kind(),
                    change.metadata_dirty(),
                    change.metadata_after_removal(),
                );
            }
        }

        for (dependent, path) in raw_roots {
            dependency_roots
                .entry(dependent)
                .or_insert_with(|| path.clone());
            if !resolved_roots.insert(dependent) {
                continue;
            }
            match self.database.record_for_stable_id(dependent).cloned() {
                Some(record) => self.resolve_record(record, AssetSourceChangeKind::Modified),
                None => {
                    let error = AssetReloadResolutionError::DependentRecordMissing {
                        path,
                        change_kind: AssetSourceChangeKind::Modified,
                    };
                    let asset_id = self.asset_server.id_for_stable_id(dependent);
                    self.reject_known_record_gap(asset_id, error);
                }
            }
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
                            .expect(
                                "dependent origin comes from a changed source or raw subscriber",
                            )
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
        self.resolve_record_observed(record, change_kind, false, false);
    }

    fn resolve_record_observed(
        &mut self,
        record: AssetRecord,
        change_kind: AssetSourceChangeKind,
        metadata_dirty: bool,
        metadata_after_removal: bool,
    ) {
        if let Err(error) =
            self.enqueue_record(record, change_kind, metadata_dirty, metadata_after_removal)
        {
            self.diagnostics
                .push(source_change_resolution_failure_diagnostic(&error));
        }
    }

    fn enqueue_record(
        &mut self,
        record: AssetRecord,
        change_kind: AssetSourceChangeKind,
        metadata_dirty: bool,
        metadata_after_removal: bool,
    ) -> Result<(), AssetReloadResolutionError> {
        let known_ids = [
            self.asset_server.id_for_path(record.path()),
            self.asset_server.id_for_stable_id(record.stable_id()),
        ];
        let registration = self
            .registrations
            .and_then(|registrations| registrations.for_kind(record.source_kind()));
        let image_recovery = registration.is_some_and(|registration| registration.image_recovery);
        if let Some(registration) = registration {
            for id in known_ids.into_iter().flatten() {
                if let Err(source) = (registration.validate_asset_type)(self.asset_server, id) {
                    return Err(AssetReloadResolutionError::Asset {
                        path: record.path().clone(),
                        change_kind,
                        source,
                    });
                }
            }
        }
        let affected_recipes = if registration.is_some() && !image_recovery {
            let affected_recipes = self.dependencies.affected_recipes(record.stable_id());
            if let Err(source) = self
                .requests
                .check_capacity_for(&record, affected_recipes.capacity())
            {
                for id in known_ids.into_iter().flatten().collect::<BTreeSet<_>>() {
                    self.observe_request(
                        id,
                        change_kind,
                        metadata_dirty,
                        metadata_after_removal,
                        false,
                    );
                    self.reject_asset(id, source.kind().diagnostic_code());
                }
                return Err(AssetReloadResolutionError::RequestAdmission {
                    path: record.path().clone(),
                    source_kind: record.source_kind().clone(),
                    change_kind,
                    source,
                });
            }
            Some(affected_recipes)
        } else {
            None
        };
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
        if let Some(registration) = registration {
            (registration.claim_asset_type)(self.asset_server, asset_id).map_err(|source| {
                AssetReloadResolutionError::Asset {
                    path: record.path().clone(),
                    change_kind,
                    source,
                }
            })?;
            if image_recovery {
                self.states
                    .bind_image_reload_authority(&registration.authority);
            }
        }
        let source_authority = registration.map(|registration| Arc::clone(&registration.authority));
        let previous_state = (
            self.states.state_revision(asset_id),
            self.states
                .state(asset_id)
                .and_then(|state| state.usable_version()),
        );
        let generation = self.observe_request(
            asset_id,
            change_kind,
            metadata_dirty,
            metadata_after_removal,
            image_recovery,
        );
        let source_authority = match source_authority {
            Some(authority) => authority,
            None => {
                let error = AssetReloadResolutionError::SourceConsumerMissing {
                    path: record.path().clone(),
                    source_kind: record.source_kind().clone(),
                    change_kind,
                };
                return Err(self.reject_request(asset_id, error, image_recovery));
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
            return Err(self.reject_request(asset_id, error, image_recovery));
        }

        let expected_version = self.states.version(asset_id).unwrap_or(AssetVersion::ZERO);
        let affected_recipes = affected_recipes
            .unwrap_or_else(|| self.dependencies.affected_recipes(record.stable_id()));
        let request_path = record.path().clone();
        let request_source_kind = record.source_kind().clone();

        let request = match self.requests.try_push_resolved(
            asset_id,
            record,
            request_kind,
            change_kind,
            expected_version,
            generation,
            Arc::clone(self.generations.store_identity()),
            affected_recipes,
            source_authority,
            Some(previous_state),
        ) {
            Ok(request) => request,
            Err(source) => {
                let error = AssetReloadResolutionError::RequestAdmission {
                    path: request_path,
                    source_kind: request_source_kind,
                    change_kind,
                    source,
                };
                return Err(self.reject_request(asset_id, error, image_recovery));
            }
        };
        let loading_version = if image_recovery {
            self.states.set_loading_for_image_reload(asset_id)
        } else {
            self.states.set_loading(asset_id)
        };
        request.set_resolved_state_revision(self.states.state_revision(asset_id));
        debug_assert_eq!(loading_version, expected_version);
        Ok(())
    }

    fn observe_request(
        &mut self,
        id: AssetId,
        change_kind: AssetSourceChangeKind,
        metadata_dirty: bool,
        metadata_after_removal: bool,
        image_recovery: bool,
    ) -> super::AssetLoadGeneration {
        let removed = change_kind == AssetSourceChangeKind::Removed;
        if removed && metadata_after_removal {
            self.observe_source_removal(id, image_recovery);
        }
        if metadata_dirty || change_kind == AssetSourceChangeKind::MetaModified {
            self.states.observe_metadata_change(id);
        }
        let generation = self.generations.begin_request(id);
        if removed && !metadata_after_removal {
            self.observe_source_removal(id, image_recovery);
        }
        generation
    }

    fn observe_source_removal(&mut self, id: AssetId, image_recovery: bool) {
        if image_recovery {
            self.states.observe_image_source_removal(id);
        } else {
            self.states.observe_source_removal(id);
        }
    }

    fn reject_known_record_gap(
        &mut self,
        asset_id: Option<AssetId>,
        error: AssetReloadResolutionError,
    ) {
        if let Some(asset_id) = asset_id {
            self.invalidate_and_reject(asset_id, error.diagnostic_code());
        }
        self.diagnostics
            .push(source_change_resolution_failure_diagnostic(&error));
    }

    fn invalidate_and_reject(&mut self, asset_id: AssetId, diagnostic_code: &'static str) {
        self.generations.begin_request(asset_id);
        self.states.invalidate_publication(asset_id);
        self.reject_asset(asset_id, diagnostic_code);
    }

    fn reject_request(
        &mut self,
        asset_id: AssetId,
        error: AssetReloadResolutionError,
        image_recovery: bool,
    ) -> AssetReloadResolutionError {
        if image_recovery && matches!(&error, AssetReloadResolutionError::RequestAdmission { .. }) {
            let version = self
                .states
                .reject_image_reload_capacity(asset_id, error.diagnostic_code());
            let _ = self
                .events
                .push(asset_id, version, AssetEventKind::ReloadRejected);
        } else {
            self.reject_asset(asset_id, error.diagnostic_code());
        }
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
    sources: &BTreeMap<StableAssetId, AssetSourcePath>,
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
    sources: &BTreeMap<StableAssetId, AssetSourcePath>,
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
    registrations: Option<Res<SourceReloadRegistrations>>,
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
        registrations: registrations.as_deref(),
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
        if !generations.is_current_request(&request)
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

#[cfg(test)]
#[path = "tests_recovery_continuity.rs"]
mod recovery_continuity_tests;

#[cfg(test)]
#[path = "tests_source_routing.rs"]
mod source_routing_tests;
