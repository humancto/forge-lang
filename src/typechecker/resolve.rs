//! Name resolution over the syntax index: which definition does each name
//! occurrence refer to?
//!
//! The rules mirror what both engines do at run time:
//!
//! * A reference sees definitions in its own scope and every enclosing
//!   scope. Within one function body, a `let` (or `fn`) is visible only
//!   after its statement has run, so `let x = x + 1` reads the outer `x`.
//! * A function or lambda body runs later, when called, so a reference
//!   inside one may see a definition that comes after the function in an
//!   enclosing scope (a function calling a function defined below it).
//! * `struct`, `type`, `interface` and variant names are visible in their
//!   whole scope.
//! * A `match` arm pattern that is a bare name is a binding — unless the
//!   name is a unit variant (`Red`) or `None`, which the engines compare
//!   against instead of binding.
//! * Names not bound anywhere may be builtins, stdlib modules or names
//!   brought in by a wildcard `import "file"`.

use super::builtins;
use crate::parser::index::{DefKind, OccId, Role, ScopeId, SyntaxIndex};
use std::collections::{HashMap, HashSet};

/// What a name occurrence resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// The occurrence is itself a definition.
    Def,
    /// A reference to a definition in the same file.
    Local(OccId),
    /// A name provided by the wildcard import with this index in
    /// [`ImportedNames`].
    Imported(usize),
    /// A builtin function, stdlib module, prelude value or builtin type.
    Global,
    /// A field, key or method name (resolved by the type checker, if at all).
    Member,
    /// Not bound anywhere.
    Unresolved,
}

/// Names visible through one wildcard `import "path"`.
#[derive(Debug, Clone, Default)]
pub struct ImportedNames {
    /// The import path as written.
    pub path: String,
    /// `None` when the file could not be found or parsed: every unresolved
    /// name might come from it, so unknown-name diagnostics are suppressed.
    pub names: Option<HashSet<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct Resolution {
    pub resolved: Vec<Resolved>,
    /// References that resolved only to a definition that runs later in the
    /// same function body (or at top level): `say x` before `let x = 1`.
    pub before_definition: HashSet<OccId>,
    /// Pattern bindings that are really references to unit variants.
    pub variant_patterns: HashSet<OccId>,
    /// Variant definitions that are really type names (`type T = Int | Str`).
    pub alias_members: HashSet<OccId>,
}

impl Resolution {
    /// The definition an occurrence refers to (itself for a definition).
    pub fn definition(&self, occ: OccId) -> Option<OccId> {
        match self.resolved.get(occ)? {
            Resolved::Def => Some(occ),
            Resolved::Local(def) => Some(*def),
            _ => None,
        }
    }

    /// Every occurrence (the definition included) that refers to `def`.
    pub fn occurrences_of(&self, def: OccId) -> Vec<OccId> {
        let mut out: Vec<OccId> = self
            .resolved
            .iter()
            .enumerate()
            .filter(|(id, r)| match r {
                Resolved::Def => *id == def,
                Resolved::Local(d) => *d == def,
                _ => false,
            })
            .map(|(id, _)| id)
            .collect();
        out.sort_unstable();
        out
    }
}

/// Builtin type names usable in annotations and as alias members.
fn is_builtin_type_name(name: &str) -> bool {
    builtins::builtin_type(name).is_some()
        || matches!(
            name,
            "Option" | "Result" | "Map" | "Set" | "Array" | "List" | "Tuple"
        )
}

pub fn resolve(index: &SyntaxIndex, imports: &[ImportedNames]) -> Resolution {
    let occs = &index.occurrences;

    // Type names declared anywhere in the file: a "variant" with such a
    // name is a member of a union alias, not a constructor.
    let declared_types: HashSet<&str> = occs
        .iter()
        .filter(|o| {
            matches!(
                o.def_kind(),
                Some(DefKind::Struct | DefKind::TypeDef | DefKind::Interface)
            )
        })
        .map(|o| o.name.as_str())
        .collect();

    let mut res = Resolution {
        resolved: vec![Resolved::Unresolved; occs.len()],
        ..Default::default()
    };

    for (id, occ) in occs.iter().enumerate() {
        if let Some(DefKind::Variant { .. }) = occ.def_kind() {
            if declared_types.contains(occ.name.as_str()) || is_builtin_type_name(&occ.name) {
                res.alias_members.insert(id);
            }
        }
    }

    // Unit-variant names (constructors without fields are still recorded
    // as variants; any variant name in a bare pattern is compared, not
    // bound, by the engines).
    let variant_names: HashSet<&str> = occs
        .iter()
        .enumerate()
        .filter(|(id, o)| {
            matches!(o.def_kind(), Some(DefKind::Variant { .. })) && !res.alias_members.contains(id)
        })
        .map(|(_, o)| o.name.as_str())
        .collect();

    // Definitions per scope, by name, in source order.
    let mut defs: HashMap<(ScopeId, &str), Vec<OccId>> = HashMap::new();
    for (id, occ) in occs.iter().enumerate() {
        let Some(kind) = occ.def_kind() else { continue };
        if res.alias_members.contains(&id) {
            continue;
        }
        if matches!(kind, DefKind::PatternBinding)
            && (variant_names.contains(occ.name.as_str())
                || occ.name == "None"
                || occ.name == "null")
        {
            res.variant_patterns.insert(id);
            continue;
        }
        defs.entry((occ.scope, occ.name.as_str()))
            .or_default()
            .push(id);
    }

    let wildcard: Vec<(usize, &ImportedNames)> = imports.iter().enumerate().collect();

    for (id, occ) in occs.iter().enumerate() {
        let is_variant_pattern = res.variant_patterns.contains(&id);
        res.resolved[id] = match &occ.role {
            Role::Def(_) if res.alias_members.contains(&id) => {
                resolve_type(index, &defs, id, &wildcard)
            }
            Role::Def(_) if !is_variant_pattern => Resolved::Def,
            Role::Field => Resolved::Member,
            Role::TypeRef => resolve_type(index, &defs, id, &wildcard),
            Role::Ref | Role::Def(_) => {
                let (resolved, early) = resolve_value(index, &defs, id, &wildcard);
                if early {
                    res.before_definition.insert(id);
                }
                resolved
            }
        };
    }
    res
}

fn kind_of(index: &SyntaxIndex, def: OccId) -> Option<&DefKind> {
    index.occurrences.get(def).and_then(|o| o.def_kind())
}

/// Resolve a value reference. Returns the resolution and whether it only
/// resolved to a definition that has not run yet.
fn resolve_value(
    index: &SyntaxIndex,
    defs: &HashMap<(ScopeId, &str), Vec<OccId>>,
    occ_id: OccId,
    imports: &[(usize, &ImportedNames)],
) -> (Resolved, bool) {
    let occ = &index.occurrences[occ_id];
    let name = occ.name.as_str();
    let pos = occ.span.start;

    // Pass 1: the nearest definition already in effect.
    let mut crossed_function = false;
    let mut fallback: Option<(OccId, bool)> = None;
    for scope in index.scope_chain(occ.scope) {
        if let Some(candidates) = defs.get(&(scope, name)) {
            let candidates: Vec<OccId> = candidates
                .iter()
                .copied()
                .filter(|d| kind_of(index, *d).is_some_and(|k| k.is_value()))
                .collect();
            let visible = candidates
                .iter()
                .copied()
                .rfind(|d| index.occurrences[*d].visible_from <= pos);
            if let Some(def) = visible {
                return (Resolved::Local(def), false);
            }
            if let Some(first) = candidates.first() {
                if crossed_function {
                    // The body runs later: a definition further down the
                    // enclosing scope will have run by then.
                    return (Resolved::Local(*first), false);
                }
                fallback.get_or_insert((*first, true));
            }
        }
        if index.scopes[scope].kind.is_function_boundary() {
            crossed_function = true;
        }
    }

    if builtins::is_global(name) {
        return (Resolved::Global, false);
    }
    for (i, imported) in imports {
        if imported
            .names
            .as_ref()
            .is_some_and(|names| names.contains(name))
        {
            return (Resolved::Imported(*i), false);
        }
    }
    match fallback {
        Some((def, early)) => (Resolved::Local(def), early),
        None => (Resolved::Unresolved, false),
    }
}

fn resolve_type(
    index: &SyntaxIndex,
    defs: &HashMap<(ScopeId, &str), Vec<OccId>>,
    occ_id: OccId,
    imports: &[(usize, &ImportedNames)],
) -> Resolved {
    let occ = &index.occurrences[occ_id];
    let name = occ.name.as_str();
    for scope in index.scope_chain(occ.scope) {
        if let Some(candidates) = defs.get(&(scope, name)) {
            if let Some(def) = candidates
                .iter()
                .copied()
                .find(|d| kind_of(index, *d).is_some_and(|k| k.is_type()))
            {
                return Resolved::Local(def);
            }
        }
    }
    if is_builtin_type_name(name) {
        return Resolved::Global;
    }
    for (i, imported) in imports {
        if imported
            .names
            .as_ref()
            .is_some_and(|names| names.contains(name))
        {
            return Resolved::Imported(*i);
        }
    }
    Resolved::Unresolved
}

/// Names visible at a scope, most likely first (for "did you mean"): value
/// definitions from the innermost scope outwards, then imported names,
/// then globals. Each name appears once.
pub fn visible_value_names(
    index: &SyntaxIndex,
    scope: ScopeId,
    imports: &[ImportedNames],
) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for s in index.scope_chain(scope) {
        let mut here: Vec<String> = index
            .occurrences
            .iter()
            .filter(|o| o.scope == s && o.def_kind().is_some_and(|k| k.is_value()))
            .map(|o| o.name.clone())
            .collect();
        here.sort();
        names.extend(here);
    }
    for imported in imports {
        if let Some(set) = &imported.names {
            let mut set: Vec<String> = set.iter().cloned().collect();
            set.sort();
            names.extend(set);
        }
    }
    names.extend(builtins::global_names().into_iter().map(String::from));
    dedup_keep_first(names)
}

fn dedup_keep_first(names: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    names
        .into_iter()
        .filter(|n| seen.insert(n.clone()))
        .collect()
}

/// Type names visible anywhere in the file (types are file-global): user
/// types first, then imported names, then builtin types.
pub fn visible_type_names(index: &SyntaxIndex, imports: &[ImportedNames]) -> Vec<String> {
    let mut names: Vec<String> = index
        .occurrences
        .iter()
        .filter(|o| o.def_kind().is_some_and(|k| k.is_type()))
        .map(|o| o.name.clone())
        .collect();
    names.sort();
    for imported in imports {
        if let Some(set) = &imported.names {
            let mut set: Vec<String> = set.iter().cloned().collect();
            set.sort();
            names.extend(set);
        }
    }
    names.extend(
        [
            "Int", "Float", "String", "Bool", "Null", "Any", "Object", "Json", "Option", "Result",
            "Map", "Set", "Array",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    dedup_keep_first(names)
}
