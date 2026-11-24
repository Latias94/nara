use nara_asset::AssetRef;

use super::{Compiler, MissingRootInput, SourceTopology};
use crate::{
    ResolvedPrefabSource, SceneCompilationFailure,
    prefab::{SceneCompilationContext, SceneCompilationWorkingSet},
};

#[cfg(test)]
mod tests;

pub(in crate::prefab::overlay::compile) fn validate_saved_source<F, E>(
    context: &mut SceneCompilationContext<'_, F, E>,
    source: SourceTopology<'_>,
) -> Result<(), SceneCompilationFailure>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    Compiler::start(context, source, None)?
        .close_value_programs(MissingRootInput::Defer)
        .map(drop)
}
