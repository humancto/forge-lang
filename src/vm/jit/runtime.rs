/// Runtime bridge functions for JIT-compiled code.
///
/// JIT-compiled machine code can't call Rust methods directly.
/// These extern "C" functions serve as the bridge between native
/// code and the VM runtime (GC, globals, builtins).
///
/// Values are passed as tagged u64:
///   Bits 60-63: tag (0=Int, 1=Float, 2=Bool, 3=Null, 4=Obj)
///   Bits  0-59: payload
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
            Value::Int(n)
        }
        TAG_FLOAT => {
            let bits = payload;
            Value::Float(f64::from_bits(bits))
        }
        TAG_BOOL => Value::Bool(payload != 0),
        TAG_NULL => Value::Null,
        TAG_OBJ => Value::Obj(GcRef(payload as usize)),
        _ => Value::Null,
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
    let vm = unsafe { &mut *vm_ptr };
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
