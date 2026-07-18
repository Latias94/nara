use std::{collections::BTreeSet, fs, path::Path};

const ALLOWED_RUNTIME_EXPORTS: [&str; 4] = [
    "HeadlessRun",
    "HeadlessRunIntent",
    "HeadlessRunOutcome",
    "HeadlessRunReport",
];

const ALLOWED_PUBLIC_METHODS: [&str; 12] = [
    "configure",
    "diagnostics",
    "disable",
    "execute_bounded",
    "insert_after",
    "insert_before",
    "into_outcome",
    "new",
    "outcome",
    "with_cleanup_timeout",
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
        "DirectoryCapability",
        "HeadlessRunOutcome",
        "diagnostics",
        "execute_bounded",
        "project_headless_run",
    ] {
        assert!(
            identifiers.contains(required),
            "ordinary caller does not exercise product concept {required}"
        );
    }
}

#[test]
fn external_consumer_compiles_with_the_product_concepts() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/fixtures/project-host-boundary/ordinary_product_action.rs");
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
