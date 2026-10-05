//! Syntax index: where every name in a source file is, and which lexical
//! scope it lives in.
//!
//! The AST deliberately carries positions only on statements (expressions
//! are position-free, and both engines pattern-match on them), so tools that
//! need to point at an *identifier* — diagnostics with precise spans, hover,
//! go-to-definition, rename — cannot get that from the tree alone. Instead,
//! a parser built with [`super::Parser::with_index`] records, while it
//! parses:
//!
//! * every name occurrence ([`NameOcc`]): its text, exact span, and its
//!   [`Role`] — a definition (with a [`DefKind`]), a value reference, a type
//!   reference or a field/key name;
//! * every lexical scope ([`Scope`]) the language creates: the file,
//!   function and lambda parameter lists, `{ }` blocks, loop variables,
//!   `match` arms, `catch` clauses and struct bodies.
//!
//! The parser records only what it consumed from a token, so the index is
//! exact by construction: desugared names (`say` → `println`, `x += 1` →
//! `x = x + 1`) appear once, at the token the user wrote.
//!
//! The index is pure syntax. Name *resolution* (which definition a reference
//! means) is a language rule and lives in `typechecker::resolve`.

/// A source position: 1-based line and 1-based column counted in `char`s
/// (the lexer's convention). Ordering is source order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl Pos {
    pub const fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// A half-open range `[start, end)` of source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}

impl Span {
    pub const fn new(start: Pos, end: Pos) -> Self {
        Self { start, end }
    }

    /// `pos` lies inside the span, or exactly at its end (a cursor placed
    /// just after a name still targets that name).
    pub fn touches(&self, pos: Pos) -> bool {
        self.start <= pos && pos <= self.end
    }
}

pub type ScopeId = usize;
pub type OccId = usize;

/// What introduced a lexical scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    File,
    /// Parameters (and type parameters) of a named function, prompt or agent.
    Function,
    /// Parameters of a `fn(...) { }` lambda.
    Lambda,
    /// A `{ ... }` statement block.
    Block,
    /// Loop variables of a `for` statement.
    Loop,
    /// Bindings of one `match` arm.
    Arm,
    /// The error binding of a `catch` clause.
    Catch,
    /// Type parameters and fields of a `struct` / `thing`.
    Struct,
    /// Parameters of an interface method signature.
    Signature,
}

impl ScopeKind {
    /// Scopes whose body runs later, when called: references inside may see
    /// definitions that come after them in the enclosing scope.
    pub fn is_function_boundary(self) -> bool {
        matches!(self, ScopeKind::Function | ScopeKind::Lambda)
    }
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub kind: ScopeKind,
    pub parent: Option<ScopeId>,
    pub start: Pos,
    pub end: Pos,
}

/// The kind of thing a definition introduces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefKind {
    /// `let` / `set` / `grab` / destructuring.
    Variable {
        mutable: bool,
    },
    Function,
    Parameter,
    TypeParameter,
    /// `for x in ...` / `for k, v in ...`.
    LoopVariable,
    /// A name bound by a `match` pattern (may turn out to be a unit
    /// variant reference; see `typechecker::resolve`).
    PatternBinding,
    CatchVariable,
    /// A name brought in by `import { name } from "path"`.
    Import {
        path: String,
    },
    /// A name bound by `import native "lib" [as name]` or
    /// `import { f } from native "lib"` (a plugin namespace or function).
    NativeImport {
        path: String,
    },
    Struct,
    Interface,
    /// `type Name = ...` (an algebraic data type or a union alias).
    TypeDef,
    /// A variant of a `type` definition (a constructor value).
    Variant {
        owner: String,
    },
    /// A field declared in a struct body.
    Field {
        owner: String,
    },
    /// A method in an `impl`/`give` block or an interface.
    Method {
        owner: String,
    },
    /// `prompt name(...) { }` and `agent name(...) { }` (callable).
    Callable,
}

impl DefKind {
    /// Definitions that live in the value namespace (can be read as a
    /// variable). Fields and methods are reached through a receiver.
    pub fn is_value(&self) -> bool {
        // Interfaces are values too: `satisfies(x, Printable)`.
        !matches!(
            self,
            DefKind::Field { .. } | DefKind::Method { .. } | DefKind::TypeParameter
        )
    }

    /// Definitions that name a type.
    pub fn is_type(&self) -> bool {
        matches!(
            self,
            DefKind::Struct
                | DefKind::Interface
                | DefKind::TypeDef
                | DefKind::TypeParameter
                | DefKind::Import { .. }
        )
    }
}

/// How a name occurrence is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    Def(DefKind),
    /// A read of (or assignment to) a variable or function.
    Ref,
    /// A type name in an annotation, struct literal or `impl` header.
    TypeRef,
    /// A field or method name after `.`, an object/struct literal key, or a
    /// query field (`where age > 3`, `sort by name`).
    Field,
}

#[derive(Debug, Clone)]
pub struct NameOcc {
    pub name: String,
    pub span: Span,
    pub role: Role,
    /// Innermost scope at the occurrence.
    pub scope: ScopeId,
    /// For definitions: the first position at which the name is in scope
    /// (after the whole `let` statement, at the start of a function's
    /// parameter list, ...). Equals `span.start` for other roles.
    pub visible_from: Pos,
    /// Start of the innermost statement containing the occurrence.
    pub stmt: Pos,
}

impl NameOcc {
    pub fn is_def(&self) -> bool {
        matches!(self.role, Role::Def(_))
    }

    pub fn def_kind(&self) -> Option<&DefKind> {
        match &self.role {
            Role::Def(kind) => Some(kind),
            _ => None,
        }
    }
}

/// The names and scopes of one parsed source file.
#[derive(Debug, Clone, Default)]
pub struct SyntaxIndex {
    pub occurrences: Vec<NameOcc>,
    pub scopes: Vec<Scope>,
}

impl SyntaxIndex {
    /// The occurrence under `pos`, if any. When two occurrences touch the
    /// position (`a.b` with the cursor between them), the one that starts
    /// at or before the cursor and ends after it wins.
    pub fn occurrence_at(&self, pos: Pos) -> Option<OccId> {
        let mut best: Option<OccId> = None;
        for (id, occ) in self.occurrences.iter().enumerate() {
            if occ.span.touches(pos) {
                match best {
                    Some(b) if self.occurrences[b].span.end > pos => {}
                    _ => best = Some(id),
                }
            }
        }
        best
    }

    /// The chain of scopes from `scope` out to the file scope.
    pub fn scope_chain(&self, scope: ScopeId) -> impl Iterator<Item = ScopeId> + '_ {
        let mut next = Some(scope);
        std::iter::from_fn(move || {
            let current = next?;
            next = self.scopes.get(current).and_then(|s| s.parent);
            Some(current)
        })
    }
}

/// Builds a [`SyntaxIndex`] while the parser runs.
#[derive(Debug)]
pub(crate) struct IndexBuilder {
    index: SyntaxIndex,
    scope_stack: Vec<ScopeId>,
    stmt_stack: Vec<Pos>,
}

impl IndexBuilder {
    pub(crate) fn new() -> Self {
        Self {
            index: SyntaxIndex {
                occurrences: Vec::new(),
                scopes: vec![Scope {
                    kind: ScopeKind::File,
                    parent: None,
                    start: Pos::new(1, 1),
                    end: Pos::new(usize::MAX, usize::MAX),
                }],
            },
            scope_stack: vec![0],
            stmt_stack: Vec::new(),
        }
    }

    fn current_scope(&self) -> ScopeId {
        self.scope_stack.last().copied().unwrap_or(0)
    }

    fn current_stmt(&self) -> Pos {
        self.stmt_stack.last().copied().unwrap_or_default()
    }

    pub(crate) fn open_scope(&mut self, kind: ScopeKind, start: Pos) -> ScopeId {
        let id = self.index.scopes.len();
        self.index.scopes.push(Scope {
            kind,
            parent: Some(self.current_scope()),
            start,
            end: start,
        });
        self.scope_stack.push(id);
        id
    }

    pub(crate) fn close_scope(&mut self, end: Pos) {
        if self.scope_stack.len() > 1 {
            if let Some(id) = self.scope_stack.pop() {
                self.index.scopes[id].end = end;
            }
        }
    }

    pub(crate) fn push_stmt(&mut self, start: Pos) {
        self.stmt_stack.push(start);
    }

    pub(crate) fn pop_stmt(&mut self) {
        self.stmt_stack.pop();
    }

    fn push(&mut self, name: &str, span: Span, role: Role) -> OccId {
        let id = self.index.occurrences.len();
        self.index.occurrences.push(NameOcc {
            name: name.to_string(),
            span,
            role,
            scope: self.current_scope(),
            visible_from: span.start,
            stmt: self.current_stmt(),
        });
        id
    }

    /// Record a definition. It is visible from `visible_from` on (callers
    /// that only know it later use [`IndexBuilder::set_visible_from`]).
    pub(crate) fn def(&mut self, name: &str, span: Span, kind: DefKind) -> OccId {
        self.push(name, span, Role::Def(kind))
    }

    pub(crate) fn set_visible_from(&mut self, occ: OccId, pos: Pos) {
        if let Some(o) = self.index.occurrences.get_mut(occ) {
            o.visible_from = pos;
        }
    }

    /// Make a definition visible from the start of its scope (functions'
    /// parameters, catch variables, ...).
    pub(crate) fn hoist_to_scope(&mut self, occ: OccId) {
        if let Some(o) = self.index.occurrences.get(occ) {
            let start = self.index.scopes[o.scope].start;
            self.set_visible_from(occ, start);
        }
    }

    pub(crate) fn reference(&mut self, name: &str, span: Span, role: Role) -> OccId {
        self.push(name, span, role)
    }

    /// Merge the index of a nested parse (a string interpolation) into this
    /// one, translating its positions with `map` and re-parenting its file
    /// scope onto the current scope.
    pub(crate) fn merge_nested(&mut self, nested: SyntaxIndex, map: &dyn Fn(Pos) -> Pos) {
        let parent = self.current_scope();
        let stmt = self.current_stmt();
        let base = self.index.scopes.len();
        // Scope 0 of the nested index maps onto `parent`; the rest are
        // appended in order.
        let remap = |id: ScopeId| if id == 0 { parent } else { base + id - 1 };
        for scope in nested.scopes.iter().skip(1) {
            self.index.scopes.push(Scope {
                kind: scope.kind,
                parent: scope.parent.map(remap),
                start: map(scope.start),
                end: map(scope.end),
            });
        }
        for occ in nested.occurrences {
            self.index.occurrences.push(NameOcc {
                span: Span::new(map(occ.span.start), map(occ.span.end)),
                visible_from: map(occ.visible_from),
                scope: remap(occ.scope),
                stmt,
                ..occ
            });
        }
    }

    pub(crate) fn finish(mut self, end: Pos) -> SyntaxIndex {
        self.index.scopes[0].end = end;
        self.index
    }
}
