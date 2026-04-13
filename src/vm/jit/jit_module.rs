/// JIT compiler — compiles Forge bytecode to native machine code.
use std::collections::HashMap;

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Module;

use crate::vm::bytecode::Chunk;
use crate::vm::jit::ir_builder::{self, StringRefs};
use crate::vm::jit::runtime;

pub struct JitCompiler {
    module: JITModule,
    compiled: HashMap<String, *const u8>,
}

unsafe impl Send for JitCompiler {}

impl JitCompiler {
    pub fn new() -> Result<Self, String> {
        let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())
            .map_err(|e| format!("JIT builder error: {}", e))?;
        // Register runtime bridge symbols so Cranelift can resolve them
        builder.symbol("rt_string_concat", runtime::rt_string_concat as *const u8);
        builder.symbol("rt_string_len", runtime::rt_string_len as *const u8);
        builder.symbol("rt_string_eq", runtime::rt_string_eq as *const u8);
        let module = JITModule::new(builder);
        Ok(Self {
            module,
            compiled: HashMap::new(),
        })
    }

    pub fn compile_function(
        &mut self,
        chunk: &Chunk,
        name: &str,
        string_refs: Option<&StringRefs>,
    ) -> Result<*const u8, String> {
        if let Some(ptr) = self.compiled.get(name) {
            return Ok(*ptr);
        }

        let func_id = ir_builder::build_function(&mut self.module, chunk, name, string_refs)?;

        self.module
            .finalize_definitions()
            .map_err(|e| format!("finalize error: {}", e))?;

        let code_ptr = self.module.get_finalized_function(func_id);
        self.compiled.insert(name.to_string(), code_ptr);
        Ok(code_ptr)
    }

    pub fn get_compiled(&self, name: &str) -> Option<*const u8> {
        self.compiled.get(name).copied()
    }
}
