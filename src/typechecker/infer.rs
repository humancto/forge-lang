//! The type checker proper: walks the AST, infers a type for every
//! expression and binding, and reports what is definitely wrong.
//!
//! # Design
//!
//! * **Bidirectional.** [`Checker::infer`] takes an optional expected type.
//!   It flows into lambdas (`map(xs, fn(x) { ... })` gives `x` the element
//!   type of `xs`), empty literals and `None`.
//! * **Unification for generics.** Each call of a generic function (user or
//!   builtin) instantiates its type parameters with fresh inference
//!   variables, solved from the arguments (see `types::InferCtx`).
//! * **Gradual.** Anything unknown is `Ty::Any`, and `Any` is compatible
//!   with everything: a diagnostic is only produced when both sides of a
//!   check are known. Values are only checked against types the *user
//!   declared* (annotations, struct fields, signatures) or against rules the
//!   engines enforce at run time (operators via `semantics::binary`, call
//!   arity via `semantics::check_call_arity`, unknown fields and members).
//!   An inferred type never makes a later, different use an error — Forge
//!   arrays may be heterogeneous and unannotated variables may change type.
//! * **Two passes.** Pass 1 runs silently and records, for every mutable
//!   unannotated variable, the join of all types assigned to it anywhere
//!   (loops included). Pass 2 uses that widened type, so `let mut x = 0`
//!   later assigned `"done"` is `Any` everywhere instead of producing
//!   errors from a stale `Int`.
//!
//! Positions come from the parser's syntax index (`parser::index`) when the
//! caller provides one; without it diagnostics point at statement starts.

use super::builtins;
use super::diagnostics::{Code, Diagnostic, Fix, Severity};
use super::resolve::{Resolution, Resolved};
use super::suggest;
use super::types::{FnTy, InferCtx, Ty};
use crate::parser::ast::*;
use crate::parser::index::{OccId, Pos, Role, ScopeKind, Span, SyntaxIndex};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub type_params: Vec<String>,
    pub fields: Vec<FieldInfo>,
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub ty: Ty,
    pub has_default: bool,
    pub embedded: bool,
}

#[derive(Debug, Clone)]
pub struct AdtInfo {
    pub variants: Vec<(String, Vec<Ty>)>,
}

/// A named function, method, prompt or agent.
#[derive(Debug, Clone)]
pub struct Callable {
    pub sig: FnTy,
}

#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub callable: Callable,
    /// The first parameter receives the instance (`fn area(it)`).
    pub has_receiver: bool,
}

#[derive(Debug, Clone)]
struct IfaceMethod {
    name: String,
    param_count: usize,
    ret: Option<Ty>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindKind {
    /// `let` without `mut`: assignment is a run-time error on both engines.
    Immutable,
    Mutable,
    /// Parameters, loop variables, pattern and catch bindings.
    Other,
}

#[derive(Debug, Clone)]
struct Binding {
    ty: Ty,
    declared: Option<Ty>,
    kind: BindKind,
    id: usize,
}

struct FnFrame {
    declared_ret: Option<Ty>,
    returns: Vec<Ty>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    Normal,
    /// Control never reaches the next statement.
    Diverges,
}

/// Facts learned from a condition: `x != null` makes `x` non-null in the
/// `then` branch.
#[derive(Debug, Clone, PartialEq)]
enum Narrowing {
    NonNull,
    IsNull,
    IsOk,
    IsErr,
}

impl Narrowing {
    fn invert(&self) -> Narrowing {
        match self {
            Narrowing::NonNull => Narrowing::IsNull,
            Narrowing::IsNull => Narrowing::NonNull,
            Narrowing::IsOk => Narrowing::IsErr,
            Narrowing::IsErr => Narrowing::IsOk,
        }
    }

    fn apply(&self, ty: &Ty) -> Ty {
        match (ty, self) {
            (Ty::Option(inner), Narrowing::NonNull) => *inner.clone(),
            (Ty::Option(_), Narrowing::IsNull) => Ty::Null,
            (Ty::Result(ok, _), Narrowing::IsOk) => *ok.clone(),
            (Ty::Result(_, err), Narrowing::IsErr) => *err.clone(),
            _ => ty.clone(),
        }
    }
}

/// Which namespace a name occurrence lives in, for span lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NameClass {
    Value,
    Field,
    Type,
}

/// Results of a check, besides diagnostics, for tools (LSP hover, inlay
/// hints).
#[derive(Debug, Default, Clone)]
pub struct TypeFacts {
    /// Type of each definition occurrence (variables, parameters,
    /// functions, ...), keyed by its index occurrence.
    pub def_types: HashMap<OccId, Ty>,
    /// Type of each field-access occurrence (`p.name`).
    pub field_types: HashMap<OccId, Ty>,
}

/// Builtin methods callable on struct instances through the engines'
/// method fallback (besides every global builtin).
const EXTRA_VALUE_METHODS: &[&str] = &[
    "trim_start",
    "trim_end",
    "is_empty",
    "is_numeric",
    "is_alpha",
    "is_alphanumeric",
    "char_at",
    "encode_uri",
    "decode_uri",
    "words",
    "bytes",
    "has",
    "add",
    "remove",
    "delete",
    "clear",
    "size",
    "set",
    "get",
    "stream",
    "to_array",
    "collect",
    "to_string",
];

pub struct Checker<'a> {
    strict: bool,
    index: Option<&'a SyntaxIndex>,
    resolution: Option<&'a Resolution>,
    by_stmt: HashMap<Pos, Vec<OccId>>,

    // Declarations (collected before checking).
    structs: HashMap<String, StructInfo>,
    adts: HashMap<String, AdtInfo>,
    variant_owner: HashMap<String, String>,
    aliases: HashMap<String, Ty>,
    interfaces: HashMap<String, Vec<IfaceMethod>>,
    methods: HashMap<String, HashMap<String, MethodInfo>>,
    functions: HashMap<String, Callable>,

    // Walk state.
    cx: InferCtx,
    scopes: Vec<HashMap<String, Binding>>,
    frames: Vec<FnFrame>,
    type_params: Vec<Vec<String>>,
    current_stmt: Pos,
    loop_depth: usize,
    /// > 0 inside code that is expected to fail (`assert_throws(fn ...)`).
    suppress: usize,
    /// Pass 1 and return-type inference run without diagnostics.
    mute: bool,
    binding_counter: usize,
    def_cursor: HashMap<(Pos, String), usize>,
    field_cursor: HashMap<(Pos, String), usize>,

    /// Pass 1 → pass 2: widened type of each mutable binding.
    widened: HashMap<usize, Ty>,

    diagnostics: Vec<Diagnostic>,
    seen: HashSet<(Code, Span, String)>,
    /// Spans of code expected to fail; resolver diagnostics inside them are
    /// dropped too.
    pub suppressed_regions: Vec<Span>,
    facts: TypeFacts,
}

impl<'a> Checker<'a> {
    pub fn new(
        strict: bool,
        index: Option<&'a SyntaxIndex>,
        resolution: Option<&'a Resolution>,
    ) -> Self {
        let mut by_stmt: HashMap<Pos, Vec<OccId>> = HashMap::new();
        if let Some(ix) = index {
            for (id, occ) in ix.occurrences.iter().enumerate() {
                by_stmt.entry(occ.stmt).or_default().push(id);
            }
        }
        Self {
            strict,
            index,
            resolution,
            by_stmt,
            structs: HashMap::new(),
            adts: HashMap::new(),
            variant_owner: HashMap::new(),
            aliases: HashMap::new(),
            interfaces: HashMap::new(),
            methods: HashMap::new(),
            functions: HashMap::new(),
            cx: InferCtx::default(),
            scopes: vec![HashMap::new()],
            frames: Vec::new(),
            type_params: Vec::new(),
            current_stmt: Pos::default(),
            loop_depth: 0,
            suppress: 0,
            mute: false,
            binding_counter: 0,
            def_cursor: HashMap::new(),
            field_cursor: HashMap::new(),
            widened: HashMap::new(),
            diagnostics: Vec::new(),
            seen: HashSet::new(),
            suppressed_regions: Vec::new(),
            facts: TypeFacts::default(),
        }
    }

    pub fn into_results(self) -> (Vec<Diagnostic>, TypeFacts, Vec<Span>) {
        (self.diagnostics, self.facts, self.suppressed_regions)
    }

    /// Check a whole program.
    pub fn check_program(&mut self, program: &Program) {
        self.collect_declarations(&program.statements);
        self.infer_return_types(&program.statements);

        // Pass 1: silent, records widened types of mutable variables.
        self.mute = true;
        self.walk_program(program);
        self.mute = false;

        // Pass 2: the real check.
        self.walk_program(program);
    }

    /// Bring in the declarations of an imported module (all of them for a
    /// wildcard import, only `names` otherwise).
    pub fn import_declarations(&mut self, program: &Program, names: Option<&[String]>) {
        let wanted = |name: &str| names.is_none_or(|n| n.iter().any(|x| x == name));
        let stmts: Vec<SpannedStmt> = program
            .statements
            .iter()
            .filter(|s| match &s.stmt {
                Stmt::FnDef { name, .. }
                | Stmt::StructDef { name, .. }
                | Stmt::InterfaceDef { name, .. } => wanted(name),
                Stmt::TypeDef { name, variants } => {
                    wanted(name) || variants.iter().any(|v| wanted(&v.name))
                }
                Stmt::ImplBlock { type_name, .. } => wanted(type_name),
                _ => false,
            })
            .cloned()
            .collect();
        self.collect_declarations(&stmts);
        // Imported function bodies are not checked here, but their return
        // types are still inferred so calls get useful result types.
        let before = std::mem::take(&mut self.diagnostics);
        let mute = std::mem::replace(&mut self.mute, true);
        self.infer_return_types(&stmts);
        self.mute = mute;
        self.diagnostics = before;
    }

    fn walk_program(&mut self, program: &Program) {
        self.cx = InferCtx::default();
        self.scopes = vec![HashMap::new()];
        self.frames.clear();
        self.binding_counter = 0;
        self.def_cursor.clear();
        self.field_cursor.clear();
        self.loop_depth = 0;
        self.suppress = 0;
        self.check_stmts(&program.statements);
    }

    // ===================================================================
    // Diagnostics and positions
    // ===================================================================

    fn report(
        &mut self,
        code: Code,
        span: Span,
        message: impl Into<String>,
        help: Option<String>,
        fixes: Vec<Fix>,
    ) {
        if self.mute || self.suppress > 0 {
            return;
        }
        let message = message.into();
        if !self.seen.insert((code, span, message.clone())) {
            return;
        }
        self.diagnostics.push(Diagnostic {
            code,
            severity: if self.strict {
                Severity::Error
            } else {
                Severity::Warning
            },
            message,
            span,
            help,
            fixes,
        });
    }

    fn simple(&mut self, code: Code, span: Span, message: impl Into<String>) {
        self.report(code, span, message, None, Vec::new());
    }

    fn stmt_span(&self) -> Span {
        let start = self.current_stmt;
        let end = self
            .by_stmt
            .get(&start)
            .and_then(|ids| {
                ids.iter()
                    .filter_map(|id| self.index.map(|ix| ix.occurrences[*id].span.end))
                    .max()
            })
            .unwrap_or(Pos::new(start.line, start.col + 1));
        if start.line == 0 {
            return Span::default();
        }
        Span::new(start, end.max(Pos::new(start.line, start.col + 1)))
    }

    fn occ_matches(&self, id: OccId, name: &str, class: NameClass) -> bool {
        let Some(ix) = self.index else { return false };
        let occ = &ix.occurrences[id];
        occ.name == name
            && match class {
                NameClass::Field => matches!(occ.role, Role::Field),
                NameClass::Type => matches!(occ.role, Role::TypeRef),
                NameClass::Value => matches!(occ.role, Role::Ref | Role::Def(_)),
            }
    }

    /// Occurrences of `name` in the current statement, in source order.
    fn occs_in_stmt(&self, name: &str, class: NameClass) -> Vec<OccId> {
        self.by_stmt
            .get(&self.current_stmt)
            .map(|ids| {
                ids.iter()
                    .copied()
                    .filter(|id| self.occ_matches(*id, name, class))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Span of the first occurrence of `name` in the current statement, or
    /// the statement itself.
    fn name_span(&self, name: &str, class: NameClass) -> Span {
        match (self.index, self.occs_in_stmt(name, class).first()) {
            (Some(ix), Some(id)) => ix.occurrences[*id].span,
            _ => self.stmt_span(),
        }
    }

    /// Best span for an expression: from its first to its last name in the
    /// current statement (literals carry no positions).
    fn expr_span(&self, expr: &Expr) -> Span {
        self.expr_span_or(expr, self.stmt_span())
    }

    /// Like [`Checker::expr_span`], with the span to use when the expression
    /// mentions no names (a literal).
    fn expr_span_or(&self, expr: &Expr, fallback: Span) -> Span {
        let Some(ix) = self.index else {
            return fallback;
        };
        let mut names = Vec::new();
        collect_expr_names(expr, &mut names);
        let mut first: Option<Span> = None;
        let mut last: Option<Span> = None;
        for (name, class) in names {
            if let Some(id) = self.occs_in_stmt(&name, class).first() {
                let span = ix.occurrences[*id].span;
                if first.is_none_or(|f| span.start < f.start) {
                    first = Some(span);
                }
                if last.is_none_or(|l| span.end > l.end) {
                    last = Some(span);
                }
            }
        }
        match (first, last) {
            (Some(f), Some(l)) => Span::new(f.start, l.end),
            _ => fallback,
        }
    }

    /// The index definition for the next binding of `name` in the current
    /// statement (definitions are matched to the index in source order).
    fn next_def(&mut self, name: &str) -> Option<OccId> {
        let ix = self.index?;
        let key = (self.current_stmt, name.to_string());
        let n = *self.def_cursor.get(&key).unwrap_or(&0);
        let defs: Vec<OccId> = self
            .by_stmt
            .get(&self.current_stmt)?
            .iter()
            .copied()
            .filter(|id| {
                let o = &ix.occurrences[*id];
                o.name == name && o.is_def()
            })
            .collect();
        let found = defs.get(n).copied();
        if found.is_some() {
            self.def_cursor.insert(key, n + 1);
        }
        found
    }

    fn record_def_type(&mut self, name: &str, ty: &Ty) {
        if self.mute {
            return;
        }
        if let Some(def) = self.next_def(name) {
            let ty = self.cx.finalize(ty);
            self.facts.def_types.insert(def, ty);
        }
    }

    fn record_field_type(&mut self, field: &str, ty: &Ty) {
        if self.mute {
            return;
        }
        let Some(ix) = self.index else { return };
        let key = (self.current_stmt, field.to_string());
        let n = *self.field_cursor.get(&key).unwrap_or(&0);
        let occs = self.occs_in_stmt(field, NameClass::Field);
        if let Some(id) = occs.get(n) {
            self.field_cursor.insert(key, n + 1);
            if matches!(ix.occurrences[*id].role, Role::Field) {
                let ty = self.cx.finalize(ty);
                self.facts.field_types.insert(*id, ty);
            }
        }
    }

    /// Is `name` (as used in the current statement) the builtin/module of
    /// that name, i.e. not shadowed by a user definition?
    fn is_global_here(&self, name: &str) -> bool {
        if self.lookup(name).is_some() || self.functions.contains_key(name) {
            return false;
        }
        match (self.index, self.resolution) {
            (Some(_), Some(res)) => {
                let occs = self.occs_in_stmt(name, NameClass::Value);
                match occs.first() {
                    Some(id) => matches!(res.resolved[*id], Resolved::Global),
                    // Synthesized names (`say` → println) have no
                    // occurrence; they are always the builtin.
                    None => builtins::is_global(name),
                }
            }
            _ => builtins::is_global(name),
        }
    }

    // ===================================================================
    // Scopes
    // ===================================================================

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    fn bind(&mut self, name: &str, ty: Ty, declared: Option<Ty>, kind: BindKind) {
        let id = self.binding_counter;
        self.binding_counter += 1;
        let effective = match (&declared, kind) {
            (Some(d), _) => d.clone(),
            (None, BindKind::Mutable) => match self.widened.get(&id) {
                Some(w) if !self.mute => w.clone(),
                _ => ty.clone(),
            },
            _ => ty.clone(),
        };
        if self.mute && kind == BindKind::Mutable && declared.is_none() {
            let resolved = self.cx.resolve(&ty);
            self.widened.insert(id, resolved);
        }
        self.record_def_type(name, &effective);
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(
                name.to_string(),
                Binding {
                    ty: effective,
                    declared,
                    kind,
                    id,
                },
            );
        }
    }

    /// Temporarily change a visible binding's type (narrowing).
    fn set_binding_type(&mut self, name: &str, ty: Ty) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(b) = scope.get_mut(name) {
                b.ty = ty;
                return;
            }
        }
    }

    // ===================================================================
    // Declarations
    // ===================================================================

    /// Lower an annotation with the given generic parameters in scope (for
    /// tools that only need declarations, like `enforce`).
    pub(crate) fn lower_with(&mut self, ann: &TypeAnn, type_params: &[String]) -> Ty {
        self.type_params.push(type_params.to_vec());
        let ty = self.lower(ann);
        self.type_params.pop();
        ty
    }

    pub(crate) fn is_interface(&self, name: &str) -> bool {
        self.interfaces.contains_key(name)
    }

    /// Lower a type annotation in the current context (type parameters,
    /// aliases, user types). Unknown names become `Any`; the resolver
    /// reports them.
    fn lower(&self, ann: &TypeAnn) -> Ty {
        match ann {
            TypeAnn::Simple(name) => {
                if self.type_params.iter().rev().any(|ps| ps.contains(name)) {
                    return Ty::Param(name.clone());
                }
                if let Some(alias) = self.aliases.get(name) {
                    return alias.clone();
                }
                if self.structs.contains_key(name)
                    || self.adts.contains_key(name)
                    || self.interfaces.contains_key(name)
                {
                    return Ty::Named(name.clone(), Vec::new());
                }
                builtins::builtin_type(name).unwrap_or(Ty::Any)
            }
            TypeAnn::Array(inner) => Ty::array(self.lower(inner)),
            TypeAnn::Optional(inner) => Ty::option(self.lower(inner)),
            TypeAnn::Tuple(items) => Ty::Tuple(items.iter().map(|t| self.lower(t)).collect()),
            TypeAnn::Function(params, ret) => Ty::func(
                params.iter().map(|t| self.lower(t)).collect(),
                self.lower(ret),
            ),
            TypeAnn::Generic(name, args) => {
                let args: Vec<Ty> = args.iter().map(|t| self.lower(t)).collect();
                if let Some(t) = builtins::builtin_generic(name, &args) {
                    return t;
                }
                if self.structs.contains_key(name) || self.adts.contains_key(name) {
                    return Ty::Named(name.clone(), args);
                }
                Ty::Any
            }
        }
    }

    fn callable_from(
        &self,
        type_params: &[String],
        params: &[Param],
        return_type: &Option<TypeAnn>,
    ) -> Callable {
        let mut me = Checker::new(self.strict, None, None);
        me.structs = self.structs.clone();
        me.adts = self.adts.clone();
        me.aliases = self.aliases.clone();
        me.interfaces = self.interfaces.clone();
        me.type_params = self.type_params.clone();
        me.type_params.push(type_params.to_vec());
        let sig = FnTy {
            type_params: type_params.to_vec(),
            params: params
                .iter()
                .map(|p| p.type_ann.as_ref().map_or(Ty::Any, |a| me.lower(a)))
                .collect(),
            required: crate::semantics::required_params(params.iter().map(|p| p.default.is_some())),
            variadic: false,
            ret: Box::new(return_type.as_ref().map_or(Ty::Any, |a| me.lower(a))),
        };
        Callable { sig }
    }

    pub(crate) fn collect_declarations(&mut self, stmts: &[SpannedStmt]) {
        // Types first (in two rounds so annotations can mention types
        // declared later), then everything that uses them.
        for _ in 0..2 {
            for s in stmts {
                match &s.stmt {
                    Stmt::StructDef {
                        name, type_params, ..
                    } => {
                        self.structs.entry(name.clone()).or_insert(StructInfo {
                            type_params: type_params.clone(),
                            fields: Vec::new(),
                        });
                    }
                    Stmt::TypeDef { name, variants } => {
                        let is_alias = !variants.is_empty()
                            && variants.iter().all(|v| {
                                v.fields.is_empty()
                                    && (builtins::builtin_type(&v.name).is_some()
                                        || self.structs.contains_key(&v.name)
                                        || self.adts.contains_key(&v.name)
                                        || self.aliases.contains_key(&v.name)
                                        || stmts.iter().any(|o| {
                                            matches!(&o.stmt, Stmt::StructDef { name, .. } if *name == v.name)
                                        }))
                            });
                        if is_alias {
                            let members: Vec<Ty> = variants
                                .iter()
                                .map(|v| self.lower(&TypeAnn::Simple(v.name.clone())))
                                .collect();
                            let ty = if members.len() == 1 {
                                members.into_iter().next().unwrap_or(Ty::Any)
                            } else {
                                Ty::Union(members)
                            };
                            self.aliases.insert(name.clone(), ty);
                            self.adts.remove(name);
                        } else {
                            self.adts.insert(
                                name.clone(),
                                AdtInfo {
                                    variants: Vec::new(),
                                },
                            );
                        }
                    }
                    Stmt::InterfaceDef { name, .. } => {
                        self.interfaces.entry(name.clone()).or_default();
                    }
                    _ => {}
                }
            }
        }

        for s in stmts {
            match &s.stmt {
                Stmt::StructDef {
                    name,
                    type_params,
                    fields,
                } => {
                    self.type_params.push(type_params.clone());
                    let fields = fields
                        .iter()
                        .map(|f| FieldInfo {
                            name: f.name.clone(),
                            ty: self.lower(&f.type_ann),
                            has_default: f.default.is_some(),
                            embedded: f.embedded,
                        })
                        .collect();
                    self.type_params.pop();
                    self.structs.insert(
                        name.clone(),
                        StructInfo {
                            type_params: type_params.clone(),
                            fields,
                        },
                    );
                }
                Stmt::TypeDef { name, variants } if self.adts.contains_key(name) => {
                    let info = AdtInfo {
                        variants: variants
                            .iter()
                            .map(|v| {
                                (
                                    v.name.clone(),
                                    v.fields.iter().map(|f| self.lower(f)).collect(),
                                )
                            })
                            .collect(),
                    };
                    for v in variants {
                        self.variant_owner.insert(v.name.clone(), name.clone());
                    }
                    self.adts.insert(name.clone(), info);
                }
                Stmt::InterfaceDef { name, methods } => {
                    let methods = methods
                        .iter()
                        .map(|m| IfaceMethod {
                            name: m.name.clone(),
                            param_count: m.params.len(),
                            ret: m.return_type.as_ref().map(|r| self.lower(r)),
                        })
                        .collect();
                    self.interfaces.insert(name.clone(), methods);
                }
                _ => {}
            }
        }

        for s in stmts {
            match &s.stmt {
                Stmt::FnDef {
                    name,
                    type_params,
                    params,
                    return_type,
                    ..
                } => {
                    let callable = self.callable_from(type_params, params, return_type);
                    self.functions.insert(name.clone(), callable);
                }
                Stmt::PromptDef { name, params, .. } | Stmt::AgentDef { name, params, .. } => {
                    let callable = self.callable_from(&[], params, &None);
                    self.functions.insert(name.clone(), callable);
                }
                Stmt::ImplBlock {
                    type_name, methods, ..
                } => {
                    for m in methods {
                        if let Stmt::FnDef {
                            name,
                            type_params,
                            params,
                            return_type,
                            ..
                        } = &m.stmt
                        {
                            let mut callable = self.callable_from(type_params, params, return_type);
                            let has_receiver = params
                                .first()
                                .is_some_and(|p| p.name == "it" || p.name == "self");
                            if has_receiver && params[0].type_ann.is_none() {
                                callable.sig.params[0] = self.self_type(type_name);
                            }
                            self.methods.entry(type_name.clone()).or_default().insert(
                                name.clone(),
                                MethodInfo {
                                    callable,
                                    has_receiver,
                                },
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// The type of `it` in methods of `type_name`.
    fn self_type(&self, type_name: &str) -> Ty {
        if self.structs.contains_key(type_name) || self.adts.contains_key(type_name) {
            Ty::Named(type_name.to_string(), Vec::new())
        } else {
            Ty::Any
        }
    }

    /// Infer the result type of every function without a return
    /// annotation (two rounds, so a caller defined before its callee gets
    /// the callee's type).
    fn infer_return_types(&mut self, stmts: &[SpannedStmt]) {
        let mute = std::mem::replace(&mut self.mute, true);
        for _ in 0..2 {
            for s in stmts {
                match &s.stmt {
                    Stmt::FnDef {
                        name,
                        params,
                        body,
                        return_type: None,
                        type_params,
                        ..
                    } => {
                        let Some(callable) = self.functions.get(name).cloned() else {
                            continue;
                        };
                        let ret =
                            self.infer_body_type(type_params, params, &callable.sig.params, body);
                        if let Some(c) = self.functions.get_mut(name) {
                            *c.sig.ret = ret;
                        }
                    }
                    Stmt::ImplBlock {
                        type_name, methods, ..
                    } => {
                        for m in methods {
                            if let Stmt::FnDef {
                                name,
                                params,
                                body,
                                return_type: None,
                                type_params,
                                ..
                            } = &m.stmt
                            {
                                let Some(info) = self
                                    .methods
                                    .get(type_name)
                                    .and_then(|t| t.get(name))
                                    .cloned()
                                else {
                                    continue;
                                };
                                let ret = self.infer_body_type(
                                    type_params,
                                    params,
                                    &info.callable.sig.params,
                                    body,
                                );
                                if let Some(mi) = self
                                    .methods
                                    .get_mut(type_name)
                                    .and_then(|t| t.get_mut(name))
                                {
                                    *mi.callable.sig.ret = ret;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        self.mute = mute;
    }

    fn infer_body_type(
        &mut self,
        type_params: &[String],
        params: &[Param],
        param_types: &[Ty],
        body: &[SpannedStmt],
    ) -> Ty {
        let saved_scopes = std::mem::replace(&mut self.scopes, vec![HashMap::new()]);
        let saved_cx = std::mem::take(&mut self.cx);
        let saved_counter = self.binding_counter;
        self.type_params.push(type_params.to_vec());
        self.push_scope();
        for (p, ty) in params.iter().zip(param_types) {
            self.bind(&p.name, ty.clone(), None, BindKind::Other);
        }
        self.frames.push(FnFrame {
            declared_ret: None,
            returns: Vec::new(),
        });
        let (tail, flow) = self.check_body(body);
        let frame = self.frames.pop();
        self.type_params.pop();
        let mut ret = Ty::Never;
        let mut any_return = false;
        if let Some(frame) = frame {
            for r in &frame.returns {
                any_return = true;
                ret = self.cx.join(&ret, r);
            }
        }
        if flow == Flow::Normal {
            ret = self.cx.join(&ret, &tail);
        } else if !any_return {
            ret = Ty::Never;
        }
        let ret = self.cx.finalize(&ret);
        self.scopes = saved_scopes;
        self.cx = saved_cx;
        self.binding_counter = saved_counter;
        ret
    }

    // ===================================================================
    // Statements
    // ===================================================================

    /// Check statements in the current scope; report unreachable code.
    /// Returns the type of the block's value (its tail) and its flow.
    fn check_stmts(&mut self, stmts: &[SpannedStmt]) -> (Ty, Flow) {
        let mut flow = Flow::Normal;
        let mut tail = Ty::Null;
        let mut reported_unreachable = false;
        for (i, s) in stmts.iter().enumerate() {
            let saved = self.current_stmt;
            if s.line > 0 {
                self.current_stmt = Pos::new(s.line, s.col);
            }
            if flow == Flow::Diverges && !reported_unreachable {
                reported_unreachable = true;
                let span = self.stmt_span();
                self.simple(
                    Code::Unreachable,
                    span,
                    "unreachable code: the statement before always exits this block",
                );
            }
            let is_last = i + 1 == stmts.len();
            let (ty, f) = self.check_stmt(&s.stmt);
            if is_last {
                tail = ty;
            }
            if f == Flow::Diverges {
                flow = Flow::Diverges;
            }
            self.current_stmt = saved;
        }
        if flow == Flow::Diverges {
            tail = Ty::Never;
        }
        (tail, flow)
    }

    /// A `{ ... }` body in a fresh scope.
    fn check_body(&mut self, stmts: &[SpannedStmt]) -> (Ty, Flow) {
        self.push_scope();
        let result = self.check_stmts(stmts);
        self.pop_scope();
        result
    }

    /// Check one statement. Returns the statement's value (meaningful for
    /// the tail of a function or block expression) and its flow.
    fn check_stmt(&mut self, stmt: &Stmt) -> (Ty, Flow) {
        match stmt {
            Stmt::Let {
                name,
                mutable,
                type_ann,
                value,
            } => {
                let declared = type_ann.as_ref().map(|a| self.lower(a));
                let vt = self.infer(value, declared.as_ref());
                if let Some(d) = &declared {
                    let span = self.expr_span_or(value, self.name_span(name, NameClass::Value));
                    self.expect_assignable(d, &vt, span, |e, a| {
                        format!(
                            "type mismatch: '{}' declared as {} but assigned {}",
                            name, e, a
                        )
                    });
                }
                let kind = if *mutable {
                    BindKind::Mutable
                } else {
                    BindKind::Immutable
                };
                self.bind(name, vt, declared, kind);
                (Ty::Null, Flow::Normal)
            }
            Stmt::Assign { target, value } => {
                self.check_assign(target, value);
                (Ty::Null, Flow::Normal)
            }
            Stmt::FnDef {
                name,
                type_params,
                params,
                return_type,
                body,
                ..
            } => {
                let callable = match self.functions.get(name) {
                    Some(c) if self.frames.is_empty() => c.clone(),
                    _ => self.callable_from(type_params, params, return_type),
                };
                // Nested functions are visible in their own body (recursion).
                self.bind(name, Ty::Fn(callable.sig.clone()), None, BindKind::Other);
                self.check_function(
                    name,
                    type_params,
                    params,
                    &callable,
                    return_type.is_some(),
                    body,
                    None,
                );
                (Ty::Null, Flow::Normal)
            }
            Stmt::Destructure { pattern, value } => {
                let vt = self.infer(value, None);
                let vt = self.cx.resolve(&vt);
                match pattern {
                    DestructurePattern::Object(names) => {
                        for n in names {
                            let ty = self.field_type(&vt, n).unwrap_or(Ty::Any);
                            self.bind(n, ty, None, BindKind::Immutable);
                        }
                    }
                    DestructurePattern::Array { items, rest } => {
                        let elem = match &vt {
                            Ty::Array(e) => *e.clone(),
                            _ => Ty::Any,
                        };
                        for n in items {
                            self.bind(n, elem.clone(), None, BindKind::Immutable);
                        }
                        if let Some(r) = rest {
                            self.bind(r, Ty::array(elem), None, BindKind::Immutable);
                        }
                    }
                    DestructurePattern::Tuple(names) => {
                        for (i, n) in names.iter().enumerate() {
                            let ty = match &vt {
                                Ty::Tuple(items) => items.get(i).cloned().unwrap_or(Ty::Null),
                                _ => Ty::Any,
                            };
                            self.bind(n, ty, None, BindKind::Immutable);
                        }
                    }
                }
                (Ty::Null, Flow::Normal)
            }
            Stmt::StructDef {
                name,
                fields,
                type_params,
            } => {
                self.record_def_type(name, &Ty::Named(name.clone(), Vec::new()));
                self.type_params.push(type_params.clone());
                for f in fields {
                    let declared = self.lower(&f.type_ann);
                    self.record_def_type(&f.name, &declared);
                    if let Some(default) = &f.default {
                        let vt = self.infer(default, Some(&declared));
                        let span = self.expr_span(default);
                        self.expect_assignable(&declared, &vt, span, |e, a| {
                            format!(
                                "type mismatch: default of field '{}' is {} but the field is {}",
                                f.name, a, e
                            )
                        });
                    }
                }
                self.type_params.pop();
                (Ty::Null, Flow::Normal)
            }
            Stmt::TypeDef { name, variants } => {
                let ty = match self.aliases.get(name) {
                    Some(alias) => alias.clone(),
                    None => Ty::Named(name.clone(), Vec::new()),
                };
                self.record_def_type(name, &ty);
                if self.adts.contains_key(name) {
                    for v in variants {
                        let vty = self.variant_value_type(&v.name).unwrap_or(Ty::Any);
                        self.record_def_type(&v.name, &vty);
                    }
                }
                (Ty::Null, Flow::Normal)
            }
            Stmt::InterfaceDef { name, methods } => {
                self.record_def_type(name, &Ty::Named(name.clone(), Vec::new()));
                for m in methods {
                    for p in &m.params {
                        let ty = p.type_ann.as_ref().map_or(Ty::Any, |a| self.lower(a));
                        self.record_def_type(&p.name, &ty);
                    }
                }
                (Ty::Null, Flow::Normal)
            }
            Stmt::ImplBlock {
                type_name,
                ability,
                methods,
            } => {
                for m in methods {
                    let saved = self.current_stmt;
                    if m.line > 0 {
                        self.current_stmt = Pos::new(m.line, m.col);
                    }
                    if let Stmt::FnDef {
                        name,
                        type_params,
                        params,
                        return_type,
                        body,
                        ..
                    } = &m.stmt
                    {
                        let info = self
                            .methods
                            .get(type_name)
                            .and_then(|t| t.get(name))
                            .cloned();
                        let callable = match info {
                            Some(i) => i.callable,
                            None => self.callable_from(type_params, params, return_type),
                        };
                        self.record_def_type(name, &Ty::Fn(callable.sig.clone()));
                        self.check_function(
                            name,
                            type_params,
                            params,
                            &callable,
                            return_type.is_some(),
                            body,
                            Some(type_name),
                        );
                    }
                    self.current_stmt = saved;
                }
                if let Some(iface) = ability {
                    let span = self.name_span(iface, NameClass::Type);
                    self.check_interface_satisfaction(type_name, iface, span);
                }
                (Ty::Null, Flow::Normal)
            }
            Stmt::Return(value) => {
                let declared = self.frames.last().and_then(|f| f.declared_ret.clone());
                let ty = match value {
                    Some(e) => {
                        let ty = self.infer(e, declared.as_ref());
                        if let Some(d) = &declared {
                            let span = self.expr_span(e);
                            self.expect_assignable_code(Code::ReturnType, d, &ty, span, |e, a| {
                                format!("return type mismatch: expected {} but returning {}", e, a)
                            });
                        }
                        ty
                    }
                    None => Ty::Null,
                };
                if let Some(frame) = self.frames.last_mut() {
                    frame.returns.push(ty);
                }
                (Ty::Never, Flow::Diverges)
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => self.check_if(condition, then_body, else_body.as_deref()),
            Stmt::Match { subject, arms } => self.check_match(subject, arms),
            Stmt::For {
                var,
                var2,
                iterable,
                body,
            } => {
                let it = self.infer(iterable, None);
                let it = self.cx.resolve(&it);
                let (t1, t2) = match (&it, var2.is_some()) {
                    (Ty::Array(e) | Ty::Set(e), _) => (*e.clone(), Ty::Any),
                    (Ty::Object, _) | (Ty::Named(..), _) => (Ty::String, Ty::Any),
                    (Ty::Map(k, v), true) => (*k.clone(), *v.clone()),
                    (Ty::Map(k, v), false) => (Ty::Tuple(vec![*k.clone(), *v.clone()]), Ty::Any),
                    _ => (Ty::Any, Ty::Any),
                };
                self.push_scope();
                self.bind(var, t1, None, BindKind::Other);
                if let Some(v2) = var2 {
                    self.bind(v2, t2, None, BindKind::Other);
                }
                self.loop_depth += 1;
                self.check_body(body);
                self.loop_depth -= 1;
                self.pop_scope();
                (Ty::Null, Flow::Normal)
            }
            Stmt::While { condition, body } => {
                self.infer(condition, None);
                self.loop_depth += 1;
                self.check_body(body);
                self.loop_depth -= 1;
                let forever = matches!(condition, Expr::Bool(true)) && !contains_break(body);
                (
                    Ty::Null,
                    if forever {
                        Flow::Diverges
                    } else {
                        Flow::Normal
                    },
                )
            }
            Stmt::Loop { body } => {
                self.loop_depth += 1;
                self.check_body(body);
                self.loop_depth -= 1;
                let forever = !contains_break(body);
                (
                    Ty::Null,
                    if forever {
                        Flow::Diverges
                    } else {
                        Flow::Normal
                    },
                )
            }
            Stmt::Break | Stmt::Continue => {
                if self.loop_depth > 0 {
                    (Ty::Never, Flow::Diverges)
                } else {
                    (Ty::Null, Flow::Normal)
                }
            }
            Stmt::Spawn { body } | Stmt::Squad { body } => {
                self.check_detached_body(body);
                (Ty::Null, Flow::Normal)
            }
            Stmt::DecoratorStmt(decorator) => {
                self.check_decorator(decorator);
                (Ty::Null, Flow::Normal)
            }
            Stmt::TryCatch {
                try_body,
                catch_var,
                catch_body,
            } => {
                let (t1, f1) = self.check_body(try_body);
                self.push_scope();
                self.bind(catch_var, Ty::Any, None, BindKind::Other);
                let (t2, f2) = self.check_body(catch_body);
                self.pop_scope();
                let flow = if f1 == Flow::Diverges && f2 == Flow::Diverges {
                    Flow::Diverges
                } else {
                    Flow::Normal
                };
                (self.cx.join(&t1, &t2), flow)
            }
            Stmt::Import { .. } => (Ty::Null, Flow::Normal),
            Stmt::YieldStmt(e) => {
                self.infer(e, None);
                (Ty::Null, Flow::Normal)
            }
            Stmt::When { subject, arms } => {
                let ty = self.check_when(subject, arms);
                (ty, Flow::Normal)
            }
            Stmt::CheckStmt { expr, check_kind } => {
                self.infer(expr, None);
                match check_kind {
                    CheckKind::Contains(e) => {
                        self.infer(e, None);
                    }
                    CheckKind::Between(lo, hi) => {
                        self.infer(lo, None);
                        self.infer(hi, None);
                    }
                    CheckKind::IsNotEmpty | CheckKind::IsTrue => {}
                }
                (Ty::Null, Flow::Normal)
            }
            Stmt::SafeBlock { body } => {
                let (t, _) = self.check_body(body);
                // A failure inside yields null.
                (self.cx.join(&t, &Ty::Null), Flow::Normal)
            }
            Stmt::TimeoutBlock { duration, body } => {
                self.infer(duration, None);
                self.check_body(body);
                (Ty::Null, Flow::Normal)
            }
            Stmt::RetryBlock { count, body } => {
                self.infer(count, None);
                self.check_body(body);
                (Ty::Null, Flow::Normal)
            }
            Stmt::ScheduleBlock { interval, body, .. } => {
                self.infer(interval, None);
                self.check_detached_body(body);
                (Ty::Null, Flow::Normal)
            }
            Stmt::WatchBlock { path, body } => {
                self.infer(path, None);
                self.check_detached_body(body);
                (Ty::Null, Flow::Normal)
            }
            Stmt::PromptDef { name, params, .. } | Stmt::AgentDef { name, params, .. } => {
                let sig = self
                    .functions
                    .get(name)
                    .map(|c| c.sig.clone())
                    .unwrap_or_else(|| FnTy::new(vec![Ty::Any; params.len()], Ty::Any));
                self.bind(name, Ty::Fn(sig), None, BindKind::Other);
                for p in params {
                    let ty = p.type_ann.as_ref().map_or(Ty::Any, |a| self.lower(a));
                    self.record_def_type(&p.name, &ty);
                }
                (Ty::Null, Flow::Normal)
            }
            Stmt::Expression(expr) => {
                let ty = self.infer(expr, None);
                let flow = if matches!(self.cx.resolve(&ty), Ty::Never) {
                    Flow::Diverges
                } else {
                    Flow::Normal
                };
                (ty, flow)
            }
        }
    }

    /// A body that runs on its own (spawn, schedule, watch): `return`
    /// inside does not return from the enclosing function.
    fn check_detached_body(&mut self, body: &[SpannedStmt]) {
        self.frames.push(FnFrame {
            declared_ret: None,
            returns: Vec::new(),
        });
        let depth = std::mem::replace(&mut self.loop_depth, 0);
        self.check_body(body);
        self.loop_depth = depth;
        self.frames.pop();
    }

    fn check_decorator(&mut self, decorator: &Decorator) {
        for arg in &decorator.args {
            match arg {
                DecoratorArg::Positional(e) | DecoratorArg::Named(_, e) => {
                    self.infer(e, None);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_function(
        &mut self,
        name: &str,
        type_params: &[String],
        params: &[Param],
        callable: &Callable,
        declared_ret: bool,
        body: &[SpannedStmt],
        owner: Option<&str>,
    ) {
        self.type_params.push(type_params.to_vec());
        // Type parameters are definitions in the index too.
        for tp in type_params {
            self.record_def_type(tp, &Ty::Param(tp.clone()));
        }
        self.push_scope();
        for (i, p) in params.iter().enumerate() {
            let mut ty = callable.sig.params.get(i).cloned().unwrap_or(Ty::Any);
            if i == 0
                && owner.is_some()
                && p.type_ann.is_none()
                && (p.name == "it" || p.name == "self")
            {
                ty = owner.map_or(Ty::Any, |o| self.self_type(o));
            }
            if let Some(default) = &p.default {
                let dt = self.infer(default, Some(&ty));
                if p.type_ann.is_some() {
                    let span = self.expr_span(default);
                    self.expect_assignable(&ty, &dt, span, |e, a| {
                        format!(
                            "type mismatch: default of parameter '{}' is {} but the parameter is {}",
                            p.name, a, e
                        )
                    });
                }
            }
            let declared = p.type_ann.as_ref().map(|_| ty.clone());
            self.bind(&p.name, ty, declared, BindKind::Other);
        }
        let ret = if declared_ret {
            Some(*callable.sig.ret.clone())
        } else {
            None
        };
        self.frames.push(FnFrame {
            declared_ret: ret.clone(),
            returns: Vec::new(),
        });
        let depth = std::mem::replace(&mut self.loop_depth, 0);
        let (tail, flow) = self.check_body(body);
        self.loop_depth = depth;
        self.frames.pop();
        self.pop_scope();
        self.type_params.pop();

        if let Some(ret) = ret {
            let ret = self.cx.resolve(&ret);
            if flow == Flow::Normal {
                let accepts_null = {
                    let snap = self.cx.snapshot();
                    let ok = self.cx.assign(&ret, &Ty::Null);
                    self.cx.rollback(snap);
                    ok
                };
                let tail_value = body.last().is_some_and(|s| tail_produces_value(&s.stmt));
                if !tail_value && !accepts_null && !matches!(ret, Ty::Never) {
                    let span = self.name_span(name, NameClass::Value);
                    self.simple(
                        Code::MissingReturn,
                        span,
                        format!(
                            "function '{}' declares return type {} but can finish without returning a value",
                            name, ret
                        ),
                    );
                } else if tail_value {
                    // The last expression is the implicit return value.
                    let saved = self.current_stmt;
                    if let Some(last) = body.last() {
                        if last.line > 0 {
                            self.current_stmt = Pos::new(last.line, last.col);
                        }
                    }
                    let span = match body.last().map(|s| &s.stmt) {
                        Some(Stmt::Expression(e)) => self.expr_span(e),
                        _ => self.stmt_span(),
                    };
                    self.expect_assignable_code(Code::ReturnType, &ret, &tail, span, |e, a| {
                        format!("return type mismatch: expected {} but returning {}", e, a)
                    });
                    self.current_stmt = saved;
                }
            }
        }
    }

    fn check_assign(&mut self, target: &Expr, value: &Expr) {
        match target {
            Expr::Ident(name) => {
                let binding = self.lookup(name).cloned();
                let expected = binding.as_ref().and_then(|b| b.declared.clone());
                let vt = self.infer(value, expected.as_ref());
                if let Some(b) = &binding {
                    if b.kind == BindKind::Immutable {
                        let span = self.name_span(name, NameClass::Value);
                        self.report(
                            Code::ImmutableAssign,
                            span,
                            crate::semantics::immutable_reassign(name),
                            None,
                            Vec::new(),
                        );
                    }
                    if let Some(d) = &b.declared {
                        let span = self.expr_span_or(value, self.name_span(name, NameClass::Value));
                        self.expect_assignable(d, &vt, span, |e, a| {
                            format!("type mismatch: '{}' is {} but assigned {}", name, e, a)
                        });
                    } else if self.mute && b.kind == BindKind::Mutable {
                        let prev = self.widened.get(&b.id).cloned().unwrap_or(b.ty.clone());
                        let joined = self.cx.join(&prev, &vt);
                        let joined = self.cx.resolve(&joined);
                        self.widened.insert(b.id, joined);
                    }
                }
            }
            Expr::FieldAccess { object, field } => {
                let ot = self.infer(object, None);
                let ot = self.cx.resolve(&ot);
                let declared = self.struct_field(&ot, field);
                let vt = self.infer(value, declared.as_ref());
                if let Some(d) = declared {
                    let span = self.expr_span_or(value, self.name_span(field, NameClass::Field));
                    self.expect_assignable(&d, &vt, span, |e, a| {
                        format!(
                            "type mismatch: field '{}' is {} but assigned {}",
                            field, e, a
                        )
                    });
                }
                self.check_mutable_root(object);
            }
            Expr::Index { object, index } => {
                self.infer(object, None);
                self.infer(index, None);
                self.infer(value, None);
                self.check_mutable_root(object);
            }
            other => {
                self.infer(other, None);
                self.infer(value, None);
            }
        }
    }

    /// `p.x = 1` / `xs[0] = 1` mutate the variable `p` / `xs`, which must
    /// be mutable.
    fn check_mutable_root(&mut self, object: &Expr) {
        let mut root = object;
        while let Expr::FieldAccess { object, .. } | Expr::Index { object, .. } = root {
            root = object;
        }
        if let Expr::Ident(name) = root {
            if self
                .lookup(name)
                .is_some_and(|b| b.kind == BindKind::Immutable)
            {
                let span = self.name_span(name, NameClass::Value);
                self.simple(
                    Code::ImmutableAssign,
                    span,
                    crate::semantics::immutable_reassign(name),
                );
            }
        }
    }

    fn check_if(
        &mut self,
        condition: &Expr,
        then_body: &[SpannedStmt],
        else_body: Option<&[SpannedStmt]>,
    ) -> (Ty, Flow) {
        self.infer(condition, None);
        let facts = self.narrowing_facts(condition);
        let originals: Vec<(String, Ty)> = facts
            .iter()
            .filter_map(|(name, _)| self.lookup(name).map(|b| (name.clone(), b.ty.clone())))
            .collect();
        let narrow = |me: &mut Self, invert: bool| {
            for (name, fact) in &facts {
                if let Some((_, original)) = originals.iter().find(|(n, _)| n == name) {
                    let fact = if invert { fact.invert() } else { fact.clone() };
                    let resolved = me.cx.resolve(original);
                    me.set_binding_type(name, fact.apply(&resolved));
                }
            }
        };
        let restore = |me: &mut Self| {
            for (name, ty) in &originals {
                me.set_binding_type(name, ty.clone());
            }
        };

        narrow(self, false);
        let (t1, f1) = self.check_body(then_body);
        restore(self);

        let (t2, f2) = match else_body {
            Some(else_body) => {
                narrow(self, true);
                let r = self.check_body(else_body);
                restore(self);
                r
            }
            None => (Ty::Null, Flow::Normal),
        };

        // `if x == null { return }` narrows the rest of the block.
        if else_body.is_none() && f1 == Flow::Diverges {
            narrow(self, true);
        }

        let flow = if f1 == Flow::Diverges && f2 == Flow::Diverges {
            Flow::Diverges
        } else {
            Flow::Normal
        };
        (self.cx.join(&t1, &t2), flow)
    }

    fn narrowing_facts(&self, expr: &Expr) -> Vec<(String, Narrowing)> {
        let is_null = |e: &Expr| matches!(e, Expr::Ident(n) if n == "null" || n == "None");
        match expr {
            Expr::BinOp { left, op, right } if matches!(op, BinOp::Eq | BinOp::NotEq) => {
                let fact = if *op == BinOp::Eq {
                    Narrowing::IsNull
                } else {
                    Narrowing::NonNull
                };
                match (left.as_ref(), right.as_ref()) {
                    (Expr::Ident(n), r) if is_null(r) => vec![(n.clone(), fact)],
                    (l, Expr::Ident(n)) if is_null(l) => vec![(n.clone(), fact)],
                    _ => vec![],
                }
            }
            Expr::BinOp {
                left,
                op: BinOp::And,
                right,
            } => {
                let mut facts = self.narrowing_facts(left);
                facts.extend(self.narrowing_facts(right));
                facts
            }
            Expr::UnaryOp {
                op: UnaryOp::Not,
                operand,
            } => {
                let facts = self.narrowing_facts(operand);
                // !(a && b) says nothing about a or b individually.
                if facts.len() == 1 {
                    facts.into_iter().map(|(n, f)| (n, f.invert())).collect()
                } else {
                    vec![]
                }
            }
            Expr::Call { function, args } if args.len() == 1 => {
                match (function.as_ref(), &args[0]) {
                    (Expr::Ident(f), Expr::Ident(v)) => match f.as_str() {
                        "is_some" => vec![(v.clone(), Narrowing::NonNull)],
                        "is_none" => vec![(v.clone(), Narrowing::IsNull)],
                        "is_ok" => vec![(v.clone(), Narrowing::IsOk)],
                        "is_err" => vec![(v.clone(), Narrowing::IsErr)],
                        _ => vec![],
                    },
                    _ => vec![],
                }
            }
            _ => vec![],
        }
    }

    fn check_match(&mut self, subject: &Expr, arms: &[MatchArm]) -> (Ty, Flow) {
        let st = self.infer(subject, None);
        let st = self.cx.resolve(&st);
        let subject_name = match subject {
            Expr::Ident(n) => Some(n.clone()),
            _ => None,
        };
        let original = subject_name
            .as_ref()
            .and_then(|n| self.lookup(n).map(|b| b.ty.clone()));
        let mut result = Ty::Never;
        let mut all_diverge = !arms.is_empty();
        for arm in arms {
            self.push_scope();
            self.bind_pattern(&arm.pattern, &st);
            if let (Some(name), Some(_)) = (&subject_name, &original) {
                let narrowed = match &arm.pattern {
                    Pattern::Literal(Expr::Ident(n)) if n == "null" => {
                        Some(Narrowing::IsNull.apply(&st))
                    }
                    Pattern::Constructor { name: c, .. } => match c.as_str() {
                        "Some" => Some(Narrowing::NonNull.apply(&st)),
                        "Ok" => Some(Narrowing::IsOk.apply(&st)),
                        "Err" => Some(Narrowing::IsErr.apply(&st)),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(t) = narrowed {
                    self.set_binding_type(name, t);
                }
            }
            let (t, f) = self.check_body(&arm.body);
            if let (Some(name), Some(orig)) = (&subject_name, &original) {
                self.set_binding_type(name, orig.clone());
            }
            self.pop_scope();
            result = self.cx.join(&result, &t);
            if f == Flow::Normal {
                all_diverge = false;
            }
        }
        let exhaustive = self.check_exhaustiveness(&st, arms);
        if !exhaustive {
            result = self.cx.join(&result, &Ty::Null);
        }
        let flow = if all_diverge && exhaustive {
            Flow::Diverges
        } else {
            Flow::Normal
        };
        (result, flow)
    }

    /// Bind the names a pattern introduces, typed from the subject.
    fn bind_pattern(&mut self, pattern: &Pattern, subject: &Ty) {
        match pattern {
            Pattern::Wildcard | Pattern::Literal(_) => {}
            Pattern::Binding(name) => {
                if self.is_unit_variant_pattern(name) {
                    return;
                }
                self.bind(name, subject.clone(), None, BindKind::Other);
            }
            Pattern::Constructor { name, fields } => {
                let field_types: Vec<Ty> = match (name.as_str(), subject) {
                    ("Some", Ty::Option(inner)) => vec![*inner.clone()],
                    ("Ok", Ty::Result(ok, _)) => vec![*ok.clone()],
                    ("Err", Ty::Result(_, err)) => vec![*err.clone()],
                    _ => self
                        .variant_owner
                        .get(name)
                        .and_then(|owner| self.adts.get(owner))
                        .and_then(|adt| adt.variants.iter().find(|(v, _)| v == name))
                        .map(|(_, f)| f.clone())
                        .unwrap_or_default(),
                };
                for (i, f) in fields.iter().enumerate() {
                    let ty = field_types.get(i).cloned().unwrap_or(Ty::Any);
                    self.bind_pattern(f, &ty);
                }
            }
        }
    }

    fn is_unit_variant_pattern(&self, name: &str) -> bool {
        name == "None" || name == "null" || self.variant_owner.contains_key(name)
    }

    /// Report a non-exhaustive match; returns whether the match is
    /// exhaustive.
    fn check_exhaustiveness(&mut self, subject: &Ty, arms: &[MatchArm]) -> bool {
        let catch_all = arms.iter().any(|a| match &a.pattern {
            Pattern::Wildcard => true,
            Pattern::Binding(n) => !self.is_unit_variant_pattern(n),
            _ => false,
        });
        if catch_all {
            return true;
        }
        let has_ctor = |name: &str| {
            arms.iter().any(|a| match &a.pattern {
                Pattern::Constructor { name: c, .. } => c == name,
                Pattern::Binding(b) => b == name,
                _ => false,
            })
        };
        let (label, missing): (String, Vec<String>) = match subject {
            Ty::Option(_) => {
                let has_none = arms.iter().any(|a| match &a.pattern {
                    Pattern::Literal(Expr::Ident(n)) => n == "null" || n == "None",
                    Pattern::Binding(n) => n == "None" || n == "null",
                    _ => false,
                });
                let mut missing = Vec::new();
                if !has_ctor("Some") {
                    missing.push("Some".to_string());
                }
                if !has_none {
                    missing.push("None".to_string());
                }
                (subject.to_string(), missing)
            }
            Ty::Result(_, _) => {
                let missing = ["Ok", "Err"]
                    .iter()
                    .filter(|c| !has_ctor(c))
                    .map(|c| c.to_string())
                    .collect();
                (subject.to_string(), missing)
            }
            Ty::Bool => {
                let has = |v: bool| {
                    arms.iter()
                        .any(|a| matches!(&a.pattern, Pattern::Literal(Expr::Bool(b)) if *b == v))
                };
                let mut missing = Vec::new();
                if !has(true) {
                    missing.push("true".to_string());
                }
                if !has(false) {
                    missing.push("false".to_string());
                }
                ("Bool".to_string(), missing)
            }
            Ty::Named(name, _) => match self.adts.get(name) {
                Some(adt) => {
                    let missing = adt
                        .variants
                        .iter()
                        .filter(|(v, _)| !has_ctor(v))
                        .map(|(v, _)| v.clone())
                        .collect();
                    (name.clone(), missing)
                }
                None => return true,
            },
            _ => return true,
        };
        if missing.is_empty() {
            return true;
        }
        let span = self.stmt_span();
        self.simple(
            Code::NonExhaustiveMatch,
            span,
            format!(
                "non-exhaustive match on {} — missing: {}",
                label,
                missing.join(", ")
            ),
        );
        false
    }

    fn check_when(&mut self, subject: &Expr, arms: &[WhenArm]) -> Ty {
        self.infer(subject, None);
        let mut result = Ty::Never;
        let mut has_else = false;
        for arm in arms {
            if let Some(v) = &arm.value {
                self.infer(v, None);
            }
            has_else |= arm.is_else;
            let t = self.infer(&arm.result, None);
            result = self.cx.join(&result, &t);
        }
        if !has_else {
            result = self.cx.join(&result, &Ty::Null);
        }
        result
    }

    // ===================================================================
    // Assignability
    // ===================================================================

    fn expect_assignable(
        &mut self,
        expected: &Ty,
        actual: &Ty,
        span: Span,
        msg: impl Fn(&Ty, &Ty) -> String,
    ) -> bool {
        self.expect_assignable_code(Code::TypeMismatch, expected, actual, span, msg)
    }

    /// Check that `actual` fits `expected`; report `code` otherwise.
    /// Interface types are checked structurally (T0012).
    fn expect_assignable_code(
        &mut self,
        code: Code,
        expected: &Ty,
        actual: &Ty,
        span: Span,
        msg: impl Fn(&Ty, &Ty) -> String,
    ) -> bool {
        let e = self.cx.resolve(expected);
        let a = self.cx.resolve(actual);
        if let Ty::Named(iface, _) = &e {
            if self.interfaces.contains_key(iface) {
                return match &a {
                    Ty::Named(st, _) => {
                        let st = st.clone();
                        self.check_interface_satisfaction(&st, iface, span)
                    }
                    t if t.is_unknown() || matches!(t, Ty::Object | Ty::Never) => true,
                    _ => {
                        let m = msg(&e, &a);
                        self.simple(code, span, m);
                        false
                    }
                };
            }
        }
        let snap = self.cx.snapshot();
        if self.cx.assign(&e, &a) {
            return true;
        }
        self.cx.rollback(snap);
        let m = msg(&self.cx.finalize(&e), &self.cx.finalize(&a));
        self.simple(code, span, m);
        false
    }

    /// Does `type_name` provide everything `iface` requires? Reports T0012
    /// for each missing or mismatched member.
    fn check_interface_satisfaction(&mut self, type_name: &str, iface: &str, span: Span) -> bool {
        let Some(required) = self.interfaces.get(iface).cloned() else {
            return true;
        };
        let fields: Vec<FieldInfo> = self
            .structs
            .get(type_name)
            .map(|s| s.fields.clone())
            .unwrap_or_default();
        let methods = self.methods.get(type_name).cloned().unwrap_or_default();
        let mut ok = true;
        for m in &required {
            let has_field = fields.iter().any(|f| f.name == m.name);
            let method = methods.get(&m.name);
            let Some(method) = method else {
                if !has_field {
                    ok = false;
                    self.simple(
                        Code::InterfaceNotSatisfied,
                        span,
                        format!(
                            "struct '{}' does not satisfy interface '{}': missing '{}'",
                            type_name, iface, m.name
                        ),
                    );
                }
                continue;
            };
            let params = method.callable.sig.params.len() - usize::from(method.has_receiver);
            if params != m.param_count {
                ok = false;
                self.simple(
                    Code::InterfaceNotSatisfied,
                    span,
                    format!(
                        "method '{}' on '{}' has {} parameter(s) but interface '{}' requires {}",
                        m.name, type_name, params, iface, m.param_count
                    ),
                );
            }
            if let Some(expected_ret) = &m.ret {
                let actual = method.callable.sig.ret.as_ref().clone();
                let snap = self.cx.snapshot();
                let fits = self.cx.assign(expected_ret, &actual);
                self.cx.rollback(snap);
                if !fits {
                    ok = false;
                    self.simple(
                        Code::InterfaceNotSatisfied,
                        span,
                        format!(
                            "method '{}' on '{}' returns {} but interface '{}' expects {}",
                            m.name, type_name, actual, iface, expected_ret
                        ),
                    );
                }
            }
        }
        ok
    }

    // ===================================================================
    // Expressions
    // ===================================================================

    pub(crate) fn infer(&mut self, expr: &Expr, expected: Option<&Ty>) -> Ty {
        match expr {
            Expr::Int(_) => Ty::Int,
            Expr::Float(_) => Ty::Float,
            Expr::StringLit(_) => Ty::String,
            Expr::Bool(_) => Ty::Bool,
            Expr::StringInterp(parts) => {
                for p in parts {
                    if let StringPart::Expr(e) = p {
                        self.infer(e, None);
                    }
                }
                Ty::String
            }
            Expr::Ident(name) => self.ident_type(name),
            Expr::Array(items) => {
                let expected_elem = match expected.map(|e| self.cx.resolve(e)) {
                    Some(Ty::Array(e)) => Some(*e),
                    _ => None,
                };
                let mut elem: Option<Ty> = None;
                // With an expected element type, the first element that
                // does not fit it decides the array's type (so the
                // mismatch is reported against the declaration).
                let mut misfit: Option<Ty> = None;
                for item in items {
                    let t = match item {
                        Expr::Spread(inner) => match self.infer(inner, None) {
                            Ty::Array(e) => *e,
                            _ => Ty::Any,
                        },
                        _ => self.infer(item, expected_elem.as_ref()),
                    };
                    if let (Some(e), None) = (&expected_elem, &misfit) {
                        let snap = self.cx.snapshot();
                        if !self.cx.assign(e, &t) {
                            self.cx.rollback(snap);
                            misfit = Some(t.clone());
                        }
                    }
                    elem = Some(match elem {
                        None => t,
                        Some(prev) => self.cx.join(&prev, &t),
                    });
                }
                if let Some(bad) = misfit {
                    return Ty::array(bad);
                }
                Ty::array(elem.or(expected_elem).unwrap_or(Ty::Any))
            }
            Expr::Object(fields) => {
                for (_, v) in fields {
                    self.infer(v, None);
                }
                Ty::Object
            }
            Expr::Tuple(items) => Ty::Tuple(items.iter().map(|i| self.infer(i, None)).collect()),
            Expr::BinOp { left, op, right } => self.infer_binop(expr, left, op, right),
            Expr::UnaryOp { op, operand } => {
                let t = self.infer(operand, None);
                match op {
                    UnaryOp::Not => Ty::Bool,
                    UnaryOp::Neg => match self.cx.resolve(&t) {
                        Ty::Int => Ty::Int,
                        Ty::Float => Ty::Float,
                        _ => Ty::Any,
                    },
                }
            }
            Expr::FieldAccess { object, field } => self.infer_field_access(object, field),
            Expr::Index { object, index } => {
                let ot = self.infer(object, None);
                let it = self.infer(index, None);
                match (self.cx.resolve(&ot), self.cx.resolve(&it)) {
                    (Ty::Array(e), _) => *e,
                    (Ty::String, _) => Ty::String,
                    (Ty::Map(_, v), _) => *v,
                    (Ty::Tuple(items), _) => match index.as_ref() {
                        Expr::Int(i) => usize::try_from(*i)
                            .ok()
                            .and_then(|i| items.get(i).cloned())
                            .unwrap_or(Ty::Any),
                        _ => Ty::Any,
                    },
                    _ => Ty::Any,
                }
            }
            Expr::Call { function, args } => self.infer_call(function, args),
            Expr::Try(inner) => match self.infer(inner, None) {
                Ty::Result(ok, _) => *ok,
                _ => Ty::Any,
            },
            Expr::Pipeline { value, function } => {
                let vt = self.infer(value, None);
                let ft = self.infer(function, None);
                match self.cx.resolve(&ft) {
                    Ty::Fn(f) => {
                        let subst = self.instantiate(&f);
                        if let Some(p) = f.params.first() {
                            let p = p.subst_params(&subst);
                            let snap = self.cx.snapshot();
                            if !self.cx.assign(&p, &vt) {
                                self.cx.rollback(snap);
                            }
                        }
                        let ret = f.ret.subst_params(&subst);
                        self.cx.resolve(&ret)
                    }
                    _ => Ty::Any,
                }
            }
            Expr::Lambda { params, body } => self.infer_lambda(params, body, expected),
            Expr::Await(inner) | Expr::Freeze(inner) => self.infer(inner, expected),
            Expr::Must(inner) => match self.infer(inner, None) {
                Ty::Result(ok, _) => *ok,
                Ty::Option(inner) => *inner,
                other => other,
            },
            Expr::Ask(inner) => {
                self.infer(inner, None);
                Ty::Any
            }
            Expr::Spawn(body) => {
                self.check_detached_body(body);
                Ty::Any
            }
            Expr::Squad(body) => {
                self.check_detached_body(body);
                Ty::array(Ty::Any)
            }
            Expr::Spread(inner) => self.infer(inner, None),
            Expr::WhereFilter { source, value, .. } => {
                let st = self.infer(source, None);
                self.infer(value, None);
                match self.cx.resolve(&st) {
                    t @ Ty::Array(_) => t,
                    _ => Ty::array(Ty::Any),
                }
            }
            Expr::PipeChain { source, steps } => {
                self.infer(source, None);
                for step in steps {
                    match step {
                        PipeStep::Keep(e) | PipeStep::Take(e) | PipeStep::Apply(e) => {
                            // `keep where` predicates are synthesized lambdas
                            // over `it`; their names carry no positions.
                            self.infer(e, None);
                        }
                        PipeStep::Sort(_) => {}
                    }
                }
                Ty::Any
            }
            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                let callee = Expr::FieldAccess {
                    object: object.clone(),
                    field: method.clone(),
                };
                self.infer_call(&callee, args)
            }
            Expr::StructInit { name, fields } => self.infer_struct_init(name, fields),
            Expr::Block(stmts) => {
                let (t, _) = self.check_body(stmts);
                t
            }
        }
    }

    fn ident_type(&mut self, name: &str) -> Ty {
        if let Some(b) = self.lookup(name) {
            return b.ty.clone();
        }
        match name {
            "null" => return Ty::Null,
            "None" => return Ty::option(Ty::Any),
            _ => {}
        }
        if let Some(c) = self.functions.get(name) {
            return Ty::Fn(c.sig.clone());
        }
        if let Some(t) = self.variant_value_type(name) {
            return t;
        }
        if let Some(sig) = builtins::function(name) {
            return Ty::Fn(sig.clone());
        }
        Ty::Any
    }

    /// The value a variant name denotes: a constructor function, or the
    /// value itself for a unit variant.
    fn variant_value_type(&self, name: &str) -> Option<Ty> {
        let owner = self.variant_owner.get(name)?;
        let adt = self.adts.get(owner)?;
        let (_, fields) = adt.variants.iter().find(|(v, _)| v == name)?;
        let owner_ty = Ty::Named(owner.clone(), Vec::new());
        Some(if fields.is_empty() {
            owner_ty
        } else {
            Ty::func(fields.clone(), owner_ty)
        })
    }

    fn infer_binop(&mut self, whole: &Expr, left: &Expr, op: &BinOp, right: &Expr) -> Ty {
        let lt = self.infer(left, None);
        let rt = self.infer(right, None);
        let lt = self.cx.resolve(&lt);
        let rt = self.cx.resolve(&rt);
        let shared = match op {
            BinOp::Eq | BinOp::NotEq | BinOp::And | BinOp::Or => return Ty::Bool,
            BinOp::Add => crate::semantics::BinaryOp::Add,
            BinOp::Sub => crate::semantics::BinaryOp::Sub,
            BinOp::Mul => crate::semantics::BinaryOp::Mul,
            BinOp::Div => crate::semantics::BinaryOp::Div,
            BinOp::Mod => crate::semantics::BinaryOp::Mod,
            BinOp::Lt => crate::semantics::BinaryOp::Lt,
            BinOp::Gt => crate::semantics::BinaryOp::Gt,
            BinOp::LtEq => crate::semantics::BinaryOp::LtEq,
            BinOp::GtEq => crate::semantics::BinaryOp::GtEq,
        };
        let is_comparison = matches!(op, BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq);

        if matches!(lt, Ty::Option(_)) || matches!(rt, Ty::Option(_)) {
            // `?T` may hold a plain T at run time (annotations accept it),
            // so this is a likely bug, not a certain one.
            let span = self.expr_span(whole);
            self.report(
                Code::OptionMisuse,
                span,
                "Option type used in operation; check with is_some() first",
                Some(
                    "unwrap the value (unwrap, unwrap_or, match) or narrow it with `if x != null`"
                        .into(),
                ),
                Vec::new(),
            );
            return if is_comparison { Ty::Bool } else { Ty::Any };
        }

        // Evaluate the operator with the engines' own rules on
        // representative operands of the known types.
        match (operand_of(&lt), operand_of(&rt)) {
            (Some(l), Some(r)) => match crate::semantics::binary(shared, l, r) {
                Ok(outcome) => match outcome {
                    crate::semantics::Outcome::Int(_) => Ty::Int,
                    crate::semantics::Outcome::Float(_) => Ty::Float,
                    crate::semantics::Outcome::Bool(_) => Ty::Bool,
                    crate::semantics::Outcome::Concat => Ty::String,
                },
                Err(message) => {
                    let span = self.expr_span(whole);
                    self.report(
                        Code::InvalidOperator,
                        span,
                        format!(
                            "operator '{}' cannot be applied to {} and {}",
                            binop_symbol(op),
                            lt,
                            rt
                        ),
                        Some(format!(
                            "this fails at run time with \"{}\"",
                            message.lines().next().unwrap_or(&message)
                        )),
                        Vec::new(),
                    );
                    if is_comparison {
                        Ty::Bool
                    } else {
                        Ty::Any
                    }
                }
            },
            _ => {
                if is_comparison {
                    Ty::Bool
                } else if *op == BinOp::Add && (lt == Ty::String || rt == Ty::String) {
                    Ty::String
                } else {
                    Ty::Any
                }
            }
        }
    }

    /// Type of `ty.field` for a struct type (fields and embedded fields).
    fn struct_field(&self, ty: &Ty, field: &str) -> Option<Ty> {
        let Ty::Named(name, args) = ty else {
            return None;
        };
        self.struct_field_in(name, args, field, 0)
    }

    fn struct_field_in(&self, name: &str, args: &[Ty], field: &str, depth: usize) -> Option<Ty> {
        if depth > 8 {
            return None;
        }
        let info = self.structs.get(name)?;
        let subst: BTreeMap<String, Ty> = info
            .type_params
            .iter()
            .cloned()
            .zip(args.iter().cloned().chain(std::iter::repeat(Ty::Any)))
            .collect();
        if let Some(f) = info.fields.iter().find(|f| f.name == field) {
            return Some(f.ty.subst_params(&subst));
        }
        for f in info.fields.iter().filter(|f| f.embedded) {
            if let Ty::Named(inner, inner_args) = &f.ty {
                if let Some(t) = self.struct_field_in(inner, inner_args, field, depth + 1) {
                    return Some(t);
                }
            }
        }
        None
    }

    /// Methods of a struct (own and embedded).
    fn find_method(&self, type_name: &str, method: &str, depth: usize) -> Option<MethodInfo> {
        if depth > 8 {
            return None;
        }
        if let Some(m) = self.methods.get(type_name).and_then(|t| t.get(method)) {
            return Some(m.clone());
        }
        let info = self.structs.get(type_name)?;
        for f in info.fields.iter().filter(|f| f.embedded) {
            if let Ty::Named(inner, _) = &f.ty {
                if let Some(m) = self.find_method(inner, method, depth + 1) {
                    return Some(m);
                }
            }
        }
        None
    }

    /// Field type for destructuring and the like (structs only).
    fn field_type(&self, ty: &Ty, field: &str) -> Option<Ty> {
        self.struct_field(ty, field)
    }

    fn struct_member_names(&self, type_name: &str) -> Vec<String> {
        let mut names = Vec::new();
        if let Some(info) = self.structs.get(type_name) {
            for f in &info.fields {
                names.push(f.name.clone());
                if f.embedded {
                    if let Ty::Named(inner, _) = &f.ty {
                        names.extend(self.struct_member_names(inner));
                    }
                }
            }
        }
        if let Some(methods) = self.methods.get(type_name) {
            names.extend(methods.keys().cloned());
        }
        names.sort();
        names.dedup();
        names
    }

    fn unknown_member_fix(&self, field: &str, candidates: &[String]) -> (Option<String>, Vec<Fix>) {
        match suggest::closest(field, candidates.iter().map(String::as_str)) {
            Some(s) => {
                let span = self.name_span(field, NameClass::Field);
                (
                    Some(format!("did you mean '{}'?", s)),
                    vec![Fix {
                        title: format!("Change to '{}'", s),
                        span,
                        replacement: s.to_string(),
                    }],
                )
            }
            None => (None, Vec::new()),
        }
    }

    fn infer_field_access(&mut self, object: &Expr, field: &str) -> Ty {
        // Module member: `math.sqrt`.
        if let Expr::Ident(module) = object {
            if self.is_global_here(module) {
                if let Some(info) = builtins::module(module) {
                    let ty = if info.members.contains(field) {
                        info.typed.get(field).cloned().unwrap_or(Ty::Any)
                    } else {
                        let mut members: Vec<String> = info.members.iter().cloned().collect();
                        members.sort();
                        let (help, fixes) = self.unknown_member_fix(field, &members);
                        let span = self.name_span(field, NameClass::Field);
                        self.report(
                            Code::UnknownMember,
                            span,
                            format!("module '{}' has no member '{}'", module, field),
                            help,
                            fixes,
                        );
                        Ty::Any
                    };
                    self.record_field_type(field, &ty);
                    return ty;
                }
            }
            // Static method or constructor-ish access on a struct name.
            if self.structs.contains_key(module) && self.lookup(module).is_none() {
                let ty = self
                    .methods
                    .get(module)
                    .and_then(|m| m.get(field))
                    .map(|m| Ty::Fn(m.callable.sig.clone()))
                    .unwrap_or(Ty::Any);
                self.record_field_type(field, &ty);
                return ty;
            }
        }
        let ot = self.infer(object, None);
        let ot = self.cx.resolve(&ot);
        let ty = match &ot {
            Ty::Named(name, _) if self.structs.contains_key(name) => {
                match self.struct_field(&ot, field) {
                    Some(t) => t,
                    None => match self.find_method(name, field, 0) {
                        Some(m) => Ty::Fn(m.callable.sig),
                        None => {
                            let name = name.clone();
                            let candidates = self.struct_member_names(&name);
                            let (help, fixes) = self.unknown_member_fix(field, &candidates);
                            let span = self.name_span(field, NameClass::Field);
                            self.report(
                                Code::UnknownField,
                                span,
                                format!("struct '{}' has no field '{}'", name, field),
                                help,
                                fixes,
                            );
                            Ty::Any
                        }
                    },
                }
            }
            _ => Ty::Any,
        };
        self.record_field_type(field, &ty);
        ty
    }

    fn infer_struct_init(&mut self, name: &str, fields: &[(String, Expr)]) -> Ty {
        let Some(info) = self.structs.get(name).cloned() else {
            for (_, v) in fields {
                self.infer(v, None);
            }
            return if self.adts.contains_key(name) {
                Ty::Named(name.to_string(), Vec::new())
            } else {
                Ty::Any
            };
        };
        let subst: BTreeMap<String, Ty> = info
            .type_params
            .iter()
            .map(|p| (p.clone(), self.cx.fresh()))
            .collect();
        let known: Vec<String> = info.fields.iter().map(|f| f.name.clone()).collect();
        for (fname, value) in fields {
            match info.fields.iter().find(|f| f.name == *fname) {
                Some(f) => {
                    let expected = f.ty.subst_params(&subst);
                    let vt = self.infer(value, Some(&expected));
                    self.record_field_type(fname, &expected);
                    let span = self.expr_span_or(value, self.name_span(fname, NameClass::Field));
                    self.expect_assignable(&expected, &vt, span, |e, a| {
                        format!(
                            "type mismatch: field '{}' of '{}' is {} but given {}",
                            fname, name, e, a
                        )
                    });
                }
                None => {
                    self.infer(value, None);
                    let (help, fixes) = self.unknown_member_fix(fname, &known);
                    let span = self.name_span(fname, NameClass::Field);
                    self.report(
                        Code::UnknownField,
                        span,
                        format!("struct '{}' has no field '{}'", name, fname),
                        help,
                        fixes,
                    );
                }
            }
        }
        let missing: Vec<&str> = info
            .fields
            .iter()
            .filter(|f| !f.has_default && !fields.iter().any(|(n, _)| *n == f.name))
            .map(|f| f.name.as_str())
            .collect();
        if !missing.is_empty() {
            let span = self.name_span(name, NameClass::Type);
            self.simple(
                Code::MissingField,
                span,
                format!(
                    "struct '{}' literal is missing field{} {}",
                    name,
                    if missing.len() == 1 { "" } else { "s" },
                    missing
                        .iter()
                        .map(|m| format!("'{}'", m))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
        let args: Vec<Ty> = info
            .type_params
            .iter()
            .map(|p| self.cx.finalize(subst.get(p).unwrap_or(&Ty::Any)))
            .collect();
        Ty::Named(name.to_string(), args)
    }

    fn infer_lambda(
        &mut self,
        params: &[Param],
        body: &[SpannedStmt],
        expected: Option<&Ty>,
    ) -> Ty {
        let expected_fn = match expected.map(|e| self.cx.resolve(e)) {
            Some(Ty::Fn(f)) => Some(f),
            _ => None,
        };
        self.push_scope();
        let mut param_types = Vec::new();
        for (i, p) in params.iter().enumerate() {
            let ty = match &p.type_ann {
                Some(a) => self.lower(a),
                None => expected_fn
                    .as_ref()
                    .and_then(|f| f.params.get(i))
                    .map(|t| self.cx.resolve(t))
                    .filter(|t| !t.has_vars())
                    .unwrap_or(Ty::Any),
            };
            if let Some(default) = &p.default {
                self.infer(default, Some(&ty));
            }
            let declared = p.type_ann.as_ref().map(|_| ty.clone());
            self.bind(&p.name, ty.clone(), declared, BindKind::Other);
            param_types.push(ty);
        }
        self.frames.push(FnFrame {
            declared_ret: None,
            returns: Vec::new(),
        });
        let depth = std::mem::replace(&mut self.loop_depth, 0);
        let (tail, flow) = self.check_body(body);
        self.loop_depth = depth;
        let frame = self.frames.pop();
        self.pop_scope();
        let mut ret = Ty::Never;
        if let Some(frame) = frame {
            for r in &frame.returns {
                ret = self.cx.join(&ret, r);
            }
        }
        if flow == Flow::Normal {
            ret = self.cx.join(&ret, &tail);
        }
        if matches!(ret, Ty::Never) && flow == Flow::Normal {
            ret = Ty::Null;
        }
        Ty::Fn(FnTy {
            type_params: Vec::new(),
            params: param_types,
            required: crate::semantics::required_params(params.iter().map(|p| p.default.is_some())),
            variadic: false,
            ret: Box::new(ret),
        })
    }

    // ===================================================================
    // Calls
    // ===================================================================

    fn instantiate(&mut self, f: &FnTy) -> BTreeMap<String, Ty> {
        f.type_params
            .iter()
            .map(|p| (p.clone(), self.cx.fresh()))
            .collect()
    }

    fn infer_call(&mut self, function: &Expr, args: &[Expr]) -> Ty {
        let has_spread = args.iter().any(|a| matches!(a, Expr::Spread(_)));
        match function {
            Expr::Ident(name) => self.infer_named_call(name, args, has_spread),
            Expr::FieldAccess { object, field } => {
                self.infer_method_call(object, field, args, has_spread)
            }
            other => {
                let ft = self.infer(other, None);
                self.call_value(&ft, "fn", args, has_spread, self.expr_span(other), true)
            }
        }
    }

    fn infer_named_call(&mut self, name: &str, args: &[Expr], has_spread: bool) -> Ty {
        let span = self.name_span(name, NameClass::Value);

        // A local binding shadows everything else.
        if let Some(b) = self.lookup(name).cloned() {
            let ty = self.cx.resolve(&b.ty);
            return self.call_value(&ty, name, args, has_spread, span, true);
        }
        if let Some(c) = self.functions.get(name).cloned() {
            return self.call_signature(name, &c.sig, args, has_spread, span, CallKind::User);
        }
        if let Some(t) = self.variant_value_type(name) {
            return match t {
                Ty::Fn(f) => {
                    self.call_signature(name, &f, args, has_spread, span, CallKind::Constructor)
                }
                other => {
                    self.infer_args(args);
                    let span = self.name_span(name, NameClass::Value);
                    self.simple(
                        Code::NotCallable,
                        span,
                        format!(
                            "'{}' is a unit variant of {}, not a constructor",
                            name, other
                        ),
                    );
                    other
                }
            };
        }
        if !self.is_global_here(name) {
            // Unknown or imported without types.
            self.infer_args(args);
            return Ty::Any;
        }

        // Builtins: the registry's arity rule is what the engines enforce.
        if !has_spread {
            if let Err(message) = crate::builtins_registry::check_arity(name, args.len()) {
                self.simple(Code::Arity, span, message);
            }
        }
        match name {
            "assert_throws" => {
                // The callback is expected to fail: nothing inside it is
                // reported.
                for (i, a) in args.iter().enumerate() {
                    if i == 0 {
                        self.suppress += 1;
                        if let Expr::Lambda { body, .. } = a {
                            self.note_suppressed_region(body);
                        }
                        self.infer(a, None);
                        self.suppress -= 1;
                    } else {
                        self.infer(a, None);
                    }
                }
                Ty::Null
            }
            "Ok" | "ok" => {
                let t = args.first().map_or(Ty::Null, |a| self.infer(a, None));
                self.infer_args(args.get(1..).unwrap_or(&[]));
                Ty::result(t, Ty::Any)
            }
            "Err" | "err" => {
                let t = args.first().map_or(Ty::Null, |a| self.infer(a, None));
                self.infer_args(args.get(1..).unwrap_or(&[]));
                Ty::result(Ty::Any, t)
            }
            "unwrap" => match args.first().map(|a| self.infer(a, None)) {
                Some(t) => match self.cx.resolve(&t) {
                    Ty::Option(inner) => *inner,
                    Ty::Result(ok, _) => *ok,
                    other => other,
                },
                None => Ty::Any,
            },
            "unwrap_err" => match args.first().map(|a| self.infer(a, None)) {
                Some(t) => match self.cx.resolve(&t) {
                    Ty::Result(_, err) => *err,
                    _ => Ty::Any,
                },
                None => Ty::Any,
            },
            "unwrap_or" => {
                let t = args.first().map(|a| self.infer(a, None));
                let fallback = args.get(1).map(|a| self.infer(a, None));
                self.infer_args(args.get(2..).unwrap_or(&[]));
                match t.map(|t| self.cx.resolve(&t)) {
                    Some(Ty::Option(inner)) | Some(Ty::Result(inner, _)) => {
                        if let Some(fb) = fallback {
                            let fb = self.cx.resolve(&fb);
                            let snap = self.cx.snapshot();
                            let fits = self.cx.assign(&inner, &fb);
                            self.cx.rollback(snap);
                            if !fits && !inner.is_unknown() {
                                let span = args.get(1).map_or(span, |a| self.expr_span(a));
                                self.simple(
                                    Code::TypeMismatch,
                                    span,
                                    format!(
                                        "unwrap_or fallback type {} incompatible with Option inner type {}",
                                        fb, inner
                                    ),
                                );
                            }
                        }
                        *inner
                    }
                    Some(other) => other,
                    None => Ty::Any,
                }
            }
            "map" if args.len() != 2 => {
                self.infer_args(args);
                Ty::Map(Box::new(Ty::Any), Box::new(Ty::Any))
            }
            "map" => {
                let sig = FnTy {
                    type_params: vec!["T".into(), "U".into()],
                    params: vec![
                        Ty::array(Ty::Param("T".into())),
                        Ty::func(vec![Ty::Param("T".into())], Ty::Param("U".into())),
                    ],
                    required: 2,
                    variadic: false,
                    ret: Box::new(Ty::array(Ty::Param("U".into()))),
                };
                self.call_signature(name, &sig, args, true, span, CallKind::Builtin)
            }
            "reduce" if args.len() == 3 => {
                let at = self.infer(&args[0], None);
                let init = self.infer(&args[1], None);
                let elem = match self.cx.resolve(&at) {
                    Ty::Array(e) => *e,
                    _ => Ty::Any,
                };
                let acc = self.cx.resolve(&init);
                let ft = self.infer(&args[2], Some(&Ty::func(vec![acc.clone(), elem], Ty::Any)));
                match self.cx.resolve(&ft) {
                    Ty::Fn(f) => self.cx.join(&acc, &f.ret),
                    _ => Ty::Any,
                }
            }
            "sum" => {
                let t = args.first().map(|a| self.infer(a, None));
                self.infer_args(args.get(1..).unwrap_or(&[]));
                match t.map(|t| self.cx.resolve(&t)) {
                    Some(Ty::Array(e)) if matches!(*e, Ty::Int | Ty::Float) => *e,
                    _ => Ty::Any,
                }
            }
            "reverse" | "slice" | "shuffle" | "sample" | "sus" => {
                let t = args.first().map(|a| self.infer(a, None));
                self.infer_args(args.get(1..).unwrap_or(&[]));
                match (name, t) {
                    ("reverse" | "slice" | "shuffle" | "sus", Some(t)) => t,
                    _ => Ty::Any,
                }
            }
            "keys" => {
                let t = args.first().map(|a| self.infer(a, None));
                match t.map(|t| self.cx.resolve(&t)) {
                    Some(Ty::Map(k, _)) => Ty::array(*k),
                    Some(Ty::Object) | Some(Ty::Named(..)) => Ty::array(Ty::String),
                    _ => Ty::array(Ty::Any),
                }
            }
            "values" => {
                let t = args.first().map(|a| self.infer(a, None));
                match t.map(|t| self.cx.resolve(&t)) {
                    Some(Ty::Map(_, v)) => Ty::array(*v),
                    _ => Ty::array(Ty::Any),
                }
            }
            _ => match builtins::function(name) {
                Some(sig) => {
                    let sig = sig.clone();
                    self.call_signature(name, &sig, args, true, span, CallKind::Builtin)
                }
                None => {
                    self.infer_args(args);
                    Ty::Any
                }
            },
        }
    }

    fn note_suppressed_region(&mut self, body: &[SpannedStmt]) {
        let (Some(ix), Some(first)) = (self.index, body.first()) else {
            return;
        };
        if first.line == 0 {
            return;
        }
        let pos = Pos::new(first.line, first.col);
        // The innermost lambda scope around the body.
        let mut best: Option<Span> = None;
        for scope in &ix.scopes {
            if scope.kind == ScopeKind::Lambda && scope.start <= pos && pos <= scope.end {
                let span = Span::new(scope.start, scope.end);
                if best.is_none_or(|b| span.start >= b.start) {
                    best = Some(span);
                }
            }
        }
        if let Some(span) = best {
            if !self.mute {
                self.suppressed_regions.push(span);
            }
        }
    }

    fn infer_args(&mut self, args: &[Expr]) {
        for a in args {
            self.infer(a, None);
        }
    }

    /// Call a value of type `ty` (a variable or expression).
    fn call_value(
        &mut self,
        ty: &Ty,
        name: &str,
        args: &[Expr],
        has_spread: bool,
        span: Span,
        check_arity: bool,
    ) -> Ty {
        match ty {
            Ty::Fn(f) => {
                let kind = if check_arity {
                    CallKind::Value
                } else {
                    CallKind::Builtin
                };
                let f = f.clone();
                self.call_signature(name, &f, args, has_spread, span, kind)
            }
            Ty::Int
            | Ty::Float
            | Ty::String
            | Ty::Bool
            | Ty::Null
            | Ty::Array(_)
            | Ty::Tuple(_) => {
                self.infer_args(args);
                self.simple(
                    Code::NotCallable,
                    span,
                    format!("'{}' is {}, not a function", name, ty),
                );
                Ty::Any
            }
            _ => {
                self.infer_args(args);
                Ty::Any
            }
        }
    }

    /// Check a call against a signature and return the result type.
    fn call_signature(
        &mut self,
        name: &str,
        sig: &FnTy,
        args: &[Expr],
        has_spread: bool,
        span: Span,
        kind: CallKind,
    ) -> Ty {
        let subst = self.instantiate(sig);
        let params: Vec<Ty> = sig.params.iter().map(|p| p.subst_params(&subst)).collect();

        if !has_spread
            && matches!(
                kind,
                CallKind::User | CallKind::Value | CallKind::Constructor
            )
        {
            let result = if kind == CallKind::Constructor {
                if args.len() == params.len() {
                    Ok(())
                } else {
                    Err(format!(
                        "{} takes {} field{}, got {}",
                        name,
                        params.len(),
                        if params.len() == 1 { "" } else { "s" },
                        args.len()
                    ))
                }
            } else {
                crate::semantics::check_call_arity(name, params.len(), sig.required, args.len())
            };
            if let Err(message) = result {
                self.simple(Code::Arity, span, message);
            }
        }

        let param_for = |i: usize| -> Option<Ty> {
            match params.get(i) {
                Some(p) => Some(p.clone()),
                None if sig.variadic => params.last().cloned(),
                None => None,
            }
        };

        // Non-lambda arguments first, so their types reach the lambdas.
        let mut arg_types: Vec<Option<Ty>> = vec![None; args.len()];
        for pass in 0..2 {
            for (i, arg) in args.iter().enumerate() {
                let is_lambda = matches!(arg, Expr::Lambda { .. });
                if (pass == 0) == is_lambda {
                    continue;
                }
                let expected = param_for(i).map(|p| self.cx.resolve(&p));
                let at = self.infer(arg, expected.as_ref());
                if let Some(expected) = expected {
                    if kind == CallKind::Builtin {
                        // Solve type variables; never report.
                        let snap = self.cx.snapshot();
                        if !self.cx.assign(&expected, &at) {
                            self.cx.rollback(snap);
                        }
                    } else if !matches!(arg, Expr::Spread(_)) {
                        let arg_span = self.expr_span_or(arg, span);
                        let n = i + 1;
                        self.expect_assignable_code(
                            Code::ArgumentType,
                            &expected,
                            &at,
                            arg_span,
                            |e, a| {
                                format!(
                                    "argument {} of '{}': expected {} but got {}",
                                    n, name, e, a
                                )
                            },
                        );
                    }
                }
                arg_types[i] = Some(at);
            }
        }
        let ret = sig.ret.subst_params(&subst);
        let ret = self.cx.resolve(&ret);
        // Unsolved type variables in the result mean "unknown".
        self.cx.finalize(&ret)
    }

    fn infer_method_call(
        &mut self,
        object: &Expr,
        method: &str,
        args: &[Expr],
        has_spread: bool,
    ) -> Ty {
        let span = self.name_span(method, NameClass::Field);
        // Module function: `math.sqrt(x)`.
        if let Expr::Ident(module) = object {
            if self.is_global_here(module) && builtins::module(module).is_some() {
                let ft = self.infer_field_access(object, method);
                return match ft {
                    Ty::Fn(f) => {
                        self.call_signature(method, &f, args, has_spread, span, CallKind::Builtin)
                    }
                    _ => {
                        self.infer_args(args);
                        Ty::Any
                    }
                };
            }
            // Static method: `Point.new(1, 2)`.
            if self.structs.contains_key(module) && self.lookup(module).is_none() {
                let info = self
                    .methods
                    .get(module)
                    .and_then(|m| m.get(method))
                    .cloned();
                self.record_field_type(
                    method,
                    &info
                        .as_ref()
                        .map_or(Ty::Any, |i| Ty::Fn(i.callable.sig.clone())),
                );
                return match info {
                    Some(info) if !info.has_receiver => {
                        let label = format!("{}.{}", module, method);
                        self.call_signature(
                            &label,
                            &info.callable.sig,
                            args,
                            has_spread,
                            span,
                            CallKind::User,
                        )
                    }
                    _ => {
                        self.infer_args(args);
                        Ty::Any
                    }
                };
            }
        }

        let ot = self.infer(object, None);
        let ot = self.cx.resolve(&ot);
        if let Ty::Named(type_name, _) = &ot {
            let type_name = type_name.clone();
            if let Some(m) = self.find_method(&type_name, method, 0) {
                self.record_field_type(method, &Ty::Fn(m.callable.sig.clone()));
                if !has_spread {
                    // Called on an instance, the first parameter receives
                    // the instance whether or not it is named `it`.
                    let params = m.callable.sig.params.len();
                    let required = m.callable.sig.required;
                    if let Err(message) =
                        crate::semantics::check_method_arity(method, params, required, args.len())
                    {
                        self.simple(Code::Arity, span, message);
                    }
                }
                // Check the explicit arguments against the parameters after
                // the receiver.
                let mut sig = m.callable.sig.clone();
                if !sig.params.is_empty() {
                    sig.params.remove(0);
                }
                sig.required = sig.required.saturating_sub(1);
                return self.call_signature(method, &sig, args, true, span, CallKind::User);
            }
            if self.structs.contains_key(&type_name) {
                if let Some(ft) = self.struct_field(&ot, method) {
                    self.record_field_type(method, &ft);
                    let ft = self.cx.resolve(&ft);
                    return self.call_value(&ft, method, args, has_spread, span, true);
                }
                let is_builtin_method = crate::builtins_registry::global(method).is_some()
                    || EXTRA_VALUE_METHODS.contains(&method);
                if !is_builtin_method {
                    let candidates = self.struct_member_names(&type_name);
                    let (help, fixes) = self.unknown_member_fix(method, &candidates);
                    self.report(
                        Code::UnknownField,
                        span,
                        format!("struct '{}' has no method or field '{}'", type_name, method),
                        help,
                        fixes,
                    );
                    self.infer_args(args);
                    return Ty::Any;
                }
            }
        }

        // Builtin used as a method: `xs.map(f)` is `map(xs, f)`.
        if let Some(sig) = builtins::function(method).cloned() {
            if !matches!(ot, Ty::Object | Ty::Named(..))
                || crate::builtins_registry::global(method).is_some()
            {
                let subst = self.instantiate(&sig);
                if let Some(first) = sig.params.first() {
                    let first = first.subst_params(&subst);
                    let snap = self.cx.snapshot();
                    if !self.cx.assign(&first, &ot) {
                        self.cx.rollback(snap);
                    }
                }
                let rest = FnTy {
                    type_params: Vec::new(),
                    params: sig
                        .params
                        .iter()
                        .skip(1)
                        .map(|p| p.subst_params(&subst))
                        .collect(),
                    required: sig.required.saturating_sub(1),
                    variadic: sig.variadic,
                    ret: Box::new(sig.ret.subst_params(&subst)),
                };
                self.record_field_type(method, &Ty::Any);
                return self.call_signature(method, &rest, args, true, span, CallKind::Builtin);
            }
        }
        self.record_field_type(method, &Ty::Any);
        if method == "map" {
            if let Ty::Array(elem) = &ot {
                let elem = *elem.clone();
                let mut ret = Ty::Any;
                for a in args {
                    let t = self.infer(a, Some(&Ty::func(vec![elem.clone()], Ty::Any)));
                    if let Ty::Fn(f) = self.cx.resolve(&t) {
                        ret = *f.ret;
                    }
                }
                return Ty::array(ret);
            }
        }
        self.infer_args(args);
        Ty::Any
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallKind {
    /// A user-declared function or method: arity and argument types are
    /// checked against the declaration.
    User,
    /// A function-typed value (lambda, parameter): same checks.
    Value,
    /// An ADT variant constructor.
    Constructor,
    /// A builtin: the signature only types results and callbacks.
    Builtin,
}

/// Representative operand for evaluating an operator with the engines'
/// rules (`semantics::binary`). `None` when the type is not fixed.
fn operand_of(ty: &Ty) -> Option<crate::semantics::Operand<'static>> {
    use crate::semantics::Operand;
    Some(match ty {
        Ty::Int => Operand::Int(1),
        Ty::Float => Operand::Float(1.0),
        Ty::String => Operand::Str("s"),
        Ty::Bool => Operand::Bool,
        Ty::Null => Operand::Null,
        // A struct instance is an object at run time.
        Ty::Named(..) | Ty::Object => Operand::Other("Object"),
        other => Operand::Other(other.runtime_name()?),
    })
}

fn binop_symbol(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::NotEq => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::LtEq => "<=",
        BinOp::GtEq => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

/// Does the last statement of a body produce the function's value?
fn tail_produces_value(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Expression(_) | Stmt::Return(_) => true,
        Stmt::If {
            then_body,
            else_body: Some(else_body),
            ..
        } => body_produces_value(then_body) && body_produces_value(else_body),
        Stmt::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|a| body_produces_value(&a.body))
        }
        Stmt::When { arms, .. } => arms.iter().any(|a| a.is_else),
        Stmt::TryCatch {
            try_body,
            catch_body,
            ..
        } => body_produces_value(try_body) && body_produces_value(catch_body),
        Stmt::Loop { body } => !contains_break(body),
        Stmt::While { condition, body } => {
            matches!(condition, Expr::Bool(true)) && !contains_break(body)
        }
        _ => false,
    }
}

fn body_produces_value(body: &[SpannedStmt]) -> bool {
    body.iter().any(|s| matches!(s.stmt, Stmt::Return(_)))
        || body.last().is_some_and(|s| tail_produces_value(&s.stmt))
}

/// Does a loop body contain a `break` for this loop (not a nested loop's,
/// and not inside a lambda)?
fn contains_break(body: &[SpannedStmt]) -> bool {
    body.iter().any(|s| stmt_breaks(&s.stmt))
}

fn stmt_breaks(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Break => true,
        Stmt::If {
            then_body,
            else_body,
            ..
        } => contains_break(then_body) || else_body.as_deref().is_some_and(contains_break),
        Stmt::Match { arms, .. } => arms.iter().any(|a| contains_break(&a.body)),
        Stmt::TryCatch {
            try_body,
            catch_body,
            ..
        } => contains_break(try_body) || contains_break(catch_body),
        Stmt::SafeBlock { body }
        | Stmt::TimeoutBlock { body, .. }
        | Stmt::RetryBlock { body, .. } => contains_break(body),
        Stmt::Expression(Expr::Block(body)) => contains_break(body),
        // Nested loops own their breaks; spawned bodies and functions too.
        _ => false,
    }
}

/// Names in an expression, in source order, with their namespace.
fn collect_expr_names(expr: &Expr, out: &mut Vec<(String, NameClass)>) {
    match expr {
        Expr::Ident(n) => out.push((n.clone(), NameClass::Value)),
        Expr::BinOp { left, right, .. } => {
            collect_expr_names(left, out);
            collect_expr_names(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_expr_names(operand, out),
        Expr::FieldAccess { object, field } => {
            collect_expr_names(object, out);
            out.push((field.clone(), NameClass::Field));
        }
        Expr::Index { object, index } => {
            collect_expr_names(object, out);
            collect_expr_names(index, out);
        }
        Expr::Call { function, args } => {
            collect_expr_names(function, out);
            for a in args {
                collect_expr_names(a, out);
            }
        }
        Expr::Array(items) | Expr::Tuple(items) => {
            for i in items {
                collect_expr_names(i, out);
            }
        }
        Expr::Object(fields) => {
            for (k, v) in fields {
                out.push((k.clone(), NameClass::Field));
                collect_expr_names(v, out);
            }
        }
        Expr::StructInit { name, fields } => {
            out.push((name.clone(), NameClass::Type));
            for (k, v) in fields {
                out.push((k.clone(), NameClass::Field));
                collect_expr_names(v, out);
            }
        }
        Expr::Try(e)
        | Expr::Await(e)
        | Expr::Must(e)
        | Expr::Freeze(e)
        | Expr::Ask(e)
        | Expr::Spread(e) => collect_expr_names(e, out),
        Expr::Pipeline { value, function } => {
            collect_expr_names(value, out);
            collect_expr_names(function, out);
        }
        Expr::StringInterp(parts) => {
            for p in parts {
                if let StringPart::Expr(e) = p {
                    collect_expr_names(e, out);
                }
            }
        }
        Expr::WhereFilter {
            source,
            field,
            value,
            ..
        } => {
            collect_expr_names(source, out);
            out.push((field.clone(), NameClass::Field));
            collect_expr_names(value, out);
        }
        _ => {}
    }
}
