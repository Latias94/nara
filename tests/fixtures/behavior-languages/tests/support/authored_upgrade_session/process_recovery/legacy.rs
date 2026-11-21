use super::*;

const SCENE: &str = "dormant/scenes/closed.scene.ron";
const PREFAB: &str = "dormant/prefabs/closed.prefab.ron";

#[derive(serde::Deserialize)]
struct LegacyProject {
    directories: Vec<PathBuf>,
    files: BTreeMap<PathBuf, LegacyFile>,
    successors: BTreeMap<String, String>,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum LegacyFile {
    Text(String),
    Bytes(Vec<u8>),
}

impl LegacyProject {
    fn materialize(&self, root: &Path) {
        for directory in &self.directories {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        for (path, contents) in &self.files {
            let bytes = match contents {
                LegacyFile::Text(text) => text.as_bytes(),
                LegacyFile::Bytes(bytes) => bytes,
            };
            fs::write(root.join(path), bytes).unwrap();
        }
    }

    fn assert_stored_writers_differ(&self) {
        let scene = &self.successors[SCENE];
        let prefab = &self.successors[PREFAB];
        let scene_tree = SceneDocumentCandidate::decode_ron_str(scene)
            .unwrap()
            .into_composition_source()
            .unwrap();
        let prefab_tree = PrefabDocumentCandidate::decode_ron_str(prefab)
            .unwrap()
            .into_composition_source()
            .unwrap();
        assert_ne!(scene_tree.to_ron_string().unwrap(), *scene);
        assert_ne!(prefab_tree.to_ron_string().unwrap(), *prefab);
    }
}

#[test]
#[ignore = "requires NARA_AUTHORED_UPGRADE_RECOVERY_EXE built separately with project-workflow-cli only"]
fn subprocess_legacy_ron_pending_uses_original_reviewed_bytes() {
    let legacy: LegacyProject = serde_json::from_str(include_str!(
        "../../../data/authored-schema-upgrade-legacy-v1.json"
    ))
    .unwrap();
    assert_eq!(
        legacy
            .successors
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            PREFAB,
            SCENE,
            "nara.toml",
            super::PREFAB,
            LEAF,
            INACTIVE,
            STARTUP,
            "schema/authoring.bundle.json",
        ],
    );
    legacy.assert_stored_writers_differ();
    for interrupted in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        legacy.materialize(root);
        let original = snapshot(root);
        let inspection = succeeded(recovery(root, "inspect", &[]));
        assert_review_counts(&inspection, 6, 0, "original");
        assert_eq!(snapshot(root), original);
        assert_pending_open(root);

        let mut published = Vec::new();
        if interrupted {
            for member in [SCENE, PREFAB] {
                let path = root.join(member);
                fs::write(&path, &legacy.successors[member]).unwrap();
                published.push((
                    member_capability(&path),
                    fs::metadata(&path).unwrap().modified().unwrap(),
                    path,
                ));
            }
            let mixed = snapshot(root);
            let inspection = succeeded(recovery(root, "inspect", &[]));
            assert_review_counts(&inspection, 4, 2, "original");
            assert_eq!(snapshot(root), mixed);
        }

        let report = succeeded(recovery(root, "resume", &["--all"]));
        assert!(report.contains("ready confirmed=true sources=6 content_verified=true"));
        assert_eq!(manifest(root).authoring.state, ProjectAuthoringState::Ready);
        for (path, expected) in &legacy.successors {
            assert_eq!(
                fs::read(root.join(path)).unwrap(),
                expected.as_bytes(),
                "{path}"
            );
        }
        for (prior_handle, modified, path) in published {
            assert!(prior_handle.same_object(&member_capability(&path)).unwrap());
            assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), modified);
        }
        let ready = snapshot(root);
        for (profile, milli) in [("base", 109000), ("dormant", 19000)] {
            let reopened = succeeded(session_child("open-legacy-ready", root, profile));
            assert!(reopened.contains(&format!(
                "ordinary-open ready=true history=0/0 milli={milli}"
            )));
            assert_eq!(snapshot(root), ready);
        }
    }
}
