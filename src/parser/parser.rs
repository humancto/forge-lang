use super::ast::*;
use super::index::{DefKind, IndexBuilder, OccId, Pos, Role, ScopeKind, Span, SyntaxIndex};
/// Forge Parser — Recursive Descent
/// Converts a token stream into an AST.
/// Expression parsing uses Pratt parsing for correct precedence.
use crate::lexer::token::{Spanned, Token};
use crate::lexer::Lexer;

pub struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
    /// Current nesting of recursive productions (see [`MAX_NESTING`]).
    depth: usize,
    /// Present when the caller asked for a [`SyntaxIndex`]
    /// ([`Parser::with_index`]); every recording helper is a no-op otherwise.
    index: Option<IndexBuilder>,
    /// Source text (as chars, matching token offsets), kept for indexing so
    /// names inside string interpolations get exact columns.
    source: Option<Vec<char>>,
    /// Set while parsing the methods of an `impl`/`give` block: function
    /// definitions there are methods of this type, not variables.
    impl_owner: Option<String>,
}

/// Deepest nesting of expressions, statements, patterns and type
/// annotations the parser accepts. Untrusted source (e.g. `((((...` from an
/// agent) must produce a parse error, never a native stack overflow that
/// aborts the host; this also bounds the recursion of every later pass.
pub const MAX_NESTING: usize = 1000;

impl Parser {
    pub fn new(tokens: Vec<Spanned>) -> Self {
        Self {
            tokens,
            pos: 0,
            depth: 0,
            index: None,
            source: None,
            impl_owner: None,
        }
    }

    /// A parser that also builds a [`SyntaxIndex`] of every name and scope
    /// (see `parser::index`). `source` must be the text `tokens` were lexed
    /// from. Retrieve the index with [`Parser::take_index`] after parsing.
    pub fn with_index(tokens: Vec<Spanned>, source: &str) -> Self {
        let mut parser = Self::new(tokens);
        parser.index = Some(IndexBuilder::new());
        parser.source = Some(source.chars().collect());
        parser
    }

    /// The index built so far (complete after a successful
    /// [`Parser::parse_program`]). `None` unless built [`Parser::with_index`].
    pub fn take_index(&mut self) -> Option<SyntaxIndex> {
        let end = self.tokens.last().map(|t| Pos::new(t.line, t.col));
        self.index.take().map(|b| b.finish(end.unwrap_or_default()))
    }

    // ========== Syntax index recording ==========
    //
    // All of these are no-ops unless the parser was built `with_index`.

    fn token_span(&self, i: usize) -> Span {
        match self.tokens.get(i) {
            Some(t) => Span::new(
                Pos::new(t.line, t.col),
                Pos::new(t.line, t.col + t.len.max(1)),
            ),
            None => Span::default(),
        }
    }

    /// Span of the token just consumed.
    fn prev_span(&self) -> Span {
        self.token_span(self.pos.saturating_sub(1))
    }

    fn cur_start(&self) -> Pos {
        self.token_span(self.pos).start
    }

    fn prev_end(&self) -> Pos {
        self.prev_span().end
    }

    /// Record the token just consumed as `name` in `role`.
    fn note_prev(&mut self, name: &str, role: Role) -> Option<OccId> {
        let span = self.prev_span();
        self.index.as_mut().map(|ix| ix.reference(name, span, role))
    }

    /// Record the token just consumed as a definition.
    fn note_def(&mut self, name: &str, kind: DefKind) -> Option<OccId> {
        let span = self.prev_span();
        self.index.as_mut().map(|ix| ix.def(name, span, kind))
    }

    fn set_visible(&mut self, occ: Option<OccId>, pos: Pos) {
        if let (Some(ix), Some(occ)) = (self.index.as_mut(), occ) {
            ix.set_visible_from(occ, pos);
        }
    }

    fn hoist(&mut self, occ: Option<OccId>) {
        if let (Some(ix), Some(occ)) = (self.index.as_mut(), occ) {
            ix.hoist_to_scope(occ);
        }
    }

    fn open_scope(&mut self, kind: ScopeKind, start: Pos) {
        if let Some(ix) = self.index.as_mut() {
            ix.open_scope(kind, start);
        }
    }

    fn close_scope(&mut self) {
        let end = self.prev_end();
        if let Some(ix) = self.index.as_mut() {
            ix.close_scope(end);
        }
    }

    /// Run `f` inside a scope opened at `start`; the scope is closed (at the
    /// end of the last consumed token) whether `f` succeeds or not.
    fn scoped<T>(
        &mut self,
        kind: ScopeKind,
        start: Pos,
        f: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        self.open_scope(kind, start);
        let result = f(self);
        self.close_scope();
        result
    }

    /// Enter one level of a recursive production. Pair with
    /// [`Parser::leave`] (see [`Parser::nested`]).
    fn enter(&mut self) -> Result<(), ParseError> {
        if self.depth >= MAX_NESTING || crate::runtime::recursion::native_stack_exhausted() {
            return Err(self.error("code is nested too deeply"));
        }
        self.depth += 1;
        Ok(())
    }

    fn nested<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        self.enter()?;
        let result = f(self);
        self.depth -= 1;
        result
    }

    /// Return (line, col) of the current token position.
    fn current_pos(&self) -> (usize, usize) {
        if self.pos < self.tokens.len() {
            (self.tokens[self.pos].line, self.tokens[self.pos].col)
        } else {
            (0, 0)
        }
    }

    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut statements = Vec::new();
        self.skip_newlines();

        while !self.is_at_end() {
            let (line, col) = self.current_pos();
            let stmt = self.parse_statement()?;
            statements.push(SpannedStmt::new(stmt, line, col));
            self.skip_newlines();
        }

        Ok(Program { statements })
    }

    // ========== Statement Parsing ==========

    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        self.nested(Self::parse_statement_inner)
    }

    fn parse_statement_inner(&mut self) -> Result<Stmt, ParseError> {
        self.skip_newlines();
        let start = self.cur_start();
        if let Some(ix) = self.index.as_mut() {
            ix.push_stmt(start);
        }
        let result = self.parse_statement_kind();
        if let Some(ix) = self.index.as_mut() {
            ix.pop_stmt();
        }
        result
    }

    /// Parse a statement that is not reached through
    /// [`Parser::parse_statement`] (an `else if`, an `impl` method) so the
    /// index still attributes its names to it.
    fn as_statement(
        &mut self,
        start: Pos,
        f: impl FnOnce(&mut Self) -> Result<Stmt, ParseError>,
    ) -> Result<Stmt, ParseError> {
        if let Some(ix) = self.index.as_mut() {
            ix.push_stmt(start);
        }
        let result = f(self);
        if let Some(ix) = self.index.as_mut() {
            ix.pop_stmt();
        }
        result
    }

    fn parse_statement_kind(&mut self) -> Result<Stmt, ParseError> {
        match self.current_token() {
            Token::Let => self.parse_let(),
            // `set` is context-sensitive: `set(...)` is the set() constructor call,
            // while `set name to value` / `set mut name to value` is the natural-language
            // assignment statement. Peek ahead to disambiguate.
            Token::Set if matches!(self.peek_token(1), Token::LParen) => {
                self.parse_expr_or_assign()
            }
            Token::Set => self.parse_set(),
            Token::Change => self.parse_change(),
            Token::Fn | Token::Define => self.parse_fn_def(Vec::new()),
            Token::Type if self.is_type_definition_start() => self.parse_type_def(),
            Token::Interface | Token::Power => self.parse_interface_def(),
            Token::Struct | Token::Thing => self.parse_struct_def(),
            Token::Impl | Token::Give => self.parse_impl_block(),
            Token::Return => self.parse_return(),
            Token::If => self.parse_if(),
            Token::Match => self.parse_match(),
            Token::For => self.parse_for(),
            Token::While => self.parse_while(),
            Token::Loop => self.parse_loop(),
            Token::Repeat => self.parse_repeat(),
            Token::Break => {
                self.advance();
                Ok(Stmt::Break)
            }
            Token::Continue => {
                self.advance();
                Ok(Stmt::Continue)
            }
            Token::Spawn => self.parse_spawn(),
            Token::Squad => self.parse_squad(),
            Token::At => self.parse_decorator_or_fn(),
            Token::Say | Token::Yell | Token::Whisper => self.parse_say_yell_whisper(),
            Token::Grab => self.parse_grab(),
            Token::Wait => self.parse_wait(),
            Token::TryKw => self.parse_try_catch(),
            Token::Import => self.parse_import(),
            Token::Async | Token::ForgeKw => self.parse_fn_def(Vec::new()),
            Token::Yield | Token::Emit => self.parse_yield(),
            Token::Unpack => self.parse_unpack(),
            Token::When => self.parse_when(),
            Token::Check => self.parse_check(),
            Token::Safe => self.parse_safe_block(),
            Token::Timeout => self.parse_timeout(),
            Token::Retry => self.parse_retry(),
            Token::Schedule => self.parse_schedule(),
            Token::Watch => self.parse_watch(),
            Token::Prompt => self.parse_prompt_def(),
            Token::Agent => self.parse_agent_def(),
            Token::Download => self.parse_download(),
            Token::Crawl => self.parse_crawl(),
            _ => self.parse_expr_or_assign(),
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Let)?;

        let mutable = if self.check(&Token::Mut) {
            self.advance();
            true
        } else {
            false
        };

        // Tuple destructuring: let (a, b, c) = expr
        if self.check(&Token::LParen) {
            self.advance();
            let mut names = Vec::new();
            let mut defs = Vec::new();
            while !self.check(&Token::RParen) {
                let name = self.expect_ident()?;
                defs.push(self.note_def(&name, DefKind::Variable { mutable }));
                names.push(name);
                if self.check(&Token::Comma) {
                    self.advance();
                }
            }
            self.expect(Token::RParen)?;
            self.expect(Token::Eq)?;
            let value = self.parse_expr()?;
            let end = self.prev_end();
            for def in defs {
                self.set_visible(def, end);
            }
            return Ok(Stmt::Destructure {
                pattern: DestructurePattern::Tuple(names),
                value,
            });
        }

        let name = self.expect_ident()?;
        let def = self.note_def(&name, DefKind::Variable { mutable });

        let type_ann = if self.check(&Token::Colon) {
            self.advance();
            Some(self.parse_type_ann()?)
        } else {
            None
        };

        self.expect(Token::Eq)?;
        let value = self.parse_expr()?;
        let end = self.prev_end();
        self.set_visible(def, end);

        Ok(Stmt::Let {
            name,
            mutable,
            type_ann,
            value,
        })
    }

    /// Parses: set [mut] name to expr
    fn parse_set(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Set)?;

        let mutable = if self.check(&Token::Mut) {
            self.advance();
            true
        } else {
            false
        };

        let name = self.expect_ident()?;
        let def = self.note_def(&name, DefKind::Variable { mutable });
        self.expect(Token::To)?;
        let value = self.parse_expr()?;
        let end = self.prev_end();
        self.set_visible(def, end);

        Ok(Stmt::Let {
            name,
            mutable,
            type_ann: None,
            value,
        })
    }

    /// Parses: change name to expr
    fn parse_change(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Change)?;
        let target = self.parse_expr()?;
        self.expect(Token::To)?;
        let value = self.parse_expr()?;

        Ok(Stmt::Assign { target, value })
    }

    /// Parses: type Name = Variant(fields) | Variant(fields) | ...
    fn parse_type_def(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Type)?;
        let name = self.expect_ident()?;
        let def = self.note_def(&name, DefKind::TypeDef);
        self.hoist(def);
        self.expect(Token::Eq)?;
        self.skip_newlines();

        let mut variants = Vec::new();
        loop {
            let variant_name = self.expect_ident_or_type_name()?;
            if matches!(
                self.tokens
                    .get(self.pos.saturating_sub(1))
                    .map(|t| &t.token),
                Some(Token::Ident(_))
            ) {
                let def = self.note_def(
                    &variant_name,
                    DefKind::Variant {
                        owner: name.clone(),
                    },
                );
                self.hoist(def);
            }
            let fields = if self.check(&Token::LParen) {
                self.advance();
                let mut fields = Vec::new();
                while !self.check(&Token::RParen) {
                    fields.push(self.parse_type_ann()?);
                    if self.check(&Token::Comma) {
                        self.advance();
                    }
                }
                self.expect(Token::RParen)?;
                fields
            } else {
                Vec::new()
            };
            variants.push(Variant {
                name: variant_name,
                fields,
            });
            self.skip_newlines();
            // | separates variants -- we use Ident("|") won't work, need to check for Pipe-like token
            // The lexer produces an error for bare `|`, so we check for `||` split or just use Ident
            // Actually the `|` char in `|>` is handled as Pipe. For ADT we need bare `|`.
            // Let's check if next char after skip is an ident (next variant) preceded by nothing,
            // or if there's no more variants
            if self.check_pipe_separator() {
                self.advance(); // skip the `|` separator
                self.skip_newlines();
            } else {
                break;
            }
        }

        Ok(Stmt::TypeDef { name, variants })
    }

    /// Parses: interface Name { fn method(params) -> Type, ... }
    fn parse_interface_def(&mut self) -> Result<Stmt, ParseError> {
        // Accept both: interface Greetable { } / power Greetable { }
        if self.check(&Token::Interface) {
            self.advance();
        } else if self.check(&Token::Power) {
            self.advance();
        } else {
            return Err(self.error("expected 'interface' or 'power'"));
        }
        let name = self.expect_ident()?;
        let def = self.note_def(&name, DefKind::Interface);
        self.hoist(def);
        self.expect(Token::LBrace)?;
        self.skip_newlines();

        let mut methods = Vec::new();
        while !self.check(&Token::RBrace) {
            if self.check(&Token::Fn) || self.check(&Token::Define) {
                self.advance();
            }
            let method_name = self.expect_ident()?;
            self.note_def(
                &method_name,
                DefKind::Method {
                    owner: name.clone(),
                },
            );
            self.expect(Token::LParen)?;
            let start = self.prev_span().start;
            let (params, return_type) = self.scoped(ScopeKind::Signature, start, |p| {
                let params = p.parse_params()?;
                p.expect(Token::RParen)?;
                let return_type = if p.check(&Token::Arrow) {
                    p.advance();
                    Some(p.parse_type_ann()?)
                } else {
                    None
                };
                Ok((params, return_type))
            })?;
            methods.push(MethodSig {
                name: method_name,
                params,
                return_type,
            });
            self.skip_newlines();
        }
        self.expect(Token::RBrace)?;

        Ok(Stmt::InterfaceDef { name, methods })
    }

    /// Parses impl/give blocks:
    ///   Classic: impl Person { fn greet(it) { } }
    ///   Classic: impl Greetable for Person { fn greet(it) { } }
    ///   Natural: give Person { define greet(it) { } }
    ///   Natural: give Person the power Greetable { define greet(it) { } }
    fn parse_impl_block(&mut self) -> Result<Stmt, ParseError> {
        let is_give = self.check(&Token::Give);
        self.advance(); // consume 'impl' or 'give'

        let first_name = self.expect_ident()?;
        self.note_prev(&first_name, Role::TypeRef);
        let mut type_name = first_name.clone();
        let mut ability: Option<String> = None;

        if is_give {
            // Natural: give Person the power Greetable { }
            if self.check(&Token::The) {
                self.advance(); // consume 'the'
                if self.check(&Token::Power) {
                    self.advance(); // consume 'power'
                    let name = self.expect_ident()?;
                    self.note_prev(&name, Role::TypeRef);
                    ability = Some(name);
                } else {
                    return Err(self.error("expected 'power' after 'the'"));
                }
            }
        } else {
            // Classic: impl Greetable for Person { }
            // or:      impl Person { }
            if self.check(&Token::For) {
                // impl Greetable for Person { }
                ability = Some(first_name);
                self.advance(); // consume 'for'
                type_name = self.expect_ident()?;
                self.note_prev(&type_name, Role::TypeRef);
            }
        }

        // Parse method body
        self.expect(Token::LBrace)?;
        self.skip_newlines();

        let mut methods = Vec::new();
        let parsed = self.parse_impl_methods(&type_name, &mut methods);
        self.impl_owner = None;
        parsed?;
        self.expect(Token::RBrace)?;

        Ok(Stmt::ImplBlock {
            type_name,
            ability,
            methods,
        })
    }

    fn parse_impl_methods(
        &mut self,
        type_name: &str,
        methods: &mut Vec<SpannedStmt>,
    ) -> Result<(), ParseError> {
        while !self.check(&Token::RBrace) {
            let (line, col) = self.current_pos();
            // `parse_fn_def` takes this, so functions nested in a method
            // body are ordinary functions again.
            self.impl_owner = Some(type_name.to_string());
            let stmt = self.as_statement(Pos::new(line, col), |p| p.parse_fn_def(Vec::new()))?;
            methods.push(SpannedStmt::new(stmt, line, col));
            self.skip_newlines();
        }
        Ok(())
    }

    /// Parses: say/yell/whisper expr
    fn parse_say_yell_whisper(&mut self) -> Result<Stmt, ParseError> {
        let builtin_name = match self.current_token() {
            Token::Say => "say",
            Token::Yell => "yell",
            Token::Whisper => "whisper",
            _ => unreachable!(),
        };
        self.advance();
        let arg = self.parse_expr()?;

        Ok(Stmt::Expression(Expr::Call {
            function: Box::new(Expr::Ident(builtin_name.to_string())),
            args: vec![arg],
        }))
    }

    /// Parses: grab name from expr [with expr]
    fn parse_grab(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Grab)?;
        let name = self.expect_ident()?;
        let def = self.note_def(&name, DefKind::Variable { mutable: false });
        self.expect(Token::From)?;
        let url_expr = self.parse_expr()?;

        let fetch_call = if self.check(&Token::Ident(String::new())) {
            if let Token::Ident(ref kw) = self.current_token() {
                if kw == "with" {
                    self.advance();
                    let opts = self.parse_expr()?;
                    Expr::Call {
                        function: Box::new(Expr::Ident("fetch".to_string())),
                        args: vec![url_expr, opts],
                    }
                } else {
                    Expr::Call {
                        function: Box::new(Expr::Ident("fetch".to_string())),
                        args: vec![url_expr],
                    }
                }
            } else {
                Expr::Call {
                    function: Box::new(Expr::Ident("fetch".to_string())),
                    args: vec![url_expr],
                }
            }
        } else {
            Expr::Call {
                function: Box::new(Expr::Ident("fetch".to_string())),
                args: vec![url_expr],
            }
        };

        let end = self.prev_end();
        self.set_visible(def, end);
        Ok(Stmt::Let {
            name,
            mutable: false,
            type_ann: None,
            value: fetch_call,
        })
    }

    /// Parses: wait expr seconds
    fn parse_wait(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Wait)?;
        let duration = self.parse_expr()?;
        if self.check(&Token::Seconds) {
            self.advance();
        }

        Ok(Stmt::Expression(Expr::Call {
            function: Box::new(Expr::Ident("wait".to_string())),
            args: vec![duration],
        }))
    }

    /// Parses: try { body } catch name { handler }
    fn parse_try_catch(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::TryKw)?;
        let try_body = self.parse_block()?;
        self.skip_newlines();
        self.expect(Token::Catch)?;
        let start = self.cur_start();
        let (catch_var, catch_body) = self.scoped(ScopeKind::Catch, start, |p| {
            let catch_var = p.expect_ident()?;
            let def = p.note_def(&catch_var, DefKind::CatchVariable);
            p.hoist(def);
            let catch_body = p.parse_block()?;
            Ok((catch_var, catch_body))
        })?;
        Ok(Stmt::TryCatch {
            try_body,
            catch_var,
            catch_body,
        })
    }

    /// Whether the cursor is at the contextual `native "<path>"` of a native
    /// import (`native` is not a keyword, so `import native` alone still
    /// names a module).
    fn at_native_path(&self) -> bool {
        matches!(self.current_token(), Token::Ident(ref s) if s == "native")
            && matches!(self.peek_token(1), Token::StringLit(_))
    }

    /// Parses the string path after `native` in a native import.
    fn parse_native_path(&mut self) -> Result<String, ParseError> {
        self.advance(); // contextual `native`
        match self.current_token() {
            Token::StringLit(s) => {
                self.advance();
                Ok(s)
            }
            _ => Err(self.error("expected a library path string after 'native'")),
        }
    }

    /// Parses: import "path" / import { name, name } from "path"
    /// and the native forms: import native "path" [as name] /
    /// import { name, name } from native "path"
    fn parse_import(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Import)?;

        if self.at_native_path() {
            let path = self.parse_native_path()?;
            // The name is bound at the alias, or (derived from the path) at
            // the path string.
            let mut name_span = self.prev_span();
            let alias = if matches!(self.current_token(), Token::Ident(ref s) if s == "as") {
                self.advance();
                let alias = self.expect_ident()?;
                name_span = self.prev_span();
                Some(alias)
            } else {
                None
            };
            let name = match alias.or_else(|| crate::plugins::default_namespace(&path)) {
                Some(name) => name,
                None => {
                    return Err(self.error(&format!(
                        "cannot derive a name from native library \"{}\"; write: import native \"{}\" as name",
                        path, path
                    )))
                }
            };
            let end = self.prev_end();
            if let Some(ix) = self.index.as_mut() {
                let def = ix.def(
                    &name,
                    name_span,
                    DefKind::NativeImport { path: path.clone() },
                );
                ix.set_visible_from(def, end);
            }
            return Ok(Stmt::ImportNative {
                path,
                binding: NativeBinding::Namespace(name),
            });
        }

        if self.check(&Token::LBrace) {
            // import { name1, name2 } from "path"
            self.advance();
            let mut names = Vec::new();
            let mut spans = Vec::new();
            while !self.check(&Token::RBrace) {
                names.push(self.expect_ident()?);
                spans.push(self.prev_span());
                if self.check(&Token::Comma) {
                    self.advance();
                }
            }
            self.expect(Token::RBrace)?;
            self.expect(Token::From)?;
            if self.at_native_path() {
                let path = self.parse_native_path()?;
                let end = self.prev_end();
                if let Some(ix) = self.index.as_mut() {
                    for (name, span) in names.iter().zip(&spans) {
                        let def = ix.def(name, *span, DefKind::NativeImport { path: path.clone() });
                        ix.set_visible_from(def, end);
                    }
                }
                return Ok(Stmt::ImportNative {
                    path,
                    binding: NativeBinding::Names(names),
                });
            }
            let path = match self.current_token() {
                Token::StringLit(s) => {
                    self.advance();
                    s
                }
                _ => return Err(self.error("expected string path after 'from'")),
            };
            let end = self.prev_end();
            if let Some(ix) = self.index.as_mut() {
                for (name, span) in names.iter().zip(spans) {
                    let def = ix.def(name, span, DefKind::Import { path: path.clone() });
                    ix.set_visible_from(def, end);
                }
            }
            Ok(Stmt::Import {
                path,
                names: Some(names),
            })
        } else {
            // import "path" or import name
            let path = match self.current_token() {
                Token::StringLit(s) => {
                    self.advance();
                    s
                }
                Token::Ident(s) => {
                    self.advance();
                    s
                }
                _ => {
                    return Err(self.error(
                        "expected module name or string path after 'import'. Examples: import \"utils.fg\" or import math",
                    ));
                }
            };
            Ok(Stmt::Import { path, names: None })
        }
    }

    fn parse_yield(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // skip yield or emit
        let expr = self.parse_expr()?;
        Ok(Stmt::YieldStmt(expr))
    }

    /// Parses: unpack { a, b } from expr  /  unpack [ a, ...rest ] from expr
    /// An identifier bound by destructuring (`unpack`).
    fn expect_bound_ident(&mut self, defs: &mut Vec<Option<OccId>>) -> Result<String, ParseError> {
        let name = self.expect_ident()?;
        defs.push(self.note_def(&name, DefKind::Variable { mutable: false }));
        Ok(name)
    }

    fn parse_unpack(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Unpack)?;
        let mut defs = Vec::new();

        let pattern = if self.check(&Token::LBrace) {
            self.advance();
            let mut names = Vec::new();
            while !self.check(&Token::RBrace) {
                names.push(self.expect_bound_ident(&mut defs)?);
                if self.check(&Token::Comma) {
                    self.advance();
                }
            }
            self.expect(Token::RBrace)?;
            DestructurePattern::Object(names)
        } else if self.check(&Token::LBracket) {
            self.advance();
            let mut items = Vec::new();
            let mut rest = None;
            while !self.check(&Token::RBracket) {
                if self.check(&Token::DotDotDot) {
                    self.advance();
                    rest = Some(self.expect_bound_ident(&mut defs)?);
                } else {
                    items.push(self.expect_bound_ident(&mut defs)?);
                }
                if self.check(&Token::Comma) {
                    self.advance();
                }
            }
            self.expect(Token::RBracket)?;
            DestructurePattern::Array { items, rest }
        } else if self.check(&Token::LParen) {
            self.advance();
            let mut names = Vec::new();
            while !self.check(&Token::RParen) {
                names.push(self.expect_bound_ident(&mut defs)?);
                if self.check(&Token::Comma) {
                    self.advance();
                }
            }
            self.expect(Token::RParen)?;
            DestructurePattern::Tuple(names)
        } else {
            return Err(self.error("expected {, [, or ( after 'unpack'"));
        };

        self.expect(Token::From)?;
        let value = self.parse_expr()?;
        let end = self.prev_end();
        for def in defs {
            self.set_visible(def, end);
        }

        Ok(Stmt::Destructure { pattern, value })
    }

    fn parse_when(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::When)?;
        let subject = self.parse_expr()?;
        self.expect(Token::LBrace)?;
        self.skip_newlines();
        let mut arms = Vec::new();
        while !self.check(&Token::RBrace) {
            if self.check(&Token::Else) || self.check(&Token::Otherwise) || self.check(&Token::Nah)
            {
                self.advance();
                if self.check(&Token::Arrow) || self.check(&Token::FatArrow) {
                    self.advance();
                }
                let result = self.parse_expr()?;
                arms.push(WhenArm {
                    op: None,
                    value: None,
                    result,
                    is_else: true,
                });
            } else {
                let op = match self.current_token() {
                    Token::Lt => {
                        self.advance();
                        Some(BinOp::Lt)
                    }
                    Token::Gt => {
                        self.advance();
                        Some(BinOp::Gt)
                    }
                    Token::LtEq => {
                        self.advance();
                        Some(BinOp::LtEq)
                    }
                    Token::GtEq => {
                        self.advance();
                        Some(BinOp::GtEq)
                    }
                    Token::EqEq => {
                        self.advance();
                        Some(BinOp::Eq)
                    }
                    Token::NotEq => {
                        self.advance();
                        Some(BinOp::NotEq)
                    }
                    _ => None,
                };
                let value = Some(self.parse_expr()?);
                if self.check(&Token::Arrow) || self.check(&Token::FatArrow) {
                    self.advance();
                }
                let result = self.parse_expr()?;
                arms.push(WhenArm {
                    op,
                    value,
                    result,
                    is_else: false,
                });
            }
            self.skip_newlines();
            if self.check(&Token::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }
        self.expect(Token::RBrace)?;
        Ok(Stmt::When { subject, arms })
    }

    fn parse_check(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Check)?;
        let expr = self.parse_expr()?;
        let check_kind = if self.check(&Token::Ident(String::new())) {
            match self.current_token() {
                Token::Ident(ref s) if s == "is" => {
                    self.advance();
                    if self.check(&Token::Not) {
                        self.advance();
                    } else if matches!(self.current_token(), Token::Ident(ref s) if s == "not") {
                        self.advance();
                    }
                    if let Token::Ident(ref w) = self.current_token() {
                        if w == "empty" {
                            self.advance();
                            CheckKind::IsNotEmpty
                        } else {
                            CheckKind::IsTrue
                        }
                    } else {
                        CheckKind::IsTrue
                    }
                }
                Token::Ident(ref s) if s == "between" => {
                    self.advance();
                    // Bounds are parsed below the logical operators so the
                    // separator `and` (or `&&`) is never folded into the
                    // lower bound: `check x between 1 && 10` has bounds
                    // 1 and 10, not `1 && 10`.
                    let lo = self.parse_addition()?;
                    match self.current_token() {
                        Token::Ident(ref w) if w == "and" => self.advance(),
                        Token::And => self.advance(),
                        other => {
                            return Err(self.error(&format!(
                            "expected 'and' between the bounds of `check ... between`, found {:?}",
                            other
                        )))
                        }
                    }
                    let hi = self.parse_addition()?;
                    CheckKind::Between(lo, hi)
                }
                _ => CheckKind::IsTrue,
            }
        } else if self.check(&Token::Ident(String::new())) {
            CheckKind::IsTrue
        } else {
            CheckKind::IsTrue
        };
        Ok(Stmt::CheckStmt { expr, check_kind })
    }

    fn parse_safe_block(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Safe)?;
        let body = self.parse_block()?;
        Ok(Stmt::SafeBlock { body })
    }

    fn parse_timeout(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Timeout)?;
        let duration = self.parse_expr()?;
        if self.check(&Token::Seconds) {
            self.advance();
        }
        let body = self.parse_block()?;
        Ok(Stmt::TimeoutBlock { duration, body })
    }

    fn parse_retry(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Retry)?;
        let count = self.parse_expr()?;
        if self.check(&Token::Times) {
            self.advance();
        }
        let body = self.parse_block()?;
        Ok(Stmt::RetryBlock { count, body })
    }

    fn parse_schedule(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Schedule)?;
        if self.check(&Token::Every) {
            self.advance();
        }
        let interval = self.parse_expr()?;
        let unit = match self.current_token() {
            Token::Seconds => {
                self.advance();
                "seconds".to_string()
            }
            Token::Ident(ref s) if s == "minutes" => {
                self.advance();
                "minutes".to_string()
            }
            Token::Ident(ref s) if s == "hours" => {
                self.advance();
                "hours".to_string()
            }
            _ => "seconds".to_string(),
        };
        let body = self.parse_block()?;
        Ok(Stmt::ScheduleBlock {
            interval,
            unit,
            body,
        })
    }

    fn parse_watch(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Watch)?;
        let path = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::WatchBlock { path, body })
    }

    /// Parses: download url to filepath
    fn parse_download(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Download)?;
        let url = self.parse_expr()?;
        let dest = if self.check(&Token::To) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        // Desugar to: let _dl = http.download(url, dest)
        let mut call_args = vec![url];
        if let Some(d) = dest {
            call_args.push(d);
        }
        Ok(Stmt::Expression(Expr::Call {
            function: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("http".to_string())),
                field: "download".to_string(),
            }),
            args: call_args,
        }))
    }

    /// Parses: crawl url -> let result = http.crawl(url)
    fn parse_crawl(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Crawl)?;
        let url = self.parse_expr()?;
        Ok(Stmt::Expression(Expr::Call {
            function: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("http".to_string())),
                field: "crawl".to_string(),
            }),
            args: vec![url],
        }))
    }

    fn parse_prompt_def(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Prompt)?;
        let name = self.expect_ident()?;
        let def = self.note_def(&name, DefKind::Callable);
        self.expect(Token::LParen)?;
        let start = self.prev_span().start;
        let params = self.scoped(ScopeKind::Function, start, |p| p.parse_params())?;
        self.expect(Token::RParen)?;
        let end = self.prev_end();
        self.set_visible(def, end);
        self.expect(Token::LBrace)?;
        self.skip_newlines();
        let mut system = String::new();
        let mut user_template = String::new();
        let mut returns = None;
        while !self.check(&Token::RBrace) {
            if let Token::Ident(ref key) = self.current_token() {
                let key = key.clone();
                self.advance();
                self.expect(Token::Colon)?;
                if let Token::StringLit(ref s) | Token::RawStringLit(ref s) = self.current_token() {
                    let val = s.clone();
                    self.advance();
                    match key.as_str() {
                        "system" => system = val,
                        "user" => user_template = val,
                        "returns" => returns = Some(val),
                        _ => {}
                    }
                }
            }
            self.skip_newlines();
        }
        self.expect(Token::RBrace)?;
        Ok(Stmt::PromptDef {
            name,
            params,
            system,
            user_template,
            returns,
        })
    }

    fn parse_agent_def(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Agent)?;
        let name = self.expect_ident()?;
        let def = self.note_def(&name, DefKind::Callable);
        self.expect(Token::LParen)?;
        let start = self.prev_span().start;
        let params = self.scoped(ScopeKind::Function, start, |p| p.parse_params())?;
        self.expect(Token::RParen)?;
        let end = self.prev_end();
        self.set_visible(def, end);
        self.expect(Token::LBrace)?;
        self.skip_newlines();

        let mut tools = Vec::new();
        let mut goal = String::new();
        let mut max_steps = 5usize;

        while !self.check(&Token::RBrace) {
            let Token::Ident(key) = self.current_token().clone() else {
                return Err(self.error("expected agent field name"));
            };
            self.advance();
            self.expect(Token::Colon)?;

            match key.as_str() {
                "tools" => {
                    self.expect(Token::LBracket)?;
                    self.skip_newlines();
                    while !self.check(&Token::RBracket) {
                        match self.current_token().clone() {
                            Token::StringLit(tool) | Token::RawStringLit(tool) => {
                                tools.push(tool);
                                self.advance();
                            }
                            _ => return Err(self.error("agent tools must be string literals")),
                        }
                        if self.check(&Token::Comma) {
                            self.advance();
                        }
                        self.skip_newlines();
                    }
                    self.expect(Token::RBracket)?;
                }
                "goal" => match self.current_token().clone() {
                    Token::StringLit(text) | Token::RawStringLit(text) => {
                        goal = text;
                        self.advance();
                    }
                    _ => return Err(self.error("agent goal must be a string literal")),
                },
                "max_steps" => match self.current_token().clone() {
                    Token::Int(steps) if steps >= 0 => {
                        max_steps = steps as usize;
                        self.advance();
                    }
                    _ => return Err(self.error("agent max_steps must be a non-negative integer")),
                },
                _ => return Err(self.error("unknown field in agent definition")),
            }

            self.skip_newlines();
        }

        self.expect(Token::RBrace)?;
        Ok(Stmt::AgentDef {
            name,
            params,
            tools,
            goal,
            max_steps,
        })
    }

    /// Parses: repeat expr times { body }
    fn parse_repeat(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Repeat)?;
        let count = self.parse_expr()?;
        if self.check(&Token::Times) {
            self.advance();
        }
        let body = self.parse_block()?;

        Ok(Stmt::For {
            var: "_".to_string(),
            var2: None,
            iterable: Expr::Call {
                function: Box::new(Expr::Ident("range".to_string())),
                args: vec![count],
            },
            body,
        })
    }

    fn parse_type_params(&mut self) -> Result<Vec<String>, ParseError> {
        if !self.check(&Token::Lt) {
            return Ok(vec![]);
        }
        self.advance(); // consume <
        let mut params = Vec::new();
        loop {
            let name = self.expect_ident()?;
            let def = self.note_def(&name, DefKind::TypeParameter);
            self.hoist(def);
            params.push(name);
            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(Token::Gt)?;
        Ok(params)
    }

    fn parse_fn_def(&mut self, decorators: Vec<Decorator>) -> Result<Stmt, ParseError> {
        let is_async = if self.check(&Token::Async) || self.check(&Token::ForgeKw) {
            self.advance();
            true
        } else {
            false
        };

        // After forge/async, fn/define is optional
        if self.check(&Token::Define) {
            self.advance();
        } else if self.check(&Token::Fn) {
            self.advance();
        } else if !is_async {
            return Err(self.error("expected 'fn' or 'define'"));
        }
        let name = self.expect_ident()?;
        let def = match self.impl_owner.take() {
            Some(owner) => self.note_def(&name, DefKind::Method { owner }),
            None => self.note_def(&name, DefKind::Function),
        };
        let start = self.prev_end();
        let parsed = self.scoped(ScopeKind::Function, start, |p| {
            let type_params = p.parse_type_params()?;

            p.expect(Token::LParen)?;
            let params = p.parse_params()?;
            p.expect(Token::RParen)?;

            let return_type = if p.check(&Token::Arrow) {
                p.advance();
                Some(p.parse_type_ann()?)
            } else {
                None
            };

            let body = p.parse_block()?;
            Ok((type_params, params, return_type, body))
        });
        let (type_params, params, return_type, body) = parsed?;
        let end = self.prev_end();
        self.set_visible(def, end);

        Ok(Stmt::FnDef {
            name,
            type_params,
            params,
            return_type,
            body,
            decorators,
            is_async,
        })
    }

    fn parse_struct_def(&mut self) -> Result<Stmt, ParseError> {
        // Accept both: struct Person { } / thing Person { }
        if self.check(&Token::Struct) {
            self.advance();
        } else if self.check(&Token::Thing) {
            self.advance();
        } else {
            return Err(self.error("expected 'struct' or 'thing'"));
        }
        let name = self.expect_ident()?;
        let def = self.note_def(&name, DefKind::Struct);
        self.hoist(def);
        let start = self.prev_end();
        let (type_params, fields) =
            self.scoped(ScopeKind::Struct, start, |p| p.parse_struct_body(&name))?;
        Ok(Stmt::StructDef {
            name,
            type_params,
            fields,
        })
    }

    /// `<T, ...> { field: Type [= default], has embedded: Type, ... }`
    fn parse_struct_body(
        &mut self,
        name: &str,
    ) -> Result<(Vec<String>, Vec<FieldDef>), ParseError> {
        let type_params = self.parse_type_params()?;

        self.expect(Token::LBrace)?;
        self.skip_newlines();

        let mut fields = Vec::new();
        while !self.check(&Token::RBrace) {
            // Contextual "has" — embedding: has address: Address
            let embedded = if let Token::Ident(ref id) = self.current_token() {
                if id == "has" {
                    self.advance();
                    true
                } else {
                    false
                }
            } else {
                false
            };

            let field_name = self.expect_ident()?;
            self.note_def(
                &field_name,
                DefKind::Field {
                    owner: name.to_string(),
                },
            );
            self.expect(Token::Colon)?;
            let type_ann = self.parse_type_ann()?;

            // Optional default: field: Type = expr
            let default = if self.check(&Token::Eq) {
                self.advance();
                Some(self.parse_expr()?)
            } else {
                None
            };

            fields.push(FieldDef {
                name: field_name,
                type_ann,
                default,
                embedded,
            });
            self.skip_newlines();
            if self.check(&Token::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }

        self.expect(Token::RBrace)?;
        Ok((type_params, fields))
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Return)?;

        // Check if there's an expression after return
        if self.is_at_end() || self.check(&Token::RBrace) || self.check(&Token::Newline) {
            return Ok(Stmt::Return(None));
        }

        let expr = self.parse_expr()?;
        Ok(Stmt::Return(Some(expr)))
    }

    fn check_else_keyword(&self) -> bool {
        self.check(&Token::Else) || self.check(&Token::Otherwise) || self.check(&Token::Nah)
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::If)?;
        let condition = self.parse_expr()?;
        let then_body = self.parse_block()?;

        self.skip_newlines();
        let else_body = if self.check_else_keyword() {
            self.advance();
            if self.check(&Token::If) {
                let (line, col) = if self.pos < self.tokens.len() {
                    (self.tokens[self.pos].line, self.tokens[self.pos].col)
                } else {
                    (0, 0)
                };
                let elif = self.as_statement(Pos::new(line, col), Self::parse_if)?;
                Some(vec![SpannedStmt::new(elif, line, col)])
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };

        Ok(Stmt::If {
            condition,
            then_body,
            else_body,
        })
    }

    fn parse_match(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Match)?;
        let subject = self.parse_expr()?;

        self.expect(Token::LBrace)?;
        self.skip_newlines();

        let mut arms = Vec::new();
        while !self.check(&Token::RBrace) {
            let start = self.cur_start();
            let arm = self.scoped(ScopeKind::Arm, start, Self::parse_match_arm)?;
            arms.push(arm);
            self.skip_newlines();
            if self.check(&Token::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }

        self.expect(Token::RBrace)?;
        Ok(Stmt::Match { subject, arms })
    }

    /// `pattern => { body }` or `pattern => statement`.
    fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let pattern = self.parse_pattern()?;
        self.expect(Token::FatArrow)?;

        let body = if self.check(&Token::LBrace) {
            self.parse_block()?
        } else {
            let (line, col) = if self.pos < self.tokens.len() {
                (self.tokens[self.pos].line, self.tokens[self.pos].col)
            } else {
                (0, 0)
            };
            let stmt = self.parse_statement()?;
            vec![SpannedStmt::new(stmt, line, col)]
        };
        Ok(MatchArm { pattern, body })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        self.nested(Self::parse_pattern_inner)
    }

    fn parse_pattern_inner(&mut self) -> Result<Pattern, ParseError> {
        match self.current_token() {
            Token::Ident(ref name) if name == "_" => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            Token::Ident(ref name) => {
                let name = name.clone();
                self.advance();

                // Check for constructor pattern: Name(fields...)
                if self.check(&Token::LParen) {
                    self.note_prev(&name, Role::Ref);
                    self.advance();
                    let mut fields = Vec::new();
                    while !self.check(&Token::RParen) {
                        fields.push(self.parse_pattern()?);
                        if self.check(&Token::Comma) {
                            self.advance();
                        }
                    }
                    self.expect(Token::RParen)?;
                    Ok(Pattern::Constructor { name, fields })
                } else {
                    let def = self.note_def(&name, DefKind::PatternBinding);
                    self.hoist(def);
                    Ok(Pattern::Binding(name))
                }
            }
            Token::Int(n) => {
                let n = n;
                self.advance();
                Ok(Pattern::Literal(Expr::Int(n)))
            }
            Token::Float(n) => {
                let n = n;
                self.advance();
                Ok(Pattern::Literal(Expr::Float(n)))
            }
            Token::StringLit(ref s) => {
                let s = s.clone();
                self.advance();
                Ok(Pattern::Literal(Expr::StringLit(s)))
            }
            Token::True => {
                self.advance();
                Ok(Pattern::Literal(Expr::Bool(true)))
            }
            Token::False => {
                self.advance();
                Ok(Pattern::Literal(Expr::Bool(false)))
            }
            _ => Err(self.error("expected pattern")),
        }
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::For)?;
        let start = self.prev_span().start;
        self.scoped(ScopeKind::Loop, start, Self::parse_for_rest)
    }

    fn parse_for_rest(&mut self) -> Result<Stmt, ParseError> {
        if self.check(&Token::Each) {
            self.advance();
        }
        let var = self.expect_ident()?;
        let def1 = self.note_def(&var, DefKind::LoopVariable);
        // Check for key, value syntax
        let (var2, def2) = if self.check(&Token::Comma) {
            self.advance();
            let name = self.expect_ident()?;
            let def = self.note_def(&name, DefKind::LoopVariable);
            (Some(name), def)
        } else {
            (None, None)
        };
        self.expect(Token::In)?;
        let iterable = self.parse_expr()?;
        // The loop variables are in scope in the body, not in the iterable.
        let body_start = self.cur_start();
        self.set_visible(def1, body_start);
        self.set_visible(def2, body_start);
        let body = self.parse_block()?;

        Ok(Stmt::For {
            var,
            var2,
            iterable,
            body,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::While)?;
        let condition = self.parse_expr()?;
        let body = self.parse_block()?;

        Ok(Stmt::While { condition, body })
    }

    fn parse_loop(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Loop)?;
        let body = self.parse_block()?;
        Ok(Stmt::Loop { body })
    }

    fn parse_spawn(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Spawn)?;
        let body = self.parse_block()?;
        Ok(Stmt::Spawn { body })
    }

    fn parse_squad(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Squad)?;
        let body = self.parse_block()?;
        Ok(Stmt::Squad { body })
    }

    fn parse_decorator_or_fn(&mut self) -> Result<Stmt, ParseError> {
        let decorator = self.parse_decorator()?;
        self.skip_newlines();

        // @server is always a standalone config decorator
        if decorator.name == "server" {
            return Ok(Stmt::DecoratorStmt(decorator));
        }

        if self.check(&Token::Fn) || self.check(&Token::Define) {
            self.parse_fn_def(vec![decorator])
        } else if self.check(&Token::At) {
            let mut decorators = vec![decorator];
            while self.check(&Token::At) {
                decorators.push(self.parse_decorator()?);
                self.skip_newlines();
            }
            if self.check(&Token::Fn) || self.check(&Token::Define) {
                self.parse_fn_def(decorators)
            } else {
                Ok(Stmt::DecoratorStmt(decorators.pop().ok_or_else(|| {
                    self.error("internal: empty decorator list")
                })?))
            }
        } else {
            // Standalone decorator
            Ok(Stmt::DecoratorStmt(decorator))
        }
    }

    fn parse_decorator(&mut self) -> Result<Decorator, ParseError> {
        self.expect(Token::At)?;
        let name = self.expect_ident()?;

        let args = if self.check(&Token::LParen) {
            self.advance();
            let mut args = Vec::new();
            while !self.check(&Token::RParen) {
                // Try named arg: key: value
                if let Token::Ident(ref key) = self.current_token() {
                    let key = key.clone();
                    let saved_pos = self.pos;
                    self.advance();
                    if self.check(&Token::Colon) {
                        self.advance();
                        let value = self.parse_expr()?;
                        args.push(DecoratorArg::Named(key, value));
                    } else {
                        // Not a named arg, backtrack
                        self.pos = saved_pos;
                        let expr = self.parse_expr()?;
                        args.push(DecoratorArg::Positional(expr));
                    }
                } else {
                    let expr = self.parse_expr()?;
                    args.push(DecoratorArg::Positional(expr));
                }

                if self.check(&Token::Comma) {
                    self.advance();
                }
            }
            self.expect(Token::RParen)?;
            args
        } else {
            Vec::new()
        };

        Ok(Decorator { name, args })
    }

    fn parse_expr_or_assign(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expr()?;

        if self.check(&Token::Eq) {
            self.advance();
            let value = self.parse_expr()?;
            Ok(Stmt::Assign {
                target: expr,
                value,
            })
        } else if self.check(&Token::PlusEq)
            || self.check(&Token::MinusEq)
            || self.check(&Token::StarEq)
            || self.check(&Token::SlashEq)
            || self.check(&Token::PercentEq)
        {
            let op = match self.current_token() {
                Token::PlusEq => BinOp::Add,
                Token::MinusEq => BinOp::Sub,
                Token::StarEq => BinOp::Mul,
                Token::SlashEq => BinOp::Div,
                Token::PercentEq => BinOp::Mod,
                _ => unreachable!(),
            };
            self.advance();
            let rhs = self.parse_expr()?;
            Ok(Stmt::Assign {
                target: expr.clone(),
                value: Expr::BinOp {
                    left: Box::new(expr),
                    op,
                    right: Box::new(rhs),
                },
            })
        } else {
            Ok(Stmt::Expression(expr))
        }
    }

    // ========== Expression Parsing (Pratt) ==========

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.nested(Self::parse_query_chain)
    }

    fn parse_query_chain(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_pipeline()?;

        if self.check(&Token::Where) {
            expr = self.parse_where_filter_suffix(expr)?;
        }

        if self.check(&Token::PipeRight) {
            let mut steps = Vec::new();
            while self.check(&Token::PipeRight) {
                self.advance();
                steps.push(self.parse_pipe_step()?);
            }
            expr = Expr::PipeChain {
                source: Box::new(expr),
                steps,
            };
        }

        Ok(expr)
    }

    /// Pipeline: expr |> expr |> expr
    fn parse_pipeline(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_or()?;

        while self.check(&Token::Pipe) {
            self.advance();
            let func = self.parse_or()?;
            expr = Expr::Pipeline {
                value: Box::new(expr),
                function: Box::new(func),
            };
        }

        Ok(expr)
    }

    fn parse_where_filter_suffix(&mut self, source: Expr) -> Result<Expr, ParseError> {
        self.expect(Token::Where)?;
        let field = self.expect_ident()?;
        self.note_prev(&field, Role::Field);
        let op = self.parse_query_compare_op()?;
        let value = self.parse_pipeline()?;
        Ok(Expr::WhereFilter {
            source: Box::new(source),
            field,
            op,
            value: Box::new(value),
        })
    }

    fn parse_pipe_step(&mut self) -> Result<PipeStep, ParseError> {
        match self.current_token() {
            Token::Keep => {
                self.advance();
                self.expect(Token::Where)?;
                Ok(PipeStep::Keep(Box::new(self.parse_pipe_keep_predicate()?)))
            }
            Token::Ident(ref name) if name == "sort" => {
                self.advance();
                let field = if self.check(&Token::By) {
                    self.advance();
                    let field = self.expect_ident()?;
                    self.note_prev(&field, Role::Field);
                    Some(field)
                } else {
                    None
                };
                Ok(PipeStep::Sort(field))
            }
            Token::Take => {
                self.advance();
                Ok(PipeStep::Take(Box::new(self.parse_pipeline()?)))
            }
            _ => Ok(PipeStep::Apply(Box::new(self.parse_pipeline()?))),
        }
    }

    fn parse_pipe_keep_predicate(&mut self) -> Result<Expr, ParseError> {
        let field = self.expect_ident()?;
        self.note_prev(&field, Role::Field);
        let predicate = if self.is_query_compare_op() {
            let op = self.parse_query_compare_op()?;
            let rhs = self.parse_pipeline()?;
            Expr::BinOp {
                left: Box::new(Expr::FieldAccess {
                    object: Box::new(Expr::Ident("it".to_string())),
                    field,
                }),
                op,
                right: Box::new(rhs),
            }
        } else {
            Expr::FieldAccess {
                object: Box::new(Expr::Ident("it".to_string())),
                field,
            }
        };

        Ok(Expr::Lambda {
            params: vec![Param {
                name: "it".to_string(),
                type_ann: None,
                default: None,
            }],
            body: vec![SpannedStmt::unspanned(Stmt::Return(Some(predicate)))],
        })
    }

    fn is_query_compare_op(&self) -> bool {
        matches!(
            self.current_token(),
            Token::EqEq | Token::NotEq | Token::Lt | Token::Gt | Token::LtEq | Token::GtEq
        )
    }

    fn parse_query_compare_op(&mut self) -> Result<BinOp, ParseError> {
        let op = match self.current_token() {
            Token::EqEq => BinOp::Eq,
            Token::NotEq => BinOp::NotEq,
            Token::Lt => BinOp::Lt,
            Token::Gt => BinOp::Gt,
            Token::LtEq => BinOp::LtEq,
            Token::GtEq => BinOp::GtEq,
            _ => return Err(self.error("expected comparison operator after query field")),
        };
        self.advance();
        Ok(op)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.check(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality()?;
        while self.check(&Token::And) {
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.current_token() {
                Token::EqEq => BinOp::Eq,
                Token::NotEq => BinOp::NotEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_addition()?;
        loop {
            let op = match self.current_token() {
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                Token::LtEq => BinOp::LtEq,
                Token::GtEq => BinOp::GtEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_addition()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_addition(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = match self.current_token() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplication()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplication(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.current_token() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        self.nested(Self::parse_unary_inner)
    }

    fn parse_unary_inner(&mut self) -> Result<Expr, ParseError> {
        match self.current_token() {
            Token::Await | Token::Hold => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Await(Box::new(operand)))
            }
            Token::Must => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Must(Box::new(operand)))
            }
            Token::Freeze => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Freeze(Box::new(operand)))
            }
            Token::Ask => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Ask(Box::new(operand)))
            }
            Token::DotDotDot => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Spread(Box::new(operand)))
            }
            Token::Minus => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                })
            }
            Token::Not => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                })
            }
            _ => self.parse_postfix(),
        }
    }

    /// Postfix: calls, field access, indexing, try (?)
    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            match self.current_token() {
                Token::LParen => {
                    self.advance();
                    let args = self.parse_call_args()?;
                    self.expect(Token::RParen)?;
                    expr = Expr::Call {
                        function: Box::new(expr),
                        args,
                    };
                }
                Token::Dot => {
                    self.advance();
                    let field = self.expect_ident()?;
                    self.note_prev(&field, Role::Field);
                    expr = Expr::FieldAccess {
                        object: Box::new(expr),
                        field,
                    };
                }
                Token::LBracket => {
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(Token::RBracket)?;
                    expr = Expr::Index {
                        object: Box::new(expr),
                        index: Box::new(index),
                    };
                }
                Token::Question => {
                    self.advance();
                    expr = Expr::Try(Box::new(expr));
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.current_token() {
            Token::Int(n) => {
                let n = n;
                self.advance();
                Ok(Expr::Int(n))
            }
            Token::Float(n) => {
                let n = n;
                self.advance();
                Ok(Expr::Float(n))
            }
            Token::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            Token::NullLit => {
                self.advance();
                Ok(Expr::Ident("null".to_string()))
            }

            Token::Spawn => {
                self.advance();
                let body = self.parse_block()?;
                Ok(Expr::Spawn(body))
            }

            Token::Squad => {
                self.advance();
                let body = self.parse_block()?;
                Ok(Expr::Squad(body))
            }

            Token::StringLit(ref s) => {
                let s = s.clone();
                self.advance();
                if s.contains('{') && s.contains('}') {
                    let token = self.pos - 1;
                    self.parse_string_interpolation(&s, token)
                } else {
                    Ok(Expr::StringLit(s))
                }
            }

            Token::RawStringLit(ref s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::StringLit(s))
            }

            // Allow 'any' keyword as identifier in expression context (builtin function)
            Token::Any => {
                self.advance();
                self.note_prev("any", Role::Ref);
                Ok(Expr::Ident("any".to_string()))
            }

            // Allow `set` keyword as identifier in expression context so that
            // `set(...)` parses as a call to the set() constructor. Statement-level
            // `set name to ...` is handled earlier in parse_statement.
            Token::Set => {
                self.advance();
                self.note_prev("set", Role::Ref);
                Ok(Expr::Ident("set".to_string()))
            }

            // craft Person { name: "Alice", age: 30 }
            Token::Craft => {
                self.advance();
                let name = self.expect_ident()?;
                self.note_prev(&name, Role::TypeRef);
                self.expect(Token::LBrace)?;
                self.skip_newlines();
                let mut fields = Vec::new();
                while !self.check(&Token::RBrace) {
                    let field_name = self.expect_ident()?;
                    self.note_prev(&field_name, Role::Field);
                    self.expect(Token::Colon)?;
                    let value = self.parse_expr()?;
                    fields.push((field_name, value));
                    self.skip_newlines();
                    if self.check(&Token::Comma) {
                        self.advance();
                    }
                    self.skip_newlines();
                }
                self.expect(Token::RBrace)?;
                Ok(Expr::StructInit { name, fields })
            }

            Token::Ident(ref name) => {
                let name = name.clone();
                self.advance();

                // Check for struct init: Name { field: value }
                if name.chars().next().is_some_and(|c| c.is_uppercase())
                    && self.check(&Token::LBrace)
                {
                    self.note_prev(&name, Role::TypeRef);
                    self.advance();
                    self.skip_newlines();
                    let mut fields = Vec::new();
                    while !self.check(&Token::RBrace) {
                        let field_name = self.expect_ident()?;
                        self.note_prev(&field_name, Role::Field);
                        self.expect(Token::Colon)?;
                        let value = self.parse_expr()?;
                        fields.push((field_name, value));
                        self.skip_newlines();
                        if self.check(&Token::Comma) {
                            self.advance();
                        }
                        self.skip_newlines();
                    }
                    self.expect(Token::RBrace)?;
                    Ok(Expr::StructInit { name, fields })
                } else {
                    self.note_prev(&name, Role::Ref);
                    Ok(Expr::Ident(name))
                }
            }

            Token::LParen => {
                self.advance();
                let first = self.parse_expr()?;
                if self.check(&Token::Comma) {
                    // Tuple: (e1, e2, ...) or (e1,)
                    self.advance();
                    let mut items = vec![first];
                    self.skip_newlines();
                    while !self.check(&Token::RParen) {
                        items.push(self.parse_expr()?);
                        self.skip_newlines();
                        if self.check(&Token::Comma) {
                            self.advance();
                        }
                        self.skip_newlines();
                    }
                    self.expect(Token::RParen)?;
                    Ok(Expr::Tuple(items))
                } else {
                    // Grouping: (expr)
                    self.expect(Token::RParen)?;
                    Ok(first)
                }
            }

            Token::LBrace => {
                // Object literal or block — disambiguate
                self.parse_object_or_block()
            }

            Token::LBracket => {
                // Array literal
                self.advance();
                let mut elements = Vec::new();
                self.skip_newlines();
                while !self.check(&Token::RBracket) {
                    elements.push(self.parse_expr()?);
                    self.skip_newlines();
                    if self.check(&Token::Comma) {
                        self.advance();
                    }
                    self.skip_newlines();
                }
                self.expect(Token::RBracket)?;
                Ok(Expr::Array(elements))
            }

            Token::If => {
                // if-expression: if cond { expr } else { expr }
                self.advance();
                let cond = self.parse_expr()?;
                let then_body = self.parse_block()?;
                self.skip_newlines();
                let else_body = if self.check_else_keyword() {
                    self.advance();
                    Some(self.parse_block()?)
                } else {
                    None
                };
                // Wrap as block expression
                Ok(Expr::Block(vec![SpannedStmt::unspanned(Stmt::If {
                    condition: cond,
                    then_body,
                    else_body,
                })]))
            }

            Token::Type => {
                self.advance();
                self.note_prev("type", Role::Ref);
                Ok(Expr::Ident("type".to_string()))
            }

            Token::Select => {
                self.advance();
                self.note_prev("select", Role::Ref);
                Ok(Expr::Ident("select".to_string()))
            }

            Token::Fn => {
                self.advance();
                let start = self.prev_span().start;
                self.scoped(ScopeKind::Lambda, start, |p| {
                    p.expect(Token::LParen)?;
                    let params = p.parse_params()?;
                    p.expect(Token::RParen)?;
                    let body = p.parse_block()?;
                    Ok(Expr::Lambda { params, body })
                })
            }

            Token::When => {
                let when_stmt = self.parse_when()?;
                Ok(Expr::Block(vec![SpannedStmt::unspanned(when_stmt)]))
            }

            Token::Safe => {
                let safe_stmt = self.parse_safe_block()?;
                Ok(Expr::Block(vec![SpannedStmt::unspanned(safe_stmt)]))
            }

            _ => Err(self.error(&format!("unexpected token: {:?}", self.current_token()))),
        }
    }

    /// `token` is the index of the string literal token (for the index).
    fn parse_string_interpolation(&mut self, s: &str, token: usize) -> Result<Expr, ParseError> {
        let mut parts = Vec::new();
        let mut chars = s.chars().enumerate().peekable();
        let mut current = String::new();

        while let Some((_, ch)) = chars.next() {
            if ch == '{' {
                if !current.is_empty() {
                    parts.push(StringPart::Literal(std::mem::take(&mut current)));
                }
                let mut expr_str = String::new();
                let mut expr_start = None;
                let mut depth = 1;
                for (i, inner) in chars.by_ref() {
                    if inner == '{' {
                        depth += 1;
                    }
                    if inner == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    expr_start.get_or_insert(i);
                    expr_str.push(inner);
                }
                if depth != 0 {
                    return Err(self.error("unterminated interpolation expression"));
                }

                let leading = expr_str.chars().take_while(|c| c.is_whitespace()).count();
                let expr_str = expr_str.trim();
                if expr_str.is_empty() {
                    return Err(self.error("empty interpolation expression"));
                }

                // Value index of the first char of the trimmed expression.
                let start = expr_start.unwrap_or(0) + leading;
                let parsed = self.parse_interpolation_expr(expr_str, token, start)?;
                parts.push(StringPart::Expr(parsed));
            } else if ch == '}' {
                return Err(self.error("unexpected '}' in string literal"));
            } else {
                current.push(ch);
            }
        }

        if !current.is_empty() {
            parts.push(StringPart::Literal(current));
        }

        if parts.len() == 1 {
            if let StringPart::Literal(s) = &parts[0] {
                return Ok(Expr::StringLit(s.clone()));
            }
        }

        Ok(Expr::StringInterp(parts))
    }

    /// Parse one `{expr}` of an interpolated string. `token` is the string
    /// literal's token index and `value_start` the index (in the string's
    /// value, after escape processing) of the expression's first char;
    /// both are used only to place the expression's names in the index.
    fn parse_interpolation_expr(
        &mut self,
        expr_source: &str,
        token: usize,
        value_start: usize,
    ) -> Result<Expr, ParseError> {
        let mut lexer = Lexer::new(expr_source);
        let tokens = lexer.tokenize().map_err(|e| {
            self.error(&format!(
                "invalid interpolation expression '{{{}}}': {}",
                expr_source, e.message
            ))
        })?;

        let mut parser = if self.index.is_some() {
            Parser::with_index(tokens, expr_source)
        } else {
            Parser::new(tokens)
        };
        let expr = parser.parse_expr().map_err(|e| {
            self.error(&format!(
                "invalid interpolation expression '{{{}}}': {}",
                expr_source, e.message
            ))
        })?;
        parser.skip_newlines();
        if !parser.is_at_end() {
            return Err(self.error(&format!(
                "invalid interpolation expression '{{{}}}': trailing tokens",
                expr_source
            )));
        }

        if let Some(nested) = parser.take_index() {
            let columns = self.string_value_columns(token);
            let line = self.tokens.get(token).map_or(0, |t| t.line);
            let map = |pos: Pos| {
                // Nested positions are 1-based within `expr_source`, which
                // is one line (string literals cannot span lines).
                let value_index = value_start + pos.col.saturating_sub(1);
                let col = columns
                    .get(value_index)
                    .copied()
                    .or_else(|| columns.last().map(|c| c + 1))
                    .unwrap_or(0);
                Pos::new(line, col)
            };
            if let Some(ix) = self.index.as_mut() {
                ix.merge_nested(nested, &map);
            }
        }

        Ok(expr)
    }

    /// Source column of each char of a string literal's value (plus one
    /// past the end), accounting for two-char escapes like `\"`.
    fn string_value_columns(&self, token: usize) -> Vec<usize> {
        let (Some(source), Some(tok)) = (self.source.as_ref(), self.tokens.get(token)) else {
            return Vec::new();
        };
        let raw = source.get(tok.offset..tok.offset + tok.len).unwrap_or(&[]);
        let mut columns = Vec::new();
        // Skip the opening quote.
        let mut i = 1;
        while i + 1 < raw.len() {
            columns.push(tok.col + i);
            i += if raw[i] == '\\' { 2 } else { 1 };
        }
        columns.push(tok.col + i);
        columns
    }

    fn parse_object_or_block(&mut self) -> Result<Expr, ParseError> {
        self.expect(Token::LBrace)?;
        self.skip_newlines();

        // Empty braces = empty object
        if self.check(&Token::RBrace) {
            self.advance();
            return Ok(Expr::Object(Vec::new()));
        }

        // Peek ahead: if we see `ident:` or `"string":` it's an object literal
        if matches!(self.current_token(), Token::Ident(_) | Token::StringLit(_)) {
            let saved = self.pos;
            self.advance();
            if self.check(&Token::Colon) {
                self.pos = saved;
                return self.parse_object_fields();
            }
            self.pos = saved;
        }

        // Otherwise it's a block
        let start = self.prev_span().start;
        self.scoped(ScopeKind::Block, start, |p| {
            let mut stmts = Vec::new();
            while !p.check(&Token::RBrace) {
                let (line, col) = p.current_pos();
                let stmt = p.parse_statement()?;
                stmts.push(SpannedStmt::new(stmt, line, col));
                p.skip_newlines();
            }
            p.expect(Token::RBrace)?;
            Ok(Expr::Block(stmts))
        })
    }

    fn parse_object_fields(&mut self) -> Result<Expr, ParseError> {
        let mut fields = Vec::new();

        while !self.check(&Token::RBrace) {
            let key = match self.current_token() {
                Token::StringLit(s) => {
                    let s = s;
                    self.advance();
                    s
                }
                _ => {
                    let key = self.expect_ident()?;
                    self.note_prev(&key, Role::Field);
                    key
                }
            };
            self.expect(Token::Colon)?;
            let value = self.parse_expr()?;
            fields.push((key, value));
            self.skip_newlines();
            if self.check(&Token::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }

        self.expect(Token::RBrace)?;
        Ok(Expr::Object(fields))
    }

    // ========== Helpers ==========

    fn parse_block(&mut self) -> Result<Vec<SpannedStmt>, ParseError> {
        self.skip_newlines();
        self.expect(Token::LBrace)?;
        let start = self.prev_span().start;
        self.scoped(ScopeKind::Block, start, Self::parse_block_rest)
    }

    /// The statements of a block whose `{` was consumed, through its `}`.
    fn parse_block_rest(&mut self) -> Result<Vec<SpannedStmt>, ParseError> {
        self.skip_newlines();

        let mut stmts = Vec::new();
        while !self.check(&Token::RBrace) {
            let (line, col) = self.current_pos();
            let stmt = self.parse_statement()?;
            stmts.push(SpannedStmt::new(stmt, line, col));
            self.skip_newlines();
        }

        self.expect(Token::RBrace)?;
        Ok(stmts)
    }

    /// Parameters, visible from the start of the enclosing (function or
    /// lambda) scope.
    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        while !self.check(&Token::RParen) {
            let name = self.expect_ident()?;
            let def = self.note_def(&name, DefKind::Parameter);
            self.hoist(def);

            let type_ann = if self.check(&Token::Colon) {
                self.advance();
                Some(self.parse_type_ann()?)
            } else {
                None
            };

            let default = if self.check(&Token::Eq) {
                self.advance();
                Some(self.parse_expr()?)
            } else {
                None
            };

            params.push(Param {
                name,
                type_ann,
                default,
            });

            if self.check(&Token::Comma) {
                self.advance();
            }
        }
        Ok(params)
    }

    fn parse_call_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        self.skip_newlines();
        while !self.check(&Token::RParen) {
            args.push(self.parse_expr()?);
            self.skip_newlines();
            if self.check(&Token::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }
        Ok(args)
    }

    fn parse_type_ann(&mut self) -> Result<TypeAnn, ParseError> {
        self.nested(Self::parse_type_ann_inner)
    }

    /// A type annotation that must be the whole input (see
    /// `parser::parse_type_annotation`).
    pub(crate) fn parse_standalone_type(&mut self) -> Result<TypeAnn, ParseError> {
        let ann = self.parse_type_ann()?;
        self.skip_newlines();
        if !self.is_at_end() {
            return Err(self.error(&format!("unexpected {:?} after type", self.current_token())));
        }
        Ok(ann)
    }

    fn parse_type_ann_inner(&mut self) -> Result<TypeAnn, ParseError> {
        match self.current_token() {
            Token::LBracket => {
                self.advance();
                let inner = self.parse_type_ann()?;
                self.expect(Token::RBracket)?;
                Ok(TypeAnn::Array(Box::new(inner)))
            }
            Token::Question => {
                self.advance();
                let inner = self.parse_type_ann()?;
                Ok(TypeAnn::Optional(Box::new(inner)))
            }
            Token::Ident(ref name) if matches!(self.current_token(), Token::Ident(_)) => {
                let name = name.clone();
                self.advance();
                self.note_prev(&name, Role::TypeRef);
                if self.check(&Token::Lt) {
                    self.advance();
                    let mut type_args = Vec::new();
                    while !self.check_type_args_close() {
                        type_args.push(self.parse_type_ann()?);
                        if self.check(&Token::Comma) {
                            self.advance();
                        }
                    }
                    self.expect_type_args_close()?;
                    Ok(TypeAnn::Generic(name, type_args))
                } else {
                    Ok(TypeAnn::Simple(name))
                }
            }
            // Function type: fn(A, B) -> R. Without `-> R` the function's
            // result is unconstrained (`Any`).
            Token::Fn => {
                self.advance();
                self.expect(Token::LParen)?;
                let mut params = Vec::new();
                while !self.check(&Token::RParen) {
                    params.push(self.parse_type_ann()?);
                    if self.check(&Token::Comma) {
                        self.advance();
                    } else if !self.check(&Token::RParen) {
                        return Err(self.error(&format!(
                            "expected ',' or ')' in function type, got {:?}",
                            self.current_token()
                        )));
                    }
                }
                self.expect(Token::RParen)?;
                let ret = if self.check(&Token::Arrow) {
                    self.advance();
                    self.parse_type_ann()?
                } else {
                    TypeAnn::Simple("Any".into())
                };
                Ok(TypeAnn::Function(params, Box::new(ret)))
            }
            // Tuple type (A, B), or a parenthesized type (A).
            Token::LParen => {
                self.advance();
                let mut items = Vec::new();
                let mut trailing_comma = false;
                while !self.check(&Token::RParen) {
                    items.push(self.parse_type_ann()?);
                    trailing_comma = false;
                    if self.check(&Token::Comma) {
                        self.advance();
                        trailing_comma = true;
                    } else if !self.check(&Token::RParen) {
                        return Err(self.error(&format!(
                            "expected ',' or ')' in tuple type, got {:?}",
                            self.current_token()
                        )));
                    }
                }
                self.expect(Token::RParen)?;
                if items.len() == 1 && !trailing_comma {
                    Ok(items.pop().unwrap_or(TypeAnn::Tuple(Vec::new())))
                } else {
                    Ok(TypeAnn::Tuple(items))
                }
            }
            Token::NullLit => {
                self.advance();
                Ok(TypeAnn::Simple("Null".into()))
            }
            Token::IntType => {
                self.advance();
                Ok(TypeAnn::Simple("Int".into()))
            }
            Token::FloatType => {
                self.advance();
                Ok(TypeAnn::Simple("Float".into()))
            }
            Token::StringType => {
                self.advance();
                Ok(TypeAnn::Simple("String".into()))
            }
            Token::BoolType => {
                self.advance();
                Ok(TypeAnn::Simple("Bool".into()))
            }
            Token::JsonType => {
                self.advance();
                Ok(TypeAnn::Simple("Json".into()))
            }
            _ => Err(self.error(&format!("expected type, got {:?}", self.current_token()))),
        }
    }

    // ========== Token Navigation ==========

    fn current_token(&self) -> Token {
        self.tokens
            .get(self.pos)
            .map(|s| s.token.clone())
            .unwrap_or(Token::Eof)
    }

    fn peek_token(&self, offset: usize) -> Token {
        self.tokens
            .get(self.pos + offset)
            .map(|s| s.token.clone())
            .unwrap_or(Token::Eof)
    }

    /// At the `>` closing a type argument list. `>>` (lexed as one token)
    /// closes two nested lists: `Option<Option<Int>>`.
    fn check_type_args_close(&self) -> bool {
        self.check(&Token::Gt) || self.check(&Token::PipeRight)
    }

    fn expect_type_args_close(&mut self) -> Result<(), ParseError> {
        if self.check(&Token::PipeRight) {
            // Consume the first `>` of `>>`: the token becomes the second.
            let tok = &mut self.tokens[self.pos];
            tok.token = Token::Gt;
            tok.col += 1;
            tok.offset += 1;
            tok.len = 1;
            return Ok(());
        }
        self.expect(Token::Gt)
    }

    fn is_type_definition_start(&self) -> bool {
        matches!(self.current_token(), Token::Type)
            && matches!(self.peek_token(1), Token::Ident(_))
            && matches!(self.peek_token(2), Token::Eq)
    }

    fn check(&self, expected: &Token) -> bool {
        std::mem::discriminant(&self.current_token()) == std::mem::discriminant(expected)
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), ParseError> {
        if self.check(&expected) {
            self.advance();
            Ok(())
        } else {
            Err(self.error(&format!(
                "expected {:?}, got {:?}",
                expected,
                self.current_token()
            )))
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        let name = match self.current_token() {
            Token::Ident(name) => Some(name),
            // Allow keywords as field/method names
            Token::Set => Some("set".to_string()),
            Token::Type => Some("type".to_string()),
            Token::Match => Some("match".to_string()),
            Token::From => Some("from".to_string()),
            Token::To => Some("to".to_string()),
            Token::In => Some("in".to_string()),
            Token::Import => Some("import".to_string()),
            Token::Table => Some("table".to_string()),
            Token::Check => Some("check".to_string()),
            Token::Where => Some("where".to_string()),
            Token::Watch => Some("watch".to_string()),
            Token::Transform => Some("transform".to_string()),
            Token::Select => Some("select".to_string()),
            Token::Order => Some("order".to_string()),
            Token::Limit => Some("limit".to_string()),
            Token::Crawl => Some("crawl".to_string()),
            Token::Download => Some("download".to_string()),
            Token::Ask => Some("ask".to_string()),
            Token::Prompt => Some("prompt".to_string()),
            Token::Agent => Some("agent".to_string()),
            Token::Schedule => Some("schedule".to_string()),
            Token::Retry => Some("retry".to_string()),
            Token::Timeout => Some("timeout".to_string()),
            Token::Safe => Some("safe".to_string()),
            Token::Must => Some("must".to_string()),
            Token::Freeze => Some("freeze".to_string()),
            Token::Keep => Some("keep".to_string()),
            Token::Take => Some("take".to_string()),
            Token::Every => Some("every".to_string()),
            Token::Any => Some("any".to_string()),
            Token::By => Some("by".to_string()),
            Token::Thing => Some("thing".to_string()),
            Token::Power => Some("power".to_string()),
            Token::Give => Some("give".to_string()),
            Token::Craft => Some("craft".to_string()),
            Token::The => Some("the".to_string()),
            _ => None,
        };
        match name {
            Some(n) => {
                self.advance();
                Ok(n)
            }
            None => Err(self.error(&format!(
                "expected identifier, got {:?}",
                self.current_token()
            ))),
        }
    }

    /// Like expect_ident but also accepts type keywords (Int, Float, String, Bool, Null)
    /// for use in type definitions like `type StringOrInt = String | Int`.
    fn expect_ident_or_type_name(&mut self) -> Result<String, ParseError> {
        let name = match self.current_token() {
            Token::IntType => Some("Int".to_string()),
            Token::FloatType => Some("Float".to_string()),
            Token::StringType => Some("String".to_string()),
            Token::BoolType => Some("Bool".to_string()),
            Token::NullLit => Some("Null".to_string()),
            _ => None,
        };
        if let Some(n) = name {
            self.advance();
            return Ok(n);
        }
        self.expect_ident()
    }

    fn skip_newlines(&mut self) {
        while self.check(&Token::Newline) {
            self.advance();
        }
    }

    fn check_pipe_separator(&self) -> bool {
        self.check(&Token::Bar)
    }

    fn is_at_end(&self) -> bool {
        self.check(&Token::Eof)
    }

    fn error(&self, msg: &str) -> ParseError {
        let (line, col) = self
            .tokens
            .get(self.pos)
            .map(|s| (s.line, s.col))
            .unwrap_or((0, 0));
        ParseError {
            message: msg.to_string(),
            line,
            col,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}:{}] Parse error: {}",
            self.line, self.col, self.message
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_program(input: &str) -> Program {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize().expect("lexing should succeed");
        let mut parser = Parser::new(tokens);
        parser.parse_program().expect("parsing should succeed")
    }

    fn parse_err(input: &str) -> ParseError {
        let tokens = Lexer::new(input).tokenize().expect("lexing should succeed");
        match Parser::new(tokens).parse_program() {
            Ok(_) => panic!("expected a parse error for {input:.40}"),
            Err(e) => e,
        }
    }

    #[test]
    fn pathological_nesting_is_a_parse_error_not_a_stack_overflow() {
        // Run on a small stack: the guard must trip before it overflows.
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let n = 100_000;
                for src in [
                    format!("let x = {}1{}", "(".repeat(n), ")".repeat(n)),
                    format!("let x = {}1", "-".repeat(n)),
                    format!("let x = {}{}", "[".repeat(n), "]".repeat(n)),
                    format!("{}{}", "if true { ".repeat(n), "}".repeat(n)),
                ] {
                    let e = parse_err(&src);
                    assert!(e.message.contains("nested too deeply"), "{}", e.message);
                }
                // Reasonable nesting still parses.
                let ok = format!("let x = {}1{}", "(".repeat(100), ")".repeat(100));
                parse_program(&ok);
            })
            .expect("spawn")
            .join()
            .expect("no stack overflow");
    }

    #[test]
    fn check_between_bounds_are_not_a_logical_expression() {
        for src in [
            "check x between 1 and 10",
            "check x between 1 && 10",
            "check x between lo and hi + 1",
        ] {
            let program = parse_program(src);
            match &program.statements[0].stmt {
                Stmt::CheckStmt {
                    check_kind: CheckKind::Between(lo, hi),
                    ..
                } => {
                    assert!(
                        !matches!(lo, Expr::BinOp { op: BinOp::And, .. }),
                        "{}: lower bound swallowed the separator: {:?}",
                        src,
                        lo
                    );
                    assert!(
                        matches!(lo, Expr::Int(1) | Expr::Ident(_)),
                        "{}: {:?}",
                        src,
                        lo
                    );
                    assert!(
                        matches!(hi, Expr::Int(10) | Expr::BinOp { op: BinOp::Add, .. }),
                        "{}: {:?}",
                        src,
                        hi
                    );
                }
                other => panic!("{}: expected check between, got {:?}", src, other),
            }
        }
    }

    #[test]
    fn check_between_requires_and() {
        let mut lexer = Lexer::new("check x between 1 10");
        let tokens = lexer.tokenize().expect("lexing should succeed");
        let err = Parser::new(tokens)
            .parse_program()
            .expect_err("missing `and` must be a parse error");
        assert!(err.message.contains("expected 'and'"), "{}", err.message);
    }

    #[test]
    fn parses_expression_interpolation() {
        let program = parse_program(r#"let msg = "sum = {a + b}""#);
        let spanned = program.statements.first().expect("expected one statement");

        match &spanned.stmt {
            Stmt::Let { value, .. } => match value {
                Expr::StringInterp(parts) => {
                    assert_eq!(parts.len(), 2);
                    match &parts[0] {
                        StringPart::Literal(s) => assert_eq!(s, "sum = "),
                        _ => panic!("first part should be literal"),
                    }
                    match &parts[1] {
                        StringPart::Expr(Expr::BinOp { op, .. }) => assert_eq!(*op, BinOp::Add),
                        other => panic!("expected binary expression, got {:?}", other),
                    }
                }
                other => panic!("expected interpolated string, got {:?}", other),
            },
            _ => panic!("expected let statement"),
        }
    }

    #[test]
    fn parses_field_access_interpolation() {
        let program = parse_program(r#"let msg = "name = {user.name}""#);
        let spanned = program.statements.first().expect("expected one statement");

        match &spanned.stmt {
            Stmt::Let { value, .. } => match value {
                Expr::StringInterp(parts) => {
                    assert_eq!(parts.len(), 2);
                    match &parts[1] {
                        StringPart::Expr(Expr::FieldAccess { field, .. }) => {
                            assert_eq!(field, "name")
                        }
                        other => panic!("expected field access expression, got {:?}", other),
                    }
                }
                other => panic!("expected interpolated string, got {:?}", other),
            },
            _ => panic!("expected let statement"),
        }
    }

    #[test]
    fn rejects_invalid_interpolation_expression() {
        let mut lexer = Lexer::new(r#"let msg = "{a + }""#);
        let tokens = lexer.tokenize().expect("lexing should succeed");
        let mut parser = Parser::new(tokens);
        let err = parser.parse_program().expect_err("parsing should fail");
        assert!(err.message.contains("invalid interpolation expression"));
    }

    #[test]
    fn parses_where_filter_expression() {
        let program = parse_program("let adults = users where age >= 18");
        let spanned = program.statements.first().expect("expected one statement");

        match &spanned.stmt {
            Stmt::Let { value, .. } => match value {
                Expr::WhereFilter {
                    field, op, value, ..
                } => {
                    assert_eq!(field, "age");
                    assert_eq!(*op, BinOp::GtEq);
                    assert!(matches!(value.as_ref(), Expr::Int(18)));
                }
                other => panic!("expected where filter, got {:?}", other),
            },
            other => panic!("expected let statement, got {:?}", other),
        }
    }

    #[test]
    fn parses_query_pipe_chain_expression() {
        let program = parse_program("users >> keep where active >> sort by name >> take 2");
        let spanned = program.statements.first().expect("expected one statement");

        match &spanned.stmt {
            Stmt::Expression(Expr::PipeChain { steps, .. }) => {
                assert_eq!(steps.len(), 3);
                assert!(matches!(steps[0], PipeStep::Keep(_)));
                assert!(matches!(steps[1], PipeStep::Sort(Some(ref field)) if field == "name"));
                assert!(matches!(steps[2], PipeStep::Take(_)));
            }
            other => panic!("expected pipe chain expression, got {:?}", other),
        }
    }

    #[test]
    fn parses_prompt_definition_statement() {
        let program = parse_program(
            r#"
            prompt summarize(text) {
                system: "You are concise"
                user: "Summarize: {text}"
                returns: "summary"
            }
            "#,
        );

        match &program.statements[0].stmt {
            Stmt::PromptDef {
                name,
                params,
                system,
                user_template,
                returns,
            } => {
                assert_eq!(name, "summarize");
                assert_eq!(params.len(), 1);
                assert_eq!(system, "You are concise");
                assert_eq!(user_template, "Summarize: {text}");
                assert_eq!(returns.as_deref(), Some("summary"));
            }
            other => panic!("expected prompt definition, got {:?}", other),
        }
    }

    #[test]
    fn parses_agent_definition_statement() {
        let program = parse_program(
            r#"
            agent researcher(topic) {
                tools: ["search", "read"]
                goal: "Research {topic}"
                max_steps: 5
            }
            "#,
        );

        match &program.statements[0].stmt {
            Stmt::AgentDef {
                name,
                params,
                tools,
                goal,
                max_steps,
            } => {
                assert_eq!(name, "researcher");
                assert_eq!(params.len(), 1);
                assert_eq!(tools, &vec!["search".to_string(), "read".to_string()]);
                assert_eq!(goal, "Research {topic}");
                assert_eq!(*max_steps, 5);
            }
            other => panic!("expected agent definition, got {:?}", other),
        }
    }

    #[test]
    fn parses_type_builtin_call_at_statement_start() {
        let program = parse_program("type(42)");

        match &program.statements[0].stmt {
            Stmt::Expression(Expr::Call { function, args }) => {
                assert!(matches!(function.as_ref(), Expr::Ident(name) if name == "type"));
                assert!(matches!(args.as_slice(), [Expr::Int(42)]));
            }
            other => panic!("expected type() call expression, got {:?}", other),
        }
    }

    #[test]
    fn still_parses_type_definitions() {
        let program = parse_program("type Color = Red | Blue");

        match &program.statements[0].stmt {
            Stmt::TypeDef { name, variants } => {
                assert_eq!(name, "Color");
                assert_eq!(variants.len(), 2);
            }
            other => panic!("expected type definition, got {:?}", other),
        }
    }

    // ========== 8B.1: Generic Type Parameters ==========

    #[test]
    fn parse_generic_function_single_param() {
        let program = parse_program("fn identity<T>(x: T) -> T { return x }");
        match &program.statements[0].stmt {
            Stmt::FnDef {
                name,
                type_params,
                params,
                ..
            } => {
                assert_eq!(name, "identity");
                assert_eq!(type_params, &vec!["T".to_string()]);
                assert_eq!(params.len(), 1);
            }
            other => panic!("expected FnDef, got {:?}", other),
        }
    }

    #[test]
    fn parse_generic_function_multiple_params() {
        let program = parse_program("fn map<T, U>(arr: [T], b: U) -> [U] { return arr }");
        match &program.statements[0].stmt {
            Stmt::FnDef {
                name, type_params, ..
            } => {
                assert_eq!(name, "map");
                assert_eq!(type_params, &vec!["T".to_string(), "U".to_string()]);
            }
            other => panic!("expected FnDef, got {:?}", other),
        }
    }

    #[test]
    fn parse_non_generic_function_has_empty_type_params() {
        let program = parse_program("fn add(a: Int, b: Int) { return a + b }");
        match &program.statements[0].stmt {
            Stmt::FnDef { type_params, .. } => {
                assert!(type_params.is_empty());
            }
            other => panic!("expected FnDef, got {:?}", other),
        }
    }

    #[test]
    fn parse_generic_struct() {
        let program = parse_program("struct Pair<T> {\n  first: T\n  second: T\n}");
        match &program.statements[0].stmt {
            Stmt::StructDef {
                name,
                type_params,
                fields,
                ..
            } => {
                assert_eq!(name, "Pair");
                assert_eq!(type_params, &vec!["T".to_string()]);
                assert_eq!(fields.len(), 2);
            }
            other => panic!("expected StructDef, got {:?}", other),
        }
    }

    #[test]
    fn parse_non_generic_struct_has_empty_type_params() {
        let program = parse_program("struct Point {\n  x: Int\n  y: Int\n}");
        match &program.statements[0].stmt {
            Stmt::StructDef { type_params, .. } => {
                assert!(type_params.is_empty());
            }
            other => panic!("expected StructDef, got {:?}", other),
        }
    }

    #[test]
    fn parses_tuple_literal() {
        let program = parse_program("let t = (1, 2, 3)");
        let spanned = program.statements.first().unwrap();
        match &spanned.stmt {
            Stmt::Let { value, .. } => match value {
                Expr::Tuple(items) => assert_eq!(items.len(), 3),
                other => panic!("expected Tuple, got {:?}", other),
            },
            other => panic!("expected Let, got {:?}", other),
        }
    }

    #[test]
    fn parses_single_element_tuple_with_trailing_comma() {
        let program = parse_program("let t = (42,)");
        let spanned = program.statements.first().unwrap();
        match &spanned.stmt {
            Stmt::Let { value, .. } => match value {
                Expr::Tuple(items) => {
                    assert_eq!(items.len(), 1);
                    match &items[0] {
                        Expr::Int(n) => assert_eq!(*n, 42),
                        other => panic!("expected Int, got {:?}", other),
                    }
                }
                other => panic!("expected Tuple, got {:?}", other),
            },
            other => panic!("expected Let, got {:?}", other),
        }
    }

    #[test]
    fn parses_grouping_not_tuple() {
        let program = parse_program("let x = (42)");
        let spanned = program.statements.first().unwrap();
        match &spanned.stmt {
            Stmt::Let { value, .. } => match value {
                Expr::Int(n) => assert_eq!(*n, 42),
                other => panic!("expected Int (grouping), got {:?}", other),
            },
            other => panic!("expected Let, got {:?}", other),
        }
    }

    #[test]
    fn parses_nested_tuple() {
        let program = parse_program("let t = ((1, 2), 3)");
        let spanned = program.statements.first().unwrap();
        match &spanned.stmt {
            Stmt::Let { value, .. } => match value {
                Expr::Tuple(items) => {
                    assert_eq!(items.len(), 2);
                    assert!(matches!(&items[0], Expr::Tuple(_)));
                    assert!(matches!(&items[1], Expr::Int(3)));
                }
                other => panic!("expected Tuple, got {:?}", other),
            },
            other => panic!("expected Let, got {:?}", other),
        }
    }

    #[test]
    fn parses_tuple_destructure_with_let() {
        let program = parse_program("let (a, b) = expr");
        let spanned = program.statements.first().unwrap();
        match &spanned.stmt {
            Stmt::Destructure { pattern, .. } => match pattern {
                DestructurePattern::Tuple(names) => {
                    assert_eq!(names, &["a", "b"]);
                }
                other => panic!("expected Tuple pattern, got {:?}", other),
            },
            other => panic!("expected Destructure, got {:?}", other),
        }
    }

    // ========== Type annotations ==========

    fn let_type(input: &str) -> TypeAnn {
        match &parse_program(input).statements[0].stmt {
            Stmt::Let {
                type_ann: Some(t), ..
            } => t.clone(),
            other => panic!("expected annotated let, got {:?}", other),
        }
    }

    #[test]
    fn parses_function_type_annotations() {
        assert_eq!(
            let_type("let f: fn(Int, String) -> Bool = g"),
            TypeAnn::Function(
                vec![
                    TypeAnn::Simple("Int".into()),
                    TypeAnn::Simple("String".into())
                ],
                Box::new(TypeAnn::Simple("Bool".into()))
            )
        );
        // No arrow: the result is unconstrained.
        assert_eq!(
            let_type("let f: fn() = g"),
            TypeAnn::Function(vec![], Box::new(TypeAnn::Simple("Any".into())))
        );
        // Function types nest.
        assert!(matches!(
            let_type("let f: fn(fn(Int) -> Int) -> [Int] = g"),
            TypeAnn::Function(params, _) if matches!(params[0], TypeAnn::Function(..))
        ));
    }

    #[test]
    fn parses_tuple_and_nested_generic_annotations() {
        assert_eq!(
            let_type("let t: (Int, String) = x"),
            TypeAnn::Tuple(vec![
                TypeAnn::Simple("Int".into()),
                TypeAnn::Simple("String".into())
            ])
        );
        assert_eq!(let_type("let t: (Int) = x"), TypeAnn::Simple("Int".into()));
        // `>>` closes two type argument lists.
        assert_eq!(
            let_type("let o: Option<Option<Int>> = x"),
            TypeAnn::Generic(
                "Option".into(),
                vec![TypeAnn::Generic(
                    "Option".into(),
                    vec![TypeAnn::Simple("Int".into())]
                )]
            )
        );
        assert!(crate::parser::parse_type_annotation("fn(Int) -> ").is_err());
        assert!(crate::parser::parse_type_annotation("[Int] extra").is_err());
    }

    // ========== Syntax index ==========

    fn indexed(input: &str) -> super::super::index::SyntaxIndex {
        let tokens = Lexer::new(input).tokenize().expect("lexes");
        let mut parser = Parser::with_index(tokens, input);
        parser.parse_program().expect("parses");
        parser.take_index().expect("index built")
    }

    fn roles(ix: &super::super::index::SyntaxIndex) -> Vec<(String, usize, usize, String)> {
        ix.occurrences
            .iter()
            .map(|o| {
                let role = match &o.role {
                    Role::Def(k) => format!("def:{:?}", k),
                    Role::Ref => "ref".into(),
                    Role::TypeRef => "type".into(),
                    Role::Field => "field".into(),
                };
                (o.name.clone(), o.span.start.line, o.span.start.col, role)
            })
            .collect()
    }

    #[test]
    fn index_records_names_with_roles_and_positions() {
        let ix = indexed("let total: Int = add(1, 2)\nsay total.value");
        // (`Int` is a keyword token: builtin type names are not indexed.)
        let expected: Vec<(String, usize, usize, String)> = vec![
            (
                "total".into(),
                1,
                5,
                "def:Variable { mutable: false }".into(),
            ),
            ("add".into(), 1, 18, "ref".into()),
            ("total".into(), 2, 5, "ref".into()),
            ("value".into(), 2, 11, "field".into()),
        ];
        assert_eq!(roles(&ix), expected);
        // `let` is visible only after its statement.
        assert!(ix.occurrences[0].visible_from > ix.occurrences[1].span.start);
    }

    #[test]
    fn index_records_desugared_names_once() {
        // `x += 1` is `x = x + 1` in the AST but one token in the source.
        let ix = indexed("let mut x = 1\nx += 1");
        assert_eq!(ix.occurrences.iter().filter(|o| o.name == "x").count(), 2);
        // `say` is a keyword, not a recorded reference.
        let ix = indexed("say 1");
        assert!(ix.occurrences.is_empty());
    }

    #[test]
    fn index_scopes_functions_lambdas_and_blocks() {
        let ix = indexed("fn f(a) {\n  let b = fn(c) { c }\n  return b(a)\n}");
        let param = ix
            .occurrences
            .iter()
            .find(|o| o.name == "a")
            .expect("param a");
        assert_eq!(ix.scopes[param.scope].kind, ScopeKind::Function);
        let lambda_param = ix
            .occurrences
            .iter()
            .find(|o| o.name == "c")
            .expect("param c");
        assert_eq!(ix.scopes[lambda_param.scope].kind, ScopeKind::Lambda);
        // The lambda's scope ends at its closing brace, on line 2.
        assert_eq!(ix.scopes[lambda_param.scope].end.line, 2);
    }

    #[test]
    fn index_places_interpolated_names_at_their_columns() {
        let ix = indexed("let who = 1\nsay \"a\\\"b {who}\"");
        let r = ix
            .occurrences
            .iter()
            .find(|o| o.name == "who" && matches!(o.role, Role::Ref))
            .expect("interpolated ref");
        // `say "a\"b {who}"`: the escape takes two source columns.
        assert_eq!((r.span.start.line, r.span.start.col), (2, 12));
    }

    #[test]
    fn index_marks_methods_fields_and_types() {
        let ix = indexed(
            "struct P { x: Int }\nimpl P { fn get(it) { return it.x } }\nlet p = P { x: 1 }",
        );
        let kinds: Vec<String> = ix
            .occurrences
            .iter()
            .map(|o| format!("{}:{:?}", o.name, o.role))
            .collect();
        assert!(kinds.contains(&"P:Def(Struct)".to_string()), "{:?}", kinds);
        assert!(kinds.contains(&"x:Def(Field { owner: \"P\" })".to_string()));
        assert!(kinds.contains(&"get:Def(Method { owner: \"P\" })".to_string()));
        assert!(kinds.iter().filter(|k| *k == "P:TypeRef").count() == 2);
        assert!(kinds.contains(&"x:Field".to_string()));
    }

    #[test]
    fn plain_parser_builds_no_index() {
        let tokens = Lexer::new("let a = 1").tokenize().expect("lexes");
        let mut parser = Parser::new(tokens);
        parser.parse_program().expect("parses");
        assert!(parser.take_index().is_none());
    }
}
