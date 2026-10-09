//! Deterministic resource limits shared by every execution engine.
//!
//! A [`Limits`] value is the *configuration* a host picks (CLI flags,
//! [`crate::sandbox::Sandbox`], `forge mcp`). A [`Budget`] is one run's
//! *accounting* against it: fuel spent, memory in use, handles open,
//! modules imported. Engines and the stdlib ask the **active budget** — the
//! same thread-scoped model as [`crate::permissions`]:
//!
//! * a process-wide budget set with [`set_global`] (the CLI), or
//! * a per-thread override installed with [`scope`] (the sandbox runs each
//!   program on its own thread under a fresh budget).
//!
//! # Invariant: forks inherit the budget
//!
//! [`crate::permissions::inherit`] (and therefore `permissions::spawn` and
//! `recursion::spawn_worker`) carries the current budget into the new
//! thread together with the permission policy, so `spawn`ed tasks, `timeout`
//! blocks and squads charge the run that started them.
//!
//! # What each limit means
//!
//! * **Fuel** ([`Limits::max_fuel`]) is a count of execution steps. The
//!   bytecode VM charges one unit per instruction; the interpreter charges
//!   one unit per statement, function call and loop iteration. Engines count
//!   in a local countdown and settle with the budget only at their existing
//!   safe points ([`Meter::safepoint`]), so the hot path stays a decrement
//!   and a branch. For a single-threaded program the step at which fuel runs
//!   out is exactly reproducible on a given engine. The two engines count
//!   different units, so the same program spends different amounts of fuel
//!   on each. JIT-compiled code has no fuel counter: while a fuel limit is
//!   active the VM does not enter native code (functions run in the VM).
//! * **Memory** ([`Limits::max_memory`]): the VM accounts the bytes held by
//!   its GC heap (strings, arrays, objects, maps, sets, closures, boxed
//!   ints); when an allocation pushes the heap past the limit it collects
//!   first and fails only if the live heap is still too big. The interpreter
//!   has no heap of its own, so it uses the allocation meter of
//!   [`CountingAllocator`]: bytes allocated minus bytes freed by the threads
//!   of the run, polled at every statement. Both are approximations (the VM
//!   estimates object sizes; the interpreter also counts transient copies)
//!   and may overshoot by what a single statement or builtin allocates —
//!   which is why the size caps below exist.
//! * **Size caps** ([`Limits::max_string_bytes`],
//!   [`Limits::max_collection_len`]) reject one huge value *before* it is
//!   built (`"x".repeat(n)`, `range(1e12)`, doubling concatenation). With a
//!   memory limit and no explicit cap, the caps default to what the memory
//!   limit could hold.
//! * **Handles** ([`Resource`]): concurrently open files, sockets,
//!   subprocesses and tasks. A [`Slot`] holds one and gives it back on drop.
//! * **Imports** ([`Limits::max_imports`]): module files loaded per run.
//!
//! # Errors
//!
//! Fuel and memory exhaustion are **fatal**: `try`/`catch`, `safe` and
//! `retry` do not catch them, and the budget remembers the trip
//! ([`Budget::tripped`]) so every later safe point fails too and the host
//! can report the limit even if a builtin swallowed the error. The other
//! limits raise ordinary (catchable) runtime errors. Every message starts
//! with a stable prefix ([`FUEL_EXHAUSTED`], [`MEMORY_LIMIT_EXCEEDED`],
//! [`RESOURCE_LIMIT_EXCEEDED`]) that [`classify`] recognises.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicIsize, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

/// Prefix of the fatal error raised when a run's fuel is spent.
pub const FUEL_EXHAUSTED: &str = "fuel exhausted";
/// Prefix of the fatal error raised when a run exceeds its memory limit.
pub const MEMORY_LIMIT_EXCEEDED: &str = "memory limit exceeded";
/// Prefix of the (catchable) error raised by every other limit.
pub const RESOURCE_LIMIT_EXCEEDED: &str = "resource limit exceeded";

/// Bytes assumed per collection element when deriving
/// [`Limits::max_collection_len`] from [`Limits::max_memory`].
const BYTES_PER_ELEMENT: usize = 16;

/// Resource limits for one run. `None` everywhere (the default) means
/// unlimited, which is what plain `forge run` uses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Limits {
    /// Execution steps (see the module docs for what a step is).
    pub max_fuel: Option<u64>,
    /// Bytes of memory the program may hold.
    pub max_memory: Option<usize>,
    /// Concurrently open files (filesystem calls, SQLite connections).
    pub max_open_files: Option<usize>,
    /// Concurrently open network connections (HTTP requests, WebSockets,
    /// PostgreSQL/MySQL connections).
    pub max_sockets: Option<usize>,
    /// Concurrently running subprocesses (`sh`, `run_command`, ...).
    pub max_processes: Option<usize>,
    /// Concurrently running tasks (`spawn`, `timeout` blocks).
    pub max_tasks: Option<usize>,
    /// Longest string, in bytes, a program may build.
    pub max_string_bytes: Option<usize>,
    /// Most elements a program may put in one array (e.g. `range(n)`).
    pub max_collection_len: Option<usize>,
    /// Module files a program may import.
    pub max_imports: Option<usize>,
}

impl Limits {
    /// No limits at all.
    pub fn none() -> Self {
        Limits::default()
    }

    /// True when nothing is limited.
    pub fn is_unlimited(&self) -> bool {
        *self == Limits::default()
    }

    /// Effective cap on one string's length in bytes.
    pub fn string_cap(&self) -> usize {
        self.max_string_bytes
            .or(self.max_memory)
            .unwrap_or(usize::MAX)
    }

    /// Effective cap on one collection's element count.
    pub fn collection_cap(&self) -> usize {
        self.max_collection_len
            .or(self.max_memory.map(|m| m / BYTES_PER_ELEMENT))
            .unwrap_or(usize::MAX)
    }

    /// The size caps engines check when they build values.
    pub fn caps(&self) -> Caps {
        Caps {
            string: self.string_cap(),
            collection: self.collection_cap(),
        }
    }

    fn handle_cap(&self, r: Resource) -> Option<usize> {
        match r {
            Resource::Files => self.max_open_files,
            Resource::Sockets => self.max_sockets,
            Resource::Processes => self.max_processes,
            Resource::Tasks => self.max_tasks,
        }
    }

    /// Human-readable list of the limits that are set (for policy
    /// summaries), e.g. `["fuel 1000000 steps", "memory 256 MiB"]`.
    pub fn describe(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(n) = self.max_fuel {
            out.push(format!("fuel {} steps", n));
        }
        if let Some(n) = self.max_memory {
            out.push(format!("memory {}", format_bytes(n)));
        }
        for r in Resource::ALL {
            if let Some(n) = self.handle_cap(r) {
                out.push(format!("{} {}", n, r.name()));
            }
        }
        if let Some(n) = self.max_string_bytes {
            out.push(format!("strings up to {}", format_bytes(n)));
        }
        if let Some(n) = self.max_collection_len {
            out.push(format!("collections up to {} elements", n));
        }
        if let Some(n) = self.max_imports {
            out.push(format!("{} imports", n));
        }
        out
    }
}

/// Size caps copied out of [`Limits`] so engines can check them without
/// touching thread-local state on hot paths. `usize::MAX` = unlimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    pub string: usize,
    pub collection: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Caps::UNLIMITED
    }
}

impl Caps {
    pub const UNLIMITED: Caps = Caps {
        string: usize::MAX,
        collection: usize::MAX,
    };

    /// The caps of the budget active on this thread.
    pub fn current() -> Caps {
        current().map_or(Caps::UNLIMITED, |b| b.limits.caps())
    }

    /// Fail if a string of `len` bytes would exceed the cap.
    #[inline]
    pub fn check_string(&self, len: usize) -> Result<(), String> {
        if len > self.string {
            Err(format!(
                "{}: a string of {} bytes is larger than the limit of {} bytes",
                RESOURCE_LIMIT_EXCEEDED, len, self.string
            ))
        } else {
            Ok(())
        }
    }

    /// Fail if a collection of `len` elements would exceed the cap.
    #[inline]
    pub fn check_collection(&self, len: usize) -> Result<(), String> {
        if len > self.collection {
            Err(format!(
                "{}: a collection of {} elements is larger than the limit of {} elements",
                RESOURCE_LIMIT_EXCEEDED, len, self.collection
            ))
        } else {
            Ok(())
        }
    }
}

/// Kinds of handles a run may hold open at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resource {
    Files,
    Sockets,
    Processes,
    Tasks,
}

impl Resource {
    pub const ALL: [Resource; 4] = [
        Resource::Files,
        Resource::Sockets,
        Resource::Processes,
        Resource::Tasks,
    ];

    /// Plural noun used in messages ("open files", "sockets", ...).
    pub fn name(self) -> &'static str {
        match self {
            Resource::Files => "open files",
            Resource::Sockets => "sockets",
            Resource::Processes => "subprocesses",
            Resource::Tasks => "tasks",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// A fatal limit that has been hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trip {
    Fuel,
    Memory,
}

const TRIP_NONE: u8 = 0;
const TRIP_FUEL: u8 = 1;
const TRIP_MEMORY: u8 = 2;

/// Kind of a limit error, recovered from its message by [`classify`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    Fuel,
    Memory,
    Resource,
}

/// Which limit an error message reports, if any. Engines and hosts wrap
/// messages (`task failed: ...`), so the prefix may follow other text.
pub fn classify(message: &str) -> Option<LimitKind> {
    let at = |p: &str| message.starts_with(p) || message.contains(&format!(": {}", p));
    if at(FUEL_EXHAUSTED) {
        Some(LimitKind::Fuel)
    } else if at(MEMORY_LIMIT_EXCEEDED) {
        Some(LimitKind::Memory)
    } else if message.starts_with(RESOURCE_LIMIT_EXCEEDED) {
        Some(LimitKind::Resource)
    } else {
        None
    }
}

/// One run's accounting against its [`Limits`]. Shared (via `Arc`) by every
/// thread of the run.
#[derive(Debug)]
pub struct Budget {
    limits: Limits,
    fuel_used: AtomicU64,
    /// Bytes allocated minus bytes freed by the run's threads (fed by
    /// [`CountingAllocator`]). Signed: memory allocated before the run and
    /// freed during it counts negative.
    mem_used: AtomicIsize,
    open: [AtomicUsize; 4],
    imports: AtomicUsize,
    tripped: AtomicU8,
}

impl Budget {
    pub fn new(limits: Limits) -> Arc<Budget> {
        Arc::new(Budget {
            limits,
            fuel_used: AtomicU64::new(0),
            mem_used: AtomicIsize::new(0),
            open: Default::default(),
            imports: AtomicUsize::new(0),
            tripped: AtomicU8::new(TRIP_NONE),
        })
    }

    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Fuel spent so far (as settled at safe points).
    #[allow(dead_code)] // unused in the `forge` binary's copy of this module
    pub fn fuel_used(&self) -> u64 {
        self.fuel_used.load(Ordering::Relaxed)
    }

    /// Bytes the run's threads currently hold according to the allocation
    /// meter (0 when [`CountingAllocator`] is not installed).
    #[allow(dead_code)] // unused in the `forge` binary's copy of this module
    pub fn memory_used(&self) -> usize {
        self.mem_used.load(Ordering::Relaxed).max(0) as usize
    }

    /// Count `bytes` the run already holds when it starts (state carried
    /// over from an earlier run, such as an MCP session's interpreter), so
    /// [`Limits::max_memory`] bounds the total, not just this run's growth.
    pub fn preload_memory(&self, bytes: usize) {
        let bytes = isize::try_from(bytes).unwrap_or(isize::MAX);
        self.mem_used.fetch_add(bytes, Ordering::Relaxed);
    }

    /// The fatal limit this run hit, if any. Sticky.
    pub fn tripped(&self) -> Option<Trip> {
        match self.tripped.load(Ordering::Acquire) {
            TRIP_FUEL => Some(Trip::Fuel),
            TRIP_MEMORY => Some(Trip::Memory),
            _ => None,
        }
    }

    /// Record a fatal limit (the first one wins) and return its message.
    pub fn trip(&self, kind: Trip) -> String {
        let code = match kind {
            Trip::Fuel => TRIP_FUEL,
            Trip::Memory => TRIP_MEMORY,
        };
        let _ = self
            .tripped
            .compare_exchange(TRIP_NONE, code, Ordering::AcqRel, Ordering::Acquire);
        self.trip_message()
            .unwrap_or_else(|| self.message_for(kind))
    }

    /// The message of the fatal limit this run hit, if any.
    pub fn trip_message(&self) -> Option<String> {
        self.tripped().map(|k| self.message_for(k))
    }

    fn message_for(&self, kind: Trip) -> String {
        match kind {
            Trip::Fuel => fuel_exhausted_message(self.limits.max_fuel.unwrap_or(0)),
            Trip::Memory => memory_exceeded_message(self.limits.max_memory.unwrap_or(0)),
        }
    }

    /// Charge `units` of fuel and return how many are left (`u64::MAX` when
    /// fuel is unlimited).
    #[inline]
    pub fn charge_fuel(&self, units: u64) -> u64 {
        let Some(max) = self.limits.max_fuel else {
            return u64::MAX;
        };
        let used = self
            .fuel_used
            .fetch_add(units, Ordering::Relaxed)
            .saturating_add(units);
        max.saturating_sub(used)
    }

    /// True when the allocation meter shows more than the memory limit.
    #[inline]
    pub fn over_memory(&self) -> bool {
        match self.limits.max_memory {
            Some(max) => self.mem_used.load(Ordering::Relaxed) > max as isize,
            None => false,
        }
    }

    /// Take one handle of kind `r`, or fail when the run already holds as
    /// many as its limit allows.
    pub fn acquire(self: &Arc<Self>, r: Resource) -> Result<Slot, String> {
        let counter = &self.open[r.index()];
        let prev = counter.fetch_add(1, Ordering::AcqRel);
        if let Some(max) = self.limits.handle_cap(r) {
            if prev >= max {
                counter.fetch_sub(1, Ordering::AcqRel);
                return Err(format!(
                    "{}: too many {} (limit {})",
                    RESOURCE_LIMIT_EXCEEDED,
                    r.name(),
                    max
                ));
            }
        }
        Ok(Slot(Some((self.clone(), r))))
    }

    /// Handles of kind `r` currently held.
    #[allow(dead_code)] // unused in the `forge` binary's copy of this module
    pub fn open_count(&self, r: Resource) -> usize {
        self.open[r.index()].load(Ordering::Acquire)
    }

    /// Count one module import.
    pub fn charge_import(&self) -> Result<(), String> {
        let n = self.imports.fetch_add(1, Ordering::AcqRel) + 1;
        match self.limits.max_imports {
            Some(max) if n > max => Err(format!(
                "{}: more than {} imports",
                RESOURCE_LIMIT_EXCEEDED, max
            )),
            _ => Ok(()),
        }
    }
}

/// The fatal message for spent fuel.
pub fn fuel_exhausted_message(limit: u64) -> String {
    format!(
        "{}: the program ran more than {} steps\n  hint: raise the budget with --max-fuel (or Sandbox::max_fuel), or do less work",
        FUEL_EXHAUSTED, limit
    )
}

/// The fatal message for exceeding the memory limit.
pub fn memory_exceeded_message(limit: usize) -> String {
    format!(
        "{}: the program needed more than {}\n  hint: raise the limit with --max-memory (or Sandbox::max_memory), or process data in smaller pieces",
        MEMORY_LIMIT_EXCEEDED,
        format_bytes(limit)
    )
}

/// One open handle, given back to its budget when dropped. A slot taken
/// with no budget active (unlimited run) is a no-op.
#[must_use = "the handle is only counted while the slot lives"]
#[derive(Debug, Default)]
pub struct Slot(Option<(Arc<Budget>, Resource)>);

impl Slot {
    /// A slot that counts nothing.
    pub fn none() -> Slot {
        Slot(None)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        if let Some((budget, r)) = self.0.take() {
            budget.open[r.index()].fetch_sub(1, Ordering::AcqRel);
        }
    }
}

/// Take a handle of kind `r` from the active budget.
pub fn acquire(r: Resource) -> Result<Slot, String> {
    match current() {
        Some(b) => b.acquire(r),
        None => Ok(Slot::none()),
    }
}

/// Count one module import against the active budget.
pub fn charge_import() -> Result<(), String> {
    match current() {
        Some(b) => b.charge_import(),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Active budget
// ---------------------------------------------------------------------------

static GLOBAL: RwLock<Option<Arc<Budget>>> = RwLock::new(None);

thread_local! {
    static CURRENT: RefCell<Option<Arc<Budget>>> = const { RefCell::new(None) };
    /// Budget the allocator charges on this thread (null = none). Points
    /// into an `Arc<Budget>` kept alive by the [`LimitsGuard`] that set it.
    static METER: Cell<*const Budget> = const { Cell::new(std::ptr::null()) };
}

/// The budget in force on this thread, if any.
pub fn current() -> Option<Arc<Budget>> {
    CURRENT
        .with(|c| c.borrow().clone())
        .or_else(|| GLOBAL.read().unwrap_or_else(|e| e.into_inner()).clone())
}

/// Replace the process-wide budget (used by threads with no [`scope`]).
/// Unlimited configurations are stored as `None`.
pub fn set_global(budget: Option<Arc<Budget>>) {
    let budget = budget.filter(|b| !b.limits.is_unlimited());
    *GLOBAL.write().unwrap_or_else(|e| e.into_inner()) = budget;
}

/// Restores the previous thread budget when dropped.
#[must_use = "the budget is only in force while the guard lives"]
pub struct LimitsGuard {
    previous: Option<Arc<Budget>>,
    previous_meter: *const Budget,
}

impl Drop for LimitsGuard {
    fn drop(&mut self) {
        METER.with(|m| m.set(self.previous_meter));
        let prev = self.previous.take();
        CURRENT.with(|c| *c.borrow_mut() = prev);
    }
}

/// Run this thread under `budget` until the guard is dropped. `None` keeps
/// whatever is active. When the budget limits memory, this thread's
/// allocations are charged to it (see [`CountingAllocator`]).
pub fn scope(budget: Option<Arc<Budget>>) -> LimitsGuard {
    let previous_meter = METER.with(Cell::get);
    let previous = match budget {
        Some(b) => {
            let meter = if b.limits.max_memory.is_some() {
                Arc::as_ptr(&b)
            } else {
                std::ptr::null()
            };
            METER.with(|m| m.set(meter));
            CURRENT.with(|c| c.borrow_mut().replace(b))
        }
        None => CURRENT.with(|c| c.borrow().clone()),
    };
    LimitsGuard {
        previous,
        previous_meter,
    }
}

// ---------------------------------------------------------------------------
// Engine safe points
// ---------------------------------------------------------------------------

/// Per-engine safe-point state: settles fuel with the shared [`Budget`] and
/// polls memory and fatal trips. Engines keep their own countdown and call
/// [`Meter::safepoint`] when it reaches zero.
#[derive(Debug, Clone, Default)]
pub struct Meter {
    budget: Option<Arc<Budget>>,
    fuel: bool,
    poll_memory: bool,
    /// Poll memory at every step (the interpreter, whose steps are
    /// statements) rather than at the engine's ordinary safe points.
    memory_every_step: bool,
}

impl Meter {
    /// A meter for the budget active on this thread.
    pub fn current() -> Meter {
        Meter::for_budget(current())
    }

    pub fn for_budget(budget: Option<Arc<Budget>>) -> Meter {
        let budget = budget.filter(|b| !b.limits.is_unlimited());
        let fuel = budget.as_ref().is_some_and(|b| b.limits.max_fuel.is_some());
        Meter {
            budget,
            fuel,
            poll_memory: false,
            memory_every_step: false,
        }
    }

    /// Also poll the allocation meter at every safe point (the
    /// interpreter's memory accounting).
    pub fn with_memory_polling(mut self) -> Meter {
        self.poll_memory = self
            .budget
            .as_ref()
            .is_some_and(|b| b.limits.max_memory.is_some());
        self.memory_every_step = self.poll_memory;
        self
    }

    /// Also poll the allocation meter, at the engine's ordinary safe points.
    /// The sandbox turns this on for the VM, so a run's memory limit covers
    /// every thread of the run (spawned tasks each have their own GC heap,
    /// which only bounds itself), as it does on the interpreter.
    pub fn with_safepoint_memory_polling(mut self) -> Meter {
        if !self.poll_memory {
            self.poll_memory = self
                .budget
                .as_ref()
                .is_some_and(|b| b.limits.max_memory.is_some());
        }
        self
    }

    /// Whether this meter polls the allocation meter.
    pub fn polls_memory(&self) -> bool {
        self.poll_memory
    }

    pub fn budget(&self) -> Option<&Arc<Budget>> {
        self.budget.as_ref()
    }

    /// True when fuel is limited (the VM then stays out of JIT code).
    #[inline]
    pub fn fuel_limited(&self) -> bool {
        self.fuel
    }

    /// Settle `executed` steps and decide how many more may run before the
    /// next call: the engine sets its countdown to the returned value, so
    /// the step that triggers the next call plus the countdown's steps add
    /// up to the returned value + 1. `interval` is the engine's normal
    /// distance between safe points. An `Err` carries the fatal message.
    #[inline]
    pub fn safepoint(&self, executed: u64, interval: u32) -> Result<u32, String> {
        match &self.budget {
            None => Ok(interval),
            Some(b) => self.settle(b, executed, interval),
        }
    }

    #[cold]
    #[inline(never)]
    fn settle(&self, b: &Budget, executed: u64, interval: u32) -> Result<u32, String> {
        if let Some(msg) = b.trip_message() {
            return Err(msg);
        }
        let mut next = interval as u64;
        if self.fuel {
            let left = b.charge_fuel(executed);
            if left == 0 {
                return Err(b.trip(Trip::Fuel));
            }
            next = next.min(left - 1);
        }
        if self.poll_memory {
            if b.over_memory() {
                return Err(b.trip(Trip::Memory));
            }
            if self.memory_every_step {
                next = 0;
            }
        }
        Ok(next as u32)
    }
}

// ---------------------------------------------------------------------------
// Allocation meter
// ---------------------------------------------------------------------------

/// A [`System`]-backed global allocator that charges each thread's
/// allocations to the budget installed on it with [`scope`], when that
/// budget limits memory. With no such budget the overhead is one
/// thread-local load and a branch per allocation.
///
/// The `forge` binary installs it. Hosts that want
/// [`crate::sandbox::Sandbox::max_memory`] must install it too:
///
/// ```ignore
/// #[global_allocator]
/// static ALLOC: forge_lang::CountingAllocator = forge_lang::CountingAllocator;
/// ```
pub struct CountingAllocator;

#[inline]
fn meter(delta: isize) {
    let budget = METER.try_with(Cell::get).unwrap_or(std::ptr::null());
    if !budget.is_null() {
        // SAFETY: METER only holds pointers set by `scope`, whose guard
        // keeps the `Arc<Budget>` alive and resets METER before releasing it.
        unsafe { (*budget).mem_used.fetch_add(delta, Ordering::Relaxed) };
    }
}

// SAFETY: every method forwards to `System` unchanged; metering only reads
// a thread-local and updates an atomic counter, and never allocates.
unsafe impl GlobalAlloc for CountingAllocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            meter(layout.size() as isize);
        }
        p
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(layout);
        if !p.is_null() {
            meter(layout.size() as isize);
        }
        p
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        meter(-(layout.size() as isize));
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = System.realloc(ptr, layout, new_size);
        if !p.is_null() {
            meter(new_size as isize - layout.size() as isize);
        }
        p
    }
}

/// Whether [`CountingAllocator`] is this process's global allocator (the
/// interpreter's memory limit depends on it).
pub fn allocation_meter_installed() -> bool {
    let probe = Budget::new(Limits {
        max_memory: Some(usize::MAX),
        ..Limits::none()
    });
    {
        let _scope = scope(Some(probe.clone()));
        let v: Vec<u8> = Vec::with_capacity(4096);
        std::hint::black_box(&v);
        let seen = probe.mem_used.load(Ordering::Relaxed) >= 4096;
        drop(v);
        if seen {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse a byte size: `1048576`, `512K`/`512KB`/`512KiB`, `256M`/`256MB`/
/// `256MiB`, `2G`/`2GB`/`2GiB` (binary multiples, case-insensitive).
pub fn parse_bytes(raw: &str) -> Result<usize, String> {
    let s = raw.trim();
    let split = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let mult: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "k" | "kb" | "kib" => 1024.0,
        "m" | "mb" | "mib" => 1024.0 * 1024.0,
        "g" | "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        _ => return Err(format!("invalid size '{}': use bytes or K/M/G", raw)),
    };
    let n: f64 = num
        .parse()
        .map_err(|_| format!("invalid size '{}': use bytes or K/M/G", raw))?;
    let bytes = n * mult;
    if !bytes.is_finite() || bytes < 1.0 || bytes > usize::MAX as f64 {
        return Err(format!("invalid size '{}': must be at least 1 byte", raw));
    }
    Ok(bytes as usize)
}

/// `268435456` → `256 MiB`.
pub fn format_bytes(n: usize) -> String {
    const UNITS: [(&str, usize); 3] = [("GiB", 1 << 30), ("MiB", 1 << 20), ("KiB", 1 << 10)];
    for (name, size) in UNITS {
        if n >= size && n % size == 0 {
            return format!("{} {}", n / size, name);
        }
    }
    for (name, size) in UNITS {
        if n >= size {
            return format!("{:.1} {}", n as f64 / size as f64, name);
        }
    }
    format!("{} bytes", n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget(l: Limits) -> Arc<Budget> {
        Budget::new(l)
    }

    #[test]
    fn parse_and_format_bytes() {
        assert_eq!(parse_bytes("1024"), Ok(1024));
        assert_eq!(parse_bytes("256MB"), Ok(256 << 20));
        assert_eq!(parse_bytes("256 mib"), Ok(256 << 20));
        assert_eq!(parse_bytes("1.5k"), Ok(1536));
        assert_eq!(parse_bytes("2G"), Ok(2 << 30));
        assert!(parse_bytes("lots").is_err());
        assert!(parse_bytes("0").is_err());
        assert!(parse_bytes("-5M").is_err());
        assert_eq!(format_bytes(256 << 20), "256 MiB");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(10), "10 bytes");
    }

    #[test]
    fn meter_settles_fuel_exactly() {
        // 10 steps of fuel with an interval of 4: the engine runs
        // (returned + 1) steps per window.
        let m = Meter::for_budget(Some(budget(Limits {
            max_fuel: Some(10),
            ..Limits::none()
        })));
        let mut executed = 0u64;
        let mut window = 0u64;
        let mut ran = 0u64;
        loop {
            match m.safepoint(executed, 4) {
                Ok(n) => {
                    window = n as u64 + 1;
                    ran += window;
                    executed = window;
                }
                Err(e) => {
                    assert!(e.starts_with(FUEL_EXHAUSTED), "{e}");
                    break;
                }
            }
        }
        assert_eq!(ran, 10);
        assert_eq!(window, 5);
        // Sticky: the next safe point fails too, even with nothing executed.
        assert!(m.safepoint(0, 4).is_err());
    }

    #[test]
    fn unlimited_meter_is_inert() {
        let m = Meter::for_budget(Some(budget(Limits::none())));
        assert!(m.budget().is_none());
        assert_eq!(m.safepoint(1_000_000, 1024), Ok(1024));
        assert!(!m.fuel_limited());
    }

    #[test]
    fn handle_slots_are_counted_and_released() {
        let b = budget(Limits {
            max_processes: Some(1),
            ..Limits::none()
        });
        let first = b.acquire(Resource::Processes).expect("first");
        let err = b.acquire(Resource::Processes).expect_err("second");
        assert!(err.starts_with(RESOURCE_LIMIT_EXCEEDED), "{err}");
        assert!(err.contains("subprocesses (limit 1)"), "{err}");
        drop(first);
        assert_eq!(b.open_count(Resource::Processes), 0);
        let _again = b.acquire(Resource::Processes).expect("after release");
        // Unlimited kinds are still counted.
        let _t = b.acquire(Resource::Tasks).expect("tasks unlimited");
        assert_eq!(b.open_count(Resource::Tasks), 1);
    }

    #[test]
    fn imports_and_caps() {
        let b = budget(Limits {
            max_imports: Some(2),
            max_memory: Some(1600),
            ..Limits::none()
        });
        assert!(b.charge_import().is_ok());
        assert!(b.charge_import().is_ok());
        assert!(b.charge_import().is_err());
        let caps = b.limits().caps();
        assert_eq!(caps.string, 1600);
        assert_eq!(caps.collection, 100);
        assert!(caps.check_string(1600).is_ok());
        assert!(caps.check_string(1601).is_err());
        assert!(caps.check_collection(101).is_err());
        assert!(Caps::UNLIMITED.check_string(usize::MAX).is_ok());
    }

    #[test]
    fn scope_installs_and_restores() {
        let b = budget(Limits {
            max_imports: Some(1),
            ..Limits::none()
        });
        assert!(CURRENT.with(|c| c.borrow().is_none()));
        {
            let _g = scope(Some(b.clone()));
            assert!(Arc::ptr_eq(&current().expect("scoped"), &b));
            // Spawned workers inherit it.
            let inherited = crate::permissions::spawn(|| current().is_some())
                .join()
                .expect("join");
            assert!(inherited);
        }
        assert!(CURRENT.with(|c| c.borrow().is_none()));
    }

    #[test]
    fn classify_messages() {
        assert_eq!(classify(&fuel_exhausted_message(5)), Some(LimitKind::Fuel));
        assert_eq!(
            classify(&format!("task failed: {}", memory_exceeded_message(5))),
            Some(LimitKind::Memory)
        );
        assert_eq!(
            classify("resource limit exceeded: too many tasks (limit 1)"),
            Some(LimitKind::Resource)
        );
        assert_eq!(classify("division by zero"), None);
    }

    #[test]
    fn trip_is_sticky_and_first_wins() {
        let b = budget(Limits {
            max_fuel: Some(1),
            max_memory: Some(1),
            ..Limits::none()
        });
        assert_eq!(b.tripped(), None);
        let msg = b.trip(Trip::Memory);
        assert!(msg.starts_with(MEMORY_LIMIT_EXCEEDED));
        let again = b.trip(Trip::Fuel);
        assert!(again.starts_with(MEMORY_LIMIT_EXCEEDED));
        assert_eq!(b.tripped(), Some(Trip::Memory));
    }
}
