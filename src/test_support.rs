//! Helpers shared by tests.

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

/// Poll a future that is ready at once, as futures built on mocks are.
pub(crate) fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut task = Context::from_waker(Waker::noop());

    match pin!(future).poll(&mut task) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("future should be ready at once"),
    }
}

/// A small repeatable random source, so a failure can be run again.
pub(crate) struct Random(u64);

impl Random {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// A number below `limit` (xorshift).
    pub(crate) fn below(&mut self, limit: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % limit as u64) as usize
    }

    /// Up to `max_parts` pieces picked from `parts`, joined.
    pub(crate) fn text(&mut self, parts: &[&str], max_parts: usize) -> String {
        let count = self.below(max_parts) + 1;
        (0..count).map(|_| parts[self.below(parts.len())]).collect()
    }
}
