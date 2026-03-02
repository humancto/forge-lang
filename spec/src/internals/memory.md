# Memory Model

Forge uses different memory management strategies depending on the execution tier.

## Interpreter Memory

The interpreter uses Rust's standard memory management with no garbage collector.

### Value Representation

All Forge values are represented by the `Value` enum in Rust. Heap-allocated variants:

| Value                             | Heap Allocation                          |
| --------------------------------- | ---------------------------------------- |
| `String(String)`                  | Rust `String` (heap-allocated, growable) |
| `Array(Vec<Value>)`               | `Vec` on the heap                        |
| `Object(IndexMap<String, Value>)` | `IndexMap` on the heap                   |
| `Function { closure, ... }`       | Captured environment on the heap         |
| `Lambda { closure, ... }`         | Captured environment on the heap         |
| `Channel(Arc<ChannelInner>)`      | Reference-counted channel                |
| `TaskHandle(Arc<TaskInner>)`      | Reference-counted task                   |

Primitive types (`Int`, `Float`, `Bool`, `Null`) are stored inline without heap allocation.

### Ownership and Cloning

The interpreter clones values when:

- Passing arguments to functions
- Returning values from functions
- Assigning variables
- Indexing into arrays or objects

This is a simple, correct approach that avoids shared mutable state. The trade-off is increased memory allocation for large data structures.

### Reference Counting

Channels and task handles use `Arc` (atomic reference counting) for shared ownership across threads. When the last reference is dropped, the resource is freed.

### No Manual Memory Management

Forge programs never explicitly allocate or free memory. There is no `malloc`, `free`, `new`, or `delete`. Memory is managed entirely by the Rust runtime.

## VM Memory

The bytecode VM uses a **mark-sweep garbage collector** for heap-allocated objects.

### VM Value Representation

The VM has its own `Value` enum (in `src/vm/value.rs`) optimized for the register-based architecture:

| Value        | Representation         |
| ------------ | ---------------------- |
| `Int(i64)`   | Inline 64-bit integer  |
| `Float(f64)` | Inline 64-bit float    |
| `Bool(bool)` | Inline boolean         |
| `Null`       | Inline null marker     |
| `Obj(usize)` | Index into the GC heap |

Heap-allocated objects are stored in the GC's object table and referenced by index.

### Object Kinds

```
ObjKind::String(String)
ObjKind::Array(Vec<Value>)
ObjKind::Object(IndexMap<String, Value>)
ObjKind::Closure { ... }
```

### GC Algorithm

The mark-sweep collector works in two phases:

1. **Mark**: Starting from roots (registers, global environment, call stack frames), traverse all reachable objects and set their mark bit.
2. **Sweep**: Walk the object table. Free any object without a mark bit. Clear all mark bits for the next cycle.

Collection is triggered when the number of allocated objects exceeds a dynamic threshold that grows as the program allocates more objects.

### GC Roots

The following locations are scanned as roots:

- All registers in the current and parent call frames
- The global environment
- Constant pools of active chunks
- Green thread stacks

## JIT Memory

The JIT compiler allocates executable memory pages for generated native code via Cranelift's `JITModule`. This memory is mapped with execute permissions and is freed when the JIT module is dropped.

JIT-compiled functions operate on machine registers and stack-allocated values. There is no GC interaction -- the JIT tier does not support heap-allocated objects.
