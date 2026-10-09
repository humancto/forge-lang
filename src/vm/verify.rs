//! Structural bytecode verifier.
//!
//! The VM's dispatch loop trusts its input: it indexes registers, the
//! constant pool and the prototype table straight from instruction operands.
//! The compiler only ever emits well-formed code, but `forge run app.fgc`
//! (and the `forge_execute_bytecode` C entry point used by AOT binaries)
//! execute *serialized* bytecode, which may have been crafted. A malformed
//! chunk could index past its register window — reading or clobbering
//! another frame's registers, or storing a heap reference where the GC does
//! not scan it — or panic the VM.
//!
//! [`verify_chunk`] checks every rule the dispatch loop relies on, for the
//! root chunk and (recursively) every nested prototype, before a single
//! instruction runs:
//!
//! * **Shape:** `code`, `lines` and `cols` have the same length; the code is
//!   non-empty and its last instruction cannot fall through (the VM would
//!   otherwise resume the *caller's* frame inside the callee's dispatch loop).
//! * **Opcodes:** every opcode byte decodes.
//! * **Registers:** every register operand (including ranges such as call
//!   arguments, array/tuple/object/interpolation operands and
//!   `CloseUpvalues` bounds) lies inside the frame window
//!   `0..max(max_registers, 1)` the VM allocates for the chunk.
//! * **Constants:** constant indices are in range, and name operands
//!   (`GetGlobal`, `SetGlobal`, `GetField`, `SetField`) are strings.
//! * **Prototypes:** `Closure` indices are in range.
//! * **Branches:** every jump / loop / handler / timeout target is an
//!   instruction of the same chunk. Only `Loop` may branch backward (and it
//!   must): it is the back-edge that polls cancellation, so verified code
//!   cannot spin in a loop the host is unable to stop.
//! * **Upvalues:** `upvalue_count` matches `upvalue_sources`; `GetUpvalue` /
//!   `SetUpvalue` indices are in range; a prototype's `Local` sources name a
//!   register of its parent's frame and `Upvalue` sources one of the
//!   parent's upvalues; the root chunk captures nothing.
//! * **Arity:** `min_arity <= arity <= max(max_registers, 1)` (arguments are
//!   copied into registers `0..arity`), and `JumpIfArg` names a parameter.
//! * **Nesting:** prototypes nest at most [`MAX_PROTO_DEPTH`] deep.
//!
//! The verifier is structural, not a type checker: operand *values* are
//! still checked dynamically by the VM (and errors there are ordinary
//! runtime errors, never panics).
//!
//! Every deserialized chunk is verified (`serialize::deserialize_chunk`).
//! Debug builds also verify every chunk the compiler produces, so a
//! compiler change that emits malformed code fails the test suite loudly.

use super::bytecode::{
    decode_a, decode_b, decode_bx, decode_c, decode_op, decode_sbx, Chunk, Constant, OpCode,
    UpvalueSource,
};
use std::fmt;

/// Deepest prototype nesting accepted. Matches the parser's nesting limit,
/// which bounds how deep compiled functions can nest, so verified compiled
/// code can never hit it.
pub const MAX_PROTO_DEPTH: usize = crate::parser::MAX_NESTING;

/// Most instructions accepted in one chunk (also the deserializer's cap).
pub const MAX_CODE_LEN: usize = 1_000_000;

/// A rule violation, located by the chain of chunk names from the root and
/// (when it concerns one instruction) the instruction index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyError {
    /// Chunk names from the root down to the offending chunk, e.g.
    /// `<main> > add`.
    pub chunk: String,
    /// Instruction index inside that chunk, when the error is about one.
    pub ip: Option<usize>,
    pub message: String,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.ip {
            Some(ip) => write!(
                f,
                "invalid bytecode in '{}' at instruction {}: {}",
                self.chunk, ip, self.message
            ),
            None => write!(f, "invalid bytecode in '{}': {}", self.chunk, self.message),
        }
    }
}

impl std::error::Error for VerifyError {}

/// Verify `root` and all of its nested prototypes. See the module docs for
/// the rules.
pub fn verify_chunk(root: &Chunk) -> Result<(), VerifyError> {
    if !root.upvalue_sources.is_empty() {
        return Err(VerifyError {
            chunk: display_name(&root.name),
            ip: None,
            message: format!(
                "the top-level chunk cannot capture upvalues (has {})",
                root.upvalue_sources.len()
            ),
        });
    }
    let mut path = Vec::new();
    verify_rec(root, &mut path)
}

/// Debug-build check on compiler output: malformed bytecode from the
/// compiler is a compiler bug, so it panics with the verifier's diagnosis.
#[inline]
pub(crate) fn debug_verify_compiled(chunk: &Chunk) {
    #[cfg(debug_assertions)]
    if let Err(e) = verify_chunk(chunk) {
        panic!("BUG: the compiler emitted bytecode the verifier rejects: {e}");
    }
    #[cfg(not(debug_assertions))]
    let _ = chunk;
}

fn display_name(name: &str) -> String {
    if name.is_empty() {
        "<anonymous>".to_string()
    } else {
        // Names come from untrusted input: keep diagnostics short and
        // printable.
        let mut s: String = name
            .chars()
            .take(64)
            .map(|c| if c.is_control() { '?' } else { c })
            .collect();
        if name.chars().count() > 64 {
            s.push('…');
        }
        s
    }
}

fn verify_rec<'a>(chunk: &'a Chunk, path: &mut Vec<&'a str>) -> Result<(), VerifyError> {
    path.push(&chunk.name);
    let result = ChunkVerifier::new(chunk, path).run().and_then(|()| {
        if path.len() >= MAX_PROTO_DEPTH && !chunk.prototypes.is_empty() {
            return Err(err_at(
                path,
                None,
                format!("prototypes nest deeper than {}", MAX_PROTO_DEPTH),
            ));
        }
        for proto in &chunk.prototypes {
            verify_captures(chunk, proto, path)?;
            verify_rec(proto, path)?;
        }
        Ok(())
    });
    path.pop();
    result
}

fn err_at(path: &[&str], ip: Option<usize>, message: String) -> VerifyError {
    VerifyError {
        chunk: path
            .iter()
            .map(|n| display_name(n))
            .collect::<Vec<_>>()
            .join(" > "),
        ip,
        message,
    }
}

/// A prototype's upvalue sources are resolved against its parent when the
/// parent executes `Closure`.
fn verify_captures(parent: &Chunk, proto: &Chunk, path: &[&str]) -> Result<(), VerifyError> {
    let parent_regs = frame_registers(parent);
    for (i, src) in proto.upvalue_sources.iter().enumerate() {
        let ok = match *src {
            UpvalueSource::Local(reg) => (reg as usize) < parent_regs,
            UpvalueSource::Upvalue(idx) => (idx as usize) < parent.upvalue_sources.len(),
        };
        if !ok {
            let mut child = path.to_vec();
            child.push(&proto.name);
            return Err(err_at(
                &child,
                None,
                format!(
                    "upvalue {} source {:?} is out of range for parent '{}' ({} registers, {} upvalues)",
                    i,
                    src,
                    display_name(&parent.name),
                    parent_regs,
                    parent.upvalue_sources.len()
                ),
            ));
        }
    }
    Ok(())
}

/// The register window the VM allocates for a frame of `chunk`.
fn frame_registers(chunk: &Chunk) -> usize {
    (chunk.max_registers as usize).max(1)
}

struct ChunkVerifier<'c, 'p> {
    chunk: &'c Chunk,
    path: &'p [&'p str],
    regs: usize,
}

impl<'c, 'p> ChunkVerifier<'c, 'p> {
    fn new(chunk: &'c Chunk, path: &'p [&'p str]) -> Self {
        Self {
            chunk,
            path,
            regs: frame_registers(chunk),
        }
    }

    fn fail(&self, ip: Option<usize>, message: String) -> VerifyError {
        err_at(self.path, ip, message)
    }

    fn run(&self) -> Result<(), VerifyError> {
        let c = self.chunk;
        let len = c.code.len();
        if len == 0 {
            return Err(self.fail(None, "empty code section".to_string()));
        }
        if len > MAX_CODE_LEN {
            return Err(self.fail(
                None,
                format!(
                    "code section has {} instructions (max {})",
                    len, MAX_CODE_LEN
                ),
            ));
        }
        if c.lines.len() != len || c.cols.len() != len {
            return Err(self.fail(
                None,
                format!(
                    "line/column tables ({}/{}) do not match code length {}",
                    c.lines.len(),
                    c.cols.len(),
                    len
                ),
            ));
        }
        if c.min_arity > c.arity {
            return Err(self.fail(
                None,
                format!("min_arity {} exceeds arity {}", c.min_arity, c.arity),
            ));
        }
        if c.arity as usize > self.regs {
            return Err(self.fail(
                None,
                format!(
                    "arity {} exceeds the frame's {} registers",
                    c.arity, self.regs
                ),
            ));
        }
        if c.upvalue_count as usize != c.upvalue_sources.len() {
            return Err(self.fail(
                None,
                format!(
                    "upvalue_count {} does not match {} upvalue sources",
                    c.upvalue_count,
                    c.upvalue_sources.len()
                ),
            ));
        }
        if c.prototypes.len() > u16::MAX as usize + 1 {
            return Err(self.fail(
                None,
                format!(
                    "{} prototypes (max {})",
                    c.prototypes.len(),
                    u16::MAX as usize + 1
                ),
            ));
        }
        for ip in 0..len {
            self.instruction(ip)?;
        }
        let last = c.code[len - 1];
        let terminal = matches!(
            OpCode::try_from(decode_op(last)),
            Ok(OpCode::Return | OpCode::ReturnNull | OpCode::Jump | OpCode::Loop)
        );
        if !terminal {
            return Err(self.fail(
                Some(len - 1),
                "the last instruction can fall off the end of the chunk".to_string(),
            ));
        }
        Ok(())
    }

    fn reg(&self, ip: usize, what: &str, r: u8) -> Result<(), VerifyError> {
        if (r as usize) < self.regs {
            Ok(())
        } else {
            Err(self.fail(
                Some(ip),
                format!(
                    "{} register {} is outside the frame's {} registers",
                    what, r, self.regs
                ),
            ))
        }
    }

    /// Registers `start .. start + count` (empty when `count == 0`).
    fn reg_range(
        &self,
        ip: usize,
        what: &str,
        start: usize,
        count: usize,
    ) -> Result<(), VerifyError> {
        if count == 0 || start + count <= self.regs {
            Ok(())
        } else {
            Err(self.fail(
                Some(ip),
                format!(
                    "{} registers {}..{} exceed the frame's {} registers",
                    what,
                    start,
                    start + count,
                    self.regs
                ),
            ))
        }
    }

    fn constant(&self, ip: usize, idx: usize) -> Result<&Constant, VerifyError> {
        self.chunk.constants.get(idx).ok_or_else(|| {
            self.fail(
                Some(ip),
                format!(
                    "constant index {} out of range ({} constants)",
                    idx,
                    self.chunk.constants.len()
                ),
            )
        })
    }

    fn name_constant(&self, ip: usize, idx: usize) -> Result<(), VerifyError> {
        match self.constant(ip, idx)? {
            Constant::Str(_) => Ok(()),
            other => Err(self.fail(
                Some(ip),
                format!("name operand constant {} is not a string: {:?}", idx, other),
            )),
        }
    }

    fn target(&self, ip: usize, sbx: i16, backward: bool) -> Result<(), VerifyError> {
        // The VM increments ip before applying the offset.
        let target = ip as i64 + 1 + sbx as i64;
        if target < 0 || target as usize >= self.chunk.code.len() {
            return Err(self.fail(
                Some(ip),
                format!(
                    "branch target {} is outside the chunk (0..{})",
                    target,
                    self.chunk.code.len()
                ),
            ));
        }
        // Back-edges must be `Loop`: it is the instruction that polls
        // cancellation (and counts JIT tier-up), so a backward `Jump` could
        // spin forever without ever being stoppable.
        if backward != (target <= ip as i64) {
            return Err(self.fail(
                Some(ip),
                if backward {
                    format!("Loop target {} does not branch backward", target)
                } else {
                    format!(
                        "forward branch targets {} at or before itself (back-edges must use Loop)",
                        target
                    )
                },
            ));
        }
        Ok(())
    }

    fn instruction(&self, ip: usize) -> Result<(), VerifyError> {
        let inst = self.chunk.code[ip];
        let op = decode_op(inst);
        let a = decode_a(inst);
        let b = decode_b(inst);
        let c = decode_c(inst);
        let bx = decode_bx(inst) as usize;
        let sbx = decode_sbx(inst);
        let opcode = OpCode::try_from(op)
            .map_err(|bad| self.fail(Some(ip), format!("invalid opcode {}", bad)))?;
        use OpCode::*;
        match opcode {
            LoadConst => {
                self.reg(ip, "destination", a)?;
                self.constant(ip, bx)?;
            }
            LoadNull | LoadTrue | LoadFalse | Spawn | SquadBegin | SquadEnd | Return => {
                self.reg(ip, "operand", a)?;
            }
            Add | Sub | Mul | Div | Mod | Eq | NotEq | Lt | Gt | LtEq | GtEq | And | Or
            | Concat | GetIndex | IterGet | SetIndex | IterHas | Schedule => {
                self.reg(ip, "A", a)?;
                self.reg(ip, "B", b)?;
                self.reg(ip, "C", c)?;
            }
            Neg | Not | Move | GetLocal | SetLocal | AddLocal | PushLocal | PopLocal | Len
            | Try | Await | Must | Ask | Freeze | Watch | ExtractField => {
                self.reg(ip, "A", a)?;
                self.reg(ip, "B", b)?;
            }
            GetGlobal | SetGlobal => {
                self.reg(ip, "value", a)?;
                self.name_constant(ip, bx)?;
            }
            NewArray | NewTuple | Interpolate => {
                self.reg(ip, "destination", a)?;
                self.reg_range(ip, "element", b as usize, c as usize)?;
            }
            NewObject => {
                self.reg(ip, "destination", a)?;
                self.reg_range(ip, "key/value", b as usize, 2 * c as usize)?;
            }
            GetField => {
                self.reg(ip, "destination", a)?;
                self.reg(ip, "object", b)?;
                self.name_constant(ip, c as usize)?;
            }
            SetField => {
                self.reg(ip, "object", a)?;
                self.name_constant(ip, b as usize)?;
                self.reg(ip, "value", c)?;
            }
            ForRangePrep => {
                // Counting loop (vm/range_loop.rs): reads the callee in R(A)
                // and its B arguments, writes the bounds to R(A), R(A+1)
                // and the fast-path flag to R(C).
                self.reg(ip, "callee", a)?;
                self.reg_range(ip, "argument", a as usize + 1, (b as usize).max(1))?;
                self.reg(ip, "mode", c)?;
            }
            ForRangeNext => {
                // Reads/writes the bounds R(A), R(A+1), writes the loop
                // variable R(B), and on success skips the exit jump at ip+1.
                self.reg_range(ip, "bounds", a as usize, 2)?;
                self.reg(ip, "loop variable", b)?;
                if ip + 1 >= self.chunk.code.len() {
                    return Err(self.fail(
                        Some(ip),
                        "ForRangeNext must be followed by its exit jump".to_string(),
                    ));
                }
            }
            Jump => self.target(ip, sbx, false)?,
            Loop => self.target(ip, sbx, true)?,
            JumpIfFalse | JumpIfTrue | PushHandler | PushTimeout => {
                self.reg(ip, "operand", a)?;
                self.target(ip, sbx, false)?;
            }
            JumpIfArg => {
                if a >= self.chunk.arity {
                    return Err(self.fail(
                        Some(ip),
                        format!(
                            "JumpIfArg names parameter {} but arity is {}",
                            a, self.chunk.arity
                        ),
                    ));
                }
                self.target(ip, sbx, false)?;
            }
            Call => {
                self.reg(ip, "callee", a)?;
                self.reg_range(ip, "argument", a as usize + 1, b as usize)?;
                self.reg(ip, "destination", c)?;
            }
            ReturnNull | Pop | PopHandler | PopTimeout => {}
            Closure => {
                self.reg(ip, "destination", a)?;
                if bx >= self.chunk.prototypes.len() {
                    return Err(self.fail(
                        Some(ip),
                        format!(
                            "prototype index {} out of range ({} prototypes)",
                            bx,
                            self.chunk.prototypes.len()
                        ),
                    ));
                }
            }
            GetUpvalue => {
                self.reg(ip, "destination", a)?;
                self.upvalue(ip, b)?;
            }
            SetUpvalue => {
                self.upvalue(ip, a)?;
                self.reg(ip, "source", b)?;
            }
            CloseUpvalues => {
                self.reg(ip, "first", a)?;
                self.reg(ip, "last", b)?;
            }
        }
        Ok(())
    }

    fn upvalue(&self, ip: usize, idx: u8) -> Result<(), VerifyError> {
        if (idx as usize) < self.chunk.upvalue_sources.len() {
            Ok(())
        } else {
            Err(self.fail(
                Some(ip),
                format!(
                    "upvalue index {} out of range ({} upvalues)",
                    idx,
                    self.chunk.upvalue_sources.len()
                ),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::bytecode::{encode_abc, encode_abx, encode_asbx};

    /// A valid chunk: `R0 = K0; return R0`.
    fn base() -> Chunk {
        let mut c = Chunk::new("<main>");
        c.max_registers = 2;
        c.add_constant(Constant::Int(1));
        c.add_constant(Constant::Str("name".into()));
        c.emit(encode_abx(OpCode::LoadConst, 0, 0), 1);
        c.emit(encode_abc(OpCode::Return, 0, 0, 0), 1);
        c
    }

    /// `base()` with `inst` inserted before the final `Return`.
    fn with(inst: u32) -> Chunk {
        let mut c = base();
        let ret = c.code.pop().unwrap();
        c.lines.pop();
        c.cols.pop();
        c.emit(inst, 1);
        c.emit(ret, 1);
        c
    }

    fn rejects(c: &Chunk, needle: &str) {
        let err = verify_chunk(c).expect_err("verifier accepted a malformed chunk");
        assert!(
            err.to_string().contains(needle),
            "expected error containing {:?}, got: {}",
            needle,
            err
        );
    }

    #[test]
    fn accepts_valid_chunk() {
        verify_chunk(&base()).unwrap();
    }

    #[test]
    fn accepts_compiled_programs() {
        let src = r#"
            let mut total = 0
            fn add(a, b = 2) { return a + b }
            let f = fn(x) {
                total = total + x
                return total
            }
            for i in range(0, 3) { total = add(total, i) }
            let o = { a: 1, b: [1, 2, (3, 4)] }
            try { let z = o.a / 0 } catch e { total = total + 1 }
            match o.a { 1 => total, _ => 0 }
            say "value: {total}"
        "#;
        let tokens = crate::lexer::Lexer::new(src).tokenize().unwrap();
        let program = crate::parser::Parser::new(tokens).parse_program().unwrap();
        let chunk = crate::vm::compiler::compile(&program).unwrap();
        verify_chunk(&chunk).unwrap();
    }

    #[test]
    fn rejects_empty_code() {
        rejects(&Chunk::new("<main>"), "empty code section");
    }

    #[test]
    fn rejects_fall_through_at_end() {
        let mut c = base();
        c.emit(encode_abc(OpCode::LoadNull, 0, 0, 0), 1);
        rejects(&c, "fall off the end");
    }

    #[test]
    fn rejects_mismatched_line_table() {
        let mut c = base();
        c.lines.pop();
        rejects(&c, "line/column tables");
        let mut c = base();
        c.cols.push(0);
        rejects(&c, "line/column tables");
    }

    #[test]
    fn rejects_invalid_opcode() {
        rejects(&with(0xFF00_0000), "invalid opcode 255");
        // The first byte that is not an opcode (stays correct as opcodes
        // are added).
        let first_invalid = (0u8..=255)
            .find(|b| OpCode::try_from(*b).is_err())
            .expect("some byte is not an opcode");
        rejects(&with((first_invalid as u32) << 24), "invalid opcode");
    }

    #[test]
    fn rejects_out_of_range_registers() {
        rejects(&with(encode_abc(OpCode::LoadNull, 2, 0, 0)), "register 2");
        rejects(&with(encode_abc(OpCode::Add, 0, 1, 200)), "C register 200");
        rejects(&with(encode_abc(OpCode::Move, 0, 9, 0)), "B register 9");
        rejects(&with(encode_abc(OpCode::Return, 7, 0, 0)), "register 7");
        rejects(&with(encode_abc(OpCode::SetLocal, 3, 0, 0)), "register 3");
        rejects(
            &with(encode_abc(OpCode::CloseUpvalues, 0, 5, 0)),
            "register 5",
        );
        rejects(
            &with(encode_asbx(OpCode::PushHandler, 4, 0)),
            "operand register 4",
        );
    }

    #[test]
    fn max_registers_zero_still_has_one_register() {
        let mut c = Chunk::new("<main>");
        c.emit(encode_abc(OpCode::LoadNull, 0, 0, 0), 1);
        c.emit(encode_abc(OpCode::Return, 0, 0, 0), 1);
        verify_chunk(&c).unwrap();
        c.code[0] = encode_abc(OpCode::LoadNull, 1, 0, 0);
        rejects(&c, "register 1");
    }

    #[test]
    fn rejects_out_of_range_register_ranges() {
        rejects(
            &with(encode_abc(OpCode::NewArray, 0, 1, 2)),
            "element registers 1..3",
        );
        rejects(
            &with(encode_abc(OpCode::NewTuple, 0, 0, 3)),
            "element registers",
        );
        rejects(
            &with(encode_abc(OpCode::Interpolate, 0, 255, 255)),
            "element registers",
        );
        rejects(
            &with(encode_abc(OpCode::NewObject, 0, 0, 2)),
            "key/value registers 0..4",
        );
        rejects(
            &with(encode_abc(OpCode::Call, 0, 2, 0)),
            "argument registers 1..3",
        );
        rejects(
            &with(encode_abc(OpCode::Call, 1, 0, 2)),
            "destination register 2",
        );
        // Empty ranges are fine wherever they start.
        verify_chunk(&with(encode_abc(OpCode::NewArray, 0, 200, 0))).unwrap();
        verify_chunk(&with(encode_abc(OpCode::Call, 1, 0, 0))).unwrap();
    }

    #[test]
    fn rejects_bad_constant_indices_and_kinds() {
        rejects(
            &with(encode_abx(OpCode::LoadConst, 0, 2)),
            "constant index 2",
        );
        rejects(
            &with(encode_abx(OpCode::GetGlobal, 0, 9)),
            "constant index 9",
        );
        rejects(&with(encode_abx(OpCode::SetGlobal, 0, 0)), "not a string");
        rejects(&with(encode_abc(OpCode::GetField, 0, 0, 0)), "not a string");
        rejects(
            &with(encode_abc(OpCode::SetField, 0, 7, 0)),
            "constant index 7",
        );
        verify_chunk(&with(encode_abx(OpCode::GetGlobal, 0, 1))).unwrap();
        verify_chunk(&with(encode_abc(OpCode::SetField, 0, 1, 1))).unwrap();
    }

    #[test]
    fn rejects_bad_branch_targets() {
        // Target = ip + 1 + sbx; ip of the inserted instruction is 1.
        rejects(&with(encode_asbx(OpCode::Jump, 0, 5)), "branch target 7");
        rejects(&with(encode_asbx(OpCode::Loop, 0, -3)), "branch target -1");
        rejects(
            &with(encode_asbx(OpCode::JumpIfFalse, 0, 1)),
            "branch target 3",
        );
        rejects(
            &with(encode_asbx(OpCode::PushTimeout, 0, i16::MAX)),
            "branch target",
        );
        rejects(
            &with(encode_asbx(OpCode::PushHandler, 0, i16::MIN)),
            "branch target",
        );
        verify_chunk(&with(encode_asbx(OpCode::JumpIfTrue, 0, 0))).unwrap();
        verify_chunk(&with(encode_asbx(OpCode::Loop, 0, -2))).unwrap();
    }

    #[test]
    fn back_edges_must_be_loop() {
        // A backward `Jump` would spin without polling cancellation.
        rejects(
            &with(encode_asbx(OpCode::Jump, 0, -2)),
            "back-edges must use Loop",
        );
        rejects(
            &with(encode_asbx(OpCode::Jump, 0, -1)),
            "back-edges must use Loop",
        );
        rejects(&with(encode_asbx(OpCode::JumpIfFalse, 0, -2)), "back-edges");
        rejects(&with(encode_asbx(OpCode::PushHandler, 0, -1)), "back-edges");
        rejects(
            &with(encode_asbx(OpCode::Loop, 0, 0)),
            "does not branch backward",
        );
        // `Jump +0` is the compiler's no-op placeholder.
        verify_chunk(&with(encode_asbx(OpCode::Jump, 0, 0))).unwrap();
    }

    #[test]
    fn rejects_bad_prototype_index() {
        rejects(
            &with(encode_abx(OpCode::Closure, 0, 0)),
            "prototype index 0",
        );
    }

    #[test]
    fn rejects_bad_upvalue_indices() {
        rejects(
            &with(encode_abc(OpCode::GetUpvalue, 0, 0, 0)),
            "upvalue index 0",
        );
        rejects(
            &with(encode_abc(OpCode::SetUpvalue, 3, 0, 0)),
            "upvalue index 3",
        );
    }

    #[test]
    fn rejects_upvalue_count_mismatch() {
        let mut parent = base();
        let mut proto = base();
        proto.name = "f".into();
        proto.upvalue_count = 1;
        parent.prototypes.push(proto);
        rejects(&parent, "upvalue_count 1 does not match 0");
    }

    #[test]
    fn rejects_root_upvalues() {
        let mut c = base();
        c.upvalue_sources.push(UpvalueSource::Local(0));
        c.upvalue_count = 1;
        rejects(&c, "top-level chunk cannot capture");
    }

    #[test]
    fn rejects_bad_capture_sources() {
        let mk = |src: UpvalueSource| {
            let mut parent = base();
            let mut proto = with(encode_abc(OpCode::GetUpvalue, 0, 0, 0));
            proto.name = "inner".into();
            proto.upvalue_sources.push(src);
            proto.upvalue_count = 1;
            parent.prototypes.push(proto);
            parent
        };
        verify_chunk(&mk(UpvalueSource::Local(1))).unwrap();
        rejects(&mk(UpvalueSource::Local(2)), "<main> > inner");
        rejects(&mk(UpvalueSource::Upvalue(0)), "out of range for parent");
    }

    #[test]
    fn rejects_bad_arity() {
        let mut c = base();
        c.arity = 3;
        rejects(&c, "arity 3 exceeds");
        let mut c = base();
        c.arity = 1;
        c.min_arity = 2;
        rejects(&c, "min_arity 2 exceeds arity 1");
        rejects(
            &with(encode_asbx(OpCode::JumpIfArg, 0, 0)),
            "JumpIfArg names parameter 0",
        );
        let mut c = with(encode_asbx(OpCode::JumpIfArg, 0, 0));
        c.arity = 1;
        verify_chunk(&c).unwrap();
    }

    #[test]
    fn errors_in_nested_prototypes_name_the_path() {
        let mut inner = with(encode_abc(OpCode::LoadNull, 99, 0, 0));
        inner.name = "inner".into();
        let mut outer = base();
        outer.name = "outer".into();
        outer.prototypes.push(inner);
        let mut root = base();
        root.prototypes.push(outer);
        let err = verify_chunk(&root).unwrap_err();
        assert_eq!(err.chunk, "<main> > outer > inner");
        assert_eq!(err.ip, Some(1));
    }

    #[test]
    fn rejects_excessive_nesting() {
        let mut c = base();
        for _ in 0..MAX_PROTO_DEPTH {
            let mut parent = base();
            parent.prototypes.push(c);
            c = parent;
        }
        rejects(&c, "nest deeper than");
    }

    #[test]
    fn hostile_names_are_sanitized_in_errors() {
        let mut c = Chunk::new(&format!("\x1b[31m{}", "x".repeat(500)));
        c.max_registers = 1;
        let err = verify_chunk(&c).unwrap_err().to_string();
        assert!(!err.contains('\x1b'));
        assert!(err.len() < 200, "{err}");
    }
}
