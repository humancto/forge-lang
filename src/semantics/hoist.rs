//! Hoisting of top-level function definitions (both engines, all editions).
//!
//! A top-level `fn` may be called above its definition:
//!
//! ```forge
//! let avg = mean([1, 2, 3])
//! fn mean(xs) { return sum(xs) / len(xs) }
//! ```
//!
//! Neither engine has a separate declaration pass, so the rule is a
//! reordering of the top-level statements that both engines (and the type
//! checker) take from [`plan`]: each hoistable definition runs as early as
//! it can *without changing what any name means*. A definition moves up
//! past a statement only when that statement binds none of the names the
//! function reads (its free names, see `Free`); so a function that
//! reads a top-level variable lands right after that variable's (last)
//! definition, and sees exactly the bindings it saw at its written
//! position. There is therefore never a "not yet initialized" captured
//! variable: calling such a function above the variable it reads fails
//! with the ordinary undefined-variable error, naming the function.
//!
//! What is hoisted:
//!
//! * `fn` / `async fn` statements at the top level (decorated ones too),
//!   whose name is bound by no other statement of the file and is not a
//!   builtin, stdlib module or other predefined name (REPL sessions also
//!   count the names earlier steps defined). A file that defines `f` twice,
//!   or also assigns `f`, keeps both definitions where they are — so
//!   redefinition behaves exactly as before.
//!
//! `impl` / `give` blocks are not hoisted: a method call above the block
//! does not fail today, it resolves to a field, an embedded type's method
//! or a builtin method, so moving the block would change working programs.
//! Method calls inside function bodies already see every `impl` (they run
//! later).
//!
//! Function definitions have no effects besides binding their name, and a
//! name that was not bound yet could only have failed to resolve, so only
//! programs that used to stop with an undefined-name error change meaning.
//! The analysis over-approximates wherever it is unsure (a statement may
//! bind more names than it really does, a function may read more), which
//! only ever keeps a definition lower.

use crate::parser::ast::*;
use std::collections::{HashMap, HashSet};

/// The execution order of a list of top-level statements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Statement indices in the order they run.
    pub order: Vec<usize>,
    /// For each statement that runs earlier than written: the index of the
    /// first non-hoisted statement it now runs before (the definition is
    /// visible from that statement on).
    pub moved: Vec<(usize, usize)>,
}

impl Plan {
    /// Whether statement `index` runs earlier than written.
    pub fn was_moved(&self, index: usize) -> bool {
        self.moved.binary_search_by_key(&index, |(i, _)| *i).is_ok()
    }
}

/// Names defined before any program code runs: builtins of the current
/// edition, stdlib modules and the Option constructors (the same set the
/// interpreter's global scope starts with). A top-level function with one
/// of these names is not hoisted, so code above it keeps calling the
/// builtin.
pub fn is_predefined(name: &str) -> bool {
    matches!(name, "Some" | "None" | "null")
        || crate::builtins_registry::modules()
            .iter()
            .any(|m| m.name == name)
        || crate::builtins_registry::globals_in(super::edition::current()).any(|b| b.name == name)
}

/// The order in which `statements` run, hoisting function definitions.
/// `predefined` says which names exist before the first
/// statement runs (normally [`is_predefined`]; a REPL step also passes the
/// names earlier steps defined).
pub fn plan(statements: &[SpannedStmt], predefined: &dyn Fn(&str) -> bool) -> Plan {
    let n = statements.len();
    let identity = || Plan {
        order: (0..n).collect(),
        moved: Vec::new(),
    };
    if !statements
        .iter()
        .any(|s| matches!(s.stmt, Stmt::FnDef { .. }))
    {
        return identity();
    }

    let binders: Vec<Binders> = statements.iter().map(|s| binders(&s.stmt)).collect();
    // How many statements may bind each name.
    let mut bound_by: HashMap<&str, usize> = HashMap::new();
    for b in &binders {
        for name in &b.names {
            *bound_by.entry(name.as_str()).or_default() += 1;
        }
    }

    let hoistable: Vec<bool> = statements
        .iter()
        .map(|s| match &s.stmt {
            Stmt::FnDef { name, .. } => {
                !predefined(name) && bound_by.get(name.as_str()).copied() == Some(1)
            }
            _ => false,
        })
        .collect();
    // Hoisted definitions by name (each is bound exactly once).
    let mut hoisted_fn: HashMap<&str, usize> = HashMap::new();
    for (i, spanned) in statements.iter().enumerate() {
        if let (true, Stmt::FnDef { name, .. }) = (hoistable[i], &spanned.stmt) {
            hoisted_fn.insert(name.as_str(), i);
        }
    }
    // Names some non-hoisted statement binds, and the last statement that
    // may bind any name.
    let bound_somewhere: HashSet<&str> = binders
        .iter()
        .enumerate()
        .filter(|(i, _)| !hoistable[*i])
        .flat_map(|(_, b)| b.names.iter().map(String::as_str))
        .collect();
    let last_everything = binders
        .iter()
        .enumerate()
        .filter(|(i, b)| !hoistable[*i] && b.everything)
        .map(|(i, _)| i)
        .last();

    // Each hoisted statement is placed in the "bucket" of the statement it
    // runs before (`anchor`); buckets keep source order.
    let mut anchors: Vec<Option<usize>> = vec![None; n];
    // Hoisted definitions each hoisted definition reads.
    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];
    // Lowest bucket a statement reading `name` may be placed in.
    let mut ready: HashMap<String, usize> = HashMap::new();
    // Lowest bucket for anything (after a statement that may bind any name).
    let mut barrier = 0usize;
    for (i, spanned) in statements.iter().enumerate() {
        if hoistable[i] {
            let mut anchor = barrier;
            for name in needed_names(&spanned.stmt) {
                if let Some(&j) = hoisted_fn.get(name.as_str()) {
                    if j != i {
                        deps[i].push(j);
                    }
                } else if let Some(&r) = ready.get(name.as_str()) {
                    anchor = anchor.max(r);
                } else if !predefined(&name)
                    && (bound_somewhere.contains(name.as_str())
                        || last_everything.is_some_and(|e| e > i))
                {
                    // Only defined below this definition: moving it up
                    // would expose it where what it reads does not exist.
                    anchor = i;
                }
            }
            anchors[i] = Some(anchor.min(i));
        } else {
            let b = &binders[i];
            if b.everything {
                barrier = i + 1;
            }
            for name in &b.names {
                ready.insert(name.clone(), i + 1);
            }
        }
    }
    // A definition goes no higher than the definitions it reads, wherever
    // they are written (transitively; anchors only grow, so this ends).
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..n {
            let Some(current) = anchors[i] else { continue };
            let wanted = deps[i]
                .iter()
                .filter_map(|&j| anchors[j])
                .fold(current, usize::max)
                .min(i);
            if wanted != current {
                anchors[i] = Some(wanted);
                changed = true;
            }
        }
    }

    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); n + 1];
    for (i, anchor) in anchors.iter().enumerate() {
        if let Some(anchor) = anchor {
            buckets[*anchor].push(i);
        }
    }
    let mut order = Vec::with_capacity(n);
    for (a, bucket) in buckets.iter().enumerate().take(n) {
        order.extend(bucket.iter().copied());
        if anchors[a].is_none() {
            order.push(a);
        }
    }

    // Visibility for tools: the first non-hoisted statement running after a
    // moved one, if that statement was written before it.
    let mut moved = Vec::new();
    for (pos, &i) in order.iter().enumerate() {
        if anchors[i].is_none() {
            continue;
        }
        if let Some(&next) = order[pos + 1..].iter().find(|&&j| anchors[j].is_none()) {
            if next < i {
                moved.push((i, next));
            }
        }
    }
    if moved.is_empty() {
        return identity();
    }
    moved.sort_unstable();
    Plan { order, moved }
}

/// Names a hoisted definition must see defined before it runs.
fn needed_names(stmt: &Stmt) -> HashSet<String> {
    let mut free = Free::default();
    if let Stmt::FnDef {
        params,
        body,
        decorators,
        ..
    } = stmt
    {
        free.decorators(decorators);
        free.function(params, body);
    }
    free.out
}

/// Free-name analysis of a function: every name it may read from outside
/// itself. Over-approximates: a name counts as free unless a binding
/// certainly in scope at the use shadows it.
#[derive(Default)]
struct Free {
    scopes: Vec<HashSet<String>>,
    out: HashSet<String>,
    /// With `track_roots`: free names the analysed function's own body (at
    /// `depth` 1, not its nested functions) uses as the root of a
    /// container operation (see [`container_roots`]).
    track_roots: bool,
    roots: HashSet<String>,
    depth: usize,
}

/// Free names that a function's own body (not its nested functions) uses
/// as a container: `x[i]`, `x.f` (reads and assignment targets),
/// `x.method(..)`, `len(x)` / `get(x, ..)` / `has_key(x, ..)` /
/// `contains(x, ..)` / `push(x, ..)` / `pop(x)` (not `x = x + e`: a scalar
/// counter gains nothing and pays the binding per call). The VM
/// binds such names to a local register that mirrors the captured cell
/// (`BindUpvalue`), so that the in-place container opcodes apply to them.
/// Over-approximates like the free-name analysis (a name counts unless a
/// binding certainly shadows it); the compiler checks that each name really
/// is a captured mutable variable.
pub fn container_roots(params: &[Param], body: &[SpannedStmt]) -> HashSet<String> {
    let mut free = Free {
        track_roots: true,
        ..Free::default()
    };
    free.function(params, body);
    free.roots
}

impl Free {
    fn bound(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(name))
    }

    fn use_name(&mut self, name: &str) {
        if !self.bound(name) {
            self.out.insert(name.to_string());
        }
    }

    /// Record `expr` as a container root when it is a free name used in the
    /// analysed function's own body.
    fn root(&mut self, expr: &Expr) {
        if let Expr::Ident(name) = expr {
            if self.track_roots && self.depth == 1 && !self.bound(name) {
                self.roots.insert(name.clone());
            }
        }
    }

    /// The container-root shapes of [`container_roots`] in `expr` itself.
    fn expr_roots(&mut self, expr: &Expr) {
        if !self.track_roots || self.depth != 1 {
            return;
        }
        match expr {
            Expr::Index { object, .. }
            | Expr::FieldAccess { object, .. }
            | Expr::MethodCall { object, .. } => self.root(object),
            Expr::Call { function, args } => {
                if let (Expr::Ident(f), Some(first)) = (function.as_ref(), args.first()) {
                    if matches!(
                        f.as_str(),
                        "len" | "get" | "has_key" | "contains" | "push" | "pop"
                    ) {
                        self.root(first);
                    }
                }
            }
            _ => {}
        }
    }

    fn bind(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    fn decorators(&mut self, decorators: &[Decorator]) {
        for d in decorators {
            for arg in &d.args {
                match arg {
                    DecoratorArg::Positional(e) | DecoratorArg::Named(_, e) => self.expr(e),
                }
            }
        }
    }

    /// Parameters are bound in a fresh scope; default values are read
    /// before any parameter is (conservative), then the body runs.
    fn function(&mut self, params: &[Param], body: &[SpannedStmt]) {
        for p in params {
            if let Some(d) = &p.default {
                self.expr(d);
            }
        }
        self.scopes
            .push(params.iter().map(|p| p.name.clone()).collect());
        self.depth += 1;
        self.block(body);
        self.depth -= 1;
        self.scopes.pop();
    }

    fn block(&mut self, body: &[SpannedStmt]) {
        self.scopes.push(HashSet::new());
        for s in body {
            self.stmt(&s.stmt);
        }
        self.scopes.pop();
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, value, .. } => {
                self.expr(value);
                self.bind(name);
            }
            Stmt::Destructure { pattern, value, .. } => {
                self.expr(value);
                for name in destructure_names(pattern) {
                    self.bind(name);
                }
            }
            Stmt::Assign { target, value } => {
                self.expr(value);
                self.expr(target);
            }
            Stmt::FnDef {
                name,
                params,
                body,
                decorators,
                ..
            } => {
                self.decorators(decorators);
                self.function(params, body);
                self.bind(name);
            }
            Stmt::StructDef { fields, .. } => {
                for f in fields {
                    if let Some(d) = &f.default {
                        self.expr(d);
                    }
                }
            }
            Stmt::Return(e) => {
                if let Some(e) = e {
                    self.expr(e);
                }
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                self.expr(condition);
                self.block(then_body);
                if let Some(b) = else_body {
                    self.block(b);
                }
            }
            Stmt::Match { subject, arms } => {
                self.expr(subject);
                for arm in arms {
                    self.scopes.push(HashSet::new());
                    self.pattern(&arm.pattern);
                    if let Some(g) = &arm.guard {
                        self.expr(g);
                    }
                    self.block(&arm.body);
                    self.scopes.pop();
                }
            }
            Stmt::For {
                var,
                var2,
                iterable,
                body,
            } => {
                self.expr(iterable);
                let mut scope = HashSet::new();
                scope.insert(var.clone());
                if let Some(v) = var2 {
                    scope.insert(v.clone());
                }
                self.scopes.push(scope);
                self.block(body);
                self.scopes.pop();
            }
            Stmt::While { condition, body } => {
                self.expr(condition);
                self.block(body);
            }
            Stmt::Loop { body }
            | Stmt::Spawn { body }
            | Stmt::Squad { body }
            | Stmt::SafeBlock { body } => self.block(body),
            Stmt::TryCatch {
                try_body,
                catch_var,
                catch_body,
            } => {
                self.block(try_body);
                self.scopes.push(HashSet::from([catch_var.clone()]));
                self.block(catch_body);
                self.scopes.pop();
            }
            Stmt::ImplBlock {
                type_name,
                ability,
                methods,
            } => {
                self.use_name(type_name);
                if let Some(a) = ability {
                    self.use_name(a);
                }
                for m in methods {
                    match &m.stmt {
                        Stmt::FnDef {
                            params,
                            body,
                            decorators,
                            ..
                        } => {
                            self.decorators(decorators);
                            self.function(params, body);
                        }
                        other => self.stmt(other),
                    }
                }
            }
            Stmt::DecoratorStmt(d) => self.decorators(std::slice::from_ref(d)),
            Stmt::YieldStmt(e) | Stmt::Expression(e) => self.expr(e),
            Stmt::When { subject, arms } => {
                self.expr(subject);
                self.when_arms(arms);
            }
            Stmt::CheckStmt { expr, check_kind } => {
                self.expr(expr);
                match check_kind {
                    CheckKind::Contains(e) => self.expr(e),
                    CheckKind::Between(a, b) => {
                        self.expr(a);
                        self.expr(b);
                    }
                    CheckKind::IsNotEmpty | CheckKind::IsTrue => {}
                }
            }
            Stmt::TimeoutBlock { duration: e, body }
            | Stmt::RetryBlock { count: e, body }
            | Stmt::ScheduleBlock {
                interval: e, body, ..
            }
            | Stmt::WatchBlock { path: e, body } => {
                self.expr(e);
                self.block(body);
            }
            Stmt::PromptDef { params, .. } | Stmt::AgentDef { params, .. } => {
                for p in params {
                    if let Some(d) = &p.default {
                        self.expr(d);
                    }
                }
            }
            // Names these bind are not counted as bindings (conservative:
            // a later use stays free).
            Stmt::TypeDef { .. }
            | Stmt::InterfaceDef { .. }
            | Stmt::Import { .. }
            | Stmt::ImportNamespace { .. }
            | Stmt::ImportNative { .. }
            | Stmt::Break
            | Stmt::Continue => {}
        }
    }

    fn when_arms(&mut self, arms: &[WhenArm]) {
        for arm in arms {
            if let Some(v) = &arm.value {
                self.expr(v);
            }
            self.expr(&arm.result);
        }
    }

    /// Pattern names are both read (a bare name may be a unit variant the
    /// engines compare against) and bound for the arm.
    fn pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Literal(e) => self.expr(e),
            Pattern::Binding(name) => {
                self.use_name(name);
                self.bind(name);
            }
            Pattern::Constructor { name, fields } => {
                self.use_name(name);
                for f in fields {
                    self.pattern(f);
                }
            }
            Pattern::Or(alts) => {
                for a in alts {
                    self.pattern(a);
                }
            }
        }
    }

    fn expr(&mut self, expr: &Expr) {
        self.expr_roots(expr);
        match expr {
            Expr::Int(_) | Expr::Float(_) | Expr::StringLit(_) | Expr::Bool(_) => {}
            Expr::Ident(name) => self.use_name(name),
            Expr::StringInterp(parts) => {
                for p in parts {
                    if let StringPart::Expr(e) = p {
                        self.expr(e);
                    }
                }
            }
            Expr::Object(fields) => {
                for (_, e) in fields {
                    self.expr(e);
                }
            }
            Expr::StructInit {
                name,
                fields,
                namespace,
            } => {
                self.use_name(name);
                if let Some(ns) = namespace {
                    self.use_name(ns);
                }
                for (_, e) in fields {
                    self.expr(e);
                }
            }
            Expr::Array(items) | Expr::Tuple(items) => {
                for e in items {
                    self.expr(e);
                }
            }
            Expr::BinOp { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            Expr::UnaryOp { operand: e, .. }
            | Expr::FieldAccess { object: e, .. }
            | Expr::Try(e)
            | Expr::Await(e)
            | Expr::Spread(e)
            | Expr::Must(e)
            | Expr::Freeze(e)
            | Expr::Ask(e) => self.expr(e),
            Expr::Index { object, index } => {
                self.expr(object);
                self.expr(index);
            }
            Expr::Call { function, args } => {
                self.expr(function);
                for a in args {
                    self.expr(a);
                }
            }
            Expr::MethodCall { object, args, .. } => {
                self.expr(object);
                for a in args {
                    self.expr(a);
                }
            }
            Expr::Pipeline { value, function } => {
                self.expr(value);
                self.expr(function);
            }
            Expr::Lambda { params, body } => self.function(params, body),
            Expr::Spawn(body) | Expr::Squad(body) | Expr::Block(body) => self.block(body),
            Expr::WhereFilter { source, value, .. } => {
                self.expr(source);
                self.expr(value);
            }
            Expr::PipeChain { source, steps } => {
                self.expr(source);
                for step in steps {
                    match step {
                        PipeStep::Keep(e) | PipeStep::Take(e) | PipeStep::Apply(e) => self.expr(e),
                        PipeStep::Sort(_) => {}
                    }
                }
            }
        }
    }
}

pub(crate) fn destructure_names(pattern: &DestructurePattern) -> Vec<&str> {
    match pattern {
        DestructurePattern::Object(pairs) => pairs.iter().map(|(_, b)| b.as_str()).collect(),
        DestructurePattern::Array { items, rest } => items
            .iter()
            .map(String::as_str)
            .chain(rest.as_deref())
            .collect(),
        DestructurePattern::Tuple(items) => items.iter().map(String::as_str).collect(),
    }
}

/// Names a top-level statement may bind in the file's scope.
#[derive(Default)]
struct Binders {
    names: HashSet<String>,
    /// The statement may bind names not known here (`import "file"`).
    everything: bool,
}

/// Over-approximation: every binding outside function bodies (blocks that
/// some engine might not scope), plus, inside function and lambda bodies,
/// the forms that can create a global when the body runs (the VM registers
/// nested `fn`s and assignments to unknown names as globals).
fn binders(stmt: &Stmt) -> Binders {
    let mut b = Binders::default();
    collect_binders(stmt, false, &mut b);
    b
}

/// The names a module's top level may bind (over-approximated like
/// [`plan`]'s analysis), and whether it may bind names not known here (a
/// wildcard `import "file"`). The type checker uses it to keep an imported
/// module's own names from resolving to the importer's or to builtins.
pub fn module_binders(statements: &[SpannedStmt]) -> (HashSet<String>, bool) {
    let mut b = Binders::default();
    for s in statements {
        collect_binders(&s.stmt, false, &mut b);
    }
    (b.names, b.everything)
}

fn collect_binders(stmt: &Stmt, in_function: bool, b: &mut Binders) {
    let add = |name: &str, b: &mut Binders| {
        b.names.insert(name.to_string());
    };
    match stmt {
        Stmt::Let { name, value, .. } => {
            if !in_function {
                add(name, b);
            }
            expr_binders(value, in_function, b);
        }
        Stmt::Destructure { pattern, value, .. } => {
            if !in_function {
                for name in destructure_names(pattern) {
                    add(name, b);
                }
            }
            expr_binders(value, in_function, b);
        }
        Stmt::Assign { target, value } => {
            if let Expr::Ident(name) = target {
                add(name, b);
            }
            expr_binders(target, in_function, b);
            expr_binders(value, in_function, b);
        }
        Stmt::FnDef {
            name, params, body, ..
        } => {
            add(name, b);
            params_binders(params, b);
            body_binders(body, true, b);
        }
        Stmt::StructDef { name, fields, .. } => {
            add(name, b);
            for f in fields {
                if let Some(d) = &f.default {
                    expr_binders(d, in_function, b);
                }
            }
        }
        Stmt::TypeDef { name, variants } => {
            add(name, b);
            add(&format!("__type_{}__", name), b);
            for v in variants {
                add(&v.name, b);
            }
        }
        Stmt::InterfaceDef { name, .. } => {
            add(name, b);
            add(&format!("__interface_{}__", name), b);
        }
        Stmt::PromptDef { name, .. } | Stmt::AgentDef { name, .. } => add(name, b),
        Stmt::Import { names, .. } => match names {
            Some(names) => {
                for name in names {
                    add(name.binding(), b);
                }
            }
            None => b.everything = true,
        },
        Stmt::ImportNamespace { name, .. } => add(name, b),
        Stmt::ImportNative { binding, .. } => match binding {
            NativeBinding::Namespace(name) => add(name, b),
            NativeBinding::Names(names) => {
                for name in names {
                    add(name, b);
                }
            }
        },
        Stmt::ImplBlock {
            type_name, methods, ..
        } => {
            add(type_name, b);
            for m in methods {
                if let Stmt::FnDef { params, body, .. } = &m.stmt {
                    params_binders(params, b);
                    body_binders(body, true, b);
                }
            }
        }
        Stmt::Return(e) => {
            if let Some(e) = e {
                expr_binders(e, in_function, b);
            }
        }
        Stmt::If {
            condition,
            then_body,
            else_body,
        } => {
            expr_binders(condition, in_function, b);
            body_binders(then_body, in_function, b);
            if let Some(e) = else_body {
                body_binders(e, in_function, b);
            }
        }
        Stmt::Match { subject, arms } => {
            expr_binders(subject, in_function, b);
            for arm in arms {
                if !in_function {
                    for name in arm.pattern.binding_names() {
                        add(name, b);
                    }
                }
                if let Some(g) = &arm.guard {
                    expr_binders(g, in_function, b);
                }
                body_binders(&arm.body, in_function, b);
            }
        }
        Stmt::For {
            var,
            var2,
            iterable,
            body,
        } => {
            if !in_function {
                add(var, b);
                if let Some(v) = var2 {
                    add(v, b);
                }
            }
            expr_binders(iterable, in_function, b);
            body_binders(body, in_function, b);
        }
        Stmt::While { condition, body } => {
            expr_binders(condition, in_function, b);
            body_binders(body, in_function, b);
        }
        Stmt::Loop { body }
        | Stmt::Spawn { body }
        | Stmt::Squad { body }
        | Stmt::SafeBlock { body } => body_binders(body, in_function, b),
        Stmt::TryCatch {
            try_body,
            catch_var,
            catch_body,
        } => {
            if !in_function {
                add(catch_var, b);
            }
            body_binders(try_body, in_function, b);
            body_binders(catch_body, in_function, b);
        }
        Stmt::TimeoutBlock { duration: e, body }
        | Stmt::RetryBlock { count: e, body }
        | Stmt::ScheduleBlock {
            interval: e, body, ..
        }
        | Stmt::WatchBlock { path: e, body } => {
            expr_binders(e, in_function, b);
            body_binders(body, in_function, b);
        }
        Stmt::When { subject, arms } => {
            expr_binders(subject, in_function, b);
            for arm in arms {
                if let Some(v) = &arm.value {
                    expr_binders(v, in_function, b);
                }
                expr_binders(&arm.result, in_function, b);
            }
        }
        Stmt::CheckStmt { expr, check_kind } => {
            expr_binders(expr, in_function, b);
            match check_kind {
                CheckKind::Contains(e) => expr_binders(e, in_function, b),
                CheckKind::Between(x, y) => {
                    expr_binders(x, in_function, b);
                    expr_binders(y, in_function, b);
                }
                CheckKind::IsNotEmpty | CheckKind::IsTrue => {}
            }
        }
        Stmt::DecoratorStmt(d) => {
            add(&format!("@{}", d.name), b);
            for arg in &d.args {
                match arg {
                    DecoratorArg::Positional(e) | DecoratorArg::Named(_, e) => {
                        expr_binders(e, in_function, b)
                    }
                }
            }
        }
        Stmt::YieldStmt(e) | Stmt::Expression(e) => expr_binders(e, in_function, b),
        Stmt::Break | Stmt::Continue => {}
    }
}

fn params_binders(params: &[Param], b: &mut Binders) {
    for p in params {
        if let Some(d) = &p.default {
            expr_binders(d, true, b);
        }
    }
}

fn body_binders(body: &[SpannedStmt], in_function: bool, b: &mut Binders) {
    for s in body {
        collect_binders(&s.stmt, in_function, b);
    }
}

/// Bindings made by statements nested inside an expression (lambda and
/// block bodies).
fn expr_binders(expr: &Expr, in_function: bool, b: &mut Binders) {
    match expr {
        Expr::Int(_) | Expr::Float(_) | Expr::StringLit(_) | Expr::Bool(_) | Expr::Ident(_) => {}
        Expr::StringInterp(parts) => {
            for p in parts {
                if let StringPart::Expr(e) = p {
                    expr_binders(e, in_function, b);
                }
            }
        }
        Expr::Object(fields) | Expr::StructInit { fields, .. } => {
            for (_, e) in fields {
                expr_binders(e, in_function, b);
            }
        }
        Expr::Array(items) | Expr::Tuple(items) => {
            for e in items {
                expr_binders(e, in_function, b);
            }
        }
        Expr::BinOp { left, right, .. } => {
            expr_binders(left, in_function, b);
            expr_binders(right, in_function, b);
        }
        Expr::UnaryOp { operand: e, .. }
        | Expr::FieldAccess { object: e, .. }
        | Expr::Try(e)
        | Expr::Await(e)
        | Expr::Spread(e)
        | Expr::Must(e)
        | Expr::Freeze(e)
        | Expr::Ask(e) => expr_binders(e, in_function, b),
        Expr::Index { object, index } => {
            expr_binders(object, in_function, b);
            expr_binders(index, in_function, b);
        }
        Expr::Call { function, args } => {
            expr_binders(function, in_function, b);
            for a in args {
                expr_binders(a, in_function, b);
            }
        }
        Expr::MethodCall { object, args, .. } => {
            expr_binders(object, in_function, b);
            for a in args {
                expr_binders(a, in_function, b);
            }
        }
        Expr::Pipeline { value, function } => {
            expr_binders(value, in_function, b);
            expr_binders(function, in_function, b);
        }
        Expr::Lambda { params, body } => {
            params_binders(params, b);
            body_binders(body, true, b);
        }
        Expr::Spawn(body) | Expr::Squad(body) | Expr::Block(body) => {
            body_binders(body, in_function, b)
        }
        Expr::WhereFilter { source, value, .. } => {
            expr_binders(source, in_function, b);
            expr_binders(value, in_function, b);
        }
        Expr::PipeChain { source, steps } => {
            expr_binders(source, in_function, b);
            for step in steps {
                match step {
                    PipeStep::Keep(e) | PipeStep::Take(e) | PipeStep::Apply(e) => {
                        expr_binders(e, in_function, b)
                    }
                    PipeStep::Sort(_) => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Program {
        let tokens = crate::lexer::Lexer::new(src)
            .tokenize()
            .expect("BUG: test source lexes");
        crate::parser::Parser::new(tokens)
            .parse_program()
            .expect("BUG: test source parses")
    }

    fn order(src: &str) -> Vec<usize> {
        plan(&parse(src).statements, &is_predefined).order
    }

    #[test]
    fn call_above_definition_moves_the_definition_to_the_top() {
        let src =
            "let nums = [1, 2]\nlet avg = mean(nums)\nfn mean(xs) { return sum(xs) / len(xs) }";
        assert_eq!(order(src), vec![2, 0, 1]);
    }

    #[test]
    fn a_function_reading_a_top_level_variable_lands_after_it() {
        let src = "say f()\nlet k = 2\nsay f()\nfn f() { return k }";
        assert_eq!(order(src), vec![0, 1, 3, 2]);
        let p = plan(&parse(src).statements, &is_predefined);
        assert_eq!(p.moved, vec![(3, 2)]);
    }

    #[test]
    fn locals_and_parameters_shadow_top_level_names() {
        // `total` and `xs` inside `mean` are its own; it may move above
        // the top-level `let total`.
        let src = "let xs = [1]\nlet total = mean(xs)\nfn mean(xs) { let total = sum(xs)\n return total / len(xs) }";
        assert_eq!(order(src), vec![2, 0, 1]);
    }

    #[test]
    fn a_local_used_before_its_let_is_free() {
        let src = "let total = 1\nsay f()\nfn f() { say total\n let total = 2 }";
        assert_eq!(order(src), vec![0, 2, 1]);
    }

    #[test]
    fn redefined_or_builtin_names_stay_in_place() {
        assert_eq!(
            order("say f()\nfn f() { 1 }\nsay f()\nfn f() { 2 }"),
            vec![0, 1, 2, 3]
        );
        assert_eq!(order("say len([1])\nfn len(x) { 9 }"), vec![0, 1]);
        assert_eq!(order("say f\nlet f = 1\nfn f() { 2 }"), vec![0, 1, 2]);
    }

    #[test]
    fn functions_wait_for_the_functions_they_capture_by_order() {
        let src = "say g()\nlet k = 1\nfn f() { k }\nfn g() { f() }";
        // f needs k; g needs f (placed after it).
        assert_eq!(order(src), vec![0, 1, 2, 3]);
    }

    #[test]
    fn names_defined_only_below_keep_the_definition_in_place() {
        // `late` exists only after `f`: hoisting `f` would make it callable
        // where `late` is still undefined.
        let src = "say f()\nfn f() { return late }\nlet late = 1";
        assert_eq!(order(src), vec![0, 1, 2]);
        // Transitively, through a definition written later.
        let src = "say f()\nfn f() { return g() }\nfn g() { return late }\nlet late = 1";
        assert_eq!(order(src), vec![0, 1, 2, 3]);
        // A later wildcard import may define it too.
        let src = "say f()\nfn f() { return late }\nimport \"x.fg\"";
        assert_eq!(order(src), vec![0, 1, 2]);
        // A function written later that needs nothing still hoists both.
        let src = "say f()\nfn f() { return g() }\nfn g() { return 1 }";
        assert_eq!(order(src), vec![1, 2, 0]);
    }

    #[test]
    fn wildcard_imports_are_barriers() {
        let src = "say f()\nimport \"x.fg\"\nfn f() { 1 }";
        assert_eq!(order(src), vec![0, 1, 2]);
    }

    #[test]
    fn impl_blocks_stay_in_place() {
        let src =
            "struct P { x: Int }\nlet p = P { x: 1 }\nsay p.get()\nimpl P { fn get(it) { it.x } }";
        assert_eq!(order(src), vec![0, 1, 2, 3]);
    }

    #[test]
    fn repl_names_already_defined_are_not_hoisted() {
        let program = parse("say f()\nfn f() { 2 }");
        let p = plan(&program.statements, &|n| n == "f");
        assert!(p.moved.is_empty());
    }
}
