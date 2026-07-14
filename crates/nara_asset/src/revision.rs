use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct RevisionIdentity {
    store: Arc<()>,
    generation: Option<Arc<()>>,
    revision: u64,
}

impl PartialEq for RevisionIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.store, &other.store)
            && self.revision == other.revision
            && match (&self.generation, &other.generation) {
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                (None, None) => true,
                (Some(_), None) | (None, Some(_)) => false,
            }
    }
}

impl Eq for RevisionIdentity {}

impl RevisionIdentity {
    pub(crate) fn capture(store: &Arc<()>, entry: Option<&RevisionCounter>) -> Self {
        let (generation, revision) = entry.map_or((None, 0), |entry| {
            (Some(Arc::clone(&entry.generation)), entry.revision)
        });
        Self {
            store: Arc::clone(store),
            generation,
            revision,
        }
    }
}

#[derive(Debug)]
pub(crate) struct RevisionCounter {
    generation: Arc<()>,
    revision: u64,
}

impl Default for RevisionCounter {
    fn default() -> Self {
        Self {
            generation: Arc::new(()),
            revision: 0,
        }
    }
}

impl RevisionCounter {
    pub(crate) fn advance(&mut self) {
        if let Some(revision) = self.revision.checked_add(1) {
            self.revision = revision;
        } else {
            self.generation = Arc::new(());
            self.revision = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_rotates_generation_instead_of_reusing_an_exhausted_identity() {
        let store = Arc::new(());
        let mut counter = RevisionCounter {
            generation: Arc::new(()),
            revision: u64::MAX,
        };
        let exhausted = RevisionIdentity::capture(&store, Some(&counter));

        counter.advance();

        assert!(RevisionIdentity::capture(&store, Some(&counter)) != exhausted);
    }
}
