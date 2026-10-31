//! Candidate-local resource admission and deterministic serial compilation.

mod model;

pub use model::{AccessRole, GraphError, GraphLimits, LimitKind, TextureAlpha, TextureTransfer};
pub(crate) use model::{
    BufferMeaning, CompiledGraph, GraphBuilder, GraphOperation, Initialization, ResourceDescriptor,
    ResourceRead, ResourceVersion, ResourceWrite, TextureMeaning,
};

#[cfg(test)]
mod tests;
