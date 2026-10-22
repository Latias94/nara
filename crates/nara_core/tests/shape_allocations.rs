#![cfg(feature = "serde")]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

use nara_core::{
    ByteLimit, DepthLimit, ItemLimit, SerdeShapeError, SerdeShapeLimits, SerdeShapePreflightError,
    preflight_serde_shape,
};

struct ObservingAllocator;

#[global_allocator]
static ALLOCATOR: ObservingAllocator = ObservingAllocator;

thread_local! {
    static MAX_REQUEST: Cell<Option<usize>> = const { Cell::new(None) };
}

fn observe(size: usize) {
    let _ = MAX_REQUEST.try_with(|maximum| {
        if let Some(previous) = maximum.get() {
            maximum.set(Some(previous.max(size)));
        }
    });
}

unsafe impl GlobalAlloc for ObservingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        observe(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        observe(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        observe(size);
        unsafe { System.realloc(pointer, layout, size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

fn limits(per_string: usize, total: usize) -> SerdeShapeLimits {
    SerdeShapeLimits::new(
        DepthLimit::new(8).unwrap(),
        ItemLimit::new(32).unwrap(),
        ItemLimit::new(16).unwrap(),
        ByteLimit::new(per_string).unwrap(),
        ByteLimit::new(total).unwrap(),
    )
}

fn preflight(
    input: &str,
    limits: SerdeShapeLimits,
) -> (
    Result<(), SerdeShapePreflightError<serde_json::Error>>,
    usize,
) {
    let mut parser = serde_json::Deserializer::from_str(input);
    MAX_REQUEST.set(Some(0));
    let result = preflight_serde_shape(&mut parser, limits);
    let maximum = MAX_REQUEST.replace(None).unwrap();
    (result, maximum)
}

#[test]
fn borrowed_map_key_limits_precede_nara_owned_copies() {
    const KEY_BYTES: usize = 1024 * 1024;
    let key = "x".repeat(KEY_BYTES);
    let input = format!(r#"{{"{key}":0}}"#);
    let (result, maximum) = preflight(&input, limits(32, 64));
    assert!(matches!(
        result,
        Err(SerdeShapePreflightError::Shape(
            SerdeShapeError::StringLimitExceeded { maximum: 32 }
        ))
    ));
    assert!(
        maximum < KEY_BYTES,
        "rejected key allocated {maximum} bytes"
    );

    let input = format!(r#"{{"short":0,"{key}":1}}"#);
    let (result, maximum) = preflight(&input, limits(KEY_BYTES, KEY_BYTES));
    assert!(matches!(
        result,
        Err(SerdeShapePreflightError::Shape(
            SerdeShapeError::TotalStringLimitExceeded { maximum: KEY_BYTES }
        ))
    ));
    assert!(
        maximum < KEY_BYTES,
        "aggregate-rejected key allocated {maximum} bytes"
    );

    let key = "x".repeat(4096);
    let input = format!(r#"{{"{key}":0}}"#);
    let (result, maximum) = preflight(&input, limits(4096, 4096));
    result.unwrap();
    assert!(
        maximum >= 4096,
        "accepted keys must still participate in duplicate detection"
    );
}

#[test]
fn owned_and_escaped_keys_charge_once_and_preserve_duplicate_detection() {
    let map = serde::de::value::MapDeserializer::<_, serde::de::value::Error>::new(
        [("key".to_owned(), 0_u32)].into_iter(),
    );
    preflight_serde_shape(map, limits(3, 3)).unwrap();
    let (result, _) = preflight(r#"{"k\u0065y":0}"#, limits(3, 3));
    result.unwrap();
    let (result, _) = preflight(r#"{"key":0,"k\u0065y":1}"#, limits(3, 6));
    assert!(matches!(
        result,
        Err(SerdeShapePreflightError::Shape(
            SerdeShapeError::DuplicateMapKey
        ))
    ));
}
