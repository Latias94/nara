use std::path::PathBuf;

use super::{ProjectToolSummary, ToolError, host_fs};
use crate::ProductRecipe;

pub(super) fn execute(
    project: PathBuf,
    profile: Option<String>,
    write: bool,
    recipe: ProductRecipe,
) -> Result<ProjectToolSummary, ToolError> {
    let root = if write {
        host_fs::writable_directory(&project)?
    } else {
        host_fs::readable_directory(&project)?
    };
    let written = crate::project_host::schema::execute(&root, profile.as_deref(), recipe, write)
        .map_err(ToolError::Schema)?;
    Ok(ProjectToolSummary::Schema { written })
}
