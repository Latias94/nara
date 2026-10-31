//! Shared cooking and seven-case process oracle for independently supplied native Player bytes.

use std::{
    fmt::Write as _,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use engine::{
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetPathScope, AssetSourceKind,
        ProjectAssetDatabase, RuntimeAssetCodec,
    },
    core::{FormatKind, FormatVersion},
    fs::ContentDigest,
    input::{
        ActionBinding, ActionContext, ActionId, ActionMapAsset, ActionMapBuilder,
        ActionMapFileCandidate, ActionMapLimits, ActionMapRuntimeCodec,
    },
    project::{ProductCapability, RuntimeConfiguration, RuntimePreset},
    project_host::resolve_runtime_configuration,
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeContentCatalog, RuntimeContentCatalogDefinition,
        RuntimeContentCatalogLimits, RuntimeContentExpectation, RuntimeContentMember,
        RuntimeContentMemberKind, RuntimeContentTarget,
    },
    scene::{
        ExpandedSceneCandidate, RuntimeSceneCandidate, RuntimeSceneLimits, SceneDocumentCandidate,
        SourceDocumentKey, SourceDocumentKind,
    },
};

use super::probe::{self, Case};

const PREFIX: &str = "nara-processed-input-process-";
const SCENES: [&str; 3] = ["a.runtime.json", "b.runtime.json", "bad.runtime.json"];

pub fn run_cases(executable_name: &str, target: &RuntimeContentTarget, install: impl Fn(&Path)) {
    let fixture = Fixture::new();
    // Each child has one finite purpose; all builds precede this serial execution loop.
    for case in Case::ALL {
        let case_root = fixture.root.join(case.name());
        fs::create_dir(&case_root).unwrap();
        let source = case_root.join("source");
        let player = case_root.join("player");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&player).unwrap();
        let expected = cook(&source, &player, case, target);
        let executable = player.join(executable_name);
        install(&executable);
        fixture.remove_owned_directory(&source);
        assert!(!source.exists());
        if case == Case::MissingInput {
            fs::remove_file(player.join(probe::INPUT_FILE)).unwrap();
        }
        assert_payload(&player, executable_name, case);
        launch(&case_root, &player, &executable, &expected, case);
        assert!(
            !source.exists(),
            "the child must not recreate authoring inputs"
        );
        assert_payload(&player, executable_name, case);
    }
}

fn cook(
    source: &Path,
    player: &Path,
    case: Case,
    target: &RuntimeContentTarget,
) -> RuntimeContentExpectation {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    builder.declare_context(probe::menu(), false).unwrap();
    builder
        .bind(ActionBinding::key(
            ActionId::new(probe::MOVE).unwrap(),
            probe::KEY,
        ))
        .unwrap();
    builder
        .bind(ActionBinding::mouse(
            ActionId::new(probe::FIRE).unwrap(),
            probe::MOUSE,
        ))
        .unwrap();
    let action_source = source.join("player.actions.json");
    let source_map =
        ActionMapFileCandidate::new(probe::input_product(), builder.compile().unwrap())
            .unwrap()
            .to_json_string()
            .unwrap();
    fs::write(&action_source, source_map).unwrap();

    let mut configuration = RuntimeConfiguration::default();
    configuration.runtime.preset = RuntimePreset::LocalHeadless;
    configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
    configuration.tasks.io.workers = 1;
    configuration.tasks.compute.workers = 1;
    configuration.tasks.async_compute.workers = 1;
    configuration.input.action_map = Some(probe::input_product());
    let plan =
        resolve_runtime_configuration(&configuration.resolve().unwrap(), probe::recipe(case))
            .unwrap();
    assert_eq!(
        plan.settings().input().action_map,
        Some(probe::input_product())
    );
    fs::create_dir(source.join("schema")).unwrap();
    fs::write(
        source.join("schema/authoring.bundle.json"),
        plan.schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    fs::create_dir_all(source.join(".nara/import-cache")).unwrap();
    fs::write(
        source.join(".nara/import-cache/disposable"),
        b"source-only cache",
    )
    .unwrap();
    fs::write(
        source.join("nara.toml"),
        concat!(
            "schema_version = 1\n[project]\nname = 'Processed Input fixture'\n",
            "source_universe = '75e12840-0000-4000-8000-000000000099'\n",
            "native_document_identity = 'stable_metadata'\n",
            "[paths]\nassets = 'assets'\nscenes = 'scenes'\nprefabs = 'prefabs'\n",
            "[startup]\ndefault_scene = '1.scene.json'\n",
            "[content]\nscenes = ['2.scene.json', '3.scene.json']\n",
            "[input]\naction_map = 'player.actions.json'\n",
            "[authoring]\nschema_bundle = 'schema/authoring.bundle.json'\n",
            "[runtime]\npreset = 'local-headless'\n",
            "[capabilities]\nrequested = ['runtime-2d']\n",
        ),
    )
    .unwrap();
    for path in ["assets", "scenes", "prefabs"] {
        fs::create_dir(source.join(path)).unwrap();
    }

    // The actual saved source reader and move-only definition feed the concrete Runtime codec.
    let candidate =
        ActionMapFileCandidate::decode_json_bytes(&fs::read(action_source).unwrap()).unwrap();
    let (product, definition) = candidate.into_parts();
    let identity = if case == Case::WrongIdentity {
        probe::product(11)
    } else {
        product
    };
    let asset = ActionMapAsset::new(identity, definition).unwrap();
    let mut input = ActionMapRuntimeCodec::encode_runtime(&asset)
        .unwrap()
        .into_vec();
    if case == Case::MalformedInput {
        input = br#"{"kind":"runtime_action_map"}"#.to_vec();
    }
    fs::write(player.join(probe::INPUT_FILE), &input).unwrap();
    let mut members = vec![
        RuntimeContentMember::new(
            probe::INPUT_FILE,
            ContentDigest::of_bytes(&input),
            RuntimeContentMemberKind::Asset {
                format: FormatKind::new(ActionMapRuntimeCodec::RUNTIME_KIND).unwrap(),
                version: FormatVersion::ONE,
            },
            vec![probe::input_product()],
            vec![],
        )
        .unwrap(),
    ];
    let database = ProjectAssetDatabase::default();
    for (index, path) in SCENES.into_iter().enumerate() {
        let scene = index as u8 + 1;
        let source_name = format!("{scene}.scene.json");
        let source_path = source.join("scenes").join(&source_name);
        fs::write(
            &source_path,
            probe::document(scene).to_json_string().unwrap(),
        )
        .unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                probe::product(scene).source(),
                AssetPath::new(&source_name).unwrap(),
                AssetSourceKind::Scene,
            ),
            (),
        )
        .to_json_string()
        .unwrap();
        fs::write(
            source.join("scenes").join(format!("{source_name}.meta")),
            metadata,
        )
        .unwrap();
        let document =
            SceneDocumentCandidate::decode_json_bytes(&fs::read(source_path).unwrap()).unwrap();
        let document = document.into_composition_source().unwrap();
        let key = SourceDocumentKey::stable_asset(
            probe::product(scene).source(),
            SourceDocumentKind::Scene,
        );
        let expanded = ExpandedSceneCandidate::plain(key.clone(), Arc::new(document));
        let mut encoded = RuntimeSceneCandidate::cook(
            &expanded,
            plan.schema_validation().bindings(),
            database.resolver(&AssetPathScope::ROOT),
            RuntimeSceneLimits::default(),
        )
        .unwrap()
        .to_json_bytes()
        .unwrap();
        if scene == 3 {
            // The catalog digest is correct; only a later acquisition must reject this payload.
            encoded = b"{invalid Runtime Scene".to_vec();
        }
        fs::write(player.join(path), &encoded).unwrap();
        members.push(
            RuntimeContentMember::new(
                path,
                ContentDigest::of_bytes(&encoded),
                RuntimeContentMemberKind::Scene { source: key },
                vec![probe::product(scene)],
                vec![],
            )
            .unwrap(),
        );
    }
    let catalog = RuntimeContentCatalog::new(
        RuntimeContentCatalogDefinition {
            target: target.clone(),
            configuration,
            plugin_plan_fingerprint: plan.plugin_plan().fingerprint().as_bytes(),
            schema_fingerprint: plan
                .schema_validation()
                .composition_fingerprint()
                .as_bytes(),
            runtime_codecs_fingerprint: plan.runtime_codecs_fingerprint().as_bytes(),
            startup_scene: probe::product(1),
            scenes: vec![probe::product(1), probe::product(2), probe::product(3)],
            addressable_roots: vec![],
            members,
        },
        RuntimeContentCatalogLimits::default(),
    )
    .unwrap();
    let encoded = catalog.to_json_bytes().unwrap();
    let expected =
        RuntimeContentExpectation::new(ContentDigest::of_bytes(&encoded), target.clone());
    fs::write(player.join(RUNTIME_CONTENT_CATALOG_FILE), encoded).unwrap();
    expected
}

fn assert_payload(player: &Path, executable: &str, case: Case) {
    let mut actual: Vec<_> = fs::read_dir(player)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            entry.file_name().into_string().unwrap()
        })
        .collect();
    let mut expected = vec![
        executable.to_owned(),
        RUNTIME_CONTENT_CATALOG_FILE.to_owned(),
    ];
    expected.extend(SCENES.map(str::to_owned));
    if case != Case::MissingInput {
        expected.push(probe::INPUT_FILE.to_owned());
    }
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}

fn launch(
    case_root: &Path,
    player: &Path,
    executable: &Path,
    expected: &RuntimeContentExpectation,
    case: Case,
) {
    let mut hash = *expected.catalog_digest().as_bytes();
    if case == Case::WrongDigest {
        hash[0] ^= 1;
    }
    let mut encoded_hash = String::with_capacity(64);
    for byte in hash {
        write!(&mut encoded_hash, "{byte:02x}").unwrap();
    }
    let stdout_path = case_root.join("stdout.log");
    let stderr_path = case_root.join("stderr.log");
    let missing_path = case_root.join("unavailable-PATH");
    assert!(!missing_path.exists());
    let mut command = Command::new(executable);
    command
        .env_clear()
        .env("PATH", &missing_path)
        .current_dir(player)
        .arg(case.name())
        .arg(player)
        .arg(expected.catalog_digest().length().to_string())
        .arg(encoded_hash)
        .arg(if case == Case::WrongTarget {
            "wrong-native-target"
        } else {
            expected.target().triple()
        })
        .arg(expected.target().profile())
        .stdin(Stdio::null())
        .stdout(File::create(&stdout_path).unwrap())
        .stderr(File::create(&stderr_path).unwrap());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
        for name in ["SystemRoot", "WINDIR"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
    }
    let mut child = command
        .spawn()
        .expect("the relocated native Player must launch with normal platform libraries");
    drop(command);
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            result => {
                let killed = child.kill();
                let reaped = child.wait();
                panic!(
                    "{case:?}: child watchdog: poll={result:?}, kill={killed:?}, wait={reaped:?}",
                );
            }
        }
    };
    drop(child);
    for path in [&stdout_path, &stderr_path] {
        assert!(
            fs::metadata(path).unwrap().len() <= 64 * 1024,
            "fixture output must be finite",
        );
    }
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    assert!(
        status.success(),
        "{case:?}: {status}\nstdout:\n{stdout}\nstderr:\n{stderr}",
    );
    let exact = if case == Case::Physical {
        "processed-input-process physical ok starts=2/2 cancels=1/1 commits=2 rejected=1\n"
            .to_owned()
    } else {
        format!(
            "processed-input-process {} rejected-before-startup\n",
            case.name()
        )
    };
    assert_eq!(stdout, exact, "{case:?}; stderr:\n{stderr}");
}

struct Fixture {
    temporary: PathBuf,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = temporary.join(format!("{PREFIX}{}-{unique}", std::process::id()));
        fs::create_dir(&root).unwrap();
        Self {
            temporary,
            root: root.canonicalize().unwrap(),
        }
    }

    fn remove_owned_directory(&self, path: &Path) {
        let resolved = path.canonicalize().unwrap();
        assert_eq!(self.root.parent(), Some(self.temporary.as_path()));
        assert!(
            self.root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(PREFIX)
        );
        assert!(resolved == self.root || resolved.starts_with(&self.root));
        fs::remove_dir_all(&resolved).unwrap();
        assert!(!path.exists());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.root.exists() {
            self.remove_owned_directory(&self.root);
        }
    }
}
