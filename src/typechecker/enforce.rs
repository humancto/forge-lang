//! Runtime enforcement of type annotations (`forge run --strict`).
//!
//! Static checking cannot see every value (anything typed `Any` flows
//! freely), so under `--strict` the front end also *instruments* the
//! program: every function or lambda with annotated parameters checks its
//! arguments on entry, and every function with a declared return type
//! checks each value it returns. The checks are ordinary calls of the hidden
//! stdlib member `__types.check(value, type, context)`, so the instrumented
//! program runs unchanged on either engine — neither the interpreter nor the
//! VM knows about annotations, and the rule itself lives once in
//! `semantics::types`.
//!
//! Instrumentation never changes what a correct program does: checks return
//! their value, argument checks are inserted before the body (an empty body
//! keeps returning `null`), and `Any`, generic parameters and interfaces
//! are not checked at run time.

use super::infer::Checker;
use super::types::Ty;
use crate::parser::ast::*;

/// Name of the hidden module (see `stdlib::types_module`).
pub const MODULE: &str = "__types";

/// Instrument `program` in place.
pub fn instrument(program: &mut Program) {
    let mut env = Checker::new(false, None, None);
    env.collect_declarations(&program.statements);
    let mut ins = Instrumenter { env };
    ins.stmts(&mut program.statements);
}

struct Instrumenter<'a> {
    env: Checker<'a>,
}

/// `__types.check(value, "<type>", "<context>")`.
fn check_call(value: Expr, ty: &str, context: &str) -> Expr {
    Expr::Call {
        function: Box::new(Expr::FieldAccess {
            object: Box::new(Expr::Ident(MODULE.to_string())),
            field: "check".to_string(),
        }),
        args: vec![
            value,
            Expr::StringLit(ty.to_string()),
            Expr::StringLit(context.to_string()),
        ],
    }
}

impl Instrumenter<'_> {
    /// The canonical runtime notation for `ty`, or `None` when nothing
    /// needs checking.
    fn canonical(&self, ty: &Ty) -> Option<String> {
        let inner = |t: &Ty| self.canonical(t).unwrap_or_else(|| "Any".to_string());
        Some(match ty {
            Ty::Any | Ty::Never | Ty::Var(_) | Ty::Param(_) => return None,
            Ty::Int => "Int".into(),
            Ty::Float => "Float".into(),
            Ty::String => "String".into(),
            Ty::Bool => "Bool".into(),
            Ty::Null => "Null".into(),
            Ty::Object => "Object".into(),
            Ty::Fn(_) => "Fn".into(),
            Ty::Array(e) => format!("[{}]", inner(e)),
            Ty::Set(e) => format!("Set<{}>", inner(e)),
            Ty::Map(k, v) => format!("Map<{},{}>", inner(k), inner(v)),
            Ty::Result(o, e) => format!("Result<{},{}>", inner(o), inner(e)),
            Ty::Option(t) => format!("?{}", self.canonical(t)?),
            Ty::Tuple(items) => format!(
                "({})",
                items.iter().map(inner).collect::<Vec<_>>().join(",")
            ),
            Ty::Union(members) => {
                let parts: Option<Vec<String>> =
                    members.iter().map(|m| self.canonical(m)).collect();
                format!("Union<{}>", parts?.join(","))
            }
            Ty::Named(name, _) if self.env.is_interface(name) => return None,
            Ty::Named(name, _) => format!("Named:{}", name),
        })
    }

    fn stmts(&mut self, stmts: &mut [SpannedStmt]) {
        for s in stmts.iter_mut() {
            let (line, col) = (s.line, s.col);
            self.stmt(&mut s.stmt, line, col);
        }
    }

    fn stmt(&mut self, stmt: &mut Stmt, line: usize, col: usize) {
        match stmt {
            Stmt::FnDef {
                name,
                type_params,
                params,
                return_type,
                body,
                ..
            } => {
                self.stmts(body);
                let label = name.clone();
                self.function(
                    &label,
                    type_params,
                    params,
                    return_type.as_ref(),
                    body,
                    line,
                    col,
                );
            }
            Stmt::ImplBlock {
                type_name, methods, ..
            } => {
                for m in methods.iter_mut() {
                    let (line, col) = (m.line, m.col);
                    if let Stmt::FnDef {
                        name,
                        type_params,
                        params,
                        return_type,
                        body,
                        ..
                    } = &mut m.stmt
                    {
                        self.stmts(body);
                        let label = format!("{}.{}", type_name, name);
                        self.function(
                            &label,
                            type_params,
                            params,
                            return_type.as_ref(),
                            body,
                            line,
                            col,
                        );
                    }
                }
            }
            Stmt::Let { value, .. }
            | Stmt::Destructure { value, .. }
            | Stmt::YieldStmt(value)
            | Stmt::Expression(value) => self.expr(value),
            Stmt::Assign { target, value } => {
                self.expr(target);
                self.expr(value);
            }
            Stmt::Return(Some(e)) => self.expr(e),
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                self.expr(condition);
                self.stmts(then_body);
                if let Some(b) = else_body {
                    self.stmts(b);
                }
            }
            Stmt::Match { subject, arms } => {
                self.expr(subject);
                for arm in arms {
                    self.stmts(&mut arm.body);
                }
            }
            Stmt::For { iterable, body, .. } => {
                self.expr(iterable);
                self.stmts(body);
            }
            Stmt::While { condition, body } => {
                self.expr(condition);
                self.stmts(body);
            }
            Stmt::Loop { body }
            | Stmt::Spawn { body }
            | Stmt::Squad { body }
            | Stmt::SafeBlock { body } => self.stmts(body),
            Stmt::TryCatch {
                try_body,
                catch_body,
                ..
            } => {
                self.stmts(try_body);
                self.stmts(catch_body);
            }
            Stmt::When { subject, arms } => {
                self.expr(subject);
                for arm in arms {
                    if let Some(v) = &mut arm.value {
                        self.expr(v);
                    }
                    self.expr(&mut arm.result);
                }
            }
            Stmt::TimeoutBlock { duration: e, body }
            | Stmt::RetryBlock { count: e, body }
            | Stmt::ScheduleBlock {
                interval: e, body, ..
            }
            | Stmt::WatchBlock { path: e, body } => {
                self.expr(e);
                self.stmts(body);
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
            Stmt::DecoratorStmt(d) => {
                for arg in &mut d.args {
                    match arg {
                        DecoratorArg::Positional(e) | DecoratorArg::Named(_, e) => self.expr(e),
                    }
                }
            }
            Stmt::Return(None)
            | Stmt::StructDef { .. }
            | Stmt::TypeDef { .. }
            | Stmt::InterfaceDef { .. }
            | Stmt::Break
            | Stmt::Continue
            | Stmt::Import { .. }
            | Stmt::ImportNative { .. }
            | Stmt::PromptDef { .. }
            | Stmt::AgentDef { .. } => {}
        }
    }

    fn expr(&mut self, expr: &mut Expr) {
        match expr {
            Expr::Lambda { params, body } => {
                self.stmts(body);
                // Lambdas cannot declare a return type.
                self.function("fn", &[], params, None, body, 0, 0);
            }
            Expr::StringInterp(parts) => {
                for p in parts {
                    if let StringPart::Expr(e) = p {
                        self.expr(e);
                    }
                }
            }
            Expr::Object(fields) | Expr::StructInit { fields, .. } => {
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
            Expr::Spawn(body) | Expr::Squad(body) | Expr::Block(body) => self.stmts(body),
            Expr::Int(_) | Expr::Float(_) | Expr::StringLit(_) | Expr::Bool(_) | Expr::Ident(_) => {
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn function(
        &mut self,
        label: &str,
        type_params: &[String],
        params: &[Param],
        return_type: Option<&TypeAnn>,
        body: &mut Vec<SpannedStmt>,
        line: usize,
        col: usize,
    ) {
        if let Some(ann) = return_type {
            let ty = self.env.lower_with(ann, type_params);
            if let Some(canon) = self.canonical(&ty) {
                let context = format!("return value of '{}'", label);
                wrap_returns(body, &canon, &context);
                wrap_tail(body, &canon, &context, line, col);
            }
        }
        let mut checks = Vec::new();
        for p in params {
            let Some(ann) = &p.type_ann else { continue };
            let ty = self.env.lower_with(ann, type_params);
            if let Some(canon) = self.canonical(&ty) {
                let context = format!("argument '{}' of '{}'", p.name, label);
                checks.push(SpannedStmt::new(
                    Stmt::Expression(check_call(Expr::Ident(p.name.clone()), &canon, &context)),
                    line,
                    col,
                ));
            }
        }
        if !checks.is_empty() {
            if body.is_empty() {
                // Keep the implicit `null` result of an empty body.
                body.push(SpannedStmt::new(
                    Stmt::Expression(Expr::Ident("null".to_string())),
                    line,
                    col,
                ));
            }
            body.splice(0..0, checks);
        }
    }
}

/// Wrap every `return` of this function (not of nested functions, lambdas
/// or detached bodies) in a check.
fn wrap_returns(body: &mut [SpannedStmt], ty: &str, context: &str) {
    for s in body.iter_mut() {
        wrap_returns_stmt(&mut s.stmt, ty, context);
    }
}

fn wrap_returns_stmt(stmt: &mut Stmt, ty: &str, context: &str) {
    match stmt {
        Stmt::Return(value) => {
            let v = value.take().unwrap_or(Expr::Ident("null".to_string()));
            *value = Some(check_call(v, ty, context));
        }
        Stmt::If {
            condition,
            then_body,
            else_body,
        } => {
            wrap_returns_expr(condition, ty, context);
            wrap_returns(then_body, ty, context);
            if let Some(b) = else_body {
                wrap_returns(b, ty, context);
            }
        }
        Stmt::Match { subject, arms } => {
            wrap_returns_expr(subject, ty, context);
            for arm in arms {
                wrap_returns(&mut arm.body, ty, context);
            }
        }
        Stmt::For { body, .. }
        | Stmt::While { body, .. }
        | Stmt::Loop { body }
        | Stmt::SafeBlock { body }
        | Stmt::TimeoutBlock { body, .. }
        | Stmt::RetryBlock { body, .. } => wrap_returns(body, ty, context),
        Stmt::TryCatch {
            try_body,
            catch_body,
            ..
        } => {
            wrap_returns(try_body, ty, context);
            wrap_returns(catch_body, ty, context);
        }
        Stmt::Let { value, .. } | Stmt::Expression(value) | Stmt::Assign { value, .. } => {
            wrap_returns_expr(value, ty, context)
        }
        _ => {}
    }
}

/// `return` inside a block expression (`let x = if c { return 1 } else {
/// 2 }`) returns from the function too.
fn wrap_returns_expr(expr: &mut Expr, ty: &str, context: &str) {
    if let Expr::Block(body) = expr {
        wrap_returns(body, ty, context);
    }
}

/// Check the implicit result (the tail) of a body.
fn wrap_tail(body: &mut Vec<SpannedStmt>, ty: &str, context: &str, line: usize, col: usize) {
    let Some(last) = body.last_mut() else {
        body.push(SpannedStmt::new(
            Stmt::Expression(check_call(Expr::Ident("null".to_string()), ty, context)),
            line,
            col,
        ));
        return;
    };
    match &mut last.stmt {
        Stmt::Expression(e) => {
            let value = std::mem::replace(e, Expr::Ident("null".to_string()));
            *e = check_call(value, ty, context);
        }
        Stmt::Return(_) => {}
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            wrap_tail(then_body, ty, context, line, col);
            match else_body {
                Some(b) => wrap_tail(b, ty, context, line, col),
                // The missing branch yields null: give it an explicit,
                // checked one.
                None => {
                    *else_body = Some(vec![SpannedStmt::new(
                        Stmt::Expression(check_call(Expr::Ident("null".to_string()), ty, context)),
                        line,
                        col,
                    )])
                }
            }
        }
        Stmt::Match { arms, .. } => {
            for arm in arms {
                wrap_tail(&mut arm.body, ty, context, line, col);
            }
        }
        // Other value tails (`when`, `safe`): their value is the result;
        // appending a statement would change it.
        tail if crate::semantics::is_value_tail(tail) => {}
        _ => body.push(SpannedStmt::new(
            Stmt::Expression(check_call(Expr::Ident("null".to_string()), ty, context)),
            line,
            col,
        )),
    }
}
