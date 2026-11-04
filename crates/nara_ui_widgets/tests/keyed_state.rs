use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use nara_ui_kernel::{MAX_UI_KEY_BYTES, UiKey, UiKeyError, UiOpaqueId, UiRootKey};
use nara_ui_widgets::{UiStateBinding, UiStateChanges, UiStateError, UiStateStore};

fn root() -> UiRootKey {
    UiRootKey::runtime("keyed-state-tests", "panel").unwrap()
}

fn binding(owner: &str, schema: u64) -> UiStateBinding {
    UiStateBinding::new(UiOpaqueId::new(owner.as_bytes()).unwrap(), schema)
}

// Deliberately lacks Clone, Debug and PartialEq to check handle trait bounds as well.
struct Tracked {
    value: usize,
    drops: Rc<Cell<usize>>,
}

impl Drop for Tracked {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[derive(Debug)]
enum InitError {
    State(UiStateError),
    Rejected,
}

impl From<UiStateError> for InitError {
    fn from(error: UiStateError) -> Self {
        Self::State(error)
    }
}

#[test]
fn reuse_is_lazy_across_reorder_and_insertion_and_preserves_updates() {
    let mut store = UiStateStore::new(root(), 3).unwrap();
    assert!(store.is_empty());
    assert_eq!(store.last_changes(), UiStateChanges::default());
    let left_key = store.root().child("left").unwrap();
    let right_key = store.root().child("right").unwrap();
    let inserted_key = store.root().child("inserted").unwrap();
    let bound = binding("document", 1);
    let calls = Cell::new(0);
    let mut pass = store.begin();
    assert_eq!(pass.root().root_key(), &root());
    let left = pass
        .try_state::<_, UiStateError>(left_key.clone(), bound.clone(), || {
            calls.set(calls.get() + 1);
            Ok(String::from("left draft"))
        })
        .unwrap();
    let right = pass
        .try_state::<_, UiStateError>(right_key.clone(), bound.clone(), || {
            calls.set(calls.get() + 1);
            Ok(String::from("right draft"))
        })
        .unwrap();
    assert_eq!(pass.read(&left).unwrap(), "left draft");
    assert_eq!(
        pass.finish().unwrap(),
        UiStateChanges {
            initialized: 2,
            live: 2,
            ..Default::default()
        }
    );
    *store.get_mut(&left).unwrap() = String::from("edited draft");
    let mut pass = store.begin();
    let right_again = pass
        .try_state::<String, UiStateError>(right_key.clone(), bound.clone(), || {
            panic!("reused state must not initialize")
        })
        .unwrap();
    let inserted = pass
        .try_state::<_, UiStateError>(inserted_key.clone(), bound.clone(), || {
            calls.set(calls.get() + 1);
            Ok(String::from("inserted draft"))
        })
        .unwrap();
    let left_again = pass
        .try_state::<String, UiStateError>(left_key.clone(), bound.clone(), || {
            panic!("reordered state must not initialize")
        })
        .unwrap();
    assert_eq!(right_again, right);
    assert_eq!(left_again, left);
    assert_eq!(pass.read(&left).unwrap(), "edited draft");
    assert_eq!(pass.read(&inserted).unwrap(), "inserted draft");
    let changes = pass.finish().unwrap();
    assert_eq!(
        changes,
        UiStateChanges {
            reused: 2,
            initialized: 1,
            live: 3,
            ..Default::default()
        }
    );
    assert_eq!(store.last_changes(), changes);
    assert_eq!(store.len(), 3);
    assert_eq!(calls.get(), 3);

    let mut pass = store.begin();
    for key in [left_key, inserted_key, right_key] {
        pass.try_state::<String, UiStateError>(key, bound.clone(), || {
            panic!("a repeated composition must not initialize")
        })
        .unwrap();
    }
    assert_eq!(pass.finish().unwrap().reused, 3);
    assert_eq!(calls.get(), 3);
    assert_eq!(store.get(&right).unwrap(), "right draft");
}

#[test]
fn exact_paths_keep_parent_leaf_and_delimiter_collisions_separate() {
    let mut store = UiStateStore::new(root(), 6).unwrap();
    let base = store.root();
    let keys = [
        base.child("ab").unwrap().child("c").unwrap(),
        base.child("a").unwrap().child("bc").unwrap(),
        base.child("row/name").unwrap(),
        base.child("row").unwrap().child("name").unwrap(),
        base.child("parent-a").unwrap().child("value").unwrap(),
        base.child("parent-b").unwrap().child("value").unwrap(),
    ];
    let mut pass = store.begin();
    let mut handles = Vec::new();
    for (index, key) in keys.iter().enumerate() {
        for earlier in &keys[..index] {
            assert_ne!(key, earlier);
        }
        handles.push(
            pass.try_state::<_, UiStateError>(key.clone(), binding("same owner", 1), || Ok(index))
                .unwrap(),
        );
    }
    assert_eq!(pass.finish().unwrap().live, keys.len());
    for (index, handle) in handles.iter().enumerate() {
        assert_eq!(handle.key(), &keys[index]);
        assert_eq!(*store.get(handle).unwrap(), index);
    }
    let other =
        UiStateStore::new(UiRootKey::runtime("keyed-state-tests", "other").unwrap(), 6).unwrap();
    assert_ne!(base, other.root());
    assert_ne!(keys[2], other.root().child("row/name").unwrap());
}

#[test]
fn path_limits_charge_the_complete_root_and_each_utf8_atom_length() {
    let store = UiStateStore::new(root(), 1).unwrap();
    let base = store.root();
    let root_bytes = UiKey::root(root()).unwrap().encoded_len();
    let available = MAX_UI_KEY_BYTES - root_bytes - 2;
    let full = base.child("x".repeat(available)).unwrap();
    assert_eq!(full.root_key(), &root());
    assert!(matches!(
        base.child("x".repeat(available + 1)),
        Err(UiStateError::PathTooLong { actual, maximum })
            if actual == MAX_UI_KEY_BYTES + 1 && maximum == MAX_UI_KEY_BYTES
    ));
    assert!(matches!(
        full.child("a"),
        Err(UiStateError::PathTooLong { actual, .. }) if actual == MAX_UI_KEY_BYTES + 3
    ));
    let unicode = base.child("层").unwrap();
    let remaining = MAX_UI_KEY_BYTES - root_bytes - 2 - "层".len() - 2;
    unicode.child("x".repeat(remaining)).unwrap();
    assert!(matches!(
        unicode.child("x".repeat(remaining + 1)),
        Err(UiStateError::PathTooLong { .. })
    ));
    assert!(matches!(
        base.child(""),
        Err(UiStateError::Key(UiKeyError::EmptyAtom))
    ));
    assert!(matches!(
        base.child("line\nbreak"),
        Err(UiStateError::Key(UiKeyError::ControlCharacter))
    ));
}

#[test]
fn swallowed_duplicate_poisons_the_pass_and_drops_only_staged_values() {
    let drops = Rc::new(Cell::new(0));
    let mut store = UiStateStore::new(root(), 3).unwrap();
    let key = store.root().child("retained").unwrap();
    let staged_key = store.root().child("staged").unwrap();
    let bound = binding("document", 1);
    let mut pass = store.begin();
    let retained = pass
        .try_state::<_, UiStateError>(key.clone(), bound.clone(), || {
            Ok(Tracked {
                value: 17,
                drops: drops.clone(),
            })
        })
        .unwrap();
    pass.finish().unwrap();
    assert_eq!(retained.clone(), retained);
    let previous = store.last_changes();
    let mut pass = store.begin();
    pass.try_state::<Tracked, UiStateError>(key.clone(), bound.clone(), || {
        panic!("retained state must not initialize")
    })
    .unwrap();
    let staged = pass
        .try_state::<_, UiStateError>(staged_key, bound.clone(), || {
            Ok(Tracked {
                value: 23,
                drops: drops.clone(),
            })
        })
        .unwrap();
    assert!(matches!(
        pass.replace::<Tracked, UiStateError>(key.clone(), bound.clone(), || {
            panic!("duplicate detection must precede initialization")
        }),
        Err(UiStateError::DuplicateKey { key: failed }) if failed == key
    ));
    assert!(matches!(pass.read(&retained), Err(UiStateError::Aborted)));
    assert!(matches!(
        pass.try_state::<Tracked, UiStateError>(key, bound, || panic!("poisoned pass")),
        Err(UiStateError::Aborted)
    ));
    assert!(matches!(pass.finish(), Err(UiStateError::Aborted)));
    assert_eq!(drops.get(), 1);
    assert_eq!(store.len(), 1);
    assert_eq!(store.last_changes(), previous);
    assert_eq!(store.get(&retained).unwrap().value, 17);
    assert!(matches!(
        store.get(&staged),
        Err(UiStateError::StaleHandle { .. })
    ));
    drop(store);
    assert_eq!(drops.get(), 2);
}

#[test]
fn failed_initializer_preserves_old_membership_and_allows_an_explicit_retry() {
    let drops = Rc::new(Cell::new(0));
    let mut store = UiStateStore::new(root(), 2).unwrap();
    let key = store.root().child("retained").unwrap();
    let added_key = store.root().child("added").unwrap();
    let bound = binding("document", 1);
    let mut pass = store.begin();
    let retained = pass
        .try_state::<_, UiStateError>(key.clone(), bound.clone(), || {
            Ok(Tracked {
                value: 7,
                drops: drops.clone(),
            })
        })
        .unwrap();
    pass.finish().unwrap();
    let previous = store.last_changes();
    let mut pass = store.begin();
    pass.try_state::<_, UiStateError>(added_key.clone(), bound.clone(), || {
        Ok(Tracked {
            value: 8,
            drops: drops.clone(),
        })
    })
    .unwrap();
    assert!(matches!(
        pass.replace::<Tracked, InitError>(key.clone(), bound.clone(), || {
            Err(InitError::Rejected)
        }),
        Err(InitError::Rejected)
    ));
    assert!(matches!(
        pass.try_state::<Tracked, InitError>(key.clone(), bound.clone(), || {
            panic!("a swallowed initializer error must poison the pass")
        }),
        Err(InitError::State(UiStateError::Aborted))
    ));
    assert!(matches!(pass.finish(), Err(UiStateError::Aborted)));
    assert_eq!(store.len(), 1);
    assert_eq!(store.get(&retained).unwrap().value, 7);
    assert_eq!(store.last_changes(), previous);
    assert_eq!(drops.get(), 1);

    let mut pass = store.begin();
    let replacement = pass
        .replace::<_, InitError>(key, bound.clone(), || {
            Ok(Tracked {
                value: 9,
                drops: drops.clone(),
            })
        })
        .unwrap();
    let added = pass
        .try_state::<_, InitError>(added_key, bound, || {
            Ok(Tracked {
                value: 10,
                drops: drops.clone(),
            })
        })
        .unwrap();
    assert_eq!(pass.read(&replacement).unwrap().value, 9);
    assert_eq!(pass.read(&added).unwrap().value, 10);
    assert_eq!(drops.get(), 1);
    assert_eq!(
        pass.finish().unwrap(),
        UiStateChanges {
            reused: 0,
            initialized: 2,
            replaced: 1,
            retired: 1,
            live: 2,
        }
    );
    assert_eq!(drops.get(), 2);
    assert!(matches!(
        store.get(&retained),
        Err(UiStateError::StaleHandle { .. })
    ));
    assert_eq!(store.get(&replacement).unwrap().value, 9);
    drop(store);
    assert_eq!(drops.get(), 4);
}

#[test]
fn type_binding_and_schema_mismatches_require_explicit_replacement() {
    let mut store = UiStateStore::new(root(), 1).unwrap();
    let key = store.root().child("value").unwrap();
    let bound = binding("document-a", 1);
    let mut pass = store.begin();
    let original = pass
        .try_state::<_, UiStateError>(key.clone(), bound.clone(), || Ok(String::from("draft")))
        .unwrap();
    pass.finish().unwrap();

    let mut pass = store.begin();
    assert!(matches!(
        pass.try_state::<u32, UiStateError>(key.clone(), bound.clone(), || {
            panic!("an incompatible type must not initialize implicitly")
        }),
        Err(UiStateError::IncompatibleState { .. })
    ));
    assert!(matches!(pass.finish(), Err(UiStateError::Aborted)));
    for incompatible in [binding("document-b", 1), binding("document-a", 2)] {
        let mut pass = store.begin();
        assert!(matches!(
            pass.try_state::<String, UiStateError>(key.clone(), incompatible, || {
                panic!("an incompatible binding/schema must not initialize implicitly")
            }),
            Err(UiStateError::IncompatibleState { .. })
        ));
        assert!(matches!(pass.finish(), Err(UiStateError::Aborted)));
        assert_eq!(store.get(&original).unwrap(), "draft");
    }

    let mut pass = store.begin();
    let same_type = pass
        .replace::<_, UiStateError>(key.clone(), bound, || Ok(String::from("fresh")))
        .unwrap();
    assert_ne!(same_type, original);
    let expected = UiStateChanges {
        initialized: 1,
        replaced: 1,
        retired: 1,
        live: 1,
        ..Default::default()
    };
    assert_eq!(pass.finish().unwrap(), expected);
    assert!(matches!(
        store.get(&original),
        Err(UiStateError::StaleHandle { .. })
    ));
    assert!(matches!(
        store.get_mut(&original),
        Err(UiStateError::StaleHandle { .. })
    ));

    let mut pass = store.begin();
    let changed_type = pass
        .replace::<_, UiStateError>(key, binding("document-b", 2), || Ok(42_u32))
        .unwrap();
    assert_eq!(pass.finish().unwrap(), expected);
    assert!(matches!(
        store.get(&same_type),
        Err(UiStateError::StaleHandle { .. })
    ));
    assert_eq!(*store.get(&changed_type).unwrap(), 42);
}

#[test]
fn capacity_bounds_selected_membership_and_allows_a_full_new_set() {
    assert!(matches!(
        UiStateStore::new(root(), 0),
        Err(UiStateError::InvalidLimit)
    ));
    let mut store = UiStateStore::new(root(), 2).unwrap();
    let keys = ["a", "b", "c", "d"].map(|leaf| store.root().child(leaf).unwrap());
    let bound = binding("document", 1);
    let mut pass = store.begin();
    let a = pass
        .try_state::<_, UiStateError>(keys[0].clone(), bound.clone(), || Ok(1_usize))
        .unwrap();
    let b = pass
        .try_state::<_, UiStateError>(keys[1].clone(), bound.clone(), || Ok(2_usize))
        .unwrap();
    pass.finish().unwrap();
    let previous = store.last_changes();
    let mut pass = store.begin();
    pass.try_state::<usize, UiStateError>(keys[0].clone(), bound.clone(), || panic!("reuse"))
        .unwrap();
    let staged = pass
        .try_state::<_, UiStateError>(keys[2].clone(), bound.clone(), || Ok(3_usize))
        .unwrap();
    assert!(matches!(
        pass.try_state::<usize, UiStateError>(keys[3].clone(), bound.clone(), || {
            panic!("capacity must be checked before initialization")
        }),
        Err(UiStateError::Capacity { maximum: 2 })
    ));
    assert!(matches!(pass.finish(), Err(UiStateError::Aborted)));
    assert_eq!(store.last_changes(), previous);
    assert_eq!(*store.get(&a).unwrap(), 1);
    assert_eq!(*store.get(&b).unwrap(), 2);
    assert!(matches!(
        store.get(&staged),
        Err(UiStateError::StaleHandle { .. })
    ));

    let mut pass = store.begin();
    for (index, key) in keys[2..].iter().enumerate() {
        pass.try_state::<_, UiStateError>(key.clone(), bound.clone(), || Ok(index))
            .unwrap();
    }
    assert_eq!(
        pass.finish().unwrap(),
        UiStateChanges {
            initialized: 2,
            retired: 2,
            live: 2,
            ..Default::default()
        }
    );
    assert!(matches!(
        store.get(&a),
        Err(UiStateError::StaleHandle { .. })
    ));
    assert!(matches!(
        store.get(&b),
        Err(UiStateError::StaleHandle { .. })
    ));
}

#[test]
fn dropped_passes_never_reuse_incarnations_or_destroy_committed_state() {
    let drops = Rc::new(Cell::new(0));
    let mut store = UiStateStore::new(root(), 1).unwrap();
    let key = store.root().child("value").unwrap();
    let bound = binding("document", 1);
    let mut pass = store.begin();
    let abandoned = pass
        .try_state::<_, UiStateError>(key.clone(), bound.clone(), || {
            Ok(Tracked {
                value: 1,
                drops: drops.clone(),
            })
        })
        .unwrap();
    assert_eq!(pass.read(&abandoned).unwrap().value, 1);
    drop(pass);
    assert_eq!(drops.get(), 1);
    assert!(store.is_empty());
    assert_eq!(store.last_changes(), UiStateChanges::default());
    let mut pass = store.begin();
    let retained = pass
        .try_state::<_, UiStateError>(key.clone(), bound.clone(), || {
            Ok(Tracked {
                value: 2,
                drops: drops.clone(),
            })
        })
        .unwrap();
    assert_ne!(retained, abandoned);
    pass.finish().unwrap();
    assert!(matches!(
        store.get(&abandoned),
        Err(UiStateError::StaleHandle { .. })
    ));
    let previous = store.last_changes();
    let mut pass = store.begin();
    let abandoned_replacement = pass
        .replace::<_, UiStateError>(key.clone(), bound.clone(), || {
            Ok(Tracked {
                value: 3,
                drops: drops.clone(),
            })
        })
        .unwrap();
    assert_eq!(pass.read(&abandoned_replacement).unwrap().value, 3);
    drop(pass);
    assert_eq!(drops.get(), 2);
    assert_eq!(store.last_changes(), previous);
    assert_eq!(store.get(&retained).unwrap().value, 2);
    assert!(matches!(
        store.get(&abandoned_replacement),
        Err(UiStateError::StaleHandle { .. })
    ));
    let mut pass = store.begin();
    let replacement = pass
        .replace::<_, UiStateError>(key, bound, || {
            Ok(Tracked {
                value: 4,
                drops: drops.clone(),
            })
        })
        .unwrap();
    assert_ne!(replacement, abandoned_replacement);
    pass.finish().unwrap();
    assert_eq!(drops.get(), 3);
    assert!(matches!(
        store.get(&retained),
        Err(UiStateError::StaleHandle { .. })
    ));

    assert_eq!(
        store.begin().finish().unwrap(),
        UiStateChanges {
            retired: 1,
            ..Default::default()
        }
    );
    assert_eq!(drops.get(), 4);
    assert!(store.is_empty());
    assert!(matches!(
        store.get(&replacement),
        Err(UiStateError::StaleHandle { .. })
    ));
    drop(store);
    assert_eq!(drops.get(), 4);
}

#[test]
fn foreign_roots_stores_and_unselected_reads_cannot_commit() {
    let mut first = UiStateStore::new(root(), 1).unwrap();
    let mut second = UiStateStore::new(root(), 1).unwrap();
    let key = first.root().child("same path").unwrap();
    let bound = binding("same binding", 1);
    let mut pass = first.begin();
    let first_handle = pass
        .try_state::<_, UiStateError>(key.clone(), bound.clone(), || Ok(1_u32))
        .unwrap();
    pass.finish().unwrap();
    let mut pass = second.begin();
    let second_handle = pass
        .try_state::<_, UiStateError>(key.clone(), bound.clone(), || Ok(2_u32))
        .unwrap();
    pass.finish().unwrap();
    assert_eq!(first_handle.key(), second_handle.key());
    assert_ne!(first_handle, second_handle);
    assert!(matches!(
        second.get(&first_handle),
        Err(UiStateError::StaleHandle { .. })
    ));
    assert!(matches!(
        first.get_mut(&second_handle),
        Err(UiStateError::StaleHandle { .. })
    ));

    let previous = second.last_changes();
    let mut pass = second.begin();
    pass.try_state::<u32, UiStateError>(key.clone(), bound.clone(), || panic!("reuse"))
        .unwrap();
    assert_eq!(*pass.read(&second_handle).unwrap(), 2);
    assert!(matches!(
        pass.read(&first_handle),
        Err(UiStateError::StaleHandle { .. })
    ));
    assert!(matches!(pass.finish(), Err(UiStateError::Aborted)));
    assert_eq!(second.last_changes(), previous);

    let pass = first.begin();
    assert!(matches!(
        pass.read(&first_handle),
        Err(UiStateError::StaleHandle { .. })
    ));
    assert!(matches!(pass.finish(), Err(UiStateError::Aborted)));
    assert_eq!(*first.get(&first_handle).unwrap(), 1);

    let other_root = UiRootKey::runtime("keyed-state-tests", "foreign").unwrap();
    let other = UiStateStore::new(other_root, 1).unwrap();
    let foreign_key = other.root().child("same path").unwrap();
    let mut pass = first.begin();
    assert!(matches!(
        pass.try_state::<u32, UiStateError>(foreign_key.clone(), bound, || {
            panic!("a foreign root must not initialize")
        }),
        Err(UiStateError::ForeignRoot { key: failed, expected })
            if failed == foreign_key && expected == root()
    ));
    assert!(matches!(pass.finish(), Err(UiStateError::Aborted)));
    assert_eq!(*first.get(&first_handle).unwrap(), 1);
}

#[test]
fn caught_initializer_unwind_poisons_new_and_replacement_passes_without_reviving_handles() {
    for replace in [false, true] {
        let drops = Rc::new(Cell::new(0));
        let mut store = UiStateStore::new(root(), 3).unwrap();
        let key = store.root().child("retained").unwrap();
        let staged_key = store.root().child("staged").unwrap();
        let panic_key = store.root().child("panic").unwrap();
        let bound = binding("document", 1);
        let mut pass = store.begin();
        let retained = pass
            .try_state::<_, UiStateError>(key.clone(), bound.clone(), || {
                Ok(Tracked {
                    value: 7,
                    drops: drops.clone(),
                })
            })
            .unwrap();
        pass.finish().unwrap();
        let previous = store.last_changes();
        let mut pass = store.begin();
        let staged = pass
            .try_state::<_, UiStateError>(staged_key.clone(), bound.clone(), || {
                Ok(Tracked {
                    value: 8,
                    drops: drops.clone(),
                })
            })
            .unwrap();
        let panic = catch_unwind(AssertUnwindSafe(|| {
            if replace {
                pass.replace::<Tracked, UiStateError>(key.clone(), bound.clone(), || {
                    panic!("injected replacement initializer unwind")
                })
            } else {
                pass.try_state::<Tracked, UiStateError>(panic_key, bound.clone(), || {
                    panic!("injected new initializer unwind")
                })
            }
        }));
        assert!(panic.is_err());
        assert!(matches!(pass.finish(), Err(UiStateError::Aborted)));
        assert_eq!(store.last_changes(), previous);
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(&retained).unwrap().value, 7);
        assert!(matches!(
            store.get(&staged),
            Err(UiStateError::StaleHandle { .. })
        ));
        assert_eq!(drops.get(), 1);
        let mut pass = store.begin();
        pass.try_state::<Tracked, UiStateError>(key, bound.clone(), || panic!("reuse"))
            .unwrap();
        let retried = pass
            .try_state::<_, UiStateError>(staged_key, bound, || {
                Ok(Tracked {
                    value: 9,
                    drops: drops.clone(),
                })
            })
            .unwrap();
        assert_ne!(retried, staged);
        pass.finish().unwrap();
        assert!(store.get(&staged).is_err());
        assert_eq!(store.get(&retried).unwrap().value, 9);
    }
}
