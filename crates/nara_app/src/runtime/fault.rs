use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::{Arc, OnceLock},
};

use nara_ecs::{
    Resource,
    bevy_ecs::error::{BevyError, ErrorContext, Severity, match_severity},
};
use thiserror::Error;

use crate::{AppRunError, failure_metadata};

/// A read-only check supplied by a domain that owns runtime resources.
pub type RuntimeValidator = dyn Fn(&nara_ecs::World, Option<super::RuntimeGeneration>) -> Result<(), RuntimeFault>
    + Send
    + Sync;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFaultKind {
    AppFrame,
    RuntimeAuthority,
    System,
    RunCondition,
    Command,
    Observer,
    RequiredService,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFaultDetailError {
    #[error("runtime fault code is invalid")]
    InvalidCode,
    #[error("runtime fault summary is invalid")]
    InvalidSummary,
    #[error("runtime fault origin is invalid")]
    InvalidOrigin,
}

/// Source-authored metadata for a runtime failure.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[error("{summary} ({origin}, {code})")]
pub struct RuntimeFaultDetail {
    code: &'static str,
    summary: &'static str,
    origin: &'static str,
}

impl RuntimeFaultDetail {
    pub const MAX_CODE_BYTES: usize = failure_metadata::MAX_CODE_BYTES;
    pub const MAX_SUMMARY_BYTES: usize = failure_metadata::MAX_SUMMARY_BYTES;
    pub const MAX_ORIGIN_BYTES: usize = 96;

    pub fn new(
        code: &'static str,
        summary: &'static str,
        origin: &'static str,
    ) -> Result<Self, RuntimeFaultDetailError> {
        if !failure_metadata::is_valid_identifier(code, Self::MAX_CODE_BYTES, false) {
            return Err(RuntimeFaultDetailError::InvalidCode);
        }
        if !failure_metadata::is_valid_summary(summary) {
            return Err(RuntimeFaultDetailError::InvalidSummary);
        }
        if !failure_metadata::is_valid_identifier(origin, Self::MAX_ORIGIN_BYTES, true) {
            return Err(RuntimeFaultDetailError::InvalidOrigin);
        }
        Ok(Self {
            code,
            summary,
            origin,
        })
    }

    pub const fn code(self) -> &'static str {
        self.code
    }

    pub const fn summary(self) -> &'static str {
        self.summary
    }

    pub const fn origin(self) -> &'static str {
        self.origin
    }

    pub fn into_bevy_error(self) -> BevyError {
        BevyError::error(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeFault {
    kind: RuntimeFaultKind,
    source: &'static str,
    detail: Option<RuntimeFaultDetail>,
    app_error: Option<Box<AppRunError>>,
}

impl RuntimeFault {
    pub const fn engine(kind: RuntimeFaultKind, source: &'static str) -> Self {
        Self {
            kind,
            source,
            detail: None,
            app_error: None,
        }
    }

    pub const fn engine_with_detail(
        kind: RuntimeFaultKind,
        source: &'static str,
        detail: RuntimeFaultDetail,
    ) -> Self {
        Self {
            kind,
            source,
            detail: Some(detail),
            app_error: None,
        }
    }

    pub(crate) fn app(error: AppRunError) -> Self {
        let (kind, source, detail) = error.runtime_fault_parts().unwrap_or((
            RuntimeFaultKind::AppFrame,
            "nara.app.frame",
            None,
        ));
        Self {
            kind,
            source,
            detail,
            app_error: Some(Box::new(error)),
        }
    }

    pub const fn kind(&self) -> RuntimeFaultKind {
        self.kind
    }

    pub const fn source(&self) -> &'static str {
        self.source
    }

    pub const fn detail(&self) -> Option<RuntimeFaultDetail> {
        self.detail
    }

    pub fn app_error(&self) -> Option<&AppRunError> {
        self.app_error.as_deref()
    }
}

/// Shares the first fault with systems and native runtime adapters.
#[derive(Clone, Debug, Default, Resource)]
pub struct RuntimeFaultReporter(Arc<OnceLock<RuntimeFault>>);

impl RuntimeFaultReporter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn report(&self, fault: RuntimeFault) -> bool {
        self.0.set(fault).is_ok()
    }

    pub fn fault(&self) -> Option<RuntimeFault> {
        self.fault_ref().cloned()
    }

    pub(crate) fn fault_ref(&self) -> Option<&RuntimeFault> {
        self.0.get()
    }

    pub(crate) fn record(&self, fault: RuntimeFault) -> RuntimeFault {
        self.0.get_or_init(|| fault).clone()
    }
}

struct ExecutionFault(RuntimeFault);

pub(crate) fn handle_execution_error(error: BevyError, context: ErrorContext) {
    if error.severity() != Severity::Error {
        match_severity(error, context);
        return;
    }
    let kind = match context {
        ErrorContext::System { .. } => RuntimeFaultKind::System,
        ErrorContext::RunCondition { .. } => RuntimeFaultKind::RunCondition,
        ErrorContext::Command { .. } => RuntimeFaultKind::Command,
        ErrorContext::Observer { .. } => RuntimeFaultKind::Observer,
    };
    let source = "nara.ecs.fallible-execution";
    let fault = match error.downcast_ref::<RuntimeFaultDetail>() {
        Some(detail) => RuntimeFault::engine_with_detail(kind, source, *detail),
        None => RuntimeFault::engine(kind, source),
    };
    resume_unwind(Box::new(ExecutionFault(fault)));
}

pub(crate) fn execute<R>(run: impl FnOnce() -> R) -> Result<R, RuntimeFault> {
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(output) => Ok(output),
        Err(payload) => match payload.downcast::<ExecutionFault>() {
            Ok(fault) => Err(fault.0),
            Err(payload) => resume_unwind(payload),
        },
    }
}
