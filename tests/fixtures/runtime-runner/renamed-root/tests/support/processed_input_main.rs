//! Source-free entry point: executable authority plus independently supplied content expectations.

#![forbid(unsafe_code)]

use std::{
    num::NonZeroU32,
    path::PathBuf,
    time::{Duration, Instant},
};

use engine::{
    app::FixedTime,
    diagnostic::DiagnosticValueRef,
    fs::ContentDigest,
    project_host::{
        HeadlessRun, HeadlessRunOptions, HeadlessRunOutcome, open_explicit_product_project_root,
    },
    runtime_content::{
        RuntimeContentBase, RuntimeContentError, RuntimeContentExpectation, RuntimeContentLimits,
        RuntimeContentTarget,
    },
};

mod processed_input_probe;
use processed_input_probe::{Case, Outcome};

fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let case = Case::parse(
        arguments
            .next()
            .expect("case")
            .to_str()
            .expect("UTF-8 case"),
    );
    let directory = PathBuf::from(arguments.next().expect("processed directory"));
    let length: u64 = arguments
        .next()
        .expect("catalog length")
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    let hash = arguments
        .next()
        .expect("catalog hash")
        .into_string()
        .unwrap();
    assert_eq!(hash.len(), 64);
    assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let mut digest = [0; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hash[index * 2..index * 2 + 2], 16).unwrap();
    }
    let target = arguments
        .next()
        .expect("expected target")
        .into_string()
        .unwrap();
    let profile = arguments
        .next()
        .expect("expected profile")
        .into_string()
        .unwrap();
    assert!(arguments.next().is_none());

    assert!(directory.is_absolute());
    let directory = directory.canonicalize().unwrap();
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        directory
    );
    let executable = std::env::current_exe().unwrap().canonicalize().unwrap();
    assert_eq!(executable.parent(), Some(directory.as_path()));
    let path = PathBuf::from(std::env::var_os("PATH").expect("deliberately unavailable PATH"));
    assert!(path.is_absolute() && !path.exists());
    for name in [
        "CARGO",
        "CARGO_HOME",
        "RUSTC",
        "RUSTUP_HOME",
        "HOME",
        "XDG_CONFIG_HOME",
        "LD_LIBRARY_PATH",
    ] {
        assert!(
            std::env::var_os(name).is_none(),
            "unexpected inherited {name}"
        );
    }
    for name in [
        "nara.toml",
        "Cargo.toml",
        "Cargo.lock",
        "schema",
        ".nara",
        "source",
    ] {
        assert!(
            !directory.join(name).exists(),
            "unexpected source input {name}"
        );
    }

    // None of these expected facts are read from catalog.json by this executable.
    let expected = RuntimeContentExpectation::new(
        ContentDigest::from_parts(length, digest),
        RuntimeContentTarget::new(target, profile).unwrap(),
    );
    let capability = open_explicit_product_project_root(&directory).unwrap();
    let captured = RuntimeContentBase::open(
        &capability,
        &expected,
        processed_input_probe::recipe(case),
        RuntimeContentLimits::default(),
    );
    drop(capability);
    let base = match captured {
        Ok(base) => {
            assert!(
                !matches!(
                    case,
                    Case::MissingInput | Case::WrongDigest | Case::WrongTarget
                ),
                "base admission accepted {case:?}",
            );
            base
        }
        Err(error) => {
            assert_eq!(processed_input_probe::startup_count(), 0);
            match case {
                Case::MissingInput => {
                    assert!(!directory.join(processed_input_probe::INPUT_FILE).exists());
                    assert!(matches!(error, RuntimeContentError::Host(_)), "{error:?}");
                }
                Case::WrongDigest => {
                    assert!(
                        matches!(error, RuntimeContentError::CatalogDigestMismatch),
                        "{error:?}"
                    );
                }
                Case::WrongTarget => {
                    assert!(
                        matches!(error, RuntimeContentError::TargetMismatch),
                        "{error:?}"
                    );
                }
                _ => panic!("unexpected base rejection for {case:?}: {error:?}"),
            }
            println!(
                "processed-input-process {} rejected-before-startup",
                case.name()
            );
            return;
        }
    };
    assert_eq!(
        base.plan().settings().input().action_map,
        Some(processed_input_probe::input_product()),
    );

    let options = HeadlessRunOptions::frames(
        NonZeroU32::new(20_000).unwrap(),
        FixedTime::DEFAULT_TIMESTEP,
    )
    .with_cleanup_timeout(Duration::from_secs(2))
    .stop_when(Outcome::finished);
    let mut run = HeadlessRun::<Outcome>::from_runtime_content(base, options, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(20);
    let report = loop {
        let report = run.execute_bounded();
        assert!(report.startup_content_closure().is_none());
        if !matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
            break report;
        }
        assert!(
            Instant::now() < deadline,
            "processed Runtime cleanup did not finish"
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    match case {
        Case::Physical => {
            assert!(
                !report.diagnostics().has_errors(),
                "{:?}",
                report.diagnostics()
            );
            let HeadlessRunOutcome::Completed(outcome) = report.into_outcome() else {
                panic!("the physical-input consumer did not complete")
            };
            outcome.assert_complete();
            println!(
                "processed-input-process physical ok starts=2/2 cancels=1/1 commits=2 rejected=1"
            );
        }
        Case::MalformedInput | Case::WrongIdentity | Case::ConflictingInput => {
            assert!(
                matches!(report.outcome(), HeadlessRunOutcome::Failed),
                "{report:?}"
            );
            assert!(report.diagnostics().has_errors());
            assert_eq!(processed_input_probe::startup_count(), 0);
            let (code, reason) = match case {
                Case::MalformedInput => (
                    "project.run.runtime-content-rejected",
                    Some("asset-decode-failed"),
                ),
                Case::WrongIdentity => (
                    "project.run.runtime-content-rejected",
                    Some("product-identity-mismatch"),
                ),
                Case::ConflictingInput => ("project.run.input-selection-rejected", None),
                _ => unreachable!(),
            };
            assert!(
                report.diagnostics().iter().any(|diagnostic| {
                    diagnostic.code().as_str() == code
                        && reason.is_none_or(|reason| {
                            diagnostic.fields().iter().any(|field| {
                                field.key().as_str() == "reason"
                                    && field.value() == DiagnosticValueRef::Identifier(reason)
                            })
                        })
                }),
                "unexpected startup refusal: {:?}",
                report.diagnostics(),
            );
            println!(
                "processed-input-process {} rejected-before-startup",
                case.name()
            );
        }
        _ => unreachable!(),
    }
}
