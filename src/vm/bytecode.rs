/// Bytecode opcodes for the Forge register-based VM.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
#[allow(dead_code)]
pub enum OpCode {
    LoadConst, // A=dst, Bx=const_idx
    LoadNull,  // A=dst
    LoadTrue,  // A=dst
    LoadFalse, // A=dst
    Add,       // A=dst, B=left, C=right
    Sub,
    Mul,
    Div,
    Mod,
    Neg, // A=dst, B=src
    Eq,  // A=dst, B=left, C=right
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And, // A=dst, B=left, C=right
    Or,
    Not,         // A=dst, B=src
    Move,        // A=dst, B=src
    GetLocal,    // A=dst, B=slot
    SetLocal,    // A=slot, B=src
    GetGlobal,   // A=dst, Bx=name_const_idx
    SetGlobal,   // Bx=name_const_idx, A=src
    NewArray,    // A=dst, B=start_reg, C=count
    NewObject,   // A=dst, B=pair_count (keys/vals in const pool starting at Bx)
    GetField,    // A=dst, B=obj_reg, C=field_name_const_idx
    SetField,    // A=obj_reg, B=field_name_const_idx, C=val_reg
    GetIndex,    // A=dst, B=obj_reg, C=idx_reg
    SetIndex,    // A=obj_reg, B=idx_reg, C=val_reg
    Jump,        // sBx=signed offset
    JumpIfFalse, // A=cond_reg, sBx=signed offset
    JumpIfTrue,  // A=cond_reg, sBx=signed offset
    Loop,        // sBx=negative offset
    Call,        // A=func_reg, B=arg_count, C=dst_reg
    Return,      // A=src_reg
    ReturnNull,
    Closure,      // A=dst, Bx=fn_prototype_idx
    Concat,       // A=dst, B=left, C=right
    Len,          // A=dst, B=src
    Try,          // A=dst, B=src
    Spawn,        // A=closure_reg
    ExtractField, // A=dst, B=obj_reg, C=field_index (0="_0", 1="_1", etc.)
    Interpolate,  // A=dst, B=start_reg, C=part_count
    GetUpvalue,   // A=dst, B=upvalue_index
    SetUpvalue,   // A=upvalue_index, B=src
    Pop,
}

/// Compile-time constant — can hold strings, unlike the runtime Value.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Constant {
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    Str(String),
}

/// A compiled bytecode chunk (one per function/closure/module).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Chunk {
    pub code: Vec<u32>,
    pub constants: Vec<Constant>,
    pub lines: Vec<usize>,
    pub name: String,
    pub prototypes: Vec<Chunk>,
    pub max_registers: u8,
    pub upvalue_count: u8,
    pub arity: u8,
    pub upvalue_sources: Vec<u8>,
}

impl Chunk {
    pub fn new(name: &str) -> Self {
        Self {
            code: Vec::new(),
            constants: Vec::new(),
            lines: Vec::new(),
            name: name.to_string(),
            prototypes: Vec::new(),
            max_registers: 0,
            upvalue_count: 0,
            arity: 0,
            upvalue_sources: Vec::new(),
        }
    }

    pub fn emit(&mut self, instruction: u32, line: usize) {
        self.code.push(instruction);
        self.lines.push(line);
    }

    pub fn add_constant(&mut self, value: Constant) -> u16 {
        for (i, existing) in self.constants.iter().enumerate() {
            if existing.identical(&value) {
                return i as u16;
            }
        }
        let idx = self.constants.len();
        self.constants.push(value);
        idx as u16
    }

    pub fn code_len(&self) -> usize {
        self.code.len()
    }

    pub fn patch_jump(&mut self, offset: usize, target: usize) {
        let instruction = self.code[offset];
        let op = instruction >> 24;
        let a = (instruction >> 16) & 0xFF;
        let jump = target as i16 - offset as i16 - 1;
        let jump_bits = (jump as u16) as u32;
        self.code[offset] = (op << 24) | (a << 16) | jump_bits;
    }
}

impl Constant {
    pub fn identical(&self, other: &Constant) -> bool {
        match (self, other) {
            (Constant::Int(a), Constant::Int(b)) => a == b,
            (Constant::Float(a), Constant::Float(b)) => a == b,
            (Constant::Bool(a), Constant::Bool(b)) => a == b,
            (Constant::Null, Constant::Null) => true,
            (Constant::Str(a), Constant::Str(b)) => a == b,
            _ => false,
        }
    }
}

pub fn encode_abc(op: OpCode, a: u8, b: u8, c: u8) -> u32 {
    ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
}

pub fn encode_abx(op: OpCode, a: u8, bx: u16) -> u32 {
    ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
}

pub fn encode_asbx(op: OpCode, a: u8, sbx: i16) -> u32 {
    ((op as u32) << 24) | ((a as u32) << 16) | ((sbx as u16) as u32)
}

#[inline(always)]
pub fn decode_op(instruction: u32) -> u8 {
    (instruction >> 24) as u8
}

#[inline(always)]
pub fn decode_a(instruction: u32) -> u8 {
    ((instruction >> 16) & 0xFF) as u8
}

#[inline(always)]
pub fn decode_b(instruction: u32) -> u8 {
    ((instruction >> 8) & 0xFF) as u8
}

#[inline(always)]
pub fn decode_c(instruction: u32) -> u8 {
    (instruction & 0xFF) as u8
}

#[inline(always)]
pub fn decode_bx(instruction: u32) -> u16 {
    (instruction & 0xFFFF) as u16
}

#[inline(always)]
pub fn decode_sbx(instruction: u32) -> i16 {
    (instruction & 0xFFFF) as i16
}
