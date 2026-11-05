//! What an answer reports while it works, so a slow model does not look
//! frozen. Contract: a stage carries only our own words and numbers, never
//! text a model wrote, and reporting can never change the answer.

/// Where an answer has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The request is with the model; nothing has come back yet.
    Asking,
    /// The model is writing its reply.
    Writing {
        /// Characters of reply so far.
        chars: usize,
    },
    /// A check on the server during a long wait. Says nothing about where
    /// the answer has got to, only whether the server is still there.
    Connection {
        /// The server answered the check.
        answering: bool,
    },
    /// The decision model is ranking the commands.
    Ranking,
    /// The decision model is judging whether the best command is safe.
    Checking,
}

/// Something that wants to hear about stages. `Sync` because the futures
/// that report them may move between threads.
pub trait Progress: Sync {
    /// The answer has reached `stage`.
    fn stage(&self, stage: Stage);
}

/// Reports to nobody: for scripts, and for tests about something else.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoProgress;

impl Progress for NoProgress {
    fn stage(&self, _stage: Stage) {}
}
