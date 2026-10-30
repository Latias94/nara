//! Concrete PNG import and processed Image cooking through the existing Image codecs.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use nara_asset::{
    AssetMetaFileLimits, AssetProductRef, AssetSourceKind, ImportedProductId, RuntimeAssetCodec,
};
use nara_core::{FormatKind, FormatVersion};
use nara_image::{
    ImageAsset, ImageBytesImportRequest, ImageCookError, ImageCookLimits, ImageCookSettings,
    ImageCookTarget, ImageImportSettings, ImageImporter, ImageOversizePolicy, ImageRuntimeCodec,
    cook_runtime_image,
};

use super::{
    ArtifactBuildDefinition, ArtifactBuildError, ArtifactBuildObservation, ArtifactBuildOutput,
    ArtifactBuildReason, ArtifactBuildResult, ArtifactBuildSeed, ArtifactBuildStore,
    ArtifactContentDigest, ArtifactGroupCandidate, ArtifactPublicationPoint, ArtifactSourceRequest,
    VerifiedArtifactGroup,
    budget::{ArtifactBuildLease, add, mul},
    execute::{SourceInputs, admit_metadata},
};

const RUNTIME_MEMBER: &str = "runtime";

// Keep the value and its charge in one owner, including all early-return paths.
// Rust drops fields in declaration order, so the value retires before the refund.
struct WorkingValue<T> {
    value: T,
    _lease: ArtifactBuildLease,
}

/// Imports the actual PNG and admitted Image settings into one source-free Runtime member.
///
/// The Host supplies the selected definition's explicit ID/revision and the digest of its actual
/// compiled implementation. The callback fences the current request, including cache reuse; it
/// must not re-enter this store during commit. All source reads use the request's narrow authority.
pub fn build_image_import(
    store: &ArtifactBuildStore,
    request: &ArtifactSourceRequest,
    mut control: impl FnMut(ArtifactPublicationPoint) -> Result<(), ArtifactBuildError>,
) -> Result<ArtifactBuildResult, ArtifactBuildError> {
    catch_unwind(AssertUnwindSafe(|| {
        import_inner(store, request, &mut control)
    }))
    .unwrap_or(Err(ArtifactBuildError::Panicked))
}

fn import_inner(
    store: &ArtifactBuildStore,
    request: &ArtifactSourceRequest,
    control: &mut impl FnMut(ArtifactPublicationPoint) -> Result<(), ArtifactBuildError>,
) -> Result<ArtifactBuildResult, ArtifactBuildError> {
    // Normalization can hold the original metadata, typed values and canonical JSON together.
    let _metadata = store.reserve_work(mul(
        AssetMetaFileLimits::default().encoded_bytes().get(),
        8,
    )?)?;
    let mut inputs = SourceInputs::new(store, request);
    let (actual, normalized) =
        admit_metadata::<ImageImportSettings>(&mut inputs, &AssetSourceKind::Image, Ok)?;
    let selected = actual
        .admit::<ImageImportSettings>()
        .map_err(|_| ArtifactBuildError::InvalidCandidate("Image settings failed admission"))?;
    let (_, settings) = selected.into_meta().into_parts();
    let record = actual.identity().clone().into_record(&request.scope);
    let product = AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY)
        .map_err(|_| ArtifactBuildError::InvalidCandidate("Image requires a complete Product"))?;
    let importer = ImageImporter::default();
    let primary = inputs
        .read(&request.path, importer.limits().max_encoded_bytes(), true)?
        .ok_or(ArtifactBuildError::InvalidCandidate(
            "Image source is absent",
        ))?;
    let seed = ArtifactBuildSeed {
        source: product,
        location: Some(record.path().clone()),
        primary: Some(ArtifactContentDigest::of_bytes(&primary)),
        definitions: vec![request.definition.clone()],
        settings: normalized,
        target: Vec::new(),
        format: image_format()?,
        format_version: image_version()?,
    };
    let reason = if let Some(group) = store.lookup_seed(&seed)? {
        if let Some(path) = inputs.replay(group.observations())? {
            inputs.forget_dependencies();
            ArtifactBuildReason::RawInputChanged(path)
        } else {
            control(ArtifactPublicationPoint::Reuse)?;
            inputs.revalidate()?;
            return Ok(ArtifactBuildResult::reused(group));
        }
    } else {
        ArtifactBuildReason::NoPriorSuccess
    };

    // The importer owns a Vec. Charge that copy before allocation, independently of the tracked
    // immutable input that remains live for publication revalidation.
    let source_copy = store.reserve_work(primary.len())?;
    let unpublished = importer
        .preflight_unpublished_import(ImageBytesImportRequest::new(
            record,
            primary.to_vec(),
            settings,
        ))
        .map_err(ArtifactBuildError::Image)?;
    let plan = unpublished.memory_plan();
    // The encoded Vec is already charged. Include one extra RGBA buffer for pixel-storage
    // normalization, whose allocation may overlap the decoder's output Vec.
    let import_work = store.reserve_work(add(
        plan.peak_bytes()
            .checked_sub(plan.encoded_allocation_bytes())
            .ok_or(ArtifactBuildError::Overflow)?,
        add(plan.rgba_bytes(), image_overhead())?,
    )?)?;
    let imported = unpublished.import().map_err(ArtifactBuildError::Image)?;
    drop(source_copy);
    if imported.product() != Some(product)
        || imported.color_space() != settings.color_space()
        || imported.pixels().len() != plan.rgba_bytes()
    {
        return Err(ArtifactBuildError::InvalidCandidate(
            "Image importer exceeded its product promise",
        ));
    }

    // Source provenance and its attached import lease are not a bounded Runtime value. Preserve
    // the complete Product, descriptor and exact pixels while creating independent pixel custody.
    let mut pixels_work =
        store.reserve_work(add(mul(imported.pixels().len(), 2)?, image_overhead())?)?;
    let runtime = ImageAsset::from_product(
        product,
        imported.extent(),
        imported.format(),
        imported.color_space(),
        imported.pixels().to_vec(),
    )
    .map_err(|error| ArtifactBuildError::Cook(ImageCookError::Image(error)))?;
    drop(imported);
    drop(import_work);
    pixels_work.shrink_to(
        ImageRuntimeCodec::runtime_retained_bytes(&runtime).map_err(ArtifactBuildError::Codec)?,
    )?;
    let encoded = encode_image(store, &runtime)?;
    // Complete trusted destruction before the store can publish its successful index.
    drop(runtime);
    drop(pixels_work);
    let candidate = image_candidate(
        seed,
        inputs.observations(),
        product,
        Arc::clone(&encoded.value),
    );
    let receipt = store.commit(candidate, |point| {
        control(point)?;
        inputs.revalidate()
    })?;
    Ok(ArtifactBuildResult::built(receipt, reason))
}

/// Cooks the captured `runtime` member of a verified upstream group into another complete group.
///
/// No source or generated filename is reopened. The upstream group remains retained throughout
/// decode, cooking, encoding and commit. Its immutable validity does not prove that it is still
/// selected by the caller: `control` must fence that selection on reuse and publication boundaries.
#[allow(
    clippy::too_many_arguments,
    reason = "This concrete operation keeps each authority typed."
)]
pub fn cook_image_group(
    store: &ArtifactBuildStore,
    input: &VerifiedArtifactGroup,
    product: AssetProductRef,
    definition: ArtifactBuildDefinition,
    settings: ImageCookSettings,
    target: ImageCookTarget,
    limits: ImageCookLimits,
    mut control: impl FnMut(ArtifactPublicationPoint) -> Result<(), ArtifactBuildError>,
) -> Result<ArtifactBuildResult, ArtifactBuildError> {
    catch_unwind(AssertUnwindSafe(|| {
        cook_inner(
            store,
            input,
            product,
            definition,
            settings,
            target,
            limits,
            &mut control,
        )
    }))
    .unwrap_or(Err(ArtifactBuildError::Panicked))
}

#[allow(
    clippy::too_many_arguments,
    reason = "Private implementation of the concrete cook operation."
)]
fn cook_inner(
    store: &ArtifactBuildStore,
    input: &VerifiedArtifactGroup,
    product: AssetProductRef,
    definition: ArtifactBuildDefinition,
    settings: ImageCookSettings,
    target: ImageCookTarget,
    limits: ImageCookLimits,
    control: &mut impl FnMut(ArtifactPublicationPoint) -> Result<(), ArtifactBuildError>,
) -> Result<ArtifactBuildResult, ArtifactBuildError> {
    let upstream = input.clone();
    let format = image_format()?;
    let version = image_version()?;
    let member = upstream
        .members()
        .iter()
        .find(|member| member.name == RUNTIME_MEMBER)
        .ok_or(ArtifactBuildError::InvalidCandidate(
            "upstream Runtime Image member is absent",
        ))?;
    if member.format != format
        || member.format_version != version
        || member.products.as_slice() != [product]
        || !member.dependencies.is_empty()
    {
        return Err(ArtifactBuildError::InvalidCandidate(
            "upstream member is not the selected Runtime Image",
        ));
    }
    let encoded =
        upstream
            .member_bytes(RUNTIME_MEMBER)
            .ok_or(ArtifactBuildError::InvalidCandidate(
                "upstream Runtime Image bytes are absent",
            ))?;
    if ArtifactContentDigest::of_bytes(encoded) != member.digest {
        return Err(ArtifactBuildError::InvalidCandidate(
            "upstream captured member digest mismatch",
        ));
    }
    let image = decode_image(store, encoded, product)?;
    // A warm cache never bypasses the concrete policy's rejection of its current input.
    if settings.oversize == ImageOversizePolicy::Reject
        && image.value.extent().width.max(image.value.extent().height)
            > target.max_texture_dimension_2d().get()
    {
        return Err(ArtifactBuildError::Cook(
            ImageCookError::TargetDimensionExceeded {
                extent: image.value.extent(),
                max_texture_dimension_2d: target.max_texture_dimension_2d(),
            },
        ));
    }
    let _metadata = store.reserve_work(mul(store.limits().metadata_bytes.get(), 2)?)?;
    let mut setting_bytes = b"nara.image.cook-settings.v1\0".to_vec();
    setting_bytes.push(match settings.oversize {
        ImageOversizePolicy::Reject => 0,
        ImageOversizePolicy::FitNearest => 1,
    });
    let mut target_bytes = b"nara.image.cook-target.v1\0".to_vec();
    target_bytes.extend_from_slice(&target.max_texture_dimension_2d().get().to_le_bytes());
    let seed = ArtifactBuildSeed {
        source: product,
        location: upstream.seed().location.clone(),
        primary: Some(member.digest),
        definitions: vec![definition],
        settings: setting_bytes,
        target: target_bytes,
        format,
        format_version: version,
    };
    let observations = vec![ArtifactBuildObservation::Processed {
        product,
        group: upstream.digest(),
        member: member.digest,
        format: member.format.clone(),
        format_version: member.format_version,
    }];
    let prior = store.lookup_seed(&seed)?;
    let reason = if let Some(group) = prior.as_ref() {
        if group.observations() == observations {
            // Per-call bounds are execution admission, not recipe meaning. A hit still has to fit
            // today's output/allocation limits and contain the actual requested Runtime Product.
            let cached = group
                .members()
                .iter()
                .find(|member| member.name == RUNTIME_MEMBER)
                .ok_or(ArtifactBuildError::InvalidCache(
                    "cooked Runtime member is absent",
                ))?;
            if cached.format != seed.format
                || cached.format_version != version
                || cached.products.as_slice() != [product]
                || !cached.dependencies.is_empty()
            {
                return Err(ArtifactBuildError::InvalidCache(
                    "cooked Runtime member contract mismatch",
                ));
            }
            let value = decode_image(
                store,
                group
                    .member_bytes(RUNTIME_MEMBER)
                    .ok_or(ArtifactBuildError::InvalidCache(
                        "cooked Runtime bytes are absent",
                    ))?,
                product,
            )?;
            admit_cook_output(&value.value, limits)?;
            drop(value);
            drop(image);
            control(ArtifactPublicationPoint::Reuse)?;
            return Ok(ArtifactBuildResult::reused(group.clone()));
        }
        ArtifactBuildReason::ProcessedInputChanged(product)
    } else {
        ArtifactBuildReason::NoPriorSuccess
    };
    drop(prior);

    let mut cook_work = store.reserve_work(limits.max_allocation_bytes().get())?;
    let cooked = cook_runtime_image(&image.value, settings, target, limits)
        .map_err(ArtifactBuildError::Cook)?;
    drop(image);
    admit_cook_output(&cooked, limits)?;
    cook_work.shrink_to(
        ImageRuntimeCodec::runtime_retained_bytes(&cooked).map_err(ArtifactBuildError::Codec)?,
    )?;
    let encoded = encode_image(store, &cooked)?;
    drop(cooked);
    drop(cook_work);
    let receipt = store.commit(
        image_candidate(seed, observations, product, Arc::clone(&encoded.value)),
        control,
    )?;
    Ok(ArtifactBuildResult::built(receipt, reason))
}

fn decode_image(
    store: &ArtifactBuildStore,
    bytes: &[u8],
    product: AssetProductRef,
) -> Result<WorkingValue<ImageAsset>, ArtifactBuildError> {
    let limits = ImageRuntimeCodec::runtime_limits();
    if bytes.len() > limits.encoded_bytes().get() {
        return Err(ArtifactBuildError::InvalidCandidate(
            "Runtime Image exceeds its original input promise",
        ));
    }
    let mut work = store.reserve_work(add(
        limits.work_bytes().get(),
        limits.retained_bytes().get(),
    )?)?;
    let image = ImageRuntimeCodec::decode_runtime(bytes).map_err(ArtifactBuildError::Codec)?;
    if !ImageRuntimeCodec::contains_runtime_product(&image, product) {
        return Err(ArtifactBuildError::InvalidCandidate(
            "decoded Runtime Image Product mismatch",
        ));
    }
    let retained =
        ImageRuntimeCodec::runtime_retained_bytes(&image).map_err(ArtifactBuildError::Codec)?;
    if retained > limits.retained_bytes().get() {
        return Err(ArtifactBuildError::InvalidCandidate(
            "Runtime Image exceeds its original output promise",
        ));
    }
    work.shrink_to(retained)?;
    Ok(WorkingValue {
        value: image,
        _lease: work,
    })
}

fn encode_image(
    store: &ArtifactBuildStore,
    image: &ImageAsset,
) -> Result<WorkingValue<Arc<[u8]>>, ArtifactBuildError> {
    let limits = ImageRuntimeCodec::runtime_limits();
    let mut work = store.reserve_work(add(
        limits.work_bytes().get(),
        mul(limits.encoded_bytes().get(), 2)?,
    )?)?;
    let encoded = ImageRuntimeCodec::encode_runtime(image).map_err(ArtifactBuildError::Codec)?;
    if encoded.len() > limits.encoded_bytes().get() {
        return Err(ArtifactBuildError::InvalidCandidate(
            "Runtime Image encoder exceeded its original byte promise",
        ));
    }
    let encoded: Arc<[u8]> = Arc::from(encoded);
    work.shrink_to(add(encoded.len(), 2 * size_of::<usize>())?)?;
    Ok(WorkingValue {
        value: encoded,
        _lease: work,
    })
}

fn admit_cook_output(
    image: &ImageAsset,
    limits: ImageCookLimits,
) -> Result<(), ArtifactBuildError> {
    let retained =
        ImageRuntimeCodec::runtime_retained_bytes(image).map_err(ArtifactBuildError::Codec)?;
    let allocation = add(retained, image.pixels().len())?;
    if retained > limits.max_output_bytes().get() {
        return Err(ArtifactBuildError::Cook(
            ImageCookError::OutputBytesExceeded {
                required: retained,
                limit: limits.max_output_bytes(),
            },
        ));
    }
    if allocation > limits.max_allocation_bytes().get() {
        return Err(ArtifactBuildError::Cook(
            ImageCookError::AllocationBytesExceeded {
                required: allocation,
                limit: limits.max_allocation_bytes(),
            },
        ));
    }
    Ok(())
}

fn image_candidate(
    seed: ArtifactBuildSeed,
    observations: Vec<ArtifactBuildObservation>,
    product: AssetProductRef,
    bytes: Arc<[u8]>,
) -> ArtifactGroupCandidate {
    ArtifactGroupCandidate {
        outputs: vec![ArtifactBuildOutput {
            name: RUNTIME_MEMBER.to_owned(),
            format: seed.format.clone(),
            format_version: seed.format_version,
            products: vec![product],
            dependencies: Vec::new(),
            bytes,
        }],
        seed,
        observations,
    }
}

fn image_format() -> Result<FormatKind, ArtifactBuildError> {
    FormatKind::new(ImageRuntimeCodec::RUNTIME_KIND)
        .map_err(|_| ArtifactBuildError::InvalidCandidate("invalid compiled Runtime Image format"))
}

fn image_version() -> Result<FormatVersion, ArtifactBuildError> {
    FormatVersion::new(ImageRuntimeCodec::RUNTIME_VERSION).ok_or(
        ArtifactBuildError::InvalidCandidate("invalid compiled Runtime Image version"),
    )
}

fn image_overhead() -> usize {
    size_of::<ImageAsset>() + size_of::<Box<[u8]>>() + 2 * size_of::<usize>()
}
