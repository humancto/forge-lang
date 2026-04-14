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
use indexmap::IndexMap;

use crate::vm::machine::VM;
use crate::vm::value::*;

const TAG_INT: u64 = 0;
const TAG_FLOAT: u64 = 1;
const TAG_BOOL: u64 = 2;
const TAG_NULL: u64 = 3;
const TAG_OBJ: u64 = 4;
const TAG_SHIFT: u64 = 60;
const PAYLOAD_MASK: u64 = (1u64 << 60) - 1;

pub fn encode_value(v: &Value, gc: &crate::vm::gc::Gc) -> u64 {
    match v.classify(gc) {
        crate::vm::value::ValueKind::Int(n) => (TAG_INT << TAG_SHIFT) | (n as u64 & PAYLOAD_MASK),
        crate::vm::value::ValueKind::Float(f) => {
            let bits = f.to_bits();
            (TAG_FLOAT << TAG_SHIFT) | (bits & PAYLOAD_MASK)
        }
        crate::vm::value::ValueKind::Bool(b) => (TAG_BOOL << TAG_SHIFT) | (b as u64),
        crate::vm::value::ValueKind::Null => TAG_NULL << TAG_SHIFT,
        crate::vm::value::ValueKind::Obj(r) => (TAG_OBJ << TAG_SHIFT) | (r.0 as u64 & PAYLOAD_MASK),
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
            // JIT uses 60-bit payload which can exceed NaN-box 48-bit inline range.
            // Fall back to float if we can't inline (no gc available for BoxedInt).
            const INT48_MAX: i64 = (1_i64 << 47) - 1;
            const INT48_MIN: i64 = -(1_i64 << 47);
            if n >= INT48_MIN && n <= INT48_MAX {
                Value::small_int(n)
            } else {
                Value::float(n as f64)
            }
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
        Ok(result) => encode_value(&result, &vm.gc),
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

// ---------------------------------------------------------------------------
// Collection bridges (arrays, objects, interpolation)
// ---------------------------------------------------------------------------

/// Bridge: create a new array from tagged elements on a stack buffer.
/// `elements_ptr` points to `count` tagged i64 values.
/// Returns the GcRef index of the new array.
pub extern "C" fn rt_array_new(vm_ptr: *mut VM, elements_ptr: *const i64, count: i64) -> i64 {
    let vm = unsafe { &mut *vm_ptr };
    let count = count as usize;
    // Decode all tagged values into a Vec<Value> before allocating (GC safety).
    let mut items = Vec::with_capacity(count);
    for i in 0..count {
        let tagged = unsafe { *elements_ptr.add(i) } as u64;
        items.push(decode_value(tagged));
    }
    let r = vm.gc.alloc(ObjKind::Array(items));
    r.0 as i64
}

/// Bridge: create an empty array (avoids zero-size stack slot issues).
pub extern "C" fn rt_empty_array(vm_ptr: *mut VM) -> i64 {
    let vm = unsafe { &mut *vm_ptr };
    let r = vm.gc.alloc(ObjKind::Array(Vec::new()));
    r.0 as i64
}

/// Bridge: get element from array by integer index.
/// Returns a tagged value. Returns tagged null on error/out-of-bounds.
pub extern "C" fn rt_array_get(vm_ptr: *mut VM, arr_ref: i64, idx: i64) -> i64 {
    let vm = unsafe { &mut *vm_ptr };
    let r = GcRef(arr_ref as usize);
    if let Some(obj) = vm.gc.get(r) {
        if let ObjKind::Array(items) = &obj.kind {
            if let Some(val) = items.get(idx as usize) {
                return encode_value(val, &vm.gc) as i64;
            }
        }
    }
    encode_null() as i64
}

/// Bridge: set element in array by integer index. No-op on error.
pub extern "C" fn rt_array_set(vm_ptr: *mut VM, arr_ref: i64, idx: i64, val: i64) {
    let vm = unsafe { &mut *vm_ptr };
    let decoded = decode_value(val as u64);
    let r = GcRef(arr_ref as usize);
    if let Some(obj) = vm.gc.get_mut(r) {
        if let ObjKind::Array(items) = &mut obj.kind {
            let i = idx as usize;
            if i < items.len() {
                items[i] = decoded;
            }
        }
    }
}

/// Bridge: return the length of a string, array, or object.
/// Replaces rt_string_len with a generalized version.
pub extern "C" fn rt_obj_len(vm_ptr: *mut VM, obj_ref: i64) -> i64 {
    let vm = unsafe { &mut *vm_ptr };
    match vm.gc.get(GcRef(obj_ref as usize)) {
        Some(obj) => match &obj.kind {
            ObjKind::String(s) => s.chars().count() as i64,
            ObjKind::Array(a) => a.len() as i64,
            ObjKind::Object(o) => o.len() as i64,
            _ => 0,
        },
        None => 0,
    }
}

/// Bridge: create a new object from tagged key-value pairs on a stack buffer.
/// `pairs_ptr` points to `pair_count * 2` tagged i64 values: [key, val, key, val, ...].
/// Keys must be ObjKind::String GcRefs (tag=4). Returns the GcRef index of the new object.
pub extern "C" fn rt_object_new(vm_ptr: *mut VM, pairs_ptr: *const i64, pair_count: i64) -> i64 {
    let vm = unsafe { &mut *vm_ptr };
    let pair_count = pair_count as usize;
    // First pass: collect all key strings and values (GC safety — no allocs during reads).
    let mut entries: Vec<(String, Value)> = Vec::with_capacity(pair_count);
    for i in 0..pair_count {
        let key_tagged = unsafe { *pairs_ptr.add(i * 2) } as u64;
        let val_tagged = unsafe { *pairs_ptr.add(i * 2 + 1) } as u64;
        let key_val = decode_value(key_tagged);
        let val = decode_value(val_tagged);
        // Key should be a GcRef pointing to a string
        let key_str = if let Some(r) = key_val.as_obj() {
            if let Some(obj) = vm.gc.get(r) {
                if let ObjKind::String(s) = &obj.kind {
                    s.clone()
                } else {
                    continue;
                }
            } else {
                continue;
            }
        } else {
            continue;
        };
        entries.push((key_str, val));
    }
    // Second pass: build the IndexMap and allocate
    let mut map = IndexMap::new();
    for (key, val) in entries {
        map.insert(key, val);
    }
    let r = vm.gc.alloc(ObjKind::Object(map));
    r.0 as i64
}

/// Bridge: create an empty object (avoids zero-size stack slot issues).
pub extern "C" fn rt_empty_object(vm_ptr: *mut VM) -> i64 {
    let vm = unsafe { &mut *vm_ptr };
    let r = vm.gc.alloc(ObjKind::Object(IndexMap::new()));
    r.0 as i64
}

/// Bridge: get a field from an object by GcRef string key.
/// Returns a tagged value. Returns tagged null if field not found.
pub extern "C" fn rt_object_get(vm_ptr: *mut VM, obj_ref: i64, field_ref: i64) -> i64 {
    let vm = unsafe { &mut *vm_ptr };
    // Read the field name string first (no allocation)
    let field_name = match vm.gc.get(GcRef(field_ref as usize)) {
        Some(obj) => match &obj.kind {
            ObjKind::String(s) => s.clone(),
            _ => return encode_null() as i64,
        },
        None => return encode_null() as i64,
    };
    // Now look up the field in the object
    match vm.gc.get(GcRef(obj_ref as usize)) {
        Some(obj) => match &obj.kind {
            ObjKind::Object(map) => match map.get(&field_name) {
                Some(val) => encode_value(val, &vm.gc) as i64,
                None => encode_null() as i64,
            },
            _ => encode_null() as i64,
        },
        None => encode_null() as i64,
    }
}

/// Bridge: set a field on an object by GcRef string key.
pub extern "C" fn rt_object_set(vm_ptr: *mut VM, obj_ref: i64, field_ref: i64, val: i64) {
    let vm = unsafe { &mut *vm_ptr };
    // Read the field name string first
    let field_name = match vm.gc.get(GcRef(field_ref as usize)) {
        Some(obj) => match &obj.kind {
            ObjKind::String(s) => s.clone(),
            _ => return,
        },
        None => return,
    };
    let decoded = decode_value(val as u64);
    if let Some(obj) = vm.gc.get_mut(GcRef(obj_ref as usize)) {
        if let ObjKind::Object(map) = &mut obj.kind {
            map.insert(field_name, decoded);
        }
    }
}

/// Bridge: extract a tuple-like field ("_0", "_1", etc.) from an object.
/// Returns a tagged value.
pub extern "C" fn rt_extract_field(vm_ptr: *mut VM, obj_ref: i64, field_index: i64) -> i64 {
    let vm = unsafe { &mut *vm_ptr };
    let field_name = format!("_{}", field_index);
    match vm.gc.get(GcRef(obj_ref as usize)) {
        Some(obj) => match &obj.kind {
            ObjKind::Object(map) => match map.get(&field_name) {
                Some(val) => encode_value(val, &vm.gc) as i64,
                None => encode_null() as i64,
            },
            _ => encode_null() as i64,
        },
        None => encode_null() as i64,
    }
}

/// Bridge: interpolate N tagged values into a single string.
/// `parts_ptr` points to `count` tagged i64 values.
/// Returns the GcRef index of the resulting string.
pub extern "C" fn rt_interpolate(vm_ptr: *mut VM, parts_ptr: *const i64, count: i64) -> i64 {
    let vm = unsafe { &mut *vm_ptr };
    let count = count as usize;
    // Collect display strings first (GC safety)
    let mut parts: Vec<String> = Vec::with_capacity(count);
    for i in 0..count {
        let tagged = unsafe { *parts_ptr.add(i) } as u64;
        let val = decode_value(tagged);
        parts.push(val.display(&vm.gc));
    }
    let mut result = String::new();
    for part in &parts {
        result.push_str(part);
    }
    let r = vm.gc.alloc_string(result);
    r.0 as i64
}

/// Bridge: create an empty string (for zero-part interpolation).
pub extern "C" fn rt_empty_string(vm_ptr: *mut VM) -> i64 {
    let vm = unsafe { &mut *vm_ptr };
    let r = vm.gc.alloc_string(String::new());
    r.0 as i64
}
