use super::value::GcRef;

/// A call frame representing one function invocation in the VM.
/// Each frame has a window into the VM's flat register array.
pub struct CallFrame {
    /// GcRef to the ObjClosure being executed
    pub closure: GcRef,
    /// Instruction pointer — index into the closure's chunk.code
    pub ip: usize,
    /// Base index into the VM's register array for this frame's window
    pub base: usize,
}

impl CallFrame {
    pub fn new(closure: GcRef, base: usize) -> Self {
        Self {
            closure,
            ip: 0,
            base,
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
pub const MAX_REGISTERS: usize = MAX_FRAMES * 256;
