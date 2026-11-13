use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_source_builds<D: AssetDomain>(
    mut executor: ResMut<SourceBuildExecutor<D>>,
    mut assets: ResMut<Assets<BuiltSourceAsset<D::Product>>>,
    mut states: ResMut<AssetStates>,
    generations: Res<AssetLoadGenerations>,
    mut graph: ResMut<AssetDependencyGraph>,
    mut events: ResMut<AssetEvents>,
    mut changes: ResMut<AssetSourceChanges>,
    mut diagnostics: ResMut<AssetReloadDiagnostics>,
    mut stats: ResMut<SourceAssetBuildStats<D>>,
) {
    let mut ready = std::mem::take(&mut executor.ready);
    ready.sort_unstable_by_key(|result| result.order);
    for ReadyBuild {
        context, terminal, ..
    } in ready.drain(..)
    {
        if !generations.is_current_request(&context.request)
            || context.request.resolved_state_revision()
                != Some(&states.state_revision(context.request.asset_id()))
            || context.baseline.validate(&assets, &states).is_err()
        {
            stats.stale = stats.stale.saturating_add(1);
            continue;
        }
        let output = match terminal {
            TaskTerminal::Completed(output) => output,
            TaskTerminal::Cancelled(_) => {
                record_failure(
                    context.baseline.clone(),
                    &mut assets,
                    &mut states,
                    &mut events,
                    &mut diagnostics,
                    &mut stats,
                    diagnostic(
                        "asset.build.task-cancelled",
                        "Source build task was cancelled",
                    ),
                );
                continue;
            }
            TaskTerminal::Failed(_) => {
                record_failure(
                    context.baseline.clone(),
                    &mut assets,
                    &mut states,
                    &mut events,
                    &mut diagnostics,
                    &mut stats,
                    diagnostic("asset.build.task-panicked", "Source build task panicked"),
                );
                continue;
            }
        };
        let SourceBuildOutput {
            mut result,
            observations,
        } = output;
        match observations.revalidate(&executor.configuration.store, &context.source) {
            Err(ArtifactBuildError::Stale) => {
                record_failure(
                    context.baseline.clone(),
                    &mut assets,
                    &mut states,
                    &mut events,
                    &mut diagnostics,
                    &mut stats,
                    build_diagnostic(ArtifactBuildError::Stale),
                );
                if changes
                    .push(AssetSourceChange::new(
                        context.request.path().clone(),
                        AssetSourceChangeKind::Modified,
                    ))
                    .is_err()
                {
                    stats.rejected = stats.rejected.saturating_add(1);
                    diagnostics.push(diagnostic(
                        "asset.build.retry-admission-refused",
                        "Changed source inputs could not be queued for another build",
                    ));
                }
                continue;
            }
            Err(error) => {
                result = Err(error);
            }
            Ok(()) => {}
        }
        if result.is_ok() && !observations.is_complete() {
            result = Err(ArtifactBuildError::InvalidCandidate(
                "successful source build has an unresolved input",
            ));
        }
        match result {
            Ok(PreparedBuiltSource { value, status }) => {
                let prepared = graph.prepare_source_subscriptions(
                    &executor.consumer,
                    &context.request,
                    &generations,
                    observations.paths().cloned().collect(),
                    SourceSubscriptionKind::Published,
                );
                let prepared = match prepared {
                    Ok(prepared) => prepared,
                    Err(_) => {
                        stats.rejected = stats.rejected.saturating_add(1);
                        record_failure(
                            context.baseline.clone(),
                            &mut assets,
                            &mut states,
                            &mut events,
                            &mut diagnostics,
                            &mut stats,
                            diagnostic(
                                "asset.build.subscription-refused",
                                "Source dependency subscriptions were refused",
                            ),
                        );
                        continue;
                    }
                };
                if prepared
                    .commit_with(&mut graph, &generations, || {
                        let version = context.baseline.clone().commit_loaded(
                            value,
                            &mut assets,
                            &mut states,
                            &mut events,
                            None,
                            None,
                        )?;
                        states.acknowledge_metadata_publication(context.request.asset_id());
                        Ok::<_, nara_asset::AssetStateError>(version)
                    })
                    .is_ok()
                {
                    stats.adopted = stats.adopted.saturating_add(1);
                    match status {
                        ArtifactBuildStatus::Built => stats.built = stats.built.saturating_add(1),
                        ArtifactBuildStatus::Hit => stats.reused = stats.reused.saturating_add(1),
                    }
                } else {
                    record_failure(
                        context.baseline.clone(),
                        &mut assets,
                        &mut states,
                        &mut events,
                        &mut diagnostics,
                        &mut stats,
                        diagnostic(
                            "asset.build.publication-refused",
                            "Source build publication was superseded or refused",
                        ),
                    );
                }
            }
            Err(error) => {
                let preserves_retry = match &error {
                    ArtifactBuildError::Overflow => true,
                    ArtifactBuildError::Budget { kind, .. } => *kind != "source-bytes",
                    _ => false,
                };
                let failure = build_diagnostic(error);
                if observations.is_empty() || !observations.may_replace_retry() || preserves_retry {
                    record_failure(
                        context.baseline.clone(),
                        &mut assets,
                        &mut states,
                        &mut events,
                        &mut diagnostics,
                        &mut stats,
                        failure,
                    );
                    continue;
                }
                let prepared = graph.prepare_source_subscriptions(
                    &executor.consumer,
                    &context.request,
                    &generations,
                    observations.paths().cloned().collect(),
                    SourceSubscriptionKind::Retry,
                );
                match prepared {
                    Ok(prepared) => {
                        if prepared
                            .commit_with(&mut graph, &generations, || {
                                context.baseline.clone().record_failure(
                                    &mut assets,
                                    &mut states,
                                    &mut events,
                                    failure.code().as_str(),
                                )
                            })
                            .is_ok()
                        {
                            stats.failed = stats.failed.saturating_add(1);
                            diagnostics.push(failure);
                        } else {
                            stats.stale = stats.stale.saturating_add(1);
                        }
                    }
                    Err(_) => {
                        stats.rejected = stats.rejected.saturating_add(1);
                        record_failure(
                            context.baseline.clone(),
                            &mut assets,
                            &mut states,
                            &mut events,
                            &mut diagnostics,
                            &mut stats,
                            failure,
                        );
                        diagnostics.push(diagnostic(
                            "asset.build.retry-subscription-refused",
                            "Source build recovery subscriptions were refused",
                        ));
                    }
                }
            }
        }
    }
    executor.ready = ready;
}
