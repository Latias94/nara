use std::{cell::RefCell, fs};

use crate::{
    asset::RuntimeAssetCodec,
    asset_build::{ArtifactBuildError, ArtifactBuildReason, ArtifactBuildStatus},
    core::ByteLimit,
    fs::{ContentDigest, FsError},
    project_host::{
        ProjectContentBudgetHost, ProjectContentBudgetKind, ProjectContentErrorKind,
        ProjectContentMemberKind,
    },
    project_workflow::{
        ProjectCheckRequest, check_project,
        cooked_content::{CookedContentError, CookedContentLimits},
    },
    scene::SourceDocumentOrigin,
};

use super::*;

mod fixture;
mod metadata;

use fixture::{
    Codec, Domain, Fixture, Mapping, OPTIONAL, WideIntegers, auxiliary, product, write_metadata,
};

const STARTUP_META: &str = "scenes/first.scene.json.meta";
const DECLARED_META: &str = "scenes/second.scene.json.meta";

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
fn dependency_free_scene_roots_cook_from_adjacent_metadata() {
    let fixture = Fixture::<0>::empty_scenes();
    fixture.write("assets/unrelated.bin.meta", b"invalid metadata");
    let plan = resolve_product_recipe(&fixture.candidate, fixture.recipe.clone()).unwrap();
    let loader = ProjectContentLoader::new(fixture.root.share()).unwrap();
    let ordinary = loader.load(&fixture.candidate, &plan).unwrap();
    assert_eq!(ordinary.selected_scene_product(), None);
    drop(ordinary);
    let checked = loader.load_delivery(&fixture.candidate, &plan).unwrap();

    let cooked = prepare_project_cooked_content(fixture.request(), |context| {
        let snapshot = context.snapshot();
        assert!(snapshot.input_action_map().is_none());
        assert!(matches!(
            snapshot.selected_scene_source_key().origin(),
            SourceDocumentOrigin::ProjectLocation { .. }
        ));
        Ok(())
    })
    .unwrap();
    let startup = cooked.source().startup_content();
    assert_eq!(startup.members(), checked.startup_content().members());
    assert_eq!(
        startup.content_digest(),
        checked.startup_content().content_digest(),
    );
    assert_eq!(
        startup.closure_plan_digest(),
        checked.startup_content().closure_plan_digest(),
    );
    let catalog = cooked.content().base().catalog();
    assert_eq!(catalog.startup_scene(), product(1));
    assert_eq!(catalog.scenes(), [product(1), product(2)]);
    assert_eq!(catalog.members().len(), 2);
    for path in [STARTUP_META, DECLARED_META] {
        let member = cooked
            .source()
            .members()
            .iter()
            .find(|member| member.path().as_str() == path)
            .unwrap();
        assert_eq!(member.kind(), ProjectContentMemberKind::AssetMetadata);
        let bytes = fs::read(fixture.path.join(path)).unwrap();
        assert_eq!(member.digest(), ContentDigest::of_bytes(&bytes));
    }

    let (_, selection) = cooked.into_parts();
    selection.verify(&fixture.path).unwrap();
    for path in [STARTUP_META, DECLARED_META] {
        let original = fs::read_to_string(fixture.path.join(path)).unwrap();
        let changed = original.replacen('\n', " ", 1);
        assert_ne!(changed, original);
        fixture.write(path, changed.as_bytes());
        assert!(matches!(
            selection.verify(&fixture.path),
            Err(ProjectCookError::Filesystem(FsError::DigestMismatch { .. }))
        ));
        fixture.write(path, original.as_bytes());
        selection.verify(&fixture.path).unwrap();
    }
}

#[test]
fn dependency_free_scene_cook_rejects_missing_startup_metadata() {
    let fixture = Fixture::<0>::empty_scenes();
    fs::remove_file(fixture.path.join(STARTUP_META)).unwrap();
    let error = prepare_project_cooked_content(fixture.request(), |_| Ok(()))
        .err()
        .expect("Cook requires startup metadata");
    let ProjectCookError::Source(error) = error else {
        panic!("expected missing source metadata, got {error:?}");
    };
    assert_eq!(error.kind(), ProjectContentErrorKind::HostIo);
    let diagnostic = error.diagnostics().iter().next().unwrap();
    assert_eq!(diagnostic.code().as_str(), "project.content.host-io");
    for (key, value) in [("io_kind", "not-found"), ("project-path", STARTUP_META)] {
        assert!(
            diagnostic
                .fields()
                .iter()
                .any(|field| { field.key().as_str() == key && field.display_value() == value })
        );
    }
}

#[test]
fn dependency_free_scene_cook_rejects_duplicate_root_identity() {
    let fixture = Fixture::<0>::empty_scenes();
    write_metadata(
        &fixture.path,
        "scenes/second.scene.json",
        "second.scene.json",
        product(1).source(),
        AssetSourceKind::Scene,
    );
    let error = prepare_project_cooked_content(fixture.request(), |_| Ok(()))
        .err()
        .expect("different Scene roots cannot claim the same Product");
    let ProjectCookError::Source(error) = error else {
        panic!("expected conflicting source identity, got {error:?}");
    };
    assert_eq!(error.kind(), ProjectContentErrorKind::AssetMetaMismatch);
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.delivery.source-identity-conflict"
    }));
}

#[test]
fn selected_scene_metadata_rejects_nil_identity_before_contribution() {
    for fixture in [Fixture::<0>::empty_scenes(), Fixture::<0>::new(Some(b'A'))] {
        write_metadata(
            &fixture.path,
            "scenes/second.scene.json",
            "second.scene.json",
            StableAssetId::from_uuid(uuid::Uuid::nil()),
            AssetSourceKind::Scene,
        );
        let mut visited = Vec::new();
        let error = prepare_project_cooked_content(fixture.request(), |context| {
            visited.push(context.snapshot().selected_scene_path().as_str().to_owned());
            Ok(())
        })
        .err()
        .expect("a nil UUID does not admit a selected Scene Product");
        let ProjectCookError::Source(error) = error else {
            panic!("expected invalid source identity, got {error:?}");
        };
        assert_eq!(error.kind(), ProjectContentErrorKind::AssetMetaMismatch);
        assert_eq!(visited, ["first.scene.json"]);
    }
}

#[test]
fn declared_scene_roots_share_one_actual_compound_build_and_finished_runtime_payload() {
    let fixture = Fixture::<0>::new(Some(b'A'));
    let plan = resolve_product_recipe(&fixture.candidate, fixture.recipe.clone()).unwrap();
    let loader = ProjectContentLoader::new(fixture.root.share()).unwrap();
    let checked = loader.load_delivery(&fixture.candidate, &plan).unwrap();
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
    let startup = cooked.source().startup_content();
    assert_eq!(startup.members(), checked.startup_content().members());
    assert!(
        startup
            .members()
            .iter()
            .any(|member| member.path().as_str() == STARTUP_META)
    );
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
fn supplied_current_group_cooks_exact_wide_integer_settings() {
    let fixture = Fixture::<7>::new(None);
    let above_u64_max = 1_u128 << 64;
    let unsigned = [above_u64_max, above_u64_max + 1, u128::from(u64::MAX)];
    let below_i64_min = i128::from(i64::MIN) - 1;
    fixture.write_settings(Mapping {
        assignments: [(ImportedProductId::PRIMARY, 0), (auxiliary().product(), 1)],
        integers: Some(WideIntegers {
            unsigned,
            signed: below_i64_min,
        }),
    });
    check_project(ProjectCheckRequest::new(
        fixture.root.share(),
        fixture.recipe.clone(),
    ))
    .unwrap();

    let built = fixture.build();
    assert_eq!(built.status(), ArtifactBuildStatus::Built);
    assert!(built.publication_receipt().is_some());
    let settings = str::from_utf8(&built.group().seed().settings).unwrap();
    let unsigned_json = serde_json::to_string(&unsigned).unwrap();
    let sorted_integers =
        format!(r#""integers":{{"signed":{below_i64_min},"unsigned":{unsigned_json}}}"#);
    assert!(settings.contains(&sorted_integers), "{settings}");

    let reused = fixture.build();
    assert_eq!(reused.reason(), &ArtifactBuildReason::Reused);
    assert_eq!(reused.group().digest(), built.group().digest());
    let cooked = prepare_project_cooked_content(fixture.request(), |context| {
        context.add_group::<Domain<7>>(reused.group())
    })
    .unwrap();
    let (content, selection) = cooked.into_parts();
    selection.verify(&fixture.path).unwrap();
    let asset = content
        .base()
        .acquire_asset::<Domain<7>, Codec>(product(3))
        .unwrap();
    let integers = asset.product().integers.as_ref().unwrap();
    assert_eq!(integers.unsigned, unsigned);
    assert_eq!(integers.signed, below_i64_min);
    assert_eq!(fixture.builds(), 1);
}

#[test]
fn retained_group_must_match_the_current_typed_sidecar_mapping() {
    let fixture = Fixture::<5>::new(None);
    let original = fixture.build_group();
    let before = fixture.builds();
    let primary_id = ImportedProductId::PRIMARY;
    let auxiliary_id = auxiliary().product();

    fixture.write_mapping([(primary_id, 1), (auxiliary_id, 0)]);
    let result = prepare_project_cooked_content(fixture.request(), |context| {
        let current = context
            .snapshot()
            .domain_assets()
            .collection::<Domain<5>>()
            .unwrap();
        let imported = current.iter().next().unwrap();
        assert_eq!(
            (imported.source_value, imported.auxiliary_value),
            (b'Q', b'S')
        );
        context.add_group::<Domain<5>>(&original)
    });
    let error = result
        .err()
        .expect("the original group has the wrong mapping");
    assert!(
        matches!(&error, ProjectCookError::Invalid(message) if message.contains("does not match the selected source build: assets/shared.compound")),
        "{error:?}",
    );
    assert_eq!(fixture.builds(), before);

    let current = fixture.build_group();
    assert_eq!(current.observations(), original.observations());
    assert_eq!(
        current.members()[0].products,
        original.members()[0].products
    );
    assert_ne!(current.recipe_digest(), original.recipe_digest());
    let cooked = prepare_project_cooked_content(fixture.request(), |context| {
        context.add_group::<Domain<5>>(&current)?;
        let wrong_domain = context.add_group::<Domain<6>>(&current).unwrap_err();
        assert!(matches!(wrong_domain, ProjectCookError::Invalid(message)
            if message.contains("already belongs to a different source build domain")));
        context.add_group::<Domain<5>>(&current)
    })
    .unwrap();
    let (content, selection) = cooked.into_parts();
    selection.verify(&fixture.path).unwrap();
    let asset = content
        .base()
        .acquire_asset::<Domain<5>, Codec>(auxiliary())
        .unwrap();
    assert_eq!(
        (
            asset.product().source_value,
            asset.product().auxiliary_value
        ),
        (b'Q', b'S'),
    );
    assert_eq!(fixture.builds(), before + 1);

    let plan = resolve_product_recipe(&fixture.candidate, fixture.recipe.clone()).unwrap();
    let loader = ProjectContentLoader::new(fixture.root.share()).unwrap();
    let snapshot = loader.load(&fixture.candidate, &plan).unwrap();
    let mut historical = fixture.builder(CookedContentLimits::default()).unwrap();
    historical
        .add_scene(
            product(1),
            snapshot.compiled_candidate(),
            snapshot.asset_resolution(),
        )
        .unwrap();
    historical.add_group(&original).unwrap();
    let historical = historical.finish().unwrap();
    let asset = historical
        .base()
        .acquire_asset::<Domain<5>, Codec>(auxiliary())
        .unwrap();
    assert_eq!(
        (
            asset.product().source_value,
            asset.product().auxiliary_value
        ),
        (b'S', b'Q'),
    );

    fixture.write_mapping([(auxiliary_id, 1), (primary_id, 0)]);
    let same = prepare_project_cooked_content(fixture.request(), |context| {
        context.add_group::<Domain<5>>(&original)
    })
    .unwrap();
    let (same, selection) = same.into_parts();
    selection.verify(&fixture.path).unwrap();
    let asset = same
        .base()
        .acquire_asset::<Domain<5>, Codec>(product(3))
        .unwrap();
    assert_eq!(
        (
            asset.product().source_value,
            asset.product().auxiliary_value
        ),
        (b'S', b'Q'),
    );
    assert_eq!(fixture.builds(), before + 1);
}

#[test]
fn source_build_refuses_sidecar_changed_after_snapshot_admission() {
    let fixture = Fixture::<6>::new(None);
    let before = fixture.builds();
    let result = prepare_project_cooked_content(fixture.request(), |context| {
        fixture.write_mapping([(ImportedProductId::PRIMARY, 1), (auxiliary().product(), 0)]);
        context.add_source_assets::<Domain<6>>()
    });
    assert!(matches!(
        result,
        Err(ProjectCookError::Artifact(ArtifactBuildError::Stale))
    ));
    assert_eq!(fixture.builds(), before);
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
