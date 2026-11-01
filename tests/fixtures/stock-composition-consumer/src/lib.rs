#![forbid(unsafe_code)]

//! Independent stock-stage composition, shared by ordinary Runtime and Editor consumers.
//!
//! The blur and grading implementations are independent Cargo packages. This crate selects
//! typed ports and graph edges; stock extraction, Canvas mapping and UI lowering stay with Nara.

mod collector;
mod composition;

/// Predetermined, format-aware numeric expectations shared with native owner tests.
#[path = "../tests/support/oracle.rs"]
pub mod oracle;
/// Declared typed GPU readback shared with native owner tests; submission stays Host-owned.
#[path = "../tests/support/readback.rs"]
pub mod readback;

pub use collector::{CompositionPlugin, RetainedInputs};
pub use composition::{
    CompositionBindings, CompositionCapture, CompositionConfiguration, CompositionPolicy,
    CompositionPorts, EffectSpace, SelectedPorts, recipe, recipe_with_inspection,
};
pub use nara_stock_grade_effect::GradeParameters;
