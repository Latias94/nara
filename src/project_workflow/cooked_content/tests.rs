use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use nara_app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
use nara_asset::{
    AssetPathScope, AssetRecord, AssetSourceKind, AssetSourcePath, ImportedProductId,
    ProjectAssetDatabase, StableAssetId,
};
use nara_core::Vec2;
use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_identity::SceneEntityId;
use nara_image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImageRuntimeCodec};
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMapBuilder, ActionMapLimits, KeyCode,
};
use nara_project::{ProductCapability, RuntimePreset};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueShape, PersistentComponentSchemaOwnerId,
    encode_asset_reference_value,
};
use nara_scene::{
    RuntimeSceneLimits, SceneComponentRecord, SceneDocument, SceneEntityRecord, SourceDocumentKey,
    SourceDocumentKind,
};
use nara_tilemap::TileAtlasLayout;

use super::*;
use crate::{
    SchemaContribution,
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildLimits, ArtifactBuildOutput, ArtifactBuildSeed,
        ArtifactBuildStore, ArtifactContentDigest, ArtifactGroupCandidate,
    },
    runtime_content::RuntimeCodecDefinitionId,
};

const COMPONENT: &str = "nara.test.CookedReferences";
const FUTURE: u8 = 3;
const INPUT: u8 = 10;
const ROOT_PREFIX: &str = "nara-cooked-content-";
static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

fn product(tag: u8) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(format!("7c310000-0000-4000-8000-{tag:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn auxiliary(tag: u8) -> AssetProductRef {
    AssetProductRef::new(
        product(tag).source(),
        ImportedProductId::parse_subproduct(&format!("8c310000-0000-4000-8000-{tag:012x}"))
            .unwrap(),
    )
    .unwrap()
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("nara.test.cooked-content").unwrap()
}

#[derive(Default)]
struct Definitions;

impl Plugin for Definitions {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.cooked-content"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }
    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn scene_recipe() -> ProductRecipe {
    let capabilities = [ComponentCapability::Scene, ComponentCapability::AssetRef];
    let schema = ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Cooked references",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities([ComponentCapability::Scene])
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("now"),
            "Now",
            ComponentFieldPath::from_fields(["now"]),
            ComponentValueShape::list(ComponentValueShape::asset_ref(
                AssetReferenceDemand::Immediate,
            )),
        )
        .with_capabilities(capabilities),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("future"),
            "Future",
            ComponentFieldPath::from_fields(["future"]),
            ComponentValueShape::asset_ref(AssetReferenceDemand::Deferred),
            encode_asset_reference_value(&AssetRef::Product(product(FUTURE))),
        )
        .with_capabilities(capabilities),
    ]);
    let snapshot = ComponentSchemaOwnerSnapshot::admit(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    ProductRecipe::new()
        .add_contribution(SchemaContribution::<Definitions>::data_for_default([snapshot]).unwrap())
        .unwrap()
}

fn recipe() -> ProductRecipe {
    scene_recipe()
        .add_runtime_asset_codec::<ActionMapDomain, ActionMapRuntimeCodec>(
            RuntimeCodecDefinitionId::new("nara.test.cooked.input", 1),
        )
        .unwrap()
}

fn request() -> CookedContentRequest {
    let mut configuration = RuntimeConfiguration::default();
    configuration.runtime.preset = RuntimePreset::LocalHeadless;
    configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
    configuration.tasks.io.workers = 1;
    configuration.tasks.compute.workers = 1;
    configuration.tasks.async_compute.workers = 1;
    CookedContentRequest {
        target: RuntimeContentTarget::new("x86_64-pc-windows-msvc", "delivery-test").unwrap(),
        configuration,
        startup_scene: product(1),
        scenes: vec![product(1)],
        addressable_roots: Vec::new(),
    }
}

fn limits() -> CookedContentLimits {
    let bytes = |value| ByteLimit::new(value).unwrap();
    CookedContentLimits {
        runtime: RuntimeContentLimits::default()
            .with_scene(RuntimeSceneLimits::default().with_working_bytes(bytes(1024 * 1024)))
            .with_work_bytes(bytes(32 * 1024 * 1024))
            .with_retained_bytes(bytes(32 * 1024 * 1024))
            .with_catalog(
                crate::runtime_content::RuntimeContentCatalogLimits::default()
                    .with_working_bytes(bytes(16 * 1024 * 1024))
                    .with_retained_bytes(bytes(8 * 1024 * 1024)),
            ),
        ..CookedContentLimits::default()
    }
}

fn database() -> ProjectAssetDatabase {
    let mut database = ProjectAssetDatabase::default();
    for tag in [1, 2, FUTURE, INPUT, 11] {
        database
            .insert(AssetRecord::new(
                product(tag).source(),
                AssetSourcePath::new(format!("assets/{tag}.fixture")).unwrap(),
                AssetSourceKind::Unknown,
            ))
            .unwrap();
    }
    database
}

fn scene(tag: u8, immediate: Option<&[AssetProductRef]>) -> ExpandedSceneCandidate {
    let mut entity = SceneEntityRecord::new(SceneEntityId::new("root").unwrap());
    if let Some(references) = immediate {
        entity = entity.with_component(
            ComponentTypeId::new(COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "now",
                    ComponentValue::List(
                        references
                            .iter()
                            .map(|reference| {
                                encode_asset_reference_value(&AssetRef::Product(*reference))
                            })
                            .collect(),
                    ),
                )]),
            )
            .with_owner(owner()),
        );
    }
    ExpandedSceneCandidate::plain(
        SourceDocumentKey::stable_asset(product(tag).source(), SourceDocumentKind::Scene),
        Arc::new(SceneDocument::from_ordered_roots([entity])),
    )
}

fn add_scene(builder: &mut CookedContentBuilder, tag: u8, immediate: Option<&[AssetProductRef]>) {
    builder
        .add_scene(
            product(tag),
            &scene(tag, immediate),
            database().resolver(&AssetPathScope::ROOT),
        )
        .unwrap();
}

fn input() -> ActionMapAsset {
    input_for(INPUT)
}

fn input_for(tag: u8) -> ActionMapAsset {
    let mut definition = ActionMapBuilder::new(ActionMapLimits::default());
    definition
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    definition
        .bind(ActionBinding::key(
            ActionId::new("jump").unwrap(),
            KeyCode::Space,
        ))
        .unwrap();
    ActionMapAsset::new(product(tag), definition.compile().unwrap()).unwrap()
}

struct StoreRoot(PathBuf);

impl StoreRoot {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "{ROOT_PREFIX}{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn open(&self) -> ArtifactBuildStore {
        #[cfg(windows)]
        let handle = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(0x1 | 0x2 | 0x4)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.0)
                .unwrap()
        };
        #[cfg(not(windows))]
        let handle = fs::File::open(&self.0).unwrap();
        let capability = DirectoryCapability::from_host_handle(
            handle,
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap();
        ArtifactBuildStore::open(capability, ArtifactBuildLimits::default()).unwrap()
    }
    fn remove(&self) {
        if self.0.exists() {
            let absolute = self.0.canonicalize().unwrap();
            assert!(absolute.starts_with(std::env::temp_dir().canonicalize().unwrap()));
            assert!(
                absolute
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(ROOT_PREFIX)
            );
            fs::remove_dir_all(absolute).unwrap();
        }
    }
}
impl Drop for StoreRoot {
    fn drop(&mut self) {
        self.remove();
    }
}

fn group(
    store: &ArtifactBuildStore,
    bytes: &[u8],
    mapping: AssetProductRef,
    format: &str,
    dependencies: Vec<(AssetProductRef, AssetReferenceDemand)>,
) -> VerifiedArtifactGroup {
    store
        .commit(
            ArtifactGroupCandidate {
                seed: ArtifactBuildSeed {
                    source: mapping,
                    location: None,
                    primary: None,
                    definitions: vec![ArtifactBuildDefinition {
                        id: "nara.test.cooked.operation".into(),
                        revision: FormatVersion::ONE,
                        implementation: ArtifactContentDigest::of_bytes(
                            b"cooked content owner fixture",
                        ),
                    }],
                    settings: Vec::new(),
                    target: Vec::new(),
                    format: FormatKind::new("fixture_operation").unwrap(),
                    format_version: FormatVersion::ONE,
                },
                observations: Vec::new(),
                outputs: vec![ArtifactBuildOutput {
                    name: "unrelated-cache-output-name".into(),
                    format: FormatKind::new(format).unwrap(),
                    format_version: FormatVersion::ONE,
                    products: vec![mapping],
                    dependencies,
                    bytes: Arc::from(bytes),
                }],
            },
            |_| Ok(()),
        )
        .unwrap()
        .into_group()
}

fn replace_scene_bytes(
    builder: &mut CookedContentBuilder,
    tag: u8,
    bytes: &[u8],
    update_digest: bool,
) {
    let pending = builder
        .members
        .iter_mut()
        .find(|member| member.member.products() == [product(tag)])
        .unwrap();
    let old = &pending.member;
    pending.member = RuntimeContentMember::new(
        old.path(),
        if update_digest {
            ContentDigest::of_bytes(bytes)
        } else {
            old.digest()
        },
        old.kind().clone(),
        old.products().to_vec(),
        old.dependencies().to_vec(),
    )
    .unwrap();
    pending.bytes = MemberBytes::Owned(bytes.to_vec());
}

#[test]
fn actual_scene_and_input_cooking_is_canonical_across_insertion_order() {
    let prepare = |reverse| {
        let mut request = request();
        request.scenes.push(product(2));
        request.configuration.input.action_map = Some(product(INPUT));
        let mut builder = CookedContentBuilder::new(request, recipe(), limits()).unwrap();
        if reverse {
            builder.add_input(&input()).unwrap();
            add_scene(&mut builder, 2, None);
            add_scene(&mut builder, 1, None);
        } else {
            add_scene(&mut builder, 1, None);
            add_scene(&mut builder, 2, None);
            builder.add_input(&input()).unwrap();
        }
        builder.finish().unwrap()
    };
    let first = prepare(false);
    let second = prepare(true);
    assert_eq!(
        first.expectation().catalog_digest(),
        second.expectation().catalog_digest()
    );
    let mut packs = [Vec::new(), Vec::new()];
    first.base().write_pack(&mut packs[0]).unwrap();
    second.base().write_pack(&mut packs[1]).unwrap();
    assert_eq!(packs[0], packs[1]);
    assert!(first.retained_bytes() > packs[0].len());
    assert_eq!(first.base().validate_deep().unwrap().members(), 3);
    assert_eq!(
        first.base().acquire_scene(product(2)).unwrap().product(),
        product(2)
    );
    let input = first
        .base()
        .acquire_asset::<ActionMapDomain, ActionMapRuntimeCodec>(product(INPUT))
        .unwrap();
    assert_eq!(input.product().definition().bindings().len(), 1);
    assert!(
        first
            .base()
            .catalog()
            .members()
            .iter()
            .all(|member| member.path().starts_with("members/"))
    );
    let base = first.into_base();
    assert_eq!(base.acquire_startup_scene().unwrap().product(), product(1));
}

#[test]
fn effective_default_deferred_scene_is_delivered_and_validated_without_travel_authority() {
    let mut builder = CookedContentBuilder::new(request(), recipe(), limits()).unwrap();
    add_scene(&mut builder, 1, Some(&[]));
    add_scene(&mut builder, FUTURE, None);
    let content = builder.finish().unwrap();
    let dependencies = content.base().member(product(1)).unwrap().dependencies();
    assert_eq!(
        dependencies,
        &[RuntimeContentDependency::new(
            product(FUTURE),
            RuntimeContentDependencyRole::Reference
        )]
    );
    assert_eq!(content.base().validate_deep().unwrap().members(), 2);
    assert!(matches!(
        content.base().acquire_scene(product(FUTURE)),
        Err(RuntimeContentError::UndeclaredScene)
    ));
}

#[test]
fn immediate_use_takes_precedence_over_the_same_deferred_default() {
    let mut builder = CookedContentBuilder::new(request(), recipe(), limits()).unwrap();
    add_scene(&mut builder, 1, Some(&[product(FUTURE), product(FUTURE)]));
    add_scene(&mut builder, FUTURE, None);
    let content = builder.finish().unwrap();
    assert_eq!(
        content.base().member(product(1)).unwrap().dependencies(),
        &[RuntimeContentDependency::new(
            product(FUTURE),
            RuntimeContentDependencyRole::Required
        ),]
    );
}

#[test]
fn missing_deferred_destination_is_a_delivery_error() {
    let mut builder = CookedContentBuilder::new(request(), recipe(), limits()).unwrap();
    add_scene(&mut builder, 1, Some(&[]));
    assert!(
        matches!(builder.finish(), Err(CookedContentError::Runtime(RuntimeContentError::Catalog(
        RuntimeContentCatalogError::MissingDependencyProduct { product: missing, .. },
    ))) if missing == product(FUTURE))
    );
}

#[test]
fn corrupt_deferred_payload_is_decoded_even_when_its_digest_agrees() {
    let mut builder = CookedContentBuilder::new(request(), recipe(), limits()).unwrap();
    add_scene(&mut builder, 1, Some(&[]));
    add_scene(&mut builder, FUTURE, None);
    replace_scene_bytes(&mut builder, FUTURE, b"not a Runtime Scene", true);
    assert!(matches!(
        builder.finish(),
        Err(CookedContentError::Runtime(RuntimeContentError::Scene(_)))
    ));
}

#[test]
fn surplus_members_are_explicitly_refused_instead_of_escaping_deep_validation() {
    let mut builder = CookedContentBuilder::new(request(), recipe(), limits()).unwrap();
    add_scene(&mut builder, 1, None);
    add_scene(&mut builder, 2, None);
    assert!(matches!(
        builder.finish(),
        Err(CookedContentError::UnreachableMembers {
            reachable: 1,
            total: 2
        })
    ));
}

#[test]
fn missing_global_input_and_unselected_input_codec_are_explicit_errors() {
    let mut with_input = request();
    with_input.configuration.input.action_map = Some(product(INPUT));
    let mut missing = CookedContentBuilder::new(with_input, recipe(), limits()).unwrap();
    add_scene(&mut missing, 1, None);
    assert!(matches!(
        missing.finish(),
        Err(CookedContentError::Runtime(RuntimeContentError::Catalog(
            RuntimeContentCatalogError::InvalidStartupInput { .. },
        )))
    ));
    let mut unselected = CookedContentBuilder::new(request(), scene_recipe(), limits()).unwrap();
    let retained = unselected.retained_bytes;
    assert!(matches!(
        unselected.add_input(&input()),
        Err(CookedContentError::Runtime(
            RuntimeContentError::CodecNotSelected
        ))
    ));
    assert_eq!(unselected.retained_bytes, retained);
    assert!(unselected.members.is_empty());
    add_scene(&mut unselected, 1, None);
    unselected.finish().unwrap();
}

#[test]
fn failed_scene_cook_preserves_the_source_and_prior_builder_members() {
    let mut request = request();
    request.scenes.push(product(2));
    let mut builder = CookedContentBuilder::new(request, recipe(), limits()).unwrap();
    add_scene(&mut builder, 1, None);
    let retained = builder.retained_bytes;
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("unknown-component").unwrap(),
    )
    .with_component(
        ComponentTypeId::new("nara.test.UnselectedComponent"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
    )]);
    let before = document.clone();
    let expanded = ExpandedSceneCandidate::plain(
        SourceDocumentKey::stable_asset(product(2).source(), SourceDocumentKind::Scene),
        Arc::new(document),
    );
    assert!(matches!(
        builder.add_scene(
            product(2),
            &expanded,
            database().resolver(&AssetPathScope::ROOT),
        ),
        Err(CookedContentError::Runtime(RuntimeContentError::Scene(_)))
    ));
    assert_eq!(expanded.document(), &before);
    assert_eq!(builder.retained_bytes, retained);
    assert_eq!(builder.members.len(), 1);
    add_scene(&mut builder, 2, None);
    assert_eq!(
        builder.finish().unwrap().base().catalog().members().len(),
        2
    );
}

#[test]
fn group_retains_actual_bytes_across_cache_removal_and_duplicate_add_is_transactional() {
    let root = StoreRoot::new();
    let store = root.open();
    let encoded = ActionMapRuntimeCodec::encode_runtime(&input()).unwrap();
    let group = group(
        &store,
        &encoded,
        product(INPUT),
        ActionMapRuntimeCodec::RUNTIME_KIND,
        vec![],
    );
    let mut request = request();
    request.configuration.input.action_map = Some(product(INPUT));
    let mut builder = CookedContentBuilder::new(request, recipe(), limits()).unwrap();
    add_scene(&mut builder, 1, None);
    builder.add_group(&group).unwrap();
    assert_eq!(builder.groups.len(), 1);
    let retained = builder.retained_bytes;
    assert!(
        matches!(builder.add_group(&group), Err(CookedContentError::DuplicateProduct(duplicate)) if duplicate == product(INPUT))
    );
    assert_eq!(builder.groups.len(), 1);
    assert_eq!(builder.retained_bytes, retained);
    drop(group);
    drop(store);
    root.remove();
    let content = builder.finish().unwrap();
    assert!(!root.0.exists());
    let member = content.base().member(product(INPUT)).unwrap();
    assert_eq!(member.digest(), ContentDigest::of_bytes(&encoded));
    assert!(!member.path().contains("unrelated-cache-output-name"));
    assert_eq!(
        content
            .base()
            .acquire_asset::<ActionMapDomain, ActionMapRuntimeCodec>(product(INPUT))
            .unwrap()
            .product()
            .product(),
        product(INPUT)
    );
}

#[test]
fn a_multi_member_group_cannot_partially_add_before_a_later_duplicate() {
    let root = StoreRoot::new();
    let store = root.open();
    let first = ActionMapRuntimeCodec::encode_runtime(&input_for(11)).unwrap();
    let second = ActionMapRuntimeCodec::encode_runtime(&input()).unwrap();
    let template = group(
        &store,
        &second,
        product(INPUT),
        ActionMapRuntimeCodec::RUNTIME_KIND,
        vec![],
    );
    let mut seed = template.seed().clone();
    seed.settings = b"two complete members".to_vec();
    let group = store
        .commit(
            ArtifactGroupCandidate {
                seed,
                observations: vec![],
                outputs: vec![
                    ArtifactBuildOutput {
                        name: "a-new-product".into(),
                        format: FormatKind::new(ActionMapRuntimeCodec::RUNTIME_KIND).unwrap(),
                        format_version: FormatVersion::ONE,
                        products: vec![product(11)],
                        dependencies: vec![],
                        bytes: Arc::from(first.as_ref()),
                    },
                    ArtifactBuildOutput {
                        name: "z-already-added-product".into(),
                        format: FormatKind::new(ActionMapRuntimeCodec::RUNTIME_KIND).unwrap(),
                        format_version: FormatVersion::ONE,
                        products: vec![product(INPUT)],
                        dependencies: vec![],
                        bytes: Arc::from(second.as_ref()),
                    },
                ],
            },
            |_| Ok(()),
        )
        .unwrap()
        .into_group();
    let mut selected = request();
    selected.configuration.input.action_map = Some(product(INPUT));
    selected.addressable_roots.push(product(11));
    let mut builder = CookedContentBuilder::new(selected.clone(), recipe(), limits()).unwrap();
    add_scene(&mut builder, 1, None);
    builder.add_input(&input()).unwrap();
    let retained = builder.retained_bytes;
    assert!(
        matches!(builder.add_group(&group), Err(CookedContentError::DuplicateProduct(duplicate)) if duplicate == product(INPUT))
    );
    assert_eq!(builder.retained_bytes, retained);
    assert_eq!(builder.members.len(), 2);
    assert!(builder.groups.is_empty());
    builder.add_input(&input_for(11)).unwrap();
    builder.finish().unwrap();

    let mut complete = CookedContentBuilder::new(selected, recipe(), limits()).unwrap();
    add_scene(&mut complete, 1, None);
    complete.add_group(&group).unwrap();
    assert_eq!(
        complete
            .finish()
            .unwrap()
            .base()
            .validate_deep()
            .unwrap()
            .members(),
        3
    );
}

#[test]
fn verified_group_integrity_does_not_replace_actual_payload_or_mapping_validation() {
    for case in 0..3 {
        let root = StoreRoot::new();
        let store = root.open();
        let encoded = if case == 0 {
            b"malformed ActionMap".to_vec()
        } else {
            ActionMapRuntimeCodec::encode_runtime(&input())
                .unwrap()
                .into_vec()
        };
        let mapping = if case == 1 {
            product(11)
        } else {
            product(INPUT)
        };
        let dependencies = if case == 2 {
            vec![(product(FUTURE), AssetReferenceDemand::Deferred)]
        } else {
            vec![]
        };
        let group = group(
            &store,
            &encoded,
            mapping,
            ActionMapRuntimeCodec::RUNTIME_KIND,
            dependencies,
        );
        let mut request = request();
        request.addressable_roots.push(mapping);
        let mut builder = CookedContentBuilder::new(request, recipe(), limits()).unwrap();
        add_scene(&mut builder, 1, None);
        if case == 2 {
            add_scene(&mut builder, FUTURE, None);
        }
        builder.add_group(&group).unwrap();
        let error = builder.finish().unwrap_err();
        assert!(match (case, error) {
            (0, CookedContentError::Runtime(RuntimeContentError::Asset(_))) => true,
            (1, CookedContentError::Runtime(RuntimeContentError::ProductIdentityMismatch)) => true,
            (2, CookedContentError::Runtime(RuntimeContentError::DependencyMismatch { .. })) =>
                true,
            _ => false,
        });
    }
}

#[test]
fn group_retention_exact_and_one_short_preserve_the_builder_for_retry() {
    let root = StoreRoot::new();
    let store = root.open();
    let encoded = ActionMapRuntimeCodec::encode_runtime(&input()).unwrap();
    let group = group(
        &store,
        &encoded,
        product(INPUT),
        ActionMapRuntimeCodec::RUNTIME_KIND,
        vec![],
    );
    let mut probe = CookedContentBuilder::new(request(), recipe(), limits()).unwrap();
    add_scene(&mut probe, 1, None);
    let before = probe.retained_bytes;
    probe.add_group(&group).unwrap();
    let exact = probe.retained_bytes;
    assert!(exact >= before + group.retained_bytes());

    let mut builder = CookedContentBuilder::new(request(), recipe(), limits()).unwrap();
    add_scene(&mut builder, 1, None);
    builder.limits.retained_bytes = ByteLimit::new(exact - 1).unwrap();
    assert!(
        matches!(builder.add_group(&group), Err(CookedContentError::Budget {
        kind: "retained-bytes", required, limit,
    }) if required == exact && limit == exact - 1)
    );
    assert_eq!(builder.retained_bytes, before);
    assert!(builder.groups.is_empty());
    assert_eq!(builder.members.len(), 1);
    builder.limits.retained_bytes = ByteLimit::new(exact).unwrap();
    builder.add_group(&group).unwrap();
    assert_eq!(builder.retained_bytes, exact);
}

#[test]
fn finish_admits_original_group_base_and_decoder_promises_at_exact_work_limit() {
    let root = StoreRoot::new();
    let store = root.open();
    let encoded = ActionMapRuntimeCodec::encode_runtime(&input()).unwrap();
    let group = group(
        &store,
        &encoded,
        product(INPUT),
        ActionMapRuntimeCodec::RUNTIME_KIND,
        vec![],
    );
    let mut selected = request();
    selected.configuration.input.action_map = Some(product(INPUT));
    let model = limits();
    let outside = size_of::<PreparedRuntimeContent>()
        + selected.target.triple().len()
        + selected.target.profile().len()
        + size_of::<VerifiedArtifactGroup>()
        + group.retained_bytes();
    let exact = outside + model.runtime.retained_bytes().get() + model.runtime.work_bytes().get();
    for allowance in [exact - 1, exact] {
        let mut builder = CookedContentBuilder::new(
            selected.clone(),
            recipe(),
            CookedContentLimits {
                work_bytes: ByteLimit::new(allowance).unwrap(),
                ..model
            },
        )
        .unwrap();
        add_scene(&mut builder, 1, None);
        builder.add_group(&group).unwrap();
        match builder.finish() {
            Err(CookedContentError::Budget {
                kind: "work-bytes",
                required,
                limit,
            }) if allowance == exact - 1 => {
                assert_eq!(required, exact);
                assert_eq!(limit, exact - 1);
            }
            Ok(content) if allowance == exact => {
                assert_eq!(content.base().catalog().members().len(), 2)
            }
            result => panic!("unexpected work-bound result: {result:?}"),
        }
    }
}

#[test]
fn memory_capture_checks_exact_member_bytes_and_complete_table() {
    let mut corrupt = CookedContentBuilder::new(request(), recipe(), limits()).unwrap();
    add_scene(&mut corrupt, 1, None);
    replace_scene_bytes(&mut corrupt, 1, b"replaced after cooking", false);
    assert!(matches!(
        corrupt.finish(),
        Err(CookedContentError::Runtime(
            RuntimeContentError::MemberDigestMismatch
        ))
    ));

    let mut valid = CookedContentBuilder::new(request(), recipe(), limits()).unwrap();
    add_scene(&mut valid, 1, None);
    let prepared = valid.finish().unwrap();
    let encoded = prepared.base().catalog().to_json_bytes().unwrap();
    assert!(matches!(
        RuntimeContentBase::capture_members(
            encoded,
            vec![],
            prepared.expectation(),
            recipe(),
            limits().runtime,
        ),
        Err(RuntimeContentError::ProductIdentityMismatch)
    ));
}

#[test]
fn production_tileset_codec_and_actual_image_extent_are_checked_before_preparation() {
    for width in [1, 2] {
        let root = StoreRoot::new();
        let store = root.open();
        let image = ImageAsset::from_product(
            auxiliary(11),
            ImageExtent::new(width, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            vec![255; width as usize * 4],
        )
        .unwrap();
        let encoded = ImageRuntimeCodec::encode_runtime(&image).unwrap();
        let group = group(
            &store,
            &encoded,
            auxiliary(11),
            ImageRuntimeCodec::RUNTIME_KIND,
            vec![],
        );
        let tileset = TileSetDescriptor::new(
            auxiliary(12),
            auxiliary(11),
            TileAtlasLayout::grid(Vec2::ONE, 1, 1),
        )
        .unwrap();
        let selected_recipe = recipe()
            .add_runtime_image_codec::<ImageRuntimeCodec>(RuntimeCodecDefinitionId::new(
                "nara.test.cooked.image",
                1,
            ))
            .unwrap()
            .add_runtime_tileset_codec::<TileSetRuntimeCodec>(RuntimeCodecDefinitionId::new(
                "nara.test.cooked.tileset",
                1,
            ))
            .unwrap();
        let mut selected_request = request();
        selected_request.addressable_roots.push(auxiliary(12));
        let mut builder = CookedContentBuilder::new(
            selected_request,
            selected_recipe,
            CookedContentLimits::default(),
        )
        .unwrap();
        add_scene(&mut builder, 1, None);
        builder.add_group(&group).unwrap();
        builder.add_tileset(&tileset).unwrap();
        if width == 1 {
            let content = builder.finish().unwrap();
            assert_eq!(content.base().validate_deep().unwrap().members(), 3);
            assert_eq!(
                content.base().member(auxiliary(12)).unwrap().dependencies(),
                &[RuntimeContentDependency::new(
                    auxiliary(11),
                    RuntimeContentDependencyRole::Required
                ),]
            );
        } else {
            assert!(matches!(
                builder.finish(),
                Err(CookedContentError::Runtime(RuntimeContentError::TileSet(_)))
            ));
        }
    }
}

#[test]
fn escaped_acquisition_keeps_its_original_captured_base_through_a_new_build_and_cache_deletion() {
    let root = StoreRoot::new();
    let store = root.open();
    let encoded = ActionMapRuntimeCodec::encode_runtime(&input()).unwrap();
    let group = group(
        &store,
        &encoded,
        product(INPUT),
        ActionMapRuntimeCodec::RUNTIME_KIND,
        vec![],
    );
    let mut selected = request();
    selected.configuration.input.action_map = Some(product(INPUT));
    let mut first = CookedContentBuilder::new(selected.clone(), recipe(), limits()).unwrap();
    add_scene(&mut first, 1, None);
    first.add_group(&group).unwrap();
    let first = first.finish().unwrap();
    let escaped = first
        .base()
        .acquire_asset::<ActionMapDomain, ActionMapRuntimeCodec>(product(INPUT))
        .unwrap();
    let original_digest = first.base().catalog_digest();
    let original_charge = escaped.retained_bytes();
    drop((first, group, store));
    root.remove();
    assert!(!root.0.exists());

    let mut definition = ActionMapBuilder::new(ActionMapLimits::default());
    definition
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    definition
        .bind(ActionBinding::key(
            ActionId::new("jump").unwrap(),
            KeyCode::Enter,
        ))
        .unwrap();
    let changed = ActionMapAsset::new(product(INPUT), definition.compile().unwrap()).unwrap();
    let mut second = CookedContentBuilder::new(selected, recipe(), limits()).unwrap();
    add_scene(&mut second, 1, None);
    second.add_input(&changed).unwrap();
    let second = second.finish().unwrap();
    assert_ne!(second.base().catalog_digest(), original_digest);
    let newer = second
        .base()
        .acquire_asset::<ActionMapDomain, ActionMapRuntimeCodec>(product(INPUT))
        .unwrap();
    assert_ne!(
        ActionMapRuntimeCodec::encode_runtime(newer.product()).unwrap(),
        ActionMapRuntimeCodec::encode_runtime(escaped.product()).unwrap(),
    );
    drop((newer, second));
    assert_eq!(escaped.retained_bytes(), original_charge);
    assert_eq!(escaped.base().catalog_digest(), original_digest);
    assert_eq!(
        ActionMapRuntimeCodec::encode_runtime(escaped.product()).unwrap(),
        encoded
    );
    let reacquired = escaped
        .base()
        .acquire_asset::<ActionMapDomain, ActionMapRuntimeCodec>(product(INPUT))
        .unwrap();
    assert_eq!(
        ActionMapRuntimeCodec::encode_runtime(reacquired.product()).unwrap(),
        encoded
    );
    assert_eq!(
        escaped.base().acquire_startup_scene().unwrap().product(),
        product(1)
    );
}
