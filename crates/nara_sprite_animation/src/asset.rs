//! Bounded source import and source-independent version-one JSON content.
//!
//! Source documents contain `format_version` and an explicit `clips` sequence. Runtime documents
//! additionally carry the complete `product`; both formats use the same clip/frame records and
//! owning constructors. Neither format selects an Image or acquires dependency bytes.

use std::{
    fmt,
    io::{self, Write},
    mem::size_of,
    time::Duration,
};

use nara_asset::{
    AssetDomain, AssetImportError, AssetImportLimits, AssetImportSource, AssetProductRef,
    AssetReferenceDemand, AssetRuntimeError, AssetRuntimeLimits, AssetSourceKind,
    ImportedProductId, RuntimeAssetCodec, SourceAssetBuild, SourceAssetImporter,
};
use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, Vec2, preflight_serde_shape};
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
use nara_sprite::TextureRegion;
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    Clip, Frame, MAX_CLIP_NAME_BYTES, MAX_CLIPS, MAX_FRAMES_PER_CLIP, MAX_TOTAL_FRAMES,
    PlaybackMode, SpriteAnimationSet,
};

const FORMAT_VERSION: u32 = 1;
const MAX_ENCODED_BYTES: usize = 4 * 1024 * 1024;
const MAX_DEPTH: usize = 8;
/// Every valid frame contributes eleven nodes, including field names and both UV arrays.
const MAX_NODES: usize = 32 + 8 * MAX_CLIPS + 12 * MAX_TOTAL_FRAMES;
const MAX_STRING_BYTES: usize =
    256 + MAX_CLIPS * (MAX_CLIP_NAME_BYTES + 32) + MAX_TOTAL_FRAMES * 32;
/// Includes the Clip's frame-end index as well as its frames, record and normalized boxed name.
const MAX_RETAINED_BYTES: usize = size_of::<SpriteAnimationSet>()
    + MAX_CLIPS * (size_of::<(Box<str>, Clip)>() + MAX_CLIP_NAME_BYTES)
    + MAX_TOTAL_FRAMES * (size_of::<Frame>() + size_of::<u64>());
/// Covers JSON adapter scratch, structural-preflight keys, DTO arrays/strings, construction's
/// overlapping Vec/boxed allocations and encoding's Vec/boxed output. Input and Host wrappers
/// are separately charged. These are conservative logical reservations, not RSS measurements.
const MAX_WORK_BYTES: usize =
    8 * MAX_ENCODED_BYTES + 256 * MAX_NODES + 1024 * MAX_DEPTH + 2 * MAX_RETAINED_BYTES;
/// Typed settings admission reparses normal Host metadata; its input/identity custody is separate.
const SETTINGS_WORK_BYTES: usize = 8 * 64 * 1024;

/// The only supported source-import settings value is exactly `{}`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SpriteAnimationSettings {}

impl<'de> Deserialize<'de> for SpriteAnimationSettings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EmptySettings;

        impl<'de> de::Visitor<'de> for EmptySettings {
            type Value = SpriteAnimationSettings;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("exactly an empty JSON object")
            }

            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                if map.next_key::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("sprite animation settings must be empty"));
                }
                Ok(SpriteAnimationSettings {})
            }
        }

        deserializer.deserialize_map(EmptySettings)
    }
}

/// Stateless import and tracked build of one source's complete primary animation Product.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteAnimationDomain;

/// Source-independent, deterministic JSON codec for one complete animation Product.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteAnimationCodec;

impl AssetDomain for SpriteAnimationDomain {
    type Product = SpriteAnimationSet;

    fn contains_product(product: &SpriteAnimationSet, id: ImportedProductId) -> bool {
        product.product().product() == id
    }
}

impl SourceAssetImporter for SpriteAnimationDomain {
    type Settings = SpriteAnimationSettings;

    const SOURCE_KIND: &'static str = "nara.sprite-animation";

    fn limits() -> AssetImportLimits {
        AssetImportLimits::new(
            byte_limit(MAX_ENCODED_BYTES),
            byte_limit(MAX_WORK_BYTES + SETTINGS_WORK_BYTES),
            byte_limit(MAX_RETAINED_BYTES),
        )
    }

    fn prepare<E>(
        source: &mut AssetImportSource<'_, E>,
    ) -> Result<SpriteAnimationSet, AssetImportError<E>> {
        let identity = source.metadata().identity();
        if !matches!(identity.source_kind(), AssetSourceKind::Other(kind) if kind == Self::SOURCE_KIND)
        {
            return Err(import_error(Rejection::SourceKind));
        }
        source
            .metadata()
            .admit::<Self::Settings>()
            .map_err(|_| import_error(Rejection::Settings))?;
        let product = AssetProductRef::new(identity.stable_id(), ImportedProductId::PRIMARY)
            .map_err(|_| import_error(Rejection::Identity))?;
        let descriptor: SourceDescriptor = decode_json(source.bytes()).map_err(import_error)?;
        decode_clips(descriptor.format_version, product, descriptor.clips).map_err(import_error)
    }
}

impl SourceAssetBuild for SpriteAnimationDomain {
    type Codec = SpriteAnimationCodec;

    const BUILD_ID: &'static str = "nara.sprite-animation.source-build";
    const BUILD_REVISION: u32 = 1;

    fn normalize_settings(settings: Self::Settings) -> Result<Self::Settings, Diagnostic> {
        Ok(settings)
    }

    fn visit_products<E>(
        product: &SpriteAnimationSet,
        mut visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        visitor(product.product())
    }
}

impl RuntimeAssetCodec<SpriteAnimationSet> for SpriteAnimationCodec {
    const RUNTIME_KIND: &'static str = "nara_sprite_animation";
    const RUNTIME_VERSION: u32 = FORMAT_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        AssetRuntimeLimits::new(
            byte_limit(MAX_ENCODED_BYTES),
            byte_limit(MAX_WORK_BYTES),
            byte_limit(MAX_RETAINED_BYTES),
        )
    }

    fn encode_runtime(product: &SpriteAnimationSet) -> Result<Box<[u8]>, AssetRuntimeError> {
        let descriptor = RuntimeDescriptor {
            format_version: FORMAT_VERSION,
            product: product.product(),
            clips: product
                .clips()
                .map(|(name, clip)| {
                    let frames = clip
                        .frames()
                        .iter()
                        .map(|frame| {
                            let region = frame.region();
                            Ok(JsonRecord(FrameDescriptor {
                                min: region.min.to_array(),
                                size: region.size.to_array(),
                                duration_nanos: u64::try_from(frame.duration().as_nanos())
                                    .map_err(|_| runtime_error(Rejection::Data))?,
                            }))
                        })
                        .collect::<Result<_, AssetRuntimeError>>()?;
                    Ok(JsonRecord(ClipDescriptor {
                        name: name.to_owned(),
                        mode: clip.mode(),
                        frames,
                    }))
                })
                .collect::<Result<_, AssetRuntimeError>>()?,
        };
        let mut counter = EncodedCounter(0);
        serde_json::to_writer(&mut counter, &descriptor)
            .map_err(|_| runtime_error(Rejection::Bounds))?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(counter.0)
            .map_err(|_| runtime_error(Rejection::Allocation))?;
        serde_json::to_writer(&mut bytes, &descriptor)
            .map_err(|_| runtime_error(Rejection::Shape))?;
        Ok(bytes.into_boxed_slice())
    }

    fn decode_runtime(bytes: &[u8]) -> Result<SpriteAnimationSet, AssetRuntimeError> {
        let descriptor: RuntimeDescriptor = decode_json(bytes).map_err(runtime_error)?;
        decode_clips(
            descriptor.format_version,
            descriptor.product,
            descriptor.clips,
        )
        .map_err(runtime_error)
    }

    fn runtime_retained_bytes(product: &SpriteAnimationSet) -> Result<usize, AssetRuntimeError> {
        let retained = product.retained_bytes();
        if retained > MAX_RETAINED_BYTES {
            return Err(runtime_error(Rejection::Bounds));
        }
        Ok(retained)
    }

    fn visit_runtime_dependencies<E>(
        _product: &SpriteAnimationSet,
        _visitor: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        Ok(())
    }

    fn contains_runtime_product(product: &SpriteAnimationSet, reference: AssetProductRef) -> bool {
        product.product() == reference
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceDescriptor {
    format_version: u32,
    clips: Vec<JsonRecord<ClipDescriptor>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeDescriptor {
    format_version: u32,
    product: AssetProductRef,
    clips: Vec<JsonRecord<ClipDescriptor>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipDescriptor {
    name: String,
    mode: PlaybackMode,
    frames: Vec<JsonRecord<FrameDescriptor>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameDescriptor {
    min: [f32; 2],
    size: [f32; 2],
    duration_nanos: u64,
}

/// Keeps JSON clip/frame records as objects while delegating field decoding to Serde.
#[derive(Serialize)]
#[serde(transparent)]
struct JsonRecord<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for JsonRecord<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RecordVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T: Deserialize<'de>> de::Visitor<'de> for RecordVisitor<T> {
            type Value = JsonRecord<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON record object")
            }

            fn visit_map<M: de::MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                T::deserialize(de::value::MapAccessDeserializer::new(map)).map(JsonRecord)
            }
        }

        deserializer.deserialize_map(RecordVisitor(std::marker::PhantomData))
    }
}

fn decode_json<T: de::DeserializeOwned>(bytes: &[u8]) -> Result<T, Rejection> {
    if bytes.len() > MAX_ENCODED_BYTES {
        return Err(Rejection::Bounds);
    }
    if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
        return Err(Rejection::Shape);
    }
    let shape = SerdeShapeLimits::new(
        DepthLimit::new(MAX_DEPTH).expect("fixed nonzero sprite animation depth"),
        ItemLimit::new(MAX_NODES).expect("fixed nonzero sprite animation node limit"),
        ItemLimit::new(MAX_FRAMES_PER_CLIP).expect("fixed nonzero sprite animation item limit"),
        byte_limit(MAX_CLIP_NAME_BYTES),
        byte_limit(MAX_STRING_BYTES),
    );
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut parser, shape).map_err(|_| Rejection::Shape)?;
    parser.end().map_err(|_| Rejection::Shape)?;
    serde_json::from_slice(bytes).map_err(|_| Rejection::Shape)
}

fn decode_clips(
    version: u32,
    product: AssetProductRef,
    clips: Vec<JsonRecord<ClipDescriptor>>,
) -> Result<SpriteAnimationSet, Rejection> {
    if version != FORMAT_VERSION {
        return Err(Rejection::Version);
    }
    let clips = clips
        .into_iter()
        .map(|JsonRecord(clip)| {
            let frames = clip
                .frames
                .into_iter()
                .map(|JsonRecord(frame)| {
                    Frame::new(
                        TextureRegion::new(
                            Vec2::from_array(frame.min),
                            Vec2::from_array(frame.size),
                        ),
                        Duration::from_nanos(frame.duration_nanos),
                    )
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| Rejection::Data)?;
            let admitted = Clip::new(clip.mode, frames).map_err(|_| Rejection::Data)?;
            Ok((clip.name, admitted))
        })
        .collect::<Result<Vec<_>, Rejection>>()?;
    SpriteAnimationSet::new(product, clips).map_err(|_| Rejection::Data)
}

struct EncodedCounter(usize);

impl Write for EncodedCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|total| *total <= MAX_ENCODED_BYTES)
            .ok_or_else(|| io::Error::other("sprite animation exceeds its encoded byte limit"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn byte_limit(bytes: usize) -> ByteLimit {
    ByteLimit::new(bytes).expect("fixed nonzero sprite animation byte limit")
}

#[derive(Clone, Copy)]
enum Rejection {
    Bounds,
    Shape,
    Version,
    SourceKind,
    Identity,
    Settings,
    Data,
    Allocation,
}

impl Rejection {
    fn diagnostic(self) -> Diagnostic {
        let (code, summary) = match self {
            Self::Bounds => (
                "nara.sprite-animation.bounds",
                "Sprite animation exceeds its encoded or retained byte allowance",
            ),
            Self::Shape => (
                "nara.sprite-animation.shape",
                "Sprite animation requires a bounded strict JSON document with explicit clip and frame sequences",
            ),
            Self::Version => (
                "nara.sprite-animation.version",
                "Sprite animation format version is unsupported",
            ),
            Self::SourceKind => (
                "nara.sprite-animation.source-kind",
                "Sprite animation metadata must select the sprite animation source kind",
            ),
            Self::Identity => (
                "nara.sprite-animation.identity",
                "Sprite animation requires a complete non-nil Product identity",
            ),
            Self::Settings => (
                "nara.sprite-animation.settings",
                "Sprite animation settings must be exactly an empty JSON object",
            ),
            Self::Data => (
                "nara.sprite-animation.data",
                "Sprite animation clips must satisfy the domain name, frame, UV and duration limits",
            ),
            Self::Allocation => (
                "nara.sprite-animation.allocation",
                "Sprite animation output storage could not be reserved",
            ),
        };
        Diagnostic::error(
            DiagnosticCode::new(code).expect("fixed valid sprite animation diagnostic code"),
            SafeSummary::new(summary).expect("fixed safe sprite animation diagnostic summary"),
        )
    }
}

fn import_error<E>(error: Rejection) -> AssetImportError<E> {
    AssetImportError::domain(error.diagnostic())
}

fn runtime_error(error: Rejection) -> AssetRuntimeError {
    AssetRuntimeError::new(error.diagnostic())
}
