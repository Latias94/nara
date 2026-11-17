//! Canonical logical cost over borrowed component values.

use crate::{ComponentValueCost, ComponentValueKind};

use super::{ComponentListIter, ComponentMapIter, ComponentValueRef};

enum Frame<'a> {
    List(ComponentListIter<'a>),
    Map(ComponentMapIter<'a>),
}

impl ComponentValueRef<'_> {
    #[must_use]
    pub fn cost(self) -> ComponentValueCost {
        self.cost_up_to_impl(usize::MAX, usize::MAX, ComponentValueCost::ZERO, None)
            .1
    }

    #[must_use]
    pub fn cost_up_to(self, nodes: usize, bytes: usize) -> Option<ComponentValueCost> {
        let (accepted, cost) = self.cost_up_to_impl(nodes, bytes, ComponentValueCost::ZERO, None);
        accepted.then_some(cost)
    }

    /// Admits traversal scratch before each replacement allocation and retains exact refusal.
    pub fn cost_up_to_with_admission<E>(
        self,
        nodes: usize,
        bytes: usize,
        admit: impl FnMut(Option<usize>) -> Result<(), E>,
    ) -> Result<Option<ComponentValueCost>, E> {
        self.cost_up_to_with_admission_measured(nodes, bytes, admit)
            .0
    }

    /// Returns the same result and the actually visited prefix, including refused attempts.
    pub fn cost_up_to_with_admission_measured<E>(
        self,
        nodes: usize,
        bytes: usize,
        mut admit: impl FnMut(Option<usize>) -> Result<(), E>,
    ) -> (Result<Option<ComponentValueCost>, E>, ComponentValueCost) {
        let mut refusal = None;
        let mut callback = |allocation| match admit(allocation) {
            Ok(()) => true,
            Err(error) => {
                refusal = Some(error);
                false
            }
        };
        let (accepted, cost) =
            self.cost_up_to_impl(nodes, bytes, ComponentValueCost::ZERO, Some(&mut callback));
        (
            match refusal {
                Some(error) => Err(error),
                None => Ok(accepted.then_some(cost)),
            },
            cost,
        )
    }

    pub(crate) fn cost_up_to_impl(
        self,
        max_nodes: usize,
        max_bytes: usize,
        initial: ComponentValueCost,
        mut admit: Option<&mut dyn FnMut(Option<usize>) -> bool>,
    ) -> (bool, ComponentValueCost) {
        if max_nodes == 0 {
            return (false, initial);
        }
        let mut nodes = initial.nodes();
        let mut bytes = initial.logical_bytes();
        let mut pending = Vec::new();
        let mut current = Some((0_usize, self));
        let mut minimum_nodes = 1_usize;
        while let Some((key_bytes, value)) = current {
            bytes = bytes.saturating_add(key_bytes);
            let (observed_nodes, observed_bytes, children) = match value.kind() {
                ComponentValueKind::Null => (1, 0, 0),
                ComponentValueKind::Bool => (1, 1, 0),
                ComponentValueKind::I64 | ComponentValueKind::U64 | ComponentValueKind::F64 => {
                    (1, 8, 0)
                }
                ComponentValueKind::String => {
                    (1, value.as_str().expect("String kind checked").len(), 0)
                }
                ComponentValueKind::List => {
                    (1, 0, value.as_list().expect("List kind checked").len())
                }
                ComponentValueKind::Map => (1, 0, value.as_map().expect("Map kind checked").len()),
                ComponentValueKind::EntityRef => {
                    let cost = value
                        .as_entity_reference()
                        .expect("reference kind checked")
                        .cost();
                    (cost.nodes(), cost.logical_bytes(), 0)
                }
                ComponentValueKind::AssetRef => {
                    unreachable!("asset references use ordinary component values")
                }
            };
            nodes = nodes.saturating_add(observed_nodes);
            bytes = bytes.saturating_add(observed_bytes);
            minimum_nodes = minimum_nodes
                .saturating_add(observed_nodes.saturating_sub(1))
                .saturating_add(children);
            if minimum_nodes > max_nodes || bytes > max_bytes {
                return (false, ComponentValueCost::from_parts(nodes, bytes));
            }
            if children != 0 {
                if let Some(admit) = admit.as_deref_mut()
                    && pending.len() == pending.capacity()
                {
                    let capacity = pending
                        .len()
                        .checked_add(1)
                        .map(|required| required.max(pending.capacity().saturating_mul(2)).max(4));
                    if !admit(capacity.and_then(|capacity| {
                        capacity.checked_mul(std::mem::size_of::<Frame<'_>>())
                    })) {
                        return (false, ComponentValueCost::from_parts(nodes, bytes));
                    }
                    let Some(capacity) = capacity else {
                        return (false, ComponentValueCost::from_parts(nodes, bytes));
                    };
                    pending.reserve_exact(capacity - pending.len());
                }
                if let Some(items) = value.as_list() {
                    pending.push(Frame::List(items.iter()));
                } else if let Some(fields) = value.as_map() {
                    pending.push(Frame::Map(fields.iter()));
                }
            }
            current = loop {
                let next = match pending.last_mut() {
                    Some(Frame::List(items)) => items.next().map(|value| (0, value)),
                    Some(Frame::Map(fields)) => {
                        fields.next().map(|(name, value)| (name.len(), value))
                    }
                    None => break None,
                };
                if next.is_some() {
                    break next;
                }
                pending.pop();
            };
        }
        (true, ComponentValueCost::from_parts(nodes, bytes))
    }
}
