//! Import-cycle detection shared by every execution engine.
//!
//! # Invariant
//!
//! Both the interpreter (`Stmt::Import`) and the VM (`__forge_import_module`)
//! execute an imported file by recursively running it. Before running a
//! module, an engine must call [`enter_import`] with the resolved file path
//! and keep the returned [`ImportGuard`] alive for the duration of the
//! module's execution. If the file is already being imported further up the
//! chain, `enter_import` fails with a single, readable error:
//!
//! ```text
//! circular import: a.fg -> b.fg -> a.fg
//! ```
//!
//! The chain is tracked per thread (imports run synchronously on the thread
//! that executes the `import` statement) and is unwound by the guard's `Drop`,
//! so errors and early returns cannot leave stale entries behind.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

thread_local! {
    static IMPORT_STACK: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

/// Keeps a module on the import chain; popped on drop.
#[must_use = "the module stays on the import chain only while the guard lives"]
pub struct ImportGuard {
    depth: usize,
}

impl Drop for ImportGuard {
    fn drop(&mut self) {
        IMPORT_STACK.with(|s| s.borrow_mut().truncate(self.depth));
    }
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn display_path(path: &Path) -> String {
    if let Ok(cwd) = std::env::current_dir() {
        let cwd = canonical(&cwd);
        if let Ok(rel) = path.strip_prefix(&cwd) {
            return rel.display().to_string();
        }
    }
    path.display().to_string()
}

/// Push `path` onto the current thread's import chain, or report a cycle.
pub fn enter_import(path: &Path) -> Result<ImportGuard, String> {
    // Every module load counts against the run's import limit
    // (`runtime::limits`).
    crate::runtime::limits::charge_import()?;
    let path = canonical(path);
    IMPORT_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        if let Some(start) = stack.iter().position(|p| *p == path) {
            let chain: Vec<String> = stack[start..]
                .iter()
                .chain(std::iter::once(&path))
                .map(|p| display_path(p))
                .collect();
            return Err(format!("circular import: {}", chain.join(" -> ")));
        }
        let depth = stack.len();
        stack.push(path);
        Ok(ImportGuard { depth })
    })
}

/// How often a thread waiting for another thread's module wakes up to
/// check whether it was cancelled (same slice as the engines' other waits).
const MODULE_WAIT_POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// The modules one program has run, by canonical path, shared by every
/// engine instance of the program (spawned tasks, imports, `timeout`
/// bodies, `schedule`/`watch` threads; an HTTP request fork starts its own).
///
/// It makes "a module's top level runs at most once per program" hold
/// across threads: the first thread to import a module claims it
/// ([`Claim::Run`]) and runs it; a thread importing the same module
/// meanwhile waits (cancellably) until the runner publishes the module
/// ([`RunClaim::finish`]) and then gets that instance ([`Claim::Loaded`]).
/// If the runner fails, its claim is dropped and a waiter claims the module
/// itself, like a later import after a failed one on a single thread.
///
/// Cycles: on the claiming thread the import chain ([`enter_import`])
/// reports them; across threads (task 1 runs `a.fg`, which imports `b.fg`,
/// which task 2 is running and which imports `a.fg`) a waiter follows the
/// "waits for a module run by" edges and reports the cycle instead of
/// waiting forever.
///
/// `T` is what an importer binds: the interpreter's shared module scopes,
/// or the VM's transferable snapshot of the module's globals.
pub struct ModuleRegistry<T> {
    state: std::sync::Mutex<RegistryState<T>>,
    changed: std::sync::Condvar,
}

struct RegistryState<T> {
    modules: std::collections::HashMap<PathBuf, ModuleSlot<T>>,
    /// Module each blocked thread waits for (cross-thread cycle detection).
    waiting: std::collections::HashMap<std::thread::ThreadId, PathBuf>,
}

enum ModuleSlot<T> {
    Running(std::thread::ThreadId),
    Loaded(T),
}

/// Result of [`ModuleRegistry::claim`].
pub enum Claim<'a, T> {
    /// The module already ran in this program.
    Loaded(T),
    /// The caller must run the module and then call [`RunClaim::finish`].
    Run(RunClaim<'a, T>),
}

/// The right (and duty) to run a module. Keeps the module on this thread's
/// import chain; dropped without [`RunClaim::finish`] (an error, a panic),
/// it releases the module so another import can run it.
#[must_use = "a claimed module must be run and finished"]
pub struct RunClaim<'a, T> {
    registry: &'a ModuleRegistry<T>,
    key: PathBuf,
    finished: bool,
    _chain: ImportGuard,
}

impl<T: Clone> RunClaim<'_, T> {
    /// Publish the module and wake every thread waiting for it.
    pub fn finish(mut self, value: T) -> T {
        self.finished = true;
        let mut state = self.registry.lock();
        state
            .modules
            .insert(self.key.clone(), ModuleSlot::Loaded(value.clone()));
        drop(state);
        self.registry.changed.notify_all();
        value
    }
}

impl<T> Drop for RunClaim<'_, T> {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let mut state = self.registry.lock();
        if matches!(state.modules.get(&self.key), Some(ModuleSlot::Running(_))) {
            state.modules.remove(&self.key);
        }
        drop(state);
        self.registry.changed.notify_all();
    }
}

impl<T> Default for ModuleRegistry<T> {
    fn default() -> Self {
        ModuleRegistry {
            state: std::sync::Mutex::new(RegistryState {
                modules: std::collections::HashMap::new(),
                waiting: std::collections::HashMap::new(),
            }),
            changed: std::sync::Condvar::new(),
        }
    }
}

impl<T> ModuleRegistry<T> {
    fn lock(&self) -> std::sync::MutexGuard<'_, RegistryState<T>> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl<T: Clone> ModuleRegistry<T> {
    /// The module at `key`, if it already ran.
    pub fn get(&self, key: &Path) -> Option<T> {
        match self.lock().modules.get(key) {
            Some(ModuleSlot::Loaded(v)) => Some(v.clone()),
            _ => None,
        }
    }

    /// Every loaded module, removed from the registry (teardown).
    pub fn take_loaded(&self) -> Vec<T> {
        let mut state = self.lock();
        let keys: Vec<PathBuf> = state
            .modules
            .iter()
            .filter(|(_, slot)| matches!(slot, ModuleSlot::Loaded(_)))
            .map(|(k, _)| k.clone())
            .collect();
        keys.into_iter()
            .filter_map(|k| match state.modules.remove(&k) {
                Some(ModuleSlot::Loaded(v)) => Some(v),
                _ => None,
            })
            .collect()
    }

    /// Get the module at `key` (its canonical path; `file_path` is the
    /// resolved file, for the import chain), or claim it to run it.
    ///
    /// While another thread runs it, block until it is published, calling
    /// `interrupted` every [`MODULE_WAIT_POLL`]: its error (cancellation, a
    /// deadline) ends the wait. A cycle (on this thread or across threads)
    /// and the run's import limit are reported through `error`.
    pub fn claim<E>(
        &self,
        key: &Path,
        file_path: &Path,
        mut interrupted: impl FnMut() -> Result<(), E>,
        error: impl Fn(String) -> E,
    ) -> Result<Claim<'_, T>, E> {
        let me = std::thread::current().id();
        let mut state = self.lock();
        loop {
            match state.modules.get(key) {
                Some(ModuleSlot::Loaded(v)) => return Ok(Claim::Loaded(v.clone())),
                Some(ModuleSlot::Running(owner)) if *owner == me => {
                    // This thread is running it further up its import
                    // chain: `enter_import` reports the cycle.
                    drop(state);
                    let message = match enter_import(file_path) {
                        Err(message) => message,
                        Ok(_guard) => format!(
                            "circular import: {} -> {}",
                            display_path(key),
                            display_path(key)
                        ),
                    };
                    return Err(error(message));
                }
                Some(ModuleSlot::Running(owner)) => {
                    if let Some(message) = cross_thread_cycle(&state, me, *owner, key) {
                        return Err(error(message));
                    }
                    if !crate::clock::HAS_THREADS {
                        return Err(error(crate::clock::WAITS_FOREVER.to_string()));
                    }
                    state.waiting.insert(me, key.to_path_buf());
                    let (guard, _) = self
                        .changed
                        .wait_timeout(state, MODULE_WAIT_POLL)
                        .unwrap_or_else(|e| e.into_inner());
                    state = guard;
                    state.waiting.remove(&me);
                    drop(state);
                    interrupted()?;
                    state = self.lock();
                }
                None => {
                    state
                        .modules
                        .insert(key.to_path_buf(), ModuleSlot::Running(me));
                    drop(state);
                    // Every module run counts against the import limit and
                    // stays on this thread's chain while it runs. On error
                    // the half-built claim releases the module.
                    let chain = match enter_import(file_path) {
                        Ok(chain) => chain,
                        Err(message) => {
                            let mut state = self.lock();
                            state.modules.remove(key);
                            drop(state);
                            self.changed.notify_all();
                            return Err(error(message));
                        }
                    };
                    return Ok(Claim::Run(RunClaim {
                        registry: self,
                        key: key.to_path_buf(),
                        finished: false,
                        _chain: chain,
                    }));
                }
            }
        }
    }
}

/// `me` is about to wait for `key`, which `owner` runs. Follow the threads
/// the owner (transitively) waits for; reaching `me` is an import cycle,
/// reported as `circular import: x.fg -> key.fg -> ... -> x.fg` (each
/// module imports, directly or not, the next).
fn cross_thread_cycle<T>(
    state: &RegistryState<T>,
    me: std::thread::ThreadId,
    owner: std::thread::ThreadId,
    key: &Path,
) -> Option<String> {
    let mut path: Vec<&Path> = vec![key];
    let mut current = owner;
    for _ in 0..=state.waiting.len() {
        let wanted = state.waiting.get(&current)?;
        path.push(wanted);
        match state.modules.get(wanted) {
            Some(ModuleSlot::Running(next)) if *next == me => {
                // `me` runs `wanted`: wanted -> key -> ... -> wanted.
                let chain: Vec<String> = std::iter::once(wanted.as_path())
                    .chain(path.iter().copied())
                    .map(display_path)
                    .collect();
                return Some(format!("circular import: {}", chain.join(" -> ")));
            }
            Some(ModuleSlot::Running(next)) => current = *next,
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_cycle_and_unwinds() {
        let dir = std::env::temp_dir().join(format!("forge_import_cycle_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let a = dir.join("a.fg");
        let b = dir.join("b.fg");
        std::fs::write(&a, "").expect("write a");
        std::fs::write(&b, "").expect("write b");
        {
            let _ga = enter_import(&a).expect("a");
            let _gb = enter_import(&b).expect("b");
            let err = enter_import(&a).err().expect("cycle must be detected");
            assert!(err.starts_with("circular import: "), "{}", err);
            assert!(err.contains("a.fg -> "), "{}", err);
            assert!(err.ends_with("a.fg"), "{}", err);
            assert!(err.contains("b.fg"), "{}", err);
        }
        // Guards dropped: the chain is empty again, so re-entry succeeds.
        let _again = enter_import(&a).expect("chain must unwind on drop");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn module_file(tag: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "forge_module_registry_{}_{}",
            tag,
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let file = dir.join("m.fg");
        std::fs::write(&file, "").expect("write");
        (canonical(&file), dir)
    }

    fn never_interrupted() -> Result<(), String> {
        Ok(())
    }

    #[test]
    fn concurrent_claims_run_a_module_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let (file, dir) = module_file("once");
        let registry: Arc<ModuleRegistry<usize>> = Arc::default();
        let runs = Arc::new(AtomicUsize::new(0));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let (registry, runs, file) = (registry.clone(), runs.clone(), file.clone());
                std::thread::spawn(move || {
                    match registry.claim(&file, &file, never_interrupted, |m| m) {
                        Ok(Claim::Loaded(v)) => v,
                        Ok(Claim::Run(claim)) => {
                            runs.fetch_add(1, Ordering::SeqCst);
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            claim.finish(7)
                        }
                        Err(e) => panic!("claim failed: {}", e),
                    }
                })
            })
            .collect();
        for t in threads {
            assert_eq!(t.join().expect("thread"), 7);
        }
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(registry.get(&file), Some(7));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_run_releases_the_module_and_a_wait_can_be_interrupted() {
        let (file, dir) = module_file("release");
        let registry: ModuleRegistry<usize> = ModuleRegistry::default();
        match registry.claim(&file, &file, never_interrupted, |m| m) {
            Ok(Claim::Run(claim)) => drop(claim), // the run failed
            _ => panic!("first claim must run the module"),
        }
        assert!(registry.get(&file).is_none());
        let claim = match registry.claim(&file, &file, never_interrupted, |m| m) {
            Ok(Claim::Run(claim)) => claim,
            _ => panic!("a released module is claimed again"),
        };
        // Another thread waiting for it stops when interrupted.
        std::thread::scope(|s| {
            let waiter = s.spawn(|| {
                let mut polls = 0;
                let outcome = registry.claim(
                    &file,
                    &file,
                    || {
                        polls += 1;
                        if polls == 2 {
                            Err("cancelled".to_string())
                        } else {
                            Ok(())
                        }
                    },
                    |m| m,
                );
                matches!(outcome, Err(ref m) if m == "cancelled")
            });
            assert!(waiter.join().expect("waiter"));
        });
        assert_eq!(claim.finish(3), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
