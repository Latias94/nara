//! Exercise the production source-tool owner with real metadata-only project Sessions.

use super::*;

use nara_tooling::{
    EditorPropertyTarget, EditorSourceToolActivationId, EditorSourceToolError,
    EditorSourceToolIntent, EditorSourceToolRequestId,
};

mod child;
mod fixture;
mod lifecycle;
mod rejection;

use child::Output;
use fixture::{
    GatePoint, HeldProcess, Project, WAIT, activate, attach, catalog_json, component, document,
    proposal, snapshot, target, wait,
};
