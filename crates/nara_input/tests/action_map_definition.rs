use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use nara_core::{ByteLimit, ItemLimit};
use nara_input::{
    ActionBinding, ActionContext, ActionContextDefinition, ActionId, ActionIdError, ActionMap,
    ActionMapBindings, ActionMapBuilder, ActionMapDefinition, ActionMapError, ActionMapLimitKind,
    ActionMapLimits, InputBinding, KeyCode, MAX_ACTION_BINDINGS, MAX_ACTION_CONTEXTS,
    MAX_ACTION_IDENTIFIER_BYTES, MAX_BINDINGS_PER_INPUT, MouseButton,
};

fn sample_definition(limits: ActionMapLimits) -> Result<ActionMapDefinition, ActionMapError> {
    let mut builder = ActionMapBuilder::new(limits);
    builder.declare_context(ActionContext::gameplay(), true)?;
    let menu = ActionContext::new("menu").unwrap();
    builder.declare_context(menu.clone(), false)?;
    for binding in [
        ActionBinding::key(ActionId::new("confirm").unwrap(), KeyCode::Enter),
        ActionBinding::mouse(ActionId::new("fire").unwrap(), MouseButton::Left),
        ActionBinding::key(ActionId::new("choose").unwrap(), KeyCode::Enter).with_context(menu),
    ] {
        builder.bind(binding)?;
    }
    builder.compile()
}

#[test]
fn empty_builder_requires_an_explicit_gameplay_declaration() {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    assert_eq!(
        builder
            .bind(ActionBinding::key(
                ActionId::new("jump").unwrap(),
                KeyCode::Space
            ))
            .unwrap_err(),
        ActionMapError::UnknownContext(ActionContext::gameplay())
    );
    let definition = builder.compile().unwrap();
    assert!(definition.contexts().is_empty());
    assert!(definition.bindings().is_empty());

    let mut map = ActionMap::default();
    assert!(map.definition().contexts().is_empty());
    assert!(!map.is_context_enabled(&ActionContext::gameplay()));
    assert_eq!(
        map.enable_context(&ActionContext::gameplay()),
        Err(ActionMapError::UnknownContext(ActionContext::gameplay()))
    );
}

#[test]
fn duplicate_context_does_not_overwrite_the_original_declaration() {
    let menu = ActionContext::new("menu").unwrap();
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder.declare_context(menu.clone(), false).unwrap();
    assert_eq!(
        builder.declare_context(menu.clone(), true).unwrap_err(),
        ActionMapError::DuplicateContext(menu.clone())
    );
    let definition = builder.compile().unwrap();
    assert_eq!(
        definition.contexts(),
        [ActionContextDefinition {
            context: menu,
            enabled: false,
        }]
    );
    assert!(definition.bindings().is_empty());
}

#[test]
fn unknown_binding_context_is_rejected_without_becoming_a_declaration() {
    let menu = ActionContext::new("menu").unwrap();
    let binding = ActionBinding::key(ActionId::new("choose").unwrap(), KeyCode::Enter)
        .with_context(menu.clone());
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    assert_eq!(
        builder.bind(binding.clone()).unwrap_err(),
        ActionMapError::UnknownContext(menu.clone())
    );
    builder.declare_context(menu.clone(), false).unwrap();
    builder.bind(binding.clone()).unwrap();
    let definition = builder.compile().unwrap();
    assert_eq!(definition.bindings(), [binding]);
    assert_eq!(definition.contexts().len(), 2);
    assert_eq!(definition.contexts()[1].context, menu);
    assert!(!definition.contexts()[1].enabled);
}

#[test]
fn only_complete_duplicate_bindings_are_rejected_and_lookup_keeps_declaration_order() {
    let gameplay = ActionContext::gameplay();
    let menu = ActionContext::new("menu").unwrap();
    let action = ActionId::new("confirm").unwrap();
    let first = ActionBinding::key(action.clone(), KeyCode::Enter);
    let expected = [
        first.clone(),
        ActionBinding::key(ActionId::new("accept").unwrap(), KeyCode::Enter),
        ActionBinding::mouse(action.clone(), MouseButton::Left),
        ActionBinding::key(action, KeyCode::Enter).with_context(menu.clone()),
    ];
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder.declare_context(gameplay, true).unwrap();
    builder.declare_context(menu, false).unwrap();
    builder.bind(first.clone()).unwrap();
    assert_eq!(
        builder.bind(first).unwrap_err(),
        ActionMapError::DuplicateBinding
    );
    for binding in &expected[1..] {
        builder.bind(binding.clone()).unwrap();
    }
    let definition = builder.compile().unwrap();
    assert_eq!(definition.bindings(), expected);
    let map = ActionMap::new(
        definition.into_bindings(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        map.binding_indices_for_input(InputBinding::Key(KeyCode::Enter))
            .collect::<Vec<_>>(),
        [0, 1, 3]
    );
    assert_eq!(
        map.binding_indices_for_input(InputBinding::Mouse(MouseButton::Left))
            .collect::<Vec<_>>(),
        [2]
    );
    assert!(
        map.binding_indices_for_input(InputBinding::Key(KeyCode::Escape))
            .next()
            .is_none()
    );
    for (index, binding) in expected.iter().enumerate() {
        assert_eq!(map.binding_at(index), Some(binding));
    }
    assert_eq!(map.binding_at(expected.len()), None);
}

#[test]
fn context_declarations_obey_custom_limits_and_the_hard_cap() {
    for (limit, requested) in [(2, 2), (MAX_ACTION_CONTEXTS, MAX_ACTION_CONTEXTS + 1)] {
        let limits = ActionMapLimits::default().with_contexts(ItemLimit::new(requested).unwrap());
        let mut builder = ActionMapBuilder::new(limits);
        let mut expected = Vec::new();
        for index in 0..limit {
            let context = ActionContext::new(format!("context-{index}")).unwrap();
            let enabled = index % 2 == 0;
            builder.declare_context(context.clone(), enabled).unwrap();
            expected.push(ActionContextDefinition { context, enabled });
        }
        assert_eq!(
            builder
                .declare_context(ActionContext::new("overflow").unwrap(), true)
                .unwrap_err(),
            ActionMapError::LimitExceeded {
                kind: ActionMapLimitKind::Contexts,
                required: limit + 1,
                limit,
            }
        );
        let definition = builder.compile().unwrap();
        assert_eq!(definition.contexts(), expected);
        let map = ActionMap::new(
            definition.into_bindings(),
            ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap();
        for entry in &expected {
            assert_eq!(map.is_context_enabled(&entry.context), entry.enabled);
        }
    }
}

#[test]
fn binding_declarations_obey_custom_limits_and_the_hard_cap() {
    for (limit, requested) in [(2, 2), (MAX_ACTION_BINDINGS, MAX_ACTION_BINDINGS + 1)] {
        let limits = ActionMapLimits::default().with_bindings(ItemLimit::new(requested).unwrap());
        let mut builder = ActionMapBuilder::new(limits);
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        let mut expected = Vec::new();
        for index in 0..limit {
            let binding = ActionBinding::mouse(
                ActionId::new(format!("action-{index}")).unwrap(),
                MouseButton::Other(u16::try_from(index).unwrap()),
            );
            builder.bind(binding.clone()).unwrap();
            expected.push(binding);
        }
        assert_eq!(
            builder
                .bind(ActionBinding::key(
                    ActionId::new("overflow").unwrap(),
                    KeyCode::Enter
                ))
                .unwrap_err(),
            ActionMapError::LimitExceeded {
                kind: ActionMapLimitKind::Bindings,
                required: limit + 1,
                limit,
            }
        );
        assert_eq!(builder.compile().unwrap().bindings(), expected);
    }
}

#[test]
fn input_fanout_counts_all_contexts_and_obeys_custom_and_hard_limits() {
    for (limit, requested) in [(2, 2), (MAX_BINDINGS_PER_INPUT, MAX_BINDINGS_PER_INPUT + 1)] {
        let limits =
            ActionMapLimits::default().with_bindings_per_input(ItemLimit::new(requested).unwrap());
        let gameplay = ActionContext::gameplay();
        let menu = ActionContext::new("menu").unwrap();
        let mut builder = ActionMapBuilder::new(limits);
        builder.declare_context(gameplay.clone(), true).unwrap();
        builder.declare_context(menu.clone(), false).unwrap();
        for index in 0..limit {
            builder
                .bind(
                    ActionBinding::key(
                        ActionId::new(format!("action-{index}")).unwrap(),
                        KeyCode::Space,
                    )
                    .with_context(if index % 2 == 0 {
                        gameplay.clone()
                    } else {
                        menu.clone()
                    }),
                )
                .unwrap();
        }
        assert_eq!(
            builder
                .bind(
                    ActionBinding::key(ActionId::new("overflow").unwrap(), KeyCode::Space)
                        .with_context(menu),
                )
                .unwrap_err(),
            ActionMapError::LimitExceeded {
                kind: ActionMapLimitKind::BindingsPerInput,
                required: limit + 1,
                limit,
            }
        );
        let other = ActionBinding::mouse(ActionId::new("independent").unwrap(), MouseButton::Left);
        builder.bind(other.clone()).unwrap();
        let map = ActionMap::new(
            builder.compile().unwrap().into_bindings(),
            ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap();
        assert_eq!(map.bindings().len(), limit + 1);
        assert_eq!(
            map.binding_indices_for_input(InputBinding::Key(KeyCode::Space))
                .collect::<Vec<_>>(),
            (0..limit).collect::<Vec<_>>()
        );
        assert_eq!(
            map.binding_indices_for_input(InputBinding::Mouse(MouseButton::Left))
                .collect::<Vec<_>>(),
            [limit]
        );
        assert_eq!(map.binding_at(limit), Some(&other));
    }
}

#[test]
fn custom_identifier_limits_count_utf8_bytes_for_contexts_and_actions() {
    let limits = ActionMapLimits::default().with_identifier_bytes(ByteLimit::new(8).unwrap());
    let exact = "é".repeat(4);
    let over = "é".repeat(5);
    let context = ActionContext::new(&exact).unwrap();
    let mut builder = ActionMapBuilder::new(limits);
    builder.declare_context(context.clone(), true).unwrap();
    let expected = ActionBinding::key(ActionId::new(&exact).unwrap(), KeyCode::Enter)
        .with_context(context.clone());
    builder.bind(expected.clone()).unwrap();
    let error = ActionMapError::LimitExceeded {
        kind: ActionMapLimitKind::IdentifierBytes,
        required: over.len(),
        limit: exact.len(),
    };
    assert_eq!(
        builder
            .declare_context(ActionContext::new(&over).unwrap(), false)
            .unwrap_err(),
        error
    );
    assert_eq!(
        builder
            .bind(
                ActionBinding::key(ActionId::new(&over).unwrap(), KeyCode::Space)
                    .with_context(context.clone()),
            )
            .unwrap_err(),
        error
    );
    let definition = builder.compile().unwrap();
    assert_eq!(definition.bindings(), [expected]);
    assert_eq!(
        definition.contexts(),
        [ActionContextDefinition {
            context,
            enabled: true,
        }]
    );
}

#[test]
fn raising_identifier_allowance_cannot_bypass_the_identifier_hard_cap() {
    let exact = format!("{}é", "a".repeat(MAX_ACTION_IDENTIFIER_BYTES - 2));
    assert_eq!(exact.len(), MAX_ACTION_IDENTIFIER_BYTES);
    let over = format!("{exact}a");
    let limits =
        ActionMapLimits::default().with_identifier_bytes(ByteLimit::new(over.len()).unwrap());
    let context = ActionContext::new(&exact).unwrap();
    let binding = ActionBinding::key(ActionId::new(&exact).unwrap(), KeyCode::Enter)
        .with_context(context.clone());
    let mut builder = ActionMapBuilder::new(limits);
    builder.declare_context(context, true).unwrap();
    builder.bind(binding.clone()).unwrap();
    assert_eq!(builder.compile().unwrap().bindings(), [binding]);
    let error = ActionIdError::TooLong {
        bytes: over.len(),
        limit: MAX_ACTION_IDENTIFIER_BYTES,
    };
    assert_eq!(ActionContext::new(&over), Err(error.clone()));
    assert_eq!(ActionId::new(&over), Err(error));
}

#[test]
fn compiled_work_and_retained_allowances_accept_exact_and_reject_one_byte_short() {
    let baseline = sample_definition(ActionMapLimits::default()).unwrap();
    let cost = baseline.cost();
    let exact = ActionMapLimits::default()
        .with_work_bytes(ByteLimit::new(cost.work_bytes()).unwrap())
        .with_retained_bytes(ByteLimit::new(cost.retained_bytes()).unwrap());
    let definition = sample_definition(exact).unwrap();
    assert_eq!(definition.cost(), cost);
    assert_eq!(definition.contexts(), baseline.contexts());
    assert_eq!(definition.bindings(), baseline.bindings());
    assert_eq!(
        cost.runtime_state_bytes(),
        ActionMap::required_state_bytes()
    );
    assert_eq!(
        sample_definition(exact.with_work_bytes(ByteLimit::new(cost.work_bytes() - 1).unwrap()),)
            .unwrap_err(),
        ActionMapError::LimitExceeded {
            kind: ActionMapLimitKind::WorkBytes,
            required: cost.work_bytes(),
            limit: cost.work_bytes() - 1,
        }
    );
    assert_eq!(
        sample_definition(
            exact.with_retained_bytes(ByteLimit::new(cost.retained_bytes() - 1).unwrap()),
        )
        .unwrap_err(),
        ActionMapError::LimitExceeded {
            kind: ActionMapLimitKind::RetainedBytes,
            required: cost.retained_bytes(),
            limit: cost.retained_bytes() - 1,
        }
    );
}

#[test]
fn empty_definition_compile_charges_work_and_retained_ownership() {
    let cost = ActionMapBuilder::new(ActionMapLimits::default())
        .compile()
        .unwrap()
        .cost();
    let exact = ActionMapLimits::default()
        .with_work_bytes(ByteLimit::new(cost.work_bytes()).unwrap())
        .with_retained_bytes(ByteLimit::new(cost.retained_bytes()).unwrap());
    assert_eq!(ActionMapBuilder::new(exact).compile().unwrap().cost(), cost);
    for (limits, kind, required) in [
        (
            exact.with_work_bytes(ByteLimit::new(cost.work_bytes() - 1).unwrap()),
            ActionMapLimitKind::WorkBytes,
            cost.work_bytes(),
        ),
        (
            exact.with_retained_bytes(ByteLimit::new(cost.retained_bytes() - 1).unwrap()),
            ActionMapLimitKind::RetainedBytes,
            cost.retained_bytes(),
        ),
    ] {
        assert_eq!(
            ActionMapBuilder::new(limits).compile().unwrap_err(),
            ActionMapError::LimitExceeded {
                kind,
                required,
                limit: required - 1,
            }
        );
    }
}

#[test]
fn runtime_state_allowance_accepts_exact_and_rejects_one_byte_short() {
    let required = ActionMap::required_state_bytes();
    for definition in [
        ActionMapBuilder::new(ActionMapLimits::default())
            .compile()
            .unwrap(),
        sample_definition(ActionMapLimits::default()).unwrap(),
    ] {
        let bindings = definition.into_bindings();
        assert_eq!(
            ActionMap::new(bindings.clone(), ByteLimit::new(required - 1).unwrap()).unwrap_err(),
            ActionMapError::LimitExceeded {
                kind: ActionMapLimitKind::RuntimeStateBytes,
                required,
                limit: required - 1,
            }
        );
        let map = ActionMap::new(bindings.clone(), ByteLimit::new(required).unwrap()).unwrap();
        assert!(bindings.shares_owner_with(&map.bindings_handle()));
        assert!(std::ptr::eq(
            bindings.definition().bindings(),
            map.definition().bindings()
        ));
        for entry in map.definition().contexts() {
            assert_eq!(map.is_context_enabled(&entry.context), entry.enabled);
        }
    }
}

#[test]
fn unknown_context_operations_cannot_grow_or_reset_world_state() {
    let bindings = sample_definition(ActionMapLimits::default())
        .unwrap()
        .into_bindings();
    let mut map = ActionMap::new(
        bindings.clone(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap();
    let gameplay = ActionContext::gameplay();
    let menu = ActionContext::new("menu").unwrap();
    assert!(map.disable_context(gameplay.clone()).unwrap());
    assert!(map.enable_context(&menu).unwrap());
    for index in 0..=MAX_ACTION_CONTEXTS {
        let unknown = ActionContext::new(format!("unknown-{index}")).unwrap();
        let error = Err(ActionMapError::UnknownContext(unknown.clone()));
        assert_eq!(map.enable_context(&unknown), error);
        assert_eq!(map.disable_context(unknown.clone()), error);
        assert_eq!(map.set_context_enabled(&unknown, true), error);
        assert_eq!(map.set_context_enabled(&unknown, false), error);
        assert!(!map.is_context_enabled(&unknown));
    }
    assert!(bindings.shares_owner_with(&map.bindings_handle()));
    assert!(std::ptr::eq(
        bindings.definition().bindings(),
        map.definition().bindings()
    ));
    assert_eq!(map.definition().contexts().len(), 2);
    assert!(!map.is_context_enabled(&gameplay));
    assert!(map.is_context_enabled(&menu));
    assert!(!map.disable_context(gameplay.clone()).unwrap());
    assert!(!map.enable_context(&menu).unwrap());
    assert!(map.enable_context(&gameplay).unwrap());
    assert!(map.disable_context(menu).unwrap());
}

struct DefinitionOwner {
    definition: ActionMapDefinition,
    drops: Arc<AtomicUsize>,
}

struct SelectingDefinitionOwner {
    first: ActionMapDefinition,
    second: ActionMapDefinition,
    select_second: AtomicBool,
    projections: AtomicUsize,
}

impl AsRef<ActionMapDefinition> for SelectingDefinitionOwner {
    fn as_ref(&self) -> &ActionMapDefinition {
        self.projections.fetch_add(1, Ordering::SeqCst);
        if self.select_second.load(Ordering::SeqCst) {
            &self.second
        } else {
            &self.first
        }
    }
}

#[test]
fn bindings_capture_the_definition_once_even_when_the_owner_changes_its_projection() {
    let owner = Arc::new(SelectingDefinitionOwner {
        first: sample_definition(ActionMapLimits::default()).unwrap(),
        second: ActionMapBuilder::new(ActionMapLimits::default())
            .compile()
            .unwrap(),
        select_second: AtomicBool::new(false),
        projections: AtomicUsize::new(0),
    });
    let original = ActionMapBindings::from_owner(Arc::clone(&owner));
    let mut map = ActionMap::new(
        original.clone(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap();
    owner.select_second.store(true, Ordering::SeqCst);
    assert_eq!(owner.projections.load(Ordering::SeqCst), 1);
    assert_eq!(original.definition().bindings().len(), 3);
    assert_eq!(map.definition().contexts().len(), 2);
    assert!(map.is_context_enabled(&ActionContext::gameplay()));
    assert_eq!(
        map.binding_indices_for_input(InputBinding::Key(KeyCode::Enter))
            .collect::<Vec<_>>(),
        [0, 2]
    );
    assert_eq!(owner.projections.load(Ordering::SeqCst), 1);
    let successor = ActionMapBindings::from_owner(Arc::clone(&owner));
    assert!(successor.definition().contexts().is_empty());
    assert!(!original.shares_owner_with(&successor));
    assert!(original.shares_owner_with(&map.bindings_handle()));
    assert_eq!(owner.projections.load(Ordering::SeqCst), 2);
    map.replace_bindings(successor.clone());
    assert!(map.definition().contexts().is_empty());
    assert!(!map.is_context_enabled(&ActionContext::gameplay()));
    assert!(successor.shares_owner_with(&map.bindings_handle()));
    assert_eq!(original.definition().contexts().len(), 2);
    assert_eq!(owner.projections.load(Ordering::SeqCst), 2);
    let retained = Arc::downgrade(&owner);
    drop((owner, successor, map));
    assert!(retained.upgrade().is_some());
    assert_eq!(original.definition().bindings().len(), 3);
    drop(original);
    assert!(retained.upgrade().is_none());
}

impl AsRef<ActionMapDefinition> for DefinitionOwner {
    fn as_ref(&self) -> &ActionMapDefinition {
        &self.definition
    }
}

impl Drop for DefinitionOwner {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn cloned_bindings_retain_the_actual_definition_owner_until_the_last_handle_drops() {
    let drops = Arc::new(AtomicUsize::new(0));
    let owner = Arc::new(DefinitionOwner {
        definition: sample_definition(ActionMapLimits::default()).unwrap(),
        drops: Arc::clone(&drops),
    });
    let weak_owner = Arc::downgrade(&owner);
    let definition_address = owner.definition.bindings() as *const [ActionBinding];
    let bindings = ActionMapBindings::from_owner(Arc::clone(&owner));
    let cloned = bindings.clone();
    assert!(bindings.shares_owner_with(&cloned));
    assert!(std::ptr::eq(
        bindings.definition().bindings(),
        definition_address
    ));
    let map = ActionMap::new(
        bindings,
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap();
    let escaped = map.bindings_handle();
    assert!(cloned.shares_owner_with(&escaped));
    drop(owner);
    drop(map);
    assert!(weak_owner.upgrade().is_some());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert!(std::ptr::eq(
        cloned.definition().bindings(),
        definition_address
    ));
    drop(cloned);
    assert!(weak_owner.upgrade().is_some());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert!(std::ptr::eq(
        escaped.definition().bindings(),
        definition_address
    ));
    assert_eq!(escaped.definition().bindings().len(), 3);
    assert_eq!(
        escaped.definition().bindings()[0].action.as_str(),
        "confirm"
    );
    drop(escaped);
    assert!(weak_owner.upgrade().is_none());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

struct StateRetention {
    drops: Arc<AtomicUsize>,
}

impl Drop for StateRetention {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn state_constructor_retains_each_original_owner_without_mutating_other_world_state() {
    let bindings = sample_definition(ActionMapLimits::default())
        .unwrap()
        .into_bindings();
    let first_drops = Arc::new(AtomicUsize::new(0));
    let first_owner = Arc::new(StateRetention {
        drops: Arc::clone(&first_drops),
    });
    let first_weak = Arc::downgrade(&first_owner);
    let mut first = ActionMap::new_with_retention_owner(
        bindings.clone(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        first_owner,
    )
    .unwrap();
    let gameplay = ActionContext::gameplay();
    let menu = ActionContext::new("menu").unwrap();
    first.disable_context(gameplay.clone()).unwrap();
    first.enable_context(&menu).unwrap();

    let second_drops = Arc::new(AtomicUsize::new(0));
    let second_owner = Arc::new(StateRetention {
        drops: Arc::clone(&second_drops),
    });
    let second_weak = Arc::downgrade(&second_owner);
    let mut second = ActionMap::new_with_retention_owner(
        bindings.clone(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        second_owner,
    )
    .unwrap();
    assert!(
        first
            .bindings_handle()
            .shares_owner_with(&second.bindings_handle())
    );
    assert!(!first.is_context_enabled(&gameplay));
    assert!(first.is_context_enabled(&menu));
    assert!(second.is_context_enabled(&gameplay));
    assert!(!second.is_context_enabled(&menu));
    assert!(second.enable_context(&menu).unwrap());
    assert!(!first.is_context_enabled(&gameplay));
    assert!(bindings.definition().contexts()[0].enabled);
    assert!(!bindings.definition().contexts()[1].enabled);
    assert!(first_weak.upgrade().is_some());
    assert!(second_weak.upgrade().is_some());

    first.replace_bindings(
        ActionMapBuilder::new(ActionMapLimits::default())
            .compile()
            .unwrap()
            .into_bindings(),
    );
    assert!(first.definition().contexts().is_empty());
    assert!(second.is_context_enabled(&gameplay));
    assert!(second.is_context_enabled(&menu));
    assert_eq!(first_drops.load(Ordering::SeqCst), 0);
    assert!(first_weak.upgrade().is_some());
    let escaped = first.bindings_handle();
    drop(second);
    assert!(second_weak.upgrade().is_none());
    assert_eq!(second_drops.load(Ordering::SeqCst), 1);
    assert!(first_weak.upgrade().is_some());
    assert_eq!(first_drops.load(Ordering::SeqCst), 0);
    drop(first);
    assert!(first_weak.upgrade().is_none());
    assert_eq!(first_drops.load(Ordering::SeqCst), 1);
    assert!(escaped.definition().contexts().is_empty());
}

#[test]
fn failed_state_admission_releases_its_owner_and_preserves_existing_state() {
    let bindings = sample_definition(ActionMapLimits::default())
        .unwrap()
        .into_bindings();
    let required = ActionMap::required_state_bytes();
    let mut existing = ActionMap::new(bindings.clone(), ByteLimit::new(required).unwrap()).unwrap();
    existing.disable_context(ActionContext::gameplay()).unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let owner = Arc::new(StateRetention {
        drops: Arc::clone(&drops),
    });
    let weak_owner = Arc::downgrade(&owner);
    assert_eq!(
        ActionMap::new_with_retention_owner(
            bindings.clone(),
            ByteLimit::new(required - 1).unwrap(),
            owner,
        )
        .unwrap_err(),
        ActionMapError::LimitExceeded {
            kind: ActionMapLimitKind::RuntimeStateBytes,
            required,
            limit: required - 1,
        }
    );
    assert!(weak_owner.upgrade().is_none());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert!(!existing.is_context_enabled(&ActionContext::gameplay()));
    assert!(!existing.is_context_enabled(&ActionContext::new("menu").unwrap()));
    assert!(bindings.shares_owner_with(&existing.bindings_handle()));
    assert!(std::ptr::eq(
        bindings.definition().bindings(),
        existing.definition().bindings()
    ));
}
