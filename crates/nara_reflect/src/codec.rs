//! Component decode and encode contracts.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_asset::{
    AssetRef, AssetRefError, AssetRefExportPolicy, AssetServer, AssetSourceKind, Handle,
    ProjectAssetDatabase,
};
use nara_ecs::{Entity, World};

use crate::{
    ComponentTypeId, ComponentValue, ComponentValueError,
    persistent_apply::{PersistentApplyRejection, PreparedComponentCandidate},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentCodecError {
    MissingField {
        field: String,
    },
    InvalidField {
        field: String,
        expected: String,
    },
    InvalidAssetRef {
        field: String,
        asset_ref: String,
        message: String,
    },
    WrongWorld,
    AssetServerChanged,
    EntityMissing,
    PreparedComponentTypeMismatch {
        expected: &'static str,
        actual: &'static str,
    },
    PersistentApplyReceiptMissing,
    PersistentApplyTargetNotEmpty,
    PersistentApplyBindingConflict {
        component_id: ComponentTypeId,
    },
    PersistentApplySupportRejected {
        reason: PersistentApplyRejection,
    },
    PersistentApplyRejected {
        component_id: ComponentTypeId,
        reason: PersistentApplyRejection,
    },
    Message(String),
}

impl ComponentCodecError {
    #[must_use]
    pub fn missing_field(field: impl Into<String>) -> Self {
        Self::MissingField {
            field: field.into(),
        }
    }

    #[must_use]
    pub fn invalid_field(field: impl Into<String>, expected: impl Into<String>) -> Self {
        Self::InvalidField {
            field: field.into(),
            expected: expected.into(),
        }
    }

    #[must_use]
    pub fn invalid_asset_ref(
        field: impl Into<String>,
        asset_ref: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::InvalidAssetRef {
            field: field.into(),
            asset_ref: asset_ref.into(),
            message: message.into(),
        }
    }
}

impl Display for ComponentCodecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField { field } => write!(formatter, "missing component field '{field}'"),
            Self::InvalidField { field, expected } => {
                write!(
                    formatter,
                    "invalid component field '{field}', expected {expected}"
                )
            }
            Self::InvalidAssetRef {
                field,
                asset_ref,
                message,
            } => {
                write!(
                    formatter,
                    "invalid asset reference in '{field}' ('{asset_ref}'): {message}"
                )
            }
            Self::WrongWorld => formatter.write_str("component batch belongs to a different world"),
            Self::AssetServerChanged => {
                formatter.write_str("asset server changed after the component batch was created")
            }
            Self::EntityMissing => formatter.write_str("target entity does not exist"),
            Self::PreparedComponentTypeMismatch { expected, actual } => write!(
                formatter,
                "prepared component Rust type '{actual}' does not match registered type '{expected}'"
            ),
            Self::PersistentApplyReceiptMissing => {
                formatter.write_str("persistent component apply receipt is missing")
            }
            Self::PersistentApplyTargetNotEmpty => formatter
                .write_str("persistent apply target must be empty before its receipt is declared"),
            Self::PersistentApplyBindingConflict { component_id } => write!(
                formatter,
                "persistent component '{component_id}' is already bound to a different runtime component"
            ),
            Self::PersistentApplySupportRejected { reason } => write!(
                formatter,
                "persistent apply support topology is ineligible: {reason}"
            ),
            Self::PersistentApplyRejected {
                component_id,
                reason,
            } => write!(
                formatter,
                "persistent component '{component_id}' is ineligible for target-World apply: {reason}"
            ),
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl Error for ComponentCodecError {}

impl From<ComponentValueError> for ComponentCodecError {
    fn from(error: ComponentValueError) -> Self {
        Self::Message(error.to_string())
    }
}

#[derive(Default)]
pub struct ComponentDecodeContext<'a> {
    asset_server: Option<&'a mut AssetServer>,
    project_asset_database: Option<&'a ProjectAssetDatabase>,
    asset_server_touched: bool,
}

impl<'a> ComponentDecodeContext<'a> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_asset_server(asset_server: &'a mut AssetServer) -> Self {
        Self {
            asset_server: Some(asset_server),
            project_asset_database: None,
            asset_server_touched: false,
        }
    }

    #[must_use]
    pub fn with_project_asset_database(mut self, database: &'a ProjectAssetDatabase) -> Self {
        self.project_asset_database = Some(database);
        self
    }

    #[must_use]
    pub const fn project_asset_database(&self) -> Option<&ProjectAssetDatabase> {
        self.project_asset_database
    }

    #[must_use]
    pub const fn asset_server_touched(&self) -> bool {
        self.asset_server_touched
    }

    pub fn resolve_asset_ref<T>(
        &mut self,
        asset_ref: &AssetRef,
    ) -> Option<Result<Handle<T>, AssetRefError>> {
        self.resolve_asset_ref_as(asset_ref, None)
    }

    pub fn resolve_asset_ref_with_kind<T>(
        &mut self,
        asset_ref: &AssetRef,
        expected_source_kind: &AssetSourceKind,
    ) -> Option<Result<Handle<T>, AssetRefError>> {
        self.resolve_asset_ref_as(asset_ref, Some(expected_source_kind))
    }

    pub fn validate_asset_ref_with_kind(
        &self,
        asset_ref: &AssetRef,
        expected_source_kind: &AssetSourceKind,
    ) -> Option<Result<(), AssetRefError>> {
        let database = self.project_asset_database?;
        Some(
            asset_ref
                .validate_with_database_as(database, expected_source_kind)
                .map(|_| ()),
        )
    }

    fn resolve_asset_ref_as<T>(
        &mut self,
        asset_ref: &AssetRef,
        expected_source_kind: Option<&AssetSourceKind>,
    ) -> Option<Result<Handle<T>, AssetRefError>> {
        let database = self.project_asset_database;
        let asset_server = self.asset_server.as_deref_mut()?;
        self.asset_server_touched = true;
        Some(match database {
            Some(database) => match expected_source_kind {
                Some(expected_source_kind) => {
                    asset_ref.resolve_with_database_as(asset_server, database, expected_source_kind)
                }
                None => asset_ref.resolve_with_database(asset_server, database),
            },
            None => asset_ref.resolve(asset_server),
        })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ComponentEncodeContext<'a> {
    asset_ref_export_policy: AssetRefExportPolicy,
    project_asset_database: Option<&'a ProjectAssetDatabase>,
}

impl<'a> ComponentEncodeContext<'a> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn asset_ref_export_policy(&self) -> AssetRefExportPolicy {
        self.asset_ref_export_policy
    }

    #[must_use]
    pub const fn with_asset_ref_export_policy(mut self, policy: AssetRefExportPolicy) -> Self {
        self.asset_ref_export_policy = policy;
        self
    }

    #[must_use]
    pub fn with_project_asset_database(mut self, database: &'a ProjectAssetDatabase) -> Self {
        self.project_asset_database = Some(database);
        self
    }

    #[must_use]
    pub const fn project_asset_database(&self) -> Option<&ProjectAssetDatabase> {
        self.project_asset_database
    }
}

pub trait ComponentCodec: Send + Sync {
    fn preflight(
        &self,
        value: &ComponentValue,
    ) -> Result<PreparedComponentCandidate, ComponentCodecError> {
        let mut context = ComponentDecodeContext::new();
        self.preflight_with_context(value, &mut context)
    }

    fn preflight_with_context(
        &self,
        value: &ComponentValue,
        context: &mut ComponentDecodeContext<'_>,
    ) -> Result<PreparedComponentCandidate, ComponentCodecError>;

    fn encode(
        &self,
        world: &World,
        entity: Entity,
    ) -> Result<Option<ComponentValue>, ComponentCodecError> {
        let context = ComponentEncodeContext::new();
        self.encode_with_context(world, entity, &context)
    }

    fn encode_with_context(
        &self,
        world: &World,
        entity: Entity,
        context: &ComponentEncodeContext<'_>,
    ) -> Result<Option<ComponentValue>, ComponentCodecError>;
}

pub(crate) struct FnComponentCodec<Preflight, Encode> {
    pub(crate) preflight: Preflight,
    pub(crate) encode: Encode,
}

impl<Preflight, Encode> ComponentCodec for FnComponentCodec<Preflight, Encode>
where
    Encode: for<'a> Fn(
            &World,
            Entity,
            &ComponentEncodeContext<'a>,
        ) -> Result<Option<ComponentValue>, ComponentCodecError>
        + Send
        + Sync
        + 'static,
    Preflight: for<'a> Fn(
            &ComponentValue,
            &mut ComponentDecodeContext<'a>,
        ) -> Result<PreparedComponentCandidate, ComponentCodecError>
        + Send
        + Sync
        + 'static,
{
    fn preflight_with_context(
        &self,
        value: &ComponentValue,
        context: &mut ComponentDecodeContext<'_>,
    ) -> Result<PreparedComponentCandidate, ComponentCodecError> {
        (self.preflight)(value, context)
    }

    fn encode_with_context(
        &self,
        world: &World,
        entity: Entity,
        context: &ComponentEncodeContext<'_>,
    ) -> Result<Option<ComponentValue>, ComponentCodecError> {
        (self.encode)(world, entity, context)
    }
}
