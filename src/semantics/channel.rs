//! The receiving end of a Forge channel, shared by both engines.
//!
//! A `std::sync::mpsc::Receiver` cannot tell "closed and drained" from
//! "empty" without taking a value. [`Receiver`] keeps one value of
//! lookahead so `is_closed(ch)` can ask without losing anything: every
//! receive (`receive`, `try_receive`, `select`, `for x in ch`) takes the
//! looked-ahead value first, so values still come out in send order.
//!
//! Channel rules (both engines):
//!
//! * `close(ch)` drops the only sender: later `send`s fail with `channel
//!   closed` (E0030), values sent before stay receivable.
//! * A channel is *finished* once it is closed and every value sent before
//!   `close` was received. `is_closed(ch)` is true exactly then (like
//!   Kotlin's `isClosedForReceive`), so "finished" can never be observed
//!   while a value is still in flight.
//! * `select(chs, timeout?)` skips finished channels; it returns `null` on
//!   timeout and, at once, when every channel is finished (or `chs` is
//!   empty). After a `null`, `all(chs, is_closed)` says which it was.

use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::time::Duration;

/// A channel's receiver with one value of lookahead (see the module docs).
#[derive(Debug)]
pub struct Receiver<T> {
    rx: mpsc::Receiver<T>,
    peeked: Option<T>,
}

impl<T> Receiver<T> {
    pub fn new(rx: mpsc::Receiver<T>) -> Self {
        Receiver { rx, peeked: None }
    }

    /// The next value if one is waiting.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        match self.peeked.take() {
            Some(v) => Ok(v),
            None => self.rx.try_recv(),
        }
    }

    /// Wait at most `timeout` for the next value.
    pub fn recv_timeout(&mut self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        match self.peeked.take() {
            Some(v) => Ok(v),
            None => crate::clock::recv_timeout(&self.rx, timeout),
        }
    }

    /// Closed and drained: nothing was waiting and nothing can arrive.
    pub fn is_finished(&mut self) -> bool {
        if self.peeked.is_some() {
            return false;
        }
        match self.rx.try_recv() {
            Ok(v) => {
                self.peeked = Some(v);
                false
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finished_only_after_close_and_drain_and_order_is_kept() {
        let (tx, rx) = mpsc::channel();
        let mut rx = Receiver::new(rx);
        assert!(!rx.is_finished());
        tx.send(1).expect("send");
        tx.send(2).expect("send");
        drop(tx);
        assert!(!rx.is_finished(), "values are still waiting");
        assert!(!rx.is_finished(), "asking twice keeps the value");
        assert_eq!(rx.try_recv(), Ok(1));
        assert_eq!(rx.recv_timeout(Duration::from_millis(1)), Ok(2));
        assert!(rx.is_finished());
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
    }
}
