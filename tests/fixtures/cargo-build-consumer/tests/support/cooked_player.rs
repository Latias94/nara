//! Source-free native Player. Only generated, compiled constants authorize the opened content.

#![forbid(unsafe_code)]

use std::{
    num::NonZeroU32,
    path::PathBuf,
    time::{Duration, Instant},
};

use aurora_game::{self as game, probe::Outcome};
use engine::{
    app::FixedTime,
    fs::{ContentDigest, RelativePath},
    project_host::{
        HeadlessRun, HeadlessRunOptions, HeadlessRunOutcome, open_executable_relative_runtime_pack,
    },
    runtime_content::{RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentTarget},
};

mod generated;

fn main() {
    assert!(!cfg!(feature = "unwanted-default"));
    assert!(cfg!(feature = "player-entry") && cfg!(feature = "quality"));
    assert_eq!(
        std::env::args_os().len(),
        1,
        "no CLI expectation override exists"
    );
    let executable = std::env::current_exe().unwrap().canonicalize().unwrap();
    let directory = executable.parent().unwrap();
    let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
    assert_ne!(
        cwd, directory,
        "the test must qualify executable-relative lookup"
    );
    let path = PathBuf::from(std::env::var_os("PATH").unwrap());
    assert!(path.is_absolute() && !path.exists());
    for variable in [
        "CARGO",
        "RUSTC",
        "RUSTUP_HOME",
        "CARGO_HOME",
        "HOME",
        "LD_LIBRARY_PATH",
    ] {
        assert!(
            std::env::var_os(variable).is_none(),
            "unexpected inherited {variable}"
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
    members.sort();
    let mut expected_files = vec![
        executable.file_name().unwrap().to_str().unwrap().to_owned(),
        generated::PACK_FILE.to_owned(),
        "package.json".to_owned(),
    ];
    expected_files.sort();
    assert_eq!(members, expected_files);

    // Neither the test process, package manifest nor the catalog supplies these expected facts.
    let expected = RuntimeContentExpectation::new(
        ContentDigest::from_parts(generated::CATALOG_LENGTH, generated::CATALOG_HASH),
        RuntimeContentTarget::new(generated::CONTENT_TARGET, generated::CONTENT_PROFILE).unwrap(),
    );
    assert_eq!(generated::CONTENT_PROFILE, "cpu4-shipping");
    let pack_path = RelativePath::new(generated::PACK_FILE).unwrap();
    let base = open_executable_relative_runtime_pack(
        &pack_path,
        &expected,
        game::recipe(),
        RuntimeContentLimits::default(),
    )
    .unwrap();
    assert_eq!(base.catalog().members().len(), 7);
    assert_eq!(
        base.plan().settings().input().action_map,
        Some(game::product(game::INPUT))
    );
    let options = HeadlessRunOptions::frames(
        NonZeroU32::new(30_000).unwrap(),
        FixedTime::DEFAULT_TIMESTEP,
    )
    .with_cleanup_timeout(Duration::from_secs(2))
    .stop_when(Outcome::finished);
    let mut run = HeadlessRun::<Outcome>::from_runtime_content(base, options, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(40);
    let report = loop {
        let report = run.execute_bounded();
        assert!(report.startup_content_closure().is_none());
        if !matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
            break report;
        }
        assert!(Instant::now() < deadline, "Runtime cleanup deadline");
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(
        !report.diagnostics().has_errors(),
        "{:?}",
        report.diagnostics()
    );
    let HeadlessRunOutcome::Completed(outcome) = report.into_outcome() else {
        panic!("processed game did not finish")
    };
    outcome.assert_complete();
    println!(
        "cooked-player ok physical-starts=2/2 cancels=1/1 commits={} rejected={} source-free",
        outcome.commits, outcome.rejections
    );
}
