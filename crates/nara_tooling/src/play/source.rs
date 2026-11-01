//! Native runtime export lowered through the retained source owner's authoring contract.

use nara_reflect::{
    ComponentFieldPathError, ComponentFieldSchema, ComponentValue, ComponentValueCost,
};
use nara_scene::{
    AuthoringSchemaBasis, MAX_AUTHORING_EDIT_OPERATIONS, MAX_AUTHORING_FIELD_LOGICAL_BYTES,
    MAX_AUTHORING_FIELD_VALUE_NODES, MAX_AUTHORING_FIELD_WRITES,
};

use super::*;

pub(super) struct SourceApplyChanges<'a> {
    session: &'a SceneAuthoringSession,
    basis: AuthoringSchemaBasis,
    budget: ApplyChangesBudget,
}

impl<'a> SourceApplyChanges<'a> {
    pub(super) fn new(session: &'a SceneAuthoringSession) -> Option<Self> {
        Some(Self {
            session,
            basis: session.authoring_schema_basis()?,
            budget: ApplyChangesBudget::default(),
        })
    }

    pub(super) fn export_component_change(
        &mut self,
        world: &World,
        play_entity: Entity,
        document_entity: &SceneEntityRecord,
        registry: &ComponentRegistrySnapshot,
        component: &ComponentTypeId,
        context: &ComponentEncodeContext<'_>,
    ) -> ExportedComponentChange {
        match self.operations(
            world,
            play_entity,
            document_entity,
            registry,
            component,
            context,
        ) {
            Ok(operations) => ExportedComponentChange {
                report: SceneApplyChangesComponentReport {
                    entity: document_entity.id.clone(),
                    component: component.clone(),
                    status: if operations.is_empty() {
                        SceneApplyChangesComponentStatus::NoOp
                    } else {
                        SceneApplyChangesComponentStatus::Pending
                    },
                    operation_count: operations.len(),
                    diagnostics: DiagnosticReport::default(),
                },
                operations,
            },
            Err(entry) => ExportedComponentChange {
                report: rejected_component_report(&document_entity.id, component, entry),
                operations: Vec::new(),
            },
        }
    }

    fn operations(
        &mut self,
        world: &World,
        play_entity: Entity,
        document_entity: &SceneEntityRecord,
        registry: &ComponentRegistrySnapshot,
        component: &ComponentTypeId,
        context: &ComponentEncodeContext<'_>,
    ) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
        let error = |code, summary| component_error(code, summary, &document_entity.id, component);
        let budget_error = |code| {
            error(
                code,
                "Apply Changes exceeds the cumulative source authoring budget before retaining an operation",
            )
        };
        let schema = self.basis.schema(component).ok_or_else(|| {
            error(
                "tooling.apply-changes-source-schema-unavailable",
                "Apply Changes requires the component's retained source Schema",
            )
        })?;
        if registry.schema(component) != Some(schema)
            || !self
                .basis
                .component_schema_owner(component)
                .zip(registry.component_schema_owner(component))
                .is_some_and(|(source, native)| source.matches_executable(native) == Ok(true))
        {
            return Err(error(
                "tooling.apply-changes-source-schema-mismatch",
                "Apply Changes requires exact source and Native owner and Schema agreement",
            ));
        }
        if !schema.has_capability(ComponentCapability::Scene)
            || !schema.has_capability(ComponentCapability::Edit)
        {
            return Err(error(
                "tooling.apply-changes-component-not-editable",
                "Apply Changes targets a component without scene/edit capabilities",
            ));
        }
        let current = document_entity.components.get(component);
        if current.is_some()
            && self
                .session
                .authoring_component_schema(&document_entity.id, component)
                != Some(schema)
        {
            return Err(error(
                "tooling.apply-changes-source-component-unavailable",
                "Apply Changes requires an Available current direct source component",
            ));
        }
        let runtime = registry
            .encode_component_with_context(component, world, play_entity, context)
            .map_err(|failure| {
                with_codec_error(
                    error(
                        "tooling.apply-changes-component-encode-failed",
                        "Apply Changes could not encode the runtime component",
                    ),
                    &failure,
                    Some(schema),
                )
            })?;

        let mut operations = Vec::new();
        let Some(runtime) = runtime else {
            if current.is_some() {
                // The source owner checks complete field editability and creates the exact Undo.
                operations.push(
                    self.budget
                        .prepare(false, None, || ScenePatchOperation::RemoveComponent {
                            entity: document_entity.id.clone(),
                            component: component.clone(),
                        })
                        .map_err(budget_error)?,
                );
            }
            return Ok(operations);
        };
        let mut template = None;
        let current = if let Some(current) = current {
            &current.value
        } else {
            let initial = self
                .budget
                .prepare(false, None, || {
                    self.session.authoring_initial_component(component)
                })
                .map_err(budget_error)?
                .ok_or_else(|| {
                    error(
                        "tooling.apply-changes-source-template-unavailable",
                        "Apply Changes component creation requires its source-owned complete template",
                    )
                })?;
            &template.insert(initial).value
        };
        for field in schema.fields().iter().filter(|field| {
            field.has_capability(ComponentCapability::Scene)
                && field.has_capability(ComponentCapability::Edit)
        }) {
            let path_error = |_| {
                error(
                    "tooling.apply-changes-field-path-unavailable",
                    "Apply Changes could not read a declared component field",
                )
            };
            let previous = field_value(current, field).map_err(path_error)?;
            let replacement = field_value(&runtime, field).map_err(path_error)?;
            if previous.or(field.default_value()) == replacement.or(field.default_value()) {
                continue;
            }
            // No effective component is copied into the source: only changed declared roots.
            // Omitted defaults and all unrelated, unknown and read-only data stay authored as-is.
            operations.push(
                self.budget
                    .prepare(true, replacement, || match replacement {
                        Some(value) => ScenePatchOperation::SetField {
                            entity: document_entity.id.clone(),
                            component: component.clone(),
                            component_version: schema.version(),
                            field: field.id().clone(),
                            value: value.clone(),
                        },
                        None => ScenePatchOperation::RemoveField {
                            entity: document_entity.id.clone(),
                            component: component.clone(),
                            component_version: schema.version(),
                            field: field.id().clone(),
                        },
                    })
                    .map_err(budget_error)?,
            );
        }
        if let Some(value) = template {
            operations.insert(
                0,
                ScenePatchOperation::AddComponent {
                    entity: document_entity.id.clone(),
                    component: component.clone(),
                    value,
                },
            );
        }
        Ok(operations)
    }

    pub(super) fn prepare(&self, patch: &ScenePatchDocument) -> Result<(), ScenePatchReport> {
        // This is the same failure-atomic source transaction used by the Host at publication.
        // It owns budgets, references, read-only policy, exact preservation and history admission.
        self.session
            .prepare_authoring_edit(self.session.revision(), patch)
            .map(|_| ())
    }
}

#[derive(Default)]
struct ApplyChangesBudget {
    operations: usize,
    field_writes: usize,
    value_nodes: usize,
    logical_bytes: usize,
}

impl ApplyChangesBudget {
    fn prepare<T>(
        &mut self,
        field_write: bool,
        value: Option<&ComponentValue>,
        prepare: impl FnOnce() -> T,
    ) -> Result<T, &'static str> {
        if self.operations >= MAX_AUTHORING_EDIT_OPERATIONS {
            return Err("tooling.apply-changes-operation-limit");
        }
        if field_write && self.field_writes >= MAX_AUTHORING_FIELD_WRITES {
            return Err("tooling.apply-changes-field-limit");
        }
        let cost = match value {
            Some(value) => value
                .cost_up_to(
                    MAX_AUTHORING_FIELD_VALUE_NODES.saturating_sub(self.value_nodes),
                    MAX_AUTHORING_FIELD_LOGICAL_BYTES.saturating_sub(self.logical_bytes),
                )
                .ok_or("tooling.apply-changes-field-value-limit")?,
            None => ComponentValueCost::ZERO,
        };
        self.operations += 1;
        self.field_writes += usize::from(field_write);
        self.value_nodes += cost.nodes();
        self.logical_bytes += cost.logical_bytes();
        // The Native codec already owns its output. Admit the borrowed replacement before
        // the next source template copy, field clone, or operation retention can begin.
        Ok(prepare())
    }
}

fn field_value<'a>(
    value: &'a ComponentValue,
    field: &ComponentFieldSchema,
) -> Result<Option<&'a ComponentValue>, ComponentFieldPathError> {
    match value.get_path(field.path()) {
        Ok(value) => Ok(Some(value)),
        Err(
            ComponentFieldPathError::MissingField { .. }
            | ComponentFieldPathError::IndexOutOfBounds { .. },
        ) => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use nara_reflect::{ComponentFieldId, ComponentSchemaVersion};

    use super::*;

    #[test]
    fn source_apply_changes_operation_budget_includes_template_add_remove_and_fields() {
        let mut budget = ApplyChangesBudget::default();
        let entity = SceneEntityId::new("player").unwrap();
        let component = ComponentTypeId::new("play.Budget");
        let value = ComponentValue::U64(17);
        let template = SceneComponentRecord::new(ComponentSchemaVersion::ONE, value.clone());
        let remove = || ScenePatchOperation::RemoveComponent {
            entity: entity.clone(),
            component: component.clone(),
        };
        budget
            .prepare(false, None, || ScenePatchOperation::AddComponent {
                entity: entity.clone(),
                component: component.clone(),
                value: template.clone(),
            })
            .unwrap();
        for _ in 1..(MAX_AUTHORING_EDIT_OPERATIONS - 1) {
            budget.prepare(false, None, remove).unwrap();
        }
        assert_eq!(budget.operations, MAX_AUTHORING_EDIT_OPERATIONS - 1);
        assert_eq!(budget.field_writes, 0);
        assert_eq!(budget.value_nodes, 0);
        assert_eq!(budget.logical_bytes, 0);
        budget
            .prepare(true, Some(&value), || ScenePatchOperation::SetField {
                entity: entity.clone(),
                component: component.clone(),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("value"),
                value: value.clone(),
            })
            .unwrap();
        assert_eq!(budget.operations, MAX_AUTHORING_EDIT_OPERATIONS);
        assert_eq!(budget.field_writes, 1);
        assert_eq!(budget.value_nodes, 1);
        assert_eq!(budget.logical_bytes, 8);
        let mut prepared = false;
        for field_write in [false, true] {
            let replacement = field_write.then_some(&value);
            assert_eq!(
                budget.prepare(field_write, replacement, || {
                    prepared = true;
                    remove()
                }),
                Err("tooling.apply-changes-operation-limit"),
            );
        }
        assert!(!prepared);
        assert_eq!(budget.operations, MAX_AUTHORING_EDIT_OPERATIONS);
        assert_eq!(budget.field_writes, 1);
    }

    #[test]
    fn source_apply_changes_field_budget_counts_both_sets_and_removals() {
        let mut budget = ApplyChangesBudget::default();
        let value = ComponentValue::U64(17);
        let mut prepared = 0;
        let mut sets = 0;
        for index in 0..MAX_AUTHORING_FIELD_WRITES {
            let replacement = (index % 2 == 0).then_some(&value);
            budget
                .prepare(true, replacement, || {
                    prepared += 1;
                    replacement.cloned()
                })
                .unwrap();
            sets += usize::from(replacement.is_some());
        }
        assert_eq!(budget.operations, MAX_AUTHORING_FIELD_WRITES);
        assert_eq!(budget.field_writes, MAX_AUTHORING_FIELD_WRITES);
        assert_eq!(budget.value_nodes, sets);
        assert_eq!(budget.logical_bytes, sets * 8);
        for replacement in [Some(&value), None] {
            assert_eq!(
                budget.prepare(true, replacement, || {
                    prepared += 1;
                    replacement.cloned()
                }),
                Err("tooling.apply-changes-field-limit"),
            );
        }
        assert_eq!(prepared, MAX_AUTHORING_FIELD_WRITES);
        assert_eq!(budget.operations, MAX_AUTHORING_FIELD_WRITES);
        assert_eq!(budget.field_writes, MAX_AUTHORING_FIELD_WRITES);
    }

    #[test]
    fn source_apply_changes_cumulative_nodes_admit_exact_but_refuse_one_short_before_clone() {
        let first = ComponentValue::List(vec![
            ComponentValue::Null;
            MAX_AUTHORING_FIELD_VALUE_NODES / 2 - 1
        ]);
        let first_nodes = first.cost().nodes();
        for extra in [0, 1] {
            let mut budget = ApplyChangesBudget::default();
            let mut copies = 0;
            let second =
                ComponentValue::List(vec![
                    ComponentValue::Null;
                    MAX_AUTHORING_FIELD_VALUE_NODES - first_nodes - 1
                        + extra
                ]);
            budget
                .prepare(true, Some(&first), || {
                    copies += 1;
                    first.clone()
                })
                .unwrap();
            let result = budget.prepare(true, Some(&second), || {
                copies += 1;
                second.clone()
            });
            if extra == 0 {
                assert_eq!(result, Ok(second));
                assert_eq!(copies, 2);
                assert_eq!(budget.value_nodes, MAX_AUTHORING_FIELD_VALUE_NODES);
                budget.prepare(true, None, || ()).unwrap();
                assert_eq!(budget.value_nodes, MAX_AUTHORING_FIELD_VALUE_NODES);
            } else {
                assert_eq!(result, Err("tooling.apply-changes-field-value-limit"));
                assert_eq!(copies, 1);
                assert_eq!(budget.operations, 1);
                assert_eq!(budget.field_writes, 1);
                assert_eq!(budget.value_nodes, first_nodes);
            }
        }
    }

    #[test]
    fn source_apply_changes_cumulative_bytes_admit_exact_but_refuse_one_short_before_clone() {
        let first = ComponentValue::map([(
            "first",
            ComponentValue::String("a".repeat(MAX_AUTHORING_FIELD_LOGICAL_BYTES / 2 - 5)),
        )]);
        let first_cost = first.cost();
        let key = "λ";
        for extra in [0, 1] {
            let mut budget = ApplyChangesBudget::default();
            let mut copies = 0;
            let second = ComponentValue::map([(
                key,
                ComponentValue::String("b".repeat(
                    MAX_AUTHORING_FIELD_LOGICAL_BYTES - first_cost.logical_bytes() - key.len()
                        + extra,
                )),
            )]);
            budget
                .prepare(true, Some(&first), || {
                    copies += 1;
                    first.clone()
                })
                .unwrap();
            let result = budget.prepare(true, Some(&second), || {
                copies += 1;
                second.clone()
            });
            if extra == 0 {
                assert_eq!(result, Ok(second));
                assert_eq!(copies, 2);
                assert_eq!(budget.logical_bytes, MAX_AUTHORING_FIELD_LOGICAL_BYTES);
                let empty = ComponentValue::String(String::new());
                budget
                    .prepare(true, Some(&empty), || empty.clone())
                    .unwrap();
                budget.prepare(true, None, || ()).unwrap();
                assert_eq!(budget.logical_bytes, MAX_AUTHORING_FIELD_LOGICAL_BYTES);
            } else {
                assert_eq!(result, Err("tooling.apply-changes-field-value-limit"));
                assert_eq!(copies, 1);
                assert_eq!(budget.operations, 1);
                assert_eq!(budget.field_writes, 1);
                assert_eq!(budget.value_nodes, first_cost.nodes());
                assert_eq!(budget.logical_bytes, first_cost.logical_bytes());
            }
        }
    }
}
