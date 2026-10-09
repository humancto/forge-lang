//! The VM's global variables: indexed slots behind a name map.
//!
//! # Representation
//!
//! Every global *name* gets a [`GlobalId`] from an interner *domain*
//! ([`GlobalNames`]): an append-only name table shared, through an `Arc`,
//! by the VMs that share ids and globals with each other — a VM and its
//! spawned tasks (`fork_for_spawn`), a server template and its request /
//! WebSocket / MCP tool forks (`VmTemplate`); imports and REPL steps run
//! in the same VM. Every other VM (`VM::new`) starts a fresh domain, so a
//! sandbox run (`Sandbox::run_contained`: `forge mcp`'s `run_forge`, the
//! Python `Sandbox`, embedders) interns into a domain of its own that is
//! freed with its VM. Names are owned (`Arc<str>`), never leaked.
//!
//! The id of a name never changes *within its domain*, so it can be cached
//! in a [`Chunk`]: the first time a `GetGlobal`/`SetGlobal` with name
//! constant `k` runs, the chunk's [`GlobalIdCache`] entry `k` is filled
//! with the id *tagged with the domain*, and from then on the hot path is
//! an atomic load, a tag compare and a bounds-checked index into
//! [`Globals::slots`] — no hashing, no string comparison, no lock. A chunk
//! run by a VM of another domain (the same compiled chunk executed by two
//! independent VMs) sees a tag mismatch, resolves the name in its own
//! domain and re-tags the entry: a cached id is never used in a domain
//! that did not issue it.
//!
//! A VM's [`Globals`] holds its *values*: `slots[id]` is `Some(value)` when
//! the VM defines that global, `None` otherwise (a name another VM of the
//! domain defined, or one that was only read). `index` maps the names this
//! VM defines to their ids, for name-based access from Rust (builtins,
//! imports, JIT guards, server templates) without touching the domain's
//! lock; it is copy-on-write (`Arc`), so a fork shares its template's map
//! until it defines a new global.
//!
//! # Invariants
//!
//! * `index` and the `Some` slots describe the same set: `index[n] == id`
//!   iff `slots[id].is_some()` and `names.name_of(id) == n`. Globals are
//!   never removed (the VM has no operation that undefines one).
//! * Ids are only meaningful within their domain. Copying globals by id
//!   (`Clone`, [`Globals::try_map_values`]) keeps the domain; copying into
//!   a VM of another domain must go by name.
//! * A domain only grows, and lives as long as a VM or template using it.
//!   Its size is bounded by the distinct global names compiled or
//!   registered by the programs run in it — for a sandbox run, by that
//!   run's own source, and its allocations are charged to that run's
//!   memory budget (they happen on the run's worker thread).
//! * `slots` are GC roots: [`Globals::values`] yields every defined value.
//!
//! [`Chunk`]: super::bytecode::Chunk

use super::value::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

/// Identity of a global name within one [`GlobalNames`] domain (see the
/// module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlobalId(u32);

impl GlobalId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Bits of a [`GlobalIdCache`] entry that hold the id; the others hold the
/// domain tag. Ids at or above `1 << ID_BITS` work but are not cached.
const ID_BITS: u32 = 24;
const ID_MASK: u64 = (1 << ID_BITS) - 1;
/// Largest cacheable domain tag (`64 - ID_BITS` bits). Domains created
/// after the counter passes it get tag 0, which is never cached, so a tag
/// is never reused.
const MAX_TAG: u64 = (1 << (64 - ID_BITS)) - 1;

static NEXT_TAG: AtomicU64 = AtomicU64::new(1);
/// Live domains and the names they hold (test hook for leak checks).
static LIVE_DOMAINS: AtomicUsize = AtomicUsize::new(0);
static LIVE_NAMES: AtomicUsize = AtomicUsize::new(0);

#[derive(Default)]
struct Table {
    ids: HashMap<Arc<str>, GlobalId>,
    names: Vec<Arc<str>>,
}

/// An interner domain: the names whose [`GlobalId`]s a group of VMs shares
/// (see the module docs).
pub struct GlobalNames {
    /// Unique per domain; 0 means "never cache".
    tag: u64,
    table: RwLock<Table>,
}

impl std::fmt::Debug for GlobalNames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlobalNames")
            .field("tag", &self.tag)
            .field("len", &self.len())
            .finish()
    }
}

impl GlobalNames {
    /// A fresh, empty domain.
    pub fn new() -> Arc<Self> {
        let tag = NEXT_TAG.fetch_add(1, Ordering::Relaxed);
        LIVE_DOMAINS.fetch_add(1, Ordering::Relaxed);
        Arc::new(Self {
            tag: if tag <= MAX_TAG { tag } else { 0 },
            table: RwLock::new(Table::default()),
        })
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Table> {
        // The table is append-only and every write completes before the
        // guard drops, so a poisoned lock still holds a consistent table.
        self.table.read().unwrap_or_else(|e| e.into_inner())
    }

    /// The id of `name` in this domain, assigning one on first use.
    pub fn intern(&self, name: &str) -> GlobalId {
        self.intern_shared(name).0
    }

    /// [`GlobalNames::intern`], also returning the domain's copy of the
    /// name.
    fn intern_shared(&self, name: &str) -> (GlobalId, Arc<str>) {
        if let Some((key, id)) = self.read().ids.get_key_value(name) {
            return (*id, Arc::clone(key));
        }
        let mut table = self.table.write().unwrap_or_else(|e| e.into_inner());
        if let Some((key, id)) = table.ids.get_key_value(name) {
            return (*id, Arc::clone(key));
        }
        let id = GlobalId(
            u32::try_from(table.names.len())
                .expect("BUG: more than u32::MAX distinct global names in one domain"),
        );
        let owned: Arc<str> = Arc::from(name);
        table.names.push(Arc::clone(&owned));
        table.ids.insert(Arc::clone(&owned), id);
        LIVE_NAMES.fetch_add(1, Ordering::Relaxed);
        (id, owned)
    }

    /// The name of `id`, which must have been issued by this domain.
    pub fn name_of(&self, id: GlobalId) -> Arc<str> {
        self.read()
            .names
            .get(id.index())
            .cloned()
            .expect("BUG: GlobalId not issued by this domain")
    }

    /// Number of names interned in this domain.
    pub fn len(&self) -> usize {
        self.read().names.len()
    }

    #[allow(dead_code)] // pairs with `len`
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Process-wide `(live domains, names they hold)`: a test hook for
    /// checking that finished runs do not keep their names.
    #[allow(dead_code)] // library API; the CLI binary does not call it
    pub fn live_counts() -> (usize, usize) {
        (
            LIVE_DOMAINS.load(Ordering::Relaxed),
            LIVE_NAMES.load(Ordering::Relaxed),
        )
    }
}

impl Drop for GlobalNames {
    fn drop(&mut self) {
        let names = match self.table.get_mut() {
            Ok(table) => table.names.len(),
            Err(poisoned) => poisoned.into_inner().names.len(),
        };
        LIVE_NAMES.fetch_sub(names, Ordering::Relaxed);
        LIVE_DOMAINS.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Per-chunk cache: constant index → [`GlobalId`] of that name constant,
/// filled lazily the first time a global instruction uses it. Shared (an
/// `Arc`) by every clone of the chunk — closure instantiation clones the
/// prototype — so each name is resolved once per prototype, not per
/// closure. Each entry is `tag << ID_BITS | id` in one atomic word (0:
/// unresolved), so a reader never sees an id without the domain that
/// issued it. Holds no reference to any domain. Not serialized; a
/// deserialized chunk starts empty.
#[derive(Debug, Clone, Default)]
pub struct GlobalIdCache(Arc<OnceLock<Box<[AtomicU64]>>>);

impl GlobalIdCache {
    /// The id of constant `index` (out of `len` constants) in `names`,
    /// resolving it with `name` — called only on a miss: an empty entry,
    /// or one filled for another domain.
    #[inline]
    pub fn get_or_resolve(
        &self,
        names: &GlobalNames,
        index: usize,
        len: usize,
        name: impl FnOnce() -> Option<GlobalId>,
    ) -> Option<GlobalId> {
        let table = self
            .0
            .get_or_init(|| (0..len).map(|_| AtomicU64::new(0)).collect());
        let Some(cell) = table.get(index) else {
            // Not a constant of the chunk the table was sized for: resolve
            // without caching (the verifier keeps real code in range).
            return name();
        };
        let tag = names.tag;
        let cached = cell.load(Ordering::Relaxed);
        if tag != 0 && cached >> ID_BITS == tag {
            return Some(GlobalId((cached & ID_MASK) as u32));
        }
        let id = name()?;
        if tag != 0 && u64::from(id.0) <= ID_MASK {
            // Racing resolutions in one domain store the same word; across
            // domains the last store wins and the other domain re-resolves.
            cell.store(tag << ID_BITS | u64::from(id.0), Ordering::Relaxed);
        }
        Some(id)
    }
}

/// One VM's global variables (see the module docs).
#[derive(Clone)]
pub struct Globals {
    names: Arc<GlobalNames>,
    slots: Vec<Option<Value>>,
    index: Arc<HashMap<Arc<str>, GlobalId>>,
}

impl Default for Globals {
    fn default() -> Self {
        Self::new()
    }
}

impl Globals {
    /// No globals, in a fresh domain.
    pub fn new() -> Self {
        Self::in_domain(GlobalNames::new())
    }

    /// No globals, in domain `names`.
    pub fn in_domain(names: Arc<GlobalNames>) -> Self {
        Self {
            names,
            slots: Vec::new(),
            index: Arc::default(),
        }
    }

    /// The interner domain these globals' ids belong to.
    #[inline]
    pub fn names(&self) -> &Arc<GlobalNames> {
        &self.names
    }

    /// The value of global `id`, if this VM defines it. The hot path of
    /// `GetGlobal`.
    #[inline]
    pub fn get_id(&self, id: GlobalId) -> Option<Value> {
        match self.slots.get(id.index()) {
            Some(Some(v)) => Some(*v),
            _ => None,
        }
    }

    /// Define or reassign global `id`. The hot path of `SetGlobal`.
    #[inline]
    pub fn set_id(&mut self, id: GlobalId, value: Value) {
        if let Some(Some(slot)) = self.slots.get_mut(id.index()) {
            *slot = value;
            return;
        }
        let name = self.names.name_of(id);
        self.define(id, name, value);
    }

    fn define(&mut self, id: GlobalId, name: Arc<str>, value: Value) {
        let i = id.index();
        if self.slots.len() <= i {
            self.slots.resize(i + 1, None);
        }
        self.slots[i] = Some(value);
        Arc::make_mut(&mut self.index).insert(name, id);
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        let id = self.index.get(name)?;
        self.slots.get(id.index())?.as_ref()
    }

    #[allow(dead_code)] // library API; the CLI binary does not call it
    pub fn contains_key(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    /// Define or reassign `name`; returns the previous value.
    pub fn insert(&mut self, name: impl AsRef<str>, value: Value) -> Option<Value> {
        let name = name.as_ref();
        if let Some(id) = self.index.get(name).copied() {
            return self.slots[id.index()].replace(value);
        }
        let (id, name) = self.names.intern_shared(name);
        self.define(id, name, value);
        None
    }

    /// Names of the defined globals (arbitrary order).
    pub fn keys(&self) -> impl Iterator<Item = &str> + '_ {
        self.index.keys().map(|k| &**k)
    }

    /// Values of the defined globals (GC roots).
    pub fn values(&self) -> impl Iterator<Item = &Value> + '_ {
        self.slots.iter().flatten()
    }

    /// `(name, value)` of every defined global (arbitrary order).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> + '_ {
        self.index.iter().filter_map(|(name, id)| {
            self.slots
                .get(id.index())
                .and_then(Option::as_ref)
                .map(|v| (&**name, v))
        })
    }

    /// The same globals, in the same domain, with every value replaced by
    /// `f(value)` (server templates freeze and re-materialize globals this
    /// way: no name is re-hashed or re-interned).
    pub fn try_map_values<E>(
        &self,
        mut f: impl FnMut(&Value) -> Result<Value, E>,
    ) -> Result<Globals, E> {
        let slots = self
            .slots
            .iter()
            .map(|slot| slot.as_ref().map(&mut f).transpose())
            .collect::<Result<Vec<_>, E>>()?;
        Ok(Globals {
            names: Arc::clone(&self.names),
            slots,
            index: Arc::clone(&self.index),
        })
    }

    #[allow(dead_code)] // library API; the CLI binary does not call it
    pub fn len(&self) -> usize {
        self.index.len()
    }

    #[allow(dead_code)] // library API; the CLI binary does not call it
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Length of the slot table (the highest defined id + 1): test hook
    /// for checking that ids do not grow across unrelated runs.
    #[allow(dead_code)] // library API; the CLI binary does not call it
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }
}

impl<S: AsRef<str>> FromIterator<(S, Value)> for Globals {
    fn from_iter<I: IntoIterator<Item = (S, Value)>>(iter: I) -> Self {
        let mut globals = Globals::new();
        for (name, value) in iter {
            globals.insert(name, value);
        }
        globals
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_within_a_domain() {
        let names = GlobalNames::new();
        let a = names.intern("__globals_test_a");
        assert_eq!(names.intern("__globals_test_a"), a);
        assert_ne!(names.intern("__globals_test_b"), a);
        assert_eq!(&*names.name_of(a), "__globals_test_a");
        assert_eq!(names.len(), 2);
        // Another domain numbers names independently.
        let other = GlobalNames::new();
        assert_eq!(other.intern("__globals_test_b"), GlobalId(0));
        assert_eq!(other.intern("__globals_test_a"), GlobalId(1));
    }

    #[test]
    fn slots_and_names_agree() {
        let mut g = Globals::new();
        assert!(g.get("__globals_test_x").is_none());
        assert!(g
            .insert("__globals_test_x", Value::bool_val(true))
            .is_none());
        let id = g.names().intern("__globals_test_x");
        assert_eq!(g.get_id(id).and_then(|v| v.as_bool()), Some(true));
        g.set_id(id, Value::bool_val(false));
        assert_eq!(
            g.get("__globals_test_x").and_then(|v| v.as_bool()),
            Some(false)
        );
        // A name this VM never defined has an id but no slot.
        let other = g.names().intern("__globals_test_never_defined");
        assert!(g.get_id(other).is_none());
        assert!(!g.contains_key("__globals_test_never_defined"));
        // Defining through the id path registers the name.
        g.set_id(other, Value::null());
        assert!(g.contains_key("__globals_test_never_defined"));
        assert_eq!(g.len(), 2);
        assert_eq!(g.values().count(), 2);
        let mut names: Vec<_> = g.keys().collect();
        names.sort_unstable();
        assert_eq!(names, ["__globals_test_never_defined", "__globals_test_x"]);
    }

    #[test]
    fn copies_share_the_index_until_they_define() {
        let mut g = Globals::new();
        g.insert("__globals_test_shared", Value::null());
        let mut fork = g.try_map_values(|v| Ok::<_, ()>(*v)).expect("infallible");
        assert!(Arc::ptr_eq(g.names(), fork.names()));
        assert!(Arc::ptr_eq(&g.index, &fork.index));
        fork.insert("__globals_test_fork_only", Value::null());
        assert!(fork.contains_key("__globals_test_fork_only"));
        assert!(!g.contains_key("__globals_test_fork_only"));
        assert_eq!(g.len(), 1);
    }

    #[test]
    fn chunk_cache_resolves_once_per_domain() {
        let names = GlobalNames::new();
        let cache = GlobalIdCache::default();
        let shared = cache.clone();
        let id = names.intern("__globals_test_cached");
        let mut calls = 0;
        for _ in 0..3 {
            let got = shared.get_or_resolve(&names, 1, 2, || {
                calls += 1;
                Some(id)
            });
            assert_eq!(got, Some(id));
        }
        assert_eq!(calls, 1);
        // The clone shares the filled table.
        assert_eq!(cache.get_or_resolve(&names, 1, 2, || None), Some(id));
        // Out-of-range constant index: resolved, not cached.
        assert_eq!(cache.get_or_resolve(&names, 5, 2, || Some(id)), Some(id));
        assert_eq!(cache.get_or_resolve(&names, 5, 2, || None), None);
    }

    #[test]
    fn chunk_cache_never_returns_another_domains_id() {
        let a = GlobalNames::new();
        let b = GlobalNames::new();
        b.intern("__globals_test_pad");
        let in_a = a.intern("__globals_test_name");
        let in_b = b.intern("__globals_test_name");
        assert_ne!(in_a, in_b);
        let cache = GlobalIdCache::default();
        assert_eq!(cache.get_or_resolve(&a, 0, 1, || Some(in_a)), Some(in_a));
        // Filled for `a`: a lookup in `b` misses and resolves in `b`.
        assert_eq!(cache.get_or_resolve(&b, 0, 1, || Some(in_b)), Some(in_b));
        assert_eq!(cache.get_or_resolve(&b, 0, 1, || None), Some(in_b));
        // ... and `a` re-resolves rather than seeing `b`'s id.
        assert_eq!(cache.get_or_resolve(&a, 0, 1, || Some(in_a)), Some(in_a));
        assert_eq!(cache.get_or_resolve(&a, 0, 1, || None), Some(in_a));
    }
}
