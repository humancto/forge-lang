use super::value::GcRef;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Clone, Copy)]
pub struct ExceptionHandler {
    pub catch_ip: usize,
    pub error_register: u8,
}

#[derive(Clone, Copy)]
pub struct TimeoutGuard {
    pub deadline: Instant,
    pub seconds: u64,
    pub catch_ip: usize,
    pub error_register: u8,
    pub handler_base: usize,
}

/// A call frame representing one function invocation in the VM.
/// Each frame has a window into the VM's flat register array.
pub struct CallFrame {
    /// GcRef to the ObjClosure being executed
    pub closure: GcRef,
    /// Instruction pointer — index into the closure's chunk.code
    pub ip: usize,
    /// Base index into the VM's register array for this frame's window
    pub base: usize,
    /// Active exception handlers for this frame, innermost last.
    pub handlers: Vec<ExceptionHandler>,
    /// Active timeout scopes for this frame, innermost last.
    pub timeouts: Vec<TimeoutGuard>,
    /// Shared cells for locals captured by closures created in this frame.
    pub open_upvalues: HashMap<u8, GcRef>,
}

impl CallFrame {
    pub fn new(closure: GcRef, base: usize) -> Self {
        Self {
            closure,
            ip: 0,
            base,
            handlers: Vec::new(),
            timeouts: Vec::new(),
            open_upvalues: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn read_instruction(&mut self, code: &[u32]) -> u32 {
        let inst = code[self.ip];
        self.ip += 1;
        inst
    }
}

/// Maximum call stack depth to prevent stack overflow.
pub const MAX_FRAMES: usize = 256;

/// Maximum register count across all frames.
#[allow(dead_code)]
pub const MAX_REGISTERS: usize = MAX_FRAMES * 256;
