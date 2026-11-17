use std::ops::Range;

use nara::{
    asset::AssetProductRef,
    fs::ContentDigest,
    runtime_content::{
        RuntimeContentCatalog, RuntimeContentCatalogLimits, RuntimeContentMemberKind,
    },
};
#[cfg(all(feature = "cook", feature = "development"))]
use serde::de::DeserializeOwned;

const HEADER_BYTES: usize = 20;
const MAX_PACK_BYTES: usize = 32 * 1024 * 1024;

struct KnownPack {
    catalog_bytes: Range<usize>,
    catalog: RuntimeContentCatalog,
    members: Vec<Range<usize>>,
}

fn inspect_pack(bytes: &[u8]) -> Result<KnownPack, String> {
    if bytes.len() > MAX_PACK_BYTES
        || bytes.len() < HEADER_BYTES
        || &bytes[..8] != b"NARAPACK"
        || bytes[8..12] != 1_u32.to_le_bytes()
    {
        return Err("expected the bounded version-one Two-Level content.pack".into());
    }
    let length = usize::try_from(u64::from_le_bytes(bytes[12..20].try_into().unwrap()))
        .map_err(|_| "catalog length does not fit the host")?;
    let end = HEADER_BYTES
        .checked_add(length)
        .filter(|end| *end <= bytes.len())
        .ok_or("catalog extends past content.pack")?;
    let catalog_bytes = HEADER_BYTES..end;
    let catalog = RuntimeContentCatalog::from_json_bytes(
        &bytes[catalog_bytes.clone()],
        RuntimeContentCatalogLimits::default(),
    )
    .map_err(|error| error.to_string())?;
    let mut offset = end;
    let mut members = Vec::with_capacity(catalog.members().len());
    for member in catalog.members() {
        let length = usize::try_from(member.digest().length())
            .map_err(|_| "member length does not fit the host")?;
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .ok_or("member extends past content.pack")?;
        members.push(offset..end);
        offset = end;
    }
    if offset != bytes.len() {
        return Err("content.pack has an unexpected trailing extent".into());
    }
    Ok(KnownPack {
        catalog_bytes,
        catalog,
        members,
    })
}

fn corrupt_later_scene(bytes: &mut [u8], product: AssetProductRef) -> Result<Range<usize>, String> {
    let pack = inspect_pack(bytes)?;
    if product == pack.catalog.startup_scene() || !pack.catalog.scenes().contains(&product) {
        return Err("corruption must select a declared non-startup Scene".into());
    }
    let index = pack
        .catalog
        .members()
        .iter()
        .position(|member| member.products().contains(&product))
        .ok_or("later Scene has no member")?;
    if !matches!(
        pack.catalog.members()[index].kind(),
        RuntimeContentMemberKind::Scene { .. }
    ) {
        return Err("selected later product is not a Scene member".into());
    }
    for (member, range) in pack.catalog.members().iter().zip(&pack.members) {
        if ContentDigest::of_bytes(&bytes[range.clone()]) != member.digest() {
            return Err("corruption requires previously valid member digests".into());
        }
    }
    let range = pack.members[index].clone();
    if range.is_empty() {
        return Err("later Scene member must be nonempty".into());
    }
    bytes[range.start + range.len() / 2] ^= 1;
    Ok(range)
}

fn corrupt_catalog(bytes: &mut [u8]) -> Result<Range<usize>, String> {
    let pack = inspect_pack(bytes)?;
    let range = pack.catalog_bytes;
    if bytes[range.start] != b'{' {
        return Err("expected the production JSON catalog envelope".into());
    }
    bytes[range.start] ^= 1;
    Ok(range)
}

#[cfg(all(feature = "cook", feature = "development"))]
fn record<T: DeserializeOwned>(text: &str, prefix: &str) -> Result<T, String> {
    let mut records = text.lines().filter_map(|line| line.strip_prefix(prefix));
    let json = records
        .next()
        .ok_or_else(|| format!("missing {prefix} record"))?;
    if records.next().is_some() {
        return Err(format!("duplicate {prefix} records"));
    }
    serde_json::from_str(json).map_err(|error| format!("invalid {prefix} JSON: {error}"))
}

#[cfg(all(feature = "cook", feature = "development"))]
#[path = "cooked_audio/images.rs"]
mod images;

#[cfg(all(feature = "cook", feature = "development"))]
#[path = "cooked_desktop/walls.rs"]
mod walls;

#[cfg(all(feature = "cook", feature = "development"))]
pub mod native {
    use std::{
        collections::{BTreeMap, BTreeSet},
        ffi::OsString,
        fs::{self, File},
        io::Read,
        num::NonZeroUsize,
        path::{Path, PathBuf},
        thread,
        time::{Duration, Instant},
    };

    use nara::{
        asset::{AssetProductRef, RuntimeAssetCodec},
        fs::{ContentDigest, RelativePath},
        image::{ImageColorSpace, ImageExtent, ImageFormat, ImageRuntimeCodec},
        project_tooling::{
            cargo_build::CargoBuildTools,
            host_fs,
            process::{
                ExactExecutableLaunchAuthority, ProcessLogLimits, ProcessOutputStream,
                SupervisedProcessCleanupReceipt, SupervisedProcessCommand, SupervisedProcessOwner,
                SupervisedProcessState, SupervisedProductEnvironmentPolicy,
                SupervisedProductProcessCommand,
            },
        },
        runtime_content::{RuntimeContentDependencyRole, RuntimeContentMemberKind},
        scene::{SourceDocumentKey, SourceDocumentKind, SourceDocumentOrigin},
    };
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use two_level_game::{
        animation::atlas_product,
        levels::{AUTHORED_LEVEL_MEMBERS, LevelId, LevelKey},
        save::{Checkpoint, MAX_CHECKPOINT_BYTES, decode_checkpoint},
        sound_bank::{SoundBankCodec, bank_product},
    };

    use crate::continuity::Fixture;

    use super::{
        HEADER_BYTES, KnownPack, MAX_PACK_BYTES, corrupt_catalog, corrupt_later_scene, images,
        inspect_pack, record, walls,
    };

    const OUTPUT_BYTES: usize = 2 * 1024 * 1024;
    const POLL: Duration = Duration::from_millis(10);
    const PLAYER_TIMEOUT: Duration = Duration::from_secs(120);
    const COOK_TIMEOUT: Duration = Duration::from_secs(30 * 60);
    const CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);
    const WORKER: &str = "cooked_delivery_sealed_player_worker";
    const REQUEST: &str = "two-level-player-request.json";
    const CLEANUP: &str = "TWO_LEVEL_NATIVE_CLEANUP ";
    const PUBLISHED: &str = "TWO_LEVEL_PACKAGE_PUBLISHED ";
    const RESULT: &str = "TWO_LEVEL_PLAYER_RESULT ";

    #[derive(Deserialize)]
    struct Publication {
        directory: PathBuf,
        executable: String,
        postpublication_error: Option<String>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PlayerRequest {
        executable: PathBuf,
        cwd: PathBuf,
        invalid_path: PathBuf,
        arguments: Vec<OsString>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct NativeExit {
        code: Option<i32>,
        success: bool,
        containment_empty: bool,
        readers_stopped: bool,
        output_drained: bool,
        force_killed: bool,
    }

    impl From<SupervisedProcessCleanupReceipt> for NativeExit {
        fn from(receipt: SupervisedProcessCleanupReceipt) -> Self {
            Self {
                code: receipt.exit().code(),
                success: receipt.exit().success(),
                containment_empty: receipt.containment_empty(),
                readers_stopped: receipt.readers_stopped(),
                output_drained: receipt.output_drained(),
                force_killed: receipt.force_killed(),
            }
        }
    }

    impl NativeExit {
        fn assert_clean(&self) {
            assert!(
                self.containment_empty
                    && self.readers_stopped
                    && self.output_drained
                    && !self.force_killed,
                "native process did not retire normally: {self:?}",
            );
        }
    }

    struct Output {
        stdout: String,
        stderr: String,
        receipt: SupervisedProcessCleanupReceipt,
    }

    struct Custody {
        fixture: Option<Fixture>,
        process: Option<SupervisedProcessOwner>,
    }

    impl Custody {
        fn run(&mut self, process: SupervisedProcessOwner, timeout: Duration) -> Output {
            assert!(self.process.is_none(), "all native operations are serial");
            self.process = Some(process);
            let deadline = Instant::now() + timeout;
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            loop {
                let owner = self.process.as_mut().unwrap();
                let (_, outputs) =
                    owner
                        .advance_with_output(Instant::now())
                        .unwrap_or_else(|error| {
                            panic!("native observation: {error}; {}", diagnostics(owner))
                        });
                for output in outputs {
                    let bytes = match output.stream() {
                        ProcessOutputStream::Stdout => &mut stdout,
                        ProcessOutputStream::Stderr => &mut stderr,
                    };
                    assert!(
                        !output.loss() && output.bytes().len() <= OUTPUT_BYTES - bytes.len(),
                        "native output exceeded its exact finite allowance: {}",
                        diagnostics(owner),
                    );
                    bytes.extend_from_slice(output.bytes());
                }
                if owner.state() == SupervisedProcessState::Complete {
                    let receipt = owner
                        .cleanup_receipt()
                        .expect("original native cleanup receipt");
                    NativeExit::from(receipt).assert_clean();
                    assert!(!owner.output_loss_observed(), "{}", diagnostics(owner));
                    self.process.take();
                    return Output {
                        stdout: String::from_utf8(stdout).expect("native stdout must be UTF-8"),
                        stderr: String::from_utf8(stderr).expect("native stderr must be UTF-8"),
                        receipt,
                    };
                }
                assert!(
                    Instant::now() < deadline,
                    "native process exceeded {timeout:?}: {}",
                    diagnostics(owner),
                );
                thread::sleep(POLL);
            }
        }

        fn fixture(&self) -> &Fixture {
            self.fixture.as_ref().unwrap()
        }

        fn player(&mut self, executable: &Path, arguments: Vec<OsString>, name: &str) -> Output {
            let worker = self.fixture().directory(name);
            let cwd = worker.join("empty");
            fs::create_dir(&cwd).unwrap();
            let invalid_path = worker.join("no-cargo-or-rustc");
            assert!(!invalid_path.exists());
            let request = PlayerRequest {
                executable: executable.to_owned(),
                cwd: cwd.clone(),
                invalid_path,
                arguments,
            };
            fs::write(worker.join(REQUEST), serde_json::to_vec(&request).unwrap()).unwrap();
            let authority = exact_authority(&std::env::current_exe().unwrap());
            #[cfg(windows)]
            let policy = SupervisedProductEnvironmentPolicy::AllowlistedDesktopSession;
            #[cfg(not(windows))]
            let policy = SupervisedProductEnvironmentPolicy::Sealed;
            let command = SupervisedProductProcessCommand::new(policy, &worker).arguments([
                "--exact",
                WORKER,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ]);
            let process =
                SupervisedProcessOwner::spawn_exact(authority, &command, log_limits()).unwrap();
            let output = self.run(process, PLAYER_TIMEOUT + Duration::from_secs(45));
            assert!(
                output.receipt.exit().success(),
                "sealed worker failed: stdout={} stderr={}",
                output.stdout,
                output.stderr,
            );
            let cleanup: NativeExit = record(&output.stdout, CLEANUP).unwrap();
            cleanup.assert_clean();
            assert!(fs::read_dir(cwd).unwrap().next().is_none());
            output
        }
    }

    impl Drop for Custody {
        fn drop(&mut self) {
            let Some(owner) = self.process.as_mut() else {
                return;
            };
            let _ = owner.request_terminate(Instant::now());
            for _ in 0..2 {
                let deadline = Instant::now() + CLEANUP_TIMEOUT;
                while owner.state() != SupervisedProcessState::Complete && Instant::now() < deadline
                {
                    if owner.advance(Instant::now()).is_err() {
                        let _ = owner.retry_cleanup(Instant::now());
                    }
                    thread::sleep(POLL);
                }
                if owner.state() == SupervisedProcessState::Complete
                    && owner.cleanup_receipt().is_some_and(|receipt| {
                        receipt.containment_empty()
                            && receipt.readers_stopped()
                            && receipt.output_drained()
                    })
                {
                    return;
                }
                let _ = owner.retry_cleanup(Instant::now());
            }
            eprintln!(
                "Two-Level delivery retains original native custody: {}",
                diagnostics(owner),
            );
            let owner = self.process.take().unwrap();
            let fixture = self.fixture.take();
            std::mem::forget((owner, fixture));
            if !thread::panicking() {
                panic!("native cleanup is incomplete; the owned source/package tree was retained");
            }
        }
    }

    pub fn player_worker() {
        #[cfg(windows)]
        let allowed = ["SystemRoot", "WINDIR", "TEMP", "TMP"];
        #[cfg(not(windows))]
        let allowed: [&str; 0] = [];
        for (name, _) in std::env::vars_os() {
            let name = name.to_string_lossy();
            assert!(
                allowed
                    .iter()
                    .any(|allowed| name.eq_ignore_ascii_case(allowed)),
                "sealed worker inherited an unexpected variable: {name}",
            );
        }
        let request: PlayerRequest = serde_json::from_slice(&read_bounded(
            &std::env::current_dir().unwrap().join(REQUEST),
            32 * 1024,
        ))
        .unwrap();
        assert!(request.executable.is_absolute() && request.cwd.is_absolute());
        assert!(request.invalid_path.is_absolute() && !request.invalid_path.exists());
        let command = SupervisedProcessCommand::new(&request.executable, &request.cwd)
            .arguments(request.arguments)
            .environment("PATH", request.invalid_path)
            .bounded_stdout(NonZeroUsize::new(OUTPUT_BYTES).unwrap());
        let mut custody = Custody {
            fixture: None,
            process: None,
        };
        let owner = SupervisedProcessOwner::spawn(&command, log_limits()).unwrap();
        let output = custody.run(owner, PLAYER_TIMEOUT);
        println!();
        print!("{}", output.stdout);
        eprint!("{}", output.stderr);
        println!(
            "{CLEANUP}{}",
            serde_json::to_string(&NativeExit::from(output.receipt)).unwrap(),
        );
    }

    pub fn accept(cooker: &Path) {
        let fixture = Fixture::new();
        assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
        fixture.assert_player_feature_closure(false);
        assert!(fixture.project.join("build.rs").is_file());
        let target = fixture.repository.join("target").canonicalize().unwrap();
        let tools = CargoBuildTools::discover_in(&fixture.repository).unwrap();
        let triple = fixture.request().selection().target_triple().to_owned();
        let output_parent = fixture.directory("cook-output");
        let copied_lock = read_bounded(&fixture.project.join("Cargo.lock"), 1024 * 1024);
        let command = SupervisedProcessCommand::new(cooker, &fixture.root)
            .arguments([
                OsString::from("--project-root"),
                fixture.project.as_os_str().to_owned(),
                OsString::from("--output-parent"),
                output_parent.as_os_str().to_owned(),
                OsString::from("--name"),
                OsString::from("TwoLevel"),
                OsString::from("--target-dir"),
                target.as_os_str().to_owned(),
                OsString::from("--target"),
                OsString::from(&triple),
                OsString::from("--cargo"),
                tools.cargo_path().as_os_str().to_owned(),
                OsString::from("--rustc"),
                tools.rustc_path().as_os_str().to_owned(),
                OsString::from("--profile"),
                OsString::from("release"),
            ])
            .environment("CARGO_BUILD_JOBS", "1")
            .environment("CARGO_NET_OFFLINE", "true")
            .environment("CARGO_TARGET_DIR", &target)
            .environment("RUSTUP_TOOLCHAIN", "1.95.0")
            .environment("RUSTUP_AUTO_INSTALL", "0")
            .environment("CARGO", tools.cargo_path())
            .environment("RUSTC", tools.rustc_path())
            .bounded_stdout(NonZeroUsize::new(OUTPUT_BYTES).unwrap());
        let mut custody = Custody {
            fixture: Some(fixture),
            process: None,
        };
        let owner = SupervisedProcessOwner::spawn(&command, log_limits()).unwrap();
        let cooked = custody.run(owner, COOK_TIMEOUT);
        assert!(
            cooked.receipt.exit().success(),
            "stdout={} stderr={}",
            cooked.stdout,
            cooked.stderr,
        );
        assert_eq!(
            read_bounded(&custody.fixture().project.join("Cargo.lock"), 1024 * 1024),
            copied_lock,
            "cooking must retain the existing lock without dependency resolution",
        );
        let publication: Publication = record(&cooked.stdout, PUBLISHED).unwrap();
        assert!(
            publication.postpublication_error.is_none(),
            "publication did not complete cleanly",
        );
        let published = output_parent.join("TwoLevel").canonicalize().unwrap();
        assert_eq!(publication.directory.canonicalize().unwrap(), published);
        let executable_name = format!("player{}", std::env::consts::EXE_SUFFIX);
        assert_eq!(publication.executable, executable_name);
        assert_payload(&published, &executable_name);
        let package_bytes = read_bounded(&published.join("package.json"), 1024 * 1024);
        let manifest: Value = serde_json::from_slice(&package_bytes).unwrap();
        let pack_bytes = read_bounded(&published.join("content.pack"), MAX_PACK_BYTES);
        let pack = inspect_pack(&pack_bytes).unwrap();
        let catalog_digest = ContentDigest::of_bytes(&pack_bytes[pack.catalog_bytes.clone()]);
        let payload = &manifest["payload"];
        assert_eq!(payload["executable_name"], executable_name);
        assert_eq!(payload["catalog_digest"], digest_json(catalog_digest));
        assert_eq!(
            payload["pack_digest"],
            digest_json(ContentDigest::of_bytes(&pack_bytes))
        );
        assert_eq!(payload["pack_file"], "content.pack");
        let executable_bytes = read_bounded(&published.join(&executable_name), 512 * 1024 * 1024);
        assert_eq!(
            payload["executable_digest"],
            digest_json(ContentDigest::of_bytes(&executable_bytes))
        );
        assert_eq!(payload["build"]["selection"]["package"], "two-level-game");
        assert_eq!(payload["build"]["selection"]["bin"], "player");
        assert_eq!(payload["build"]["selection"]["cargo_profile"], "release");
        assert_eq!(payload["build"]["selection"]["target_triple"], triple);
        assert_eq!(payload["build"]["selection"]["default_features"], false);
        assert_eq!(
            payload["build"]["selection"]["requested_features"],
            json!(["cooked-player"])
        );
        assert_eq!(pack.catalog.target().triple(), triple);
        let mut scenes = LevelId::ALL.map(LevelId::scene_product).to_vec();
        scenes.sort();
        assert_eq!(pack.catalog.scenes(), scenes);
        assert_eq!(
            pack.catalog.startup_scene(),
            LevelId::Meadow.scene_product()
        );
        assert!(pack.catalog.addressable_roots().is_empty());
        assert_eq!(pack.catalog.members().len(), 6);
        let mut products = BTreeSet::new();
        for member in pack.catalog.members() {
            assert_eq!(member.products().len(), 1);
            assert!(products.insert(member.products()[0]));
        }
        assert_eq!(
            products,
            LevelId::ALL
                .into_iter()
                .flat_map(|level| [level.scene_product(), level.image_product()])
                .chain([atlas_product(), bank_product()])
                .collect()
        );
        for level in LevelId::ALL {
            assert!(
                pack.catalog
                    .member_for_product(level.image_product())
                    .is_some()
            );
            let scene = pack
                .catalog
                .member_for_product(level.scene_product())
                .unwrap();
            assert!(matches!(
                scene.kind(),
                RuntimeContentMemberKind::Scene { .. }
            ));
            assert_eq!(scene.dependencies().len(), 3);
            assert_eq!(
                scene
                    .dependencies()
                    .iter()
                    .map(|dependency| dependency.product())
                    .collect::<BTreeSet<_>>(),
                [level.image_product(), atlas_product(), bank_product()]
                    .into_iter()
                    .collect()
            );
            assert!(
                scene.dependencies().iter().all(|dependency| {
                    dependency.role() == RuntimeContentDependencyRole::Required
                })
            );
        }
        let bank = pack.catalog.member_for_product(bank_product()).unwrap();
        assert!(matches!(
            bank.kind(),
            RuntimeContentMemberKind::Asset { format, version }
                if format.as_str() == SoundBankCodec::RUNTIME_KIND
                    && version.get() == SoundBankCodec::RUNTIME_VERSION
        ));
        for (member, extent) in pack.catalog.members().iter().zip(&pack.members) {
            assert_eq!(
                ContentDigest::of_bytes(&pack_bytes[extent.clone()]),
                member.digest()
            );
        }
        let expected_images = expected_images(&pack, &pack_bytes);
        let bootstrap = payload["generated_input_path"].as_str().unwrap();
        assert!(RelativePath::new(bootstrap).is_ok());
        assert_eq!(bootstrap, "src/bin/player/generated.rs");
        let bootstrap_bytes = read_bounded(&custody.fixture().project.join(bootstrap), 32 * 1024);
        assert_eq!(
            payload["generated_input_digest"],
            digest_json(ContentDigest::of_bytes(&bootstrap_bytes))
        );
        let inputs = payload["build"]["declared_inputs"].as_array().unwrap();
        for required in [
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            "src/bin/player.rs",
            "src/bin/player/runtime.rs",
            "src/bin/player/evidence.rs",
            "src/gameplay/mod.rs",
            "src/gameplay/player_launch.rs",
            "src/gameplay/verification.rs",
            "src/animation.rs",
            "src/levels.rs",
            bootstrap,
        ] {
            assert!(
                inputs.iter().any(|input| input["path"] == required),
                "missing actual code input {required}",
            );
        }
        let installation = custody.fixture().directory("relocated-installation");
        let relocated = installation.join("TwoLevel");
        assert_eq!(published.parent(), Some(output_parent.as_path()));
        assert!(!relocated.exists());
        fs::rename(&published, &relocated).unwrap();
        assert!(!published.exists());
        assert_eq!(
            fs::read(relocated.join("package.json")).unwrap(),
            package_bytes
        );
        let executable = relocated.join(&executable_name);
        let project = custody.fixture().project.canonicalize().unwrap();
        let cache = output_parent
            .join("two-level-cook-cache")
            .canonicalize()
            .unwrap();
        assert!(cache.is_dir());
        remove_owned_tree(custody.fixture(), &project, &target);
        remove_owned_tree(custody.fixture(), &cache, &target);
        assert_sources_gone(&project, &cache);
        assert!(target.is_dir() && custody.fixture().repository.join("Cargo.toml").is_file());

        let startup = custody.player(&executable, vec![], "startup");
        let startup = assert_player(&startup, "interactive", true);
        assert_eq!(startup["score"], 0);
        assert_eq!(startup["collected"], 0);
        assert_eq!(startup["level"], json!(LevelKey::main(LevelId::Meadow)));
        assert_images(&startup, &[LevelId::Meadow], &expected_images);
        assert_eq!(startup["markers"], json!([]));
        assert_eq!(startup["state"]["travel_count"], 0);

        let travel = custody.player(&executable, vec!["--verify-travel".into()], "travel");
        let travel = assert_player(&travel, "travel", true);
        assert_progress(&travel, LevelId::Meadow, &[1]);
        assert_eq!(travel["animation_frames"], json!([0, 1, 2, 3]));
        assert_images(&travel, &LevelId::ALL, &expected_images);
        assert_scene_sequence(
            &travel,
            &[
                LevelId::Meadow,
                LevelId::Cavern,
                LevelId::Cavern,
                LevelId::Cavern,
                LevelId::Cavern,
                LevelId::Meadow,
            ],
        );
        assert_eq!(travel["state"]["travel_count"], 4);
        assert_eq!(travel["state"]["restarts"], 1);
        let save_parent = custody.fixture().directory("application-save");
        let save = save_parent.join("checkpoint.json");
        let saved = custody.player(
            &executable,
            vec!["--verify-save".into(), save.as_os_str().to_owned()],
            "save",
        );
        let saved = assert_player(&saved, "save", true);
        assert_progress(&saved, LevelId::Cavern, &[2]);
        assert_eq!(saved["animation_frames"], json!([0, 1, 2, 3]));
        assert_images(&saved, &LevelId::ALL, &expected_images);
        assert_scene_sequence(
            &saved,
            &[
                LevelId::Meadow,
                LevelId::Cavern,
                LevelId::Cavern,
                LevelId::Cavern,
                LevelId::Cavern,
            ],
        );
        assert_eq!(saved["state"]["saves"], 1);
        assert_eq!(saved["save"]["load_verified"], true);
        assert_eq!(
            saved["save"]["format_version"],
            two_level_game::save::CURRENT_FORMAT_VERSION
        );
        assert_eq!(saved["save"]["next_marker_id"], 4);
        let checkpoint_bytes = read_bounded(&save, MAX_CHECKPOINT_BYTES);
        let decoded = decode_checkpoint(&checkpoint_bytes).unwrap();
        assert!(decoded.migrated_from.is_none());
        assert_checkpoint(&decoded.checkpoint);
        assert_eq!(saved["save"]["markers"], json!(decoded.checkpoint.markers));
        let restored = custody.player(
            &executable,
            vec!["--verify-restore".into(), save.as_os_str().to_owned()],
            "restore",
        );
        let restored = assert_player(&restored, "restore", true);
        assert_progress(&restored, LevelId::Meadow, &[1]);
        assert_eq!(restored["animation_frames"], json!([0]));
        assert_images(&restored, &LevelId::ALL, &expected_images);
        assert_scene_sequence(
            &restored,
            &[
                LevelId::Meadow,
                LevelId::Cavern,
                LevelId::Cavern,
                LevelId::Meadow,
            ],
        );
        assert_eq!(restored["state"]["restores"], 1);
        assert_eq!(read_bounded(&save, MAX_CHECKPOINT_BYTES), checkpoint_bytes);
        assert_sources_gone(&project, &cache);

        let pack_path = relocated.join("content.pack");
        let mut damaged = pack_bytes.clone();
        let cavern = corrupt_later_scene(&mut damaged, LevelId::Cavern.scene_product()).unwrap();
        assert_eq!(
            &damaged[pack.catalog_bytes.clone()],
            &pack_bytes[pack.catalog_bytes.clone()]
        );
        assert_eq!(damaged.len(), pack_bytes.len());
        assert_eq!(&damaged[..cavern.start], &pack_bytes[..cavern.start]);
        assert_eq!(&damaged[cavern.end..], &pack_bytes[cavern.end..]);
        fs::write(&pack_path, &damaged).unwrap();
        let startup = custody.player(&executable, vec![], "damaged-later-startup");
        assert_images(
            &assert_player(&startup, "interactive", true),
            &[LevelId::Meadow],
            &expected_images,
        );
        let refused = custody.player(
            &executable,
            vec!["--verify-travel".into()],
            "damaged-travel",
        );
        let refused = assert_player(&refused, "travel", false);
        assert_eq!(refused["level"], json!(LevelKey::main(LevelId::Meadow)));
        assert_eq!(refused["score"], 10);
        assert_eq!(refused["collected"], 1);
        assert_eq!(refused["markers"], json!([1]));
        assert_eq!(refused["preserved_startup_instance"], true);
        assert!(
            refused["state"]["error"]
                .as_str()
                .is_some_and(|error| !error.is_empty())
        );
        assert_eq!(refused["state"]["travel_count"], 0);
        assert_eq!(refused["state"]["visible_pickups"], 1);
        assert_eq!(refused["current_receipt"], refused["startup_receipt"]);
        assert_scene_sequence(&refused, &[LevelId::Meadow]);
        assert_images(&refused, &[LevelId::Meadow], &expected_images);
        let mut bad_catalog = pack_bytes.clone();
        let changed_catalog = corrupt_catalog(&mut bad_catalog).unwrap();
        assert_eq!(&bad_catalog[..HEADER_BYTES], &pack_bytes[..HEADER_BYTES]);
        assert_eq!(
            &bad_catalog[changed_catalog.end..],
            &pack_bytes[changed_catalog.end..]
        );
        fs::write(&pack_path, &bad_catalog).unwrap();
        let refused = custody.player(&executable, vec![], "damaged-catalog");
        let exit: NativeExit = record(&refused.stdout, CLEANUP).unwrap();
        assert!(!exit.success && exit.code != Some(0));
        let rejected: Value = record(&refused.stdout, RESULT).unwrap();
        assert_eq!(rejected["success"], false);
        assert_eq!(rejected["scenario"], "interactive");
        assert_eq!(rejected["source_free"], true);
        assert_eq!(rejected["cleanup_complete"], true);
        for missing in [
            "level",
            "score",
            "collected",
            "state",
            "startup_receipt",
            "current_receipt",
        ] {
            assert!(
                rejected[missing].is_null(),
                "catalog refusal activated gameplay: {rejected}"
            );
        }
        for empty in [
            "markers",
            "visited_levels",
            "observed_image_products",
            "resident_images",
            "image_proofs",
            "scene_receipts",
        ] {
            assert_eq!(rejected[empty], json!([]));
        }
        let message = rejected["message"].as_str().unwrap().to_ascii_lowercase();
        assert!(
            message.contains("catalog") || message.contains("pack"),
            "{rejected}"
        );
        assert_eq!(
            fs::read(relocated.join("package.json")).unwrap(),
            package_bytes
        );
        assert_eq!(
            read_bounded(&executable, 512 * 1024 * 1024),
            executable_bytes
        );
        assert_payload(&relocated, &executable_name);
        assert_sources_gone(&project, &cache);
        assert!(target.is_dir());
    }

    fn assert_player(output: &Output, scenario: &str, success: bool) -> Value {
        let exit: NativeExit = record(&output.stdout, CLEANUP).unwrap();
        exit.assert_clean();
        assert_eq!(
            exit.success, success,
            "stdout={} stderr={}",
            output.stdout, output.stderr
        );
        assert_eq!(exit.code == Some(0), success);
        let result: Value = record(&output.stdout, RESULT).unwrap();
        assert_eq!(result["success"], success, "{result}");
        assert_eq!(result["scenario"], scenario);
        assert_eq!(result["source_free"], true);
        assert_eq!(result["cleanup_complete"], true);
        for field in ["level", "score", "collected", "markers"] {
            assert_eq!(result[field], result["state"][field]);
        }
        assert_runtime_receipts(&result);
        if success {
            assert_eq!(result["state"]["phase"], "Running");
            assert_eq!(result["state"]["initialized"], true);
            assert_eq!(result["state"]["restoring"], false);
            assert!(result["state"]["error"].is_null());
            if scenario != "interactive" {
                assert_eq!(result["state"]["verified"], true);
            }
        }
        result
    }

    fn assert_progress(result: &Value, level: LevelId, markers: &[u64]) {
        assert_eq!(result["level"], json!(LevelKey::main(level)));
        assert_eq!(result["score"], 30);
        assert_eq!(result["collected"], 3);
        assert_eq!(result["markers"], json!(markers));
    }

    fn expected_images(pack: &KnownPack, bytes: &[u8]) -> BTreeMap<AssetProductRef, Value> {
        let mut images: BTreeMap<_, _> = LevelId::ALL
            .into_iter()
            .map(|level| {
                let index = pack
                    .catalog
                    .members()
                    .iter()
                    .position(|member| member.products().contains(&level.image_product()))
                    .unwrap();
                let member = &pack.catalog.members()[index];
                assert!(matches!(
                    member.kind(),
                    RuntimeContentMemberKind::Asset { format, version }
                        if format.as_str() == ImageRuntimeCodec::RUNTIME_KIND
                            && version.get() == ImageRuntimeCodec::RUNTIME_VERSION
                ));
                let image =
                    ImageRuntimeCodec::decode_runtime(&bytes[pack.members[index].clone()]).unwrap();
                assert_eq!(image.product(), Some(level.image_product()));
                assert_eq!(image.extent(), ImageExtent::new(16, 16));
                assert_eq!(image.format(), ImageFormat::Rgba8);
                assert_eq!(image.color_space(), ImageColorSpace::Srgb);
                assert_eq!(image.pixels().len(), 1024);
                assert!(image.source().is_none());
                (
                    level.image_product(),
                    json!({
                        "level": level,
                        "product": level.image_product().to_string(),
                        "format": image.format(),
                        "color_space": image.color_space(),
                        "extent": image.extent(),
                        "pixel_bytes": image.pixels().len(),
                        "pixel_blake3": ContentDigest::of_bytes(image.pixels()).as_bytes(),
                        "content_hash": image.content_hash().as_bytes(),
                        "source_free": true,
                    }),
                )
            })
            .collect();
        let index = pack
            .catalog
            .members()
            .iter()
            .position(|member| member.products() == [atlas_product()])
            .unwrap();
        let member = &pack.catalog.members()[index];
        assert!(matches!(
            member.kind(),
            RuntimeContentMemberKind::Asset { format, version }
                if format.as_str() == ImageRuntimeCodec::RUNTIME_KIND
                    && version.get() == ImageRuntimeCodec::RUNTIME_VERSION
        ));
        assert!(member.dependencies().is_empty());
        let image = ImageRuntimeCodec::decode_runtime(&bytes[pack.members[index].clone()]).unwrap();
        assert!(
            images
                .insert(atlas_product(), images::atlas_proof(&image))
                .is_none()
        );
        assert_eq!(images.len(), 3);
        images
    }

    fn assert_images(
        result: &Value,
        levels: &[LevelId],
        images: &BTreeMap<AssetProductRef, Value>,
    ) {
        let current: LevelKey = serde_json::from_value(result["level"].clone()).unwrap();
        let resident: BTreeSet<_> = [current.level().image_product(), atlas_product()]
            .into_iter()
            .collect();
        assert_eq!(
            result["resident_images"],
            json!(resident.iter().map(ToString::to_string).collect::<Vec<_>>())
        );
        let actual = result["observed_image_products"]
            .as_array()
            .expect("actual ProcessedRuntime Image residency proofs")
            .iter()
            .map(|proof| proof.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let mut expected = levels
            .iter()
            .map(|level| level.image_product().to_string())
            .chain([atlas_product().to_string()])
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(actual, expected);
        let proofs = result["image_proofs"].as_array().unwrap();
        assert_eq!(proofs.len(), levels.len() + 1);
        assert_eq!(
            proofs
                .iter()
                .map(|proof| proof["product"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            expected
        );
        for level in levels {
            assert_eq!(
                proofs
                    .iter()
                    .filter(|proof| **proof == images[&level.image_product()])
                    .count(),
                1
            );
        }
        assert_eq!(
            proofs
                .iter()
                .filter(|proof| **proof == images[&atlas_product()])
                .count(),
            1
        );
        let visited: Vec<LevelId> =
            serde_json::from_value(result["visited_levels"].clone()).unwrap();
        assert_eq!(visited.len(), levels.len());
        assert_eq!(
            visited.into_iter().collect::<BTreeSet<_>>(),
            levels.iter().copied().collect()
        );
    }

    fn assert_runtime_receipts(result: &Value) {
        let startup = &result["startup_receipt"];
        let current = &result["current_receipt"];
        let receipts = result["scene_receipts"]
            .as_array()
            .expect("actual Runtime Scene observations");
        assert!(!receipts.is_empty() && receipts.len() <= 16);
        assert_eq!(receipts.first().unwrap()["level"], json!(LevelId::Meadow));
        assert_eq!(receipts.first().unwrap()["receipt"], *startup);
        assert_eq!(receipts.last().unwrap()["receipt"], *current);
        assert_eq!(result["state"]["current_instance"], current["instance_id"]);
        assert_eq!(result["state"]["scene_members"], AUTHORED_LEVEL_MEMBERS);
        assert!(startup["domain_id"].as_u64().unwrap() > 0);
        let mut instances = BTreeSet::new();
        for observed in receipts {
            let level: LevelId = serde_json::from_value(observed["level"].clone()).unwrap();
            assert_eq!(observed["representation"], "runtime_scene");
            assert_eq!(observed["scene_product"], level.scene_product().to_string());
            let source: SourceDocumentKey =
                serde_json::from_value(observed["source_key"].clone()).unwrap();
            assert_eq!(source.kind(), SourceDocumentKind::Scene);
            let SourceDocumentOrigin::ProjectLocation { universe, path } = source.origin() else {
                panic!("processed Scene lost its original ProjectLocation: {source:?}");
            };
            assert_eq!(universe.to_string(), "be3dea0c-93ef-5607-ab84-af715a1f98b3");
            assert_eq!(path.as_str(), format!("scenes/{}", level.scene_path()));
            let receipt = &observed["receipt"];
            assert_eq!(receipt["domain_id"], startup["domain_id"]);
            let instance = receipt["instance_id"].as_u64().unwrap();
            assert!(instance > 0 && instances.insert(instance));
            walls::scene(observed);
        }
    }

    fn assert_scene_sequence(result: &Value, expected: &[LevelId]) {
        let actual = result["scene_receipts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|observed| serde_json::from_value::<LevelId>(observed["level"].clone()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    fn assert_checkpoint(checkpoint: &Checkpoint) {
        let keys = [
            LevelKey::main(LevelId::Meadow),
            LevelKey::main(LevelId::Cavern),
            LevelKey::new(LevelId::Cavern, 1).unwrap(),
        ];
        assert_eq!(checkpoint.location.level, keys[1]);
        assert_eq!(checkpoint.score, 30);
        assert_eq!(checkpoint.collected.len(), 3);
        assert_eq!(checkpoint.markers.len(), 3);
        assert_eq!(checkpoint.next_marker_id.get(), 4);
        for (index, key) in keys.iter().enumerate() {
            let pickups = checkpoint
                .collected
                .iter()
                .filter(|pickup| pickup.level == *key)
                .collect::<Vec<_>>();
            assert_eq!(pickups.len(), 1);
            assert_eq!(pickups[0].entity.as_str(), "gem-near");
            let markers = checkpoint
                .markers
                .iter()
                .filter(|marker| marker.level == *key)
                .collect::<Vec<_>>();
            assert_eq!(markers.len(), 1);
            assert_eq!(markers[0].id.get(), index as u64 + 1);
        }
    }

    fn assert_payload(directory: &Path, executable: &str) {
        let mut names = fs::read_dir(directory)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                assert!(entry.file_type().unwrap().is_file());
                entry.file_name().into_string().unwrap()
            })
            .collect::<Vec<_>>();
        names.sort();
        let mut expected = vec![
            executable.to_owned(),
            "content.pack".into(),
            "package.json".into(),
        ];
        expected.sort();
        assert_eq!(names, expected);
    }

    fn assert_sources_gone(project: &Path, cache: &Path) {
        assert!(!project.exists());
        assert!(!cache.exists());
        for member in [
            "src",
            "assets",
            "scenes",
            "schema",
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            "src/bin/player/generated.rs",
        ] {
            assert!(
                !project.join(member).exists(),
                "source/build input survived: {member}"
            );
        }
    }

    fn remove_owned_tree(fixture: &Fixture, root: &Path, target: &Path) {
        assert_eq!(root, root.canonicalize().unwrap());
        assert!(
            root == fixture.project
                || root == fixture.root.join("cook-output/two-level-cook-cache")
        );
        assert!(root.starts_with(&fixture.root));
        assert_ne!(root, fixture.repository);
        assert!(!fixture.repository.starts_with(root) && !target.starts_with(root));
        let mut pending = vec![root.to_owned()];
        let mut visited = 0;
        while let Some(path) = pending.pop() {
            visited += 1;
            assert!(
                visited <= 16_384,
                "finite copied project/cache/build-input tree"
            );
            let metadata = fs::symlink_metadata(&path).unwrap();
            assert!(!metadata.file_type().is_symlink());
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                assert_eq!(
                    metadata.file_attributes() & 0x400,
                    0,
                    "no reparse points in owned deletion"
                );
            }
            assert_eq!(path.canonicalize().unwrap(), path);
            assert!(path.starts_with(root));
            if metadata.is_dir() {
                pending.extend(
                    fs::read_dir(&path)
                        .unwrap()
                        .map(|entry| entry.unwrap().path()),
                );
            } else {
                assert!(metadata.is_file());
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    fn digest_json(digest: ContentDigest) -> Value {
        json!({ "length": digest.length(), "hash": digest.as_bytes() })
    }

    fn read_bounded(path: &Path, limit: usize) -> Vec<u8> {
        let file = File::open(path).unwrap();
        assert!(
            file.metadata().unwrap().len() <= limit as u64,
            "{}",
            path.display()
        );
        let mut bytes = Vec::new();
        file.take(limit as u64 + 1).read_to_end(&mut bytes).unwrap();
        assert!(
            bytes.len() <= limit,
            "{} grew beyond its bound",
            path.display()
        );
        bytes
    }

    fn log_limits() -> ProcessLogLimits {
        ProcessLogLimits::try_from_counts(256, OUTPUT_BYTES).unwrap()
    }

    fn exact_authority(executable: &Path) -> ExactExecutableLaunchAuthority {
        let file = host_fs::readable_file(executable).unwrap();
        let directory = host_fs::readable_directory(executable.parent().unwrap()).unwrap();
        ExactExecutableLaunchAuthority::from_verified_handles(
            file.duplicate_restrictive_executable_host_handle().unwrap(),
            directory.duplicate_restrictive_host_handle().unwrap(),
        )
        .unwrap()
    }

    fn diagnostics(owner: &SupervisedProcessOwner) -> String {
        let mut result = format!(
            "pid={} state={:?} cleanup={:?} loss={}",
            owner.process_id(),
            owner.state(),
            owner.cleanup_receipt(),
            owner.output_loss_observed()
        );
        for entry in owner.logs().iter() {
            result.push_str(&format!(
                "\n{:?}: {}",
                entry.stream(),
                String::from_utf8_lossy(entry.bytes())
            ));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use nara::{
        asset::{AssetProductRef, ImportedProductId, StableAssetId},
        fs::ContentDigest,
        project::RuntimeConfiguration,
        runtime_content::{
            RuntimeContentCatalog, RuntimeContentCatalogDefinition, RuntimeContentCatalogLimits,
            RuntimeContentMember, RuntimeContentMemberKind, RuntimeContentTarget,
        },
        scene::{SourceDocumentKey, SourceDocumentKind},
    };

    use super::{HEADER_BYTES, corrupt_catalog, corrupt_later_scene, inspect_pack};

    fn product(last: u8) -> AssetProductRef {
        AssetProductRef::new(
            StableAssetId::parse_str(format!("00000000-0000-4000-8000-{last:012x}")).unwrap(),
            ImportedProductId::PRIMARY,
        )
        .unwrap()
    }

    fn pack() -> Vec<u8> {
        let startup = b"startup-scene-payload";
        let later = b"later-scene-payload-with-unequal-extent";
        let members = [
            ("scenes/a.runtime.json", product(1), startup.as_slice()),
            ("scenes/b.runtime.json", product(2), later.as_slice()),
        ]
        .into_iter()
        .map(|(path, product, bytes)| {
            RuntimeContentMember::new(
                path,
                ContentDigest::of_bytes(bytes),
                RuntimeContentMemberKind::Scene {
                    source: SourceDocumentKey::stable_asset(
                        product.source(),
                        SourceDocumentKind::Scene,
                    ),
                },
                vec![product],
                vec![],
            )
            .unwrap()
        })
        .collect();
        let catalog = RuntimeContentCatalog::new(
            RuntimeContentCatalogDefinition {
                target: RuntimeContentTarget::new("x86_64-pc-windows-msvc", "release").unwrap(),
                configuration: RuntimeConfiguration::default(),
                plugin_plan_fingerprint: [1; 32],
                schema_fingerprint: [2; 32],
                runtime_codecs_fingerprint: [3; 32],
                startup_scene: product(1),
                scenes: vec![product(1), product(2)],
                addressable_roots: vec![],
                members,
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap()
        .to_json_bytes()
        .unwrap();
        let mut bytes = b"NARAPACK".to_vec();
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&(catalog.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&catalog);
        bytes.extend_from_slice(startup);
        bytes.extend_from_slice(later);
        bytes
    }

    #[test]
    fn later_corruption_changes_one_byte_in_the_selected_member_only() {
        let original = pack();
        let admitted = inspect_pack(&original).unwrap();
        let mut damaged = original.clone();
        let changed = corrupt_later_scene(&mut damaged, product(2)).unwrap();
        assert_eq!(damaged.len(), original.len());
        assert_eq!(&damaged[..changed.start], &original[..changed.start]);
        assert_eq!(&damaged[changed.end..], &original[changed.end..]);
        assert_eq!(
            original
                .iter()
                .zip(&damaged)
                .filter(|(a, b)| a != b)
                .count(),
            1
        );
        let reopened = inspect_pack(&damaged).unwrap();
        assert_eq!(
            &damaged[reopened.catalog_bytes],
            &original[admitted.catalog_bytes]
        );
        for (index, member) in admitted.catalog.members().iter().enumerate() {
            let actual = ContentDigest::of_bytes(&damaged[admitted.members[index].clone()]);
            assert_eq!(
                actual == member.digest(),
                !member.products().contains(&product(2))
            );
        }
    }

    #[test]
    fn corruption_refuses_startup_unknown_products_and_already_damaged_members_without_writes() {
        let mut bytes = pack();
        let original = bytes.clone();
        for forbidden in [product(1), product(3)] {
            assert!(corrupt_later_scene(&mut bytes, forbidden).is_err());
            assert_eq!(bytes, original);
        }
        corrupt_later_scene(&mut bytes, product(2)).unwrap();
        let damaged = bytes.clone();
        assert!(corrupt_later_scene(&mut bytes, product(2)).is_err());
        assert_eq!(bytes, damaged);
    }

    #[test]
    fn catalog_corruption_keeps_the_header_and_all_payload_extents_unchanged() {
        let original = pack();
        let mut damaged = original.clone();
        let range = corrupt_catalog(&mut damaged).unwrap();
        assert_eq!(&damaged[..HEADER_BYTES], &original[..HEADER_BYTES]);
        assert_eq!(&damaged[range.end..], &original[range.end..]);
        assert_eq!(damaged.len(), original.len());
        assert!(inspect_pack(&damaged).is_err());
    }

    #[test]
    fn corruption_refuses_truncated_header_overflowing_catalog_and_wrong_pack_extents() {
        let original = pack();
        let mut overflowing = original.clone();
        overflowing[12..20].copy_from_slice(&u64::MAX.to_le_bytes());
        let mut trailing = original.clone();
        trailing.push(0);
        for mut invalid in [
            original[..19].to_vec(),
            original[..original.len() - 1].to_vec(),
            overflowing,
            trailing,
        ] {
            let before = invalid.clone();
            assert!(corrupt_later_scene(&mut invalid, product(2)).is_err());
            assert_eq!(invalid, before);
        }
    }
}
