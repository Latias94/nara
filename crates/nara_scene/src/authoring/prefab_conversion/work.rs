//! One finite conversion allowance, including compiler work and later owning copies.

use nara_core::ByteLimit;

use crate::ScenePatchReport;

pub(crate) struct ConversionWork {
    maximum: usize,
    used: usize,
}

impl ConversionWork {
    pub(super) fn after_compilation(
        maximum: ByteLimit,
        compiler_bytes: usize,
        dependency_bytes: usize,
    ) -> Result<Self, ScenePatchReport> {
        let mut work = Self {
            maximum: maximum.get(),
            used: 0,
        };
        work.reserve("selected-compilation", Some(compiler_bytes))?;
        work.reserve("dependency-snapshots", Some(dependency_bytes))?;
        Ok(work)
    }

    /// Accounting is cumulative: dropping finite scratch does not restore allowance.
    pub(crate) fn reserve(
        &mut self,
        stage: &'static str,
        bytes: Option<usize>,
    ) -> Result<(), ScenePatchReport> {
        let next = bytes.and_then(|bytes| self.used.checked_add(bytes));
        let Some(next) = next.filter(|next| *next <= self.maximum) else {
            let mut diagnostics = nara_diagnostic::DiagnosticReport::default();
            diagnostics.push(crate::diagnostics::with_public_locator(
                crate::diagnostics::error(
                    "scene.authoring-prefab-conversion-copy-limit",
                    "Conversion refused before its next owning copy",
                ),
                "copy-stage",
                stage,
            ));
            return Err(ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics,
            });
        };
        self.used = next;
        Ok(())
    }
}

#[cfg(test)]
std::thread_local! {
    static COPIES: std::cell::RefCell<Vec<&'static str>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

pub(crate) fn copying(stage: &'static str) {
    #[cfg(test)]
    COPIES.with(|copies| copies.borrow_mut().push(stage));
    #[cfg(not(test))]
    let _ = stage;
}

#[cfg(test)]
pub(super) fn take_copies() -> Vec<&'static str> {
    COPIES.with(|copies| std::mem::take(&mut *copies.borrow_mut()))
}
