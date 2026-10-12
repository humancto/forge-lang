//! Database connections shared by the threads of one program.
//!
//! `db.open` / `pg.connect` make a connection *current* on the calling
//! thread. Other threads that run the same program's Forge code see it too:
//!
//! - HTTP request handlers (`runtime::server`): the connections current when
//!   the server starts — opened by the program's top level — are installed
//!   on every handler thread ([`Databases::enter_request`]);
//! - spawned tasks and `schedule`/`watch` threads: [`crate::permissions::inherit`]
//!   carries the parent thread's connections ([`Databases::enter`]).
//!
//! A connection is one [`Shared`]: a mutex around it, so statements from
//! concurrent threads run one at a time, and a *transaction owner*. Each
//! request is its own owner, so a transaction a handler opens
//! (`db.begin()`, or `BEGIN` in SQL) is private to it: statements of other
//! requests wait until it commits or rolls back (at most [`TX_WAIT`]), and
//! a transaction still open when the request ends is rolled back. Spawned
//! threads act as their parent's owner (a task's statements join the
//! transaction its parent opened).
//!
//! `db.open` / `db.close` inside a handler only change that handler's
//! thread; other requests keep the shared connection.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// How long a statement waits for another owner's open transaction.
pub const TX_WAIT: Duration = Duration::from_secs(30);

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

/// A fresh transaction-owner id.
pub fn new_owner() -> u64 {
    NEXT_OWNER.fetch_add(1, Ordering::Relaxed)
}

struct State<C> {
    conn: C,
    /// The owner whose transaction is open on the connection, if any.
    tx_owner: Option<u64>,
}

/// One connection shared between threads. See the module docs.
pub struct Shared<C> {
    state: Mutex<State<C>>,
    released: Condvar,
    /// Handle slot (`runtime::limits`) of the connection, given back when
    /// the last thread drops it.
    _slot: crate::runtime::limits::Slot,
}

/// A thread's view of a shared connection: the connection and the owner
/// its statements run as.
pub struct Handle<C> {
    pub shared: Arc<Shared<C>>,
    pub owner: u64,
}

impl<C> Clone for Handle<C> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
            owner: self.owner,
        }
    }
}

impl<C> Handle<C> {
    /// A new connection, owned by a fresh owner.
    pub fn open(conn: C, slot: crate::runtime::limits::Slot) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    conn,
                    tx_owner: None,
                }),
                released: Condvar::new(),
                _slot: slot,
            }),
            owner: new_owner(),
        }
    }

    /// The same connection under another owner.
    fn with_owner(&self, owner: u64) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
            owner,
        }
    }

    /// Run `op` on the connection. While another owner's transaction is
    /// open it first waits for it (at most [`TX_WAIT`]). Afterwards
    /// `tx_open(conn, result, was_open)` says whether this owner now has a
    /// transaction open.
    pub fn run<R>(
        &self,
        op: impl FnOnce(&mut C) -> R,
        tx_open: impl FnOnce(&C, &R, bool) -> bool,
    ) -> Result<R, String> {
        let shared = &self.shared;
        let deadline = Instant::now() + TX_WAIT;
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        while matches!(state.tx_owner, Some(o) if o != self.owner) {
            let now = Instant::now();
            if now >= deadline {
                return Err(format!(
                    "database is busy: a transaction of another request or task has been open for over {} seconds",
                    TX_WAIT.as_secs()
                ));
            }
            state = shared
                .released
                .wait_timeout(state, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        let was_open = state.tx_owner == Some(self.owner);
        let result = op(&mut state.conn);
        let open = tx_open(&state.conn, &result, was_open);
        state.tx_owner = open.then_some(self.owner);
        if was_open && !open {
            shared.released.notify_all();
        }
        Ok(result)
    }

    /// This owner is done with the connection: `end(conn, tx_open)` cleans
    /// up after it — rolling back its transaction when `tx_open` — and
    /// waiting owners are let in.
    pub fn release(&self, end: impl FnOnce(&mut C, bool)) {
        let shared = &self.shared;
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let tx_open = state.tx_owner == Some(self.owner);
        end(&mut state.conn, tx_open);
        if tx_open {
            state.tx_owner = None;
            shared.released.notify_all();
            tracing::warn!(
                target: "forge.runtime",
                "rolled back an open database transaction: its request ended or its connection was closed"
            );
        }
    }
}

/// The database connections current on a thread.
#[derive(Clone, Default)]
pub struct Databases {
    sqlite: Option<Handle<super::db::Sqlite>>,
    #[cfg(feature = "postgres")]
    pg: Option<Handle<Arc<tokio_postgres::Client>>>,
}

impl Databases {
    /// The connections current on this thread.
    pub fn current() -> Self {
        Self {
            sqlite: super::db::current_handle(),
            #[cfg(feature = "postgres")]
            pg: super::pg::current_handle(),
        }
    }

    fn install(self) -> Self {
        Self {
            sqlite: super::db::replace_handle(self.sqlite),
            #[cfg(feature = "postgres")]
            pg: super::pg::replace_handle(self.pg),
        }
    }

    /// Make these connections current on this thread, as the same owners
    /// (a spawned thread continuing its parent's work), until the guard
    /// drops.
    pub fn enter(&self) -> DatabaseScope {
        DatabaseScope {
            previous: Some(self.clone().install()),
            request: None,
        }
    }

    /// Make these connections current for one HTTP request, as a fresh
    /// owner. When the guard drops, a transaction the request left open is
    /// rolled back and the thread's previous connections come back.
    pub fn enter_request(&self) -> DatabaseScope {
        let owner = new_owner();
        let request = Self {
            sqlite: self.sqlite.as_ref().map(|h| h.with_owner(owner)),
            #[cfg(feature = "postgres")]
            pg: self.pg.as_ref().map(|h| h.with_owner(owner)),
        };
        DatabaseScope {
            previous: Some(request.clone().install()),
            request: Some(request),
        }
    }
}

/// Restores a thread's previous connections; see [`Databases::enter`].
#[must_use = "the connections are current only while the scope lives"]
pub struct DatabaseScope {
    previous: Option<Databases>,
    /// The request's own handles, released on drop.
    request: Option<Databases>,
}

impl Drop for DatabaseScope {
    fn drop(&mut self) {
        if let Some(request) = self.request.take() {
            if let Some(h) = &request.sqlite {
                super::db::release(h);
            }
            #[cfg(feature = "postgres")]
            if let Some(h) = &request.pg {
                super::pg::release(h);
            }
        }
        if let Some(previous) = self.previous.take() {
            let _ = previous.install();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn another_owners_transaction_blocks_until_released() {
        let a = Handle::open(0u32, crate::runtime::limits::Slot::none());
        let b = a.with_owner(new_owner());
        // `a` opens a transaction.
        a.run(|n| *n += 1, |_, _, _| true).expect("a runs");
        let (tx, rx) = std::sync::mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let r = b.run(|n| *n, |_, _, _| false).expect("b runs");
            tx.send(r).expect("send");
        });
        // `b` waits while `a`'s transaction is open.
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
        // Ending the request rolls back and lets `b` in.
        a.release(|n, open| {
            assert!(open);
            *n = 100
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(100));
        waiter.join().expect("join");
    }

    #[test]
    fn the_owner_keeps_using_its_transaction() {
        let a = Handle::open(Vec::<u32>::new(), crate::runtime::limits::Slot::none());
        a.run(|v| v.push(1), |_, _, _| true).expect("begin");
        a.run(|v| v.push(2), |_, _, was| was).expect("same owner");
        a.run(|v| v.push(3), |_, _, _| false).expect("commit");
        a.release(|v, open| {
            assert!(!open);
            v.clear()
        });
        assert_eq!(a.run(|v| v.clone(), |_, _, _| false), Ok(vec![]));
    }
}
