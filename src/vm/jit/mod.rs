//! Forge JIT — typed, guarded, deoptimizing specializations of VM functions.
//!
//! # Contract
//!
//! The JIT is an *optimization*: for any program, running with the JIT
//! (auto-tiered or `--jit`) must produce exactly what the bytecode VM
//! produces. Every piece below exists to keep that promise.
//!
//! # Pipeline
//!
//! ```text
//!  call_value(closure, args)
//!     │
//!     ├─ FnId::of(&closure.function.chunk)        identity, never the name
//!     ├─ TypeSig::of_args(args)                   runtime type signature
//!     │      └─ None (unsupported kind) ──────────► guard failure → VM
//!     ├─ JitState::lookup((FnId, TypeSig))
//!     │      ├─ miss & hot ─► verifier::verify ─► ir_builder ─► Compiled
//!     │      │                       └─ Err(Reject) ─► cached as Rejected
//!     │      └─ Rejected / disabled ─────────────► VM
//!     └─ Compiled: entry guards (arity, self-binding, no active timeout,
//!        stack budget) ─► native code ─► Ok(value) | Deopt ─► re-run in VM
//! ```
//!
//! * [`types`] — [`types::FnId`] (per-prototype identity), [`types::JitType`]
//!   (value kinds a specialization may assume) and [`types::TypeSig`].
//! * [`verifier`] — an explicit opcode allowlist plus a flow-sensitive type
//!   inference over the CFG. It *proves* a function compilable for a given
//!   signature or returns a structured [`verifier::Reject`] explaining why
//!   not. The IR builder only ever sees verified functions and has no
//!   fallback arms: anything not proven is never compiled.
//! * [`ir_builder`] — lowers a [`verifier::VerifiedFn`] to Cranelift IR.
//! * [`jit_module`] — owns the Cranelift module and code memory.
//! * [`tier`] — the per-VM cache keyed by `(FnId, TypeSig)`, hotness
//!   counters, guard-failure/deopt accounting and the native-call ABI.
//!
//! # Deoptimization
//!
//! Every specialization the verifier accepts is *pure*: it reads only its
//! arguments, writes only its own registers and calls only itself. So when
//! native code meets a situation whose VM semantics it does not implement
//! (integer overflow promoting to float, division by zero, `i64::MIN / -1`,
//! VM stack-depth exhaustion, task cancellation) it sets a deopt flag and
//! unwinds; the dispatcher then re-executes the *whole call* in the VM, which
//! produces the exact VM result or error. Purity makes re-execution
//! observationally identical. Each deopt is counted; a specialization that
//! deopts too often is disabled. Functions whose calls repeatedly fail the
//! type guards are disabled too, so we stop paying for guard checks.
//!
//! # Loop tier-up
//!
//! Call counts alone never make a function hot if it is called once and
//! spends its time in a loop. Each VM frame also counts backward jumps; at
//! `LOOP_HOT_THRESHOLD` (`vm::machine`) the VM asks [`tier::JitState::select`]
//! for a specialization with `force_hot` and the arguments the frame was
//! entered with (`CallFrame::entry_args`), and on success *restarts the whole
//! call natively*, using its result as the frame's result. Purity makes this
//! the mirror image of deopt-by-re-execution: the VM work done so far had no
//! observable effect. A deopt or failed guard leaves the frame running in the
//! VM. True on-stack replacement (entering at the loop header with the
//! frame's live registers) would avoid the bounded repeated work and could
//! extend tier-up to impure callers; it needs a second entry block per loop
//! header in [`ir_builder`] and is left as a follow-up.
//!
//! # Extending
//!
//! New tiers (floats, strings, calls to other functions, ...) are added by:
//! 1. adding a [`types::JitType`] variant and its guard in
//!    [`types::JitType::of_value`] plus its boxing in `tier`,
//! 2. teaching [`verifier`] which opcodes accept/produce it (with exact VM
//!    semantics, including every error path either compiled or deopted), and
//! 3. lowering those opcodes in [`ir_builder`].
//!
//! Anything the verifier has not been taught stays rejected.

pub mod ir_builder;
pub mod jit_module;
pub mod runtime;
pub mod tier;
pub mod types;
pub mod verifier;
