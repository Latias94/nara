//! Component decode and encode contracts.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use crate::{
    ComponentDomainError, ComponentTypeId, ComponentValueError,
    ComponentValueMaterializationLimits, persistent_apply::PersistentApplyRejection,
};
use nara_asset::{
    AssetPathScope, AssetRef, AssetRefError, AssetRefExportPolicy, AssetReferenceResolver,
    AssetServer, AssetSourceKind, Handle,
};

static ROOT_ASSET_PATH_SCOPE: AssetPathScope = AssetPathScope::ROOT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentCodecError {
    UnknownComponent {
        component_id: ComponentTypeId,
    },
    MissingNativeBinding {
        component_id: ComponentTypeId,
    },
    Domain(ComponentDomainError),
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
    /// The first expanded-value node count rejected before materialization.
    MaterializationNodeLimit {
        observed: usize,
        maximum: usize,
    },
    /// The first expanded-value logical byte count rejected before materialization.
    MaterializationByteLimit {
        observed: usize,
        maximum: usize,
    },
    /// The first expanded-value depth rejected before materialization.
    MaterializationDepthLimit {
        observed: usize,
        maximum: usize,
    },
    WrongWorld,
    AssetServerChanged,
    EntityMissing,
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
    pub fn domain(error: ComponentDomainError) -> Self {
        Self::Domain(error)
    }

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
            Self::UnknownComponent { component_id } => {
                write!(
                    formatter,
                    "component '{component_id}' is not in the frozen schema"
                )
            }
            Self::MissingNativeBinding { component_id } => {
                write!(
                    formatter,
                    "component '{component_id}' has no native binding"
                )
            }
            Self::Domain(error) => Display::fmt(error, formatter),
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
            Self::MaterializationNodeLimit { observed, maximum } => write!(
                formatter,
                "effective component value materialization observed {observed} nodes, maximum is {maximum}"
            ),
            Self::MaterializationByteLimit { observed, maximum } => write!(
                formatter,
                "effective component value materialization observed {observed} logical bytes, maximum is {maximum}"
            ),
            Self::MaterializationDepthLimit { observed, maximum } => write!(
                formatter,
                "effective component value materialization observed depth {observed}, maximum is {maximum}"
            ),
            Self::WrongWorld => formatter.write_str("component batch belongs to a different world"),
            Self::AssetServerChanged => {
                formatter.write_str("asset server changed after the component batch was created")
            }
            Self::EntityMissing => formatter.write_str("target entity does not exist"),
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

impl From<ComponentDomainError> for ComponentCodecError {
    fn from(error: ComponentDomainError) -> Self {
        Self::Domain(error)
    }
}

impl From<ComponentValueError> for ComponentCodecError {
    fn from(error: ComponentValueError) -> Self {
        Self::Message(error.to_string())
    }
}

#[derive(Default)]
pub struct ComponentDecodeContext<'a> {
    asset_server: Option<&'a mut AssetServer>,
    asset_resolution: Option<AssetReferenceResolver<'a>>,
    asset_path_scope: Option<&'a AssetPathScope>,
    asset_server_touched: bool,
    materialization_limits: ComponentValueMaterializationLimits,
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
            asset_resolution: None,
            asset_path_scope: None,
            asset_server_touched: false,
            materialization_limits: ComponentValueMaterializationLimits::default(),
        }
    }

    #[must_use]
    pub const fn materialization_limits(&self) -> ComponentValueMaterializationLimits {
        self.materialization_limits
    }

    pub fn set_materialization_limits(&mut self, limits: ComponentValueMaterializationLimits) {
        self.materialization_limits = limits;
    }

    #[must_use]
    pub const fn with_materialization_limits(
        mut self,
        limits: ComponentValueMaterializationLimits,
    ) -> Self {
        self.materialization_limits = limits;
        self
    }

    #[must_use]
    pub const fn with_asset_resolution(mut self, resolver: AssetReferenceResolver<'a>) -> Self {
        self.asset_resolution = Some(resolver);
        self
    }

    #[must_use]
    pub const fn asset_resolution(&self) -> Option<AssetReferenceResolver<'a>> {
        self.asset_resolution
    }

    /// Binds direct Path references when no source catalogue is selected.
    #[must_use]
    pub const fn with_asset_path_scope(mut self, scope: &'a AssetPathScope) -> Self {
        self.asset_path_scope = Some(scope);
        self
    }

    /// A catalogue's admitted scope always takes precedence over the standalone scope.
    #[must_use]
    pub fn asset_path_scope(&self) -> &'a AssetPathScope {
        self.asset_resolution.map_or_else(
            || self.asset_path_scope.unwrap_or(&ROOT_ASSET_PATH_SCOPE),
            |resolver| resolver.scope(),
        )
    }

    #[must_use]
    pub const fn asset_server_touched(&self) -> bool {
        self.asset_server_touched
    }

    pub fn resolve_asset_ref<T: 'static>(
        &mut self,
        asset_ref: &AssetRef,
    ) -> Option<Result<Handle<T>, AssetRefError>> {
        self.resolve_asset_ref_as(asset_ref, None)
    }

    pub fn resolve_asset_ref_with_kind<T: 'static>(
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
        let resolver = self.asset_resolution?;
        Some(
            asset_ref
                .validate_with_database_as(resolver, expected_source_kind)
                .map(|_| ()),
        )
    }

    fn resolve_asset_ref_as<T: 'static>(
        &mut self,
        asset_ref: &AssetRef,
        expected_source_kind: Option<&AssetSourceKind>,
    ) -> Option<Result<Handle<T>, AssetRefError>> {
        let resolver = self.asset_resolution;
        let scope = self.asset_path_scope();
        let asset_server = self.asset_server.as_deref_mut()?;
        self.asset_server_touched = true;
        Some(match resolver {
            Some(resolver) => match expected_source_kind {
                Some(expected_source_kind) => {
                    asset_ref.resolve_with_database_as(asset_server, resolver, expected_source_kind)
                }
                None => asset_ref.resolve_with_database(asset_server, resolver),
            },
            None => asset_ref.resolve(asset_server, scope),
        })
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ComponentEncodeContext<'a> {
    asset_ref_export_policy: AssetRefExportPolicy,
    asset_resolution: Option<AssetReferenceResolver<'a>>,
    asset_path_scope: Option<&'a AssetPathScope>,
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
    pub const fn with_asset_resolution(mut self, resolver: AssetReferenceResolver<'a>) -> Self {
        self.asset_resolution = Some(resolver);
        self
    }

    #[must_use]
    pub const fn asset_resolution(&self) -> Option<AssetReferenceResolver<'a>> {
        self.asset_resolution
    }

    /// Binds handle-to-Path export when no source catalogue is selected.
    #[must_use]
    pub const fn with_asset_path_scope(mut self, scope: &'a AssetPathScope) -> Self {
        self.asset_path_scope = Some(scope);
        self
    }

    #[must_use]
    pub fn asset_path_scope(&self) -> &'a AssetPathScope {
        self.asset_resolution.map_or_else(
            || self.asset_path_scope.unwrap_or(&ROOT_ASSET_PATH_SCOPE),
            |resolver| resolver.scope(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::{
        AssetProductRef, AssetRecord, AssetSourcePath, ImportedProductId, ProjectAssetDatabase,
        StableAssetId,
    };

    #[test]
    fn contexts_use_the_admitted_scope_for_path_stable_and_product_selectors() {
        let assets = AssetPathScope::under(AssetSourcePath::new("game/art").unwrap());
        let other = AssetPathScope::under(AssetSourcePath::new("package/art").unwrap());
        let source: StableAssetId = "11111111-1111-4111-8111-111111111111".parse().unwrap();
        let foreign: StableAssetId = "22222222-2222-4222-8222-222222222222".parse().unwrap();
        let mut database = ProjectAssetDatabase::default();
        for (id, path) in [
            (source, "game/art/shared.png"),
            (foreign, "package/art/shared.png"),
        ] {
            database
                .insert(AssetRecord::new(
                    id,
                    AssetSourcePath::new(path).unwrap(),
                    AssetSourceKind::Image,
                ))
                .unwrap();
        }
        let path = AssetRef::path("shared.png").unwrap();
        let product = AssetProductRef::new(source, ImportedProductId::PRIMARY).unwrap();
        let mut server = AssetServer::new();
        let handle = {
            let mut context = ComponentDecodeContext::with_asset_server(&mut server)
                .with_asset_path_scope(&other)
                .with_asset_resolution(database.resolver(&assets));
            assert!(std::ptr::eq(
                context.asset_resolution().unwrap().database(),
                &database
            ));
            assert_eq!(context.asset_path_scope(), &assets);
            let handle = context.resolve_asset_ref::<String>(&path).unwrap().unwrap();
            for reference in [AssetRef::StableId(source), AssetRef::Product(product)] {
                assert_eq!(
                    context
                        .resolve_asset_ref::<String>(&reference)
                        .unwrap()
                        .unwrap(),
                    handle
                );
            }
            handle
        };
        let encode = ComponentEncodeContext::new()
            .with_asset_resolution(database.resolver(&assets))
            .with_asset_path_scope(&other);
        assert_eq!(
            AssetRef::from_handle(&server, handle, encode.asset_path_scope()).unwrap(),
            path
        );
        let cross_root =
            ComponentEncodeContext::new().with_asset_resolution(database.resolver(&other));
        assert_eq!(
            AssetRef::from_handle(&server, handle, cross_root.asset_path_scope()).unwrap(),
            AssetRef::Product(product)
        );
        let mut foreign_context = ComponentDecodeContext::with_asset_server(&mut server)
            .with_asset_resolution(database.resolver(&other));
        assert_ne!(
            foreign_context
                .resolve_asset_ref::<String>(&path)
                .unwrap()
                .unwrap(),
            handle
        );
    }
}
