/// JIT compiler that compiles Forge bytecode to native machine code at runtime.
use std::collections::HashMap;

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Module;

use crate::vm::bytecode::Chunk;
use crate::vm::jit::ir_builder;
use crate::vm::jit::runtime;

type JitFn = unsafe extern "C" fn(i64, i64) -> u64;

pub struct JitCompiler {
    module: JITModule,
    compiled: HashMap<String, *const u8>,
}

unsafe impl Send for JitCompiler {}

impl JitCompiler {
    pub fn new() -> Result<Self, String> {
        let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())
            .map_err(|e| format!("JIT builder error: {}", e))?;

        builder.symbol("rt_int_add", runtime::rt_int_add as *const u8);
        builder.symbol("rt_int_sub", runtime::rt_int_sub as *const u8);
        builder.symbol("rt_int_mul", runtime::rt_int_mul as *const u8);

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

    /// Call a JIT-compiled function with integer arguments, return encoded result.
    pub unsafe fn call_jit_fn(&self, ptr: *const u8, args: &[i64]) -> u64 {
        match args.len() {
            0 => {
                let f: extern "C" fn(i64) -> u64 = std::mem::transmute(ptr);
                f(0)
            }
            1 => {
                let f: extern "C" fn(i64, i64) -> u64 = std::mem::transmute(ptr);
                f(0, runtime::encode_int(args[0]) as i64)
            }
            2 => {
                let f: extern "C" fn(i64, i64, i64) -> u64 = std::mem::transmute(ptr);
                f(
                    0,
                    runtime::encode_int(args[0]) as i64,
                    runtime::encode_int(args[1]) as i64,
                )
            }
            _ => {
                let f: extern "C" fn(i64, i64) -> u64 = std::mem::transmute(ptr);
                f(0, runtime::encode_int(args[0]) as i64)
            }
        }
    }
}
