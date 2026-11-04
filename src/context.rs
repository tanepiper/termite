//! What the model and the decision endpoint are told about where the
//! command will run.

/// Where the terminal session runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    /// A shell on this machine.
    Local,
    /// A shell on another machine over SSH.
    Ssh,
}

impl Session {
    /// The name used in requests: `local` or `ssh`.
    pub fn as_str(self) -> &'static str {
        match self {
            Session::Local => "local",
            Session::Ssh => "ssh",
        }
    }
}

/// The machine, shell and folder a command is meant for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    os: String,
    shell: String,
    cwd: String,
    session: Session,
    is_idle: bool,
}

impl Context {
    /// The operating system, such as `macos` or `linux`.
    pub fn os(&self) -> &str {
        &self.os
    }

    /// The shell, such as `zsh`.
    pub fn shell(&self) -> &str {
        &self.shell
    }

    /// The current folder of the shell.
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Where the session runs.
    pub fn session(&self) -> Session {
        self.session
    }

    /// Whether the shell is at a prompt with no command running.
    pub fn is_idle(&self) -> bool {
        self.is_idle
    }

    /// Create a context.
    ///
    /// ```
    /// use termite::{Context, Session};
    ///
    /// let context = Context::new("macos", "zsh", "/Users/me", Session::Local, true);
    /// assert_eq!(context.session().as_str(), "local");
    /// ```
    pub fn new(
        os: impl Into<String>,
        shell: impl Into<String>,
        cwd: impl Into<String>,
        session: Session,
        is_idle: bool,
    ) -> Self {
        Self {
            os: os.into(),
            shell: shell.into(),
            cwd: cwd.into(),
            session,
            is_idle,
        }
    }
}
