use crate::core::FormatKind;

use super::*;

#[derive(Clone, Copy)]
enum Limit {
    Retained,
    Work,
}

impl Limit {
    fn limits(self, bytes: usize) -> CookedContentLimits {
        let mut limits = CookedContentLimits::default();
        match self {
            Self::Retained => limits.retained_bytes = ByteLimit::new(bytes).unwrap(),
            Self::Work => limits.work_bytes = ByteLimit::new(bytes).unwrap(),
        }
        limits
    }

    fn assert_error(self, error: ProjectCookError, expected: usize, bound: usize) {
        let kind = match self {
            Self::Retained => "retained-bytes",
            Self::Work => "work-bytes",
        };
        assert!(
            matches!(error, ProjectCookError::Content(CookedContentError::Budget {
                kind: actual, required, limit,
            }) if actual == kind && required == expected && limit == bound),
            "expected {kind}: required={expected}, limit={bound}; got {error:?}",
        );
    }
}

#[test]
fn consumer_tables_admit_exact_metadata_and_refuse_one_byte_short_before_mutation_or_build() {
    let fixture = Fixture::<4>::new(None);
    let baseline = match fixture.builder(Limit::Retained.limits(1)).unwrap_err() {
        CookedContentError::Budget {
            kind: "retained-bytes",
            required,
            limit: 1,
        } => required,
        error => panic!("expected the original builder's initial retention bound, got {error:?}"),
    };
    let raw_bytes =
        (size_of::<(AssetSourcePath, Option<ArtifactContentDigest>)>() + 64) * 2 + OPTIONAL.len();
    let processed = ArtifactBuildObservation::Processed {
        product: product(3),
        group: ArtifactContentDigest::of_bytes(b"original group"),
        member: ArtifactContentDigest::of_bytes(b"original member"),
        format: FormatKind::new(Codec::RUNTIME_KIND).unwrap(),
        format_version: FormatVersion::new(Codec::RUNTIME_VERSION).unwrap(),
    };
    let processed_bytes = size_of::<ArtifactBuildObservation>() * 2 + Codec::RUNTIME_KIND.len();
    let output_bytes = (size_of::<(AssetProductRef, Output)>() + 64) * 2;
    let built_source_bytes = (size_of::<(StableAssetId, &'static str)>() + 64) * 2;

    for kind in [Limit::Retained, Limit::Work] {
        for short in [false, true] {
            let bound = baseline + raw_bytes - usize::from(short);
            let mut builder = fixture.builder(kind.limits(bound)).unwrap();
            let mut observations = Observations::default();
            let result = observations.raw(&mut builder, OPTIONAL, None);
            if short {
                kind.assert_error(result.unwrap_err(), baseline + raw_bytes, bound);
                assert!(observations.raw.is_empty());
                builder.reserve_consumer_metadata(raw_bytes - 1).unwrap();
            } else {
                result.unwrap();
                assert_eq!(observations.raw.len(), 1);
                observations.raw(&mut builder, OPTIONAL, None).unwrap();
                kind.assert_error(
                    builder.reserve_consumer_metadata(1).unwrap_err().into(),
                    bound + 1,
                    bound,
                );
            }

            let bound = baseline + processed_bytes - usize::from(short);
            let mut builder = fixture.builder(kind.limits(bound)).unwrap();
            let mut observations = Observations::default();
            let result = observations.add(&mut builder, &processed);
            if short {
                kind.assert_error(result.unwrap_err(), baseline + processed_bytes, bound);
                assert!(observations.processed.is_empty());
                builder
                    .reserve_consumer_metadata(processed_bytes - 1)
                    .unwrap();
            } else {
                result.unwrap();
                assert_eq!(observations.processed, [processed.clone()]);
                kind.assert_error(
                    builder.reserve_consumer_metadata(1).unwrap_err().into(),
                    bound + 1,
                    bound,
                );
            }

            let finish_bytes = size_of::<ArtifactBuildObservation>() * 2;
            let required = baseline + raw_bytes + processed_bytes + finish_bytes;
            let bound = required - usize::from(short);
            let mut builder = fixture.builder(kind.limits(bound)).unwrap();
            let mut observations = Observations::default();
            observations.raw(&mut builder, OPTIONAL, None).unwrap();
            observations.add(&mut builder, &processed).unwrap();
            let result = observations.finish(&mut builder);
            if short {
                kind.assert_error(result.unwrap_err(), required, bound);
                builder.reserve_consumer_metadata(finish_bytes - 1).unwrap();
            } else {
                assert_eq!(result.unwrap().len(), 2);
                kind.assert_error(
                    builder.reserve_consumer_metadata(1).unwrap_err().into(),
                    bound + 1,
                    bound,
                );
            }

            let bound = baseline + output_bytes - usize::from(short);
            let mut builder = fixture.builder(kind.limits(bound)).unwrap();
            let result = reserve_output(&mut builder);
            if short {
                kind.assert_error(result.unwrap_err(), baseline + output_bytes, bound);
                builder.reserve_consumer_metadata(output_bytes - 1).unwrap();
            } else {
                result.unwrap();
                kind.assert_error(
                    builder.reserve_consumer_metadata(1).unwrap_err().into(),
                    bound + 1,
                    bound,
                );
            }
        }

        let fixture = Fixture::<4>::new(Some(b'A'));
        let plan = resolve_product_recipe(&fixture.candidate, fixture.recipe.clone()).unwrap();
        let loader = ProjectContentLoader::with_limits(
            fixture.root.share(),
            fixture.recipe.source_content_limits(),
        )
        .unwrap();
        let snapshot = loader.load(&fixture.candidate, &plan).unwrap();
        let mut outputs = BTreeMap::new();
        let mut built_sources = BTreeMap::new();
        let mut observations = Observations::default();
        let bound = baseline + built_source_bytes - 1;
        let mut builder = fixture.builder(kind.limits(bound)).unwrap();
        let before = fixture.builds();
        let mut context = ProjectCookContext {
            snapshot: &snapshot,
            root: &fixture.root,
            store: &fixture.store,
            implementation: fixture.request().implementation,
            builder: &mut builder,
            omitted_scene_components: &[],
            outputs: &mut outputs,
            built_sources: &mut built_sources,
            observations: &mut observations,
        };
        let error = context.add_source_assets::<Domain<4>>().unwrap_err();
        kind.assert_error(error, baseline + built_source_bytes, bound);
        assert_eq!(fixture.builds(), before);
        assert!(context.built_sources.is_empty());
        assert!(context.outputs.is_empty());
        assert!(context.observations.raw.is_empty());
        context
            .builder
            .reserve_consumer_metadata(built_source_bytes - 1)
            .unwrap();

        let bound = baseline + built_source_bytes;
        let mut builder = fixture.builder(kind.limits(bound)).unwrap();
        context.builder = &mut builder;
        let error = context.add_source_assets::<Domain<4>>().unwrap_err();
        kind.assert_error(error, bound + output_bytes, bound);
        assert_eq!(fixture.builds() - before, 1);
        assert!(context.built_sources.is_empty());
        assert!(context.outputs.is_empty());
        assert!(context.observations.raw.is_empty());
        drop(context);
        drop(snapshot);
        assert_eq!(loader.budget_snapshot().active_generations(), 0);
        assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    }
}
