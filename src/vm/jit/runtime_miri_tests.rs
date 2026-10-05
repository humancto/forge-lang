//! Tests of the `extern "C"` JIT bridges that also run under Miri
//! (`cargo +nightly miri test --lib -- miri_`, see the `miri` job in
//! `.github/workflows/fuzz.yml`).
//!
//! Miri cannot execute Cranelift-generated machine code, so these tests call
//! the bridges directly from Rust, exactly as JIT code would: with a raw
//! `*mut VM` and raw pointers to tagged-value buffers. That covers the
//! pointer arithmetic over argument buffers, the tagged-value encoding, and
//! the `&mut *vm_ptr` reborrows (sound here because no other reference to
//! the VM is live during the call — the real JIT path's aliasing caveat is
//! documented on `rt_get_global`).

use super::runtime::*;
use crate::vm::machine::VM;
use crate::vm::value::{GcRef, ObjKind};

fn obj_tag(r: GcRef) -> i64 {
    ((TAG_OBJ << TAG_SHIFT) | (r.0 as u64 & PAYLOAD_MASK)) as i64
}

fn string_ref(vm: &mut VM, s: &str) -> GcRef {
    vm.gc.alloc_string(s.to_string())
}

#[test]
fn miri_tagged_int_encoding_round_trips() {
    for n in [
        0i64,
        1,
        -1,
        42,
        -42,
        (1 << 47) - 1,
        -(1 << 47),
        (1 << 59) - 1,
        -(1 << 59),
    ] {
        let e = encode_int(n);
        assert_eq!(get_tag(e), TAG_INT);
        assert_eq!(get_int_payload(e), n, "round trip of {n}");
    }
    assert_eq!(get_tag(encode_bool(true)), TAG_BOOL);
    assert_eq!(get_tag(encode_null()), TAG_NULL);
    assert!(decode_value(encode_null()).is_null());
    assert!(decode_value(encode_int(7)).as_inline_int() == Some(7));
}

#[test]
fn miri_int_arithmetic_bridges_promote_on_overflow() {
    assert_eq!(get_int_payload(rt_int_add(2, 3)), 5);
    assert_eq!(get_int_payload(rt_int_sub(2, 3)), -1);
    assert_eq!(get_int_payload(rt_int_mul(6, 7)), 42);
    // Overflow promotes to float like the VM (never wraps silently).
    assert_ne!(get_tag(rt_int_add(i64::MAX, 1)), TAG_INT);
}

#[test]
fn miri_array_bridges_read_argument_buffers() {
    let mut vm = VM::new();
    let vm_ptr: *mut VM = &mut vm;
    let elements = [
        encode_int(10) as i64,
        encode_int(20) as i64,
        encode_bool(true) as i64,
    ];
    let arr = rt_array_new(vm_ptr, elements.as_ptr(), elements.len() as i64);
    assert_eq!(rt_obj_len(vm_ptr, arr), 3);
    assert_eq!(get_int_payload(rt_array_get(vm_ptr, arr, 1) as u64), 20);
    rt_array_set(vm_ptr, arr, 1, encode_int(99) as i64);
    assert_eq!(get_int_payload(rt_array_get(vm_ptr, arr, 1) as u64), 99);
    // Out of range: null, and set is a no-op.
    assert_eq!(rt_array_get(vm_ptr, arr, 3) as u64, encode_null());
    rt_array_set(vm_ptr, arr, 1000, encode_int(1) as i64);
    assert_eq!(rt_obj_len(vm_ptr, arr), 3);
    // Zero elements: the pointer is never read.
    let empty = rt_array_new(vm_ptr, std::ptr::NonNull::<i64>::dangling().as_ptr(), 0);
    assert_eq!(rt_obj_len(vm_ptr, empty), 0);
    assert_eq!(rt_obj_len(vm_ptr, rt_empty_array(vm_ptr)), 0);
}

#[test]
fn miri_object_and_string_bridges() {
    let mut vm = VM::new();
    let key_a = string_ref(&mut vm, "a");
    let key_b = string_ref(&mut vm, "b");
    let hello = string_ref(&mut vm, "héllo");
    let vm_ptr: *mut VM = &mut vm;

    let pairs = [
        obj_tag(key_a),
        encode_int(1) as i64,
        obj_tag(key_b),
        encode_bool(false) as i64,
    ];
    let obj = rt_object_new(vm_ptr, pairs.as_ptr(), 2);
    assert_eq!(rt_obj_len(vm_ptr, obj), 2);
    assert_eq!(
        get_int_payload(rt_object_get(vm_ptr, obj, key_a.0 as i64) as u64),
        1
    );
    rt_object_set(vm_ptr, obj, key_a.0 as i64, encode_int(5) as i64);
    assert_eq!(
        get_int_payload(rt_object_get(vm_ptr, obj, key_a.0 as i64) as u64),
        5
    );
    // A non-string key reference is ignored, not dereferenced as a string.
    assert_eq!(
        rt_object_get(vm_ptr, obj, obj) as u64,
        encode_null(),
        "object used as a key"
    );

    let joined = rt_string_concat(vm_ptr, hello.0 as i64, key_a.0 as i64);
    assert_eq!(rt_string_len(vm_ptr, joined), 6);
    assert_eq!(rt_string_eq(vm_ptr, hello.0 as i64, hello.0 as i64), 1);
    assert_eq!(rt_string_eq(vm_ptr, hello.0 as i64, key_a.0 as i64), 0);
    // Invalid references are reported, not followed.
    assert_eq!(rt_string_len(vm_ptr, i64::MAX >> 8), -1);
    assert_eq!(rt_string_concat(vm_ptr, obj, hello.0 as i64), -1);

    let parts = [encode_int(1) as i64, obj_tag(hello), encode_null() as i64];
    let s = rt_interpolate(vm_ptr, parts.as_ptr(), parts.len() as i64);
    let text = match vm.gc.get(GcRef(s as usize)).map(|o| &o.kind) {
        Some(ObjKind::String(t)) => t.clone(),
        _ => panic!("expected a string"),
    };
    assert_eq!(text, "1héllonull");
}
