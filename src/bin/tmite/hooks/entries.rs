//! The text each harness gets: its hook entry, and the files that belong to
//! termite alone. Contract: only builds text; `plan` decides where it goes.

use std::path::PathBuf;

use serde_json::json;

use super::harness::{HOOK_TIMEOUT_SECS, Harness};
use super::json::Json;
use super::plan::Place;

/// The command as a shell line. `|| exit 2` stops the command when `tmite` is
/// missing or crashes, in harnesses that let other exit statuses through.
pub(super) fn shell_line(harness: &Harness, place: &Place) -> String {
    let program = place.program.to_string_lossy();
    format!(
        "{} hook {} || exit 2",
        shell_words::quote(&program),
        harness.id
    )
}

/// One hook handler, in the words each harness reads. Each asks the harness
/// to stop the command if the hook fails, where it has a way to.
pub(super) fn handler(harness: &Harness, place: &Place) -> Json {
    let command = shell_line(harness, place);
    let timeout = HOOK_TIMEOUT_SECS;
    Json::from_value(match harness.id {
        "claude" => json!({
            "type": "command",
            "command": command,
            "timeout": timeout,
            "onFailure": "block",
        }),
        "gemini" => json!({
            "type": "command",
            "name": "termite",
            "command": command,
            "timeout": timeout * 1000,
        }),
        "cursor" => json!({
            "command": command,
            "timeout": timeout,
            "failClosed": true,
        }),
        "windsurf" => json!({ "command": command, "show_output": true }),
        _ => json!({
            "type": "command",
            "command": command,
            "timeout": timeout,
        }),
    })
}

/// The Hermes block. Hermes runs this without a shell, so failure is handled
/// by `fail_closed`, not `|| exit 2`.
pub(super) fn hermes_block(harness: &Harness, place: &Place) -> String {
    let program = place.program.to_string_lossy();
    format!(
        "hooks:\n  pre_tool_call:\n    - matcher: \"^terminal$\"\n      \
         command: \"{} hook {}\"\n      timeout: {HOOK_TIMEOUT_SECS}\n      \
         fail_closed: true\n",
        shell_words::quote(&program).replace('"', "\\\""),
        harness.id,
    )
}

const OPENCODE_PLUGIN: &str = r#"// Written by `tmite hooks install opencode`. Remove it with `tmite hooks remove opencode`.
import { spawnSync } from "node:child_process";

const TMITE = __TMITE__;

export const Termite = async ({ directory }) => ({
  "tool.execute.before": async (input, output) => {
    if (input.tool !== "bash") return;
    const message = JSON.stringify({
      tool_input: { command: output.args.command },
      cwd: directory,
    });
    const run = spawnSync(TMITE, ["hook", "opencode"], {
      input: message,
      encoding: "utf8",
      timeout: __TIMEOUT__,
    });
    if (run.status !== 0) {
      throw new Error(run.stderr || "termite stopped this command");
    }
  },
});
"#;

/// The files that belong to termite alone, with their text.
pub(super) fn own_files(
    harness: &Harness,
    place: &Place,
) -> Vec<(PathBuf, String)> {
    let path = harness.path_in(&place.home);
    let group = |matcher: &str, action: serde_json::Value| json!({ "matcher": matcher, "hooks": [action] });
    match harness.id {
        "copilot" => {
            let hook = group(
                "Bash",
                json!({
                    "type": "command",
                    "bash": shell_line(harness, place),
                    "timeoutSec": HOOK_TIMEOUT_SECS,
                }),
            );
            let text =
                json!({ "version": 1, "hooks": { "PreToolUse": [hook] } });
            vec![(path, Json::from_value(text).pretty())]
        },
        "goose" => {
            let hook = group(
                "^shell$",
                json!({
                    "type": "command",
                    "command": shell_line(harness, place),
                    "timeout": HOOK_TIMEOUT_SECS,
                    "on_failure": "block",
                }),
            );
            let hooks = json!({ "hooks": { "PreToolUse": [hook] } });
            let manifest = json!({
                "name": "termite",
                "version": env!("CARGO_PKG_VERSION"),
                "description": "Check each shell command with tmite first",
            });
            vec![
                (
                    harness.folder_in(&place.home).join("plugin.json"),
                    Json::from_value(manifest).pretty(),
                ),
                (path, Json::from_value(hooks).pretty()),
            ]
        },
        _ => {
            let program =
                serde_json::to_string(&place.program.to_string_lossy())
                    .unwrap_or_default();
            let text = OPENCODE_PLUGIN.replace("__TMITE__", &program).replace(
                "__TIMEOUT__",
                &(HOOK_TIMEOUT_SECS * 1000).to_string(),
            );
            vec![(path, text)]
        },
    }
}
