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

/// Who asked. The model never decides this: it comes from the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    /// Not stated, as in the evaluation. Gets the plain prompt.
    Unstated,
    /// A person at a terminal, who reads every command.
    Person,
    /// An agent, CI or anything else with nobody there to answer.
    Agent,
}

impl Caller {
    /// The word used in the alert log.
    pub fn as_str(self) -> &'static str {
        match self {
            Caller::Unstated => "tmite",
            Caller::Person => "person",
            Caller::Agent => "agent",
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
    caller: Caller,
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

    /// Who asked.
    pub fn caller(&self) -> Caller {
        self.caller
    }

    /// Nobody is there to answer a question: an agent, CI or no terminal.
    pub fn unattended(&self) -> bool {
        self.caller == Caller::Agent
    }

    /// Say who asked.
    #[must_use]
    pub fn with_caller(mut self, caller: Caller) -> Self {
        self.caller = caller;
        self
    }

    /// Create a context.
    ///
    /// ```
    /// use termite::{Context, Session};
    ///
    /// let context = Context::new("macos", "zsh", "/Users/me", Session::Local);
    /// assert_eq!(context.session().as_str(), "local");
    /// ```
    pub fn new(
        os: impl Into<String>,
        shell: impl Into<String>,
        cwd: impl Into<String>,
        session: Session,
    ) -> Self {
        Self {
            os: os.into(),
            shell: shell.into(),
            cwd: cwd.into(),
            session,
            caller: Caller::Unstated,
        }
    }
}
