//! The VM's global variables: indexed slots behind a name map.
//!
//! # Representation
//!
//! Every global *name* the process ever uses gets a [`GlobalId`] from one
//! process-wide, append-only interner. The id of a name never changes and is
//! the same in every VM of the process (template, request forks, spawned
//! tasks, imports, REPL steps), so it can be cached in a [`Chunk`]: the
//! first time a `GetGlobal`/`SetGlobal` with name constant `k` runs, the
//! chunk's [`GlobalIdCache`] entry `k` is filled, and from then on the hot
//! path is an atomic load plus a bounds-checked index into
//! [`Globals::slots`] — no hashing, no string comparison.
//!
//! A VM's [`Globals`] holds its *values*: `slots[id]` is `Some(value)` when
//! the VM defines that global, `None` otherwise (a name another VM defined,
//! or one that was only read). `index` maps the names this VM defines to
//! their ids, for name-based access from Rust (builtins, imports, JIT
//! guards, server templates) without touching the interner's lock.
//!
//! # Invariants
//!
//! * `index` and the `Some` slots describe the same set: `index[n] == id`
//!   iff `slots[id].is_some()` and `name_of(id) == n`. Globals are never
//!   removed (the VM has no operation that undefines one).
//! * Ids are process-wide, so a chunk's cache is valid in every VM, and
//!   copying globals between VMs (`fork_for_spawn`, `VmTemplate::fork`) can
//!   go by name or by id interchangeably.
//! * The interner only grows. Its size is bounded by the distinct global
//!   names compiled or registered in the process (program identifiers,
//!   builtin and module names, import prefixes), never by run-time data.
//! * `slots` are GC roots: [`Globals::values`] yields every defined value.
//!
//! [`Chunk`]: super::bytecode::Chunk

use super::value::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, OnceLock, RwLock};

/// Process-wide identity of a global name (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlobalId(u32);

impl GlobalId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Default)]
struct Interner {
    ids: HashMap<&'static str, GlobalId>,
    names: Vec<&'static str>,
}

static INTERNER: LazyLock<RwLock<Interner>> = LazyLock::new(|| RwLock::new(Interner::default()));

fn read_interner() -> std::sync::RwLockReadGuard<'static, Interner> {
    // The interner is append-only and every write completes before the
    // guard drops, so a poisoned lock still holds a consistent table.
    INTERNER.read().unwrap_or_else(|e| e.into_inner())
}

/// The id of `name`, assigning one on first use.
pub fn intern(name: &str) -> GlobalId {
    if let Some(id) = read_interner().ids.get(name) {
        return *id;
    }
    let mut table = INTERNER.write().unwrap_or_else(|e| e.into_inner());
    if let Some(id) = table.ids.get(name) {
        return *id;
    }
    let id = GlobalId(
        u32::try_from(table.names.len())
            .ok()
            .filter(|n| *n != UNRESOLVED)
            .expect("BUG: more than u32::MAX - 1 distinct global names"),
    );
    // Leaked on purpose: names are interned for the life of the process
    // (see the module docs) and `&'static str` keys let every VM's `index`
    // share them without reference counting.
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    table.names.push(leaked);
    table.ids.insert(leaked, id);
    id
}

/// The name of `id`.
pub fn name_of(id: GlobalId) -> &'static str {
    read_interner()
        .names
        .get(id.index())
        .copied()
        .expect("BUG: GlobalId not issued by the interner")
}

/// Sentinel for an unresolved [`GlobalIdCache`] entry.
const UNRESOLVED: u32 = u32::MAX;

/// Per-chunk cache: constant index → [`GlobalId`] of that name constant,
/// filled lazily the first time a global instruction uses it. Shared (an
/// `Arc`) by every clone of the chunk — closure instantiation clones the
/// prototype — so each name is resolved once per prototype, not per
/// closure. Not serialized; a deserialized chunk starts empty.
#[derive(Debug, Clone, Default)]
pub struct GlobalIdCache(Arc<OnceLock<Box<[AtomicU32]>>>);

impl GlobalIdCache {
    /// The cached id of constant `index`, resolving it with `name` (called
    /// only on a miss) out of `len` constants.
    #[inline]
    pub fn get_or_resolve(
        &self,
        index: usize,
        len: usize,
        name: impl FnOnce() -> Option<GlobalId>,
    ) -> Option<GlobalId> {
        let table = self
            .0
            .get_or_init(|| (0..len).map(|_| AtomicU32::new(UNRESOLVED)).collect());
        let Some(cell) = table.get(index) else {
            // Not a constant of the chunk the table was sized for: resolve
            // without caching (the verifier keeps real code in range).
            return name();
        };
        let cached = cell.load(Ordering::Relaxed);
        if cached != UNRESOLVED {
            return Some(GlobalId(cached));
        }
        let id = name()?;
        // Racing resolutions store the same id (the interner is
        // process-wide), so a relaxed store is enough.
        cell.store(id.0, Ordering::Relaxed);
        Some(id)
    }
}

/// One VM's global variables (see the module docs).
#[derive(Clone, Default)]
pub struct Globals {
    slots: Vec<Option<Value>>,
    index: HashMap<&'static str, GlobalId>,
}

impl Globals {
    pub fn new() -> Self {
        Self::default()
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
        self.define(id, name_of(id), value);
    }

    fn define(&mut self, id: GlobalId, name: &'static str, value: Value) {
        let i = id.index();
        if self.slots.len() <= i {
            self.slots.resize(i + 1, None);
        }
        self.slots[i] = Some(value);
        self.index.insert(name, id);
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
        let id = intern(name);
        self.define(id, name_of(id), value);
        None
    }

    /// Names of the defined globals (arbitrary order).
    pub fn keys(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.index.keys().copied()
    }

    /// Values of the defined globals (GC roots).
    pub fn values(&self) -> impl Iterator<Item = &Value> + '_ {
        self.slots.iter().flatten()
    }

    /// `(name, value)` of every defined global (arbitrary order).
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &Value)> + '_ {
        self.index.iter().filter_map(|(name, id)| {
            self.slots
                .get(id.index())
                .and_then(Option::as_ref)
                .map(|v| (*name, v))
        })
    }

    /// The same globals with every value replaced by `f(value)` (server
    /// templates freeze and re-materialize globals this way: no name is
    /// re-hashed or re-interned).
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
            slots,
            index: self.index.clone(),
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
    fn ids_are_process_wide_and_stable() {
        let a = intern("__globals_test_a");
        assert_eq!(intern("__globals_test_a"), a);
        assert_ne!(intern("__globals_test_b"), a);
        assert_eq!(name_of(a), "__globals_test_a");
    }

    #[test]
    fn slots_and_names_agree() {
        let mut g = Globals::new();
        assert!(g.get("__globals_test_x").is_none());
        assert!(g
            .insert("__globals_test_x", Value::bool_val(true))
            .is_none());
        let id = intern("__globals_test_x");
        assert_eq!(g.get_id(id).and_then(|v| v.as_bool()), Some(true));
        g.set_id(id, Value::bool_val(false));
        assert_eq!(
            g.get("__globals_test_x").and_then(|v| v.as_bool()),
            Some(false)
        );
        // A name this VM never defined has an id but no slot.
        let other = intern("__globals_test_never_defined");
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
    fn chunk_cache_resolves_once() {
        let cache = GlobalIdCache::default();
        let shared = cache.clone();
        let id = intern("__globals_test_cached");
        let mut calls = 0;
        for _ in 0..3 {
            let got = shared.get_or_resolve(1, 2, || {
                calls += 1;
                Some(id)
            });
            assert_eq!(got, Some(id));
        }
        assert_eq!(calls, 1);
        // The clone shares the filled table.
        assert_eq!(cache.get_or_resolve(1, 2, || None), Some(id));
        // Out-of-range constant index: resolved, not cached.
        assert_eq!(cache.get_or_resolve(5, 2, || Some(id)), Some(id));
        assert_eq!(cache.get_or_resolve(5, 2, || None), None);
    }
}
