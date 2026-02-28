use super::bytecode::*;
use crate::parser::ast::*;

struct Local {
    name: String,
    depth: usize,
    register: u8,
    mutable: bool,
}

struct LoopContext {
    start: usize,
    break_jumps: Vec<usize>,
}

pub struct Compiler {
    chunk: Chunk,
    locals: Vec<Local>,
    scope_depth: usize,
    next_register: u8,
    max_register: u8,
    loops: Vec<LoopContext>,
}

#[derive(Debug)]
pub struct CompileError {
    pub message: String,
}

impl CompileError {
    fn new(msg: &str) -> Self {
        Self {
            message: msg.to_string(),
        }
    }
}

impl Compiler {
    fn new(name: &str) -> Self {
        Self {
            chunk: Chunk::new(name),
            locals: Vec::new(),
            scope_depth: 0,
            next_register: 0,
            max_register: 0,
            loops: Vec::new(),
        }
    }

    fn alloc_reg(&mut self) -> u8 {
        let r = self.next_register;
        self.next_register += 1;
        if self.next_register > self.max_register {
            self.max_register = self.next_register;
        }
        r
    }

    fn free_to(&mut self, target: u8) {
        self.next_register = target;
    }

    fn emit(&mut self, inst: u32, line: usize) {
        self.chunk.emit(inst, line);
    }

    fn emit_jump(&mut self, op: OpCode, a: u8, line: usize) -> usize {
        let idx = self.chunk.code_len();
        self.emit(encode_asbx(op, a, 0), line);
        idx
    }

    fn patch_jump(&mut self, offset: usize) {
        let target = self.chunk.code_len();
        self.chunk.patch_jump(offset, target);
    }

    fn emit_loop(&mut self, loop_start: usize, line: usize) {
        let current = self.chunk.code_len();
        let offset = -(current as i16 - loop_start as i16) - 1;
        self.emit(encode_asbx(OpCode::Loop, 0, offset), line);
    }

    fn const_str(&mut self, s: &str) -> u16 {
        self.chunk.add_constant(Constant::Str(s.to_string()))
    }

    fn const_int(&mut self, n: i64) -> u16 {
        self.chunk.add_constant(Constant::Int(n))
    }

    fn const_float(&mut self, n: f64) -> u16 {
        self.chunk.add_constant(Constant::Float(n))
    }

    fn begin_scope(&mut self) {
        self.scope_depth += 1;
    }

    fn end_scope(&mut self) {
        self.scope_depth -= 1;
        while let Some(local) = self.locals.last() {
            if local.depth > self.scope_depth {
                self.locals.pop();
            } else {
                break;
            }
        }
    }

    fn add_local(&mut self, name: &str, mutable: bool) -> u8 {
        let reg = self.alloc_reg();
        self.locals.push(Local {
            name: name.to_string(),
            depth: self.scope_depth,
            register: reg,
            mutable,
        });
        reg
    }

    fn resolve_local(&self, name: &str) -> Option<(u8, bool)> {
        for local in self.locals.iter().rev() {
            if local.name == name {
                return Some((local.register, local.mutable));
            }
        }
        None
    }
}

pub fn compile(program: &Program) -> Result<Chunk, CompileError> {
    let mut c = Compiler::new("<main>");
    c.begin_scope();
    for stmt in &program.statements {
        compile_stmt(&mut c, stmt)?;
    }
    c.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
    c.chunk.max_registers = c.max_register;
    Ok(c.chunk)
}

fn compile_stmt(c: &mut Compiler, stmt: &Stmt) -> Result<(), CompileError> {
    match stmt {
        Stmt::Let {
            name,
            mutable,
            value,
            ..
        } => {
            let reg = c.add_local(name, *mutable);
            compile_expr(c, value, reg)?;
            Ok(())
        }

        Stmt::Assign { target, value } => {
            match target {
                Expr::Ident(name) => {
                    if let Some((reg, mutable)) = c.resolve_local(name) {
                        if !mutable {
                            return Err(CompileError::new(&format!(
                                "cannot reassign immutable variable '{}'",
                                name
                            )));
                        }
                        compile_expr(c, value, reg)?;
                    } else {
                        let tmp = c.alloc_reg();
                        compile_expr(c, value, tmp)?;
                        let name_idx = c.const_str(name);
                        c.emit(encode_abx(OpCode::SetGlobal, tmp, name_idx), 0);
                        c.free_to(tmp);
                    }
                }
                Expr::FieldAccess { object, field } => {
                    let saved = c.next_register;
                    let obj_reg = c.alloc_reg();
                    compile_expr(c, object, obj_reg)?;
                    let val_reg = c.alloc_reg();
                    compile_expr(c, value, val_reg)?;
                    let field_idx = c.const_str(field);
                    c.emit(
                        encode_abc(OpCode::SetField, obj_reg, field_idx as u8, val_reg),
                        0,
                    );
                    c.free_to(saved);
                }
                Expr::Index { object, index } => {
                    let saved = c.next_register;
                    let obj_reg = c.alloc_reg();
                    compile_expr(c, object, obj_reg)?;
                    let idx_reg = c.alloc_reg();
                    compile_expr(c, index, idx_reg)?;
                    let val_reg = c.alloc_reg();
                    compile_expr(c, value, val_reg)?;
                    c.emit(encode_abc(OpCode::SetIndex, obj_reg, idx_reg, val_reg), 0);
                    c.free_to(saved);
                }
                _ => return Err(CompileError::new("invalid assignment target")),
            }
            Ok(())
        }

        Stmt::FnDef {
            name, params, body, ..
        } => {
            let mut fc = Compiler::new(name);
            fc.begin_scope();
            for param in params {
                fc.add_local(&param.name, true);
            }
            for s in body {
                compile_stmt(&mut fc, s)?;
            }
            fc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
            fc.chunk.max_registers = fc.max_register;
            fc.chunk.arity = params.len() as u8;

            let proto_idx = c.chunk.prototypes.len() as u16;
            c.chunk.prototypes.push(fc.chunk);

            let fn_reg = c.add_local(name, false);
            c.emit(encode_abx(OpCode::Closure, fn_reg, proto_idx), 0);
            // Also register as global for recursion and cross-scope access
            let name_idx = c.const_str(name);
            c.emit(encode_abx(OpCode::SetGlobal, fn_reg, name_idx), 0);
            Ok(())
        }

        Stmt::Return(expr) => {
            if let Some(e) = expr {
                let saved = c.next_register;
                let reg = c.alloc_reg();
                compile_expr(c, e, reg)?;
                c.emit(encode_abc(OpCode::Return, reg, 0, 0), 0);
                c.free_to(saved);
            } else {
                c.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
            }
            Ok(())
        }

        Stmt::If {
            condition,
            then_body,
            else_body,
        } => {
            let saved = c.next_register;
            let cond = c.alloc_reg();
            compile_expr(c, condition, cond)?;
            let else_jump = c.emit_jump(OpCode::JumpIfFalse, cond, 0);
            c.free_to(saved);

            c.begin_scope();
            for s in then_body {
                compile_stmt(c, s)?;
            }
            c.end_scope();

            if let Some(eb) = else_body {
                let end_jump = c.emit_jump(OpCode::Jump, 0, 0);
                c.patch_jump(else_jump);
                c.begin_scope();
                for s in eb {
                    compile_stmt(c, s)?;
                }
                c.end_scope();
                c.patch_jump(end_jump);
            } else {
                c.patch_jump(else_jump);
            }
            Ok(())
        }

        Stmt::While { condition, body } => {
            let loop_start = c.chunk.code_len();
            c.loops.push(LoopContext {
                start: loop_start,
                break_jumps: Vec::new(),
            });

            let saved = c.next_register;
            let cond = c.alloc_reg();
            compile_expr(c, condition, cond)?;
            let exit = c.emit_jump(OpCode::JumpIfFalse, cond, 0);
            c.free_to(saved);

            c.begin_scope();
            for s in body {
                compile_stmt(c, s)?;
            }
            c.end_scope();

            c.emit_loop(loop_start, 0);
            c.patch_jump(exit);

            let ctx = c.loops.pop().unwrap();
            for bj in ctx.break_jumps {
                c.patch_jump(bj);
            }
            Ok(())
        }

        Stmt::Loop { body } => {
            let loop_start = c.chunk.code_len();
            c.loops.push(LoopContext {
                start: loop_start,
                break_jumps: Vec::new(),
            });

            c.begin_scope();
            for s in body {
                compile_stmt(c, s)?;
            }
            c.end_scope();

            c.emit_loop(loop_start, 0);

            let ctx = c.loops.pop().unwrap();
            for bj in ctx.break_jumps {
                c.patch_jump(bj);
            }
            Ok(())
        }

        Stmt::For {
            var,
            iterable,
            body,
            ..
        } => {
            let saved = c.next_register;
            let arr_reg = c.alloc_reg();
            compile_expr(c, iterable, arr_reg)?;

            let idx_reg = c.alloc_reg();
            let zero = c.const_int(0);
            c.emit(encode_abx(OpCode::LoadConst, idx_reg, zero), 0);

            let loop_start = c.chunk.code_len();
            c.loops.push(LoopContext {
                start: loop_start,
                break_jumps: Vec::new(),
            });

            let len_reg = c.alloc_reg();
            c.emit(encode_abc(OpCode::Len, len_reg, arr_reg, 0), 0);
            let cond_reg = c.alloc_reg();
            c.emit(encode_abc(OpCode::Lt, cond_reg, idx_reg, len_reg), 0);
            let exit = c.emit_jump(OpCode::JumpIfFalse, cond_reg, 0);
            c.free_to(len_reg); // free len and cond temps

            c.begin_scope();
            let var_reg = c.add_local(var, false);
            c.emit(encode_abc(OpCode::GetIndex, var_reg, arr_reg, idx_reg), 0);

            for s in body {
                compile_stmt(c, s)?;
            }
            c.end_scope();

            let one = c.const_int(1);
            let one_reg = c.alloc_reg();
            c.emit(encode_abx(OpCode::LoadConst, one_reg, one), 0);
            c.emit(encode_abc(OpCode::Add, idx_reg, idx_reg, one_reg), 0);
            c.free_to(one_reg);

            c.emit_loop(loop_start, 0);
            c.patch_jump(exit);

            let ctx = c.loops.pop().unwrap();
            for bj in ctx.break_jumps {
                c.patch_jump(bj);
            }
            c.free_to(saved);
            Ok(())
        }

        Stmt::Break => {
            let j = c.emit_jump(OpCode::Jump, 0, 0);
            if let Some(ctx) = c.loops.last_mut() {
                ctx.break_jumps.push(j);
            }
            Ok(())
        }

        Stmt::Continue => {
            if let Some(ctx) = c.loops.last() {
                let start = ctx.start;
                c.emit_loop(start, 0);
            }
            Ok(())
        }

        Stmt::Match { subject, arms } => {
            let saved = c.next_register;
            let subj = c.alloc_reg();
            compile_expr(c, subject, subj)?;
            let mut end_jumps = Vec::new();

            for arm in arms {
                match &arm.pattern {
                    Pattern::Wildcard => {
                        c.begin_scope();
                        for s in &arm.body {
                            compile_stmt(c, s)?;
                        }
                        c.end_scope();
                        break;
                    }
                    Pattern::Binding(name) => {
                        c.begin_scope();
                        let vr = c.add_local(name, false);
                        c.emit(encode_abc(OpCode::Move, vr, subj, 0), 0);
                        for s in &arm.body {
                            compile_stmt(c, s)?;
                        }
                        c.end_scope();
                        break;
                    }
                    Pattern::Literal(lit) => {
                        let lr = c.alloc_reg();
                        compile_expr(c, lit, lr)?;
                        let cr = c.alloc_reg();
                        c.emit(encode_abc(OpCode::Eq, cr, subj, lr), 0);
                        let skip = c.emit_jump(OpCode::JumpIfFalse, cr, 0);
                        c.free_to(lr);

                        c.begin_scope();
                        for s in &arm.body {
                            compile_stmt(c, s)?;
                        }
                        c.end_scope();

                        let ej = c.emit_jump(OpCode::Jump, 0, 0);
                        end_jumps.push(ej);
                        c.patch_jump(skip);
                    }
                    Pattern::Constructor { name, fields } => {
                        let variant_idx = c.const_str(name);
                        let field_name = c.const_str("__variant__");
                        let vr = c.alloc_reg();
                        c.emit(encode_abc(OpCode::GetField, vr, subj, field_name as u8), 0);
                        let nr = c.alloc_reg();
                        c.emit(encode_abx(OpCode::LoadConst, nr, variant_idx), 0);
                        let cr = c.alloc_reg();
                        c.emit(encode_abc(OpCode::Eq, cr, vr, nr), 0);
                        let skip = c.emit_jump(OpCode::JumpIfFalse, cr, 0);
                        c.free_to(vr);

                        c.begin_scope();
                        for (i, fp) in fields.iter().enumerate() {
                            if let Pattern::Binding(bname) = fp {
                                let fr = c.add_local(bname, false);
                                c.emit(encode_abc(OpCode::ExtractField, fr, subj, i as u8), 0);
                            }
                        }
                        for s in &arm.body {
                            compile_stmt(c, s)?;
                        }
                        c.end_scope();

                        let ej = c.emit_jump(OpCode::Jump, 0, 0);
                        end_jumps.push(ej);
                        c.patch_jump(skip);
                    }
                }
            }
            for ej in end_jumps {
                c.patch_jump(ej);
            }
            c.free_to(saved);
            Ok(())
        }

        Stmt::Expression(expr) => {
            let saved = c.next_register;
            let reg = c.alloc_reg();
            compile_expr(c, expr, reg)?;
            c.free_to(saved);
            Ok(())
        }

        Stmt::TypeDef { .. }
        | Stmt::StructDef { .. }
        | Stmt::InterfaceDef { .. }
        | Stmt::DecoratorStmt(_) => Ok(()),

        Stmt::Destructure { .. } => Ok(()),
        Stmt::YieldStmt(_) => Ok(()),

        Stmt::When { subject, arms } => {
            let subj_reg = c.alloc_reg();
            compile_expr(c, subject, subj_reg)?;
            let mut end_jumps = Vec::new();
            for arm in arms {
                if arm.is_else {
                    let result_reg = c.alloc_reg();
                    compile_expr(c, &arm.result, result_reg)?;
                    c.free_to(result_reg);
                    break;
                }
                if let (Some(op), Some(cmp_val)) = (&arm.op, &arm.value) {
                    let cmp_reg = c.alloc_reg();
                    compile_expr(c, cmp_val, cmp_reg)?;
                    let cond_reg = c.alloc_reg();
                    let opcode = match op {
                        BinOp::Lt => OpCode::Lt,
                        BinOp::Gt => OpCode::Gt,
                        BinOp::LtEq => OpCode::LtEq,
                        BinOp::GtEq => OpCode::GtEq,
                        BinOp::Eq => OpCode::Eq,
                        BinOp::NotEq => OpCode::NotEq,
                        _ => OpCode::Eq,
                    };
                    c.emit(encode_abc(opcode, cond_reg, subj_reg, cmp_reg), 0);
                    let skip = c.emit_jump(OpCode::JumpIfFalse, cond_reg, 0);
                    let result_reg = c.alloc_reg();
                    compile_expr(c, &arm.result, result_reg)?;
                    c.free_to(result_reg);
                    end_jumps.push(c.emit_jump(OpCode::Jump, 0, 0));
                    c.patch_jump(skip);
                    c.free_to(cmp_reg);
                }
            }
            for j in end_jumps {
                c.patch_jump(j);
            }
            c.free_to(subj_reg);
            Ok(())
        }

        Stmt::CheckStmt { expr, check_kind } => {
            let val_reg = c.alloc_reg();
            compile_expr(c, expr, val_reg)?;
            c.free_to(val_reg);
            let _ = check_kind;
            Ok(())
        }

        Stmt::SafeBlock { body } => {
            c.begin_scope();
            for s in body {
                compile_stmt(c, s)?;
            }
            c.end_scope();
            Ok(())
        }

        Stmt::TimeoutBlock { duration, body } => {
            let _dur_reg = c.alloc_reg();
            compile_expr(c, duration, _dur_reg)?;
            c.free_to(_dur_reg);
            c.begin_scope();
            for s in body {
                compile_stmt(c, s)?;
            }
            c.end_scope();
            Ok(())
        }

        Stmt::RetryBlock { count, body } => {
            let _count_reg = c.alloc_reg();
            compile_expr(c, count, _count_reg)?;
            c.free_to(_count_reg);
            c.begin_scope();
            for s in body {
                compile_stmt(c, s)?;
            }
            c.end_scope();
            Ok(())
        }

        Stmt::ScheduleBlock { .. } => Ok(()),
        Stmt::WatchBlock { .. } => Ok(()),
        Stmt::PromptDef { .. } => Ok(()),
        Stmt::AgentDef { .. } => Ok(()),

        Stmt::TryCatch {
            try_body,
            catch_var,
            catch_body,
        } => {
            c.begin_scope();
            for s in try_body {
                compile_stmt(c, s)?;
            }
            c.end_scope();
            let _ = catch_var;
            let _ = catch_body;
            Ok(())
        }

        Stmt::Import { path, names } => {
            let builtin_modules = [
                "math", "fs", "io", "crypto", "db", "pg", "env", "json", "regex", "log", "term",
                "http", "csv", "exec",
            ];
            if builtin_modules.contains(&path.as_str()) {
                return Ok(());
            }
            let _ = names;
            Ok(())
        }

        Stmt::Spawn { body } => {
            let mut sc = Compiler::new("<spawn>");
            sc.begin_scope();
            for s in body {
                compile_stmt(&mut sc, s)?;
            }
            sc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
            sc.chunk.max_registers = sc.max_register;
            let proto = c.chunk.prototypes.len() as u16;
            c.chunk.prototypes.push(sc.chunk);
            let cr = c.alloc_reg();
            c.emit(encode_abx(OpCode::Closure, cr, proto), 0);
            c.emit(encode_abc(OpCode::Spawn, cr, 0, 0), 0);
            c.free_to(cr);
            Ok(())
        }
    }
}

fn compile_expr(c: &mut Compiler, expr: &Expr, dst: u8) -> Result<(), CompileError> {
    match expr {
        Expr::Int(n) => {
            let idx = c.const_int(*n);
            c.emit(encode_abx(OpCode::LoadConst, dst, idx), 0);
        }
        Expr::Float(n) => {
            let idx = c.const_float(*n);
            c.emit(encode_abx(OpCode::LoadConst, dst, idx), 0);
        }
        Expr::Bool(true) => c.emit(encode_abc(OpCode::LoadTrue, dst, 0, 0), 0),
        Expr::Bool(false) => c.emit(encode_abc(OpCode::LoadFalse, dst, 0, 0), 0),
        Expr::StringLit(s) => {
            let idx = c.const_str(s);
            c.emit(encode_abx(OpCode::LoadConst, dst, idx), 0);
        }
        Expr::Ident(name) => {
            if let Some((reg, _)) = c.resolve_local(name) {
                if reg != dst {
                    c.emit(encode_abc(OpCode::Move, dst, reg, 0), 0);
                }
            } else {
                let idx = c.const_str(name);
                c.emit(encode_abx(OpCode::GetGlobal, dst, idx), 0);
            }
        }
        Expr::BinOp { left, op, right } => {
            let saved = c.next_register;
            let lr = c.alloc_reg();
            compile_expr(c, left, lr)?;
            let rr = c.alloc_reg();
            compile_expr(c, right, rr)?;
            let opcode = match op {
                BinOp::Add => OpCode::Add,
                BinOp::Sub => OpCode::Sub,
                BinOp::Mul => OpCode::Mul,
                BinOp::Div => OpCode::Div,
                BinOp::Mod => OpCode::Mod,
                BinOp::Eq => OpCode::Eq,
                BinOp::NotEq => OpCode::NotEq,
                BinOp::Lt => OpCode::Lt,
                BinOp::Gt => OpCode::Gt,
                BinOp::LtEq => OpCode::LtEq,
                BinOp::GtEq => OpCode::GtEq,
                BinOp::And => OpCode::And,
                BinOp::Or => OpCode::Or,
            };
            c.emit(encode_abc(opcode, dst, lr, rr), 0);
            c.free_to(saved);
        }
        Expr::UnaryOp { op, operand } => {
            let saved = c.next_register;
            let sr = c.alloc_reg();
            compile_expr(c, operand, sr)?;
            let opcode = match op {
                UnaryOp::Neg => OpCode::Neg,
                UnaryOp::Not => OpCode::Not,
            };
            c.emit(encode_abc(opcode, dst, sr, 0), 0);
            c.free_to(saved);
        }
        Expr::Call { function, args } => {
            let saved = c.next_register;
            let fr = c.alloc_reg();
            compile_expr(c, function, fr)?;
            for arg in args {
                let ar = c.alloc_reg();
                compile_expr(c, arg, ar)?;
            }
            c.emit(encode_abc(OpCode::Call, fr, args.len() as u8, dst), 0);
            c.free_to(saved);
        }
        Expr::Pipeline { value, function } => {
            let saved = c.next_register;
            let fr = c.alloc_reg();
            compile_expr(c, function, fr)?;
            let ar = c.alloc_reg();
            compile_expr(c, value, ar)?;
            c.emit(encode_abc(OpCode::Call, fr, 1, dst), 0);
            c.free_to(saved);
        }
        Expr::FieldAccess { object, field } => {
            let saved = c.next_register;
            let or = c.alloc_reg();
            compile_expr(c, object, or)?;
            let fi = c.const_str(field);
            c.emit(encode_abc(OpCode::GetField, dst, or, fi as u8), 0);
            c.free_to(saved);
        }
        Expr::Index { object, index } => {
            let saved = c.next_register;
            let or = c.alloc_reg();
            compile_expr(c, object, or)?;
            let ir = c.alloc_reg();
            compile_expr(c, index, ir)?;
            c.emit(encode_abc(OpCode::GetIndex, dst, or, ir), 0);
            c.free_to(saved);
        }
        Expr::Array(items) => {
            let start = c.next_register;
            for item in items {
                let r = c.alloc_reg();
                compile_expr(c, item, r)?;
            }
            c.emit(
                encode_abc(OpCode::NewArray, dst, start, items.len() as u8),
                0,
            );
            c.free_to(start);
        }
        Expr::Object(fields) => {
            let start = c.next_register;
            for (key, val) in fields {
                let kr = c.alloc_reg();
                let ki = c.const_str(key);
                c.emit(encode_abx(OpCode::LoadConst, kr, ki), 0);
                let vr = c.alloc_reg();
                compile_expr(c, val, vr)?;
            }
            c.emit(
                encode_abc(OpCode::NewObject, dst, start, fields.len() as u8),
                0,
            );
            c.free_to(start);
        }
        Expr::StringInterp(parts) => {
            let start = c.next_register;
            for part in parts {
                let r = c.alloc_reg();
                match part {
                    StringPart::Literal(s) => {
                        let idx = c.const_str(s);
                        c.emit(encode_abx(OpCode::LoadConst, r, idx), 0);
                    }
                    StringPart::Expr(e) => compile_expr(c, e, r)?,
                }
            }
            c.emit(
                encode_abc(OpCode::Interpolate, dst, start, parts.len() as u8),
                0,
            );
            c.free_to(start);
        }
        Expr::Try(inner) => {
            let saved = c.next_register;
            let sr = c.alloc_reg();
            compile_expr(c, inner, sr)?;
            c.emit(encode_abc(OpCode::Try, dst, sr, 0), 0);
            c.free_to(saved);
        }
        Expr::Lambda { params, body } => {
            let mut lc = Compiler::new("<lambda>");
            lc.begin_scope();
            for p in params {
                lc.add_local(&p.name, true);
            }
            for s in body {
                compile_stmt(&mut lc, s)?;
            }
            lc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
            lc.chunk.max_registers = lc.max_register;
            lc.chunk.arity = params.len() as u8;
            let pi = c.chunk.prototypes.len() as u16;
            c.chunk.prototypes.push(lc.chunk);
            c.emit(encode_abx(OpCode::Closure, dst, pi), 0);
        }
        Expr::StructInit { name, fields } => {
            let start = c.next_register;
            let tkr = c.alloc_reg();
            let tki = c.const_str("__type__");
            c.emit(encode_abx(OpCode::LoadConst, tkr, tki), 0);
            let tvr = c.alloc_reg();
            let tvi = c.const_str(name);
            c.emit(encode_abx(OpCode::LoadConst, tvr, tvi), 0);
            for (key, val) in fields {
                let kr = c.alloc_reg();
                let ki = c.const_str(key);
                c.emit(encode_abx(OpCode::LoadConst, kr, ki), 0);
                let vr = c.alloc_reg();
                compile_expr(c, val, vr)?;
            }
            c.emit(
                encode_abc(OpCode::NewObject, dst, start, (fields.len() + 1) as u8),
                0,
            );
            c.free_to(start);
        }
        Expr::Block(stmts) => {
            c.begin_scope();
            for s in stmts {
                compile_stmt(c, s)?;
            }
            c.end_scope();
        }
        Expr::Await(inner) | Expr::Must(inner) | Expr::Freeze(inner) | Expr::Ask(inner) => {
            compile_expr(c, inner, dst)?;
        }
        Expr::Spread(inner) => {
            compile_expr(c, inner, dst)?;
        }
        Expr::WhereFilter { source, .. } => {
            compile_expr(c, source, dst)?;
        }
        Expr::PipeChain { source, .. } => {
            compile_expr(c, source, dst)?;
        }
        Expr::MethodCall {
            object,
            method,
            args,
        } => {
            // Desugar to function call: method(obj, args...)
            let saved = c.next_register;
            let fr = c.alloc_reg();
            let mi = c.const_str(method);
            c.emit(encode_abx(OpCode::GetGlobal, fr, mi), 0);
            let or = c.alloc_reg();
            compile_expr(c, object, or)?;
            for arg in args {
                let ar = c.alloc_reg();
                compile_expr(c, arg, ar)?;
            }
            c.emit(encode_abc(OpCode::Call, fr, (args.len() + 1) as u8, dst), 0);
            c.free_to(saved);
        }
    }
    Ok(())
}
