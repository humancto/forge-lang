//! Persistent `run_forge` sessions.
//!
//! With a `session_id`, `run_forge` runs the code on an interpreter that
//! keeps its top-level state (variables, functions, structs) between calls,
//! so an agent can build up work step by step. Each step still runs under
//! the full sandbox (fresh worker thread, policy, deadline, output capture,
//! cancellation); only the interpreter's environment carries over.
//!
//! Bounds: at most `max_sessions` live at once (a new id beyond that is
//! refused until one is reset or expires), a session idle for longer than
//! `idle` is dropped, and a session runs one call at a time. Each step gets
//! a fresh fuel budget; the memory limit covers the session's whole state
//! (the bytes retained after a step count from the start of the next, via
//! `Sandbox::memory_baseline`). A step that
//! fails keeps whatever it defined before the error, like a REPL. Tasks a
//! step `spawn`s stop when that step ends. A step whose worker had to be
//! abandoned (it did not stop after a timeout or cancel) loses the session.
//!
//! Engine: session steps always run on the interpreter, whatever
//! `forge mcp --engine` says (one-shot `run_forge` calls and Forge tools
//! use the configured engine). The containment is the same either way
//! (`Sandbox::run_interpreter` is `Sandbox::run_contained`). What the VM
//! lacks is a faithful way to carry state between separately compiled
//! steps: its compiler keeps top-level `let`s in registers of the step's
//! chunk (captured by closures as upvalue cells) and only copies a binding
//! to a global when it is defined, so a later step would read stale values
//! (`let mut n = 0; fn bump() { n = n + 1 }` then `bump(); say n`). A
//! session on the VM needs a compile mode where top-level bindings live in
//! globals; the per-step state could then be kept as a frozen
//! `vm::serve::VmTemplate` and thawed on each step's worker.

use crate::interpreter::Interpreter;
use crate::sandbox::CancelHandle;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Longest accepted session id.
pub const MAX_SESSION_ID_LEN: usize = 128;

/// A session id is 1-128 of `[A-Za-z0-9_.:-]`.
pub fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_SESSION_ID_LEN
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
}

struct Session {
    /// `None` while a call is running with it.
    interp: Option<Interpreter>,
    last_used: Instant,
    /// Distinguishes this session from a later one with the same id, so a
    /// call that outlives a reset cannot put its interpreter back.
    generation: u64,
    /// The running call, so a reset can stop it.
    running: Option<CancelHandle>,
    /// Bytes the interpreter holds (allocation meter, as of the last call);
    /// the next call starts its memory budget from here.
    memory: usize,
}

/// Result of [`Sessions::checkout`].
pub enum Checkout {
    Ready {
        interp: Box<Interpreter>,
        generation: u64,
        created: bool,
        /// See `Session::memory`.
        memory: usize,
    },
    /// A call is already running in this session.
    Busy,
    /// `max_sessions` are live; nothing was created.
    Full(usize),
}

#[derive(Default)]
pub struct Sessions {
    map: HashMap<String, Session>,
    next_generation: u64,
}

impl Sessions {
    /// Drop sessions idle for longer than `idle` (never running ones).
    fn expire(&mut self, idle: Duration) {
        let now = Instant::now();
        self.map
            .retain(|_, s| s.interp.is_none() || now.duration_since(s.last_used) <= idle);
    }

    /// Take the session's interpreter for one call, creating the session if
    /// needed. `cancel` is the call's handle (a reset triggers it).
    pub fn checkout(
        &mut self,
        id: &str,
        max_sessions: usize,
        idle: Duration,
        cancel: &CancelHandle,
    ) -> Checkout {
        self.expire(idle);
        if let Some(session) = self.map.get_mut(id) {
            let Some(interp) = session.interp.take() else {
                return Checkout::Busy;
            };
            session.running = Some(cancel.clone());
            session.last_used = Instant::now();
            return Checkout::Ready {
                interp: Box::new(interp),
                generation: session.generation,
                created: false,
                memory: session.memory,
            };
        }
        if self.map.len() >= max_sessions {
            return Checkout::Full(max_sessions);
        }
        self.next_generation += 1;
        let generation = self.next_generation;
        self.map.insert(
            id.to_string(),
            Session {
                interp: None,
                last_used: Instant::now(),
                generation,
                running: Some(cancel.clone()),
                memory: 0,
            },
        );
        Checkout::Ready {
            interp: Box::new(Interpreter::new()),
            generation,
            created: true,
            memory: 0,
        }
    }

    /// Return the interpreter after a call. `None` (the worker was
    /// abandoned) ends the session. A no-op if the session was reset (or
    /// replaced) meanwhile. Returns whether the session lives on.
    pub fn checkin(
        &mut self,
        id: &str,
        generation: u64,
        interp: Option<Interpreter>,
        memory: usize,
    ) -> bool {
        let Some(session) = self.map.get_mut(id) else {
            return false;
        };
        if session.generation != generation {
            return false;
        }
        match interp {
            Some(interp) => {
                session.interp = Some(interp);
                session.running = None;
                session.memory = memory;
                session.last_used = Instant::now();
                true
            }
            None => {
                self.map.remove(id);
                false
            }
        }
    }

    /// Forget a session, stopping its running call if any. Returns whether
    /// it existed.
    pub fn reset(&mut self, id: &str) -> bool {
        match self.map.remove(id) {
            Some(session) => {
                if let Some(running) = session.running {
                    running.cancel();
                }
                true
            }
            None => false,
        }
    }

    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(c: Checkout) -> (Interpreter, u64, bool) {
        match c {
            Checkout::Ready {
                interp,
                generation,
                created,
                ..
            } => (*interp, generation, created),
            Checkout::Busy => panic!("busy"),
            Checkout::Full(_) => panic!("full"),
        }
    }

    #[test]
    fn checkout_checkin_busy_full_reset() {
        let mut s = Sessions::default();
        let idle = Duration::from_secs(60);
        let c = CancelHandle::new();
        let (interp, generation, created) = ready(s.checkout("a", 2, idle, &c));
        assert!(created);
        assert!(matches!(s.checkout("a", 2, idle, &c), Checkout::Busy));
        assert!(s.checkin("a", generation, Some(interp), 0));
        let (interp_a, gen_a, created) = ready(s.checkout("a", 2, idle, &c));
        assert!(!created);
        let (_b, _, _) = ready(s.checkout("b", 2, idle, &c));
        assert!(matches!(s.checkout("c", 2, idle, &c), Checkout::Full(2)));
        // Reset while running cancels the call; its checkin is ignored.
        assert!(s.reset("a"));
        assert!(c.is_cancelled());
        assert!(!s.checkin("a", gen_a, Some(interp_a), 0));
        assert!(!s.reset("a"));
        assert_eq!(s.count(), 1);
    }

    #[test]
    fn idle_sessions_expire_and_lost_workers_end_the_session() {
        let mut s = Sessions::default();
        let c = CancelHandle::new();
        let (interp, generation, _) = ready(s.checkout("a", 1, Duration::from_secs(60), &c));
        s.checkin("a", generation, Some(interp), 0);
        std::thread::sleep(Duration::from_millis(20));
        // A zero idle timeout expires it, which frees the slot.
        let (_, generation, created) = ready(s.checkout("b", 1, Duration::ZERO, &c));
        assert!(created);
        assert!(!s.checkin("b", generation, None, 0));
        assert_eq!(s.count(), 0);
    }

    #[test]
    fn session_ids() {
        assert!(valid_session_id("agent-1:step.2_x"));
        assert!(!valid_session_id(""));
        assert!(!valid_session_id("a b"));
        assert!(!valid_session_id(&"x".repeat(129)));
    }
}
