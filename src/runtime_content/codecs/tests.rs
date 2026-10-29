use std::sync::atomic::{AtomicUsize, Ordering};

use nara_asset::{AssetRuntimeError, ImportedProductId, StableAssetId};
use nara_core::ByteLimit;
use nara_fs::ContentDigest;

use super::*;

struct Family;
struct OtherFamily;
impl AssetDomain for Family {
    type Product = u32;
}
impl AssetDomain for OtherFamily {
    type Product = u32;
}

struct First;
struct Later;
struct Substitute;

macro_rules! codec {
    ($codec:ty, $kind:literal, $version:literal) => {
        impl RuntimeAssetCodec<u32> for $codec {
            const RUNTIME_KIND: &'static str = $kind;
            const RUNTIME_VERSION: u32 = $version;
            fn runtime_limits() -> AssetRuntimeLimits {
                AssetRuntimeLimits::new(
                    ByteLimit::new(64).unwrap(),
                    ByteLimit::new(128).unwrap(),
                    ByteLimit::new(32).unwrap(),
                )
            }
            fn encode_runtime(value: &u32) -> Result<Box<[u8]>, AssetRuntimeError> {
                Ok(value.to_le_bytes().into())
            }
            fn decode_runtime(bytes: &[u8]) -> Result<u32, AssetRuntimeError> {
                Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
            }
            fn runtime_retained_bytes(_: &u32) -> Result<usize, AssetRuntimeError> {
                Ok(size_of::<u32>())
            }
            fn visit_required_runtime_products<E>(
                _: &u32,
                _: impl FnMut(AssetProductRef) -> Result<(), E>,
            ) -> Result<(), E> {
                Ok(())
            }
            fn contains_runtime_product(_: &u32, product: AssetProductRef) -> bool {
                product.product() == ImportedProductId::PRIMARY
            }
        }
    };
}

codec!(First, "consumer_number", 1);
codec!(Later, "consumer_number", 2);
codec!(Substitute, "consumer_number", 1);

fn id(name: &'static str, revision: u32) -> RuntimeCodecDefinitionId {
    RuntimeCodecDefinitionId::new(name, revision)
}

fn member(version: u32) -> RuntimeContentMember {
    let product = AssetProductRef::new(
        StableAssetId::parse_str("1ba673b7-1a8d-4a72-bfe5-36158af23f72").unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap();
    RuntimeContentMember::new(
        "number.bin",
        ContentDigest::of_bytes(&3_u32.to_le_bytes()),
        RuntimeContentMemberKind::Asset {
            format: FormatKind::new("consumer_number").unwrap(),
            version: FormatVersion::new(version).unwrap(),
        },
        vec![product],
        Vec::new(),
    )
    .unwrap()
}

#[test]
fn multiple_formats_of_one_family_have_order_independent_agreement() {
    let mut left = RuntimeCodecs::new();
    left.add::<Family, First>(id("consumer.number.first", 1), RuntimeCodecRole::Cpu)
        .unwrap();
    left.add::<Family, Later>(id("consumer.number.later", 1), RuntimeCodecRole::Cpu)
        .unwrap();
    let mut right = RuntimeCodecs::new();
    right
        .add::<Family, Later>(id("consumer.number.later", 1), RuntimeCodecRole::Cpu)
        .unwrap();
    right
        .add::<Family, First>(id("consumer.number.first", 1), RuntimeCodecRole::Cpu)
        .unwrap();
    assert_eq!(left.fingerprint(), right.fingerprint());
    assert_eq!(left.families.len(), 1);
    assert!(left.selected_limits::<Family, First>(&member(1)).is_ok());
    assert!(left.selected_limits::<Family, Later>(&member(2)).is_ok());
}

#[test]
fn family_execution_order_follows_canonical_definitions_not_recipe_insertion() {
    let mut left = RuntimeCodecs::new();
    left.add::<OtherFamily, Later>(id("consumer.z", 1), RuntimeCodecRole::Cpu)
        .unwrap();
    left.add::<Family, First>(id("consumer.a", 1), RuntimeCodecRole::Cpu)
        .unwrap();
    let mut right = RuntimeCodecs::new();
    right
        .add::<Family, First>(id("consumer.a", 1), RuntimeCodecRole::Cpu)
        .unwrap();
    right
        .add::<OtherFamily, Later>(id("consumer.z", 1), RuntimeCodecRole::Cpu)
        .unwrap();
    assert_eq!(left.fingerprint(), right.fingerprint());
    let families = |codecs: &RuntimeCodecs| {
        codecs
            .families
            .iter()
            .map(|family| family.family_type())
            .collect::<Vec<_>>()
    };
    assert_eq!(families(&left), families(&right));
    assert_eq!(
        families(&left),
        [TypeId::of::<Family>(), TypeId::of::<OtherFamily>()]
    );
}

#[test]
fn stable_revision_and_concrete_role_are_independent_of_format_version() {
    let selected = |revision, role| {
        let mut codecs = RuntimeCodecs::new();
        codecs
            .add::<Family, First>(id("consumer.number", revision), role)
            .unwrap();
        codecs.fingerprint()
    };
    assert_ne!(
        selected(1, RuntimeCodecRole::Cpu),
        selected(2, RuntimeCodecRole::Cpu)
    );
    assert_ne!(
        selected(1, RuntimeCodecRole::Cpu),
        selected(1, RuntimeCodecRole::Image)
    );
}

#[test]
fn duplicate_definitions_and_competing_selectors_refuse_without_replacing_the_selection() {
    let mut codecs = RuntimeCodecs::new();
    codecs
        .add::<Family, First>(id("consumer.number", 1), RuntimeCodecRole::Cpu)
        .unwrap();
    let original = codecs.fingerprint();
    assert!(matches!(
        codecs.add::<Family, Later>(id("consumer.number", 1), RuntimeCodecRole::Cpu),
        Err(RuntimeCodecSelectionError::DuplicateDefinition { .. })
    ));
    assert!(matches!(
        codecs.add::<OtherFamily, Substitute>(id("consumer.substitute", 1), RuntimeCodecRole::Cpu),
        Err(RuntimeCodecSelectionError::AmbiguousFormat { .. })
    ));
    assert_eq!(codecs.fingerprint(), original);
    assert!(matches!(
        codecs.selected_limits::<Family, Substitute>(&member(1)),
        Err(RuntimeContentError::CodecNotSelected)
    ));
    assert!(matches!(
        codecs.selected_limits::<OtherFamily, First>(&member(1)),
        Err(RuntimeContentError::CodecNotSelected)
    ));
    assert!(matches!(
        codecs.selected_limits::<Family, First>(&member(2)),
        Err(RuntimeContentError::CodecMismatch)
    ));
}

static FROZEN_LIMIT: AtomicUsize = AtomicUsize::new(64);
struct Frozen;
impl RuntimeAssetCodec<u32> for Frozen {
    const RUNTIME_KIND: &'static str = "consumer_frozen";
    const RUNTIME_VERSION: u32 = 1;
    fn runtime_limits() -> AssetRuntimeLimits {
        AssetRuntimeLimits::new(
            ByteLimit::new(FROZEN_LIMIT.load(Ordering::SeqCst)).unwrap(),
            ByteLimit::new(128).unwrap(),
            ByteLimit::new(32).unwrap(),
        )
    }
    fn encode_runtime(value: &u32) -> Result<Box<[u8]>, AssetRuntimeError> {
        First::encode_runtime(value)
    }
    fn decode_runtime(bytes: &[u8]) -> Result<u32, AssetRuntimeError> {
        First::decode_runtime(bytes)
    }
    fn runtime_retained_bytes(value: &u32) -> Result<usize, AssetRuntimeError> {
        First::runtime_retained_bytes(value)
    }
    fn visit_required_runtime_products<E>(
        value: &u32,
        visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        First::visit_required_runtime_products(value, visitor)
    }
    fn contains_runtime_product(value: &u32, product: AssetProductRef) -> bool {
        First::contains_runtime_product(value, product)
    }
}

#[test]
fn composition_freezes_the_actual_predecode_limits_in_its_agreement() {
    let mut codecs = RuntimeCodecs::new();
    codecs
        .add::<Family, Frozen>(id("consumer.frozen", 1), RuntimeCodecRole::Cpu)
        .unwrap();
    let fingerprint = codecs.fingerprint();
    FROZEN_LIMIT.store(128, Ordering::SeqCst);
    let mut changed = RuntimeCodecs::new();
    changed
        .add::<Family, Frozen>(id("consumer.frozen", 1), RuntimeCodecRole::Cpu)
        .unwrap();
    assert_ne!(fingerprint, changed.fingerprint());
    assert_eq!(codecs.definitions[0].limits.encoded_bytes().get(), 64);
    assert_eq!(codecs.fingerprint(), fingerprint);
}
