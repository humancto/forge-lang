//! Tests of the C-ABI entry points in `lib.rs` (also run under Miri).

use crate::vm::bytecode::{encode_abc, encode_abx, Chunk, Constant, OpCode};
use crate::vm::serialize::serialize_chunk;
use crate::{forge_execute_bytecode, forge_execute_source};

fn tiny_program() -> Vec<u8> {
    let mut chunk = Chunk::new("<main>");
    chunk.max_registers = 2;
    chunk.add_constant(Constant::Int(40));
    chunk.add_constant(Constant::Int(2));
    chunk.emit(encode_abx(OpCode::LoadConst, 0, 0), 1);
    chunk.emit(encode_abx(OpCode::LoadConst, 1, 1), 1);
    chunk.emit(encode_abc(OpCode::Add, 0, 0, 1), 1);
    chunk.emit(encode_abc(OpCode::Return, 0, 0, 0), 1);
    serialize_chunk(&chunk).expect("serialize")
}

#[test]
fn miri_execute_bytecode_runs_valid_bytecode() {
    let bytes = tiny_program();
    assert_eq!(forge_execute_bytecode(bytes.as_ptr(), bytes.len()), 0);
}

#[test]
fn miri_execute_bytecode_rejects_bad_input_without_ub() {
    assert_eq!(forge_execute_bytecode(std::ptr::null(), 10), 1);
    let bytes = tiny_program();
    assert_eq!(forge_execute_bytecode(bytes.as_ptr(), 0), 1);
    // Truncated: the length the caller passes is the only bound read.
    assert_eq!(forge_execute_bytecode(bytes.as_ptr(), bytes.len() / 2), 1);
    // Corrupt: an out-of-range register is rejected by the verifier.
    let mut bad = bytes.clone();
    let n = bad.len();
    // The final instruction (`Return R0`) sits before the line/column tables
    // and the two u16 table counts: patch its A operand to 200.
    let ret_a = n - 2 - 2 - (4 + 4 * 4) - (4 + 4 * 4) - 4 + 2;
    assert_eq!(bad[ret_a], 0);
    bad[ret_a] = 200;
    assert_eq!(forge_execute_bytecode(bad.as_ptr(), bad.len()), 1);
}

#[test]
fn miri_execute_source_validates_pointers_before_reading() {
    let src = b"let x = 1";
    // SAFETY (all calls): every non-null pointer covers its length.
    unsafe {
        assert_eq!(
            forge_execute_source(std::ptr::null(), 5, std::ptr::null(), 0, 0),
            1
        );
        assert_eq!(
            forge_execute_source(src.as_ptr(), 0, std::ptr::null(), 0, 0),
            1
        );
        assert_eq!(
            forge_execute_source(src.as_ptr(), src.len(), std::ptr::null(), 3, 0),
            1
        );
        let invalid_utf8 = [0xFFu8, 0xFE];
        assert_eq!(
            forge_execute_source(invalid_utf8.as_ptr(), 2, std::ptr::null(), 0, 0),
            1
        );
        assert_eq!(
            forge_execute_source(src.as_ptr(), src.len(), invalid_utf8.as_ptr(), 2, 0),
            1
        );
    }
}
