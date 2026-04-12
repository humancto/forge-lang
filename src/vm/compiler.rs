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

#[derive(Clone, Copy)]
enum CleanupKind {
    Handler,
    Timeout,
}

struct CleanupContext {
    kind: CleanupKind,
    loop_depth: usize,
}

#[derive(Clone)]
struct UpvalueEntry {
    name: String,
    source: UpvalueSource,
}

pub struct Compiler {
    chunk: Chunk,
    locals: Vec<Local>,
    scope_depth: usize,
    next_register: u8,
    max_register: u8,
    loops: Vec<LoopContext>,
    cleanup_contexts: Vec<CleanupContext>,
    upvalues: Vec<UpvalueEntry>,
    parent_locals: Vec<(String, u8)>,
    parent_upvalues: Vec<(String, u8)>,
    module_mode: bool,
    /// Source line currently being compiled. Top-level loops update this
    /// from `SpannedStmt::line` before each statement so any `emit` that
    /// passes `0` for the line picks up a real number instead.
    current_line: usize,
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
            cleanup_contexts: Vec::new(),
            upvalues: Vec::new(),
            parent_locals: Vec::new(),
            parent_upvalues: Vec::new(),
            module_mode: false,
            current_line: 0,
        }
    }

    fn snapshot_locals(&self) -> Vec<(String, u8)> {
        self.locals
            .iter()
            .map(|l| (l.name.clone(), l.register))
            .collect()
    }

    fn snapshot_upvalues(&self) -> Vec<(String, u8)> {
        self.upvalues
            .iter()
            .enumerate()
            .map(|(index, upvalue)| (upvalue.name.clone(), index as u8))
            .collect()
    }

    fn alloc_reg(&mut self) -> Result<u8, CompileError> {
        if self.next_register == 255 {
            return Err(CompileError::new(
                "Function too complex: uses more than 255 registers. Try splitting into smaller functions.",
            ));
        }
        let r = self.next_register;
        self.next_register += 1;
        if self.next_register > self.max_register {
            self.max_register = self.next_register;
        }
        Ok(r)
    }

    fn free_to(&mut self, target: u8) {
        self.next_register = target;
    }

    fn emit(&mut self, inst: u32, line: usize) {
        // Most call sites pass `0` because per-instruction source tracking
        // never got plumbed through. Fall back to `current_line`, which is
        // updated per top-level statement, so runtime stack traces at least
        // point at the right statement instead of always reporting line 0.
        let actual = if line == 0 { self.current_line } else { line };
        self.chunk.emit(inst, actual);
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

    fn add_local(&mut self, name: &str, mutable: bool) -> Result<u8, CompileError> {
        let reg = self.alloc_reg()?;
        self.locals.push(Local {
            name: name.to_string(),
            depth: self.scope_depth,
            register: reg,
            mutable,
        });
        Ok(reg)
    }

    fn resolve_local(&self, name: &str) -> Option<(u8, bool)> {
        for local in self.locals.iter().rev() {
            if local.name == name {
                return Some((local.register, local.mutable));
            }
        }
        None
    }

    fn resolve_upvalue(&self, name: &str) -> Option<u8> {
        for (i, uv) in self.upvalues.iter().enumerate() {
            if uv.name == name {
                return Some(i as u8);
            }
        }
        None
    }

    fn add_upvalue(&mut self, name: &str, source: UpvalueSource) -> u8 {
        if let Some(idx) = self.resolve_upvalue(name) {
            return idx;
        }
        let idx = self.upvalues.len() as u8;
        self.upvalues.push(UpvalueEntry {
            name: name.to_string(),
            source,
        });
        idx
    }

    fn resolve_in_parent(&self, name: &str) -> Option<u8> {
        for (pname, preg) in &self.parent_locals {
            if pname == name {
                return Some(*preg);
            }
        }
        None
    }

    fn resolve_parent_upvalue(&self, name: &str) -> Option<u8> {
        for (pname, upvalue_idx) in &self.parent_upvalues {
            if pname == name {
                return Some(*upvalue_idx);
            }
        }
        None
    }

    fn emit_handler_pops_for_loop_exit(&mut self) {
        let current_loop_depth = self.loops.len();
        let cleanup_kinds: Vec<CleanupKind> = self
            .cleanup_contexts
            .iter()
            .rev()
            .take_while(|ctx| ctx.loop_depth >= current_loop_depth)
            .map(|ctx| ctx.kind)
            .collect();
        for kind in cleanup_kinds {
            match kind {
                CleanupKind::Handler => self.emit(encode_abc(OpCode::PopHandler, 0, 0, 0), 0),
                CleanupKind::Timeout => self.emit(encode_abc(OpCode::PopTimeout, 0, 0, 0), 0),
            }
        }
    }
}

pub fn compile(program: &Program) -> Result<Chunk, CompileError> {
    let mut c = Compiler::new("<main>");
    c.begin_scope();
    for spanned in &program.statements {
        c.current_line = spanned.line;
        compile_stmt(&mut c, &spanned.stmt)?;
    }
    c.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
    c.chunk.max_registers = c.max_register;
    Ok(c.chunk)
}

pub fn compile_module(program: &Program) -> Result<Chunk, CompileError> {
    let mut c = Compiler::new("<module>");
    c.module_mode = true;
    c.begin_scope();
    for spanned in &program.statements {
        c.current_line = spanned.line;
        compile_stmt(&mut c, &spanned.stmt)?;
    }
    c.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
    c.chunk.max_registers = c.max_register;
    Ok(c.chunk)
}

pub fn compile_repl(program: &Program) -> Result<Chunk, CompileError> {
    let mut c = Compiler::new("<repl>");
    c.begin_scope();

    let result_reg = c.alloc_reg()?;
    let mut has_result = false;

    for spanned in &program.statements {
        c.current_line = spanned.line;
        match &spanned.stmt {
            Stmt::Expression(expr) if !is_output_expr(expr) => {
                compile_expr(&mut c, expr, result_reg)?;
                has_result = true;
            }
            _ => compile_stmt(&mut c, &spanned.stmt)?,
        }
    }

    if has_result {
        c.emit(encode_abc(OpCode::Return, result_reg, 0, 0), 0);
    } else {
        c.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
    }
    c.chunk.max_registers = c.max_register;
    Ok(c.chunk)
}

fn is_output_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Call { function, .. }
            if matches!(
                function.as_ref(),
                Expr::Ident(name)
                    if matches!(
                        name.as_str(),
                        "print" | "println" | "say" | "yell" | "whisper"
                    )
            )
    )
}

fn compile_hidden_call(
    c: &mut Compiler,
    name: &str,
    args: Vec<Expr>,
    dst: u8,
) -> Result<(), CompileError> {
    let call = Expr::Call {
        function: Box::new(Expr::Ident(name.to_string())),
        args,
    };
    compile_expr(c, &call, dst)
}

fn compile_hidden_stmt(c: &mut Compiler, name: &str, args: Vec<Expr>) -> Result<(), CompileError> {
    let saved = c.next_register;
    let reg = c.alloc_reg()?;
    compile_hidden_call(c, name, args, reg)?;
    c.free_to(saved);
    Ok(())
}

fn compile_hidden_call_from_regs(
    c: &mut Compiler,
    name: &str,
    arg_regs: &[u8],
    dst: u8,
) -> Result<(), CompileError> {
    let saved = c.next_register;
    let fn_reg = c.alloc_reg()?;
    let fn_idx = c.const_str(name);
    c.emit(encode_abx(OpCode::GetGlobal, fn_reg, fn_idx), 0);
    for &arg_reg in arg_regs {
        let slot = c.alloc_reg()?;
        c.emit(encode_abc(OpCode::Move, slot, arg_reg, 0), 0);
    }
    c.emit(
        encode_abc(OpCode::Call, fn_reg, arg_regs.len() as u8, dst),
        0,
    );
    c.free_to(saved);
    Ok(())
}

fn compile_call_from_expr_and_regs(
    c: &mut Compiler,
    function: &Expr,
    arg_regs: &[u8],
    dst: u8,
) -> Result<(), CompileError> {
    let saved = c.next_register;
    let fn_reg = c.alloc_reg()?;
    compile_expr(c, function, fn_reg)?;
    for &arg_reg in arg_regs {
        let slot = c.alloc_reg()?;
        c.emit(encode_abc(OpCode::Move, slot, arg_reg, 0), 0);
    }
    c.emit(
        encode_abc(OpCode::Call, fn_reg, arg_regs.len() as u8, dst),
        0,
    );
    c.free_to(saved);
    Ok(())
}

fn query_op_name(op: &BinOp) -> &'static str {
    match op {
        BinOp::Eq => "==",
        BinOp::NotEq => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::LtEq => "<=",
        BinOp::GtEq => ">=",
        _ => "==",
    }
}

fn compile_set_global_expr(c: &mut Compiler, name: &str, expr: Expr) -> Result<(), CompileError> {
    let saved = c.next_register;
    let reg = c.alloc_reg()?;
    compile_expr(c, &expr, reg)?;
    let name_idx = c.const_str(name);
    c.emit(encode_abx(OpCode::SetGlobal, reg, name_idx), 0);
    c.free_to(saved);
    Ok(())
}

fn type_ann_name(type_ann: &TypeAnn) -> String {
    match type_ann {
        TypeAnn::Simple(name) => name.clone(),
        TypeAnn::Array(inner) => format!("[{}]", type_ann_name(inner)),
        TypeAnn::Generic(name, args) => {
            let inner = args
                .iter()
                .map(type_ann_name)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", name, inner)
        }
        TypeAnn::Function(params, ret) => {
            let params = params
                .iter()
                .map(type_ann_name)
                .collect::<Vec<_>>()
                .join(", ");
            format!("fn({}) -> {}", params, type_ann_name(ret))
        }
        TypeAnn::Optional(inner) => format!("{}?", type_ann_name(inner)),
    }
}

fn resolve_import_path(path: &str) -> Result<std::path::PathBuf, CompileError> {
    crate::package::resolve_import(path).ok_or_else(|| {
        CompileError::new(&format!(
            "cannot import '{}': file not found (checked {0}.fg, forge_modules/{0}/main.fg)",
            path
        ))
    })
}

fn parse_import_program(path: &str) -> Result<(String, Program), CompileError> {
    let resolved = resolve_import_path(path)?;
    let source = std::fs::read_to_string(&resolved)
        .map_err(|e| CompileError::new(&format!("cannot import '{}': {}", path, e)))?;
    let mut lexer = crate::lexer::Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| CompileError::new(&format!("import '{}' lex error: {}", path, e.message)))?;
    let mut parser = crate::parser::Parser::new(tokens);
    let program = parser
        .parse_program()
        .map_err(|e| CompileError::new(&format!("import '{}' parse error: {}", path, e.message)))?;
    Ok((resolved.display().to_string(), program))
}

fn import_export_names(program: &Program) -> Vec<String> {
    program
        .statements
        .iter()
        .filter_map(|spanned| match &spanned.stmt {
            Stmt::FnDef { name, .. } | Stmt::Let { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn struct_embeds_expr(fields: &[FieldDef]) -> Expr {
    Expr::Array(
        fields
            .iter()
            .filter(|field| field.embedded)
            .map(|field| {
                Expr::Object(vec![
                    ("field".to_string(), Expr::StringLit(field.name.clone())),
                    (
                        "type".to_string(),
                        Expr::StringLit(type_ann_name(&field.type_ann)),
                    ),
                ])
            })
            .collect(),
    )
}

fn struct_defaults_expr(fields: &[FieldDef]) -> Expr {
    Expr::Object(
        fields
            .iter()
            .filter_map(|field| {
                field
                    .default
                    .as_ref()
                    .map(|default| (field.name.clone(), default.clone()))
            })
            .collect(),
    )
}

fn interface_methods_expr(methods: &[MethodSig]) -> Expr {
    Expr::Array(
        methods
            .iter()
            .map(|method| {
                Expr::Object(vec![
                    ("name".to_string(), Expr::StringLit(method.name.clone())),
                    (
                        "param_count".to_string(),
                        Expr::Int(method.params.len() as i64),
                    ),
                ])
            })
            .collect(),
    )
}

fn type_metadata_expr(name: &str, variants: &[Variant]) -> Expr {
    Expr::Object(vec![
        ("__kind__".to_string(), Expr::StringLit("type".to_string())),
        ("name".to_string(), Expr::StringLit(name.to_string())),
        (
            "variants".to_string(),
            Expr::Array(
                variants
                    .iter()
                    .map(|variant| Expr::StringLit(variant.name.clone()))
                    .collect(),
            ),
        ),
    ])
}

fn variant_object_expr(type_name: &str, variant_name: &str, field_params: &[String]) -> Expr {
    let mut fields = vec![
        (
            "__type__".to_string(),
            Expr::StringLit(type_name.to_string()),
        ),
        (
            "__variant__".to_string(),
            Expr::StringLit(variant_name.to_string()),
        ),
    ];
    for (index, param_name) in field_params.iter().enumerate() {
        fields.push((format!("_{}", index), Expr::Ident(param_name.clone())));
    }
    Expr::Object(fields)
}

fn compile_stmt(c: &mut Compiler, stmt: &Stmt) -> Result<(), CompileError> {
    match stmt {
        Stmt::Let {
            name,
            mutable,
            value,
            ..
        } => {
            let reg = c.add_local(name, *mutable)?;
            compile_expr(c, value, reg)?;
            if c.module_mode && c.scope_depth == 1 {
                let name_idx = c.const_str(name);
                c.emit(encode_abx(OpCode::SetGlobal, reg, name_idx), 0);
            }
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
                        let saved = c.next_register;
                        let tmp = c.alloc_reg()?;
                        compile_expr(c, value, tmp)?;
                        c.emit(encode_abc(OpCode::SetLocal, reg, tmp, 0), 0);
                        c.free_to(saved);
                    } else if let Some(uv_idx) = c.resolve_upvalue(name) {
                        let saved = c.next_register;
                        let tmp = c.alloc_reg()?;
                        compile_expr(c, value, tmp)?;
                        c.emit(encode_abc(OpCode::SetUpvalue, uv_idx, tmp, 0), 0);
                        c.free_to(saved);
                    } else if let Some(parent_reg) = c.resolve_in_parent(name) {
                        let uv_idx = c.add_upvalue(name, UpvalueSource::Local(parent_reg));
                        let saved = c.next_register;
                        let tmp = c.alloc_reg()?;
                        compile_expr(c, value, tmp)?;
                        c.emit(encode_abc(OpCode::SetUpvalue, uv_idx, tmp, 0), 0);
                        c.free_to(saved);
                    } else if let Some(parent_upvalue) = c.resolve_parent_upvalue(name) {
                        let uv_idx = c.add_upvalue(name, UpvalueSource::Upvalue(parent_upvalue));
                        let saved = c.next_register;
                        let tmp = c.alloc_reg()?;
                        compile_expr(c, value, tmp)?;
                        c.emit(encode_abc(OpCode::SetUpvalue, uv_idx, tmp, 0), 0);
                        c.free_to(saved);
                    } else {
                        let tmp = c.alloc_reg()?;
                        compile_expr(c, value, tmp)?;
                        let name_idx = c.const_str(name);
                        c.emit(encode_abx(OpCode::SetGlobal, tmp, name_idx), 0);
                        c.free_to(tmp);
                    }
                }
                Expr::FieldAccess { object, field } => {
                    let saved = c.next_register;
                    let obj_reg = c.alloc_reg()?;
                    compile_expr(c, object, obj_reg)?;
                    let val_reg = c.alloc_reg()?;
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
                    let obj_reg = c.alloc_reg()?;
                    compile_expr(c, object, obj_reg)?;
                    let idx_reg = c.alloc_reg()?;
                    compile_expr(c, index, idx_reg)?;
                    let val_reg = c.alloc_reg()?;
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
            let parent_locals = c.snapshot_locals();
            let parent_upvalues = c.snapshot_upvalues();

            let mut fc = Compiler::new(name);
            fc.parent_locals = parent_locals;
            fc.parent_upvalues = parent_upvalues;
            fc.current_line = c.current_line;
            fc.begin_scope();
            for param in params {
                fc.add_local(&param.name, true)?;
            }
            for s in body {
                fc.current_line = s.line;
                compile_stmt(&mut fc, &s.stmt)?;
            }
            fc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
            fc.chunk.max_registers = fc.max_register;
            fc.chunk.arity = params.len() as u8;
            fc.chunk.upvalue_count = fc.upvalues.len() as u8;

            let upvalue_sources: Vec<UpvalueSource> =
                fc.upvalues.iter().map(|u| u.source).collect();
            let proto_idx = c.chunk.prototypes.len() as u16;
            let mut proto_chunk = fc.chunk;
            proto_chunk.upvalue_sources = upvalue_sources;
            c.chunk.prototypes.push(proto_chunk);

            let fn_reg = c.add_local(name, false)?;
            c.emit(encode_abx(OpCode::Closure, fn_reg, proto_idx), 0);

            // Also register as global for recursion and cross-scope access
            let name_idx = c.const_str(name);
            c.emit(encode_abx(OpCode::SetGlobal, fn_reg, name_idx), 0);
            Ok(())
        }

        Stmt::Return(expr) => {
            if let Some(e) = expr {
                let saved = c.next_register;
                let reg = c.alloc_reg()?;
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
            let cond = c.alloc_reg()?;
            compile_expr(c, condition, cond)?;
            let else_jump = c.emit_jump(OpCode::JumpIfFalse, cond, 0);
            c.free_to(saved);

            c.begin_scope();
            for s in then_body {
                c.current_line = s.line;
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();

            if let Some(eb) = else_body {
                let end_jump = c.emit_jump(OpCode::Jump, 0, 0);
                c.patch_jump(else_jump);
                c.begin_scope();
                for s in eb {
                    c.current_line = s.line;
                    compile_stmt(c, &s.stmt)?;
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
            let cond = c.alloc_reg()?;
            compile_expr(c, condition, cond)?;
            let exit = c.emit_jump(OpCode::JumpIfFalse, cond, 0);
            c.free_to(saved);

            c.begin_scope();
            for s in body {
                c.current_line = s.line;
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();

            c.emit_loop(loop_start, 0);
            c.patch_jump(exit);

            let ctx = c
                .loops
                .pop()
                .ok_or_else(|| CompileError::new("internal: loop stack underflow in while"))?;
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
                c.current_line = s.line;
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();

            c.emit_loop(loop_start, 0);

            let ctx = c
                .loops
                .pop()
                .ok_or_else(|| CompileError::new("internal: loop stack underflow in loop"))?;
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
            let arr_reg = c.alloc_reg()?;
            compile_expr(c, iterable, arr_reg)?;

            let idx_reg = c.alloc_reg()?;
            let zero = c.const_int(0);
            c.emit(encode_abx(OpCode::LoadConst, idx_reg, zero), 0);

            let loop_start = c.chunk.code_len();
            c.loops.push(LoopContext {
                start: loop_start,
                break_jumps: Vec::new(),
            });

            let len_reg = c.alloc_reg()?;
            c.emit(encode_abc(OpCode::Len, len_reg, arr_reg, 0), 0);
            let cond_reg = c.alloc_reg()?;
            c.emit(encode_abc(OpCode::Lt, cond_reg, idx_reg, len_reg), 0);
            let exit = c.emit_jump(OpCode::JumpIfFalse, cond_reg, 0);
            c.free_to(len_reg); // free len and cond temps

            c.begin_scope();
            let var_reg = c.add_local(var, false)?;
            c.emit(encode_abc(OpCode::GetIndex, var_reg, arr_reg, idx_reg), 0);

            for s in body {
                c.current_line = s.line;
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();

            let one = c.const_int(1);
            let one_reg = c.alloc_reg()?;
            c.emit(encode_abx(OpCode::LoadConst, one_reg, one), 0);
            c.emit(encode_abc(OpCode::Add, idx_reg, idx_reg, one_reg), 0);
            c.free_to(one_reg);

            c.emit_loop(loop_start, 0);
            c.patch_jump(exit);

            let ctx = c
                .loops
                .pop()
                .ok_or_else(|| CompileError::new("internal: loop stack underflow in for"))?;
            for bj in ctx.break_jumps {
                c.patch_jump(bj);
            }
            c.free_to(saved);
            Ok(())
        }

        Stmt::Break => {
            c.emit_handler_pops_for_loop_exit();
            let j = c.emit_jump(OpCode::Jump, 0, 0);
            if let Some(ctx) = c.loops.last_mut() {
                ctx.break_jumps.push(j);
            }
            Ok(())
        }

        Stmt::Continue => {
            c.emit_handler_pops_for_loop_exit();
            if let Some(ctx) = c.loops.last() {
                let start = ctx.start;
                c.emit_loop(start, 0);
            }
            Ok(())
        }

        Stmt::Match { subject, arms } => {
            let saved = c.next_register;
            let subj = c.alloc_reg()?;
            compile_expr(c, subject, subj)?;
            let mut end_jumps = Vec::new();

            for arm in arms {
                match &arm.pattern {
                    Pattern::Wildcard => {
                        c.begin_scope();
                        for s in &arm.body {
                            c.current_line = s.line;
                            compile_stmt(c, &s.stmt)?;
                        }
                        c.end_scope();
                        break;
                    }
                    Pattern::Binding(name) => {
                        let saved = c.next_register;
                        let fn_reg = c.alloc_reg()?;
                        let fn_idx = c.const_str("__forge_binding_matches");
                        c.emit(encode_abx(OpCode::GetGlobal, fn_reg, fn_idx), 0);

                        let name_reg = c.alloc_reg()?;
                        let name_idx = c.const_str(name);
                        c.emit(encode_abx(OpCode::LoadConst, name_reg, name_idx), 0);

                        let value_reg = c.alloc_reg()?;
                        c.emit(encode_abc(OpCode::Move, value_reg, subj, 0), 0);

                        let check_reg = c.alloc_reg()?;
                        c.emit(encode_abc(OpCode::Call, fn_reg, 2, check_reg), 0);
                        let skip = c.emit_jump(OpCode::JumpIfFalse, check_reg, 0);
                        c.free_to(saved);

                        c.begin_scope();
                        let vr = c.add_local(name, false)?;
                        c.emit(encode_abc(OpCode::Move, vr, subj, 0), 0);
                        for s in &arm.body {
                            c.current_line = s.line;
                            compile_stmt(c, &s.stmt)?;
                        }
                        c.end_scope();

                        let ej = c.emit_jump(OpCode::Jump, 0, 0);
                        end_jumps.push(ej);
                        c.patch_jump(skip);
                    }
                    Pattern::Literal(lit) => {
                        let lr = c.alloc_reg()?;
                        compile_expr(c, lit, lr)?;
                        let cr = c.alloc_reg()?;
                        c.emit(encode_abc(OpCode::Eq, cr, subj, lr), 0);
                        let skip = c.emit_jump(OpCode::JumpIfFalse, cr, 0);
                        c.free_to(lr);

                        c.begin_scope();
                        for s in &arm.body {
                            c.current_line = s.line;
                            compile_stmt(c, &s.stmt)?;
                        }
                        c.end_scope();

                        let ej = c.emit_jump(OpCode::Jump, 0, 0);
                        end_jumps.push(ej);
                        c.patch_jump(skip);
                    }
                    Pattern::Constructor { name, fields } => {
                        let variant_idx = c.const_str(name);
                        let field_name = c.const_str("__variant__");
                        let vr = c.alloc_reg()?;
                        c.emit(encode_abc(OpCode::GetField, vr, subj, field_name as u8), 0);
                        let nr = c.alloc_reg()?;
                        c.emit(encode_abx(OpCode::LoadConst, nr, variant_idx), 0);
                        let cr = c.alloc_reg()?;
                        c.emit(encode_abc(OpCode::Eq, cr, vr, nr), 0);
                        let skip = c.emit_jump(OpCode::JumpIfFalse, cr, 0);
                        c.free_to(vr);

                        c.begin_scope();
                        for (i, fp) in fields.iter().enumerate() {
                            if let Pattern::Binding(bname) = fp {
                                let fr = c.add_local(bname, false)?;
                                c.emit(encode_abc(OpCode::ExtractField, fr, subj, i as u8), 0);
                            }
                        }
                        for s in &arm.body {
                            c.current_line = s.line;
                            compile_stmt(c, &s.stmt)?;
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
            let reg = c.alloc_reg()?;
            compile_expr(c, expr, reg)?;
            c.free_to(saved);
            Ok(())
        }

        Stmt::TypeDef { name, variants } => {
            for variant in variants {
                if variant.fields.is_empty() {
                    compile_set_global_expr(
                        c,
                        &variant.name,
                        variant_object_expr(name, &variant.name, &[]),
                    )?;
                    continue;
                }

                let params: Vec<Param> = variant
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(index, type_ann)| Param {
                        name: format!("field{}", index),
                        type_ann: Some(type_ann.clone()),
                        default: None,
                    })
                    .collect();
                let param_names = params
                    .iter()
                    .map(|param| param.name.clone())
                    .collect::<Vec<_>>();
                let constructor = Expr::Lambda {
                    params,
                    body: vec![SpannedStmt::unspanned(Stmt::Return(Some(
                        variant_object_expr(name, &variant.name, &param_names),
                    )))],
                };
                compile_set_global_expr(c, &variant.name, constructor)?;
            }

            compile_set_global_expr(
                c,
                &format!("__type_{}__", name),
                type_metadata_expr(name, variants),
            )
        }

        Stmt::DecoratorStmt(_) => Ok(()),

        Stmt::StructDef { name, fields } => compile_hidden_stmt(
            c,
            "__forge_register_struct",
            vec![
                Expr::StringLit(name.clone()),
                struct_embeds_expr(fields),
                struct_defaults_expr(fields),
            ],
        ),

        Stmt::InterfaceDef { name, methods } => {
            let iface = Expr::Object(vec![
                (
                    "__kind__".to_string(),
                    Expr::StringLit("interface".to_string()),
                ),
                ("name".to_string(), Expr::StringLit(name.clone())),
                ("methods".to_string(), interface_methods_expr(methods)),
            ]);
            compile_hidden_stmt(
                c,
                "__forge_register_interface",
                vec![Expr::StringLit(name.clone()), iface],
            )
        }

        Stmt::Destructure { pattern, value } => {
            let value_reg = c.alloc_reg()?;
            compile_expr(c, value, value_reg)?;

            match pattern {
                DestructurePattern::Object(names) => {
                    let target_regs: Vec<u8> = names
                        .iter()
                        .map(|name| c.add_local(name, false))
                        .collect::<Result<_, _>>()?;
                    for (name, target_reg) in names.iter().zip(target_regs.iter().copied()) {
                        let field_idx = c.const_str(name);
                        c.emit(
                            encode_abc(OpCode::GetField, target_reg, value_reg, field_idx as u8),
                            0,
                        );
                    }
                }
                DestructurePattern::Array { items, rest } => {
                    let item_regs: Vec<u8> = items
                        .iter()
                        .map(|name| c.add_local(name, false))
                        .collect::<Result<_, _>>()?;
                    let rest_reg = rest
                        .as_ref()
                        .map(|name| c.add_local(name, false))
                        .transpose()?;

                    let temp_base = c.next_register;
                    for (index, target_reg) in item_regs.iter().copied().enumerate() {
                        let idx_reg = c.alloc_reg()?;
                        let const_idx = c.const_int(index as i64);
                        c.emit(encode_abx(OpCode::LoadConst, idx_reg, const_idx), 0);
                        c.emit(
                            encode_abc(OpCode::GetIndex, target_reg, value_reg, idx_reg),
                            0,
                        );
                        c.free_to(temp_base);
                    }

                    if let Some(rest_reg) = rest_reg {
                        c.emit(encode_abc(OpCode::NewArray, rest_reg, temp_base, 0), 0);

                        let idx_reg = c.alloc_reg()?;
                        let start_idx = c.const_int(items.len() as i64);
                        c.emit(encode_abx(OpCode::LoadConst, idx_reg, start_idx), 0);

                        let loop_start = c.chunk.code_len();
                        let len_reg = c.alloc_reg()?;
                        c.emit(encode_abc(OpCode::Len, len_reg, value_reg, 0), 0);
                        let cond_reg = c.alloc_reg()?;
                        c.emit(encode_abc(OpCode::Lt, cond_reg, idx_reg, len_reg), 0);
                        let exit = c.emit_jump(OpCode::JumpIfFalse, cond_reg, 0);
                        c.free_to(len_reg);

                        let push_fn_reg = c.alloc_reg()?;
                        let push_idx = c.const_str("push");
                        c.emit(encode_abx(OpCode::GetGlobal, push_fn_reg, push_idx), 0);

                        let rest_src_reg = c.alloc_reg()?;
                        c.emit(encode_abc(OpCode::GetLocal, rest_src_reg, rest_reg, 0), 0);

                        let item_reg = c.alloc_reg()?;
                        c.emit(
                            encode_abc(OpCode::GetIndex, item_reg, value_reg, idx_reg),
                            0,
                        );

                        let updated_rest_reg = c.alloc_reg()?;
                        c.emit(
                            encode_abc(OpCode::Call, push_fn_reg, 2, updated_rest_reg),
                            0,
                        );
                        c.emit(
                            encode_abc(OpCode::SetLocal, rest_reg, updated_rest_reg, 0),
                            0,
                        );

                        let one_reg = c.alloc_reg()?;
                        let one_idx = c.const_int(1);
                        c.emit(encode_abx(OpCode::LoadConst, one_reg, one_idx), 0);
                        c.emit(encode_abc(OpCode::Add, idx_reg, idx_reg, one_reg), 0);
                        c.free_to(len_reg);

                        c.emit_loop(loop_start, 0);
                        c.patch_jump(exit);
                        c.free_to(temp_base);
                    }
                }
            }
            Ok(())
        }
        Stmt::YieldStmt(_) => Ok(()),

        Stmt::When { subject, arms } => {
            let subj_reg = c.alloc_reg()?;
            compile_expr(c, subject, subj_reg)?;
            let mut end_jumps = Vec::new();
            for arm in arms {
                if arm.is_else {
                    let result_reg = c.alloc_reg()?;
                    compile_expr(c, &arm.result, result_reg)?;
                    c.free_to(result_reg);
                    break;
                }
                if let (Some(op), Some(cmp_val)) = (&arm.op, &arm.value) {
                    let cmp_reg = c.alloc_reg()?;
                    compile_expr(c, cmp_val, cmp_reg)?;
                    let cond_reg = c.alloc_reg()?;
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
                    let result_reg = c.alloc_reg()?;
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
            let val_reg = c.alloc_reg()?;
            compile_expr(c, expr, val_reg)?;
            c.free_to(val_reg);
            let _ = check_kind;
            Ok(())
        }

        Stmt::SafeBlock { body } => {
            let saved = c.next_register;
            let err_reg = c.alloc_reg()?;
            let handler_jump = c.emit_jump(OpCode::PushHandler, err_reg, 0);
            c.cleanup_contexts.push(CleanupContext {
                kind: CleanupKind::Handler,
                loop_depth: c.loops.len(),
            });
            c.begin_scope();
            for s in body {
                c.current_line = s.line;
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();
            c.cleanup_contexts.pop();
            c.emit(encode_abc(OpCode::PopHandler, 0, 0, 0), 0);
            let end_jump = c.emit_jump(OpCode::Jump, 0, 0);
            c.patch_jump(handler_jump);
            c.patch_jump(end_jump);
            c.free_to(saved);
            Ok(())
        }

        Stmt::TimeoutBlock { duration, body } => {
            let saved = c.next_register;
            let error_reg = c.alloc_reg()?;
            compile_expr(c, duration, error_reg)?;

            let handler_jump = c.emit_jump(OpCode::PushHandler, error_reg, 0);
            c.cleanup_contexts.push(CleanupContext {
                kind: CleanupKind::Handler,
                loop_depth: c.loops.len(),
            });

            let timeout_jump = c.emit_jump(OpCode::PushTimeout, error_reg, 0);
            c.cleanup_contexts.push(CleanupContext {
                kind: CleanupKind::Timeout,
                loop_depth: c.loops.len(),
            });

            c.begin_scope();
            for s in body {
                c.current_line = s.line;
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();

            c.cleanup_contexts.pop();
            c.emit(encode_abc(OpCode::PopTimeout, 0, 0, 0), 0);
            c.cleanup_contexts.pop();
            c.emit(encode_abc(OpCode::PopHandler, 0, 0, 0), 0);
            let end_jump = c.emit_jump(OpCode::Jump, 0, 0);

            c.patch_jump(timeout_jump);
            c.patch_jump(handler_jump);
            c.emit(encode_abc(OpCode::PopTimeout, 0, 0, 0), 0);
            compile_hidden_call_from_regs(c, "__forge_raise_error", &[error_reg], error_reg)?;

            c.patch_jump(end_jump);
            c.free_to(saved);
            Ok(())
        }

        Stmt::RetryBlock { count, body } => {
            let saved = c.next_register;
            let raw_count_reg = c.alloc_reg()?;
            compile_expr(c, count, raw_count_reg)?;

            let count_reg = c.alloc_reg()?;
            compile_hidden_call_from_regs(c, "__forge_retry_count", &[raw_count_reg], count_reg)?;

            let attempt_reg = c.alloc_reg()?;
            let zero_idx = c.const_int(0);
            c.emit(encode_abx(OpCode::LoadConst, attempt_reg, zero_idx), 0);

            let error_reg = c.alloc_reg()?;
            c.emit(encode_abc(OpCode::LoadNull, error_reg, 0, 0), 0);

            let zero_cmp_reg = c.alloc_reg()?;
            c.emit(encode_abx(OpCode::LoadConst, zero_cmp_reg, zero_idx), 0);
            let can_run_reg = c.alloc_reg()?;
            c.emit(
                encode_abc(OpCode::Lt, can_run_reg, zero_cmp_reg, count_reg),
                0,
            );
            let fail_without_attempts = c.emit_jump(OpCode::JumpIfFalse, can_run_reg, 0);
            c.free_to(error_reg + 1);

            let loop_start = c.chunk.code_len();
            let handler_jump = c.emit_jump(OpCode::PushHandler, error_reg, 0);
            c.cleanup_contexts.push(CleanupContext {
                kind: CleanupKind::Handler,
                loop_depth: c.loops.len(),
            });
            c.begin_scope();
            for s in body {
                c.current_line = s.line;
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();
            c.cleanup_contexts.pop();
            c.emit(encode_abc(OpCode::PopHandler, 0, 0, 0), 0);
            let success_jump = c.emit_jump(OpCode::Jump, 0, 0);

            c.patch_jump(handler_jump);

            let one_reg = c.alloc_reg()?;
            let one_idx = c.const_int(1);
            c.emit(encode_abx(OpCode::LoadConst, one_reg, one_idx), 0);
            c.emit(
                encode_abc(OpCode::Add, attempt_reg, attempt_reg, one_reg),
                0,
            );
            c.free_to(error_reg + 1);

            let retry_cond_reg = c.alloc_reg()?;
            c.emit(
                encode_abc(OpCode::Lt, retry_cond_reg, attempt_reg, count_reg),
                0,
            );
            let fail_jump = c.emit_jump(OpCode::JumpIfFalse, retry_cond_reg, 0);
            c.free_to(error_reg + 1);

            compile_hidden_call_from_regs(c, "__forge_retry_wait", &[attempt_reg], error_reg)?;
            c.emit_loop(loop_start, 0);

            c.patch_jump(fail_without_attempts);
            c.patch_jump(fail_jump);
            compile_hidden_call_from_regs(
                c,
                "__forge_retry_failed",
                &[count_reg, error_reg],
                error_reg,
            )?;

            c.patch_jump(success_jump);
            c.free_to(saved);
            Ok(())
        }

        Stmt::ScheduleBlock {
            interval,
            unit,
            body,
        } => {
            // Compile interval expression into a register
            let interval_reg = c.alloc_reg()?;
            compile_expr(c, interval, interval_reg)?;

            // Load unit string into a register
            let unit_reg = c.alloc_reg()?;
            let unit_idx = c.const_str(unit);
            c.emit(encode_abx(OpCode::LoadConst, unit_reg, unit_idx), 0);

            // Compile body as closure (same pattern as Stmt::Spawn)
            let parent_locals = c.snapshot_locals();
            let parent_upvalues = c.snapshot_upvalues();

            let mut sc = Compiler::new("<schedule>");
            sc.parent_locals = parent_locals;
            sc.parent_upvalues = parent_upvalues;
            sc.current_line = c.current_line;
            sc.begin_scope();
            for s in body {
                sc.current_line = s.line;
                compile_stmt(&mut sc, &s.stmt)?;
            }
            sc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
            sc.chunk.upvalue_count = sc.upvalues.len() as u8;
            sc.chunk.max_registers = sc.max_register;
            let upvalue_sources: Vec<UpvalueSource> =
                sc.upvalues.iter().map(|u| u.source).collect();
            let mut proto_chunk = sc.chunk;
            proto_chunk.upvalue_sources = upvalue_sources;
            let proto = c.chunk.prototypes.len() as u16;
            c.chunk.prototypes.push(proto_chunk);
            let closure_reg = c.alloc_reg()?;
            c.emit(encode_abx(OpCode::Closure, closure_reg, proto), 0);

            // Emit Schedule opcode: A=closure, B=interval, C=unit
            c.emit(
                encode_abc(OpCode::Schedule, closure_reg, interval_reg, unit_reg),
                c.current_line,
            );
            c.free_to(interval_reg);
            Ok(())
        }

        Stmt::WatchBlock { path, body } => {
            // Compile path expression into a register
            let path_reg = c.alloc_reg()?;
            compile_expr(c, path, path_reg)?;

            // Compile body as closure (same pattern as Stmt::Spawn)
            let parent_locals = c.snapshot_locals();
            let parent_upvalues = c.snapshot_upvalues();

            let mut sc = Compiler::new("<watch>");
            sc.parent_locals = parent_locals;
            sc.parent_upvalues = parent_upvalues;
            sc.current_line = c.current_line;
            sc.begin_scope();
            for s in body {
                sc.current_line = s.line;
                compile_stmt(&mut sc, &s.stmt)?;
            }
            sc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
            sc.chunk.upvalue_count = sc.upvalues.len() as u8;
            sc.chunk.max_registers = sc.max_register;
            let upvalue_sources: Vec<UpvalueSource> =
                sc.upvalues.iter().map(|u| u.source).collect();
            let mut proto_chunk = sc.chunk;
            proto_chunk.upvalue_sources = upvalue_sources;
            let proto = c.chunk.prototypes.len() as u16;
            c.chunk.prototypes.push(proto_chunk);
            let closure_reg = c.alloc_reg()?;
            c.emit(encode_abx(OpCode::Closure, closure_reg, proto), 0);

            // Emit Watch opcode: A=closure, B=path
            c.emit(
                encode_abc(OpCode::Watch, closure_reg, path_reg, 0),
                c.current_line,
            );
            c.free_to(path_reg);
            Ok(())
        }
        Stmt::PromptDef { name, .. } => compile_hidden_stmt(
            c,
            "__forge_register_prompt",
            vec![Expr::StringLit(name.clone())],
        ),
        Stmt::AgentDef { name, .. } => compile_hidden_stmt(
            c,
            "__forge_register_agent",
            vec![Expr::StringLit(name.clone())],
        ),
        Stmt::ImplBlock {
            type_name,
            ability,
            methods,
        } => {
            for method_spanned in methods {
                let Stmt::FnDef {
                    name, params, body, ..
                } = &method_spanned.stmt
                else {
                    return Err(CompileError::new(
                        "impl/give blocks may only contain methods",
                    ));
                };
                let has_receiver = params.first().is_some_and(|param| param.name == "it");
                let function = Expr::Lambda {
                    params: params.clone(),
                    body: body.clone(),
                };
                compile_hidden_stmt(
                    c,
                    "__forge_register_method",
                    vec![
                        Expr::StringLit(type_name.clone()),
                        Expr::StringLit(name.clone()),
                        Expr::Bool(has_receiver),
                        function,
                    ],
                )?;
            }

            if let Some(ability_name) = ability {
                compile_hidden_stmt(
                    c,
                    "__forge_validate_impl",
                    vec![
                        Expr::StringLit(type_name.clone()),
                        Expr::StringLit(ability_name.clone()),
                    ],
                )?;
            }

            Ok(())
        }

        Stmt::TryCatch {
            try_body,
            catch_var,
            catch_body,
        } => {
            let catch_reg = c.alloc_reg()?;
            let handler_jump = c.emit_jump(OpCode::PushHandler, catch_reg, 0);
            c.cleanup_contexts.push(CleanupContext {
                kind: CleanupKind::Handler,
                loop_depth: c.loops.len(),
            });
            c.begin_scope();
            for s in try_body {
                c.current_line = s.line;
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();
            c.cleanup_contexts.pop();
            c.emit(encode_abc(OpCode::PopHandler, 0, 0, 0), 0);
            let end_jump = c.emit_jump(OpCode::Jump, 0, 0);

            c.patch_jump(handler_jump);
            c.begin_scope();
            c.locals.push(Local {
                name: catch_var.clone(),
                depth: c.scope_depth,
                register: catch_reg,
                mutable: false,
            });
            for s in catch_body {
                c.current_line = s.line;
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();
            c.patch_jump(end_jump);
            Ok(())
        }

        Stmt::Import { path, names } => {
            let builtin_modules = [
                "math", "fs", "io", "crypto", "db", "pg", "env", "json", "regex", "log", "term",
                "http", "csv", "exec", "time", "url", "toml", "npc", "ws", "jwt", "mysql", "os",
                "path",
            ];
            if builtin_modules.contains(&path.as_str()) {
                return Ok(());
            }

            let (resolved_path, export_names) = match names {
                Some(name_list) => (
                    resolve_import_path(path)?.display().to_string(),
                    name_list.clone(),
                ),
                None => {
                    let (resolved_path, program) = parse_import_program(path)?;
                    (resolved_path, import_export_names(&program))
                }
            };

            if export_names.is_empty() {
                return Ok(());
            }

            let import_args = match names {
                Some(name_list) => vec![
                    Expr::StringLit(resolved_path),
                    Expr::Array(
                        name_list
                            .iter()
                            .map(|name| Expr::StringLit(name.clone()))
                            .collect(),
                    ),
                ],
                None => vec![Expr::StringLit(resolved_path)],
            };

            let module_reg = c.alloc_reg()?;
            compile_hidden_call(c, "__forge_import_module", import_args, module_reg)?;
            for name in export_names {
                let local_reg = c.add_local(&name, false)?;
                let field_idx = c.const_str(&name);
                c.emit(
                    encode_abc(OpCode::GetField, local_reg, module_reg, field_idx as u8),
                    0,
                );
            }
            Ok(())
        }

        Stmt::Spawn { body } => {
            let parent_locals = c.snapshot_locals();
            let parent_upvalues = c.snapshot_upvalues();

            let mut sc = Compiler::new("<spawn>");
            sc.parent_locals = parent_locals;
            sc.parent_upvalues = parent_upvalues;
            sc.current_line = c.current_line;
            sc.begin_scope();
            for s in body {
                sc.current_line = s.line;
                compile_stmt(&mut sc, &s.stmt)?;
            }
            sc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
            sc.chunk.upvalue_count = sc.upvalues.len() as u8;
            sc.chunk.max_registers = sc.max_register;
            let upvalue_sources: Vec<UpvalueSource> =
                sc.upvalues.iter().map(|u| u.source).collect();
            let mut proto_chunk = sc.chunk;
            proto_chunk.upvalue_sources = upvalue_sources;
            let proto = c.chunk.prototypes.len() as u16;
            c.chunk.prototypes.push(proto_chunk);
            let cr = c.alloc_reg()?;
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
                c.emit(encode_abc(OpCode::GetLocal, dst, reg, 0), 0);
            } else if let Some(uv_idx) = c.resolve_upvalue(name) {
                c.emit(encode_abc(OpCode::GetUpvalue, dst, uv_idx, 0), 0);
            } else if let Some(parent_reg) = c.resolve_in_parent(name) {
                let uv_idx = c.add_upvalue(name, UpvalueSource::Local(parent_reg));
                c.emit(encode_abc(OpCode::GetUpvalue, dst, uv_idx, 0), 0);
            } else if let Some(parent_upvalue) = c.resolve_parent_upvalue(name) {
                let uv_idx = c.add_upvalue(name, UpvalueSource::Upvalue(parent_upvalue));
                c.emit(encode_abc(OpCode::GetUpvalue, dst, uv_idx, 0), 0);
            } else {
                let idx = c.const_str(name);
                c.emit(encode_abx(OpCode::GetGlobal, dst, idx), 0);
            }
        }
        Expr::BinOp { left, op, right } => {
            // Short-circuit && and || — evaluate left, conditionally skip right
            if matches!(op, BinOp::And | BinOp::Or) {
                compile_expr(c, left, dst)?;
                // Coerce left to bool via double-Not
                c.emit(encode_abc(OpCode::Not, dst, dst, 0), 0);
                c.emit(encode_abc(OpCode::Not, dst, dst, 0), 0);
                let jump_op = if matches!(op, BinOp::And) {
                    OpCode::JumpIfFalse
                } else {
                    OpCode::JumpIfTrue
                };
                let jump_pc = c.emit_jump(jump_op, dst, 0);
                compile_expr(c, right, dst)?;
                // Coerce right to bool via double-Not
                c.emit(encode_abc(OpCode::Not, dst, dst, 0), 0);
                c.emit(encode_abc(OpCode::Not, dst, dst, 0), 0);
                c.patch_jump(jump_pc);
                return Ok(());
            }
            let saved = c.next_register;
            let lr = c.alloc_reg()?;
            compile_expr(c, left, lr)?;
            let rr = c.alloc_reg()?;
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
                // And/Or handled above via short-circuit jumps
                BinOp::And | BinOp::Or => unreachable!(),
            };
            c.emit(encode_abc(opcode, dst, lr, rr), 0);
            c.free_to(saved);
        }
        Expr::UnaryOp { op, operand } => {
            let saved = c.next_register;
            let sr = c.alloc_reg()?;
            compile_expr(c, operand, sr)?;
            let opcode = match op {
                UnaryOp::Neg => OpCode::Neg,
                UnaryOp::Not => OpCode::Not,
            };
            c.emit(encode_abc(opcode, dst, sr, 0), 0);
            c.free_to(saved);
        }
        Expr::Call { function, args } => {
            if let Expr::FieldAccess { object, field } = function.as_ref() {
                let mut lowered_args = Vec::with_capacity(args.len() + 2);
                lowered_args.push((**object).clone());
                lowered_args.push(Expr::StringLit(field.clone()));
                lowered_args.extend(args.clone());
                return compile_hidden_call(c, "__forge_call_method", lowered_args, dst);
            }
            let saved = c.next_register;
            let fr = c.alloc_reg()?;
            compile_expr(c, function, fr)?;
            for arg in args {
                let ar = c.alloc_reg()?;
                compile_expr(c, arg, ar)?;
            }
            c.emit(encode_abc(OpCode::Call, fr, args.len() as u8, dst), 0);
            c.free_to(saved);
        }
        Expr::Pipeline { value, function } => {
            let saved = c.next_register;
            let fr = c.alloc_reg()?;
            compile_expr(c, function, fr)?;
            let ar = c.alloc_reg()?;
            compile_expr(c, value, ar)?;
            c.emit(encode_abc(OpCode::Call, fr, 1, dst), 0);
            c.free_to(saved);
        }
        Expr::FieldAccess { object, field } => {
            let saved = c.next_register;
            let or = c.alloc_reg()?;
            compile_expr(c, object, or)?;
            let fi = c.const_str(field);
            c.emit(encode_abc(OpCode::GetField, dst, or, fi as u8), 0);
            c.free_to(saved);
        }
        Expr::Index { object, index } => {
            let saved = c.next_register;
            let or = c.alloc_reg()?;
            compile_expr(c, object, or)?;
            let ir = c.alloc_reg()?;
            compile_expr(c, index, ir)?;
            c.emit(encode_abc(OpCode::GetIndex, dst, or, ir), 0);
            c.free_to(saved);
        }
        Expr::Array(items) => {
            let start = c.next_register;
            for item in items {
                let r = c.alloc_reg()?;
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
                let kr = c.alloc_reg()?;
                let ki = c.const_str(key);
                c.emit(encode_abx(OpCode::LoadConst, kr, ki), 0);
                let vr = c.alloc_reg()?;
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
                let r = c.alloc_reg()?;
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
            let sr = c.alloc_reg()?;
            compile_expr(c, inner, sr)?;
            c.emit(encode_abc(OpCode::Try, dst, sr, 0), 0);
            c.free_to(saved);
        }
        Expr::Lambda { params, body } => {
            let parent_locals = c.snapshot_locals();
            let parent_upvalues = c.snapshot_upvalues();

            let mut lc = Compiler::new("<lambda>");
            lc.parent_locals = parent_locals;
            lc.parent_upvalues = parent_upvalues;
            lc.current_line = c.current_line;
            lc.begin_scope();
            for p in params {
                lc.add_local(&p.name, true)?;
            }
            for s in body {
                lc.current_line = s.line;
                compile_stmt(&mut lc, &s.stmt)?;
            }
            lc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
            lc.chunk.max_registers = lc.max_register;
            lc.chunk.arity = params.len() as u8;
            lc.chunk.upvalue_count = lc.upvalues.len() as u8;

            let upvalue_sources: Vec<UpvalueSource> =
                lc.upvalues.iter().map(|u| u.source).collect();
            let mut proto_chunk = lc.chunk;
            proto_chunk.upvalue_sources = upvalue_sources;
            let pi = c.chunk.prototypes.len() as u16;
            c.chunk.prototypes.push(proto_chunk);
            c.emit(encode_abx(OpCode::Closure, dst, pi), 0);
        }
        Expr::StructInit { name, fields } => {
            let provided_fields = Expr::Object(fields.clone());
            compile_hidden_call(
                c,
                "__forge_new_struct",
                vec![Expr::StringLit(name.clone()), provided_fields],
                dst,
            )?;
        }
        Expr::Block(stmts) => {
            c.begin_scope();
            for s in stmts {
                c.current_line = s.line;
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();
        }
        Expr::Await(inner) => {
            let src = c.alloc_reg()?;
            compile_expr(c, inner, src)?;
            c.emit(encode_abc(OpCode::Await, dst, src, 0), c.current_line);
            c.free_to(src);
        }
        Expr::Must(inner) => {
            let src = c.alloc_reg()?;
            compile_expr(c, inner, src)?;
            c.emit(encode_abc(OpCode::Must, dst, src, 0), c.current_line);
            c.free_to(src);
        }
        Expr::Ask(inner) => {
            let src = c.alloc_reg()?;
            compile_expr(c, inner, src)?;
            c.emit(encode_abc(OpCode::Ask, dst, src, 0), c.current_line);
            c.free_to(src);
        }
        Expr::Freeze(inner) => {
            let src = c.alloc_reg()?;
            compile_expr(c, inner, src)?;
            c.emit(encode_abc(OpCode::Freeze, dst, src, 0), c.current_line);
            c.free_to(src);
        }
        Expr::Spawn(body) => {
            let parent_locals = c.snapshot_locals();
            let parent_upvalues = c.snapshot_upvalues();

            let mut sc = Compiler::new("<spawn>");
            sc.parent_locals = parent_locals;
            sc.parent_upvalues = parent_upvalues;
            sc.current_line = c.current_line;
            sc.begin_scope();
            for s in body {
                sc.current_line = s.line;
                compile_stmt(&mut sc, &s.stmt)?;
            }
            sc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
            sc.chunk.upvalue_count = sc.upvalues.len() as u8;
            sc.chunk.max_registers = sc.max_register;
            let upvalue_sources: Vec<UpvalueSource> =
                sc.upvalues.iter().map(|u| u.source).collect();
            let mut proto_chunk = sc.chunk;
            proto_chunk.upvalue_sources = upvalue_sources;
            let proto = c.chunk.prototypes.len() as u16;
            c.chunk.prototypes.push(proto_chunk);
            c.emit(encode_abx(OpCode::Closure, dst, proto), 0);
            c.emit(encode_abc(OpCode::Spawn, dst, 0, 0), 0);
        }
        Expr::Spread(inner) => {
            compile_expr(c, inner, dst)?;
        }
        Expr::WhereFilter {
            source,
            field,
            op,
            value,
        } => {
            compile_hidden_call(
                c,
                "__forge_where_filter",
                vec![
                    (**source).clone(),
                    Expr::StringLit(field.clone()),
                    Expr::StringLit(query_op_name(op).to_string()),
                    (**value).clone(),
                ],
                dst,
            )?;
        }
        Expr::PipeChain { source, steps } => {
            compile_expr(c, source, dst)?;

            for step in steps {
                match step {
                    PipeStep::Keep(predicate) => {
                        let saved = c.next_register;
                        let pred_reg = c.alloc_reg()?;
                        compile_expr(c, predicate, pred_reg)?;
                        compile_hidden_call_from_regs(c, "filter", &[dst, pred_reg], dst)?;
                        c.free_to(saved);
                    }
                    PipeStep::Sort(Some(field)) => {
                        let saved = c.next_register;
                        let field_reg = c.alloc_reg()?;
                        compile_expr(c, &Expr::StringLit(field.clone()), field_reg)?;
                        compile_hidden_call_from_regs(
                            c,
                            "__forge_pipe_sort",
                            &[dst, field_reg],
                            dst,
                        )?;
                        c.free_to(saved);
                    }
                    PipeStep::Sort(None) => {
                        compile_hidden_call_from_regs(c, "sort", &[dst], dst)?;
                    }
                    PipeStep::Take(count) => {
                        let saved = c.next_register;
                        let count_reg = c.alloc_reg()?;
                        compile_expr(c, count, count_reg)?;
                        compile_hidden_call_from_regs(
                            c,
                            "__forge_pipe_take",
                            &[dst, count_reg],
                            dst,
                        )?;
                        c.free_to(saved);
                    }
                    PipeStep::Apply(function) => {
                        compile_call_from_expr_and_regs(c, function, &[dst], dst)?;
                    }
                }
            }
        }
        Expr::MethodCall {
            object,
            method,
            args,
        } => {
            let mut lowered_args = Vec::with_capacity(args.len() + 2);
            lowered_args.push((**object).clone());
            lowered_args.push(Expr::StringLit(method.clone()));
            lowered_args.extend(args.clone());
            compile_hidden_call(c, "__forge_call_method", lowered_args, dst)?;
        }
    }
    Ok(())
}
