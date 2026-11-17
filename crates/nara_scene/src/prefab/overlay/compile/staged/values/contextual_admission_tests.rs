//! Actual staged compilation must admit masked defaults against its original work allowance.

use std::convert::Infallible;

use nara_core::{ByteLimit, DepthLimit};
use nara_diagnostic::{DiagnosticReport, DiagnosticValueRef};
use nara_identity::EntityReference;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
    ComponentValue, ComponentValueShape, PersistentComponentSchemaOwnerId,
};

use super::*;
use crate::prefab::{SceneCompilationContext, SceneCompilationLimits, SceneCompilationUsage};
use crate::schema_authority::SchemaAuthority;
use crate::{
    __private::AuthoringAdapterReceipt, AuthoringSchemaBasis, SceneCompilationOptions,
    SceneDocument, SceneEntityRecord,
};

const PAYLOAD_ITEMS: usize = 8_192;
const PRIOR_NODES: usize = 7;
const PRIOR_BYTES: usize = 11;
const PRIOR_SCRATCH: usize = 13;

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.CompilerContextualAdmission")
}

fn field() -> ComponentFieldId {
    ComponentFieldId::new("payload")
}

fn empty_list() -> ComponentValue {
    ComponentValue::List(Vec::new())
}

fn null_list() -> ComponentValue {
    ComponentValue::List(vec![ComponentValue::Null; PAYLOAD_ITEMS])
}

fn long_reference() -> ComponentValue {
    ComponentValue::List(vec![ComponentValue::EntityReference(
        EntityReference::SceneLocal {
            entity: SceneEntityId::new("x".repeat(PAYLOAD_ITEMS)).unwrap(),
        },
    )])
}

fn output_limits() -> SceneCompilationLimits {
    SceneCompilationLimits::default()
        .with_materialized_value_nodes(ItemLimit::new(2).unwrap())
        .with_materialized_value_bytes(ByteLimit::new("payload".len()).unwrap())
}

struct Fixture {
    basis: AuthoringSchemaBasis,
    source: SceneDocument,
}

impl Fixture {
    fn new(canonical: ComponentValue, contextual: Option<ComponentValue>) -> Self {
        Self::with_root(canonical, contextual, "payload")
    }

    fn with_root(
        canonical: ComponentValue,
        contextual: Option<ComponentValue>,
        root: &str,
    ) -> Self {
        let capabilities = ComponentCapability::SCENE_AUTHORING;
        let schema = ComponentSchema::new(
            component(),
            "Compiler contextual admission",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(capabilities)
        .with_fields([ComponentFieldSchema::optional_with_default(
            field(),
            "Payload",
            ComponentFieldPath::from_fields([root]),
            ComponentValueShape::list(ComponentValueShape::optional(
                ComponentValueShape::EntityRef,
            )),
            canonical,
        )
        .with_capabilities(capabilities)
        .with_capability(ComponentCapability::EntityRef)]);
        let owner = PersistentComponentSchemaOwnerId::try_new("test.compiler-contextual").unwrap();
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner.clone(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        let adapter =
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap();
        let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
        let mut record = SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([(root, empty_list())]),
        )
        .with_owner(owner);
        if let Some(contextual) = contextual {
            record = record.with_contextual_default(field(), contextual);
        }
        let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("root").unwrap(),
        )
        .with_component(component(), record)]);
        Self { basis, source }
    }

    fn compile(&self, limits: SceneCompilationLimits) -> Compilation {
        let mut loader = |_: &AssetRef,
                          _: &mut SceneCompilationWorkingSet|
         -> Result<Option<ResolvedPrefabSource>, Infallible> {
            panic!("masked defaults must not acquire source demand");
        };
        let mut context = SceneCompilationContext::new(
            SchemaAuthority::Authoring(&self.basis),
            &mut loader,
            SceneCompilationOptions::default().with_limits(limits),
            None,
        );
        // Exercise the suspended owner's original ledger, including already consumed storage.
        context
            .admit_terminal_work(PRIOR_NODES, PRIOR_BYTES, Some(PRIOR_SCRATCH))
            .unwrap();
        let result = crate::prefab::overlay::compile::expand_source(
            &mut context,
            &self.source.placements,
            &self.source.entities,
            None,
        );
        if let Err(failure) = &result {
            context.push_expansion_failure(failure);
        }
        Compilation {
            result,
            usage: context.usage,
            reference_work: context.reference_work,
            diagnostics: context.diagnostics,
            budget_failed: context.budget_failed,
        }
    }
}

#[derive(Debug)]
struct Compilation {
    result: Result<SceneDraft, SceneCompilationFailure>,
    usage: SceneCompilationUsage,
    reference_work: (usize, usize),
    diagnostics: DiagnosticReport,
    budget_failed: bool,
}

fn assert_effective_empty(report: &Compilation) {
    let document = report
        .result
        .as_ref()
        .expect("actual compilation must succeed");
    assert!(!report.budget_failed, "{report:?}");
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    assert_eq!(document.entities.len(), 1);
    let record = &document.entities[0].components[&component()];
    assert_eq!(
        record.value(),
        &ComponentValue::map([("payload", empty_list())])
    );
    assert!(record.contextual_defaults.is_empty());
    assert_eq!(report.usage.materialized_components, 1);
    assert_eq!(report.usage.materialized_value_nodes, 2);
    assert_eq!(report.usage.materialized_value_bytes, "payload".len());
}

fn assert_budget(report: &Compilation, kind: &str, maximum: usize) {
    assert_eq!(
        report.result.as_ref().unwrap_err().reason(),
        &SceneCompilationFailureKind::WorkLimit,
        "{report:?}"
    );
    assert!(report.budget_failed);
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded")
        .expect("the original compiler must retain its quota diagnostic");
    for (name, value) in [
        ("budget-kind", DiagnosticValueRef::Identifier(kind)),
        ("maximum", DiagnosticValueRef::Unsigned(maximum as u64)),
    ] {
        assert!(
            diagnostic
                .fields()
                .iter()
                .any(|field| field.key().as_str() == name && field.value() == value),
            "{diagnostic:?}"
        );
    }
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == "observed"
            && matches!(field.value(), DiagnosticValueRef::Unsigned(value) if value > maximum as u64)
    }));
}

#[test]
fn masked_canonical_reference_default_stops_before_program_scratch() {
    let fixture = Fixture::new(null_list(), None);
    let before = fixture.source.clone();
    let report =
        fixture.compile(output_limits().with_reference_work_nodes(ItemLimit::new(128).unwrap()));
    assert_budget(&report, "reference-work-nodes", 128);
    assert_eq!(report.reference_work.0, 129);
    assert_eq!(report.usage.materialized_value_nodes, 0);
    assert_eq!(report.usage.materialized_value_bytes, 0);
    // Neither the hidden default nor the conservative three-copy program scratch was copied.
    assert!(report.usage.scratch_bytes < PAYLOAD_ITEMS * 96);
    assert_eq!(fixture.source, before);
}

#[test]
fn large_explicit_values_admit_actual_reference_work_without_a_depth_multiplier() {
    for with_reference in [false, true] {
        let mut payload = vec![ComponentValue::Null; PAYLOAD_ITEMS];
        if with_reference {
            payload[PAYLOAD_ITEMS / 2] =
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: SceneEntityId::new("root").unwrap(),
                });
        }
        let value = ComponentValue::map([("payload", ComponentValue::List(payload))]);
        let mut fixture = Fixture::new(empty_list(), None);
        fixture.source.entities[0]
            .components
            .get_mut(&component())
            .unwrap()
            .set_value(value.clone());
        let before = fixture.source.clone();
        let limits = SceneCompilationLimits::default()
            .with_working_bytes(ByteLimit::new(8 * 1024 * 1024).unwrap());
        let report = fixture.compile(limits);
        let document = report
            .result
            .as_ref()
            .expect("bounded complete reference planning must fit");
        assert!(
            !report.budget_failed && !report.diagnostics.has_errors(),
            "{report:?}"
        );
        assert_eq!(
            document.entities[0].components[&component()].value(),
            &value
        );
        assert!(std::ptr::eq(
            document.entities[0].components[&component()]
                .value()
                .as_value()
                .unwrap(),
            fixture.source.entities[0].components[&component()].value(),
        ));
        assert_eq!(report.usage.materialized_value_nodes, PAYLOAD_ITEMS + 2);
        assert!(report.usage.checked_work_bytes().unwrap() <= limits.working_bytes().get());
        assert!(report.reference_work.0 >= PAYLOAD_ITEMS);
        assert_budget(
            &fixture.compile(limits.with_reference_work_nodes(ItemLimit::new(64).unwrap())),
            "reference-work-nodes",
            64,
        );
        assert_eq!(fixture.source, before);
    }
}

#[test]
fn persistent_reference_visits_keep_long_path_storage_in_the_original_quota() {
    use nara_identity::{
        PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
    };

    let root = "route".repeat(2048);
    let reference = ComponentValue::EntityReference(EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("test.reference-visits").unwrap(),
            PersistentRuntimeId::parse_str("34567890-3456-4456-8456-345678901abc").unwrap(),
        ),
    });
    let value = ComponentValue::map([(root.as_str(), ComponentValue::List(vec![reference; 16]))]);
    let mut fixture = Fixture::with_root(empty_list(), None, &root);
    fixture.source.entities[0]
        .components
        .get_mut(&component())
        .unwrap()
        .set_value(value.clone());
    let before = fixture.source.clone();
    let defaults = SceneCompilationLimits::default();
    assert_budget(
        &fixture.compile(defaults),
        "reference-work-bytes",
        defaults.reference_work_bytes().get(),
    );
    let limits = defaults.with_reference_work_bytes(ByteLimit::new(usize::MAX).unwrap());
    let observed = fixture.compile(limits);
    assert_eq!(
        observed.result.as_ref().unwrap().entities[0].components[&component()].value(),
        &value
    );
    assert!(
        !observed.budget_failed && !observed.diagnostics.has_errors(),
        "{observed:?}"
    );
    let working = observed.usage.checked_work_bytes().unwrap();
    assert!(observed.usage.scratch_bytes >= root.len() * 16 * 2 * 3);
    let exact = limits.with_working_bytes(ByteLimit::new(working).unwrap());
    let fits = fixture.compile(exact);
    assert_eq!(fits.result, observed.result);
    assert_eq!(fits.usage, observed.usage);
    assert_budget(
        &fixture.compile(exact.with_working_bytes(ByteLimit::new(working - 1).unwrap())),
        "working-bytes",
        working - 1,
    );
    assert_eq!(fixture.source, before);
}

#[test]
fn hidden_contextual_payload_uses_the_original_node_and_byte_quotas() {
    for (payload, limits, kind) in [
        (
            null_list(),
            output_limits().with_reference_work_nodes(ItemLimit::new(128).unwrap()),
            "reference-work-nodes",
        ),
        (
            long_reference(),
            output_limits().with_reference_work_bytes(ByteLimit::new(128).unwrap()),
            "reference-work-bytes",
        ),
    ] {
        let copied = value_work_bytes(payload.cost()).unwrap();
        let fixture = Fixture::new(empty_list(), Some(payload));
        let before = fixture.source.clone();
        let report = fixture.compile(limits);
        assert_budget(&report, kind, 128);
        assert_eq!(report.usage.materialized_value_nodes, 0);
        assert_eq!(report.usage.materialized_value_bytes, 0);
        // The raw declaration was admitted and copied; default-program scratch was not.
        assert!(report.usage.scratch_bytes >= copied);
        assert!(report.usage.scratch_bytes < copied + 16 * 1_024);
        assert_eq!(fixture.source, before);
    }
}

#[test]
fn masked_defaults_fit_exact_compiler_allowances_and_refuse_one_short() {
    let empty = Fixture::new(empty_list(), None).compile(output_limits());
    assert_effective_empty(&empty);
    for (canonical, contextual) in [
        (null_list(), None),
        (empty_list(), Some(null_list())),
        (empty_list(), Some(long_reference())),
    ] {
        let fixture = Fixture::new(canonical, contextual);
        let before = fixture.source.clone();
        let observed = fixture.compile(output_limits());
        assert_effective_empty(&observed);
        assert!(observed.reference_work.0 > PRIOR_NODES);
        assert!(observed.reference_work.1 > PRIOR_BYTES);
        if fixture.source.entities[0].components[&component()]
            .contextual_defaults
            .is_empty()
        {
            assert_eq!(
                observed.reference_work.0,
                empty.reference_work.0 + PAYLOAD_ITEMS
            );
            assert_eq!(observed.reference_work.1, empty.reference_work.1);
        }
        let working = observed.usage.checked_work_bytes().unwrap();
        let (nodes, bytes) = observed.reference_work;
        let exact = output_limits()
            .with_working_bytes(ByteLimit::new(working).unwrap())
            .with_reference_work_nodes(ItemLimit::new(nodes).unwrap())
            .with_reference_work_bytes(ByteLimit::new(bytes).unwrap());
        let fits = fixture.compile(exact);
        assert_effective_empty(&fits);
        assert_eq!(fits.result, observed.result);
        assert_eq!(fits.usage, observed.usage);
        assert_eq!(fits.reference_work, observed.reference_work);
        for (limits, kind, maximum) in [
            (
                exact.with_working_bytes(ByteLimit::new(working - 1).unwrap()),
                "working-bytes",
                working - 1,
            ),
            (
                exact.with_reference_work_nodes(ItemLimit::new(nodes - 1).unwrap()),
                "reference-work-nodes",
                nodes - 1,
            ),
            (
                exact.with_reference_work_bytes(ByteLimit::new(bytes - 1).unwrap()),
                "reference-work-bytes",
                bytes - 1,
            ),
        ] {
            assert_budget(&fixture.compile(limits), kind, maximum);
        }
        assert_eq!(fixture.source, before);
    }
}

#[test]
fn contextual_admission_preserves_source_output_and_semantic_failure_classes() {
    let raw_limits = ComponentValueMaterializationLimits::new(
        ItemLimit::new(2).unwrap(),
        ByteLimit::new("payload".len()).unwrap(),
        DepthLimit::new(4).unwrap(),
    );
    let limits = output_limits().with_materialization_limits(raw_limits);
    let canonical = Fixture::new(null_list(), None);
    // A Schema-owned default is not authored syntax or selected effective output.
    assert_effective_empty(&canonical.compile(limits));
    let contextual = Fixture::new(empty_list(), Some(null_list()));
    assert_budget(
        &contextual.compile(limits),
        "source-value-nodes",
        raw_limits.nodes(),
    );
    let raw_bytes = ComponentValueMaterializationLimits::new(
        ItemLimit::new(32).unwrap(),
        ByteLimit::new(8).unwrap(),
        DepthLimit::new(4).unwrap(),
    );
    assert_budget(
        &Fixture::new(empty_list(), Some(long_reference()))
            .compile(output_limits().with_materialization_limits(raw_bytes)),
        "source-value-bytes",
        raw_bytes.bytes(),
    );
    let raw_depth = ComponentValueMaterializationLimits::new(
        ItemLimit::new(32).unwrap(),
        ByteLimit::new(64).unwrap(),
        DepthLimit::new(1).unwrap(),
    );
    // Isolate the source-value quota. The combined diagnostic policy also lowers topology
    // and final-value depth and can reject the source placement before value interpretation.
    let mut source_depth_limits = output_limits();
    source_depth_limits.source_value_limits = Some(raw_depth);
    assert_budget(
        &canonical.compile(source_depth_limits),
        "source-value-depth",
        raw_depth.depth().get(),
    );
    let output = canonical.compile(output_limits().with_materialized_value_nodes(ItemLimit::ONE));
    assert_budget(&output, "materialized-value-nodes", 1);

    let invalid = Fixture::new(empty_list(), Some(ComponentValue::Bool(false)));
    let report = invalid.compile(output_limits());
    assert_eq!(
        report.result.as_ref().unwrap_err().reason(),
        &SceneCompilationFailureKind::Component
    );
    assert!(!report.budget_failed);
    let diagnostic = report.diagnostics.iter().next().unwrap();
    assert_eq!(
        diagnostic.code().as_str(),
        "scene.prefab-compilation-failed"
    );
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == "failure-kind"
            && field.value() == DiagnosticValueRef::Identifier("component")
    }));
}

#[test]
fn opaque_cost_preserves_raw_payload_and_separate_hidden_default_admission() {
    let mut fixture = Fixture::new(empty_list(), Some(null_list()));
    let record = fixture.source.entities[0]
        .components
        .get_mut(&component())
        .unwrap();
    record.version = ComponentSchemaVersion::new(2).unwrap();
    record.set_value(ComponentValue::String("opaque".repeat(32)));
    let original = record.clone();
    let before = fixture.source.clone();
    let limits = SceneCompilationLimits::default()
        .with_materialized_value_nodes(ItemLimit::ONE)
        .with_materialized_value_bytes(ByteLimit::new(192).unwrap());
    let observed = fixture.compile(limits);
    let document = observed.result.as_ref().unwrap();
    assert!(!observed.budget_failed && !observed.diagnostics.has_errors());
    let observed_record = &document.entities[0].components[&component()];
    assert_eq!(observed_record.owner, original.owner);
    assert_eq!(observed_record.version, original.version);
    assert_eq!(observed_record.value(), original.value());
    assert_eq!(
        observed_record.contextual_defaults,
        original.contextual_defaults
    );
    assert_eq!(observed.usage.materialized_value_nodes, 1);
    assert_eq!(observed.usage.materialized_value_bytes, 192);
    assert!(observed.usage.scratch_bytes >= PAYLOAD_ITEMS * 96);
    let working = observed.usage.checked_work_bytes().unwrap();
    let exact = limits.with_working_bytes(ByteLimit::new(working).unwrap());
    let fits = fixture.compile(exact);
    assert_eq!(fits.result, observed.result);
    assert_eq!(fits.usage, observed.usage);
    assert_eq!(fits.reference_work, observed.reference_work);
    assert_budget(
        &fixture.compile(exact.with_working_bytes(ByteLimit::new(working - 1).unwrap())),
        "working-bytes",
        working - 1,
    );
    assert_budget(
        &fixture.compile(limits.with_materialized_value_bytes(ByteLimit::new(191).unwrap())),
        "materialized-value-bytes",
        191,
    );
    let source_limits = ComponentValueMaterializationLimits::new(
        ItemLimit::new(2).unwrap(),
        ByteLimit::new(192).unwrap(),
        DepthLimit::new(4).unwrap(),
    );
    assert_budget(
        &fixture.compile(limits.with_materialization_limits(source_limits)),
        "source-value-nodes",
        2,
    );
    assert_eq!(fixture.source, before);
}
