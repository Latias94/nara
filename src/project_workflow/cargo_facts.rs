//! Bounded, capability-free Cargo metadata and JSON-message facts.
//!
//! This Module deliberately owns no process, filesystem, or publication capability. Absolute paths
//! are retained only as transient invocation facts so the private owner can bind Cargo output before
//! copying the selected executable into its immutable generation.

use std::path::Path;

mod desktop_build;
mod messages;
mod metadata;

pub use desktop_build::{
    CargoArtifactMismatchField, CargoDesktopArtifactFacts, CargoDesktopBuildDecoder,
    CargoDesktopBuildError, CargoDesktopBuildFacts, CargoDesktopBuildInvocation,
    CargoDesktopInvocationError,
};
pub use messages::{
    CargoCompilerArtifactFacts, CargoCompilerDiagnostic, CargoDiagnosticLevel, CargoDiagnostics,
    CargoJsonMessage, CargoJsonMessageError,
};
pub use metadata::{
    CargoHeadlessMetadataFacts, CargoMetadataFacts, CargoMetadataFactsError, CargoPackageGraphFact,
    CargoPackageSourceKind,
};

const DESKTOP_TARGET: &str = "desktop";
const DESKTOP_FEATURE: &str = "desktop";
const DEV_PROFILE: &str = "dev";
const DEV_PROFILE_DIRECTORY: &str = "debug";

const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;
const MAX_PACKAGE_COUNT: usize = 8_192;
const MAX_TARGET_COUNT: usize = 512;
const MAX_FEATURE_COUNT: usize = 256;
const MAX_PACKAGE_ID_BYTES: usize = 4 * 1024;
const MAX_PACKAGE_NAME_BYTES: usize = 256;
const MAX_VERSION_BYTES: usize = 256;
const MAX_PATH_BYTES: usize = 128 * 1024;
const MAX_FEATURE_BYTES: usize = 256;
const MAX_HOST_TARGET_BYTES: usize = 256;

const MAX_JSON_STREAM_BYTES: usize = 64 * 1024 * 1024;
const MAX_JSON_RECORD_BYTES: usize = 4 * 1024 * 1024;
const MAX_JSON_RECORDS: usize = 131_072;
const MAX_DIAGNOSTIC_COUNT: usize = 256;
const MAX_DIAGNOSTIC_BYTES: usize = 256 * 1024;
const MAX_DIAGNOSTIC_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_DIAGNOSTIC_CODE_BYTES: usize = 256;

fn paths_match(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        fn normalized(path: &Path) -> String {
            let path = path.to_string_lossy();
            path.strip_prefix(r"\\?\")
                .unwrap_or(&path)
                .replace('/', "\\")
                .to_ascii_lowercase()
        }

        normalized(left) == normalized(right)
    }

    #[cfg(not(windows))]
    {
        left == right
    }
}

#[cfg(test)]
#[path = "cargo_facts/tests.rs"]
mod tests;
