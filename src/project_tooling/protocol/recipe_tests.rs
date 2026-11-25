use std::{error::Error, ffi::OsString, fmt};

use nara_app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};

use super::*;
use crate::{ProductRecipe, ProductRecipeError, SchemaContribution};

const PLUGIN: PluginId = PluginId::new("consumer.recipe");
const SCHEMA_PLUGIN: PluginId = PluginId::new("consumer.recipe-schema");
const PRIVATE: &str = "NARA_PRIVATE_SOURCE_CANARY";
const MAXIMUM_ID: &str = concat!(
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
);
const OVERSIZED_ID: &str = concat!(
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab",
    "b",
);
const OPERATIONS: [&str; 4] = [
    "check",
    "schema-check",
    "schema-write",
    "repair-startup-scene",
];

#[derive(Default)]
struct RecipePlugin;

impl Plugin for RecipePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PLUGIN, PluginCategory::Runtime);
        &DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[derive(Default)]
struct SchemaPlugin;

impl Plugin for SchemaPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(SCHEMA_PLUGIN, PluginCategory::Runtime)
                .provides_schema(&[nara_app::PluginSchemaProviderId::new(PRIVATE)]);
        &DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn roundtrip<E: 'static>(operation: &str, error: E) -> (ProjectToolFailure, Vec<u8>) {
    let error = ToolError::recipe(error);
    assert!(!format!("{error:?}").contains(PRIVATE));
    let expected = error.failure(operation);
    let response = encode_response(operation, Err(error)).unwrap();
    assert_eq!(response["version"], 1);
    assert_eq!(response["operation"], operation);
    assert_eq!(response["success"], false);
    let mut bytes = serde_json::to_vec(&response).unwrap();
    bytes.push(b'\n');
    assert!(bytes.len() <= failure::MAX_FAILURE_BYTES);
    let decoded = decode_response(&bytes, operation).unwrap().unwrap_err();
    assert_eq!(decoded, expected);
    assert_eq!(decoded.operation(), operation);
    assert_eq!(decoded.stage(), ProjectToolFailureStage::Recipe);
    assert!(decoded.repair_hint().is_none());
    (decoded, bytes)
}

#[test]
fn actual_duplicate_plugin_preserves_classification_and_admitted_plugin_in_every_operation() {
    let error = ProductRecipe::new()
        .add_plugin::<RecipePlugin>()
        .unwrap()
        .add_plugin::<RecipePlugin>()
        .unwrap_err();
    assert_eq!(
        error,
        ProductRecipeError::DuplicatePlugin { plugin: PLUGIN }
    );

    for operation in OPERATIONS {
        let (failure, _) = roundtrip(operation, error.clone());
        assert_eq!(failure.code(), "project.tool.recipe-duplicate-plugin");
        assert_eq!(failure.loss(), ProjectToolDiagnosticLoss::default());
        assert_eq!(
            failure.summary(),
            "A plugin appears more than once in the product recipe (plugin `consumer.recipe`)"
        );
        assert!(failure.diagnostics().is_empty());
        assert!(failure.to_string().contains(failure.summary()));
    }
}

#[test]
fn actual_schema_construction_errors_keep_distinct_reasons_and_withhold_provider_details() {
    let mismatch = SchemaContribution::<SchemaPlugin>::for_default([]).unwrap_err();
    let runtime = ProductRecipe::new()
        .add_plugin::<SchemaPlugin>()
        .unwrap_err();
    let empty = SchemaContribution::<RecipePlugin>::for_default([]).unwrap_err();
    for (error, code, omitted_fields) in [
        (mismatch, "project.tool.recipe-schema-provider-mismatch", 2),
        (
            runtime,
            "project.tool.recipe-runtime-plugin-provides-schema",
            0,
        ),
        (
            empty,
            "project.tool.recipe-schema-contribution-without-schema",
            0,
        ),
    ] {
        let (failure, bytes) = roundtrip("schema-write", error);
        assert_eq!(failure.code(), code);
        assert!(failure.summary().contains("(plugin `consumer.recipe"));
        assert!(failure.diagnostics().is_empty());
        assert_eq!(failure.loss().omitted_fields, omitted_fields);
        assert!(!String::from_utf8(bytes).unwrap().contains(PRIVATE));
        assert!(!format!("{failure:?}\n{failure}").contains(PRIVATE));
    }
}

#[test]
fn known_recipe_variants_have_distinct_static_codes_and_explicit_field_omissions() {
    use crate::ProductRecipeEntryKind;
    use ProductRecipeError as Recipe;
    use nara_app::PluginSchemaProviderId;
    use nara_reflect::PersistentComponentSchemaOwnerId;

    let mut codes = std::collections::BTreeSet::new();
    for (error, code, has_plugin, omitted_fields) in [
        #[cfg(feature = "render-wgpu")]
        (
            Recipe::WgpuPolicyRequiresStockCapture { plugin: PLUGIN },
            "project.tool.recipe-wgpu-policy-requires-stock-capture",
            true,
            0,
        ),
        #[cfg(feature = "render-wgpu")]
        (
            Recipe::SelectedWgpuPolicyConfiguration { plugin: PLUGIN },
            "project.tool.recipe-selected-wgpu-policy-configuration",
            true,
            0,
        ),
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        (
            Recipe::RuntimeCodec(
                crate::runtime_content::RuntimeCodecSelectionError::AmbiguousFormat {
                    format: PRIVATE,
                    version: u32::MAX,
                },
            ),
            "project.tool.recipe-runtime-codec-rejected",
            false,
            1,
        ),
        (
            Recipe::ReservedAssetDomain { kind: PRIVATE },
            "project.tool.recipe-reserved-asset-domain",
            false,
            1,
        ),
        (
            Recipe::DuplicateAssetDomain { kind: PRIVATE },
            "project.tool.recipe-duplicate-asset-domain",
            false,
            1,
        ),
        (
            Recipe::DuplicateDefaultConfiguration { plugin: PLUGIN },
            "project.tool.recipe-duplicate-default-configuration",
            true,
            0,
        ),
        (
            Recipe::DeclarationPanicked {
                plugin_type: PRIVATE,
            },
            "project.tool.recipe-declaration-panicked",
            false,
            1,
        ),
        (
            Recipe::MissingConfigurationIdentity { plugin: PLUGIN },
            "project.tool.recipe-missing-configuration-identity",
            true,
            0,
        ),
        (
            Recipe::DuplicatePlugin { plugin: PLUGIN },
            "project.tool.recipe-duplicate-plugin",
            true,
            0,
        ),
        (
            Recipe::PluginNotPresent { plugin: PLUGIN },
            "project.tool.recipe-plugin-not-present",
            true,
            0,
        ),
        (
            Recipe::PluginTypeMismatch { plugin: PLUGIN },
            "project.tool.recipe-plugin-type-mismatch",
            true,
            0,
        ),
        (
            Recipe::EntryKindMismatch {
                plugin: PLUGIN,
                expected: ProductRecipeEntryKind::RuntimePlugin,
                actual: ProductRecipeEntryKind::SchemaContribution,
            },
            "project.tool.recipe-entry-kind-mismatch",
            true,
            2,
        ),
        (
            Recipe::RuntimePluginProvidesSchema { plugin: PLUGIN },
            "project.tool.recipe-runtime-plugin-provides-schema",
            true,
            0,
        ),
        (
            Recipe::SchemaContributionWithoutSchema { plugin: PLUGIN },
            "project.tool.recipe-schema-contribution-without-schema",
            true,
            0,
        ),
        (
            Recipe::DuplicateDataOwner {
                plugin: PLUGIN,
                owner: PersistentComponentSchemaOwnerId::try_new(PRIVATE).unwrap(),
            },
            "project.tool.recipe-duplicate-data-owner",
            true,
            1,
        ),
        (
            Recipe::DataOwnerLimit {
                plugin: PLUGIN,
                maximum: usize::MAX,
            },
            "project.tool.recipe-data-owner-limit",
            true,
            1,
        ),
        (
            Recipe::SchemaProviderMismatch {
                plugin: PLUGIN,
                declared: vec![PluginSchemaProviderId::new(PRIVATE)],
                supplied: Vec::new(),
            },
            "project.tool.recipe-schema-provider-mismatch",
            true,
            2,
        ),
        (
            Recipe::DuplicateSchemaProvider {
                plugin: PLUGIN,
                provider: PluginSchemaProviderId::new(PRIVATE),
            },
            "project.tool.recipe-duplicate-schema-provider",
            true,
            1,
        ),
    ] {
        let (failure, bytes) = roundtrip("check", error);
        assert!(codes.insert(failure.code().to_owned()));
        assert_eq!(failure.code(), code);
        assert_eq!(
            failure.summary().contains("(plugin `consumer.recipe`)"),
            has_plugin
        );
        if has_plugin {
            assert!(failure.summary().len() + MAXIMUM_ID.len() - PLUGIN.as_str().len() <= 256);
        }
        assert!(failure.diagnostics().is_empty());
        assert_eq!(
            failure.loss(),
            ProjectToolDiagnosticLoss {
                omitted_fields,
                ..ProjectToolDiagnosticLoss::default()
            }
        );
        assert!(!String::from_utf8(bytes).unwrap().contains(PRIVATE));
        assert!(!format!("{failure:?}\n{failure}").contains(PRIVATE));
    }
}

#[test]
fn plugin_identifiers_use_existing_admission_without_truncating_identity() {
    for id in ["consumer.recipe", "Consumer/Plugin-v1_2", MAXIMUM_ID] {
        let (failure, _) = roundtrip(
            "check",
            ProductRecipeError::DuplicatePlugin {
                plugin: PluginId::new(id),
            },
        );
        assert_eq!(
            failure.summary(),
            format!("A plugin appears more than once in the product recipe (plugin `{id}`)")
        );
        assert_eq!(failure.loss(), ProjectToolDiagnosticLoss::default());
        assert!(failure.summary().len() <= 256);
    }
    for id in [
        "",
        OVERSIZED_ID,
        "C:\\private\\NARA_PRIVATE_SOURCE_CANARY",
        "/private/NARA_PRIVATE_SOURCE_CANARY",
        "consumer/../NARA_PRIVATE_SOURCE_CANARY",
        "https://private.example/NARA_PRIVATE_SOURCE_CANARY",
        "consumer\nNARA_PRIVATE_SOURCE_CANARY",
        "consumer\u{202e}private",
        "consumer.token",
        "consumer@private.example",
        "consumer=private",
        "consumer.\u{03bb}",
    ] {
        let (failure, bytes) = roundtrip(
            "check",
            ProductRecipeError::SchemaProviderMismatch {
                plugin: PluginId::new(id),
                declared: Vec::new(),
                supplied: Vec::new(),
            },
        );
        assert_eq!(
            failure.code(),
            "project.tool.recipe-schema-provider-mismatch"
        );
        assert_eq!(
            failure.summary(),
            "A schema contribution does not match its declared providers"
        );
        assert!(failure.diagnostics().is_empty());
        assert_eq!(failure.loss().omitted_fields, 3);
        assert_eq!(failure.loss().truncated_summaries, 0);
        assert!(!String::from_utf8(bytes).unwrap().contains(PRIVATE));
    }
}

#[test]
fn provider_vector_size_does_not_change_the_bounded_projection_or_field_loss() {
    let mut reference = None;
    for count in [0, 16_384] {
        let error = ToolError::recipe(ProductRecipeError::SchemaProviderMismatch {
            plugin: PLUGIN,
            declared: vec![nara_app::PluginSchemaProviderId::new(PRIVATE); count],
            supplied: vec![nara_app::PluginSchemaProviderId::new(PRIVATE); count],
        });
        assert!(!format!("{error:?}").contains(PRIVATE));
        let response = encode_response("check", Err(error)).unwrap();
        let mut bytes = serde_json::to_vec(&response).unwrap();
        bytes.push(b'\n');
        assert!(bytes.len() <= failure::MAX_FAILURE_BYTES);
        let failure = decode_response(&bytes, "check").unwrap().unwrap_err();
        assert_eq!(failure.loss().omitted_fields, 2);
        assert!(failure.summary().contains("(plugin `consumer.recipe`)"));
        assert!(failure.diagnostics().is_empty());
        assert!(!String::from_utf8_lossy(&bytes).contains(PRIVATE));
        if let Some(reference) = &reference {
            assert_eq!(&bytes, reference);
        } else {
            reference = Some(bytes);
        }
    }
}

struct PanickingError;

impl fmt::Display for PanickingError {
    fn fmt(&self, _formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("opaque Display must not be invoked")
    }
}

impl fmt::Debug for PanickingError {
    fn fmt(&self, _formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("opaque Debug must not be invoked")
    }
}

impl Error for PanickingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        panic!("opaque source must not be invoked")
    }
}

fn assert_opaque<E: 'static>(error: E) {
    let (failure, bytes) = roundtrip("check", error);
    assert_eq!(failure.code(), "project.tool.recipe-failed");
    assert_eq!(failure.summary(), "Project recipe construction failed");
    assert!(failure.diagnostics().is_empty());
    assert_eq!(failure.loss(), ProjectToolDiagnosticLoss::default());
    assert!(!String::from_utf8(bytes).unwrap().contains(PRIVATE));
    assert!(!format!("{failure:?}\n{failure}").contains(PRIVATE));
}

#[test]
fn opaque_errors_never_need_error_traits_or_invoke_formatters_or_sources() {
    struct ArbitraryError;

    assert_opaque(ArbitraryError);
    assert_opaque(PRIVATE.to_owned());
    assert_opaque(std::io::Error::other(PRIVATE));
    assert_opaque(PanickingError);
    assert_opaque(Box::new(ProductRecipeError::DuplicatePlugin {
        plugin: PLUGIN,
    }));
    assert_opaque((ProductRecipeError::DuplicatePlugin { plugin: PLUGIN },));
    let erased: Box<dyn Error> = Box::new(PanickingError);
    assert_opaque(erased);
    let projected = ToolError::recipe(PanickingError);
    assert!(!format!("{projected:?}\n{projected}").contains(PRIVATE));
}

#[test]
fn gameplay_does_not_invoke_the_factory_even_with_an_opaque_error_type() {
    assert_eq!(
        crate::project_tooling::run_project_tool_mode(
            [OsString::from("--max-ticks"), OsString::from("1")],
            || -> Result<ProductRecipe, PanickingError> {
                panic!("gameplay must skip the factory")
            },
        ),
        None
    );
}

#[test]
fn normal_success_shapes_and_operation_association_remain_canonical_protocol_one() {
    for (operation, summary) in [
        ("check", ProjectToolSummary::Checked { content_members: 3 }),
        (
            "schema-check",
            ProjectToolSummary::Schema { written: false },
        ),
        ("schema-write", ProjectToolSummary::Schema { written: true }),
        (
            "repair-startup-scene",
            ProjectToolSummary::Repaired {
                replacement: "level.scene".to_owned(),
            },
        ),
    ] {
        let response = encode_response(operation, Ok(summary.clone())).unwrap();
        let mut bytes = serde_json::to_vec(&response).unwrap();
        bytes.push(b'\n');
        assert_eq!(response["version"], 1);
        assert_eq!(decode_response(&bytes, operation), Ok(Ok(summary)));
        let other = if operation == "check" {
            "schema-check"
        } else {
            "check"
        };
        assert!(decode_response(&bytes, other).is_err());
        let (_, failure) = roundtrip(
            operation,
            ProductRecipeError::DuplicatePlugin { plugin: PLUGIN },
        );
        assert!(decode_response(&failure, other).is_err());
    }
}
