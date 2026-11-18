#![forbid(unsafe_code)]

use std::{
    num::NonZeroU32,
    path::Path,
    time::{Duration, Instant},
};

use engine::{
    fs::{ContentDigest, RelativePath},
    project_host::{
        HeadlessRun, HeadlessRunOptions, HeadlessRunOutcome, open_executable_relative_runtime_pack,
    },
    runtime_content::{RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentTarget},
};
use palette_cooked_game::{self as game, probe::Outcome};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/nara-player-bootstrap.rs"));
}

fn main() {
    assert!(cfg!(feature = "player"));
    assert!(!cfg!(feature = "cook"));
    assert_eq!(std::env::args_os().len(), 1);
    assert!(!Path::new(env!("CARGO_MANIFEST_DIR")).exists());
    let executable = std::env::current_exe().unwrap().canonicalize().unwrap();
    let directory = executable.parent().unwrap();
    let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
    assert_ne!(cwd, directory);
    for name in [
        "PATH",
        "CARGO",
        "RUSTC",
        "RUSTUP_HOME",
        "CARGO_HOME",
        "HOME",
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "RUSTUP_TOOLCHAIN",
        "CARGO_TARGET_DIR",
        "NARA_CARGO_GENERATED_RUST_SOURCE",
    ] {
        assert!(
            std::env::var_os(name).is_none(),
            "inherited build/tool path: {name}"
        );
    }
    let mut members = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            entry.file_name().into_string().unwrap()
        })
        .collect::<Vec<_>>();
    let mut expected = vec![
        executable.file_name().unwrap().to_str().unwrap().to_owned(),
        generated::PACK_FILE.to_owned(),
        "package.json".to_owned(),
    ];
    members.sort();
    expected.sort();
    assert_eq!(members, expected);
    assert_eq!(generated::CONTENT_PROFILE, "palette-shipping");
    let expectation = RuntimeContentExpectation::new(
        ContentDigest::from_parts(generated::CATALOG_LENGTH, generated::CATALOG_HASH),
        RuntimeContentTarget::new(generated::CONTENT_TARGET, generated::CONTENT_PROFILE).unwrap(),
    );
    let base = open_executable_relative_runtime_pack(
        &RelativePath::new(generated::PACK_FILE).unwrap(),
        &expectation,
        game::recipe(),
        RuntimeContentLimits::default(),
    )
    .unwrap();
    assert_eq!(base.catalog().members().len(), 4);
    assert_eq!(base.catalog().scenes(), [game::scene_product()]);
    assert!(base.catalog().addressable_roots().is_empty());
    let options = HeadlessRunOptions::frames(NonZeroU32::new(2).unwrap(), Duration::ZERO)
        .with_cleanup_timeout(Duration::from_secs(2));
    let mut run = HeadlessRun::<Outcome>::from_runtime_content(base, options, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(10);
    let report = loop {
        let report = run.execute_bounded();
        assert!(report.startup_content_closure().is_none());
        if !matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
            break report;
        }
        assert!(Instant::now() < deadline, "Palette runtime cleanup deadline");
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(
        !report.diagnostics().has_errors(),
        "{:?}",
        report.diagnostics()
    );
    let HeadlessRunOutcome::Completed(outcome) = report.into_outcome() else {
        panic!("cooked Palette game did not finish");
    };
    assert_eq!(outcome.frames, 2);
    println!("palette-player ok frames=2 products=4 default-palette-pixels=24,120,220,255 source-free");
}
