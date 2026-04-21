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
    PushHandler, // A=error_reg, sBx=catch_target
    PopHandler,
    PushTimeout, // A=duration/error reg, sBx=catch_target
    PopTimeout,
    Await,      // A=dst, B=src (if TaskHandle: block + deserialize; else: pass through)
    Schedule,   // A=closure_reg, B=interval_reg, C=unit_reg
    Watch,      // A=closure_reg, B=path_reg
    Must,       // A=dst, B=src (unwrap Ok, crash on Err/null)
    Ask,        // A=dst, B=prompt_reg (call LLM API)
    Freeze,     // A=dst, B=src (wrap value as frozen/immutable)
    NewTuple,   // A=dst, B=start_reg, C=count
    IterGet, // A=dst, B=obj_reg, C=idx_reg — like GetIndex but allows Set (for for-loop iteration)
    SquadBegin, // A=dst (push squad context for collecting spawn handles)
    SquadEnd, // A=dst (join all collected handles, produce result array)
}

// Compile-time guard: if a new variant is added to OpCode, this assertion
// will fail, reminding you to update the TryFrom impl below.
const _: () = assert!(OpCode::SquadEnd as u8 + 1 == 61);

impl TryFrom<u8> for OpCode {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(OpCode::LoadConst),
            1 => Ok(OpCode::LoadNull),
            2 => Ok(OpCode::LoadTrue),
            3 => Ok(OpCode::LoadFalse),
            4 => Ok(OpCode::Add),
            5 => Ok(OpCode::Sub),
            6 => Ok(OpCode::Mul),
            7 => Ok(OpCode::Div),
            8 => Ok(OpCode::Mod),
            9 => Ok(OpCode::Neg),
            10 => Ok(OpCode::Eq),
            11 => Ok(OpCode::NotEq),
            12 => Ok(OpCode::Lt),
            13 => Ok(OpCode::Gt),
            14 => Ok(OpCode::LtEq),
            15 => Ok(OpCode::GtEq),
            16 => Ok(OpCode::And),
            17 => Ok(OpCode::Or),
            18 => Ok(OpCode::Not),
            19 => Ok(OpCode::Move),
            20 => Ok(OpCode::GetLocal),
            21 => Ok(OpCode::SetLocal),
            22 => Ok(OpCode::GetGlobal),
            23 => Ok(OpCode::SetGlobal),
            24 => Ok(OpCode::NewArray),
            25 => Ok(OpCode::NewObject),
            26 => Ok(OpCode::GetField),
            27 => Ok(OpCode::SetField),
            28 => Ok(OpCode::GetIndex),
            29 => Ok(OpCode::SetIndex),
            30 => Ok(OpCode::Jump),
            31 => Ok(OpCode::JumpIfFalse),
            32 => Ok(OpCode::JumpIfTrue),
            33 => Ok(OpCode::Loop),
            34 => Ok(OpCode::Call),
            35 => Ok(OpCode::Return),
            36 => Ok(OpCode::ReturnNull),
            37 => Ok(OpCode::Closure),
            38 => Ok(OpCode::Concat),
            39 => Ok(OpCode::Len),
            40 => Ok(OpCode::Try),
            41 => Ok(OpCode::Spawn),
            42 => Ok(OpCode::ExtractField),
            43 => Ok(OpCode::Interpolate),
            44 => Ok(OpCode::GetUpvalue),
            45 => Ok(OpCode::SetUpvalue),
            46 => Ok(OpCode::Pop),
            47 => Ok(OpCode::PushHandler),
            48 => Ok(OpCode::PopHandler),
            49 => Ok(OpCode::PushTimeout),
            50 => Ok(OpCode::PopTimeout),
            51 => Ok(OpCode::Await),
            52 => Ok(OpCode::Schedule),
            53 => Ok(OpCode::Watch),
            54 => Ok(OpCode::Must),
            55 => Ok(OpCode::Ask),
            56 => Ok(OpCode::Freeze),
            57 => Ok(OpCode::NewTuple),
            58 => Ok(OpCode::IterGet),
            59 => Ok(OpCode::SquadBegin),
            60 => Ok(OpCode::SquadEnd),
            _ => Err(value),
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpvalueSource {
    Local(u8),
    Upvalue(u8),
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
    pub upvalue_sources: Vec<UpvalueSource>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_from_valid_opcodes() {
        assert_eq!(OpCode::try_from(0u8), Ok(OpCode::LoadConst));
        assert_eq!(OpCode::try_from(56u8), Ok(OpCode::Freeze));
        assert_eq!(OpCode::try_from(57u8), Ok(OpCode::NewTuple));
        assert_eq!(OpCode::try_from(58u8), Ok(OpCode::IterGet));
    }

    #[test]
    fn try_from_invalid_opcode() {
        assert_eq!(OpCode::try_from(61u8), Err(61));
        assert_eq!(OpCode::try_from(255u8), Err(255));
    }
}
