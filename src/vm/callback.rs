//! Calling Forge functions back from native builtins.
//!
//! `map`, `filter`, `reduce`, `sort` with a comparator, `find`, `any`,
//! `all`, `flat_map`, `for_each`, ... and the `.stream()` stages call a
//! Forge function once per element. They go through [`VM::call_callback`]
//! instead of the general [`VM::call_value`], which re-resolves everything
//! on every call. The work done per element is what made these builtins
//! the VM's slowest common shape (`benchmarks/vm/map_filter.fg`).
//!
//! # What is cached
//!
//! The VM remembers the last few closures called back ([`CallbackMemo`],
//! a handful of entries: a stream pipeline alternates between its stages'
//! functions). An entry holds what entering the closure needs (arity,
//! register window, whether it has a loop) and, with the JIT, the
//! specialization the tier selected for the last argument kinds.
//!
//! * **VM path.** The frame is entered from the argument slice: no
//!   argument `Vec`, no name clone, no tier lookup once the tier has
//!   declined repeatedly (see [`DECLINE_LIMIT`]). `entry_args` (kept for
//!   loop tier-up, `try_jit_loop_restart`) is only filled when the function
//!   has a backward jump; a function without one is never restarted.
//! * **JIT path.** A cached specialization is re-entered directly when the
//!   arguments match its signature and *nothing could have invalidated its
//!   entry guards since they were checked*. Guards are facts about globals,
//!   captured variables (`select`'s guards) and VM state (depth, fuel,
//!   `squad`/`timeout` scopes; `jit_vm_guards_ok`). Only running Forge
//!   bytecode or a native builtin can change any of them, so the VM counts
//!   both in [`VM::exec_epoch`] (`run_until` entry and `call_native`
//!   entry). The cache records the epoch at which every guard held, plus
//!   `Globals::version` (any global write, even from Rust), the frame depth
//!   and the `JitState` instance; a mismatch in any of them
//!   sends the call back through the full `jit_select`. Native code cannot
//!   run Forge code or assign globals (the JIT's purity invariant), so a
//!   run of native code never invalidates the guards it was entered under.
//!   A deopt runs the callback in the VM, which bumps the epoch.
//!
//! # Semantics kept identical to `call_value`
//!
//! Callbacks stay lenient (missing parameters are `null`, extra arguments
//! are dropped); errors propagate with the same message and stack trace
//! (the frame is entered and unwound by the same `enter_closure` /
//! `run_until`); `try`/`catch` inside and around callbacks, the depth limit
//! and fuel/timeout safe points are unchanged. GC rooting follows the rule
//! in `vm/gc.rs`: auto-pinning is suspended while bytecode runs and the
//! result is pinned into the caller's native scope. A callback that is not
//! a closure (a builtin, a `__call__` object, a non-callable value), or any
//! call while `--profile` is on, simply goes through `call_value`.
//!
//! One deliberate addition: like a direct call (`Call` opcode), every
//! callback first polls cancellation, so a `squad`/`timeout`/sandbox
//! cancellation stops a long `map` between elements.

use super::machine::{VMError, VM};
use super::value::{GcRef, ObjKind, Value};

/// Closures remembered by [`CallbackMemo`].
const MEMO_SIZE: usize = 4;

/// After this many consecutive calls the JIT tier declined (not hot yet,
/// rejected by the verifier, a guard failed, ...), a callee only asks the
/// tier every [`DECLINE_RETRY`]th call. Asking costs a map lookup per call;
/// retrying keeps a later signature change able to tier up. Twice the
/// tier's hotness threshold, so a function always gets the chance to
/// become hot first.
#[cfg(feature = "jit")]
const DECLINE_LIMIT: u32 = 2 * super::jit::tier::HOT_THRESHOLD;
#[cfg(feature = "jit")]
const DECLINE_RETRY: u32 = 1024;

/// One remembered closure.
struct Callee {
    closure: GcRef,
    /// The closure's code, held so the address cannot be reused while the
    /// entry exists (identity check in [`VM::callee_slot`]).
    chunk: std::sync::Arc<super::bytecode::Chunk>,
    arity: usize,
    frame_size: usize,
    /// The code has a backward jump, so a call can be restarted natively by
    /// loop tier-up and needs `entry_args`.
    has_loop: bool,
    #[cfg(feature = "jit")]
    jit: CalleeJit,
}

#[cfg(feature = "jit")]
#[derive(Default)]
struct CalleeJit {
    cached: Option<CachedSpec>,
    /// Consecutive calls for which the tier selected nothing.
    declined: u32,
}

/// A specialization selected for this callee, with the conditions under
/// which its entry guards are known to hold.
#[cfg(feature = "jit")]
struct CachedSpec {
    sig: super::jit::types::TypeSig,
    sel: super::jit::tier::Selected,
    /// `VM::exec_epoch` when every guard was checked.
    epoch: u64,
    /// `Globals::version` when every guard was checked.
    globals_version: u64,
    /// Frames beneath the call when the guards were checked.
    depth: usize,
    /// The `JitState` that owns `sel.entry`.
    jit_instance: u64,
}

/// The VM's small cache of recently called-back closures.
#[derive(Default)]
pub(crate) struct CallbackMemo {
    entries: Vec<Callee>,
    /// Next entry to replace once full (round robin).
    next: usize,
}

impl CallbackMemo {
    fn insert(&mut self, callee: Callee) -> usize {
        if self.entries.len() < MEMO_SIZE {
            self.entries.push(callee);
            return self.entries.len() - 1;
        }
        let slot = self.next;
        self.next = (self.next + 1) % MEMO_SIZE;
        self.entries[slot] = callee;
        slot
    }
}

impl VM {
    /// Call `func` with `args` on behalf of a native builtin (see the module
    /// documentation). Same result, errors and side effects as
    /// `self.call_value(func, args.to_vec())`, faster when called
    /// repeatedly with the same closure.
    pub(crate) fn call_callback(&mut self, func: Value, args: &[Value]) -> Result<Value, VMError> {
        if self.is_cancelled() {
            return Err(VMError::new("task cancelled"));
        }
        let Some(slot) = self.callee_slot(func) else {
            return self.call_value(func, args.to_vec());
        };

        #[cfg(feature = "jit")]
        if let Some(value) = self.callback_native(slot, args)? {
            // Allocations made decoding the result were auto-pinned (the
            // builtin's native scope is active); this covers the rest.
            self.gc.pin_value(value);
            return Ok(value);
        }

        let callee = &self.callbacks.entries[slot];
        let (closure, arity, frame_size, has_loop) = (
            callee.closure,
            callee.arity,
            callee.frame_size,
            callee.has_loop,
        );
        // Exactly `call_value`'s rooting: bytecode run by the callee is not
        // auto-pinned, and the result is pinned into the caller's scope.
        let was_pinning = self.gc.set_pinning(false);
        let result = self.run_callee(closure, arity, frame_size, has_loop, args);
        self.gc.set_pinning(was_pinning);
        if let Ok(v) = &result {
            self.gc.pin_value(*v);
        }
        result
    }

    /// Enter and run one call of a remembered closure in the VM.
    fn run_callee(
        &mut self,
        closure: GcRef,
        arity: usize,
        frame_size: usize,
        has_loop: bool,
        args: &[Value],
    ) -> Result<Value, VMError> {
        self.enter_closure(closure, arity, frame_size, args)?;
        let boundary = self.frames.len() - 1;
        if has_loop {
            self.frames[boundary].entry_args = args.to_vec();
        }
        self.run_until(boundary)
    }

    /// The memo slot for `func` (adding it if needed), or `None` when it
    /// must take the general `call_value` path: not a closure, or the
    /// profiler is on (it records named calls on that path).
    fn callee_slot(&mut self, func: Value) -> Option<usize> {
        if self.profiler.is_enabled() {
            return None;
        }
        let r = func.as_obj()?;
        let ObjKind::Closure(closure) = &self.gc.get(r)?.kind else {
            return None;
        };
        let chunk = &closure.function.chunk;
        // The `GcRef` alone is not an identity (a freed slot is reused);
        // the entry holds the chunk `Arc`, so an equal pointer is the same
        // closure object.
        if let Some(i) = self
            .callbacks
            .entries
            .iter()
            .position(|c| c.closure == r && std::sync::Arc::ptr_eq(&c.chunk, chunk))
        {
            return Some(i);
        }
        let callee = Callee {
            closure: r,
            chunk: chunk.clone(),
            arity: chunk.arity as usize,
            frame_size: (chunk.max_registers as usize).max(1),
            has_loop: chunk.code.iter().any(|&inst| {
                super::bytecode::decode_op(inst) == super::bytecode::OpCode::Loop as u8
            }),
            #[cfg(feature = "jit")]
            jit: CalleeJit::default(),
        };
        Some(self.callbacks.insert(callee))
    }

    /// Run the call natively if the JIT has (or now selects) a
    /// specialization for these arguments. `None`: run it in the VM.
    #[cfg(feature = "jit")]
    fn callback_native(&mut self, slot: usize, args: &[Value]) -> Result<Option<Value>, VMError> {
        if self.jit.mode == super::jit::tier::JitMode::Off {
            return Ok(None);
        }
        let depth = self.frames.len();
        let epoch = self.exec_epoch;
        let globals_version = self.globals.version();
        let jit_instance = self.jit.instance();

        // Fast path: the cached selection still applies.
        let cached = {
            let callee = &self.callbacks.entries[slot];
            match &callee.jit.cached {
                Some(c)
                    if c.epoch == epoch
                        && c.globals_version == globals_version
                        && c.depth == depth
                        && c.jit_instance == jit_instance
                        && c.sig.matches(args, &self.gc) =>
                {
                    Some(c.sel)
                }
                _ => None,
            }
        };
        let sel = match cached {
            Some(sel) => sel,
            None => {
                let callee = &mut self.callbacks.entries[slot];
                callee.jit.cached = None;
                let declined = callee.jit.declined;
                if declined >= DECLINE_LIMIT && declined % DECLINE_RETRY != 0 {
                    callee.jit.declined = declined.saturating_add(1);
                    return Ok(None);
                }
                let (closure, chunk) = (callee.closure, callee.chunk.clone());
                match self.jit_select(closure, &chunk, args, depth, false) {
                    Some(sel) => {
                        // `select` matched `args` against this signature.
                        let sig = super::jit::types::TypeSig::of_args(args, &self.gc);
                        let callee = &mut self.callbacks.entries[slot];
                        callee.jit.declined = 0;
                        if let Some(sig) = sig {
                            callee.jit.cached = Some(CachedSpec {
                                sig,
                                sel,
                                epoch,
                                globals_version,
                                depth,
                                jit_instance,
                            });
                        }
                        sel
                    }
                    None => {
                        let callee = &mut self.callbacks.entries[slot];
                        callee.jit.declined = callee.jit.declined.saturating_add(1);
                        return Ok(None);
                    }
                }
            }
        };
        let result = self.jit_run_selected(&sel, args, depth);
        if !matches!(result, Ok(Some(_))) {
            // Deopt or error: the tier's state for this specialization
            // changed (it may now be disabled); select again next time.
            if let Some(callee) = self.callbacks.entries.get_mut(slot) {
                callee.jit.cached = None;
            }
        }
        result
    }
}
