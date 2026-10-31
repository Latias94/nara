//! Cook real source files, remove their entire owner, then run only relocated processed content.
//! The fixed child is built by Cargo before this test; this test never starts a build tool.

#![cfg(any(windows, target_os = "linux"))]
#![forbid(unsafe_code)]

use std::fs;

use engine::runtime_content::RuntimeContentTarget;

#[path = "support/processed_input_probe.rs"]
#[allow(dead_code)]
mod probe;
#[path = "support/processed_input_fixture.rs"]
mod processed_input_fixture;

#[test]
fn prebuilt_player_uses_only_processed_input_and_scenes_after_source_removal() {
    // This is a native fixture label, not a target-build selector or cross-build attestation.
    let target = RuntimeContentTarget::new(
        format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        "processed-input-process",
    )
    .unwrap();
    let executable_name = format!("processed_input_player{}", std::env::consts::EXE_SUFFIX);
    processed_input_fixture::run_cases(&executable_name, &target, |executable| {
        fs::copy(env!("CARGO_BIN_EXE_processed_input_player"), executable).unwrap();
    });
}
