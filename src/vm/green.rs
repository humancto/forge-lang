#![allow(dead_code)]

use super::bytecode::Chunk;
/// Green thread scheduler for Forge VM.
///
/// Phase 3 implementation: cooperative green threads.
/// Each green thread has its own register file and call stack
/// but shares the GC heap and globals.
///
/// Current: synchronous execution (spawn runs inline).
/// Future: tokio-based cooperative scheduling with yield points
/// at function calls and loop back-edges.
use super::machine::{VMError, VM};
use super::value::Value;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThreadState {
    Ready,
    Running,
    Yielded,
    Completed,
}

pub struct GreenThread {
    pub id: usize,
    pub state: ThreadState,
    pub chunk: Chunk,
}

pub struct Scheduler {
    threads: Vec<GreenThread>,
    next_id: usize,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            threads: Vec::new(),
            next_id: 0,
        }
    }

    pub fn spawn(&mut self, chunk: Chunk) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.threads.push(GreenThread {
            id,
            state: ThreadState::Ready,
            chunk,
        });
        id
    }

    /// Run all spawned threads. Currently synchronous (round-robin single-step).
    pub fn run_all(&mut self, vm: &mut VM) -> Result<(), VMError> {
        while let Some(thread) = self
            .threads
            .iter_mut()
            .find(|t| t.state == ThreadState::Ready)
        {
            thread.state = ThreadState::Running;
            let chunk = thread.chunk.clone();
            vm.execute(&chunk)?;
            thread.state = ThreadState::Completed;
        }
        self.threads.retain(|t| t.state != ThreadState::Completed);
        Ok(())
    }

    pub fn active_count(&self) -> usize {
        self.threads
            .iter()
            .filter(|t| t.state != ThreadState::Completed)
            .count()
    }
}

/// Execute a spawn block. Currently runs synchronously.
/// When the tokio scheduler is ready, this will create a tokio task instead.
pub fn spawn_sync(vm: &mut VM, closure: Value) -> Result<(), VMError> {
    vm.call_value(closure, vec![])?;
    Ok(())
}
