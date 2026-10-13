//! Storage for `Set` and `Map` values on both engines: insertion-ordered,
//! with a hash index so membership, `add`, `remove`, `get` and `set` are
//! O(1) on average. A map is an `OrderedSet<(key, value)>` whose slots are
//! hashed and compared by their key alone (lookups go through
//! [`find`](OrderedSet::find), which returns the oldest match).
//!
//! The container does not know how to hash or compare its items: each
//! engine computes an item's hash with its own rule (interpreter
//! `Value::set_hash`, VM `Value::set_hash`) and passes an equality callback
//! (`container_eq` / `set_eq`) to lookups. The one requirement on the hash
//! is that **items the engine considers equal hash equally**; collisions
//! only cost a comparison. The helpers at the bottom (`hash_number`,
//! `hash_str`, `mix`) are the shared building blocks.
//!
//! Order: items iterate in insertion order. `remove` leaves a tombstone
//! (so it does not shift the items after it); tombstones are compacted
//! away once they outnumber the live items, keeping every operation
//! amortized O(1) and iteration O(len).
//!
//! Equality need not be transitive (an Int and a Float can both equal a
//! third value without equalling each other), so `remove_matching` removes
//! *every* stored item equal to the probe, exactly like the linear
//! `retain` it replaces.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Below this many slots lookups scan linearly (comparing stored hashes
/// first) and no index is allocated.
const SMALL: usize = 8;
const NONE: u32 = u32::MAX;

#[derive(Clone)]
struct Slot<T> {
    hash: u64,
    /// `None` once removed (a tombstone).
    item: Option<T>,
    /// Next slot with the same hash (index chain), or `NONE`.
    next: u32,
}

/// The hashes are already mixed; the index map uses them as they are.
#[derive(Default, Clone, Copy)]
pub struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 << 8) | u64::from(b);
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
}

type Index = HashMap<u64, u32, BuildHasherDefault<IdentityHasher>>;

#[derive(Clone)]
pub struct OrderedSet<T> {
    slots: Vec<Slot<T>>,
    live: usize,
    /// Hash → most recently inserted live slot with that hash. Built once
    /// the set outgrows `SMALL`.
    index: Option<Index>,
}

impl<T> Default for OrderedSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> OrderedSet<T> {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            live: 0,
            index: None,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            live: 0,
            index: None,
        }
    }

    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Items in insertion order.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &T> + Clone + '_ {
        self.slots.iter().filter_map(|s| s.item.as_ref())
    }

    /// Items with their stored hashes, in insertion order (so another set
    /// can take them without hashing again).
    pub fn iter_hashed(&self) -> impl Iterator<Item = (u64, &T)> + '_ {
        self.slots
            .iter()
            .filter_map(|s| s.item.as_ref().map(|item| (s.hash, item)))
    }

    pub fn into_vec(self) -> Vec<T> {
        self.slots.into_iter().filter_map(|s| s.item).collect()
    }

    pub fn into_hashed(self) -> impl Iterator<Item = (u64, T)> {
        self.slots
            .into_iter()
            .filter_map(|s| s.item.map(|item| (s.hash, item)))
    }

    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.iter().cloned().collect()
    }

    /// The `i`-th item in iteration order. O(1) unless the set has
    /// tombstones (see [`make_dense`](Self::make_dense)); then O(n).
    pub fn get_index(&self, i: usize) -> Option<&T> {
        if self.live == self.slots.len() {
            self.slots.get(i).and_then(|s| s.item.as_ref())
        } else {
            self.iter().nth(i)
        }
    }

    /// Whether removed items still occupy slots (positional access is then
    /// O(n)).
    pub fn has_tombstones(&self) -> bool {
        self.live != self.slots.len()
    }

    /// Drop tombstones so [`get_index`](Self::get_index) is O(1). Does not
    /// change the items or their order.
    pub fn make_dense(&mut self) {
        if self.has_tombstones() {
            self.compact();
        }
    }

    /// A set with `f` applied to every item, keeping each item's stored
    /// hash and the order. Only for transformations that do not change
    /// how an item hashes or compares.
    pub fn map_same_hash<U>(&self, mut f: impl FnMut(&T) -> U) -> OrderedSet<U> {
        let mut out = OrderedSet::with_capacity(self.live);
        for (hash, item) in self.iter_hashed() {
            out.push_unchecked(hash, f(item));
        }
        out
    }

    /// [`map_same_hash`](Self::map_same_hash), consuming the items.
    pub fn into_map_same_hash<U>(self, mut f: impl FnMut(T) -> U) -> OrderedSet<U> {
        let mut out = OrderedSet::with_capacity(self.live);
        for (hash, item) in self.into_hashed() {
            out.push_unchecked(hash, f(item));
        }
        out
    }

    /// Approximate heap bytes held (for memory accounting).
    pub fn heap_bytes(&self) -> usize {
        let index = self.index.as_ref().map_or(0, |index| {
            index.capacity() * (std::mem::size_of::<(u64, u32)>() + 1)
        });
        self.slots.capacity() * std::mem::size_of::<Slot<T>>() + index
    }

    /// Slot of the first live item (in the index chain) with `hash` for
    /// which `eq` holds.
    fn find_slot(&self, hash: u64, mut eq: impl FnMut(&T) -> bool) -> Option<usize> {
        match &self.index {
            None => self
                .slots
                .iter()
                .position(|s| s.hash == hash && s.item.as_ref().is_some_and(&mut eq)),
            Some(index) => {
                let mut at = index.get(&hash).copied().unwrap_or(NONE);
                while at != NONE {
                    let slot = &self.slots[at as usize];
                    if slot.hash == hash && slot.item.as_ref().is_some_and(&mut eq) {
                        return Some(at as usize);
                    }
                    at = slot.next;
                }
                None
            }
        }
    }

    /// Slot of the first live item, in insertion order, with `hash` for
    /// which `eq` holds. Index chains run from the newest slot to the
    /// oldest, so the whole chain is walked: with a non-transitive equality
    /// several items may match, and a map's `get`/`set` must see the oldest
    /// one, exactly as a linear scan would.
    fn find_first_slot(&self, hash: u64, mut eq: impl FnMut(&T) -> bool) -> Option<usize> {
        let Some(index) = &self.index else {
            return self.find_slot(hash, eq);
        };
        let mut found = None;
        let mut at = index.get(&hash).copied().unwrap_or(NONE);
        while at != NONE {
            let slot = &self.slots[at as usize];
            if slot.hash == hash && slot.item.as_ref().is_some_and(&mut eq) {
                found = Some(at as usize);
            }
            at = slot.next;
        }
        found
    }

    /// The first live item (in insertion order) with `hash` for which `eq`
    /// holds.
    pub fn find(&self, hash: u64, eq: impl FnMut(&T) -> bool) -> Option<&T> {
        let at = self.find_first_slot(hash, eq)?;
        self.slots[at].item.as_ref()
    }

    /// [`find`](Self::find), mutably. The caller must not change how the
    /// item hashes or compares (a map updates only the value of a pair).
    pub fn find_mut(&mut self, hash: u64, eq: impl FnMut(&T) -> bool) -> Option<&mut T> {
        let at = self.find_first_slot(hash, eq)?;
        self.slots[at].item.as_mut()
    }

    /// Whether a live item with `hash` satisfies `eq`.
    pub fn contains(&self, hash: u64, eq: impl FnMut(&T) -> bool) -> bool {
        self.find_slot(hash, eq).is_some()
    }

    /// Append `item` without checking for an equal one: the caller has
    /// established that there is none (with [`contains`](Self::contains)).
    pub fn push_unchecked(&mut self, hash: u64, item: T) {
        let at = self.slots.len();
        let next = match &mut self.index {
            Some(index) => index.insert(hash, at as u32).unwrap_or(NONE),
            None => NONE,
        };
        self.slots.push(Slot {
            hash,
            item: Some(item),
            next,
        });
        self.live += 1;
        if self.index.is_none() && self.slots.len() > SMALL {
            self.rebuild_index();
        }
    }

    /// Remove every live item with `hash` for which `eq` holds. Returns how
    /// many were removed.
    pub fn remove_matching(&mut self, hash: u64, mut eq: impl FnMut(&T) -> bool) -> usize {
        let mut removed = 0;
        match self.index.take() {
            None => {
                for slot in &mut self.slots {
                    if slot.hash == hash && slot.item.as_ref().is_some_and(&mut eq) {
                        slot.item = None;
                        removed += 1;
                    }
                }
            }
            Some(mut index) => {
                let mut prev = NONE;
                let mut at = index.get(&hash).copied().unwrap_or(NONE);
                while at != NONE {
                    let slot = &mut self.slots[at as usize];
                    let next = slot.next;
                    if slot.hash == hash && slot.item.as_ref().is_some_and(&mut eq) {
                        slot.item = None;
                        slot.next = NONE;
                        removed += 1;
                        if prev == NONE {
                            if next == NONE {
                                index.remove(&hash);
                            } else {
                                index.insert(hash, next);
                            }
                        } else {
                            self.slots[prev as usize].next = next;
                        }
                    } else {
                        prev = at;
                    }
                    at = next;
                }
                self.index = Some(index);
            }
        }
        self.live -= removed;
        if removed > 0 {
            self.maybe_compact();
        }
        removed
    }

    /// Keep only the items for which `keep` holds (order preserved).
    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        if self.is_empty() {
            return;
        }
        let mut removed = 0;
        for slot in &mut self.slots {
            if slot.item.as_ref().is_some_and(|item| !keep(item)) {
                slot.item = None;
                removed += 1;
            }
        }
        if removed > 0 {
            self.live -= removed;
            self.compact();
        }
    }

    fn maybe_compact(&mut self) {
        let dead = self.slots.len() - self.live;
        if dead > SMALL && dead > self.live {
            self.compact();
        }
    }

    /// Drop tombstones and rebuild the index chains.
    fn compact(&mut self) {
        self.slots.retain(|s| s.item.is_some());
        self.rebuild_index();
    }

    fn rebuild_index(&mut self) {
        if self.slots.len() <= SMALL && self.live == self.slots.len() {
            for slot in &mut self.slots {
                slot.next = NONE;
            }
            self.index = None;
            return;
        }
        let mut index = Index::with_capacity_and_hasher(self.slots.len(), Default::default());
        for (at, slot) in self.slots.iter_mut().enumerate() {
            if slot.item.is_some() {
                slot.next = index.insert(slot.hash, at as u32).unwrap_or(NONE);
            } else {
                slot.next = NONE;
            }
        }
        self.index = Some(index);
    }
}

/// Items of an [`OrderedSet`] in insertion order (`for x in &set`).
pub struct Iter<'a, T>(std::slice::Iter<'a, Slot<T>>);

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        self.0.by_ref().find_map(|s| s.item.as_ref())
    }
}

impl<'a, T> IntoIterator for &'a OrderedSet<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Iter<'a, T> {
        Iter(self.slots.iter())
    }
}

/// Owned items of an [`OrderedSet`] in insertion order.
pub struct IntoIter<T>(std::vec::IntoIter<Slot<T>>);

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.0.by_ref().find_map(|s| s.item)
    }
}

impl<T> IntoIterator for OrderedSet<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> IntoIter<T> {
        IntoIter(self.slots.into_iter())
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for OrderedSet<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

// ---------------------------------------------------------------------------
// Hash building blocks shared by the engines' `set_hash` rules.
// ---------------------------------------------------------------------------

/// Final avalanche (splitmix64), so the identity-hashed index sees well
/// mixed bits.
pub fn finish(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Order-dependent combination of `h` with the next component `x`.
pub fn mix(h: u64, x: u64) -> u64 {
    (h.rotate_left(5) ^ x).wrapping_mul(0x517c_c1b7_2722_0a95)
}

/// Hash of a number for set membership: Ints and Floats that compare equal
/// (`1` and `1.0`, `0.0` and `-0.0`) hash equally, and every NaN hashes
/// alike (sets treat NaN as equal to itself).
pub fn hash_number(x: f64) -> u64 {
    if x.is_nan() {
        return mix(TAG_NUMBER, 0x7ff8_0000_0000_0000);
    }
    let x = if x == 0.0 { 0.0 } else { x };
    mix(TAG_NUMBER, x.to_bits())
}

pub fn hash_int(n: i64) -> u64 {
    hash_number(n as f64)
}

pub fn hash_str(s: &str) -> u64 {
    let bytes = s.as_bytes();
    let mut h = mix(TAG_STRING, bytes.len() as u64);
    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        let mut word = [0u8; 8];
        word.copy_from_slice(chunk);
        h = mix(h, u64::from_le_bytes(word));
    }
    let rest = chunks.remainder();
    if !rest.is_empty() {
        let mut word = [0u8; 8];
        word[..rest.len()].copy_from_slice(rest);
        h = mix(h, u64::from_le_bytes(word));
    }
    h
}

/// Tags keep values of kinds that never compare equal apart.
pub const TAG_NUMBER: u64 = 1;
pub const TAG_STRING: u64 = 2;
pub const TAG_BOOL: u64 = 3;
pub const TAG_NULL: u64 = 4;
pub const TAG_ARRAY: u64 = 5;
pub const TAG_TUPLE: u64 = 6;
pub const TAG_SET: u64 = 7;
pub const TAG_MAP: u64 = 8;
pub const TAG_OBJECT: u64 = 9;
pub const TAG_OK: u64 = 10;
pub const TAG_ERR: u64 = 11;
pub const TAG_SOME: u64 = 12;
pub const TAG_NONE: u64 = 13;
pub const TAG_OTHER: u64 = 14;

/// Containers nested deeper than this hash as their tag alone (still
/// consistent with equality, just less selective).
pub const MAX_HASH_DEPTH: usize = 16;

#[cfg(test)]
mod tests {
    use super::*;

    /// Items are (key, tag); equal when keys are equal. `hash` collides on
    /// purpose for keys with the same value mod 4.
    fn hash(key: i64) -> u64 {
        finish(key.rem_euclid(4) as u64)
    }

    fn insert(set: &mut OrderedSet<(i64, char)>, key: i64, tag: char) -> bool {
        if set.contains(hash(key), |&(k, _)| k == key) {
            return false;
        }
        set.push_unchecked(hash(key), (key, tag));
        true
    }

    fn remove(set: &mut OrderedSet<(i64, char)>, key: i64) -> usize {
        set.remove_matching(hash(key), |&(k, _)| k == key)
    }

    fn has(set: &OrderedSet<(i64, char)>, key: i64) -> bool {
        set.contains(hash(key), |&(k, _)| k == key)
    }

    fn keys(set: &OrderedSet<(i64, char)>) -> Vec<i64> {
        set.iter().map(|&(k, _)| k).collect()
    }

    #[test]
    fn keeps_insertion_order_and_first_of_duplicates() {
        for n in [3, 8, 9, 100] {
            let mut set = OrderedSet::new();
            for k in (0..n).rev() {
                assert!(insert(&mut set, k, 'a'));
            }
            for k in 0..n {
                assert!(!insert(&mut set, k, 'b'), "duplicate {} added", k);
                assert!(has(&set, k));
            }
            assert!(!has(&set, n));
            assert_eq!(set.len() as i64, n);
            assert_eq!(keys(&set), (0..n).rev().collect::<Vec<_>>());
            assert!(set.iter().all(|&(_, tag)| tag == 'a'));
        }
    }

    #[test]
    fn removal_keeps_order_and_compacts() {
        let mut set = OrderedSet::new();
        for k in 0..100 {
            insert(&mut set, k, 'a');
        }
        for k in (0..100).filter(|k| k % 3 != 0) {
            assert_eq!(remove(&mut set, k), 1);
            assert_eq!(remove(&mut set, k), 0);
        }
        let expected: Vec<i64> = (0..100).filter(|k| k % 3 == 0).collect();
        assert_eq!(keys(&set), expected);
        assert_eq!(set.len(), expected.len());
        // Compaction ran (tombstones outnumbered live items at some point).
        assert!(set.slots.len() < 100);
        for (i, k) in expected.iter().enumerate() {
            assert_eq!(set.get_index(i).map(|&(k, _)| k), Some(*k));
        }
        // Re-adding a removed key appends it.
        assert!(insert(&mut set, 1, 'c'));
        assert_eq!(keys(&set).last(), Some(&1));
        assert!(has(&set, 1) && !has(&set, 2));
    }

    #[test]
    fn positional_access_with_tombstones() {
        let mut set = OrderedSet::new();
        for k in 0..20 {
            insert(&mut set, k, 'a');
        }
        remove(&mut set, 5);
        assert!(set.has_tombstones());
        assert_eq!(set.get_index(5).map(|&(k, _)| k), Some(6));
        set.make_dense();
        assert!(!set.has_tombstones());
        assert_eq!(set.get_index(5).map(|&(k, _)| k), Some(6));
        assert_eq!(set.get_index(19), None);
        assert!(has(&set, 19) && !has(&set, 5));
    }

    #[test]
    fn remove_matching_removes_every_match() {
        // A non-transitive equality: the probe matches several items.
        for n in [4, 40] {
            let mut set: OrderedSet<i64> = OrderedSet::new();
            for k in 0..n {
                set.push_unchecked(finish(7), k);
            }
            assert_eq!(
                set.remove_matching(finish(7), |k| k % 2 == 0),
                (n / 2) as usize
            );
            assert_eq!(
                set.iter().copied().collect::<Vec<_>>(),
                (0..n).filter(|k| k % 2 == 1).collect::<Vec<_>>()
            );
            assert!(set.contains(finish(7), |&k| k == 1));
            assert!(!set.contains(finish(7), |&k| k == 2));
        }
    }

    #[test]
    fn find_returns_the_oldest_match() {
        // A non-transitive equality: the probe matches several items, and
        // `find` must see the first in insertion order (a map's `get` and
        // `set`), with or without the index and after removals.
        for n in [4, 40] {
            let mut set: OrderedSet<(i64, char)> = OrderedSet::new();
            for k in 0..n {
                set.push_unchecked(finish(7), (k, 'a'));
            }
            let odd = |&(k, _): &(i64, char)| k % 2 == 1;
            assert_eq!(set.find(finish(7), odd).map(|&(k, _)| k), Some(1));
            if let Some(entry) = set.find_mut(finish(7), odd) {
                entry.1 = 'z';
            }
            assert_eq!(set.iter().filter(|&&(_, t)| t == 'z').count(), 1);
            set.remove_matching(finish(7), |&(k, _)| k < 2);
            assert_eq!(set.find(finish(7), odd).map(|&(k, _)| k), Some(3));
            assert_eq!(set.find(finish(8), odd), None);
            // Iterating by reference and by value keeps the order.
            let by_ref: Vec<i64> = (&set).into_iter().map(|&(k, _)| k).collect();
            let by_value: Vec<i64> = set.into_iter().map(|(k, _)| k).collect();
            assert_eq!(by_ref, by_value);
            assert_eq!(by_ref, (2..n).collect::<Vec<_>>());
        }
    }

    #[test]
    fn retain_and_map_keep_order() {
        let mut set = OrderedSet::new();
        for k in 0..30 {
            insert(&mut set, k, 'a');
        }
        set.retain(|&(k, _)| k >= 10);
        assert_eq!(keys(&set), (10..30).collect::<Vec<_>>());
        let mapped = set.map_same_hash(|&(k, _)| (k, 'z'));
        assert_eq!(keys(&mapped), (10..30).collect::<Vec<_>>());
        assert!(has(&mapped, 15) && !has(&mapped, 5));
        assert_eq!(set.clone().into_vec().len(), 20);
    }

    #[test]
    fn numbers_that_compare_equal_hash_equally() {
        assert_eq!(hash_int(1), hash_number(1.0));
        assert_eq!(hash_number(0.0), hash_number(-0.0));
        assert_eq!(hash_number(f64::NAN), hash_number(-f64::NAN));
        assert_eq!(hash_int(i64::MAX), hash_number(i64::MAX as f64));
        assert_ne!(hash_int(1), hash_int(2));
        assert_ne!(hash_str("ab"), hash_str("ba"));
        assert_ne!(hash_str("a"), hash_str("a\0"));
    }
}
