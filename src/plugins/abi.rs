//! Host-side mirror of the native plugin ABI, version 1.
//!
//! The normative definition is `crates/forge-plugin/include/forge_plugin.h`;
//! the Rust SDK has its own copy in `crates/forge-plugin/src/abi.rs`. The
//! host does not depend on the SDK crate (so `forge-lang` builds and
//! publishes on its own), which is why the layout is pinned by the tests at
//! the bottom of this file and exercised end to end by
//! `tests/native_plugins.rs` with real Rust and C plugins.
//!
//! **Changing anything here is an ABI break**: bump [`ABI_VERSION`] in all
//! three places and keep loading the old version or reject it explicitly.

use std::os::raw::c_char;

/// The plugin ABI version this host implements.
pub const ABI_VERSION: u32 = 1;

pub const TAG_NULL: u32 = 0;
pub const TAG_BOOL: u32 = 1;
pub const TAG_INT: u32 = 2;
pub const TAG_FLOAT: u32 = 3;
pub const TAG_STRING: u32 = 4;
pub const TAG_BYTES: u32 = 5;
pub const TAG_ARRAY: u32 = 6;
pub const TAG_OBJECT: u32 = 7;

/// `call` succeeded; `*out` holds the result.
pub const STATUS_OK: i32 = 0;
/// `call` failed; `*out` should hold a `STRING` error message.
pub const STATUS_ERR: i32 = 1;

/// A UTF-8 string or byte buffer (`STRING`, `BYTES`, object keys). Not
/// NUL-terminated. `ptr` may be null when `len` is 0.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ForgeBuf {
    pub ptr: *mut u8,
    pub len: usize,
}

/// `ARRAY` payload.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ForgeArray {
    pub ptr: *mut ForgeValue,
    pub len: usize,
}

/// `OBJECT` payload.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ForgeObject {
    pub ptr: *mut ForgeEntry,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union ForgePayload {
    /// `BOOL` (0 or non-zero) and `INT`.
    pub i: i64,
    pub f: f64,
    /// `STRING` and `BYTES`.
    pub buf: ForgeBuf,
    pub array: ForgeArray,
    pub object: ForgeObject,
}

/// A tagged value crossing the plugin boundary.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ForgeValue {
    pub tag: u32,
    pub as_: ForgePayload,
}

impl ForgeValue {
    pub const NULL: ForgeValue = ForgeValue {
        tag: TAG_NULL,
        as_: ForgePayload { i: 0 },
    };
}

/// One `OBJECT` entry; `key` is UTF-8.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ForgeEntry {
    pub key: ForgeBuf,
    pub value: ForgeValue,
}

/// `int32_t (*)(const ForgeValue *args, size_t argc, ForgeValue *out)`
pub type ForgeFnPtr =
    unsafe extern "C" fn(args: *const ForgeValue, argc: usize, out: *mut ForgeValue) -> i32;

/// One exported function.
#[repr(C)]
pub struct ForgeFunction {
    pub name: *const c_char,
    /// Exact argument count, or -1 for variadic.
    pub arity: i32,
    pub call: Option<ForgeFnPtr>,
}

/// The descriptor returned by `forge_plugin_register`.
#[repr(C)]
pub struct ForgePlugin {
    pub abi_version: u32,
    pub name: *const c_char,
    pub version: *const c_char,
    pub functions: *const ForgeFunction,
    pub function_count: usize,
    pub free_value: Option<unsafe extern "C" fn(value: *mut ForgeValue)>,
}

/// `uint32_t forge_plugin_abi_version(void)`
pub type AbiVersionFn = unsafe extern "C" fn() -> u32;
/// `const ForgePlugin *forge_plugin_register(void)`
pub type RegisterFn = unsafe extern "C" fn() -> *const ForgePlugin;

pub const SYM_ABI_VERSION: &[u8] = b"forge_plugin_abi_version\0";
pub const SYM_REGISTER: &[u8] = b"forge_plugin_register\0";

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    /// The layout `forge_plugin.h` documents (and its `_Static_assert`s
    /// check) for 64-bit targets.
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn layout_matches_the_c_header() {
        assert_eq!(size_of::<ForgeBuf>(), 16);
        assert_eq!(size_of::<ForgePayload>(), 16);
        assert_eq!(offset_of!(ForgeValue, tag), 0);
        assert_eq!(offset_of!(ForgeValue, as_), 8);
        assert_eq!(align_of::<ForgeValue>(), 8);
        assert_eq!(size_of::<ForgeValue>(), 24);
        assert_eq!(offset_of!(ForgeEntry, key), 0);
        assert_eq!(offset_of!(ForgeEntry, value), 16);
        assert_eq!(size_of::<ForgeEntry>(), 40);
        assert_eq!(offset_of!(ForgeFunction, name), 0);
        assert_eq!(offset_of!(ForgeFunction, arity), 8);
        assert_eq!(offset_of!(ForgeFunction, call), 16);
        assert_eq!(size_of::<ForgeFunction>(), 24);
        assert_eq!(offset_of!(ForgePlugin, abi_version), 0);
        assert_eq!(offset_of!(ForgePlugin, name), 8);
        assert_eq!(offset_of!(ForgePlugin, functions), 24);
        assert_eq!(offset_of!(ForgePlugin, function_count), 32);
        assert_eq!(offset_of!(ForgePlugin, free_value), 40);
        assert_eq!(size_of::<ForgePlugin>(), 48);
    }
}
