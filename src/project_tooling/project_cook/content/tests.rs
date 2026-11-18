use std::cell::RefCell;

use crate::{
    asset::RuntimeAssetCodec,
    core::ByteLimit,
    project_host::{ProjectContentBudgetHost, ProjectContentBudgetKind, ProjectContentErrorKind},
    project_workflow::cooked_content::{CookedContentError, CookedContentLimits},
};

use super::*;

mod fixture;
mod metadata;

use fixture::{Codec, Domain, Fixture, OPTIONAL, auxiliary, product};

thread_local! {
    static COOK_BUDGET: RefCell<Option<ProjectContentBudgetHost>> = const { RefCell::new(None) };
}

pub(super) fn observe_loader_budget(loader: &ProjectContentLoader) {
    COOK_BUDGET.with_borrow_mut(|host| *host = Some(loader.budget_host()));
}

fn take_loader_budget() -> ProjectContentBudgetHost {
    COOK_BUDGET.with_borrow_mut(|host| {
        host.take().expect(
            "prepare must call tests::observe_loader_budget(&loader) after creating its loader",
        )
    })
}

fn assert_source_work_released(host: &ProjectContentBudgetHost) {
    let budget = host.snapshot();
    assert_eq!(budget.active_generations(), 0);
    assert_eq!(budget.active(ProjectContentBudgetKind::WorkBytes), 0);
    assert_eq!(budget.active(ProjectContentBudgetKind::EncodedBytes), 0);
    assert_eq!(budget.active(ProjectContentBudgetKind::OpenHandles), 0);
}

#[test]
fn declared_scene_roots_share_one_actual_compound_build_and_finished_runtime_payload() {
    let fixture = Fixture::<0>::new(Some(b'A'));
    let before = fixture.builds();
    let mut visited = Vec::new();
    let cooked = prepare_project_cooked_content(fixture.request(), |context| {
        let snapshot = context.snapshot();
        visited.push(snapshot.selected_scene_product().unwrap());
        let collection = snapshot.domain_assets().collection::<Domain<0>>().unwrap();
        assert_eq!(collection.source_records().len(), 1);
        assert_eq!(collection.iter().len(), 1);
        let imported = collection.iter().next().unwrap();
        assert_eq!(imported.source_value, b'S');
        assert_eq!(imported.build_value, None);
        assert!(
            snapshot
                .content_closure()
                .members()
                .iter()
                .all(|member| member.path().as_str() != OPTIONAL)
        );
        context.add_source_assets::<Domain<0>>()
    })
    .unwrap();

    let budget = take_loader_budget();
    assert_source_work_released(&budget);
    assert_eq!(budget.snapshot().generation_high_water(), 1);
    assert_eq!(visited, [product(1), product(2)]);
    assert_eq!(fixture.builds() - before, 1);
    assert_eq!(cooked.source().scene_roots().len(), 2);
    let base = cooked.content().base();
    assert_eq!(base.catalog().scenes(), [product(1), product(2)]);
    assert_eq!(base.catalog().members().len(), 3);
    let compound = base.catalog().member_for_product(product(3)).unwrap();
    assert_eq!(compound.products(), [product(3), auxiliary()]);
    assert_eq!(
        compound.path(),
        base.catalog()
            .member_for_product(auxiliary())
            .unwrap()
            .path(),
    );
    for reference in [product(3), auxiliary()] {
        let asset = base.acquire_asset::<Domain<0>, Codec>(reference).unwrap();
        assert_eq!(asset.product().source, product(3).source());
        assert_eq!(asset.product().source_value, b'S');
        assert_eq!(asset.product().build_value, Some(b'A'));
    }
    assert_eq!(base.validate_deep().unwrap().members(), 3);
    let (_, selection) = cooked.into_parts();
    selection.verify(&fixture.path).unwrap();
    drop(selection);
    assert_eq!(budget.snapshot().active_reservations(), 0);
}

#[test]
fn original_selection_rejects_changed_or_newly_present_build_only_inputs() {
    for initial in [Some(b'A'), None] {
        let fixture = Fixture::<1>::new(initial);
        let before = fixture.builds();
        let cooked = prepare_project_cooked_content(fixture.request(), |context| {
            context.add_source_assets::<Domain<1>>()
        })
        .unwrap();
        assert_eq!(fixture.builds() - before, 1);
        assert!(
            cooked
                .source()
                .members()
                .iter()
                .all(|member| member.path().as_str() != OPTIONAL)
        );
        let optional = cooked
            .observations()
            .iter()
            .filter(|observation| {
                matches!(observation, ArtifactBuildObservation::Raw { path, .. } if path.as_str() == OPTIONAL)
            })
            .collect::<Vec<_>>();
        assert_eq!(optional.len(), 1);
        assert_eq!(
            optional[0],
            &ArtifactBuildObservation::Raw {
                path: AssetSourcePath::new(OPTIONAL).unwrap(),
                digest: initial.map(|value| ArtifactContentDigest::of_bytes(&[value])),
            },
        );
        let (content, selection) = cooked.into_parts();
        selection.verify(&fixture.path).unwrap();
        fixture.write(OPTIONAL, b"B");
        assert!(matches!(
            selection.verify(&fixture.path),
            Err(ProjectCookError::Filesystem(_)),
        ));
        let asset = content
            .base()
            .acquire_asset::<Domain<1>, Codec>(auxiliary())
            .unwrap();
        assert_eq!(asset.product().build_value, initial);
        assert_eq!(fixture.builds() - before, 1);
    }
}

#[test]
fn changing_the_shared_source_after_its_first_contribution_rejects_the_whole_cook() {
    let fixture = Fixture::<2>::new(Some(b'A'));
    let before = fixture.builds();
    let mut visited = Vec::new();
    let result = prepare_project_cooked_content(fixture.request(), |context| {
        visited.push(context.snapshot().selected_scene_product().unwrap());
        context.add_source_assets::<Domain<2>>()?;
        if visited.len() == 1 {
            fixture.write_source(b'T');
        }
        Ok(())
    });

    assert_eq!(fixture.builds() - before, 1);
    assert_eq!(visited.first(), Some(&product(1)));
    let error = result
        .err()
        .expect("mixed source generations must reject the Cook");
    let ProjectCookError::Source(error) = error else {
        panic!("expected original delivery rejection, got {error:?}");
    };
    assert_eq!(error.kind(), ProjectContentErrorKind::HostAuthorityRejected);
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "project.delivery.member-changed")
    );
}

#[test]
fn a_panicking_typed_contribution_returns_panicked_releases_source_work_and_allows_recovery() {
    let fixture = Fixture::<3>::new(Some(b'A'));
    let before = fixture.builds();
    let mut calls = 0;
    let result = prepare_project_cooked_content(fixture.request(), |context| {
        calls += 1;
        context.add_source_assets::<Domain<3>>()?;
        panic!("typed Cook contribution panic after its real build");
    });
    assert!(matches!(result, Err(ProjectCookError::Panicked)));
    let budget = take_loader_budget();
    assert_source_work_released(&budget);
    assert_eq!(budget.snapshot().generation_high_water(), 1);
    assert_eq!(budget.snapshot().active_reservations(), 0);
    for &kind in ProjectContentBudgetKind::ALL {
        assert_eq!(budget.snapshot().active(kind), 0, "{kind:?}");
    }
    assert_eq!(calls, 1);
    assert_eq!(fixture.builds() - before, 1);

    let recovered = prepare_project_cooked_content(fixture.request(), |context| {
        context.add_source_assets::<Domain<3>>()
    })
    .unwrap();
    assert_eq!(fixture.builds() - before, 1);
    let asset = recovered
        .content()
        .base()
        .acquire_asset::<Domain<3>, Codec>(product(3))
        .unwrap();
    assert_eq!(asset.product().build_value, Some(b'A'));
    let (_, selection) = recovered.into_parts();
    selection.verify(&fixture.path).unwrap();
}

#[test]
fn raw_observations_preserve_presence_and_digest_agreement_in_both_orders() {
    let fixture = Fixture::<4>::new(None);
    let a = Some(ArtifactContentDigest::of_bytes(b"A"));
    let b = Some(ArtifactContentDigest::of_bytes(b"B"));
    for (first, second) in [(None, None), (a, a), (None, a), (a, None), (a, b), (b, a)] {
        let mut builder = fixture.builder(CookedContentLimits::default()).unwrap();
        let mut observations = Observations::default();
        observations.raw(&mut builder, OPTIONAL, first).unwrap();
        let result = observations.raw(&mut builder, OPTIONAL, second);
        if first == second {
            result.unwrap();
        } else {
            assert!(
                matches!(result, Err(ProjectCookError::Invalid(message)) if message.contains(OPTIONAL))
            );
        }
        assert_eq!(observations.raw.len(), 1);
        assert_eq!(
            observations.finish(&mut builder).unwrap(),
            [ArtifactBuildObservation::Raw {
                path: AssetSourcePath::new(OPTIONAL).unwrap(),
                digest: first,
            }],
        );
    }
}
