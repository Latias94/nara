//! Native executable-relative adaptation for a compiled, independently expected Runtime pack.

use std::io;

use nara_fs::{FsError, FsOperation, RelativePath};

use crate::{
    ProductRecipe,
    runtime_content::{
        RuntimeContentBase, RuntimeContentError, RuntimeContentExpectation, RuntimeContentLimits,
    },
};

/// Opens one pack relative to the actual native executable, independent of the working directory.
///
/// The Player supplies `relative_pack` and `expected` from its compiled build input and uses its
/// own selected recipe. Neither a delivery manifest nor the pack being opened may supply its own
/// expected digest. Failure never falls back to authoring source, metadata, an importer or cache.
/// This path adapter requires no Cargo/project-tooling feature and grants no Runtime Ready or
/// native execution qualification. Acquisition retains the original opened file and plan.
pub fn open_executable_relative_runtime_pack(
    relative_pack: &RelativePath,
    expected: &RuntimeContentExpectation,
    recipe: ProductRecipe,
    limits: RuntimeContentLimits,
) -> Result<RuntimeContentBase, RuntimeContentError> {
    let executable = std::env::current_exe().map_err(native_error)?;
    let parent = executable.parent().ok_or_else(|| {
        native_error(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Player executable has no parent",
        ))
    })?;
    let directory =
        super::open_explicit_product_project_root(parent).map_err(RuntimeContentError::Host)?;
    let file = directory
        .open_file(relative_pack)
        .map_err(RuntimeContentError::Host)?;
    RuntimeContentBase::open_pack(file, expected, recipe, limits)
}

fn native_error(source: io::Error) -> RuntimeContentError {
    RuntimeContentError::Host(FsError::Io {
        operation: FsOperation::OpenDirectory,
        source,
    })
}
