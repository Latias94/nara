//! Engine-owned bounded task execution for nara.

mod runtime;

pub use runtime::*;

#[cfg(test)]
mod tests;
