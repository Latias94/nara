use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use two_level_game::{command::SemanticCommand, replay::GameProgram};

pub struct ProjectCopy {
    temporary_parent: PathBuf,
    directory: PathBuf,
    project: PathBuf,
}

impl ProjectCopy {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary_parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        let directory = temporary_parent.join(format!(
            "two-level-game-replay-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let directory = fs::canonicalize(directory).unwrap();
        assert_eq!(directory.parent(), Some(temporary_parent.as_path()));
        let copy = Self {
            project: directory.join("project"),
            temporary_parent,
            directory,
        };
        for relative in [
            "nara.toml",
            "input/explorer.actions.json",
            "scenes/meadow.scene.json",
            "scenes/cavern.scene.json",
            "scenes/marker.scene.json",
            "scenes/marker.scene.json.meta",
            "scenes/side-room.scene.json",
            "scenes/side-room.scene.json.meta",
            "prefabs/marker.prefab.json",
            "assets/textures/meadow.png",
            "assets/textures/meadow.png.meta",
            "assets/textures/cavern.png",
            "assets/textures/cavern.png.meta",
            "assets/textures/explorer.png",
            "assets/textures/explorer.png.meta",
            "assets/textures/marker.png",
            "assets/textures/marker.png.meta",
            "assets/textures/side-room.png",
            "assets/textures/side-room.png.meta",
            "assets/animations/explorer.sprite-animations.json",
            "assets/animations/explorer.sprite-animations.json.meta",
            "assets/audio/game.soundbank.json",
            "assets/audio/game.soundbank.json.meta",
            "assets/audio/pickup.wav",
            "assets/audio/travel.wav",
            "schema/authoring.bundle.json",
            "prefabs/README.md",
        ] {
            let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
            let destination = copy.project.join(relative);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(source, destination).unwrap();
        }
        assert_ne!(
            fs::canonicalize(&copy.project).unwrap(),
            fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap()
        );
        copy
    }

    pub fn root(&self) -> &Path {
        &self.project
    }
}

impl Drop for ProjectCopy {
    fn drop(&mut self) {
        let Ok(directory) = fs::canonicalize(&self.directory) else {
            return;
        };
        if directory == self.directory
            && directory.parent() == Some(self.temporary_parent.as_path())
            && directory != fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap()
        {
            let _ = fs::remove_dir_all(directory);
        }
    }
}

pub fn full_program() -> GameProgram {
    let mut steps = Vec::new();
    walk_right(&mut steps);
    steps.extend([
        vec![SemanticCommand::Marker],
        vec![SemanticCommand::Move {
            direction: 1,
            pressed: false,
        }],
        vec![],
        vec![SemanticCommand::Travel],
    ]);
    walk_right(&mut steps);
    steps.extend([
        vec![SemanticCommand::Marker],
        vec![SemanticCommand::Instance],
    ]);
    walk_right(&mut steps);
    steps.extend([
        vec![SemanticCommand::Marker],
        vec![SemanticCommand::Instance],
        vec![SemanticCommand::Retry],
        vec![SemanticCommand::Travel],
        vec![],
    ]);
    GameProgram::new(steps).expect("the complete game trajectory fits the public program profile")
}

fn walk_right(steps: &mut Vec<Vec<SemanticCommand>>) {
    steps.push(vec![SemanticCommand::Move {
        direction: 1,
        pressed: true,
    }]);
    steps.extend((1..20).map(|_| Vec::new()));
}
