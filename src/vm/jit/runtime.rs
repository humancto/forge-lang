// JIT runtime bridge functions. Many are unused until M2 NaN-boxing JIT is wired up.
#![allow(dead_code)]

/// Runtime bridge functions for JIT-compiled code.
///
/// JIT-compiled machine code can't call Rust methods directly.
/// These extern "C" functions serve as the bridge between native
/// code and the VM runtime (GC, globals, builtins).
///
/// Values are passed as tagged u64:
///   Bits 60-63: tag (0=Int, 1=Float, 2=Bool, 3=Null, 4=Obj)
///   Bits  0-59: payload
///
/// NOTE: Many functions here are intentionally "unused" — they are infrastructure
/// for the M2 NaN-boxing JIT (Milestone 2) and will be wired up in that milestone.
/// The allow(dead_code) below suppresses the warnings until then.
use crate::vm::machine::VM;
use crate::vm::value::*;

const TAG_INT: u64 = 0;
const TAG_FLOAT: u64 = 1;
const TAG_BOOL: u64 = 2;
const TAG_NULL: u64 = 3;
const TAG_OBJ: u64 = 4;
const TAG_SHIFT: u64 = 60;
const PAYLOAD_MASK: u64 = (1u64 << 60) - 1;

pub fn encode_value(v: &Value) -> u64 {
    match v {
        Value::Int(n) => (TAG_INT << TAG_SHIFT) | (*n as u64 & PAYLOAD_MASK),
        Value::Float(f) => {
            let bits = f.to_bits();
            (TAG_FLOAT << TAG_SHIFT) | (bits & PAYLOAD_MASK)
        }
        Value::Bool(b) => (TAG_BOOL << TAG_SHIFT) | (*b as u64),
        Value::Null => TAG_NULL << TAG_SHIFT,
        Value::Obj(GcRef(idx)) => (TAG_OBJ << TAG_SHIFT) | (*idx as u64 & PAYLOAD_MASK),
    }
}

pub fn decode_value(encoded: u64) -> Value {
    let tag = encoded >> TAG_SHIFT;
    let payload = encoded & PAYLOAD_MASK;
    match tag {
        TAG_INT => {
            let n = if payload & (1u64 << 59) != 0 {
                (payload | !PAYLOAD_MASK) as i64
            } else {
                payload as i64
            };
            Value::small_int(n)
        }
        TAG_FLOAT => {
            let bits = payload;
            Value::float(f64::from_bits(bits))
        }
        TAG_BOOL => Value::bool_val(payload != 0),
        TAG_NULL => Value::null(),
        TAG_OBJ => Value::obj(GcRef(payload as usize)),
        _ => Value::null(),
    }
}

pub fn encode_int(n: i64) -> u64 {
    (TAG_INT << TAG_SHIFT) | (n as u64 & PAYLOAD_MASK)
}

pub fn encode_bool(b: bool) -> u64 {
    (TAG_BOOL << TAG_SHIFT) | (b as u64)
}

pub fn encode_null() -> u64 {
    TAG_NULL << TAG_SHIFT
}

pub fn get_tag(encoded: u64) -> u64 {
    encoded >> TAG_SHIFT
}

pub fn get_int_payload(encoded: u64) -> i64 {
    let payload = encoded & PAYLOAD_MASK;
    if payload & (1u64 << 59) != 0 {
        (payload | !PAYLOAD_MASK) as i64
    } else {
        payload as i64
    }
}

/// Bridge: print a value (called by say/println in JIT code)
pub extern "C" fn rt_print(vm_ptr: *mut VM, encoded: u64) {
    let vm = unsafe { &mut *vm_ptr };
    let val = decode_value(encoded);
    let text = val.display(&vm.gc);
    println!("{}", text);
}

/// Bridge: get a global variable by constant index
pub extern "C" fn rt_get_global(vm_ptr: *mut VM, name_idx: u64) -> u64 {
    let _vm = unsafe { &mut *vm_ptr };
    let _ = name_idx;
    encode_null()
}

/// Bridge: call a native/builtin function
pub extern "C" fn rt_call_native(
    vm_ptr: *mut VM,
    func_encoded: u64,
    args_ptr: *const u64,
    argc: u64,
) -> u64 {
    let vm = unsafe { &mut *vm_ptr };
    let func = decode_value(func_encoded);
    let args: Vec<Value> = (0..argc as usize)
        .map(|i| decode_value(unsafe { *args_ptr.add(i) }))
        .collect();
    match vm.call_value(func, args) {
        Ok(result) => encode_value(&result),
        Err(_) => encode_null(),
    }
}

/// Bridge: integer addition with overflow promotion to float
pub extern "C" fn rt_int_add(a: i64, b: i64) -> u64 {
    match a.checked_add(b) {
        Some(r) => encode_int(r),
        None => {
            let f = a as f64 + b as f64;
            (TAG_FLOAT << TAG_SHIFT) | (f.to_bits() & PAYLOAD_MASK)
        }
    }
}

/// Bridge: integer subtraction with overflow promotion
pub extern "C" fn rt_int_sub(a: i64, b: i64) -> u64 {
    match a.checked_sub(b) {
        Some(r) => encode_int(r),
        None => {
            let f = a as f64 - b as f64;
            (TAG_FLOAT << TAG_SHIFT) | (f.to_bits() & PAYLOAD_MASK)
        }
    }
}

/// Bridge: integer multiplication with overflow promotion
pub extern "C" fn rt_int_mul(a: i64, b: i64) -> u64 {
    match a.checked_mul(b) {
        Some(r) => encode_int(r),
        None => {
            let f = a as f64 * b as f64;
            (TAG_FLOAT << TAG_SHIFT) | (f.to_bits() & PAYLOAD_MASK)
        }
    }
}

/// Simple bridges for JIT code using raw i64/f64 calling convention.
/// These don't use tagged encoding — they work with the current
/// type-aware JIT that passes raw values.

pub extern "C" fn rt_println_i64(val: i64) {
    println!("{}", val);
}

pub extern "C" fn rt_println_f64(val: f64) {
    if val.fract() == 0.0 && val >= i64::MIN as f64 && val <= i64::MAX as f64 {
        println!("{}", val as i64);
    } else {
        println!("{}", val);
    }
}

pub extern "C" fn rt_print_i64(val: i64) {
    print!("{}", val);
}

pub extern "C" fn rt_print_f64(val: f64) {
    if val.fract() == 0.0 && val >= i64::MIN as f64 && val <= i64::MAX as f64 {
        print!("{}", val as i64);
    } else {
        print!("{}", val);
    }
}

/// Bridge: concatenate two GC strings, return new GcRef index.
/// Returns -1 on error (invalid refs).
pub extern "C" fn rt_string_concat(vm_ptr: *mut VM, a_ref: i64, b_ref: i64) -> i64 {
    let vm = unsafe { &mut *vm_ptr };
    let a_str = match vm.gc.get(GcRef(a_ref as usize)) {
        Some(obj) => match &obj.kind {
            ObjKind::String(s) => s.clone(),
            _ => return -1,
        },
        None => return -1,
    };
    let b_str = match vm.gc.get(GcRef(b_ref as usize)) {
        Some(obj) => match &obj.kind {
            ObjKind::String(s) => s.clone(),
            _ => return -1,
        },
        None => return -1,
    };
    let mut result = a_str;
    result.push_str(&b_str);
    let gc_ref = vm.gc.alloc_string(result);
    gc_ref.0 as i64
}

/// Bridge: return the char count of a GC string.
/// Returns -1 on error (invalid ref).
pub extern "C" fn rt_string_len(vm_ptr: *mut VM, s_ref: i64) -> i64 {
    let vm = unsafe { &mut *vm_ptr };
    match vm.gc.get(GcRef(s_ref as usize)) {
        Some(obj) => match &obj.kind {
            ObjKind::String(s) => s.chars().count() as i64,
            _ => -1,
        },
        None => -1,
    }
}

/// Bridge: compare two GC strings for equality.
/// Returns 1 if equal, 0 if not.
/// Type analysis guarantees both operands are StringRef, so invalid refs
/// are impossible — we return 0 defensively rather than -1.
pub extern "C" fn rt_string_eq(vm_ptr: *mut VM, a_ref: i64, b_ref: i64) -> i64 {
    let vm = unsafe { &mut *vm_ptr };
    // Fast path: same GcRef index means same string (interning deduplicates)
    if a_ref == b_ref {
        return 1;
    }
    // SAFETY: Both gc.get() calls borrow vm.gc immutably. gc.get() is a pure
    // read (no allocation, no collection), so the first &str reference remains
    // valid while we obtain the second. No mutation can occur between the calls.
    let a_str = match vm.gc.get(GcRef(a_ref as usize)) {
        Some(obj) => match &obj.kind {
            ObjKind::String(s) => s.as_str(),
            _ => return 0,
        },
        None => return 0,
    };
    let b_str = match vm.gc.get(GcRef(b_ref as usize)) {
        Some(obj) => match &obj.kind {
            ObjKind::String(s) => s.as_str(),
            _ => return 0,
        },
        None => return 0,
    };
    if a_str == b_str {
        1
    } else {
        0
    }
}
