use super::bytecode::{Chunk, Constant};
use std::io::{self, Read, Write};

const MAGIC: &[u8; 4] = b"FGC\0";
const VERSION_MAJOR: u8 = 1;
const VERSION_MINOR: u8 = 0;

#[derive(Debug)]
pub struct SerializeError {
    pub message: String,
}

impl SerializeError {
    fn new(msg: &str) -> Self {
        Self {
            message: msg.to_string(),
        }
    }
}

impl std::fmt::Display for SerializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<io::Error> for SerializeError {
    fn from(e: io::Error) -> Self {
        SerializeError::new(&format!("I/O error: {}", e))
    }
}

pub fn serialize_chunk(chunk: &Chunk) -> Result<Vec<u8>, SerializeError> {
    let mut buf = Vec::new();
    write_chunk(&mut buf, chunk)?;
    Ok(buf)
}

pub fn deserialize_chunk(data: &[u8]) -> Result<Chunk, SerializeError> {
    let mut cursor = io::Cursor::new(data);
    read_chunk_root(&mut cursor)
}

fn write_chunk(w: &mut Vec<u8>, chunk: &Chunk) -> Result<(), SerializeError> {
    w.write_all(MAGIC)?;
    w.push(VERSION_MAJOR);
    w.push(VERSION_MINOR);
    write_chunk_inner(w, chunk)
}

fn write_chunk_inner(w: &mut Vec<u8>, chunk: &Chunk) -> Result<(), SerializeError> {
    write_string(w, &chunk.name)?;
    w.push(chunk.arity);
    w.push(chunk.max_registers);
    w.push(chunk.upvalue_count);

    write_u32(w, chunk.constants.len() as u32)?;
    for constant in &chunk.constants {
        write_constant(w, constant)?;
    }

    write_u32(w, chunk.code.len() as u32)?;
    for &instruction in &chunk.code {
        write_u32(w, instruction)?;
    }

    write_u32(w, chunk.lines.len() as u32)?;
    for &line in &chunk.lines {
        write_u32(w, line as u32)?;
    }

    write_u16(w, chunk.prototypes.len() as u16)?;
    for proto in &chunk.prototypes {
        write_chunk_inner(w, proto)?;
    }

    Ok(())
}

fn write_constant(w: &mut Vec<u8>, constant: &Constant) -> Result<(), SerializeError> {
    match constant {
        Constant::Int(n) => {
            w.push(0x01);
            write_i64(w, *n)?;
        }
        Constant::Float(n) => {
            w.push(0x02);
            write_f64(w, *n)?;
        }
        Constant::Bool(b) => {
            w.push(0x03);
            w.push(if *b { 1 } else { 0 });
        }
        Constant::Null => {
            w.push(0x04);
        }
        Constant::Str(s) => {
            w.push(0x05);
            write_string(w, s)?;
        }
    }
    Ok(())
}

fn write_string(w: &mut Vec<u8>, s: &str) -> Result<(), SerializeError> {
    let bytes = s.as_bytes();
    if bytes.len() > u32::MAX as usize {
        return Err(SerializeError::new("string too long to serialize"));
    }
    write_u32(w, bytes.len() as u32)?;
    w.write_all(bytes)?;
    Ok(())
}

fn write_u16(w: &mut Vec<u8>, n: u16) -> Result<(), SerializeError> {
    w.write_all(&n.to_le_bytes())?;
    Ok(())
}

fn write_u32(w: &mut Vec<u8>, n: u32) -> Result<(), SerializeError> {
    w.write_all(&n.to_le_bytes())?;
    Ok(())
}

fn write_i64(w: &mut Vec<u8>, n: i64) -> Result<(), SerializeError> {
    w.write_all(&n.to_le_bytes())?;
    Ok(())
}

fn write_f64(w: &mut Vec<u8>, n: f64) -> Result<(), SerializeError> {
    w.write_all(&n.to_bits().to_le_bytes())?;
    Ok(())
}

fn read_chunk_root<R: Read>(r: &mut R) -> Result<Chunk, SerializeError> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(SerializeError::new(
            "not a valid Forge bytecode file (bad magic bytes)",
        ));
    }

    let mut version = [0u8; 2];
    r.read_exact(&mut version)?;
    if version[0] > VERSION_MAJOR {
        return Err(SerializeError::new(&format!(
            "bytecode version {}.{} is newer than supported {}.{}",
            version[0], version[1], VERSION_MAJOR, VERSION_MINOR
        )));
    }

    read_chunk_inner(r)
}

fn read_chunk_inner<R: Read>(r: &mut R) -> Result<Chunk, SerializeError> {
    let name = read_string(r)?;

    let mut meta = [0u8; 3];
    r.read_exact(&mut meta)?;
    let arity = meta[0];
    let max_registers = meta[1];
    let upvalue_count = meta[2];

    let const_count = read_u32(r)? as usize;
    if const_count > 65536 {
        return Err(SerializeError::new("constant pool too large"));
    }
    let mut constants = Vec::with_capacity(const_count);
    for _ in 0..const_count {
        constants.push(read_constant(r)?);
    }

    let code_count = read_u32(r)? as usize;
    if code_count > 1_000_000 {
        return Err(SerializeError::new("code section too large"));
    }
    let mut code = Vec::with_capacity(code_count);
    for _ in 0..code_count {
        code.push(read_u32(r)?);
    }

    let lines_count = read_u32(r)? as usize;
    if lines_count > 1_000_000 {
        return Err(SerializeError::new("line table too large"));
    }
    let mut lines = Vec::with_capacity(lines_count);
    for _ in 0..lines_count {
        lines.push(read_u32(r)? as usize);
    }

    let proto_count = read_u16(r)? as usize;
    if proto_count > 65536 {
        return Err(SerializeError::new("too many prototypes"));
    }
    let mut prototypes = Vec::with_capacity(proto_count);
    for _ in 0..proto_count {
        prototypes.push(read_chunk_inner(r)?);
    }

    Ok(Chunk {
        code,
        constants,
        lines,
        name,
        prototypes,
        max_registers,
        upvalue_count,
        arity,
    })
}

fn read_constant<R: Read>(r: &mut R) -> Result<Constant, SerializeError> {
    let mut tag = [0u8; 1];
    r.read_exact(&mut tag)?;
    match tag[0] {
        0x01 => Ok(Constant::Int(read_i64(r)?)),
        0x02 => Ok(Constant::Float(read_f64(r)?)),
        0x03 => {
            let mut b = [0u8; 1];
            r.read_exact(&mut b)?;
            Ok(Constant::Bool(b[0] != 0))
        }
        0x04 => Ok(Constant::Null),
        0x05 => Ok(Constant::Str(read_string(r)?)),
        other => Err(SerializeError::new(&format!(
            "unknown constant tag: 0x{:02x}",
            other
        ))),
    }
}

fn read_string<R: Read>(r: &mut R) -> Result<String, SerializeError> {
    let len = read_u32(r)? as usize;
    if len > 10_000_000 {
        return Err(SerializeError::new("string too long"));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|_| SerializeError::new("invalid UTF-8 in string constant"))
}

fn read_u16<R: Read>(r: &mut R) -> Result<u16, SerializeError> {
    let mut buf = [0u8; 2];
    r.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
}

fn read_u32<R: Read>(r: &mut R) -> Result<u32, SerializeError> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_i64<R: Read>(r: &mut R) -> Result<i64, SerializeError> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(i64::from_le_bytes(buf))
}

fn read_f64<R: Read>(r: &mut R) -> Result<f64, SerializeError> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(f64::from_bits(u64::from_le_bytes(buf)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::bytecode::*;

    fn make_simple_chunk() -> Chunk {
        let mut chunk = Chunk::new("<test>");
        chunk.arity = 0;
        chunk.max_registers = 4;
        chunk.upvalue_count = 0;

        chunk.add_constant(Constant::Int(42));
        chunk.add_constant(Constant::Float(3.14));
        chunk.add_constant(Constant::Bool(true));
        chunk.add_constant(Constant::Null);
        chunk.add_constant(Constant::Str("hello".to_string()));

        chunk.emit(encode_abx(OpCode::LoadConst, 0, 0), 1);
        chunk.emit(encode_abc(OpCode::Return, 0, 0, 0), 2);

        chunk
    }

    #[test]
    fn round_trip_simple() {
        let original = make_simple_chunk();
        let bytes = serialize_chunk(&original).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        assert_eq!(original.name, restored.name);
        assert_eq!(original.arity, restored.arity);
        assert_eq!(original.max_registers, restored.max_registers);
        assert_eq!(original.upvalue_count, restored.upvalue_count);
        assert_eq!(original.code, restored.code);
        assert_eq!(original.lines, restored.lines);
        assert_eq!(original.constants.len(), restored.constants.len());
        assert_eq!(original.prototypes.len(), restored.prototypes.len());
    }

    #[test]
    fn round_trip_constants() {
        let original = make_simple_chunk();
        let bytes = serialize_chunk(&original).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        for (orig, rest) in original.constants.iter().zip(restored.constants.iter()) {
            assert!(
                orig.identical(rest),
                "constant mismatch: {:?} vs {:?}",
                orig,
                rest
            );
        }
    }

    #[test]
    fn round_trip_with_prototypes() {
        let mut main_chunk = Chunk::new("<main>");
        main_chunk.max_registers = 2;

        let mut fn_chunk = Chunk::new("add");
        fn_chunk.arity = 2;
        fn_chunk.max_registers = 3;
        fn_chunk.add_constant(Constant::Int(1));
        fn_chunk.emit(encode_abc(OpCode::Add, 2, 0, 1), 1);
        fn_chunk.emit(encode_abc(OpCode::Return, 2, 0, 0), 2);

        main_chunk.prototypes.push(fn_chunk);
        main_chunk.emit(encode_abx(OpCode::Closure, 0, 0), 1);
        main_chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 2);

        let bytes = serialize_chunk(&main_chunk).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        assert_eq!(restored.prototypes.len(), 1);
        let proto = &restored.prototypes[0];
        assert_eq!(proto.name, "add");
        assert_eq!(proto.arity, 2);
        assert_eq!(proto.max_registers, 3);
        assert_eq!(proto.code.len(), 2);
        assert_eq!(proto.constants.len(), 1);
    }

    #[test]
    fn round_trip_nested_prototypes() {
        let mut inner = Chunk::new("inner");
        inner.arity = 1;
        inner.max_registers = 2;
        inner.emit(encode_abc(OpCode::Return, 0, 0, 0), 1);

        let mut outer = Chunk::new("outer");
        outer.arity = 0;
        outer.max_registers = 3;
        outer.prototypes.push(inner);
        outer.emit(encode_abx(OpCode::Closure, 0, 0), 1);
        outer.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 2);

        let mut main_chunk = Chunk::new("<main>");
        main_chunk.max_registers = 2;
        main_chunk.prototypes.push(outer);
        main_chunk.emit(encode_abx(OpCode::Closure, 0, 0), 1);
        main_chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 2);

        let bytes = serialize_chunk(&main_chunk).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        assert_eq!(restored.prototypes.len(), 1);
        assert_eq!(restored.prototypes[0].name, "outer");
        assert_eq!(restored.prototypes[0].prototypes.len(), 1);
        assert_eq!(restored.prototypes[0].prototypes[0].name, "inner");
        assert_eq!(restored.prototypes[0].prototypes[0].arity, 1);
    }

    #[test]
    fn round_trip_empty_chunk() {
        let chunk = Chunk::new("<empty>");
        let bytes = serialize_chunk(&chunk).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        assert_eq!(restored.name, "<empty>");
        assert_eq!(restored.code.len(), 0);
        assert_eq!(restored.constants.len(), 0);
        assert_eq!(restored.prototypes.len(), 0);
    }

    #[test]
    fn round_trip_string_constants() {
        let mut chunk = Chunk::new("<strings>");
        chunk.max_registers = 1;
        chunk.add_constant(Constant::Str("".to_string()));
        chunk.add_constant(Constant::Str("hello world".to_string()));
        chunk.add_constant(Constant::Str("unicode: \u{1F525}\u{2764}".to_string()));
        chunk.add_constant(Constant::Str("newlines\n\ttabs".to_string()));
        chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 1);

        let bytes = serialize_chunk(&chunk).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        assert_eq!(restored.constants.len(), 4);
        match &restored.constants[0] {
            Constant::Str(s) => assert_eq!(s, ""),
            other => panic!("expected Str, got {:?}", other),
        }
        match &restored.constants[1] {
            Constant::Str(s) => assert_eq!(s, "hello world"),
            other => panic!("expected Str, got {:?}", other),
        }
        match &restored.constants[2] {
            Constant::Str(s) => assert_eq!(s, "unicode: \u{1F525}\u{2764}"),
            other => panic!("expected Str, got {:?}", other),
        }
        match &restored.constants[3] {
            Constant::Str(s) => assert_eq!(s, "newlines\n\ttabs"),
            other => panic!("expected Str, got {:?}", other),
        }
    }

    #[test]
    fn round_trip_edge_case_numbers() {
        let mut chunk = Chunk::new("<numbers>");
        chunk.max_registers = 1;
        chunk.add_constant(Constant::Int(0));
        chunk.add_constant(Constant::Int(-1));
        chunk.add_constant(Constant::Int(i64::MAX));
        chunk.add_constant(Constant::Int(i64::MIN));
        chunk.add_constant(Constant::Float(0.0));
        // -0.0 == 0.0 in IEEE 754, so add_constant deduplicates them
        chunk.add_constant(Constant::Float(f64::INFINITY));
        chunk.add_constant(Constant::Float(f64::NEG_INFINITY));
        chunk.add_constant(Constant::Float(f64::MIN));
        chunk.add_constant(Constant::Float(f64::MAX));
        chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 1);

        let bytes = serialize_chunk(&chunk).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        assert_eq!(restored.constants.len(), 9);
        match &restored.constants[2] {
            Constant::Int(n) => assert_eq!(*n, i64::MAX),
            other => panic!("expected Int, got {:?}", other),
        }
        match &restored.constants[3] {
            Constant::Int(n) => assert_eq!(*n, i64::MIN),
            other => panic!("expected Int, got {:?}", other),
        }
        match &restored.constants[5] {
            Constant::Float(n) => assert!(n.is_infinite() && n.is_sign_positive()),
            other => panic!("expected +Inf, got {:?}", other),
        }
    }

    #[test]
    fn round_trip_nan_constant() {
        let mut chunk = Chunk::new("<nan>");
        chunk.max_registers = 1;
        chunk.add_constant(Constant::Float(f64::NAN));
        chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 1);

        let bytes = serialize_chunk(&chunk).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        match &restored.constants[0] {
            Constant::Float(n) => assert!(n.is_nan()),
            other => panic!("expected NaN, got {:?}", other),
        }
    }

    #[test]
    fn bad_magic_rejected() {
        let data = b"BADM\x01\x00";
        let result = deserialize_chunk(data);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("bad magic bytes"));
    }

    #[test]
    fn future_version_rejected() {
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC);
        data.push(99); // future major version
        data.push(0);
        let result = deserialize_chunk(&data);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("newer than supported"));
    }

    #[test]
    fn truncated_data_rejected() {
        let chunk = make_simple_chunk();
        let bytes = serialize_chunk(&chunk).unwrap();
        let truncated = &bytes[..bytes.len() / 2];
        let result = deserialize_chunk(truncated);
        assert!(result.is_err());
    }

    #[test]
    fn magic_bytes_correct() {
        let chunk = Chunk::new("<test>");
        let bytes = serialize_chunk(&chunk).unwrap();
        assert_eq!(&bytes[0..4], b"FGC\0");
        assert_eq!(bytes[4], VERSION_MAJOR);
        assert_eq!(bytes[5], VERSION_MINOR);
    }

    #[test]
    fn round_trip_all_instruction_opcodes() {
        let mut chunk = Chunk::new("<opcodes>");
        chunk.max_registers = 10;
        chunk.add_constant(Constant::Int(1));
        chunk.add_constant(Constant::Str("x".to_string()));

        chunk.emit(encode_abx(OpCode::LoadConst, 0, 0), 1);
        chunk.emit(encode_abc(OpCode::LoadNull, 1, 0, 0), 2);
        chunk.emit(encode_abc(OpCode::LoadTrue, 2, 0, 0), 3);
        chunk.emit(encode_abc(OpCode::LoadFalse, 3, 0, 0), 4);
        chunk.emit(encode_abc(OpCode::Add, 4, 0, 1), 5);
        chunk.emit(encode_abc(OpCode::Sub, 4, 0, 1), 6);
        chunk.emit(encode_abc(OpCode::Mul, 4, 0, 1), 7);
        chunk.emit(encode_abc(OpCode::Div, 4, 0, 1), 8);
        chunk.emit(encode_abc(OpCode::Mod, 4, 0, 1), 9);
        chunk.emit(encode_abc(OpCode::Neg, 5, 0, 0), 10);
        chunk.emit(encode_abc(OpCode::Eq, 6, 0, 1), 11);
        chunk.emit(encode_abc(OpCode::Move, 7, 0, 0), 12);
        chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 13);

        let bytes = serialize_chunk(&chunk).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        assert_eq!(original_code(&chunk), original_code(&restored));
    }

    fn original_code(chunk: &Chunk) -> Vec<u32> {
        chunk.code.clone()
    }

    #[test]
    fn round_trip_compiled_program() {
        use crate::lexer::Lexer;
        use crate::parser::Parser;
        use crate::vm::compiler;

        let source = r#"
let x = 42
let y = x + 8
println(y)

fn add(a, b) {
    return a + b
}

let result = add(10, 20)
println(result)
"#;

        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let chunk = compiler::compile(&program).unwrap();

        let bytes = serialize_chunk(&chunk).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        assert_eq!(chunk.code, restored.code);
        assert_eq!(chunk.lines, restored.lines);
        assert_eq!(chunk.name, restored.name);
        assert_eq!(chunk.arity, restored.arity);
        assert_eq!(chunk.max_registers, restored.max_registers);
        assert_eq!(chunk.prototypes.len(), restored.prototypes.len());

        for (orig, rest) in chunk.constants.iter().zip(restored.constants.iter()) {
            assert!(orig.identical(rest));
        }

        for (orig, rest) in chunk.prototypes.iter().zip(restored.prototypes.iter()) {
            assert_eq!(orig.code, rest.code);
            assert_eq!(orig.name, rest.name);
            assert_eq!(orig.arity, rest.arity);
            for (oc, rc) in orig.constants.iter().zip(rest.constants.iter()) {
                assert!(oc.identical(rc));
            }
        }
    }

    #[test]
    fn round_trip_control_flow_program() {
        use crate::lexer::Lexer;
        use crate::parser::Parser;
        use crate::vm::compiler;

        let source = r#"
fn fib(n) {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

let result = fib(10)
println(result)
"#;

        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let chunk = compiler::compile(&program).unwrap();

        let bytes = serialize_chunk(&chunk).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        assert_eq!(chunk.code, restored.code);
        assert_eq!(chunk.prototypes.len(), restored.prototypes.len());

        let orig_fib = &chunk.prototypes[0];
        let rest_fib = &restored.prototypes[0];
        assert_eq!(orig_fib.code, rest_fib.code);
        assert_eq!(orig_fib.name, rest_fib.name);
        assert_eq!(orig_fib.arity, rest_fib.arity);
    }

    #[test]
    fn round_trip_loop_program() {
        use crate::lexer::Lexer;
        use crate::parser::Parser;
        use crate::vm::compiler;

        let source = r#"
let mut sum = 0
let items = [1, 2, 3, 4, 5]
for item in items {
    sum = sum + item
}
println(sum)
"#;

        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let chunk = compiler::compile(&program).unwrap();

        let bytes = serialize_chunk(&chunk).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        assert_eq!(chunk.code, restored.code);
        assert_eq!(chunk.lines, restored.lines);
    }

    #[test]
    fn serialized_size_reasonable() {
        let chunk = make_simple_chunk();
        let bytes = serialize_chunk(&chunk).unwrap();
        assert!(
            bytes.len() < 200,
            "simple chunk serialized to {} bytes",
            bytes.len()
        );
        assert!(
            bytes.len() > 20,
            "simple chunk too small: {} bytes",
            bytes.len()
        );
    }

    #[test]
    fn round_trip_empty_string_constant() {
        let mut chunk = Chunk::new("");
        chunk.add_constant(Constant::Str(String::new()));
        chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 1);

        let bytes = serialize_chunk(&chunk).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        assert_eq!(restored.name, "");
        match &restored.constants[0] {
            Constant::Str(s) => assert_eq!(s, ""),
            other => panic!("expected empty Str, got {:?}", other),
        }
    }
}
