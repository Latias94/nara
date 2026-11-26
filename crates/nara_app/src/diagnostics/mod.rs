//! Routes runtime diagnostics to the configured tracing subscriber.

use nara_core::{Diagnostic, DiagnosticSeverity};
use nara_ecs::bevy_ecs::{
    event::{Event, GlobalTrigger},
    observer::On,
};

use crate::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};

pub struct DiagnosticEvent(pub Diagnostic);

impl Event for DiagnosticEvent {
    type Trigger<'a> = GlobalTrigger;
}

#[derive(Debug, Default)]
pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PluginId::new("nara.diagnostics"), PluginCategory::Service);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_observer(|event: On<DiagnosticEvent>| emit_diagnostic_to_tracing(&event.0))?;
        Ok(())
    }
}

pub fn emit_diagnostic_to_tracing(diagnostic: &Diagnostic) {
    let code = diagnostic.code.as_deref();
    let location = diagnostic.location.as_ref();
    let message = &diagnostic.message;
    match diagnostic.severity {
        DiagnosticSeverity::Error => {
            tracing::error!(target: "nara.diagnostics", code, ?location, "{message}")
        }
        DiagnosticSeverity::Warning => {
            tracing::warn!(target: "nara.diagnostics", code, ?location, "{message}")
        }
        DiagnosticSeverity::Info => {
            tracing::info!(target: "nara.diagnostics", code, ?location, "{message}")
        }
    }
}
