//! A source badge with authored Image and build-only stock dependencies.

use std::cell::Cell;

use nara_asset::{
    AssetBuildSource, AssetDomain, AssetImportError, AssetImportLimits, AssetImportSource,
    AssetPath, AssetProductRef, AssetRef, AssetReferenceDemand, AssetRuntimeError,
    AssetRuntimeLimits, AssetSourceKind, ImportedProductId, RuntimeAssetCodec, SourceAssetBuild,
    SourceAssetImporter,
};
use nara_core::ByteLimit;
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
use serde::{Deserialize, Serialize};

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const PNG_BYTES: usize = 256;
const RUNTIME_BYTES: usize = 2048;
pub(super) const MAX_STOCK_DEPENDENCIES: usize = 12;

thread_local! {
    static EMBEDDED_IMPORTS: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn embedded_imports() -> usize {
    EMBEDDED_IMPORTS.get()
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ImageDelivery {
    Embedded,
    Linked,
    LinkedAtBuild,
    StockAtBuild(Box<[(AssetProductRef, AssetReferenceDemand)]>),
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BadgeSettings {
    pub(super) image_delivery: ImageDelivery,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BadgeSource {
    pub(super) png: AssetPath,
    pub(super) image: AssetProductRef,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Badge {
    pub(super) product: AssetProductRef,
    pub(super) art: BadgeArt,
    #[serde(skip)]
    source_image: Option<AssetRef>,
    #[serde(skip)]
    build_art: Option<BadgeArt>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum BadgeArt {
    Embedded(Box<[u8]>),
    Linked(AssetProductRef),
    Stock(Box<[(AssetProductRef, AssetReferenceDemand)]>),
}

pub(super) struct BadgeDomain;

impl AssetDomain for BadgeDomain {
    type Product = Badge;
}

impl SourceAssetImporter for BadgeDomain {
    const SOURCE_KIND: &'static str = "consumer.cook-badge";
    type Settings = BadgeSettings;

    fn limits() -> AssetImportLimits {
        AssetImportLimits::new(bytes(1024), bytes(16 * 1024), bytes(2048))
    }

    fn prepare<E>(source: &mut AssetImportSource<'_, E>) -> Result<Badge, AssetImportError<E>> {
        let candidate = source
            .metadata()
            .admit::<BadgeSettings>()
            .map_err(|_| AssetImportError::domain(invalid_badge()))?;
        let (identity, settings) = candidate.into_meta().into_parts();
        let product = AssetProductRef::new(identity.stable_id(), ImportedProductId::PRIMARY)
            .map_err(|_| AssetImportError::domain(invalid_badge()))?;
        let body: BadgeSource = serde_json::from_slice(source.bytes())
            .map_err(|_| AssetImportError::domain(invalid_badge()))?;
        let png = source.read_dependency(&body.png, bytes(PNG_BYTES))?;
        if !png.starts_with(PNG_SIGNATURE) || body.image.product() != ImportedProductId::PRIMARY {
            return Err(AssetImportError::domain(invalid_badge()));
        }
        let embedded = || BadgeArt::Embedded(Box::from(png.as_ref()));
        let (art, source_image, build_art) = match settings.image_delivery {
            ImageDelivery::Embedded => {
                EMBEDDED_IMPORTS.set(EMBEDDED_IMPORTS.get() + 1);
                (embedded(), Some(AssetRef::Path(body.png)), None)
            }
            ImageDelivery::Linked => (
                BadgeArt::Linked(body.image),
                Some(AssetRef::Product(body.image)),
                None,
            ),
            ImageDelivery::LinkedAtBuild => (embedded(), None, Some(BadgeArt::Linked(body.image))),
            ImageDelivery::StockAtBuild(dependencies) => {
                if !valid_stock_dependencies(&dependencies) {
                    return Err(AssetImportError::domain(invalid_badge()));
                }
                (embedded(), None, Some(BadgeArt::Stock(dependencies)))
            }
        };
        Ok(Badge {
            product,
            art,
            source_image,
            build_art,
        })
    }

    fn visit_source_dependencies<E>(
        badge: &Badge,
        mut visit: impl FnMut(&AssetRef, &AssetSourceKind, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        if let Some(image) = &badge.source_image {
            visit(
                image,
                &AssetSourceKind::Image,
                AssetReferenceDemand::Immediate,
            )?;
        }
        Ok(())
    }
}

impl SourceAssetBuild for BadgeDomain {
    type Codec = Self;
    const BUILD_ID: &'static str = "consumer.cook-badge.build";
    const BUILD_REVISION: u32 = 1;

    fn normalize_settings(settings: BadgeSettings) -> Result<BadgeSettings, Diagnostic> {
        Ok(settings)
    }

    fn build<E>(source: &mut AssetBuildSource<'_, E>) -> Result<Badge, AssetImportError<E>> {
        let mut badge = source.prepare_import::<Self>()?;
        if let Some(art) = badge.build_art.take() {
            badge.art = art;
        }
        Ok(badge)
    }

    fn visit_products<E>(
        badge: &Badge,
        mut visit: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        visit(badge.product)
    }
}

impl RuntimeAssetCodec<Badge> for BadgeDomain {
    const RUNTIME_KIND: &'static str = "consumer_cook_badge";
    const RUNTIME_VERSION: u32 = 1;

    fn runtime_limits() -> AssetRuntimeLimits {
        AssetRuntimeLimits::new(bytes(RUNTIME_BYTES), bytes(16 * 1024), bytes(2048))
    }

    fn encode_runtime(badge: &Badge) -> Result<Box<[u8]>, AssetRuntimeError> {
        serde_json::to_vec(badge)
            .map(Vec::into_boxed_slice)
            .map_err(|_| AssetRuntimeError::new(invalid_badge()))
    }

    fn decode_runtime(encoded: &[u8]) -> Result<Badge, AssetRuntimeError> {
        if encoded.len() > RUNTIME_BYTES {
            return Err(AssetRuntimeError::new(invalid_badge()));
        }
        let badge: Badge =
            serde_json::from_slice(encoded).map_err(|_| AssetRuntimeError::new(invalid_badge()))?;
        let valid_art = match &badge.art {
            BadgeArt::Embedded(png) => png.len() <= PNG_BYTES && png.starts_with(PNG_SIGNATURE),
            BadgeArt::Linked(product) => product.product() == ImportedProductId::PRIMARY,
            BadgeArt::Stock(dependencies) => valid_stock_dependencies(dependencies),
        };
        if badge.product.product() != ImportedProductId::PRIMARY || !valid_art {
            return Err(AssetRuntimeError::new(invalid_badge()));
        }
        Ok(badge)
    }

    fn runtime_retained_bytes(badge: &Badge) -> Result<usize, AssetRuntimeError> {
        let payload = match &badge.art {
            BadgeArt::Embedded(png) => png.len(),
            BadgeArt::Linked(_) => 0,
            BadgeArt::Stock(dependencies) => std::mem::size_of_val(dependencies.as_ref()),
        };
        Ok(size_of::<Badge>() + payload)
    }

    fn visit_runtime_dependencies<E>(
        badge: &Badge,
        mut visit: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        match &badge.art {
            BadgeArt::Embedded(_) => Ok(()),
            BadgeArt::Linked(image) => visit(*image, AssetReferenceDemand::Immediate),
            BadgeArt::Stock(dependencies) => {
                for &(product, demand) in dependencies.iter() {
                    visit(product, demand)?;
                }
                Ok(())
            }
        }
    }

    fn contains_runtime_product(badge: &Badge, product: AssetProductRef) -> bool {
        badge.product == product
    }
}

fn valid_stock_dependencies(dependencies: &[(AssetProductRef, AssetReferenceDemand)]) -> bool {
    (1..=MAX_STOCK_DEPENDENCIES).contains(&dependencies.len())
        && dependencies
            .iter()
            .all(|(product, _)| product.product() == ImportedProductId::PRIMARY)
}

fn bytes(limit: usize) -> ByteLimit {
    ByteLimit::new(limit).unwrap()
}

fn invalid_badge() -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new("consumer.cook-badge.invalid").unwrap(),
        SafeSummary::new("The badge or its PNG input is invalid").unwrap(),
    )
}
