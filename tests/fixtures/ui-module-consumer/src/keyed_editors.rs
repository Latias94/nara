//! An independently authored composite. Only public text, kernel and widget interfaces are used.

use nara_text::TextEngine;
use nara_ui_kernel::{
    UiCustomLayout, UiDimension, UiEdges, UiLayer, UiLocalKey, UiNode, UiOpaqueId, UiRoot,
    UiRootKey, UiStyle, UiText,
};
use nara_ui_widgets::{
    UiStateBinding, UiStateHandle, UiStateKey, UiStatePass, UiStateStore, UiTextField,
};

use crate::StatusLabelError;
use crate::measured_column::MeasuredColumn;

/// Domain identity and initial data are distinct from the shared accessible label.
pub struct EditorRow<'a> {
    pub id: &'a str,
    pub document: &'a str,
    pub schema: u64,
    pub initial: &'a str,
    pub replace: bool,
}

/// A library can compose keyed local controls without owning journal or platform services.
pub struct KeyedEditors {
    pub state: UiStateStore,
    layout: UiCustomLayout,
}

impl KeyedEditors {
    pub fn new(root: UiRootKey) -> Result<Self, StatusLabelError> {
        Ok(Self {
            state: UiStateStore::new(root, 32)?,
            layout: UiCustomLayout::new(MeasuredColumn { gap: 8.0 }),
        })
    }

    /// A complete active membership pass. Omitted rows discard local drafts and preedit.
    ///
    /// This fixture deliberately chooses eviction instead of virtualized pinning. Repeatable
    /// composition does no document mutation, task spawning or subscription installation.
    pub fn compose(
        &mut self,
        rows: &[EditorRow<'_>],
        engine: &mut TextEngine,
    ) -> Result<(UiRoot, Vec<UiStateHandle<UiTextField>>), StatusLabelError> {
        let mut pass = self.state.begin();
        let parent = pass.root().child("editors")?;
        let mut root = UiRoot::new(
            parent.root_key().clone(),
            UiLayer::Base,
            UiStyle::fill().with_padding(UiEdges::all(12.0)?),
        )
        .with_custom_layout(self.layout.clone())
        .with_child(
            UiNode::new(UiStyle::flex())
                .with_key(UiLocalKey::runtime("independent-column", "title")?)
                .with_text(UiText::new("Keyed editors", 16.0)?),
        );
        let mut fields = Vec::new();
        for row in rows {
            let (field, node) = compose_row(&mut pass, &parent, row, engine)?;
            root = root.with_child(node);
            fields.push(field);
        }
        pass.finish()?;
        Ok((root, fields))
    }
}

/// Reusable row content receives a parent scope, not a global field/token map or a row-index ID.
fn compose_row(
    pass: &mut UiStatePass<'_>,
    parent: &UiStateKey,
    row: &EditorRow<'_>,
    engine: &mut TextEngine,
) -> Result<(UiStateHandle<UiTextField>, UiNode), StatusLabelError> {
    let key = parent.child(row.id)?.child("value")?;
    let root = parent.root_key().clone();
    let binding = UiStateBinding::new(UiOpaqueId::new(row.document.as_bytes())?, row.schema);
    let init = || -> Result<_, StatusLabelError> {
        Ok(UiTextField::new(
            root.clone(),
            "Value",
            row.initial,
            18.0,
            engine,
        )?)
    };
    let field = if row.replace {
        pass.replace(key, binding, init)?
    } else {
        pass.try_state(key, binding, init)?
    };
    let node = pass.read(&field)?.authored_node(
        parent.root_key(),
        UiStyle::flex()
            .with_width(UiDimension::Px(220.0))
            .with_height(UiDimension::Px(40.0)),
        220.0,
    )?;
    Ok((field, node))
}
