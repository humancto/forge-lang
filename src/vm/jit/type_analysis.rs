use crate::vm::bytecode::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegType {
    Int,
    Float,
    Bool,
    /// A GcRef index pointing to a string in the GC heap.
    StringRef,
    /// A GcRef index pointing to an array or object in the GC heap.
    ObjRef,
    Unknown,
}

#[allow(dead_code)]
impl RegType {
    pub fn is_numeric(&self) -> bool {
        matches!(self, RegType::Int | RegType::Float)
    }
}

#[allow(dead_code)]
pub struct TypeInfo {
    pub reg_types: Vec<RegType>,
    pub has_unsupported_ops: bool,
    pub has_float: bool,
    pub has_string_ops: bool,
    /// True when the function uses array/object/interpolate/extract opcodes.
    pub has_collection_ops: bool,
    /// The type of the value returned by the function (from the last Return opcode).
    pub return_type: RegType,
}

#[derive(Clone, Copy)]
enum ConstValue {
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl ConstValue {
    fn is_zero(self) -> bool {
        match self {
            ConstValue::Int(n) => n == 0,
            ConstValue::Float(n) => n == 0.0,
            ConstValue::Bool(false) => true,
            ConstValue::Bool(true) => false,
        }
    }
}

/// Pre-pass: analyze bytecode to determine register types.
/// Returns None if the function uses unsupported operations.
pub fn analyze(chunk: &Chunk) -> TypeInfo {
    let num_regs = chunk.max_registers.max(chunk.arity).max(1) as usize + 1;
    let mut types = vec![RegType::Unknown; num_regs];
    let mut constants = vec![None; num_regs];
    let mut has_unsupported = false;
    let mut has_float = false;
    let mut has_string_ops = false;
    let mut has_collection_ops = false;
    let mut return_type = RegType::Int;

    for i in 0..chunk.arity as usize {
        types[i] = RegType::Int;
    }

    for &inst in &chunk.code {
        let op = decode_op(inst);
        let a = decode_a(inst) as usize;
        let bb = decode_b(inst) as usize;
        let cc = decode_c(inst) as usize;
        let bx = decode_bx(inst);
        let Ok(opcode) = OpCode::try_from(op) else {
            continue;
        };

        match opcode {
            OpCode::LoadConst => {
                if (bx as usize) < chunk.constants.len() {
                    match &chunk.constants[bx as usize] {
                        Constant::Int(_) => {
                            if a < types.len() {
                                types[a] = RegType::Int;
                                constants[a] = match &chunk.constants[bx as usize] {
                                    Constant::Int(n) => Some(ConstValue::Int(*n)),
                                    _ => None,
                                };
                            }
                        }
                        Constant::Float(_) => {
                            if a < types.len() {
                                types[a] = RegType::Float;
                                has_float = true;
                                constants[a] = match &chunk.constants[bx as usize] {
                                    Constant::Float(n) => Some(ConstValue::Float(*n)),
                                    _ => None,
                                };
                            }
                        }
                        Constant::Bool(_) => {
                            if a < types.len() {
                                types[a] = RegType::Bool;
                                constants[a] = match &chunk.constants[bx as usize] {
                                    Constant::Bool(v) => Some(ConstValue::Bool(*v)),
                                    _ => None,
                                };
                            }
                        }
                        Constant::Str(_) => {
                            has_string_ops = true;
                            if a < types.len() {
                                types[a] = RegType::StringRef;
                                constants[a] = None;
                            }
                        }
                        Constant::Null => {
                            if a < types.len() {
                                types[a] = RegType::Int;
                                constants[a] = Some(ConstValue::Int(0));
                            }
                        }
                    }
                }
            }
            OpCode::LoadNull => {
                if a < types.len() {
                    types[a] = RegType::Int;
                    constants[a] = Some(ConstValue::Int(0));
                }
            }
            OpCode::LoadTrue | OpCode::LoadFalse => {
                if a < types.len() {
                    types[a] = RegType::Bool;
                    constants[a] = Some(ConstValue::Bool(matches!(opcode, OpCode::LoadTrue)));
                }
            }
            OpCode::Add | OpCode::Sub | OpCode::Mul => {
                if a < types.len() && bb < types.len() && cc < types.len() {
                    if types[bb] == RegType::Float || types[cc] == RegType::Float {
                        types[a] = RegType::Float;
                        has_float = true;
                    } else {
                        types[a] = RegType::Int;
                    }
                    constants[a] = None;
                }
            }
            OpCode::Div | OpCode::Mod => {
                if a < types.len() && bb < types.len() && cc < types.len() {
                    if constants[cc].is_some_and(ConstValue::is_zero) {
                        has_unsupported = true;
                    }
                    if types[bb] == RegType::Float || types[cc] == RegType::Float {
                        types[a] = RegType::Float;
                        has_float = true;
                    } else {
                        types[a] = RegType::Int;
                    }
                    constants[a] = None;
                }
            }
            OpCode::Neg => {
                if a < types.len() && bb < types.len() {
                    types[a] = types[bb];
                    constants[a] = None;
                }
            }
            OpCode::Eq
            | OpCode::NotEq
            | OpCode::Lt
            | OpCode::Gt
            | OpCode::LtEq
            | OpCode::GtEq
            | OpCode::Not
            | OpCode::And
            | OpCode::Or => {
                if a < types.len() {
                    types[a] = RegType::Bool;
                    constants[a] = None;
                }
            }
            OpCode::Move | OpCode::GetLocal | OpCode::SetLocal => {
                if a < types.len() && bb < types.len() {
                    types[a] = types[bb];
                    constants[a] = constants[bb];
                }
            }
            OpCode::Jump | OpCode::JumpIfFalse | OpCode::JumpIfTrue | OpCode::Loop => {}
            OpCode::Call => {
                let dst = cc;
                if dst < types.len() {
                    types[dst] = RegType::Int;
                    constants[dst] = None;
                }
            }
            OpCode::Return => {
                if a < types.len() {
                    return_type = types[a];
                }
            }
            OpCode::ReturnNull => {}

            OpCode::Concat => {
                has_string_ops = true;
                if a < types.len() {
                    types[a] = RegType::StringRef;
                    constants[a] = None;
                }
            }
            OpCode::Len => {
                has_string_ops = true;
                if a < types.len() {
                    types[a] = RegType::Int;
                    constants[a] = None;
                }
            }

            OpCode::NewArray => {
                has_collection_ops = true;
                if a < types.len() {
                    types[a] = RegType::ObjRef;
                    constants[a] = None;
                }
            }
            OpCode::NewObject => {
                has_collection_ops = true;
                if a < types.len() {
                    types[a] = RegType::ObjRef;
                    constants[a] = None;
                }
            }
            OpCode::GetField | OpCode::GetIndex | OpCode::ExtractField => {
                has_collection_ops = true;
                if a < types.len() {
                    types[a] = RegType::Unknown;
                    constants[a] = None;
                }
            }
            OpCode::SetField | OpCode::SetIndex => {
                has_collection_ops = true;
            }
            OpCode::Interpolate => {
                has_collection_ops = true;
                has_string_ops = true;
                if a < types.len() {
                    types[a] = RegType::StringRef;
                    constants[a] = None;
                }
            }

            OpCode::Spawn
            | OpCode::Try
            | OpCode::PushHandler
            | OpCode::PopHandler
            | OpCode::PushTimeout
            | OpCode::PopTimeout => {
                has_unsupported = true;
                if a < constants.len() {
                    constants[a] = None;
                }
            }

            OpCode::Closure
            | OpCode::GetGlobal
            | OpCode::SetGlobal
            | OpCode::GetUpvalue
            | OpCode::SetUpvalue => {
                has_unsupported = true;
                if a < constants.len() {
                    constants[a] = None;
                }
            }

            _ => {}
        }
    }

    // Functions mixing strings/collections with floats are unsupported
    // (would need per-register Cranelift types)
    if (has_string_ops || has_collection_ops) && has_float {
        has_unsupported = true;
    }

    TypeInfo {
        reg_types: types,
        has_unsupported_ops: has_unsupported,
        has_float,
        has_string_ops,
        has_collection_ops,
        return_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_integer_function() {
        let mut chunk = Chunk::new("add");
        chunk.arity = 2;
        chunk.max_registers = 3;
        chunk.add_constant(Constant::Int(1));
        chunk.emit(encode_abc(OpCode::Add, 2, 0, 1), 1);
        chunk.emit(encode_abc(OpCode::Return, 2, 0, 0), 2);

        let info = analyze(&chunk);
        assert!(!info.has_unsupported_ops);
        assert!(!info.has_float);
        assert_eq!(info.reg_types[0], RegType::Int);
        assert_eq!(info.reg_types[1], RegType::Int);
        assert_eq!(info.reg_types[2], RegType::Int);
    }

    #[test]
    fn analyze_float_function() {
        let mut chunk = Chunk::new("area");
        chunk.arity = 1;
        chunk.max_registers = 3;
        let pi_idx = chunk.add_constant(Constant::Float(3.14159));
        chunk.emit(encode_abx(OpCode::LoadConst, 1, pi_idx), 1);
        chunk.emit(encode_abc(OpCode::Mul, 2, 0, 1), 2);
        chunk.emit(encode_abc(OpCode::Return, 2, 0, 0), 3);

        let info = analyze(&chunk);
        assert!(!info.has_unsupported_ops);
        assert!(info.has_float);
        assert_eq!(info.reg_types[1], RegType::Float);
        assert_eq!(info.reg_types[2], RegType::Float);
    }

    #[test]
    fn analyze_string_function_supported() {
        let mut chunk = Chunk::new("greet");
        chunk.arity = 0;
        chunk.max_registers = 2;
        let str_idx = chunk.add_constant(Constant::Str("hello".to_string()));
        chunk.emit(encode_abx(OpCode::LoadConst, 0, str_idx), 1);
        chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 2);

        let info = analyze(&chunk);
        assert!(!info.has_unsupported_ops);
        assert!(info.has_string_ops);
        assert_eq!(info.reg_types[0], RegType::StringRef);
    }

    #[test]
    fn analyze_string_with_float_unsupported() {
        let mut chunk = Chunk::new("mixed");
        chunk.arity = 0;
        chunk.max_registers = 3;
        let str_idx = chunk.add_constant(Constant::Str("hello".to_string()));
        let float_idx = chunk.add_constant(Constant::Float(1.5));
        chunk.emit(encode_abx(OpCode::LoadConst, 0, str_idx), 1);
        chunk.emit(encode_abx(OpCode::LoadConst, 1, float_idx), 2);
        chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 3);

        let info = analyze(&chunk);
        assert!(
            info.has_unsupported_ops,
            "string+float mix should be unsupported"
        );
    }

    #[test]
    fn analyze_concat_produces_string_ref() {
        let mut chunk = Chunk::new("concat");
        chunk.arity = 0;
        chunk.max_registers = 3;
        let s1 = chunk.add_constant(Constant::Str("hello ".to_string()));
        let s2 = chunk.add_constant(Constant::Str("world".to_string()));
        chunk.emit(encode_abx(OpCode::LoadConst, 0, s1), 1);
        chunk.emit(encode_abx(OpCode::LoadConst, 1, s2), 2);
        chunk.emit(encode_abc(OpCode::Concat, 2, 0, 1), 3);
        chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 4);

        let info = analyze(&chunk);
        assert!(!info.has_unsupported_ops);
        assert!(info.has_string_ops);
        assert_eq!(info.reg_types[2], RegType::StringRef);
    }

    #[test]
    fn analyze_len_produces_int() {
        let mut chunk = Chunk::new("strlen");
        chunk.arity = 0;
        chunk.max_registers = 2;
        let s = chunk.add_constant(Constant::Str("hello".to_string()));
        chunk.emit(encode_abx(OpCode::LoadConst, 0, s), 1);
        chunk.emit(encode_abc(OpCode::Len, 1, 0, 0), 2);
        chunk.emit(encode_abc(OpCode::Return, 1, 0, 0), 3);

        let info = analyze(&chunk);
        assert!(!info.has_unsupported_ops);
        assert!(info.has_string_ops);
        assert_eq!(info.reg_types[1], RegType::Int);
    }

    #[test]
    fn analyze_array_supported() {
        let mut chunk = Chunk::new("make_arr");
        chunk.arity = 0;
        chunk.max_registers = 4;
        let one = chunk.add_constant(Constant::Int(1));
        let two = chunk.add_constant(Constant::Int(2));
        chunk.emit(encode_abx(OpCode::LoadConst, 1, one), 1);
        chunk.emit(encode_abx(OpCode::LoadConst, 2, two), 2);
        chunk.emit(encode_abc(OpCode::NewArray, 0, 1, 2), 3);
        chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 4);

        let info = analyze(&chunk);
        assert!(!info.has_unsupported_ops);
        assert!(info.has_collection_ops);
        assert_eq!(info.reg_types[0], RegType::ObjRef);
    }

    #[test]
    fn analyze_object_supported() {
        let mut chunk = Chunk::new("make_obj");
        chunk.arity = 0;
        chunk.max_registers = 4;
        chunk.emit(encode_abc(OpCode::NewObject, 0, 1, 1), 1);
        chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 2);

        let info = analyze(&chunk);
        assert!(!info.has_unsupported_ops);
        assert!(info.has_collection_ops);
        assert_eq!(info.reg_types[0], RegType::ObjRef);
    }

    #[test]
    fn analyze_get_field_produces_unknown() {
        let mut chunk = Chunk::new("get_field");
        chunk.arity = 0;
        chunk.max_registers = 3;
        chunk.emit(encode_abc(OpCode::NewObject, 0, 1, 0), 1);
        chunk.emit(encode_abc(OpCode::GetField, 1, 0, 0), 2);
        chunk.emit(encode_abc(OpCode::Return, 1, 0, 0), 3);

        let info = analyze(&chunk);
        assert!(!info.has_unsupported_ops);
        assert!(info.has_collection_ops);
        assert_eq!(info.reg_types[1], RegType::Unknown);
    }

    #[test]
    fn analyze_collection_with_float_unsupported() {
        let mut chunk = Chunk::new("mixed");
        chunk.arity = 0;
        chunk.max_registers = 3;
        let f = chunk.add_constant(Constant::Float(1.5));
        chunk.emit(encode_abx(OpCode::LoadConst, 0, f), 1);
        chunk.emit(encode_abc(OpCode::NewArray, 1, 0, 1), 2);
        chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 3);

        let info = analyze(&chunk);
        assert!(
            info.has_unsupported_ops,
            "collection+float mix should be unsupported"
        );
    }

    #[test]
    fn analyze_interpolate_produces_string_ref() {
        let mut chunk = Chunk::new("interp");
        chunk.arity = 0;
        chunk.max_registers = 4;
        let s = chunk.add_constant(Constant::Str("hello ".to_string()));
        let n = chunk.add_constant(Constant::Int(42));
        chunk.emit(encode_abx(OpCode::LoadConst, 1, s), 1);
        chunk.emit(encode_abx(OpCode::LoadConst, 2, n), 2);
        chunk.emit(encode_abc(OpCode::Interpolate, 0, 1, 2), 3);
        chunk.emit(encode_abc(OpCode::Return, 0, 0, 0), 4);

        let info = analyze(&chunk);
        assert!(!info.has_unsupported_ops);
        assert!(info.has_collection_ops);
        assert!(info.has_string_ops);
        assert_eq!(info.reg_types[0], RegType::StringRef);
    }

    #[test]
    fn analyze_mixed_int_float() {
        let mut chunk = Chunk::new("mixed");
        chunk.arity = 1;
        chunk.max_registers = 3;
        let float_idx = chunk.add_constant(Constant::Float(2.5));
        chunk.emit(encode_abx(OpCode::LoadConst, 1, float_idx), 1);
        chunk.emit(encode_abc(OpCode::Add, 2, 0, 1), 2);
        chunk.emit(encode_abc(OpCode::Return, 2, 0, 0), 3);

        let info = analyze(&chunk);
        assert!(info.has_float);
        assert_eq!(info.reg_types[2], RegType::Float);
    }

    #[test]
    fn analyze_comparison_produces_bool() {
        let mut chunk = Chunk::new("cmp");
        chunk.arity = 2;
        chunk.max_registers = 3;
        chunk.emit(encode_abc(OpCode::Lt, 2, 0, 1), 1);
        chunk.emit(encode_abc(OpCode::Return, 2, 0, 0), 2);

        let info = analyze(&chunk);
        assert_eq!(info.reg_types[2], RegType::Bool);
    }

    #[test]
    fn analyze_division_by_zero_constant_is_unsupported() {
        let mut chunk = Chunk::new("boom");
        chunk.arity = 0;
        chunk.max_registers = 3;
        let one_idx = chunk.add_constant(Constant::Int(1));
        let zero_idx = chunk.add_constant(Constant::Int(0));
        chunk.emit(encode_abx(OpCode::LoadConst, 0, one_idx), 1);
        chunk.emit(encode_abx(OpCode::LoadConst, 1, zero_idx), 2);
        chunk.emit(encode_abc(OpCode::Div, 2, 0, 1), 3);
        chunk.emit(encode_abc(OpCode::Return, 2, 0, 0), 4);

        let info = analyze(&chunk);
        assert!(info.has_unsupported_ops);
    }
}
