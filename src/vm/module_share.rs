//! A module runs once per program on the VM too, across spawned tasks.
//!
//! Every VM of a program (the main one, `spawn` tasks, `schedule`/`watch`
//! threads: everything [`VM::fork_for_spawn`] makes) shares one
//! [`ModuleRegistry`] (`runtime::imports`). A VM imports a module it has not
//! run itself (`VM::loaded_modules`) through the registry:
//!
//! - nobody ran it yet: this VM claims it, runs the module's top level and
//!   publishes a [`ModuleSnapshot`]: transferable copies (`SharedValue`) of
//!   the globals, methods and struct metadata the run defined;
//! - another VM is running it: this one waits (cancellably, with
//!   cross-thread cycle detection) until it is published;
//! - it was published: this VM installs copies of the snapshot's values
//!   (and of the modules it imported, [`ModuleState::deps`]) instead of
//!   running it again.
//!
//! Value semantics are kept: like a spawned task's copies of its parent's
//! globals, an importer gets copies of the module's values as they were
//! when its top level finished, not a view of another task's later writes.
//!
//! The snapshot is only built when another VM of the program exists when
//! the module finishes (the registry has other holders). Otherwise no VM
//! can ever need it: every VM made later is forked from one that already
//! has the module in `loaded_modules`. A snapshot without state (which
//! that argument says is never needed) makes the importer run the module
//! itself, as before this registry existed.

use indexmap::IndexMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::machine::{LoadedModule, VMError, VM};
use super::value::{shared_to_value, value_to_shared, SharedValue, Value};

/// The program-wide module registry of the VM.
pub(crate) type ModuleRegistry = crate::runtime::imports::ModuleRegistry<Arc<ModuleSnapshot>>;

/// A module as published in the [`ModuleRegistry`].
pub(crate) struct ModuleSnapshot {
    pub(crate) module: Arc<LoadedModule>,
    /// `None` when no other VM existed to need it (see the module docs).
    state: Option<ModuleState>,
}

/// What a run of a module's top level defined, as transferable copies.
struct ModuleState {
    /// Globals the run defined: the module's own (prefixed) names, plus
    /// the unprefixed ones it registered (struct types, names it imported).
    globals: Vec<(String, SharedValue)>,
    /// Methods (`impl`), static methods and struct defaults the run added:
    /// (table, type name, member, value).
    members: Vec<(Table, String, String, SharedValue)>,
    embedded_fields: Vec<(String, Vec<(String, String)>)>,
    /// Modules the module imported (directly), by key: installed first, as
    /// the module's functions may read their globals.
    deps: Vec<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Table {
    Methods,
    Static,
    Defaults,
}

/// What existed before a module ran (to find what the run defined).
pub(super) struct Baseline {
    globals: HashSet<String>,
    members: HashSet<(Table, String, String)>,
    embedded: HashSet<String>,
}

// The registry is shared by the program's threads.
const _: () = {
    const fn shared_across_threads<T: Send + Sync>() {}
    shared_across_threads::<ModuleSnapshot>();
};

impl VM {
    fn table(&self, table: Table) -> &std::collections::HashMap<String, IndexMap<String, Value>> {
        match table {
            Table::Methods => &self.method_tables,
            Table::Static => &self.static_methods,
            Table::Defaults => &self.struct_defaults,
        }
    }

    fn table_mut(
        &mut self,
        table: Table,
    ) -> &mut std::collections::HashMap<String, IndexMap<String, Value>> {
        match table {
            Table::Methods => &mut self.method_tables,
            Table::Static => &mut self.static_methods,
            Table::Defaults => &mut self.struct_defaults,
        }
    }

    fn baseline(&self) -> Baseline {
        let mut members = HashSet::new();
        for table in [Table::Methods, Table::Static, Table::Defaults] {
            for (ty, entries) in self.table(table) {
                for name in entries.keys() {
                    members.insert((table, ty.clone(), name.clone()));
                }
            }
        }
        Baseline {
            globals: self.globals.keys().map(str::to_string).collect(),
            members,
            embedded: self.embedded_fields.keys().cloned().collect(),
        }
    }

    /// Copies of what running `module` defined since `before`.
    fn module_state(
        &self,
        module: &LoadedModule,
        before: &Baseline,
        deps: Vec<PathBuf>,
    ) -> ModuleState {
        // A value that cannot cross threads (a native function) is left
        // out, as `fork_for_spawn` does: the importer keeps its own.
        let transferable = |v: &Value| {
            let shared = value_to_shared(&self.gc, v);
            (!matches!(shared, SharedValue::Null) || v.is_null()).then_some(shared)
        };
        let mut globals = Vec::new();
        for (name, value) in self.globals.iter() {
            if name.starts_with(&module.prefix) || !before.globals.contains(name) {
                if let Some(shared) = transferable(value) {
                    globals.push((name.to_string(), shared));
                }
            }
        }
        let mut members = Vec::new();
        for table in [Table::Methods, Table::Static, Table::Defaults] {
            for (ty, entries) in self.table(table) {
                for (name, value) in entries {
                    if before.members.contains(&(table, ty.clone(), name.clone())) {
                        continue;
                    }
                    if let Some(shared) = transferable(value) {
                        members.push((table, ty.clone(), name.clone(), shared));
                    }
                }
            }
        }
        let embedded_fields = self
            .embedded_fields
            .iter()
            .filter(|(ty, _)| !before.embedded.contains(*ty))
            .map(|(ty, fields)| (ty.clone(), fields.clone()))
            .collect();
        // A captured stream is dropped like everywhere else values cross
        // threads; the flag must not leak into the next builtin call.
        self.drain_stream_boundary_flags();
        ModuleState {
            globals,
            members,
            embedded_fields,
            deps,
        }
    }

    /// Bind a module another VM of the program ran: copies of its values
    /// (and first of the modules it imported). `None` when the snapshot has
    /// no state (the caller then runs the module itself).
    fn install_module(
        &mut self,
        key: &Path,
        snapshot: &ModuleSnapshot,
        registry: &ModuleRegistry,
    ) -> Option<Arc<LoadedModule>> {
        let state = snapshot.state.as_ref()?;
        let module = Arc::clone(&snapshot.module);
        self.loaded_modules
            .insert(key.to_path_buf(), Arc::clone(&module));
        for dep in &state.deps {
            if self.loaded_modules.contains_key(dep) {
                continue;
            }
            if let Some(dep_snapshot) = registry.get(dep) {
                let _ = self.install_module(dep, &dep_snapshot, registry);
            }
        }
        for (name, shared) in &state.globals {
            // The module's own names are its; any other name it defined
            // (a struct type, a name it imported) does not replace one this
            // VM already has.
            if name.starts_with(&module.prefix) || !self.globals.contains_key(name) {
                let value = shared_to_value(&mut self.gc, shared);
                self.globals.insert(name, value);
            }
        }
        for (table, ty, name, shared) in &state.members {
            let exists = self
                .table(*table)
                .get(ty)
                .is_some_and(|entries| entries.contains_key(name));
            if !exists {
                let value = shared_to_value(&mut self.gc, shared);
                self.table_mut(*table)
                    .entry(ty.clone())
                    .or_default()
                    .insert(name.clone(), value);
            }
        }
        for (ty, fields) in &state.embedded_fields {
            self.embedded_fields
                .entry(ty.clone())
                .or_insert_with(|| fields.clone());
        }
        Some(module)
    }

    /// The module at `key` (`file_path` resolved, `path` as written), run
    /// once per program: by this VM, or by another one of the program whose
    /// copy is installed here. `run` runs the module's top level in this VM
    /// (the import chain entry is already held).
    pub(super) fn import_module_once(
        &mut self,
        key: &Path,
        file_path: &Path,
        run: impl FnOnce(&mut VM) -> Result<Arc<LoadedModule>, VMError>,
    ) -> Result<Arc<LoadedModule>, VMError> {
        let registry = Arc::clone(&self.module_registry);
        let claim = registry.claim(
            key,
            file_path,
            || self.wait_interrupted(),
            |message| VMError::new(&message),
        )?;
        let claim = match claim {
            crate::runtime::imports::Claim::Loaded(snapshot) => {
                if let Some(module) = self.install_module(key, &snapshot, &registry) {
                    return Ok(module);
                }
                // Not reachable (see the module docs); run it here, still
                // on the import chain.
                let _chain = crate::runtime::imports::enter_import(file_path)
                    .map_err(|m| VMError::new(&m))?;
                let module = run(self)?;
                self.loaded_modules
                    .insert(key.to_path_buf(), Arc::clone(&module));
                return Ok(module);
            }
            crate::runtime::imports::Claim::Run(claim) => claim,
        };

        let before = self.baseline();
        self.module_imports.push(Vec::new());
        let result = run(self);
        let deps = self.module_imports.pop().unwrap_or_default();
        let module = result?;
        self.loaded_modules
            .insert(key.to_path_buf(), Arc::clone(&module));
        // Held here (`registry`) and by this VM: more holders are other VMs
        // of the program (or this VM's own outer imports, harmlessly).
        let state =
            (Arc::strong_count(&registry) > 2).then(|| self.module_state(&module, &before, deps));
        claim.finish(Arc::new(ModuleSnapshot {
            module: Arc::clone(&module),
            state,
        }));
        Ok(module)
    }
}
