use std::{collections::BTreeSet, fs, ops::Range, path::Path};

use nara::{
    fs::ContentDigest,
    project_tooling::host_fs,
    project_workflow::cargo::CargoBuildSelection,
    runtime_content::{
        RuntimeContentBase, RuntimeContentDependencyRole, RuntimeContentError,
        RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentMemberKind,
        RuntimeContentTarget,
    },
    scene::{
        SourceDocumentOrigin,
        runtime_configuration::{
            ProductCapability, ProductCapabilitySet, ProjectFixedCatchUpPolicy, RuntimePreset,
        },
    },
};
use serde_json::{Value, json};
use two_level_game::{delivery::PlayerSelection, levels::LevelId, markers, side_room};

use crate::cooked_delivery::{
    inspect_pack,
    native::{assert_payload, read_bounded},
};

use super::CONTENT_PROFILE;

#[cfg(feature = "local-player")]
#[path = "../../../src/bin/server/client_services.rs"]
mod client_services;

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
    #[cfg(feature = "local-player")]
    assert_client_services_absent(base);
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

#[cfg(feature = "local-player")]
fn assert_client_services_absent(base: RuntimeContentBase) {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
        time::{Duration, Instant},
    };

    use nara::project_host::{HeadlessSession, HeadlessSessionClose};
    use two_level_game::{
        GamePhase, GameSnapshot, SideRoomPhase,
        command::{
            CommandSchedule, CommandSequence, CommandSource, CommandTick, SERVER_COMMAND_SOURCE,
            ScheduledCommand, SemanticCommand, ServerCommand,
        },
        levels::LevelKey,
    };

    let timestep = base.plan().settings().runtime().fixed_time().timestep();
    let mut session = HeadlessSession::from_runtime_content(base);
    let exercise = catch_unwind(AssertUnwindSafe(|| {
        session
            .start()
            .expect("Server pack starts with local-player types compiled");
        let frame = |session: &mut HeadlessSession, delta| {
            assert_eq!(session.frame(delta).unwrap(), None);
            session
                .observe(|world| {
                    let snapshot = world.resource::<GameSnapshot>();
                    assert_eq!(
                        client_services::verify_absent(world),
                        Ok(()),
                        "{snapshot:?}"
                    );
                    assert!(snapshot.error.is_none(), "{snapshot:?}");
                    snapshot.clone()
                })
                .unwrap()
        };
        let startup = frame(&mut session, Duration::ZERO);
        assert!(startup.initialized && startup.current_instance.is_some());
        assert_eq!(startup.tick, 0);
        assert_eq!(startup.phase, GamePhase::Running);
        assert_eq!(startup.level, Some(LevelKey::main(LevelId::Meadow)));
        assert_eq!(startup.side_room, SideRoomPhase::Closed);
        assert_eq!(startup.lookout, SideRoomPhase::Closed);

        let source = CommandSource::external(SERVER_COMMAND_SOURCE).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut sequence = 0;
        let mut submit_and_wait =
            |command: ServerCommand,
             previous: &GameSnapshot,
             reached: fn(&GameSnapshot) -> bool| {
                sequence += 1;
                let submission = ScheduledCommand::new(
                    CommandTick::new(previous.tick + 1).unwrap(),
                    source.clone(),
                    CommandSequence::new(sequence).unwrap(),
                    command.into_intent().unwrap(),
                );
                let key = submission.key().clone();
                assert_eq!(submission.submit_headless(&mut session).unwrap(), key);
                loop {
                    let snapshot = frame(&mut session, timestep);
                    if reached(&snapshot) {
                        return snapshot;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "Server did not settle command {key:?}: {snapshot:?}"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
            };
        let side_room = submit_and_wait(ServerCommand::SideRoom, &startup, |snapshot| {
            snapshot.side_room == SideRoomPhase::Open
        });
        assert_eq!(side_room.current_instance, startup.current_instance);
        let both_rooms = submit_and_wait(ServerCommand::Lookout, &side_room, |snapshot| {
            snapshot.lookout == SideRoomPhase::Open
        });
        assert_eq!(both_rooms.current_instance, startup.current_instance);
        assert_eq!(both_rooms.side_room, SideRoomPhase::Open);
        let travelled = submit_and_wait(
            ServerCommand::Gameplay(SemanticCommand::Travel),
            &both_rooms,
            |snapshot| {
                snapshot.phase == GamePhase::Running
                    && snapshot.level == Some(LevelKey::main(LevelId::Cavern))
            },
        );
        assert!(travelled.current_instance.is_some());
        assert_ne!(travelled.current_instance, startup.current_instance);
        assert_eq!(travelled.travel_count, 1);
        assert_eq!(travelled.side_room, SideRoomPhase::Closed);
        assert_eq!(travelled.lookout, SideRoomPhase::Closed);
        let queue = session
            .observe(|world| world.resource::<CommandSchedule>().stats())
            .unwrap();
        assert_eq!(queue.acknowledged, 3);
        assert_eq!(queue.rejected, 0);
        assert_eq!(queue.retained_commands, 0);
    }));
    let deadline = Instant::now() + Duration::from_secs(10);
    let closed = loop {
        match session.close() {
            HeadlessSessionClose::Pending if Instant::now() < deadline => std::thread::yield_now(),
            outcome => break outcome,
        }
    };
    assert_eq!(
        closed,
        HeadlessSessionClose::Complete,
        "{:?}",
        session.diagnostics()
    );
    if let Err(panic) = exercise {
        resume_unwind(panic);
    }
    println!(
        "TWO_LEVEL_SERVER_CLIENT_SERVICES local_player=true services_absent=true regions=open travel=committed"
    );
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
