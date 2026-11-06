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
    /// Printed on standard output when the check finds nothing. Must never
    /// approve anything. Cursor reads no output as a stop, so it needs a word
    /// here, and its agent acts only on `deny`: its own approval rules run.
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
        no_objection: "{\"permission\":\"allow\"}",
        next_step: "Restart Cursor so it reads the hook.",
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
        match self.deny {
            Deny::HookSpecific => Some(json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }
            })),
            Deny::Decision(word) => {
                Some(json!({ "decision": word, "reason": reason }))
            },
            Deny::Permission => Some(json!({
                "permission": "deny",
                "user_message": reason,
                "agent_message": reason,
            })),
            Deny::PermissionDecision => Some(json!({
                "permissionDecision": "deny",
                "permissionDecisionReason": reason,
            })),
            Deny::ExitOnly => None,
        }
    }
}
