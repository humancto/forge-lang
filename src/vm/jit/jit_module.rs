//! Owns the Cranelift JIT module (and therefore all generated code memory).
//!
//! One `JitCompiler` lives for the lifetime of a VM's JIT state; compiled
//! code pointers stay valid as long as it is alive. Functions are named by a
//! monotonically increasing counter, never by their Forge name.

use cranelift_jit::{JITBuilder, JITModule};

use crate::vm::bytecode::Chunk;
use crate::vm::jit::ir_builder;
use crate::vm::jit::runtime;
use crate::vm::jit::verifier::VerifiedFn;

pub struct JitCompiler {
    /// Dropped through `free_memory` in `Drop` (a plain drop of a
    /// `JITModule` leaks its code pages).
    module: std::mem::ManuallyDrop<JITModule>,
    next_symbol: u64,
}

// SAFETY: JITModule holds raw code pointers but no thread-affine state; it is
// only ever used from the thread that owns the VM.
unsafe impl Send for JitCompiler {}

impl JitCompiler {
    pub fn new() -> Result<Self, String> {
        let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())
            .map_err(|e| format!("JIT builder error: {}", e))?;
        // Runtime bridges for future string/collection tiers. Registering
        // them is harmless; the current tier never imports them.
        builder.symbol("rt_string_concat", runtime::rt_string_concat as *const u8);
        builder.symbol("rt_string_len", runtime::rt_string_len as *const u8);
        builder.symbol("rt_string_eq", runtime::rt_string_eq as *const u8);
        let module = JITModule::new(builder);
        Ok(Self {
            module: std::mem::ManuallyDrop::new(module),
            next_symbol: 0,
        })
    }

    /// Compile a verified specialization and return its entry point
    /// (`extern "C" fn(*mut JitCtx, *const i64) -> i64`).
    pub fn compile(&mut self, chunk: &Chunk, vf: &VerifiedFn) -> Result<*const u8, String> {
        let symbol = format!("forge_jit_{}", self.next_symbol);
        self.next_symbol += 1;
        let entry = ir_builder::build_function(&mut *self.module, chunk, vf, &symbol)?;
        self.module
            .finalize_definitions()
            .map_err(|e| format!("finalize error: {}", e))?;
        Ok(self.module.get_finalized_function(entry))
    }
}

impl Drop for JitCompiler {
    fn drop(&mut self) {
        // SAFETY: every entry pointer handed out by `compile` is stored in
        // the `JitState` that owns this compiler, and the VM only invokes
        // them while that state is alive (forked VMs start with a fresh
        // `JitState`). So no compiled code can run, or be on the stack,
        // once the compiler is dropped. `module` is never used again.
        unsafe {
            let module = std::mem::ManuallyDrop::take(&mut self.module);
            module.free_memory();
        }
    }
}
