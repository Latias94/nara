use std::{collections::BTreeSet, fs, path::Path};

use nara::{
    fs::DirectoryCapability,
    gameplay::GameplayCommandSubmission,
    prelude::Resource,
    project_host::{HeadlessRun, HeadlessRunIntent},
};

#[derive(Clone, Resource)]
struct ProductOutcome;

const ALLOWED_RUNTIME_EXPORTS: [&str; 9] = [
    "DesktopPresentationVerificationReceipt",
    "DesktopRun",
    "DesktopRunIntent",
    "DesktopRunOutcome",
    "DesktopRunReport",
    "HeadlessRun",
    "HeadlessRunIntent",
    "HeadlessRunOutcome",
    "HeadlessRunReport",
];

const ALLOWED_PUBLIC_METHODS: [&str; 15] = [
    "diagnostics",
    "execute",
    "execute_bounded",
    "from_recipe",
    "into_outcome",
    "new",
    "outcome",
    "presentation_verification",
    "startup_content_closure",
    "stop_when",
    "with_cleanup_timeout",
    "with_control_flow",
    "with_first_present_verification",
    "with_profile",
    "with_recipe",
];

const FORBIDDEN_CALLER_CONCEPTS: [&str; 19] = [
    "App",
    "ReadyRuntimeCandidate",
    "RuntimeAdmissionFailure",
    "RuntimeCandidate",
    "RuntimeCandidateFailure",
    "RuntimeCloseParticipant",
    "RuntimeConstructionFailure",
    "RuntimeInstance",
    "RuntimeObligationLedger",
    "RuntimePlan",
    "RuntimePreparationRetirement",
    "RuntimeRetirement",
    "RuntimeStartAttempt",
    "SealedApp",
    "World",
    "begin_retirement",
    "complete_startup",
    "drive_retirement",
    "promote",
];

#[test]
fn concrete_product_action_exposes_only_the_reviewed_carriers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let host_source = read(root.join("src/project_host.rs"));
    let compact_host = host_source.split_whitespace().collect::<String>();
    assert!(compact_host.contains(
        "pubuseruntime::{HeadlessRun,HeadlessRunIntent,HeadlessRunOutcome,HeadlessRunReport};"
    ));
    assert!(compact_host.contains(
        "pubuseruntime::{DesktopPresentationVerificationReceipt,DesktopRun,DesktopRunIntent,DesktopRunOutcome,DesktopRunReport,};"
    ));

    let runtime_source = format!(
        "{}\n{}",
        read(root.join("src/project_host/runtime/action.rs")),
        read(root.join("src/project_host/runtime/desktop_verification.rs"))
    );
    let mut public_types = BTreeSet::new();
    let mut public_methods = BTreeSet::new();
    for line in runtime_source.lines() {
        let line = line.trim();
        if let Some(declaration) = line
            .strip_prefix("pub struct ")
            .or_else(|| line.strip_prefix("pub enum "))
        {
            public_types.insert(identifier(declaration));
            continue;
        }
        if let Some(declaration) = line
            .strip_prefix("pub fn ")
            .or_else(|| line.strip_prefix("pub const fn "))
        {
            public_methods.insert(identifier(declaration));
            continue;
        }
        assert!(
            !line.starts_with("pub "),
            "project runtime exposes an unreviewed item: {line}"
        );
    }

    assert_eq!(public_types, ALLOWED_RUNTIME_EXPORTS.into_iter().collect());
    assert_eq!(public_methods, ALLOWED_PUBLIC_METHODS.into_iter().collect());
}

#[test]
fn advanced_runtime_probe_is_single_definition_and_has_no_provider_or_edit_channel() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = read(root.join("src/project_host/advanced.rs"));
    let syntax = syn::parse_file(&source).expect("advanced project Host source must parse");
    let mut fields = BTreeSet::new();
    let mut methods = BTreeSet::new();

    for item in syntax.items {
        match item {
            syn::Item::Struct(item) if item.ident == "RuntimeProbe" => {
                for field in item.fields {
                    fields.insert(
                        field
                            .ident
                            .expect("RuntimeProbe fields are named")
                            .to_string(),
                    );
                }
            }
            syn::Item::Impl(item_impl) => {
                if item_impl.trait_.is_some() {
                    continue;
                }
                let syn::Type::Path(self_ty) = item_impl.self_ty.as_ref() else {
                    continue;
                };
                if !self_ty
                    .path
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "RuntimeProbe")
                {
                    continue;
                }
                for item in item_impl.items {
                    if let syn::ImplItem::Fn(method) = item {
                        methods.insert(method.sig.ident.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    assert_eq!(fields, BTreeSet::from(["definition".to_owned()]));
    assert_eq!(
        methods,
        BTreeSet::from(["into_overlay".to_owned(), "new".to_owned()])
    );
    for forbidden in [
        "configure",
        "disable",
        "insert_after",
        "insert_before",
        "provider",
        "with_schema_provider",
    ] {
        assert!(
            !methods.contains(forbidden),
            "RuntimeProbe exposes forbidden authority {forbidden}"
        );
    }
}

#[test]
fn ordinary_product_intents_have_one_recipe_composition_authority() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert_intent_surface(
        &read(root.join("src/project_host/runtime/action.rs")),
        "HeadlessRunIntent",
        &[
            "cleanup_policy",
            "fixed_ticks",
            "profile",
            "recipe",
            "terminal_predicate",
        ],
        &[
            "new",
            "stop_when",
            "with_cleanup_timeout",
            "with_profile",
            "with_recipe",
        ],
    );
    assert_intent_surface(
        &read(root.join("src/project_host/runtime/action.rs")),
        "DesktopRunIntent",
        &[
            "cleanup_policy",
            "profile",
            "recipe",
            "runner",
            "verify_first_present",
        ],
        &[
            "new",
            "with_cleanup_timeout",
            "with_control_flow",
            "with_first_present_verification",
            "with_profile",
            "with_recipe",
        ],
    );
}

#[test]
fn ordinary_reference_binary_names_only_product_level_concepts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = read(root.join("reference-game/src/bin/headless.rs"));
    let identifiers = source_identifiers(&source);

    for forbidden in FORBIDDEN_CALLER_CONCEPTS {
        assert!(
            !identifiers.contains(forbidden),
            "ordinary caller names internal lifecycle concept {forbidden}"
        );
    }
    for required in [
        "HeadlessRunOutcome",
        "bundled_wave_product_run_with_completed_tick_observer",
        "diagnostics",
        "execute_bounded",
        "open_project_root",
    ] {
        assert!(
            identifiers.contains(required),
            "ordinary caller does not exercise product concept {required}"
        );
    }
}

#[test]
fn ordinary_desktop_binary_names_only_product_level_concepts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = read(root.join("reference-game/src/bin/desktop.rs"));
    let identifiers = source_identifiers(&source);

    for forbidden in FORBIDDEN_CALLER_CONCEPTS {
        assert!(
            !identifiers.contains(forbidden),
            "ordinary desktop caller names internal lifecycle concept {forbidden}"
        );
    }
    for required in [
        "DesktopRunOutcome",
        "bundled_desktop_run",
        "diagnostics",
        "execute",
    ] {
        assert!(
            identifiers.contains(required),
            "ordinary desktop caller does not exercise product concept {required}"
        );
    }
}

#[test]
fn headless_run_constructor_requires_an_owned_command_buffer() {
    type Constructor = fn(
        DirectoryCapability,
        HeadlessRunIntent<ProductOutcome>,
        Vec<GameplayCommandSubmission>,
    ) -> HeadlessRun<ProductOutcome>;

    let constructor: Constructor = HeadlessRun::new;
    let _ = constructor;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = read(root.join("src/project_host/runtime/action.rs"));
    let syntax = syn::parse_file(&source).expect("project Host runtime source must parse");
    let constructor = syntax
        .items
        .iter()
        .filter_map(|item| {
            let syn::Item::Impl(item_impl) = item else {
                return None;
            };
            let syn::Type::Path(self_ty) = item_impl.self_ty.as_ref() else {
                return None;
            };
            if !self_ty
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "HeadlessRun")
            {
                return None;
            }
            item_impl.items.iter().find_map(|item| match item {
                syn::ImplItem::Fn(method) if method.sig.ident == "new" => Some(method),
                _ => None,
            })
        })
        .next()
        .expect("HeadlessRun must expose one constructor");

    assert!(
        constructor.sig.generics.params.is_empty()
            && constructor.sig.generics.where_clause.is_none(),
        "HeadlessRun::new must not generalize command admission through method generics"
    );
    assert_eq!(
        constructor.sig.inputs.len(),
        3,
        "HeadlessRun::new must retain the reviewed three-argument product action"
    );
    let commands = constructor
        .sig
        .inputs
        .iter()
        .nth(2)
        .and_then(|input| match input {
            syn::FnArg::Typed(argument) => Some(argument),
            syn::FnArg::Receiver(_) => None,
        })
        .expect("the third constructor argument must be the command buffer");
    let syn::Pat::Ident(pattern) = commands.pat.as_ref() else {
        panic!("the command buffer argument must have a stable name");
    };
    assert_eq!(pattern.ident, "commands");
    assert_vec_of_gameplay_submissions(commands.ty.as_ref());
}

fn assert_vec_of_gameplay_submissions(ty: &syn::Type) {
    let syn::Type::Path(commands) = ty else {
        panic!("the command buffer must be a concrete path type");
    };
    assert_eq!(commands.path.segments.len(), 1);
    let vec = commands.path.segments.last().unwrap();
    assert_eq!(vec.ident, "Vec");
    let syn::PathArguments::AngleBracketed(arguments) = &vec.arguments else {
        panic!("the command buffer must identify its bounded item type");
    };
    assert_eq!(arguments.args.len(), 1);
    let Some(syn::GenericArgument::Type(syn::Type::Path(item))) = arguments.args.first() else {
        panic!("the command buffer must contain one concrete item type");
    };
    assert_eq!(item.path.segments.len(), 1);
    assert_eq!(
        item.path.segments.last().unwrap().ident,
        "GameplayCommandSubmission"
    );
}

fn assert_intent_surface(
    source: &str,
    target: &str,
    expected_fields: &[&str],
    expected_methods: &[&str],
) {
    let syntax = syn::parse_file(source).expect("project Host intent source must parse");
    let mut fields = BTreeSet::new();
    let mut methods = BTreeSet::new();

    for item in syntax.items {
        match item {
            syn::Item::Struct(item) if item.ident == target => {
                for field in item.fields {
                    fields.insert(field.ident.expect("intent fields are named").to_string());
                }
            }
            syn::Item::Impl(item_impl) => {
                let syn::Type::Path(self_ty) = item_impl.self_ty.as_ref() else {
                    continue;
                };
                if !self_ty
                    .path
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == target)
                {
                    continue;
                }
                for item in item_impl.items {
                    if let syn::ImplItem::Fn(method) = item
                        && matches!(method.vis, syn::Visibility::Public(_))
                    {
                        methods.insert(method.sig.ident.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    assert_eq!(
        fields,
        expected_fields.iter().map(ToString::to_string).collect()
    );
    assert_eq!(
        methods,
        expected_methods.iter().map(ToString::to_string).collect()
    );
}

fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn identifier(declaration: &str) -> &str {
    declaration
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .next()
        .expect("a public declaration starts with an identifier")
}

fn source_identifiers(source: &str) -> BTreeSet<&str> {
    source
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|identifier| !identifier.is_empty())
        .collect()
}
