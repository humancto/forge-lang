use crate::interpreter::Interpreter;
use crate::lexer::Lexer;
use crate::parser::ast::Program;
use crate::parser::Parser;
use crate::vm::jit::jit_module::JitCompiler;
use crate::vm::jit::type_analysis;
use crate::vm::machine::{JitEntry, VM};
use crate::vm::{compiler, serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SupportedParityCase {
    pub path: PathBuf,
    pub source: String,
    pub expected: String,
}

#[derive(Debug, Clone)]
pub struct VmRejectionCase {
    pub path: PathBuf,
    pub source: String,
    pub expected_error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendOutputs {
    pub interpreter: String,
    pub vm: String,
    pub bytecode: String,
    pub jit: String,
}

pub fn load_supported_cases() -> Vec<SupportedParityCase> {
    load_cases("supported")
        .into_iter()
        .map(|(path, source)| SupportedParityCase {
            expected: metadata_value(&source, "expect")
                .unwrap_or_else(|| panic!("missing '// expect:' header in {}", path.display())),
            path,
            source,
        })
        .collect()
}

pub fn load_vm_rejection_cases() -> Vec<VmRejectionCase> {
    load_cases("unsupported_vm")
        .into_iter()
        .map(|(path, source)| VmRejectionCase {
            expected_error: metadata_value(&source, "expect-error").unwrap_or_else(|| {
                panic!("missing '// expect-error:' header in {}", path.display())
            }),
            path,
            source,
        })
        .collect()
}

pub fn parse_program(source: &str) -> Program {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer error");
    let mut parser = Parser::new(tokens);
    parser.parse_program().expect("parse error")
}

pub fn run_value_backends(source: &str) -> BackendOutputs {
    let program = parse_program(source);
    BackendOutputs {
        interpreter: run_on_interpreter_value(&program),
        vm: run_on_vm_value(&program),
        bytecode: run_on_bytecode_value(&program),
        jit: run_on_jit_value(&program),
    }
}

pub fn assert_supported_case(case: &SupportedParityCase) {
    let outputs = run_value_backends(&case.source);
    assert_eq!(
        outputs.interpreter,
        case.expected,
        "{} interpreter output mismatch",
        case.path.display()
    );
    assert_eq!(
        outputs.vm,
        case.expected,
        "{} vm output mismatch",
        case.path.display()
    );
    assert_eq!(
        outputs.bytecode,
        case.expected,
        "{} bytecode output mismatch",
        case.path.display()
    );
    assert_eq!(
        outputs.jit,
        case.expected,
        "{} jit output mismatch",
        case.path.display()
    );
}

fn load_cases(kind: &str) -> Vec<(PathBuf, String)> {
    let dir = fixtures_root().join(kind);
    let entries = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to read parity fixtures '{}': {}", dir.display(), e));
    let mut cases = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "fg"))
        .collect::<Vec<_>>();
    cases.sort();
    assert!(
        !cases.is_empty(),
        "expected parity fixtures in '{}'",
        dir.display()
    );

    cases
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read '{}': {}", path.display(), e));
            (path, source)
        })
        .collect()
}

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("parity")
}

fn metadata_value(source: &str, key: &str) -> Option<String> {
    for line in source.lines().take(8) {
        let trimmed = line.trim();
        if !trimmed.starts_with("//") {
            continue;
        }
        let comment = trimmed.trim_start_matches("//").trim();
        let prefix = format!("{}:", key);
        if let Some(rest) = comment.strip_prefix(&prefix) {
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn run_on_interpreter_value(program: &Program) -> String {
    let mut interpreter = Interpreter::new();
    let value = interpreter.run_repl(program).expect("interpreter error");
    value.to_string()
}

fn run_on_vm_value(program: &Program) -> String {
    let chunk = compiler::compile_repl(program).expect("vm compile error");
    let mut vm = VM::new();
    let value = vm.execute(&chunk).expect("vm execution error");
    value.display(&vm.gc)
}

fn run_on_bytecode_value(program: &Program) -> String {
    let chunk = compiler::compile_repl(program).expect("bytecode compile error");
    let bytes = serialize::serialize_chunk(&chunk).expect("serialize error");
    let restored = serialize::deserialize_chunk(&bytes).expect("deserialize error");
    let mut vm = VM::new();
    let value = vm.execute(&restored).expect("bytecode execution error");
    value.display(&vm.gc)
}

fn run_on_jit_value(program: &Program) -> String {
    let chunk = compiler::compile_repl(program).expect("jit compile error");

    let mut jit = JitCompiler::new().expect("jit init error");
    for (index, proto) in chunk.prototypes.iter().enumerate() {
        let name = if proto.name.is_empty() {
            format!("fn_{}", index)
        } else {
            proto.name.clone()
        };
        let info = type_analysis::analyze(proto);
        if !info.has_unsupported_ops {
            let _ = jit.compile_function(proto, &name);
        }
    }

    let mut vm = VM::new();
    for (index, proto) in chunk.prototypes.iter().enumerate() {
        let name = if proto.name.is_empty() {
            format!("fn_{}", index)
        } else {
            proto.name.clone()
        };
        let info = type_analysis::analyze(proto);
        if !info.has_unsupported_ops {
            if let Some(ptr) = jit.get_compiled(&name) {
            vm.jit_cache.insert(
                name,
                JitEntry {
                    ptr,
                    uses_float: info.has_float,
                },
            );
        }
        }
    }

    let value = vm.execute(&chunk).expect("jit-assisted execution error");
    value.display(&vm.gc)
}
