use super::value::{GcRef, Value};
use crate::clock::Instant;

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
    /// Length of `VM::scope_cancels` before this scope pushed its own flag.
    /// When the deadline fires, every flag from here on is set (cancelling
    /// tasks the block started, including squad tasks) and dropped.
    pub scope_depth: usize,
}

/// The open upvalue cells of a frame, indexed by register. Captured locals
/// are read and written through their cell on every access (`read_local` /
/// `write_local`), which is hot in top-level code whose variables are
/// captured by top-level functions, so this is a direct index rather than a
/// hash map. Allocates nothing until the first capture.
#[derive(Default)]
pub struct OpenUpvalues {
    cells: Vec<Option<GcRef>>,
    count: usize,
}

impl OpenUpvalues {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[inline]
    pub fn get(&self, reg: &u8) -> Option<&GcRef> {
        self.cells.get(*reg as usize)?.as_ref()
    }

    #[inline]
    pub fn contains_key(&self, reg: &u8) -> bool {
        self.get(reg).is_some()
    }

    pub fn insert(&mut self, reg: u8, cell: GcRef) -> Option<GcRef> {
        let i = reg as usize;
        if self.cells.len() <= i {
            self.cells.resize(i + 1, None);
        }
        let old = self.cells[i].replace(cell);
        if old.is_none() {
            self.count += 1;
        }
        old
    }

    pub fn remove(&mut self, reg: &u8) -> Option<GcRef> {
        let old = self.cells.get_mut(*reg as usize)?.take();
        if old.is_some() {
            self.count -= 1;
        }
        old
    }

    /// Every open cell (GC roots).
    pub fn values(&self) -> impl Iterator<Item = &GcRef> + '_ {
        self.cells.iter().flatten()
    }
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
    /// Number of registers this frame uses (from chunk.max_registers)
    pub size: usize,
    /// Active exception handlers for this frame, innermost last.
    pub handlers: Vec<ExceptionHandler>,
    /// Active timeout scopes for this frame, innermost last.
    pub timeouts: Vec<TimeoutGuard>,
    /// Shared cells for locals captured by closures created in this frame.
    pub open_upvalues: OpenUpvalues,
    /// Number of arguments the caller passed (`JumpIfArg` uses it to decide
    /// whether a parameter's default value applies).
    pub argc: usize,
    /// The arguments this call was entered with (empty for the main and
    /// module frames). Kept so a hot loop can re-run the whole call in
    /// native code (`VM::try_jit_loop_restart`) even after the body has
    /// overwritten its parameters. GC roots.
    pub entry_args: Vec<Value>,
    /// Backward jumps taken in this frame (loop hotness for JIT tier-up).
    pub back_edges: u32,
}

impl CallFrame {
    pub fn new(closure: GcRef, base: usize, size: usize) -> Self {
        Self {
            closure,
            ip: 0,
            base,
            size,
            handlers: Vec::new(),
            timeouts: Vec::new(),
            open_upvalues: OpenUpvalues::default(),
            argc: usize::MAX,
            entry_args: Vec::new(),
            back_edges: 0,
        }
    }

    #[allow(dead_code)]
    pub fn read_instruction(&mut self, code: &[u32]) -> u32 {
        let inst = code[self.ip];
        self.ip += 1;
        inst
    }
}

/// Initial capacity of the frame stack. This is NOT a depth limit: call depth
/// is bounded by `runtime::recursion::check_call_depth` (configurable, shared
/// with the interpreter), which also guards the native stack.
pub const INITIAL_FRAME_CAPACITY: usize = 256;
