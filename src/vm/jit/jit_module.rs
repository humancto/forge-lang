/// JIT compiler — compiles Forge bytecode to native machine code.
use std::collections::HashMap;

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Module;

use crate::vm::bytecode::Chunk;
use crate::vm::jit::ir_builder;

pub struct JitCompiler {
    module: JITModule,
    compiled: HashMap<String, *const u8>,
}

unsafe impl Send for JitCompiler {}

impl JitCompiler {
    pub fn new() -> Result<Self, String> {
        let builder = JITBuilder::new(cranelift_module::default_libcall_names())
            .map_err(|e| format!("JIT builder error: {}", e))?;
        let module = JITModule::new(builder);
        Ok(Self {
            module,
            compiled: HashMap::new(),
        })
    }

    pub fn compile_function(&mut self, chunk: &Chunk, name: &str) -> Result<*const u8, String> {
        if let Some(ptr) = self.compiled.get(name) {
            return Ok(*ptr);
        }

        let func_id = ir_builder::build_function(&mut self.module, chunk, name)?;

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
