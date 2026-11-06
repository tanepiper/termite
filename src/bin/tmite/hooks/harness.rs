//! The agent harnesses termite can sit in front of: where each one keeps its
//! hooks, where its JSON holds the command, and how it is told to stop.
//! Contract: this is data. Adding a harness adds a row, not a code path.

use std::env;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// How long a hook may take, in seconds. The adapter stops the command itself
/// before this, so a harness that lets a timed-out hook through never gets
/// that far.
pub(super) const HOOK_TIMEOUT_SECS: u64 = 30;

/// How the hook is written down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Install {
    /// `hooks.EVENT[] = {matcher, hooks: [handler]}` in a shared JSON file.
    Grouped {
        event: &'static str,
        matcher: &'static str,
    },
    /// `hooks.EVENT[] = handler` in a shared JSON file.
    Flat { event: &'static str },
    /// `termite.EVENT[] = {matcher, hooks: [handler]}` in a shared JSON file,
    /// where the top-level keys name hooks.
    Named {
        event: &'static str,
        matcher: &'static str,
    },
    /// Files that belong to termite alone.
    OwnFiles,
    /// A marked block in a shared YAML file.
    YamlBlock,
}

/// How a harness is told no. Every harness also gets exit status 2 and the
/// reason on standard error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Deny {
    HookSpecific,
    Decision(&'static str),
    Permission,
    PermissionDecision,
    /// Only the exit status and standard error.
    ExitOnly,
}

/// How a harness is asked, and how to know this message came from the part of
/// it that was tried. A settings file can be shared with an app that wasn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ask {
    /// The word, in the same shape as a denial.
    word: &'static str,
    proof: &'static [Proof],
}

/// What a message must show. Anything else is a stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Proof {
    /// The field holds one of these.
    Is(&'static str, &'static [&'static str]),
}

impl Proof {
    fn holds(self, message: &Value) -> bool {
        let text = |at| message.pointer(at).and_then(Value::as_str);
        match self {
            Self::Is(at, words) => {
                text(at).is_some_and(|found| words.contains(&found))
            },
        }
    }
}

/// One agent harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Harness {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    /// Programs on `PATH` that show it is installed.
    pub(super) programs: &'static [&'static str],
    /// App bundles that show it is installed.
    pub(super) apps: &'static [&'static str],
    /// The variable that moves its settings folder, if it has one.
    pub(super) folder_variable: Option<&'static str>,
    /// Its settings folder, under the home folder.
    pub(super) folder: &'static str,
    /// The file the hook goes in, under the settings folder.
    pub(super) file: &'static str,
    pub(super) install: Install,
    /// JSON pointer to the command in what the harness sends.
    pub(super) command_at: &'static str,
    /// JSON pointer to the working folder in what the harness sends.
    pub(super) cwd_at: &'static str,
    pub(super) deny: Deny,
    /// How to hand a command to the person. Asking goes out with status 0, so
    /// a harness that ignored it would run the command. `None`, which stops
    /// the command, unless a live trial has shown that it prompts, that a no
    /// stops the command, and that it doesn't run when nobody can answer.
    pub(crate) ask: Option<Ask>,
    /// Printed on standard output when the check finds nothing. Must never
    /// approve anything. Cursor and Antigravity read no output as a stop, so
    /// they need a word here. Cursor's agent acts only on `deny`, and
    /// Antigravity's `allow` hands over to its normal rules; `auto_approve`
    /// is its word for approving, and is never used.
    pub(super) no_objection: &'static str,
    /// What the person must do after installing, if anything.
    pub(super) next_step: &'static str,
}

const TOOL_COMMAND: &str = "/tool_input/command";

pub(crate) const HARNESSES: &[Harness] = &[
    Harness {
        id: "claude",
        name: "Claude Code",
        programs: &["claude"],
        apps: &["/Applications/Claude.app"],
        folder_variable: Some("CLAUDE_CONFIG_DIR"),
        folder: ".claude",
        file: "settings.json",
        install: Install::Grouped {
            event: "PreToolUse",
            matcher: "Bash|Monitor",
        },
        command_at: TOOL_COMMAND,
        cwd_at: "/cwd",
        deny: Deny::HookSpecific,
        // VS Code can run this hook too, and sends no permission mode. Bypass
        // mode isn't listed, because it may not ask.
        ask: Some(Ask {
            word: "ask",
            proof: &[Proof::Is(
                "/permission_mode",
                &["default", "acceptEdits", "auto", "plan", "dontAsk"],
            )],
        }),
        no_objection: "",
        next_step: "Restart Claude Code so it reads the hook.",
    },
    Harness {
        id: "codex",
        name: "Codex",
        programs: &["codex"],
        apps: &["/Applications/Codex.app"],
        folder_variable: Some("CODEX_HOME"),
        folder: ".codex",
        file: "hooks.json",
        install: Install::Grouped {
            event: "PreToolUse",
            matcher: "^Bash$",
        },
        command_at: TOOL_COMMAND,
        cwd_at: "/cwd",
        deny: Deny::HookSpecific,
        // Never: it marks the run failed and the tool call continues.
        ask: None,
        no_objection: "",
        next_step: "Codex skips a new hook until you trust it: run /hooks in \
                    Codex and trust this one.",
    },
    Harness {
        id: "gemini",
        name: "Gemini CLI",
        programs: &["gemini"],
        apps: &[],
        folder_variable: None,
        folder: ".gemini",
        file: "settings.json",
        install: Install::Grouped {
            event: "BeforeTool",
            matcher: "^run_shell_command$",
        },
        command_at: TOOL_COMMAND,
        cwd_at: "/cwd",
        deny: Deny::Decision("deny"),
        ask: None,
        no_objection: "",
        next_step: "",
    },
    Harness {
        id: "cursor",
        name: "Cursor",
        programs: &["cursor-agent", "cursor"],
        apps: &["/Applications/Cursor.app"],
        folder_variable: None,
        folder: ".cursor",
        file: "hooks.json",
        install: Install::Flat {
            event: "beforeShellExecution",
        },
        command_at: "/command",
        cwd_at: "/cwd",
        deny: Deny::Permission,
        // Never: in a trial its agent ran the command it was told to ask about.
        ask: None,
        no_objection: "{\"permission\":\"allow\"}",
        next_step: "Restart Cursor so it reads the hook.",
    },
    Harness {
        id: "antigravity",
        name: "Antigravity",
        programs: &["agy", "antigravity", "agy-ide", "antigravity-ide"],
        apps: &[
            "/Applications/Antigravity.app",
            "/Applications/Antigravity IDE.app",
        ],
        folder_variable: None,
        folder: ".gemini/config",
        file: "hooks.json",
        install: Install::Named {
            event: "PreToolUse",
            matcher: "^run_command$",
        },
        command_at: "/toolCall/args/CommandLine",
        cwd_at: "/toolCall/args/Cwd",
        deny: Deny::Decision("deny"),
        // Not yet: nobody has seen force_ask hold against the flag that approves
        // everything, and the message doesn't show that flag.
        ask: None,
        no_objection: "{\"decision\":\"allow\"}",
        next_step: "Restart Antigravity so it reads the hook.",
    },
    Harness {
        id: "copilot",
        name: "GitHub Copilot CLI",
        programs: &["copilot"],
        apps: &[],
        folder_variable: Some("COPILOT_HOME"),
        folder: ".copilot",
        file: "hooks/termite.json",
        install: Install::OwnFiles,
        command_at: TOOL_COMMAND,
        cwd_at: "/cwd",
        deny: Deny::PermissionDecision,
        // Never: VS Code reads this file, runs the command in Allow all without
        // a prompt, and sends the same message as the CLI, so nothing here
        // can tell the two apart.
        ask: None,
        no_objection: "",
        next_step: "",
    },
    Harness {
        id: "hermes",
        name: "Hermes Agent",
        programs: &["hermes"],
        apps: &[],
        folder_variable: Some("HERMES_HOME"),
        folder: ".hermes",
        file: "config.yaml",
        install: Install::YamlBlock,
        command_at: TOOL_COMMAND,
        cwd_at: "/cwd",
        deny: Deny::Decision("block"),
        ask: None,
        no_objection: "{}",
        next_step: "Hermes skips a new hook until you approve it: answer yes \
                    when it asks, then run `hermes hooks doctor`.",
    },
    Harness {
        id: "goose",
        name: "goose",
        programs: &["goose"],
        apps: &["/Applications/Goose.app"],
        folder_variable: None,
        folder: ".agents/plugins/termite",
        file: "hooks/hooks.json",
        install: Install::OwnFiles,
        command_at: TOOL_COMMAND,
        cwd_at: "/working_dir",
        deny: Deny::Decision("block"),
        ask: None,
        no_objection: "",
        next_step: "",
    },
    Harness {
        id: "opencode",
        name: "OpenCode",
        programs: &["opencode"],
        apps: &[],
        folder_variable: None,
        folder: ".config/opencode",
        file: "plugins/termite.js",
        install: Install::OwnFiles,
        command_at: TOOL_COMMAND,
        cwd_at: "/cwd",
        deny: Deny::ExitOnly,
        ask: None,
        no_objection: "",
        next_step: "Restart OpenCode so it loads the plugin.",
    },
    Harness {
        id: "windsurf",
        name: "Windsurf",
        programs: &["windsurf"],
        apps: &["/Applications/Windsurf.app"],
        folder_variable: None,
        folder: ".codeium/windsurf",
        file: "hooks.json",
        install: Install::Flat {
            event: "pre_run_command",
        },
        command_at: "/tool_info/command_line",
        cwd_at: "/tool_info/cwd",
        deny: Deny::ExitOnly,
        ask: None,
        no_objection: "",
        next_step: "Restart Windsurf so it reads the hook.",
    },
];

impl Harness {
    pub(crate) fn find(id: &str) -> Option<&'static Harness> {
        HARNESSES.iter().find(|harness| harness.id == id)
    }

    /// The settings folder, under `home` unless a variable moves it.
    pub(super) fn folder_in(&self, home: &Path) -> PathBuf {
        self.folder_variable
            .and_then(env::var_os)
            .filter(|folder| !folder.is_empty())
            .map(PathBuf::from)
            .filter(|folder| folder.is_absolute())
            .unwrap_or_else(|| home.join(self.folder))
    }

    /// The file the hook goes in.
    pub(super) fn path_in(&self, home: &Path) -> PathBuf {
        self.folder_in(home).join(self.file)
    }

    /// What this harness reads when a command must stop. `None` when it needs
    /// only the exit status.
    pub(super) fn denial(&self, reason: &str) -> Option<Value> {
        let word = match self.deny {
            Deny::Decision(word) => word,
            _ => "deny",
        };
        self.told(word, reason)
    }

    /// What hands a command to the person. `None` when this harness can't be
    /// trusted to ask.
    pub(super) fn asking(&self, reason: &str) -> Option<Value> {
        self.ask.and_then(|ask| self.told(ask.word, reason))
    }

    /// Whether this message came from a part of the harness proven to ask.
    pub(super) fn asks_in(&self, message: &Value) -> bool {
        self.ask
            .is_some_and(|ask| ask.proof.iter().all(|one| one.holds(message)))
    }

    fn told(&self, word: &str, reason: &str) -> Option<Value> {
        match self.deny {
            Deny::HookSpecific => Some(json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": word,
                    "permissionDecisionReason": reason,
                }
            })),
            Deny::Decision(_) => {
                Some(json!({ "decision": word, "reason": reason }))
            },
            Deny::Permission => Some(json!({
                "permission": word,
                "user_message": reason,
                "agent_message": reason,
            })),
            Deny::PermissionDecision => Some(json!({
                "permissionDecision": word,
                "permissionDecisionReason": reason,
            })),
            Deny::ExitOnly => None,
        }
    }
}
