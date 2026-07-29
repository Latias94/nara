use std::{collections::BTreeSet, fs, path::Path};

use nara::{
    fs::DirectoryCapability,
    gameplay::GameplayCommandSubmission,
    prelude::Resource,
    project_host::{HeadlessRun, HeadlessRunIntent},
};

#[derive(Clone, Resource)]
struct ProductOutcome;

const ALLOWED_RUNTIME_EXPORTS: [&str; 8] = [
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
    "configure",
    "diagnostics",
    "disable",
    "execute",
    "execute_bounded",
    "insert_after",
    "insert_before",
    "into_outcome",
    "new",
    "outcome",
    "stop_when",
    "with_cleanup_timeout",
    "with_control_flow",
    "with_profile",
    "with_schema_provider",
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
        "pubuseruntime::{DesktopRun,DesktopRunIntent,DesktopRunOutcome,DesktopRunReport};"
    ));

    let runtime_source = read(root.join("src/project_host/runtime/action.rs"));
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
        "bundled_wave_run_with_completed_tick_observer",
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
