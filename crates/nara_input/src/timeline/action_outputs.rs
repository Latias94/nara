use crate::{
    ActionOutcome, InputIngressError, InputLimits, MAX_BINDINGS_PER_INPUT, SemanticActionOutcome,
};

/// Backing arrays for the next result batch. Published arrays can cover the next batch because
/// frame cleanup returns them before the next resolution. Only ordinary action admission grows
/// this storage. Terminal events spend capacity reserved with their original member.
#[derive(Debug, Default)]
pub(super) struct ActionOutputBuffers {
    bindings: Vec<ActionOutcome>,
    semantic: Vec<SemanticActionOutcome>,
    lent_bindings: usize,
    lent_semantic: usize,
    held_actions: usize,
    #[cfg(test)]
    pub(super) refuse_semantic_growth: bool,
}

impl ActionOutputBuffers {
    #[cfg(test)]
    pub(super) fn capacities(&self) -> (usize, usize) {
        (
            self.bindings.capacity().max(self.lent_bindings),
            self.semantic.capacity().max(self.lent_semantic),
        )
    }

    pub(super) fn reserve(
        &mut self,
        occupied: usize,
        limits: InputLimits,
    ) -> Result<(), InputIngressError> {
        let physical = occupied * MAX_BINDINGS_PER_INPUT;
        let maximum_physical = limits.events() * MAX_BINDINGS_PER_INPUT;
        let semantic = physical + self.held_actions;
        let maximum_semantic = maximum_physical + limits.members() * MAX_BINDINGS_PER_INPUT;
        reserve_output(
            &mut self.bindings,
            self.lent_bindings,
            physical,
            maximum_physical,
        )?;
        #[cfg(test)]
        if self.refuse_semantic_growth
            && semantic > self.semantic.capacity().max(self.lent_semantic)
        {
            return Err(InputIngressError::ActionOutputAllocationRefused);
        }
        reserve_output(
            &mut self.semantic,
            self.lent_semantic,
            semantic,
            maximum_semantic,
        )
    }

    pub(super) fn observe_held_actions(&mut self, count: usize) {
        self.held_actions = count;
    }

    pub(super) fn lend(
        &mut self,
        bindings: &mut Vec<ActionOutcome>,
        semantic: &mut Vec<SemanticActionOutcome>,
    ) {
        if self.bindings.capacity() > bindings.capacity() {
            *bindings = std::mem::take(&mut self.bindings);
        }
        if self.semantic.capacity() > semantic.capacity() {
            *semantic = std::mem::take(&mut self.semantic);
        }
        self.lent_bindings = bindings.capacity();
        self.lent_semantic = semantic.capacity();
    }

    pub(super) fn reclaim(
        &mut self,
        bindings: &mut Vec<ActionOutcome>,
        semantic: &mut Vec<SemanticActionOutcome>,
    ) {
        reclaim_output(&mut self.bindings, bindings);
        reclaim_output(&mut self.semantic, semantic);
        self.lent_bindings = 0;
        self.lent_semantic = 0;
    }
}

fn reserve_output<T>(
    buffer: &mut Vec<T>,
    lent: usize,
    required: usize,
    maximum: usize,
) -> Result<(), InputIngressError> {
    let available = buffer.capacity().max(lent);
    if required > available {
        let target = required.max(available.saturating_mul(2)).min(maximum);
        buffer
            .try_reserve_exact(target)
            .map_err(|_| InputIngressError::ActionOutputAllocationRefused)?;
    }
    Ok(())
}

fn reclaim_output<T>(buffer: &mut Vec<T>, published: &mut Vec<T>) {
    let published = std::mem::take(published);
    if published.capacity() > buffer.capacity() {
        *buffer = published;
    }
}
