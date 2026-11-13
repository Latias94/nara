use super::*;

fn limits(owners: usize, paths: usize, items: usize, bytes: usize) -> SourceSubscriptionLimits {
    SourceSubscriptionLimits::new(
        ItemLimit::new(owners).unwrap(),
        ItemLimit::new(paths).unwrap(),
        ItemLimit::new(items).unwrap(),
        ByteLimit::new(bytes).unwrap(),
    )
}

#[test]
fn owner_limit_refunds_on_complete_removal() {
    let fixture = Fixture::new(2);
    let mut graph =
        AssetDependencyGraph::with_source_subscription_limits(limits(1, 8, 8, 1_000_000));
    fixture.replace(
        &mut graph,
        0,
        &["raw/one"],
        SourceSubscriptionKind::Published,
    );
    assert!(matches!(
        fixture.prepare(
            &graph,
            1,
            paths(&["raw/two"]),
            SourceSubscriptionKind::Published
        ),
        Err(SourceSubscriptionError::LimitExceeded {
            kind: SourceSubscriptionLimitKind::Owners,
            attempted: Some(2),
            ..
        })
    ));
    fixture.replace(&mut graph, 0, &[], SourceSubscriptionKind::Published);
    assert_eq!(
        graph.source_subscription_usage(),
        SourceSubscriptionUsage::default()
    );
    fixture.replace(
        &mut graph,
        1,
        &["raw/two"],
        SourceSubscriptionKind::Published,
    );
    assert_eq!(graph.source_subscription_usage().owners(), 1);
    assert_eq!(dependents(&graph, "raw/two"), vec![source(1)]);
}

#[test]
fn shared_raw_paths_route_each_source_once_and_refund_after_last_owner() {
    let fixture = Fixture::new(2);
    let mut graph =
        AssetDependencyGraph::with_source_subscription_limits(limits(2, 1, 8, 1_000_000));
    fixture.replace(
        &mut graph,
        0,
        &["raw/shared", "raw/shared"],
        SourceSubscriptionKind::Published,
    );
    fixture.replace(
        &mut graph,
        1,
        &["raw/shared"],
        SourceSubscriptionKind::Published,
    );
    fixture.replace(
        &mut graph,
        0,
        &["raw/shared"],
        SourceSubscriptionKind::Retry,
    );
    assert_eq!(dependents(&graph, "raw/shared"), vec![source(0), source(1)]);
    assert_eq!(graph.source_subscription_usage().paths(), 1);
    assert_eq!(graph.source_subscription_usage().items(), 3);
    assert!(matches!(
        fixture.prepare(
            &graph,
            0,
            paths(&["raw/extra"]),
            SourceSubscriptionKind::Retry
        ),
        Err(SourceSubscriptionError::LimitExceeded {
            kind: SourceSubscriptionLimitKind::Paths,
            attempted: Some(2),
            ..
        })
    ));
    fixture.replace(&mut graph, 0, &[], SourceSubscriptionKind::Published);
    assert_eq!(dependents(&graph, "raw/shared"), vec![source(1)]);
    assert_eq!(graph.source_subscription_usage().paths(), 1);
    fixture.replace(&mut graph, 1, &[], SourceSubscriptionKind::Published);
    assert!(dependents(&graph, "raw/shared").is_empty());
    assert_eq!(
        graph.source_subscription_usage(),
        SourceSubscriptionUsage::default()
    );
}

#[test]
fn item_limits_include_both_sets_and_success_reuses_released_capacity() {
    let fixture = Fixture::new(1);
    let mut graph =
        AssetDependencyGraph::with_source_subscription_limits(limits(1, 8, 2, 1_000_000));
    fixture.replace(
        &mut graph,
        0,
        &["raw/good"],
        SourceSubscriptionKind::Published,
    );
    fixture.replace(&mut graph, 0, &["raw/retry"], SourceSubscriptionKind::Retry);
    let usage = graph.source_subscription_usage();
    assert!(matches!(
        fixture.prepare(
            &graph,
            0,
            paths(&["raw/A", "raw/B"]),
            SourceSubscriptionKind::Retry
        ),
        Err(SourceSubscriptionError::LimitExceeded {
            kind: SourceSubscriptionLimitKind::Items,
            attempted: Some(3),
            ..
        })
    ));
    assert_eq!(graph.source_subscription_usage(), usage);
    assert_eq!(
        observed(&graph, 0, SourceSubscriptionKind::Retry),
        vec!["raw/retry"]
    );
    fixture.replace(
        &mut graph,
        0,
        &["raw/A", "raw/B"],
        SourceSubscriptionKind::Published,
    );
    assert_eq!(graph.source_subscription_usage().items(), 2);
    assert!(observed(&graph, 0, SourceSubscriptionKind::Retry).is_empty());
    assert!(dependents(&graph, "raw/retry").is_empty());
}

fn oversized_path() -> Vec<AssetSourcePath> {
    let mut value = String::with_capacity(4_096);
    value.push_str("raw/capacity");
    vec![AssetSourcePath::new(value).unwrap()]
}

#[test]
fn retained_byte_limit_charges_real_capacity_and_refunds_replacement() {
    let fixture = Fixture::new(1);
    let mut probe = AssetDependencyGraph::default();
    fixture
        .prepare(
            &probe,
            0,
            oversized_path(),
            SourceSubscriptionKind::Published,
        )
        .unwrap()
        .commit_with(&mut probe, &fixture.generations, || Ok::<_, ()>(()))
        .unwrap();
    let charged = probe.source_subscription_usage().retained_bytes();
    assert!(charged >= 2 * 4_096);
    let rejected =
        AssetDependencyGraph::with_source_subscription_limits(limits(1, 8, 8, charged - 1));
    assert!(matches!(
        fixture.prepare(&rejected, 0, oversized_path(), SourceSubscriptionKind::Published),
        Err(SourceSubscriptionError::LimitExceeded { kind: SourceSubscriptionLimitKind::RetainedBytes, attempted: Some(value), .. }) if value == charged
    ));
    assert_eq!(
        rejected.source_subscription_usage(),
        SourceSubscriptionUsage::default()
    );
    let mut graph = AssetDependencyGraph::with_source_subscription_limits(limits(1, 8, 8, charged));
    fixture
        .prepare(
            &graph,
            0,
            oversized_path(),
            SourceSubscriptionKind::Published,
        )
        .unwrap()
        .commit_with(&mut graph, &fixture.generations, || Ok::<_, ()>(()))
        .unwrap();
    assert_eq!(graph.source_subscription_usage().retained_bytes(), charged);
    fixture.replace(
        &mut graph,
        0,
        &["raw/small"],
        SourceSubscriptionKind::Published,
    );
    assert!(graph.source_subscription_usage().retained_bytes() < charged);
    fixture.replace(&mut graph, 0, &[], SourceSubscriptionKind::Published);
    assert_eq!(graph.source_subscription_usage().retained_bytes(), 0);
}

#[test]
fn retry_storage_is_replaced_without_history_or_per_path_versions() {
    let fixture = Fixture::new(1);
    let mut graph = AssetDependencyGraph::default();
    fixture.replace(
        &mut graph,
        0,
        &["raw/good"],
        SourceSubscriptionKind::Published,
    );
    fixture.replace(
        &mut graph,
        0,
        &["raw/retry-000"],
        SourceSubscriptionKind::Retry,
    );
    let usage = graph.source_subscription_usage();
    for index in 1..100 {
        fixture.replace(
            &mut graph,
            0,
            &[&format!("raw/retry-{index:03}")],
            SourceSubscriptionKind::Retry,
        );
        assert_eq!(graph.source_subscription_usage(), usage);
        assert!(dependents(&graph, &format!("raw/retry-{:03}", index - 1)).is_empty());
    }
    assert_eq!(dependents(&graph, "raw/good"), vec![source(0)]);
    assert_eq!(
        observed(&graph, 0, SourceSubscriptionKind::Retry),
        vec!["raw/retry-099"]
    );
    fixture.replace(&mut graph, 0, &[], SourceSubscriptionKind::Retry);
    assert_eq!(graph.source_subscription_usage().items(), 1);
    assert_eq!(dependents(&graph, "raw/good"), vec![source(0)]);
    assert!(dependents(&graph, "raw/retry-099").is_empty());
}

#[test]
fn normalized_set_does_not_retain_unused_input_vector_capacity() {
    let fixture = Fixture::new(1);
    let mut graph = AssetDependencyGraph::default();
    fixture.replace(&mut graph, 0, &["raw/A"], SourceSubscriptionKind::Published);
    let usage = graph.source_subscription_usage();
    let mut values = Vec::with_capacity(100_000);
    values.extend(paths(&["raw/A", "raw/A"]));
    fixture
        .prepare(&graph, 0, values, SourceSubscriptionKind::Published)
        .unwrap()
        .commit_with(&mut graph, &fixture.generations, || Ok::<_, ()>(()))
        .unwrap();
    assert_eq!(graph.source_subscription_usage(), usage);
    assert_eq!(
        observed(&graph, 0, SourceSubscriptionKind::Published),
        vec!["raw/A"]
    );
}
