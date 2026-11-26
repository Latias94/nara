//! Context-scoped shortcuts independent of the native shell's focus implementation.

use std::collections::BTreeMap;

use crate::viewport::ToolMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActionContext {
    Workspace,
    Viewport,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Shortcut {
    pub key: &'static str,
    pub primary: bool,
    pub shift: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    Save,
    Undo,
    Redo,
    Delete,
    Cancel,
    FrameSelection,
    Tool(ToolMode),
}

pub struct EditorKeymap(BTreeMap<(ActionContext, Shortcut), EditorAction>);

impl Default for EditorKeymap {
    fn default() -> Self {
        use ActionContext::{Viewport, Workspace};
        use EditorAction::*;
        let key = |key| Shortcut {
            key,
            primary: false,
            shift: false,
        };
        let primary = |key, shift| Shortcut {
            key,
            primary: true,
            shift,
        };
        Self(BTreeMap::from([
            ((Workspace, primary("s", false)), Save),
            ((Workspace, primary("z", false)), Undo),
            ((Workspace, primary("z", true)), Redo),
            ((Workspace, primary("y", false)), Redo),
            ((Viewport, key("escape")), Cancel),
            ((Viewport, key("delete")), Delete),
            ((Viewport, key("f")), FrameSelection),
            ((Viewport, key("q")), Tool(ToolMode::Select)),
            ((Viewport, key("w")), Tool(ToolMode::Move)),
            ((Viewport, key("e")), Tool(ToolMode::Rotate)),
            ((Viewport, key("r")), Tool(ToolMode::Scale)),
            ((Viewport, key("b")), Tool(ToolMode::TileBrush)),
        ]))
    }
}

impl EditorKeymap {
    pub fn bind(&mut self, context: ActionContext, shortcut: Shortcut, action: EditorAction) {
        self.0.insert((context, shortcut), action);
    }

    pub fn resolve(
        &self,
        context: ActionContext,
        key: &str,
        primary: bool,
        shift: bool,
    ) -> Option<EditorAction> {
        let accepts = |(scope, binding): &(ActionContext, Shortcut)| {
            *scope == context
                && binding.key == key
                && binding.primary == primary
                && binding.shift == shift
        };
        self.0
            .iter()
            .find(|(binding, _)| accepts(binding))
            .map(|(_, action)| *action)
            .or_else(|| {
                if context != ActionContext::Viewport {
                    return None;
                }
                self.0
                    .iter()
                    .find(|((scope, binding), _)| {
                        *scope == ActionContext::Workspace
                            && binding.key == key
                            && binding.primary == primary
                            && binding.shift == shift
                    })
                    .map(|(_, action)| *action)
            })
    }
}
