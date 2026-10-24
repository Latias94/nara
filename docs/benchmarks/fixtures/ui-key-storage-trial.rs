//! Decision-local storage experiment, not a production identity implementation.
//! Compile directly with rustc; no engine dependency, layout or rendering is simulated.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    hint::black_box,
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);

struct ObservedAllocator;

fn record(added: usize, removed: usize) {
    let live = if added >= removed {
        LIVE.fetch_add(added - removed, Ordering::Relaxed) + added - removed
    } else {
        LIVE.fetch_sub(removed - added, Ordering::Relaxed) - (removed - added)
    };
    PEAK.fetch_max(live, Ordering::Relaxed);
}

// SAFETY: System receives the original allocation arguments unchanged. Observation only uses
// atomics and neither allocates nor examines the memory. Counts begin at process startup.
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller supplies the valid layout forwarded to System.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            CALLS.fetch_add(1, Ordering::Relaxed);
            record(layout.size(), 0);
        }
        pointer
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: The live pointer, original layout and new size are forwarded unchanged.
        let replacement = unsafe { System.realloc(pointer, layout, size) };
        if !replacement.is_null() {
            CALLS.fetch_add(1, Ordering::Relaxed);
            record(size, layout.size());
        }
        replacement
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The caller supplies the live pointer and its original layout.
        unsafe { System.dealloc(pointer, layout) };
        record(0, layout.size());
    }
}

trait Address: Clone {
    const NAME: &'static str;
    fn new(context: &Arc<[u8]>, leaf: &[u8]) -> Self;
    fn write(&self, output: &mut [u8; 256]) -> usize;
    fn scoped(_root: &Self, local: &Self, _prefix: usize) -> Self {
        local.clone()
    }
}

fn canonical(context: &[u8], leaf: &[u8], output: &mut [u8; 256]) -> usize {
    let length = 5 + context.len() + leaf.len();
    assert!(length <= output.len());
    output[0] = 0;
    output[1..3].copy_from_slice(&(context.len() as u16).to_le_bytes());
    output[3..3 + context.len()].copy_from_slice(context);
    output[3 + context.len()..length - 2].copy_from_slice(leaf);
    output[length - 2..length].copy_from_slice(&(leaf.len() as u16).to_le_bytes());
    length
}

#[derive(Clone)]
struct Flat(Arc<Vec<u8>>);

impl Address for Flat {
    const NAME: &'static str = "complete";
    fn new(context: &Arc<[u8]>, leaf: &[u8]) -> Self {
        let mut bytes = [0; 256];
        let length = canonical(context, leaf, &mut bytes);
        Self(Arc::new(bytes[..length].to_vec()))
    }
    fn write(&self, output: &mut [u8; 256]) -> usize {
        output[..self.0.len()].copy_from_slice(&self.0);
        self.0.len()
    }
}

#[derive(Clone)]
struct Typed(Arc<ContextLeaf>);

struct ContextLeaf {
    context: Arc<[u8]>,
    leaf: Box<[u8]>,
}

impl Address for Typed {
    const NAME: &'static str = "typed-context";
    fn new(context: &Arc<[u8]>, leaf: &[u8]) -> Self {
        Self(Arc::new(ContextLeaf {
            context: Arc::clone(context),
            leaf: leaf.into(),
        }))
    }
    fn write(&self, output: &mut [u8; 256]) -> usize {
        canonical(&self.0.context, &self.0.leaf, output)
    }
}

#[derive(Clone)]
struct Segmented(Arc<Parts>);

struct Parts {
    head: Arc<[u8]>,
    used: usize,
    tail: Box<[u8]>,
}

impl Address for Segmented {
    const NAME: &'static str = "segmented";
    fn new(context: &Arc<[u8]>, leaf: &[u8]) -> Self {
        let mut bytes = [0; 256];
        let length = canonical(context, leaf, &mut bytes);
        Self(Arc::new(Parts {
            head: Arc::from(&bytes[..length]),
            used: length,
            tail: Box::default(),
        }))
    }
    fn write(&self, output: &mut [u8; 256]) -> usize {
        let length = self.0.used + self.0.tail.len();
        output[..self.0.used].copy_from_slice(&self.0.head[..self.0.used]);
        output[self.0.used..length].copy_from_slice(&self.0.tail);
        length
    }
    fn scoped(root: &Self, local: &Self, prefix: usize) -> Self {
        // Tiny framing-only overlap does not justify another allocation. Recomposition of an
        // already shared local must not allocate another suffix or introduce a parent chain.
        if prefix < 2 * size_of::<usize>()
            || (Arc::ptr_eq(&root.0.head, &local.0.head) && local.0.used == prefix)
        {
            return local.clone();
        }
        assert!(
            root.0.tail.is_empty(),
            "root addresses in this probe stay flat"
        );
        let mut bytes = [0; 256];
        let length = local.write(&mut bytes);
        Self(Arc::new(Parts {
            head: Arc::clone(&root.0.head),
            used: prefix,
            tail: bytes[prefix..length].into(),
        }))
    }
}

fn prefix(root: &[u8], local: &[u8]) -> usize {
    root.iter().zip(local).take_while(|(a, b)| a == b).count()
}

fn key<A: Address>(root: &A, local: &A) -> (A, A) {
    let mut root_bytes = [0; 256];
    let mut local_bytes = [0; 256];
    let r = root.write(&mut root_bytes);
    let l = local.write(&mut local_bytes);
    let p = prefix(&root_bytes[..r], &local_bytes[..l]);
    black_box(7 + r + l - p);
    (root.clone(), A::scoped(root, local, p))
}

fn encoded<A: Address>(root: &A, local: &A) -> ([u8; 520], usize) {
    let mut r = [0; 256];
    let mut l = [0; 256];
    let rlen = root.write(&mut r);
    let llen = local.write(&mut l);
    let common = prefix(&r[..rlen], &l[..llen]);
    let suffix = llen - common;
    let mut output = [0; 520];
    output[0] = 1;
    for (slot, value) in [rlen, common, suffix].into_iter().enumerate() {
        output[1 + slot * 2..3 + slot * 2].copy_from_slice(&(value as u16).to_le_bytes());
    }
    output[7..7 + rlen].copy_from_slice(&r[..rlen]);
    output[7 + rlen..7 + rlen + suffix].copy_from_slice(&l[common..llen]);
    (output, 7 + rlen + suffix)
}

fn conformance() {
    let context: Arc<[u8]> = Arc::from(&[9; 120][..]);
    let independent: Arc<[u8]> = Arc::from(context.as_ref());
    let root = Flat::new(&context, b"root");
    let local = Flat::new(&context, b"node");
    let expected = encoded(&root, &local);
    let typed_root = Typed::new(&context, b"root");
    let typed_local = Typed::new(&independent, b"node");
    assert_eq!(expected, encoded(&typed_root, &typed_local));
    let split_root = Segmented::new(&context, b"root");
    let split_local = Segmented::new(&independent, b"node");
    let (_, split_local) = key(&split_root, &split_local);
    assert_eq!(expected, encoded(&split_root, &split_local));
    let (_, again) = key(&split_root, &split_local);
    assert!(Arc::ptr_eq(&split_local.0, &again.0));
    let other_root = Segmented::new(&context, b"else");
    let (_, reparented) = key(&other_root, &split_local);
    let flat_other = Flat::new(&context, b"else");
    assert_eq!(
        encoded(&flat_other, &local),
        encoded(&other_root, &reparented)
    );
    for leaf in [116, 117, 118] {
        let r = Flat::new(&Arc::from(&[1; 1][..]), &vec![1; 123]);
        let l = Flat::new(&Arc::from(&[2; 1][..]), &vec![2; leaf]);
        let (_, length) = encoded(&r, &l);
        assert_eq!(length, 7 + 129 + (6 + leaf) - 3);
        assert_eq!(length <= 256, leaf <= 117);
    }
    let mut bytes = [0; 256];
    let length = split_local.write(&mut bytes);
    let mut split_hash = DefaultHasher::new();
    bytes[..length].hash(&mut split_hash);
    let length = local.write(&mut bytes);
    let mut flat_hash = DefaultHasher::new();
    bytes[..length].hash(&mut flat_hash);
    assert_eq!(flat_hash.finish(), split_hash.finish());
}

fn run<A: Address>(population: usize, case: &str, context_len: usize, path_len: usize) {
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    let begin = Instant::now();
    let context: Arc<[u8]> = Arc::from(vec![0; context_len]);
    let mut leaf = [7; 96];
    leaf[path_len..path_len + 8].copy_from_slice(&0_u64.to_be_bytes());
    let root = A::new(&context, &leaf[..path_len + 8]);
    let locals = (1..population)
        .map(|index| {
            leaf[path_len..path_len + 8].copy_from_slice(&(index as u64).to_be_bytes());
            if case == "foreign-context" {
                let mut foreign = vec![0; context_len];
                foreign[0] = 1;
                foreign[1..9].copy_from_slice(&(index as u64).to_be_bytes());
                A::new(&Arc::from(foreign), &leaf[..path_len + 8])
            } else {
                A::new(&context, &leaf[..path_len + 8])
            }
        })
        .collect::<Vec<_>>();
    drop(context);
    let buttons = locals
        .iter()
        .step_by(2)
        .map(|local| key(&root, local))
        .collect::<Vec<_>>();
    let snapshot = locals
        .iter()
        .enumerate()
        .map(|(index, local)| {
            if index % 2 == 0 {
                buttons[index / 2].1.clone()
            } else {
                local.clone()
            }
        })
        .collect::<Vec<_>>();
    drop(locals);
    let before_validation = CALLS.load(Ordering::Relaxed);
    for local in &snapshot {
        black_box(key(&root, local));
    }
    let validation_calls = CALLS.load(Ordering::Relaxed) - before_validation;
    let before_compile = CALLS.load(Ordering::Relaxed);
    let frame = snapshot
        .iter()
        .map(|local| key(&root, local))
        .collect::<Vec<_>>();
    let cache = frame.clone();
    let compile_calls = CALLS.load(Ordering::Relaxed) - before_compile;
    let with_snapshot = LIVE.load(Ordering::Relaxed) - baseline;
    drop(snapshot);
    let without_snapshot = LIVE.load(Ordering::Relaxed) - baseline;
    let construction_ns = begin.elapsed().as_nanos();
    let comparisons = Instant::now();
    for _ in 0..16 {
        for pair in frame.windows(2) {
            let (a, an) = encoded(&pair[0].0, &pair[0].1);
            let (b, bn) = encoded(&pair[1].0, &pair[1].1);
            black_box(a[..an].cmp(&b[..bn]));
        }
    }
    let compare_ns = comparisons.elapsed().as_nanos();
    let peak = PEAK.load(Ordering::Relaxed) - baseline;
    drop((buttons, frame, cache, root));
    assert_eq!(LIVE.load(Ordering::Relaxed), baseline);
    println!(
        "{{\"model\":\"{}\",\"case\":\"{case}\",\"population\":{population},\"with_snapshot_requested_bytes\":{with_snapshot},\"without_snapshot_requested_bytes\":{without_snapshot},\"peak_requested_bytes\":{peak},\"validation_allocation_calls\":{validation_calls},\"compile_allocation_calls\":{compile_calls},\"construction_ns\":{construction_ns},\"canonical_compare_ns\":{compare_ns},\"key_handle_bytes\":{}}}",
        A::NAME,
        size_of::<(A, A)>()
    );
}

fn main() {
    conformance();
    println!(
        "{{\"experiment\":\"ui-key-storage-lifecycle\",\"version\":1,\"scope\":\"allocator requests, not RSS or engine frame timing\"}}"
    );
    for population in [32, 4096] {
        for (case, context, path) in [
            ("shared-context", 120, 0),
            ("shared-placement", 40, 80),
            ("foreign-context", 120, 0),
        ] {
            run::<Flat>(population, case, context, path);
            run::<Typed>(population, case, context, path);
            run::<Segmented>(population, case, context, path);
        }
    }
}
