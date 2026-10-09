use super::bytecode::{Chunk, Constant, UpvalueSource};
use super::verify::{MAX_CODE_LEN, MAX_PROTO_DEPTH};
use std::io::{self, Write};

const MAGIC: &[u8; 4] = b"FGC\0";
const VERSION_MAJOR: u8 = 1;
const VERSION_MINOR: u8 = 3;

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

/// Decode serialized bytecode and verify it (see [`super::verify`]) before
/// it can reach the VM. This is the only way bytecode from outside the
/// process (`forge run app.fgc`, AOT binaries) enters the VM.
///
/// Hostile input never panics and never allocates more than a small
/// multiple of its own size: every length prefix is checked against the
/// bytes that remain before anything is allocated, and prototype nesting
/// is bounded by [`MAX_PROTO_DEPTH`].
pub fn deserialize_chunk(data: &[u8]) -> Result<Chunk, SerializeError> {
    let chunk = decode_chunk(data)?;
    super::verify::verify_chunk(&chunk).map_err(|e| SerializeError {
        message: e.to_string(),
    })?;
    Ok(chunk)
}

/// Decode without verifying. Only for tests that inspect the raw decoding.
#[cfg(test)]
pub(crate) fn decode_chunk_unverified(data: &[u8]) -> Result<Chunk, SerializeError> {
    decode_chunk(data)
}

fn decode_chunk(data: &[u8]) -> Result<Chunk, SerializeError> {
    let mut r = Reader { data, pos: 0 };
    let chunk = read_chunk_root(&mut r)?;
    if r.remaining() != 0 {
        return Err(SerializeError::new(&format!(
            "{} trailing bytes after the bytecode",
            r.remaining()
        )));
    }
    Ok(chunk)
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
    // v1.3+: required argument count (default parameters).
    w.push(chunk.min_arity);

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

    write_u32(w, chunk.cols.len() as u32)?;
    for &col in &chunk.cols {
        write_u32(w, col as u32)?;
    }

    write_u16(w, chunk.prototypes.len() as u16)?;
    for proto in &chunk.prototypes {
        write_chunk_inner(w, proto)?;
    }

    write_u16(w, chunk.upvalue_sources.len() as u16)?;
    for &src in &chunk.upvalue_sources {
        match src {
            UpvalueSource::Local(reg) => {
                w.push(0x01);
                w.push(reg);
            }
            UpvalueSource::Upvalue(idx) => {
                w.push(0x02);
                w.push(idx);
            }
        }
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

/// Bounds-checked cursor over the input.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8], SerializeError> {
        if n > self.remaining() {
            return Err(SerializeError::new(&format!(
                "unexpected end of bytecode at offset {} (needed {} more bytes, {} left)",
                self.pos,
                n,
                self.remaining()
            )));
        }
        let out = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], SerializeError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.bytes(N)?);
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, SerializeError> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, SerializeError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, SerializeError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn i64(&mut self) -> Result<i64, SerializeError> {
        Ok(i64::from_le_bytes(self.array()?))
    }

    fn f64(&mut self) -> Result<f64, SerializeError> {
        Ok(f64::from_bits(u64::from_le_bytes(self.array()?)))
    }

    /// Validate a section's element count: at most `max`, and no more than
    /// the remaining input could hold at `min_element_bytes` each — so the
    /// caller may allocate `count` elements without trusting the prefix.
    /// A u32 length prefix, validated by [`Reader::count`].
    fn len_u32(
        &mut self,
        min_element_bytes: usize,
        max: usize,
        what: &str,
    ) -> Result<usize, SerializeError> {
        let raw = self.u32()? as usize;
        self.count(raw, min_element_bytes, max, what)
    }

    /// A u16 length prefix, validated by [`Reader::count`].
    fn len_u16(
        &mut self,
        min_element_bytes: usize,
        max: usize,
        what: &str,
    ) -> Result<usize, SerializeError> {
        let raw = self.u16()? as usize;
        self.count(raw, min_element_bytes, max, what)
    }

    fn count(
        &self,
        raw: usize,
        min_element_bytes: usize,
        max: usize,
        what: &str,
    ) -> Result<usize, SerializeError> {
        if raw > max {
            return Err(SerializeError::new(&format!(
                "{} too large ({} entries, max {})",
                what, raw, max
            )));
        }
        if raw.saturating_mul(min_element_bytes) > self.remaining() {
            return Err(SerializeError::new(&format!(
                "{} claims {} entries but only {} bytes remain",
                what,
                raw,
                self.remaining()
            )));
        }
        Ok(raw)
    }
}

/// Smallest possible encoding of a nested chunk: empty name, 4 meta bytes,
/// four empty u32-counted sections and two empty u16-counted ones.
const MIN_CHUNK_BYTES: usize = 4 + 4 + 4 * 4 + 2 * 2;

fn read_chunk_root(r: &mut Reader<'_>) -> Result<Chunk, SerializeError> {
    let magic = r
        .bytes(4)
        .map_err(|_| SerializeError::new("not a valid Forge bytecode file (too short)"))?;
    if magic != MAGIC {
        return Err(SerializeError::new(
            "not a valid Forge bytecode file (bad magic bytes)",
        ));
    }

    let version: [u8; 2] = r.array()?;
    if version[0] > VERSION_MAJOR || (version[0] == VERSION_MAJOR && version[1] > VERSION_MINOR) {
        return Err(SerializeError::new(&format!(
            "bytecode version {}.{} is newer than supported {}.{}",
            version[0], version[1], VERSION_MAJOR, VERSION_MINOR
        )));
    }

    read_chunk_inner(r, version[1], 1)
}

fn read_chunk_inner(
    r: &mut Reader<'_>,
    minor_version: u8,
    depth: usize,
) -> Result<Chunk, SerializeError> {
    if depth > MAX_PROTO_DEPTH {
        return Err(SerializeError::new(&format!(
            "prototypes nest deeper than {}",
            MAX_PROTO_DEPTH
        )));
    }
    let name = read_string(r)?;

    let meta: [u8; 3] = r.array()?;
    let arity = meta[0];
    let max_registers = meta[1];
    let upvalue_count = meta[2];
    // Bytecode older than v1.3 has no default parameters: every parameter
    // was optional at run time, so keep it that way.
    let min_arity = if minor_version >= 3 { r.u8()? } else { 0 };

    // Smallest constant: a 1-byte tag (null).
    let const_count = r.len_u32(1, 65536, "constant pool")?;
    let mut constants = Vec::with_capacity(const_count);
    for _ in 0..const_count {
        constants.push(read_constant(r)?);
    }

    let code_count = r.len_u32(4, MAX_CODE_LEN, "code section")?;
    let mut code = Vec::with_capacity(code_count);
    for _ in 0..code_count {
        code.push(r.u32()?);
    }

    let lines_count = r.len_u32(4, MAX_CODE_LEN, "line table")?;
    if lines_count != code.len() {
        return Err(SerializeError::new(&format!(
            "line table length {} does not match code length {}",
            lines_count,
            code.len()
        )));
    }
    let mut lines = Vec::with_capacity(lines_count);
    for _ in 0..lines_count {
        lines.push(r.u32()? as usize);
    }

    let cols = if minor_version >= 2 {
        let cols_count = r.len_u32(4, MAX_CODE_LEN, "column table")?;
        if cols_count != code.len() {
            return Err(SerializeError::new(&format!(
                "column table length {} does not match code length {}",
                cols_count,
                code.len()
            )));
        }
        let mut cols = Vec::with_capacity(cols_count);
        for _ in 0..cols_count {
            cols.push(r.u32()? as usize);
        }
        cols
    } else {
        vec![0; code.len()]
    };

    let proto_count = r.len_u16(MIN_CHUNK_BYTES, 65536, "prototype table")?;
    let mut prototypes = Vec::with_capacity(proto_count);
    for _ in 0..proto_count {
        prototypes.push(read_chunk_inner(r, minor_version, depth + 1)?);
    }

    let uv_sources_count = r.len_u16(2, 256, "upvalue source table")?;
    let mut upvalue_sources = Vec::with_capacity(uv_sources_count);
    for _ in 0..uv_sources_count {
        let source: [u8; 2] = r.array()?;
        let upvalue_source = match source[0] {
            0x01 => UpvalueSource::Local(source[1]),
            0x02 => UpvalueSource::Upvalue(source[1]),
            other => {
                return Err(SerializeError::new(&format!(
                    "unknown upvalue source tag: 0x{:02x}",
                    other
                )));
            }
        };
        upvalue_sources.push(upvalue_source);
    }

    Ok(Chunk {
        code,
        constants,
        lines,
        cols,
        name,
        prototypes,
        max_registers,
        upvalue_count,
        arity,
        min_arity,
        upvalue_sources,
        proto_id: crate::vm::bytecode::next_proto_id(),
        global_hints: Vec::new(),
        global_ids: Default::default(),
    })
}

fn read_constant(r: &mut Reader<'_>) -> Result<Constant, SerializeError> {
    match r.u8()? {
        0x01 => Ok(Constant::Int(r.i64()?)),
        0x02 => Ok(Constant::Float(r.f64()?)),
        0x03 => Ok(Constant::Bool(r.u8()? != 0)),
        0x04 => Ok(Constant::Null),
        0x05 => Ok(Constant::Str(read_string(r)?)),
        other => Err(SerializeError::new(&format!(
            "unknown constant tag: 0x{:02x}",
            other
        ))),
    }
}

fn read_string(r: &mut Reader<'_>) -> Result<String, SerializeError> {
    let len = r.len_u32(1, 10_000_000, "string")?;
    let bytes = r.bytes(len)?;
    String::from_utf8(bytes.to_vec())
        .map_err(|_| SerializeError::new("invalid UTF-8 in string constant"))
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
        assert_eq!(original.cols, restored.cols);
        assert_eq!(original.constants.len(), restored.constants.len());
        assert_eq!(original.prototypes.len(), restored.prototypes.len());
    }

    #[test]
    fn round_trip_columns() {
        let mut original = Chunk::new("<columns>");
        original.max_registers = 1;
        original.emit_at(encode_abc(OpCode::LoadNull, 0, 0, 0), 7, 13);
        original.emit_at(encode_abc(OpCode::Return, 0, 0, 0), 8, 5);

        let bytes = serialize_chunk(&original).unwrap();
        let restored = deserialize_chunk(&bytes).unwrap();

        assert_eq!(original.lines, restored.lines);
        assert_eq!(original.cols, restored.cols);
    }

    fn serialize_chunk_v1_1(chunk: &Chunk) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.push(1);
        bytes.push(1);
        write_string(&mut bytes, &chunk.name).unwrap();
        bytes.push(chunk.arity);
        bytes.push(chunk.max_registers);
        bytes.push(chunk.upvalue_count);

        write_u32(&mut bytes, chunk.constants.len() as u32).unwrap();
        for constant in &chunk.constants {
            write_constant(&mut bytes, constant).unwrap();
        }

        write_u32(&mut bytes, chunk.code.len() as u32).unwrap();
        for &instruction in &chunk.code {
            write_u32(&mut bytes, instruction).unwrap();
        }

        write_u32(&mut bytes, chunk.lines.len() as u32).unwrap();
        for &line in &chunk.lines {
            write_u32(&mut bytes, line as u32).unwrap();
        }

        write_u16(&mut bytes, chunk.prototypes.len() as u16).unwrap();
        for prototype in &chunk.prototypes {
            bytes.extend_from_slice(&serialize_chunk_v1_1(prototype)[6..]);
        }

        write_u16(&mut bytes, chunk.upvalue_sources.len() as u16).unwrap();
        for &src in &chunk.upvalue_sources {
            match src {
                UpvalueSource::Local(reg) => {
                    bytes.push(0x01);
                    bytes.push(reg);
                }
                UpvalueSource::Upvalue(idx) => {
                    bytes.push(0x02);
                    bytes.push(idx);
                }
            }
        }

        bytes
    }

    #[test]
    fn deserializes_v1_1_without_columns_as_zero_columns() {
        let original = make_simple_chunk();
        let bytes = serialize_chunk_v1_1(&original);
        let restored = deserialize_chunk(&bytes).unwrap();

        assert_eq!(restored.code, original.code);
        assert_eq!(restored.lines, original.lines);
        assert_eq!(restored.cols, vec![0; restored.code.len()]);
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
        let restored = decode_chunk_unverified(&bytes).unwrap();

        assert_eq!(restored.name, "<empty>");
        assert_eq!(restored.code.len(), 0);
        assert_eq!(restored.constants.len(), 0);
        assert_eq!(restored.prototypes.len(), 0);

        // It decodes, but an empty chunk is not executable bytecode.
        let err = deserialize_chunk(&bytes).unwrap_err();
        assert!(err.message.contains("empty code section"), "{}", err);
    }

    /// Serialized `chunk` with one u32 field at byte `offset` overwritten.
    fn patched(chunk: &Chunk, offset: usize, value: u32) -> Vec<u8> {
        let mut bytes = serialize_chunk(chunk).unwrap();
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        bytes
    }

    /// Byte offset of the constant-count field of the root chunk.
    fn const_count_offset(chunk: &Chunk) -> usize {
        // magic + version, name (u32 len + bytes), 4 meta bytes.
        6 + 4 + chunk.name.len() + 4
    }

    #[test]
    fn hostile_length_prefixes_are_rejected_before_allocating() {
        let chunk = make_simple_chunk();
        let at = const_count_offset(&chunk);
        // A count within the hard cap but beyond what the input can hold.
        let err = deserialize_chunk(&patched(&chunk, at, 60_000)).unwrap_err();
        assert!(err.message.contains("only"), "{}", err);
        let err = deserialize_chunk(&patched(&chunk, at, u32::MAX)).unwrap_err();
        assert!(err.message.contains("too large"), "{}", err);

        // Code count, right after the constants.
        let bytes = serialize_chunk(&chunk).unwrap();
        let code_at = bytes.len()
            - (4 + 4 * chunk.code.len()) // cols
            - (4 + 4 * chunk.lines.len()) // lines
            - (4 * chunk.code.len()) // code
            - 4 // code count
            - 2 // proto count
            - 2; // upvalue count
        assert_eq!(
            u32::from_le_bytes(bytes[code_at..code_at + 4].try_into().unwrap()) as usize,
            chunk.code.len()
        );
        let err = deserialize_chunk(&patched(&chunk, code_at, 999_999)).unwrap_err();
        assert!(err.message.contains("code section claims"), "{}", err);

        // A string length larger than the input.
        let err = deserialize_chunk(&patched(&chunk, 6, 9_999_999)).unwrap_err();
        assert!(err.message.contains("string claims"), "{}", err);
    }

    #[test]
    fn deeply_nested_prototypes_are_rejected_without_recursing_unboundedly() {
        // Header, then MAX_PROTO_DEPTH + 10 chunks that each declare one
        // nested prototype (and are never finished).
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.push(VERSION_MAJOR);
        bytes.push(VERSION_MINOR);
        for _ in 0..MAX_PROTO_DEPTH + 10 {
            write_string(&mut bytes, "").unwrap();
            bytes.extend_from_slice(&[0, 1, 0, 0]); // arity, regs, upvalues, min_arity
            for _ in 0..4 {
                write_u32(&mut bytes, 0).unwrap(); // constants, code, lines, cols
            }
            write_u16(&mut bytes, 1).unwrap(); // one prototype
        }
        bytes.resize(bytes.len() + 64 * 1024, 0);
        let err = deserialize_chunk(&bytes).unwrap_err();
        assert!(err.message.contains("nest deeper"), "{}", err);
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = serialize_chunk(&make_simple_chunk()).unwrap();
        bytes.push(0);
        let err = deserialize_chunk(&bytes).unwrap_err();
        assert!(err.message.contains("trailing"), "{}", err);
    }

    #[test]
    fn deserialize_runs_the_verifier() {
        let mut chunk = make_simple_chunk();
        chunk.code[0] = encode_abx(OpCode::LoadConst, 0, 77);
        let err = deserialize_chunk(&serialize_chunk(&chunk).unwrap()).unwrap_err();
        assert!(err.message.contains("constant index 77"), "{}", err);
        assert!(
            err.message.contains("invalid bytecode in '<test>'"),
            "{}",
            err
        );
    }

    #[test]
    fn every_truncation_of_a_compiled_program_is_an_error_not_a_panic() {
        use crate::lexer::Lexer;
        use crate::parser::Parser;
        use crate::vm::compiler;
        let src = "fn f(a, b = 1) { let g = fn() { return a + b }\n return g() }\nlet x = [f(1), \"s\", 2.5]\nsay x";
        let tokens = Lexer::new(src).tokenize().unwrap();
        let program = Parser::new(tokens).parse_program().unwrap();
        let bytes = serialize_chunk(&compiler::compile(&program).unwrap()).unwrap();
        deserialize_chunk(&bytes).unwrap();
        for len in 0..bytes.len() {
            assert!(deserialize_chunk(&bytes[..len]).is_err(), "prefix {len}");
        }
        // Single-byte corruptions either decode to verified bytecode or fail.
        for i in 0..bytes.len() {
            for flip in [0x01u8, 0x80, 0xFF] {
                let mut b = bytes.clone();
                b[i] ^= flip;
                let _ = deserialize_chunk(&b);
            }
        }
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
    fn min_arity_roundtrips_and_defaults_for_old_bytecode() {
        let mut chunk = make_simple_chunk();
        chunk.arity = 3;
        chunk.min_arity = 1;
        let restored = deserialize_chunk(&serialize_chunk(&chunk).unwrap()).unwrap();
        assert_eq!(restored.arity, 3);
        assert_eq!(restored.min_arity, 1);

        // v1.1 bytecode predates default parameters: no required minimum.
        let old = deserialize_chunk(&serialize_chunk_v1_1(&chunk)).unwrap();
        assert_eq!(old.arity, 3);
        assert_eq!(old.min_arity, 0);
    }

    #[test]
    fn future_minor_version_rejected() {
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC);
        data.push(VERSION_MAJOR);
        data.push(VERSION_MINOR + 1);
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
        chunk.emit(encode_abc(OpCode::Spawn, 0, 0, 0), 13);
        chunk.emit(encode_abc(OpCode::Await, 1, 0, 0), 14);
        chunk.emit(encode_abc(OpCode::Schedule, 2, 3, 4), 15);
        chunk.emit(encode_abc(OpCode::Watch, 5, 6, 0), 16);
        chunk.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 17);

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
        assert_eq!(chunk.cols, restored.cols);
        assert_eq!(chunk.name, restored.name);
        assert_eq!(chunk.arity, restored.arity);
        assert_eq!(chunk.max_registers, restored.max_registers);
        assert_eq!(chunk.prototypes.len(), restored.prototypes.len());

        for (orig, rest) in chunk.constants.iter().zip(restored.constants.iter()) {
            assert!(orig.identical(rest));
        }

        for (orig, rest) in chunk.prototypes.iter().zip(restored.prototypes.iter()) {
            assert_eq!(orig.code, rest.code);
            assert_eq!(orig.cols, rest.cols);
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
        assert_eq!(orig_fib.cols, rest_fib.cols);
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
        assert_eq!(chunk.cols, restored.cols);
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
