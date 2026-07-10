use std::collections::BTreeMap;

use nara_ecs::Resource;
use nara_input::{ActionContext, ActionId, ActionPhase};
use thiserror::Error;

use crate::{
    GameplayCommandDraft, GameplayCommandPayload, GameplayCommandTarget, GameplayCommandTypeId,
};

pub const MAX_ACTION_COMMAND_BINDINGS: usize = 4_096;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ActionCommandMapError {
    #[error("action command map exceeds its binding limit")]
    BindingLimit { requested: usize, maximum: usize },
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ActionCommandBinding {
    action: ActionId,
    #[cfg_attr(feature = "serde", serde(default))]
    context: ActionContext,
    phase: ActionPhase,
    command: GameplayCommandDraft,
}

impl ActionCommandBinding {
    #[must_use]
    pub fn new(action: ActionId, phase: ActionPhase, command_type: GameplayCommandTypeId) -> Self {
        Self {
            action,
            context: ActionContext::gameplay(),
            phase,
            command: GameplayCommandDraft::new(command_type),
        }
    }

    #[must_use]
    pub fn with_context(mut self, context: ActionContext) -> Self {
        self.context = context;
        self
    }

    #[must_use]
    pub fn with_command(mut self, command: GameplayCommandDraft) -> Self {
        self.command = command;
        self
    }

    #[must_use]
    pub fn with_target(mut self, target: GameplayCommandTarget) -> Self {
        self.command = self.command.with_target(target);
        self
    }

    #[must_use]
    pub fn with_payload(mut self, payload: GameplayCommandPayload) -> Self {
        self.command = self.command.with_payload(payload);
        self
    }

    #[must_use]
    pub const fn action(&self) -> &ActionId {
        &self.action
    }

    #[must_use]
    pub const fn context(&self) -> &ActionContext {
        &self.context
    }

    #[must_use]
    pub const fn phase(&self) -> ActionPhase {
        self.phase
    }

    #[must_use]
    pub const fn command(&self) -> &GameplayCommandDraft {
        &self.command
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ActionCommandKey {
    action: ActionId,
    context: ActionContext,
    phase: ActionPhase,
}

impl ActionCommandKey {
    fn new(action: ActionId, context: ActionContext, phase: ActionPhase) -> Self {
        Self {
            action,
            context,
            phase,
        }
    }

    fn from_binding(binding: &ActionCommandBinding) -> Self {
        Self::new(
            binding.action.clone(),
            binding.context.clone(),
            binding.phase,
        )
    }
}

/// Bounded action-to-command authoring data.
///
/// Serde validation limits binding count, but file-backed callers must still enforce ADR 0049
/// encoded-byte and nesting budgets before deserializing untrusted project data.
#[derive(Debug, Default, Clone, Resource)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ActionCommandMap {
    bindings: Vec<ActionCommandBinding>,
    #[cfg_attr(feature = "serde", serde(skip))]
    bindings_by_action: BTreeMap<ActionCommandKey, Vec<usize>>,
}

impl PartialEq for ActionCommandMap {
    fn eq(&self, other: &Self) -> bool {
        self.bindings == other.bindings
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ActionCommandMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Default)]
        struct BoundedBindings(Vec<ActionCommandBinding>);

        impl<'de> serde::Deserialize<'de> for BoundedBindings {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct BindingsVisitor;

                impl<'de> serde::de::Visitor<'de> for BindingsVisitor {
                    type Value = BoundedBindings;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter<'_>,
                    ) -> std::fmt::Result {
                        formatter.write_str("a bounded action command binding sequence")
                    }

                    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
                    where
                        A: serde::de::SeqAccess<'de>,
                    {
                        let mut bindings = Vec::with_capacity(
                            sequence
                                .size_hint()
                                .unwrap_or_default()
                                .min(MAX_ACTION_COMMAND_BINDINGS),
                        );
                        while let Some(binding) = sequence.next_element()? {
                            let Some(requested) = bindings.len().checked_add(1) else {
                                return Err(serde::de::Error::custom(
                                    ActionCommandMapError::BindingLimit {
                                        requested: usize::MAX,
                                        maximum: MAX_ACTION_COMMAND_BINDINGS,
                                    },
                                ));
                            };
                            if requested > MAX_ACTION_COMMAND_BINDINGS {
                                return Err(serde::de::Error::custom(
                                    ActionCommandMapError::BindingLimit {
                                        requested,
                                        maximum: MAX_ACTION_COMMAND_BINDINGS,
                                    },
                                ));
                            }
                            bindings.push(binding);
                        }
                        Ok(BoundedBindings(bindings))
                    }
                }

                deserializer.deserialize_seq(BindingsVisitor)
            }
        }

        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawActionCommandMap {
            #[serde(default)]
            bindings: BoundedBindings,
        }

        let raw = <RawActionCommandMap as serde::Deserialize>::deserialize(deserializer)?;
        let mut command_map = Self::default();
        for binding in raw.bindings.0 {
            command_map
                .bind(binding)
                .map_err(serde::de::Error::custom)?;
        }
        Ok(command_map)
    }
}

impl ActionCommandMap {
    pub fn bind(&mut self, binding: ActionCommandBinding) -> Result<(), ActionCommandMapError> {
        let Some(requested) = self.bindings.len().checked_add(1) else {
            return Err(ActionCommandMapError::BindingLimit {
                requested: usize::MAX,
                maximum: MAX_ACTION_COMMAND_BINDINGS,
            });
        };
        if requested > MAX_ACTION_COMMAND_BINDINGS {
            return Err(ActionCommandMapError::BindingLimit {
                requested,
                maximum: MAX_ACTION_COMMAND_BINDINGS,
            });
        }
        self.bindings_by_action
            .entry(ActionCommandKey::from_binding(&binding))
            .or_default()
            .push(self.bindings.len());
        self.bindings.push(binding);
        Ok(())
    }

    pub fn bind_action(
        &mut self,
        action: ActionId,
        phase: ActionPhase,
        command_type: GameplayCommandTypeId,
    ) -> Result<(), ActionCommandMapError> {
        self.bind(ActionCommandBinding::new(action, phase, command_type))
    }

    #[must_use]
    pub fn bindings(&self) -> &[ActionCommandBinding] {
        &self.bindings
    }

    pub fn matching_bindings(
        &self,
        action: &ActionId,
        context: &ActionContext,
        phase: ActionPhase,
    ) -> impl Iterator<Item = &ActionCommandBinding> {
        let key = ActionCommandKey::new(action.clone(), context.clone(), phase);
        self.bindings_by_action
            .get(&key)
            .into_iter()
            .flat_map(|indices| indices.iter().filter_map(|index| self.bindings.get(*index)))
    }
}
