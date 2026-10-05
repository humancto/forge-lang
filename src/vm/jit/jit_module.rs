//! Owns the Cranelift JIT module (and therefore all generated code memory).
//!
//! One `JitCompiler` lives for the lifetime of a VM's JIT state; compiled
//! code pointers stay valid as long as it is alive. Functions are named by a
//! monotonically increasing counter, never by their Forge name.

use cranelift_jit::{JITBuilder, JITModule};

use cranelift_module::FuncId;

use crate::vm::bytecode::Chunk;
use crate::vm::jit::ir_builder::{self, CalleeBodies};
use crate::vm::jit::verifier::VerifiedFn;
use crate::vm::jit::{math_bridges, runtime};

pub struct JitCompiler {
    module: JITModule,
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
        // Pure numeric bridges used by the Float tier.
        for (name, ptr) in math_bridges::symbols() {
            builder.symbol(name, ptr);
        }
        let module = JITModule::new(builder);
        Ok(Self {
            module,
            next_symbol: 0,
        })
    }

    /// Compile a verified specialization. Returns its entry point
    /// (`extern "C" fn(*mut JitCtx, *const i64) -> i64`) and the id of its
    /// body, which later specializations may call directly. `callees` must
    /// hold the body of every function `vf` calls (all compiled earlier in
    /// this module).
    pub fn compile(
        &mut self,
        chunk: &Chunk,
        vf: &VerifiedFn,
        callees: &CalleeBodies,
    ) -> Result<(*const u8, FuncId), String> {
        let symbol = format!("forge_jit_{}", self.next_symbol);
        self.next_symbol += 1;
        let built = ir_builder::build_function(&mut self.module, chunk, vf, &symbol, callees)?;
        self.module
            .finalize_definitions()
            .map_err(|e| format!("finalize error: {}", e))?;
        Ok((self.module.get_finalized_function(built.entry), built.body))
    }
}
