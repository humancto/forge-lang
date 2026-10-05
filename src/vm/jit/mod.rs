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
//!     │      ├─ miss & hot ─► verifier::verify_in ─► ir_builder ─► Compiled
//!     │      │      │                └─ Err(Reject) ─► cached as Rejected
//!     │      │      └─ callee g(sig') ─► same pipeline for g (cached)
//!     │      └─ Rejected / disabled ─────────────► VM
//!     └─ Compiled: entry guards (arity, globals the code relies on, no
//!        active timeout, stack budget) ─► native code ─► Ok(value)
//!                                                   | Deopt ─► re-run in VM
//! ```
//!
//! * [`types`] — [`types::FnId`] (per-prototype identity), [`types::JitType`]
//!   (value kinds a specialization may assume: Int, Bool, Float) and
//!   [`types::TypeSig`].
//! * [`verifier`] — an explicit opcode allowlist plus a flow-sensitive type
//!   inference over the CFG. It *proves* a function compilable for a given
//!   signature or returns a structured [`verifier::Reject`] explaining why
//!   not. The IR builder only ever sees verified functions and has no
//!   fallback arms: anything not proven is never compiled.
//! * [`ir_builder`] — lowers a [`verifier::VerifiedFn`] to Cranelift IR.
//! * [`math_bridges`] — pure `extern "C"` float helpers (fmod, sin, pow,
//!   NaN-ignoring min/max, ...) that evaluate the VM's own Rust expression.
//! * [`jit_module`] — owns the Cranelift module and code memory.
//! * [`tier`] — the per-VM cache keyed by `(FnId, TypeSig)`, hotness
//!   counters, guard-failure/deopt accounting and the native-call ABI.
//!
//! # Purity, guards and calls
//!
//! Every specialization the verifier accepts is *pure*: it reads only its
//! arguments, writes only its own registers, and calls only itself, other
//! verified pure functions (direct native calls of their specializations,
//! [`verifier::CallInfo::Callee`]) and pure builtins (`math.*`, `float`,
//! `int`). It may read a global only to call it or, for `math`, to read a
//! Float constant. Each such read is resolved at compile time against the
//! VM's globals and recorded as a [`verifier::Guard`] (a function's guards
//! include its callees', transitively). The VM checks every guard before
//! each native entry; since native code cannot run Forge code or assign
//! globals, guards that hold at entry hold for the whole native call, so
//! rebinding a global (`g = fn(x) {..}`, `m.sqrt = ..`) simply makes the
//! next call run in the VM.
//!
//! Direct callees are compiled as separate Cranelift functions in the same
//! module and called through their body entry (`ctx`, `depth + 1`, typed
//! arguments). Only self-recursion is supported; mutual recursion is
//! rejected because the callee's result kind would depend on the caller.
//!
//! # Deoptimization
//!
//! When native code meets a situation whose VM semantics it does not
//! implement (Int overflow promoting to Float, division by zero, `MIN / -1`,
//! `math.floor` of a value with no Int representation, VM stack-depth
//! exhaustion, task cancellation) it sets a deopt flag and unwinds through
//! every native frame; the dispatcher then re-executes the *whole call* in
//! the VM, which produces the exact VM result or error. Purity makes
//! re-execution observationally identical. Each deopt is counted; a
//! specialization that deopts too often is disabled. Functions whose calls
//! repeatedly fail the type guards are disabled too, so we stop paying for
//! guard checks.
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
//! VM.
//!
//! ## Why restart and not on-stack replacement
//!
//! True OSR (entering native code at the loop header with the frame's live
//! registers) was evaluated and deliberately not done:
//!
//! * The only thing it buys over restart is skipping at most
//!   `LOOP_HOT_THRESHOLD` (1000) iterations of repeated work, once per call:
//!   microseconds, against compilation times in the hundreds of µs.
//! * It would not widen what can be compiled. Entering mid-function requires
//!   the frame's registers to have the kinds the code was specialized for,
//!   which still requires the whole function to verify; and the restart's
//!   purity requirement is the same requirement deopt-by-re-execution
//!   already imposes on every compiled function. Impure functions need real
//!   *deopt* metadata (materializing native state back into a VM frame at
//!   any guard), not just OSR entry — that is the prerequisite for the
//!   next tier, and the place to add OSR along with it.
//! * The cost would be real: a second entry block per loop header whose
//!   signature is the live register set at that header (typed per the
//!   verifier's state there, including NaN-boxed values that must be
//!   unboxed and type-checked), another ABI between `machine.rs` and native
//!   code, and its own deopt path.
//!
//! # Extending
//!
//! New tiers (strings, arrays, ...) are added by:
//! 1. adding a [`types::JitType`] variant and its guard in
//!    [`types::JitType::of_value`] plus its boxing in [`types::JitType`],
//! 2. teaching [`verifier`] which opcodes accept/produce it (with exact VM
//!    semantics, including every error path either compiled or deopted), and
//! 3. lowering those opcodes in [`ir_builder`].
//!
//! New pure builtins are added to [`verifier::PureOp`] (result kinds) and
//! `ir_builder::lower_pure`, with a [`verifier::Guard`] on the global that
//! provides them. Anything the verifier has not been taught stays rejected.

pub mod ir_builder;
pub mod jit_module;
pub mod math_bridges;
pub mod runtime;
pub mod tier;
pub mod types;
pub mod verifier;
