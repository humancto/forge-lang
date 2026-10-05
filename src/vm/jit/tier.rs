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

use cranelift_module::FuncId;

use crate::vm::bytecode::{Chunk, Constant};
use crate::vm::gc::Gc;
use crate::vm::jit::ir_builder::CalleeBodies;
use crate::vm::jit::jit_module::JitCompiler;
use crate::vm::jit::types::{FnId, JitType, TypeSig};
use crate::vm::jit::verifier::{
    self, push_guard, CallInfo, ClosurePath, Env, GlobalView, Guard, MemberView, OpInfo, Reject,
};
use crate::vm::value::{GcRef, Value};

/// Calls before a function is considered hot in [`JitMode::Auto`].
pub const HOT_THRESHOLD: u32 = 100;
/// Deopts after which a specialization is disabled for good.
pub const MAX_DEOPTS: u32 = 8;
/// Guard failures after which a function that never ran natively is
/// disabled (we stop classifying its arguments).
pub const GUARD_FAILURE_LIMIT: u32 = 256;
/// Longest chain of distinct functions compiled for one call (`f` calls
/// `g` calls `h` ...). Bounds compile-time recursion.
pub const MAX_CALLEE_CHAIN: usize = 8;

/// Read access to the VM's globals (and closures' captured variables) for
/// the verifier. Implemented by the VM.
pub trait Globals {
    fn global(&self, name: &str) -> GlobalView;
    /// Captured variable `index` of `closure`.
    fn upvalue(&self, closure: GcRef, index: u8) -> GlobalView;
    fn member(&self, object: &str, field: &str) -> MemberView;
    /// Does guard `g` hold now for a call of the closure `entry`?
    fn holds(&self, entry: GcRef, g: &Guard) -> bool;
}

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
    /// Body function, called directly by other compiled functions.
    pub body: FuncId,
    pub ret: JitType,
    /// Every assumption about globals the code (including all code it
    /// calls) relies on; the VM checks them before each native entry.
    pub guards: Arc<Vec<Guard>>,
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

/// A specialization chosen for one call (its guards already passed).
#[derive(Debug, Clone, Copy)]
pub struct Selected {
    pub id: FnId,
    pub spec: usize,
    pub entry: *const u8,
    pub ret: JitType,
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
    /// `None` to run in the VM. Covers the guards that depend on the
    /// function, its arguments and the globals: identity/code validation,
    /// arity, argument kinds and the specialization's [`Guard`]s (checked
    /// against `closure`, the closure being called).
    ///
    /// `force_hot` treats the function as hot regardless of its call count
    /// (a loop in it is hot, see `VM::try_jit_loop_restart`); the attempt is
    /// not counted as a call.
    pub fn select(
        &mut self,
        closure: GcRef,
        chunk: &Arc<Chunk>,
        args: &[Value],
        gc: &Gc,
        force_hot: bool,
        globals: &dyn Globals,
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
        if !Self::validate_code(state, chunk) {
            Self::guard_failed(state);
            return None;
        }

        if args.len() != chunk.arity as usize {
            Self::guard_failed(state);
            return None;
        }
        let Some(sig) = TypeSig::of_args(args, gc) else {
            Self::guard_failed(state);
            return None;
        };

        let (state, idx) = match state.specs.iter().position(|(s, _)| *s == sig) {
            Some(i) => (state, i),
            None => {
                let compiled = self.ensure_spec(closure, chunk, &sig, globals, &mut Vec::new());
                let state = self.fns.get_mut(&id)?;
                match compiled {
                    Ok(i) => (state, i),
                    Err(_) => {
                        Self::guard_failed(state);
                        return None;
                    }
                }
            }
        };
        // Entry guards on globals and captured variables. Native code
        // cannot assign either, so guards that hold here hold for the whole
        // native call.
        let selected = match &state.specs[idx].1 {
            SpecState::Compiled(c) if c.guards.iter().all(|g| globals.holds(closure, g)) => {
                Some(Selected {
                    id,
                    spec: idx,
                    entry: c.entry,
                    ret: c.ret,
                })
            }
            _ => None,
        };
        if selected.is_none() {
            Self::guard_failed(state);
        }
        selected
    }

    /// Identity is the prototype id; additionally confirm the bytecode is
    /// what we compiled (or will compile) from.
    fn validate_code(state: &mut FnJitState, chunk: &Arc<Chunk>) -> bool {
        match &state.reference {
            None => {
                state.reference = Some(chunk.clone());
                true
            }
            Some(reference) => {
                let validated = Arc::ptr_eq(reference, chunk)
                    || state
                        .last_validated
                        .as_ref()
                        .is_some_and(|l| Arc::ptr_eq(l, chunk));
                if validated {
                    return true;
                }
                if same_code(reference, chunk) {
                    state.last_validated = Some(chunk.clone());
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Index of the specialization of `chunk` for `sig`, verifying and
    /// compiling it first if there is none yet (the outcome, including a
    /// rejection, is cached). `stack` holds the functions whose compilation
    /// is in progress (the callers of this one).
    fn ensure_spec(
        &mut self,
        closure: GcRef,
        chunk: &Arc<Chunk>,
        sig: &TypeSig,
        globals: &dyn Globals,
        stack: &mut Vec<FnId>,
    ) -> Result<usize, String> {
        let id = FnId::of(chunk);
        {
            let state = self
                .fns
                .entry(id)
                .or_insert_with(|| FnJitState::new(&chunk.name));
            if !Self::validate_code(state, chunk) {
                return Err("bytecode differs from the compiled prototype".into());
            }
            if let Some(i) = state.specs.iter().position(|(s, _)| s == sig) {
                return Ok(i);
            }
        }
        stack.push(id);
        let outcome = self.compile_spec(closure, chunk, sig, globals, stack);
        stack.pop();
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
        let state = self
            .fns
            .get_mut(&id)
            .ok_or_else(|| "BUG: JIT state vanished during compilation".to_string())?;
        state.specs.push((sig.clone(), outcome));
        Ok(state.specs.len() - 1)
    }

    fn compiled(&self, id: FnId, sig: &TypeSig) -> Option<&CompiledSpec> {
        self.fns
            .get(&id)?
            .specs
            .iter()
            .find_map(|(s, st)| match st {
                SpecState::Compiled(c) if s == sig => Some(c),
                _ => None,
            })
    }

    fn guard_failed(state: &mut FnJitState) {
        state.guard_failures = state.guard_failures.saturating_add(1);
        if state.guard_failures >= GUARD_FAILURE_LIMIT && !state.ever_ran() {
            state.disabled = true;
        }
    }

    /// Verify and compile `chunk` for `sig`. `closure` is the closure being
    /// entered (its captured variables resolve `GetUpvalue`); the result is
    /// valid for every closure of the prototype that passes the guards.
    fn compile_spec(
        &mut self,
        closure: GcRef,
        chunk: &Arc<Chunk>,
        sig: &TypeSig,
        globals: &dyn Globals,
        stack: &mut Vec<FnId>,
    ) -> SpecState {
        let verified = {
            let mut env = TierEnv {
                tier: self,
                globals,
                stack,
                closure,
            };
            verifier::verify_in(chunk, sig, &mut env)
        };
        let vf = match verified {
            Ok(vf) => vf,
            Err(reject) => return SpecState::Rejected(describe(&reject)),
        };

        // Entry guards: this function's own, its self-binding, and
        // (transitively) those of every function it calls natively.
        let mut guards = vf.guards.clone();
        if vf.has_self_calls {
            push_guard(
                &mut guards,
                Guard::Closure {
                    at: ClosurePath {
                        global: Some(chunk.name.clone()),
                        upvalues: Vec::new(),
                    },
                    chunk: chunk.clone(),
                },
            );
        }
        let mut bodies = CalleeBodies::new();
        for op in vf.ops.iter().flatten() {
            let OpInfo::Call(CallInfo::Callee { id, sig, via, .. }) = op else {
                continue;
            };
            let Some(callee) = self.compiled(*id, sig) else {
                return SpecState::Rejected("BUG: verified callee is not compiled".into());
            };
            bodies.insert((*id, sig.clone()), callee.body);
            // The callee's guards are relative to the callee's closure;
            // rebase them onto the path by which this function reaches it.
            for g in callee.guards.iter() {
                let g = match g {
                    Guard::Closure { at, chunk } => Guard::Closure {
                        at: via.join(at),
                        chunk: chunk.clone(),
                    },
                    other => other.clone(),
                };
                push_guard(&mut guards, g);
            }
        }

        if self.compiler.is_none() && !self.compiler_failed {
            match JitCompiler::new() {
                Ok(c) => self.compiler = Some(c),
                Err(_) => self.compiler_failed = true,
            }
        }
        let Some(jit) = self.compiler.as_mut() else {
            return SpecState::Rejected("JIT backend unavailable".into());
        };
        match jit.compile(chunk, &vf, &bodies) {
            Ok((entry, body)) => SpecState::Compiled(CompiledSpec {
                entry,
                body,
                ret: vf.ret,
                guards: Arc::new(guards),
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

/// The verifier's environment during compilation: globals from the VM,
/// callees compiled (and cached) through the tier.
struct TierEnv<'a> {
    tier: &'a mut JitState,
    globals: &'a dyn Globals,
    stack: &'a mut Vec<FnId>,
    /// The closure whose code is being verified.
    closure: GcRef,
}

impl Env for TierEnv<'_> {
    fn global(&self, name: &str) -> GlobalView {
        self.globals.global(name)
    }

    fn upvalue(&self, index: u8) -> GlobalView {
        self.globals.upvalue(self.closure, index)
    }

    fn member(&self, object: &str, field: &str) -> MemberView {
        self.globals.member(object, field)
    }

    fn callee(
        &mut self,
        closure: GcRef,
        chunk: &Arc<Chunk>,
        sig: &TypeSig,
    ) -> Result<JitType, String> {
        let id = FnId::of(chunk);
        if self.stack.contains(&id) {
            // Mutual recursion: the callee's return kind would depend on
            // the caller being compiled. Only direct self-recursion is
            // supported.
            return Err("mutually recursive".into());
        }
        if self.stack.len() >= MAX_CALLEE_CHAIN {
            return Err("call chain too deep".into());
        }
        let idx = self
            .tier
            .ensure_spec(closure, chunk, sig, self.globals, self.stack)?;
        let spec = self
            .tier
            .fns
            .get(&id)
            .and_then(|f| f.specs.get(idx))
            .map(|(_, s)| s);
        match spec {
            Some(SpecState::Compiled(c)) => Ok(c.ret),
            Some(SpecState::Rejected(why)) => Err(why.clone()),
            _ => Err("disabled after repeated deopts".into()),
        }
    }
}
