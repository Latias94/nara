use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::{
    asset::{
        AssetDomain, AssetMetaIdentityCandidate, AssetPath, AssetPathScope, AssetProductRef,
        AssetRef, AssetSourceKind, AssetSourcePath, ImportedProductId, RuntimeAssetCodec,
        SourceAssetBuild, SourceAssetImporter, StableAssetId,
    },
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildLimits, ArtifactBuildObservation,
        ArtifactBuildReason, ArtifactBuildStatus, ArtifactBuildStore, ArtifactContentDigest,
        ArtifactSourceRequest, VerifiedArtifactGroup, build_image_import, build_source,
    },
    core::FormatVersion,
    fs::{
        CapabilityRights, DirectoryCapability, FileCapability, HostCapabilityOptions, RelativePath,
        TrustMode,
    },
    project_host::{
        HeadlessSession, HeadlessSessionClose, ProjectContentBudgetKind,
        ProjectContentBudgetSnapshot, ProjectContentLoader, ProjectDomainAssets,
        ProjectSettingsCandidate, RuntimePlan, ingest_project_manifest, resolve_product_recipe,
    },
    project_workflow::cooked_content::{
        CookedContentBuilder, CookedContentLimits, CookedContentRequest, PreparedRuntimeContent,
    },
    runtime_content::{
        RuntimeContentBase, RuntimeContentCatalog, RuntimeContentCatalogDefinition,
        RuntimeContentCatalogLimits, RuntimeContentError, RuntimeContentExpectation,
        RuntimeContentLimits, RuntimeContentMember, RuntimeContentTarget,
    },
};
use serde_json::{Value, json};
use two_level_game::{
    GameSnapshot, SoundPalette,
    levels::LevelId,
    sound_bank::{BANK_PATH, SoundBankCodec, SoundBankDomain, SoundCue, bank_product},
};

#[path = "sound_bank_content/wav.rs"]
mod wav;

type Bank = <SoundBankDomain as AssetDomain>::Product;

const BANK_ID: &str = "6c7d4d42-cb57-4a47-9f16-741293f5aab2";
const FOREIGN_ID: &str = "eb2f8fb6-d913-47e2-b458-0f4a5c1383ed";
const PICKUP: &str = "audio/pickup.wav";
const TRAVEL: &str = "audio/travel.wav";

struct Fixture {
    root: PathBuf,
    project: PathBuf,
    session: Option<HeadlessSession>,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "nara-sound-bank-content-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let project = root.join("project");
        fs::create_dir(&project).unwrap();
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"));
        for name in ["assets", "scenes", "schema", "prefabs"] {
            copy_tree(&sample.join(name), &project.join(name));
        }
        fs::copy(sample.join("nara.toml"), project.join("nara.toml")).unwrap();
        fs::create_dir(root.join("cache")).unwrap();
        Self {
            root,
            project,
            session: None,
        }
    }

    fn root_capability(&self) -> DirectoryCapability {
        two_level_game::open_explicit_project_root(&self.project).unwrap()
    }

    fn inputs(&self, root: &DirectoryCapability) -> (ProjectSettingsCandidate, RuntimePlan) {
        let manifest = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, two_level_game::cooked_recipe()).unwrap();
        (candidate, plan)
    }

    fn store(&self) -> ArtifactBuildStore {
        let path = self.root.join("cache");
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(path)
                .unwrap()
        };
        #[cfg(not(windows))]
        let file = fs::File::open(path).unwrap();
        let root = DirectoryCapability::from_host_handle(
            file,
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap();
        ArtifactBuildStore::open(root, ArtifactBuildLimits::default()).unwrap()
    }

    fn request(&self, path: &str, definition: ArtifactBuildDefinition) -> ArtifactSourceRequest {
        ArtifactSourceRequest::new(
            self.root_capability()
                .open_directory(&RelativePath::new("assets").unwrap())
                .unwrap(),
            AssetPathScope::under(AssetSourcePath::new("assets").unwrap()),
            AssetPath::new(path).unwrap(),
            definition,
        )
    }

    fn samples(&self) -> [Vec<f32>; 2] {
        [PICKUP, TRAVEL]
            .map(|path| wav::decode(&fs::read(self.project.join("assets").join(path)).unwrap()))
    }

    fn close(&mut self) -> bool {
        let Some(session) = self.session.as_mut() else {
            return true;
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match session.close() {
                HeadlessSessionClose::Complete => {
                    assert!(session.is_closed() && session.state().is_none());
                    assert!(session.observe(|_| ()).is_err());
                    self.session.take();
                    return true;
                }
                HeadlessSessionClose::Failed => {
                    eprintln!(
                        "original SoundBank Host retired with failure: {:?}",
                        session.diagnostics()
                    );
                    self.session.take();
                    return false;
                }
                HeadlessSessionClose::Pending if Instant::now() < deadline => {
                    std::thread::yield_now()
                }
                HeadlessSessionClose::Pending => return false,
            }
        }
    }

    fn retire_source(&self) {
        assert!(
            self.session.is_none(),
            "source cleanup follows original Host retirement"
        );
        let root = self.root.canonicalize().unwrap();
        let source = self.project.canonicalize().unwrap();
        let hidden = root.join("retired-copied-source");
        assert!(source == self.project && source.starts_with(&root) && !hidden.exists());
        assert!(
            !Path::new(env!("CARGO_MANIFEST_DIR"))
                .canonicalize()
                .unwrap()
                .starts_with(&source)
        );
        fs::rename(source, hidden).unwrap();
        assert!(!self.project.exists());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if !self.close() && self.session.is_some() {
            eprintln!(
                "retaining unresolved SoundBank Host and copied source at {}",
                self.root.display()
            );
            std::mem::forget(self.session.take());
            return;
        }
        if self.root.exists() {
            let parent = self.root.parent().unwrap();
            remove_owned_tree(parent, &self.root);
        }
    }
}

fn copy_tree(source: &Path, destination: &Path) {
    assert!(
        !fs::symlink_metadata(source)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::create_dir(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(!kind.is_symlink());
        if kind.is_dir() {
            copy_tree(&entry.path(), &destination.join(entry.file_name()));
        } else {
            assert!(kind.is_file());
            fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

fn remove_owned_tree(parent: &Path, path: &Path) {
    let parent = parent.canonicalize().unwrap();
    let resolved = path.canonicalize().unwrap();
    assert!(resolved == path && resolved != parent && resolved.starts_with(&parent));
    assert!(
        !Path::new(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap()
            .starts_with(&resolved)
    );
    let mut pending = vec![resolved.clone()];
    let mut visited = 0;
    while let Some(path) = pending.pop() {
        visited += 1;
        assert!(visited <= 16_384 && path.starts_with(&resolved));
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert!(!metadata.file_type().is_symlink());
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            assert_eq!(metadata.file_attributes() & 0x400, 0);
        }
        if metadata.is_dir() {
            pending.extend(
                fs::read_dir(path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else {
            assert!(metadata.is_file());
        }
    }
    fs::remove_dir_all(resolved).unwrap();
}

fn implementation() -> ArtifactContentDigest {
    static DIGEST: OnceLock<ArtifactContentDigest> = OnceLock::new();
    *DIGEST.get_or_init(|| {
        let file = fs::File::open(std::env::current_exe().unwrap()).unwrap();
        let length = file.metadata().unwrap().len();
        FileCapability::from_host_handle(file, TrustMode::TrustedLocal, 0)
            .unwrap()
            .digest(length)
            .map(ArtifactContentDigest::from_content)
            .unwrap()
    })
}

fn bank_definition() -> ArtifactBuildDefinition {
    ArtifactBuildDefinition {
        id: SoundBankDomain::BUILD_ID.to_owned(),
        revision: FormatVersion::new(SoundBankDomain::BUILD_REVISION).unwrap(),
        implementation: implementation(),
    }
}

fn bank(assets: &ProjectDomainAssets) -> &Bank {
    let collection = assets
        .collection::<SoundBankDomain>()
        .expect("ordinary Host installed the SoundBank collection");
    assert_eq!(collection.iter().len(), 1);
    let selected = collection.get(&AssetRef::Product(bank_product())).unwrap();
    assert_eq!(selected.product(), bank_product());
    selected
}

fn exact_samples(bank: &Bank, expected: &[Vec<f32>; 2]) {
    assert_eq!(bank.samples(SoundCue::Pickup), expected[0]);
    assert_eq!(bank.samples(SoundCue::Travel), expected[1]);
}

fn decode_group(group: &VerifiedArtifactGroup) -> Bank {
    assert_eq!(group.members().len(), 1);
    let member = &group.members()[0];
    assert_eq!(member.products, [bank_product()]);
    assert_eq!(member.format.as_str(), SoundBankCodec::RUNTIME_KIND);
    assert_eq!(member.format_version.get(), SoundBankCodec::RUNTIME_VERSION);
    assert!(member.dependencies.is_empty());
    let bytes = group.member_bytes(&member.name).unwrap();
    assert_eq!(member.digest, ArtifactContentDigest::of_bytes(bytes));
    SoundBankCodec::decode_runtime(bytes).unwrap()
}

pub fn source_and_build() {
    let fixture = Fixture::new();
    assert_eq!(bank_product().source().to_string(), BANK_ID);
    assert_eq!(SoundBankDomain::SOURCE_KIND, "two-level-game.sound-bank");
    let metadata = fs::read(
        fixture
            .project
            .join("assets")
            .join(format!("{BANK_PATH}.meta")),
    )
    .unwrap();
    let metadata_json: Value = serde_json::from_slice(&metadata).unwrap();
    assert_eq!(metadata_json["payload"]["settings"], json!({}));
    let identity = AssetMetaIdentityCandidate::decode_json_bytes(&metadata).unwrap();
    assert_eq!(identity.identity().stable_id(), bank_product().source());
    assert_eq!(
        identity.identity().source_kind(),
        &AssetSourceKind::Other(SoundBankDomain::SOURCE_KIND.to_owned())
    );
    let expected = fixture.samples();
    assert_eq!([expected[0].len(), expected[1].len()], [5760, 11520]);
    let root = fixture.root_capability();
    let (candidate, plan) = fixture.inputs(&root);
    let loader = ProjectContentLoader::new(root).unwrap();
    let original = loader.load(&candidate, &plan).unwrap();
    exact_samples(bank(original.domain_assets()), &expected);
    let collection = original
        .domain_assets()
        .collection::<SoundBankDomain>()
        .unwrap();
    assert!(std::ptr::eq(
        bank(original.domain_assets()),
        collection.get(&AssetRef::path(BANK_PATH).unwrap()).unwrap()
    ));
    assert!(std::ptr::eq(
        bank(original.domain_assets()),
        collection
            .get(&AssetRef::StableId(bank_product().source()))
            .unwrap()
    ));
    assert_eq!(collection.source_records().len(), 1);
    let closure = original.content_closure();
    for path in [BANK_PATH, PICKUP, TRAVEL].map(|path| format!("assets/{path}")) {
        let member = closure
            .members()
            .iter()
            .find(|member| member.path().as_str() == path)
            .unwrap();
        assert_eq!(
            member.digest(),
            nara::fs::ContentDigest::of_bytes(&fs::read(fixture.project.join(&path)).unwrap())
        );
    }
    drop(closure);
    let store = fixture.store();
    let request = fixture.request(BANK_PATH, bank_definition());
    let first = build_source::<SoundBankDomain>(&store, &request, |_| Ok(())).unwrap();
    assert_eq!(first.status(), ArtifactBuildStatus::Built);
    exact_samples(&decode_group(first.group()), &expected);
    for path in [PICKUP, TRAVEL].map(|path| format!("assets/{path}")) {
        assert!(first.group().observations().iter().any(|observation| matches!(
            observation, ArtifactBuildObservation::Raw { path: observed, digest: Some(digest) }
                if observed.as_str() == path && *digest == ArtifactContentDigest::of_bytes(&fs::read(fixture.project.join(&path)).unwrap())
        )), "missing the actual raw WAV observation: {path}");
    }
    let warm = build_source::<SoundBankDomain>(&store, &request, |_| Ok(())).unwrap();
    assert_eq!(warm.status(), ArtifactBuildStatus::Hit);
    assert_eq!(warm.group().digest(), first.group().digest());
    let path = fixture.project.join("assets").join(TRAVEL);
    let mut bytes = fs::read(&path).unwrap();
    let sample = wav::pcm16_data(&bytes).start + 300;
    let changed_value = i16::from_le_bytes(bytes[sample..sample + 2].try_into().unwrap()) ^ 0x0100;
    bytes[sample..sample + 2].copy_from_slice(&changed_value.to_le_bytes());
    fs::write(path, bytes).unwrap();
    let changed_samples = fixture.samples();
    assert_eq!(changed_samples[0], expected[0]);
    assert_ne!(changed_samples[1], expected[1]);
    let changed = loader.load(&candidate, &plan).unwrap();
    assert_ne!(changed.revision(), original.revision());
    exact_samples(bank(changed.domain_assets()), &changed_samples);
    exact_samples(bank(original.domain_assets()), &expected);
    let rebuilt = build_source::<SoundBankDomain>(&store, &request, |_| Ok(())).unwrap();
    assert_eq!(rebuilt.status(), ArtifactBuildStatus::Built);
    assert_eq!(
        rebuilt.reason(),
        &ArtifactBuildReason::RawInputChanged(
            AssetSourcePath::new(format!("assets/{TRAVEL}")).unwrap()
        )
    );
    assert_ne!(rebuilt.group().digest(), first.group().digest());
    assert_ne!(
        rebuilt.group().recipe_digest(),
        first.group().recipe_digest()
    );
    exact_samples(&decode_group(rebuilt.group()), &changed_samples);
    exact_samples(&decode_group(first.group()), &expected);
}

fn prepared(fixture: &Fixture) -> PreparedRuntimeContent {
    let root = fixture.root_capability();
    let (candidate, plan) = fixture.inputs(&root);
    let configuration = candidate
        .settings()
        .runtime_configuration()
        .unwrap()
        .to_configuration();
    let mut builder = CookedContentBuilder::new(
        CookedContentRequest {
            target: RuntimeContentTarget::new(
                if cfg!(windows) {
                    "x86_64-pc-windows-msvc"
                } else {
                    "x86_64-unknown-linux-gnu"
                },
                "sound-bank-independent-acceptance",
            )
            .unwrap(),
            configuration,
            startup_scene: LevelId::STARTUP.scene_product(),
            scenes: LevelId::ALL
                .into_iter()
                .map(LevelId::scene_product)
                .collect(),
            addressable_roots: Vec::new(),
        },
        two_level_game::cooked_recipe(),
        CookedContentLimits::default(),
    )
    .unwrap();
    let store = fixture.store();
    let group = build_source::<SoundBankDomain>(
        &store,
        &fixture.request(BANK_PATH, bank_definition()),
        |_| Ok(()),
    )
    .unwrap()
    .into_group();
    builder.add_group(&group).unwrap();
    for path in ["textures/meadow.png", "textures/cavern.png"] {
        let request = fixture.request(
            path,
            ArtifactBuildDefinition {
                id: "nara.image.import".to_owned(),
                revision: FormatVersion::ONE,
                implementation: implementation(),
            },
        );
        builder
            .add_group(
                &build_image_import(&store, &request, |_| Ok(()))
                    .unwrap()
                    .into_group(),
            )
            .unwrap();
    }
    let loader = ProjectContentLoader::new(root).unwrap();
    for level in LevelId::ALL {
        let snapshot = loader
            .load_scene(&candidate, &plan, &level.scene_path())
            .unwrap();
        exact_samples(bank(snapshot.domain_assets()), &fixture.samples());
        let compilation = plan
            .schema_validation()
            .bindings()
            .try_compile_scene_with_loader(
                snapshot.scene(),
                Default::default(),
                Some(snapshot.asset_resolution()),
                |_, _| Ok::<_, std::convert::Infallible>(None),
            )
            .unwrap();
        assert!(
            !compilation.diagnostics.has_errors(),
            "{:?}",
            compilation.diagnostics
        );
        let compiled = compilation
            .candidate
            .unwrap()
            .bind_source(snapshot.selected_scene_source_key().clone());
        builder
            .add_scene(
                level.scene_product(),
                &compiled,
                snapshot.asset_resolution(),
            )
            .unwrap();
    }
    builder.finish().unwrap()
}

fn foreign_membership(fixture: &Fixture, original: &RuntimeContentBase, foreign: AssetProductRef) {
    let catalog = original.catalog();
    let members = catalog
        .members()
        .iter()
        .map(|member| {
            if member.products() == [bank_product()] {
                RuntimeContentMember::new(
                    member.path(),
                    member.digest(),
                    member.kind().clone(),
                    vec![bank_product(), foreign],
                    member.dependencies().to_vec(),
                )
                .unwrap()
            } else {
                member.clone()
            }
        })
        .collect();
    let changed = RuntimeContentCatalog::new(
        RuntimeContentCatalogDefinition {
            target: catalog.target().clone(),
            configuration: catalog.configuration().clone(),
            plugin_plan_fingerprint: catalog.plugin_plan_fingerprint(),
            schema_fingerprint: catalog.schema_fingerprint(),
            runtime_codecs_fingerprint: catalog.runtime_codecs_fingerprint(),
            startup_scene: catalog.startup_scene(),
            scenes: catalog.scenes().to_vec(),
            addressable_roots: catalog.addressable_roots().to_vec(),
            members,
        },
        RuntimeContentCatalogLimits::default(),
    )
    .unwrap();
    let changed_catalog = changed.to_json_bytes().unwrap();
    let mut pack = Vec::new();
    original.write_pack(&mut pack).unwrap();
    let original_length = u64::from_le_bytes(pack[12..20].try_into().unwrap()) as usize;
    let mut changed_pack = b"NARAPACK".to_vec();
    changed_pack.extend(1_u32.to_le_bytes());
    changed_pack.extend((changed_catalog.len() as u64).to_le_bytes());
    changed_pack.extend_from_slice(&changed_catalog);
    changed_pack.extend_from_slice(&pack[20 + original_length..]);
    let path = fixture.root.join("foreign-membership.pack");
    fs::write(&path, changed_pack).unwrap();
    let file =
        FileCapability::from_host_handle(fs::File::open(path).unwrap(), TrustMode::TrustedLocal, 0)
            .unwrap();
    let expected = RuntimeContentExpectation::new(
        nara::fs::ContentDigest::of_bytes(&changed_catalog),
        changed.target().clone(),
    );
    let relabelled = RuntimeContentBase::open_pack(
        file,
        &expected,
        two_level_game::cooked_recipe(),
        RuntimeContentLimits::default(),
    )
    .unwrap();
    for reference in [bank_product(), foreign] {
        assert!(matches!(
            relabelled.acquire_asset::<SoundBankDomain, SoundBankCodec>(reference),
            Err(RuntimeContentError::ProductIdentityMismatch)
        ));
    }
}

pub fn processed_and_membership() {
    let fixture = Fixture::new();
    let expected = fixture.samples();
    let prepared = prepared(&fixture);
    assert_eq!(prepared.base().catalog().members().len(), 5);
    let acquired = prepared
        .base()
        .acquire_asset::<SoundBankDomain, SoundBankCodec>(bank_product())
        .unwrap();
    exact_samples(acquired.product(), &expected);
    let foreign = AssetProductRef::new(
        StableAssetId::parse_str(FOREIGN_ID).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap();
    assert!(SoundBankCodec::contains_runtime_product(
        acquired.product(),
        bank_product()
    ));
    assert!(!SoundBankCodec::contains_runtime_product(
        acquired.product(),
        foreign
    ));
    assert!(
        prepared
            .base()
            .acquire_asset::<SoundBankDomain, SoundBankCodec>(foreign)
            .is_err()
    );
    foreign_membership(&fixture, prepared.base(), foreign);
    let absent = AssetProductRef::new(
        bank_product().source(),
        ImportedProductId::parse_subproduct("b8735336-e1b3-414e-a465-9b26d0904213").unwrap(),
    )
    .unwrap();
    assert!(!SoundBankCodec::contains_runtime_product(
        acquired.product(),
        absent
    ));
    assert!(
        prepared
            .base()
            .acquire_asset::<SoundBankDomain, SoundBankCodec>(absent)
            .is_err()
    );
    let mut bytes = SoundBankCodec::encode_runtime(acquired.product()).unwrap();
    exact_samples(&SoundBankCodec::decode_runtime(&bytes).unwrap(), &expected);
    assert_eq!(&bytes[..8], b"NARASBNK");
    bytes[12..28].copy_from_slice(foreign.source().as_uuid().as_bytes());
    assert!(SoundBankCodec::decode_runtime(&bytes).is_err());
    fixture.retire_source();
    exact_samples(acquired.product(), &expected);
    let mut fixture = fixture;
    fixture.session = Some(HeadlessSession::from_runtime_content(prepared.into_base()));
    fixture.session.as_mut().unwrap().start().unwrap();
    fixture
        .session
        .as_mut()
        .unwrap()
        .frame(Duration::ZERO)
        .unwrap();
    let retained = fixture
        .session
        .as_ref()
        .unwrap()
        .observe(|world| {
            assert!(world.resource::<GameSnapshot>().initialized);
            let assets = world.resource::<ProjectDomainAssets>();
            assert_eq!(
                assets
                    .collection::<SoundBankDomain>()
                    .unwrap()
                    .source_records()
                    .len(),
                0
            );
            assert!(
                assets
                    .collection::<SoundBankDomain>()
                    .unwrap()
                    .get(&AssetRef::path(BANK_PATH).unwrap())
                    .is_none()
            );
            exact_samples(bank(assets), &expected);
            assets.clone()
        })
        .unwrap();
    assert!(fixture.close());
    exact_samples(bank(&retained), &expected);
}

pub fn world_retirement() {
    let mut fixture = Fixture::new();
    let expected = fixture.samples();
    fixture.session = Some(HeadlessSession::new(
        fixture.root_capability(),
        two_level_game::product_recipe(),
    ));
    fixture.session.as_mut().unwrap().start().unwrap();
    fixture
        .session
        .as_mut()
        .unwrap()
        .frame(Duration::ZERO)
        .unwrap();
    let retained = fixture
        .session
        .as_ref()
        .unwrap()
        .observe(|world| {
            assert!(world.resource::<GameSnapshot>().initialized);
            let palettes = world
                .iter_entities()
                .filter_map(|entity| entity.get::<SoundPalette>())
                .collect::<Vec<_>>();
            assert_eq!(palettes.len(), 1);
            let assets = world.resource::<ProjectDomainAssets>();
            let selected = assets
                .collection::<SoundBankDomain>()
                .unwrap()
                .get(&palettes[0].bank)
                .unwrap();
            exact_samples(selected, &expected);
            assets.clone()
        })
        .unwrap();
    assert!(fixture.close());
    fixture.retire_source();
    exact_samples(bank(&retained), &expected);
    let last = retained.clone();
    drop(retained);
    exact_samples(bank(&last), &expected);
}

fn active_budget(actual: ProjectContentBudgetSnapshot, expected: ProjectContentBudgetSnapshot) {
    assert_eq!(actual.active_generations(), expected.active_generations());
    assert_eq!(actual.active_reservations(), expected.active_reservations());
    for kind in ProjectContentBudgetKind::ALL {
        assert_eq!(actual.active(*kind), expected.active(*kind), "{kind:?}");
    }
}

pub fn budget_retirement() {
    let fixture = Fixture::new();
    let expected = fixture.samples();
    let root = fixture.root_capability();
    let (candidate, plan) = fixture.inputs(&root);
    let loader = ProjectContentLoader::new(root).unwrap();
    let host = loader.budget_host();
    let empty = host.snapshot();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let retained = snapshot.domain_assets().clone();
    exact_samples(bank(&retained), &expected);
    drop(snapshot);
    drop(loader);
    drop((candidate, plan));
    fixture.retire_source();
    let charged = host.snapshot();
    assert_eq!(charged.active_generations(), 0);
    assert!(charged.active_reservations() > 0);
    assert!(
        charged.active(ProjectContentBudgetKind::RetainedBytes)
            >= (expected[0].len() + expected[1].len()) * std::mem::size_of::<f32>()
    );
    assert_eq!(charged.active(ProjectContentBudgetKind::WorkBytes), 0);
    let last = retained.clone();
    active_budget(host.snapshot(), charged);
    drop(retained);
    active_budget(host.snapshot(), charged);
    exact_samples(bank(&last), &expected);
    drop(last);
    active_budget(host.snapshot(), empty);
}
