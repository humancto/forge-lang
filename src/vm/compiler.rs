use super::bytecode::*;
use crate::parser::ast::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

struct Local {
    name: String,
    depth: usize,
    register: u8,
    mutable: bool,
    /// Set once a nested closure captures this local as an upvalue. Scopes
    /// that release a captured local emit `CloseUpvalues` so the next binding
    /// in the same register (e.g. the next loop iteration) gets a fresh cell.
    captured: bool,
}

struct LoopContext {
    start: usize,
    /// Scope depth outside the loop body; locals deeper than this belong to
    /// the loop body and are released on every iteration.
    scope_depth: usize,
    break_jumps: Vec<usize>,
    /// Forward jumps from `continue` that must land on the loop's step code
    /// (`for` loops). `None` means `continue` jumps back to `start`.
    continue_jumps: Option<Vec<usize>>,
    /// No-op placeholders emitted by `break`/`continue`; patched into
    /// `CloseUpvalues` when the loop body captured any locals.
    close_placeholders: Vec<usize>,
    /// Inclusive register range of loop-body locals that were captured.
    captured_range: Option<(u8, u8)>,
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
    mutable: bool,
}

/// A variable visible from an enclosing function: (name, register or
/// upvalue index, mutable).
type ParentBinding = (String, u8, bool);

pub struct Compiler {
    chunk: Chunk,
    locals: Vec<Local>,
    scope_depth: usize,
    next_register: u8,
    max_register: u8,
    loops: Vec<LoopContext>,
    cleanup_contexts: Vec<CleanupContext>,
    upvalues: Vec<UpvalueEntry>,
    parent_locals: Vec<ParentBinding>,
    parent_upvalues: Vec<ParentBinding>,
    /// Names bound in functions enclosing the parent (grandparent and up).
    /// Referencing one of these makes the parent capture it on our behalf.
    outer_names: HashSet<String>,
    /// Upvalues whose source must be resolved by the parent once this
    /// function is finished (the parent itself has to capture the binding
    /// from further out first): (our upvalue index, name).
    pending_captures: Vec<(u8, String)>,
    module_mode: bool,
    /// Directory of the file being compiled; `import` paths resolve
    /// relative to it first.
    base_dir: Option<PathBuf>,
    /// Imported modules keep their top-level names under a private prefix so
    /// that names the importer did not ask for never leak into its globals.
    global_prefix: Option<String>,
    module_globals: HashSet<String>,
    /// Source line currently being compiled. Top-level loops update this
    /// from `SpannedStmt` before each statement so any `emit` that passes
    /// `0` for the line picks up a real source span instead.
    current_line: usize,
    current_col: usize,
    /// Set when a branch distance did not fit the 16-bit sBx operand. The
    /// branch is left as a placeholder and the function fails to compile in
    /// `check_branches` (instead of silently jumping to the wrong place).
    branch_overflow: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileErrorKind {
    /// The program cannot be compiled (e.g. a function needs too many registers).
    Error,
    /// The program is valid Forge but uses a construct the bytecode VM
    /// cannot execute faithfully. Callers fall back to the interpreter.
    Unsupported,
}

#[derive(Debug)]
pub struct CompileError {
    pub message: String,
    pub kind: CompileErrorKind,
}

impl CompileError {
    fn new(msg: &str) -> Self {
        Self {
            message: msg.to_string(),
            kind: CompileErrorKind::Error,
        }
    }

    /// A construct the VM does not implement. `construct` names it for the
    /// fallback notice ("VM does not support <construct>").
    pub fn unsupported(construct: &str) -> Self {
        Self {
            message: format!("VM does not support {}", construct),
            kind: CompileErrorKind::Unsupported,
        }
    }

    pub fn is_unsupported(&self) -> bool {
        self.kind == CompileErrorKind::Unsupported
    }
}

/// Options for [`compile_with`].
#[derive(Default, Clone)]
pub struct CompileOptions {
    /// Directory of the source file; relative imports resolve against it.
    pub base_dir: Option<PathBuf>,
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
            outer_names: HashSet::new(),
            pending_captures: Vec::new(),
            module_mode: false,
            base_dir: None,
            global_prefix: None,
            module_globals: HashSet::new(),
            current_line: 0,
            current_col: 0,
            branch_overflow: false,
        }
    }

    /// Start compiling a nested function (fn, lambda, spawn/schedule/watch
    /// body) that may capture this compiler's bindings.
    fn child(&self, name: &str) -> Compiler {
        let mut fc = Compiler::new(name);
        fc.parent_locals = self
            .locals
            .iter()
            .map(|l| (l.name.clone(), l.register, l.mutable))
            .collect();
        fc.parent_upvalues = self
            .upvalues
            .iter()
            .enumerate()
            .map(|(index, uv)| (uv.name.clone(), index as u8, uv.mutable))
            .collect();
        let mut outer = self.outer_names.clone();
        outer.extend(self.parent_locals.iter().map(|(n, _, _)| n.clone()));
        outer.extend(self.parent_upvalues.iter().map(|(n, _, _)| n.clone()));
        fc.outer_names = outer;
        fc.current_line = self.current_line;
        fc.current_col = self.current_col;
        fc.base_dir = self.base_dir.clone();
        fc.global_prefix = self.global_prefix.clone();
        fc.module_globals = self.module_globals.clone();
        fc
    }

    /// Finish a nested function started with [`Compiler::child`]: resolve
    /// captures that had to go through this compiler, mark captured locals
    /// and register the prototype. Returns the prototype index.
    fn finish_child(&mut self, mut fc: Compiler) -> Result<u16, CompileError> {
        fc.check_branches()?;
        for (uv_idx, name) in std::mem::take(&mut fc.pending_captures) {
            let source = self.capture_for_child(&name).ok_or_else(|| {
                CompileError::new(&format!("internal: cannot resolve captured '{}'", name))
            })?;
            fc.upvalues[uv_idx as usize].source = source;
        }
        for uv in &fc.upvalues {
            if let UpvalueSource::Local(reg) = uv.source {
                self.mark_captured(reg);
            }
        }
        fc.chunk.max_registers = fc.max_register;
        fc.chunk.upvalue_count = fc.upvalues.len() as u8;
        fc.chunk.upvalue_sources = fc.upvalues.iter().map(|u| u.source).collect();
        let idx = self.chunk.prototypes.len();
        if idx > u16::MAX as usize {
            return Err(CompileError::new("too many nested functions"));
        }
        self.chunk.prototypes.push(fc.chunk);
        Ok(idx as u16)
    }

    /// How a child function should reach `name`, capturing it in this
    /// compiler first when it lives further out.
    fn capture_for_child(&mut self, name: &str) -> Option<UpvalueSource> {
        if let Some((reg, _)) = self.resolve_local(name) {
            return Some(UpvalueSource::Local(reg));
        }
        self.resolve_capture(name).map(UpvalueSource::Upvalue)
    }

    fn mark_captured(&mut self, reg: u8) {
        if let Some(local) = self.locals.iter_mut().rev().find(|l| l.register == reg) {
            local.captured = true;
        }
    }

    /// Resolve `name` as an upvalue of this function, adding the upvalue on
    /// first use. Returns `None` when the name is not bound in any enclosing
    /// function (i.e. it is a global).
    fn resolve_capture(&mut self, name: &str) -> Option<u8> {
        if let Some(idx) = self.resolve_upvalue(name) {
            return Some(idx);
        }
        if let Some((reg, mutable)) = self.resolve_in_parent(name) {
            return Some(self.add_upvalue(name, UpvalueSource::Local(reg), mutable));
        }
        if let Some((parent_idx, mutable)) = self.resolve_parent_upvalue(name) {
            return Some(self.add_upvalue(name, UpvalueSource::Upvalue(parent_idx), mutable));
        }
        if self.outer_names.contains(name) {
            // Placeholder source; patched by the parent in `finish_child`.
            let idx = self.add_upvalue(name, UpvalueSource::Upvalue(u8::MAX), true);
            self.pending_captures.push((idx, name.to_string()));
            return Some(idx);
        }
        None
    }

    /// Whether `name` currently refers to a mutable binding (local or
    /// captured). `None` for globals.
    fn binding_mutability(&self, name: &str) -> Option<bool> {
        if let Some((_, mutable)) = self.resolve_local(name) {
            return Some(mutable);
        }
        if let Some(uv) = self.upvalues.iter().find(|u| u.name == name) {
            return Some(uv.mutable);
        }
        if let Some((_, mutable)) = self.resolve_in_parent(name) {
            return Some(mutable);
        }
        if let Some((_, mutable)) = self.resolve_parent_upvalue(name) {
            return Some(mutable);
        }
        if self.outer_names.contains(name) {
            return Some(true);
        }
        None
    }

    /// The runtime global name for a user-visible top-level name.
    fn global_name(&self, name: &str) -> String {
        match &self.global_prefix {
            Some(prefix) if self.module_globals.contains(name) => format!("{}{}", prefix, name),
            _ => name.to_string(),
        }
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
        let actual_line = if line == 0 { self.current_line } else { line };
        let actual_col = self.current_col;
        self.chunk.emit_at(inst, actual_line, actual_col);
    }

    fn set_current_span(&mut self, line: usize, col: usize) {
        self.current_line = line;
        self.current_col = col;
    }

    fn set_span(&mut self, spanned: &SpannedStmt) {
        self.set_current_span(spanned.line, spanned.col);
    }

    fn emit_jump(&mut self, op: OpCode, a: u8, line: usize) -> usize {
        let idx = self.chunk.code_len();
        self.emit(encode_asbx(op, a, 0), line);
        idx
    }

    fn patch_jump(&mut self, offset: usize) {
        let target = self.chunk.code_len();
        if !self.chunk.patch_jump(offset, target) {
            self.branch_overflow = true;
        }
    }

    fn emit_loop(&mut self, loop_start: usize, line: usize) {
        let current = self.chunk.code_len();
        let offset = branch_offset(current, loop_start).unwrap_or_else(|| {
            self.branch_overflow = true;
            0
        });
        self.emit(encode_asbx(OpCode::Loop, 0, offset), line);
    }

    /// Fail when a branch in this function did not fit the 16-bit offset
    /// (see `branch_overflow`). Called once a function's code is complete.
    fn check_branches(&self) -> Result<(), CompileError> {
        if self.branch_overflow {
            return Err(CompileError::new(&format!(
                "function '{}' is too large: a jump spans more than {} instructions. Try splitting it into smaller functions.",
                self.chunk.name,
                i16::MAX
            )));
        }
        Ok(())
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

    /// Leave the innermost scope. If any local released here was captured by
    /// a closure, emits `CloseUpvalues` for those registers and returns the
    /// closed range so exceptional paths (catch landing pads) can repeat it.
    fn end_scope(&mut self) -> Option<(u8, u8)> {
        self.scope_depth -= 1;
        let mut range: Option<(u8, u8)> = None;
        let mut lowest_released: Option<u8> = None;
        while let Some(local) = self.locals.last() {
            if local.depth <= self.scope_depth {
                break;
            }
            lowest_released =
                Some(lowest_released.map_or(local.register, |low| low.min(local.register)));
            if local.captured {
                let reg = local.register;
                range = Some(match range {
                    Some((lo, hi)) => (lo.min(reg), hi.max(reg)),
                    None => (reg, reg),
                });
                let depth = local.depth;
                if let Some(ctx) = self.loops.last_mut() {
                    if depth > ctx.scope_depth {
                        ctx.captured_range = Some(match ctx.captured_range {
                            Some((lo, hi)) => (lo.min(reg), hi.max(reg)),
                            None => (reg, reg),
                        });
                    }
                }
            }
            self.locals.pop();
        }
        if let Some(range) = range {
            self.emit_close(range);
        }
        // Registers of the released locals (and any temporaries above them)
        // are free again; without this every block-scoped `let` would hold
        // its register until the function ends.
        if let Some(low) = lowest_released {
            if low < self.next_register {
                self.next_register = low;
            }
        }
        range
    }

    fn emit_close(&mut self, (lo, hi): (u8, u8)) {
        self.emit(encode_abc(OpCode::CloseUpvalues, lo, hi, 0), 0);
    }

    fn push_loop(&mut self, start: usize, continue_to_start: bool) {
        self.loops.push(LoopContext {
            start,
            scope_depth: self.scope_depth,
            break_jumps: Vec::new(),
            continue_jumps: if continue_to_start {
                None
            } else {
                Some(Vec::new())
            },
            close_placeholders: Vec::new(),
            captured_range: None,
        });
    }

    /// Pop the innermost loop, patching `break` jumps to the current
    /// position and `break`/`continue` placeholders into `CloseUpvalues`.
    fn pop_loop(&mut self) -> Result<(), CompileError> {
        let ctx = self
            .loops
            .pop()
            .ok_or_else(|| CompileError::new("internal: loop stack underflow"))?;
        if let Some((lo, hi)) = ctx.captured_range {
            for pc in ctx.close_placeholders {
                self.chunk.code[pc] = encode_abc(OpCode::CloseUpvalues, lo, hi, 0);
            }
        }
        for bj in ctx.break_jumps {
            self.patch_jump(bj);
        }
        Ok(())
    }

    /// Emit a no-op that `pop_loop` may later turn into `CloseUpvalues`.
    fn emit_close_placeholder(&mut self) {
        let pc = self.chunk.code_len();
        self.emit(encode_asbx(OpCode::Jump, 0, 0), 0);
        if let Some(ctx) = self.loops.last_mut() {
            ctx.close_placeholders.push(pc);
        }
    }

    fn add_local(&mut self, name: &str, mutable: bool) -> Result<u8, CompileError> {
        let reg = self.alloc_reg()?;
        self.locals.push(Local {
            name: name.to_string(),
            depth: self.scope_depth,
            register: reg,
            mutable,
            captured: false,
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

    fn add_upvalue(&mut self, name: &str, source: UpvalueSource, mutable: bool) -> u8 {
        if let Some(idx) = self.resolve_upvalue(name) {
            return idx;
        }
        let idx = self.upvalues.len() as u8;
        self.upvalues.push(UpvalueEntry {
            name: name.to_string(),
            source,
            mutable,
        });
        idx
    }

    fn resolve_in_parent(&self, name: &str) -> Option<(u8, bool)> {
        self.parent_locals
            .iter()
            .rev()
            .find(|(pname, _, _)| pname == name)
            .map(|(_, reg, mutable)| (*reg, *mutable))
    }

    fn resolve_parent_upvalue(&self, name: &str) -> Option<(u8, bool)> {
        self.parent_upvalues
            .iter()
            .find(|(pname, _, _)| pname == name)
            .map(|(_, idx, mutable)| (*idx, *mutable))
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

/// Names a module defines at top level (and therefore exports).
fn module_top_level_names(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    for spanned in &program.statements {
        match &spanned.stmt {
            Stmt::FnDef { name, .. } | Stmt::Let { name, .. } => {
                names.insert(name.clone());
            }
            Stmt::TypeDef { name, variants } => {
                names.insert(format!("__type_{}__", name));
                for variant in variants {
                    names.insert(variant.name.clone());
                }
            }
            _ => {}
        }
    }
    names
}

fn compile_top_level(c: &mut Compiler, program: &Program) -> Result<(), CompileError> {
    c.begin_scope();
    for spanned in &program.statements {
        c.set_span(spanned);
        compile_stmt(c, &spanned.stmt)?;
    }
    c.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
    c.chunk.max_registers = c.max_register;
    Ok(())
}

pub fn compile(program: &Program) -> Result<Chunk, CompileError> {
    compile_with(program, &CompileOptions::default())
}

pub fn compile_with(program: &Program, options: &CompileOptions) -> Result<Chunk, CompileError> {
    let mut c = Compiler::new("<main>");
    c.base_dir = options.base_dir.clone();
    compile_top_level(&mut c, program)?;
    c.check_branches()?;
    super::verify::debug_verify_compiled(&c.chunk);
    Ok(c.chunk)
}

/// Compile an imported module. Its top-level names are stored as globals
/// under `global_prefix` (see [`module_global_prefix`]) so the importer only
/// sees the names it imports.
pub fn compile_module_with(
    program: &Program,
    base_dir: Option<&Path>,
    global_prefix: Option<&str>,
) -> Result<Chunk, CompileError> {
    let mut c = Compiler::new("<module>");
    c.module_mode = true;
    c.base_dir = base_dir.map(Path::to_path_buf);
    if let Some(prefix) = global_prefix {
        c.global_prefix = Some(prefix.to_string());
        c.module_globals = module_top_level_names(program);
    }
    compile_top_level(&mut c, program)?;
    c.check_branches()?;
    super::verify::debug_verify_compiled(&c.chunk);
    Ok(c.chunk)
}

/// Private global namespace for an imported module's top-level names.
pub fn module_global_prefix(resolved_path: &str) -> String {
    format!("__module[{}]::", resolved_path)
}

pub fn compile_repl(program: &Program) -> Result<Chunk, CompileError> {
    let mut c = Compiler::new("<repl>");
    c.begin_scope();

    let result_reg = c.alloc_reg()?;
    let mut has_result = false;

    for spanned in &program.statements {
        c.set_span(spanned);
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
    c.check_branches()?;
    super::verify::debug_verify_compiled(&c.chunk);
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

/// `dst = obj.field`. `GetField` only has an 8-bit constant operand, so
/// functions with more than 256 constants fall back to a builtin call.
fn emit_get_field(c: &mut Compiler, dst: u8, obj: u8, field: &str) -> Result<(), CompileError> {
    let idx = c.const_str(field);
    if let Ok(idx) = u8::try_from(idx) {
        c.emit(encode_abc(OpCode::GetField, dst, obj, idx), 0);
        return Ok(());
    }
    let saved = c.next_register;
    let name_reg = c.alloc_reg()?;
    c.emit(encode_abx(OpCode::LoadConst, name_reg, idx), 0);
    compile_hidden_call_from_regs(c, "__forge_get_field", &[obj, name_reg], dst)?;
    c.free_to(saved);
    Ok(())
}

/// `obj.field = value`, with the same wide-constant fallback.
fn emit_set_field(c: &mut Compiler, obj: u8, field: &str, value: u8) -> Result<(), CompileError> {
    let idx = c.const_str(field);
    if let Ok(idx) = u8::try_from(idx) {
        c.emit(encode_abc(OpCode::SetField, obj, idx, value), 0);
        return Ok(());
    }
    let saved = c.next_register;
    let name_reg = c.alloc_reg()?;
    c.emit(encode_abx(OpCode::LoadConst, name_reg, idx), 0);
    // The native returns the updated copy; it replaces `obj` like SetField.
    compile_hidden_call_from_regs(c, "__forge_set_field", &[obj, name_reg, value], obj)?;
    c.free_to(saved);
    Ok(())
}

/// Store `src` into a variable, enforcing `let` immutability.
fn compile_store_variable(c: &mut Compiler, name: &str, src: u8) -> Result<(), CompileError> {
    if let Some((reg, mutable)) = c.resolve_local(name) {
        if !mutable {
            return compile_hidden_stmt(
                c,
                "__forge_raise_error",
                vec![Expr::StringLit(crate::semantics::immutable_reassign(name))],
            );
        }
        c.emit(encode_abc(OpCode::SetLocal, reg, src, 0), 0);
    } else if c.binding_mutability(name) == Some(false) {
        return compile_hidden_stmt(
            c,
            "__forge_raise_error",
            vec![Expr::StringLit(crate::semantics::immutable_reassign(name))],
        );
    } else if let Some(uv_idx) = c.resolve_capture(name) {
        c.emit(encode_abc(OpCode::SetUpvalue, uv_idx, src, 0), 0);
    } else {
        let name_idx = c.const_str(&c.global_name(name));
        c.emit(encode_abx(OpCode::SetGlobal, src, name_idx), 0);
    }
    Ok(())
}

/// Store `src` into an assignment target.
///
/// Collections have value semantics (as in the interpreter): `SetIndex` and
/// `SetField` never mutate the container in place. They replace the
/// container register with an updated copy, which is then stored back into
/// the place it was read from — recursively, so `grid[0][1] = v` rebuilds
/// `grid[0]` and then `grid`. Other bindings that refer to the old
/// container (`let w = z`, function arguments, captured values) are
/// unaffected.
fn compile_store(c: &mut Compiler, target: &Expr, src: u8) -> Result<(), CompileError> {
    match target {
        Expr::Ident(name) => compile_store_variable(c, name, src),
        Expr::FieldAccess { object, field } => {
            let saved = c.next_register;
            let obj_reg = c.alloc_reg()?;
            compile_expr(c, object, obj_reg)?;
            emit_set_field(c, obj_reg, field, src)?;
            store_back(c, object, obj_reg)?;
            c.free_to(saved);
            Ok(())
        }
        Expr::Index { object, index } => {
            let saved = c.next_register;
            let obj_reg = c.alloc_reg()?;
            compile_expr(c, object, obj_reg)?;
            let idx_reg = c.alloc_reg()?;
            compile_expr(c, index, idx_reg)?;
            c.emit(encode_abc(OpCode::SetIndex, obj_reg, idx_reg, src), 0);
            store_back(c, object, obj_reg)?;
            c.free_to(saved);
            Ok(())
        }
        _ => Err(CompileError::new("invalid assignment target")),
    }
}

/// After updating a container read from `place`, store the updated copy
/// back. Temporaries (`f()[0] = 1`) have nowhere to go.
fn store_back(c: &mut Compiler, place: &Expr, src: u8) -> Result<(), CompileError> {
    match place {
        Expr::Ident(_) | Expr::FieldAccess { .. } | Expr::Index { .. } => {
            compile_store(c, place, src)
        }
        _ => Ok(()),
    }
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
    let name_idx = c.const_str(&c.global_name(name));
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
        TypeAnn::Tuple(items) => {
            let inner = items
                .iter()
                .map(type_ann_name)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({})", inner)
        }
    }
}

fn resolve_import_path(c: &Compiler, path: &str) -> Result<PathBuf, CompileError> {
    let resolved = crate::package::resolve_import_from(path, c.base_dir.as_deref())
        .ok_or_else(|| CompileError::new(&crate::semantics::import_not_found(path)))?;
    Ok(std::fs::canonicalize(&resolved).unwrap_or(resolved))
}

fn parse_import_program(path: &str, resolved: &Path) -> Result<Program, CompileError> {
    let source = std::fs::read_to_string(resolved)
        .map_err(|e| CompileError::new(&format!("cannot import '{}': {}", path, e)))?;
    let mut lexer = crate::lexer::Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| CompileError::new(&format!("import '{}' lex error: {}", path, e.message)))?;
    let mut parser = crate::parser::Parser::new(tokens);
    parser
        .parse_program()
        .map_err(|e| CompileError::new(&format!("import '{}' parse error: {}", path, e.message)))
}

/// Names a bare `import` (no name list) binds in the importer.
pub(crate) fn import_export_names(program: &Program) -> Vec<String> {
    program
        .statements
        .iter()
        .filter_map(|spanned| match &spanned.stmt {
            Stmt::FnDef { name, .. } | Stmt::Let { name, .. } => Some(vec![name.clone()]),
            // Like the interpreter, `import "x"` brings in a module's ADT
            // constructors and type metadata too.
            Stmt::TypeDef { name, variants } => Some(
                variants
                    .iter()
                    .map(|variant| variant.name.clone())
                    .chain(std::iter::once(format!("__type_{}__", name)))
                    .collect(),
            ),
            _ => None,
        })
        .flatten()
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

/// Methods that, called on a mutable variable, rebind that variable to the
/// updated collection (`xs.push(v)`, `xs.pop()`, `s.add(v)`, `s.remove(v)`).
fn is_mutating_method(method: &str, arg_count: usize) -> bool {
    matches!(
        (method, arg_count),
        ("push", 1) | ("pop", 0) | ("add", 1) | ("remove", 1)
    )
}

/// Compile `receiver.method(args)` (or `method(receiver, args)`) where the
/// receiver is a mutable variable and the method mutates in place. Returns
/// `false` when the call is not of that shape and was not compiled.
///
/// `__forge_method_mut` returns `(new_receiver, result)`; the variable is
/// rebound to `new_receiver` (unchanged when the value's type has no
/// in-place form, e.g. `add` on an array).
fn try_compile_mutating_call(
    c: &mut Compiler,
    receiver: &Expr,
    method: &str,
    args: &[Expr],
    dst: u8,
) -> Result<bool, CompileError> {
    let Expr::Ident(var) = receiver else {
        return Ok(false);
    };
    if !is_mutating_method(method, args.len()) || c.binding_mutability(var) != Some(true) {
        return Ok(false);
    }
    if method == "pop" && args.is_empty() {
        if let Some((reg, true)) = c.resolve_local(var) {
            // In place when the local owns its array (see `vm::local_ops`).
            c.emit(encode_abc(OpCode::PopLocal, reg, dst, 0), 0);
            return Ok(true);
        }
    }
    let saved = c.next_register;
    let pair_reg = c.alloc_reg()?;
    let mut lowered = vec![receiver.clone(), Expr::StringLit(method.to_string())];
    lowered.extend(args.iter().cloned());
    compile_hidden_call(c, "__forge_method_mut", lowered, pair_reg)?;
    let idx_reg = c.alloc_reg()?;
    let new_value_reg = c.alloc_reg()?;
    let zero = c.const_int(0);
    c.emit(encode_abx(OpCode::LoadConst, idx_reg, zero), 0);
    c.emit(
        encode_abc(OpCode::GetIndex, new_value_reg, pair_reg, idx_reg),
        0,
    );
    emit_store_var(c, var, new_value_reg)?;
    let one = c.const_int(1);
    c.emit(encode_abx(OpCode::LoadConst, idx_reg, one), 0);
    c.emit(encode_abc(OpCode::GetIndex, dst, pair_reg, idx_reg), 0);
    c.free_to(saved);
    Ok(true)
}

/// An operand whose evaluation has no side effects and runs no user code
/// (literals, variable reads, arithmetic on those). Evaluating it before
/// rather than after reading a local therefore cannot change the result,
/// which lets `AddLocal` / `PushLocal` read the local last.
fn is_simple_operand(expr: &Expr) -> bool {
    match expr {
        Expr::Int(_) | Expr::Float(_) | Expr::StringLit(_) | Expr::Bool(_) | Expr::Ident(_) => true,
        Expr::BinOp { left, right, .. } => is_simple_operand(left) && is_simple_operand(right),
        Expr::UnaryOp { operand, .. } => is_simple_operand(operand),
        _ => false,
    }
}

/// `x = x + e` (and `x += e`) on a mutable local with a simple `e`:
/// one `AddLocal`, which appends in place to a string the local owns.
fn try_compile_add_local(
    c: &mut Compiler,
    target: &Expr,
    value: &Expr,
) -> Result<bool, CompileError> {
    let (
        Expr::Ident(name),
        Expr::BinOp {
            left,
            op: BinOp::Add,
            right,
        },
    ) = (target, value)
    else {
        return Ok(false);
    };
    if !matches!(left.as_ref(), Expr::Ident(l) if l == name) || !is_simple_operand(right) {
        return Ok(false);
    }
    let Some((reg, true)) = c.resolve_local(name) else {
        return Ok(false);
    };
    let saved = c.next_register;
    let rhs = c.alloc_reg()?;
    compile_expr(c, right, rhs)?;
    c.emit(encode_abc(OpCode::AddLocal, reg, rhs, 0), 0);
    c.free_to(saved);
    Ok(true)
}

/// Statement `xs.push(e)` / `push(xs, e)` on a mutable local with a simple
/// `e`: one `PushLocal`, in place when the local owns its array. Only in
/// statement position, because `push` returns the receiver itself and a
/// used result would be a second reference to an owned array.
fn try_compile_push_statement(c: &mut Compiler, expr: &Expr) -> Result<bool, CompileError> {
    let (receiver, value) = match expr {
        Expr::MethodCall {
            object,
            method,
            args,
        } if method == "push" && args.len() == 1 => (object.as_ref(), &args[0]),
        Expr::Call { function, args } => match function.as_ref() {
            Expr::FieldAccess { object, field } if field == "push" && args.len() == 1 => {
                (object.as_ref(), &args[0])
            }
            Expr::Ident(fn_name)
                if fn_name == "push"
                    && args.len() == 2
                    && c.resolve_local(fn_name).is_none()
                    && c.binding_mutability(fn_name).is_none() =>
            {
                (&args[0], &args[1])
            }
            _ => return Ok(false),
        },
        _ => return Ok(false),
    };
    let Expr::Ident(var) = receiver else {
        return Ok(false);
    };
    if !is_simple_operand(value) {
        return Ok(false);
    }
    let Some((reg, true)) = c.resolve_local(var) else {
        return Ok(false);
    };
    let saved = c.next_register;
    let value_reg = c.alloc_reg()?;
    compile_expr(c, value, value_reg)?;
    c.emit(encode_abc(OpCode::PushLocal, reg, value_reg, 0), 0);
    c.free_to(saved);
    Ok(true)
}

/// Store `src` into the variable `name` (local, captured or global).
fn emit_store_var(c: &mut Compiler, name: &str, src: u8) -> Result<(), CompileError> {
    if let Some((reg, _)) = c.resolve_local(name) {
        c.emit(encode_abc(OpCode::SetLocal, reg, src, 0), 0);
    } else if let Some(uv_idx) = c.resolve_capture(name) {
        c.emit(encode_abc(OpCode::SetUpvalue, uv_idx, src, 0), 0);
    } else {
        let name_idx = c.const_str(&c.global_name(name));
        c.emit(encode_abx(OpCode::SetGlobal, src, name_idx), 0);
    }
    Ok(())
}

/// Value of a block expression (`if`/`when`/`safe` expressions and `{ ... }`
/// blocks): the value of its final statement — an expression's value, the
/// taken branch of an `if`, the matched arm of a `when`, the result of a
/// `safe` block — or null for any other statement.
fn compile_block_value(
    c: &mut Compiler,
    stmts: &[SpannedStmt],
    dst: u8,
) -> Result<(), CompileError> {
    c.begin_scope();
    match stmts.split_last() {
        None => c.emit(encode_abc(OpCode::LoadNull, dst, 0, 0), 0),
        Some((last, init)) => {
            for s in init {
                c.set_span(s);
                compile_stmt(c, &s.stmt)?;
            }
            c.set_span(last);
            compile_stmt_value(c, &last.stmt, dst)?;
        }
    }
    c.end_scope();
    Ok(())
}

fn compile_stmt_value(c: &mut Compiler, stmt: &Stmt, dst: u8) -> Result<(), CompileError> {
    match stmt {
        Stmt::Expression(expr) => compile_expr(c, expr, dst),
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
            compile_block_value(c, then_body, dst)?;
            let end_jump = c.emit_jump(OpCode::Jump, 0, 0);
            c.patch_jump(else_jump);
            match else_body {
                Some(else_body) => compile_block_value(c, else_body, dst)?,
                None => c.emit(encode_abc(OpCode::LoadNull, dst, 0, 0), 0),
            }
            c.patch_jump(end_jump);
            Ok(())
        }
        Stmt::When { subject, arms } => compile_when(c, subject, arms, dst),
        Stmt::Match { subject, arms } => compile_match(c, subject, arms, Some(dst)),
        Stmt::SafeBlock { body } => compile_safe_block(c, body, Some(dst)),
        other => {
            compile_stmt(c, other)?;
            c.emit(encode_abc(OpCode::LoadNull, dst, 0, 0), 0);
            Ok(())
        }
    }
}

/// `match subject { pattern => body, ... }`. With `dst`, the taken arm's
/// body is compiled as a block value into `dst` (null when no arm matches),
/// so a `match` that ends a function body is its return value.
fn compile_match(
    c: &mut Compiler,
    subject: &Expr,
    arms: &[MatchArm],
    dst: Option<u8>,
) -> Result<(), CompileError> {
    let saved = c.next_register;
    let subj = c.alloc_reg()?;
    compile_expr(c, subject, subj)?;
    if let Some(dst) = dst {
        c.emit(encode_abc(OpCode::LoadNull, dst, 0, 0), 0);
    }
    let mut end_jumps = Vec::new();
    let mut has_catch_all = false;

    for arm in arms {
        match &arm.pattern {
            Pattern::Wildcard => {
                c.begin_scope();
                compile_arm_body(c, &arm.body, dst)?;
                c.end_scope();
                has_catch_all = true;
                break;
            }
            Pattern::Binding(name) => {
                let saved = c.next_register;
                let fn_reg = c.alloc_reg()?;
                let fn_idx = c.const_str("__forge_binding_matches");
                c.emit(encode_abx(OpCode::GetGlobal, fn_reg, fn_idx), 0);

                let name_reg = c.alloc_reg()?;
                let name_idx = c.const_str(&c.global_name(name));
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
                compile_arm_body(c, &arm.body, dst)?;
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
                compile_arm_body(c, &arm.body, dst)?;
                c.end_scope();

                let ej = c.emit_jump(OpCode::Jump, 0, 0);
                end_jumps.push(ej);
                c.patch_jump(skip);
            }
            Pattern::Constructor { name, fields } => {
                let variant_idx = c.const_str(name);
                let vr = c.alloc_reg()?;
                emit_get_field(c, vr, subj, "__variant__")?;
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
                compile_arm_body(c, &arm.body, dst)?;
                c.end_scope();

                let ej = c.emit_jump(OpCode::Jump, 0, 0);
                end_jumps.push(ej);
                c.patch_jump(skip);
            }
        }
    }
    if !has_catch_all {
        // No arm matched: a runtime error, as in the interpreter.
        compile_hidden_stmt(
            c,
            "__forge_raise_error",
            vec![Expr::StringLit("non-exhaustive match".to_string())],
        )?;
    }
    for ej in end_jumps {
        c.patch_jump(ej);
    }
    c.free_to(saved);
    Ok(())
}

fn compile_arm_body(
    c: &mut Compiler,
    body: &[SpannedStmt],
    dst: Option<u8>,
) -> Result<(), CompileError> {
    match dst {
        Some(dst) => compile_block_value(c, body, dst),
        None => {
            for s in body {
                c.set_span(s);
                compile_stmt(c, &s.stmt)?;
            }
            Ok(())
        }
    }
}

/// `when subject { op value -> result, ..., else -> result }` into `dst`
/// (null when no arm matches). Arm tests use the shared
/// `semantics::when_matches` rule via `__forge_when_matches`.
fn compile_when(
    c: &mut Compiler,
    subject: &Expr,
    arms: &[WhenArm],
    dst: u8,
) -> Result<(), CompileError> {
    let saved = c.next_register;
    let subj_reg = c.alloc_reg()?;
    compile_expr(c, subject, subj_reg)?;
    c.emit(encode_abc(OpCode::LoadNull, dst, 0, 0), 0);
    let mut end_jumps = Vec::new();
    for arm in arms {
        if arm.is_else {
            compile_expr(c, &arm.result, dst)?;
            break;
        }
        let (Some(op), Some(cmp_val)) = (&arm.op, &arm.value) else {
            continue;
        };
        let arm_saved = c.next_register;
        let op_reg = c.alloc_reg()?;
        let op_idx = c.const_str(query_op_name(op));
        c.emit(encode_abx(OpCode::LoadConst, op_reg, op_idx), 0);
        let cmp_reg = c.alloc_reg()?;
        compile_expr(c, cmp_val, cmp_reg)?;
        let cond_reg = c.alloc_reg()?;
        compile_hidden_call_from_regs(
            c,
            "__forge_when_matches",
            &[op_reg, subj_reg, cmp_reg],
            cond_reg,
        )?;
        let skip = c.emit_jump(OpCode::JumpIfFalse, cond_reg, 0);
        c.free_to(arm_saved);
        compile_expr(c, &arm.result, dst)?;
        end_jumps.push(c.emit_jump(OpCode::Jump, 0, 0));
        c.patch_jump(skip);
    }
    for j in end_jumps {
        c.patch_jump(j);
    }
    c.free_to(saved);
    Ok(())
}

/// `safe { ... }`: errors inside the body are swallowed. As an expression
/// (`dst` given) it yields the body's final expression value, or null when
/// the body failed or did not end in an expression.
fn compile_safe_block(
    c: &mut Compiler,
    body: &[SpannedStmt],
    dst: Option<u8>,
) -> Result<(), CompileError> {
    let saved = c.next_register;
    let err_reg = c.alloc_reg()?;
    let handler_jump = c.emit_jump(OpCode::PushHandler, err_reg, 0);
    c.cleanup_contexts.push(CleanupContext {
        kind: CleanupKind::Handler,
        loop_depth: c.loops.len(),
    });
    c.begin_scope();
    let (init, last) = match (dst, body.split_last()) {
        (Some(_), Some((last, init))) => (init, Some(last)),
        _ => (body, None),
    };
    for s in init {
        c.set_span(s);
        compile_stmt(c, &s.stmt)?;
    }
    if let (Some(dst), Some(last)) = (dst, last) {
        c.set_span(last);
        if let Stmt::Expression(expr) = &last.stmt {
            compile_expr(c, expr, dst)?;
        } else {
            compile_stmt(c, &last.stmt)?;
            c.emit(encode_abc(OpCode::LoadNull, dst, 0, 0), 0);
        }
    } else if let Some(dst) = dst {
        c.emit(encode_abc(OpCode::LoadNull, dst, 0, 0), 0);
    }
    let captured = c.end_scope();
    c.cleanup_contexts.pop();
    c.emit(encode_abc(OpCode::PopHandler, 0, 0, 0), 0);
    let end_jump = c.emit_jump(OpCode::Jump, 0, 0);
    c.patch_jump(handler_jump);
    if let Some(range) = captured {
        c.emit_close(range);
    }
    if let Some(dst) = dst {
        c.emit(encode_abc(OpCode::LoadNull, dst, 0, 0), 0);
    }
    c.patch_jump(end_jump);
    c.free_to(saved);
    Ok(())
}

/// Compile a function or lambda body. Like the interpreter, a body whose
/// final statement is an expression returns that expression's value.
fn compile_function_body(
    fc: &mut Compiler,
    params: &[Param],
    body: &[SpannedStmt],
) -> Result<(), CompileError> {
    fc.begin_scope();
    let mut param_regs = Vec::with_capacity(params.len());
    for param in params {
        param_regs.push(fc.add_local(&param.name, true)?);
    }
    // Default values: evaluated in the callee (so they can use earlier
    // parameters) only when the caller did not pass that argument, like the
    // interpreter. An explicitly passed `null` is kept.
    for (param, &reg) in params.iter().zip(&param_regs) {
        if let Some(default) = &param.default {
            let skip = fc.emit_jump(OpCode::JumpIfArg, reg, 0);
            compile_expr(fc, default, reg)?;
            fc.patch_jump(skip);
        }
    }
    let (init, last) = match body.split_last() {
        Some((last, init)) => (init, Some(last)),
        None => (body, None),
    };
    for s in init {
        fc.set_span(s);
        compile_stmt(fc, &s.stmt)?;
    }
    match last {
        Some(s) => {
            fc.set_span(s);
            if let Stmt::Expression(expr) = &s.stmt {
                let dst = fc.alloc_reg()?;
                compile_expr(fc, expr, dst)?;
                fc.emit(encode_abc(OpCode::Return, dst, 0, 0), 0);
                fc.free_to(dst);
            } else if crate::semantics::is_value_tail(&s.stmt) {
                // `if`/`when`/`match`/`safe` ending a body yield the value
                // of the branch taken (shared rule: semantics::is_value_tail).
                let dst = fc.alloc_reg()?;
                compile_stmt_value(fc, &s.stmt, dst)?;
                fc.emit(encode_abc(OpCode::Return, dst, 0, 0), 0);
                fc.free_to(dst);
            } else {
                compile_stmt(fc, &s.stmt)?;
                fc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
            }
        }
        None => fc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0),
    }
    fc.chunk.arity = params.len() as u8;
    fc.chunk.min_arity =
        crate::semantics::required_params(params.iter().map(|p| p.default.is_some())) as u8;
    Ok(())
}

/// Compile a spawn/squad body: if the last statement is an expression,
/// compile it as a return so the task returns its value (not null).
fn compile_spawn_body(sc: &mut Compiler, body: &[SpannedStmt]) -> Result<(), CompileError> {
    if body.is_empty() {
        sc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
        return Ok(());
    }
    let (init, last) = body.split_at(body.len() - 1);
    for s in init {
        sc.set_span(s);
        compile_stmt(sc, &s.stmt)?;
    }
    let last_stmt = &last[0];
    sc.set_span(last_stmt);
    match &last_stmt.stmt {
        Stmt::Expression(expr) => {
            let dst = sc.alloc_reg()?;
            compile_expr(sc, expr, dst)?;
            sc.emit(encode_abc(OpCode::Return, dst, 0, 0), 0);
            sc.free_to(dst);
        }
        Stmt::Return(Some(expr)) => {
            let dst = sc.alloc_reg()?;
            compile_expr(sc, expr, dst)?;
            sc.emit(encode_abc(OpCode::Return, dst, 0, 0), 0);
            sc.free_to(dst);
        }
        tail if crate::semantics::is_value_tail(tail) => {
            let dst = sc.alloc_reg()?;
            compile_stmt_value(sc, tail, dst)?;
            sc.emit(encode_abc(OpCode::Return, dst, 0, 0), 0);
            sc.free_to(dst);
        }
        other => {
            compile_stmt(sc, other)?;
            sc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
        }
    }
    Ok(())
}

fn compile_stmt(c: &mut Compiler, stmt: &Stmt) -> Result<(), CompileError> {
    match stmt {
        Stmt::Let {
            name,
            mutable,
            value,
            ..
        } => {
            let reg = if matches!(value, Expr::Lambda { .. }) {
                // A lambda may refer to itself recursively: bind first, then
                // sync the binding's upvalue cell if the lambda captured it.
                let reg = c.add_local(name, *mutable)?;
                compile_expr(c, value, reg)?;
                if c.locals
                    .last()
                    .is_some_and(|l| l.register == reg && l.captured)
                {
                    c.emit(encode_abc(OpCode::SetLocal, reg, reg, 0), 0);
                }
                reg
            } else {
                // The initializer sees the previous binding of `name`
                // (`let x = x + 1` shadows), so bind after compiling it.
                let reg = c.alloc_reg()?;
                compile_expr(c, value, reg)?;
                c.free_to(reg);
                c.add_local(name, *mutable)?
            };
            if c.module_mode && c.scope_depth == 1 {
                let name_idx = c.const_str(&c.global_name(name));
                c.emit(encode_abx(OpCode::SetGlobal, reg, name_idx), 0);
            }
            Ok(())
        }

        Stmt::Assign { target, value } => {
            if try_compile_add_local(c, target, value)? {
                return Ok(());
            }
            // Like the interpreter: evaluate the value, then store it.
            let saved = c.next_register;
            let val_reg = c.alloc_reg()?;
            compile_expr(c, value, val_reg)?;
            compile_store(c, target, val_reg)?;
            c.free_to(saved);
            Ok(())
        }

        Stmt::FnDef {
            name, params, body, ..
        } => {
            let mut fc = c.child(name);
            compile_function_body(&mut fc, params, body)?;
            let proto_idx = c.finish_child(fc)?;

            let fn_reg = c.add_local(name, false)?;
            c.emit(encode_abx(OpCode::Closure, fn_reg, proto_idx), 0);

            // Also register as global for recursion and cross-scope access
            let name_idx = c.const_str(&c.global_name(name));
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
                c.set_span(s);
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();

            if let Some(eb) = else_body {
                let end_jump = c.emit_jump(OpCode::Jump, 0, 0);
                c.patch_jump(else_jump);
                c.begin_scope();
                for s in eb {
                    c.set_span(s);
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
            c.push_loop(loop_start, true);

            let saved = c.next_register;
            let cond = c.alloc_reg()?;
            compile_expr(c, condition, cond)?;
            let exit = c.emit_jump(OpCode::JumpIfFalse, cond, 0);
            c.free_to(saved);

            c.begin_scope();
            for s in body {
                c.set_span(s);
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();

            c.emit_loop(loop_start, 0);
            c.patch_jump(exit);
            c.pop_loop()
        }

        Stmt::Loop { body } => {
            let loop_start = c.chunk.code_len();
            c.push_loop(loop_start, true);

            c.begin_scope();
            for s in body {
                c.set_span(s);
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();

            c.emit_loop(loop_start, 0);
            c.pop_loop()
        }

        Stmt::For {
            var,
            var2,
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
            // `continue` must still advance the index, so it jumps forward to
            // the step code instead of back to `loop_start`.
            c.push_loop(loop_start, false);

            let cond_reg = c.alloc_reg()?;
            c.emit(encode_abc(OpCode::IterHas, cond_reg, arr_reg, idx_reg), 0);
            let exit = c.emit_jump(OpCode::JumpIfFalse, cond_reg, 0);
            c.free_to(cond_reg);

            // Each iteration gets a fresh binding: the body scope (including
            // the loop variables) is closed before the step code runs, so
            // closures created in different iterations see different values.
            c.begin_scope();
            if let Some(var2_name) = var2 {
                // `for k, v in ...` — IterGet yields a 2-tuple (k, v); extract
                // each component into its own local. Works for Map, Object,
                // and any sequence whose elements are 2-tuples.
                let pair_reg = c.alloc_reg()?;
                c.emit(encode_abc(OpCode::IterGet, pair_reg, arr_reg, idx_reg), 0);
                let var_reg = c.add_local(var, false)?;
                let var2_reg = c.add_local(var2_name, false)?;
                let k_idx_reg = c.alloc_reg()?;
                let k_const = c.const_int(0);
                c.emit(encode_abx(OpCode::LoadConst, k_idx_reg, k_const), 0);
                c.emit(
                    encode_abc(OpCode::GetIndex, var_reg, pair_reg, k_idx_reg),
                    0,
                );
                let v_const = c.const_int(1);
                c.emit(encode_abx(OpCode::LoadConst, k_idx_reg, v_const), 0);
                c.emit(
                    encode_abc(OpCode::GetIndex, var2_reg, pair_reg, k_idx_reg),
                    0,
                );
                c.free_to(k_idx_reg);
            } else {
                let var_reg = c.add_local(var, false)?;
                c.emit(encode_abc(OpCode::IterGet, var_reg, arr_reg, idx_reg), 0);
            }

            for s in body {
                c.set_span(s);
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();

            let continue_jumps = c
                .loops
                .last_mut()
                .and_then(|ctx| ctx.continue_jumps.take())
                .unwrap_or_default();
            for jump in continue_jumps {
                c.patch_jump(jump);
            }

            let one = c.const_int(1);
            let one_reg = c.alloc_reg()?;
            c.emit(encode_abx(OpCode::LoadConst, one_reg, one), 0);
            c.emit(encode_abc(OpCode::Add, idx_reg, idx_reg, one_reg), 0);
            c.free_to(one_reg);

            c.emit_loop(loop_start, 0);
            c.patch_jump(exit);
            c.pop_loop()?;
            c.free_to(saved);
            Ok(())
        }

        Stmt::Break => {
            if c.loops.is_empty() {
                return Err(CompileError::unsupported("`break` outside of a loop"));
            }
            c.emit_handler_pops_for_loop_exit();
            c.emit_close_placeholder();
            let j = c.emit_jump(OpCode::Jump, 0, 0);
            if let Some(ctx) = c.loops.last_mut() {
                ctx.break_jumps.push(j);
            }
            Ok(())
        }

        Stmt::Continue => {
            if c.loops.is_empty() {
                return Err(CompileError::unsupported("`continue` outside of a loop"));
            }
            c.emit_handler_pops_for_loop_exit();
            c.emit_close_placeholder();
            let to_start = c
                .loops
                .last()
                .is_some_and(|ctx| ctx.continue_jumps.is_none());
            if to_start {
                let start = c.loops.last().map(|ctx| ctx.start).unwrap_or_default();
                c.emit_loop(start, 0);
            } else {
                let j = c.emit_jump(OpCode::Jump, 0, 0);
                if let Some(jumps) = c
                    .loops
                    .last_mut()
                    .and_then(|ctx| ctx.continue_jumps.as_mut())
                {
                    jumps.push(j);
                }
            }
            Ok(())
        }

        Stmt::Match { subject, arms } => compile_match(c, subject, arms, None),

        Stmt::Expression(expr) => {
            if try_compile_push_statement(c, expr)? {
                return Ok(());
            }
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

        // `@server(port: 8080, ...)` with literal arguments is pure metadata:
        // the host runtime reads it from the AST (`runtime::metadata`) for
        // both engines, so there is nothing to execute. Anything else keeps
        // the program on the interpreter.
        Stmt::DecoratorStmt(decorator) => {
            match crate::runtime::metadata::vm_unsupported_decorator(decorator, true) {
                None => Ok(()),
                Some(issue) => Err(CompileError::unsupported(&issue)),
            }
        }

        Stmt::StructDef { name, fields, .. } => compile_hidden_stmt(
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
            // `__forge_destructure` applies the interpreter's rules (missing
            // fields / items become null, wrong container type is an error)
            // and returns the bound values in order.
            let value_reg = c.alloc_reg()?;
            compile_expr(c, value, value_reg)?;
            let (kind, names, rest): (&str, &[String], Option<&String>) = match pattern {
                DestructurePattern::Object(names) => ("object", names, None),
                DestructurePattern::Array { items, rest } => ("array", items, rest.as_ref()),
                DestructurePattern::Tuple(names) => ("tuple", names, None),
            };
            let parts_reg = c.alloc_reg()?;
            let saved = c.next_register;
            let kind_reg = c.alloc_reg()?;
            let kind_idx = c.const_str(kind);
            c.emit(encode_abx(OpCode::LoadConst, kind_reg, kind_idx), 0);
            let names_reg = c.alloc_reg()?;
            compile_expr(
                c,
                &Expr::Array(names.iter().map(|n| Expr::StringLit(n.clone())).collect()),
                names_reg,
            )?;
            let rest_flag_reg = c.alloc_reg()?;
            let flag_op = if rest.is_some() {
                OpCode::LoadTrue
            } else {
                OpCode::LoadFalse
            };
            c.emit(encode_abc(flag_op, rest_flag_reg, 0, 0), 0);
            compile_hidden_call_from_regs(
                c,
                "__forge_destructure",
                &[value_reg, kind_reg, names_reg, rest_flag_reg],
                parts_reg,
            )?;
            c.free_to(saved);

            let targets: Vec<&String> = names.iter().chain(rest).collect();
            let target_regs: Vec<u8> = targets
                .iter()
                .map(|name| c.add_local(name, false))
                .collect::<Result<_, _>>()?;
            let idx_reg = c.alloc_reg()?;
            for (index, target_reg) in target_regs.into_iter().enumerate() {
                let const_idx = c.const_int(index as i64);
                c.emit(encode_abx(OpCode::LoadConst, idx_reg, const_idx), 0);
                c.emit(
                    encode_abc(OpCode::GetIndex, target_reg, parts_reg, idx_reg),
                    0,
                );
            }
            c.free_to(idx_reg);
            Ok(())
        }

        Stmt::YieldStmt(_) => compile_hidden_stmt(
            c,
            "__forge_raise_error",
            vec![Expr::StringLit(
                crate::semantics::YIELD_UNSUPPORTED.to_string(),
            )],
        ),

        Stmt::When { subject, arms } => {
            let saved = c.next_register;
            let dst = c.alloc_reg()?;
            compile_when(c, subject, arms, dst)?;
            c.free_to(saved);
            Ok(())
        }

        Stmt::CheckStmt { expr, check_kind } => {
            let (kind, extra): (&str, Vec<Expr>) = match check_kind {
                CheckKind::IsNotEmpty => ("not_empty", vec![]),
                CheckKind::Contains(needle) => ("contains", vec![needle.clone()]),
                CheckKind::Between(lo, hi) => ("between", vec![lo.clone(), hi.clone()]),
                CheckKind::IsTrue => ("true", vec![]),
            };
            let mut args = vec![Expr::StringLit(kind.to_string()), expr.clone()];
            args.extend(extra);
            compile_hidden_stmt(c, "__forge_check", args)
        }

        Stmt::SafeBlock { body } => compile_safe_block(c, body, None),

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
                c.set_span(s);
                compile_stmt(c, &s.stmt)?;
            }
            let captured = c.end_scope();

            c.cleanup_contexts.pop();
            c.emit(encode_abc(OpCode::PopTimeout, 0, 0, 0), 0);
            c.cleanup_contexts.pop();
            c.emit(encode_abc(OpCode::PopHandler, 0, 0, 0), 0);
            let end_jump = c.emit_jump(OpCode::Jump, 0, 0);

            c.patch_jump(timeout_jump);
            c.patch_jump(handler_jump);
            if let Some(range) = captured {
                c.emit_close(range);
            }
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
                c.set_span(s);
                compile_stmt(c, &s.stmt)?;
            }
            let captured = c.end_scope();
            c.cleanup_contexts.pop();
            c.emit(encode_abc(OpCode::PopHandler, 0, 0, 0), 0);
            let success_jump = c.emit_jump(OpCode::Jump, 0, 0);

            c.patch_jump(handler_jump);
            if let Some(range) = captured {
                c.emit_close(range);
            }

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
            let mut sc = c.child("<schedule>");
            sc.begin_scope();
            for s in body {
                sc.set_span(s);
                compile_stmt(&mut sc, &s.stmt)?;
            }
            sc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
            let proto = c.finish_child(sc)?;
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
            let mut sc = c.child("<watch>");
            sc.begin_scope();
            for s in body {
                sc.set_span(s);
                compile_stmt(&mut sc, &s.stmt)?;
            }
            sc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
            let proto = c.finish_child(sc)?;
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
            let saved = c.next_register;
            let catch_reg = c.alloc_reg()?;
            let handler_jump = c.emit_jump(OpCode::PushHandler, catch_reg, 0);
            c.cleanup_contexts.push(CleanupContext {
                kind: CleanupKind::Handler,
                loop_depth: c.loops.len(),
            });
            c.begin_scope();
            for s in try_body {
                c.set_span(s);
                compile_stmt(c, &s.stmt)?;
            }
            let captured = c.end_scope();
            c.cleanup_contexts.pop();
            c.emit(encode_abc(OpCode::PopHandler, 0, 0, 0), 0);
            let end_jump = c.emit_jump(OpCode::Jump, 0, 0);

            c.patch_jump(handler_jump);
            if let Some(range) = captured {
                c.emit_close(range);
            }
            c.begin_scope();
            c.locals.push(Local {
                name: catch_var.clone(),
                depth: c.scope_depth,
                register: catch_reg,
                mutable: false,
                captured: false,
            });
            for s in catch_body {
                c.set_span(s);
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();
            c.patch_jump(end_jump);
            c.free_to(saved);
            Ok(())
        }

        Stmt::Import { path, names } => {
            if crate::semantics::BUILTIN_MODULES.contains(&path.as_str()) {
                return Ok(());
            }

            let resolved = resolve_import_path(c, path)?;
            let resolved_path = resolved.display().to_string();
            let export_names = match names {
                Some(name_list) => name_list.clone(),
                None => import_export_names(&parse_import_program(path, &resolved)?),
            };

            // (resolved path, requested names or `false` for all, path as written)
            let import_args = vec![
                Expr::StringLit(resolved_path),
                match names {
                    Some(name_list) => Expr::Array(
                        name_list
                            .iter()
                            .map(|name| Expr::StringLit(name.clone()))
                            .collect(),
                    ),
                    None => Expr::Bool(false),
                },
                Expr::StringLit(path.clone()),
            ];

            // The module runs even when it exports nothing (top-level side
            // effects happen exactly as in the interpreter).
            let module_reg = c.alloc_reg()?;
            compile_hidden_call(c, "__forge_import_module", import_args, module_reg)?;
            for name in export_names {
                let local_reg = c.add_local(&name, false)?;
                emit_get_field(c, local_reg, module_reg, &name)?;
                if c.scope_depth == 1 {
                    // Top-level imports are also globals, as in the
                    // interpreter (pattern matching on imported variants
                    // looks them up by name).
                    let name_idx = c.const_str(&c.global_name(&name));
                    c.emit(encode_abx(OpCode::SetGlobal, local_reg, name_idx), 0);
                }
            }
            Ok(())
        }

        Stmt::ImportNative { path, binding } => {
            // Loading happens at run time in the shared `plugins::import`
            // (resolution, `ffi` check, registry); the bound names are known
            // now, so they compile to ordinary locals.
            let base_dir = c
                .base_dir
                .as_ref()
                .map(|d| d.display().to_string())
                .unwrap_or_default();
            let names_arg = match binding {
                NativeBinding::Names(names) => Expr::Array(
                    names
                        .iter()
                        .map(|name| Expr::StringLit(name.clone()))
                        .collect(),
                ),
                NativeBinding::Namespace(_) => Expr::Bool(false),
            };
            let import_args = vec![
                Expr::StringLit(path.clone()),
                Expr::StringLit(base_dir),
                names_arg,
            ];
            let namespace_reg = c.alloc_reg()?;
            compile_hidden_call(c, "__forge_import_native", import_args, namespace_reg)?;
            let bind_global = |c: &mut Compiler, name: &str, reg: u8| {
                if c.scope_depth == 1 {
                    // Top-level imports are also globals, as in the interpreter.
                    let name_idx = c.const_str(&c.global_name(name));
                    c.emit(encode_abx(OpCode::SetGlobal, reg, name_idx), 0);
                }
            };
            match binding {
                NativeBinding::Namespace(alias) => {
                    let local_reg = c.add_local(alias, false)?;
                    c.emit(encode_abc(OpCode::Move, local_reg, namespace_reg, 0), 0);
                    bind_global(c, alias, local_reg);
                }
                NativeBinding::Names(names) => {
                    for name in names {
                        let local_reg = c.add_local(name, false)?;
                        emit_get_field(c, local_reg, namespace_reg, name)?;
                        bind_global(c, name, local_reg);
                    }
                }
            }
            Ok(())
        }

        Stmt::Spawn { body } => {
            let mut sc = c.child("<spawn>");
            sc.begin_scope();
            compile_spawn_body(&mut sc, body)?;
            let proto = c.finish_child(sc)?;
            let cr = c.alloc_reg()?;
            c.emit(encode_abx(OpCode::Closure, cr, proto), 0);
            c.emit(encode_abc(OpCode::Spawn, cr, 0, 0), 0);
            c.free_to(cr);
            Ok(())
        }

        Stmt::Squad { body } => {
            let dst = c.alloc_reg()?;
            c.emit(encode_abc(OpCode::SquadBegin, dst, 0, 0), c.current_line);
            c.begin_scope();
            for s in body {
                c.set_span(s);
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();
            c.emit(encode_abc(OpCode::SquadEnd, dst, 0, 0), c.current_line);
            c.free_to(dst);
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
            } else if let Some(uv_idx) = c.resolve_capture(name) {
                c.emit(encode_abc(OpCode::GetUpvalue, dst, uv_idx, 0), 0);
            } else {
                let idx = c.const_str(&c.global_name(name));
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
                if try_compile_mutating_call(c, object, field, args, dst)? {
                    return Ok(());
                }
                let mut lowered_args = Vec::with_capacity(args.len() + 2);
                lowered_args.push((**object).clone());
                lowered_args.push(Expr::StringLit(field.clone()));
                lowered_args.extend(args.clone());
                return compile_hidden_call(c, "__forge_call_method", lowered_args, dst);
            }
            // `push(xs, v)` / `pop(xs)` on a mutable variable update it in
            // place, exactly like the method forms.
            if let (Expr::Ident(fn_name), Some(Expr::Ident(_))) = (function.as_ref(), args.first())
            {
                let is_builtin =
                    c.resolve_local(fn_name).is_none() && c.binding_mutability(fn_name).is_none();
                let arity_ok = match fn_name.as_str() {
                    "push" => args.len() == 2,
                    "pop" => args.len() == 1,
                    _ => false,
                };
                if is_builtin && arity_ok {
                    let receiver = &args[0];
                    if try_compile_mutating_call(c, receiver, fn_name, &args[1..], dst)? {
                        return Ok(());
                    }
                }
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
            emit_get_field(c, dst, or, field)?;
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
        Expr::Array(items) if items.iter().any(|item| matches!(item, Expr::Spread(_))) => {
            // "1" marks a spread position: arrays are flattened into the
            // result, any other value is appended as-is (interpreter rules).
            let flags: String = items
                .iter()
                .map(|item| {
                    if matches!(item, Expr::Spread(_)) {
                        '1'
                    } else {
                        '0'
                    }
                })
                .collect();
            let mut lowered = vec![Expr::StringLit(flags)];
            lowered.extend(items.iter().map(|item| match item {
                Expr::Spread(inner) => (**inner).clone(),
                other => other.clone(),
            }));
            compile_hidden_call(c, "__forge_array_spread", lowered, dst)?;
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
        Expr::Tuple(items) => {
            let start = c.next_register;
            for item in items {
                let r = c.alloc_reg()?;
                compile_expr(c, item, r)?;
            }
            c.emit(
                encode_abc(OpCode::NewTuple, dst, start, items.len() as u8),
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
            let mut lc = c.child("<lambda>");
            compile_function_body(&mut lc, params, body)?;
            let pi = c.finish_child(lc)?;
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
        Expr::Block(stmts) => compile_block_value(c, stmts, dst)?,
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
            let mut sc = c.child("<spawn>");
            sc.begin_scope();
            compile_spawn_body(&mut sc, body)?;
            let proto = c.finish_child(sc)?;
            c.emit(encode_abx(OpCode::Closure, dst, proto), 0);
            c.emit(encode_abc(OpCode::Spawn, dst, 0, 0), 0);
        }
        Expr::Squad(body) => {
            c.emit(encode_abc(OpCode::SquadBegin, dst, 0, 0), c.current_line);
            c.begin_scope();
            for s in body {
                c.set_span(s);
                compile_stmt(c, &s.stmt)?;
            }
            c.end_scope();
            c.emit(encode_abc(OpCode::SquadEnd, dst, 0, 0), c.current_line);
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
            if try_compile_mutating_call(c, object, method, args, dst)? {
                return Ok(());
            }
            let mut lowered_args = Vec::with_capacity(args.len() + 2);
            lowered_args.push((**object).clone());
            lowered_args.push(Expr::StringLit(method.clone()));
            lowered_args.extend(args.clone());
            compile_hidden_call(c, "__forge_call_method", lowered_args, dst)?;
        }
    }
    Ok(())
}
