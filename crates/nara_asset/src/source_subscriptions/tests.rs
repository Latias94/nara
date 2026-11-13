use super::*;
use std::{cell::Cell, sync::Mutex, time::Duration};

use nara_app::{App, CoreStage};
use nara_ecs::{ResMut, schedule::IntoScheduleConfigs};

use crate::{
    AssetEventKind, AssetEvents, AssetPlugin, AssetRecord, AssetReloadRequestId,
    AssetReloadRequestKind, AssetReloadRequests, AssetSourceChangeKind, AssetSourceChanges,
    AssetSourceKind, AssetSourcePlugin, AssetStateError, AssetStates, AssetTaskUpdateSet,
    AssetVersion, Assets, ProjectAssetDatabase, register_source_reload_consumer,
};

fn source(index: usize) -> StableAssetId {
    StableAssetId::parse_str(match index {
        0 => "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f",
        1 => "b73f0f16-09e8-4265-b090-b689b41c197e",
        _ => "d87c8f9d-0dc2-4863-8e9c-d3e6eaa8d41f",
    })
    .unwrap()
}

fn path(value: &str) -> AssetSourcePath {
    AssetSourcePath::new(value).unwrap()
}

fn paths(values: &[&str]) -> Vec<AssetSourcePath> {
    values.iter().map(|value| path(value)).collect()
}

struct Fixture {
    consumer: Arc<SourceReloadConsumer>,
    requests: Vec<AssetReloadRequest>,
    generations: AssetLoadGenerations,
}

impl Fixture {
    fn new(count: usize) -> Self {
        Self::with_kind(
            count,
            AssetSourceKind::Other("subscription-test".to_owned()),
        )
    }

    fn with_kind(count: usize, kind: AssetSourceKind) -> Self {
        let mut app = App::new();
        app.insert_resource(
            nara_tasks::TaskPools::inline_for_tests(nara_tasks::TaskPoolConfig::default()).unwrap(),
        )
        .unwrap();
        app.add_plugins((
            nara_tasks::TaskPlugin::default(),
            AssetPlugin,
            AssetSourcePlugin,
        ))
        .unwrap();
        let consumer =
            Arc::new(register_source_reload_consumer::<String>(&mut app, kind.clone()).unwrap());
        let output = Arc::new(Mutex::new(None));
        let captured_output = Arc::clone(&output);
        let captured_consumer = Arc::clone(&consumer);
        app.add_systems(
            CoreStage::TaskUpdate,
            (move |mut requests: ResMut<AssetReloadRequests>,
                   mut generations: ResMut<AssetLoadGenerations>| {
                let drained = requests.drain_source(&captured_consumer).unwrap();
                if !drained.is_empty() {
                    *captured_output.lock().unwrap() =
                        Some((drained, std::mem::take(&mut *generations)));
                }
            })
            .in_set(AssetTaskUpdateSet::SpawnJobs),
        )
        .unwrap();
        for index in 0..count {
            let record = AssetRecord::new(
                source(index),
                path(&format!("sources/{index}.data")),
                kind.clone(),
            );
            let world = app.world_mut().unwrap();
            world
                .resource_mut::<ProjectAssetDatabase>()
                .insert(record.clone())
                .unwrap();
            world
                .resource_mut::<AssetSourceChanges>()
                .modified(record.path().clone())
                .unwrap();
        }
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        runtime.finish().unwrap();
        let (requests, generations) = output.lock().unwrap().take().unwrap();
        assert_eq!(requests.len(), count);
        Self {
            consumer,
            requests,
            generations,
        }
    }

    fn request(&self, index: usize) -> &AssetReloadRequest {
        self.requests
            .iter()
            .find(|request| request.stable_id() == source(index))
            .unwrap()
    }

    fn prepare(
        &self,
        graph: &AssetDependencyGraph,
        index: usize,
        values: Vec<AssetSourcePath>,
        kind: SourceSubscriptionKind,
    ) -> Result<PreparedSourceSubscriptions, SourceSubscriptionError> {
        graph.prepare_source_subscriptions(
            &self.consumer,
            self.request(index),
            &self.generations,
            values,
            kind,
        )
    }

    fn replace(
        &self,
        graph: &mut AssetDependencyGraph,
        index: usize,
        values: &[&str],
        kind: SourceSubscriptionKind,
    ) {
        self.prepare(graph, index, paths(values), kind)
            .unwrap()
            .commit_with(graph, &self.generations, || Ok::<_, ()>(()))
            .unwrap();
    }
}

fn observed(graph: &AssetDependencyGraph, index: usize, kind: SourceSubscriptionKind) -> Vec<&str> {
    graph
        .source_subscription_paths(source(index), kind)
        .iter()
        .map(AssetSourcePath::as_str)
        .collect()
}

fn dependents(graph: &AssetDependencyGraph, value: &str) -> Vec<StableAssetId> {
    graph.raw_dependents_for_path(&path(value)).collect()
}

#[test]
fn registered_current_request_atomically_publishes_value_and_replaces_complete_inputs() {
    let fixture = Fixture::new(1);
    let request = fixture.request(0);
    assert!(fixture.consumer.owns_request(request));
    assert!(request.generation().raw() > 0);
    assert!(fixture.generations.is_current_request(request));
    let handle = crate::Handle::<String>::new(request.asset_id());
    let mut graph = AssetDependencyGraph::default();
    let mut assets = Assets::<String>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let baseline = assets.prepare_mutation(handle, &states).unwrap();
    fixture
        .prepare(
            &graph,
            0,
            paths(&["raw/A", "sources/0.data"]),
            SourceSubscriptionKind::Published,
        )
        .unwrap()
        .commit_with(&mut graph, &fixture.generations, || {
            baseline.commit_loaded(
                "first".to_owned(),
                &mut assets,
                &mut states,
                &mut events,
                None,
                None,
            )
        })
        .unwrap();
    assert_eq!(assets.get(handle).map(String::as_str), Some("first"));
    assert_eq!(dependents(&graph, "raw/A"), vec![source(0)]);
    let baseline = assets.prepare_mutation(handle, &states).unwrap();
    fixture
        .prepare(
            &graph,
            0,
            paths(&["raw/B"]),
            SourceSubscriptionKind::Published,
        )
        .unwrap()
        .commit_with(&mut graph, &fixture.generations, || {
            baseline.commit_loaded(
                "second".to_owned(),
                &mut assets,
                &mut states,
                &mut events,
                None,
                None,
            )
        })
        .unwrap();
    assert_eq!(assets.get(handle).map(String::as_str), Some("second"));
    assert!(dependents(&graph, "raw/A").is_empty());
    assert!(dependents(&graph, "sources/0.data").is_empty());
    assert_eq!(dependents(&graph, "raw/B"), vec![source(0)]);
    let baseline = assets.prepare_mutation(handle, &states).unwrap();
    fixture
        .prepare(&graph, 0, Vec::new(), SourceSubscriptionKind::Published)
        .unwrap()
        .commit_with(&mut graph, &fixture.generations, || {
            baseline.remove(&mut assets, &mut states, &mut events)
        })
        .unwrap();
    assert!(assets.get(handle).is_none());
    assert!(dependents(&graph, "raw/B").is_empty());
    assert_eq!(
        graph.source_subscription_usage(),
        SourceSubscriptionUsage::default()
    );
    assert!(
        events
            .iter()
            .any(|event| event.kind() == AssetEventKind::Removed)
    );
}

#[test]
fn retry_replaces_only_latest_failed_inputs_and_success_clears_it() {
    let fixture = Fixture::new(1);
    let mut graph = AssetDependencyGraph::default();
    fixture.replace(
        &mut graph,
        0,
        &["raw/last-good"],
        SourceSubscriptionKind::Published,
    );
    fixture.replace(
        &mut graph,
        0,
        &["raw/missing-A", "raw/last-good"],
        SourceSubscriptionKind::Retry,
    );
    assert_eq!(dependents(&graph, "raw/last-good"), vec![source(0)]);
    fixture.replace(
        &mut graph,
        0,
        &["raw/missing-B"],
        SourceSubscriptionKind::Retry,
    );
    assert_eq!(
        observed(&graph, 0, SourceSubscriptionKind::Published),
        vec!["raw/last-good"]
    );
    assert_eq!(
        observed(&graph, 0, SourceSubscriptionKind::Retry),
        vec!["raw/missing-B"]
    );
    assert!(dependents(&graph, "raw/missing-A").is_empty());
    assert_eq!(dependents(&graph, "raw/last-good"), vec![source(0)]);
    fixture.replace(
        &mut graph,
        0,
        &["raw/new-good"],
        SourceSubscriptionKind::Published,
    );
    assert!(observed(&graph, 0, SourceSubscriptionKind::Retry).is_empty());
    assert!(dependents(&graph, "raw/missing-B").is_empty());
    assert!(dependents(&graph, "raw/last-good").is_empty());
}

#[test]
fn executed_failure_records_retry_without_replacing_last_good_value_or_successful_inputs() {
    let fixture = Fixture::new(1);
    let handle = crate::Handle::<String>::new(fixture.request(0).asset_id());
    let mut graph = AssetDependencyGraph::default();
    let mut assets = Assets::<String>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let version = assets
        .commit_loaded(
            handle,
            "last-good".to_owned(),
            &mut states,
            &mut events,
            None,
            None,
        )
        .unwrap();
    fixture.replace(
        &mut graph,
        0,
        &["raw/good"],
        SourceSubscriptionKind::Published,
    );
    fixture.replace(
        &mut graph,
        0,
        &["raw/previous-missing"],
        SourceSubscriptionKind::Retry,
    );
    let baseline = assets.prepare_mutation(handle, &states).unwrap();
    fixture
        .prepare(
            &graph,
            0,
            paths(&["raw/new-missing"]),
            SourceSubscriptionKind::Retry,
        )
        .unwrap()
        .commit_with(&mut graph, &fixture.generations, || {
            baseline.record_failure(&mut assets, &mut states, &mut events, "missing input")
        })
        .unwrap();
    assert_eq!(assets.get(handle).map(String::as_str), Some("last-good"));
    assert_eq!(
        states.state(handle.id()).unwrap().usable_version(),
        Some(version)
    );
    assert_eq!(
        observed(&graph, 0, SourceSubscriptionKind::Published),
        vec!["raw/good"]
    );
    assert_eq!(
        observed(&graph, 0, SourceSubscriptionKind::Retry),
        vec!["raw/new-missing"]
    );
    assert!(dependents(&graph, "raw/previous-missing").is_empty());
    assert!(
        events
            .iter()
            .any(|event| event.kind() == AssetEventKind::ReloadFailed)
    );
}

#[test]
fn refused_callback_preserves_sets_usage_and_prepared_revision() {
    let fixture = Fixture::new(1);
    let mut graph = AssetDependencyGraph::default();
    fixture.replace(
        &mut graph,
        0,
        &["raw/good"],
        SourceSubscriptionKind::Published,
    );
    fixture.replace(&mut graph, 0, &["raw/retry"], SourceSubscriptionKind::Retry);
    let usage = graph.source_subscription_usage();
    let witness = fixture
        .prepare(
            &graph,
            0,
            paths(&["raw/witness"]),
            SourceSubscriptionKind::Published,
        )
        .unwrap();
    let refused = fixture
        .prepare(
            &graph,
            0,
            paths(&["raw/refused"]),
            SourceSubscriptionKind::Published,
        )
        .unwrap();
    assert!(matches!(
        refused.commit_with(&mut graph, &fixture.generations, || Err::<(), _>("refused")),
        Err(SourceSubscriptionCommitError::Mutation("refused"))
    ));
    assert!(witness.validate(&graph, &fixture.generations).is_ok());
    assert_eq!(graph.source_subscription_usage(), usage);
    assert_eq!(
        observed(&graph, 0, SourceSubscriptionKind::Published),
        vec!["raw/good"]
    );
    assert_eq!(
        observed(&graph, 0, SourceSubscriptionKind::Retry),
        vec!["raw/retry"]
    );
    assert!(dependents(&graph, "raw/refused").is_empty());
}

#[test]
fn superseded_value_baseline_refuses_new_edges_and_preserves_concurrent_value() {
    let fixture = Fixture::new(1);
    let handle = crate::Handle::<String>::new(fixture.request(0).asset_id());
    let mut graph = AssetDependencyGraph::default();
    let mut assets = Assets::<String>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    assets
        .commit_loaded(
            handle,
            "first".to_owned(),
            &mut states,
            &mut events,
            None,
            None,
        )
        .unwrap();
    fixture.replace(
        &mut graph,
        0,
        &["raw/good"],
        SourceSubscriptionKind::Published,
    );
    let baseline = assets.prepare_mutation(handle, &states).unwrap();
    let prepared = fixture
        .prepare(
            &graph,
            0,
            paths(&["raw/unadopted"]),
            SourceSubscriptionKind::Published,
        )
        .unwrap();
    assets
        .commit_loaded(
            handle,
            "concurrent".to_owned(),
            &mut states,
            &mut events,
            None,
            None,
        )
        .unwrap();
    assert!(matches!(
        prepared.commit_with(&mut graph, &fixture.generations, || {
            baseline.commit_loaded(
                "stale".to_owned(),
                &mut assets,
                &mut states,
                &mut events,
                None,
                None,
            )
        }),
        Err(SourceSubscriptionCommitError::Mutation(
            AssetStateError::MutationSuperseded { .. }
        ))
    ));
    assert_eq!(assets.get(handle).map(String::as_str), Some("concurrent"));
    assert_eq!(dependents(&graph, "raw/good"), vec![source(0)]);
    assert!(dependents(&graph, "raw/unadopted").is_empty());
}

#[test]
fn superseded_request_cannot_prepare_or_run_its_commit_callback() {
    let mut fixture = Fixture::new(1);
    let mut graph = AssetDependencyGraph::default();
    fixture.replace(
        &mut graph,
        0,
        &["raw/good"],
        SourceSubscriptionKind::Published,
    );
    let prepared = fixture
        .prepare(
            &graph,
            0,
            paths(&["raw/stale"]),
            SourceSubscriptionKind::Retry,
        )
        .unwrap();
    fixture
        .generations
        .begin_request(fixture.request(0).asset_id());
    let called = Cell::new(false);
    assert!(matches!(
        prepared.commit_with(&mut graph, &fixture.generations, || {
            called.set(true);
            Ok::<_, ()>(())
        }),
        Err(SourceSubscriptionCommitError::Validation(
            SourceSubscriptionError::Superseded
        ))
    ));
    assert!(!called.get());
    assert!(matches!(
        fixture.prepare(&graph, 0, Vec::new(), SourceSubscriptionKind::Published),
        Err(SourceSubscriptionError::Superseded)
    ));
    assert_eq!(dependents(&graph, "raw/good"), vec![source(0)]);
}

#[test]
fn foreign_and_publicly_fabricated_requests_cannot_replace_an_owner() {
    let fixture = Fixture::new(1);
    let foreign = Fixture::new(1);
    let other_kind = Fixture::with_kind(1, AssetSourceKind::Scene);
    let mut graph = AssetDependencyGraph::default();
    fixture.replace(
        &mut graph,
        0,
        &["raw/good"],
        SourceSubscriptionKind::Published,
    );
    let usage = graph.source_subscription_usage();
    let request = fixture.request(0);
    assert!(!fixture.consumer.owns_request(foreign.request(0)));
    assert!(matches!(
        graph.prepare_source_subscriptions(
            &fixture.consumer,
            foreign.request(0),
            &foreign.generations,
            Vec::new(),
            SourceSubscriptionKind::Published
        ),
        Err(SourceSubscriptionError::AuthorityMismatch)
    ));
    assert!(matches!(
        foreign.prepare(&graph, 0, Vec::new(), SourceSubscriptionKind::Published),
        Err(SourceSubscriptionError::AuthorityMismatch)
    ));
    assert!(matches!(
        other_kind.prepare(&graph, 0, Vec::new(), SourceSubscriptionKind::Retry),
        Err(SourceSubscriptionError::AuthorityMismatch)
    ));
    let fabricated = AssetReloadRequest::new(
        AssetReloadRequestId::from_raw(400),
        request.asset_id(),
        request.record(),
        AssetReloadRequestKind::LoadOrReload,
        AssetSourceChangeKind::Modified,
        AssetVersion::ZERO,
        request.generation(),
        Vec::new(),
    );
    assert!(!fixture.generations.is_current_request(&fabricated));
    assert!(matches!(
        graph.prepare_source_subscriptions(
            &fixture.consumer,
            &fabricated,
            &fixture.generations,
            Vec::new(),
            SourceSubscriptionKind::Published
        ),
        Err(SourceSubscriptionError::AuthorityMismatch)
    ));
    assert_eq!(graph.source_subscription_usage(), usage);
    assert_eq!(dependents(&graph, "raw/good"), vec![source(0)]);
}

#[test]
fn replacement_graph_and_generation_store_reject_before_callback() {
    let fixture = Fixture::new(1);
    let graph = AssetDependencyGraph::default();
    let prepared = fixture
        .prepare(
            &graph,
            0,
            paths(&["raw/input"]),
            SourceSubscriptionKind::Published,
        )
        .unwrap();
    let mut replacement = AssetDependencyGraph::default();
    let called = Cell::new(false);
    assert!(matches!(
        prepared.commit_with(&mut replacement, &fixture.generations, || {
            called.set(true);
            Ok::<_, ()>(())
        }),
        Err(SourceSubscriptionCommitError::Validation(
            SourceSubscriptionError::GraphChanged
        ))
    ));
    let prepared = fixture
        .prepare(
            &graph,
            0,
            paths(&["raw/input"]),
            SourceSubscriptionKind::Published,
        )
        .unwrap();
    let mut replacement_generations = AssetLoadGenerations::default();
    let request = fixture.request(0);
    for _ in 0..request.generation().raw() {
        replacement_generations.begin_request(request.asset_id());
    }
    assert!(replacement_generations.is_current(request.asset_id(), request.generation()));
    assert!(!replacement_generations.is_current_request(request));
    assert!(matches!(
        graph.prepare_source_subscriptions(
            &fixture.consumer,
            request,
            &replacement_generations,
            paths(&["raw/replacement"]),
            SourceSubscriptionKind::Published,
        ),
        Err(SourceSubscriptionError::Superseded)
    ));
    let mut graph = graph;
    assert!(matches!(
        prepared.commit_with(&mut graph, &replacement_generations, || {
            called.set(true);
            Ok::<_, ()>(())
        }),
        Err(SourceSubscriptionCommitError::Validation(
            SourceSubscriptionError::GenerationStoreChanged
        ))
    ));
    assert!(!called.get());
    assert_eq!(
        graph.source_subscription_usage(),
        SourceSubscriptionUsage::default()
    );
}

#[test]
fn all_existing_graph_mutations_and_other_root_commits_invalidate_preparation() {
    let fixture = Fixture::new(2);
    for mutation in 0..5 {
        let mut graph = AssetDependencyGraph::default();
        let prepared = fixture
            .prepare(
                &graph,
                0,
                paths(&["raw/input"]),
                SourceSubscriptionKind::Published,
            )
            .unwrap();
        let recipe = ImportRecipeDigest::from_digest([4; 32]);
        match mutation {
            0 => graph.record_recipe(source(0), recipe),
            1 => graph.remove_recipe(source(0), recipe),
            2 => graph.add_source_dependency(source(0), source(1)),
            3 => graph.remove_source_dependency(source(0), source(1)),
            _ => fixture.replace(
                &mut graph,
                1,
                &["raw/other"],
                SourceSubscriptionKind::Published,
            ),
        }
        let called = Cell::new(false);
        assert!(matches!(
            prepared.commit_with(&mut graph, &fixture.generations, || {
                called.set(true);
                Ok::<_, ()>(())
            }),
            Err(SourceSubscriptionCommitError::Validation(
                SourceSubscriptionError::GraphChanged
            ))
        ));
        assert!(!called.get());
        assert!(dependents(&graph, "raw/input").is_empty());
    }
}

mod bounds;
