use std::{collections::BTreeSet, fs, ops::Range, path::Path};

use nara::{
    fs::ContentDigest,
    project::{ProductCapability, ProductCapabilitySet, ProjectFixedCatchUpPolicy, RuntimePreset},
    project_tooling::host_fs,
    project_workflow::cargo::CargoBuildSelection,
    runtime_content::{
        RuntimeContentBase, RuntimeContentDependencyRole, RuntimeContentError,
        RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentMemberKind,
        RuntimeContentTarget,
    },
    scene::SourceDocumentOrigin,
};
use serde_json::{Value, json};
use two_level_game::{delivery::PlayerSelection, levels::LevelId, markers, side_room};

use crate::cooked_delivery::{
    inspect_pack,
    native::{assert_payload, read_bounded},
};

use super::CONTENT_PROFILE;

const PACK: &str = "content.pack";
const MANIFEST: &str = "package.json";
const BOOTSTRAP: &str = "player-bootstrap.rs";
const MAX_PACK_BYTES: usize = 32 * 1024 * 1024;
const WRONG_PROFILE: &str = "two-level-server-shippinx";

pub(super) struct Package {
    executable: String,
    executable_digest: ContentDigest,
    manifest: Vec<u8>,
    pack: Vec<u8>,
    catalog_bytes: Range<usize>,
    expectation: RuntimeContentExpectation,
    player: PlayerSelection,
}

pub(super) fn inspect(
    directory: &Path,
    executable: &str,
    selection: &CargoBuildSelection,
    lock: &[u8],
) -> Package {
    assert_payload(directory, executable);
    let executable_digest = digest_executable(&directory.join(executable));
    let manifest = read_bounded(&directory.join(MANIFEST), 1024 * 1024);
    let document: Value = serde_json::from_slice(&manifest).unwrap();
    let payload = &document["payload"];
    let pack = read_bounded(&directory.join(PACK), MAX_PACK_BYTES);
    let catalog_bytes = inspect_pack(&pack).unwrap().catalog_bytes;
    let catalog_digest = ContentDigest::of_bytes(&pack[catalog_bytes.clone()]);
    let target = RuntimeContentTarget::new(selection.target_triple(), CONTENT_PROFILE).unwrap();
    let expectation = RuntimeContentExpectation::new(catalog_digest, target);

    assert_eq!(payload["executable_name"], executable);
    assert_eq!(payload["executable_digest"], digest_json(executable_digest));
    assert_eq!(payload["pack_file"], PACK);
    assert_eq!(
        payload["pack_digest"],
        digest_json(ContentDigest::of_bytes(&pack))
    );
    assert_eq!(payload["catalog_digest"], digest_json(catalog_digest));
    assert_eq!(payload["content_target"], json!(expectation.target()));
    let build = &payload["build"];
    assert_eq!(
        build["selection"],
        json!({
            "package": selection.package(),
            "bin": selection.target(),
            "cargo_profile": selection.profile(),
            "target_triple": selection.target_triple(),
            "default_features": selection.default_features(),
            "requested_features": selection.features(),
        })
    );
    assert_eq!(build["resolved_features"], json!(selection.features()));
    assert_eq!(payload["generated_input_name"], BOOTSTRAP);
    let generated = &build["generated_rust_input"];
    assert_eq!(generated["path"], BOOTSTRAP);
    assert_eq!(
        generated["digest"][0],
        payload["generated_input_digest"]["length"]
    );
    assert_eq!(
        generated["digest"][1],
        payload["generated_input_digest"]["hash"]
    );
    assert!(
        generated["digest"][0]
            .as_u64()
            .is_some_and(|length| length > 0)
    );
    let inputs = build["declared_inputs"].as_array().unwrap();
    for required in [
        "Cargo.toml",
        "build.rs",
        "src/bin/server.rs",
        "src/bin/server/clock.rs",
        "src/bin/server/control.rs",
        "src/bin/server/output.rs",
        "src/bin/server/service.rs",
        "src/delivery.rs",
    ] {
        assert!(
            inputs.iter().any(|input| input["path"] == required),
            "the captured server build omitted {required}"
        );
    }
    let lock_input = inputs
        .iter()
        .find(|input| input["path"] == "Cargo.lock")
        .unwrap();
    let lock_digest = ContentDigest::of_bytes(lock);
    assert_eq!(
        lock_input["digest"],
        json!([lock_digest.length(), lock_digest.as_bytes()])
    );
    assert!(inputs.iter().all(|input| input["path"] != BOOTSTRAP));

    let player = PlayerSelection::from_content_profile(CONTENT_PROFILE).unwrap();
    let base = open(&directory.join(PACK), &expectation, player).unwrap();
    assert_scene_closure(&base);
    Package {
        executable: executable.to_owned(),
        executable_digest,
        manifest,
        pack,
        catalog_bytes,
        expectation,
        player,
    }
}

fn assert_scene_closure(base: &RuntimeContentBase) {
    use RuntimeContentDependencyRole::{Reference, Required};

    let catalog = base.catalog();
    assert_eq!(
        catalog.configuration().runtime.preset,
        RuntimePreset::Server
    );
    assert_eq!(
        catalog.configuration().runtime.catch_up_policy,
        ProjectFixedCatchUpPolicy::PreserveDebt
    );
    assert!(!catalog.configuration().window.enabled);
    assert!(catalog.configuration().input.action_map.is_none());
    assert!(base.acquire_startup_input().unwrap().is_none());
    assert_eq!(
        base.plan().normalized_capabilities(),
        ProductCapabilitySet::singleton(ProductCapability::RuntimeCore)
    );
    assert_eq!(catalog.startup_scene(), LevelId::Meadow.scene_product());
    let mut roots = LevelId::ALL.map(LevelId::scene_product);
    roots.sort_unstable();
    assert_eq!(catalog.scenes(), roots);
    assert!(catalog.addressable_roots().is_empty());
    assert_eq!(catalog.members().len(), 5);

    let marker = markers::marker_product();
    let side_room = side_room::scene_product();
    let lookout = side_room::lookout_scene_product();
    for (product, authored_path, dependencies) in [
        (
            LevelId::Meadow.scene_product(),
            "meadow.scene.json",
            BTreeSet::from([
                (marker, Required),
                (side_room, Reference),
                (lookout, Reference),
            ]),
        ),
        (
            LevelId::Cavern.scene_product(),
            "cavern.scene.json",
            BTreeSet::from([(marker, Required)]),
        ),
        (marker, markers::SCENE_PATH, BTreeSet::new()),
        (side_room, side_room::SCENE_PATH, BTreeSet::new()),
        (lookout, side_room::LOOKOUT_SCENE_PATH, BTreeSet::new()),
    ] {
        let member = catalog.member_for_product(product).expect(authored_path);
        assert_eq!(member.products(), [product], "{authored_path}");
        let RuntimeContentMemberKind::Scene { source } = member.kind() else {
            panic!("{authored_path} must be a cooked Scene");
        };
        let SourceDocumentOrigin::ProjectLocation { path, .. } = source.origin() else {
            panic!("{authored_path} must retain its original ProjectLocation");
        };
        assert_eq!(path.as_str(), format!("scenes/{authored_path}"));
        assert_eq!(
            member.dependencies().len(),
            dependencies.len(),
            "{authored_path}"
        );
        assert_eq!(
            member
                .dependencies()
                .iter()
                .map(|edge| (edge.product(), edge.role()))
                .collect::<BTreeSet<_>>(),
            dependencies,
            "{authored_path}"
        );
    }
    let report = base.validate_deep().unwrap();
    assert_eq!(report.members(), 5);
    assert_eq!(report.products(), 5);
    println!(
        "TWO_LEVEL_SERVER_PACK scenes=5 roots=2 immediate_scene_edges=2 deferred_scene_edges=2 non_scene_members=0"
    );
}

impl Package {
    pub(super) fn assert_unchanged(&self, installation: &Path) {
        assert_payload(installation, &self.executable);
        assert_eq!(
            read_bounded(&installation.join(MANIFEST), 1024 * 1024),
            self.manifest
        );
        assert_eq!(
            read_bounded(&installation.join(PACK), MAX_PACK_BYTES),
            self.pack
        );
        assert_eq!(
            digest_executable(&installation.join(&self.executable)),
            self.executable_digest
        );
    }

    pub(super) fn install_mismatched(&self, installation: &Path, wrong: &Path) {
        fs::copy(
            installation.join(&self.executable),
            wrong.join(&self.executable),
        )
        .unwrap();
        fs::write(wrong.join(MANIFEST), &self.manifest).unwrap();
        assert_eq!(
            digest_executable(&wrong.join(&self.executable)),
            self.executable_digest
        );

        assert_eq!(CONTENT_PROFILE.len(), WRONG_PROFILE.len());
        let mut bytes = self.pack.clone();
        let catalog = &mut bytes[self.catalog_bytes.clone()];
        let mut profiles = catalog.windows(CONTENT_PROFILE.len()).enumerate();
        let index = profiles
            .find(|(_, value)| *value == CONTENT_PROFILE.as_bytes())
            .expect("the original server content target")
            .0;
        assert!(!profiles.any(|(_, value)| value == CONTENT_PROFILE.as_bytes()));
        catalog[index..index + CONTENT_PROFILE.len()].copy_from_slice(WRONG_PROFILE.as_bytes());
        let unexpected = RuntimeContentExpectation::new(
            ContentDigest::of_bytes(catalog),
            RuntimeContentTarget::new(self.expectation.target().triple(), WRONG_PROFILE).unwrap(),
        );
        let path = wrong.join(PACK);
        fs::write(&path, bytes).unwrap();
        let base = open(&path, &unexpected, self.player).expect("the alternate pack is valid");
        assert_eq!(base.validate_deep().unwrap().members(), 5);
        assert!(matches!(
            open(&path, &self.expectation, self.player),
            Err(RuntimeContentError::CatalogDigestMismatch)
        ));
    }
}

fn open(
    pack: &Path,
    expectation: &RuntimeContentExpectation,
    player: PlayerSelection,
) -> Result<RuntimeContentBase, RuntimeContentError> {
    RuntimeContentBase::open_pack(
        host_fs::readable_file(pack).unwrap(),
        expectation,
        player.recipe().unwrap(),
        RuntimeContentLimits::default(),
    )
}

fn digest_executable(path: &Path) -> ContentDigest {
    host_fs::readable_file(path)
        .unwrap()
        .digest(512 * 1024 * 1024)
        .unwrap()
}

fn digest_json(digest: ContentDigest) -> Value {
    json!({"length": digest.length(), "hash": digest.as_bytes()})
}
