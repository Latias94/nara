use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
};

use nara_app::{App, CoreStage, Plugin, PluginError, TaskUpdateSet};
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, SafeSummary,
};
use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};

use crate::{
    AssetDatabaseError, AssetDependencyGraph, AssetError, AssetId, AssetPath, AssetRecord,
    AssetServer, AssetSourceKind, AssetStates, AssetVersion, ImportArtifactDigest,
    ProjectAssetDatabase, StableAssetId,
};

#[derive(Debug, Clone, PartialEq, Eq, Resource)]
pub struct AssetSourceRoot {
    root: PathBuf,
}

impl AssetSourceRoot {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn source_path(&self, path: &AssetPath) -> PathBuf {
        path.as_str()
            .split('/')
            .fold(self.root.clone(), |path, segment| path.join(segment))
    }

    pub fn logical_path_from_source_path(
        &self,
        source_path: impl AsRef<Path>,
    ) -> Result<AssetPath, AssetDatabaseError> {
        ProjectAssetDatabase::logical_path_from_source_path(&self.root, source_path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssetSourceChangeKind {
    MetaModified,
    Modified,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSourceChange {
    path: AssetPath,
    kind: AssetSourceChangeKind,
}

impl AssetSourceChange {
    #[must_use]
    pub const fn new(path: AssetPath, kind: AssetSourceChangeKind) -> Self {
        Self { path, kind }
    }

    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    #[must_use]
    pub const fn kind(&self) -> AssetSourceChangeKind {
        self.kind
    }
}

#[derive(Debug, Default, Resource)]
pub struct AssetSourceChanges {
    changes: Vec<AssetSourceChange>,
}

impl AssetSourceChanges {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, change: AssetSourceChange) {
        self.changes.push(change);
    }

    pub fn modified(&mut self, path: AssetPath) {
        self.push(AssetSourceChange::new(
            path,
            AssetSourceChangeKind::Modified,
        ));
    }

    pub fn meta_modified(&mut self, path: AssetPath) {
        self.push(AssetSourceChange::new(
            path,
            AssetSourceChangeKind::MetaModified,
        ));
    }

    pub fn removed(&mut self, path: AssetPath) {
        self.push(AssetSourceChange::new(path, AssetSourceChangeKind::Removed));
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = &AssetSourceChange> {
        self.changes.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn drain_coalesced(&mut self) -> Vec<AssetSourceChange> {
        let mut by_path = BTreeMap::<AssetPath, AssetSourceChangeKind>::new();
        for change in self.changes.drain(..) {
            by_path
                .entry(change.path)
                .and_modify(|kind| *kind = coalesce_change_kind(*kind, change.kind))
                .or_insert(change.kind);
        }

        by_path
            .into_iter()
            .map(|(path, kind)| AssetSourceChange::new(path, kind))
            .collect()
    }
}

fn coalesce_change_kind(
    _left: AssetSourceChangeKind,
    right: AssetSourceChangeKind,
) -> AssetSourceChangeKind {
    right
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetLoadGeneration(u64);

impl AssetLoadGeneration {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Default, Resource)]
pub struct AssetLoadGenerations {
    generations: BTreeMap<AssetId, AssetLoadGeneration>,
}

impl AssetLoadGenerations {
    #[must_use]
    pub fn current(&self, id: AssetId) -> AssetLoadGeneration {
        self.generations
            .get(&id)
            .copied()
            .unwrap_or(AssetLoadGeneration::ZERO)
    }

    pub fn begin_request(&mut self, id: AssetId) -> AssetLoadGeneration {
        let next = AssetLoadGeneration(self.current(id).raw().saturating_add(1));
        self.generations.insert(id, next);
        next
    }

    #[must_use]
    pub fn is_current(&self, id: AssetId, generation: AssetLoadGeneration) -> bool {
        self.current(id) == generation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetReloadRequestId(u64);

impl AssetReloadRequestId {
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetReloadRequestKind {
    LoadOrReload,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetReloadRequest {
    id: AssetReloadRequestId,
    asset_id: AssetId,
    stable_id: StableAssetId,
    path: AssetPath,
    source_kind: AssetSourceKind,
    request_kind: AssetReloadRequestKind,
    source_change_kind: AssetSourceChangeKind,
    expected_version: AssetVersion,
    generation: AssetLoadGeneration,
    affected_artifacts: Vec<ImportArtifactDigest>,
}

impl AssetReloadRequest {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        id: AssetReloadRequestId,
        asset_id: AssetId,
        record: &AssetRecord,
        request_kind: AssetReloadRequestKind,
        source_change_kind: AssetSourceChangeKind,
        expected_version: AssetVersion,
        generation: AssetLoadGeneration,
        affected_artifacts: Vec<ImportArtifactDigest>,
    ) -> Self {
        Self {
            id,
            asset_id,
            stable_id: record.stable_id(),
            path: record.path().clone(),
            source_kind: record.source_kind().clone(),
            request_kind,
            source_change_kind,
            expected_version,
            generation,
            affected_artifacts,
        }
    }

    #[must_use]
    pub const fn id(&self) -> AssetReloadRequestId {
        self.id
    }

    #[must_use]
    pub const fn asset_id(&self) -> AssetId {
        self.asset_id
    }

    #[must_use]
    pub const fn stable_id(&self) -> StableAssetId {
        self.stable_id
    }

    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    #[must_use]
    pub const fn source_kind(&self) -> &AssetSourceKind {
        &self.source_kind
    }

    #[must_use]
    pub const fn request_kind(&self) -> AssetReloadRequestKind {
        self.request_kind
    }

    #[must_use]
    pub const fn source_change_kind(&self) -> AssetSourceChangeKind {
        self.source_change_kind
    }

    #[must_use]
    pub const fn expected_version(&self) -> AssetVersion {
        self.expected_version
    }

    #[must_use]
    pub const fn generation(&self) -> AssetLoadGeneration {
        self.generation
    }

    #[must_use]
    pub fn affected_artifacts(&self) -> &[ImportArtifactDigest] {
        &self.affected_artifacts
    }

    #[must_use]
    pub fn record(&self) -> AssetRecord {
        AssetRecord::new(self.stable_id, self.path.clone(), self.source_kind.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedAssetSourceChange {
    change: AssetSourceChange,
}

impl UnresolvedAssetSourceChange {
    #[must_use]
    pub const fn new(change: AssetSourceChange) -> Self {
        Self { change }
    }

    #[must_use]
    pub const fn change(&self) -> &AssetSourceChange {
        &self.change
    }
}

#[derive(Debug, Default, Resource)]
pub struct AssetReloadRequests {
    next_id: u64,
    requests: Vec<AssetReloadRequest>,
    unresolved: Vec<UnresolvedAssetSourceChange>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Resource)]
pub struct AssetReloadDiagnostics {
    report: DiagnosticReport,
}

impl AssetReloadDiagnostics {
    pub fn clear(&mut self) {
        self.report = DiagnosticReport::default();
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.report.push(diagnostic);
    }

    #[must_use]
    pub const fn report(&self) -> &DiagnosticReport {
        &self.report
    }

    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Diagnostic> {
        self.report.iter()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.report.is_empty()
    }

    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.report.has_errors()
    }
}

impl AssetReloadRequests {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_resolved(
        &mut self,
        asset_id: AssetId,
        record: &AssetRecord,
        request_kind: AssetReloadRequestKind,
        source_change_kind: AssetSourceChangeKind,
        expected_version: AssetVersion,
        generation: AssetLoadGeneration,
        affected_artifacts: Vec<ImportArtifactDigest>,
    ) -> AssetReloadRequestId {
        let id = AssetReloadRequestId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.requests.push(AssetReloadRequest::new(
            id,
            asset_id,
            record,
            request_kind,
            source_change_kind,
            expected_version,
            generation,
            affected_artifacts,
        ));
        id
    }

    pub fn push_unresolved(&mut self, change: AssetSourceChange) {
        self.unresolved
            .push(UnresolvedAssetSourceChange::new(change));
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = &AssetReloadRequest> {
        self.requests.iter()
    }

    #[must_use]
    pub fn unresolved(&self) -> &[UnresolvedAssetSourceChange] {
        &self.unresolved
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.requests.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty() && self.unresolved.is_empty()
    }

    pub fn drain_for_source_kind(
        &mut self,
        source_kind: &AssetSourceKind,
    ) -> Vec<AssetReloadRequest> {
        let mut drained = Vec::new();
        let mut kept = Vec::new();

        for request in self.requests.drain(..) {
            if request.source_kind() == source_kind {
                drained.push(request);
            } else {
                kept.push(request);
            }
        }

        self.requests = kept;
        drained.sort_by_key(AssetReloadRequest::id);
        drained
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SourceChangeResolver;

impl SourceChangeResolver {
    pub fn resolve_change(
        &self,
        change: AssetSourceChange,
        database: &ProjectAssetDatabase,
        dependencies: &AssetDependencyGraph,
        asset_server: &mut AssetServer,
        states: &mut AssetStates,
        generations: &mut AssetLoadGenerations,
        requests: &mut AssetReloadRequests,
    ) -> Result<(), AssetError> {
        let Some(record) = database.record_for_path(change.path()) else {
            requests.push_unresolved(change);
            return Ok(());
        };

        self.enqueue_record(
            record,
            change.kind(),
            dependencies,
            asset_server,
            states,
            generations,
            requests,
        )?;

        for dependent in transitive_dependents(record.stable_id(), dependencies) {
            if dependent == record.stable_id() {
                continue;
            }
            if let Some(dependent_record) = database.record_for_stable_id(dependent) {
                self.enqueue_record(
                    dependent_record,
                    AssetSourceChangeKind::Modified,
                    dependencies,
                    asset_server,
                    states,
                    generations,
                    requests,
                )?;
            }
        }

        Ok(())
    }

    fn enqueue_record(
        &self,
        record: &AssetRecord,
        change_kind: AssetSourceChangeKind,
        dependencies: &AssetDependencyGraph,
        asset_server: &mut AssetServer,
        states: &mut AssetStates,
        generations: &mut AssetLoadGenerations,
        requests: &mut AssetReloadRequests,
    ) -> Result<(), AssetError> {
        let asset_id = asset_server.reserve_record_id(record)?;
        let expected_version = states.set_loading(asset_id);
        let generation = generations.begin_request(asset_id);
        let request_kind = if change_kind == AssetSourceChangeKind::Removed {
            AssetReloadRequestKind::Remove
        } else {
            AssetReloadRequestKind::LoadOrReload
        };
        let affected_artifacts = dependencies.affected_artifacts(record.stable_id());

        requests.push_resolved(
            asset_id,
            record,
            request_kind,
            change_kind,
            expected_version,
            generation,
            affected_artifacts,
        );
        Ok(())
    }
}

fn transitive_dependents(
    source: StableAssetId,
    dependencies: &AssetDependencyGraph,
) -> Vec<StableAssetId> {
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    for dependent in dependencies.dependents_for_source(source) {
        queue.push_back(dependent);
    }

    while let Some(dependent) = queue.pop_front() {
        if !visited.insert(dependent) {
            continue;
        }
        for next in dependencies.dependents_for_source(dependent) {
            queue.push_back(next);
        }
    }

    visited.into_iter().collect()
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AssetPlugin;

pub const ASSET_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.asset");
pub const ASSET_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(ASSET_PLUGIN_ID, nara_app::PluginCategory::Asset)
        .requires_plugins(&[nara_tasks::TASK_PLUGIN_ID]);

impl Plugin for AssetPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &ASSET_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<AssetServer>()?;
        app.init_resource::<AssetStates>()?;
        app.init_resource::<crate::AssetEvents>()?;
        app.init_resource::<AssetDependencyGraph>()?;
        app.init_resource::<ProjectAssetDatabase>()?;
        app.init_resource::<AssetSourceChanges>()?;
        app.init_resource::<AssetReloadRequests>()?;
        app.init_resource::<AssetReloadDiagnostics>()?;
        app.init_resource::<AssetLoadGenerations>()?;
        app.configure_sets(
            CoreStage::TaskUpdate,
            (
                TaskUpdateSet::Poll,
                TaskUpdateSet::CoalesceAssetChanges,
                TaskUpdateSet::SpawnAssetJobs,
                TaskUpdateSet::ApplyAssetResults,
            )
                .chain(),
        )?;
        app.add_systems(
            CoreStage::TaskUpdate,
            resolve_asset_source_changes.in_set(TaskUpdateSet::CoalesceAssetChanges),
        )?;
        Ok(())
    }
}

pub fn resolve_asset_source_changes(
    mut changes: ResMut<AssetSourceChanges>,
    database: Res<ProjectAssetDatabase>,
    dependencies: Res<AssetDependencyGraph>,
    mut asset_server: ResMut<AssetServer>,
    mut states: ResMut<AssetStates>,
    mut generations: ResMut<AssetLoadGenerations>,
    mut requests: ResMut<AssetReloadRequests>,
    mut diagnostics: ResMut<AssetReloadDiagnostics>,
) {
    diagnostics.clear();
    let resolver = SourceChangeResolver;
    for change in changes.drain_coalesced() {
        let diagnostic_path = change.path().clone();
        let diagnostic_kind = change.kind();
        if let Err(error) = resolver.resolve_change(
            change,
            &database,
            &dependencies,
            &mut asset_server,
            &mut states,
            &mut generations,
            &mut requests,
        ) {
            diagnostics.push(source_change_resolution_failure_diagnostic(
                &diagnostic_path,
                diagnostic_kind,
                &error,
            ));
        }
    }
}

fn source_change_resolution_failure_diagnostic(
    path: &AssetPath,
    change_kind: AssetSourceChangeKind,
    error: &AssetError,
) -> Diagnostic {
    let diagnostic = Diagnostic::error(
        DiagnosticCode::new("asset.reload-source-change-resolve-failed")
            .expect("asset diagnostic code literals must be valid"),
        SafeSummary::new("Asset source change resolution failed")
            .expect("asset diagnostic summaries must be safe literals"),
    );
    let path_key = diagnostic_field_key("asset-path");
    let path_field = DiagnosticField::project_relative(path_key, path.as_str())
        .unwrap_or_else(|_| DiagnosticField::sensitive(path_key));
    let diagnostic = with_diagnostic_field(diagnostic, path_field);
    let diagnostic = with_diagnostic_field(
        diagnostic,
        DiagnosticField::public_identifier(
            diagnostic_field_key("change-kind"),
            PublicDiagnosticIdentifier::new(source_change_kind_identifier(change_kind))
                .expect("asset source change kind identifiers must be valid"),
        ),
    );
    let diagnostic = with_diagnostic_field(
        diagnostic,
        DiagnosticField::public_identifier(
            diagnostic_field_key("reason"),
            PublicDiagnosticIdentifier::new(asset_error_identifier(error))
                .expect("asset error identifiers must be valid"),
        ),
    );
    with_diagnostic_field(
        diagnostic,
        DiagnosticField::sensitive(diagnostic_field_key("error-detail")),
    )
}

fn source_change_kind_identifier(kind: AssetSourceChangeKind) -> &'static str {
    match kind {
        AssetSourceChangeKind::MetaModified => "meta-modified",
        AssetSourceChangeKind::Modified => "modified",
        AssetSourceChangeKind::Removed => "removed",
    }
}

fn asset_error_identifier(error: &AssetError) -> &'static str {
    match error {
        AssetError::IdSpaceExhausted => "id-space-exhausted",
        AssetError::InvalidPath(_) => "invalid-path",
        AssetError::ConflictingAssetIdentity { .. } => "conflicting-asset-identity",
        AssetError::PathAlreadyBound { .. } => "path-already-bound",
        AssetError::StableIdAlreadyBound { .. } => "stable-id-already-bound",
        AssetError::AssetIdAlreadyBoundToPath { .. } => "asset-id-already-bound-to-path",
        AssetError::AssetIdAlreadyBoundToStableId { .. } => "asset-id-already-bound-to-stable-id",
    }
}

fn diagnostic_field_key(value: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(value).expect("asset diagnostic field key literals must be valid")
}

fn with_diagnostic_field(diagnostic: Diagnostic, field: DiagnosticField) -> Diagnostic {
    diagnostic
        .try_with_field(field)
        .expect("asset diagnostics must use unique fields within the hard field limit")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetEventKind, AssetEvents, Assets};

    fn stable_id() -> StableAssetId {
        StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap()
    }

    fn dependent_stable_id() -> StableAssetId {
        StableAssetId::parse_str("b73f0f16-09e8-4265-b090-b689b41c197e").unwrap()
    }

    fn transitive_dependent_stable_id() -> StableAssetId {
        StableAssetId::parse_str("d87c8f9d-0dc2-4863-8e9c-d3e6eaa8d41f").unwrap()
    }

    fn image_record(path: &str, stable_id: StableAssetId) -> AssetRecord {
        AssetRecord::new(
            stable_id,
            AssetPath::new(path).unwrap(),
            AssetSourceKind::Image,
        )
    }

    fn install_asset_plugin(app: &mut App) {
        app.add_plugins((nara_tasks::TaskPlugin::default(), AssetPlugin))
            .unwrap();
    }

    #[test]
    fn source_changes_coalesce_duplicate_frame_events() {
        let mut changes = AssetSourceChanges::new();
        let path = AssetPath::new("textures/player.png").unwrap();
        changes.meta_modified(path.clone());
        changes.modified(path.clone());
        changes.removed(path.clone());

        let coalesced = changes.drain_coalesced();

        assert_eq!(
            coalesced,
            vec![AssetSourceChange::new(path, AssetSourceChangeKind::Removed)]
        );
        assert!(changes.is_empty());
    }

    #[test]
    fn source_changes_keep_last_semantic_event_for_atomic_save_sequences() {
        let mut changes = AssetSourceChanges::new();
        let path = AssetPath::new("textures/player.png").unwrap();
        changes.removed(path.clone());
        changes.modified(path.clone());

        let coalesced = changes.drain_coalesced();

        assert_eq!(
            coalesced,
            vec![AssetSourceChange::new(
                path,
                AssetSourceChangeKind::Modified
            )]
        );
    }

    #[test]
    fn asset_plugin_resolves_manual_changes_into_generation_requests() {
        let mut app = App::new();
        app.insert_resource(
            nara_tasks::TaskPools::inline_for_tests(nara_tasks::TaskPoolConfig::default()).unwrap(),
        )
        .unwrap();
        install_asset_plugin(&mut app);
        app.world_mut()
            .unwrap()
            .resource_mut::<ProjectAssetDatabase>()
            .insert(image_record("textures/player.png", stable_id()))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<AssetSourceChanges>()
            .modified(AssetPath::new("textures/player.png").unwrap());

        app.update().unwrap();

        let requests = app.world().resource::<AssetReloadRequests>();
        let request = requests.iter().next().unwrap();
        assert_eq!(request.path().as_str(), "textures/player.png");
        assert_eq!(request.generation().raw(), 1);
        assert_eq!(request.expected_version(), AssetVersion::ZERO);
        assert_eq!(request.request_kind(), AssetReloadRequestKind::LoadOrReload);
        assert_eq!(
            app.world()
                .resource::<AssetStates>()
                .state(request.asset_id())
                .unwrap()
                .load_state(),
            &crate::LoadState::Loading
        );
    }

    #[test]
    fn resolver_records_unresolved_changes_instead_of_guessing() {
        let mut app = App::new();
        app.insert_resource(
            nara_tasks::TaskPools::inline_for_tests(nara_tasks::TaskPoolConfig::default()).unwrap(),
        )
        .unwrap();
        install_asset_plugin(&mut app);
        app.world_mut()
            .unwrap()
            .resource_mut::<AssetSourceChanges>()
            .modified(AssetPath::new("textures/missing.png").unwrap());

        app.update().unwrap();

        let requests = app.world().resource::<AssetReloadRequests>();
        assert_eq!(requests.len(), 0);
        assert_eq!(requests.unresolved().len(), 1);
    }

    #[test]
    fn resolver_errors_are_recorded_as_reload_diagnostics() {
        let mut app = App::new();
        app.insert_resource(
            nara_tasks::TaskPools::inline_for_tests(nara_tasks::TaskPoolConfig::default()).unwrap(),
        )
        .unwrap();
        install_asset_plugin(&mut app);
        app.world_mut()
            .unwrap()
            .resource_mut::<ProjectAssetDatabase>()
            .insert(image_record("textures/player.png", stable_id()))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<AssetServer>()
            .reserve_record_id(&image_record("textures/player.png", dependent_stable_id()))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<AssetSourceChanges>()
            .modified(AssetPath::new("textures/player.png").unwrap());

        app.update().unwrap();

        let diagnostics = app.world().resource::<AssetReloadDiagnostics>();
        assert!(diagnostics.has_errors());
        assert_eq!(diagnostics.iter().len(), 1);
        let diagnostic = diagnostics.iter().next().unwrap();
        assert_eq!(
            diagnostic.code().as_str(),
            "asset.reload-source-change-resolve-failed"
        );
        assert_eq!(
            diagnostic.summary().as_str(),
            "Asset source change resolution failed"
        );
        let field = |key: &str| {
            diagnostic
                .fields()
                .iter()
                .find(|field| field.key().as_str() == key)
                .unwrap()
        };
        assert_eq!(
            field("asset-path").value(),
            nara_diagnostic::DiagnosticValueRef::ProjectRelative("textures/player.png")
        );
        assert_eq!(
            field("change-kind").value(),
            nara_diagnostic::DiagnosticValueRef::Identifier("modified")
        );
        assert_eq!(
            field("reason").value(),
            nara_diagnostic::DiagnosticValueRef::Identifier("asset-id-already-bound-to-stable-id")
        );
        assert_eq!(
            field("error-detail").class(),
            nara_diagnostic::DiagnosticFieldClass::Sensitive
        );
        assert_eq!(
            field("error-detail").value(),
            nara_diagnostic::DiagnosticValueRef::Redacted
        );
        let diagnostic_debug = format!("{diagnostic:?}");
        assert!(!diagnostic_debug.contains(&stable_id().to_string()));
        assert!(!diagnostic_debug.contains(&dependent_stable_id().to_string()));
        assert!(app.world().resource::<AssetReloadRequests>().is_empty());
    }

    #[test]
    fn dependency_records_enqueue_dependent_assets() {
        let mut app = App::new();
        app.insert_resource(
            nara_tasks::TaskPools::inline_for_tests(nara_tasks::TaskPoolConfig::default()).unwrap(),
        )
        .unwrap();
        install_asset_plugin(&mut app);
        {
            let mut database = app
                .world_mut()
                .unwrap()
                .resource_mut::<ProjectAssetDatabase>();
            database
                .insert(image_record("textures/source.png", stable_id()))
                .unwrap();
            database
                .insert(image_record(
                    "textures/dependent.png",
                    dependent_stable_id(),
                ))
                .unwrap();
        }
        app.world_mut()
            .unwrap()
            .resource_mut::<AssetDependencyGraph>()
            .add_source_dependency(stable_id(), dependent_stable_id());
        app.world_mut()
            .unwrap()
            .resource_mut::<AssetSourceChanges>()
            .modified(AssetPath::new("textures/source.png").unwrap());

        app.update().unwrap();

        let paths = app
            .world()
            .resource::<AssetReloadRequests>()
            .iter()
            .map(|request| request.path().as_str().to_string())
            .collect::<Vec<_>>();
        assert_eq!(paths, vec!["textures/source.png", "textures/dependent.png"]);
    }

    #[test]
    fn dependency_records_enqueue_transitive_dependents_once() {
        let mut app = App::new();
        app.insert_resource(
            nara_tasks::TaskPools::inline_for_tests(nara_tasks::TaskPoolConfig::default()).unwrap(),
        )
        .unwrap();
        install_asset_plugin(&mut app);
        {
            let mut database = app
                .world_mut()
                .unwrap()
                .resource_mut::<ProjectAssetDatabase>();
            database
                .insert(image_record("textures/source.png", stable_id()))
                .unwrap();
            database
                .insert(image_record(
                    "textures/dependent.png",
                    dependent_stable_id(),
                ))
                .unwrap();
            database
                .insert(image_record(
                    "textures/transitive.png",
                    transitive_dependent_stable_id(),
                ))
                .unwrap();
        }
        {
            let mut graph = app
                .world_mut()
                .unwrap()
                .resource_mut::<AssetDependencyGraph>();
            graph.add_source_dependency(stable_id(), dependent_stable_id());
            graph.add_source_dependency(dependent_stable_id(), transitive_dependent_stable_id());
            graph.add_source_dependency(stable_id(), transitive_dependent_stable_id());
        }
        app.world_mut()
            .unwrap()
            .resource_mut::<AssetSourceChanges>()
            .modified(AssetPath::new("textures/source.png").unwrap());

        app.update().unwrap();

        let paths = app
            .world()
            .resource::<AssetReloadRequests>()
            .iter()
            .map(|request| request.path().as_str().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            vec![
                "textures/source.png",
                "textures/dependent.png",
                "textures/transitive.png"
            ]
        );
    }

    #[test]
    fn first_load_failure_uses_distinct_event_without_value() {
        let mut assets = Assets::<String>::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        let mut server = AssetServer::new();
        let handle = server
            .reserve_record::<String>(&image_record("textures/player.png", stable_id()))
            .unwrap();
        states.set_loading(handle.id());

        assets
            .record_load_failure(handle, &mut states, &mut events, "decode failed")
            .unwrap();

        assert!(assets.get(handle).is_none());
        assert_eq!(
            states.state(handle.id()).unwrap().load_state(),
            &crate::LoadState::Failed {
                message: "decode failed".to_string()
            }
        );
        assert_eq!(
            events.drain(),
            vec![crate::AssetEvent::new(
                handle.id(),
                AssetVersion::ZERO,
                AssetEventKind::LoadFailed
            )]
        );
    }
}
