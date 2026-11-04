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
