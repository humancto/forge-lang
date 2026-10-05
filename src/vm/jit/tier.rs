//! Per-VM JIT tier: hotness, the `(FnId, TypeSig)` specialization cache,
//! guard-failure / deopt accounting, and the native call ABI.
//!
//! The VM's call path (`VM::call_value`) asks [`JitState::select`] for a
//! specialization, performs the VM-state entry guards it alone can check,
//! calls [`invoke`], and reports the outcome back with
//! [`JitState::record_deopt`] or [`JitState::record_guard_failure`].

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::vm::bytecode::{Chunk, Constant};
use crate::vm::gc::Gc;
use crate::vm::jit::jit_module::JitCompiler;
use crate::vm::jit::types::{FnId, JitType, TypeSig};
use crate::vm::jit::verifier::{self, Reject};
use crate::vm::value::Value;

/// Calls before a function is considered hot in [`JitMode::Auto`].
pub const HOT_THRESHOLD: u32 = 100;
/// Deopts after which a specialization is disabled for good.
pub const MAX_DEOPTS: u32 = 8;
/// Guard failures after which a function that never ran natively is
/// disabled (we stop classifying its arguments).
pub const GUARD_FAILURE_LIMIT: u32 = 256;

/// When the VM compiles functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitMode {
    /// Never compile; pure bytecode VM.
    Off,
    /// Compile a `(function, signature)` pair once it has been called
    /// [`HOT_THRESHOLD`] times (default).
    Auto,
    /// Compile on first call (`forge --jit`).
    Eager,
}

/// Context block shared with native code. Layout is part of the ABI.
#[repr(C)]
pub struct JitCtx {
    /// 0 = ok, 1 = deopt requested (result must be discarded).
    pub status: i64,
    /// Maximum self-call depth (entry call is depth 0) before deopting.
    pub max_depth: i64,
    /// The VM's cancellation flag.
    pub cancel: *const AtomicBool,
}

pub const CTX_STATUS_OFFSET: i32 = 0;
pub const CTX_MAX_DEPTH_OFFSET: i32 = 8;
pub const CTX_CANCEL_OFFSET: i32 = 16;

const _: () = {
    assert!(std::mem::offset_of!(JitCtx, status) == CTX_STATUS_OFFSET as usize);
    assert!(std::mem::offset_of!(JitCtx, max_depth) == CTX_MAX_DEPTH_OFFSET as usize);
    assert!(std::mem::offset_of!(JitCtx, cancel) == CTX_CANCEL_OFFSET as usize);
};

/// Outcome of one native invocation.
pub enum Invoke {
    Returned(i64),
    Deopt,
}

/// Run a compiled entry point.
///
/// # Safety
/// `entry` must come from [`JitCompiler::compile`] on a compiler that is
/// still alive, `args.len()` must equal the specialization's arity, and
/// `cancel` must point to a live `AtomicBool`.
pub unsafe fn invoke(
    entry: *const u8,
    args: &[i64],
    max_depth: i64,
    cancel: *const AtomicBool,
) -> Invoke {
    let mut ctx = JitCtx {
        status: 0,
        max_depth,
        cancel,
    };
    let f: extern "C" fn(*mut JitCtx, *const i64) -> i64 = std::mem::transmute(entry);
    let r = f(&mut ctx, args.as_ptr());
    if ctx.status == 0 {
        Invoke::Returned(r)
    } else {
        Invoke::Deopt
    }
}

/// A compiled specialization.
#[derive(Debug, Clone)]
pub struct CompiledSpec {
    pub entry: *const u8,
    pub ret: JitType,
    /// Whether the code calls itself through its global binding (the entry
    /// guard must then confirm the binding still names this function).
    pub needs_self_binding: bool,
    pub runs: u64,
    pub deopts: u32,
}

#[derive(Debug, Clone)]
pub enum SpecState {
    Compiled(CompiledSpec),
    /// The verifier (or code generation) refused this signature.
    Rejected(String),
    /// Compiled once but deopted too often.
    Disabled,
}

/// JIT bookkeeping for one function prototype.
pub struct FnJitState {
    pub name: String,
    pub calls: u32,
    pub guard_failures: u32,
    /// Set once guard failures dominate; the function then always runs in
    /// the VM without further checks.
    pub disabled: bool,
    /// The chunk the specializations were compiled from (pinned) and the
    /// last chunk instance validated against it.
    reference: Option<Arc<Chunk>>,
    last_validated: Option<Arc<Chunk>>,
    pub specs: Vec<(TypeSig, SpecState)>,
}

impl FnJitState {
    fn new(name: &str) -> Self {
        FnJitState {
            name: name.to_string(),
            calls: 0,
            guard_failures: 0,
            disabled: false,
            reference: None,
            last_validated: None,
            specs: Vec::new(),
        }
    }

    fn ever_ran(&self) -> bool {
        self.specs
            .iter()
            .any(|(_, s)| matches!(s, SpecState::Compiled(c) if c.runs > 0))
    }
}

/// A specialization chosen for one call.
#[derive(Debug, Clone, Copy)]
pub struct Selected {
    pub id: FnId,
    pub spec: usize,
    pub entry: *const u8,
    pub ret: JitType,
    pub needs_self_binding: bool,
}

/// All JIT state owned by one VM.
pub struct JitState {
    pub mode: JitMode,
    /// Report compile / reject decisions on stderr (`--jit`).
    pub verbose: bool,
    compiler: Option<JitCompiler>,
    compiler_failed: bool,
    fns: HashMap<FnId, FnJitState>,
}

impl Default for JitState {
    fn default() -> Self {
        JitState::new(JitMode::Auto)
    }
}

/// Bytecode-level equality, used to validate that a chunk presented under a
/// known `FnId` is the code we compiled.
pub fn same_code(a: &Chunk, b: &Chunk) -> bool {
    fn const_eq(x: &Constant, y: &Constant) -> bool {
        match (x, y) {
            (Constant::Int(p), Constant::Int(q)) => p == q,
            (Constant::Float(p), Constant::Float(q)) => p.to_bits() == q.to_bits(),
            (Constant::Bool(p), Constant::Bool(q)) => p == q,
            (Constant::Null, Constant::Null) => true,
            (Constant::Str(p), Constant::Str(q)) => p == q,
            _ => false,
        }
    }
    a.proto_id == b.proto_id
        && a.name == b.name
        && a.arity == b.arity
        && a.max_registers == b.max_registers
        && a.code == b.code
        && a.constants.len() == b.constants.len()
        && a.constants
            .iter()
            .zip(b.constants.iter())
            .all(|(x, y)| const_eq(x, y))
}

impl JitState {
    pub fn new(mode: JitMode) -> Self {
        JitState {
            mode,
            verbose: false,
            compiler: None,
            compiler_failed: false,
            fns: HashMap::new(),
        }
    }

    /// True when nothing has been compiled or recorded (required before a
    /// VM is moved to another thread).
    pub fn is_empty(&self) -> bool {
        self.compiler.is_none() && self.fns.is_empty()
    }

    #[allow(dead_code)] // introspection (tests, tooling)
    pub fn function(&self, id: FnId) -> Option<&FnJitState> {
        self.fns.get(&id)
    }

    /// Names of functions with at least one compiled specialization.
    #[allow(dead_code)] // introspection (tests, tooling)
    pub fn compiled_function_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .fns
            .values()
            .filter(|f| {
                f.specs
                    .iter()
                    .any(|(_, s)| matches!(s, SpecState::Compiled(_)))
            })
            .map(|f| f.name.clone())
            .collect();
        v.sort();
        v
    }

    /// Total successful native runs of functions with this name.
    #[allow(dead_code)] // introspection (tests, tooling)
    pub fn native_runs(&self, name: &str) -> u64 {
        self.fns
            .values()
            .filter(|f| f.name == name)
            .flat_map(|f| f.specs.iter())
            .map(|(_, s)| match s {
                SpecState::Compiled(c) => c.runs,
                _ => 0,
            })
            .sum()
    }

    /// Snapshot of every tracked function: (name, specialization states,
    /// disabled flag).
    #[allow(dead_code)] // introspection (tests, tooling)
    pub fn debug_states(&self) -> Vec<(String, Vec<SpecState>, bool)> {
        self.fns
            .values()
            .map(|f| {
                (
                    f.name.clone(),
                    f.specs.iter().map(|(_, s)| s.clone()).collect(),
                    f.disabled,
                )
            })
            .collect()
    }

    /// Choose (compiling if needed) a specialization for this call, or
    /// `None` to run in the VM. Covers the guards that depend only on the
    /// function and its arguments: identity/code validation, arity and
    /// argument kinds.
    ///
    /// `force_hot` treats the function as hot regardless of its call count
    /// (a loop in it is hot, see `VM::try_jit_loop_restart`); the attempt is
    /// not counted as a call.
    pub fn select(
        &mut self,
        chunk: &Arc<Chunk>,
        args: &[Value],
        gc: &Gc,
        force_hot: bool,
    ) -> Option<Selected> {
        if self.mode == JitMode::Off {
            return None;
        }
        let id = FnId::of(chunk);
        let threshold = if self.mode == JitMode::Eager || force_hot {
            1
        } else {
            HOT_THRESHOLD
        };
        let state = self
            .fns
            .entry(id)
            .or_insert_with(|| FnJitState::new(&chunk.name));
        if state.disabled {
            return None;
        }
        if !force_hot {
            state.calls = state.calls.saturating_add(1);
        }
        if state.calls < threshold && !force_hot {
            return None;
        }

        // Identity is the prototype id; additionally confirm the bytecode is
        // what we compiled (or will compile) from.
        match &state.reference {
            None => state.reference = Some(chunk.clone()),
            Some(reference) => {
                let validated = Arc::ptr_eq(reference, chunk)
                    || state
                        .last_validated
                        .as_ref()
                        .is_some_and(|l| Arc::ptr_eq(l, chunk));
                if !validated {
                    if same_code(reference, chunk) {
                        state.last_validated = Some(chunk.clone());
                    } else {
                        Self::guard_failed(state);
                        return None;
                    }
                }
            }
        }

        if args.len() != chunk.arity as usize {
            Self::guard_failed(state);
            return None;
        }
        let Some(sig) = TypeSig::of_args(args, gc) else {
            Self::guard_failed(state);
            return None;
        };

        let idx = match state.specs.iter().position(|(s, _)| *s == sig) {
            Some(i) => i,
            None => {
                let outcome =
                    Self::compile_spec(&mut self.compiler, &mut self.compiler_failed, chunk, &sig);
                if self.verbose {
                    match &outcome {
                        SpecState::Compiled(c) => {
                            eprintln!("  JIT compiled: {}{} -> {}", chunk.name, sig, c.ret)
                        }
                        SpecState::Rejected(why) => {
                            eprintln!("  JIT skip: {}{} ({})", chunk.name, sig, why)
                        }
                        SpecState::Disabled => {}
                    }
                }
                state.specs.push((sig, outcome));
                state.specs.len() - 1
            }
        };
        match &state.specs[idx].1 {
            SpecState::Compiled(c) => Some(Selected {
                id,
                spec: idx,
                entry: c.entry,
                ret: c.ret,
                needs_self_binding: c.needs_self_binding,
            }),
            _ => {
                Self::guard_failed(state);
                None
            }
        }
    }

    fn guard_failed(state: &mut FnJitState) {
        state.guard_failures = state.guard_failures.saturating_add(1);
        if state.guard_failures >= GUARD_FAILURE_LIMIT && !state.ever_ran() {
            state.disabled = true;
        }
    }

    fn compile_spec(
        compiler: &mut Option<JitCompiler>,
        compiler_failed: &mut bool,
        chunk: &Chunk,
        sig: &TypeSig,
    ) -> SpecState {
        let vf = match verifier::verify(chunk, sig) {
            Ok(vf) => vf,
            Err(reject) => return SpecState::Rejected(describe(&reject)),
        };
        if compiler.is_none() && !*compiler_failed {
            match JitCompiler::new() {
                Ok(c) => *compiler = Some(c),
                Err(_) => *compiler_failed = true,
            }
        }
        let Some(jit) = compiler.as_mut() else {
            return SpecState::Rejected("JIT backend unavailable".into());
        };
        match jit.compile(chunk, &vf) {
            Ok(entry) => SpecState::Compiled(CompiledSpec {
                entry,
                ret: vf.ret,
                needs_self_binding: vf.has_self_calls,
                runs: 0,
                deopts: 0,
            }),
            Err(e) => SpecState::Rejected(format!("codegen: {}", e)),
        }
    }

    pub fn record_run(&mut self, sel: &Selected) {
        if let Some(SpecState::Compiled(c)) = self.spec_mut(sel) {
            c.runs += 1;
        }
    }

    /// Native code bailed out; the caller re-runs the call in the VM.
    pub fn record_deopt(&mut self, sel: &Selected) {
        let disable = match self.spec_mut(sel) {
            Some(SpecState::Compiled(c)) => {
                c.deopts += 1;
                c.deopts >= MAX_DEOPTS
            }
            _ => false,
        };
        if disable {
            if let Some(s) = self.spec_mut(sel) {
                *s = SpecState::Disabled;
            }
        }
    }

    /// A VM-state entry guard (self binding, timeouts, stack) failed.
    pub fn record_guard_failure(&mut self, sel: &Selected) {
        if let Some(state) = self.fns.get_mut(&sel.id) {
            Self::guard_failed(state);
        }
    }

    fn spec_mut(&mut self, sel: &Selected) -> Option<&mut SpecState> {
        self.fns
            .get_mut(&sel.id)
            .and_then(|f| f.specs.get_mut(sel.spec))
            .map(|(_, s)| s)
    }
}

fn describe(r: &Reject) -> String {
    r.to_string()
}
